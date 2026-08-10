//! The restage driver: the [`TreeRestager`] entry point and the staging context
//! ([`RestageContext`]) handed to visitors.

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

/// The restage driver: holds the visitor and the run configuration, and
/// [`restage`](TreeRestager::restage)s a tree — see
/// [`RestageVisitor`](super::RestageVisitor) and the [module docs](super).
///
/// ```text
/// let output = TreeRestager::new(&mut visitor).restage(&tree)?;
/// let output = TreeRestager::new(&mut visitor)
///     .with_descent_guard_init(StdDescentGuardInit::depth_limit(64))
///     .restage(&tree)?;
/// ```
///
/// The visitor is held by `&mut` borrow: its run-spanning state stays
/// caller-owned and is read back after the run.
pub struct TreeRestager<'v, V: ?Sized> {
    visitor: &'v mut V,
    descent_guard_init: StdDescentGuardInit,
}

impl<'v, V: ?Sized> TreeRestager<'v, V> {
    /// A restager driving `visitor` — configure with the `with_*` methods, then
    /// run with [`restage`](TreeRestager::restage).
    pub fn new(visitor: &'v mut V) -> TreeRestager<'v, V> {
        TreeRestager { visitor, descent_guard_init: StdDescentGuardInit::default() }
    }

    /// Configure the run's descent guard
    /// ([`StdDescentGuardInit`](crate::core::StdDescentGuardInit): a stack
    /// budget, a depth limit, or off; a traversal costs one descent per tree
    /// nesting level — the re-entrant region ops included). Without this call
    /// the run uses the guard's default — a deliberately tight stack budget
    /// whose refusal names this method family. One caveat: a *single* op that
    /// copies a subtree verbatim (the `_with_content` helpers' wrapper copies)
    /// recurses over that subtree between two guard checks.
    pub fn with_descent_guard_init(mut self, init: StdDescentGuardInit) -> TreeRestager<'v, V> {
        self.descent_guard_init = init;
        self
    }

    /// Transform a tree by streaming restage: the visitor is invoked
    /// **top-down** over the frozen `tree` (root included), staging the output
    /// **bottom-up**; the finished tree is returned. See the
    /// [module docs](super) for the callback contract, the annotation pathway,
    /// and the edit policy.
    ///
    /// The root must restage to exactly one node —
    /// [`RootNotSingular`](RestageError::RootNotSingular) otherwise.
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

/// Restage one subtree through the visitor, guarded: ask the guard, ask the
/// verdict for `node`, then either recurse over all children and restage the
/// node over their results (`Descend`), or accept the callback's staged
/// replacement (`Emit`). Records the node's replacement in the context's map
/// either way. The guard's `exit` runs on the success and error paths alike;
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

/// How one input node was restaged (the content-parent translation oracle).
#[derive(Clone, Debug)]
enum Replaced {
    /// Driver-restaged over its children's replacements: the staged id plus the
    /// replacement-length prefix sums over the old children — the table that
    /// translates an [`InChildrenOf`](crate::core::node::ContentNodes::InChildrenOf)
    /// content range through the parent's own restaging.
    Restaged { id: BuildId, prefix: Vec<u32> },
    /// An `Emit` takeover with exactly one staged node: content ranges into it
    /// are carried verbatim (the visitor chose the replacement's shape) and
    /// re-validated at staging.
    One(BuildId),
    /// An `Emit` takeover with zero or several staged nodes (the count) — no
    /// shape an `InChildrenOf` designation could re-anchor onto.
    Count(usize),
}

/// The staging side of a [`TreeRestager`] run, handed to every visitor call: the
/// region-aware restaging ops and the raw output
/// [`builder()`](RestageContext::builder) underneath them.
///
/// The ops accept nodes from **any** tree (the [module docs](super)' cross-tree
/// contract), and driving the same node more than once is legal — each drive
/// stages a fresh copy (the internal replacement map keeps the latest, which is
/// what subsequent content-parent translations resolve against).
pub struct RestageContext<'t, L: Lang, A, B> {
    builder: NodeTreeBuilder<L, B>,
    /// Input-node id → its staged replacement, recorded for every driven node;
    /// the `content_parents` oracle for record translation (tree-tagged ids, so
    /// entries from several input trees never collide).
    replaced: HashMap<NodeId, Replaced>,
    /// The run's descent guard, consulted by every [`drive`] — the re-entrant
    /// region ops included, since they drive through this same context.
    descent_guard: StdDescentGuard,
    /// The run's frozen input tree, as a type/lifetime anchor: the context
    /// itself stores no borrow of it (ops take their input nodes explicitly and
    /// accept any tree's).
    _input: PhantomData<&'t NodeTree<L, A>>,
}

impl<'t, L: Lang, A, B> RestageContext<'t, L, A, B> {
    /// The raw staging builder of the output tree — arbitrary programmatic
    /// staging underneath the ready-made ops (they are conveniences, not the power
    /// boundary). Newly synthesized nodes are minted with the explicit two-line
    /// recipe: call
    /// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) (over
    /// [`staged_children`](NodeTreeBuilder::staged_children)), then
    /// [`add`](NodeTreeBuilder::add); restaged copies carry their old ext
    /// verbatim.
    pub fn builder(&mut self) -> &mut NodeTreeBuilder<L, B> {
        &mut self.builder
    }

    // --- region-aware restaging ops -----------------------------------------------------

    /// Run the full visitor over `node`'s subtree, the node itself included —
    /// exactly what the driver does for a `Descend`ed child. Returns the staged
    /// ids replacing `node` (one for a `Descend` of `node` itself; whatever the
    /// visitor emitted otherwise). The typical use is a takeover that still
    /// wants parts restaged through the pass:
    /// `Restage::Emit(cx.restage_subtree(other, self)?)`.
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

