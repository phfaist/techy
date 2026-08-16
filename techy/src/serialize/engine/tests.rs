//! Tests of the engine, standalone with toy object kinds: a homogeneous table
//! (`Leaf`), a heterogeneous table over a toy trait object (`dyn Shape`, with a recipe
//! type and an identity-resolved type), and a composite table referencing both, all
//! for a toy `SerializableLang`. Multi-segment round trips with sharing preserved,
//! read-then-append, the failure battery, resolver dispatch and memoization,
//! determinism.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::engine::StdDescentGuardInit;
use crate::state::TrivialLang;

use crate::serialize::{
    DeserializableObject, DeserializeContext, DeserializeError, DispatchingSerdeDriver,
    IdentifierResolver, ObjectReader, ObjectSerdeDriver, RegistrationError, SerdeSession,
    Segment, SerialEntry, SerialIndex, SerialValue, SerializableLang,
    SerializableObject, SerializeContext, SerializeError, TableId,
};

// --- the toy lang ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct ToyLang;
impl TrivialLang for ToyLang {}
impl SerializableLang for ToyLang {}

// --- the toy object kinds -------------------------------------------------------------

/// A homogeneous-table object: a leaf with some text.
#[derive(Debug, PartialEq, Eq)]
struct Leaf {
    text: String,
}

crate::serial_index! {
    /// Position in the leaves table.
    pub struct LeafIndex;
}

struct LeafDriver;

impl ObjectSerdeDriver<ToyLang> for LeafDriver {
    type Object = Leaf;
    type Index = LeafIndex;

    fn table_name(&self) -> &'static str {
        "leaves"
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        Some("toy.leaf")
    }

    fn serialize_object(
        &self,
        leaf: &Arc<Leaf>,
        _cx: &mut SerializeContext<'_, ToyLang>,
    ) -> Result<SerialEntry, SerializeError> {
        Ok(SerialEntry { identifier: "toy.leaf".into(), data: SerialValue::Str(leaf.text.clone()) })
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        _cx: &mut DeserializeContext<'_, ToyLang>,
    ) -> Result<Arc<Leaf>, DeserializeError> {
        match &entry.data {
            SerialValue::Str(text) => Ok(Arc::new(Leaf { text: text.clone() })),
            _ => Err(DeserializeError::failed("a leaf is a string")),
        }
    }
}

/// The heterogeneous-table trait object.
trait Shape: SerializableObject<ToyLang> + core::fmt::Debug + Send + Sync {
    fn area(&self) -> u32;
}

crate::serial_index! {
    /// Position in the shapes table.
    pub struct ShapeIndex;
}

/// A recipe shape: fully described on the wire.
#[derive(Debug, PartialEq, Eq)]
struct Square {
    side: u32,
}

impl Shape for Square {
    fn area(&self) -> u32 {
        self.side * self.side
    }
}

impl SerializableObject<ToyLang> for Square {
    fn serialize_object(
        &self,
        _cx: &mut SerializeContext<'_, ToyLang>,
    ) -> Result<SerialEntry, SerializeError> {
        Ok(SerialEntry {
            identifier: "toy.square".into(),
            data: SerialValue::Map(Vec::from([(String::from("side"), SerialValue::Int(i64::from(self.side)))])),
        })
    }
}

impl DeserializableObject<ToyLang> for Square {
    type Output = Self;

    fn deserialize_object(
        value: &SerialValue,
        _cx: &mut DeserializeContext<'_, ToyLang>,
    ) -> Result<Self, DeserializeError> {
        let side = field_u32(value, "side")?;
        Ok(Square { side })
    }
}

/// An identity-resolved shape: looked up in the reading environment by name.
#[derive(Debug)]
struct NamedShape {
    name: String,
    area: u32,
}

