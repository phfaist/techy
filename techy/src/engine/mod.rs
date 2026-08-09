//! Engine orchestration: [`ParserSession`], the root object of a parse, and
//! the [`ParseDriver`] behavior object.
//!
//! A session bundles everything one parse **accumulates** — the staging
//! [`NodeTreeBuilder`], the [`Diagnostics`] sink, the derivation memos, the live frame
//! stack — and [`finish`](ParserSession::finish) freezes it into a [`ParseResult`].
//! Sessions are transient: one parse each, no reuse, pure scratch/output. Parse
//! *behavior* — the [`Recovery`] policy included, moved off the session in 7.2 — lives
//! on the language's [`ParseDriver`] (see its docs for the placement doctrine).
//!
//! The [`Language<L>`] runtime bundle is the long-lived counterpart: seed
//! state, driver instance, and source resolver, with the [`parse()`](Language::parse)
//! convenience entry driving reader → root content loop → root list → `finish()`.
//! `ParseResult` deliberately carries no `'env` lifetime and no `Language` reference: nodes are self-contained, results outlive their bundle.

mod driver;
mod language;
mod state_memo;

use core::fmt;

use crate::error::{
    Diagnostic, DiagnosticData, Diagnostics, ParseError, Recovery, Severity, TraceFrame,
};
use crate::node::{BuildId, NodeBuildError, NodeTree, NodeTreeBuilder};
use crate::source::SourceSpan;
use crate::spec::{CallableSpec, FrameRole};
use crate::state::{DeriveError, Lang, ParsingState, ParsingStateDelta, ParsingStateStack};
use crate::token::GroupRule;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use state_memo::{
    GroupInteriorEntry, GroupInteriorKey, GroupInteriorMemo, GroupInteriorProbe, StateMemo,
    StateMemoKey, StateMemoProbe,
};

pub use driver::{
    resolve_command_in_scopes, CommandResolution, CommandResolver, ParseDriver,
    ResolvedCallable, ScopesCommandResolver, StdParseDriver,
};
pub use language::Language;

/// One live entry of the session's parse-frame stack:
/// pushed at the descent points through
/// [`ParseContext::with_frame`](crate::constructs::ParseContext::with_frame) and
/// snapshotted into `L`-free [`TraceFrame`]s by the recovery entry point. Pushes run
/// once per construct during normal parsing, so a frame is **allocation-free to
/// build** (`Arc` reference-count increments only); its title is rendered only at
/// snapshot time, when a problem is being reported.
pub struct Frame<L: Lang> {
    /// How the frame's traceback title is produced at snapshot time.
    pub title: FrameTitle<L>,
    /// Where in the source the parse descended (the traceback's location line).
    pub span: SourceSpan<L::SourceOrigin>,
}

/// A live frame's title recipe: **mechanisms, not a construct taxonomy**
/// — the core has no macro/environment vocabulary, so a
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
    /// Render the live frame into a snapshot [`TraceFrame`] — run only when a
    /// condition is recorded: titles
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
/// The diagnostics and ext fields are public: construct parsers reach them through
/// [`ParseContext::session`](crate::constructs::ParseContext) — the session *is* the
/// shared mutable surface of a parse (trees stay immutable; this is the mutation
/// boundary, consumed by [`finish`](ParserSession::finish)). The staging builder is
/// deliberately **not** public: parse staging goes exclusively through the single
/// staging entry point,
/// [`ParseContext::stage_node`](crate::constructs::ParseContext::stage_node) (which
/// mints the node ext — no parser can stage an unpopulated node); the read view is
/// [`ParseContext::staged_nodes`](crate::constructs::ParseContext::staged_nodes).
pub struct ParserSession<L: Lang> {
    /// The staging node builder (crate-internal; see the type docs).
    pub(crate) builder: NodeTreeBuilder<L>,
    /// The diagnostics accumulated so far.
    pub diagnostics: Diagnostics<L::SourceOrigin>,
    /// The parse-global mutable language extension ([`Lang::SessionExt`],
    /// `Default`-initialized): the preset-owned mutable object of a parse — transition
    /// observation counters ([`ParseDriver::observe_transition`]), parse-global caches.
    pub ext: L::SessionExt,
    /// The derivation memo:
    /// overrides-only derivations (token rules and/or mode) deduplicated by
    /// [`derived_state`](ParserSession::derived_state), keyed on base-state `Arc`
    /// identity plus the delta's overrides — rule payloads by `Arc` identity, the mode
    /// override by value (see [`state_memo`]). Entries hold their key `Arc`s alive, so
    /// pointer keys cannot be reused (no ABA hazard); retention is bounded by the
    /// session — one transient parse.
    state_memo: StateMemo<L>,
    /// The group-interior derivation memo: one entry per `(base, rule)`
    /// descent — the canonical expecting-close derivation *merged with the driver's*
    /// [`group_interior_delta`](ParseDriver::group_interior_delta), which runs on memo
    /// miss only. Deliberately separate from [`state_memo`](ParserSession::state_memo):
    /// sharing it would let a hand-built expecting-close delta collide with a
    /// driver-augmented descent under one key (see [`state_memo`] module docs).
    group_interior_memo: GroupInteriorMemo<L>,
    /// The live parse-frame stack, outermost first:
    /// maintained exclusively by
    /// [`ParseContext::with_frame`](crate::constructs::ParseContext::with_frame)
    /// (closure-scoped push/pop) and snapshotted — innermost first — into every
    /// condition the recover funnel records. Private: the push/pop balance is an
    /// invariant.
    frames: Vec<Frame<L>>,
    /// The live **enclosing-state stack** ([`ParsingStateStack`]): the states the
    /// parse descended through, pushed/popped at the same descent points as the
    /// frame stack — maintained exclusively by
    /// [`ParseContext::with_parsing_state`](crate::constructs::ParseContext::with_parsing_state)
    /// (closure-scoped push/pop) and lent by reference to the driver's
    /// event-lowering hook ([`ParseDriver::resolve_state_event`]) inside
    /// [`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state).
    /// The engine retains exactly these states implicitly anyway (leaving a scope
    /// structurally restores the outer `Arc`) — the stack only materializes them,
    /// and it is dropped with the session: no ancestry data survives into parsed
    /// material. Private: the push/pop balance is an invariant.
    state_stack: ParsingStateStack<L>,
}

impl<L: Lang> ParserSession<L> {
    /// A fresh session. The recovery policy is no longer session state — it lives on
    /// the language's [`ParseDriver`]: the session is pure scratch/output.
    pub fn new() -> ParserSession<L> {
        ParserSession {
            builder: NodeTreeBuilder::new(),
            diagnostics: Diagnostics::new(),
            ext: Default::default(),
            state_memo: StateMemo::new(),
            group_interior_memo: GroupInteriorMemo::new(),
            frames: Vec::new(),
            state_stack: ParsingStateStack::new(),
        }
    }

