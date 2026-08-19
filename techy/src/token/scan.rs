//! The scan helpers: the recognition primitives of the standard tokenizer, as free
//! functions over the text being scanned.
//!
//! A **scan helper** recognizes one construct at a byte offset in a string of content and
//! answers what it found — a *match value*, holding byte ranges
//! ([`Span`](crate::source::Span)) into that content plus the rule that matched — or
//! nothing. A helper advances no position, builds no token, and never sees a
//! [`Source`](crate::source::Source), so a reader may compose the helpers it wants and
//! build tokens of its own from their answers.
//! [`StdTokenReader::scan_std_token_at`](super::StdTokenReader::scan_std_token_at) is the
//! composition the standard reader uses, which is what keeps one implementation per
//! construct.
//!
//! `content` and `pos` mean the same thing in every helper: the text being scanned, and a
//! byte offset into it that must satisfy `pos <= content.len()` and fall on a `char`
//! boundary. A `pos` that violates this panics in all builds (each helper says so in its
//! `# Panics` section); a reader validates the offsets it receives from its caller once,
//! where it receives them, and passes derived offsets on.

use alloc::boxed::Box;
use alloc::format;
use alloc::sync::Arc;
use core::fmt;

use crate::constructs::ImplementationError;
use crate::source::Span;
use crate::state::{FeaturePresence, Lang, LangFeatures, ParsingState};

use super::error::{EndOfStreamAfterEscape, TokenErrorKind};
use super::rules::{CommandRule, GroupRule, TokenRules};
use super::specials::{SpecialsMatch, SpecialsScanError};

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

/// Panic on a `pos` that violates the scan helpers' shared precondition — out of the
/// content's bounds, or not on a `char` boundary. `pos == content.len()` is valid.
///
/// The deliberate panic of this family: the offset a helper is asked about comes from
/// the reader that composes the helpers, which validated it at its own boundary, so an
/// invalid one is a mistake in that reader and not a condition to report.
#[track_caller]
fn check_pos(content: &str, pos: usize) {
    assert!(
        content.is_char_boundary(pos),
        "pos {} is out of bounds or not a char boundary (content len {})",
        pos,
        content.len()
    );
}

/// A paragraph break — a whitespace run holding two or more newlines — beginning
/// exactly at `pos`: `Some(span)` from the first newline through the last newline of the
/// run (whitespace after the last newline is left for the next token's pre-space).
///
/// `None` when the paragraphs or whitespace feature is absent or disabled, when `'\n'`
/// is not a whitespace character, when the text at `pos` does not start with `'\n'`, or
/// when the run holds a single newline. Intended to be called at the offset
/// [`skip_whitespace`] answered, which stops right before the first newline of such a
/// run.
///
/// # Panics
///
/// A `pos` that is out of bounds for `content` or not on a `char` boundary is a
/// caller-contract violation and panics, in all builds — one of the crate's few
/// deliberate panics (see the [Panics list](techy::guide::panics)).
pub fn scan_paragraph_break<L: Lang>(
    content: &str,
    pos: usize,
    rules: &TokenRules<L>,
) -> Option<Span> {
    check_pos(content, pos);
    if !(<L::Features as LangFeatures>::Paragraphs::PRESENT
        && <L::Features as LangFeatures>::Whitespace::PRESENT
        && rules.paragraphs_enabled()
        && rules.whitespace_enabled())
    {
        return None;
    }
    let ws_chars = rules.whitespace_chars();
    if !content[pos..].starts_with('\n') || !ws_chars.contains('\n') {
        return None;
    }
    let mut newlines = 0usize;
    let mut end = pos;
    let mut last_nl_end = pos;
    for c in content[pos..].chars() {
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
    Some(Span::new(pos, last_nl_end))
}

/// A group delimiter recognized at a position, with the rule it belongs to.
///
/// The rule travels with the match in both directions: a reader building a group-open
/// token needs the close delimiter to expect and the group's class without matching
/// again, and a reader that builds tokens of its own may want the same of a close. Only
/// byte ranges into the scanned content and that borrowed rule are carried — nothing
/// that knows where the content came from.
pub enum GroupDelimiterMatch<'r, L: Lang> {
    /// An opening delimiter. Also the answer for a string that both opens and closes
    /// (`$`), unless it is the close the state is waiting for.
    Open {
        /// The delimiter's byte range in the scanned content.
        span: Span,
        /// The rule whose group this delimiter opens.
        rule: &'r Arc<GroupRule<L>>,
    },
    /// A closing delimiter.
    Close {
        /// The delimiter's byte range in the scanned content.
        span: Span,
        /// The rule whose group this delimiter closes.
        rule: &'r Arc<GroupRule<L>>,
    },
}

impl<'r, L: Lang> GroupDelimiterMatch<'r, L> {
    /// The delimiter's byte range in the scanned content, whichever direction matched.
    pub fn span(&self) -> Span {
        match self {
            GroupDelimiterMatch::Open { span, .. }
            | GroupDelimiterMatch::Close { span, .. } => *span,
        }
    }

    /// The rule the delimiter belongs to, whichever direction matched.
    pub fn rule(&self) -> &'r Arc<GroupRule<L>> {
        match self {
            GroupDelimiterMatch::Open { rule, .. }
            | GroupDelimiterMatch::Close { rule, .. } => rule,
        }
    }
}

