//! The plain byte range type [`Span`], used throughout parsing.

use core::fmt;
use core::ops::Range;

/// A byte range within the content of a single source.
///
/// Both ends are byte offsets into that content, and `start..end` is half-open, like a
/// standard Rust range. A `Span` does not record *which* source it refers to; it is
/// `Copy` and holds no `Arc`, and it is what tokens and other short-lived positions use
/// during a parse.
///
/// Nodes and diagnostics outlive the parse and have to name their source, so they store
/// a [`SourceSpan`](crate::source::SourceSpan) instead.
/// [`SourceSpan::new`](crate::source::SourceSpan::new) builds one from a `Span` and the
/// source it points into, and [`SourceSpan::span`](crate::source::SourceSpan::span)
/// returns the plain range.
///
/// Offsets are expected to fall on `char` boundaries. This type does not check that;
/// the check happens where a span meets content, in [`slice`](Span::slice) (which
/// panics) and [`get`](Span::get) (which returns `None`).
///
/// Every span satisfies `start <= end`. [`new`](Span::new) asserts it, and the only
/// in-place mutation, [`extend_to`](Span::extend_to), can move the end forward but not
/// backward. The `From<Range<usize>>` conversion goes through `new`, so
/// `Span::from(7..3)` panics on the same assert.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Byte offset of the first byte of the range.
    start: usize,
    /// Byte offset one past the last byte of the range.
    end: usize,
}

impl Span {
    /// Creates a span covering `start..end`.
    ///
    /// # Panics
    ///
    /// Panics if `start > end`. Keeping the two ends ordered is the caller's contract,
    /// and it is checked in all builds — one of the crate's few deliberate panics (see
    /// the [list of panicking items](crate::guide::panics)). The
    /// `From<Range<usize>>` conversion delegates here and panics on the same
    /// condition.
    #[inline]
    pub fn new(start: usize, end: usize) -> Span {
        assert!(start <= end, "span start {} is after end {}", start, end);
        Span { start, end }
    }

    /// Creates an empty span at byte offset `pos`.
    #[inline]
    pub fn empty(pos: usize) -> Span {
        Span { start: pos, end: pos }
    }

    /// Byte offset of the first byte of the range.
    #[inline]
    pub fn start(&self) -> usize {
        self.start
    }

    /// Byte offset one past the last byte of the range.
    #[inline]
    pub fn end(&self) -> usize {
        self.end
    }

    /// Length of the range in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        // `start <= end` holds for every span built through the public API, so this is
        // just `end - start`; the saturating subtraction is defensive only.
        self.end.saturating_sub(self.start)
    }

    /// Whether the range is empty, that is, whether its [`len`](Span::len) is zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The range as a standard `Range<usize>`.
    #[inline]
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// Grows the span in place so that it ends at `end`.
    ///
    /// This is the only in-place mutation a span allows. Parsers use it to accumulate a
    /// run of tokens into the span of the node they are building. The start is left
    /// untouched, so the `start <= end` invariant still holds afterwards.
    ///
    /// # Panics
    ///
    /// Panics if `end` is before the span's current end: growth is monotone, and asking
    /// for a shorter span violates the caller's contract. The check runs in all builds
    /// (see the [list of panicking items](crate::guide::panics)).
    #[inline]
    pub fn extend_to(&mut self, end: usize) {
        assert!(
            end >= self.end,
            "extend_to({}) would shrink the span ending at {}",
            end,
            self.end
        );
        self.end = end;
    }

    /// The smallest span covering both `self` and `other`.
    ///
    /// This is the union of the two byte ranges, including any gap between them. The
    /// operands may touch, overlap, nest, or be disjoint, in either order.
    #[inline]
    pub fn cover(&self, other: Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end) }
    }

    /// Whether byte offset `pos` lies within the span.
    ///
    /// Containment is half-open, like a standard Rust range: `start() <= pos < end()`.
    /// The end offset itself is therefore not contained, and an empty span contains no
    /// position at all, not even its own start.
    #[inline]
    pub fn contains(&self, pos: usize) -> bool {
        self.start <= pos && pos < self.end
    }

    /// Borrows the text this span covers out of `content`.
    ///
    /// Use this for a span that was produced from `content` itself. For a span of
    /// unknown provenance, use the non-panicking companion [`get`](Span::get).
    ///
    /// # Panics
    ///
    /// Panics if the span is out of bounds for `content`, or if either end falls inside
    /// a multi-byte character — the same contract as `&content[range]`, and one of the
    /// crate's few deliberate panics (see the [list of panicking
    /// items](crate::guide::panics)).
    #[inline]
    pub fn slice<'s>(&self, content: &'s str) -> &'s str {
        &content[self.range()]
    }

    /// Borrows the text this span covers out of `content`, or `None` if the span is out
    /// of bounds or does not fall on `char` boundaries.
    ///
    /// This is the non-panicking companion of [`slice`](Span::slice), with the same
    /// contract as `content.get(range)`.
    #[inline]
    pub fn get<'s>(&self, content: &'s str) -> Option<&'s str> {
        content.get(self.range())
    }
}

