//! Node construction: [`NodeTreeBuilder`], which stages nodes and freezes them into a
//! finished [`NodeTree`].
//!
//! Nodes are staged bottom-up — a parent's child list is made of its children's
//! [`BuildId`]s, so the children go in first — and [`finish`](NodeTreeBuilder::finish)
//! turns everything reachable from a designated root into the flat tree.
//!
//! # Why staging, then flattening
//!
//! `NodeData.children: Range<u32>` requires sibling-contiguous storage, and pushing
//! nodes into the arena directly during recursive descent cannot provide that:
//! emission order is subtree-contiguous, not sibling-contiguous (`G(c1(d1,d2), c2(e1))`
//! emits `d1,d2,c1,e1,c2,G` post-order — `c1` and `c2` are not adjacent). The builder
//! therefore stages nodes with explicit child lists and lays the tree out breadth-first
//! in `finish()`: the root lands at index 0, and each node's children are appended as
//! one contiguous block. O(n), one transient copy.
//!
//! # Region resolution (the two-phase record contract)
//!
//! For the same reason, `finish()` also resolves each callable's staged argument and
//! slot regions ([`ChildRegion`](super::ChildRegion)) from staging coordinates (child
//! offsets plus [`ContentNodes`](super::ContentNodes) designations in `BuildId` terms)
//! into global node-index ranges, which name positions in the flattened layout — a
//! layout that exists only here.
//!
//! That is what lets parsers build `ParsedArguments`/`ParsedSlots` directly and stage
//! them through the one `add()`. The price is that a record's phase is a runtime
//! invariant rather than a type-level one; it stays contained because resolution
//! happens at exactly this one point, so a finished tree never holds staged regions.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::source::{SourceSpan, Span, TextContent};
use crate::state::{Lang, ParsingState};

use super::arguments::ContentNodes;
use super::kind::{CallableData, NodeKind};
use super::tree::{NodeData, NodeTree, TreeCore, TreeTag, NO_PARENT};
use super::NodeExt;

/// The id of a node staged in a [`NodeTreeBuilder`], returned by
/// [`add`](NodeTreeBuilder::add).
///
/// Build ids number nodes in staging order and mean nothing outside the builder that
/// minted them. They are deliberately a different type from [`NodeId`](super::NodeId),
/// which numbers the nodes of a finished tree in the breadth-first layout
/// [`finish`](NodeTreeBuilder::finish) produces.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BuildId(u32);

impl fmt::Debug for BuildId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BuildId({})", self.0)
    }
}

struct Staged<L: Lang> {
    kind: NodeKind<L>,
    ext: NodeExt<L>,
    span: SourceSpan<L::SourceOrigin>,
    parsing_state: Arc<ParsingState<L>>,
    children: Vec<BuildId>,
    /// Whether some other staged node already lists this one as a child (each node has
    /// at most one parent).
    claimed: bool,
}

