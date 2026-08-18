//! [`NodeSlice`]: a `Copy` view of a contiguous run of sibling nodes, with a covering
//! span.
//!
//! Every "list of nodes" the read API hands out — a node's children, an argument's
//! region or content nodes, a slot's content — is a contiguous run of siblings in the
//! flat tree layout, and this is its currency: the return type of
//! [`NodeRef::children`](super::NodeRef::children) and the region/content accessors,
//! and of [`NodeTree::slice`](super::NodeTree::slice), the validated constructor
//! over a bare index range.
//! Beyond iteration it answers *where the run is in the source*
//! ([`span`](NodeSlice::span), [`source_text`](NodeSlice::source_text)): the first
//! node's span start to the last node's span end. On a tree parsed from a language
//! that obeys span tiling
//! ([`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)) that covering
//! span is **exact** — sibling spans of such a tree are adjacent, so it is the run's
//! own text and nothing besides. Otherwise it is the covering span of the coordinates
//! recorded on the nodes, which may include bytes no node of the run claims (see the
//! two contracts). Both accessors answer only when the whole run lies within a single
//! source (restaged trees, and parses of a language with `OBEYS_SPAN_TILING = false`,
//! can mix sources within one run). What the nodes *say* is read from their own data,
//! not from these coordinates. The extraction helpers ([`extract`](crate::extract))
//! consume and produce these views.

use core::fmt;
use core::ops::Range;

use crate::source::SourceSpan;
use crate::state::Lang;

use super::node_ref::NodeRef;
use super::tree::NodeTree;

/// A contiguous run of sibling nodes of one [`NodeTree`] — the node-list view returned
/// by [`NodeRef::children`](super::NodeRef::children) and the argument/slot content
/// accessors — or built from a bare index range via
/// [`NodeTree::slice`](super::NodeTree::slice) — and the input of the
/// [`extract`](crate::extract) helpers.
///
/// `Copy` like [`NodeRef`]: it stores only the tree borrow and an index range, and the
/// borrow checker guarantees it cannot outlive the tree. Iterate it directly
/// (`for node in slice`, via [`IntoIterator`]) or through [`iter`](NodeSlice::iter) for
/// adaptor chains.
pub struct NodeSlice<'t, L: Lang, A = ()> {
    tree: &'t NodeTree<L, A>,
    // Stored unpacked (not `Range<u32>`) so the view stays `Copy`.
    start: u32,
    end: u32,
}

impl<'t, L: Lang, A> NodeSlice<'t, L, A> {
    /// A slice over `range` of `tree`'s flat storage. In-crate constructor: public
    /// values come from the accessors ([`NodeRef::children`](super::NodeRef::children),
    /// the region/content accessors, the extract helpers), which only mint ranges of
    /// sibling nodes, and from the validated [`NodeTree::slice`].
    pub(crate) fn new(tree: &'t NodeTree<L, A>, range: Range<u32>) -> NodeSlice<'t, L, A> {
        assert!(
            range.start <= range.end && range.end as usize <= tree.node_count(),
            "node range {:?} out of range",
            range
        );
        NodeSlice { tree, start: range.start, end: range.end }
    }

    /// The number of nodes in the run.
    pub fn len(&self) -> usize {
        (self.end - self.start) as usize
    }

    /// Whether the run is empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// The `i`-th node of the run.
    pub fn get(&self, i: usize) -> Option<NodeRef<'t, L, A>> {
        let id = self.start.checked_add(u32::try_from(i).ok()?)?;
        (id < self.end).then(|| self.tree.node(self.tree.make_id(id)))
    }

