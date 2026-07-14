//! Construct parsers: the parsing layer of the S1 core (ARCHITECTURE.md §constructs).
//!
//! [`ConstructParser`] is the single most important trait in the system: every construct —
//! the main content loop ([`NodesParser`]), groups ([`GroupParser`]), callable invocations
//! ([`StdInvocationParser`], behind the
//! [`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser) factory),
//! arguments (the standard [`ArgumentParser`](crate::spec::ArgumentParser)
//! implementations), environment bodies ([`EnvironmentBodyParser`]) — is parsed by an
//! implementation of it, reading tokens and staging nodes through one [`ParseContext`].
//!
//! # The two-tier ownership model (DESIGN_RATIONALE.md §3.6)
//!
//! Construct parsers are **temporaries** (tier 2): constructed with their per-use
//! configuration where they are needed, `parse(&mut self, …)` so working state lives in
//! fields, free to borrow (`'s` content, token refs), dropped when the frame ends —
//! never stored in specs. *Stored* behavior objects (tier 1 — specs,
//! [`ArgumentParser`](crate::spec::ArgumentParser)s) are `Arc`-shared, `Send + Sync`,
//! immutable, and receive every per-use input as arguments. Closures (stop predicates)
//! are thereby confined to tier 2; specs stay data.
//!
//! # State threading (the §state "caller applies deltas" law, pinned to `cx`)
//!
//! [`ParseContext::state`] is the parser's **input** state — the caller sets it. A parser
//! that scopes a child state (group interior, argument extent, slot body) derives it
//! locally and either builds a child `cx` or swaps `cx.state` and restores it afterwards
//! (structural revert — `Arc` clone is cheap). The `Option<ParsingStateDelta>` in the
//! return value is exclusively the *after-effect for the caller* (`\newcommand`).
//!
//! # Errors (DESIGN_RATIONALE.md §3.8)
//!
//! `Err` means **abort**: recovery happens at the detection site (the
//! [`recover`](ParseContext::recover) helper), and abnormal endings of sub-parses travel
//! as data ([`StopCause`]) — nobody continues past an `Err`.

mod argument_parsers;
mod child_state;
mod environment_parser;
mod group_parser;
mod invocation_parser;
mod nodes_parser;

pub use argument_parsers::{
    scan_argument_noise, stage_pre_space, ArgumentNoise, ExpectedExpressionArgument,
    ExpressionParser, GroupArgumentParser, MarkerArgumentParser, MissingMandatoryArgument,
    OptionalGroupArgumentParser,
};
pub use child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
pub use environment_parser::{
    EnvironmentBody, EnvironmentBodyParser, EnvironmentTerminatorMismatch,
    MalformedEnvironmentTerminator, MissingEnvironmentTerminator, MissingTerminatorFound,
};
pub use group_parser::{GroupParser, UnclosedGroup, UnclosedGroupFound};
pub use invocation_parser::StdInvocationParser;
pub use nodes_parser::{
    ExpressionCallableTakesArguments, NodesOutcome, NodesParser, StopCause, StopSpec,
    TokenStopCondition, TokenStopKind, UnresolvableCommand, UnusableRecoveryToken,
    UnusableRecoveryTokenKind,
};

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;

use crate::engine::{Frame, FrameTitle, ParserSession};
use crate::error::{DiagnosticData, DiagnosticInfo, ParseError, Recovery};
use crate::source::{Source, SourceSpan, Span};
use crate::spec::{CallableSpec, FrameRole};
use crate::state::{Lang, ParsingState, ParsingStateDelta};
use crate::token::{Token, TokenKind, TokenReader};

/// Reposition the reader to an absolute byte position (a `TokenRecovery::resume_pos`,
/// an argument parser's absent-rewind target).
///
/// [`TokenReader`] expresses positioning through tokens, so "go to `pos`" is phrased as
/// moving to a zero-width marker token at `pos` — `move_to(tok, false)` means "position
/// = `tok.span.start`" for any reader honoring the span conventions.
///
/// `resume_at` itself is deliberately bidirectional — it also serves as the rewind for
/// absent arguments and environment name groups — so it asserts nothing about the
/// direction of the move. When adopting a `TokenRecovery`, the *caller* enforces the
/// [`resume_pos` advancement contract](crate::token::TokenRecovery#contract-resume_pos-must-advance-the-reader):
/// the content loop aborts if the reader did not advance.
pub(crate) fn resume_at<'s, L: Lang>(tokens: &mut dyn TokenReader<'s, L>, pos: usize) {
    let marker: Token<'s, L> =
        Token::new(TokenKind::EndOfStream, Span::empty(pos), Span::empty(pos));
    tokens.move_to(&marker, false);
}

