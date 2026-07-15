//! `Source`, `SourceSpan`, and `SourceProvenance` — the Arc-based source model.

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;
use core::ops::Range;

use super::line_index::LineIndex;
use super::origin::SourceOrigin;
use super::span::Span;

/// One unit of source content (a document, an included file, a synthesized snippet).
///
/// Sources are shared as `Arc<Source>`; every [`SourceSpan`] carries such an `Arc`, making
/// spans (and everything that stores them) self-contained. Sources are immutable once
/// created.
///
/// The content backing is a plain `String`, deliberately (July 2026, Action 06; retired
/// the earlier `SourceContent` trait seam): a memory-mapped UTF-8 file can be handed in
/// as text by the embedder after one validation pass, and a genuinely chunked/streaming
/// backing would need a different reader design — not a backing swap behind this type.
pub struct Source<O: SourceOrigin = Option<String>> {
    /// The source text.
    content: String,
    /// Display metadata: where this content nominally comes from (with the default origin
    /// type, the URL it was obtained from, if available).
    origin: O,
    /// Structural record of how this source entered the parse.
    provenance: SourceProvenance<O>,
    /// Line number offset (default 1 for 1-indexed line numbers; use 0 for 0-indexed).
    line_number_offset: usize,
    /// Column number offset (default 1 for 1-indexed column numbers; use 0 for 0-indexed).
    column_number_offset: usize,
}

impl<O: SourceOrigin> Source<O> {
    /// Create a primary source (top-level content provided directly by the user).
    ///
    /// Defaults: unknown origin, [`SourceProvenance::Primary`], line/column offsets `(1, 1)`.
    pub fn new(content: impl Into<String>) -> Self {
        Source {
            content: content.into(),
            origin: O::default(),
            provenance: SourceProvenance::Primary,
            line_number_offset: 1,
            column_number_offset: 1,
        }
    }

    /// Create a source whose content was resolved from an external `reference`
    /// (e.g. by an `\input`-like construct at `triggered_at`).
    ///
    /// The origin is left at its default; a creator that knows where the content was
    /// obtained from (conventionally a URL) attaches it via [`with_origin`](Self::with_origin).
    pub fn resolved(
        content: impl Into<String>,
        reference: impl Into<String>,
        triggered_at: SourceSpan<O>,
    ) -> Self {
        Source {
            provenance: SourceProvenance::Resolved { reference: reference.into(), triggered_at },
            ..Source::new(content)
        }
    }

    /// Create a source whose content was synthesized during parsing (e.g. a macro
    /// expansion triggered at `triggered_at`).
    ///
    /// The origin is left at its default (no origin: the content was not *obtained* from
    /// anywhere); the provenance carries `description`.
    pub fn synthesized(
        content: impl Into<String>,
        description: impl Into<String>,
        triggered_at: SourceSpan<O>,
    ) -> Self {
        Source {
            provenance: SourceProvenance::Synthesized {
                description: description.into(),
                triggered_at,
            },
            ..Source::new(content)
        }
    }

    /// Set the origin (conventionally the URL the content was obtained from) for this source.
    pub fn with_origin(mut self, origin: O) -> Self {
        self.origin = origin;
        self
    }

    /// Set the provenance for this source.
    pub fn with_provenance(mut self, provenance: SourceProvenance<O>) -> Self {
        self.provenance = provenance;
        self
    }

    /// Set the line and column number offsets.
    ///
    /// Default offsets are `(1, 1)` for 1-indexed line/column numbers; use `(0, 0)` for
    /// 0-indexed numbers.
    pub fn with_line_column_number_offsets(
        mut self,
        line_number_offset: usize,
        column_number_offset: usize,
    ) -> Self {
        self.line_number_offset = line_number_offset;
        self.column_number_offset = column_number_offset;
        self
    }

    /// The source text.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// The origin metadata.
    pub fn origin(&self) -> &O {
        &self.origin
    }

    /// How this source entered the parse.
    pub fn provenance(&self) -> &SourceProvenance<O> {
        &self.provenance
    }

