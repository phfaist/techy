//! [`NodeTreeBuilder`]: the staging builder that produces frozen [`NodeTree`]s.
//!
//! # Why staging, then flattening
//!
//! `NodeData.children: Range<u32>` requires **sibling-contiguous** storage. Pushing nodes
//! into the arena directly during recursive descent cannot provide that: emission orders
//! are subtree-contiguous, not sibling-contiguous (`G(c1(d1,d2), c2(e1))` emits
//! `d1,d2,c1,e1,c2,G` post-order — `c1` and `c2` are not adjacent). The builder therefore
//! stages nodes with explicit child lists and lays the tree out breadth-first in
//! [`finish`](NodeTreeBuilder::finish): the root lands at index 0, and each node's
//! children are appended as one contiguous block. O(n), one transient copy.
//!
//! # Region resolution (the two-phase record contract)
//!
//! For the same reason, `finish()` also **resolves** each callable's staged
//! argument/slot regions ([`ChildRegion`](super::ChildRegion)) from staging coordinates
//! (child offsets + [`ContentNodes`](super::ContentNodes) designations in `BuildId`
//! terms) into global node-index ranges: those ranges name positions in the flattened
//! layout, which exists only here. This is the accepted "honest cost" of letting
//! parsers build `ParsedArguments`/`ParsedSlots` directly with an unchanged `add()`
//! — the record's phase is a runtime invariant, contained by
//! resolving at exactly this one point, so a finished tree never holds staged regions.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::source::{SourceSpan, Span, TextContent};
use crate::state::{Lang, ParsingState};

use super::arguments::ContentNodes;
use super::kind::{CallableData, NodeKind};
use super::tree::{NodeData, NodeTree};
use super::NodeExt;

/// Id of a staged node within its builder. Deliberately distinct from
/// [`NodeId`](super::NodeId): staging order is construction order, while final ids
/// reflect the flattened breadth-first layout.
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

/// Builds a [`NodeTree`] bottom-up: children are staged before their parent (their
/// `BuildId`s go into the parent's child list), and [`finish`](NodeTreeBuilder::finish)
/// freezes everything reachable from the designated root into flat storage.
///
/// This is the mutation boundary of the node system: trees are immutable, and this
/// builder — driven by `ParserSession`, tests, and future transforms — is the
/// only place nodes are assembled.
///
/// # Contract (validated at the boundary — violations return [`NodeBuildError`])
///
/// The staging input comes from argument/construct parser implementations and
/// [`Lang::finalize_node`] hooks — outer layers whose bugs must surface as errors, not
/// panics (the panic policy). Every check runs in every build:
///
/// - A child `BuildId` must already be staged (which also makes cycles unrepresentable).
/// - Each staged node is used as a child at most once, and the root must not be anyone's
///   child.
/// - A `Callable` kind's `ParsedArguments`/`ParsedSlots` regions must be *staged* (never
///   reuse records from a finished tree — ranges are only meaningful for the layout that
///   minted them) and must **tile** the child list exactly, in order (argument regions,
///   then slot regions, no gaps — every child accounted for: the partition
///   invariant); content designations must fit their parent's child list, and a content
///   parent must lie inside its own region's subtree (checked in
///   [`finish`](NodeTreeBuilder::finish), where the layout exists).
/// - The `TextContent` invariant: `Spanned` ranges must lie inside the node's own source
///   content, on `char` boundaries.
///
/// A builder whose `add` returned an `Err` is **poisoned**: children claimed by the
/// failed call may stay claimed, so the build must be abandoned — the error reports an
/// implementation bug to fix, not a condition to recover from.
///
/// Staged nodes unreachable from the root are silently dropped: parsers may abandon
/// speculatively built nodes (tolerant-parsing recovery paths).
pub struct NodeTreeBuilder<L: Lang> {
    staged: Vec<Staged<L>>,
}

impl<L: Lang> NodeTreeBuilder<L> {
    /// An empty builder.
    pub fn new() -> NodeTreeBuilder<L> {
        NodeTreeBuilder { staged: Vec::new() }
    }

    /// Stage a node with the default uniform ext. `children` are the node's structural
    /// children in order (for a `Callable`: the concatenation of one child region per
    /// provided argument, then one per slot — the `ParsedArguments`/`ParsedSlots`
    /// regions index this list).
    pub fn add(
        &mut self,
        kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
        parsing_state: Arc<ParsingState<L>>,
        children: Vec<BuildId>,
    ) -> Result<BuildId, NodeBuildError> {
        self.add_with_ext(kind, span, parsing_state, children, Default::default())
    }

