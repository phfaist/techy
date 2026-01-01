//! Token types and tokenization for LaTeX source code.
//!
//! This module defines the token types that make up LaTeX syntax and provides
//! traits for reading tokens from a source.

pub mod reader;

pub use reader::StringTokenReader;

use crate::source::SourceLocation;
use crate::spec::SpecialsSpecBase;
use crate::state::ParsingState;
use std::fmt;

/// Types of tokens in LaTeX.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType<'lib> {
    /// Regular characters (text content).
    Char { chars: String },

    /// A macro/command (e.g., `\textbf`).
    Macro { macro_name: String, post_space: String },

    /// Beginning of an environment (e.g., `\begin{equation}`).
    BeginEnvironment { environment_name: String },

    /// End of an environment (e.g., `\end{equation}`).
    EndEnvironment { environment_name: String },

    /// Inline math mode delimiter (`$` or `\(` `\)`).
    MathModeInline { delimiter: String },

    /// Display math mode delimiter (`$$` or `\[` `\]`).
    MathModeDisplay { delimiter: String },

    /// A comment (starting with `%`).
    Comment(String),

    /// Opening brace `{`.
    BraceOpen,

    /// Closing brace `}`.
    BraceClose,

    /// Opening bracket `[`.
    BracketOpen,

    /// Closing bracket `]`.
    BracketClose,

    /// Special characters with meaning in LaTeX (e.g., `&`, `~`, `#`).
    Specials { specials_chars: String, specials_spec: dyn Box<SpecialsSpecBase> },
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Char(s) => write!(f, "Char({})", s),
            TokenType::Macro(s) => write!(f, "Macro(\\{})", s),
            TokenType::BeginEnvironment(s) => write!(f, "BeginEnvironment({})", s),
            TokenType::EndEnvironment(s) => write!(f, "EndEnvironment({})", s),
            TokenType::MathModeInline => write!(f, "MathModeInline"),
            TokenType::MathModeDisplay => write!(f, "MathModeDisplay"),
            TokenType::Comment(s) => write!(f, "Comment({})", s),
            TokenType::BraceOpen => write!(f, "BraceOpen"),
            TokenType::BraceClose => write!(f, "BraceClose"),
            TokenType::BracketOpen => write!(f, "BracketOpen"),
            TokenType::BracketClose => write!(f, "BracketClose"),
            TokenType::Specials(s) => write!(f, "Specials({})", s),
        }
    }
}

/// A token with source location and whitespace information.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The type of token.
    pub token_type: TokenType,
    /// The source location where this token appears.
    pub pos: SourceLocation,
    /// Whitespace that appeared before this token.
    pub pre_space: String,
}

impl Token {
    /// Create a new token.
    pub fn new(token_type: TokenType, pos: SourceLocation, pre_space: String)
    -> Self {
        Self {
            token_type,
            pos,
            pre_space,
        }
    }
}

/// Trait for reading tokens from a source.
pub trait TokenReader {
    /// Peek at the next token without consuming it.
    ///
    /// Returns `Ok(None)` if at end of input.
    fn peek_token(&mut self, &parsing_state : ParsingState)
     -> crate::Result<Option<Token>>;

    /// Consume and return the next token.
    ///
    /// Returns `Ok(None)` if at end of input.
    fn next_token(&mut self, &parsing_state : ParsingState)
     -> crate::Result<Option<Token>>;

    /// Get the current position in the source.
    fn position(&self, &parsing_state : ParsingState) -> usize;

    /// Check if we're at the end of input.
    fn is_at_end(&self, &parsing_state : ParsingState) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_display() {
        let token_type = TokenType::Macro("textbf".to_string());
        assert_eq!(format!("{}", token_type), "Macro(\\textbf)");
    }
}