// Manual impls: derives would demand `L: Clone/Copy/Debug/PartialEq` bounds although no
// `L` value is stored (only a borrowed rule handle, whose contents are bounded via
// `Lang`). Rules compare structurally, as they do in `TokenKind::GroupOpen`.

impl<L: Lang> Clone for GroupDelimiterMatch<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for GroupDelimiterMatch<'_, L> {}

impl<L: Lang> PartialEq for GroupDelimiterMatch<'_, L> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                GroupDelimiterMatch::Open { span: s1, rule: r1 },
                GroupDelimiterMatch::Open { span: s2, rule: r2 },
            ) => s1 == s2 && r1 == r2,
            (
                GroupDelimiterMatch::Close { span: s1, rule: r1 },
                GroupDelimiterMatch::Close { span: s2, rule: r2 },
            ) => s1 == s2 && r1 == r2,
            _ => false,
        }
    }
}

impl<L: Lang> Eq for GroupDelimiterMatch<'_, L> {}

impl<L: Lang> fmt::Debug for GroupDelimiterMatch<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroupDelimiterMatch::Open { span, rule } => {
                f.debug_struct("Open").field("span", span).field("rule", rule).finish()
            }
            GroupDelimiterMatch::Close { span, rule } => {
                f.debug_struct("Close").field("span", span).field("rule", rule).finish()
            }
        }
    }
}

/// The delimiter the standard reader recognizes at `pos`: the close delimiter of
/// [`TokenRules::expecting_group_close`] when that rule has a non-empty close and it
/// stands at `pos` — regardless of the groups gate, since the expected close is what the
/// parser inside the group waits for — and otherwise the longest entry of the state's
/// [`PrefixTable`](super::PrefixTable), read as an opener when the string is both an open
/// and a close delimiter.
///
/// `None` when nothing matches, when the groups feature is absent (there is no table),
/// or when it is disabled (the table is empty).
///
/// # Panics
///
/// A `pos` that is out of bounds for `content` or not on a `char` boundary is a
/// caller-contract violation and panics, in all builds — one of the crate's few
/// deliberate panics (see the [Panics list](techy::guide::panics)).
pub fn scan_group_delimiter<'r, L: Lang>(
    content: &str,
    pos: usize,
    state: &'r ParsingState<L>,
) -> Option<GroupDelimiterMatch<'r, L>> {
    check_pos(content, pos);
    let rules = state.rules();
    let rest = &content[pos..];

    if let Some(expected) = rules.expecting_group_close() {
        if !expected.close.is_empty() && rest.starts_with(expected.close.as_str()) {
            let span = Span::new(pos, pos + expected.close.len());
            return Some(GroupDelimiterMatch::Close { span, rule: expected });
        }
    }

    // `None` when the language declares the groups feature absent — no table
    // exists, and no delimiter can match.
    let entry = state.prefix_table()?.match_at(rest)?;
    let span = Span::new(pos, pos + entry.delim().len());
    Some(match (entry.open(), entry.close()) {
        (Some(rule), _) => GroupDelimiterMatch::Open { span, rule },
        (None, Some(rule)) => GroupDelimiterMatch::Close { span, rule },
        (None, None) => unreachable!("prefix table entries always carry a direction"),
    })
}

/// The first command rule (in [`TokenRules::command_rules`] order) whose escape character
/// stands at `pos` — the rule [`scan_command`] then reads the command under.
///
/// `None` at the end of the content, when the commands feature is absent or disabled, or
/// when no rule's escape character stands there.
///
/// # Panics
///
/// A `pos` that is out of bounds for `content` or not on a `char` boundary is a
/// caller-contract violation and panics, in all builds — one of the crate's few
/// deliberate panics (see the [Panics list](techy::guide::panics)).
pub fn command_rule_at<'r, L: Lang>(
    content: &str,
    pos: usize,
    rules: &'r TokenRules<L>,
) -> Option<&'r Arc<CommandRule>> {
    check_pos(content, pos);
    if !<L::Features as LangFeatures>::Commands::PRESENT || !rules.commands_enabled() {
        return None;
    }
    let c = content[pos..].chars().next()?;
    rules.command_rules().iter().find(|r| c == r.escape_char)
}

/// A command read at a position under one [`CommandRule`], as [`scan_command`] answers.
///
/// The parts are coherent by construction: `span` starts at the escape character and ends
/// where `post_space` ends, `name` runs from just past the escape character to where
/// `post_space` starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMatch {
    /// The escape character of the rule the command was read under — the character at the
    /// scanned position.
    pub escape_char: char,
    /// The whole command: from the escape character through the post-space.
    pub span: Span,
    /// The name — what follows the escape character: a run of the rule's name
    /// characters, or a single character when the first one is not a name character.
    pub name: Span,
    /// The syntactic whitespace after a multi-character name ([`skip_whitespace`] from
    /// the name's end, so it never crosses a paragraph break); empty after a
    /// single-character name.
    pub post_space: Span,
}

