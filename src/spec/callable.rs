//! [`CallableSpec`]: the behavior of anything invocable from the token stream; plus the
//! standard declarative implementation [`StdCallableSpec`].
//!
//! The invocation-form identifier is [`Lang::CallableTypeId`] — a closed per-language
//! type (decided July 2026; formerly an open interned id).

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::state::Lang;

use super::structure::{ArgumentSpec, SlotSpec};

/// Behavior of anything invocable from the token stream. De-keyed: carries no name and no
/// invocation form; one spec may back several names (`\emph` and `\textit` can share), and
/// per-callable-type unknown-callable fallbacks can be shared singletons
/// (ARCHITECTURE.md §specs).
///
/// The declarative surface is the two structure lists — [`ArgumentSpec`]s (arguments
/// *configure* an invocation) and [`SlotSpec`]s (slots hold *content regions*), both
/// pylatexenc-`arguments_spec_list`-shaped: `Arc`-shared so parsed nodes can record which
/// spec each argument was parsed against. The default method bodies describe the neutral
/// callable — no arguments, no slots — suitable for simple specials (`~`) and fallback
/// specs. The declarative standard implementation is [`StdCallableSpec`].
///
/// `make_invocation_parser()` — the factory returning a fresh boxed
/// [`ConstructParser`](crate::constructs::ConstructParser) per resolved
/// [`Invocation`](crate::constructs::Invocation), whose override is the full-takeover
/// escape hatch for `\verb`-like constructs — lands in Phase 6.4 with its default
/// implementation, the declarative `StdInvocationParser` (DESIGN_RATIONALE.md §3.6).
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits, decided July
/// 2026): specs are stored in parsed trees, so `NodeTree: Send + Sync` requires it.
/// Every method takes `&self`, so a stateful implementation needs interior mutability
/// regardless — under this contract that means locks or atomics (`Mutex`/`RwLock`/
/// `OnceLock`, or `spin` on `no_std`), not `RefCell`/`Cell` (DESIGN_RATIONALE.md).
pub trait CallableSpec<L: Lang>: fmt::Debug + Send + Sync {
    /// The declarative argument structure of an invocation, in invocation order.
    /// Default: no arguments.
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &[]
    }

    /// The declarative slot (content-region) structure, in source order. Default: no
    /// slots (macro-shaped); an environment-shaped callable has exactly one (its body).
    fn slots(&self) -> &[Arc<SlotSpec<L>>] {
        &[]
    }
}

/// The standard declarative [`CallableSpec`]: the two structure lists as plain data.
pub struct StdCallableSpec<L: Lang> {
    /// The argument structure.
    pub arguments: Vec<Arc<ArgumentSpec<L>>>,
    /// The slot structure.
    pub slots: Vec<Arc<SlotSpec<L>>>,
}

impl<L: Lang> StdCallableSpec<L> {
    /// A spec with the given argument and slot structures.
    pub fn new(
        arguments: Vec<Arc<ArgumentSpec<L>>>,
        slots: Vec<Arc<SlotSpec<L>>>,
    ) -> StdCallableSpec<L> {
        StdCallableSpec { arguments, slots }
    }
}

impl<L: Lang> CallableSpec<L> for StdCallableSpec<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }

    fn slots(&self) -> &[Arc<SlotSpec<L>>] {
        &self.slots
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug`/`L: Default` although only
// `Arc`s to spec data are stored.

impl<L: Lang> Default for StdCallableSpec<L> {
    fn default() -> Self {
        StdCallableSpec { arguments: Vec::new(), slots: Vec::new() }
    }
}

impl<L: Lang> Clone for StdCallableSpec<L> {
    fn clone(&self) -> Self {
        StdCallableSpec { arguments: self.arguments.clone(), slots: self.slots.clone() }
    }
}

impl<L: Lang> fmt::Debug for StdCallableSpec<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdCallableSpec")
            .field("arguments", &self.arguments)
            .field("slots", &self.slots)
            .finish()
    }
}
