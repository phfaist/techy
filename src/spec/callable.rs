//! [`CallableSpec`]: the behavior of anything invocable from the token stream; plus the
//! standard declarative implementation [`StdCallableSpec`].
//!
//! The invocation-form identifier is [`Lang::CallableTypeId`] — a closed per-language
//! type (decided July 2026; formerly an open interned id).

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{ConstructParser, Invocation, StdInvocationParser};
use crate::node::BuildId;
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
/// The behavioral surface is [`make_invocation_parser`](CallableSpec::make_invocation_parser):
/// a factory returning a fresh boxed [`ConstructParser`] per resolved [`Invocation`],
/// defaulting to the declarative [`StdInvocationParser`]. Overriding it is the
/// full-takeover escape hatch for `\verb`-like constructs (DESIGN_RATIONALE.md §3.6).
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

    /// The factory producing this spec's invocation parser: a **fresh boxed parser per
    /// resolved invocation**, ownership moved to the caller, the [`Invocation`]
    /// traveling inside the parser instance (decided July 2026, DESIGN_RATIONALE.md
    /// §3.6 — pylatexenc's `get_node_parser(token)` shape with ownership made
    /// explicit). The dispatch loop resolves the trigger, builds the `Invocation`,
    /// calls this factory, runs `parser.parse(cx)` once, and drops the parser.
    ///
    /// When the parser runs, the trigger token has already been consumed whole by the
    /// dispatching arm; the parser's `cx.state` is the invocation's base state (see
    /// [`StdInvocationParser`]'s module docs for the full contract, including the
    /// post-space rule).
    ///
    /// The default returns the declarative [`StdInvocationParser`]. **Overriding this
    /// factory is the full-takeover escape hatch**: a custom parser reads tokens
    /// however it wants (`\verb` raw content, tabular preambles), stages its own node
    /// shape, and may return a state delta as the invocation's after-effect for
    /// subsequent siblings (`\newcommand`).
    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, L>,
    ) -> Box<dyn ConstructParser<L, Output = BuildId> + 'a>
    where
        's: 'a,
    {
        Box::new(StdInvocationParser::new(invocation))
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
