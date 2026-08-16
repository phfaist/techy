//! The serde bridge (cargo feature `serde`): [`SerialValue`] as a serde data format of
//! its own, so that any type implementing serde's `Serialize` converts to a
//! `SerialValue` ([`to_value`]) and any type implementing `Deserialize` is read from
//! one ([`from_value`]) — the `serde_json::Value` pattern. The bridge is where the
//! value model's policy is enforced mechanically: floating-point numbers, integers
//! outside `i64`, and maps with non-string keys are errors.
//!
//! Two kinds of data have dedicated variants that serde has no native notion of, and
//! the bridge intercepts them: byte strings travel through serde's own bytes channel
//! (`serialize_bytes` / `deserialize_bytes`, which derived types reach with the
//! [`serial_bytes`] helper) and become [`SerialValue::Bytes`]; table positions travel
//! as one fixed sentinel newtype struct ([`INDEX_SENTINEL`], written and read by
//! [`serialize_index`] / [`deserialize_index`]) and become [`SerialValue::Index`].
//! `SerialValue`'s own `Serialize`/`Deserialize` impls (see `render.rs`) are
//! intercepted the same way, so a `SerialValue` converts to itself unchanged.

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use serde::de::{
    self, Deserialize, DeserializeSeed, Deserializer, EnumAccess, IntoDeserializer, MapAccess,
    SeqAccess, VariantAccess, Visitor,
};
use serde::ser::{self, Impossible, Serialize, SerializeMap, SerializeSeq, Serializer};

use super::error::SerialValueError;
use super::render::{compact_variant_name, VALUE_SENTINEL};
use super::value::{SerialValue, TableId};

// --- serde's error traits ---------------------------------------------------------------

impl ser::Error for SerialValueError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        SerialValueError::Custom(message.to_string())
    }
}

impl de::Error for SerialValueError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        SerialValueError::Custom(message.to_string())
    }

    fn missing_field(field: &'static str) -> Self {
        SerialValueError::MissingField { name: field }
    }

    fn unknown_field(field: &str, expected: &'static [&'static str]) -> Self {
        SerialValueError::UnknownField { name: String::from(field), expected }
    }

    fn unknown_variant(variant: &str, expected: &'static [&'static str]) -> Self {
        SerialValueError::UnknownVariant { name: String::from(variant), expected }
    }

    fn duplicate_field(field: &'static str) -> Self {
        SerialValueError::DuplicateField { name: String::from(field) }
    }
}

fn mismatch(expected: &'static str, found: &SerialValue) -> SerialValueError {
    SerialValueError::TypeMismatch { expected: Cow::Borrowed(expected), found: found.kind_name() }
}

// --- the index sentinel ---------------------------------------------------------------------

/// The name of the newtype struct through which a table position — a table's ordinal
/// and the position within it — travels through serde. The bridge intercepts a newtype
/// struct of exactly this name and maps its `(u32, u32)` payload to
/// [`SerialValue::Index`]; every other newtype struct is transparent. Typed table
/// positions use it through [`serialize_index`] and [`deserialize_index`].
pub(crate) const INDEX_SENTINEL: &str = "techy::serialize::Index";

/// Serialize the table position `(table, index)` so that the bridge produces
/// [`SerialValue::Index`] and any other serde format writes the pair
/// `(table ordinal, index)` as a newtype struct named [`INDEX_SENTINEL`] (transparent
/// in every common format: a two-element sequence).
pub(crate) fn serialize_index<S: Serializer>(
    table: TableId,
    index: u32,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_newtype_struct(INDEX_SENTINEL, &(table.ordinal(), index))
}

