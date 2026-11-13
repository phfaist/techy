//! This module defines the error types of serialization. [`SerializeError`] is the
//! error of writing objects, [`DeserializeError`] the error of reading them back, and
//! [`RegistrationError`] the error of setting up a session. [`SerialValueError`] is the
//! error of converting plain data to or from a
//! [`SerialValue`](crate::serialize::SerialValue).

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use crate::error::InconsistentDiagnosticCounts;
use crate::scopes::DefinitionKey;

use super::value::TableId;

/// The shared error value behind the `cause` field of [`SerializeError::Failed`] and
/// [`DeserializeError::Failed`]: an `Arc` (the shape of
/// [`HookFailed::cause`](crate::error::HookFailed::cause)) so that the error types stay
/// `Clone`.
type SharedCause = Arc<dyn core::error::Error + Send + Sync + 'static>;

/// Renders a source in error messages by its optional origin label: `source ‘label’`, or
/// `a source without origin label`.
struct OriginLabel<'a>(&'a Option<String>);

impl fmt::Display for OriginLabel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(label) => write!(f, "source ‘{label}’"),
            None => f.write_str("a source without origin label"),
        }
    }
}

/// The error that the write side of serialization returns.
///
/// Every operation that serializes objects returns this type: the
/// [`serialize_object`](crate::serialize::SerializableObject::serialize_object) method of
/// a serializable object, the
/// [`serialize_argument_spec`](crate::core::specs::CallableSpec::serialize_argument_spec)
/// method of a callable spec, an [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver),
/// and the methods that intern objects into a session
/// ([`SerdeSession::intern`](crate::serialize::SerdeSession::intern),
/// [`SerializeContext::intern`](crate::serialize::SerializeContext::intern)).
///
/// Most variants report that the program has not set up what the serialization needs,
/// for instance a table that is not registered or a spec without a provenance stamp.
/// Other variants report objects that cannot be serialized, for instance objects that
/// refer to one another in a cycle. An implementation of `serialize_object` or of a
/// driver reports a failure of its own as [`Failed`](SerializeError::Failed).
///
/// The variants [`InTable`](SerializeError::InTable) and
/// [`InNode`](SerializeError::InNode) record where an error occurred: in which table, and
/// at which node of a tree. Their `cause` field contains the error itself.
///
/// The read side of serialization reports [`DeserializeError`]. The guide chapter
/// [Serializing parses](crate::guide::serialize) introduces both sides.
///
/// The type does not implement `PartialEq`, because a
/// [`Failed`](SerializeError::Failed) value can contain an arbitrary underlying error.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SerializeError {
    /// The type of the object does not support serialization.
    ///
    /// The default implementation of
    /// [`serialize_object`](crate::serialize::SerializableObject::serialize_object)
    /// returns this error. A type that must implement the trait but is never serialized
    /// can therefore use an empty `impl` block. Implement the method to make the type
    /// serializable.
    Unsupported,
    /// A parsed argument has an argument spec that its callable spec does not declare at
    /// the index of the argument, and the callable spec cannot serialize such an argument
    /// spec.
    ///
    /// The default implementation of
    /// [`serialize_argument_spec`](crate::core::specs::CallableSpec::serialize_argument_spec)
    /// serializes the argument spec of the argument at index `i` as the index `i`: this
    /// argument must have the `i`-th argument spec that the callable spec declares. An
    /// argument spec that the parser supplies for one invocation alone is an
    /// *out-of-band* argument spec. To serialize out-of-band argument specs, a callable
    /// spec overrides `serialize_argument_spec` and serializes a description of the
    /// argument spec.
    ///
    /// The message of this error numbers the arguments from 1.
    ArgumentSpecOutOfBand {
        /// The index of the parsed argument in invocation order, counted from 0.
        index: usize,
        /// The number of argument specs that the callable spec declares.
        count: usize,
    },
    /// An implementation failed to serialize an object, for a reason of its own.
    ///
    /// A [`serialize_object`](crate::serialize::SerializableObject::serialize_object)
    /// method or a driver reports this error. `detail` describes the failure, and `cause`
    /// contains the underlying error if there is one;
    /// [`Error::source`](core::error::Error::source) returns it.
    ///
    /// Create the value with [`failed`](SerializeError::failed) and
    /// [`with_cause`](SerializeError::with_cause).
    Failed {
        /// The description of the failure, as a message of its own. The message of this
        /// error contains it after `Serialization failed:`.
        detail: String,
        /// The underlying error, if any. It is stored in an `Arc`, so that the error
        /// type stays `Clone`.
        cause: Option<SharedCause>,
    },
    /// A value cannot be converted to the value model of serialization.
    ///
    /// The [`SerialValueError`] tells what is wrong, for instance an integer outside the
    /// range of `i64`, a floating-point number, or a map key that is not a string. The
    /// session also reports this error, with
    /// [`NestingTooDeep`](SerialValueError::NestingTooDeep), when the serialized entry of
    /// an object is nested more deeply than
    /// [`SerialValue::MAX_NESTING_DEPTH`](crate::serialize::SerialValue::MAX_NESTING_DEPTH)
    /// allows.
    Value(SerialValueError),
    /// A table handle does not refer to a table of the session that it is used with.
    ///
    /// A [`TableHandle`](crate::serialize::TableHandle) is valid only for the session that
    /// registered its table. This error occurs when the handle comes from another session,
    /// or when the session has a table at the position of the handle, but with a driver
    /// of another type.
    UnknownTable {
        /// The table identifier of the handle.
        table: TableId,
    },
    /// The session has no table of the requested name with the expected driver type.
    ///
    /// The accessors of techy's standard tables (see
    /// [`StandardTableInterning`](crate::serialize::StandardTableInterning)) look up their
    /// table by name. For these accessors, the error means that the session was created
    /// with [`SerdeSession::empty`](crate::serialize::SerdeSession::empty) and not with
    /// [`SerdeSession::new`](crate::serialize::SerdeSession::new), or that a driver of
    /// another type is registered under the name.
    UnknownTableName {
        /// The requested table name.
        name: String,
    },
    /// The table already has the maximum number of entries (`u32::MAX`), and no further
    /// object can be interned into it.
    TableFull {
        /// The name of the table.
        table: &'static str,
    },
    /// A table position is beyond the end of its table.
    ///
    /// [`take_segment_with_main`](crate::serialize::SerdeSession::take_segment_with_main)
    /// reports this error when its main entry does not exist in the session. No segment
    /// is emitted in this case.
    IndexOutOfRange {
        /// The name of the table.
        table: &'static str,
        /// The position, counted from 0.
        index: u32,
        /// The number of entries in the table.
        len: u32,
    },
    /// The objects being serialized refer to one another in a cycle.
    ///
    /// While the session serialized an object of table `referrer`, this object interned
    /// an object of table `table` whose serialization was still in progress. Serialized
    /// references cannot form a cycle: the read side rebuilds each object from the
    /// objects that it refers to.
    ReferenceCycle {
        /// The table of the object that was interned again.
        table: &'static str,
        /// The table of the object that interned it.
        referrer: &'static str,
    },
    /// The objects refer to one another more deeply than the descent limit of the
    /// session allows.
    ///
    /// The serialization stops with this error. The limit is set with
    /// [`SerdeSession::with_descent_guard_init`](crate::serialize::SerdeSession::with_descent_guard_init).
    DescentLimitExceeded {
        /// The description of the limit that was reached, as the descent guard reports
        /// it.
        detail: String,
    },
    /// A driver produced an entry with an identifier other than the identifier of its
    /// table.
    ///
    /// A table whose objects are all of one kind declares their identifier with the
    /// [`homogeneous_identifier`](crate::serialize::ObjectSerdeDriver::homogeneous_identifier)
    /// method of its driver. Every entry that the driver produces must have this
    /// identifier. This error therefore indicates a bug in the driver.
    UnexpectedIdentifier {
        /// The name of the table.
        table: &'static str,
        /// The identifier that every entry of the table must have.
        expected: &'static str,
        /// The identifier of the entry.
        found: String,
    },
    /// A spec that can only be serialized by identity has no provenance stamp.
    ///
    /// Some spec types have no self-contained serialized form. A spec of such a type is
    /// serialized as a reference to its definition in the provider that defines it. This
    /// reference is taken from the provenance stamp of the spec
    /// ([`SpecProvenance`](crate::core::specs::SpecProvenance)). Only a package created
    /// with [`Package::new_shared`](crate::core::specs::Package::new_shared) issues
    /// provenance stamps: stamp each spec with
    /// [`Package::provenance_for`](crate::core::specs::Package::provenance_for) before
    /// inserting it, or insert it with a method that stamps the spec, such as
    /// [`Package::define_macro`](crate::core::specs::Package::define_macro).
    MissingProvenance {
        /// The name of the type of the spec.
        spec: &'static str,
    },
    /// The provider that defined a spec no longer exists, so the identity of the spec
    /// cannot be serialized.
    ///
    /// A provenance stamp refers to its provider through a weak reference. This error
    /// occurs when all strong references to the provider have been dropped. Keep the
    /// package that defines a spec alive as long as objects that refer to the spec are
    /// serialized.
    ProviderDropped {
        /// The callable type that the spec was defined under, in its `Debug` form.
        callable_type: String,
        /// The key that the spec was defined under.
        key: DefinitionKey,
    },
    /// An error that occurred while the session serialized an object into the table
    /// `table`.
    ///
    /// The session wraps every error of a driver in this variant, and `cause` contains the
    /// error. Only the innermost table is recorded: `cause` is never an `InTable` itself.
    /// It can be an [`InNode`](SerializeError::InNode) whose own `cause` is the `InTable`
    /// of another table, for instance when a tree node interned its spec and the
    /// serialization of the spec failed.
    InTable {
        /// The name of the table.
        table: &'static str,
        /// The error.
        cause: Box<SerializeError>,
    },
    /// An error that occurred while the tree driver serialized the node `node` of a tree:
    /// its payload, its argument specs, or an object that it interned.
    ///
    /// The tree driver ([`TreeSerdeDriver`](crate::serialize::TreeSerdeDriver)) wraps
    /// every error at a node in this variant, and the session wraps the result in the
    /// [`InTable`](SerializeError::InTable) of the trees table. Only the innermost node is
    /// recorded: `cause` is never an `InNode` itself.
    InNode {
        /// The position of the node in the storage order of the tree, with the root first.
        node: u32,
        /// The invocation name of the node, if the node is a callable.
        callable: Option<String>,
        /// The error.
        cause: Box<SerializeError>,
    },
}

