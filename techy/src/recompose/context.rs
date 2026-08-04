//! The recompose driver: the [`recompose`] entry point and the run context
//! ([`RecomposeContext`]) handed to recomposers.

use core::marker::PhantomData;

use crate::node::{NodeRef, NodeTree};
use crate::state::Lang;
use crate::visit::scoped_children;

use super::{ComposePiece, Recompose, RecomposeError, Recomposer};

/// Recompose `tree` into one piece: the fold asks `recomposer` for an
/// [instruction](Recompose) per node (root first), lowers `Concat`
/// instructions over the scoped children, and composes the pieces bottom-up —
/// see the [module docs](super) for the state model, the scope, and the
/// wrapping contract. `state` is the root's downward state (`()` for
/// stateless recomposers).
pub fn recompose<L, A, R>(
    tree: &NodeTree<L, A>,
    state: R::State,
    recomposer: &mut R,
) -> Result<R::Piece, RecomposeError<R::Error>>
where
    L: Lang,
    R: Recomposer<L, A> + ?Sized,
{
    let mut cx = RecomposeContext { _input: PhantomData };
    drive(recomposer, tree.root(), &state, &mut cx)
}

/// Fold one node: ask the instruction, then emit or lower the concat over the
/// scoped children (recursing per child under the derived or inherited
/// state).
pub(super) fn drive<L, A, R>(
    recomposer: &mut R,
    node: NodeRef<'_, L, A>,
    state: &R::State,
    cx: &mut RecomposeContext<'_, L, A>,
) -> Result<R::Piece, RecomposeError<R::Error>>
where
    L: Lang,
    R: Recomposer<L, A> + ?Sized,
{
    match recomposer
        .recompose_node(node, state, cx)
        .map_err(RecomposeError::Recomposer)?
    {
        Recompose::Emit(piece) => Ok(piece),
        Recompose::Concat(pieces) => {
            let (head, sep, tail, derived, include_attached, include_hidden) =
                pieces.into_parts();
            // The children fold under the derived state when the instruction
            // carries one, else they inherit the parent's.
            let child_state = derived.as_ref().unwrap_or(state);
            let mut acc = head;
            let mut first = true;
            for child in scoped_children(node, include_attached, include_hidden) {
                if !first {
                    // Per gap — the ComposePiece Clone requirement.
                    acc.append(sep.clone());
                }
                first = false;
                acc.append(drive(recomposer, child, child_state, cx)?);
            }
            acc.append(tail);
            Ok(acc)
        }
    }
}

/// The run context of a [`recompose`] fold, handed to every recomposer call.
/// It carries **no user state** (the three-channel discipline,
/// [`techy::visit`](crate::visit)); its surface is the self-passing region
/// ops (arriving with the op roster) that re-enter the fold for one
/// argument's/slot's nodes — the recompose mirror of
/// [`RestageContext`](crate::transform::RestageContext)'s op family.
pub struct RecomposeContext<'t, L: Lang, A = ()> {
    /// The run's input tree, as a type/lifetime anchor: the context stores no
    /// borrow of it (ops take their nodes explicitly and accept any tree's).
    _input: PhantomData<&'t NodeTree<L, A>>,
}

impl<L: Lang, A> core::fmt::Debug for RecomposeContext<'_, L, A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RecomposeContext").finish()
    }
}
