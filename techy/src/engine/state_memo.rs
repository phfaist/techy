//! Key types and identity-based hashing for the session's derivation memos.
//!
//! [`ParserSession::derived_state`](super::ParserSession::derived_state) memoizes
//! derivations whose delta carries only token-rule overrides and/or a mode override
//! (no ext replacement, no events, no scope ops). The key is the base state's `Arc`
//! identity plus those overrides: rule payloads by `Arc` identity, the per-block
//! `enabled` gates and the mode override by value.
//!
//! Identity keying is conservatively correct. Pointer-equal inputs are value-equal,
//! and `derived()` is a pure function of (base data, delta, events), so a hit is
//! exact; a miss is possible on value-equal but distinct `Arc`s. The mode key is a
//! `Copy + Eq` value, so it cannot even miss.
//!
//! Keys own their `Arc`s. An entry therefore pins the allocations of its base state
//! and payload rules, so a live `Arc` that compares pointer-equal to a stored key is
//! the same object — no address reuse, no ABA false hits. The retention that implies
//! (entries live until the session drops) is bounded by one transient parse, and most
//! memoized states are pinned by the node tree anyway.
//!
//! Lookups allocate nothing: they go through the borrowed [`StateMemoProbe`] view
//! (hashbrown's `Equivalent` seam), and the owned [`StateMemoKey`] is built only on
//! insert.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

use hashbrown::Equivalent;

use crate::state::{
    FeaturePresence, Lang, LangFeatures, ParsingState, ParsingStateDelta, TokenRulesOverrides,
};
use crate::token::GroupRule;

/// The memo map: owned keys → memoized derived states.
pub(super) type StateMemo<L> = hashbrown::HashMap<StateMemoKey<L>, Arc<ParsingState<L>>>;

/// The group-interior memo map: one entry per `(base, rule)` descent.
///
/// Deliberately separate from [`StateMemo`]. A group-interior derivation is the
/// canonical `expecting_group_close` override *plus* the driver's
/// [`group_interior_delta`](crate::engine::ParseDriver::group_interior_delta), which
/// runs on a memo miss only; sharing [`StateMemo`] would let a hand-built
/// expecting-close-only delta and a driver-augmented descent collide under one key
/// while deriving different states.
///
/// Keyed on the `(base, rule)` `Arc` identities, which is sound because the driver
/// hook is pure per `(state, rule)`. The entry stores the merged delta, so a hit can
/// still pass the true delta to `observe_transition`. Keys own their `Arc`s for the
/// same no-ABA reason as [`StateMemoKey`].
pub(super) type GroupInteriorMemo<L> =
    hashbrown::HashMap<GroupInteriorKey<L>, GroupInteriorEntry<L>>;

/// One memoized group-interior derivation: the frozen interior state and the merged
/// delta that produced it (canonical expecting-close + driver descent delta).
pub(super) struct GroupInteriorEntry<L: Lang> {
    pub(super) state: Arc<ParsingState<L>>,
    pub(super) delta: Arc<ParsingStateDelta<L>>,
}

/// Owned key of one group-interior memo entry.
pub(super) struct GroupInteriorKey<L: Lang> {
    pub(super) base: Arc<ParsingState<L>>,
    pub(super) rule: Arc<GroupRule<L>>,
}

/// Borrowed probe view of a [`GroupInteriorKey`].
pub(super) struct GroupInteriorProbe<'a, L: Lang> {
    pub(super) base: &'a Arc<ParsingState<L>>,
    pub(super) rule: &'a Arc<GroupRule<L>>,
}

impl<L: Lang> PartialEq for GroupInteriorKey<L> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.base, &other.base) && Arc::ptr_eq(&self.rule, &other.rule)
    }
}

impl<L: Lang> Eq for GroupInteriorKey<L> {}

impl<L: Lang> Hash for GroupInteriorKey<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        arc_addr(&self.base).hash(state);
        arc_addr(&self.rule).hash(state);
    }
}

impl<L: Lang> Hash for GroupInteriorProbe<'_, L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        arc_addr(self.base).hash(state);
        arc_addr(self.rule).hash(state);
    }
}

impl<L: Lang> Equivalent<GroupInteriorKey<L>> for GroupInteriorProbe<'_, L> {
    fn equivalent(&self, key: &GroupInteriorKey<L>) -> bool {
        Arc::ptr_eq(self.base, &key.base) && Arc::ptr_eq(self.rule, &key.rule)
    }
}

