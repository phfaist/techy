//! Token-level errors and the recovery-token mechanism.
//!
//! A [`TokenError`] may carry a [`TokenRecovery`]: a placeholder token to emit *as if*
//! tokenization had succeeded, plus the stream position at which to resume reading. The
//! token reader itself is policy-free — it always reports the error; the session-level
//! [`Recovery`](crate::error::Recovery) policy decides whether to abort (strict)
//! or to record a [`Diagnostic`](crate::error::Diagnostic) and continue with the recovery
//! token (tolerant). Conversion to Arc-span diagnostics happens there too; within the token
//! layer, errors are transient values carrying plain byte [`Span`]s, like tokens themselves.
//!
//! Like the tokens themselves, these types are generic over `L: Lang` (the recovery token rides
//! inside, and `Lang::scan_specials` implementations return them) — token machinery lives
//! wholly in the S1 stratum, so error types are free to grow language/state context later.

use alloc::boxed::Box;
use core::fmt;

use crate::error::{DiagnosticData, DiagnosticInfo};
use crate::source::SourceSpan;
use crate::state::Lang;


/// Result type of tokenization operations.
pub type TokenResult<L, T> = core::result::Result<T, TokenError<L>>;

/// Condition: the input ended immediately after a command escape character, before any
/// name (the token layer's conditions are ordinary
/// [`DiagnosticInfo`] data structs, wrapped by [`TokenErrorKind`] for the recovery
/// protocol).
#[derive(Debug, Clone, Copy, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.token.end-of-stream-after-escape",
    message = "expected command name after escape character ‘{escape_char}’ but reached \
               end of input"
)]
pub struct EndOfStreamAfterEscape {
    /// The escape character that was read.
    pub escape_char: char,
}

/// Condition: a character listed in `ForbiddenCharsRules::chars` appeared as content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.token.forbidden-char")]
pub struct ForbiddenChar {
    /// The forbidden character encountered (serialization key `char`).
    #[diagnostic(key = "char")]
    pub ch: char,
}

// Hand-written wording: the code-point rendering needs a cast (`as u32`), which the
// message format string cannot express.
impl fmt::Display for ForbiddenChar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "character is forbidden here: ‘{}’ (U+{:04X})", self.ch, self.ch as u32)
    }
}

/// What went wrong while reading a token.
///
/// Closed enum per the naming rule (`…Kind`); grows as the tokenizer learns to detect
/// more error conditions — hence `#[non_exhaustive]`. The built-in variants wrap plain
/// condition structs (each a [`DiagnosticInfo`] impl) and [`Custom`](Self::Custom)
/// carries any language-defined payload, so token errors join the structured-diagnostics
/// model while the token layer keeps a concrete matchable enum for the recovery protocol.
/// Not `Copy` (a custom payload is boxed) and no `PartialEq`
/// — consumers match the variants or downcast the payload.
#[derive(Debug, Clone)]
// `Custom` and `#[non_exhaustive]` serve different extension axes: `Custom` lets third
// parties define new condition *payloads*, while `non_exhaustive` reserves *our* right to
// promote a recurring condition to a first-class matchable variant in a minor release —
// without it, adding any variant breaks every exhaustive downstream `match` (semver).
#[non_exhaustive]
pub enum TokenErrorKind {
    /// The input ended immediately after a command escape character, before any name.
    EndOfStreamAfterEscape(EndOfStreamAfterEscape),
    /// A character listed in `ForbiddenCharsRules::chars` appeared as content.
    ForbiddenChar(ForbiddenChar),
    /// A language-defined condition, reported by an extension point participating in
    /// the recovery protocol (`Lang::scan_specials`, a custom
    /// [`TokenReader`](super::TokenReader)) — one extension mechanism serves both
    /// layers.
    Custom(Box<dyn DiagnosticData>),
}

impl TokenErrorKind {
    /// Lift the kind into a condition payload: the built-in
    /// conditions are boxed; a `Custom` payload is unwrapped, never double-boxed.
    pub(crate) fn into_condition(self) -> Box<dyn DiagnosticData> {
        match self {
            TokenErrorKind::EndOfStreamAfterEscape(condition) => Box::new(condition),
            TokenErrorKind::ForbiddenChar(condition) => Box::new(condition),
            TokenErrorKind::Custom(data) => data,
        }
    }
}

