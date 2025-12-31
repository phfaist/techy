//! # techy
//!
//! A fast, extensible LaTeX-like code parser for Rust.
//!
//! This library provides a parser for LaTeX-like markup languages. It builds an
//! Abstract Syntax Tree (AST) with a syntax inspired by LaTeX, allowing you
//! to analyze, transform, or convert documents in this LaTeX-like language.
//! The latex-like language includes constructs such as macros, environments,
//! special active characters, inline math. The parser should be usable for
//! most LaTeX documents that contain content only.
//!
//! The latex-like language syntax forms the basis of the
//! [`FLM` (Flexible Latex-like Markup) project](https://github.com/phfaist/flm).
//!
//! ## Quick Start
//!
//! ```rust
//! use techy::Parser;
//!
//! let source = r"\textbf{Hello} \emph{world}!";
//! let parser = Parser::new(source.to_string());
//! let ast = parser.parse().unwrap();
//!
//! println!("Parsed {} nodes", ast.nodes.len());
//! ```
//!
//! ## Features
//!
//! - **Fast**: Zero-copy parsing where possible, efficient memory usage
//! - **Extensible**: Define custom macros, environments, and special characters
//! - **Type-safe**: Leverages Rust's type system for correctness
//! - **Flexible**: Support for standard LaTeX and custom LaTeX-like languages
//!
//! ## Architecture
//!
//! The parser follows a three-stage pipeline:
//!
//! 1. **Tokenization** (`token` module): Break source into tokens
//! 2. **Parsing** (`parser` module): Build AST from tokens
//! 3. **Processing** (`node` module): Traverse and manipulate AST
//!
//! ## Modules
//!
//! - [`token`]: Token types and tokenization
//! - [`node`]: AST node definitions
//! - [`parser`]: High-level parsing API
//! - [`constructs`]: Parsers for individual LaTeX constructs
//! - [`spec`]: Macro/environment specifications for extensibility
//! - [`state`]: Parsing state and context management
//! - [`error`]: Error types

//pub mod constructs;
pub mod error;
//pub mod node;
//pub mod parser;
pub mod source;
//pub mod spec;
//pub mod state;
//pub mod token;

// Re-export main types for convenience
//pub use error::{ParseError, Result};
//pub use node::{Arguments, Node, NodeList};
//pub use parser::Parser;
pub use source::{Source, SourceLocation, SourceLocationAnalyzer};
//pub use spec::{ArgumentSpec, ArgumentStructureSpec, ContextDb, EnvironmentSpec, MacroSpec};
//pub use state::{ParsingState, ParsingStateDelta};
//pub use token::{Token, TokenType};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn test_basic_parsing() {
    //     let source = r"Hello world";
    //     let parser = Parser::new(source.to_string());
    //     let result = parser.parse();
    //     assert!(result.is_ok());
    // }

    // #[test]
    // fn test_macro_parsing() {
    //     let source = r"\textbf{bold text}";
    //     let parser = Parser::new(source.to_string());
    //     let result = parser.parse();
    //     assert!(result.is_ok());
    // }
}
