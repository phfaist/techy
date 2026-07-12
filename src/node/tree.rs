//! [`NodeTree`]: flat, frozen, index-based node storage; [`NodeData`]: one stored node.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::source::SourceSpan;
use crate::state::{Lang, ParsingState};

use super::kind::NodeKind;
use super::node_ref::NodeRef;
use super::NodeExt;

/// Wrapping counter minting debug-build tree provenance tags (see [`NodeTree`]'s `tag`
/// field). `fetch_add` wraps on overflow, so tags may recur after 2^32 tree layouts —
/// acceptable for a debug-only heuristic.
#[cfg(debug_assertions)]
static NEXT_TREE_TAG: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Mint a provenance tag for a new tree layout: a fresh counter value in debug builds,
/// `0` in release builds (where tags are not stored).
pub(crate) fn next_tree_tag() -> u32 {
    #[cfg(debug_assertions)]
    return NEXT_TREE_TAG.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    #[cfg(not(debug_assertions))]
    return 0;
}

/// Index of a node within its [`NodeTree`]. Minted by the tree's builder; only
/// meaningful for the tree that produced it (access goes through [`NodeRef`] proxies,
/// whose lifetime ties them to their tree). Debug builds additionally stamp each id
/// with its tree's provenance tag, so cross-tree misuse trips a debug assertion.
#[derive(Clone, Copy)]
pub struct NodeId(
    pub(crate) u32,
    /// Debug-only provenance tag of the tree layout that minted this id. Excluded from
    /// comparisons and hashing so debug and release builds agree on id semantics.
    #[cfg(debug_assertions)] pub(crate) u32,
);

impl NodeId {
    pub(crate) fn new(index: u32, tree_tag: u32) -> NodeId {
        let _ = tree_tag;
        #[cfg(debug_assertions)]
        return NodeId(index, tree_tag);
        #[cfg(not(debug_assertions))]
        return NodeId(index);
    }

    /// The raw index of this id.
    pub const fn index(&self) -> usize {
        self.0 as usize
    }

    /// The provenance tag stamped at mint time (`0` in release builds).
    pub(crate) fn tree_tag(&self) -> u32 {
        #[cfg(debug_assertions)]
        return self.1;
        #[cfg(not(debug_assertions))]
        return 0;
    }
}

// Identity is the index alone (the debug-only tag is provenance metadata, checked by
// explicit debug assertions — folding it into `Eq`/`Ord`/`Hash` would make debug and
// release builds disagree).

impl PartialEq for NodeId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for NodeId {}

impl PartialOrd for NodeId {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NodeId {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl core::hash::Hash for NodeId {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", self.0)
    }
}

/// One stored node: structural kind, uniform ext, provenance span, parse-time state, and
/// the contiguous children block (ARCHITECTURE.md §nodes).
///
/// Fields are crate-private; the public read surface is [`NodeRef`]. Nodes carry **no
/// lifetime parameters** — Arc-wrapped spans, specs, and states make them self-contained,
/// which is what lets transformed trees outlive the parse they came from.
///
/// The runtime ownership graph stays acyclic by type structure (ARCHITECTURE.md §3
/// rule 3): nodes reference states, specs, and sources; no runtime value references
/// nodes back.
pub struct NodeData<L: Lang> {
    pub(crate) kind: NodeKind<L>,
    pub(crate) ext: NodeExt<L>,
    pub(crate) span: SourceSpan<L::SourceOrigin>,
    pub(crate) parsing_state: Arc<ParsingState<L>>,
    pub(crate) children: Range<u32>,
}

/// A parsed document as flat, frozen, index-based node storage: nodes live in one `Vec`,
/// a node's children occupy a contiguous index block, and the root is a node like any
/// other (typically a `List`) at index 0.
///
/// Trees are **immutable**: mutation happens only inside the builder
/// ([`NodeTreeBuilder`](super::NodeTreeBuilder)), whose `finish()` consumes it.
/// Transformations build *new* trees — Arc-shared sources, specs, and states make
/// mixed-origin trees cheap.
pub struct NodeTree<L: Lang> {
    pub(crate) nodes: Vec<NodeData<L>>,
    /// Debug-only provenance tag ([`next_tree_tag`]) stamped into every [`NodeId`] this
    /// tree mints, so cross-tree id misuse trips a debug assertion in [`NodeRef::new`].
    /// Layout-preserving copies ([`clone`](Clone::clone), [`materialize`](NodeTree::materialize))
    /// share the tag — their ids are genuinely interchangeable.
    #[cfg(debug_assertions)]
    pub(crate) tag: u32,
}

impl<L: Lang> NodeTree<L> {
    /// The root node (index 0; a tree always has at least one node).
    pub fn root(&self) -> NodeRef<'_, L> {
        self.node(self.make_id(0))
    }

