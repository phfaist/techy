//! [`ParsingStateDelta`] and [`TokenRulesOverrides`]: reified state changes.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::scopes::{ScopeOp, ScopeOpError, SpecsProvider};
use crate::token::{
    CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
    GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
};

use super::features::{FeaturePresence, LangFeatures};
use super::lang::Lang;
use super::parsing_state::StateData;

/// Optional overrides of the whitespace block ([`WhitespaceRules`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WhitespaceOverrides {
    /// Override the whitespace-handling gate.
    pub enabled: Option<bool>,
    /// Replace the whitespace character set.
    pub chars: Option<Arc<str>>,
}

impl WhitespaceOverrides {
    /// The whitespace block of [`TokenRulesOverrides::disable_all`]:
    /// `enabled: Some(false)`, everything else untouched.
    pub fn disable() -> WhitespaceOverrides {
        WhitespaceOverrides { enabled: Some(false), ..WhitespaceOverrides::default() }
    }

    pub(crate) fn merge_from(&mut self, stronger: WhitespaceOverrides) {
        if let Some(v) = stronger.enabled {
            self.enabled = Some(v);
        }
        if let Some(v) = stronger.chars {
            self.chars = Some(v);
        }
    }

    pub(crate) fn apply(&self, rules: &mut WhitespaceRules) {
        if let Some(v) = self.enabled {
            rules.enabled = v;
        }
        if let Some(v) = &self.chars {
            rules.chars = v.clone();
        }
    }
}

/// Optional overrides of the paragraphs block ([`ParagraphRules`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParagraphOverrides {
    /// Override the paragraph-break gate.
    pub enabled: Option<bool>,
}

impl ParagraphOverrides {
    /// The paragraphs block of [`TokenRulesOverrides::disable_all`]:
    /// `enabled: Some(false)`.
    pub fn disable() -> ParagraphOverrides {
        ParagraphOverrides { enabled: Some(false) }
    }

    pub(crate) fn merge_from(&mut self, stronger: ParagraphOverrides) {
        if let Some(v) = stronger.enabled {
            self.enabled = Some(v);
        }
    }

    pub(crate) fn apply(&self, rules: &mut ParagraphRules) {
        if let Some(v) = self.enabled {
            rules.enabled = v;
        }
    }
}

/// Optional overrides of the groups block ([`GroupRules`]).
pub struct GroupOverrides<L: Lang> {
    /// Override the group-delimiter gate.
    pub enabled: Option<bool>,
    /// Replace the recognizable group delimiter rules.
    pub rules: Option<Vec<Arc<GroupRule<L>>>>,
    /// Replace the temporary (scoped-lifecycle) group rules
    /// ([`GroupRules::temporary`]). An explicit override wins over the
    /// derivation-path stripping rule: a delta that sets this field *and* installs an
    /// [`expecting_close`](GroupRules::expecting_close) keeps exactly the
    /// list it names (see [`ParsingState::derived`](super::ParsingState::derived)).
    pub temporary: Option<Vec<Arc<GroupRule<L>>>>,
    /// Override the expected group close (`Some(None)` clears it).
    pub expecting_close: Option<Option<Arc<GroupRule<L>>>>,
}

impl<L: Lang> GroupOverrides<L> {
    /// The groups block of [`TokenRulesOverrides::disable_all`]:
    /// `enabled: Some(false)`, everything else untouched — the base a
    /// takeover parser's groups literal spreads from (see the struct-update note on
    /// [`TokenRulesOverrides`]).
    pub fn disable() -> GroupOverrides<L> {
        GroupOverrides { enabled: Some(false), ..GroupOverrides::default() }
    }

    pub(crate) fn merge_from(&mut self, stronger: GroupOverrides<L>) {
        if let Some(v) = stronger.enabled {
            self.enabled = Some(v);
        }
        if let Some(v) = stronger.rules {
            self.rules = Some(v);
        }
        if let Some(v) = stronger.temporary {
            self.temporary = Some(v);
        }
        if let Some(v) = stronger.expecting_close {
            self.expecting_close = Some(v);
        }
    }

