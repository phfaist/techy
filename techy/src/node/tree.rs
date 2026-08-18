//! [`NodeTree`]: flat, frozen, index-based node storage over an `Arc`-shared core;
//! [`NodeId`]/[`TreeTag`]: tagged node identity; [`NodeData`]: one stored node.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::source::{Source, SourcePos, SourceSpan};
use crate::state::{Lang, ParsingState};

use super::kind::NodeKind;
use super::node_ref::NodeRef;
use super::slice::NodeSlice;
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

    /// The stored parent table, indexed like [`nodes`](NodeTree::nodes) (in-crate:
    /// [`NO_PARENT`] marks the root; behind [`NodeRef::parent`]).
    #[inline]
    pub(crate) fn parent_table(&self) -> &[u32] {
        &self.core.parent
    }

    /// Whether every node's span lies in one and the same `Source` — the flag
    /// `finish()` computes (in-crate: the O(1) fast path for the whole-run
    /// single-source verification of [`NodeSlice::span`](super::NodeSlice::span)/
    /// [`source_text`](super::NodeSlice::source_text)).
    #[inline]
    pub(crate) fn is_single_source(&self) -> bool {
        self.core.single_source
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

    /// This tree's layout tag ([`TreeTag`]). Comparing it with a
    /// [`NodeId::tree_tag`] pre-checks whether an id belongs to this tree before
    /// calling an accessor that asserts ownership in every build
    /// ([`node`](NodeTree::node)).
    pub fn tree_tag(&self) -> TreeTag {
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

    /// The **deepest** node whose span contains the position — half-open
    /// containment (`start ≤ pos < end`; a node with an empty span never matches) —
    /// or `None` when no node's span contains it.
    ///
    /// A position inside a node's span but inside none of its children — a group's
    /// delimiter bytes, a callable's trigger spelling — resolves to that node.
    /// Ancestors of the answer come free via [`NodeRef::parent`].
    ///
    /// Multi-source trees are answered **per source**, on exact per-node spans only:
    /// the lookup enters a *different-source* child only while still searching for
    /// the query's source, never from a node that already matched. A query in the
    /// including source therefore stops at the `\input`-like node itself (its
    /// resolved content lives in its own source), while a query in an attached
    /// source finds the attached content. Where a child's recorded span does not
    /// nest in its parent's — restaged trees with rearranged spans, and parses of a
    /// language with
    /// [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`, whose
    /// node spans are the ones the token reader described — this degrades to the
    /// shallowest node whose recorded span answers.
    pub fn node_at(&self, pos: &SourcePos<L::SourceOrigin>) -> Option<NodeRef<'_, L, A>> {
        deepest_containing(self.root(), pos.source(), &|span: &SourceSpan<
            L::SourceOrigin,
        >| span.span().contains(pos.pos()))
    }

    /// The **minimal covering sibling run** for a span query: the shortest run of
    /// siblings, within the deepest node list that can answer, covering every byte
    /// of `span` — as the name says, the run may cover *more* than the query (a
    /// query cutting into the middle of a node returns the whole node).
    ///
    /// The descent mirrors [`node_at`](NodeTree::node_at) (per source, exact spans
    /// only, half-open containment): it walks to the deepest node whose span
    /// contains the whole query, then answers the minimal run of that node's
    /// children covering the query. When the children cannot cover it — query bytes
    /// in the node's own delimiters or trigger spelling, or children whose spans do
    /// not tile the queried stretch (restaged trees, and parses of a language with
    /// [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`) — the
    /// covering node itself is the answer,
    /// as a single-node run within its parent's child list. An **empty** query span
    /// resolves by point containment like `node_at`, to a single-node run of the
    /// deepest containing node. `None` when no node's span contains the query.
    ///
    /// A run already addressed by a node-index range (rather than a span query) is
    /// built with [`slice`](NodeTree::slice) instead.
    pub fn covering_slice(
        &self,
        span: &SourceSpan<L::SourceOrigin>,
    ) -> Option<NodeSlice<'_, L, A>> {
        let node = deepest_containing(self.root(), span.source(), &|node_span: &SourceSpan<
            L::SourceOrigin,
        >| {
            if span.is_empty() {
                node_span.span().contains(span.start())
            } else {
                node_span.start() <= span.start() && span.end() <= node_span.end()
            }
        })?;
        if let Some(run) = covering_child_run(node, span) {
            return Some(NodeSlice::new(self, run));
        }
        let index = node.id().index() as u32;
        Some(NodeSlice::new(self, index..index + 1))
    }

    /// The [`NodeSlice`] over a node-index range of this tree's flat storage —
    /// the validated constructor for ranges kept as bare numbers (an exported
    /// `Range<u32>` from a [`ChildRegion`](super::ChildRegion), a range computed by
    /// binding or persistence code). The span-addressed companion — the run
    /// answering a source-span query — is
    /// [`covering_slice`](NodeTree::covering_slice).
    ///
    /// A `NodeSlice` is always a **contiguous run of sibling nodes** — every node in
    /// the range must be a child of one and the same parent (that is what the slice's
    /// [`span`](NodeSlice::span)/[`source_text`](NodeSlice::source_text) exactness and
    /// the [`extract`](crate::extract) helpers rely on). This constructor answers
    /// `Some` exactly when `range` is such a run:
    ///
    /// - a range within one parent's children block (a node's children, an
    ///   argument/slot content range, or any sub-range of one) answers `Some`;
    /// - the root (index 0) has no siblings, so a range containing it answers `Some`
    ///   only as `0..1`;
    /// - an **empty** in-bounds range answers `Some` — empty runs are real values
    ///   (the accessors hand them out for childless nodes and empty regions);
    /// - everything else answers `None`: out-of-bounds or inverted ranges, and
    ///   in-bounds ranges that cross from one parent's children into another's.
    ///
    /// Like all bare node-index ranges (see [`nodes_in`](NodeTree::nodes_in)), the
    /// range carries no [`TreeTag`]: an in-bounds range minted by a *different* tree
    /// is not detected here and silently addresses this tree's nodes at those
    /// indices.
    pub fn slice(&self, range: Range<u32>) -> Option<NodeSlice<'_, L, A>> {
        // `finish()` caps node counts below `u32::MAX`, so the cast is lossless.
        let node_count = self.core.nodes.len() as u32;
        if range.start > range.end || range.end > node_count {
            return None;
        }
        if range.is_empty() {
            return Some(NodeSlice::new(self, range));
        }
        if range.start == 0 {
            // The root has no siblings: the only non-empty run containing it is 0..1.
            return (range.end == 1).then(|| NodeSlice::new(self, range));
        }
        // The sibling-run check, O(1) via the parent table: every index of one
        // parent's children block maps to that parent, and children blocks are
        // contiguous — so `range.start`'s parent block containing the range's last
        // index contains everything between.
        let parent = self.core.parent[range.start as usize];
        debug_assert_ne!(parent, NO_PARENT, "non-root node without a parent");
        let children = &self.core.nodes[parent as usize].children;
        (range.end <= children.end).then(|| NodeSlice::new(self, range))
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
                // Each node materializes against its OWN source (the `Spanned`
                // invariant) — a multi-source tree never resolves payloads
                // against an ambient single source.
                NodeData {
                    kind: data.kind.materialized(data.span.source()),
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

/// The deepest node under `node` whose span, lying in `source`, satisfies
/// `contains` — the shared per-source descent of
/// [`NodeTree::node_at`]/[`NodeTree::covering_slice`]. A node in the query's source
/// either satisfies the predicate (a match — refined only through *same-source*
/// children) or prunes its whole subtree (the recorded spans are trusted to nest,
/// which holds by construction under span tiling and is what a tree without it
/// degrades on); a node in a different source never matches, but its children are
/// searched (the route towards attached-source content).
fn deepest_containing<'t, L: Lang, A>(
    node: NodeRef<'t, L, A>,
    source: &Arc<Source<L::SourceOrigin>>,
    contains: &impl Fn(&SourceSpan<L::SourceOrigin>) -> bool,
) -> Option<NodeRef<'t, L, A>> {
    if Arc::ptr_eq(node.span().source(), source) {
        if !contains(node.span()) {
            return None;
        }
        for child in node.children() {
            if Arc::ptr_eq(child.span().source(), source) {
                if let Some(deeper) = deepest_containing(child, source, contains) {
                    return Some(deeper);
                }
            }
        }
        Some(node)
    } else {
        for child in node.children() {
            if let Some(found) = deepest_containing(child, source, contains) {
                return Some(found);
            }
        }
        None
    }
}

