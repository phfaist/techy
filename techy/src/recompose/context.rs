//! The recompose driver: the [`TreeRecomposer`] entry point and the run
//! context ([`RecomposeContext`]) handed to recomposers.

use core::marker::PhantomData;
use core::ops::Range;

use alloc::string::String;

use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use crate::node::{BodySlotExt, CallableData, NodeRef, NodeTree, SlotExt};
use crate::state::Lang;
use crate::visit::scoped_children;

use super::{ComposePiece, Recompose, RecomposeError, Recomposer};

/// The recompose driver: holds the recomposer and the run configuration, and
/// [`recompose`](TreeRecomposer::recompose)s a tree — see
/// [`Recomposer`] and the [module docs](super).
///
/// ```text
/// let piece = TreeRecomposer::new(&mut recomposer).recompose(&tree, state)?;
/// let piece = TreeRecomposer::new(&mut recomposer)
///     .with_descent_guard_init(StdDescentGuardInit::depth_limit(64))
///     .recompose(&tree, state)?;
/// ```
///
/// The recomposer is held by `&mut` borrow: its run-spanning state stays
/// caller-owned and is read back after the run.
pub struct TreeRecomposer<'v, R: ?Sized> {
    recomposer: &'v mut R,
    descent_guard_init: StdDescentGuardInit,
}

impl<'v, R: ?Sized> TreeRecomposer<'v, R> {
    /// A driver folding through `recomposer` — configure with the `with_*`
    /// methods, then run with [`recompose`](TreeRecomposer::recompose).
    pub fn new(recomposer: &'v mut R) -> TreeRecomposer<'v, R> {
        TreeRecomposer { recomposer, descent_guard_init: StdDescentGuardInit::default() }
    }

    /// Configure the run's descent guard
    /// ([`StdDescentGuardInit`](crate::core::StdDescentGuardInit): a stack
    /// budget, a depth limit, or off; a traversal costs one descent per tree
    /// nesting level — the re-entrant region ops included). Without this call
    /// the run uses the guard's default — a deliberately tight stack budget
    /// whose refusal names this method family.
    pub fn with_descent_guard_init(
        mut self,
        init: StdDescentGuardInit,
    ) -> TreeRecomposer<'v, R> {
        self.descent_guard_init = init;
        self
    }

    /// Recompose `tree` into one piece: the fold asks the recomposer for an
    /// [instruction](Recompose) per node (root first), lowers `Concat`
    /// instructions over the scoped children, and composes the pieces bottom-up
    /// — see the [module docs](super) for the state model, the scope, and the
    /// wrapping contract. `state` is the root's downward state (`()` for
    /// stateless recomposers).
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

/// Fold one node, guarded: ask the guard, ask the instruction, then emit or
/// lower the concat over the scoped children (recursing per child under the
/// derived or inherited state). The guard's `exit` runs on the success and
/// error paths alike; a refused descent gets no `exit`.
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
            let lowering = pieces.into_parts();
            // The children fold under the derived state when the instruction
            // carries one, else they inherit the parent's.
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
            // The instruction's post-processing sees the whole assembled piece,
            // head and tail included — after the children lowered against this
            // (outermost) recomposer.
            if let Some(map) = lowering.map {
                acc = map(acc);
            }
            Ok(acc)
        }
    }
}

/// The run context of a [`TreeRecomposer`] fold, handed to every recomposer call.
/// It carries **no user state** (the three-channel discipline,
/// [`techy::visit`](crate::visit)); its surface is the self-passing region
/// ops (arriving with the op roster) that re-enter the fold for one node's
/// children or for one argument's/slot's nodes — the recompose mirror of
/// [`RestageContext`](crate::transform::RestageContext)'s op family.
pub struct RecomposeContext<'t, L: Lang, A = ()> {
    /// The run's descent guard, consulted by every [`drive`] — the re-entrant
    /// region ops included, since they fold through this same context.
    descent_guard: StdDescentGuard,
    /// The run's input tree, as a type/lifetime anchor: the context stores no
    /// borrow of it (ops take their nodes explicitly and accept any tree's).
    _input: PhantomData<&'t NodeTree<L, A>>,
}

impl<L: Lang, A> RecomposeContext<'_, L, A> {
    // --- the region ops (the restage-family mirror) -------------------------------------

    /// Recompose the **children** of `node` — any kind of node, callable or
    /// not — folded through `recomposer` under `state`, in source order. The
    /// two flags select the same child scope a
    /// [`Concat`](Recompose::Concat) instruction does:
    /// [`Attached`](crate::core::node::SlotRole::Attached) and
    /// [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions are skipped
    /// unless `include_attached` / `include_hidden` opts them in (only
    /// callables carry slots; for every other kind both flags are moot). A node
    /// with no children in scope composes the empty piece — not an error.
    ///
    /// This is the op-family mirror of
    /// [`restage_children`](crate::transform::RestageContext::restage_children),
    /// and it is **self-passing** like every op here: the sub-fold lowers
    /// against the recomposer the caller hands in, so a recomposer that passes
    /// `self` cannot reach a recomposer wrapping it — those children are folded
    /// by the callee alone. To post-process a fold *without* stepping outside
    /// the wrapping contract, return the `Concat` instruction and attach
    /// [`ConcatPieces::map`](crate::recompose::ConcatPieces::map) instead.
    ///
    /// Errors: whatever the fold produces (the descent guard applies, one
    /// descent per level, exactly as for an instruction's own lowering).
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