    /// Run the visitor over every child subtree of `node` (the node itself
    /// excluded), returning the concatenated replacements — the unwrap move:
    /// `Restage::Emit(cx.restage_children(group, self)?)` dissolves a wrapper
    /// while its content still flows through the pass.
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

    /// Restage argument `index` of callable `node` through the visitor,
    /// returning its [`RestagedArgument`] bundle: the region's nodes are driven
    /// like any `Descend`ed children, and the record's spec, presence, and
    /// content designation are carried over in bundle-relative staging
    /// coordinates. An absent argument yields
    /// [`RestagedArgument::absent`] (presence transfers — no error).
    ///
    /// Errors: [`NotACallable`](RestageError::NotACallable),
    /// [`ArgumentIndexOutOfRange`](RestageError::ArgumentIndexOutOfRange),
    /// [`ContentParentDropped`](RestageError::ContentParentDropped) (the
    /// visitor dropped/multiplied the node anchoring the record's content),
    /// plus anything the visitor run produces.
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

    /// [`restage_argument`](RestageContext::restage_argument) by the argument's
    /// spec name. A name matching none of the callable's argument specs is
    /// [`UnknownArgumentName`](RestageError::UnknownArgumentName) — asking for
    /// an argument the callable does not have is a caller bug, not an absence.
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

    /// Restage slot `index` of callable `node` through the visitor, returning
    /// its [`RestagedSlot`] bundle (name, role, and ext carried over; region
    /// nodes driven; content designation in bundle-relative coordinates).
    ///
    /// Errors: [`NotACallable`](RestageError::NotACallable),
    /// [`SlotIndexOutOfRange`](RestageError::SlotIndexOutOfRange),
    /// [`ContentParentDropped`](RestageError::ContentParentDropped), plus
    /// anything the visitor run produces.
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

    /// Restage callable `node`'s invocation over the given bundles, **in the
    /// order given**: the new node's children are the bundles' nodes
    /// (arguments first, then slots), its argument/slot records are retiled
    /// accordingly, and everything else — invocation form, name, spec,
    /// invocation-syntax payload, span, state, ext — is carried over from
    /// `node` verbatim. The annotation is the new node's (single-pathway rule).
    ///
    /// Reordering bundles reorders whole records: the argument swap
    /// `\a{1}{2}` → `\a{2}{1}` is two
    /// [`restage_argument`](RestageContext::restage_argument) calls and one
    /// reordered `restage_invocation` — each bundle keeps its own spec, so the
    /// swapped record's names/specs travel with their content. Children of
    /// `node` outside every bundle are simply not part of the replacement
    /// (bundles define the new child list exhaustively).
    ///
    /// Errors: [`NotACallable`](RestageError::NotACallable) and staging
    /// failures ([`Build`](RestageError::Build) — e.g. bundle nodes already
    /// claimed, or designations that do not fit).
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

    /// Restage argument `index` of callable `node` with its **content swapped**
    /// for the already-staged `content` nodes: the wrapper syntax and noise of
    /// the argument's region are restaged **verbatim by contract** — they never
    /// flow through a visitor — and the record's content designation is
    /// re-anchored onto the swapped position. The record's spec and ext carry
    /// over unchanged. `annotation` is cloned onto every verbatim-restaged
    /// wrapper/noise node (the annotation single-pathway rule needs an explicit
    /// channel here); the `content` nodes were staged by the caller and carry
    /// the annotations they were staged with.
    ///
    /// Changing the noise or wrapper too is not this helper's job: use
    /// [`restage_argument`](RestageContext::restage_argument) (noise flows
    /// through the visitor) or hand-build the bundle via
    /// [`RestagedArgument::provided`] (the general take-both form).
    ///
    /// The helper covers the standard wrapper shapes (groups/lists/chars on the
    /// path to the content). A wrapper chain that contains a *callable* — whose
    /// own records would need retiling around the swap — is outside its
    /// contract and surfaces as a [`Build`](RestageError::Build) error; take
    /// the visitor route for those.
    ///
    /// Errors: [`NotACallable`](RestageError::NotACallable),
    /// [`ArgumentIndexOutOfRange`](RestageError::ArgumentIndexOutOfRange),
    /// [`ArgumentAbsent`](RestageError::ArgumentAbsent) (an absent argument has
    /// no wrapper to put content into), and staging failures
    /// ([`Build`](RestageError::Build)).
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

    /// [`restage_argument_with_content`](RestageContext::restage_argument_with_content)
    /// for slot `index`: wrapper and noise verbatim, content swapped,
    /// designation re-anchored; the slot's name, role, and ext carry over.
    ///
    /// Errors: [`NotACallable`](RestageError::NotACallable),
    /// [`SlotIndexOutOfRange`](RestageError::SlotIndexOutOfRange), and staging
    /// failures ([`Build`](RestageError::Build)).
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
    /// region's content designation into bundle-relative staging coordinates —
    /// the shared tail of the argument/slot ops.
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

    /// Record an `Emit` takeover's replacement for `old`.
    pub(super) fn record_emit(&mut self, old: NodeId, ids: &[BuildId]) {
        let entry = match ids {
            [one] => Replaced::One(*one),
            _ => Replaced::Count(ids.len()),
        };
        self.replaced.insert(old, entry);
    }

    /// Stage `node` over its children's replacements (the level-0 restage
    /// arithmetic with the run's replacement map as the content-parent oracle:
    /// content ranges into driver-restaged parents are *translated* through the
    /// parent's own replacements, ranges into single-node `Emit` replacements
    /// carried verbatim), record the result, and upgrade an unmapped content
    /// parent into the diagnosed
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