impl SerializeError {
    /// The [`Unsupported`](SerializeError::Unsupported) error: the type does not support
    /// serialization.
    ///
    /// This is what the default
    /// [`serialize_object`](crate::serialize::SerializableObject::serialize_object) returns.
    pub fn unsupported() -> SerializeError {
        SerializeError::Unsupported
    }

    /// An implementation's own failure ([`Failed`](SerializeError::Failed)) with the given
    /// description and no underlying error.
    ///
    /// Attach one with [`with_cause`](SerializeError::with_cause).
    pub fn failed(detail: impl Into<String>) -> SerializeError {
        SerializeError::Failed { detail: detail.into(), cause: None }
    }

    /// Attach the underlying error to a [`Failed`](SerializeError::Failed); any other variant
    /// is returned unchanged.
    ///
    /// The error is stored in an `Arc`, so that the value stays `Clone`, and is reachable
    /// through [`Error::source`](core::error::Error::source).
    pub fn with_cause(self, cause: impl core::error::Error + Send + Sync + 'static) -> SerializeError {
        match self {
            SerializeError::Failed { detail, .. } => {
                SerializeError::Failed { detail, cause: Some(Arc::new(cause)) }
            }
            other => other,
        }
    }

    /// Wrap a driver's failure with the table it happened in — unless it already
    /// carries a table location (only the innermost is kept).
    pub(crate) fn in_table(self, table: &'static str) -> SerializeError {
        match self {
            located @ SerializeError::InTable { .. } => located,
            cause => SerializeError::InTable { table, cause: Box::new(cause) },
        }
    }

    /// Wrap a per-node failure of the tree driver with the node it happened at —
    /// unless it already carries a node location (only the innermost is kept).
    pub(crate) fn in_node(self, node: u32, callable: Option<String>) -> SerializeError {
        match self {
            located @ SerializeError::InNode { .. } => located,
            cause => SerializeError::InNode { node, callable, cause: Box::new(cause) },
        }
    }
}

