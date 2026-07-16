//! Plain byte ranges ([`Span`]) used throughout parsing.

use core::fmt;
use core::ops::Range;

/// A plain byte range within one source's content. `Copy`, no `Arc` — used everywhere
/// during parsing (tokens, transient scanning). Nodes and errors that must outlive the
/// parse carry [`SourceSpan`](crate::source::SourceSpan) instead.
///
/// Both offsets are byte positions into the source content; `start..end` is half-open,
/// like standard Rust ranges. Positions are expected to fall on `char` boundaries — a
/// caller contract, enforced where spans meet content ([`slice`](Span::slice) panics,
/// [`get`](Span::get) returns `None`), not by this type.
///
/// Fields are private and every mutator preserves `start <= end` (decided July 2026,
/// Action-05 session — consistent with `SourceSpan`): [`new`](Span::new) debug-asserts
/// it, and in-place growth goes through the monotone [`extend_to`](Span::extend_to).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Byte offset of the first byte of the range.
    start: usize,
    /// Byte offset one past the last byte of the range.
    end: usize,
}

impl Span {
    /// Create a span covering `start..end`. `start <= end` is the caller's contract
    /// (debug-asserted).
    #[inline]
    pub fn new(start: usize, end: usize) -> Span {
        debug_assert!(start <= end, "span start {} is after end {}", start, end);
        Span { start, end }
    }

    /// Create an empty span positioned at `pos`.
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
    ///
    /// Saturating: an inverted span (`start > end` — a caller bug `new` debug-asserts
    /// against, unreachable through the mutators) has length 0 rather than wrapping in
    /// release builds (panic policy, DESIGN_RATIONALE.md).
    #[inline]
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Whether the range is empty (length 0 — consistent with [`len`](Span::len) on
    /// inverted spans).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The range as a standard `Range<usize>`.
    #[inline]
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// Grow the span in place so it ends at `end` — the one sanctioned in-place
    /// mutation (accumulating a run of tokens into one node's span). Monotone:
    /// `end >= self.end()` is the caller's contract (debug-asserted), so the
    /// `start <= end` invariant is preserved.
    #[inline]
    pub fn extend_to(&mut self, end: usize) {
        debug_assert!(
            end >= self.end,
            "extend_to({}) would shrink the span ending at {}",
            end,
            self.end
        );
        self.end = end;
    }

    /// The smallest span covering both `self` and `other` (byte-range union including
    /// any gap between them; the operands may touch, overlap, nest, or be disjoint, in
    /// either order).
    #[inline]
    pub fn cover(&self, other: Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end) }
    }

    /// Borrow the spanned text out of the content the span refers into.
    ///
    /// Use this for spans minted from `content` itself; for spans of unknown
    /// provenance, use the non-panicking [`get`](Span::get).
    ///
    /// # Panics
    ///
    /// Panics if the span is out of bounds for `content` or not on `char` boundaries
    /// (same contract as `&content[range]` — the approved indexing-style exception,
    /// panic policy, DESIGN_RATIONALE.md).
    #[inline]
    pub fn slice<'s>(&self, content: &'s str) -> &'s str {
        &content[self.range()]
    }

    /// Borrow the spanned text, or `None` if the span is out of bounds for `content`
    /// or not on `char` boundaries — the non-panicking companion of
    /// [`slice`](Span::slice) (same contract as `content.get(range)`).
    #[inline]
    pub fn get<'s>(&self, content: &'s str) -> Option<&'s str> {
        content.get(self.range())
    }
}

impl From<Range<usize>> for Span {
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
    #[should_panic(expected = "would shrink")]
    #[cfg(debug_assertions)]
    fn extend_to_rejects_shrinking() {
        let mut span = Span::new(2, 9);
        span.extend_to(4);
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
        // An inverted span is a caller bug (`Span::new` debug-asserts; the mutators
        // preserve the invariant), but release builds skip the assert; `len`/`is_empty`
        // stay consistent and benign.
        let inverted = Span { start: 7, end: 3 };
        assert_eq!(inverted.len(), 0);
        assert!(inverted.is_empty());
    }
}
