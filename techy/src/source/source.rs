//! The `Arc`-based source model: [`Source`], [`SourceSpan`], [`SourcePos`], and
//! [`SourceProvenance`].

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;
use core::ops::Range;

use super::line_index::LineIndex;
use super::origin::SourceOrigin;
use super::span::Span;

/// One unit of source content: a document, an included file, or a snippet synthesized
/// while parsing.
///
/// A source is immutable once created and is shared as `Arc<Source>`. Every
/// [`SourceSpan`] holds such an `Arc`, which is what makes spans — and the nodes and
/// diagnostics that store them — self-contained.
///
/// Create the top-level content with [`new`](Source::new); [`resolved`](Source::resolved)
/// and [`synthesized`](Source::synthesized) create the two kinds of derived source and
/// record where they came from (see [`SourceProvenance`]). The builder methods
/// [`with_origin`](Source::with_origin) and
/// [`with_line_column_number_offsets`](Source::with_line_column_number_offsets) adjust how
/// the source is displayed in diagnostics.
///
/// Content is held as a plain `String`: an embedder with a memory-mapped UTF-8 file hands
/// it in as text after one validation pass, and genuinely chunked or streaming input would
/// need a different reader design rather than a different backing behind this type.
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
    /// Creates a primary source: top-level content provided directly by the user.
    ///
    /// The origin is unknown, the provenance is [`SourceProvenance::Primary`], and the
    /// line and column number offsets are `(1, 1)`.
    ///
    /// Every call creates a new source *identity*. [`SourceSpan`] and [`SourcePos`]
    /// compare their source by identity rather than by content (see
    /// [`SourceSpan::same_source`]), so spans into two separate `Source` values never
    /// compare equal, even when the two contents are byte-identical. Code that has to
    /// correlate positions across several operations therefore creates the source once
    /// and passes the same `Arc<Source>` everywhere — parsing included:
    /// [`Language::parse_setup`](crate::core::Language::parse_setup) takes such a handle,
    /// whereas [`Language::parse`](crate::core::Language::parse) creates a fresh source on
    /// every call.
    pub fn new(content: impl Into<String>) -> Self {
        Source {
            content: content.into(),
            origin: O::default(),
            provenance: SourceProvenance::Primary,
            line_number_offset: 1,
            column_number_offset: 1,
        }
    }

    /// Creates a source whose content was resolved from the external reference
    /// `reference`, requested by a construct at `triggered_at`.
    ///
    /// This is what a caller of a [`SourceResolver`](super::SourceResolver) builds from
    /// the resolved content; [`resolve_source_reference`](super::resolve_source_reference)
    /// does it for you. The origin is left at its default, so a creator that knows where
    /// the content was obtained from (conventionally a URL) attaches it with
    /// [`with_origin`](Self::with_origin).
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

    /// Creates a source whose content was synthesized during parsing — a macro expansion,
    /// for instance — triggered by the construct at `triggered_at`.
    ///
    /// The origin is left at its default, since the content was not obtained from
    /// anywhere; `description` says what produced it and is recorded in the source's
    /// [`SourceProvenance`].
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

    /// Sets the origin of this source, conventionally the URL the content was obtained
    /// from (see [`SourceOrigin`](super::SourceOrigin)).
    pub fn with_origin(mut self, origin: O) -> Self {
        self.origin = origin;
        self
    }

    /// Sets the [`SourceProvenance`] of this source, overriding the one its constructor
    /// recorded.
    pub fn with_provenance(mut self, provenance: SourceProvenance<O>) -> Self {
        self.provenance = provenance;
        self
    }

    /// Sets the line and column number offsets used when a position in this source is
    /// displayed.
    ///
    /// The defaults are `(1, 1)`, giving 1-indexed line and column numbers; use `(0, 0)`
    /// for 0-indexed ones. A resolver that consumed a leading part of a file itself uses a
    /// larger line offset so that reported line numbers still match the original file (see
    /// [`ResolvedContent::line_number_offset`](super::ResolvedContent::line_number_offset)).
    ///
    /// Any value is accepted. A displayed number is the zero-based position plus the
    /// offset, added with saturating arithmetic, so an offset near `usize::MAX` — read
    /// back from a serialized source, say — yields `usize::MAX` rather than overflowing.
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

    /// The line number offset (see
    /// [`with_line_column_number_offsets`](Self::with_line_column_number_offsets)).
    pub fn line_number_offset(&self) -> usize {
        self.line_number_offset
    }

    /// The column number offset (see
    /// [`with_line_column_number_offsets`](Self::with_line_column_number_offsets)).
    pub fn column_number_offset(&self) -> usize {
        self.column_number_offset
    }

    /// Creates a [`LineIndex`](super::LineIndex) over this source's content, with this
    /// source's line and column number offsets applied.
    ///
    /// The index computes line starts lazily and borrows the content, so it is meant to be
    /// used and dropped. To keep line information across many queries or several parses,
    /// use a [`LineIndexCache`](super::LineIndexCache) instead.
    pub fn line_index(&self) -> LineIndex<'_> {
        LineIndex::new(&self.content)
            .with_line_column_number_offsets(self.line_number_offset, self.column_number_offset)
    }

    /// Iterates over this source's provenance chain: this source's own provenance first,
    /// then the provenance of each triggering source in turn, ending with a
    /// [`SourceProvenance::Primary`].
    ///
    /// The companion [`including_sources`](Source::including_sources) walks the same chain
    /// but yields the sources rather than their provenance records.
    pub fn provenance_chain(&self) -> ProvenanceChain<'_, O> {
        ProvenanceChain { next: Some(&self.provenance) }
    }

    /// Iterates over the chain of including sources: this source first, then the source
    /// containing its [`triggered_at`](SourceProvenance::triggered_at) location, and so on
    /// up to the primary source.
    ///
    /// This is the building block for include-recursion policies, because it yields the
    /// sources themselves and their [origins](Source::origin) carry the names a policy
    /// compares: `.any(…)` gives a cycle check, `.count()` a depth bound, and
    /// `.filter(…).count()` a bounded self-inclusion policy of the kind `.dtx` files need.
    /// [`check_include_chain`](super::check_include_chain) is the ready-made combination
    /// of the first two.
    ///
    /// The chain is always finite: a triggering location always lies in a source created
    /// before this one.
    pub fn including_sources(&self) -> IncludingSources<'_, O> {
        IncludingSources { next: Some(self) }
    }
}

