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
//! The one deliberate omission: **specials trigger strings are not enumerated here.**
//! Their recognition is delegated to the language via `Lang::scan_specials` (see
//! [`specials`](super::SpecialsMatch)), because trigger sets can be large and
//! library-driven. Everything else — commands, comments, groups, whitespace — is rules
//! data.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Identifier of a group *type* — a registered pairing of open/close delimiters
/// (`{…}`, `[…]`, `$…$`, …). Open registry per the naming rule (`…TypeId`, not `…Kind`):
/// group types are preset-/user-registered, not a closed core enum.
///
/// Interning machinery arrives with `Language<L>` (Phase 6); until then ids are constructed
/// directly.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupTypeId(u32);

impl GroupTypeId {
    /// Create a group type id from a raw index.
    pub const fn new(index: u32) -> GroupTypeId {
        GroupTypeId(index)
    }

    /// The raw index of this id.
    pub const fn index(&self) -> u32 {
        self.0
    }
}

impl fmt::Debug for GroupTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GroupTypeId({})", self.0)
    }
}

/// One group type usable in the current parsing state: its id and delimiter pair.
///
/// Open and close delimiters are arbitrary non-empty strings; several group types may share
/// delimiter strings (`$…$` and `$$…$$`), including the same string for open and close.
/// The [`PrefixTable`](super::PrefixTable) resolves the resulting matching ambiguities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupType {
    /// The group type's identity, recorded in `GroupOpen`/`GroupClose` tokens.
    pub id: GroupTypeId,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRules {
    /// Whitespace handling; `None` disables it entirely.
    pub whitespace: Option<WhitespaceRules>,
    /// Whether a whitespace run containing two or more newlines forms a paragraph break.
    /// Gates both `ParagraphBreak` tokens and the no-double-newline rule of whitespace
    /// skipping (pre-space and post-space never consume a newline belonging to a
    /// `\n\s*\n` sequence). Only meaningful when whitespace handling is enabled.
    pub double_newline_paragraphs: bool,
    /// The group types recognizable here (`{…}`, `[…]`, `$…$`, `$$…$$`, `\(…\)`, …
    /// — all just group types; math is not a core concept). On delimiter conflicts,
    /// earlier entries win (see [`PrefixTable`](super::PrefixTable)).
    pub group_types: Vec<GroupType>,
    /// Command syntaxes; empty = no command recognition.
    pub commands: Vec<CommandRule>,
    /// Comment syntaxes; empty = no comment recognition.
    pub comments: Vec<CommentRule>,
    /// Characters that may not appear as content; encountering one yields a recoverable
    /// [`TokenError`](super::TokenError).
    pub forbidden_chars: String,
    /// The group type whose *close* delimiter takes precedence over all other delimiter
    /// matches — set (via a state delta) by the group construct parser upon entering a
    /// group whose delimiters are ambiguous. This is how `$…$` inside `$$…$$` resolves:
    /// inside a `$…$` group this field names the `$…$` type, so a following `$$` tokenizes
    /// as close-`$` (then open-`$`) rather than as a `$$` delimiter. Generalizes
    /// pylatexenc's `math_mode_delimiter` without privileging math.
    pub expecting_group_close: Option<GroupTypeId>,
}
