//! serde `Serialize`/`Deserialize` for [`SerialValue`] (cargo feature `serde`): how a
//! serialized value renders through a serde format.
//!
//! Two renderings, chosen by the format's `is_human_readable()`:
//!
//! - **The canonical rendering** (human-readable formats, JSON being the one the public
//!   contract is stated in — not yet frozen, see the module docs' "Stability of the
//!   serialized form"):
//!   `Null` → `null`, `Bool` → boolean, `Int` → number, `Str` → string, `List` →
//!   array, `Map` → object in entry order. The two variants without a native JSON form
//!   render as reserved one-entry objects: `Bytes` → `{"$bytes": "<base64>"}` (standard
//!   alphabet, `=` padding, no line breaks); `Index` → `{"$index": [<table>, <index>]}`
//!   (the table's ordinal and the position, two integers). So that no map can be
//!   mistaken for those forms, the value model reserves every key beginning with `$`:
//!   a map holding such a key is a rendering error (there is no escaping), and on
//!   reading, an object key beginning with `$` that is not one of the two reserved
//!   forms is an error.
//! - **The compact rendering** (every other format): serde's externally tagged form of
//!   the enum — variant index and name, then the payload; `Bytes` through the format's
//!   `serialize_bytes`/`deserialize_bytes` methods, `Index` as the two-integer pair,
//!   `Map` as a serde map.
//!
//! Both renderings enforce the reserved-key rule, and both read back to the identical
//! value. Through the bridge (`to_value` /
//! `from_value`, `bridge.rs`), a `SerialValue` converts to itself unchanged: the impls
//! wrap the value in a newtype struct named [`VALUE_SENTINEL`] that the bridge
//! intercepts.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::de::{
    self, Deserialize, DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor,
};
use serde::ser::{self, Serialize, SerializeMap, Serializer};

use super::base64;
use super::bridge::{deserialize_index, serial_bytes, serialize_index};
use super::error::SerialValueError;
use super::value::{SerialValue, TableId};

/// The newtype-struct name `SerialValue`'s serde impls wrap the value in, and the enum
/// name of its compact rendering. The bridge intercepts both.
pub(crate) const VALUE_SENTINEL: &str = "techy::serialize::SerialValue";

/// The variant names of the compact rendering, in variant-index order.
pub(crate) const COMPACT_VARIANTS: &[&str] =
    &["Null", "Bool", "Int", "Str", "Bytes", "List", "Map", "Index"];

/// The compact-rendering variant name of `value`'s kind.
pub(crate) fn compact_variant_name(value: &SerialValue) -> &'static str {
    // Invariant: the index is one of 0..8, the positions of `COMPACT_VARIANTS`.
    COMPACT_VARIANTS[compact_variant_index(value) as usize]
}

/// The compact-rendering variant index of `value`'s kind: its position in
/// [`COMPACT_VARIANTS`], always within it.
fn compact_variant_index(value: &SerialValue) -> u32 {
    match value {
        SerialValue::Null => 0,
        SerialValue::Bool(_) => 1,
        SerialValue::Int(_) => 2,
        SerialValue::Str(_) => 3,
        SerialValue::Bytes(_) => 4,
        SerialValue::List(_) => 5,
        SerialValue::Map(_) => 6,
        SerialValue::Index { .. } => 7,
    }
}

/// The reserved key of the canonical rendering's byte-string object.
const BYTES_KEY: &str = "$bytes";
/// The reserved key of the canonical rendering's table-position object.
const INDEX_KEY: &str = "$index";

// --- Serialize ------------------------------------------------------------------------------

/// Available with the `serde` cargo feature. Renders in the canonical form through a
/// human-readable format and in the compact form otherwise; see the type's
/// documentation for both forms.
impl Serialize for SerialValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // JSON and the common binary formats render the wrapper as its content; the
        // bridge intercepts it.
        serializer.serialize_newtype_struct(VALUE_SENTINEL, &Rendering(self))
    }
}

