//! [`TokenListReader`]: a [`TokenReader`] over a pre-built token list.
//!
//! Its purpose is testing construct parsers in isolation (Phase 6): a hand-written or
//! pre-scanned `Vec<Token>` drives a parser without a live tokenizer, so a test can
//! exercise exactly the token sequence it means to. It is also the existence proof that
//! the parsing layer holds up its end of the [`TokenReader`] abstraction — construct
//! parsers must never reach around the reader into raw content.
//!
//! # Fidelity contract
//!
//! The reader keeps a byte **position**, exactly like [`StdTokenReader`]: `move_past` /
//! `move_to` place `pos()` per the span conventions (after the token or its post-space;
//! at the token start or before its pre-space), and `peek` returns the first listed token
//! at or after the position, clipping its `pre_space` to start no earlier than `pos()`
//! (re-peeking mid-pre-space behaves like a fresh scan). Past the last token, `peek`
//! yields the terminal idempotent [`EndOfStream`](super::TokenKind::EndOfStream).
//!
//! The one inherent difference from a scanning reader: the list is **fixed**. Tokens are
//! not re-scanned under the state passed to `peek`, and a position placed *inside* a
//! token's span (e.g. `move_past(tok, false)` rewinding before swallowed post-space, the
//! `\verb` idiom) yields the *following* listed token — a scanning reader would re-read
//! the post-space bytes as fresh content. Tests that need re-scanning behavior use
//! [`StdTokenReader`] on real content.
//!
//! [`StdTokenReader`]: super::StdTokenReader

use alloc::vec::Vec;

use crate::source::Span;
use crate::state::{Lang, ParsingState};

use super::error::TokenResult;
use super::reader::TokenReader;
use super::token::{Token, TokenKind};

/// A [`TokenReader`] serving tokens from a pre-built list (see the module docs for the
/// fidelity contract).
///
/// The tokens must be in source order (ascending, non-overlapping spans — the order a
/// scan produces); a trailing [`EndOfStream`](super::TokenKind::EndOfStream) token is
/// permitted but not required (one is synthesized past the end of the list).
pub struct TokenListReader<'s, L: Lang> {
    tokens: Vec<Token<'s, L>>,
    pos: usize,
}

impl<'s, L: Lang> TokenListReader<'s, L> {
    /// Create a reader positioned before the first token.
    pub fn new(tokens: Vec<Token<'s, L>>) -> TokenListReader<'s, L> {
        debug_assert!(
            tokens.windows(2).all(|w| w[0].span.end <= w[1].span.start),
            "tokens must be in source order with non-overlapping spans"
        );
        let pos = tokens.first().map(|t| t.pre_space.start).unwrap_or(0);
        TokenListReader { tokens, pos }
    }

    /// The tokens being served.
    pub fn tokens(&self) -> &[Token<'s, L>] {
        &self.tokens
    }

    /// Move to an absolute byte position (mirrors `StdTokenReader::move_to_pos`).
    pub fn move_to_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// The first listed token at or after the current position (its span not yet begun).
    fn current(&self) -> Option<&Token<'s, L>> {
        let i = self.tokens.partition_point(|t| t.span.start < self.pos);
        self.tokens.get(i)
    }
}