impl From<SerialValueError> for SerializeError {
    fn from(error: SerialValueError) -> SerializeError {
        SerializeError::Value(error)
    }
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializeError::Unsupported => {
                write!(f, "Serialization is not supported by the type of this object")
            }
            SerializeError::ArgumentSpecOutOfBand { index, count } => write!(
                f,
                "Unsupported out-of-band argument spec of argument #{} (the callable spec \
                 declares {} argument specs): the callable spec must override \
                 ‘serialize_argument_spec’ to serialize it",
                index.saturating_add(1),
                count
            ),
            SerializeError::Failed { detail, .. } => {
                write!(f, "Serialization failed: {detail}")
            }
            SerializeError::Value(error) => {
                write!(f, "Failed to convert a value to the serialized value model: {error}")
            }
            SerializeError::UnknownTable { table } => write!(
                f,
                "Unknown table #{} in this session: the table handle comes from another \
                 session, or the table has a driver of another type",
                table.ordinal()
            ),
            SerializeError::UnknownTableName { name } => write!(
                f,
                "Missing table ‘{name}’: no table of this name with the expected \
                 driver type is registered in this session"
            ),
            SerializeError::TableFull { table } => {
                write!(f, "Table ‘{table}’ is full (u32::MAX entries)")
            }
            SerializeError::IndexOutOfRange { table, index, len } => write!(
                f,
                "Position #{index} is out of range for table ‘{table}’ ({len} entries)"
            ),
            SerializeError::ReferenceCycle { table, referrer } => write!(
                f,
                "Reference cycle: an object of table ‘{table}’ refers back to \
                 itself (interned again by an object of table ‘{referrer}’)"
            ),
            SerializeError::DescentLimitExceeded { detail } => {
                write!(f, "Objects nested too deeply for serialization: {detail}")
            }
            SerializeError::UnexpectedIdentifier { table, expected, found } => write!(
                f,
                "Misconfigured driver of table ‘{table}’: The driver produced an \
                 entry with identifier ‘{found}’ (expected ‘{expected}’)"
            ),
            SerializeError::MissingProvenance { spec } => write!(
                f,
                "Missing provenance stamp on a spec of type ‘{spec}’ (specs of \
                 this type are serialized by identity only)"
            ),
            SerializeError::ProviderDropped { callable_type, key } => write!(
                f,
                "Provider of the spec with {key} (callable type \
                 ‘{callable_type}’) no longer exists"
            ),
            SerializeError::InTable { table, cause } => {
                write!(f, "Failed to serialize an object into table ‘{table}’: {cause}")
            }
            SerializeError::InNode { node, callable, cause } => match callable {
                Some(callable) => write!(
                    f,
                    "Failed to serialize node #{node} (callable ‘{callable}’) of a \
                     tree: {cause}"
                ),
                None => write!(f, "Failed to serialize node #{node} of a tree: {cause}"),
            },
        }
    }
}

impl core::error::Error for SerializeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            SerializeError::Failed { cause, .. } => {
                cause.as_ref().map(|cause| &**cause as &(dyn core::error::Error + 'static))
            }
            SerializeError::Value(error) => Some(error),
            SerializeError::InTable { cause, .. } | SerializeError::InNode { cause, .. } => Some(&**cause),
            _ => None,
        }
    }
}