/// Deserialize a table position written by [`serialize_index`]: from the bridge, a
/// [`SerialValue::Index`]; from any other format, the two-element pair.
pub(crate) fn deserialize_index<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<(TableId, u32), D::Error> {
    struct IndexPairVisitor;

    impl<'de> Visitor<'de> for IndexPairVisitor {
        type Value = (TableId, u32);

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a table position (table ordinal and index)")
        }

        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            let (table, index) = <(u32, u32)>::deserialize(deserializer)?;
            Ok((TableId::new(table), index))
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let table: u32 = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(0, &self))?;
            let index: u32 = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(1, &self))?;
            Ok((TableId::new(table), index))
        }
    }

    deserializer.deserialize_newtype_struct(INDEX_SENTINEL, IndexPairVisitor)
}

// --- the bytes helper -------------------------------------------------------------------------

/// Serialize a byte string field as [`SerialValue::Bytes`] rather than as a list of
/// integers: the `#[serde(with = "…")]` helper for byte-string fields of payload
/// types.
///
/// A plain `Vec<u8>` field is, to serde, a sequence of integers, and the bridge
/// converts it to a [`SerialValue::List`] of [`SerialValue::Int`] accordingly. To make
/// it a byte string — rendered compactly (as base64 text in JSON) — mark the field:
///
/// ```
/// use serde::{Deserialize, Serialize};
/// use techy::serialize::{from_value, to_value, SerialValue};
///
/// #[derive(Serialize, Deserialize, PartialEq, Debug)]
/// struct Digest {
///     algorithm: String,
///     #[serde(with = "techy::serialize::serial_bytes")]
///     bytes: Vec<u8>,
/// }
///
/// let digest = Digest { algorithm: "sha256".into(), bytes: vec![0xde, 0xad] };
/// let value = to_value(&digest)?;
/// assert_eq!(
///     value,
///     SerialValue::Map(vec![
///         ("algorithm".into(), SerialValue::Str("sha256".into())),
///         ("bytes".into(), SerialValue::Bytes(vec![0xde, 0xad])),
///     ])
/// );
/// assert_eq!(from_value::<Digest>(&value)?, digest);
/// # Ok::<(), techy::serialize::SerialValueError>(())
/// ```
///
/// The functions use serde's byte-string channel (`serialize_bytes` /
/// `deserialize_byte_buf`); through any serde format other than the bridge, the field
/// takes that format's byte-string form. Reading accepts a byte string only, never a
/// list of integers.
pub mod serial_bytes {
    use alloc::vec::Vec;
    use core::fmt;

    use serde::de::{Deserializer, Visitor};
    use serde::ser::Serializer;

    /// Serialize `bytes` as a byte string.
    ///
    /// # Errors
    ///
    /// Whatever `serializer` reports for a byte string.
    pub fn serialize<T: AsRef<[u8]> + ?Sized, S: Serializer>(
        bytes: &T,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes.as_ref())
    }

    /// Deserialize a byte string into a `Vec<u8>`.
    ///
    /// # Errors
    ///
    /// The value is not a byte string, or whatever `deserializer` reports.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        struct BytesVisitor;

        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a byte string")
            }

            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                Ok(bytes.to_vec())
            }

            fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Self::Value, E> {
                Ok(bytes)
            }
        }

        deserializer.deserialize_byte_buf(BytesVisitor)
    }
}

// --- to_value: the Serializer -------------------------------------------------------------

