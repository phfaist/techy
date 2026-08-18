//! [`ScriptedReader`]: a [`TokenReader`] that serves one parse from several sources,
//! following a script written by the test.
//!
//! **Internal test infrastructure** — compiled under `cfg(test)` only, deliberately not
//! public API. Its purpose is to exercise the parsing machinery of a language that
//! declares [`Lang::OBEYS_SPAN_TILING`] `= false` without implementing a macro expander:
//! the reader serves a token stream whose tokens come from several sources, in an order
//! and with gaps the test chooses.
//!
//! # The script
//!
//! A script is a list of **segments**. A segment is a source together with a byte range
//! of it; the reader tokenizes each segment with [`StdTokenReader`] under a
//! [`ParsingState`] given once at construction, and the tokens of all segments,
//! concatenated in order, are the stream. Each token of the stream is one **entry** of
//! the script, and entries are numbered from zero.
//!
//! Segments may name the same source more than once and may leave holes:
//!
//! - `A[0..2], B[0..3]` chains one source after another (one seam, between the two).
//! - `A[0..1], B[0..3], A[5..7]` splices `B` into the middle of `A` and skips `A[1..5]`
//!   (two seams, and a hole in `A`).
//! - `A[0..1], A[5..6]` reads one source with a hole in it (no seam: one source
//!   throughout).
//!
//! A **seam** is a place in the stream where the next token comes from a different
//! source than the previous one ([`TokenReader`]'s contract, section *Seams*).
//!
//! Only the last segment contributes an [`EndOfStream`](TokenKind::EndOfStream) token:
//! end of stream is the end of the *whole* stream, so a middle segment's is dropped.
//!
//! # Positions
//!
//! A [`ScriptedPosition`] is an entry index together with a [`TokenEdge`], kept in a
//! **canonical form** so that two positions compare equal exactly when they name the
//! same place in the stream:
//!
//! - the place past an entry is the place before the following entry: `(i,
//!   EndPastPostSpace)` is stored as `(i + 1, StartBeforePreSpace)`. This is what makes
//!   contract clauses 2 and 7 hold at a seam by construction — the two sides of a seam
//!   are one value, not two.
//! - within one entry, edges that fall on the same offset name one place, and are stored
//!   as the earliest of them: for a token with no pre-space, `(i, Start)` is stored as
//!   `(i, StartBeforePreSpace)`.
//!
//! The place past the last entry is `(n, StartBeforePreSpace)`, where `n` is the number
//! of entries.
//!
//! # Fidelity gaps
//!
//! The script is fixed, which a scanning reader's stream is not. Two consequences, both
//! shared with [`TokenListReader`](super::TokenListReader):
//!
//! - Tokens are not re-scanned under the state passed to
//!   [`peek`](TokenReader::peek): a state transition mid-parse does not change what the
//!   remaining entries are.
//! - A position inside an entry's token proper — at
//!   [`ContentStart`](TokenEdge::ContentStart) or [`End`](TokenEdge::End), which the
//!   `\verb` idiom moves to — is not a place the script can be read from. Peeking there
//!   yields the *following* entry, so the bytes between the position and that entry
//!   (a command's post-space, say) are not read again as content. Contract clause 2
//!   still holds: the token reports the position it was peeked at as its
//!   [`StartBeforePreSpace`](TokenEdge::StartBeforePreSpace) edge.
//!
//! # Panics
//!
//! This module asserts where a test's own script is wrong — a segment that ends inside
//! a token, a middle segment that ends in whitespace (its bytes would be dropped
//! silently), content that does not tokenize, a token or position from another reader.
//! These are mistakes in test code, which is what makes a panic the right report; the
//! crate's library code follows the no-panic policy unchanged.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::engine::StdParseDriver;
use crate::node::{NodeBuildError, NodeKind, StagedChildren};
use crate::source::{Source, SourcePos, SourceSpan, Span};
use crate::state::{AllLangFeatures, Lang, ParsingState};

use super::error::TokenResult;
use super::reader::{StdTokenReader, TokenEdge, TokenReader};
use super::token::{StdToken, TokenKind};
use super::tokenization::{StreamPosition, Token, Tokenization};

/// One segment of a script: a source, and the byte range of it to tokenize.
type ScriptSegment<'s, L> = (&'s Arc<Source<<L as Lang>::SourceOrigin>>, Range<usize>);

/// The five edges in reading order, for the canonical-form search.
const EVERY_EDGE: [TokenEdge; 5] = [
    TokenEdge::StartBeforePreSpace,
    TokenEdge::Start,
    TokenEdge::ContentStart,
    TokenEdge::End,
    TokenEdge::EndPastPostSpace,
];

/// The tokenization of a language read by a [`ScriptedReader`]: its own token and
/// stream-position types, and no language-side reader worth the name.
///
/// A script is runtime data, and [`make_token_reader`](Tokenization::make_token_reader)
/// receives only a source, so the language-side reader it builds serves an *empty*
/// stream over that source (one [`EndOfStream`](TokenKind::EndOfStream) token, no
/// content). Tests build the reader they mean with [`ScriptedReader::new`] and drive
/// parsers with it directly, or hand it out from a
/// [`ParseDriver::make_token_reader`](crate::engine::ParseDriver::make_token_reader)
/// override that holds the script.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScriptedTokenization;

impl<L: Lang<Tokenization = ScriptedTokenization>> Tokenization<L> for ScriptedTokenization {
    type Token = ScriptedToken<L>;
    type StreamPosition = ScriptedPosition;

    fn make_token_reader<'s>(
        source: &'s Arc<Source<L::SourceOrigin>>,
    ) -> Box<dyn TokenReader<'s, L> + 's> {
        Box::new(ScriptedReader::empty_script(source))
    }
}

