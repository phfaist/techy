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
    read_rigid_name_group, EnvironmentBody, EnvironmentBodyParser,
    EnvironmentTerminatorMismatch, MalformedEnvironmentTerminator,
    MissingEnvironmentTerminator, MissingTerminatorFound, NameGroup,
};
pub use group_parser::{GroupParser, UnclosedGroup, UnclosedGroupFound};
pub use invocation_parser::{parse_declared_arguments, StdInvocationParser};
pub use nodes_parser::{
    ExpressionCallableRequiresContent, NodesOutcome, NodesParser, StopCause, StopSpec,
    TokenStopCondition, TokenStopKind, UnresolvableCommand, UnusableRecoveryToken,
    UnusableRecoveryTokenKind,
};

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::fmt;

use crate::engine::{Frame, FrameTitle, ParserSession};
use crate::error::{DiagnosticData, DiagnosticInfo, ParseError, Recovery};
use crate::source::{Source, SourceSpan, Span};
use crate::spec::{CallableSpec, FrameRole};
use crate::state::{Lang, ParsingState, ParsingStateDelta};
use crate::token::{Token, TokenReader};

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
    /// [`SourceSpan`] requires (added July 2026, Phase 6.4, user-approved). It lives
    /// here, not on tokens or readers, because the token layer deliberately carries
    /// only transient byte spans (DESIGN_RATIONALE.md §3.8); the construct-parser layer
    /// is where byte spans become `Arc`-backed source spans. Not a parsing input:
    /// construct parsers make no forward parsing decision from raw content — even a
    /// verbatim parser reads `Char` tokens under a features-disabled state
    /// (DESIGN_RATIONALE.md §3.2, Action-02 entry).
    pub source: Arc<Source<L::SourceOrigin>>,
    /// The parser's **input** parsing state (the caller sets it; see the module docs
    /// for the state-threading convention).
    pub state: Arc<ParsingState<L>>,
    /// The session: node building, diagnostics, and the [`Recovery`] policy.
    pub session: &'a mut ParserSession<L>,
}

impl<'a, 's, L: Lang> ParseContext<'a, 's, L> {
    /// Bundle the four parse inputs into a context. Prefer this over a struct literal
    /// (the fields stay public for access): the context is the type's stated "one place
    /// to grow" (depth limits, cancellation), and construction through `new` keeps
    /// future fields from breaking every driver.
    pub fn new(
        tokens: &'a mut dyn TokenReader<'s, L>,
        source: Arc<Source<L::SourceOrigin>>,
        state: Arc<ParsingState<L>>,
        session: &'a mut ParserSession<L>,
    ) -> ParseContext<'a, 's, L> {
        ParseContext { tokens, source, state, session }
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
    /// [`Recovery::Tolerant`] — mirroring the content loop, whose re-read would abort
    /// anyway; reporting `None` first would only add a spurious absent-position recovery
    /// (and its diagnostic) on the way down.
    ///
    /// `state` is passed explicitly (usually `&Arc::clone(&cx.state)`; peeking never
    /// consumes) so a parser can probe under a derived state — an optional-argument
    /// parser peeking with its minted group rule in force — without swapping
    /// [`state`](ParseContext::state).
    pub fn probe_token(
        &mut self,
        state: &Arc<ParsingState<L>>,
    ) -> ConstructParserResult<L, Option<Token<'s, L>>> {
        match self.tokens.peek(state) {
            Ok(token) => Ok(Some(token)),
            Err(error) => {
                if self.session.recovery == Recovery::Tolerant && error.recovery().is_some() {
                    return Ok(None);
                }
                let span = SourceSpan::new(&self.source, error.span());
                Err(ParseError::from_token_error(error.kind().clone(), span)
                    .with_frames(self.session.snapshot_frames()))
            }
        }
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
    /// caller business (the §state "caller applies deltas" law).
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

    /// Build the abort error for an extension-implementation contract violation
    /// detected at `span` — an [`ImplementationError`] with the live traceback
    /// attached. `detail` is typically a [`NodeBuildError`](crate::node::NodeBuildError)
    /// or a literal contract description.
    ///
    /// Deliberately **not** the recover funnel: an implementation bug is not a source
    /// condition — it aborts even under [`Recovery::Tolerant`], and no
    /// [`Lang::refine_diagnostic`] pass applies.
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
/// under [`Recovery::Tolerant`] (built through
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
