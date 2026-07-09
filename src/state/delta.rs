//! [`ParsingStateDelta`] and [`TokenRulesOverrides`]: reified state changes.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::library::SpecLookup;
use crate::token::{CommandRule, CommentRule, GroupRule, TokenRules, WhitespaceRules};

use super::lang::Lang;
use super::parsing_state::StateData;

/// Typed optional overrides of [`TokenRules`] — pylatexenc's "changed kwargs", reified.
/// `None` = leave unchanged; `Some(value)` = replace the whole field.
///
/// The `enable_*` gates override independently of their data: disabling a feature for a
/// scope is `enable_commands: Some(false)`, and a later `Some(true)` re-enables it with
/// the *original* rules intact — no party has to carry them (DESIGN_RATIONALE.md §3.2).
///
/// Collections are replaced wholesale, not merged: a delta that wants "current group
/// rules plus one more" is built by the party that can see the current state (typically
/// via [`ParsingState::rules()`](super::ParsingState::rules)); merge semantics in the
/// override itself would smuggle policy into the choke point.
pub struct TokenRulesOverrides<L: Lang> {
    /// Override the whitespace-handling gate.
    pub enable_whitespace: Option<bool>,
    /// Replace the whitespace rules.
    pub whitespace: Option<WhitespaceRules>,
    /// Override the paragraph-break gate.
    pub enable_multi_newline_paragraphs: Option<bool>,
    /// Override the group-delimiter gate.
    pub enable_groups: Option<bool>,
    /// Replace the recognizable group delimiter rules.
    pub groups: Option<Vec<Arc<GroupRule<L>>>>,
    /// Override the command-syntax gate.
    pub enable_commands: Option<bool>,
    /// Replace the command syntaxes.
    pub commands: Option<Vec<CommandRule>>,
    /// Override the comment-syntax gate.
    pub enable_comments: Option<bool>,
    /// Replace the comment syntaxes.
    pub comments: Option<Vec<CommentRule>>,
    /// Override the specials-scan gate.
    pub enable_specials: Option<bool>,
    /// Replace the forbidden-character set.
    pub forbidden_chars: Option<String>,
    /// Override the expected group close (`Some(None)` clears it).
    pub expecting_group_close: Option<Option<Arc<GroupRule<L>>>>,
}

