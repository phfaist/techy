//! The restage driver ([`TreeRestager`]) and the staging context
//! ([`RestageContext`]) passed to every visitor call.

use core::marker::PhantomData;

use alloc::vec::Vec;

use hashbrown::HashMap;

use alloc::string::String;
use alloc::sync::Arc;

use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use crate::node::{
    BuildId, CallableData, ChildRegion, ContentNodes, ContentParentMapping, NodeBuildError,
    NodeId, NodeKind, NodeRef, NodeTree, NodeTreeBuilder, ParsedArgument, ParsedArguments,
    ParsedSlot, ParsedSlots,
};
use crate::state::Lang;

use super::bundles::ProvidedRegion;
use super::{Restage, RestageError, RestageVisitor, RestagedArgument, RestagedSlot};

/// Drives a [`RestageVisitor`](super::RestageVisitor) over an input tree and
/// assembles the output tree.
///
/// This is the entry point of [`transform`](super): create one with
/// [`new`](TreeRestager::new), configure it with the `with_*` methods, and run it
/// with [`restage`](TreeRestager::restage), which returns the new tree.
///
/// ```text
/// let output = TreeRestager::new(&mut visitor).restage(&tree)?;
/// let output = TreeRestager::new(&mut visitor)
///     .with_descent_guard_init(StdDescentGuardInit::depth_limit(64))
///     .restage(&tree)?;
/// ```
///
/// The visitor is borrowed mutably, so whatever state it accumulated during the
/// run stays owned by the caller and can be read back afterwards.
pub struct TreeRestager<'v, V: ?Sized> {
    visitor: &'v mut V,
    descent_guard_init: StdDescentGuardInit,
}

impl<'v, V: ?Sized> TreeRestager<'v, V> {
    /// Creates a restager that will drive `visitor`.
    ///
    /// Configure it with the `with_*` methods, then run it with
    /// [`restage`](TreeRestager::restage).
    pub fn new(visitor: &'v mut V) -> TreeRestager<'v, V> {
        TreeRestager { visitor, descent_guard_init: StdDescentGuardInit::default() }
    }

    /// Sets how deeply the run may descend: a stack budget, a fixed depth limit,
    /// or no limit ([`StdDescentGuardInit`](crate::core::StdDescentGuardInit)).
    ///
    /// One descent is counted per level of tree nesting, the re-entrant region
    /// operations of [`RestageContext`] included; exceeding the limit abandons the
    /// run with [`DescentLimitExceeded`](RestageError::DescentLimitExceeded).
    /// Without this call the run uses the guard's default, a deliberately tight
    /// stack budget whose refusal message names this method.
    ///
    /// One operation escapes the per-level accounting: a single copy of a subtree
    /// made unchanged (the wrapper copies inside the `_with_content` helpers)
    /// recurses over that whole subtree between two checks of the guard.
    pub fn with_descent_guard_init(mut self, init: StdDescentGuardInit) -> TreeRestager<'v, V> {
        self.descent_guard_init = init;
        self
    }

    /// Transforms `tree` into a new tree, calling the visitor once per node.
    ///
    /// The visitor is called over the frozen input from the root downward, the
    /// root included, and the output nodes are staged from the leaves upward. See
    /// the [module docs](super) for what the visitor may answer, where output
    /// annotations come from, and which edits the driver refuses.
    ///
    /// # Errors
    ///
    /// [`RestageError`], which returns the visitor's own failure unchanged and
    /// otherwise reports what the driver or the output builder rejected. Note that
    /// the root must restage to exactly one node, or the run fails with
    /// [`RootNotSingular`](RestageError::RootNotSingular).
    pub fn restage<L, A, B>(
        self,
        tree: &NodeTree<L, A>,
    ) -> Result<NodeTree<L, B>, RestageError<V::Error>>
    where
        L: Lang,
        V: RestageVisitor<L, A, B>,
    {
        let mut cx = RestageContext {
            builder: NodeTreeBuilder::new(),
            replaced: HashMap::new(),
            descent_guard: StdDescentGuard::init(&self.descent_guard_init),
            _input: PhantomData,
        };
        let ids = drive(&mut cx, tree.root(), self.visitor)?;
        if ids.len() != 1 {
            return Err(RestageError::RootNotSingular { count: ids.len() });
        }
        cx.builder.finish(ids[0]).map_err(RestageError::Build)
    }
}

impl<V: ?Sized> core::fmt::Debug for TreeRestager<'_, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TreeRestager")
            .field("descent_guard_init", &self.descent_guard_init)
            .finish_non_exhaustive()
    }
}

