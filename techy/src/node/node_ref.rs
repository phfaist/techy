//! [`NodeRef`]: the copyable read proxy over a [`NodeTree`]'s flat storage.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::fmt;

use crate::source::{Source, SourceSpan};
use crate::spec::CallableSpec;
use crate::state::{Lang, ParsingState};

use alloc::vec::Vec;

use super::arguments::{BodySlotExt, ChildRegion, ParsedArguments, ParsedSlots};
use super::kind::{CallableData, CommentData, GroupData, NodeKind};
use super::slice::NodeSlice;
use super::tree::{NodeData, NodeId, NodeTree, NO_PARENT};
use super::{NodeExt, SlotExt};

/// A reference to one node of a [`NodeTree`]: `Copy`, resolves indices, and borrows the
/// tree — the borrow checker guarantees a `NodeRef` cannot outlive the storage its index
/// points into (indices made safe by construction).
///
/// Accessors return `'t`-borrowed data (borrowing the *tree*, not this transient proxy),
/// so extracted references outlive the `NodeRef` value itself.
///
/// Kind-specific accessors are `Option`-returning on the wrong kind; preset-level sugar
/// (`as_math()`-style environment/macro views) arrives with the latexlike preset.
pub struct NodeRef<'t, L: Lang, A = ()> {
    tree: &'t NodeTree<L, A>,
    id: NodeId,
}

impl<'t, L: Lang, A> NodeRef<'t, L, A> {
    pub(crate) fn new(tree: &'t NodeTree<L, A>, id: NodeId) -> NodeRef<'t, L, A> {
        debug_assert_eq!(
            id.tree_tag(),
            tree.tree_tag(),
            "NodeId used with a tree that did not mint it \
             (ids are only meaningful for their own tree)"
        );
        NodeRef { tree, id }
    }

    fn data(&self) -> &'t NodeData<L> {
        &self.tree.nodes()[self.id.index()]
    }

