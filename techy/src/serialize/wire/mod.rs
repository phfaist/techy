//! The wire layer's conversion traits — [`ToSerialValue`] and [`FromSerialValue`] —
//! and their internal derives: how the crate's own wire structs (the serialized shape
//! of sources — [`source`], of states — [`state`]; trees and diagnostics to be added
//! here) convert to and from [`SerialValue`], unconditionally (without the `serde`
//! cargo feature, which gates only the serde bridge for implementer payloads).
//!
//! Crate-private throughout. A wire struct derives both traits and gives every field
//! and variant an explicit wire name:
//!
//! ```ignore
//! #[derive(ToSerialValue, FromSerialValue)]
//! struct WireThing {
//!     #[serial(name = "name")]
//!     name: String,
//!     #[serial(name = "count")]
//!     count: Option<u32>,
//! }
//! ```
//!
//! **Wire shape** — the same the serde bridge produces for the corresponding serde
//! shapes, so a value renders identically whichever mechanism produced it: a struct
//! is a [`SerialValue::Map`] from wire names to field values in declaration order, an
//! `Option` field that is `None` omitted (and read back as `None` when its key is
//! missing, or its value is `Null`); a unit enum variant is the [`SerialValue::Str`]
//! of its wire name; a variant with data is a one-entry map from the wire name to the
//! data (a newtype variant's payload as is, a struct variant's fields as a map).
//! Reads are strict: an unknown, repeated, or missing key, an unknown variant, or a
//! value of the wrong kind is a [`SerialValueError`].
//!
//! **Field types**: `bool`; `char` (a one-character string); the integers that fit
//! `i64` losslessly (`i8`–`i64`, `u8`–`u32`) plus `u64`, `usize`, `isize`, `i128`,
//! `u128` (written only when they fit `i64`, an error otherwise; read with range
//! checks); `String` and
//! `Cow<'static, str>`; `Option<T>`; `Vec<T>` (a `Vec<u8>` is a list of integers —
//! byte strings are the explicit [`SerialBytes`]); [`SerialValue`] itself, carried
//! verbatim (how a wire struct holds a part encoded elsewhere, e.g. by a language's
//! own codec); and any other type implementing the traits, wire structs included.

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::error::SerialValueError;
use super::value::{SerialValue, TableId};

// The derives, next to the traits they implement (their generated code refers to
// `crate::serialize::wire::…`).
pub(crate) use techy_derive::{FromSerialValue, ToSerialValue};

/// Conversion of a wire-layer value into its [`SerialValue`].
///
/// Declared `pub` so that the expansion of the `serial_index!` macro can implement it
/// for typed table positions in other crates (reached through `techy::__private`);
/// not otherwise nameable outside the crate, and not public API.
pub trait ToSerialValue {
    /// This value's serialized form.
    ///
    /// # Errors
    ///
    /// The value cannot be represented — an integer outside `i64`, or a nested
    /// conversion's error.
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError>;

    /// Whether this value, as a struct field, is left out of the field map entirely
    /// (`Option::None`). The default is `false`.
    fn is_absent_field(&self) -> bool {
        false
    }
}

/// Conversion of a [`SerialValue`] back into a wire-layer value. Strict: `value` is
/// untrusted input.
///
/// Declared `pub` for the same reason as [`ToSerialValue`]; not public API.
pub trait FromSerialValue: Sized {
    /// Read the value from its serialized form.
    ///
    /// # Errors
    ///
    /// The value is not of the expected kind or shape.
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError>;

    /// Read a struct field named `name`, whose key may be missing from the map
    /// (`value == None`). The default requires the key
    /// ([`SerialValueError::MissingField`]); `Option<T>` reads a missing key as `None`.
    ///
    /// # Errors
    ///
    /// The key is missing and required, or the value is not of the expected kind.
    fn from_serial_field(name: &'static str, value: Option<&SerialValue>) -> Result<Self, SerialValueError> {
        match value {
            Some(value) => Self::from_serial_value(value),
            None => Err(SerialValueError::MissingField { name }),
        }
    }
}

fn mismatch(expected: &'static str, found: &SerialValue) -> SerialValueError {
    SerialValueError::TypeMismatch { expected: Cow::Borrowed(expected), found: found.kind_name() }
}

// --- primitive impls --------------------------------------------------------------------

impl ToSerialValue for SerialValue {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(self.clone())
    }
}

impl FromSerialValue for SerialValue {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        Ok(value.clone())
    }
}

impl ToSerialValue for bool {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Bool(*self))
    }
}

impl FromSerialValue for bool {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        match value {
            SerialValue::Bool(b) => Ok(*b),
            other => Err(mismatch("a boolean", other)),
        }
    }
}

/// A `char` is a one-character string.
impl ToSerialValue for char {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Str(String::from(*self)))
    }
}

