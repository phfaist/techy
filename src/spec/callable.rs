//! [`CallableSpec`]: the behavior of anything invocable from the token stream; plus the
//! [`CallableTypeId`] invocation-form registry id and the standard declarative
//! implementation [`StdCallableSpec`].

use alloc::vec::Vec;
use core::fmt;

use crate::state::Lang;

use super::structure::{ArgumentStructureSpec, SlotStructureSpec};

/// Identifier of a callable *type* — a registered invocation form (the latexlike preset
/// registers `MACRO` / `ENVIRONMENT` / `SPECIALS`). Open registry per the naming rule
/// (`…TypeId`, not `…Kind`): invocation forms are preset-/user-registered, not a closed
/// core enum.
///
/// Interning machinery arrives with `Language<L>` (Phase 6), exactly like
/// [`GroupTypeId`](crate::token::GroupTypeId); until then ids are constructed directly.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallableTypeId(u32);

impl CallableTypeId {
    /// Create a callable type id from a raw index.
    pub const fn new(index: u32) -> CallableTypeId {
        CallableTypeId(index)
    }

    /// The raw index of this id.
    pub const fn index(&self) -> u32 {
        self.0
    }
}

impl fmt::Debug for CallableTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CallableTypeId({})", self.0)
    }
}

/// Behavior of anything invocable from the token stream. De-keyed: carries no name and no
/// invocation form; one spec may back several names (`\emph` and `\textit` can share), and
/// per-[`CallableTypeId`] unknown-callable fallbacks can be shared singletons
/// (ARCHITECTURE.md §specs).
///
/// The default method bodies describe the neutral callable — no arguments, no slots —
/// suitable for simple specials (`~`) and fallback specs. The declarative standard
/// implementation is [`StdCallableSpec`].
///
/// `invocation_parser()` — the parser consuming the invocation, and with it the
/// full-takeover escape hatch for `\verb`-like constructs — arrives in Phase 6 together
/// with `ConstructParser`/`ParseContext` (DESIGN_RATIONALE.md §3.4).
pub trait CallableSpec<L: Lang>: fmt::Debug {
    /// The declarative argument structure of an invocation. Default: no arguments.
    fn arguments(&self) -> &ArgumentStructureSpec {
        static NONE: ArgumentStructureSpec = ArgumentStructureSpec { arguments: Vec::new() };
        &NONE
    }

    /// The declarative slot (content-region) structure. Default: no slots.
    fn slots(&self) -> &SlotStructureSpec {
        static NONE: SlotStructureSpec = SlotStructureSpec { slots: Vec::new() };
        &NONE
    }
}

/// The standard declarative [`CallableSpec`]: the two structure specs as plain data.
///
/// Not generic over `L` — one value serves every language (it is the *dyn trait impl*
/// that mentions `L`). The custom invocation-parser override slot arrives in Phase 6.
///
/// The preset-level constructor helpers (`MacroSpec` / `EnvironmentSpec` /
/// `SpecialsSpec`) that build `StdCallableSpec`s live in the latexlike preset (Phase 7)
/// — "macro" and "environment" are invocation forms, not core concepts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StdCallableSpec {
    /// The argument structure.
    pub arguments: ArgumentStructureSpec,
    /// The slot structure.
    pub slots: SlotStructureSpec,
}

impl StdCallableSpec {
    /// A spec with the given argument and slot structures.
    pub fn new(arguments: ArgumentStructureSpec, slots: SlotStructureSpec) -> StdCallableSpec {
        StdCallableSpec { arguments, slots }
    }
}

impl<L: Lang> CallableSpec<L> for StdCallableSpec {
    fn arguments(&self) -> &ArgumentStructureSpec {
        &self.arguments
    }

    fn slots(&self) -> &SlotStructureSpec {
        &self.slots
    }
}