    pub(crate) fn apply(&self, rules: &mut GroupRules<L>) {
        if let Some(v) = self.enabled {
            rules.enabled = v;
        }
        if let Some(v) = &self.rules {
            rules.rules = v.clone();
        }
        if let Some(v) = &self.temporary {
            rules.temporary = v.clone();
        }
        if let Some(v) = &self.expecting_close {
            rules.expecting_close = v.clone();
        }
    }
}

/// Optional overrides of the commands block ([`CommandRules`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOverrides {
    /// Override the command-syntax gate.
    pub enabled: Option<bool>,
    /// Replace the command syntaxes.
    pub rules: Option<Vec<Arc<CommandRule>>>,
}

impl CommandOverrides {
    /// The commands block of [`TokenRulesOverrides::disable_all`]:
    /// `enabled: Some(false)`, everything else untouched.
    pub fn disable() -> CommandOverrides {
        CommandOverrides { enabled: Some(false), ..CommandOverrides::default() }
    }

    pub(crate) fn merge_from(&mut self, stronger: CommandOverrides) {
        if let Some(v) = stronger.enabled {
            self.enabled = Some(v);
        }
        if let Some(v) = stronger.rules {
            self.rules = Some(v);
        }
    }

    pub(crate) fn apply(&self, rules: &mut CommandRules) {
        if let Some(v) = self.enabled {
            rules.enabled = v;
        }
        if let Some(v) = &self.rules {
            rules.rules = v.clone();
        }
    }
}

/// Optional overrides of the comments block ([`CommentRules`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentOverrides {
    /// Override the comment-syntax gate.
    pub enabled: Option<bool>,
    /// Replace the comment syntaxes.
    pub rules: Option<Vec<Arc<CommentRule>>>,
}

impl CommentOverrides {
    /// The comments block of [`TokenRulesOverrides::disable_all`]:
    /// `enabled: Some(false)`, everything else untouched.
    pub fn disable() -> CommentOverrides {
        CommentOverrides { enabled: Some(false), ..CommentOverrides::default() }
    }

    pub(crate) fn merge_from(&mut self, stronger: CommentOverrides) {
        if let Some(v) = stronger.enabled {
            self.enabled = Some(v);
        }
        if let Some(v) = stronger.rules {
            self.rules = Some(v);
        }
    }

    pub(crate) fn apply(&self, rules: &mut CommentRules) {
        if let Some(v) = self.enabled {
            rules.enabled = v;
        }
        if let Some(v) = &self.rules {
            rules.rules = v.clone();
        }
    }
}

/// Optional overrides of the specials block ([`SpecialsRules`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecialsOverrides {
    /// Override the specials-scan gate.
    pub enabled: Option<bool>,
}

impl SpecialsOverrides {
    /// The specials block of [`TokenRulesOverrides::disable_all`]:
    /// `enabled: Some(false)`.
    pub fn disable() -> SpecialsOverrides {
        SpecialsOverrides { enabled: Some(false) }
    }

    pub(crate) fn merge_from(&mut self, stronger: SpecialsOverrides) {
        if let Some(v) = stronger.enabled {
            self.enabled = Some(v);
        }
    }

    pub(crate) fn apply(&self, rules: &mut SpecialsRules) {
        if let Some(v) = self.enabled {
            rules.enabled = v;
        }
    }
}

/// Optional overrides of the forbidden-characters block ([`ForbiddenCharsRules`]).
/// No `enabled` override — the block has no gate (one trivially restorable string).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForbiddenCharsOverrides {
    /// Replace the forbidden-character set.
    pub chars: Option<Arc<str>>,
}

impl ForbiddenCharsOverrides {
    pub(crate) fn merge_from(&mut self, stronger: ForbiddenCharsOverrides) {
        if let Some(v) = stronger.chars {
            self.chars = Some(v);
        }
    }

    pub(crate) fn apply(&self, rules: &mut ForbiddenCharsRules) {
        if let Some(v) = &self.chars {
            rules.chars = v.clone();
        }
    }
}

