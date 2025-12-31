//! Error types for the LaTeX parser.

use thiserror::Error;
use crate::source::SourceLocation;

/// Result type alias for parser operations.
pub type Result<'src, T> = std::result::Result<T, ParseError<'src>>;

/// Errors that can occur during parsing.
#[derive(Error, Debug, Clone)]
pub enum ParseError<'src> {
    /// Unexpected end of input while parsing.
    #[error("unexpected end of input")]
    UnexpectedEndOfInput { pos: SourceLocation<'src> },

    // /// Encountered an unexpected token.
    // #[error("unexpected token, expected {expected}, found {found}")]
    // UnexpectedToken {
    //     pos: SourceLocation<'src>,
    //     expected: String,
    //     found: String,
    // },

    // /// Expected a macro but found something else.
    // #[error("expected macro")]
    // ExpectedMacro { pos: SourceLocation<'src> },

    // /// Unknown macro encountered.
    // #[error("unknown macro '\\{name}'")]
    // UnknownMacro { name: String, pos: SourceLocation<'src> },

    // /// Unknown environment encountered.
    // #[error("unknown environment '{name}'")]
    // UnknownEnvironment { name: String, pos: SourceLocation<'src> },

    // /// Mismatched environment (begin/end don't match).
    // #[error("environment mismatch, expected \\end{{{expected}}}, found \\end{{{found}}}")]
    // UnmatchedEnvironment {
    //     expected: String,
    //     found: String,
    //     pos: SourceLocation<'src>,
    // },

    // /// Unmatched brace (opening or closing).
    // #[error("unmatched {brace_type} brace")]
    // UnmatchedBrace { brace_type: String, pos: SourceLocation<'src> },

    // /// Invalid argument specification.
    // #[error("invalid argument specification: {0}.  At {pos:?}")]
    // InvalidArgumentSpec { ..... pos: SourceLocation },

    // /// Missing required argument.
    // #[error("missing required argument for {construct}")]
    // MissingArgument { construct: String, pos: SourceLocation<'src> },

    // /// Math mode error.
    // #[error("math mode error: {message}")]
    // MathModeError { message: String, pos: SourceLocation<'src> },

    /// Generic parse error with custom message.
    #[error("parse error: {message}")]
    Generic { message: String, pos: SourceLocation<'src> },
}

impl<'src> ParseError<'src> {
    /// Get the pos where the error occurred, if available.
    pub fn pos(&self) -> &SourceLocation<'src> {
        match self {
            ParseError::UnexpectedEndOfInput { pos, .. } => pos,
            // ParseError::UnexpectedToken { pos, .. } => pos,
            // ParseError::ExpectedMacro { pos, .. } => pos,
            // ParseError::UnknownMacro { pos, .. } => pos,
            // ParseError::UnknownEnvironment { pos, .. } => pos,
            // ParseError::UnmatchedEnvironment { pos, .. } => pos,
            // ParseError::UnmatchedBrace { pos, .. } => pos,
            //ParseError::InvalidArgumentSpec(_) => None,
            // ParseError::MissingArgument { pos, .. } => pos,
            //ParseError::MathModeError { pos, .. } => pos,
            ParseError::Generic { pos, .. } => pos,
        }
    }

    /// Create a formatted error message with source context.
    pub fn format(&self) -> String {
        use crate::source::Source;

        let mut msg = format!("{}", self);

        let pos = self.pos();

        // Get the source from the position and create an analyzer
        let source: &Source = pos.source();
        let mut analyzer = source.make_analyzer();

        // Format the position with line/column info
        let origin = if !source.origin().is_empty() {
            Some(source.origin())
        } else {
            None
        };

        if let Some((line, col)) = analyzer.get_line_col(pos.start()) {
            if let Some(origin) = origin {
                msg.push_str(&format!("\n  at: @ (line {}, col {}) [{}]", line, col, origin));
            } else {
                msg.push_str(&format!("\n  at: @ (line {}, col {})", line, col));
            }
        } else {
            if let Some(origin) = origin {
                msg.push_str(&format!("\n  at: @ char pos {} [{}]", pos.start(), origin));
            } else {
                msg.push_str(&format!("\n  at: @ char pos {}", pos.start()));
            }
        }

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
        let error = ParseError::Generic {
            message: "unknown".to_string(),
            pos,
        };

        let formatted = error.format();
        assert!(formatted.contains("line 2"));
    }

    #[test]
    fn test_pos_extraction() {
        let source = Source::new("Hello\nWorld\nTest".to_string());
        let pos = source.make_pos(10, 15);
        let error = ParseError::Generic {
            message: "test".to_string(),
            pos,
        };

        let extracted_pos = error.pos();
        assert_eq!(extracted_pos.start(), 10);
        assert_eq!(extracted_pos.end(), 15);
    }

    #[test]
    fn test_error_formatting_with_origin() {
        let source = Source::new("Hello\n\\unknown\nworld".to_string())
            .with_origin("test.tex".to_string());
        let pos = source.make_pos(6, 14);
        let error = ParseError::Generic {
            message: "unknown".to_string(),
            pos,
        };

        let formatted = error.format();
        assert!(formatted.contains("line 2"));
        assert!(formatted.contains("[test.tex]"));
    }
}
