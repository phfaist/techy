//! Source location tracking for parsed content.
//!
//! This module provides types for tracking positions and ranges in source text.
//! The `Source` struct owns the source string, and `SourceLocation` references
//! it to provide location information. Line/column computation is lazy and only
//! performed when needed (e.g., for error reporting).


// PHF REVIEWED ✅


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
    /// Line number offset (default: 1 for 1-indexed, or 0 for 0-indexed)
    line_number_offset: usize,
    /// Column number offset (default: 1 for 1-indexed, or 0 for 0-indexed)
    column_number_offset: usize,
}

impl Source {
    /// Create a new source from a string with default settings.
    ///
    /// Defaults: origin = "", line_number_offset = 1, column_number_offset = 1
    pub fn new(content: String) -> Self {
        Self {
            content,
            origin: String::new(),
            line_number_offset: 1,
            column_number_offset: 1,
        }
    }

    /// Set the origin (e.g., file name, URL) for this source.
    pub fn with_origin(mut self, origin: String) -> Self {
        self.origin = origin;
        self
    }

    /// Set the line and column number offsets.
    ///
    /// Default offsets are (1, 1) for 1-indexed line/column numbers.
    /// Use (0, 0) for 0-indexed line/column numbers.
    pub fn with_line_column_number_offsets(
        mut self,
        line_number_offset: usize,
        column_number_offset: usize,
    ) -> Self {
        self.line_number_offset = line_number_offset;
        self.column_number_offset = column_number_offset;
        self
    }

    /// Get the source content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the source origin (file name, url, or other origin information)
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Get the line number offset.
    pub fn line_number_offset(&self) -> usize {
        self.line_number_offset
    }

    /// Get the column number offset.
    pub fn column_number_offset(&self) -> usize {
        self.column_number_offset
    }

    pub fn make_pos(&self, start: usize, end: usize) -> SourceLocation {
        SourceLocation { source: &self, start, end }
    }

    /// Get detailed location information for a source location.
    ///
    /// This computes line/column information on-demand.
    pub fn get_pos_details<'src>(
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
#[derive(Debug, Clone, Copy)] // implements PartialEq manually
pub struct SourceLocation<'src> {
    /// Reference to the source.
    source: &'src Source,
    /// Starting byte position (inclusive).
    start: usize,
    /// Ending byte position (exclusive).
    end: usize,
}

impl<'src> SourceLocation<'src> {
    // /// Create a new source location.
    // pub fn new(source: &'src Source, start: usize, end: usize) -> Self {
    //     Self { source, start, end }
    // }

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
        self.source.get_pos_details(*self)
    }
}

impl<'src> PartialEq for SourceLocation<'src> {
    fn eq(&self, other: &Self) -> bool {
        // Compare source by pointer equality (same Source object)
        std::ptr::eq(self.source, other.source)
            && self.start == other.start
            && self.end == other.end
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
    /// Remember location until which we computed line starts
    line_starts_computed_end: usize,
}

impl<'src> SourceLocationDetails<'src> {
    /// Create detailed location information.
    ///
    /// Computes line starts up to the location's end position.
    fn new(source: &'src Source, location: SourceLocation<'src>) -> Self {
        Self::new_with_partial_line_starts(
            source,
            location,
            usize::max(location.start, location.end),
            &vec![0],
            0,
        )
    }
    fn new_with_partial_line_starts(
        source: &'src Source,
        location: SourceLocation<'src>,
        line_starts_up_to: usize,
        old_line_starts: &Vec<usize>,
        old_line_starts_computed_end: usize,
    ) -> Self {
        // Clone the line_starts we've already computed
        let mut line_starts = old_line_starts.clone();

        let line_starts_computed_end = line_starts_up_to + 1;

        // Extend if the new location goes beyond what we've computed
        if line_starts_computed_end > old_line_starts_computed_end {
            let start_from = old_line_starts_computed_end;
            for (i, ch) in source.content[start_from..].char_indices() {
                let abs_pos = start_from + i;
                if abs_pos >= line_starts_computed_end {
                    break;
                }
                if ch == '\n' {
                    line_starts.push(abs_pos + 1);
                }
            }
        }

        SourceLocationDetails {
            source, location, line_starts, line_starts_computed_end
        }
    }

