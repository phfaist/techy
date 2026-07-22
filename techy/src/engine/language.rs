//! [`Language<L>`]: the long-lived runtime bundle and the `parse()` convenience entry
//! (Phase 7.4; ARCHITECTURE.md §engine).
//!
//! A `Language` is everything a parse needs that outlives any one parse: the frozen
//! seed [`ParsingState`], the [`ParseDriver`](crate::engine::ParseDriver) instance, and
//! the [`SourceResolver`] for `\input`-like external references. It contributes at
//! exactly one moment — seeding — and owns **no per-parse state**: define a language
//! once, parse many documents in it. Per-parse accumulation lives on the transient
//! [`ParserSession`]; results are frozen [`ParseResult`]s owning their tree and
//! diagnostics (no borrow of the `Language` — nodes are self-contained).

use core::fmt;

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::constructs::{
    ChildStateSpec, ImplementationError, ParseContext, StopCause, StopSpec, StrayGroupClose,
};
use crate::error::ParseError;
use crate::node::NodeKind;
use crate::scopes::SpecsProvider;
use crate::source::{
    resolve_source, NoResolver, ResolveError, Source, SourceResolver, SourceSpan, Span,
};
use crate::state::{DeriveError, Lang, ParsingState, ParsingStateDelta};
use crate::token::StdTokenReader;

use super::{ParseResult, ParserSession};

/// The runtime bundle of a language: seed state, driver, resolver — long-lived,
/// shareable (`Send + Sync` through its parts), owning no per-parse state.
///
/// Construction starts from the `Lang`'s canonical seed
/// ([`Lang::initial_state_data`], frozen through [`ParsingState::initial`]) and
/// customizes by *deriving*, never by assembling states from scratch:
/// [`with_seed_delta`](Language::with_seed_delta) routes through
/// [`derived()`](ParsingState::derived), so [`Lang::finalize_transition`] holds its
/// invariants over every customized seed. The `Lang` hook remains the seed source for
/// parses driven without a `Language` (the advanced path).
///
/// ```
/// # use techy::{Language, Recovery, SimpleLang, StdParseDriver};
/// # #[derive(Debug, Clone, Copy)]
/// # struct MyLang;
/// # impl SimpleLang for MyLang {}
/// let language: Language<MyLang> = Language::new(StdParseDriver::new(Recovery::Tolerant));
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
/// let mut reader = StdTokenReader::new(content);
/// let mut cx = ParseContext::new(
///     &mut reader,
///     source,
///     Arc::clone(language.initial_state()),
///     &mut session,
///     language.driver(),
/// );
/// ```
pub struct Language<L: Lang> {
    /// The language's parse-behavior instance ([`Lang::Driver`]).
    driver: L::Driver,
    /// The frozen seed state every parse starts from — shared by `Arc` across parses
    /// (states are immutable).
    initial_state: Arc<ParsingState<L>>,
    /// Resolver for `\input`-like external references (DESIGN_RATIONALE.md §3.3);
    /// [`NoResolver`] by default — no lookup, no I/O.
    resolver: Arc<dyn SourceResolver<L::SourceOrigin>>,
}

impl<L: Lang> Language<L> {
    /// A language over `driver`, seeded from [`Lang::initial_state_data`] and with no
    /// source resolution ([`NoResolver`]).
    pub fn new(driver: L::Driver) -> Language<L> {
        Language {
            driver,
            initial_state: Arc::new(ParsingState::initial()),
            resolver: Arc::new(NoResolver),
        }
    }

    /// Customize the seed state by deriving with `delta` — the sanctioned
    /// customization path ([`Lang::initial_state_data`]'s contract): the derivation
    /// runs [`Lang::finalize_transition`], so language invariants hold over the
    /// customized seed. Everything a delta expresses is available: token-rules
    /// overrides, a mode override, scope ops (pushing packages), an ext replacement.
    ///
    /// Fallible because scope ops are (Phase 7.3): a failing op yields the
    /// [`DeriveError`], and the `Language` under construction is dropped — a bad
    /// definition setup is an embedder bug to surface at build time, not a source
    /// condition to recover from.
    pub fn with_seed_delta(
        mut self,
        delta: ParsingStateDelta<L>,
    ) -> Result<Language<L>, DeriveError<L>> {
        self.initial_state = Arc::new(self.initial_state.derived(&delta)?);
        Ok(self)
    }

