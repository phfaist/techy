//! Tokenization rules — the plain data that drives [`StdTokenReader`](super::StdTokenReader).
//!
//! `TokenRules` is defined here in the token topic and *stored* in the parsing state
//! ([`StateData<L>`](crate::state::StateData)). Everything that can vary during a parse —
//! delimiters, escape characters, enabled features — is a plain value in these structs,
//! changed only through reified state deltas at the transition choke point
//! (ARCHITECTURE.md §state). There are no privileged language concepts here: no default
//! `\`, `{}`, `%`, or `$` — the familiar LaTeX values are supplied by the latexlike preset
//! (Phase 7), which is also why none of these types implement `Default`.
//!
//! Group *classes* are the language's business: [`Lang::GroupTypeId`] is a closed
//! per-language classification (typically an enum — the latexlike preset: content vs.
//! math groups; revised July 2026), fully detached from delimiter spellings. Which
//! *delimiter pairs* exist, and which class each belongs to, is runtime data — the
//! [`GroupRule`] values here. Any construct parser may mint a new rule mid-parse via a
//! state delta (an optional-argument parser momentarily declaring `[`…`]` group
//! delimiters, a custom spec declaring `<`…`>`).
//!
//! The one deliberate omission: **specials trigger strings are not enumerated here.**
//! Their recognition is delegated to the language via `Lang::scan_specials` (see
//! [`specials`](super::SpecialsMatch)), because trigger sets can be large and
//! library-driven. Everything else — commands, comments, groups, whitespace — is rules
//! data.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::state::Lang;

/// One group syntax usable in the current parsing state: a delimiter pair and the
/// language-native class ([`Lang::GroupTypeId`]) of the groups it delimits.
///
/// Open and close delimiters are arbitrary non-empty strings; several rules may share
/// delimiter strings (`$…$` and `$$…$$`), including the same string for open and close.
/// The [`PrefixTable`](super::PrefixTable) resolves the resulting matching ambiguities.
///
/// Rules are held behind `Arc` in [`TokenRules::groups`]: the tokenizer's resolution of
/// *which* rule matched travels with the emitted
/// [`GroupOpen`](super::TokenKind::GroupOpen) token, so parsers never re-derive it.
pub struct GroupRule<L: Lang> {
    /// The class of the groups this rule delimits (e.g. the latexlike preset's
    /// content-group vs. math-group distinction) — detached from the spellings below.
    pub group_type: L::GroupTypeId,
    /// Opening delimiter (e.g. `{`).
    pub open: String,
    /// Closing delimiter (e.g. `}`).
    pub close: String,
}

/// Whitespace-handling rules. Absent (`None` in [`TokenRules::whitespace`]) = whitespace
/// handling disabled: whitespace characters are ordinary content characters, `pre_space` is
/// always empty, and paragraph breaks are never detected (character-level access mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhitespaceRules {
    /// The characters treated as whitespace (e.g. `" \t\n\r"`).
    pub chars: String,
}

/// One command syntax: an escape character introducing a named invocation (`\textbf`,
/// `\&`). Several rules may coexist (distinct escape characters; earlier entries win on
/// conflict); an empty [`TokenRules::commands`] disables command recognition entirely.
///
/// "Command" is the token-level term (TeX lineage: control sequence). It is deliberately
/// *not* "macro": at the token level `\begin` is a command exactly like `\foobar` — which
/// names are macros, environments, or anything else is decided at parse time by the preset
/// (terminology stack: command → callable → macro/environment; NAMING_STRATEGY.md).
///
/// A future syntax-kind extension (e.g. `@MARKER@`-style commands without an escape
/// character) would grow an enum inside this struct; flat escape-char form only, for now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRule {
    /// The escape character introducing the command (`\` in LaTeX).
    pub escape_char: char,
    /// Characters that form multi-character command names. A name starts with any
    /// character; only if the first character is in this set does the name extend greedily
    /// over further characters of the set (`\textbf` vs. the single-character `\&`). Only
    /// multi-character (name-chars) commands consume post-space.
    pub name_chars: String,
}

/// One comment syntax: a start delimiter, with the comment running to the end of the line.
/// Several rules may coexist (longest matching start wins); an empty
/// [`TokenRules::comments`] disables comment recognition.
///
/// The terminator is implicitly `'\n'` (or end of input) — independent of
/// [`WhitespaceRules`], so comments work even with whitespace handling disabled. A future
/// extension may add per-rule terminators (block comments à la `/* … */`); end-of-line
/// only, for now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentRule {
    /// The comment-start delimiter string (`%` in LaTeX).
    pub start: String,
}

