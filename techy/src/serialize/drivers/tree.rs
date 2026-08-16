//! The tree driver: [`TreeSerdeDriver`], the driver of the trees table, and
//! [`TreeIndex`], its position type; the annotation-codec registration
//! ([`TableHandle::register_annotation`]); the [`TreeSerialization`] extension trait
//! (`serialize_tree` / `tree`); and the context-aware value conversions of the core
//! payload types a language's own codecs reuse ([`TextContent`], [`Span`],
//! [`SlotRole`], [`GroupRule`]).
//!
//! A node tree is rebuilt through the node builder: the reader stages the nodes in
//! reverse storage order (so a node's children exist before it), re-resolves each
//! callable's argument and slot regions from the builder-ready form the wire stores,
//! and [`finish`](crate::node::NodeTreeBuilder::finish)es the tree — which mints a
//! fresh layout tag, recomputes the parent table, and re-establishes every region
//! invariant by construction. Everything read is untrusted input: a children range
//! out of bounds, a region that does not tile the child list, a content parent
//! outside its region, a span out of bounds, a reference into the wrong table — each
//! is an error naming the node, never a panic.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
use core::fmt::{self, Debug};

use hashbrown::HashMap;

use crate::node::{
    validate_tree, ArgumentExt, BuildId, CallableData, ChildRegion, CommentData, ContentNodes,
    GroupData, NodeExt, NodeKind, NodeTree, NodeTreeBuilder, ParsedArgument, ParsedArguments,
    ParsedSlot, ParsedSlots, SlotExt, SlotRole,
};
use crate::source::{Span, TextContent};
use crate::spec::CallableSpec;
use crate::state::Lang;
use crate::token::GroupRule;

use super::super::engine::{
    DeserializeContext, ObjectSerdeDriver, SerdeSession, SerializeContext, TableHandle, TableRegistry,
};
use super::super::error::{DeserializeError, RegistrationError, SerializeError};
use super::super::object::{DeserializableValue, SerializableLang, SerializableValue};
use super::super::value::{SerialEntry, SerialValue};
use super::super::wire::state::WireGroupRule;
use super::super::wire::tree::{
    WireArgument, WireNode, WireNodeKind, WireRange, WireRegion, WireSlot, WireTree,
};
use super::super::wire::{FromSerialValue, ToSerialValue};
use super::source::{deserialize_span, serialize_span};
use super::standard::{StandardTableInterning, StandardTableReading};
use super::{CORE_TREE_IDENTIFIER, TREES_TABLE};

crate::serial_index! {
    /// A position in the trees table — the `Index` type of [`TreeSerdeDriver`]: the
    /// serialized reference to a [`NodeTree`]. Interning a tree returns a fresh
    /// position every time (a tree is a value, written in full — see
    /// [`TreeSerialization::serialize_tree`]).
    pub struct TreeIndex;
}

/// The driver of the trees table (table name `trees`): how a [`NodeTree`] is
/// serialized and rebuilt.
///
/// The trees table is *heterogeneous* by the tree's **annotation type**: a
/// `NodeTree<L, A>` is written under the identifier of the codec registered for `A`,
/// and read back through it. The unit annotation (`NodeTree<L>`, the parser's output)
/// is pre-registered under the identifier `core.tree`, and its annotations are omitted
/// from the wire; any other annotation type is registered on the table's handle with
/// [`register_annotation`](TableHandle::register_annotation), which serializes each
/// node's annotation through the type's own
/// [`SerializableValue`]/[`DeserializableValue`] conversions (so an annotation that
/// refers to a table object — a [`SourceSpan`](crate::source::SourceSpan), say —
/// interns it like any other value). Serializing a tree whose annotation type is not
/// registered is a [`SerializeError`].
///
/// The everyday spellings are the [`TreeSerialization`] extension trait's
/// `serialize_tree` and `tree`. Registered by
/// [`SerdeSession::new`](crate::serialize::SerdeSession::new).
pub struct TreeSerdeDriver<L: SerializableLang> {
    lang: core::marker::PhantomData<fn() -> L>,
}