/// The error that the read side of serialization returns.
///
/// Every operation that reads serialized data returns this type: the
/// [`deserialize_object`](crate::serialize::DeserializableObject::deserialize_object)
/// function of a deserializable type, the
/// [`deserialize_argument_spec`](crate::core::specs::CallableSpec::deserialize_argument_spec)
/// method of a callable spec, an [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver),
/// an [`IdentifierResolver`](crate::serialize::IdentifierResolver), and the methods that
/// read data from a session
/// ([`SerdeSession::push_segment`](crate::serialize::SerdeSession::push_segment),
/// [`SerdeSession::object`](crate::serialize::SerdeSession::object),
/// [`DeserializeContext::object`](crate::serialize::DeserializeContext::object)).
///
/// The read side treats serialized data as untrusted input. Malformed data, for instance
/// a value of the wrong kind, a position beyond the end of a table, a reference cycle or
/// an unknown identifier, is reported as an error and never causes a panic.
///
/// Some variants report that the data was written by a program that does not match the
/// reading program:
///
/// - another version of techy
///   ([`UnsupportedVersion`](DeserializeError::UnsupportedVersion));
/// - another configuration ([`ProfileMismatch`](DeserializeError::ProfileMismatch));
/// - another language ([`FeatureAbsent`](DeserializeError::FeatureAbsent));
/// - another set of packages ([`MissingProvider`](DeserializeError::MissingProvider),
///   [`MissingDefinition`](DeserializeError::MissingDefinition)).
///
/// Other variants report that the reading program has not registered or supplied what
/// the data needs ([`UnknownIdentifier`](DeserializeError::UnknownIdentifier),
/// [`UnknownTableName`](DeserializeError::UnknownTableName),
/// [`NoSourceTextSupplier`](DeserializeError::NoSourceTextSupplier)).
///
/// The variants [`InEntry`](DeserializeError::InEntry) and
/// [`InNode`](DeserializeError::InNode) record where an error occurred: in which entry of
/// which table, and at which node of a tree. Their `cause` field contains the error
/// itself.
///
/// The write side of serialization reports [`SerializeError`]. The guide chapter
/// [Serializing parses](crate::guide::serialize) introduces both sides.
///
/// The type does not implement `PartialEq`, because a
/// [`Failed`](DeserializeError::Failed) value can contain an arbitrary underlying error.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DeserializeError {
    /// A serialized argument refers to an argument spec that the callable spec of the
    /// reading program does not declare.
    ///
    /// The argument at index `index` is serialized as a reference to the `index`-th
    /// argument spec of its callable spec. The callable spec that was rebuilt or looked
    /// up by the reading program declares only `count` argument specs: it is not the
    /// callable spec that the data was written with.
    ///
    /// The message of this error numbers the arguments from 1.
    ArgumentIndexOutOfRange {
        /// The index of the serialized argument in invocation order, counted from 0.
        index: usize,
        /// The number of argument specs that the callable spec declares.
        count: usize,
    },
    /// A serialized argument contains a description of its argument spec, and the
    /// callable spec of the reading program does not read such descriptions.
    ///
    /// A callable spec that overrides
    /// [`serialize_argument_spec`](crate::core::specs::CallableSpec::serialize_argument_spec)
    /// writes such a description. The callable spec that reads the argument uses the
    /// default implementation of
    /// [`deserialize_argument_spec`](crate::core::specs::CallableSpec::deserialize_argument_spec),
    /// which reads only the index of a declared argument spec. The callable spec of the
    /// reading program is therefore not of the type that wrote the argument.
    ///
    /// The message of this error numbers the arguments from 1.
    UnexpectedArgumentSpecPayload {
        /// The index of the serialized argument in invocation order, counted from 0.
        index: usize,
    },
    /// An implementation failed to rebuild an object, for a reason of its own.
    ///
    /// A
    /// [`deserialize_object`](crate::serialize::DeserializableObject::deserialize_object)
    /// function, a driver or a resolver reports this error, for instance when the reading
    /// program lacks an object that the data refers to. `detail` describes the failure,
    /// and `cause` contains the underlying error if there is one;
    /// [`Error::source`](core::error::Error::source) returns it.
    ///
    /// Create the value with [`failed`](DeserializeError::failed) and
    /// [`with_cause`](DeserializeError::with_cause).
    Failed {
        /// The description of the failure, as a message of its own. The message of this
        /// error contains it after `Deserialization failed:`.
        detail: String,
        /// The underlying error, if any. It is stored in an `Arc`, so that the error
        /// type stays `Clone`.
        cause: Option<SharedCause>,
    },
    /// A serialized value has the wrong form.
    ///
    /// The [`SerialValueError`] tells what is wrong, for instance a value of the wrong
    /// kind, a missing, unknown or repeated map key, an integer out of range, an unknown
    /// enum variant, or a value nested too deeply.
    Value(SerialValueError),
    /// A table handle does not refer to a table of the session that it is used with.
    ///
    /// A [`TableHandle`](crate::serialize::TableHandle) is valid only for the session that
    /// registered its table. This error occurs when the handle comes from another session,
    /// or when the session has a table at the position of the handle, but with a driver
    /// of another type.
    UnknownTable {
        /// The table identifier of the handle.
        table: TableId,
    },
    /// A reference refers to a position beyond the end of its table.
    IndexOutOfRange {
        /// The name of the table.
        table: &'static str,
        /// The position that the reference refers to, counted from 0.
        index: u32,
        /// The number of entries in the table.
        len: u32,
    },
    /// A typed table position was read through the handle of another table.
    ///
    /// A typed position ([`SerialIndex`](crate::serialize::SerialIndex)) contains the
    /// identifier of its table in the session that created the position. In a session
    /// that registered its tables in another order, this identifier refers to another
    /// table.
    WrongTable {
        /// The name of the table of the handle.
        expected: &'static str,
        /// The table identifier that the position contains.
        found: TableId,
    },
    /// The references between the serialized entries form a cycle.
    ///
    /// While the session deserialized entry `referrer_index` of table `referrer_table`,
    /// this entry read entry `index` of table `table`, whose deserialization was still in
    /// progress. A cycle cannot be rebuilt: the read side rebuilds each object from the
    /// objects that it refers to.
    ReferenceCycle {
        /// The table of the entry that was read again.
        table: &'static str,
        /// The position of the entry that was read again.
        index: u32,
        /// The table of the entry that read it.
        referrer_table: &'static str,
        /// The position of the entry that read it.
        referrer_index: u32,
    },
    /// The entries refer to one another more deeply than the descent limit of the
    /// session allows.
    ///
    /// The deserialization stops with this error. The limit is set with
    /// [`SerdeSession::with_descent_guard_init`](crate::serialize::SerdeSession::with_descent_guard_init).
    DescentLimitExceeded {
        /// The description of the limit that was reached, as the descent guard reports
        /// it.
        detail: String,
    },
    /// The reading session has no reader for the identifier of an entry.
    ///
    /// No reader is registered for `identifier` in the table `table`, and no resolver
    /// ([`TableHandle::register_resolver`](crate::serialize::TableHandle::register_resolver))
    /// provided one. Register the type that reads such entries with
    /// [`TableHandle::register_type`](crate::serialize::TableHandle::register_type), or,
    /// for the annotations of a tree, with
    /// [`TableHandle::register_annotation`](crate::serialize::TableHandle::register_annotation).
    /// [`latexlike::serialize::register`](crate::latexlike::serialize::register) registers
    /// the readers of all identifiers of techy and of the `latexlike` preset.
    UnknownIdentifier {
        /// The name of the table.
        table: &'static str,
        /// The identifier of the entry.
        identifier: String,
    },
    /// The segment has a format version that this version of techy does not read.
    ///
    /// The data was written by another version of techy. This version reads segments of
    /// version [`Segment::VERSION`](crate::serialize::Segment::VERSION).
    UnsupportedVersion {
        /// The version of the segment.
        found: u32,
        /// The version that this version of techy reads.
        expected: u32,
    },
    /// The profile of the segment differs from the profile of the session.
    ///
    /// The session requires the profile `expected`, which is set with
    /// [`SerdeSession::set_profile`](crate::serialize::SerdeSession::set_profile). The
    /// segment has the profile `found`, or no profile. The data was therefore written for
    /// another configuration. The session reports this error before it absorbs any entry
    /// of the segment.
    ProfileMismatch {
        /// The profile of the session.
        expected: String,
        /// The profile of the segment, or `None` if the segment has no profile.
        found: Option<String>,
    },
    /// The session has no table of the requested name with the expected driver type.
    ///
    /// This error occurs when the segment that the session absorbs lists such a table. It
    /// also occurs when an accessor of techy's standard tables (see
    /// [`StandardTableReading`](crate::serialize::StandardTableReading)) is used on a
    /// session that does not have the table, for instance a session created with
    /// [`SerdeSession::empty`](crate::serialize::SerdeSession::empty).
    UnknownTableName {
        /// The requested table name.
        name: String,
    },
    /// A serialized span does not fit its source.
    ///
    /// Either the byte range `start..end` extends beyond the `len` bytes of the source,
    /// or `start` is greater than `end`.
    SpanOutOfBounds {
        /// The span's start (byte offset, inclusive).
        start: usize,
        /// The span's end (byte offset, exclusive).
        end: usize,
        /// The source's length in bytes.
        len: usize,
    },
    /// A serialized span starts or ends inside a multi-byte character of its source.
    SpanNotOnCharBoundary {
        /// The span's start (byte offset, inclusive).
        start: usize,
        /// The span's end (byte offset, exclusive).
        end: usize,
    },
    /// The serialized data uses a parsing feature that the reading language declares
    /// absent.
    ///
    /// The serialized rules of a parsing state contain the section of the feature
    /// `feature`, or the serialized scope stack of a state is not empty, and the
    /// [`Lang::Features`](crate::core::Lang::Features) of the reading language declare
    /// this feature absent. The language that reads the data is therefore not the
    /// language that wrote it, or it declares other features.
    FeatureAbsent {
        /// The feature's name (`whitespace`, `paragraphs`, `groups`, `commands`,
        /// `comments`, `specials`, `forbidden_chars`, `scopes`).
        feature: &'static str,
    },
    /// The text of a source is not embedded in the data, and no supplier of source text
    /// is configured.
    ///
    /// A source can be serialized by reference: its entry records the length of its text
    /// and optionally a digest of the text, and the reading program supplies the text.
    /// Configure a supplier with
    /// [`SourceSerdeDriver::with_text_supplier`](crate::serialize::SourceSerdeDriver::with_text_supplier).
    NoSourceTextSupplier {
        /// The origin label of the source, if it has one.
        origin: Option<String>,
    },
    /// The text supplied for a source serialized by reference does not have the length
    /// that its entry records.
    ///
    /// Usually the text behind the source, for instance a file, has changed since the
    /// data was written.
    SourceLengthMismatch {
        /// The origin label of the source, if it has one.
        origin: Option<String>,
        /// The length in bytes that the entry records.
        expected: usize,
        /// The length in bytes of the supplied text.
        found: usize,
    },
    /// The text supplied for a source serialized by reference does not match the digest
    /// that its entry records.
    ///
    /// The [`digest_matches`](crate::serialize::SourceTextSupplier::digest_matches)
    /// method of the supplier returned `false`. Usually the text behind the source, for
    /// instance a file, has changed since the data was written.
    SourceDigestMismatch {
        /// The origin label of the source, if it has one.
        origin: Option<String>,
        /// The name of the digest algorithm, as the entry records it.
        algorithm: String,
    },
    /// The segment lists a table twice, or lists two tables with the same writer-side
    /// table identifier.
    DuplicateSegmentTable {
        /// The name of the table.
        name: String,
    },
    /// The segment does not continue the data that the session has absorbed.
    ///
    /// The entries of the segment for the table `table` start at position `found`, and
    /// the table has `expected` entries. Push the segments of a stream in order, each
    /// exactly once, into a session that has absorbed all earlier segments.
    SegmentOutOfOrder {
        /// The name of the table.
        table: &'static str,
        /// The number of entries in the table: the position at which the entries of the
        /// segment must start.
        expected: u32,
        /// The position at which the entries of the segment start.
        found: u32,
    },
    /// The session has entries that were interned since its last emitted segment, so it
    /// cannot absorb a segment.
    ///
    /// A segment continues the data that the session has emitted. Emit the pending
    /// entries of the table `table` with
    /// [`take_segment`](crate::serialize::SerdeSession::take_segment) first.
    UnemittedEntries {
        /// The name of the table.
        table: &'static str,
    },
    /// A reference in the segment contains a writer-side table identifier that the table
    /// directory of the segment does not list.
    UnknownWriterTable {
        /// The writer-side table identifier.
        table: TableId,
    },
    /// Absorbing the segment would give the table `table` more entries than the maximum
    /// (`u32::MAX`).
    TableFull {
        /// The name of the table.
        table: &'static str,
    },
    /// A check of the session on its own bookkeeping failed, which indicates a bug in
    /// techy.
    ///
    /// The error is not caused by the input or by an implementation. The session undoes
    /// the operation that found the error (a segment that was being absorbed is dropped),
    /// and the session stays usable.
    Internal {
        /// The description of the failed check, with the entry concerned.
        detail: String,
    },
    /// The data refers to a provider that the reading program does not supply.
    ///
    /// The session looks up providers by name in its
    /// [`KnownProviders`](crate::serialize::KnownProviders), which is stored in the user
    /// data of the session. This error occurs when the `KnownProviders` value has neither
    /// a provider nor a recipe of the name `name`, or when the session has no
    /// `KnownProviders` value. Add the package that the parse used, or a recipe that
    /// builds it, before reading.
    MissingProvider {
        /// The name of the provider.
        name: String,
    },
    /// The data refers to a definition that its provider does not contain in the reading
    /// program.
    ///
    /// The provider `provider`, a package that the reading program supplied, has no
    /// definition with the key `key` under the callable type `callable_type`. This package
    /// is therefore not the package that the data was written with.
    MissingDefinition {
        /// The name of the provider.
        provider: String,
        /// The callable type, in its `Debug` form.
        callable_type: String,
        /// The key that the spec was defined under.
        key: DefinitionKey,
    },
    /// The serialized data of a diagnostic contains a value that a
    /// [`DiagnosticValue`](crate::error::DiagnosticValue) cannot represent.
    ///
    /// The data of a diagnostic is the value that the
    /// [`serializable_data`](crate::error::DiagnosticInfo::serializable_data) method of
    /// its condition returns. This error occurs when the serialized data contains a byte
    /// string or a table position, which `serializable_data` never produces. `kind` tells
    /// which of the two the value is, and `path` tells where it is.
    UnrepresentableDiagnosticValue {
        /// The kind of the value: `bytes` or `index`.
        kind: &'static str,
        /// The location of the value in the data: the map keys and list positions from
        /// the root of the data, as in `error.cause_chain[2]`. The path is empty if the
        /// data itself is the value.
        path: String,
    },
    /// The counts of a serialized diagnostics collection contradict one another, so
    /// [`Diagnostics::from_parts`](crate::error::Diagnostics::from_parts) rejected them.
    ///
    /// The [`InconsistentDiagnosticCounts`] value contains the counts that were read and
    /// the invariant that they violate. A collection built with
    /// [`Diagnostics::push`](crate::error::Diagnostics::push) always satisfies these
    /// invariants.
    InconsistentDiagnosticCounts(
        /// The counts that were read and the invariant that they violate.
        InconsistentDiagnosticCounts,
    ),
    /// An error that occurred while the session deserialized the entry at position
    /// `index` of the table `table`.
    ///
    /// The session wraps every error of a driver in this variant, and `cause` contains the
    /// error. `identifier` is the identifier of the entry if it is known: the identifier
    /// of a table whose objects are all of one kind, or else the identifier that the
    /// entry contains, unless the entry itself is malformed.
    ///
    /// Only the innermost entry is recorded: `cause` is never an `InEntry` itself. It can
    /// be an [`InNode`](DeserializeError::InNode) whose own `cause` is the `InEntry` of
    /// another table, for instance when a tree node refers to a state that cannot be
    /// rebuilt.
    InEntry {
        /// The name of the table.
        table: &'static str,
        /// The position of the entry in the table.
        index: u32,
        /// The identifier of the entry, if it is known.
        identifier: Option<Cow<'static, str>>,
        /// The error.
        cause: Box<DeserializeError>,
    },
    /// An error that occurred while the tree driver rebuilt the node `node` of a tree:
    /// its structure, its payload, or an object that it refers to.
    ///
    /// The tree driver ([`TreeSerdeDriver`](crate::serialize::TreeSerdeDriver)) wraps
    /// every error at a node in this variant, and the session wraps the result in the
    /// [`InEntry`](DeserializeError::InEntry) of the entry of the tree. Only the innermost
    /// node is recorded: `cause` is never an `InNode` itself.
    InNode {
        /// The position of the node in the serialized node list, which is the storage
        /// order of the tree, with the root first.
        node: u32,
        /// The invocation name of the node, if the node is a callable.
        callable: Option<String>,
        /// The error.
        cause: Box<DeserializeError>,
    },
}