impl<L: Lang> TokenRulesOverrides<L> {
    /// Apply these overrides to `rules`, leaving `None` fields untouched.
    pub fn apply(&self, rules: &mut TokenRules<L>) {
        if let Some(v) = self.enable_whitespace {
            rules.enable_whitespace = v;
        }
        if let Some(v) = &self.whitespace {
            rules.whitespace = v.clone();
        }
        if let Some(v) = self.enable_multi_newline_paragraphs {
            rules.enable_multi_newline_paragraphs = v;
        }
        if let Some(v) = self.enable_groups {
            rules.enable_groups = v;
        }
        if let Some(v) = &self.groups {
            rules.groups = v.clone();
        }
        if let Some(v) = self.enable_commands {
            rules.enable_commands = v;
        }
        if let Some(v) = &self.commands {
            rules.commands = v.clone();
        }
        if let Some(v) = self.enable_comments {
            rules.enable_comments = v;
        }
        if let Some(v) = &self.comments {
            rules.comments = v.clone();
        }
        if let Some(v) = self.enable_specials {
            rules.enable_specials = v;
        }
        if let Some(v) = &self.forbidden_chars {
            rules.forbidden_chars = v.clone();
        }
        if let Some(v) = &self.expecting_group_close {
            rules.expecting_group_close = v.clone();
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
pub struct ParsingStateDelta<L: Lang> {
    /// Overrides of the stored token rules; every field optional.
    pub rules: TokenRulesOverrides<L>,
    /// Lookups to push onto the state's library stack, in order (the last one pushed
    /// ends up innermost). This is how definitions extend mid-parse (`\newcommand`);
    /// scope reversion is structural — the caller keeps the previous
    /// `Arc<ParsingState>` (ARCHITECTURE.md §specs).
    pub push_libraries: Vec<Arc<dyn SpecLookup<L>>>,
    /// Whole-value replacement of the language-specific state extension; generic code
    /// leaves this `None` (presets prefer events + `finalize_transition`).
    pub ext: Option<L::StateExt>,
    /// Semantic transition events, consumed by [`Lang::finalize_transition`].
    pub events: Vec<L::Event>,
}

impl<L: Lang> ParsingStateDelta<L> {
    /// An empty delta (deriving with it yields an equivalent state).
    pub fn new() -> ParsingStateDelta<L> {
        ParsingStateDelta {
            rules: TokenRulesOverrides::default(),
            push_libraries: Vec::new(),
            ext: None,
            events: Vec::new(),
        }
    }

    /// Set the token-rules overrides.
    pub fn rules(mut self, rules: TokenRulesOverrides<L>) -> Self {
        self.rules = rules;
        self
    }

    /// Push a lookup onto the state's library stack (innermost = pushed last).
    pub fn push_library(mut self, lookup: Arc<dyn SpecLookup<L>>) -> Self {
        self.push_libraries.push(lookup);
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
        for lookup in &self.push_libraries {
            data.libraries.push(lookup.clone());
        }
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

impl<L: Lang> Default for TokenRulesOverrides<L> {
    fn default() -> Self {
        TokenRulesOverrides {
            enable_whitespace: None,
            whitespace: None,
            enable_multi_newline_paragraphs: None,
            enable_groups: None,
            groups: None,
            enable_commands: None,
            commands: None,
            enable_comments: None,
            comments: None,
            enable_specials: None,
            forbidden_chars: None,
            expecting_group_close: None,
        }
    }
}

impl<L: Lang> Clone for TokenRulesOverrides<L> {
    fn clone(&self) -> Self {
        TokenRulesOverrides {
            enable_whitespace: self.enable_whitespace,
            whitespace: self.whitespace.clone(),
            enable_multi_newline_paragraphs: self.enable_multi_newline_paragraphs,
            enable_groups: self.enable_groups,
            groups: self.groups.clone(),
            enable_commands: self.enable_commands,
            commands: self.commands.clone(),
            enable_comments: self.enable_comments,
            comments: self.comments.clone(),
            enable_specials: self.enable_specials,
            forbidden_chars: self.forbidden_chars.clone(),
            expecting_group_close: self.expecting_group_close.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for TokenRulesOverrides<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRulesOverrides")
            .field("enable_whitespace", &self.enable_whitespace)
            .field("whitespace", &self.whitespace)
            .field("enable_multi_newline_paragraphs", &self.enable_multi_newline_paragraphs)
            .field("enable_groups", &self.enable_groups)
            .field("groups", &self.groups)
            .field("enable_commands", &self.enable_commands)
            .field("commands", &self.commands)
            .field("enable_comments", &self.enable_comments)
            .field("comments", &self.comments)
            .field("enable_specials", &self.enable_specials)
            .field("forbidden_chars", &self.forbidden_chars)
            .field("expecting_group_close", &self.expecting_group_close)
            .finish()
    }
}

impl<L: Lang> PartialEq for TokenRulesOverrides<L> {
    fn eq(&self, other: &Self) -> bool {
        self.enable_whitespace == other.enable_whitespace
            && self.whitespace == other.whitespace
            && self.enable_multi_newline_paragraphs == other.enable_multi_newline_paragraphs
            && self.enable_groups == other.enable_groups
            && self.groups == other.groups
            && self.enable_commands == other.enable_commands
            && self.commands == other.commands
            && self.enable_comments == other.enable_comments
            && self.comments == other.comments
            && self.enable_specials == other.enable_specials
            && self.forbidden_chars == other.forbidden_chars
            && self.expecting_group_close == other.expecting_group_close
    }
}

impl<L: Lang> Eq for TokenRulesOverrides<L> {}

impl<L: Lang> Clone for ParsingStateDelta<L> {
    fn clone(&self) -> Self {
        ParsingStateDelta {
            rules: self.rules.clone(),
            push_libraries: self.push_libraries.clone(),
            ext: self.ext.clone(),
            events: self.events.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsingStateDelta<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsingStateDelta")
            .field("rules", &self.rules)
            .field("push_libraries", &self.push_libraries)
            .field("ext", &self.ext)
            .field("events", &self.events)
            .finish()
    }
}
