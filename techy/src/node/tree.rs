//! [`NodeTree`]: flat, frozen, index-based node storage over an `Arc`-shared core;
//! [`NodeId`]/[`TreeTag`]: tagged node identity; [`NodeData`]: one stored node.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::source::SourceSpan;
use crate::state::{Lang, ParsingState};

use super::kind::NodeKind;
use super::node_ref::NodeRef;
use super::NodeExt;

/// Wrapping counter minting tree-layout tags ([`TreeTag`]) in **all builds**.
/// `fetch_add` wraps on overflow, so tags may recur after 2^32 tree layouts per
/// process — accepted, because a tag is a *misuse detector*, never an addressing
/// mechanism (see [`TreeTag`]).
static NEXT_TREE_TAG: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Mint the tag for a new tree layout: a fresh counter value, in every build.
pub(crate) fn next_tree_tag() -> TreeTag {
    TreeTag(NEXT_TREE_TAG.fetch_add(1, core::sync::atomic::Ordering::Relaxed))
}

/// The tag of one tree *layout*, stamped into every [`NodeId`] the layout mints —
/// the tree-identity half of node identity (a newtype, so signatures cannot confuse
/// tags with node indices).
///
/// Layout-preserving copies — [`clone`](Clone::clone),
/// [`materialize`](NodeTree::materialize), [`annotate`](NodeTree::annotate) stages —
/// share their original's tag: their ids are genuinely interchangeable (ids identify
/// the *layout*, not the annotation stage).
///
/// A tag is a **misuse detector, never an addressing mechanism**: resolving an id
/// always goes through an explicit tree in hand ([`NodeTree::node`] /
/// [`NodeTree::get`]); the tag only makes cross-tree misuse detectable. Accordingly,
/// tags are minted by a process-global counter that *wraps* after 2^32 layouts — a
/// recurring tag can only matter where a stale-id bug already exists — and they are
/// **process-local**: never serialize a tag or treat it as wire material.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreeTag(u32);

impl fmt::Debug for TreeTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TreeTag({})", self.0)
    }
}

/// Identity of one node of a [`NodeTree`]: the node's index in the flat layout plus
/// the layout's [`TreeTag`] — 8 bytes, `Copy`.
///
/// The tag **participates in equality, ordering, and hashing**: ids minted by
/// different trees are different values, so one map can key ids from several trees,
/// and an old tree's `NodeId` stored inside a new tree's annotation is unambiguous.
/// Ids stay meaningful only for the tree that minted them (or a layout-preserving
/// copy — see [`TreeTag`]); resolve them through [`NodeTree::node`] (panicking,
/// own-tree ids) or [`NodeTree::get`] (`None` for foreign ids, every build).
///
/// Bare `Range<u32>` node-index ranges (child regions, [`NodeTree::nodes_in`])
/// carry no tag, as before.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId {
    pub(crate) index: u32,
    pub(crate) tree_tag: TreeTag,
}

impl NodeId {
    pub(crate) fn new(index: u32, tree_tag: TreeTag) -> NodeId {
        NodeId { index, tree_tag }
    }

    /// The raw index of this id within its tree's flat storage.
    pub const fn index(&self) -> usize {
        self.index as usize
    }

    /// The tag of the tree layout that minted this id (see [`TreeTag`]).
    pub const fn tree_tag(&self) -> TreeTag {
        self.tree_tag
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({}@{})", self.index, self.tree_tag.0)
    }
}

/// One stored node: structural kind, uniform ext, provenance span, parse-time state, and
/// the contiguous children block.
///
/// Fields are crate-private; the public read surface is [`NodeRef`]. Nodes carry **no
/// lifetime parameters** — Arc-wrapped spans, specs, and states make them self-contained,
/// which is what lets transformed trees outlive the parse they came from.
///
/// The runtime ownership graph stays acyclic by type structure (by design): nodes reference states, specs, and sources; no runtime value references
/// nodes back.
pub struct NodeData<L: Lang> {
    pub(crate) kind: NodeKind<L>,
    pub(crate) ext: NodeExt<L>,
    pub(crate) span: SourceSpan<L::SourceOrigin>,
    pub(crate) parsing_state: Arc<ParsingState<L>>,
    pub(crate) children: Range<u32>,
}

/// The frozen layout shared by every annotation stage of one tree: the node storage,
/// the parent table, the layout's [`TreeTag`], and the single-source fast-path flag —
/// everything except the per-stage annotations, behind one `Arc`
/// (see [`NodeTree`]'s layout notes).
pub(crate) struct TreeCore<L: Lang> {
    pub(crate) nodes: Vec<NodeData<L>>,
    /// Final-index parent per node; [`NO_PARENT`] at the root (index 0). Computed by
    /// `finish()` (which needs it for region resolution anyway) and kept for O(1)
    /// upward navigation ([`NodeRef::parent`]).
    pub(crate) parent: Vec<u32>,
    pub(crate) tree_tag: TreeTag,
    /// Whether every node's span lies in one and the same `Source` — the O(1) fast
    /// path for the whole-run single-source verification of
    /// [`NodeSlice::span`](super::NodeSlice::span)/
    /// [`source_text`](super::NodeSlice::source_text).
    pub(crate) single_source: bool,
}

