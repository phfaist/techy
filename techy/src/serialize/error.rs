//! The serialization error types: [`SerializeError`] (write side),
//! [`DeserializeError`] (read side), and [`SerialValueError`] (conversion of plain
//! data to and from a [`SerialValue`](crate::serialize::SerialValue)).

use alloc::borrow::Cow;
use alloc::string::String;
use core::fmt;

/// Error of the write side: what a
/// [`serialize_object`](crate::serialize::SerializableObject::serialize_object) call,
/// a [`serialize_argument_spec`](crate::spec::CallableSpec::serialize_argument_spec)
/// call, or the machinery driving them can report.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SerializeError {
    /// The type does not participate in serialization: its
    /// [`serialize_object`](crate::serialize::SerializableObject::serialize_object)
    /// is the default, which reports exactly this.
    Unsupported,
    /// A parsed argument was parsed against an argument spec that is not the one its
    /// callable spec declares at that index — an *out-of-band* argument spec — and the
    /// callable spec's
    /// [`serialize_argument_spec`](crate::spec::CallableSpec::serialize_argument_spec)
    /// is the default, which handles declared argument specs only. `count` is the
    /// number of argument specs the callable spec declares (`index >= count` means the
    /// index itself is out of range).
    ArgumentSpecOutOfBand {
        /// The parsed argument's index in invocation order.
        index: usize,
        /// The number of argument specs the callable spec declares.
        count: usize,
    },
}

impl SerializeError {
    /// The error the default
    /// [`serialize_object`](crate::serialize::SerializableObject::serialize_object)
    /// reports: the type does not participate in serialization.
    pub fn unsupported() -> SerializeError {
        SerializeError::Unsupported
    }
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializeError::Unsupported => {
                write!(f, "serialization is unsupported by this type")
            }
            SerializeError::ArgumentSpecOutOfBand { index, count } => write!(
                f,
                "argument #{} was parsed against an argument spec its callable spec does \
                 not declare at that index ({} declared); the callable spec must \
                 implement serialize_argument_spec to serialize it",
                index.saturating_add(1),
                count
            ),
        }
    }
}

impl core::error::Error for SerializeError {}

/// Error of the read side: what a
/// [`deserialize_object`](crate::serialize::DeserializableObject::deserialize_object)
/// call, a
/// [`deserialize_argument_spec`](crate::spec::CallableSpec::deserialize_argument_spec)
/// call, or the machinery driving them can report. Everything read is untrusted
/// input: a malformed value is an error, never a panic.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeserializeError {
    /// A serialized argument refers to its callable spec's declared argument spec by
    /// index, and the index is beyond the `count` argument specs the callable spec
    /// declares in the reading environment (the live objects the deserializing
    /// program already holds — here, the callable spec that was rebuilt or looked
    /// up for the serialized one).
    ArgumentIndexOutOfRange {
        /// The serialized argument's index in invocation order.
        index: usize,
        /// The number of argument specs the callable spec declares.
        count: usize,
    },
    /// A serialized argument carries a description of its argument spec — written by
    /// a callable spec that overrides
    /// [`serialize_argument_spec`](crate::spec::CallableSpec::serialize_argument_spec)
    /// — but the callable spec it is read against uses the default
    /// [`deserialize_argument_spec`](crate::spec::CallableSpec::deserialize_argument_spec),
    /// which reads no such description: the reading environment's callable spec is
    /// not of the type that wrote the argument.
    UnexpectedArgumentSpecPayload {
        /// The serialized argument's index in invocation order.
        index: usize,
    },
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::ArgumentIndexOutOfRange { index, count } => write!(
                f,
                "serialized argument #{} refers to a declared argument spec by index, \
                 but the callable spec declares only {} argument specs",
                index.saturating_add(1),
                count
            ),
            DeserializeError::UnexpectedArgumentSpecPayload { index } => write!(
                f,
                "serialized argument #{} carries a description of its argument spec, but \
                 the callable spec it is read against does not override \
                 deserialize_argument_spec to read one (the callable spec type that wrote \
                 the argument differs from the one reading it)",
                index.saturating_add(1)
            ),
        }
    }
}