    /// Push `provider` onto the seed state's scope stack (innermost — it shadows
    /// what is below) — sugar for the dominant [`with_seed_delta`](Language::with_seed_delta)
    /// shape, "define a package, add it to the language" (promoted from the preset's
    /// test support, Phase 7.9):
    ///
    /// ```ignore
    /// let language = Language::<Latexlike>::default()
    ///     .with_provider(Arc::new(my_package))?;
    /// ```
    ///
    /// Fallible like [`with_seed_delta`](Language::with_seed_delta) (the sanctioned
    /// derive path underneath): the push itself cannot fail today, but the derivation
    /// runs the full transition machinery.
    pub fn with_provider(
        self,
        provider: Arc<dyn SpecsProvider<L>>,
    ) -> Result<Language<L>, DeriveError<L>> {
        self.with_seed_delta(ParsingStateDelta::new().push_provider(provider))
    }

    /// Use `resolver` for `\input`-like external references (default: [`NoResolver`]).
    pub fn with_resolver(
        mut self,
        resolver: impl SourceResolver<L::SourceOrigin> + 'static,
    ) -> Language<L> {
        self.resolver = Arc::new(resolver);
        self
    }

    /// The frozen seed state every parse starts from.
    pub fn initial_state(&self) -> &Arc<ParsingState<L>> {
        &self.initial_state
    }

    /// The language's [`ParseDriver`](crate::engine::ParseDriver) instance —
    /// concretely typed, so preset helper methods are directly reachable.
    pub fn driver(&self) -> &L::Driver {
        &self.driver
    }

    /// The language's source resolver.
    pub fn resolver(&self) -> &Arc<dyn SourceResolver<L::SourceOrigin>> {
        &self.resolver
    }

    /// Resolve an external reference through this language's resolver and mint the
    /// [`Source`] — the [`resolve_source`] composition: provenance
    /// (`Resolved { reference, triggered_at }`) is stamped here, per include site,
    /// so diagnostics inside the inclusion render the right include chain. Feed the
    /// result to [`parse_source`](Language::parse_source).
    pub fn resolve_source(
        &self,
        reference: &str,
        triggered_at: &SourceSpan<L::SourceOrigin>,
    ) -> Result<Arc<Source<L::SourceOrigin>>, ResolveError> {
        resolve_source(&self.resolver, reference, triggered_at)
    }

    /// Parse `content` as an anonymous in-memory [`Source`]. For a pre-minted source
    /// carrying origin or provenance (a file, a [`resolve_source`](Language::resolve_source)
    /// result), use [`parse_source`](Language::parse_source).
    pub fn parse(
        &self,
        content: impl Into<String>,
    ) -> Result<ParseResult<L>, ParseError<L::SourceOrigin>> {
        self.parse_source(Arc::new(Source::new(content)))
    }

