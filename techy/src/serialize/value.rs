//! The wire-side value model: [`SerialValue`], [`SerialEntry`], [`TableId`], and the
//! [`SerialIndex`] bound. Type-blind: nothing here names a source, a state, or a
//! spec — every object kind is written in these terms alike.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

/// A serialized value: the in-memory, format-independent form that every
/// serialization produces and every deserialization reads.
///
/// The variant set is deliberately small so that every value has exactly one
/// rendering in the canonical JSON form, and two values render identically exactly
/// when they compare equal: there are no floating-point numbers and no sized-integer
/// variants (every integer is an [`Int`](SerialValue::Int)); map keys are strings;
/// maps preserve insertion order.
///
/// [`Index`](SerialValue::Index) is a reference to an object stored in a numbered
/// table: `table` names the table, `index` the position within it. Shared objects
/// are written into tables once and referred to by such indices, so identity and
/// sharing survive a round trip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SerialValue {
    /// The absent value.
    Null,
    /// A boolean.
    Bool(bool),
    /// An integer. The only numeric variant: every integer width is written as an
    /// `i64` (a value that does not fit is a serialization error, never a
    /// silent truncation).
    Int(i64),
    /// A string.
    Str(String),
    /// A byte string (rendered as base64 in the canonical JSON form).
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
/// Tables are numbered in the order they are registered, deterministically, by the
/// machinery that manages them; user code receives `TableId`s and passes them along
/// but never mints them.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(u32);

impl TableId {
    /// A table id with the given ordinal. Crate-internal: table ids are assigned by
    /// the machinery that registers tables, in registration order.
    // No in-crate caller yet: the session that registers tables (M2) is the minter.
    #[allow(dead_code)]
    pub(crate) fn new(ordinal: u32) -> TableId {
        TableId(ordinal)
    }
}

/// The bound satisfied by every typed table position: a `Copy` value that can be
/// compared, hashed, and printed. Each kind of table has its own position type
/// (a `u32` newtype), so that a position in one table cannot be mistaken for a
/// position in another.
pub trait SerialIndex: Copy + Eq + core::hash::Hash + core::fmt::Debug {}