/// Owned key of one derivation-memo entry (the module docs explain why it owns its
/// `Arc`s).
pub(super) struct StateMemoKey<L: Lang> {
    pub(super) base: Arc<ParsingState<L>>,
    pub(super) mode: Option<L::ModeId>,
    pub(super) rules: TokenRulesOverrides<L>,
}

/// Borrowed probe view of a [`StateMemoKey`]: hashes and compares exactly like the
/// owned key, cloning nothing (the mode override is `Copy`, so it is carried by value).
pub(super) struct StateMemoProbe<'a, L: Lang> {
    pub(super) base: &'a Arc<ParsingState<L>>,
    pub(super) mode: Option<L::ModeId>,
    pub(super) rules: &'a TokenRulesOverrides<L>,
}

impl<L: Lang> PartialEq for StateMemoKey<L> {
    fn eq(&self, other: &Self) -> bool {
        keys_eq(&self.base, self.mode, &self.rules, &other.base, other.mode, &other.rules)
    }
}

impl<L: Lang> Eq for StateMemoKey<L> {}

impl<L: Lang> Hash for StateMemoKey<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_key(&self.base, self.mode, &self.rules, state)
    }
}

impl<L: Lang> Hash for StateMemoProbe<'_, L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_key(self.base, self.mode, self.rules, state)
    }
}

impl<L: Lang> Equivalent<StateMemoKey<L>> for StateMemoProbe<'_, L> {
    fn equivalent(&self, key: &StateMemoKey<L>) -> bool {
        keys_eq(self.base, self.mode, self.rules, &key.base, key.mode, &key.rules)
    }
}

fn arc_addr<T>(arc: &Arc<T>) -> usize {
    Arc::as_ptr(arc) as usize
}

fn str_addr(arc: &Arc<str>) -> usize {
    Arc::as_ptr(arc) as *const u8 as usize
}

// hash_key/keys_eq walk the override blocks field by field: gates and the mode by
// value, every rule payload by `Arc` identity. Every field of every *present*
// feature's block is covered — none skipped, none added. A block whose feature the
// language declares absent has zero-sized storage and does not exist at all, so both
// functions reach a block through the `store_get` projection — `Some` exactly for
// present features. The projections stay in lockstep (the same presence declaration
// guards the same fields in both functions), so hash-equal keys are still exactly the
// keys_eq-equal keys.
fn hash_key<L: Lang, H: Hasher>(
    base: &Arc<ParsingState<L>>,
    mode: Option<L::ModeId>,
    rules: &TokenRulesOverrides<L>,
    h: &mut H,
) {
    arc_addr(base).hash(h);
    mode.hash(h);
    if let Some(whitespace) = <L::Features as LangFeatures>::Whitespace::store_get(&rules.whitespace)
    {
        whitespace.enabled.hash(h);
        match &whitespace.chars {
            None => h.write_u8(0),
            Some(chars) => {
                h.write_u8(1);
                str_addr(chars).hash(h);
            }
        }
    }
    if let Some(paragraphs) = <L::Features as LangFeatures>::Paragraphs::store_get(&rules.paragraphs)
    {
        paragraphs.enabled.hash(h);
    }
    if let Some(groups) = <L::Features as LangFeatures>::Groups::store_get(&rules.groups) {
        groups.enabled.hash(h);
        hash_arc_slice(&groups.rules, h);
        hash_arc_slice(&groups.temporary, h);
    }
    if let Some(commands) = <L::Features as LangFeatures>::Commands::store_get(&rules.commands) {
        commands.enabled.hash(h);
        hash_arc_slice(&commands.rules, h);
    }
    if let Some(comments) = <L::Features as LangFeatures>::Comments::store_get(&rules.comments) {
        comments.enabled.hash(h);
        hash_arc_slice(&comments.rules, h);
    }
    if let Some(specials) = <L::Features as LangFeatures>::Specials::store_get(&rules.specials) {
        specials.enabled.hash(h);
    }
    if let Some(forbidden_chars) =
        <L::Features as LangFeatures>::ForbiddenChars::store_get(&rules.forbidden_chars)
    {
        match &forbidden_chars.chars {
            None => h.write_u8(0),
            Some(chars) => {
                h.write_u8(1);
                str_addr(chars).hash(h);
            }
        }
    }
    // expecting_close is hashed last, preserving the original field order.
    if let Some(groups) = <L::Features as LangFeatures>::Groups::store_get(&rules.groups) {
        match &groups.expecting_close {
            None => h.write_u8(0),
            Some(None) => h.write_u8(1),
            Some(Some(rule)) => {
                h.write_u8(2);
                arc_addr(rule).hash(h);
            }
        }
    }
}

