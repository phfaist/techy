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
    #[inline]
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the range is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// The range as a standard `Range<usize>`.
    #[inline]
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// Borrow the spanned text out of the content the span refers into.
    ///
    /// # Panics
    ///
    /// Panics if the span is out of bounds for `content` or not on `char` boundaries
    /// (same contract as `&content[range]`).
    #[inline]
    pub fn slice<'s>(&self, content: &'s str) -> &'s str {
        &content[self.range()]
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
}