/// Read the command whose escape character (`rule.escape_char`) stands at `pos`.
///
/// The name is a greedy run of the rule's name characters, or a single character when the
/// first one is not a name character. Only a multi-character name consumes its following
/// whitespace as post-space — `\&` and friends do not (pylatexenc behavior).
///
/// `Err(EndOfStreamAfterEscape)` when the escape character is the last character of the
/// content: there is no name to read. That is the condition the standard reader reports
/// with a [`Char`](super::TokenKind::Char) placeholder holding the escape character over
/// `pos..content.len()`, resuming at `content.len()`.
///
/// # Panics
///
/// A `pos` that is out of bounds for `content` or not on a `char` boundary is a
/// caller-contract violation and panics, in all builds — one of the crate's few
/// deliberate panics (see the [Panics list](techy::guide::panics)). So is a `rule` whose
/// escape character does not stand at `pos`: reading a name under a rule that did not
/// match would slice the content at an offset that is not a character boundary, and
/// panic there instead, less clearly.
pub fn scan_command<L: Lang>(
    content: &str,
    pos: usize,
    rules: &TokenRules<L>,
    rule: &CommandRule,
) -> Result<CommandMatch, EndOfStreamAfterEscape> {
    check_pos(content, pos);
    assert!(
        content[pos..].starts_with(rule.escape_char),
        "the rule's escape character {:?} does not stand at pos {}",
        rule.escape_char,
        pos
    );
    let s = content;
    let name_start = pos + rule.escape_char.len_utf8();

    if name_start >= s.len() {
        return Err(EndOfStreamAfterEscape::new(rule.escape_char));
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

    // The name is what lies between the escape character and the post-space.
    Ok(CommandMatch {
        escape_char: rule.escape_char,
        span: Span::new(pos, post_space.end()),
        name: Span::new(name_start, name_end),
        post_space,
    })
}

/// A whole comment read at a position, as [`scan_comment`] answers.
///
/// The parts are coherent by construction: `span` starts where `start` starts and ends
/// where `post_space` ends, `content` runs from the end of `start` to the beginning of
/// `post_space`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentMatch {
    /// The whole comment: from the start delimiter through the post-space.
    pub span: Span,
    /// The matched start delimiter — a leading sub-range of `span`.
    pub start: Span,
    /// The comment text, up to but excluding the terminating newline (or to the end of
    /// the content, when the comment is not terminated).
    pub content: Span,
    /// The terminating newline plus the following indentation, as [`skip_whitespace`]
    /// reads it — empty when that whitespace forms a paragraph break, which then
    /// surfaces on its own.
    pub post_space: Span,
}

/// The comment starting at `pos` when a comment-start delimiter matches there: the
/// longest non-empty one across [`TokenRules::comment_rules`].
///
/// `None` when none does, or when the comments feature is absent or disabled. `'\n'` is
/// the sole line terminator; `'\r'` is ordinary content.
///
/// # Panics
///
/// A `pos` that is out of bounds for `content` or not on a `char` boundary is a
/// caller-contract violation and panics, in all builds — one of the crate's few
/// deliberate panics (see the [Panics list](techy::guide::panics)).
pub fn scan_comment<L: Lang>(
    content: &str,
    pos: usize,
    rules: &TokenRules<L>,
) -> Option<CommentMatch> {
    check_pos(content, pos);
    if !<L::Features as LangFeatures>::Comments::PRESENT || !rules.comments_enabled() {
        return None;
    }
    let s = content;
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

    Some(CommentMatch {
        span: Span::new(pos, post_space.end()),
        start: Span::new(pos, content_start),
        content: Span::new(content_start, content_end),
        post_space,
    })
}

/// The specials step of the standard reader at `pos`: ask
/// [`Lang::scan_specials`](crate::state::Lang::scan_specials) whether a
/// callable-triggering character sequence starts there, after the state's
/// [`TriggerChars`](super::TriggerChars) filter says a trigger could.
///
/// `Ok(None)` when the specials feature is absent, at the end of the content, when the
/// character at `pos` is not in the filter (no filter at all, or
/// [`may_start`](super::TriggerChars::may_start) `false` — which is also how a disabled
/// specials gate reads, since the gate is applied when the filter is derived), or when
/// the hook answers no match. `Ok(Some(m))` for a match whose `m.end` is what the
/// [`SpecialsMatch::end`](super::SpecialsMatch::end) documentation requires (`pos < end
/// <= content.len()`, on a `char` boundary).
///
/// `Err(e)` for a hook failure: `e` is the hook's own error when the span it reported
/// lies within the content on `char` boundaries. Otherwise — and likewise for an `m.end`
/// the `SpecialsMatch` documentation rules out — `e` is a
/// [`SpecialsScanError`] whose `kind` is an implementation error naming the violation and
/// whose `span` is `Span::empty(pos)`. No failure of this step can carry a recovery: the
/// hook knows neither the reader's token type nor its stream positions, so the standard
/// reader reports every one of them as `TokenError::new(e.kind, SourceSpan::new(source,
/// e.span), None)`.
///
/// # Panics
///
/// A `pos` that is out of bounds for `content` or not on a `char` boundary is a
/// caller-contract violation and panics, in all builds — one of the crate's few
/// deliberate panics (see the [Panics list](techy::guide::panics)).
pub fn scan_specials_trigger<L: Lang>(
    content: &str,
    pos: usize,
    state: &ParsingState<L>,
) -> Result<Option<SpecialsMatch<L>>, SpecialsScanError> {
    check_pos(content, pos);
    if !<L::Features as LangFeatures>::Specials::PRESENT {
        return Ok(None);
    }
    let Some(c) = content[pos..].chars().next() else {
        return Ok(None);
    };
    if !state.trigger_chars().is_some_and(|trigger_chars| trigger_chars.may_start(c)) {
        return Ok(None);
    }
    let scanned = L::scan_specials(state, content, pos)
        .map_err(|error| checked_scan_error(error, content, pos))?;
    let Some(m) = scanned else {
        return Ok(None);
    };
    // A malformed `end` from the hook would yield a zero-width token (the
    // dispatch loop would never advance) or a span that panics when
    // sliced. The hook is outer-layer code, so the contract is validated,
    // not debug-asserted ([§dd-dr:panic-policy]); no recovery — an
    // implementation bug aborts even under tolerant recovery.
    if !(m.end > pos && m.end <= content.len() && content.is_char_boundary(m.end)) {
        return Err(SpecialsScanError {
            kind: TokenErrorKind::Custom(Box::new(ImplementationError::new(format!(
                "Lang::scan_specials returned an invalid match end \
                 {} for a match at {} (content length {})",
                m.end,
                pos,
                content.len()
            )))),
            span: Span::empty(pos),
        });
    }
    Ok(Some(m))
}