/// Builds a [`NodeTree`]: stage every node with [`add`](NodeTreeBuilder::add), then
/// freeze the result with [`finish`](NodeTreeBuilder::finish).
///
/// Trees are immutable, so this builder is the only place nodes are assembled. During a
/// parse the [`ParserSession`](crate::core::ParserSession) drives it, and a construct
/// parser reaches it through
/// [`ParseContext::stage_node`](crate::core::constructs::ParseContext::stage_node)
/// rather than directly — see the guide chapter on
/// [custom construct parsers](crate::guide::construct_parsers). Transforms, extraction
/// helpers, and tests use it directly.
///
/// # Order of construction
///
/// Nodes are staged bottom-up. `add` takes the [`BuildId`]s of the node's children, so
/// every child must already be staged when its parent is added; `add` returns the new
/// node's own `BuildId`. `finish` then takes the id of the node that is to become the
/// root, and returns the tree.
///
/// Staged nodes not reachable from that root are dropped without complaint: a parser
/// recovering from malformed input may abandon nodes it staged speculatively.
///
/// The finished tree's node order is not the staging order — `finish` lays the nodes
/// out breadth-first, and that is also where each callable's argument and slot regions
/// are resolved into node-index ranges.
///
/// # What the builder does not do
///
/// It runs no hook and has no modes; `add` demands ready values, namely the
/// already-minted [`NodeExt`] and the node's annotation. Parse staging mints the ext
/// automatically inside
/// [`ParseContext::stage_node`](crate::core::constructs::ParseContext::stage_node). A
/// transform author mints it explicitly — call
/// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) with the view
/// [`staged_children`](NodeTreeBuilder::staged_children) returns, then `add` — or
/// supplies a value of their own. [`restage_node`](NodeTreeBuilder::restage_node) is
/// not a second staging path: it clones an existing node's data and calls `add`.
///
/// # Contract
///
/// Staged input comes from argument and construct parser implementations, whose bugs
/// must surface as errors rather than panics, so every rule below is checked on every
/// call, in every build, and a violation is returned as a [`NodeBuildError`]:
///
/// - A child `BuildId` must already be staged in this builder (which also makes cycles
///   unrepresentable), each staged node may be used as a child at most once, and the
///   root must not be anyone's child.
/// - A `Callable`'s [`ParsedArguments`](super::ParsedArguments) and
///   [`ParsedSlots`](super::ParsedSlots) regions must still be *staged* — records read
///   back from a finished tree may not be reused, because their ranges mean something
///   only for the layout that minted them — and they must tile the child list exactly,
///   in order: one region per provided argument, then one per slot, with no gap and no
///   child left over.
/// - A content designation must fit the child list it points into, and a content parent
///   must lie inside its own region's subtree (checked in
///   [`finish`](NodeTreeBuilder::finish), where the layout exists).
/// - A `Spanned` text payload must be a valid range of the node's own source content,
///   on `char` boundaries.
///
/// A builder whose `add` returned an `Err` is **poisoned**: children the failed call
/// already claimed stay claimed, so the build must be abandoned. The error reports an
/// implementation bug to fix, not a condition to recover from.
pub struct NodeTreeBuilder<L: Lang, A = ()> {
    staged: Vec<Staged<L>>,
    /// The staged nodes' annotations, parallel to `staged` (kept out of `Staged` so
    /// the staged read views stay annotation-free — `Lang` never sees `A`).
    annotations: Vec<A>,
}

impl<L: Lang, A> NodeTreeBuilder<L, A> {
    /// An empty builder.
    pub fn new() -> NodeTreeBuilder<L, A> {
        NodeTreeBuilder { staged: Vec::new(), annotations: Vec::new() }
    }
}

