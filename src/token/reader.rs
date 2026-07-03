//! The standard token reader, driven entirely by [`TokenRules`] data.
//!
//! `StdTokenReader` follows pylatexenc's proven `LatexTokenReader` protocol: `peek` parses
//! a token at the current position without advancing; `move_past`/`move_to` reposition
//! relative to a token; `next` = peek + move-past. The scanning core is decomposed into
//! private `detect_*` methods (salvaged from the WIP design), each driven by one facet of
//! the rules.
//!
//! The `TokenReader<'s, L>` *trait* — the behavior extension point, whose `peek`
//! deliberately receives the full `ParsingState<L>` (ARCHITECTURE.md §token, stratum
//! split) — arrives in Phase 3 together with `ParsingState<L>` itself. `StdTokenReader`'s
//! inherent API is shaped to match: `peek(&mut self, rules, table)` becomes the trait's
//! `peek(&mut self, state)` with rules and cached table read from the state.

use super::error::{TokenError, TokenErrorKind, TokenRecovery, TokenResult};
use super::prefix_table::PrefixTable;
use super::rules::{MacroRules, TokenRules};
use super::span::Span;
use super::token::{Token, TokenKind};

/// Standard tokenizer over in-memory content, 100% driven by [`TokenRules`] data.
///
/// The reader holds only the content borrow and a position; all tokenization behavior
/// comes from the rules (and their derived [`PrefixTable`]) passed to [`peek`](Self::peek)
/// — which is what lets the rules change mid-parse through state transitions.
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

    /// Whether the reader is at the end of the content. Note that trailing whitespace
    /// still counts as remaining content even though it yields no further token.
    pub fn is_at_end(&self) -> bool {
        self.pos >= self.content.len()
    }

    /// Move to an absolute byte position (must lie on a `char` boundary).
    pub fn move_to_pos(&mut self, pos: usize) {
        debug_assert!(pos <= self.content.len(), "position {} beyond content end", pos);
        debug_assert!(self.content.is_char_boundary(pos), "position {} not on char boundary", pos);
        self.pos = pos;
    }

    /// Move immediately past `tok`. If `skip_post_space` is true the position lands after
    /// the token's post-space; otherwise right after the token proper, before it.
    pub fn move_past(&mut self, tok: &Token<'s>, skip_post_space: bool) {
        if skip_post_space {
            self.pos = tok.span.end;
        } else {
            self.pos = tok.span.end - tok.post_space.len();
        }
    }

    /// Move to `tok`'s own start, so that it would be read again. If `rewind_pre_space` is
    /// true the position lands before the token's preceding whitespace instead.
    pub fn move_to(&mut self, tok: &Token<'s>, rewind_pre_space: bool) {
        if rewind_pre_space {
            self.pos = tok.pre_space.start;
        } else {
            self.pos = tok.span.start;
        }
    }

    /// Parse the token at the current position without advancing.
    ///
    /// Returns `Ok(None)` at the end of the token stream — i.e. when nothing but
    /// whitespace (with no paragraph break) remains; such trailing whitespace is not
    /// covered by any token and remains recoverable from the content and [`pos`](Self::pos).
    /// Subsequent calls with the same rules return the same result.
    ///
    /// `table` must be the [`PrefixTable`] derived from `rules` (the parsing state caches
    /// it per instance from Phase 3 on).
    pub fn peek(
        &mut self,
        rules: &TokenRules,
        table: &PrefixTable,
    ) -> TokenResult<'s, Option<Token<'s>>> {
        let s = self.content;
        let start = self.pos;

        let ws_end = self.detect_whitespace(start, rules);
        let pre_space = Span::new(start, ws_end);

        // A paragraph break hiding in the whitespace run trumps everything, including
        // end-of-stream (trailing "…\n\n" still yields the break).
        if let Some(token) = self.detect_paragraph_break(pre_space, rules) {
            return Ok(Some(token));
        }

        let pos = ws_end;
        if pos >= s.len() {
            return Ok(None);
        }

        // Group delimiters come before macros so that escape-char-led delimiters like
        // `\(` win over macro interpretation (as in pylatexenc, where math delimiters
        // are checked first).
        if let Some(token) = self.detect_group_delimiter(pos, pre_space, rules, table) {
            return Ok(Some(token));
        }

        if let Some(macros) = &rules.macros {
            let c = s[pos..].chars().next().expect("pos < len checked above");
            if c == macros.escape_char {
                return self.read_macro(pos, pre_space, rules, macros).map(Some);
            }
        }

        if let Some(token) = self.detect_comment_start(pos, pre_space, rules) {
            return Ok(Some(token));
        }

        if let Some(token) = self.detect_specials(pos, pre_space, rules) {
            return Ok(Some(token));
        }

        self.read_chars(pos, pre_space, rules, table).map(Some)
    }

    /// Parse the token at the current position and move past it (including its
    /// post-space): [`peek`](Self::peek) + [`move_past`](Self::move_past).
    pub fn next(
        &mut self,
        rules: &TokenRules,
        table: &PrefixTable,
    ) -> TokenResult<'s, Option<Token<'s>>> {
        match self.peek(rules, table)? {
            None => Ok(None),
            Some(token) => {
                self.move_past(&token, true);
                Ok(Some(token))
            }
        }
    }

    // --- detect_* scanning core -------------------------------------------------------

    /// End position of the whitespace run starting at `pos` (= `pos` if none, or if
    /// whitespace handling is disabled).
    fn detect_whitespace(&self, pos: usize, rules: &TokenRules) -> usize {
        let Some(ws) = &rules.whitespace else {
            return pos;
        };
        let mut end = pos;
        for c in self.content[pos..].chars() {
            if !ws.chars.contains(c) {
                break;
            }
            end += c.len_utf8();
        }
        end
    }

    /// A `ParagraphBreak` token if the whitespace run `ws_span` contains two or more
    /// newlines (and paragraph breaks are enabled). The token spans from the first through
    /// the last newline; leading whitespace becomes its `pre_space`, trailing whitespace
    /// after the last newline is left for the next token.
    fn detect_paragraph_break(&self, ws_span: Span, rules: &TokenRules) -> Option<Token<'s>> {
        if !rules.paragraph_breaks {
            return None;
        }
        let ws = ws_span.slice(self.content);
        if ws.matches('\n').count() < 2 {
            return None;
        }
        let first_nl = ws_span.start + ws.find('\n').expect("counted above");
        let last_nl_end = ws_span.start + ws.rfind('\n').expect("counted above") + 1;
        Some(Token::new(
            TokenKind::ParagraphBreak,
            Span::new(first_nl, last_nl_end),
            Span::new(ws_span.start, first_nl),
        ))
    }

    /// A `GroupOpen`/`GroupClose` token at `pos`, if a delimiter matches. The close
    /// delimiter expected per `rules.expecting_group_close` takes precedence; otherwise
    /// the longest table match wins, read as an opener when the string is ambiguous.
    fn detect_group_delimiter(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules,
        table: &PrefixTable,
    ) -> Option<Token<'s>> {
        let rest = &self.content[pos..];

        if let Some(expected_id) = rules.expecting_group_close {
            if let Some(group_type) = rules.group_types.iter().find(|g| g.id == expected_id) {
                if !group_type.close.is_empty() && rest.starts_with(group_type.close.as_str()) {
                    let span = Span::new(pos, pos + group_type.close.len());
                    return Some(Token::new(
                        TokenKind::GroupClose {
                            delim: span.slice(self.content),
                            group_type: expected_id,
                        },
                        span,
                        pre_space,
                    ));
                }
            }
        }

        let entry = table.match_at(rest)?;
        let span = Span::new(pos, pos + entry.delim().len());
        let delim = span.slice(self.content);
        let kind = match (entry.open(), entry.close()) {
            (Some(group_type), _) => TokenKind::GroupOpen { delim, group_type },
            (None, Some(group_type)) => TokenKind::GroupClose { delim, group_type },
            (None, None) => unreachable!("prefix table entries always carry a direction"),
        };
        Some(Token::new(kind, span, pre_space))
    }

    /// Read a macro token at `pos` (the escape character's position). The name is a greedy
    /// run of name characters, or a single character if the first one is not a name
    /// character. Multi-character names consume their following whitespace as `post_space`,
    /// stopping short of a paragraph break.
    fn read_macro(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules,
        macros: &MacroRules,
    ) -> TokenResult<'s, Token<'s>> {
        let s = self.content;
        let name_start = pos + macros.escape_char.len_utf8();

        if name_start >= s.len() {
            // Recovery: pretend an empty chars token was read, resume at end of input.
            let placeholder =
                Token::new(TokenKind::Chars(""), Span::empty(pos), pre_space);
            return Err(TokenError::new(
                TokenErrorKind::EndOfStreamAfterEscape { escape_char: macros.escape_char },
                Span::new(pos, name_start),
                Some(TokenRecovery { token: placeholder, resume_pos: s.len() }),
            ));
        }

        let first = s[name_start..].chars().next().expect("name_start < len checked above");
        let mut name_end = name_start + first.len_utf8();
        let is_named = macros.name_chars.contains(first);
        if is_named {
            for c in s[name_end..].chars() {
                if !macros.name_chars.contains(c) {
                    break;
                }
                name_end += c.len_utf8();
            }
        }

        // Only multi-character (name-chars) macros swallow their post-space; `\&` and
        // friends do not (pylatexenc behavior). Post-space never crosses a paragraph
        // break: if the whitespace run holds 2+ newlines, keep only up to the first.
        let mut post_space = Span::empty(name_end);
        if is_named {
            let ws_end = self.detect_whitespace(name_end, rules);
            let ws = &s[name_end..ws_end];
            let end = match () {
                _ if rules.paragraph_breaks && ws.matches('\n').count() >= 2 => {
                    name_end + ws.find('\n').expect("counted above")
                }
                _ => ws_end,
            };
            post_space = Span::new(name_end, end);
        }

        Ok(Token {
            kind: TokenKind::Macro { name: &s[name_start..name_end] },
            span: Span::new(pos, post_space.end),
            pre_space,
            post_space,
        })
    }

    /// A `CommentStart` token at `pos`, if a comment-start delimiter matches
    /// (longest-first). Only the delimiter is covered.
    fn detect_comment_start(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules,
    ) -> Option<Token<'s>> {
        let comments = rules.comments.as_ref()?;
        let delim = longest_match(&comments.starts, &self.content[pos..])?;
        let span = Span::new(pos, pos + delim.len());
        Some(Token::new(TokenKind::CommentStart { delim: span.slice(self.content) }, span, pre_space))
    }

    /// A `Specials` token at `pos`, if a specials string matches (longest-first).
    fn detect_specials(&self, pos: usize, pre_space: Span, rules: &TokenRules) -> Option<Token<'s>> {
        let chars = longest_match(&rules.specials, &self.content[pos..])?;
        let span = Span::new(pos, pos + chars.len());
        Some(Token::new(TokenKind::Specials { chars: span.slice(self.content) }, span, pre_space))
    }

    /// Read a `Chars` token at `pos`: a maximal run of content characters, stopping before
    /// any character that could start another kind of token. Errs on a forbidden first
    /// character (with the offending char as recovery token).
    fn read_chars(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules,
        table: &PrefixTable,
    ) -> TokenResult<'s, Token<'s>> {
        let s = self.content;
        let first = s[pos..].chars().next().expect("caller ensured pos < len");

        if rules.forbidden_chars.contains(first) {
            let span = Span::new(pos, pos + first.len_utf8());
            let placeholder = Token::new(TokenKind::Chars(span.slice(s)), span, pre_space);
            return Err(TokenError::new(
                TokenErrorKind::ForbiddenChar { ch: first },
                span,
                Some(TokenRecovery { token: placeholder, resume_pos: span.end }),
            ));
        }

        let mut end = pos + first.len_utf8();
        for c in s[end..].chars() {
            if self.is_token_start_char(c, rules, table) {
                break;
            }
            end += c.len_utf8();
        }
        let span = Span::new(pos, end);
        Ok(Token::new(TokenKind::Chars(span.slice(s)), span, pre_space))
    }

    /// Whether `c` could start a token other than plain content — the stop set for
    /// [`read_chars`](Self::read_chars) runs. Conservative: a first-character match
    /// merely ends the run; the full checks happen on the next `peek`.
    fn is_token_start_char(&self, c: char, rules: &TokenRules, table: &PrefixTable) -> bool {
        if let Some(ws) = &rules.whitespace {
            if ws.chars.contains(c) {
                return true;
            }
        }
        if let Some(macros) = &rules.macros {
            if c == macros.escape_char {
                return true;
            }
        }
        if table.first_chars().contains(c) {
            return true;
        }
        if let Some(comments) = &rules.comments {
            if comments.starts.iter().any(|d| d.starts_with(c)) {
                return true;
            }
        }
        if rules.specials.iter().any(|d| d.starts_with(c)) {
            return true;
        }
        rules.forbidden_chars.contains(c)
    }
}