/// Typed optional overrides of [`TokenRules`] — pylatexenc's "changed kwargs", reified.
/// One override block per feature block, each a struct of `Option` fields:
/// `None` = leave unchanged; `Some(value)` = replace the whole field.
///
/// The `enabled` gates override independently of their data: disabling a feature for a
/// scope is `commands: CommandOverrides::disable()`, and a later
/// `enabled: Some(true)` re-enables it with the *original* rules intact — no party has
/// to carry them.
///
/// Collections are replaced wholesale, not merged: a delta that wants "current group
/// rules plus one more" is built by the party that can see the current state (typically
/// via [`ParsingState::rules()`](super::ParsingState::rules)); merge semantics in the
/// override itself would put policy decisions inside the derivation point.
///
/// # Struct update replaces whole feature blocks
///
/// A struct-update expression works at field granularity, and here every field is a
/// whole feature block: in
/// `TokenRulesOverrides { groups: GroupOverrides { … }, ..TokenRulesOverrides::disable_all() }`
/// the explicit `groups:` literal replaces the *entire* groups block that
/// [`disable_all`](Self::disable_all) set up — including its `enabled: Some(false)`.
/// An inner literal must itself spread from the intended base: a takeover parser that
/// wants "everything disabled, plus an expected close" writes
/// `groups: GroupOverrides { expecting_close: Some(Some(rule)), ..GroupOverrides::disable() }`
/// inside the outer literal.
pub struct TokenRulesOverrides<L: Lang> {
    /// Overrides of the whitespace block.
    pub whitespace: WhitespaceOverrides,
    /// Overrides of the paragraphs block.
    pub paragraphs: ParagraphOverrides,
    /// Overrides of the groups block.
    pub groups: GroupOverrides<L>,
    /// Overrides of the commands block.
    pub commands: CommandOverrides,
    /// Overrides of the comments block.
    pub comments: CommentOverrides,
    /// Overrides of the specials block.
    pub specials: SpecialsOverrides,
    /// Overrides of the forbidden-characters block.
    pub forbidden_chars: ForbiddenCharsOverrides,
}

impl<L: Lang> TokenRulesOverrides<L> {
    /// The overrides value with all six `enabled` gates `Some(false)` (whitespace,
    /// multi-newline paragraphs, groups, commands, comments, specials) and every other
    /// field untouched — the raw-state block a rest-of-line or verbatim-like takeover
    /// parser starts from. It composes: tweak fields afterwards, e.g. install the
    /// terminator that ends the raw region
    /// ([`verbatim_state_delta`](crate::constructs::verbatim_state_delta) is exactly
    /// this plus its [`expecting_close`](GroupRules::expecting_close)) — minding the
    /// whole-block struct-update note above: the tweak spreads from the block's
    /// [`disable()`](GroupOverrides::disable), not from its default.
    ///
    /// This is the *scoped* off — the gates flip while the rules data stays in place,
    /// so a later delta can re-enable a feature with its original rules. The
    /// *constitutive* off (no rules data at all) is
    /// [`TokenRules::empty`](crate::token::TokenRules::empty).
    pub fn disable_all() -> TokenRulesOverrides<L> {
        TokenRulesOverrides {
            whitespace: WhitespaceOverrides::disable(),
            paragraphs: ParagraphOverrides::disable(),
            groups: GroupOverrides::disable(),
            commands: CommandOverrides::disable(),
            comments: CommentOverrides::disable(),
            specials: SpecialsOverrides::disable(),
            forbidden_chars: ForbiddenCharsOverrides::default(),
        }
    }

    /// Merge `stronger` into `self`: every `Some` field of `stronger` replaces
    /// `self`'s, every `None` field leaves `self`'s untouched — the override-layer
    /// composition used by event lowering
    /// ([`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state)).
    pub(crate) fn merge_from(&mut self, stronger: TokenRulesOverrides<L>) {
        self.whitespace.merge_from(stronger.whitespace);
        self.paragraphs.merge_from(stronger.paragraphs);
        self.groups.merge_from(stronger.groups);
        self.commands.merge_from(stronger.commands);
        self.comments.merge_from(stronger.comments);
        self.specials.merge_from(stronger.specials);
        self.forbidden_chars.merge_from(stronger.forbidden_chars);
    }