/// An error encountered while reading a token, with an optional recovery possibility.
///
/// The recovery payload is boxed: every `peek`/`next` returns a `Result` sized by its
/// error variant, and the error path is cold by construction — boxing keeps the hot
/// `Result` at payload-plus-tag size.
pub struct TokenError<L: Lang> {
    kind: TokenErrorKind,
    span: SourceSpan<L::SourceOrigin>,
    recovery: Option<Box<TokenRecovery<L>>>,
}

/// How to continue past a [`TokenError`] in tolerant mode: pretend `token` was read, then
/// resume reading at `resume`.
///
/// `resume` is explicit rather than derived from the token: a custom token source's
/// placeholder need not end where reading should resume (its span may stand for
/// normalized or synthesized content), and the explicit position is what the content
/// loop's advancement check (below) is enforced against.
///
/// # Contract: `resume` must move the stream
///
/// `resume` must name a place the reader is not already at — in particular, one past
/// the error. The content loop's recovery arm consumes no token (the placeholder was
/// never in the stream), so its termination rests entirely on this move: after
/// [`move_to_position(&resume)`](super::TokenReader::move_to_position) the loop
/// compares [`position_here()`](super::TokenReader::position_here) with the position it
/// held before, and treats an unchanged position as a contract violation by the token
/// source — it aborts the parse with the token error, even in tolerant mode. Stream
/// positions compare only for equality, so the check is "different", not "greater".
pub struct TokenRecovery<L: Lang> {
    /// The placeholder token to emit in place of the failed read.
    pub token: L::Token,
    /// The stream position at which to resume reading — one the reader minted, and not
    /// the position the failed read started from (see the
    /// [advancement contract](TokenRecovery#contract-resume-must-move-the-stream)).
    pub resume: L::StreamPosition,
}

impl<L: Lang> TokenError<L> {
    /// Create a token error.
    pub fn new(
        kind: TokenErrorKind,
        span: SourceSpan<L::SourceOrigin>,
        recovery: Option<TokenRecovery<L>>,
    ) -> Self {
        TokenError { kind, span, recovery: recovery.map(Box::new) }
    }

    /// What went wrong.
    pub fn kind(&self) -> &TokenErrorKind {
        &self.kind
    }

    /// Where the error occurred — source-qualified, so it is a diagnostic anchor as it
    /// stands, whichever source the reader was reading.
    pub fn span(&self) -> &SourceSpan<L::SourceOrigin> {
        &self.span
    }

    /// The recovery possibility, if the tokenizer could construct one.
    pub fn recovery(&self) -> Option<&TokenRecovery<L>> {
        self.recovery.as_deref()
    }

    /// Consume the error, returning its recovery possibility if any.
    pub fn into_recovery(self) -> Option<TokenRecovery<L>> {
        self.recovery.map(|boxed| *boxed)
    }
}

// Manual impls to avoid spurious `L:` bounds (see token.rs).

impl<L: Lang> Clone for TokenRecovery<L> {
    fn clone(&self) -> Self {
        TokenRecovery { token: self.token.clone(), resume: self.resume.clone() }
    }
}

impl<L: Lang> Clone for TokenError<L> {
    fn clone(&self) -> Self {
        TokenError {
            kind: self.kind.clone(),
            span: self.span.clone(),
            recovery: self.recovery.clone(),
        }
    }
}

impl<L: Lang> PartialEq for TokenRecovery<L> {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token && self.resume == other.resume
    }
}

impl<L: Lang> Eq for TokenRecovery<L> {}

// No PartialEq for TokenError: its kind may carry a dyn condition payload ([§dd-dr:errors]) —
// consumers match the kind's variants or downcast the payload.

impl<L: Lang> fmt::Debug for TokenRecovery<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRecovery")
            .field("token", &self.token)
            .field("resume", &self.resume)
            .finish()
    }
}

impl<L: Lang> fmt::Debug for TokenError<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenError")
            .field("kind", &self.kind)
            .field("span", &self.span)
            .field("recovery", &self.recovery)
            .finish()
    }
}

impl fmt::Display for TokenErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The wording lives on the condition payloads (the message is a pure function
        // of the payload, [§dd-dr:errors]); the enum only delegates.
        match self {
            TokenErrorKind::EndOfStreamAfterEscape(condition) => {
                fmt::Display::fmt(condition, f)
            }
            TokenErrorKind::ForbiddenChar(condition) => fmt::Display::fmt(condition, f),
            TokenErrorKind::Custom(data) => fmt::Display::fmt(data, f),
        }
    }
}

impl<L: Lang> fmt::Display for TokenError<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl<L: Lang> core::error::Error for TokenError<L> {}
