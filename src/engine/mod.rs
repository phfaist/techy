//! Engine orchestration: [`ParserSession`], the root object of a parse (Phase 6).
//!
//! A session bundles everything one parse accumulates — the staging
//! [`NodeTreeBuilder`], the [`Diagnostics`] sink, and the [`Recovery`] policy — and
//! [`finish`](ParserSession::finish) freezes it into a [`ParseResult`]. Sessions are
//! transient: one parse each, no reuse.
//!
//! The `Language<L>` runtime bundle (long-lived defaults + libraries, with a `parse()`
//! convenience entry point) is **deferred** past Phase 6 (DESIGN_RATIONALE.md §3.6):
//! Phase 6 drives sessions directly, and convenience code is not written before its
//! convenience is demonstrable. Consequently `ParseResult` carries no `'env` lifetime
//! and no `Language` reference.

use core::fmt;

use crate::error::{
    Diagnostic, DiagnosticData, Diagnostics, ParseError, Recovery, Severity, TraceFrame,
};
use crate::node::{BuildId, NodeTree, NodeTreeBuilder};
use crate::source::SourceSpan;
use crate::spec::{CallableSpec, FrameRole};
use crate::state::{Lang, ParsingState, ParsingStateDelta, TokenRulesOverrides};
use crate::token::GroupRule;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// One live entry of the session's parse-frame stack (DESIGN_RATIONALE.md §3.8):
/// pushed at the descent points through
/// [`ParseContext::with_frame`](crate::constructs::ParseContext::with_frame) and
/// snapshotted into `L`-free [`TraceFrame`]s by the recover funnel. Pushes run on the
/// hot success path — once per construct — so a frame is **allocation-free to build**
/// (`Arc` bumps only); its title is rendered only at snapshot time, on the cold path.
pub struct Frame<L: Lang> {
    /// How the frame's traceback title is produced at snapshot time.
    pub title: FrameTitle<L>,
    /// Where in the source the parse descended (the traceback's location line).
    pub span: SourceSpan<L::SourceOrigin>,
}

/// A live frame's title recipe: **mechanisms, not a construct taxonomy**
/// (DESIGN_RATIONALE.md §3.8) — the core has no macro/environment vocabulary, so a
/// callable's title comes from its spec's
/// [`stack_frame_title`](CallableSpec::stack_frame_title) hook at snapshot time.
pub enum FrameTitle<L: Lang> {
    /// A fixed title.
    Static(&'static str),
    /// `label ‘<slice>’`, quoting a source slice (a group's open delimiter, an
    /// environment's name).
    Quoted {
        /// The label preceding the quoted slice.
        label: &'static str,
        /// The span whose content is quoted.
        name: SourceSpan<L::SourceOrigin>,
    },
    /// A callable's frame, titled by the spec's
    /// [`stack_frame_title`](CallableSpec::stack_frame_title) hook.
    Callable {
        /// The invocation's behavior spec.
        spec: Arc<dyn CallableSpec<L>>,
        /// Which part of the callable's parse the frame covers.
        role: FrameRole,
        /// The span of the invocation spelling, quoted into the title.
        name: SourceSpan<L::SourceOrigin>,
    },
}

impl<L: Lang> Frame<L> {
    /// Render the live frame into a snapshot [`TraceFrame`] — the cold path: titles
    /// allocate here, never on push.
    fn render(&self) -> TraceFrame<L::SourceOrigin> {
        let title = match &self.title {
            FrameTitle::Static(title) => String::from(*title),
            FrameTitle::Quoted { label, name } => {
                format!("{} ‘{}’", label, name.content())
            }
            FrameTitle::Callable { spec, role, name } => {
                spec.stack_frame_title(*role, name.content())
            }
        };
        TraceFrame::new(title, self.span.clone())
    }
}

// Manual Debug impls: derives would demand `L: Debug` although only associated types
// (already bounded) and `Arc`s are stored.

impl<L: Lang> fmt::Debug for Frame<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("title", &self.title)
            .field("span", &self.span)
            .finish()
    }
}