impl DeserializeError {
    /// An implementation's own failure ([`Failed`](DeserializeError::Failed)) with the given
    /// description and no underlying error.
    ///
    /// Attach one with [`with_cause`](DeserializeError::with_cause).
    pub fn failed(detail: impl Into<String>) -> DeserializeError {
        DeserializeError::Failed { detail: detail.into(), cause: None }
    }

    /// Attach the underlying error to a [`Failed`](DeserializeError::Failed); any other
    /// variant is returned unchanged.
    ///
    /// The error is stored in an `Arc`, so that the value stays `Clone`, and is reachable
    /// through [`Error::source`](core::error::Error::source).
    pub fn with_cause(self, cause: impl core::error::Error + Send + Sync + 'static) -> DeserializeError {
        match self {
            DeserializeError::Failed { detail, .. } => {
                DeserializeError::Failed { detail, cause: Some(Arc::new(cause)) }
            }
            other => other,
        }
    }

    /// Wrap a driver's failure with the entry it happened in — unless it already
    /// carries an entry location (only the innermost is kept).
    pub(crate) fn in_entry(
        self,
        table: &'static str,
        index: u32,
        identifier: Option<Cow<'static, str>>,
    ) -> DeserializeError {
        match self {
            located @ DeserializeError::InEntry { .. } => located,
            cause => DeserializeError::InEntry { table, index, identifier, cause: Box::new(cause) },
        }
    }

