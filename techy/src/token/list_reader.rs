//! [`TokenListReader`]: a [`TokenReader`] over a pre-built token list.
//!
//! **Internal test infrastructure** — compiled under `cfg(test)` only, deliberately not
//! public API. Its purpose is testing construct
//! parsers in isolation: a hand-written or pre-scanned `Vec<Token>` drives a parser
//! without a live tokenizer, so a test can exercise exactly the token sequence it means
//! to — most importantly as the lockstep reader-agreement harness of the construct-parser
//! suites, the enforcement mechanism for "construct parsers never reach around the reader
//! into raw content". The fidelity gap below (a fixed list cannot re-tokenize under the
//! peek state) is acceptable for a test tool and is why it makes no public reader
//! contract.
//!
//! # Fidelity contract
//!
//! The reader keeps a byte **position**, exactly like [`StdTokenReader`]:
//! [`move_to`](super::TokenReader::move_to) places it at the named edge of
//! the token,
//! and `peek` returns the first listed token at or after the position, clipping its
//! `pre_space` to start no earlier than that position (re-peeking mid-pre-space behaves
//! like a fresh scan). Past the last token, `peek` yields the terminal idempotent
//! [`EndOfStream`](super::TokenKind::EndOfStream).
//!
//! The one inherent difference from a scanning reader: the list is **fixed**. Tokens are
//! not re-scanned under the state passed to `peek`, and a position placed *inside* a
//! token's span (e.g. a move to [`End`](super::TokenEdge::End), rewinding before
//! swallowed post-space — the `\verb` idiom) yields the *following* listed token — a
//! scanning reader would re-read the post-space bytes as fresh content. Tests that need
//! re-scanning behavior use [`StdTokenReader`] on real content.
//!
//! [`StdTokenReader`]: super::StdTokenReader

use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::source::{Source, SourcePos, SourceSpan, Span};
use crate::state::{Lang, ParsingState};

use super::error::TokenResult;
use super::reader::{StdStreamPosition, TokenEdge, TokenReader};
use super::token::{Token, TokenKind, TokenKindView};

/// A [`TokenReader`] serving tokens from a pre-built list (see the module docs for the
/// fidelity contract).
///
/// The tokens must be in source order (ascending, non-overlapping spans — the order a
/// scan produces); a trailing [`EndOfStream`](super::TokenKind::EndOfStream) token is
/// permitted but not required (one is synthesized past the end of the list).
///
/// # Rejecting what this reader never issued
///
/// This reader is the mechanical guard behind the lockstep harness: it **panics** when
/// handed a token or a stream position it did not issue, so a construct parser that
/// invents either — instead of asking the reader — fails loudly rather than quietly
/// producing a plausible wrong span. (Panicking is right here and only here: this is
/// test infrastructure, and the violation is a bug in the test suite's own code.)
///
/// - A **token** is accepted when a listed token has the same span and the same kind, or
///   when it is an end-of-stream token (which this reader synthesizes rather than serves
///   from the list). `pre_space` is left out of the comparison because `peek` clips it
///   to the current position.
/// - A **position** is accepted when its offset is one this reader handed out: the set
///   starts with the initial position alone and grows with the five edge offsets of
///   every token the reader serves, with every position it answers
///   (`position_here`/`position_at`), and with every offset it is moved to
///   (`move_to`). A position taken from somewhere the reader never served is
///   therefore rejected.
pub struct TokenListReader<'s, L: Lang> {
    source: &'s Arc<Source<L::SourceOrigin>>,
    tokens: Vec<Token<'s, L>>,
    pos: usize,
    /// Every offset this reader has handed out as a stream position (see
    /// [the validation rules](TokenListReader#rejecting-what-this-reader-never-issued)).
    /// A `RefCell` because the position accessors take `&self`.
    issued: RefCell<BTreeSet<usize>>,
}