impl<L: Lang> fmt::Debug for FrameTitle<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameTitle::Static(title) => f.debug_tuple("Static").field(title).finish(),
            FrameTitle::Quoted { label, name } => f
                .debug_struct("Quoted")
                .field("label", label)
                .field("name", name)
                .finish(),
            FrameTitle::Callable { spec, role, name } => f
                .debug_struct("Callable")
                .field("spec", spec)
                .field("role", role)
                .field("name", name)
                .finish(),
        }
    }
}

/// The root object of one parse: node building, diagnostics, and the recovery policy.
///
/// Fields are public: construct parsers reach the builder and diagnostics through
/// [`ParseContext::session`](crate::constructs::ParseContext) — the session *is* the
/// shared mutable surface of a parse (trees stay immutable; this is the mutation
/// boundary, consumed by [`finish`](ParserSession::finish)).
pub struct ParserSession<L: Lang> {
    /// The staging node builder.
    pub builder: NodeTreeBuilder<L>,
    /// The diagnostics accumulated so far.
    pub diagnostics: Diagnostics<L::SourceOrigin>,
    /// The tolerant-parsing policy in force.
    pub recovery: Recovery,
    /// The parse-global mutable language extension ([`Lang::SessionExt`],
    /// `Default`-initialized): the preset-owned mutable object of a parse — transition
    /// observation counters ([`Lang::observe_transition`]), parse-global caches.
    pub ext: L::SessionExt,
    /// The group-interior-state memo (DESIGN_RATIONALE.md §3.6): `(base, rule, interior)`
    /// triples matched by `Arc` pointer identity, consulted only by
    /// [`group_interior_state`](ParserSession::group_interior_state) — the one derivation
    /// whose inputs recur with `Arc` identity. Entries hold their key `Arc`s alive, so
    /// pointer keys cannot be reused (no ABA hazard).
    group_interior_memo: GroupInteriorMemo<L>,
    /// The live parse-frame stack, outermost first (DESIGN_RATIONALE.md §3.8):
    /// maintained exclusively by
    /// [`ParseContext::with_frame`](crate::constructs::ParseContext::with_frame)
    /// (closure-scoped push/pop) and snapshotted — innermost first — into every
    /// condition the recover funnel records. Private: the push/pop balance is an
    /// invariant.
    frames: Vec<Frame<L>>,
}

type GroupInteriorMemo<L> =
    Vec<(Arc<ParsingState<L>>, Arc<GroupRule<L>>, Arc<ParsingState<L>>)>;

impl<L: Lang> ParserSession<L> {
    /// A fresh session under the given recovery policy.
    pub fn new(recovery: Recovery) -> ParserSession<L> {
        ParserSession {
            builder: NodeTreeBuilder::new(),
            diagnostics: Diagnostics::new(),
            recovery,
            ext: Default::default(),
            group_interior_memo: Vec::new(),
            frames: Vec::new(),
        }
    }

    /// Push a live traceback frame — called only by
    /// [`ParseContext::with_frame`](crate::constructs::ParseContext::with_frame), whose
    /// closure scoping guarantees the matching [`pop_frame`](ParserSession::pop_frame).
    pub(crate) fn push_frame(&mut self, frame: Frame<L>) {
        self.frames.push(frame);
    }

    /// Pop the innermost live traceback frame (`with_frame`'s epilogue).
    pub(crate) fn pop_frame(&mut self) {
        let popped = self.frames.pop();
        debug_assert!(popped.is_some(), "with_frame pops exactly what it pushed");
    }

    /// Snapshot the live frame stack into `L`-free [`TraceFrame`]s, innermost first —
    /// titles are rendered here, on the cold path (DESIGN_RATIONALE.md §3.8).
    pub(crate) fn snapshot_frames(&self) -> Vec<TraceFrame<L::SourceOrigin>> {
        self.frames.iter().rev().map(Frame::render).collect()
    }

