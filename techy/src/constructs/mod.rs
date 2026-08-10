//! Construct parsers: the parsing layer of the S1 core.
//!
//! [`ConstructParser`] is the single most important trait in the system: every construct —
//! the main content loop ([`NodesParser`]), groups ([`GroupParser`]), callable invocations
//! ([`StdInvocationParser`], behind the
//! [`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser) factory),
//! arguments (the standard [`ArgumentParser`](crate::spec::ArgumentParser)
//! implementations), environment bodies ([`EnvironmentBodyParser`]) — is parsed by an
//! implementation of it, reading tokens and staging nodes through one [`ParseContext`].
//!
//! # The two-tier ownership model
//!
//! Construct parsers are **temporaries** (tier 2): constructed with their per-use
//! configuration where they are needed, `parse(&mut self, …)` so working state lives in
//! fields, free to borrow (`'s` content, token refs), dropped when the frame ends —
//! never stored in specs. *Stored* behavior objects (tier 1 — specs,
//! [`ArgumentParser`](crate::spec::ArgumentParser)s) are `Arc`-shared, `Send + Sync`,
//! immutable, and receive every per-use input as arguments. Closures (stop predicates)
//! are thereby confined to tier 2; specs stay data.
//!
//! # State threading (the "caller applies deltas" law, pinned to `cx`)
//!
//! [`ParseContext::state`] is the parser's **input** state — the caller sets it. A parser
//! that scopes a child state (group interior, argument extent, slot body) derives it
//! locally and scopes it through
//! [`parse_construct`](ParseContext::parse_construct) /
//! [`with_parsing_state`](ParseContext::with_parsing_state) (structural revert —
//! `Arc` clone is
//! cheap — plus the session's enclosing-state stack bookkeeping). The
//! `Option<ParsingStateDelta>` in the
//! return value is exclusively the *after-effect for the caller* (`\newcommand`).
//!
//! # Errors
//!
//! `Err` means **abort**: recovery happens at the detection site (the
//! [`recover`](ParseContext::recover) helper), and abnormal endings of sub-parses travel
//! as data ([`StopCause`]) — nobody continues past an `Err`.

mod argument_parsers;
mod attached_source;
mod chars_group_parser;
mod child_state;
mod embellishments_parser;
mod environment_parser;
mod group_parser;
mod invocation_parser;
mod nodes_parser;
mod tack_on_parser;
mod verbatim_parser;

pub use argument_parsers::{
    scan_argument_noise, stage_pre_space, ArgumentNoise, ExpectedExpressionArgument,
    ExpressionParser, GroupArgumentParser, MarkerArgumentParser, MissingMandatoryArgument,
    OptionalGroupArgumentParser,
};
pub use attached_source::{AttachedSourceOutcome, NoSourceResolver, UnresolvableSourceReference};
pub use chars_group_parser::CharsGroupArgumentParser;
pub use child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
pub use embellishments_parser::EmbellishmentsArgumentParser;
pub use tack_on_parser::{RepeatedTackOnField, TackOnFieldsArgumentParser};
pub use environment_parser::{
    read_rigid_name_group, EnvironmentBeginSyntaxData, EnvironmentBody,
    EnvironmentBodyParser, EnvironmentTerminatorMismatch,
    EnvironmentTerminatorSyntaxData, MalformedEnvironmentTerminator,
    MissingEnvironmentTerminator, MissingTerminatorFound, NameGroup,
};
pub use group_parser::{GroupParser, UnclosedGroup, UnclosedGroupFound};
pub use invocation_parser::{parse_declared_arguments, StdInvocationParser};
pub use nodes_parser::{
    CommandResolutionFailed, ExpressionCallableRequiresContent, NodesOutcome, NodesParser,
    StopCause, StopSpec, StrayGroupClose, TokenStopCondition, TokenStopKind,
    UnresolvableCommand, UnusableRecoveryToken, UnusableRecoveryTokenKind,
};
pub use verbatim_parser::{
    verbatim_state_delta, ExpectedVerbatimDelimiter, UnterminatedVerbatim,
    VerbatimArgumentParser, VerbatimBodyParser,
};

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::sync::Arc;
use core::fmt;

use crate::engine::{Frame, FrameTitle, ParseDriver, ParserSession};
use crate::error::{DiagnosticData, DiagnosticInfo, ParseError};
use crate::source::{Source, SourceSpan, Span};
use crate::spec::{CallableSpec, FrameRole};
use crate::node::{
    BuildId, CallableData, NodeBuildError, NodeKind, ParsedArguments, ParsedSlots,
    StagedNodes,
};
use crate::state::{FeaturePresence, Lang, LangFeatures, ParsingState, ParsingStateDelta};
use crate::token::{GroupRule, Token, TokenReader};

/// The live frame covering a resolved invocation's parse (the dispatch push site): the spec's title hook with the invocation spelling — the
/// trigger token minus its syntactic post-space — anchored at the trigger. Built before
/// the `Invocation` moves into the spec's parser factory; allocation-free (`Arc` bumps
/// only).
pub(crate) fn invocation_frame<L: Lang>(
    cx: &ParseContext<'_, '_, L>,
    invocation: &Invocation<'_, '_, L>,
) -> Frame<L> {
    let token = invocation.token;
    let name_span = Span::new(token.span.start(), token.post_space().start());
    Frame {
        title: FrameTitle::Callable {
            spec: Arc::clone(invocation.spec),
            role: FrameRole::Invocation,
            name: SourceSpan::new(&cx.source, name_span),
        },
        span: SourceSpan::new(&cx.source, token.span),
    }
}

