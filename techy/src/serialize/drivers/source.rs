//! The source driver: [`SourceSerdeDriver`], the driver of the sources table, with
//! the caller-supplied embed-or-reference policy ([`SourceTextPolicy`],
//! [`SourceTextForm`]) and the caller-supplied text supply and digest verification
//! for referenced sources ([`SourceTextSupplier`], [`ReferencedSource`],
//! [`SourceDigest`]); [`SourceIndex`], the sources table's position type; and the
//! value conversion of spans, which refer to their source by position.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use crate::source::{Source, SourceOrigin, SourceProvenance, SourceSpan};
use crate::state::Lang;

use super::super::engine::{DeserializeContext, ObjectSerdeDriver, SerializeContext};
use super::super::error::{DeserializeError, SerializeError};
use super::super::object::{DeserializableValue, SerializableLang, SerializableValue};
use super::super::value::{SerialEntry, SerialValue};
use super::super::wire::source::{
    WireDigest, WireProvenance, WireSource, WireSourceReference, WireSourceText, WireSpan,
};
use super::super::wire::{FromSerialValue, SerialBytes, ToSerialValue};
use super::standard::StandardTableReading;
use super::{StandardTableInterning, SOURCES_TABLE, SOURCE_IDENTIFIER};

crate::serial_index! {
    /// A position in the sources table — the `Index` type of [`SourceSerdeDriver`]:
    /// the serialized reference to a [`Source`].
    pub struct SourceIndex;
}

/// A digest of a source's text: a fixed-size fingerprint of the text computed by a
/// hash function, named by `algorithm` (the writer's choice — `"sha256"`, say) and
/// held as `bytes` (the function's output). The crate neither chooses nor implements
/// any hash function: a [`SourceTextPolicy`] computes digests when it decides to
/// reference a source, and a [`SourceTextSupplier`] verifies them when the source is
/// read back; the crate stores the pair and asks the caller's supplier to verify it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDigest {
    /// The hash function's name.
    pub algorithm: String,
    /// The hash function's output for the text.
    pub bytes: Vec<u8>,
}

impl SourceDigest {
    /// A digest computed by the hash function named `algorithm`, with output `bytes`.
    pub fn new(algorithm: impl Into<String>, bytes: impl Into<Vec<u8>>) -> SourceDigest {
        SourceDigest { algorithm: algorithm.into(), bytes: bytes.into() }
    }
}

/// How a source's text is written: embedded in the source's entry, or kept outside
/// the serialized form and *referenced* — the entry then records the text's length
/// and, optionally, a [`SourceDigest`], and the reading side obtains the text from
/// its [`SourceTextSupplier`]. The [`SourceTextPolicy`] of the writing session's
/// source driver decides per source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceTextForm {
    /// The text is written into the entry: the serialized form is self-contained.
    Embedded,
    /// The text is not written; the entry records its length and `digest` (when
    /// given), and the reading side must supply the text.
    Referenced {
        /// A digest of the text, recorded so that the reading side can verify the
        /// text it supplies is the one that was serialized; `None` records the length
        /// only.
        digest: Option<SourceDigest>,
    },
}

/// The writing side's per-source choice between embedding and referencing the text
/// (see [`SourceTextForm`]): configured on the source driver with
/// [`SourceSerdeDriver::with_text_policy`]. Without a policy, every source is
/// embedded.
pub trait SourceTextPolicy<O: SourceOrigin>: Send + Sync {
    /// Decide how `source`'s text is written — computing the digest here, when the
    /// text is referenced with one.
    ///
    /// # Errors
    ///
    /// The policy cannot decide or cannot compute the digest; the source is not
    /// serialized and the error is reported through the interning call.
    fn text_form(&self, source: &Source<O>) -> Result<SourceTextForm, SerializeError>;
}

