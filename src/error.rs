//! Error types for the LaTeX parser.

use thiserror::Error;
use crate::source::SourceLocationDetails;

/// Result type alias for parser operations.
pub type Result<'src, T> = std::result::Result<T, ParseError<'src>>;

/// Errors that can occur during parsing.
#[derive(Error, Debug, Clone)]
pub enum ParseError<'src> {
    /// Unexpected end of input while parsing.
    #[error("unexpected end of input")]
    UnexpectedEndOfInput { pos: SourceLocationDetails<'src> },

    /// Encountered an unexpected token.
    #[error("unexpected token, expected {expected}, found {found}")]
    UnexpectedToken {
        pos: SourceLocationDetails<'src>,
        expected: String,
        found: String,
    },

    /// Expected a macro but found something else.
    #[error("expected macro")]
    ExpectedMacro { pos: SourceLocationDetails<'src> },

    /// Unknown macro encountered.
    #[error("unknown macro '\\{name}'")]
    UnknownMacro { name: String, pos: SourceLocationDetails<'src> },

    /// Unknown environment encountered.
    #[error("unknown environment '{name}'")]
    UnknownEnvironment { name: String, pos: SourceLocationDetails<'src> },

    /// Mismatched environment (begin/end don't match).
    #[error("environment mismatch, expected \\end{{{expected}}}, found \\end{{{found}}}")]
    UnmatchedEnvironment {
        expected: String,
        found: String,
        pos: SourceLocationDetails<'src>,
    },

    /// Unmatched brace (opening or closing).
    #[error("unmatched {brace_type} brace")]
    UnmatchedBrace { brace_type: String, pos: SourceLocationDetails<'src> },

    // /// Invalid argument specification.
    // #[error("invalid argument specification: {0}.  At {pos:?}")]
    // InvalidArgumentSpec { ..... pos: SourceLocation },

    /// Missing required argument.
    #[error("missing required argument for {construct}")]
    MissingArgument { construct: String, pos: SourceLocationDetails<'src> },

    /// Math mode error.
    #[error("math mode error: {message}")]
    MathModeError { message: String, pos: SourceLocationDetails<'src> },

    /// Generic parse error with custom message.
    #[error("parse error: {message}")]
    Generic { message: String, pos: SourceLocationDetails<'src> },
}

impl<'src> ParseError<'src> {
    /// Get the pos where the error occurred, if available.
    pub fn pos(&self) -> &SourceLocationDetails<'src> {
        match self {
            ParseError::UnexpectedEndOfInput { pos, .. } => pos,
            ParseError::UnexpectedToken { pos, .. } => pos,
            ParseError::ExpectedMacro { pos, .. } => pos,
            ParseError::UnknownMacro { pos, .. } => pos,
            ParseError::UnknownEnvironment { pos, .. } => pos,
            ParseError::UnmatchedEnvironment { pos, .. } => pos,
            ParseError::UnmatchedBrace { pos, .. } => pos,
            //ParseError::InvalidArgumentSpec(_) => None,
            ParseError::MissingArgument { pos, .. } => pos,
            ParseError::MathModeError { pos, .. } => pos,
            ParseError::Generic { pos, .. } => pos,
        }
    }

    /// Create a formatted error message with source context.
    pub fn format(&self) -> String {
        let mut msg = format!("{}", self);

        let pos = self.pos();
        msg.push_str(&format!("\n  at: {}", pos.formatted_location()));

        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::source::Source;

    #[test]
    fn test_error_formatting() {
        let source = Source::new("Hello\n\\unknown\nworld".to_string());
        let pos = source.make_pos(6, 14);
        let error = ParseError::UnknownMacro {
            name: "unknown".to_string(),
            pos: pos.details(),
        };

        let formatted = error.format();
        assert!(formatted.contains("line 2"));
    }

    #[test]
    fn test_pos_extraction() {
        let source = Source::new("Hello\nWorld\nTest".to_string());
        let pos = source.make_pos(10, 15);
        let error = ParseError::UnknownMacro {
            name: "test".to_string(),
            pos: pos.details(),
        };

        let pos_details = error.pos();
        assert_eq!(pos_details.start_line(), 2);
    }
}
