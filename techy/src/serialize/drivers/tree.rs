//! The tree driver: [`TreeSerdeDriver`], the driver of the trees table, and
//! [`TreeIndex`], its position type; the annotation-codec registration
//! ([`TableHandle::register_annotation`]); the [`TreeSerialization`] extension trait
//! (`serialize_tree` / `tree`); and the context-aware value conversions of the core
//! payload types a language's own codecs reuse ([`TextContent`] — owned text only,
//! [`SlotRole`], [`GroupRule`]).
//!
//! A node tree is rebuilt through the node builder: the reader stages the nodes in
//! reverse storage order (so a node's children exist before it), re-resolves each
//! callable's argument and slot regions from the builder-ready form the wire stores,
//! and [`finish`](crate::node::NodeTreeBuilder::finish)es the tree — which mints a
//! fresh layout tag, recomputes the parent table, and re-establishes every region
//! invariant by construction. Everything read is untrusted input: a children range
//! out of bounds, a node no other node claims as a child, a region that does not tile
//! the child list, a content parent outside its region, a span out of bounds, a
//! reference into the wrong table — each is an error naming the node
//! ([`DeserializeError::InNode`]), never a panic. Text inside language-typed payloads
//! (the invocation syntax, the ext values) is owned on the wire — the value
//! conversions receive no node whose source a span could be validated against; the
//! [`TreeSerdeDriver`] docs state the rule.

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
    GroupData, NodeBuildError, NodeExt, NodeKind, NodeTree, NodeTreeBuilder, ParsedArgument,
    ParsedArguments, ParsedSlot, ParsedSlots, SlotExt, SlotRole,
};
use crate::source::TextContent;
use crate::spec::CallableSpec;
use crate::state::{InvocationSyntax, Lang};
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
/// is registered by the driver itself, in every session it is registered in, under
/// the identifier `core.tree`, and its annotations are omitted from the wire; any
/// other annotation type is registered on the table's handle with
/// [`register_annotation`](TableHandle::register_annotation), which serializes each
/// node's annotation through the type's own
/// [`SerializableValue`]/[`DeserializableValue`] conversions (so an annotation that
/// refers to a table object — a [`SourceSpan`](crate::source::SourceSpan), say —
/// interns it like any other value). Serializing a tree whose annotation type is not
/// registered is a [`SerializeError`].
///
/// The table's object type is `dyn Any + Send + Sync` (one table for every annotation
/// type), but the table accepts `NodeTree<L, A>` values only: interning any other
/// object through the general [`SerdeSession::intern`](crate::serialize::SerdeSession::intern)
/// is a [`SerializeError`] naming the object as not a node tree of a registered
/// annotation type. The everyday spellings, typed to node trees, are the
/// [`TreeSerialization`] extension trait's `serialize_tree` and `tree`. Registered by
/// [`SerdeSession::new`](crate::serialize::SerdeSession::new); a session composed with
/// [`SerdeSession::empty`](crate::serialize::SerdeSession::empty) registers it with
/// [`SerdeSession::register_table`](crate::serialize::SerdeSession::register_table)
/// (the unit annotation needs no further registration).
///
/// **Text in language payloads is owned on the wire.** A node's own text payloads
/// (the content of a `Chars` node, a group's delimiters, a comment's parts) may be
/// span-backed ([`TextContent::Spanned`]) on the wire: the reader validates each such
/// byte range against the node's own source before the tree is finished. A callable's
/// invocation syntax and every ext value (a node's, an argument's, a slot's) are
/// language-typed payloads the driver writes and reads through their value
/// conversions, which receive no node — hence no source to validate against. So the
/// writer materializes the invocation syntax against the node's source
/// ([`InvocationSyntax::materialized`]) before converting it, [`TextContent`]'s value
/// conversion writes and reads owned text only, and ext values must not carry spans
/// relative to the node's source (the contract [`NodeTree::materialize`] states — it
/// leaves ext values untouched).
///
/// # Panics
///
/// The writer calls [`InvocationSyntax::materialized`] on each callable node's
/// invocation syntax: a live tree that violates the [`TextContent::Spanned`]
/// invariant — a span-backed text of the invocation syntax whose byte range is not a
/// valid range of the node's source — panics there, exactly as
/// [`NodeTree::materialize`] does on such a tree (the crate-wide contract on
/// span-backed text; a tree built by the parser or the node builder never violates
/// it). Reading panics on no input.
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
/// identifier (for reading). Created on first use with the unit annotation's codec
/// (`core.tree`) registered, so every trees table has it.
struct TreeRegistry<L: SerializableLang> {
    by_type: HashMap<TypeId, TreeCodec<L>>,
    by_identifier: HashMap<String, TreeCodec<L>>,
}