    /// Stage a node with an explicit uniform ext (tier 1).
    ///
    /// Runs [`Lang::finalize_node`] on the node's parts first — the builder is the single
    /// mutation boundary, so hooking here guarantees no node escapes finalization
    /// — then the staging checks (hook mutations are validated
    /// too). `Err` means the input violated the staging contract (see the type docs; the
    /// builder is poisoned then).
    pub fn add_with_ext(
        &mut self,
        mut kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
        parsing_state: Arc<ParsingState<L>>,
        children: Vec<BuildId>,
        mut ext: NodeExt<L>,
    ) -> Result<BuildId, NodeBuildError> {
        L::finalize_node(&mut kind, &mut ext, &span, &parsing_state, &children, &self.staged_nodes());
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
        Ok(id)
    }

    /// A read-only view of the nodes staged so far (what [`Lang::finalize_node`] and
    /// node-based stop predicates consume).
    pub fn staged_nodes(&self) -> StagedNodes<'_, L> {
        StagedNodes { staged: &self.staged }
    }

    /// Freeze everything reachable from `root` into a flat [`NodeTree`] (breadth-first:
    /// root at index 0, each node's children as one contiguous block), **resolving**
    /// every callable's staged argument/slot regions into global node-index ranges (see
    /// the module docs). Staged nodes not reachable from `root` are dropped. `Err`
    /// means the staged input violated the contract's layout-time obligations (root
    /// staged and unclaimed; content parents inside their region's subtree).
    pub fn finish(self, root: BuildId) -> Result<NodeTree<L>, NodeBuildError> {
        const NONE: u32 = u32::MAX; // safe sentinel: add() caps staging below u32::MAX
        let tree_tag = super::tree::next_tree_tag();
        match self.staged.get(root.0 as usize) {
            None => return Err(NodeBuildError::RootNotStaged { root }),
            Some(entry) if entry.claimed => return Err(NodeBuildError::RootClaimed { root }),
            Some(_) => {}
        }
        let mut staged: Vec<Option<Staged<L>>> = self.staged.into_iter().map(Some).collect();

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

        // Pass 2: move the staged data into place, resolving callable region records
        // (only possible here, where the flattened layout exists).
        let mut nodes = Vec::with_capacity(order.len());
        for (f, &sid) in order.iter().enumerate() {
            let children = ranges[f].clone();
            let mut staged = staged[sid as usize].take().expect("staged node used twice");
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
        Ok(NodeTree {
            nodes,
            #[cfg(debug_assertions)]
            tag: tree_tag,
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
    tree_tag: u32,
) -> Result<(), NodeBuildError> {
    const NONE: u32 = u32::MAX;
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

/// A read-only view over a [`NodeTreeBuilder`]'s staged nodes, keyed by [`BuildId`] —
/// the "already staged" context handed to [`Lang::finalize_node`] (so a callable's hook
/// can inspect its children, e.g. to extract environment scaffolding sub-spans) and to
/// node-based stop predicates.
///
/// Obtained from [`NodeTreeBuilder::staged_nodes`]; borrows the builder, no mutation.
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

/// One staged node, viewed read-only (see [`StagedNodes`]). Accessors return
/// `'b`-borrowed data (borrowing the builder, not this transient proxy), mirroring
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

    /// The uniform (tier-1) ext data.
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

impl<L: Lang> Default for NodeTreeBuilder<L> {
    fn default() -> Self {
        NodeTreeBuilder::new()
    }
}

impl<L: Lang> fmt::Debug for NodeTreeBuilder<L> {
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
        NodeKind::Comment { content, start, post_space, .. } => {
            check(content, "comment content")?;
            check(start, "comment start delimiter")?;
            check(post_space, "comment post_space")
        }
        NodeKind::Callable(data) => check(&data.post_space, "callable post_space"),
        NodeKind::Group(data) => {
            check(&data.open, "group open delimiter")?;
            check(&data.close, "group close delimiter")
        }
        NodeKind::List { .. } => Ok(()),
    }
}

/// Contract-violation error of [`NodeTreeBuilder`]: the staged input — produced by an
/// argument/construct parser implementation or mutated by a [`Lang::finalize_node`]
/// hook — broke the builder's documented contract (see [`NodeTreeBuilder`]'s type docs).
///
/// This reports an **implementation bug** in an extension, not a source-input
/// condition: parse layers lift it into a `ParseError` that aborts even under tolerant
/// recovery, and a builder that returned one is poisoned (the build must be abandoned).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeBuildError {
    /// Staging would exceed the `u32` id space.
    TooManyNodes,
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
}

impl fmt::Display for NodeBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeBuildError::TooManyNodes => write!(f, "node tree too large"),
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
        }
    }
}

impl core::error::Error for NodeBuildError {}