/// Convert any serializable value to a [`SerialValue`] (available with the `serde`
/// cargo feature).
///
/// The mapping: booleans → [`Bool`](SerialValue::Bool); integers of every width →
/// [`Int`](SerialValue::Int) (a value outside `i64` is an error); `char`, strings →
/// [`Str`](SerialValue::Str); byte strings (serde's `serialize_bytes`, reached with
/// [`serial_bytes`]) → [`Bytes`](SerialValue::Bytes); `()`, unit structs, and `None` →
/// [`Null`](SerialValue::Null); `Some(x)` → `x`; sequences and tuples →
/// [`List`](SerialValue::List); structs → [`Map`](SerialValue::Map) in field order;
/// maps → `Map` (keys must be strings — `char` keys and unit enum variants count as
/// their string form; anything else is an error); newtype structs → their content
/// (except the table-position sentinel, which becomes [`Index`](SerialValue::Index));
/// enums in serde's externally tagged form — a unit variant → `Str` of the variant
/// name, a variant with data → a one-entry `Map` from the variant name to the data. A
/// `SerialValue` converts to itself unchanged.
///
/// # Errors
///
/// [`SerialValueError::FloatRejected`] for `f32`/`f64`;
/// [`SerialValueError::IntegerOutOfRange`] for an integer outside `i64`;
/// [`SerialValueError::NonStringMapKey`] for a map key that is not a string; a
/// [`SerialValueError::Custom`] for whatever the type's own `Serialize` impl reports.
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<SerialValue, SerialValueError> {
    value.serialize(ValueSerializer { human_readable: true })
}

/// The `Serializer` producing a `SerialValue`. `human_readable` is what it answers to
/// serde's `is_human_readable()`: `true` at the top level (the value model is the
/// in-memory form of the canonical JSON rendering, so types with a text form and a
/// compact form use their text form), `false` while serializing the payload of a
/// `SerialValue` — whose own `Serialize` impl then takes its compact form, which the
/// bridge maps back to the identical value.
#[derive(Clone, Copy)]
struct ValueSerializer {
    human_readable: bool,
}

/// Wrap `payload` in `Map([(variant, payload)])`: serde's externally tagged form.
fn tagged(variant: &str, payload: SerialValue) -> SerialValue {
    SerialValue::Map(Vec::from([(String::from(variant), payload)]))
}

macro_rules! serialize_fitting_int {
    ($($method:ident: $t:ty),* $(,)?) => {$(
        fn $method(self, v: $t) -> Result<SerialValue, SerialValueError> {
            Ok(SerialValue::Int(i64::from(v)))
        }
    )*};
}

macro_rules! serialize_checked_int {
    ($($method:ident: $t:ty),* $(,)?) => {$(
        fn $method(self, v: $t) -> Result<SerialValue, SerialValueError> {
            i64::try_from(v).map(SerialValue::Int).map_err(|_| {
                SerialValueError::IntegerOutOfRange { value: v.to_string(), target: "i64" }
            })
        }
    )*};
}

impl Serializer for ValueSerializer {
    type Ok = SerialValue;
    type Error = SerialValueError;
    type SerializeSeq = ListSerializer;
    type SerializeTuple = ListSerializer;
    type SerializeTupleStruct = ListSerializer;
    type SerializeTupleVariant = ListVariantSerializer;
    type SerializeMap = MapSerializer;
    type SerializeStruct = MapSerializer;
    type SerializeStructVariant = MapVariantSerializer;

    fn is_human_readable(&self) -> bool {
        self.human_readable
    }