    /// The line number offset (see [`with_line_column_number_offsets`](Self::with_line_column_number_offsets)).
    pub fn line_number_offset(&self) -> usize {
        self.line_number_offset
    }

    /// The column number offset (see [`with_line_column_number_offsets`](Self::with_line_column_number_offsets)).
    pub fn column_number_offset(&self) -> usize {
        self.column_number_offset
    }

    /// Create a lazy line/column index over this source's content, with this source's
    /// line/column offsets applied.
    pub fn line_index(&self) -> LineIndex<'_> {
        LineIndex::new(&self.content)
            .with_line_column_number_offsets(self.line_number_offset, self.column_number_offset)
    }

    /// Iterate over this source's provenance chain: this source's provenance first, then the
    /// provenance of each triggering source in turn, ending with a
    /// [`SourceProvenance::Primary`].
    pub fn provenance_chain(&self) -> ProvenanceChain<'_, O> {
        ProvenanceChain { next: Some(&self.provenance) }
    }
}

/// Truncating content preview used by the hand-written `Debug` impls, so that debugging a
/// span never dumps an entire document.
struct ContentPreview<'a>(&'a str);

impl fmt::Debug for ContentPreview<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const MAX_CHARS: usize = 60;
        if self.0.len() <= MAX_CHARS {
            write!(f, "{:?}", self.0)
        } else {
            let prefix: String = self.0.chars().take(MAX_CHARS).collect();
            write!(f, "{:?}… ({} bytes total)", prefix, self.0.len())
        }
    }
}

impl<O: SourceOrigin> fmt::Debug for Source<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source")
            .field("origin", &self.origin)
            .field("provenance", &self.provenance)
            .field("content", &ContentPreview(&self.content))
            .finish_non_exhaustive()
    }
}

/// A byte range within one [`Source`], carrying an `Arc` to that source.
///
/// This is the location type stored in nodes, diagnostics, and provenance records: it
/// resolves its own content with no external lookup, and it keeps its source alive across
/// tree transformations. Equality is *identity-based* for the source (same `Arc`) plus equal
/// byte range.
#[derive(Clone)]
pub struct SourceSpan<O: SourceOrigin = Option<String>> {
    source: Arc<Source<O>>,
    /// Starting byte position (inclusive).
    start: usize,
    /// Ending byte position (exclusive).
    end: usize,
}

impl<O: SourceOrigin> SourceSpan<O> {
    /// Create a span over `range` within `source` — a `Range<usize>` or a plain
    /// [`Span`] (the transient parsing type; this is the bridge from byte-range land
    /// into `Arc`-carrying land).
    ///
    /// The range must lie within the source content and fall on `char` boundaries
    /// (checked in debug builds; violating it makes [`content`](Self::content) panic).
    pub fn new(source: &Arc<Source<O>>, range: impl Into<Range<usize>>) -> Self {
        let range = range.into();
        debug_assert!(
            range.start <= range.end && range.end <= source.content.len(),
            "SourceSpan range {}..{} out of bounds (source length {})",
            range.start,
            range.end,
            source.content.len(),
        );
        debug_assert!(
            source.content.is_char_boundary(range.start)
                && source.content.is_char_boundary(range.end),
            "SourceSpan range {}..{} not on char boundaries",
            range.start,
            range.end,
        );
        SourceSpan { source: Arc::clone(source), start: range.start, end: range.end }
    }

    /// Create a span covering the entire content of `source`.
    pub fn entire(source: &Arc<Source<O>>) -> Self {
        SourceSpan::new(source, 0..source.content.len())
    }

    /// The source this span points into.
    pub fn source(&self) -> &Arc<Source<O>> {
        &self.source
    }

    /// Starting byte position (inclusive).
    pub fn start(&self) -> usize {
        self.start
    }

    /// Ending byte position (exclusive).
    pub fn end(&self) -> usize {
        self.end
    }

    /// The byte range within the source.
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// The byte range as a plain [`Span`] — the inverse of the [`new`](Self::new)
    /// bridge, for handing a stored location back to `Span`-based helpers.
    pub fn span(&self) -> Span {
        Span::new(self.start, self.end)
    }

