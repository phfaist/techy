//! Content-backing abstraction and the scanning cursor.
//!
//! [`SourceContent`] is the trait boundary that decouples reading from the backing storage.
//! Only in-memory strings implement it today; it exists so that a memory-mapped backing can
//! be added later without changing cursor or parser code (DESIGN_RATIONALE.md §3.1,
//! "future huge files" — deliberately deferred).

use alloc::string::String;
use core::fmt;
use core::ops::Range;

/// Abstraction over the storage backing a source's content.
///
/// Implementations expose UTF-8 text by byte range. Like `std` string slicing, [`slice`]
/// panics on an out-of-bounds range or one that does not fall on `char` boundaries — callers
/// (the cursor, span types) maintain the boundary invariant.
///
/// [`slice`]: SourceContent::slice
pub trait SourceContent {
    /// Total content length in bytes.
    fn len(&self) -> usize;

    /// Whether the content is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow the given byte range as text.
    ///
    /// # Panics
    ///
    /// Panics if the range is out of bounds or not on `char` boundaries (same contract as
    /// `&s[range]` on `str`).
    fn slice(&self, range: Range<usize>) -> &str;
}

impl SourceContent for str {
    fn len(&self) -> usize {
        str::len(self)
    }

    fn slice(&self, range: Range<usize>) -> &str {
        &self[range]
    }
}

impl SourceContent for String {
    fn len(&self) -> usize {
        self.as_str().len()
    }

    fn slice(&self, range: Range<usize>) -> &str {
        &self.as_str()[range]
    }
}

/// Forward-scanning cursor over source content, with mark/rewind for small backtracks
/// (e.g. tentatively trying to parse `[` as an optional-argument delimiter).
///
/// The `'s` borrow is ephemeral — it lives for the duration of scanning one source unit and
/// never enters the AST (nodes store [`SourceSpan`](super::SourceSpan)s instead).
pub struct SourceCursor<'s, C: SourceContent + ?Sized = str> {
    content: &'s C,
    pos: usize,
}

impl<'s, C: SourceContent + ?Sized> SourceCursor<'s, C> {
    /// Create a cursor positioned at the start of `content`.
    pub fn new(content: &'s C) -> Self {
        SourceCursor { content, pos: 0 }
    }

    /// Current byte position.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Whether the cursor has reached the end of the content.
    pub fn is_eof(&self) -> bool {
        self.pos >= self.content.len()
    }

    /// The text from the current position to the end of the content.
    pub fn remaining(&self) -> &'s str {
        self.content.slice(self.pos..self.content.len())
    }

    /// The character at the current position, without advancing.
    pub fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// Consume and return the character at the current position.
    pub fn next_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    /// Advance the position by `n_bytes`.
    ///
    /// The new position must not exceed the content length and must fall on a `char`
    /// boundary (checked in debug builds).
    pub fn advance(&mut self, n_bytes: usize) {
        let new_pos = self.pos + n_bytes;
        debug_assert!(
            new_pos <= self.content.len(),
            "cursor advanced past end of content ({} > {})",
            new_pos,
            self.content.len(),
        );
        self.pos = new_pos;
    }

    /// Record the current position for a later [`rewind`](Self::rewind).
    pub fn mark(&self) -> usize {
        self.pos
    }

    /// Move back to a position previously obtained from [`mark`](Self::mark).
    ///
    /// Rewinding only moves backward (checked in debug builds).
    pub fn rewind(&mut self, mark: usize) {
        debug_assert!(mark <= self.pos, "rewind target {} is ahead of position {}", mark, self.pos);
        self.pos = mark;
    }
}

impl<C: SourceContent + ?Sized> Clone for SourceCursor<'_, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: SourceContent + ?Sized> Copy for SourceCursor<'_, C> {}

impl<C: SourceContent + ?Sized> fmt::Debug for SourceCursor<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const MAX_PREVIEW: usize = 40;
        let remaining = self.remaining();
        let preview: String = remaining.chars().take(MAX_PREVIEW).collect();
        write!(f, "SourceCursor(pos={}, remaining={:?}", self.pos, preview)?;
        if remaining.len() > preview.len() {
            write!(f, "…")?;
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_basic_scanning() {
        let mut cursor = SourceCursor::new("ab");
        assert_eq!(cursor.pos(), 0);
        assert!(!cursor.is_eof());
        assert_eq!(cursor.peek_char(), Some('a'));
        assert_eq!(cursor.pos(), 0); // peek does not advance

        assert_eq!(cursor.next_char(), Some('a'));
        assert_eq!(cursor.next_char(), Some('b'));
        assert!(cursor.is_eof());
        assert_eq!(cursor.peek_char(), None);
        assert_eq!(cursor.next_char(), None);
    }

    #[test]
    fn cursor_advance_and_remaining() {
        let mut cursor = SourceCursor::new("Hello World");
        cursor.advance(6);
        assert_eq!(cursor.remaining(), "World");
        assert_eq!(cursor.pos(), 6);
    }

    #[test]
    fn cursor_mark_and_rewind() {
        let mut cursor = SourceCursor::new("abc[def");
        cursor.advance(3);
        let mark = cursor.mark();

        // Tentatively scan ahead, then backtrack.
        assert_eq!(cursor.next_char(), Some('['));
        cursor.advance(3);
        assert!(cursor.is_eof());

        cursor.rewind(mark);
        assert_eq!(cursor.pos(), 3);
        assert_eq!(cursor.peek_char(), Some('['));
    }

    #[test]
    fn cursor_multibyte_chars() {
        let mut cursor = SourceCursor::new("é→x");
        assert_eq!(cursor.next_char(), Some('é'));
        assert_eq!(cursor.pos(), 2); // 'é' is 2 bytes
        assert_eq!(cursor.next_char(), Some('→'));
        assert_eq!(cursor.pos(), 5); // '→' is 3 bytes
        assert_eq!(cursor.next_char(), Some('x'));
        assert!(cursor.is_eof());
    }

    #[test]
    fn string_content_impl() {
        let content = String::from("Hello");
        assert_eq!(SourceContent::len(&content), 5);
        assert!(!SourceContent::is_empty(&content));
        assert_eq!(content.slice(1..3), "el");

        let cursor = SourceCursor::new(&content);
        assert_eq!(cursor.remaining(), "Hello");
    }
}