/// A `Lang::scan_specials` failure with its span validated against the scanned content.
///
/// The hook is outer-layer code, so its span is *validated*, not trusted: a span out of
/// the content's bounds or cutting a character would make `SourceSpan::new` assert once
/// the reader qualifies it with a source. Such a span is itself a contract violation,
/// reported the way every other extension-contract violation in this file is — an
/// implementation error anchored where the scan was asked for ([§dd-dr:panic-policy]).
fn checked_scan_error(
    error: SpecialsScanError,
    content: &str,
    pos: usize,
) -> SpecialsScanError {
    let span = error.span;
    let valid = span.end() <= content.len()
        && content.is_char_boundary(span.start())
        && content.is_char_boundary(span.end());
    if !valid {
        return SpecialsScanError {
            kind: TokenErrorKind::Custom(Box::new(ImplementationError::new(format!(
                "Lang::scan_specials reported an error at an invalid span {}..{} \
                 for a scan at {} (content length {})",
                span.start(),
                span.end(),
                pos,
                content.len()
            )))),
            span: Span::empty(pos),
        };
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DiagnosticInfo;
    use crate::scopes::ScopeStack;
    use crate::spec::CallableSpec;
    use crate::state::{StateData, TrivialLang};
    use crate::token::{
        CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRules,
        ParagraphRules, SpecialsRules, TriggerChars, WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    // Group classes of the hardcoded latexlike-flavored test rules below (the test langs
    // use the `TrivialLang`-style `u32` class space). Distinct per rule, so a test can
    // look a rule up by class.
    const BRACES: u32 = 0;
    const BRACKETS: u32 = 1;
    const MATH_INLINE: u32 = 2;
    const MATH_DISPLAY: u32 = 3;
    const MATH_INLINE_PAREN: u32 = 4;
    const MATH_DISPLAY_BRACKET: u32 = 5;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestLang;
    impl TrivialLang for TestLang {}

    /// Hardcoded LaTeX-flavored rules — the same set the reader's own tests use, so a
    /// helper's answer here is comparable with the token the reader builds from it.
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

    // --- skip_whitespace ---------------------------------------------------------------

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

    // --- fixtures for the state-taking helpers -----------------------------------------

    fn sp(start: usize, end: usize) -> Span {
        Span::new(start, end)
    }

    fn state(rules: TokenRules<TestLang>) -> Arc<ParsingState<TestLang>> {
        Arc::new(ParsingState::new(StateData {
            rules,
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    /// The `latex_rules` group rule of the given class (unique per rule in these tests).
    fn rule_of(group_type: u32) -> Arc<GroupRule<TestLang>> {
        latex_rules::<TestLang>()
            .groups
            .rules
            .into_iter()
            .find(|g| g.group_type == group_type)
            .expect("class present in latex_rules")
    }

    /// A state expecting the given rule's close delimiter, as the group parser sets up
    /// when it enters an ambiguously-delimited group.
    fn expecting_close(group_type: u32) -> Arc<ParsingState<TestLang>> {
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.groups.expecting_close = Some(rule_of(group_type));
        state(rules)
    }

    /// The single command rule of `latex_rules`: escape character `\`, ASCII letters as
    /// name characters.
    fn backslash() -> Arc<CommandRule> {
        latex_rules::<TestLang>().commands.rules[0].clone()
    }

    // --- scan_paragraph_break ----------------------------------------------------------

    #[test]
    fn paragraph_break_runs_through_the_last_newline() {
        let rules: TokenRules<TestLang> = latex_rules();
        // Two newlines with whitespace between them: the break ends at the last one, and
        // the indentation after it is left for the next token's pre-space.
        assert_eq!(scan_paragraph_break("a\n \n  b", 1, &rules), Some(sp(1, 4)));
        // Three newlines: still through the last one.
        assert_eq!(scan_paragraph_break("\n\n\n x", 0, &rules), Some(sp(0, 3)));
    }

    #[test]
    fn paragraph_break_needs_a_run_of_two_newlines_at_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        // A lone newline is consumable whitespace, not a break.
        assert_eq!(scan_paragraph_break("a\n b", 1, &rules), None);
        // The run must begin at `pos` with a newline — which is what the offset
        // `skip_whitespace` answers guarantees; here it does not.
        assert_eq!(scan_paragraph_break(" \n\n", 0, &rules), None);
        // No whitespace at all, and the end of the content.
        assert_eq!(scan_paragraph_break("ab", 1, &rules), None);
        assert_eq!(scan_paragraph_break("ab", 2, &rules), None);
    }

    #[test]
    fn paragraph_break_off_when_the_rules_say_so() {
        // The paragraphs gate off.
        let mut no_par: TokenRules<TestLang> = latex_rules();
        no_par.paragraphs.enabled = false;
        assert_eq!(scan_paragraph_break("\n\nx", 0, &no_par), None);
        // Whitespace handling off is the other half of the condition.
        let mut no_ws: TokenRules<TestLang> = latex_rules();
        no_ws.whitespace.enabled = false;
        assert_eq!(scan_paragraph_break("\n\nx", 0, &no_ws), None);
        // `'\n'` not being one of the whitespace characters has the same effect.
        let mut no_nl: TokenRules<TestLang> = latex_rules();
        no_nl.whitespace.chars = " \t".into();
        assert_eq!(scan_paragraph_break("\n\nx", 0, &no_nl), None);
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_paragraph_break_panics_on_an_out_of_bounds_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = scan_paragraph_break("ab", 5, &rules);
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_paragraph_break_panics_on_a_mid_char_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = scan_paragraph_break("é!", 1, &rules);
    }

    // --- scan_group_delimiter ----------------------------------------------------------

    #[test]
    fn group_delimiter_reads_both_directions_and_the_longest_match() {
        let st = state(latex_rules());
        let braces = rule_of(BRACES);
        assert_eq!(
            scan_group_delimiter("{a}", 0, &st),
            Some(GroupDelimiterMatch::Open { span: sp(0, 1), rule: &braces }),
        );
        // `}` is a close delimiter only, and the match carries its rule all the same.
        assert_eq!(
            scan_group_delimiter("{a}", 2, &st),
            Some(GroupDelimiterMatch::Close { span: sp(2, 3), rule: &braces }),
        );
        // Longest first: `$$` beats `$`.
        let display = rule_of(MATH_DISPLAY);
        assert_eq!(
            scan_group_delimiter("$$x", 0, &st),
            Some(GroupDelimiterMatch::Open { span: sp(0, 2), rule: &display }),
        );
        // Nothing there.
        assert_eq!(scan_group_delimiter("ax", 0, &st), None);
        assert_eq!(scan_group_delimiter("ax", 2, &st), None);
    }

    #[test]
    fn group_delimiter_reads_an_ambiguous_string_as_an_opener() {
        let st = state(latex_rules());
        // `$` opens and closes the same rule; with no expected close, it opens.
        let inline = rule_of(MATH_INLINE);
        assert_eq!(
            scan_group_delimiter("$x$", 0, &st),
            Some(GroupDelimiterMatch::Open { span: sp(0, 1), rule: &inline }),
        );
    }

    #[test]
    fn group_delimiter_gives_the_expected_close_precedence() {
        let st = expecting_close(MATH_INLINE);
        let inline = rule_of(MATH_INLINE);
        // The parser inside the group waits for `$`, so `$` closes rather than opens ...
        assert_eq!(
            scan_group_delimiter("$x", 0, &st),
            Some(GroupDelimiterMatch::Close { span: sp(0, 1), rule: &inline }),
        );
        // ... and it wins even over the longer table entry `$$`.
        assert_eq!(
            scan_group_delimiter("$$x", 0, &st),
            Some(GroupDelimiterMatch::Close { span: sp(0, 1), rule: &inline }),
        );
    }

    #[test]
    fn group_delimiter_with_groups_disabled_still_answers_the_expected_close() {
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.groups.enabled = false;
        rules.groups.expecting_close = Some(rule_of(BRACES));
        let st = state(rules);
        // The gate empties the table, so no rule's delimiter matches ...
        assert_eq!(scan_group_delimiter("{a}", 0, &st), None);
        // ... but the close the parser inside the group waits for is not gated.
        let braces = rule_of(BRACES);
        assert_eq!(
            scan_group_delimiter("}a", 0, &st),
            Some(GroupDelimiterMatch::Close { span: sp(0, 1), rule: &braces }),
        );
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_group_delimiter_panics_on_an_out_of_bounds_pos() {
        let st = state(latex_rules());
        let _ = scan_group_delimiter("ab", 5, &st);
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_group_delimiter_panics_on_a_mid_char_pos() {
        let st = state(latex_rules());
        let _ = scan_group_delimiter("é!", 1, &st);
    }

    // --- command_rule_at ---------------------------------------------------------------

    #[test]
    fn command_rule_at_answers_the_rule_whose_escape_char_stands_there() {
        let rules: TokenRules<TestLang> = latex_rules();
        assert_eq!(command_rule_at(r"a\foo", 1, &rules), Some(&backslash()));
        // Not an escape character, and the end of the content.
        assert_eq!(command_rule_at(r"a\foo", 0, &rules), None);
        assert_eq!(command_rule_at("a", 1, &rules), None);
    }

    #[test]
    fn command_rule_at_answers_the_first_rule_in_list_order() {
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.commands.rules = vec![
            Arc::new(CommandRule { escape_char: '@', name_chars: "a".into() }),
            Arc::new(CommandRule { escape_char: '@', name_chars: "b".into() }),
        ];
        let found = command_rule_at("@a", 0, &rules).expect("the escape char stands there");
        assert_eq!(&*found.name_chars, "a");
    }

    #[test]
    fn command_rule_at_off_when_the_rules_say_so() {
        let mut off: TokenRules<TestLang> = latex_rules();
        off.commands.enabled = false;
        assert_eq!(command_rule_at(r"\foo", 0, &off), None);
        // Empty data is the other spelling of "off".
        let mut empty: TokenRules<TestLang> = latex_rules();
        empty.commands.rules = Vec::new();
        assert_eq!(command_rule_at(r"\foo", 0, &empty), None);
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn command_rule_at_panics_on_an_out_of_bounds_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = command_rule_at("ab", 5, &rules);
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn command_rule_at_panics_on_a_mid_char_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = command_rule_at("é!", 1, &rules);
    }

    // --- scan_command ------------------------------------------------------------------

    #[test]
    fn command_with_a_name_takes_its_post_space() {
        let rules: TokenRules<TestLang> = latex_rules();
        assert_eq!(
            scan_command(r"\foo  bar", 0, &rules, &backslash()),
            Ok(CommandMatch {
                escape_char: '\\',
                span: sp(0, 6),
                name: sp(1, 4),
                post_space: sp(4, 6),
            }),
        );
    }

    #[test]
    fn command_of_a_single_character_takes_no_post_space() {
        let rules: TokenRules<TestLang> = latex_rules();
        // `&` is not a name character, so the name is that one character and the
        // whitespace after it stays content (pylatexenc behavior).
        assert_eq!(
            scan_command(r"\& x", 0, &rules, &backslash()),
            Ok(CommandMatch {
                escape_char: '\\',
                span: sp(0, 2),
                name: sp(1, 2),
                post_space: Span::empty(2),
            }),
        );
    }

    #[test]
    fn command_post_space_stops_before_a_paragraph_break() {
        let rules: TokenRules<TestLang> = latex_rules();
        // `skip_whitespace`'s multi-newline rule: the post-space ends before the first
        // newline of the break, which then surfaces on its own.
        let m = scan_command("\\foo \n\nbar", 0, &rules, &backslash()).expect("a command");
        assert_eq!(m.post_space, sp(4, 5));
        assert_eq!(m.span, sp(0, 5));
        assert_eq!(scan_paragraph_break("\\foo \n\nbar", 5, &rules), Some(sp(5, 7)));
    }

    #[test]
    fn command_at_a_dangling_escape_character_is_an_end_of_stream_condition() {
        let rules: TokenRules<TestLang> = latex_rules();
        assert_eq!(
            scan_command(r"a\", 1, &rules, &backslash()),
            Err(EndOfStreamAfterEscape::new('\\')),
        );
    }

    #[test]
    fn command_with_a_multi_byte_escape_character() {
        let mut rules: TokenRules<TestLang> = latex_rules();
        let rule = Arc::new(CommandRule { escape_char: '§', name_chars: "abc".into() });
        rules.commands.rules = vec![rule.clone()];
        // '§' occupies two bytes, so the name starts at 2 and every span accounts for it.
        assert_eq!(
            scan_command("§ab d", 0, &rules, &rule),
            Ok(CommandMatch {
                escape_char: '§',
                span: sp(0, 5),
                name: sp(2, 4),
                post_space: sp(4, 5),
            }),
        );
        // A dangling multi-byte escape character is the same condition as a dangling
        // one-byte one.
        assert_eq!(scan_command("§", 0, &rules, &rule), Err(EndOfStreamAfterEscape::new('§')));
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_command_panics_on_an_out_of_bounds_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = scan_command(r"\a", 5, &rules, &backslash());
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_command_panics_on_a_mid_char_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = scan_command("é!", 1, &rules, &backslash());
    }

    /// Reading a name under a rule that did not match is the other half of this
    /// function's precondition: it would slice the content at an offset that is not a
    /// character boundary, and panic there instead, less clearly.
    #[test]
    #[should_panic(expected = "does not stand at pos")]
    fn scan_command_panics_when_the_rules_escape_char_is_not_at_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = scan_command("abc", 0, &rules, &backslash());
    }

    // --- scan_comment ------------------------------------------------------------------

    #[test]
    fn comment_takes_the_longest_start_delimiter() {
        let mut rules: TokenRules<TestLang> = latex_rules();
        rules.comments.rules = vec![
            Arc::new(CommentRule { start: "%".into() }),
            Arc::new(CommentRule { start: "%%".into() }),
            // An empty delimiter is ignored, rather than matching everywhere.
            Arc::new(CommentRule { start: "".into() }),
        ];
        assert_eq!(
            scan_comment("%%x\ny", 0, &rules),
            Some(CommentMatch {
                span: sp(0, 4),
                start: sp(0, 2),
                content: sp(2, 3),
                post_space: sp(3, 4),
            }),
        );
    }

    #[test]
    fn comment_post_space_is_the_newline_and_the_next_lines_indentation() {
        let rules: TokenRules<TestLang> = latex_rules();
        assert_eq!(
            scan_comment("a% note\n   b", 1, &rules),
            Some(CommentMatch {
                span: sp(1, 11),
                start: sp(1, 2),
                content: sp(2, 7),
                post_space: sp(7, 11),
            }),
        );
    }

    #[test]
    fn comment_without_a_terminating_newline_runs_to_the_end() {
        let rules: TokenRules<TestLang> = latex_rules();
        assert_eq!(
            scan_comment("%tail", 0, &rules),
            Some(CommentMatch {
                span: sp(0, 5),
                start: sp(0, 1),
                content: sp(1, 5),
                post_space: Span::empty(5),
            }),
        );
    }

    #[test]
    fn comment_keeps_a_carriage_return_in_its_content() {
        let rules: TokenRules<TestLang> = latex_rules();
        // '\n' is the sole line terminator; '\r' is ordinary content, even though these
        // rules do count it as a whitespace character.
        let m = scan_comment("%a\r\nb", 0, &rules).expect("a comment");
        assert_eq!(m.content, sp(1, 3));
        assert_eq!(m.post_space, sp(3, 4));
    }

    #[test]
    fn comment_takes_no_post_space_before_a_paragraph_break() {
        let rules: TokenRules<TestLang> = latex_rules();
        // The terminating newline opens a `\n\s*\n` sequence, so it stays behind for the
        // break to surface on its own ...
        let m = scan_comment("%a\n\nb", 0, &rules).expect("a comment");
        assert_eq!(m.content, sp(1, 2));
        assert_eq!(m.post_space, Span::empty(2));
        assert_eq!(m.span, sp(0, 2));
        // ... which is exactly what the next scan at the comment's end answers.
        assert_eq!(scan_paragraph_break("%a\n\nb", 2, &rules), Some(sp(2, 4)));
    }

    #[test]
    fn comment_none_without_a_delimiter_and_when_the_rules_say_so() {
        let rules: TokenRules<TestLang> = latex_rules();
        assert_eq!(scan_comment("a%x", 0, &rules), None);
        assert_eq!(scan_comment("a%x", 3, &rules), None);
        let mut off: TokenRules<TestLang> = latex_rules();
        off.comments.enabled = false;
        assert_eq!(scan_comment("%x", 0, &off), None);
        // Empty data is the other spelling of "off".
        let mut empty: TokenRules<TestLang> = latex_rules();
        empty.comments.rules = Vec::new();
        assert_eq!(scan_comment("%x", 0, &empty), None);
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_comment_panics_on_an_out_of_bounds_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = scan_comment("ab", 5, &rules);
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_comment_panics_on_a_mid_char_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = scan_comment("é!", 1, &rules);
    }

    // --- scan_specials_trigger ---------------------------------------------------------

    #[derive(Debug)]
    struct StubSpec;
    impl crate::serialize::SerializableObject<HookLang> for StubSpec {}
    impl CallableSpec<HookLang> for StubSpec {}

    /// A language whose specials hook produces, in one place, every answer this helper
    /// must handle — selected by the character at the scanned position, so that one
    /// language serves all the cases: `~`, `~~`, `---` and `&` match; `-` alone is a
    /// filter character the hook then declines; `!` answers a zero-width match, which
    /// the [`SpecialsMatch::end`](crate::token::SpecialsMatch::end) documentation rules
    /// out; `?` reports a condition at a span inside the content; `;` and `:` report one
    /// at a span the content does not have.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct HookLang;
    impl Lang for HookLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~&-!?;:".into())
        }

        fn scan_specials(
            _state: &ParsingState<Self>,
            content: &str,
            pos: usize,
        ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
            let rest = &content[pos..];
            // Longest-first over a hardcoded trigger list — a stand-in for a preset
            // dispatching to its libraries.
            for trigger in ["---", "~~", "~", "&"] {
                if rest.starts_with(trigger) {
                    return Ok(Some(SpecialsMatch {
                        end: pos + trigger.len(),
                        callable_type: 7,
                        spec: Arc::new(StubSpec),
                    }));
                }
            }
            if rest.starts_with('!') {
                // Zero-width: a token built from this would never advance the parse.
                return Ok(Some(SpecialsMatch {
                    end: pos,
                    callable_type: 7,
                    spec: Arc::new(StubSpec),
                }));
            }
            let bad_span = match rest.chars().next() {
                Some('?') => Some(Span::new(pos, pos + 1)), // a span the content has
                Some(';') => Some(Span::new(content.len() + 1, content.len() + 2)),
                Some(':') => Some(Span::new(1, 2)), // cuts a leading two-byte character
                _ => None,
            };
            match bad_span {
                Some(span) => Err(SpecialsScanError {
                    kind: TokenErrorKind::Custom(Box::new(ImplementationError::new(
                        String::from("the scan declined loudly"),
                    ))),
                    span,
                }),
                None => Ok(None),
            }
        }

        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &Arc<ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    fn hook_state(rules: TokenRules<HookLang>) -> Arc<ParsingState<HookLang>> {
        Arc::new(ParsingState::new(StateData {
            rules,
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    /// A language whose hook must never run: the filter admits `~` only, so a scan that
    /// reached the hook at any other character fails the test loudly.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct UnconsultedHookLang;
    impl Lang for UnconsultedHookLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
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
            panic!("scan_specials consulted where the filter admits no trigger")
        }

        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &Arc<ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    fn unconsulted_state(
        rules: TokenRules<UnconsultedHookLang>,
    ) -> Arc<ParsingState<UnconsultedHookLang>> {
        Arc::new(ParsingState::new(StateData {
            rules,
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    #[test]
    fn specials_trigger_answers_the_hooks_match() {
        let st = hook_state(latex_rules());
        let m = scan_specials_trigger("a---b", 1, &st).expect("no failure").expect("a match");
        // The match is the hook's own, unchanged: its end is where the trigger ends.
        assert_eq!(m.end, 4);
        assert_eq!(m.callable_type, 7);
    }

    #[test]
    fn specials_trigger_answers_none_when_no_trigger_is_there() {
        let st = hook_state(latex_rules());
        // A filter character the hook then declines to match on (`-` without `--`).
        assert!(scan_specials_trigger("-x", 0, &st).expect("no failure").is_none());
        // A character the filter excludes.
        assert!(scan_specials_trigger("x~", 0, &st).expect("no failure").is_none());
        // The end of the content: no character to offer the hook.
        assert!(scan_specials_trigger("x", 1, &st).expect("no failure").is_none());
    }

    #[test]
    fn specials_trigger_does_not_consult_the_hook_off_the_filter() {
        let st = unconsulted_state(latex_rules());
        // The hook panics if consulted, so reaching the assertions is the assertion.
        assert!(scan_specials_trigger("x~", 0, &st).expect("no failure").is_none());
        assert!(scan_specials_trigger("x~", 2, &st).expect("no failure").is_none());
    }

    #[test]
    fn specials_trigger_is_off_when_the_specials_gate_is_off() {
        // The gate is applied where the filter is derived, so a disabled specials block
        // leaves the empty filter and the hook is out of reach even at `~`.
        let mut rules: TokenRules<UnconsultedHookLang> = latex_rules();
        rules.specials.enabled = false;
        let st = unconsulted_state(rules);
        assert!(scan_specials_trigger("~x", 0, &st).expect("no failure").is_none());
    }

    #[test]
    fn specials_trigger_rejects_a_match_end_the_documentation_rules_out() {
        let st = hook_state(latex_rules());
        let err = scan_specials_trigger("a!b", 1, &st).expect_err("a rejected match");
        // Anchored where the scan was asked for, and named as an implementation error.
        assert_eq!(err.span, Span::empty(1));
        match &err.kind {
            TokenErrorKind::Custom(data) => {
                assert_eq!(data.identifier(), ImplementationError::IDENTIFIER);
                assert!(data.to_string().contains("invalid match end"), "{}", data);
            }
            other => panic!("expected a Custom implementation error, got {:?}", other),
        }
    }

    #[test]
    fn specials_trigger_passes_a_hook_error_with_a_valid_span_through() {
        let st = hook_state(latex_rules());
        let err = scan_specials_trigger("a?b", 1, &st).expect_err("the hook's failure");
        // The hook's own span, untouched: it lies within the content on char boundaries.
        assert_eq!(err.span, sp(1, 2));
        assert!(err.to_string().contains("declined loudly"), "{err}");
    }

    #[test]
    fn specials_trigger_anchors_a_hook_error_reported_at_an_invalid_span() {
        let st = hook_state(latex_rules());
        //                      'é' occupies bytes 0..2, so the span 1..2 cuts it in half
        for (content, pos, half) in [(";x", 0, "out of bounds"), ("é:", 2, "mid-character")] {
            let err = scan_specials_trigger(content, pos, &st).expect_err("a rejected span");
            // The hook is outer-layer code: a span the content does not have is itself a
            // violation, reported at the scanned position instead of the hook's wording.
            assert_eq!(err.span, Span::empty(pos), "{half}");
            match &err.kind {
                TokenErrorKind::Custom(data) => {
                    assert_eq!(data.identifier(), ImplementationError::IDENTIFIER, "{half}");
                    assert!(data.to_string().contains("invalid span"), "{half}: {}", data);
                }
                other => panic!("{half}: expected a Custom error, got {:?}", other),
            }
        }
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_specials_trigger_panics_on_an_out_of_bounds_pos() {
        let st = hook_state(latex_rules());
        let _ = scan_specials_trigger("ab", 5, &st);
    }

    #[test]
    #[should_panic(expected = "char boundary")]
    fn scan_specials_trigger_panics_on_a_mid_char_pos() {
        let st = hook_state(latex_rules());
        let _ = scan_specials_trigger("é!", 1, &st);
    }
}