/// Sentinel in [`TreeCore::parent`] marking the root (no parent). Safe: `finish()`
/// caps node counts below `u32::MAX`.
pub(crate) const NO_PARENT: u32 = u32::MAX;

/// A parsed document as flat, frozen, index-based node storage: nodes live in one `Vec`,
/// a node's children occupy a contiguous index block, and the root is a node like any
/// other (typically a `List`) at index 0.
///
/// Trees are **immutable**: mutation happens only inside the builder
/// ([`NodeTreeBuilder`](super::NodeTreeBuilder)), whose `finish()` consumes it.
/// Transformations build *new* trees — Arc-shared sources, specs, and states make
/// mixed-origin trees cheap.
///
/// # Annotations: the second generic parameter
///
/// `A` is the per-node **annotation** type — consumer-owned data, one value per node,
/// uniform across kinds, chosen per processing stage (the framework-side counterpart
/// of the lang-side [`NodeExt`]: a multi-stage pipeline types each stage's derived
/// data, `NodeTree<L>` → `NodeTree<L, SemInfo>` → …, instead of keeping
/// `HashMap<NodeId, T>` side tables). The parser emits `A = ()` — the default, so
/// plain `NodeTree<L>` spellings mean the unannotated tree — and `Lang` never sees
/// `A`.
///
/// Annotation types are expected to be `Clone + Debug + Send + Sync` and deliberately
/// never `Default` (every annotation value is supplied explicitly — by the builder's
/// [`add`](super::NodeTreeBuilder::add), or by an [`annotate`](NodeTree::annotate)
/// callback); the APIs state these bounds where they are used.
///
/// # Layout sharing
///
/// A `NodeTree` is a thin value: an `Arc`-shared frozen core (nodes, parent table,
/// [`TreeTag`]) plus this stage's annotation vector. [`annotate`](NodeTree::annotate)
/// therefore re-annotates **zero-copy** (no node is cloned; the input tree is
/// untouched), same-layout stages share the tag — their [`NodeId`]s are
/// interchangeable — and `clone()` costs O(annotations).
pub struct NodeTree<L: Lang, A = ()> {
    pub(crate) core: Arc<TreeCore<L>>,
    pub(crate) annotations: Vec<A>,
}

impl<L: Lang, A> NodeTree<L, A> {
    /// The stored nodes (in-crate: the flat-layout accessor behind every view type).
    #[inline]
    pub(crate) fn nodes(&self) -> &[NodeData<L>] {
        &self.core.nodes
    }