impl<L: Lang, A> NodeTreeBuilder<L, A> {
    /// Stages one node and returns its [`BuildId`].
    ///
    /// This is the builder's only staging method. The nodes named in `children` must
    /// already be staged, and none of them may already be another node's child; the
    /// node staged here can in turn serve as a child of a later `add`, or as the root
    /// of [`finish`](NodeTreeBuilder::finish).
    ///
    /// The parameters are positional, in the fixed order *identity → provenance →
    /// context → structure → language data → consumer data*:
    ///
    /// 1. `kind` — what the node structurally is;
    /// 2. `span` — the source range it was parsed (or synthesized) from;
    /// 3. `parsing_state` — the state it was parsed under;
    /// 4. `children` — its structural children in order. For a `Callable` this is the
    ///    concatenation of one child region per provided argument, then one per slot;
    ///    the [`ParsedArguments`](super::ParsedArguments) and
    ///    [`ParsedSlots`](super::ParsedSlots) records index this list and must tile it
    ///    exactly;
    /// 5. `ext` — the **already-minted** [`NodeExt`]; the builder never mints one
    ///    itself (parse staging mints through
    ///    [`ParseContext::stage_node`](crate::core::constructs::ParseContext::stage_node),
    ///    transforms call
    ///    [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) explicitly or pass
    ///    a value of their own, and restaged copies keep their old ext unchanged);
    /// 6. `annotation` — the node's consumer-side annotation (`()` on parse paths).
    ///
    /// # Errors
    ///
    /// Returns a [`NodeBuildError`] when the input violates the staging contract
    /// documented on [`NodeTreeBuilder`]: an unstaged or already-claimed child, a
    /// callable whose argument and slot regions do not tile its child list, a text
    /// payload outside the node's own source, and so on. The builder is poisoned once
    /// that happens — abandon it rather than staging further nodes.
    pub fn add(
        &mut self,
        kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
        parsing_state: Arc<ParsingState<L>>,
        children: Vec<BuildId>,
        ext: NodeExt<L>,
        annotation: A,
    ) -> Result<BuildId, NodeBuildError> {
        if self.staged.len() >= u32::MAX as usize {
            return Err(NodeBuildError::TooManyNodes);
        }
        for child in &children {
            let staged = self
                .staged
                .get_mut(child.0 as usize)
                .ok_or(NodeBuildError::ChildNotStaged { child: *child })?;
            if staged.claimed {
                return Err(NodeBuildError::ChildAlreadyClaimed { child: *child });
            }
            staged.claimed = true;
        }
        if let NodeKind::Callable(data) = &kind {
            let n_children = children.len() as u32;
            let mut next: u32 = 0;
            let regions = data
                .arguments
                .iter()
                .filter_map(|arg| arg.region.as_ref())
                .chain(data.slots.iter().map(|slot| &slot.region));
            for region in regions {
                let Some((child_range, content)) = region.staged() else {
                    return Err(NodeBuildError::RegionAlreadyResolved);
                };
                if child_range.start > child_range.end || child_range.end > n_children {
                    return Err(NodeBuildError::RegionOutOfBounds {
                        region: child_range.clone(),
                        n_children,
                    });
                }
                if child_range.start != next {
                    return Err(NodeBuildError::RegionNotTiling {
                        region: child_range.clone(),
                        expected_start: next,
                    });
                }
                next = child_range.end;
                match content {
                    ContentNodes::InRegion(r) => {
                        let region_len = child_range.end - child_range.start;
                        if r.start > r.end || r.end > region_len {
                            return Err(NodeBuildError::ContentOutOfBounds {
                                content: r.clone(),
                                available: region_len,
                            });
                        }
                    }
                    ContentNodes::InChildrenOf(b, r) => {
                        let content_parent = self
                            .staged
                            .get(b.0 as usize)
                            .ok_or(NodeBuildError::ContentParentNotStaged { parent: *b })?;
                        if r.start > r.end || r.end as usize > content_parent.children.len() {
                            return Err(NodeBuildError::ContentOutOfBounds {
                                content: r.clone(),
                                available: content_parent.children.len() as u32,
                            });
                        }
                    }
                }
            }
            if next != n_children {
                return Err(NodeBuildError::ChildrenNotInRegions { unassigned: next..n_children });
            }
        }
        check_spanned_contents(&kind, &span)?;

        let id = BuildId(self.staged.len() as u32);
        self.staged.push(Staged { kind, ext, span, parsing_state, children, claimed: false });
        self.annotations.push(annotation);
        Ok(id)
    }

