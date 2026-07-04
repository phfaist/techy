//! [`NodeRef`]: the copyable read proxy over a [`NodeTree`]'s flat storage.

use alloc::sync::Arc;
use core::fmt;

use crate::source::SourceSpan;
use crate::spec::{CallableSpec, CallableTypeId};
use crate::state::{Lang, ParsingState};
use crate::token::GroupTypeId;

use super::kind::{CallableData, NodeKind};
use super::layout::{ArgLayout, ArgsLayout, SlotsLayout};
use super::tree::{NodeData, NodeId, NodeTree};
use super::NodeExt;

/// A reference to one node of a [`NodeTree`]: `Copy`, resolves indices, and borrows the
/// tree — the borrow checker guarantees a `NodeRef` cannot outlive the storage its index
/// points into (indices made safe by construction).
///
/// Accessors return `'t`-borrowed data (borrowing the *tree*, not this transient proxy),
/// so extracted references outlive the `NodeRef` value itself.
///
/// Kind-specific accessors are `Option`-returning on the wrong kind; preset-level sugar
/// (`as_math()`-style environment/macro views) arrives with the latexlike preset
/// (Phase 7).
pub struct NodeRef<'t, L: Lang> {
    tree: &'t NodeTree<L>,
    id: NodeId,
}

impl<'t, L: Lang> NodeRef<'t, L> {
    pub(crate) fn new(tree: &'t NodeTree<L>, id: NodeId) -> NodeRef<'t, L> {
        NodeRef { tree, id }
    }

    fn data(&self) -> &'t NodeData<L> {
        &self.tree.nodes[self.id.index()]
    }