    /// Parse `source` under this language's defaults: tokenize with the seed state's
    /// rules, drive the root content loop (through the driver's construct provision,
    /// like every descent), stage the root `List` over the whole source, and freeze
    /// the session into a [`ParseResult`].
    ///
    /// **Recovery** follows the driver's policy. A stray group close at the root —
    /// nobody's to claim — is diagnosed as [`StrayGroupClose`] through the recover
    /// funnel; tolerant parses consume the delimiter, stage it as a `Chars` node
    /// (the markup-in-chars recovery artifact — revised in Phase 7.9, superseding
    /// 7.4's byte-dropping quirk: the root span partition now holds across the
    /// skip, so `check_tree_invariants` stays clean on recovered parses), and
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
    ) -> Result<ParseResult<L>, ParseError<L::SourceOrigin>> {
        let mut reader = StdTokenReader::new(source.content());
        let mut session = ParserSession::new();
        let mut nodes = Vec::new();
        let seed = Arc::clone(&self.initial_state);
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&source),
            Arc::clone(&seed),
            &mut session,
            &self.driver,
        );
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
                StopCause::UnexpectedGroupClose { span } => {
                    // Diagnose-and-skip at the root (DESIGN_RATIONALE.md §3.8): the
                    // loop left the close unconsumed at `span.start`, and the span is
                    // the delimiter exactly as matched (`StopCause`'s contract) —
                    // sliced, not re-peeked: a re-read under any state but the loop's
                    // own could tokenize different bytes.
                    let delim = span.slice(cx.source.content()).to_string();
                    cx.recover(StrayGroupClose { delim }, SourceSpan::new(&cx.source, span))?;
                    cx.tokens.move_to_pos(span.end());
                    // Stage the consumed delimiter as a chars node (the
                    // markup-in-chars recovery artifact; 7.9): the root partition
                    // stays exact across the skip.
                    let id = cx
                        .session
                        .builder
                        .add(
                            NodeKind::chars(span),
                            SourceSpan::new(&cx.source, span),
                            Arc::clone(&cx.state),
                            Vec::new(),
                        )
                        .map_err(|error| cx.implementation_error(error, span))?;
                    nodes.push(id);
                }
                StopCause::TokenCondition { span } => {
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
                        Span::empty(cx.tokens.pos()),
                    ));
                }
            }
        }
        let root = cx
            .session
            .builder
            .add(NodeKind::list(), SourceSpan::entire(&source), seed, nodes)
            .map_err(|error| cx.implementation_error(error, Span::empty(0)))?;
        session.finish(root).map_err(|error| {
            ParseError::new(
                ImplementationError::new(error.to_string()),
                SourceSpan::entire(&source),
            )
        })
    }
}

/// The all-defaults language bundle, for drivers constructible without configuration
/// (e.g. [`StdParseDriver`](crate::engine::StdParseDriver), whose default is strict).
impl<L: Lang> Default for Language<L>
where
    L::Driver: Default,
{
    fn default() -> Self {
        Language::new(L::Driver::default())
    }
}

