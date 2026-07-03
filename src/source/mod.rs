//! L0 — source management: content, spans, provenance, resolution, line/column analysis.
//!
//! This layer implements the design of `SOURCE_ARCHITECTURE.md` (March 2026) as adopted by
//! `ARCHITECTURE.md` §L0 (July 2026):
//!
//! - [`Source`] owns one unit of source content, its origin metadata, and its
//!   [`SourceProvenance`]. Sources are shared as `Arc<Source>`.
//! - [`SourceSpan`] is an `Arc<Source>` + byte range. Nodes, tokens-turned-diagnostics, and
//!   errors carry `SourceSpan`s, making them self-contained — no lifetime parameters, no
//!   external source store.
//! - [`SourceProvenance`] records where a source came from (`Primary` / `Resolved` /
//!   `Synthesized`), with a `triggered_at: SourceSpan` back-reference forming a provenance
//!   tree walkable for error reporting. Provenance lives on the *source* (one hop per
//!   resolved/synthesized source), not on every location.
//! - [`SourceResolver`] is the pluggable content-lookup extension point (`\input`-like
//!   references); [`NoResolver`] is the zero-cost default.
//! - [`SourceContent`] abstracts the backing storage (in-memory strings today; the trait
//!   boundary is what will later allow memory-mapped files without parser changes).
//! - [`SourceCursor`] provides forward scanning with mark/rewind over source content.
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

pub use content::{SourceContent, SourceCursor};
pub use line_index::LineIndex;
pub use origin::SourceOrigin;
pub use resolver::{MapResolver, NoResolver, ResolveError, SourceResolver};
pub use source::{ProvenanceChain, Source, SourceProvenance, SourceSpan};
pub use span::Span;