    /// Length of the span in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the span is empty (zero-length).
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// The source text this span covers.
    pub fn content(&self) -> &str {
        &self.source.content[self.start..self.end]
    }

    /// Whether `self` and `other` point into the same [`Source`] instance (identity, not
    /// content comparison).
    pub fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
    }
}

impl<O: SourceOrigin> PartialEq for SourceSpan<O> {
    fn eq(&self, other: &Self) -> bool {
        self.same_source(other) && self.start == other.start && self.end == other.end
    }
}

impl<O: SourceOrigin> Eq for SourceSpan<O> {}

impl<O: SourceOrigin> fmt::Debug for SourceSpan<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SourceSpan(")?;
        if let Some(label) = self.source.origin.label() {
            write!(f, "[{}] ", label)?;
        }
        write!(f, "{}..{} {:?})", self.start, self.end, ContentPreview(self.content()))
    }
}

/// How a [`Source`] entered the parse.
///
/// `Resolved` and `Synthesized` sources point back (via `triggered_at`) to the location that
/// caused their creation. Since a triggering location always lies in an *older* source, these
/// back-references form a provenance tree, never a cycle. Per the module-level invariant,
/// provenance only ever references sources — never nodes.
#[derive(Debug, Clone)]
pub enum SourceProvenance<O: SourceOrigin = Option<String>> {
    /// Top-level source provided directly by the user.
    Primary,
    /// Resolved from an external reference (e.g. `\input{file.tex}`).
    Resolved {
        /// The reference that was resolved (e.g. the file name).
        reference: String,
        /// Where the resolution was triggered.
        triggered_at: SourceSpan<O>,
    },
    /// Synthesized during parsing (e.g. macro expansion).
    Synthesized {
        /// What produced the content (e.g. `"macro expansion"`).
        description: String,
        /// Where the synthesis was triggered.
        triggered_at: SourceSpan<O>,
    },
}

impl<O: SourceOrigin> SourceProvenance<O> {
    /// Whether this is a primary (user-provided, top-level) source.
    pub fn is_primary(&self) -> bool {
        matches!(self, SourceProvenance::Primary)
    }

    /// The location that triggered this source's creation, if any.
    pub fn triggered_at(&self) -> Option<&SourceSpan<O>> {
        match self {
            SourceProvenance::Primary => None,
            SourceProvenance::Resolved { triggered_at, .. } => Some(triggered_at),
            SourceProvenance::Synthesized { triggered_at, .. } => Some(triggered_at),
        }
    }
}

/// Iterator over a provenance chain; see [`Source::provenance_chain`].
pub struct ProvenanceChain<'a, O: SourceOrigin> {
    next: Option<&'a SourceProvenance<O>>,
}

impl<'a, O: SourceOrigin> Iterator for ProvenanceChain<'a, O> {
    type Item = &'a SourceProvenance<O>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next.take()?;
        self.next = current
            .triggered_at()
            .map(|span| span.source().provenance());
        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc_source(content: &str) -> Arc<Source> {
        Arc::new(Source::new(content))
    }

    #[test]
    fn source_creation() {
        let source: Source = Source::new("Hello\nWorld\n")
            .with_origin(Some("https://example.com/test.tex".to_string()));
        assert_eq!(source.content(), "Hello\nWorld\n");
        assert_eq!(
            source.origin().label().as_deref(),
            Some("https://example.com/test.tex")
        );
        assert!(source.provenance().is_primary());
        assert_eq!(source.line_number_offset(), 1);
        assert_eq!(source.column_number_offset(), 1);
    }

