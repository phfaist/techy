//! [`NodesParser`]: the main content dispatch loop, with
//! its stop machinery ([`StopSpec`], [`TokenStopCondition`], [`StopCause`]).
//!
//! The parser peeks one token at a time and dispatches on its kind — never on parser
//! registries. The content arms (6.2) cover chars accumulation, paragraph breaks
//! (via [`ParseDriver::make_paragraph_break_node`]), comments, and end of stream. The
//! `GroupOpen` arm (6.3) descends: it resolves the interior's base state through the
//! per-use [`ChildStateSpec`] policy, consumes the trigger token, and runs a
//! [`GroupParser`] under the policy's state (structural swap/revert). The
//! `Command`/`Specials` invocation arms (6.4) descend the same way: a `Command` token
//! resolves through [`ParseDriver::resolve_command`] under the loop's own state (resolution
//! precedes the descent policy), a `Specials` token carries its resolution; the
//! arm consumes the trigger whole, builds the [`Invocation`], and runs the parser
//! returned by the spec's
//! [`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser)
//! factory under the policy's state. The state delta an invocation parser returns is
//! the invocation's after-effect for subsequent siblings (`\newcommand`), applied to
//! the loop's own state session-mediated (`cx.session.derived_state(…)`); the applied
//! deltas are also merged into one record the outcome exports
//! ([`NodesOutcome::after_effects`]) for callers that propagate the run's state
//! effects elsewhere.
//!
//! # Whitespace and span invariants
//!
//! 1. `Char` tokens accumulate into **maximal** `Chars` nodes; every token's `pre_space`
//!    (content whitespace) joins the pending run, and pending whitespace with no
//!    adjacent chars becomes a whitespace-only `Chars` node. Parsed content is always
//!    `TextContent::Spanned` (the exact span slice).
//! 2. Paragraph breaks are their own nodes (the `Lang` hook's kind, staged by the loop
//!    over the full token span); runs flush at breaks and never merge across them.
//! 3. Comment nodes come straight from whole-comment tokens (start delimiter, content,
//!    and post-space each recorded).
//! 4. At end of stream, the terminal token's `pre_space` materializes as a final
//!    whitespace-only `Chars` node (or joins a pending run).
//!
//! Together these give the **partition invariant**: the staged sibling spans tile the
//! parsed extent exactly, with no gaps and no double counting.
//!
//! # Stop conditions and the position seam
//!
//! A [`StopSpec`] carries two independent triggers: a *token*
//! condition tested on peek — a match ends the parse and, per the condition's
//! [`consume`](TokenStopCondition::consume) switch, either leaves the token unconsumed
//! for the caller or consumes it here — and a *node* condition tested after each staged
//! node — a match includes that node and stops after it. Conditions are tested only at
//! this parser's own nesting level (a nested group is consumed whole by the group
//! parser). Abnormal endings are **data**, not errors: the parser reports its
//! [`StopCause`] and the caller decides — an unexpected group close stays
//! unconsumed.
//!
//! On *any* return the stop token's pre-space is first flushed into the sibling nodes
//! (the partition invariant requires it — the whitespace before a `}` or `\end` is
//! interior content). A **left** stop token then sits at its own `span.start`, so
//! re-peeking yields it with an **empty** `pre_space` and no byte is represented twice; a
//! **consumed** stop token is taken whole, including any syntactic post-space (a command
//! name's terminating whitespace), so the reader stands just past it. The matched span is
//! reported in [`StopCause::TokenCondition`] either way.
//!
//! When the two triggers collide — the pre-stop flush stages a node the node condition
//! would match — the token condition wins outright: that flush does **not**
//! consult the node predicate. Its answer could change nothing (the parse ends as
//! `TokenCondition` either way; honoring it would instead leave a `consume = true` token
//! unconsumed, breaking the flag's atomicity), and the predicate is a stateful `FnMut`
//! that must not observe a consulted-but-ignored call.
//!
//! # Recovery
//!
//! Recovery happens where a problem is detected, through the session's policy helper.
//! Tokenizer errors continue with their [`TokenRecovery`](crate::token::TokenRecovery)
//! placeholder token, the reader repositioned to the error's `resume` position (so the error
//! is never re-read); an unresolvable command recovers as a diagnostic plus a chars
//! fallback node over the token's span (specials never take this path: recognition =
//! resolution, so a recognized trigger always dispatches).
//! Markup text inside a `Chars` node is an accepted tolerant-recovery artifact, always
//! accompanied by a diagnostic; fallback nodes are deliberately *not* merged into
//! neighboring chars runs. Group recovery (unclosed at end of input, mismatched close)
//! lives in [`GroupParser`]. `Err` means abort — nobody continues past one.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::mem;

use crate::error::{DiagnosticInfo, ParseError, ToDiagnosticValue};
use crate::node::{BuildId, NodeKind, StagedNodeView};
use crate::source::SourceSpan;
use crate::engine::{CommandResolution, ParseDriver};
use crate::state::{FeaturePresence, Lang, LangFeatures, ParsingState, ParsingStateDelta};
use crate::token::{TokenEdge, TokenKind, TokenReader};

use super::child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
use super::{comment_node_kind, ConstructParser, ConstructParserResult, FromInvocation, Invocation, invocation_frame, ParseContext};

/// Condition: a [`Command`](TokenKind::Command) token resolved to no callable
/// ([`ParseDriver::resolve_command`](crate::engine::ParseDriver::resolve_command) returned no
/// [`Resolved`](crate::engine::CommandResolution::Resolved)) — the content loop recovers
/// with a span-backed chars fallback.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.specs.unresolvable-command")]
pub struct UnresolvableCommand {
    /// The command name, as written (without the escape character).
    pub name: String,
    /// The escape character that introduced the command.
    pub escape_char: char,
    /// Optional detail on why resolution failed, straight from
    /// [`CommandResolution::Unresolved`](crate::engine::CommandResolution::Unresolved):
    /// the trait's default hook reports that command resolution is not implemented;
    /// a resolver may report where it searched or hint at a fix. Appended to the
    /// message.
    pub detail: Option<String>,
}

// Hand-written wording: the detail suffix is conditional (a branch the derive's
// message format string cannot express).
impl fmt::Display for UnresolvableCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot resolve command ‘{}{}’", self.escape_char, self.name)?;
        if let Some(detail) = &self.detail {
            write!(f, " ({detail})")?;
        }
        Ok(())
    }
}

/// Condition: a [`Command`](TokenKind::Command) token's resolution *failed
/// operationally* — a definition provider errored while answering the query
/// ([`ParseDriver::resolve_command`](crate::engine::ParseDriver::resolve_command)
/// returned [`Failed`](crate::engine::CommandResolution::Failed)) — as opposed to a
/// clean miss ([`UnresolvableCommand`]). The content loop recovers the same way (a
/// span-backed chars fallback), but the distinct condition
/// lets tooling tell "command unknown" from "resolver broken" (mirrors the
/// [`ScopeOpFailed`](crate::constructs::ScopeOpFailed) precedent for operational
/// scope-op failures).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.specs.command-resolution-failed")]
pub struct CommandResolutionFailed {
    /// The command name, as written (without the escape character).
    pub name: String,
    /// The escape character that introduced the command.
    pub escape_char: char,
    /// Optional detail on the operational failure — typically the provider's rendered
    /// error, straight from [`CommandResolution::Failed`](crate::engine::CommandResolution::Failed).
    /// Appended to the message.
    pub detail: Option<String>,
}

// Hand-written wording: the detail suffix is conditional (as for UnresolvableCommand).
impl fmt::Display for CommandResolutionFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "command resolution failed for ‘{}{}’", self.escape_char, self.name)?;
        if let Some(detail) = &self.detail {
            write!(f, " ({detail})")?;
        }
        Ok(())
    }
}

/// Condition: a callable whose invocation requires content — a mandatory argument, a
/// body ([`CallableSpec::requires_content`](crate::spec::CallableSpec::requires_content))
/// — was used *bare* where a single expression was required (pylatexenc's
/// requires-arguments diagnostic) — the expression position recovers by staging the
/// bare single-token callable.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.arguments.expression-callable-requires-content",
    message = "cannot use ‘{callable}’ as a single expression: it requires content \
               (arguments or a body)"
)]
pub struct ExpressionCallableRequiresContent {
    /// The callable's invocation spelling, as written (`\frac`, `~`).
    pub callable: String,
}

/// Condition: a [`TokenRecovery`](crate::token::TokenRecovery) placeholder token of a
/// kind the content loop cannot process as content (a `Specials` or `GroupOpen`
/// placeholder). The placeholder stands in for a failed read — it has no real source
/// bytes behind it — so it cannot be dispatched or parsed as a construct; the loop
/// recovers with a chars fallback over the error's span.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.recovery.unusable-recovery-token")]
pub struct UnusableRecoveryToken {
    /// The placeholder's spelling (the specials trigger or open delimiter as written).
    pub spelling: String,
    /// Which token kind the placeholder had.
    pub kind: UnusableRecoveryTokenKind,
}

/// Which token kind an [`UnusableRecoveryToken`] placeholder had.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToDiagnosticValue)]
#[non_exhaustive]
pub enum UnusableRecoveryTokenKind {
    /// A `Specials` placeholder (a recognized trigger cannot be invoked without real
    /// bytes to consume).
    Specials,
    /// A `GroupOpen` placeholder (a group cannot be parsed out of a delimiter with no
    /// bytes behind it).
    GroupOpen,
}

// Hand-written wording: the message varies by placeholder kind (a match, which the
// message format string cannot express).
impl fmt::Display for UnusableRecoveryToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            UnusableRecoveryTokenKind::Specials => write!(
                f,
                "cannot invoke specials ‘{}’ here: the token is a recovery placeholder \
                 standing in for a tokenization error, with no source bytes to parse",
                self.spelling
            ),
            UnusableRecoveryTokenKind::GroupOpen => write!(
                f,
                "cannot parse group opened by ‘{}’ here: the token is a recovery \
                 placeholder standing in for a tokenization error, with no source bytes \
                 to parse",
                self.spelling
            ),
        }
    }
}

/// Which peeked token matches a [`TokenStopCondition`] (mirroring pylatexenc's
/// `stop_token_condition`, reified as a closed enum plus a tier-2 predicate escape).
pub enum TokenStopKind<'p, L: Lang> {
    /// Stop at a [`Command`](TokenKind::Command) token with this name (an
    /// environment body stopping at `\end`).
    Command {
        /// The command name to stop at (as written, without the escape character).
        name: &'p str,
    },
    /// Stop at a [`GroupClose`](TokenKind::GroupClose) token that spells `close` **and**
    /// whose class (resolved against the current state) is `group_type` — the exact
    /// `(group_type, close)` pairing the enclosing group opened with. Both must match:
    /// a group opened with `{` (class `group_type`) stops at `}`, but neither at a `]`
    /// that merely shares its class (different `close`) nor at a `}` a state change has
    /// re-classed to a *different* group class (same `close`, different `group_type`). A
    /// non-matching close surfaces as [`StopCause::UnexpectedGroupClose`] instead.
    ///
    /// The class is not carried on the token — `GroupClose` holds only its `delim`
    /// — so it is re-resolved against `cx.state`, the same
    /// state (including sibling deltas applied so far) the tokenizer used to emit the
    /// token; a reclassifying delta is therefore reflected here.
    GroupClose {
        /// The group class the enclosing group belongs to (resolved for the arriving
        /// close against the current state — the expected close, then the delimiter table).
        group_type: L::GroupTypeId,
        /// The closing delimiter (as written, e.g. `}`) the enclosing group expects.
        close: &'p str,
    },
    /// Stop at a [`ParagraphBreak`](TokenKind::ParagraphBreak) token.
    ParagraphBreak,
    /// Stop at any token the predicate matches. Programmatic conditions live only in
    /// tier-2 parser temporaries, never in spec data. The predicate receives the
    /// peeked **token** and a shared, call-scoped reference to the **reader that
    /// produced it**, so it can ask whatever it needs about the token —
    /// [`token_kind`](crate::token::TokenReader::token_kind) for what it is,
    /// [`source_span_of`](crate::token::TokenReader::source_span_of) for where — and
    /// cannot move the stream.
    ///
    /// An `Err` from the predicate **aborts the parse** under any recovery policy —
    /// a predicate that cannot answer leaves no sound way to decide where the run
    /// ends. Carry [`HookFailed`](crate::error::HookFailed) for an operational
    /// failure in the predicate's own code,
    /// [`ImplementationError`](super::ImplementationError) for a violated library
    /// contract, or a document condition for a diagnosis made deliberately. The
    /// consultation site attaches the live traceback when the error carries no
    /// frames of its own. An infallible predicate wraps its answer in `Ok(...)`
    /// and that is the only change.
    // The predicate signature is the variant's whole documented meaning; hiding it
    // behind an alias would only make callers look the signature up elsewhere.
    #[allow(clippy::type_complexity)]
    Predicate(
        &'p dyn Fn(
            &L::Token,
            &dyn TokenReader<'_, L>,
        ) -> Result<bool, ParseError<L::SourceOrigin>>,
    ),
}

/// The token-condition half of a [`StopSpec`]: which peeked token ends the parse
/// ([`kind`](Self::kind)) and whether [`NodesParser`] consumes it on a match
/// ([`consume`](Self::consume)).
///
/// On a match `NodesParser` returns [`StopCause::TokenCondition`] with the matched
/// token's span. With `consume = false` the token is left **unconsumed** at its own
/// `span.start` (peek it again); with `consume = true` it is taken whole — including any
/// syntactic post-space (a command name's terminating whitespace), so no byte is re-read
/// as content. This is a declarative consume/leave switch, not pylatexenc's
/// `handle_stop_condition_token` interpretation hook.
pub struct TokenStopCondition<'p, L: Lang> {
    /// Which token ends the parse.
    pub kind: TokenStopKind<'p, L>,
    /// Whether `NodesParser` consumes the matched token (`true`) or leaves it unconsumed
    /// for the caller (`false`).
    pub consume: bool,
}

/// What ends a [`NodesParser`] run — both triggers optional and independent.
///
/// The `'p` lifetime ties borrowed conditions (names, predicates) to the parser
/// temporary — construct parsers are free to borrow (two-tier ownership model).
pub struct StopSpec<'p, L: Lang> {
    /// Token condition, tested on peek; a match ends the parse, consuming the token or
    /// leaving it per its [`consume`](TokenStopCondition::consume) switch.
    pub token: Option<TokenStopCondition<'p, L>>,
    /// Node condition, tested after each staged node with (number of nodes staged so
    /// far, view of the just-staged node); a match includes that node and stops after
    /// it. Not consulted on the final flush a matched token condition triggers — the
    /// token condition wins outright (its answer could change nothing, and the
    /// predicate is a stateful `FnMut` that must not observe a
    /// consulted-but-ignored call). The
    /// (count, last node) signature is a deliberate deviation from pylatexenc's
    /// whole-nodelist rescans.
    ///
    /// An `Err` from the condition **aborts the parse** under any recovery policy —
    /// a condition that cannot answer leaves no sound way to decide where the run
    /// ends. Carry [`HookFailed`](crate::error::HookFailed) for an operational
    /// failure in the condition's own code,
    /// [`ImplementationError`](super::ImplementationError) for a violated library
    /// contract, or a document condition for a diagnosis made deliberately. The
    /// consultation site attaches the live traceback when the error carries no
    /// frames of its own. An infallible condition wraps its answer in `Ok(...)`
    /// and that is the only change.
    // The decided signature (DESIGN_RATIONALE.md [§dd-dr:parsers-engine]); an alias would only rename it.
    #[allow(clippy::type_complexity)]
    pub node: Option<
        &'p mut dyn FnMut(
            usize,
            StagedNodeView<'_, L>,
        ) -> Result<bool, ParseError<L::SourceOrigin>>,
    >,
}

impl<'p, L: Lang> StopSpec<'p, L> {
    /// No stop conditions: parse to end of input.
    pub fn none() -> StopSpec<'p, L> {
        StopSpec { token: None, node: None }
    }

    /// Only a token condition: stop at `kind`, consuming the matched token or leaving it
    /// per `consume`.
    pub fn at_token(kind: TokenStopKind<'p, L>, consume: bool) -> StopSpec<'p, L> {
        StopSpec { token: Some(TokenStopCondition { kind, consume }), node: None }
    }
}

impl<L: Lang> Default for StopSpec<'_, L> {
    fn default() -> Self {
        StopSpec::none()
    }
}

/// Condition: a group close delimiter appeared with no group open — the *root driver's*
/// diagnosis of [`StopCause::UnexpectedGroupClose`] (defined here, next to the stop
/// cause that announces the situation, so custom root drivers reuse it). Inside a group the enclosing [`GroupParser`](super::GroupParser) claims the token
/// instead ([`UnclosedGroup`](super::UnclosedGroup) covers *that* family) — this condition is for the
/// outermost level, where nobody claims it: the core
/// [`Language::parse`](crate::engine::Language::parse) drive loop reports it through
/// the recovery entry point, consumes the token, and resumes (strict parses abort; the
/// skipped bytes are the accepted tolerant byte-accounting break).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.groups.stray-group-close",
    message = "unexpected closing ‘{delim}’ — no group is open"
)]
pub struct StrayGroupClose {
    /// The stray close delimiter as written (e.g. `}`).
    pub delim: String,
}

/// How a [`NodesParser`] run ended. Abnormal endings are **data**, not errors — only the
/// caller knows whether reaching end of input before `\end{align}` is a problem.
pub enum StopCause<L: Lang> {
    /// The token stop condition matched. `span` is the matched token's span; whether it
    /// was consumed is the [`consume`](TokenStopCondition::consume) the caller set —
    /// consumed ⇒ the reader stands just past it, otherwise it sits unconsumed at
    /// its own start, its pre-space already staged as sibling content.
    TokenCondition {
        /// The matched stop token's span.
        span: SourceSpan<L::SourceOrigin>,
        /// The stream position just past the matched token (its post-space
        /// included) — where a caller that wants the token skipped repositions the
        /// reader ([`move_to_position`](crate::token::TokenReader::move_to_position)),
        /// whether or not the condition consumed it.
        after: L::StreamPosition,
    },
    /// The node stop condition fired on the last staged node (the reader stands where
    /// that node ended: a directly staged node is consumed, a flush leaves the triggering
    /// token unconsumed at its own start).
    NodeCondition,
    /// [`EndOfStream`](TokenKind::EndOfStream) was reached (its trailing-whitespace
    /// node, if any, is already staged).
    EndOfInput,
    /// A group close no condition asked for; the close token is left unconsumed at
    /// its own start and the caller decides (diagnose-and-skip at the root, unwind in
    /// a group parser). The span covers the delimiter exactly as matched
    /// ([`GroupClose`](crate::token::TokenKind::GroupClose) carries the span's slice
    /// and nothing more), so a caller diagnosing the close reads it off the span
    /// ([`SourceSpan::content`](crate::source::SourceSpan::content)) — re-peeking
    /// under any state but the loop's own could tokenize different bytes.
    UnexpectedGroupClose {
        /// The unexpected close token's span.
        span: SourceSpan<L::SourceOrigin>,
        /// The stream position just past the unconsumed close token — the skip
        /// target of a caller that recovers from it.
        after: L::StreamPosition,
    },
}

// Manual impls: derives would demand `L: Debug`/`L: Clone`/`L: PartialEq`, although
// only the (already bounded) associated types are stored.
impl<L: Lang> fmt::Debug for StopCause<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StopCause::TokenCondition { span, after } => f
                .debug_struct("TokenCondition")
                .field("span", span)
                .field("after", after)
                .finish(),
            StopCause::NodeCondition => f.write_str("NodeCondition"),
            StopCause::EndOfInput => f.write_str("EndOfInput"),
            StopCause::UnexpectedGroupClose { span, after } => f
                .debug_struct("UnexpectedGroupClose")
                .field("span", span)
                .field("after", after)
                .finish(),
        }
    }
}

impl<L: Lang> Clone for StopCause<L> {
    fn clone(&self) -> Self {
        match self {
            StopCause::TokenCondition { span, after } => StopCause::TokenCondition {
                span: span.clone(),
                after: after.clone(),
            },
            StopCause::NodeCondition => StopCause::NodeCondition,
            StopCause::EndOfInput => StopCause::EndOfInput,
            StopCause::UnexpectedGroupClose { span, after } => {
                StopCause::UnexpectedGroupClose {
                    span: span.clone(),
                    after: after.clone(),
                }
            }
        }
    }
}

impl<L: Lang> PartialEq for StopCause<L> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                StopCause::TokenCondition { span, after },
                StopCause::TokenCondition { span: other_span, after: other_after },
            ) => span == other_span && after == other_after,
            (StopCause::NodeCondition, StopCause::NodeCondition) => true,
            (StopCause::EndOfInput, StopCause::EndOfInput) => true,
            (
                StopCause::UnexpectedGroupClose { span, after },
                StopCause::UnexpectedGroupClose { span: other_span, after: other_after },
            ) => span == other_span && after == other_after,
            _ => false,
        }
    }
}

impl<L: Lang> Eq for StopCause<L> {}