/// Everything a construct parser needs, in one context value.  Includes pretty much
/// all methods the construct parser might need, including to stage nodes, to delegate
/// parsing to sub-parsers, pushing frames on the frames stack, to change the parsing
/// state, etc.
pub struct ParseContext<'a, 's, L: Lang> {
    /// The token stream.
    pub tokens: &'a mut dyn TokenReader<'s, L>,
    /// The source the token spans refer into — what staging a node's
    /// [`SourceSpan`] requires. It lives
    /// here, not on tokens or readers, because the token layer deliberately carries
    /// only transient byte spans; the construct-parser layer
    /// is where byte spans become `Arc`-backed source spans. Not a parsing input:
    /// construct parsers make no forward parsing decision from raw content — even a
    /// verbatim parser reads `Char` tokens under a features-disabled state.
    pub source: Arc<Source<L::SourceOrigin>>,
    /// The parser's **input** parsing state (the caller sets it; see the
    /// state-threading contract in [`core::constructs`](crate::core::constructs)).
    pub state: Arc<ParsingState<L>>,
    /// The session: node building, diagnostics, derivation memos, frames.
    pub session: &'a mut ParserSession<L>,
    /// The language's [`ParseDriver`]: recovery policy, parse-time hooks,
    /// the descent-delta channel, construct provision. **Concretely typed through
    /// `L`** — preset parsers reach preset helper methods (inherent methods on the
    /// driver type) with zero downcasts; generic code sees only the trait.
    pub driver: &'a L::Driver,
}

impl<'a, 's, L: Lang> ParseContext<'a, 's, L> {
    /// Bundle the five parse inputs into a context. Prefer this over a struct literal
    /// (the fields stay public for access): the context is the type's stated "one place
    /// to grow" (depth limits, cancellation), and construction through `new` keeps
    /// future fields from breaking every embedder.
    pub fn new(
        tokens: &'a mut dyn TokenReader<'s, L>,
        source: Arc<Source<L::SourceOrigin>>,
        state: Arc<ParsingState<L>>,
        session: &'a mut ParserSession<L>,
        driver: &'a L::Driver,
    ) -> ParseContext<'a, 's, L> {
        ParseContext { tokens, source, state, session, driver }
    }

    /// Stage one parsed node — **the single staging entry point** of parsing (every
    /// parsed node enters the tree through it), and the one automatic
    /// [`Lang::make_node_ext`] site: the node's ext is minted here (with the
    /// descent-only [`StagedChildren`](crate::node::StagedChildren) view of
    /// `children`), then the node is staged with annotation `()` (parse output is the
    /// unannotated `NodeTree<L>`; annotations are consumer vocabulary).
    ///
    /// Construct parsers have no other route to the builder — no node escapes ext
    /// minting, with zero parser cooperation required. `children` are the node's
    /// structural children in order (for a `Callable`: one region per provided
    /// argument, then one per slot — see
    /// [`NodeTreeBuilder::add`](crate::node::NodeTreeBuilder::add)).
    ///
    /// `Err` reports a staging-contract violation
    /// ([`NodeBuildError`](crate::node::NodeBuildError)) — an implementation bug in an
    /// extension, not a source condition; lift it with
    /// [`implementation_error`](ParseContext::implementation_error).
    pub fn stage_node(
        &mut self,
        kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
        state: Arc<ParsingState<L>>,
        children: Vec<BuildId>,
    ) -> Result<BuildId, NodeBuildError> {
        let ext = L::make_node_ext(
            &kind,
            &span,
            &state,
            self.session.builder.staged_children(&children),
        );
        self.session.builder.add(kind, span, state, children, ext, ())
    }

