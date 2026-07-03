//! Lazy line/column analysis.
//!
//! The parser works purely with byte offsets; line/column information is computed only on
//! demand, for display (error messages, diagnostics). This is the standalone successor of
//! the earlier `SourceLocationAnalyzer` (its lazy line-start extension logic is preserved;
//! the traceback formatting moved to the `error` module).

use alloc::vec;
use alloc::vec::Vec;

/// Lazily computed line-start index over a piece of source content.
///
/// Line starts are computed incrementally, only up to the largest byte offset queried so
/// far — indexing a large source costs nothing until (and unless) positions near its end are
/// actually displayed. Obtain one preconfigured with a source's line/column offsets via
/// [`Source::line_index`](super::Source::line_index).
///
/// To bound memory use on huge inputs, content longer than the configured maximum scan
/// length (default 100 000 bytes, see [`set_max_scan_len`](Self::set_max_scan_len)) is not
/// indexed at all: [`line_col`](Self::line_col) then returns `None` and callers fall back to
/// displaying raw byte positions.
#[derive(Debug, Clone)]
pub struct LineIndex<'c> {
    /// The content being indexed.
    content: &'c str,
    /// Line number offset (1 for 1-indexed line numbers).
    line_number_offset: usize,
    /// Column number offset (1 for 1-indexed column numbers).
    column_number_offset: usize,
    /// Byte positions where lines start, computed so far.
    line_starts: Vec<usize>,
    /// Position up to which line starts have been computed. `usize::MAX` marks content that
    /// exceeded the maximum scan length (no line information available).
    computed_end: usize,
    /// Maximum content length (in bytes) for which line information is computed.
    max_scan_len: usize,
}

/// Default maximum content length (in bytes) for which line information is computed.
pub const DEFAULT_MAX_SCAN_LEN: usize = 100_000;

impl<'c> LineIndex<'c> {
    /// Create a lazy line index over `content` with default line/column offsets `(1, 1)`.
    pub fn new(content: &'c str) -> Self {
        LineIndex {
            content,
            line_number_offset: 1,
            column_number_offset: 1,
            line_starts: vec![0],
            computed_end: 0,
            max_scan_len: DEFAULT_MAX_SCAN_LEN,
        }
    }

    /// Set the line and column number offsets (see
    /// [`Source::with_line_column_number_offsets`](super::Source::with_line_column_number_offsets)).
    pub fn with_line_column_number_offsets(
        mut self,
        line_number_offset: usize,
        column_number_offset: usize,
    ) -> Self {
        self.line_number_offset = line_number_offset;
        self.column_number_offset = column_number_offset;
        self
    }

    /// Set the maximum content length (in bytes) for which line information is computed.
    ///
    /// Content longer than this is not indexed and [`line_col`](Self::line_col) returns
    /// `None` for every position.
    pub fn set_max_scan_len(&mut self, max_scan_len: usize) {
        if self.computed_end == usize::MAX && self.content.len() > self.max_scan_len {
            // Indexing was previously abandoned because the content exceeded the old limit.
            // Reset in case the new limit allows computing line starts after all.
            if self.content.len() <= max_scan_len {
                self.line_starts = vec![0];
                self.computed_end = 0;
            }
        }
        self.max_scan_len = max_scan_len;
    }

    /// Extend the computed line starts to cover byte position `up_to`.
    fn extend_line_starts_up_to(&mut self, up_to: usize) {
        let new_computed_end = up_to + 1;

        if self.content.len() > self.max_scan_len {
            if self.computed_end == 0 {
                // Content too large to index; abandon (callers get `None` and fall back to
                // raw byte positions).
                self.line_starts.clear();
                self.computed_end = usize::MAX;
            }
            return;
        }

        if new_computed_end > self.computed_end {
            let start_from = self.computed_end;
            let end_at = self.max_scan_len.min(self.content.len());
            for (i, ch) in self.content[start_from..end_at].char_indices() {
                let abs_pos = start_from + i;
                if abs_pos >= new_computed_end {
                    break;
                }
                if ch == '\n' {
                    self.line_starts.push(abs_pos + 1);
                }
            }
            self.computed_end = new_computed_end;
        }
    }