/// The value inside the sentinel wrapper: chooses the rendering by the format.
struct Rendering<'a>(&'a SerialValue);

impl Serialize for Rendering<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serialize_canonical(self.0, serializer)
        } else {
            serialize_compact(self.0, serializer)
        }
    }
}

fn serialize_canonical<S: Serializer>(value: &SerialValue, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        SerialValue::Null => serializer.serialize_unit(),
        SerialValue::Bool(b) => serializer.serialize_bool(*b),
        SerialValue::Int(i) => serializer.serialize_i64(*i),
        SerialValue::Str(s) => serializer.serialize_str(s),
        SerialValue::Bytes(bytes) => {
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry(BYTES_KEY, &base64::encode(bytes))?;
            map.end()
        }
        SerialValue::List(items) => serializer.collect_seq(items),
        SerialValue::Map(entries) => {
            let mut map = serializer.serialize_map(Some(entries.len()))?;
            serialize_entries(&mut map, entries)?;
            map.end()
        }
        SerialValue::Index { table, index } => {
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry(INDEX_KEY, &(table.ordinal(), *index))?;
            map.end()
        }
    }
}

fn serialize_compact<S: Serializer>(value: &SerialValue, serializer: S) -> Result<S::Ok, S::Error> {
    let index = compact_variant_index(value);
    let name = compact_variant_name(value);
    match value {
        SerialValue::Null => serializer.serialize_unit_variant(VALUE_SENTINEL, index, name),
        SerialValue::Bool(b) => serializer.serialize_newtype_variant(VALUE_SENTINEL, index, name, b),
        SerialValue::Int(i) => serializer.serialize_newtype_variant(VALUE_SENTINEL, index, name, i),
        SerialValue::Str(s) => serializer.serialize_newtype_variant(VALUE_SENTINEL, index, name, s),
        SerialValue::Bytes(bytes) => {
            serializer.serialize_newtype_variant(VALUE_SENTINEL, index, name, &CompactBytes(bytes))
        }
        SerialValue::List(items) => serializer.serialize_newtype_variant(VALUE_SENTINEL, index, name, items),
        SerialValue::Map(entries) => {
            serializer.serialize_newtype_variant(VALUE_SENTINEL, index, name, &CompactMap(entries))
        }
        SerialValue::Index { table, index: position } => serializer.serialize_newtype_variant(
            VALUE_SENTINEL,
            index,
            name,
            &CompactIndex { table: *table, index: *position },
        ),
    }
}

/// The `Bytes` payload of the compact rendering: written with `serialize_bytes`.
struct CompactBytes<'a>(&'a [u8]);

impl Serialize for CompactBytes<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

/// Write a map's entries, checking every key against the reserved-key rule.
fn serialize_entries<M: SerializeMap>(map: &mut M, entries: &[(String, SerialValue)]) -> Result<(), M::Error> {
    for (key, value) in entries {
        // The bridge's own map serializer checks the key itself and reports the typed
        // error; every other format takes any key, so the rule is checked here too.
        map.serialize_key(key)?;
        check_map_key(key).map_err(ser::Error::custom)?;
        map.serialize_value(value)?;
    }
    Ok(())
}

/// The `Map` payload of the compact rendering: a serde map with the keys as they are
/// (checked against the reserved-key rule like the canonical rendering's).
struct CompactMap<'a>(&'a [(String, SerialValue)]);

impl Serialize for CompactMap<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        serialize_entries(&mut map, self.0)?;
        map.end()
    }
}

/// The `Index` payload of the compact rendering: the table-position pair.
struct CompactIndex {
    table: TableId,
    index: u32,
}

impl Serialize for CompactIndex {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_index(self.table, self.index, serializer)
    }
}

// --- Deserialize --------------------------------------------------------------------------