    /// The read-only view of the nodes staged so far, keyed by
    /// [`BuildId`](crate::node::BuildId) — what node-based stop predicates and span
    /// arithmetic over already-staged children consume. Read view only: staging goes
    /// through [`stage_node`](ParseContext::stage_node).
    pub fn staged_nodes(&self) -> StagedNodes<'_, L> {
        self.session.builder.staged_nodes()
    }

    /// Stage the resolved invocation's `Callable` node — the **transcription-case
    /// shorthand** over the single staging entry point ([`stage_node`](ParseContext::stage_node)):
    /// builds the [`CallableData`] by transcribing `callable_type`/`name`/`spec`
    /// from the bundle and minting the invocation-syntax payload from it
    /// ([`FromInvocation`]), computes the node's span, stages (minting the node
    /// ext, as staging always does), and returns the id. What [`StdInvocationParser`] does, packaged
    /// for takeover parsers of the same macro shape.
    ///
    /// `arguments`/`slots` are **caller-tiled** records in staged child-list
    /// coordinates, and `children` the flat child list they tile — the natural
    /// output of the argument loop
    /// ([`parse_declared_arguments`]). The parse-side/restage-side symmetry with
    /// `restage_invocation` is by shared *vocabulary*, deliberately not shared
    /// arity: the restage side passes driver-tiled bundles because the region
    /// arithmetic is owned by the other party there.
    ///
    /// `end_pos: None` = the **standard rule**: the node's span runs from the
    /// trigger's start to the last staged child's span end — or the trigger's own
    /// end for childless shapes. `Some(end)` serves takeovers whose consumed
    /// extent outruns their last child (rest-of-line and heredoc shapes).
    ///
    /// Deliberately **no `callable_type`/`name` overrides**: a composition that
    /// overrides both and whose span outruns its children (the environment shape)
    /// stays on [`stage_node`](ParseContext::stage_node) itself with
    /// an explicit [`CallableData`]. No ext/annotation parameters — staging
    /// mints the ext, and parse annotations are `()`.
    pub fn stage_invocation(
        &mut self,
        invocation: &Invocation<'_, '_, L>,
        arguments: ParsedArguments<L>,
        slots: ParsedSlots<L>,
        children: Vec<BuildId>,
        end_pos: Option<usize>,
    ) -> ConstructParserResult<L, BuildId>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        let token = invocation.token;
        // The std end rule: last staged child's span end, else the trigger's end.
        // A last child no parser ever staged (an implementation bug) falls back to
        // the trigger's end — the builder diagnoses the foreign id in `add`.
        let end = end_pos.unwrap_or_else(|| {
            children
                .last()
                .and_then(|last| self.staged_nodes().get(*last))
                .map(|child| child.span().end())
                .unwrap_or(token.span.end())
        });
        let data = CallableData {
            callable_type: invocation.callable_type,
            name: invocation.name.into(),
            spec: Arc::clone(invocation.spec),
            arguments,
            slots,
            invocation_syntax: L::InvocationSyntax::from_invocation(invocation),
        };
        self.stage_node(
            NodeKind::callable(data),
            SourceSpan::new(&self.source, token.span.start()..end),
            Arc::clone(&self.state),
            children,
        )
        .map_err(|error| self.implementation_error(error, Span::new(token.span.start(), end)))
    }

    /// Probe the token at the current position under `state`, mapping a tokenizer error
    /// per the recovery policy — the **probing peek** of the argument-probe protocol:
    /// strict mode aborts with the token error (mirroring the content loop); tolerant
    /// mode reports `None` **without diagnosing or consuming** — the caller treats the
    /// position as unusable (argument absent, terminator malformed) and the enclosing
    /// content loop re-reads the error and applies its own token recovery, avoiding a
    /// double report.
    ///
    /// A token error carrying **no** recovery is unrecoverable and aborts even under
    /// [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) — mirroring the content loop, whose re-read would abort
    /// anyway; reporting `None` first would only add a spurious absent-position recovery
    /// (and its diagnostic) on the way down.
    ///
    /// `state` is passed explicitly (usually `&Arc::clone(&cx.state)`; peeking never
    /// consumes) so a parser can probe under a derived state — an optional-argument
    /// parser peeking with its minted group rule in force — without swapping
    /// [`state`](ParseContext::state).
    ///
    /// Thin sugar over [`ParseDriver::probe_token`], where the policy is defined.
    pub fn probe_token(
        &mut self,
        state: &Arc<ParsingState<L>>,
    ) -> ConstructParserResult<L, Option<Token<'s, L>>> {
        self.driver.probe_token(self.tokens, &self.source, self.session, state)
    }

    /// Run `parser` as one **sub-parse** — the single entry point that every descent
    /// (a construct parser running another [`ConstructParser`] over the same input)
    /// MUST go through; the pylatexenc
    /// `walker.parse_content(parser, …, parsing_state)` analog.
    ///
    /// The contract is **normative**: a `ConstructParser` runs only through this
    /// method — called directly, or through the thin wrappers
    /// [`parse_nodes`](ParseContext::parse_nodes) and
    /// [`parse_group`](ParseContext::parse_group), which delegate here. One shared
    /// entry point is what lets the engine attach its per-descent bookkeeping (the
    /// enclosing-state stack, the optional traceback frame) uniformly, with no
    /// per-site cooperation. The contract's limit: plain Rust recursion that
    /// bypasses it — code calling another parser's
    /// [`parse`](ConstructParser::parse) method directly — cannot be detected by
    /// the library. The rule is documented, not enforceable.
    ///
    /// `state` is the sub-parse's input state, scoped structurally for the duration
    /// of the run — swapped in, restored afterwards, with the session's
    /// enclosing-state stack maintained alongside (the
    /// [`with_parsing_state`](ParseContext::with_parsing_state) discipline):
    ///
    /// - `Some(state)`: the sub-parse runs under `state`.
    /// - `None`: the sub-parse runs under the **current** state — exactly as if
    ///   `Some(Arc::clone(&cx.state))` had been passed: the same swap/restore
    ///   scoping runs and the same enclosing-state stack entry is pushed. `None`
    ///   never means "skip the scoping".
    ///
    /// `frame`, when `Some`, is pushed on the session's live frame stack around the
    /// whole sub-parse (the [`with_frame`](ParseContext::with_frame) discipline):
    /// every condition recorded while `parser` runs carries the frame in its
    /// traceback snapshot. The frame is popped before this method returns, on the
    /// `Ok` and `Err` paths alike (errors are values, not unwinds).
    ///
    /// The returned [`ParsingStateDelta`] is the construct's after-effect for the
    /// caller, passed through **unapplied** — whether and where it applies is
    /// caller business (the "caller applies deltas" law).
    pub fn parse_construct<P>(
        &mut self,
        parser: &mut P,
        state: Option<Arc<ParsingState<L>>>,
        frame: Option<Frame<L>>,
    ) -> ConstructParserResult<L, (P::Output, Option<ParsingStateDelta<L>>)>
    where
        P: ConstructParser<L> + ?Sized,
    {
        // `None` = the current state; the scoping below runs identically either way.
        let state = match state {
            Some(state) => state,
            None => Arc::clone(&self.state),
        };
        let framed = frame.is_some();
        if let Some(frame) = frame {
            self.session.push_frame(frame);
        }
        // descent-guard slot (Part 2)
        let result = self.with_parsing_state(state, |cx| parser.parse(cx));
        // The pop covers the `Err` path too — errors are values, not unwinds.
        if framed {
            self.session.pop_frame();
        }
        result
    }

    /// Run `f` with [`state`](ParseContext::state) scoped to `state`, restoring the
    /// outer state afterwards — the closure-shaped scoped-state primitive under
    /// [`parse_construct`](ParseContext::parse_construct), for state scopes that
    /// are not `ConstructParser`-shaped (the per-argument delta around
    /// `ArgumentParser::parse_argument`; takeover parsers scoping hand-derived
    /// states around arbitrary code).
    ///
    /// This is a **state-scoping utility, not a descent entry point**: code that
    /// runs another [`ConstructParser`] must go through
    /// [`parse_construct`](ParseContext::parse_construct) (which performs this
    /// scoping itself, plus its per-descent obligations), never through this
    /// method alone.
    ///
    /// Besides the structural swap/restore, this maintains the session's
    /// **enclosing-state stack** ([`ParsingStateStack`](crate::state::ParsingStateStack)):
    /// `state` is pushed for the duration of `f` and popped after — the same
    /// closure-scoped discipline as [`with_frame`](ParseContext::with_frame), at
    /// the same descent points. Prefer this over hand-rolled `cx.state` swaps: a
    /// manual swap leaves the stack (and therefore context-dependent event
    /// lowering, [`derive_state`](ParseContext::derive_state)) blind to the scope.
    pub fn with_parsing_state<R>(
        &mut self,
        state: Arc<ParsingState<L>>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.session.push_state(Arc::clone(&state));
        let outer = core::mem::replace(&mut self.state, state);
        let result = f(self);
        self.state = outer;
        self.session.pop_state();
        result
    }

    /// Detection-site recovery — **the recovery entry point**: every problem a
    /// construct parser detects in the source is reported through this one method,
    /// which boxes the condition and hands it to [`ParseDriver::recover`], where the
    /// policy is defined — the default driver path applies
    /// [`refine_diagnostic`](ParseDriver::refine_diagnostic) exactly once (it needs
    /// this context's parsing state) and then records the condition as an
    /// error-severity diagnostic and returns `Ok(())` (tolerant — the caller continues
    /// with its local recovery) or returns it as a [`ParseError`] to bubble (strict).
    pub fn recover(
        &mut self,
        condition: impl DiagnosticInfo,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        self.recover_boxed(Box::new(condition), span)
    }

    /// The recovery entry point's boxed form — for payloads that already live behind the dyn facade
    /// (the token-error lift, where a `Custom` payload must not be double-boxed).
    pub(crate) fn recover_boxed(
        &mut self,
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        self.driver.recover(self.session, &self.state, data, span)
    }

    /// The parser-facing state derivation, from the **current** state
    /// ([`state`](ParseContext::state)) — the one derivation point every construct
    /// parser derives through: lowers **context-dependent events**, then runs the
    /// session-mediated derivation ([`ParserSession::derived_state`] with this
    /// context's driver), so every transition reaches
    /// [`ParseDriver::observe_transition`]. Deriving from another base goes through
    /// the session method directly
    /// (`cx.session.derived_state(cx.driver, &base, &delta)`) — with no event
    /// lowering: context-dependent events are positional, they mean something only
    /// at *this* context's position.
    ///
    /// # Event lowering
    ///
    /// When the delta carries [`events`](crate::state::ParsingStateDelta::events),
    /// each is offered to [`ParseDriver::resolve_state_event`] with the session's
    /// live enclosing-state stack (current state first —
    /// [`ParsingStateStack`](crate::state::ParsingStateStack)). Lowered events are
    /// **removed** and their patches merged; unlowered (context-free) events stay
    /// for [`Lang::finalize_transition`](crate::state::Lang::finalize_transition).
    /// The event *loop* lives here — parsers never iterate events; per-event
    /// *policy* lives on the driver. Merge order: patches apply in event order,
    /// and the delta's own explicit overrides win over patch overrides (the delta
    /// author spoke).
    ///
    /// # Failures
    ///
    /// **Failing scope ops** are reported through the recovery entry point as
    /// [`ScopeOpFailed`] conditions at the current position: under
    /// [`Recovery::Strict`](crate::error::Recovery::Strict) the first failure aborts;
    /// under [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) each failure is
    /// recorded and the parse continues under the error's
    /// [`recovered`](crate::state::DeriveError::recovered) state (the failing ops
    /// skipped, everything else applied), with the transition observed as usual.
    /// A **finalize refusal** ([`FinalizeError`](crate::state::FinalizeError) — a
    /// context-requiring event the driver did not lower) is an extension wiring
    /// bug, not a source condition: it aborts as an
    /// [`ImplementationError`] under any recovery policy.
    pub fn derive_state(
        &mut self,
        delta: &ParsingStateDelta<L>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let base = Arc::clone(&self.state);
        let effective;
        let delta = if delta.events.is_empty() {
            delta
        } else {
            effective = self.lower_state_events(delta);
            &effective
        };
        self.commit_derivation(&base, delta)
    }

    /// [`derive_state`](ParseContext::derive_state) with the **effective, as-applied
    /// delta recorded**: after the transition commits (cleanly or through the
    /// tolerant ops-skipped recovery), the delta actually handed to the derivation —
    /// context-dependent events already lowered into their override patches — is
    /// merged into `record` ([`ParsingStateDelta::merge_from`]: later field
    /// overrides win, scope ops and events concatenate in application order). This
    /// is where the merged after-effect record
    /// ([`NodesOutcome::after_effects`]) is captured.
    ///
    /// Recording the *effective* delta is essential: context-dependent events
    /// are positional (they mean something only at this context's position), so a
    /// record replayed elsewhere must carry their lowered patches, never the raw
    /// events. Context-free events that survive lowering are recorded as-is — by
    /// the [`Lang::Event`](crate::state::Lang::Event) contract they are consumed
    /// wherever the delta is applied, so they replay exactly. On the tolerant
    /// ops-skipped path the record keeps the delta **as applied** (the
    /// [`DeriveError::delta`](crate::state::DeriveError::delta) notion), failing
    /// ops included: a replay elsewhere re-attempts them and may re-diagnose.
    pub(crate) fn derive_state_recording(
        &mut self,
        delta: &ParsingStateDelta<L>,
        record: &mut Option<ParsingStateDelta<L>>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let base = Arc::clone(&self.state);
        let effective;
        let applied = if delta.events.is_empty() {
            delta
        } else {
            effective = self.lower_state_events(delta);
            &effective
        };
        let new = self.commit_derivation(&base, applied)?;
        match record {
            Some(record) => record.merge_from(applied.clone()),
            None => *record = Some(applied.clone()),
        }
        Ok(new)
    }

    /// The shared commit tail of the derivation seams: run the session-mediated
    /// derivation (so the transition reaches
    /// [`ParseDriver::observe_transition`]) and route failures through
    /// [`recover_derive_failure`](ParseContext::recover_derive_failure).
    fn commit_derivation(
        &mut self,
        base: &Arc<ParsingState<L>>,
        applied: &ParsingStateDelta<L>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        match self.session.derived_state(self.driver, base, applied) {
            Ok(new) => Ok(new),
            Err(failure) => self.recover_derive_failure(base, failure),
        }
    }

    /// Derive via [`derive_state`](ParseContext::derive_state), then run `f` with
    /// [`state`](ParseContext::state) scoped to the derived state
    /// ([`with_parsing_state`](ParseContext::with_parsing_state)) — the
    /// delta-shaped state scope in one call, for takeover parsers.
    ///
    /// Like `with_parsing_state`, this is a state-scoping utility, **not** a
    /// descent entry point: running another [`ConstructParser`] under the derived
    /// state goes through [`parse_construct`](ParseContext::parse_construct)
    /// instead.
    pub fn with_derived_state<R>(
        &mut self,
        delta: &ParsingStateDelta<L>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> ConstructParserResult<L, R> {
        let state = self.derive_state(delta)?;
        Ok(self.with_parsing_state(state, f))
    }

    /// The event loop of [`derive_state`](ParseContext::derive_state): offer each
    /// event to the driver with the live enclosing-state stack lent **current
    /// state first** (the context's current state is pushed for the lend when it
    /// is not already the innermost entry — sibling after-effects evolve
    /// `cx.state` between descents; an `Arc`-equal duplicate is harmless under
    /// the stack's scan semantics), and build the effective delta: patches merged
    /// in event order, the original delta's explicit overrides on top, lowered
    /// events removed.
    fn lower_state_events(&mut self, delta: &ParsingStateDelta<L>) -> ParsingStateDelta<L> {
        let lent = match self.session.state_stack().innermost() {
            Some(innermost) => !Arc::ptr_eq(innermost, &self.state),
            None => true,
        };
        if lent {
            self.session.push_state(Arc::clone(&self.state));
        }
        let mut patches: Vec<ParsingStateDelta<L>> = Vec::new();
        let mut kept_events: Vec<L::Event> = Vec::new();
        for event in &delta.events {
            match self.driver.resolve_state_event(event, self.session.state_stack()) {
                Some(patch) => patches.push(patch),
                None => kept_events.push(event.clone()),
            }
        }
        if lent {
            self.session.pop_state();
        }

        // Merge: patches in event order (later wins), then the original delta's own
        // explicit overrides on top of all patches (the delta author spoke).
        let mut effective = ParsingStateDelta::new();
        for patch in patches {
            effective.rules.merge_from(patch.rules);
            // Matched store projections: with the scopes feature absent, neither
            // side's zero-sized scope-op store holds anything to concatenate.
            if let (Some(ops), Some(patch_ops)) = (
                <L::Features as LangFeatures>::Scopes::store_get_mut(&mut effective.scope_ops),
                <L::Features as LangFeatures>::Scopes::store_into_inner(patch.scope_ops),
            ) {
                ops.extend(patch_ops);
            }
            if patch.mode.is_some() {
                effective.mode = patch.mode;
            }
            if patch.ext.is_some() {
                effective.ext = patch.ext;
            }
            // Patch events are context-free by contract (resolve_state_event docs):
            // they pass through to finalize_transition un-lowered.
            effective.events.extend(patch.events);
        }
        effective.rules.merge_from(delta.rules.clone());
        if let (Some(ops), Some(delta_ops)) = (
            <L::Features as LangFeatures>::Scopes::store_get_mut(&mut effective.scope_ops),
            <L::Features as LangFeatures>::Scopes::store_get(&delta.scope_ops),
        ) {
            ops.extend(delta_ops.iter().cloned());
        }
        if delta.mode.is_some() {
            effective.mode = delta.mode;
        }
        if delta.ext.is_some() {
            effective.ext = delta.ext.clone();
        }
        effective.events.extend(kept_events);
        effective
    }

    /// The group-interior derivation from the **current** state — sugar over
    /// [`ParserSession::group_interior_state`] supplying this context's driver: the
    /// canonical expecting-close override merged with the driver's
    /// [`group_interior_delta`](ParseDriver::group_interior_delta), memoized per
    /// `(base, rule)`. Failing scope ops in the driver's descent delta recover exactly
    /// like [`derive_state`](ParseContext::derive_state)'s (the recovered interior
    /// still expects the entered rule's close — the descent invariant is an override,
    /// not an op).
    pub fn group_interior_state(
        &mut self,
        rule: &Arc<GroupRule<L>>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let base = Arc::clone(&self.state);
        match self.session.group_interior_state(self.driver, &base, rule) {
            Ok(new) => Ok(new),
            Err(failure) => self.recover_derive_failure(&base, failure),
        }
    }

    /// The shared recovery path of the two fallible derivation sugars: report every
    /// failing op through the recovery entry point (strict: the first one aborts); a
    /// finalize refusal aborts as an [`ImplementationError`] under any policy (a
    /// context-requiring event reached the underlying derivation point un-lowered —
    /// the driver failed to lower it: extension wiring, not source input). Otherwise commit
    /// the ops-skipped transition — continue under the error's recovered state
    /// and observe it with the delta the derivation actually applied (which the
    /// error carries: for group interiors that is the *merged* descent delta this
    /// context never built).
    fn recover_derive_failure(
        &mut self,
        base: &Arc<ParsingState<L>>,
        failure: crate::state::DeriveError<L>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let pos = self.tokens.pos();
        let span = SourceSpan::new(&self.source, Span::new(pos, pos));
        let crate::state::DeriveError {
            failures,
            finalize_error,
            recovered,
            delta,
        } = failure;
        for failed_op in &failures {
            self.recover(ScopeOpFailed::new(failed_op.to_string()), span.clone())?;
        }
        if let Some(finalize_error) = finalize_error {
            return Err(self.implementation_error(finalize_error, Span::new(pos, pos)));
        }
        // Tolerant continuation: commit the recovered transition — the session seam
        // observed nothing on the Err path (no transition had been committed).
        let recovered = Arc::new(recovered);
        self.driver.observe_transition(&mut self.session.ext, base, &recovered, &delta);
        Ok(recovered)
    }

    /// **A thin wrapper over [`parse_construct`](ParseContext::parse_construct)**:
    /// builds the content-loop parser from the driver's
    /// [`make_nodes_parser`](ParseDriver::make_nodes_parser) factory and runs it
    /// through `parse_construct(parser, Some(state), None)`. Fusing the factory
    /// with the descent entry point is the point of this method: routing every
    /// content descent through it makes one driver override apply at every
    /// descent site.
    ///
    /// Parses one **nodes descent** (a content run: group interior, environment
    /// body, top-level drive) under `state`. State scoping and restoration follow
    /// [`parse_construct`](ParseContext::parse_construct).
    ///
    /// # Resuming a stopped run
    ///
    /// A run that stops before end of input — a token condition, an unexpected group
    /// close ([`StopCause`]) — can be **resumed**: handle the stop (diagnose, consume
    /// or skip the offending token), then call `parse_nodes` again. There is no resume
    /// method on [`NodesParser`]; resumption is *re-invocation* — each call builds a
    /// fresh parser from the factory with its own per-run `stop`/`child_states` — and
    /// the caller bridges the runs. The canonical bridge is the root drive loop
    /// ([`Language::parse_source`](crate::engine::Language::parse_source)), which
    /// diagnoses a stray group close, skips it, and re-enters. The bridge has three
    /// obligations:
    ///
    /// - **Resume under [`NodesOutcome::state`], never under this context's restored
    ///   `state`.** The descent restores [`state`](ParseContext::state) structurally on
    ///   return ([`parse_construct`](ParseContext::parse_construct)), and the run's sibling
    ///   after-effects (a `\newcommand`-style definition) are applied *internally*, not
    ///   returned as a pass-through delta — so after the call, the outcome's exported
    ///   live state is their only carrier. Re-entering with a clone of `cx.state`
    ///   silently rolls them back. A caller that re-anchors its ambient state first
    ///   (the root loop's `cx.state = outcome.state`) makes the two coincide.
    ///
    /// - **Stand the reader where the next run should start.** The stop contract is
    ///   defined ([`TokenStopCondition::consume`], [`StopCause`]'s per-variant docs): a
    ///   left stop token sits at its own `span.start`, pre-space already staged, and
    ///   re-peeks clean — but re-entering with the reader still on it and the same
    ///   stop condition stops again immediately, staging nothing: an infinite loop for
    ///   an unconditional resumer. Deal with the token first — consume it
    ///   ([`probe_token`](ParseContext::probe_token) +
    ///   [`move_past`](crate::token::TokenReader::move_past), the environment
    ///   terminator flow), skip it (`move_to_pos(span.end())`, the root loop), or
    ///   exclude it from the next run's stop spec.
    ///
    /// - **Concatenate the segments yourself.** Each run returns its own
    ///   [`NodesOutcome::nodes`]; the resuming caller extends one list across runs.
    ///   Bytes skipped *between* segments then belong to no node — either accept the
    ///   diagnosed byte-accounting break (the root loop's stray-close precedent) or
    ///   stage a span-backed chars fallback over them (the unresolvable-command
    ///   precedent, [`UnresolvableCommand`]) so the siblings keep tiling the parsed
    ///   extent. And a node stop condition counts each run's own siblings — its count
    ///   argument restarts at zero in a resumed segment. A bridge that *propagates*
    ///   the runs' state effects likewise merges each run's
    ///   [`NodesOutcome::after_effects`] into one record in run order (the
    ///   precedent of
    ///   [`parse_attached_source`](ParseContext::parse_attached_source)).
    ///
    /// Whether to resume at all is a per-construct policy question, not a default:
    /// the environment body deliberately **unwinds** on a terminator mismatch instead
    /// of resuming — a body that diagnosed `\end{A}` and kept going inside
    /// `\begin{A}…\begin{B}…\end{A}` would consume the enclosing environment's
    /// terminator (see [`EnvironmentBodyParser`]).
    // The output-plus-delta pair is the decided ConstructParser signature ([§dd-dr:parsers-engine]).
    #[allow(clippy::type_complexity)]
    pub fn parse_nodes<'p>(
        &mut self,
        state: Arc<ParsingState<L>>,
        stop: StopSpec<'p, L>,
        child_states: ChildStateSpec<'p, L>,
    ) -> ConstructParserResult<L, (NodesOutcome<L>, Option<ParsingStateDelta<L>>)>
    where
        'a: 'p,
        L::InvocationSyntax: FromInvocation<L>,
    {
        let driver = self.driver;
        let mut parser = driver.make_nodes_parser(stop, child_states);
        self.parse_construct(&mut *parser, Some(state), None)
    }

    /// **A thin wrapper over [`parse_construct`](ParseContext::parse_construct)**:
    /// builds the group parser from the driver's
    /// [`make_group_parser`](ParseDriver::make_group_parser) factory and runs it
    /// through `parse_construct(parser, Some(base), frame)`. Fusing the factory
    /// with the descent entry point is the point of this method: routing every
    /// group descent through it makes one driver override apply at every group
    /// site.
    ///
    /// Parses one **group descent** (the consumed `GroupOpen` token's facts: open
    /// span and resolved rule) with `base` as the group's input state. `frame`,
    /// when `Some`, is pushed around the whole descent
    /// ([`parse_construct`](ParseContext::parse_construct)'s frame semantics).
    // Same decided pair as above.
    #[allow(clippy::type_complexity)]
    pub fn parse_group<'p>(
        &mut self,
        base: Arc<ParsingState<L>>,
        open_span: Span,
        rule: Arc<GroupRule<L>>,
        child_states: ChildStateSpec<'p, L>,
        frame: Option<Frame<L>>,
    ) -> ConstructParserResult<L, (BuildId, Option<ParsingStateDelta<L>>)>
    where
        'a: 'p,
        L::InvocationSyntax: FromInvocation<L>,
    {
        let driver = self.driver;
        let mut parser = driver.make_group_parser(open_span, rule, child_states);
        self.parse_construct(&mut *parser, Some(base), frame)
    }

    /// Run `f` with `frame` pushed on the session's live frame stack — the descent-point
    /// primitive of the parse traceback: every condition recorded through the
    /// recovery entry point while `f` runs carries the frame in its snapshot.
    ///
    /// Closure-scoped rather than an RAII guard, deliberately: a guard would hold
    /// `&mut self` against the parser body. The pop after `f` returns covers the `Err`
    /// path too — a bubbling [`ParseError`] leaves no stale frames behind.
    pub fn with_frame<R>(&mut self, frame: Frame<L>, f: impl FnOnce(&mut Self) -> R) -> R {
        self.session.push_frame(frame);
        let result = f(self);
        self.session.pop_frame();
        result
    }

    /// Build the abort error for an extension-implementation contract violation
    /// detected at `span` — an [`ImplementationError`] with the live traceback
    /// attached. `detail` is typically a [`NodeBuildError`](crate::node::NodeBuildError)
    /// or a literal contract description.
    ///
    /// Deliberately **not** the recovery entry point: an implementation bug is not a source
    /// condition — it aborts even under [`Recovery::Tolerant`](crate::error::Recovery::Tolerant), and no
    /// [`ParseDriver::refine_diagnostic`] pass applies.
    pub fn implementation_error(
        &self,
        detail: impl fmt::Display,
        span: Span,
    ) -> ParseError<L::SourceOrigin> {
        ParseError::new(
            ImplementationError::new(detail.to_string()),
            SourceSpan::new(&self.source, span),
        )
        .with_frames(self.session.snapshot_frames())
    }
}

