//! [`Segment`]: the unit a session emits and absorbs — the entries new since the
//! previous emission, table by table — and its serialized form.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use super::super::error::SerialValueError;
use super::super::value::{SerialValue, TableId};
use super::super::wire::{index_from_serial_value, FromSerialValue, ToSerialValue};

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
/// [`VERSION`](Segment::VERSION)), its [`meta`](Segment::meta) — information about
/// the segment as a whole, today the emitting session's *profile* (see
/// [`SegmentMeta`]) — and its *table directory* — every table of the
/// emitting session, in registration order, by name (how the reading session finds
/// its own table, whatever its registration order), with the writer's table id (how
/// the table references inside the entries are translated) and the start position.
/// A segment may also name its [`main`](Segment::main) entry: the position of the
/// one entry the segment is about (the parse result of a line in a stream of parse
/// results, say), so that a reader need not know the layout of the tables to find
/// it — [`push_segment`](crate::serialize::SerdeSession::push_segment) returns it
/// translated into the reading session's numbering. The segments of a stream are
/// independently valid values, so a stream can be stored or sent as one segment per
/// file, message, or line.
///
/// # Serialized form
///
/// [`to_serial_value`](Segment::to_serial_value) / [`from_serial_value`](Segment::from_serial_value)
/// convert a segment to and from a [`SerialValue`] — the map
/// `{"version": <int>, "meta": {"profile"?: <str>}, "tables": [<table>, …], "main"?:
/// <table position>}` (`meta` always present, its keys optional; `main` omitted when
/// none), each table the map `{"name": <str>, "table": <int>, "start": <int>,
/// "entries": [<value>, …]}` (`table` being the writer's table id, the ordinal every
/// table reference inside the entries uses) — with the key names
/// provisional until the vocabulary of the serialized form is finalized. An entry of
/// a table holding objects of one kind only is the entry's data itself; an entry of
/// any other table is the map `{"identifier": <identifier>, "data": <value>}`. With the
/// `serde` cargo feature the type implements `Serialize` and `Deserialize` by
/// rendering that `SerialValue` (see [`SerialValue`]'s rendering), so a segment
/// encodes through any serde format; in JSON, one segment per line is the canonical
/// stream rendering (each line an independently valid segment; the stream ends with
/// the input).
#[derive(Clone, Debug, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub struct Segment {
    #[serial(name = "version")]
    version: u32,
    #[serial(name = "meta")]
    meta: SegmentMeta,
    #[serial(name = "tables")]
    tables: Vec<SegmentTable>,
    #[serial(name = "main")]
    main: Option<WireMain>,
}

/// What a [`Segment`] says about itself as a whole, as opposed to its entries: today
/// the emitting session's *profile* — the caller-chosen string naming what can read
/// the stream fully (see [`SerdeSession::set_profile`](crate::serialize::SerdeSession::set_profile)),
/// when the session declared one. The object is always present in a segment's
/// serialized form (empty when nothing is set), so that later layouts can add
/// segment-wide information to it.
#[derive(Clone, Debug, Default, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub struct SegmentMeta {
    #[serial(name = "profile")]
    profile: Option<String>,
}

impl SegmentMeta {
    /// Segment metadata carrying `profile` (the session builds it).
    pub(super) fn new(profile: Option<String>) -> SegmentMeta {
        SegmentMeta { profile }
    }

    /// The emitting session's profile, when it declared one.
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

/// The wire form of a segment's main entry: a table position (`{"$index": [table,
/// index]}` in the canonical rendering), in the writer's numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WireMain {
    table: TableId,
    index: u32,
}

impl ToSerialValue for WireMain {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Index { table: self.table, index: self.index })
    }
}

impl FromSerialValue for WireMain {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        let (table, index) = index_from_serial_value(value)?;
        Ok(WireMain { table, index })
    }
}

/// One table's part of a [`Segment`]: the table's name and writer-side id, the
/// position its entries start at, and the entries themselves in position order.
#[derive(Clone, Debug, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub struct SegmentTable {
    #[serial(name = "name")]
    name: String,
    #[serial(name = "table")]
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
    #[serial(name = "identifier")]
    pub(super) identifier: Cow<'static, str>,
    #[serial(name = "data")]
    pub(super) data: SerialValue,
}

impl Segment {
    /// The version of the segment layout this crate writes and reads. A segment
    /// declaring any other version is rejected.
    pub const VERSION: u32 = 1;

    /// A segment of the given version, metadata, tables, and main entry (the session
    /// builds them).
    pub(super) fn new(
        version: u32,
        meta: SegmentMeta,
        tables: Vec<SegmentTable>,
        main: Option<(TableId, u32)>,
    ) -> Segment {
        Segment { version, meta, tables, main: main.map(|(table, index)| WireMain { table, index }) }
    }

    /// The version of the layout the segment declares.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// What the segment says about itself as a whole (its profile, when the emitting
    /// session declared one).
    pub fn meta(&self) -> &SegmentMeta {
        &self.meta
    }

    /// The tables, in the emitting session's registration order.
    pub fn tables(&self) -> &[SegmentTable] {
        &self.tables
    }

    /// The segment's main entry, when the emitting session named one
    /// ([`take_segment_with_main`](crate::serialize::SerdeSession::take_segment_with_main)):
    /// the entry's table id **in the emitting session's numbering** (the same numbering
    /// as the directory's `table` ids and the references inside the entries) and its
    /// position — as the segment carries it, untranslated. A reading session gets the
    /// translated pair from [`push_segment`](crate::serialize::SerdeSession::push_segment).
    pub fn main(&self) -> Option<(TableId, u32)> {
        self.main.map(|main| (main.table, main.index))
    }

    /// Whether the segment carries no entries at all (every table empty).
    pub fn is_empty(&self) -> bool {
        self.tables.iter().all(|table| table.entries.is_empty())
    }

    /// The segment's serialized form (see the type documentation).
    pub fn to_serial_value(&self) -> SerialValue {
        // Invariant: the conversion fails only for an integer outside `i64`, and a
        // segment's integers are `u32`s — so it cannot fail here.
        ToSerialValue::to_serial_value(self)
            .expect("a segment's fields (u32s, strings, lists, values) always convert to a SerialValue")
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

    /// Take the segment apart: version, metadata, tables, main entry.
    pub(super) fn into_parts(self) -> (u32, SegmentMeta, Vec<SegmentTable>, Option<(TableId, u32)>) {
        let main = self.main();
        (self.version, self.meta, self.tables, main)
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