/// Restage one subtree through the visitor, under the descent guard: ask the
/// guard, ask the visitor about `node`, then either recurse over all children and
/// restage the node over their results (`Descend`), or accept the callback's
/// staged replacement (`Emit`). Records the node's replacement in the context's
/// map either way. The guard's `exit` runs on the success and error paths alike;
/// a refused descent gets no `exit`.
pub(super) fn drive<L, A, B, V>(
    cx: &mut RestageContext<'_, L, A, B>,
    node: NodeRef<'_, L, A>,
    visitor: &mut V,
) -> Result<Vec<BuildId>, RestageError<V::Error>>
where
    L: Lang,
    V: RestageVisitor<L, A, B> + ?Sized,
{
    match cx.descent_guard.try_enter() {
        Err(refusal) => {
            return Err(RestageError::DescentLimitExceeded { detail: refusal.detail })
        }
        Ok(Some(warning)) => visitor.observe_descent_warning(warning),
        Ok(None) => {}
    }
    let result = drive_entered(cx, node, visitor);
    cx.descent_guard.exit();
    result
}

/// The granted-descent body of [`drive`].
fn drive_entered<L, A, B, V>(
    cx: &mut RestageContext<'_, L, A, B>,
    node: NodeRef<'_, L, A>,
    visitor: &mut V,
) -> Result<Vec<BuildId>, RestageError<V::Error>>
where
    L: Lang,
    V: RestageVisitor<L, A, B> + ?Sized,
{
    match visitor.restage(node, cx).map_err(RestageError::Visitor)? {
        Restage::Emit(ids) => {
            cx.record_emit(node.id(), &ids);
            Ok(ids)
        }
        Restage::Descend(annotation) => {
            // The safety invariant: Descend ALWAYS descends — every child
            // subtree goes through the visitor (structurally: slot children of
            // any role included).
            let mut replacements = Vec::with_capacity(node.child_count());
            for child in node.children() {
                replacements.push(drive(cx, child, visitor)?);
            }
            let id = cx.restage_over(node, &replacements, annotation)?;
            Ok(alloc::vec![id])
        }
    }
}

/// How one input node was restaged — the record a content-parent designation is
/// translated through.
#[derive(Clone, Debug)]
enum Replaced {
    /// Restaged by the driver over its children's replacements: the staged id
    /// plus the running sums of the replacement lengths of the old children, which
    /// together translate an
    /// [`InChildrenOf`](crate::core::node::ContentNodes::InChildrenOf) content
    /// range through the parent's own restaging.
    Restaged { id: BuildId, prefix: Vec<u32> },
    /// Replaced by an `Emit` with exactly one staged node: content ranges into it
    /// are reproduced unchanged (the visitor chose the replacement's shape) and
    /// checked again at staging.
    One(BuildId),
    /// Replaced by an `Emit` with zero or several staged nodes (the count) —
    /// nothing an `InChildrenOf` designation could be moved onto.
    Count(usize),
}

/// The staging side of a [`TreeRestager`] run, passed to every visitor call.
///
/// Its surface is the region-aware restaging operations below — restage a whole
/// subtree, a node's children, one argument, one slot, or a whole invocation —
/// plus the raw output [`builder()`](RestageContext::builder) they are built on,
/// for anything they do not cover.
///
/// The operations accept input nodes from **any** tree, not only the run's own
/// input (see the [module docs](super)). Restaging the same input node more than
/// once is legal, and each call stages a fresh copy; the context remembers only
/// the most recent copy of a node, and that is the one a later content-parent
/// translation resolves against.
pub struct RestageContext<'t, L: Lang, A, B> {
    builder: NodeTreeBuilder<L, B>,
    /// Input-node id to its staged replacement, recorded for every driven node;
    /// this is what answers the `content_parents` question when a record is
    /// translated. Ids are tagged with their tree, so entries coming from several
    /// input trees never collide.
    replaced: HashMap<NodeId, Replaced>,
    /// The run's descent guard, consulted by every [`drive`] — the re-entrant
    /// region ops included, since they drive through this same context.
    descent_guard: StdDescentGuard,
    /// The run's frozen input tree, present only to anchor the type and lifetime
    /// parameters: the context itself stores no borrow of it, since the operations
    /// take their input nodes explicitly and accept nodes of any tree.
    _input: PhantomData<&'t NodeTree<L, A>>,
}