/// What a [`NodesParser`] produces: the staged sibling nodes, in source order, how the
/// run ended, the loop's live state at the stop, and the merged record of the sibling
/// after-effect deltas the run applied.
pub struct NodesOutcome<L: Lang> {
    /// The staged nodes, in source order (the caller claims them as children).
    pub nodes: Vec<BuildId>,
    /// How the parse ended.
    pub stop: StopCause<L>,
    /// The loop's live state when it returned: the entry state evolved by the sibling
    /// after-effect deltas applied so far. A caller that resumes content at the stop
    /// position (the root's tolerant stray-close skip) continues under this state —
    /// resuming under its own copy of the entry state would silently roll those
    /// after-effects back.
    pub state: Arc<ParsingState<L>>,
    /// The sibling after-effect deltas this run applied, merged into one delta in
    /// application order (`None` = the run applied none). Each merged component is
    /// the **effective, as-applied** delta — context-dependent events already
    /// lowered into their override patches at the loop's own position — so the
    /// record is replayable against a base its producers never saw: later field
    /// overrides win, scope ops (and any context-free events) concatenate in
    /// application order ([`ParsingStateDelta`]'s value-not-closure design). This is
    /// the channel for callers that must **propagate** the run's state effects
    /// rather than resume under them — the `\input` `persist_state` composition
    /// forwards it as the invocation's own after-effect
    /// ([`AttachedSourceOutcome`](super::AttachedSourceOutcome)); a caller that
    /// merely resumes at the stop position wants [`state`](NodesOutcome::state)
    /// instead (re-deriving from the record would re-run fallible scope ops and
    /// re-fire transition observation on a second path). A scope op that *failed*
    /// when first applied stays in the record — nothing is silently stripped — so
    /// a propagating replay re-attempts it and may re-diagnose the same failure
    /// at the propagation site. Boxed like the [`ConstructParser`] pass-through
    /// delta: the common `None` costs one pointer-sized slot.
    pub after_effects: Option<Box<ParsingStateDelta<L>>>,
}

// Manual impls: derives would demand `L: Debug`/`L: Clone`, but the state rides behind
// an `Arc` and `ParsingState` is `Debug` for every `L: Lang`.
impl<L: Lang> fmt::Debug for NodesOutcome<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodesOutcome")
            .field("nodes", &self.nodes)
            .field("stop", &self.stop)
            .field("state", &self.state)
            .field("after_effects", &self.after_effects)
            .finish()
    }
}

impl<L: Lang> Clone for NodesOutcome<L> {
    fn clone(&self) -> Self {
        NodesOutcome {
            nodes: self.nodes.clone(),
            stop: self.stop.clone(),
            state: Arc::clone(&self.state),
            after_effects: self.after_effects.clone(),
        }
    }
}

/// The main content loop: parses a sequence of sibling nodes until a stop condition,
/// an unexpected group close, or end of input (pylatexenc's `LatexGeneralNodesParser`
/// plus its nodes collector).
///
/// A tier-2 temporary: constructed with its per-use configuration (the source the token
/// spans refer into, and the [`StopSpec`]), working state in fields, dropped with the
/// frame. The input parsing state is `cx.state` (the caller sets it); sibling deltas
/// returned by invocation parsers are applied internally as the loop proceeds, and the
/// parser itself returns `None` as its pass-through delta (the state-threading
/// convention). The applied deltas are not lost: the outcome exports the loop's live
/// state at the stop ([`NodesOutcome::state`]) for callers that resume content at the
/// stop position, and their merged record ([`NodesOutcome::after_effects`]) for
/// callers that propagate the run's state effects elsewhere (the `\input`
/// `persist_state` composition).
pub struct NodesParser<'p, L: Lang> {
    stop: StopSpec<'p, L>,
    /// Descent-state policy for child constructs (groups and invocations); defaults to
    /// inherit-everywhere.
    child_states: ChildStateSpec<'p, L>,
    nodes: Vec<BuildId>,
    /// The pending maximal chars run (invariant 1), as the pair of stream positions
    /// it spans: extended by `Char` tokens and every token's pre-space, flushed when
    /// a non-`Char` construct starts.
    run: Option<(L::StreamPosition, L::StreamPosition)>,
    /// The merged record of the sibling after-effect deltas applied so far
    /// ([`NodesOutcome::after_effects`]); drained at every return like `nodes`.
    after_effects: Option<Box<ParsingStateDelta<L>>>,
}

impl<'p, L: Lang> NodesParser<'p, L> {
    /// A parser staging the nodes it reads, stopping per `stop`.
    pub fn new(stop: StopSpec<'p, L>) -> NodesParser<'p, L> {
        NodesParser {
            stop,
            child_states: ChildStateSpec::inherit(),
            nodes: Vec::new(),
            run: None,
            after_effects: None,
        }
    }

    /// Replace the descent-state policy (default: inherit everywhere). See
    /// [`ChildStateSpec`].
    pub fn with_child_states(mut self, child_states: ChildStateSpec<'p, L>) -> Self {
        self.child_states = child_states;
        self
    }

    /// Extend the pending run with a token's pre-space (content whitespace joins the
    /// run — invariant 1; pending whitespace with no adjacent chars becomes a
    /// whitespace-only run). A non-contiguous extension can only come from a token
    /// reader breaking the in-order, gap-free token contract — outer-layer input,
    /// reported as the `Err` detail rather than asserted ([§dd-dr:panic-policy]).
    fn take_pre_space(
        &mut self,
        cx: &ParseContext<'_, '_, L>,
        token: &L::Token,
    ) -> Result<(), String> {
        let start = cx.tokens.position_at(token, TokenEdge::StartBeforePreSpace);
        let end = cx.tokens.position_at(token, TokenEdge::Start);
        if start == end {
            return Ok(());
        }
        self.extend_run_to(start, end, "the token's pre-space")
    }

    /// The `Char` arm: pre-space and the character extend the pending run — one
    /// extension over the token's whole extent, from where its pre-space begins to
    /// where it ends (the two coincide when the token has no pre-space). Same
    /// contiguity contract (and `Err` reporting) as
    /// [`take_pre_space`](Self::take_pre_space).
    fn extend_run(
        &mut self,
        cx: &ParseContext<'_, '_, L>,
        token: &L::Token,
    ) -> Result<(), String> {
        let start = cx.tokens.position_at(token, TokenEdge::StartBeforePreSpace);
        let end = cx.tokens.position_at(token, TokenEdge::EndPastPostSpace);
        self.extend_run_to(start, end, "the char token with its pre-space")
    }

    /// The shared run extension: `start..end` must begin exactly where the pending
    /// run ends, or the token reader broke the in-order, gap-free token contract —
    /// outer-layer input, reported as the `Err` detail rather than asserted
    /// ([§dd-dr:panic-policy]).
    fn extend_run_to(
        &mut self,
        start: L::StreamPosition,
        end: L::StreamPosition,
        what: &str,
    ) -> Result<(), String> {
        match &mut self.run {
            Some((_, run_end)) => {
                if *run_end != start {
                    return Err(alloc::format!(
                        "{what} starts at {:?}, which is not where the pending chars \
                         run ends ({:?}) (the token reader broke the in-order, \
                         gap-free token contract)",
                        start,
                        run_end
                    ));
                }
                *run_end = end;
            }
            None => self.run = Some((start, end)),
        }
        Ok(())
    }

    /// Flush the pending run as a `Chars` node (span-backed over the exact run slice).
    /// Returns whether the node stop condition fired on it.
    fn flush(&mut self, cx: &mut ParseContext<'_, '_, L>) -> ConstructParserResult<L, bool> {
        match self.run.take() {
            Some((start, end)) => {
                let span = cx.source_span_within(&start, &end)?;
                self.stage_node(cx, NodeKind::chars(span.span()), span)
            }
            None => Ok(false),
        }
    }

    /// Extend the pending run with `pre_space`, then flush it: the path taken when a
    /// non-`Char` construct starts (invariant 1). A matched *stop* token's flush goes
    /// through [`flush_for_token_stop`](Self::flush_for_token_stop) instead — same
    /// staging, no node-condition test.
    fn flush_through(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        token: &L::Token,
    ) -> ConstructParserResult<L, bool> {
        self.take_pre_space(cx, token).map_err(|detail| {
            let span = cx.tokens.source_span_between(
                token,
                TokenEdge::StartBeforePreSpace,
                TokenEdge::Start,
            );
            cx.implementation_error(detail, span)
        })?;
        self.flush(cx)
    }

    /// [`flush_through`](Self::flush_through) minus the node-condition test: the flush
    /// performed when the token stop condition has matched. The stop token's pre-space
    /// is interior content and must land in a sibling node (partition invariant), but
    /// the token condition has already ended the parse and wins outright: a
    /// node-condition match here could not change the outcome, and honoring it instead
    /// would leave a `consume = true` stop token unconsumed, forfeiting the consume
    /// flag's atomicity guarantee. The predicate is a stateful `FnMut`, so even a
    /// consulted-but-ignored call would be an observable side effect — it is not
    /// consulted at all.
    fn flush_for_token_stop(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        token: &L::Token,
    ) -> ConstructParserResult<L, ()> {
        self.take_pre_space(cx, token).map_err(|detail| {
            let span = cx.tokens.source_span_between(
                token,
                TokenEdge::StartBeforePreSpace,
                TokenEdge::Start,
            );
            cx.implementation_error(detail, span)
        })?;
        if let Some((start, end)) = self.run.take() {
            let span = cx.source_span_within(&start, &end)?;
            self.stage(cx, NodeKind::chars(span.span()), span)?;
        }
        Ok(())
    }

    /// Stage a childless node under the current state and record it as a sibling —
    /// without testing the node stop condition (that is [`stage_node`](Self::stage_node)'s
    /// job; the token-stop flush stages through this directly).
    fn stage(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> ConstructParserResult<L, BuildId> {
        let id = cx
            .stage_node(kind, span.clone(), Arc::clone(&cx.state), vec![])
            .map_err(|error| cx.staging_error(error, span))?;
        self.nodes.push(id);
        Ok(id)
    }

    /// Stage a childless node ([`stage`](Self::stage)) and test the node stop condition
    /// on it.
    fn stage_node(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> ConstructParserResult<L, bool> {
        let id = self.stage(cx, kind, span)?;
        self.test_node_stop(cx, id)
    }

    /// Test the node stop condition against an already-recorded sibling (the last entry
    /// of `self.nodes` — staged either by [`stage`](Self::stage) or by a child construct
    /// parser). A condition `Err` aborts under any policy ([`StopSpec::node`]'s
    /// contract), with the live traceback attached here — the callback has no
    /// session access.
    fn test_node_stop(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        id: BuildId,
    ) -> ConstructParserResult<L, bool> {
        let Some(condition) = &mut self.stop.node else {
            return Ok(false);
        };
        let staged = cx.staged_nodes();
        // A miss means a child construct parser handed back a foreign id (an
        // implementation bug — the builder diagnoses it when the id lands in a child
        // list); treat it as "condition did not fire" rather than panic (panic policy).
        let Some(view) = staged.get(id) else {
            return Ok(false);
        };
        condition(self.nodes.len(), view).map_err(|error| cx.attach_hook_frames(error))
    }

    /// If the token stop condition matches the peeked token, whether it is to be consumed
    /// ([`TokenStopCondition::consume`]); `None` when no token condition matches. A
    /// predicate `Err` aborts under any policy ([`TokenStopKind::Predicate`]'s
    /// contract); the caller attaches the live traceback.
    fn token_stop(
        &self,
        state: &ParsingState<L>,
        token: &L::Token,
        token_kind: TokenKind<'_, L>,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Result<Option<bool>, ParseError<L::SourceOrigin>> {
        let Some(cond) = self.stop.token.as_ref() else {
            return Ok(None);
        };
        let matches = match &cond.kind {
            TokenStopKind::Command { name } => {
                matches!(token_kind, TokenKind::Command { name: n, .. } if n == *name)
            }
            // Both the spelling and the state-resolved class must match the pairing the
            // group opened with (a `]` sharing the class, or a `}` a delta re-classed,
            // must not close it).
            TokenStopKind::GroupClose { group_type, close } => match token_kind {
                TokenKind::GroupClose { delim } => {
                    delim == *close && group_close_type(state, delim) == Some(*group_type)
                }
                _ => false,
            },
            TokenStopKind::ParagraphBreak => {
                matches!(token_kind, TokenKind::ParagraphBreak)
            }
            TokenStopKind::Predicate(predicate) => predicate(token, tokens)?,
        };
        Ok(matches.then_some(cond.consume))
    }

    /// The shared tolerant recovery of the not-yet-wired arms (`Command` until
    /// resolution dispatch lands in 6.4, `Specials` likewise, plus recovery-placeholder
    /// `GroupOpen` tokens) — and the decided unresolvable-command recovery:
    /// flush, record the condition (or abort under strict), consume the token, and stage
    /// a chars fallback node over its full span. For a `Command` token the span includes
    /// its post-space, which the fallback deliberately swallows (consuming the token
    /// without its post-space would desynchronize [`TokenListReader`]'s fixed list —
    /// its module docs flag exactly this — and the fallback is a diagnosed artifact
    /// anyway). Returns whether a stop condition fired.
    ///
    /// [`TokenListReader`]: crate::token::TokenListReader
    fn recover_as_chars<'s>(
        &mut self,
        cx: &mut ParseContext<'_, 's, L>,
        token: &L::Token,
        recovered: bool,
        condition: impl DiagnosticInfo,
    ) -> ConstructParserResult<L, bool> {
        if self.flush_through(cx, token)? {
            if !recovered {
                cx.tokens.move_to(token, TokenEdge::Start);
            }
            return Ok(true);
        }
        let span = cx.tokens.source_span_of(token);
        cx.recover(condition, span.clone())?;
        if !recovered {
            cx.tokens.move_to(token, TokenEdge::EndPastPostSpace);
        }
        self.stage_node(cx, NodeKind::chars(span.span()), span)
    }

    /// Dispatch a resolved invocation (the `Command`/`Specials` arms): resolve the
    /// descent base state through the [`invocation`](ChildStateSpec::invocation) policy
    /// (resolution already ran under the loop's own state — resolution precedes policy), consume the trigger token **whole** (syntactic post-space included —
    /// mirroring the `GroupOpen` arm, so loop progress holds by construction), run the
    /// spec's invocation parser under the policy state (structural swap/revert), record
    /// the staged node, and apply the parser's after-effect delta to the loop's own
    /// state for subsequent siblings (`\newcommand` — session-mediated, so the
    /// transition is observed). Returns whether the node stop condition fired.
    fn dispatch_invocation<'s>(
        &mut self,
        cx: &mut ParseContext<'_, 's, L>,
        invocation: Invocation<'_, L>,
    ) -> ConstructParserResult<L, bool>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        // `Arc` in, `Arc` out — pass-through policies preserve pointer identity.
        // A failing Compute aborts under any policy (the hook fallibility
        // contract), with the live traceback attached here — the callback has no
        // session access.
        let base = match &self.child_states.invocation {
            InvocationChildState::Inherit => Arc::clone(&cx.state),
            InvocationChildState::Fixed(state) => Arc::clone(state),
            InvocationChildState::Compute(compute) => compute(&cx.state, &invocation)
                .map_err(|error| cx.attach_hook_frames(error))?,
        };
        // The invocation's traceback frame — built before the `Invocation` moves into
        // the factory, pushed around the factory call and the parser run alike (the
        // dispatch push site, [§dd-dr:errors]): a failing factory's traceback names
        // the failing spec too.
        let frame = invocation_frame(cx, &invocation);
        cx.tokens.move_to(invocation.token, TokenEdge::EndPastPostSpace);
        let driver = cx.driver;
        let result = cx.with_frame(frame, |cx| {
            // The parser comes from the driver's interception seam (default: the
            // spec's own factory) — Phase 7.2. A factory Err aborts under any
            // policy ("could not build the parser"), with the live traceback
            // attached here.
            let mut parser = driver
                .make_invocation_parser(invocation)
                .map_err(|error| cx.attach_hook_frames(error))?;
            // The frame is already live — `parse_construct` gets `None`, not a
            // second copy of it.
            let result = cx.parse_construct(&mut *parser, Some(base), None);
            drop(parser);
            result
        });
        let (id, delta) = result?;
        self.nodes.push(id);
        if let Some(delta) = delta {
            // The after-effect applies to the loop's own state, not the policy base
            // (decided semantics 4, [§dd-dr:parsers-engine] — [§dd-dr:parsing-state]'s outward propagation blesses applying
            // a delta to a base the producer never saw) — and its effective,
            // as-applied form joins the run's merged record
            // ([`NodesOutcome::after_effects`]).
            cx.state = cx.derive_state_recording(&delta, &mut self.after_effects)?;
        }
        self.test_node_stop(cx, id)
    }

    /// Drain the collected siblings (and the merged after-effect record) into the
    /// outcome.
    fn outcome(&mut self, state: &Arc<ParsingState<L>>, stop: StopCause<L>) -> NodesOutcome<L> {
        NodesOutcome {
            nodes: mem::take(&mut self.nodes),
            stop,
            state: Arc::clone(state),
            // `Some` yet empty would be a pathological construct's empty after-effect
            // delta; the exported spelling for "no after-effects" is `None`.
            after_effects: self.after_effects.take().filter(|delta| !delta.is_empty()),
        }
    }
}