/// The reading side's supply of the text of a *referenced* source (one whose entry
/// carries no text — see [`SourceTextForm::Referenced`]) and its verification of
/// recorded digests: configured on the source driver with
/// [`SourceSerdeDriver::with_text_supplier`]. Without a supplier, a referenced
/// source is an error ([`DeserializeError::NoSourceTextSupplier`]).
///
/// The driver calls [`source_text`](Self::source_text), checks the text's length
/// against the recorded one ([`DeserializeError::SourceLengthMismatch`]), and, when
/// the entry records a digest, calls [`digest_matches`](Self::digest_matches)
/// ([`DeserializeError::SourceDigestMismatch`] on `false`) — so a text that has
/// changed since it was serialized is a clean error, never a source with wrong
/// offsets.
pub trait SourceTextSupplier<O: SourceOrigin>: Send + Sync {
    /// The text of `source`, as it was when the source was serialized: `source`
    /// carries everything the entry recorded about it (its origin, its provenance,
    /// the text's length, the digest).
    ///
    /// # Errors
    ///
    /// The text cannot be obtained (typically [`DeserializeError::failed`] with the
    /// reason).
    fn source_text(&self, source: &ReferencedSource<O>) -> Result<String, DeserializeError>;

    /// Whether `text` matches `digest`: `Ok(true)` when the digest of `text` under
    /// the algorithm the digest names equals the digest's bytes, `Ok(false)` when it
    /// does not (the driver then reports [`DeserializeError::SourceDigestMismatch`]).
    ///
    /// # Errors
    ///
    /// The digest cannot be checked — an algorithm this supplier does not know, say
    /// (a supplier is free to refuse any algorithm; the source is then not
    /// deserialized).
    fn digest_matches(&self, digest: &SourceDigest, text: &str) -> Result<bool, DeserializeError>;
}

/// What a source's entry records about a referenced source (one whose text is kept
/// outside the serialized form): the argument of
/// [`SourceTextSupplier::source_text`].
#[derive(Clone, Debug)]
pub struct ReferencedSource<O: SourceOrigin> {
    origin: O,
    provenance: SourceProvenance<O>,
    length: usize,
    digest: Option<SourceDigest>,
    line_number_offset: usize,
    column_number_offset: usize,
}

impl<O: SourceOrigin> ReferencedSource<O> {
    /// The source's origin ([`Source::origin`]) — for the default origin type, the
    /// URL or path the text was obtained from, if it was recorded.
    pub fn origin(&self) -> &O {
        &self.origin
    }

    /// How the source entered the parse ([`Source::provenance`]).
    pub fn provenance(&self) -> &SourceProvenance<O> {
        &self.provenance
    }

    /// The text's length in bytes, as recorded.
    pub fn length(&self) -> usize {
        self.length
    }

    /// The text's digest, if the writer recorded one.
    pub fn digest(&self) -> Option<&SourceDigest> {
        self.digest.as_ref()
    }

    /// The source's line number offset ([`Source::line_number_offset`]).
    pub fn line_number_offset(&self) -> usize {
        self.line_number_offset
    }

    /// The source's column number offset ([`Source::column_number_offset`]).
    pub fn column_number_offset(&self) -> usize {
        self.column_number_offset
    }
}

/// The driver of the sources table (table name `sources`, one kind of object —
/// identifier `core.source`): how a [`Source`] is serialized and rebuilt.
///
/// A source's entry carries its origin (through the language's `SourceOrigin`
/// value conversion), its provenance — a resolved or synthesized source refers to
/// the source its triggering location lies in by position, so provenance chains are
/// serialized as references between entries of this table — its line and column
/// number offsets, and its text, either **embedded** or **referenced**:
///
/// - *Embedded*: the text is written into the entry, and the serialized form is
///   self-contained. The default for every source.
/// - *Referenced*: the text is kept outside; the entry records the text's length
///   and, optionally, a [`SourceDigest`] — a fixed-size fingerprint of the text
///   computed by a hash function of the writer's choice, stored as the function's
///   name and output. On reading, the driver asks its [`SourceTextSupplier`] for the
///   text, checks the length, and asks the supplier to verify the digest; a text
///   that changed since it was serialized is a clean error, never a source with
///   wrong offsets. The crate neither chooses nor implements a hash function.
///
/// The choice is the [`SourceTextPolicy`]'s ([`with_text_policy`](Self::with_text_policy)),
/// per source; the supply is the [`SourceTextSupplier`]'s
/// ([`with_text_supplier`](Self::with_text_supplier)). Without a policy every source
/// is embedded; without a supplier a referenced source is an error.
///
/// Sharing survives: every span of a stream that points into one `Arc<Source>`
/// refers to one entry, and reading the entries back yields one `Arc<Source>` per
/// entry — [`SourceSpan::same_source`] holds after a round trip exactly where it
/// held before.
///
/// Registered by [`SerdeSession::new`](crate::serialize::SerdeSession::new); a
/// configured driver is registered through
/// [`SerdeSession::with_source_driver`](crate::serialize::SerdeSession::with_source_driver).
pub struct SourceSerdeDriver<L: Lang> {
    text_policy: Option<Arc<dyn SourceTextPolicy<L::SourceOrigin>>>,
    text_supplier: Option<Arc<dyn SourceTextSupplier<L::SourceOrigin>>>,
    lang: PhantomData<fn() -> L>,
}

