//! Parsing state: materialized state data behind a single transition choke point.
//!
//! This implements ARCHITECTURE.md §state ("Option C", Decision 1):
//!
//! - [`ParsingState<L>`] holds [`StateData<L>`] (the plain stored settings: [`TokenRules`]
//!   plus the language-specific `L::StateExt`) behind a getter-only public surface,
//!   together with per-instance derived caches (the delimiter [`PrefixTable`] and the
//!   specials [`TriggerChars`] set).
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
//! `LibraryStack` (definitions visible in a state) and `push_libraries` on the delta
//! arrive with Phase 4; `Lang::NodeExts` with Phase 5.

mod delta;
mod lang;
mod parsing_state;

pub use delta::{ParsingStateDelta, TokenRulesOverrides};
pub use lang::Lang;
pub use parsing_state::{ParsingState, StateData};
