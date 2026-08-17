//! Tokenization: zero-copy tokens, tokenization rules, and the standard token reader.
//!
//! The whole token topic lives in the **S1 core stratum**: tokens are
//! generic over `L: Lang` (a `Specials` token carries its resolution), the reader reads
//! the full [`ParsingState<L>`](crate::state::ParsingState), and token errors are free to
//! grow language/state context. The only tokenization-adjacent S0 type is the plain byte
//! range [`Span`](crate::source::Span), which lives in the source topic.
//!
//! # Design highlights
//!
//! - **Tokens are structural and minimal — and single-character for content**: a token is
//!   an atomic unit identifying *what to parse next*. [`TokenKind::Char`] covers exactly
//!   one character (construct parsers may need char-by-char reading); chars accumulate
//!   into nodes at the node level.
//! - **No invocation forms on parse-time-resolved tokens**: no macro/environment/specials
//!   taxonomy on [`Command`](TokenKind::Command) tokens, no begin/end-environment tokens.
//!   `\begin` is a `Command` like any other; what its name *means* is decided at parse
//!   time by the preset. ([`Specials`](TokenKind::Specials) is the scoped exception:
//!   recognition *is* resolution there, so it carries its `CallableTypeId` and spec.)
//! - **Two callable-trigger token kinds, split by mechanism**:
//!   [`Command`](TokenKind::Command) is recognized from [`CommandRule`] *data* in the
//!   rules; [`Specials`](TokenKind::Specials) is recognized by the
//!   [`Lang::scan_specials`](crate::state::Lang::scan_specials) *hook* (recognition =
//!   resolution: the token carries the spec, and the matched text is the name), gated by
//!   the state's cached [`TriggerChars`] filter.
//! - **Where tokens are is the reader's answer**: a token is *at* a place in the
//!   reader's stream, and only the reader can say where that is in text. A construct
//!   parser asks it — [`TokenReader::source_span_of`], `source_span_between` at a
//!   [`TokenEdge`], `position_here`/`position_at` — and never computes a location
//!   itself.
//! - **Syntactic vs. content whitespace**: `pre_space` (on every token) is content
//!   whitespace belonging to the document flow; post-space (only on
//!   [`Command`](TokenKind::Command) and [`Comment`](TokenKind::Comment)) is whitespace
//!   consumed by the construct's syntax and ignored as content. One primitive,
//!   [`skip_whitespace`], enforces the paragraph rule everywhere: skipped whitespace
//!   never consumes a newline of a `\n\s*\n` sequence.
//! - **A terminal [`EndOfStream`](TokenKind::EndOfStream) token** whose `pre_space`
//!   reports final whitespace; `peek` never returns an `Option`.
//! - **Tolerant parsing hooks**: recoverable conditions yield a [`TokenError`] carrying a
//!   [`TokenRecovery`] (placeholder token + the stream position to resume at); the
//!   strict/tolerant decision belongs to the session's
//!   [`Recovery`](crate::error::Recovery) policy, not the reader. A
//!   [`Lang::scan_specials`](crate::state::Lang::scan_specials) failure
//!   ([`SpecialsScanError`]) carries no recovery: the hook cannot describe one, so the
//!   reader reports it as unrecoverable.

mod error;
#[cfg(test)]
mod list_reader;
mod prefix_table;
mod reader;
mod rules;
mod specials;
// The submodule sharing the parent's name is deliberate: `Token` is this topic's anchor
// type, and the submodule is private (everything is re-exported here).
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
pub use reader::{skip_whitespace, StdStreamPosition, StdTokenReader, TokenEdge, TokenReader};
pub use rules::{
    CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
    GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
};
pub use specials::{SpecialsMatch, SpecialsScanError, TriggerChars};
pub use token::{Token, TokenKind, TokenKindView};