/// The minimal run of `node`'s children covering the `query`, as a global node-index
/// range — `None` when the children cannot cover the query's bytes (the caller then
/// answers with `node` itself). Binary search over the children first (sibling spans
/// are sorted and tile whenever the tree comes from a parse of a language that obeys
/// span tiling); the candidate is verified locally, and any failure falls back to a
/// linear scan whose result is verified the same way — no verified run, no answer.
fn covering_child_run<L: Lang, A>(
    node: NodeRef<'_, L, A>,
    query: &SourceSpan<L::SourceOrigin>,
) -> Option<Range<u32>> {
    if query.is_empty() || node.child_count() == 0 {
        // An empty query resolves by point containment: no child contains the point
        // (the descent would have gone deeper), so only the node itself answers.
        return None;
    }
    let tree = node.tree();
    let block = node.children().range();
    if let Some(run) = binary_candidate_run(tree, &block, query) {
        if verify_covering_run(tree, &block, &run, query) {
            return Some(run);
        }
    }
    // Linear fallback: the first and the last child overlapping the query.
    let overlaps = |i: u32| {
        let span = &tree.nodes()[i as usize].span;
        Arc::ptr_eq(span.source(), query.source())
            && span.start() < query.end()
            && span.end() > query.start()
    };
    let first = block.clone().find(|&i| overlaps(i))?;
    let last = block.clone().rev().find(|&i| overlaps(i))?;
    let run = first..last + 1;
    verify_covering_run(tree, &block, &run, query).then_some(run)
}

