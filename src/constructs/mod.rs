//! Construct parsers: the parsing layer of the S1 core (ARCHITECTURE.md §constructs).
//!
//! [`ConstructParser`] is the single most important trait in the system: every construct —
//! the main content loop ([`NodesParser`]), groups ([`GroupParser`]), callable invocations
//! ([`StdInvocationParser`], behind the
//! [`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser) factory),
//! arguments (6.5), environment bodies (6.6) — is parsed by an implementation of it,
//! reading tokens and staging nodes through one [`ParseContext`].
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

mod child_state;
mod group_parser;
mod invocation_parser;
mod nodes_parser;

pub use child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
pub use group_parser::GroupParser;
pub use invocation_parser::StdInvocationParser;
pub use nodes_parser::{
    NodesOutcome, NodesParser, StopCause, StopSpec, TokenStopCondition, TokenStopKind,
};

use alloc::sync::Arc;
use core::fmt;

use crate::engine::ParserSession;
use crate::error::{ParseError, ParseErrorKind};
use crate::source::{Source, SourceSpan};
use crate::spec::CallableSpec;
use crate::state::{Lang, ParsingState, ParsingStateDelta};
use crate::token::{Token, TokenReader};

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
    /// The session: node building, diagnostics, and the [`Recovery`](crate::error::Recovery)
    /// policy.
    pub session: &'a mut ParserSession<L>,
}

impl<L: Lang> ParseContext<'_, '_, L> {
    /// Detection-site recovery — forwards to [`ParserSession::recover`]: tolerant mode
    /// records the condition as a diagnostic and returns `Ok(())` (the caller continues
    /// with its local recovery); strict mode returns the condition as a [`ParseError`]
    /// to bubble.
    pub fn recover(
        &mut self,
        kind: ParseErrorKind,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        self.session.recover(kind, span)
    }
}

impl<L: Lang> fmt::Debug for ParseContext<'_, '_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseContext")
            .field("pos", &self.tokens.pos())
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
