//! [`NodesParser`]: the main content dispatch loop (ARCHITECTURE.md §constructs), with
//! its stop machinery ([`StopSpec`], [`TokenStopCondition`], [`StopCause`]).
//!
//! The parser peeks one token at a time and dispatches on its kind — never on parser
//! registries (§2.6). This subphase (6.2) implements the content arms: chars
//! accumulation, paragraph breaks (via [`Lang::make_paragraph_break_node`]), comments,
//! and end of stream. The `GroupOpen` (6.3) and `Command`/`Specials` invocation arms
//! (6.4) currently take a minimal tolerant recovery — diagnostic plus a span-backed
//! chars fallback node — until their parsers land; sibling state-deltas returned by
//! invocation parsers will be applied inside the loop (`state.derived(&delta)`) once the
//! 6.4 arms produce them.
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
//! is never re-read); parse-level conditions (unresolvable command, and — until their
//! parsers land — groups and specials) recover as a diagnostic plus a chars fallback
//! node over the token's span. Markup text inside a `Chars` node is an accepted
//! tolerant-recovery artifact, always accompanied by a diagnostic; fallback nodes are
//! deliberately *not* merged into neighboring chars runs. `Err` means abort — nobody
//! continues past one.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::mem;

use crate::error::{ParseError, ParseErrorKind};
use crate::node::{BuildId, NodeKind, StagedNodeView};
use crate::source::{Source, SourceSpan, Span};
use crate::state::{Lang, ParsingState, ParsingStateDelta};
use crate::token::{Token, TokenKind, TokenReader};

use super::{ConstructParser, ConstructParserResult, ParseContext};

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
    source: Arc<Source<L::SourceOrigin>>,
    stop: StopSpec<'p, L>,
    nodes: Vec<BuildId>,
    /// The pending maximal chars run (invariant 1): extended by `Char` tokens and every
    /// token's pre-space, flushed when a non-`Char` construct starts.
    run: Option<Span>,
}

