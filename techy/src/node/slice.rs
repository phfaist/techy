//! [`NodeSlice`]: a `Copy` view of a contiguous run of sibling nodes, with exact span
//! information.
//!
//! Every "list of nodes" the read API hands out — a node's children, an argument's
//! region or content nodes, a slot's content — is a contiguous run of siblings in the
//! flat tree layout, and this is its currency: the return type of
//! [`NodeRef::children`](super::NodeRef::children) and the region/content accessors.
//! Beyond iteration it answers *where the run is in the source* ([`span`](NodeSlice::span),
//! [`source_text`](NodeSlice::source_text)) **exactly** — sibling runs are
//! span-contiguous by the partition invariant, so the covering span is the first
//! node's start to the last node's end, not an approximation. The extraction helpers
//! ([`extract`](crate::extract)) consume and produce these views.

use core::fmt;
use core::ops::Range;

use crate::source::SourceSpan;
use crate::state::Lang;

use super::node_ref::NodeRef;
use super::tree::NodeTree;

/// A contiguous run of sibling nodes of one [`NodeTree`] — the node-list view returned
/// by [`NodeRef::children`](super::NodeRef::children) and the argument/slot content
/// accessors, and the input of the [`extract`](crate::extract) helpers.
///
/// `Copy` like [`NodeRef`]: it stores only the tree borrow and an index range, and the
/// borrow checker guarantees it cannot outlive the tree. Iterate it directly
/// (`for node in slice`, via [`IntoIterator`]) or through [`iter`](NodeSlice::iter) for
/// adaptor chains.
pub struct NodeSlice<'t, L: Lang> {
    tree: &'t NodeTree<L>,
    // Stored unpacked (not `Range<u32>`) so the view stays `Copy`.
    start: u32,
    end: u32,
}

impl<'t, L: Lang> NodeSlice<'t, L> {
    /// A slice over `range` of `tree`'s flat storage. In-crate constructor: public
    /// values come from the accessors ([`NodeRef::children`](super::NodeRef::children),
    /// the region/content accessors, the extract helpers), which only mint ranges of
    /// sibling nodes.
    pub(crate) fn new(tree: &'t NodeTree<L>, range: Range<u32>) -> NodeSlice<'t, L> {
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
    pub fn get(&self, i: usize) -> Option<NodeRef<'t, L>> {
        let id = self.start.checked_add(u32::try_from(i).ok()?)?;
        (id < self.end).then(|| self.tree.node(self.tree.make_id(id)))
    }

    /// The first node of the run.
    pub fn first(&self) -> Option<NodeRef<'t, L>> {
        self.get(0)
    }

    /// The last node of the run.
    pub fn last(&self) -> Option<NodeRef<'t, L>> {
        if self.is_empty() {
            return None;
        }
        Some(self.tree.node(self.tree.make_id(self.end - 1)))
    }

    /// The nodes, in source order.
    pub fn iter(&self) -> NodeSliceIter<'t, L> {
        NodeSliceIter { tree: self.tree, next: self.start, end: self.end }
    }

    /// The run's global node-index range (the [`ChildRegion`](super::ChildRegion) /
    /// [`NodeTree::nodes_in`](super::NodeTree::nodes_in) coordinate system).
    pub fn range(&self) -> Range<u32> {
        self.start..self.end
    }

    /// The tree this slice views (in-crate: the extract helpers' anchor for empty
    /// slices).
    pub(crate) fn tree(&self) -> &'t NodeTree<L> {
        self.tree
    }

    /// The run's covering [`SourceSpan`] — **exact**: sibling runs are span-contiguous
    /// (the partition invariant), so this is the first node's start to the last
    /// node's end, in the run's own source.
    ///
    /// `None` in exactly two honest cases: the run is **empty** (no source material to
    /// point at), or its first and last nodes live in **different sources** (possible
    /// in synthesized/spliced trees only — a parsed tree's siblings share one source).
    pub fn span(&self) -> Option<SourceSpan<L::SourceOrigin>> {
        let (first, last) = (self.first()?, self.last()?);
        let (first, last) = (first.span(), last.span());
        if !first.same_source(last) || first.start() > last.end() {
            return None;
        }
        Some(SourceSpan::new(first.source(), first.start()..last.end()))
    }

    /// The exact original source text of the run (the text [`span`](NodeSlice::span)
    /// points at) — pylatexenc's `latex_verbatim()` for a node list. `None` exactly
    /// when `span()` is.
    pub fn source_text(&self) -> Option<&'t str> {
        let (first, last) = (self.first()?, self.last()?);
        let (first, last) = (first.span(), last.span());
        if !first.same_source(last) {
            return None;
        }
        first.source().content().get(first.start()..last.end())
    }
}

/// Iterator over a [`NodeSlice`]'s nodes, in source order.
pub struct NodeSliceIter<'t, L: Lang> {
    tree: &'t NodeTree<L>,
    next: u32,
    end: u32,
}

impl<'t, L: Lang> Iterator for NodeSliceIter<'t, L> {
    type Item = NodeRef<'t, L>;

    fn next(&mut self) -> Option<NodeRef<'t, L>> {
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

impl<L: Lang> ExactSizeIterator for NodeSliceIter<'_, L> {}

impl<'t, L: Lang> DoubleEndedIterator for NodeSliceIter<'t, L> {
    fn next_back(&mut self) -> Option<NodeRef<'t, L>> {
        if self.next >= self.end {
            return None;
        }
        self.end -= 1;
        Some(self.tree.node(self.tree.make_id(self.end)))
    }
}

impl<'t, L: Lang> IntoIterator for NodeSlice<'t, L> {
    type Item = NodeRef<'t, L>;
    type IntoIter = NodeSliceIter<'t, L>;

    fn into_iter(self) -> NodeSliceIter<'t, L> {
        self.iter()
    }
}

impl<'t, L: Lang> IntoIterator for &NodeSlice<'t, L> {
    type Item = NodeRef<'t, L>;
    type IntoIter = NodeSliceIter<'t, L>;

    fn into_iter(self) -> NodeSliceIter<'t, L> {
        self.iter()
    }
}

// Manual impls: `NodeSlice` is Copy regardless of `L` (a borrow and two indices);
// the iterator is Clone (resumable position, deliberately not Copy — mutating
// iterators that silently copy are a footgun).

impl<L: Lang> Clone for NodeSlice<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for NodeSlice<'_, L> {}

impl<L: Lang> Clone for NodeSliceIter<'_, L> {
    fn clone(&self) -> Self {
        NodeSliceIter { tree: self.tree, next: self.next, end: self.end }
    }
}

impl<L: Lang> fmt::Debug for NodeSlice<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeSlice").field("range", &(self.start..self.end)).finish()
    }
}

impl<L: Lang> fmt::Debug for NodeSliceIter<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeSliceIter").field("range", &(self.next..self.end)).finish()
    }
}