    /// Get the (line, column) for a byte position using cached line starts.
    ///
    /// Lines and columns use the offsets configured in the Source.
    /// Returns (usize::MAX, usize::MAX) if position exceeds cached line information or source content length.
    fn get_line_col(&self, pos: usize) -> (usize, usize) {
        if pos > self.source.content.len() ||
           pos >= self.line_starts_computed_end {
            return (usize::MAX, usize::MAX);
        }

        // Binary search to find the line
        let line_idx = match self.line_starts.binary_search(&pos) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        // Compute 0-based line/col, then add offsets
        let line = line_idx + self.source.line_number_offset;
        let col = (pos - self.line_starts[line_idx]) + self.source.column_number_offset;

        (line, col)
    }

    /// Get the starting (line, column) position.
    ///
    /// Uses the offsets configured in the Source (default: 1-indexed).
    pub fn start_line_col(&self) -> (usize, usize) {
        self.get_line_col(self.location.start)
    }

    /// Get the starting line number.
    ///
    /// Uses the line offset configured in the Source (default: 1-indexed).
    pub fn start_line(&self) -> usize {
        self.get_line_col(self.location.start).0
    }

    /// Get the ending (line, column) position.
    ///
    /// Uses the offsets configured in the Source (default: 1-indexed).
    pub fn end_line_col(&self) -> (usize, usize) {
        self.get_line_col(self.location.end)
    }

    /// Get the ending line number.
    ///
    /// Uses the line offset configured in the Source (default: 1-indexed).
    pub fn end_line(&self) -> usize {
        self.get_line_col(self.location.end).0
    }

    /// Get a formatted string describing this location.
    ///
    /// Returns a human-readable description like "line 10, column 15"
    /// or "line 5, columns 3–18". Includes origin information if set.
    pub fn formatted_location(&self) -> String {
        let (start_line, start_col) = self.get_line_col(self.location.start);
        let (end_line, end_col) = self.get_line_col(self.location.end);

        // Build origin prefix if available
        let origin_prefix = if !self.source.origin.is_empty() {
            format!("{}: ", self.source.origin)
        } else {
            String::new()
        };

        // Check if line info is available (not usize::MAX)
        if start_line == usize::MAX || end_line == usize::MAX {
            return format!(
                "{}position {}–{}",
                origin_prefix, self.location.start, self.location.end
            );
        }

        if start_line == end_line {
            if start_col == end_col {
                format!("{}line {}, column {}", origin_prefix, start_line, start_col)
            } else {
                format!(
                    "{}line {}, columns {}–{}",
                    origin_prefix, start_line, start_col, end_col
                )
            }
        } else {
            format!(
                "{}line {}, column {} to line {}, column {}",
                origin_prefix, start_line, start_col, end_line, end_col
            )
        }
    }

