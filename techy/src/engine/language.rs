//! [`Language<L>`]: the long-lived runtime bundle and the `parse()` convenience entry.
//!
//! A `Language` is everything a parse needs that outlives any one parse: the frozen
//! initial [`ParsingState`] and the [`ParseDriver`](crate::engine::ParseDriver)
//! instance. It contributes at exactly one moment — seeding — and owns **no per-parse
//! state**: define a language once, parse many documents in it. Per-parse
//! accumulation lives on the transient [`ParserSession`]; results are frozen
//! [`ParseResult`]s owning their tree and diagnostics (no borrow of the `Language` —
//! nodes are self-contained).

use core::fmt;

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::constructs::{
    ChildStateSpec, FromInvocation, ImplementationError, ParseContext, StopCause, StopSpec, StrayGroupClose,
};
use crate::error::{Diagnostics, ParseError};
use crate::node::NodeKind;
use super::descent_guard::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use super::driver::ParseDriver;
use crate::source::{Source, SourceSpan};
use crate::state::{FeaturePresence, Lang, LangFeatures, ParsingState};

use super::{ParseResult, ParserSession};

/// The runtime bundle of a language: initial state and driver — long-lived, shareable
/// (`Send + Sync` through its parts), owning no per-parse state.
///
/// [`new`](Language::new) asks for the real inputs — the driver (with its explicit
/// recovery policy) and the initial state — kept cheap by the two seed constructors:
/// [`ParsingState::lang_initial()`] is the `Lang`'s canonical seed, and
/// [`ParsingState::lang_initial_with_packages`] is the everyday "seed plus these
/// packages" form. Any further customization derives
/// *before* construction, through the single derivation point:
/// `Language::new(driver, ParsingState::lang_initial()?.derived(&delta)?)` — so
/// [`Lang::finalize_transition`](crate::state::Lang::finalize_transition) holds its
/// invariants over every customized seed.
///
/// ```
/// # use techy::core::{Language, ParsingState, StdParseDriver, TrivialLang};
/// # use techy::error::Recovery;
/// # #[derive(Debug, Clone, Copy)]
/// # struct MyLang;
/// # impl TrivialLang for MyLang {}
/// let language: Language<MyLang> = Language::new(
///     StdParseDriver::new(Recovery::Tolerant, ()),
///     ParsingState::lang_initial().expect("seed state"),
/// );
/// let result = language.parse("hello").unwrap();
/// assert_eq!(result.tree.root().chars(), None); // the root is a List
/// ```
///
/// **The advanced path** (driving construct parsers directly under this language's
/// defaults) composes from the accessors — a [`ParserSession`] is `Language`-independent
/// scratch, created directly:
///
/// ```ignore
/// let mut session = ParserSession::new();
/// let mut reader = language.driver().make_token_reader(&source);
/// let mut cx = ParseContext::new(
///     &mut *reader,
///     Arc::clone(language.initial_state()),
///     &mut session,
///     language.driver(),
/// );
/// ```
pub struct Language<L: Lang> {
    /// The language's parse-behavior instance ([`Lang::Driver`]).
    driver: L::Driver,
    /// The frozen initial state every parse starts from — shared by `Arc` across
    /// parses (states are immutable).
    initial_state: Arc<ParsingState<L>>,
    /// The configuration for the per-parse [`StdDescentGuard`] (the parsing-depth
    /// limiter): defaulted in [`new`](Language::new), set with
    /// [`with_descent_guard_init`](Language::with_descent_guard_init), consumed by
    /// [`parse_source`](Language::parse_source) to create each parse's guard.
    descent_guard_init: StdDescentGuardInit,
}

impl<L: Lang> Language<L> {
    /// A language over `driver`, parsing from `initial_state`. Both inputs are
    /// mandatory — the type-level docs show the canonical construction, and the
    /// [`ParsingState::lang_initial`]`[_with_packages]` constructors keep the everyday
    /// spellings short. `initial_state` accepts a state by value or an already-shared
    /// `Arc<ParsingState<L>>`: passing the shared handle preserves the state's
    /// identity (states are shared by handle — a data-equal copy is a different
    /// state), so a language can start parses from exactly the state some parsed
    /// node carries. The parsing-depth limit starts at the guard's default
    /// configuration; choose one explicitly with
    /// [`with_descent_guard_init`](Language::with_descent_guard_init).
    pub fn new(
        driver: L::Driver,
        initial_state: impl Into<Arc<ParsingState<L>>>,
    ) -> Language<L> {
        Language {
            driver,
            initial_state: initial_state.into(),
            descent_guard_init: Default::default(),
        }
    }