    /// A read-only view of the nodes staged so far, keyed by [`BuildId`].
    ///
    /// Node-based stop predicates read this view; during a parse it is reached through
    /// [`ParseContext::staged_nodes`](crate::core::constructs::ParseContext::staged_nodes).
    pub fn staged_nodes(&self) -> StagedNodes<'_, L> {
        StagedNodes { staged: &self.staged }
    }

    /// The descent-only view of `children` — the shape
    /// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) receives.
    ///
    /// A transform that mints an ext itself builds the view here and passes it to the
    /// hook, then stages the node:
    ///
    /// ```ignore
    /// let ext = L::make_node_ext(&kind, &span, &state, builder.staged_children(&children))?;
    /// let id = builder.add(kind, span, state, children, ext, annotation)?;
    /// ```
    pub fn staged_children<'b>(
        &'b self,
        children: &'b [BuildId],
    ) -> StagedChildren<'b, L> {
        StagedChildren { arena: &self.staged, children }
    }

    /// Freezes everything reachable from `root` into a flat [`NodeTree`], consuming
    /// the builder.
    ///
    /// The nodes are laid out breadth-first — the root at index 0, each node's children
    /// as one contiguous block — and every callable's staged argument and slot regions
    /// are resolved into global node-index ranges of that layout (the two-phase record
    /// contract on [`ChildRegion`](super::ChildRegion)). Staged nodes not reachable
    /// from `root` are dropped.
    ///
    /// # Errors
    ///
    /// Returns a [`NodeBuildError`] for the parts of the builder's contract that can
    /// only be checked once the layout exists: `root` must itself be staged
    /// ([`RootNotStaged`](NodeBuildError::RootNotStaged)) and must not be another
    /// node's child ([`RootClaimed`](NodeBuildError::RootClaimed)), and every content
    /// parent must be reachable from `root` and lie inside its own argument or slot
    /// region's subtree.
    pub fn finish(self, root: BuildId) -> Result<NodeTree<L, A>, NodeBuildError> {
        const NONE: u32 = NO_PARENT; // safe sentinel: add() caps staging below u32::MAX
        let tree_tag = super::tree::next_tree_tag();
        match self.staged.get(root.0 as usize) {
            None => return Err(NodeBuildError::RootNotStaged { root }),
            Some(entry) if entry.claimed => return Err(NodeBuildError::RootClaimed { root }),
            Some(_) => {}
        }
        let mut staged: Vec<Option<Staged<L>>> = self.staged.into_iter().map(Some).collect();
        let mut staged_annotations: Vec<Option<A>> =
            self.annotations.into_iter().map(Some).collect();

        // Pass 1: breadth-first order, per-node children ranges, parent links, and the
        // staged-id → final-index map (the layout tables region resolution needs).
        // Child ids were checked staged-and-claimed-once in add(), so the traversal
        // visits each staged node at most once.
        let mut order: Vec<u32> = Vec::with_capacity(staged.len());
        let mut ranges: Vec<Range<u32>> = Vec::with_capacity(staged.len());
        let mut parent: Vec<u32> = Vec::with_capacity(staged.len()); // final index; NONE at the root
        let mut final_of: Vec<u32> = alloc::vec![NONE; staged.len()]; // staged id → final index
        order.push(root.0);
        parent.push(NONE);
        final_of[root.0 as usize] = 0;
        let mut pos = 0;
        while pos < order.len() {
            let sid = order[pos] as usize;
            let start = order.len() as u32;
            for child in &staged[sid].as_ref().unwrap().children {
                final_of[child.0 as usize] = order.len() as u32;
                parent.push(pos as u32);
                order.push(child.0);
            }
            let end = order.len() as u32;
            ranges.push(start..end);
            pos += 1;
        }

        // Pass 2: move the staged data (and its annotation) into place, resolving
        // callable region records (only possible here, where the flattened layout
        // exists).
        let mut nodes = Vec::with_capacity(order.len());
        let mut annotations = Vec::with_capacity(order.len());
        for (f, &sid) in order.iter().enumerate() {
            let children = ranges[f].clone();
            let mut staged = staged[sid as usize].take().expect("staged node used twice");
            annotations
                .push(staged_annotations[sid as usize].take().expect("staged node used twice"));
            if let NodeKind::Callable(data) = &mut staged.kind {
                resolve_regions(data, f as u32, &children, &ranges, &parent, &final_of, tree_tag)?;
            }
            nodes.push(NodeData {
                kind: staged.kind,
                ext: staged.ext,
                span: staged.span,
                parsing_state: staged.parsing_state,
                children,
            });
        }
        // The single-source fast-path flag: whether every node's span lies in one and
        // the same `Source` (the O(1) short-circuit for whole-run slice verification).
        let single_source = match nodes.split_first() {
            Some((first, rest)) => {
                let source = first.span.source();
                rest.iter().all(|data| Arc::ptr_eq(data.span.source(), source))
            }
            None => true,
        };
        Ok(NodeTree {
            core: Arc::new(TreeCore { nodes, parent, tree_tag, single_source }),
            annotations,
        })
    }
}

/// Resolve a callable's staged argument/slot regions into global node-index ranges.
/// `callable` is the callable's final index and `callable_children` its final children
/// block; `ranges`/`parent`/`final_of` are the pass-1 layout tables. Bounds were checked
/// at `add()`; what is only checkable here is reachability and that each content parent
/// lies inside its own region's subtree.
fn resolve_regions<L: Lang>(
    data: &mut CallableData<L>,
    callable: u32,
    callable_children: &Range<u32>,
    ranges: &[Range<u32>],
    parent: &[u32],
    final_of: &[u32],
    tree_tag: TreeTag,
) -> Result<(), NodeBuildError> {
    const NONE: u32 = NO_PARENT;
    let regions = data
        .arguments
        .arguments
        .iter_mut()
        .filter_map(|arg| arg.region.as_mut())
        .chain(data.slots.slots.iter_mut().map(|slot| &mut slot.region));
    for region in regions {
        let (child_range, content) = {
            let (c, k) = region.staged().expect("staged regions were checked in add()");
            (c.clone(), k.clone())
        };
        let children =
            callable_children.start + child_range.start..callable_children.start + child_range.end;
        let (content_range, content_parent) = match content {
            ContentNodes::InRegion(r) => {
                (children.start + r.start..children.start + r.end, callable)
            }
            ContentNodes::InChildrenOf(b, r) => {
                let bf = final_of[b.0 as usize];
                if bf == NONE {
                    return Err(NodeBuildError::ContentParentUnreachable { parent: b });
                }
                // The content parent must be one of this region's nodes or a descendant
                // of one — walk up to its child-of-the-callable ancestor.
                let mut a = bf;
                loop {
                    let p = parent[a as usize];
                    if p == NONE {
                        return Err(NodeBuildError::ContentParentOutsideSubtree { parent: b });
                    }
                    if p == callable {
                        break;
                    }
                    a = p;
                }
                if !children.contains(&a) {
                    return Err(NodeBuildError::ContentParentOutsideRegion { parent: b });
                }
                let block = &ranges[bf as usize];
                (block.start + r.start..block.start + r.end, bf)
            }
        };
        region.resolve(children, content_range, content_parent, tree_tag);
    }
    Ok(())
}