    /// Session-mediated state derivation — the in-parse standard (DESIGN_RATIONALE.md
    /// §3.6): within a parse frame, construct parsers derive states through this seam so
    /// every transition event reaches [`Lang::observe_transition`] (with the session's
    /// [`ext`](ParserSession::ext)). **Data-equivalent to
    /// [`ParsingState::derived`]** — the session layer may observe, never alter the
    /// resulting state — and **never memoized**: an arbitrary delta has no identity to
    /// key a memo on (keyed helpers exist where derivation inputs have `Arc` identity —
    /// [`group_interior_state`](ParserSession::group_interior_state)).
    ///
    /// Out-of-parse code (initial states, tests, tree transforms) keeps calling
    /// `derived()` directly.
    pub fn derived_state(
        &mut self,
        base: &Arc<ParsingState<L>>,
        delta: &ParsingStateDelta<L>,
    ) -> Arc<ParsingState<L>> {
        let new = Arc::new(base.derived(delta));
        L::observe_transition(&mut self.ext, base, &new, delta);
        new
    }

    /// The group-interior derivation, deduplicated (DESIGN_RATIONALE.md §3.6): the state
    /// a group's interior is parsed under is always `base` + `expecting_group_close =
    /// rule` — the uniform invariant that guarantees the close delimiter stays
    /// recognizable — and sibling groups under one state repeat the identical
    /// derivation, the dominant state-cloning cost in deep documents. This helper keeps
    /// always-derive *semantics* but consults a `(base, rule)` pointer-identity memo, so
    /// one `StateData` clone (and one [`Lang::finalize_transition`] run) serves every
    /// sibling descent; [`Lang::observe_transition`] still fires on **every** call, memo
    /// hits included.
    pub fn group_interior_state(
        &mut self,
        base: &Arc<ParsingState<L>>,
        rule: &Arc<GroupRule<L>>,
    ) -> Arc<ParsingState<L>> {
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(Arc::clone(rule))),
            ..TokenRulesOverrides::default()
        });
        let memoized = self
            .group_interior_memo
            .iter()
            .find(|(b, r, _)| Arc::ptr_eq(b, base) && Arc::ptr_eq(r, rule))
            .map(|(_, _, interior)| Arc::clone(interior));
        let interior = match memoized {
            Some(interior) => interior,
            None => {
                let interior = Arc::new(base.derived(&delta));
                self.group_interior_memo.push((
                    Arc::clone(base),
                    Arc::clone(rule),
                    Arc::clone(&interior),
                ));
                interior
            }
        };
        L::observe_transition(&mut self.ext, base, &interior, &delta);
        interior
    }

    /// The raw record-or-abort primitive of detection-site recovery
    /// (DESIGN_RATIONALE.md §3.8, rule 1). Construct parsers call
    /// [`ParseContext::recover`](crate::constructs::ParseContext::recover) instead — the
    /// funnel that boxes the condition and applies `Lang::refine_diagnostic` (which needs
    /// the context's state) before ending up here.
    ///
    /// Under [`Recovery::Tolerant`], records the condition as an error-severity
    /// [`Diagnostic`] at `span` and returns `Ok(())` (the caller continues with its
    /// site's local recovery). Under [`Recovery::Strict`], returns the condition as a
    /// [`ParseError`] to bubble — nobody continues past an `Err`. Either carrier
    /// receives a snapshot of the live frame stack (the parse traceback).
    pub fn recover(
        &mut self,
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        let frames = self.snapshot_frames();
        match self.recovery {
            Recovery::Tolerant => {
                self.diagnostics
                    .push(Diagnostic::from_parts(Severity::Error, data, span, frames));
                Ok(())
            }
            Recovery::Strict => Err(ParseError::from_parts(data, span, frames)),
        }
    }

    /// Freeze the session: flatten everything reachable from `root` into the final
    /// [`NodeTree`] (resolving staged argument/slot regions) and hand over the
    /// diagnostics — available even for successful tolerant parses.
    pub fn finish(self, root: BuildId) -> ParseResult<L> {
        ParseResult { tree: self.builder.finish(root), diagnostics: self.diagnostics }
    }
}

