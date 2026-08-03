//! [`ParsingStateDelta`] and [`TokenRulesOverrides`]: reified state changes.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::scopes::{ScopeOp, ScopeOpError, SpecsProvider};
use crate::token::{CommandRule, CommentRule, GroupRule, TokenRules, WhitespaceRules};

use super::lang::Lang;
use super::parsing_state::StateData;

/// Typed optional overrides of [`TokenRules`] — pylatexenc's "changed kwargs", reified.
/// `None` = leave unchanged; `Some(value)` = replace the whole field.
///
/// The `enable_*` gates override independently of their data: disabling a feature for a
/// scope is `enable_commands: Some(false)`, and a later `Some(true)` re-enables it with
/// the *original* rules intact — no party has to carry them.
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
    /// Replace the temporary (scoped-lifecycle) group rules
    /// ([`TokenRules::temporary_groups`]). An explicit override wins over the
    /// derivation-path stripping rule: a delta that sets this field *and* installs an
    /// [`expecting_group_close`](TokenRules::expecting_group_close) keeps exactly the
    /// list it names (see [`ParsingState::derived`](super::ParsingState::derived)).
    pub temporary_groups: Option<Vec<Arc<GroupRule<L>>>>,
    /// Override the command-syntax gate.
    pub enable_commands: Option<bool>,
    /// Replace the command syntaxes.
    pub commands: Option<Vec<Arc<CommandRule>>>,
    /// Override the comment-syntax gate.
    pub enable_comments: Option<bool>,
    /// Replace the comment syntaxes.
    pub comments: Option<Vec<Arc<CommentRule>>>,
    /// Override the specials-scan gate.
    pub enable_specials: Option<bool>,
    /// Replace the forbidden-character set.
    pub forbidden_chars: Option<Arc<str>>,
    /// Override the expected group close (`Some(None)` clears it).
    pub expecting_group_close: Option<Option<Arc<GroupRule<L>>>>,
}