    /// Use `init` as the configuration of the per-parse [`StdDescentGuard`] — the
    /// parsing-depth limiter that refuses input nested too deeply instead of
    /// letting it crash the process by stack exhaustion. See
    /// [`StdDescentGuardInit`]'s docs for
    /// choosing among a byte budget, a depth limit, and off. Without this call,
    /// parses run under the guard's default configuration:
    /// a deliberately tight built-in stack budget that also emits a one-time
    /// warning diagnostic at half use, so that an untuned deep parse fails early,
    /// pointing at this method, instead of consuming an unknown amount of stack.
    pub fn with_descent_guard_init(mut self, init: StdDescentGuardInit) -> Language<L> {
        self.descent_guard_init = init;
        self
    }

    /// The frozen initial state every parse starts from.
    pub fn initial_state(&self) -> &Arc<ParsingState<L>> {
        &self.initial_state
    }

    /// The language's [`ParseDriver`](crate::engine::ParseDriver) instance —
    /// concretely typed, so preset helper methods (and driver-configured capabilities
    /// like [`source_resolver`](crate::engine::ParseDriver::source_resolver)) are
    /// directly reachable.
    pub fn driver(&self) -> &L::Driver {
        &self.driver
    }

    /// Parse `content` as an anonymous in-memory [`Source`]. For a pre-minted source
    /// carrying origin or provenance (a file, a
    /// [`resolve_source_reference`](crate::source::resolve_source_reference) result),
    /// use [`parse_source`](Language::parse_source).
    ///
    /// Each call mints a **fresh source identity** ([`Source::new`]), and
    /// [`SourceSpan`]/[`SourcePos`](crate::source::SourcePos) equality compares the
    /// source by identity plus offsets — so spans from two `parse` calls never
    /// compare equal, even over byte-identical text (the comparison answers
    /// `false`, it does not fail). To correlate positions across parses
    /// (re-parsing an edited document, diffing two attempts), hold one
    /// `Arc<Source>` and call [`parse_source`](Language::parse_source) with the
    /// same handle each time.
    pub fn parse(
        &self,
        content: impl Into<String>,
    ) -> Result<ParseResult<L>, ParseError<L::SourceOrigin>>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        self.parse_source(Arc::new(Source::new(content)))
    }

    /// Parse `source` under this language's defaults: tokenize with the seed state's
    /// rules, drive the root content loop (through the driver's construct provision,
    /// like every descent), stage the root `List` over the whole source, and freeze
    /// the session into a [`ParseResult`].
    ///
    /// **Recovery** follows the driver's policy. A stray group close at the root —
    /// nobody's to claim — is diagnosed as [`StrayGroupClose`] through the recovery
    /// entry point; tolerant parses consume the delimiter, stage it as a `Chars` node
    /// (the markup-in-chars recovery artifact: the root span partition holds across the
    /// skip, so recovered parse trees keep the parse-tree byte accounting), and
    /// resume; strict parses abort. Diagnosis and resume both run under the state the content loop had
    /// reached at the close (the segment's exit state,
    /// [`NodesOutcome::state`](crate::constructs::NodesOutcome::state)): the reported
    /// delimiter is the one the loop's tokenization matched, and sibling-level state
    /// changes from before the skip — a `\newcommand`-style definition, a group-rule
    /// change — stay in effect across it, exactly as if the close had not been there.
    ///
    /// `Err` is the strict-mode abort (or an implementation-contract violation, which
    /// aborts under any policy); `Ok` carries the tree plus any tolerantly recorded
    /// diagnostics.
    pub fn parse_source(
        &self,
        source: Arc<Source<L::SourceOrigin>>,
    ) -> Result<ParseResult<L>, ParseError<L::SourceOrigin>>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        let mut reader = self.driver.make_token_reader(&source);
        let mut session = ParserSession::new();
        // Seed the diagnostics sink's retention cap from the driver
        // (`ParseDriver::diagnostics_limit`); `None` keeps the default cap.
        if let Some(limit) = self.driver.diagnostics_limit() {
            session.diagnostics = Diagnostics::with_limit(limit);
        }
        // The descent guard is created eagerly, here at true parse entry on the
        // parsing thread — a stack-measuring guard anchors its reference
        // measurement before any descent runs.
        session.install_descent_guard(StdDescentGuard::init(&self.descent_guard_init));
        let mut nodes = Vec::new();
        let seed = Arc::clone(&self.initial_state);
        // Parse-initialization observation (registration-sanity diagnostics): once
        // per root parse, before any token is read.
        self.driver.observe_parse_start(&source, &seed, &mut session.diagnostics);
        let mut cx = ParseContext::new(
            &mut *reader,
            Arc::clone(&seed),
            &mut session,
            &self.driver);
        loop {
            // The root descent routes through the driver's factory like every other
            // descent site (Phase 7.2 uniform-routing contract). A pass-through delta
            // has no applicable target at the root and is discarded.
            let (outcome, _delta) = cx.parse_nodes(
                Arc::clone(&cx.state),
                StopSpec::none(),
                ChildStateSpec::inherit(),
            )?;
            nodes.extend(outcome.nodes);
            // Thread the segment's exit state: the root context's ambient state
            // advances with the content, so the recover funnel below and any resume
            // run under the state the loop actually reached — resuming from the seed
            // would roll back sibling after-effects (`\newcommand` definitions) across
            // a tolerant skip.
            cx.state = outcome.state;
            match outcome.stop {
                StopCause::EndOfInput => break,
                StopCause::UnexpectedGroupClose { span, after } => {
                    // Impossible under a language that declares groups absent: a
                    // group close cannot be tokenized, so reaching this arm means the
                    // token source violated its contract (`TokenReader` docs) — an
                    // implementation bug aborts under any policy, never a panic.
                    if !<L::Features as LangFeatures>::Groups::PRESENT {
                        return Err(cx.implementation_error(
                            "a stray group close surfaced at the root although the \
                             language declares the groups feature absent \
                             (token-source contract violation)",
                            span,
                        ));
                    }
                    // Diagnose-and-skip at the root (DESIGN_RATIONALE.md [§dd-dr:errors]): the
                    // loop left the close unconsumed at `span.start`, and the span is
                    // the delimiter exactly as matched (`StopCause`'s contract) —
                    // sliced, not re-peeked: a re-read under any state but the loop's
                    // own could tokenize different bytes.
                    let delim = span.content().to_string();
                    cx.recover(StrayGroupClose { delim }, span.clone())?;
                    cx.tokens.move_to_position(&after);
                    // Stage the consumed delimiter as a chars node (the
                    // markup-in-chars recovery artifact; 7.9): the root partition
                    // stays exact across the skip.
                    let id = cx.stage_node(
                            NodeKind::chars(span.span()),
                            span.clone(),
                            Arc::clone(&cx.state),
                            Vec::new(),
                        )
                        .map_err(|error| cx.staging_error(error, span))?;
                    nodes.push(id);
                }
                StopCause::TokenCondition { span, .. } => {
                    return Err(cx.implementation_error(
                        "the root content loop stopped on a token condition none was \
                         set (nodes-parser contract violation)",
                        span,
                    ));
                }
                StopCause::NodeCondition => {
                    return Err(cx.implementation_error(
                        "the root content loop stopped on a node condition none was \
                         set (nodes-parser contract violation)",
                        cx.here(),
                    ));
                }
            }
        }
        let root = cx
            .stage_node(NodeKind::list(), SourceSpan::entire(&source), seed, nodes)
            .map_err(|error| {
                let at = SourceSpan::at(&SourceSpan::entire(&source).start_pos());
                cx.staging_error(error, at)
            })?;
        session.finish(root).map_err(|error| {
            ParseError::new(
                ImplementationError::new(error.to_string()),
                SourceSpan::entire(&source),
            )
        })
    }
}

