//! The serialization error types: [`SerializeError`] (write side),
//! [`DeserializeError`] (read side), [`RegistrationError`] (setting up a session), and
//! [`SerialValueError`] (conversion of plain data to and from a
//! [`SerialValue`](crate::serialize::SerialValue)).

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use super::value::TableId;

/// The shared error value behind the `cause` field of [`SerializeError::Failed`] and
/// [`DeserializeError::Failed`]: an `Arc` (the shape of
/// [`HookFailed::cause`](crate::error::HookFailed::cause)) so that the error types stay
/// `Clone`.
type SharedCause = Arc<dyn core::error::Error + Send + Sync + 'static>;

/// Error of the write side: what a
/// [`serialize_object`](crate::serialize::SerializableObject::serialize_object) call,
/// a [`serialize_argument_spec`](crate::spec::CallableSpec::serialize_argument_spec)
/// call, an [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver), or the session
/// driving them ([`SerdeSession::intern`](crate::serialize::SerdeSession::intern),
/// [`SerializeContext::intern`](crate::serialize::SerializeContext::intern)) can
/// report. Every variant names what failed; a failure inside a nested call is wrapped
/// in [`InTable`](SerializeError::InTable) with the table it happened in.
///
/// Not `PartialEq`: [`Failed`](SerializeError::Failed) carries an arbitrary
/// underlying error.
#[derive(Clone, Debug)]
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
    /// An implementation — a
    /// [`serialize_object`](crate::serialize::SerializableObject::serialize_object)
    /// or a driver — could not produce the serialized form, for a reason of its own:
    /// `detail` says which, in words; `cause` optionally carries the underlying error
    /// (the shape of [`HookFailed`](crate::error::HookFailed); the `Arc` keeps the
    /// error `Clone`). Construct with [`failed`](SerializeError::failed) and
    /// [`with_cause`](SerializeError::with_cause).
    Failed {
        /// Human-readable description of the failure.
        detail: String,
        /// The underlying error, if the implementation has one to attach.
        cause: Option<SharedCause>,
    },
    /// A value could not be represented in the value model — an integer outside
    /// `i64`, a floating-point number, a map with non-string keys (the
    /// [`SerialValueError`] says which); the conversion of a serialized structure or
    /// of a payload through the serde bridge reported it.
    Value(SerialValueError),
    /// The table handle names no table of the session it was used with — it comes
    /// from another session, or the table at that ordinal is registered with a
    /// different driver type.
    UnknownTable {
        /// The id the handle carries.
        table: TableId,
    },
    /// The table has reached the maximum number of entries (`u32::MAX`), so no
    /// further object can be interned into it.
    TableFull {
        /// The table's name.
        table: &'static str,
    },
    /// The serialized reference graph would be cyclic: while an object of table
    /// `referrer` was being serialized, an object whose serialization is itself still
    /// in progress — of table `table` — was interned again (an object refers, directly
    /// or through others, back to itself). Serialized references must form no cycles:
    /// deserialization rebuilds objects from their references, which is impossible
    /// for a cycle.
    ReferenceCycle {
        /// The table of the object whose serialization was still in progress.
        table: &'static str,
        /// The table of the object whose serialization interned it again.
        referrer: &'static str,
    },
    /// The session's descent guard refused to go one level deeper (the objects refer
    /// to one another more deeply than the configured limit): the call is abandoned.
    /// Configure the limit with
    /// [`SerdeSession::with_descent_guard_init`](crate::serialize::SerdeSession::with_descent_guard_init).
    DescentLimitExceeded {
        /// Which limit was hit and how to configure it.
        detail: String,
    },
    /// The entry a driver produced for a table holding objects of one kind only
    /// carries an identifier other than the table's: a driver bug. `expected` is the
    /// table's identifier (the driver's
    /// [`homogeneous_identifier`](crate::serialize::ObjectSerdeDriver::homogeneous_identifier)),
    /// `found` the identifier of the entry.
    UnexpectedIdentifier {
        /// The table's name.
        table: &'static str,
        /// The identifier every entry of the table must carry.
        expected: &'static str,
        /// The identifier the entry carried.
        found: String,
    },
    /// The failure happened while serializing an object into table `table`: the
    /// location wrapper the session adds around a driver's failure. Only the innermost
    /// location is recorded — a `cause` is never itself an `InTable`.
    InTable {
        /// The table's name.
        table: &'static str,
        /// The failure.
        cause: Box<SerializeError>,
    },
}

