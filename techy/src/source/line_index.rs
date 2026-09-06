//! Line and column analysis: the borrowing [`LineIndex`], the persistent
//! [`LineIndexCache`], and the [`LineColProvider`] trait the rendering entry points
//! accept.
//!
//! Parsing works purely with byte offsets. Line and column numbers are computed only on
//! demand, for display in error messages and diagnostics, and a [`Source`] never computes
//! or stores them itself. Use a [`LineIndex`] for a handful of queries against one
//! content string, and a [`LineIndexCache`] to keep the computed line information across
//! many renders or several parses — source content is immutable, so a cache entry never
//! becomes stale.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use super::origin::SourceOrigin;
use super::source::Source;

/// A lazily computed table of line starts over a piece of source content.
///
/// Line starts are computed incrementally, only up to the largest byte offset queried so
/// far, so indexing a large source costs nothing until positions near its end are actually
/// displayed. [`Source::line_index`](super::Source::line_index) creates one already
/// configured with that source's line and column number offsets.
///
/// A `LineIndex` borrows the content and is meant to be used and dropped; the owning,
/// per-source form that a consumer keeps is [`LineIndexCache`].
///
/// The three queries are [`line_col`](Self::line_col) for a position,
/// [`line_of`](Self::line_of) for the containing line and its byte range, and
/// [`line_col_span`](Self::line_col_span) for both ends of a range.
///
/// To bound memory use on very large inputs, content longer than the configured maximum
/// scan length (500 000 bytes by default, see
/// [`set_max_scan_len`](Self::set_max_scan_len)) is not indexed at all: every query then
/// returns `None`, and callers fall back to displaying raw byte positions.
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

/// Default maximum content length (in bytes) for which line information is computed
/// (adjustable per index via [`LineIndex::set_max_scan_len`]). 500 000 bytes keeps
/// the line-starts table bounded (about 100 KB for a 500 KB text of short lines) while
/// covering ordinary documents; past it, queries answer `None` silently and renderers
/// fall back to raw byte positions.
const DEFAULT_MAX_SCAN_LEN: usize = 500_000;

impl<'c> LineIndex<'c> {
    /// Creates a line index over `content`, with the default line and column number
    /// offsets `(1, 1)`.
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

    /// Sets the line and column number offsets (see
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

    /// Sets the maximum content length, in bytes, for which line information is computed.
    /// The default is 500 000 bytes.
    ///
    /// Content longer than this is not indexed at all, and the queries return `None` for
    /// every position. Raising the limit above the content length makes this index compute
    /// line starts after all, on the next query.
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
        // The resume point must stay on a char boundary: `up_to + 1` may fall inside
        // a multi-byte character, and the next call resumes by slicing
        // `content[computed_end..]`. Advance to the next boundary. The loop leaves
        // past-the-end values alone; that is safe because the resume point is at
        // most `len + 1` (`line_index_of` rejects `byte_offset > content.len()`,
        // so `up_to <= len`), and a stored `len + 1` never becomes a slice start:
        // any later call computes `new_computed_end <= len + 1`, which the
        // `new_computed_end > self.computed_end` guard below rejects.
        let mut new_computed_end = up_to + 1;
        while new_computed_end < self.content.len()
            && !self.content.is_char_boundary(new_computed_end)
        {
            new_computed_end += 1;
        }

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

    /// The line and column of a byte offset, extending the computed line starts as needed.
    ///
    /// Both numbers include the configured offsets, added with saturating arithmetic: an
    /// offset near `usize::MAX` yields `usize::MAX` rather than overflowing.
    ///
    /// Returns `None` if the offset is past the end of the content, or if the content is
    /// longer than the maximum scan length (see
    /// [`set_max_scan_len`](Self::set_max_scan_len)).
    pub fn line_col(&mut self, byte_offset: usize) -> Option<(usize, usize)> {
        let line_idx = self.line_index_of(byte_offset)?;
        let line = line_idx.saturating_add(self.line_number_offset);
        let col = (byte_offset - self.line_starts[line_idx]).saturating_add(self.column_number_offset);
        Some((line, col))
    }

