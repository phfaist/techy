//! The serialized shape of a [`Source`](crate::source::Source): [`WireSource`] and its
//! parts. Written and read by the crate's source driver
//! ([`SourceSerdeDriver`](crate::serialize::SourceSerdeDriver)); the origin value is
//! carried as a [`SerialValue`] produced by the language's own conversion.

use alloc::string::String;

use crate::serialize::drivers::SourceIndex;
use crate::serialize::value::SerialValue;

use super::{FromSerialValue, SerialBytes, ToSerialValue};

/// One source. Provisional wire names (the vocabulary of the serialized form is
/// finalized before the schema is frozen).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireSource {
    /// The origin, in the language's own form (`SourceOrigin`'s value conversion).
    #[serial(name = "origin")]
    pub(crate) origin: SerialValue,
    /// How the source entered the parse.
    #[serial(name = "provenance")]
    pub(crate) provenance: WireProvenance,
    /// The line number of the first line (1 for 1-indexed line numbers).
    #[serial(name = "line_number_offset")]
    pub(crate) line_number_offset: usize,
    /// The column number of the first column (1 for 1-indexed column numbers).
    #[serial(name = "column_number_offset")]
    pub(crate) column_number_offset: usize,
    /// The text: embedded, or kept outside and referenced.
    #[serial(name = "text")]
    pub(crate) text: WireSourceText,
}

/// The text of a source: embedded in the entry, or referenced (kept outside the
/// serialized form and described by length and optional digest).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) enum WireSourceText {
    /// The text itself.
    #[serial(name = "embedded")]
    Embedded(String),
    /// The description of text kept outside.
    #[serial(name = "referenced")]
    Referenced(WireSourceReference),
}

/// The description of referenced source text.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireSourceReference {
    /// The text's length in bytes.
    #[serial(name = "length")]
    pub(crate) length: usize,
    /// The text's digest, when the writer recorded one.
    #[serial(name = "digest")]
    pub(crate) digest: Option<WireDigest>,
}

/// A digest: the name of the hash function and its output.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireDigest {
    /// The hash function's name (the writer's choice, e.g. `sha256`).
    #[serial(name = "algorithm")]
    pub(crate) algorithm: String,
    /// The hash function's output.
    #[serial(name = "bytes")]
    pub(crate) bytes: SerialBytes,
}

/// How a source entered the parse ([`SourceProvenance`](crate::source::SourceProvenance)).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) enum WireProvenance {
    /// A top-level source.
    #[serial(name = "primary")]
    Primary,
    /// Resolved from an external reference at `triggered_at`.
    #[serial(name = "resolved")]
    Resolved {
        /// The reference that was resolved.
        #[serial(name = "reference")]
        reference: String,
        /// Where the resolution was triggered.
        #[serial(name = "triggered_at")]
        triggered_at: WireSpan,
    },
    /// Synthesized during parsing at `triggered_at`.
    #[serial(name = "synthesized")]
    Synthesized {
        /// What produced the content.
        #[serial(name = "description")]
        description: String,
        /// Where the synthesis was triggered.
        #[serial(name = "triggered_at")]
        triggered_at: WireSpan,
    },
}

/// A span: a byte range within a source referred to by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireSpan {
    /// The source.
    #[serial(name = "source")]
    pub(crate) source: SourceIndex,
    /// The start byte offset (inclusive).
    #[serial(name = "start")]
    pub(crate) start: usize,
    /// The end byte offset (exclusive).
    #[serial(name = "end")]
    pub(crate) end: usize,
}
