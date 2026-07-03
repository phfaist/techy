//! Tokenization: zero-copy tokens, tokenization rules, and the standard token reader.
//!
//! This topic module spans two strata (ARCHITECTURE.md §3 — module = topic, not stratum):
//!
//! - **S0 (Lang-free), implemented here in Phase 2:** [`Span`], [`Token`]/[`TokenKind`],
//!   [`TokenRules`] and its derived [`PrefixTable`], the token-level error/recovery types
//!   ([`TokenError`], [`TokenRecovery`]), and the concrete scanning core inside
//!   [`StdTokenReader`]. All of it is driven by `&TokenRules` data and testable without
//!   inventing a language.
//! - **S1, arriving in Phase 3:** the `TokenReader<'s, L>` *trait* — the behavior extension
//!   point for genuinely different tokenization (catcode-like schemes). Its `peek`
//!   deliberately receives the full `ParsingState<L>` rather than `&TokenRules`, because a
//!   custom reader keeps its tables in `L::StateExt`; defining the trait against
//!   `&TokenRules` now would sever that escape hatch, so the trait waits for
//!   `ParsingState<L>`. [`StdTokenReader`]'s inherent API is already shaped to match it.
//!
//! # Design highlights
//!
//! - **Tokens are structural and minimal**: they identify *what to parse next*, nothing
//!   more. No begin/end-environment tokens, and comment tokens cover only the start
//!   delimiter — deliberate departures from pylatexenc (DESIGN_RATIONALE.md §3.2).
//! - **Zero-copy, ephemeral tokens**: `Token<'s>` borrows the current source unit's
//!   content; `pre_space`/`post_space` are [`Span`]s, not `String`s. The `'s` lifetime
//!   never enters the AST.
//! - **Data-driven tokenization**: [`StdTokenReader`] has no language knowledge; every
//!   behavior comes from [`TokenRules`] (stored in the parsing state from Phase 3 on).
//!   `$…$` vs `$$…$$` disambiguation is data too ([`TokenRules::expecting_group_close`]).
//! - **Tolerant parsing hooks**: recoverable conditions yield a [`TokenError`] carrying a
//!   [`TokenRecovery`] (placeholder token + resume position); the strict/tolerant decision
//!   belongs to the session's [`Recovery`](crate::error::Recovery) policy, not the reader.

mod error;
mod prefix_table;
mod reader;
mod rules;
mod span;
// The submodule sharing the parent's name is deliberate: `Token` is this topic's anchor
// type, and the submodule is private (everything is re-exported here).
#[allow(clippy::module_inception)]
mod token;

pub use error::{TokenError, TokenErrorKind, TokenRecovery, TokenResult};
pub use prefix_table::{PrefixEntry, PrefixTable};
pub use reader::StdTokenReader;
pub use rules::{CommentRules, GroupType, GroupTypeId, MacroRules, TokenRules, WhitespaceRules};
pub use span::Span;
pub use token::{Token, TokenKind};
