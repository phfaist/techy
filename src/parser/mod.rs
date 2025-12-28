//! High-level parsing API.
//!
//! The `Parser` provides the main entry point for parsing LaTeX-like documents.

use crate::error::Result;
use crate::node::NodeList;
use crate::parsing::{general::GeneralNodesParser, Parser as ParserTrait};
use crate::spec::ContextDb;
use crate::state::ParsingState;
use crate::token::StringTokenReader;

/// High-level LaTeX-like markup parser.
///
/// This is the main interface for parsing LaTeX-like documents. It manages
/// the source string, token reader, and parsing context.
///
/// # Example
///
/// ```
/// use techy::Parser;
///
/// let source = r"\textbf{Hello} world!";
/// let parser = Parser::new(source.to_string());
/// let ast = parser.parse().unwrap();
///
/// println!("Parsed {} nodes", ast.nodes.len());
/// ```
pub struct Parser {
    /// The source code.
    source: String,
    /// The context database (known macros/environments).
    context: ContextDb,
}

impl Parser {
    /// Create a new parser with default context.
    ///
    /// The default context includes standard LaTeX macros and environments.
    pub fn new(source: String) -> Self {
        Self::with_context(source, ContextDb::default())
    }

    /// Create a new parser with a custom context.
    pub fn with_context(source: String, context: ContextDb) -> Self {
        Self { source, context }
    }

    /// Parse the entire document.
    ///
    /// Returns a `NodeList` containing all the parsed nodes.
    pub fn parse(&self) -> Result<NodeList> {
        let mut token_reader = StringTokenReader::new(self.source.clone());
        let state = ParsingState::new(&self.context);

        let parser = GeneralNodesParser::new();
        let (nodelist, _) = parser.parse(&self.source, &mut token_reader, &state)?;

        Ok(nodelist)
    }

    /// Get the source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the context database.
    pub fn context(&self) -> &ContextDb {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let parser = Parser::new("Hello world".to_string());
        let result = parser.parse();
        assert!(result.is_ok());

        let nodelist = result.unwrap();
        assert!(!nodelist.is_empty());
    }

    #[test]
    fn test_parse_with_macro() {
        let parser = Parser::new(r"\textbf{bold}".to_string());
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_with_groups() {
        let parser = Parser::new("{hello} {world}".to_string());
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_custom_context() {
        let mut context = ContextDb::new();
        use crate::spec::MacroSpec;
        context.add_macro(MacroSpec::simple("mycmd", "{"));

        let parser = Parser::with_context(r"\mycmd{test}".to_string(), context);
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_source_accessor() {
        let parser = Parser::new("test".to_string());
        assert_eq!(parser.source(), "test");
    }
}