impl<L: Lang> SourceSerdeDriver<L> {
    /// The driver with the defaults: every source embedded on writing; referenced
    /// sources an error on reading.
    pub fn new() -> SourceSerdeDriver<L> {
        SourceSerdeDriver { text_policy: None, text_supplier: None, lang: PhantomData }
    }

    /// Use `policy` to decide, per source, whether the text is embedded or
    /// referenced (see [`SourceTextForm`]).
    pub fn with_text_policy(mut self, policy: Arc<dyn SourceTextPolicy<L::SourceOrigin>>) -> Self {
        self.text_policy = Some(policy);
        self
    }

    /// Use `supplier` to obtain the text of referenced sources and to verify their
    /// digests.
    pub fn with_text_supplier(mut self, supplier: Arc<dyn SourceTextSupplier<L::SourceOrigin>>) -> Self {
        self.text_supplier = Some(supplier);
        self
    }
}

impl<L: Lang> Default for SourceSerdeDriver<L> {
    fn default() -> Self {
        SourceSerdeDriver::new()
    }
}

impl<L: Lang> fmt::Debug for SourceSerdeDriver<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceSerdeDriver")
            .field("text_policy", &self.text_policy.as_ref().map(|_| "set"))
            .field("text_supplier", &self.text_supplier.as_ref().map(|_| "set"))
            .finish()
    }
}

/// The origin's label, owned, for error messages.
fn origin_label<O: SourceOrigin>(origin: &O) -> Option<String> {
    origin.label().map(|label| label.into_owned())
}

impl<L: SerializableLang> ObjectSerdeDriver<L> for SourceSerdeDriver<L> {
    type Object = Source<L::SourceOrigin>;
    type Index = SourceIndex;

