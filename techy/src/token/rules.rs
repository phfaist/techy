//! Tokenization rules: the data that decides how source text is cut into tokens.
//!
//! [`TokenRules`] is all of it — which character introduces a command, which strings
//! open and close a group, which string starts a comment, which characters count as
//! whitespace, which characters are rejected outright — with one block per tokenization
//! feature: [`WhitespaceRules`], [`ParagraphRules`], [`GroupRules`], [`CommandRules`],
//! [`CommentRules`], [`SpecialsRules`], and [`ForbiddenCharsRules`].
//!
//! A language whose syntax differs from LaTeX only in *which* characters play which role
//! needs nothing more than a `TokenRules` value of its own:
//! [`StdTokenReader`](super::StdTokenReader) reads whatever the rules say. A language
//! that has to decide *differently* which token comes next implements
//! [`TokenReader`](super::TokenReader) instead.
//!
//! Nothing is built in. There is no predefined `\`, `{}`, `%`, or `$`, and none of these
//! types implement `Default`; the familiar LaTeX values are the latexlike preset's
//! ([`default_token_rules`](crate::latexlike::default_token_rules)). Build a set of your
//! own from [`TokenRules::empty`] with struct-update syntax.
//!
//! The rules are stored in the parsing state ([`StateData`](crate::core::StateData)), so
//! every one of these settings can change while a parse runs: a
//! [`ParsingStateDelta`](crate::core::ParsingStateDelta) holds
//! [`TokenRulesOverrides`](crate::core::token::TokenRulesOverrides) — one `*Overrides`
//! type per block — and the derived state's rules are the previous ones with those
//! overrides applied.
//!
//! Group *classes* and group *delimiters* are separate things. The class is the
//! language's own closed classification, [`Lang::GroupTypeId`]: usually an enum, and the
//! preset uses it to tell content groups from math groups. Which delimiter pairs exist,
//! and which class each pair produces, is runtime data — the [`GroupRule`] values here.
//! A construct parser may add a rule for the duration of one construct through a state
//! delta, which is how an optional-argument parser makes `[` … `]` a group pair just
//! while it reads that argument.
//!
//! One thing is deliberately not listed here: the trigger strings of specials. The
//! language recognizes those itself, through
//! [`Lang::scan_specials`](crate::core::Lang::scan_specials), because a trigger set can
//! be large and is usually assembled from whichever definitions are loaded. Everything
//! else — whitespace, groups, commands, comments, forbidden characters — is rules data.
//!
//! The guide chapter [Language syntax](crate::guide::language_syntax) describes these
//! settings in LaTeX terms;
//! [Token rules and specials recognition](crate::guide::custom_lang#token-rules-and-specials-recognition)
//! covers configuring them for a language of your own.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::state::{FeaturePresence, Lang, LangFeatures};

/// One group delimiter pair, and the class of group it opens.
///
/// The delimiters are arbitrary non-empty strings — `{` … `}`, `[` … `]`, `$` … `$`,
/// `$$` … `$$`, `\(` … `\)`. Several rules may use the same string (`$…$` alongside
/// `$$…$$`), and one rule may use the same string to open and to close; the
/// [`PrefixTable`](super::PrefixTable) settles which rule a given piece of text matches.
///
/// [`group_type`](Self::group_type) is the language's own classification of the group
/// ([`Lang::GroupTypeId`]) and is independent of the spellings: the preset marks
/// `$…$` and `\(…\)` as math groups and `{…}` as a content group.
///
/// Rules are stored behind `Arc` in [`GroupRules::rules`]. Which rule matched is
/// recorded in the [`GroupOpen`](super::TokenKind::GroupOpen) token the reader produces,
/// so a construct parser never has to work it out again.
///
/// # Two equal rules are not interchangeable
///
/// Wherever behavior depends on *which* rule is in play, rules are compared by **`Arc`
/// identity** (`Arc::ptr_eq`), never by the structural `==` this type also implements:
///
/// - [`ParsingState::derived`](crate::core::ParsingState::derived) keeps the
///   [`temporary`](GroupRules::temporary) rules only when the installed
///   [`expecting_close`](GroupRules::expecting_close) **is one of them by `Arc`
///   identity**;
/// - the derivation caches key on the shared handles the same way: a derived state
///   reuses its base's [`PrefixTable`](super::PrefixTable) only when the rule lists
///   match elementwise by `Arc` identity, and the per-parse group-interior derivations
///   are memoized per `(base, rule)` handle pair
///   ([`ParserSession::group_interior_state`](crate::core::ParserSession::group_interior_state));
/// - the standard optional-argument parsers recognize the rules they declared
///   themselves by `Arc` identity in the matched token.
///
/// So clone the `Arc`, never the value: a data-equal copy behaves differently in every
/// one of these places, and a `contains(&rule)`-style check (which uses `==`) answers a
/// different question. That other question — "same group class and same delimiter
/// spellings" — is exactly what the structural `==` answers, and it is what
/// [`TokenKind`](super::TokenKind) equality uses to compare `GroupOpen` tokens.
pub struct GroupRule<L: Lang> {
    /// The class of the groups this rule opens — the preset's content-group versus
    /// math-group distinction, for instance. Independent of the spellings below.
    pub group_type: L::GroupTypeId,
    /// Opening delimiter (`{` in LaTeX).
    pub open: String,
    /// Closing delimiter (`}` in LaTeX).
    pub close: String,
}