    /// Create details for another location, reusing cached line information.
    ///
    /// This is more efficient than creating details from scratch if the
    /// new location is near the current one, as it reuses already-computed
    /// line start positions.
    pub fn other_details(&self, location: SourceLocation<'src>) -> SourceLocationDetails<'src> {
        let new_line_starts_computed_end =
            usize::max(location.start, location.end);        
        Self::new_with_partial_line_starts(
            self.source,
            location, 
            new_line_starts_computed_end,
            &self.line_starts,
            self.line_starts_computed_end,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_creation() {
        let source = Source::new("Hello\nWorld\n".to_string()).with_origin("test".to_string());
        assert_eq!(source.content(), "Hello\nWorld\n");
        assert_eq!(source.origin(), "test");
    }

    #[test]
    fn test_source_location_content() {
        let source = Source::new("Hello World".to_string());
        let loc = source.make_pos(0, 5);

        assert_eq!(loc.start(), 0);
        assert_eq!(loc.end(), 5);
        assert_eq!(loc.len(), 5);
        assert_eq!(loc.content(), "Hello");
        assert!(!loc.is_empty());
    }

    #[test]
    fn test_location_details_single_line() {
        let source = Source::new("Hello World".to_string());
        let loc = source.make_pos(0, 5); // 'H' to ' '
        let details = loc.details();

        assert_eq!(details.start_line(), 1);
        assert_eq!(details.start_line_col(), (1, 1));
        assert_eq!(details.end_line(), 1);
        assert_eq!(details.end_line_col(), (1, 6));
        assert_eq!(details.formatted_location(), "line 1, columns 1–6");
    }

    #[test]
    fn test_location_details_multiline_line_starts() {
        let source = Source::new("Hello\nWorld\nTest".to_string());
        let loc = source.make_pos(3, 13);
        let details = loc.details();

        // Starts at second 'l' in "Hello" (line 1, col 4) and ends at 'e' in "Test" (line 3, col 2)
        assert_eq!(details.line_starts, vec![0, 6, 6+6, ]);
    }

    #[test]
    fn test_location_details_multiline() {
        let source = Source::new("Hello\nWorld\nTest".to_string());
        let loc = source.make_pos(3, 9);
        let details = loc.details();

        // Starts at second 'l' in "Hello" (line 1, col 4) and ends at 'l' in "World" (line 2, col 4)
        assert_eq!(details.start_line(), 1);
        assert_eq!(details.start_line_col(), (1, 4));
        assert_eq!(details.end_line(), 2);
        assert_eq!(details.end_line_col(), (2, 4));
        assert_eq!(
            details.formatted_location(),
            "line 1, column 4 to line 2, column 4"
        );
        assert_eq!(details.line_starts, vec![0, 6, ]);
    }

    #[test]
    fn test_empty_location() {
        let source = Source::new("Hello".to_string());
        let loc = source.make_pos(3, 3);

        assert!(loc.is_empty());
        assert_eq!(loc.len(), 0);
    }

    #[test]
    fn test_other_details_reuses_line_info() {
        let source = Source::new("Line1\nLine2\nLine3\nLine4".to_string());
        let loc1 = source.make_pos(0, 10); // Spans first two lines
        let details1 = loc1.details();

        // Check line info
        assert_eq!(details1.start_line(), 1);
        assert_eq!(details1.start_line_col(), (1, 1));
        assert_eq!(details1.end_line(), 2);
        assert_eq!(details1.end_line_col(), (2, 5) );

        // Create details for another location using cached info
        let loc2 = source.make_pos(6, 15); // Spans lines 2-3
        let details2 = details1.other_details(loc2);

        assert_eq!(details2.start_line(), 2);
        assert_eq!(details2.start_line_col(), (2, 1));
        assert_eq!(details2.end_line(), 3);
        assert_eq!(details2.end_line_col(), (3, 4));
        assert_eq!(
            details2.formatted_location(),
            "line 2, column 1 to line 3, column 4"
        );
    }

    #[test]
    fn test_lazy_line_computation() {
        // Large source that we don't want to process all upfront
        let source = Source::new("a\n".repeat(1000));
        let loc = source.make_pos(0, 5);

        // Creating location doesn't compute any line info yet
        assert_eq!(loc.start(), 0);
        assert_eq!(loc.end(), 5);

        // Only when we ask for details do we compute line info
        let details = loc.details();
        assert_eq!(details.start_line(), 1);
        // Line info was only computed up to position 5, not the entire 2000-char string
    }

    #[test]
    fn test_origin_in_formatted_location() {
        let source = Source::new("Hello World".to_string()).with_origin("test.tex".to_string());
        let loc = source.make_pos(0, 5);
        let details = loc.details();

        assert_eq!(details.formatted_location(),
                   "test.tex: line 1, columns 1–6");
    }

    #[test]
    fn test_zero_indexed_offsets() {
        let source = Source::new("Hello\nWorld".to_string())
            .with_line_column_number_offsets(0, 0);
        let loc = source.make_pos(0, 5);
        let details = loc.details();

        // First line is line 0, first column is column 0
        assert_eq!(details.start_line(), 0);
        assert_eq!(details.start_line_col(), (0, 0));
        assert_eq!(details.end_line(), 0);
        assert_eq!(details.end_line_col(), (0, 5));
    }

    #[test]
    fn test_custom_offsets() {
        let source = Source::new("Hello\nWorld".to_string())
            .with_origin("snippet".to_string())
            .with_line_column_number_offsets(10, 5);
        let loc = source.make_pos(6, 11); // "World"
        let details = loc.details();

        // Second line with offset 10 = line 11, first col with offset 5 = col 5
        assert_eq!(details.start_line(), 11);
        assert_eq!(details.start_line_col(), (11, 5));
        assert_eq!(
            details.formatted_location(),
            "snippet: line 11, columns 5–10"
        );
    }
}
