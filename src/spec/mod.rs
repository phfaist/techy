//! Callable specs and definition libraries (Phase 4).
//!
//! Phase 3 ships only the [`CallableSpec`] trait *declaration*: `Specials` tokens carry an
//! `Arc<dyn CallableSpec<L>>` (the specials scan returns both name and spec — see
//! DESIGN_RATIONALE.md §3.2), so the recursive knot between the token and spec topics is
//! tied here early with a minimal contract. The behavior surface (argument/slot structure
//! specs, the invocation parser, `StdCallableSpec`, `Library`/`LibraryStack`/`SpecLookup`,
//! `CallableTypeId` interning) arrives in Phase 4 per ARCHITECTURE.md §specs.

use core::fmt;

use crate::state::Lang;

/// Behavior of anything invocable from the token stream. De-keyed: carries no name and no
/// invocation form; one spec may back several names (ARCHITECTURE.md §specs).
///
/// **Phase 3 stub:** the trait is declared (so tokens and the specials scan can reference
/// it) but has no behavior surface yet; Phase 4 adds `arguments()`, `slots()`, and
/// `invocation_parser()`.
pub trait CallableSpec<L: Lang>: fmt::Debug {}
