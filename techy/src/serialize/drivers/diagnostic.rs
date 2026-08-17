//! The diagnostic driver: [`DiagnosticSerdeDriver`], the driver of the diagnostics
//! table, and [`DiagnosticIndex`], its position type; the [`SerializableObject`] /
//! [`DeserializableObject`] impls of [`Diagnostic`], which the driver delegates to;
//! [`DeserializedCondition`], the condition a diagnostic read back from serialized
//! data carries; the [`DiagnosticSerialization`] extension trait
//! (`serialize_diagnostic` / `diagnostic`); the embedding of [`DiagnosticValue`] into
//! [`SerialValue`] and the value conversions of [`Severity`].
//!
//! A diagnostic's entry carries its severity, the condition's identifier and
//! serialization projection ([`DiagnosticInfo::serializable_data`]), the message as
//! rendered when the diagnostic was written, its span, and its traceback frames (each
//! a rendered title and a span); the spans refer to their sources by position, so a
//! diagnostic shares its sources with the tree it belongs to. Reading rebuilds the
//! diagnostic with a [`DeserializedCondition`] as its condition: the concrete condition
//! type is not rebuilt (nothing registers condition types), so consumers of a
//! deserialized diagnostic match on [`identifier()`](Diagnostic::identifier) — the wire
//! identity — and read the projection, never downcast to the original type.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use crate::error::{Diagnostic, DiagnosticInfo, DiagnosticValue, Severity, TraceFrame};
use crate::state::Lang;

use super::super::engine::{DeserializeContext, ObjectSerdeDriver, SerdeSession, SerializeContext};
use super::super::error::{DeserializeError, SerializeError};
use super::super::object::{
    DeserializableObject, DeserializableValue, SerializableLang, SerializableObject,
    SerializableValue,
};
use super::super::value::{SerialEntry, SerialValue};
use super::super::wire::diagnostic::{WireDiagnostic, WireSeverity, WireTraceFrame};
use super::super::wire::{FromSerialValue, ToSerialValue};
use super::source::{deserialize_span, serialize_span};
use super::{DIAGNOSTICS_TABLE, DIAGNOSTIC_IDENTIFIER};

crate::serial_index! {
    /// A position in the diagnostics table — the `Index` type of
    /// [`DiagnosticSerdeDriver`]: the serialized reference to a [`Diagnostic`].
    /// Serializing a diagnostic returns a fresh position every time (a diagnostic is a
    /// value, written in full — see [`DiagnosticSerialization::serialize_diagnostic`]).
    pub struct DiagnosticIndex;
}

/// The driver of the diagnostics table (table name `diagnostics`, one kind of object
/// — identifier `core.diagnostic`): how a [`Diagnostic`] is serialized and rebuilt.
///
/// A diagnostic's entry carries its severity, its condition's identifier
/// ([`Diagnostic::identifier`]) and serialization projection
/// ([`DiagnosticInfo::serializable_data`], a [`DiagnosticValue`] embedded as a
/// [`SerialValue`]), its message as rendered when it was written
/// ([`Diagnostic::message`]), its span, and its traceback frames (rendered titles and
/// spans). Every span refers to its source by position in the sources table, so the
/// diagnostics of a parse share their sources with the tree of that parse when both
/// are written into one stream.
///
/// **Reading rebuilds the condition as a [`DeserializedCondition`].** The concrete
/// condition type that was written is not rebuilt — the diagnostics table registers no
/// condition types — so a diagnostic read back answers the written identifier through
/// [`Diagnostic::identifier`], the written projection through
/// [`serializable_data`](crate::error::DiagnosticData::serializable_data), and the
/// written message through [`Diagnostic::message`] (and renders as before), but
/// [`downcast_ref`](../error/trait.DiagnosticData.html#method.downcast_ref) to the original
/// condition type answers `None`: the identifier is the contract across a
/// serialization boundary, exactly as [`DiagnosticInfo::IDENTIFIER`] documents.
///
/// A diagnostic is a value: every call writes a new entry (unlike a source or a
/// state, which is written once and shared); the convenience methods are the
/// [`DiagnosticSerialization`] extension trait's `serialize_diagnostic` and
/// `diagnostic`. The driver delegates to `Diagnostic`'s own [`SerializableObject`] and
/// [`DeserializableObject`] impls. Registered by
/// [`SerdeSession::new`](crate::serialize::SerdeSession::new).
///
/// Everything read is untrusted input: an unknown severity string, a projection
/// holding a value kind a `DiagnosticValue` cannot hold (a byte string, a table
/// position — [`DeserializeError::UnrepresentableDiagnosticValue`]), a span out of its
/// source's bounds — each is an error, never a panic.
pub struct DiagnosticSerdeDriver<L: Lang> {
    lang: PhantomData<fn() -> L>,
}

