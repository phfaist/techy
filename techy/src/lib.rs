//! # techy
//!
//! A fast, extensible parser toolkit for LaTeX-like markup languages.
//!
//! techy builds an Abstract Syntax Tree (AST) from LaTeX-like source code, allowing you to
//! analyze, transform, or convert documents. The engine has no privileged language concepts
//! (no built-in math mode, `{`/`}`, `%`, or `\`); the familiar LaTeX behavior is provided by
//! a preset, and custom LaTeX-like languages are defined with the same machinery.
//!
//! New to techy? Start with the [`guide`] — the narrative documentation; the modules
//! below are the API reference.
//!
//! ## no_std
//!
//! The crate is `no_std`-friendly: it depends only on `core` and `alloc` (sources are shared
//! as `Arc`, so the target must support atomics). Consequently the library performs no I/O
//! of its own — content lookup for `\input`-like constructs is delegated to the embedder via
//! the [`SourceResolver`](source::SourceResolver) trait.
//!
//! ## The public modules
//!
//! Every item has exactly one canonical public path, placed by role: data models and
//! consumer tool libraries at the top level, the machinery in [`core`], the preset in
//! [`latexlike`]:
//!
//! - [`source`] — source content, plain byte [`Span`](source::Span)s, `Arc`-based
//!   [`SourceSpan`](source::SourceSpan)s, provenance, pluggable resolution, lazy
//!   line/column analysis.
//! - [`error`] — span-based structured diagnostics and the tolerant-parsing policy.
//! - [`extract`] — content-extraction helpers over parsed node trees.
//! - [`transform`] — tree→tree transformation: the streaming restage driver
//!   ([`restage`](transform::restage) + [`RestageVisitor`](transform::RestageVisitor)).
//! - [`visit`] — read-only structural traversal: [`walk`](visit::walk) +
//!   [`NodeVisitor`](visit::NodeVisitor) with enter/exit and per-node flow
//!   control.
//! - [`recompose`] — tree→value recomposition: the meaning-free piece fold
//!   ([`recompose`](recompose::recompose) + [`Recomposer`](recompose::Recomposer));
//!   source re-emission is the preset's
//!   [`source_recomposer`](latexlike::source_recomposer).
//! - [`core`] — the machinery hub: the `Lang` contract and parsing state, tokens,
//!   and the parse engine ([`Language`](core::Language) + `parse()` →
//!   [`ParseResult`](core::ParseResult)), with three satellites:
//!   - [`core::specs`] — defining callables: callable specs, providers, packages
//!     and scopes, command resolution.
//!   - [`core::constructs`] — construct parsing: the
//!     [`ConstructParser`](core::constructs::ConstructParser) contract, the standard
//!     parsers, and their diagnostic conditions.
//!   - [`core::node`] — the flat, immutable node tree: reading, payloads, building.
//! - [`latexlike`] — the familiar LaTeX behavior: the
//!   [`Latexlike`](latexlike::Latexlike) lang with text/math modes, scope-stack
//!   command resolution, default token rules and base specials, environments
//!   (`\begin`/`\end`), verbatim, and `NodeRef` accessor sugar. Preset items are
//!   namespaced (`techy::latexlike::…`).
//!
//! ## Quick start
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

// no_std-friendly, alloc-only ([§dd-dr:dependencies]); tests build with std for convenience.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

// The `techy-derive` macros emit `::techy::__private::…` paths so that generated code
// resolves in downstream crates; this self-alias makes those paths resolve inside techy
// itself.
extern crate self as techy;

// Internal topic modules: private organization, permanently invisible to public paths.
// The public API is exported exclusively through the facade modules below
// ([§dd-dr:public-namespace-topology] — one canonical public path per item).
pub(crate) mod constructs;
pub(crate) mod engine;
pub(crate) mod node;
pub(crate) mod scopes;
pub(crate) mod spec;
pub(crate) mod state;
pub(crate) mod token;

// The public facades. `source` and `error` are their own facades (their submodules are
// private); `extract`, `transform`, `visit`, `recompose`, and `latexlike` are ordinary
// public modules.
pub mod core;
pub mod error;
pub mod extract;
pub mod latexlike;
pub mod recompose;
pub mod source;
pub mod transform;
pub mod visit;

// Narrative documentation: markdown pages in the workspace-level `docs/` rendered as
// doc-only modules. `cfg(doc)` keeps them out of compiled code; rustdoc (including
// doctest collection) builds with `--cfg doc`, so code blocks in these pages still run
// as doctests. The docs sidebar pins these pages in a "Guide" section
// (docs/rustdoc-header.html, wired up in .cargo/config.toml); new chapters must also be
// added to GUIDE_PAGES there.
#[cfg(doc)]
#[doc = include_str!("../../docs/guide.md")]
pub mod guide {
    // User Guide.
    #[doc = include_str!("../../docs/introduction.md")]
    pub mod introduction {}

    #[doc = include_str!("../../docs/language-syntax.md")]
    pub mod language_syntax {}

    #[doc = include_str!("../../docs/node-trees.md")]
    pub mod node_trees {}

    #[doc = include_str!("../../docs/specs.md")]
    pub mod specs {}

    #[doc = include_str!("../../docs/parsing.md")]
    pub mod parsing {}

    #[doc = include_str!("../../docs/learn-by-example.md")]
    pub mod learn_by_example {}

    // Developer Guide.
    #[doc = include_str!("../../docs/concepts-overview.md")]
    pub mod concepts_overview {}

    #[doc = include_str!("../../docs/parsing-model.md")]
    pub mod parsing_model {}

    #[doc = include_str!("../../docs/construct-parsers.md")]
    pub mod construct_parsers {}

    #[doc = include_str!("../../docs/custom-lang.md")]
    pub mod custom_lang {}

    #[doc = include_str!("../../docs/integration.md")]
    pub mod integration {}

    #[doc = include_str!("../../docs/pylatexenc-migration.md")]
    pub mod pylatexenc_migration {}

    // AI Guide.
    #[doc = include_str!("../../docs/ai-guide.md")]
    pub mod ai_guide {}

    #[doc = include_str!("../../docs/ai-guide-definitions.md")]
    pub mod ai_guide_definitions {}

    #[doc = include_str!("../../docs/ai-guide-trees.md")]
    pub mod ai_guide_trees {}

    #[doc = include_str!("../../docs/ai-guide-custom-lang.md")]
    pub mod ai_guide_custom_lang {}

    #[doc = include_str!("../../docs/ai-guide-embedding.md")]
    pub mod ai_guide_embedding {}

    #[doc = include_str!("../../docs/ai-guide-pylatexenc.md")]
    pub mod ai_guide_pylatexenc {}
}

/// Support module for `techy-derive`-generated code only: everything the generated code
/// references — `alloc` paths spelled so they resolve from both `std` and `no_std`
/// consumer crates, plus the diagnostics items the derives implement/construct. The
/// derives emit only `::techy::__private::…` paths (the serde discipline), so the
/// public topology never constrains, and is never constrained by, derive output.
/// Not public API.
#[doc(hidden)]
pub mod __private {
    pub use alloc::string::String;
    pub use alloc::vec::Vec;

    pub use crate::error::{DiagnosticInfo, DiagnosticValue, ToDiagnosticValue};
}

/// The version of the `techy` Cargo package (`CARGO_PKG_VERSION`); always a valid
/// [semver](https://semver.org/) string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