    /// Push onto the live enclosing-state stack — called only by
    /// [`ParseContext::with_parsing_state`](crate::constructs::ParseContext::with_parsing_state)
    /// (and the event-lowering lend in
    /// [`derive_state`](crate::constructs::ParseContext::derive_state)), whose
    /// closure scoping guarantees the matching pop.
    pub(crate) fn push_state(&mut self, state: Arc<ParsingState<L>>) {
        self.state_stack.push(state);
    }

    /// Pop the innermost enclosing-state entry (`with_parsing_state`'s epilogue).
    pub(crate) fn pop_state(&mut self) {
        let popped = self.state_stack.pop();
        debug_assert!(popped.is_some(), "with_parsing_state pops exactly what it pushed");
    }

    /// The live enclosing-state stack (innermost-first; see [`ParsingStateStack`]).
    pub(crate) fn state_stack(&self) -> &ParsingStateStack<L> {
        &self.state_stack
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
    /// titles are rendered here, only when a condition is recorded. Public for
    /// custom parser code building its own [`ParseError`]s
    /// ([`ParseError::with_frames`](crate::error::ParseError::with_frames)); the
    /// stack itself is only mutated through
    /// [`ParseContext::with_frame`](crate::constructs::ParseContext::with_frame).
    pub fn snapshot_frames(&self) -> Vec<TraceFrame<L::SourceOrigin>> {
        self.frames.iter().rev().map(Frame::render).collect()
    }

    /// Session-mediated state derivation — the in-parse standard: within a parse
    /// frame, construct parsers derive states through this method
    /// (usually via [`ParseContext::derive_state`], which supplies
    /// the driver **and lowers context-dependent events first** — this session
    /// method performs no event lowering) so every transition event reaches
    /// [`ParseDriver::observe_transition`] (with the session's
    /// [`ext`](ParserSession::ext)). **Data-equivalent to
    /// [`ParsingState::derived`]** — the session layer may deduplicate and observe,
    /// never alter the resulting state.
    ///
    /// [`ParseContext::derive_state`]: crate::constructs::ParseContext::derive_state
    ///
    /// **Overrides-only deltas are memoized**: when the delta carries no ext replacement, no
    /// events, and no scope ops — i.e. only token-rules overrides and/or a mode
    /// override — the derivation is keyed on the base state's `Arc` identity plus the
    /// overrides (rule payloads by `Arc` identity, gates and mode by value — see the
    /// `state_memo` module) and deduplicated across the session. `derived()` is a pure
    /// function of (base data, delta, events) — [`Lang::finalize_transition`]'s purity
    /// contract — so a pointer-keyed hit is exact; identity keying can only miss on
    /// value-equal-but-distinct `Arc`s, never falsely hit (and the mode key, a
    /// `Copy + Eq` value, cannot even miss). Deltas carrying ext/events/scope-ops
    /// always derive fresh: those payloads have no identity to key on.
    /// [`ParseDriver::observe_transition`] fires on **every** call, memo hits included;
    /// [`Lang::finalize_transition`] runs once per unique derivation.
    ///
    /// **Fallibility**: scope ops can fail, so this method is fallible like
    /// [`ParsingState::derived`] under it. Overrides-only deltas fail only by carrying
    /// data for a feature the language declares absent
    /// ([`AbsentFeatureOverrideError`](crate::state::AbsentFeatureOverrideError), a
    /// delta-author contract violation); failures are never cached, so nothing wrong
    /// ever enters the memo. On `Err`, no transition is committed:
    /// nothing is memoized and [`observe_transition`](ParseDriver::observe_transition)
    /// has **not** fired — a caller that recovers tolerantly and continues under
    /// [`DeriveError::recovered`](crate::state::DeriveError) observes that transition
    /// itself (the [`ParseContext`](crate::constructs::ParseContext) sugar does; see
    /// its recovery path).
    ///
    /// Out-of-parse code (initial states, tests, tree transforms) keeps calling
    /// `derived()` directly.
    #[allow(clippy::result_large_err)] // large `Err` by design — see `DeriveError`
    pub fn derived_state(
        &mut self,
        driver: &L::Driver,
        base: &Arc<ParsingState<L>>,
        delta: &ParsingStateDelta<L>,
    ) -> Result<Arc<ParsingState<L>>, DeriveError<L>> {
        let memoizable =
            delta.ext.is_none() && delta.events.is_empty() && delta.scope_ops.is_empty();
        if memoizable {
            if let Some(hit) = self.state_memo.get(&StateMemoProbe {
                base,
                mode: delta.mode,
                rules: &delta.rules,
            }) {
                let new = Arc::clone(hit);
                driver.observe_transition(&mut self.ext, base, &new, delta);
                return Ok(new);
            }
        }
        let new = Arc::new(base.derived(delta)?);
        if memoizable {
            self.state_memo.insert(
                StateMemoKey {
                    base: Arc::clone(base),
                    mode: delta.mode,
                    rules: delta.rules.clone(),
                },
                Arc::clone(&new),
            );
        }
        driver.observe_transition(&mut self.ext, base, &new, delta);
        Ok(new)
    }

    /// The group-interior derivation: the state a group's interior is parsed under is
    /// always `base` + `expecting_group_close = rule` — the uniform invariant that
    /// guarantees the close delimiter stays recognizable — **merged with the driver's**
    /// [`group_interior_delta`](ParseDriver::group_interior_delta), the descent-delta
    /// channel that lets a group class change its interior's state (a math group
    /// setting the parsing mode). Sibling groups under one state repeat the identical
    /// derivation — the dominant state-cloning cost in deep documents — so the result
    /// is memoized per `(base, rule)` (`Arc` identities; a dedicated session map,
    /// deliberately separate from the general derivation memo — sharing one map
    /// would let a hand-built expecting-close delta collide with a driver-augmented
    /// descent under the same key).
    ///
    /// The driver hook runs on memo **miss** only — sound because it is contractually
    /// a pure function of `(base, rule)`; a memoized hit substitutes its previous
    /// answer, with the *merged* delta stored on the entry so
    /// [`observe_transition`](ParseDriver::observe_transition) (which fires on every
    /// call, hits included) always sees the true delta. Because the memo keys on
    /// `(base, rule)` rather than on delta shape, the descent stays deduplicated even
    /// when the driver's delta carries events or an ext replacement.
    ///
    /// The descent invariant wins over the hook: whatever the driver's delta says, the
    /// interior's `expecting_group_close` is the entered rule.
    ///
    /// **Fallibility**: a driver descent delta may carry scope ops, which
    /// can fail. On `Err`, nothing is memoized and no transition was observed (see
    /// [`derived_state`](ParserSession::derived_state)); the error's
    /// [`recovered`](crate::state::DeriveError::recovered) state still has the
    /// descent invariant applied (the forced expecting-close is an override, not an
    /// op), so a tolerant caller can safely parse the interior under it. Failures are
    /// *not* cached: every failing descent re-reports — a misbehaving driver stays
    /// loud.
    #[allow(clippy::result_large_err)] // large `Err` by design — see `DeriveError`
    pub fn group_interior_state(
        &mut self,
        driver: &L::Driver,
        base: &Arc<ParsingState<L>>,
        rule: &Arc<GroupRule<L>>,
    ) -> Result<Arc<ParsingState<L>>, DeriveError<L>> {
        if let Some(entry) = self.group_interior_memo.get(&GroupInteriorProbe { base, rule })
        {
            let new = Arc::clone(&entry.state);
            let delta = Arc::clone(&entry.delta);
            driver.observe_transition(&mut self.ext, base, &new, &delta);
            return Ok(new);
        }
        let mut delta = driver.group_interior_delta(base, rule).unwrap_or_default();
        // The descent invariant, forced last: the interior always expects exactly the
        // entered rule's close — a driver delta cannot displace it.
        delta.rules.groups.expecting_close = Some(Some(Arc::clone(rule)));
        let delta = Arc::new(delta);
        let new = Arc::new(base.derived(&delta)?);
        self.group_interior_memo.insert(
            GroupInteriorKey { base: Arc::clone(base), rule: Arc::clone(rule) },
            GroupInteriorEntry { state: Arc::clone(&new), delta: Arc::clone(&delta) },
        );
        driver.observe_transition(&mut self.ext, base, &new, &delta);
        Ok(new)
    }

    /// The raw record-or-abort primitive of detection-site recovery. Construct parsers call
    /// [`ParseContext::recover`](crate::constructs::ParseContext::recover) instead —
    /// the recovery entry point, which boxes the condition and hands it to
    /// [`ParseDriver::recover`], which applies
    /// [`refine_diagnostic`](ParseDriver::refine_diagnostic) (needing the context's
    /// state) and its recovery policy before ending up here.
    ///
    /// `recovery` is the policy for **this** condition — the driver's blanket answer,
    /// or a per-condition decision by a custom driver policy. Under
    /// [`Recovery::Tolerant`], records the condition as an error-severity
    /// [`Diagnostic`] at `span` and returns `Ok(())` (the caller continues with its
    /// site's local recovery). Under [`Recovery::Strict`], returns the condition as a
    /// [`ParseError`] to bubble — nobody continues past an `Err`. Either carrier
    /// receives a snapshot of the live frame stack (the parse traceback).
    pub fn recover(
        &mut self,
        recovery: Recovery,
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        let frames = self.snapshot_frames();
        match recovery {
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
    /// diagnostics — available even for successful tolerant parses. `Err` reports a
    /// staging-contract violation ([`NodeBuildError`]) — an implementation bug in an
    /// extension, not a source condition.
    pub fn finish(self, root: BuildId) -> Result<ParseResult<L>, NodeBuildError> {
        Ok(ParseResult { tree: self.builder.finish(root)?, diagnostics: self.diagnostics })
    }
}

impl<L: Lang> Default for ParserSession<L> {
    fn default() -> Self {
        ParserSession::new()
    }
}

impl<L: Lang> fmt::Debug for ParserSession<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParserSession")
            .field("builder", &self.builder)
            .field("diagnostics", &self.diagnostics)
            .field("ext", &self.ext)
            .field("state_memo", &self.state_memo.len())
            .field("group_interior_memo", &self.group_interior_memo.len())
            .field("frames", &self.frames.len())
            .field("state_stack", &self.state_stack.len())
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
    use crate::scopes::ScopeStack;
    use crate::node::NodeKind;
    use crate::source::{Source, Span};
    use crate::state::{
        CommentOverrides, GroupOverrides, Lang, ParsingState, ParsingStateDelta, TrivialLang,
        StateData, TokenRulesOverrides,
    };
    use crate::token::{
        CommandRules, CommentRules, ForbiddenCharsRules, GroupRule, GroupRules,
        ParagraphRules, SpecialsRules, Token, TokenKind, TokenListReader, TokenRules,
        WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl TrivialLang for PlainLang {}

    /// A third-party-style condition — the extension surface demonstration: a
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

    // `Features = AllLangFeatures` (all test languages here declare it): the plain
    // block literals below only typecheck once the per-feature stores normalize to
    // the blocks themselves.
    fn min_rules<L: Lang<GroupTypeId = u32, Features = crate::state::AllLangFeatures>>(
    ) -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: Vec::new(),
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: Vec::new(),
            },
            comments: CommentRules {
                enabled: true,
                rules: Vec::new(),
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    fn state<
        L: Lang<GroupTypeId = u32, StateExt = (), Features = crate::state::AllLangFeatures>,
    >() -> Arc<ParsingState<L>> {
        Arc::new(ParsingState::new(StateData {
            rules: min_rules(),
            scopes: ScopeStack::new(),
            mode: Default::default(),
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

        let mut session: ParserSession<PlainLang> = ParserSession::new();
        let condition = TestUnresolvable { name: "foo".into() };
        assert!(session
            .recover(
                Recovery::Tolerant,
                alloc::boxed::Box::new(condition.clone()),
                span(&source, 0..3),
            )
            .is_ok());
        assert_eq!(session.diagnostics.len(), 1);
        assert!(session.diagnostics.has_errors());
        let diagnostic = session.diagnostics.iter().next().unwrap();
        // The message is rendered from the payload's Display, on demand.
        assert_eq!(diagnostic.message(), "unresolvable command ‘foo’");
        assert_eq!(diagnostic.identifier(), TestUnresolvable::IDENTIFIER);

        let mut session: ParserSession<PlainLang> = ParserSession::new();
        let err = session
            .recover(
                Recovery::Strict,
                alloc::boxed::Box::new(condition.clone()),
                span(&source, 0..3),
            )
            .unwrap_err();
        // No PartialEq on the carriers ([§dd-dr:errors]): compare identifier and downcast fields.
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
            let id = cx.stage_node(
                NodeKind::chars(token.span),
                crate::source::SourceSpan::new(&cx.source, token.span),
                cx.state.clone(),
                vec![],
            ).unwrap();
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
        let mut session = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Tolerant, ());

        let mut cx =
            ParseContext::new(&mut reader, source.clone(), st.clone(), &mut session, &driver);
        let mut parser = OneCharParser;
        let (id, delta) = parser.parse(&mut cx).unwrap();
        assert!(delta.is_none());
        assert_eq!(cx.tokens.pos(), 1);

        let result = session.finish(id).unwrap();
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.tree.root().chars(), Some("q"));
    }

    #[test]
    fn context_recover_forwards_to_the_session() {
        let source: Arc<Source> = Arc::new(Source::new("x"));
        let st = state();
        let mut reader: TokenListReader<'static, PlainLang> = TokenListReader::new(vec![]);
        let mut session = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Tolerant, ());
        let mut cx = ParseContext::new(&mut reader, source.clone(), st, &mut session, &driver);

        let condition = TestUnresolvable { name: "boom".into() };
        assert!(cx.recover(condition, span(&source, 0..1)).is_ok());
        assert_eq!(session.diagnostics.len(), 1);
    }

    // --- the live frame stack ([§dd-dr:errors]) ----------------------------------------------------

    #[test]
    fn with_frame_pushes_pops_and_snapshots_into_diagnostics() {
        let source: Arc<Source> = Arc::new(Source::new("xy"));
        let st = state();
        let mut reader: TokenListReader<'static, PlainLang> = TokenListReader::new(vec![]);
        let mut session = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Tolerant, ());
        let mut cx =
            ParseContext::new(&mut reader, Arc::clone(&source), st, &mut session, &driver);

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
        let mut session = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Strict, ());
        let mut cx =
            ParseContext::new(&mut reader, Arc::clone(&source), st, &mut session, &driver);

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
        let mut session: ParserSession<PlainLang> = ParserSession::new();
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

    // --- the default driver's hook defaults ---------------------------------------------

    #[test]
    fn default_resolve_command_reports_unimplemented_resolution() {
        let st = state();
        let token: Token<'static, PlainLang> = Token::new(
            TokenKind::Command { name: "foo", escape_char: '\\', post_space: Span::empty(4) },
            Span::new(0, 4),
            Span::empty(0),
        );
        let resolved: CommandResolution<PlainLang> =
            StdParseDriver::new(Recovery::Strict, ()).resolve_command(&st, &token);
        match resolved {
            CommandResolution::Unresolved { detail } => {
                assert!(detail.unwrap().contains("command resolution is not implemented"));
            }
            CommandResolution::Resolved(_) => panic!("the default must not resolve"),
            CommandResolution::Failed { .. } => {
                panic!("the default must not fail operationally")
            }
        }
    }

    #[test]
    fn scopes_command_resolver_resolves_through_the_scope_stack() {
        use crate::scopes::Package;
        use crate::spec::StdCallableSpec;

        // A state whose scope stack defines `foo` under callable type 0.
        let mut package: Package<PlainLang> = Package::new("defs");
        package.insert(0u32, "foo", StdCallableSpec::default());
        let st = ParsingState::<PlainLang>::lang_initial_with_packages([package]);

        let token: Token<'static, PlainLang> = Token::new(
            TokenKind::Command { name: "foo", escape_char: '\\', post_space: Span::empty(4) },
            Span::new(0, 4),
            Span::empty(0),
        );
        // Through the strategy value carried by StdParseDriver…
        let driver = StdParseDriver::new(
            Recovery::Strict,
            ScopesCommandResolver::<PlainLang> { command_type: 0u32 },
        );
        assert!(matches!(
            driver.resolve_command(&st, &token),
            CommandResolution::Resolved(_)
        ));
        // …and a clean miss carries the searched-providers detail.
        let miss: Token<'static, PlainLang> = Token::new(
            TokenKind::Command { name: "bar", escape_char: '\\', post_space: Span::empty(4) },
            Span::new(0, 4),
            Span::empty(0),
        );
        match driver.resolve_command(&st, &miss) {
            CommandResolution::Unresolved { detail } => {
                assert!(detail.unwrap().contains("defs"));
            }
            other => panic!("expected a clean miss, got {:?}", other),
        }
    }

    #[test]
    fn std_driver_source_resolver_defaults_none_and_is_configurable() {
        use crate::source::{MapResolver, SourceResolver};

        let bare: StdParseDriver = StdParseDriver::new(Recovery::Strict, ());
        assert!(ParseDriver::<PlainLang>::source_resolver(&bare).is_none());

        // By value: the builder shares internally.
        let by_value: StdParseDriver =
            StdParseDriver::new(Recovery::Strict, ()).with_source_resolver(MapResolver::new());
        assert!(ParseDriver::<PlainLang>::source_resolver(&by_value).is_some());

        // Pre-shared Arcs pass through without double-wrap (pointer identity holds).
        let premade: Arc<dyn SourceResolver> = Arc::new(MapResolver::new());
        let witness = Arc::clone(&premade);
        let shared: StdParseDriver =
            StdParseDriver::new(Recovery::Strict, ()).with_source_resolver(premade);
        assert!(Arc::ptr_eq(shared.source_resolver.as_ref().unwrap(), &witness));
    }

    #[test]
    fn default_paragraph_break_node_is_spanned_whitespace_chars() {
        let st = state();
        let token: Token<'static, PlainLang> =
            Token::new(TokenKind::ParagraphBreak, Span::new(3, 5), Span::new(1, 3));
        let kind = StdParseDriver::new(Recovery::Strict, ())
            .make_paragraph_break_node(&st, &token, "x  \n\nz");
        match kind {
            NodeKind::Chars { content, .. } => {
                // Span-backed over the full token span (newlines included), per the
                // whitespace-as-chars invariant ([§dd-dr:nodes]).
                assert!(!content.is_owned());
                let source: crate::source::Source = crate::source::Source::new("x  \n\nz");
                assert_eq!(content.resolve(&source), "\n\n");
            }
            other => panic!("expected a Chars kind, got {:?}", other),
        }
    }

    // --- the session-mediated derivation seam (6.3, DESIGN_RATIONALE.md [§dd-dr:parsers-engine]) ----------

    #[derive(Debug, Default)]
    struct Observed {
        transitions: usize,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    enum TestMode {
        #[default]
        Text,
        Math,
    }

    #[derive(Debug, Clone, Copy)]
    struct ObserverLang;
    impl Lang for ObserverLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = TestMode;
        type StateExt = ();
        type Event = ();
        type SessionExt = Observed;
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = ObserverDriver;
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) {
        }
    }

    /// ObserverLang's driver: counts every transition observation (the hook lives on the driver, not on `Lang`).
    #[derive(Debug, Clone, Copy, Default)]
    struct ObserverDriver;
    impl ParseDriver<ObserverLang> for ObserverDriver {
        fn observe_transition(
            &self,
            ext: &mut Observed,
            _prev: &ParsingState<ObserverLang>,
            _new: &ParsingState<ObserverLang>,
            _delta: &ParsingStateDelta<ObserverLang>,
        ) {
            ext.transitions += 1;
        }
    }

    #[test]
    fn derived_state_is_data_equivalent_observed_and_memoizes_rules_only_deltas() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            comments: CommentOverrides::disable(),
            ..TokenRulesOverrides::default()
        });
        let mut session: ParserSession<ObserverLang> = ParserSession::new();

        let via_session = session.derived_state(&ObserverDriver, &base, &delta).unwrap();
        // Data-equivalent to the pure transition…
        assert_eq!(via_session.rules(), base.derived(&delta).unwrap().rules());
        // …with the transition event observed.
        assert_eq!(session.ext.transitions, 1);

        // Rules-only deltas are memoized: the identical derivation returns the shared
        // state, and the transition is still observed (hits included).
        let again = session.derived_state(&ObserverDriver, &base, &delta).unwrap();
        assert!(Arc::ptr_eq(&via_session, &again));
        assert_eq!(session.ext.transitions, 2);

        // A different base misses (keys carry the base's identity).
        let other_base = Arc::new(base.derived(&ParsingStateDelta::new()).unwrap());
        let elsewhere = session.derived_state(&ObserverDriver, &other_base, &delta).unwrap();
        assert!(!Arc::ptr_eq(&via_session, &elsewhere));
    }

    #[test]
    fn derived_state_keys_payloads_by_arc_identity() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let rule: Arc<GroupRule<ObserverLang>> =
            Arc::new(GroupRule { group_type: 5, open: "[".into(), close: "]".into() });
        let mut session: ParserSession<ObserverLang> = ParserSession::new();

        // The optional-argument shape: a groups override whose Vec is rebuilt per call
        // but whose *elements* are the same Arcs — hits (elementwise identity keying).
        let delta_a = ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                rules: Some(vec![Arc::clone(&rule)]),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        });
        let delta_b = ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                rules: Some(vec![Arc::clone(&rule)]),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        });
        let first = session.derived_state(&ObserverDriver, &base, &delta_a).unwrap();
        let second = session.derived_state(&ObserverDriver, &base, &delta_b).unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        // A value-equal but Arc-distinct payload misses: identity keying is
        // conservative, never falsely shared.
        let equal_rule: Arc<GroupRule<ObserverLang>> =
            Arc::new(GroupRule { group_type: 5, open: "[".into(), close: "]".into() });
        let delta_c = ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                rules: Some(vec![equal_rule]),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        });
        let third = session.derived_state(&ObserverDriver, &base, &delta_c).unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
    }

    #[test]
    fn derived_state_memoizes_mode_overrides_by_value() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let mut session: ParserSession<ObserverLang> = ParserSession::new();

        // A mode override is memo-key material by *value* (modes are `Copy + Eq`
        // vocabulary, unlike the identity-keyed rule payloads): two independently
        // built deltas hit one entry, with the hits observed.
        let math = session.derived_state(&ObserverDriver, &base, &ParsingStateDelta::new().mode(TestMode::Math)).unwrap();
        assert_eq!(math.mode(), TestMode::Math);
        let again = session.derived_state(&ObserverDriver, &base, &ParsingStateDelta::new().mode(TestMode::Math)).unwrap();
        assert!(Arc::ptr_eq(&math, &again));
        assert_eq!(session.ext.transitions, 2);

        // Distinct modes are distinct keys — no false sharing on the same base (and a
        // mode-less delta, `mode: None`, is a third key still).
        let text = session.derived_state(&ObserverDriver, &base, &ParsingStateDelta::new().mode(TestMode::Text)).unwrap();
        assert!(!Arc::ptr_eq(&math, &text));
        assert_eq!(text.mode(), TestMode::Text);

        // Mode and rules overrides combine into one keyed derivation.
        let overrides = || TokenRulesOverrides {
            comments: CommentOverrides::disable(),
            ..TokenRulesOverrides::default()
        };
        let combo = session.derived_state(
            &ObserverDriver,
            &base,
            &ParsingStateDelta::new().mode(TestMode::Math).rules(overrides()),
        ).unwrap();
        assert_eq!(combo.mode(), TestMode::Math);
        assert!(!combo.rules().comments_enabled());
        assert!(!Arc::ptr_eq(&math, &combo)); // differs from the mode-only entry
        let combo_again = session.derived_state(
            &ObserverDriver,
            &base,
            &ParsingStateDelta::new().mode(TestMode::Math).rules(overrides()),
        ).unwrap();
        assert!(Arc::ptr_eq(&combo, &combo_again));

        // Carrying an ext replacement still disables the memo, mode or not (same for
        // events and library pushes: those payloads have no identity to key on).
        let with_ext = ParsingStateDelta::new().mode(TestMode::Math).ext(());
        let fresh_a = session.derived_state(&ObserverDriver, &base, &with_ext).unwrap();
        let fresh_b = session.derived_state(&ObserverDriver, &base, &with_ext).unwrap();
        assert!(!Arc::ptr_eq(&fresh_a, &fresh_b));
    }

    #[test]
    fn derived_state_never_memoizes_ext_or_event_deltas() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let mut session: ParserSession<ObserverLang> = ParserSession::new();

        // An event payload has no identity to key on — always a fresh derivation.
        let with_event = ParsingStateDelta::new().event(());
        let first = session.derived_state(&ObserverDriver, &base, &with_event).unwrap();
        let second = session.derived_state(&ObserverDriver, &base, &with_event).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));

        // Same for an ext replacement.
        let with_ext = ParsingStateDelta::new().ext(());
        let third = session.derived_state(&ObserverDriver, &base, &with_ext).unwrap();
        let fourth = session.derived_state(&ObserverDriver, &base, &with_ext).unwrap();
        assert!(!Arc::ptr_eq(&third, &fourth));

        // All four transitions were observed.
        assert_eq!(session.ext.transitions, 4);
    }

    #[test]
    fn group_interior_state_memoizes_by_pointer_identity_and_observes_hits() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let rule: Arc<GroupRule<ObserverLang>> =
            Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let mut session: ParserSession<ObserverLang> = ParserSession::new();

        let first = session.group_interior_state(&ObserverDriver, &base, &rule).unwrap();
        assert!(first
            .rules()
            .expecting_group_close()
            .is_some_and(|expected| Arc::ptr_eq(expected, &rule)));

        // Memo hit: the same interior Arc, and the transition event still observed.
        let second = session.group_interior_state(&ObserverDriver, &base, &rule).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(session.ext.transitions, 2);

        // Keys are pointer identities: an equal-but-distinct rule Arc is a fresh
        // derivation, and so is a distinct base.
        let equal_rule: Arc<GroupRule<ObserverLang>> =
            Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let third = session.group_interior_state(&ObserverDriver, &base, &equal_rule).unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
        let other_base = Arc::new(base.derived(&ParsingStateDelta::new()).unwrap());
        let fourth = session.group_interior_state(&ObserverDriver, &other_base, &rule).unwrap();
        assert!(!Arc::ptr_eq(&first, &fourth));
        assert_eq!(session.ext.transitions, 4);
    }

    // --- the driver seam (7.2): descent deltas, custom policy, typed helper access -----

    /// A lang whose driver enters Math mode for group class 7 — the D2 math plug at
    /// the engine seam — while counting hook runs (the counter never influences the
    /// returned delta, so the purity contract holds).
    #[derive(Debug, Clone, Copy)]
    struct DescentLang;
    impl Lang for DescentLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = TestMode;
        type StateExt = ();
        type Event = ();
        type SessionExt = Observed;
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = DescentDriver;
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) {
        }
    }

    #[derive(Debug, Default)]
    struct DescentDriver {
        hook_runs: core::sync::atomic::AtomicUsize,
    }

    impl ParseDriver<DescentLang> for DescentDriver {
        fn observe_transition(
            &self,
            ext: &mut Observed,
            _prev: &ParsingState<DescentLang>,
            _new: &ParsingState<DescentLang>,
            _delta: &ParsingStateDelta<DescentLang>,
        ) {
            ext.transitions += 1;
        }

        fn group_interior_delta(
            &self,
            _base: &ParsingState<DescentLang>,
            rule: &Arc<GroupRule<DescentLang>>,
        ) -> Option<ParsingStateDelta<DescentLang>> {
            self.hook_runs.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            (rule.group_type == 7).then(|| {
                // The delta also tries to clear the expected close — the descent
                // invariant must win over it.
                ParsingStateDelta::new().mode(TestMode::Math).rules(TokenRulesOverrides {
                    groups: GroupOverrides {
                        expecting_close: Some(None),
                        ..GroupOverrides::default()
                    },
                    ..TokenRulesOverrides::default()
                })
            })
        }
    }

    #[test]
    fn group_interior_state_merges_the_driver_delta_on_miss_only() {
        use core::sync::atomic::Ordering;

        let base: Arc<ParsingState<DescentLang>> = state();
        let math_rule: Arc<GroupRule<DescentLang>> =
            Arc::new(GroupRule { group_type: 7, open: "$".into(), close: "$".into() });
        let brace: Arc<GroupRule<DescentLang>> =
            Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let mut session: ParserSession<DescentLang> = ParserSession::new();
        let driver = DescentDriver::default();

        // A math-class descent: the driver's delta enters Math mode, and the descent
        // invariant wins over the delta's expecting-close sabotage.
        let interior = session.group_interior_state(&driver, &base, &math_rule).unwrap();
        assert_eq!(interior.mode(), TestMode::Math);
        assert!(interior
            .rules()
            .expecting_group_close()
            .is_some_and(|expected| Arc::ptr_eq(expected, &math_rule)));
        assert_eq!(driver.hook_runs.load(Ordering::Relaxed), 1);

        // Memo hit: the same interior Arc, the hook NOT re-run (miss-only contract),
        // the transition observed on the hit too.
        let again = session.group_interior_state(&driver, &base, &math_rule).unwrap();
        assert!(Arc::ptr_eq(&interior, &again));
        assert_eq!(driver.hook_runs.load(Ordering::Relaxed), 1);
        assert_eq!(session.ext.transitions, 2);

        // A class the hook declines keeps the canonical descent (and its own entry).
        let plain = session.group_interior_state(&driver, &base, &brace).unwrap();
        assert_eq!(plain.mode(), TestMode::Text);
        assert_eq!(driver.hook_runs.load(Ordering::Relaxed), 2);

        // No cross-contamination with the general memo: a hand-built expecting-close
        // delta through derived_state yields the *undecorated* interior — the two
        // maps key different operations.
        let hand_built = ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(Some(Arc::clone(&math_rule))),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        });
        let undriven = session.derived_state(&driver, &base, &hand_built).unwrap();
        assert_eq!(undriven.mode(), TestMode::Text);
        assert!(!Arc::ptr_eq(&undriven, &interior));
    }

    #[test]
    fn custom_driver_recover_policy_replaces_strict_tolerant() {
        // The policy space beyond the enum: a driver whose recover suppresses every
        // condition — nothing recorded, nothing aborted.
        #[derive(Debug, Clone, Copy)]
        struct QuietLang;
        impl Lang for QuietLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = QuietDriver;
            fn make_node_ext(
                _kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) {
            }
        }

        #[derive(Debug, Clone, Copy)]
        struct QuietDriver;
        impl ParseDriver<QuietLang> for QuietDriver {
            fn recover(
                &self,
                _session: &mut ParserSession<QuietLang>,
                _state: &ParsingState<QuietLang>,
                _data: Box<dyn DiagnosticData>,
                _span: SourceSpan<Option<String>>,
            ) -> Result<(), ParseError> {
                Ok(())
            }
        }

        let source: Arc<Source> = Arc::new(Source::new("x"));
        let st: Arc<ParsingState<QuietLang>> = state();
        let mut reader: TokenListReader<'static, QuietLang> = TokenListReader::new(vec![]);
        let mut session = ParserSession::new();
        let driver = QuietDriver;
        let mut cx = ParseContext::new(&mut reader, source.clone(), st, &mut session, &driver);

        // The funnel consults the driver: the condition vanishes — no diagnostic, no
        // abort — under a policy neither Strict nor Tolerant could express.
        assert!(cx.recover(TestUnresolvable { name: "gone".into() }, span(&source, 0..1)).is_ok());
        assert!(session.diagnostics.is_empty());
    }

    #[test]
    fn preset_helper_methods_are_reachable_through_the_typed_driver_field() {
        // The zero-downcast access pattern: `ParseContext.driver` is `&L::Driver`,
        // concretely typed, so inherent methods (the preset-helper surface — a future
        // `LatexlikeDriver::load_package`) are directly callable inside parsers.
        #[derive(Debug, Clone, Copy)]
        struct HelperLang;
        impl Lang for HelperLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = HelperDriver;
            fn make_node_ext(
                _kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) {
            }
        }

        #[derive(Debug, Clone, Copy)]
        struct HelperDriver {
            answer: u32,
        }
        impl ParseDriver<HelperLang> for HelperDriver {}
        impl HelperDriver {
            /// An inherent helper — not on the `ParseDriver` trait.
            fn preset_helper(&self) -> u32 {
                self.answer
            }
        }

        let source: Arc<Source> = Arc::new(Source::new(""));
        let st: Arc<ParsingState<HelperLang>> = state();
        let mut reader: TokenListReader<'static, HelperLang> = TokenListReader::new(vec![]);
        let mut session = ParserSession::new();
        let driver = HelperDriver { answer: 42 };
        let cx = ParseContext::new(&mut reader, source, st, &mut session, &driver);
        assert_eq!(cx.driver.preset_helper(), 42);
    }

    #[test]
    fn session_ext_is_default_initialized() {
        let session: ParserSession<ObserverLang> = ParserSession::new();
        assert_eq!(session.ext.transitions, 0);
    }

    // --- scope-op fallibility at the session seam (7.3) --------------------------------

    #[test]
    fn scope_op_deltas_derive_fresh_and_failures_are_neither_cached_nor_observed() {
        use crate::scopes::{Package, ScopeOp};

        let base: Arc<ParsingState<ObserverLang>> = state();
        let mut session: ParserSession<ObserverLang> = ParserSession::new();

        // A delta carrying scope ops is never memoized: two identical derivations are
        // distinct states (the ops-carrying gate, extending the ext/events rule)…
        let push = || {
            ParsingStateDelta::new()
                .push_provider(Arc::new(Package::<ObserverLang>::new("p")))
        };
        let first = session.derived_state(&ObserverDriver, &base, &push()).unwrap();
        let second = session.derived_state(&ObserverDriver, &base, &push()).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(session.ext.transitions, 2);

        // …and a failing delta returns the mechanical DeriveError with nothing
        // committed: no memo entry, no observation.
        let failing = ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "absent".into() });
        let error = session.derived_state(&ObserverDriver, &base, &failing).unwrap_err();
        assert_eq!(error.failures.len(), 1);
        assert_eq!(session.ext.transitions, 2);
        let error = session.derived_state(&ObserverDriver, &base, &failing).unwrap_err();
        assert_eq!(error.failures.len(), 1); // fails identically — nothing was cached
        assert_eq!(session.ext.transitions, 2);
    }

    /// A driver whose descent delta carries a *failing* scope op for group class 9 —
    /// the group-interior failure path.
    #[derive(Debug, Clone, Copy, Default)]
    struct FailingDescentDriver;

    #[derive(Debug, Clone, Copy)]
    struct FailingDescentLang;
    impl Lang for FailingDescentLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = Observed;
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = FailingDescentDriver;
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) {
        }
    }

    impl ParseDriver<FailingDescentLang> for FailingDescentDriver {
        fn observe_transition(
            &self,
            ext: &mut Observed,
            _prev: &ParsingState<FailingDescentLang>,
            _new: &ParsingState<FailingDescentLang>,
            _delta: &ParsingStateDelta<FailingDescentLang>,
        ) {
            ext.transitions += 1;
        }

        fn group_interior_delta(
            &self,
            _base: &ParsingState<FailingDescentLang>,
            rule: &Arc<GroupRule<FailingDescentLang>>,
        ) -> Option<ParsingStateDelta<FailingDescentLang>> {
            use crate::scopes::ScopeOp;
            (rule.group_type == 9).then(|| {
                ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "absent".into() })
            })
        }
    }

    #[test]
    fn group_interior_failures_repeat_uncached_and_the_recovered_state_expects_the_close() {
        let base: Arc<ParsingState<FailingDescentLang>> = state();
        let bad: Arc<GroupRule<FailingDescentLang>> =
            Arc::new(GroupRule { group_type: 9, open: "$".into(), close: "$".into() });
        let good: Arc<GroupRule<FailingDescentLang>> =
            Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let mut session: ParserSession<FailingDescentLang> = ParserSession::new();

        let error = session.group_interior_state(&FailingDescentDriver, &base, &bad).unwrap_err();
        // The recovered interior still satisfies the descent invariant (the forced
        // expecting-close is an override, not an op) — safe to parse under.
        assert!(error
            .recovered
            .rules()
            .expecting_group_close()
            .is_some_and(|expected| Arc::ptr_eq(expected, &bad)));
        // The carried delta is the *merged* descent delta (op + forced close).
        assert_eq!(error.delta.scope_ops.len(), 1);
        assert!(error.delta.rules.groups.expecting_close.is_some());
        assert_eq!(session.ext.transitions, 0);

        // Failures are not cached: the identical descent fails afresh…
        assert!(session.group_interior_state(&FailingDescentDriver, &base, &bad).is_err());
        assert_eq!(session.ext.transitions, 0);

        // …while an unaffected class memoizes as usual.
        let ok_a = session.group_interior_state(&FailingDescentDriver, &base, &good).unwrap();
        let ok_b = session.group_interior_state(&FailingDescentDriver, &base, &good).unwrap();
        assert!(Arc::ptr_eq(&ok_a, &ok_b));
        assert_eq!(session.ext.transitions, 2);
    }

    // --- the enclosing-state stack + context-dependent event lowering (E4) -------------

    /// A lang with both event classes: `NeedsContext` is context-dependent (the
    /// driver lowers it; finalize refuses it loudly), `Plain` is context-free
    /// (finalize counts it into the state ext).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CtxEvent {
        NeedsContext,
        Plain,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    struct CtxSeen {
        plain_events: usize,
    }

    #[derive(Debug, Clone, Copy)]
    struct CtxLang;
    impl Lang for CtxLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = CtxSeen;
        type Event = CtxEvent;
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = CtxDriver;

        fn finalize_transition(
            new: &mut StateData<CtxLang>,
            _prev: &ParsingState<CtxLang>,
            events: &[CtxEvent],
        ) -> Result<(), crate::state::FinalizeError> {
            for event in events {
                match event {
                    // The two-class contract's loud arm: a context-requiring event
                    // reaching the bare choke point is refused, never dropped.
                    CtxEvent::NeedsContext => {
                        return Err(crate::state::FinalizeError::new(
                            "the NeedsContext event requires the enclosing-state \
                             stack — derive through a parse context",
                        ));
                    }
                    CtxEvent::Plain => new.ext.plain_events += 1,
                }
            }
            Ok(())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) {
        }
    }

    /// Lowers `NeedsContext` into a forbidden-chars patch encoding the lent
    /// stack's depth (proving the hook saw the live stack); `lower: false`
    /// simulates a mis-wired driver that fails to lower.
    #[derive(Debug, Clone, Copy)]
    struct CtxDriver {
        lower: bool,
    }
    impl ParseDriver<CtxLang> for CtxDriver {
        fn recovery(&self) -> Recovery {
            Recovery::Tolerant
        }
        fn resolve_state_event(
            &self,
            event: &CtxEvent,
            stack: &crate::state::ParsingStateStack<CtxLang>,
        ) -> Option<ParsingStateDelta<CtxLang>> {
            if !self.lower || *event != CtxEvent::NeedsContext {
                return None;
            }
            // A context-derived patch: depth many 'd's (any stack-derived fact
            // observable from the derived state works).
            let depth_marker: String = core::iter::repeat('d').take(stack.len()).collect();
            Some(ParsingStateDelta::new().rules(
                crate::state::TokenRulesOverrides {
                    forbidden_chars: crate::state::ForbiddenCharsOverrides {
                        chars: Some(depth_marker.into()),
                    },
                    ..Default::default()
                },
            ))
        }
    }

    fn ctx_context<'a, 's>(
        reader: &'a mut crate::token::TokenListReader<'s, CtxLang>,
        source: &Arc<Source>,
        state: Arc<ParsingState<CtxLang>>,
        session: &'a mut ParserSession<CtxLang>,
        driver: &'a CtxDriver,
    ) -> ParseContext<'a, 's, CtxLang> {
        ParseContext::new(reader, Arc::clone(source), state, session, driver)
    }

    #[test]
    fn with_parsing_state_maintains_the_session_stack() {
        let source: Arc<Source> = Arc::new(Source::new(""));
        let outer: Arc<ParsingState<CtxLang>> = Arc::new(ParsingState::lang_initial());
        let inner = Arc::new(outer.derived(&ParsingStateDelta::new()).unwrap());
        let mut reader = crate::token::TokenListReader::new(alloc::vec![]);
        let mut session = ParserSession::new();
        let driver = CtxDriver { lower: true };
        let mut cx =
            ctx_context(&mut reader, &source, Arc::clone(&outer), &mut session, &driver);

        assert_eq!(cx.session.state_stack().len(), 0);
        cx.with_parsing_state(Arc::clone(&outer), |cx| {
            assert_eq!(cx.session.state_stack().len(), 1);
            cx.with_parsing_state(Arc::clone(&inner), |cx| {
                assert_eq!(cx.session.state_stack().len(), 2);
                // Innermost-first, current state first.
                let innermost = cx.session.state_stack().iter().next().unwrap();
                assert!(Arc::ptr_eq(innermost, &inner));
            });
            assert_eq!(cx.session.state_stack().len(), 1);
            // The scoped state was restored along with the stack.
            assert!(Arc::ptr_eq(&cx.state, &outer));
        });
        assert_eq!(session.state_stack().len(), 0, "zero residue after the scope");
    }

    #[test]
    fn derive_state_lowers_context_dependent_events_through_the_stack() {
        let source: Arc<Source> = Arc::new(Source::new(""));
        let base: Arc<ParsingState<CtxLang>> = Arc::new(ParsingState::lang_initial());
        let mut reader = crate::token::TokenListReader::new(alloc::vec![]);
        let mut session = ParserSession::new();
        let driver = CtxDriver { lower: true };
        let mut cx =
            ctx_context(&mut reader, &source, Arc::clone(&base), &mut session, &driver);

        let delta = ParsingStateDelta::new().event(CtxEvent::NeedsContext);
        // Outside any scope: the lend pushes the current state (depth 1).
        let derived = cx.derive_state(&delta).unwrap();
        assert_eq!(derived.rules().forbidden_chars(), "d");
        // The event was removed before finalize (no refusal, no Plain count).
        assert_eq!(derived.ext().plain_events, 0);

        // Inside a scope whose innermost entry IS the current state: no
        // duplicate push — the hook sees exactly the scoped depth.
        let scoped = cx.with_parsing_state(Arc::clone(&base), |cx| {
            cx.with_parsing_state(Arc::clone(&base), |cx| cx.derive_state(&delta))
        });
        assert_eq!(scoped.unwrap().rules().forbidden_chars(), "dd");

        // A current state that evolved past the innermost entry is lent on top.
        let evolved = Arc::new(base.derived(&ParsingStateDelta::new()).unwrap());
        let lent = cx.with_parsing_state(Arc::clone(&base), |cx| {
            cx.state = Arc::clone(&evolved); // a sibling after-effect
            cx.derive_state(&delta)
        });
        assert_eq!(lent.unwrap().rules().forbidden_chars(), "dd");
        assert_eq!(session.state_stack().len(), 0);
    }

    #[test]
    fn derive_state_keeps_context_free_events_for_finalize_and_the_author_wins() {
        let source: Arc<Source> = Arc::new(Source::new(""));
        let base: Arc<ParsingState<CtxLang>> = Arc::new(ParsingState::lang_initial());
        let mut reader = crate::token::TokenListReader::new(alloc::vec![]);
        let mut session = ParserSession::new();
        let driver = CtxDriver { lower: true };
        let mut cx =
            ctx_context(&mut reader, &source, Arc::clone(&base), &mut session, &driver);

        // A context-free event passes through to finalize untouched…
        let plain = cx.derive_state(&ParsingStateDelta::new().event(CtxEvent::Plain)).unwrap();
        assert_eq!(plain.ext().plain_events, 1);

        // …mixed deltas lower only the context-dependent one…
        let mixed = cx
            .derive_state(
                &ParsingStateDelta::new()
                    .event(CtxEvent::Plain)
                    .event(CtxEvent::NeedsContext),
            )
            .unwrap();
        assert_eq!(mixed.ext().plain_events, 1);
        assert_eq!(mixed.rules().forbidden_chars(), "d");

        // …and the delta's own explicit override wins over the event's patch
        // (the delta author spoke).
        let authored = cx
            .derive_state(
                &ParsingStateDelta::new().event(CtxEvent::NeedsContext).rules(
                    crate::state::TokenRulesOverrides {
                        forbidden_chars: crate::state::ForbiddenCharsOverrides {
                            chars: Some("A".into()),
                        },
                        ..Default::default()
                    },
                ),
            )
            .unwrap();
        assert_eq!(authored.rules().forbidden_chars(), "A");
    }

    #[test]
    fn context_dependent_event_in_bare_derived_errors_loudly() {
        // Out of any parse, the two-class contract's loud arm: finalize refuses,
        // derived() folds the refusal into the DeriveError.
        let base: Arc<ParsingState<CtxLang>> = Arc::new(ParsingState::lang_initial());
        let error = base
            .derived(&ParsingStateDelta::new().event(CtxEvent::NeedsContext))
            .unwrap_err();
        assert!(error.failures.is_empty());
        let finalize_error = error.finalize_error.as_ref().unwrap();
        assert!(finalize_error.message().contains("NeedsContext"));
        assert!(error.to_string().contains("transition refused"));
    }

    #[test]
    fn unlowered_context_event_in_parse_aborts_as_implementation_error() {
        // A driver that fails to lower a context-requiring event is extension
        // wiring gone wrong: derive_state aborts under ANY recovery policy
        // (CtxDriver reports Tolerant).
        let source: Arc<Source> = Arc::new(Source::new(""));
        let base: Arc<ParsingState<CtxLang>> = Arc::new(ParsingState::lang_initial());
        let mut reader = crate::token::TokenListReader::new(alloc::vec![]);
        let mut session = ParserSession::new();
        let driver = CtxDriver { lower: false };
        let mut cx =
            ctx_context(&mut reader, &source, Arc::clone(&base), &mut session, &driver);

        let error = cx
            .derive_state(&ParsingStateDelta::new().event(CtxEvent::NeedsContext))
            .unwrap_err();
        use crate::error::DiagnosticInfo as _;
        assert_eq!(
            error.identifier(),
            crate::constructs::ImplementationError::IDENTIFIER
        );
        assert!(error.to_string().contains("NeedsContext"));
        assert!(session.diagnostics.is_empty(), "not a tolerant diagnostic");
    }
}