/// One command syntax: an escape character introducing a named invocation.
///
/// With `escape_char: '\\'` and the ASCII letters as [`name_chars`](Self::name_chars) —
/// the LaTeX setting — `\textbf` reads as one command token named `textbf`, and `\&` as
/// one command token named `&`.
///
/// Several rules may coexist, each with its own escape character; when two of them claim
/// the same character, the earlier entry of [`CommandRules::rules`] wins. An empty rule
/// list means no command is recognized and escape characters read as ordinary content.
/// A command's syntax is always an escape character followed by a name; there is no
/// other form.
///
/// "Command" is the token-level term (TeX calls it a control sequence), deliberately not
/// "macro": at this level `\begin` is a command exactly like `\foobar`, and which names
/// are macros, environments, or anything else is decided later from the [callable
/// specs](crate::core::specs) in effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRule {
    /// The escape character introducing the command (`\` in LaTeX).
    pub escape_char: char,
    /// The characters a command name may be built from — the ASCII letters, in LaTeX.
    ///
    /// The name always takes the character right after the escape character. If that
    /// character is in this set, the name extends greedily over the following characters
    /// of the set; otherwise the name is that single character. So `\textbf` is named
    /// `textbf`, while `\&` is named `&`.
    ///
    /// A name whose first character came from this set also consumes the whitespace that
    /// follows it, which becomes the command token's post-space (`\textbf  {x}` puts the
    /// two spaces in the token, not in the content); a name of one character outside the
    /// set does not.
    pub name_chars: String,
}

/// One comment syntax: a start delimiter, with the comment running to the end of the line.
///
/// With `start: "%"` — the LaTeX setting — `% remark` reads as one comment token, which
/// also takes in the newline that ends it and the indentation of the next line (the
/// token's post-space).
///
/// The terminator is always `'\n'`, or the end of the input; `'\r'` is ordinary content.
/// It does not depend on [`WhitespaceRules`], so comments still work when whitespace
/// handling is disabled. A comment always runs to the end of the line: there is no
/// block-comment form.
///
/// Several rules may coexist, and the longest matching start delimiter wins. An empty
/// [`CommentRules::rules`] list means no comment is recognized and a `%` reads as an
/// ordinary character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentRule {
    /// The comment-start delimiter string (`%` in LaTeX).
    pub start: String,
}

/// Whitespace handling — the whitespace block of [`TokenRules`]: which characters count
/// as whitespace, and whether whitespace is treated specially at all.
///
/// While it is on, the reader skips a run of these characters ahead of each token and
/// attaches the run to that token as its *pre-space*, rather than producing a token per
/// space character. The preset's set is `" \t\n\r\u{000B}\u{000C}"`.
///
/// While it is off — [`enabled`](Self::enabled) `false`, or an empty
/// [`chars`](Self::chars) set — whitespace characters are ordinary content characters,
/// every token's pre-space is empty, and paragraph breaks are never detected. That is
/// the character-by-character reading mode.
///
/// Changed during a parse through
/// [`WhitespaceOverrides`](crate::core::token::WhitespaceOverrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhitespaceRules {
    /// Whether whitespace is treated specially rather than as content; see the type
    /// documentation for what turning it off changes.
    pub enabled: bool,
    /// The characters treated as whitespace (`" \t\n\r"`, say). Shared (`Arc<str>`), so
    /// deriving a state clones the set by reference count rather than by content.
    pub chars: Arc<str>,
}

