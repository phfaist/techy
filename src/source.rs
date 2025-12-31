//! Source location tracking for parsed content.
//!
//! This module provides types for tracking positions and ranges in source text.
//! A `Span` represents a contiguous range in the source and is used throughout
//! the library to associate tokens, AST nodes, and errors with their original
//! source locations.

/// A span representing a range in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Starting byte position (inclusive).
    pub start: usize,
    /// Ending byte position (exclusive).
    pub end: usize,
}

impl Span {
    /// Create a new span.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Get the length of the span.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Check if the span is empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Get the text this span refers to in the source.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }

    /// Extend this span to include another span.
    pub fn extend(&self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = Span::new(0, 5);
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 5);
        assert_eq!(span.len(), 5);
    }

    #[test]
    fn test_span_text() {
        let source = "Hello world";
        let span = Span::new(0, 5);
        assert_eq!(span.text(source), "Hello");
    }

    #[test]
    fn test_span_extend() {
        let span1 = Span::new(0, 5);
        let span2 = Span::new(3, 10);
        let extended = span1.extend(span2);
        assert_eq!(extended.start, 0);
        assert_eq!(extended.end, 10);
    }
}