/// A `char` is a one-character string; any other string is an error.
impl FromSerialValue for char {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        match value {
            SerialValue::Str(s) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Ok(c),
                    _ => Err(SerialValueError::TypeMismatch {
                        expected: Cow::Borrowed("a one-character string"),
                        found: "str",
                    }),
                }
            }
            other => Err(mismatch("a one-character string", other)),
        }
    }
}

/// Integers: written when they fit `i64`, read with a range check on the target.
macro_rules! int_conversions {
    ($($t:ty),* $(,)?) => {$(
        impl ToSerialValue for $t {
            fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
                i64::try_from(*self).map(SerialValue::Int).map_err(|_| {
                    SerialValueError::IntegerOutOfRange { value: self.to_string(), target: "i64" }
                })
            }
        }

        impl FromSerialValue for $t {
            fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
                match value {
                    SerialValue::Int(i) => <$t>::try_from(*i).map_err(|_| {
                        SerialValueError::IntegerOutOfRange {
                            value: i.to_string(),
                            target: stringify!($t),
                        }
                    }),
                    other => Err(mismatch("an integer", other)),
                }
            }
        }
    )*};
}
int_conversions!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

impl ToSerialValue for String {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Str(self.clone()))
    }
}

impl FromSerialValue for String {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        match value {
            SerialValue::Str(s) => Ok(s.clone()),
            other => Err(mismatch("a string", other)),
        }
    }
}

impl ToSerialValue for str {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Str(String::from(self)))
    }
}

impl ToSerialValue for Cow<'static, str> {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Str(String::from(&**self)))
    }
}

impl FromSerialValue for Cow<'static, str> {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        String::from_serial_value(value).map(Cow::Owned)
    }
}

/// `None` is `Null` — and, as a struct field, absent from the map.
impl<T: ToSerialValue> ToSerialValue for Option<T> {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        match self {
            Some(value) => value.to_serial_value(),
            None => Ok(SerialValue::Null),
        }
    }

    fn is_absent_field(&self) -> bool {
        self.is_none()
    }
}

impl<T: FromSerialValue> FromSerialValue for Option<T> {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        match value {
            SerialValue::Null => Ok(None),
            other => T::from_serial_value(other).map(Some),
        }
    }

    fn from_serial_field(_name: &'static str, value: Option<&SerialValue>) -> Result<Self, SerialValueError> {
        match value {
            Some(value) => Self::from_serial_value(value),
            None => Ok(None),
        }
    }
}

impl<T: ToSerialValue> ToSerialValue for Vec<T> {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        self.iter().map(ToSerialValue::to_serial_value).collect::<Result<Vec<_>, _>>().map(SerialValue::List)
    }
}

impl<T: FromSerialValue> FromSerialValue for Vec<T> {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        match value {
            SerialValue::List(items) => items.iter().map(T::from_serial_value).collect(),
            other => Err(mismatch("a list", other)),
        }
    }
}

/// A byte string field of a wire struct: converts to and from [`SerialValue::Bytes`]
/// (a plain `Vec<u8>` field is a list of integers).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct SerialBytes(pub(crate) Vec<u8>);

impl ToSerialValue for SerialBytes {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Bytes(self.0.clone()))
    }
}

impl FromSerialValue for SerialBytes {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        match value {
            SerialValue::Bytes(bytes) => Ok(SerialBytes(bytes.clone())),
            other => Err(mismatch("a byte string", other)),
        }
    }
}

/// A table id is its ordinal (a segment's table directory carries them).
impl ToSerialValue for TableId {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Int(i64::from(self.ordinal())))
    }
}

impl FromSerialValue for TableId {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        u32::from_serial_value(value).map(TableId::new)
    }
}

/// The two parts of a [`SerialValue::Index`], or the kind mismatch: what a typed
/// table position's `FromSerialValue` impl (the `serial_index!` macro's expansion)
/// reads. Declared `pub` for that expansion (reached through `techy::__private`); not
/// public API.
pub fn index_from_serial_value(value: &SerialValue) -> Result<(TableId, u32), SerialValueError> {
    match value {
        SerialValue::Index { table, index } => Ok((*table, *index)),
        other => Err(mismatch("a table position", other)),
    }
}

// --- support for the derived code -----------------------------------------------------

/// Collects a struct's fields into its map, in call order, leaving absent fields out.
/// Used by the derived `to_serial_value`.
pub(crate) struct FieldWriter {
    entries: Vec<(String, SerialValue)>,
}

impl FieldWriter {
    pub(crate) fn with_capacity(fields: usize) -> FieldWriter {
        FieldWriter { entries: Vec::with_capacity(fields) }
    }

