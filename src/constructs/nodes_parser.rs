//! [`NodesParser`]: the main content dispatch loop (ARCHITECTURE.md §constructs), with
//! its stop machinery ([`StopSpec`], [`TokenStopCondition`], [`StopCause`]).
//!
//! The parser peeks one token at a time and dispatches on its kind — never on parser
//! registries (§2.6). The content arms (6.2) cover chars accumulation, paragraph breaks
//! (via [`Lang::make_paragraph_break_node`]), comments, and end of stream. The
//! `GroupOpen` arm (6.3) descends: it resolves the interior's base state through the
//! per-use [`ChildStateSpec`] policy, consumes the trigger token, and runs a
//! [`GroupParser`] under the policy's state (structural swap/revert). The
//! `Command`/`Specials` invocation arms (6.4) descend the same way: a `Command` token
//! resolves through [`Lang::resolve_command`] under the loop's own state (resolution
//! precedes the descent policy — §3.6), a `Specials` token carries its resolution; the
//! arm consumes the trigger whole, builds the [`Invocation`], and runs the parser
//! returned by the spec's
//! [`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser)
//! factory under the policy's state. The state delta an invocation parser returns is
//! the invocation's after-effect for subsequent siblings (`\newcommand`), applied to
//! the loop's own state session-mediated (`cx.session.derived_state(…)` —
//! DESIGN_RATIONALE.md §3.6).
//!
//! # Whitespace and span invariants (DESIGN_RATIONALE.md §3.5, pinned)
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
//! A [`StopSpec`] carries two independent triggers (decided July 2026, §3.6): a *token*
//! condition tested on peek — a match ends the parse and, per the condition's
//! [`consume`](TokenStopCondition::consume) switch, either leaves the token unconsumed
//! for the caller or consumes it here — and a *node* condition tested after each staged
//! node — a match includes that node and stops after it. Conditions are tested only at
//! this parser's own nesting level (a nested group is consumed whole by the group
//! parser). Abnormal endings are **data**, not errors: the parser reports its
//! [`StopCause`] and the caller decides (§3.8 rule 2) — an unexpected group close stays
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
//! would match — the token condition wins outright (§3.6): that flush does **not**
//! consult the node predicate. Its answer could change nothing (the parse ends as
//! `TokenCondition` either way; honoring it would instead leave a `consume = true` token
//! unconsumed, breaking the flag's atomicity), and the predicate is a stateful `FnMut`
//! that must not observe a consulted-but-ignored call.
//!
//! # Recovery (DESIGN_RATIONALE.md §3.8)
//!
//! Recovery happens where a problem is detected, through the session's policy helper.
//! Tokenizer errors continue with their [`TokenRecovery`](crate::token::TokenRecovery)
//! placeholder token, the reader repositioned to the error's `resume_pos` (so the error
//! is never re-read); an unresolvable command recovers as a diagnostic plus a chars
//! fallback node over the token's span (specials never take this path: recognition =
//! resolution, so a recognized trigger always dispatches).
//! Markup text inside a `Chars` node is an accepted tolerant-recovery artifact, always
//! accompanied by a diagnostic; fallback nodes are deliberately *not* merged into
//! neighboring chars runs. Group recovery (unclosed at end of input, mismatched close)
//! lives in [`GroupParser`]. `Err` means abort — nobody continues past one.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::mem;

use crate::error::{DiagnosticInfo, ParseError, ToDiagnosticValue};
use crate::node::{BuildId, NodeKind, StagedNodeView};
use crate::source::{SourceSpan, Span};
use crate::state::{CommandResolution, Lang, ParsingState, ParsingStateDelta};
use crate::token::{Token, TokenKind};

use super::child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
use super::group_parser::GroupParser;
use super::{
    invocation_frame, ConstructParser, ConstructParserResult, Invocation,
    ParseContext,
};

/// Condition: a [`Command`](TokenKind::Command) token resolved to no callable
/// ([`Lang::resolve_command`] returned no
/// [`Resolved`](crate::state::CommandResolution::Resolved)) — the content loop recovers
/// with a span-backed chars fallback (DESIGN_RATIONALE.md §3.8).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.nodes_parser.unresolvable-command")]
pub struct UnresolvableCommand {
    /// The command name, as written (without the escape character).
    pub name: String,
    /// The escape character that introduced the command.
    pub escape_char: char,
    /// Whether the failure came from a language with no command resolution at all
    /// ([`Unimplemented`](crate::state::CommandResolution::Unimplemented), the trait
    /// default's answer) rather than from a resolver that did not know the name; the
    /// message then carries a hint naming the missing hook.
    pub resolver_unimplemented: bool,
}

// Hand-written wording: the unimplemented-resolver hint is conditional (a branch the
// derive's message format string cannot express).
impl fmt::Display for UnresolvableCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot resolve command ‘{}{}’", self.escape_char, self.name)?;
        if self.resolver_unimplemented {
            write!(
                f,
                " (this language implements no ‘Lang::resolve_command’ — the default \
                 resolves nothing; implement the hook or use a preset)"
            )?;
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
    id = "core.nodes_parser.expression-callable-requires-content",
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
#[diagnostic(id = "core.nodes_parser.unusable-recovery-token")]
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
    /// Stop at a [`Command`](TokenKind::Command) token with this name (an environment
    /// body stopping at `\end`).
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
    /// (DESIGN_RATIONALE.md §3.5) — so it is re-resolved against `cx.state`, the same
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
    /// Stop at any token matching the predicate. Programmatic conditions live only in
    /// tier-2 parser temporaries, never in spec data (DESIGN_RATIONALE.md §2.1).
    Predicate(&'p dyn Fn(&Token<'_, L>) -> bool),
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
/// temporary — construct parsers are free to borrow (two-tier ownership model, §3.6).
pub struct StopSpec<'p, L: Lang> {
    /// Token condition, tested on peek; a match ends the parse, consuming the token or
    /// leaving it per its [`consume`](TokenStopCondition::consume) switch.
    pub token: Option<TokenStopCondition<'p, L>>,
    /// Node condition, tested after each staged node with (number of nodes staged so
    /// far, view of the just-staged node); a match includes that node and stops after
    /// it. Not consulted on the final flush a matched token condition triggers — the
    /// token condition wins outright (see the module docs on the position seam). The
    /// (count, last node) signature is a deliberate deviation from pylatexenc's
    /// whole-nodelist rescans (§3.6).
    // The decided signature (DESIGN_RATIONALE.md §3.6); an alias would only rename it.
    #[allow(clippy::type_complexity)]
    pub node: Option<&'p mut dyn FnMut(usize, StagedNodeView<'_, L>) -> bool>,
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

/// How a [`NodesParser`] run ended. Abnormal endings are **data**, not errors — only the
/// caller knows whether reaching end of input before `\end{align}` is a problem
/// (DESIGN_RATIONALE.md §3.8 rule 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopCause {
    /// The token stop condition matched. `span` is the matched token's span; whether it
    /// was consumed is the [`consume`](TokenStopCondition::consume) the caller set —
    /// consumed ⇒ the reader stands just past it, otherwise it sits unconsumed at
    /// `span.start`, its pre-space already staged as sibling content.
    TokenCondition {
        /// The matched stop token's span.
        span: Span,
    },
    /// The node stop condition fired on the last staged node (the reader stands where
    /// that node ended: a directly staged node is consumed, a flush leaves the triggering
    /// token unconsumed at its own start).
    NodeCondition,
    /// [`EndOfStream`](TokenKind::EndOfStream) was reached (its trailing-whitespace
    /// node, if any, is already staged).
    EndOfInput,
    /// A group close no condition asked for; the close token is left unconsumed at
    /// `span.start` and the caller decides (diagnose-and-skip at the root, unwind in a
    /// group parser — §3.8).
    UnexpectedGroupClose {
        /// The unexpected close token's span.
        span: Span,
    },
}

/// What a [`NodesParser`] produces: the staged sibling nodes, in source order, and how
/// the run ended.
#[derive(Debug, Clone)]
pub struct NodesOutcome {
    /// The staged nodes, in source order (the caller claims them as children).
    pub nodes: Vec<BuildId>,
    /// How the parse ended.
    pub stop: StopCause,
}