    /// This node's id within its tree.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The structural kind (exhaustively matchable).
    pub fn kind(&self) -> &'t NodeKind<L> {
        &self.data().kind
    }

    /// The uniform (tier-1) ext data.
    pub fn ext(&self) -> &'t NodeExt<L> {
        &self.data().ext
    }

    /// The node's provenance span (`Arc<Source>` + byte range).
    pub fn span(&self) -> &'t SourceSpan<L::SourceOrigin> {
        &self.data().span
    }

    /// The exact original text of this node — level-1 verbatim recomposition
    /// (ARCHITECTURE.md §nodes): never needs an external lookup, works for detached and
    /// mixed-origin trees.
    pub fn span_content(&self) -> &'t str {
        self.data().span.content()
    }

    /// The parsing state this node was parsed under.
    pub fn parsing_state(&self) -> &'t Arc<ParsingState<L>> {
        &self.data().parsing_state
    }

    /// The content of this node's own source (what `TextContent::Spanned` resolves
    /// against).
    fn source_content(&self) -> &'t str {
        self.data().span.source().content()
    }

    // --- children ---------------------------------------------------------------------

    /// The number of structural children.
    pub fn child_count(&self) -> usize {
        self.data().children.len()
    }

    /// The `i`-th structural child.
    pub fn child(&self, i: usize) -> Option<NodeRef<'t, L>> {
        let children = &self.data().children;
        let id = children.start.checked_add(i as u32)?;
        (id < children.end).then(|| NodeRef::new(self.tree, NodeId(id)))
    }

    /// The structural children, in order.
    pub fn children(&self) -> impl Iterator<Item = NodeRef<'t, L>> {
        let tree = self.tree;
        self.data().children.clone().map(move |i| NodeRef::new(tree, NodeId(i)))
    }

    // --- kind predicates and kind-specific accessors ------------------------------------

    /// Whether this is a `Chars` node.
    pub fn is_chars(&self) -> bool {
        matches!(self.kind(), NodeKind::Chars { .. })
    }

    /// Whether this is a `Group` node.
    pub fn is_group(&self) -> bool {
        matches!(self.kind(), NodeKind::Group { .. })
    }

    /// Whether this is a `Callable` node.
    pub fn is_callable(&self) -> bool {
        matches!(self.kind(), NodeKind::Callable(_))
    }

    /// Whether this is a `Comment` node.
    pub fn is_comment(&self) -> bool {
        matches!(self.kind(), NodeKind::Comment { .. })
    }

    /// Whether this is a `List` node.
    pub fn is_list(&self) -> bool {
        matches!(self.kind(), NodeKind::List { .. })
    }

    /// A `Chars` node's logical text.
    pub fn chars(&self) -> Option<&'t str> {
        match self.kind() {
            NodeKind::Chars { content, .. } => Some(content.resolve(self.source_content())),
            _ => None,
        }
    }

    /// A `Comment` node's logical text (sans delimiter and newline).
    pub fn comment(&self) -> Option<&'t str> {
        match self.kind() {
            NodeKind::Comment { content, .. } => Some(content.resolve(self.source_content())),
            _ => None,
        }
    }

    /// A `Group` node's group type.
    pub fn group_type(&self) -> Option<GroupTypeId> {
        match self.kind() {
            NodeKind::Group { group_type, .. } => Some(*group_type),
            _ => None,
        }
    }

    // --- callable accessors -------------------------------------------------------------

    /// A `Callable` node's full invocation data.
    pub fn callable(&self) -> Option<&'t CallableData<L>> {
        match self.kind() {
            NodeKind::Callable(data) => Some(data),
            _ => None,
        }
    }

    /// A `Callable` node's invocation form.
    pub fn callable_type(&self) -> Option<CallableTypeId> {
        self.callable().map(|data| data.callable_type)
    }

    /// A `Callable` node's invocation spelling.
    pub fn name(&self) -> Option<&'t str> {
        self.callable().map(|data| &*data.name)
    }

    /// A `Callable` node's behavior spec.
    pub fn spec(&self) -> Option<&'t Arc<dyn CallableSpec<L>>> {
        self.callable().map(|data| &data.spec)
    }

    /// A `Callable` node's post-space, as logical text.
    pub fn post_space(&self) -> Option<&'t str> {
        self.callable().map(|data| data.post_space.resolve(self.source_content()))
    }

    /// A `Callable` node's argument layout.
    pub fn args_layout(&self) -> Option<&'t ArgsLayout> {
        self.callable().map(|data| &data.args)
    }

    /// A `Callable` node's slot layout.
    pub fn slots_layout(&self) -> Option<&'t SlotsLayout> {
        self.callable().map(|data| &data.slots)
    }

    /// The node of spec-argument `i`, when this is a callable and the argument is
    /// present *as a node* (`None` for absent optionals and content-free markers —
    /// consult [`args_layout`](NodeRef::args_layout) to distinguish).
    pub fn argument(&self, i: usize) -> Option<NodeRef<'t, L>> {
        let child = self.callable()?.args.get(i)?.child()?;
        self.child(child as usize)
    }

    /// A callable's argument entries with their resolved nodes, in spec order.
    pub fn arguments(&self) -> impl Iterator<Item = (&'t ArgLayout, Option<NodeRef<'t, L>>)> {
        let this = *self;
        self.callable()
            .map(|data| data.args.args.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(move |arg| (arg, arg.child().and_then(|c| this.child(c as usize))))
    }

    /// The `List` node holding slot `i`'s content, when this is a callable with such a
    /// slot.
    pub fn slot(&self, i: usize) -> Option<NodeRef<'t, L>> {
        let slot = self.callable()?.slots.get(i)?;
        self.child(slot.child as usize)
    }

    /// The first slot's content — the *body* of environment-shaped callables (which have
    /// exactly one slot).
    pub fn body(&self) -> Option<NodeRef<'t, L>> {
        self.slot(0)
    }
}

// Manual impls: `NodeRef` is Copy regardless of `L` (it stores only a borrow and an id).

impl<L: Lang> Clone for NodeRef<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for NodeRef<'_, L> {}

impl<L: Lang> fmt::Debug for NodeRef<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeRef")
            .field("id", &self.id)
            .field("kind", self.kind())
            .field("span", self.span())
            .field("children", &self.data().children)
            .finish()
    }
}
