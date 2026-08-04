//! The recompose driver: the [`recompose`] entry point and the run context
//! ([`RecomposeContext`]) handed to recomposers.

use core::marker::PhantomData;
use core::ops::Range;

use alloc::string::String;

use crate::node::{BodySlotExt, CallableData, NodeRef, NodeTree, SlotExt};
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

impl<L: Lang, A> RecomposeContext<'_, L, A> {
    // --- the region ops (the restage-family mirror) -------------------------------------

    /// Recompose argument `index` of callable `node` — the **whole region**
    /// (leading noise, wrapper syntax, and content, in source order) folded
    /// through `recomposer` under `state`. An **absent argument composes the
    /// empty piece** (it contributed nothing — presence semantics, no error).
    ///
    /// Errors: [`NotACallable`](RecomposeError::NotACallable),
    /// [`ArgumentIndexOutOfRange`](RecomposeError::ArgumentIndexOutOfRange),
    /// plus anything the fold produces.
    pub fn recompose_argument<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        index: usize,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
    {
        let argument = self.argument(node, index)?;
        match &argument.region {
            None => Ok(R::Piece::empty()),
            Some(region) => self.fold_nodes(node, region.children(), state, recomposer),
        }
    }

    /// [`recompose_argument`](RecomposeContext::recompose_argument) by the
    /// argument's spec name. A name matching none of the callable's argument
    /// specs is [`UnknownArgumentName`](RecomposeError::UnknownArgumentName)
    /// — asking for an argument the callable does not have is a caller bug,
    /// not an absence.
    pub fn recompose_argument_named<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        name: &str,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
    {
        let index = self.argument_index(node, name)?;
        self.recompose_argument(node, index, state, recomposer)
    }

    /// Recompose the **designated content nodes** of argument `index` of
    /// callable `node` (noise and wrapper syntax excluded — exactly what the
    /// argument's parser designated). An absent argument composes the empty
    /// piece.
    ///
    /// Errors: as [`recompose_argument`](RecomposeContext::recompose_argument).
    pub fn recompose_argument_content<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        index: usize,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
    {
        let argument = self.argument(node, index)?;
        match &argument.region {
            None => Ok(R::Piece::empty()),
            Some(region) => self.fold_nodes(node, region.content_range(), state, recomposer),
        }
    }

    /// [`recompose_argument_content`](RecomposeContext::recompose_argument_content)
    /// by the argument's spec name (unknown name =
    /// [`UnknownArgumentName`](RecomposeError::UnknownArgumentName)).
    pub fn recompose_argument_content_named<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        name: &str,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
    {
        let index = self.argument_index(node, name)?;
        self.recompose_argument_content(node, index, state, recomposer)
    }

    /// Recompose the **content nodes** of slot `index` of callable `node`
    /// (the body nodes, for the standard environment shape) folded through
    /// `recomposer` under `state`.
    ///
    /// Errors: [`NotACallable`](RecomposeError::NotACallable),
    /// [`SlotIndexOutOfRange`](RecomposeError::SlotIndexOutOfRange), plus
    /// anything the fold produces.
    pub fn recompose_slot_content<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        index: usize,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
    {
        let data = callable_data(node)?;
        let count = data.slots.len();
        let slot = data
            .slots
            .get(index)
            .ok_or(RecomposeError::SlotIndexOutOfRange { node: node.id(), index, count })?;
        self.fold_nodes(node, slot.region.content_range(), state, recomposer)
    }

    /// [`recompose_slot_content`](RecomposeContext::recompose_slot_content)
    /// by the slot's recorded name. A name matching none of the callable's
    /// slots is [`UnknownSlotName`](RecomposeError::UnknownSlotName).
    pub fn recompose_slot_content_named<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        name: &str,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
    {
        let data = callable_data(node)?;
        let slot = data.slots.get_named(name).ok_or_else(|| {
            RecomposeError::UnknownSlotName { node: node.id(), name: String::from(name) }
        })?;
        self.fold_nodes(node, slot.region.content_range(), state, recomposer)
    }

    /// Recompose the content of callable `node`'s **body slot** — the first
    /// slot whose ext reports
    /// [`is_body`](crate::core::node::BodySlotExt::is_body) (the
    /// [`NodeRef::body`](crate::core::node::NodeRef::body) selection: the ext
    /// axis alone, no role conjunction).
    ///
    /// Errors: [`NotACallable`](RecomposeError::NotACallable),
    /// [`NoBodySlot`](RecomposeError::NoBodySlot) (no slot ext designates a
    /// body — a macro-shaped callable, or a language whose exts mark none),
    /// plus anything the fold produces.
    pub fn recompose_body<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
        SlotExt<L>: BodySlotExt,
    {
        let data = callable_data(node)?;
        let slot = data
            .slots
            .iter()
            .find(|slot| slot.ext.is_body())
            .ok_or(RecomposeError::NoBodySlot { node: node.id() })?;
        self.fold_nodes(node, slot.region.content_range(), state, recomposer)
    }

    // --- shared tails -------------------------------------------------------------------

    /// Argument `index` of callable `node`, or the op-misuse error.
    fn argument<'n, E>(
        &self,
        node: NodeRef<'n, L, A>,
        index: usize,
    ) -> Result<&'n crate::node::ParsedArgument<L>, RecomposeError<E>> {
        let data = callable_data(node)?;
        let count = data.arguments.len();
        data.arguments
            .get(index)
            .ok_or(RecomposeError::ArgumentIndexOutOfRange { node: node.id(), index, count })
    }

    /// The index of the argument named `name`, or the op-misuse error.
    fn argument_index<E>(
        &self,
        node: NodeRef<'_, L, A>,
        name: &str,
    ) -> Result<usize, RecomposeError<E>> {
        let data = callable_data(node)?;
        data.arguments
            .iter()
            .position(|argument| argument.name() == Some(name))
            .ok_or_else(|| RecomposeError::UnknownArgumentName {
                node: node.id(),
                name: String::from(name),
            })
    }

    /// Fold one contiguous node range of `node`'s tree through the passed
    /// recomposer — the ops' shared tail. Self-passing keeps the wrapping
    /// contract intact: the sub-fold lowers against whatever recomposer the
    /// caller hands in (its own outermost self, in the wrapper pattern).
    fn fold_nodes<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        range: Range<u32>,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
    {
        let mut acc = R::Piece::empty();
        for region_node in node.tree().nodes_in(range) {
            acc.append(drive(recomposer, region_node, state, self)?);
        }
        Ok(acc)
    }
}

/// The callable payload of `node`, or the op-misuse error.
fn callable_data<'n, L: Lang, A, E>(
    node: NodeRef<'n, L, A>,
) -> Result<&'n CallableData<L>, RecomposeError<E>> {
    node.callable().ok_or(RecomposeError::NotACallable { node: node.id() })
}

impl<L: Lang, A> core::fmt::Debug for RecomposeContext<'_, L, A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RecomposeContext").finish()
    }
}
