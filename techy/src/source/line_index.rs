//! Lazy line/column analysis: the transient [`LineIndex`] view, the persistent
//! consumer-held [`LineIndexCache`], and the [`LineColProvider`] seam the
//! rendering entry points accept.
//!
//! The parser works purely with byte offsets; line/column information is computed only on
//! demand, for display (error messages, diagnostics). This is the standalone successor of
//! the earlier `SourceLocationAnalyzer` (its lazy line-start extension logic is preserved;
//! the traceback formatting moved to the `error` module).
//!
//! **Who computes and caches is layered** — never the `Source`: the parse computes
//! nothing; the diagnostics renderer keeps a per-call cache; *persistence* belongs
//! to whoever holds a [`LineIndexCache`] across renders (or parses — source content
//! is immutable, so a cache entry never invalidates). A `Source`-owned lazy cache
//! stays rejected dependency-free (`alloc` has no `Mutex`; `OnceCell` would cost
//! `Sync`).

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use super::origin::SourceOrigin;
use super::source::Source;

/// Lazily computed line-start index over a piece of source content.
///
/// Line starts are computed incrementally, only up to the largest byte offset queried so
/// far — indexing a large source costs nothing until (and unless) positions near its end are
/// actually displayed. Obtain one preconfigured with a source's line/column offsets via
/// [`Source::line_index`](super::Source::line_index). This is the borrowing, transient
/// view; the owning, per-source persistent form is [`LineIndexCache`].
///
/// To bound memory use on huge inputs, content longer than the configured maximum scan
/// length (default 500 000 bytes, see [`set_max_scan_len`](Self::set_max_scan_len)) is not
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

/// Default maximum content length (in bytes) for which line information is computed
/// (adjustable per index via [`LineIndex::set_max_scan_len`]). 500 000 bytes keeps
/// the line-starts table bounded (≈ 100 KB for a 500 KB text of short lines) while
/// covering ordinary documents; past it, queries answer `None` **silently** and
/// renderers fall back to raw byte positions.
const DEFAULT_MAX_SCAN_LEN: usize = 500_000;

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

    /// Set the maximum content length (in bytes) for which line information is computed
    /// (default: 500 000 bytes).
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

    /// Get the (line, column) for a byte offset, using (and lazily extending) the cached
    /// line starts. Line and column numbers include the configured offsets.
    ///
    /// Returns `None` if the offset exceeds the content length, or if the content is longer
    /// than the maximum scan length (see [`set_max_scan_len`](Self::set_max_scan_len)).
    pub fn line_col(&mut self, byte_offset: usize) -> Option<(usize, usize)> {
        let line_idx = self.line_index_of(byte_offset)?;
        let line = line_idx + self.line_number_offset;
        let col = (byte_offset - self.line_starts[line_idx]) + self.column_number_offset;
        Some((line, col))
    }

    /// Get the line containing a byte offset: its line number (the
    /// [`line_col`](Self::line_col) numbering, configured offsets included) **and**
    /// its byte range — the caret/underline rendering path: slice the range for the
    /// line's text, then point at the offset within it.
    ///
    /// The range excludes the line terminator (`\n`); the last line ends at the
    /// content end. An offset sitting *on* a `\n` (or at end of content) belongs to
    /// the line it terminates, so `byte_offset` may equal the range's end.
    ///
    /// `None` under the same conditions as [`line_col`](Self::line_col) (offset out
    /// of bounds, or content past the scan cap).
    pub fn line_of(&mut self, byte_offset: usize) -> Option<(usize, Range<usize>)> {
        let line_idx = self.line_index_of(byte_offset)?;
        Some((
            line_idx + self.line_number_offset,
            line_range_from(self.content, self.line_starts[line_idx]),
        ))
    }

    /// Get the (line, column) pair of both ends of a byte range (a `Range<usize>`
    /// or a plain [`Span`](super::Span)): the start's position and the **exclusive**
    /// end's position — one past the range's last byte, the
    /// [`SourceSpan::end_pos`](super::SourceSpan::end_pos) convention.
    ///
    /// `None` when either end has no answer (out of bounds, or content past the
    /// scan cap) — never a half answer.
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

/// Answers line/column queries for offsets into sources — the trait the rendering
/// entry points accept (`render_with`/`render_all_with`/
/// [`format_position_with`](crate::error::format_position_with)/
/// [`format_traceback_with`](crate::error::format_traceback_with); the no-argument
/// forms are transient-cache shorthand). [`LineIndexCache`] is the shipped
/// implementation; editor tools with their own incremental line tables — surviving
/// per-keystroke re-parses that mint new `Source`s — implement the trait over
/// them and plug in without recomputation.
///
/// The trait provides line/col *answers*; whether they come from a cache, a
/// precomputed table, or a scan is the implementation's business. Answers must
/// follow [`LineIndex::line_col`]'s conventions (the source's configured
/// line/column number offsets; `None` = no answer, the caller falls back to raw
/// byte positions).
pub trait LineColProvider<O: SourceOrigin = Option<String>> {
    /// The (line, column) of `byte_offset` within `source`, or `None` when no
    /// answer is available (see the trait docs).
    fn line_col(
        &mut self,
        source: &Arc<Source<O>>,
        byte_offset: usize,
    ) -> Option<(usize, usize)>;
}

/// A persistent line/column cache: **one owned line-starts table per source**,
/// keyed by `Arc` identity — the consumer-held counterpart of the borrowing
/// [`LineIndex`] view.
///
/// Because source content is immutable, an entry never invalidates: a tool that
/// keeps its own `Arc<Source>` across parse attempts (the span-stability
/// rule) keeps its cache valid for free, and one cache threaded through many
/// renders (`render_all_with`, repeated `render_with` calls) indexes each source
/// once instead of once per call. Consumer-held means `&mut self` is the honest
/// receiver; sharing across threads is the consumer's own lock (techy buys no
/// `no_std` synchronization).
///
/// The API mirrors the [`LineIndex`] queries with the source as first argument —
/// [`line_col`](Self::line_col), [`line_of`](Self::line_of),
/// [`line_col_span`](Self::line_col_span) — and applies each source's own
/// line/column number offsets. Content past the scan cap
/// ([`set_max_scan_len`](Self::set_max_scan_len); default 500 000 bytes) is not
/// indexed — queries answer `None` and renderers fall back to raw byte positions.
/// Entries are found by a linear scan (a report touches few distinct sources).
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
    /// An empty cache with the default scan cap.
    pub fn new() -> LineIndexCache<O> {
        LineIndexCache { entries: Vec::new(), max_scan_len: None }
    }

    /// Set the maximum content length (in bytes) for which line information is
    /// computed (default: 500 000 bytes) — the [`LineIndex::set_max_scan_len`]
    /// setting, cache-wide. A source previously skipped for exceeding the cap is
    /// re-admitted on its next query if the raised cap allows it.
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

    /// The (line, column) of `byte_offset` within `source` —
    /// [`LineIndex::line_col`] against the cached table (same conventions, the
    /// source's configured offsets included).
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
            line_idx + entry.source.line_number_offset(),
            (byte_offset - line_starts[line_idx]) + entry.source.column_number_offset(),
        ))
    }

    /// The line containing `byte_offset` within `source`: line number plus the
    /// line's byte range — [`LineIndex::line_of`] against the cached table.
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
            line_idx + entry.source.line_number_offset(),
            line_range_from(entry.source.content(), line_starts[line_idx]),
        ))
    }

    /// The (line, column) pair of both ends of a byte range within `source` —
    /// [`LineIndex::line_col_span`] against the cached table.
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