/// Iterator over a chain of including sources, returned by
/// [`Source::including_sources`].
pub struct IncludingSources<'a, O: SourceOrigin> {
    next: Option<&'a Source<O>>,
}

impl<'a, O: SourceOrigin> Iterator for IncludingSources<'a, O> {
    type Item = &'a Source<O>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next.take()?;
        self.next = current
            .provenance()
            .triggered_at()
            .map(|span| &**span.source());
        Some(current)
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

/// A byte range within one [`Source`], together with an `Arc` to that source.
///
/// This is the location type stored in nodes, diagnostics, and provenance records. It
/// resolves its own [`content`](SourceSpan::content) with no external lookup and keeps its
/// source alive for as long as it exists, which is what lets a parsed tree outlive the
/// parse without a lifetime parameter.
///
/// The plain counterpart used during a parse is [`Span`], which is `Copy` and names no
/// source: [`new`](SourceSpan::new) converts one into a `SourceSpan`, and
/// [`span`](SourceSpan::span) converts back. [`SourcePos`] is the single-offset
/// counterpart, reached through [`start_pos`](SourceSpan::start_pos) and
/// [`end_pos`](SourceSpan::end_pos).
///
/// Two spans are equal when their byte ranges are equal and their sources are the *same*
/// source — compared by `Arc` identity, not by content (see
/// [`same_source`](SourceSpan::same_source)).
#[derive(Clone)]
pub struct SourceSpan<O: SourceOrigin = Option<String>> {
    source: Arc<Source<O>>,
    /// Starting byte position (inclusive).
    start: usize,
    /// Ending byte position (exclusive).
    end: usize,
}

impl<O: SourceOrigin> SourceSpan<O> {
    /// Creates a span over `range` within `source`.
    ///
    /// The range may be given as a `Range<usize>` or as a plain [`Span`]; this is how a
    /// construct parser turns the byte range it scanned into a location it can store in a
    /// node. The reverse conversion is [`span`](Self::span).
    ///
    /// Because the range is checked here, every `SourceSpan` that exists is valid for
    /// [`content`](Self::content).
    ///
    /// # Panics
    ///
    /// Panics if the range does not lie within the source content, or if either end falls
    /// inside a multi-byte character. Passing a valid range is the caller's contract, and
    /// it is checked in all builds — one of the crate's few deliberate panics (see the
    /// [list of panicking items](crate::guide::panics)).
    pub fn new(source: &Arc<Source<O>>, range: impl Into<Range<usize>>) -> Self {
        let range = range.into();
        assert!(
            range.start <= range.end && range.end <= source.content.len(),
            "SourceSpan range {}..{} out of bounds (source length {})",
            range.start,
            range.end,
            source.content.len(),
        );
        assert!(
            source.content.is_char_boundary(range.start)
                && source.content.is_char_boundary(range.end),
            "SourceSpan range {}..{} not on char boundaries",
            range.start,
            range.end,
        );
        SourceSpan { source: Arc::clone(source), start: range.start, end: range.end }
    }

