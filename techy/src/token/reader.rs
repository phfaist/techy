//! The [`TokenReader`] trait and the standard rules-driven implementation,
//! [`StdTokenReader`].
//!
//! `StdTokenReader` follows pylatexenc's proven `LatexTokenReader` protocol: `peek` parses
//! the token at the current position without advancing; `move_past`/`move_to` reposition
//! relative to a token; `move_to_pos` repositions absolutely; `next` = peek + move-past. The scanning core is decomposed into
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
use super::token::{Token, TokenKind};

/// One of the four boundaries of a token, in reading order.
///
/// A token occupies a stretch of the stream that has two optional whitespace wings:
/// *pre-space* (content whitespace read just before the token, outside its span) and
/// *post-space* (syntactic whitespace consumed just after the token proper, inside its
/// span — only [`Command`](TokenKind::Command) and [`Comment`](TokenKind::Comment)
/// tokens have any). An edge names one of the four boundaries this creates, and is how
/// a construct parser asks a [`TokenReader`] for a position or a span without knowing
/// how the reader stores either.
///
/// For a kind without post-space, [`End`](TokenEdge::End) and
/// [`EndPastPostSpace`](TokenEdge::EndPastPostSpace) coincide; for a token with no
/// preceding whitespace, [`StartBeforePreSpace`](TokenEdge::StartBeforePreSpace) and
/// [`Start`](TokenEdge::Start) coincide.
///
/// The ordering (`PartialOrd`/`Ord`) is the declaration order, which is reading order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenEdge {
    /// Where the token's pre-space begins — the position the stream stood at when the
    /// token was read.
    StartBeforePreSpace,
    /// Where the token proper begins (its pre-space has been passed).
    Start,
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
/// # Contract
///
/// - **`peek` is idempotent per (position, state instance):** repeated calls at the same
///   position with the *same* `ParsingState` instance return the same result. The state
///   arrives as an `&Arc` precisely so implementations may memoize on that key: clone
///   the `Arc` into the cache — pointer identity is a sound key *only while a strong
///   reference pins the allocation* (a dropped state's address can be recycled for a
///   different state). A *different* state instance — even one derived with an empty
///   delta — relieves `peek` of any obligation to repeat itself.
/// - At the end of the stream `peek` returns the terminal, idempotent
///   [`EndOfStream`](TokenKind::EndOfStream) token (never an `Option`); its `pre_space`
///   carries the final whitespace.
/// - **Absent features yield no tokens:** a token kind belonging to a feature the
///   language declares absent ([`Lang::Features`]) must never be produced — no
///   `GroupOpen`/`GroupClose` without the groups feature, no `Command`, `Comment`,
///   `Specials`, or `ParagraphBreak` without theirs. The parsing machinery treats
///   any such token as a violated contract and reports an implementation error
///   instead of processing it — uniformly across token kinds.
pub trait TokenReader<'s, L: Lang> {
    /// Parse the token at the current position without advancing.
    fn peek(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<'s, L, Token<'s, L>>;

    /// Move immediately past `tok`. If `skip_post_space` is true the position lands after
    /// the token's post-space; otherwise right after the token proper, before it.
    fn move_past(&mut self, tok: &Token<'s, L>, skip_post_space: bool);

    /// Move to `tok`'s own start, so that it would be read again. If `rewind_pre_space`
    /// is true the position lands before the token's preceding whitespace instead.
    fn move_to(&mut self, tok: &Token<'s, L>, rewind_pre_space: bool);

    /// Move to an absolute byte position: a
    /// [`TokenRecovery::resume_pos`](super::TokenRecovery), an argument parser's
    /// absent-argument rewind target. The position must be one the reader can
    /// meaningfully resume from (for text-scanning readers: on a `char` boundary, at
    /// most the content's length).
    ///
    /// Deliberately bidirectional — it also serves rewinds — so implementations assert
    /// nothing about the direction of the move. When adopting a `TokenRecovery`, the
    /// *caller* enforces the [`resume_pos` advancement contract](super::TokenRecovery#contract-resume_pos-must-advance-the-reader)
    /// (the content loop aborts if the reader did not advance).
    fn move_to_pos(&mut self, pos: usize);

    /// Current byte position.
    fn pos(&self) -> usize;

    /// Parse the token at the current position and move past it (including its
    /// post-space): [`peek`](TokenReader::peek) + [`move_past`](TokenReader::move_past).
    fn next(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<'s, L, Token<'s, L>> {
        let token = self.peek(state)?;
        self.move_past(&token, true);
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

    // `pos`/`move_to_pos` exist both here and on the `TokenReader` trait: the trait
    // impl is generic over `L`, so calling through it on a concrete reader needs `L`
    // pinned by context — these inherent forms serve direct (non-generic) users. The
    // trait impl delegates here; the logic lives in one place.

    /// Current byte position.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Whether the reader is at the end of the content.
    pub fn is_at_end(&self) -> bool {
        self.pos >= self.content.len()
    }

    /// Move to an absolute byte position (must lie on a `char` boundary, at most the
    /// content's length). A violating position is not diagnosed here — the next
    /// [`peek`](TokenReader::peek) validates it and reports an implementation error
    /// (deliberately one validation point, at the consumption boundary).
    pub fn move_to_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    // --- scanning core ------------------------------------------------------------------

    fn peek_impl<L: Lang<SourceOrigin = O, StreamPosition = StdStreamPosition>>(
        &self,
        state: &ParsingState<L>,
    ) -> TokenResult<'s, L, Token<'s, L>> {
        let s = self.content;
        let rules = state.rules();
        let start = self.pos;

        // The position was set through outer-layer hands (`move_to_pos` serves
        // custom recoveries' `resume_pos` and argument rewinds; `move_past` accepts
        // caller-held tokens), so it is validated at this single consumption
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
                Span::empty(start.min(s.len())),
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
            return Ok(Token::new(TokenKind::EndOfStream, Span::empty(pos), pre_space));
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
                .map_err(|error| TokenError::new(error.kind, error.span, None))?;
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
                        Span::empty(pos),
                        None,
                    ));
                }
                return Ok(Token::new(
                    TokenKind::Specials {
                        callable_type: m.callable_type,
                        // The matched text is the name (the `SpecialsMatch` contract).
                        name: &s[pos..m.end],
                        spec: m.spec,
                    },
                    Span::new(pos, m.end),
                    pre_space,
                ));
            }
        }

        if <L::Features as LangFeatures>::ForbiddenChars::PRESENT
            && rules.forbidden_chars().contains(c)
        {
            let span = Span::new(pos, pos + c.len_utf8());
            let placeholder = Token::new(TokenKind::Char(c), span, pre_space);
            return Err(TokenError::new(
                TokenErrorKind::ForbiddenChar(ForbiddenChar::new(c)),
                span,
                Some(TokenRecovery { token: placeholder, resume_pos: span.end() }),
            ));
        }

        Ok(Token::new(TokenKind::Char(c), Span::new(pos, pos + c.len_utf8()), pre_space))
    }

    /// A `ParagraphBreak` token if a `\n\s*\n` whitespace sequence starts at `pos` (which
    /// `skip_whitespace` guarantees whenever it stopped at a consumable-whitespace
    /// newline). The token spans from the first through the last newline of the run;
    /// whitespace after the last newline is left for the next token's pre-space.
    /// Requires the paragraphs *and* whitespace features, each declared present and
    /// enabled at runtime: a language that declares either absent never yields a break.
    fn detect_paragraph_break<L: Lang<SourceOrigin = O, StreamPosition = StdStreamPosition>>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
    ) -> Option<Token<'s, L>> {
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
        Some(Token::new(TokenKind::ParagraphBreak, Span::new(pos, last_nl_end), pre_space))
    }

    /// A `GroupOpen`/`GroupClose` token at `pos`, if a delimiter matches. The close
    /// delimiter expected per `rules.expecting_group_close()` takes precedence; otherwise
    /// the longest table match wins, read as an opener when the string is ambiguous.
    fn detect_group_delimiter<L: Lang<SourceOrigin = O, StreamPosition = StdStreamPosition>>(
        &self,
        pos: usize,
        pre_space: Span,
        state: &ParsingState<L>,
    ) -> Option<Token<'s, L>> {
        let rules = state.rules();
        let rest = &self.content[pos..];

        if let Some(expected) = rules.expecting_group_close() {
            if !expected.close.is_empty() && rest.starts_with(expected.close.as_str()) {
                let span = Span::new(pos, pos + expected.close.len());
                return Some(Token::new(
                    TokenKind::GroupClose { delim: span.slice(self.content) },
                    span,
                    pre_space,
                ));
            }
        }

        // `None` when the language declares the groups feature absent — no table
        // exists, and no delimiter can match.
        let entry = state.prefix_table()?.match_at(rest)?;
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
    fn read_command<L: Lang<SourceOrigin = O, StreamPosition = StdStreamPosition>>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
        rule: &CommandRule,
    ) -> TokenResult<'s, L, Token<'s, L>> {
        let s = self.content;
        let name_start = pos + rule.escape_char.len_utf8();

        if name_start >= s.len() {
            // Recovery: a `Char` placeholder covering the dangling escape byte itself
            // (`name_start` == `s.len()` here), so the byte stays in the tree — it
            // joins the pending chars run and the tolerant parse keeps the partition
            // invariant (decided July 2026, Action 02; supersedes the empty
            // `EndOfStream` placeholder, which dropped the byte from the AST).
            let span = Span::new(pos, name_start);
            let placeholder = Token::new(TokenKind::Char(rule.escape_char), span, pre_space);
            return Err(TokenError::new(
                TokenErrorKind::EndOfStreamAfterEscape(EndOfStreamAfterEscape::new(
                    rule.escape_char,
                )),
                span,
                Some(TokenRecovery { token: placeholder, resume_pos: span.end() }),
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
            Span::new(pos, post_space.end()),
            pre_space,
        ))
    }

    /// A whole-comment token at `pos`, if a comment-start delimiter matches
    /// (longest-first across the rules). The content runs to the end of the line; the
    /// terminating newline plus following indentation is the token's post-space — unless
    /// that whitespace forms a paragraph break, in which case the comment takes no
    /// post-space and the break surfaces as its own token. Returns `None` when the
    /// language declares the comments feature absent (such a language stores no
    /// comment rules data at all).
    fn read_comment<L: Lang<SourceOrigin = O, StreamPosition = StdStreamPosition>>(
        &self,
        pos: usize,
        pre_space: Span,
        rules: &TokenRules<L>,
    ) -> Option<Token<'s, L>> {
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

        Some(Token::new(
            TokenKind::Comment {
                start: Span::new(pos, content_start),
                content: &s[content_start..content_end],
                post_space,
            },
            Span::new(pos, post_space.end()),
            pre_space,
        ))
    }
}