/// A place in a scripted stream: an entry index and an edge of that entry, in canonical
/// form (see the [module docs](self#positions)).
///
/// `entry` runs from `0` to the number of entries; the last value names the place past
/// the last entry, always with edge
/// [`StartBeforePreSpace`](TokenEdge::StartBeforePreSpace).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScriptedPosition {
    entry: usize,
    edge: TokenEdge,
}

impl ScriptedPosition {
    /// Reading order over canonical positions: entry index first, then edge order
    /// (which is reading order within an entry).
    fn precedes_or_equals(&self, other: &ScriptedPosition) -> bool {
        (self.entry, self.edge) <= (other.entry, other.edge)
    }
}

/// A token of a scripted stream: which entry it is, which source it was scanned from,
/// where it was peeked, and the standard token the scan produced.
///
/// Opaque like every token ([`Tokenization::Token`]): only the reader that issued it
/// reads anything off it.
pub(crate) struct ScriptedToken<L: Lang> {
    /// The entry this token is, or the number of entries for the terminal
    /// [`EndOfStream`](TokenKind::EndOfStream) token past the last one.
    entry: usize,
    /// Index into the reader's source table — which source `token`'s offsets are in.
    source: usize,
    /// The position this token was peeked at, which its
    /// [`StartBeforePreSpace`](TokenEdge::StartBeforePreSpace) edge reports (contract
    /// clause 2). It differs from `(entry, StartBeforePreSpace)` where the peek happened
    /// inside an entry (see the [fidelity gaps](self#fidelity-gaps)).
    peeked_at: ScriptedPosition,
    /// The token the segment's scan produced, with its pre-space clipped to the peek
    /// position where the peek happened past it.
    token: StdToken<L>,
}

// Manual impls: a derive would demand `L: Clone`/`Debug`/`PartialEq`, although no `L`
// value is stored (as for `StdToken`).

impl<L: Lang> Clone for ScriptedToken<L> {
    fn clone(&self) -> Self {
        ScriptedToken {
            entry: self.entry,
            source: self.source,
            peeked_at: self.peeked_at,
            token: self.token.clone(),
        }
    }
}

impl<L: Lang> PartialEq for ScriptedToken<L> {
    fn eq(&self, other: &Self) -> bool {
        self.entry == other.entry
            && self.source == other.source
            && self.peeked_at == other.peeked_at
            && self.token == other.token
    }
}

impl<L: Lang> Eq for ScriptedToken<L> {}

impl<L: Lang> fmt::Debug for ScriptedToken<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptedToken")
            .field("entry", &self.entry)
            .field("source", &self.source)
            .field("peeked_at", &self.peeked_at)
            .field("token", &self.token)
            .finish()
    }
}

/// One entry of the script: a token, and which source it was scanned from.
struct Entry<L: Lang> {
    source: usize,
    token: StdToken<L>,
}

/// A [`TokenReader`] serving the tokens of a script (see the [module docs](self)).
pub(crate) struct ScriptedReader<'s, L: Lang> {
    /// One [`StdTokenReader`] per source named by the script, kept for interpreting the
    /// tokens scanned from it (`token_kind`) and for the source their offsets belong to.
    /// Their own positions are never used.
    sources: Vec<StdTokenReader<'s, L::SourceOrigin>>,
    /// The stream, token by token; the last entry is always an
    /// [`EndOfStream`](TokenKind::EndOfStream) token.
    entries: Vec<Entry<L>>,
    /// Where the stream stands.
    at: ScriptedPosition,
    /// Whether to report the position past an entry that a seam follows in
    /// non-canonical form (see [`ScriptedReader::broken_at_seams`]).
    broken_seams: bool,
}

impl<'s, L: Lang> ScriptedReader<'s, L> {
    /// Build the reader for `segments`, tokenizing each of them with [`StdTokenReader`]
    /// under `state`.
    ///
    /// A segment is a source and a byte range of it; the ranges may repeat a source and
    /// may leave holes (see the [module docs](self#the-script)).
    ///
    /// # Panics
    ///
    /// If the script is not one a reader could serve: no segments at all; a range out of
    /// its source's bounds or off a character boundary; content that does not tokenize;
    /// a segment ending inside a token; or a middle segment ending in whitespace, whose
    /// bytes the concatenation would drop with the segment's end-of-stream token.
    pub(crate) fn new(
        segments: &[ScriptSegment<'s, L>],
        state: &ParsingState<L>,
    ) -> ScriptedReader<'s, L> {
        assert!(!segments.is_empty(), "a script needs at least one segment");
        let mut sources: Vec<StdTokenReader<'s, L::SourceOrigin>> = Vec::new();
        let mut entries: Vec<Entry<L>> = Vec::new();

        for (nth, (source, range)) in segments.iter().enumerate() {
            let source: &'s Arc<Source<L::SourceOrigin>> = source;
            let is_last = nth + 1 == segments.len();
            let content = source.content();
            assert!(
                range.start <= range.end
                    && range.end <= content.len()
                    && content.is_char_boundary(range.start)
                    && content.is_char_boundary(range.end),
                "segment {nth} names {}..{} of a source of {} bytes, which is not a \
                 valid range of it",
                range.start,
                range.end,
                content.len()
            );
            let known = sources.iter().position(|r| Arc::ptr_eq(r.source(), source));
            let source_index = match known {
                Some(index) => index,
                None => {
                    sources.push(StdTokenReader::new(source));
                    sources.len() - 1
                }
            };

            let mut pos = range.start;
            loop {
                if pos >= range.end {
                    // The segment ended exactly where a token ended. The stream's own
                    // end-of-stream token goes past the last segment only.
                    if is_last {
                        entries.push(Entry {
                            source: source_index,
                            token: StdToken::end_of_stream(Span::empty(pos)),
                        });
                    }
                    break;
                }
                // No recovery is offered: a script that does not tokenize cleanly is a
                // mistake in the test, reported below rather than parsed around.
                let token = match sources[source_index].scan_token_at(pos, state, |_, _| None) {
                    Ok(token) => token,
                    Err(error) => panic!("segment {nth} does not tokenize: {error}"),
                };
                if matches!(
                    sources[source_index].token_kind_of(&token),
                    TokenKind::EndOfStream
                ) {
                    // The segment reached the end of its source's content. Its
                    // pre-space is the source's final whitespace; only the last
                    // segment keeps it, since only the last segment's end is the
                    // stream's end.
                    assert!(
                        is_last || token.pre_space().is_empty(),
                        "segment {nth} ends in whitespace, which the concatenation \
                         would drop with the segment's end-of-stream token"
                    );
                    if is_last {
                        entries.push(Entry { source: source_index, token });
                    }
                    break;
                }
                assert!(
                    token.edge_offset(TokenEdge::EndPastPostSpace) <= range.end,
                    "segment {nth} ends at {} inside the token at {}..{}: a segment \
                     ends where a token ends, and does not end in whitespace",
                    range.end,
                    token.edge_offset(TokenEdge::StartBeforePreSpace),
                    token.edge_offset(TokenEdge::EndPastPostSpace)
                );
                pos = token.edge_offset(TokenEdge::EndPastPostSpace);
                entries.push(Entry { source: source_index, token });
            }
        }

        let at = ScriptedPosition { entry: 0, edge: TokenEdge::StartBeforePreSpace };
        let mut reader = ScriptedReader { sources, entries, at, broken_seams: false };
        reader.at = reader.canonical(0, TokenEdge::StartBeforePreSpace);
        reader
    }

