//! The [`TokenReader`] trait and the standard rules-driven implementation,
//! [`StdTokenReader`].
//!
//! `StdTokenReader` follows pylatexenc's proven `LatexTokenReader` protocol: `peek` parses
//! the token at the current position without advancing; `move_to` repositions at a named
//! edge of a token; `move_to_position` repositions at a position the reader handed out
//! earlier; `next` = peek + move past the token. The scanning core is decomposed into
//! private `detect_*`/`read_*` methods, each driven by one feature block of the
//! [`TokenRules`] — except specials recognition, which is delegated to
//! [`Lang::scan_specials`] (gated by the state's cached
//! [`TriggerChars`](super::TriggerChars) filter).
//!
//! The whitespace primitive [`skip_whitespace`] implements the multi-newline rule in one
//! place for pre-space, command post-space, and comment post-space alike: when
//! paragraph-break detection ([`TokenRules::paragraphs_enabled`]) is on, skipped
//! whitespace never consumes a
//! newline belonging to a `\n\s*\n` sequence — such a sequence always surfaces as a
//! [`ParagraphBreak`](TokenKind::ParagraphBreak) token.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

use crate::constructs::ImplementationError;
use crate::source::{Source, SourceOrigin, SourcePos, SourceSpan, Span};
use crate::state::{FeaturePresence, Lang, LangFeatures, ParsingState};

use super::error::{
    EndOfStreamAfterEscape, ForbiddenChar, TokenError, TokenErrorKind, TokenRecovery,
    TokenResult,
};
use super::rules::{CommandRule, TokenRules};
use super::specials::SpecialsScanError;
use super::token::{StdToken, StdTokenKindData, TokenKind};

/// One of the five boundaries of a token, in reading order.
///
/// A token occupies a stretch of the stream that has two optional whitespace wings:
/// *pre-space* (content whitespace read just before the token, outside its span) and
/// *post-space* (syntactic whitespace consumed just after the token proper, inside its
/// span — only [`Command`](TokenKind::Command) and [`Comment`](TokenKind::Comment)
/// tokens have any). Inside the token proper, a kind may carry a leading marker (a
/// comment's start delimiter, a command's escape character) before its own content.
/// An edge names one of the five boundaries this creates, and is how a construct
/// parser asks a [`TokenReader`] for a position or a span without knowing how the
/// reader stores either.
///
/// The five offsets are in reading order and may coincide:
///
/// ```text
/// StartBeforePreSpace ≤ Start ≤ ContentStart ≤ End ≤ EndPastPostSpace
/// ```
///
/// `ContentStart == Start` for a kind with no leading marker;
/// `End == EndPastPostSpace` for a kind with no post-space;
/// `StartBeforePreSpace == Start` for a token with no preceding whitespace.
///
/// The ordering (`PartialOrd`/`Ord`) is the declaration order, which is reading order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenEdge {
    /// Where the token's pre-space begins — the position the stream stood at when the
    /// token was read.
    StartBeforePreSpace,
    /// Where the token proper begins (its pre-space has been passed).
    Start,
    /// Where the token's own content begins, past its leading marker: after a
    /// [`Comment`](TokenKind::Comment)'s start delimiter, after a
    /// [`Command`](TokenKind::Command)'s escape character. Every other kind has no
    /// leading marker, so this is its [`Start`](TokenEdge::Start).
    ContentStart,
    /// Where the token proper ends — equivalently, where its post-space begins.
    End,
    /// Where the token's post-space ends: the position the stream stands at after
    /// reading the token with [`next`](TokenReader::next).
    EndPastPostSpace,
}

/// The stream position type of [`StdTokenReader`] — a byte offset into the content the
/// reader scans, kept opaque.
///
/// A *stream position* names a place in a reader's token stream. Construct parsers
/// obtain one only from the reader ([`position_here`](TokenReader::position_here),
/// [`position_at`](TokenReader::position_at)) and give it back to the reader
/// ([`move_to_position`](TokenReader::move_to_position),
/// [`source_span_within`](TokenReader::source_span_within)); there is deliberately no
/// public constructor and no arithmetic, so a position cannot be invented or shifted
/// outside the reader that minted it.
///
/// Positions compare with `==` only. Two positions of the same reader are equal exactly
/// when they name the same place, which is what the parse loops need ("did the reader
/// move?").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StdStreamPosition(usize);

impl StdStreamPosition {
    /// The position at byte offset `offset`. In-crate readers only: minting a position
    /// is the issuing reader's privilege (see the type's documentation).
    pub(crate) fn at(offset: usize) -> Self {
        StdStreamPosition(offset)
    }

    /// The byte offset this position names.
    pub(crate) fn offset(self) -> usize {
        self.0
    }
}

/// The token-reading protocol — the behavior extension point for genuinely different
/// tokenization (catcode-like schemes, non-textual sources). `peek` receives the full
/// [`ParsingState<L>`], not just `&TokenRules`: a custom reader keeps its tables in
/// `L::StateExt`, which only the state exposes.
///
/// # Positions, edges, and spans
///
/// A reader is the only interpreter of its own stream. Two vocabularies serve that:
///
/// - A **stream position** ([`Lang::StreamPosition`](crate::state::Lang::StreamPosition))
///   names a place in the token stream. Only a reader mints one
///   ([`position_here`](TokenReader::position_here),
///   [`position_at`](TokenReader::position_at)), and a caller only gives it back
///   ([`move_to_position`](TokenReader::move_to_position),
///   [`source_span_within`](TokenReader::source_span_within)). Positions compare for
///   equality, never for order.
/// - A **[`TokenEdge`]** names one of a token's five boundaries — the two whitespace
///   wings, the token proper, and where the token's own content begins past a leading
///   marker (a comment delimiter, a command's escape character). Asking for a position
///   or a span at an edge is how a caller talks about where a token is without reading
///   anything off the token. The three sub-spans a comment node records are exactly
///   `Start..ContentStart`, `ContentStart..End` and `End..EndPastPostSpace`.
///
/// Locations leave a reader in exactly one form: a
/// [`SourceSpan`]/[`SourcePos`], which carries its own source. That is what lets a
/// reader serve tokens from more than one source during a parse without its caller
/// having to know.
///
/// # Contract
///
/// 1. **`peek` is speculative and idempotent per (stream position, state instance):**
///    repeated calls at the same stream position with the *same* `ParsingState`
///    instance return an equal token. The state arrives as an `&Arc` precisely so
///    implementations may memoize on that key: clone the `Arc` into the cache — pointer
///    identity is a sound key *only while a strong reference pins the allocation* (a
///    dropped state's address can be recycled for a different state). A *different*
///    state instance — even one derived with an empty delta — relieves `peek` of any
///    obligation to repeat itself. [`move_to`](TokenReader::move_to),
///    [`move_to_position`](TokenReader::move_to_position) and
///    [`next`](TokenReader::next) commit; `peek` alone never moves the stream.
/// 2. **A peeked token's [`StartBeforePreSpace`](TokenEdge::StartBeforePreSpace) edge is
///    the stream position the peek happened at:** `move_to(&tok,
///    StartBeforePreSpace)` right after a `peek` does nothing, and later returns the
///    stream to exactly where that peek happened.
/// 3. **Tokens and positions stay usable for the whole parse:** every token and every
///    position a reader hands out remains a valid argument to `move_to`,
///    `move_to_position`, `position_at`, `source_span_between` and `source_span_within`
///    until the parse ends — a reader serving several sources must keep them all
///    addressable.
/// 4. **Interpretation stays with the issuing reader** (or a reader over the same
///    content): handing a token or a position to a reader that did not produce it is a
///    caller-contract violation. [`StdTokenReader`] cannot detect it and answers from
///    the offsets the token carries; the test-only `TokenListReader` does detect it and
///    rejects tokens and positions it did not issue (its own documentation states the
///    rules), which is what makes the two-reader lockstep suites a guard against a
///    parser inventing either. What `StdTokenReader` answers for a foreign token is
///    deliberately asymmetric: [`token_kind`](TokenReader::token_kind) reads an empty
///    slice where the token's offsets fall outside its content (a wrong answer beats a
///    new panic in library code), while
///    [`source_span_between`](TokenReader::source_span_between) hands those offsets to
///    `SourceSpan::new`, whose registered always-on assert fires.
/// 5. **Absent features yield no tokens:** a token kind belonging to a feature the
///    language declares absent ([`Lang::Features`]) must never be produced — no
///    `GroupOpen`/`GroupClose` without the groups feature, no `Command`, `Comment`,
///    `Specials`, or `ParagraphBreak` without theirs. The parsing machinery treats
///    any such token as a violated contract and reports an implementation error
///    instead of processing it — uniformly across token kinds.
/// 6. **Edge order does not matter to `source_span_between`:** the result is the span
///    between the two edges in reading order, whichever order they are named in. Two
///    equal edges give the empty span at that edge.
///
/// At the end of the stream `peek` returns the terminal, idempotent
/// [`EndOfStream`](TokenKind::EndOfStream) token (never an `Option`); the reader
/// reports the input's final whitespace as that token's pre-space.
///
/// # Writing a reader over standard tokens
///
/// A reader that produces the same tokens as [`StdTokenReader`] but decides differently
/// *which* token comes next (re-classifying a character, splicing in content) does not
/// have to reimplement interpretation: keep an inner `StdTokenReader` over the same
/// content, mint tokens with the [`StdToken`] constructors (its spans are offsets into
/// that content, which the inner reader also answers with
/// [`source_span_between`](TokenReader::source_span_between)), and delegate every
/// interpretive method to the inner reader. Nothing is read off a token — there is
/// nothing readable on one.
///
/// Because the inner reader is generic over the language, delegation goes through a
/// `&dyn TokenReader<'s, L>` / `&mut dyn TokenReader<'s, L>` view of it (plain method
/// syntax on the concrete inner reader cannot infer the language):
///
/// ```
/// use std::sync::Arc;
/// use techy::core::{
///     ParsingState, StdStreamPosition, StdToken, StdTokenReader, TokenEdge, TokenKind,
///     TokenReader, TokenResult, TrivialLang,
/// };
/// use techy::source::{SourcePos, SourceSpan};
///
/// #[derive(Debug, Clone, Copy)]
/// struct MyLang;
/// impl TrivialLang for MyLang {}
///
/// struct MyReader<'s> {
///     inner: StdTokenReader<'s>,
/// }
///
/// impl<'s> MyReader<'s> {
///     fn inner(&self) -> &dyn TokenReader<'s, MyLang> {
///         &self.inner
///     }
///     fn inner_mut(&mut self) -> &mut dyn TokenReader<'s, MyLang> {
///         &mut self.inner
///     }
/// }
///
/// impl<'s> TokenReader<'s, MyLang> for MyReader<'s> {
///     /// The one method this reader decides for itself.
///     fn peek(
///         &mut self,
///         state: &Arc<ParsingState<MyLang>>,
///     ) -> TokenResult<MyLang, StdToken<MyLang>> {
///         let token = self.inner_mut().peek(state)?;
///         // … re-classify or replace `token`, minting with `StdToken::…` and spans
///         // taken from `self.inner.content()` or from the inner reader's answers.
///         Ok(token)
///     }
///
///     /// Everything interpretive is the inner reader's answer.
///     fn token_kind<'t>(&self, tok: &'t StdToken<MyLang>) -> TokenKind<'t, MyLang>
///     where
///         's: 't,
///     {
///         self.inner().token_kind(tok)
///     }
/// #    fn move_to(&mut self, tok: &StdToken<MyLang>, edge: TokenEdge) {
/// #        self.inner_mut().move_to(tok, edge)
/// #    }
/// #    fn move_to_position(&mut self, at: &StdStreamPosition) {
/// #        self.inner_mut().move_to_position(at)
/// #    }
/// #    fn source_span_between(
/// #        &self,
/// #        tok: &StdToken<MyLang>,
/// #        a: TokenEdge,
/// #        b: TokenEdge,
/// #    ) -> SourceSpan {
/// #        self.inner().source_span_between(tok, a, b)
/// #    }
/// #    fn position_here(&self) -> StdStreamPosition {
/// #        self.inner().position_here()
/// #    }
/// #    fn position_at(&self, tok: &StdToken<MyLang>, edge: TokenEdge) -> StdStreamPosition {
/// #        self.inner().position_at(tok, edge)
/// #    }
/// #    fn source_position_at(&self, at: &StdStreamPosition) -> SourcePos {
/// #        self.inner().source_position_at(at)
/// #    }
/// #    fn source_span_within(
/// #        &self,
/// #        begin: &StdStreamPosition,
/// #        end: &StdStreamPosition,
/// #    ) -> Option<SourceSpan> {
/// #        self.inner().source_span_within(begin, end)
/// #    }
///     // … the remaining position and span methods delegate the same way.
/// }
/// ```
pub trait TokenReader<'s, L: Lang> {
    /// Parse the token at the current position without advancing.
    fn peek(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<L, L::Token>;

    // --- navigation by edge and by position ------------------------------------------

    /// Reposition the stream at `edge` of `tok` — forward or backward.
    ///
    /// This is the position-based navigation the parse loops use: `move_to(&tok,
    /// TokenEdge::EndPastPostSpace)` consumes the token, `move_to(&tok,
    /// TokenEdge::Start)` puts it back to be read again,
    /// [`StartBeforePreSpace`](TokenEdge::StartBeforePreSpace) also gives back the
    /// whitespace before it, and the two inner edges
    /// ([`ContentStart`](TokenEdge::ContentStart), [`End`](TokenEdge::End)) put the
    /// stream inside the token — past a leading marker, or before the syntactic
    /// post-space (the `\verb` idiom).
    fn move_to(&mut self, tok: &L::Token, edge: TokenEdge);

    /// Reposition the stream at a position this reader handed out earlier.
    ///
    /// Deliberately bidirectional — it serves rewinds as well as resumes — so
    /// implementations assert nothing about the direction of the move. When adopting a
    /// [`TokenRecovery`](super::TokenRecovery), the *caller* enforces the
    /// [advancement contract](super::TokenRecovery#contract-resume-must-move-the-stream).
    fn move_to_position(&mut self, at: &L::StreamPosition);

    // --- what a token is --------------------------------------------------------------

    /// The parser-facing view of `tok`: what the token *is*, with the written
    /// spellings this reader resolved.
    ///
    /// This is the only way a construct parser reads a token. The returned
    /// [`TokenKind`] borrows from the token (and, for a reader that scans borrowed
    /// content, from that content) — never from the reader, so a parser may hold it
    /// while it goes on reading and moving the stream.
    ///
    /// The view carries no location: where the token is comes from
    /// [`source_span_of`](TokenReader::source_span_of),
    /// [`source_span_between`](TokenReader::source_span_between) and
    /// [`position_at`](TokenReader::position_at) — a comment's delimiter span, for
    /// instance, is `source_span_between(tok, Start, ContentStart)`.
    fn token_kind<'t>(&self, tok: &'t L::Token) -> TokenKind<'t, L>
    where
        's: 't;

    // --- where a token is -------------------------------------------------------------

    /// The source span delimited by two edges of `tok`, in either argument order (see
    /// contract clause 6).
    fn source_span_between(
        &self,
        tok: &L::Token,
        a: TokenEdge,
        b: TokenEdge,
    ) -> SourceSpan<L::SourceOrigin>;

    /// The token's own span: from [`Start`](TokenEdge::Start) to
    /// [`EndPastPostSpace`](TokenEdge::EndPastPostSpace) — pre-space excluded,
    /// post-space included.
    fn source_span_of(&self, tok: &L::Token) -> SourceSpan<L::SourceOrigin> {
        self.source_span_between(tok, TokenEdge::Start, TokenEdge::EndPastPostSpace)
    }

    // --- where the stream is ----------------------------------------------------------

    /// The stream position the reader stands at.
    fn position_here(&self) -> L::StreamPosition;

    /// The stream position at `edge` of `tok`.
    fn position_at(&self, tok: &L::Token, edge: TokenEdge) -> L::StreamPosition;

    /// Where a stream position lies in text. An empty-span diagnostic anchor at a
    /// position is [`SourceSpan::at`] of this.
    fn source_position_at(&self, at: &L::StreamPosition) -> SourcePos<L::SourceOrigin>;

    /// The source span running from `begin` to `end`, when the two positions delimit one
    /// range of one source; `None` otherwise.
    ///
    /// `None` means the pair is incoherent — `end` before `begin`, or the two in
    /// different sources. That is a caller bug (the caller lifts it to an
    /// implementation error), not a source condition.
    fn source_span_within(
        &self,
        begin: &L::StreamPosition,
        end: &L::StreamPosition,
    ) -> Option<SourceSpan<L::SourceOrigin>>;

    /// Parse the token at the current position and move past it (including its
    /// post-space): [`peek`](TokenReader::peek) +
    /// [`move_to`](TokenReader::move_to) at
    /// [`EndPastPostSpace`](TokenEdge::EndPastPostSpace).
    fn next(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<L, L::Token> {
        let token = self.peek(state)?;
        self.move_to(&token, TokenEdge::EndPastPostSpace);
        Ok(token)
    }
}

