//! The wire-side value model: [`SerialValue`], [`SerialEntry`], [`TableId`], and the
//! [`SerialIndex`] bound. Type-blind: nothing here names a source, a state, or a
//! spec — every object kind is written in these terms alike.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

/// A serialized value: the in-memory, format-independent form that every
/// serialization produces and every deserialization reads.
///
/// The variant set is deliberately small so that every value has exactly one JSON
/// rendering (JSON is the format the public serialization contract is stated in),
/// and the rendering is designed so that two values render identically exactly when
/// they compare equal: there are no floating-point numbers and no sized-integer
/// variants (every integer is an [`Int`](SerialValue::Int)); map keys are strings
/// and never begin with `$`; a [`Map`](SerialValue::Map) is an ordered list of
/// entries — equality is order-sensitive and the rendering preserves the order, so
/// two maps with the same entries in different orders are different values that
/// render differently; and the two variants without a native JSON form,
/// [`Bytes`](SerialValue::Bytes) and [`Index`](SerialValue::Index), render as
/// reserved object shapes (keys beginning with `$`) that no other value can produce.
///
/// [`Index`](SerialValue::Index) is a reference to an object stored in a numbered
/// table: `table` names the table, `index` the position within it. Shared objects
/// are written into tables once and referred to by such indices, so identity and
/// sharing survive a round trip.
///
/// # Nesting depth
///
/// A value's *nesting depth* is the number of lists and maps that enclose its most
/// deeply nested part ([`nesting_depth`](SerialValue::nesting_depth)); it is bounded
/// by [`MAX_NESTING_DEPTH`](SerialValue::MAX_NESTING_DEPTH) at every point where a
/// value enters or leaves a session or a serialized form — read through serde,
/// converted to or from a [`Segment`](crate::serialize::Segment), interned into a
/// session — so that a hostile input cannot make a reader recurse without limit.
/// See the constant.
///
/// # Rendering through serde
///
/// With the `serde` cargo feature the type implements `Serialize` and `Deserialize`.
/// Through a human-readable format (serde's `is_human_readable()`), the rendering is
/// the canonical one — provisional until the vocabulary of the serialized form (its
/// key names and enum strings) is finalized, and stated here for JSON: `Null` → `null`, `Bool` → boolean, `Int` → number, `Str` → string,
/// `List` → array, `Map` → object in entry order; `Bytes` → the one-entry object
/// `{"$bytes": "<base64>"}` (standard alphabet, `=` padding, no line breaks); `Index`
/// → the one-entry object `{"$index": [<table>, <index>]}` (two integers: the table's
/// ordinal, then the position). Those two keys are the only keys beginning with `$`
/// the rendering ever writes: a `Map` holding a key that begins with `$` is a
/// rendering error (there is no escaping — the prefix is reserved), and on reading,
/// an object key beginning with `$` that is not one of the two reserved forms is an
/// error, as are floating-point numbers, integers outside `i64`, and malformed
/// reserved objects. Through any other format the rendering is a compact one: the
/// externally tagged form of this enum (the variant name, then its data), `Bytes`
/// through the format's `serialize_bytes`/`deserialize_bytes` methods, `Index` as the
/// two-integer pair, `Map` as a serde map. Both renderings read back to the identical
/// value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SerialValue {
    /// The absent value.
    Null,
    /// A boolean.
    Bool(bool),
    /// An integer. The only numeric variant: integers of every width are represented
    /// as `i64` (a value that does not fit is a serialization error, never a silent
    /// truncation).
    Int(i64),
    /// A string.
    Str(String),
    /// A byte string (rendered as base64 text in JSON).
    Bytes(Vec<u8>),
    /// An ordered sequence of values.
    List(Vec<SerialValue>),
    /// A string-keyed map: an ordered list of entries. The order is part of the
    /// value — equality is order-sensitive and the rendering preserves the order —
    /// so two maps with the same entries in different orders are different values
    /// that render differently. Keys are expected to be unique and must not begin
    /// with `$` (the prefix is reserved for the rendering's own objects; a key that
    /// begins with it is a rendering or bridge error); the value model itself
    /// enforces neither at construction.
    Map(Vec<(String, SerialValue)>),
    /// A reference to the object at position `index` of the table `table`.
    Index {
        /// The table the referenced object is stored in.
        table: TableId,
        /// The referenced object's position within that table.
        index: u32,
    },
}

