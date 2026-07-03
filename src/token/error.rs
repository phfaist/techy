//! Token-level errors and the recovery-token mechanism.
//!
//! A [`TokenError`] may carry a [`TokenRecovery`]: a placeholder token to emit *as if*
//! tokenization had succeeded, plus the position at which to resume reading. The token
//! reader itself is policy-free — it always reports the error; the session-level
//! [`Recovery`](crate::error::Recovery) policy (Phase 6) decides whether to abort (strict)
//! or to record a [`Diagnostic`](crate::error::Diagnostic) and continue with the recovery
//! token (tolerant). Conversion to Arc-span diagnostics happens there too; within the token
//! layer, errors are transient values carrying plain byte [`Span`]s, like tokens themselves.
//!
//! Like [`Token`], these types are generic over `L: Lang` (the recovery token rides
//! inside, and `Lang::scan_specials` implementations return them) — token machinery lives
//! wholly in the S1 stratum, so error types are free to grow language/state context later.

use core::fmt;

use crate::source::Span;
use crate::state::Lang;

use super::token::Token;

/// Result type of tokenization operations.
pub type TokenResult<'s, L, T> = core::result::Result<T, TokenError<'s, L>>;

/// What went wrong while reading a token.
///
/// Closed enum per the naming rule (`…Kind`); grows as the tokenizer learns to detect more
/// error conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenErrorKind {
    /// The input ended immediately after a command escape character, before any name.
    EndOfStreamAfterEscape {
        /// The escape character that was read.
        escape_char: char,
    },
    /// A character listed in `TokenRules::forbidden_chars` appeared as content.
    ForbiddenChar {
        /// The forbidden character encountered.
        ch: char,
    },
}

/// An error encountered while reading a token, with an optional recovery possibility.
pub struct TokenError<'s, L: Lang> {
    kind: TokenErrorKind,
    span: Span,
    recovery: Option<TokenRecovery<'s, L>>,
}

/// How to continue past a [`TokenError`] in tolerant mode: pretend `token` was read, then
/// resume reading at `resume_pos`.
///
/// `resume_pos` is explicit rather than derived from the token because the two can differ:
/// e.g. after an end-of-stream error the placeholder token is an `EndOfStream` at the
/// error position but reading resumes at the end of the input, past the offending escape
/// character.
pub struct TokenRecovery<'s, L: Lang> {
    /// The placeholder token to emit in place of the failed read.
    pub token: Token<'s, L>,
    /// Byte position at which to resume reading.
    pub resume_pos: usize,
}

impl<'s, L: Lang> TokenError<'s, L> {
    /// Create a token error.
    pub fn new(
        kind: TokenErrorKind,
        span: Span,
        recovery: Option<TokenRecovery<'s, L>>,
    ) -> Self {
        TokenError { kind, span, recovery }
    }

    /// What went wrong.
    pub fn kind(&self) -> TokenErrorKind {
        self.kind
    }

    /// Where in the content the error occurred.
    pub fn span(&self) -> Span {
        self.span
    }

    /// The recovery possibility, if the tokenizer could construct one.
    pub fn recovery(&self) -> Option<&TokenRecovery<'s, L>> {
        self.recovery.as_ref()
    }

    /// Consume the error, returning its recovery possibility if any.
    pub fn into_recovery(self) -> Option<TokenRecovery<'s, L>> {
        self.recovery
    }
}

// Manual impls to avoid spurious `L:` bounds (see token.rs).

impl<L: Lang> Clone for TokenRecovery<'_, L> {
    fn clone(&self) -> Self {
        TokenRecovery { token: self.token.clone(), resume_pos: self.resume_pos }
    }
}

impl<L: Lang> Clone for TokenError<'_, L> {
    fn clone(&self) -> Self {
        TokenError { kind: self.kind, span: self.span, recovery: self.recovery.clone() }
    }
}

impl<L: Lang> PartialEq for TokenRecovery<'_, L> {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token && self.resume_pos == other.resume_pos
    }
}

impl<L: Lang> Eq for TokenRecovery<'_, L> {}

impl<L: Lang> PartialEq for TokenError<'_, L> {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.span == other.span && self.recovery == other.recovery
    }
}

impl<L: Lang> Eq for TokenError<'_, L> {}

impl<L: Lang> fmt::Debug for TokenRecovery<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRecovery")
            .field("token", &self.token)
            .field("resume_pos", &self.resume_pos)
            .finish()
    }
}

impl<L: Lang> fmt::Debug for TokenError<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenError")
            .field("kind", &self.kind)
            .field("span", &self.span)
            .field("recovery", &self.recovery)
            .finish()
    }
}

impl<L: Lang> fmt::Display for TokenError<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TokenErrorKind::EndOfStreamAfterEscape { escape_char } => {
                write!(
                    f,
                    "expected command name after escape character ‘{}’ but reached end of input",
                    escape_char
                )
            }
            TokenErrorKind::ForbiddenChar { ch } => {
                write!(f, "character is forbidden here: ‘{}’ (U+{:04X})", ch, ch as u32)
            }
        }
    }
}

impl<L: Lang> core::error::Error for TokenError<'_, L> {}