/// Condition: an implementation of an extension point — an argument or construct
/// parser, a [`Lang`] hook, a spec factory — violated a library contract. An
/// implementation bug to fix, not a source-input problem: it aborts the parse even
/// under [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) (built through
/// [`ParseContext::implementation_error`], which ignores the recovery policy).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.constructs.implementation-error",
    message = "implementation error (extension contract violation): {detail}"
)]
pub struct ImplementationError {
    /// Description of the violated contract.
    pub detail: String,
}

/// Condition: a scope op of an in-parse state delta failed
/// ([`ScopeOpError`](crate::scopes::ScopeOpError), rendered into `detail`) — reported
/// through the recovery entry point by the [`ParseContext`] derivation sugars: strict parses abort on it; tolerant parses record it and
/// continue under the ops-skipped state
/// ([`DeriveError::recovered`](crate::state::DeriveError::recovered)).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.specs.scope-op-failed",
    message = "scope op failed: {detail}"
)]
pub struct ScopeOpFailed {
    /// The rendered failure ([`ScopeOpError`](crate::scopes::ScopeOpError)'s
    /// `Display`).
    pub detail: String,
}

impl<L: Lang> fmt::Debug for ParseContext<'_, '_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseContext")
            .field("pos", &self.tokens.pos())
            .field("source", &self.source)
            .field("state", &self.state)
            .field("session", &self.session)
            .finish()
    }
}