impl PartialEq for NamedShape {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Shape for NamedShape {
    fn area(&self) -> u32 {
        self.area
    }
}

impl SerializableObject<ToyLang> for NamedShape {
    fn serialize_object(
        &self,
        _cx: &mut SerializeContext<'_, ToyLang>,
    ) -> Result<SerialEntry, SerializeError> {
        Ok(SerialEntry {
            identifier: "toy.named".into(),
            data: SerialValue::Str(self.name.clone()),
        })
    }
}

/// The reading environment for named shapes: the caller's user data.
#[derive(Default)]
struct ShapeEnvironment {
    shapes: Vec<Arc<NamedShape>>,
}

impl ShapeEnvironment {
    fn lookup(&self, name: &str) -> Option<Arc<NamedShape>> {
        self.shapes.iter().find(|shape| shape.name == name).cloned()
    }
}

impl DeserializableObject<ToyLang> for NamedShape {
    type Output = Arc<dyn Shape>;

    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, ToyLang>,
    ) -> Result<Self::Output, DeserializeError> {
        let name = match value {
            SerialValue::Str(name) => name.clone(),
            _ => return Err(DeserializeError::failed("a named shape is a string")),
        };
        let env = cx
            .user_data::<ShapeEnvironment>()
            .ok_or_else(|| DeserializeError::failed("no shape environment"))?;
        let shape = env
            .lookup(&name)
            .ok_or_else(|| DeserializeError::failed(alloc::format!("no shape named `{name}`")))?;
        Ok(shape as Arc<dyn Shape>)
    }
}

fn shapes_driver() -> DispatchingSerdeDriver<ToyLang, dyn Shape, ShapeIndex> {
    DispatchingSerdeDriver::new("shapes")
}

/// The composite table: a node referencing leaves and one shape.
#[derive(Debug)]
struct Node {
    leaves: Vec<LeafIndex>,
    shape: ShapeIndex,
}

crate::serial_index! {
    /// Position in the nodes table.
    pub struct NodeIndex;
}

struct NodeDriver {
    leaves: crate::serialize::TableHandle<LeafDriver>,
    shapes: crate::serialize::TableHandle<DispatchingSerdeDriver<ToyLang, dyn Shape, ShapeIndex>>,
}

impl ObjectSerdeDriver<ToyLang> for NodeDriver {
    type Object = Node;
    type Index = NodeIndex;

    fn table_name(&self) -> &'static str {
        "nodes"
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        Some("toy.node")
    }

    fn serialize_object(
        &self,
        node: &Arc<Node>,
        _cx: &mut SerializeContext<'_, ToyLang>,
    ) -> Result<SerialEntry, SerializeError> {
        let leaves = SerialValue::List(
            node.leaves
                .iter()
                .map(|leaf| SerialValue::Index { table: leaf.table(), index: leaf.index() })
                .collect(),
        );
        let shape = SerialValue::Index { table: node.shape.table(), index: node.shape.index() };
        Ok(SerialEntry {
            identifier: "toy.node".into(),
            data: SerialValue::Map(Vec::from([
                (String::from("leaves"), leaves),
                (String::from("shape"), shape),
            ])),
        })
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, ToyLang>,
    ) -> Result<Arc<Node>, DeserializeError> {
        let leaves_value = field(&entry.data, "leaves")?;
        let leaf_indices = match leaves_value {
            SerialValue::List(items) => items,
            _ => return Err(DeserializeError::failed("`leaves` is a list")),
        };
        let mut leaves = Vec::new();
        for item in leaf_indices {
            let index = as_index::<LeafIndex>(item)?;
            // Force resolution — the shared leaf comes back as one Arc.
            cx.resolve(self.leaves, index)?;
            leaves.push(index);
        }
        let shape = as_index::<ShapeIndex>(field(&entry.data, "shape")?)?;
        cx.resolve(self.shapes, shape)?;
        Ok(Arc::new(Node { leaves, shape }))
    }
}

// --- helpers --------------------------------------------------------------------------

fn field<'a>(value: &'a SerialValue, name: &str) -> Result<&'a SerialValue, DeserializeError> {
    match value {
        SerialValue::Map(entries) => entries
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
            .ok_or_else(|| DeserializeError::failed(alloc::format!("missing `{name}`"))),
        _ => Err(DeserializeError::failed("expected a map")),
    }
}

fn field_u32(value: &SerialValue, name: &str) -> Result<u32, DeserializeError> {
    match field(value, name)? {
        SerialValue::Int(i) => u32::try_from(*i).map_err(|_| DeserializeError::failed("out of range")),
        _ => Err(DeserializeError::failed("expected an integer")),
    }
}

fn as_index<I: SerialIndex>(value: &SerialValue) -> Result<I, DeserializeError> {
    match value {
        SerialValue::Index { table, index } => Ok(I::from_parts(*table, *index)),
        _ => Err(DeserializeError::failed("expected a table position")),
    }
}