impl<'t, L: Lang, A, B> RestageContext<'t, L, A, B> {
    /// Returns the output tree's builder, for staging the ready-made operations
    /// do not cover.
    ///
    /// Those operations are conveniences over this builder, not a limit on what a
    /// pass can stage. Building a brand-new node takes two calls: make its ext
    /// with [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) (over
    /// [`staged_children`](NodeTreeBuilder::staged_children)), then stage the node
    /// with [`add`](NodeTreeBuilder::add). A restaged copy of an existing node
    /// instead keeps that node's ext unchanged.
    pub fn builder(&mut self) -> &mut NodeTreeBuilder<L, B> {
        &mut self.builder
    }

    // --- region-aware restaging ops -----------------------------------------------------

    /// Runs the visitor over `node` and its whole subtree, exactly as the driver
    /// does for a child it descended into, and returns the staged ids that replace
    /// `node`.
    ///
    /// That is a single id when the visitor answered `Descend` for `node` itself,
    /// and whatever it emitted otherwise. The usual use is inside a callback that
    /// takes a node over but still wants parts of the tree restaged through the
    /// pass: `Restage::Emit(cx.restage_subtree(other, self)?)`.
    ///
    /// # Errors
    ///
    /// Whatever the visitor run produces, including the descent guard's refusal.
    pub fn restage_subtree<V>(
        &mut self,
        node: NodeRef<'_, L, A>,
        visitor: &mut V,
    ) -> Result<Vec<BuildId>, RestageError<V::Error>>
    where
        V: RestageVisitor<L, A, B> + ?Sized,
    {
        drive(self, node, visitor)
    }

    /// Runs the visitor over every child subtree of `node`, the node itself
    /// excluded, and returns the replacements of all of them concatenated.
    ///
    /// This is how a pass removes a wrapper while its content still goes through
    /// the pass: `Restage::Emit(cx.restage_children(group, self)?)` replaces a
    /// group with its restaged children.
    ///
    /// # Errors
    ///
    /// Whatever the visitor run produces, including the descent guard's refusal.
    pub fn restage_children<V>(
        &mut self,
        node: NodeRef<'_, L, A>,
        visitor: &mut V,
    ) -> Result<Vec<BuildId>, RestageError<V::Error>>
    where
        V: RestageVisitor<L, A, B> + ?Sized,
    {
        let mut ids = Vec::new();
        for child in node.children() {
            ids.extend(drive(self, child, visitor)?);
        }
        Ok(ids)
    }

    /// Restages argument `index` of callable `node` through the visitor, and
    /// returns the result as a [`RestagedArgument`] to hand to
    /// [`restage_invocation`](RestageContext::restage_invocation).
    ///
    /// The nodes of the argument's region go through the visitor like the children
    /// of a node the driver descended into, and the record's spec, presence, and
    /// content designation are reproduced against the staged nodes. An argument
    /// that was not provided in the input is returned as
    /// [`RestagedArgument::absent`]; that is not an error, it is how presence is
    /// preserved.
    ///
    /// # Errors
    ///
    /// [`NotACallable`](RestageError::NotACallable) if `node` is not a callable,
    /// [`ArgumentIndexOutOfRange`](RestageError::ArgumentIndexOutOfRange) for an
    /// index the callable has no argument at,
    /// [`ContentParentDropped`](RestageError::ContentParentDropped) if the visitor
    /// dropped the node the record's content designation points into or replaced
    /// it with several nodes, plus anything the visitor run produces.
    pub fn restage_argument<V>(
        &mut self,
        node: NodeRef<'_, L, A>,
        index: usize,
        visitor: &mut V,
    ) -> Result<RestagedArgument<L>, RestageError<V::Error>>
    where
        V: RestageVisitor<L, A, B> + ?Sized,
    {
        let data = callable_data(node)?;
        let count = data.arguments.len();
        let argument = data
            .arguments
            .get(index)
            .ok_or(RestageError::ArgumentIndexOutOfRange { node: node.id(), index, count })?;
        let spec = Arc::clone(&argument.spec);
        match &argument.region {
            None => Ok(RestagedArgument::absent(spec)),
            Some(region) => {
                let ext = argument.ext.clone();
                let (nodes, content) = self.restage_region(node, region, visitor)?;
                Ok(RestagedArgument {
                    spec,
                    provided: Some(ProvidedRegion { nodes, content, ext }),
                })
            }
        }
    }

