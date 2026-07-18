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
use crate::source::{
    resolve_source, NoResolver, ResolveError, Source, SourceResolver, SourceSpan, Span,
};
use crate::state::{DeriveError, Lang, ParsingState, ParsingStateDelta};
use crate::token::{StdTokenReader, TokenKind};

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
    /// funnel; tolerant parses consume the token and resume (its bytes are dropped
    /// from the tree — the accepted tolerant byte-accounting break), strict parses
    /// abort. Each resume re-enters under the **seed** state: sibling-level state
    /// changes from before the stray close do not carry across it (the content loop's
    /// state is internal by the state-threading convention).
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
            let (outcome, _delta) =
                cx.parse_nodes(Arc::clone(&seed), StopSpec::none(), ChildStateSpec::inherit())?;
            nodes.extend(outcome.nodes);
            match outcome.stop {
                StopCause::EndOfInput => break,
                StopCause::UnexpectedGroupClose { span } => {
                    // Diagnose-and-skip at the root (DESIGN_RATIONALE.md §3.8): the
                    // loop left the close token unconsumed at `span.start`.
                    let stray = match cx.tokens.peek(&seed) {
                        Ok(token) => token,
                        Err(error) => {
                            return Err(ParseError::from_token_error(
                                error.kind().clone(),
                                SourceSpan::new(&cx.source, error.span()),
                            )
                            .with_frames(cx.session.snapshot_frames()))
                        }
                    };
                    let TokenKind::GroupClose { delim } = stray.kind else {
                        return Err(cx.implementation_error(
                            "UnexpectedGroupClose stop without a GroupClose token \
                             at the stop position (nodes-parser contract violation)",
                            span,
                        ));
                    };
                    cx.recover(
                        StrayGroupClose { delim: delim.to_string() },
                        SourceSpan::new(&cx.source, stray.span),
                    )?;
                    cx.tokens.move_past(&stray, true);
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
    use crate::engine::{ParseDriver, StdParseDriver};
    use crate::error::{DiagnosticInfo, Recovery};
    use crate::node::check_tree_invariants;
    use crate::scopes::{ScopeOp, ScopeStack};
    use crate::source::{MapResolver, SourceProvenance};
    use crate::state::{StateData, TokenRulesOverrides};
    use crate::token::{
        CommentRule, GroupRule, TokenRules, WhitespaceRules,
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
            .map(|child| match child.chars() {
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
        assert_eq!(result.tree.root().children().count(), 0);
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
    fn stray_close_aborts_strict_and_is_skipped_tolerantly() {
        // Strict: the root drive aborts with the core condition.
        let err = strict().parse("a}b").unwrap_err();
        assert_eq!(err.identifier(), StrayGroupClose::IDENTIFIER);
        assert_eq!(err.span().range(), 1..2);
        assert_eq!(err.to_string(), "unexpected closing ‘}’ — no group is open");

        // Tolerant: diagnose, consume, resume — the stray byte is dropped from the
        // tree (the accepted byte-accounting break; no invariant check here).
        let result = tolerant().parse("a}b").unwrap();
        assert_eq!(shapes(&result), ["chars(a)", "chars(b)"]);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        assert_eq!(diagnostic.message(), "unexpected closing ‘}’ — no group is open");
    }

    #[test]
    fn consecutive_stray_closes_each_report_and_resume() {
        let result = tolerant().parse("}}x").unwrap();
        assert_eq!(shapes(&result), ["chars(x)"]);
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
            type Output = NodesOutcome;
            fn parse(
                &mut self,
                _cx: &mut ParseContext<'_, '_, BogusLang>,
            ) -> ConstructParserResult<
                BogusLang,
                (Self::Output, Option<ParsingStateDelta<BogusLang>>),
            > {
                Ok((
                    NodesOutcome {
                        nodes: Vec::new(),
                        stop: StopCause::TokenCondition { span: Span::empty(0) },
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
            ) -> alloc::boxed::Box<dyn ConstructParser<BogusLang, Output = NodesOutcome> + 'p>
            {
                alloc::boxed::Box::new(BogusParser)
            }
        }

        let language: Language<BogusLang> = Language::new(BogusDriver);
        let err = language.parse("x").unwrap_err();
        assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
    }
}