    /// The same reader as [`new`](ScriptedReader::new), reporting the position past the
    /// last entry of a source **not** in canonical form: `(i, EndPastPostSpace)` where
    /// a seam follows entry `i`, instead of the canonical `(i + 1,
    /// StartBeforePreSpace)`.
    ///
    /// **Broken by design.** Such a reader breaks the corollary of contract clauses 2
    /// and 7 — two consecutive tokens no longer meet at one position — and exists so
    /// that tests can check that the machinery reports the violation as an
    /// implementation error. Everything else about it, the stream it serves included,
    /// is what [`new`](ScriptedReader::new) gives.
    pub(crate) fn broken_at_seams(
        segments: &[ScriptSegment<'s, L>],
        state: &ParsingState<L>,
    ) -> ScriptedReader<'s, L> {
        let mut reader = ScriptedReader::new(segments, state);
        reader.broken_seams = true;
        reader
    }

    /// A reader over an empty stream on `source`: no content, one end-of-stream token at
    /// its start. This is what the language-side
    /// [`Tokenization::make_token_reader`] can build, having no script to read.
    pub(crate) fn empty_script(
        source: &'s Arc<Source<L::SourceOrigin>>,
    ) -> ScriptedReader<'s, L> {
        ScriptedReader {
            sources: vec![StdTokenReader::new(source)],
            entries: vec![Entry { source: 0, token: StdToken::end_of_stream(Span::empty(0)) }],
            at: ScriptedPosition { entry: 1, edge: TokenEdge::StartBeforePreSpace },
            broken_seams: false,
        }
    }

    /// The canonical form of the place named by `edge` of entry `entry` (see the
    /// [module docs](self#positions)).
    fn canonical(&self, entry: usize, edge: TokenEdge) -> ScriptedPosition {
        let mut entry = entry;
        let mut edge = edge;
        loop {
            let Some(current) = self.entries.get(entry) else {
                return ScriptedPosition {
                    entry: self.entries.len(),
                    edge: TokenEdge::StartBeforePreSpace,
                };
            };
            let offset = current.token.edge_offset(edge);
            if offset == current.token.edge_offset(TokenEdge::EndPastPostSpace) {
                // The place past this entry is the place before the next one.
                entry += 1;
                edge = TokenEdge::StartBeforePreSpace;
                continue;
            }
            let earliest = EVERY_EDGE
                .into_iter()
                .find(|&candidate| current.token.edge_offset(candidate) == offset)
                .expect("the queried edge itself has this offset");
            return ScriptedPosition { entry, edge: earliest };
        }
    }

    /// Which source, and which offset in it, a canonical position names. Past the last
    /// entry, that is the end of the last entry.
    fn source_offset_at(&self, at: &ScriptedPosition) -> (usize, usize) {
        match self.entries.get(at.entry) {
            Some(entry) => (entry.source, entry.token.edge_offset(at.edge)),
            None => {
                let last = self.entries.last().expect("a script always has entries");
                (last.source, last.token.edge_offset(TokenEdge::EndPastPostSpace))
            }
        }
    }

    /// Whether the entry after `entry` comes from another source — the reader's own
    /// notion of a seam.
    fn seam_follows(&self, entry: usize) -> bool {
        match (self.entries.get(entry), self.entries.get(entry + 1)) {
            (Some(current), Some(next)) => current.source != next.source,
            _ => false,
        }
    }

    /// The token to serve at the canonical position `at`.
    fn serve(&self, at: &ScriptedPosition) -> ScriptedToken<L> {
        // At `StartBeforePreSpace` and `Start` the stream stands before the entry (past
        // its pre-space in the second case); inside the token proper, the script cannot
        // be read again and the following entry is what comes next (see the
        // [fidelity gaps](self#fidelity-gaps)).
        let target = match at.edge {
            TokenEdge::StartBeforePreSpace | TokenEdge::Start => at.entry,
            _ => at.entry + 1,
        };
        let Some(entry) = self.entries.get(target) else {
            let (source, offset) = self.source_offset_at(at);
            return ScriptedToken {
                entry: self.entries.len(),
                source,
                peeked_at: *at,
                token: StdToken::end_of_stream(Span::empty(offset)),
            };
        };
        let token = match at.edge == TokenEdge::Start && target == at.entry {
            // The pre-space has been passed: report what is left of it, as a fresh scan
            // from here would.
            true => entry
                .token
                .with_pre_space(Span::empty(entry.token.edge_offset(TokenEdge::Start))),
            false => entry.token.clone(),
        };
        ScriptedToken { entry: target, source: entry.source, peeked_at: *at, token }
    }

    /// Panic unless `tok` names an entry of this script — as much of contract clause 4
    /// (a token goes back to the reader that issued it) as this reader can check: a
    /// token of *another* script whose indices happen to be in range is not detected.
    fn check_issued(&self, tok: &ScriptedToken<L>, what: &str) {
        assert!(
            tok.entry <= self.entries.len() && tok.source < self.sources.len(),
            "ScriptedReader::{what} was handed a token it never issued: {tok:?}"
        );
    }

    /// Panic unless `at` names a place in this script — the same partial check of
    /// contract clause 4 as [`check_issued`](ScriptedReader::check_issued).
    fn check_position(&self, at: &ScriptedPosition, what: &str) {
        assert!(
            at.entry <= self.entries.len(),
            "ScriptedReader::{what} was handed a position it never issued: {at:?}"
        );
    }
}

