//! Plain byte ranges ([`Span`]) used throughout parsing.

use core::fmt;
use core::ops::Range;

/// A plain byte range within one source's content. `Copy`, no `Arc` — used everywhere
/// during parsing (tokens, transient scanning). Nodes and errors that must outlive the
/// parse carry [`SourceSpan`](crate::source::SourceSpan) instead.
///
/// Both offsets are byte positions into the source content; `start..end` is half-open,
/// like standard Rust ranges, and must fall on `char` boundaries.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Byte offset of the first byte of the range.
    pub start: usize,
    /// Byte offset one past the last byte of the range.
    pub end: usize,
}

impl Span {
    /// Create a span covering `start..end`.
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

    /// Length of the range in bytes.
    ///
    /// Saturating: an inverted span (`start > end`, constructible through the public
    /// fields — a caller bug `Span::new` debug-asserts against) has length 0 rather
    /// than wrapping (panic policy, DESIGN_RATIONALE.md).
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
    fn span_from_range() {
        let span: Span = (2..4).into();
        assert_eq!(span, Span::new(2, 4));
    }

    #[test]
    fn span_get_is_the_non_panicking_slice() {
        let span = Span::new(3, 7);
        assert_eq!(span.get("abcdefghij"), Some("defg"));
        assert_eq!(span.get("ab"), None); // out of bounds
        assert_eq!(Span::new(0, 1).get("é!"), None); // mid-char boundary
    }

    #[test]
    fn inverted_span_has_len_zero() {
        // An inverted span is a caller bug (`Span::new` debug-asserts), but the public
        // fields make it constructible; `len`/`is_empty` stay consistent and benign.
        let inverted = Span { start: 7, end: 3 };
        assert_eq!(inverted.len(), 0);
        assert!(inverted.is_empty());
    }
}