/// Binary-search candidate for the covering run, assuming span-sorted same-source
/// siblings (true for a tree parsed from a language that obeys span tiling; the
/// caller verifies before trusting it): the
/// first child whose span reaches past the query's start, through the last child
/// starting before the query's end.
fn binary_candidate_run<L: Lang, A>(
    tree: &NodeTree<L, A>,
    block: &Range<u32>,
    query: &SourceSpan<L::SourceOrigin>,
) -> Option<Range<u32>> {
    let len = (block.end - block.start) as usize;
    let span_at = |k: usize| &tree.nodes()[block.start as usize + k].span;
    let first = partition_point(len, |k| span_at(k).end() <= query.start());
    let one_past_last = partition_point(len, |k| span_at(k).start() < query.end());
    (first < one_past_last)
        .then(|| block.start + first as u32..block.start + one_past_last as u32)
}

/// `[T]::partition_point` over plain indices (the predicate's inputs are read out of
/// the node storage, not a slice).
fn partition_point(len: usize, pred: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, len);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pred(mid) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Verify a candidate covering run against the recorded spans: same-source
/// throughout, query bytes in both end nodes, the query's edges covered, adjacent
/// spans across the run, and minimality against both neighbors. Anything else is not
/// trusted to cover the query (the caller degrades to the covering node itself) —
/// which is what a tree whose siblings do not tile answers with.
fn verify_covering_run<L: Lang, A>(
    tree: &NodeTree<L, A>,
    block: &Range<u32>,
    run: &Range<u32>,
    query: &SourceSpan<L::SourceOrigin>,
) -> bool {
    if run.start < block.start || run.end > block.end || run.start >= run.end {
        return false;
    }
    let span_at = |i: u32| &tree.nodes()[i as usize].span;
    let same_source = |i: u32| Arc::ptr_eq(span_at(i).source(), query.source());
    let overlaps = |i: u32| {
        same_source(i) && span_at(i).start() < query.end() && span_at(i).end() > query.start()
    };
    if !run.clone().all(same_source) {
        return false;
    }
    // The run's end nodes carry query bytes and cover the query's edges.
    if !overlaps(run.start) || !overlaps(run.end - 1) {
        return false;
    }
    if span_at(run.start).start() > query.start() || span_at(run.end - 1).end() < query.end() {
        return false;
    }
    // Adjacent across the run: the sibling spans must meet exactly, or the interior
    // bytes are not covered by these children.
    if !(run.start..run.end - 1).all(|i| span_at(i).end() == span_at(i + 1).start()) {
        return false;
    }
    // Minimal: the neighbors carry no query bytes.
    if run.start > block.start && overlaps(run.start - 1) {
        return false;
    }
    if run.end < block.end && overlaps(run.end) {
        return false;
    }
    true
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