    /// Append the field `name` unless the value is absent.
    pub(crate) fn field<T: ToSerialValue + ?Sized>(
        &mut self,
        name: &'static str,
        value: &T,
    ) -> Result<(), SerialValueError> {
        if value.is_absent_field() {
            return Ok(());
        }
        self.entries.push((String::from(name), value.to_serial_value()?));
        Ok(())
    }

    pub(crate) fn finish(self) -> SerialValue {
        SerialValue::Map(self.entries)
    }
}

/// A struct's field map, validated up front: the value is a map, every key is one of
/// the declared `fields`, and no key repeats. Used by the derived `from_serial_value`.
pub(crate) struct FieldReader<'a> {
    entries: &'a [(String, SerialValue)],
}

impl<'a> FieldReader<'a> {
    /// `what` names the type (or variant) for the error message.
    pub(crate) fn new(
        value: &'a SerialValue,
        what: &'static str,
        fields: &'static [&'static str],
    ) -> Result<FieldReader<'a>, SerialValueError> {
        let entries = match value {
            SerialValue::Map(entries) => entries.as_slice(),
            other => {
                return Err(SerialValueError::TypeMismatch {
                    expected: Cow::Owned(alloc::format!("a map ({what})")),
                    found: other.kind_name(),
                })
            }
        };
        // Bounded even for hostile input: the first unknown key ends the scan, and with
        // `fields.len()` distinct known keys, a repeat is found within `fields.len() + 1`
        // entries.
        for (position, (key, _)) in entries.iter().enumerate() {
            if !fields.contains(&key.as_str()) {
                return Err(SerialValueError::UnknownField { name: key.clone(), expected: fields });
            }
            if entries[..position].iter().any(|(earlier, _)| earlier == key) {
                return Err(SerialValueError::DuplicateField { name: key.clone() });
            }
        }
        Ok(FieldReader { entries })
    }

    /// Read the field `name` (a declared one), missing keys handled by the type.
    pub(crate) fn field<T: FromSerialValue>(&self, name: &'static str) -> Result<T, SerialValueError> {
        let found = self.entries.iter().find(|(key, _)| key == name).map(|(_, value)| value);
        T::from_serial_field(name, found)
    }
}

/// The wire form of a unit variant.
pub(crate) fn unit_variant(name: &'static str) -> SerialValue {
    SerialValue::Str(String::from(name))
}

/// The wire form of a variant with data.
pub(crate) fn data_variant(name: &'static str, data: SerialValue) -> SerialValue {
    SerialValue::Map(Vec::from([(String::from(name), data)]))
}

/// Split an enum value into its variant name and payload, checking the name is one of
/// `variants`; `what` names the enum for the error message.
pub(crate) fn read_variant<'a>(
    value: &'a SerialValue,
    what: &'static str,
    variants: &'static [&'static str],
) -> Result<(&'a str, Option<&'a SerialValue>), SerialValueError> {
    let (name, payload) = match value {
        SerialValue::Str(name) => (name.as_str(), None),
        SerialValue::Map(entries) => match entries.as_slice() {
            [(name, payload)] => (name.as_str(), Some(payload)),
            _ => return Err(enum_mismatch(what, value)),
        },
        _ => return Err(enum_mismatch(what, value)),
    };
    if !variants.contains(&name) {
        return Err(unknown_variant(name, variants));
    }
    Ok((name, payload))
}

fn enum_mismatch(what: &'static str, found: &SerialValue) -> SerialValueError {
    SerialValueError::TypeMismatch {
        expected: Cow::Owned(alloc::format!(
            "an enum value ({what}: a string naming a unit variant, or a one-entry map from \
             the variant name to its data)"
        )),
        found: found.kind_name(),
    }
}

/// The error for a variant name not among `variants`.
pub(crate) fn unknown_variant(name: &str, variants: &'static [&'static str]) -> SerialValueError {
    SerialValueError::UnknownVariant { name: String::from(name), expected: variants }
}

/// A unit variant carries no data.
pub(crate) fn expect_unit_variant(name: &'static str, payload: Option<&SerialValue>) -> Result<(), SerialValueError> {
    match payload {
        None => Ok(()),
        Some(_) => Err(SerialValueError::TypeMismatch {
            expected: Cow::Owned(alloc::format!("the string `{name}` (a unit variant carries no data)")),
            found: "map",
        }),
    }
}

/// A variant with data comes with its payload.
pub(crate) fn expect_data_variant<'a>(
    name: &'static str,
    payload: Option<&'a SerialValue>,
) -> Result<&'a SerialValue, SerialValueError> {
    match payload {
        Some(payload) => Ok(payload),
        None => Err(SerialValueError::TypeMismatch {
            expected: Cow::Owned(alloc::format!("a one-entry map from `{name}` to the variant's data")),
            found: "str",
        }),
    }
}

pub(crate) mod source;
pub(crate) mod state;

#[cfg(test)]
mod tests;
