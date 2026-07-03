//! Token-level errors and the recovery-token mechanism.
//!
//! A [`TokenError`] may carry a [`TokenRecovery`]: a placeholder token to emit *as if*
//! tokenization had succeeded, plus the position at which to resume reading. The token
//! reader itself is policy-free — it always reports the error; the session-level
//! [`Recovery`](crate::error::Recovery) policy (Phase 6) decides whether to abort (strict)
//! or to record a [`Diagnostic`](crate::error::Diagnostic) and continue with the recovery
//! token (tolerant). Conversion to Arc-span diagnostics happens there too; within the token
//! layer, errors are transient values carrying plain byte [`Span`]s, like tokens themselves.

use core::fmt;

use super::span::Span;
use super::token::Token;

/// Result type of tokenization operations.
pub type TokenResult<'s, T> = core::result::Result<T, TokenError<'s>>;

/// What went wrong while reading a token.
///
/// Closed enum per the naming rule (`…Kind`); grows as the tokenizer learns to detect more
/// error conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenErrorKind {
    /// The input ended immediately after a macro escape character, before any name.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenError<'s> {
    kind: TokenErrorKind,
    span: Span,
    recovery: Option<TokenRecovery<'s>>,
}

/// How to continue past a [`TokenError`] in tolerant mode: pretend `token` was read, then
/// resume reading at `resume_pos`.
///
/// `resume_pos` is explicit rather than derived from the token because the two can differ:
/// e.g. after an end-of-stream error the placeholder token is empty but reading resumes at
/// the end of the input, past the offending escape character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenRecovery<'s> {
    /// The placeholder token to emit in place of the failed read.
    pub token: Token<'s>,
    /// Byte position at which to resume reading.
    pub resume_pos: usize,
}

impl<'s> TokenError<'s> {
    /// Create a token error.
    pub fn new(kind: TokenErrorKind, span: Span, recovery: Option<TokenRecovery<'s>>) -> Self {
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
    pub fn recovery(&self) -> Option<&TokenRecovery<'s>> {
        self.recovery.as_ref()
    }

    /// Consume the error, returning its recovery possibility if any.
    pub fn into_recovery(self) -> Option<TokenRecovery<'s>> {
        self.recovery
    }
}

impl fmt::Display for TokenError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TokenErrorKind::EndOfStreamAfterEscape { escape_char } => {
                write!(
                    f,
                    "expected macro name after escape character ‘{}’ but reached end of input",
                    escape_char
                )
            }
            TokenErrorKind::ForbiddenChar { ch } => {
                write!(f, "character is forbidden here: ‘{}’ (U+{:04X})", ch, ch as u32)
            }
        }
    }
}

impl core::error::Error for TokenError<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages() {
        let err = TokenError::new(
            TokenErrorKind::EndOfStreamAfterEscape { escape_char: '\\' },
            Span::new(3, 4),
            None,
        );
        assert_eq!(
            format!("{}", err),
            "expected macro name after escape character ‘\\’ but reached end of input"
        );

        let err = TokenError::new(TokenErrorKind::ForbiddenChar { ch: '%' }, Span::new(0, 1), None);
        assert_eq!(format!("{}", err), "character is forbidden here: ‘%’ (U+0025)");
    }
}