/// End position of the whitespace run starting at `pos` (= `pos` if none, if
/// whitespace handling is disabled, or if the language declares it absent —
/// [`LangFeatures::Whitespace`], whose absent store holds no whitespace data at all).
///
/// A `pos` that is out of bounds for `content` or not on a `char` boundary is a
/// caller-contract violation and panics, in all builds — one of the crate's few
/// deliberate panics (see the [Panics list](techy::guide::panics)).
///
/// **The multi-newline rule** (`TokenRules::paragraphs_enabled`): skipped
/// whitespace never contains `\n\s*\n`, nor consumes a newline from such a sequence —
/// skipping stops right *before* the first newline of a paragraph break. This one
/// primitive serves pre-space, command post-space, and comment post-space, which is what
/// makes "post-space never crosses a paragraph break" hold everywhere by construction.
pub fn skip_whitespace<L: Lang>(content: &str, pos: usize, rules: &TokenRules<L>) -> usize {
    if !<L::Features as LangFeatures>::Whitespace::PRESENT || !rules.whitespace_enabled() {
        return pos;
    }
    let Some(rest) = content.get(pos..) else {
        panic!(
            "pos {} is out of bounds or not a char boundary (content len {})",
            pos,
            content.len()
        );
    };
    let ws_chars = rules.whitespace_chars();
    let mut end = pos;
    for c in rest.chars() {
        if !ws_chars.contains(c) {
            break;
        }
        if c == '\n'
            && <L::Features as LangFeatures>::Paragraphs::PRESENT
            && rules.paragraphs_enabled()
            && paragraph_continues(content, end + 1, ws_chars)
        {
            break;
        }
        end += c.len_utf8();
    }
    end
}

/// Whether another newline follows within the whitespace run starting at `after_nl`
/// (i.e. the newline just before `after_nl` opens a `\n\s*\n` paragraph sequence).
fn paragraph_continues(content: &str, after_nl: usize, ws_chars: &str) -> bool {
    for c in content[after_nl..].chars() {
        if c == '\n' {
            return true;
        }
        if !ws_chars.contains(c) {
            return false;
        }
    }
    false
}

/// Standard tokenizer over in-memory content, driven by the parsing state: the
/// [`TokenRules`] data (plus derived caches) and the `Lang::scan_specials` hook.
///
/// The reader holds only the content borrow and a position; all tokenization behavior
/// comes from the state passed to [`peek`](TokenReader::peek) — which is what lets the
/// rules change mid-parse through state transitions.
///
/// A feature the language declares absent ([`Lang::Features`]) is never detected:
/// its detection branch is eliminated at compile time, and its rules block stores no
/// data to consult in the first place.
#[derive(Debug, Clone)]
pub struct StdTokenReader<'s, O: SourceOrigin = Option<String>> {
    source: &'s Arc<Source<O>>,
    content: &'s str,
    pos: usize,
}

impl<'s, O: SourceOrigin> StdTokenReader<'s, O> {
    /// Create a reader positioned at the start of `source`'s content.
    ///
    /// The reader borrows the source rather than cloning the `Arc`: it needs the
    /// source to answer where its tokens are (every location it hands out is a
    /// [`SourceSpan`]/[`SourcePos`] qualified by this source), and cloning the `Arc`
    /// once per span keeps that cheap.
    pub fn new(source: &'s Arc<Source<O>>) -> StdTokenReader<'s, O> {
        StdTokenReader { source, content: source.content(), pos: 0 }
    }

    /// The source being tokenized.
    pub fn source(&self) -> &'s Arc<Source<O>> {
        self.source
    }