    /// Wrap a per-node failure of the tree driver with the node it happened at —
    /// unless it already carries a node location (only the innermost is kept).
    pub(crate) fn in_node(self, node: u32, callable: Option<String>) -> DeserializeError {
        match self {
            located @ DeserializeError::InNode { .. } => located,
            cause => DeserializeError::InNode { node, callable, cause: Box::new(cause) },
        }
    }
}

impl From<SerialValueError> for DeserializeError {
    fn from(error: SerialValueError) -> DeserializeError {
        DeserializeError::Value(error)
    }
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::ArgumentIndexOutOfRange { index, count } => write!(
                f,
                "Missing argument spec for serialized argument #{}: the callable spec \
                 declares only {} argument specs",
                index.saturating_add(1),
                count
            ),
            DeserializeError::UnexpectedArgumentSpecPayload { index } => write!(
                f,
                "Unexpected description of the argument spec of serialized argument #{}: \
                 the callable spec does not override ‘deserialize_argument_spec’",
                index.saturating_add(1)
            ),
            DeserializeError::Failed { detail, .. } => {
                write!(f, "Deserialization failed: {detail}")
            }
            DeserializeError::Value(error) => {
                write!(f, "Malformed serialized value: {error}")
            }
            DeserializeError::UnknownTable { table } => write!(
                f,
                "Unknown table #{} in this session: the table handle comes from another \
                 session, or the table has a driver of another type",
                table.ordinal()
            ),
            DeserializeError::IndexOutOfRange { table, index, len } => write!(
                f,
                "Position #{index} is out of range for table ‘{table}’ ({len} entries)"
            ),
            DeserializeError::WrongTable { expected, found } => write!(
                f,
                "Table mismatch: a position of table #{} was read through the handle of table \
                 ‘{expected}’",
                found.ordinal()
            ),
            DeserializeError::ReferenceCycle { table, index, referrer_table, referrer_index } => {
                write!(
                    f,
                    "Reference cycle: entry #{index} of table ‘{table}’ refers back \
                     to itself (read again by entry #{referrer_index} of table \
                     ‘{referrer_table}’)"
                )
            }
            DeserializeError::DescentLimitExceeded { detail } => {
                write!(f, "Serialized entries nested too deeply: {detail}")
            }
            DeserializeError::UnknownIdentifier { table, identifier } => write!(
                f,
                "Missing reader for identifier ‘{identifier}’ in table \
                 ‘{table}’"
            ),
            DeserializeError::UnsupportedVersion { found, expected } => write!(
                f,
                "Unsupported segment version {found} (this version of techy reads version \
                 {expected})"
            ),
            DeserializeError::ProfileMismatch { expected, found } => match found {
                Some(found) => write!(
                    f,
                    "Segment profile ‘{found}’ does not match the session profile \
                     ‘{expected}’"
                ),
                None => write!(
                    f,
                    "Missing segment profile: the session requires profile \
                     ‘{expected}’"
                ),
            },
            DeserializeError::UnknownTableName { name } => write!(
                f,
                "Missing table ‘{name}’: no table of this name with the expected \
                 driver type is registered in this session"
            ),
            DeserializeError::SpanOutOfBounds { start, end, len } => write!(
                f,
                "Invalid serialized span {start}..{end}: the source has {len} bytes"
            ),
            DeserializeError::SpanNotOnCharBoundary { start, end } => write!(
                f,
                "Invalid serialized span {start}..{end}: the span must start and end on \
                 character boundaries of the source"
            ),
            DeserializeError::FeatureAbsent { feature } => write!(
                f,
                "Unsupported parsing feature ‘{feature}’ in serialized data (the \
                 reading language declares this feature absent)"
            ),
            DeserializeError::NoSourceTextSupplier { origin } => write!(
                f,
                "Missing supplier of referenced source text for {}",
                OriginLabel(origin)
            ),
            DeserializeError::SourceLengthMismatch { origin, expected, found } => write!(
                f,
                "Text supplied for {} has {found} bytes; the entry records {expected} bytes",
                OriginLabel(origin)
            ),
            DeserializeError::SourceDigestMismatch { origin, algorithm } => write!(
                f,
                "Text supplied for {} does not match the recorded \
                 ‘{algorithm}’ digest",
                OriginLabel(origin)
            ),
            DeserializeError::DuplicateSegmentTable { name } => write!(
                f,
                "Duplicate table ‘{name}’ in segment (repeated name or writer-side \
                 table identifier)"
            ),
            DeserializeError::SegmentOutOfOrder { table, expected, found } => write!(
                f,
                "Segment out of order: its entries for table ‘{table}’ start at \
                 position {found}; the table has {expected} entries"
            ),
            DeserializeError::UnemittedEntries { table } => write!(
                f,
                "Unemitted entries in table ‘{table}’: call \
                 ‘take_segment’ before absorbing a segment"
            ),
            DeserializeError::UnknownWriterTable { table } => write!(
                f,
                "Unknown writer-side table #{} in a reference: the table directory of the \
                 segment does not list it",
                table.ordinal()
            ),
            DeserializeError::TableFull { table } => write!(
                f,
                "Table ‘{table}’ is full: absorbing the segment would exceed \
                 u32::MAX entries"
            ),
            DeserializeError::Internal { detail } => {
                write!(f, "Internal error in the serialization session (a bug in techy): {detail}")
            }
            DeserializeError::MissingProvider { name } => write!(
                f,
                "Missing provider ‘{name}’ in the ‘KnownProviders’ \
                 of the session"
            ),
            DeserializeError::MissingDefinition { provider, callable_type, key } => write!(
                f,
                "Missing definition with {key} (callable type ‘{callable_type}’) \
                 in provider ‘{provider}’"
            ),
            DeserializeError::UnrepresentableDiagnosticValue { kind, path } => {
                write!(
                    f,
                    "Invalid value of kind ‘{kind}’ in the data of a serialized \
                     diagnostic, "
                )?;
                if path.is_empty() {
                    write!(f, "at the root")?;
                } else {
                    write!(f, "at ‘{path}’")?;
                }
                write!(
                    f,
                    " (diagnostic values can contain only null, booleans, integers, strings, \
                     lists, and string-keyed maps)"
                )
            }
            DeserializeError::InconsistentDiagnosticCounts(counts) => {
                write!(f, "{counts}")
            }
            DeserializeError::InEntry { table, index, identifier, cause } => match identifier {
                Some(identifier) => write!(
                    f,
                    "Failed to deserialize entry #{index} (identifier \
                     ‘{identifier}’) of table ‘{table}’: {cause}"
                ),
                None => write!(
                    f,
                    "Failed to deserialize entry #{index} of table ‘{table}’: \
                     {cause}"
                ),
            },
            DeserializeError::InNode { node, callable, cause } => match callable {
                Some(callable) => write!(
                    f,
                    "Failed to rebuild node #{node} (callable ‘{callable}’) of a \
                     tree: {cause}"
                ),
                None => write!(f, "Failed to rebuild node #{node} of a tree: {cause}"),
            },
        }
    }
}

