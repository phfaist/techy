//! The serialized shape of a [`ParseResult`](crate::engine::ParseResult):
//! [`WireParseResult`] — the tree's position in the trees table, the diagnostics
//! collection ([`WireDiagnostics`]: the retained diagnostics' positions in the
//! diagnostics table plus the collection's retention cap and counts), and the session
//! extension in the language's own form. Written and read by the crate's parse-result
//! driver ([`ParseResultSerdeDriver`](crate::serialize::ParseResultSerdeDriver)).

use alloc::vec::Vec;

use crate::serialize::drivers::{DiagnosticIndex, TreeIndex};
use crate::serialize::value::SerialValue;

use super::{FromSerialValue, ToSerialValue};

/// One parse result. Wire names not yet frozen (see the
/// `techy::serialize` module documentation, "Stability of the serialized form").
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireParseResult {
    /// The parsed tree (an entry of the trees table with the unit annotation).
    #[serial(name = "tree")]
    pub(crate) tree: TreeIndex,
    /// The diagnostics recorded during the parse.
    #[serial(name = "diagnostics")]
    pub(crate) diagnostics: WireDiagnostics,
    /// The parse's final session extension, in the language's own form
    /// (`SessionExt`'s value conversion).
    #[serial(name = "session_ext")]
    pub(crate) session_ext: SerialValue,
}

/// A diagnostics collection ([`Diagnostics`](crate::error::Diagnostics)): the retained
/// diagnostics as positions in the diagnostics table, in recording order, and the
/// collection's retention cap and counts.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireDiagnostics {
    /// The retained diagnostics, in recording order.
    #[serial(name = "items")]
    pub(crate) items: Vec<DiagnosticIndex>,
    /// The retention cap (`Diagnostics::limit`).
    #[serial(name = "limit")]
    pub(crate) limit: usize,
    /// The number of diagnostics pushed beyond the cap (`Diagnostics::suppressed`).
    #[serial(name = "suppressed")]
    pub(crate) suppressed: usize,
    /// The number of error-severity pushes, retained and suppressed.
    #[serial(name = "error_count")]
    pub(crate) error_count: usize,
}