    /// The tree this reference points into — the anchor for tree-level operations
    /// ([`NodeTree::node_at`](super::NodeTree::node_at),
    /// [`annotations`](super::NodeTree::annotations), …) from a node in hand.
    pub fn tree(&self) -> &'t NodeTree<L, A> {
        self.tree
    }

    /// This node's parent — `None` at the root. O(1): the tree stores its parent
    /// table (`finish()` computes it for region resolution anyway).
    ///
    /// Walking all ancestors is a one-liner, innermost first:
    /// `core::iter::successors(node.parent(), |n| n.parent())`.
    pub fn parent(&self) -> Option<NodeRef<'t, L, A>> {
        let parent = self.tree.parent_table()[self.id.index()];
        (parent != NO_PARENT).then(|| NodeRef::new(self.tree, self.tree.make_id(parent)))
    }

    /// This node's position in its parent's child list (`parent().child(i)` is this
    /// node) — `None` at the root. O(1): the own index minus the parent's
    /// children-block start.
    pub fn index_in_parent(&self) -> Option<usize> {
        let parent = self.parent()?;
        Some(self.id.index() - parent.data().children.start as usize)
    }

    /// This node's annotation (the tree's consumer-owned per-node data; see
    /// [`NodeTree`]'s annotation notes). Annotations have no setter — trees are
    /// frozen; a new annotation stage comes from
    /// [`NodeTree::annotate`](super::NodeTree::annotate).
    pub fn annotation(&self) -> &'t A {
        &self.tree.annotations[self.id.index()]
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

    /// The node's provenance span (`Arc<Source>` + byte range) — where the node came
    /// from.
    ///
    /// On a tree parsed from a language that obeys span tiling
    /// ([`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)) the span
    /// is exactly the stretch of source the node was parsed from. For a language with
    /// `OBEYS_SPAN_TILING = false` it is the span the token reader described for that
    /// stretch of the stream, and for a restaged or synthesized node it is whatever
    /// the transform recorded. What the node *says* is read from its own data, never
    /// from the span.
    pub fn span(&self) -> &'t SourceSpan<L::SourceOrigin> {
        &self.data().span
    }

    /// The text this node's [`span`](NodeRef::span) points at — level-1 verbatim
    /// recomposition: it never needs an external lookup and works for detached and
    /// mixed-origin trees.
    ///
    /// This answers the coordinates, not the node's data. The two agree on a tree
    /// parsed from a language that obeys span tiling
    /// ([`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)): the span
    /// is exactly the node's original text. For a language with
    /// `OBEYS_SPAN_TILING = false`, and on restaged or synthesized trees, they need
    /// not agree — what the node says is its recorded data
    /// ([`chars`](NodeRef::chars), [`group_delimiters`](NodeRef::group_delimiters),
    /// [`comment`](NodeRef::comment), [`callable`](NodeRef::callable)), and
    /// re-emitting a tree's spelling is [`recompose`](crate::recompose)'s job.
    pub fn span_content(&self) -> &'t str {
        self.data().span.content()
    }

    /// The parsing state this node was parsed under.
    pub fn parsing_state(&self) -> &'t Arc<ParsingState<L>> {
        &self.data().parsing_state
    }

    /// A compact one-line description of the node — `chars(ab )`, `group(Math(Inline) $ $)`,
    /// `Macro(emph)`, `comment( note)`, `list(3)` — the assertion/logging companion
    /// to the verbose `Debug` rendering (promoted from the preset's test support). Groups print their class (`?` when classless) and recorded
    /// delimiters; callables print their invocation form (`Debug`) and spelling;
    /// chars/comments print their full logical text; lists print their child count.
    ///
    /// The format is human-oriented and **not a stability contract**: it may change
    /// between releases. Within a release it is exact, and the crate's own tests
    /// pin it — a re-implementation that must agree with this crate's renderings
    /// (a binding porting the test suite) reproduces it verbatim. Compare trees
    /// structurally (kinds, spans, accessors) where exactness matters beyond a
    /// test's lifetime.
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
        } else if let Some(data) = self.comment() {
            format!("comment({})", data.content.resolve(self.source()))
        } else {
            format!("list({})", self.child_count())
        }
    }

    /// This node's own source (what `TextContent::Spanned` resolves against).
    pub(crate) fn source(&self) -> &'t Source<L::SourceOrigin> {
        self.data().span.source()
    }

    // --- children ---------------------------------------------------------------------

    /// The number of structural children.
    pub fn child_count(&self) -> usize {
        self.data().children.len()
    }

    /// The `i`-th structural child.
    pub fn child(&self, i: usize) -> Option<NodeRef<'t, L, A>> {
        let children = &self.data().children;
        let id = children.start.checked_add(u32::try_from(i).ok()?)?;
        (id < children.end).then(|| NodeRef::new(self.tree, self.tree.make_id(id)))
    }

    /// The structural children, in order, as a [`NodeSlice`] (iterable directly; use
    /// [`iter()`](NodeSlice::iter) for adaptor chains).
    pub fn children(&self) -> NodeSlice<'t, L, A> {
        NodeSlice::new(self.tree, self.data().children.clone())
    }

    /// The node's descendants in **document order** (preorder depth-first: each node
    /// before its children, siblings left to right), the node itself excluded. This is
    /// the "walk everything under here" primitive — find every `\cite`, collect every
    /// math group — that flat storage order
    /// ([`NodeTree::iter_storage_order`](super::NodeTree::iter_storage_order),
    /// breadth-first) deliberately does not provide.
    pub fn descendants(&self) -> Descendants<'t, L, A> {
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
        matches!(self.kind(), NodeKind::List)
    }

    /// A `Chars` node's logical text.
    pub fn chars(&self) -> Option<&'t str> {
        match self.kind() {
            NodeKind::Chars { content, .. } => Some(content.resolve(self.source())),
            _ => None,
        }
    }

    /// A `Comment` node's full payload ([`CommentData`]: start delimiter, content,
    /// and syntactic post-space). Span-backed payload fields resolve against the
    /// node's own source, `node.span().source()`
    /// ([`TextContent::resolve`](crate::source::TextContent::resolve)).
    pub fn comment(&self) -> Option<&'t CommentData> {
        match self.kind() {
            NodeKind::Comment(data) => Some(data),
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
            (data.open.resolve(self.source()), data.close.resolve(self.source()))
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

    /// A `Callable` node's recorded invocation-syntax payload
    /// ([`CallableData::invocation_syntax`]) — the Lang-owned trigger-spelling
    /// facts. Typed readers live with the payload type (the latexlike sugar's
    /// [`post_space`](NodeRef::post_space) reads its `Macro` arm).
    pub fn invocation_syntax(&self) -> Option<&'t L::InvocationSyntax> {
        self.callable().map(|data| &data.invocation_syntax)
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
    /// (consult [`arguments`](NodeRef::arguments) to distinguish — or use the
    /// by-name form [`argument_nodes_named`](NodeRef::argument_nodes_named), whose
    /// `Result` discriminates the category errors from a merely absent argument).
    pub fn argument_nodes(&self, i: usize) -> Option<NodeSlice<'t, L, A>> {
        self.region_nodes(self.callable()?.arguments.get(i)?.region.as_ref()?)
    }

    /// The nodes of the region of the argument named `name` (per its
    /// [`ArgumentSpec`](crate::spec::ArgumentSpec)) — [`argument_nodes`](NodeRef::argument_nodes)
    /// by name, with the by-name error contract: `Err` is the category mismatch (not
    /// a callable, or `name` is not among the spec's declared arguments — the
    /// misspelling trap), `Ok(None)` is precisely "declared but absent", `Ok(Some)`
    /// is present.
    pub fn argument_nodes_named(
        &self,
        name: &str,
    ) -> Result<Option<NodeSlice<'t, L, A>>, NamedAccessError> {
        let callable = self.callable().ok_or(NamedAccessError::NotACallable)?;
        let argument = callable.arguments.get_named(name).ok_or_else(|| {
            NamedAccessError::UnknownArgumentName { name: name.into() }
        })?;
        Ok(argument.region.as_ref().and_then(|region| self.region_nodes(region)))
    }

    /// The content nodes of argument `i`, noise and delimiters excluded: the group's
    /// children for `\textbf{abc}`, the single `Chars` node for `\frac 1 2` — exactly
    /// what the argument's parser designated, no unwrap heuristics. `None` for
    /// non-callables, out-of-range indices, and absent arguments (consult
    /// [`arguments`](NodeRef::arguments) to distinguish — or use the by-name form
    /// [`argument_content_nodes_named`](NodeRef::argument_content_nodes_named),
    /// whose `Result` discriminates the category errors from a merely absent
    /// argument).
    pub fn argument_content_nodes(&self, i: usize) -> Option<NodeSlice<'t, L, A>> {
        self.region_content(self.callable()?.arguments.get(i)?.region.as_ref()?)
    }

    /// The content nodes of the argument named `name` —
    /// [`argument_content_nodes`](NodeRef::argument_content_nodes) by name, with
    /// the by-name error contract of
    /// [`argument_nodes_named`](NodeRef::argument_nodes_named): `Err` = category
    /// mismatch, `Ok(None)` = declared but absent, `Ok(Some)` = present.
    pub fn argument_content_nodes_named(
        &self,
        name: &str,
    ) -> Result<Option<NodeSlice<'t, L, A>>, NamedAccessError> {
        let callable = self.callable().ok_or(NamedAccessError::NotACallable)?;
        let argument = callable.arguments.get_named(name).ok_or_else(|| {
            NamedAccessError::UnknownArgumentName { name: name.into() }
        })?;
        Ok(argument.region.as_ref().and_then(|region| self.region_content(region)))
    }

    fn region_nodes(&self, region: &ChildRegion) -> Option<NodeSlice<'t, L, A>> {
        Some(NodeSlice::new(self.tree, region.children()))
    }

    fn region_content(&self, region: &ChildRegion) -> Option<NodeSlice<'t, L, A>> {
        Some(NodeSlice::new(self.tree, region.content_range()))
    }

    /// The node whose child list holds slot `i`'s content — the body `List` of the
    /// standard environment shape — when this is a callable with such a slot. `None`
    /// when the slot's content sits directly among the callable's own children
    /// ([`ContentNodes::InRegion`](super::ContentNodes) designations), where no such
    /// wrapper node exists — returning the callable itself would send naive recursive
    /// walkers into a loop. The shipped producer of that wrapperless shape is
    /// [`input_macro_spec`](crate::latexlike::input_macro_spec)'s `attached` slot:
    /// the included content's nodes sit directly among the `\input` callable's
    /// children (reaching it takes a
    /// [`SourceResolver`](crate::source::SourceResolver) on the driver). Content
    /// access that works for *both* shapes is
    /// [`slot_content_nodes`](NodeRef::slot_content_nodes) / [`body`](NodeRef::body).
    pub fn slot_content_parent(&self, i: usize) -> Option<NodeRef<'t, L, A>> {
        let region = &self.callable()?.slots.get(i)?.region;
        let parent = region.content_parent();
        (parent != self.id).then(|| NodeRef::new(self.tree, parent))
    }

    /// The content nodes of slot `i` (the body nodes, for the standard environment
    /// shape), in source order. `None` for non-callables and out-of-range indices
    /// (the by-name form
    /// [`slot_content_nodes_named`](NodeRef::slot_content_nodes_named) turns those
    /// category mismatches into errors).
    pub fn slot_content_nodes(&self, i: usize) -> Option<NodeSlice<'t, L, A>> {
        self.region_content(&self.callable()?.slots.get(i)?.region)
    }

    /// The content nodes of the slot named `name` (per its
    /// [`ParsedSlot`](super::ParsedSlot) record) —
    /// [`slot_content_nodes`](NodeRef::slot_content_nodes) by name, with the
    /// by-name error contract: `Err` is the category mismatch (not a callable, or
    /// no slot named `name` — the misspelling trap). Unlike the argument family
    /// there is no `Option` layer: a recorded slot always has content nodes
    /// (possibly zero of them — an `Ok` slice may be empty), with no
    /// "declared but absent" state.
    pub fn slot_content_nodes_named(
        &self,
        name: &str,
    ) -> Result<NodeSlice<'t, L, A>, NamedAccessError> {
        let callable = self.callable().ok_or(NamedAccessError::NotACallable)?;
        let slot = callable
            .slots
            .get_named(name)
            .ok_or_else(|| NamedAccessError::UnknownSlotName { name: name.into() })?;
        Ok(NodeSlice::new(self.tree, slot.region.content_range()))
    }

    /// The content nodes of the **body slot** — the *body* of environment-shaped
    /// callables, in source order: the first slot whose ext reports
    /// [`is_body()`](BodySlotExt::is_body) (available where the language's
    /// [`SlotExt`](crate::state::NodeExtTypes::SlotExt) implements [`BodySlotExt`];
    /// for no-ext languages every slot reports body, so this is the first slot).
    ///
    /// The selection is the **ext axis alone** — deliberately no conjunction with the
    /// slot's [`SlotRole`](super::SlotRole): a framework marking an
    /// `Attached` or `Hidden` slot as its body must not find it silently
    /// unlocatable here.
    ///
    /// The body's wrapper node, when one exists, is
    /// [`slot_content_parent`](NodeRef::slot_content_parent) at the same slot's index.
    pub fn body(&self) -> Option<NodeSlice<'t, L, A>>
    where
        SlotExt<L>: BodySlotExt,
    {
        let slot = self.callable()?.slots.iter().find(|slot| slot.ext.is_body())?;
        self.region_content(&slot.region)
    }
}