impl<L: Lang> ConstructParser<L> for NodesParser<'_, L>
where
    L::InvocationSyntax: FromInvocation<L>,
{
    type Output = NodesOutcome<L>;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (NodesOutcome<L>, Option<Box<ParsingStateDelta<L>>>)> {
        loop {
            // Read one token. On a tokenizer error: strict mode aborts, tolerant mode
            // records the diagnostic and adopts the error's recovery — the placeholder
            // token below, with the reader repositioned to the explicit resume position
            // (so the error is never re-read; `recovered` marks that the reader must
            // not be moved for this token).
            let (token, recovered) = match cx.tokens.peek(&cx.state) {
                Ok(token) => (token, false),
                Err(error) => {
                    let kind = error.kind().clone();
                    // The error's location is already source-qualified: only the reader
                    // knows which source it was reading.
                    let span = error.span().clone();
                    match error.into_recovery() {
                        None => {
                            return Err(ParseError::from_token_error(kind, span)
                                .with_frames(cx.session.snapshot_frames()))
                        }
                        Some(recovery) => {
                            // The lift boxes the built-in token conditions and unwraps
                            // a `Custom` payload (never double-boxed) — [§dd-dr:errors].
                            cx.recover_boxed(kind.clone().into_condition(), span.clone())?;
                            // The recovery arm is the one arm that consumes no token,
                            // so loop progress rests entirely on the resume position
                            // moving the stream (the `TokenRecovery::resume`
                            // contract). A violating token source — a custom reader —
                            // would otherwise re-read the same error forever; degrade
                            // the hang into an abort, even in tolerant mode: its
                            // promise is a best-effort tree, not tolerance of
                            // non-termination. Stream positions compare only for
                            // equality, so the check is "different", not "greater".
                            let before = cx.tokens.position_here();
                            cx.tokens.move_to_position(&recovery.resume);
                            if cx.tokens.position_here() == before {
                                return Err(ParseError::from_token_error(kind, span)
                                    .with_frames(cx.session.snapshot_frames()));
                            }
                            (recovery.token, true)
                        }
                    }
                }
            };

            // Token stop condition — consulted for cleanly read tokens only: a recovery
            // placeholder is processed as content (its site already diagnosed it, and a
            // stop token that cannot be re-read cannot be left for the caller). A
            // predicate Err aborts under any policy, the live traceback attached
            // here — the callback has no session access.
            // The one query per iteration: what this token is. Where it is stays a
            // separate reader answer, asked for by the arms that need it.
            let kind = cx.tokens.token_kind(&token);

            if !recovered {
                if let Some(consume) = self
                    .token_stop(&cx.state, &token, kind, &*cx.tokens)
                    .map_err(|error| cx.attach_hook_frames(error))?
                {
                    self.flush_for_token_stop(cx, &token)?;
                    let span = cx.tokens.source_span_of(&token);
                    let after = cx.tokens.position_at(&token, TokenEdge::EndPastPostSpace);
                    if consume {
                        // Take the whole token, syntactic post-space included; its
                        // pre-space is already housed in the flushed sibling nodes.
                        cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                    } else {
                        cx.tokens.move_to(&token, TokenEdge::Start);
                    }
                    return Ok((
                        self.outcome(&cx.state, StopCause::TokenCondition { span, after }),
                        None,
                    ));
                }
            }

            match kind {
                TokenKind::Char(_) => {
                    self.extend_run(cx, &token).map_err(|detail| {
                        let span = cx.tokens.source_span_of(&token);
                        cx.implementation_error(detail, span)
                    })?;
                    if !recovered {
                        cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                    }
                }

                TokenKind::EndOfStream => {
                    // Invariant 4: the terminal token's pre-space is the input's
                    // trailing whitespace and reaches the tree.
                    let fired = self.flush_through(cx, &token)?;
                    if !recovered {
                        cx.tokens.move_to(&token, TokenEdge::Start);
                    }
                    let cause =
                        if fired { StopCause::NodeCondition } else { StopCause::EndOfInput };
                    return Ok((self.outcome(&cx.state, cause), None));
                }

                TokenKind::GroupClose { .. } => {
                    // A close the stop condition did not ask for: report it as data and
                    // let the caller decide ([§dd-dr:panic-policy] rule 2); the token stays unconsumed.
                    let fired = self.flush_through(cx, &token)?;
                    if !recovered {
                        cx.tokens.move_to(&token, TokenEdge::Start);
                    }
                    let cause = if fired {
                        StopCause::NodeCondition
                    } else {
                        StopCause::UnexpectedGroupClose {
                            span: cx.tokens.source_span_of(&token),
                            after: cx
                                .tokens
                                .position_at(&token, TokenEdge::EndPastPostSpace),
                        }
                    };
                    return Ok((self.outcome(&cx.state, cause), None));
                }

                TokenKind::ParagraphBreak => {
                    // Impossible under a language that declares paragraphs absent:
                    // the token source violated its contract (`TokenReader` docs) —
                    // an implementation bug aborts under any policy, never a panic.
                    if !<L::Features as LangFeatures>::Paragraphs::PRESENT {
                        return Err(cx.implementation_error(
                            "a ParagraphBreak token reached content dispatch although \
                             the language declares the paragraphs feature absent \
                             (token-source contract violation)",
                            cx.tokens.source_span_of(&token),
                        ));
                    }
                    if self.flush_through(cx, &token)? {
                        if !recovered {
                            cx.tokens.move_to(&token, TokenEdge::Start);
                        }
                        return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                    }
                    // Invariant 2: the break is its own node — the hook's kind, staged
                    // by the loop over the full token span (a driver cannot stage nodes
                    // itself); runs never merge across it. The hook receives the
                    // break's span so a callable-shaped kind can record the break's
                    // actual spelling (name-as-written).
                    let span = cx.tokens.source_span_of(&token);
                    let kind = cx.driver.make_paragraph_break_node(&cx.state, &span);
                    if !recovered {
                        cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                    }
                    if self.stage_node(cx, kind, span)? {
                        return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                    }
                }

                TokenKind::Comment { .. } => {
                    // Impossible under a language that declares comments absent: the
                    // token source violated its contract (`TokenReader` docs) — an
                    // implementation bug aborts under any policy, never a panic.
                    if !<L::Features as LangFeatures>::Comments::PRESENT {
                        return Err(cx.implementation_error(
                            "a Comment token reached content dispatch although the \
                             language declares the comments feature absent \
                             (token-source contract violation)",
                            cx.tokens.source_span_of(&token),
                        ));
                    }
                    if self.flush_through(cx, &token)? {
                        if !recovered {
                            cx.tokens.move_to(&token, TokenEdge::Start);
                        }
                        return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                    }
                    let (kind, span) = comment_node_kind(cx, &token);
                    if !recovered {
                        cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                    }
                    if self.stage_node(cx, kind, span)? {
                        return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                    }
                }

                TokenKind::Command { name, escape_char } => {
                    // Impossible under a language that declares commands absent: the
                    // token source violated its contract (`TokenReader` docs) — an
                    // implementation bug aborts under any policy, never a panic.
                    if !<L::Features as LangFeatures>::Commands::PRESENT {
                        return Err(cx.implementation_error(
                            "a Command token reached content dispatch although the \
                             language declares the commands feature absent \
                             (token-source contract violation)",
                            cx.tokens.source_span_of(&token),
                        ));
                    }
                    // Resolution runs under the loop's own state — coherent with the
                    // state that tokenized the token (resolution precedes policy,
                    // [§dd-dr:parsers-engine]). A recovery placeholder is never dispatched: its site
                    // already diagnosed it, and a token with no real bytes behind it
                    // cannot be consumed by the arm (no detail: the hook never ran).
                    let resolved = if recovered {
                        CommandResolution::Unresolved { detail: None }
                    } else {
                        // A hook Err aborts under any policy (resolve_command's
                        // contract); the recoverable channels are the Ok values.
                        cx.driver
                            .resolve_command(&cx.state, &token, &*cx.tokens)
                            .map_err(|error| cx.attach_hook_frames(error))?
                    };
                    match resolved {
                        CommandResolution::Resolved(resolved) => {
                            if self.flush_through(cx, &token)? {
                                cx.tokens.move_to(&token, TokenEdge::Start);
                                return Ok((
                                    self.outcome(&cx.state, StopCause::NodeCondition),
                                    None,
                                ));
                            }
                            let invocation = Invocation {
                                callable_type: resolved.callable_type,
                                name,
                                spec: &resolved.spec,
                                token: &token,
                            };
                            if self.dispatch_invocation(cx, invocation)? {
                                return Ok((
                                    self.outcome(&cx.state, StopCause::NodeCondition),
                                    None,
                                ));
                            }
                        }
                        CommandResolution::Unresolved { detail } => {
                            // Unresolvable command ([§dd-dr:errors]): diagnostic plus span-backed
                            // chars fallback.
                            let condition =
                                UnresolvableCommand::new(name, escape_char, detail);
                            if self.recover_as_chars(cx, &token, recovered, condition)? {
                                return Ok((
                                    self.outcome(&cx.state, StopCause::NodeCondition),
                                    None,
                                ));
                            }
                        }
                        CommandResolution::Failed { detail } => {
                            // Operational resolver failure ([§dd-dr:errors]): a distinct condition
                            // from a clean miss, same span-backed chars recovery.
                            let condition =
                                CommandResolutionFailed::new(name, escape_char, detail);
                            if self.recover_as_chars(cx, &token, recovered, condition)? {
                                return Ok((
                                    self.outcome(&cx.state, StopCause::NodeCondition),
                                    None,
                                ));
                            }
                        }
                    }
                }

                TokenKind::Specials { callable_type, name, spec } => {
                    // Impossible under a language that declares specials absent: the
                    // token source violated its contract (`TokenReader` docs) — an
                    // implementation bug aborts under any policy, never a panic.
                    if !<L::Features as LangFeatures>::Specials::PRESENT {
                        return Err(cx.implementation_error(
                            "a Specials token reached content dispatch although the \
                             language declares the specials feature absent \
                             (token-source contract violation)",
                            cx.tokens.source_span_of(&token),
                        ));
                    }
                    // Recognition = resolution: the token carries the full resolution
                    // (callable type + spec). Recovery placeholders are never
                    // dispatched, as for commands.
                    if recovered {
                        let condition = UnusableRecoveryToken::new(
                            name,
                            UnusableRecoveryTokenKind::Specials,
                        );
                        if self.recover_as_chars(cx, &token, recovered, condition)? {
                            return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                        }
                        continue;
                    }
                    if self.flush_through(cx, &token)? {
                        cx.tokens.move_to(&token, TokenEdge::Start);
                        return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                    }
                    let invocation =
                        Invocation { callable_type, name, spec, token: &token };
                    if self.dispatch_invocation(cx, invocation)? {
                        return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                    }
                }

                TokenKind::GroupOpen { delim, rule } => {
                    // Impossible under a language that declares groups absent: the
                    // token source violated its contract (`TokenReader` docs) — an
                    // implementation bug aborts under any policy, never a panic.
                    if !<L::Features as LangFeatures>::Groups::PRESENT {
                        return Err(cx.implementation_error(
                            "a GroupOpen token reached content dispatch although the \
                             language declares the groups feature absent \
                             (token-source contract violation)",
                            cx.tokens.source_span_of(&token),
                        ));
                    }
                    // A recovery placeholder GroupOpen (no current TokenRecovery emits
                    // one) has no real bytes behind it and cannot be parsed as a group:
                    // chars fallback, reader untouched.
                    if recovered {
                        let condition = UnusableRecoveryToken::new(
                            delim,
                            UnusableRecoveryTokenKind::GroupOpen,
                        );
                        if self.recover_as_chars(cx, &token, recovered, condition)? {
                            return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                        }
                        continue;
                    }
                    if self.flush_through(cx, &token)? {
                        cx.tokens.move_to(&token, TokenEdge::Start);
                        return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                    }
                    // The interior's *base* state per the descent policy (the group
                    // parser derives expecting_group_close from it); `Arc` in, `Arc`
                    // out — pass-through policies preserve pointer identity. A
                    // failing Compute aborts under any policy (the hook fallibility
                    // contract), with the live traceback attached here.
                    let base = match &self.child_states.group {
                        GroupChildState::Inherit => Arc::clone(&cx.state),
                        GroupChildState::Fixed(state) => Arc::clone(state),
                        GroupChildState::Compute(compute) => {
                            compute(&cx.state, &token, &*cx.tokens)
                                .map_err(|error| cx.attach_hook_frames(error))?
                        }
                    };
                    // Consume the trigger token here, at the site that peeked it and
                    // under the state that tokenized it (the at-match-time atomicity
                    // rule); the group parser gets its two facts — open span and rule.
                    cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                    // The parser's input state is the policy's answer, scoped to the
                    // descent; the parser itself comes from the driver's factory
                    // (Phase 7.2 uniform routing).
                    let (id, _delta) = cx.parse_group(
                        base,
                        &token,
                        Arc::clone(rule),
                        ChildStateSpec::inherit(),
                        None,
                    )?; // groups have no after-effect
                    self.nodes.push(id);
                    if self.test_node_stop(cx, id)? {
                        return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
                    }
                }
            }
        }
    }
}

/// The group class a close delimiter belongs to under `state`'s rules: the expected
/// close takes precedence (mirroring the tokenizer's priority in
/// `detect_group_delimiter`), then the delimiter table. `None` when the delimiter
/// belongs to no close rule in scope.
fn group_close_type<L: Lang>(state: &ParsingState<L>, delim: &str) -> Option<L::GroupTypeId> {
    if let Some(rule) = state.rules().expecting_group_close() {
        if rule.close == delim {
            return Some(rule.group_type);
        }
    }
    state
        .prefix_table() // `None` when the language declares the groups feature absent
        .and_then(|table| table.match_at(delim))
        .filter(|entry| entry.delim() == delim)
        .and_then(|entry| entry.close())
        .map(|rule| rule.group_type)
}

// Manual impls: predicates have no useful Debug, and derives would demand `L:` bounds.

impl<L: Lang> fmt::Debug for TokenStopKind<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenStopKind::Command { name } => {
                f.debug_struct("Command").field("name", name).finish()
            }
            TokenStopKind::GroupClose { group_type, close } => f
                .debug_struct("GroupClose")
                .field("group_type", group_type)
                .field("close", close)
                .finish(),
            TokenStopKind::ParagraphBreak => write!(f, "ParagraphBreak"),
            TokenStopKind::Predicate(_) => write!(f, "Predicate(..)"),
        }
    }
}

impl<L: Lang> fmt::Debug for TokenStopCondition<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenStopCondition")
            .field("kind", &self.kind)
            .field("consume", &self.consume)
            .finish()
    }
}

impl<L: Lang> fmt::Debug for StopSpec<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StopSpec")
            .field("token", &self.token)
            .field("node", &self.node.as_ref().map(|_| "FnMut(..)"))
            .finish()
    }
}