/// Result type of construct parsing. `Err` means abort — see the error contract
/// in [`core::constructs`](crate::core::constructs).
///
/// Parameter convention follows [`TokenResult`](crate::token::TokenResult) (lang first,
/// payload last). The underlying [`ParseError`] is generic over the source origin only
/// (mirroring [`Diagnostic`](crate::error::Diagnostic)); the alias derives it from `L`.
pub type ConstructParserResult<L, T> = Result<T, ParseError<<L as Lang>::SourceOrigin>>;

/// A parser for one construct, reading tokens and staging nodes through the context.
///
/// Implementations are tier-2 **temporaries** (the two-tier ownership model in
/// [`core::constructs`](crate::core::constructs)): per-use configuration in
/// fields, `&mut self` working state, dropped with the frame.
///
/// On success, a parser returns its output (typically staged `BuildId`s) together with
/// an optional [`ParsingStateDelta`] — the construct's *after-effect for the caller*
/// (`\newcommand` pushing definitions for subsequent siblings), never its internal
/// scoping.
pub trait ConstructParser<L: Lang> {
    /// What the parser produces: a staged `BuildId`, a `Vec<BuildId>`, a
    /// `ParsedArguments`, …
    type Output;

    /// Parse the construct at the context's current position.
    // The output-plus-delta pair is the decided signature (DESIGN_RATIONALE.md [§dd-dr:parsers-engine]);
    // splitting it into a named type would only rename the complexity.
    #[allow(clippy::type_complexity)]
    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (Self::Output, Option<ParsingStateDelta<L>>)>;
}

