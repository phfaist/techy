//! General node list parser.

use super::{ParseResult, Parser};
use crate::error::ParseError;
use crate::node::{CharsNode, CommentNode, GroupNode, MacroNode, Node, NodeList, Arguments};
use crate::source::Span;
use crate::state::ParsingState;
use crate::token::{TokenReader, TokenType};

/// Parser for a sequence of nodes.
///
/// This is the main workhorse parser that reads a sequence of LaTeX nodes
/// until it hits a stopping condition.
pub struct GeneralNodesParser {
    /// Stop when we encounter a closing brace `}`.
    pub stop_on_brace_close: bool,

    /// Stop when we encounter an environment end.
    pub stop_on_environment: Option<String>,

    /// Maximum number of nodes to read.
    pub max_nodes: Option<usize>,
}

impl GeneralNodesParser {
    /// Create a parser that reads until end of input.
    pub fn new() -> Self {
        Self {
            stop_on_brace_close: false,
            stop_on_environment: None,
            max_nodes: None,
        }
    }

    /// Create a parser that stops at a closing brace.
    pub fn until_brace_close() -> Self {
        Self {
            stop_on_brace_close: true,
            stop_on_environment: None,
            max_nodes: None,
        }
    }

    /// Create a parser that stops at the end of an environment.
    pub fn until_environment_end(env_name: String) -> Self {
        Self {
            stop_on_brace_close: false,
            stop_on_environment: Some(env_name),
            max_nodes: None,
        }
    }

    /// Set maximum number of nodes to parse.
    pub fn with_max_nodes(mut self, max: usize) -> Self {
        self.max_nodes = Some(max);
        self
    }

    /// Check if we should stop parsing based on the current token.
    fn should_stop(&self, token: &crate::token::Token) -> bool {
        match &token.token_type {
            TokenType::BraceClose if self.stop_on_brace_close => true,
            TokenType::EndEnvironment(name) => {
                if let Some(expected) = &self.stop_on_environment {
                    name == expected
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl Default for GeneralNodesParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser for GeneralNodesParser {
    type Output = NodeList;

    fn parse<'ctx>(
        &self,
        source: &str,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState<'ctx>,
    ) -> ParseResult<NodeList> {
        let start_pos = token_reader.position();
        let mut nodes = Vec::new();

        loop {
            // Check if we've reached max nodes
            if let Some(max) = self.max_nodes {
                if nodes.len() >= max {
                    break;
                }
            }

            // Peek at next token
            let token = match token_reader.peek_token()? {
                Some(t) => t,
                None => break, // End of input
            };

            // Check stopping conditions
            if self.should_stop(&token) {
                break;
            }

            // Consume the token
            token_reader.next_token()?;

            // Parse based on token type
            let node = match token.token_type {
                TokenType::Char(chars) => Node::Chars(CharsNode {
                    span: token.span,
                    chars,
                }),

                TokenType::Comment(comment) => Node::Comment(CommentNode {
                    span: token.span,
                    comment,
                    post_space: String::new(),
                }),

                TokenType::BraceOpen => {
                    // Parse group contents recursively
                    let parser = Self::until_brace_close();
                    let (nodelist, _) = parser.parse(source, token_reader, state)?;

                    // Consume closing brace
                    match token_reader.next_token()? {
                        Some(t) if matches!(t.token_type, TokenType::BraceClose) => {}
                        Some(t) => {
                            return Err(ParseError::UnexpectedToken {
                                span: t.span,
                                expected: "closing brace".to_string(),
                                found: format!("{}", t.token_type),
                            });
                        }
                        None => {
                            return Err(ParseError::UnmatchedBrace {
                                brace_type: "opening".to_string(),
                                span: token.span,
                            });
                        }
                    }

                    Node::Group(GroupNode {
                        span: token.span,
                        nodelist,
                    })
                }

                TokenType::BraceClose => {
                    return Err(ParseError::UnmatchedBrace {
                        brace_type: "closing".to_string(),
                        span: token.span,
                    });
                }

                TokenType::Macro(name) => {
                    // Look up macro specification
                    let spec = state.context.get_macro(&name);

                    // TODO: Parse arguments based on spec
                    // For now, just create macro with no arguments
                    Node::Macro(MacroNode {
                        span: token.span,
                        name,
                        spec,
                        args: Arguments::empty(token.span),
                        post_space: String::new(),
                    })
                }

                TokenType::BeginEnvironment(_name) => {
                    // TODO: Implement environment parsing
                    // For now, skip
                    continue;
                }

                TokenType::EndEnvironment(name) => {
                    // Unexpected end environment
                    return Err(ParseError::UnmatchedEnvironment {
                        expected: self
                            .stop_on_environment
                            .clone()
                            .unwrap_or_else(|| "none".to_string()),
                        found: name,
                        span: token.span,
                    });
                }

                // Skip other token types for now
                _ => continue,
            };

            nodes.push(node);
        }

        let end_pos = token_reader.position();

        Ok((
            NodeList::with_nodes(nodes, Span::new(start_pos, end_pos)),
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::ContextDb;
    use crate::token::StringTokenReader;

    #[test]
    fn test_parse_simple_text() {
        let source = "hello world".to_string();
        let mut reader = StringTokenReader::new(source.clone());
        let ctx = ContextDb::new();
        let state = ParsingState::new(&ctx);

        let parser = GeneralNodesParser::new();
        let result = parser.parse(&source, &mut reader, &state);

        assert!(result.is_ok());
        let (nodelist, _) = result.unwrap();
        assert!(!nodelist.is_empty());
    }

    #[test]
    fn test_parse_with_braces() {
        let source = "{hello}".to_string();
        let mut reader = StringTokenReader::new(source.clone());
        let ctx = ContextDb::new();
        let state = ParsingState::new(&ctx);

        let parser = GeneralNodesParser::new();
        let result = parser.parse(&source, &mut reader, &state);

        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_max_nodes() {
        let source = "one two three".to_string();
        let mut reader = StringTokenReader::new(source.clone());
        let ctx = ContextDb::new();
        let state = ParsingState::new(&ctx);

        let parser = GeneralNodesParser::new().with_max_nodes(1);
        let result = parser.parse(&source, &mut reader, &state);

        assert!(result.is_ok());
        let (nodelist, _) = result.unwrap();
        assert_eq!(nodelist.len(), 1);
    }
}
