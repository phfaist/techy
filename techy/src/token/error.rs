//! Token-level errors and the recovery-token mechanism.
//!
//! A [`TokenError`] may carry a [`TokenRecovery`]: a placeholder token to emit *as if*
//! tokenization had succeeded, plus the stream position at which to resume reading. The
//! token reader itself is policy-free — it always reports the error; the session-level
//! [`Recovery`](crate::error::Recovery) policy decides whether to abort (strict)
//! or to record a [`Diagnostic`](crate::error::Diagnostic) and continue with the recovery
//! token (tolerant). Conversion to diagnostics happens there too; within the token layer,
//! errors are transient values carrying plain byte [`Span`](crate::source::Span)s, like
//! tokens themselves.
//!
//! Like the tokens themselves, these types are generic over the language type `L`: a
//! recovery holds one of the language's own tokens, and implementations of
//! `Lang::scan_specials` report their failures as these conditions.

use alloc::boxed::Box;
use core::fmt;

use crate::error::{DiagnosticData, DiagnosticInfo};
use crate::source::SourceSpan;
use crate::state::Lang;

use super::tokenization::{StreamPosition, Token};

/// Result of a tokenization operation: `T`, or a [`TokenError`] for the language `L`.
pub type TokenResult<L, T> = core::result::Result<T, TokenError<L>>;

/// The error that is reported when the input ends directly after an escape character
/// (id `core.token.end-of-stream-after-escape`).
///
/// An escape character starts a command and must be followed by the command name, as in
/// `\alpha` or `\&`. The escape characters of a language are those of its
/// [`CommandRule`](super::CommandRule)s.
///
/// Reported by the standard token reader ([`StdTokenReader`](super::StdTokenReader)) when
/// the escape character is the last character of its input. In a tolerant parse, the
/// escape character is parsed as an ordinary character of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.token.end-of-stream-after-escape",
    message = "Missing command name after ‘{escape_char}’ at the end of the \
               input"
)]
pub struct EndOfStreamAfterEscape {
    /// The escape character that ends the input.
    pub escape_char: char,
}

/// The error that is reported when a character that the language forbids appears in the
/// text (id `core.token.forbidden-char`).
///
/// A language lists the characters that may not appear as ordinary characters of the
/// text in [`ForbiddenCharsRules::chars`](super::ForbiddenCharsRules::chars). The list is
/// part of the parsing state, so a character can be forbidden in one part of a document
/// and allowed in another, e.g. in math mode and in text mode.
///
/// Reported by the standard token reader ([`StdTokenReader`](super::StdTokenReader)) when
/// it reads a forbidden character as an ordinary character. Whitespace and the characters
/// that start a group delimiter, a command, a comment or a specials trigger are read as
/// such and are not checked. In a tolerant parse, the forbidden character is parsed as
/// an ordinary character of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.token.forbidden-char")]
pub struct ForbiddenChar {
    /// The forbidden character (serialization key `char`).
    #[diagnostic(key = "char")]
    pub ch: char,
}

// Hand-written wording: the code-point rendering needs a cast (`as u32`), which the
// message format string cannot express.
impl fmt::Display for ForbiddenChar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Character ‘{}’ (U+{:04X}) may not be used here",
            self.ch,
            self.ch as u32
        )
    }
}

/// The condition of a [`TokenError`]: the description of what went wrong while reading a
/// token.
///
/// Each variant contains a condition that implements [`DiagnosticInfo`], so that a token
/// error is reported as a [`Diagnostic`](crate::error::Diagnostic) like any other
/// condition. The [`Custom`](Self::Custom) variant contains a condition that a language or
/// a custom token reader defines. To inspect a `Custom` condition, downcast it to its
/// type.
///
/// Variants may be added in a minor release; a `match` needs a catch-all arm.
#[derive(Debug, Clone)]
// `Custom` and `#[non_exhaustive]` serve different extension axes: `Custom` lets third
// parties define new condition *payloads*, while `non_exhaustive` reserves *our* right to
// promote a recurring condition to a first-class matchable variant in a minor release —
// without it, adding any variant breaks every exhaustive downstream `match` (semver).
#[non_exhaustive]
pub enum TokenErrorKind {
    /// The input ends directly after an escape character.
    EndOfStreamAfterEscape(EndOfStreamAfterEscape),
    /// A character that the language forbids appears in the text.
    ForbiddenChar(ForbiddenChar),
    /// A condition reported by the `Lang::scan_specials` hook of the language or by a
    /// custom [`TokenReader`](super::TokenReader). The token layer also uses this variant
    /// for an [`ImplementationError`](crate::core::constructs::ImplementationError) when
    /// such a hook or reader violates its contract.
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

/// An error that a [`TokenReader`](super::TokenReader) returns when it fails to read a
/// token.
///
/// A token error contains the condition that describes the error
/// ([`kind`](TokenError::kind)), the span of the source where the error occurred
/// ([`span`](TokenError::span)), and optionally a [`TokenRecovery`]
/// ([`recovery`](TokenError::recovery)). The recovery describes how a parse can continue
/// past the error: it contains a placeholder token to use in place of the token that
/// could not be read, and the position at which to resume reading.
///
/// The token reader always returns the error. The [`Recovery`](crate::error::Recovery)
/// policy of the parse decides what happens next. A strict parse stops with the error. A
/// tolerant parse reports the condition as a [`Diagnostic`](crate::error::Diagnostic) and
/// continues with the placeholder token. A token error without a recovery stops the
/// parse in both cases.
// The recovery payload is boxed: every `peek`/`next` returns a `Result` sized by its
// error variant, and the error path is cold by construction — boxing keeps the hot
// `Result` at payload-plus-tag size.
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
    pub token: Token<L>,
    /// The stream position at which to resume reading — one the reader produced, and not
    /// the position the failed read started from (see the
    /// [advancement contract](TokenRecovery#contract-resume-must-move-the-stream)).
    pub resume: StreamPosition<L>,
}

impl<L: Lang> TokenError<L> {
    /// Creates a token error from a condition, the source-qualified span it was
    /// detected at, and a [`TokenRecovery`] when the caller can describe one.
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