    /// The root node (index 0; a tree always has at least one node).
    pub fn root(&self) -> NodeRef<'_, L, A> {
        self.node(self.make_id(0))
    }

    /// The node with the given id. Use this for ids this tree minted; for ids of
    /// unknown provenance, use the non-panicking [`get`](NodeTree::get).
    ///
    /// # Panics
    ///
    /// Panics if `id` was not minted by this tree's layout (a foreign [`TreeTag`] —
    /// ids are only meaningful for the tree that minted them or a layout-preserving
    /// copy of it) or is out of range.
    pub fn node(&self, id: NodeId) -> NodeRef<'_, L, A> {
        // Panic here is an approved exception (indexing-style exception) per panic policy in DESIGN_RATIONALE.md .
        assert!(
            id.tree_tag == self.core.tree_tag,
            "node id {:?} used with a tree it does not belong to (this tree is {:?})",
            id,
            self.core.tree_tag
        );
        assert!(id.index() < self.core.nodes.len(), "node id {:?} out of range", id);
        NodeRef::new(self, id)
    }

    /// The node with the given id, or `None` if `id` does not belong to this tree —
    /// the non-panicking companion of [`node`](NodeTree::node), for ids of unknown
    /// provenance. An id minted by a different tree layout is rejected by its
    /// [`TreeTag`] in **every build** (never silently resolved to whatever node sits
    /// at that index here).
    pub fn get(&self, id: NodeId) -> Option<NodeRef<'_, L, A>> {
        if id.index() >= self.core.nodes.len() || id.tree_tag != self.core.tree_tag {
            return None;
        }
        Some(NodeRef::new(self, id))
    }

    /// This tree's layout tag.
    pub(crate) fn tree_tag(&self) -> TreeTag {
        self.core.tree_tag
    }

    /// Mint the id of index `index` of *this* tree.
    pub(crate) fn make_id(&self, index: u32) -> NodeId {
        NodeId::new(index, self.core.tree_tag)
    }

    /// Every node except the root, in **document order** — sugar for
    /// [`root().descendants()`](super::NodeRef::descendants) (preorder depth-first;
    /// contrast [`iter_storage_order`](NodeTree::iter_storage_order)).
    pub fn descendants(&self) -> super::Descendants<'_, L, A> {
        self.root().descendants()
    }

    /// The number of nodes stored (at least 1 — the root).
    pub fn node_count(&self) -> usize {
        self.core.nodes.len()
    }

    /// All nodes in **storage order** (breadth-first: root first, every node's children
    /// contiguous) — *not* document order: for `a{b}c` it yields `a`, `c`, `b`. Named
    /// for what it is so nobody
    /// mistakes it for a document-order walk; recurse via
    /// [`children()`](super::NodeRef::children) for structure-aware traversal.
    pub fn iter_storage_order(&self) -> impl Iterator<Item = NodeRef<'_, L, A>> {
        (0..self.core.nodes.len() as u32).map(move |i| NodeRef::new(self, self.make_id(i)))
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
    /// bare ranges carry no tree tag: applying an in-bounds range
    /// from another tree silently yields this tree's nodes at those indices.
    pub fn nodes_in(&self, range: Range<u32>) -> impl Iterator<Item = NodeRef<'_, L, A>> {
        assert!(
            range.start <= range.end && range.end as usize <= self.core.nodes.len(),
            "node range {:?} out of range",
            range
        );
        range.map(move |i| NodeRef::new(self, self.make_id(i)))
    }

    /// The annotations, one per node, indexed by [`NodeId::index`] — **storage order**
    /// (breadth-first, like [`iter_storage_order`](NodeTree::iter_storage_order));
    /// also the bulk-export shape for bindings. Per-node access is
    /// [`NodeRef::annotation`](super::NodeRef::annotation); there is no setter —
    /// trees are frozen, re-annotation is [`annotate`](NodeTree::annotate).
    pub fn annotations(&self) -> &[A] {
        &self.annotations
    }

    /// A tree with the **same layout** and new annotations: `f` is called once per
    /// node and its results become the new tree's annotation vector.
    ///
    /// Zero-copy over the layout: no node is cloned and `self` is untouched — the
    /// returned tree shares this tree's frozen core *and its [`TreeTag`]*, so
    /// [`NodeId`]s of the two stages are interchangeable (ids identify the layout,
    /// not the annotation stage). Only the new annotation vector is allocated.
    ///
    /// **The callback runs in storage order** (breadth-first — root first, then every
    /// node's children as contiguous blocks), **not document order**: a stateful
    /// closure must not assume it sees nodes in source order. Consumers that need a
    /// document-order preparation pass read [`descendants`](NodeTree::descendants)
    /// first. Each call receives the node as a [`NodeRef`], with the *current*
    /// annotation reachable via [`annotation()`](super::NodeRef::annotation).
    pub fn annotate<B>(&self, mut f: impl FnMut(NodeRef<'_, L, A>) -> B) -> NodeTree<L, B> {
        let annotations =
            (0..self.core.nodes.len() as u32).map(|i| f(self.node(self.make_id(i)))).collect();
        NodeTree { core: Arc::clone(&self.core), annotations }
    }

    /// A new tree with every [`TextContent`](crate::source::TextContent) owned
    /// (node contents, group delimiters, and callable post-spaces). Trees stay
    /// immutable — `self` is untouched; spans, states, and specs are Arc-shared, and
    /// the copy is **layout-preserving**: it keeps this tree's [`TreeTag`] (ids stay
    /// interchangeable) and clones the annotations.
    pub fn materialize(&self) -> NodeTree<L, A>
    where
        A: Clone,
    {
        let nodes = self
            .core
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
            core: Arc::new(TreeCore {
                nodes,
                parent: self.core.parent.clone(),
                tree_tag: self.core.tree_tag,
                single_source: self.core.single_source,
            }),
            annotations: self.annotations.clone(),
        }
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only associated
// types (already bounded) are stored; the annotation type's obligations surface as
// plain `A:` bounds.

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

/// O(annotations): the frozen core is `Arc`-shared (same layout, same [`TreeTag`]);
/// only the annotation vector is cloned.
impl<L: Lang, A: Clone> Clone for NodeTree<L, A> {
    fn clone(&self) -> Self {
        NodeTree { core: Arc::clone(&self.core), annotations: self.annotations.clone() }
    }
}

impl<L: Lang, A: fmt::Debug> fmt::Debug for NodeTree<L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeTree")
            .field("nodes", &self.core.nodes)
            .field("annotations", &self.annotations)
            .finish()
    }
}