    /// Apply these overrides to `rules`, leaving `None` fields untouched.
    ///
    /// # Errors
    ///
    /// An override block that carries data — any non-`None` field — for a feature the
    /// language declares absent ([`Lang::Features`]) is a violated contract of the
    /// override's author: an absent feature has no runtime data to change. Nothing of
    /// such a block is applied, and the violation is reported as an
    /// [`AbsentFeatureOverrideError`]. Blocks of present features apply as documented
    /// above regardless.
    pub fn apply(&self, rules: &mut TokenRules<L>) -> Result<(), AbsentFeatureOverrideError> {
        let absent = self.apply_to_present_features(rules);
        if absent.is_empty() {
            Ok(())
        } else {
            Err(AbsentFeatureOverrideError { features: absent })
        }
    }

    /// The gated application core shared by [`apply`](Self::apply) and
    /// [`ParsingStateDelta::apply_overrides`]: apply each feature block the language
    /// declares present; for absent features, apply nothing (absent wins over runtime
    /// data) and collect the block names that carried data — the violation report.
    fn apply_to_present_features(&self, rules: &mut TokenRules<L>) -> Vec<&'static str> {
        let mut absent: Vec<&'static str> = Vec::new();
        if <L::Features as LangFeatures>::Whitespace::PRESENT {
            self.whitespace.apply(&mut rules.whitespace);
        } else if self.whitespace != WhitespaceOverrides::default() {
            absent.push("whitespace");
        }
        if <L::Features as LangFeatures>::Paragraphs::PRESENT {
            self.paragraphs.apply(&mut rules.paragraphs);
        } else if self.paragraphs != ParagraphOverrides::default() {
            absent.push("paragraphs");
        }
        if <L::Features as LangFeatures>::Groups::PRESENT {
            self.groups.apply(&mut rules.groups);
        } else if self.groups != GroupOverrides::default() {
            absent.push("groups");
        }
        if <L::Features as LangFeatures>::Commands::PRESENT {
            self.commands.apply(&mut rules.commands);
        } else if self.commands != CommandOverrides::default() {
            absent.push("commands");
        }
        if <L::Features as LangFeatures>::Comments::PRESENT {
            self.comments.apply(&mut rules.comments);
        } else if self.comments != CommentOverrides::default() {
            absent.push("comments");
        }
        if <L::Features as LangFeatures>::Specials::PRESENT {
            self.specials.apply(&mut rules.specials);
        } else if self.specials != SpecialsOverrides::default() {
            absent.push("specials");
        }
        if <L::Features as LangFeatures>::ForbiddenChars::PRESENT {
            self.forbidden_chars.apply(&mut rules.forbidden_chars);
        } else if self.forbidden_chars != ForbiddenCharsOverrides::default() {
            absent.push("forbidden_chars");
        }
        absent
    }
}

/// A state change carried data for a feature the language declares absent
/// ([`Lang::Features`]): a rules-override block with a non-`None` field, or scope ops
/// under a language without the scope stack. This is a violated contract of the
/// change's author — an absent feature has no runtime data to change — so the
/// violating data is never applied (absent wins over runtime data) and this error is
/// reported instead, never a panic.
/// [`ParsingState::derived`](super::ParsingState::derived) folds it into its
/// [`DeriveError`](super::DeriveError).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsentFeatureOverrideError {
    features: Vec<&'static str>,
}

impl AbsentFeatureOverrideError {
    /// The affected features, by feature-block name (`"whitespace"`, `"paragraphs"`,
    /// `"groups"`, `"commands"`, `"comments"`, `"specials"`, `"forbidden_chars"`,
    /// `"scopes"`), each listed once, in declaration order.
    pub fn features(&self) -> &[&'static str] {
        &self.features
    }
}