impl<L: SerializableLang> TreeSerdeDriver<L> {
    /// The driver (it has no configuration).
    pub fn new() -> TreeSerdeDriver<L> {
        TreeSerdeDriver { lang: core::marker::PhantomData }
    }
}

impl<L: SerializableLang> Default for TreeSerdeDriver<L> {
    fn default() -> Self {
        TreeSerdeDriver::new()
    }
}

impl<L: SerializableLang> fmt::Debug for TreeSerdeDriver<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TreeSerdeDriver").finish()
    }
}

impl<L: SerializableLang> ObjectSerdeDriver<L> for TreeSerdeDriver<L> {
    type Object = dyn Any + Send + Sync;
    type Index = TreeIndex;

    fn table_name(&self) -> &'static str {
        TREES_TABLE
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        None
    }

    fn serialize_object(
        &self,
        object: &Arc<dyn Any + Send + Sync>,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError> {
        let type_id = (**object).type_id();
        let codec = codec_by_type::<L>(cx, type_id)?;
        let data = (codec.serialize)(object, cx)?;
        Ok(SerialEntry { identifier: codec.identifier.clone(), data })
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<dyn Any + Send + Sync>, DeserializeError> {
        let codec = codec_by_identifier::<L>(cx, &entry.identifier)?;
        (codec.deserialize)(&entry.data, cx)
    }
}

// --- the annotation codecs ------------------------------------------------------------

/// The serialize and deserialize routines for trees of one annotation type, keyed in
/// the trees table's registry by the type and by its identifier.
struct TreeCodec<L: SerializableLang> {
    identifier: Cow<'static, str>,
    serialize: Arc<SerializeTreeFn<L>>,
    deserialize: Arc<DeserializeTreeFn<L>>,
}

type SerializeTreeFn<L> = dyn Fn(&Arc<dyn Any + Send + Sync>, &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    + Send
    + Sync;
type DeserializeTreeFn<L> = dyn Fn(&SerialValue, &mut DeserializeContext<'_, L>) -> Result<Arc<dyn Any + Send + Sync>, DeserializeError>
    + Send
    + Sync;

impl<L: SerializableLang> Clone for TreeCodec<L> {
    fn clone(&self) -> Self {
        TreeCodec {
            identifier: self.identifier.clone(),
            serialize: Arc::clone(&self.serialize),
            deserialize: Arc::clone(&self.deserialize),
        }
    }
}

/// The trees table's registry: the codecs by annotation type (for writing) and by
/// identifier (for reading).
struct TreeRegistry<L: SerializableLang> {
    by_type: HashMap<TypeId, TreeCodec<L>>,
    by_identifier: HashMap<String, TreeCodec<L>>,
}

impl<L: SerializableLang> Default for TreeRegistry<L> {
    fn default() -> Self {
        TreeRegistry { by_type: HashMap::new(), by_identifier: HashMap::new() }
    }
}

impl<L: SerializableLang> TableRegistry for TreeRegistry<L> {
    fn as_any_mut(&mut self) -> &mut (dyn Any + Send + Sync) {
        self
    }

    fn forget_memo(&mut self, _identifier: &str) {
        // The trees table memoizes nothing.
    }
}

/// The codec for the annotation type of the tree stored in `object`, in the session
/// `cx` belongs to. Cloned out so the session is free while it runs.
fn codec_by_type<L: SerializableLang>(
    cx: &mut SerializeContext<'_, L>,
    type_id: TypeId,
) -> Result<TreeCodec<L>, SerializeError> {
    let ordinal = cx
        .session_mut()
        .table_ordinal_by_name(TREES_TABLE)
        .ok_or_else(|| SerializeError::UnknownTableName { name: TREES_TABLE.to_string() })?;
    let registry = cx
        .session_mut()
        .registry_mut::<TreeRegistry<L>>(ordinal)
        .ok_or_else(|| SerializeError::failed("the trees table registry is of the wrong type"))?;
    registry.by_type.get(&type_id).cloned().ok_or_else(|| {
        SerializeError::failed(
            "this tree's annotation type is not registered for serialization; register it \
             with TableHandle::register_annotation",
        )
    })
}

/// The codec for `identifier` in the trees table of the session `cx` belongs to.
fn codec_by_identifier<L: SerializableLang>(
    cx: &mut DeserializeContext<'_, L>,
    identifier: &str,
) -> Result<TreeCodec<L>, DeserializeError> {
    let ordinal = cx.session_mut().table_ordinal_by_name(TREES_TABLE).ok_or_else(|| {
        DeserializeError::failed("the tree driver was called with a context of a session that has no trees table")
    })?;
    let registry = cx
        .session_mut()
        .registry_mut::<TreeRegistry<L>>(ordinal)
        .ok_or_else(|| DeserializeError::failed("the trees table registry is of the wrong type"))?;
    registry.by_identifier.get(identifier).cloned().ok_or_else(|| DeserializeError::UnknownIdentifier {
        table: TREES_TABLE,
        identifier: identifier.to_string(),
    })
}

/// The codec of the unit annotation (`core.tree`): the annotations are omitted from
/// the wire and rebuilt as one `()` per node.
fn unit_tree_codec<L: SerializableLang>() -> TreeCodec<L> {
    TreeCodec {
        identifier: Cow::Borrowed(CORE_TREE_IDENTIFIER),
        serialize: Arc::new(|object, cx| {
            let tree = downcast_tree::<L, ()>(object)?;
            let nodes = serialize_nodes(tree, cx)?;
            Ok(WireTree { nodes, annotations: None }.to_serial_value()?)
        }),
        deserialize: Arc::new(|data, cx| {
            let wire = WireTree::from_serial_value(data)?;
            let annotations = alloc::vec![(); wire.nodes.len()];
            let tree = rebuild_tree::<L, ()>(wire.nodes, annotations, cx)?;
            Ok(Arc::new(tree) as Arc<dyn Any + Send + Sync>)
        }),
    }
}

/// The codec of an annotation type `A`: each node's annotation is serialized through
/// `A`'s own value conversion, one wire value per node.
fn value_tree_codec<L, A>(identifier: Cow<'static, str>) -> TreeCodec<L>
where
    L: SerializableLang,
    A: SerializableValue<L> + DeserializableValue<L> + Clone + Debug + Send + Sync + 'static,
{
    TreeCodec {
        identifier,
        serialize: Arc::new(|object, cx| {
            let tree = downcast_tree::<L, A>(object)?;
            let nodes = serialize_nodes(tree, cx)?;
            let annotations = tree
                .annotations()
                .iter()
                .map(|annotation| annotation.serialize_value(cx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(WireTree { nodes, annotations: Some(annotations) }.to_serial_value()?)
        }),
        deserialize: Arc::new(|data, cx| {
            let mut wire = WireTree::from_serial_value(data)?;
            let node_count = wire.nodes.len();
            let list = wire.annotations.take().ok_or_else(|| {
                DeserializeError::failed(
                    "the tree entry carries no annotations, but its annotation type is not the unit type",
                )
            })?;
            if list.len() != node_count {
                return Err(DeserializeError::failed(alloc::format!(
                    "the tree has {node_count} nodes but {} annotations",
                    list.len()
                )));
            }
            let annotations = list
                .iter()
                .map(|value| <A as DeserializableValue<L>>::deserialize_value(value, cx))
                .collect::<Result<Vec<A>, _>>()?;
            let tree = rebuild_tree::<L, A>(wire.nodes, annotations, cx)?;
            Ok(Arc::new(tree) as Arc<dyn Any + Send + Sync>)
        }),
    }
}

/// The `NodeTree<L, A>` behind an erased tree object, or a serialize error (never
/// reached in practice: the codec is chosen by the object's own type).
fn downcast_tree<L: SerializableLang, A: 'static>(
    object: &Arc<dyn Any + Send + Sync>,
) -> Result<&NodeTree<L, A>, SerializeError> {
    (**object)
        .downcast_ref::<NodeTree<L, A>>()
        .ok_or_else(|| SerializeError::failed("the tree codec was called on a tree of another annotation type"))
}

// --- registration ---------------------------------------------------------------------

impl<L: SerializableLang> TableHandle<TreeSerdeDriver<L>> {
    /// Register the codec for tree annotation type `A` under `identifier` in this
    /// trees table of `session`: `NodeTree<L, A>` values are then serialized and read
    /// back through `A`'s own [`SerializableValue`] / [`DeserializableValue`]
    /// conversions (one wire value per node). The unit annotation (`NodeTree<L>`) is
    /// pre-registered under `core.tree` and needs no registration.
    ///
    /// # Errors
    ///
    /// [`RegistrationError::UnknownTable`] when the handle is not one of `session`'s;
    /// [`RegistrationError::DuplicateIdentifier`] when `identifier` is already
    /// registered in the table; [`RegistrationError::DuplicateAnnotationType`] when a
    /// codec for `A` is already registered.
    pub fn register_annotation<A>(
        self,
        session: &mut SerdeSession<L>,
        identifier: impl Into<Cow<'static, str>>,
    ) -> Result<(), RegistrationError>
    where
        A: SerializableValue<L> + DeserializableValue<L> + Clone + Debug + Send + Sync + 'static,
    {
        register_codec::<L>(self, session, TypeId::of::<NodeTree<L, A>>(), value_tree_codec::<L, A>(identifier.into()))
    }

    /// Pre-register the unit annotation codec (`core.tree`) — called by
    /// [`SerdeSession::new`](crate::serialize::SerdeSession::new).
    pub(crate) fn register_core_tree(self, session: &mut SerdeSession<L>) -> Result<(), RegistrationError> {
        register_codec::<L>(self, session, TypeId::of::<NodeTree<L, ()>>(), unit_tree_codec::<L>())
    }
}

fn register_codec<L: SerializableLang>(
    handle: TableHandle<TreeSerdeDriver<L>>,
    session: &mut SerdeSession<L>,
    type_id: TypeId,
    codec: TreeCodec<L>,
) -> Result<(), RegistrationError> {
    let ordinal = session.table_index(handle).map_err(|table| RegistrationError::UnknownTable { table })?;
    let table = session.table_name(ordinal);
    let registry = session
        .registry_mut::<TreeRegistry<L>>(ordinal)
        .ok_or(RegistrationError::UnknownTable { table: handle.id() })?;
    if registry.by_identifier.contains_key(&*codec.identifier) {
        return Err(RegistrationError::DuplicateIdentifier { table, identifier: codec.identifier.to_string() });
    }
    if registry.by_type.contains_key(&type_id) {
        return Err(RegistrationError::DuplicateAnnotationType { table });
    }
    registry.by_type.insert(type_id, codec.clone());
    registry.by_identifier.insert(codec.identifier.to_string(), codec);
    Ok(())
}

// --- the sugar -------------------------------------------------------------------------

/// Serializing a node tree into and reading one back from a session's trees table by
/// kind — `serialize_tree` and `tree` — on a [`SerdeSession`]: the everyday spellings
/// over the general [`SerdeSession::intern`](crate::serialize::SerdeSession::intern) /
/// [`SerdeSession::object`](crate::serialize::SerdeSession::object) with the trees
/// table handle.
///
/// An extension trait: bring it into scope with `use techy::serialize::TreeSerialization;`.
pub trait TreeSerialization<L: SerializableLang> {
    /// Serialize `tree` into the trees table, returning its position. Every call is a
    /// new entry: a node tree is a value, written in full, so two calls with equal
    /// trees produce two entries (unlike an interned source or state, which is written
    /// once and shared). The tree's annotation type must be registered
    /// ([`TableHandle::register_annotation`]; the unit annotation is pre-registered).
    ///
    /// # Errors
    ///
    /// The session has no trees table ([`SerializeError::UnknownTableName`]); the
    /// annotation type is not registered, or a node's serialization fails
    /// ([`SerializeError`], wrapped in [`SerializeError::InTable`]).
    fn serialize_tree<A>(&mut self, tree: &NodeTree<L, A>) -> Result<TreeIndex, SerializeError>
    where
        A: SerializableValue<L> + DeserializableValue<L> + Clone + Debug + Send + Sync + 'static;

    /// The tree at `position` of the trees table, rebuilt with annotation type `A`.
    ///
    /// # Errors
    ///
    /// The session has no trees table ([`DeserializeError::UnknownTableName`]); the
    /// errors of [`SerdeSession::object`](crate::serialize::SerdeSession::object); the
    /// tree at that position was serialized with a different annotation type than `A`
    /// ([`DeserializeError::Failed`]).
    fn tree<A>(&mut self, position: TreeIndex) -> Result<NodeTree<L, A>, DeserializeError>
    where
        A: SerializableValue<L> + DeserializableValue<L> + Clone + Debug + Send + Sync + 'static;
}

impl<L: SerializableLang> TreeSerialization<L> for SerdeSession<L> {
    fn serialize_tree<A>(&mut self, tree: &NodeTree<L, A>) -> Result<TreeIndex, SerializeError>
    where
        A: SerializableValue<L> + DeserializableValue<L> + Clone + Debug + Send + Sync + 'static,
    {
        let handle = self
            .table_handle::<TreeSerdeDriver<L>>(TREES_TABLE)
            .ok_or_else(|| SerializeError::UnknownTableName { name: TREES_TABLE.to_string() })?;
        let object: Arc<dyn Any + Send + Sync> = Arc::new(tree.clone());
        self.intern(handle, &object)
    }

    fn tree<A>(&mut self, position: TreeIndex) -> Result<NodeTree<L, A>, DeserializeError>
    where
        A: SerializableValue<L> + DeserializableValue<L> + Clone + Debug + Send + Sync + 'static,
    {
        let handle = self
            .table_handle::<TreeSerdeDriver<L>>(TREES_TABLE)
            .ok_or_else(|| DeserializeError::UnknownTableName { name: TREES_TABLE.to_string() })?;
        let object = self.object(handle, position)?;
        match object.downcast::<NodeTree<L, A>>() {
            Ok(tree) => Ok((*tree).clone()),
            Err(_) => Err(DeserializeError::failed(
                "the tree at this position was serialized with a different annotation type than the one requested",
            )),
        }
    }
}

// --- writing ---------------------------------------------------------------------------

/// The nodes of `tree` in storage order — annotation-blind (the annotations are
/// serialized by the codec).
fn serialize_nodes<L: SerializableLang, A>(
    tree: &NodeTree<L, A>,
    cx: &mut SerializeContext<'_, L>,
) -> Result<Vec<WireNode>, SerializeError> {
    let nodes = tree.nodes();
    nodes
        .iter()
        .enumerate()
        .map(|(index, data)| serialize_node(index as u32, data, nodes, cx))
        .collect()
}

fn serialize_node<L: SerializableLang>(
    index: u32,
    data: &crate::node::NodeData<L>,
    nodes: &[crate::node::NodeData<L>],
    cx: &mut SerializeContext<'_, L>,
) -> Result<WireNode, SerializeError> {
    let kind = serialize_kind(index, data, nodes, cx)?;
    let span = serialize_span(&data.span, cx)?;
    let state = cx.intern_state(&data.parsing_state)?;
    let ext = data.ext.serialize_value(cx)?;
    let children = WireRange { start: data.children.start, end: data.children.end };
    Ok(WireNode { kind, span, state, ext, children })
}

fn serialize_kind<L: SerializableLang>(
    index: u32,
    data: &crate::node::NodeData<L>,
    nodes: &[crate::node::NodeData<L>],
    cx: &mut SerializeContext<'_, L>,
) -> Result<WireNodeKind, SerializeError> {
    match &data.kind {
        NodeKind::Chars { content } => Ok(WireNodeKind::Chars { content: content.clone() }),
        NodeKind::Group(group) => Ok(WireNodeKind::Group {
            group_type: group.group_type.serialize_value(cx)?,
            open: group.open.clone(),
            close: group.close.clone(),
        }),
        NodeKind::Comment(comment) => Ok(WireNodeKind::Comment {
            start: comment.start.clone(),
            content: comment.content.clone(),
            post_space: comment.post_space.clone(),
        }),
        NodeKind::List => Ok(WireNodeKind::List),
        NodeKind::Callable(callable) => {
            let children_start = data.children.start;
            let arguments = callable
                .arguments
                .iter()
                .enumerate()
                .map(|(arg_index, argument)| {
                    serialize_argument(arg_index, argument, &callable.spec, index, children_start, nodes, cx)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let slots = callable
                .slots
                .iter()
                .map(|slot| serialize_slot(slot, index, children_start, nodes, cx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(WireNodeKind::Callable {
                callable_type: callable.callable_type.serialize_value(cx)?,
                name: callable.name.to_string(),
                spec: cx.intern_spec(&callable.spec)?,
                arguments,
                slots,
                invocation_syntax: callable.invocation_syntax.serialize_value(cx)?,
            })
        }
    }
}

fn serialize_argument<L: SerializableLang>(
    arg_index: usize,
    argument: &ParsedArgument<L>,
    callable_spec: &Arc<dyn CallableSpec<L>>,
    callable_index: u32,
    children_start: u32,
    nodes: &[crate::node::NodeData<L>],
    cx: &mut SerializeContext<'_, L>,
) -> Result<WireArgument, SerializeError> {
    let spec_payload = callable_spec.serialize_argument_spec(arg_index, &argument.spec, cx)?;
    let region = argument.region.as_ref().map(|region| serialize_region(region, callable_index, children_start, nodes));
    let ext = argument.ext.as_ref().map(|ext| ext.serialize_value(cx)).transpose()?;
    Ok(WireArgument { region, ext, spec_payload })
}

fn serialize_slot<L: SerializableLang>(
    slot: &ParsedSlot<L>,
    callable_index: u32,
    children_start: u32,
    nodes: &[crate::node::NodeData<L>],
    cx: &mut SerializeContext<'_, L>,
) -> Result<WireSlot, SerializeError> {
    Ok(WireSlot {
        name: slot.name().map(String::from),
        region: serialize_region(&slot.region, callable_index, children_start, nodes),
        role: slot.role,
        ext: slot.ext.serialize_value(cx)?,
    })
}

/// The builder-ready form of a resolved region (see [`WireRegion`]): its node offsets
/// within the callable's child list, its content offsets, and the storage index of
/// the content parent.
fn serialize_region<L: Lang>(
    region: &ChildRegion,
    callable_index: u32,
    children_start: u32,
    nodes: &[crate::node::NodeData<L>],
) -> WireRegion {
    let region_children = region.children();
    let region_start = region_children.start;
    let children = WireRange { start: region_start - children_start, end: region_children.end - children_start };
    let content_parent = region.content_parent().index() as u32;
    let content_range = region.content_range();
    let content = if content_parent == callable_index {
        WireRange { start: content_range.start - region_start, end: content_range.end - region_start }
    } else {
        let parent_start = nodes[content_parent as usize].children.start;
        WireRange { start: content_range.start - parent_start, end: content_range.end - parent_start }
    };
    WireRegion { children, content, content_parent }
}

// --- reading ---------------------------------------------------------------------------

fn node_failure(index: usize, detail: &str) -> DeserializeError {
    DeserializeError::failed(alloc::format!("node #{index}: {detail}"))
}

/// Rebuild a tree from its wire nodes and annotations through the node builder.
fn rebuild_tree<L: SerializableLang, A: Clone>(
    wire_nodes: Vec<WireNode>,
    annotations: Vec<A>,
    cx: &mut DeserializeContext<'_, L>,
) -> Result<NodeTree<L, A>, DeserializeError> {
    let node_count = wire_nodes.len();
    if node_count == 0 {
        return Err(DeserializeError::failed("a node tree has at least one node (the root)"));
    }
    // `node_count == annotations.len()` by the codec; guard anyway.
    if annotations.len() != node_count {
        return Err(DeserializeError::failed("the number of annotations does not match the number of nodes"));
    }

    let mut builder = NodeTreeBuilder::<L, A>::new();
    let mut build_id_of: Vec<Option<BuildId>> = alloc::vec![None; node_count];

    // Stage in reverse storage order: a node's children (and every region's content
    // parent, a descendant) are stored after it, so they are staged first.
    for index in (0..node_count).rev() {
        let wire = &wire_nodes[index];
        let (child_start, child_end) = (wire.children.start, wire.children.end);
        if child_start > child_end || child_end as usize > node_count {
            return Err(node_failure(index, "children range out of bounds"));
        }
        if child_start != child_end && (child_start as usize) <= index {
            return Err(node_failure(index, "children must be stored after their parent"));
        }
        let mut children = Vec::with_capacity((child_end - child_start) as usize);
        for child in child_start..child_end {
            let build_id = build_id_of[child as usize]
                .ok_or_else(|| node_failure(index, "a child references a node not stored after it"))?;
            children.push(build_id);
        }

        let kind = rebuild_kind(index as u32, wire, &build_id_of, cx)?;
        let span = deserialize_span(wire.span, cx)?;
        let state = cx.state(wire.state)?;
        let ext = <NodeExt<L> as DeserializableValue<L>>::deserialize_value(&wire.ext, cx)?;
        let annotation = annotations[index].clone();
        let build_id = builder
            .add(kind, span, state, children, ext, annotation)
            .map_err(|error| node_failure(index, &error.to_string()))?;
        build_id_of[index] = Some(build_id);
    }

    // `node_count >= 1` and every index was staged: node 0 has a build id.
    let root = build_id_of[0].expect("the root node was staged");
    let tree = builder
        .finish(root)
        .map_err(|error| DeserializeError::failed(alloc::format!("finishing the tree: {error}")))?;
    // Defense in depth: the builder enforces the all-trees law, but the reader is a
    // total validator of untrusted input.
    validate_tree(&tree)
        .map_err(|violation| DeserializeError::failed(alloc::format!("the rebuilt tree is invalid: {violation}")))?;
    Ok(tree)
}

fn rebuild_kind<L: SerializableLang>(
    index: u32,
    wire: &WireNode,
    build_id_of: &[Option<BuildId>],
    cx: &mut DeserializeContext<'_, L>,
) -> Result<NodeKind<L>, DeserializeError> {
    match &wire.kind {
        WireNodeKind::Chars { content } => Ok(NodeKind::Chars { content: content.clone() }),
        WireNodeKind::Group { group_type, open, close } => {
            let group_type = <Option<L::GroupTypeId> as DeserializableValue<L>>::deserialize_value(group_type, cx)?;
            Ok(NodeKind::Group(Box::new(GroupData { group_type, open: open.clone(), close: close.clone() })))
        }
        WireNodeKind::Comment { start, content, post_space } => Ok(NodeKind::Comment(Box::new(CommentData {
            start: start.clone(),
            content: content.clone(),
            post_space: post_space.clone(),
        }))),
        WireNodeKind::List => Ok(NodeKind::List),
        WireNodeKind::Callable { callable_type, name, spec, arguments, slots, invocation_syntax } => {
            let callable_type = <L::CallableTypeId as DeserializableValue<L>>::deserialize_value(callable_type, cx)?;
            let spec = cx.spec(*spec)?;
            let arguments = arguments
                .iter()
                .enumerate()
                .map(|(arg_index, wire)| rebuild_argument(arg_index, wire, &spec, index, build_id_of, cx))
                .collect::<Result<Vec<_>, _>>()?;
            let slots = slots
                .iter()
                .map(|wire| rebuild_slot(wire, index, build_id_of, cx))
                .collect::<Result<Vec<_>, _>>()?;
            let invocation_syntax =
                <L::InvocationSyntax as DeserializableValue<L>>::deserialize_value(invocation_syntax, cx)?;
            Ok(NodeKind::Callable(Box::new(CallableData {
                callable_type,
                name: name.as_str().into(),
                spec,
                arguments: ParsedArguments::new(arguments),
                slots: ParsedSlots::new(slots),
                invocation_syntax,
            })))
        }
    }
}

fn rebuild_argument<L: SerializableLang>(
    arg_index: usize,
    wire: &WireArgument,
    callable_spec: &Arc<dyn CallableSpec<L>>,
    callable_index: u32,
    build_id_of: &[Option<BuildId>],
    cx: &mut DeserializeContext<'_, L>,
) -> Result<ParsedArgument<L>, DeserializeError> {
    let spec = callable_spec.deserialize_argument_spec(arg_index, wire.spec_payload.as_ref(), cx)?;
    match &wire.region {
        Some(region) => {
            let region = staged_region(region, callable_index, build_id_of)?;
            let ext_value = wire
                .ext
                .as_ref()
                .ok_or_else(|| DeserializeError::failed("a provided argument carries an ext"))?;
            let ext = <ArgumentExt<L> as DeserializableValue<L>>::deserialize_value(ext_value, cx)?;
            Ok(ParsedArgument::provided(spec, region, ext))
        }
        None => Ok(ParsedArgument::absent(spec)),
    }
}

fn rebuild_slot<L: SerializableLang>(
    wire: &WireSlot,
    callable_index: u32,
    build_id_of: &[Option<BuildId>],
    cx: &mut DeserializeContext<'_, L>,
) -> Result<ParsedSlot<L>, DeserializeError> {
    let region = staged_region(&wire.region, callable_index, build_id_of)?;
    let ext = <SlotExt<L> as DeserializableValue<L>>::deserialize_value(&wire.ext, cx)?;
    match &wire.name {
        Some(name) => Ok(ParsedSlot::new(region, name.as_str(), wire.role, ext)),
        None => Ok(ParsedSlot::new_unnamed(region, wire.role, ext)),
    }
}

/// Convert a wire region into the builder's staged form: the region's child offsets
/// and its content designation ([`ContentNodes`]) in build-id terms.
fn staged_region(
    wire: &WireRegion,
    callable_index: u32,
    build_id_of: &[Option<BuildId>],
) -> Result<ChildRegion, DeserializeError> {
    let children = wire.children.start..wire.children.end;
    let content = wire.content.start..wire.content.end;
    let content = if wire.content_parent == callable_index {
        ContentNodes::InRegion(content)
    } else {
        let build_id = build_id_of
            .get(wire.content_parent as usize)
            .copied()
            .flatten()
            .ok_or_else(|| DeserializeError::failed("a region's content parent is not a node stored before its callable"))?;
        ContentNodes::InChildrenOf(build_id, content)
    };
    Ok(ChildRegion::new(children, content))
}

// --- value conversions of core payload types (D23; a language's own codecs reuse them)

/// Textual content is `{spanned: {start, end}}` (a byte range into the carrying node's
/// own source) or `{owned: "text"}` — its serialized shape needs no context.
impl<L: Lang> SerializableValue<L> for TextContent {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(self.to_serial_value()?)
    }
}

impl<L: Lang> DeserializableValue<L> for TextContent {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        Ok(TextContent::from_serial_value(value)?)
    }
}

/// A byte range is `{start, end}`; `start > end` is rejected on reading.
impl<L: Lang> SerializableValue<L> for Span {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(self.to_serial_value()?)
    }
}

impl<L: Lang> DeserializableValue<L> for Span {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        Ok(Span::from_serial_value(value)?)
    }
}

/// A slot's role is `"content"`, `"attached"`, or `"hidden"`.
impl<L: Lang> SerializableValue<L> for SlotRole {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(self.to_serial_value()?)
    }
}

impl<L: Lang> DeserializableValue<L> for SlotRole {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        Ok(SlotRole::from_serial_value(value)?)
    }
}

/// A group rule is `{group_type, open, close}` — its class in the language's own form,
/// its delimiters as strings.
impl<L: Lang> SerializableValue<L> for GroupRule<L> {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        let wire = WireGroupRule {
            group_type: self.group_type.serialize_value(cx)?,
            open: self.open.clone(),
            close: self.close.clone(),
        };
        Ok(wire.to_serial_value()?)
    }
}

impl<L: Lang> DeserializableValue<L> for GroupRule<L> {
    fn deserialize_value(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let wire = WireGroupRule::from_serial_value(value)?;
        Ok(GroupRule {
            group_type: <L::GroupTypeId as DeserializableValue<L>>::deserialize_value(&wire.group_type, cx)?,
            open: wire.open,
            close: wire.close,
        })
    }
}
