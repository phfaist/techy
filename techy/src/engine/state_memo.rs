//! The session's derivation memo: key types and identity-based hashing.
//!
//! [`ParserSession::derived_state`](super::ParserSession::derived_state) memoizes the
//! *gated* subset of state derivations — deltas that carry only token-rules overrides
//! and/or a mode override (no ext replacement, no events, no scope ops) — keyed on
//! the base state's `Arc` identity plus the overrides, with every rule payload taken by
//! `Arc` identity and the per-block `enabled` gates and the mode override by value
//! (modes are
//! `Copy + Eq` vocabulary — value keying is *exact* for them, not conservative).
//! Pointer-equal inputs imply value-equal inputs, and `derived()` is a pure function of
//! (base data, delta, events), so identity keying is conservatively correct: it can
//! only miss (value-equal but distinct `Arc`s), never falsely hit.
//!
//! **Why keys own their `Arc`s:** an entry pins the allocations of its base state and
//! payload rules, so a live `Arc` that compares pointer-equal to a stored key is
//! necessarily the *same* object — no address reuse, no ABA false hits. The retention
//! this implies (entries live until the session drops) is a decided trade: a session is one transient parse, and most
//! memoized states end up pinned by the node tree anyway.
//!
//! Probes are allocation-free: lookups go through the borrowed [`StateMemoProbe`] view
//! (hashbrown's `Equivalent` seam); the owned [`StateMemoKey`] is materialized only on
//! insert.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

use hashbrown::Equivalent;

use crate::state::{Lang, ParsingState, ParsingStateDelta, TokenRulesOverrides};
use crate::token::GroupRule;

/// The memo map: owned keys → memoized derived states.
pub(super) type StateMemo<L> = hashbrown::HashMap<StateMemoKey<L>, Arc<ParsingState<L>>>;

/// The **group-interior** memo map: one entry per `(base, rule)` descent,
/// deliberately separate from [`StateMemo`]. The group-interior derivation is its own
/// operation — the canonical `expecting_group_close` override *plus* the driver's
/// [`group_interior_delta`](crate::engine::ParseDriver::group_interior_delta), which
/// runs on memo **miss** only. Sharing [`StateMemo`] would be unsound: a hand-built
/// expecting-close-only delta and a driver-augmented descent would collide under one
/// key while deriving different states. Keyed on `(base, rule)` `Arc` identities
/// (sound because the driver hook is pure per `(state, rule)`); the entry stores the
/// merged delta so hits can hand the true delta to `observe_transition`. Keys own
/// their `Arc`s for the same no-ABA reason as [`StateMemoKey`].
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

/// Owned key of one memo entry (see the module docs for the pinning rationale).
pub(super) struct StateMemoKey<L: Lang> {
    pub(super) base: Arc<ParsingState<L>>,
    pub(super) mode: Option<L::ModeId>,
    pub(super) rules: TokenRulesOverrides<L>,
}

/// Borrowed probe view of a [`StateMemoKey`]: hashes and compares exactly like the
/// owned key without cloning anything (the mode override is `Copy` — carried by value).
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

// hash_key/keys_eq walk the override blocks field by field, in the original
// (pre-regrouping) field order, so hash values and equality answers are unchanged by
// the M1 regrouping: gates and the mode by value, every rule payload by `Arc`
// identity. Every field of every block is covered — none skipped, none added. At M3,
// a compile-time-absent feature's block collapses to a ZST and will hash as nothing;
// until then every block hashes unconditionally.
fn hash_key<L: Lang, H: Hasher>(
    base: &Arc<ParsingState<L>>,
    mode: Option<L::ModeId>,
    rules: &TokenRulesOverrides<L>,
    h: &mut H,
) {
    arc_addr(base).hash(h);
    mode.hash(h);
    rules.whitespace.enabled.hash(h);
    match &rules.whitespace.chars {
        None => h.write_u8(0),
        Some(chars) => {
            h.write_u8(1);
            str_addr(chars).hash(h);
        }
    }
    rules.paragraphs.enabled.hash(h);
    rules.groups.enabled.hash(h);
    hash_arc_slice(&rules.groups.rules, h);
    hash_arc_slice(&rules.groups.temporary, h);
    rules.commands.enabled.hash(h);
    hash_arc_slice(&rules.commands.rules, h);
    rules.comments.enabled.hash(h);
    hash_arc_slice(&rules.comments.rules, h);
    rules.specials.enabled.hash(h);
    match &rules.forbidden_chars.chars {
        None => h.write_u8(0),
        Some(chars) => {
            h.write_u8(1);
            str_addr(chars).hash(h);
        }
    }
    match &rules.groups.expecting_close {
        None => h.write_u8(0),
        Some(None) => h.write_u8(1),
        Some(Some(rule)) => {
            h.write_u8(2);
            arc_addr(rule).hash(h);
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
        && a.whitespace.enabled == b.whitespace.enabled
        && opt_eq_by(&a.whitespace.chars, &b.whitespace.chars, Arc::ptr_eq)
        && a.paragraphs.enabled == b.paragraphs.enabled
        && a.groups.enabled == b.groups.enabled
        && opt_eq_by(&a.groups.rules, &b.groups.rules, |x, y| arc_slices_eq(x, y))
        && opt_eq_by(&a.groups.temporary, &b.groups.temporary, |x, y| arc_slices_eq(x, y))
        && a.commands.enabled == b.commands.enabled
        && opt_eq_by(&a.commands.rules, &b.commands.rules, |x, y| arc_slices_eq(x, y))
        && a.comments.enabled == b.comments.enabled
        && opt_eq_by(&a.comments.rules, &b.comments.rules, |x, y| arc_slices_eq(x, y))
        && a.specials.enabled == b.specials.enabled
        && opt_eq_by(&a.forbidden_chars.chars, &b.forbidden_chars.chars, Arc::ptr_eq)
        && opt_eq_by(&a.groups.expecting_close, &b.groups.expecting_close, |x, y| {
            opt_eq_by(x, y, Arc::ptr_eq)
        })
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