impl<'s, L: Lang> TokenReader<'s, L> for TokenListReader<'s, L> {
    fn peek(&mut self, _state: &ParsingState<L>) -> TokenResult<'s, L, Token<'s, L>> {
        match self.current() {
            Some(token) => {
                // Clip the recorded pre-space to the current position: peeking from
                // within (or past) the original pre-space run reports only the remainder,
                // as a fresh scan would.
                let mut token = token.clone();
                token.pre_space = Span::new(
                    token.pre_space.start.max(self.pos).min(token.pre_space.end),
                    token.pre_space.end,
                );
                Ok(token)
            }
            None => Ok(Token::new(
                TokenKind::EndOfStream,
                Span::empty(self.pos),
                Span::empty(self.pos),
            )),
        }
    }

    fn move_past(&mut self, tok: &Token<'s, L>, skip_post_space: bool) {
        if skip_post_space {
            self.pos = tok.span.end;
        } else {
            // Post-space is a trailing sub-range of `span`, so its `start` is the end
            // of the token proper — underflow-free even for hand-built tokens.
            self.pos = tok.post_space().start;
        }
    }

    fn move_to(&mut self, tok: &Token<'s, L>, rewind_pre_space: bool) {
        if rewind_pre_space {
            self.pos = tok.pre_space.start;
        } else {
            self.pos = tok.span.start;
        }
    }

    fn pos(&self) -> usize {
        self.pos
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
    use crate::library::LibraryStack;
    use crate::state::{SimpleLang, StateData};
    use crate::token::{
        CommandRule, CommentRule, GroupRule, StdTokenReader, TokenRules, WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct TestLang;
    impl SimpleLang for TestLang {}

    fn latex_rules() -> TokenRules<TestLang> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n\r".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups: vec![Arc::new(GroupRule {
                group_type: 0,
                open: "{".into(),
                close: "}".into(),
            })],
            enable_commands: true,
            commands: vec![CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            }],
            enable_comments: true,
            comments: vec![CommentRule { start: "%".into() }],
            enable_specials: true,
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn state() -> ParsingState<TestLang> {
        ParsingState::new(StateData {
            rules: latex_rules(),
            libraries: LibraryStack::new(),
            ext: (),
        })
    }

    /// Scan `content` into the full token list with `StdTokenReader` (including the
    /// terminal `EndOfStream`).
    fn scan(content: &str) -> Vec<Token<'_, TestLang>> {
        let st = state();
        let mut tr = StdTokenReader::new(content);
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
        let content = "ab \\vec c % note\n {x}\n\nz  ";
        let scanned = scan(content);
        let st = state();

        let mut lr = TokenListReader::new(scanned.clone());
        for expected in &scanned {
            assert_eq!(&TokenReader::next(&mut lr, &st).unwrap(), expected);
        }
    }

    #[test]
    fn peek_is_idempotent_and_does_not_advance() {
        let content = " \\vec b";
        let mut lr = TokenListReader::new(scan(content));
        let st = state();

        let start = TokenReader::pos(&lr);
        let first = TokenReader::peek(&mut lr, &st).unwrap();
        assert_eq!(TokenReader::peek(&mut lr, &st).unwrap(), first);
        assert_eq!(TokenReader::pos(&lr), start);
        assert_eq!(first.pre_space, Span::new(0, 1));
    }

    #[test]
    fn move_past_and_move_to_flags_match_std_reader() {
        // The movement matrix of the Phase 3 `move_past_and_move_to_flags` test, run
        // against both readers in lockstep.
        let content = "  \\vec b";
        let st = state();
        let mut std_r = StdTokenReader::new(content);
        let mut list_r = TokenListReader::new(scan(content));

        let token = TokenReader::peek(&mut std_r, &st).unwrap();
        assert_eq!(TokenReader::peek(&mut list_r, &st).unwrap(), token);

        for (skip_post_space, expected_pos) in [(true, 7), (false, 6)] {
            TokenReader::move_past(&mut std_r, &token, skip_post_space);
            TokenReader::move_past(&mut list_r, &token, skip_post_space);
            assert_eq!(std_r.pos(), expected_pos);
            assert_eq!(TokenReader::pos(&list_r), expected_pos);
        }
        for (rewind_pre_space, expected_pos) in [(false, 2), (true, 0)] {
            TokenReader::move_to(&mut std_r, &token, rewind_pre_space);
            TokenReader::move_to(&mut list_r, &token, rewind_pre_space);
            assert_eq!(std_r.pos(), expected_pos);
            assert_eq!(TokenReader::pos(&list_r), expected_pos);
        }

        // Reading again after move_to yields the same token from both.
        assert_eq!(TokenReader::peek(&mut std_r, &st).unwrap(), token);
        assert_eq!(TokenReader::peek(&mut list_r, &st).unwrap(), token);
    }

    #[test]
    fn move_to_an_earlier_token_rewinds() {
        let content = "a {b}";
        let scanned = scan(content);
        let st = state();
        let mut lr = TokenListReader::new(scanned.clone());

        let first = TokenReader::next(&mut lr, &st).unwrap();
        let second = TokenReader::next(&mut lr, &st).unwrap();
        assert_eq!(second, scanned[1]);

        TokenReader::move_to(&mut lr, &first, false);
        assert_eq!(TokenReader::pos(&lr), first.span.start);
        assert_eq!(TokenReader::peek(&mut lr, &st).unwrap().kind, first.kind);
        // pre_space was already consumed positionally: clipped to empty.
        assert_eq!(
            TokenReader::peek(&mut lr, &st).unwrap().pre_space,
            Span::empty(first.span.start),
        );
    }

    #[test]
    fn pre_space_clipped_when_peeking_mid_whitespace() {
        let content = "a   b";
        let mut lr = TokenListReader::new(scan(content));
        let st = state();

        lr.move_to_pos(2); // inside b's pre-space run (1..4)
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
        let mut std_r = StdTokenReader::new(content);
        let mut list_r = TokenListReader::new(scan(content));
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
    fn empty_list_yields_end_of_stream() {
        let mut lr: TokenListReader<'_, TestLang> = TokenListReader::new(vec![]);
        let st = state();
        assert_eq!(
            TokenReader::peek(&mut lr, &st).unwrap(),
            Token::new(TokenKind::EndOfStream, Span::empty(0), Span::empty(0)),
        );
    }

    #[test]
    fn hand_built_lists_need_no_source() {
        // The unit-test use case: a token list written by hand, spans made up.
        let tokens: Vec<Token<'static, TestLang>> = vec![
            Token::new(TokenKind::Char('x'), Span::new(0, 1), Span::empty(0)),
            Token::new(
                TokenKind::Command { name: "vec", escape_char: '\\', post_space: Span::new(5, 6) },
                Span::new(1, 6),
                Span::empty(1),
            ),
        ];
        let st = state();
        let mut lr = TokenListReader::new(tokens.clone());
        assert_eq!(TokenReader::next(&mut lr, &st).unwrap(), tokens[0]);
        assert_eq!(TokenReader::next(&mut lr, &st).unwrap(), tokens[1]);
        assert_eq!(TokenReader::pos(&lr), 6);
        assert_eq!(
            TokenReader::peek(&mut lr, &st).unwrap().kind,
            TokenKind::EndOfStream,
        );
    }
}