// Manual Debug: a derive would demand `L: Debug`, and `dyn SourceResolver` carries no
// `Debug` bound — the field is shown by presence only.
impl<L: Lang> fmt::Debug for Language<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Language")
            .field("driver", &self.driver)
            .field("initial_state", &self.initial_state)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::{
        ConstructParser, ConstructParserResult, Invocation, StdInvocationParser,
    };
    use crate::engine::{CommandResolution, ParseDriver, ResolvedCallable, StdParseDriver};
    use crate::error::{DiagnosticInfo, Recovery};
    use crate::node::{check_tree_invariants, BuildId};
    use crate::scopes::{CallableQuery, CallableSyntax, Package, ScopeOp, ScopeStack};
    use crate::source::{MapResolver, SourceProvenance};
    use crate::spec::{CallableSpec, StdCallableSpec};
    use crate::state::{StateData, TokenRulesOverrides};
    use crate::token::{
        CommandRule, CommentRule, GroupRule, Token, TokenKind, TokenRules, WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::vec;

    /// A language whose canonical seed enables the latex-ish syntax the tests use —
    /// exercising the "Language seeds from the Lang hook" path.
    #[derive(Debug, Clone, Copy)]
    struct DocLang;
    impl Lang for DocLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type Driver = StdParseDriver;

        fn initial_state_data() -> StateData<Self> {
            StateData {
                rules: TokenRules {
                    enable_whitespace: true,
                    whitespace: WhitespaceRules { chars: " \t\n".into() },
                    enable_multi_newline_paragraphs: true,
                    enable_groups: true,
                    groups: vec![Arc::new(GroupRule {
                        group_type: 0,
                        open: "{".into(),
                        close: "}".into(),
                    })],
                    temporary_groups: Vec::new(),
                    enable_commands: false,
                    commands: Vec::new(),
                    enable_comments: true,
                    comments: vec![Arc::new(CommentRule { start: "%".into() })],
                    enable_specials: false,
                    forbidden_chars: "".into(),
                    expecting_group_close: None,
                },
                scopes: ScopeStack::new(),
                mode: (),
                ext: (),
            }
        }
    }

    fn strict() -> Language<DocLang> {
        Language::new(StdParseDriver::new(Recovery::Strict))
    }

    fn tolerant() -> Language<DocLang> {
        Language::new(StdParseDriver::new(Recovery::Tolerant))
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
        // byte-dropping quirk: the root partition invariant holds across the skip).
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

    #[test]
    fn with_seed_delta_customizes_through_the_derive_path() {
        // Disabling comments through a seed delta: `%` becomes plain content.
        let language = strict()
            .with_seed_delta(ParsingStateDelta::new().rules(TokenRulesOverrides {
                enable_comments: Some(false),
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        assert!(!language.initial_state().rules().enable_comments);
        let result = language.parse("a%b").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(a%b)"]);
    }

    #[test]
    fn with_seed_delta_surfaces_scope_op_failures() {
        let error = strict()
            .with_seed_delta(
                ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "absent".into() }),
            )
            .unwrap_err();
        assert_eq!(error.failures.len(), 1);
    }

    #[test]
    fn resolver_round_trip_parses_a_resolved_source() {
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter {content}");
        let language = strict().with_resolver(resolver);

        let main = language.parse(r"\input{chapter.tex}").unwrap();
        let trigger = main.tree.root().span().clone();
        let resolved = language.resolve_source("chapter.tex", &trigger).unwrap();
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
    fn the_default_resolver_resolves_nothing() {
        let language = strict();
        let root = language.parse("x").unwrap();
        let trigger = root.tree.root().span().clone();
        assert!(language.resolve_source("chapter.tex", &trigger).is_err());
    }

    #[test]
    fn default_language_uses_the_default_driver() {
        let language: Language<DocLang> = Language::default();
        assert_eq!(language.driver().recovery, Recovery::Strict);
        assert!(language.parse("a}b").is_err());
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
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ();
            type Driver = BogusDriver;
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
                (Self::Output, Option<ParsingStateDelta<BogusLang>>),
            > {
                Ok((
                    NodesOutcome {
                        nodes: Vec::new(),
                        stop: StopCause::TokenCondition { span: Span::empty(0) },
                        state: Arc::clone(&cx.state),
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
            ) -> alloc::boxed::Box<
                dyn ConstructParser<BogusLang, Output = NodesOutcome<BogusLang>> + 'p,
            > {
                alloc::boxed::Box::new(BogusParser)
            }
        }

        let language: Language<BogusLang> = Language::new(BogusDriver);
        let err = language.parse("x").unwrap_err();
        assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
    }

    // --- state threading across tolerant stray-close skips (findings #1–#3) -----------------
    //
    // A language whose top-level commands carry a `\newcommand`-style after-effect delta:
    // processing one evolves the loop's live state, so it diverges from the frozen seed
    // *before* a later stray close. The scaffolding mirrors the `CmdLang` pattern in the
    // `constructs::nodes_parser` tests (a driver that resolves commands against the scope
    // stack, since `StdParseDriver` resolves nothing).

    const CT_MACRO: u32 = 1;

    #[derive(Debug, Clone, Copy)]
    struct MacroLang;
    impl Lang for MacroLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type Driver = MacroDriver;

        fn initial_state_data() -> StateData<Self> {
            StateData {
                rules: macro_rules(vec![brace_rule(), bracket_rule()]),
                scopes: ScopeStack::new(),
                mode: (),
                ext: (),
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct MacroDriver {
        recovery: Recovery,
    }
    impl MacroDriver {
        fn new(recovery: Recovery) -> Self {
            MacroDriver { recovery }
        }
    }
    impl ParseDriver<MacroLang> for MacroDriver {
        fn recovery(&self) -> Recovery {
            self.recovery
        }
        fn resolve_command(
            &self,
            state: &ParsingState<MacroLang>,
            token: &Token<'_, MacroLang>,
        ) -> CommandResolution<MacroLang> {
            let TokenKind::Command { name, escape_char, .. } = &token.kind else {
                return CommandResolution::Unresolved { detail: None };
            };
            let query = CallableQuery::new(
                CT_MACRO,
                name,
                CallableSyntax::Command { escape_char: *escape_char },
            )
            .with_token(token);
            match state.scopes().retrieve_spec(&query, state) {
                Ok(resolved) => resolved
                    .map(|spec| ResolvedCallable { callable_type: CT_MACRO, spec })
                    .into(),
                Err(error) => {
                    CommandResolution::Unresolved { detail: Some(error.to_string()) }
                }
            }
        }
    }

    /// A callable whose invocation stages the standard node, then returns `delta` as its
    /// after-effect for subsequent siblings — the `\newcommand` shape.
    #[derive(Debug)]
    struct AfterEffectSpec {
        delta: ParsingStateDelta<MacroLang>,
    }
    impl CallableSpec<MacroLang> for AfterEffectSpec {
        fn make_invocation_parser<'a, 's>(
            &'a self,
            invocation: Invocation<'a, 's, MacroLang>,
        ) -> alloc::boxed::Box<dyn ConstructParser<MacroLang, Output = BuildId> + 'a>
        where
            's: 'a,
        {
            alloc::boxed::Box::new(AfterEffectParser {
                inner: StdInvocationParser::new(invocation),
                delta: self.delta.clone(),
            })
        }
    }
    struct AfterEffectParser<'a, 's> {
        inner: StdInvocationParser<'a, 's, MacroLang>,
        delta: ParsingStateDelta<MacroLang>,
    }
    impl ConstructParser<MacroLang> for AfterEffectParser<'_, '_> {
        type Output = BuildId;
        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, MacroLang>,
        ) -> ConstructParserResult<MacroLang, (BuildId, Option<ParsingStateDelta<MacroLang>>)>
        {
            let (id, _) = self.inner.parse(cx)?;
            Ok((id, Some(self.delta.clone())))
        }
    }

    fn brace_rule() -> Arc<GroupRule<MacroLang>> {
        Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() })
    }
    fn bracket_rule() -> Arc<GroupRule<MacroLang>> {
        Arc::new(GroupRule { group_type: 1, open: "[".into(), close: "]".into() })
    }
    fn angle_rule() -> Arc<GroupRule<MacroLang>> {
        Arc::new(GroupRule { group_type: 2, open: "<".into(), close: ">".into() })
    }
    fn double_bracket_rule() -> Arc<GroupRule<MacroLang>> {
        Arc::new(GroupRule { group_type: 3, open: "[[".into(), close: "]]".into() })
    }

    fn macro_rules(groups: Vec<Arc<GroupRule<MacroLang>>>) -> TokenRules<MacroLang> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups,
            temporary_groups: Vec::new(),
            enable_commands: true,
            commands: vec![Arc::new(CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            })],
            enable_comments: true,
            comments: vec![Arc::new(CommentRule { start: "%".into() })],
            enable_specials: false,
            forbidden_chars: "".into(),
            expecting_group_close: None,
        }
    }

    /// A tolerant `MacroLang` whose seed defines `name` as a command with after-effect
    /// `delta`.
    fn macro_lang_defining(
        name: &str,
        delta: ParsingStateDelta<MacroLang>,
    ) -> Language<MacroLang> {
        let mut lib: Package<MacroLang> = Package::new("test");
        lib.insert(CT_MACRO, name, Arc::new(AfterEffectSpec { delta }));
        Language::new(MacroDriver::new(Recovery::Tolerant))
            .with_seed_delta(ParsingStateDelta::new().push_provider(Arc::new(lib)))
            .unwrap()
    }

    /// A zero-arg macro package defining `name` — a `\def`-style after-effect payload.
    fn zero_arg_macro(name: &str) -> Arc<Package<MacroLang>> {
        let mut lib: Package<MacroLang> = Package::new("defined");
        lib.insert(CT_MACRO, name, Arc::new(StdCallableSpec::default()));
        Arc::new(lib)
    }

    /// A `groups` override keeping the seed's `{}`/`[]` and adding `extra`.
    fn add_group(extra: Arc<GroupRule<MacroLang>>) -> ParsingStateDelta<MacroLang> {
        ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: Some(vec![brace_rule(), bracket_rule(), extra]),
            ..TokenRulesOverrides::default()
        })
    }

    /// The callable names among a result's root children, in order (chars/other skipped).
    fn callable_names(result: &ParseResult<MacroLang>) -> Vec<String> {
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
        let language = macro_lang_defining("addangle", add_group(angle_rule()));
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
        let language = macro_lang_defining(
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
        let language = macro_lang_defining("widen", add_group(double_bracket_rule()));
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
