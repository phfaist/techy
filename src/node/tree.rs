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

/// Index of a node within its [`NodeTree`]. Minted by the tree's builder; only
/// meaningful for the tree that produced it (access goes through [`NodeRef`] proxies,
/// whose lifetime ties them to their tree).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub(crate) u32);

impl NodeId {
    /// The raw index of this id.
    pub const fn index(&self) -> usize {
        self.0 as usize
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
}

impl<L: Lang> NodeTree<L> {
    /// The root node (index 0; a tree always has at least one node).
    pub fn root(&self) -> NodeRef<'_, L> {
        self.node(NodeId(0))
    }

    /// The node with the given id.
    ///
    /// # Panics
    ///
    /// Panics if `id` does not belong to this tree (out of range) — ids are only
    /// meaningful for the tree that minted them.
    pub fn node(&self, id: NodeId) -> NodeRef<'_, L> {
        assert!(id.index() < self.nodes.len(), "node id {:?} out of range", id);
        NodeRef::new(self, id)
    }

    /// The number of nodes stored (at least 1 — the root).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// All nodes in storage order (root first; every node's children contiguous).
    pub fn iter(&self) -> impl Iterator<Item = NodeRef<'_, L>> {
        (0..self.nodes.len() as u32).map(move |i| NodeRef::new(self, NodeId(i)))
    }

    /// A new tree with every [`TextContent`](crate::source::TextContent) owned
    /// (node contents, callable post-spaces, and marker spellings). Trees stay
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
        NodeTree { nodes }
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
        NodeTree { nodes: self.nodes.clone() }
    }
}

impl<L: Lang> fmt::Debug for NodeTree<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeTree").field("nodes", &self.nodes).finish()
    }
}