impl SerializeError {
    /// The error the default
    /// [`serialize_object`](crate::serialize::SerializableObject::serialize_object)
    /// reports: the type does not participate in serialization.
    pub fn unsupported() -> SerializeError {
        SerializeError::Unsupported
    }

    /// An implementation's own failure ([`Failed`](SerializeError::Failed)) with the
    /// given description and no underlying error; attach one with
    /// [`with_cause`](SerializeError::with_cause).
    pub fn failed(detail: impl Into<String>) -> SerializeError {
        SerializeError::Failed { detail: detail.into(), cause: None }
    }

    /// Attach the underlying error to a [`Failed`](SerializeError::Failed) (any other
    /// variant is returned unchanged): the error is shared internally (`Arc`) so that
    /// the value stays `Clone`, and reachable through
    /// [`Error::source`](core::error::Error::source).
    pub fn with_cause(self, cause: impl core::error::Error + Send + Sync + 'static) -> SerializeError {
        match self {
            SerializeError::Failed { detail, .. } => {
                SerializeError::Failed { detail, cause: Some(Arc::new(cause)) }
            }
            other => other,
        }
    }

    /// Wrap a driver's failure with the table it happened in — unless it already
    /// carries a location (only the innermost is kept).
    pub(crate) fn in_table(self, table: &'static str) -> SerializeError {
        match self {
            located @ SerializeError::InTable { .. } => located,
            cause => SerializeError::InTable { table, cause: Box::new(cause) },
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
            SerializeError::Failed { detail, .. } => {
                write!(f, "serialization failed: {detail}")
            }
            SerializeError::Value(error) => {
                write!(f, "the value cannot be represented in the serialized form: {error}")
            }
            SerializeError::UnknownTable { table } => write!(
                f,
                "table #{} is not registered in this session (the handle comes from another \
                 session, or the table was registered with a different driver type)",
                table.ordinal()
            ),
            SerializeError::TableFull { table } => {
                write!(f, "table `{table}` is full (u32::MAX entries)")
            }
            SerializeError::ReferenceCycle { table, referrer } => write!(
                f,
                "serialized references would form a cycle: an object of table `{table}` \
                 whose serialization is in progress was interned again while serializing \
                 an object of table `{referrer}`"
            ),
            SerializeError::DescentLimitExceeded { detail } => {
                write!(f, "serialization descent limit exceeded: {detail}")
            }
            SerializeError::UnexpectedIdentifier { table, expected, found } => write!(
                f,
                "the driver of table `{table}` produced an entry with identifier `{found}`, \
                 but every entry of that table must carry `{expected}`"
            ),
            SerializeError::InTable { table, cause } => {
                write!(f, "while serializing an object into table `{table}`: {cause}")
            }
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
            SerializeError::InTable { cause, .. } => Some(&**cause),
            _ => None,
        }
    }
}

