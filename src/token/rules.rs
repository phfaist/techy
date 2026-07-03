//! Tokenization rules — the plain data that drives [`StdTokenReader`](super::StdTokenReader).
//!
//! `TokenRules` is S0 data *defined* here in the token topic and *stored* in the parsing
//! state (`StateData<L>`, Phase 3). Everything that can vary during a parse — delimiters,
//! escape chars, enabled features — is a plain value in these structs, changed only through
//! reified state deltas at the transition choke point (ARCHITECTURE.md §state). There are no
//! privileged language concepts here: no default `\`, `{}`, `%`, or `$` — the familiar LaTeX
//! values are supplied by the latexlike preset (Phase 7), which is also why none of these
//! types implement `Default`.

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

/// Macro-recognition rules. Absent = the escape character is an ordinary content character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroRules {
    /// The escape character introducing a macro invocation (`\` in LaTeX).
    pub escape_char: char,
    /// Characters that form multi-character macro names. A name starts with any character;
    /// only if the first character is in this set does the name extend greedily over further
    /// characters of the set (`\textbf` vs. the single-character `\&`).
    pub name_chars: String,
}

/// Comment-recognition rules. Absent = comment-start delimiters are ordinary content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentRules {
    /// Comment-start delimiter strings (`%` in LaTeX). Matched longest-first. The token
    /// covers only the delimiter — reading the comment's content to end-of-line is the
    /// comment construct parser's job (Phase 6), per the minimal-token principle.
    pub starts: Vec<String>,
}

/// The complete data driving standard tokenization, stored in the parsing state.
///
/// [`StdTokenReader`](super::StdTokenReader) is 100% driven by this data (plus the derived
/// [`PrefixTable`](super::PrefixTable)); anyone needing genuinely different tokenization
/// *behavior* implements the `TokenReader` trait instead (Phase 3).
///
/// Detection priority at a given position: paragraph break (within leading whitespace) →
/// group delimiters (expected close first, then longest match) → macro escape → comment
/// starts → specials → forbidden-character check → content characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRules {
    /// Whitespace handling; `None` disables it entirely.
    pub whitespace: Option<WhitespaceRules>,
    /// Macro recognition; `None` disables it.
    pub macros: Option<MacroRules>,
    /// The group types recognizable here (`{…}`, `[…]`, `$…$`, `$$…$$`, `\(…\)`, …
    /// — all just group types; math is not a core concept). On delimiter conflicts,
    /// earlier entries win (see [`PrefixTable`](super::PrefixTable)).
    pub group_types: Vec<GroupType>,
    /// Comment recognition; `None` disables it.
    pub comments: Option<CommentRules>,
    /// Whether two or more newlines within one whitespace run form a `ParagraphBreak`
    /// token. Only meaningful when whitespace handling is enabled.
    pub paragraph_breaks: bool,
    /// Specials *strings* (e.g. `~`, `&`, `#`); matched longest-first. Only the strings
    /// live here — their semantics live in libraries (Phase 4).
    pub specials: Vec<String>,
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
