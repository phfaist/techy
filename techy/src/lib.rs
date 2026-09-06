//! # techy
//!
//! A fast, extensible parser toolkit for LaTeX-like markup languages.
//!
//! techy reads marked-up source text — LaTeX itself, or any language built from similar
//! ingredients — and produces a *node tree*: an Abstract Syntax Tree (AST) your program
//! can analyze, transform, or convert to another format. The parsing engine has no
//! built-in LaTeX behavior; the familiar meaning of `\`, `{`, `}`, `%`, math modes, and
//! environments comes from the [`latexlike`] preset, which is built entirely from the
//! same public extension points you would use to define a language of your own.
//!
//! ## Quick start
//!
//! Parse a document with the `latexlike` preset and read what came out. Definitions are
//! supplied by the embedder — techy ships no LaTeX definitions database — so the example
//! first registers the one macro it uses.
//!
//! ```rust
//! use techy::core::specs::Package;
//! use techy::core::{Language, ParsingState};
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//!
//! // `\cite` takes an optional `[…]` argument, then a mandatory `{…}` one.
//! let mut package: Package<Latexlike> = Package::new("mydefs");
//! package.define_macro("cite", ["o", "m"]).unwrap();
//!
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial_with_packages([package]).expect("seed state"),
//! );
//! let result = language.parse(r"see \cite[Lemma 3]{Author}!").unwrap();
//!
//! // The root node lists the top-level content; its second child is the invocation.
//! let cite = result.tree.root().child(1).unwrap();
//! assert_eq!(cite.macro_name(), Some("cite"));
//! assert_eq!(cite.span_content(), r"\cite[Lemma 3]{Author}");
//!
//! // Each argument records whether it was provided and where its content nodes are.
//! let author = cite.argument_content_nodes(1).unwrap();
//! assert_eq!(author.source_text(), Some("Author"));
//! ```
//!
//! ## Where to go next
//!
//! New to techy? The [`guide`] is the narrative documentation; the modules listed below
//! are the API reference. [Learn techy by example](guide::learn_by_example) covers
//! parsing, definitions, and the tree-consumer tools in one page, and
//! [Introduction](guide::introduction) explains what the library is for and how the
//! documentation is organized.
//!
//! ## The public modules
//!
//! Every item has exactly one canonical public path, placed by role: the data models
//! and the tree-consumer tools at the top level, the parsing machinery in [`core`], and
//! the ready-made language in [`latexlike`].
//!
//! - [`source`] — the source model: source content, plain byte ranges
//!   ([`Span`](source::Span)), ranges that also name their source
//!   ([`SourceSpan`](source::SourceSpan)), the provenance of included content,
//!   pluggable source resolution, and line/column analysis computed on demand.
//! - [`error`] — structured diagnostics tied to source spans, and the policy that
//!   decides whether a parse stops at the first problem or recovers and continues.
//! - [`extract`] — helpers that pull content out of a parsed node tree, such as
//!   splitting a node run at chosen characters or reading a key/value list.
//! - [`visit`] — read-only traversal of a node tree:
//!   [`TreeWalker`](visit::TreeWalker) drives a [`NodeVisitor`](visit::NodeVisitor),
//!   which is called on entering and on leaving each node and controls where the walk
//!   goes next.
//! - [`transform`] — tree-to-tree transformation:
//!   [`TreeRestager`](transform::TreeRestager) reads an existing tree and asks a
//!   [`RestageVisitor`](transform::RestageVisitor), node by node, what the new tree
//!   should contain.
//! - [`recompose`] — tree-to-value recomposition: a
//!   [`Recomposer`](recompose::Recomposer) returns one instruction per node — emit this
//!   value, or combine the children's values — and
//!   [`TreeRecomposer`](recompose::TreeRecomposer) combines them into a single result.
//!   Re-emitting the original source is one such recomposer, the preset's
//!   [`source_recomposer`](latexlike::source_recomposer).
//! - [`serialize`] — serialization to and from a format-independent value model
//!   ([`SerialValue`](serialize::SerialValue)). Types opt in through the write and read
//!   traits — [`SerializableObject`](serialize::SerializableObject) and
//!   [`DeserializableObject`](serialize::DeserializableObject) for objects stored in a
//!   table, [`SerializableValue`](serialize::SerializableValue) and
//!   [`DeserializableValue`](serialize::DeserializableValue) for values embedded in
//!   place — and a language declares its participation with
//!   [`SerializableLang`](serialize::SerializableLang). A
//!   [`SerdeSession`](serialize::SerdeSession) holds the standard tables of sources,
//!   parsing states, specs, providers, trees, diagnostics, and parse results.
//! - [`core`] — the parsing machinery: the `Lang` contract that defines a language, the
//!   parsing state, and the parse engine ([`Language`](core::Language) and its
//!   `parse()`, producing a [`ParseResult`](core::ParseResult)). It has four
//!   submodules:
//!   - [`core::token`] — tokenization: a language declares its tokenization as one
//!     type, which names the token types, the [`TokenReader`](core::token::TokenReader)
//!     trait and the standard reader, the [`TokenRules`](core::token::TokenRules) data,
//!     and the token errors.
//!   - [`core::specs`] — defining callables: the specs that describe a macro,
//!     environment, or specials; the providers and packages that hold them; and the
//!     scopes that resolve a name to a spec.
//!   - [`core::constructs`] — parsing individual constructs: the
//!     [`ConstructParser`](core::constructs::ConstructParser) contract, the standard
//!     parsers, and the conditions they report.
//!   - [`core::node`] — the flat, immutable node tree: reading it, its per-kind
//!     payloads, and building one.
//! - [`latexlike`] — the ready-made LaTeX-like language: the
//!   [`Latexlike`](latexlike::Latexlike) language with text and math modes, command
//!   resolution through a stack of scopes, the default token rules and base specials,
//!   environments (`\begin`/`\end`), verbatim content, and convenience accessors on
//!   node references for the preset's own constructs. Every preset item is namespaced
//!   under `techy::latexlike`.
//!
//! ## Dependencies, features, and the panic policy
//!
//! **no_std and alloc.** The crate depends on `core` and `alloc` only. Several objects
//! are shared as `Arc`, so the target must support atomics. The crate performs no
//! input/output of its own: content lookup (for `\input`, if the language declares it)
//! is delegated to traits the embedder implements. It therefore builds for WebAssembly
//! targets such as `wasm32-unknown-unknown`, where the host supplies all input.
//!
//! **Minimal dependencies.** The crate pulls in as few runtime dependencies as
//! possible: currently only `hashbrown`, plus `serde` when that feature is enabled.
//! A few build-time dependencies implement the derive macros.
//!
//! **The panic policy.** This library never panics on document input. A small set of
//! public methods and functions may panic on a caller contract violation, following
//! standard Rust patterns such as unguarded index accessors; the guide chapter
//! [Panics](guide::panics) is the complete list of public items that can panic.
//!
//! **Cargo features.**
//! - `serde` (off by default) — renders techy's serialized values (node trees, for
//!   instance) through [serde](https://serde.rs). See [`serialize`] and the guide
//!   chapter [Serialization](guide::serialize).

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

    #[doc = include_str!("../../docs/serialize.md")]
    pub mod serialize {}

    #[doc = include_str!("../../docs/panics.md")]
    pub mod panics {}

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

    #[doc = include_str!("../../docs/ai-guide-serialize.md")]
    pub mod ai_guide_serialize {}

    #[doc = include_str!("../../docs/ai-guide-pylatexenc.md")]
    pub mod ai_guide_pylatexenc {}
}

