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
/// Rules are held behind `Arc` in [`GroupRules::rules`]: the tokenizer's resolution of
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

/// One command syntax: an escape character introducing a named invocation (`\textbf`,
/// `\&`). Several rules may coexist (distinct escape characters; earlier entries win on
/// conflict); an empty [`CommandRules::rules`] list means no command recognition at all.
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
/// [`CommentRules::rules`] list means no comment recognition at all.
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

/// Whitespace-handling rules — the whitespace block of [`TokenRules`]. With
/// [`enabled`](Self::enabled) `false` (or an empty `chars` set), whitespace characters
/// are ordinary content characters, `pre_space` is always empty, and paragraph breaks
/// are never detected (character-level access mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhitespaceRules {
    /// Whether whitespace handling is on; disabled = whitespace characters are ordinary
    /// content characters, `pre_space` is always empty, and paragraph breaks are never
    /// detected (character-level access mode).
    pub enabled: bool,
    /// The characters treated as whitespace (e.g. `" \t\n\r"`). Shared (`Arc<str>`) so
    /// state derivations clone rules data by refcount, not by content.
    pub chars: Arc<str>,
}

impl WhitespaceRules {
    /// The all-empty value: `enabled` `false`, no whitespace characters — the
    /// whitespace block of [`TokenRules::empty`].
    pub fn empty() -> WhitespaceRules {
        WhitespaceRules { enabled: false, chars: "".into() }
    }
}

/// Paragraph-break rules — the paragraphs block of [`TokenRules`]. Paragraph breaks are
/// detected within whitespace runs, so this block only takes effect while whitespace
/// handling ([`WhitespaceRules::enabled`]) is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParagraphRules {
    /// Whether a whitespace run containing two or more newlines forms a paragraph break.
    /// Gates both `ParagraphBreak` tokens and the no-multi-newline rule of whitespace
    /// skipping (pre-space and post-space never consume a newline belonging to a
    /// `\n\s*\n` sequence). Only meaningful when whitespace handling is enabled.
    pub enabled: bool,
}

impl ParagraphRules {
    /// The all-empty value: `enabled` `false` — the paragraphs block of
    /// [`TokenRules::empty`].
    pub fn empty() -> ParagraphRules {
        ParagraphRules { enabled: false }
    }
}

/// Group-delimiter rules — the groups block of [`TokenRules`]: the delimiter table
/// ([`rules`](Self::rules) plus the scoped-lifecycle [`temporary`](Self::temporary)
/// list), its `enabled` gate, and the positional
/// [`expecting_close`](Self::expecting_close) slot.
pub struct GroupRules<L: Lang> {
    /// Whether group delimiters are recognized (gates the delimiter table — but **not**
    /// [`expecting_close`](Self::expecting_close), which is positional data: a group
    /// interior that disables groups still finds its own close).
    pub enabled: bool,
    /// The group delimiter rules recognizable here (`{…}`, `[…]`, `$…$`, `$$…$$`,
    /// `\(…\)`, … — all just delimiter pairs; math is not a core concept). On delimiter
    /// conflicts, earlier entries win (see [`PrefixTable`](super::PrefixTable)).
    pub rules: Vec<Arc<GroupRule<L>>>,
    /// Group rules with a *scoped lifecycle*: they
    /// tokenize exactly like [`rules`](Self::rules) — same gate, listed **first** in
    /// the [`PrefixTable`](super::PrefixTable), so they win same-spelling ties — but a
    /// state derivation that installs an
    /// [`expecting_close`](Self::expecting_close) which is *not* one of
    /// these rules (by `Arc` identity) clears this list in the derived state. Descending
    /// into a temporary rule's own group keeps them (nested delimiters balance
    /// recursively); descending into any other group drops them for that whole subtree
    /// (see [`ParsingState::derived`](crate::state::ParsingState::derived)). This is how
    /// a construct parser mints delimiters "for the occasion" — an optional `[`…`]`
    /// argument — with brace protection at any depth: the minted rule is dropped at
    /// the first descent into a group that is not itself.
    pub temporary: Vec<Arc<GroupRule<L>>>,
    /// The group rule whose *close* delimiter takes precedence over all other delimiter
    /// matches — set (via a state delta) by the group construct parser upon entering a
    /// group whose delimiters are ambiguous. This is how `$…$` inside `$$…$$` resolves:
    /// inside a `$…$` group this field holds the `$…$` rule, so a following `$$`
    /// tokenizes as close-`$` (then open-`$`) rather than as a `$$` delimiter.
    /// Generalizes pylatexenc's `math_mode_delimiter` without privileging math.
    /// Not gated by [`enabled`](Self::enabled) — positional data, not a
    /// feature; the recognition guarantee for an entered group's close must survive any
    /// interior rule set.
    pub expecting_close: Option<Arc<GroupRule<L>>>,
}