    /// The first node of the run.
    pub fn first(&self) -> Option<NodeRef<'t, L, A>> {
        self.get(0)
    }

    /// The last node of the run.
    pub fn last(&self) -> Option<NodeRef<'t, L, A>> {
        if self.is_empty() {
            return None;
        }
        Some(self.tree.node(self.tree.make_id(self.end - 1)))
    }

    /// The nodes, in source order.
    pub fn iter(&self) -> NodeSliceIter<'t, L, A> {
        NodeSliceIter { tree: self.tree, next: self.start, end: self.end }
    }

    /// The run's global node-index range (the [`ChildRegion`](super::ChildRegion) /
    /// [`NodeTree::nodes_in`](super::NodeTree::nodes_in) coordinate system).
    pub fn range(&self) -> Range<u32> {
        self.start..self.end
    }

    /// The tree this slice views (in-crate: the extract helpers' anchor for empty
    /// slices).
    pub(crate) fn tree(&self) -> &'t NodeTree<L, A> {
        self.tree
    }

    /// Whether the whole run lies within one single `Source` — the contract gate of
    /// [`span`](NodeSlice::span)/[`source_text`](NodeSlice::source_text). O(1) when
    /// the whole tree is single-source (the flag `finish()` computes); otherwise a
    /// scan of every node of the run. `false` for an empty run (no source to name).
    fn is_single_source_run(&self) -> bool {
        if self.tree.is_single_source() {
            return true;
        }
        let Some(first) = self.first() else {
            return false;
        };
        let source = first.span().source();
        self.iter().all(|node| alloc::sync::Arc::ptr_eq(node.span().source(), source))
    }

    /// The run's covering [`SourceSpan`]: the first node's span start to the last
    /// node's span end.
    ///
    /// **Exact** on a tree parsed from a language that obeys span tiling
    /// ([`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)): sibling
    /// spans of such a tree are adjacent, so the covering span is the run's own text,
    /// not an approximation. For a language with `OBEYS_SPAN_TILING = false`, and on
    /// restaged or synthesized trees, the answer is the covering span of the
    /// coordinates the nodes carry — the nodes need not be adjacent, so it can
    /// include bytes no node of the run claims. It is a provenance answer either way;
    /// what the nodes say is read from their own data
    /// ([`NodeRef::chars`](super::NodeRef::chars) and the delimiter and payload
    /// accessors).
    ///
    /// Answers only when **the whole run lies within a single source**: every node
    /// of the run — not just the endpoints — must live in one and the same `Source`
    /// (verified across the run; O(1) on single-source trees via the flag computed
    /// at `finish()`), with the endpoints in source order (first start ≤ last end).
    /// `None` means there is no single-source answer: the run is empty, a node of
    /// the run lives in a different source, or the endpoints are out of order. A
    /// sibling run of a tree parsed from a language that obeys span tiling always
    /// answers; restaged and synthesized trees, and parses of a language with
    /// `OBEYS_SPAN_TILING = false`, need not. Per-node accessors
    /// ([`NodeRef::span`](super::NodeRef::span)) stay valid on any tree (a node's own
    /// span is its provenance).
    pub fn span(&self) -> Option<SourceSpan<L::SourceOrigin>> {
        let (first, last) = (self.first()?, self.last()?);
        let (first, last) = (first.span(), last.span());
        if !self.is_single_source_run() || first.start() > last.end() {
            return None;
        }
        Some(SourceSpan::new(first.source(), first.start()..last.end()))
    }

    /// The source text the run's covering [`span`](NodeSlice::span) points at —
    /// pylatexenc's `latex_verbatim()` for a node list. Same contract as
    /// [`span`](NodeSlice::span), exactness included: it is the run's own original
    /// text where that span is exact, and the text under the recorded coordinates
    /// otherwise. Answers only when the whole run lies within a single source, with
    /// the endpoints in source order; `None` exactly when `span()` is.
    pub fn source_text(&self) -> Option<&'t str> {
        let (first, last) = (self.first()?, self.last()?);
        let (first, last) = (first.span(), last.span());
        if !self.is_single_source_run() || first.start() > last.end() {
            return None;
        }
        first.source().content().get(first.start()..last.end())
    }
}

/// Iterator over a [`NodeSlice`]'s nodes, in source order.
pub struct NodeSliceIter<'t, L: Lang, A = ()> {
    tree: &'t NodeTree<L, A>,
    next: u32,
    end: u32,
}

impl<'t, L: Lang, A> Iterator for NodeSliceIter<'t, L, A> {
    type Item = NodeRef<'t, L, A>;

    fn next(&mut self) -> Option<NodeRef<'t, L, A>> {
        if self.next >= self.end {
            return None;
        }
        let node = self.tree.node(self.tree.make_id(self.next));
        self.next += 1;
        Some(node)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.end - self.next) as usize;
        (len, Some(len))
    }
}

impl<L: Lang, A> ExactSizeIterator for NodeSliceIter<'_, L, A> {}

impl<'t, L: Lang, A> DoubleEndedIterator for NodeSliceIter<'t, L, A> {
    fn next_back(&mut self) -> Option<NodeRef<'t, L, A>> {
        if self.next >= self.end {
            return None;
        }
        self.end -= 1;
        Some(self.tree.node(self.tree.make_id(self.end)))
    }
}

impl<'t, L: Lang, A> IntoIterator for NodeSlice<'t, L, A> {
    type Item = NodeRef<'t, L, A>;
    type IntoIter = NodeSliceIter<'t, L, A>;

    fn into_iter(self) -> NodeSliceIter<'t, L, A> {
        self.iter()
    }
}

impl<'t, L: Lang, A> IntoIterator for &NodeSlice<'t, L, A> {
    type Item = NodeRef<'t, L, A>;
    type IntoIter = NodeSliceIter<'t, L, A>;

    fn into_iter(self) -> NodeSliceIter<'t, L, A> {
        self.iter()
    }
}

// Manual impls: `NodeSlice` is Copy regardless of `L` (a borrow and two indices);
// the iterator is Clone (resumable position, deliberately not Copy — mutating
// iterators that silently copy are a footgun).

impl<L: Lang, A> Clone for NodeSlice<'_, L, A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang, A> Copy for NodeSlice<'_, L, A> {}

impl<L: Lang, A> Clone for NodeSliceIter<'_, L, A> {
    fn clone(&self) -> Self {
        NodeSliceIter { tree: self.tree, next: self.next, end: self.end }
    }
}

impl<L: Lang, A> fmt::Debug for NodeSlice<'_, L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeSlice").field("range", &(self.start..self.end)).finish()
    }
}

impl<L: Lang, A> fmt::Debug for NodeSliceIter<'_, L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeSliceIter").field("range", &(self.next..self.end)).finish()
    }
}