/// Support module for generated code only. Not public API.
///
/// It re-exports everything the expansions of `techy-derive`'s derives and of the
/// `serial_index!` macro refer to: `alloc` paths spelled so they resolve from both `std`
/// and `no_std` consumer crates; the diagnostics items the condition derives implement
/// and construct; the contexts, errors, and field and variant helpers the value derives'
/// generated bodies use; and the wire conversion traits and helpers a typed table
/// position implements.
///
/// The derives and the macro emit only `::techy::__private::…` / `$crate::__private::…`
/// paths, as serde does, so the public module topology never constrains, and is never
/// constrained by, generated output.
#[doc(hidden)]
pub mod __private {
    pub use alloc::string::String;
    pub use alloc::vec::Vec;

    pub use crate::error::{DiagnosticInfo, DiagnosticValue, ToDiagnosticValue};

    // The `SerializableValue` / `DeserializableValue` derives' expansions: the
    // contexts, errors, and value type of the method signatures, and the
    // field/variant helpers the generated bodies call. The traits themselves
    // (`SerializableValue`, `DeserializableValue`, `SerializableLang`, `Lang`) are
    // deliberately NOT re-exported here: adding a `#[doc(hidden)]` import path to a
    // public trait makes cargo-semver-checks treat the trait as sealed (reported as
    // "newly sealed" against the baseline, and its later changes then checked under
    // a sealed trait's weaker rules); the generated code names them by their
    // canonical public paths instead. `DiagnosticInfo` / `ToDiagnosticValue` above
    // predate this finding and are already counted as sealed on the baseline; routing
    // the condition derives through `::techy::error::…` is a follow-up (TODO_Big.md).
    pub use crate::serialize::wire::{
        data_variant, expect_data_variant, expect_unit_variant, read_variant, unit_variant,
        unknown_variant, FieldReader, FieldWriter,
    };
    pub use crate::serialize::{
        DeserializeContext, DeserializeError, SerialValue, SerializeContext, SerializeError,
    };

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