impl<L: Lang> GroupRules<L> {
    /// The all-empty value: `enabled` `false`, no delimiter rules, no temporary rules,
    /// no expected close — the groups block of [`TokenRules::empty`].
    pub fn empty() -> GroupRules<L> {
        GroupRules {
            enabled: false,
            rules: Vec::new(),
            temporary: Vec::new(),
            expecting_close: None,
        }
    }
}

/// Command-syntax rules — the commands block of [`TokenRules`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRules {
    /// Whether command syntax is recognized; disabled = escape characters are ordinary
    /// content characters.
    pub enabled: bool,
    /// Command syntaxes; empty = no command recognition. `Arc`-shared like
    /// [`GroupRules::rules`]: state derivations clone the
    /// rule list by refcount, and the shared rules carry pointer identity.
    pub rules: Vec<Arc<CommandRule>>,
}

impl CommandRules {
    /// The all-empty value: `enabled` `false`, no command syntaxes — the commands block
    /// of [`TokenRules::empty`].
    pub fn empty() -> CommandRules {
        CommandRules { enabled: false, rules: Vec::new() }
    }
}

/// Comment-syntax rules — the comments block of [`TokenRules`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentRules {
    /// Whether comment syntax is recognized; disabled = comment starts are ordinary
    /// content characters.
    pub enabled: bool,
    /// Comment syntaxes; empty = no comment recognition. `Arc`-shared like
    /// [`GroupRules::rules`].
    pub rules: Vec<Arc<CommentRule>>,
}

impl CommentRules {
    /// The all-empty value: `enabled` `false`, no comment syntaxes — the comments block
    /// of [`TokenRules::empty`].
    pub fn empty() -> CommentRules {
        CommentRules { enabled: false, rules: Vec::new() }
    }
}

/// Specials-scan rules — the specials block of [`TokenRules`]. The block holds only
/// the gate: the specials *data* lives with the language (see
/// [`enabled`](Self::enabled)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialsRules {
    /// Whether the specials scan runs. The specials *data* lives with the language
    /// ([`Lang::scan_specials`](crate::state::Lang::scan_specials) → the scope stack's
    /// providers), but the
    /// gate is rules data so a delta can switch it: disabled states freeze with the empty
    /// [`TriggerChars`](super::TriggerChars) filter and the scan hook is never consulted.
    pub enabled: bool,
}

impl SpecialsRules {
    /// The all-empty value: `enabled` `false` — the specials block of
    /// [`TokenRules::empty`].
    pub fn empty() -> SpecialsRules {
        SpecialsRules { enabled: false }
    }
}

/// Forbidden-character rules — the forbidden-characters block of [`TokenRules`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForbiddenCharsRules {
    /// Characters that may not appear as content; encountering one yields a recoverable
    /// [`TokenError`](super::TokenError). Empty = off (deliberately no `enabled` gate:
    /// one trivially restorable string, not a feature toggle). Shared (`Arc<str>`) so
    /// state derivations clone it by refcount.
    pub chars: Arc<str>,
}

impl ForbiddenCharsRules {
    /// The all-empty value: no forbidden characters — the forbidden-characters block of
    /// [`TokenRules::empty`].
    pub fn empty() -> ForbiddenCharsRules {
        ForbiddenCharsRules { chars: "".into() }
    }
}