/// Error of the read side: what a
/// [`deserialize_object`](crate::serialize::DeserializableObject::deserialize_object)
/// call, a
/// [`deserialize_argument_spec`](crate::spec::CallableSpec::deserialize_argument_spec)
/// call, an [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver), an
/// [`IdentifierResolver`](crate::serialize::IdentifierResolver), or the session
/// driving them ([`SerdeSession::push_segment`](crate::serialize::SerdeSession::push_segment),
/// [`SerdeSession::object`](crate::serialize::SerdeSession::object),
/// [`DeserializeContext::object`](crate::serialize::DeserializeContext::object))
/// can report. Everything read is untrusted input: a malformed value, an index out of
/// range, a reference cycle, or an unknown identifier is an error naming the culprit,
/// never a panic. A failure inside a nested call is wrapped in
/// [`InEntry`](DeserializeError::InEntry) with the entry it happened in.
///
/// Not `PartialEq`: [`Failed`](DeserializeError::Failed) carries an arbitrary
/// underlying error.
#[derive(Clone, Debug)]
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
    /// An implementation — a
    /// [`deserialize_object`](crate::serialize::DeserializableObject::deserialize_object),
    /// a driver, or a resolver — could not rebuild the object, for a reason of its own
    /// (an object the reading environment lacks, a definition that could not be
    /// loaded, …): `detail` says which, in words; `cause` optionally carries the
    /// underlying error (the shape of [`HookFailed`](crate::error::HookFailed); the
    /// `Arc` keeps the error `Clone`). Construct with
    /// [`failed`](DeserializeError::failed) and [`with_cause`](DeserializeError::with_cause).
    Failed {
        /// Human-readable description of the failure.
        detail: String,
        /// The underlying error, if the implementation has one to attach.
        cause: Option<SharedCause>,
    },
    /// A serialized value has the wrong shape — a value of the wrong kind, a missing,
    /// unknown, or repeated key, an integer that does not fit, an unknown variant (the
    /// [`SerialValueError`] says which).
    Value(SerialValueError),
    /// The table handle names no table of the session it was used with — it comes
    /// from another session, or the table at that ordinal is registered with a
    /// different driver type.
    UnknownTable {
        /// The id the handle carries.
        table: TableId,
    },
    /// A reference points beyond the end of its table: position `index` of table
    /// `table`, which holds `len` entries.
    IndexOutOfRange {
        /// The table's name.
        table: &'static str,
        /// The position referred to.
        index: u32,
        /// The number of entries the table holds.
        len: u32,
    },
    /// A typed table position was read through the handle of a table other than its
    /// own: the position carries table id `found`, the handle names table `expected`.
    /// A position minted by another session — one whose registration order differs —
    /// has this effect: typed positions are scoped to the session that minted them
    /// (see [`SerialIndex`](crate::serialize::SerialIndex)).
    WrongTable {
        /// The name of the table the handle names.
        expected: &'static str,
        /// The table id the position carries.
        found: TableId,
    },
    /// The serialized reference graph is cyclic: while entry `referrer_index` of table
    /// `referrer_table` was being deserialized, entry `index` of table `table` — whose
    /// own deserialization is still in progress — was read again. Objects are rebuilt
    /// from their references, which is impossible for a cycle.
    ReferenceCycle {
        /// The table of the entry whose deserialization was still in progress.
        table: &'static str,
        /// The position of that entry.
        index: u32,
        /// The table of the entry whose deserialization read it again.
        referrer_table: &'static str,
        /// The position of that entry.
        referrer_index: u32,
    },
    /// The session's descent guard refused to go one level deeper (the entries refer
    /// to one another more deeply than the configured limit): the call is abandoned.
    /// Configure the limit with
    /// [`SerdeSession::with_descent_guard_init`](crate::serialize::SerdeSession::with_descent_guard_init).
    DescentLimitExceeded {
        /// Which limit was hit and how to configure it.
        detail: String,
    },
    /// No registered reader and no resolver of table `table` recognizes the
    /// identifier: the reading environment does not know how to rebuild objects of
    /// that kind (nothing was registered for it, or the resolvers of its namespace
    /// declined).
    UnknownIdentifier {
        /// The table's name.
        table: &'static str,
        /// The identifier no reader is registered for.
        identifier: String,
    },
    /// The segment's version is not the one this crate reads
    /// ([`Segment::VERSION`](crate::serialize::Segment::VERSION)).
    UnsupportedVersion {
        /// The version the segment declares.
        found: u32,
        /// The version this crate reads.
        expected: u32,
    },
    /// The segment lists a table by a name the reading session has not registered.
    UnknownTableName {
        /// The name in the segment.
        name: String,
    },
    /// The segment lists the same table twice, or two of its tables carry the same
    /// writer-side table id.
    DuplicateSegmentTable {
        /// The table's name.
        name: String,
    },
    /// The segment's entries for table `table` do not continue where the session's
    /// table ends: the segment says they start at position `found`, the table holds
    /// `expected` entries. Segments of a stream must be pushed in order, each exactly
    /// once, into a session that has absorbed every earlier one.
    SegmentOutOfOrder {
        /// The table's name.
        table: &'static str,
        /// The position the segment's entries would have to start at.
        expected: u32,
        /// The position the segment declares.
        found: u32,
    },
    /// The session has entries in table `table` that were interned since its last
    /// emission and not yet emitted with
    /// [`take_segment`](crate::serialize::SerdeSession::take_segment): a segment
    /// continues the stream the session has emitted so far, so pending entries must be
    /// emitted first.
    UnemittedEntries {
        /// The table's name.
        table: &'static str,
    },
    /// A reference in the segment names a writer-side table id that the segment's
    /// table directory does not list, so it cannot be translated to a table of the
    /// reading session.
    UnknownWriterTable {
        /// The writer-side table id.
        table: TableId,
    },
    /// Absorbing the segment would give table `table` more than `u32::MAX` entries.
    TableFull {
        /// The table's name.
        table: &'static str,
    },
    /// A check the session runs on its own bookkeeping failed: a bug in this crate,
    /// not in the input and not in an implementation — `detail` says what was found.
    /// Reported as an error rather than a panic; the operation that found it has
    /// been undone (a segment being absorbed is dropped) and the session stays
    /// usable.
    Internal {
        /// What the check found, in words, naming the entry.
        detail: String,
    },
    /// The failure happened while deserializing entry `index` of table `table`, whose
    /// identifier is `identifier` when it is known (a table holding one kind of object
    /// has a fixed identifier; an entry of any other table carries its own, unless the
    /// entry's shape itself was malformed): the location wrapper the session adds
    /// around a driver's failure. Only the innermost location is recorded — a `cause`
    /// is never itself an `InEntry`.
    InEntry {
        /// The table's name.
        table: &'static str,
        /// The entry's position in the table.
        index: u32,
        /// The entry's identifier, when known.
        identifier: Option<Cow<'static, str>>,
        /// The failure.
        cause: Box<DeserializeError>,
    },
}