impl From<Range<usize>> for Span {
    /// Creates the span covering `range`.
    ///
    /// # Panics
    ///
    /// Panics if the range is inverted (`range.start > range.end`): the conversion
    /// delegates to [`Span::new`] and inherits its precondition, so `Span::from(7..3)`
    /// panics, in all builds (see the [list of panicking
    /// items](crate::guide::panics)).
    fn from(range: Range<usize>) -> Span {
        Span::new(range.start, range.end)
    }
}

impl From<Span> for Range<usize> {
    fn from(span: Span) -> Range<usize> {
        span.range()
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_basics() {
        let span = Span::new(3, 7);
        assert_eq!((span.start(), span.end()), (3, 7));
        assert_eq!(span.len(), 4);
        assert!(!span.is_empty());
        assert_eq!(span.range(), 3..7);
        assert_eq!(span.slice("abcdefghij"), "defg");
        assert_eq!(format!("{:?}", span), "3..7");
    }

    #[test]
    fn span_empty() {
        let span = Span::empty(5);
        assert_eq!(span, Span::new(5, 5));
        assert!(span.is_empty());
        assert_eq!(span.len(), 0);
        assert_eq!(span.slice("abcdefghij"), "");
    }

    #[test]
    fn span_from_range_and_back() {
        let span: Span = (2..4).into();
        assert_eq!(span, Span::new(2, 4));
        let range: Range<usize> = span.into();
        assert_eq!(range, 2..4);
    }

    #[test]
    fn span_get_is_the_non_panicking_slice() {
        let span = Span::new(3, 7);
        assert_eq!(span.get("abcdefghij"), Some("defg"));
        assert_eq!(span.get("ab"), None); // out of bounds
        assert_eq!(Span::new(0, 1).get("é!"), None); // mid-char boundary
    }

    #[test]
    fn extend_to_grows_in_place() {
        let mut span = Span::new(2, 4);
        span.extend_to(9);
        assert_eq!(span, Span::new(2, 9));
        span.extend_to(9); // extending to the current end is a no-op, not a shrink
        assert_eq!(span, Span::new(2, 9));
    }

    #[test]
    #[should_panic(expected = "is after end")]
    fn new_rejects_an_inverted_span() {
        let _ = Span::new(7, 3);
    }

    #[test]
    #[should_panic(expected = "would shrink")]
    fn extend_to_rejects_shrinking() {
        let mut span = Span::new(2, 9);
        span.extend_to(4);
    }

    #[test]
    fn contains_is_half_open_and_empty_spans_never_match() {
        let span = Span::new(3, 7);
        assert!(!span.contains(2));
        assert!(span.contains(3)); // start included
        assert!(span.contains(6)); // last byte included
        assert!(!span.contains(7)); // end excluded (half-open)
        assert!(!span.contains(8));

        let empty = Span::empty(5);
        assert!(!empty.contains(4));
        assert!(!empty.contains(5)); // an empty span contains nothing
        assert!(!empty.contains(6));
    }

    #[test]
    fn cover_is_the_byte_range_union() {
        let a = Span::new(2, 5);
        let b = Span::new(8, 11);
        assert_eq!(a.cover(b), Span::new(2, 11)); // disjoint: gap included
        assert_eq!(b.cover(a), Span::new(2, 11)); // order-independent
        assert_eq!(a.cover(Span::new(3, 4)), a); // nested
        assert_eq!(a.cover(Span::empty(5)), a); // touching empty span
    }

    #[test]
    fn inverted_span_has_len_zero() {
        // Unrepresentable through the public API (`Span::new` asserts in all builds;
        // the mutators preserve the invariant); this pins the defensive saturation
        // for the private-field escape hatch only this module has.
        let inverted = Span { start: 7, end: 3 };
        assert_eq!(inverted.len(), 0);
        assert!(inverted.is_empty());
    }
}