impl<L: Lang> fmt::Debug for NodesParser<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodesParser")
            .field("stop", &self.stop)
            .field("nodes", &self.nodes.len())
            .field("run", &self.run)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;
    use crate::engine::{resolve_command_in_scopes, ParseResult, ParserSession, StdParseDriver};
    use crate::error::Recovery;
    use crate::scopes::{
        CallableQuery, CallableSyntax, Package, ProviderError, ScopeStack, SpecsProvider,
    };
    use crate::node::GroupData;
    use crate::source::{Source, SourcePos, TextContent};
    use crate::spec::{CallableSpec, StdCallableSpec};
    use super::super::{InvocationChildState, StdInvocationParser};
    use crate::state::{
        CommentOverrides, NodeExtTypes, TrivialLang, StateData, TokenRulesOverrides,
    };
    use crate::token::{
        CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
        GroupRules, ParagraphRules, SpecialsMatch, SpecialsRules, SpecialsScanError,
        StdStreamPosition, StdToken, StdTokenReader, TokenEdge, TokenError,
        TokenErrorKind, TokenKind, TokenListReader, TokenReader,
        TokenRecovery, TokenResult, TokenRules, TriggerChars, WhitespaceRules,
    };
    use alloc::boxed::Box;
    use alloc::string::ToString;

    const GT_BRACE: u32 = 0;
    const GT_MATH: u32 = 1;
    const CT_MACRO: u32 = 10;
    const CT_SPECIALS: u32 = 11;

    #[derive(Debug, Clone, Copy)]
    struct TestLang;
    impl TrivialLang for TestLang {}

    /// The preset resolution pattern, shared by the 6.4 test langs: dispatch a
    /// `Command` token to the state's scope stack under the `CT_MACRO` form. Delegates
    /// to the standard [`resolve_command_in_scopes`] so the test langs and
    /// the latexlike preset share one query-and-dispatch implementation.
    fn resolve_macro_in_scopes<L: Lang<CallableTypeId = u32>>(
        state: &ParsingState<L>,
        token: &L::Token,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Result<CommandResolution<L>, ParseError<L::SourceOrigin>> {
        Ok(resolve_command_in_scopes(state, token, tokens, CT_MACRO))
    }

    /// Test-side driver factory: the generic run helpers construct each lang's
    /// driver from the recovery setting alone (drivers carry the policy).
    trait TestDriver {
        fn with_recovery(recovery: Recovery) -> Self;
    }

    impl TestDriver for StdParseDriver {
        fn with_recovery(recovery: Recovery) -> Self {
            StdParseDriver::new(recovery, ())
        }
    }

    /// Test lang resolving `Command` tokens against the state's libraries under the
    /// `CT_MACRO` form (the hook lives on its driver since 7.2).
    #[derive(Debug, Clone, Copy)]
    struct CmdLang;
    impl Lang for CmdLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = CmdDriver;
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
    struct CmdDriver {
        recovery: Recovery,
    }

    impl TestDriver for CmdDriver {
        fn with_recovery(recovery: Recovery) -> Self {
            CmdDriver { recovery }
        }
    }

    impl ParseDriver<CmdLang> for CmdDriver {
        fn make_token_reader<'s>(
            &'s self,
            source: &'s alloc::sync::Arc<crate::source::Source>,
        ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, CmdLang> + 's> {
            alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
        }

        fn recovery(&self) -> Recovery {
            self.recovery
        }

        fn resolve_command(
            &self,
            state: &ParsingState<CmdLang>,
            token: &StdToken<CmdLang>,
            tokens: &dyn TokenReader<'_, CmdLang>,
        ) -> Result<CommandResolution<CmdLang>, ParseError> {
            resolve_macro_in_scopes(state, token, tokens)
        }
    }

    /// Test lang recognizing `~` as a specials trigger resolving to a zero-arg spec
    /// under the `CT_SPECIALS` form.
    #[derive(Debug, Clone, Copy)]
    struct TildeLang;
    impl Lang for TildeLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn scan_specials(
            _state: &ParsingState<Self>,
            content: &str,
            pos: usize,
        ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
            if content[pos..].starts_with('~') {
                Ok(Some(SpecialsMatch {
                    end: pos + 1,
                    callable_type: CT_SPECIALS,
                    spec: Arc::new(StdCallableSpec::default()),
                }))
            } else {
                Ok(None)
            }
        }

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~".into())
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

    /// A library defining each of `names` as a zero-arg `CT_MACRO` callable (one shared
    /// spec — flyweight).
    fn macro_library<L: Lang<CallableTypeId = u32> + 'static>(names: &[&str]) -> Arc<Package<L>> {
        let mut lib = Package::new("test-macros");
        let spec: Arc<dyn CallableSpec<L>> = Arc::new(StdCallableSpec::default());
        for name in names {
            lib.insert(CT_MACRO, *name, Arc::clone(&spec));
        }
        Arc::new(lib)
    }

    /// A `CmdLang` state whose library stack defines `names` as zero-arg macros.
    fn state_with_macros(names: &[&str]) -> Arc<ParsingState<CmdLang>> {
        let mut scopes = ScopeStack::new();
        scopes.push(macro_library(names));
        Arc::new(ParsingState::new(StateData { rules: rules(), scopes, mode: (), ext: () }))
    }

    fn math_rule<L: Lang<GroupTypeId = u32>>() -> Arc<GroupRule<L>> {
        Arc::new(GroupRule { group_type: GT_MATH, open: "$".into(), close: "$".into() })
    }

    // `Features = AllLangFeatures` (all test languages here declare it): the plain
    // block literals below only typecheck once the per-feature stores normalize to
    // the blocks themselves.
    fn rules<L: Lang<GroupTypeId = u32, Features = crate::state::AllLangFeatures>>(
    ) -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: vec![Arc::new(GroupRule {
                    group_type: GT_BRACE,
                    open: "{".into(),
                    close: "}".into(),
                })],
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: vec![Arc::new(CommandRule {
                    escape_char: '\\',
                    name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
                })],
            },
            comments: CommentRules {
                enabled: true,
                rules: vec![Arc::new(CommentRule { start: "%".into() })],
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    fn state_with<L: Lang<GroupTypeId = u32, StateExt = ()>>(
        rules: TokenRules<L>,
    ) -> Arc<ParsingState<L>> {
        Arc::new(ParsingState::new(StateData {
            rules,
            scopes: ScopeStack::new(),
            mode: Default::default(),
            ext: (),
        }))
    }

    fn state() -> Arc<ParsingState<TestLang>> {
        state_with(rules())
    }

    // --- harness ------------------------------------------------------------------------

    struct Parsed<L: Lang> {
        result: ParseResult<L>,
        stop: StopCause<L>,
        /// The reader's exit position, as a byte offset (for assertions).
        pos: usize,
        /// The reader's exit position itself — what a test resumes a fresh reader
        /// over the same content from.
        position: L::StreamPosition,
    }

    /// A stop cause rendered for assertions: the variant, plus the matched span's
    /// byte range where the variant carries one. (The `after` position is opaque;
    /// tests that care about it resume the reader from it.)
    fn stop_shape<L: Lang>(stop: &StopCause<L>) -> String {
        match stop {
            StopCause::TokenCondition { span, .. } => {
                alloc::format!("token {:?}", span.range())
            }
            StopCause::UnexpectedGroupClose { span, .. } => {
                alloc::format!("close {:?}", span.range())
            }
            StopCause::NodeCondition => String::from("node"),
            StopCause::EndOfInput => String::from("end of input"),
        }
    }

    impl<L: Lang> fmt::Debug for Parsed<L> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Parsed")
                .field("shapes", &shapes(&self.result))
                .field("stop", &self.stop)
                .field("pos", &self.pos)
                .field("position", &self.position)
                .finish()
        }
    }

    /// Drive a `NodesParser` over `tokens`, stage the outcome under a root `List`
    /// spanning exactly the parsed extent, freeze, and run the invariant checker over
    /// the finished tree. The reader must be reading `content`.
    fn try_run<'s, L: Lang<SourceOrigin = Option<String>>>(
        source: &'s Arc<Source>,
        tokens: &mut dyn TokenReader<'s, L>,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop: StopSpec<'_, L>,
    ) -> Result<Parsed<L>, ParseError>
    where
        L::Driver: TestDriver,
        L::InvocationSyntax: FromInvocation<L>,
    {
        try_run_with(source, tokens, state, recovery, stop, ChildStateSpec::inherit())
    }

    /// [`try_run`] with an explicit descent-state policy.
    fn try_run_with<'s, 'p, L: Lang<SourceOrigin = Option<String>>>(
        source: &'s Arc<Source>,
        tokens: &mut dyn TokenReader<'s, L>,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop: StopSpec<'p, L>,
        child_states: ChildStateSpec<'p, L>,
    ) -> Result<Parsed<L>, ParseError>
    where
        L::Driver: TestDriver,
        L::InvocationSyntax: FromInvocation<L>,
    {
        let mut session = ParserSession::new();
        let driver = L::Driver::with_recovery(recovery);
        let mut cx =
            ParseContext::new(tokens, Arc::clone(state), &mut session, &driver);
        let mut parser = NodesParser::new(stop).with_child_states(child_states);
        let (outcome, delta) = parser.parse(&mut cx)?;
        assert!(delta.is_none(), "NodesParser returns no pass-through delta");
        // The reader's exit position: as a byte offset (for assertions) and as
        // itself (for resuming).
        let position = cx.tokens.position_here();
        let pos = cx.here().start();
        // The root `List` spans exactly the parsed extent (its content interior — the
        // partition invariant the checker verifies); a consumed stop token lies outside.
        let root_span = {
            let staged = session.builder.staged_nodes();
            match (outcome.nodes.first(), outcome.nodes.last()) {
                (Some(&first), Some(&last)) => Span::new(
                    staged.get(first).unwrap().span().start(),
                    staged.get(last).unwrap().span().end(),
                ),
                _ => Span::empty(0),
            }
        };
        let root = {
            // The generic test harness stages its root list via the explicit recipe
            // (the harness plays the transform-author role here).
            let kind = NodeKind::list();
            let span = SourceSpan::new(source, root_span);
            let ext = L::make_node_ext(
                &kind,
                &span,
                state,
                session.builder.staged_children(&outcome.nodes),
            )
            .expect("mint node ext");
            session.builder.add(kind, span, Arc::clone(state), outcome.nodes, ext, ()).unwrap()
        };
        let result = session.finish(root).unwrap();
        crate::node::check_tree_invariants(&result.tree);
        Ok(Parsed { result, stop: outcome.stop, pos, position })
    }

    /// Scan `source` into the full token list (including the terminal `EndOfStream`).
    fn scan<L>(source: &Arc<Source>, state: &Arc<ParsingState<L>>) -> Vec<L::Token>
    where
        L: Lang<
            SourceOrigin = Option<String>,
            Token = StdToken<L>,
            StreamPosition = StdStreamPosition,
        >,
    {
        let mut reader = StdTokenReader::new(source);
        let mut tokens = Vec::new();
        loop {
            let token = TokenReader::next(&mut reader, state).expect("clean scan");
            let reader_ref: &dyn TokenReader<'_, L> = &reader;
            let done = matches!(reader_ref.token_kind(&token), TokenKind::EndOfStream);
            tokens.push(token);
            if done {
                break;
            }
        }
        tokens
    }

    /// Run the same parse against `StdTokenReader` and `TokenListReader` (report R6) and
    /// assert they agree on shapes, stop cause, position, and diagnostics count. The two
    /// stop specs must be equivalent (node predicates are `&mut`, so each run needs its
    /// own).
    fn run_both<'p, L>(
        content: &str,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop_std: StopSpec<'p, L>,
        stop_list: StopSpec<'p, L>,
    ) -> Parsed<L>
    where
        L: Lang<
            SourceOrigin = Option<String>,
            Token = StdToken<L>,
            StreamPosition = StdStreamPosition,
        >,
        L::Driver: TestDriver,
        L::InvocationSyntax: FromInvocation<L>,
    {
        run_both_with(content, state, recovery, stop_std, stop_list, ChildStateSpec::inherit())
    }

    /// [`run_both`] with an explicit descent-state policy (cloned per reader — it is
    /// shallow: `Arc`s and borrows).
    fn run_both_with<'p, L>(
        content: &str,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop_std: StopSpec<'p, L>,
        stop_list: StopSpec<'p, L>,
        child_states: ChildStateSpec<'p, L>,
    ) -> Parsed<L>
    where
        L: Lang<
            SourceOrigin = Option<String>,
            Token = StdToken<L>,
            StreamPosition = StdStreamPosition,
        >,
        L::Driver: TestDriver,
        L::InvocationSyntax: FromInvocation<L>,
    {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut std_reader = StdTokenReader::new(&source);
        let a = try_run_with(
            &source,
            &mut std_reader,
            state,
            recovery,
            stop_std,
            child_states.clone(),
        )
        .expect("std reader");
        let mut list_reader = TokenListReader::new(&source, scan(&source, state));
        let b =
            try_run_with(&source, &mut list_reader, state, recovery, stop_list, child_states)
                .expect("list reader");
        assert_eq!(shapes(&a.result), shapes(&b.result), "readers disagree on {:?}", content);
        assert_eq!(a.stop, b.stop, "stop causes disagree on {:?}", content);
        assert_eq!(a.pos, b.pos, "positions disagree on {:?}", content);
        assert_eq!(a.result.diagnostics.len(), b.result.diagnostics.len());
        a
    }

    /// One readable line per root child: kind, exact span, resolved contents.
    fn shapes<L: Lang>(result: &ParseResult<L>) -> Vec<String> {
        result
            .tree
            .root()
            .children()
            .iter().map(|node| {
                let span = format!("{}..{}", node.span().start(), node.span().end());
                match node.kind() {
                    NodeKind::Chars { .. } => {
                        format!("chars {} {:?}", span, node.chars().unwrap())
                    }
                    NodeKind::Comment(_) => {
                        let data = node.comment().unwrap();
                        format!(
                            "comment {} start={:?} content={:?} post={:?}",
                            span,
                            data.start.resolve(node.source()),
                            data.content.resolve(node.source()),
                            data.post_space.resolve(node.source())
                        )
                    }
                    NodeKind::Group(_) => format!("group {}", span),
                    NodeKind::Callable(_) => format!("callable {}", span),
                    NodeKind::List => format!("list {}", span),
                }
            })
            .collect()
    }

    /// The partition invariant (invariant 5 of
    /// [`check_tree_invariants`](crate::node::check_tree_invariants)): the root's children tile `interior`
    /// exactly — no gaps, no double counting.
    fn assert_partition<L: Lang>(result: &ParseResult<L>, interior: core::ops::Range<usize>) {
        let mut pos = interior.start;
        for child in result.tree.root().children() {
            assert_eq!(child.span().start(), pos, "gap before a sibling: {:?}", shapes(result));
            pos = child.span().end();
        }
        assert_eq!(pos, interior.end, "siblings fall short: {:?}", shapes(result));
    }

    // --- content shapes (invariants 1–4) --------------------------------------------------

    #[test]
    fn empty_input_is_an_empty_list() {
        let st = state();
        let parsed = run_both("", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_eq!(parsed.result.tree.root().child_count(), 0);
        assert_eq!(parsed.pos, 0);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn chars_accumulate_into_one_maximal_node() {
        let st = state();
        let parsed =
            run_both("ab cd", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["chars 0..5 \"ab cd\""]);
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_eq!(parsed.pos, 5);
        assert_partition(&parsed.result, 0..5);
    }

    #[test]
    fn leading_and_trailing_whitespace_joins_the_run() {
        let st = state();
        let parsed =
            run_both("  ab  ", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["chars 0..6 \"  ab  \""]);
        assert_eq!(parsed.pos, 6);
        assert_partition(&parsed.result, 0..6);
    }

    #[test]
    fn whitespace_only_input_is_a_whitespace_chars_node() {
        let st = state();
        let parsed = run_both("  ", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"  \""]);
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_partition(&parsed.result, 0..2);
    }

    #[test]
    fn paragraph_break_gets_its_own_node_runs_do_not_merge_across() {
        let st = state();
        let parsed =
            run_both("a \n \n b", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        // The break token spans first through last newline; the whitespace before it
        // joins the preceding run, the whitespace after it the following run.
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"a \"", "chars 2..5 \"\\n \\n\"", "chars 5..7 \" b\""]
        );
        assert_partition(&parsed.result, 0..7);
    }

    #[test]
    fn paragraph_break_node_comes_from_the_driver_hook() {
        #[derive(Debug, Clone, Copy)]
        struct MarkLang;
        impl Lang for MarkLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = MarkDriver;
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
        struct MarkDriver {
            recovery: Recovery,
        }

        impl TestDriver for MarkDriver {
            fn with_recovery(recovery: Recovery) -> Self {
                MarkDriver { recovery }
            }
        }

        impl ParseDriver<MarkLang> for MarkDriver {
            fn make_token_reader<'s>(
                &'s self,
                source: &'s alloc::sync::Arc<crate::source::Source>,
            ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, MarkLang> + 's> {
                alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
            }

            fn recovery(&self) -> Recovery {
                self.recovery
            }

            fn make_paragraph_break_node(
                &self,
                _state: &ParsingState<MarkLang>,
                _break_span: &SourceSpan,
            ) -> NodeKind<MarkLang> {
                NodeKind::chars("¶") // owned content, unlike the spanned default
            }
        }

        let st = state_with(rules::<MarkLang>());
        let parsed =
            run_both("a\n\nb", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..1 \"a\"", "chars 1..3 \"¶\"", "chars 3..4 \"b\""]
        );
        // The loop stages the hook's kind as-is, over the token's span.
        let break_node = parsed.result.tree.root().child(1).unwrap();
        assert!(matches!(
            break_node.kind(),
            NodeKind::Chars { content: TextContent::Owned(_), .. }
        ));
        assert_partition(&parsed.result, 0..4);
    }

    #[test]
    fn comment_node_records_start_content_and_post_space() {
        let st = state();
        let parsed = run_both(
            "x % note\n  y",
            &st,
            Recovery::Strict,
            StopSpec::none(),
            StopSpec::none(),
        );
        assert_eq!(
            shapes(&parsed.result),
            [
                "chars 0..2 \"x \"",
                "comment 2..11 start=\"%\" content=\" note\" post=\"\\n  \"",
                "chars 11..12 \"y\"",
            ]
        );
        assert_partition(&parsed.result, 0..12);
    }

    #[test]
    fn comment_bordering_a_paragraph_break_has_empty_post_space() {
        let st = state();
        let parsed =
            run_both("x %n\n\ny", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            [
                "chars 0..2 \"x \"",
                "comment 2..4 start=\"%\" content=\"n\" post=\"\"",
                "chars 4..6 \"\\n\\n\"",
                "chars 6..7 \"y\"",
            ]
        );
        assert_partition(&parsed.result, 0..7);
    }

    #[test]
    fn comment_at_end_of_input() {
        let st = state();
        let parsed =
            run_both("x %n", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"x \"", "comment 2..4 start=\"%\" content=\"n\" post=\"\""]
        );
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_partition(&parsed.result, 0..4);
    }

    // --- stop conditions -------------------------------------------------------------------

    #[test]
    fn stop_at_command_leaves_the_token_peekable_with_pre_space_housed() {
        let st = state();
        let content = "ab \\end rest";
        let parsed = run_both(
            content,
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::Command { name: "end" }, false),
            StopSpec::at_token(TokenStopKind::Command { name: "end" }, false),
        );
        // The stop token's pre-space is interior content: it lands in the flushed run.
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"ab \""]);
        // The reported span covers the whole `\end` token (name + terminating space).
        assert_eq!(stop_shape(&parsed.stop), "token 3..8");
        assert_eq!(parsed.pos, 3);

        // Re-peeking from the seam yields the stop token itself, with empty pre-space —
        // no byte is represented twice.
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        TokenReader::<'_, TestLang>::move_to_position(&mut reader, &parsed.position);
        let token: StdToken<TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        let reader: &dyn TokenReader<'_, TestLang> = &reader;
        assert!(matches!(
            reader.token_kind(&token),
            TokenKind::Command { name: "end", .. }
        ));
        assert!(reader
            .source_span_between(&token, TokenEdge::StartBeforePreSpace, TokenEdge::Start)
            .is_empty());
    }

    #[test]
    fn the_stop_causes_after_position_skips_the_unconsumed_token() {
        // An unconsumed stop token sits at its own start; the cause's `after`
        // position is where a caller that skips it resumes — no re-peek needed.
        let st = state();
        let source: Arc<Source> = Arc::new(Source::new("ab}c"));
        let mut reader = StdTokenReader::new(&source);
        let parsed: Parsed<TestLang> =
            try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap();
        let StopCause::UnexpectedGroupClose { span, after } = parsed.stop else {
            panic!("expected an unexpected group close");
        };
        assert_eq!(span.range(), 2..3);
        assert_eq!(span.content(), "}");
        // The close is left unconsumed, at its own start …
        assert_eq!(parsed.pos, 2);
        // … and `after` stands just past it.
        TokenReader::<'_, TestLang>::move_to_position(&mut reader, &after);
        let token: StdToken<TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        let reader_ref: &dyn TokenReader<'_, TestLang> = &reader;
        assert!(matches!(reader_ref.token_kind(&token), TokenKind::Char('c')));
    }

    #[test]
    fn lone_pre_space_before_a_stop_token_becomes_a_whitespace_node() {
        let st = state();
        let parsed = run_both(
            " \\end",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::Command { name: "end" }, false),
            StopSpec::at_token(TokenStopKind::Command { name: "end" }, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..1 \" \""]);
        assert_eq!(stop_shape(&parsed.stop), "token 1..5");
        assert_eq!(parsed.pos, 1);
    }

    #[test]
    fn stop_at_group_close_produced_by_the_delimiter_table() {
        let st = state();
        let parsed = run_both(
            "ab}c",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_BRACE, close: "}" }, false),
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_BRACE, close: "}" }, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(stop_shape(&parsed.stop), "token 2..3");
        assert_eq!(parsed.pos, 2);
    }

    #[test]
    fn stop_at_group_close_produced_by_the_expected_close() {
        // `$` closes only through `expecting_group_close` (ambiguous delimiter read as
        // an opener otherwise) — the 6.3 group parser's configuration, exercised here.
        let mut r = rules::<TestLang>();
        r.groups.rules.push(math_rule());
        r.groups.expecting_close = Some(math_rule());
        let st = state_with(r);
        let parsed = run_both(
            "a b$c",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_MATH, close: "$" }, false),
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_MATH, close: "$" }, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"a b\""]);
        assert_eq!(stop_shape(&parsed.stop), "token 3..4");
        assert_eq!(parsed.pos, 3);
    }

    #[test]
    fn group_close_of_a_different_delimiter_is_unexpected() {
        // The stop condition waits for the math `$` close; a `}` arrives: the delimiter
        // does not match — reported as data, token unconsumed, no diagnostic (the caller
        // decides).
        let mut r = rules::<TestLang>();
        r.groups.rules.push(math_rule());
        r.groups.expecting_close = Some(math_rule());
        let st = state_with(r);
        let parsed = run_both(
            "ab}c",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_MATH, close: "$" }, false),
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_MATH, close: "$" }, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(stop_shape(&parsed.stop), "close 2..3");
        assert_eq!(parsed.pos, 2);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn group_close_of_a_shared_class_but_different_delimiter_is_unexpected() {
        // `[`/`]` and `{`/`}` share the class GT_BRACE. A group opened with `{` must not
        // be closed by a `]`: same class, different delimiter — the `close` field
        // disambiguates within a class.
        let mut r = rules::<TestLang>();
        r.groups.rules.push(Arc::new(GroupRule {
            group_type: GT_BRACE,
            open: "[".into(),
            close: "]".into(),
        }));
        let st = state_with(r);
        let parsed = run_both(
            "ab]c",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_BRACE, close: "}" }, false),
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_BRACE, close: "}" }, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(stop_shape(&parsed.stop), "close 2..3");
        assert_eq!(parsed.pos, 2);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn group_close_reclassified_to_another_class_is_unexpected() {
        // A `{`-opened GT_BRACE group; a state delta has re-classed `}` to close a
        // GT_MATH group (same delimiter, different class). The `}` must not close the
        // GT_BRACE group: `group_type` disambiguates within a delimiter spelling. Modeled
        // by an `expecting_group_close` whose close is `}` but whose class is GT_MATH.
        let mut r = rules::<TestLang>();
        r.groups.expecting_close = Some(Arc::new(GroupRule {
            group_type: GT_MATH,
            open: "{".into(),
            close: "}".into(),
        }));
        let st = state_with(r);
        let parsed = run_both(
            "ab}c",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_BRACE, close: "}" }, false),
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_BRACE, close: "}" }, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(stop_shape(&parsed.stop), "close 2..3");
        assert_eq!(parsed.pos, 2);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn group_close_without_any_stop_condition_is_unexpected() {
        let st = state();
        let parsed =
            run_both("ab}c", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(stop_shape(&parsed.stop), "close 2..3");
        assert_eq!(parsed.pos, 2);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn stop_at_paragraph_break() {
        let st = state();
        let parsed = run_both(
            "ab\n\ncd",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::ParagraphBreak, false),
            StopSpec::at_token(TokenStopKind::ParagraphBreak, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(stop_shape(&parsed.stop), "token 2..4");
        assert_eq!(parsed.pos, 2);
    }

    #[test]
    fn stop_at_a_token_predicate() {
        let st = state();
        // The predicate reads the token through the reader it is handed.
        let predicate = |token: &StdToken<TestLang>,
                         tokens: &dyn TokenReader<'_, TestLang>| {
            Ok(matches!(tokens.token_kind(token), TokenKind::Comment { .. }))
        };
        let parsed = run_both(
            "ab %c\nd",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::Predicate(&predicate), false),
            StopSpec::at_token(TokenStopKind::Predicate(&predicate), false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"ab \""]);
        assert_eq!(stop_shape(&parsed.stop), "token 3..6");
        assert_eq!(parsed.pos, 3);
    }

    #[test]
    fn a_failing_stop_predicate_aborts_under_any_policy() {
        // The hook-fallibility contract on `TokenStopKind::Predicate`: an Err ends
        // the parse even under tolerant recovery — a predicate that cannot answer
        // leaves no sound way to decide where the run ends — and the consultation
        // site attaches the live traceback (the predicate has no session access).
        let st = state();
        let failing = |_: &StdToken<TestLang>,
                       _: &dyn TokenReader<'_, TestLang>|
         -> Result<bool, ParseError> {
            let scratch: Arc<Source> = Arc::new(Source::new(""));
            Err(ParseError::new(
                crate::error::HookFailed::new("stop table unavailable", None),
                SourceSpan::new(&scratch, 0..0),
            ))
        };
        let content = "ab";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let mut session: ParserSession<TestLang> = ParserSession::new();
        let driver: StdParseDriver = StdParseDriver::new(Recovery::Tolerant, ());
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&st),
            &mut session,
            &driver);
        let mut parser = NodesParser::new(StopSpec::at_token(
            TokenStopKind::Predicate(&failing),
            false,
        ));
        let frame = crate::engine::Frame {
            title: crate::engine::FrameTitle::Static("test descent"),
            span: SourceSpan::new(&source, 0..0),
        };
        let error = cx.with_frame(frame, |cx| parser.parse(cx)).unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        assert_eq!(
            error.message(),
            "extension hook reported a failure: stop table unavailable"
        );
        // The live traceback, attached at the consultation site.
        assert_eq!(error.frames().len(), 1);
        assert_eq!(error.frames()[0].title(), "test descent");
    }

    #[test]
    fn consume_flag_swallows_a_command_stop_token_with_its_post_space() {
        let st = state();
        let content = "ab \\end rest";
        let parsed = run_both(
            content,
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::Command { name: "end" }, true),
            StopSpec::at_token(TokenStopKind::Command { name: "end" }, true),
        );
        // Same sibling content as the leave-it case; the command's terminating space is
        // syntactic post-space, taken with the token — so the reader lands past it (8),
        // not at the token start (3).
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"ab \""]);
        assert_eq!(stop_shape(&parsed.stop), "token 3..8");
        assert_eq!(parsed.pos, 8);

        // The next read is the following content, its pre-space empty (nothing re-read).
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        TokenReader::<'_, TestLang>::move_to_position(&mut reader, &parsed.position);
        let token: StdToken<TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        let reader: &dyn TokenReader<'_, TestLang> = &reader;
        assert!(matches!(reader.token_kind(&token), TokenKind::Char('r')));
        assert!(reader
            .source_span_between(&token, TokenEdge::StartBeforePreSpace, TokenEdge::Start)
            .is_empty());
    }

    #[test]
    fn consume_flag_swallows_a_group_close_stop_token() {
        let st = state();
        let parsed = run_both(
            "ab} c",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_BRACE, close: "}" }, true),
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_BRACE, close: "}" }, true),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(stop_shape(&parsed.stop), "token 2..3");
        // The close is consumed (reader past `}`); `}` carries no post-space, so the
        // following space is not the close's — it stays for the enclosing content as the
        // next token's pre-space, unclaimed here.
        assert_eq!(parsed.pos, 3);
    }

    #[test]
    fn node_condition_stops_after_a_flushed_chars_node() {
        let st = state();
        let mut c1 = |count: usize, _: StagedNodeView<'_, TestLang>| Ok(count >= 1);
        let mut c2 = |count: usize, _: StagedNodeView<'_, TestLang>| Ok(count >= 1);
        let parsed = run_both(
            "ab %c\nde",
            &st,
            Recovery::Strict,
            StopSpec { token: None, node: Some(&mut c1) },
            StopSpec { token: None, node: Some(&mut c2) },
        );
        // The comment token triggered the flush; the condition fired on the flushed
        // node, so the comment stays unconsumed at its own start.
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"ab \""]);
        assert_eq!(parsed.stop, StopCause::NodeCondition);
        assert_eq!(parsed.pos, 3);
        assert_partition(&parsed.result, 0..3);
    }

    #[test]
    fn node_condition_stops_after_a_directly_staged_node() {
        let st = state();
        let mut c1 = |count: usize, view: StagedNodeView<'_, TestLang>| {
            Ok(count >= 1 && matches!(view.kind(), NodeKind::Comment { .. }))
        };
        let mut c2 = |count: usize, view: StagedNodeView<'_, TestLang>| {
            Ok(count >= 1 && matches!(view.kind(), NodeKind::Comment { .. }))
        };
        let parsed = run_both(
            "%c\nab",
            &st,
            Recovery::Strict,
            StopSpec { token: None, node: Some(&mut c1) },
            StopSpec { token: None, node: Some(&mut c2) },
        );
        // The triggering node is included, and the parse stops right after it — the
        // comment token was consumed (post-space and all).
        assert_eq!(
            shapes(&parsed.result),
            ["comment 0..3 start=\"%\" content=\"c\" post=\"\\n\""]
        );
        assert_eq!(parsed.stop, StopCause::NodeCondition);
        assert_eq!(parsed.pos, 3);
    }

    #[test]
    fn node_condition_on_the_trailing_whitespace_node_wins_over_end_of_input() {
        let st = state();
        let mut c1 = |count: usize, _: StagedNodeView<'_, TestLang>| Ok(count >= 1);
        let mut c2 = |count: usize, _: StagedNodeView<'_, TestLang>| Ok(count >= 1);
        let parsed = run_both(
            "ab",
            &st,
            Recovery::Strict,
            StopSpec { token: None, node: Some(&mut c1) },
            StopSpec { token: None, node: Some(&mut c2) },
        );
        // The final flush (at end of stream) staged the only node; the node condition
        // fired on it, so the cause is the condition, not EndOfInput.
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(parsed.stop, StopCause::NodeCondition);
    }

    #[test]
    fn a_failing_node_stop_condition_aborts_under_any_policy() {
        // The hook-fallibility contract on `StopSpec::node`: an Err ends the parse
        // even under tolerant recovery — a condition that cannot answer leaves no
        // sound way to decide where the run ends — and the consultation site
        // attaches the live traceback (the callback has no session access).
        let st = state();
        let mut failing =
            |_: usize, _: StagedNodeView<'_, TestLang>| -> Result<bool, ParseError> {
                let scratch: Arc<Source> = Arc::new(Source::new(""));
                Err(ParseError::new(
                    crate::error::HookFailed::new("node table unavailable", None),
                    SourceSpan::new(&scratch, 0..0),
                ))
            };
        let content = "ab";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let mut session: ParserSession<TestLang> = ParserSession::new();
        let driver: StdParseDriver = StdParseDriver::new(Recovery::Tolerant, ());
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&st),
            &mut session,
            &driver);
        let mut parser =
            NodesParser::new(StopSpec { token: None, node: Some(&mut failing) });
        let frame = crate::engine::Frame {
            title: crate::engine::FrameTitle::Static("test descent"),
            span: SourceSpan::new(&source, 0..0),
        };
        let error = cx.with_frame(frame, |cx| parser.parse(cx)).unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        // The live traceback, attached at the consultation site.
        assert_eq!(error.frames().len(), 1);
        assert_eq!(error.frames()[0].title(), "test descent");
    }

    #[test]
    fn token_stop_flush_does_not_consult_the_node_condition() {
        // Both triggers collide: the `\end` match flushes the pending run, and the
        // (always-true) node condition would fire on the flushed node. The token
        // condition wins outright — the run is staged, the predicate is never invoked
        // (a stateful `FnMut` must not observe a consulted-but-ignored call), and the
        // `consume = true` token is taken atomically.
        let st = state();
        let mut calls_std = 0usize;
        let mut calls_list = 0usize;
        let mut c1 = |_: usize, _: StagedNodeView<'_, TestLang>| {
            calls_std += 1;
            Ok(true)
        };
        let mut c2 = |_: usize, _: StagedNodeView<'_, TestLang>| {
            calls_list += 1;
            Ok(true)
        };
        let parsed = run_both(
            "ab \\end rest",
            &st,
            Recovery::Strict,
            StopSpec {
                token: Some(TokenStopCondition {
                    kind: TokenStopKind::Command { name: "end" },
                    consume: true,
                }),
                node: Some(&mut c1),
            },
            StopSpec {
                token: Some(TokenStopCondition {
                    kind: TokenStopKind::Command { name: "end" },
                    consume: true,
                }),
                node: Some(&mut c2),
            },
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"ab \""]);
        assert_eq!(stop_shape(&parsed.stop), "token 3..8");
        assert_eq!(parsed.pos, 8);
        assert_eq!(calls_std, 0, "the token-stop flush consulted the node condition");
        assert_eq!(calls_list, 0, "the token-stop flush consulted the node condition");
    }

    // --- tokenizer-error recovery (std reader only: the list reader cannot fail) ---------

    #[test]
    fn forbidden_char_tolerant_adopts_the_recovery_token() {
        let mut r = rules::<TestLang>();
        r.forbidden_chars.chars = "#".into();
        let st = state_with(r);
        let source: Arc<Source> = Arc::new(Source::new("ab#cd"));
        let mut reader = StdTokenReader::new(&source);
        let parsed =
            try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none()).unwrap();
        // The placeholder Char joins the run: one maximal chars node, plus a diagnostic.
        assert_eq!(shapes(&parsed.result), ["chars 0..5 \"ab#cd\""]);
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert!(parsed
            .result
            .diagnostics
            .iter()
            .next()
            .unwrap()
            .message()
            .contains("forbidden"));
        assert_partition(&parsed.result, 0..5);
    }

    #[test]
    fn forbidden_char_strict_aborts() {
        let mut r = rules::<TestLang>();
        r.forbidden_chars.chars = "#".into();
        let st = state_with(r);
        let source: Arc<Source> = Arc::new(Source::new("ab#cd"));
        let mut reader = StdTokenReader::new(&source);
        let err =
            try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap_err();
        // The token error was lifted into the structured condition ([§dd-dr:errors]): compare
        // identifier and downcast fields (no PartialEq on the carriers).
        assert_eq!(err.identifier(), crate::token::ForbiddenChar::IDENTIFIER);
        assert_eq!(
            err.data().downcast_ref::<crate::token::ForbiddenChar>().map(|c| c.ch),
            Some('#')
        );
        assert_eq!(err.span().range(), 2..3);
    }

    /// A token source that violates the `TokenRecovery::resume` advancement contract:
    /// every `peek` reports a recoverable forbidden-char error whose `resume` is the
    /// position the reader already stands at, so adopting the recovery re-reads the
    /// same error.
    struct StuckRecoveryReader<'s> {
        inner: StdTokenReader<'s>,
    }

    impl<'s> StuckRecoveryReader<'s> {
        /// Delegation goes through a `dyn` view: the inner reader's `TokenReader` impl is
        /// generic over the language, which plain method syntax cannot infer here.
        fn inner(&self) -> &dyn TokenReader<'s, TestLang> {
            &self.inner
        }

        fn inner_mut(&mut self) -> &mut dyn TokenReader<'s, TestLang> {
            &mut self.inner
        }
    }

    impl<'s> TokenReader<'s, TestLang> for StuckRecoveryReader<'s> {
        fn peek(
            &mut self,
            _state: &Arc<ParsingState<TestLang>>,
        ) -> TokenResult<TestLang, StdToken<TestLang>> {
            let here = self.inner().position_here();
            let pos = here.offset();
            let span = Span::new(pos, pos + 1);
            Err(TokenError::new(
                TokenErrorKind::ForbiddenChar(crate::token::ForbiddenChar::new('#')),
                SourceSpan::new(self.inner.source(), span),
                Some(TokenRecovery {
                    token: StdToken::char('#', span, Span::empty(pos)),
                    resume: here, // the violation: the stream does not move
                }),
            ))
        }


        fn move_to(&mut self, tok: &StdToken<TestLang>, edge: TokenEdge) {
            self.inner_mut().move_to(tok, edge);
        }

        fn move_to_position(&mut self, at: &StdStreamPosition) {
            self.inner_mut().move_to_position(at);
        }

        fn token_kind<'t>(&self, tok: &'t StdToken<TestLang>) -> TokenKind<'t, TestLang>
        where
            's: 't,
        {
            self.inner().token_kind(tok)
        }

        fn source_span_between(
            &self,
            tok: &StdToken<TestLang>,
            a: TokenEdge,
            b: TokenEdge,
        ) -> SourceSpan {
            self.inner().source_span_between(tok, a, b)
        }

        fn position_here(&self) -> StdStreamPosition {
            self.inner().position_here()
        }

        fn position_at(&self, tok: &StdToken<TestLang>, edge: TokenEdge) -> StdStreamPosition {
            self.inner().position_at(tok, edge)
        }

        fn source_position_at(&self, at: &StdStreamPosition) -> SourcePos {
            self.inner().source_position_at(at)
        }

        fn source_span_within(
            &self,
            begin: &StdStreamPosition,
            end: &StdStreamPosition,
        ) -> Option<SourceSpan> {
            self.inner().source_span_within(begin, end)
        }
    }

    #[test]
    fn non_advancing_token_recovery_aborts_instead_of_spinning() {
        // Without the advancement guard this parse never terminates in tolerant mode
        // (the recovery arm consumes no token), pushing one diagnostic per iteration.
        let st = state();
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let mut reader = StuckRecoveryReader { inner: StdTokenReader::new(&source) };
        let err = try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none())
            .expect_err("a non-advancing resume position must abort the parse");
        assert_eq!(err.identifier(), crate::token::ForbiddenChar::IDENTIFIER);
        assert_eq!(
            err.data().downcast_ref::<crate::token::ForbiddenChar>().map(|c| c.ch),
            Some('#')
        );
        assert_eq!(err.span().range(), 0..1);
    }

    // --- TokenErrorKind::Custom ([§dd-dr:errors]: one extension mechanism serves both layers) --------

    /// A language-defined token condition, carried by `TokenErrorKind::Custom`.
    #[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
    #[diagnostic(
        id = "taboolang.token.taboo-char",
        message = "the character ‘{ch}’ is taboo here",
        no_constructor
    )]
    struct TabooChar {
        ch: char,
    }

    /// A `scan_specials` reporting a recoverable `Custom` token error on `!` — the
    /// extension point the variant exists for.
    #[derive(Debug, Clone, Copy)]
    struct TabooLang;
    impl Lang for TabooLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn scan_specials(
            _state: &ParsingState<Self>,
            content: &str,
            pos: usize,
        ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
            if content[pos..].starts_with('!') {
                Err(SpecialsScanError {
                    kind: TokenErrorKind::Custom(Box::new(TabooChar { ch: '!' })),
                    span: Span::new(pos, pos + 1),
                })
            } else {
                Ok(None)
            }
        }

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("!".into())
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

    /// A reader that reports a recoverable `Custom` token error on `!`, delegating
    /// everything else to an inner `StdTokenReader`.
    ///
    /// A scan hook can no longer describe a recovery (`SpecialsScanError` carries a
    /// condition and a range, nothing else), so the *recoverable* half of the `Custom`
    /// extension point is exercised where recoveries live: in a reader.
    struct TabooReader<'s> {
        inner: StdTokenReader<'s>,
    }

    impl<'s> TabooReader<'s> {
        /// Delegation goes through a `dyn` view: the inner reader's `TokenReader` impl is
        /// generic over the language, which plain method syntax cannot infer here.
        fn inner(&self) -> &dyn TokenReader<'s, TabooLang> {
            &self.inner
        }

        fn inner_mut(&mut self) -> &mut dyn TokenReader<'s, TabooLang> {
            &mut self.inner
        }
    }

    impl<'s> TokenReader<'s, TabooLang> for TabooReader<'s> {
        fn peek(
            &mut self,
            state: &Arc<ParsingState<TabooLang>>,
        ) -> TokenResult<TabooLang, StdToken<TabooLang>> {
            let pos = TokenReader::<TabooLang>::position_here(&self.inner).offset();
            if self.inner.content()[pos..].starts_with('!') {
                let span = Span::new(pos, pos + 1);
                return Err(TokenError::new(
                    TokenErrorKind::Custom(Box::new(TabooChar { ch: '!' })),
                    SourceSpan::new(self.inner.source(), span),
                    Some(TokenRecovery {
                        token: StdToken::char('!', span, Span::empty(pos)),
                        // In-crate test infrastructure may build a position directly.
                        resume: StdStreamPosition::at(span.end()),
                    }),
                ));
            }
            TokenReader::peek(&mut self.inner, state)
        }


        fn move_to(&mut self, tok: &StdToken<TabooLang>, edge: TokenEdge) {
            self.inner_mut().move_to(tok, edge);
        }

        fn move_to_position(&mut self, at: &StdStreamPosition) {
            self.inner_mut().move_to_position(at);
        }

        fn token_kind<'t>(&self, tok: &'t StdToken<TabooLang>) -> TokenKind<'t, TabooLang>
        where
            's: 't,
        {
            self.inner().token_kind(tok)
        }

        fn source_span_between(
            &self,
            tok: &StdToken<TabooLang>,
            a: TokenEdge,
            b: TokenEdge,
        ) -> SourceSpan {
            self.inner().source_span_between(tok, a, b)
        }

        fn position_here(&self) -> StdStreamPosition {
            self.inner().position_here()
        }

        fn position_at(&self, tok: &StdToken<TabooLang>, edge: TokenEdge) -> StdStreamPosition {
            self.inner().position_at(tok, edge)
        }

        fn source_position_at(&self, at: &StdStreamPosition) -> SourcePos {
            self.inner().source_position_at(at)
        }

        fn source_span_within(
            &self,
            begin: &StdStreamPosition,
            end: &StdStreamPosition,
        ) -> Option<SourceSpan> {
            self.inner().source_span_within(begin, end)
        }
    }

    #[test]
    fn custom_token_condition_flows_through_the_lift_unwrapped() {
        let st = state_with(rules::<TabooLang>());

        // A scan-hook failure is unrecoverable: both policies abort, carrying the
        // language's own condition.
        for recovery in [Recovery::Tolerant, Recovery::Strict] {
            let source: Arc<Source> = Arc::new(Source::new("a!b"));
            let mut reader = StdTokenReader::new(&source);
            let err = try_run(&source, &mut reader, &st, recovery, StopSpec::none())
                .expect_err("a specials scan error carries no recovery");
            assert_eq!(err.identifier(), TabooChar::IDENTIFIER);
            // The downcast reaches the payload directly: the lift unwraps `Custom` —
            // never double-boxed.
            assert!(err.data().downcast_ref::<TabooChar>().is_some());
        }

        // The same condition reported *recoverably* by a reader: tolerant mode records
        // the diagnostic (payload unwrapped) and the placeholder char joins the run.
        let source: Arc<Source> = Arc::new(Source::new("a!b"));
        let mut reader = TabooReader { inner: StdTokenReader::new(&source) };
        let parsed =
            try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none()).unwrap();
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"a!b\""]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), TabooChar::IDENTIFIER);
        assert_eq!(
            diagnostic.data().downcast_ref::<TabooChar>(),
            Some(&TabooChar { ch: '!' })
        );

        // Strict: the same condition rides the ParseError.
        let source: Arc<Source> = Arc::new(Source::new("a!b"));
        let mut reader = TabooReader { inner: StdTokenReader::new(&source) };
        let err = try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none())
            .unwrap_err();
        assert_eq!(err.identifier(), TabooChar::IDENTIFIER);
        assert!(err.data().downcast_ref::<TabooChar>().is_some());
    }

    #[test]
    fn escape_at_end_of_input_tolerant_recovers_and_repositions() {
        let st = state();
        let source: Arc<Source> = Arc::new(Source::new("ab \\"));
        let mut reader = StdTokenReader::new(&source);
        let parsed =
            try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none()).unwrap();
        // The placeholder is a Char covering the dangling escape byte: it joins the
        // pending chars run, so the recovery is partition-clean ([§dd-dr:errors]) — the escape
        // byte stays in the tree, accompanied by its diagnostic.
        assert_eq!(shapes(&parsed.result), ["chars 0..4 \"ab \\\\\""]);
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_eq!(parsed.pos, 4);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_partition(&parsed.result, 0..4);
    }

    #[test]
    fn escape_at_end_of_input_strict_aborts() {
        let st = state();
        let source: Arc<Source> = Arc::new(Source::new("ab \\"));
        let mut reader = StdTokenReader::new(&source);
        let err =
            try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap_err();
        assert_eq!(err.identifier(), crate::token::EndOfStreamAfterEscape::IDENTIFIER);
        assert_eq!(
            err.data()
                .downcast_ref::<crate::token::EndOfStreamAfterEscape>()
                .map(|c| c.escape_char),
            Some('\\')
        );
    }

    // --- unresolvable-command recovery ([§dd-dr:errors]) ---------------------------------------------

    #[test]
    fn unresolved_command_recovers_as_a_chars_fallback() {
        let st = state();
        let parsed =
            run_both("a \\foo  b", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        // The fallback covers the token's full span (post-space included) and is not
        // merged into neighboring runs — a diagnosed artifact, not content accumulation.
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"a \"", "chars 2..8 \"\\\\foo  \"", "chars 8..9 \"b\""]
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        // `TestLang` keeps the default `resolve_command`: its detail — command
        // resolution is not implemented — rides the condition into the message.
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.message(),
            "cannot resolve command ‘\\foo’ (command resolution is not implemented by \
             this language’s driver — implement ‘ParseDriver::resolve_command’ or use \
             a preset)"
        );
        assert!(
            diagnostic
                .data()
                .downcast_ref::<UnresolvableCommand>()
                .unwrap()
                .detail
                .is_some()
        );
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_partition(&parsed.result, 0..9);
    }

    #[test]
    fn diagnostics_inside_a_group_carry_the_group_frame() {
        // `{\foo` (tolerant): the unresolvable command fires *inside* the group, so its
        // snapshot carries the group frame; the unclosed-group condition fires after
        // the interior frame is popped, so its snapshot is empty (root level).
        let st = state();
        let parsed =
            run_both("{\\foo", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(parsed.result.diagnostics.len(), 2);

        let unresolvable = parsed
            .result
            .diagnostics
            .with_identifier(UnresolvableCommand::IDENTIFIER)
            .next()
            .unwrap();
        assert_eq!(unresolvable.frames().len(), 1);
        assert_eq!(unresolvable.frames()[0].title(), "group ‘{’");
        assert_eq!(unresolvable.frames()[0].span().range(), 0..1);
        // render() appends the traceback.
        assert!(unresolvable.render().contains("Open blocks:\n  @ (line 1, col 1): group ‘{’"));

        let unclosed = parsed
            .result
            .diagnostics
            .with_identifier(super::super::UnclosedGroup::IDENTIFIER)
            .next()
            .unwrap();
        assert!(unclosed.frames().is_empty());
    }

    #[test]
    fn unresolved_command_strict_aborts() {
        let st = state();
        let source: Arc<Source> = Arc::new(Source::new("a \\foo  b"));
        let mut reader = StdTokenReader::new(&source);
        let err = try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none())
            .unwrap_err();
        // Exact detail wording pinned in unresolved_command_recovers_as_a_chars_fallback.
        let message = err.to_string();
        assert!(message.starts_with("cannot resolve command ‘\\foo’"));
        assert!(message.contains("command resolution is not implemented"));
        assert_eq!(err.span().range(), 2..8);
    }

    #[test]
    fn library_miss_takes_the_unresolvable_command_recovery() {
        // Same recovery, reached through a resolve_command that consults libraries and
        // misses (no fallback registered).
        let st = state_with_macros(&[]);
        let parsed =
            run_both("a \\foo b", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"a \"", "chars 2..7 \"\\\\foo \"", "chars 7..8 \"b\""]
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        // `CmdLang` resolves through the shared `resolve_command_in_scopes`,
        // which reports the searched providers as the miss detail — one behavior across
        // the test langs and the latexlike preset (unified in the 7.5 review).
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.message(),
            "cannot resolve command ‘\\foo’ (searched providers: test-macros)"
        );
        assert_eq!(
            diagnostic
                .data()
                .downcast_ref::<UnresolvableCommand>()
                .unwrap()
                .detail
                .as_deref(),
            Some("searched providers: test-macros")
        );
        assert_partition(&parsed.result, 0..8);
    }

    #[test]
    fn provider_failure_takes_the_command_resolution_failed_recovery() {
        // An operational provider failure (vs. a clean miss) surfaces the distinct
        // CommandResolutionFailed condition through resolve_command_in_scopes, recovered as
        // chars like an unresolvable command.
        #[derive(Debug)]
        struct BrokenProvider;
        impl crate::serialize::SerializableObject<CmdLang> for BrokenProvider {}
        impl SpecsProvider<CmdLang> for BrokenProvider {
            fn name(&self) -> &str {
                "broken"
            }
            fn retrieve_spec(
                &self,
                _query: &CallableQuery<'_, CmdLang>,
                _state: &ParsingState<CmdLang>,
            ) -> Result<Option<Arc<dyn CallableSpec<CmdLang>>>, ProviderError> {
                Err(ProviderError::Failed("provider is down".into()))
            }
        }

        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(BrokenProvider));
        let st = Arc::new(ParsingState::new(StateData {
            rules: rules(),
            scopes,
            mode: (),
            ext: (),
        }));

        let parsed = run_both(
            "a \\foo b",
            &st,
            Recovery::Tolerant,
            StopSpec::none(),
            StopSpec::none(),
        );
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"a \"", "chars 2..7 \"\\\\foo \"", "chars 7..8 \"b\""]
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), CommandResolutionFailed::IDENTIFIER);
        assert_eq!(
            diagnostic.message(),
            "command resolution failed for ‘\\foo’ (provider ‘broken’: provider is down)"
        );
        assert_eq!(
            diagnostic
                .data()
                .downcast_ref::<CommandResolutionFailed>()
                .unwrap()
                .detail
                .as_deref(),
            Some("provider ‘broken’: provider is down")
        );
    }

    /// A driver whose resolver supplies its own failure detail — the channel a
    /// library-backed resolver would use for hints like "load this library".
    #[derive(Debug, Clone, Copy)]
    struct HintLang;
    impl Lang for HintLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = HintDriver;
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
    struct HintDriver {
        recovery: Recovery,
    }

    impl TestDriver for HintDriver {
        fn with_recovery(recovery: Recovery) -> Self {
            HintDriver { recovery }
        }
    }

    impl ParseDriver<HintLang> for HintDriver {
        fn make_token_reader<'s>(
            &'s self,
            source: &'s alloc::sync::Arc<crate::source::Source>,
        ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, HintLang> + 's> {
            alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
        }

        fn recovery(&self) -> Recovery {
            self.recovery
        }

        fn resolve_command(
            &self,
            _state: &ParsingState<HintLang>,
            _token: &StdToken<HintLang>,
            _tokens: &dyn TokenReader<'_, HintLang>,
        ) -> Result<CommandResolution<HintLang>, ParseError> {
            Ok(CommandResolution::Unresolved {
                detail: Some("load the {amsmath} library for this command".into()),
            })
        }
    }

    #[test]
    fn resolver_supplied_detail_rides_the_diagnostic() {
        let st = state_with(rules::<HintLang>());
        let parsed =
            run_both("a \\foo b", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(
            parsed.result.diagnostics.iter().next().unwrap().message(),
            "cannot resolve command ‘\\foo’ (load the {amsmath} library for this command)"
        );
    }

    /// A driver whose resolver fails hard — the hook-fallibility **abort**
    /// channel (`Err`), as opposed to `HintLang`'s recoverable `Unresolved` and
    /// the diagnosed `Failed` resolution.
    #[derive(Debug, Clone, Copy)]
    struct AbortLang;
    impl Lang for AbortLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = AbortDriver;
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
    struct AbortDriver {
        recovery: Recovery,
    }

    impl ParseDriver<AbortLang> for AbortDriver {
        fn make_token_reader<'s>(
            &'s self,
            source: &'s alloc::sync::Arc<crate::source::Source>,
        ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, AbortLang> + 's> {
            alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
        }

        fn recovery(&self) -> Recovery {
            self.recovery
        }

        fn resolve_command(
            &self,
            _state: &ParsingState<AbortLang>,
            _token: &StdToken<AbortLang>,
            _tokens: &dyn TokenReader<'_, AbortLang>,
        ) -> Result<CommandResolution<AbortLang>, ParseError> {
            let source: Arc<Source> = Arc::new(Source::new(""));
            Err(ParseError::new(
                crate::error::HookFailed::new("resolver backend is down", None),
                SourceSpan::new(&source, 0..0),
            ))
        }
    }

    impl TestDriver for AbortDriver {
        fn with_recovery(recovery: Recovery) -> Self {
            AbortDriver { recovery }
        }
    }

    #[test]
    fn a_failing_resolve_command_aborts_under_any_policy() {
        // The abort channel: an Err from resolve_command stops the parse even in
        // tolerant mode — nothing is diagnosed-and-recovered (that is what the
        // Unresolved/Failed resolution values are for) — and the descent site
        // attaches the live traceback (the hook has no session access).
        let st = state_with(rules::<AbortLang>());
        let content = "a {\\foo} b";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let error =
            try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none())
                .unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        assert_eq!(
            error.message(),
            "extension hook reported a failure: resolver backend is down"
        );
        // The command sits inside a group: the group frame is on the traceback
        // (resolution precedes the invocation frame, so it is the only frame).
        assert_eq!(error.frames().len(), 1);
        assert_eq!(error.frames()[0].title(), "group ‘{’");
    }

    // --- ParseDriver::refine_diagnostic ([§dd-dr:errors]) ----------------------------------------------

    /// The refinement demonstration's own condition: structured, so tools see it — not
    /// just better prose.
    #[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
    #[diagnostic(
        id = "refinelang.commands.not-available",
        message = "command ‘\\{name}’ is not available: this language defines no commands",
        no_constructor
    )]
    struct CommandsNotAvailable {
        name: String,
    }

    /// A Lang that refines the core's [`UnresolvableCommand`] into its own condition —
    /// the funnel applies the hook exactly once, on both the tolerant and strict paths.
    #[derive(Debug, Clone, Copy)]
    struct RefineLang;
    impl Lang for RefineLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = RefineDriver;
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
    struct RefineDriver {
        recovery: Recovery,
    }

    impl TestDriver for RefineDriver {
        fn with_recovery(recovery: Recovery) -> Self {
            RefineDriver { recovery }
        }
    }

    impl ParseDriver<RefineLang> for RefineDriver {
        fn make_token_reader<'s>(
            &'s self,
            source: &'s alloc::sync::Arc<crate::source::Source>,
        ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, RefineLang> + 's> {
            alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
        }

        fn recovery(&self) -> Recovery {
            self.recovery
        }

        fn refine_diagnostic(
            &self,
            data: alloc::boxed::Box<dyn crate::error::DiagnosticData>,
            _state: &ParsingState<RefineLang>,
        ) -> alloc::boxed::Box<dyn crate::error::DiagnosticData> {
            match data.downcast_ref::<UnresolvableCommand>() {
                Some(condition) => alloc::boxed::Box::new(CommandsNotAvailable {
                    name: condition.name.clone(),
                }),
                None => data,
            }
        }
    }

    #[test]
    fn refine_diagnostic_replaces_the_condition_in_the_funnel() {
        let st = state_with(rules::<RefineLang>());
        let parsed = run_both(
            "a \\foo b",
            &st,
            Recovery::Tolerant,
            StopSpec::none(),
            StopSpec::none(),
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), CommandsNotAvailable::IDENTIFIER);
        assert_eq!(
            diagnostic.data().downcast_ref::<CommandsNotAvailable>().unwrap().name,
            "foo"
        );
        assert!(diagnostic.message().contains("is not available"));

        // Strict mode: the refined condition rides the ParseError too (one funnel).
        let source: Arc<Source> = Arc::new(Source::new("a \\foo b"));
        let mut reader = StdTokenReader::new(&source);
        let err = try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none())
            .unwrap_err();
        assert_eq!(err.identifier(), CommandsNotAvailable::IDENTIFIER);
    }

    // --- invocation dispatch (6.4) ----------------------------------------------------------

    #[test]
    fn zero_arg_macro_end_to_end() {
        let st = state_with_macros(&["foo"]);
        let parsed =
            run_both("ab \\foo cd", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        // The callable's span is the whole trigger token: escape + name + syntactic
        // post-space. The pre-space is sibling content.
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..3 \"ab \"", "callable 3..8", "chars 8..10 \"cd\""]
        );
        let node = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(node.callable_type(), Some(CT_MACRO));
        assert_eq!(node.name(), Some("foo"));
        let data = node.callable().unwrap();
        assert!(data.arguments.is_empty() && data.slots.is_empty());
        // The node records the package's spec (the flyweight Arc).
        let expected = st
            .scopes()
            .retrieve_spec(
                &CallableQuery::new(
                    CT_MACRO,
                    "foo",
                    CallableSyntax::Command { escape_char: '\\' },
                ),
                &st,
            )
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&data.spec, &expected));
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert!(parsed.result.diagnostics.is_empty());
        assert_partition(&parsed.result, 0..10);
    }

    #[test]
    fn macro_post_space_is_the_trigger_tokens_own_and_nothing_more() {
        // `\foo bar`: the name-terminating space is the token's syntactic post-space —
        // recorded on the node, inside its span (the user-decided 6.4 rule: nothing
        // beyond the token's own post-space is ever claimed).
        let st = state_with_macros(&["foo", "&"]);
        let parsed =
            run_both("\\foo bar", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["callable 0..5", "chars 5..8 \"bar\""]);
        assert_partition(&parsed.result, 0..8);

        // A single non-name-char command (`\&`) takes no post-space at the token level,
        // and the invocation claims nothing beyond the token: the space is sibling
        // content (TeX/pylatexenc parity).
        let parsed =
            run_both("\\& b", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["callable 0..2", "chars 2..4 \" b\""]);
        assert_partition(&parsed.result, 0..4);
    }

    #[test]
    fn macro_post_space_stops_at_a_paragraph_break() {
        let st = state_with_macros(&["foo"]);
        let parsed = run_both(
            "\\foo \n\nb",
            &st,
            Recovery::Strict,
            StopSpec::none(),
            StopSpec::none(),
        );
        // The token's post-space is cut off before the paragraph break (a token-rules
        // guarantee); the break is its own node.
        assert_eq!(
            shapes(&parsed.result),
            ["callable 0..5", "chars 5..7 \"\\n\\n\"", "chars 7..8 \"b\""]
        );
        assert_partition(&parsed.result, 0..8);
    }

    #[test]
    fn specials_dispatch_through_the_resolution_riding_on_the_token() {
        // Recognition = resolution: a recognized specials trigger always dispatches
        // (6.4) — the pre-6.4 chars-fallback recovery no longer applies to specials.
        let st = state_with(rules::<TildeLang>());
        let parsed =
            run_both("a~b", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..1 \"a\"", "callable 1..2", "chars 2..3 \"b\""]
        );
        let node = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(node.callable_type(), Some(CT_SPECIALS));
        assert_eq!(node.name(), Some("~"));
        assert!(parsed.result.diagnostics.is_empty());
        assert_partition(&parsed.result, 0..3);
    }

    #[test]
    fn invocation_inside_a_group_dispatches_at_that_level() {
        let st = state_with_macros(&["foo"]);
        let parsed =
            run_both("{\\foo x}", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["group 0..8"]);
        let group = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(group.child_count(), 2);
        assert_eq!(group.child(0).unwrap().name(), Some("foo"));
        assert_eq!(group.child(1).unwrap().chars(), Some("x"));
        assert_partition(&parsed.result, 0..8);
    }

    #[test]
    fn invocation_delta_defines_later_siblings() {
        // The `\newcommand` shape: `\def`'s parser delegates to the standard parser
        // for its own node, then returns a push-library delta as its after-effect;
        // the later sibling `\late` resolves against the pushed library.
        #[derive(Debug)]
        struct DefSpec;
        impl crate::serialize::SerializableObject<CmdLang> for DefSpec {}
        impl CallableSpec<CmdLang> for DefSpec {
            fn make_invocation_parser<'a>(
                &'a self,
                invocation: Invocation<'a, CmdLang>,
            ) -> Result<Box<dyn ConstructParser<CmdLang, Output = BuildId> + 'a>, ParseError>
            {
                Ok(Box::new(DefParser { inner: StdInvocationParser::new(invocation) }))
            }
        }

        struct DefParser<'a> {
            inner: StdInvocationParser<'a, CmdLang>,
        }

        impl ConstructParser<CmdLang> for DefParser<'_> {
            type Output = BuildId;

            fn parse(
                &mut self,
                cx: &mut ParseContext<'_, '_, CmdLang>,
            ) -> ConstructParserResult<
                CmdLang,
                (BuildId, Option<Box<ParsingStateDelta<CmdLang>>>),
            > {
                let (id, _) = self.inner.parse(cx)?;
                let delta =
                    ParsingStateDelta::new().push_provider(macro_library(&["late"]));
                Ok((id, Some(Box::new(delta))))
            }
        }

        let mut lib: Package<CmdLang> = Package::new("cmds");
        lib.insert(CT_MACRO, "def", Arc::new(DefSpec));
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(lib));
        let st: Arc<ParsingState<CmdLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), scopes, mode: (), ext: () }));

        let parsed = run_both(
            "\\def \\late x",
            &st,
            Recovery::Strict,
            StopSpec::none(),
            StopSpec::none(),
        );
        assert_eq!(
            shapes(&parsed.result),
            ["callable 0..5", "callable 5..11", "chars 11..12 \"x\""]
        );
        let def = parsed.result.tree.root().child(0).unwrap();
        let late = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(def.name(), Some("def"));
        assert_eq!(late.name(), Some("late"));
        // The after-effect was applied to the loop's state: the later sibling records
        // the derived state (session-mediated), not the initial one.
        assert!(Arc::ptr_eq(def.parsing_state(), &st));
        assert!(!Arc::ptr_eq(late.parsing_state(), &st));
        assert!(parsed.result.diagnostics.is_empty());
        assert_partition(&parsed.result, 0..12);

        // Control: without the preceding `\def`, `\late` is unresolvable.
        let st = state_with_macros(&[]);
        let parsed = run_both(
            "\\late x",
            &st,
            Recovery::Tolerant,
            StopSpec::none(),
            StopSpec::none(),
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
    }

    #[test]
    fn takeover_parser_reads_raw_tokens_stages_custom_shape_and_returns_a_delta() {
        // The full-takeover escape hatch ([§dd-dr:parsers-engine], the C6 obligation): an overridden
        // factory returns a custom parser that consumes tokens up to a `!` marker —
        // markup included, no group descent — stages its own node shape (an untyped
        // group holding the raw chars), and returns a tokenization-affecting delta
        // (comments disabled) as its after-effect.
        #[derive(Debug)]
        struct TakeSpec;
        impl crate::serialize::SerializableObject<CmdLang> for TakeSpec {}
        impl CallableSpec<CmdLang> for TakeSpec {
            fn make_invocation_parser<'a>(
                &'a self,
                invocation: Invocation<'a, CmdLang>,
            ) -> Result<Box<dyn ConstructParser<CmdLang, Output = BuildId> + 'a>, ParseError>
            {
                Ok(Box::new(TakeParser { invocation }))
            }
        }

        struct TakeParser<'a> {
            invocation: Invocation<'a, CmdLang>,
        }

        impl ConstructParser<CmdLang> for TakeParser<'_> {
            type Output = BuildId;

            fn parse(
                &mut self,
                cx: &mut ParseContext<'_, '_, CmdLang>,
            ) -> ConstructParserResult<
                CmdLang,
                (BuildId, Option<Box<ParsingStateDelta<CmdLang>>>),
            > {
                // The trigger is already consumed whole; its span becomes the "open
                // delimiter". Raw content runs from there to the `!` marker.
                let open_span = cx.tokens.source_span_of(self.invocation.token);
                let content_start = cx.tokens.position_here();
                let (content_end, close_span, end) = loop {
                    let token = cx.tokens.peek(&cx.state).expect("clean test content");
                    let at = cx.tokens.position_at(&token, TokenEdge::Start);
                    cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                    match cx.tokens.token_kind(&token) {
                        TokenKind::Char('!') => {
                            break (
                                at,
                                cx.tokens.source_span_of(&token),
                                cx.tokens.position_here(),
                            )
                        }
                        TokenKind::EndOfStream => {
                            panic!("test content has a marker")
                        }
                        _ => {}
                    }
                };
                let content = cx.source_span_within(&content_start, &content_end)?;
                let child = cx.stage_node(
                    NodeKind::chars(content.span()),
                    content,
                    Arc::clone(&cx.state),
                    vec![],
                ).unwrap();
                let span = cx.source_span_within(
                    &cx.tokens.position_at(self.invocation.token, TokenEdge::Start),
                    &end,
                )?;
                // Node data sub-spans go through the shared recording rule, exactly
                // as the production sites do.
                let delimiter =
                    |at: &SourceSpan| super::super::node_text_content(at, &span);
                let data: GroupData<CmdLang> =
                    GroupData::untyped(delimiter(&open_span), delimiter(&close_span));
                let id = cx.stage_node(
                    NodeKind::group(data),
                    span,
                    Arc::clone(&cx.state),
                    vec![child],
                ).unwrap();
                let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
                    comments: CommentOverrides::disable(),
                    ..TokenRulesOverrides::default()
                });
                Ok((id, Some(Box::new(delta))))
            }
        }

        let mut lib: Package<CmdLang> = Package::new("cmds");
        lib.insert(CT_MACRO, "take", Arc::new(TakeSpec));
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(lib));
        let st: Arc<ParsingState<CmdLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), scopes, mode: (), ext: () }));

        // `a{b` would normally open a group (and diagnose it unclosed); the takeover
        // parser consumes it raw. ` %c` would normally be a comment; the returned
        // delta disables comments, so it parses as plain chars. StdTokenReader only:
        // a pre-scanned token list can neither re-serve raw bytes nor re-tokenize
        // under mid-parse rule changes (TokenListReader's documented fidelity limit).
        let content = "\\take a{b! %c";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let parsed =
            try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap();
        assert_eq!(
            shapes(&parsed.result),
            ["group 0..10", "chars 10..13 \" %c\""]
        );
        let take = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(take.group_type(), None); // untyped: no language group class
        assert_eq!(take.group_delimiters(), Some(("\\take ", "!")));
        assert_eq!(take.child(0).unwrap().chars(), Some("a{b"));
        assert!(parsed.result.diagnostics.is_empty());
        assert_partition(&parsed.result, 0..13);
    }

    #[test]
    fn make_node_ext_mints_through_the_dispatch_loop() {
        // FLM pattern rehearsal ([§dd-dr:ext-minting]): parse staging goes through the
        // one automatic minting site (`cx.stage_node`), so every node the dispatch
        // loop stages — third-party construct parsers included — carries a properly
        // minted ext. The mint sees the kind: a preset downcasts a `Callable`'s
        // `data.spec` to its concrete spec type (the `Any` supertrait contract) and
        // derives ext data from spec + invocation facts. A preset dispatching on an
        // *open* set of spec types funnels them through one concrete wrapper first
        // (DESIGN_RATIONALE.md [§dd-dr:specs]).
        struct ExtBundle;
        impl NodeExtTypes for ExtBundle {
            type NodeExt = Option<Box<str>>; // Some for callables, None elsewhere
            type ArgumentExt = ();
            type SlotExt = ();
        }

        #[derive(Debug, Clone, Copy)]
        struct ExtLang;

        #[derive(Debug, Clone, Copy)]
        struct ExtDriver {
            recovery: Recovery,
        }

        impl TestDriver for ExtDriver {
            fn with_recovery(recovery: Recovery) -> Self {
                ExtDriver { recovery }
            }
        }

        impl ParseDriver<ExtLang> for ExtDriver {
            fn make_token_reader<'s>(
                &'s self,
                source: &'s alloc::sync::Arc<crate::source::Source>,
            ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, ExtLang> + 's> {
                alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
            }

            fn recovery(&self) -> Recovery {
                self.recovery
            }

            fn resolve_command(
                &self,
                state: &ParsingState<ExtLang>,
                token: &StdToken<ExtLang>,
                tokens: &dyn TokenReader<'_, ExtLang>,
            ) -> Result<CommandResolution<ExtLang>, ParseError> {
                resolve_macro_in_scopes(state, token, tokens)
            }
        }

        impl Lang for ExtLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ExtBundle;
            type InvocationSyntax = ();
            type Driver = ExtDriver;

            fn make_node_ext(
                kind: &NodeKind<Self>,
                _span: &SourceSpan<Self::SourceOrigin>,
                _state: &Arc<ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) -> Result<Option<Box<str>>, crate::node::NodeBuildError> {
                let NodeKind::Callable(data) = kind else { return Ok(None) };
                // The downcast: trait-upcast the stored spec to `&dyn Any`, then
                // recover the concrete type — field access included.
                let spec = (&*data.spec as &dyn core::any::Any)
                    .downcast_ref::<StdCallableSpec<ExtLang>>()
                    .expect("the test library registers StdCallableSpec");
                Ok(Some(format!("{}#{}", data.name, spec.arguments.len()).into_boxed_str()))
            }
        }

        let mut scopes = ScopeStack::new();
        scopes.push(macro_library::<ExtLang>(&["foo"]));
        let st: Arc<ParsingState<ExtLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), scopes, mode: (), ext: () }));

        let parsed =
            run_both("\\foo x", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        let node = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(node.ext().as_deref(), Some("foo#0"));
        // Non-callable siblings got the mint's other arm.
        assert_eq!(*parsed.result.tree.root().child(1).unwrap().ext(), None);
    }

    #[test]
    fn invocation_child_state_policy_fixed_and_compute() {
        let st = state_with_macros(&["foo"]);
        let other = Arc::new(st.derived(&ParsingStateDelta::new()).unwrap());

        // Fixed: the invocation parser runs under (and its node records) the fixed
        // state; the policy scopes the descent only — the next sibling is back on the
        // loop's own state.
        let parsed = run_both_with(
            "\\foo x",
            &st,
            Recovery::Strict,
            StopSpec::none(),
            StopSpec::none(),
            ChildStateSpec {
                group: GroupChildState::Inherit,
                invocation: InvocationChildState::Fixed(Arc::clone(&other)),
            },
        );
        let node = parsed.result.tree.root().child(0).unwrap();
        assert!(Arc::ptr_eq(node.parsing_state(), &other));
        let sibling = parsed.result.tree.root().child(1).unwrap();
        assert!(Arc::ptr_eq(sibling.parsing_state(), &st));

        // Compute: a pure selection receiving the resolved invocation (resolution
        // precedes policy); returning an input preserves pointer identity.
        let compute = |state: &Arc<ParsingState<CmdLang>>,
                       invocation: &Invocation<'_, CmdLang>|
         -> Result<Arc<ParsingState<CmdLang>>, ParseError> {
            assert_eq!(invocation.name, "foo");
            assert_eq!(invocation.callable_type, CT_MACRO);
            Ok(Arc::clone(state))
        };
        let parsed = run_both_with(
            "\\foo x",
            &st,
            Recovery::Strict,
            StopSpec::none(),
            StopSpec::none(),
            ChildStateSpec {
                group: GroupChildState::Inherit,
                invocation: InvocationChildState::Compute(&compute),
            },
        );
        let node = parsed.result.tree.root().child(0).unwrap();
        assert!(Arc::ptr_eq(node.parsing_state(), &st));
    }

    // --- groups (6.3) ----------------------------------------------------------------------

    #[test]
    fn group_round_trips_with_exact_spans_and_delimiters() {
        let st = state();
        let parsed =
            run_both("a {b} c", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"a \"", "group 2..5", "chars 5..7 \" c\""]
        );
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert!(parsed.result.diagnostics.is_empty());
        let group = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(group.group_type(), Some(GT_BRACE));
        assert_eq!(group.group_delimiters(), Some(("{", "}")));
        assert_eq!(group.child_count(), 1);
        assert_eq!(group.child(0).unwrap().chars(), Some("b"));
        // The space after `}` is enclosing content, not a group post-space.
        assert_partition(&parsed.result, 0..7);
    }

    #[test]
    fn nested_groups_with_exact_spans() {
        let st = state();
        let parsed =
            run_both("{a{b}c}", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["group 0..7"]);
        let outer = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(outer.child_count(), 3);
        assert_eq!(outer.child(0).unwrap().chars(), Some("a"));
        let inner = outer.child(1).unwrap();
        assert!(inner.is_group());
        assert_eq!(inner.span().range(), 2..5);
        assert_eq!(inner.child(0).unwrap().chars(), Some("b"));
        assert_eq!(outer.child(2).unwrap().chars(), Some("c"));
    }

    #[test]
    fn empty_group_has_no_children() {
        let st = state();
        let parsed =
            run_both("x{}y", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..1 \"x\"", "group 1..3", "chars 3..4 \"y\""]
        );
        let group = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(group.child_count(), 0);
        assert_eq!(group.group_delimiters(), Some(("{", "}")));
    }

    #[test]
    fn paragraph_break_inside_a_group_stays_inside() {
        let st = state();
        let parsed =
            run_both("{a\n\nb}", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["group 0..6"]);
        let group = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(group.child_count(), 3); // "a", the break node, "b"
        assert_eq!(group.child(1).unwrap().chars(), Some("\n\n"));
    }

    #[test]
    fn ambiguous_dollar_group_closes_via_the_derived_expected_close() {
        // `$` opens and closes GT_MATH groups. As a *closer* it is only recognizable
        // through `expecting_group_close` — which the interior state carries precisely
        // because the group parser derives it (session-memoized); without it the
        // interior `$` would read as another opener. Std reader only: a pre-scanned
        // token list is tokenized under the base state and cannot see the interior
        // state's reclassification.
        let mut r = rules::<TestLang>();
        r.groups.rules.push(math_rule());
        let st = state_with(r);
        let source: Arc<Source> = Arc::new(Source::new("a $x$ b"));
        let mut reader = StdTokenReader::new(&source);
        let parsed =
            try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap();
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"a \"", "group 2..5", "chars 5..7 \" b\""]
        );
        let group = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(group.group_type(), Some(GT_MATH));
        assert_eq!(group.group_delimiters(), Some(("$", "$")));
        assert_eq!(group.child(0).unwrap().chars(), Some("x"));
    }

    #[test]
    fn node_condition_counts_a_group_as_one_node() {
        let st = state();
        let mut c1 = |count: usize, view: StagedNodeView<'_, TestLang>| {
            Ok(count >= 1 && matches!(view.kind(), NodeKind::Group(_)))
        };
        let mut c2 = |count: usize, view: StagedNodeView<'_, TestLang>| {
            Ok(count >= 1 && matches!(view.kind(), NodeKind::Group(_)))
        };
        let parsed = run_both(
            "{a}b",
            &st,
            Recovery::Strict,
            StopSpec { token: None, node: Some(&mut c1) },
            StopSpec { token: None, node: Some(&mut c2) },
        );
        // The condition fired on the group (one staged sibling, consumed whole); the
        // parse stops right after it.
        assert_eq!(shapes(&parsed.result), ["group 0..3"]);
        assert_eq!(parsed.stop, StopCause::NodeCondition);
        assert_eq!(parsed.pos, 3);
    }

    #[test]
    fn stop_conditions_are_not_consulted_inside_a_group() {
        // Conditions are tested only at the parser's own nesting level ([§dd-dr:parsers-engine]): the
        // `\end` inside the group is the *interior* parser's business (here the
        // 6.4-pending unresolvable-command recovery), never an outer stop.
        let st = state();
        let parsed = run_both(
            "a{\\end}b",
            &st,
            Recovery::Tolerant,
            StopSpec::at_token(TokenStopKind::Command { name: "end" }, false),
            StopSpec::at_token(TokenStopKind::Command { name: "end" }, false),
        );
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let group = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(group.child(0).unwrap().chars(), Some("\\end"));
    }

    // --- group recovery (6.3) --------------------------------------------------------------

    #[test]
    fn unclosed_group_at_end_of_input_recovers_with_an_empty_close() {
        let st = state();
        let parsed =
            run_both("a {bc", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"a \"", "group 2..5"]);
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_eq!(parsed.pos, 5);
        let group = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(group.group_delimiters(), Some(("{", "")));
        assert_eq!(group.child(0).unwrap().chars(), Some("bc"));
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(
            parsed.result.diagnostics.iter().next().unwrap().message(),
            "unclosed group: expected ‘}’ before end of input"
        );
    }

    #[test]
    fn unclosed_group_strict_aborts() {
        let st = state();
        let source: Arc<Source> = Arc::new(Source::new("a {bc"));
        let mut reader = StdTokenReader::new(&source);
        let err = try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none())
            .unwrap_err();
        assert_eq!(err.to_string(), "unclosed group: expected ‘}’ before end of input");
        // The diagnostic points at the open delimiter that was never closed.
        assert_eq!(err.span().range(), 2..3);
    }

    #[test]
    fn mismatched_close_inside_a_group_unwinds_without_consuming() {
        // `[`/`]` shares class GT_BRACE. Inside the `{` group, the `]` matches neither
        // the expected close spelling nor rules it out by class alone: the interior
        // reports it as data, the group parser diagnoses and closes *without consuming*
        // (unwinding — every level consumes or unwinds, [§dd-dr:parsers-engine]), and the stray close then
        // surfaces at this level, still unconsumed, for the root driver to adjudicate.
        let mut r = rules::<TestLang>();
        r.groups.rules.push(Arc::new(GroupRule {
            group_type: GT_BRACE,
            open: "[".into(),
            close: "]".into(),
        }));
        let st = state_with(r);
        let parsed =
            run_both("{a]}", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["group 0..2"]);
        let group = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(group.group_delimiters(), Some(("{", "")));
        assert_eq!(group.child(0).unwrap().chars(), Some("a"));
        assert_eq!(stop_shape(&parsed.stop), "close 2..3");
        assert_eq!(parsed.pos, 2);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(
            parsed.result.diagnostics.iter().next().unwrap().message(),
            "mismatched group close: expected ‘}’"
        );
    }

    #[test]
    fn mismatched_close_inside_a_group_strict_aborts() {
        let mut r = rules::<TestLang>();
        r.groups.rules.push(Arc::new(GroupRule {
            group_type: GT_BRACE,
            open: "[".into(),
            close: "]".into(),
        }));
        let st = state_with(r);
        let source: Arc<Source> = Arc::new(Source::new("{a]}"));
        let mut reader = StdTokenReader::new(&source);
        let err =
            try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap_err();
        assert_eq!(err.to_string(), "mismatched group close: expected ‘}’");
        assert_eq!(err.span().range(), 2..3);
    }

    #[test]
    fn root_driver_skips_a_stray_close_and_continues() {
        // The root always consumes ([§dd-dr:parsers-engine]): `UnexpectedGroupClose` is data, and the
        // *root driver* — here the test; the core `Language::parse` entry drives this
        // same loop since Phase 7.4 — diagnoses, skips the token, and resumes. The
        // skipped byte is dropped from the tree: an accepted tolerant byte-accounting
        // break, so this is the one tree the invariant checker is deliberately not
        // applied to.
        //
        // The condition here is deliberately the driver's own, third-party style
        // ([§dd-dr:errors]): a custom root driver defines its diagnoses like any downstream
        // language would (or reuses the core `StrayGroupClose`, which is what
        // `Language::parse` reports).
        #[derive(Debug, Clone, DiagnosticInfo)]
        #[diagnostic(
            id = "test.root-driver.stray-group-close",
            message = "unexpected closing ‘{delim}’",
            no_constructor
        )]
        struct StrayGroupClose {
            delim: String,
        }

        let content = "a}b";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let st = state();
        let mut reader = StdTokenReader::new(&source);
        let mut session: ParserSession<TestLang> = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Tolerant, ());
        let mut nodes = Vec::new();
        let stop = loop {
            let mut cx = ParseContext::new(
                &mut reader,
                Arc::clone(&st),
                &mut session,
                &driver);
            let mut parser = NodesParser::new(StopSpec::none());
            let (outcome, _) = parser.parse(&mut cx).unwrap();
            nodes.extend(outcome.nodes);
            match outcome.stop {
                StopCause::UnexpectedGroupClose { span, after } => {
                    session
                        .recover(
                            Recovery::Tolerant,
                            Box::new(StrayGroupClose { delim: "}".into() }),
                            span,
                        )
                        .unwrap();
                    // The skip target rides on the cause: no re-peek.
                    <dyn TokenReader<'_, TestLang>>::move_to_position(&mut reader, &after);
                }
                other => break other,
            }
        };
        assert_eq!(stop, StopCause::EndOfInput);
        let root = session.builder.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            Arc::clone(&st),
            nodes, (), (),
        ).unwrap();
        let result = session.finish(root).unwrap();
        let texts: Vec<_> = result
            .tree
            .root()
            .children()
            .iter().map(|c| c.chars().unwrap().to_string())
            .collect();
        assert_eq!(texts, ["a", "b"]);
        assert_eq!(result.diagnostics.len(), 1);
    }

    // --- descent-state policy + the session seam (6.3) --------------------------------------

    #[test]
    fn child_state_fixed_reverts_group_interiors_to_the_outer_state() {
        // The chars-except-groups motivating case ([§dd-dr:parsers-engine]): a restricted outer state
        // (comments disabled) whose group interiors revert to the full state. Std
        // reader only: a pre-scanned list cannot see the interior reclassification.
        let full = state();
        let restricted = Arc::new(full.derived(&ParsingStateDelta::new().rules(
            TokenRulesOverrides { comments: CommentOverrides::disable(), ..Default::default() },
        )).unwrap());
        let content = "%x{%y\n}z";

        // Under the restricted state alone, `%` is ordinary content everywhere.
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let plain =
            try_run(&source, &mut reader, &restricted, Recovery::Strict, StopSpec::none())
                .unwrap();
        let group = plain.result.tree.root().child(1).unwrap();
        assert!(group.children().iter().all(|c| c.is_chars()));

        // With `group: Fixed(full)` the interior reverts: `%y` is a comment again.
        let policy = ChildStateSpec {
            group: GroupChildState::Fixed(Arc::clone(&full)),
            invocation: InvocationChildState::Inherit,
        };
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let parsed = try_run_with(&source,
            &mut reader,
            &restricted,
            Recovery::Strict,
            StopSpec::none(),
            policy,
        )
        .unwrap();
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"%x\"", "group 2..7", "chars 7..8 \"z\""]
        );
        let group = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(group.child_count(), 1);
        let comment = group.child(0).unwrap();
        assert_eq!(
            comment.comment().map(|data| data.content.resolve(comment.source())),
            Some("y")
        );
        // The group node itself records the policy's base state (its input state).
        assert!(Arc::ptr_eq(group.parsing_state(), &full));
    }

    #[test]
    fn child_state_compute_selects_by_the_open_token() {
        // The callback receives the loop's state and the open token (delim + resolved
        // rule): comments stay enabled inside `{…}` (pass-through) but are disabled
        // inside `[…]` (precomputed state selected by group class). Std reader only.
        const GT_OPT: u32 = 2;
        let mut r = rules::<TestLang>();
        r.groups.rules.push(Arc::new(GroupRule {
            group_type: GT_OPT,
            open: "[".into(),
            close: "]".into(),
        }));
        let full = state_with(r);
        let no_comments = Arc::new(full.derived(&ParsingStateDelta::new().rules(
            TokenRulesOverrides { comments: CommentOverrides::disable(), ..Default::default() },
        )).unwrap());
        let compute = |state: &Arc<ParsingState<TestLang>>,
                       token: &StdToken<TestLang>,
                       tokens: &dyn TokenReader<'_, TestLang>| {
            Ok(match tokens.token_kind(token) {
                TokenKind::GroupOpen { rule, .. } if rule.group_type == GT_OPT => {
                    Arc::clone(&no_comments)
                }
                _ => Arc::clone(state), // pass-through preserves pointer identity
            })
        };
        let policy = ChildStateSpec {
            group: GroupChildState::Compute(&compute),
            invocation: InvocationChildState::Inherit,
        };

        let content = "{%a\n}[%b\n]";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let parsed = try_run_with(&source,
            &mut reader,
            &full,
            Recovery::Strict,
            StopSpec::none(),
            policy,
        )
        .unwrap();
        let braces = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(braces.child_count(), 1);
        let comment = braces.child(0).unwrap();
        assert_eq!(
            comment.comment().map(|data| data.content.resolve(comment.source())),
            Some("a")
        );
        assert!(Arc::ptr_eq(braces.parsing_state(), &full)); // pass-through identity
        let brackets = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(brackets.group_type(), Some(GT_OPT));
        assert_eq!(brackets.child_count(), 1);
        assert_eq!(brackets.child(0).unwrap().chars(), Some("%b\n"));
        assert!(Arc::ptr_eq(brackets.parsing_state(), &no_comments));
    }

    #[test]
    fn a_failing_child_state_compute_aborts_under_any_policy() {
        // The hook-fallibility contract on the Compute arms: an Err is an abort
        // even under tolerant recovery — the descent-state seam has no recovery
        // channel — and the callback's condition rides the abort error. The
        // demonstrated body is the documented DeriveError lift: HookFailed with
        // the derivation failure as the cause.
        let full = state_with(rules::<TestLang>());
        let compute = |state: &Arc<ParsingState<TestLang>>,
                       _token: &StdToken<TestLang>,
                       _tokens: &dyn TokenReader<'_, TestLang>| {
            // A derivation the policy needs, failing operationally (a scope op
            // against a provider name that does not exist).
            let delta = ParsingStateDelta::new()
                .scope_op(crate::scopes::ScopeOp::Unload { name: "no-such-provider".into() });
            match state.derived(&delta) {
                Ok(state) => Ok(Arc::new(state)),
                Err(error) => {
                    let scratch: Arc<Source> = Arc::new(Source::new(""));
                    Err(ParseError::new(
                        crate::error::HookFailed::new(error.to_string(), None)
                            .with_cause(error),
                        SourceSpan::new(&scratch, 0..0),
                    ))
                }
            }
        };
        let policy = ChildStateSpec {
            group: GroupChildState::Compute(&compute),
            invocation: InvocationChildState::Inherit,
        };

        let content = "{a}";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let error = try_run_with(&source,
            &mut reader,
            &full,
            Recovery::Tolerant,
            StopSpec::none(),
            policy,
        )
        .unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        // The DeriveError cause chain is reachable off the carried condition.
        let condition =
            error.data().downcast_ref::<crate::error::HookFailed>().unwrap();
        assert!(condition.cause.as_ref().unwrap().to_string().contains("scope op failed"));
        // The policy computes the descent base *before* the group frame is pushed,
        // and the harness drives the root loop with no frame of its own — the
        // attached snapshot is exactly empty.
        assert_eq!(error.frames().len(), 0);
    }

    #[test]
    fn sibling_group_interiors_share_one_memoized_state() {
        let st = state();
        let parsed =
            run_both("{a}{b}", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        let root = parsed.result.tree.root();
        let g1 = root.child(0).unwrap();
        let g2 = root.child(1).unwrap();
        // The group nodes record the outer state (structural revert restores the Arc)…
        assert!(Arc::ptr_eq(g1.parsing_state(), &st));
        assert!(Arc::ptr_eq(g2.parsing_state(), &st));
        // …while their interior children share one derived Arc (the session memo) that
        // carries the expected close.
        let a = g1.child(0).unwrap();
        let b = g2.child(0).unwrap();
        assert!(Arc::ptr_eq(a.parsing_state(), b.parsing_state()));
        assert!(!Arc::ptr_eq(a.parsing_state(), &st));
        assert_eq!(
            a.parsing_state()
                .rules()
                .expecting_group_close()
                .map(|rule| rule.close.as_str()),
            Some("}")
        );
    }

    #[test]
    fn observe_transition_fires_per_descent_finalize_once_per_derivation() {
        // The two-level transition doctrine ([§dd-dr:parsers-engine]): `{a}{b}` makes two group descents —
        // both reach observe_transition (memo hits included) — but only one actual
        // derivation runs finalize_transition.
        use core::sync::atomic::{AtomicUsize, Ordering};
        static FINALIZE_RUNS: AtomicUsize = AtomicUsize::new(0);

        #[derive(Debug, Default)]
        struct Counts {
            observed: usize,
        }

        #[derive(Debug, Clone, Copy)]
        struct CountLang;
        impl Lang for CountLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = Counts;
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = CountDriver;

            fn finalize_transition(
                _new: &mut StateData<Self>,
                _prev: &ParsingState<Self>,
                _events: &[()],
            ) -> Result<(), crate::state::FinalizeError> {
                FINALIZE_RUNS.fetch_add(1, Ordering::Relaxed);
                Ok(())
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

        /// Counts observations (the hook moved to the driver in 7.2); strict by
        /// default, which is what the manual drive below wants.
        #[derive(Debug, Clone, Copy)]
        struct CountDriver;

        impl ParseDriver<CountLang> for CountDriver {
            fn make_token_reader<'s>(
                &'s self,
                source: &'s alloc::sync::Arc<crate::source::Source>,
            ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, CountLang> + 's> {
                alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
            }

            fn observe_transition(
                &self,
                ext: &mut Counts,
                _diagnostics: &mut crate::error::Diagnostics,
                _prev: &ParsingState<CountLang>,
                _new: &ParsingState<CountLang>,
                _delta: &ParsingStateDelta<CountLang>,
            ) -> Result<(), ParseError> {
                ext.observed += 1;
            Ok(())
            }
        }

        // Driven manually so the session stays inspectable after the parse.
        let content = "{a}{b}";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let st = state_with(rules::<CountLang>());
        let mut reader = StdTokenReader::new(&source);
        let mut session: ParserSession<CountLang> = ParserSession::new();
        let driver = CountDriver;
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&st),
            &mut session,
            &driver);
        let mut parser = NodesParser::new(StopSpec::none());
        let (outcome, _) = parser.parse(&mut cx).unwrap();
        assert!(matches!(outcome.stop, StopCause::EndOfInput));
        assert_eq!(session.ext.observed, 2);
        assert_eq!(FINALIZE_RUNS.load(Ordering::Relaxed), 1);
    }

    /// The two `observe_transition` reporting channels, exercised separately: the
    /// diagnostics sink records without affecting the parse (an error-severity
    /// entry does **not** abort — record-versus-abort for source conditions is
    /// `recover`'s business), while an `Err` aborts under any recovery policy.
    #[derive(Debug, Default)]
    struct Seen {
        transitions: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
    #[diagnostic(
        id = "testlang.observe.transition-observed",
        message = "a state transition was observed",
        no_constructor
    )]
    struct TransitionObserved;

    #[test]
    fn observe_transition_diagnostics_sink_records_without_aborting() {
        #[derive(Debug, Clone, Copy)]
        struct SinkLang;
        impl Lang for SinkLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = Seen;
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = SinkDriver;
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
        struct SinkDriver;
        impl TestDriver for SinkDriver {
            fn with_recovery(_recovery: Recovery) -> Self {
                SinkDriver // strict — the sink must not abort even so
            }
        }
        impl ParseDriver<SinkLang> for SinkDriver {
            fn make_token_reader<'s>(
                &'s self,
                source: &'s alloc::sync::Arc<crate::source::Source>,
            ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, SinkLang> + 's> {
                alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
            }

            fn observe_transition(
                &self,
                ext: &mut Seen,
                diagnostics: &mut crate::error::Diagnostics,
                _prev: &ParsingState<SinkLang>,
                _new: &ParsingState<SinkLang>,
                _delta: &ParsingStateDelta<SinkLang>,
            ) -> Result<(), ParseError> {
                ext.transitions += 1;
                // An error-severity observation: recorded, never an abort.
                let scratch: Arc<Source> = Arc::new(Source::new(""));
                diagnostics.push(crate::error::Diagnostic::error(
                    TransitionObserved,
                    SourceSpan::new(&scratch, 0..0),
                ));
                Ok(())
            }
        }

        // One group descent = one observed transition; the parse completes.
        let st = state_with(rules::<SinkLang>());
        let content = "{a}";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let parsed =
            try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none())
                .expect("the sink records; it never aborts");
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let recorded = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(recorded.severity(), crate::error::Severity::Error);
        assert_eq!(recorded.identifier(), "testlang.observe.transition-observed");
        assert_eq!(shapes(&parsed.result), ["group 0..3"]);
    }

    #[test]
    fn a_failing_observe_transition_aborts_under_any_policy() {
        #[derive(Debug, Clone, Copy)]
        struct FailObserveLang;
        impl Lang for FailObserveLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = Seen;
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = FailObserveDriver;
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
        struct FailObserveDriver;
        impl TestDriver for FailObserveDriver {
            fn with_recovery(_recovery: Recovery) -> Self {
                FailObserveDriver // tolerant by hand: recovery() below
            }
        }
        impl ParseDriver<FailObserveLang> for FailObserveDriver {
            fn make_token_reader<'s>(
                &'s self,
                source: &'s alloc::sync::Arc<crate::source::Source>,
            ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, FailObserveLang> + 's> {
                alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
            }

            fn recovery(&self) -> Recovery {
                Recovery::Tolerant
            }
            fn observe_transition(
                &self,
                ext: &mut Seen,
                _diagnostics: &mut crate::error::Diagnostics,
                _prev: &ParsingState<FailObserveLang>,
                _new: &ParsingState<FailObserveLang>,
                _delta: &ParsingStateDelta<FailObserveLang>,
            ) -> Result<(), ParseError> {
                ext.transitions += 1;
                if ext.transitions < 2 {
                    return Ok(());
                }
                // The second transition (the inner group's interior derivation,
                // inside the outer group's frame) is the truly problematic state.
                let scratch: Arc<Source> = Arc::new(Source::new(""));
                Err(ParseError::new(
                    crate::error::HookFailed::new("observer backend gone", None),
                    SourceSpan::new(&scratch, 0..0),
                ))
            }
        }

        let st = state_with(rules::<FailObserveLang>());
        let content = "{{a}}";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let error =
            try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none())
                .unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        assert_eq!(
            error.message(),
            "extension hook reported a failure: observer backend gone"
        );
        // The inner descent's consultation runs inside the outer group's frame.
        assert!(!error.frames().is_empty());
    }

    // --- everything at once ----------------------------------------------------------------

    #[test]
    fn mixed_content_with_groups_partitions_exactly() {
        let st = state();
        let content = "ab {cd %e\nf} g\n\nh";
        let parsed =
            run_both(content, &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            [
                "chars 0..3 \"ab \"",
                "group 3..12",
                "chars 12..14 \" g\"",
                "chars 14..16 \"\\n\\n\"",
                "chars 16..17 \"h\"",
            ]
        );
        let group = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(group.child_count(), 3);
        assert_eq!(group.child(0).unwrap().chars(), Some("cd "));
        let comment = group.child(1).unwrap();
        assert_eq!(
            comment.comment().map(|data| data.content.resolve(comment.source())),
            Some("e")
        );
        assert_eq!(group.child(2).unwrap().chars(), Some("f"));
        assert_partition(&parsed.result, 0..content.len());
    }

    #[test]
    fn mixed_content_shapes_and_partition() {
        let st = state();
        let content = "ab %c\nd\n\n e \\foo f";
        let parsed =
            run_both(content, &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            [
                "chars 0..3 \"ab \"",
                "comment 3..6 start=\"%\" content=\"c\" post=\"\\n\"",
                "chars 6..7 \"d\"",
                "chars 7..9 \"\\n\\n\"",
                "chars 9..12 \" e \"",
                "chars 12..17 \"\\\\foo \"",
                "chars 17..18 \"f\"",
            ]
        );
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_partition(&parsed.result, 0..content.len());
    }

    // --- the driver's construct provision + descent-delta channel, end to end (7.2) -----

    /// One custom driver exercising every provision seam at once: all three factories
    /// intercepted (counted, then delegating to the standard parsers), command
    /// resolution via libraries, and the D2 math plug — `group_interior_delta` returns
    /// a mode-entering delta for the math group class, so `$…$` interiors parse (and
    /// record their nodes) in math mode with zero core changes.
    #[test]
    fn custom_driver_intercepts_every_factory_and_math_groups_enter_math_mode() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        use super::super::GroupParser;

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        enum Mode {
            #[default]
            Text,
            Math,
        }

        #[derive(Debug, Clone, Copy)]
        struct DriveLang;
        impl Lang for DriveLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = Mode;
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = DriveDriver;
            fn make_node_ext(
                _kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) -> Result<(), crate::node::NodeBuildError> {
                Ok(())
            }
        }

        #[derive(Debug, Default)]
        struct DriveDriver {
            nodes_parsers: AtomicUsize,
            group_parsers: AtomicUsize,
            invocation_parsers: AtomicUsize,
        }

        impl ParseDriver<DriveLang> for DriveDriver {
            fn make_token_reader<'s>(
                &'s self,
                source: &'s alloc::sync::Arc<crate::source::Source>,
            ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, DriveLang> + 's> {
                alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
            }

            fn resolve_command(
                &self,
                state: &ParsingState<DriveLang>,
                token: &StdToken<DriveLang>,
                tokens: &dyn TokenReader<'_, DriveLang>,
            ) -> Result<CommandResolution<DriveLang>, ParseError> {
                resolve_macro_in_scopes(state, token, tokens)
            }

            fn group_interior_delta(
                &self,
                _base: &ParsingState<DriveLang>,
                rule: &Arc<GroupRule<DriveLang>>,
            ) -> Option<ParsingStateDelta<DriveLang>> {
                (rule.group_type == GT_MATH)
                    .then(|| ParsingStateDelta::new().mode(Mode::Math))
            }

            fn make_nodes_parser<'p>(
                &'p self,
                stop: StopSpec<'p, DriveLang>,
                child_states: ChildStateSpec<'p, DriveLang>,
            ) -> Result<
                Box<dyn ConstructParser<DriveLang, Output = NodesOutcome<DriveLang>> + 'p>,
                ParseError,
            > {
                self.nodes_parsers.fetch_add(1, Ordering::Relaxed);
                Ok(Box::new(NodesParser::new(stop).with_child_states(child_states)))
            }

            fn make_group_parser<'p>(
                &'p self,
                open: &StdToken<DriveLang>,
                rule: Arc<GroupRule<DriveLang>>,
                child_states: ChildStateSpec<'p, DriveLang>,
            ) -> Result<Box<dyn ConstructParser<DriveLang, Output = BuildId> + 'p>, ParseError>
            {
                self.group_parsers.fetch_add(1, Ordering::Relaxed);
                Ok(Box::new(
                    GroupParser::new(open.clone(), rule).with_child_states(child_states),
                ))
            }

            fn make_invocation_parser<'a>(
                &'a self,
                invocation: Invocation<'a, DriveLang>,
            ) -> Result<Box<dyn ConstructParser<DriveLang, Output = BuildId> + 'a>, ParseError>
            {
                self.invocation_parsers.fetch_add(1, Ordering::Relaxed);
                let spec = invocation.spec;
                spec.make_invocation_parser(invocation)
            }
        }

        // `{a}$m$\m {y}`: a brace group, a math group, a zero-arg macro, a sibling
        // brace group — one root drive, three group descents, one invocation.
        let content = "{a}$m$\\m {y}";
        let mut rules = rules::<DriveLang>();
        rules.groups.rules.push(math_rule());
        let mut scopes = ScopeStack::new();
        scopes.push(macro_library::<DriveLang>(&["m"]));
        let st = Arc::new(ParsingState::new(StateData {
            rules,
            scopes,
            mode: Mode::Text,
            ext: (),
        }));
        let st = Arc::new(st.derived(&ParsingStateDelta::new()).unwrap()); // through the choke point
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let mut session: ParserSession<DriveLang> = ParserSession::new();
        let driver = DriveDriver::default();
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&st),
            &mut session,
            &driver);

        // The top-level drive goes through the same seam as every interior descent.
        let (outcome, delta) = cx
            .parse_nodes(Arc::clone(&st), StopSpec::none(), ChildStateSpec::inherit())
            .unwrap();
        assert!(delta.is_none());
        assert!(matches!(outcome.stop, StopCause::EndOfInput));

        let root = session
            .builder
            .add(
                NodeKind::list(),
                SourceSpan::new(&source, 0..content.len()),
                Arc::clone(&st),
                outcome.nodes, (), (),
            )
            .unwrap();
        let result = session.finish(root).unwrap();
        crate::node::check_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        assert_eq!(
            shapes(&result),
            ["group 0..3", "group 3..6", "callable 6..9", "group 9..12"]
        );

        // Factory counts: 1 root + 3 group interiors = 4 nodes descents; 3 group
        // descents; 1 invocation.
        assert_eq!(driver.nodes_parsers.load(Ordering::Relaxed), 4);
        assert_eq!(driver.group_parsers.load(Ordering::Relaxed), 3);
        assert_eq!(driver.invocation_parsers.load(Ordering::Relaxed), 1);

        // The math plug: the `$…$` interior's nodes record a Math-mode state; the
        // brace interiors stay in Text mode (the driver's delta keyed on the class).
        let math_child = result.tree.root().child(1).unwrap().child(0).unwrap();
        assert_eq!(math_child.chars(), Some("m"));
        assert_eq!(math_child.parsing_state().mode(), Mode::Math);
        let brace_child = result.tree.root().child(0).unwrap().child(0).unwrap();
        assert_eq!(brace_child.chars(), Some("a"));
        assert_eq!(brace_child.parsing_state().mode(), Mode::Text);
        // The mode scoped structurally: content after the math group is Text again.
        let after = result.tree.root().child(3).unwrap().child(0).unwrap();
        assert_eq!(after.chars(), Some("y"));
        assert_eq!(after.parsing_state().mode(), Mode::Text);
    }

    #[test]
    fn a_failing_invocation_parser_factory_aborts_under_any_policy() {
        // The hook-fallibility contract on the parser factories: a factory Err
        // means "the parser could not be built" and stops the parse even in
        // tolerant mode; the dispatch site attaches the live traceback (the
        // factory has no session access). Distinct from a depth refusal — that
        // stays the descent guard's business (`DescentLimitExceeded`).
        #[derive(Debug)]
        struct BrokenFactorySpec;
        impl crate::serialize::SerializableObject<CmdLang> for BrokenFactorySpec {}
        impl CallableSpec<CmdLang> for BrokenFactorySpec {
            fn make_invocation_parser<'a>(
                &'a self,
                _invocation: Invocation<'a, CmdLang>,
            ) -> Result<Box<dyn ConstructParser<CmdLang, Output = BuildId> + 'a>, ParseError>
            {
                let scratch: Arc<Source> = Arc::new(Source::new(""));
                Err(ParseError::new(
                    crate::error::HookFailed::new("parser backend unavailable", None),
                    SourceSpan::new(&scratch, 0..0),
                ))
            }
        }

        let mut lib = Package::new("broken");
        lib.insert(CT_MACRO, "fail", BrokenFactorySpec);
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(lib));
        let st: Arc<ParsingState<CmdLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), scopes, mode: (), ext: () }));

        let content = "a {\\fail} b";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let error =
            try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none())
                .unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        assert_eq!(
            error.message(),
            "extension hook reported a failure: parser backend unavailable"
        );
        // The dispatch pushes the invocation's own frame around the factory call,
        // so the traceback names the failing spec — inside the group's frame.
        assert_eq!(error.frames().len(), 2);
        assert_eq!(error.frames()[0].title(), "callable ‘\\fail’");
        assert_eq!(error.frames()[1].title(), "group ‘{’");
    }

    #[test]
    fn a_failing_nodes_parser_factory_aborts_under_any_policy() {
        // The same contract on the driver's content-loop factory, exercised at a
        // group descent (the interior's nodes run is built through
        // `ParseContext::parse_nodes`, which lifts the factory Err and attaches
        // the live traceback).
        #[derive(Debug, Clone, Copy)]
        struct BrokenLang;
        impl Lang for BrokenLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = BrokenDriver;
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
        struct BrokenDriver;
        impl TestDriver for BrokenDriver {
            fn with_recovery(_recovery: Recovery) -> Self {
                BrokenDriver // tolerant by hand: recovery() below
            }
        }
        impl ParseDriver<BrokenLang> for BrokenDriver {
            fn make_token_reader<'s>(
                &'s self,
                source: &'s alloc::sync::Arc<crate::source::Source>,
            ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, BrokenLang> + 's> {
                alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
            }

            fn recovery(&self) -> Recovery {
                Recovery::Tolerant
            }
            fn make_nodes_parser<'p>(
                &'p self,
                _stop: StopSpec<'p, BrokenLang>,
                _child_states: ChildStateSpec<'p, BrokenLang>,
            ) -> Result<
                Box<dyn ConstructParser<BrokenLang, Output = NodesOutcome<BrokenLang>> + 'p>,
                ParseError,
            > {
                let scratch: Arc<Source> = Arc::new(Source::new(""));
                Err(ParseError::new(
                    crate::error::HookFailed::new("content-loop backend unavailable", None),
                    SourceSpan::new(&scratch, 0..0),
                ))
            }
        }

        // The harness drives the root loop directly, so the factory is first
        // consulted at the `{…}` descent — inside the group's frame.
        let st = state_with(rules::<BrokenLang>());
        let content = "{a}";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let error =
            try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none())
                .unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        assert_eq!(error.frames().len(), 1);
        assert_eq!(error.frames()[0].title(), "group ‘{’");
    }

    #[test]
    fn a_failing_make_node_ext_aborts_as_a_hook_failure() {
        // The mint's error channel inside a parse: `Lang::make_node_ext` errs with
        // the builder-level `NodeBuildError::ExtMintFailed`; the staging entry
        // point reports it like any other builder error, and the staging caller's
        // lift applies the condition split — the mint's reported operational
        // failure becomes `HookFailed`, while every other builder error stays
        // `ImplementationError` — an abort under any recovery policy, with the
        // live traceback attached.
        #[derive(Debug, Clone, Copy)]
        struct MintFailLang;
        impl Lang for MintFailLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = StdParseDriver;
            fn make_node_ext(
                kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) -> Result<(), crate::node::NodeBuildError> {
                if matches!(kind, NodeKind::Chars { .. }) {
                    return Err(crate::node::NodeBuildError::ExtMintFailed {
                        detail: "ext backend unavailable".into(),
                    });
                }
                Ok(())
            }
        }

        // The first chars node is staged inside the group, so the group frame is
        // live when the mint fails.
        let st = state_with(rules::<MintFailLang>());
        let content = "{a}";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let error =
            try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none())
                .unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        assert_eq!(
            error.message(),
            "extension hook reported a failure: ext backend unavailable"
        );
        // The chars node is staged inside the group, so the group frame is live.
        assert_eq!(error.frames().len(), 1);
        assert_eq!(error.frames()[0].title(), "group ‘{’");
    }

    // --- the scope stack in a driven parse (7.3): error specs, op failures, specials ---

    #[test]
    fn error_callable_spec_diagnoses_and_recovers_as_chars() {
        use crate::scopes::{CallableDefinedAsError, ErrorCallableSpec};

        let mut package: Package<CmdLang> = Package::new("cmds");
        package.insert(
            CT_MACRO,
            "gone",
            Arc::new(ErrorCallableSpec::with_detail("removed upstream"))
                as Arc<dyn CallableSpec<CmdLang>>,
        );
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(package));
        let st: Arc<ParsingState<CmdLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), scopes, mode: (), ext: () }));

        // Tolerant: the resolved error spec diagnoses and stages the trigger as a
        // chars fallback (post-space included — the token was consumed whole); the
        // parse continues past it.
        let parsed =
            run_both("a\\gone b", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..1 \"a\"", "chars 1..7 \"\\\\gone \"", "chars 7..8 \"b\""]
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.message(),
            "\u{2018}gone\u{2019} is defined to be an error: removed upstream"
        );
        let condition =
            diagnostic.data().downcast_ref::<CallableDefinedAsError>().unwrap();
        assert_eq!(condition.name, "gone");
        assert_partition(&parsed.result, 0..8);

        // Strict: the same condition aborts (through the recover funnel).
        let source: Arc<Source> = Arc::new(Source::new("a\\gone b"));
        let mut reader = StdTokenReader::new(&source);
        let error =
            try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none())
                .unwrap_err();
        assert_eq!(error.data().identifier(), CallableDefinedAsError::IDENTIFIER);
    }

    /// A lang whose driver returns a *failing* scope op as the descent delta of math
    /// groups — the in-parse op-failure path (7.3).
    #[derive(Debug, Clone, Copy)]
    struct FailingMathLang;
    impl Lang for FailingMathLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = FailingMathDriver;
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
    struct FailingMathDriver {
        recovery: Recovery,
    }

    impl TestDriver for FailingMathDriver {
        fn with_recovery(recovery: Recovery) -> Self {
            FailingMathDriver { recovery }
        }
    }

    impl ParseDriver<FailingMathLang> for FailingMathDriver {
        fn make_token_reader<'s>(
            &'s self,
            source: &'s alloc::sync::Arc<crate::source::Source>,
        ) -> alloc::boxed::Box<dyn crate::token::TokenReader<'s, FailingMathLang> + 's> {
            alloc::boxed::Box::new(crate::token::StdTokenReader::new(source))
        }

        fn recovery(&self) -> Recovery {
            self.recovery
        }

        fn group_interior_delta(
            &self,
            _base: &ParsingState<FailingMathLang>,
            rule: &Arc<GroupRule<FailingMathLang>>,
        ) -> Option<ParsingStateDelta<FailingMathLang>> {
            use crate::scopes::ScopeOp;
            (rule.group_type == GT_MATH).then(|| {
                ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "absent".into() })
            })
        }
    }

    #[test]
    fn descent_scope_op_failure_recovers_tolerantly_and_aborts_strictly() {
        use crate::constructs::ScopeOpFailed;

        let mut math_rules = rules::<FailingMathLang>();
        math_rules.groups.rules.push(math_rule());
        let st = state_with(math_rules);

        // Tolerant: the failure is reported through the recover funnel as a
        // ScopeOpFailed condition and the group parses under the ops-skipped state.
        // (Std reader only: a pre-scanned token list cannot disambiguate the closing
        // `$` of a same-delimiter group — that is expecting_group_close's job.)
        let source: Arc<Source> = Arc::new(Source::new("a$m$b"));
        let mut reader = StdTokenReader::new(&source);
        let parsed = try_run(&source, &mut reader, &st, Recovery::Tolerant, StopSpec::none())
            .expect("tolerant parse continues");
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..1 \"a\"", "group 1..4", "chars 4..5 \"b\""]
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.message(),
            "scope op failed: no provider named \u{2018}absent\u{2019} on the scope stack"
        );
        assert!(diagnostic.data().is::<ScopeOpFailed>());
        assert_partition(&parsed.result, 0..5);

        // Strict: the first failing op aborts the parse.
        let source: Arc<Source> = Arc::new(Source::new("a$m$b"));
        let mut reader = StdTokenReader::new(&source);
        let error = try_run(&source, &mut reader, &st, Recovery::Strict, StopSpec::none())
            .unwrap_err();
        assert_eq!(error.data().identifier(), ScopeOpFailed::IDENTIFIER);
    }

    /// A lang whose specials hooks fold over the state's scope stack — the standard
    /// preset wiring of the 7.3 provider-based specials.
    #[derive(Debug, Clone, Copy)]
    struct StackSpecialsLang;
    impl Lang for StackSpecialsLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn scan_specials(
        state: &ParsingState<Self>,
        content: &str,
        pos: usize,
    ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
            state.scopes().scan_specials(state, content, pos)
        }

        fn specials_trigger_chars(data: &StateData<Self>) -> TriggerChars {
            data.scopes.specials_trigger_chars()
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

    #[test]
    fn package_specials_resolve_through_the_stack_fold_end_to_end() {
        let inner_short: Arc<dyn CallableSpec<StackSpecialsLang>> =
            Arc::new(StdCallableSpec::default());
        let outer_short: Arc<dyn CallableSpec<StackSpecialsLang>> =
            Arc::new(StdCallableSpec::default());
        let outer_long: Arc<dyn CallableSpec<StackSpecialsLang>> =
            Arc::new(StdCallableSpec::default());

        let mut inner: Package<StackSpecialsLang> = Package::new("inner");
        inner.insert_specials(CT_SPECIALS, "--", Arc::clone(&inner_short));
        let mut outer: Package<StackSpecialsLang> = Package::new("outer");
        outer.insert_specials(CT_SPECIALS, "--", Arc::clone(&outer_short));
        outer.insert_specials(CT_SPECIALS, "---", Arc::clone(&outer_long));

        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(outer));
        scopes.push(Arc::new(inner));
        let st: Arc<ParsingState<StackSpecialsLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), scopes, mode: (), ext: () }));

        // `x---y--z`: the longest match wins across providers (`---`, defined outer),
        // and the equal-length tie goes innermost (`--` resolves to the inner spec).
        let parsed =
            run_both("x---y--z", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            [
                "chars 0..1 \"x\"",
                "callable 1..4",
                "chars 4..5 \"y\"",
                "callable 5..7",
                "chars 7..8 \"z\""
            ]
        );
        let long = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(long.name(), Some("---"));
        assert_eq!(long.callable_type(), Some(CT_SPECIALS));
        assert!(Arc::ptr_eq(&long.callable().unwrap().spec, &outer_long));
        let short = parsed.result.tree.root().child(3).unwrap();
        assert_eq!(short.name(), Some("--"));
        assert!(Arc::ptr_eq(&short.callable().unwrap().spec, &inner_short));
        assert!(parsed.result.diagnostics.is_empty());
        assert_partition(&parsed.result, 0..8);
    }
}