impl DeserializeError {
    /// An implementation's own failure ([`Failed`](DeserializeError::Failed)) with the
    /// given description and no underlying error; attach one with
    /// [`with_cause`](DeserializeError::with_cause).
    pub fn failed(detail: impl Into<String>) -> DeserializeError {
        DeserializeError::Failed { detail: detail.into(), cause: None }
    }

    /// Attach the underlying error to a [`Failed`](DeserializeError::Failed) (any
    /// other variant is returned unchanged): the error is shared internally (`Arc`)
    /// so that the value stays `Clone`, and reachable through
    /// [`Error::source`](core::error::Error::source).
    pub fn with_cause(self, cause: impl core::error::Error + Send + Sync + 'static) -> DeserializeError {
        match self {
            DeserializeError::Failed { detail, .. } => {
                DeserializeError::Failed { detail, cause: Some(Arc::new(cause)) }
            }
            other => other,
        }
    }

    /// Wrap a driver's failure with the entry it happened in — unless it already
    /// carries a location (only the innermost is kept).
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
            DeserializeError::Failed { detail, .. } => {
                write!(f, "deserialization failed: {detail}")
            }
            DeserializeError::Value(error) => {
                write!(f, "malformed serialized value: {error}")
            }
            DeserializeError::UnknownTable { table } => write!(
                f,
                "table #{} is not registered in this session (the handle comes from another \
                 session, or the table was registered with a different driver type)",
                table.ordinal()
            ),
            DeserializeError::IndexOutOfRange { table, index, len } => write!(
                f,
                "reference to entry #{index} of table `{table}`, which holds {len} entries"
            ),
            DeserializeError::WrongTable { expected, found } => write!(
                f,
                "a position in table #{} was read through the handle of table `{expected}`",
                found.ordinal()
            ),
            DeserializeError::ReferenceCycle { table, index, referrer_table, referrer_index } => {
                write!(
                    f,
                    "serialized references form a cycle: entry #{index} of table `{table}`, \
                     whose deserialization is in progress, was read again while \
                     deserializing entry #{referrer_index} of table `{referrer_table}`"
                )
            }
            DeserializeError::DescentLimitExceeded { detail } => {
                write!(f, "deserialization descent limit exceeded: {detail}")
            }
            DeserializeError::UnknownIdentifier { table, identifier } => write!(
                f,
                "no reader is registered for identifier `{identifier}` in table `{table}` \
                 (nothing was registered for it, or the resolvers of its namespace declined)"
            ),
            DeserializeError::UnsupportedVersion { found, expected } => write!(
                f,
                "segment version {found} is not supported (this crate reads version {expected})"
            ),
            DeserializeError::UnknownTableName { name } => write!(
                f,
                "the segment lists table `{name}`, which is not registered in this session"
            ),
            DeserializeError::DuplicateSegmentTable { name } => write!(
                f,
                "the segment lists table `{name}` twice (or reuses its writer-side table id)"
            ),
            DeserializeError::SegmentOutOfOrder { table, expected, found } => write!(
                f,
                "the segment's entries for table `{table}` start at position {found}, but the \
                 table holds {expected} entries (segments must be pushed in stream order)"
            ),
            DeserializeError::UnemittedEntries { table } => write!(
                f,
                "table `{table}` has entries interned since the last emission; emit them with \
                 take_segment before absorbing a segment"
            ),
            DeserializeError::UnknownWriterTable { table } => write!(
                f,
                "a reference names writer-side table #{}, which the segment's table \
                 directory does not list",
                table.ordinal()
            ),
            DeserializeError::TableFull { table } => write!(
                f,
                "absorbing the segment would give table `{table}` more than u32::MAX entries"
            ),
            DeserializeError::Internal { detail } => {
                write!(f, "internal error of the serialization session (a bug in this crate): {detail}")
            }
            DeserializeError::InEntry { table, index, identifier, cause } => match identifier {
                Some(identifier) => write!(
                    f,
                    "while deserializing entry #{index} (`{identifier}`) of table `{table}`: \
                     {cause}"
                ),
                None => write!(f, "while deserializing entry #{index} of table `{table}`: {cause}"),
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
            DeserializeError::InEntry { cause, .. } => Some(&**cause),
            _ => None,
        }
    }
}

