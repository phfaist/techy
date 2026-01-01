//! Parsers for individual LaTeX constructs.
//!
//! This module contains parsers for individual LaTeX constructs such as macros,
//! environments, groups, and other language elements. Each parser is responsible
//! for reading tokens and building the corresponding AST nodes.
//!
//! For the high-level parsing API, see the [`parser`](crate::parser) module.

pub mod general;

use crate::error::Result;
use crate::node::Node;
use crate::state::{ParsingState, ParsingStateDelta};
use crate::token::TokenReader;

/// Result type for parsers.
pub type ConstructParseResult<T> = Result<(T, Option<ParsingStateDelta>)>;

/// Base trait for all construct parsers.
pub trait ConstructParser {
    /// The type of value this parser produces.
    type Output;

    /// Parse from the token stream.
    ///
    /// Returns the parsed value and an optional state delta indicating
    /// how the parsing state should change after this parse.
    fn parse<'ctx>(
        &self,
        source: &str,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState<'ctx>,
    ) -> ParseResult<Self::Output>;
}

/// A parser that parses a single node.
pub struct SingleNodeParser;

impl Parser for SingleNodeParser {
    type Output = Node;

    fn parse<'ctx>(
        &self,
        source: &str,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState<'ctx>,
    ) -> ParseResult<Self::Output> {
        use crate::token::TokenType;
        use crate::node::*;
        use crate::error::ParseError;

        let token = token_reader
            .next_token()?
            .ok_or_else(|| ParseError::UnexpectedEndOfInput(token_reader.position()))?;

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
                // Parse group contents
                let parser = general::GeneralNodesParser::until_brace_close();
                let (nodelist, _) = parser.parse(source, token_reader, state)?;
                
                // Consume closing brace
                if let Some(close_token) = token_reader.next_token()? {
                    if !matches!(close_token.token_type, TokenType::BraceClose) {
                        return Err(ParseError::UnexpectedToken {
                            span: close_token.span,
                            expected: "closing brace".to_string(),
                            found: format!("{}", close_token.token_type),
                        });
                    }
                }

                Node::Group(GroupNode {
                    span: token.span,
                    nodelist,
                })
            }

            TokenType::Macro(name) => {
                // Look up macro specification
                let spec = state.context.get_macro(&name);
                
                // For now, create macro with no arguments
                // TODO: Parse arguments based on spec
                Node::Macro(MacroNode {
                    span: token.span,
                    name,
                    spec,
                    args: Arguments::empty(token.span),
                    post_space: String::new(),
                })
            }

            _ => {
                return Err(ParseError::UnexpectedToken {
                    span: token.span,
                    expected: "node".to_string(),
                    found: format!("{}", token.token_type),
                });
            }
        };

        Ok((node, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::ContextDb;
    use crate::token::StringTokenReader;

    #[test]
    fn test_single_node_parser() {
        let source = "hello".to_string();
        let mut reader = StringTokenReader::new(source.clone());
        let ctx = ContextDb::new();
        let state = ParsingState::new(&ctx);

        let parser = SingleNodeParser;
        let result = parser.parse(&source, &mut reader, &state);

        assert!(result.is_ok());
    }
}