/// The complete data driving standard tokenization, stored in the parsing state — one
/// field per tokenization feature.
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
/// Each feature block is a plain public field (construction sites write struct literals
/// over the blocks); reading code uses the per-feature accessor methods
/// ([`whitespace_enabled`](Self::whitespace_enabled), [`group_rules`](Self::group_rules),
/// …).
///
/// # Per-feature `enabled` gates
///
/// Every feature block stores a boolean `enabled` gate next to its data (pylatexenc's
/// `enable_macros`/`enable_comments`/… pattern). A disabled
/// feature's syntax reads as ordinary content characters while its data **stays in
/// place** — so a state delta can disable a feature and a later delta re-enable it,
/// without any party having to carry the original rules. Three spellings of "off" are
/// deliberate, each with its own word: gate `false` is the *scoped* off (**disabled** —
/// data preserved for re-enabling); empty data is the *constitutive* off (**empty** —
/// no rules data, so nothing is recognized even with the gate `true`); and beyond both
/// runtime spellings, a feature can be **absent** — the *compile-time* off, declared
/// per feature on the language via
/// [`Lang::Features`](crate::state::Lang::Features): the language has no such feature
/// at all, and no runtime data can say otherwise (the
/// [`LangFeatures`](crate::state::LangFeatures) docs define the full vocabulary).
pub struct TokenRules<L: Lang> {
    /// Whitespace handling: the whitespace character set and its gate
    /// ([`WhitespaceRules`]).
    pub whitespace: WhitespaceRules,
    /// Paragraph-break detection ([`ParagraphRules`]).
    pub paragraphs: ParagraphRules,
    /// Group delimiters: the delimiter table, its gate, and the expected close
    /// ([`GroupRules`]).
    pub groups: GroupRules<L>,
    /// Command syntaxes and their gate ([`CommandRules`]).
    pub commands: CommandRules,
    /// Comment syntaxes and their gate ([`CommentRules`]).
    pub comments: CommentRules,
    /// The specials-scan gate ([`SpecialsRules`]).
    pub specials: SpecialsRules,
    /// The forbidden-character set ([`ForbiddenCharsRules`]).
    pub forbidden_chars: ForbiddenCharsRules,
}

