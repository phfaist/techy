//! The recompose driver ([`TreeRecomposer`]) and the run context
//! ([`RecomposeContext`]) passed to every recomposer call.

use core::marker::PhantomData;
use core::ops::Range;

use alloc::string::String;

use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use crate::node::{BodySlotExt, CallableData, NodeRef, NodeTree, SlotExt};
use crate::state::Lang;
use crate::visit::scoped_children;

use super::{ComposePiece, Recompose, RecomposeError, Recomposer};

/// Drives a [`Recomposer`] over a tree and returns the single value it produces.
///
/// This is the entry point of [`recompose`](super): create one with
/// [`new`](TreeRecomposer::new), configure it with the `with_*` methods, and run it
/// with [`recompose`](TreeRecomposer::recompose).
///
/// ```text
/// let piece = TreeRecomposer::new(&mut recomposer).recompose(&tree, state)?;
/// let piece = TreeRecomposer::new(&mut recomposer)
///     .with_descent_guard_init(StdDescentGuardInit::depth_limit(64))
///     .recompose(&tree, state)?;
/// ```
///
/// The recomposer is borrowed mutably, so whatever state it accumulated during the
/// run stays owned by the caller and can be read back afterwards.
pub struct TreeRecomposer<'v, R: ?Sized> {
    recomposer: &'v mut R,
    descent_guard_init: StdDescentGuardInit,
}

impl<'v, R: ?Sized> TreeRecomposer<'v, R> {
    /// Creates a driver that will ask `recomposer` about each node.
    ///
    /// Configure it with the `with_*` methods, then run it with
    /// [`recompose`](TreeRecomposer::recompose).
    pub fn new(recomposer: &'v mut R) -> TreeRecomposer<'v, R> {
        TreeRecomposer { recomposer, descent_guard_init: StdDescentGuardInit::default() }
    }

    /// Sets how deeply the run may descend: a stack budget, a fixed depth limit,
    /// or no limit ([`StdDescentGuardInit`](crate::core::StdDescentGuardInit)).
    ///
    /// One descent is counted per level of tree nesting, the re-entrant operations
    /// of [`RecomposeContext`] included; exceeding the limit abandons the run with
    /// [`DescentLimitExceeded`](RecomposeError::DescentLimitExceeded). Without this
    /// call the run uses the guard's default, a deliberately tight stack budget
    /// whose refusal message names this method.
    pub fn with_descent_guard_init(
        mut self,
        init: StdDescentGuardInit,
    ) -> TreeRecomposer<'v, R> {
        self.descent_guard_init = init;
        self
    }

    /// Recomposes `tree` into one value.
    ///
    /// The driver asks the recomposer for an [instruction](Recompose) about each
    /// node, starting at the root; it carries out a `Concat` by recomposing the
    /// children it includes and appending their values in document order, and
    /// returns the value of the root. See the [module docs](super) for how state is
    /// threaded, which children a `Concat` includes, and what happens when one
    /// recomposer wraps another.
    ///
    /// `state` is the state the root is recomposed under (`()` for a recomposer
    /// that needs none).
    ///
    /// # Errors
    ///
    /// [`RecomposeError`], which returns the recomposer's own failure unchanged and
    /// otherwise reports the descent guard's refusal.
    pub fn recompose<L, A>(
        self,
        tree: &NodeTree<L, A>,
        state: R::State,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        L: Lang,
        R: Recomposer<L, A>,
    {
        let mut cx = RecomposeContext {
            descent_guard: StdDescentGuard::init(&self.descent_guard_init),
            _input: PhantomData,
        };
        drive(self.recomposer, tree.root(), &state, &mut cx)
    }
}

impl<R: ?Sized> core::fmt::Debug for TreeRecomposer<'_, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TreeRecomposer")
            .field("descent_guard_init", &self.descent_guard_init)
            .finish_non_exhaustive()
    }
}

