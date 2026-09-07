//! The parse-results table: [`ParseResultSerdeDriver`], its driver, and
//! [`ParseResultIndex`], its position type.
//!
//! The driver delegates to the [`SerializableObject`] and [`DeserializableObject`] impls
//! of [`ParseResult`], which are defined here, and [`ParseResultSerialization`] holds
//! the by-kind methods (`serialize_parse_result` and `parse_result`).
//!
//! A parse result's entry ties together what a parse produced: its tree (an entry of the
//! trees table), its diagnostics (entries of the diagnostics table, plus the
//! collection's retention cap and counts), and its session extension (in the language's
//! own form). Everything the tree and the diagnostics refer to — sources, states, specs,
//! providers — is shared through the standard tables, so one stream holds a whole parse.
//! This is the table a program that keeps or transmits complete parses works with.

use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;
use core::marker::PhantomData;

use crate::engine::ParseResult;
use crate::error::Diagnostics;
use crate::node::NodeTree;
use crate::state::Lang;

use super::super::engine::{DeserializeContext, ObjectSerdeDriver, SerdeSession, SerializeContext};
use super::super::error::{DeserializeError, SerializeError};
use super::super::object::{
    DeserializableObject, DeserializableValue, SerializableLang, SerializableObject,
    SerializableValue,
};
use super::super::value::{SerialEntry, SerialValue};
use super::super::wire::parse_result::{WireDiagnostics, WireParseResult};
use super::super::wire::{FromSerialValue, ToSerialValue};
use super::standard::StandardTables;
use super::tree::tree_of_object;
use super::{PARSE_RESULTS_TABLE, PARSE_RESULT_IDENTIFIER, TREES_TABLE};

crate::serial_index! {
    /// A position in the parse-results table — the `Index` type of
    /// [`ParseResultSerdeDriver`]: the serialized reference to a [`ParseResult`].
    pub struct ParseResultIndex;
}

/// The driver of the parse-results table (table name `parse-results`, one kind of
/// object — identifier `core.parse-result`): how a [`ParseResult`] is serialized and
/// rebuilt.
///
/// A parse result's entry records the position of its tree in the trees table (written
/// under the unit annotation, as the parser produces it), its diagnostics collection,
/// and its session extension ([`ParseResult::session_ext`], through the language's
/// `SessionExt` value conversion).
///
/// The diagnostics collection is recorded as the positions of the retained diagnostics
/// in the diagnostics table, in recording order, plus the collection's retention cap
/// ([`Diagnostics::limit`]), its [`suppressed`](Diagnostics::suppressed) count, and its
/// count of error-severity pushes (what [`Diagnostics::has_errors`] answers, retained
/// and suppressed alike).
///
/// Writing a parse result writes its tree and each of its diagnostics as new entries of
/// their tables, since both are values, and interns everything they refer to into the
/// standard tables, so the whole parse is written into one stream with its sharing
/// intact.
///
/// Reading rebuilds the tree through the trees table and each diagnostic through the
/// diagnostics table (its condition is then a
/// [`DeserializedCondition`](crate::serialize::DeserializedCondition) — see
/// [`DiagnosticSerdeDriver`](crate::serialize::DiagnosticSerdeDriver)), reads the
/// session extension back, and re-establishes
/// the diagnostics collection with the recorded cap and counts through
/// [`Diagnostics::from_parts`], which checks that they are consistent with one another
/// (the invariants [`Diagnostics::push`] maintains: no more retained diagnostics than
/// the cap, suppressed pushes only when the cap was reached, an error count between the
/// retained errors and the retained errors plus the suppressed pushes —
/// [`DeserializeError::InconsistentDiagnosticCounts`] otherwise; whether the suppressed
/// pushes were errors is not recoverable, so the error count is trusted within those
/// bounds).
///
/// A parse result is an object of the table: interning the same `Arc<ParseResult>` twice
/// yields the existing position, unlike a tree or a diagnostic written on its own, which
/// is a value. Reading back yields the shared `Arc<ParseResult>` the session holds — a
/// parse result is not cloned out, since a language's session extension need not be
/// `Clone`.
///
/// The by-kind methods are the [`ParseResultSerialization`] extension trait's
/// `serialize_parse_result` and `parse_result`. The driver delegates to `ParseResult`'s
/// own [`SerializableObject`] and [`DeserializableObject`] impls. Registered by
/// [`SerdeSession::new`](crate::serialize::SerdeSession::new).
///
/// Everything read is untrusted input: a tree position naming an entry that is not a
/// tree of the unit annotation, a diagnostic position out of range or into another
/// table, inconsistent counts — each is an error, never a panic.
pub struct ParseResultSerdeDriver<L: Lang> {
    lang: PhantomData<fn() -> L>,
}