/// Available with the `serde` cargo feature. Reads the canonical form from a
/// human-readable format and the compact form otherwise; see the type's documentation
/// for both forms. Everything read is untrusted input: a malformed rendering — a bad
/// base64 text, a malformed `$index` pair, an object key beginning with `$` that is
/// not one of the two reserved forms, a floating-point number, an integer outside
/// `i64` — is an error, and so is a value nesting deeper than
/// [`SerialValue::MAX_NESTING_DEPTH`] (the depth is checked as the value is read, so
/// a malicious input cannot exhaust the stack whatever the format).
impl<'de> Deserialize<'de> for SerialValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ValueAt { depth: 0 }.deserialize(deserializer)
    }
}

/// A `SerialValue` being read at nesting depth `depth` — the number of lists and maps
/// enclosing it. The seed carries the depth into the nested reads, so that the bound
/// [`SerialValue::MAX_NESTING_DEPTH`] is checked as each list or map is entered, on
/// the way down, in every format alike.
#[derive(Clone, Copy)]
struct ValueAt {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for ValueAt {
    type Value = SerialValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<SerialValue, D::Error> {
        deserializer.deserialize_newtype_struct(VALUE_SENTINEL, RenderingVisitor(self))
    }
}

/// Entering a list or a map whose enclosing depth is `depth`: the depth of its items,
/// or the nesting error when the bound is exceeded.
fn enter<E: de::Error>(depth: usize) -> Result<usize, E> {
    if depth >= SerialValue::MAX_NESTING_DEPTH {
        Err(E::custom(SerialValueError::NestingTooDeep { limit: SerialValue::MAX_NESTING_DEPTH }))
    } else {
        Ok(depth + 1)
    }
}

/// Unwraps the sentinel newtype struct (JSON and the common binary formats read it as
/// its content; the bridge intercepts it) and reads the rendering the format uses.
struct RenderingVisitor(ValueAt);

impl<'de> Visitor<'de> for RenderingVisitor {
    type Value = SerialValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a serialized value")
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<SerialValue, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_any(CanonicalVisitor(self.0))
        } else {
            deserializer.deserialize_enum(VALUE_SENTINEL, COMPACT_VARIANTS, CompactVisitor(self.0))
        }
    }
}

/// Reads the canonical rendering from a self-describing format.
struct CanonicalVisitor(ValueAt);

