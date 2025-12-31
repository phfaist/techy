//! Source location tracking for parsed content.
//!
//! This module provides types for tracking positions and ranges in source text.
//! The `Source` struct owns the source string, and `SourceLocation` references
//! it to provide location information. Line/column computation is lazy and only
//! performed when needed (e.g., for error reporting).

/// A source string with utilities for position tracking.
///
/// Stores the source content. Line/column information is computed on-demand
/// rather than cached upfront.
#[derive(Debug, Clone)]
pub struct Source {
    /// The source content.
    content: String,
    /// The source origin (e.g., file name)
    origin: String,
}

impl Source {
    /// Create a new source from a string.
    pub fn new(content: String, origin: String) -> Self {
        Self { content, origin }
    }

    /// Get the source content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the source origin (file name, url, or other origin information)
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Get detailed location information for a source location.
    ///
    /// This computes line/column information on-demand.
    pub fn get_location_details<'src>(
        &'src self,
        location: SourceLocation<'src>,
    ) -> SourceLocationDetails<'src> {
        SourceLocationDetails::new(self, location)
    }
}

/// A location or range in source text.
///
/// References a `Source` object and stores byte positions.
/// Line/column information is computed lazily via `details()`.
#[derive(Debug, Clone, Copy)]
pub struct SourceLocation<'src> {
    /// Reference to the source.
    source: &'src Source,
    /// Starting byte position (inclusive).
    start: usize,
    /// Ending byte position (exclusive).
    end: usize,
}

impl<'src> SourceLocation<'src> {
    /// Create a new source location.
    pub fn new(source: &'src Source, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }

    /// Get the starting byte position (inclusive).
    pub fn start(&self) -> usize {
        self.start
    }

    /// Get the ending byte position (exclusive).
    pub fn end(&self) -> usize {
        self.end
    }

    /// Get the length of the location in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Check if the location is empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Get the content at this location.
    pub fn content(&self) -> &'src str {
        &self.source.content[self.start..self.end]
    }

    /// Get detailed location information including line/column data.
    ///
    /// This computes line numbers on-demand.
    pub fn details(&self) -> SourceLocationDetails<'src> {
        self.source.get_location_details(*self)
    }
}

/// Detailed location information including line/column data.
///
/// This struct caches computed line start positions up to the end
/// of the location, allowing efficient creation of details for
/// other locations in the same vicinity.
#[derive(Debug, Clone)]
pub struct SourceLocationDetails<'src> {
    /// Reference to the source.
    source: &'src Source,
    /// The location being described.
    location: SourceLocation<'src>,
    /// Line start positions computed up to the end position.
    /// Each element is the byte position where a line starts.
    line_starts: Vec<usize>,
}

impl<'src> SourceLocationDetails<'src> {
    /// Create detailed location information.
    ///
    /// Computes line starts up to the location's end position.
    fn new(source: &'src Source, location: SourceLocation<'src>) -> Self {
        let line_starts = Self::compute_line_starts_up_to(&source.content, location.end);
        Self {
            source,
            location,
            line_starts,
        }
    }

    /// Compute line start positions up to a given byte position.
    fn compute_line_starts_up_to(content: &str, up_to: usize) -> Vec<usize> {
        let mut line_starts = vec![0];
        for (i, ch) in content.char_indices() {
            if i >= up_to {
                break;
            }
            if ch == '\n' {
                line_starts.push(i + 1);
            }
        }
        line_starts
    }

    /// Get the (line, column) for a byte position using cached line starts.
    ///
    /// Lines and columns are 1-indexed.
    /// Returns (usize::MAX, usize::MAX) if position exceeds cached line information.
    fn get_line_col(&self, pos: usize) -> (usize, usize) {
        if pos > self.source.content.len() {
            return (usize::MAX, usize::MAX);
        }

        // Check if position is beyond what we've cached
        let last_cached = self.line_starts.last().copied().unwrap_or(0);
        if pos >= last_cached && !self.line_starts.is_empty() {
            // Position exceeds cached line information
            return (usize::MAX, usize::MAX);
        }

        // Binary search to find the line
        let line_idx = match self.line_starts.binary_search(&pos) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let line = line_idx + 1; // 1-indexed
        let col = pos - self.line_starts[line_idx] + 1; // 1-indexed

        (line, col)
    }

    /// Get the starting (line, column) position (1-indexed).
    pub fn start_line_col(&self) -> (usize, usize) {
        self.get_line_col(self.location.start)
    }

    /// Get the starting line number (1-indexed).
    pub fn start_line(&self) -> usize {
        self.get_line_col(self.location.start).0
    }

    /// Get the ending (line, column) position (1-indexed).
    pub fn end_line_col(&self) -> (usize, usize) {
        self.get_line_col(self.location.end)
    }

    /// Get the ending line number (1-indexed).
    pub fn end_line(&self) -> usize {
        self.get_line_col(self.location.end).0
    }