/// Recompose one node, under the descent guard: ask the guard, ask for the
/// instruction, then either return the emitted value or recompose the children in
/// scope (one recursive call each, under the derived or inherited state) and append
/// their values. The guard's `exit` runs on the success and error paths alike; a
/// refused descent gets no `exit`.
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
    match cx.descent_guard.try_enter() {
        Err(refusal) => {
            return Err(RecomposeError::DescentLimitExceeded { detail: refusal.detail })
        }
        Ok(Some(warning)) => recomposer.observe_descent_warning(warning),
        Ok(None) => {}
    }
    let result = drive_entered(recomposer, node, state, cx);
    cx.descent_guard.exit();
    result
}

/// The granted-descent body of [`drive`].
fn drive_entered<L, A, R>(
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
            let lowering = pieces.into_lowering();
            // The children are recomposed under the derived state when the
            // instruction supplies one, else they inherit the parent's.
            let child_state = lowering.state.as_ref().unwrap_or(state);
            let mut acc = lowering.head;
            let mut first = true;
            for child in
                scoped_children(node, lowering.include_attached, lowering.include_hidden)
            {
                if !first {
                    // Per gap — the ComposePiece Clone requirement.
                    acc.append(lowering.sep.clone());
                }
                first = false;
                acc.append(drive(recomposer, child, child_state, cx)?);
            }
            acc.append(lowering.tail);
            // The instruction's post-processing sees the whole assembled value,
            // head and tail included — after the children were recomposed by this
            // (outermost) recomposer.
            if let Some(map) = lowering.map {
                acc = map(acc);
            }
            Ok(acc)
        }
    }
}

/// The run context of a [`TreeRecomposer`] run, passed to every recomposer call.
///
/// It holds **no user state** — that belongs in the recomposer's own `&mut self`
/// fields or in the downward-threaded state, as in [`visit`](crate::visit). Its
/// surface is the operations below, which recompose one node's children, or one
/// argument's or slot's nodes, on demand. They mirror the operations of
/// [`RestageContext`](crate::transform::RestageContext) on the transform side.
///
/// Every one of them recomposes through the recomposer *you pass in*, which matters
/// when recomposers are wrapped: see the [module docs](super).
pub struct RecomposeContext<'t, L: Lang, A = ()> {
    /// The run's descent guard, consulted by every [`drive`] — the re-entrant
    /// region operations included, since they recompose through this same
    /// context.
    descent_guard: StdDescentGuard,
    /// The run's input tree, present only to anchor the type and lifetime
    /// parameters: the context stores no borrow of it, since the operations take
    /// their nodes explicitly and accept nodes of any tree.
    _input: PhantomData<&'t NodeTree<L, A>>,
}

