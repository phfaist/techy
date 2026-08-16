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
/// variants (every integer is an [`Int`](SerialValue::Int)); map keys are strings;
/// maps preserve insertion order; and the two variants without a native JSON form,
/// [`Bytes`](SerialValue::Bytes) and [`Index`](SerialValue::Index), render as
/// reserved object shapes that no other value can produce.
///
/// [`Index`](SerialValue::Index) is a reference to an object stored in a numbered
/// table: `table` names the table, `index` the position within it. Shared objects
/// are written into tables once and referred to by such indices, so identity and
/// sharing survive a round trip.
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
/// ordinal, then the position). A map key beginning with `$` is written with one
/// extra leading `$` (`"$foo"` → `"$$foo"`) and unescaped on reading; on reading, an
/// object key beginning with `$` that is neither a reserved key nor `$$`-escaped is
/// an error, as are floating-point numbers, integers outside `i64`, and malformed
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
    /// A string-keyed map, in insertion order. Keys are expected to be unique;
    /// the value model itself does not enforce uniqueness.
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
/// it: when a segment is read by another session, its table references are
/// translated by table *name* (see [`SerdeSession::push_segment`](crate::serialize::SerdeSession::push_segment)).
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
/// the driver of that table. Positions are minted only by the session (a
/// [`SerializeContext::intern`](crate::serialize::SerializeContext::intern) call
/// returns one), so a position's two parts are always consistent on the writing side;
/// on the reading side, the session validates them.
pub trait SerialIndex: Copy + Eq + core::hash::Hash + core::fmt::Debug + Send + Sync + 'static {
    /// The position `index` of table `table`.
    fn from_parts(table: TableId, index: u32) -> Self;

    /// The table this position refers into.
    fn table(self) -> TableId;

    /// The position within the table, from `0`.
    fn index(self) -> u32;
}