/// The longest candidate string that is a non-empty prefix of `rest`, if any.
fn longest_match<'a>(candidates: &'a [alloc::string::String], rest: &str) -> Option<&'a str> {
    candidates
        .iter()
        .map(|d| d.as_str())
        .filter(|d| !d.is_empty() && rest.starts_with(d))
        .max_by_key(|d| d.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::rules::{CommentRules, GroupType, GroupTypeId, WhitespaceRules};
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    // Group type ids used by the hardcoded latexlike-flavored test rules.
    const BRACES: GroupTypeId = GroupTypeId::new(0);
    const BRACKETS: GroupTypeId = GroupTypeId::new(1);
    const MATH_INLINE: GroupTypeId = GroupTypeId::new(2);
    const MATH_DISPLAY: GroupTypeId = GroupTypeId::new(3);
    const MATH_INLINE_PAREN: GroupTypeId = GroupTypeId::new(4);
    const MATH_DISPLAY_BRACKET: GroupTypeId = GroupTypeId::new(5);

    /// Hardcoded-for-now LaTeX-flavored rules (ARCHITECTURE.md §9 Phase 2); the real
    /// defaults arrive with the latexlike preset (Phase 7).
    fn latex_rules() -> TokenRules {
        TokenRules {
            whitespace: Some(WhitespaceRules { chars: " \t\n\r\u{000B}\u{000C}".into() }),
            macros: Some(MacroRules {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            }),
            group_types: vec![
                group(BRACES, "{", "}"),
                group(BRACKETS, "[", "]"),
                group(MATH_INLINE, "$", "$"),
                group(MATH_DISPLAY, "$$", "$$"),
                group(MATH_INLINE_PAREN, r"\(", r"\)"),
                group(MATH_DISPLAY_BRACKET, r"\[", r"\]"),
            ],
            comments: Some(CommentRules { starts: vec!["%".into()] }),
            paragraph_breaks: true,
            specials: Vec::new(),
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn group(id: GroupTypeId, open: &str, close: &str) -> GroupType {
        GroupType { id, open: open.into(), close: close.into() }
    }

    fn sp(start: usize, end: usize) -> Span {
        Span::new(start, end)
    }

    /// peek with rules, building the prefix table on the fly.
    fn peek_with<'s>(reader: &mut StdTokenReader<'s>, rules: &TokenRules) -> Option<Token<'s>> {
        let table = PrefixTable::for_rules(rules);
        reader.peek(rules, &table).unwrap()
    }

    fn next_with<'s>(reader: &mut StdTokenReader<'s>, rules: &TokenRules) -> Option<Token<'s>> {
        let table = PrefixTable::for_rules(rules);
        reader.next(rules, &table).unwrap()
    }

    /// Rules with the given group type's close delimiter expected (as the group parser
    /// sets up when entering an ambiguously-delimited group).
    fn expecting_close(id: GroupTypeId) -> TokenRules {
        TokenRules { expecting_group_close: Some(id), ..latex_rules() }
    }

    // --- chars ------------------------------------------------------------------------

    #[test]
    fn simple_chars() {
        // pylatexenc emits per-character tokens; techy emits maximal zero-copy runs
        // stopping at whitespace (which becomes the next token's pre_space).
        let mut tr = StdTokenReader::new("Some Chars");
        let rules = latex_rules();

        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::Chars("Some"), sp(0, 4), Span::empty(0)),
        );
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::Chars("Chars"), sp(5, 10), sp(4, 5)),
        );
        assert_eq!(next_with(&mut tr, &rules), None);
    }

    #[test]
    fn chars_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}Some Chars", pre_space);
        let mut tr = StdTokenReader::new(&text);

        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap(),
            Token::new(TokenKind::Chars("Some"), sp(7, 11), sp(0, 7)),
        );
    }

    #[test]
    fn peek_does_not_advance() {
        let mut tr = StdTokenReader::new("abc");
        let rules = latex_rules();
        let first = peek_with(&mut tr, &rules);
        assert_eq!(peek_with(&mut tr, &rules), first);
        assert_eq!(tr.pos(), 0);
    }

    #[test]
    fn chars_run_stops_before_token_start_chars() {
        let mut tr = StdTokenReader::new("ab{c%d\\e$f");
        let rules = latex_rules();
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("ab"));
        assert_eq!(tr.pos(), 2);
    }

    #[test]
    fn chars_multibyte() {
        let mut tr = StdTokenReader::new("héllo→ wörld");
        let rules = latex_rules();
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("héllo→"));
        let token = next_with(&mut tr, &rules).unwrap();
        assert_eq!(token.kind, TokenKind::Chars("wörld"));
        assert_eq!(token.span.slice("héllo→ wörld"), "wörld");
    }

    // --- macros -----------------------------------------------------------------------

    #[test]
    fn macro_with_post_space() {
        let text = r"\somemacro and more stuff";
        let mut tr = StdTokenReader::new(text);

        // span includes the post_space (pylatexenc: pos_end past post_space).
        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap(),
            Token {
                kind: TokenKind::Macro { name: "somemacro" },
                span: sp(0, 11),
                pre_space: Span::empty(0),
                post_space: sp(10, 11),
            },
        );
    }

    #[test]
    fn macro_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}\\somemacro and more stuff", pre_space);
        let mut tr = StdTokenReader::new(&text);

        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap(),
            Token {
                kind: TokenKind::Macro { name: "somemacro" },
                span: sp(7, 18),
                pre_space: sp(0, 7),
                post_space: sp(17, 18),
            },
        );
    }

    #[test]
    fn single_char_macro_takes_no_post_space() {
        let mut tr = StdTokenReader::new(r"\& also");
        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap(),
            Token::new(TokenKind::Macro { name: "&" }, sp(0, 2), Span::empty(0)),
        );
    }

    #[test]
    fn accent_macro_then_chars() {
        let mut tr = StdTokenReader::new(r"\`accent");
        let rules = latex_rules();
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::Macro { name: "`" }, sp(0, 2), Span::empty(0)),
        );
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("accent"));
    }

    #[test]
    fn macro_post_space_stops_before_paragraph_break() {
        // Whitespace after the macro contains 2+ newlines: no post_space at all
        // (pylatexenc: "put back whitespace that breaks into a new paragraph").
        let mut tr = StdTokenReader::new("\\macroname\n  \n ");
        let rules = latex_rules();
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token {
                kind: TokenKind::Macro { name: "macroname" },
                span: sp(0, 10),
                pre_space: Span::empty(0),
                post_space: Span::empty(10),
            },
        );
        // ... and the paragraph break follows as its own token.
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::ParagraphBreak, sp(10, 14), Span::empty(10)),
        );
    }

    #[test]
    fn macro_post_space_kept_up_to_paragraph_break() {
        let mut tr = StdTokenReader::new("\\macroname   \n  \n ");
        let rules = latex_rules();
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token {
                kind: TokenKind::Macro { name: "macroname" },
                span: sp(0, 13),
                pre_space: Span::empty(0),
                post_space: sp(10, 13),
            },
        );
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::ParagraphBreak, sp(13, 17), Span::empty(13)),
        );
    }

    #[test]
    fn macro_custom_name_chars() {
        let text = r"\zzz1234567890-haha_works! is a macro here";
        let mut tr = StdTokenReader::new(text);
        let rules = TokenRules {
            macros: Some(MacroRules {
                escape_char: '\\',
                name_chars: "0123456789abcdefghijklmnopqrstuvwxyz\
                             ABCDEFGHIJKLMNOPQRSTUVWXYZ_+!-"
                    .into(),
            }),
            ..latex_rules()
        };

        let name = "zzz1234567890-haha_works!";
        assert_eq!(
            peek_with(&mut tr, &rules).unwrap(),
            Token {
                kind: TokenKind::Macro { name },
                span: sp(0, 1 + name.len() + 1),
                pre_space: Span::empty(0),
                post_space: sp(1 + name.len(), 1 + name.len() + 1),
            },
        );
    }

    #[test]
    fn macros_disabled_escape_is_plain_content() {
        let mut tr = StdTokenReader::new(r"\foo bar");
        let rules = TokenRules { macros: None, ..latex_rules() };
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars(r"\foo"));
    }

    // --- environments are NOT tokens (departure from pylatexenc) -----------------------

    #[test]
    fn begin_environment_is_ordinary_tokens() {
        // pylatexenc emits a begin_environment token; techy tokenizes \begin{equation}
        // as macro + group open + chars (+ group close): environment recognition is a
        // construct-parser concern.
        let mut tr = StdTokenReader::new(r"\begin{equation}");
        let rules = latex_rules();

        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            // No post_space: '{' follows the name directly.
            Token::new(TokenKind::Macro { name: "begin" }, sp(0, 6), Span::empty(0)),
        );
        assert_eq!(
            next_with(&mut tr, &rules).unwrap().kind,
            TokenKind::GroupOpen { delim: "{", group_type: BRACES },
        );
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("equation"));
        assert_eq!(
            next_with(&mut tr, &rules).unwrap().kind,
            TokenKind::GroupClose { delim: "}", group_type: BRACES },
        );
    }

    #[test]
    fn begin_prefixed_macro_name_reads_fully() {
        let mut tr = StdTokenReader::new(r"\beginMacroWithConfusingName");
        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap().kind,
            TokenKind::Macro { name: "beginMacroWithConfusingName" },
        );
    }

    // --- groups -----------------------------------------------------------------------

    #[test]
    fn group_open_and_close() {
        let rules = latex_rules();

        let mut tr = StdTokenReader::new("{begin group here");
        assert_eq!(
            peek_with(&mut tr, &rules).unwrap(),
            Token::new(
                TokenKind::GroupOpen { delim: "{", group_type: BRACES },
                sp(0, 1),
                Span::empty(0),
            ),
        );

        let mut tr = StdTokenReader::new("} a braced group just ended here");
        assert_eq!(
            peek_with(&mut tr, &rules).unwrap(),
            Token::new(
                TokenKind::GroupClose { delim: "}", group_type: BRACES },
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
            peek_with(&mut tr, &latex_rules()).unwrap(),
            Token::new(
                TokenKind::GroupOpen { delim: "{", group_type: BRACES },
                sp(7, 8),
                sp(0, 7),
            ),
        );
    }

    #[test]
    fn optional_argument_brackets_are_group_tokens() {
        let mut tr = StdTokenReader::new("[(i)]");
        let rules = latex_rules();
        assert_eq!(
            next_with(&mut tr, &rules).unwrap().kind,
            TokenKind::GroupOpen { delim: "[", group_type: BRACKETS },
        );
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("(i)"));
        assert_eq!(
            next_with(&mut tr, &rules).unwrap().kind,
            TokenKind::GroupClose { delim: "]", group_type: BRACKETS },
        );
    }

    // --- math-style ambiguous group delimiters ------------------------------------------

    #[test]
    fn ambiguous_delimiter_reads_as_open_by_default() {
        let mut tr = StdTokenReader::new("$x$");
        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap().kind,
            TokenKind::GroupOpen { delim: "$", group_type: MATH_INLINE },
        );
    }

    #[test]
    fn expected_close_wins_over_open_interpretation() {
        let mut tr = StdTokenReader::new("$ and more");
        assert_eq!(
            peek_with(&mut tr, &expecting_close(MATH_INLINE)).unwrap().kind,
            TokenKind::GroupClose { delim: "$", group_type: MATH_INLINE },
        );
    }

    #[test]
    fn close_only_delimiter_reads_as_close_even_unexpected() {
        // "report closing '\)' also with incorrect parsing state -- it's not the
        // tokenizer's job to report syntax errors" (pylatexenc test suite).
        let mut tr = StdTokenReader::new(r"\) rest");
        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap().kind,
            TokenKind::GroupClose { delim: r"\)", group_type: MATH_INLINE_PAREN },
        );
    }

    #[test]
    fn escape_led_delimiters_win_over_macro_interpretation() {
        let rules = latex_rules();

        let mut tr = StdTokenReader::new(r" \(x\)");
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(
                TokenKind::GroupOpen { delim: r"\(", group_type: MATH_INLINE_PAREN },
                sp(1, 3),
                sp(0, 1),
            ),
        );

        let mut tr = StdTokenReader::new("\n\\[ cx^2 \\]");
        assert_eq!(
            peek_with(&mut tr, &rules).unwrap(),
            Token::new(
                TokenKind::GroupOpen { delim: r"\[", group_type: MATH_DISPLAY_BRACKET },
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
        let plain = latex_rules();
        let in_inline = expecting_close(MATH_INLINE);
        let in_display = expecting_close(MATH_DISPLAY);

        let cases: [(usize, &TokenRules, TokenKind<'_>, usize); 8] = [
            // (pos, rules, expected kind, expected end)
            (1, &plain, TokenKind::GroupOpen { delim: "$", group_type: MATH_INLINE }, 2),
            // expected close beats the longest ('$$') match:
            (9, &in_inline, TokenKind::GroupClose { delim: "$", group_type: MATH_INLINE }, 10),
            (10, &plain, TokenKind::GroupOpen { delim: "$", group_type: MATH_INLINE }, 11),
            (18, &in_inline, TokenKind::GroupClose { delim: "$", group_type: MATH_INLINE }, 19),
            // not expecting a close: longest match wins -> display '$$':
            (19, &plain, TokenKind::GroupOpen { delim: "$$", group_type: MATH_DISPLAY }, 21),
            (30, &plain, TokenKind::GroupOpen { delim: "$", group_type: MATH_INLINE }, 31),
            (34, &in_inline, TokenKind::GroupClose { delim: "$", group_type: MATH_INLINE }, 35),
            (36, &in_display, TokenKind::GroupClose { delim: "$$", group_type: MATH_DISPLAY }, 38),
        ];

        for (pos, rules, kind, end) in cases {
            let mut tr = StdTokenReader::new(text);
            tr.move_to_pos(pos);
            let token = peek_with(&mut tr, rules).unwrap();
            assert_eq!(token.kind, kind, "at pos {}", pos);
            assert_eq!(token.span, sp(pos, end), "at pos {}", pos);
        }
    }

    // --- comments ---------------------------------------------------------------------

    #[test]
    fn comment_start_token_covers_only_the_delimiter() {
        // pylatexenc's comment token includes content and post_space; techy's covers the
        // start delimiter only (minimal structural tokens) — the comment construct parser
        // reads the rest (Phase 6).
        let mut tr = StdTokenReader::new("% Comment here\n  more stuff");
        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap(),
            Token::new(TokenKind::CommentStart { delim: "%" }, sp(0, 1), Span::empty(0)),
        );
    }

    #[test]
    fn comment_start_with_pre_space() {
        let pre_space = "   \t\n \t";
        let text = format!("{}% Comment here\n  more stuff", pre_space);
        let mut tr = StdTokenReader::new(&text);
        assert_eq!(
            peek_with(&mut tr, &latex_rules()).unwrap(),
            Token::new(TokenKind::CommentStart { delim: "%" }, sp(7, 8), sp(0, 7)),
        );
    }

    #[test]
    fn comment_alternative_start_string_longest_wins() {
        let text = "%!!COMMENT!! Comment here\n  more stuff";
        let mut tr = StdTokenReader::new(text);
        let rules = TokenRules {
            comments: Some(CommentRules { starts: vec!["%".into(), "%!!COMMENT!!".into()] }),
            ..latex_rules()
        };
        assert_eq!(
            peek_with(&mut tr, &rules).unwrap(),
            Token::new(
                TokenKind::CommentStart { delim: "%!!COMMENT!!" },
                sp(0, 12),
                Span::empty(0),
            ),
        );
    }

    #[test]
    fn comments_disabled_percent_is_plain_content() {
        let mut tr = StdTokenReader::new("a % here");
        let rules = TokenRules { comments: None, ..latex_rules() };
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("a"));
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::Chars("%"), sp(2, 3), sp(1, 2)),
        );
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("here"));
    }

    // --- paragraph breaks ---------------------------------------------------------------

    #[test]
    fn paragraph_break_token() {
        // "Abc    \n\n  z": break spans first..last newline; leading run is pre_space,
        // whitespace after the last newline is left for the next token.
        let mut tr = StdTokenReader::new("Abc    \n\n  z");
        let rules = latex_rules();
        tr.move_to_pos(3);

        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::ParagraphBreak, sp(7, 9), sp(3, 7)),
        );
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::Chars("z"), sp(11, 12), sp(9, 11)),
        );
    }

    #[test]
    fn paragraph_break_with_inner_whitespace() {
        let mut tr = StdTokenReader::new("Abc  \t \n   \t\nz");
        let rules = latex_rules();
        tr.move_to_pos(3);
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::ParagraphBreak, sp(7, 13), sp(3, 7)),
        );
    }

    #[test]
    fn paragraph_break_in_trailing_whitespace_still_emitted() {
        let mut tr = StdTokenReader::new("x  \n\n  ");
        let rules = latex_rules();
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("x"));
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::ParagraphBreak, sp(3, 5), sp(1, 3)),
        );
        assert_eq!(next_with(&mut tr, &rules), None);
    }

    #[test]
    fn paragraph_breaks_disabled() {
        let mut tr = StdTokenReader::new("Abc\n\nNew paragraph");
        let rules = TokenRules { paragraph_breaks: false, ..latex_rules() };
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("Abc"));
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::Chars("New"), sp(5, 8), sp(3, 5)),
        );
    }

    // --- specials -----------------------------------------------------------------------

    #[test]
    fn specials_recognized() {
        let mut tr = StdTokenReader::new("a&b");
        let rules = TokenRules { specials: vec!["&".into(), "~".into()], ..latex_rules() };
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("a"));
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::Specials { chars: "&" }, sp(1, 2), Span::empty(1)),
        );
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("b"));
    }

    #[test]
    fn specials_longest_match_wins() {
        let mut tr = StdTokenReader::new("---x");
        let rules = TokenRules {
            specials: vec!["-".into(), "---".into(), "--".into()],
            ..latex_rules()
        };
        assert_eq!(
            peek_with(&mut tr, &rules).unwrap().kind,
            TokenKind::Specials { chars: "---" },
        );
    }

    #[test]
    fn specials_not_matching_falls_through_to_chars() {
        // '~' is a specials *start* char, so it ends the preceding run; but "~~" only
        // matches as a whole, so a lone '~' becomes content.
        let mut tr = StdTokenReader::new("a~b");
        let rules = TokenRules { specials: vec!["~~".into()], ..latex_rules() };
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("a"));
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("~b"));
    }

    // --- errors and recovery -------------------------------------------------------------

    #[test]
    fn forbidden_char_error_with_recovery() {
        let mut tr = StdTokenReader::new("% forbidden here");
        let rules = TokenRules {
            comments: None,
            forbidden_chars: "%$".into(),
            ..latex_rules()
        };
        let table = PrefixTable::for_rules(&rules);

        let err = tr.peek(&rules, &table).unwrap_err();
        assert_eq!(err.kind(), TokenErrorKind::ForbiddenChar { ch: '%' });
        assert_eq!(err.span(), sp(0, 1));

        // Tolerant continuation: use the recovery token, resume past it.
        let recovery = err.into_recovery().unwrap();
        assert_eq!(
            recovery.token,
            Token::new(TokenKind::Chars("%"), sp(0, 1), Span::empty(0)),
        );
        assert_eq!(recovery.resume_pos, 1);
        tr.move_to_pos(recovery.resume_pos);
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("forbidden"));
    }

    #[test]
    fn end_of_stream_after_escape_error_with_recovery() {
        let mut tr = StdTokenReader::new(r"abc \");
        let rules = latex_rules();
        let table = PrefixTable::for_rules(&rules);

        assert_eq!(tr.next(&rules, &table).unwrap().unwrap().kind, TokenKind::Chars("abc"));

        let err = tr.peek(&rules, &table).unwrap_err();
        assert_eq!(err.kind(), TokenErrorKind::EndOfStreamAfterEscape { escape_char: '\\' });
        assert_eq!(err.span(), sp(4, 5));

        let recovery = err.into_recovery().unwrap();
        assert_eq!(
            recovery.token,
            Token::new(TokenKind::Chars(""), Span::empty(4), sp(3, 4)),
        );
        assert_eq!(recovery.resume_pos, 5); // resume at end of input
        tr.move_to_pos(recovery.resume_pos);
        assert_eq!(tr.peek(&rules, &table).unwrap(), None);
    }

    // --- end of stream --------------------------------------------------------------------

    #[test]
    fn end_of_stream() {
        let rules = latex_rules();

        let mut tr = StdTokenReader::new("");
        assert_eq!(peek_with(&mut tr, &rules), None);

        // Trailing whitespace without a paragraph break yields no token; it is
        // recoverable from pos()..content.len().
        let mut tr = StdTokenReader::new("x   ");
        assert_eq!(next_with(&mut tr, &rules).unwrap().kind, TokenKind::Chars("x"));
        assert_eq!(peek_with(&mut tr, &rules), None);
        assert_eq!(tr.pos(), 1);
        assert!(!tr.is_at_end());
    }

    // --- whitespace handling disabled ------------------------------------------------------

    #[test]
    fn whitespace_disabled_gives_character_level_content() {
        let mut tr = StdTokenReader::new("a b\nc{d");
        let rules = TokenRules { whitespace: None, ..latex_rules() };
        assert_eq!(
            next_with(&mut tr, &rules).unwrap(),
            Token::new(TokenKind::Chars("a b\nc"), sp(0, 5), Span::empty(0)),
        );
        assert_eq!(
            next_with(&mut tr, &rules).unwrap().kind,
            TokenKind::GroupOpen { delim: "{", group_type: BRACES },
        );
    }

    // --- movement -----------------------------------------------------------------------

    #[test]
    fn move_past_and_move_to_flags() {
        let text = "  \\vec b";
        let mut tr = StdTokenReader::new(text);
        let rules = latex_rules();

        let token = peek_with(&mut tr, &rules).unwrap();
        assert_eq!(token.kind, TokenKind::Macro { name: "vec" });
        assert_eq!(token.span, sp(2, 7)); // includes post_space
        assert_eq!(token.pre_space, sp(0, 2));
        assert_eq!(token.post_space, sp(6, 7));

        tr.move_past(&token, true);
        assert_eq!(tr.pos(), 7); // past post_space
        tr.move_past(&token, false);
        assert_eq!(tr.pos(), 6); // before post_space

        tr.move_to(&token, false);
        assert_eq!(tr.pos(), 2); // at the token
        tr.move_to(&token, true);
        assert_eq!(tr.pos(), 0); // before pre_space

        // Reading again after move_to yields the same token.
        assert_eq!(peek_with(&mut tr, &rules).unwrap(), token);
    }

    // --- an end-to-end walk (port of test_multiple_tokens_advances_and_stuff) -------------

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
        let rules = latex_rules();
        let table = PrefixTable::for_rules(&rules);
        let mut tr = StdTokenReader::new(text);

        let find = |needle: &str| text.find(needle).unwrap();

        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            Token::new(TokenKind::Chars("Text"), sp(0, 4), Span::empty(0)),
        );

        let p = find(r"\`");
        tr.move_to_pos(p);
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            Token::new(TokenKind::Macro { name: "`" }, sp(p, p + 2), Span::empty(p)),
        );

        let p = find(r"\textbf") - 1; // pre space
        tr.move_to_pos(p);
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            // '{' follows the name: no post_space.
            Token::new(TokenKind::Macro { name: "textbf" }, sp(p + 1, p + 8), sp(p, p + 1)),
        );
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap().kind,
            TokenKind::GroupOpen { delim: "{", group_type: BRACES },
        );

        let p = find(r"\vec"); // post-space present
        tr.move_to_pos(p);
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            Token {
                kind: TokenKind::Macro { name: "vec" },
                span: sp(p, p + 5),
                pre_space: Span::empty(p),
                post_space: sp(p + 4, p + 5),
            },
        );

        let p = find(r"\&") - 1; // pre-space and *no* post-space
        tr.move_to_pos(p);
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            Token::new(TokenKind::Macro { name: "&" }, sp(p + 1, p + 3), sp(p, p + 1)),
        );

        // \begin is just a macro token; the environment name is ordinary group+chars.
        let p = find(r"\begin");
        tr.move_to_pos(p);
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap().kind,
            TokenKind::Macro { name: "begin" },
        );

        let p = find("@@@") + 3; // pre space up to \end
        tr.move_to_pos(p);
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            Token::new(TokenKind::Macro { name: "end" }, sp(p + 6, p + 10), sp(p, p + 6)),
        );

        let p = find("%") - 1;
        tr.move_to_pos(p);
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            Token::new(TokenKind::CommentStart { delim: "%" }, sp(p + 1, p + 2), sp(p, p + 1)),
        );

        // \mymacro directly precedes a paragraph break: no post_space.
        let p = find(r"\mymacro");
        tr.move_to_pos(p);
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            Token::new(TokenKind::Macro { name: "mymacro" }, sp(p, p + 8), Span::empty(p)),
        );
        let p2 = find("\n\n");
        assert_eq!(
            tr.next(&rules, &table).unwrap().unwrap(),
            Token::new(TokenKind::ParagraphBreak, sp(p2, p2 + 2), Span::empty(p2)),
        );
    }
}
