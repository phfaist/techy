//! The [`TokenReader`] trait and the standard rules-driven implementation,
//! [`StdTokenReader`].
//!
//! `StdTokenReader` follows pylatexenc's proven `LatexTokenReader` protocol: `peek` parses
//! the token at the current position without advancing; `move_past`/`move_to` reposition
//! relative to a token; `next` = peek + move-past. The scanning core is decomposed into
//! private `detect_*`/`read_*` methods, each driven by one facet of the
//! [`TokenRules`] — except specials recognition, which is delegated to
//! [`Lang::scan_specials`] (gated by the state's cached
//! [`TriggerChars`](super::TriggerChars) filter).
//!
//! The whitespace primitive [`skip_whitespace`] implements the multi-newline rule in one
//! place for pre-space, command post-space, and comment post-space alike: when
//! [`TokenRules::enable_multi_newline_paragraphs`] is set, skipped whitespace never consumes a
//! newline belonging to a `\n\s*\n` sequence — such a sequence always surfaces as a
//! [`ParagraphBreak`](TokenKind::ParagraphBreak) token.

use crate::source::Span;
use crate::state::{Lang, ParsingState};

use super::error::{
    EndOfStreamAfterEscape, ForbiddenChar, TokenError, TokenErrorKind, TokenRecovery,
    TokenResult,
};
use super::rules::{CommandRule, TokenRules, WhitespaceRules};
use super::token::{Token, TokenKind};

