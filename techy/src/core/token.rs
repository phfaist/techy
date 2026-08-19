//! The tokenization library: token types, the token reader, the token rules, and the
//! token errors.
//!
//! A **token** is the smallest unit of input a parser asks for: an atomic value naming
//! what to parse next, and nothing more. Tokens hold no readable data — a construct
//! parser holds and passes them around, and asks the reader that produced a token what
//! that token *is* ([`TokenReader::token_kind`], which answers a [`TokenKind`] view) and
//! where it is ([`TokenReader::source_span_of`], and the spans and stream positions taken
//! at a [`TokenEdge`]). A language declares its tokenization as one type,
//! [`Tokenization`], named as [`Lang::Tokenization`](crate::core::Lang::Tokenization):
//! the token type its readers produce, the type naming a place in the token stream, and
//! how the reader for one parse is built. [`StdTokenization`] is the standard
//! declaration, and [`StdTokenReader`] the reader it names.
//!
//! # What lives here
//!
//! `core::token` holds what a token reader produces, consumes and answers with — the
//! token and stream-position types, the `TokenReader` trait and the standard reader, the
//! scan helpers, the token rules the reader reads together with the overrides that change
//! them mid-parse and the caches derived from them, the types the specials-scan hooks
//! answer with, and the token conditions and errors. The [hub](crate::core) keeps the
//! `Lang` trait (its associated types and hooks), the parsing state and its deltas, and
//! the engine.
//!
//! # The items, by group
//!
//! - **The tokenization declaration** — [`Tokenization`] and the standard
//!   [`StdTokenization`], spelled everywhere else through the two aliases
//!   [`Token<L>`](Token) and [`StreamPosition<L>`](StreamPosition).
//! - **Token values and views** — the standard token [`StdToken`] and the standard stream
//!   position [`StdStreamPosition`]; [`TokenKind`], the view a reader answers a token's
//!   identity with; [`TokenEdge`], which names one of a token's five boundaries.
//! - **The reader** — the [`TokenReader`] trait that every reader implements and the
//!   parser side calls, the standard [`StdTokenReader`], and [`skip_whitespace`], the
//!   whitespace-skipping primitive that never crosses a paragraph break.
//! - **Token rules** — [`TokenRules`], the tokenization data a parsing state holds, one
//!   block per tokenization feature: [`WhitespaceRules`], [`ParagraphRules`],
//!   [`GroupRules`] of [`GroupRule`]s, [`CommandRules`] of [`CommandRule`]s,
//!   [`CommentRules`] of [`CommentRule`]s, [`SpecialsRules`], and
//!   [`ForbiddenCharsRules`]. Two families come with them:
//!   - the overrides a parsing-state delta carries to change the rules mid-parse —
//!     [`TokenRulesOverrides`] and its per-block [`WhitespaceOverrides`],
//!     [`ParagraphOverrides`], [`GroupOverrides`], [`CommandOverrides`],
//!     [`CommentOverrides`], [`SpecialsOverrides`], [`ForbiddenCharsOverrides`];
//!   - the caches a parsing state derives at each state transition: the group-delimiter
//!     [`PrefixTable`] of [`PrefixEntry`]s, and [`TriggerChars`], the filter saying which
//!     characters a specials match may start with.
//! - **What a specials scan answers with** — [`SpecialsMatch`] for a match and
//!   [`SpecialsScanError`] for a failure of
//!   [`Lang::scan_specials`](crate::core::Lang::scan_specials), the hook a reader consults
//!   for callable-triggering character sequences.
//! - **Errors and conditions** — [`TokenError`], its [`TokenErrorKind`], the
//!   [`TokenRecovery`] a recoverable condition offers (a placeholder token plus the stream
//!   position to resume at), and the [`TokenResult`] alias; the two diagnostic conditions
//!   a reader reports are [`EndOfStreamAfterEscape`] and [`ForbiddenChar`].
//!
//! # Writing a token reader
//!
//! There are three ways to give a language its tokenization, in increasing order of work.
//!
//! **Rules data only — no reader at all.** A language whose tokenization differs from the
//! standard one only in *which* characters play which role declares
//! [`StdTokenization`] and seeds its parsing state with its own [`TokenRules`]: the escape
//! character(s) of its commands, its group delimiters, its comment delimiters, its
//! whitespace and paragraph settings, its forbidden characters. The preset's
//! [`default_token_rules`](crate::latexlike::default_token_rules) is the worked example —
//! the LaTeX-like rule set, built block by block — and
//! [`TokenRules::empty()`](TokenRules::empty) with struct-update syntax is the starting
//! point for a set of one's own. Nothing else has to be written: [`StdTokenReader`] reads
//! whatever the rules say.
//!
//! **A reader over standard tokens.** A language whose tokenization *behavior* differs —
//! it decides differently which token comes next, re-classifying a character or splicing
//! in content — implements [`TokenReader`] itself while keeping [`StdToken`] as its token
//! type: hold an inner [`StdTokenReader`] over the same content, build tokens with the
//! [`StdToken`] constructors, and delegate every interpretive method to the inner reader.
//! [*Writing a reader over standard tokens*](TokenReader#writing-a-reader-over-standard-tokens)
//! on the [`TokenReader`] page states how that delegation goes and shows a complete
//! example. A reader that instead declares a token type of its own — one that wraps
//! standard tokens read from one or several sources, as a macro expander does — keeps one
//! inner [`StdTokenReader`] per source; what the stream positions on the two sides of a
//! source change mean is
//! [*Seams*](TokenReader#seams--readers-that-serve-several-sources-at-one-nesting-level)
//! on the same page, and reading tokens from several sources during one parse requires
//! the language to declare
//! [`Lang::OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING) `= false`.
//!
//! **A reader with its own token kinds.** The scan helpers are described with the
//! functions themselves.

pub use crate::state::{
    CommandOverrides, CommentOverrides, ForbiddenCharsOverrides, GroupOverrides,
    ParagraphOverrides, SpecialsOverrides, TokenRulesOverrides, WhitespaceOverrides,
};
pub use crate::token::{
    command_rule_at, scan_command, scan_comment, scan_group_delimiter, scan_paragraph_break,
    scan_specials_trigger, skip_whitespace, CommandMatch, CommandRule, CommandRules,
    CommentMatch, CommentRule, CommentRules, EndOfStreamAfterEscape, ForbiddenChar,
    ForbiddenCharsRules, GroupDelimiterMatch, GroupRule, GroupRules, ParagraphRules,
    PrefixEntry, PrefixTable, SpecialsMatch, SpecialsRules, SpecialsScanError,
    StdStreamPosition, StdToken, StdTokenReader, StdTokenization, StreamPosition, Token,
    TokenEdge, TokenError, TokenErrorKind, TokenKind, TokenReader, TokenRecovery,
    TokenResult, TokenRules, Tokenization, TriggerChars, WhitespaceRules,
};
