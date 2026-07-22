//! [`NodeRef`]: the copyable read proxy over a [`NodeTree`]'s flat storage.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::fmt;

use crate::source::SourceSpan;
use crate::spec::CallableSpec;
use crate::state::{Lang, ParsingState};

use alloc::vec::Vec;

use super::arguments::{ChildRegion, ParsedArguments, ParsedSlots};
use super::kind::{CallableData, GroupData, NodeKind};
use super::slice::NodeSlice;
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
        debug_assert_eq!(
            id.tree_tag(),
            tree.tree_tag(),
            "NodeId used with a tree that did not mint it \
             (ids are only meaningful for their own tree)"
        );
        NodeRef { tree, id }
    }

    fn data(&self) -> &'t NodeData<L> {
        &self.tree.nodes[self.id.index()]
    }

    /// The tree this reference points into (in-crate: copy/extract machinery).
    pub(crate) fn tree(&self) -> &'t NodeTree<L> {
        self.tree
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

    /// A compact one-line description of the node — `chars(ab )`, `group(Math $ $)`,
    /// `Macro(emph)`, `comment( note)`, `list(3)` — the assertion/logging companion
    /// to the verbose `Debug` rendering (promoted from the preset's test support,
    /// Phase 7.9). Groups print their class (`?` when classless) and recorded
    /// delimiters; callables print their invocation form (`Debug`) and spelling;
    /// chars/comments print their full logical text; lists print their child count.
    ///
    /// The format is human-oriented and **not a stability contract** — compare trees
    /// structurally (kinds, spans, accessors) where exactness matters beyond a test's
    /// lifetime.
    pub fn summary(&self) -> String {
        if let Some(text) = self.chars() {
            format!("chars({text})")
        } else if self.is_group() {
            let class = self
                .group_type()
                .map_or_else(|| "?".to_string(), |group_type| format!("{group_type:?}"));
            let (open, close) = self.group_delimiters().unwrap_or(("", ""));
            format!("group({class} {open} {close})")
        } else if let Some(data) = self.callable() {
            format!("{:?}({})", data.callable_type, data.name)
        } else if let Some(text) = self.comment() {
            format!("comment({text})")
        } else {
            format!("list({})", self.child_count())
        }
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
        let id = children.start.checked_add(u32::try_from(i).ok()?)?;
        (id < children.end).then(|| NodeRef::new(self.tree, self.tree.make_id(id)))
    }

    /// The structural children, in order, as a [`NodeSlice`] (iterable directly; use
    /// [`iter()`](NodeSlice::iter) for adaptor chains).
    pub fn children(&self) -> NodeSlice<'t, L> {
        NodeSlice::new(self.tree, self.data().children.clone())
    }

    /// The node's descendants in **document order** (preorder depth-first: each node
    /// before its children, siblings left to right), the node itself excluded. This is
    /// the "walk everything under here" primitive — find every `\cite`, collect every
    /// math group — that flat storage order
    /// ([`NodeTree::iter_storage_order`](super::NodeTree::iter_storage_order),
    /// breadth-first) deliberately does not provide.
    pub fn descendants(&self) -> Descendants<'t, L> {
        let mut stack = Vec::new();
        push_children_reversed(&mut stack, self.data());
        Descendants { tree: self.tree, stack }
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

    /// A `Comment` node's start delimiter, as logical text (`%` in LaTeX — records which
    /// comment syntax fired).
    pub fn comment_start(&self) -> Option<&'t str> {
        match self.kind() {
            NodeKind::Comment { start, .. } => Some(start.resolve(self.source_content())),
            _ => None,
        }
    }

    /// A `Comment` node's syntactic post-space (terminating newline plus following
    /// indentation; empty at end of input or before a paragraph break), as logical text.
    pub fn comment_post_space(&self) -> Option<&'t str> {
        match self.kind() {
            NodeKind::Comment { post_space, .. } => {
                Some(post_space.resolve(self.source_content()))
            }
            _ => None,
        }
    }

    /// A `Group` node's full payload (delimiters, group class, ext).
    pub fn group(&self) -> Option<&'t GroupData<L>> {
        match self.kind() {
            NodeKind::Group(data) => Some(data),
            _ => None,
        }
    }

    /// A `Group` node's class. `None` for non-groups *and* for synthesized groups
    /// without a language group class — consult [`group()`](NodeRef::group) to
    /// distinguish.
    pub fn group_type(&self) -> Option<L::GroupTypeId> {
        self.group().and_then(|data| data.group_type)
    }

    /// A `Group` node's delimiters, as logical text.
    pub fn group_delimiters(&self) -> Option<(&'t str, &'t str)> {
        self.group().map(|data| {
            (data.open.resolve(self.source_content()), data.close.resolve(self.source_content()))
        })
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
    pub fn callable_type(&self) -> Option<L::CallableTypeId> {
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

    /// A `Callable` node's parsed-arguments record.
    pub fn arguments(&self) -> Option<&'t ParsedArguments<L>> {
        self.callable().map(|data| &data.arguments)
    }

    /// A `Callable` node's parsed-slots record.
    pub fn slots(&self) -> Option<&'t ParsedSlots<L>> {
        self.callable().map(|data| &data.slots)
    }

    /// The nodes of argument `i`'s region — the argument's full syntactic extent
    /// (leading comment/whitespace noise plus the syntax-bearing nodes), in source
    /// order. `None` for non-callables, out-of-range indices, and absent arguments
    /// (consult [`arguments`](NodeRef::arguments) to distinguish).
    pub fn argument_nodes(&self, i: usize) -> Option<NodeSlice<'t, L>> {
        self.region_nodes(self.callable()?.arguments.get(i)?.region.as_ref()?)
    }

    /// The nodes of the region of the argument named `name` (per its
    /// [`ArgumentSpec`](crate::spec::ArgumentSpec)) — [`argument_nodes`](NodeRef::argument_nodes)
    /// by name.
    pub fn argument_nodes_named(&self, name: &str) -> Option<NodeSlice<'t, L>> {
        self.region_nodes(self.callable()?.arguments.get_named(name)?.region.as_ref()?)
    }

    /// The content nodes of argument `i`, noise and delimiters excluded: the group's
    /// children for `\textbf{abc}`, the single `Chars` node for `\frac 1 2` — exactly
    /// what the argument's parser designated, no unwrap heuristics.
    pub fn argument_content_nodes(&self, i: usize) -> Option<NodeSlice<'t, L>> {
        self.region_content(self.callable()?.arguments.get(i)?.region.as_ref()?)
    }

    /// The content nodes of the argument named `name` —
    /// [`argument_content_nodes`](NodeRef::argument_content_nodes) by name.
    pub fn argument_content_nodes_named(&self, name: &str) -> Option<NodeSlice<'t, L>> {
        self.region_content(self.callable()?.arguments.get_named(name)?.region.as_ref()?)
    }

    fn region_nodes(&self, region: &ChildRegion) -> Option<NodeSlice<'t, L>> {
        Some(NodeSlice::new(self.tree, region.children()))
    }

    fn region_content(&self, region: &ChildRegion) -> Option<NodeSlice<'t, L>> {
        Some(NodeSlice::new(self.tree, region.content_range()))
    }

    /// The node whose child list holds slot `i`'s content — the body `List` of the
    /// standard environment shape — when this is a callable with such a slot. `None`
    /// when the slot's content sits directly among the callable's own children
    /// ([`ContentNodes::InRegion`](super::ContentNodes) designations), where no such
    /// wrapper node exists — returning the callable itself would send naive recursive
    /// walkers into a loop. Content access that works for *both* shapes is
    /// [`slot_content_nodes`](NodeRef::slot_content_nodes) / [`body`](NodeRef::body).
    pub fn slot_content_parent(&self, i: usize) -> Option<NodeRef<'t, L>> {
        let region = &self.callable()?.slots.get(i)?.region;
        let parent = region.content_parent();
        (parent != self.id).then(|| NodeRef::new(self.tree, parent))
    }

    /// The content nodes of slot `i` (the body nodes, for the standard environment
    /// shape), in source order.
    pub fn slot_content_nodes(&self, i: usize) -> Option<NodeSlice<'t, L>> {
        self.region_content(&self.callable()?.slots.get(i)?.region)
    }

    /// The content nodes of the slot named `name` (per its
    /// [`ParsedSlot`](super::ParsedSlot) record) —
    /// [`slot_content_nodes`](NodeRef::slot_content_nodes) by name.
    pub fn slot_content_nodes_named(&self, name: &str) -> Option<NodeSlice<'t, L>> {
        self.region_content(&self.callable()?.slots.get_named(name)?.region)
    }

    /// The content nodes of the first slot — the *body* of environment-shaped callables
    /// (which have exactly one slot), in source order. Sugar for
    /// [`slot_content_nodes(0)`](NodeRef::slot_content_nodes); the body `List` node
    /// itself, when one exists, is [`slot_content_parent(0)`](NodeRef::slot_content_parent).
    pub fn body(&self) -> Option<NodeSlice<'t, L>> {
        self.slot_content_nodes(0)
    }
}

/// Iterator over a node's descendants in document order (see
/// [`NodeRef::descendants`]; preorder depth-first via an explicit stack).
pub struct Descendants<'t, L: Lang> {
    tree: &'t NodeTree<L>,
    /// Nodes not yet yielded, next on top; a yielded node's children are pushed in
    /// reverse so the leftmost comes off first.
    stack: Vec<u32>,
}

fn push_children_reversed<L: Lang>(stack: &mut Vec<u32>, data: &NodeData<L>) {
    for i in data.children.clone().rev() {
        stack.push(i);
    }
}

impl<'t, L: Lang> Iterator for Descendants<'t, L> {
    type Item = NodeRef<'t, L>;

    fn next(&mut self) -> Option<NodeRef<'t, L>> {
        let index = self.stack.pop()?;
        let node = self.tree.node(self.tree.make_id(index));
        push_children_reversed(&mut self.stack, &self.tree.nodes[index as usize]);
        Some(node)
    }
}

impl<L: Lang> fmt::Debug for Descendants<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Descendants").field("pending", &self.stack.len()).finish()
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
