//! Token types and tokenization for LaTeX source code.
//!
//! This module defines the token types that make up LaTeX syntax and provides
//! traits for reading tokens from a source.

pub mod reader;

pub use reader::StringTokenReader;

use std::fmt;

/// A span representing a range in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Starting byte position (inclusive).
    pub start: usize,
    /// Ending byte position (exclusive).
    pub end: usize,
}

impl Span {
    /// Create a new span.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Get the length of the span.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Check if the span is empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Get the text this span refers to in the source.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }

    /// Extend this span to include another span.
    pub fn extend(&self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Types of tokens in LaTeX.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    /// Regular characters (text content).
    Char(String),

    /// A macro/command (e.g., `\textbf`).
    Macro(String),

    /// Beginning of an environment (e.g., `\begin{equation}`).
    BeginEnvironment(String),

    /// End of an environment (e.g., `\end{equation}`).
    EndEnvironment(String),

    /// Inline math mode delimiter (`$` or `\(` `\)`).
    MathModeInline,

    /// Display math mode delimiter (`$$` or `\[` `\]`).
    MathModeDisplay,

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
    Specials(String),
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
    /// The span in the source where this token appears.
    pub span: Span,
    /// Whitespace that appeared before this token.
    pub pre_space: String,
}

impl Token {
    /// Create a new token.
    pub fn new(token_type: TokenType, span: Span, pre_space: String) -> Self {
        Self {
            token_type,
            span,
            pre_space,
        }
    }

    /// Get the text content of this token from the source.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        self.span.text(source)
    }
}

/// Trait for reading tokens from a source.
pub trait TokenReader {
    /// Peek at the next token without consuming it.
    ///
    /// Returns `Ok(None)` if at end of input.
    fn peek_token(&mut self) -> crate::Result<Option<Token>>;

    /// Consume and return the next token.
    ///
    /// Returns `Ok(None)` if at end of input.
    fn next_token(&mut self) -> crate::Result<Option<Token>>;

    /// Get the current position in the source.
    fn position(&self) -> usize;

    /// Check if we're at the end of input.
    fn is_at_end(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = Span::new(0, 5);
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 5);
        assert_eq!(span.len(), 5);
    }

    #[test]
    fn test_span_text() {
        let source = "Hello world";
        let span = Span::new(0, 5);
        assert_eq!(span.text(source), "Hello");
    }

    #[test]
    fn test_span_extend() {
        let span1 = Span::new(0, 5);
        let span2 = Span::new(3, 10);
        let extended = span1.extend(span2);
        assert_eq!(extended.start, 0);
        assert_eq!(extended.end, 10);
    }

    #[test]
    fn test_token_display() {
        let token_type = TokenType::Macro("textbf".to_string());
        assert_eq!(format!("{}", token_type), "Macro(\\textbf)");
    }
}
