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

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::source::{SourceSpan, TextContent};
use crate::state::{Lang, ParsingState};

use super::kind::NodeKind;
use super::layout::ArgLayout;
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
/// builder — driven by `ParserSession` (Phase 6), tests, and future transforms — is the
/// only place nodes are assembled.
///
/// # Contract (checked, panicking on violation — these are caller bugs, not runtime
/// conditions)
///
/// - A child `BuildId` must already be staged (which also makes cycles unrepresentable).
/// - Each staged node is used as a child at most once, and the root must not be anyone's
///   child.
/// - A `Callable` kind's `ArgsLayout`/`SlotsLayout` child offsets must index into the
///   node's child list.
/// - Debug builds additionally check the `TextContent` invariant: `Spanned` ranges must
///   lie inside the node's own source content, on `char` boundaries.
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
    /// children in order (for a `Callable`: one node per present argument, then one
    /// `List` node per slot — the layout offsets index this list).
    pub fn add(
        &mut self,
        kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
        parsing_state: Arc<ParsingState<L>>,
        children: Vec<BuildId>,
    ) -> BuildId {
        self.add_with_ext(kind, span, parsing_state, children, Default::default())
    }

    /// Stage a node with an explicit uniform ext (tier 1).
    pub fn add_with_ext(
        &mut self,
        kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
        parsing_state: Arc<ParsingState<L>>,
        children: Vec<BuildId>,
        ext: NodeExt<L>,
    ) -> BuildId {
        assert!(self.staged.len() < u32::MAX as usize, "node tree too large");
        for child in &children {
            let staged = self
                .staged
                .get_mut(child.0 as usize)
                .unwrap_or_else(|| panic!("child {:?} has not been staged", child));
            assert!(!staged.claimed, "child {:?} already has a parent", child);
            staged.claimed = true;
        }
        if let NodeKind::Callable(data) = &kind {
            for arg in &data.args.args {
                if let ArgLayout::Present { child } = arg {
                    assert!(
                        (*child as usize) < children.len(),
                        "argument child offset {} out of bounds ({} children)",
                        child,
                        children.len()
                    );
                }
            }
            for slot in &data.slots.slots {
                assert!(
                    (slot.child as usize) < children.len(),
                    "slot child offset {} out of bounds ({} children)",
                    slot.child,
                    children.len()
                );
            }
        }
        debug_assert_spanned_contents(&kind, &span);

        let id = BuildId(self.staged.len() as u32);
        self.staged.push(Staged { kind, ext, span, parsing_state, children, claimed: false });
        id
    }

    /// Freeze everything reachable from `root` into a flat [`NodeTree`] (breadth-first:
    /// root at index 0, each node's children as one contiguous block). Staged nodes not
    /// reachable from `root` are dropped.
    pub fn finish(self, root: BuildId) -> NodeTree<L> {
        let mut staged: Vec<Option<Staged<L>>> = self.staged.into_iter().map(Some).collect();
        assert!((root.0 as usize) < staged.len(), "root {:?} has not been staged", root);
        assert!(
            !staged[root.0 as usize].as_ref().unwrap().claimed,
            "root {:?} is another node's child",
            root
        );

        // Pass 1: breadth-first order and per-node children ranges. Child ids were
        // checked staged-and-claimed-once in add(), so the traversal visits each staged
        // node at most once.
        let mut order: Vec<u32> = Vec::with_capacity(staged.len());
        let mut ranges: Vec<core::ops::Range<u32>> = Vec::with_capacity(staged.len());
        order.push(root.0);
        let mut pos = 0;
        while pos < order.len() {
            let sid = order[pos] as usize;
            let start = order.len() as u32;
            for child in &staged[sid].as_ref().unwrap().children {
                order.push(child.0);
            }
            let end = order.len() as u32;
            ranges.push(start..end);
            pos += 1;
        }

        // Pass 2: move the staged data into place.
        let nodes = order
            .iter()
            .zip(ranges)
            .map(|(&sid, children)| {
                let staged = staged[sid as usize].take().expect("staged node used twice");
                NodeData {
                    kind: staged.kind,
                    ext: staged.ext,
                    span: staged.span,
                    parsing_state: staged.parsing_state,
                    children,
                }
            })
            .collect();
        NodeTree { nodes }
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

/// Debug-check the `TextContent` invariant: every `Spanned` value of this node refers
/// into the node's own source, in bounds and on `char` boundaries.
fn debug_assert_spanned_contents<L: Lang>(kind: &NodeKind<L>, span: &SourceSpan<L::SourceOrigin>) {
    if cfg!(debug_assertions) {
        let content = span.source().content();
        let check = |text: &TextContent, what: &str| {
            if let TextContent::Spanned(s) = text {
                debug_assert!(
                    content.get(s.range()).is_some(),
                    "{} span {:?} is not a valid range of the node's source (len {})",
                    what,
                    s,
                    content.len()
                );
            }
        };
        match kind {
            NodeKind::Chars { content: text, .. } => check(text, "chars content"),
            NodeKind::Comment { content: text, .. } => check(text, "comment content"),
            NodeKind::Callable(data) => {
                check(&data.post_space, "callable post_space");
                for arg in &data.args.args {
                    if let ArgLayout::Marker { text } = arg {
                        check(text, "argument marker");
                    }
                }
            }
            NodeKind::Group { .. } | NodeKind::List { .. } => {}
        }
    }
}