impl<'s, L: Lang> TokenListReader<'s, L> {
    /// Create a reader positioned before the first token. The source-order contract
    /// (type docs) is the caller's; it is debug-asserted here, and an out-of-order
    /// list is not rejected — the parse's span bookkeeping reports the breakage as an
    /// implementation error where it surfaces.
    pub fn new(
        source: &'s Arc<Source<L::SourceOrigin>>,
        tokens: Vec<Token<'s, L>>,
    ) -> TokenListReader<'s, L> {
        debug_assert!(
            tokens.windows(2).all(|w| w[0].span.end() <= w[1].span.start()),
            "tokens must be in source order with non-overlapping spans"
        );
        let pos = tokens.first().map(|t| t.pre_space.start()).unwrap_or(0);
        // Seeded with the initial position alone: every other offset becomes valid only
        // by being served or answered (see the type's validation rules).
        let issued = RefCell::new(BTreeSet::from([pos]));
        TokenListReader { source, tokens, pos, issued }
    }

    /// Record `offset` as one this reader handed out.
    fn issue(&self, offset: usize) -> usize {
        self.issued.borrow_mut().insert(offset);
        offset
    }

    /// Panic unless `tok` is one this reader could have issued: a listed token with the
    /// same span and kind, or an end-of-stream token (which the reader synthesizes past
    /// the end of the list rather than serving from it). `pre_space` is excluded from
    /// the comparison — `peek` clips it to the current position, so a token that came
    /// out of this very reader need not compare equal to its list entry.
    fn check_issued(&self, tok: &Token<'_, L>, what: &str) {
        if matches!(tok.kind, TokenKind::EndOfStream) {
            return;
        }
        let known = self
            .tokens
            .iter()
            .any(|listed| listed.span == tok.span && listed.kind == tok.kind);
        assert!(
            known,
            "TokenListReader::{what} was handed a token it never issued: {:?}",
            tok
        );
    }

    /// Panic unless `at` is an offset this reader handed out (see the type's docs).
    fn check_position(&self, at: usize, what: &str) {
        assert!(
            self.issued.borrow().contains(&at),
            "TokenListReader::{what} was handed a position it never issued: {at}"
        );
    }

    /// The tokens being served.
    // Unused since the demotion to test-only (July 2026) — it existed as public API
    // surface. Kept for now; drop it if it stays unused.
    #[allow(dead_code)]
    pub fn tokens(&self) -> &[Token<'s, L>] {
        &self.tokens
    }

    /// The first listed token at or after the current position (its span not yet begun).
    fn current(&self) -> Option<&Token<'s, L>> {
        let i = self.tokens.partition_point(|t| t.span.start() < self.pos);
        self.tokens.get(i)
    }
}

/// The five edges, for seeding the issued-offset set.
const EVERY_EDGE: [TokenEdge; 5] = [
    TokenEdge::StartBeforePreSpace,
    TokenEdge::Start,
    TokenEdge::ContentStart,
    TokenEdge::End,
    TokenEdge::EndPastPostSpace,
];

