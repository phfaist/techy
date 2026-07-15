//! S0 — source management: content, spans, provenance, resolution, line/column analysis.
//!
//! This stratum implements `ARCHITECTURE.md` §source and `DESIGN_RATIONALE.md` §3.1:
//!
//! - [`Source`] owns one unit of source content, its origin metadata, and its
//!   [`SourceProvenance`]. Sources are shared as `Arc<Source>`.
//! - [`Span`] is a plain `Copy` byte range — the transient span type on which all span
//!   arithmetic rests. Tokens and readers carry only `Span`s (deliberately, §3.8; the
//!   type lives here rather than in the token topic because errors and cursors use it
//!   independently of tokenization).
//! - [`SourceSpan`] is an `Arc<Source>` + byte range. Nodes and errors/diagnostics carry
//!   `SourceSpan`s, making them self-contained — no lifetime parameters, no external
//!   source store. The construct-parser layer is where a byte `Span` becomes a
//!   `SourceSpan`, via its `ParseContext`'s source.
//! - [`SourceProvenance`] records where a source came from (`Primary` / `Resolved` /
//!   `Synthesized`), with a `triggered_at: SourceSpan` back-reference forming a provenance
//!   tree walkable for error reporting. Provenance lives on the *source* (one hop per
//!   resolved/synthesized source), not on every location.
//! - [`SourceResolver`] is the pluggable content-lookup extension point (`\input`-like
//!   references); [`NoResolver`] is the zero-cost default.
//! - [`SourceContent`] abstracts the backing storage (in-memory strings today; the trait
//!   boundary is what will later allow memory-mapped files without parser changes).
//! - [`SourceCursor`] provides forward scanning with mark/rewind over source content.
//! - [`TextContent`] is logical textual content — span-backed when it came from parsing,
//!   owned when synthesized or normalized. Node payloads carry it (Phase 5).
//! - [`LineIndex`] computes line/column information lazily, for display only — parsing works
//!   purely in byte offsets.
//!
//! # Cycle-prevention invariant
//!
//! **Source types never reference node types.** `Source`, `SourceSpan`, and
//! `SourceProvenance` may only reference other sources. The reference graph is strictly
//! layered (nodes → sources; sources → sources), which makes `Arc` cycles impossible by
//! construction.
//!
//! # Genericity note (Phase 1)
//!
//! The origin metadata type is a plain type parameter `O: SourceOrigin` with the default
//! `Option<String>` (conventionally the URL the content was obtained from, `None` when
//! unknown or synthesized). When the `Lang` trait arrives (Phase 3+), `L::SourceOrigin` will
//! be plugged into this parameter by the higher layers; L0 itself never depends on `Lang`,
//! preserving the strict layering of ARCHITECTURE.md §3.
//!
//! # no_std
//!
//! This layer, like the whole crate, is `no_std`-friendly (it uses `core` and `alloc` only).
//! In particular there is no file-system-backed resolver: embedders that want file (or URL,
//! or database) lookup implement [`SourceResolver`] themselves.

mod content;
mod line_index;
mod origin;
mod resolver;
// The submodule sharing the parent's name is deliberate: `Source` is this layer's anchor
// type, and the submodule is private (everything is re-exported here).
#[allow(clippy::module_inception)]
mod source;
mod span;
mod text_content;

pub use content::{SourceContent, SourceCursor};
pub use line_index::LineIndex;
pub use origin::SourceOrigin;
pub use resolver::{
    resolve_source, MapResolver, NoResolver, ResolveError, ResolvedContent, SourceResolver,
};
pub use source::{ProvenanceChain, Source, SourceProvenance, SourceSpan};
pub use span::Span;
pub use text_content::TextContent;
