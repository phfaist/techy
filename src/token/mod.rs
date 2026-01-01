//! Token types and tokenization for LaTeX-like markup languages.
//!
//! This module defines the token types that make up LaTeX syntax and provides
//! configuration for tokenization behavior and traits for reading tokens from a source.
//!
//! ## Module Organization
//!
//! - [`token`] - Core token types (`Token`, `TokenType`)
//! - [`tokenizationstate`] - Tokenization configuration (`TokenizationState`)
//! - [`tokenreader`] - Token reader trait (`TokenReader`)

mod token;
mod tokenizationstate;
mod tokenreader;

// Re-export public types
pub use token::{Token, TokenType};
pub use tokenizationstate::{TokenizationState, TokenizationStateBuilder};
pub use tokenreader::TokenReader;