impl<'s, L: Lang<StreamPosition = StdStreamPosition>> TokenReader<'s, L>
    for TokenListReader<'s, L>
{
    fn peek(&mut self, _state: &Arc<ParsingState<L>>) -> TokenResult<'s, L, Token<'s, L>> {
        match self.current() {
            Some(token) => {
                // Clip the recorded pre-space to the current position: peeking from
                // within (or past) the original pre-space run reports only the remainder,
                // as a fresh scan would.
                let mut token = token.clone();
                token.pre_space = Span::new(
                    token.pre_space.start().max(self.pos).min(token.pre_space.end()),
                    token.pre_space.end(),
                );
                for edge in EVERY_EDGE {
                    self.issue(token.edge_offset(edge));
                }
                Ok(token)
            }
            None => {
                self.issue(self.pos);
                Ok(Token::new(
                    TokenKind::EndOfStream,
                    Span::empty(self.pos),
                    Span::empty(self.pos),
                ))
            }
        }
    }

    fn move_to(&mut self, tok: &Token<'_, L>, edge: TokenEdge) {
        self.check_issued(tok, "move_to");
        self.pos = self.issue(tok.edge_offset(edge));
    }

    fn move_to_position(&mut self, at: &L::StreamPosition) {
        self.check_position(at.offset(), "move_to_position");
        self.pos = at.offset();
    }

    fn token_kind<'t>(&self, tok: &'t Token<'_, L>) -> TokenKindView<'t, L>
    where
        's: 't,
    {
        self.check_issued(tok, "token_kind");
        match &tok.kind {
            TokenKind::Char(c) => TokenKindView::Char(*c),
            TokenKind::GroupOpen { delim, rule } => TokenKindView::GroupOpen { delim, rule },
            TokenKind::GroupClose { delim } => TokenKindView::GroupClose { delim },
            TokenKind::Command { name, escape_char, .. } => {
                TokenKindView::Command { name, escape_char: *escape_char }
            }
            TokenKind::Specials { callable_type, name, spec } => {
                TokenKindView::Specials { callable_type: *callable_type, name, spec }
            }
            // Interpreted exactly as `StdTokenReader` interprets it: the delimiter is
            // the run of this source's content the token's `start` span names.
            TokenKind::Comment { start, content, .. } => TokenKindView::Comment {
                start_delim: self
                    .source
                    .content()
                    .get(start.start()..start.end())
                    .unwrap_or(""),
                content,
            },
            TokenKind::ParagraphBreak => TokenKindView::ParagraphBreak,
            TokenKind::EndOfStream => TokenKindView::EndOfStream,
        }
    }

    fn source_span_between(
        &self,
        tok: &Token<'_, L>,
        a: TokenEdge,
        b: TokenEdge,
    ) -> SourceSpan<L::SourceOrigin> {
        self.check_issued(tok, "source_span_between");
        let (a, b) = (tok.edge_offset(a), tok.edge_offset(b));
        SourceSpan::new(self.source, a.min(b)..a.max(b))
    }

    fn position_here(&self) -> L::StreamPosition {
        StdStreamPosition::at(self.issue(self.pos))
    }

    fn position_at(&self, tok: &Token<'_, L>, edge: TokenEdge) -> L::StreamPosition {
        self.check_issued(tok, "position_at");
        StdStreamPosition::at(self.issue(tok.edge_offset(edge)))
    }

    fn source_position_at(&self, at: &L::StreamPosition) -> SourcePos<L::SourceOrigin> {
        self.check_position(at.offset(), "source_position_at");
        SourcePos::new(self.source, at.offset())
    }

    fn source_span_within(
        &self,
        begin: &L::StreamPosition,
        end: &L::StreamPosition,
    ) -> Option<SourceSpan<L::SourceOrigin>> {
        self.check_position(begin.offset(), "source_span_within");
        self.check_position(end.offset(), "source_span_within");
        (begin.offset() <= end.offset())
            .then(|| SourceSpan::new(self.source, begin.offset()..end.offset()))
    }
}

