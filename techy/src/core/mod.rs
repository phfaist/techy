//! The parsing machinery: what defines a language, the conditions a parse runs
//! under, and the engine that turns source text into a node tree.
//!
//! Most programs enter here through [`Language`]. A `Language` value pairs the
//! [`ParsingState`] that parses start from with a [`ParseDriver`], the object holding
//! a language's parse-time behavior; [`Language::parse`] then reads a string and
//! returns a [`ParseResult`] carrying the finished tree and the diagnostics recorded
//! along the way. If you only want to parse LaTeX-like input, take the ready-made
//! [`Latexlike`](crate::latexlike::Latexlike) language from the
//! [`latexlike`](crate::latexlike) preset and read
//! [Running the parser](crate::guide::parsing).
//!
//! # What a language is
//!
//! A language is a type implementing [`Lang`]. The trait fixes, at compile time, the
//! types a language parses with: the identifiers it uses for group classes and
//! callables, its modes, its tokenization, and the extra data it attaches to states,
//! nodes, and sessions. [`LangFeatures`] declares which syntactic features the
//! language has at all — groups, commands, comments, whitespace, and so on — so that a
//! language that has no comments pays nothing for them. [`TrivialLang`] supplies
//! defaults for every one of those choices, which is enough for tests and for
//! languages with no vocabularies of their own.
//!
//! # The parsing state
//!
//! A [`ParsingState`] is the complete set of conditions in force at one point of a
//! parse: the token rules currently in effect, the current mode, and the definitions
//! in scope. States are immutable and shared. To change one, describe the change as a
//! [`ParsingStateDelta`] — token-rule overrides, a mode switch, scope operations — and
//! derive a new state from it; the underlying data is [`StateData`]. Every node in a
//! finished tree records the state it was parsed under, so a fragment can later be
//! re-parsed under exactly the same conditions.
//!
//! # The engine
//!
//! One parse runs as follows. [`Language::parse`] (or the configurable
//! [`parse_setup`](Language::parse_setup) form) creates a [`ParserSession`], which
//! accumulates everything that one parse produces: the nodes being staged, the
//! diagnostics, and a stack of [`Frame`]s recording which constructs are currently
//! open, so that a reported problem can name the enclosing group or macro. Construct
//! parsers from [`constructs`] then read tokens and stage nodes, calling one another
//! for nested constructs; [`StdDescentGuard`] caps how deep that nesting may go.
//! Finally the session freezes into the [`ParseResult`].
//!
//! Customization happens on the driver: implementing [`ParseDriver`] is how a language
//! resolves command names, chooses whether a problem aborts the parse or is recorded
//! and tolerated, and substitutes parsers of its own. Every method has a default, and
//! [`StdParseDriver`] is a ready-made implementation configured with a recovery policy
//! and a command resolver. [Defining a language](crate::guide::custom_lang) covers
//! this end of the API.
//!
//! # The four submodules
//!
//! - [`token`] — turning source text into tokens: the
//!   [`TokenRules`](token::TokenRules) that say what counts as a group delimiter, a
//!   command, or a comment; the [`TokenReader`](token::TokenReader) that produces
//!   tokens from those rules; and the token errors.
//! - [`specs`] — defining what a callable does: the specs that describe a macro or an
//!   environment and the arguments it takes, and the providers, packages, and scopes
//!   that hold those definitions and resolve a name to one.
//! - [`constructs`] — parsing one construct at a time: the [`ConstructParser`]
//!   contract, the standard parsers the engine uses for content, groups, and
//!   invocations, and the conditions they report.
//! - [`node`] — the node tree: reading it, its per-kind payloads, and building one
//!   directly.
//!
//! The data models a parse produces are at the crate root — [`source`](crate::source)
//! for source text and positions, [`error`](crate::error) for diagnostics — together
//! with the tools that consume a finished tree, such as [`extract`](crate::extract).
//!
//! [`ConstructParser`]: constructs::ConstructParser

pub mod constructs;
pub mod node;
pub mod specs;
pub mod token;

pub use crate::engine::{
    CommandResolver, DescentGuard, DescentRefusal, DescentWarning, Frame, FrameTitle,
    Language, ParseDriver, ParseResult, ParseSetup, ParserSession, SessionDeriveError,
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
