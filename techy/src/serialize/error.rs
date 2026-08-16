//! The serialization error types: [`SerializeError`] (write side) and
//! [`DeserializeError`] (read side).

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
    ArgumentSpecPayloadUnexpected {
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
            DeserializeError::ArgumentSpecPayloadUnexpected { index } => write!(
                f,
                "serialized argument #{} carries a description of its argument spec, but \
                 the callable spec it is read against does not implement \
                 deserialize_argument_spec to read one (the callable spec type that wrote \
                 the argument differs from the one reading it)",
                index.saturating_add(1)
            ),
        }
    }
}

impl core::error::Error for DeserializeError {}