    /// Restages the argument whose spec is named `name`, otherwise like
    /// [`restage_argument`](RestageContext::restage_argument).
    ///
    /// # Errors
    ///
    /// [`UnknownArgumentName`](RestageError::UnknownArgumentName) when no
    /// argument spec of the callable carries that name — asking for an argument
    /// the callable does not have is a mistake in calling code, not an absent
    /// argument — plus every error of
    /// [`restage_argument`](RestageContext::restage_argument).
    pub fn restage_argument_named<V>(
        &mut self,
        node: NodeRef<'_, L, A>,
        name: &str,
        visitor: &mut V,
    ) -> Result<RestagedArgument<L>, RestageError<V::Error>>
    where
        V: RestageVisitor<L, A, B> + ?Sized,
    {
        let data = callable_data(node)?;
        let index = data
            .arguments
            .iter()
            .position(|argument| argument.name() == Some(name))
            .ok_or_else(|| RestageError::UnknownArgumentName {
                node: node.id(),
                name: String::from(name),
            })?;
        self.restage_argument(node, index, visitor)
    }

    /// Restages slot `index` of callable `node` through the visitor, and returns
    /// the result as a [`RestagedSlot`] to hand to
    /// [`restage_invocation`](RestageContext::restage_invocation).
    ///
    /// The nodes of the slot's region go through the visitor; the slot's name,
    /// role, and ext are reproduced unchanged, and its content designation is
    /// reproduced against the staged nodes.
    ///
    /// # Errors
    ///
    /// [`NotACallable`](RestageError::NotACallable),
    /// [`SlotIndexOutOfRange`](RestageError::SlotIndexOutOfRange),
    /// [`ContentParentDropped`](RestageError::ContentParentDropped), plus anything
    /// the visitor run produces.
    pub fn restage_slot<V>(
        &mut self,
        node: NodeRef<'_, L, A>,
        index: usize,
        visitor: &mut V,
    ) -> Result<RestagedSlot<L>, RestageError<V::Error>>
    where
        V: RestageVisitor<L, A, B> + ?Sized,
    {
        let data = callable_data(node)?;
        let count = data.slots.len();
        let slot = data
            .slots
            .get(index)
            .ok_or(RestageError::SlotIndexOutOfRange { node: node.id(), index, count })?;
        let (name, role, ext) = (slot.name.clone(), slot.role, slot.ext.clone());
        let (nodes, content) = self.restage_region(node, &slot.region, visitor)?;
        Ok(RestagedSlot { name, role, nodes, content, ext })
    }

    /// Stages a copy of callable `node` whose arguments and slots are the given
    /// bundles, **in the order given**, and returns the staged node's id.
    ///
    /// The new node's children are the bundles' nodes, arguments first and then
    /// slots, and its argument and slot records are laid out over them
    /// accordingly. Everything else — invocation form, name, spec, the recorded
    /// invocation syntax, span, parsing state, and ext — is copied from `node`
    /// unchanged. `annotation` becomes the new node's annotation.
    ///
    /// The bundles define the new child list exhaustively: children of `node` that
    /// no bundle covers are not part of the replacement. Reordering the bundles
    /// reorders whole records, and each bundle keeps its own spec, so names and
    /// specs move together with their content — swapping the arguments of
    /// `\a{1}{2}` into `\a{2}{1}` is two
    /// [`restage_argument`](RestageContext::restage_argument) calls followed by one
    /// `restage_invocation` with the two bundles exchanged.
    ///
    /// # Errors
    ///
    /// [`NotACallable`](RestageError::NotACallable) if `node` is not a callable,
    /// and [`Build`](RestageError::Build) if the output builder rejects the
    /// result — for instance because a bundle's nodes were already used as
    /// another node's children, or because a content designation does not fit the
    /// nodes it points at.
    pub fn restage_invocation<E>(
        &mut self,
        node: NodeRef<'_, L, A>,
        arguments: Vec<RestagedArgument<L>>,
        slots: Vec<RestagedSlot<L>>,
        annotation: B,
    ) -> Result<BuildId, RestageError<E>> {
        let data = callable_data(node)?;
        let mut children: Vec<BuildId> = Vec::new();
        let mut offset = |added: &[BuildId]| -> Result<core::ops::Range<u32>, RestageError<E>> {
            let start = u32::try_from(children.len())
                .map_err(|_| RestageError::Build(NodeBuildError::TooManyNodes))?;
            let end = start
                .checked_add(
                    u32::try_from(added.len())
                        .map_err(|_| RestageError::Build(NodeBuildError::TooManyNodes))?,
                )
                .ok_or(RestageError::Build(NodeBuildError::TooManyNodes))?;
            children.extend_from_slice(added);
            Ok(start..end)
        };

        let mut parsed_arguments = Vec::with_capacity(arguments.len());
        for bundle in arguments {
            let RestagedArgument { spec, provided } = bundle;
            let (region, ext) = match provided {
                None => (None, None),
                Some(ProvidedRegion { nodes, content, ext }) => {
                    let range = offset(&nodes)?;
                    (Some(ChildRegion::new(range, content)), ext)
                }
            };
            parsed_arguments.push(ParsedArgument { spec, region, ext });
        }
        let mut parsed_slots = Vec::with_capacity(slots.len());
        for bundle in slots {
            let RestagedSlot { name, role, nodes, content, ext } = bundle;
            let range = offset(&nodes)?;
            parsed_slots.push(ParsedSlot {
                name,
                region: ChildRegion::new(range, content),
                role,
                ext,
            });
        }

        let kind = NodeKind::callable(CallableData {
            callable_type: data.callable_type,
            name: data.name.clone(),
            spec: Arc::clone(&data.spec),
            arguments: ParsedArguments::new(parsed_arguments),
            slots: ParsedSlots::new(parsed_slots),
            invocation_syntax: data.invocation_syntax.clone(),
        });
        self.builder
            .add(
                kind,
                node.span().clone(),
                node.parsing_state().clone(),
                children,
                node.ext().clone(),
                annotation,
            )
            .map_err(RestageError::Build)
    }

