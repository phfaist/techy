//! High-level parsing API.
//!
//! The `LatexWalker` provides the main entry point for parsing LaTeX documents.

use crate::error::Result;
use crate::node::NodeList;
use crate::parser::{general::GeneralNodesParser, Parser};
use crate::spec::LatexContextDb;
use crate::state::ParsingState;
use crate::token::StringTokenReader;

/// High-level LaTeX parser.
///
/// This is the main interface for parsing LaTeX documents. It manages
/// the source string, token reader, and parsing context.
///
/// # Example
///
/// ```
/// use techy::LatexWalker;
///
/// let source = r"\textbf{Hello} world!";
/// let walker = LatexWalker::new(source.to_string());
/// let ast = walker.parse().unwrap();
///
/// println!("Parsed {} nodes", ast.nodes.len());
/// ```
pub struct LatexWalker {
    /// The source LaTeX code.
    source: String,
    /// The LaTeX context (known macros/environments).
    context: LatexContextDb,
}

impl LatexWalker {
    /// Create a new walker with default context.
    ///
    /// The default context includes standard LaTeX macros and environments.
    pub fn new(source: String) -> Self {
        Self::with_context(source, LatexContextDb::default())
    }

    /// Create a new walker with a custom context.
    pub fn with_context(source: String, context: LatexContextDb) -> Self {
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

    /// Get the LaTeX context.
    pub fn context(&self) -> &LatexContextDb {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let walker = LatexWalker::new("Hello world".to_string());
        let result = walker.parse();
        assert!(result.is_ok());
        
        let nodelist = result.unwrap();
        assert!(!nodelist.is_empty());
    }

    #[test]
    fn test_parse_with_macro() {
        let walker = LatexWalker::new(r"\textbf{bold}".to_string());
        let result = walker.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_with_groups() {
        let walker = LatexWalker::new("{hello} {world}".to_string());
        let result = walker.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_custom_context() {
        let mut context = LatexContextDb::new();
        use crate::spec::MacroSpec;
        context.add_macro(MacroSpec::simple("mycmd", "{"));

        let walker = LatexWalker::with_context(r"\mycmd{test}".to_string(), context);
        let result = walker.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_source_accessor() {
        let walker = LatexWalker::new("test".to_string());
        assert_eq!(walker.source(), "test");
    }
}