impl WhitespaceRules {
    /// The all-empty value: `enabled` `false`, no whitespace characters — the
    /// whitespace block of [`TokenRules::empty`].
    pub fn empty() -> WhitespaceRules {
        WhitespaceRules { enabled: false, chars: "".into() }
    }
}

/// Paragraph-break detection — the paragraphs block of [`TokenRules`].
///
/// A paragraph break is a run of whitespace holding two or more newlines: the blank line
/// between two paragraphs of LaTeX source. Breaks are found inside whitespace runs, so
/// this block has an effect only while whitespace handling
/// ([`WhitespaceRules::enabled`]) is on.
///
/// Changed during a parse through
/// [`ParagraphOverrides`](crate::core::token::ParagraphOverrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParagraphRules {
    /// Whether a whitespace run containing two or more newlines is a paragraph break.
    ///
    /// This gates two things at once: the
    /// [`ParagraphBreak`](super::TokenKind::ParagraphBreak) tokens themselves, and the
    /// rule that whitespace skipping stops in front of such a run — so with it on,
    /// neither a token's pre-space nor a command's or comment's post-space ever swallows
    /// a newline belonging to a `\n\s*\n` sequence. Meaningful only while whitespace
    /// handling is enabled.
    pub enabled: bool,
}

impl ParagraphRules {
    /// The all-empty value: `enabled` `false` — the paragraphs block of
    /// [`TokenRules::empty`].
    pub fn empty() -> ParagraphRules {
        ParagraphRules { enabled: false }
    }
}

/// Group delimiters — the groups block of [`TokenRules`].
///
/// [`rules`](Self::rules) is the table of delimiter pairs in effect,
/// [`temporary`](Self::temporary) holds pairs added for the duration of one construct,
/// [`enabled`](Self::enabled) turns delimiter recognition on and off, and
/// [`expecting_close`](Self::expecting_close) names the closing delimiter the current
/// position is waiting for.
///
/// Changed during a parse through
/// [`GroupOverrides`](crate::core::token::GroupOverrides).
pub struct GroupRules<L: Lang> {
    /// Whether group delimiters are recognized; when `false`, delimiter strings read as
    /// ordinary content characters.
    ///
    /// This gates the delimiter table only, **not**
    /// [`expecting_close`](Self::expecting_close): a group interior that switches groups
    /// off can still find its own closing delimiter.
    pub enabled: bool,
    /// The delimiter pairs recognized here — `{` … `}`, `[` … `]`, `$` … `$`,
    /// `$$` … `$$`, `\(` … `\)`, whatever the language declares. They are only pairs of
    /// strings; math is not a concept the machinery knows.
    ///
    /// A longer delimiter is matched ahead of a shorter one it starts with, and when two
    /// rules claim the same delimiter string the earlier entry wins; see
    /// [`PrefixTable`](super::PrefixTable) for the exact resolution.
    pub rules: Vec<Arc<GroupRule<L>>>,
    /// Group rules that last only as long as the construct that added them.
    ///
    /// They tokenize exactly like [`rules`](Self::rules) — the same
    /// [`enabled`](Self::enabled) gate applies — and they come first in the
    /// [`PrefixTable`](super::PrefixTable), so they win ties against a permanent rule
    /// spelled the same way.
    ///
    /// What makes them temporary is how a state derivation treats them: descending into
    /// a group whose rule is *not* one of these (by `Arc` identity) clears this list for
    /// that whole subtree, while descending into a temporary rule's own group keeps it,
    /// so nested delimiters still balance
    /// ([`ParsingState::derived`](crate::core::ParsingState::derived)).
    ///
    /// This is how a construct parser declares delimiters for one occasion. An
    /// optional-argument parser adds `[` … `]` as a temporary pair, and `\cmd[a{]}b]`
    /// then reads `a{]}b` as the whole argument: the `]` inside the braces is not a
    /// delimiter there, because the temporary rule was dropped on the way into the `{`
    /// group. That protection holds at any depth.
    pub temporary: Vec<Arc<GroupRule<L>>>,
    /// The group rule whose *closing* delimiter takes precedence over every other
    /// delimiter match — set through a state delta by the group construct parser when it
    /// enters a group whose delimiters are ambiguous.
    ///
    /// This is how `$…$` inside `$$…$$` resolves: inside a `$…$` group this field holds
    /// the `$…$` rule, so a following `$$` reads as a closing `$` and then an opening
    /// `$`, rather than as one `$$` delimiter. It generalizes pylatexenc's
    /// `math_mode_delimiter` without treating math as special.
    ///
    /// Not gated by [`enabled`](Self::enabled): a group that has been entered must be
    /// able to find its own close whatever rules its interior installs.
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

/// Command syntaxes — the commands block of [`TokenRules`]: which escape characters
/// introduce a command, and whether commands are recognized at all.
///
/// Changed during a parse through
/// [`CommandOverrides`](crate::core::token::CommandOverrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRules {
    /// Whether command syntax is recognized; when `false`, an escape character such as
    /// `\` reads as an ordinary content character.
    pub enabled: bool,
    /// The command syntaxes ([`CommandRule`]); an empty list means no command is
    /// recognized even with the gate on. Shared behind `Arc`, so deriving a state clones
    /// the list by reference count.
    pub rules: Vec<Arc<CommandRule>>,
}

