//! The machinery hub: the [`Lang`] contract and parsing state, the token layer, and
//! the parse engine.
//!
//! The hub is deliberately **flat**. State, tokens, and the engine form one mutually
//! recursive cluster — token rules live in the parsing state, the state is derived
//! through the engine's session, the engine drives readers over rules — so the hub
//! keeps the whole [`Language::parse()`](Language::parse) → [`ParseResult`] flow on
//! one page:
//!
//! - **Language contract and state** — [`Lang`] (the compile-time customization
//!   bundle) and its [`LangFeatures`] feature-presence declarations,
//!   [`ParsingState`] and its reified [`ParsingStateDelta`]s, [`StateData`],
//!   [`TokenRulesOverrides`], the [`NodeExtTypes`] ext bundle, and the
//!   [`TrivialLang`] test lang.
//! - **Tokens** — zero-copy [`Token`]s, data-driven [`TokenRules`], the
//!   [`TokenReader`] trait and standard reader ([`StdTokenReader`]), specials
//!   scanning ([`SpecialsMatch`], [`TriggerChars`]), and the token error family.
//! - **Engine** — the [`Language`] runtime bundle and its `parse()` entry,
//!   [`ParserSession`], the [`ParseDriver`] customization point ([`StdParseDriver`]),
//!   [`ParseResult`], the live parse-frame stack ([`Frame`], [`FrameTitle`],
//!   [`FrameRole`]), and the parsing-depth limiter ([`DescentGuard`],
//!   [`StdDescentGuard`] with its [`StdDescentGuardInit`] configuration).
//!
//! Three submodules hold the subsets with clear boundaries:
//!
//! - [`specs`] — author-side: defining callables and organizing definitions
//!   (callable specs, providers/packages/scopes, command resolution).
//! - [`constructs`] — the construct-parsing library (the [`ConstructParser`]
//!   contract, standard parsers, their diagnostic conditions).
//! - [`node`] — the node tree: reading, payloads, building.
//!
//! Consumer-side data models live at the crate top level ([`source`](crate::source),
//! [`error`](crate::error)), as do the consumer tool libraries
//! ([`extract`](crate::extract)); the familiar LaTeX behavior is the
//! [`latexlike`](crate::latexlike) preset.
//!
//! [`ConstructParser`]: constructs::ConstructParser

pub mod constructs;
pub mod node;
pub mod specs;

pub use crate::engine::{
    CommandResolver, DescentGuard, DescentRefusal, DescentWarning, Frame, FrameTitle,
    Language, ParseDriver, ParseResult, ParserSession, SessionDeriveError,
    StdDescentGuard, StdDescentGuardInit, StdParseDriver,
};
pub use crate::spec::FrameRole;
pub use crate::state::{
    AllLangFeatures, ClosedVocabulary, CommandOverrides,
    CommentOverrides, DeriveError,
    FeatureAbsent, FeaturePresence, FeaturePresent, FinalizeError,
    ForbiddenCharsOverrides, GroupOverrides, InvocationSyntax, Lang, LangFeatures,
    LangHasCommands, LangHasComments, LangHasForbiddenChars, LangHasGroups,
    LangHasParagraphs, LangHasScopes, LangHasSpecials, LangHasWhitespace, NodeExtTypes,
    NoLangFeatures, ParagraphOverrides, ParsingState, ParsingStateDelta,
    ParsingStateStack, SpecialsOverrides, StateData, TokenRulesOverrides, TrivialLang,
    WhitespaceOverrides,
};
pub use crate::token::{
    skip_whitespace, CommandRule, CommandRules, CommentRule, CommentRules,
    EndOfStreamAfterEscape, ForbiddenChar, ForbiddenCharsRules, GroupRule, GroupRules,
    ParagraphRules, PrefixEntry, PrefixTable, SpecialsMatch, SpecialsRules,
    StdTokenReader, Token, TokenError, TokenErrorKind, TokenKind, TokenReader,
    TokenRecovery, TokenResult, TokenRules, TriggerChars, WhitespaceRules,
};
