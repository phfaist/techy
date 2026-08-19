//! The machinery hub: the [`Lang`] contract and parsing state, and the parse engine.
//!
//! The hub is deliberately **flat**. State and the engine form one mutually recursive
//! cluster — the state carries what the engine's next step needs, the state is derived
//! through the engine's session, and readers and parsers run over both — so the hub keeps
//! the whole [`Language::parse()`](Language::parse) → [`ParseResult`] flow on one page:
//!
//! - **Language contract and state** — [`Lang`] (the compile-time customization
//!   bundle) and its [`LangFeatures`] feature-presence declarations,
//!   [`ParsingState`] and its reified [`ParsingStateDelta`]s (which carry the token
//!   layer's [`TokenRulesOverrides`](token::TokenRulesOverrides)), [`StateData`], the
//!   [`NodeExtTypes`] ext bundle, and the [`TrivialLang`] test lang.
//! - **Engine** — the [`Language`] runtime bundle and its `parse()` entry,
//!   [`ParserSession`], the [`ParseDriver`] customization point ([`StdParseDriver`]),
//!   [`ParseResult`], the live parse-frame stack ([`Frame`], [`FrameTitle`],
//!   [`FrameRole`]), and the parsing-depth limiter ([`DescentGuard`],
//!   [`StdDescentGuard`] with its [`StdDescentGuardInit`] configuration).
//!
//! Four submodules hold the subsets with clear boundaries:
//!
//! - [`token`] — the tokenization library: a language's tokenization declared as one
//!   type, the token and stream-position types it names, the
//!   [`TokenReader`](token::TokenReader) trait and the standard reader, the
//!   [`TokenRules`](token::TokenRules) data with its overrides and derived caches, and
//!   the token errors.
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
pub mod token;

pub use crate::engine::{
    CommandResolver, DescentGuard, DescentRefusal, DescentWarning, Frame, FrameTitle,
    Language, ParseDriver, ParseResult, ParserSession, SessionDeriveError,
    StdDescentGuard, StdDescentGuardInit, StdParseDriver,
};
pub use crate::spec::FrameRole;
pub use crate::state::{
    AllLangFeatures, ClosedVocabulary, DeriveError, FeatureAbsent, FeaturePresence,
    FeaturePresent, FinalizeError, InvocationSyntax, Lang, LangFeatures, LangHasCommands,
    LangHasComments, LangHasForbiddenChars, LangHasGroups, LangHasParagraphs,
    LangHasScopes, LangHasSpecials, LangHasWhitespace, NodeExtTypes, NoLangFeatures,
    ParsingState, ParsingStateDelta, ParsingStateStack, StateData, TrivialLang,
};
