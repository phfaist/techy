//! Error types for the LaTeX parser.

use thiserror::Error;
use crate::token::Span;

/// Result type alias for parser operations.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Errors that can occur during parsing.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ParseError {
    /// Unexpected end of input while parsing.
    #[error("unexpected end of input at position {0}")]
    UnexpectedEndOfInput(usize),

    /// Encountered an unexpected token.
    #[error("unexpected token at {span:?}: expected {expected}, found {found}")]
    UnexpectedToken {
        span: Span,
        expected: String,
        found: String,
    },

    /// Expected a macro but found something else.
    #[error("expected macro at position {0}")]
    ExpectedMacro(usize),

    /// Unknown macro encountered.
    #[error("unknown macro '\\{name}' at {span:?}")]
    UnknownMacro { name: String, span: Span },

    /// Unknown environment encountered.
    #[error("unknown environment '{name}' at {span:?}")]
    UnknownEnvironment { name: String, span: Span },

    /// Mismatched environment (begin/end don't match).
    #[error("environment mismatch at {span:?}: expected \\end{{{expected}}}, found \\end{{{found}}}")]
    UnmatchedEnvironment {
        expected: String,
        found: String,
        span: Span,
    },

    /// Unmatched brace (opening or closing).
    #[error("unmatched {brace_type} brace at {span:?}")]
    UnmatchedBrace { brace_type: String, span: Span },

    /// Invalid argument specification.
    #[error("invalid argument specification: {0}")]
    InvalidArgumentSpec(String),

    /// Missing required argument.
    #[error("missing required argument for {construct} at {span:?}")]
    MissingArgument { construct: String, span: Span },

    /// Math mode error.
    #[error("math mode error at {span:?}: {message}")]
    MathModeError { message: String, span: Span },

    /// Generic parse error with custom message.
    #[error("parse error at {span:?}: {message}")]
    Generic { message: String, span: Span },
}

impl ParseError {
    /// Get the span where the error occurred, if available.
    pub fn span(&self) -> Option<Span> {
        match self {
            ParseError::UnexpectedEndOfInput(_) => None,
            ParseError::UnexpectedToken { span, .. } => Some(*span),
            ParseError::ExpectedMacro(_) => None,
            ParseError::UnknownMacro { span, .. } => Some(*span),
            ParseError::UnknownEnvironment { span, .. } => Some(*span),
            ParseError::UnmatchedEnvironment { span, .. } => Some(*span),
            ParseError::UnmatchedBrace { span, .. } => Some(*span),
            ParseError::InvalidArgumentSpec(_) => None,
            ParseError::MissingArgument { span, .. } => Some(*span),
            ParseError::MathModeError { span, .. } => Some(*span),
            ParseError::Generic { span, .. } => Some(*span),
        }
    }

    /// Create a formatted error message with source context.
    pub fn format_with_source(&self, source: &str) -> String {
        let mut msg = format!("{}", self);

        if let Some(span) = self.span() {
            if span.start < source.len() {
                // Get line and column information
                let (line, col) = get_line_col(source, span.start);
                msg.push_str(&format!("\n  at line {}, column {}", line, col));

                // Show the relevant source line
                if let Some(line_text) = get_line_text(source, line) {
                    msg.push_str(&format!("\n  | {}", line_text));
                    msg.push_str(&format!("\n  | {}^", " ".repeat(col - 1)));
                }
            }
        }

        msg
    }
}

/// Get line and column number for a position in the source.
fn get_line_col(source: &str, pos: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;

    for (i, ch) in source.chars().enumerate() {
        if i >= pos {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    (line, col)
}

/// Get the text of a specific line number.
fn get_line_text(source: &str, line_num: usize) -> Option<String> {
    source
        .lines()
        .nth(line_num.saturating_sub(1))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_formatting() {
        let source = "Hello\n\\unknown\nworld";
        let error = ParseError::UnknownMacro {
            name: "unknown".to_string(),
            span: Span { start: 6, end: 14 },
        };

        let formatted = error.format_with_source(source);
        assert!(formatted.contains("line 2"));
    }

    #[test]
    fn test_span_extraction() {
        let error = ParseError::UnknownMacro {
            name: "test".to_string(),
            span: Span { start: 10, end: 15 },
        };

        assert_eq!(error.span(), Some(Span { start: 10, end: 15 }));
    }
}