    // --- content-swap helpers -----------------------------------------------------------

    /// Restages argument `index` of callable `node` with its content replaced by
    /// the already-staged `content` nodes, and returns the resulting
    /// [`RestagedArgument`].
    ///
    /// The argument keeps its surroundings: the wrapper syntax and the
    /// non-content nodes of the region (whitespace and comments before or inside
    /// it) are copied unchanged and never go through a visitor, and the record's
    /// content designation is moved onto the position the new content occupies.
    /// The record's spec and ext are copied unchanged too.
    ///
    /// `annotation` is cloned onto every node copied unchanged, since those nodes
    /// never reach the visitor that would otherwise supply one. The `content`
    /// nodes were staged by the caller and keep the annotations they were staged
    /// with.
    ///
    /// To change the surroundings as well, use
    /// [`restage_argument`](RestageContext::restage_argument), where they do go
    /// through the visitor, or build the bundle yourself with
    /// [`RestagedArgument::provided`].
    ///
    /// This helper handles the usual wrapper shapes — groups, lists, and chars
    /// nodes on the path down to the content. A wrapper chain containing a
    /// *callable*, whose own records would have to be laid out again around the
    /// replacement, is outside its contract and is reported as a
    /// [`Build`](RestageError::Build) error; restage those through the visitor
    /// instead.
    ///
    /// # Errors
    ///
    /// [`NotACallable`](RestageError::NotACallable),
    /// [`ArgumentIndexOutOfRange`](RestageError::ArgumentIndexOutOfRange),
    /// [`ArgumentAbsent`](RestageError::ArgumentAbsent) when the argument was not
    /// provided and so has no wrapper to put content into, and
    /// [`Build`](RestageError::Build) when the output builder rejects the
    /// result.
    pub fn restage_argument_with_content<E>(
        &mut self,
        node: NodeRef<'_, L, A>,
        index: usize,
        content: Vec<BuildId>,
        annotation: B,
    ) -> Result<RestagedArgument<L>, RestageError<E>>
    where
        B: Clone,
    {
        let data = callable_data(node)?;
        let count = data.arguments.len();
        let argument = data
            .arguments
            .get(index)
            .ok_or(RestageError::ArgumentIndexOutOfRange { node: node.id(), index, count })?;
        let region = argument
            .region
            .as_ref()
            .ok_or(RestageError::ArgumentAbsent { node: node.id(), index })?;
        let spec = Arc::clone(&argument.spec);
        let ext = argument.ext.clone();
        let (nodes, content) =
            self.copy_region_with_swapped_content(node, region, content, &annotation)?;
        Ok(RestagedArgument { spec, provided: Some(ProvidedRegion { nodes, content, ext }) })
    }