    /// The content being tokenized.
    pub fn content(&self) -> &'s str {
        self.content
    }

    /// The largest offset at or before `offset` that is a valid position in the
    /// content: within bounds and on a character boundary. Used to anchor the report of
    /// an invalid position, which by definition is not itself an anchor.
    fn nearest_valid_offset(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.content.len());
        while !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// Whether the reader is at the end of the content.
    pub fn is_at_end(&self) -> bool {
        self.pos >= self.content.len()
    }

    // --- scanning core ------------------------------------------------------------------

    fn peek_impl<L>(&self, state: &ParsingState<L>) -> TokenResult<L, StdToken<L>>
    where
        L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
    {
        let s = self.content;
        let rules = state.rules();
        let start = self.pos;

        // The position was set through outer-layer hands (`move_to` and
        // `move_to_position` accept caller-held tokens and positions, and a token
        // recovery's resume position comes from a custom reader), so it is validated
        // at this single consumption
        // boundary ([§dd-dr:panic-policy]): an out-of-bounds or non-boundary
        // position aborts the read instead of panicking in the scanners below.
        if s.get(start..).is_none() {
            return Err(TokenError::new(
                TokenErrorKind::Custom(Box::new(ImplementationError::new(format!(
                    "token reader position {} is out of bounds or not a char \
                     boundary (content length {})",
                    start,
                    s.len()
                )))),
                // The offending position is by definition not a valid anchor, so the
                // error is reported at the nearest valid one at or before it (a
                // `SourceSpan` must be in bounds and on char boundaries).
                SourceSpan::new(self.source, Span::empty(self.nearest_valid_offset(start))),
                None,
            ));
        }

        let ws_end = skip_whitespace(s, start, rules);
        let pre_space = Span::new(start, ws_end);

        // skip_whitespace stops right before the first newline of a paragraph break, so a
        // break (which trumps everything, including end-of-stream) is detectable here.
        if let Some(token) = self.detect_paragraph_break(ws_end, pre_space, rules) {
            return Ok(token);
        }

        let pos = ws_end;
        if pos >= s.len() {
            return Ok(StdToken::end_of_stream(pre_space));
        }

        // Group delimiters come before commands so that escape-char-led delimiters like
        // `\(` win over command interpretation (as in pylatexenc, where math delimiters
        // are checked first).
        if let Some(token) = self.detect_group_delimiter(pos, pre_space, state) {
            return Ok(token);
        }

        let c = s[pos..].chars().next().expect("pos < len checked above");

        if <L::Features as LangFeatures>::Commands::PRESENT && rules.commands_enabled() {
            if let Some(rule) = rules.command_rules().iter().find(|r| c == r.escape_char) {
                return self.read_command(pos, pre_space, rules, rule);
            }
        }

        if let Some(token) = self.read_comment(pos, pre_space, rules) {
            return Ok(token);
        }

        if <L::Features as LangFeatures>::Specials::PRESENT
            && state.trigger_chars().is_some_and(|trigger_chars| trigger_chars.may_start(c))
        {
            // A scan failure is unrecoverable here: the hook reports a condition and a
            // byte range, and knows nothing about this reader's tokens or positions, so
            // it cannot describe how to carry on ([`SpecialsScanError`]). The reader
            // qualifies the range with its own source and attaches no recovery.
            let scanned = L::scan_specials(state, s, pos)
                .map_err(|error| self.lift_specials_scan_error(error, pos))?;
            if let Some(m) = scanned {
                // A malformed `end` from the hook would yield a zero-width token (the
                // dispatch loop would never advance) or a span that panics when
                // sliced. The hook is outer-layer code, so the contract is validated,
                // not debug-asserted ([§dd-dr:panic-policy]); no recovery — an
                // implementation bug aborts even under tolerant recovery.
                if !(m.end > pos && m.end <= s.len() && s.is_char_boundary(m.end)) {
                    return Err(TokenError::new(
                        TokenErrorKind::Custom(Box::new(ImplementationError::new(
                            format!(
                                "Lang::scan_specials returned an invalid match end \
                                 {} for a match at {} (content length {})",
                                m.end,
                                pos,
                                s.len()
                            ),
                        ))),
                        SourceSpan::new(self.source, Span::empty(pos)),
                        None,
                    ));
                }
                // The name is the matched text (the `SpecialsMatch` contract): the
                // token records only the span, and the reader slices it on demand.
                return Ok(StdToken::specials(
                    m.callable_type,
                    m.spec,
                    Span::new(pos, m.end),
                    pre_space,
                ));
            }
        }

        if <L::Features as LangFeatures>::ForbiddenChars::PRESENT
            && rules.forbidden_chars().contains(c)
        {
            let span = Span::new(pos, pos + c.len_utf8());
            let placeholder = StdToken::char(c, span, pre_space);
            return Err(TokenError::new(
                TokenErrorKind::ForbiddenChar(ForbiddenChar::new(c)),
                SourceSpan::new(self.source, span),
                Some(TokenRecovery {
                    token: placeholder,
                    resume: StdStreamPosition::at(span.end()),
                }),
            ));
        }

        Ok(StdToken::char(c, Span::new(pos, pos + c.len_utf8()), pre_space))
    }

    /// Turn a `Lang::scan_specials` failure into a token error qualified by this
    /// reader's source, with no recovery (the hook cannot describe one).
    ///
    /// The hook is outer-layer code, so its span is *validated*, not trusted: a span
    /// out of the content's bounds or cutting a character would make `SourceSpan::new`
    /// assert. Such a span is itself a contract violation, reported the way every other
    /// extension-contract violation in this reader is — an unrecoverable implementation
    /// error, anchored where the scan was asked for ([§dd-dr:panic-policy]).
    fn lift_specials_scan_error<L>(
        &self,
        error: SpecialsScanError,
        pos: usize,
    ) -> TokenError<L>
    where
        L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
    {
        let span = error.span;
        let valid = span.end() <= self.content.len()
            && self.content.is_char_boundary(span.start())
            && self.content.is_char_boundary(span.end());
        if !valid {
            return TokenError::new(
                TokenErrorKind::Custom(Box::new(ImplementationError::new(format!(
                    "Lang::scan_specials reported an error at an invalid span {}..{} \
                     for a scan at {} (content length {})",
                    span.start(),
                    span.end(),
                    pos,
                    self.content.len()
                )))),
                SourceSpan::new(self.source, Span::empty(self.nearest_valid_offset(pos))),
                None,
            );
        }
        TokenError::new(error.kind, SourceSpan::new(self.source, span), None)
    }

    /// A `ParagraphBreak` token if a `\n\s*\n` whitespace sequence starts at `pos` (which
    /// `skip_whitespace` guarantees whenever it stopped at a consumable-whitespace
    /// newline). The token spans from the first through the last newline of the run;
    /// whitespace after the last newline is left for the next token's pre-space.
    /// Requires the paragraphs *and* whitespace features, each declared present and
    /// enabled at runtime: a language that declares either absent never yields a break.
    fn detect_paragraph_break<L>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
    ) -> Option<StdToken<L>>
    where
        L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
    {
        if !(<L::Features as LangFeatures>::Paragraphs::PRESENT
            && <L::Features as LangFeatures>::Whitespace::PRESENT
            && rules.paragraphs_enabled()
            && rules.whitespace_enabled())
        {
            return None;
        }
        let ws_chars = rules.whitespace_chars();
        if !self.content[pos..].starts_with('\n') || !ws_chars.contains('\n') {
            return None;
        }
        let mut newlines = 0usize;
        let mut end = pos;
        let mut last_nl_end = pos;
        for c in self.content[pos..].chars() {
            if !ws_chars.contains(c) {
                break;
            }
            end += c.len_utf8();
            if c == '\n' {
                newlines += 1;
                last_nl_end = end;
            }
        }
        if newlines < 2 {
            return None; // lone newline: consumable whitespace, not a break
        }
        Some(StdToken::paragraph_break(Span::new(pos, last_nl_end), pre_space))
    }

    /// A `GroupOpen`/`GroupClose` token at `pos`, if a delimiter matches. The close
    /// delimiter expected per `rules.expecting_group_close()` takes precedence; otherwise
    /// the longest table match wins, read as an opener when the string is ambiguous.
    fn detect_group_delimiter<L>(
        &self,
        pos: usize,
        pre_space: Span,
        state: &ParsingState<L>,
    ) -> Option<StdToken<L>>
    where
        L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
    {
        let rules = state.rules();
        let rest = &self.content[pos..];

        if let Some(expected) = rules.expecting_group_close() {
            if !expected.close.is_empty() && rest.starts_with(expected.close.as_str()) {
                let span = Span::new(pos, pos + expected.close.len());
                return Some(StdToken::group_close(span, pre_space));
            }
        }

        // `None` when the language declares the groups feature absent — no table
        // exists, and no delimiter can match.
        let entry = state.prefix_table()?.match_at(rest)?;
        let span = Span::new(pos, pos + entry.delim().len());
        Some(match (entry.open(), entry.close()) {
            (Some(rule), _) => StdToken::group_open(rule.clone(), span, pre_space),
            (None, Some(_)) => StdToken::group_close(span, pre_space),
            (None, None) => unreachable!("prefix table entries always carry a direction"),
        })
    }

    /// Read a command token at `pos` (the escape character's position). The name is a
    /// greedy run of the rule's name characters, or a single character if the first one
    /// is not a name character. Multi-character names consume their following whitespace
    /// as post-space — syntactic whitespace, never crossing a paragraph break (enforced
    /// by [`skip_whitespace`] itself).
    fn read_command<L>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
        rule: &CommandRule,
    ) -> TokenResult<L, StdToken<L>>
    where
        L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
    {
        let s = self.content;
        let name_start = pos + rule.escape_char.len_utf8();

        if name_start >= s.len() {
            // Recovery: a `Char` placeholder covering the dangling escape byte itself
            // (`name_start` == `s.len()` here), so the byte stays in the tree — it
            // joins the pending chars run and the tolerant parse keeps the partition
            // invariant (decided July 2026, Action 02; supersedes the empty
            // `EndOfStream` placeholder, which dropped the byte from the AST).
            let span = Span::new(pos, name_start);
            let placeholder = StdToken::char(rule.escape_char, span, pre_space);
            return Err(TokenError::new(
                TokenErrorKind::EndOfStreamAfterEscape(EndOfStreamAfterEscape::new(
                    rule.escape_char,
                )),
                SourceSpan::new(self.source, span),
                Some(TokenRecovery {
                    token: placeholder,
                    resume: StdStreamPosition::at(span.end()),
                }),
            ));
        }

        let first = s[name_start..].chars().next().expect("name_start < len checked above");
        let mut name_end = name_start + first.len_utf8();
        let is_named = rule.name_chars.contains(first);
        if is_named {
            for c in s[name_end..].chars() {
                if !rule.name_chars.contains(c) {
                    break;
                }
                name_end += c.len_utf8();
            }
        }

        // Only multi-character (name-chars) commands swallow their post-space; `\&` and
        // friends do not (pylatexenc behavior).
        let post_space = if is_named {
            Span::new(name_end, skip_whitespace(s, name_end, rules))
        } else {
            Span::empty(name_end)
        };

        // The name is what lies between the escape character and the post-space; the
        // token records the spans and the reader slices its content on demand.
        Ok(StdToken::command(
            rule.escape_char,
            Span::new(pos, post_space.end()),
            pre_space,
            post_space,
        ))
    }

    /// A whole-comment token at `pos`, if a comment-start delimiter matches
    /// (longest-first across the rules). The content runs to the end of the line; the
    /// terminating newline plus following indentation is the token's post-space — unless
    /// that whitespace forms a paragraph break, in which case the comment takes no
    /// post-space and the break surfaces as its own token. Returns `None` when the
    /// language declares the comments feature absent (such a language stores no
    /// comment rules data at all).
    fn read_comment<L>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
    ) -> Option<StdToken<L>>
    where
        L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
    {
        if !<L::Features as LangFeatures>::Comments::PRESENT || !rules.comments_enabled() {
            return None;
        }
        let s = self.content;
        let rest = &s[pos..];
        let start = rules
            .comment_rules()
            .iter()
            .map(|r| r.start.as_str())
            .filter(|d| !d.is_empty() && rest.starts_with(d))
            .max_by_key(|d| d.len())?;

        let content_start = pos + start.len();
        // '\n' is the sole line terminator — '\r' gets no special treatment anywhere in
        // the tokenizer (feeding text-mode-normalized content is the embedder's job;
        // Action-02 follow-up, July 2026).
        let content_end = match s[content_start..].find('\n') {
            Some(i) => content_start + i,
            None => s.len(),
        };
        let post_space = Span::new(content_end, skip_whitespace(s, content_end, rules));

        Some(StdToken::comment(
            Span::new(pos, content_start),
            Span::new(pos, post_space.end()),
            pre_space,
            post_space,
        ))
    }
}