/// One resolved callable invocation, as handed to
/// [`CallableSpec::make_invocation_parser`]: the dispatch loop resolves the trigger
/// token (via [`ParseDriver::resolve_command`], or the
/// resolution riding on a `Specials` token), builds this value, and moves it into the
/// invocation parser the spec's factory returns.
///
/// When the parser runs, the trigger token has already been **consumed whole** by the
/// dispatching arm (`move_past(token, true)`, syntactic post-space included) — see
/// [`StdInvocationParser`]'s documentation for the full contract.
///
/// [`CallableSpec::make_invocation_parser`]: crate::spec::CallableSpec::make_invocation_parser
pub struct Invocation<'a, 's, L: Lang> {
    /// The invocation form the trigger resolved to.
    pub callable_type: L::CallableTypeId,
    /// The trigger's invocation spelling, as written. The *standard* parser stores an
    /// owned copy on the node; a takeover composition may store a name of its own
    /// (an environment node records the environment's name, not `begin`).
    pub name: &'s str,
    /// The behavior spec driving the parse.
    pub spec: &'a Arc<dyn CallableSpec<L>>,
    /// The trigger token: span, pre-space, escape char.
    pub token: &'a Token<'s, L>,
}

impl<L: Lang> fmt::Debug for Invocation<'_, '_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Invocation")
            .field("callable_type", &self.callable_type)
            .field("name", &self.name)
            .field("spec", &self.spec)
            .field("token", &self.token)
            .finish()
    }
}

