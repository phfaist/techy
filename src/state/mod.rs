//! Parsing state: materialized state data behind a single transition choke point.
//!
//! This implements ARCHITECTURE.md §state ("Option C", Decision 1):
//!
//! - [`ParsingState<L>`] holds [`StateData<L>`] (the plain stored settings:
//!   [`TokenRules`](crate::token::TokenRules) plus the language-specific `L::StateExt`)
//!   behind a getter-only public surface, together with per-instance derived caches (the
//!   delimiter [`PrefixTable`](crate::token::PrefixTable) and the specials
//!   [`TriggerChars`](crate::token::TriggerChars) set).
//! - [`ParsingStateDelta<L>`] is the reified change value — typed optional overrides
//!   ([`TokenRulesOverrides`]) plus semantic `L::Event`s. Deltas are data, not closures:
//!   mergeable, inspectable, and applicable by a *caller* to a base state the producer
//!   never saw (the producer/scope split of ARCHITECTURE.md §state).
//! - [`ParsingState::derived`] is the **sole constructor of non-initial states**: it
//!   applies the overrides, runs the [`Lang::finalize_transition`] customizer exactly
//!   once, and freezes the result (caches rebuilt). Cross-cutting rules ("in math mode
//!   the escape char is `#`") live in the customizer, not in delta writers.
//! - [`Lang`] is the compile-time bundle (one generic parameter everywhere). It also
//!   carries the two token-level hooks, [`Lang::scan_specials`] and
//!   [`Lang::specials_trigger_chars`] — specials recognition is the one part of
//!   tokenization delegated to the language rather than enumerated as rules data
//!   (DESIGN_RATIONALE.md §3.2).
//!
//! The state also stores the definitions visible at this point of the parse
//! ([`LibraryStack`](crate::library::LibraryStack), Phase 4): construct parsers extend
//! definitions mid-parse by returning a delta with
//! [`push_library`](ParsingStateDelta::push_library) (`\newcommand`), and scopes revert
//! structurally when the caller drops the derived state.
//!
//! [`Lang::NodeExts`] selects the node extension type bundle ([`NodeExtTypes`], Phase 5);
//! [`SimpleLang`] is the all-defaults shortcut for languages with no customization.

mod delta;
mod lang;
mod parsing_state;

pub use delta::{ParsingStateDelta, TokenRulesOverrides};
pub use lang::{Lang, NodeExtTypes, SimpleLang};
pub use parsing_state::{ParsingState, StateData};