/// A read-only view of every node staged in a [`NodeTreeBuilder`] so far, keyed by
/// [`BuildId`].
///
/// Node-based stop predicates read their "already staged" context through it. Obtain
/// one from [`NodeTreeBuilder::staged_nodes`] or, inside a parse, from
/// [`ParseContext::staged_nodes`](crate::core::constructs::ParseContext::staged_nodes);
/// it borrows the builder and cannot mutate it.
///
/// Ext minting receives the narrower, descent-only [`StagedChildren`] view instead.
pub struct StagedNodes<'b, L: Lang> {
    staged: &'b [Staged<L>],
}

impl<'b, L: Lang> StagedNodes<'b, L> {
    /// The view of staged node `id`, if `id` has been staged in this builder.
    pub fn get(&self, id: BuildId) -> Option<StagedNodeView<'b, L>> {
        self.staged.get(id.0 as usize).map(|staged| StagedNodeView { id, staged })
    }

    /// The number of nodes staged so far.
    pub fn len(&self) -> usize {
        self.staged.len()
    }

    /// Whether nothing has been staged yet.
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty()
    }
}

/// One staged node of a [`NodeTreeBuilder`], viewed read-only.
///
/// Obtained from [`StagedNodes::get`]. Its accessors return data borrowed from the
/// builder rather than from this transient proxy, mirroring
/// [`NodeRef`](super::NodeRef) over finished trees.
pub struct StagedNodeView<'b, L: Lang> {
    id: BuildId,
    staged: &'b Staged<L>,
}

impl<'b, L: Lang> StagedNodeView<'b, L> {
    /// This node's staging id.
    pub fn id(&self) -> BuildId {
        self.id
    }

    /// The structural kind.
    pub fn kind(&self) -> &'b NodeKind<L> {
        &self.staged.kind
    }

    /// The language's extension data for this node.
    pub fn ext(&self) -> &'b NodeExt<L> {
        &self.staged.ext
    }

    /// The node's provenance span.
    pub fn span(&self) -> &'b SourceSpan<L::SourceOrigin> {
        &self.staged.span
    }

    /// The parsing state the node was staged under.
    pub fn parsing_state(&self) -> &'b Arc<ParsingState<L>> {
        &self.staged.parsing_state
    }

    /// The node's structural children, as staging ids in order.
    pub fn children(&self) -> &'b [BuildId] {
        &self.staged.children
    }
}

/// A subtree-deep view of one staged node's children — the `children` parameter of
/// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext), built by
/// [`NodeTreeBuilder::staged_children`].
///
/// The view descends and only descends: it exposes exactly the children it was built
/// from, and each child view resolves *its* own children in turn
/// ([`StagedChildView::children`]), so argument content at grandchild depth is
/// reachable. Siblings, ancestors, and unrelated staged nodes are not, and there is no
/// lookup by [`BuildId`] — the wider view is [`StagedNodes`].
///
/// A child id that was never staged in this builder reads as absent rather than
/// failing: [`get`](StagedChildren::get) answers `None` and
/// [`iter`](StagedChildren::iter) skips it, and neither panics. The subsequent
/// [`add`](NodeTreeBuilder::add) reports the same caller mistake as
/// [`ChildNotStaged`](NodeBuildError::ChildNotStaged).
///
/// The view borrows the builder's staging storage, which the very next staging call
/// grows, so nothing reached through it — child views and everything they return — may
/// be kept past the call that received the view; copy out what is needed. Safe Rust
/// cannot break this rule, since the lifetimes forbid it; it is stated for embeddings
/// that adapt the receiving hook across a boundary where lifetimes are erased.
pub struct StagedChildren<'b, L: Lang> {
    arena: &'b [Staged<L>],
    children: &'b [BuildId],
}