    /// Creates a span covering the entire content of `source`.
    pub fn entire(source: &Arc<Source<O>>) -> Self {
        SourceSpan::new(source, 0..source.content.len())
    }

    /// Creates the empty span at position `pos`.
    ///
    /// This is what a diagnostic uses when it reports about a place rather than about a
    /// stretch of text ("expected an argument here"). It is the inverse of
    /// [`start_pos`](Self::start_pos) and [`end_pos`](Self::end_pos), which turn a span
    /// into a position.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use techy::source::{Source, SourcePos, SourceSpan};
    ///
    /// let source: Arc<Source> = Arc::new(Source::new("ab\ncd"));
    /// let span = SourceSpan::at(&SourcePos::new(&source, 3));
    /// assert!(span.is_empty());
    /// assert_eq!(span.range(), 3..3);
    /// assert_eq!(span.start_pos(), SourcePos::new(&source, 3));
    /// ```
    pub fn at(pos: &SourcePos<O>) -> Self {
        SourceSpan { source: Arc::clone(&pos.source), start: pos.pos, end: pos.pos }
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

    /// The byte range as a plain [`Span`], for passing a stored location back to
    /// `Span`-based helpers.
    ///
    /// This is the inverse of [`new`](Self::new).
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

    /// Whether `self` and `other` point into the same [`Source`] value.
    ///
    /// Sources are compared by `Arc` identity, never by content: two sources built from
    /// the same text are still different sources.
    pub fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
    }

    /// The span's start, as a [`SourcePos`].
    ///
    /// [`at`](Self::at) turns a position back into an empty span.
    pub fn start_pos(&self) -> SourcePos<O> {
        SourcePos { source: Arc::clone(&self.source), pos: self.start }
    }