impl<L: Lang> fmt::Debug for ParserSession<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParserSession")
            .field("builder", &self.builder)
            .field("diagnostics", &self.diagnostics)
            .field("recovery", &self.recovery)
            .field("ext", &self.ext)
            .field("group_interior_memo", &self.group_interior_memo.len())
            .field("frames", &self.frames.len())
            .finish()
    }
}

/// A finished parse: the frozen tree plus everything reported along the way.
pub struct ParseResult<L: Lang> {
    /// The parsed document.
    pub tree: NodeTree<L>,
    /// The diagnostics recorded during the parse (possibly non-empty even on success —
    /// tolerant parsing).
    pub diagnostics: Diagnostics<L::SourceOrigin>,
}

impl<L: Lang> fmt::Debug for ParseResult<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseResult")
            .field("tree", &self.tree)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::{ConstructParser, ConstructParserResult, ParseContext};
    use crate::library::LibraryStack;
    use crate::node::NodeKind;
    use crate::source::{Source, Span};
    use crate::state::{
        Lang, ParsingState, ParsingStateDelta, ResolvedCallable, SimpleLang, StateData,
        TokenRulesOverrides,
    };
    use crate::token::{
        GroupRule, Token, TokenKind, TokenListReader, TokenRules, WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl SimpleLang for PlainLang {}

    /// A third-party-style condition — the extension surface demonstration (§3.8): a
    /// plain data struct, a `Display` for the wording, and a `DiagnosticInfo` impl,
    /// structurally identical to the library's own conditions.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestUnresolvable {
        name: String,
    }

    impl fmt::Display for TestUnresolvable {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "unresolvable command ‘{}’", self.name)
        }
    }

    impl crate::error::DiagnosticInfo for TestUnresolvable {
        const IDENTIFIER: &'static str = "test.engine.unresolvable-command";
    }

    fn min_rules<L: Lang<GroupTypeId = u32>>() -> TokenRules<L> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups: Vec::new(),
            enable_commands: true,
            commands: Vec::new(),
            enable_comments: true,
            comments: Vec::new(),
            enable_specials: true,
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn state<L: Lang<GroupTypeId = u32, StateExt = ()>>() -> Arc<ParsingState<L>> {
        Arc::new(ParsingState::new(StateData {
            rules: min_rules(),
            libraries: LibraryStack::new(),
            ext: (),
        }))
    }

    fn span(source: &Arc<Source>, range: core::ops::Range<usize>) -> crate::source::SourceSpan {
        crate::source::SourceSpan::new(source, range)
    }

    #[test]
    fn recover_is_tolerant_or_strict() {
        use crate::error::DiagnosticInfo;

        let source: Arc<Source> = Arc::new(Source::new("abc"));

        let mut session: ParserSession<PlainLang> = ParserSession::new(Recovery::Tolerant);
        let condition = TestUnresolvable { name: "foo".into() };
        assert!(session
            .recover(alloc::boxed::Box::new(condition.clone()), span(&source, 0..3))
            .is_ok());
        assert_eq!(session.diagnostics.len(), 1);
        assert!(session.diagnostics.has_errors());
        let diagnostic = session.diagnostics.iter().next().unwrap();
        // The message is rendered from the payload's Display, on demand.
        assert_eq!(diagnostic.message(), "unresolvable command ‘foo’");
        assert_eq!(diagnostic.identifier(), TestUnresolvable::IDENTIFIER);

        let mut session: ParserSession<PlainLang> = ParserSession::new(Recovery::Strict);
        let err = session
            .recover(alloc::boxed::Box::new(condition.clone()), span(&source, 0..3))
            .unwrap_err();
        // No PartialEq on the carriers (§3.8): compare identifier and downcast fields.
        assert_eq!(err.identifier(), TestUnresolvable::IDENTIFIER);
        assert_eq!(err.data().downcast_ref::<TestUnresolvable>(), Some(&condition));
        assert_eq!(err.span().start(), 0);
        assert!(session.diagnostics.is_empty()); // strict mode records nothing
        // Display renders the condition's message; render() adds position info.
        assert_eq!(alloc::format!("{}", err), "unresolvable command ‘foo’");
        assert!(err.render().contains("line 1"));
    }

    #[test]
    fn parse_error_is_a_core_error() {
        fn assert_error<E: core::error::Error>() {}
        assert_error::<ParseError>();
    }

    /// A toy tier-2 construct parser: reads one `Char` token via the context, stages a
    /// `Chars` node, returns no delta. Exercises the full 6.1 plumbing —
    /// `ParseContext` over a `TokenListReader`, staging through the session's builder,
    /// `finish` into a `ParseResult`.
    struct OneCharParser;

    impl ConstructParser<PlainLang> for OneCharParser {
        type Output = crate::node::BuildId;

        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, PlainLang>,
        ) -> ConstructParserResult<
            PlainLang,
            (Self::Output, Option<ParsingStateDelta<PlainLang>>),
        > {
            let token = cx.tokens.next(&cx.state).expect("test token stream is error-free");
            let TokenKind::Char(_) = token.kind else { panic!("test feeds a Char token") };
            let id = cx.session.builder.add(
                NodeKind::chars(token.span),
                crate::source::SourceSpan::new(&cx.source, token.span.range()),
                cx.state.clone(),
                vec![],
            );
            Ok((id, None))
        }
    }

    #[test]
    fn construct_parser_plumbing_end_to_end() {
        let source: Arc<Source> = Arc::new(Source::new("q"));
        let st = state();
        let tokens: Vec<Token<'static, PlainLang>> =
            vec![Token::new(TokenKind::Char('q'), Span::new(0, 1), Span::empty(0))];
        let mut reader = TokenListReader::new(tokens);
        let mut session = ParserSession::new(Recovery::Tolerant);

        let mut cx = ParseContext {
            tokens: &mut reader,
            source: source.clone(),
            state: st.clone(),
            session: &mut session,
        };
        let mut parser = OneCharParser;
        let (id, delta) = parser.parse(&mut cx).unwrap();
        assert!(delta.is_none());
        assert_eq!(cx.tokens.pos(), 1);

        let result = session.finish(id);
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.tree.root().chars(), Some("q"));
    }

    #[test]
    fn context_recover_forwards_to_the_session() {
        let source: Arc<Source> = Arc::new(Source::new("x"));
        let st = state();
        let mut reader: TokenListReader<'static, PlainLang> = TokenListReader::new(vec![]);
        let mut session = ParserSession::new(Recovery::Tolerant);
        let mut cx = ParseContext {
            tokens: &mut reader,
            source: source.clone(),
            state: st,
            session: &mut session,
        };

        let condition = TestUnresolvable { name: "boom".into() };
        assert!(cx.recover(condition, span(&source, 0..1)).is_ok());
        assert_eq!(session.diagnostics.len(), 1);
    }

    // --- the live frame stack (§3.8) ----------------------------------------------------

    #[test]
    fn with_frame_pushes_pops_and_snapshots_into_diagnostics() {
        let source: Arc<Source> = Arc::new(Source::new("xy"));
        let st = state();
        let mut reader: TokenListReader<'static, PlainLang> = TokenListReader::new(vec![]);
        let mut session = ParserSession::new(Recovery::Tolerant);
        let mut cx = ParseContext {
            tokens: &mut reader,
            source: Arc::clone(&source),
            state: st,
            session: &mut session,
        };

        let frame =
            Frame { title: FrameTitle::Static("test frame"), span: span(&source, 0..1) };
        let inner_depth = cx.with_frame(frame, |cx| {
            // A condition recorded inside the frame carries it in its snapshot.
            cx.recover(TestUnresolvable { name: "x".into() }, span(&source, 1..2))
                .unwrap();
            cx.session.frames.len()
        });
        assert_eq!(inner_depth, 1);
        assert!(session.frames.is_empty(), "with_frame pops after the closure returns");

        let diagnostic = session.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.frames().len(), 1);
        assert_eq!(diagnostic.frames()[0].title(), "test frame");
        assert_eq!(diagnostic.frames()[0].span().range(), 0..1);
    }

    #[test]
    fn with_frame_pops_on_the_err_path_and_strict_errors_carry_frames() {
        let source: Arc<Source> = Arc::new(Source::new("xy"));
        let st = state();
        let mut reader: TokenListReader<'static, PlainLang> = TokenListReader::new(vec![]);
        let mut session = ParserSession::new(Recovery::Strict);
        let mut cx = ParseContext {
            tokens: &mut reader,
            source: Arc::clone(&source),
            state: st,
            session: &mut session,
        };

        // The closure body aborts (strict recover); with_frame still pops — the pop
        // after the closure returns covers the Err path by construction.
        let outer =
            Frame { title: FrameTitle::Static("outer frame"), span: span(&source, 0..1) };
        let inner =
            Frame { title: FrameTitle::Static("inner frame"), span: span(&source, 1..2) };
        let result: Result<(), ParseError> = cx.with_frame(outer, |cx| {
            cx.with_frame(inner, |cx| {
                cx.recover(TestUnresolvable { name: "x".into() }, span(&source, 1..2))
            })
        });
        let err = result.unwrap_err();
        assert!(session.frames.is_empty(), "both frames popped on the Err path");

        // The strict ParseError snapshotted the stack, innermost first.
        let titles: Vec<&str> = err.frames().iter().map(|f| f.title()).collect();
        assert_eq!(titles, ["inner frame", "outer frame"]);
        assert!(err.render().contains("Open blocks:"));
    }

    #[test]
    fn snapshot_renders_quoted_and_callable_frame_titles() {
        use crate::spec::{FrameRole, StdCallableSpec};

        let source: Arc<Source> = Arc::new(Source::new(r"{\frac ab}"));
        let mut session: ParserSession<PlainLang> = ParserSession::new(Recovery::Tolerant);
        let spec: Arc<dyn crate::spec::CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::default());

        session.push_frame(Frame {
            title: FrameTitle::Quoted { label: "group", name: span(&source, 0..1) },
            span: span(&source, 0..1),
        });
        session.push_frame(Frame {
            title: FrameTitle::Callable {
                spec: Arc::clone(&spec),
                role: FrameRole::Invocation,
                name: span(&source, 1..6),
            },
            span: span(&source, 1..6),
        });
        session.push_frame(Frame {
            title: FrameTitle::Callable {
                spec,
                role: FrameRole::Argument { index: 1 },
                name: span(&source, 1..6),
            },
            span: span(&source, 8..8),
        });

        // Innermost first; titles rendered only here (the cold path), through the
        // spec's defaulted stack_frame_title hook for callables.
        let frames = session.snapshot_frames();
        let titles: Vec<&str> = frames.iter().map(|f| f.title()).collect();
        assert_eq!(
            titles,
            ["argument #2 of ‘\\frac’", "callable ‘\\frac’", "group ‘{’"]
        );
    }

    // --- the Phase 6 Lang hook defaults ------------------------------------------------

    #[test]
    fn default_resolve_command_resolves_nothing() {
        let st = state();
        let token: Token<'static, PlainLang> = Token::new(
            TokenKind::Command { name: "foo", escape_char: '\\', post_space: Span::empty(4) },
            Span::new(0, 4),
            Span::empty(0),
        );
        let resolved: Option<ResolvedCallable<PlainLang>> =
            PlainLang::resolve_command(&st, &token);
        assert!(resolved.is_none());
    }

    #[test]
    fn default_paragraph_break_node_is_spanned_whitespace_chars() {
        let st = state();
        let token: Token<'static, PlainLang> =
            Token::new(TokenKind::ParagraphBreak, Span::new(3, 5), Span::new(1, 3));
        let kind = PlainLang::make_paragraph_break_node(&st, &token);
        match kind {
            NodeKind::Chars { content, .. } => {
                // Span-backed over the full token span (newlines included), per the
                // whitespace-as-chars invariant (§3.5).
                assert!(!content.is_owned());
                assert_eq!(content.resolve("x  \n\nz"), "\n\n");
            }
            other => panic!("expected a Chars kind, got {:?}", other),
        }
    }

    // --- the session-mediated derivation seam (6.3, DESIGN_RATIONALE.md §3.6) ----------

    #[derive(Debug, Default)]
    struct Observed {
        transitions: usize,
    }

    #[derive(Debug, Clone, Copy)]
    struct ObserverLang;
    impl Lang for ObserverLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = Observed;
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn observe_transition(
            ext: &mut Observed,
            _prev: &ParsingState<Self>,
            _new: &ParsingState<Self>,
            _delta: &ParsingStateDelta<Self>,
        ) {
            ext.transitions += 1;
        }
    }

    #[test]
    fn derived_state_is_data_equivalent_and_observed_but_never_memoized() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_comments: Some(false),
            ..TokenRulesOverrides::default()
        });
        let mut session: ParserSession<ObserverLang> = ParserSession::new(Recovery::Strict);

        let via_session = session.derived_state(&base, &delta);
        // Data-equivalent to the pure transition…
        assert_eq!(via_session.rules(), base.derived(&delta).rules());
        // …with the transition event observed.
        assert_eq!(session.ext.transitions, 1);

        // Never memoized: an identical call derives a fresh state (an arbitrary delta
        // has no identity to key a memo on).
        let again = session.derived_state(&base, &delta);
        assert!(!Arc::ptr_eq(&via_session, &again));
        assert_eq!(session.ext.transitions, 2);
    }

    #[test]
    fn group_interior_state_memoizes_by_pointer_identity_and_observes_hits() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let rule: Arc<GroupRule<ObserverLang>> =
            Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let mut session: ParserSession<ObserverLang> = ParserSession::new(Recovery::Strict);

        let first = session.group_interior_state(&base, &rule);
        assert!(first
            .rules()
            .expecting_group_close
            .as_ref()
            .is_some_and(|expected| Arc::ptr_eq(expected, &rule)));

        // Memo hit: the same interior Arc, and the transition event still observed.
        let second = session.group_interior_state(&base, &rule);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(session.ext.transitions, 2);

        // Keys are pointer identities: an equal-but-distinct rule Arc is a fresh
        // derivation, and so is a distinct base.
        let equal_rule: Arc<GroupRule<ObserverLang>> =
            Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let third = session.group_interior_state(&base, &equal_rule);
        assert!(!Arc::ptr_eq(&first, &third));
        let other_base = Arc::new(base.derived(&ParsingStateDelta::new()));
        let fourth = session.group_interior_state(&other_base, &rule);
        assert!(!Arc::ptr_eq(&first, &fourth));
        assert_eq!(session.ext.transitions, 4);
    }

    #[test]
    fn session_ext_is_default_initialized() {
        let session: ParserSession<ObserverLang> = ParserSession::new(Recovery::Tolerant);
        assert_eq!(session.ext.transitions, 0);
    }
}
