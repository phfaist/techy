//! Source management: content, spans, provenance, resolution, line/column analysis.
//!
//! This module provides:
//!
//! - [`Source`] owns one unit of source content, its origin metadata, and its
//!   [`SourceProvenance`]. Sources are shared as `Arc<Source>`.
//! - [`Span`] is a plain `Copy` byte range — the transient span type on which all span
//!   arithmetic rests. Tokens and readers carry only `Span`s (deliberately; the
//!   type lives here rather than in the token topic because errors use it independently
//!   of tokenization).
//! - [`SourceSpan`] is an `Arc<Source>` + byte range. Nodes and errors/diagnostics carry
//!   `SourceSpan`s, making them self-contained — no lifetime parameters, no external
//!   source store. The construct-parser layer is where a byte `Span` becomes a
//!   `SourceSpan`, via its `ParseContext`'s source.
//! - [`SourcePos`] is an `Arc<Source>` + single byte offset — the point counterpart
//!   of `SourceSpan` (position lookups over parsed trees query with it).
//! - [`SourceProvenance`] records where a source came from (`Primary` / `Resolved` /
//!   `Synthesized`), with a `triggered_at: SourceSpan` back-reference forming a provenance
//!   tree walkable for error reporting. Provenance lives on the *source* (one hop per
//!   resolved/synthesized source), not on every location.
//! - [`SourceResolver`] is the pluggable content-lookup extension point (`\input`-like
//!   references), configured on a parse driver through the sealed
//!   [`IntoSourceResolver`] conversion; an unconfigured driver resolves nothing.
//!   Recursion/cycle policy stays the embedder's — [`Source::including_sources`]
//!   and [`check_include_chain`] are the ready-made policy tools.
//! - [`TextContent`] is logical textual content — span-backed when it came from parsing,
//!   owned when synthesized or normalized. Node payloads carry it.
//! - [`LineIndex`] computes line/column information lazily, for display only — parsing works
//!   purely in byte offsets. [`LineIndexCache`] is its persistent, per-source
//!   consumer-held form, and [`LineColProvider`] the trait the rendering entry
//!   points accept.
//!
//! # Cycle-prevention invariant
//!
//! **Source types never reference node types.** `Source`, `SourceSpan`, and
//! `SourceProvenance` may only reference other sources. The reference graph is strictly
//! layered (nodes → sources; sources → sources), which makes `Arc` cycles impossible by
//! construction.
//!
//! # Genericity note
//!
//! The origin metadata type is a plain type parameter `O: SourceOrigin` with the default
//! `Option<String>` (conventionally the URL the content was obtained from, `None` when
//! unknown or synthesized). The higher layers plug `L::SourceOrigin` into this parameter; the source
//! layer itself never depends on `Lang`, preserving the strict layering.
//!
//! # no_std
//!
//! This layer, like the whole crate, is `no_std`-friendly (it uses `core` and `alloc` only).
//! In particular there is no file-system-backed resolver: embedders that want file (or URL,
//! or database) lookup implement [`SourceResolver`] themselves.

mod line_index;
mod origin;
mod resolver;
// The submodule sharing the parent's name is deliberate: `Source` is this layer's anchor
// type, and the submodule is private (everything is re-exported here).
#[allow(clippy::module_inception)]
mod source;
mod span;
mod text_content;

pub use line_index::{LineColProvider, LineIndex, LineIndexCache};
pub use origin::SourceOrigin;
pub use resolver::{
    check_include_chain, resolve_source_reference, IntoSourceResolver, MapResolver,
    ResolveError, ResolvedContent, SourceResolver,
};
pub use source::{
    IncludingSources, ProvenanceChain, Source, SourcePos, SourceProvenance, SourceSpan,
};
pub use span::Span;
pub use text_content::TextContent;
