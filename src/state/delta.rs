//! [`ParsingStateDelta`] and [`TokenRulesOverrides`]: reified state changes.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::token::{CommandRule, CommentRule, GroupType, GroupTypeId, TokenRules, WhitespaceRules};

use super::lang::Lang;
use super::parsing_state::StateData;

/// Typed optional overrides of [`TokenRules`] — pylatexenc's "changed kwargs", reified.
/// `None` = leave unchanged; `Some(value)` = replace the whole field (including
/// `Some(None)` for the optional fields, meaning "disable the feature").
///
/// Collections are replaced wholesale, not merged: a delta that wants "current group
/// types plus one more" is built by the party that can see the current state (typically
/// via [`ParsingState::rules()`](super::ParsingState::rules)); merge semantics in the
/// override itself would smuggle policy into the choke point.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenRulesOverrides {
    /// Override whitespace handling (`Some(None)` disables it).
    pub whitespace: Option<Option<WhitespaceRules>>,
    /// Override the paragraph-break flag.
    pub double_newline_paragraphs: Option<bool>,
    /// Replace the recognizable group types.
    pub group_types: Option<Vec<GroupType>>,
    /// Replace the command syntaxes.
    pub commands: Option<Vec<CommandRule>>,
    /// Replace the comment syntaxes.
    pub comments: Option<Vec<CommentRule>>,
    /// Replace the forbidden-character set.
    pub forbidden_chars: Option<String>,
    /// Override the expected group close (`Some(None)` clears it).
    pub expecting_group_close: Option<Option<GroupTypeId>>,
}

impl TokenRulesOverrides {
    /// Apply these overrides to `rules`, leaving `None` fields untouched.
    pub fn apply(&self, rules: &mut TokenRules) {
        if let Some(v) = &self.whitespace {
            rules.whitespace = v.clone();
        }
        if let Some(v) = self.double_newline_paragraphs {
            rules.double_newline_paragraphs = v;
        }
        if let Some(v) = &self.group_types {
            rules.group_types = v.clone();
        }
        if let Some(v) = &self.commands {
            rules.commands = v.clone();
        }
        if let Some(v) = &self.comments {
            rules.comments = v.clone();
        }
        if let Some(v) = &self.forbidden_chars {
            rules.forbidden_chars = v.clone();
        }
        if let Some(v) = self.expecting_group_close {
            rules.expecting_group_close = v;
        }
    }
}

/// A reified state change: the argument of [`ParsingState::derived()`](super::ParsingState::derived).
///
/// Deltas are **values, not closures** — mergeable, inspectable, and propagatable to base
/// states their producer never saw (a construct parser returns its delta; the *caller*
/// decides the scope: apply to its own state for following siblings, or drop it with the
/// group). Standard overrides and semantic events travel together so one transition (and
/// one `finalize_transition` run) covers both.
///
/// `push_libraries` arrives with Phase 4.
pub struct ParsingStateDelta<L: Lang> {
    /// Overrides of the stored token rules; every field optional.
    pub rules: TokenRulesOverrides,
    /// Whole-value replacement of the language-specific state extension; generic code
    /// leaves this `None` (presets prefer events + `finalize_transition`).
    pub ext: Option<L::StateExt>,
    /// Semantic transition events, consumed by [`Lang::finalize_transition`].
    pub events: Vec<L::Event>,
}

impl<L: Lang> ParsingStateDelta<L> {
    /// An empty delta (deriving with it yields an equivalent state).
    pub fn new() -> ParsingStateDelta<L> {
        ParsingStateDelta { rules: TokenRulesOverrides::default(), ext: None, events: Vec::new() }
    }

    /// Set the token-rules overrides.
    pub fn rules(mut self, rules: TokenRulesOverrides) -> Self {
        self.rules = rules;
        self
    }

    /// Set the state-extension replacement.
    pub fn ext(mut self, ext: L::StateExt) -> Self {
        self.ext = Some(ext);
        self
    }

    /// Add a semantic transition event.
    pub fn event(mut self, event: L::Event) -> Self {
        self.events.push(event);
        self
    }

    /// Apply overrides (rules + ext) to `data`. Internal, pre-freeze: called only from
    /// `derived()`, before `finalize_transition` runs.
    pub(crate) fn apply_overrides(&self, data: &mut StateData<L>) {
        self.rules.apply(&mut data.rules);
        if let Some(ext) = &self.ext {
            data.ext = ext.clone();
        }
    }
}

impl<L: Lang> Default for ParsingStateDelta<L> {
    fn default() -> Self {
        ParsingStateDelta::new()
    }
}

// Manual impls to avoid spurious `L:` bounds (associated types are bounded in `Lang`).

impl<L: Lang> Clone for ParsingStateDelta<L> {
    fn clone(&self) -> Self {
        ParsingStateDelta {
            rules: self.rules.clone(),
            ext: self.ext.clone(),
            events: self.events.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsingStateDelta<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsingStateDelta")
            .field("rules", &self.rules)
            .field("ext", &self.ext)
            .field("events", &self.events)
            .finish()
    }
}