impl fmt::Display for AbsentFeatureOverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the state change carries data for features the language declares absent: {}",
            self.features.join(", ")
        )
    }
}

impl core::error::Error for AbsentFeatureOverrideError {}

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
    /// Semantic transition events. **Two classes** (the contract on
    /// [`Lang::Event`]): *context-free* events are consumed by
    /// [`Lang::finalize_transition`] wherever the delta is applied;
    /// *context-dependent* events (needing the enclosing-state stack — the
    /// latexlike exit-math restore) are lowered to ordinary override patches by
    /// the driver inside
    /// [`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state)
    /// and never reach `finalize_transition` — reaching it anyway (a bare
    /// out-of-parse [`derived()`](super::ParsingState::derived)) is a loud
    /// [`FinalizeError`](super::FinalizeError).
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

    /// Whether this delta changes nothing: no rules overrides, no scope ops, no
    /// mode/ext override, no events. Internal — the merged after-effect record
    /// ([`NodesOutcome::after_effects`](crate::constructs::NodesOutcome::after_effects))
    /// spells "no after-effects" as `None`, never as an empty delta.
    pub(crate) fn is_empty(&self) -> bool {
        self.rules == TokenRulesOverrides::default()
            && self.scope_ops.is_empty()
            && self.mode.is_none()
            && self.ext.is_none()
            && self.events.is_empty()
    }

    /// Merge `later` into `self` as a **sequentially later** delta — the composition
    /// used by the merged after-effect record
    /// ([`NodesOutcome::after_effects`](crate::constructs::NodesOutcome::after_effects)):
    /// applying `self` then `later` to a base is reproduced by applying the merged
    /// value once. Rules overrides: `later`'s `Some` fields win
    /// ([`TokenRulesOverrides`] fields replace wholesale, so last-writer-wins is
    /// exact); scope ops concatenate in application order; `mode`/`ext`
    /// last-writer-wins; events concatenate in application order (an event's
    /// position among the ops does not matter — events are consumed by the
    /// transition as a whole, [`Lang::finalize_transition`]).
    pub(crate) fn merge_from(&mut self, later: ParsingStateDelta<L>) {
        self.rules.merge_from(later.rules);
        self.scope_ops.extend(later.scope_ops);
        if later.mode.is_some() {
            self.mode = later.mode;
        }
        if later.ext.is_some() {
            self.ext = later.ext;
        }
        self.events.extend(later.events);
    }

    /// Apply overrides (rules + scope ops + mode + ext) to `data`. Internal, pre-freeze:
    /// called only from `derived()`, before `finalize_transition` runs. Scope-op
    /// failures are collected (the failing op is skipped, the rest still apply) and
    /// returned for `derived()` to report — an empty vec is full success. The second
    /// element reports absent-feature contract violations ([`AbsentFeatureOverrideError`]):
    /// rules overrides carrying data for an absent feature, or scope ops under a
    /// language that declares the scope stack absent. The violating data is skipped
    /// (absent wins over runtime data), the rest of the delta still applies, and
    /// `derived()` folds the report into its error.
    pub(crate) fn apply_overrides(
        &self,
        data: &mut StateData<L>,
    ) -> (Vec<ScopeOpError>, Option<AbsentFeatureOverrideError>) {
        let mut absent = self.rules.apply_to_present_features(&mut data.rules);
        let mut failures = Vec::new();
        if <L::Features as LangFeatures>::Scopes::PRESENT {
            for op in &self.scope_ops {
                if let Err(failure) = data.scopes.apply_op(op) {
                    failures.push(failure);
                }
            }
        } else if !self.scope_ops.is_empty() {
            // Scope ops address the scope stack — a feature like any other on the
            // compile-time axis: with the stack absent, none of them applies.
            absent.push("scopes");
        }
        if let Some(mode) = self.mode {
            data.mode = mode;
        }
        if let Some(ext) = &self.ext {
            data.ext = ext.clone();
        }
        let absent_overrides =
            (!absent.is_empty()).then(|| AbsentFeatureOverrideError { features: absent });
        (failures, absent_overrides)
    }
}