/// Opt-in constructor contract on a language's invocation-syntax payload
/// ([`Lang::InvocationSyntax`], bounded by the core
/// [`InvocationSyntax`](crate::state::InvocationSyntax) trait): build the
/// recorded trigger-spelling facts from one resolved [`Invocation`].
///
/// Consulted by the **standard staging sites** —
/// [`ParseContext::stage_invocation`] (and through it
/// [`StdInvocationParser`] and the expression-position bare-callable staging) plus
/// the preset's specials sites — under a bound-where-used
/// (`where L::InvocationSyntax: FromInvocation<L>`): a standard parser's knowledge
/// about a custom payload is exactly "what the invocation bundle shows", and the
/// bound says so. The bundle carries the trigger token
/// ([`Invocation::token`]), so the constructor sees precisely what was matched
/// (spelling, escape character, syntactic post-space).
///
/// Deliberately **separate from the required data bound**: a language whose
/// payload cannot be built from an `Invocation` alone stages its callables through
/// custom parsers (via [`stage_node`](ParseContext::stage_node)) and never
/// implements this trait — but driving the standard engine requires it (the
/// standard dispatch loop reaches [`StdInvocationParser`] through the defaulted
/// spec factory). techy implements it for `()` (records nothing), and the
/// latexlike preset for its payload enum, so `Lang`s with `InvocationSyntax = ()`
/// and latexlike-family languages satisfy the bound out of the box.
pub trait FromInvocation<L: Lang>: Sized {
    /// The payload recording `invocation`'s trigger spelling. Pure transcription:
    /// reads the bundle (typically [`Invocation::token`]'s facts), performs no
    /// parsing and consumes nothing.
    fn from_invocation(invocation: &Invocation<'_, '_, L>) -> Self;
}

/// The no-record payload: nothing to transcribe.
impl<L: Lang> FromInvocation<L> for () {
    fn from_invocation(_invocation: &Invocation<'_, '_, L>) {}
}
