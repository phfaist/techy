//! [`ParsingStateDelta`], a parsing-state change described as data, and the
//! [`TokenRulesOverrides`] blocks it applies to the tokenization rules.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::scopes::{ScopeOp, ScopeOpError, SpecsProvider};
use crate::token::{
    CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
    GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
};

use super::features::{FeaturePresence, LangFeatures, LangHasScopes};
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
    /// Overrides that turn whitespace handling off: `enabled: Some(false)`, every
    /// other field left alone. This is the block
    /// [`TokenRulesOverrides::disable_all`] installs for a language with the
    /// whitespace feature.
    pub fn disable() -> WhitespaceOverrides {
        WhitespaceOverrides { enabled: Some(false), ..WhitespaceOverrides::default() }
    }

    /// Overrides that replace the whole block with `rules`: every field set to
    /// `Some` of the corresponding value — the block-level piece of
    /// [`TokenRulesOverrides::override_all`].
    pub fn override_all(rules: &WhitespaceRules) -> WhitespaceOverrides {
        WhitespaceOverrides { enabled: Some(rules.enabled), chars: Some(rules.chars.clone()) }
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
    /// Overrides that turn paragraph-break detection off: `enabled: Some(false)`.
    /// This is the block [`TokenRulesOverrides::disable_all`] installs for a language
    /// with the paragraphs feature.
    pub fn disable() -> ParagraphOverrides {
        ParagraphOverrides { enabled: Some(false) }
    }

    /// Overrides that replace the whole block with `rules`: the gate set to `Some`
    /// of its value — the block-level piece of
    /// [`TokenRulesOverrides::override_all`].
    pub fn override_all(rules: &ParagraphRules) -> ParagraphOverrides {
        ParagraphOverrides { enabled: Some(rules.enabled) }
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
    /// Replace the temporary group rules ([`GroupRules::temporary`]) — the ones that
    /// last only for the region that installed them.
    ///
    /// Setting this field explicitly overrides the rule by which deriving a state
    /// otherwise empties the list: a delta that sets it *and* installs an
    /// [`expecting_close`](GroupRules::expecting_close) keeps exactly the list named
    /// here. See [`ParsingState::derived`](super::ParsingState::derived).
    pub temporary: Option<Vec<Arc<GroupRule<L>>>>,
    /// Override the expected group close (`Some(None)` clears it).
    pub expecting_close: Option<Option<Arc<GroupRule<L>>>>,
}

impl<L: Lang> GroupOverrides<L> {
    /// Overrides that turn group delimiters off: `enabled: Some(false)`, every other
    /// field left alone. This is the block [`TokenRulesOverrides::disable_all`]
    /// installs for a language with the groups feature, and the base a takeover
    /// parser's own groups literal spreads from (see the struct-update section on
    /// [`TokenRulesOverrides`]).
    pub fn disable() -> GroupOverrides<L> {
        GroupOverrides { enabled: Some(false), ..GroupOverrides::default() }
    }

    /// Overrides that copy `rules`' **lasting** group data: the
    /// [`enabled`](GroupRules::enabled) gate and the delimiter
    /// [`rules`](GroupRules::rules) list. The list is cloned as `Arc` handles, so rule
    /// identity survives (see the identity section on [`GroupRule`]).
    ///
    /// [`temporary`](GroupRules::temporary) and
    /// [`expecting_close`](GroupRules::expecting_close) are deliberately left `None`:
    /// they describe what one live parse position expects, not rules data to install
    /// elsewhere — and setting both would additionally trigger the explicit-list rule
    /// of [`ParsingState::derived`](super::ParsingState::derived). A parser that wants
    /// an expected close adds it on top of this value:
    /// `GroupOverrides { expecting_close: Some(Some(rule)), ..GroupOverrides::override_all(&rules) }`
    /// — the same shape
    /// [`verbatim_state_delta`](crate::core::constructs::verbatim_state_delta) writes
    /// over [`disable()`](Self::disable).
    ///
    /// This is the groups part of [`TokenRulesOverrides::override_all`].
    pub fn override_all(rules: &GroupRules<L>) -> GroupOverrides<L> {
        GroupOverrides {
            enabled: Some(rules.enabled),
            rules: Some(rules.rules.clone()),
            temporary: None,
            expecting_close: None,
        }
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
    /// Overrides that turn command syntax off: `enabled: Some(false)`, every other
    /// field left alone. This is the block [`TokenRulesOverrides::disable_all`]
    /// installs for a language with the commands feature.
    pub fn disable() -> CommandOverrides {
        CommandOverrides { enabled: Some(false), ..CommandOverrides::default() }
    }

    /// Overrides that replace the whole block with `rules`: every field set to
    /// `Some` of the corresponding value (the syntaxes are cloned as `Arc`
    /// handles) — the block-level piece of
    /// [`TokenRulesOverrides::override_all`].
    pub fn override_all(rules: &CommandRules) -> CommandOverrides {
        CommandOverrides { enabled: Some(rules.enabled), rules: Some(rules.rules.clone()) }
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
    /// Overrides that turn comment syntax off: `enabled: Some(false)`, every other
    /// field left alone. This is the block [`TokenRulesOverrides::disable_all`]
    /// installs for a language with the comments feature.
    pub fn disable() -> CommentOverrides {
        CommentOverrides { enabled: Some(false), ..CommentOverrides::default() }
    }

    /// Overrides that replace the whole block with `rules`: every field set to
    /// `Some` of the corresponding value (the syntaxes are cloned as `Arc`
    /// handles) — the block-level piece of
    /// [`TokenRulesOverrides::override_all`].
    pub fn override_all(rules: &CommentRules) -> CommentOverrides {
        CommentOverrides { enabled: Some(rules.enabled), rules: Some(rules.rules.clone()) }
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
    /// Overrides that turn the specials scan off: `enabled: Some(false)`. This is the
    /// block [`TokenRulesOverrides::disable_all`] installs for a language with the
    /// specials feature.
    pub fn disable() -> SpecialsOverrides {
        SpecialsOverrides { enabled: Some(false) }
    }

    /// Overrides that replace the whole block with `rules`: the gate set to `Some`
    /// of its value — the block-level piece of
    /// [`TokenRulesOverrides::override_all`].
    pub fn override_all(rules: &SpecialsRules) -> SpecialsOverrides {
        SpecialsOverrides { enabled: Some(rules.enabled) }
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
    /// Overrides that turn the forbidden-character check off. The block has no gate,
    /// so the off is expressed in the data itself: `chars: Some("")`, the empty
    /// forbidden set. This is the block [`TokenRulesOverrides::disable_all`] installs
    /// for a language with the forbidden-characters feature.
    pub fn disable() -> ForbiddenCharsOverrides {
        ForbiddenCharsOverrides { chars: Some("".into()) }
    }

    /// Overrides that replace the whole block with `rules`: the forbidden-character
    /// set overridden to `rules`' own (the block has no gate) — the block-level
    /// piece of [`TokenRulesOverrides::override_all`].
    pub fn override_all(rules: &ForbiddenCharsRules) -> ForbiddenCharsOverrides {
        ForbiddenCharsOverrides { chars: Some(rules.chars.clone()) }
    }

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

/// Optional overrides of the tokenization rules ([`TokenRules`]): one override block
/// per feature block, each a struct of `Option` fields, where `None` leaves the field
/// unchanged and `Some(value)` replaces it.
///
/// This is the token-rules part of a [`ParsingStateDelta`]. Start from
/// [`default()`](Default::default) (change nothing),
/// [`disable_all()`](Self::disable_all) (turn every feature off) or
/// [`override_all()`](Self::override_all) (install a complete set of rules), then
/// adjust the fields that matter.
///
/// Each `enabled` gate overrides independently of the data it gates. Turning a feature
/// off for a region is `commands: CommandOverrides::disable()`, and a later
/// `enabled: Some(true)` turns it back on with the *original* rules still in place —
/// no one has to remember and restore them.
///
/// **A field is replaced, never merged into.** An override that sets a rule list
/// installs that list; there is no "the current rules plus one more". A delta wanting
/// that is built by whoever can see the current state, through
/// [`ParsingState::rules()`](super::ParsingState::rules). Wholesale replacement is
/// also what makes combining two deltas simple: when both set the same field, the
/// later one wins outright.
///
/// Like [`TokenRules`] itself, each block is stored according to the language's
/// declaration for that feature ([`Lang::Features`]). For a feature the language
/// declares absent, the field holds no override block at all and takes no space
/// ([`FeaturePresence::Store`]), so a delta can never carry overrides for it. For a
/// language with every feature present the fields *are* the override blocks, and none
/// of this is visible.
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
    /// Overrides of the whitespace block. For a language that declares the
    /// whitespace feature absent, this field holds the zero-sized store and cannot
    /// carry overrides.
    pub whitespace:
        <<L::Features as LangFeatures>::Whitespace as FeaturePresence>::Store<WhitespaceOverrides>,
    /// Overrides of the paragraphs block. For a language that declares the
    /// paragraphs feature absent, this field holds the zero-sized store and cannot
    /// carry overrides.
    pub paragraphs:
        <<L::Features as LangFeatures>::Paragraphs as FeaturePresence>::Store<ParagraphOverrides>,
    /// Overrides of the groups block. For a language that declares the groups
    /// feature absent, this field holds the zero-sized store and cannot hold
    /// overrides.
    pub groups:
        <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<GroupOverrides<L>>,
    /// Overrides of the commands block. For a language that declares the commands
    /// feature absent, this field holds the zero-sized store and cannot hold
    /// overrides.
    pub commands:
        <<L::Features as LangFeatures>::Commands as FeaturePresence>::Store<CommandOverrides>,
    /// Overrides of the comments block. For a language that declares the comments
    /// feature absent, this field holds the zero-sized store and cannot hold
    /// overrides.
    pub comments:
        <<L::Features as LangFeatures>::Comments as FeaturePresence>::Store<CommentOverrides>,
    /// Overrides of the specials block. For a language that declares the specials
    /// feature absent, this field holds the zero-sized store and cannot hold
    /// overrides.
    pub specials:
        <<L::Features as LangFeatures>::Specials as FeaturePresence>::Store<SpecialsOverrides>,
    /// Overrides of the forbidden-characters block. For a language that declares the
    /// forbidden-characters feature absent, this field holds the zero-sized store and
    /// cannot carry overrides.
    pub forbidden_chars: <<L::Features as LangFeatures>::ForbiddenChars as FeaturePresence>::Store<
        ForbiddenCharsOverrides,
    >,
}

impl<L: Lang> TokenRulesOverrides<L> {
    /// Overrides that turn off every feature the language has: each block whose
    /// feature `L` declares present ([`Lang::Features`]) is set to its `disable()`
    /// value.
    ///
    /// For the six gated blocks (whitespace, paragraph breaks, groups, commands,
    /// comments, specials) that means `enabled: Some(false)` with every other field
    /// left alone. `forbidden_chars` has no gate, so its off is expressed in the data:
    /// the empty forbidden set, `chars: Some("")`. A feature the language declares
    /// absent is not mentioned by the returned value at all.
    ///
    /// This is the starting point for a parser that takes over the raw input — a
    /// rest-of-line or verbatim region. Adjust the result afterwards, for instance to
    /// install the terminator that ends the region;
    /// [`verbatim_state_delta`](crate::core::constructs::verbatim_state_delta) is this
    /// value plus an [`expecting_close`](GroupRules::expecting_close). Mind the
    /// whole-block struct-update rule above: such an adjustment must spread from the
    /// block's own [`disable()`](GroupOverrides::disable), not from its default.
    ///
    /// Only the gates flip; the rules data stays in place, so a later delta can turn a
    /// feature back on with its original rules. The forbidden set, having no gate, is
    /// replaced instead, so restoring it means overriding the characters again. To
    /// remove the rules data itself rather than turn the features off, use
    /// [`TokenRules::empty`](crate::core::token::TokenRules::empty).
    pub fn disable_all() -> TokenRulesOverrides<L> {
        // `store_with` consults the presence declaration: a present feature's field
        // gets the block's `disable()` value; an absent feature's field is the
        // zero-sized store (the constructor is never called).
        TokenRulesOverrides {
            whitespace: <L::Features as LangFeatures>::Whitespace::store_with(
                WhitespaceOverrides::disable,
            ),
            paragraphs: <L::Features as LangFeatures>::Paragraphs::store_with(
                ParagraphOverrides::disable,
            ),
            groups: <L::Features as LangFeatures>::Groups::store_with(GroupOverrides::disable),
            commands: <L::Features as LangFeatures>::Commands::store_with(
                CommandOverrides::disable,
            ),
            comments: <L::Features as LangFeatures>::Comments::store_with(
                CommentOverrides::disable,
            ),
            specials: <L::Features as LangFeatures>::Specials::store_with(
                SpecialsOverrides::disable,
            ),
            forbidden_chars: <L::Features as LangFeatures>::ForbiddenChars::store_with(
                ForbiddenCharsOverrides::disable,
            ),
        }
    }

    /// Overrides that install `rules` wholesale: for every feature the language
    /// declares present ([`Lang::Features`]) the block is set to its `override_all()`
    /// value, so applying the result makes the target's rules equal to `rules`. A
    /// feature the language declares absent is not mentioned by the returned value at
    /// all.
    ///
    /// The result is exactly the composition of the seven per-block constructors
    /// ([`WhitespaceOverrides::override_all`], [`GroupOverrides::override_all`], and
    /// so on). It is the counterpart of [`disable_all()`](Self::disable_all): that one
    /// flips the gates and leaves the data alone, this one replaces the data and sets
    /// each gate to whatever `rules` has. Rule lists are cloned as `Arc` handles, so
    /// rule identity survives (see the identity section on [`GroupRule`]).
    ///
    /// **The two transient group fields are not copied.** Whatever `rules` holds in
    /// [`temporary`](GroupRules::temporary) and
    /// [`expecting_close`](GroupRules::expecting_close) is left `None`: they are
    /// expectations of one live parse position, not rules data to install elsewhere
    /// ([`GroupOverrides::override_all`] gives the full reasoning, and shows how a
    /// parser that wants an expected close adds it on top). Every other field of every
    /// present block is `Some`.
    pub fn override_all(rules: &TokenRules<L>) -> TokenRulesOverrides<L> {
        // Matched projections per feature, as in `apply`: the rules store and the
        // override store carry the same presence marker, so a present feature's
        // block is built from its rules block, and an absent feature's field stays
        // the zero-sized store `default()` put there — there is nothing on either
        // side to carry.
        let mut overrides = TokenRulesOverrides::<L>::default();
        if let (Some(slot), Some(block)) = (
            <L::Features as LangFeatures>::Whitespace::store_get_mut(&mut overrides.whitespace),
            <L::Features as LangFeatures>::Whitespace::store_get(&rules.whitespace),
        ) {
            *slot = WhitespaceOverrides::override_all(block);
        }
        if let (Some(slot), Some(block)) = (
            <L::Features as LangFeatures>::Paragraphs::store_get_mut(&mut overrides.paragraphs),
            <L::Features as LangFeatures>::Paragraphs::store_get(&rules.paragraphs),
        ) {
            *slot = ParagraphOverrides::override_all(block);
        }
        if let (Some(slot), Some(block)) = (
            <L::Features as LangFeatures>::Groups::store_get_mut(&mut overrides.groups),
            <L::Features as LangFeatures>::Groups::store_get(&rules.groups),
        ) {
            *slot = GroupOverrides::override_all(block);
        }
        if let (Some(slot), Some(block)) = (
            <L::Features as LangFeatures>::Commands::store_get_mut(&mut overrides.commands),
            <L::Features as LangFeatures>::Commands::store_get(&rules.commands),
        ) {
            *slot = CommandOverrides::override_all(block);
        }
        if let (Some(slot), Some(block)) = (
            <L::Features as LangFeatures>::Comments::store_get_mut(&mut overrides.comments),
            <L::Features as LangFeatures>::Comments::store_get(&rules.comments),
        ) {
            *slot = CommentOverrides::override_all(block);
        }
        if let (Some(slot), Some(block)) = (
            <L::Features as LangFeatures>::Specials::store_get_mut(&mut overrides.specials),
            <L::Features as LangFeatures>::Specials::store_get(&rules.specials),
        ) {
            *slot = SpecialsOverrides::override_all(block);
        }
        if let (Some(slot), Some(block)) = (
            <L::Features as LangFeatures>::ForbiddenChars::store_get_mut(
                &mut overrides.forbidden_chars,
            ),
            <L::Features as LangFeatures>::ForbiddenChars::store_get(&rules.forbidden_chars),
        ) {
            *slot = ForbiddenCharsOverrides::override_all(block);
        }
        overrides
    }

    /// Merge `stronger` into `self`: every `Some` field of `stronger` replaces
    /// `self`'s, every `None` field leaves `self`'s untouched — the override-layer
    /// composition used by event lowering
    /// ([`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state)).
    pub(crate) fn merge_from(&mut self, stronger: TokenRulesOverrides<L>) {
        // Matched projections per feature: both sides' stores carry the same
        // presence marker, so either both project `Some` (present — merge the
        // blocks) or both project `None` (absent — the zero-sized stores hold
        // nothing to merge).
        if let (Some(mine), Some(stronger)) = (
            <L::Features as LangFeatures>::Whitespace::store_get_mut(&mut self.whitespace),
            <L::Features as LangFeatures>::Whitespace::store_into_inner(stronger.whitespace),
        ) {
            mine.merge_from(stronger);
        }
        if let (Some(mine), Some(stronger)) = (
            <L::Features as LangFeatures>::Paragraphs::store_get_mut(&mut self.paragraphs),
            <L::Features as LangFeatures>::Paragraphs::store_into_inner(stronger.paragraphs),
        ) {
            mine.merge_from(stronger);
        }
        if let (Some(mine), Some(stronger)) = (
            <L::Features as LangFeatures>::Groups::store_get_mut(&mut self.groups),
            <L::Features as LangFeatures>::Groups::store_into_inner(stronger.groups),
        ) {
            mine.merge_from(stronger);
        }
        if let (Some(mine), Some(stronger)) = (
            <L::Features as LangFeatures>::Commands::store_get_mut(&mut self.commands),
            <L::Features as LangFeatures>::Commands::store_into_inner(stronger.commands),
        ) {
            mine.merge_from(stronger);
        }
        if let (Some(mine), Some(stronger)) = (
            <L::Features as LangFeatures>::Comments::store_get_mut(&mut self.comments),
            <L::Features as LangFeatures>::Comments::store_into_inner(stronger.comments),
        ) {
            mine.merge_from(stronger);
        }
        if let (Some(mine), Some(stronger)) = (
            <L::Features as LangFeatures>::Specials::store_get_mut(&mut self.specials),
            <L::Features as LangFeatures>::Specials::store_into_inner(stronger.specials),
        ) {
            mine.merge_from(stronger);
        }
        if let (Some(mine), Some(stronger)) = (
            <L::Features as LangFeatures>::ForbiddenChars::store_get_mut(
                &mut self.forbidden_chars,
            ),
            <L::Features as LangFeatures>::ForbiddenChars::store_into_inner(
                stronger.forbidden_chars,
            ),
        ) {
            mine.merge_from(stronger);
        }
    }

    /// Apply these overrides to `rules`, leaving `None` fields untouched. Cannot
    /// fail: override data for a feature the language declares absent is
    /// unrepresentable (the field is the zero-sized store), so every override block
    /// that exists has a rules block to apply to.
    pub fn apply(&self, rules: &mut TokenRules<L>) {
        // Matched projections per feature: the override store and the rules store
        // carry the same presence marker, so either both project `Some` (present —
        // apply the block) or both project `None` (absent — nothing exists on either
        // side).
        if let (Some(overrides), Some(block)) = (
            <L::Features as LangFeatures>::Whitespace::store_get(&self.whitespace),
            <L::Features as LangFeatures>::Whitespace::store_get_mut(&mut rules.whitespace),
        ) {
            overrides.apply(block);
        }
        if let (Some(overrides), Some(block)) = (
            <L::Features as LangFeatures>::Paragraphs::store_get(&self.paragraphs),
            <L::Features as LangFeatures>::Paragraphs::store_get_mut(&mut rules.paragraphs),
        ) {
            overrides.apply(block);
        }
        if let (Some(overrides), Some(block)) = (
            <L::Features as LangFeatures>::Groups::store_get(&self.groups),
            <L::Features as LangFeatures>::Groups::store_get_mut(&mut rules.groups),
        ) {
            overrides.apply(block);
        }
        if let (Some(overrides), Some(block)) = (
            <L::Features as LangFeatures>::Commands::store_get(&self.commands),
            <L::Features as LangFeatures>::Commands::store_get_mut(&mut rules.commands),
        ) {
            overrides.apply(block);
        }
        if let (Some(overrides), Some(block)) = (
            <L::Features as LangFeatures>::Comments::store_get(&self.comments),
            <L::Features as LangFeatures>::Comments::store_get_mut(&mut rules.comments),
        ) {
            overrides.apply(block);
        }
        if let (Some(overrides), Some(block)) = (
            <L::Features as LangFeatures>::Specials::store_get(&self.specials),
            <L::Features as LangFeatures>::Specials::store_get_mut(&mut rules.specials),
        ) {
            overrides.apply(block);
        }
        if let (Some(overrides), Some(block)) = (
            <L::Features as LangFeatures>::ForbiddenChars::store_get(&self.forbidden_chars),
            <L::Features as LangFeatures>::ForbiddenChars::store_get_mut(
                &mut rules.forbidden_chars,
            ),
        ) {
            overrides.apply(block);
        }
    }

    /// Whether every stored override block leaves the rules unchanged: each present
    /// feature's block equals its all-`None` default (an absent feature's store
    /// holds nothing and is trivially unchanged). Internal — the whole-value `==`
    /// spelling needs per-store equality bounds a bare `L: Lang` cannot supply
    /// (see [`ParsingStateDelta::is_empty`]).
    pub(crate) fn is_empty(&self) -> bool {
        <L::Features as LangFeatures>::Whitespace::store_get(&self.whitespace)
            .is_none_or(|block| *block == WhitespaceOverrides::default())
            && <L::Features as LangFeatures>::Paragraphs::store_get(&self.paragraphs)
                .is_none_or(|block| *block == ParagraphOverrides::default())
            && <L::Features as LangFeatures>::Groups::store_get(&self.groups)
                .is_none_or(|block| *block == GroupOverrides::default())
            && <L::Features as LangFeatures>::Commands::store_get(&self.commands)
                .is_none_or(|block| *block == CommandOverrides::default())
            && <L::Features as LangFeatures>::Comments::store_get(&self.comments)
                .is_none_or(|block| *block == CommentOverrides::default())
            && <L::Features as LangFeatures>::Specials::store_get(&self.specials)
                .is_none_or(|block| *block == SpecialsOverrides::default())
            && <L::Features as LangFeatures>::ForbiddenChars::store_get(&self.forbidden_chars)
                .is_none_or(|block| *block == ForbiddenCharsOverrides::default())
    }
}

/// A description of a parsing-state change: the argument of
/// [`ParsingState::derived()`](super::ParsingState::derived).
///
/// A delta lists what is to be different in the new state — token-rules overrides,
/// scope operations, a parsing mode, a replacement state extension — together with any
/// semantic [events](Lang::Event) the language interprets. Overrides and events travel
/// in the same value, so one transition, and one run of
/// [`Lang::finalize_transition`], covers both.
///
/// A delta is plain data rather than a closure. It can be inspected, stored, merged
/// with another delta, and applied by a *caller* to a base state its producer never
/// saw. That is what lets a construct parser return the change it wants and leave the
/// caller to decide where the change applies: to the caller's own state, so the
/// following siblings see it, or nowhere, because the group it was made in has ended.
///
/// Build one with [`new()`](Self::new) and the chaining setters —
/// [`rules()`](Self::rules()), [`scope_op()`](Self::scope_op),
/// [`push_provider()`](Self::push_provider), [`mode()`](Self::mode()),
/// [`ext()`](Self::ext()), [`event()`](Self::event) — or by filling in the public
/// fields. Inside a driven parse, apply one through
/// [`ParseContext::derive_state`](crate::core::constructs::ParseContext::derive_state)
/// rather than calling `derived()` directly.
///
/// # Combining two deltas
///
/// Applying one delta and then another has the same effect as applying a single
/// combined delta, and the machinery does combine them — a group's interior delta with
/// the descent's own, an invocation's after-effect with the ones before it. The later
/// delta wins, field by field:
///
/// - **Token-rules overrides** ([`rules`](field@Self::rules)): each `Some` field of the
///   later delta replaces the earlier delta's; a `None` field leaves the earlier one
///   in place. Since an override replaces a whole field rather than merging into it
///   (see [`TokenRulesOverrides`]), "the later one wins" is exact.
/// - **[`mode`](field@Self::mode) and [`ext`](field@Self::ext)**: the later `Some`
///   wins; `None` keeps what the earlier delta set.
/// - **[`scope_ops`](field@Self::scope_ops) and [`events`](field@Self::events)**:
///   appended in application order — nothing is discarded. Where an event sits among
///   the scope ops does not matter, because [`Lang::finalize_transition`] receives all
///   the events of one transition at once.
///
/// The [concepts overview](crate::guide::concepts_overview#parsing-state-and-deltas)
/// introduces states and deltas, and [the parsing
/// model](crate::guide::parsing_model#how-parsing-state-flows) shows where each kind of
/// change applies.
pub struct ParsingStateDelta<L: Lang> {
    /// Overrides of the stored token rules; every field of every block is optional.
    pub rules: TokenRulesOverrides<L>,
    /// Operations on the scope stack, applied in the order listed: operations that
    /// change the shape of the stack, and definition operations addressed to a named
    /// provider ([`ScopeOp`]).
    ///
    /// This is how definitions are added during a parse (`\newcommand`). Undoing them
    /// takes nothing: leaving the scope means continuing with the previous
    /// `Arc<ParsingState>`, which the caller still holds.
    ///
    /// An operation can **fail** — it names a provider that is not on the stack, or
    /// sends a definition to an immutable provider. Each failure is recorded and the
    /// remaining operations still apply; the failures reach the caller through
    /// [`derived()`](super::ParsingState::derived).
    ///
    /// For a language that declares the scopes feature absent ([`Lang::Features`]),
    /// this field occupies no space and cannot hold operations, and the
    /// [`scope_op()`](Self::scope_op) and [`push_provider()`](Self::push_provider)
    /// builders are unavailable.
    pub scope_ops: <<L::Features as LangFeatures>::Scopes as FeaturePresence>::Store<Vec<ScopeOp<L>>>,
    /// The parsing mode to switch to ([`StateData::mode`]); `None` leaves it
    /// unchanged.
    ///
    /// Setting it is itself the signal that the mode changes:
    /// [`Lang::finalize_transition`] sees the new mode already applied and compares it
    /// with the previous state's [`mode()`](super::ParsingState::mode). A
    /// mode change needs no [`Lang::Event`].
    pub mode: Option<L::ModeId>,
    /// A replacement for the language's own state ([`Lang::StateExt`]), as a whole
    /// value; `None` leaves it unchanged. Generic code leaves this `None` — a language
    /// usually adjusts its extension from an event in
    /// [`Lang::finalize_transition`] instead.
    pub ext: Option<L::StateExt>,
    /// Semantic transition events for the language to interpret.
    ///
    /// There are two kinds, described on [`Lang::Event`]. A *context-free* event is
    /// interpreted by [`Lang::finalize_transition`] wherever the delta is applied. A
    /// *context-dependent* event — one whose effect depends on the enclosing states,
    /// such as the latexlike restore on leaving math — is translated into ordinary
    /// overrides by the driver inside
    /// [`ParseContext::derive_state`](crate::core::constructs::ParseContext::derive_state)
    /// and never reaches `finalize_transition`. If one does reach it — a bare
    /// [`derived()`](super::ParsingState::derived) call outside a parse — the
    /// customizer must refuse with a [`FinalizeError`](super::FinalizeError) rather
    /// than ignore it.
    pub events: Vec<L::Event>,
}

impl<L: Lang> ParsingStateDelta<L> {
    /// An empty delta, the starting point for the chaining setters. Deriving with it
    /// yields a state equivalent to the base.
    pub fn new() -> ParsingStateDelta<L> {
        ParsingStateDelta {
            rules: TokenRulesOverrides::default(),
            scope_ops: <L::Features as LangFeatures>::Scopes::store_with(Vec::new),
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

    /// Add a scope-stack operation; operations apply in the order they are added.
    ///
    /// Available only for a language with the scopes feature ([`LangHasScopes`]): the
    /// operations address the scope stack, which a language without the feature does
    /// not have.
    pub fn scope_op(mut self, op: ScopeOp<L>) -> Self
    where
        L: LangHasScopes,
    {
        self.scope_ops.push(op);
        self
    }

    /// Push a provider onto the state's scope stack; the last one pushed is the
    /// innermost. A convenience method for the most common operation,
    /// [`ScopeOp::Push`].
    ///
    /// Available only for a language with the scopes feature, like
    /// [`scope_op()`](Self::scope_op).
    pub fn push_provider(mut self, provider: Arc<dyn SpecsProvider<L>>) -> Self
    where
        L: LangHasScopes,
    {
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
    /// spells "no after-effects" as `None`, never as an empty delta. Per-block
    /// comparisons rather than a whole-value `==`: the store-level equality bounds
    /// do not resolve under a bare `L: Lang`.
    pub(crate) fn is_empty(&self) -> bool {
        self.rules.is_empty()
            && <L::Features as LangFeatures>::Scopes::store_get(&self.scope_ops)
                .is_none_or(|ops| ops.is_empty())
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
        // Matched projections: both scope-op stores carry the same presence marker
        // (present — concatenate; absent — the zero-sized stores hold no ops).
        if let (Some(ops), Some(later_ops)) = (
            <L::Features as LangFeatures>::Scopes::store_get_mut(&mut self.scope_ops),
            <L::Features as LangFeatures>::Scopes::store_into_inner(later.scope_ops),
        ) {
            ops.extend(later_ops);
        }
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
    /// returned for `derived()` to report — an empty vec is full success. For a
    /// language that declares the scope stack absent, no scope ops exist to apply
    /// (the list is the zero-sized store), just as no override data exists for any
    /// other absent feature.
    pub(crate) fn apply_overrides(&self, data: &mut StateData<L>) -> Vec<ScopeOpError> {
        self.rules.apply(&mut data.rules);
        let mut failures = Vec::new();
        if let Some(ops) = <L::Features as LangFeatures>::Scopes::store_get(&self.scope_ops) {
            for op in ops {
                if let Err(failure) = data.scopes.apply_op(op) {
                    failures.push(failure);
                }
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
        // A present feature's field gets its block's all-`None` default; an absent
        // feature's field is the zero-sized store.
        TokenRulesOverrides {
            whitespace: <L::Features as LangFeatures>::Whitespace::store_with(
                WhitespaceOverrides::default,
            ),
            paragraphs: <L::Features as LangFeatures>::Paragraphs::store_with(
                ParagraphOverrides::default,
            ),
            groups: <L::Features as LangFeatures>::Groups::store_with(GroupOverrides::default),
            commands: <L::Features as LangFeatures>::Commands::store_with(
                CommandOverrides::default,
            ),
            comments: <L::Features as LangFeatures>::Comments::store_with(
                CommentOverrides::default,
            ),
            specials: <L::Features as LangFeatures>::Specials::store_with(
                SpecialsOverrides::default,
            ),
            forbidden_chars: <L::Features as LangFeatures>::ForbiddenChars::store_with(
                ForbiddenCharsOverrides::default,
            ),
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

// The equality impls carry one where-clause per store: the `Store` GAT itself
// promises only `Clone`/`Debug`, but both markers' stores satisfy `PartialEq`/`Eq`
// whenever the stored block does (and every override block does), so the bounds hold
// at every concrete language.

impl<L: Lang> PartialEq for TokenRulesOverrides<L>
where
    <<L::Features as LangFeatures>::Whitespace as FeaturePresence>::Store<WhitespaceOverrides>:
        PartialEq,
    <<L::Features as LangFeatures>::Paragraphs as FeaturePresence>::Store<ParagraphOverrides>:
        PartialEq,
    <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<GroupOverrides<L>>:
        PartialEq,
    <<L::Features as LangFeatures>::Commands as FeaturePresence>::Store<CommandOverrides>:
        PartialEq,
    <<L::Features as LangFeatures>::Comments as FeaturePresence>::Store<CommentOverrides>:
        PartialEq,
    <<L::Features as LangFeatures>::Specials as FeaturePresence>::Store<SpecialsOverrides>:
        PartialEq,
    <<L::Features as LangFeatures>::ForbiddenChars as FeaturePresence>::Store<
        ForbiddenCharsOverrides,
    >: PartialEq,
{
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

impl<L: Lang> Eq for TokenRulesOverrides<L>
where
    <<L::Features as LangFeatures>::Whitespace as FeaturePresence>::Store<WhitespaceOverrides>:
        Eq,
    <<L::Features as LangFeatures>::Paragraphs as FeaturePresence>::Store<ParagraphOverrides>:
        Eq,
    <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<GroupOverrides<L>>: Eq,
    <<L::Features as LangFeatures>::Commands as FeaturePresence>::Store<CommandOverrides>: Eq,
    <<L::Features as LangFeatures>::Comments as FeaturePresence>::Store<CommentOverrides>: Eq,
    <<L::Features as LangFeatures>::Specials as FeaturePresence>::Store<SpecialsOverrides>: Eq,
    <<L::Features as LangFeatures>::ForbiddenChars as FeaturePresence>::Store<
        ForbiddenCharsOverrides,
    >: Eq,
{
}

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

    // Under an all-features-present language (`PlainLang` is a `TrivialLang`, so
    // `AllLangFeatures`), the feature-aware `disable_all()` (user rulings
    // 2026-08-10: flip only the gates of features the language has; a gateless
    // feature's off is its inactive data) sets up all six gated blocks plus the
    // cleared forbidden set. The partially-absent languages are pinned in
    // tests/lang_features.rs.
    #[test]
    fn disable_all_flips_the_six_gates_and_clears_the_forbidden_set() {
        let overrides: TokenRulesOverrides<PlainLang> = TokenRulesOverrides::disable_all();
        // All six gates off…
        assert_eq!(overrides.whitespace.enabled, Some(false));
        assert_eq!(overrides.paragraphs.enabled, Some(false));
        assert_eq!(overrides.groups.enabled, Some(false));
        assert_eq!(overrides.commands.enabled, Some(false));
        assert_eq!(overrides.comments.enabled, Some(false));
        assert_eq!(overrides.specials.enabled, Some(false));
        // …the gateless forbidden set overridden to empty (its inactive state)…
        assert_eq!(overrides.forbidden_chars.chars.as_deref(), Some(""));
        // …and nothing else touched: the gated blocks are the default plus the gate
        // flips, so rules data (and the expected close) survives for later
        // re-enabling.
        let mut expected: TokenRulesOverrides<PlainLang> = TokenRulesOverrides::default();
        expected.whitespace.enabled = Some(false);
        expected.paragraphs.enabled = Some(false);
        expected.groups.enabled = Some(false);
        expected.commands.enabled = Some(false);
        expected.comments.enabled = Some(false);
        expected.specials.enabled = Some(false);
        expected.forbidden_chars.chars = Some("".into());
        assert_eq!(overrides, expected);
    }

    /// A populated rules value to mirror: every block carries data, including the
    /// two transient group fields (the ones `override_all` must *not* carry).
    fn populated_rules() -> TokenRules<PlainLang> {
        let brace = Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let bracket = Arc::new(GroupRule { group_type: 1, open: "[".into(), close: "]".into() });
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: vec![Arc::clone(&brace)],
                temporary: vec![Arc::clone(&bracket)],
                expecting_close: Some(Arc::clone(&bracket)),
            },
            commands: CommandRules {
                enabled: true,
                rules: vec![Arc::new(CommandRule { escape_char: '\\', name_chars: "az".into() })],
            },
            comments: CommentRules {
                enabled: true,
                rules: vec![Arc::new(CommentRule { start: "%".into() })],
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "#".into() },
        }
    }

    // `override_all()` sets every field of every present block from the source
    // rules — except the two transient group fields, which are in-flight
    // structural expectations and are never carried (user ruling 2026-08-18,
    // matching the exclusion `exit_math_context_delta` already spells out).
    #[test]
    fn override_all_mirrors_every_field_but_the_transient_group_ones() {
        let rules = populated_rules();
        let overrides = TokenRulesOverrides::<PlainLang>::override_all(&rules);

        assert_eq!(overrides.whitespace.enabled, Some(true));
        assert_eq!(overrides.whitespace.chars.as_deref(), Some(" \t"));
        assert_eq!(overrides.paragraphs.enabled, Some(true));
        assert_eq!(overrides.groups.enabled, Some(true));
        assert_eq!(overrides.groups.rules.as_deref(), Some(&rules.groups.rules[..]));
        assert_eq!(overrides.commands.enabled, Some(true));
        assert_eq!(overrides.commands.rules.as_deref(), Some(&rules.commands.rules[..]));
        assert_eq!(overrides.comments.enabled, Some(true));
        assert_eq!(overrides.comments.rules.as_deref(), Some(&rules.comments.rules[..]));
        assert_eq!(overrides.specials.enabled, Some(true));
        assert_eq!(overrides.forbidden_chars.chars.as_deref(), Some("#"));

        // The two transient fields stay untouched, whatever the source held.
        assert!(overrides.groups.temporary.is_none());
        assert!(overrides.groups.expecting_close.is_none());

        // The whole value is exactly the composition of the seven block
        // constructors — no field decided anywhere else.
        assert_eq!(
            overrides,
            TokenRulesOverrides::<PlainLang> {
                whitespace: WhitespaceOverrides::override_all(&rules.whitespace),
                paragraphs: ParagraphOverrides::override_all(&rules.paragraphs),
                groups: GroupOverrides::override_all(&rules.groups),
                commands: CommandOverrides::override_all(&rules.commands),
                comments: CommentOverrides::override_all(&rules.comments),
                specials: SpecialsOverrides::override_all(&rules.specials),
                forbidden_chars: ForbiddenCharsOverrides::override_all(&rules.forbidden_chars),
            }
        );
    }

    // Applying `override_all(&source)` to any starting value installs `source`'s
    // rules wholesale — the transient group fields excepted, which keep the
    // target's own (here: the all-empty target's).
    #[test]
    fn applying_override_all_installs_the_source_rules() {
        let source = populated_rules();
        let mut target = TokenRules::<PlainLang>::empty();
        TokenRulesOverrides::override_all(&source).apply(&mut target);

        assert_eq!(target.whitespace, source.whitespace);
        assert_eq!(target.paragraphs, source.paragraphs);
        assert_eq!(target.commands, source.commands);
        assert_eq!(target.comments, source.comments);
        assert_eq!(target.specials, source.specials);
        assert_eq!(target.forbidden_chars, source.forbidden_chars);
        assert_eq!(target.groups.enabled, source.groups.enabled);
        // Rule identity survives the round trip: the `Arc`s are the source's own,
        // not data-equal copies (the identity comparisons of `GroupRule`).
        assert!(Arc::ptr_eq(&target.groups.rules[0], &source.groups.rules[0]));
        // …while the transients were not carried.
        assert!(target.groups.temporary.is_empty());
        assert!(target.groups.expecting_close.is_none());
    }

    // The gate-carrying blocks mirror a *disabled* source just as faithfully:
    // `override_all` is a copy of the source's own gates, not an enable-all.
    #[test]
    fn override_all_carries_the_source_gates_as_they_are() {
        let mut source = populated_rules();
        source.commands.enabled = false;
        source.comments.enabled = false;
        let overrides = TokenRulesOverrides::<PlainLang>::override_all(&source);
        assert_eq!(overrides.commands.enabled, Some(false));
        assert_eq!(overrides.comments.enabled, Some(false));
        // The disabled features' data still travels — the scoped off keeps it in
        // place for a later re-enable.
        assert_eq!(overrides.commands.rules.as_deref(), Some(&source.commands.rules[..]));
    }
}