impl CommandRules {
    /// The all-empty value: `enabled` `false`, no command syntaxes — the commands block
    /// of [`TokenRules::empty`].
    pub fn empty() -> CommandRules {
        CommandRules { enabled: false, rules: Vec::new() }
    }
}

/// Comment syntaxes — the comments block of [`TokenRules`]: which strings start a
/// comment, and whether comments are recognized at all.
///
/// Changed during a parse through
/// [`CommentOverrides`](crate::core::token::CommentOverrides): a callable can switch
/// comments off for its body, so that a `%` inside stays ordinary content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentRules {
    /// Whether comment syntax is recognized; when `false`, a comment-start string such
    /// as `%` reads as an ordinary content character.
    pub enabled: bool,
    /// The comment syntaxes ([`CommentRule`]); an empty list means no comment is
    /// recognized even with the gate on. Shared behind `Arc`, so deriving a state clones
    /// the list by reference count.
    pub rules: Vec<Arc<CommentRule>>,
}

impl CommentRules {
    /// The all-empty value: `enabled` `false`, no comment syntaxes — the comments block
    /// of [`TokenRules::empty`].
    pub fn empty() -> CommentRules {
        CommentRules { enabled: false, rules: Vec::new() }
    }
}

/// The specials scan — the specials block of [`TokenRules`].
///
/// *Specials* are callables triggered by a plain character sequence instead of by a
/// command name: `~`, `--`, the quote ligatures. This block holds nothing but the on/off
/// gate, because the triggers themselves are not rules data — the language recognizes
/// them through [`Lang::scan_specials`](crate::core::Lang::scan_specials), which in the
/// preset asks whichever definition providers are loaded.
///
/// Changed during a parse through
/// [`SpecialsOverrides`](crate::core::token::SpecialsOverrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialsRules {
    /// Whether the specials scan runs.
    ///
    /// The gate is rules data even though the triggers are not, so that a state delta
    /// can switch the scan off: a state with it `false` is created with the empty
    /// [`TriggerChars`](super::TriggerChars) filter, and the language's scan hook is
    /// never consulted there.
    pub enabled: bool,
}

impl SpecialsRules {
    /// The all-empty value: `enabled` `false` — the specials block of
    /// [`TokenRules::empty`].
    pub fn empty() -> SpecialsRules {
        SpecialsRules { enabled: false }
    }
}