impl core::error::Error for DeserializeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            DeserializeError::Failed { cause, .. } => {
                cause.as_ref().map(|cause| &**cause as &(dyn core::error::Error + 'static))
            }
            DeserializeError::Value(error) => Some(error),
            DeserializeError::InconsistentDiagnosticCounts(error) => Some(error),
            DeserializeError::InEntry { cause, .. } | DeserializeError::InNode { cause, .. } => Some(&**cause),
            _ => None,
        }
    }
}

/// The error that the setup of a [`SerdeSession`](crate::serialize::SerdeSession)
/// returns.
///
/// [`register_table`](crate::serialize::SerdeSession::register_table) returns this
/// error, and so do the methods that register the readers and the resolvers of a table
/// ([`TableHandle::register_type`](crate::serialize::TableHandle::register_type) and the
/// related methods). Each variant reports a call that violates the documented
/// requirements of the method.
///
/// The setup takes place before any data is written or read. The errors of writing and
/// reading are [`SerializeError`] and [`DeserializeError`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegistrationError {
    /// The session already has a table of this name.
    DuplicateTableName {
        /// The name of the table.
        name: &'static str,
    },
    /// The session already has the maximum number of tables (`u32::MAX`).
    TooManyTables,
    /// A table handle does not refer to a table of the session that it is used with.
    ///
    /// The handle comes from another session, or the session has a table at the position
    /// of the handle, but with a driver of another type.
    UnknownTable {
        /// The table identifier of the handle.
        table: TableId,
    },
    /// The session has no table of the requested name with the expected driver type.
    ///
    /// A registration function that looks up techy's standard tables by name, such as
    /// [`register_core_readers`](crate::serialize::register_core_readers), was called on a
    /// session that does not have one of these tables.
    UnknownTableName {
        /// The requested table name.
        name: String,
    },
    /// The table already has a reader for this identifier.
    ///
    /// The reader was registered earlier, or a resolver of the table provided it earlier
    /// and the table stored it.
    DuplicateIdentifier {
        /// The name of the table.
        table: &'static str,
        /// The identifier.
        identifier: String,
    },
    /// The driver of the table returns the empty string from
    /// [`homogeneous_identifier`](crate::serialize::ObjectSerdeDriver::homogeneous_identifier).
    ///
    /// An identifier is a non-empty string. The driver of a table whose objects are of
    /// several kinds returns `None`.
    EmptyHomogeneousIdentifier {
        /// The name of the table.
        table: &'static str,
    },
    /// The tree annotation type is already registered in the trees table.
    ///
    /// Each annotation type is registered once, with
    /// [`TableHandle::register_annotation`](crate::serialize::TableHandle::register_annotation).
    /// The unit annotation, which the parser output has, is registered from the start.
    DuplicateAnnotationType {
        /// The name of the trees table.
        table: &'static str,
    },
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistrationError::DuplicateTableName { name } => {
                write!(f, "Duplicate table name ‘{name}’ in this session")
            }
            RegistrationError::TooManyTables => {
                write!(f, "Too many tables: the session already has u32::MAX tables")
            }
            RegistrationError::UnknownTable { table } => write!(
                f,
                "Unknown table #{} in this session: the table handle comes from another \
                 session, or the table has a driver of another type",
                table.ordinal()
            ),
            RegistrationError::UnknownTableName { name } => write!(
                f,
                "Missing table ‘{name}’: no table of this name with the expected \
                 driver type is registered in this session"
            ),
            RegistrationError::DuplicateIdentifier { table, identifier } => write!(
                f,
                "Duplicate reader for identifier ‘{identifier}’ in table \
                 ‘{table}’"
            ),
            RegistrationError::EmptyHomogeneousIdentifier { table } => write!(
                f,
                "Misconfigured driver of table ‘{table}’: The homogeneous \
                 identifier may not be empty; return ‘None’ for a table with \
                 objects of several kinds"
            ),
            RegistrationError::DuplicateAnnotationType { table } => write!(
                f,
                "Duplicate registration of a tree annotation type in table \
                 ‘{table}’"
            ),
        }
    }
}