impl<L: Lang> ParseResultSerdeDriver<L> {
    /// The driver (it has no configuration).
    pub fn new() -> ParseResultSerdeDriver<L> {
        ParseResultSerdeDriver { lang: PhantomData }
    }
}

impl<L: Lang> Default for ParseResultSerdeDriver<L> {
    fn default() -> Self {
        ParseResultSerdeDriver::new()
    }
}

impl<L: Lang> fmt::Debug for ParseResultSerdeDriver<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseResultSerdeDriver").finish()
    }
}

impl<L: SerializableLang> ObjectSerdeDriver<L> for ParseResultSerdeDriver<L> {
    type Object = ParseResult<L>;
    type Index = ParseResultIndex;

    fn table_name(&self) -> &'static str {
        PARSE_RESULTS_TABLE
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        Some(PARSE_RESULT_IDENTIFIER)
    }

    fn serialize_object(
        &self,
        result: &Arc<ParseResult<L>>,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError> {
        result.serialize_object(cx)
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<ParseResult<L>>, DeserializeError> {
        ParseResult::<L>::deserialize_object(&entry.data, cx).map(Arc::new)
    }
}

// --- the object impls -------------------------------------------------------------------

/// A parse result is serialized as its tree's position (the tree written into the
/// trees table under the unit annotation), its diagnostics collection (each retained
/// diagnostic written into the diagnostics table, plus the cap and counts), and its
/// session extension (see [`ParseResultSerdeDriver`]).
impl<L: Lang> SerializableObject<L> for ParseResult<L> {
    /// # Panics
    ///
    /// Writing the result's tree materializes each callable node's invocation syntax
    /// against that node's own source, so this panics if a range recorded there is not a
    /// valid `char`-boundary range of that source — a broken tree invariant, which no
    /// parsed input can cause and which
    /// [`validate_tree`](crate::core::node::validate_tree) detects; the panic is
    /// [`TextContent::resolve`](crate::source::TextContent::resolve)'s (see the [list of
    /// panicking items](crate::guide::panics)).
    fn serialize_object(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let StandardTables { trees, diagnostics, .. } = cx
            .standard_tables()
            .ok_or_else(|| SerializeError::UnknownTableName { name: TREES_TABLE.to_string() })?;
        // The tree and the diagnostics are values: fresh entries, written in full.
        let tree_object: Arc<dyn Any + Send + Sync> = Arc::new(self.tree.clone());
        let tree = cx.intern(trees, &tree_object)?;
        let items = self
            .diagnostics
            .iter()
            .map(|diagnostic| cx.intern(diagnostics, &Arc::new(diagnostic.clone())))
            .collect::<Result<Vec<_>, _>>()?;
        let wire = WireParseResult {
            tree,
            diagnostics: WireDiagnostics {
                items,
                limit: self.diagnostics.limit(),
                suppressed: self.diagnostics.suppressed(),
                error_count: self.diagnostics.error_count(),
            },
            session_ext: self.session_ext.serialize_value(cx)?,
        };
        Ok(SerialEntry { identifier: PARSE_RESULT_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

/// A parse result is read back from its tree's and diagnostics' entries and its
/// session extension, the diagnostics collection re-established with the recorded cap
/// and counts after they are checked for consistency (see [`ParseResultSerdeDriver`]).
impl<L: SerializableLang> DeserializableObject<L> for ParseResult<L> {
    type Output = ParseResult<L>;

    fn deserialize_object(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<ParseResult<L>, DeserializeError> {
        let wire = WireParseResult::from_serial_value(value)?;
        let StandardTables { trees, diagnostics, .. } = cx
            .standard_tables()
            .ok_or_else(|| DeserializeError::UnknownTableName { name: TREES_TABLE.to_string() })?;
        let tree_object = cx.object(trees, wire.tree)?;
        let tree: NodeTree<L> = tree_of_object(cx.session_mut(), tree_object)?;
        let items = wire
            .diagnostics
            .items
            .iter()
            .map(|&position| cx.object(diagnostics, position).map(|diagnostic| (*diagnostic).clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let session_ext = <L::SessionExt as DeserializableValue<L>>::deserialize_value(&wire.session_ext, cx)?;
        let WireDiagnostics { limit, suppressed, error_count, .. } = wire.diagnostics;
        let diagnostics = Diagnostics::from_parts(items, limit, suppressed, error_count)
            .map_err(DeserializeError::InconsistentDiagnosticCounts)?;
        Ok(ParseResult { tree, diagnostics, session_ext })
    }
}

// --- the by-kind methods ----------------------------------------------------------------

/// Serializing a parse result into a session's parse-results table and reading one back:
/// `serialize_parse_result` and `parse_result` on a [`SerdeSession`].
///
/// These are the convenience methods over the general
/// [`SerdeSession::intern`](crate::serialize::SerdeSession::intern) /
/// [`SerdeSession::object`](crate::serialize::SerdeSession::object) with the
/// parse-results table handle.
///
/// An extension trait: bring it into scope with `use techy::serialize::ParseResultSerialization;`.
pub trait ParseResultSerialization<L: SerializableLang> {
    /// Serializes `result` into the parse-results table, returning its position.
    ///
    /// Its tree and each of its diagnostics are written as new entries of their tables,
    /// and everything they refer to is interned into the standard tables.
    ///
    /// The parse result is interned by identity — the same `Arc` again yields its
    /// existing position — which is why it is passed as an `Arc`: wrap a
    /// [`Language::parse`](crate::core::Language::parse) result with `Arc::new`. It
    /// cannot be cloned into one here, since a language's session extension need not be
    /// `Clone`.
    ///
    /// # Errors
    ///
    /// The session lacks a standard table ([`SerializeError::UnknownTableName`]); a
    /// node's, a diagnostic's, or the session extension's serialization fails (its
    /// error, wrapped in [`SerializeError::InTable`]); the diagnostics' retention cap
    /// or counts do not fit the serialized form's integers — a collection created with
    /// [`Diagnostics::with_limit`](crate::error::Diagnostics::with_limit) above
    /// `i64::MAX` (`usize::MAX` as "no cap", say) cannot be serialized
    /// ([`SerialValueError::IntegerOutOfRange`](crate::serialize::SerialValueError::IntegerOutOfRange)
    /// through [`SerializeError::Value`]); the errors of
    /// [`SerdeSession::intern`](crate::serialize::SerdeSession::intern).
    ///
    /// # Panics
    ///
    /// Writing the result's tree materializes each callable node's invocation syntax
    /// against that node's own source, so this panics if a range recorded there is not a
    /// valid `char`-boundary range of that source. That is a broken tree invariant,
    /// which no parsed input can cause and which
    /// [`validate_tree`](crate::core::node::validate_tree) detects; the panic is
    /// [`TextContent::resolve`](crate::source::TextContent::resolve)'s (see the [list of
    /// panicking items](crate::guide::panics)).
    fn serialize_parse_result(&mut self, result: &Arc<ParseResult<L>>) -> Result<ParseResultIndex, SerializeError>;

    /// The parse result at `position` of the parse-results table: the `Arc` the session
    /// holds, the same one for every call with that position.
    ///
    /// Its tree is a `NodeTree<L>`, and each of its diagnostics has a
    /// [`DeserializedCondition`](crate::serialize::DeserializedCondition) as its
    /// condition.
    ///
    /// # Errors
    ///
    /// The session has no parse-results table ([`DeserializeError::UnknownTableName`]);
    /// the errors of [`SerdeSession::object`](crate::serialize::SerdeSession::object).
    fn parse_result(&mut self, position: ParseResultIndex) -> Result<Arc<ParseResult<L>>, DeserializeError>;
}

impl<L: SerializableLang> ParseResultSerialization<L> for SerdeSession<L> {
    fn serialize_parse_result(&mut self, result: &Arc<ParseResult<L>>) -> Result<ParseResultIndex, SerializeError> {
        let handle = self
            .table_handle::<ParseResultSerdeDriver<L>>(PARSE_RESULTS_TABLE)
            .ok_or_else(|| SerializeError::UnknownTableName { name: PARSE_RESULTS_TABLE.to_string() })?;
        self.intern(handle, result)
    }

    fn parse_result(&mut self, position: ParseResultIndex) -> Result<Arc<ParseResult<L>>, DeserializeError> {
        let handle = self
            .table_handle::<ParseResultSerdeDriver<L>>(PARSE_RESULTS_TABLE)
            .ok_or_else(|| DeserializeError::UnknownTableName { name: PARSE_RESULTS_TABLE.to_string() })?;
        self.object(handle, position)
    }
}