/// Error of setting up a [`SerdeSession`](crate::serialize::SerdeSession):
/// registering a table ([`register_table`](crate::serialize::SerdeSession::register_table))
/// or the readers and resolvers of a table
/// ([`TableHandle::register_type`](crate::serialize::TableHandle::register_type) and
/// its siblings). Every variant is a contract violation by the calling code, reported
/// as an error rather than a panic.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegistrationError {
    /// A table of that name is already registered in the session.
    DuplicateTableName {
        /// The name.
        name: &'static str,
    },
    /// The session already has `u32::MAX` tables.
    TooManyTables,
    /// The table handle names no table of the session it was used with — it comes
    /// from another session, or the table at that ordinal is registered with a
    /// different driver type.
    UnknownTable {
        /// The id the handle carries.
        table: TableId,
    },
    /// A reader is already registered for that identifier in the table (by an earlier
    /// registration, or by a resolver whose answer was kept).
    DuplicateIdentifier {
        /// The table's name.
        table: &'static str,
        /// The identifier.
        identifier: String,
    },
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistrationError::DuplicateTableName { name } => {
                write!(f, "a table named `{name}` is already registered in this session")
            }
            RegistrationError::TooManyTables => {
                write!(f, "the session already has u32::MAX tables")
            }
            RegistrationError::UnknownTable { table } => write!(
                f,
                "table #{} is not registered in this session (the handle comes from another \
                 session, or the table was registered with a different driver type)",
                table.ordinal()
            ),
            RegistrationError::DuplicateIdentifier { table, identifier } => write!(
                f,
                "a reader is already registered for identifier `{identifier}` in table `{table}`"
            ),
        }
    }
}

impl core::error::Error for RegistrationError {}

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
