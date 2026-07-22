//! Tokenization rules — the plain data that drives [`StdTokenReader`](super::StdTokenReader).
//!
//! `TokenRules` is defined here in the token topic and *stored* in the parsing state
//! ([`StateData<L>`](crate::state::StateData)). Everything that can vary during a parse —
//! delimiters, escape characters, enabled features — is a plain value in these structs,
//! changed only through reified state deltas at the transition choke point.
//! There are no privileged language concepts here: no default
//! `\`, `{}`, `%`, or `$` — the familiar LaTeX values are supplied by the latexlike preset,
//! which is also why none of these types implement `Default`.
//!
//! Group *classes* are the language's business: [`Lang::GroupTypeId`] is a closed
//! per-language classification (typically an enum — the latexlike preset: content vs.
//! math groups), fully detached from delimiter spellings. Which
//! *delimiter pairs* exist, and which class each belongs to, is runtime data — the
//! [`GroupRule`] values here. Any construct parser may mint a new rule mid-parse via a
//! state delta (an optional-argument parser momentarily declaring `[`…`]` group
//! delimiters, a custom spec declaring `<`…`>`).
//!
//! The one deliberate omission: **specials trigger strings are not enumerated here.**
//! Their recognition is delegated to the language via `Lang::scan_specials` (see
//! [`specials`](super::SpecialsMatch)), because trigger sets can be large and
//! provider-driven (the scope stack). Everything else — commands, comments, groups,
//! whitespace — is rules data.

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

/// Whitespace-handling rules. Gated by [`TokenRules::enable_whitespace`]; with the gate
/// off (or an empty `chars` set), whitespace characters are ordinary content characters,
/// `pre_space` is always empty, and paragraph breaks are never detected (character-level
/// access mode).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WhitespaceRules {
    /// The characters treated as whitespace (e.g. `" \t\n\r"`). Shared (`Arc<str>`) so
    /// state derivations clone rules data by refcount, not by content.
    pub chars: Arc<str>,
}