    /// The span's end, as a [`SourcePos`].
    ///
    /// The end is exclusive: this is the position one past the span's last byte, where the
    /// content following the span begins, not a position inside the span.
    pub fn end_pos(&self) -> SourcePos<O> {
        SourcePos { source: Arc::clone(&self.source), pos: self.end }
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

/// A single location within one [`Source`], together with an `Arc` to that source.
///
/// This is the single-offset counterpart of [`SourceSpan`], passed as one value instead of
/// a loose `(source, offset)` pair. Position lookups over parsed trees take a `SourcePos`;
/// lookups by range take a `SourceSpan`.
///
/// The offset is a byte position into the source content and must fall on a `char`
/// boundary, which [`new`](SourcePos::new) checks. Equality compares the offset and the
/// source by `Arc` identity, exactly as for [`SourceSpan`].
///
/// Line and column numbers are not a method on a position: as for spans, they come from
/// the source's [`LineIndex`](crate::source::LineIndex) (or from a
/// [`LineIndexCache`](crate::source::LineIndexCache) held across queries).
///
/// ```
/// use std::sync::Arc;
/// use techy::source::{Source, SourcePos};
///
/// let source: Arc<Source> = Arc::new(Source::new("ab\ncd"));
/// let pos = SourcePos::new(&source, 4);
/// let mut index = pos.source().line_index();
/// assert_eq!(index.line_col(pos.pos()), Some((2, 2)));
/// ```
#[derive(Clone)]
pub struct SourcePos<O: SourceOrigin = Option<String>> {
    source: Arc<Source<O>>,
    pos: usize,
}

impl<O: SourceOrigin> SourcePos<O> {
    /// Creates a position at byte offset `pos` within `source`.
    ///
    /// # Panics
    ///
    /// Panics if `pos` lies outside the source content, or inside a multi-byte character.
    /// The valid offsets are `0..=len`; the content length itself is a valid
    /// end-of-content position. Passing a valid offset is the caller's contract, and it is
    /// checked in all builds — one of the crate's few deliberate panics (see the [list of
    /// panicking items](crate::guide::panics)).
    pub fn new(source: &Arc<Source<O>>, pos: usize) -> Self {
        assert!(
            pos <= source.content.len(),
            "SourcePos {} out of bounds (source length {})",
            pos,
            source.content.len(),
        );
        assert!(
            source.content.is_char_boundary(pos),
            "SourcePos {} not on a char boundary",
            pos,
        );
        SourcePos { source: Arc::clone(source), pos }
    }

    /// The source this position points into.
    pub fn source(&self) -> &Arc<Source<O>> {
        &self.source
    }

    /// The byte offset within the source content.
    pub fn pos(&self) -> usize {
        self.pos
    }
}

impl<O: SourceOrigin> PartialEq for SourcePos<O> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source) && self.pos == other.pos
    }
}

impl<O: SourceOrigin> Eq for SourcePos<O> {}

impl<O: SourceOrigin> fmt::Debug for SourcePos<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SourcePos(")?;
        if let Some(label) = self.source.origin.label() {
            write!(f, "[{}] ", label)?;
        }
        write!(f, "{})", self.pos)
    }
}

/// How a [`Source`] entered the parse: as the primary input, resolved from an external
/// reference, or synthesized while parsing.
///
/// A `Resolved` or `Synthesized` source points back, through its `triggered_at` span, at
/// the location that caused it to be created. Following those back-references gives the
/// include chain of a location, which is what
/// [`Source::provenance_chain`] and [`Source::including_sources`] iterate over.
///
/// A triggering location always lies in a source created earlier, so these
/// back-references form a tree and never a cycle. Provenance references only sources,
/// never nodes.
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
    /// Whether this is a [`Primary`](SourceProvenance::Primary) source: top-level content
    /// provided by the user.
    pub fn is_primary(&self) -> bool {
        matches!(self, SourceProvenance::Primary)
    }

    /// The location that triggered this source's creation, or `None` for a
    /// [`Primary`](SourceProvenance::Primary) source.
    pub fn triggered_at(&self) -> Option<&SourceSpan<O>> {
        match self {
            SourceProvenance::Primary => None,
            SourceProvenance::Resolved { triggered_at, .. } => Some(triggered_at),
            SourceProvenance::Synthesized { triggered_at, .. } => Some(triggered_at),
        }
    }
}