impl<'b, L: Lang> StagedChildren<'b, L> {
    /// The number of children in the view.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Whether the view holds no children.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// The view of the `i`-th child. `None` for an out-of-range index — or for a
    /// child id that was never staged (see the type docs).
    pub fn get(&self, i: usize) -> Option<StagedChildView<'b, L>> {
        let id = *self.children.get(i)?;
        let staged = self.arena.get(id.0 as usize)?;
        Some(StagedChildView { arena: self.arena, staged })
    }

    /// The child views, in order (skipping never-staged ids — see the type docs).
    pub fn iter(&self) -> impl Iterator<Item = StagedChildView<'b, L>> + use<'b, L> {
        let arena = self.arena;
        self.children
            .iter()
            .filter_map(move |id| arena.get(id.0 as usize))
            .map(move |staged| StagedChildView { arena, staged })
    }
}

/// One staged child, viewed read-only through [`StagedChildren`].
///
/// Its accessors return data borrowed from the builder rather than from this transient
/// proxy, and [`children`](StagedChildView::children) descends to that child's own
/// children — and only descends.
pub struct StagedChildView<'b, L: Lang> {
    arena: &'b [Staged<L>],
    staged: &'b Staged<L>,
}

impl<'b, L: Lang> StagedChildView<'b, L> {
    /// The structural kind.
    pub fn kind(&self) -> &'b NodeKind<L> {
        &self.staged.kind
    }

    /// The already-minted uniform ext.
    pub fn ext(&self) -> &'b NodeExt<L> {
        &self.staged.ext
    }

    /// The node's provenance span.
    pub fn span(&self) -> &'b SourceSpan<L::SourceOrigin> {
        &self.staged.span
    }

    /// The parsing state the node was staged under.
    pub fn parsing_state(&self) -> &'b Arc<ParsingState<L>> {
        &self.staged.parsing_state
    }

    /// This child's own children — the recursive descent (grandchildren and deeper
    /// are reachable; nothing else is).
    pub fn children(&self) -> StagedChildren<'b, L> {
        StagedChildren { arena: self.arena, children: &self.staged.children }
    }
}

// Manual impls: the views are Copy/Clone/Debug regardless of `L` (only borrows stored).

impl<L: Lang> Clone for StagedNodes<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for StagedNodes<'_, L> {}

impl<L: Lang> fmt::Debug for StagedNodes<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagedNodes").field("len", &self.staged.len()).finish()
    }
}

impl<L: Lang> Clone for StagedNodeView<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for StagedNodeView<'_, L> {}

impl<L: Lang> fmt::Debug for StagedNodeView<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagedNodeView")
            .field("id", &self.id)
            .field("kind", &self.staged.kind)
            .field("span", &self.staged.span)
            .field("children", &self.staged.children)
            .finish()
    }
}

impl<L: Lang> Clone for StagedChildren<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for StagedChildren<'_, L> {}

impl<L: Lang> fmt::Debug for StagedChildren<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagedChildren").field("children", &self.children).finish()
    }
}

impl<L: Lang> Clone for StagedChildView<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for StagedChildView<'_, L> {}

impl<L: Lang> fmt::Debug for StagedChildView<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagedChildView")
            .field("kind", &self.staged.kind)
            .field("span", &self.staged.span)
            .field("children", &self.staged.children)
            .finish()
    }
}

impl<L: Lang, A> Default for NodeTreeBuilder<L, A> {
    fn default() -> Self {
        NodeTreeBuilder::new()
    }
}

impl<L: Lang, A> fmt::Debug for NodeTreeBuilder<L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeTreeBuilder").field("staged", &self.staged.len()).finish()
    }
}