impl<L: SerializableLang> Default for TreeRegistry<L> {
    fn default() -> Self {
        let mut registry = TreeRegistry { by_type: HashMap::new(), by_identifier: HashMap::new() };
        let unit = unit_tree_codec::<L>();
        registry.by_type.insert(TypeId::of::<NodeTree<L, ()>>(), unit.clone());
        registry.by_identifier.insert(unit.identifier.to_string(), unit);
        registry
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
            "the object is not a NodeTree of a registered annotation type (the trees table \
             accepts NodeTree<L, A> values only, for an annotation type A registered with \
             TableHandle::register_annotation; the unit annotation is pre-registered)",
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
            let annotations = serialize_annotations(tree, |annotation, cx| annotation.serialize_value(cx), cx)?;
            Ok(WireTree { nodes, annotations: Some(annotations) }.to_serial_value()?)
        }),
        deserialize: Arc::new(|data, cx| {
            let wire = WireTree::from_serial_value(data)?;
            let (nodes, annotations) = deserialize_annotations(
                wire,
                |value, cx| <A as DeserializableValue<L>>::deserialize_value(value, cx),
                cx,
            )?;
            let tree = rebuild_tree::<L, A>(nodes, annotations, cx)?;
            Ok(Arc::new(tree) as Arc<dyn Any + Send + Sync>)
        }),
    }
}