/// Characters rejected as content — the forbidden-characters block of [`TokenRules`].
///
/// The preset forbids nothing; a language might forbid, say, a raw `\t` in a context
/// where only spaces are meaningful.
///
/// This block has no `enabled` gate on purpose: an empty
/// [`chars`](Self::chars) string is already the off setting, and it is one short string
/// to put back rather than a feature to re-enable. Changed during a parse through
/// [`ForbiddenCharsOverrides`](crate::core::token::ForbiddenCharsOverrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForbiddenCharsRules {
    /// The characters that may not appear as content. Reading one produces a recoverable
    /// [`TokenError`](super::TokenError) rather than a content character, so a tolerant
    /// parse reports it and carries on past it. Empty means nothing is forbidden.
    ///
    /// Shared (`Arc<str>`), so deriving a state clones the set by reference count.
    pub chars: Arc<str>,
}

impl ForbiddenCharsRules {
    /// The all-empty value: no forbidden characters — the forbidden-characters block of
    /// [`TokenRules::empty`].
    pub fn empty() -> ForbiddenCharsRules {
        ForbiddenCharsRules { chars: "".into() }
    }
}

/// The data that decides how source text is cut into tokens: one block per tokenization
/// feature.
///
/// A `TokenRules` value says which characters are whitespace, whether a blank line is a
/// paragraph break, which delimiter pairs open and close a group, which escape
/// characters introduce a command and which characters its name may use, which strings
/// start a comment, whether the specials scan runs, and which characters are rejected as
/// content. Setting those seven blocks is all it takes to give a LaTeX-like language its
/// own surface syntax.
///
/// The value is stored in the parsing state, so it can change while a parse runs — see
/// [`TokenRulesOverrides`](crate::core::token::TokenRulesOverrides), the matching set of
/// per-block override types a state delta holds.
///
/// [`StdTokenReader`](super::StdTokenReader) reads exactly what this data says (together
/// with the [`PrefixTable`](super::PrefixTable) derived from it and the language's
/// specials-scan hook). A language that needs genuinely different tokenization
/// *behavior*, rather than different characters, implements the
/// [`TokenReader`](super::TokenReader) trait instead.
///
/// # Where the values come from
///
/// There is no default rule set. Start from
/// [`default_token_rules`](crate::latexlike::default_token_rules) to get the familiar
/// LaTeX values (`\` with the ASCII letters as command names, `{`/`}` and the math
/// delimiters as groups, `%` comments, ASCII whitespace with paragraph breaks on,
/// nothing forbidden), or from [`empty()`](Self::empty) to build a set of your own.
///
/// Each block is a public field, so construction writes a struct literal per block;
/// reading code uses the accessor methods ([`whitespace_enabled`](Self::whitespace_enabled),
/// [`group_rules`](Self::group_rules), …), which answer sensible values even for a
/// language that does not have the feature at all.
///
/// # Detection priority
///
/// At a position, whitespace is skipped first and becomes the next token's pre-space.
/// Then the first of these that matches decides the token: a paragraph break; the
/// closing delimiter the state expects
/// ([`expecting_group_close`](Self::expecting_group_close)); the longest group delimiter
/// in the [`PrefixTable`](super::PrefixTable); a command escape character; a comment
/// start; a specials trigger; a forbidden character (reported as an error); otherwise a
/// single content character. Group delimiters are tried before commands, so a delimiter
/// written with an escape character (`\(`) wins over reading a command. Whichever step
/// belongs to a disabled or absent feature never matches.
/// [`StdTokenReader::scan_std_token_at`](super::StdTokenReader::scan_std_token_at)
/// documents the order in full.
///
/// # The three ways a feature can be off
///
/// Each block except forbidden characters has its own `enabled` gate, in the spirit
/// of pylatexenc's `enable_macros`/`enable_comments` switches — and there are three
/// distinct ways for a feature to be off, each with its own word:
///
/// - **disabled** — the gate is `false`. The feature's syntax reads as ordinary content
///   characters while its data stays in place, so one state delta can switch a feature
///   off and a later one switch it back on without anyone having to keep a copy of the
///   rules.
/// - **empty** — there is no data: an empty rule list, or an empty character set.
///   Nothing is recognized even with the gate `true`.
/// - **absent** — the language does not have the feature at all. This one is a
///   compile-time declaration, made per feature through
///   [`Lang::Features`](crate::core::Lang::Features), and no runtime data can say
///   otherwise; [`LangFeatures`](crate::core::LangFeatures) defines the full vocabulary.
///
/// Absence reaches the storage: an absent feature's field below holds a zero-sized
/// stand-in ([`FeaturePresence::Store`](crate::core::FeaturePresence::Store)) rather
/// than its rules block, so the field takes no space and rules data for that feature
/// cannot even be written. For a language that has every feature — the usual case, and
/// the preset's — the fields simply *are* the blocks and none of this shows.
///
/// # Constructing rules for a language with absent features
///
/// Write the present features' blocks as plain literals and take every other field
/// from [`empty()`](Self::empty) with struct-update syntax:
///
/// ```
/// # use techy::core::token::{CommandRule, CommandRules, TokenRules};
/// # use techy::core::TrivialLang;
/// # use std::sync::Arc;
/// # // The example language declares every feature present (`TrivialLang`), so the
/// # // recipe compiles here as-is; for a partially-absent language the same recipe
/// # // applies, and only the absent features' fields must come from `empty()`.
/// # #[derive(Debug, Clone, Copy)]
/// # struct MyLang;
/// # impl TrivialLang for MyLang {}
/// let rules: TokenRules<MyLang> = TokenRules {
///     commands: CommandRules {
///         enabled: true,
///         rules: vec![Arc::new(CommandRule { escape_char: '\\', name_chars: "".into() })],
///     },
///     ..TokenRules::empty()
/// };
/// ```
///
/// (Writing a literal for an *absent* feature's field is a type error: the field is the
/// zero-sized stand-in, not the block.)
pub struct TokenRules<L: Lang> {
    /// Whitespace handling: the whitespace character set and its gate
    /// ([`WhitespaceRules`]). Holds the zero-sized stand-in, and no data, for a language
    /// that declares the whitespace feature absent.
    pub whitespace:
        <<L::Features as LangFeatures>::Whitespace as FeaturePresence>::Store<WhitespaceRules>,
    /// Paragraph-break detection ([`ParagraphRules`]). Holds the zero-sized stand-in for
    /// a language that declares the paragraphs feature absent.
    pub paragraphs:
        <<L::Features as LangFeatures>::Paragraphs as FeaturePresence>::Store<ParagraphRules>,
    /// Group delimiters: the delimiter table, its gate, and the expected close
    /// ([`GroupRules`]). Holds the zero-sized stand-in for a language that declares the
    /// groups feature absent.
    ///
    /// The rules are held behind `Arc`, and wherever behavior depends on which rule is
    /// in play they are compared by identity — see the identity section on
    /// [`GroupRule`].
    pub groups: <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<GroupRules<L>>,
    /// Command syntaxes and their gate ([`CommandRules`]). Holds the zero-sized stand-in
    /// for a language that declares the commands feature absent.
    pub commands:
        <<L::Features as LangFeatures>::Commands as FeaturePresence>::Store<CommandRules>,
    /// Comment syntaxes and their gate ([`CommentRules`]). Holds the zero-sized stand-in
    /// for a language that declares the comments feature absent.
    pub comments:
        <<L::Features as LangFeatures>::Comments as FeaturePresence>::Store<CommentRules>,
    /// The specials-scan gate ([`SpecialsRules`]). Holds the zero-sized stand-in for a
    /// language that declares the specials feature absent.
    pub specials:
        <<L::Features as LangFeatures>::Specials as FeaturePresence>::Store<SpecialsRules>,
    /// The forbidden-character set ([`ForbiddenCharsRules`]). Holds the zero-sized
    /// stand-in for a language that declares the forbidden-characters feature absent.
    pub forbidden_chars: <<L::Features as LangFeatures>::ForbiddenChars as FeaturePresence>::Store<
        ForbiddenCharsRules,
    >,
}