/// Iterator over a provenance chain, returned by [`Source::provenance_chain`].
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

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn a_source_span_with_an_out_of_bounds_range_panics_in_all_builds() {
        // The approved always-on precondition asserts ([§dd-dr:panic-policy] rule 3).
        let source = arc_source("ab");
        let _ = SourceSpan::new(&source, 0..9);
    }

    #[test]
    #[should_panic(expected = "not on char boundaries")]
    fn a_source_span_cutting_a_char_panics_in_all_builds() {
        let source = arc_source("\u{e9}!"); // 'é' occupies bytes 0..2
        let _ = SourceSpan::new(&source, 1..3);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn a_source_pos_out_of_bounds_panics_in_all_builds() {
        let source = arc_source("ab");
        let _ = SourcePos::new(&source, 9);
    }

    #[test]
    #[should_panic(expected = "not on a char boundary")]
    fn a_source_pos_inside_a_char_panics_in_all_builds() {
        let source = arc_source("\u{e9}!");
        let _ = SourcePos::new(&source, 1);
    }

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
    fn source_pos_points_at_one_location() {
        let source = arc_source("Hello");
        let pos = SourcePos::new(&source, 2);
        assert_eq!(pos.pos(), 2);
        assert!(Arc::ptr_eq(pos.source(), &source));
        assert_eq!(pos, SourcePos::new(&source, 2));
        assert_ne!(pos, SourcePos::new(&source, 3));
        // Identity-based source equality, like SourceSpan.
        let twin = arc_source("Hello");
        assert_ne!(pos, SourcePos::new(&twin, 2));
        assert_eq!(format!("{:?}", pos), "SourcePos(2)");
        // End-of-content is a valid position.
        let end = SourcePos::new(&source, 5);
        assert_eq!(end.pos(), 5);
    }

    #[test]
    fn span_start_and_end_pos_bridge_to_positions() {
        let source = arc_source("Hello");
        let span = SourceSpan::new(&source, 1..4);
        assert_eq!(span.start_pos(), SourcePos::new(&source, 1));
        // end_pos is exclusive: one past the span's last byte.
        assert_eq!(span.end_pos(), SourcePos::new(&source, 4));
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
    fn including_sources_walks_self_to_primary() {
        let document: Arc<Source> =
            Arc::new(Source::new(r"\input{main.tex}").with_origin(Some("doc".into())));
        let main: Arc<Source> = Arc::new(
            Source::resolved(r"\mycommand", "main.tex", SourceSpan::entire(&document))
                .with_origin(Some("main".into())),
        );
        let expanded: Arc<Source> = Arc::new(Source::synthesized(
            "expansion",
            "macro expansion",
            SourceSpan::entire(&main),
        ));

        // Self first, then each including source, ending at the primary — the
        // *sources* (whose origins carry the names), not the provenance records.
        let labels: Vec<Option<String>> = expanded
            .including_sources()
            .map(|source| source.origin().label().map(|label| label.into_owned()))
            .collect();
        assert_eq!(labels, [None, Some("main".into()), Some("doc".into())]);

        // A primary source's chain is itself alone.
        assert_eq!(document.including_sources().count(), 1);
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
    fn source_span_at_a_position_is_the_empty_span_there() {
        let source = arc_source("ab\ncd");
        let pos = SourcePos::new(&source, 3);
        let span = SourceSpan::at(&pos);

        assert!(span.is_empty());
        assert_eq!(span.range(), 3..3);
        assert!(Arc::ptr_eq(span.source(), &source));
        // Round trip: `at` is the inverse of `start_pos`/`end_pos`.
        assert_eq!(span.start_pos(), pos);
        assert_eq!(span.end_pos(), pos);
        assert_eq!(SourceSpan::at(&SourceSpan::new(&source, 1..4).end_pos()).range(), 4..4);
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