    fn table_name(&self) -> &'static str {
        SOURCES_TABLE
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        Some(SOURCE_IDENTIFIER)
    }

    fn serialize_object(
        &self,
        source: &Arc<Source<L::SourceOrigin>>,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError> {
        let form = match &self.text_policy {
            Some(policy) => policy.text_form(source)?,
            None => SourceTextForm::Embedded,
        };
        let origin = source.origin().serialize_value(cx)?;
        let provenance = match source.provenance() {
            SourceProvenance::Primary => WireProvenance::Primary,
            SourceProvenance::Resolved { reference, triggered_at } => WireProvenance::Resolved {
                reference: reference.clone(),
                triggered_at: serialize_span(triggered_at, cx)?,
            },
            SourceProvenance::Synthesized { description, triggered_at } => WireProvenance::Synthesized {
                description: description.clone(),
                triggered_at: serialize_span(triggered_at, cx)?,
            },
        };
        let text = match form {
            SourceTextForm::Embedded => WireSourceText::Embedded(String::from(source.content())),
            SourceTextForm::Referenced { digest } => WireSourceText::Referenced(WireSourceReference {
                length: source.content().len(),
                digest: digest.map(|digest| WireDigest {
                    algorithm: digest.algorithm,
                    bytes: SerialBytes(digest.bytes),
                }),
            }),
        };
        let wire = WireSource {
            origin,
            provenance,
            line_number_offset: source.line_number_offset(),
            column_number_offset: source.column_number_offset(),
            text,
        };
        Ok(SerialEntry { identifier: SOURCE_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<Source<L::SourceOrigin>>, DeserializeError> {
        let wire = WireSource::from_serial_value(&entry.data)?;
        let origin = <L::SourceOrigin as DeserializableValue<L>>::deserialize_value(&wire.origin, cx)?;
        let provenance = match wire.provenance {
            WireProvenance::Primary => SourceProvenance::Primary,
            WireProvenance::Resolved { reference, triggered_at } => SourceProvenance::Resolved {
                reference,
                triggered_at: deserialize_span(triggered_at, cx)?,
            },
            WireProvenance::Synthesized { description, triggered_at } => SourceProvenance::Synthesized {
                description,
                triggered_at: deserialize_span(triggered_at, cx)?,
            },
        };
        let text = match wire.text {
            WireSourceText::Embedded(text) => text,
            WireSourceText::Referenced(reference) => {
                let label = origin_label(&origin);
                let Some(supplier) = &self.text_supplier else {
                    return Err(DeserializeError::NoSourceTextSupplier { origin: label });
                };
                let digest = reference
                    .digest
                    .map(|digest| SourceDigest { algorithm: digest.algorithm, bytes: digest.bytes.0 });
                let referenced = ReferencedSource {
                    origin: origin.clone(),
                    provenance: provenance.clone(),
                    length: reference.length,
                    digest: digest.clone(),
                    line_number_offset: wire.line_number_offset,
                    column_number_offset: wire.column_number_offset,
                };
                let text = supplier.source_text(&referenced)?;
                if text.len() != reference.length {
                    return Err(DeserializeError::SourceLengthMismatch {
                        origin: label,
                        expected: reference.length,
                        found: text.len(),
                    });
                }
                if let Some(digest) = &digest {
                    if !supplier.digest_matches(digest, &text)? {
                        return Err(DeserializeError::SourceDigestMismatch {
                            origin: label,
                            algorithm: digest.algorithm.clone(),
                        });
                    }
                }
                text
            }
        };
        let source = Source::new(text)
            .with_origin(origin)
            .with_provenance(provenance)
            .with_line_column_number_offsets(wire.line_number_offset, wire.column_number_offset);
        Ok(Arc::new(source))
    }
}

/// The serialized form of `span`: its source interned into the sources table, and
/// its byte range.
pub(crate) fn serialize_span<L: SerializableLang>(
    span: &SourceSpan<L::SourceOrigin>,
    cx: &mut SerializeContext<'_, L>,
) -> Result<WireSpan, SerializeError> {
    let source = cx.intern_source(span.source())?;
    Ok(WireSpan { source, start: span.start(), end: span.end() })
}

/// The span `wire` describes, over the source read back from the sources table.
/// Validates the range against the source before constructing the span (the range
/// must lie within the text and on character boundaries).
pub(crate) fn deserialize_span<L: SerializableLang>(
    wire: WireSpan,
    cx: &mut DeserializeContext<'_, L>,
) -> Result<SourceSpan<L::SourceOrigin>, DeserializeError> {
    let source = cx.source(wire.source)?;
    let content = source.content();
    let (start, end) = (wire.start, wire.end);
    if start > end || end > content.len() {
        return Err(DeserializeError::SpanOutOfBounds { start, end, len: content.len() });
    }
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return Err(DeserializeError::SpanNotOnCharBoundary { start, end });
    }
    // Validated above: within bounds and on character boundaries, so the constructor's
    // precondition holds.
    Ok(SourceSpan::new(&source, start..end))
}

/// A span is the value `{"source": <position>, "start": <int>, "end": <int>}`
/// (provisional wire names): its source is interned into the sources table (a source
/// interned before yields its existing position, so spans into one source share one
/// entry), and its byte range is embedded.
impl<L: Lang> SerializableValue<L> for SourceSpan<L::SourceOrigin> {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(serialize_span(self, cx)?.to_serial_value()?)
    }
}

/// A span is read back over the source at the recorded position (the same `Arc` for
/// every span into that source), with its byte range validated against the source's
/// text ([`DeserializeError::SpanOutOfBounds`],
/// [`DeserializeError::SpanNotOnCharBoundary`]).
impl<L: Lang> DeserializableValue<L> for SourceSpan<L::SourceOrigin> {
    fn deserialize_value(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        deserialize_span(WireSpan::from_serial_value(value)?, cx)
    }
}