impl<L: Lang> TokenRules<L> {
    /// Returns the all-empty rules: every gate `false`, every rule list and character
    /// set empty, no expected group close.
    ///
    /// Nothing at all is recognized — the reader produces one token per content
    /// character. This is the starting point for a rule set of your own: fill in the
    /// blocks the language needs with struct-update syntax, as shown on
    /// [`TokenRules`]. The default [`Lang::initial_state_data`] seeds a parse with
    /// exactly this value (through
    /// [`StateData::empty`](crate::core::StateData::empty)).
    ///
    /// This is the *empty* off — no rules data at all. To switch every feature off while
    /// keeping its data for later, use
    /// [`TokenRulesOverrides::disable_all`](crate::core::token::TokenRulesOverrides::disable_all),
    /// which flips the gates instead.
    ///
    /// It is a named constructor rather than a `Default` implementation because there is
    /// no privileged "default language" here — the familiar LaTeX values are the
    /// latexlike preset's
    /// [`default_token_rules`](crate::latexlike::default_token_rules) — and because
    /// `..Default::default()` would silently zero a field added later where this name
    /// states the all-empty intent. Each block has its own matching constructor
    /// ([`WhitespaceRules::empty`], [`GroupRules::empty`], …).
    ///
    /// It answers for *every* language: a present feature's field gets its block's
    /// `empty()` value, an absent feature's field the zero-sized stand-in. That makes it
    /// the struct-update base for a language with absent features too.
    pub fn empty() -> TokenRules<L> {
        TokenRules {
            whitespace: <L::Features as LangFeatures>::Whitespace::store_with(
                WhitespaceRules::empty,
            ),
            paragraphs: <L::Features as LangFeatures>::Paragraphs::store_with(
                ParagraphRules::empty,
            ),
            groups: <L::Features as LangFeatures>::Groups::store_with(GroupRules::empty),
            commands: <L::Features as LangFeatures>::Commands::store_with(CommandRules::empty),
            comments: <L::Features as LangFeatures>::Comments::store_with(CommentRules::empty),
            specials: <L::Features as LangFeatures>::Specials::store_with(SpecialsRules::empty),
            forbidden_chars: <L::Features as LangFeatures>::ForbiddenChars::store_with(
                ForbiddenCharsRules::empty,
            ),
        }
    }