impl<L: Lang> DiagnosticSerdeDriver<L> {
    /// The driver (it has no configuration).
    pub fn new() -> DiagnosticSerdeDriver<L> {
        DiagnosticSerdeDriver { lang: PhantomData }
    }
}

impl<L: Lang> Default for DiagnosticSerdeDriver<L> {
    fn default() -> Self {
        DiagnosticSerdeDriver::new()
    }
}

impl<L: Lang> fmt::Debug for DiagnosticSerdeDriver<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiagnosticSerdeDriver").finish()
    }
}

impl<L: SerializableLang> ObjectSerdeDriver<L> for DiagnosticSerdeDriver<L> {
    type Object = Diagnostic<L::SourceOrigin>;
    type Index = DiagnosticIndex;

    fn table_name(&self) -> &'static str {
        DIAGNOSTICS_TABLE
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        Some(DIAGNOSTIC_IDENTIFIER)
    }

    fn serialize_object(
        &self,
        diagnostic: &Arc<Diagnostic<L::SourceOrigin>>,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError> {
        diagnostic.serialize_object(cx)
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<Diagnostic<L::SourceOrigin>>, DeserializeError> {
        Diagnostic::<L::SourceOrigin>::deserialize_object(&entry.data, cx).map(Arc::new)
    }
}

// --- the object impls -------------------------------------------------------------------

/// A diagnostic is serialized as its severity, its condition's identifier and
/// serialization projection, its rendered message, its span, and its traceback frames
/// (see [`DiagnosticSerdeDriver`]); the spans intern their sources.
impl<L: Lang> SerializableObject<L> for Diagnostic<L::SourceOrigin> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let frames = self
            .frames()
            .iter()
            .map(|frame| {
                Ok(WireTraceFrame { title: String::from(frame.title()), span: serialize_span(frame.span(), cx)? })
            })
            .collect::<Result<Vec<_>, SerializeError>>()?;
        let wire = WireDiagnostic {
            severity: WireSeverity::from(self.severity()),
            identifier: String::from(self.identifier()),
            message: self.message(),
            data: SerialValue::from(self.data().serializable_data()),
            span: serialize_span(self.span(), cx)?,
            frames,
        };
        Ok(SerialEntry { identifier: DIAGNOSTIC_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

/// A diagnostic is read back with a [`DeserializedCondition`] holding the written
/// identifier, projection, and message as its condition (see
/// [`DiagnosticSerdeDriver`]); every span is validated against its source.
impl<L: SerializableLang> DeserializableObject<L> for Diagnostic<L::SourceOrigin> {
    type Output = Diagnostic<L::SourceOrigin>;

    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Diagnostic<L::SourceOrigin>, DeserializeError> {
        let wire = WireDiagnostic::from_serial_value(value)?;
        let data = diagnostic_value_from_serial(&wire.data)?;
        let span = deserialize_span(wire.span, cx)?;
        let frames = wire
            .frames
            .into_iter()
            .map(|frame| Ok(TraceFrame::new(frame.title, deserialize_span(frame.span, cx)?)))
            .collect::<Result<Vec<_>, DeserializeError>>()?;
        let condition = DeserializedCondition::new(wire.identifier, data, wire.message);
        Ok(Diagnostic::from_parts(Severity::from(wire.severity), Box::new(condition), span, frames))
    }
}

// --- the deserialized condition -------------------------------------------------------

/// The condition a diagnostic read back from serialized data carries: the written
/// condition's identifier, serialization projection, and rendered message, held as
/// values. Implements [`DiagnosticInfo`] as an adapter type — the exceptional per-instance
/// [`identifier()`](DiagnosticInfo::identifier) override that trait documents: the
/// instance answers the identifier it holds (the written condition's), while the type's
/// own [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER), `core.serialization.deserialized-condition`,
/// names the adapter itself for type-keyed uses.
///
/// So a diagnostic read back ([`DiagnosticSerialization::diagnostic`], or inside a
/// deserialized [`ParseResult`](crate::engine::ParseResult)) answers the same
/// [`identifier()`](Diagnostic::identifier), the same
/// [`serializable_data()`](crate::error::DiagnosticData::serializable_data), and the
/// same [`message()`](Diagnostic::message) as the diagnostic that was written, and
/// [`Diagnostics::with_identifier`](crate::error::Diagnostics::with_identifier) finds
/// it — but its condition is a `DeserializedCondition`, not the concrete condition
/// type that was written: [`downcast_ref`](../error/trait.DiagnosticData.html#method.downcast_ref)
/// to that type answers `None`, and
/// [`Diagnostics::conditions`](crate::error::Diagnostics::conditions) over it yields
/// nothing. Consumers on the far side of a serialization boundary work from the
/// identifier and the projection (the contract of [`DiagnosticInfo::IDENTIFIER`]).
///
/// The message is the written diagnostic's rendering at the time it was written; it
/// is not re-rendered (the condition type's `Display` wording is not part of the
/// stable contract, the identifier and the projection's keys are).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeserializedCondition {
    identifier: String,
    data: DiagnosticValue,
    message: String,
}

impl DeserializedCondition {
    /// A condition answering `identifier`, projecting `data`, and rendering `message`
    /// — what the diagnostic driver builds from a diagnostic's entry.
    pub fn new(identifier: impl Into<String>, data: DiagnosticValue, message: impl Into<String>) -> DeserializedCondition {
        DeserializedCondition { identifier: identifier.into(), data, message: message.into() }
    }

    /// The written condition's serialization projection (what
    /// [`serializable_data`](DiagnosticInfo::serializable_data) answers, borrowed).
    pub fn data(&self) -> &DiagnosticValue {
        &self.data
    }
}

impl fmt::Display for DeserializedCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl DiagnosticInfo for DeserializedCondition {
    /// The adapter type's own identity; a stored instance answers the written
    /// condition's identifier instead (see [`identifier`](DiagnosticInfo::identifier)).
    const IDENTIFIER: &'static str = "core.serialization.deserialized-condition";

    /// The written condition's identifier.
    fn identifier(&self) -> &str {
        &self.identifier
    }

    /// The written condition's projection.
    fn serializable_data(&self) -> DiagnosticValue {
        self.data.clone()
    }
}

// --- the DiagnosticValue embedding ----------------------------------------------------

/// A [`DiagnosticValue`] is a strict subset of [`SerialValue`] (no byte strings, no
/// table positions): the embedding maps each variant to the same-named one.
impl From<DiagnosticValue> for SerialValue {
    fn from(value: DiagnosticValue) -> SerialValue {
        match value {
            DiagnosticValue::Null => SerialValue::Null,
            DiagnosticValue::Bool(b) => SerialValue::Bool(b),
            DiagnosticValue::Int(i) => SerialValue::Int(i),
            DiagnosticValue::Str(s) => SerialValue::Str(s),
            DiagnosticValue::List(items) => SerialValue::List(items.into_iter().map(SerialValue::from).collect()),
            DiagnosticValue::Map(entries) => {
                SerialValue::Map(entries.into_iter().map(|(key, value)| (key, SerialValue::from(value))).collect())
            }
        }
    }
}

/// The embedding of a borrowed [`DiagnosticValue`] (a clone of its strings).
impl From<&DiagnosticValue> for SerialValue {
    fn from(value: &DiagnosticValue) -> SerialValue {
        SerialValue::from(value.clone())
    }
}

/// The reverse of the embedding, for the diagnostic reader: a `SerialValue` holding
/// only the kinds a [`DiagnosticValue`] holds. A byte string or a table position
/// anywhere inside is [`DeserializeError::UnrepresentableDiagnosticValue`], naming the
/// path to the offending value (map keys and list positions from the root, as
/// `key.key[3]`; empty for the root itself).
fn diagnostic_value_from_serial(value: &SerialValue) -> Result<DiagnosticValue, DeserializeError> {
    let mut path = String::new();
    diagnostic_value_at(value, &mut path)
}

/// [`diagnostic_value_from_serial`] at `path` (extended and restored around each
/// descent).
fn diagnostic_value_at(value: &SerialValue, path: &mut String) -> Result<DiagnosticValue, DeserializeError> {
    Ok(match value {
        SerialValue::Null => DiagnosticValue::Null,
        SerialValue::Bool(b) => DiagnosticValue::Bool(*b),
        SerialValue::Int(i) => DiagnosticValue::Int(*i),
        SerialValue::Str(s) => DiagnosticValue::Str(s.clone()),
        SerialValue::List(items) => {
            let mut converted = Vec::with_capacity(items.len());
            for (position, item) in items.iter().enumerate() {
                let len = path.len();
                use core::fmt::Write as _;
                // Writing into a `String` cannot fail.
                let _ = write!(path, "[{position}]");
                converted.push(diagnostic_value_at(item, path)?);
                path.truncate(len);
            }
            DiagnosticValue::List(converted)
        }
        SerialValue::Map(entries) => {
            let mut converted = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
                converted.push((key.clone(), diagnostic_value_at(value, path)?));
                path.truncate(len);
            }
            DiagnosticValue::Map(converted)
        }
        SerialValue::Bytes(_) | SerialValue::Index { .. } => {
            return Err(DeserializeError::UnrepresentableDiagnosticValue { kind: value.kind_name(), path: path.clone() })
        }
    })
}

/// Available with the `serde` cargo feature: a [`DiagnosticValue`] renders exactly as
/// the [`SerialValue`] it embeds into (see [`SerialValue`]'s rendering) — one rendering
/// path for both value models.
#[cfg(feature = "serde")]
impl serde::Serialize for DiagnosticValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SerialValue::from(self).serialize(serializer)
    }
}