/// Error of the by-name accessors ([`NodeRef::argument_nodes_named`],
/// [`NodeRef::argument_content_nodes_named`],
/// [`NodeRef::slot_content_nodes_named`]): a **category mismatch** — the node is
/// not a callable, or the name matches nothing declared.
///
/// Deliberately an error rather than a silent `None`: for a *name*, `None` on a
/// typo has no cheap call-site discriminator — and names, unlike indices, are
/// exactly the access form the API recommends. `Ok(None)` stays reserved for the
/// argument family's "declared but absent". Never a panic: this family is the
/// non-panicking companion shape by design.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NamedAccessError {
    /// The node is not a `Callable` — it has no arguments or slots to access.
    NotACallable,
    /// The name matches none of the callable's declared arguments (the names on
    /// its spec's [`ArgumentSpec`](crate::spec::ArgumentSpec)s).
    UnknownArgumentName {
        /// The unmatched name, as queried.
        name: Box<str>,
    },
    /// The name matches none of the callable's recorded slots
    /// ([`ParsedSlot`](super::ParsedSlot) names).
    UnknownSlotName {
        /// The unmatched name, as queried.
        name: Box<str>,
    },
}

impl fmt::Display for NamedAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NamedAccessError::NotACallable => {
                write!(f, "the node is not a callable (no arguments or slots)")
            }
            NamedAccessError::UnknownArgumentName { name } => {
                write!(f, "no argument named ‘{name}’ among the declared arguments")
            }
            NamedAccessError::UnknownSlotName { name } => {
                write!(f, "no slot named ‘{name}’ on the callable")
            }
        }
    }
}