impl<L: Lang> Default for ParsingStateDelta<L> {
    fn default() -> Self {
        ParsingStateDelta::new()
    }
}

// Manual impls to avoid spurious `L:` bounds (associated types are bounded in `Lang`).

impl<L: Lang> Default for GroupOverrides<L> {
    fn default() -> Self {
        GroupOverrides {
            enabled: None,
            rules: None,
            temporary: None,
            expecting_close: None,
        }
    }
}

impl<L: Lang> Clone for GroupOverrides<L> {
    fn clone(&self) -> Self {
        GroupOverrides {
            enabled: self.enabled,
            rules: self.rules.clone(),
            temporary: self.temporary.clone(),
            expecting_close: self.expecting_close.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for GroupOverrides<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupOverrides")
            .field("enabled", &self.enabled)
            .field("rules", &self.rules)
            .field("temporary", &self.temporary)
            .field("expecting_close", &self.expecting_close)
            .finish()
    }
}

impl<L: Lang> PartialEq for GroupOverrides<L> {
    fn eq(&self, other: &Self) -> bool {
        self.enabled == other.enabled
            && self.rules == other.rules
            && self.temporary == other.temporary
            && self.expecting_close == other.expecting_close
    }
}

impl<L: Lang> Eq for GroupOverrides<L> {}

impl<L: Lang> Default for TokenRulesOverrides<L> {
    fn default() -> Self {
        TokenRulesOverrides {
            whitespace: WhitespaceOverrides::default(),
            paragraphs: ParagraphOverrides::default(),
            groups: GroupOverrides::default(),
            commands: CommandOverrides::default(),
            comments: CommentOverrides::default(),
            specials: SpecialsOverrides::default(),
            forbidden_chars: ForbiddenCharsOverrides::default(),
        }
    }
}

impl<L: Lang> Clone for TokenRulesOverrides<L> {
    fn clone(&self) -> Self {
        TokenRulesOverrides {
            whitespace: self.whitespace.clone(),
            paragraphs: self.paragraphs.clone(),
            groups: self.groups.clone(),
            commands: self.commands.clone(),
            comments: self.comments.clone(),
            specials: self.specials.clone(),
            forbidden_chars: self.forbidden_chars.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for TokenRulesOverrides<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRulesOverrides")
            .field("whitespace", &self.whitespace)
            .field("paragraphs", &self.paragraphs)
            .field("groups", &self.groups)
            .field("commands", &self.commands)
            .field("comments", &self.comments)
            .field("specials", &self.specials)
            .field("forbidden_chars", &self.forbidden_chars)
            .finish()
    }
}

impl<L: Lang> PartialEq for TokenRulesOverrides<L> {
    fn eq(&self, other: &Self) -> bool {
        self.whitespace == other.whitespace
            && self.paragraphs == other.paragraphs
            && self.groups == other.groups
            && self.commands == other.commands
            && self.comments == other.comments
            && self.specials == other.specials
            && self.forbidden_chars == other.forbidden_chars
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
        assert_eq!(overrides.whitespace.enabled, Some(false));
        assert_eq!(overrides.paragraphs.enabled, Some(false));
        assert_eq!(overrides.groups.enabled, Some(false));
        assert_eq!(overrides.commands.enabled, Some(false));
        assert_eq!(overrides.comments.enabled, Some(false));
        assert_eq!(overrides.specials.enabled, Some(false));
        // …and nothing else touched: the value is the default plus the gate flips, so
        // rules data (and the expected close) survives for later re-enabling.
        let mut expected: TokenRulesOverrides<PlainLang> = TokenRulesOverrides::default();
        expected.whitespace.enabled = Some(false);
        expected.paragraphs.enabled = Some(false);
        expected.groups.enabled = Some(false);
        expected.commands.enabled = Some(false);
        expected.comments.enabled = Some(false);
        expected.specials.enabled = Some(false);
        assert_eq!(overrides, expected);
    }
}