impl<'s, L> TokenReader<'s, L> for ScriptedReader<'s, L>
where
    L: Lang<Tokenization = ScriptedTokenization>,
{
    /// The entry the stream stands at. The script is fixed: `state` is not consulted
    /// (a [fidelity gap](self#fidelity-gaps)).
    fn peek(&mut self, _state: &Arc<ParsingState<L>>) -> TokenResult<L, Token<L>> {
        let at = self.canonical(self.at.entry, self.at.edge);
        Ok(self.serve(&at))
    }

    fn move_to(&mut self, tok: &Token<L>, edge: TokenEdge) {
        self.at = self.position_at(tok, edge);
    }

    fn move_to_position(&mut self, at: &StreamPosition<L>) {
        self.check_position(at, "move_to_position");
        // Stored as given: a position this reader handed out is already canonical,
        // except from `broken_at_seams`, whose whole point is to hand out one that is
        // not. Reading canonicalizes.
        self.at = *at;
    }

    fn token_kind<'t>(&self, tok: &'t Token<L>) -> TokenKind<'t, L>
    where
        's: 't,
    {
        self.check_issued(tok, "token_kind");
        // The reader of the source the token was scanned from interprets it: the view
        // borrows the token and that source's content, never a reader.
        self.sources[tok.source].token_kind_of(&tok.token)
    }

    fn source_span_between(
        &self,
        tok: &Token<L>,
        a: TokenEdge,
        b: TokenEdge,
    ) -> SourceSpan<L::SourceOrigin> {
        self.check_issued(tok, "source_span_between");
        let (a, b) = (tok.token.edge_offset(a), tok.token.edge_offset(b));
        // Reading order, whichever order the edges were named in (contract clause 6).
        SourceSpan::new(self.sources[tok.source].source(), a.min(b)..a.max(b))
    }

    fn position_here(&self) -> StreamPosition<L> {
        self.at
    }

    fn position_at(&self, tok: &Token<L>, edge: TokenEdge) -> StreamPosition<L> {
        self.check_issued(tok, "position_at");
        if edge == TokenEdge::StartBeforePreSpace {
            // Where the peek happened (contract clause 2), which is where the entry
            // begins unless the peek happened inside the preceding one.
            return tok.peeked_at;
        }
        if self.broken_seams
            && edge == TokenEdge::EndPastPostSpace
            && self.seam_follows(tok.entry)
        {
            // Broken by design: the place past this entry, reported as a value distinct
            // from the place before the next one.
            return ScriptedPosition { entry: tok.entry, edge };
        }
        self.canonical(tok.entry, edge)
    }

    /// Where a position lies in text: the offset of the entry it stands before, in that
    /// entry's own source. At a seam that is the start of the incoming token in the
    /// source it comes from — the reader's segments form a flat chain, so there is no
    /// outer source to report instead.
    fn source_position_at(&self, at: &StreamPosition<L>) -> SourcePos<L::SourceOrigin> {
        self.check_position(at, "source_position_at");
        let at = self.canonical(at.entry, at.edge);
        let (source, offset) = self.source_offset_at(&at);
        SourcePos::new(self.sources[source].source(), offset)
    }

    /// The range from `begin` to `end` when the stretch between them is one range of one
    /// source: the two in reading order, every entry between them from that source, and
    /// every entry boundary between them without a gap. `None` otherwise — across a
    /// seam, across a hole, or for an inverted pair.
    fn source_span_within(
        &self,
        begin: &StreamPosition<L>,
        end: &StreamPosition<L>,
    ) -> Option<SourceSpan<L::SourceOrigin>> {
        self.check_position(begin, "source_span_within");
        self.check_position(end, "source_span_within");
        let begin = self.canonical(begin.entry, begin.edge);
        let end = self.canonical(end.entry, end.edge);
        if !begin.precedes_or_equals(&end) {
            return None;
        }
        let (begin_source, begin_offset) = self.source_offset_at(&begin);
        let (end_source, end_offset) = self.source_offset_at(&end);
        if begin_source != end_source || begin_offset > end_offset {
            return None;
        }
        // Every entry boundary the stretch crosses joins two entries of one source at
        // one offset.
        for (index, entry) in
            self.entries.iter().enumerate().take(end.entry).skip(begin.entry)
        {
            let Some(next) = self.entries.get(index + 1) else {
                break;
            };
            if next.source != entry.source
                || next.token.edge_offset(TokenEdge::StartBeforePreSpace)
                    != entry.token.edge_offset(TokenEdge::EndPastPostSpace)
            {
                return None;
            }
        }
        Some(SourceSpan::new(self.sources[begin_source].source(), begin_offset..end_offset))
    }

    /// `begin`'s source, from `begin` to the last offset in that source among the
    /// entries up to `end` — the shape the [`TokenReader`] contract recommends. The
    /// empty span at `begin` when `end` precedes it.
    fn source_span_describing(
        &self,
        begin: &StreamPosition<L>,
        end: &StreamPosition<L>,
    ) -> SourceSpan<L::SourceOrigin> {
        self.check_position(begin, "source_span_describing");
        self.check_position(end, "source_span_describing");
        let begin = self.canonical(begin.entry, begin.edge);
        let end = self.canonical(end.entry, end.edge);
        let (source, begin_offset) = self.source_offset_at(&begin);
        let described = self.sources[source].source();
        if !begin.precedes_or_equals(&end) {
            return SourceSpan::new(described, begin_offset..begin_offset);
        }
        let (end_source, end_offset) = self.source_offset_at(&end);
        let mut last = begin_offset;
        let through = end.entry.min(self.entries.len() - 1);
        for (index, entry) in
            self.entries.iter().enumerate().take(through + 1).skip(begin.entry)
        {
            if entry.source != source {
                continue;
            }
            let offset = match index == end.entry && end_source == source {
                true => end_offset,
                false => entry.token.edge_offset(TokenEdge::EndPastPostSpace),
            };
            last = last.max(offset);
        }
        SourceSpan::new(described, begin_offset..last)
    }
}