    /// Restages slot `index` of callable `node` with its content replaced by the
    /// already-staged `content` nodes.
    ///
    /// This is
    /// [`restage_argument_with_content`](RestageContext::restage_argument_with_content)
    /// for a slot: wrapper and non-content nodes copied unchanged, content
    /// replaced, the content designation moved onto the new position, and the
    /// slot's name, role, and ext copied unchanged.
    ///
    /// # Errors
    ///
    /// [`NotACallable`](RestageError::NotACallable),
    /// [`SlotIndexOutOfRange`](RestageError::SlotIndexOutOfRange), and
    /// [`Build`](RestageError::Build) when the output builder rejects the
    /// result.
    pub fn restage_slot_with_content<E>(
        &mut self,
        node: NodeRef<'_, L, A>,
        index: usize,
        content: Vec<BuildId>,
        annotation: B,
    ) -> Result<RestagedSlot<L>, RestageError<E>>
    where
        B: Clone,
    {
        let data = callable_data(node)?;
        let count = data.slots.len();
        let slot = data
            .slots
            .get(index)
            .ok_or(RestageError::SlotIndexOutOfRange { node: node.id(), index, count })?;
        let (name, role, ext) = (slot.name.clone(), slot.role, slot.ext.clone());
        let (nodes, content) =
            self.copy_region_with_swapped_content(node, &slot.region, content, &annotation)?;
        Ok(RestagedSlot { name, role, nodes, content, ext })
    }

    /// The content-swap machinery: restage one resolved region with its
    /// designated content replaced by `new_content`, everything else copied
    /// verbatim (annotations cloned from `annotation`), returning the new
    /// region nodes and the re-anchored designation.
    fn copy_region_with_swapped_content<E>(
        &mut self,
        callable: NodeRef<'_, L, A>,
        region: &ChildRegion,
        new_content: Vec<BuildId>,
        annotation: &B,
    ) -> Result<(Vec<BuildId>, ContentNodes), RestageError<E>>
    where
        B: Clone,
    {
        let children = region.children();
        let content = region.content_range();
        let parent = region.content_parent();
        let tree = callable.tree();

        if parent == callable.id() {
            // Region-level content: splice the new content at the designated
            // position among the region's nodes, copying the rest verbatim.
            let cstart = (content.start - children.start) as usize;
            let cend = (content.end - children.start) as usize;
            let mut nodes: Vec<BuildId> = Vec::new();
            let mut new_range = 0u32..0u32;
            let splice =
                |nodes: &mut Vec<BuildId>| -> Result<core::ops::Range<u32>, RestageError<E>> {
                    let start = u32::try_from(nodes.len())
                        .map_err(|_| RestageError::Build(NodeBuildError::TooManyNodes))?;
                    nodes.extend_from_slice(&new_content);
                    let end = u32::try_from(nodes.len())
                        .map_err(|_| RestageError::Build(NodeBuildError::TooManyNodes))?;
                    Ok(start..end)
                };
            for (i, region_node) in tree.nodes_in(children.clone()).enumerate() {
                if i == cstart {
                    new_range = splice(&mut nodes)?;
                }
                if (cstart..cend).contains(&i) {
                    continue; // the old content, replaced
                }
                nodes.push(self.copy_verbatim(region_node, annotation)?);
            }
            if cstart == children.len() {
                // Empty content anchored at the region's end.
                new_range = splice(&mut nodes)?;
            }
            Ok((nodes, ContentNodes::InRegion(new_range)))
        } else {
            // Content inside a descendant: copy the region verbatim except
            // along the path to the content parent, where the parent's
            // designated children are swapped. Path from the parent up to (not
            // including) the callable, innermost first — the invariant that the
            // parent lies inside the region's subtree was validated by the
            // input tree's `finish()`.
            let mut path: Vec<NodeId> = alloc::vec![parent];
            let mut cursor = tree.node(parent);
            loop {
                let up = cursor
                    .parent()
                    .expect("resolved content parents lie inside their region's subtree");
                if up.id() == callable.id() {
                    break;
                }
                path.push(up.id());
                cursor = up;
            }
            let anchor = *path.last().expect("path holds at least the parent");

            let parent_node = tree.node(parent);
            let parent_base = parent_node.children().range().start;
            let rel = (content.start - parent_base) as usize..(content.end - parent_base) as usize;

            let mut nodes: Vec<BuildId> = Vec::new();
            let mut swapped: Option<(BuildId, core::ops::Range<u32>)> = None;
            for region_node in tree.nodes_in(children.clone()) {
                if region_node.id() == anchor {
                    let id = self.copy_swapping_at(
                        region_node,
                        &path,
                        rel.clone(),
                        &new_content,
                        annotation,
                        &mut swapped,
                    )?;
                    nodes.push(id);
                } else {
                    nodes.push(self.copy_verbatim(region_node, annotation)?);
                }
            }
            let (new_parent, new_range) =
                swapped.expect("the path's anchor lies among the region's nodes");
            Ok((nodes, ContentNodes::InChildrenOf(new_parent, new_range)))
        }
    }