/// Check the `TextContent` invariant: every `Spanned` value of this node refers into the
/// node's own source, in bounds and on `char` boundaries.
fn check_spanned_contents<L: Lang>(
    kind: &NodeKind<L>,
    span: &SourceSpan<L::SourceOrigin>,
) -> Result<(), NodeBuildError> {
    let content = span.source().content();
    let check = |text: &TextContent, what: &'static str| match text {
        TextContent::Spanned(s) if s.get(content).is_none() => {
            Err(NodeBuildError::SpannedContentInvalid {
                what,
                span: *s,
                content_len: content.len(),
            })
        }
        _ => Ok(()),
    };
    match kind {
        NodeKind::Chars { content: text, .. } => check(text, "chars content"),
        NodeKind::Comment(data) => {
            check(&data.content, "comment content")?;
            check(&data.start, "comment start delimiter")?;
            check(&data.post_space, "comment post_space")
        }
        // A Callable's invocation-syntax payload is Lang-opaque here: span-backed
        // fields inside it are the Lang's own recording discipline (checked by the
        // in-crate span-tiling law oracle for the shipped payloads, not builder law).
        NodeKind::Callable(_) => Ok(()),
        NodeKind::Group(data) => {
            check(&data.open, "group open delimiter")?;
            check(&data.close, "group close delimiter")
        }
        NodeKind::List => Ok(()),
    }
}

/// The error of [`NodeTreeBuilder`]: staged input broke the builder's contract.
///
/// It reports an implementation bug in the code doing the staging — an argument or
/// construct parser, or a transform — not a condition in the parsed document. A parse
/// lifts it into a `ParseError` that aborts even under tolerant recovery, and the
/// builder that returned it is poisoned, so the build must be abandoned. The contract
/// itself, and the one exception to the poisoning rule, are documented on
/// [`NodeTreeBuilder`] and below.
///
/// One variant reports a failure rather than a violated contract:
/// [`ExtMintFailed`](NodeBuildError::ExtMintFailed) is the error channel of
/// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext). Minting is part of
/// staging, and the hook also runs for consumer-built trees, where no parse-side error
/// type exists. That variant comes out of the mint call itself and never out of
/// [`add`](NodeTreeBuilder::add), so no children have been claimed and the builder
/// stays usable. Inside a parse it aborts like every other value of this type, but it
/// is raised as a [`HookFailed`](crate::error::HookFailed) condition — an operational
/// failure in consumer-supplied hook code — rather than an implementation error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeBuildError {
    /// Staging would exceed the `u32` id space.
    TooManyNodes,
    /// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) reported a failure
    /// while minting a node's ext. `detail` is the implementation's own description of
    /// what went wrong; an implementation with an underlying error chain renders it
    /// into that string, which keeps this variant plain data so the enum can derive
    /// `PartialEq`/`Eq`.
    ExtMintFailed {
        /// The mint's description of the failure.
        detail: String,
    },
    /// A child id was never staged in this builder.
    ChildNotStaged {
        /// The offending child id.
        child: BuildId,
    },
    /// A child is already another staged node's child.
    ChildAlreadyClaimed {
        /// The offending child id.
        child: BuildId,
    },
    /// A callable was staged with an already-resolved argument/slot region: parsed
    /// argument/slot records must be built fresh for the tree being staged.
    RegionAlreadyResolved,
    /// An argument/slot region lies outside the callable's child list.
    RegionOutOfBounds {
        /// The offending region's child-offset range.
        region: Range<u32>,
        /// The callable's child count.
        n_children: u32,
    },
    /// An argument/slot region does not start where the previous one ended: the regions
    /// must tile the child list exactly, in order (arguments, then slots).
    RegionNotTiling {
        /// The offending region's child-offset range.
        region: Range<u32>,
        /// Where the region was expected to start.
        expected_start: u32,
    },
    /// Children after the last argument/slot region belong to no region (the regions
    /// must tile the child list exactly).
    ChildrenNotInRegions {
        /// The child offsets no region accounts for.
        unassigned: Range<u32>,
    },
    /// A region's content range lies outside its containing child list.
    ContentOutOfBounds {
        /// The offending content range.
        content: Range<u32>,
        /// The containing child list's length.
        available: u32,
    },
    /// A `ContentNodes::InChildrenOf` parent id was never staged in this builder.
    ContentParentNotStaged {
        /// The offending content-parent id.
        parent: BuildId,
    },
    /// A `Spanned` text payload is not a valid range of the node's own source.
    SpannedContentInvalid {
        /// Which payload (e.g. `"chars content"`, `"group open delimiter"`).
        what: &'static str,
        /// The offending span.
        span: Span,
        /// The node's source content length.
        content_len: usize,
    },
    /// [`finish`](NodeTreeBuilder::finish)'s root was never staged in this builder.
    RootNotStaged {
        /// The offending root id.
        root: BuildId,
    },
    /// [`finish`](NodeTreeBuilder::finish)'s root is another staged node's child.
    RootClaimed {
        /// The offending root id.
        root: BuildId,
    },
    /// A content parent is not reachable from the root passed to
    /// [`finish`](NodeTreeBuilder::finish).
    ContentParentUnreachable {
        /// The offending content-parent id.
        parent: BuildId,
    },
    /// A content parent is not inside its callable's subtree.
    ContentParentOutsideSubtree {
        /// The offending content-parent id.
        parent: BuildId,
    },
    /// A content parent lies outside its own argument/slot region's subtree.
    ContentParentOutsideRegion {
        /// The offending content-parent id.
        parent: BuildId,
    },
    /// [`restage_node`](NodeTreeBuilder::restage_node)'s replacement list does not
    /// have exactly one entry per child of the input node.
    ReplacementsLengthMismatch {
        /// The input node's child count.
        children: usize,
        /// The number of replacement entries supplied.
        replacements: usize,
    },
    /// [`restage_node`](NodeTreeBuilder::restage_node)'s content-parent mapping
    /// answered `None` for a content parent the input node's argument/slot records
    /// designate.
    ContentParentUnmapped {
        /// The old tree's id of the unmapped content parent.
        parent: super::NodeId,
    },
}

