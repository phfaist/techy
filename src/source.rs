//! Source location tracking for parsed content.
//!
//! This module provides types for tracking positions and ranges in source text.
//! The `SourceLocation` trait defines the interface for source location information,
//! and `Span` is the default concrete implementation representing byte offsets.

use std::borrow::Cow;

/// Trait for types that represent a location or range in source text.
///
/// This allows different parser implementations to store additional
/// source location information beyond simple byte offsets.
pub trait SourceLocation: std::fmt::Debug + Clone {
    /// Get the starting byte position (inclusive).
    fn start(&self) -> usize;

    /// Get the ending byte position (exclusive).
    fn end(&self) -> usize;

    /// Get the length of the location in bytes.
    fn len(&self) -> usize {
        self.end() - self.start()
    }

    /// Check if the location is empty.
    fn is_empty(&self) -> bool {
        self.start() >= self.end()
    }

    /// Get the text content at this location in the source.
    fn content(&self) -> Cow<'_, str>;

    /// Where the content was obtained from (e.g., file name, URL, information
    /// about how it was auto-generated, etc.).
    fn source_name(&self) -> Cow<'_, str>;

    /// Get a formatted string describing this location.
    ///
    /// For simple spans, this might be "position 130–145" or
    /// "line 10, column 15".
    /// For extended implementations, this might include file paths,
    /// line/column numbers, or other context.
    fn formatted_location(&self) -> String;
}


#[derive(Debug, Clone, Copy, Hash)]
pub struct SimpleStringSourceLocation {
    start: usize;
    end: usize;
    content: String;
}
impl SourceLocation for SimpleStringSourceLocation {
    fn start(&self) -> usize {
        self.start
    }
    fn end(&self) -> usize {
        self.end
    }
    fn content(&self) -> &str {
        Cow::Borrowed(content)
    }
    fn source_name(&self) -> Cow<'_, str> {
        Cow::Borrowed("<no source information>")
    }
    fn formatted_location(&self) -> String {
        format!("pos {}–{}", self.start, self.end)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = SimpleStringSourceLocation { 0, 5, "dummy content" };
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 5);
        assert_eq!(span.len(), 5);
    }

    #[test]
    fn test_span_text() {
        let span = SimpleStringSourceLocation { 0, 5, source };
        assert_eq!(span.text(), "Hello");
    }
}