impl<'de> Visitor<'de> for CanonicalVisitor {
    type Value = SerialValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a serialized value (null, a boolean, an integer, a string, an array, or an object)")
    }

    fn visit_unit<E: de::Error>(self) -> Result<SerialValue, E> {
        Ok(SerialValue::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<SerialValue, E> {
        Ok(SerialValue::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<SerialValue, D::Error> {
        self.0.deserialize(deserializer)
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<SerialValue, E> {
        Ok(SerialValue::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<SerialValue, E> {
        Ok(SerialValue::Int(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<SerialValue, E> {
        i64::try_from(v).map(SerialValue::Int).map_err(|_| out_of_range(v))
    }

    fn visit_i128<E: de::Error>(self, v: i128) -> Result<SerialValue, E> {
        i64::try_from(v).map(SerialValue::Int).map_err(|_| out_of_range(v))
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<SerialValue, E> {
        i64::try_from(v).map(SerialValue::Int).map_err(|_| out_of_range(v))
    }

    fn visit_f64<E: de::Error>(self, _v: f64) -> Result<SerialValue, E> {
        Err(E::custom(SerialValueError::FloatRejected))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<SerialValue, E> {
        Ok(SerialValue::Str(String::from(v)))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<SerialValue, E> {
        Ok(SerialValue::Str(v))
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<SerialValue, E> {
        Ok(SerialValue::Bytes(v.to_vec()))
    }

    fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<SerialValue, E> {
        Ok(SerialValue::Bytes(v))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<SerialValue, A::Error> {
        read_list(self.0.depth, seq)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<SerialValue, A::Error> {
        let mut entries: Vec<(String, SerialValue)> =
            Vec::with_capacity(cautious_capacity(map.size_hint()));
        // The depth of the entries' values, once the object is known to be a map (the
        // reserved objects stand for values that are not lists or maps and count no
        // level; an empty object is a map).
        let mut inner: Option<usize> = None;
        while let Some(key) = map.next_key::<String>()? {
            let inner = match inner {
                Some(inner) => inner,
                None => {
                    if key == BYTES_KEY {
                        let text: String = map.next_value()?;
                        let bytes = base64::decode(&text).map_err(de::Error::custom)?;
                        expect_end(&mut map, BYTES_KEY)?;
                        return Ok(SerialValue::Bytes(bytes));
                    }
                    if key == INDEX_KEY {
                        let (table, index): (u32, u32) = map.next_value()?;
                        expect_end(&mut map, INDEX_KEY)?;
                        return Ok(SerialValue::Index { table: TableId::new(table), index });
                    }
                    *inner.insert(enter(self.0.depth)?)
                }
            };
            check_map_key(&key).map_err(de::Error::custom)?;
            let value = map.next_value_seed(ValueAt { depth: inner })?;
            entries.push((key, value));
        }
        if inner.is_none() {
            enter::<A::Error>(self.0.depth)?;
        }
        Ok(SerialValue::Map(entries))
    }
}

/// Read a list whose enclosing depth is `depth`: the items at the next depth.
fn read_list<'de, A: SeqAccess<'de>>(depth: usize, mut seq: A) -> Result<SerialValue, A::Error> {
    let inner = enter(depth)?;
    let mut items = Vec::with_capacity(cautious_capacity(seq.size_hint()));
    while let Some(item) = seq.next_element_seed(ValueAt { depth: inner })? {
        items.push(item);
    }
    Ok(SerialValue::List(items))
}

/// An `Int` outside `i64` in the rendering: the same message as the bridge's error.
fn out_of_range<E: de::Error, N: fmt::Display>(v: N) -> E {
    use alloc::string::ToString;
    E::custom(SerialValueError::IntegerOutOfRange { value: v.to_string(), target: "i64" })
}

/// A capacity to preallocate from an untrusted size hint: bounded, so that a malicious
/// hint cannot demand a huge allocation up front.
pub(super) fn cautious_capacity(hint: Option<usize>) -> usize {
    hint.map_or(0, |n| n.min(1024))
}

/// The reserved objects have exactly one entry.
fn expect_end<'de, A: MapAccess<'de>>(map: &mut A, key: &str) -> Result<(), A::Error> {
    match map.next_key::<de::IgnoredAny>()? {
        None => Ok(()),
        Some(_) => Err(de::Error::custom(format_args!(
            "an object with the reserved key `{key}` must have that one entry only"
        ))),
    }
}

/// The value model's reserved-key rule: a map key never begins with `$` (the prefix
/// belongs to the canonical rendering's own objects, `$bytes` and `$index`). Checked
/// wherever a map is rendered or read, and by the bridge wherever it builds a map.
pub(super) fn check_map_key(key: &str) -> Result<(), SerialValueError> {
    if key.starts_with('$') {
        Err(SerialValueError::ReservedMapKey { key: String::from(key) })
    } else {
        Ok(())
    }
}

/// Reads the compact rendering: the externally tagged enum.
struct CompactVisitor(ValueAt);

impl<'de> Visitor<'de> for CompactVisitor {
    type Value = SerialValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a serialized value in its compact form (a tagged variant)")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<SerialValue, A::Error> {
        let (tag, variant) = data.variant::<CompactTag>()?;
        Ok(match tag {
            CompactTag::Null => {
                variant.unit_variant()?;
                SerialValue::Null
            }
            CompactTag::Bool => SerialValue::Bool(variant.newtype_variant()?),
            CompactTag::Int => SerialValue::Int(variant.newtype_variant()?),
            CompactTag::Str => SerialValue::Str(variant.newtype_variant()?),
            CompactTag::Bytes => SerialValue::Bytes(variant.newtype_variant::<CompactBytesOwned>()?.0),
            CompactTag::List => variant.newtype_variant_seed(CompactListAt(self.0))?,
            CompactTag::Map => variant.newtype_variant_seed(CompactMapAt(self.0))?,
            CompactTag::Index => {
                let CompactIndex { table, index } = variant.newtype_variant()?;
                SerialValue::Index { table, index }
            }
        })
    }
}

/// The variant tag of the compact rendering, read by index or by name.
enum CompactTag {
    Null,
    Bool,
    Int,
    Str,
    Bytes,
    List,
    Map,
    Index,
}

impl<'de> Deserialize<'de> for CompactTag {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TagVisitor;

        impl Visitor<'_> for TagVisitor {
            type Value = CompactTag;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a variant of the compact serialized-value form")
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<CompactTag, E> {
                Ok(match v {
                    0 => CompactTag::Null,
                    1 => CompactTag::Bool,
                    2 => CompactTag::Int,
                    3 => CompactTag::Str,
                    4 => CompactTag::Bytes,
                    5 => CompactTag::List,
                    6 => CompactTag::Map,
                    7 => CompactTag::Index,
                    _ => {
                        return Err(E::invalid_value(de::Unexpected::Unsigned(v), &"a variant index 0..=7"))
                    }
                })
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<CompactTag, E> {
                Ok(match v {
                    "Null" => CompactTag::Null,
                    "Bool" => CompactTag::Bool,
                    "Int" => CompactTag::Int,
                    "Str" => CompactTag::Str,
                    "Bytes" => CompactTag::Bytes,
                    "List" => CompactTag::List,
                    "Map" => CompactTag::Map,
                    "Index" => CompactTag::Index,
                    _ => return Err(E::unknown_variant(v, COMPACT_VARIANTS)),
                })
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<CompactTag, E> {
                match core::str::from_utf8(v) {
                    Ok(s) => self.visit_str(s),
                    Err(_) => Err(E::invalid_value(de::Unexpected::Bytes(v), &self)),
                }
            }
        }

        deserializer.deserialize_identifier(TagVisitor)
    }
}

/// The `Bytes` payload of the compact rendering, read with `deserialize_byte_buf`.
struct CompactBytesOwned(Vec<u8>);

impl<'de> Deserialize<'de> for CompactBytesOwned {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serial_bytes::deserialize(deserializer).map(CompactBytesOwned)
    }
}

/// The `List` payload of the compact rendering, read as a serde sequence at the
/// enclosing depth it carries.
struct CompactListAt(ValueAt);

impl<'de> DeserializeSeed<'de> for CompactListAt {
    type Value = SerialValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<SerialValue, D::Error> {
        struct ItemsVisitor(ValueAt);

        impl<'de> Visitor<'de> for ItemsVisitor {
            type Value = SerialValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a sequence of serialized values")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<SerialValue, A::Error> {
                read_list(self.0.depth, seq)
            }
        }

        deserializer.deserialize_seq(ItemsVisitor(self.0))
    }
}

/// The `Map` payload of the compact rendering, read as a serde map at the enclosing
/// depth it carries.
struct CompactMapAt(ValueAt);

impl<'de> DeserializeSeed<'de> for CompactMapAt {
    type Value = SerialValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<SerialValue, D::Error> {
        struct EntriesVisitor(ValueAt);

        impl<'de> Visitor<'de> for EntriesVisitor {
            type Value = SerialValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map with string keys")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<SerialValue, A::Error> {
                let inner = enter(self.0.depth)?;
                let mut entries = Vec::with_capacity(cautious_capacity(map.size_hint()));
                while let Some(key) = map.next_key::<String>()? {
                    check_map_key(&key).map_err(de::Error::custom)?;
                    let value = map.next_value_seed(ValueAt { depth: inner })?;
                    entries.push((key, value));
                }
                Ok(SerialValue::Map(entries))
            }
        }

        deserializer.deserialize_map(EntriesVisitor(self.0))
    }
}

impl<'de> Deserialize<'de> for CompactIndex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (table, index) = deserialize_index(deserializer)?;
        Ok(CompactIndex { table, index })
    }
}