    /// Copy `node` verbatim, descending along `path` (innermost first; `node`
    /// is its last element) and swapping the designated `rel` child range for
    /// `new_content` at the path's innermost node — reporting that node's new
    /// id and content range through `swapped`.
    fn copy_swapping_at<E>(
        &mut self,
        node: NodeRef<'_, L, A>,
        path: &[NodeId],
        rel: core::ops::Range<usize>,
        new_content: &[BuildId],
        annotation: &B,
        swapped: &mut Option<(BuildId, core::ops::Range<u32>)>,
    ) -> Result<BuildId, RestageError<E>>
    where
        B: Clone,
    {
        let at_target = node.id() == path[0];
        let mut children: Vec<BuildId> = Vec::new();
        let mut new_range = 0u32..0u32;
        let splice =
            |children: &mut Vec<BuildId>| -> Result<core::ops::Range<u32>, RestageError<E>> {
                let start = u32::try_from(children.len())
                    .map_err(|_| RestageError::Build(NodeBuildError::TooManyNodes))?;
                children.extend_from_slice(new_content);
                let end = u32::try_from(children.len())
                    .map_err(|_| RestageError::Build(NodeBuildError::TooManyNodes))?;
                Ok(start..end)
            };
        for (i, child) in node.children().iter().enumerate() {
            if at_target {
                if i == rel.start {
                    new_range = splice(&mut children)?;
                }
                if rel.contains(&i) {
                    continue; // the old content, replaced
                }
                children.push(self.copy_verbatim(child, annotation)?);
            } else if path[..path.len() - 1].last() == Some(&child.id()) {
                // The next node down the path.
                let id = self.copy_swapping_at(
                    child,
                    &path[..path.len() - 1],
                    rel.clone(),
                    new_content,
                    annotation,
                    swapped,
                )?;
                children.push(id);
            } else {
                children.push(self.copy_verbatim(child, annotation)?);
            }
        }
        if at_target && rel.start == node.child_count() {
            new_range = splice(&mut children)?;
        }

        // Stage the node itself over the reassembled children: kind, span,
        // state, and ext cloned verbatim. A callable here would carry resolved
        // records that cannot survive a child swap — the staged add rejects it
        // (the documented Build boundary of the _with_content helpers).
        let id = self
            .builder
            .add(
                node.kind().clone(),
                node.span().clone(),
                node.parsing_state().clone(),
                children,
                node.ext().clone(),
                annotation.clone(),
            )
            .map_err(RestageError::Build)?;
        if at_target {
            *swapped = Some((id, new_range));
        }
        Ok(id)
    }

    /// Deep-copy `node` verbatim (exts and records carried; annotations cloned
    /// from `annotation`).
    fn copy_verbatim<E>(
        &mut self,
        node: NodeRef<'_, L, A>,
        annotation: &B,
    ) -> Result<BuildId, RestageError<E>>
    where
        B: Clone,
    {
        crate::node::copy_subtree_into(&mut self.builder, node, &mut |_| annotation.clone())
            .map_err(RestageError::Build)
    }