    #[test]
    fn span_content() {
        let source = arc_source("Hello World");
        let span = SourceSpan::new(&source, 0..5);

        assert_eq!(span.start(), 0);
        assert_eq!(span.end(), 5);
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());
        assert_eq!(span.content(), "Hello");
    }

    /// The `Span` bridge: `new` accepts a plain `Span` directly, and `span()` hands the
    /// byte range back as a `Span` (the inverse).
    #[test]
    fn span_bridge_round_trips() {
        let source = arc_source("Hello World");
        let from_span = SourceSpan::new(&source, Span::new(1, 4));
        assert_eq!(from_span.range(), 1..4);
        assert_eq!(from_span.span(), Span::new(1, 4));
        assert_eq!(from_span, SourceSpan::new(&source, 1..4));
    }

    #[test]
    fn empty_span() {
        let source = arc_source("Hello");
        let span = SourceSpan::new(&source, 3..3);

        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
        assert_eq!(span.content(), "");
    }

    #[test]
    fn entire_span() {
        let source = arc_source("Hello");
        let span = SourceSpan::entire(&source);
        assert_eq!(span.range(), 0..5);
        assert_eq!(span.content(), "Hello");
    }

    #[test]
    fn span_equality_is_source_identity() {
        let source_a = arc_source("same content");
        let source_b = arc_source("same content");

        let span_a1 = SourceSpan::new(&source_a, 0..4);
        let span_a2 = SourceSpan::new(&source_a, 0..4);
        let span_a3 = SourceSpan::new(&source_a, 0..5);
        let span_b = SourceSpan::new(&source_b, 0..4);

        assert_eq!(span_a1, span_a2);
        assert_ne!(span_a1, span_a3); // same source, different range
        assert_ne!(span_a1, span_b); // equal content, different Source instance
        assert!(span_a1.same_source(&span_a3));
        assert!(!span_a1.same_source(&span_b));
    }

    #[test]
    fn span_survives_dropping_other_references() {
        // A span keeps its source alive on its own — the self-containment property.
        let span = {
            let source = arc_source("transient source");
            SourceSpan::new(&source, 0..9)
        };
        assert_eq!(span.content(), "transient");
    }

    #[test]
    fn synthesized_source_provenance() {
        let main = arc_source(r"\mycommand");
        let trigger = SourceSpan::entire(&main);
        let expanded: Arc<Source> = Arc::new(Source::synthesized(
            "expanded content",
            "macro expansion",
            trigger.clone(),
        ));

        assert_eq!(expanded.content(), "expanded content");
        // Synthesized content has no origin; the provenance carries the description.
        assert_eq!(expanded.origin().label(), None);
        assert!(!expanded.provenance().is_primary());
        assert_eq!(expanded.provenance().triggered_at(), Some(&trigger));
    }

    #[test]
    fn resolved_source_provenance() {
        let main = arc_source(r"\input{chapter.tex}");
        let trigger = SourceSpan::entire(&main);
        let included: Arc<Source> =
            Arc::new(Source::resolved("chapter text", "chapter.tex", trigger.clone()));

        // The reference lives in the provenance; the origin stays at its default (`None`)
        // unless the resolver attaches one via `with_origin`.
        assert_eq!(included.origin().label(), None);
        match included.provenance() {
            SourceProvenance::Resolved { reference, triggered_at } => {
                assert_eq!(reference, "chapter.tex");
                assert_eq!(triggered_at, &trigger);
            }
            other => panic!("expected Resolved provenance, got {:?}", other),
        }
    }

    #[test]
    fn provenance_chain_walks_to_primary() {
        let document = arc_source(r"\input{main.tex}");
        let main: Arc<Source> = Arc::new(Source::resolved(
            r"\mycommand",
            "main.tex",
            SourceSpan::entire(&document),
        ));
        let expanded: Arc<Source> = Arc::new(Source::synthesized(
            "expansion",
            "macro expansion",
            SourceSpan::entire(&main),
        ));

        let chain: Vec<_> = expanded.provenance_chain().collect();
        assert_eq!(chain.len(), 3);
        assert!(matches!(chain[0], SourceProvenance::Synthesized { .. }));
        assert!(matches!(chain[1], SourceProvenance::Resolved { .. }));
        assert!(matches!(chain[2], SourceProvenance::Primary));
    }

    #[test]
    fn debug_output_truncates_content() {
        let long_content = "x".repeat(10_000);
        let source = arc_source(&long_content);
        let span = SourceSpan::entire(&source);

        let debug = format!("{:?}", span);
        assert!(debug.len() < 300, "Debug output too long: {} bytes", debug.len());
        assert!(debug.contains("0..10000"));

        let debug_source = format!("{:?}", source);
        assert!(debug_source.len() < 300);
    }
}