/// A session with leaves + shapes + nodes registered, and the standard shape readers.
struct Tables {
    leaves: crate::serialize::TableHandle<LeafDriver>,
    shapes: crate::serialize::TableHandle<DispatchingSerdeDriver<ToyLang, dyn Shape, ShapeIndex>>,
    nodes: crate::serialize::TableHandle<NodeDriver>,
}

fn full_session() -> (SerdeSession<ToyLang>, Tables) {
    let mut session = SerdeSession::<ToyLang>::empty();
    let leaves = session.register_table(LeafDriver).unwrap();
    let shapes = session.register_table(shapes_driver()).unwrap();
    let nodes = session.register_table(NodeDriver { leaves, shapes }).unwrap();
    shapes.register_type::<Square>(&mut session, "toy.square", |square| Arc::new(square) as Arc<dyn Shape>).unwrap();
    shapes.register_type::<NamedShape>(&mut session, "toy.named", |shape| shape).unwrap();
    (session, Tables { leaves, shapes, nodes })
}

// --- registration ---------------------------------------------------------------------

#[test]
fn registration_numbers_tables_and_rejects_duplicates() {
    let mut session = SerdeSession::<ToyLang>::empty();
    let leaves = session.register_table(LeafDriver).unwrap();
    assert_eq!(leaves.id().ordinal(), 0);
    let shapes = session.register_table(shapes_driver()).unwrap();
    assert_eq!(shapes.id().ordinal(), 1);

    // A second table of the same name is rejected.
    assert_eq!(
        session.register_table(LeafDriver).unwrap_err(),
        RegistrationError::DuplicateTableName { name: "leaves" }
    );
}

#[test]
fn a_handle_from_another_session_is_unknown() {
    let mut a = SerdeSession::<ToyLang>::empty();
    let leaves_a = a.register_table(LeafDriver).unwrap();
    let mut b = SerdeSession::<ToyLang>::empty();
    // `b` has a leaves table too, but the handle is `a`'s.
    let _leaves_b = b.register_table(LeafDriver).unwrap();
    let leaf = Arc::new(Leaf { text: "x".into() });
    // Same ordinal, but interning through it into `b` still works because the driver
    // type matches; the genuinely-unknown case is an ordinal beyond the end.
    assert!(b.intern(leaves_a, &leaf).is_ok());
    // A handle whose ordinal is out of range.
    let mut c = SerdeSession::<ToyLang>::empty();
    assert!(matches!(
        c.intern(leaves_a, &leaf),
        Err(SerializeError::UnknownTable { .. })
    ));
}

// --- sharing and round trips ----------------------------------------------------------

#[test]
fn interning_the_same_arc_yields_the_same_position() {
    let (mut session, tables) = full_session();
    let leaf = Arc::new(Leaf { text: "shared".into() });
    let first = session.intern(tables.leaves, &leaf).unwrap();
    let again = session.intern(tables.leaves, &leaf).unwrap();
    assert_eq!(first, again);
    // A different allocation with equal content is a distinct entry.
    let other = session.intern(tables.leaves, &Arc::new(Leaf { text: "shared".into() })).unwrap();
    assert_ne!(first, other);
    assert_eq!(session.table_len(0), 2);
}

#[test]
fn multi_segment_round_trip_preserves_sharing() {
    // Build a graph: a node with two leaves (one shared with the shape-less path) and
    // a square shape; the same leaf Arc interned in two nodes.
    let (mut writer, wt) = full_session();
    let shared_leaf = Arc::new(Leaf { text: "shared".into() });
    let solo_leaf = Arc::new(Leaf { text: "solo".into() });
    let square: Arc<dyn Shape> = Arc::new(Square { side: 3 });

    let node_a = Arc::new(Node {
        leaves: Vec::from([
            writer.intern(wt.leaves, &shared_leaf).unwrap(),
            writer.intern(wt.leaves, &solo_leaf).unwrap(),
        ]),
        shape: writer.intern(wt.shapes, &square).unwrap(),
    });
    let a = writer.intern(wt.nodes, &node_a).unwrap();
    let node_b = Arc::new(Node {
        leaves: Vec::from([writer.intern(wt.leaves, &shared_leaf).unwrap()]),
        shape: writer.intern(wt.shapes, &square).unwrap(),
    });
    let b = writer.intern(wt.nodes, &node_b).unwrap();
    let segment = writer.take_segment();

    // Read it back in another session.
    let (mut reader, rt) = full_session();
    reader.push_segment(segment).unwrap();
    let read_a = reader.resolve(rt.nodes, a).unwrap();
    let read_b = reader.resolve(rt.nodes, b).unwrap();

    // The shared leaf is one Arc, referenced from both nodes.
    let a_shared = reader.resolve(rt.leaves, read_a.leaves[0]).unwrap();
    let b_shared = reader.resolve(rt.leaves, read_b.leaves[0]).unwrap();
    assert!(Arc::ptr_eq(&a_shared, &b_shared));
    assert_eq!(a_shared.text, "shared");
    // The square shape is shared too.
    let a_shape = reader.resolve(rt.shapes, read_a.shape).unwrap();
    let b_shape = reader.resolve(rt.shapes, read_b.shape).unwrap();
    assert!(Arc::ptr_eq(&a_shape, &b_shape));
    assert_eq!(a_shape.area(), 9);
}