impl<L: Lang> core::fmt::Debug for TokenListReader<'_, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TokenListReader")
            .field("tokens", &self.tokens.len())
            .field("pos", &self.pos)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scopes::ScopeStack;
    use crate::source::Source;
    use crate::state::{TrivialLang, StateData};
    use crate::token::{
        CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
        GroupRules, ParagraphRules, SpecialsRules, StdTokenReader, TokenRules,
        WhitespaceRules,
    };
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct TestLang;
    impl TrivialLang for TestLang {}

    fn latex_rules() -> TokenRules<TestLang> {
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

    fn state() -> Arc<ParsingState<TestLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: latex_rules(),
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    /// Scan `source` into the full token list with `StdTokenReader` (including the
    /// terminal `EndOfStream`).
    fn scan(source: &Arc<Source>) -> Vec<Token<'_, TestLang>> {
        let st = state();
        let mut tr = StdTokenReader::new(source);
        let mut tokens = Vec::new();
        loop {
            let token = TokenReader::next(&mut tr, &st).unwrap();
            let done = matches!(token.kind, TokenKind::EndOfStream);
            tokens.push(token);
            if done {
                break;
            }
        }
        tokens
    }

    #[test]
    fn round_trips_a_scanned_token_sequence() {
        let source: Arc<Source> = Arc::new(Source::new("ab \\vec c % note\n {x}\n\nz  "));
        let scanned = scan(&source);
        let st = state();

        let mut lr = TokenListReader::new(&source, scanned.clone());
        for expected in &scanned {
            assert_eq!(&TokenReader::next(&mut lr, &st).unwrap(), expected);
        }
    }

    #[test]
    fn peek_is_idempotent_and_does_not_advance() {
        let source: Arc<Source> = Arc::new(Source::new(" \\vec b"));
        let mut lr = TokenListReader::new(&source, scan(&source));
        let st = state();

        let start = TokenReader::position_here(&lr);
        let first = TokenReader::peek(&mut lr, &st).unwrap();
        assert_eq!(TokenReader::peek(&mut lr, &st).unwrap(), first);
        assert_eq!(TokenReader::position_here(&lr), start);
        assert_eq!(first.pre_space, Span::new(0, 1));
    }

    #[test]
    fn moving_to_each_edge_matches_the_std_reader() {
        // The movement matrix of the reader suite's edge test, run against both
        // readers in lockstep.
        let content = "  \\vec b";
        let st = state();
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut std_r = StdTokenReader::new(&source);
        let mut list_r = TokenListReader::new(&source, scan(&source));

        let token = TokenReader::peek(&mut std_r, &st).unwrap();
        assert_eq!(TokenReader::peek(&mut list_r, &st).unwrap(), token);

        for (edge, offset) in [
            (TokenEdge::EndPastPostSpace, 7),
            (TokenEdge::End, 6),
            (TokenEdge::ContentStart, 3),
            (TokenEdge::Start, 2),
            (TokenEdge::StartBeforePreSpace, 0),
        ] {
            let std_reader: &mut dyn TokenReader<'_, TestLang> = &mut std_r;
            std_reader.move_to(&token, edge);
            let at = std_reader.source_position_at(&std_reader.position_here());
            assert_eq!(at, SourcePos::new(&source, offset), "{edge:?}");

            let list_reader: &mut dyn TokenReader<'_, TestLang> = &mut list_r;
            list_reader.move_to(&token, edge);
            assert_eq!(
                list_reader.source_position_at(&list_reader.position_here()),
                at,
                "{edge:?}"
            );
        }

        // Reading again after the rewind yields the same token from both.
        assert_eq!(TokenReader::peek(&mut std_r, &st).unwrap(), token);
        assert_eq!(TokenReader::peek(&mut list_r, &st).unwrap(), token);
    }

    #[test]
    fn the_token_view_matches_the_std_readers_view() {
        // Both readers interpret the same tokens over the same content, so their views
        // agree kind for kind — the lockstep guarantee the construct-parser suites rest
        // on.
        let source: Arc<Source> = Arc::new(Source::new("a{\\vec  }% note\nb"));
        let st = state();
        let mut std_r = StdTokenReader::new(&source);
        let mut list_r = TokenListReader::new(&source, scan(&source));
        loop {
            let token = TokenReader::next(&mut std_r, &st).unwrap();
            let listed = TokenReader::next(&mut list_r, &st).unwrap();
            let std_reader: &dyn TokenReader<'_, TestLang> = &std_r;
            let list_reader: &dyn TokenReader<'_, TestLang> = &list_r;
            assert_eq!(list_reader.token_kind(&listed), std_reader.token_kind(&token));
            if matches!(token.kind, TokenKind::EndOfStream) {
                break;
            }
        }
    }

    #[test]
    fn the_comment_view_names_the_delimiter_and_the_text_between_it_and_the_post_space() {
        let source: Arc<Source> = Arc::new(Source::new("%% note\nx"));
        let st = state();
        let mut lr = TokenListReader::new(&source, scan(&source));
        let token = TokenReader::next(&mut lr, &st).unwrap();
        let reader: &dyn TokenReader<'_, TestLang> = &lr;
        // The rules define `%` as the only comment start, so the second `%` is content.
        assert_eq!(
            reader.token_kind(&token),
            TokenKindView::Comment { start_delim: "%", content: "% note" }
        );
        // The same two facts as edge answers — what the comment node records.
        assert_eq!(
            reader
                .source_span_between(&token, TokenEdge::Start, TokenEdge::ContentStart)
                .content(),
            "%"
        );
        assert_eq!(
            reader
                .source_span_between(&token, TokenEdge::ContentStart, TokenEdge::End)
                .content(),
            "% note"
        );
    }

    #[test]
    #[should_panic(expected = "was handed a token it never issued")]
    fn the_view_of_a_foreign_token_is_rejected() {
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let other: Arc<Source> = Arc::new(Source::new("zz"));
        let lr = TokenListReader::new(&source, scan(&source));
        let foreign = scan(&other);
        let reader: &dyn TokenReader<'_, TestLang> = &lr;
        let _ = reader.token_kind(&foreign[0]);
    }

    #[test]
    fn move_to_an_earlier_token_rewinds() {
        let source: Arc<Source> = Arc::new(Source::new("a {b}"));
        let scanned = scan(&source);
        let st = state();
        let mut lr = TokenListReader::new(&source, scanned.clone());

        let first = TokenReader::next(&mut lr, &st).unwrap();
        let second = TokenReader::next(&mut lr, &st).unwrap();
        assert_eq!(second, scanned[1]);

        TokenReader::move_to(&mut lr, &first, TokenEdge::Start);
        assert_eq!(
            TokenReader::position_here(&lr),
            TokenReader::position_at(&lr, &first, TokenEdge::Start)
        );
        assert_eq!(TokenReader::peek(&mut lr, &st).unwrap().kind, first.kind);
        // pre_space was already consumed positionally: clipped to empty.
        assert_eq!(
            TokenReader::peek(&mut lr, &st).unwrap().pre_space,
            Span::empty(first.span.start()),
        );
    }

    #[test]
    fn pre_space_clipped_when_peeking_mid_whitespace() {
        let source: Arc<Source> = Arc::new(Source::new("a   b"));
        let mut scanned = scan(&source);
        // A filler token over the first whitespace character, so that consuming it
        // leaves the reader inside `b`'s recorded pre-space run.
        scanned.insert(
            1,
            Token::new(TokenKind::Char(' '), Span::new(1, 2), Span::empty(1)),
        );
        let mut lr = TokenListReader::new(&source, scanned);
        let st = state();

        // Land inside b's pre-space run (1..4) the only way a reader ever lands
        // anywhere: by consuming a token that ends there. The hand-built list holds
        // one covering `1..2`, so reading it leaves the stream mid-whitespace.
        let _ = TokenReader::next(&mut lr, &st).unwrap(); // `a`
        let _ = TokenReader::next(&mut lr, &st).unwrap(); // the 1..2 filler
        let token = TokenReader::peek(&mut lr, &st).unwrap();
        assert_eq!(token.kind, TokenKind::Char('b'));
        assert_eq!(token.pre_space, Span::new(2, 4));
    }

    #[test]
    fn end_of_stream_is_terminal_and_idempotent() {
        let content = "x  ";
        let st = state();
        // Same walk against both readers: EOS carries the trailing whitespace once,
        // then repeats with empty pre_space.
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut std_r = StdTokenReader::new(&source);
        let mut list_r = TokenListReader::new(&source, scan(&source));
        for _ in 0..2 {
            let a = TokenReader::next(&mut std_r, &st).unwrap();
            let b = TokenReader::next(&mut list_r, &st).unwrap();
            assert_eq!(a, b);
        }
        let a = TokenReader::next(&mut std_r, &st).unwrap();
        let b = TokenReader::next(&mut list_r, &st).unwrap();
        assert_eq!(a, b);
        assert_eq!(b, Token::new(TokenKind::EndOfStream, Span::empty(3), Span::empty(3)));
    }

    #[test]
    fn positions_and_spans_match_the_std_reader_in_lockstep() {
        let content = "a \\vec  b";
        let st = state();
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut std_r = StdTokenReader::new(&source);
        let mut list_r = TokenListReader::new(&source, scan(&source));
        let std_reader: &mut dyn TokenReader<'_, TestLang> = &mut std_r;
        let list_reader: &mut dyn TokenReader<'_, TestLang> = &mut list_r;

        assert_eq!(std_reader.position_here(), list_reader.position_here());

        let a = std_reader.next(&st).unwrap();
        let b = list_reader.next(&st).unwrap();
        assert_eq!(a, b);

        let token = std_reader.peek(&st).unwrap();
        assert_eq!(list_reader.peek(&st).unwrap(), token);

        for edge in [
            TokenEdge::StartBeforePreSpace,
            TokenEdge::Start,
            TokenEdge::ContentStart,
            TokenEdge::End,
            TokenEdge::EndPastPostSpace,
        ] {
            assert_eq!(
                std_reader.position_at(&token, edge),
                list_reader.position_at(&token, edge),
                "positions disagree at {edge:?}"
            );
            assert_eq!(
                std_reader.source_span_between(&token, TokenEdge::Start, edge),
                list_reader.source_span_between(&token, TokenEdge::Start, edge),
                "spans disagree at {edge:?}"
            );
            std_reader.move_to(&token, edge);
            list_reader.move_to(&token, edge);
            assert_eq!(std_reader.position_here(), list_reader.position_here());
        }

        assert_eq!(std_reader.source_span_of(&token), list_reader.source_span_of(&token));

        let begin = std_reader.position_at(&token, TokenEdge::Start);
        let end = std_reader.position_at(&token, TokenEdge::EndPastPostSpace);
        assert_eq!(
            std_reader.source_span_within(&begin, &end),
            list_reader.source_span_within(&begin, &end)
        );
        assert_eq!(
            std_reader.source_position_at(&begin),
            list_reader.source_position_at(&begin)
        );
    }

    #[test]
    #[should_panic(expected = "a token it never issued")]
    fn a_token_this_reader_never_issued_is_rejected() {
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let mut lr = TokenListReader::new(&source, scan(&source));
        let reader: &mut dyn TokenReader<'_, TestLang> = &mut lr;

        // Not from this reader — and not from any scan of this content.
        let forged = Token::new(TokenKind::Char('z'), Span::new(0, 1), Span::empty(0));
        let _ = reader.source_span_between(&forged, TokenEdge::Start, TokenEdge::End);
    }

    #[test]
    #[should_panic(expected = "a position it never issued")]
    fn a_position_this_reader_never_issued_is_rejected() {
        // The list covers only the first two characters of the source, so a position
        // taken further along is one this reader never handed out.
        let source: Arc<Source> = Arc::new(Source::new("ab cd"));
        let prefix: Vec<Token<'_, TestLang>> =
            scan(&source).into_iter().take(2).collect();
        let mut lr = TokenListReader::new(&source, prefix);
        let mut std_r = StdTokenReader::new(&source);
        let std_reader: &mut dyn TokenReader<'_, TestLang> = &mut std_r;
        // Read past the list's extent: the position the std reader stands at then is
        // one this list reader never handed out.
        for _ in 0..3 {
            let _ = std_reader.next(&state()).unwrap();
        }
        let elsewhere = std_reader.position_here();

        let reader: &mut dyn TokenReader<'_, TestLang> = &mut lr;
        reader.move_to_position(&elsewhere);
    }

    #[test]
    fn empty_list_yields_end_of_stream() {
        let source: Arc<Source> = Arc::new(Source::new(""));
        let mut lr: TokenListReader<'_, TestLang> = TokenListReader::new(&source, vec![]);
        let st = state();
        assert_eq!(
            TokenReader::peek(&mut lr, &st).unwrap(),
            Token::new(TokenKind::EndOfStream, Span::empty(0), Span::empty(0)),
        );
    }

    #[test]
    fn hand_built_lists_need_only_a_long_enough_source() {
        // The unit-test use case: a token list written by hand, spans made up — the
        // source only has to be long enough for the spans to be valid ranges in it.
        let source: Arc<Source> = Arc::new(Source::new(r"x\vec  "));
        let tokens: Vec<Token<'static, TestLang>> = vec![
            Token::new(TokenKind::Char('x'), Span::new(0, 1), Span::empty(0)),
            Token::new(
                TokenKind::Command { name: "vec", escape_char: '\\', post_space: Span::new(5, 6) },
                Span::new(1, 6),
                Span::empty(1),
            ),
        ];
        let st = state();
        let mut lr = TokenListReader::new(&source, tokens.clone());
        assert_eq!(TokenReader::next(&mut lr, &st).unwrap(), tokens[0]);
        assert_eq!(TokenReader::next(&mut lr, &st).unwrap(), tokens[1]);
        assert_eq!(
            TokenReader::source_position_at(&lr, &TokenReader::position_here(&lr)),
            SourcePos::new(&source, 6)
        );
        assert_eq!(
            TokenReader::peek(&mut lr, &st).unwrap().kind,
            TokenKind::EndOfStream,
        );
    }
}