    /// Get a formatted string describing this location.
    ///
    /// Returns a human-readable description like "line 10, column 15"
    /// or "line 5, columns 3–18".
    pub fn formatted_location(&self) -> String {
        let (start_line, start_col) = self.get_line_col(self.location.start);
        let (end_line, end_col) = self.get_line_col(self.location.end);

        // Check if line info is available (not usize::MAX)
        if start_line == usize::MAX || end_line == usize::MAX {
            return format!("position {}–{}", self.location.start, self.location.end);
        }

        if start_line == end_line {
            if start_col == end_col {
                format!("line {}, column {}", start_line, start_col)
            } else {
                format!("line {}, columns {}–{}", start_line, start_col, end_col)
            }
        } else {
            format!(
                "line {}, column {} to line {}, column {}",
                start_line, start_col, end_line, end_col
            )
        }
    }

    /// Create details for another location, reusing cached line information.
    ///
    /// This is more efficient than creating details from scratch if the
    /// new location is near the current one, as it reuses already-computed
    /// line start positions.
    pub fn other_details(&self, location: SourceLocation<'src>) -> SourceLocationDetails<'src> {
        // Clone the line_starts we've already computed
        let mut line_starts = self.line_starts.clone();

        // Extend if the new location goes beyond what we've computed
        if location.end > self.line_starts.last().copied().unwrap_or(0) {
            let start_from = self.line_starts.last().copied().unwrap_or(0);
            for (i, ch) in self.source.content[start_from..].char_indices() {
                let abs_pos = start_from + i;
                if abs_pos >= location.end {
                    break;
                }
                if ch == '\n' {
                    line_starts.push(abs_pos + 1);
                }
            }
        }

        SourceLocationDetails {
            source: self.source,
            location,
            line_starts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_creation() {
        let source = Source::new("Hello\nWorld\n".to_string(), "test".to_string());
        assert_eq!(source.content(), "Hello\nWorld\n");
    }

    #[test]
    fn test_source_location_content() {
        let source = Source::new("Hello World".to_string(), "test".to_string());
        let loc = SourceLocation::new(&source, 0, 5);

        assert_eq!(loc.start(), 0);
        assert_eq!(loc.end(), 5);
        assert_eq!(loc.len(), 5);
        assert_eq!(loc.content(), "Hello");
        assert!(!loc.is_empty());
    }

    #[test]
    fn test_location_details_single_line() {
        let source = Source::new("Hello World".to_string(), "test".to_string());
        let loc = SourceLocation::new(&source, 0, 5);
        let details = loc.details();

        assert_eq!(details.start_line(), 1);
        assert_eq!(details.start_line_col(), (1, 1));
        assert_eq!(details.end_line(), 1);
        assert_eq!(details.end_line_col(), (1, 5));
        assert_eq!(details.formatted_location(), "line 1, columns 1–5");
    }

    #[test]
    fn test_location_details_multiline() {
        let source = Source::new("Hello\nWorld\nTest".to_string(), "test".to_string());
        let loc = SourceLocation::new(&source, 3, 9);
        let details = loc.details();

        // Starts at 'l' in "Hello" (line 1, col 4) and ends at 'r' in "World" (line 2, col 3)
        assert_eq!(details.start_line(), 1);
        assert_eq!(details.start_line_col(), (1, 4));
        assert_eq!(details.end_line(), 2);
        assert_eq!(details.end_line_col(), (2, 3));
        assert_eq!(
            details.formatted_location(),
            "line 1, column 4 to line 2, column 3"
        );
    }

    #[test]
    fn test_empty_location() {
        let source = Source::new("Hello".to_string(), "test".to_string());
        let loc = SourceLocation::new(&source, 3, 3);

        assert!(loc.is_empty());
        assert_eq!(loc.len(), 0);
    }

    #[test]
    fn test_other_details_reuses_line_info() {
        let source = Source::new("Line1\nLine2\nLine3\nLine4".to_string(), "test".to_string());
        let loc1 = SourceLocation::new(&source, 0, 10); // Spans first two lines
        let details1 = loc1.details();

        // Check line info
        assert_eq!(details1.start_line(), 1);
        assert_eq!(details1.end_line(), 2);

        // Create details for another location using cached info
        let loc2 = SourceLocation::new(&source, 6, 15); // Spans lines 2-3
        let details2 = details1.other_details(loc2);

        assert_eq!(details2.start_line(), 2);
        assert_eq!(details2.start_line_col(), (2, 1));
        assert_eq!(details2.end_line(), 3);
        assert_eq!(
            details2.formatted_location(),
            "line 2, column 1 to line 3, column 3"
        );
    }

    #[test]
    fn test_lazy_line_computation() {
        // Large source that we don't want to process all upfront
        let source = Source::new("a\n".repeat(1000), "test".to_string());
        let loc = SourceLocation::new(&source, 0, 5);

        // Creating location doesn't compute any line info yet
        assert_eq!(loc.start(), 0);
        assert_eq!(loc.end(), 5);

        // Only when we ask for details do we compute line info
        let details = loc.details();
        assert_eq!(details.start_line(), 1);
        // Line info was only computed up to position 5, not the entire 2000-char string
    }
}