    /// The node with the given id.
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of range. Ids are only meaningful for the tree that minted
    /// them (or a layout-preserving copy of it): an *in-range* id from a different tree
    /// silently resolves to whatever node sits at that index here — release builds
    /// cannot detect it; debug builds catch it with a provenance-tag assertion.
    pub fn node(&self, id: NodeId) -> NodeRef<'_, L> {
        assert!(id.index() < self.nodes.len(), "node id {:?} out of range", id);
        NodeRef::new(self, id)
    }

    /// This tree's provenance tag (`0` in release builds).
    pub(crate) fn tree_tag(&self) -> u32 {
        #[cfg(debug_assertions)]
        return self.tag;
        #[cfg(not(debug_assertions))]
        return 0;
    }

    /// Mint the id of index `index` of *this* tree (debug builds stamp the tree's tag).
    pub(crate) fn make_id(&self, index: u32) -> NodeId {
        NodeId::new(index, self.tree_tag())
    }

    /// The number of nodes stored (at least 1 — the root).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// All nodes in storage order (root first; every node's children contiguous).
    pub fn iter(&self) -> impl Iterator<Item = NodeRef<'_, L>> {
        (0..self.nodes.len() as u32).map(move |i| NodeRef::new(self, self.make_id(i)))
    }

    /// The nodes of a global node-index range — a resolved
    /// [`ChildRegion`](super::ChildRegion)'s [`children()`](super::ChildRegion::children)
    /// or [`content_range()`](super::ChildRegion::content_range) of *this* tree — in
    /// source order.
    ///
    /// # Panics
    ///
    /// Panics if the range does not lie within this tree's storage — like [`NodeId`]s,
    /// ranges are only meaningful for the tree whose builder minted them. Unlike ids,
    /// bare ranges carry no provenance even in debug builds: applying an in-bounds range
    /// from another tree silently yields this tree's nodes at those indices.
    pub fn nodes_in(&self, range: Range<u32>) -> impl Iterator<Item = NodeRef<'_, L>> {
        assert!(
            range.start <= range.end && range.end as usize <= self.nodes.len(),
            "node range {:?} out of range",
            range
        );
        range.map(move |i| NodeRef::new(self, self.make_id(i)))
    }

    /// A new tree with every [`TextContent`](crate::source::TextContent) owned
    /// (node contents, group delimiters, and callable post-spaces). Trees stay
    /// immutable — `self` is untouched; spans, states, and specs are Arc-shared.
    pub fn materialize(&self) -> NodeTree<L> {
        let nodes = self
            .nodes
            .iter()
            .map(|data| {
                let source_content = data.span.source().content();
                NodeData {
                    kind: data.kind.materialized(source_content),
                    ext: data.ext.clone(),
                    span: data.span.clone(),
                    parsing_state: Arc::clone(&data.parsing_state),
                    children: data.children.clone(),
                }
            })
            .collect();
        NodeTree {
            nodes,
            #[cfg(debug_assertions)]
            tag: self.tag,
        }
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only associated
// types (already bounded) are stored.

impl<L: Lang> Clone for NodeData<L> {
    fn clone(&self) -> Self {
        NodeData {
            kind: self.kind.clone(),
            ext: self.ext.clone(),
            span: self.span.clone(),
            parsing_state: Arc::clone(&self.parsing_state),
            children: self.children.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for NodeData<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The parse-time state is deliberately omitted: it is context, not identity, and
        // printing a full ParsingState per node would drown the tree.
        f.debug_struct("NodeData")
            .field("kind", &self.kind)
            .field("ext", &self.ext)
            .field("span", &self.span)
            .field("children", &self.children)
            .finish_non_exhaustive()
    }
}

impl<L: Lang> Clone for NodeTree<L> {
    fn clone(&self) -> Self {
        NodeTree {
            nodes: self.nodes.clone(),
            #[cfg(debug_assertions)]
            tag: self.tag,
        }
    }
}

impl<L: Lang> fmt::Debug for NodeTree<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeTree").field("nodes", &self.nodes).finish()
    }
}
