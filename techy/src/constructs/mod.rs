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
//! locally and either builds a child `cx` or swaps `cx.state` and restores it afterwards
//! (structural revert — `Arc` clone is cheap). The `Option<ParsingStateDelta>` in the
//! return value is exclusively the *after-effect for the caller* (`\newcommand`).
//!
//! # Errors
//!
//! `Err` means **abort**: recovery happens at the detection site (the
//! [`recover`](ParseContext::recover) helper), and abnormal endings of sub-parses travel
//! as data ([`StopCause`]) — nobody continues past an `Err`.

mod argument_parsers;
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
pub use chars_group_parser::CharsGroupArgumentParser;
pub use child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
pub use embellishments_parser::EmbellishmentsArgumentParser;
pub use tack_on_parser::{RepeatedTackOnField, TackOnFieldsArgumentParser};
pub use environment_parser::{
    read_rigid_name_group, EnvironmentBody, EnvironmentBodyParser,
    EnvironmentTerminatorMismatch, MalformedEnvironmentTerminator,
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
use alloc::sync::Arc;
use core::fmt;

use crate::engine::{Frame, FrameTitle, ParseDriver, ParserSession};
use crate::error::{DiagnosticData, DiagnosticInfo, ParseError};
use crate::source::{Source, SourceSpan, Span};
use crate::spec::{CallableSpec, FrameRole};
use crate::node::BuildId;
use crate::state::{Lang, ParsingState, ParsingStateDelta};
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

/// Everything a construct parser needs, in one context value — avoiding pylatexenc's
/// three-argument threading (`walker, token_reader, parsing_state`) and giving the API
/// one place to grow (depth limits, cancellation).
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
    /// The parser's **input** parsing state (the caller sets it; see the module docs
    /// for the state-threading convention).
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

    /// Run `parser` with [`state`](ParseContext::state) scoped to `state` for the
    /// duration of the sub-parse, restoring the outer state afterwards — the **descent
    /// primitive** for every construct that parses child content under a derived state
    /// (group interiors, argument extents, slot bodies; the pylatexenc
    /// `walker.parse_content(parser, …, parsing_state)` analog).
    ///
    /// This is ordering enforcement, not unwind safety: hand-rolled swap/restore must
    /// remember to restore **before** `?`-propagating the result; here the restore is
    /// structural. The returned [`ParsingStateDelta`] is the construct's after-effect
    /// for the caller, passed through **unapplied** — whether and where it applies is
    /// caller business (the "caller applies deltas" law).
    pub fn parse_scoped<P>(
        &mut self,
        state: Arc<ParsingState<L>>,
        parser: &mut P,
    ) -> ConstructParserResult<L, (P::Output, Option<ParsingStateDelta<L>>)>
    where
        P: ConstructParser<L> + ?Sized,
    {
        self.with_scoped_state(state, |cx| parser.parse(cx))
    }

    /// The scoped-state primitive under [`parse_scoped`](ParseContext::parse_scoped),
    /// for the descents that are not `ConstructParser`-shaped (the per-argument
    /// delta around `ArgumentParser::parse_argument`).
    pub(crate) fn with_scoped_state<R>(
        &mut self,
        state: Arc<ParsingState<L>>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let outer = core::mem::replace(&mut self.state, state);
        let result = f(self);
        self.state = outer;
        result
    }

    /// Detection-site recovery — **the recover funnel**:
    /// boxes the condition and hands it to [`ParseDriver::recover`], where the policy
    /// is defined — the default driver path applies
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

    /// The funnel's boxed entry — for payloads that already live behind the dyn facade
    /// (the token-error lift, where a `Custom` payload must not be double-boxed).
    pub(crate) fn recover_boxed(
        &mut self,
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        self.driver.recover(self.session, &self.state, data, span)
    }

    /// Session-mediated derivation from the **current** state
    /// ([`state`](ParseContext::state)) — sugar over
    /// [`ParserSession::derived_state`] supplying this context's driver, so every
    /// transition reaches [`ParseDriver::observe_transition`]. The dominant descent
    /// shape; derive from another base via the session method directly
    /// (`cx.session.derived_state(cx.driver, &base, &delta)`).
    ///
    /// **Failing scope ops** route through the recover funnel as
    /// [`ScopeOpFailed`] conditions at the current position: under
    /// [`Recovery::Strict`](crate::error::Recovery::Strict) the first failure aborts;
    /// under [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) each failure is
    /// recorded and the parse continues under the error's
    /// [`recovered`](crate::state::DeriveError::recovered) state (the failing ops
    /// skipped, everything else applied), with the transition observed as usual.
    pub fn derived_state(
        &mut self,
        delta: &ParsingStateDelta<L>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let base = Arc::clone(&self.state);
        match self.session.derived_state(self.driver, &base, delta) {
            Ok(new) => Ok(new),
            Err(failure) => self.recover_derive_failure(&base, failure),
        }
    }

    /// The group-interior derivation from the **current** state — sugar over
    /// [`ParserSession::group_interior_state`] supplying this context's driver: the
    /// canonical expecting-close override merged with the driver's
    /// [`group_interior_delta`](ParseDriver::group_interior_delta), memoized per
    /// `(base, rule)`. Failing scope ops in the driver's descent delta recover exactly
    /// like [`derived_state`](ParseContext::derived_state)'s (the recovered interior
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
    /// failing op through the recover funnel (strict: the first one aborts), then
    /// commit the ops-skipped transition — continue under the error's recovered state
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
        let crate::state::DeriveError { failures, recovered, delta } = failure;
        for failed_op in &failures {
            self.recover(ScopeOpFailed::new(failed_op.to_string()), span.clone())?;
        }
        // Tolerant continuation: commit the recovered transition — the session seam
        // observed nothing on the Err path (no transition had been committed).
        let recovered = Arc::new(recovered);
        self.driver.observe_transition(&mut self.session.ext, base, &recovered, &delta);
        Ok(recovered)
    }

    /// Parse one **nodes descent** (a content run: group interior, environment body,
    /// top-level drive) under `state`, with the parser obtained from the driver's
    /// [`make_nodes_parser`](ParseDriver::make_nodes_parser) factory — the uniform
    /// routing that makes one driver override apply to every descent site.
    /// State scoping and restoration follow
    /// [`parse_scoped`](ParseContext::parse_scoped).
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
    ///   return ([`parse_scoped`](ParseContext::parse_scoped)), and the run's sibling
    ///   after-effects (a `\newcommand`-style definition) are applied *internally*, not
    ///   returned as a pass-through delta — so after the call, the outcome's exported
    ///   live state is their only carrier. Re-entering with a clone of `cx.state`
    ///   silently rolls them back. A caller that re-anchors its ambient state first
    ///   (the root loop's `cx.state = outcome.state`) makes the two coincide.
    ///
    /// - **Stand the reader where the next run should start.** The stop seam is
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
    ///   argument restarts at zero in a resumed segment.
    ///
    /// Whether to resume at all is a per-construct policy question, not a default:
    /// the environment body deliberately **unwinds** on a terminator mismatch instead
    /// of resuming — a body that diagnosed `\end{A}` and kept going inside
    /// `\begin{A}…\begin{B}…\end{A}` would swallow the enclosing environment's
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
    {
        let driver = self.driver;
        let mut parser = driver.make_nodes_parser(stop, child_states);
        self.parse_scoped(state, &mut *parser)
    }

    /// Parse one **group descent** (the consumed `GroupOpen` token's facts: open span
    /// and resolved rule) with `base` as the group's input state, the parser obtained
    /// from the driver's [`make_group_parser`](ParseDriver::make_group_parser)
    /// factory — the uniform routing of every group descent site.
    // Same decided pair as above.
    #[allow(clippy::type_complexity)]
    pub fn parse_group<'p>(
        &mut self,
        base: Arc<ParsingState<L>>,
        open_span: Span,
        rule: Arc<GroupRule<L>>,
        child_states: ChildStateSpec<'p, L>,
    ) -> ConstructParserResult<L, (BuildId, Option<ParsingStateDelta<L>>)>
    where
        'a: 'p,
    {
        let driver = self.driver;
        let mut parser = driver.make_group_parser(open_span, rule, child_states);
        self.parse_scoped(base, &mut *parser)
    }

    /// Run `f` with `frame` pushed on the session's live frame stack — the descent-point
    /// primitive of the parse traceback: every condition the
    /// recover funnel records while `f` runs carries the frame in its snapshot.
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
    /// Deliberately **not** the recover funnel: an implementation bug is not a source
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
/// [`ParseContext::implementation_error`], which bypasses the recover funnel).
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
/// through the recover funnel by the [`ParseContext`] derivation sugars: strict parses abort on it; tolerant parses record it and
/// continue under the ops-skipped state
/// ([`DeriveError::recovered`](crate::state::DeriveError::recovered)).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.constructs.scope-op-failed",
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

/// Result type of construct parsing. `Err` means abort — see the module docs.
///
/// Parameter convention follows [`TokenResult`](crate::token::TokenResult) (lang first,
/// payload last). The underlying [`ParseError`] is generic over the source origin only
/// (mirroring [`Diagnostic`](crate::error::Diagnostic)); the alias derives it from `L`.
pub type ConstructParserResult<L, T> = Result<T, ParseError<<L as Lang>::SourceOrigin>>;

/// A parser for one construct, reading tokens and staging nodes through the context.
///
/// Implementations are tier-2 **temporaries** (module docs): per-use configuration in
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
/// [`StdInvocationParser`]'s module docs for the full contract.
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