#[test]
fn read_then_append_emits_only_new_entries() {
    // Session A emits seg1 with one node.
    let (mut a, at) = full_session();
    let leaf = Arc::new(Leaf { text: "l".into() });
    let square: Arc<dyn Shape> = Arc::new(Square { side: 2 });
    let node1 = Arc::new(Node {
        leaves: Vec::from([a.intern(at.leaves, &leaf).unwrap()]),
        shape: a.intern(at.shapes, &square).unwrap(),
    });
    let n1 = a.intern(at.nodes, &node1).unwrap();
    let seg1 = a.take_segment();

    // Session B absorbs seg1, interns a new node sharing the absorbed leaf, emits seg2.
    let (mut b, bt) = full_session();
    b.push_segment(seg1.clone()).unwrap();
    let absorbed_leaf = b.resolve(bt.leaves, node1.leaves[0]).unwrap();
    let square2: Arc<dyn Shape> = Arc::new(Square { side: 5 });
    let node2 = Arc::new(Node {
        leaves: Vec::from([b.intern(bt.leaves, &absorbed_leaf).unwrap()]),
        shape: b.intern(bt.shapes, &square2).unwrap(),
    });
    // The absorbed leaf is not re-emitted: its position is the existing one.
    assert_eq!(node2.leaves[0], node1.leaves[0]);
    let n2 = b.intern(bt.nodes, &node2).unwrap();
    let seg2 = b.take_segment();

    // seg2 carries only the new entries: no leaf (the shared one was absorbed), one
    // new square, one new node.
    let leaves_in_seg2 = seg2.tables().iter().find(|t| t.name() == "leaves").unwrap();
    assert!(leaves_in_seg2.entries().is_empty());
    let nodes_in_seg2 = seg2.tables().iter().find(|t| t.name() == "nodes").unwrap();
    assert_eq!(nodes_in_seg2.entries().len(), 1);
    assert_eq!(nodes_in_seg2.start(), 1);

    // Session C absorbs seg1 + seg2 and sees the sharing across the two segments.
    let (mut c, ct) = full_session();
    c.push_segment(seg1).unwrap();
    c.push_segment(seg2).unwrap();
    let c_node1 = c.resolve(ct.nodes, n1).unwrap();
    let c_node2 = c.resolve(ct.nodes, n2).unwrap();
    let l1 = c.resolve(ct.leaves, c_node1.leaves[0]).unwrap();
    let l2 = c.resolve(ct.leaves, c_node2.leaves[0]).unwrap();
    assert!(Arc::ptr_eq(&l1, &l2));
}

#[test]
fn empty_take_segment_lists_every_table_with_no_entries() {
    let (mut session, _tables) = full_session();
    let segment = session.take_segment();
    assert!(segment.is_empty());
    assert_eq!(segment.tables().len(), 3);
    assert!(segment.tables().iter().all(|t| t.entries().is_empty()));
    assert_eq!(segment.version(), Segment::VERSION);
}

// --- identity resolution through the environment --------------------------------------