impl<'s, O, L> TokenReader<'s, L> for StdTokenReader<'s, O>
where
    O: SourceOrigin,
    L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
{
    fn peek(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<L, L::Token> {
        self.peek_impl(state)
    }

    fn move_to(&mut self, tok: &L::Token, edge: TokenEdge) {
        self.pos = tok.edge_offset(edge);
    }

    fn move_to_position(&mut self, at: &L::StreamPosition) {
        self.pos = at.offset();
    }

    fn token_kind<'t>(&self, tok: &'t L::Token) -> TokenKind<'t, L>
    where
        's: 't,
    {
        // Every written spelling is a run of this reader's content between two of the
        // token's edges — that is where a token's spans live (contract clause 4). A
        // token this reader never issued — a caller-contract violation it cannot
        // detect — may carry offsets this content does not have; such a run reads as
        // empty rather than panicking.
        let text = |a: TokenEdge, b: TokenEdge| -> &'t str {
            self.content.get(tok.edge_offset(a)..tok.edge_offset(b)).unwrap_or("")
        };
        match tok.kind_data() {
            StdTokenKindData::Char(c) => TokenKind::Char(*c),
            StdTokenKindData::GroupOpen { rule } => TokenKind::GroupOpen {
                delim: text(TokenEdge::Start, TokenEdge::End),
                rule,
            },
            StdTokenKindData::GroupClose => {
                TokenKind::GroupClose { delim: text(TokenEdge::Start, TokenEdge::End) }
            }
            StdTokenKindData::Command { escape_char, .. } => TokenKind::Command {
                name: text(TokenEdge::ContentStart, TokenEdge::End),
                escape_char: *escape_char,
            },
            StdTokenKindData::Specials { callable_type, spec } => TokenKind::Specials {
                callable_type: *callable_type,
                name: text(TokenEdge::Start, TokenEdge::End),
                spec,
            },
            StdTokenKindData::Comment { .. } => TokenKind::Comment {
                start_delim: text(TokenEdge::Start, TokenEdge::ContentStart),
                content: text(TokenEdge::ContentStart, TokenEdge::End),
            },
            StdTokenKindData::ParagraphBreak => TokenKind::ParagraphBreak,
            StdTokenKindData::EndOfStream => TokenKind::EndOfStream,
        }
    }

    fn source_span_between(
        &self,
        tok: &L::Token,
        a: TokenEdge,
        b: TokenEdge,
    ) -> SourceSpan<L::SourceOrigin> {
        let (a, b) = (tok.edge_offset(a), tok.edge_offset(b));
        // Reading order, whichever order the edges were named in; equal edges give the
        // empty span there.
        SourceSpan::new(self.source, a.min(b)..a.max(b))
    }

    fn position_here(&self) -> L::StreamPosition {
        StdStreamPosition::at(self.pos)
    }

    fn position_at(&self, tok: &L::Token, edge: TokenEdge) -> L::StreamPosition {
        StdStreamPosition::at(tok.edge_offset(edge))
    }

    fn source_position_at(&self, at: &L::StreamPosition) -> SourcePos<L::SourceOrigin> {
        SourcePos::new(self.source, at.offset())
    }

    fn source_span_within(
        &self,
        begin: &L::StreamPosition,
        end: &L::StreamPosition,
    ) -> Option<SourceSpan<L::SourceOrigin>> {
        // One source, so the only incoherent pair is an inverted one.
        (begin.offset() <= end.offset())
            .then(|| SourceSpan::new(self.source, begin.offset()..end.offset()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DiagnosticInfo;
    use crate::scopes::ScopeStack;
    use crate::spec::CallableSpec;
    use crate::state::StateData;
    use crate::token::{
        CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule, GroupRules,
        ParagraphRules, SpecialsMatch, SpecialsRules, SpecialsScanError, TriggerChars,
        WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    // Group classes used by the hardcoded latexlike-flavored test rules (the test langs
    // use the TrivialLang-style `u32` class space; a real preset would use a small enum,
    // with several rules sharing a class). Distinct per rule here so tests can look
    // rules up by class.
    const BRACES: u32 = 0;
    const BRACKETS: u32 = 1;
    const MATH_INLINE: u32 = 2;
    const MATH_DISPLAY: u32 = 3;
    const MATH_INLINE_PAREN: u32 = 4;
    const MATH_DISPLAY_BRACKET: u32 = 5;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestLang;
    impl Lang for TestLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    /// Hardcoded LaTeX-flavored rules; the real defaults arrive with the latexlike
    /// preset. Generic so the several test langs of this module can share it.
    // `Features = AllLangFeatures` (all test languages here declare it): the plain
    // block literals below only typecheck once the per-feature stores normalize to
    // the blocks themselves.
    fn latex_rules<L: Lang<GroupTypeId = u32, Features = crate::state::AllLangFeatures>>(
    ) -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n\r\u{000B}\u{000C}".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: vec![
                    group(BRACES, "{", "}"),
                    group(BRACKETS, "[", "]"),
                    group(MATH_INLINE, "$", "$"),
                    group(MATH_DISPLAY, "$$", "$$"),
                    group(MATH_INLINE_PAREN, r"\(", r"\)"),
                    group(MATH_DISPLAY_BRACKET, r"\[", r"\]"),
                ],
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

    fn group<L: Lang<GroupTypeId = u32>>(
        group_type: u32,
        open: &str,
        close: &str,
    ) -> Arc<GroupRule<L>> {
        Arc::new(GroupRule { group_type, open: open.into(), close: close.into() })
    }

    fn state(rules: TokenRules<TestLang>) -> Arc<ParsingState<TestLang>> {
        Arc::new(ParsingState::new(StateData { rules, scopes: ScopeStack::new(), mode: (), ext: () }))
    }

    /// The `latex_rules` rule of the given class (unique per rule in these tests).
    fn rule_of(group_type: u32) -> Arc<GroupRule<TestLang>> {
        latex_rules::<TestLang>()
            .groups
            .rules
            .into_iter()
            .find(|g| g.group_type == group_type)
            .expect("class present in latex_rules")
    }

    /// Rules with the given rule's close delimiter expected (as the group parser sets up
    /// when entering an ambiguously-delimited group).
    fn expecting_close(group_type: u32) -> Arc<ParsingState<TestLang>> {
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.groups.expecting_close = Some(rule_of(group_type));
        state(rules)
    }

    fn sp(start: usize, end: usize) -> Span {
        Span::new(start, end)
    }

    fn peek(
        tr: &mut StdTokenReader<'_>,
        st: &Arc<ParsingState<TestLang>>,
    ) -> StdToken<TestLang> {
        TokenReader::peek(tr, st).unwrap()
    }

    fn next(
        tr: &mut StdTokenReader<'_>,
        st: &Arc<ParsingState<TestLang>>,
    ) -> StdToken<TestLang> {
        TokenReader::next(tr, st).unwrap()
    }

    /// The reader's view of a token — the only way to read one, in the reader's own
    /// tests as in a construct parser. Generic over the language, since this module
    /// exercises several.
    fn kind_of<'s: 't, 't, L>(
        tr: &StdTokenReader<'s>,
        tok: &'t StdToken<L>,
    ) -> TokenKind<'t, L>
    where
        L: Lang<
            SourceOrigin = Option<String>,
            Token = StdToken<L>,
            StreamPosition = StdStreamPosition,
        >,
    {
        let reader: &dyn TokenReader<'s, L> = tr;
        reader.token_kind(tok)
    }

    fn char_token(c: char, at: usize, pre_space: Span) -> StdToken<TestLang> {
        StdToken::char(c, sp(at, at + c.len_utf8()), pre_space)
    }

    /// Put the reader at a byte offset. The scanner's own tests read from places no
    /// token walk reaches (mid-content starts, deliberately invalid offsets), and this
    /// module — the reader's own — is where positions may be minted from offsets.
    fn seek<L>(tr: &mut StdTokenReader<'_>, offset: usize)
    where
        L: Lang<
            SourceOrigin = Option<String>,
            Token = StdToken<L>,
            StreamPosition = StdStreamPosition,
        >,
    {
        let reader: &mut dyn TokenReader<'_, L> = tr;
        reader.move_to_position(&StdStreamPosition::at(offset));
    }

    /// Where the reader stands, as a byte offset.
    fn at<L>(tr: &StdTokenReader<'_>) -> usize
    where
        L: Lang<
            SourceOrigin = Option<String>,
            Token = StdToken<L>,
            StreamPosition = StdStreamPosition,
        >,
    {
        let reader: &dyn TokenReader<'_, L> = tr;
        reader.source_position_at(&reader.position_here()).pos()
    }

    // --- chars ------------------------------------------------------------------------

    #[test]
    fn single_char_tokens() {
        // pylatexenc parity: one token per content character; whitespace between tokens
        // becomes the next token's pre_space.
        let source = Arc::new(Source::new("ab c"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());

        assert_eq!(next(&mut tr, &st), char_token('a', 0, Span::empty(0)));
        assert_eq!(next(&mut tr, &st), char_token('b', 1, Span::empty(1)));
        assert_eq!(next(&mut tr, &st), char_token('c', 3, sp(2, 3)));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::EndOfStream);
    }

    #[test]
    fn char_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}Some", pre_space);
        let source = Arc::new(Source::new(&text));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(peek(&mut tr, &state(latex_rules())), char_token('S', 7, sp(0, 7)));
    }

    #[test]
    fn peek_does_not_advance() {
        let source = Arc::new(Source::new("abc"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let first = peek(&mut tr, &st);
        assert_eq!(peek(&mut tr, &st), first);
        assert_eq!(at::<TestLang>(&tr), 0);
    }

    #[test]
    fn char_multibyte() {
        let text = "héllo→";
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('h'));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('é'));
        assert_eq!(token.span().slice(text), "é");
        assert_eq!(at::<TestLang>(&tr), 1 + 'é'.len_utf8());
    }

    // --- commands -----------------------------------------------------------------------

    #[test]
    fn command_with_post_space() {
        let text = r"\somemacro and more stuff";
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);

        // span includes the post_space (pylatexenc: pos_end past post_space).
        assert_eq!(
            peek(&mut tr, &state(latex_rules())),
            StdToken::command('\\', sp(0, 11), Span::empty(0), sp(10, 11)),
        );
    }

    #[test]
    fn command_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}\\somemacro and more stuff", pre_space);
        let source = Arc::new(Source::new(&text));
        let mut tr = StdTokenReader::new(&source);

        assert_eq!(
            peek(&mut tr, &state(latex_rules())),
            StdToken::command('\\', sp(7, 18), sp(0, 7), sp(17, 18)),
        );
    }

    #[test]
    fn single_char_command_takes_no_post_space() {
        let source = Arc::new(Source::new(r"\& also"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &state(latex_rules())),
            StdToken::command('\\', sp(0, 2), Span::empty(0), Span::empty(2)),
        );
    }

    #[test]
    fn accent_command_then_char() {
        let source = Arc::new(Source::new(r"\`accent"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Command { name: "`", escape_char: '\\' });
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));
    }

    #[test]
    fn command_post_space_stops_before_paragraph_break() {
        // Whitespace after the command starts with the break sequence: no post_space at
        // all (pylatexenc: "put back whitespace that breaks into a new paragraph").
        let source = Arc::new(Source::new("\\macroname\n  \n "));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('\\', sp(0, 10), Span::empty(0), Span::empty(10)),
        );
        // ... and the paragraph break follows as its own token.
        assert_eq!(
            next(&mut tr, &st),
            StdToken::paragraph_break(sp(10, 14), Span::empty(10)),
        );
    }

    #[test]
    fn command_post_space_kept_up_to_paragraph_break() {
        let source = Arc::new(Source::new("\\macroname   \n  \n "));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('\\', sp(0, 13), Span::empty(0), sp(10, 13)),
        );
        assert_eq!(
            next(&mut tr, &st),
            StdToken::paragraph_break(sp(13, 17), Span::empty(13)),
        );
    }

    #[test]
    fn command_custom_name_chars() {
        let text = r"\zzz1234567890-haha_works! is a macro here";
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.commands.rules = vec![Arc::new(CommandRule {
            escape_char: '\\',
            name_chars: "0123456789abcdefghijklmnopqrstuvwxyz\
                         ABCDEFGHIJKLMNOPQRSTUVWXYZ_+!-"
                .into(),
        })];
        let st = state(rules);

        let name = "zzz1234567890-haha_works!";
        assert_eq!(
            peek(&mut tr, &st),
            StdToken::command(
                '\\',
                sp(0, 1 + name.len() + 1),
                Span::empty(0),
                sp(1 + name.len(), 1 + name.len() + 1),
            ),
        );
    }

    #[test]
    fn command_records_the_fired_rules_escape_char() {
        // Two coexisting command syntaxes: each token records which rule's escape
        // character fired (parse-time lookup disambiguates by it — DESIGN_RATIONALE [§dd-dr:tokens]).
        let names = "abcdefghijklmnopqrstuvwxyz";
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.commands.rules = vec![
            Arc::new(CommandRule { escape_char: '\\', name_chars: names.into() }),
            Arc::new(CommandRule { escape_char: '@', name_chars: names.into() }),
        ];
        let st = state(rules);
        let source = Arc::new(Source::new("\\foo @bar"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('\\', sp(0, 5), Span::empty(0), sp(4, 5)),
        );
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('@', sp(5, 9), Span::empty(5), Span::empty(9)),
        );
    }

    #[test]
    fn commands_disabled_escape_is_plain_content() {
        let source = Arc::new(Source::new(r"\foo"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.commands.rules = Vec::new();
        let st = state(rules);
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('\\'));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('f'));
    }

    #[test]
    fn commands_gate_off_is_the_scoped_disable() {
        // The gate variant of the test above: the command rules stay in the data (a
        // later enabled: Some(true) delta restores recognition without carrying them).
        let source = Arc::new(Source::new(r"\foo"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.commands.enabled = false;
        let st = state(rules);
        assert!(!st.rules().command_rules().is_empty());
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('\\'));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('f'));
    }

    // --- \begin is NOT special at the token level ---------------------------------------

    #[test]
    fn begin_environment_is_ordinary_tokens() {
        // \begin{equation} tokenizes as command + group open + chars (+ group close):
        // "as far as token parsing is concerned, \begin is a command just like \foobar".
        // Environment recognition is entirely a parse-time (preset) concern.
        let source = Arc::new(Source::new(r"\begin{equation}"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());

        assert_eq!(
            next(&mut tr, &st),
            // No post_space: '{' follows the name directly.
            StdToken::command('\\', sp(0, 6), Span::empty(0), Span::empty(6)),
        );
        let token = next(&mut tr, &st);
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::GroupOpen { delim: "{", rule: &rule_of(BRACES) },
        );
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('e'));
    }

    #[test]
    fn begin_prefixed_command_name_reads_fully() {
        let source = Arc::new(Source::new(r"\beginMacroWithConfusingName"));
        let mut tr = StdTokenReader::new(&source);
        let token = peek(&mut tr, &state(latex_rules()));
        match kind_of(&tr, &token) {
            TokenKind::Command { name, .. } => assert_eq!(name, "beginMacroWithConfusingName"),
            other => panic!("expected command, got {:?}", other),
        }
    }

    // --- groups -----------------------------------------------------------------------

    #[test]
    fn group_open_and_close() {
        let st = state(latex_rules());

        let source = Arc::new(Source::new("{begin group here"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &st),
            StdToken::group_open(rule_of(BRACES), sp(0, 1), Span::empty(0)),
        );

        let source = Arc::new(Source::new("} a braced group just ended here"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &st),
            StdToken::group_close(sp(0, 1), Span::empty(0)),
        );
    }

    #[test]
    fn group_delimiters_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}{{begin group here", pre_space);
        let source = Arc::new(Source::new(&text));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &state(latex_rules())),
            StdToken::group_open(rule_of(BRACES), sp(7, 8), sp(0, 7)),
        );
    }

    #[test]
    fn optional_argument_brackets_are_group_tokens() {
        let source = Arc::new(Source::new("[(i)]"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let token = next(&mut tr, &st);
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::GroupOpen { delim: "[", rule: &rule_of(BRACKETS) },
        );
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('('));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('i'));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char(')'));
        let token = next(&mut tr, &st);
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::GroupClose { delim: "]" },
        );
    }

    // --- math-style ambiguous group delimiters ------------------------------------------

    #[test]
    fn ambiguous_delimiter_reads_as_open_by_default() {
        let source = Arc::new(Source::new("$x$"));
        let mut tr = StdTokenReader::new(&source);
        let token = peek(&mut tr, &state(latex_rules()));
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::GroupOpen { delim: "$", rule: &rule_of(MATH_INLINE) },
        );
    }

    #[test]
    fn expected_close_wins_over_open_interpretation() {
        let source = Arc::new(Source::new("$ and more"));
        let mut tr = StdTokenReader::new(&source);
        let token = peek(&mut tr, &expecting_close(MATH_INLINE));
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::GroupClose { delim: "$" },
        );
    }

    #[test]
    fn close_only_delimiter_reads_as_close_even_unexpected() {
        // "report closing '\)' also with incorrect parsing state -- it's not the
        // tokenizer's job to report syntax errors" (pylatexenc test suite).
        let source = Arc::new(Source::new(r"\) rest"));
        let mut tr = StdTokenReader::new(&source);
        let token = peek(&mut tr, &state(latex_rules()));
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::GroupClose { delim: r"\)" },
        );
    }

    #[test]
    fn escape_led_delimiters_win_over_command_interpretation() {
        let st = state(latex_rules());

        let source = Arc::new(Source::new(r" \(x\)"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            next(&mut tr, &st),
            StdToken::group_open(rule_of(MATH_INLINE_PAREN), sp(1, 3), sp(0, 1)),
        );

        let source = Arc::new(Source::new("\n\\[ cx^2 \\]"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &st),
            StdToken::group_open(rule_of(MATH_DISPLAY_BRACKET), sp(1, 3), sp(0, 1)),
        );
    }

    #[test]
    fn dollar_dollar_disambiguation() {
        // Port of pylatexenc's test_get_token_mathmodes_dollardollar.
        // pos:            0         10        20        30
        //                 x$\dagger$$\dagger$$$A=B\mbox{$b=a$}$$
        let text = r"x$\dagger$$\dagger$$$A=B\mbox{$b=a$}$$";
        let plain = state(latex_rules());
        let in_inline = expecting_close(MATH_INLINE);
        let in_display = expecting_close(MATH_DISPLAY);

        // The views below borrow the rules they name, so the two live in bindings.
        let inline = rule_of(MATH_INLINE);
        let display = rule_of(MATH_DISPLAY);

        #[allow(clippy::type_complexity)]
        let cases: [(usize, &Arc<ParsingState<TestLang>>, TokenKind<'_, TestLang>, usize); 8] = [
            // (pos, state, expected kind, expected end)
            (1, &plain, TokenKind::GroupOpen { delim: "$", rule: &inline }, 2),
            // expected close beats the longest ('$$') match:
            (9, &in_inline, TokenKind::GroupClose { delim: "$" }, 10),
            (10, &plain, TokenKind::GroupOpen { delim: "$", rule: &inline }, 11),
            (18, &in_inline, TokenKind::GroupClose { delim: "$" }, 19),
            // not expecting a close: longest match wins -> display '$$':
            (19, &plain, TokenKind::GroupOpen { delim: "$$", rule: &display }, 21),
            (30, &plain, TokenKind::GroupOpen { delim: "$", rule: &inline }, 31),
            (34, &in_inline, TokenKind::GroupClose { delim: "$" }, 35),
            (36, &in_display, TokenKind::GroupClose { delim: "$$" }, 38),
        ];

        for (pos, st, kind, end) in cases {
            let source = Arc::new(Source::new(text));
            let mut tr = StdTokenReader::new(&source);
            seek::<TestLang>(&mut tr, pos);
            let token = peek(&mut tr, st);
            assert_eq!(kind_of(&tr, &token), kind, "at pos {}", pos);
            assert_eq!(token.span(), sp(pos, end), "at pos {}", pos);
        }
    }

    // --- comments (whole-comment tokens) -------------------------------------------------

    #[test]
    fn comment_token_covers_content_and_post_space() {
        // "% Comment here\n  more": content sans delimiter and newline; post_space =
        // newline + indentation (syntactic whitespace, consumed by the comment).
        let source = Arc::new(Source::new("% Comment here\n  more stuff"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st),
            StdToken::comment(sp(0, 1), sp(0, 17), Span::empty(0), sp(14, 17)),
        );
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('m'));
    }

    #[test]
    fn comment_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}% Comment here\n  more stuff", pre_space);
        let source = Arc::new(Source::new(&text));
        let mut tr = StdTokenReader::new(&source);
        let token = peek(&mut tr, &state(latex_rules()));
        assert_eq!(token.pre_space(), sp(0, 7));
        assert_eq!(token.span(), sp(7, 24));
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::Comment { start_delim: "%", content: " Comment here" },
        );
        // The post-space is a reader answer, not a token field.
        let reader: &dyn TokenReader<'_, TestLang> = &tr;
        assert_eq!(
            reader.source_span_between(&token, TokenEdge::End, TokenEdge::EndPastPostSpace),
            SourceSpan::new(&source, sp(21, 24)),
        );
    }

    #[test]
    fn comment_before_paragraph_break_takes_no_post_space() {
        // "a% c\n\nb": the comment's terminating newline belongs to a \n\s*\n sequence,
        // so the comment takes no post-space and the paragraph break survives as its own
        // token (TeX-wise: the blank line still yields \par).
        let source = Arc::new(Source::new("a% c\n\nb"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));
        assert_eq!(
            next(&mut tr, &st),
            StdToken::comment(sp(1, 2), sp(1, 4), Span::empty(1), Span::empty(4)),
        );
        assert_eq!(
            next(&mut tr, &st),
            StdToken::paragraph_break(sp(4, 6), Span::empty(4)),
        );
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('b'));
    }

    #[test]
    fn comment_at_end_of_input_without_newline() {
        let source = Arc::new(Source::new("x% trailing"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('x'));
        assert_eq!(
            next(&mut tr, &st),
            StdToken::comment(sp(1, 2), sp(1, 11), Span::empty(1), Span::empty(11)),
        );
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::EndOfStream);
    }

    #[test]
    fn comment_crlf_carriage_return_is_ordinary_content() {
        // Deliberate (July 2026, Action-02 follow-up): '\n' is the sole line
        // terminator, and the tokenizer gives '\r' no special treatment whatsoever —
        // input is expected text-mode-normalized by the embedder (the no_std core
        // never reads files). On CRLF input the '\r' therefore stays inside the
        // comment content, as in pylatexenc.
        let source = Arc::new(Source::new("% note\r\n  more"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st),
            StdToken::comment(sp(0, 1), sp(0, 10), Span::empty(0), sp(7, 10)),
        );
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('m'));
    }

    #[test]
    fn comment_alternative_start_string_longest_wins() {
        let text = "%!!COMMENT!! Comment here\nmore";
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.comments.rules = vec![
            Arc::new(CommentRule { start: "%".into() }),
            Arc::new(CommentRule { start: "%!!COMMENT!!".into() }),
        ];
        let st = state(rules);
        assert_eq!(
            peek(&mut tr, &st),
            StdToken::comment(sp(0, 12), sp(0, 26), Span::empty(0), sp(25, 26)),
        );
    }

    #[test]
    fn comments_disabled_percent_is_plain_content() {
        let source = Arc::new(Source::new("a %b"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.comments.rules = Vec::new();
        let st = state(rules);
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));
        assert_eq!(next(&mut tr, &st), char_token('%', 2, sp(1, 2)));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('b'));
    }

    #[test]
    fn comments_gate_off_is_the_scoped_disable() {
        let source = Arc::new(Source::new("a %b"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.comments.enabled = false;
        let st = state(rules);
        assert!(!st.rules().comment_rules().is_empty());
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));
        assert_eq!(next(&mut tr, &st), char_token('%', 2, sp(1, 2)));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('b'));
    }

    // --- paragraph breaks ---------------------------------------------------------------

    #[test]
    fn paragraph_break_token() {
        // "Abc    \n\n  z": break spans first..last newline; leading run is pre_space,
        // whitespace after the last newline is left for the next token.
        let source = Arc::new(Source::new("Abc    \n\n  z"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        seek::<TestLang>(&mut tr, 3);

        assert_eq!(
            next(&mut tr, &st),
            StdToken::paragraph_break(sp(7, 9), sp(3, 7)),
        );
        assert_eq!(next(&mut tr, &st), char_token('z', 11, sp(9, 11)));
    }

    #[test]
    fn paragraph_break_with_inner_whitespace() {
        let source = Arc::new(Source::new("Abc  \t \n   \t\nz"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        seek::<TestLang>(&mut tr, 3);
        assert_eq!(
            next(&mut tr, &st),
            StdToken::paragraph_break(sp(7, 13), sp(3, 7)),
        );
    }

    #[test]
    fn paragraph_break_in_trailing_whitespace_still_emitted() {
        let source = Arc::new(Source::new("x  \n\n  "));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('x'));
        assert_eq!(
            next(&mut tr, &st),
            StdToken::paragraph_break(sp(3, 5), sp(1, 3)),
        );
        // The whitespace after the break's last newline is the end-of-stream token's
        // pre_space — nothing is lost.
        assert_eq!(
            next(&mut tr, &st),
            StdToken::end_of_stream(sp(5, 7)),
        );
    }

    #[test]
    fn paragraph_breaks_disabled() {
        let source = Arc::new(Source::new("Abc\n\nNew"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.paragraphs.enabled = false;
        let st = state(rules);
        seek::<TestLang>(&mut tr, 2);
        assert_eq!(next(&mut tr, &st), char_token('c', 2, Span::empty(2)));
        // The double newline is ordinary consumable whitespace now.
        assert_eq!(next(&mut tr, &st), char_token('N', 5, sp(3, 5)));
    }

    // --- specials (via the Lang scan hook) ------------------------------------------------

    #[derive(Debug)]
    struct StubSpec;
    impl crate::serialize::SerializableObject<SpecialsLang> for StubSpec {}
    impl CallableSpec<SpecialsLang> for StubSpec {}

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SpecialsLang;
    impl Lang for SpecialsLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~&-".into())
        }

        fn scan_specials(
            _state: &ParsingState<Self>,
            content: &str,
            pos: usize,
        ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
            // Longest-first over a hardcoded trigger list — a stand-in for the preset
            // dispatching to its libraries (Phase 4+).
            for trigger in ["---", "~~", "~", "&"] {
                if content[pos..].starts_with(trigger) {
                    return Ok(Some(SpecialsMatch {
                        end: pos + trigger.len(),
                        callable_type: 7,
                        spec: Arc::new(StubSpec),
                    }));
                }
            }
            Ok(None)
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    fn specials_state(rules: TokenRules<SpecialsLang>) -> Arc<ParsingState<SpecialsLang>> {
        Arc::new(ParsingState::new(StateData { rules, scopes: ScopeStack::new(), mode: (), ext: () }))
    }

    #[test]
    fn specials_recognized_with_spec_attached() {
        let source = Arc::new(Source::new("a&b"));
        let mut tr = StdTokenReader::new(&source);
        let st = specials_state(latex_rules());

        let token = TokenReader::next(&mut tr, &st).unwrap();
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));
        let token = TokenReader::next(&mut tr, &st).unwrap();
        match kind_of(&tr, &token) {
            TokenKind::Specials { callable_type, name, spec } => {
                assert_eq!(callable_type, 7);
                assert_eq!(name, "&");
                assert_eq!(format!("{:?}", spec), "StubSpec");
            }
            other => panic!("expected specials, got {:?}", other),
        }
        assert_eq!(token.span(), sp(1, 2));
        let token = TokenReader::next(&mut tr, &st).unwrap();
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('b'));
    }

    #[test]
    fn specials_longest_match_is_the_scanners_business() {
        let source = Arc::new(Source::new("---x"));
        let mut tr = StdTokenReader::new(&source);
        let st = specials_state(latex_rules());
        let token = TokenReader::peek(&mut tr, &st).unwrap();
        match kind_of(&tr, &token) {
            TokenKind::Specials { name, .. } => assert_eq!(name, "---"),
            other => panic!("expected specials, got {:?}", other),
        }
        assert_eq!(token.span(), sp(0, 3));
    }

    #[test]
    fn specials_scan_miss_falls_through_to_char() {
        // '-' is a trigger char, but a lone '-' matches no trigger: plain content.
        let source = Arc::new(Source::new("-x"));
        let mut tr = StdTokenReader::new(&source);
        let st = specials_state(latex_rules());
        let token = TokenReader::next(&mut tr, &st).unwrap();
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('-'));
    }

    // --- outer-layer contract violations are reported, never panicking or hanging
    // --- ([§dd-dr:panic-policy]) ----------------------------------------------------------

    /// A lang whose scan hook violates the match-end contract: a zero-width match
    /// (`end == pos`) that would hang the dispatch loop if the reader emitted it.
    #[derive(Debug, Clone, Copy)]
    struct BadEndLang;
    impl Lang for BadEndLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;
        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~".into())
        }
        fn scan_specials(
            _state: &ParsingState<Self>,
            _content: &str,
            pos: usize,
        ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
            Ok(Some(SpecialsMatch {
                end: pos, // contract violation: a match must advance
                callable_type: 7,
                spec: Arc::new(BadEndStubSpec),
            }))
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct BadEndStubSpec;
    impl crate::serialize::SerializableObject<BadEndLang> for BadEndStubSpec {}
    impl CallableSpec<BadEndLang> for BadEndStubSpec {}

    /// A lang whose scan hook always fails, with a span chosen by the trigger:
    /// `!` reports a well-formed span, `?` a span past the end of the content, and `~`
    /// a span cutting the two-byte character the test content starts with.
    #[derive(Debug, Clone, Copy)]
    struct ScanErrorLang;
    impl Lang for ScanErrorLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Token = crate::token::StdToken<Self>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;
        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("!?~".into())
        }
        fn scan_specials(
            _state: &ParsingState<Self>,
            content: &str,
            pos: usize,
        ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
            let span = match content[pos..].chars().next() {
                Some('!') => Span::new(pos, pos + 1),
                Some('?') => Span::new(0, 99), // out of the content's bounds
                _ => Span::new(1, 2),          // cuts the leading two-byte character
            };
            Err(SpecialsScanError {
                kind: TokenErrorKind::Custom(Box::new(ImplementationError::new(
                    String::from("the scan declined loudly"),
                ))),
                span,
            })
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    fn scan_error_state() -> Arc<ParsingState<ScanErrorLang>> {
        let rules: TokenRules<ScanErrorLang> = latex_rules();
        Arc::new(ParsingState::new(StateData {
            rules,
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    #[test]
    fn a_lifted_specials_scan_error_carries_no_recovery() {
        let source = Arc::new(Source::new("!x"));
        let mut tr = StdTokenReader::new(&source);
        let err = TokenReader::peek(&mut tr, &scan_error_state()).unwrap_err();

        // The reader qualifies the hook's range with its own source, and the failure is
        // unrecoverable: the hook cannot describe how to carry on.
        assert_eq!(err.span(), &SourceSpan::new(&source, sp(0, 1)));
        assert!(err.into_recovery().is_none());
    }

    #[test]
    fn a_specials_scan_error_at_an_invalid_span_is_reported_not_panicked() {
        // The hook is outer-layer code: a span out of bounds or cutting a character
        // must not reach `SourceSpan::new`'s assert ([§dd-dr:panic-policy]).
        //                                    'é' occupies bytes 0..2, so 1 cuts it
        for (content, at, trigger) in [("?x", 0, "out of bounds"), ("\u{e9}~", 2, "mid-character")]
        {
            let source = Arc::new(Source::new(content));
            let mut tr = StdTokenReader::new(&source);
            let st = scan_error_state();
            seek::<ScanErrorLang>(&mut tr, at);

            let err = TokenReader::peek(&mut tr, &st).unwrap_err();
            match err.kind() {
                TokenErrorKind::Custom(data) => {
                    assert_eq!(
                        data.identifier(),
                        crate::constructs::ImplementationError::IDENTIFIER,
                        "{trigger}"
                    );
                    assert!(
                        data.to_string().contains("Lang::scan_specials"),
                        "{trigger}: {}",
                        data
                    );
                }
                other => panic!("{trigger}: expected a Custom error, got {:?}", other),
            }
            // Anchored at a valid offset, and unrecoverable under either policy.
            assert!(err.span().is_empty());
            assert!(err.into_recovery().is_none());
        }
    }

    #[test]
    fn scan_specials_invalid_match_end_is_an_unrecoverable_implementation_error() {
        let source = Arc::new(Source::new("~x"));
        let mut tr = StdTokenReader::new(&source);
        let rules: TokenRules<BadEndLang> = latex_rules();
        let st = Arc::new(ParsingState::new(StateData {
            rules,
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }));
        let err = TokenReader::peek(&mut tr, &st).unwrap_err();
        match err.kind() {
            TokenErrorKind::Custom(data) => assert_eq!(
                data.identifier(),
                crate::constructs::ImplementationError::IDENTIFIER
            ),
            other => panic!("expected a Custom implementation error, got {:?}", other),
        }
        // No recovery: an implementation bug aborts even under tolerant recovery.
        assert!(err.into_recovery().is_none());
    }

    #[test]
    fn invalid_reader_position_is_an_unrecoverable_implementation_error_at_peek() {
        let st = state(latex_rules());

        // Out of bounds (a resume position comes from outer-layer hands, so the
        // violation is reported at the next read, not panicked).
        let source = Arc::new(Source::new("ab"));
        let mut tr = StdTokenReader::new(&source);
        seek::<TestLang>(&mut tr, 5);
        let err = TokenReader::peek(&mut tr, &st).unwrap_err();
        assert!(matches!(err.kind(), TokenErrorKind::Custom(data)
            if data.identifier() == crate::constructs::ImplementationError::IDENTIFIER));
        assert!(err.into_recovery().is_none());

        // Not a char boundary.
        let source = Arc::new(Source::new("é"));
        let mut tr = StdTokenReader::new(&source);
        seek::<TestLang>(&mut tr, 1);
        let err = TokenReader::peek(&mut tr, &st).unwrap_err();
        assert!(matches!(err.kind(), TokenErrorKind::Custom(data)
            if data.identifier() == crate::constructs::ImplementationError::IDENTIFIER));
    }

    #[test]
    fn specials_gate_off_freezes_the_empty_filter() {
        // The gate is baked at freeze: the state stores the empty TriggerChars, so the
        // scan hook is unreachable and triggers read as plain content — this is what
        // makes "no specials here" delta-expressible (DESIGN_RATIONALE [§dd-dr:tokens], ex-[§dd-dr:open-questions]).
        let source = Arc::new(Source::new("a&b"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<SpecialsLang> = latex_rules();
        rules.specials.enabled = false;
        let st = specials_state(rules);
        assert_eq!(
            st.trigger_chars().expect("all-present test language"),
            &TriggerChars::default()
        );
        let token = TokenReader::next(&mut tr, &st).unwrap();
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));
        let token = TokenReader::next(&mut tr, &st).unwrap();
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('&'));
        let token = TokenReader::next(&mut tr, &st).unwrap();
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('b'));
    }

    #[test]
    fn scan_hook_not_consulted_outside_trigger_chars() {
        #[derive(Debug, Clone, Copy)]
        struct PanickyLang;
        impl Lang for PanickyLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = ();
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type Token = crate::token::StdToken<Self>;
            type StreamPosition = crate::token::StdStreamPosition;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = crate::engine::StdParseDriver;
            fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
                TriggerChars::Only("~".into())
            }
            fn scan_specials(
                _state: &ParsingState<Self>,
                _content: &str,
                _pos: usize,
            ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
                panic!("scan_specials consulted for a non-trigger character");
            }
            fn make_node_ext(
                _kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) -> Result<(), crate::node::NodeBuildError> {
                Ok(())
            }
        }
        let st: Arc<ParsingState<PanickyLang>> = Arc::new(ParsingState::new(StateData {
            rules: latex_rules(),
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }));
        let source = Arc::new(Source::new("x"));
        let mut tr = StdTokenReader::new(&source);
        let token = TokenReader::next(&mut tr, &st).unwrap();
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('x'));
    }

    #[test]
    fn the_content_start_edge_lies_past_a_kinds_leading_marker() {
        //                                    0....5....10...15
        let source = Arc::new(Source::new("%% note\n\\vec  x"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        // A comment: `ContentStart` is right after the start delimiter, so the
        // delimiter, the text and the post-space are three plain reader answers.
        let comment = reader.next(&st).unwrap();
        assert_eq!(
            reader.source_span_between(&comment, TokenEdge::Start, TokenEdge::ContentStart),
            SourceSpan::new(&source, sp(0, 1))
        );
        assert_eq!(
            reader
                .source_span_between(&comment, TokenEdge::ContentStart, TokenEdge::End)
                .content(),
            "% note"
        );
        assert_eq!(
            reader.source_span_between(&comment, TokenEdge::End, TokenEdge::EndPastPostSpace),
            SourceSpan::new(&source, sp(7, 8))
        );

        // A command: `ContentStart` is right after the escape character, so the name is
        // `ContentStart..End`.
        let command = reader.next(&st).unwrap();
        assert_eq!(
            reader
                .source_span_between(&command, TokenEdge::Start, TokenEdge::ContentStart)
                .content(),
            "\\"
        );
        assert_eq!(
            reader
                .source_span_between(&command, TokenEdge::ContentStart, TokenEdge::End)
                .content(),
            "vec"
        );

        // A kind with no leading marker: `ContentStart` is its `Start`.
        let plain = reader.next(&st).unwrap();
        assert_eq!(
            reader.position_at(&plain, TokenEdge::ContentStart),
            reader.position_at(&plain, TokenEdge::Start)
        );
    }

    #[test]
    fn a_multi_byte_escape_character_moves_content_start_by_its_own_width() {
        // `§` is two bytes: the command's name starts two bytes past its start.
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.commands.rules = vec![Arc::new(CommandRule {
            escape_char: '§',
            name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
        })];
        let source = Arc::new(Source::new("§vec x"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(rules);
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let command = reader.next(&st).unwrap();
        assert_eq!(
            reader.source_span_between(&command, TokenEdge::Start, TokenEdge::ContentStart),
            SourceSpan::new(&source, sp(0, '§'.len_utf8()))
        );
        assert_eq!(
            reader
                .source_span_between(&command, TokenEdge::ContentStart, TokenEdge::End)
                .content(),
            "vec"
        );
    }

    #[test]
    fn the_five_edges_are_in_reading_order_for_every_kind() {
        // `StartBeforePreSpace ≤ Start ≤ ContentStart ≤ End ≤ EndPastPostSpace`, with
        // equality where a kind has no pre-space, no leading marker, or no post-space.
        let source = Arc::new(Source::new("a {%c\n} \\vec  b\n\nz"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;
        loop {
            let token = reader.next(&st).unwrap();
            // Measured from the earliest edge, so each span's end *is* the edge's
            // own offset.
            let offsets: Vec<usize> = [
                TokenEdge::StartBeforePreSpace,
                TokenEdge::Start,
                TokenEdge::ContentStart,
                TokenEdge::End,
                TokenEdge::EndPastPostSpace,
            ]
            .into_iter()
            .map(|edge| {
                reader
                    .source_span_between(&token, TokenEdge::StartBeforePreSpace, edge)
                    .range()
                    .end
            })
            .collect();
            let kind = reader.token_kind(&token);
            assert!(
                offsets.windows(2).all(|w| w[0] <= w[1]),
                "{kind:?} edges out of order: {offsets:?}"
            );
            if matches!(kind, TokenKind::EndOfStream) {
                break;
            }
        }
    }

    // --- the token view ---------------------------------------------------------------

    #[test]
    fn the_view_reports_what_each_token_kind_is() {
        //                                    0....5....10...15...20
        let source = Arc::new(Source::new("a{\\vec  }% note\nb"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let token = reader.next(&st).unwrap();
        assert_eq!(reader.token_kind(&token), TokenKind::Char('a'));

        let token = reader.next(&st).unwrap();
        match reader.token_kind(&token) {
            TokenKind::GroupOpen { delim, rule } => {
                assert_eq!(delim, "{");
                assert_eq!(rule.group_type, BRACES);
            }
            other => panic!("expected a group open, got {other:?}"),
        }

        let token = reader.next(&st).unwrap();
        assert_eq!(
            reader.token_kind(&token),
            TokenKind::Command { name: "vec", escape_char: '\\' }
        );

        let token = reader.next(&st).unwrap();
        assert_eq!(reader.token_kind(&token), TokenKind::GroupClose { delim: "}" });

        // The comment's two texts, as this reader resolves them: `%` then ` note`,
        // the newline being post-space.
        let token = reader.next(&st).unwrap();
        assert_eq!(
            reader.token_kind(&token),
            TokenKind::Comment { start_delim: "%", content: " note" }
        );
        let body = reader.source_span_between(&token, TokenEdge::Start, TokenEdge::End);
        assert_eq!(body.content(), "% note");

        let token = reader.next(&st).unwrap();
        assert_eq!(reader.token_kind(&token), TokenKind::Char('b'));

        let token = reader.next(&st).unwrap();
        assert_eq!(reader.token_kind(&token), TokenKind::EndOfStream);
    }

    #[test]
    fn the_view_of_a_paragraph_break_carries_no_data() {
        let source = Arc::new(Source::new("a\n\nb"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;
        let _ = reader.next(&st).unwrap();
        let token = reader.next(&st).unwrap();
        assert_eq!(reader.token_kind(&token), TokenKind::ParagraphBreak);
        assert_eq!(reader.token_kind(&token).as_str(), "ParagraphBreak");
    }

    #[test]
    fn the_view_of_a_specials_token_carries_its_resolution() {
        let source = Arc::new(Source::new("a&b"));
        let mut tr = StdTokenReader::new(&source);
        let st = specials_state(latex_rules());
        let reader: &mut dyn TokenReader<'_, SpecialsLang> = &mut tr;

        let _ = reader.next(&st).unwrap();
        let token = reader.next(&st).unwrap();
        match reader.token_kind(&token) {
            TokenKind::Specials { callable_type, name, spec } => {
                assert_eq!(callable_type, 7);
                assert_eq!(name, "&");
                assert_eq!(format!("{:?}", spec), "StubSpec");
            }
            other => panic!("expected specials, got {other:?}"),
        }
        // Two views of the same token are equal (the spec compares by `Arc` identity).
        assert_eq!(reader.token_kind(&token), reader.token_kind(&token));
    }

    #[test]
    fn a_view_is_copy_and_survives_further_reading() {
        let source = Arc::new(Source::new("\\vec  x"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let token = reader.next(&st).unwrap();
        let kind = reader.token_kind(&token);
        // The view borrows the token and the content, never the reader: reading on and
        // moving the stream leaves it usable, and it copies rather than moves.
        let _next = reader.next(&st).unwrap();
        reader.move_to(&token, TokenEdge::Start);
        let copy = kind;
        assert_eq!(kind, copy);
        assert_eq!(kind.to_string(), "Command(\\vec)");
    }

    // --- positions, edges, spans -----------------------------------------------------

    #[test]
    fn the_four_edges_of_a_token_delimit_its_pre_space_body_and_post_space() {
        //                                    0....5....1
        let source = Arc::new(Source::new("a \\vec  b"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let _ = reader.next(&st).unwrap(); // the leading `a`
        let token = reader.peek(&st).unwrap();
        // ` \vec  `: pre-space 1..2, the command 2..6, its post-space 6..8.
        assert_eq!(
            reader.source_span_between(
                &token,
                TokenEdge::StartBeforePreSpace,
                TokenEdge::Start
            ),
            SourceSpan::new(&source, sp(1, 2))
        );
        assert_eq!(
            reader.source_span_between(&token, TokenEdge::Start, TokenEdge::End),
            SourceSpan::new(&source, sp(2, 6))
        );
        assert_eq!(
            reader.source_span_between(&token, TokenEdge::End, TokenEdge::EndPastPostSpace),
            SourceSpan::new(&source, sp(6, 8))
        );
        // `source_span_of` is Start..EndPastPostSpace — the token's own span.
        assert_eq!(reader.source_span_of(&token), SourceSpan::new(&source, sp(2, 8)));
        assert_eq!(reader.source_span_of(&token).span(), token.span());
    }

    #[test]
    fn source_span_between_ignores_edge_order_and_empties_on_equal_edges() {
        let source = Arc::new(Source::new(" x"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let token = reader.peek(&st).unwrap();
        let forward =
            reader.source_span_between(&token, TokenEdge::StartBeforePreSpace, TokenEdge::End);
        let backward =
            reader.source_span_between(&token, TokenEdge::End, TokenEdge::StartBeforePreSpace);
        assert_eq!(forward, backward);
        assert_eq!(forward, SourceSpan::new(&source, sp(0, 2)));

        let empty = reader.source_span_between(&token, TokenEdge::Start, TokenEdge::Start);
        assert!(empty.is_empty());
        assert_eq!(empty, SourceSpan::new(&source, sp(1, 1)));
    }

    #[test]
    fn a_peeked_tokens_first_edge_is_the_position_the_peek_happened_at() {
        // Contract clause 2: `move_to(&tok, StartBeforePreSpace)` returns the
        // stream to where the peek happened.
        let source = Arc::new(Source::new("a  b"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let first = reader.next(&st).unwrap();
        let before = reader.position_here();
        let token = reader.peek(&st).unwrap();
        assert_eq!(reader.position_at(&token, TokenEdge::StartBeforePreSpace), before);

        reader.move_to(&token, TokenEdge::EndPastPostSpace);
        assert_ne!(reader.position_here(), before);
        reader.move_to(&token, TokenEdge::StartBeforePreSpace);
        assert_eq!(reader.position_here(), before);
        assert_eq!(reader.peek(&st).unwrap(), token);

        // And back to the very beginning, through the first token.
        reader.move_to(&first, TokenEdge::StartBeforePreSpace);
        assert_eq!(reader.position_here(), reader.position_at(&first, TokenEdge::Start));
    }

    #[test]
    fn positions_answer_text_locations_and_delimit_spans() {
        let source = Arc::new(Source::new("ab cd"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let begin = reader.position_here();
        assert_eq!(reader.source_position_at(&begin), SourcePos::new(&source, 0));

        let _ = reader.next(&st).unwrap();
        let _ = reader.next(&st).unwrap();
        let end = reader.position_here();

        assert_eq!(
            reader.source_span_within(&begin, &end),
            Some(SourceSpan::new(&source, sp(0, 2)))
        );
        // An inverted pair does not delimit a range: the caller lifts `None` to an
        // implementation error.
        assert_eq!(reader.source_span_within(&end, &begin), None);
        // Equal positions delimit the empty span there.
        assert_eq!(
            reader.source_span_within(&end, &end),
            Some(SourceSpan::new(&source, sp(2, 2)))
        );
    }

    #[test]
    fn a_position_returns_the_stream_to_where_it_was_taken() {
        let source = Arc::new(Source::new("a{b}c"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let _ = reader.next(&st).unwrap();
        let mark = reader.position_here();
        let open = reader.next(&st).unwrap();
        assert!(matches!(reader.token_kind(&open), TokenKind::GroupOpen { .. }));

        reader.move_to_position(&mark);
        assert_eq!(reader.position_here(), mark);
        assert_eq!(reader.peek(&st).unwrap(), open);
    }

    // --- errors and recovery -------------------------------------------------------------

    #[test]
    fn forbidden_char_error_with_recovery() {
        let source = Arc::new(Source::new("% forbidden here"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.comments.rules = Vec::new();
        rules.forbidden_chars.chars = "%$".into();
        let st = state(rules);

        let err = TokenReader::peek(&mut tr, &st).unwrap_err();
        assert!(matches!(
            err.kind(),
            TokenErrorKind::ForbiddenChar(ForbiddenChar { ch: '%' })
        ));
        assert_eq!(err.span(), &SourceSpan::new(&source, sp(0, 1)));

        // Tolerant continuation: use the recovery token, resume past it.
        let recovery = err.into_recovery().unwrap();
        assert_eq!(recovery.token, char_token('%', 0, Span::empty(0)));
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;
        assert_eq!(recovery.resume, StdStreamPosition::at(1));
        reader.move_to_position(&recovery.resume);
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('f'));
    }

    #[test]
    fn end_of_stream_after_escape_error_with_recovery() {
        let source = Arc::new(Source::new(r"a \"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());

        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));

        let err = TokenReader::peek(&mut tr, &st).unwrap_err();
        assert!(matches!(
            err.kind(),
            TokenErrorKind::EndOfStreamAfterEscape(EndOfStreamAfterEscape {
                escape_char: '\\',
            })
        ));
        assert_eq!(err.span(), &SourceSpan::new(&source, sp(2, 3)));

        // Recovery: a Char placeholder covering the dangling escape byte itself, so the
        // byte stays in the tree (the tolerant parse keeps the partition invariant).
        let recovery = err.into_recovery().unwrap();
        assert_eq!(
            recovery.token,
            StdToken::char('\\', sp(2, 3), sp(1, 2)),
        );
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;
        assert_eq!(recovery.resume, StdStreamPosition::at(3));
        reader.move_to_position(&recovery.resume);
        let token = peek(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::EndOfStream);
    }

    // --- end of stream --------------------------------------------------------------------

    #[test]
    fn end_of_stream_token_is_terminal_and_idempotent() {
        let st = state(latex_rules());

        let source = Arc::new(Source::new(""));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &st),
            StdToken::end_of_stream(Span::empty(0)),
        );

        // Trailing whitespace (no paragraph break) is the end-of-stream token's
        // pre_space — reported, so it can land in the node tree.
        let source = Arc::new(Source::new("x   "));
        let mut tr = StdTokenReader::new(&source);
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('x'));
        let eos = StdToken::end_of_stream(sp(1, 4));
        assert_eq!(next(&mut tr, &st), eos);
        assert_eq!(at::<TestLang>(&tr), 4);
        assert!(tr.is_at_end());
        // Terminal: further reads yield end-of-stream again (now with empty pre_space,
        // the earlier trailing whitespace having been consumed).
        assert_eq!(
            next(&mut tr, &st),
            StdToken::end_of_stream(Span::empty(4)),
        );
    }

    // --- whitespace handling disabled ------------------------------------------------------

    #[test]
    fn whitespace_disabled_gives_character_level_content() {
        let source = Arc::new(Source::new("a b{"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.whitespace.enabled = false;
        let st = state(rules);
        assert_eq!(next(&mut tr, &st), char_token('a', 0, Span::empty(0)));
        assert_eq!(next(&mut tr, &st), char_token(' ', 1, Span::empty(1)));
        assert_eq!(next(&mut tr, &st), char_token('b', 2, Span::empty(2)));
        let token = next(&mut tr, &st);
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::GroupOpen { delim: "{", rule: &rule_of(BRACES) },
        );
    }

    // --- the groups gate (baked into the prefix table) -------------------------------------

    #[test]
    fn groups_gate_off_delimiters_are_plain_content() {
        let source = Arc::new(Source::new("{a}"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.groups.enabled = false;
        let st = state(rules);
        assert!(!st.rules().group_rules().is_empty());
        // Baked-in empty table (present feature, disabled at runtime: `Some`).
        assert!(st
            .prefix_table()
            .expect("all-present test language")
            .match_at("{a}")
            .is_none());
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('{'));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('}'));
    }

    #[test]
    fn groups_gate_off_does_not_gate_the_expected_close() {
        // The decided interaction (DESIGN_RATIONALE [§dd-dr:tokens]): expecting_group_close is
        // positional data, not a feature — a group interior that disables groups
        // entirely still finds its own close, so the entered group always terminates.
        let source = Arc::new(Source::new("a{$"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.groups.enabled = false;
        rules.groups.expecting_close = Some(rule_of(MATH_INLINE));
        let st = state(rules);
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('a'));
        // The table is off: `{` is plain content …
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::Char('{'));
        // … but the expected `$` close still tokenizes as GroupClose.
        let token = next(&mut tr, &st);
        assert_eq!(kind_of(&tr, &token), TokenKind::GroupClose { delim: "$" });
    }

    // --- movement -----------------------------------------------------------------------

    #[test]
    fn move_to_lands_at_each_of_the_five_edges() {
        let text = "  \\vec b";
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut tr;

        let token = reader.peek(&st).unwrap();
        assert_eq!(
            reader.token_kind(&token),
            TokenKind::Command { name: "vec", escape_char: '\\' }
        );
        assert_eq!(token.span(), sp(2, 7)); // includes post_space
        assert_eq!(token.pre_space(), sp(0, 2));

        // Each edge, checked through the text location the reader answers for the
        // position it lands on.
        let at = |reader: &dyn TokenReader<'_, TestLang>| {
            reader.source_position_at(&reader.position_here())
        };
        for (edge, offset) in [
            (TokenEdge::EndPastPostSpace, 7), // past post-space
            (TokenEdge::End, 6),              // before it (the `\verb`-style read)
            (TokenEdge::ContentStart, 3),     // past the escape character
            (TokenEdge::Start, 2),            // at the token, to read it again
            (TokenEdge::StartBeforePreSpace, 0), // giving back the pre-space too
        ] {
            reader.move_to(&token, edge);
            assert_eq!(at(reader), SourcePos::new(&source, offset), "{edge:?}");
            assert_eq!(reader.position_here(), reader.position_at(&token, edge));
        }

        // Reading again after the rewind yields the same token.
        assert_eq!(reader.peek(&st).unwrap(), token);
    }

    // --- an end-to-end walk (adapted from pylatexenc's multiple-tokens test) ---------------

    #[test]
    fn multiple_tokens_walk() {
        let text = "Text \\`accent and \\textbf{bold text} and $\\vec b$ \
                    vector \\& also Fran\\c cois\n\
                    \\begin{enumerate}[(i)]\n\
                    \\item Hi there!  % here goes a comment\n\
                    \\item[a] Hello!  @@@\n     \
                    \\end{enumerate}\n\
                    \\mymacro\n\n\
                    New paragraph\n";
        let st = state(latex_rules());
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);

        let find = |needle: &str| text.find(needle).unwrap();

        assert_eq!(next(&mut tr, &st), char_token('T', 0, Span::empty(0)));

        let p = find(r"\`");
        seek::<TestLang>(&mut tr, p);
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('\\', sp(p, p + 2), Span::empty(p), Span::empty(p + 2)),
        );

        let p = find(r"\textbf") - 1; // pre space
        seek::<TestLang>(&mut tr, p);
        assert_eq!(
            next(&mut tr, &st),
            // '{' follows the name: no post_space.
            StdToken::command('\\', sp(p + 1, p + 8), sp(p, p + 1), Span::empty(p + 8)),
        );
        let token = next(&mut tr, &st);
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::GroupOpen { delim: "{", rule: &rule_of(BRACES) },
        );

        let p = find(r"\vec"); // post-space present
        seek::<TestLang>(&mut tr, p);
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('\\', sp(p, p + 5), Span::empty(p), sp(p + 4, p + 5)),
        );

        let p = find(r"\&") - 1; // pre-space and *no* post-space
        seek::<TestLang>(&mut tr, p);
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('\\', sp(p + 1, p + 3), sp(p, p + 1), Span::empty(p + 3)),
        );

        // \begin is just a command token; the environment name is ordinary group+chars.
        let p = find(r"\begin");
        seek::<TestLang>(&mut tr, p);
        let token = next(&mut tr, &st);
        assert_eq!(
            kind_of(&tr, &token),
            TokenKind::Command { name: "begin", escape_char: '\\' },
        );

        let p = find("@@@") + 3; // pre space up to \end
        seek::<TestLang>(&mut tr, p);
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('\\', sp(p + 6, p + 10), sp(p, p + 6), Span::empty(p + 10)),
        );

        // The whole comment is one token: content to end of line, newline as post_space.
        let p = find("%") - 2;
        seek::<TestLang>(&mut tr, p);
        let nl = find(" % here goes a comment") + " % here goes a comment".len();
        assert_eq!(
            next(&mut tr, &st),
            StdToken::comment(sp(p + 2, p + 3), sp(p + 2, nl + 1), sp(p, p + 2), sp(nl, nl + 1)),
        );

        // \mymacro directly precedes a paragraph break: no post_space.
        let p = find(r"\mymacro");
        seek::<TestLang>(&mut tr, p);
        assert_eq!(
            next(&mut tr, &st),
            StdToken::command('\\', sp(p, p + 8), Span::empty(p), Span::empty(p + 8)),
        );
        let p2 = find("\n\n");
        assert_eq!(
            next(&mut tr, &st),
            StdToken::paragraph_break(sp(p2, p2 + 2), Span::empty(p2)),
        );

        // ... and the file's trailing newline is the end-of-stream token's pre_space.
        let p = find("New paragraph");
        seek::<TestLang>(&mut tr, p + "New paragraph".len());
        assert_eq!(
            next(&mut tr, &st),
            StdToken::end_of_stream(sp(text.len() - 1, text.len())),
        );
    }

    // --- the whitespace primitive directly -------------------------------------------------

    #[test]
    fn skip_whitespace_never_consumes_paragraph_newlines() {
        let rules: TokenRules<TestLang> = latex_rules();
        // Plain run (lone newline included): consumed fully.
        assert_eq!(skip_whitespace("  \n x", 0, &rules), 4);
        // Run holding a \n\s*\n sequence: stops before its first newline.
        assert_eq!(skip_whitespace("   \n  \n x", 0, &rules), 3);
        assert_eq!(skip_whitespace("\n\nx", 0, &rules), 0);
        // Flag off: everything is consumable.
        let mut no_par: TokenRules<TestLang> = latex_rules();
        no_par.paragraphs.enabled = false;
        assert_eq!(skip_whitespace("   \n  \n x", 0, &no_par), 8);
        // Whitespace handling disabled: nothing is skipped.
        let mut no_ws: TokenRules<TestLang> = latex_rules();
        no_ws.whitespace.enabled = false;
        assert_eq!(skip_whitespace("  x", 0, &no_ws), 0);
    }

    /// An invalid `pos` is a caller-contract violation and panics in all builds
    /// (the approved panic-policy exception).
    #[test]
    #[should_panic(expected = "char boundary")]
    fn skip_whitespace_panics_on_an_out_of_bounds_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = skip_whitespace("ab", 5, &rules);
    }

    /// A mid-character `pos` is the same contract violation (the boundary half).
    #[test]
    #[should_panic(expected = "char boundary")]
    fn skip_whitespace_panics_on_a_mid_char_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = skip_whitespace("é!", 1, &rules);
    }
}