    /// Get the (line, column) for a byte offset, using (and lazily extending) the cached
    /// line starts. Line and column numbers include the configured offsets.
    ///
    /// Returns `None` if the offset exceeds the content length, or if the content is longer
    /// than the maximum scan length (see [`set_max_scan_len`](Self::set_max_scan_len)).
    pub fn line_col(&mut self, byte_offset: usize) -> Option<(usize, usize)> {
        if byte_offset > self.content.len() {
            return None;
        }

        self.extend_line_starts_up_to(byte_offset);

        if byte_offset >= self.computed_end || self.line_starts.is_empty() {
            return None;
        }

        // Binary search to find the line containing this offset.
        let line_idx = match self.line_starts.binary_search(&byte_offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let line = line_idx + self.line_number_offset;
        let col = (byte_offset - self.line_starts[line_idx]) + self.column_number_offset;

        Some((line, col))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;

    #[test]
    fn single_line() {
        let mut index = LineIndex::new("Hello World");

        assert_eq!(index.line_col(0), Some((1, 1))); // 'H'
        assert_eq!(index.line_col(5), Some((1, 6))); // ' '
        assert_eq!(index.line_col(10), Some((1, 11))); // 'd'
    }

    #[test]
    fn multiline() {
        let mut index = LineIndex::new("Hello\nWorld\nTest");

        // Line 1
        assert_eq!(index.line_col(0), Some((1, 1))); // 'H'
        assert_eq!(index.line_col(3), Some((1, 4))); // second 'l'
        assert_eq!(index.line_col(5), Some((1, 6))); // '\n'

        // Line 2
        assert_eq!(index.line_col(6), Some((2, 1))); // 'W'
        assert_eq!(index.line_col(9), Some((2, 4))); // 'l'
        assert_eq!(index.line_col(11), Some((2, 6))); // '\n'

        // Line 3
        assert_eq!(index.line_col(12), Some((3, 1))); // 'T'
        assert_eq!(index.line_col(13), Some((3, 2))); // 'e'
    }

    #[test]
    fn reuses_line_info() {
        let mut index = LineIndex::new("Line1\nLine2\nLine3\nLine4");

        assert_eq!(index.line_col(0), Some((1, 1))); // Start of line 1
        assert_eq!(index.line_col(10), Some((2, 5))); // End of line 2

        // Queries within the already-computed region reuse cached line starts.
        assert_eq!(index.line_col(6), Some((2, 1))); // Start of line 2
        assert_eq!(index.line_col(15), Some((3, 4))); // Mid line 3
    }

    #[test]
    fn lazy_computation() {
        // Large source that we don't want to process all upfront.
        let content = "a\n".repeat(1000);
        let mut index = LineIndex::new(&content);

        assert_eq!(index.line_col(0), Some((1, 1)));
        assert_eq!(index.line_col(5), Some((3, 2)));

        // Line info was only computed up to position 5, not the entire 2000-byte string.
        assert_eq!(index.computed_end, 6);
    }

    #[test]
    fn zero_indexed_offsets() {
        let source: Source =
            Source::new("Hello\nWorld").with_line_column_number_offsets(0, 0);
        let mut index = source.line_index();

        // First line is line 0, first column is column 0.
        assert_eq!(index.line_col(0), Some((0, 0)));
        assert_eq!(index.line_col(5), Some((0, 5)));
        assert_eq!(index.line_col(6), Some((1, 0))); // Start of second line
    }

    #[test]
    fn custom_offsets() {
        let source: Source =
            Source::new("Hello\nWorld").with_line_column_number_offsets(10, 5);
        let mut index = source.line_index();

        assert_eq!(index.line_col(0), Some((10, 5))); // Line 0 + 10, col 0 + 5
        assert_eq!(index.line_col(6), Some((11, 5))); // Line 1 + 10, col 0 + 5
        assert_eq!(index.line_col(10), Some((11, 9))); // Line 1 + 10, col 4 + 5
    }

    #[test]
    fn source_too_long() {
        // 120 bytes exceeds a maximum scan length of 100.
        let content = "a\n".repeat(60);
        let mut index = LineIndex::new(&content);
        index.set_max_scan_len(100);

        assert_eq!(index.line_col(0), None);
        assert_eq!(index.line_col(100), None);
    }

    #[test]
    fn raising_max_scan_len_recovers() {
        let content = "a\n".repeat(60); // 120 bytes
        let mut index = LineIndex::new(&content);
        index.set_max_scan_len(100);
        assert_eq!(index.line_col(0), None);

        // Raising the limit re-enables indexing.
        index.set_max_scan_len(1000);
        assert_eq!(index.line_col(0), Some((1, 1)));
        assert_eq!(index.line_col(2), Some((2, 1)));
    }

    #[test]
    fn out_of_bounds() {
        let mut index = LineIndex::new("Hello");

        // Position beyond content length returns None.
        assert_eq!(index.line_col(100), None);

        // Valid positions still work afterwards.
        assert_eq!(index.line_col(0), Some((1, 1)));
        assert_eq!(index.line_col(4), Some((1, 5)));
    }
}
