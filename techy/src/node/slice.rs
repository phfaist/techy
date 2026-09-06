//! Views over runs of sibling nodes: [`NodeSlice`], a `Copy` view of a contiguous run
//! with a covering span, and its iterator [`NodeSliceIter`].
//!
//! Every list of nodes the read API returns — a node's children, an argument's region
//! or content nodes, a slot's content — is a contiguous run of siblings in the flat
//! tree layout, and [`NodeSlice`] is that value: the return type of
//! [`NodeRef::children`](super::NodeRef::children) and of the region and content
//! accessors, and of [`NodeTree::slice`](super::NodeTree::slice), the validated
//! constructor over a bare index range. The extraction helpers
//! ([`extract`](crate::extract)) consume and produce these views.
//!
//! Beyond iteration, a slice answers *where the run is in the source*
//! ([`span`](NodeSlice::span), [`source_text`](NodeSlice::source_text)): from the first
//! node's span start to the last node's span end. On a tree parsed from a language that
//! obeys span tiling
//! ([`Lang::OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING)) that covering
//! span is **exact**, because sibling spans of such a tree are adjacent: it is the
//! run's own text and nothing besides. Otherwise it covers the coordinates recorded on
//! the nodes and may include bytes no node of the run claims. Both accessors answer
//! only when the whole run lies within a single source; restaged trees, and parses of a
//! language with `OBEYS_SPAN_TILING = false`, can mix sources within one run. Their own
//! documentation states the full contract.
//!
//! What the nodes *say* is read from their own data, not from these coordinates.

use core::fmt;
use core::ops::Range;

use crate::source::SourceSpan;
use crate::state::Lang;

use super::node_ref::NodeRef;
use super::tree::NodeTree;

/// A contiguous run of sibling nodes of one [`NodeTree`].
///
/// This is the node-list view returned by
/// [`NodeRef::children`](super::NodeRef::children) and by the argument and slot content
/// accessors; [`NodeTree::slice`](super::NodeTree::slice) builds one from a bare index
/// range, and the [`extract`](crate::extract) helpers take one as input.
///
/// `NodeSlice` is `Copy` like [`NodeRef`]: it stores only the tree borrow and an index
/// range, and the borrow checker guarantees it cannot outlive the tree. Iterate it
/// directly (`for node in slice`, through [`IntoIterator`]) or call
/// [`iter`](NodeSlice::iter) for adaptor chains. Beyond iteration it answers
/// [`span`](NodeSlice::span) and [`source_text`](NodeSlice::source_text) for the whole
/// run.
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

    /// The run's global node-index range, in the coordinate system of
    /// [`ChildRegion`](super::ChildRegion) and
    /// [`NodeTree::nodes_in`](super::NodeTree::nodes_in).
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

    /// The run's covering [`SourceSpan`]: from the first node's span start to the last
    /// node's span end.
    ///
    /// The span is **exact** on a tree parsed from a language that obeys span tiling
    /// ([`Lang::OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING)), whose
    /// sibling spans are adjacent: it is the run's own text, not an approximation. For
    /// a language with `OBEYS_SPAN_TILING = false`, and on restaged or synthesized
    /// trees, it covers the coordinates the nodes carry, which need not be adjacent, so
    /// it can include bytes no node of the run claims. Either way it answers
    /// provenance; what the nodes say is read from their own data
    /// ([`NodeRef::chars`](super::NodeRef::chars) and the delimiter and payload
    /// accessors).
    ///
    /// The method answers only when **the whole run lies within a single source**:
    /// every node of the run, not just the endpoints, must belong to one and the same
    /// `Source`, and the endpoints must be in source order (first start ≤ last end).
    /// This is verified across the run, in O(1) on a single-source tree.
    ///
    /// `None` therefore means there is no single-source answer: the run is empty, one
    /// of its nodes belongs to a different source, or the endpoints are out of order. A
    /// sibling run of a tree parsed from a language that obeys span tiling always
    /// answers; restaged and synthesized trees, and parses of a language with
    /// `OBEYS_SPAN_TILING = false`, need not. The per-node
    /// [`NodeRef::span`](super::NodeRef::span) stays valid on any tree, since a node's
    /// own span is its provenance.
    pub fn span(&self) -> Option<SourceSpan<L::SourceOrigin>> {
        let (first, last) = (self.first()?, self.last()?);
        let (first, last) = (first.span(), last.span());
        if !self.is_single_source_run() || first.start() > last.end() {
            return None;
        }
        Some(SourceSpan::new(first.source(), first.start()..last.end()))
    }

    /// The source text the run's covering [`span`](NodeSlice::span) points at —
    /// pylatexenc's `latex_verbatim()` for a node list.
    ///
    /// The contract is that of [`span`](NodeSlice::span), exactness included: this is
    /// the run's own original text where that span is exact, and the text under the
    /// recorded coordinates otherwise. It answers only when the whole run lies within a
    /// single source with the endpoints in source order, and is `None` exactly when
    /// `span()` is.
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