#[test]
fn named_shapes_resolve_against_the_environment() {
    let (mut writer, wt) = full_session();
    let named: Arc<dyn Shape> = Arc::new(NamedShape { name: "circle".into(), area: 7 });
    let shape = writer.intern(wt.shapes, &named).unwrap();
    let segment = writer.take_segment();

    // The reader supplies the environment; the named shape resolves to its Arc.
    let (mut reader, rt) = full_session();
    let circle = Arc::new(NamedShape { name: "circle".into(), area: 7 });
    reader.set_user_data(ShapeEnvironment { shapes: Vec::from([Arc::clone(&circle)]) });
    reader.push_segment(segment).unwrap();
    let resolved = reader.resolve(rt.shapes, shape).unwrap();
    assert_eq!(resolved.area(), 7);

    // Without the shape in the environment, absorption fails cleanly.
    let (mut bare, _bt) = full_session();
    let (mut writer2, wt2) = full_session();
    let named2: Arc<dyn Shape> = Arc::new(NamedShape { name: "square-ish".into(), area: 1 });
    writer2.intern(wt2.shapes, &named2).unwrap();
    bare.set_user_data(ShapeEnvironment::default());
    let err = bare.push_segment(writer2.take_segment()).unwrap_err();
    assert!(matches!(err, DeserializeError::InEntry { .. }));
}

// --- resolver dispatch and memoization ------------------------------------------------

/// A resolver that constructs a reader dynamically and counts its invocations.
struct CountingResolver {
    calls: Arc<AtomicUsize>,
}