// Manual Debug: a derive would demand `L: Debug` although only associated types
// (already bounded) are stored. There is deliberately no `Default` impl: it would
// reintroduce an implicit seed by the back door, and the driver's recovery policy
// must be an explicit choice.
impl<L: Lang> fmt::Debug for Language<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Language")
            .field("driver", &self.driver)
            .field("initial_state", &self.initial_state)
            // The descent-guard configuration carries no `Debug` bound: shown by
            // omission.
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{ParseDriver, StdParseDriver};
    use crate::error::{DiagnosticInfo, Recovery};
    use crate::node::check_tree_invariants;
    use crate::scopes::{Package, ScopeOp, ScopeStack};
    use crate::source::{MapResolver, SourceProvenance};
    use crate::state::{GroupOverrides, ParsingStateDelta, StateData, TokenRulesOverrides};
    use crate::token::{
        CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
        GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::vec;

    /// The latex-ish seed rules the `DocLang`-family test languages share (groups
    /// `{`/`}` and `%` comments enabled) — generic so a sibling lang differing only
    /// in its driver does not re-derive the block.
    fn doc_state_data<
        L: Lang<
            GroupTypeId = u32,
            ModeId = (),
            StateExt = (),
            Features = crate::state::AllLangFeatures,
        >,
    >() -> StateData<L> {
        StateData {
            rules: TokenRules {
                whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
                paragraphs: ParagraphRules { enabled: true },
                groups: GroupRules {
                    enabled: true,
                    rules: vec![Arc::new(GroupRule {
                        group_type: 0,
                        open: "{".into(),
                        close: "}".into(),
                    })],
                    temporary: Vec::new(),
                    expecting_close: None,
                },
                commands: CommandRules {
                    enabled: false,
                    rules: Vec::new(),
                },
                comments: CommentRules {
                    enabled: true,
                    rules: vec![Arc::new(CommentRule { start: "%".into() })],
                },
                specials: SpecialsRules { enabled: false },
                forbidden_chars: ForbiddenCharsRules { chars: "".into() },
            },
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }
    }

    /// A language whose canonical seed enables the latex-ish syntax the tests use —
    /// exercising the "Language seeds from the Lang hook" path.
    #[derive(Debug, Clone, Copy)]
    struct DocLang;
    impl Lang for DocLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = StdParseDriver;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(doc_state_data())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    fn strict() -> Language<DocLang> {
        Language::new(
            StdParseDriver::new(Recovery::Strict, ()),
            ParsingState::lang_initial().expect("seed state"),
        )
    }

    fn tolerant() -> Language<DocLang> {
        Language::new(
            StdParseDriver::new(Recovery::Tolerant, ()),
            ParsingState::lang_initial().expect("seed state"),
        )
    }

    /// The staged child shapes of a result's root list, as compact strings.
    fn shapes(result: &ParseResult<DocLang>) -> Vec<String> {
        result
            .tree
            .root()
            .children()
            .iter().map(|child| match child.chars() {
                Some(text) => alloc::format!("chars({})", text),
                None if child.group().is_some() => "group".into(),
                None => "other".into(),
            })
            .collect()
    }

    #[test]
    fn new_accepts_a_shared_state_handle_preserving_identity() {
        // A parsed node's state is a shared handle; seeding a new language from that
        // handle preserves the state's identity (no data-equal copy is minted).
        let result = strict().parse("hello").unwrap();
        let node_state = Arc::clone(result.tree.root().parsing_state());

        let language =
            Language::new(StdParseDriver::new(Recovery::Strict, ()), Arc::clone(&node_state));
        assert!(Arc::ptr_eq(language.initial_state(), &node_state));
    }

    #[test]
    fn parse_drives_reader_to_tree_end_to_end() {
        let result = strict().parse("hello {world}").unwrap();
        check_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        assert_eq!(shapes(&result), ["chars(hello )", "group"]);
        // The root List spans the entire source.
        assert_eq!(result.tree.root().span().range(), 0..13);
    }

    #[test]
    fn parse_of_empty_content_is_an_empty_root_list() {
        let result = strict().parse("").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.tree.root().children().iter().count(), 0);
        assert_eq!(result.tree.root().span().range(), 0..0);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn define_once_parse_many_shares_the_seed_state() {
        let language = strict();
        let first = language.parse("a").unwrap();
        let second = language.parse("b{c}").unwrap();
        // Every parse starts from the same frozen seed Arc — the Language contributes
        // at exactly one moment and accumulates nothing.
        for result in [&first, &second] {
            assert!(Arc::ptr_eq(
                result.tree.root().parsing_state(),
                language.initial_state()
            ));
        }
        assert_eq!(shapes(&first), ["chars(a)"]);
        assert_eq!(shapes(&second), ["chars(b)", "group"]);
    }

    #[test]
    fn stray_close_aborts_strict_and_recovers_as_chars_tolerantly() {
        // Strict: the root drive aborts with the core condition.
        let err = strict().parse("a}b").unwrap_err();
        assert_eq!(err.identifier(), StrayGroupClose::IDENTIFIER);
        assert_eq!(err.span().range(), 1..2);
        assert_eq!(err.to_string(), "unexpected closing ‘}’ — no group is open");

        // Tolerant: diagnose, consume, and stage the delimiter as a chars node (the
        // markup-in-chars recovery artifact — revised in 7.9, superseding 7.4's
        // byte-dropping quirk: the root's span tiling holds across the skip).
        let result = tolerant().parse("a}b").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(a)", "chars(})", "chars(b)"]);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        assert_eq!(diagnostic.message(), "unexpected closing ‘}’ — no group is open");
    }

    #[test]
    fn consecutive_stray_closes_each_report_and_resume() {
        let result = tolerant().parse("}}x").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(})", "chars(})", "chars(x)"]);
        assert_eq!(result.diagnostics.len(), 2);
    }

    // --- the driver's diagnostics retention cap -----------------------------------------

    /// `DocLang` under a driver that caps diagnostics retention — the
    /// `ParseDriver::diagnostics_limit` seeding path.
    #[derive(Debug, Clone, Copy)]
    struct CapLang;
    impl Lang for CapLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = CapDriver;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(doc_state_data())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    /// Tolerant driver retaining at most two diagnostics per parse.
    #[derive(Debug, Clone, Copy)]
    struct CapDriver;

    impl ParseDriver<CapLang> for CapDriver {
        fn recovery(&self) -> Recovery {
            Recovery::Tolerant
        }
        fn diagnostics_limit(&self) -> Option<usize> {
            Some(2)
        }
    }

    // --- the session extension returned on the result ------------------------------------

    /// Transition counts accumulated by `observe_transition` — read back off the
    /// [`ParseResult`].
    #[derive(Debug, Default)]
    struct Observed {
        transitions: usize,
    }

    /// `DocLang` under a driver whose `observe_transition` accumulates into the
    /// session extension — the `ParseResult::session_ext` read-back path.
    #[derive(Debug, Clone, Copy)]
    struct ObserveLang;
    impl Lang for ObserveLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = Observed;
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = ObserveDriver;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(doc_state_data())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct ObserveDriver;

    impl ParseDriver<ObserveLang> for ObserveDriver {
        fn observe_transition(
            &self,
            ext: &mut Observed,
            _diagnostics: &mut crate::error::Diagnostics,
            _prev: &ParsingState<ObserveLang>,
            _new: &ParsingState<ObserveLang>,
            _delta: &ParsingStateDelta<ObserveLang>,
        ) -> Result<(), ParseError> {
            ext.transitions += 1;
            Ok(())
        }
    }

    #[test]
    fn the_session_extension_is_returned_on_the_parse_result() {
        // Two group descents = two observed transitions (sibling groups memoize the
        // derivation, but observation fires per transition, memo hits included):
        // `observe_transition` accumulates them into the session extension, and the
        // finished result hands the value out.
        let language: Language<ObserveLang> =
            Language::new(ObserveDriver, ParsingState::lang_initial().expect("seed state"));
        let result = language.parse("{a}{b}").unwrap();
        assert_eq!(result.session_ext.transitions, 2);

        // A language declaring no session extension reads back `()`.
        let result = strict().parse("{a}").unwrap();
        let () = result.session_ext;
    }

    #[test]
    fn a_driver_diagnostics_limit_caps_the_parses_diagnostics() {
        // Four stray closes under the tolerant policy: four pushes, two retained,
        // two counted as suppressed (`Diagnostics::with_limit`'s cap semantics).
        let language: Language<CapLang> =
            Language::new(CapDriver, ParsingState::lang_initial().expect("seed state"));
        let result = language.parse("}}}}").unwrap();
        assert_eq!(result.diagnostics.limit(), 2);
        assert_eq!(result.diagnostics.len(), 2);
        assert_eq!(result.diagnostics.suppressed(), 2);
        assert!(result.diagnostics.has_errors());

        // The `None` default keeps the standard cap.
        let result = tolerant().parse("}}").unwrap();
        assert_eq!(
            result.diagnostics.limit(),
            crate::error::Diagnostics::<Option<String>>::DEFAULT_LIMIT
        );
        assert_eq!(result.diagnostics.suppressed(), 0);
    }

    #[test]
    fn a_derived_seed_customizes_through_the_choke_point() {
        // Disabling comments through the delta idiom — the seed derives *before*
        // construction: `Language::new(driver, lang_initial()?.derived(&delta)?)`.
        let seed = ParsingState::<DocLang>::lang_initial().expect("seed state")
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                comments: crate::state::CommentOverrides::disable(),
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        let language = Language::new(StdParseDriver::new(Recovery::Strict, ()), seed);
        assert!(!language.initial_state().rules().comments_enabled());
        let result = language.parse("a%b").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(a%b)"]);
    }

    #[test]
    fn the_delta_idiom_surfaces_scope_op_failures_before_construction() {
        // A failing scope op aborts seed derivation — no `Language` is ever built.
        let error = ParsingState::<DocLang>::lang_initial().expect("seed state")
            .derived(
                &ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "absent".into() }),
            )
            .unwrap_err();
        assert_eq!(error.failures.len(), 1);
    }

    #[test]
    fn resolver_round_trip_parses_a_resolved_source() {
        // The source resolver lives on the driver ([§dd-dr:input-wiring]): configure
        // it there, reach it through the `ParseDriver::source_resolver` accessor, and
        // compose with the free `resolve_source_reference`.
        use crate::source::resolve_source_reference;

        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter {content}");
        let language: Language<DocLang> = Language::new(
            StdParseDriver::new(Recovery::Strict, ()).with_source_resolver(resolver),
            ParsingState::lang_initial().expect("seed state"),
        );

        let main = language.parse(r"\input{chapter.tex}").unwrap();
        let trigger = main.tree.root().span().clone();
        let driver_resolver =
            ParseDriver::<DocLang>::source_resolver(language.driver()).unwrap();
        let resolved =
            resolve_source_reference(driver_resolver, "chapter.tex", &trigger).unwrap();
        match resolved.provenance() {
            SourceProvenance::Resolved { reference, triggered_at } => {
                assert_eq!(reference, "chapter.tex");
                assert_eq!(triggered_at, &trigger);
            }
            other => panic!("expected Resolved provenance, got {:?}", other),
        }

        let result = language.parse_source(Arc::clone(&resolved)).unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(chapter )", "group"]);
        // The tree's spans reference the resolved source (provenance intact).
        assert!(Arc::ptr_eq(result.tree.root().span().source(), &resolved));
    }

    #[test]
    fn an_unconfigured_driver_resolves_no_sources() {
        let language = strict();
        assert!(ParseDriver::<DocLang>::source_resolver(language.driver()).is_none());
    }

    /// A driver whose nodes-parser factory violates the output contract by stopping on
    /// a token condition none was set — the root drive aborts with an implementation
    /// error under *any* recovery policy.
    #[test]
    fn a_contract_violating_root_stop_is_an_implementation_error() {
        use crate::constructs::{
            ConstructParser, ConstructParserResult, ImplementationError, NodesOutcome,
        };

        #[derive(Debug, Clone, Copy)]
        struct BogusLang;
        impl Lang for BogusLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type Tokenization = crate::token::StdTokenization;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = BogusDriver;
            fn make_node_ext(
                _kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) -> Result<(), crate::node::NodeBuildError> {
                Ok(())
            }
        }

        #[derive(Debug, Clone, Copy)]
        struct BogusDriver;

        struct BogusParser;
        impl ConstructParser<BogusLang> for BogusParser {
            type Output = NodesOutcome<BogusLang>;
            fn parse(
                &mut self,
                cx: &mut ParseContext<'_, '_, BogusLang>,
            ) -> ConstructParserResult<
                BogusLang,
                (Self::Output, Option<alloc::boxed::Box<ParsingStateDelta<BogusLang>>>),
            > {
                Ok((
                    NodesOutcome {
                        nodes: Vec::new(),
                        stop: StopCause::TokenCondition {
                            span: cx.here(),
                            after: cx.tokens.position_here(),
                        },
                        state: Arc::clone(&cx.state),
                        after_effects: None,
                    },
                    None,
                ))
            }
        }

        impl ParseDriver<BogusLang> for BogusDriver {
            fn recovery(&self) -> Recovery {
                Recovery::Tolerant
            }
            fn make_nodes_parser<'p>(
                &'p self,
                _stop: StopSpec<'p, BogusLang>,
                _child_states: ChildStateSpec<'p, BogusLang>,
            ) -> Result<
                alloc::boxed::Box<
                    dyn ConstructParser<BogusLang, Output = NodesOutcome<BogusLang>> + 'p,
                >,
                ParseError,
            > {
                Ok(alloc::boxed::Box::new(BogusParser))
            }
        }

        let language: Language<BogusLang> =
            Language::new(BogusDriver, ParsingState::lang_initial().expect("seed state"));
        let err = language.parse("x").unwrap_err();
        assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
    }

    // --- the descent guard end to end (Part 2) ------------------------------------------

    use crate::constructs::DescentLimitExceeded;
    use crate::engine::{StdDescentGuard, StdDescentGuardInit};

    /// Balanced `{…{…}…}` nesting, `levels` deep, with one char inside.
    fn nested_braces(levels: usize) -> String {
        let mut input = String::new();
        for _ in 0..levels {
            input.push('{');
        }
        input.push('a');
        for _ in 0..levels {
            input.push('}');
        }
        input
    }

    #[test]
    fn deep_nesting_under_the_unconfigured_default_is_an_error_not_a_crash() {
        // 50 000 nesting levels would exhaust the call stack without the guard; the
        // built-in default refuses long before that, under either recovery policy
        // (a refusal aborts under any policy), and the process survives. The
        // refusal is self-describing: it names the built-in default and the
        // configuration entry point.
        let deep = nested_braces(50_000);
        for language in [strict(), tolerant()] {
            let err = language.parse(deep.clone()).unwrap_err();
            assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
            let message = err.to_string();
            assert!(message.contains("StdDescentGuard::DEFAULT_STACK_BUDGET"), "{message}");
            assert!(message.contains("with_descent_guard_init"), "{message}");
        }
    }

    #[test]
    fn a_tiny_fixed_budget_refuses_deep_nesting_in_any_build_profile() {
        // 200 levels always outrun a 4 KiB consumption cap, optimized or not — the
        // deterministic refusal contrast for the `Off` test below. The configured
        // refusal names the configured cap, not the built-in default.
        let language = strict()
            .with_descent_guard_init(StdDescentGuardInit::fixed_stack_budget(4 * 1024));
        let err = language.parse(nested_braces(200)).unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
        let message = err.to_string();
        assert!(message.contains("configured budget"), "{message}");
        assert!(!message.contains("with_descent_guard_init"), "{message}");
    }

    #[test]
    fn off_disables_the_limit() {
        // The same 30-level input that a 4 KiB cap refuses parses cleanly under
        // `Off` (30 levels stay comfortably within the test thread's real stack).
        let input = nested_braces(30);
        let capped = strict()
            .with_descent_guard_init(StdDescentGuardInit::fixed_stack_budget(4 * 1024));
        assert!(capped.parse(input.clone()).is_err());
        let off = strict().with_descent_guard_init(StdDescentGuardInit::off());
        let result = off.parse(input).unwrap();
        assert!(result.diagnostics.is_empty());
        check_tree_invariants(&result.tree);
    }

    #[test]
    fn depth_limit_counts_nesting_not_siblings() {
        // Depth mode is deterministic across build profiles: nesting past the
        // limit is refused; sibling constructs at the same depth re-use the level
        // (each descent's exit rebalances the count).
        let language =
            strict().with_descent_guard_init(StdDescentGuardInit::depth_limit(10));
        assert!(language.parse(nested_braces(3)).is_ok());
        let err = language.parse(nested_braces(20)).unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
        assert!(err.to_string().contains("depth limit"), "{}", err);
        // Twelve sibling groups — more constructs than the limit, none deeper
        // than the limit — parse cleanly.
        let siblings = "{a}".repeat(12);
        assert!(language.parse(siblings).is_ok());
    }

    #[test]
    fn a_depth_limit_refusal_carries_the_live_traceback() {
        // The refusal error snapshots the live frame stack, including the frame
        // of the construct whose descent was refused (pushed before the guard is
        // asked). With a limit of 4 the refused descent is a group's interior
        // content run: that group's own frame is already on the stack.
        let language =
            strict().with_descent_guard_init(StdDescentGuardInit::depth_limit(4));
        let err = language.parse(nested_braces(20)).unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
        let titles: Vec<&str> = err.frames().iter().map(|f| f.title()).collect();
        assert!(!titles.is_empty(), "the refusal carries the live traceback");
        assert!(
            titles.contains(&"group ‘{’"),
            "the traceback includes the refused construct's group frame: {titles:?}"
        );
    }

    #[test]
    fn a_computed_budget_resolves_probe_minus_headroom_at_parse_entry() {
        // The probe answers HEADROOM + 100 bytes: the resolved budget is 100
        // bytes — which the very first descent's own frames already outrun — and
        // the refusal names the headroom-subtracted number, observable end to end.
        fn probe() -> Option<usize> {
            Some(StdDescentGuard::HEADROOM + 100)
        }
        let language = strict()
            .with_descent_guard_init(StdDescentGuardInit::computed_stack_budget(probe));
        let err = language.parse("x").unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
        assert!(err.to_string().contains("configured budget of 100 bytes"), "{}", err);
    }

    #[test]
    fn an_empty_driver_impl_is_complete() {
        // Every `ParseDriver` item is defaulted — tokenization included, since
        // `make_token_reader` falls back to the language's `Lang::Tokenization` (the
        // guard is engine-owned, not a driver choice either): `impl ParseDriver<L> for
        // D {}` is a whole driver, and the language parses.
        #[derive(Debug, Clone, Copy)]
        struct OneLineLang;
        impl Lang for OneLineLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type Tokenization = crate::token::StdTokenization;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = OneLineDriver;
            fn make_node_ext(
                _kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) -> Result<(), crate::node::NodeBuildError> {
                Ok(())
            }
        }
        #[derive(Debug, Clone, Copy)]
        struct OneLineDriver;
        impl ParseDriver<OneLineLang> for OneLineDriver {}
        let language: Language<OneLineLang> =
            Language::new(OneLineDriver, ParsingState::lang_initial().expect("seed state"));
        assert!(language.parse("hello").is_ok());
    }

    // --- state threading across tolerant stray-close skips (findings #1–#3) -----------------
    //
    // A language whose top-level commands carry a `\newcommand`-style after-effect delta:
    // processing one evolves the loop's live state, so it diverges from the frozen seed
    // *before* a later stray close. The after-effect travels the public path
    // (`latexlike::MacroSpec::with_after_effect`); the findings themselves are root-loop
    // regressions, independent of the language flavor driving them.

    use crate::latexlike::{
        default_token_rules, CallableType, GroupType, Latexlike, LatexlikeDriver, MacroSpec,
    };

    fn content_group(open: &str, close: &str) -> Arc<crate::token::GroupRule<Latexlike>> {
        Arc::new(crate::token::GroupRule {
            group_type: GroupType::Content,
            open: open.into(),
            close: close.into(),
        })
    }

    /// A groups override replacing the rule list with the latexlike defaults plus
    /// `extra` (override lists replace wholesale, so an addition restates the
    /// defaults).
    fn add_groups(
        extra: &[Arc<crate::token::GroupRule<Latexlike>>],
    ) -> ParsingStateDelta<Latexlike> {
        let mut rules = default_token_rules::<Latexlike>().groups.rules;
        rules.extend(extra.iter().cloned());
        ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides { rules: Some(rules), ..GroupOverrides::default() },
            ..TokenRulesOverrides::default()
        })
    }

    /// A tolerant latexlike language whose seed defines `\{name}` as a zero-argument
    /// macro with after-effect `delta`.
    fn latexlike_defining(
        name: &str,
        delta: ParsingStateDelta<Latexlike>,
    ) -> Language<Latexlike> {
        let mut lib: Package<Latexlike> = Package::new("test");
        lib.insert(CallableType::Macro, name, MacroSpec::new(vec![]).with_after_effect(delta));
        Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::lang_initial_with_packages([lib]).expect("seed state"),
        )
    }

    /// A zero-arg macro package defining `name` — a `\def`-style after-effect payload.
    fn zero_arg_macro(name: &str) -> Arc<Package<Latexlike>> {
        let mut lib: Package<Latexlike> = Package::new("defined");
        lib.insert(CallableType::Macro, name, MacroSpec::default());
        Arc::new(lib)
    }

    /// The callable names among a result's root children, in order (chars/other skipped).
    fn callable_names(result: &ParseResult<Latexlike>) -> Vec<String> {
        result
            .tree
            .root()
            .children()
            .iter().filter_map(|child| child.name().map(String::from))
            .collect()
    }

    #[test]
    fn a_stray_close_of_a_delimiter_a_sibling_delta_added_recovers_tolerantly() {
        // Finding #1. `\addangle`'s after-effect adds `<`/`>` as a group pair the seed
        // state lacks, so the `>` that follows is a stray close *only under the loop's
        // evolved state*. Tolerant parsing must diagnose and skip it. The bug: the old
        // root loop re-tokenized the close under the frozen `seed`, where `>` is an
        // ordinary character (not a `GroupClose`), so the recovery misfired into a
        // spurious `ImplementationError` that aborted the parse even under tolerant
        // recovery. The delimiter the loop actually saw must drive the diagnosis, never a
        // re-read under a different state.
        let language = latexlike_defining("addangle", add_groups(&[content_group("<", ">")]));
        let result = language.parse("\\addangle >x").unwrap();
        // Recovery continued past the stray close: `\addangle` staged and `x` reached.
        assert_eq!(callable_names(&result), ["addangle"]);
        assert!(
            result.tree.root().children().iter().any(|c| c.chars() == Some("x")),
            "content after the stray close should be parsed"
        );
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        assert!(diagnostic.message().contains('>'));
    }

    #[test]
    fn definitions_before_a_tolerant_stray_close_stay_in_scope_after_it() {
        // Finding #2. `\def`'s after-effect defines `\late` for subsequent siblings; a
        // stray `}` sits between the definition and its use. Tolerant parsing must skip the
        // `}` and *continue with the definition in scope*, so `\late` resolves. The bug:
        // the old root loop resumed every descent from the frozen `seed`, so the `\def`
        // definition was silently dropped across the skip and `\late` came out unresolvable
        // (a second, spurious diagnostic plus a chars fallback).
        let language = latexlike_defining(
            "def",
            ParsingStateDelta::new().push_provider(zero_arg_macro("late")),
        );
        let result = language.parse("\\def } \\late").unwrap();
        // `\late` resolved against the definition established before the stray close…
        assert_eq!(callable_names(&result), ["def", "late"]);
        // …so the only diagnostic is the stray close itself — no unresolvable-command.
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics.iter().next().unwrap().identifier(),
            StrayGroupClose::IDENTIFIER
        );
    }

    #[test]
    fn a_stray_close_reports_the_delimiter_the_loop_saw_not_a_reparse() {
        // Finding #3. `\widen`'s after-effect adds `[[`/`]]`; the seed already closes on a
        // single `]`. A stray `]]` is one 2-char close under the loop's evolved state, and
        // the diagnosis must report *that* delimiter. The bug: the old root loop
        // re-tokenized the close under the frozen `seed`, which only knows the 1-char `]`,
        // so it reported the wrong (shorter) delimiter and consumed a single byte — leaving
        // the second `]` to surface as a *second* stray close. Carrying the delimiter on
        // the stop cause makes the loop report `]]` once and skip both bytes.
        // The seed must already close on a single `]` (the finding's premise), so the
        // language seeds from a state derived with the `[`/`]` pair added; `\widen`'s
        // after-effect keeps that pair and adds `[[`/`]]`.
        let bracket = content_group("[", "]");
        let mut lib: Package<Latexlike> = Package::new("test");
        lib.insert(
            CallableType::Macro,
            "widen",
            MacroSpec::new(vec![])
                .with_after_effect(add_groups(&[bracket.clone(), content_group("[[", "]]")])),
        );
        let seed = ParsingState::lang_initial_with_packages([lib]).expect("seed state")
            .derived(&add_groups(&[bracket]))
            .unwrap();
        let language = Language::new(LatexlikeDriver::new(Recovery::Tolerant), seed);
        let result = language.parse("\\widen ]]x").unwrap();
        assert_eq!(callable_names(&result), ["widen"]);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        assert!(
            diagnostic.message().contains("]]"),
            "diagnostic should name the 2-char delimiter, got: {}",
            diagnostic.message()
        );
    }
}
