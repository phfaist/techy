//! Parsing state: what is in effect at a point in a parse, and how it changes.
//!
//! A [`ParsingState<L>`] is an immutable snapshot of everything that can vary while
//! parsing: the stored settings ([`StateData<L>`] — the
//! [tokenization rules](crate::core::token::TokenRules), the parsing mode `L::ModeId`,
//! and the language's own `L::StateExt`), the definitions visible at that point
//! ([`ScopeStack`](crate::core::specs::ScopeStack)), and the lookup caches derived
//! from them (the delimiter [`PrefixTable`](crate::core::token::PrefixTable) and the
//! specials [`TriggerChars`](crate::core::token::TriggerChars) filter). A state is
//! read through getters and is never modified once built.
//!
//! A state comes into existence in exactly two ways:
//!
//! - [`ParsingState::lang_initial`] freezes the language's canonical
//!   [`Lang::initial_state_data`] into the seed state. It is the only source of a
//!   first state: turning data into a state is this crate's job, so a caller cannot
//!   assemble a state any other way.
//! - [`ParsingState::derived`] produces every other state. It applies a delta's
//!   overrides, runs [`Lang::finalize_transition`] exactly once, and freezes the
//!   result. Because every transition goes through it, a rule the language wants to
//!   hold in every state — "in math mode the escape character is `#`" — is written
//!   once in `finalize_transition` instead of in every piece of code that writes a
//!   delta.
//!
//! A change is described by a [`ParsingStateDelta<L>`]: typed optional overrides
//! ([`TokenRulesOverrides`], the mode and state-extension overrides) plus semantic
//! `L::Event`s. A delta is plain data rather than a closure, so it can be merged with
//! another delta, inspected, stored, and applied by a *caller* to a base state its
//! producer never saw. That is what lets a construct parser return the change it
//! wants and leave the caller to decide whether it applies to the following siblings
//! or is dropped with the enclosing group.
//!
//! Definitions change the same way: a delta whose
//! [`scope_ops`](ParsingStateDelta::scope_ops) push a provider or add a definition
//! (`\newcommand`, package loads) makes them visible from the derived state onward,
//! and leaving the scope restores the previous definitions because the caller still
//! holds the previous state. Scope ops can fail, which is why
//! [`derived()`](ParsingState::derived) returns a `Result` — see [`DeriveError`].
//!
//! [`Lang`] is the trait a language definition implements: it names every type the
//! machinery is generic over and supplies the language's hooks. Its
//! [`Lang::Features`] member declares, feature by feature, whether the language has
//! that parsing feature at all ([`LangFeatures`], the [`FeaturePresence`] vocabulary
//! and the per-feature `LangHas*` bound traits); [`Lang::NodeExts`] selects the node
//! extension types ([`NodeExtTypes`]); and [`Lang::scan_specials`] with
//! [`Lang::specials_trigger_chars`] implement specials recognition, the one part of
//! tokenization a language recognizes itself instead of describing as rules data.
//! [`TrivialLang`] is an all-defaults language for tests and for experiments with the
//! machinery.
//!
//! [The parsing model](crate::guide::parsing_model#how-parsing-state-flows) shows how
//! states flow through a parse, and [Defining a custom
//! language](crate::guide::custom_lang) walks through writing a [`Lang`].

mod delta;
mod features;
mod lang;
mod parsing_state;
mod stack;

pub use delta::{
    CommandOverrides, CommentOverrides, ForbiddenCharsOverrides, GroupOverrides,
    ParagraphOverrides, ParsingStateDelta, SpecialsOverrides, TokenRulesOverrides,
    WhitespaceOverrides,
};
pub use features::{
    AllLangFeatures, FeatureAbsent, FeaturePresence, FeaturePresent, LangFeatures,
    LangHasCommands, LangHasComments, LangHasForbiddenChars, LangHasGroups,
    LangHasParagraphs, LangHasScopes, LangHasSpecials, LangHasWhitespace, NoLangFeatures,
};
pub use lang::{ClosedVocabulary, InvocationSyntax, Lang, NodeExtTypes, TrivialLang};
pub use parsing_state::{DeriveError, FinalizeError, ParsingState, StateData};
pub use stack::ParsingStateStack;
