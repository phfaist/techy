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
//! as `Arc`, so the target must support atomics). Consequently the library performs no
//! input/output of its own — content lookup for `\input`-like constructs is delegated to the
//! embedder via the [`SourceResolver`](source::SourceResolver) trait. In particular the
//! crate builds for WebAssembly targets such as `wasm32-unknown-unknown`, where the host
//! supplies all input.
//!
//! ## Cargo features
//!
//! - `serde` (off by default) — enables the optional [serde](https://serde.rs)
//!   dependency for the rendering layer of [`serialize`]: `Serialize`/`Deserialize`
//!   impls for [`SerialValue`](serialize::SerialValue), so serialized values encode
//!   through any serde format (JSON is the format the public rendering is stated in),
//!   and the bridge `serialize::to_value` / `serialize::from_value`, which converts any
//!   type implementing serde's traits to and from a `SerialValue`. The serialization
//!   capability itself — the value model and the capability traits — is always present
//!   and dependency-free; the feature adds only rendering, and adds no obligation to
//!   any implementer.
//!
//! ## Panics
//!
//! Parsing never panics on document input: problems in the parsed content surface as
//! diagnostics or as an `Err` (see [`error`]), and every fallible operation of the API
//! returns a `Result`. The panicking items of the public API are exactly the two
//! families below; those panics guard against programming errors in calling code — no
//! document content can trigger them.
//!
//! **Precondition asserts.** Six value functions document a precondition on their
//! arguments and panic, in all builds, when calling code violates it. These functions
//! are deliberately infallible (there is no error channel to prefer), the checks are
//! cheap, and the immediate panic keeps invalid values unrepresentable instead of
//! letting them cause misbehavior far from the mistake:
//!
//! - [`Span::new`](source::Span::new) — requires `start <= end`;
//! - [`Span::extend_to`](source::Span::extend_to) — requires the new end not to
//!   precede the span's current end;
//! - [`SourceSpan::new`](source::SourceSpan::new) — requires the range to lie within
//!   the source content, on `char` boundaries;
//! - [`SourcePos::new`](source::SourcePos::new) — requires the offset to lie within
//!   the source content, on a `char` boundary;
//! - [`Token::new`](core::Token::new) — requires the documented coherence of the
//!   token's spans;
//! - [`skip_whitespace`](core::skip_whitespace) — requires `pos` to lie within the
//!   content, on a `char` boundary.
//!
//! **Indexing-style accessors.** Accessors that follow the standard library's
//! slice-indexing convention: the panicking form is for ids, spans, and regions
//! obtained from the very tree or source in hand, and each panic is stated in a
//! "Panics" section on the item's own page; for values of unknown provenance, use
//! the non-panicking companion:
//!
//! - [`NodeTree::node`](core::node::NodeTree::node) — panics on an id another tree
//!   minted (the non-panicking companion is
//!   [`NodeTree::get`](core::node::NodeTree::get));
//! - [`NodeTree::nodes_in`](core::node::NodeTree::nodes_in) — panics on a range
//!   outside the tree's storage;
//! - [`Span::slice`](source::Span::slice) — panics on a span invalid for the given
//!   content (the non-panicking companion is [`Span::get`](source::Span::get));
//! - [`TextContent::resolve`](source::TextContent::resolve) — panics on a stored
//!   range invalid for the given source's content (a broken invariant, not
//!   document input);
//! - [`ChildRegion::children`](core::node::ChildRegion::children),
//!   [`ChildRegion::content_range`](core::node::ChildRegion::content_range), and
//!   [`ChildRegion::content_parent`](core::node::ChildRegion::content_parent) —
//!   panic on a staged (never finished) region, which no finished tree can contain
//!   (guard with [`ChildRegion::is_resolved`](core::node::ChildRegion::is_resolved);
//!   the non-panicking companion answering the staged coordinates is
//!   [`ChildRegion::staged`](core::node::ChildRegion::staged)).
//!
//! These two families are the complete list of documented panics in the public API;
//! no other public item panics on documented use.
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
//!   ([`TreeRestager`](transform::TreeRestager) +
//!   [`RestageVisitor`](transform::RestageVisitor)).
//! - [`visit`] — read-only structural traversal:
//!   [`TreeWalker`](visit::TreeWalker) +
//!   [`NodeVisitor`](visit::NodeVisitor) with enter/exit and per-node flow
//!   control.
//! - [`recompose`] — tree→value recomposition: the meaning-free piece fold
//!   ([`TreeRecomposer`](recompose::TreeRecomposer) +
//!   [`Recomposer`](recompose::Recomposer));
//!   source re-emission is the preset's
//!   [`source_recomposer`](latexlike::source_recomposer).
//! - [`serialize`] — serialization to and from a format-independent value model
//!   ([`SerialValue`](serialize::SerialValue)): the write/read capability traits
//!   ([`SerializableObject`](serialize::SerializableObject) +
//!   [`DeserializableObject`](serialize::DeserializableObject) for table objects,
//!   [`SerializableValue`](serialize::SerializableValue) +
//!   [`DeserializableValue`](serialize::DeserializableValue) for embedded values),
//!   the [`SerializableLang`](serialize::SerializableLang) declaration, the session
//!   ([`SerdeSession`](serialize::SerdeSession)) with the standard tables of
//!   sources, states, specs, providers, trees, diagnostics, and parse results.
//! - [`core`] — the machinery hub: the `Lang` contract and parsing state, tokens,
//!   and the parse engine ([`Language`](core::Language) + `parse()` →
//!   [`ParseResult`](core::ParseResult)), with three submodules:
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

// The public facades. `source`, `error`, and `serialize` are their own facades (their
// submodules are private); `extract`, `transform`, `visit`, `recompose`, and
// `latexlike` are ordinary public modules.
pub mod core;
pub mod error;
pub mod extract;
pub mod latexlike;
pub mod recompose;
pub mod serialize;
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

/// Support module for generated code only — `techy-derive`'s derives and the
/// `serial_index!` macro: everything the generated code references — `alloc` paths
/// spelled so they resolve from both `std` and `no_std` consumer crates, the
/// diagnostics items the derives implement/construct, and the serialization
/// conversion traits and helpers a typed table position implements. The derives and
/// the macro emit only `::techy::__private::…` / `$crate::__private::…` paths (the
/// serde discipline), so the public topology never constrains, and is never
/// constrained by, generated output. Not public API.
#[doc(hidden)]
pub mod __private {
    pub use alloc::string::String;
    pub use alloc::vec::Vec;

    pub use crate::error::{DiagnosticInfo, DiagnosticValue, ToDiagnosticValue};

    // The `serial_index!` macro's expansion: the wire conversion traits it implements
    // and the helpers it calls; with the `serde` feature, serde itself (a downstream
    // crate need not depend on serde to define a position type) and the index
    // sentinel helpers.
    pub use crate::serialize::wire::{index_from_serial_value, FromSerialValue, ToSerialValue};
    #[cfg(feature = "serde")]
    pub use crate::serialize::bridge::{deserialize_index, serialize_index};
    #[cfg(feature = "serde")]
    pub use serde;
}

/// The version of the `techy` Cargo package (`CARGO_PKG_VERSION`); always a valid
/// [semver](https://semver.org/) string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