impl<L: Lang> TokenRulesOverrides<L> {
    /// The overrides value with all six `enable_*` gates `Some(false)` (whitespace,
    /// multi-newline paragraphs, groups, commands, comments, specials) and every other
    /// field untouched — the raw-state block a rest-of-line or verbatim-like takeover
    /// parser starts from. It composes: tweak fields afterwards, e.g. install the
    /// terminator that ends the raw region
    /// ([`verbatim_state_delta`](crate::constructs::verbatim_state_delta) is exactly
    /// this plus its [`expecting_group_close`](TokenRules::expecting_group_close)).
    ///
    /// This is the *scoped* off — the gates flip while the rules data stays in place,
    /// so a later delta can re-enable a feature with its original rules. The
    /// *constitutive* off (a language without the features at all) is
    /// [`TokenRules::empty`](crate::token::TokenRules::empty).
    pub fn disable_all() -> TokenRulesOverrides<L> {
        TokenRulesOverrides {
            enable_whitespace: Some(false),
            enable_multi_newline_paragraphs: Some(false),
            enable_groups: Some(false),
            enable_commands: Some(false),
            enable_comments: Some(false),
            enable_specials: Some(false),
            ..TokenRulesOverrides::default()
        }
    }

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
        if let Some(v) = &self.temporary_groups {
            rules.temporary_groups = v.clone();
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
    /// Scope-stack operations, applied in order: stack-shape ops and definition ops routed to a named
    /// provider — see [`ScopeOp`]. This is how definitions extend mid-parse
    /// (`\newcommand`); scope reversion is structural — the caller keeps the previous
    /// `Arc<ParsingState>`. Ops can **fail** (absent target
    /// name, immutable provider): failures are collected per op — the rest still
    /// apply — and surface through the fallible
    /// [`derived()`](super::ParsingState::derived).
    pub scope_ops: Vec<ScopeOp<L>>,
    /// Override the parsing mode ([`StateData::mode`]); `None` = leave unchanged.
    /// The override *is* the mode-change signal:
    /// [`Lang::finalize_transition`] sees it applied on the new data and interprets it
    /// against the previous state's [`mode()`](super::ParsingState::mode) — no
    /// [`Lang::Event`] needed for mode-shaped transitions.
    pub mode: Option<L::ModeId>,
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
            scope_ops: Vec::new(),
            mode: None,
            ext: None,
            events: Vec::new(),
        }
    }

    /// Set the token-rules overrides.
    pub fn rules(mut self, rules: TokenRulesOverrides<L>) -> Self {
        self.rules = rules;
        self
    }

    /// Add a scope-stack operation (ops apply in the order added).
    pub fn scope_op(mut self, op: ScopeOp<L>) -> Self {
        self.scope_ops.push(op);
        self
    }

    /// Push a provider onto the state's scope stack (innermost = pushed last) — sugar
    /// for the dominant [`ScopeOp::Push`] shape.
    pub fn push_provider(mut self, provider: Arc<dyn SpecsProvider<L>>) -> Self {
        self.scope_ops.push(ScopeOp::Push(provider));
        self
    }

    /// Set the parsing-mode override.
    pub fn mode(mut self, mode: L::ModeId) -> Self {
        self.mode = Some(mode);
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

    /// Apply overrides (rules + scope ops + mode + ext) to `data`. Internal, pre-freeze:
    /// called only from `derived()`, before `finalize_transition` runs. Scope-op
    /// failures are collected (the failing op is skipped, the rest still apply) and
    /// returned for `derived()` to report — an empty vec is full success.
    pub(crate) fn apply_overrides(&self, data: &mut StateData<L>) -> Vec<ScopeOpError> {
        self.rules.apply(&mut data.rules);
        let mut failures = Vec::new();
        for op in &self.scope_ops {
            if let Err(failure) = data.scopes.apply_op(op) {
                failures.push(failure);
            }
        }
        if let Some(mode) = self.mode {
            data.mode = mode;
        }
        if let Some(ext) = &self.ext {
            data.ext = ext.clone();
        }
        failures
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
            temporary_groups: None,
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
            temporary_groups: self.temporary_groups.clone(),
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
            .field("temporary_groups", &self.temporary_groups)
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
            && self.temporary_groups == other.temporary_groups
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
            scope_ops: self.scope_ops.clone(),
            mode: self.mode,
            ext: self.ext.clone(),
            events: self.events.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsingStateDelta<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsingStateDelta")
            .field("rules", &self.rules)
            .field("scope_ops", &self.scope_ops)
            .field("mode", &self.mode)
            .field("ext", &self.ext)
            .field("events", &self.events)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct PlainLang;
    impl crate::state::TrivialLang for PlainLang {}

    #[test]
    fn disable_all_flips_exactly_the_six_gates() {
        let overrides: TokenRulesOverrides<PlainLang> = TokenRulesOverrides::disable_all();
        // All six gates off…
        assert_eq!(overrides.enable_whitespace, Some(false));
        assert_eq!(overrides.enable_multi_newline_paragraphs, Some(false));
        assert_eq!(overrides.enable_groups, Some(false));
        assert_eq!(overrides.enable_commands, Some(false));
        assert_eq!(overrides.enable_comments, Some(false));
        assert_eq!(overrides.enable_specials, Some(false));
        // …and nothing else touched: the value is the default plus the gate flips, so
        // rules data (and the expected close) survives for later re-enabling.
        let mut expected: TokenRulesOverrides<PlainLang> = TokenRulesOverrides::default();
        expected.enable_whitespace = Some(false);
        expected.enable_multi_newline_paragraphs = Some(false);
        expected.enable_groups = Some(false);
        expected.enable_commands = Some(false);
        expected.enable_comments = Some(false);
        expected.enable_specials = Some(false);
        assert_eq!(overrides, expected);
    }
}
