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
//! A **scan helper**, one of the item groups that rule places here, is a free function
//! that recognizes one construct at a byte offset in the text being scanned and answers
//! what it found — byte ranges into that text ([`Span`](crate::source::Span)s), plus the
//! rule or the specification that matched — or nothing. A helper advances no position,
//! builds no token, and never sees the [`Source`](crate::source::Source) the text came
//! from. *Writing a token reader* below lists the seven of them.
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
//! inner [`StdTokenReader`] per source and works through two methods of it, which need no
//! tokenization declaration of their own:
//! [`scan_std_token_at`](StdTokenReader::scan_std_token_at) reads the standard token at an
//! offset without moving that inner reader, and
//! [`token_kind_of_std_token`](StdTokenReader::token_kind_of_std_token) answers what one of
//! the standard tokens it stores is — the trait method
//! [`token_kind`](TokenReader::token_kind) is out of reach for such a language, since the
//! implementation of [`TokenReader`] for [`StdTokenReader`] serves only languages tokenized
//! in [`StdToken`]/[`StdStreamPosition`]. What the stream positions on the two sides of a
//! source change mean is
//! [*Seams*](TokenReader#seams--readers-that-serve-several-sources-at-one-nesting-level)
//! on the same page, and one reader serving several sources at one nesting level requires
//! the language to declare
//! [`Lang::OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING) `= false`.
//!
//! **A reader with its own token kinds.** A language whose tokens are not the standard
//! ones at all — its own kinds, carrying its own data — implements [`TokenReader`] over a
//! token type of its own, and composes the scan helpers for whichever constructs it wants
//! recognized the way the standard reader recognizes them: [`skip_whitespace`] for a
//! whitespace run, [`scan_paragraph_break`] for a paragraph break,
//! [`scan_group_delimiter`] for a group delimiter (answering a [`GroupDelimiterMatch`]),
//! [`command_rule_at`] and then [`scan_command`] for a command (a [`CommandMatch`]),
//! [`scan_comment`] for a whole comment (a [`CommentMatch`]), and
//! [`scan_specials_trigger`] for a specials trigger (a [`SpecialsMatch`]). Each answers
//! byte ranges and the rule that matched, and the reader builds whatever token it likes
//! from that. [`StdTokenReader::scan_std_token_at`] is itself written as a composition of
//! these seven, so a construct is recognized in one place and nowhere else; the order it
//! tries them in is documented there. Three of its steps are a line each rather than a
//! helper: the test for the end of the content, the forbidden-character test
//! ([`TokenRules::forbidden_chars`] answers the empty string for a language that declares
//! no such feature), and the fallback to a single content character.
//!
//! # Where a scan helper may be asked to look
//!
//! Every scan helper takes the text being scanned and a byte offset `pos` into it, and
//! every one of them requires `pos <= content.len()` with `pos` on a `char` boundary.
//! `pos == content.len()` is valid and names the end of the content: a helper answers
//! that nothing is there. A `pos` that violates the requirement panics, in all builds —
//! it is a mistake in the calling code, which no scanned text can cause, and it is one of
//! this crate's few deliberate panics (the [Panics list](crate::guide::panics) names them
//! all). A reader validates the offsets that reach it from its own caller once, where
//! they reach it — which is what [`StdTokenReader::scan_std_token_at`] does with `start`,
//! reporting an invalid one as an implementation error instead of panicking — and passes
//! offsets derived from a validated one to the helpers.

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
