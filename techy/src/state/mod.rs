//! Parsing state: materialized state data behind a single transition choke point.
//!
//! The state model:
//!
//! - [`ParsingState<L>`] holds [`StateData<L>`] (the plain stored settings:
//!   [`TokenRules`](crate::token::TokenRules), the first-class parsing mode
//!   `L::ModeId`, and the language-specific `L::StateExt`) behind a getter-only public
//!   surface, together with per-instance derived caches (the delimiter
//!   [`PrefixTable`](crate::token::PrefixTable) and the specials
//!   [`TriggerChars`](crate::token::TriggerChars) set).
//! - [`ParsingStateDelta<L>`] is the reified change value — typed optional overrides
//!   ([`TokenRulesOverrides`], the mode channel) plus semantic `L::Event`s. Deltas are
//!   data, not closures: mergeable, inspectable, and applicable by a *caller* to a base
//!   state the producer never saw (the producer/scope split).
//! - [`ParsingState::derived`] is the **sole constructor of non-initial states**: it
//!   applies the overrides, runs the [`Lang::finalize_transition`] customizer exactly
//!   once, and freezes the result (caches rebuilt). Cross-cutting rules ("in math mode
//!   the escape char is `#`") live in the customizer, not in delta writers. The seed
//!   state comes only from [`ParsingState::initial`], which freezes the language's
//!   canonical [`Lang::initial_state_data`] — the data→state step is crate-owned, so no
//!   caller can assemble a state that bypasses the choke point (seed coherence itself is
//!   the `Lang` author's contract; see the hook's docs).
//! - [`Lang`] is the compile-time bundle (one generic parameter everywhere). It also
//!   carries the two token-level hooks, [`Lang::scan_specials`] and
//!   [`Lang::specials_trigger_chars`] — specials recognition is the one part of
//!   tokenization delegated to the language rather than enumerated as rules data.
//!
//! The state also stores the definitions visible at this point of the parse
//! ([`ScopeStack`](crate::scopes::ScopeStack)): construct parsers extend or
//! reshape definitions mid-parse by returning a delta carrying
//! [`scope_ops`](ParsingStateDelta::scope_ops) (`\newcommand`, package loads), and
//! scopes revert structurally when the caller drops the derived state. Scope ops can
//! fail, which makes [`derived()`](ParsingState::derived) fallible — see
//! [`DeriveError`].
//!
//! [`Lang::NodeExts`] selects the node extension type bundle ([`NodeExtTypes`]);
//! [`SimpleLang`] is the all-defaults shortcut for languages with no customization.

mod delta;
mod lang;
mod parsing_state;

pub use delta::{ParsingStateDelta, TokenRulesOverrides};
pub use lang::{ClosedVocabulary, Lang, NodeExtTypes, SimpleLang};
pub use parsing_state::{DeriveError, ParsingState, StateData};