    /// Whether whitespace handling is on ([`WhitespaceRules::enabled`]); `false`
    /// when the language declares the whitespace feature absent.
    pub fn whitespace_enabled(&self) -> bool {
        <L::Features as LangFeatures>::Whitespace::store_get(&self.whitespace)
            .is_some_and(|block| block.enabled)
    }

    /// The characters treated as whitespace ([`WhitespaceRules::chars`]); empty when
    /// the language declares the whitespace feature absent.
    ///
    /// The set as stored, whether or not whitespace handling is enabled — check
    /// [`whitespace_enabled`](Self::whitespace_enabled) too.
    pub fn whitespace_chars(&self) -> &str {
        <L::Features as LangFeatures>::Whitespace::store_get(&self.whitespace)
            .map_or("", |block| &block.chars)
    }

    /// Whether a multi-newline whitespace run forms a paragraph break
    /// ([`ParagraphRules::enabled`]); `false` when the language declares the
    /// paragraphs feature absent.
    pub fn paragraphs_enabled(&self) -> bool {
        <L::Features as LangFeatures>::Paragraphs::store_get(&self.paragraphs)
            .is_some_and(|block| block.enabled)
    }

    /// Whether group delimiters are recognized ([`GroupRules::enabled`]); `false`
    /// when the language declares the groups feature absent.
    pub fn groups_enabled(&self) -> bool {
        <L::Features as LangFeatures>::Groups::store_get(&self.groups)
            .is_some_and(|block| block.enabled)
    }

    /// The group delimiter rules ([`GroupRules::rules`]); empty when the language
    /// declares the groups feature absent.
    ///
    /// The list as stored, whether or not groups are enabled — check
    /// [`groups_enabled`](Self::groups_enabled) too.
    pub fn group_rules(&self) -> &[Arc<GroupRule<L>>] {
        <L::Features as LangFeatures>::Groups::store_get(&self.groups)
            .map_or(&[], |block| &block.rules)
    }

    /// The group rules that last only as long as the construct that added them
    /// ([`GroupRules::temporary`]); empty when the language declares the groups feature
    /// absent.
    pub fn temporary_group_rules(&self) -> &[Arc<GroupRule<L>>] {
        <L::Features as LangFeatures>::Groups::store_get(&self.groups)
            .map_or(&[], |block| &block.temporary)
    }

    /// The group rule whose close delimiter takes precedence over all other delimiter
    /// matches ([`GroupRules::expecting_close`]); `None` when the language declares
    /// the groups feature absent.
    pub fn expecting_group_close(&self) -> Option<&Arc<GroupRule<L>>> {
        <L::Features as LangFeatures>::Groups::store_get(&self.groups)
            .and_then(|block| block.expecting_close.as_ref())
    }