impl SerialValue {
    /// The greatest nesting depth a serialized value may have: the number of lists
    /// and maps enclosing any part of the value, [`nesting_depth`](SerialValue::nesting_depth),
    /// is at most this. The bound is enforced wherever a value crosses the
    /// serialization boundary: reading a value through serde (the `Deserialize`
    /// impls of `SerialValue` and of [`Segment`](crate::serialize::Segment), with the
    /// `serde` cargo feature), converting a segment from its serialized form
    /// ([`Segment::from_serial_value`](crate::serialize::Segment::from_serial_value)),
    /// absorbing a segment ([`SerdeSession::push_segment`](crate::serialize::SerdeSession::push_segment)),
    /// and interning an object ([`SerdeSession::intern`](crate::serialize::SerdeSession::intern)) —
    /// a deeper value is a [`SerialValueError::NestingTooDeep`](crate::serialize::SerialValueError::NestingTooDeep)
    /// error there, so that a hostile input cannot exhaust the stack of a reader
    /// that walks values recursively, and so that a writer learns at once when an
    /// entry would be unreadable. A segment's serialized form counts as one value:
    /// its own structure — the segment map, its table list, a table's map, its entry
    /// list — takes four levels, so an entry's stored form may nest at most four
    /// levels less than this (a table that stores identifiers with the data wraps
    /// each entry in one more map).
    ///
    /// The bound is well above what any serialized object of this crate needs (a
    /// tree's entry nests about ten levels), and below the default recursion limit
    /// of the `serde_json` reader (128), so that the same bound is in force whatever
    /// the format.
    pub const MAX_NESTING_DEPTH: usize = 64;

    /// The value's nesting depth: the number of lists and maps that enclose its most
    /// deeply nested part — `0` for a value that is not a list or a map, `1` for a
    /// list or map holding no list or map, and so on (a list holding one list holding
    /// one integer — `[[1]]` in JSON — has depth `2`). Computed
    /// without recursion, so any value can be measured. Compare with
    /// [`MAX_NESTING_DEPTH`](SerialValue::MAX_NESTING_DEPTH).
    pub fn nesting_depth(&self) -> usize {
        let mut deepest = 0;
        let mut pending: Vec<(&SerialValue, usize)> = Vec::from([(self, 0)]);
        while let Some((value, enclosing)) = pending.pop() {
            let inner = enclosing + 1;
            match value {
                SerialValue::List(items) => {
                    deepest = deepest.max(inner);
                    pending.extend(items.iter().map(|item| (item, inner)));
                }
                SerialValue::Map(entries) => {
                    deepest = deepest.max(inner);
                    pending.extend(entries.iter().map(|(_, item)| (item, inner)));
                }
                SerialValue::Null
                | SerialValue::Bool(_)
                | SerialValue::Int(_)
                | SerialValue::Str(_)
                | SerialValue::Bytes(_)
                | SerialValue::Index { .. } => {}
            }
        }
        deepest
    }

    /// The name of this value's kind — `null`, `bool`, `int`, `str`, `bytes`, `list`,
    /// `map`, or `index` — as error messages report it (the `found` of a
    /// [`SerialValueError::TypeMismatch`](crate::serialize::SerialValueError::TypeMismatch)).
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            SerialValue::Null => "null",
            SerialValue::Bool(_) => "bool",
            SerialValue::Int(_) => "int",
            SerialValue::Str(_) => "str",
            SerialValue::Bytes(_) => "bytes",
            SerialValue::List(_) => "list",
            SerialValue::Map(_) => "map",
            SerialValue::Index { .. } => "index",
        }
    }
}