    fn serialize_bool(self, v: bool) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Bool(v))
    }

    serialize_fitting_int! {
        serialize_i8: i8, serialize_i16: i16, serialize_i32: i32, serialize_i64: i64,
        serialize_u8: u8, serialize_u16: u16, serialize_u32: u32,
    }

    serialize_checked_int! {
        serialize_u64: u64, serialize_i128: i128, serialize_u128: u128,
    }

    fn serialize_f32(self, _v: f32) -> Result<SerialValue, SerialValueError> {
        Err(SerialValueError::FloatRejected)
    }

    fn serialize_f64(self, _v: f64) -> Result<SerialValue, SerialValueError> {
        Err(SerialValueError::FloatRejected)
    }

    fn serialize_char(self, v: char) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Str(String::from(v)))
    }

    fn serialize_str(self, v: &str) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Str(String::from(v)))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Bytes(v.to_vec()))
    }

    fn serialize_none(self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Null)
    }

    fn serialize_some<T: Serialize + ?Sized>(
        self,
        value: &T,
    ) -> Result<SerialValue, SerialValueError> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Null)
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<SerialValue, SerialValueError> {
        if name == VALUE_SENTINEL {
            // `SerialValue`'s compact form: the only unit variant is `Null`.
            return Ok(SerialValue::Null);
        }
        Ok(SerialValue::Str(String::from(variant)))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<SerialValue, SerialValueError> {
        if name == INDEX_SENTINEL {
            return index_from_pair(value.serialize(self)?);
        }
        if name == VALUE_SENTINEL {
            // A `SerialValue` serializes itself: its compact form, read back below into
            // the identical value.
            return value.serialize(ValueSerializer { human_readable: false });
        }
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<SerialValue, SerialValueError> {
        if name == VALUE_SENTINEL {
            // The compact form's payload is the value itself.
            return value.serialize(self);
        }
        Ok(tagged(variant, value.serialize(self)?))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<ListSerializer, SerialValueError> {
        Ok(ListSerializer {
            items: Vec::with_capacity(len.unwrap_or(0)),
            human_readable: self.human_readable,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<ListSerializer, SerialValueError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<ListSerializer, SerialValueError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<ListVariantSerializer, SerialValueError> {
        Ok(ListVariantSerializer { variant, list: self.serialize_seq(Some(len))? })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<MapSerializer, SerialValueError> {
        Ok(MapSerializer {
            entries: Vec::with_capacity(len.unwrap_or(0)),
            pending_key: None,
            human_readable: self.human_readable,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<MapSerializer, SerialValueError> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<MapVariantSerializer, SerialValueError> {
        Ok(MapVariantSerializer { variant, map: self.serialize_map(Some(len))? })
    }
}

/// The `(u32, u32)` payload of the index sentinel, serialized, becomes an `Index`.
fn index_from_pair(pair: SerialValue) -> Result<SerialValue, SerialValueError> {
    let to_u32 = |v: i64| {
        u32::try_from(v).map_err(|_| SerialValueError::IntegerOutOfRange {
            value: v.to_string(),
            target: "u32",
        })
    };
    match &pair {
        SerialValue::List(items) => match items.as_slice() {
            [SerialValue::Int(table), SerialValue::Int(index)] => Ok(SerialValue::Index {
                table: TableId::new(to_u32(*table)?),
                index: to_u32(*index)?,
            }),
            _ => Err(mismatch("a table position (a pair of two integers)", &pair)),
        },
        _ => Err(mismatch("a table position (a pair of two integers)", &pair)),
    }
}

/// Collects a sequence, tuple, or tuple struct into a `List`.
struct ListSerializer {
    items: Vec<SerialValue>,
    human_readable: bool,
}

impl SerializeSeq for ListSerializer {
    type Ok = SerialValue;
    type Error = SerialValueError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), SerialValueError> {
        let item = value.serialize(ValueSerializer { human_readable: self.human_readable })?;
        self.items.push(item);
        Ok(())
    }

    fn end(self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::List(self.items))
    }
}

impl ser::SerializeTuple for ListSerializer {
    type Ok = SerialValue;
    type Error = SerialValueError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), SerialValueError> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<SerialValue, SerialValueError> {
        SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for ListSerializer {
    type Ok = SerialValue;
    type Error = SerialValueError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), SerialValueError> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<SerialValue, SerialValueError> {
        SerializeSeq::end(self)
    }
}

/// Collects a tuple variant: `Map([(variant, List)])`.
struct ListVariantSerializer {
    variant: &'static str,
    list: ListSerializer,
}

impl ser::SerializeTupleVariant for ListVariantSerializer {
    type Ok = SerialValue;
    type Error = SerialValueError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), SerialValueError> {
        SerializeSeq::serialize_element(&mut self.list, value)
    }

    fn end(self) -> Result<SerialValue, SerialValueError> {
        Ok(tagged(self.variant, SerializeSeq::end(self.list)?))
    }
}

/// Collects a map or a struct into a `Map`.
struct MapSerializer {
    entries: Vec<(String, SerialValue)>,
    pending_key: Option<String>,
    human_readable: bool,
}

impl SerializeMap for MapSerializer {
    type Ok = SerialValue;
    type Error = SerialValueError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), SerialValueError> {
        self.pending_key = Some(key.serialize(MapKeySerializer)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), SerialValueError> {
        // serde's contract: `serialize_value` follows a `serialize_key`. Report a
        // violation as an error, never a panic.
        let key = self
            .pending_key
            .take()
            .ok_or_else(|| ser::Error::custom("serialize_value called before serialize_key"))?;
        let value = value.serialize(ValueSerializer { human_readable: self.human_readable })?;
        self.entries.push((key, value));
        Ok(())
    }

    fn end(self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Map(self.entries))
    }
}

impl ser::SerializeStruct for MapSerializer {
    type Ok = SerialValue;
    type Error = SerialValueError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), SerialValueError> {
        let value = value.serialize(ValueSerializer { human_readable: self.human_readable })?;
        self.entries.push((String::from(key), value));
        Ok(())
    }

    fn end(self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Map(self.entries))
    }
}

/// Collects a struct variant: `Map([(variant, Map)])`.
struct MapVariantSerializer {
    variant: &'static str,
    map: MapSerializer,
}

impl ser::SerializeStructVariant for MapVariantSerializer {
    type Ok = SerialValue;
    type Error = SerialValueError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), SerialValueError> {
        ser::SerializeStruct::serialize_field(&mut self.map, key, value)
    }

    fn end(self) -> Result<SerialValue, SerialValueError> {
        Ok(tagged(self.variant, ser::SerializeStruct::end(self.map)?))
    }
}

/// Serializes a map key: strings only (`char` and unit variants as their string form;
/// a newtype struct as its content); anything else is [`SerialValueError::NonStringMapKey`].
struct MapKeySerializer;

macro_rules! non_string_key {
    ($($method:ident: $t:ty),* $(,)?) => {$(
        fn $method(self, _v: $t) -> Result<String, SerialValueError> {
            Err(SerialValueError::NonStringMapKey)
        }
    )*};
}

impl Serializer for MapKeySerializer {
    type Ok = String;
    type Error = SerialValueError;
    type SerializeSeq = Impossible<String, SerialValueError>;
    type SerializeTuple = Impossible<String, SerialValueError>;
    type SerializeTupleStruct = Impossible<String, SerialValueError>;
    type SerializeTupleVariant = Impossible<String, SerialValueError>;
    type SerializeMap = Impossible<String, SerialValueError>;
    type SerializeStruct = Impossible<String, SerialValueError>;
    type SerializeStructVariant = Impossible<String, SerialValueError>;

    fn serialize_str(self, v: &str) -> Result<String, SerialValueError> {
        Ok(String::from(v))
    }

    fn serialize_char(self, v: char) -> Result<String, SerialValueError> {
        Ok(String::from(v))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<String, SerialValueError> {
        Ok(String::from(variant))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<String, SerialValueError> {
        value.serialize(self)
    }

    non_string_key! {
        serialize_bool: bool,
        serialize_i8: i8, serialize_i16: i16, serialize_i32: i32, serialize_i64: i64,
        serialize_i128: i128,
        serialize_u8: u8, serialize_u16: u16, serialize_u32: u32, serialize_u64: u64,
        serialize_u128: u128,
        serialize_f32: f32, serialize_f64: f64,
        serialize_bytes: &[u8],
    }

    fn serialize_none(self) -> Result<String, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_some<T: Serialize + ?Sized>(self, _value: &T) -> Result<String, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_unit(self) -> Result<String, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<String, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<String, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, SerialValueError> {
        Err(SerialValueError::NonStringMapKey)
    }
}

// --- from_value: the Deserializer ----------------------------------------------------------

/// Read any deserializable value from a [`SerialValue`] (available with the `serde`
/// cargo feature): the reverse of [`to_value`], with the same mapping. The value is
/// borrowed, so strings and byte strings can be borrowed from it (`&'de str`,
/// `&'de [u8]`).
///
/// # Errors
///
/// [`SerialValueError::TypeMismatch`] when the value's kind is not what the type
/// reads (a `Str` where a struct's `Map` is needed, a `List` where a byte string is
/// needed, an `Index` read as anything but a typed table position);
/// [`SerialValueError::FloatRejected`] for an `f32`/`f64` target;
/// [`SerialValueError::IntegerOutOfRange`] when an `Int` does not fit the target
/// integer type; the field and variant errors ([`MissingField`], [`UnknownField`],
/// [`DuplicateField`], [`UnknownVariant`]) as the type's `Deserialize` impl reports
/// them; a [`Custom`] for anything else it reports.
///
/// [`MissingField`]: SerialValueError::MissingField
/// [`UnknownField`]: SerialValueError::UnknownField
/// [`DuplicateField`]: SerialValueError::DuplicateField
/// [`UnknownVariant`]: SerialValueError::UnknownVariant
/// [`Custom`]: SerialValueError::Custom
pub fn from_value<'de, T: de::Deserialize<'de>>(value: &'de SerialValue) -> Result<T, SerialValueError> {
    T::deserialize(value)
}

/// `&SerialValue` is a serde `Deserializer` (available with the `serde` cargo feature):
/// [`from_value`] is `T::deserialize(&value)`. Its `is_human_readable()` is `true`;
/// see [`to_value`] for the mapping.
impl<'de> Deserializer<'de> for &'de SerialValue {
    type Error = SerialValueError;

    fn is_human_readable(&self) -> bool {
        true
    }

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, SerialValueError> {
        match self {
            SerialValue::Null => visitor.visit_unit(),
            SerialValue::Bool(b) => visitor.visit_bool(*b),
            SerialValue::Int(i) => visitor.visit_i64(*i),
            SerialValue::Str(s) => visitor.visit_borrowed_str(s),
            SerialValue::Bytes(b) => visitor.visit_borrowed_bytes(b),
            SerialValue::List(items) => visit_list(items, visitor),
            SerialValue::Map(entries) => visit_entries(entries, visitor),
            SerialValue::Index { .. } => Err(mismatch(
                "a value that is not a table position (a table position is read only \
                 into a type made for one)",
                self,
            )),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, SerialValueError> {
        match self {
            SerialValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        if name == INDEX_SENTINEL {
            return match self {
                SerialValue::Index { table, index } => visitor.visit_seq(IndexPairReader {
                    remaining: [table.ordinal(), *index].into_iter(),
                }),
                _ => Err(mismatch("a table position", self)),
            };
        }
        if name == VALUE_SENTINEL {
            return visitor.visit_newtype_struct(CompactReader(self));
        }
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        if name == VALUE_SENTINEL {
            return visitor.visit_enum(ValueAsVariant(self));
        }
        match self {
            SerialValue::Str(variant) => {
                visitor.visit_enum(EnumReader { variant, payload: None })
            }
            SerialValue::Map(entries) => match entries.as_slice() {
                [(variant, payload)] => {
                    visitor.visit_enum(EnumReader { variant, payload: Some(payload) })
                }
                _ => Err(mismatch(
                    "an enum value (a string naming a unit variant, or a one-entry map \
                     from the variant name to its data)",
                    self,
                )),
            },
            _ => Err(mismatch(
                "an enum value (a string naming a unit variant, or a one-entry map from \
                 the variant name to its data)",
                self,
            )),
        }
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, SerialValueError> {
        match self {
            SerialValue::Bytes(b) => visitor.visit_borrowed_bytes(b),
            _ => Err(mismatch("a byte string", self)),
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, SerialValueError> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, SerialValueError> {
        Err(SerialValueError::FloatRejected)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, SerialValueError> {
        Err(SerialValueError::FloatRejected)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, SerialValueError> {
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 char str string
        unit unit_struct seq tuple tuple_struct map struct identifier
    }
}

/// `visit_seq` over a list, requiring the visitor to consume every element.
fn visit_list<'de, V: Visitor<'de>>(
    items: &'de [SerialValue],
    visitor: V,
) -> Result<V::Value, SerialValueError> {
    let mut reader = ListReader { items: items.iter() };
    let value = visitor.visit_seq(&mut reader)?;
    let remaining = reader.items.len();
    if remaining == 0 {
        Ok(value)
    } else {
        Err(de::Error::invalid_length(items.len(), &"a list with fewer elements"))
    }
}

/// `visit_map` over map entries, requiring the visitor to consume every entry.
fn visit_entries<'de, V: Visitor<'de>>(
    entries: &'de [(String, SerialValue)],
    visitor: V,
) -> Result<V::Value, SerialValueError> {
    let mut reader = MapReader { entries: entries.iter(), pending_value: None };
    let value = visitor.visit_map(&mut reader)?;
    let remaining = reader.entries.len();
    if remaining == 0 {
        Ok(value)
    } else {
        Err(de::Error::invalid_length(entries.len(), &"a map with fewer entries"))
    }
}

struct ListReader<'de> {
    items: core::slice::Iter<'de, SerialValue>,
}

impl<'de> SeqAccess<'de> for ListReader<'de> {
    type Error = SerialValueError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, SerialValueError> {
        match self.items.next() {
            Some(item) => seed.deserialize(item).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }
}

struct MapReader<'de> {
    entries: core::slice::Iter<'de, (String, SerialValue)>,
    pending_value: Option<&'de SerialValue>,
}

impl<'de> MapAccess<'de> for MapReader<'de> {
    type Error = SerialValueError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, SerialValueError> {
        match self.entries.next() {
            Some((key, value)) => {
                self.pending_value = Some(value);
                seed.deserialize(MapKeyReader(key)).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, SerialValueError> {
        // serde's contract: `next_value_seed` follows a `next_key_seed`. Report a
        // violation as an error, never a panic.
        let value = self
            .pending_value
            .take()
            .ok_or_else(|| de::Error::custom("next_value called before next_key"))?;
        seed.deserialize(value)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

/// Deserializes a map key: a borrowed string, also readable as a newtype struct
/// (transparent) or a unit enum variant — the forms [`MapKeySerializer`] writes.
struct MapKeyReader<'de>(&'de str);

impl<'de> Deserializer<'de> for MapKeyReader<'de> {
    type Error = SerialValueError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, SerialValueError> {
        visitor.visit_borrowed_str(self.0)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        visitor.visit_enum(EnumReader { variant: self.0, payload: None })
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf option unit unit_struct seq tuple tuple_struct map struct identifier
        ignored_any
    }
}

/// The two elements of a table position, for the index sentinel's visitor.
struct IndexPairReader {
    remaining: core::array::IntoIter<u32, 2>,
}

impl<'de> SeqAccess<'de> for IndexPairReader {
    type Error = SerialValueError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, SerialValueError> {
        match self.remaining.next() {
            Some(part) => {
                let part: de::value::U32Deserializer<SerialValueError> = part.into_deserializer();
                seed.deserialize(part).map(Some)
            }
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining.len())
    }
}

/// An enum in the externally tagged form: the variant name and its payload, if any.
struct EnumReader<'de> {
    variant: &'de str,
    payload: Option<&'de SerialValue>,
}

impl<'de> EnumAccess<'de> for EnumReader<'de> {
    type Error = SerialValueError;
    type Variant = VariantReader<'de>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, VariantReader<'de>), SerialValueError> {
        let variant = seed.deserialize(MapKeyReader(self.variant))?;
        Ok((variant, VariantReader { payload: self.payload }))
    }
}

struct VariantReader<'de> {
    payload: Option<&'de SerialValue>,
}

impl<'de> VariantAccess<'de> for VariantReader<'de> {
    type Error = SerialValueError;

    fn unit_variant(self) -> Result<(), SerialValueError> {
        match self.payload {
            None => Ok(()),
            Some(payload) => Err(SerialValueError::TypeMismatch {
                expected: Cow::Borrowed("a string naming a unit variant"),
                found: payload.kind_name(),
            }),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, SerialValueError> {
        match self.payload {
            Some(payload) => seed.deserialize(payload),
            None => Err(SerialValueError::TypeMismatch {
                expected: Cow::Borrowed("a one-entry map from the variant name to its data"),
                found: "str",
            }),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, SerialValueError> {
        match self.payload {
            Some(payload) => payload.deserialize_seq(visitor),
            None => Err(SerialValueError::TypeMismatch {
                expected: Cow::Borrowed("a one-entry map from the variant name to its data"),
                found: "str",
            }),
        }
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        match self.payload {
            Some(payload) => payload.deserialize_map(visitor),
            None => Err(SerialValueError::TypeMismatch {
                expected: Cow::Borrowed("a one-entry map from the variant name to its data"),
                found: "str",
            }),
        }
    }
}

/// A `SerialValue` presented as its own compact-form variant: the variant name is the
/// value's kind, the payload is the value itself. This is what makes a `SerialValue`
/// read from a `SerialValue` come out identical.
struct ValueAsVariant<'de>(&'de SerialValue);

impl<'de> EnumAccess<'de> for ValueAsVariant<'de> {
    type Error = SerialValueError;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self), SerialValueError> {
        let name: &'static str = compact_variant_name(self.0);
        let variant = seed.deserialize(MapKeyReader(name))?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueAsVariant<'de> {
    type Error = SerialValueError;

    fn unit_variant(self) -> Result<(), SerialValueError> {
        // Only `Null` is a unit variant, and its name was just handed out.
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, SerialValueError> {
        seed.deserialize(self.0)
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, _visitor: V) -> Result<V::Value, SerialValueError> {
        Err(de::Error::custom("the compact form of a serialized value has no tuple variant"))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        Err(de::Error::custom("the compact form of a serialized value has no struct variant"))
    }
}

/// `&SerialValue` as a deserializer that answers `is_human_readable() == false`: what
/// a `SerialValue`'s own `Deserialize` impl sees when it reads from a `SerialValue`,
/// so that it takes its compact form and comes out identical. Every other method
/// behaves as on `&SerialValue`.
struct CompactReader<'de>(&'de SerialValue);

macro_rules! delegate_to_value {
    ($($method:ident),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, SerialValueError> {
            self.0.$method(visitor)
        }
    )*};
}

impl<'de> Deserializer<'de> for CompactReader<'de> {
    type Error = SerialValueError;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        self.0.deserialize_enum(name, variants, visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        self.0.deserialize_newtype_struct(name, visitor)
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        self.0.deserialize_unit_struct(name, visitor)
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, SerialValueError> {
        self.0.deserialize_tuple(len, visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        self.0.deserialize_tuple_struct(name, len, visitor)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, SerialValueError> {
        self.0.deserialize_struct(name, fields, visitor)
    }

    delegate_to_value! {
        deserialize_any, deserialize_bool,
        deserialize_i8, deserialize_i16, deserialize_i32, deserialize_i64, deserialize_i128,
        deserialize_u8, deserialize_u16, deserialize_u32, deserialize_u64, deserialize_u128,
        deserialize_f32, deserialize_f64, deserialize_char, deserialize_str,
        deserialize_string, deserialize_bytes, deserialize_byte_buf, deserialize_option,
        deserialize_unit, deserialize_seq, deserialize_map, deserialize_identifier,
        deserialize_ignored_any,
    }
}
