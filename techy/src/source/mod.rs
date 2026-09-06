//! The source model: content, byte ranges, provenance, resolution, and line/column
//! analysis.
//!
//! Everything a parse produces points back into a [`Source`]: one unit of content — a
//! document, an included file, or a snippet synthesized while parsing. Sources are
//! immutable once created, and are shared as `Arc<Source>`.
//!
//! Two types name a stretch of text inside a source, and the difference between them
//! decides which one an API asks for:
//!
//! - [`Span`] is a plain `Copy` byte range that does not say *which* source it belongs
//!   to. Tokens and scanning code use it, and all span arithmetic happens on it.
//! - [`SourceSpan`] is a byte range plus an `Arc` to the source it points into. Nodes
//!   and diagnostics store `SourceSpan`s, which makes them self-contained: no lifetime
//!   parameter, and no external table of sources to look a location up in.
//!
//! [`SourceSpan::new`] turns a `Span` into a `SourceSpan` and [`SourceSpan::span`] turns
//! it back; construct parsers do the first as they build nodes. [`SourcePos`] is the
//! single-offset counterpart of `SourceSpan`, used to query a parsed tree by location,
//! and [`TextContent`] is the logical text a node payload stores — a byte range when it
//! came from parsing, an owned string when it was synthesized or normalized.
//!
//! Every source records a [`SourceProvenance`]: primary input, content resolved from an
//! external reference, or content synthesized during the parse. The latter two point
//! back at the location that caused them, so the include chain of any location can be
//! recovered for error reporting — [`Source::provenance_chain`] yields the provenance
//! records, [`Source::including_sources`] the sources themselves.
//!
//! [`SourceResolver`] is the extension point that turns an `\input`-like reference into
//! content; a parse driver with no resolver configured resolves nothing. Bounding
//! include recursion is the embedder's decision, and [`check_include_chain`] implements
//! the usual cycle-and-depth policy.
//!
//! Line and column numbers are computed on demand, for display only — parsing itself
//! works purely in byte offsets. [`LineIndex`] is the borrowing index over one content
//! string, [`LineIndexCache`] the persistent per-source form a consumer keeps across
//! renders, and [`LineColProvider`] the trait the diagnostic rendering entry points
//! accept.
//!
//! For how these types fit into a parse, see [the concepts
//! overview](crate::guide::concepts_overview#sources-and-spans); for position queries
//! and line/column handling in editors and other tools, see [the integration
//! chapter](crate::guide::integration#tooling-starting-points).
//!
//! # Sources never reference nodes
//!
//! `Source`, `SourceSpan`, and `SourceProvenance` may reference other sources, and
//! nothing else. The reference graph is therefore strictly layered — nodes point at
//! sources, sources point at sources — which makes `Arc` cycles impossible to build out
//! of these types.
//!
//! # The origin type parameter
//!
//! Where a source's content nominally comes from is described by a plain type parameter
//! `O: `[`SourceOrigin`], defaulting to `Option<String>` (conventionally the URL the
//! content was obtained from, `None` when unknown or synthesized). Higher layers
//! substitute a language's own origin type for it; this module never depends on the
//! language type itself, which is what keeps the layering strict.
//!
//! # no_std
//!
//! Like the rest of the crate, this module uses only `core` and `alloc`. In particular
//! it ships no file-system-backed resolver: an embedder that wants file, URL, or
//! database lookup implements [`SourceResolver`] itself.

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