    /// Whether command syntax is recognized ([`CommandRules::enabled`]); `false`
    /// when the language declares the commands feature absent.
    pub fn commands_enabled(&self) -> bool {
        <L::Features as LangFeatures>::Commands::store_get(&self.commands)
            .is_some_and(|block| block.enabled)
    }

    /// The command syntaxes ([`CommandRules::rules`]); empty when the language declares
    /// the commands feature absent.
    ///
    /// The list as stored, whether or not commands are enabled — check
    /// [`commands_enabled`](Self::commands_enabled) too.
    pub fn command_rules(&self) -> &[Arc<CommandRule>] {
        <L::Features as LangFeatures>::Commands::store_get(&self.commands)
            .map_or(&[], |block| &block.rules)
    }

    /// Whether comment syntax is recognized ([`CommentRules::enabled`]); `false`
    /// when the language declares the comments feature absent.
    pub fn comments_enabled(&self) -> bool {
        <L::Features as LangFeatures>::Comments::store_get(&self.comments)
            .is_some_and(|block| block.enabled)
    }

    /// The comment syntaxes ([`CommentRules::rules`]); empty when the language declares
    /// the comments feature absent.
    ///
    /// The list as stored, whether or not comments are enabled — check
    /// [`comments_enabled`](Self::comments_enabled) too.
    pub fn comment_rules(&self) -> &[Arc<CommentRule>] {
        <L::Features as LangFeatures>::Comments::store_get(&self.comments)
            .map_or(&[], |block| &block.rules)
    }

    /// Whether the specials scan runs ([`SpecialsRules::enabled`]); `false` when
    /// the language declares the specials feature absent.
    pub fn specials_enabled(&self) -> bool {
        <L::Features as LangFeatures>::Specials::store_get(&self.specials)
            .is_some_and(|block| block.enabled)
    }

    /// The characters that may not appear as content ([`ForbiddenCharsRules::chars`]);
    /// empty when the language declares the forbidden-characters feature absent.
    pub fn forbidden_chars(&self) -> &str {
        <L::Features as LangFeatures>::ForbiddenChars::store_get(&self.forbidden_chars)
            .map_or("", |block| &block.chars)
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

// The equality impls carry one where-clause per store: the `Store` GAT itself
// promises only `Clone`/`Debug`, but both markers' stores satisfy `PartialEq`/`Eq`
// whenever the stored block does (and every rules block does), so the bounds hold at
// every concrete language.

impl<L: Lang> PartialEq for TokenRules<L>
where
    <<L::Features as LangFeatures>::Whitespace as FeaturePresence>::Store<WhitespaceRules>:
        PartialEq,
    <<L::Features as LangFeatures>::Paragraphs as FeaturePresence>::Store<ParagraphRules>:
        PartialEq,
    <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<GroupRules<L>>: PartialEq,
    <<L::Features as LangFeatures>::Commands as FeaturePresence>::Store<CommandRules>: PartialEq,
    <<L::Features as LangFeatures>::Comments as FeaturePresence>::Store<CommentRules>: PartialEq,
    <<L::Features as LangFeatures>::Specials as FeaturePresence>::Store<SpecialsRules>: PartialEq,
    <<L::Features as LangFeatures>::ForbiddenChars as FeaturePresence>::Store<
        ForbiddenCharsRules,
    >: PartialEq,
{
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

impl<L: Lang> Eq for TokenRules<L>
where
    <<L::Features as LangFeatures>::Whitespace as FeaturePresence>::Store<WhitespaceRules>: Eq,
    <<L::Features as LangFeatures>::Paragraphs as FeaturePresence>::Store<ParagraphRules>: Eq,
    <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<GroupRules<L>>: Eq,
    <<L::Features as LangFeatures>::Commands as FeaturePresence>::Store<CommandRules>: Eq,
    <<L::Features as LangFeatures>::Comments as FeaturePresence>::Store<CommentRules>: Eq,
    <<L::Features as LangFeatures>::Specials as FeaturePresence>::Store<SpecialsRules>: Eq,
    <<L::Features as LangFeatures>::ForbiddenChars as FeaturePresence>::Store<
        ForbiddenCharsRules,
    >: Eq,
{
}