/// Available with the `serde` cargo feature: reads a [`SerialValue`]'s rendering (see
/// [`SerialValue`]'s `Deserialize`) and accepts only the kinds a `DiagnosticValue`
/// holds — a byte string or a table position anywhere inside is an error (the message
/// of [`DeserializeError::UnrepresentableDiagnosticValue`], naming the path to it).
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for DiagnosticValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = SerialValue::deserialize(deserializer)?;
        diagnostic_value_from_serial(&value).map_err(serde::de::Error::custom)
    }
}

// --- the severity's value conversions ---------------------------------------------------

/// A severity is the string `"note"`, `"warning"`, or `"error"`.
impl<L: Lang> SerializableValue<L> for Severity {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(WireSeverity::from(*self).to_serial_value()?)
    }
}

/// A severity is read from its string; any other string is an error.
impl<L: Lang> DeserializableValue<L> for Severity {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        Ok(Severity::from(WireSeverity::from_serial_value(value)?))
    }
}

// --- the sugar -------------------------------------------------------------------------

/// Serializing a diagnostic into and reading one back from a session's diagnostics
/// table by kind — `serialize_diagnostic` and `diagnostic` — on a [`SerdeSession`]:
/// the convenience methods over the general
/// [`SerdeSession::intern`](crate::serialize::SerdeSession::intern) /
/// [`SerdeSession::object`](crate::serialize::SerdeSession::object) with the
/// diagnostics table handle.
///
/// An extension trait: bring it into scope with `use techy::serialize::DiagnosticSerialization;`.
pub trait DiagnosticSerialization<L: SerializableLang> {
    /// Serialize `diagnostic` into the diagnostics table, returning its position.
    /// Every call is a new entry: a diagnostic is a value, written in full, so two
    /// calls with equal diagnostics produce two entries (unlike an interned source or
    /// state, which is written once and shared); its spans' sources are interned into
    /// the sources table.
    ///
    /// # Errors
    ///
    /// The session has no diagnostics table ([`SerializeError::UnknownTableName`]);
    /// the errors of [`SerdeSession::intern`](crate::serialize::SerdeSession::intern).
    fn serialize_diagnostic(
        &mut self,
        diagnostic: &Diagnostic<L::SourceOrigin>,
    ) -> Result<DiagnosticIndex, SerializeError>;