impl<L: Lang> fmt::Debug for ScriptedReader<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptedReader")
            .field("sources", &self.sources.len())
            .field("entries", &self.entries.len())
            .field("at", &self.at)
            .field("broken_seams", &self.broken_seams)
            .finish()
    }
}

/// The crate's test language over the scripted reader: its tokenization is
/// [`ScriptedTokenization`], and it declares that its parse trees are **not** span-tiled
/// ([`Lang::OBEYS_SPAN_TILING`] `= false`), which is what a language whose reader serves
/// several sources at one nesting level must declare.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RelaxedLang;

impl Lang for RelaxedLang {
    const OBEYS_SPAN_TILING: bool = false;

    type Features = AllLangFeatures;
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type ModeId = ();
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type Tokenization = ScriptedTokenization;
    type NodeExts = ();
    type InvocationSyntax = ();
    type Driver = StdParseDriver;

    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: StagedChildren<'_, Self>,
    ) -> Result<(), NodeBuildError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Through the crate path the rest of the crate reaches these by, not `super::*`.
    use crate::token::{
        RelaxedLang, ScriptedPosition, ScriptedReader, ScriptedToken, ScriptedTokenization,
    };

    use super::EVERY_EDGE;
    use crate::scopes::ScopeStack;
    use crate::source::{Source, SourcePos};
    use crate::state::{ParsingState, StateData};
    use crate::token::{
        CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
        GroupRules, ParagraphRules, SpecialsRules, TokenEdge, TokenKind, TokenReader,
        TokenRules, Tokenization, WhitespaceRules,
    };
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    fn latex_rules() -> TokenRules<RelaxedLang> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n\r".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: vec![Arc::new(GroupRule {
                    group_type: 0,
                    open: "{".into(),
                    close: "}".into(),
                })],
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: vec![Arc::new(CommandRule {
                    escape_char: '\\',
                    name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
                })],
            },
            comments: CommentRules {
                enabled: true,
                rules: vec![Arc::new(CommentRule { start: "%".into() })],
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    fn state() -> Arc<ParsingState<RelaxedLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: latex_rules(),
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    fn source(content: &str) -> Arc<Source> {
        Arc::new(Source::new(content))
    }

    /// The reader's view of a token — the only way to read one.
    fn kind_of<'s: 't, 't>(
        reader: &ScriptedReader<'s, RelaxedLang>,
        tok: &'t ScriptedToken<RelaxedLang>,
    ) -> TokenKind<'t, RelaxedLang> {
        reader.token_kind(tok)
    }

    /// Read the whole stream, returning each token with the position it was peeked at
    /// and the position the stream stood at after it.
    fn walk(
        reader: &mut ScriptedReader<'_, RelaxedLang>,
        state: &Arc<ParsingState<RelaxedLang>>,
    ) -> Vec<(ScriptedToken<RelaxedLang>, ScriptedPosition, ScriptedPosition)> {
        let mut read = Vec::new();
        loop {
            let before = reader.position_here();
            let token = TokenReader::peek(reader, state).expect("the script serves a token");
            let done = matches!(kind_of(reader, &token), TokenKind::EndOfStream);
            TokenReader::move_to(reader, &token, TokenEdge::EndPastPostSpace);
            read.push((token, before, reader.position_here()));
            if done {
                break;
            }
        }
        read
    }

    #[test]
    fn a_chain_of_two_sources_reads_as_one_stream() {
        let a = source("ab");
        let b = source("cd");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..2), (&b, 0..2)], &st);
        let read = walk(&mut reader, &st);

        let kinds: Vec<_> = read.iter().map(|(tok, _, _)| kind_of(&reader, tok)).collect();
        assert_eq!(
            kinds,
            [
                TokenKind::Char('a'),
                TokenKind::Char('b'),
                TokenKind::Char('c'),
                TokenKind::Char('d'),
                TokenKind::EndOfStream
            ]
        );
        // Each token's span is answered in the source it was scanned from.
        assert_eq!(reader.source_span_of(&read[1].0).source().content(), "ab");
        assert_eq!(reader.source_span_of(&read[2].0).source().content(), "cd");
        assert_eq!(reader.source_span_of(&read[2].0).range(), 0..1);
    }

    #[test]
    fn consecutive_tokens_meet_at_one_position_across_a_seam() {
        // Contract clause 7's corollary, on a stream with two seams: the position past
        // each token is the position the next one was peeked at — a seam included.
        let a = source("ab\\vec ");
        let b = source(" {x}");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(
            &[(&a, 0..2), (&b, 0..4), (&a, 2..7)],
            &st,
        );
        let read = walk(&mut reader, &st);

        let kinds: Vec<_> = read.iter().map(|(tok, _, _)| kind_of(&reader, tok)).collect();
        assert!(
            matches!(kinds.as_slice(), [
                TokenKind::Char('a'),
                TokenKind::Char('b'),
                TokenKind::GroupOpen { .. },
                TokenKind::Char('x'),
                TokenKind::GroupClose { .. },
                TokenKind::Command { name: "vec", .. },
                TokenKind::EndOfStream,
            ]),
            "{kinds:?}"
        );

        for pair in read.windows(2) {
            let (previous, _, after_previous) = &pair[0];
            let (next, peeked_at, _) = &pair[1];
            assert_eq!(
                reader.position_at(previous, TokenEdge::EndPastPostSpace),
                reader.position_at(next, TokenEdge::StartBeforePreSpace),
                "consecutive tokens must meet at one position: {previous:?} / {next:?}"
            );
            // … which is where the stream stood, and where the next peek happened
            // (clauses 2 and 7).
            assert_eq!(after_previous, peeked_at);
        }
    }

    #[test]
    fn edges_that_fall_on_one_offset_are_one_position() {
        // The other half of the canonical form: within an entry, edges at the same
        // offset name one place. `x` has no pre-space and no post-space, so four of its
        // five edges coincide pairwise.
        let a = source("x y");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..3)], &st);
        let first = TokenReader::peek(&mut reader, &st).expect("a token");

        assert_eq!(
            reader.position_at(&first, TokenEdge::StartBeforePreSpace),
            reader.position_at(&first, TokenEdge::Start),
        );
        assert_eq!(
            reader.position_at(&first, TokenEdge::Start),
            reader.position_at(&first, TokenEdge::ContentStart),
        );
        assert_eq!(
            reader.position_at(&first, TokenEdge::End),
            reader.position_at(&first, TokenEdge::EndPastPostSpace),
        );
        // … and the token with pre-space distinguishes its two leading edges.
        TokenReader::move_to(&mut reader, &first, TokenEdge::EndPastPostSpace);
        let second = TokenReader::peek(&mut reader, &st).expect("a token");
        assert_ne!(
            reader.position_at(&second, TokenEdge::StartBeforePreSpace),
            reader.position_at(&second, TokenEdge::Start),
        );
    }

    #[test]
    fn moving_to_a_position_sets_the_position() {
        // Contract clause 7, both halves, at every edge of a token that has five
        // distinct ones.
        let a = source("x \\vec y");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..8)], &st);
        let first = TokenReader::peek(&mut reader, &st).expect("a token");
        TokenReader::move_to(&mut reader, &first, TokenEdge::EndPastPostSpace);
        let command = TokenReader::peek(&mut reader, &st).expect("a token");

        for edge in EVERY_EDGE {
            let at = reader.position_at(&command, edge);
            TokenReader::move_to(&mut reader, &command, edge);
            assert_eq!(reader.position_here(), at, "{edge:?}");
            TokenReader::move_to_position(&mut reader, &at);
            assert_eq!(reader.position_here(), at, "{edge:?}");
        }
    }

    #[test]
    fn peeking_inside_a_token_yields_the_following_entry() {
        // The fidelity gap of a fixed script: the stream cannot be read again from
        // inside a token, so a position at `End` (where the `\verb` idiom rewinds to,
        // before a command's post-space) yields what comes after that token — and the
        // token yielded reports that very position as its `StartBeforePreSpace` edge,
        // so contract clause 2 holds.
        let a = source("\\vec x");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..6)], &st);
        let command = TokenReader::peek(&mut reader, &st).expect("a token");
        let inside = reader.position_at(&command, TokenEdge::End);

        TokenReader::move_to_position(&mut reader, &inside);
        let next = TokenReader::peek(&mut reader, &st).expect("a token");
        assert_eq!(kind_of(&reader, &next), TokenKind::Char('x'));
        assert_eq!(reader.position_at(&next, TokenEdge::StartBeforePreSpace), inside);
        // The post-space between the two is not read again as content: the token's own
        // span is the one the script holds.
        assert_eq!(reader.source_span_of(&next).range(), 5..6);
    }

    #[test]
    fn peeking_twice_at_one_position_reads_one_token() {
        // Contract clause 1: `peek` is idempotent and does not move the stream.
        let a = source("ab");
        let b = source("c");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..2), (&b, 0..1)], &st);
        let before = reader.position_here();
        let first = TokenReader::peek(&mut reader, &st).expect("a token");
        assert_eq!(TokenReader::peek(&mut reader, &st).expect("a token"), first);
        assert_eq!(reader.position_here(), before);
    }

    #[test]
    fn end_of_stream_is_terminal_and_idempotent() {
        let a = source("x");
        let b = source("y  ");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..1), (&b, 0..3)], &st);
        let read = walk(&mut reader, &st);

        // The last segment's trailing whitespace is the end-of-stream token's pre-space.
        let (end, _, _) = read.last().expect("a stream ends");
        assert!(matches!(kind_of(&reader, end), TokenKind::EndOfStream));
        assert_eq!(
            reader
                .source_span_between(end, TokenEdge::StartBeforePreSpace, TokenEdge::Start)
                .content(),
            "  "
        );
        // Reading on yields end of stream again, now with nothing before it.
        let again = TokenReader::peek(&mut reader, &st).expect("a token");
        assert!(matches!(kind_of(&reader, &again), TokenKind::EndOfStream));
        assert_eq!(
            reader.source_span_between(&again, TokenEdge::StartBeforePreSpace, TokenEdge::Start)
                .content(),
            ""
        );
        assert_eq!(TokenReader::peek(&mut reader, &st).expect("a token"), again);
    }

    #[test]
    fn the_view_of_a_token_comes_from_the_source_it_was_scanned_from() {
        // `token_kind` across segments: each spelling is read in its own source.
        let a = source("\\alpha ");
        let b = source("% note\n{z}");
        let st = state();
        let mut reader =
            ScriptedReader::<RelaxedLang>::new(&[(&a, 0..7), (&b, 0..10)], &st);
        let read = walk(&mut reader, &st);
        let kinds: Vec<_> = read.iter().map(|(tok, _, _)| kind_of(&reader, tok)).collect();

        assert!(
            matches!(kinds.as_slice(), [
                TokenKind::Command { name: "alpha", escape_char: '\\' },
                TokenKind::Comment { start_delim: "%", content: " note" },
                TokenKind::GroupOpen { delim: "{", .. },
                TokenKind::Char('z'),
                TokenKind::GroupClose { delim: "}" },
                TokenKind::EndOfStream,
            ]),
            "{kinds:?}"
        );
        // The spans are answered in the source each token came from.
        assert_eq!(
            reader.source_span_of(&read[0].0).source().content(),
            "\\alpha "
        );
        assert_eq!(reader.source_span_of(&read[2].0).source().content(), "% note\n{z}");
    }

    #[test]
    fn a_span_within_stops_at_a_seam_while_a_described_span_covers_the_chain() {
        let a = source("ab");
        let b = source("cd");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..2), (&b, 0..2)], &st);
        let read = walk(&mut reader, &st);
        let start = read[0].1;
        let after_first = read[0].2;
        let at_the_seam = read[1].2;
        let end = read.last().expect("a stream ends").2;

        // Inside one segment, the exact range …
        let within = reader.source_span_within(&start, &after_first).expect("one source");
        assert_eq!(within.range(), 0..1);
        assert_eq!(within.content(), "a");
        // … at the seam, none: the place past the last token of `a` is the place before
        // the first token of `b`, and that place lies in `b` …
        assert_eq!(reader.source_span_within(&start, &at_the_seam), None);
        assert_eq!(reader.source_span_within(&start, &end), None);
        // … while the described span is the stretch of `begin`'s source the stream read.
        let described = reader.source_span_describing(&start, &end);
        assert_eq!(described.source().content(), "ab");
        assert_eq!(described.range(), 0..2);
        assert_eq!(reader.source_span_describing(&start, &at_the_seam).range(), 0..2);
        // An inverted pair describes the empty span at `begin`.
        assert_eq!(reader.source_span_describing(&end, &start).range(), 2..2);
        assert_eq!(reader.source_span_within(&end, &start), None);
    }

    #[test]
    fn a_described_span_of_a_splice_covers_what_the_stream_read_in_that_source() {
        // The splice of the plan: `a` from A, `xyz` from B, then ` b` from A again.
        let a = source("a..... b");
        let b = source("xyz");
        let st = state();
        let mut reader =
            ScriptedReader::<RelaxedLang>::new(&[(&a, 0..1), (&b, 0..3), (&a, 6..8)], &st);
        let read = walk(&mut reader, &st);
        let start = read[0].1;
        let end = read.last().expect("a stream ends").2;

        assert_eq!(reader.source_span_within(&start, &end), None);
        let described = reader.source_span_describing(&start, &end);
        assert_eq!(described.source().content(), "a..... b");
        // From the first entry to the last offset the stream reached in that source —
        // the skipped `.....` included, since the stream came back past it.
        assert_eq!(described.range(), 0..8);
        // A stretch that stays inside the spliced source is one range of it.
        let inside_b = reader
            .source_span_within(&read[1].1, &read[2].2)
            .expect("two entries of one source");
        assert_eq!(inside_b.source().content(), "xyz");
        assert_eq!(inside_b.content(), "xy");
        // A described span that begins in the spliced source ends where the stream left
        // that source.
        assert_eq!(reader.source_span_describing(&read[1].1, &end).range(), 0..3);
    }

    #[test]
    fn a_hole_in_one_source_has_no_span_within_but_a_described_span() {
        // One source throughout, with `....` skipped: no seam, but a gap.
        let a = source("ab....c");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..2), (&a, 6..7)], &st);
        let read = walk(&mut reader, &st);
        let start = read[0].1;
        let end = read.last().expect("a stream ends").2;

        // Contiguous entries of one source still answer their exact range …
        assert_eq!(
            reader.source_span_within(&start, &read[0].2).expect("one source").content(),
            "a"
        );
        // … but no range of the source holds the stream's content across the hole …
        assert_eq!(reader.source_span_within(&start, &end), None);
        // … and the described span covers the hole, as the recommended shape does.
        assert_eq!(reader.source_span_describing(&start, &end).range(), 0..7);
    }

    #[test]
    fn a_position_reports_the_coordinate_of_the_entry_it_stands_before() {
        let a = source("ab");
        let b = source("cd");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..2), (&b, 0..2)], &st);
        let read = walk(&mut reader, &st);

        // Inside the first source, the offsets of A …
        assert_eq!(reader.source_position_at(&read[0].1), SourcePos::new(&a, 0));
        // … and at the seam, the start of the incoming token in B.
        assert_eq!(reader.source_position_at(&read[1].2), SourcePos::new(&b, 0));
        assert_eq!(reader.source_position_at(&read[2].2), SourcePos::new(&b, 1));
        // Past the last entry: the end of the stream, in the last entry's source.
        let end = read.last().expect("a stream ends").2;
        assert_eq!(reader.source_position_at(&end), SourcePos::new(&b, 2));
    }

    #[test]
    fn rewinding_into_an_exhausted_segment_reads_it_again() {
        // Contract clause 3: a position stays usable after the stream has left its
        // source — what an argument probe that fails does.
        let a = source("ab");
        let b = source("cd");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..2), (&b, 0..2)], &st);
        let read = walk(&mut reader, &st);
        let (second_of_a, at_second_of_a, _) = &read[1];

        TokenReader::move_to_position(&mut reader, at_second_of_a);
        assert_eq!(reader.position_here(), *at_second_of_a);
        let again = TokenReader::peek(&mut reader, &st).expect("a token");
        assert_eq!(&again, second_of_a);
        assert_eq!(kind_of(&reader, &again), TokenKind::Char('b'));
        // Reading on from there crosses the seam again, as the first walk did.
        TokenReader::move_to(&mut reader, &again, TokenEdge::EndPastPostSpace);
        let across = TokenReader::peek(&mut reader, &st).expect("a token");
        assert_eq!(kind_of(&reader, &across), TokenKind::Char('c'));
        assert_eq!(&across, &read[2].0);
    }

    #[test]
    fn un_consuming_a_token_at_a_seam_returns_to_the_trigger() {
        // The seam rule of the contract: the first token of a new source carries the
        // position the stream stood at as its `StartBeforePreSpace` edge, so putting it
        // back reproduces it.
        let a = source("a");
        let b = source("b");
        let st = state();
        let mut reader = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..1), (&b, 0..1)], &st);
        let first = TokenReader::next(&mut reader, &st).expect("a token");
        let across = TokenReader::peek(&mut reader, &st).expect("a token");

        assert_eq!(
            reader.position_at(&across, TokenEdge::StartBeforePreSpace),
            reader.position_at(&first, TokenEdge::EndPastPostSpace),
        );
        TokenReader::move_to(&mut reader, &across, TokenEdge::StartBeforePreSpace);
        assert_eq!(TokenReader::peek(&mut reader, &st).expect("a token"), across);
    }

    #[test]
    fn the_broken_variant_reports_the_two_sides_of_a_seam_as_two_positions() {
        let a = source("a");
        let b = source("b");
        let st = state();
        let segments = [(&a, 0..1), (&b, 0..1)];

        let mut sound = ScriptedReader::<RelaxedLang>::new(&segments, &st);
        let first = TokenReader::next(&mut sound, &st).expect("a token");
        let across = TokenReader::peek(&mut sound, &st).expect("a token");
        assert_eq!(
            sound.position_at(&first, TokenEdge::EndPastPostSpace),
            sound.position_at(&across, TokenEdge::StartBeforePreSpace),
        );

        let mut broken = ScriptedReader::<RelaxedLang>::broken_at_seams(&segments, &st);
        let first = TokenReader::next(&mut broken, &st).expect("a token");
        let across = TokenReader::peek(&mut broken, &st).expect("a token");
        // Broken by design: the same place, reported as two values.
        assert_ne!(
            broken.position_at(&first, TokenEdge::EndPastPostSpace),
            broken.position_at(&across, TokenEdge::StartBeforePreSpace),
        );
        // The stream it serves is the same one, and it still reads on correctly.
        assert_eq!(kind_of(&broken, &across), TokenKind::Char('b'));
        // Where no seam follows, nothing is broken.
        let one_source = source("ab");
        let mut inner =
            ScriptedReader::<RelaxedLang>::broken_at_seams(&[(&one_source, 0..2)], &st);
        let one = TokenReader::next(&mut inner, &st).expect("a token");
        let two = TokenReader::peek(&mut inner, &st).expect("a token");
        assert_eq!(
            inner.position_at(&one, TokenEdge::EndPastPostSpace),
            inner.position_at(&two, TokenEdge::StartBeforePreSpace),
        );
    }

    #[test]
    fn the_language_side_reader_serves_an_empty_stream() {
        // `make_token_reader` never sees the script, so it reads nothing.
        let a = source("ignored");
        let st = state();
        let mut reader =
            <ScriptedTokenization as Tokenization<RelaxedLang>>::make_token_reader(&a);
        let token = reader.peek(&st).expect("a token");
        assert!(matches!(reader.token_kind(&token), TokenKind::EndOfStream));
        assert_eq!(reader.source_position_at(&reader.position_here()), SourcePos::new(&a, 0));
    }

    #[test]
    #[should_panic(expected = "ends in whitespace")]
    fn a_middle_segment_ending_in_whitespace_is_rejected() {
        let a = source("a ");
        let b = source("b");
        let st = state();
        let _ = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..2), (&b, 0..1)], &st);
    }

    #[test]
    #[should_panic(expected = "inside the token")]
    fn a_segment_ending_inside_a_token_is_rejected() {
        let a = source("\\alpha");
        let b = source("b");
        let st = state();
        let _ = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..3), (&b, 0..1)], &st);
    }

    #[test]
    #[should_panic(expected = "a position it never issued")]
    fn a_position_from_another_script_is_rejected() {
        let a = source("ab");
        let st = state();
        let mut long = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..2)], &st);
        let read = walk(&mut long, &st);
        let past_the_end = read.last().expect("a stream ends").2;

        let short = ScriptedReader::<RelaxedLang>::new(&[(&a, 0..1)], &st);
        let _ = short.source_position_at(&past_the_end);
    }
}