impl<L: Lang, A> RecomposeContext<'_, L, A> {
    // --- the region ops (the restage-family mirror) -------------------------------------

    /// Recomposes the children of `node` — a callable or any other kind of node —
    /// through `recomposer` under `state`, appending their values in document
    /// order.
    ///
    /// The two flags choose the same children a [`Concat`](Recompose::Concat)
    /// instruction would: children in
    /// [`Attached`](crate::core::node::SlotRole::Attached) and
    /// [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions are left out
    /// unless `include_attached` or `include_hidden` asks for them. Only callables
    /// have slots, so for every other kind both flags make no difference. A node
    /// with no children in scope produces the empty value, not an error.
    ///
    /// This mirrors
    /// [`restage_children`](crate::transform::RestageContext::restage_children) on
    /// the transform side, which needs no such flags because it descends into every
    /// child regardless of role.
    ///
    /// Like every operation here, this one recomposes through the recomposer you
    /// pass in: a recomposer that passes `self` cannot be reached by a recomposer
    /// wrapping it for those children. To post-process a result *without* leaving
    /// the wrapping recomposer out, return the `Concat` instruction with
    /// [`ConcatPieces::map`](crate::recompose::ConcatPieces::map) attached instead.
    ///
    /// # Errors
    ///
    /// Whatever recomposing the children produces, including the descent guard's
    /// refusal — one descent per level, exactly as when the driver carries out an
    /// instruction itself.
    pub fn recompose_children<R>(
        &mut self,
        node: NodeRef<'_, L, A>,
        include_attached: bool,
        include_hidden: bool,
        state: &R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<L, A> + ?Sized,
    {
        let mut acc = R::Piece::empty();
        for child in scoped_children(node, include_attached, include_hidden) {
            acc.append(drive(recomposer, child, state, self)?);
        }
        Ok(acc)
    }

    /// Recomposes the whole of argument `index` of callable `node` through
    /// `recomposer` under `state`: its leading whitespace and comments, its wrapper
    /// syntax, and its content, in document order.
    ///
    /// An argument that was not provided produces the empty value, since it
    /// contributed nothing to the source; that is not an error. To recompose only
    /// the argument's content, use
    /// [`recompose_argument_content`](RecomposeContext::recompose_argument_content).
    ///
    /// # Errors
    ///
    /// [`NotACallable`](RecomposeError::NotACallable) if `node` is not a callable,
    /// [`ArgumentIndexOutOfRange`](RecomposeError::ArgumentIndexOutOfRange) for an
    /// index the callable has no argument at, plus anything recomposing the nodes
    /// produces.
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

    /// Recomposes the argument whose spec is named `name`, otherwise like
    /// [`recompose_argument`](RecomposeContext::recompose_argument).
    ///
    /// # Errors
    ///
    /// [`UnknownArgumentName`](RecomposeError::UnknownArgumentName) when no
    /// argument spec of the callable carries that name — asking for an argument the
    /// callable does not have is a mistake in calling code, not an absent argument
    /// — plus every error of
    /// [`recompose_argument`](RecomposeContext::recompose_argument).
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

    /// Recomposes only the content nodes of argument `index` of callable `node`:
    /// exactly the nodes the argument's parser designated as its content, with the
    /// surrounding whitespace, comments, and wrapper syntax left out.
    ///
    /// An argument that was not provided produces the empty value.
    ///
    /// # Errors
    ///
    /// As [`recompose_argument`](RecomposeContext::recompose_argument).
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

    /// Recomposes the content of the argument whose spec is named `name`,
    /// otherwise like
    /// [`recompose_argument_content`](RecomposeContext::recompose_argument_content).
    ///
    /// # Errors
    ///
    /// [`UnknownArgumentName`](RecomposeError::UnknownArgumentName) when no
    /// argument spec of the callable carries that name, plus every error of
    /// [`recompose_argument_content`](RecomposeContext::recompose_argument_content).
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

    /// Recomposes the content nodes of slot `index` of callable `node` through
    /// `recomposer` under `state` — for the usual shape of an environment, the
    /// nodes of its body.
    ///
    /// # Errors
    ///
    /// [`NotACallable`](RecomposeError::NotACallable),
    /// [`SlotIndexOutOfRange`](RecomposeError::SlotIndexOutOfRange), plus anything
    /// recomposing the nodes produces.
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

    /// Recomposes the content of the slot recorded under `name`, otherwise like
    /// [`recompose_slot_content`](RecomposeContext::recompose_slot_content).
    ///
    /// # Errors
    ///
    /// [`UnknownSlotName`](RecomposeError::UnknownSlotName) when the callable has
    /// no slot of that name, plus every error of
    /// [`recompose_slot_content`](RecomposeContext::recompose_slot_content).
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

    /// Recomposes the content of callable `node`'s body slot: the first slot whose
    /// ext reports [`is_body`](crate::core::node::BodySlotExt::is_body).
    ///
    /// That is the same slot [`NodeRef::body`](crate::core::node::NodeRef::body)
    /// selects — chosen by the ext alone, with the slot's role playing no part.
    ///
    /// # Errors
    ///
    /// [`NotACallable`](RecomposeError::NotACallable),
    /// [`NoBodySlot`](RecomposeError::NoBodySlot) when no slot of the callable is
    /// marked as its body — a callable shaped like a macro, or a language whose
    /// exts mark none — plus anything recomposing the nodes produces.
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

    /// Argument `index` of callable `node`, or the error for a misused
    /// operation.
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

    /// The index of the argument named `name`, or the error for a misused
    /// operation.
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

    /// Recompose one contiguous range of nodes of `node`'s tree through the passed
    /// recomposer, appending their values — the shared tail of the operations
    /// above. Passing the recomposer explicitly is what keeps wrapping working:
    /// the nodes are recomposed by whatever recomposer the caller supplies, which
    /// in the wrapping pattern is its own outermost self.
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

/// The callable payload of `node`, or the error for a misused operation.
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