/// One command syntax: an escape character introducing a named invocation (`\textbf`,
/// `\&`). Several rules may coexist (distinct escape characters; earlier entries win on
/// conflict); an empty [`TokenRules::commands`] disables command recognition entirely.
///
/// "Command" is the token-level term (TeX lineage: control sequence). It is deliberately
/// *not* "macro": at the token level `\begin` is a command exactly like `\foobar` — which
/// names are macros, environments, or anything else is decided at parse time by the preset
/// (terminology stack: command → callable → macro/environment).
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
///
/// # `enable_*` feature gates
///
/// Every major feature has a boolean gate stored next to its data (pylatexenc's
/// `enable_macros`/`enable_comments`/… pattern). A disabled
/// feature's syntax reads as ordinary content characters while its data **stays in
/// place** — so a state delta can disable a feature and a later delta re-enable it,
/// without any party having to carry the original rules. Two spellings of "off" are
/// deliberate: gate `false` is the *scoped* off (data preserved for re-enabling), empty
/// data is the *constitutive* off (the language has no such feature).
pub struct TokenRules<L: Lang> {
    /// Whether whitespace handling is on; disabled = whitespace characters are ordinary
    /// content characters, `pre_space` is always empty, and paragraph breaks are never
    /// detected (character-level access mode).
    pub enable_whitespace: bool,
    /// Whitespace handling (gated by [`enable_whitespace`](Self::enable_whitespace)).
    pub whitespace: WhitespaceRules,
    /// Whether a whitespace run containing two or more newlines forms a paragraph break.
    /// Gates both `ParagraphBreak` tokens and the no-multi-newline rule of whitespace
    /// skipping (pre-space and post-space never consume a newline belonging to a
    /// `\n\s*\n` sequence). Only meaningful when whitespace handling is enabled.
    pub enable_multi_newline_paragraphs: bool,
    /// Whether group delimiters are recognized (gates the delimiter table — but **not**
    /// [`expecting_group_close`](Self::expecting_group_close), which is positional data:
    /// a group interior that disables groups still finds its own close).
    pub enable_groups: bool,
    /// The group delimiter rules recognizable here (`{…}`, `[…]`, `$…$`, `$$…$$`,
    /// `\(…\)`, … — all just delimiter pairs; math is not a core concept). On delimiter
    /// conflicts, earlier entries win (see [`PrefixTable`](super::PrefixTable)).
    pub groups: Vec<Arc<GroupRule<L>>>,
    /// Group rules with a *scoped lifecycle*: they
    /// tokenize exactly like [`groups`](Self::groups) — same gate, listed **first** in
    /// the [`PrefixTable`](super::PrefixTable), so they win same-spelling ties — but a
    /// state derivation that installs an
    /// [`expecting_group_close`](Self::expecting_group_close) which is *not* one of
    /// these rules (by `Arc` identity) clears this list in the derived state. Descending
    /// into a temporary rule's own group keeps them (nested delimiters balance
    /// recursively); descending into any other group drops them for that whole subtree
    /// (see [`ParsingState::derived`](crate::state::ParsingState::derived)). This is how
    /// a construct parser mints delimiters "for the occasion" — an optional `[`…`]`
    /// argument — with brace protection at any depth: the minted rule dies at the first
    /// descent into a group that is not itself.
    pub temporary_groups: Vec<Arc<GroupRule<L>>>,
    /// Whether command syntax is recognized; disabled = escape characters are ordinary
    /// content characters.
    pub enable_commands: bool,
    /// Command syntaxes; empty = no command recognition. `Arc`-shared like
    /// [`groups`](Self::groups): state derivations clone the
    /// rule list by refcount, and the shared rules carry pointer identity.
    pub commands: Vec<Arc<CommandRule>>,
    /// Whether comment syntax is recognized; disabled = comment starts are ordinary
    /// content characters.
    pub enable_comments: bool,
    /// Comment syntaxes; empty = no comment recognition. `Arc`-shared like
    /// [`groups`](Self::groups).
    pub comments: Vec<Arc<CommentRule>>,
    /// Whether the specials scan runs. The specials *data* lives with the language
    /// ([`Lang::scan_specials`](crate::state::Lang::scan_specials) → the scope stack's
    /// providers), but the
    /// gate is rules data so a delta can switch it: disabled states freeze with the empty
    /// [`TriggerChars`](super::TriggerChars) filter and the scan hook is never consulted.
    pub enable_specials: bool,
    /// Characters that may not appear as content; encountering one yields a recoverable
    /// [`TokenError`](super::TokenError). Empty = off (deliberately no `enable_*` gate:
    /// one trivially restorable string, not a feature toggle). Shared (`Arc<str>`) so
    /// state derivations clone it by refcount.
    pub forbidden_chars: Arc<str>,
    /// The group rule whose *close* delimiter takes precedence over all other delimiter
    /// matches — set (via a state delta) by the group construct parser upon entering a
    /// group whose delimiters are ambiguous. This is how `$…$` inside `$$…$$` resolves:
    /// inside a `$…$` group this field holds the `$…$` rule, so a following `$$`
    /// tokenizes as close-`$` (then open-`$`) rather than as a `$$` delimiter.
    /// Generalizes pylatexenc's `math_mode_delimiter` without privileging math.
    /// Not gated by [`enable_groups`](Self::enable_groups) — positional data, not a
    /// feature; the recognition guarantee for an entered group's close must survive any
    /// interior rule set.
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
            enable_whitespace: self.enable_whitespace,
            whitespace: self.whitespace.clone(),
            enable_multi_newline_paragraphs: self.enable_multi_newline_paragraphs,
            enable_groups: self.enable_groups,
            groups: self.groups.clone(),
            temporary_groups: self.temporary_groups.clone(),
            enable_commands: self.enable_commands,
            commands: self.commands.clone(),
            enable_comments: self.enable_comments,
            comments: self.comments.clone(),
            enable_specials: self.enable_specials,
            forbidden_chars: self.forbidden_chars.clone(),
            expecting_group_close: self.expecting_group_close.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for TokenRules<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRules")
            .field("enable_whitespace", &self.enable_whitespace)
            .field("whitespace", &self.whitespace)
            .field("enable_multi_newline_paragraphs", &self.enable_multi_newline_paragraphs)
            .field("enable_groups", &self.enable_groups)
            .field("groups", &self.groups)
            .field("temporary_groups", &self.temporary_groups)
            .field("enable_commands", &self.enable_commands)
            .field("commands", &self.commands)
            .field("enable_comments", &self.enable_comments)
            .field("comments", &self.comments)
            .field("enable_specials", &self.enable_specials)
            .field("forbidden_chars", &self.forbidden_chars)
            .field("expecting_group_close", &self.expecting_group_close)
            .finish()
    }
}

impl<L: Lang> PartialEq for TokenRules<L> {
    fn eq(&self, other: &Self) -> bool {
        self.enable_whitespace == other.enable_whitespace
            && self.whitespace == other.whitespace
            && self.enable_multi_newline_paragraphs == other.enable_multi_newline_paragraphs
            && self.enable_groups == other.enable_groups
            && self.groups == other.groups
            && self.temporary_groups == other.temporary_groups
            && self.enable_commands == other.enable_commands
            && self.commands == other.commands
            && self.enable_comments == other.enable_comments
            && self.comments == other.comments
            && self.enable_specials == other.enable_specials
            && self.forbidden_chars == other.forbidden_chars
            && self.expecting_group_close == other.expecting_group_close
    }
}

impl<L: Lang> Eq for TokenRules<L> {}
