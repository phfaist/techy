//! Source location tracking for parsed content.
//!
//! This module provides types for tracking positions and ranges in source text.
//! The `Source` struct owns the source string, and `SourceLocation` references
//! it to provide location information. Line/column computation is lazy and only
//! performed when needed (e.g., for error reporting).


// PHF REVIEWED ✅

// LATER TODO: Allow Source not to have to keep in memory its entire content


use log::warn;

pub trait SourceOrigin : Default + Debug + Clone {
    fn from_description(what : String) -> Self;
}

/// A source string with utilities for position tracking.
///
/// Stores the source content. Line/column information is computed on-demand
/// rather than cached upfront.
#[derive(Debug, Clone)]
pub struct Source<SourceOrigin : SourceOrigin> {
    /// The source content.
    content: String,
    /// The source origin (e.g., file name)
    origin: SourceOrigin,
    /// Line number offset (default: 1 for 1-indexed, or 0 for 0-indexed)
    line_number_offset: usize,
    /// Column number offset (default: 1 for 1-indexed, or 0 for 0-indexed)
    column_number_offset: usize,

    parent_source : Option<& Source<SourceOrigin>>, ...........
}

impl<SourceOrigin : SourceOrigin> Source<SourceOrigin> {
    /// Create a new source from a string with default settings.
    ///
    /// Defaults: origin = (SourceOrigin default),
    /// line_number_offset = 1, column_number_offset = 1
    pub fn new(content: String) -> Self {
        Self {
            content,
            origin: SourceOrigin::default(),
            line_number_offset: 1,
            column_number_offset: 1,
            parent_source: None,
        }
    }

    /// Set the origin (e.g., file name, URL) for this source.
    pub fn with_origin(mut self, origin : SourceOrigin) -> Self {
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

    pub fn make_analyzer(&self) -> SourceLocationAnalyzer {
        SourceLocationAnalyzer::new(self)
    }

    pub fn make_generated_source(
        &self,
        content : String,
        what : String,
    ) -> Self {
        Self {
            content,
            origin: SourceOrigin::from_description(what),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceLocationVia<'src, SourceOrigin : SourceOrigin> {
    /// An explanation for why there is an intermediate "source" representing
    /// the node or token (e.g. file input, automatically generated, node
    /// processed/replaced, macro expanded, etc.)
    what : String,
    /// The new source, if applicable (e.g. input file)
    pos : SourceLocation<'src, SourceOrigin>,
}

/// A location or range in source text.
///
/// References a `Source` object and stores byte positions.
#[derive(Debug, Clone)] // implements PartialEq manually
pub struct SourceLocation<'src, SourceOrigin : SourceOrigin> {
    /// Reference to the source.
    source: &'src Source,
    /// Starting byte position (inclusive) in the source which resulted
    /// in the parsing of the given token or node.  Note that
    /// source.content[start..end] might not be the verbatim code
    /// represented by the considered node or token, as they might have
    /// been generated automatically or processed from the source (e.g.
    /// performed some kind of macro replacement).
    start: usize,
    /// Ending byte position (exclusive) in the source which resulted
    /// in the parsing of the given token or node.
    end: usize,
    /// Steps that will help trace the location through steps of \include,
    /// auto-generated content, processing, macro expansion, etc.
    via: [ SourceLocationVia ],
}

impl<'src> SourceLocation<'src> {

    pub fn source(&self) -> &'src Source {
        self.source
    }

    /// Get the starting byte position (inclusive).
    pub fn start(&self) -> usize {
        self.start
    }

    /// Get the ending byte position (exclusive).
    pub fn end(&self) -> usize {
        self.end
    }

    pub fn via(&self) -> & [SourceLocationVia] {
        &self.via
    }

    /// Get the length of the location in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Get the content at this location.
    pub fn original_content(&self) -> &'src str {
        &self.source.content[self.start..self.end]
    }
}

impl<'src> PartialEq for SourceLocation<'src> {
    fn eq(&self, other: &Self) -> bool {
        // Compare source by pointer equality (same Source object)
        std::ptr::eq(self.source, other.source)
            && self.start == other.start
            && self.end == other.end
            && self.via == other.via
    }
}


/// Detailed location information including line/column data.
///
/// This struct caches computed line start positions up to the end
/// of the location, allowing efficient creation of details for
/// other locations in the same vicinity.
#[derive(Debug, Clone)]
pub struct SourceLocationAnalyzer<'src> {
    /// Reference to the source.
    source: &'src Source,
    /// Line start positions computed up to the end position.
    /// Each element is the byte position where a line starts.
    line_starts: Vec<usize>,
    /// Remember location until which we computed line starts
    line_starts_computed_end: usize,
    /// Maximum number of characters to compute line information for
    max_chars_line_count: usize,
}