impl<'s, O, L> TokenReader<'s, L> for StdTokenReader<'s, O>
where
    O: SourceOrigin,
    L: Lang<SourceOrigin = O, StreamPosition = StdStreamPosition>,
{
    fn peek(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<'s, L, Token<'s, L>> {
        self.peek_impl(state)
    }

    fn move_past(&mut self, tok: &Token<'s, L>, skip_post_space: bool) {
        if skip_post_space {
            self.pos = tok.span.end();
        } else {
            // Post-space is a trailing sub-range of `span`, so its `start` is the end
            // of the token proper — for every kind (empty post-space sits at `span.end`).
            self.pos = tok.post_space().start();
        }
    }

    fn move_to(&mut self, tok: &Token<'s, L>, rewind_pre_space: bool) {
        if rewind_pre_space {
            self.pos = tok.pre_space.start();
        } else {
            self.pos = tok.span.start();
        }
    }

    fn move_to_pos(&mut self, pos: usize) {
        StdTokenReader::move_to_pos(self, pos);
    }

    fn pos(&self) -> usize {
        StdTokenReader::pos(self)
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

    fn peek<'s>(
        tr: &mut StdTokenReader<'s>,
        st: &Arc<ParsingState<TestLang>>,
    ) -> Token<'s, TestLang> {
        TokenReader::peek(tr, st).unwrap()
    }

    fn next<'s>(
        tr: &mut StdTokenReader<'s>,
        st: &Arc<ParsingState<TestLang>>,
    ) -> Token<'s, TestLang> {
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
        let source = Arc::new(Source::new("ab c"));
        let mut tr = StdTokenReader::new(&source);
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
        assert_eq!(tr.pos(), 0);
    }

    #[test]
    fn char_multibyte() {
        let text = "héllo→";
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);

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
        let source = Arc::new(Source::new(&text));
        let mut tr = StdTokenReader::new(&source);

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
        let source = Arc::new(Source::new(r"\& also"));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new(r"\`accent"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        let token = next(&mut tr, &st);
        assert_eq!(token.kind, TokenKind::Command { name: "`", escape_char: '\\', post_space: Span::empty(2) });
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
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
        let source = Arc::new(Source::new("\\macroname   \n  \n "));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new(r"\foo"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.commands.rules = Vec::new();
        let st = state(rules);
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('\\'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('f'));
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
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('\\'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('f'));
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
        let source = Arc::new(Source::new(r"\beginMacroWithConfusingName"));
        let mut tr = StdTokenReader::new(&source);
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

        let source = Arc::new(Source::new("{begin group here"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &st),
            Token::new(
                TokenKind::GroupOpen { delim: "{", rule: rule_of(BRACES) },
                sp(0, 1),
                Span::empty(0),
            ),
        );

        let source = Arc::new(Source::new("} a braced group just ended here"));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new(&text));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new("[(i)]"));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new("$x$"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &state(latex_rules())).kind,
            TokenKind::GroupOpen { delim: "$", rule: rule_of(MATH_INLINE) },
        );
    }

    #[test]
    fn expected_close_wins_over_open_interpretation() {
        let source = Arc::new(Source::new("$ and more"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &expecting_close(MATH_INLINE)).kind,
            TokenKind::GroupClose { delim: "$" },
        );
    }

    #[test]
    fn close_only_delimiter_reads_as_close_even_unexpected() {
        // "report closing '\)' also with incorrect parsing state -- it's not the
        // tokenizer's job to report syntax errors" (pylatexenc test suite).
        let source = Arc::new(Source::new(r"\) rest"));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &state(latex_rules())).kind,
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
            Token::new(
                TokenKind::GroupOpen { delim: r"\(", rule: rule_of(MATH_INLINE_PAREN) },
                sp(1, 3),
                sp(0, 1),
            ),
        );

        let source = Arc::new(Source::new("\n\\[ cx^2 \\]"));
        let mut tr = StdTokenReader::new(&source);
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

        #[allow(clippy::type_complexity)]
        let cases: [(usize, &Arc<ParsingState<TestLang>>, TokenKind<'_, TestLang>, usize); 8] = [
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
            let source = Arc::new(Source::new(text));
            let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new("% Comment here\n  more stuff"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Comment {
                    start: sp(0, 1),
                    content: " Comment here",
                    post_space: sp(14, 17),
                },
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
        let source = Arc::new(Source::new(&text));
        let mut tr = StdTokenReader::new(&source);
        let token = peek(&mut tr, &state(latex_rules()));
        assert_eq!(token.pre_space, sp(0, 7));
        assert_eq!(token.span, sp(7, 24));
        assert_eq!(
            token.kind,
            TokenKind::Comment {
                start: sp(7, 8),
                content: " Comment here",
                post_space: sp(21, 24),
            },
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
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Comment {
                    start: sp(1, 2),
                    content: " c",
                    post_space: Span::empty(4),
                },
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
        let source = Arc::new(Source::new("x% trailing"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('x'));
        assert_eq!(
            next(&mut tr, &st),
            Token::new(
                TokenKind::Comment {
                    start: sp(1, 2),
                    content: " trailing",
                    post_space: Span::empty(11),
                },
                sp(1, 11),
                Span::empty(1),
            ),
        );
        assert_eq!(next(&mut tr, &st).kind, TokenKind::EndOfStream);
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
            Token::new(
                TokenKind::Comment {
                    start: sp(0, 1),
                    content: " note\r",
                    post_space: sp(7, 10),
                },
                sp(0, 10),
                Span::empty(0),
            ),
        );
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('m'));
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
            Token::new(
                TokenKind::Comment {
                    start: sp(0, 12),
                    content: " Comment here",
                    post_space: sp(25, 26),
                },
                sp(0, 26),
                Span::empty(0),
            ),
        );
    }

    #[test]
    fn comments_disabled_percent_is_plain_content() {
        let source = Arc::new(Source::new("a %b"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.comments.rules = Vec::new();
        let st = state(rules);
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        assert_eq!(next(&mut tr, &st), char_token('%', 2, sp(1, 2)));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('b'));
    }

    #[test]
    fn comments_gate_off_is_the_scoped_disable() {
        let source = Arc::new(Source::new("a %b"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.comments.enabled = false;
        let st = state(rules);
        assert!(!st.rules().comment_rules().is_empty());
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        assert_eq!(next(&mut tr, &st), char_token('%', 2, sp(1, 2)));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('b'));
    }

    // --- paragraph breaks ---------------------------------------------------------------

    #[test]
    fn paragraph_break_token() {
        // "Abc    \n\n  z": break spans first..last newline; leading run is pre_space,
        // whitespace after the last newline is left for the next token.
        let source = Arc::new(Source::new("Abc    \n\n  z"));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new("Abc  \t \n   \t\nz"));
        let mut tr = StdTokenReader::new(&source);
        let st = state(latex_rules());
        tr.move_to_pos(3);
        assert_eq!(
            next(&mut tr, &st),
            Token::new(TokenKind::ParagraphBreak, sp(7, 13), sp(3, 7)),
        );
    }

    #[test]
    fn paragraph_break_in_trailing_whitespace_still_emitted() {
        let source = Arc::new(Source::new("x  \n\n  "));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new("Abc\n\nNew"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.paragraphs.enabled = false;
        let st = state(rules);
        tr.move_to_pos(2);
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
        let source = Arc::new(Source::new("---x"));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new("-x"));
        let mut tr = StdTokenReader::new(&source);
        let st = specials_state(latex_rules());
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('-'));
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
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;
        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~".into())
        }
        fn scan_specials(
            _state: &ParsingState<Self>,
            content: &str,
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

        // Out of bounds (`move_to_pos` serves outer-layer resume positions, so the
        // violation is reported at the next read, not panicked).
        let source = Arc::new(Source::new("ab"));
        let mut tr = StdTokenReader::new(&source);
        tr.move_to_pos(5);
        let err = TokenReader::peek(&mut tr, &st).unwrap_err();
        assert!(matches!(err.kind(), TokenErrorKind::Custom(data)
            if data.identifier() == crate::constructs::ImplementationError::IDENTIFIER));
        assert!(err.into_recovery().is_none());

        // Not a char boundary.
        let source = Arc::new(Source::new("é"));
        let mut tr = StdTokenReader::new(&source);
        tr.move_to_pos(1);
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
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('a'));
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('&'));
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('b'));
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
        assert_eq!(TokenReader::next(&mut tr, &st).unwrap().kind, TokenKind::Char('x'));
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
        let source = Arc::new(Source::new(r"a \"));
        let mut tr = StdTokenReader::new(&source);
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

        // Recovery: a Char placeholder covering the dangling escape byte itself, so the
        // byte stays in the tree (the tolerant parse keeps the partition invariant).
        let recovery = err.into_recovery().unwrap();
        assert_eq!(
            recovery.token,
            Token::new(TokenKind::Char('\\'), sp(2, 3), sp(1, 2)),
        );
        assert_eq!(recovery.resume_pos, 3);
        tr.move_to_pos(recovery.resume_pos);
        assert_eq!(peek(&mut tr, &st).kind, TokenKind::EndOfStream);
    }

    // --- end of stream --------------------------------------------------------------------

    #[test]
    fn end_of_stream_token_is_terminal_and_idempotent() {
        let st = state(latex_rules());

        let source = Arc::new(Source::new(""));
        let mut tr = StdTokenReader::new(&source);
        assert_eq!(
            peek(&mut tr, &st),
            Token::new(TokenKind::EndOfStream, Span::empty(0), Span::empty(0)),
        );

        // Trailing whitespace (no paragraph break) is the end-of-stream token's
        // pre_space — reported, so it can land in the node tree.
        let source = Arc::new(Source::new("x   "));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new("a b{"));
        let mut tr = StdTokenReader::new(&source);
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.whitespace.enabled = false;
        let st = state(rules);
        assert_eq!(next(&mut tr, &st), char_token('a', 0, Span::empty(0)));
        assert_eq!(next(&mut tr, &st), char_token(' ', 1, Span::empty(1)));
        assert_eq!(next(&mut tr, &st), char_token('b', 2, Span::empty(2)));
        assert_eq!(
            next(&mut tr, &st).kind,
            TokenKind::GroupOpen { delim: "{", rule: rule_of(BRACES) },
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
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('{'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('a'));
        assert_eq!(next(&mut tr, &st).kind, TokenKind::Char('}'));
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
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);
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
        let source = Arc::new(Source::new(text));
        let mut tr = StdTokenReader::new(&source);

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
                    start: sp(p + 2, p + 3),
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