impl<L: Lang> TokenRules<L> {
    /// The all-empty rules value: every feature block's `enabled` gate `false`, every
    /// collection and string empty, no expected group close — nothing is recognized,
    /// and content is consumed as plain characters (character-level access mode). The
    /// default [`Lang::initial_state_data`] builds its seed over exactly this value
    /// (via [`StateData::empty`](crate::state::StateData::empty)); real languages start
    /// from it and fill in their canonical rules.
    ///
    /// This is the *constitutive* off (no rules data at all) — for the
    /// *scoped* off over existing rules, see
    /// [`TokenRulesOverrides::disable_all`](crate::state::TokenRulesOverrides::disable_all),
    /// which flips the gates while the data stays in place.
    ///
    /// Deliberately a named constructor, not a `Default` impl: there is no privileged
    /// "default language" in the machinery (the familiar LaTeX values are the
    /// latexlike preset's [`default_token_rules`](crate::latexlike::default_token_rules)), and a struct-update
    /// `..Default::default()` would silently zero future fields where the named
    /// constructor documents the all-empty intent. Each feature block has a matching
    /// `empty()` constructor ([`WhitespaceRules::empty`], [`GroupRules::empty`], …).
    pub fn empty() -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules::empty(),
            paragraphs: ParagraphRules::empty(),
            groups: GroupRules::empty(),
            commands: CommandRules::empty(),
            comments: CommentRules::empty(),
            specials: SpecialsRules::empty(),
            forbidden_chars: ForbiddenCharsRules::empty(),
        }
    }

    /// Whether whitespace handling is on ([`WhitespaceRules::enabled`]).
    pub fn whitespace_enabled(&self) -> bool {
        self.whitespace.enabled
    }

    /// The characters treated as whitespace ([`WhitespaceRules::chars`]).
    pub fn whitespace_chars(&self) -> &str {
        &self.whitespace.chars
    }

    /// Whether a multi-newline whitespace run forms a paragraph break
    /// ([`ParagraphRules::enabled`]).
    pub fn paragraphs_enabled(&self) -> bool {
        self.paragraphs.enabled
    }

    /// Whether group delimiters are recognized ([`GroupRules::enabled`]).
    pub fn groups_enabled(&self) -> bool {
        self.groups.enabled
    }

    /// The group delimiter rules recognizable here ([`GroupRules::rules`]).
    pub fn group_rules(&self) -> &[Arc<GroupRule<L>>] {
        &self.groups.rules
    }

    /// The scoped-lifecycle group rules ([`GroupRules::temporary`]).
    pub fn temporary_group_rules(&self) -> &[Arc<GroupRule<L>>] {
        &self.groups.temporary
    }

    /// The group rule whose close delimiter takes precedence over all other delimiter
    /// matches ([`GroupRules::expecting_close`]).
    pub fn expecting_group_close(&self) -> Option<&Arc<GroupRule<L>>> {
        self.groups.expecting_close.as_ref()
    }

    /// Whether command syntax is recognized ([`CommandRules::enabled`]).
    pub fn commands_enabled(&self) -> bool {
        self.commands.enabled
    }

    /// The command syntaxes ([`CommandRules::rules`]).
    pub fn command_rules(&self) -> &[Arc<CommandRule>] {
        &self.commands.rules
    }

    /// Whether comment syntax is recognized ([`CommentRules::enabled`]).
    pub fn comments_enabled(&self) -> bool {
        self.comments.enabled
    }

    /// The comment syntaxes ([`CommentRules::rules`]).
    pub fn comment_rules(&self) -> &[Arc<CommentRule>] {
        &self.comments.rules
    }

    /// Whether the specials scan runs ([`SpecialsRules::enabled`]).
    pub fn specials_enabled(&self) -> bool {
        self.specials.enabled
    }

    /// The characters that may not appear as content ([`ForbiddenCharsRules::chars`]).
    pub fn forbidden_chars(&self) -> &str {
        &self.forbidden_chars.chars
    }
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

impl<L: Lang> Clone for GroupRules<L> {
    fn clone(&self) -> Self {
        GroupRules {
            enabled: self.enabled,
            rules: self.rules.clone(),
            temporary: self.temporary.clone(),
            expecting_close: self.expecting_close.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for GroupRules<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupRules")
            .field("enabled", &self.enabled)
            .field("rules", &self.rules)
            .field("temporary", &self.temporary)
            .field("expecting_close", &self.expecting_close)
            .finish()
    }
}

impl<L: Lang> PartialEq for GroupRules<L> {
    fn eq(&self, other: &Self) -> bool {
        self.enabled == other.enabled
            && self.rules == other.rules
            && self.temporary == other.temporary
            && self.expecting_close == other.expecting_close
    }
}

impl<L: Lang> Eq for GroupRules<L> {}

impl<L: Lang> Clone for TokenRules<L> {
    fn clone(&self) -> Self {
        TokenRules {
            whitespace: self.whitespace.clone(),
            paragraphs: self.paragraphs.clone(),
            groups: self.groups.clone(),
            commands: self.commands.clone(),
            comments: self.comments.clone(),
            specials: self.specials.clone(),
            forbidden_chars: self.forbidden_chars.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for TokenRules<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRules")
            .field("whitespace", &self.whitespace)
            .field("paragraphs", &self.paragraphs)
            .field("groups", &self.groups)
            .field("commands", &self.commands)
            .field("comments", &self.comments)
            .field("specials", &self.specials)
            .field("forbidden_chars", &self.forbidden_chars)
            .finish()
    }
}

impl<L: Lang> PartialEq for TokenRules<L> {
    fn eq(&self, other: &Self) -> bool {
        self.whitespace == other.whitespace
            && self.paragraphs == other.paragraphs
            && self.groups == other.groups
            && self.commands == other.commands
            && self.comments == other.comments
            && self.specials == other.specials
            && self.forbidden_chars == other.forbidden_chars
    }
}

impl<L: Lang> Eq for TokenRules<L> {}