/// Peek under the current state, mapping a tokenizer error per the recovery policy:
/// strict mode aborts with the token error (mirroring the content loop); tolerant mode
/// reports `None` **without diagnosing or consuming** — the caller treats the position
/// as unusable (argument absent, terminator malformed) and the enclosing content loop
/// re-reads the error and applies its own token recovery, avoiding a double report.
///
/// A token error carrying **no** recovery is unrecoverable and aborts even under
/// [`Recovery::Tolerant`] — mirroring the content loop, whose re-read would abort
/// anyway; reporting `None` first would only add a spurious absent-position recovery
/// (and its diagnostic) on the way down.
pub(crate) fn try_peek<'s, L: Lang>(
    cx: &mut ParseContext<'_, 's, L>,
) -> ConstructParserResult<L, Option<Token<'s, L>>> {
    match cx.tokens.peek(&cx.state) {
        Ok(token) => Ok(Some(token)),
        Err(error) => {
            if cx.session.recovery == Recovery::Tolerant && error.recovery().is_some() {
                return Ok(None);
            }
            let span = SourceSpan::new(&cx.source, error.span().range());
            Err(ParseError::from_token_error(error.kind().clone(), span)
                .with_frames(cx.session.snapshot_frames()))
        }
    }
}

/// The live frame covering a resolved invocation's parse (the dispatch push site,
/// DESIGN_RATIONALE.md §3.8): the spec's title hook with the invocation spelling — the
/// trigger token minus its syntactic post-space — anchored at the trigger. Built before
/// the `Invocation` moves into the spec's parser factory; allocation-free (`Arc` bumps
/// only).
pub(crate) fn invocation_frame<L: Lang>(
    cx: &ParseContext<'_, '_, L>,
    invocation: &Invocation<'_, '_, L>,
) -> Frame<L> {
    let token = invocation.token;
    let name_span = Span::new(token.span.start, token.post_space().start);
    Frame {
        title: FrameTitle::Callable {
            spec: Arc::clone(invocation.spec),
            role: FrameRole::Invocation,
            name: SourceSpan::new(&cx.source, name_span.range()),
        },
        span: SourceSpan::new(&cx.source, token.span.range()),
    }
}

/// Everything a construct parser needs, in one context value — avoiding pylatexenc's
/// three-argument threading (`walker, token_reader, parsing_state`) and giving the API
/// one place to grow (depth limits, cancellation).
pub struct ParseContext<'a, 's, L: Lang> {
    /// The token stream.
    pub tokens: &'a mut dyn TokenReader<'s, L>,
    /// The source the token spans refer into — what staging a node's
    /// [`SourceSpan`] requires (added July 2026, Phase 6.4, user-approved). It lives
    /// here, not on tokens or readers, because the token layer deliberately carries
    /// only transient byte spans (DESIGN_RATIONALE.md §3.8); the construct-parser layer
    /// is where byte spans become `Arc`-backed source spans.
    pub source: Arc<Source<L::SourceOrigin>>,
    /// The parser's **input** parsing state (the caller sets it; see the module docs
    /// for the state-threading convention).
    pub state: Arc<ParsingState<L>>,
    /// The session: node building, diagnostics, and the [`Recovery`] policy.
    pub session: &'a mut ParserSession<L>,
}

impl<L: Lang> ParseContext<'_, '_, L> {
    /// Detection-site recovery — **the recover funnel** (DESIGN_RATIONALE.md §3.8):
    /// boxes the condition and hands it to the session's record-or-abort primitive
    /// ([`ParserSession::recover`]). Tolerant mode records the condition as an
    /// error-severity diagnostic and returns `Ok(())` (the caller continues with its
    /// local recovery); strict mode returns the condition as a [`ParseError`] to bubble.
    ///
    /// The funnel lives here, not on the session, because condition refinement
    /// (`Lang::refine_diagnostic`) needs the context's parsing state.
    pub fn recover(
        &mut self,
        condition: impl DiagnosticInfo,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        self.recover_boxed(Box::new(condition), span)
    }

    /// The funnel's boxed entry — for payloads that already live behind the dyn facade
    /// (the token-error lift, where a `Custom` payload must not be double-boxed).
    /// Applies [`Lang::refine_diagnostic`] exactly once — this is why the funnel sits on
    /// the context: refinement needs the parsing state.
    pub(crate) fn recover_boxed(
        &mut self,
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        let data = L::refine_diagnostic(data, &self.state);
        self.session.recover(data, span)
    }

    /// Run `f` with `frame` pushed on the session's live frame stack — the descent-point
    /// primitive of the parse traceback (DESIGN_RATIONALE.md §3.8): every condition the
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
    // The output-plus-delta pair is the decided signature (DESIGN_RATIONALE.md §3.6);
    // splitting it into a named type would only rename the complexity.
    #[allow(clippy::type_complexity)]
    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (Self::Output, Option<ParsingStateDelta<L>>)>;
}

/// One resolved callable invocation, as handed to
/// [`CallableSpec::make_invocation_parser`]: the dispatch loop resolves the trigger
/// token (via [`Lang::resolve_command`], or the
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
    /// The invocation spelling, as written (the node stores an owned copy).
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
