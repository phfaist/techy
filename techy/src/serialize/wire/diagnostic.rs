//! The serialized shape of a [`Diagnostic`](crate::error::Diagnostic): [`WireDiagnostic`]
//! and its parts — the severity ([`WireSeverity`]), the condition's identifier, the
//! rendered message, the serialization projection (a [`SerialValue`] holding the
//! embedded [`DiagnosticValue`](crate::error::DiagnosticValue)), the span,
//! and the traceback frames ([`WireTraceFrame`]). Written and read by the crate's
//! diagnostic driver ([`DiagnosticSerdeDriver`](crate::serialize::DiagnosticSerdeDriver)).

use alloc::string::String;
use alloc::vec::Vec;

use crate::error::Severity;
use crate::serialize::value::SerialValue;

use super::source::WireSpan;
use super::{FromSerialValue, ToSerialValue};

/// One diagnostic. Provisional wire names (the vocabulary of the serialized form is
/// finalized before the schema is frozen).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireDiagnostic {
    /// The severity.
    #[serial(name = "severity")]
    pub(crate) severity: WireSeverity,
    /// The condition's wire identity (`Diagnostic::identifier`).
    #[serial(name = "identifier")]
    pub(crate) identifier: String,
    /// The human-readable message, as rendered when the diagnostic was written
    /// (`Diagnostic::message`).
    #[serial(name = "message")]
    pub(crate) message: String,
    /// The condition's serialization projection (`DiagnosticInfo::serializable_data`),
    /// embedded as a `SerialValue`; the reader accepts only the kinds a
    /// `DiagnosticValue` holds.
    #[serial(name = "data")]
    pub(crate) data: SerialValue,
    /// Where in the source the condition occurred.
    #[serial(name = "span")]
    pub(crate) span: WireSpan,
    /// The traceback snapshot, innermost first.
    #[serial(name = "frames")]
    pub(crate) frames: Vec<WireTraceFrame>,
}

/// The severity of a diagnostic ([`Severity`]): `note`, `warning`, or `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) enum WireSeverity {
    /// [`Severity::Note`].
    #[serial(name = "note")]
    Note,
    /// [`Severity::Warning`].
    #[serial(name = "warning")]
    Warning,
    /// [`Severity::Error`].
    #[serial(name = "error")]
    Error,
}

impl From<Severity> for WireSeverity {
    fn from(severity: Severity) -> WireSeverity {
        match severity {
            Severity::Note => WireSeverity::Note,
            Severity::Warning => WireSeverity::Warning,
            Severity::Error => WireSeverity::Error,
        }
    }
}

impl From<WireSeverity> for Severity {
    fn from(severity: WireSeverity) -> Severity {
        match severity {
            WireSeverity::Note => Severity::Note,
            WireSeverity::Warning => Severity::Warning,
            WireSeverity::Error => Severity::Error,
        }
    }
}

/// One traceback frame ([`TraceFrame`](crate::error::TraceFrame)): its rendered title
/// and the source location the parse descended at.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireTraceFrame {
    /// The rendered frame title.
    #[serial(name = "title")]
    pub(crate) title: String,
    /// Where in the source the parse descended into the frame.
    #[serial(name = "span")]
    pub(crate) span: WireSpan,
}