impl fmt::Display for NodeBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeBuildError::TooManyNodes => write!(f, "node tree too large"),
            NodeBuildError::ExtMintFailed { detail } => {
                write!(f, "the node-ext mint (Lang::make_node_ext) reported a failure: {}", detail)
            }
            NodeBuildError::ChildNotStaged { child } => {
                write!(f, "child {:?} has not been staged", child)
            }
            NodeBuildError::ChildAlreadyClaimed { child } => {
                write!(f, "child {:?} already has a parent", child)
            }
            NodeBuildError::RegionAlreadyResolved => write!(
                f,
                "callable staged with an already-resolved region: parsed argument/slot \
                 records must be built fresh for the tree being staged"
            ),
            NodeBuildError::RegionOutOfBounds { region, n_children } => write!(
                f,
                "argument/slot region {:?} out of bounds ({} children)",
                region, n_children
            ),
            NodeBuildError::RegionNotTiling { region, expected_start } => write!(
                f,
                "argument/slot regions must tile the child list exactly, in order \
                 (region {:?} where offset {} was expected)",
                region, expected_start
            ),
            NodeBuildError::ChildrenNotInRegions { unassigned } => write!(
                f,
                "argument/slot regions must tile the child list exactly \
                 (children {:?} belong to no region)",
                unassigned
            ),
            NodeBuildError::ContentOutOfBounds { content, available } => write!(
                f,
                "content range {:?} out of bounds ({} nodes available)",
                content, available
            ),
            NodeBuildError::ContentParentNotStaged { parent } => {
                write!(f, "content parent {:?} has not been staged", parent)
            }
            NodeBuildError::SpannedContentInvalid { what, span, content_len } => write!(
                f,
                "{} span {:?} is not a valid range of the node's source (len {})",
                what, span, content_len
            ),
            NodeBuildError::RootNotStaged { root } => {
                write!(f, "root {:?} has not been staged", root)
            }
            NodeBuildError::RootClaimed { root } => {
                write!(f, "root {:?} is another node's child", root)
            }
            NodeBuildError::ContentParentUnreachable { parent } => {
                write!(f, "content parent {:?} is not reachable from the root", parent)
            }
            NodeBuildError::ContentParentOutsideSubtree { parent } => {
                write!(f, "content parent {:?} is not inside the callable's subtree", parent)
            }
            NodeBuildError::ContentParentOutsideRegion { parent } => write!(
                f,
                "content parent {:?} lies outside its own argument/slot region",
                parent
            ),
            NodeBuildError::ReplacementsLengthMismatch { children, replacements } => write!(
                f,
                "restage_node needs exactly one replacement entry per child \
                 ({} children, {} entries)",
                children, replacements
            ),
            NodeBuildError::ContentParentUnmapped { parent } => write!(
                f,
                "restage_node's content-parent mapping has no staged counterpart \
                 for content parent {:?}",
                parent
            ),
        }
    }
}

impl core::error::Error for NodeBuildError {}
