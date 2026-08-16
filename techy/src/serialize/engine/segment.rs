//! [`Segment`]: the unit a session emits and absorbs — the entries new since the
//! previous emission, table by table — and its serialized form.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use super::super::error::SerialValueError;
use super::super::value::{SerialValue, TableId};
use super::super::wire::{FromSerialValue, ToSerialValue};

/// A segment: what one [`take_segment`](crate::serialize::SerdeSession::take_segment)
/// call emits and one [`push_segment`](crate::serialize::SerdeSession::push_segment)
/// call absorbs — for every table of the emitting session, the entries interned into
/// it since the previous emission, together with the position they start at. A
/// *stream* is the sequence of segments one session emits; positions are scoped to
/// the stream, so a later segment's entries refer to earlier segments' entries by
/// position, and a reading session absorbs the segments of one stream only, in
/// order (the session checks that each segment continues its tables, but it cannot
/// tell a foreign stream's segment apart from the right one when the positions
/// happen to line up — the obligation is the caller's). Inside a segment a position
/// is a `u32` index scoped to the stream, paired with the writer's [`TableId`],
/// which the reading session translates by table name; a typed position in Rust
/// code (a [`SerialIndex`](crate::serialize::SerialIndex) value) is scoped further,
/// to the session holding it.
///
/// Every segment is self-describing: it carries the [`version`](Segment::version)
/// of the layout it uses (a reading session accepts exactly
/// [`VERSION`](Segment::VERSION)) and its *table directory* — every table of the
/// emitting session, in registration order, by name (how the reading session finds
/// its own table, whatever its registration order), with the writer's table id (how
/// the table references inside the entries are translated) and the start position.
/// The segments of a stream are independently valid values, so a stream can be
/// stored or sent as one segment per file, message, or line.
///
/// # Serialized form
///
/// [`to_serial_value`](Segment::to_serial_value) / [`from_serial_value`](Segment::from_serial_value)
/// convert a segment to and from a [`SerialValue`] — the map
/// `{"version": <int>, "tables": [<table>, …]}`, each table the map `{"name": <str>,
/// "id": <int>, "start": <int>, "entries": [<value>, …]}` — with the key names
/// provisional until the vocabulary of the serialized form is finalized. An entry of
/// a table holding objects of one kind only is the entry's data itself; an entry of
/// any other table is the map `{"id": <identifier>, "data": <value>}`. With the
/// `serde` cargo feature the type implements `Serialize` and `Deserialize` by
/// rendering that `SerialValue` (see [`SerialValue`]'s rendering), so a segment
/// encodes through any serde format; in JSON, one segment per line is the canonical
/// stream rendering (each line an independently valid segment; the stream ends with
/// the input).
#[derive(Clone, Debug, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub struct Segment {
    #[serial(name = "version")]
    version: u32,
    #[serial(name = "tables")]
    tables: Vec<SegmentTable>,
}

/// One table's part of a [`Segment`]: the table's name and writer-side id, the
/// position its entries start at, and the entries themselves in position order.
#[derive(Clone, Debug, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub struct SegmentTable {
    #[serial(name = "name")]
    name: String,
    #[serial(name = "id")]
    id: TableId,
    #[serial(name = "start")]
    start: u32,
    #[serial(name = "entries")]
    entries: Vec<SerialValue>,
}

/// The stored form of an entry of a heterogeneous table: the identifier and the
/// data (a homogeneous table stores the bare data).
#[derive(ToSerialValue, FromSerialValue)]
pub(super) struct WireEntry {
    #[serial(name = "id")]
    pub(super) identifier: Cow<'static, str>,
    #[serial(name = "data")]
    pub(super) data: SerialValue,
}

impl Segment {
    /// The version of the segment layout this crate writes and reads. A segment
    /// declaring any other version is rejected.
    pub const VERSION: u32 = 1;

    /// A segment of the given version and tables (the session builds them).
    pub(super) fn new(version: u32, tables: Vec<SegmentTable>) -> Segment {
        Segment { version, tables }
    }

    /// The version of the layout the segment declares.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The tables, in the emitting session's registration order.
    pub fn tables(&self) -> &[SegmentTable] {
        &self.tables
    }

    /// Whether the segment carries no entries at all (every table empty).
    pub fn is_empty(&self) -> bool {
        self.tables.iter().all(|table| table.entries.is_empty())
    }

    /// The segment's serialized form (see the type documentation).
    pub fn to_serial_value(&self) -> SerialValue {
        // A segment's fields — `u32`s, strings, lists, values — are always representable.
        ToSerialValue::to_serial_value(self)
            .expect("a segment's fields are always representable as a SerialValue")
    }

    /// Read a segment from its serialized form (see the type documentation). The value
    /// is untrusted input: a value of the wrong shape is an error. Reading validates
    /// the shape only; the version and the contents are validated when the segment is
    /// pushed into a session.
    ///
    /// # Errors
    ///
    /// The [`SerialValueError`] describing the shape mismatch.
    pub fn from_serial_value(value: &SerialValue) -> Result<Segment, SerialValueError> {
        FromSerialValue::from_serial_value(value)
    }

    /// Take the segment apart.
    pub(super) fn into_parts(self) -> (u32, Vec<SegmentTable>) {
        (self.version, self.tables)
    }
}

impl SegmentTable {
    /// A table's part of a segment (the session builds them).
    pub(super) fn new(name: String, id: TableId, start: u32, entries: Vec<SerialValue>) -> SegmentTable {
        SegmentTable { name, id, start, entries }
    }

    /// The table's name (its driver's
    /// [`table_name`](crate::serialize::ObjectSerdeDriver::table_name)).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The table's id in the emitting session's numbering — what the table
    /// references inside this segment's entries use.
    pub fn id(&self) -> TableId {
        self.id
    }

    /// The position the first entry of this segment's part has in the table.
    pub fn start(&self) -> u32 {
        self.start
    }

    /// The entries, in position order, in their stored form (see [`Segment`]).
    pub fn entries(&self) -> &[SerialValue] {
        &self.entries
    }

    /// Take the part apart.
    pub(super) fn into_parts(self) -> (String, TableId, u32, Vec<SerialValue>) {
        (self.name, self.id, self.start, self.entries)
    }
}

#[cfg(feature = "serde")]
mod serde_impls {
    //! `Serialize`/`Deserialize` for [`Segment`]: one rendering path — the segment's
    //! `SerialValue` form, rendered by `SerialValue`'s own impls.

    use serde::de::{self, Deserialize, Deserializer};
    use serde::ser::{Serialize, Serializer};

    use super::super::super::value::SerialValue;
    use super::Segment;

    /// Available with the `serde` cargo feature: renders
    /// [`to_serial_value`](Segment::to_serial_value) through `SerialValue`'s
    /// `Serialize` impl.
    impl Serialize for Segment {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.to_serial_value().serialize(serializer)
        }
    }

    /// Available with the `serde` cargo feature: reads a `SerialValue` through its
    /// `Deserialize` impl, then [`from_serial_value`](Segment::from_serial_value).
    /// Everything read is untrusted input: a malformed rendering or shape is an
    /// error.
    impl<'de> Deserialize<'de> for Segment {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = SerialValue::deserialize(deserializer)?;
            Segment::from_serial_value(&value).map_err(de::Error::custom)
        }
    }
}
