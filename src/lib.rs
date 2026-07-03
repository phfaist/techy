//! # techy
//!
//! A fast, extensible parser toolkit for LaTeX-like markup languages.
//!
//! techy builds an Abstract Syntax Tree (AST) from LaTeX-like source code, allowing you to
//! analyze, transform, or convert documents. The engine has no privileged language concepts
//! (no built-in math mode, `{`/`}`, `%`, or `\`); the familiar LaTeX behavior is provided by
//! a preset, and custom LaTeX-like languages are defined with the same machinery.
//!
//! ## no_std
//!
//! The crate is `no_std`-friendly: it depends only on `core` and `alloc` (sources are shared
//! as `Arc`, so the target must support atomics). Consequently the library performs no I/O
//! of its own — content lookup for `\input`-like constructs is delegated to the embedder via
//! the [`SourceResolver`] trait.
//!
//! ## Architecture
//!
//! The crate is organized in three strata (see `ARCHITECTURE.md` §3): S0, a Lang-free
//! foundation; S1, the mutually-recursive core (whose modules are topics, not dependency
//! ranks); and S2, the presets. It is being rebuilt phase by phase:
//!
//! - [`source`] (S0) — source content, `Arc`-based spans, provenance, pluggable resolution,
//!   lazy line/column analysis. **Implemented (Phase 1).**
//! - [`error`] — span-based diagnostics and the tolerant-parsing policy.
//!   **Implemented (Phase 1).**
//! - [`token`] — zero-copy tokens, tokenization rules, the standard rules-driven reader
//!   (S0); the `TokenReader<L>` trait follows in Phase 3. **Implemented (Phase 2).**
//! - `state` (S1) — parsing state and reified state deltas. *Phase 3.*
//! - `spec` + `library` (S1) — callable specs and definition libraries. *Phase 4.*
//! - `node` (S1) — the flat, immutable node tree. *Phase 5.*
//! - `constructs` + `engine` (S1) — construct parsers and the high-level API. *Phase 6.*
//! - `latexlike` preset (S2) — the familiar LaTeX behavior. *Phase 7.*
//!
//! ## Quick start (what exists today)
//!
//! ```rust
//! use std::sync::Arc;
//! use techy::source::{Source, SourceSpan};
//!
//! let source: Arc<Source> = Arc::new(Source::new(r"Hello \world{}!"));
//! let span = SourceSpan::new(&source, 6..12);
//! assert_eq!(span.content(), r"\world");
//!
//! // Line/column information is computed lazily, for display only:
//! let mut line_index = source.line_index();
//! assert_eq!(line_index.line_col(span.start()), Some((1, 7)));
//! ```

// no_std-friendly, alloc-only (see ARCHITECTURE.md); tests build with std for convenience.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod error;
pub mod source;
pub mod token;

// The remaining modules of the previous exploratory implementation (`state`, `parser`,
// `constructs`, `node`, `spec`) are kept in the tree as a quarry but are not compiled;
// they are rebuilt phase-by-phase per ARCHITECTURE.md §9.

// Re-export the public API of the implemented topics.
pub use error::{format_position, format_traceback, Diagnostic, Diagnostics, Recovery, Severity};
pub use source::{
    LineIndex, MapResolver, NoResolver, ResolveError, Source, SourceContent, SourceCursor,
    SourceOrigin, SourceProvenance, SourceResolver, SourceSpan,
};
pub use token::{
    CommentRules, GroupType, GroupTypeId, MacroRules, PrefixTable, Span, StdTokenReader, Token,
    TokenError, TokenErrorKind, TokenKind, TokenRecovery, TokenResult, TokenRules,
    WhitespaceRules,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