impl<'src> SourceLocationAnalyzer<'src> {
    /// Create detailed location information.
    ///
    /// Will compute line starts on demand.
    fn new(source: &'src Source) -> Self {
        SourceLocationAnalyzer {
            source,
            line_starts: vec![ 0 ],
            line_starts_computed_end: 0,
            max_chars_line_count: 100000,
        }
    }
    pub fn set_max_chars_line_count(&mut self, max_chars_line_count: usize) {
        if self.line_starts_computed_end == usize::MAX
           && self.source.content.len() > self.max_chars_line_count {
            // line_starts_computed_end was already set to usize::MAX (and
            // warning generated) because of source size.  Reset to zero
            // in case the new limit would warrant calculation of line starts.
            if self.source.content.len() <= max_chars_line_count {
                self.line_starts = vec![ 0 ];
                self.line_starts_computed_end = 0;
            }
        }
        self.max_chars_line_count = max_chars_line_count;
    }

    fn extend_line_starts_up_to(
        &mut self,
        up_to: usize,
    ) {
        let new_line_starts_computed_end = up_to + 1;

        println!("DEBUG: {:?} cmp {:?}; self.line_starts_computed_end={:?}", self.source.content.len(), self.max_chars_line_count, self.line_starts_computed_end);

        if self.source.content.len() > self.max_chars_line_count {
            if self.line_starts_computed_end == 0 {
                // source text too large to compute lines - emit warning
                // the first time extend_line_starts_up_to() is called
                // (self.line_starts_computed_end == 0)
                warn!("Source content too big (>{:.3}K) to compute lines",
                    self.max_chars_line_count as f64 / 1000.0);
                self.line_starts.clear();
                self.line_starts_computed_end = usize::MAX;
            }
            return;
        }

        // Extend if the new location goes beyond what we've computed
        if new_line_starts_computed_end > self.line_starts_computed_end {
            let start_from = self.line_starts_computed_end;
            let end_at = self.max_chars_line_count.min(self.source.content.len());
            for (i, ch) in self.source.content[start_from..end_at].char_indices() {
                let abs_pos = start_from + i;
                if abs_pos >= new_line_starts_computed_end {
                    break;
                }
                if ch == '\n' {
                    self.line_starts.push(abs_pos + 1);
                }
            }
            self.line_starts_computed_end = new_line_starts_computed_end;
        }
    }

    /// Get the (line, column) for a byte position using cached line starts.
    ///
    /// Lines and columns use the offsets configured in the Source.
    /// Returns None if position exceeds maximum line information or if the
    /// position exceeds the source's content length.
    pub fn get_line_col(&mut self, index: usize) -> Option<(usize, usize)> {
        if index > self.source.content.len() {
            return None;
        }

        // Extend line starts up to this position if needed
        self.extend_line_starts_up_to(index);

        if index >= self.line_starts_computed_end || self.line_starts.len() == 0 {
            return None;
        }

        // Binary search to find the line
        let line_idx = match self.line_starts.binary_search(&index) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        // Compute 0-based line/col, then add offsets
        let line = line_idx + self.source.line_number_offset;
        let col = (index - self.line_starts[line_idx]) + self.source.column_number_offset;

        Some((line, col))
    }

    /// Format a position with line/column information if available.
    ///
    /// Returns a string like "@ (line 3, col 5)" or "@ char pos 42" if
    /// line/column information is not available.
    /// If origin is provided, appends " [origin]" to the output.
    fn format_pos(&mut self, index: usize, origin: Option<&str>) -> String {
        let pos_str = if let Some((line, col)) = self.get_line_col(index) {
            format!("@ (line {}, col {})", line, col)
        } else {
            format!("@ char pos {}", index)
        };

        if let Some(origin) = origin {
            format!("{} [{}]", pos_str, origin)
        } else {
            pos_str
        }
    }

