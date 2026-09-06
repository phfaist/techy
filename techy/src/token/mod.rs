//! Tokenization: the token types, the token reader, the token rules, and the scan
//! helpers behind them.
//!
//! Tokenizing turns source text into **tokens**: minimal, opaque values that each name
//! one thing to parse next — one character, a group delimiter, a command, a specials
//! trigger, a whole comment, a paragraph break, or the end of the input. Nothing is read
//! off a token directly: a construct parser holds tokens and passes them back to the
//! [`TokenReader`] that produced them, asking what a token *is*
//! ([`TokenReader::token_kind`], answered as a [`TokenKind`] view) and where it is
//! ([`TokenReader::source_span_of`], and the spans and stream positions taken at a
//! [`TokenEdge`]).
//!
//! The types, in the order a reader meets them:
//!
//! - [`Tokenization`] is how a language declares its tokenization — as one bundle, named
//!   as [`Lang::Tokenization`](crate::core::Lang::Tokenization): the token type, the type
//!   naming a place in the token stream, and how the reader for one parse is built.
//!   Elsewhere those two types are spelled [`Token<L>`](Token) and
//!   [`StreamPosition<L>`](StreamPosition). [`StdTokenization`] is the standard bundle:
//!   [`StdToken`], [`StdStreamPosition`], [`StdTokenReader`].
//! - [`TokenKind`] is the parser-facing view of a token: the closed set of what a token
//!   can be, with the spellings the reader matched.
//! - [`TokenReader`] is the trait every reader implements and the parser side calls.
//!   [`StdTokenReader`] is the standard implementation; it recognizes constructs by
//!   composing the scan helpers ([`skip_whitespace`], [`scan_paragraph_break`],
//!   [`scan_group_delimiter`], [`command_rule_at`], [`scan_command`], [`scan_comment`],
//!   [`scan_specials_trigger`]), free functions a reader of one's own may compose
//!   differently.
//! - [`TokenRules`] is the data a reader works from — which characters are whitespace,
//!   which delimiters open groups, which characters start commands and comments. It is
//!   held in the parsing state, so it can change mid-parse; [`PrefixTable`] and
//!   [`TriggerChars`] are the caches derived from it for the two lookups that run at
//!   every position.
//! - [`TokenError`] reports a condition met while reading, optionally with a
//!   [`TokenRecovery`]: a placeholder token and the stream position to resume at.
//!   Whether a parse stops there or continues with the placeholder is decided by the
//!   session's [`Recovery`](crate::error::Recovery) policy, not by the reader.
//!
//! Four properties of this token model shape the parsers written against it:
//!
//! - A [`Char`](TokenKind::Char) token covers exactly one character. Consecutive
//!   characters are joined into a single node later, so a parser that must read
//!   character by character (a tabular preamble, say) can.
//! - A command token says only that a name was written after an escape character, not
//!   what that name means: `\begin` is a [`Command`](TokenKind::Command) token like
//!   `\foobar`, and which names are macros, environments, or anything else is decided at
//!   parse time. [`Specials`](TokenKind::Specials) is the exception — recognizing a
//!   trigger there is already resolving it, so the token holds the spec it matched.
//! - Whitespace before a token (its *pre-space*) is content and reaches the node tree;
//!   whitespace a command or a comment absorbs after itself (its *post-space*) is syntax
//!   and does not. [`skip_whitespace`] applies the paragraph rule to both: skipped
//!   whitespace never consumes a newline that belongs to a paragraph break.
//! - Every stream ends with a terminal [`EndOfStream`](TokenKind::EndOfStream) token
//!   whose pre-space carries the input's final whitespace, so reading a token never
//!   answers an `Option`.
//!
//! [Language syntax](crate::guide::language_syntax) describes the constructs these
//! tokens stand for, [Tokens and token
//! rules](crate::guide::concepts_overview#tokens-and-token-rules) places them in the
//! parsing model, and [Defining a custom
//! language](crate::guide::custom_lang#token-rules-and-specials-recognition) covers
//! writing rules and specials recognition for a language of one's own.

mod error;
#[cfg(test)]
mod list_reader;
mod prefix_table;
mod reader;
mod rules;
mod scan;
#[cfg(test)]
mod scripted_reader;
mod specials;
mod tokenization;
// The submodule sharing the parent's name is deliberate: the token is this topic's
// anchor type, and the submodule is private (everything is re-exported here).
#[allow(clippy::module_inception)]
mod token;

pub use error::{
    EndOfStreamAfterEscape, ForbiddenChar, TokenError, TokenErrorKind, TokenRecovery,
    TokenResult,
};
// Internal test infrastructure, not public API (decided July 2026, Action-02
// follow-up): its role is the lockstep reader-agreement harness of the construct-parser
// test suites, and its fixed-list fidelity gap (no re-tokenization under the peek
// state) makes it unfit as a public reader contract.
#[cfg(test)]
pub(crate) use list_reader::TokenListReader;
pub use prefix_table::{PrefixEntry, PrefixTable};
pub use reader::{StdStreamPosition, StdTokenReader, TokenEdge, TokenReader};
// Internal test infrastructure, not public API (like `TokenListReader` above): the
// scripted multi-source reader, its tokenization types, and the test language that
// declares them. It serves one parse from several sources, which is what a language
// with `Lang::OBEYS_SPAN_TILING = false` allows and nothing else in the crate does.
#[cfg(test)]
pub(crate) use scripted_reader::{
    RelaxedLang, ScriptedPosition, ScriptedReader, ScriptedToken, ScriptedTokenization,
};
pub use rules::{
    CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
    GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
};
pub use scan::{
    command_rule_at, scan_command, scan_comment, scan_group_delimiter, scan_paragraph_break,
    scan_specials_trigger, skip_whitespace, CommandMatch, CommentMatch, GroupDelimiterMatch,
};
pub use specials::{SpecialsMatch, SpecialsScanError, TriggerChars};
pub use token::{StdToken, TokenKind};
pub use tokenization::{StdTokenization, StreamPosition, Token, Tokenization};