/// The result of serializing one object: the object's identifier and its data.
///
/// The `identifier` names the kind of object `data` describes: a deliberately chosen,
/// stable string owned by whoever defines the object type — never a Rust type name.
/// A `Cow<'static, str>` so that the common case, a string literal, costs nothing,
/// while a type whose identifier depends on the instance can supply an owned string.
/// Some tables hold objects of one kind only and do not write the identifier out;
/// even then the entry carries a real, non-empty identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialEntry {
    /// The identifier of the kind of object `data` describes.
    pub identifier: Cow<'static, str>,
    /// The serialized object.
    pub data: SerialValue,
}

/// The ordinal of a table: which numbered table a [`SerialValue::Index`] refers to.
///
/// Tables are numbered in the order they are registered with a
/// [`SerdeSession`](crate::serialize::SerdeSession), deterministically; user code
/// receives `TableId`s (inside typed table positions and on
/// [`TableHandle`](crate::serialize::TableHandle)s) and passes them along but never
/// mints them. A `TableId` is meaningful only relative to the session that assigned
/// it — two sessions that register the same tables in different orders number them
/// differently. Inside a segment, table references carry the writing session's ids
/// and are translated by table *name* when another session absorbs the segment (see
/// [`SerdeSession::push_segment`](crate::serialize::SerdeSession::push_segment)); a
/// typed position held in Rust code is not translated, so it is exchanged between
/// sessions as its table's name and its `u32` index (see
/// [`SerialIndex`](crate::serialize::SerialIndex)).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(u32);

impl TableId {
    /// A table id with the given ordinal. Crate-internal: table ids are assigned by
    /// the session that registers tables, in registration order.
    pub(crate) fn new(ordinal: u32) -> TableId {
        TableId(ordinal)
    }

    /// The table's ordinal: its position in the registration order of the session
    /// that assigned it, from `0`.
    pub fn ordinal(self) -> u32 {
        self.0
    }
}

/// The bound satisfied by every typed table position — the `Index` type of an
/// [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver): a `Copy` value
/// carrying both the [`TableId`] of its table and the `u32` position within it, the
/// same two parts a [`SerialValue::Index`] holds, so that a position in one table
/// cannot be mistaken for a position in another and so that its serialized form
/// needs no further context.
///
/// Position types are defined with the [`serial_index!`](crate::serialize::serial_index)
/// macro, which supplies this impl together with the conversions to and from the
/// serialized form; each kind of table has its own position type, defined next to
/// the driver of that table. Positions are minted by the session (a
/// [`SerializeContext::intern`](crate::serialize::SerializeContext::intern) call
/// returns one) or rebuilt from a bare index with
/// [`TableHandle::position`](crate::serialize::TableHandle::position); on the
/// reading side, the session validates both parts.
///
/// **A typed position is scoped to the session that minted it.** Its `TableId` is
/// that session's, and two sessions that register the same tables in different
/// orders number them differently: a position minted by a writing session, handed as
/// it is to a reading session, names the wrong table there
/// ([`DeserializeError::WrongTable`](crate::serialize::DeserializeError::WrongTable)).
/// Inside a segment, positions are `u32` indices scoped to the stream, with table
/// references the reading session translates by table name — that is why references
/// inside entries need no care; a position held in Rust code and passed from one
/// session to another is exchanged as its table's name
/// ([`ObjectSerdeDriver::table_name`](crate::serialize::ObjectSerdeDriver::table_name))
/// and its `u32` index ([`index`](SerialIndex::index)), and rebuilt on the receiving
/// side with `TableHandle::position`.
pub trait SerialIndex: Copy + Eq + core::hash::Hash + core::fmt::Debug + Send + Sync + 'static {
    /// The position `index` of table `table`.
    fn from_parts(table: TableId, index: u32) -> Self;

    /// The table this position refers into.
    fn table(self) -> TableId;

    /// The position within the table, from `0`.
    fn index(self) -> u32;
}