    /// Drive the visitor over one resolved region's nodes and translate the
    /// region's content designation into coordinates relative to the bundle — the
    /// shared tail of the argument and slot operations.
    fn restage_region<V>(
        &mut self,
        callable: NodeRef<'_, L, A>,
        region: &ChildRegion,
        visitor: &mut V,
    ) -> Result<(Vec<BuildId>, ContentNodes), RestageError<V::Error>>
    where
        V: RestageVisitor<L, A, B> + ?Sized,
    {
        let children = region.children();
        let content = region.content_range();
        let parent = region.content_parent();

        // Drive each region node; prefix[i] = bundle offset where region node
        // i's replacement starts (the region-local coordinate translation).
        let mut nodes: Vec<BuildId> = Vec::new();
        let mut prefix: Vec<u32> = Vec::with_capacity(children.len() + 1);
        prefix.push(0);
        for region_node in callable.tree().nodes_in(children.clone()) {
            nodes.extend(drive(self, region_node, visitor)?);
            let offset = u32::try_from(nodes.len())
                .map_err(|_| RestageError::Build(NodeBuildError::TooManyNodes))?;
            prefix.push(offset);
        }

        let designation = if parent == callable.id() {
            // Region-level content: offsets relative to the region translate
            // through the region's own prefix sums (in bounds by the source
            // tree's record invariants).
            let start = (content.start - children.start) as usize;
            let end = (content.end - children.start) as usize;
            ContentNodes::InRegion(prefix[start]..prefix[end])
        } else {
            // Content inside a descendant: re-anchor through the replacement
            // map — the same policy as the driver's record translation
            // (translate through a driver-restaged parent, verbatim into a
            // single-node Emit takeover, refuse otherwise).
            let parent_node = callable.tree().node(parent);
            let parent_base = parent_node.children().range().start;
            let child_range =
                (content.start - parent_base) as usize..(content.end - parent_base) as usize;
            match self.replaced.get(&parent) {
                Some(Replaced::Restaged { id, prefix }) => ContentNodes::InChildrenOf(
                    *id,
                    prefix[child_range.start]..prefix[child_range.end],
                ),
                Some(Replaced::One(id)) => ContentNodes::InChildrenOf(
                    *id,
                    child_range.start as u32..child_range.end as u32,
                ),
                other => {
                    return Err(RestageError::ContentParentDropped {
                        callable: callable.id(),
                        parent,
                        replaced_by: match other {
                            Some(Replaced::Count(count)) => Some(*count),
                            _ => None,
                        },
                    })
                }
            }
        };
        Ok((nodes, designation))
    }

    /// Record the nodes an `Emit` staged in place of `old`.
    pub(super) fn record_emit(&mut self, old: NodeId, ids: &[BuildId]) {
        let entry = match ids {
            [one] => Replaced::One(*one),
            _ => Replaced::Count(ids.len()),
        };
        self.replaced.insert(old, entry);
    }

    /// Stage `node` over its children's replacements, answering the builder's
    /// content-parent question from the run's replacement map: a content range
    /// into a parent the driver restaged is translated through that parent's own
    /// replacements, a range into a single-node `Emit` replacement is reproduced
    /// unchanged. Records the result, and turns an unmapped content parent into
    /// the diagnosed
    /// [`ContentParentDropped`](RestageError::ContentParentDropped).
    pub(super) fn restage_over<AOld, E>(
        &mut self,
        node: NodeRef<'_, L, AOld>,
        replacements: &[Vec<BuildId>],
        annotation: B,
    ) -> Result<BuildId, RestageError<E>> {
        let replaced = &self.replaced;
        let result = self
            .builder
            .restage_node_with_content_mapping(
                node,
                replacements,
                |old| match replaced.get(&old) {
                    Some(Replaced::Restaged { id, prefix }) => {
                        Some(ContentParentMapping::Translate(*id, prefix))
                    }
                    Some(Replaced::One(id)) => Some(ContentParentMapping::Verbatim(*id)),
                    _ => None,
                },
                annotation,
            )
            .map_err(|error| match error {
                NodeBuildError::ContentParentUnmapped { parent } => {
                    RestageError::ContentParentDropped {
                        callable: node.id(),
                        parent,
                        replaced_by: match replaced.get(&parent) {
                            Some(Replaced::Restaged { .. }) | Some(Replaced::One(_)) => Some(1),
                            Some(Replaced::Count(count)) => Some(*count),
                            None => None,
                        },
                    }
                }
                other => RestageError::Build(other),
            })?;
        // Record the restage together with its replacement prefix sums — an
        // ancestor's record may designate content inside this node.
        let mut prefix: Vec<u32> = Vec::with_capacity(replacements.len() + 1);
        let mut total: u32 = 0;
        prefix.push(0);
        for entry in replacements {
            // In bounds: the staged add() above capped the total child count.
            total += entry.len() as u32;
            prefix.push(total);
        }
        self.replaced.insert(node.id(), Replaced::Restaged { id: result, prefix });
        Ok(result)
    }
}

/// The callable payload of `node`, or the op-misuse error.
fn callable_data<'n, L: Lang, A, E>(
    node: NodeRef<'n, L, A>,
) -> Result<&'n CallableData<L>, RestageError<E>> {
    node.callable().ok_or(RestageError::NotACallable { node: node.id() })
}

impl<L: Lang, A, B> core::fmt::Debug for RestageContext<'_, L, A, B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestageContext")
            .field("builder", &self.builder)
            .field("replaced", &self.replaced.len())
            .finish()
    }
}