fn hash_arc_slice<T, H: Hasher>(field: &Option<Vec<Arc<T>>>, h: &mut H) {
    match field {
        None => h.write_u8(0),
        Some(items) => {
            h.write_u8(1);
            items.len().hash(h);
            for item in items {
                arc_addr(item).hash(h);
            }
        }
    }
}

fn keys_eq<L: Lang>(
    a_base: &Arc<ParsingState<L>>,
    a_mode: Option<L::ModeId>,
    a: &TokenRulesOverrides<L>,
    b_base: &Arc<ParsingState<L>>,
    b_mode: Option<L::ModeId>,
    b: &TokenRulesOverrides<L>,
) -> bool {
    Arc::ptr_eq(a_base, b_base)
        && a_mode == b_mode
        && stores_eq::<<L::Features as LangFeatures>::Whitespace, _>(
            &a.whitespace,
            &b.whitespace,
            |x, y| {
                x.enabled == y.enabled && opt_eq_by(&x.chars, &y.chars, Arc::ptr_eq)
            },
        )
        && stores_eq::<<L::Features as LangFeatures>::Paragraphs, _>(
            &a.paragraphs,
            &b.paragraphs,
            |x, y| x.enabled == y.enabled,
        )
        && stores_eq::<<L::Features as LangFeatures>::Groups, _>(&a.groups, &b.groups, |x, y| {
            x.enabled == y.enabled
                && opt_eq_by(&x.rules, &y.rules, |x, y| arc_slices_eq(x, y))
                && opt_eq_by(&x.temporary, &y.temporary, |x, y| arc_slices_eq(x, y))
        })
        && stores_eq::<<L::Features as LangFeatures>::Commands, _>(
            &a.commands,
            &b.commands,
            |x, y| {
                x.enabled == y.enabled
                    && opt_eq_by(&x.rules, &y.rules, |x, y| arc_slices_eq(x, y))
            },
        )
        && stores_eq::<<L::Features as LangFeatures>::Comments, _>(
            &a.comments,
            &b.comments,
            |x, y| {
                x.enabled == y.enabled
                    && opt_eq_by(&x.rules, &y.rules, |x, y| arc_slices_eq(x, y))
            },
        )
        && stores_eq::<<L::Features as LangFeatures>::Specials, _>(
            &a.specials,
            &b.specials,
            |x, y| x.enabled == y.enabled,
        )
        && stores_eq::<<L::Features as LangFeatures>::ForbiddenChars, _>(
            &a.forbidden_chars,
            &b.forbidden_chars,
            |x, y| opt_eq_by(&x.chars, &y.chars, Arc::ptr_eq),
        )
        // expecting_close is compared last, mirroring hash_key's field order.
        && stores_eq::<<L::Features as LangFeatures>::Groups, _>(&a.groups, &b.groups, |x, y| {
            opt_eq_by(&x.expecting_close, &y.expecting_close, |x, y| {
                opt_eq_by(x, y, Arc::ptr_eq)
            })
        })
}

/// Compare two same-feature stores: both projections are `Some` (present feature —
/// compare the blocks with `eq`) or both `None` (absent — the zero-sized stores are
/// trivially equal). A mixed pair cannot occur (both stores carry the same presence
/// marker); answering `false` for it keeps the impossible case conservative — a memo
/// miss, never a false hit.
fn stores_eq<P: FeaturePresence, T: Clone + core::fmt::Debug + Send + Sync>(
    a: &P::Store<T>,
    b: &P::Store<T>,
    eq: impl FnOnce(&T, &T) -> bool,
) -> bool {
    match (P::store_get(a), P::store_get(b)) {
        (Some(x), Some(y)) => eq(x, y),
        (None, None) => true,
        _ => false,
    }
}

fn opt_eq_by<T>(a: &Option<T>, b: &Option<T>, eq: impl Fn(&T, &T) -> bool) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => eq(x, y),
        _ => false,
    }
}

fn arc_slices_eq<T>(a: &[Arc<T>], b: &[Arc<T>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| Arc::ptr_eq(x, y))
}