/// The annotations of `tree`, one wire value per node through `serialize`; a failure
/// is wrapped in [`SerializeError::InNode`] with the node's position.
fn serialize_annotations<L: SerializableLang, A>(
    tree: &NodeTree<L, A>,
    serialize: impl Fn(&A, &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>,
    cx: &mut SerializeContext<'_, L>,
) -> Result<Vec<SerialValue>, SerializeError> {
    tree.annotations()
        .iter()
        .zip(tree.nodes())
        .enumerate()
        .map(|(index, (annotation, node))| {
            serialize(annotation, cx).map_err(|error| error.in_node(index as u32, callable_name(&node.kind)))
        })
        .collect()
}

/// The annotations of a wire tree of a non-unit annotation type — required, one per
/// node — read through `deserialize`, and the wire nodes; a failure at an annotation is
/// wrapped in [`DeserializeError::InNode`] with the node's position.
fn deserialize_annotations<L: SerializableLang, A>(
    mut wire: WireTree,
    deserialize: impl Fn(&SerialValue, &mut DeserializeContext<'_, L>) -> Result<A, DeserializeError>,
    cx: &mut DeserializeContext<'_, L>,
) -> Result<(Vec<WireNode>, Vec<A>), DeserializeError> {
    let node_count = wire.nodes.len();
    let list = wire.annotations.take().ok_or_else(|| {
        DeserializeError::failed("the tree entry carries no annotations, but its annotation type is not the unit type")
    })?;
    if list.len() != node_count {
        return Err(DeserializeError::failed(alloc::format!(
            "the tree has {node_count} nodes but {} annotations",
            list.len()
        )));
    }
    let annotations = list
        .iter()
        .zip(&wire.nodes)
        .enumerate()
        .map(|(index, (value, node))| {
            deserialize(value, cx).map_err(|error| error.in_node(index as u32, wire_callable_name(node)))
        })
        .collect::<Result<Vec<A>, _>>()?;
    Ok((wire.nodes, annotations))
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
    /// pre-registered under `core.tree` and needs no registration. An annotation
    /// value must not carry spans relative to a node's source: the conversion runs
    /// without access to any node (a [`SourceSpan`](crate::source::SourceSpan), which names
    /// its source, is fine).
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

    /// Register the codec for tree annotation type `A` under `identifier` through the
    /// serde bridge: `A`'s annotations are serialized with
    /// [`to_value`](crate::serialize::to_value) and read back with
    /// [`from_value`](crate::serialize::from_value) — the convenience for a plain-data
    /// annotation type (one whose values refer to no table object, so they need no
    /// serialization context). An annotation type that does refer to a table object (a
    /// [`SourceSpan`](crate::source::SourceSpan)) uses
    /// [`register_annotation`](TableHandle::register_annotation) instead. Available
    /// with the `serde` cargo feature.
    ///
    /// # Errors
    ///
    /// As [`register_annotation`](TableHandle::register_annotation).
    #[cfg(feature = "serde")]
    pub fn register_serde_annotation<A>(
        self,
        session: &mut SerdeSession<L>,
        identifier: impl Into<Cow<'static, str>>,
    ) -> Result<(), RegistrationError>
    where
        A: serde::Serialize + serde::de::DeserializeOwned + Clone + Debug + Send + Sync + 'static,
    {
        register_codec::<L>(self, session, TypeId::of::<NodeTree<L, A>>(), serde_tree_codec::<L, A>(identifier.into()))
    }
}

/// The codec of a plain-data annotation type `A`, through the serde bridge (see
/// [`register_serde_annotation`](TableHandle::register_serde_annotation)).
#[cfg(feature = "serde")]
fn serde_tree_codec<L, A>(identifier: Cow<'static, str>) -> TreeCodec<L>
where
    L: SerializableLang,
    A: serde::Serialize + serde::de::DeserializeOwned + Clone + Debug + Send + Sync + 'static,
{
    TreeCodec {
        identifier,
        serialize: Arc::new(|object, cx| {
            let tree = downcast_tree::<L, A>(object)?;
            let nodes = serialize_nodes(tree, cx)?;
            let annotations = serialize_annotations(
                tree,
                |annotation, _cx| super::super::bridge::to_value(annotation).map_err(SerializeError::from),
                cx,
            )?;
            Ok(WireTree { nodes, annotations: Some(annotations) }.to_serial_value()?)
        }),
        deserialize: Arc::new(|data, cx| {
            let wire = WireTree::from_serial_value(data)?;
            let (nodes, annotations) = deserialize_annotations(
                wire,
                |value, _cx| super::super::bridge::from_value::<A>(value).map_err(DeserializeError::from),
                cx,
            )?;
            let tree = rebuild_tree::<L, A>(nodes, annotations, cx)?;
            Ok(Arc::new(tree) as Arc<dyn Any + Send + Sync>)
        }),
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
    /// ([`DeserializeError::Failed`], naming the identifier the tree was serialized
    /// under and the annotation type requested).
    fn tree<A>(&mut self, position: TreeIndex) -> Result<NodeTree<L, A>, DeserializeError>
    where
        A: SerializableValue<L> + DeserializableValue<L> + Clone + Debug + Send + Sync + 'static;
}

/// The identifier registered in `session`'s trees table for the tree type `tree_type`
/// (`TypeId::of::<NodeTree<L, A>>()`), when there is one.
fn registered_identifier<L: SerializableLang>(session: &mut SerdeSession<L>, tree_type: TypeId) -> Option<String> {
    let ordinal = session.table_ordinal_by_name(TREES_TABLE)?;
    let registry = session.registry_mut::<TreeRegistry<L>>(ordinal)?;
    registry.by_type.get(&tree_type).map(|codec| codec.identifier.to_string())
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
            Err(object) => {
                // The stored tree's codec is registered (it was read through it); the
                // requested type may not be.
                let stored = registered_identifier(self, (*object).type_id())
                    .unwrap_or_else(|| String::from("(unregistered)"));
                let requested = core::any::type_name::<A>();
                let requested_identifier = registered_identifier(self, TypeId::of::<NodeTree<L, A>>())
                    .map_or_else(|| String::from("unregistered"), |identifier| alloc::format!("`{identifier}`"));
                Err(DeserializeError::failed(alloc::format!(
                    "the tree at this position was serialized under `{stored}` and cannot be read as \
                     annotation type `{requested}` ({requested_identifier})"
                )))
            }
        }
    }
}

// --- writing ---------------------------------------------------------------------------

/// The nodes of `tree` in storage order — annotation-blind (the annotations are
/// serialized by the codec). A failure at a node is wrapped in
/// [`SerializeError::InNode`] with the node's position and, for a callable, its name.
fn serialize_nodes<L: SerializableLang, A>(
    tree: &NodeTree<L, A>,
    cx: &mut SerializeContext<'_, L>,
) -> Result<Vec<WireNode>, SerializeError> {
    let nodes = tree.nodes();
    nodes
        .iter()
        .enumerate()
        .map(|(index, data)| {
            serialize_node(index as u32, data, nodes, cx)
                .map_err(|error| error.in_node(index as u32, callable_name(&data.kind)))
        })
        .collect()
}

/// The invocation name of a callable node, for the location wrappers.
fn callable_name<L: Lang>(kind: &NodeKind<L>) -> Option<String> {
    match kind {
        NodeKind::Callable(data) => Some(data.name.to_string()),
        _ => None,
    }
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
            // The invocation syntax is materialized against the node's own source
            // first: its value conversion receives no node, so span-backed text
            // inside it could not be validated on reading (see `TreeSerdeDriver`).
            let invocation_syntax = callable.invocation_syntax.materialized(data.span.source()).serialize_value(cx)?;
            Ok(WireNodeKind::Callable {
                callable_type: callable.callable_type.serialize_value(cx)?,
                name: callable.name.to_string(),
                spec: cx.intern_spec(&callable.spec)?,
                arguments,
                slots,
                invocation_syntax,
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

/// Rebuild a tree from its wire nodes and annotations through the node builder.
///
/// The wire node list must be exactly the nodes reachable from the root: every node
/// but the root is listed among the children of exactly one node stored before it,
/// and the root among nobody's — checked here, so that no node is silently dropped
/// (the builder drops staged nodes the root does not reach) and none is claimed twice.
/// A failure at a node is wrapped in [`DeserializeError::InNode`] with the node's
/// position and, for a callable, its name.
fn rebuild_tree<L: SerializableLang, A>(
    wire_nodes: Vec<WireNode>,
    annotations: Vec<A>,
    cx: &mut DeserializeContext<'_, L>,
) -> Result<NodeTree<L, A>, DeserializeError> {
    let node_count = wire_nodes.len();
    if node_count == 0 {
        return Err(DeserializeError::failed("a node tree has at least one node (the root)"));
    }
    if u32::try_from(node_count).is_err() {
        return Err(DeserializeError::failed("a node tree has at most u32::MAX nodes"));
    }
    // `node_count == annotations.len()` by the codec; guard anyway.
    if annotations.len() != node_count {
        return Err(DeserializeError::failed("the number of annotations does not match the number of nodes"));
    }

    let mut builder = NodeTreeBuilder::<L, A>::new();
    // The staging map: wire position → builder id, filled in reverse storage order.
    let mut build_id_of: Vec<Option<BuildId>> = alloc::vec![None; node_count];
    // Which node lists each node among its children (`None`: no node so far).
    let mut claimed_by: Vec<Option<u32>> = alloc::vec![None; node_count];

    // Stage in reverse storage order: a node's children (and every region's content
    // parent, a descendant) are stored after it, so they are staged first.
    for (index, annotation) in annotations.into_iter().enumerate().rev() {
        let wire = &wire_nodes[index];
        let build_id =
            stage_node(index as u32, wire, &build_id_of, &mut claimed_by, &mut builder, annotation, cx)
                .map_err(|error| error.in_node(index as u32, wire_callable_name(wire)))?;
        build_id_of[index] = Some(build_id);
    }

    // Every node but the root is some node's child; the root is nobody's (a children
    // range reaching position 0 fails the stored-after-its-parent check in `stage_node`).
    if let Some(unclaimed) = (1..node_count).find(|&index| claimed_by[index].is_none()) {
        return Err(DeserializeError::failed(
            "no node lists this node among its children (the serialized node list must be \
             exactly the nodes reachable from the root)",
        )
        .in_node(unclaimed as u32, wire_callable_name(&wire_nodes[unclaimed])));
    }

    // `node_count >= 1` and every position was staged: node 0 has a build id.
    let root = build_id_of[0].expect("the root node was staged");
    let tree = builder.finish(root).map_err(|error| builder_failure(error, &build_id_of))?;
    // Defense in depth: the builder enforces the all-trees law, but the reader is a
    // total validator of untrusted input.
    validate_tree(&tree).map_err(|violation| {
        DeserializeError::failed(alloc::format!("the rebuilt tree is invalid: {violation}")).with_cause(violation)
    })?;
    // Every node was claimed once, so the builder reached every node from the root; a
    // shorter tree would be a bug in this reader or in the builder.
    if tree.node_count() != node_count {
        return Err(DeserializeError::Internal {
            detail: alloc::format!(
                "the rebuilt tree has {} nodes, but the entry lists {node_count} and every one was \
                 checked reachable from the root",
                tree.node_count()
            ),
        });
    }
    Ok(tree)
}

/// Validate and stage wire node `index`: its children range (in bounds, stored after
/// the node, each child listed by this node alone), then its kind, span, state, ext,
/// and annotation, into `builder`. Records this node as the one listing its children in
/// `claimed_by`.
fn stage_node<L: SerializableLang, A>(
    index: u32,
    wire: &WireNode,
    build_id_of: &[Option<BuildId>],
    claimed_by: &mut [Option<u32>],
    builder: &mut NodeTreeBuilder<L, A>,
    annotation: A,
    cx: &mut DeserializeContext<'_, L>,
) -> Result<BuildId, DeserializeError> {
    let node_count = build_id_of.len();
    let (child_start, child_end) = (wire.children.start, wire.children.end);
    if child_start > child_end || child_end as usize > node_count {
        return Err(DeserializeError::failed(alloc::format!(
            "children range {child_start}..{child_end} is out of bounds ({node_count} nodes)"
        )));
    }
    if child_start != child_end && child_start <= index {
        return Err(DeserializeError::failed(alloc::format!(
            "children range {child_start}..{child_end} does not lie after the node (a node's \
             children are stored after it)"
        )));
    }
    let mut children = Vec::with_capacity((child_end - child_start) as usize);
    for child in child_start..child_end {
        if let Some(other) = claimed_by[child as usize] {
            return Err(DeserializeError::failed(alloc::format!(
                "node #{child} is already listed among the children of node #{other}"
            )));
        }
        claimed_by[child as usize] = Some(index);
        // Stored after this node, hence staged before it (staging runs in reverse).
        let build_id = build_id_of[child as usize].ok_or_else(|| DeserializeError::Internal {
            detail: alloc::format!("child node #{child} of node #{index} was not staged before its parent"),
        })?;
        children.push(build_id);
    }

    let kind = rebuild_kind(index, wire, build_id_of, cx)?;
    let span = deserialize_span(wire.span, cx)?;
    let state = cx.state(wire.state)?;
    let ext = <NodeExt<L> as DeserializableValue<L>>::deserialize_value(&wire.ext, cx)?;
    builder.add(kind, span, state, children, ext, annotation).map_err(|error| builder_failure(error, build_id_of))
}

/// The invocation name of a callable wire node, for the location wrappers.
fn wire_callable_name(wire: &WireNode) -> Option<String> {
    match &wire.kind {
        WireNodeKind::Callable { name, .. } => Some(name.clone()),
        _ => None,
    }
}

/// The wire position a builder id was staged for — the staging map inverted (a linear
/// search, on error paths only).
fn wire_position_of(build_id_of: &[Option<BuildId>], id: BuildId) -> Option<usize> {
    build_id_of.iter().position(|staged| *staged == Some(id))
}

/// A builder error as a read error: the message names wire node positions rather than
/// builder ids, and the builder error itself is kept as the cause.
fn builder_failure(error: NodeBuildError, build_id_of: &[Option<BuildId>]) -> DeserializeError {
    let node = |id: BuildId| match wire_position_of(build_id_of, id) {
        Some(position) => alloc::format!("node #{position}"),
        None => alloc::format!("a node the reader never staged ({id:?})"),
    };
    let detail = match &error {
        NodeBuildError::ChildNotStaged { child } => alloc::format!("child {} was not staged", node(*child)),
        NodeBuildError::ChildAlreadyClaimed { child } => alloc::format!("child {} already has a parent", node(*child)),
        NodeBuildError::ContentParentNotStaged { parent } => {
            alloc::format!("content parent {} was not staged", node(*parent))
        }
        NodeBuildError::RootNotStaged { root } => alloc::format!("root {} was not staged", node(*root)),
        NodeBuildError::RootClaimed { root } => alloc::format!("root {} is another node's child", node(*root)),
        NodeBuildError::ContentParentUnreachable { parent } => {
            alloc::format!("content parent {} is not reachable from the root", node(*parent))
        }
        NodeBuildError::ContentParentOutsideSubtree { parent } => {
            alloc::format!("content parent {} is not inside the callable's subtree", node(*parent))
        }
        NodeBuildError::ContentParentOutsideRegion { parent } => {
            alloc::format!("content parent {} lies outside its own argument/slot region", node(*parent))
        }
        other => other.to_string(),
    };
    DeserializeError::failed(alloc::format!("the node builder rejected the tree: {detail}")).with_cause(error)
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
            // A provided argument's ext follows its region: an ext whose serialized form
            // is null (the unit ext, an absent optional inside the ext) is omitted from
            // the wire, and reads back from null.
            let null = SerialValue::Null;
            let ext_value = wire.ext.as_ref().unwrap_or(&null);
            let ext = <ArgumentExt<L> as DeserializableValue<L>>::deserialize_value(ext_value, cx)?;
            Ok(ParsedArgument::provided(spec, region, ext))
        }
        None => {
            // An absent argument has no ext: one on the wire is not silently dropped.
            if wire.ext.is_some() {
                return Err(DeserializeError::failed(alloc::format!(
                    "argument #{} is absent (it has no region) but carries an ext",
                    arg_index.saturating_add(1)
                )));
            }
            Ok(ParsedArgument::absent(spec))
        }
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
/// and its content designation ([`ContentNodes`]) in build-id terms. A content parent
/// other than the callable itself must be a node inside the region — a descendant of
/// the callable, stored after it, hence already staged; the builder checks that it lies
/// inside the region's own subtree when the tree is finished.
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
        let parent = wire.content_parent;
        let build_id = match build_id_of.get(parent as usize) {
            None => {
                return Err(DeserializeError::failed(alloc::format!(
                    "a region's content parent, node #{parent}, is out of range ({} nodes)",
                    build_id_of.len()
                )))
            }
            Some(None) => {
                return Err(DeserializeError::failed(alloc::format!(
                    "a region's content parent, node #{parent}, is stored before its callable (a \
                     content parent is a node inside the region, stored after the callable)"
                )))
            }
            Some(Some(build_id)) => *build_id,
        };
        ContentNodes::InChildrenOf(build_id, content)
    };
    Ok(ChildRegion::new(children, content))
}

// --- value conversions of core payload types (D23; a language's own codecs reuse them)

/// The value conversion of textual content — for text inside a language-typed payload
/// (a callable's invocation syntax, an ext value) — carries **owned text only**:
/// `{owned: "text"}`. A [`Spanned`](TextContent::Spanned) value is an error: the
/// conversion receives no node, so a byte range into the carrying node's
/// source could not be validated on reading, and text that is span-backed must be
/// materialized against the node's source first ([`TextContent::materialized`] — for a
/// callable's invocation syntax, the tree writer does so through
/// [`InvocationSyntax::materialized`] before converting the payload; see
/// [`TreeSerdeDriver`]). The text payloads of the nodes themselves (a `Chars` node's
/// content, a group's delimiters, a comment's parts) do not go through this
/// conversion: the tree driver writes them span-backed and validates the ranges
/// against the node's source.
impl<L: Lang> SerializableValue<L> for TextContent {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        match self {
            TextContent::Owned(_) => Ok(self.to_serial_value()?),
            TextContent::Spanned(span) => Err(SerializeError::failed(alloc::format!(
                "span-backed text ({span:?}) inside a language payload cannot be serialized: \
                 the payload's value conversion receives no node, so the text must be \
                 materialized against the node's source first (TextContent::materialized)"
            ))),
        }
    }
}

/// Reads `{owned: "text"}` only; a `{spanned: {start, end}}` value is an error (see
/// the write side above: without the node, the range could not be validated).
impl<L: Lang> DeserializableValue<L> for TextContent {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        match TextContent::from_serial_value(value)? {
            owned @ TextContent::Owned(_) => Ok(owned),
            TextContent::Spanned(span) => Err(DeserializeError::failed(alloc::format!(
                "span-backed text ({span:?}) inside a language payload is not accepted: the \
                 payload's value conversion receives no node to validate the range against \
                 (text inside a language payload is owned on the wire)"
            ))),
        }
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