    /// The line containing a byte offset: its line number and its byte range.
    ///
    /// This is what a renderer needs to draw a caret or an underline — slice the range out
    /// of the content for the line's text, then point at the offset within it. The line
    /// number uses the same numbering as [`line_col`](Self::line_col), configured offsets
    /// included.
    ///
    /// The range excludes the line terminator `\n`, and the last line ends at the end of
    /// the content. An offset sitting on a `\n`, or at the end of the content, belongs to
    /// the line it terminates, so `byte_offset` may equal the range's end.
    ///
    /// Returns `None` under the same conditions as [`line_col`](Self::line_col): the
    /// offset is out of bounds, or the content is past the maximum scan length.
    pub fn line_of(&mut self, byte_offset: usize) -> Option<(usize, Range<usize>)> {
        let line_idx = self.line_index_of(byte_offset)?;
        Some((
            line_idx.saturating_add(self.line_number_offset),
            line_range_from(self.content, self.line_starts[line_idx]),
        ))
    }

    /// The line and column of both ends of a byte range, which may be given as a
    /// `Range<usize>` or as a plain [`Span`](super::Span).
    ///
    /// The second pair is the position of the range's exclusive end — one past its last
    /// byte — following the [`SourceSpan::end_pos`](super::SourceSpan::end_pos)
    /// convention.
    ///
    /// Returns `None` whenever either end has no answer (out of bounds, or content past
    /// the maximum scan length); it never returns half an answer.
    pub fn line_col_span(
        &mut self,
        range: impl Into<Range<usize>>,
    ) -> Option<((usize, usize), (usize, usize))> {
        let range = range.into();
        let start = self.line_col(range.start)?;
        let end = self.line_col(range.end)?;
        Some((start, end))
    }

    /// The shared front of the queries: bounds check, lazy extension, and the
    /// line-starts search for the (zero-based) line index containing `byte_offset`.
    fn line_index_of(&mut self, byte_offset: usize) -> Option<usize> {
        if byte_offset > self.content.len() {
            return None;
        }
        self.extend_line_starts_up_to(byte_offset);
        if byte_offset >= self.computed_end || self.line_starts.is_empty() {
            return None;
        }
        Some(line_index_in_starts(&self.line_starts, byte_offset))
    }
}

/// Binary-search a line-starts table for the (zero-based) index of the line
/// containing `byte_offset` — shared by [`LineIndex`] and [`LineIndexCache`].
fn line_index_in_starts(line_starts: &[usize], byte_offset: usize) -> usize {
    match line_starts.binary_search(&byte_offset) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1),
    }
}

/// The byte range of the line starting at `line_start`, excluding the `\n`
/// terminator (the last line ends at the content end).
fn line_range_from(content: &str, line_start: usize) -> Range<usize> {
    let end = content[line_start..]
        .find('\n')
        .map(|i| line_start + i)
        .unwrap_or(content.len());
    line_start..end
}

/// The full line-starts table of `content` (position 0 plus every position after a
/// `\n`), as [`LineIndexCache`] entries own it.
fn compute_line_starts(content: &str) -> Vec<usize> {
    let mut line_starts = vec![0];
    for (i, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            line_starts.push(i + 1);
        }
    }
    line_starts
}

/// Answers line and column queries for byte offsets into sources.
///
/// This is the trait the diagnostic rendering entry points accept: `render_with`,
/// `render_all_with`, [`format_position_with`](crate::error::format_position_with) and
/// [`format_traceback_with`](crate::error::format_traceback_with) (their no-argument forms
/// use a temporary cache instead).
///
/// [`LineIndexCache`] is the implementation this crate provides. An editor that already
/// maintains its own incremental line table implements the trait over that table instead,
/// and its line information then survives the re-parses that create new `Source` values on
/// every keystroke.
///
/// Where an answer comes from — a cache, a precomputed table, or a fresh scan — is the
/// implementation's business, but answers must follow the conventions of
/// [`LineIndex::line_col`]: the source's configured line and column number offsets are
/// included, and `None` means no answer is available.
pub trait LineColProvider<O: SourceOrigin = Option<String>> {
    /// The line and column of `byte_offset` within `source`, or `None` when no answer is
    /// available.
    ///
    /// There is deliberately no error channel besides the `Option`: `None` is the
    /// no-answer answer, whatever the reason, and the caller falls back to displaying raw
    /// byte positions. An implementation whose own line table fails internally reports
    /// that failure through its own channel and returns `None` here.
    fn line_col(
        &mut self,
        source: &Arc<Source<O>>,
        byte_offset: usize,
    ) -> Option<(usize, usize)>;
}