/// The complete data driving standard tokenization, stored in the parsing state.
///
/// [`StdTokenReader`](super::StdTokenReader) is driven by this data (plus the derived
/// [`PrefixTable`](super::PrefixTable) and the `Lang::scan_specials` hook); anyone needing
/// genuinely different tokenization *behavior* implements the
/// [`TokenReader`](super::TokenReader) trait instead.
///
/// Detection priority at a given position: paragraph break (within leading whitespace) →
/// group delimiters (expected close first, then longest match) → command escape characters
/// → comment starts → specials scan → forbidden-character check → single content
/// character.
pub struct TokenRules<L: Lang> {
    /// Whitespace handling; `None` disables it entirely.
    pub whitespace: Option<WhitespaceRules>,
    /// Whether a whitespace run containing two or more newlines forms a paragraph break.
    /// Gates both `ParagraphBreak` tokens and the no-double-newline rule of whitespace
    /// skipping (pre-space and post-space never consume a newline belonging to a
    /// `\n\s*\n` sequence). Only meaningful when whitespace handling is enabled.
    pub double_newline_paragraphs: bool,
    /// The group delimiter rules recognizable here (`{…}`, `[…]`, `$…$`, `$$…$$`,
    /// `\(…\)`, … — all just delimiter pairs; math is not a core concept). On delimiter
    /// conflicts, earlier entries win (see [`PrefixTable`](super::PrefixTable)).
    pub groups: Vec<Arc<GroupRule<L>>>,
    /// Command syntaxes; empty = no command recognition.
    pub commands: Vec<CommandRule>,
    /// Comment syntaxes; empty = no comment recognition.
    pub comments: Vec<CommentRule>,
    /// Characters that may not appear as content; encountering one yields a recoverable
    /// [`TokenError`](super::TokenError).
    pub forbidden_chars: String,
    /// The group rule whose *close* delimiter takes precedence over all other delimiter
    /// matches — set (via a state delta) by the group construct parser upon entering a
    /// group whose delimiters are ambiguous. This is how `$…$` inside `$$…$$` resolves:
    /// inside a `$…$` group this field holds the `$…$` rule, so a following `$$`
    /// tokenizes as close-`$` (then open-`$`) rather than as a `$$` delimiter.
    /// Generalizes pylatexenc's `math_mode_delimiter` without privileging math.
    pub expecting_group_close: Option<Arc<GroupRule<L>>>,
}

// Manual impls: derives would demand `L: Clone`/`L: Debug`/`L: PartialEq` although only
// the `Lang::GroupTypeId` associated type (already bounded) is stored.

impl<L: Lang> Clone for GroupRule<L> {
    fn clone(&self) -> Self {
        GroupRule {
            group_type: self.group_type,
            open: self.open.clone(),
            close: self.close.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for GroupRule<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupRule")
            .field("group_type", &self.group_type)
            .field("open", &self.open)
            .field("close", &self.close)
            .finish()
    }
}

impl<L: Lang> PartialEq for GroupRule<L> {
    fn eq(&self, other: &Self) -> bool {
        self.group_type == other.group_type
            && self.open == other.open
            && self.close == other.close
    }
}

impl<L: Lang> Eq for GroupRule<L> {}

impl<L: Lang> Clone for TokenRules<L> {
    fn clone(&self) -> Self {
        TokenRules {
            whitespace: self.whitespace.clone(),
            double_newline_paragraphs: self.double_newline_paragraphs,
            groups: self.groups.clone(),
            commands: self.commands.clone(),
            comments: self.comments.clone(),
            forbidden_chars: self.forbidden_chars.clone(),
            expecting_group_close: self.expecting_group_close.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for TokenRules<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRules")
            .field("whitespace", &self.whitespace)
            .field("double_newline_paragraphs", &self.double_newline_paragraphs)
            .field("groups", &self.groups)
            .field("commands", &self.commands)
            .field("comments", &self.comments)
            .field("forbidden_chars", &self.forbidden_chars)
            .field("expecting_group_close", &self.expecting_group_close)
            .finish()
    }
}

impl<L: Lang> PartialEq for TokenRules<L> {
    fn eq(&self, other: &Self) -> bool {
        self.whitespace == other.whitespace
            && self.double_newline_paragraphs == other.double_newline_paragraphs
            && self.groups == other.groups
            && self.commands == other.commands
            && self.comments == other.comments
            && self.forbidden_chars == other.forbidden_chars
            && self.expecting_group_close == other.expecting_group_close
    }
}

impl<L: Lang> Eq for TokenRules<L> {}