    /// The diagnostic at `position` of the diagnostics table (a clone of the stored
    /// one; its condition is a [`DeserializedCondition`]).
    ///
    /// # Errors
    ///
    /// The session has no diagnostics table ([`DeserializeError::UnknownTableName`]);
    /// the errors of [`SerdeSession::object`](crate::serialize::SerdeSession::object).
    fn diagnostic(&mut self, position: DiagnosticIndex) -> Result<Diagnostic<L::SourceOrigin>, DeserializeError>;
}

impl<L: SerializableLang> DiagnosticSerialization<L> for SerdeSession<L> {
    fn serialize_diagnostic(
        &mut self,
        diagnostic: &Diagnostic<L::SourceOrigin>,
    ) -> Result<DiagnosticIndex, SerializeError> {
        let handle = self
            .table_handle::<DiagnosticSerdeDriver<L>>(DIAGNOSTICS_TABLE)
            .ok_or_else(|| SerializeError::UnknownTableName { name: DIAGNOSTICS_TABLE.to_string() })?;
        self.intern(handle, &Arc::new(diagnostic.clone()))
    }

    fn diagnostic(&mut self, position: DiagnosticIndex) -> Result<Diagnostic<L::SourceOrigin>, DeserializeError> {
        let handle = self
            .table_handle::<DiagnosticSerdeDriver<L>>(DIAGNOSTICS_TABLE)
            .ok_or_else(|| DeserializeError::UnknownTableName { name: DIAGNOSTICS_TABLE.to_string() })?;
        self.object(handle, position).map(|diagnostic| (*diagnostic).clone())
    }
}