/// The token-reading protocol — the behavior extension point for genuinely different
/// tokenization (catcode-like schemes, non-textual sources). `peek` receives the full
/// [`ParsingState<L>`], not just `&TokenRules`: a custom reader keeps its tables in
/// `L::StateExt`, which only the state exposes (ARCHITECTURE.md §token).
///
/// # Contract
///
/// - **`peek` is idempotent per (position, state instance):** repeated calls at the same
///   position with the *same* `ParsingState` instance return the same result, and
///   implementations may memoize on that key (states are immutable, so `Arc` pointer
///   identity is a sound cache key). A *different* state — even one derived with an empty
///   delta — relieves `peek` of any obligation to repeat itself.
/// - At the end of the stream `peek` returns the terminal, idempotent
///   [`EndOfStream`](TokenKind::EndOfStream) token (never an `Option`); its `pre_space`
///   carries the final whitespace.
pub trait TokenReader<'s, L: Lang> {
    /// Parse the token at the current position without advancing.
    fn peek(&mut self, state: &ParsingState<L>) -> TokenResult<'s, L, Token<'s, L>>;

    /// Move immediately past `tok`. If `skip_post_space` is true the position lands after
    /// the token's post-space; otherwise right after the token proper, before it.
    fn move_past(&mut self, tok: &Token<'s, L>, skip_post_space: bool);

    /// Move to `tok`'s own start, so that it would be read again. If `rewind_pre_space`
    /// is true the position lands before the token's preceding whitespace instead.
    fn move_to(&mut self, tok: &Token<'s, L>, rewind_pre_space: bool);

    /// Current byte position.
    fn pos(&self) -> usize;

    /// Parse the token at the current position and move past it (including its
    /// post-space): [`peek`](TokenReader::peek) + [`move_past`](TokenReader::move_past).
    fn next(&mut self, state: &ParsingState<L>) -> TokenResult<'s, L, Token<'s, L>> {
        let token = self.peek(state)?;
        self.move_past(&token, true);
        Ok(token)
    }
}

/// End position of the whitespace run starting at `pos` (= `pos` if none, or if
/// whitespace handling is disabled).
///
/// **The multi-newline rule** (`TokenRules::enable_multi_newline_paragraphs`): skipped
/// whitespace never contains `\n\s*\n`, nor consumes a newline from such a sequence —
/// skipping stops right *before* the first newline of a paragraph break. This one
/// primitive serves pre-space, command post-space, and comment post-space, which is what
/// makes "post-space never crosses a paragraph break" hold everywhere by construction.
pub fn skip_whitespace<L: Lang>(content: &str, pos: usize, rules: &TokenRules<L>) -> usize {
    if !rules.enable_whitespace {
        return pos;
    }
    let ws = &rules.whitespace;
    let mut end = pos;
    for c in content[pos..].chars() {
        if !ws.chars.contains(c) {
            break;
        }
        if c == '\n'
            && rules.enable_multi_newline_paragraphs
            && paragraph_continues(content, end + 1, ws)
        {
            break;
        }
        end += c.len_utf8();
    }
    end
}

/// Whether another newline follows within the whitespace run starting at `after_nl`
/// (i.e. the newline just before `after_nl` opens a `\n\s*\n` paragraph sequence).
fn paragraph_continues(content: &str, after_nl: usize, ws: &WhitespaceRules) -> bool {
    for c in content[after_nl..].chars() {
        if c == '\n' {
            return true;
        }
        if !ws.chars.contains(c) {
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
#[derive(Debug, Clone)]
pub struct StdTokenReader<'s> {
    content: &'s str,
    pos: usize,
}

impl<'s> StdTokenReader<'s> {
    /// Create a reader positioned at the start of `content`.
    pub fn new(content: &'s str) -> StdTokenReader<'s> {
        StdTokenReader { content, pos: 0 }
    }

    /// The content being tokenized.
    pub fn content(&self) -> &'s str {
        self.content
    }

    /// Current byte position.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Whether the reader is at the end of the content.
    pub fn is_at_end(&self) -> bool {
        self.pos >= self.content.len()
    }

    /// Move to an absolute byte position (must lie on a `char` boundary).
    pub fn move_to_pos(&mut self, pos: usize) {
        debug_assert!(pos <= self.content.len(), "position {} beyond content end", pos);
        debug_assert!(self.content.is_char_boundary(pos), "position {} not on char boundary", pos);
        self.pos = pos;
    }

    // --- scanning core ------------------------------------------------------------------

    fn peek_impl<L: Lang>(
        &self,
        state: &ParsingState<L>,
    ) -> TokenResult<'s, L, Token<'s, L>> {
        let s = self.content;
        let rules = state.rules();
        let start = self.pos;

        let ws_end = skip_whitespace(s, start, rules);
        let pre_space = Span::new(start, ws_end);

        // skip_whitespace stops right before the first newline of a paragraph break, so a
        // break (which trumps everything, including end-of-stream) is detectable here.
        if let Some(token) = self.detect_paragraph_break(ws_end, pre_space, rules) {
            return Ok(token);
        }

        let pos = ws_end;
        if pos >= s.len() {
            return Ok(Token::new(TokenKind::EndOfStream, Span::empty(pos), pre_space));
        }

        // Group delimiters come before commands so that escape-char-led delimiters like
        // `\(` win over command interpretation (as in pylatexenc, where math delimiters
        // are checked first).
        if let Some(token) = self.detect_group_delimiter(pos, pre_space, state) {
            return Ok(token);
        }

        let c = s[pos..].chars().next().expect("pos < len checked above");

        if rules.enable_commands {
            if let Some(rule) = rules.commands.iter().find(|r| c == r.escape_char) {
                return self.read_command(pos, pre_space, rules, rule);
            }
        }

        if let Some(token) = self.read_comment(pos, pre_space, rules) {
            return Ok(token);
        }

        if state.trigger_chars().may_start(c) {
            if let Some(m) = L::scan_specials(state, s, pos)? {
                // A malformed `end` from the hook would yield a zero-width token (the
                // dispatch loop would never advance) or a span that panics when sliced.
                debug_assert!(
                    m.end > pos && m.end <= s.len() && s.is_char_boundary(m.end),
                    "scan_specials returned an invalid match end {} at pos {}",
                    m.end,
                    pos
                );
                return Ok(Token::new(
                    TokenKind::Specials {
                        callable_type: m.callable_type,
                        name: m.name,
                        spec: m.spec,
                    },
                    Span::new(pos, m.end),
                    pre_space,
                ));
            }
        }

        if rules.forbidden_chars.contains(c) {
            let span = Span::new(pos, pos + c.len_utf8());
            let placeholder = Token::new(TokenKind::Char(c), span, pre_space);
            return Err(TokenError::new(
                TokenErrorKind::ForbiddenChar(ForbiddenChar::new(c)),
                span,
                Some(TokenRecovery { token: placeholder, resume_pos: span.end }),
            ));
        }

        Ok(Token::new(TokenKind::Char(c), Span::new(pos, pos + c.len_utf8()), pre_space))
    }

    /// A `ParagraphBreak` token if a `\n\s*\n` whitespace sequence starts at `pos` (which
    /// `skip_whitespace` guarantees whenever it stopped at a consumable-whitespace
    /// newline). The token spans from the first through the last newline of the run;
    /// whitespace after the last newline is left for the next token's pre-space.
    fn detect_paragraph_break<L: Lang>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
    ) -> Option<Token<'s, L>> {
        if !rules.enable_multi_newline_paragraphs || !rules.enable_whitespace {
            return None;
        }
        let ws = &rules.whitespace;
        if !self.content[pos..].starts_with('\n') || !ws.chars.contains('\n') {
            return None;
        }
        let mut newlines = 0usize;
        let mut end = pos;
        let mut last_nl_end = pos;
        for c in self.content[pos..].chars() {
            if !ws.chars.contains(c) {
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
        Some(Token::new(TokenKind::ParagraphBreak, Span::new(pos, last_nl_end), pre_space))
    }

    /// A `GroupOpen`/`GroupClose` token at `pos`, if a delimiter matches. The close
    /// delimiter expected per `rules.expecting_group_close` takes precedence; otherwise
    /// the longest table match wins, read as an opener when the string is ambiguous.
    fn detect_group_delimiter<L: Lang>(
        &self,
        pos: usize,
        pre_space: Span,
        state: &ParsingState<L>,
    ) -> Option<Token<'s, L>> {
        let rules = state.rules();
        let rest = &self.content[pos..];

        if let Some(expected) = &rules.expecting_group_close {
            if !expected.close.is_empty() && rest.starts_with(expected.close.as_str()) {
                let span = Span::new(pos, pos + expected.close.len());
                return Some(Token::new(
                    TokenKind::GroupClose { delim: span.slice(self.content) },
                    span,
                    pre_space,
                ));
            }
        }

        let entry = state.prefix_table().match_at(rest)?;
        let span = Span::new(pos, pos + entry.delim().len());
        let delim = span.slice(self.content);
        let kind = match (entry.open(), entry.close()) {
            (Some(rule), _) => TokenKind::GroupOpen { delim, rule: rule.clone() },
            (None, Some(_)) => TokenKind::GroupClose { delim },
            (None, None) => unreachable!("prefix table entries always carry a direction"),
        };
        Some(Token::new(kind, span, pre_space))
    }

    /// Read a command token at `pos` (the escape character's position). The name is a
    /// greedy run of the rule's name characters, or a single character if the first one
    /// is not a name character. Multi-character names consume their following whitespace
    /// as post-space — syntactic whitespace, never crossing a paragraph break (enforced
    /// by [`skip_whitespace`] itself).
    fn read_command<L: Lang>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
        rule: &CommandRule,
    ) -> TokenResult<'s, L, Token<'s, L>> {
        let s = self.content;
        let name_start = pos + rule.escape_char.len_utf8();

        if name_start >= s.len() {
            // Recovery: pretend the stream ended here, resume at end of input.
            let placeholder = Token::new(TokenKind::EndOfStream, Span::empty(pos), pre_space);
            return Err(TokenError::new(
                TokenErrorKind::EndOfStreamAfterEscape(EndOfStreamAfterEscape::new(
                    rule.escape_char,
                )),
                Span::new(pos, name_start),
                Some(TokenRecovery { token: placeholder, resume_pos: s.len() }),
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

        Ok(Token::new(
            TokenKind::Command {
                name: &s[name_start..name_end],
                escape_char: rule.escape_char,
                post_space,
            },
            Span::new(pos, post_space.end),
            pre_space,
        ))
    }

    /// A whole-comment token at `pos`, if a comment-start delimiter matches
    /// (longest-first across the rules). The content runs to the end of the line; the
    /// terminating newline plus following indentation is the token's post-space — unless
    /// that whitespace forms a paragraph break, in which case the comment takes no
    /// post-space and the break surfaces as its own token.
    fn read_comment<L: Lang>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
    ) -> Option<Token<'s, L>> {
        if !rules.enable_comments {
            return None;
        }
        let s = self.content;
        let rest = &s[pos..];
        let start = rules
            .comments
            .iter()
            .map(|r| r.start.as_str())
            .filter(|d| !d.is_empty() && rest.starts_with(d))
            .max_by_key(|d| d.len())?;

        let content_start = pos + start.len();
        let content_end = match s[content_start..].find('\n') {
            Some(i) => content_start + i,
            None => s.len(),
        };
        let post_space = Span::new(content_end, skip_whitespace(s, content_end, rules));

        Some(Token::new(
            TokenKind::Comment { content: &s[content_start..content_end], post_space },
            Span::new(pos, post_space.end),
            pre_space,
        ))
    }
}

impl<'s, L: Lang> TokenReader<'s, L> for StdTokenReader<'s> {
    fn peek(&mut self, state: &ParsingState<L>) -> TokenResult<'s, L, Token<'s, L>> {
        self.peek_impl(state)
    }

    fn move_past(&mut self, tok: &Token<'s, L>, skip_post_space: bool) {
        if skip_post_space {
            self.pos = tok.span.end;
        } else {
            // Post-space is a trailing sub-range of `span`, so its `start` is the end
            // of the token proper — for every kind (empty post-space sits at `span.end`).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LibraryStack;
    use crate::spec::CallableSpec;
    use crate::state::StateData;
    use crate::token::{CommentRule, GroupRule, SpecialsMatch, TriggerChars};
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    // Group classes used by the hardcoded latexlike-flavored test rules (the test langs
    // use the SimpleLang-style `u32` class space; a real preset would use a small enum,
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
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
    }

    /// Hardcoded LaTeX-flavored rules; the real defaults arrive with the latexlike
    /// preset (Phase 7). Generic so the several test langs of this module can share it.
    fn latex_rules<L: Lang<GroupTypeId = u32>>() -> TokenRules<L> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n\r\u{000B}\u{000C}".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups: vec![
                group(BRACES, "{", "}"),
                group(BRACKETS, "[", "]"),
                group(MATH_INLINE, "$", "$"),
                group(MATH_DISPLAY, "$$", "$$"),
                group(MATH_INLINE_PAREN, r"\(", r"\)"),
                group(MATH_DISPLAY_BRACKET, r"\[", r"\]"),
            ],
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

    fn group<L: Lang<GroupTypeId = u32>>(
        group_type: u32,
        open: &str,
        close: &str,
    ) -> Arc<GroupRule<L>> {
        Arc::new(GroupRule { group_type, open: open.into(), close: close.into() })
    }

    fn state(rules: TokenRules<TestLang>) -> ParsingState<TestLang> {
        ParsingState::new(StateData { rules, libraries: LibraryStack::new(), ext: () })
    }

    /// The `latex_rules` rule of the given class (unique per rule in these tests).
    fn rule_of(group_type: u32) -> Arc<GroupRule<TestLang>> {
        latex_rules::<TestLang>()
            .groups
            .into_iter()
            .find(|g| g.group_type == group_type)
            .expect("class present in latex_rules")
    }

    /// Rules with the given rule's close delimiter expected (as the group parser sets up
    /// when entering an ambiguously-delimited group).
    fn expecting_close(group_type: u32) -> ParsingState<TestLang> {
        state(TokenRules { expecting_group_close: Some(rule_of(group_type)), ..latex_rules() })
    }

    fn sp(start: usize, end: usize) -> Span {
        Span::new(start, end)
    }

    fn peek<'s>(tr: &mut StdTokenReader<'s>, st: &ParsingState<TestLang>) -> Token<'s, TestLang> {
        TokenReader::peek(tr, st).unwrap()
    }

    fn next<'s>(tr: &mut StdTokenReader<'s>, st: &ParsingState<TestLang>) -> Token<'s, TestLang> {
        TokenReader::next(tr, st).unwrap()
    }

    fn char_token(c: char, at: usize, pre_space: Span) -> Token<'static, TestLang> {
        Token::new(TokenKind::Char(c), sp(at, at + c.len_utf8()), pre_space)
    }

    // --- chars ------------------------------------------------------------------------

    #[test]
    fn single_char_tokens() {
        // pylatexenc parity: one token per content character; whitespace between tokens
        // becomes the next token's pre_space.
        let mut tr = StdTokenReader::new("ab c");
        let st = state(latex_rules());

        assert_eq!(next(&mut tr, &st), char_token('a', 0, Span::empty(0)));
        assert_eq!(next(&mut tr, &st), char_token('b', 1, Span::empty(1)));
        assert_eq!(next(&mut tr, &st), char_token('c', 3, sp(2, 3)));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::EndOfStream);
    }

    #[test]
    fn char_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}Some", pre_space);
        let mut tr = StdTokenReader::new(&text);
        assert_eq!(peek(&mut tr, &state(latex_rules())), char_token('S', 7, sp(0, 7)));
    }

    #[test]
    fn peek_does_not_advance() {
        let mut tr = StdTokenReader::new("abc");
        let st = state(latex_rules());
        let first = peek(&mut tr, &st);
        assert_eq!(peek(&mut tr, &st), first);
        assert_eq!(tr.pos(), 0);
    }

    #[test]
    fn char_multibyte() {
        let text = "héllo→";
        let mut tr = StdTokenReader::new(text);
        let st = state(latex_rules());
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('h'));
        let token = next(&mut tr, &st);
        assert_eq!(token.kind, TokenKind::Char('é'));
        assert_eq!(token.span.slice(text), "é");
        assert_eq!(tr.pos(), 1 + 'é'.len_utf8());
    }

    // --- commands -----------------------------------------------------------------------

    #[test]
    fn command_with_post_space() {
        let text = r"\somemacro and more stuff";
        let mut tr = StdTokenReader::new(text);

        // span includes the post_space (pylatexenc: pos_end past post_space).
        assert_eq!(
            peek(&mut tr, &state(latex_rules())),
            Token::new(
                TokenKind::Command { name: "somemacro", escape_char: '\\', post_space: sp(10, 11) },
                sp(0, 11),
                Span::empty(0),
            ),
        );
    }

    #[test]
    fn command_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}\\somemacro and more stuff", pre_space);
        let mut tr = StdTokenReader::new(&text);

        assert_eq!(
            peek(&mut tr, &state(latex_rules())),
            Token::new(
                TokenKind::Command { name: "somemacro", escape_char: '\\', post_space: sp(17, 18) },
                sp(7, 18),
                sp(0, 7),
            ),
        );
    }

    #[test]
    fn single_char_command_takes_no_post_space() {
        let mut tr = StdTokenReader::new(r"\& also");
        assert_eq!(
            peek(&mut tr, &state(latex_rules())),
            Token::new(
                TokenKind::Command { name: "&", escape_char: '\\', post_space: Span::empty(2) },
                sp(0, 2),
                Span::empty(0),
            ),
        );
    }

    #[test]
    fn accent_command_then_char() {
        let mut tr = StdTokenReader::new(r"\`accent");
        let st = state(latex_rules());
        let token = next(&mut tr, &st);
        assert_eq!(token.kind, TokenKind::Command { name: "`", escape_char: '\\', post_space: Span::empty(2) });
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
    }

    #[test]
    fn command_post_space_stops_before_paragraph_break() {
        // Whitespace after the command starts with the break sequence: no post_space at
        // all (pylatexenc: "put back whitespace that breaks into a new paragraph").
        let mut tr = StdTokenReader::new("\\macroname\n  \n ");
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "macroname", escape_char: '\\', post_space: Span::empty(10) },
                sp(0, 10),
                Span::empty(0),
            ),
        );
        // ... and the paragraph break follows as its own token.
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::ParagraphBreak, sp(10, 14), Span::empty(10)),
        );
    }

    #[test]
    fn command_post_space_kept_up_to_paragraph_break() {
        let mut tr = StdTokenReader::new("\\macroname   \n  \n ");
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "macroname", escape_char: '\\', post_space: sp(10, 13) },
                sp(0, 13),
                Span::empty(0),
            ),
        );
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::ParagraphBreak, sp(13, 17), Span::empty(13)),
        );
    }

    #[test]
    fn command_custom_name_chars() {
        let text = r"\zzz1234567890-haha_works! is a macro here";
        let mut tr = StdTokenReader::new(text);
        let st = state(TokenRules {
            commands: vec![CommandRule {
                escape_char: '\\',
                name_chars: "0123456789abcdefghijklmnopqrstuvwxyz\
                             ABCDEFGHIJKLMNOPQRSTUVWXYZ_+!-"
                    .into(),
            }],
            ..latex_rules()
        });

        let name = "zzz1234567890-haha_works!";
        assert_eq!(
            peek(&mut tr, &st),
            Token::new(
                TokenKind::Command {
                    name,
                    escape_char: '\\',
                    post_space: sp(1 + name.len(), 1 + name.len() + 1),
                },
                sp(0, 1 + name.len() + 1),
                Span::empty(0),
            ),
        );
    }

    #[test]
    fn command_records_the_fired_rules_escape_char() {
        // Two coexisting command syntaxes: each token records which rule's escape
        // character fired (parse-time lookup disambiguates by it — DESIGN_RATIONALE §3.2).
        let names = "abcdefghijklmnopqrstuvwxyz";
        let st = state(TokenRules {
            commands: vec![
                CommandRule { escape_char: '\\', name_chars: names.into() },
                CommandRule { escape_char: '@', name_chars: names.into() },
            ],
            ..latex_rules()
        });
        let mut tr = StdTokenReader::new("\\foo @bar");
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "foo", escape_char: '\\', post_space: sp(4, 5) },
                sp(0, 5),
                Span::empty(0),
            ),
        );
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "bar", escape_char: '@', post_space: Span::empty(9) },
                sp(5, 9),
                Span::empty(5),
            ),
        );
    }

    #[test]
    fn commands_disabled_escape_is_plain_content() {
        let mut tr = StdTokenReader::new(r"\foo");
        let st = state(TokenRules { commands: Vec::new(), ..latex_rules() });
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('\\'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('f'));
    }

    #[test]
    fn enable_commands_off_is_the_scoped_disable() {
        // The gate variant of the test above: the CommandRules stay in the data (a later
        // enable_commands: Some(true) delta restores recognition without carrying them).
        let mut tr = StdTokenReader::new(r"\foo");
        let st = state(TokenRules { enable_commands: false, ..latex_rules() });
        assert!(!st.rules().commands.is_empty());
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('\\'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('f'));
    }

    // --- \begin is NOT special at the token level ---------------------------------------

    #[test]
    fn begin_environment_is_ordinary_tokens() {
        // \begin{equation} tokenizes as command + group open + chars (+ group close):
        // "as far as token parsing is concerned, \begin is a command just like \foobar".
        // Environment recognition is entirely a parse-time (preset) concern.
        let mut tr = StdTokenReader::new(r"\begin{equation}");
        let st = state(latex_rules());

        assert_eq!(
            next(&mut tr, &st),
            // No post_space: '{' follows the name directly.
            Token::new(
                TokenKind::Command { name: "begin", escape_char: '\\', post_space: Span::empty(6) },
                sp(0, 6),
                Span::empty(0),
            ),
        );
        assert_eq!(
            next(&mut tr, &st).kind,
            TokenKind::GroupOpen { delim: "{", rule: rule_of(BRACES) },
        );
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('e'));
    }

    #[test]
    fn begin_prefixed_command_name_reads_fully() {
        let mut tr = StdTokenReader::new(r"\beginMacroWithConfusingName");
        let token = peek(&mut tr, &state(latex_rules()));
        match token.kind {
            TokenKind::Command { name, .. } => assert_eq!(name, "beginMacroWithConfusingName"),
            other => panic!("expected command, got {:?}", other),
        }
    }

    // --- groups -----------------------------------------------------------------------

    #[test]
    fn group_open_and_close() {
        let st = state(latex_rules());

        let mut tr = StdTokenReader::new("{begin group here");
        assert_eq!(
            peek(&mut tr, &st),
            Token::new(
                TokenKind::GroupOpen { delim: "{", rule: rule_of(BRACES) },
                sp(0, 1),
                Span::empty(0),
            ),
        );

        let mut tr = StdTokenReader::new("} a braced group just ended here");
        assert_eq!(
            peek(&mut tr, &st),
            Token::new(
                TokenKind::GroupClose { delim: "}" },
                sp(0, 1),
                Span::empty(0),
            ),
        );
    }

    #[test]
    fn group_delimiters_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}{{begin group here", pre_space);
        let mut tr = StdTokenReader::new(&text);
        assert_eq!(
            peek(&mut tr, &state(latex_rules())),
            Token::new(
                TokenKind::GroupOpen { delim: "{", rule: rule_of(BRACES) },
                sp(7, 8),
                sp(0, 7),
            ),
        );
    }

    #[test]
    fn optional_argument_brackets_are_group_tokens() {
        let mut tr = StdTokenReader::new("[(i)]");
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st).kind,
            TokenKind::GroupOpen { delim: "[", rule: rule_of(BRACKETS) },
        );
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('('));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('i'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char(')'));
        assert_eq!(
            next(&mut tr, &st).kind,
            TokenKind::GroupClose { delim: "]" },
        );
    }

    // --- math-style ambiguous group delimiters ------------------------------------------

    #[test]
    fn ambiguous_delimiter_reads_as_open_by_default() {
        let mut tr = StdTokenReader::new("$x$");
        assert_eq!(
            peek(&mut tr, &state(latex_rules())).kind,
            TokenKind::GroupOpen { delim: "$", rule: rule_of(MATH_INLINE) },
        );
    }

    #[test]
    fn expected_close_wins_over_open_interpretation() {
        let mut tr = StdTokenReader::new("$ and more");
        assert_eq!(
            peek(&mut tr, &expecting_close(MATH_INLINE)).kind,
            TokenKind::GroupClose { delim: "$" },
        );
    }

    #[test]
    fn close_only_delimiter_reads_as_close_even_unexpected() {
        // "report closing '\)' also with incorrect parsing state -- it's not the
        // tokenizer's job to report syntax errors" (pylatexenc test suite).
        let mut tr = StdTokenReader::new(r"\) rest");
        assert_eq!(
            peek(&mut tr, &state(latex_rules())).kind,
            TokenKind::GroupClose { delim: r"\)" },
        );
    }

    #[test]
    fn escape_led_delimiters_win_over_command_interpretation() {
        let st = state(latex_rules());

        let mut tr = StdTokenReader::new(r" \(x\)");
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::GroupOpen { delim: r"\(", rule: rule_of(MATH_INLINE_PAREN) },
                sp(1, 3),
                sp(0, 1),
            ),
        );

        let mut tr = StdTokenReader::new("\n\\[ cx^2 \\]");
        assert_eq!(
            peek(&mut tr, &st),
            Token::new(
                TokenKind::GroupOpen { delim: r"\[", rule: rule_of(MATH_DISPLAY_BRACKET) },
                sp(1, 3),
                sp(0, 1),
            ),
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

        let cases: [(usize, &ParsingState<TestLang>, TokenKind<'_, TestLang>, usize); 8] = [
            // (pos, state, expected kind, expected end)
            (1, &plain, TokenKind::GroupOpen { delim: "$", rule: rule_of(MATH_INLINE) }, 2),
            // expected close beats the longest ('$$') match:
            (9, &in_inline, TokenKind::GroupClose { delim: "$" }, 10),
            (10, &plain, TokenKind::GroupOpen { delim: "$", rule: rule_of(MATH_INLINE) }, 11),
            (18, &in_inline, TokenKind::GroupClose { delim: "$" }, 19),
            // not expecting a close: longest match wins -> display '$$':
            (19, &plain, TokenKind::GroupOpen { delim: "$$", rule: rule_of(MATH_DISPLAY) }, 21),
            (30, &plain, TokenKind::GroupOpen { delim: "$", rule: rule_of(MATH_INLINE) }, 31),
            (34, &in_inline, TokenKind::GroupClose { delim: "$" }, 35),
            (36, &in_display, TokenKind::GroupClose { delim: "$$" }, 38),
        ];

        for (pos, st, kind, end) in cases {
            let mut tr = StdTokenReader::new(text);
            tr.move_to_pos(pos);
            let token = peek(&mut tr, st);
            assert_eq!(token.kind, kind, "at pos {}", pos);
            assert_eq!(token.span, sp(pos, end), "at pos {}", pos);
        }
    }

    // --- comments (whole-comment tokens) -------------------------------------------------

    #[test]
    fn comment_token_covers_content_and_post_space() {
        // "% Comment here\n  more": content sans delimiter and newline; post_space =
        // newline + indentation (syntactic whitespace, consumed by the comment).
        let mut tr = StdTokenReader::new("% Comment here\n  more stuff");
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Comment { content: " Comment here", post_space: sp(14, 17) },
                sp(0, 17),
                Span::empty(0),
            ),
        );
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('m'));
    }

    #[test]
    fn comment_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}% Comment here\n  more stuff", pre_space);
        let mut tr = StdTokenReader::new(&text);
        let token = peek(&mut tr, &state(latex_rules()));
        assert_eq!(token.pre_space, sp(0, 7));
        assert_eq!(token.span, sp(7, 24));
        assert_eq!(
            token.kind,
            TokenKind::Comment { content: " Comment here", post_space: sp(21, 24) },
        );
    }

    #[test]
    fn comment_before_paragraph_break_takes_no_post_space() {
        // "a% c\n\nb": the comment's terminating newline belongs to a \n\s*\n sequence,
        // so the comment takes no post-space and the paragraph break survives as its own
        // token (TeX-wise: the blank line still yields \par).
        let mut tr = StdTokenReader::new("a% c\n\nb");
        let st = state(latex_rules());
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Comment { content: " c", post_space: Span::empty(4) },
                sp(1, 4),
                Span::empty(1),
            ),
        );
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::ParagraphBreak, sp(4, 6), Span::empty(4)),
        );
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('b'));
    }

    #[test]
    fn comment_at_end_of_input_without_newline() {
        let mut tr = StdTokenReader::new("x% trailing");
        let st = state(latex_rules());
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('x'));
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Comment { content: " trailing", post_space: Span::empty(11) },
                sp(1, 11),
                Span::empty(1),
            ),
        );
        assert_eq!(next(&mut tr, &st).kind, TokenKind::EndOfStream);
    }

    #[test]
    fn comment_alternative_start_string_longest_wins() {
        let text = "%!!COMMENT!! Comment here\nmore";
        let mut tr = StdTokenReader::new(text);
        let st = state(TokenRules {
            comments: vec![
                CommentRule { start: "%".into() },
                CommentRule { start: "%!!COMMENT!!".into() },
            ],
            ..latex_rules()
        });
        assert_eq!(
            peek(&mut tr, &st),
            Token::new(
                TokenKind::Comment { content: " Comment here", post_space: sp(25, 26) },
                sp(0, 26),
                Span::empty(0),
            ),
        );
    }

    #[test]
    fn comments_disabled_percent_is_plain_content() {
        let mut tr = StdTokenReader::new("a %b");
        let st = state(TokenRules { comments: Vec::new(), ..latex_rules() });
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        assert_eq!(next(&mut tr, &st), char_token('%', 2, sp(1, 2)));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('b'));
    }

    #[test]
    fn enable_comments_off_is_the_scoped_disable() {
        let mut tr = StdTokenReader::new("a %b");
        let st = state(TokenRules { enable_comments: false, ..latex_rules() });
        assert!(!st.rules().comments.is_empty());
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        assert_eq!(next(&mut tr, &st), char_token('%', 2, sp(1, 2)));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('b'));
    }

    // --- paragraph breaks ---------------------------------------------------------------

    #[test]
    fn paragraph_break_token() {
        // "Abc    \n\n  z": break spans first..last newline; leading run is pre_space,
        // whitespace after the last newline is left for the next token.
        let mut tr = StdTokenReader::new("Abc    \n\n  z");
        let st = state(latex_rules());
        tr.move_to_pos(3);

        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::ParagraphBreak, sp(7, 9), sp(3, 7)),
        );
        assert_eq!(next(&mut tr, &st), char_token('z', 11, sp(9, 11)));
    }

    #[test]
    fn paragraph_break_with_inner_whitespace() {
        let mut tr = StdTokenReader::new("Abc  \t \n   \t\nz");
        let st = state(latex_rules());
        tr.move_to_pos(3);
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::ParagraphBreak, sp(7, 13), sp(3, 7)),
        );
    }

    #[test]
    fn paragraph_break_in_trailing_whitespace_still_emitted() {
        let mut tr = StdTokenReader::new("x  \n\n  ");
        let st = state(latex_rules());
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('x'));
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::ParagraphBreak, sp(3, 5), sp(1, 3)),
        );
        // The whitespace after the break's last newline is the end-of-stream token's
        // pre_space — nothing is lost.
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::EndOfStream, Span::empty(7), sp(5, 7)),
        );
    }

    #[test]
    fn paragraph_breaks_disabled() {
        let mut tr = StdTokenReader::new("Abc\n\nNew");
        let st = state(TokenRules { enable_multi_newline_paragraphs: false, ..latex_rules() });
        tr.move_to_pos(2);
        assert_eq!(next(&mut tr, &st), char_token('c', 2, Span::empty(2)));
        // The double newline is ordinary consumable whitespace now.
        assert_eq!(next(&mut tr, &st), char_token('N', 5, sp(3, 5)));
    }

    // --- specials (via the Lang scan hook) ------------------------------------------------

    #[derive(Debug)]
    struct StubSpec;
    impl CallableSpec<SpecialsLang> for StubSpec {}

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SpecialsLang;
    impl Lang for SpecialsLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~&-".into())
        }

        fn scan_specials<'s>(
            _state: &ParsingState<Self>,
            content: &'s str,
            pos: usize,
        ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
            // Longest-first over a hardcoded trigger list — a stand-in for the preset
            // dispatching to its libraries (Phase 4+).
            for trigger in ["---", "~~", "~", "&"] {
                if content[pos..].starts_with(trigger) {
                    return Ok(Some(SpecialsMatch {
                        end: pos + trigger.len(),
                        callable_type: 7,
                        name: &content[pos..pos + trigger.len()],
                        spec: Arc::new(StubSpec),
                    }));
                }
            }
            Ok(None)
        }
    }

    fn specials_state(rules: TokenRules<SpecialsLang>) -> ParsingState<SpecialsLang> {
        ParsingState::new(StateData { rules, libraries: LibraryStack::new(), ext: () })
    }

    #[test]
    fn specials_recognized_with_spec_attached() {
        let mut tr = StdTokenReader::new("a&b");
        let st = specials_state(latex_rules());

        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('a'));
        let token = TokenReader::next(&mut tr, &st).unwrap();
        match &token.kind {
            TokenKind::Specials { callable_type, name, spec } => {
                assert_eq!(*callable_type, 7);
                assert_eq!(*name, "&");
                assert_eq!(format!("{:?}", spec), "StubSpec");
            }
            other => panic!("expected specials, got {:?}", other),
        }
        assert_eq!(token.span, sp(1, 2));
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('b'));
    }

    #[test]
    fn specials_longest_match_is_the_scanners_business() {
        let mut tr = StdTokenReader::new("---x");
        let st = specials_state(latex_rules());
        let token = TokenReader::peek(&mut tr, &st).unwrap();
        match &token.kind {
            TokenKind::Specials { name, .. } => assert_eq!(*name, "---"),
            other => panic!("expected specials, got {:?}", other),
        }
        assert_eq!(token.span, sp(0, 3));
    }

    #[test]
    fn specials_scan_miss_falls_through_to_char() {
        // '-' is a trigger char, but a lone '-' matches no trigger: plain content.
        let mut tr = StdTokenReader::new("-x");
        let st = specials_state(latex_rules());
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('-'));
    }

    #[test]
    fn enable_specials_off_freezes_the_empty_filter() {
        // The gate is baked at freeze: the state stores the empty TriggerChars, so the
        // scan hook is unreachable and triggers read as plain content — this is what
        // makes "no specials here" delta-expressible (DESIGN_RATIONALE §3.2, ex-§6.6).
        let mut tr = StdTokenReader::new("a&b");
        let st = specials_state(TokenRules { enable_specials: false, ..latex_rules() });
        assert_eq!(st.trigger_chars(), &TriggerChars::default());
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('a'));
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('&'));
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('b'));
    }

    #[test]
    fn scan_hook_not_consulted_outside_trigger_chars() {
        #[derive(Debug, Clone, Copy)]
        struct PanickyLang;
        impl Lang for PanickyLang {
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ();
            fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
                TriggerChars::Only("~".into())
            }
            fn scan_specials<'s>(
                _state: &ParsingState<Self>,
                _content: &'s str,
                _pos: usize,
            ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
                panic!("scan_specials consulted for a non-trigger character");
            }
        }
        let st: ParsingState<PanickyLang> =
            ParsingState::new(StateData { rules: latex_rules(), libraries: LibraryStack::new(), ext: () });
        let mut tr = StdTokenReader::new("x");
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('x'));
    }

    // --- errors and recovery -------------------------------------------------------------

    #[test]
    fn forbidden_char_error_with_recovery() {
        let mut tr = StdTokenReader::new("% forbidden here");
        let st = state(TokenRules {
            comments: Vec::new(),
            forbidden_chars: "%$".into(),
            ..latex_rules()
        });

        let err = TokenReader::peek(&mut tr, &st).unwrap_err();
        assert!(matches!(
            err.kind(),
            TokenErrorKind::ForbiddenChar(ForbiddenChar { ch: '%' })
        ));
        assert_eq!(err.span(), sp(0, 1));

        // Tolerant continuation: use the recovery token, resume past it.
        let recovery = err.into_recovery().unwrap();
        assert_eq!(recovery.token, char_token('%', 0, Span::empty(0)));
        assert_eq!(recovery.resume_pos, 1);
        tr.move_to_pos(recovery.resume_pos);
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('f'));
    }

    #[test]
    fn end_of_stream_after_escape_error_with_recovery() {
        let mut tr = StdTokenReader::new(r"a \");
        let st = state(latex_rules());

        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));

        let err = TokenReader::peek(&mut tr, &st).unwrap_err();
        assert!(matches!(
            err.kind(),
            TokenErrorKind::EndOfStreamAfterEscape(EndOfStreamAfterEscape {
                escape_char: '\\',
            })
        ));
        assert_eq!(err.span(), sp(2, 3));

        // Recovery: pretend the stream ended at the dangling escape, resume at the end.
        let recovery = err.into_recovery().unwrap();
        assert_eq!(
            recovery.token,
            Token::new(TokenKind::EndOfStream, Span::empty(2), sp(1, 2)),
        );
        assert_eq!(recovery.resume_pos, 3);
        tr.move_to_pos(recovery.resume_pos);
        assert_eq!(peek(&mut tr, &st).kind, TokenKind::EndOfStream);
    }

    // --- end of stream --------------------------------------------------------------------

    #[test]
    fn end_of_stream_token_is_terminal_and_idempotent() {
        let st = state(latex_rules());

        let mut tr = StdTokenReader::new("");
        assert_eq!(
            peek(&mut tr, &st),
            Token::new(TokenKind::EndOfStream, Span::empty(0), Span::empty(0)),
        );

        // Trailing whitespace (no paragraph break) is the end-of-stream token's
        // pre_space — reported, so it can land in the node tree.
        let mut tr = StdTokenReader::new("x   ");
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('x'));
        let eos = Token::new(TokenKind::EndOfStream, Span::empty(4), sp(1, 4));
        assert_eq!(next(&mut tr, &st), eos);
        assert_eq!(tr.pos(), 4);
        assert!(tr.is_at_end());
        // Terminal: further reads yield end-of-stream again (now with empty pre_space,
        // the earlier trailing whitespace having been consumed).
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::EndOfStream, Span::empty(4), Span::empty(4)),
        );
    }

    // --- whitespace handling disabled ------------------------------------------------------

    #[test]
    fn whitespace_disabled_gives_character_level_content() {
        let mut tr = StdTokenReader::new("a b{");
        let st = state(TokenRules { enable_whitespace: false, ..latex_rules() });
        assert_eq!(next(&mut tr, &st), char_token('a', 0, Span::empty(0)));
        assert_eq!(next(&mut tr, &st), char_token(' ', 1, Span::empty(1)));
        assert_eq!(next(&mut tr, &st), char_token('b', 2, Span::empty(2)));
        assert_eq!(
            next(&mut tr, &st).kind,
            TokenKind::GroupOpen { delim: "{", rule: rule_of(BRACES) },
        );
    }

    // --- enable_groups (the gate is baked into the prefix table) ---------------------------

    #[test]
    fn enable_groups_off_delimiters_are_plain_content() {
        let mut tr = StdTokenReader::new("{a}");
        let st = state(TokenRules { enable_groups: false, ..latex_rules() });
        assert!(!st.rules().groups.is_empty());
        assert!(st.prefix_table().match_at("{a}").is_none()); // baked-in empty table
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('{'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('}'));
    }

    #[test]
    fn enable_groups_off_does_not_gate_the_expected_close() {
        // The decided interaction (DESIGN_RATIONALE §3.2): expecting_group_close is
        // positional data, not a feature — a group interior that disables groups
        // entirely still finds its own close, so the entered group always terminates.
        let mut tr = StdTokenReader::new("a{$");
        let st = state(TokenRules {
            enable_groups: false,
            expecting_group_close: Some(rule_of(MATH_INLINE)),
            ..latex_rules()
        });
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        // The table is off: `{` is plain content …
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('{'));
        // … but the expected `$` close still tokenizes as GroupClose.
        assert_eq!(next(&mut tr, &st).kind, TokenKind::GroupClose { delim: "$" });
    }

    // --- movement -----------------------------------------------------------------------

    #[test]
    fn move_past_and_move_to_flags() {
        let text = "  \\vec b";
        let mut tr = StdTokenReader::new(text);
        let st = state(latex_rules());

        let token = peek(&mut tr, &st);
        assert_eq!(token.kind, TokenKind::Command { name: "vec", escape_char: '\\', post_space: sp(6, 7) });
        assert_eq!(token.span, sp(2, 7)); // includes post_space
        assert_eq!(token.pre_space, sp(0, 2));
        assert_eq!(token.post_space(), sp(6, 7));

        TokenReader::move_past(&mut tr, &token, true);
        assert_eq!(tr.pos(), 7); // past post_space
        TokenReader::move_past(&mut tr, &token, false);
        assert_eq!(tr.pos(), 6); // before post_space (e.g. for \verb-style parsers)

        TokenReader::move_to(&mut tr, &token, false);
        assert_eq!(tr.pos(), 2); // at the token
        TokenReader::move_to(&mut tr, &token, true);
        assert_eq!(tr.pos(), 0); // before pre_space

        // Reading again after move_to yields the same token.
        assert_eq!(peek(&mut tr, &st), token);
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
        let mut tr = StdTokenReader::new(text);

        let find = |needle: &str| text.find(needle).unwrap();

        assert_eq!(next(&mut tr, &st), char_token('T', 0, Span::empty(0)));

        let p = find(r"\`");
        tr.move_to_pos(p);
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "`", escape_char: '\\', post_space: Span::empty(p + 2) },
                sp(p, p + 2),
                Span::empty(p),
            ),
        );

        let p = find(r"\textbf") - 1; // pre space
        tr.move_to_pos(p);
        assert_eq!(
            next(&mut tr, &st),
            // '{' follows the name: no post_space.
            Token::new(
                TokenKind::Command { name: "textbf", escape_char: '\\', post_space: Span::empty(p + 8) },
                sp(p + 1, p + 8),
                sp(p, p + 1),
            ),
        );
        assert_eq!(
            next(&mut tr, &st).kind,
            TokenKind::GroupOpen { delim: "{", rule: rule_of(BRACES) },
        );

        let p = find(r"\vec"); // post-space present
        tr.move_to_pos(p);
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "vec", escape_char: '\\', post_space: sp(p + 4, p + 5) },
                sp(p, p + 5),
                Span::empty(p),
            ),
        );

        let p = find(r"\&") - 1; // pre-space and *no* post-space
        tr.move_to_pos(p);
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "&", escape_char: '\\', post_space: Span::empty(p + 3) },
                sp(p + 1, p + 3),
                sp(p, p + 1),
            ),
        );

        // \begin is just a command token; the environment name is ordinary group+chars.
        let p = find(r"\begin");
        tr.move_to_pos(p);
        let token = next(&mut tr, &st);
        assert_eq!(
            token.kind,
            TokenKind::Command { name: "begin", escape_char: '\\', post_space: Span::empty(p + 6) },
        );

        let p = find("@@@") + 3; // pre space up to \end
        tr.move_to_pos(p);
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "end", escape_char: '\\', post_space: Span::empty(p + 10) },
                sp(p + 6, p + 10),
                sp(p, p + 6),
            ),
        );

        // The whole comment is one token: content to end of line, newline as post_space.
        let p = find("%") - 2;
        tr.move_to_pos(p);
        let nl = find(" % here goes a comment") + " % here goes a comment".len();
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Comment {
                    content: " here goes a comment",
                    post_space: sp(nl, nl + 1),
                },
                sp(p + 2, nl + 1),
                sp(p, p + 2),
            ),
        );

        // \mymacro directly precedes a paragraph break: no post_space.
        let p = find(r"\mymacro");
        tr.move_to_pos(p);
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Command { name: "mymacro", escape_char: '\\', post_space: Span::empty(p + 8) },
                sp(p, p + 8),
                Span::empty(p),
            ),
        );
        let p2 = find("\n\n");
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::ParagraphBreak, sp(p2, p2 + 2), Span::empty(p2)),
        );

        // ... and the file's trailing newline is the end-of-stream token's pre_space.
        let p = find("New paragraph");
        tr.move_to_pos(p + "New paragraph".len());
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::EndOfStream,
                Span::empty(text.len()),
                sp(text.len() - 1, text.len()),
            ),
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
        let no_par: TokenRules<TestLang> =
            TokenRules { enable_multi_newline_paragraphs: false, ..latex_rules() };
        assert_eq!(skip_whitespace("   \n  \n x", 0, &no_par), 8);
        // Whitespace handling disabled: nothing is skipped.
        let no_ws: TokenRules<TestLang> =
            TokenRules { enable_whitespace: false, ..latex_rules() };
        assert_eq!(skip_whitespace("  x", 0, &no_ws), 0);
    }
}