    /// Format a traceback showing open blocks.
    ///
    /// Takes a list of pairs where each pair contains a `SourceLocation` and
    /// a description string. Only the `start` position of each `SourceLocation`
    /// is used for formatting.
    ///
    /// Returns a formatted string with the "Open blocks:" header followed by
    /// each block with its position information. Returns an empty string if
    /// the list is empty.
    ///
    /// # Example output
    /// ```text
    /// Open blocks:
    ///   @ (line 8, col 1): environment 'document'
    ///   @ (line 5, col 3): macro '\section'
    /// ```
    pub fn format_traceback(&mut self, open_blocks: &[(SourceLocation<'src>, String)]) -> String {
        if open_blocks.is_empty() {
            return String::new();
        }

        let mut result = String::from("Open blocks:");
        for (loc, what) in open_blocks.iter() {
            result.push_str("\n  ");
            let origin = if !loc.source.origin.is_empty() {
                Some(loc.source.origin.as_str())
            } else {
                None
            };
            result.push_str(&self.format_pos(loc.start(), origin));
            result.push_str(": ");
            result.push_str(what);
        }

        result
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_creation() {
        let source = Source::new("Hello\nWorld\n".to_string()).with_origin("test".to_string());
        assert_eq!(source.original_content(), "Hello\nWorld\n");
        assert_eq!(source.origin(), "test");
    }

    #[test]
    fn test_source_location_content() {
        let source = Source::new("Hello World".to_string());
        let loc = source.make_pos(0, 5);

        assert_eq!(loc.start(), 0);
        assert_eq!(loc.end(), 5);
        assert_eq!(loc.len(), 5);
        assert_eq!(loc.original_content(), "Hello");
    }

    #[test]
    fn test_location_analyzer_single_line() {
        let source = Source::new("Hello World".to_string());
        let mut analyzer = source.make_analyzer();

        assert_eq!(analyzer.get_line_col(0), Some((1, 1)));  // 'H'
        assert_eq!(analyzer.get_line_col(5), Some((1, 6)));  // ' '
        assert_eq!(analyzer.get_line_col(10), Some((1, 11))); // 'd'
    }

    #[test]
    fn test_location_analyzer_multiline() {
        let source = Source::new("Hello\nWorld\nTest".to_string());
        let mut analyzer = source.make_analyzer();

        // Line 1
        assert_eq!(analyzer.get_line_col(0), Some((1, 1)));  // 'H'
        assert_eq!(analyzer.get_line_col(3), Some((1, 4)));  // second 'l'
        assert_eq!(analyzer.get_line_col(5), Some((1, 6)));  // '\n'

        // Line 2
        assert_eq!(analyzer.get_line_col(6), Some((2, 1)));  // 'W'
        assert_eq!(analyzer.get_line_col(9), Some((2, 4)));  // 'l'
        assert_eq!(analyzer.get_line_col(11), Some((2, 6))); // '\n'

        // Line 3
        assert_eq!(analyzer.get_line_col(12), Some((3, 1))); // 'T'
        assert_eq!(analyzer.get_line_col(13), Some((3, 2))); // 'e'
    }

    #[test]
    fn test_location_analyzer_reuses_line_info() {
        let source = Source::new("Line1\nLine2\nLine3\nLine4".to_string());
        let mut analyzer = source.make_analyzer();

        // Check first location span
        assert_eq!(analyzer.get_line_col(0), Some((1, 1)));   // Start of line 1
        assert_eq!(analyzer.get_line_col(10), Some((2, 5)));  // End of line 2

        // Create another location using cached info - should reuse line starts
        assert_eq!(analyzer.get_line_col(6), Some((2, 1)));   // Start of line 2
        assert_eq!(analyzer.get_line_col(15), Some((3, 4)));  // Mid line 3
    }

    #[test]
    fn test_empty_location() {
        let source = Source::new("Hello".to_string());
        let loc = source.make_pos(3, 3);

        assert_eq!(loc.len(), 0);
    }

    #[test]
    fn test_location_analyzer_lazy_computation() {
        // Large source that we don't want to process all upfront
        let source = Source::new("a\n".repeat(1000));
        let mut analyzer = source.make_analyzer();

        // Only compute line info up to position 5
        assert_eq!(analyzer.get_line_col(0), Some((1, 1)));
        assert_eq!(analyzer.get_line_col(5), Some((3, 2)));

        // Line info was only computed up to position 5, not the entire 2000-char string
        assert_eq!(analyzer.line_starts_computed_end, 6);
    }

    #[test]
    fn test_location_analyzer_zero_indexed_offsets() {
        let source = Source::new("Hello\nWorld".to_string())
            .with_line_column_number_offsets(0, 0);
        let mut analyzer = source.make_analyzer();

        // First line is line 0, first column is column 0
        assert_eq!(analyzer.get_line_col(0), Some((0, 0)));
        assert_eq!(analyzer.get_line_col(5), Some((0, 5)));
        assert_eq!(analyzer.get_line_col(6), Some((1, 0))); // Start of second line
    }

    #[test]
    fn test_location_analyzer_custom_offsets() {
        let source = Source::new("Hello\nWorld".to_string())
            .with_origin("snippet".to_string())
            .with_line_column_number_offsets(10, 5);
        let mut analyzer = source.make_analyzer();

        // Second line with offset 10 = line 11, first col with offset 5 = col 5
        assert_eq!(analyzer.get_line_col(0), Some((10, 5)));  // Line 0 + 10, col 0 + 5
        assert_eq!(analyzer.get_line_col(6), Some((11, 5)));  // Line 1 + 10, col 0 + 5
        assert_eq!(analyzer.get_line_col(10), Some((11, 9))); // Line 1 + 10, col 4 + 5
    }

    #[test]
    fn test_location_analyzer_source_too_long() {
        // Create a source that exceeds max_chars_line_count.
        // 120 chars exceeds 100 chars
        let source = Source::new("a\n".repeat(60));
        let mut analyzer = source.make_analyzer();
        analyzer.set_max_chars_line_count(100);
        println!("{:?}", analyzer);

        // Should return None because source is too long
        assert_eq!(analyzer.get_line_col(0), None);
        assert_eq!(analyzer.get_line_col(100), None);
    }

    #[test]
    fn test_location_analyzer_out_of_bounds() {
        let source = Source::new("Hello".to_string());
        let mut analyzer = source.make_analyzer();

        // Position beyond content length should return None
        assert_eq!(analyzer.get_line_col(100), None);

        // Valid positions should work
        assert_eq!(analyzer.get_line_col(0), Some((1, 1)));
        assert_eq!(analyzer.get_line_col(4), Some((1, 5)));
    }

    #[test]
    fn test_format_traceback_single_block() {
        let source = Source::new("Hello\nWorld\nTest".to_string());
        let mut analyzer = source.make_analyzer();

        let open_blocks = vec![
            (source.make_pos(6, 11), "environment 'document'".to_string()),
        ];

        let traceback = analyzer.format_traceback(&open_blocks);
        assert_eq!(traceback, "Open blocks:\n  @ (line 2, col 1): environment 'document'");
    }

    #[test]
    fn test_format_traceback_multiple_open_blocks() {
        let source = Source::new("Hello\nWorld\nTest\nMore".to_string());
        let mut analyzer = source.make_analyzer();

        let open_blocks = vec![
            (source.make_pos(12, 16), "macro '\\textbf'".to_string()),
            (source.make_pos(6, 11), "environment 'document'".to_string()),
            (source.make_pos(0, 5), "macro '\\section'".to_string()),
        ];

        let traceback = analyzer.format_traceback(&open_blocks);
        assert_eq!(traceback, "Open blocks:\n  @ (line 3, col 1): macro '\\textbf'\n  @ (line 2, col 1): environment 'document'\n  @ (line 1, col 1): macro '\\section'");
    }

    #[test]
    fn test_format_traceback_empty_open_blocks() {
        let source = Source::new("Hello".to_string());
        let mut analyzer = source.make_analyzer();

        let open_blocks: Vec<(SourceLocation, String)> = vec![];
        let traceback = analyzer.format_traceback(&open_blocks);
        assert_eq!(traceback, "");
    }

    #[test]
    fn test_format_traceback_with_origin() {
        let source = Source::new("Hello\nWorld\nTest".to_string())
            .with_origin("test.tex".to_string());
        let mut analyzer = source.make_analyzer();

        let open_blocks = vec![
            (source.make_pos(6, 11), "environment 'document'".to_string()),
        ];

        let traceback = analyzer.format_traceback(&open_blocks);
        assert_eq!(traceback, "Open blocks:\n  @ (line 2, col 1) [test.tex]: environment 'document'");
    }

    #[test]
    fn test_format_traceback_multiple_blocks_with_origin() {
        let source = Source::new("Hello\nWorld\nTest\nMore".to_string())
            .with_origin("myfile.tex".to_string());
        let mut analyzer = source.make_analyzer();

        let open_blocks = vec![
            (source.make_pos(12, 16), "macro '\\textbf'".to_string()),
            (source.make_pos(6, 11), "environment 'document'".to_string()),
        ];

        let traceback = analyzer.format_traceback(&open_blocks);
        assert_eq!(traceback, "Open blocks:\n  @ (line 3, col 1) [myfile.tex]: macro '\\textbf'\n  @ (line 2, col 1) [myfile.tex]: environment 'document'");
    }
}