impl<'p, L: Lang> NodesParser<'p, L> {
    /// A parser staging nodes whose spans refer into `source` (the source the context's
    /// token reader is reading), stopping per `stop`.
    pub fn new(source: Arc<Source<L::SourceOrigin>>, stop: StopSpec<'p, L>) -> NodesParser<'p, L> {
        NodesParser { source, stop, nodes: Vec::new(), run: None }
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
                    run.end == pre_space.start,
                    "pre-space {:?} is not contiguous with the pending run {:?}",
                    pre_space,
                    run
                );
                run.end = pre_space.end;
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
                    run.end == token.span.start,
                    "char token {:?} is not contiguous with the pending run {:?}",
                    token.span,
                    run
                );
                run.end = token.span.end;
            }
            None => self.run = Some(token.span),
        }
    }

    /// Flush the pending run as a `Chars` node (span-backed over the exact run slice).
    /// Returns whether the node stop condition fired on it.
    fn flush(&mut self, cx: &mut ParseContext<'_, '_, L>) -> bool {
        match self.run.take() {
            Some(run) => self.stage_node(cx, NodeKind::chars(run), run),
            None => false,
        }
    }

    /// Extend the pending run with `pre_space`, then flush it: the path taken when a
    /// non-`Char` construct starts (invariant 1). A matched *stop* token's flush goes
    /// through [`flush_for_token_stop`](Self::flush_for_token_stop) instead — same
    /// staging, no node-condition test.
    fn flush_through(&mut self, cx: &mut ParseContext<'_, '_, L>, pre_space: Span) -> bool {
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
    fn flush_for_token_stop(&mut self, cx: &mut ParseContext<'_, '_, L>, pre_space: Span) {
        self.take_pre_space(pre_space);
        if let Some(run) = self.run.take() {
            self.stage(cx, NodeKind::chars(run), run);
        }
    }

    /// Stage a childless node under the current state and record it as a sibling —
    /// without testing the node stop condition (that is [`stage_node`](Self::stage_node)'s
    /// job; the token-stop flush stages through this directly).
    fn stage(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        kind: NodeKind<L>,
        span: Span,
    ) -> BuildId {
        let id = cx.session.builder.add(
            kind,
            SourceSpan::new(&self.source, span.range()),
            Arc::clone(&cx.state),
            vec![],
        );
        self.nodes.push(id);
        id
    }

    /// Stage a childless node ([`stage`](Self::stage)) and test the node stop condition
    /// on it.
    fn stage_node(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
        kind: NodeKind<L>,
        span: Span,
    ) -> bool {
        let id = self.stage(cx, kind, span);
        match &mut self.stop.node {
            Some(condition) => {
                let staged = cx.session.builder.staged_nodes();
                let view = staged.get(id).expect("the node was just staged");
                condition(self.nodes.len(), view)
            }
            None => false,
        }
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
    /// resolution dispatch lands in 6.4, `Specials` likewise, `GroupOpen` until group
    /// parsing lands in 6.3) — and the decided unresolvable-command recovery (§3.8):
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
        message: String,
    ) -> ConstructParserResult<L, bool> {
        if self.flush_through(cx, token.pre_space) {
            if !recovered {
                cx.tokens.move_to(token, false);
            }
            return Ok(true);
        }
        cx.recover(
            ParseErrorKind::Syntax { message },
            SourceSpan::new(&self.source, token.span.range()),
        )?;
        if !recovered {
            cx.tokens.move_past(token, true);
        }
        Ok(self.stage_node(cx, NodeKind::chars(token.span), token.span))
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
                    let kind = ParseErrorKind::Token(error.kind());
                    let span = SourceSpan::new(&self.source, error.span().range());
                    match error.into_recovery() {
                        None => return Err(ParseError::new(kind, span)),
                        Some(recovery) => {
                            cx.recover(kind, span)?;
                            resume_at(cx.tokens, recovery.resume_pos);
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
                    self.flush_for_token_stop(cx, token.pre_space);
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
                    let fired = self.flush_through(cx, token.pre_space);
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
                    let fired = self.flush_through(cx, token.pre_space);
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
                    if self.flush_through(cx, token.pre_space) {
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
                    if self.stage_node(cx, kind, token.span) {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                }

                TokenKind::Comment { content, post_space } => {
                    if self.flush_through(cx, token.pre_space) {
                        if !recovered {
                            cx.tokens.move_to(&token, false);
                        }
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                    // The token's span is start delimiter + content + post-space; the
                    // content's length and end position pin down the three sub-spans.
                    let content_span =
                        Span::new(post_space.start - content.len(), post_space.start);
                    let start_span = Span::new(token.span.start, content_span.start);
                    let kind = NodeKind::comment(start_span, content_span, *post_space);
                    if !recovered {
                        cx.tokens.move_past(&token, true);
                    }
                    if self.stage_node(cx, kind, token.span) {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                }

                TokenKind::Command { name, escape_char, .. } => {
                    // Invocation dispatch (`Lang::resolve_command` →
                    // `make_invocation_parser`) lands in 6.4; until then every command
                    // takes the unresolvable-command recovery (§3.8): diagnostic plus
                    // span-backed chars fallback.
                    let message = format!("cannot resolve command ‘{}{}’", escape_char, name);
                    if self.recover_as_chars(cx, &token, recovered, message)? {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                }

                TokenKind::Specials { name, .. } => {
                    // Invocation dispatch lands in 6.4 (the spec already rides on the
                    // token); until then, same recovery as unresolved commands.
                    let message = format!("cannot invoke specials ‘{}’ here", name);
                    if self.recover_as_chars(cx, &token, recovered, message)? {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                }

                TokenKind::GroupOpen { delim, .. } => {
                    // Group parsing lands in 6.3; until then, diagnostic plus chars
                    // fallback over the delimiter.
                    let message = format!("cannot parse group opened by ‘{}’ here", delim);
                    if self.recover_as_chars(cx, &token, recovered, message)? {
                        return Ok((self.outcome(StopCause::NodeCondition), None));
                    }
                }
            }

            // (Sibling-delta application — `cx.state = Arc::new(cx.state.derived(&d))`,
            // §2 state-threading convention — slots in here once the invocation arms
            // (6.4) return deltas; no 6.2 arm produces one.)
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

/// Reposition the reader to an absolute byte position (a `TokenRecovery::resume_pos`).
///
/// [`TokenReader`] expresses positioning through tokens, so "go to `pos`" is phrased as
/// moving to a zero-width marker token at `pos` — `move_to(tok, false)` means "position
/// = `tok.span.start`" for any reader honoring the span conventions.
fn resume_at<'s, L: Lang>(tokens: &mut dyn TokenReader<'s, L>, pos: usize) {
    let marker: Token<'s, L> =
        Token::new(TokenKind::EndOfStream, Span::empty(pos), Span::empty(pos));
    tokens.move_to(&marker, false);
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
    use crate::library::LibraryStack;
    use crate::source::TextContent;
    use crate::spec::StdCallableSpec;
    use crate::state::{SimpleLang, StateData};
    use crate::token::{
        CommandRule, CommentRule, GroupRule, SpecialsMatch, StdTokenReader, TokenErrorKind,
        TokenListReader, TokenResult, TokenRules, TriggerChars, WhitespaceRules,
    };
    use alloc::string::ToString;

    const GT_BRACE: u32 = 0;
    const GT_MATH: u32 = 1;

    #[derive(Debug, Clone, Copy)]
    struct TestLang;
    impl SimpleLang for TestLang {}

    fn math_rule<L: Lang<GroupTypeId = u32>>() -> Arc<GroupRule<L>> {
        Arc::new(GroupRule { group_type: GT_MATH, open: "$".into(), close: "$".into() })
    }

    fn rules<L: Lang<GroupTypeId = u32>>() -> TokenRules<L> {
        TokenRules {
            whitespace: Some(WhitespaceRules { chars: " \t\n".into() }),
            multi_newline_paragraphs: true,
            groups: vec![Arc::new(GroupRule {
                group_type: GT_BRACE,
                open: "{".into(),
                close: "}".into(),
            })],
            commands: vec![CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            }],
            comments: vec![CommentRule { start: "%".into() }],
            forbidden_chars: String::new(),
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

    /// Drive a `NodesParser` over `tokens`, stage the outcome under a root `List`, and
    /// freeze. The reader must be reading `content`.
    fn try_run<'s, L: Lang<SourceOrigin = Option<String>>>(
        content: &'s str,
        tokens: &mut dyn TokenReader<'s, L>,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop: StopSpec<'_, L>,
    ) -> Result<Parsed<L>, ParseError> {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut session = ParserSession::new(recovery);
        let mut cx = ParseContext { tokens, state: Arc::clone(state), session: &mut session };
        let mut parser = NodesParser::new(Arc::clone(&source), stop);
        let (outcome, delta) = parser.parse(&mut cx)?;
        assert!(delta.is_none(), "NodesParser returns no pass-through delta");
        let pos = cx.tokens.pos();
        let root = session.builder.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            Arc::clone(state),
            outcome.nodes,
        );
        Ok(Parsed { result: session.finish(root), stop: outcome.stop, pos })
    }

    /// Scan `content` into the full token list (including the terminal `EndOfStream`).
    fn scan<'s, L: Lang>(content: &'s str, state: &ParsingState<L>) -> Vec<Token<'s, L>> {
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
    fn run_both<L: Lang<SourceOrigin = Option<String>>>(
        content: &str,
        state: &Arc<ParsingState<L>>,
        recovery: Recovery,
        stop_std: StopSpec<'_, L>,
        stop_list: StopSpec<'_, L>,
    ) -> Parsed<L> {
        let mut std_reader = StdTokenReader::new(content);
        let a =
            try_run(content, &mut std_reader, state, recovery, stop_std).expect("std reader");
        let mut list_reader = TokenListReader::new(scan(content, state));
        let b =
            try_run(content, &mut list_reader, state, recovery, stop_list).expect("list reader");
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
        assert_eq!(
            *err.kind(),
            ParseErrorKind::Token(TokenErrorKind::ForbiddenChar { ch: '#' })
        );
        assert_eq!(err.span().range(), 2..3);
    }

    #[test]
    fn escape_at_end_of_input_tolerant_recovers_and_repositions() {
        let st = state();
        let mut reader = StdTokenReader::new("ab \\");
        let parsed =
            try_run("ab \\", &mut reader, &st, Recovery::Tolerant, StopSpec::none()).unwrap();
        // The placeholder is an EndOfStream at the escape position; reading resumed at
        // the input's end (the reader is past the error, not before it).
        assert_eq!(shapes(&parsed.result), ["chars 0..3 \"ab \""]);
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_eq!(parsed.pos, 4);
        assert_eq!(parsed.result.diagnostics.len(), 1);
    }

    #[test]
    fn escape_at_end_of_input_strict_aborts() {
        let st = state();
        let mut reader = StdTokenReader::new("ab \\");
        let err =
            try_run("ab \\", &mut reader, &st, Recovery::Strict, StopSpec::none()).unwrap_err();
        assert!(matches!(
            err.kind(),
            ParseErrorKind::Token(TokenErrorKind::EndOfStreamAfterEscape { escape_char: '\\' })
        ));
    }

    // --- placeholder-arm recovery (commands 6.4, specials 6.4, groups 6.3) ---------------

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
        assert_eq!(
            parsed.result.diagnostics.iter().next().unwrap().message(),
            "cannot resolve command ‘\\foo’"
        );
        assert_eq!(parsed.stop, StopCause::EndOfInput);
        assert_partition(&parsed.result, 0..9);
    }

    #[test]
    fn unresolved_command_strict_aborts() {
        let st = state();
        let mut reader = StdTokenReader::new("a \\foo  b");
        let err = try_run("a \\foo  b", &mut reader, &st, Recovery::Strict, StopSpec::none())
            .unwrap_err();
        assert_eq!(err.to_string(), "cannot resolve command ‘\\foo’");
        assert_eq!(err.span().range(), 2..8);
    }

    #[test]
    fn group_open_recovers_as_chars_and_the_stray_close_is_reported() {
        let st = state();
        let parsed =
            run_both("a {b} c", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"a \"", "chars 2..3 \"{\"", "chars 3..4 \"b\""]
        );
        assert_eq!(parsed.stop, StopCause::UnexpectedGroupClose { span: Span::new(4, 5) });
        assert_eq!(parsed.pos, 4);
        assert_eq!(parsed.result.diagnostics.len(), 1);
    }

    #[test]
    fn specials_recover_as_a_chars_fallback() {
        #[derive(Debug, Clone, Copy)]
        struct TildeLang;
        impl Lang for TildeLang {
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type StateExt = ();
            type Event = ();
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

        let st = state_with(rules::<TildeLang>());
        let parsed =
            run_both("a~b", &st, Recovery::Tolerant, StopSpec::none(), StopSpec::none());
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..1 \"a\"", "chars 1..2 \"~\"", "chars 2..3 \"b\""]
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_partition(&parsed.result, 0..3);
    }

    // --- everything at once ----------------------------------------------------------------

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