impl core::error::Error for RegistrationError {}

/// The error that the conversion of plain data to or from a
/// [`SerialValue`](crate::serialize::SerialValue) returns.
///
/// techy's own conversions of its serialized structures return this error, and so does
/// the serde bridge (`to_value` and `from_value`) with the `serde` cargo feature. The
/// type is available without the feature.
///
/// Writing fails on data that the value model cannot represent: floating-point numbers,
/// integers outside the range of `i64`, map keys that are not strings, and map keys that
/// begin with `$`.
///
/// Reading treats the value as untrusted input. A value of the wrong kind, an unknown,
/// missing or repeated map key, an unknown enum variant, or a value nested too deeply is
/// reported as an error and never causes a panic.
///
/// `to_value` does not check the nesting depth. A session rejects a value that is nested
/// too deeply when it interns the value, and reports
/// [`NestingTooDeep`](SerialValueError::NestingTooDeep).
///
/// A conversion error that occurs during serialization or deserialization is returned as
/// [`SerializeError::Value`] or [`DeserializeError::Value`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SerialValueError {
    /// The data contains a floating-point number. The value model has no floating-point
    /// values.
    FloatRejected,
    /// The data contains a map key that is not a string. The maps of the value model have
    /// string keys.
    NonStringMapKey,
    /// The data contains a map key that begins with `$`.
    ///
    /// The value model reserves this prefix for the objects of its canonical rendering
    /// (`$bytes` and `$index`). When a serialized value is read, a reserved key is rejected
    /// as well, but the error has the error type of the format: for a format whose error
    /// type is not `SerialValueError`, it is the `custom` error of the format, with the
    /// message of this variant.
    ReservedMapKey {
        /// The map key.
        key: String,
    },
    /// An integer does not fit its target type.
    ///
    /// When writing, the target type is `i64`, the type of
    /// [`Int`](crate::serialize::SerialValue::Int). When reading, the target type is the
    /// integer type that is read.
    IntegerOutOfRange {
        /// The integer, in decimal notation.
        value: String,
        /// The name of the target type, for instance `"i64"`.
        target: &'static str,
    },
    /// A value has another kind than the expected kind.
    ///
    /// `expected` describes the expected value, and `found` is the kind of the value
    /// found: `null`, `bool`, `int`, `str`, `bytes`, `list`, `map` or `index`.
    TypeMismatch {
        /// A description of the expected value, for instance `a string`.
        expected: Cow<'static, str>,
        /// The kind of the value found.
        found: &'static str,
    },
    /// A map does not have a required key.
    MissingField {
        /// The missing key.
        name: &'static str,
    },
    /// A map has a key that is not one of its expected keys.
    UnknownField {
        /// The unexpected key.
        name: String,
        /// The expected keys.
        expected: &'static [&'static str],
    },
    /// A map has the same key twice.
    DuplicateField {
        /// The repeated key.
        name: String,
    },
    /// An enum value has a variant name that the enum does not have.
    UnknownVariant {
        /// The variant name found.
        name: String,
        /// The variant names of the enum.
        expected: &'static [&'static str],
    },
    /// A value is nested more deeply than the value model allows: more than `limit` lists
    /// and maps enclose a part of the value.
    ///
    /// `limit` is
    /// [`SerialValue::MAX_NESTING_DEPTH`](crate::serialize::SerialValue::MAX_NESTING_DEPTH).
    /// Its documentation also describes how the structure of a segment counts against the
    /// limit.
    ///
    /// This error is reported when a segment is converted from its serialized form, when
    /// a session absorbs a segment, and when a session interns an object whose entry would
    /// exceed the limit. When a value is read through serde (with the `serde` feature, by
    /// the `Deserialize` implementations of `SerialValue` and `Segment`), the same check
    /// runs, but the error has the error type of the format, as for
    /// [`ReservedMapKey`](SerialValueError::ReservedMapKey).
    NestingTooDeep {
        /// The greatest nesting depth allowed.
        limit: usize,
    },
    /// Any other error, with its message. A serde `Serialize` or `Deserialize`
    /// implementation reports such an error through the `custom` method of serde's error
    /// traits.
    Custom(String),
}

impl fmt::Display for SerialValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerialValueError::FloatRejected => {
                write!(f, "Unsupported floating-point number (serialized values have no floating-point type)")
            }
            SerialValueError::NonStringMapKey => {
                write!(f, "Invalid map key: map keys must be strings")
            }
            SerialValueError::ReservedMapKey { key } => {
                write!(f, "Invalid map key ‘{key}’: map keys may not begin with ‘$’")
            }
            SerialValueError::IntegerOutOfRange { value, target } => {
                write!(f, "Integer {value} is out of range for ‘{target}’")
            }
            SerialValueError::TypeMismatch { expected, found } => {
                write!(f, "Invalid value of kind ‘{found}’: expected {expected}")
            }
            SerialValueError::MissingField { name } => write!(f, "Missing key ‘{name}’"),
            SerialValueError::UnknownField { name, expected } => {
                write!(f, "Unknown key ‘{name}’")?;
                if expected.is_empty() {
                    write!(f, " (this map has no keys)")
                } else {
                    write!(f, " (expected ")?;
                    write_name_list(f, expected)?;
                    write!(f, ")")
                }
            }
            SerialValueError::DuplicateField { name } => write!(f, "Duplicate key ‘{name}’"),
            SerialValueError::UnknownVariant { name, expected } => {
                write!(f, "Unknown variant ‘{name}’")?;
                if expected.is_empty() {
                    write!(f, " (this enum has no variants)")
                } else {
                    write!(f, " (expected ")?;
                    write_name_list(f, expected)?;
                    write!(f, ")")
                }
            }
            SerialValueError::NestingTooDeep { limit } => {
                write!(f, "Value nested too deeply (more than {limit} levels of lists and maps)")
            }
            SerialValueError::Custom(message) => f.write_str(message),
        }
    }
}

/// Writes a non-empty list of names: `‘a’` for a single name, `one of ‘a’, ‘b’` for
/// several names.
fn write_name_list(f: &mut fmt::Formatter<'_>, names: &[&str]) -> fmt::Result {
    match names {
        [only] => write!(f, "‘{only}’"),
        _ => {
            write!(f, "one of ")?;
            for (i, name) in names.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "‘{name}’")?;
            }
            Ok(())
        }
    }
}

impl core::error::Error for SerialValueError {}