impl IdentifierResolver<ToyLang, dyn Shape> for CountingResolver {
    fn resolve(
        &self,
        identifier: &str,
        _cx: &mut DeserializeContext<'_, ToyLang>,
    ) -> Result<Option<ObjectReader<ToyLang, dyn Shape>>, DeserializeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if identifier == "dyn.triangle" {
            Ok(Some(ObjectReader::new(|value, _cx| {
                let side = match value {
                    SerialValue::Int(i) => u32::try_from(*i).unwrap_or(0),
                    _ => return Err(DeserializeError::failed("a triangle is an integer")),
                };
                Ok(Arc::new(DynTriangle { side }) as Arc<dyn Shape>)
            })))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
struct DynTriangle {
    side: u32,
}

impl Shape for DynTriangle {
    fn area(&self) -> u32 {
        self.side
    }
}

impl SerializableObject<ToyLang> for DynTriangle {
    fn serialize_object(
        &self,
        _cx: &mut SerializeContext<'_, ToyLang>,
    ) -> Result<SerialEntry, SerializeError> {
        Ok(SerialEntry {
            identifier: "dyn.triangle".into(),
            data: SerialValue::Int(i64::from(self.side)),
        })
    }
}

#[test]
fn a_resolver_supplies_a_reader_and_is_asked_once_per_identifier() {
    // Write two dynamic-identifier shapes.
    let (mut writer, wt) = full_session();
    let t1: Arc<dyn Shape> = Arc::new(DynTriangle { side: 4 });
    let t2: Arc<dyn Shape> = Arc::new(DynTriangle { side: 6 });
    writer.intern(wt.shapes, &t1).unwrap();
    writer.intern(wt.shapes, &t2).unwrap();
    let segment = writer.take_segment();

    // The reader registers a resolver for the `dyn.` namespace.
    let (mut reader, rt) = full_session();
    let calls = Arc::new(AtomicUsize::new(0));
    rt.shapes
        .register_resolver(&mut reader, "dyn.", Arc::new(CountingResolver { calls: Arc::clone(&calls) }))
        .unwrap();
    reader.push_segment(segment).unwrap();
    let s1 = reader.resolve(rt.shapes, ShapeIndex::from_parts(rt.shapes.id(), 0)).unwrap();
    let s2 = reader.resolve(rt.shapes, ShapeIndex::from_parts(rt.shapes.id(), 1)).unwrap();
    assert_eq!(s1.area(), 4);
    assert_eq!(s2.area(), 6);
    // Both entries share the identifier, so the resolver was asked once — its answer
    // is kept.
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}


// --- the failure battery --------------------------------------------------------------

/// A toy object forming a write-side cycle through interior mutability; its driver
/// holds its own table handle (shared so it can be set after registration).
#[derive(Debug)]
struct Ring {
    next: Mutex<Option<Arc<Ring>>>,
}

crate::serial_index! { pub struct RingIndex; }

struct RingDriver {
    handle: Arc<Mutex<Option<crate::serialize::TableHandle<RingDriver>>>>,
}

impl ObjectSerdeDriver<ToyLang> for RingDriver {
    type Object = Ring;
    type Index = RingIndex;

    fn table_name(&self) -> &'static str {
        "rings"
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        Some("toy.ring")
    }

    fn serialize_object(
        &self,
        ring: &Arc<Ring>,
        cx: &mut SerializeContext<'_, ToyLang>,
    ) -> Result<SerialEntry, SerializeError> {
        let handle = self.handle.lock().unwrap().unwrap();
        let next = ring.next.lock().unwrap().clone();
        let data = match next {
            Some(next) => {
                let index = cx.intern(handle, &next)?;
                SerialValue::Index { table: index.table(), index: index.index() }
            }
            None => SerialValue::Null,
        };
        Ok(SerialEntry { identifier: "toy.ring".into(), data })
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, ToyLang>,
    ) -> Result<Arc<Ring>, DeserializeError> {
        let handle = self.handle.lock().unwrap().unwrap();
        let next = match &entry.data {
            SerialValue::Null => None,
            SerialValue::Index { table, index } => {
                Some(cx.resolve(handle, RingIndex::from_parts(*table, *index))?)
            }
            _ => return Err(DeserializeError::failed("a ring's next is null or a position")),
        };
        Ok(Arc::new(Ring { next: Mutex::new(next) }))
    }
}

/// A session with a single rings table whose driver knows its own handle.
fn ring_session() -> (SerdeSession<ToyLang>, crate::serialize::TableHandle<RingDriver>) {
    let mut session = SerdeSession::<ToyLang>::empty();
    let shared = Arc::new(Mutex::new(None));
    let handle = session.register_table(RingDriver { handle: Arc::clone(&shared) }).unwrap();
    *shared.lock().unwrap() = Some(handle);
    (session, handle)
}

#[test]
fn a_write_cycle_is_detected() {
    let (mut session, handle) = ring_session();
    let a = Arc::new(Ring { next: Mutex::new(None) });
    let b = Arc::new(Ring { next: Mutex::new(Some(Arc::clone(&a))) });
    *a.next.lock().unwrap() = Some(Arc::clone(&b));

    let err = session.intern(handle, &a).unwrap_err();
    let mut current = &err;
    loop {
        match current {
            SerializeError::ReferenceCycle { table, .. } => {
                assert_eq!(*table, "rings");
                break;
            }
            SerializeError::InTable { cause, .. } => current = cause,
            other => panic!("expected a reference cycle, got {other:?}"),
        }
    }
}

#[test]
fn a_reader_cycle_in_a_hostile_segment_is_detected() {
    let (mut session, handle) = ring_session();
    let id = handle.id();
    // Two ring entries referring to each other.
    let entries = Vec::from([
        SerialValue::Index { table: id, index: 1 },
        SerialValue::Index { table: id, index: 0 },
    ]);
    let err = session.push_segment(one_table_segment("rings", id, 0, entries)).unwrap_err();
    assert!(cause_is_cycle(&err), "{err:?}");
}

#[test]
fn a_deep_hostile_chain_hits_the_depth_guard() {
    let (session, handle) = ring_session();
    let mut session = session.with_descent_guard_init(StdDescentGuardInit::depth_limit(8));
    let id = handle.id();
    // A chain of 40 rings, each pointing at the next; the last is null.
    let mut entries = Vec::new();
    for i in 0..40u32 {
        entries.push(SerialValue::Index { table: id, index: i + 1 });
    }
    entries.push(SerialValue::Null);
    let err = session.push_segment(one_table_segment("rings", id, 0, entries)).unwrap_err();
    assert!(cause_is_depth(&err), "{err:?}");
    // The session is still usable after the failed push (rolled back).
    assert_eq!(session.take_segment().tables()[0].entries().len(), 0);
}

#[test]
fn out_of_bounds_and_wrong_table() {
    let (mut session, tables) = full_session();
    session.intern(tables.leaves, &Arc::new(Leaf { text: "x".into() })).unwrap();

    // Out of bounds.
    let beyond = LeafIndex::from_parts(tables.leaves.id(), 5);
    assert!(matches!(
        session.resolve(tables.leaves, beyond),
        Err(DeserializeError::IndexOutOfRange { table: "leaves", index: 5, len: 1 })
    ));
    // Wrong table: a position carrying a table id other than the handle's.
    let mislabeled = LeafIndex::from_parts(TableId::new(9), 0);
    assert!(matches!(
        session.resolve(tables.leaves, mislabeled),
        Err(DeserializeError::WrongTable { expected: "leaves", .. })
    ));
}

#[test]
fn an_unknown_identifier_fails_closed() {
    let (mut reader, rt) = full_session();
    let hostile = one_table_segment(
        "shapes",
        rt.shapes.id(),
        0,
        Vec::from([SerialValue::Map(Vec::from([
            (String::from("id"), SerialValue::Str("toy.unknown".into())),
            (String::from("data"), SerialValue::Null),
        ]))]),
    );
    let err = reader.push_segment(hostile).unwrap_err();
    assert!(matches!(
        cause_of(&err),
        DeserializeError::UnknownIdentifier { identifier, .. } if identifier == "toy.unknown"
    ));
}

#[test]
fn a_declining_resolver_still_fails_closed() {
    // A resolver registered for the namespace but declining the identifier.
    struct Declining;
    impl IdentifierResolver<ToyLang, dyn Shape> for Declining {
        fn resolve(
            &self,
            _identifier: &str,
            _cx: &mut DeserializeContext<'_, ToyLang>,
        ) -> Result<Option<ObjectReader<ToyLang, dyn Shape>>, DeserializeError> {
            Ok(None)
        }
    }
    let (mut reader, rt) = full_session();
    rt.shapes.register_resolver(&mut reader, "toy.", Arc::new(Declining)).unwrap();
    let hostile = one_table_segment(
        "shapes",
        rt.shapes.id(),
        0,
        Vec::from([SerialValue::Map(Vec::from([
            (String::from("id"), SerialValue::Str("toy.mystery".into())),
            (String::from("data"), SerialValue::Null),
        ]))]),
    );
    let err = reader.push_segment(hostile).unwrap_err();
    assert!(matches!(cause_of(&err), DeserializeError::UnknownIdentifier { .. }));
}

#[test]
fn version_unknown_table_and_out_of_order_segments() {
    let (mut reader, _rt) = full_session();

    // Version mismatch.
    let bad_version = Segment::from_serial_value(&SerialValue::Map(Vec::from([
        (String::from("version"), SerialValue::Int(99)),
        (String::from("tables"), SerialValue::List(Vec::new())),
    ])))
    .unwrap();
    assert!(matches!(
        reader.push_segment(bad_version),
        Err(DeserializeError::UnsupportedVersion { found: 99, expected: 1 })
    ));

    // Unknown table name.
    let unknown_table = one_table_segment("not-a-table", TableId::new(0), 0, Vec::new());
    assert!(matches!(
        reader.push_segment(unknown_table),
        Err(DeserializeError::UnknownTableName { .. })
    ));

    // Out of order: a leaves segment starting at position 1 when the table is empty.
    let out_of_order = one_table_segment("leaves", TableId::new(0), 1, Vec::from([SerialValue::Str("x".into())]));
    assert!(matches!(
        reader.push_segment(out_of_order),
        Err(DeserializeError::SegmentOutOfOrder { table: "leaves", expected: 0, found: 1 })
    ));
}

#[test]
fn a_malformed_heterogeneous_entry_shape_is_an_error() {
    let (mut reader, rt) = full_session();
    // A shapes entry that is not the `{id, data}` map.
    let malformed = one_table_segment("shapes", rt.shapes.id(), 0, Vec::from([SerialValue::Int(3)]));
    let err = reader.push_segment(malformed).unwrap_err();
    assert!(matches!(cause_of(&err), DeserializeError::Value(_)));
}

#[test]
fn an_unknown_writer_table_reference_is_an_error() {
    // A leaves segment whose (only) entry references a writer table id not in the
    // directory. Leaves are homogeneous strings, so smuggle the bad index into a
    // node-like heterogeneous table instead — use shapes with a data index.
    let (mut reader, rt) = full_session();
    let entry = SerialValue::Map(Vec::from([
        (String::from("id"), SerialValue::Str("toy.square".into())),
        (String::from("data"), SerialValue::Index { table: TableId::new(7), index: 0 }),
    ]));
    let hostile = one_table_segment("shapes", rt.shapes.id(), 0, Vec::from([entry]));
    assert!(matches!(
        reader.push_segment(hostile),
        Err(DeserializeError::UnknownWriterTable { .. })
    ));
}

#[test]
fn a_failed_push_rolls_back_and_the_session_stays_usable() {
    let (mut session, tables) = full_session();
    // First absorb a good leaf.
    let good = one_table_segment("leaves", tables.leaves.id(), 0, Vec::from([SerialValue::Str("ok".into())]));
    session.push_segment(good).unwrap();
    assert_eq!(session.table_len(0), 1);

    // Now a bad push: two leaves, the second malformed (an integer, not a string).
    let bad = one_table_segment(
        "leaves",
        tables.leaves.id(),
        1,
        Vec::from([SerialValue::Str("new".into()), SerialValue::Int(7)]),
    );
    assert!(session.push_segment(bad).is_err());
    // Rolled back: still one leaf.
    assert_eq!(session.table_len(0), 1);
    // Still usable: a good push after the failure works.
    let more = one_table_segment("leaves", tables.leaves.id(), 1, Vec::from([SerialValue::Str("later".into())]));
    session.push_segment(more).unwrap();
    assert_eq!(session.table_len(0), 2);
    let leaf = session.resolve(tables.leaves, LeafIndex::from_parts(tables.leaves.id(), 1)).unwrap();
    assert_eq!(leaf.text, "later");
}

#[test]
fn unemitted_entries_block_a_push() {
    let (mut session, tables) = full_session();
    session.intern(tables.leaves, &Arc::new(Leaf { text: "pending".into() })).unwrap();
    let more = one_table_segment("leaves", tables.leaves.id(), 1, Vec::from([SerialValue::Str("x".into())]));
    assert!(matches!(
        session.push_segment(more),
        Err(DeserializeError::UnemittedEntries { table: "leaves" })
    ));
}

// --- determinism ----------------------------------------------------------------------

#[test]
fn two_sessions_building_the_same_graph_emit_equal_segments() {
    fn build() -> Segment {
        let (mut session, tables) = full_session();
        let leaf1 = Arc::new(Leaf { text: "a".into() });
        let leaf2 = Arc::new(Leaf { text: "b".into() });
        let square: Arc<dyn Shape> = Arc::new(Square { side: 4 });
        let node = Arc::new(Node {
            leaves: Vec::from([
                session.intern(tables.leaves, &leaf1).unwrap(),
                session.intern(tables.leaves, &leaf2).unwrap(),
            ]),
            shape: session.intern(tables.shapes, &square).unwrap(),
        });
        session.intern(tables.nodes, &node).unwrap();
        session.take_segment()
    }
    assert_eq!(build(), build());
}

// --- segment serialized form ----------------------------------------------------------

#[test]
fn a_segment_round_trips_through_its_serial_value() {
    let (mut session, tables) = full_session();
    session.intern(tables.leaves, &Arc::new(Leaf { text: "x".into() })).unwrap();
    let square: Arc<dyn Shape> = Arc::new(Square { side: 2 });
    session.intern(tables.shapes, &square).unwrap();
    let segment = session.take_segment();
    let value = segment.to_serial_value();
    let back = Segment::from_serial_value(&value).unwrap();
    assert_eq!(back, segment);
}

// --- small helpers building hostile segments by hand ----------------------------------

/// A one-table segment (the table named `name`, with the writer-side id `id`, its
/// entries starting at `start`), for hand-built hostile input. A reader matches its
/// tables by name, so listing just the one is enough.
fn one_table_segment(name: &str, id: TableId, start: u32, entries: Vec<SerialValue>) -> Segment {
    let table = SerialValue::Map(Vec::from([
        (String::from("name"), SerialValue::Str(String::from(name))),
        (String::from("id"), SerialValue::Int(i64::from(id.ordinal()))),
        (String::from("start"), SerialValue::Int(i64::from(start))),
        (String::from("entries"), SerialValue::List(entries)),
    ]));
    Segment::from_serial_value(&SerialValue::Map(Vec::from([
        (String::from("version"), SerialValue::Int(i64::from(Segment::VERSION))),
        (String::from("tables"), SerialValue::List(Vec::from([table]))),
    ])))
    .unwrap()
}

fn cause_of(err: &DeserializeError) -> &DeserializeError {
    match err {
        DeserializeError::InEntry { cause, .. } => cause_of(cause),
        other => other,
    }
}

fn cause_is_cycle(err: &DeserializeError) -> bool {
    matches!(cause_of(err), DeserializeError::ReferenceCycle { .. })
}

fn cause_is_depth(err: &DeserializeError) -> bool {
    matches!(cause_of(err), DeserializeError::DescentLimitExceeded { .. })
}