/// The main content loop: parses a sequence of sibling nodes until a stop condition,
/// an unexpected group close, or end of input (pylatexenc's `LatexGeneralNodesParser`
/// plus its nodes collector).
///
/// A tier-2 temporary: constructed with its per-use configuration (the source the token
/// spans refer into, and the [`StopSpec`]), working state in fields, dropped with the
/// frame. The input parsing state is `cx.state` (the caller sets it); sibling deltas
/// returned by invocation parsers are applied internally as the loop proceeds, and the
/// parser itself returns `None` as its pass-through delta (§2 state-threading
/// convention — no current consumer of a merged delta).
pub struct NodesParser<'p, L: Lang> {
    stop: StopSpec<'p, L>,
    /// Descent-state policy for child constructs (groups and invocations); defaults to
    /// inherit-everywhere.
    child_states: ChildStateSpec<'p, L>,
    nodes: Vec<BuildId>,
    /// The pending maximal chars run (invariant 1): extended by `Char` tokens and every
    /// token's pre-space, flushed when a non-`Char` construct starts.
    run: Option<Span>,
}

impl<'p, L: Lang> NodesParser<'p, L> {
    /// A parser staging nodes with spans into the context's [`source`](ParseContext::source),
    /// stopping per `stop`.
    pub fn new(stop: StopSpec<'p, L>) -> NodesParser<'p, L> {
        NodesParser {
            stop,
            child_states: ChildStateSpec::inherit(),
            nodes: Vec::new(),
            run: None,
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
    /// whitespace-only run).
    fn take_pre_space(&mut self, pre_space: Span) {
        if pre_space.is_empty() {
            return;
        }
        match &mut self.run {
            Some(run) => {
                debug_assert!(
                    run.end() == pre_space.start(),
                    "pre-space {:?} is not contiguous with the pending run {:?}",
                    pre_space,
                    run
                );
                run.extend_to(pre_space.end());
            }
            None => self.run = Some(pre_space),
        }
    }

    /// The `Char` arm: pre-space and the character extend the pending run.
    fn extend_run(&mut self, token: &Token<'_, L>) {
        self.take_pre_space(token.pre_space);
        match &mut self.run {
            Some(run) => {
                debug_assert!(
                    run.end() == token.span.start(),
                    "char token {:?} is not contiguous with the pending run {:?}",
                    token.span,
                    run
                );
                run.extend_to(token.span.end());
            }
            None => self.run = Some(token.span),
        }
    }

    /// Flush the pending run as a `Chars` node (span-backed over the exact run slice).
    /// Returns whether the node stop condition fired on it.
    fn flush(&mut self, cx: &mut ParseContext<'_, '_, L>) -> ConstructParserResult<L, bool> {
        match self.run.take() {
            Some(run) => self.stage_node(cx, NodeKind::chars(run), run),
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
        pre_space: Span,
    ) -> ConstructParserResult<L, bool> {
        self.take_pre_space(pre_space);
        self.flush(cx)
    }

    /// [`flush_through`](Self::flush_through) minus the node-condition test: the flush
    /// performed when the token stop condition has matched. The stop token's pre-space
    /// is interior content and must land in a sibling node (partition invariant), but
    /// the token condition has already ended the parse and wins outright (§3.6): a
    /// node-condition match here could not change the outcome, and honoring it instead
    /// would leave a `consume = true` stop token unconsumed, forfeiting the consume
    /// flag's atomicity guarantee. The predicate is a stateful `FnMut`, so even a
    /// consulted-but-ignored call would be an observable side effect — it is not
    /// consulted at all.
    fn flush_for_token_stop(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        pre_space: Span,
    ) -> ConstructParserResult<L, ()> {
        self.take_pre_space(pre_space);
        if let Some(run) = self.run.take() {
            self.stage(cx, NodeKind::chars(run), run)?;
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
        span: Span,
    ) -> ConstructParserResult<L, BuildId> {
        let id = cx
            .session
            .builder
            .add(kind, SourceSpan::new(&cx.source, span), Arc::clone(&cx.state), vec![])
            .map_err(|error| cx.implementation_error(error, span))?;
        self.nodes.push(id);
        Ok(id)
    }

    /// Stage a childless node ([`stage`](Self::stage)) and test the node stop condition
    /// on it.
    fn stage_node(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        kind: NodeKind<L>,
        span: Span,
    ) -> ConstructParserResult<L, bool> {
        let id = self.stage(cx, kind, span)?;
        Ok(self.test_node_stop(cx, id))
    }

    /// Test the node stop condition against an already-recorded sibling (the last entry
    /// of `self.nodes` — staged either by [`stage`](Self::stage) or by a child construct
    /// parser).
    fn test_node_stop(&mut self, cx: &mut ParseContext<'_, '_, L>, id: BuildId) -> bool {
        let Some(condition) = &mut self.stop.node else {
            return false;
        };
        let staged = cx.session.builder.staged_nodes();
        // A miss means a child construct parser handed back a foreign id (an
        // implementation bug — the builder diagnoses it when the id lands in a child
        // list); treat it as "condition did not fire" rather than panic (panic policy).
        let Some(view) = staged.get(id) else {
            return false;
        };
        condition(self.nodes.len(), view)
    }

    /// If the token stop condition matches the peeked token, whether it is to be consumed
    /// ([`TokenStopCondition::consume`]); `None` when no token condition matches.
    fn token_stop(&self, state: &ParsingState<L>, token: &Token<'_, L>) -> Option<bool> {
        let cond = self.stop.token.as_ref()?;
        let matches = match &cond.kind {
            TokenStopKind::Command { name } => {
                matches!(&token.kind, TokenKind::Command { name: n, .. } if n == name)
            }
            // Both the spelling and the state-resolved class must match the pairing the
            // group opened with (a `]` sharing the class, or a `}` a delta re-classed,
            // must not close it).
            TokenStopKind::GroupClose { group_type, close } => match &token.kind {
                TokenKind::GroupClose { delim } => {
                    delim == close && group_close_type(state, delim) == Some(*group_type)
                }
                _ => false,
            },
            TokenStopKind::ParagraphBreak => matches!(token.kind, TokenKind::ParagraphBreak),
            TokenStopKind::Predicate(predicate) => predicate(token),
        };
        matches.then_some(cond.consume)
    }

    /// The shared tolerant recovery of the not-yet-wired arms (`Command` until
    /// resolution dispatch lands in 6.4, `Specials` likewise, plus recovery-placeholder
    /// `GroupOpen` tokens) — and the decided unresolvable-command recovery (§3.8):
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
        token: &Token<'s, L>,
        recovered: bool,
        condition: impl DiagnosticInfo,
    ) -> ConstructParserResult<L, bool> {
        if self.flush_through(cx, token.pre_space)? {
            if !recovered {
                cx.tokens.move_to(token, false);
            }
            return Ok(true);
        }
        cx.recover(condition, SourceSpan::new(&cx.source, token.span))?;
        if !recovered {
            cx.tokens.move_past(token, true);
        }
        self.stage_node(cx, NodeKind::chars(token.span), token.span)
    }

    /// Dispatch a resolved invocation (the `Command`/`Specials` arms): resolve the
    /// descent base state through the [`invocation`](ChildStateSpec::invocation) policy
    /// (resolution already ran under the loop's own state — resolution precedes policy,
    /// §3.6), consume the trigger token **whole** (syntactic post-space included —
    /// mirroring the `GroupOpen` arm, so loop progress holds by construction), run the
    /// spec's invocation parser under the policy state (structural swap/revert), record
    /// the staged node, and apply the parser's after-effect delta to the loop's own
    /// state for subsequent siblings (`\newcommand` — session-mediated, so the
    /// transition is observed). Returns whether the node stop condition fired.
    fn dispatch_invocation<'s>(
        &mut self,
        cx: &mut ParseContext<'_, 's, L>,
        invocation: Invocation<'_, 's, L>,
    ) -> ConstructParserResult<L, bool> {
        // `Arc` in, `Arc` out — pass-through policies preserve pointer identity.
        let base = match &self.child_states.invocation {
            InvocationChildState::Inherit => Arc::clone(&cx.state),
            InvocationChildState::Fixed(state) => Arc::clone(state),
            InvocationChildState::Compute(compute) => compute(&cx.state, &invocation),
        };
        // The invocation's traceback frame — built before the `Invocation` moves into
        // the factory, pushed around the parser run (the dispatch push site, §3.8).
        let frame = invocation_frame(cx, &invocation);
        cx.tokens.move_past(invocation.token, true);
        let spec = invocation.spec;
        let mut parser = spec.make_invocation_parser(invocation);
        let result = cx.with_frame(frame, |cx| cx.parse_scoped(base, &mut *parser));
        drop(parser);
        let (id, delta) = result?;
        self.nodes.push(id);
        if let Some(delta) = delta {
            // The after-effect applies to the loop's own state, not the policy base
            // (decided semantics 4, §3.6 — §3.3's outward propagation blesses applying
            // a delta to a base the producer never saw).
            cx.state = cx.session.derived_state(&cx.state, &delta);
        }
        Ok(self.test_node_stop(cx, id))
    }

    /// Drain the collected siblings into the outcome.
    fn outcome(&mut self, stop: StopCause) -> NodesOutcome {
        NodesOutcome { nodes: mem::take(&mut self.nodes), stop }
    }
}

impl<L: Lang> ConstructParser<L> for NodesParser<'_, L> {
    type Output = NodesOutcome;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (NodesOutcome, Option<ParsingStateDelta<L>>)> {
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
                    let span = SourceSpan::new(&cx.source, error.span());
                    match error.into_recovery() {
                        None => {
                            return Err(ParseError::from_token_error(kind, span)
                                .with_frames(cx.session.snapshot_frames()))
                        }
                        Some(recovery) => {
                            // The lift boxes the built-in token conditions and unwraps
                            // a `Custom` payload (never double-boxed) — §3.8.
                            cx.recover_boxed(kind.clone().into_condition(), span.clone())?;
                            // The recovery arm is the one arm that consumes no token,
                            // so loop progress rests entirely on the resume position
                            // advancing the reader (the `TokenRecovery::resume_pos`
                            // contract). A violating token source — a custom reader or
                            // `Lang::scan_specials` — would otherwise re-read the same
                            // error forever; degrade the hang into an abort, even in
                            // tolerant mode: its promise is a best-effort tree, not
                            // tolerance of non-termination.
                            let before = cx.tokens.pos();
                            cx.tokens.move_to_pos(recovery.resume_pos);
                            if cx.tokens.pos() <= before {
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
            // stop token that cannot be re-read cannot be left for the caller).
            if !recovered {
                if let Some(consume) = self.token_stop(&cx.state, &token) {
                    self.flush_for_token_stop(cx, token.pre_space)?;
                    if consume {
                        // Take the whole token, syntactic post-space included; its
                        // pre-space is already housed in the flushed sibling nodes.
                        cx.tokens.move_past(&token, true);
                    } else {
                        cx.tokens.move_to(&token, false);
                    }
                    let span = token.span;
                    return Ok((self.outcome(StopCause::TokenCondition { span }), None));
                }
            }

            match &token.kind {
                TokenKind::Char(_) => {
                    self.extend_run(&token);
                    if !recovered {
                        cx.tokens.move_past(&token, true);
                    }
                }

                TokenKind::EndOfStream => {
                    // Invariant 4: the terminal token's pre-space is the input's
                    // trailing whitespace and reaches the tree.
                    let fired = self.flush_through(cx, token.pre_space)?;
                    if !recovered {
                        cx.tokens.move_to(&token, false);
                    }
                    let cause =
                        if fired { StopCause::NodeCondition } else { StopCause::EndOfInput };
                    return Ok((self.outcome(cause), None));
                }

                TokenKind::GroupClose { .. } => {
                    // A close the stop condition did not ask for: report it as data and
                    // let the caller decide (§3.8 rule 2); the token stays unconsumed.
                    let fired = self.flush_through(cx, token.pre_space)?;
                    if !recovered {
                        cx.tokens.move_to(&token, false);
                    }
                    let cause = if fired {
                        StopCause::NodeCondition
                    } else {
                        StopCause::UnexpectedGroupClose { span: token.span }
                    };
                    return Ok((self.outcome(cause), None));
                }

                TokenKind::ParagraphBreak => {
                    if self.flush_through(cx, token.pre_space)? {
                        if !recovered {
                            cx.tokens.move_to(&token, false);
                        }
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                    // Invariant 2: the break is its own node — the hook's kind, staged
                    // by the loop over the full token span (a `Lang` cannot stage nodes
                    // itself); runs never merge across it.
                    let kind = L::make_paragraph_break_node(&cx.state, &token);
                    if !recovered {
                        cx.tokens.move_past(&token, true);
                    }
                    if self.stage_node(cx, kind, token.span)? {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                }

                TokenKind::Comment { start, post_space, .. } => {
                    if self.flush_through(cx, token.pre_space)? {
                        if !recovered {
                            cx.tokens.move_to(&token, false);
                        }
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                    // The token's sub-spans tile its span: start delimiter, content,
                    // post-space.
                    let content_span = Span::new(start.end(), post_space.start());
                    let kind = NodeKind::comment(*start, content_span, *post_space);
                    if !recovered {
                        cx.tokens.move_past(&token, true);
                    }
                    if self.stage_node(cx, kind, token.span)? {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                }

                TokenKind::Command { name, escape_char, .. } => {
                    // Resolution runs under the loop's own state — coherent with the
                    // state that tokenized the token (resolution precedes policy,
                    // §3.6). A recovery placeholder is never dispatched: its site
                    // already diagnosed it, and a token with no real bytes behind it
                    // cannot be consumed by the arm (`Unknown`, not `Unimplemented`:
                    // the hook never ran).
                    let resolved = if recovered {
                        CommandResolution::Unknown
                    } else {
                        L::resolve_command(&cx.state, &token)
                    };
                    match resolved {
                        CommandResolution::Resolved(resolved) => {
                            if self.flush_through(cx, token.pre_space)? {
                                cx.tokens.move_to(&token, false);
                                return Ok((self.outcome(StopCause::NodeCondition), None));
                            }
                            let invocation = Invocation {
                                callable_type: resolved.callable_type,
                                name,
                                spec: &resolved.spec,
                                token: &token,
                            };
                            if self.dispatch_invocation(cx, invocation)? {
                                return Ok((self.outcome(StopCause::NodeCondition), None));
                            }
                        }
                        unresolved @ (CommandResolution::Unknown
                        | CommandResolution::Unimplemented) => {
                            // Unresolvable command (§3.8): diagnostic plus span-backed
                            // chars fallback.
                            let condition = UnresolvableCommand::new(
                                *name,
                                *escape_char,
                                matches!(unresolved, CommandResolution::Unimplemented),
                            );
                            if self.recover_as_chars(cx, &token, recovered, condition)? {
                                return Ok((self.outcome(StopCause::NodeCondition), None));
                            }
                        }
                    }
                }

                TokenKind::Specials { callable_type, name, spec } => {
                    // Recognition = resolution: the token carries the full resolution
                    // (callable type + spec). Recovery placeholders are never
                    // dispatched, as for commands.
                    if recovered {
                        let condition = UnusableRecoveryToken::new(
                            *name,
                            UnusableRecoveryTokenKind::Specials,
                        );
                        if self.recover_as_chars(cx, &token, recovered, condition)? {
                            return Ok((self.outcome(StopCause::NodeCondition), None));
                        }
                        continue;
                    }
                    if self.flush_through(cx, token.pre_space)? {
                        cx.tokens.move_to(&token, false);
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                    let invocation = Invocation {
                        callable_type: *callable_type,
                        name,
                        spec,
                        token: &token,
                    };
                    if self.dispatch_invocation(cx, invocation)? {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                }

                TokenKind::GroupOpen { delim, rule } => {
                    // A recovery placeholder GroupOpen (no current TokenRecovery emits
                    // one) has no real bytes behind it and cannot be parsed as a group:
                    // chars fallback, reader untouched.
                    if recovered {
                        let condition = UnusableRecoveryToken::new(
                            *delim,
                            UnusableRecoveryTokenKind::GroupOpen,
                        );
                        if self.recover_as_chars(cx, &token, recovered, condition)? {
                            return Ok((self.outcome(StopCause::NodeCondition), None));
                        }
                        continue;
                    }
                    if self.flush_through(cx, token.pre_space)? {
                        cx.tokens.move_to(&token, false);
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                    // The interior's *base* state per the descent policy (the group
                    // parser derives expecting_group_close from it); `Arc` in, `Arc`
                    // out — pass-through policies preserve pointer identity.
                    let base = match &self.child_states.group {
                        GroupChildState::Inherit => Arc::clone(&cx.state),
                        GroupChildState::Fixed(state) => Arc::clone(state),
                        GroupChildState::Compute(compute) => compute(&cx.state, &token),
                    };
                    // Consume the trigger token here, at the site that peeked it and
                    // under the state that tokenized it (the at-match-time atomicity
                    // rule); the group parser gets its two facts — open span and rule.
                    cx.tokens.move_past(&token, true);
                    let mut group = GroupParser::new(token.span, Arc::clone(rule));
                    // The parser's input state is the policy's answer, scoped to the
                    // descent.
                    let (id, _delta) = cx.parse_scoped(base, &mut group)?; // groups have no after-effect
                    self.nodes.push(id);
                    if self.test_node_stop(cx, id) {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
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
    if let Some(rule) = &state.rules().expecting_group_close {
        if rule.close == delim {
            return Some(rule.group_type);
        }
    }
    state
        .prefix_table()
        .match_at(delim)
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
    use crate::engine::{ParseResult, ParserSession};
    use crate::error::Recovery;
    use crate::library::{CallableQuery, CallableSyntax, Library, LibraryStack};
    use crate::node::{GroupData, NodeExt, StagedNodes};
    use crate::source::{Source, TextContent};
    use crate::spec::{CallableSpec, StdCallableSpec};
    use super::super::{InvocationChildState, StdInvocationParser};
    use crate::state::{
        NodeExtTypes, ResolvedCallable, SimpleLang, StateData, TokenRulesOverrides,
    };
    use crate::token::{
        CommandRule, CommentRule, GroupRule, SpecialsMatch, StdTokenReader, TokenError,
        TokenErrorKind, TokenListReader, TokenReader, TokenRecovery, TokenResult, TokenRules,
        TriggerChars, WhitespaceRules,
    };
    use alloc::boxed::Box;
    use alloc::string::ToString;

    const GT_BRACE: u32 = 0;
    const GT_MATH: u32 = 1;
    const CT_MACRO: u32 = 10;
    const CT_SPECIALS: u32 = 11;

    #[derive(Debug, Clone, Copy)]
    struct TestLang;
    impl SimpleLang for TestLang {}

    /// The preset resolution pattern, shared by the 6.4 test langs: dispatch a
    /// `Command` token to the state's libraries under the `CT_MACRO` form
    /// ([`CallableQuery`], escape char included).
    fn resolve_macro_via_libraries<L: Lang<CallableTypeId = u32>>(
        state: &ParsingState<L>,
        token: &Token<'_, L>,
    ) -> CommandResolution<L> {
        let TokenKind::Command { name, escape_char, .. } = &token.kind else {
            return CommandResolution::Unknown;
        };
        let query = CallableQuery::new(
            CT_MACRO,
            name,
            CallableSyntax::Command { escape_char: *escape_char },
        )
        .with_token(token);
        state
            .libraries()
            .resolve(&query, state)
            .map(|spec| ResolvedCallable { callable_type: CT_MACRO, spec })
            .into()
    }

    /// Test lang resolving `Command` tokens against the state's libraries under the
    /// `CT_MACRO` form.
    #[derive(Debug, Clone, Copy)]
    struct CmdLang;
    impl Lang for CmdLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn resolve_command(
            state: &ParsingState<Self>,
            token: &Token<'_, Self>,
        ) -> CommandResolution<Self> {
            resolve_macro_via_libraries(state, token)
        }
    }

    /// Test lang recognizing `~` as a specials trigger resolving to a zero-arg spec
    /// under the `CT_SPECIALS` form.
    #[derive(Debug, Clone, Copy)]
    struct TildeLang;
    impl Lang for TildeLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn scan_specials<'s>(
            _state: &ParsingState<Self>,
            content: &'s str,
            pos: usize,
        ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
            if content[pos..].starts_with('~') {
                Ok(Some(SpecialsMatch {
                    end: pos + 1,
                    callable_type: CT_SPECIALS,
                    name: &content[pos..pos + 1],
                    spec: Arc::new(StdCallableSpec::default()),
                }))
            } else {
                Ok(None)
            }
        }

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~".into())
        }
    }

    /// A library defining each of `names` as a zero-arg `CT_MACRO` callable (one shared
    /// spec — flyweight).
    fn macro_library<L: Lang<CallableTypeId = u32> + 'static>(names: &[&str]) -> Arc<Library<L>> {
        let mut lib = Library::new("test-macros");
        let spec: Arc<dyn CallableSpec<L>> = Arc::new(StdCallableSpec::default());
        for name in names {
            lib.insert(CT_MACRO, *name, Arc::clone(&spec));
        }
        Arc::new(lib)
    }

    /// A `CmdLang` state whose library stack defines `names` as zero-arg macros.
    fn state_with_macros(names: &[&str]) -> Arc<ParsingState<CmdLang>> {
        let mut libraries = LibraryStack::new();
        libraries.push(macro_library(names));
        Arc::new(ParsingState::new(StateData { rules: rules(), libraries, ext: () }))
    }

    fn math_rule<L: Lang<GroupTypeId = u32>>() -> Arc<GroupRule<L>> {
        Arc::new(GroupRule { group_type: GT_MATH, open: "$".into(), close: "$".into() })
    }

    fn rules<L: Lang<GroupTypeId = u32>>() -> TokenRules<L> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups: vec![Arc::new(GroupRule {
                group_type: GT_BRACE,
                open: "{".into(),
                close: "}".into(),
            })],
            temporary_groups: Vec::new(),
            enable_commands: true,
            commands: vec![Arc::new(CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            })],
            enable_comments: true,
            comments: vec![Arc::new(CommentRule { start: "%".into() })],
            enable_specials: true,
            forbidden_chars: "".into(),
            expecting_group_close: None,
        }
    }

    fn state_with<L: Lang<GroupTypeId = u32, StateExt = ()>>(
        rules: TokenRules<L>,
    ) -> Arc<ParsingState<L>> {
        Arc::new(ParsingState::new(StateData {
            rules,
            libraries: LibraryStack::new(),
            ext: (),
        }))
    }

    fn state() -> Arc<ParsingState<TestLang>> {
        state_with(rules())
    }

    // --- harness ------------------------------------------------------------------------

    struct Parsed<L: Lang> {
        result: ParseResult<L>,
        stop: StopCause,
        pos: usize,
    }

    impl<L: Lang> fmt::Debug for Parsed<L> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Parsed")
                .field("shapes", &shapes(&self.result))
                .field("stop", &self.stop)
                .field("pos", &self.pos)
                .finish()
        }
    }

    /// Drive a `NodesParser` over `tokens`, stage the outcome under a root `List`
    /// spanning exactly the parsed extent, freeze, and run the invariant checker over
    /// the finished tree. The reader must be reading `content`.
    fn try_run<'s, L: Lang<SourceOrigin = Option<String>>>(
        content: &'s str,
        tokens: &mut dyn TokenReader<'s, L>,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop: StopSpec<'_, L>,
    ) -> Result<Parsed<L>, ParseError> {
        try_run_with(content, tokens, state, recovery, stop, ChildStateSpec::inherit())
    }

    /// [`try_run`] with an explicit descent-state policy.
    fn try_run_with<'s, 'p, L: Lang<SourceOrigin = Option<String>>>(
        content: &'s str,
        tokens: &mut dyn TokenReader<'s, L>,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop: StopSpec<'p, L>,
        child_states: ChildStateSpec<'p, L>,
    ) -> Result<Parsed<L>, ParseError> {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut session = ParserSession::new(recovery);
        let mut cx = ParseContext::new(tokens, Arc::clone(&source), Arc::clone(state), &mut session);
        let mut parser = NodesParser::new(stop).with_child_states(child_states);
        let (outcome, delta) = parser.parse(&mut cx)?;
        assert!(delta.is_none(), "NodesParser returns no pass-through delta");
        let pos = cx.tokens.pos();
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
        let root = session.builder.add(
            NodeKind::list(),
            SourceSpan::new(&source, root_span),
            Arc::clone(state),
            outcome.nodes,
        ).unwrap();
        let result = session.finish(root).unwrap();
        crate::node::check_tree_invariants(&result.tree);
        Ok(Parsed { result, stop: outcome.stop, pos })
    }

    /// Scan `content` into the full token list (including the terminal `EndOfStream`).
    fn scan<'s, L: Lang>(content: &'s str, state: &Arc<ParsingState<L>>) -> Vec<Token<'s, L>> {
        let mut reader = StdTokenReader::new(content);
        let mut tokens = Vec::new();
        loop {
            let token = TokenReader::next(&mut reader, state).expect("clean scan");
            let done = matches!(token.kind, TokenKind::EndOfStream);
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
    fn run_both<'p, L: Lang<SourceOrigin = Option<String>>>(
        content: &str,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop_std: StopSpec<'p, L>,
        stop_list: StopSpec<'p, L>,
    ) -> Parsed<L> {
        run_both_with(content, state, recovery, stop_std, stop_list, ChildStateSpec::inherit())
    }

    /// [`run_both`] with an explicit descent-state policy (cloned per reader — it is
    /// shallow: `Arc`s and borrows).
    fn run_both_with<'p, L: Lang<SourceOrigin = Option<String>>>(
        content: &str,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop_std: StopSpec<'p, L>,
        stop_list: StopSpec<'p, L>,
        child_states: ChildStateSpec<'p, L>,
    ) -> Parsed<L> {
        let mut std_reader = StdTokenReader::new(content);
        let a = try_run_with(
            content,
            &mut std_reader,
            state,
            recovery,
            stop_std,
            child_states.clone(),
        )
        .expect("std reader");
        let mut list_reader = TokenListReader::new(scan(content, state));
        let b =
            try_run_with(content, &mut list_reader, state, recovery, stop_list, child_states)
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
            .map(|node| {
                let span = format!("{}..{}", node.span().start(), node.span().end());
                match node.kind() {
                    NodeKind::Chars { .. } => {
                        format!("chars {} {:?}", span, node.chars().unwrap())
                    }
                    NodeKind::Comment { .. } => format!(
                        "comment {} start={:?} content={:?} post={:?}",
                        span,
                        node.comment_start().unwrap(),
                        node.comment().unwrap(),
                        node.comment_post_space().unwrap()
                    ),
                    NodeKind::Group(_) => format!("group {}", span),
                    NodeKind::Callable(_) => format!("callable {}", span),
                    NodeKind::List { .. } => format!("list {}", span),
                }
            })
            .collect()
    }

    /// The partition invariant (§3.5, invariant 5): the root's children tile `interior`
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
    fn paragraph_break_node_comes_from_the_lang_hook() {
        #[derive(Debug, Clone, Copy)]
        struct MarkLang;
        impl Lang for MarkLang {
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ();

            fn make_paragraph_break_node(
                _state: &ParsingState<Self>,
                _token: &Token<'_, Self>,
            ) -> NodeKind<Self> {
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
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(3, 8) });
        assert_eq!(parsed.pos, 3);

        // Re-peeking from the seam yields the stop token itself, with empty pre-space —
        // no byte is represented twice.
        let mut reader = StdTokenReader::new(content);
        reader.move_to_pos(parsed.pos);
        let token: Token<'_, TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        assert!(matches!(token.kind, TokenKind::Command { name: "end", .. }));
        assert!(token.pre_space.is_empty());
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
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(1, 5) });
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
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(2, 3) });
        assert_eq!(parsed.pos, 2);
    }

    #[test]
    fn stop_at_group_close_produced_by_the_expected_close() {
        // `$` closes only through `expecting_group_close` (ambiguous delimiter read as
        // an opener otherwise) — the 6.3 group parser's configuration, exercised here.
        let mut r = rules::<TestLang>();
        r.groups.push(math_rule());
        r.expecting_group_close = Some(math_rule());
        let st = state_with(r);
        let parsed = run_both(
            "a b$c",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_MATH, close: "$" }, false),
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_MATH, close: "$" }, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"a b\""]);
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(3, 4) });
        assert_eq!(parsed.pos, 3);
    }

    #[test]
    fn group_close_of_a_different_delimiter_is_unexpected() {
        // The stop condition waits for the math `$` close; a `}` arrives: the delimiter
        // does not match — reported as data, token unconsumed, no diagnostic (the caller
        // decides).
        let mut r = rules::<TestLang>();
        r.groups.push(math_rule());
        r.expecting_group_close = Some(math_rule());
        let st = state_with(r);
        let parsed = run_both(
            "ab}c",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_MATH, close: "$" }, false),
            StopSpec::at_token(TokenStopKind::GroupClose { group_type: GT_MATH, close: "$" }, false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(parsed.stop, StopCause::UnexpectedGroupClose { span: Span::new(2, 3) });
        assert_eq!(parsed.pos, 2);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn group_close_of_a_shared_class_but_different_delimiter_is_unexpected() {
        // `[`/`]` and `{`/`}` share the class GT_BRACE. A group opened with `{` must not
        // be closed by a `]`: same class, different delimiter — the `close` field
        // disambiguates within a class.
        let mut r = rules::<TestLang>();
        r.groups.push(Arc::new(GroupRule {
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
        assert_eq!(parsed.stop, StopCause::UnexpectedGroupClose { span: Span::new(2, 3) });
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
        r.expecting_group_close = Some(Arc::new(GroupRule {
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
        assert_eq!(parsed.stop, StopCause::UnexpectedGroupClose { span: Span::new(2, 3) });
        assert_eq!(parsed.pos, 2);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn group_close_without_any_stop_condition_is_unexpected() {
        let st = state();
        let parsed =
            run_both("ab}c", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["chars 0..2 \"ab\""]);
        assert_eq!(parsed.stop, StopCause::UnexpectedGroupClose { span: Span::new(2, 3) });
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
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(2, 4) });
        assert_eq!(parsed.pos, 2);
    }

    #[test]
    fn stop_at_a_token_predicate() {
        let st = state();
        let predicate = |t: &Token<'_, TestLang>| matches!(t.kind, TokenKind::Comment { .. });
        let parsed = run_both(
            "ab %c\nd",
            &st,
            Recovery::Strict,
            StopSpec::at_token(TokenStopKind::Predicate(&predicate), false),
            StopSpec::at_token(TokenStopKind::Predicate(&predicate), false),
        );
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"ab \""]);
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(3, 6) });
        assert_eq!(parsed.pos, 3);
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
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(3, 8) });
        assert_eq!(parsed.pos, 8);

        // The next read is the following content, its pre-space empty (nothing re-read).
        let mut reader = StdTokenReader::new(content);
        reader.move_to_pos(parsed.pos);
        let token: Token<'_, TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        assert!(matches!(token.kind, TokenKind::Char('r')));
        assert!(token.pre_space.is_empty());
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
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(2, 3) });
        // The close is consumed (reader past `}`); `}` carries no post-space, so the
        // following space is not the close's — it stays for the enclosing content as the
        // next token's pre-space, unclaimed here.
        assert_eq!(parsed.pos, 3);
    }

    #[test]
    fn node_condition_stops_after_a_flushed_chars_node() {
        let st = state();
        let mut c1 = |count: usize, _: StagedNodeView<'_, TestLang>| count >= 1;
        let mut c2 = |count: usize, _: StagedNodeView<'_, TestLang>| count >= 1;
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
            count >= 1 && matches!(view.kind(), NodeKind::Comment { .. })
        };
        let mut c2 = |count: usize, view: StagedNodeView<'_, TestLang>| {
            count >= 1 && matches!(view.kind(), NodeKind::Comment { .. })
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
        let mut c1 = |count: usize, _: StagedNodeView<'_, TestLang>| count >= 1;
        let mut c2 = |count: usize, _: StagedNodeView<'_, TestLang>| count >= 1;
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
            true
        };
        let mut c2 = |_: usize, _: StagedNodeView<'_, TestLang>| {
            calls_list += 1;
            true
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
        assert_eq!(parsed.stop, StopCause::TokenCondition { span: Span::new(3, 8) });
        assert_eq!(parsed.pos, 8);
        assert_eq!(calls_std, 0, "the token-stop flush consulted the node condition");
        assert_eq!(calls_list, 0, "the token-stop flush consulted the node condition");
    }

    // --- tokenizer-error recovery (std reader only: the list reader cannot fail) ---------

    #[test]
    fn forbidden_char_tolerant_adopts_the_recovery_token() {
        let mut r = rules::<TestLang>();
        r.forbidden_chars = "#".into();
        let st = state_with(r);
        let mut reader = StdTokenReader::new("ab#cd");
        let parsed =
            try_run("ab#cd", &mut reader, &st, Recovery::Tolerant, StopSpec::none()).unwrap();
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
        r.forbidden_chars = "#".into();
        let st = state_with(r);
        let mut reader = StdTokenReader::new("ab#cd");
        let err =
            try_run("ab#cd", &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap_err();
        // The token error was lifted into the structured condition (§3.8): compare
        // identifier and downcast fields (no PartialEq on the carriers).
        assert_eq!(err.identifier(), crate::token::ForbiddenChar::IDENTIFIER);
        assert_eq!(
            err.data().downcast_ref::<crate::token::ForbiddenChar>().map(|c| c.ch),
            Some('#')
        );
        assert_eq!(err.span().range(), 2..3);
    }

    /// A token source that violates the `TokenRecovery::resume_pos` advancement
    /// contract: every `peek` reports a recoverable forbidden-char error whose
    /// `resume_pos` is the current position, so adopting the recovery re-reads the
    /// same error.
    struct StuckRecoveryReader {
        pos: usize,
    }

    impl<'s> TokenReader<'s, TestLang> for StuckRecoveryReader {
        fn peek(
            &mut self,
            _state: &Arc<ParsingState<TestLang>>,
        ) -> TokenResult<'s, TestLang, Token<'s, TestLang>> {
            let span = Span::new(self.pos, self.pos + 1);
            Err(TokenError::new(
                TokenErrorKind::ForbiddenChar(crate::token::ForbiddenChar::new('#')),
                span,
                Some(TokenRecovery {
                    token: Token::new(TokenKind::Char('#'), span, Span::empty(self.pos)),
                    resume_pos: self.pos, // the violation: does not advance
                }),
            ))
        }

        fn move_past(&mut self, tok: &Token<'s, TestLang>, _skip_post_space: bool) {
            self.pos = tok.span.end();
        }

        fn move_to(&mut self, tok: &Token<'s, TestLang>, _rewind_pre_space: bool) {
            self.pos = tok.span.start();
        }

        fn move_to_pos(&mut self, pos: usize) {
            self.pos = pos;
        }

        fn pos(&self) -> usize {
            self.pos
        }
    }

    #[test]
    fn non_advancing_token_recovery_aborts_instead_of_spinning() {
        // Without the advancement guard this parse never terminates in tolerant mode
        // (the recovery arm consumes no token), pushing one diagnostic per iteration.
        let st = state();
        let mut reader = StuckRecoveryReader { pos: 0 };
        let err = try_run("ab", &mut reader, &st, Recovery::Tolerant, StopSpec::none())
            .expect_err("a non-advancing resume position must abort the parse");
        assert_eq!(err.identifier(), crate::token::ForbiddenChar::IDENTIFIER);
        assert_eq!(
            err.data().downcast_ref::<crate::token::ForbiddenChar>().map(|c| c.ch),
            Some('#')
        );
        assert_eq!(err.span().range(), 0..1);
    }

    // --- TokenErrorKind::Custom (§3.8: one extension mechanism serves both layers) --------

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
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn scan_specials<'s>(
            _state: &ParsingState<Self>,
            content: &'s str,
            pos: usize,
        ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
            if content[pos..].starts_with('!') {
                let span = Span::new(pos, pos + 1);
                Err(TokenError::new(
                    TokenErrorKind::Custom(Box::new(TabooChar { ch: '!' })),
                    span,
                    Some(TokenRecovery {
                        token: Token::new(TokenKind::Char('!'), span, Span::empty(pos)),
                        resume_pos: pos + 1,
                    }),
                ))
            } else {
                Ok(None)
            }
        }

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("!".into())
        }
    }

    #[test]
    fn custom_token_condition_flows_through_the_lift_unwrapped() {
        let st = state_with(rules::<TabooLang>());

        // Tolerant: a diagnostic with the language's own identifier; the placeholder
        // char joins the content run.
        let mut reader = StdTokenReader::new("a!b");
        let parsed =
            try_run("a!b", &mut reader, &st, Recovery::Tolerant, StopSpec::none()).unwrap();
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"a!b\""]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), TabooChar::IDENTIFIER);
        // The downcast reaches the payload directly: the lift unwraps `Custom` — never
        // double-boxed.
        assert_eq!(
            diagnostic.data().downcast_ref::<TabooChar>(),
            Some(&TabooChar { ch: '!' })
        );

        // Strict: the same condition rides the ParseError.
        let mut reader = StdTokenReader::new("a!b");
        let err = try_run("a!b", &mut reader, &st, Recovery::Strict, StopSpec::none())
            .unwrap_err();
        assert_eq!(err.identifier(), TabooChar::IDENTIFIER);
        assert!(err.data().downcast_ref::<TabooChar>().is_some());
    }

    #[test]
    fn escape_at_end_of_input_tolerant_recovers_and_repositions() {
        let st = state();
        let mut reader = StdTokenReader::new("ab \\");
        let parsed =
            try_run("ab \\", &mut reader, &st, Recovery::Tolerant, StopSpec::none()).unwrap();
        // The placeholder is a Char covering the dangling escape byte: it joins the
        // pending chars run, so the recovery is partition-clean (§3.8) — the escape
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
        let mut reader = StdTokenReader::new("ab \\");
        let err =
            try_run("ab \\", &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap_err();
        assert_eq!(err.identifier(), crate::token::EndOfStreamAfterEscape::IDENTIFIER);
        assert_eq!(
            err.data()
                .downcast_ref::<crate::token::EndOfStreamAfterEscape>()
                .map(|c| c.escape_char),
            Some('\\')
        );
    }

    // --- unresolvable-command recovery (§3.8) ---------------------------------------------

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
        // `TestLang` keeps the default `resolve_command`: the condition records the
        // unimplemented resolver and the message carries the hint.
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.message(),
            "cannot resolve command ‘\\foo’ (this language implements no \
             ‘Lang::resolve_command’ — the default resolves nothing; implement the hook \
             or use a preset)"
        );
        assert!(
            diagnostic
                .data()
                .downcast_ref::<UnresolvableCommand>()
                .unwrap()
                .resolver_unimplemented
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
        let mut reader = StdTokenReader::new("a \\foo  b");
        let err = try_run("a \\foo  b", &mut reader, &st, Recovery::Strict, StopSpec::none())
            .unwrap_err();
        // Exact hint wording pinned in unresolved_command_recovers_as_a_chars_fallback.
        let message = err.to_string();
        assert!(message.starts_with("cannot resolve command ‘\\foo’"));
        assert!(message.contains("implements no ‘Lang::resolve_command’"));
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
        // `CmdLang` implements the hook: a library miss is `Unknown` — the message
        // stays bare, no unimplemented-resolver hint.
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.message(), "cannot resolve command ‘\\foo’");
        assert!(
            !diagnostic
                .data()
                .downcast_ref::<UnresolvableCommand>()
                .unwrap()
                .resolver_unimplemented
        );
        assert_partition(&parsed.result, 0..8);
    }

    // --- Lang::refine_diagnostic (§3.8) -----------------------------------------------------

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
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn refine_diagnostic(
            data: alloc::boxed::Box<dyn crate::error::DiagnosticData>,
            _state: &ParsingState<Self>,
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
        let mut reader = StdTokenReader::new("a \\foo b");
        let err = try_run("a \\foo b", &mut reader, &st, Recovery::Strict, StopSpec::none())
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
        assert_eq!(node.post_space(), Some(" "));
        let data = node.callable().unwrap();
        assert!(data.arguments.is_empty() && data.slots.is_empty());
        // The node records the library's spec (the flyweight Arc).
        let expected = st
            .libraries()
            .lookup(
                &CallableQuery::new(
                    CT_MACRO,
                    "foo",
                    CallableSyntax::Command { escape_char: '\\' },
                ),
                &st,
            )
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
        let node = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(node.post_space(), Some(" "));
        assert_partition(&parsed.result, 0..8);

        // A single non-name-char command (`\&`) takes no post-space at the token level,
        // and the invocation claims nothing beyond the token: the space is sibling
        // content (TeX/pylatexenc parity).
        let parsed =
            run_both("\\& b", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        assert_eq!(shapes(&parsed.result), ["callable 0..2", "chars 2..4 \" b\""]);
        let node = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(node.post_space(), Some(""));
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
        assert_eq!(parsed.result.tree.root().child(0).unwrap().post_space(), Some(" "));
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
        assert_eq!(node.post_space(), Some("")); // a specials token has no post-space
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
        impl CallableSpec<CmdLang> for DefSpec {
            fn make_invocation_parser<'a, 's>(
                &'a self,
                invocation: Invocation<'a, 's, CmdLang>,
            ) -> Box<dyn ConstructParser<CmdLang, Output = BuildId> + 'a>
            where
                's: 'a,
            {
                Box::new(DefParser { inner: StdInvocationParser::new(invocation) })
            }
        }

        struct DefParser<'a, 's> {
            inner: StdInvocationParser<'a, 's, CmdLang>,
        }

        impl ConstructParser<CmdLang> for DefParser<'_, '_> {
            type Output = BuildId;

            fn parse(
                &mut self,
                cx: &mut ParseContext<'_, '_, CmdLang>,
            ) -> ConstructParserResult<
                CmdLang,
                (BuildId, Option<ParsingStateDelta<CmdLang>>),
            > {
                let (id, _) = self.inner.parse(cx)?;
                let delta =
                    ParsingStateDelta::new().push_library(macro_library(&["late"]));
                Ok((id, Some(delta)))
            }
        }

        let mut lib: Library<CmdLang> = Library::new("base");
        lib.insert(CT_MACRO, "def", Arc::new(DefSpec));
        let mut libraries = LibraryStack::new();
        libraries.push(Arc::new(lib));
        let st: Arc<ParsingState<CmdLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), libraries, ext: () }));

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
        // The full-takeover escape hatch (§3.6, the C6 obligation): an overridden
        // factory returns a custom parser that consumes tokens up to a `!` marker —
        // markup included, no group descent — stages its own node shape (an untyped
        // group holding the raw chars), and returns a tokenization-affecting delta
        // (comments disabled) as its after-effect.
        #[derive(Debug)]
        struct TakeSpec;
        impl CallableSpec<CmdLang> for TakeSpec {
            fn make_invocation_parser<'a, 's>(
                &'a self,
                invocation: Invocation<'a, 's, CmdLang>,
            ) -> Box<dyn ConstructParser<CmdLang, Output = BuildId> + 'a>
            where
                's: 'a,
            {
                Box::new(TakeParser { invocation })
            }
        }

        struct TakeParser<'a, 's> {
            invocation: Invocation<'a, 's, CmdLang>,
        }

        impl ConstructParser<CmdLang> for TakeParser<'_, '_> {
            type Output = BuildId;

            fn parse(
                &mut self,
                cx: &mut ParseContext<'_, '_, CmdLang>,
            ) -> ConstructParserResult<
                CmdLang,
                (BuildId, Option<ParsingStateDelta<CmdLang>>),
            > {
                // The trigger is already consumed whole; its span becomes the "open
                // delimiter". Raw content runs from there to the `!` marker.
                let open_span = self.invocation.token.span;
                let mut content = Span::empty(open_span.end());
                let close_span = loop {
                    let token = cx.tokens.peek(&cx.state).expect("clean test content");
                    cx.tokens.move_past(&token, true);
                    match token.kind {
                        TokenKind::Char('!') => break token.span,
                        TokenKind::EndOfStream => panic!("test content has a marker"),
                        _ => content.extend_to(token.span.end()),
                    }
                };
                let child = cx.session.builder.add(
                    NodeKind::chars(content),
                    SourceSpan::new(&cx.source, content),
                    Arc::clone(&cx.state),
                    vec![],
                ).unwrap();
                let data: GroupData<CmdLang> = GroupData::untyped(
                    TextContent::Spanned(open_span),
                    TextContent::Spanned(close_span),
                );
                let id = cx.session.builder.add(
                    NodeKind::group(data),
                    SourceSpan::new(&cx.source, open_span.start()..close_span.end()),
                    Arc::clone(&cx.state),
                    vec![child],
                ).unwrap();
                let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
                    enable_comments: Some(false),
                    ..TokenRulesOverrides::default()
                });
                Ok((id, Some(delta)))
            }
        }

        let mut lib: Library<CmdLang> = Library::new("base");
        lib.insert(CT_MACRO, "take", Arc::new(TakeSpec));
        let mut libraries = LibraryStack::new();
        libraries.push(Arc::new(lib));
        let st: Arc<ParsingState<CmdLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), libraries, ext: () }));

        // `a{b` would normally open a group (and diagnose it unclosed); the takeover
        // parser consumes it raw. ` %c` would normally be a comment; the returned
        // delta disables comments, so it parses as plain chars. StdTokenReader only:
        // a pre-scanned token list can neither re-serve raw bytes nor re-tokenize
        // under mid-parse rule changes (TokenListReader's documented fidelity limit).
        let content = "\\take a{b! %c";
        let mut reader = StdTokenReader::new(content);
        let parsed =
            try_run(content, &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap();
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
    fn finalize_node_populates_callable_ext_through_the_dispatch_loop() {
        // FLM pattern rehearsal (§3.6): the builder-run hook sees every staged
        // `Callable`, downcasts `data.spec` to the concrete spec type (the `Any`
        // supertrait contract, Action-05), and attaches derived data as its per-kind
        // ext. A preset dispatching on an *open* set of spec types funnels them
        // through one concrete wrapper first (DESIGN_RATIONALE.md §3.4).
        struct ExtBundle;
        impl NodeExtTypes for ExtBundle {
            type NodeExt = ();
            type CharsNodeExt = ();
            type GroupNodeExt = ();
            type CallableNodeExt = Box<str>;
            type CommentNodeExt = ();
            type ListNodeExt = ();
            type ArgumentExt = ();
            type SlotExt = ();
        }

        #[derive(Debug, Clone, Copy)]
        struct ExtLang;
        impl Lang for ExtLang {
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ExtBundle;

            fn resolve_command(
                state: &ParsingState<Self>,
                token: &Token<'_, Self>,
            ) -> CommandResolution<Self> {
                resolve_macro_via_libraries(state, token)
            }

            fn finalize_node(
                kind: &mut NodeKind<Self>,
                _ext: &mut NodeExt<Self>,
                _span: &SourceSpan<Self::SourceOrigin>,
                _parsing_state: &Arc<ParsingState<Self>>,
                _children: &[BuildId],
                _staged: &StagedNodes<'_, Self>,
            ) {
                if let NodeKind::Callable(data) = kind {
                    // The downcast: trait-upcast the stored spec to `&dyn Any`, then
                    // recover the concrete type — field access included.
                    let spec = (&*data.spec as &dyn core::any::Any)
                        .downcast_ref::<StdCallableSpec<ExtLang>>()
                        .expect("the test library registers StdCallableSpec");
                    // Idempotent: recomputing from the same facts yields the same ext.
                    data.ext = format!("{}#{}", data.name, spec.arguments.len())
                        .into_boxed_str();
                }
            }
        }

        let mut libraries = LibraryStack::new();
        libraries.push(macro_library::<ExtLang>(&["foo"]));
        let st: Arc<ParsingState<ExtLang>> =
            Arc::new(ParsingState::new(StateData { rules: rules(), libraries, ext: () }));

        let parsed =
            run_both("\\foo x", &st, Recovery::Strict, StopSpec::none(), StopSpec::none());
        let node = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(&*node.callable().unwrap().ext, "foo#0");
    }

    #[test]
    fn invocation_child_state_policy_fixed_and_compute() {
        let st = state_with_macros(&["foo"]);
        let other = Arc::new(st.derived(&ParsingStateDelta::new()));

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
                       invocation: &Invocation<'_, '_, CmdLang>|
         -> Arc<ParsingState<CmdLang>> {
            assert_eq!(invocation.name, "foo");
            assert_eq!(invocation.callable_type, CT_MACRO);
            Arc::clone(state)
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
        r.groups.push(math_rule());
        let st = state_with(r);
        let mut reader = StdTokenReader::new("a $x$ b");
        let parsed =
            try_run("a $x$ b", &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap();
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
            count >= 1 && matches!(view.kind(), NodeKind::Group(_))
        };
        let mut c2 = |count: usize, view: StagedNodeView<'_, TestLang>| {
            count >= 1 && matches!(view.kind(), NodeKind::Group(_))
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
        // Conditions are tested only at the parser's own nesting level (§3.6): the
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
        let mut reader = StdTokenReader::new("a {bc");
        let err = try_run("a {bc", &mut reader, &st, Recovery::Strict, StopSpec::none())
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
        // (unwinding — every level consumes or unwinds, §3.6), and the stray close then
        // surfaces at this level, still unconsumed, for the root driver to adjudicate.
        let mut r = rules::<TestLang>();
        r.groups.push(Arc::new(GroupRule {
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
        assert_eq!(parsed.stop, StopCause::UnexpectedGroupClose { span: Span::new(2, 3) });
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
        r.groups.push(Arc::new(GroupRule {
            group_type: GT_BRACE,
            open: "[".into(),
            close: "]".into(),
        }));
        let st = state_with(r);
        let mut reader = StdTokenReader::new("{a]}");
        let err =
            try_run("{a]}", &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap_err();
        assert_eq!(err.to_string(), "mismatched group close: expected ‘}’");
        assert_eq!(err.span().range(), 2..3);
    }

    #[test]
    fn root_driver_skips_a_stray_close_and_continues() {
        // The root always consumes (§3.6): `UnexpectedGroupClose` is data, and the
        // *root driver* — here the test, later a `Language::parse` entry (Phase 7) —
        // diagnoses, skips the token, and resumes. The skipped byte is dropped from the
        // tree: an accepted tolerant byte-accounting break, so this is the one tree the
        // invariant checker is deliberately not applied to.
        //
        // The condition is the driver's own, third-party style (§3.8): a root driver
        // defines its diagnoses like any downstream language would.
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
        let mut reader = StdTokenReader::new(content);
        let mut session: ParserSession<TestLang> = ParserSession::new(Recovery::Tolerant);
        let mut nodes = Vec::new();
        let stop = loop {
            let mut cx = ParseContext::new(&mut reader, Arc::clone(&source), Arc::clone(&st), &mut session);
            let mut parser = NodesParser::new(StopSpec::none());
            let (outcome, _) = parser.parse(&mut cx).unwrap();
            nodes.extend(outcome.nodes);
            match outcome.stop {
                StopCause::UnexpectedGroupClose { span } => {
                    session
                        .recover(
                            Box::new(StrayGroupClose { delim: "}".into() }),
                            SourceSpan::new(&source, span),
                        )
                        .unwrap();
                    let stray: Token<'_, TestLang> =
                        TokenReader::peek(&mut reader, &st).unwrap();
                    reader.move_past(&stray, true);
                }
                other => break other,
            }
        };
        assert_eq!(stop, StopCause::EndOfInput);
        let root = session.builder.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            Arc::clone(&st),
            nodes,
        ).unwrap();
        let result = session.finish(root).unwrap();
        let texts: Vec<_> = result
            .tree
            .root()
            .children()
            .map(|c| c.chars().unwrap().to_string())
            .collect();
        assert_eq!(texts, ["a", "b"]);
        assert_eq!(result.diagnostics.len(), 1);
    }

    // --- descent-state policy + the session seam (6.3) --------------------------------------

    #[test]
    fn child_state_fixed_reverts_group_interiors_to_the_outer_state() {
        // The chars-except-groups motivating case (§3.6): a restricted outer state
        // (comments disabled) whose group interiors revert to the full state. Std
        // reader only: a pre-scanned list cannot see the interior reclassification.
        let full = state();
        let restricted = Arc::new(full.derived(&ParsingStateDelta::new().rules(
            TokenRulesOverrides { enable_comments: Some(false), ..Default::default() },
        )));
        let content = "%x{%y\n}z";

        // Under the restricted state alone, `%` is ordinary content everywhere.
        let mut reader = StdTokenReader::new(content);
        let plain =
            try_run(content, &mut reader, &restricted, Recovery::Strict, StopSpec::none())
                .unwrap();
        let group = plain.result.tree.root().child(1).unwrap();
        assert!(group.children().all(|c| c.is_chars()));

        // With `group: Fixed(full)` the interior reverts: `%y` is a comment again.
        let policy = ChildStateSpec {
            group: GroupChildState::Fixed(Arc::clone(&full)),
            invocation: InvocationChildState::Inherit,
        };
        let mut reader = StdTokenReader::new(content);
        let parsed = try_run_with(
            content,
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
        assert_eq!(group.child(0).unwrap().comment(), Some("y"));
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
        r.groups.push(Arc::new(GroupRule {
            group_type: GT_OPT,
            open: "[".into(),
            close: "]".into(),
        }));
        let full = state_with(r);
        let no_comments = Arc::new(full.derived(&ParsingStateDelta::new().rules(
            TokenRulesOverrides { enable_comments: Some(false), ..Default::default() },
        )));
        let compute = |state: &Arc<ParsingState<TestLang>>, token: &Token<'_, TestLang>| {
            match &token.kind {
                TokenKind::GroupOpen { rule, .. } if rule.group_type == GT_OPT => {
                    Arc::clone(&no_comments)
                }
                _ => Arc::clone(state), // pass-through preserves pointer identity
            }
        };
        let policy = ChildStateSpec {
            group: GroupChildState::Compute(&compute),
            invocation: InvocationChildState::Inherit,
        };

        let content = "{%a\n}[%b\n]";
        let mut reader = StdTokenReader::new(content);
        let parsed = try_run_with(
            content,
            &mut reader,
            &full,
            Recovery::Strict,
            StopSpec::none(),
            policy,
        )
        .unwrap();
        let braces = parsed.result.tree.root().child(0).unwrap();
        assert_eq!(braces.child_count(), 1);
        assert_eq!(braces.child(0).unwrap().comment(), Some("a"));
        assert!(Arc::ptr_eq(braces.parsing_state(), &full)); // pass-through identity
        let brackets = parsed.result.tree.root().child(1).unwrap();
        assert_eq!(brackets.group_type(), Some(GT_OPT));
        assert_eq!(brackets.child_count(), 1);
        assert_eq!(brackets.child(0).unwrap().chars(), Some("%b\n"));
        assert!(Arc::ptr_eq(brackets.parsing_state(), &no_comments));
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
                .expecting_group_close
                .as_ref()
                .map(|rule| rule.close.as_str()),
            Some("}")
        );
    }

    #[test]
    fn observe_transition_fires_per_descent_finalize_once_per_derivation() {
        // The two-level transition doctrine (§3.6): `{a}{b}` makes two group descents —
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
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type StateExt = ();
            type Event = ();
            type SessionExt = Counts;
            type SourceOrigin = Option<String>;
            type NodeExts = ();

            fn finalize_transition(
                _new: &mut StateData<Self>,
                _prev: &ParsingState<Self>,
                _events: &[()],
            ) {
                FINALIZE_RUNS.fetch_add(1, Ordering::Relaxed);
            }

            fn observe_transition(
                ext: &mut Counts,
                _prev: &ParsingState<Self>,
                _new: &ParsingState<Self>,
                _delta: &ParsingStateDelta<Self>,
            ) {
                ext.observed += 1;
            }
        }

        // Driven manually so the session stays inspectable after the parse.
        let content = "{a}{b}";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let st = state_with(rules::<CountLang>());
        let mut reader = StdTokenReader::new(content);
        let mut session: ParserSession<CountLang> = ParserSession::new(Recovery::Strict);
        let mut cx = ParseContext::new(&mut reader, Arc::clone(&source), Arc::clone(&st), &mut session);
        let mut parser = NodesParser::new(StopSpec::none());
        let (outcome, _) = parser.parse(&mut cx).unwrap();
        assert!(matches!(outcome.stop, StopCause::EndOfInput));
        assert_eq!(session.ext.observed, 2);
        assert_eq!(FINALIZE_RUNS.load(Ordering::Relaxed), 1);
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
        assert_eq!(group.child(1).unwrap().comment(), Some("e"));
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
}