/// A persistent line and column cache holding one owned table of line starts per source,
/// keyed by source identity.
///
/// This is the owning counterpart of the borrowing [`LineIndex`], and the implementation
/// of [`LineColProvider`] this crate provides. Pass one to the `_with` rendering entry
/// points of [`error`](crate::error) to index each source once instead of once per call.
///
/// Because source content is immutable, an entry never becomes stale: a tool that keeps
/// its own `Arc<Source>` across parse attempts — the span-stability rule described in
/// [`Source::new`](super::Source::new) — keeps its cache valid for free. Since a query
/// may build an entry, the queries take `&mut self`; sharing one cache between threads is
/// the consumer's own concern, as this crate provides no `no_std` synchronization.
///
/// The queries mirror those of [`LineIndex`], with the source as first argument:
/// [`line_col`](Self::line_col), [`line_of`](Self::line_of) and
/// [`line_col_span`](Self::line_col_span), each applying that source's own line and column
/// number offsets. Content longer than the maximum scan length
/// ([`set_max_scan_len`](Self::set_max_scan_len); 500 000 bytes by default) is not
/// indexed, and queries about it return `None`.
///
/// Entries are found by a linear scan, since a report touches few distinct sources.
#[derive(Debug, Clone, Default)]
pub struct LineIndexCache<O: SourceOrigin = Option<String>> {
    entries: Vec<CacheEntry<O>>,
    max_scan_len: Option<usize>,
}

#[derive(Debug, Clone)]
struct CacheEntry<O: SourceOrigin> {
    source: Arc<Source<O>>,
    /// The owned line-starts table; `None` when the content exceeded the scan cap
    /// at (re)build time.
    line_starts: Option<Vec<usize>>,
}

impl<O: SourceOrigin> LineIndexCache<O> {
    /// Creates an empty cache with the default maximum scan length.
    pub fn new() -> LineIndexCache<O> {
        LineIndexCache { entries: Vec::new(), max_scan_len: None }
    }

    /// Sets the maximum content length, in bytes, for which line information is computed.
    /// The default is 500 000 bytes.
    ///
    /// This is [`LineIndex::set_max_scan_len`] applied to every source in this cache. A
    /// source previously skipped for exceeding the limit is indexed on its next query if a
    /// raised limit now admits it.
    pub fn set_max_scan_len(&mut self, max_scan_len: usize) {
        self.max_scan_len = Some(max_scan_len);
    }

    fn max_scan_len(&self) -> usize {
        self.max_scan_len.unwrap_or(DEFAULT_MAX_SCAN_LEN)
    }

    /// The entry for `source` (found by `Arc` identity), built — or re-admitted
    /// under a raised cap — on first touch.
    fn entry_index(&mut self, source: &Arc<Source<O>>) -> usize {
        let admissible = source.content().len() <= self.max_scan_len();
        if let Some(i) =
            self.entries.iter().position(|entry| Arc::ptr_eq(&entry.source, source))
        {
            if self.entries[i].line_starts.is_none() && admissible {
                self.entries[i].line_starts =
                    Some(compute_line_starts(source.content()));
            }
            return i;
        }
        self.entries.push(CacheEntry {
            source: Arc::clone(source),
            line_starts: admissible.then(|| compute_line_starts(source.content())),
        });
        self.entries.len() - 1
    }

    /// The line and column of `byte_offset` within `source`.
    ///
    /// This is [`LineIndex::line_col`] answered from the cached table, with the same
    /// conventions and `source`'s own configured offsets.
    pub fn line_col(
        &mut self,
        source: &Arc<Source<O>>,
        byte_offset: usize,
    ) -> Option<(usize, usize)> {
        let i = self.entry_index(source);
        let entry = &self.entries[i];
        if byte_offset > entry.source.content().len() {
            return None;
        }
        let line_starts = entry.line_starts.as_ref()?;
        let line_idx = line_index_in_starts(line_starts, byte_offset);
        Some((
            line_idx.saturating_add(entry.source.line_number_offset()),
            (byte_offset - line_starts[line_idx]).saturating_add(entry.source.column_number_offset()),
        ))
    }

    /// The line containing `byte_offset` within `source`: its line number and its byte
    /// range.
    ///
    /// This is [`LineIndex::line_of`] answered from the cached table.
    pub fn line_of(
        &mut self,
        source: &Arc<Source<O>>,
        byte_offset: usize,
    ) -> Option<(usize, Range<usize>)> {
        let i = self.entry_index(source);
        let entry = &self.entries[i];
        if byte_offset > entry.source.content().len() {
            return None;
        }
        let line_starts = entry.line_starts.as_ref()?;
        let line_idx = line_index_in_starts(line_starts, byte_offset);
        Some((
            line_idx.saturating_add(entry.source.line_number_offset()),
            line_range_from(entry.source.content(), line_starts[line_idx]),
        ))
    }

