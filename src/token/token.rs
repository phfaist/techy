//! Token types for LaTeX-like markup languages.

use std::fmt;
use crate::source::SourceLocation;

/// Types of tokens in LaTeX.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    /// Regular character(s) (text content).
    Char { content: String },

    /// A macro/command (e.g., `\textbf`).
    Macro { macro_name: String, post_space: String },

    /// Beginning of an environment (e.g., `\begin{equation}`).
    BeginEnvironment { environment_name: String },

    /// End of an environment (e.g., `\end{equation}`).
    EndEnvironment { environment_name: String },

    /// A comment (typically starting with `%`).
    Comment { comment: String, post_space: String },

    /// Typically an opening brace `{`
    GroupOpen { delimiter: String },

    /// Typically a closing brace `}`.
    GroupClose { delimiter: String },

    /// Paragraph break marker (space with multiple newlines, from first
    /// newline to final space after final newline), with possible pre_space
    /// before first newline.
    NewlinesParagraphBreak { space_chars: String },

    /// Special characters with meaning in LaTeX (e.g., `&`, `~`, `#`).
    Specials { specials_chars: String },
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Char { content } => write!(f, "Char('{}')", content),
            TokenType::Macro { macro_name, post_space } => {
                if post_space.is_empty() {
                    write!(f, "Macro(\\{})", macro_name)
                } else {
                    write!(f, "Macro(\\{}, post_space={:?})", macro_name, post_space)
                }
            }
            TokenType::BeginEnvironment { environment_name } => {
                write!(f, "BeginEnvironment('{}')", environment_name)
            }
            TokenType::EndEnvironment { environment_name } => {
                write!(f, "EndEnvironment('{}')", environment_name)
            }
            TokenType::Comment { comment, post_space } => {
                if post_space.is_empty() {
                    write!(f, "Comment('{}')", comment)
                } else {
                    write!(f, "Comment('{}', post_space={:?})", comment, post_space)
                }
            }
            TokenType::GroupOpen { delimiter } => {
                write!(f, "GroupOpen('{}')", delimiter)
            }
            TokenType::GroupClose { delimiter } => {
                write!(f, "GroupClose('{}')", delimiter)
            }
            TokenType::NewlinesParagraphBreak { space_chars } => {
                write!(f, "NewlinesParagraphBreak({:?})", space_chars)
            }
            TokenType::Specials { specials_chars } => {
                write!(f, "Specials('{}')", specials_chars)
            }
        }
    }
}

/// A token with source location and whitespace information.
#[derive(Debug, Clone, PartialEq)]
pub struct Token<'src> {
    /// The type of token.
    pub token_type: TokenType,
    /// The source location where this token appears.
    pub pos: SourceLocation<'src>,
    /// Whitespace that appeared before this token.
    pub pre_space: String,
}

impl<'src> Token<'src> {
    /// Create a new token.
    pub fn new(token_type: TokenType, pos: SourceLocation<'src>, pre_space: String) -> Self {
        Self {
            token_type,
            pos,
            pre_space,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_display() {
        let token_type = TokenType::Macro {
            macro_name: "textbf".to_string(),
            post_space: String::new(),
        };
        assert_eq!(format!("{}", token_type), "Macro(\\textbf)");

        let token_type_with_space = TokenType::Macro {
            macro_name: "textbf".to_string(),
            post_space: " ".to_string(),
        };
        assert_eq!(
            format!("{}", token_type_with_space),
            "Macro(\\textbf, post_space=\" \")"
        );
    }
}