impl core::error::Error for NamedAccessError {}

/// Iterator over a node's descendants in document order (see
/// [`NodeRef::descendants`]; preorder depth-first via an explicit stack).
pub struct Descendants<'t, L: Lang, A = ()> {
    tree: &'t NodeTree<L, A>,
    /// Nodes not yet yielded, next on top; a yielded node's children are pushed in
    /// reverse so the leftmost comes off first.
    stack: Vec<u32>,
}

fn push_children_reversed<L: Lang>(stack: &mut Vec<u32>, data: &NodeData<L>) {
    for i in data.children.clone().rev() {
        stack.push(i);
    }
}

impl<'t, L: Lang, A> Iterator for Descendants<'t, L, A> {
    type Item = NodeRef<'t, L, A>;

    fn next(&mut self) -> Option<NodeRef<'t, L, A>> {
        let index = self.stack.pop()?;
        let node = self.tree.node(self.tree.make_id(index));
        push_children_reversed(&mut self.stack, &self.tree.nodes()[index as usize]);
        Some(node)
    }
}

impl<L: Lang, A> fmt::Debug for Descendants<'_, L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Descendants").field("pending", &self.stack.len()).finish()
    }
}

// Manual impls: `NodeRef` is Copy regardless of `L` (it stores only a borrow and an id).

impl<L: Lang, A> Clone for NodeRef<'_, L, A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang, A> Copy for NodeRef<'_, L, A> {}

impl<L: Lang, A> fmt::Debug for NodeRef<'_, L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeRef")
            .field("id", &self.id)
            .field("kind", self.kind())
            .field("span", self.span())
            .field("children", &self.data().children)
            .finish()
    }
}