    /// The line and column of both ends of a byte range within `source`.
    ///
    /// This is [`LineIndex::line_col_span`] answered from the cached table.
    pub fn line_col_span(
        &mut self,
        source: &Arc<Source<O>>,
        range: impl Into<Range<usize>>,
    ) -> Option<((usize, usize), (usize, usize))> {
        let range = range.into();
        let start = self.line_col(source, range.start)?;
        let end = self.line_col(source, range.end)?;
        Some((start, end))
    }
}

impl<O: SourceOrigin> LineColProvider<O> for LineIndexCache<O> {
    fn line_col(
        &mut self,
        source: &Arc<Source<O>>,
        byte_offset: usize,
    ) -> Option<(usize, usize)> {
        LineIndexCache::line_col(self, source, byte_offset)
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

    /// Offsets are added with saturating arithmetic: the largest offsets (what a
    /// 32-bit reader gets from a serialized source declaring `4294967295`, say) yield
    /// `usize::MAX`, never an overflow — through the transient index and the cache.
    #[test]
    fn extreme_offsets_saturate_instead_of_overflowing() {
        let source: Source =
            Source::new("Hello\nWorld").with_line_column_number_offsets(usize::MAX, usize::MAX - 2);
        let mut index = source.line_index();
        assert_eq!(index.line_col(0), Some((usize::MAX, usize::MAX - 2)));
        assert_eq!(index.line_col(6), Some((usize::MAX, usize::MAX - 2)));
        assert_eq!(index.line_col(10), Some((usize::MAX, usize::MAX)));
        assert_eq!(index.line_of(7).map(|(line, _)| line), Some(usize::MAX));
        assert_eq!(
            index.line_col_span(0..10),
            Some(((usize::MAX, usize::MAX - 2), (usize::MAX, usize::MAX)))
        );
        let source = Arc::new(source);
        let mut cache = LineIndexCache::new();
        assert_eq!(cache.line_col(&source, 10), Some((usize::MAX, usize::MAX)));
        assert_eq!(cache.line_of(&source, 10).map(|(line, _)| line), Some(usize::MAX));
        assert_eq!(cache.line_col_span(&source, 0..1), Some(((usize::MAX, usize::MAX - 2), (usize::MAX, usize::MAX - 1))));
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

    #[test]
    fn multibyte_first_character_then_later_query() {
        // Regression: `line_col(k)` stored `k + 1` as the resume point and the next
        // call sliced `content[k + 1..]` — a panic whenever byte `k` began a
        // multi-byte character. Offset 0 first, a larger offset second is the
        // `display_tree` query order.
        let mut index = LineIndex::new("é—x\ny");
        assert_eq!(index.line_col(0), Some((1, 1)));
        assert_eq!(index.line_col(5), Some((1, 6))); // 'x' (columns count bytes)
        assert_eq!(index.line_col(7), Some((2, 1))); // 'y'
    }

    #[test]
    fn multibyte_incremental_queries_agree_with_a_precomputed_table() {
        // The oracle is `LineIndexCache`, which computes each source's full
        // line-starts table in one pass — independent of the lazy extension
        // under test, so a bug shared by two lazily-extended indexes cannot
        // hide. Both query orders: ascending grows the lazy index step by step,
        // descending answers every later query from one big first extension.
        for content in ["é", "—x{y}", "😀", "aé\nb—c\n😀"] {
            let source = arc_source(content);
            let mut cache: LineIndexCache = LineIndexCache::new();

            let mut ascending = LineIndex::new(content);
            for off in 0..=content.len() {
                assert_eq!(
                    ascending.line_col(off),
                    cache.line_col(&source, off),
                    "ascending, offset {off} of {content:?}"
                );
            }

            let mut descending = LineIndex::new(content);
            for off in (0..=content.len()).rev() {
                assert_eq!(
                    descending.line_col(off),
                    cache.line_col(&source, off),
                    "descending, offset {off} of {content:?}"
                );
            }
        }
    }

    // --- line_of / line_col_span ------------------------------------------------------

    #[test]
    fn line_of_returns_the_line_number_and_terminator_free_range() {
        let mut index = LineIndex::new("Hello\nWorld\nTest");

        // First line: range excludes the `\n`.
        assert_eq!(index.line_of(0), Some((1, 0..5)));
        assert_eq!(index.line_of(3), Some((1, 0..5)));
        // An offset on the `\n` belongs to the line it terminates (== range end).
        assert_eq!(index.line_of(5), Some((1, 0..5)));
        // Middle line.
        assert_eq!(index.line_of(6), Some((2, 6..11)));
        // Last line (no trailing newline): range ends at content end; the
        // end-of-content offset is a valid position on it.
        assert_eq!(index.line_of(12), Some((3, 12..16)));
        assert_eq!(index.line_of(16), Some((3, 12..16)));
        // Out of bounds.
        assert_eq!(index.line_of(17), None);
    }

    #[test]
    fn line_of_trailing_newline_and_empty_source() {
        // A trailing newline opens a final empty line.
        let mut index = LineIndex::new("ab\n");
        assert_eq!(index.line_of(2), Some((1, 0..2)));
        assert_eq!(index.line_of(3), Some((2, 3..3)));

        // The empty source has one empty line.
        let mut empty = LineIndex::new("");
        assert_eq!(empty.line_of(0), Some((1, 0..0)));
        assert_eq!(empty.line_col(0), Some((1, 1)));
    }

    #[test]
    fn line_of_applies_the_configured_offsets() {
        let source: Source =
            Source::new("Hello\nWorld").with_line_column_number_offsets(0, 0);
        let mut index = source.line_index();
        assert_eq!(index.line_of(6), Some((1, 6..11)));
    }

    #[test]
    fn line_col_span_answers_both_ends_or_neither() {
        let mut index = LineIndex::new("ab\ncd\nef");
        // 1..7 covers "b\ncd\ne": start on line 1, exclusive end on line 3.
        assert_eq!(index.line_col_span(1..7), Some(((1, 2), (3, 2))));
        // The whole content: the exclusive end is one past the last byte.
        assert_eq!(index.line_col_span(0..8), Some(((1, 1), (3, 3))));
        // An out-of-bounds end: no half answers.
        assert_eq!(index.line_col_span(0..99), None);
    }

    // --- LineIndexCache + LineColProvider ---------------------------------------------

    fn arc_source(content: &str) -> Arc<Source> {
        Arc::new(Source::new(content))
    }

    #[test]
    fn cache_agrees_with_the_fresh_index() {
        let source = arc_source("Hello\nWorld\nTest");
        let mut cache: LineIndexCache = LineIndexCache::new();
        // First query builds the entry; the second answers from it — both must
        // agree with a fresh LineIndex at every offset.
        for _ in 0..2 {
            let mut fresh = source.line_index();
            for offset in 0..=source.content().len() {
                assert_eq!(cache.line_col(&source, offset), fresh.line_col(offset));
                assert_eq!(cache.line_of(&source, offset), fresh.line_of(offset));
            }
            assert_eq!(
                cache.line_col_span(&source, 1..7),
                fresh.line_col_span(1..7)
            );
        }
        // Out of bounds answers None, like the fresh index.
        assert_eq!(cache.line_col(&source, 99), None);
        assert_eq!(cache.line_of(&source, 99), None);
    }

    #[test]
    fn cache_entries_are_per_source_by_arc_identity() {
        // Same content, different Arcs, different offset conventions: each source
        // gets its own entry answering under its own configuration — identity
        // keying, no content-based sharing.
        let one_based = arc_source("ab\ncd");
        let zero_based: Arc<Source> =
            Arc::new(Source::new("ab\ncd").with_line_column_number_offsets(0, 0));
        let mut cache: LineIndexCache = LineIndexCache::new();
        assert_eq!(cache.line_col(&one_based, 3), Some((2, 1)));
        assert_eq!(cache.line_col(&zero_based, 3), Some((1, 0)));
        // Interleaved queries keep the isolation.
        assert_eq!(cache.line_col(&one_based, 0), Some((1, 1)));
        assert_eq!(cache.line_col(&zero_based, 0), Some((0, 0)));
    }

    #[test]
    fn cache_respects_and_readmits_on_the_scan_cap() {
        let source = arc_source("a\nb\nc");
        let mut cache: LineIndexCache = LineIndexCache::new();
        cache.set_max_scan_len(3);
        // Content (5 bytes) exceeds the cap: no line info.
        assert_eq!(cache.line_col(&source, 0), None);
        // Raising the cap re-admits the source on its next query.
        cache.set_max_scan_len(1000);
        assert_eq!(cache.line_col(&source, 2), Some((2, 1)));
    }

    #[test]
    fn line_index_cache_is_a_line_col_provider() {
        fn through_the_seam<O: SourceOrigin>(
            line_cols: &mut impl LineColProvider<O>,
            source: &Arc<Source<O>>,
        ) -> Option<(usize, usize)> {
            line_cols.line_col(source, 3)
        }
        let source = arc_source("ab\ncd");
        let mut cache: LineIndexCache = LineIndexCache::new();
        assert_eq!(through_the_seam(&mut cache, &source), Some((2, 1)));
    }
}