impl core::error::Error for DeserializeError {}

/// Error of converting plain data to or from a
/// [`SerialValue`](crate::serialize::SerialValue): what the serde bridge
/// (`to_value`/`from_value`, available with the `serde` cargo feature) and the crate's
/// own conversions of its wire structures report. Writes fail on data the value model
/// cannot hold — floating-point numbers, integers outside `i64`, maps with non-string
/// keys. Reads treat the value as untrusted input: a value of the wrong kind, an
/// unknown, missing, or repeated map key, or an unknown enum variant is an error,
/// never a panic. The type is available without the `serde` feature: the crate's own
/// conversions use it too.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SerialValueError {
    /// A floating-point number was to be written or read; the value model has no
    /// floating-point variant.
    FloatRejected,
    /// A map key that is not a string was to be written; the value model's maps are
    /// string-keyed.
    NonStringMapKey,
    /// An integer does not fit its target: on writing, an integer outside the `i64`
    /// range of [`Int`](crate::serialize::SerialValue::Int); on reading, an integer
    /// outside the range of the integer type being read.
    IntegerOutOfRange {
        /// The integer, in decimal.
        value: String,
        /// The type it does not fit (`"i64"` on writing; the type being read on
        /// reading).
        target: &'static str,
    },
    /// A value of one kind was found where another was expected: `expected`
    /// describes what was expected, `found` names the kind of value found (`null`,
    /// `bool`, `int`, `str`, `bytes`, `list`, `map`, or `index`).
    TypeMismatch {
        /// What was expected, in words.
        expected: Cow<'static, str>,
        /// The kind of value found.
        found: &'static str,
    },
    /// A map lacks a required key.
    MissingField {
        /// The missing key.
        name: &'static str,
    },
    /// A map has a key that is not one of the keys expected of it.
    UnknownField {
        /// The unexpected key.
        name: String,
        /// The keys that were expected.
        expected: &'static [&'static str],
    },
    /// A map has the same key twice.
    DuplicateField {
        /// The repeated key.
        name: String,
    },
    /// An enum value names a variant the enum does not have.
    UnknownVariant {
        /// The variant name found.
        name: String,
        /// The variant names the enum has.
        expected: &'static [&'static str],
    },
    /// Any other failure, described in words — what a serde `Serialize` or
    /// `Deserialize` implementation reports through serde's `Error::custom`.
    Custom(String),
}

impl fmt::Display for SerialValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerialValueError::FloatRejected => {
                write!(f, "floating-point numbers have no serialized value form")
            }
            SerialValueError::NonStringMapKey => {
                write!(f, "map keys must be strings in the serialized value form")
            }
            SerialValueError::IntegerOutOfRange { value, target } => {
                write!(f, "integer {value} does not fit {target}")
            }
            SerialValueError::TypeMismatch { expected, found } => {
                write!(f, "expected {expected}, found {found}")
            }
            SerialValueError::MissingField { name } => write!(f, "missing key `{name}`"),
            SerialValueError::UnknownField { name, expected } => {
                write!(f, "unknown key `{name}`; expected ")?;
                write_name_list(f, expected)
            }
            SerialValueError::DuplicateField { name } => write!(f, "repeated key `{name}`"),
            SerialValueError::UnknownVariant { name, expected } => {
                write!(f, "unknown variant `{name}`; expected ")?;
                write_name_list(f, expected)
            }
            SerialValueError::Custom(message) => f.write_str(message),
        }
    }
}

/// Writes `one of `a`, `b``, or the single name, or `none` for an empty list.
fn write_name_list(f: &mut fmt::Formatter<'_>, names: &[&str]) -> fmt::Result {
    match names {
        [] => write!(f, "none"),
        [only] => write!(f, "`{only}`"),
        _ => {
            write!(f, "one of ")?;
            for (i, name) in names.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "`{name}`")?;
            }
            Ok(())
        }
    }
}

impl core::error::Error for SerialValueError {}
