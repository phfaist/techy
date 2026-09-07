//! Structured diagnostics, the strict/tolerant recovery policy, and the parse abort
//! error.
//!
//! Everything a parse can complain about is described by a **condition**: a small data
//! value of a concrete public type, one type per kind of problem, whose fields say what
//! went wrong. A condition reaches the caller in one of two ways: as a [`Diagnostic`],
//! which a parse records and continues past, or as a [`ParseError`], which ends the
//! parse. Both store the condition itself, the [`SourceSpan`] where it occurred, and a
//! snapshot of the parse frames that were open at that moment ([`TraceFrame`]s).
//!
//! The library reports its conditions only through these two types; it never writes them
//! to a logging side channel.
//!
//! # Tolerant parsing
//!
//! The [`Recovery`] policy, chosen on the parse driver, decides which of the two a
//! problem becomes:
//!
//! - [`Recovery::Tolerant`] — the condition is recorded as an error-severity
//!   [`Diagnostic`], the parser repairs the input at the point of detection in the way
//!   that condition type documents, and parsing continues.
//! - [`Recovery::Strict`] — the condition is returned as a [`ParseError`] and the parse
//!   aborts at the first problem.
//!
//! A tolerant parse therefore succeeds and still reports problems. `parse()` returns a
//! [`ParseResult`](crate::core::ParseResult) whose
//! [`tree`](crate::core::ParseResult::tree) covers the whole input and whose
//! [`diagnostics`](crate::core::ParseResult::diagnostics) field is a [`Diagnostics`]
//! collection holding what was recorded. Call
//! [`has_errors`](Diagnostics::has_errors) before treating the tree as clean,
//! [`render_all`](Diagnostics::render_all) to print a report, and
//! [`sorted_by_position`](Diagnostics::sorted_by_position) to read the problems in
//! document order rather than in the order the parse hit them.
//!
//! [`Severity`] is a separate axis from the recovery policy: it grades a diagnostic as a
//! note, a warning, or an error, and only errors make
//! [`has_errors`](Diagnostics::has_errors) true.
//!
//! # Working with conditions
//!
//! Neither type stores a message string. The human wording is computed from the
//! condition on demand ([`Diagnostic::message`], [`ParseError::message`]), and a full
//! report with position and traceback comes from [`Diagnostic::render`].
//!
//! To act on a condition programmatically, downcast it to its concrete type — the
//! condition is reached as [`Diagnostic::data`] and downcast with
//! [`downcast_ref`](trait.DiagnosticData.html#method.downcast_ref), or, over a whole
//! collection, with [`Diagnostics::conditions`].
//!
//! Where Rust types cannot travel — log lines, wire formats, configuration — use the
//! condition's identifier string, read from the type as [`DiagnosticInfo::IDENTIFIER`]
//! rather than written as a literal.
//!
//! Conditions of your own are ordinary conditions: implement [`DiagnosticInfo`] on a data
//! struct, normally with [`#[derive(DiagnosticInfo)]`](derive@DiagnosticInfo), and it
//! travels through the same types as the library's own. Library conditions are themselves
//! plain structs with public fields, defined next to the construct that detects them.
//!
//! # Spans outlive the parse
//!
//! Diagnostics and parse errors hold `Arc`-based [`SourceSpan`]s, so they are
//! self-contained values that outlive the parse that produced them, and no source
//! lifetime appears in error signatures.
//!
//! # Related error types
//!
//! [`ParseError`] ends construct parsing, and deliberately has **no** recovery payload:
//! nothing continues past one. The token-level error that does hold a recovery token,
//! [`TokenError`](crate::core::token::TokenError), is defined with the other token types,
//! though the [`Recovery`] policy governing it is defined here.
//!
//! # See also
//!
//! - [Running the parser](crate::guide::parsing#working-with-diagnostics) — choosing a
//!   recovery policy and working with what a parse reports.
//! - [The parsing model](crate::guide::parsing_model#how-problems-flow) — where
//!   conditions are detected and how the policy is applied to them.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;

use crate::source::{
    LineColProvider, LineIndexCache, Source, SourceOrigin, SourceProvenance, SourceSpan,
};
use crate::token::TokenErrorKind;

// The condition-declaration derives (ARCHITECTURE.md "Condition declaration via
// derive"), re-exported next to the traits they implement: `#[derive(DiagnosticInfo)]`
// for condition structs, `#[derive(ToDiagnosticValue)]` for field-less payload enums.
pub use techy_derive::{DiagnosticInfo, ToDiagnosticValue};

/// Makes a data struct a parse **condition**: one kind of problem a parse can report.
///
/// The implementors listed below are the conditions the library itself reports. A
/// condition defined outside the library is indistinguishable from them — it is carried,
/// rendered, matched, and serialized by exactly the same machinery.
///
/// # Writing a condition type
///
/// A condition is a plain data struct whose public fields describe the problem. Four
/// things turn it into one:
///
/// - `Clone` and `Debug`, so the carriers can store and duplicate it;
/// - a `Display` implementation producing the human wording, computed from the fields
///   rather than stored as a message string;
/// - this trait, supplying the stable [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) and,
///   optionally, [`serializable_data`](DiagnosticInfo::serializable_data);
/// - by convention `#[non_exhaustive]` plus a constructor, so that fields can be added
///   later without breaking callers.
///
/// [`#[derive(DiagnosticInfo)]`](derive@DiagnosticInfo) writes all of that except the
/// fields themselves: the trait implementation, a `Display` implementation from a message
/// format string, and the constructor.
///
/// # How a condition is carried
///
/// [`DiagnosticData`] is the dyn-compatible form of this trait and is what [`Diagnostic`]
/// and [`ParseError`] store; it is blanket-implemented for every `DiagnosticInfo` type,
/// so a condition needs no further code to be reportable. Consumers get back to the
/// concrete struct by downcasting.
///
/// Downcasting works within one process. A diagnostic read back through
/// [`techy::serialize`](crate::serialize) has lost the original Rust type and carries a
/// [`DeserializedCondition`](crate::serialize::DeserializedCondition) instead, which
/// answers the written identifier, projection, and message.
pub trait DiagnosticInfo: Any + Clone + fmt::Display + fmt::Debug + Send + Sync {
    /// The condition's identity as a string: `<crate-or-lang>.<area>.<condition>`.
    ///
    /// This is the identity to use wherever Rust types cannot travel — log lines, wire
    /// formats, configuration that names conditions. Library conditions use the
    /// `core.<area>.*` namespace; presets and downstream languages use their own.
    ///
    /// It is chosen by hand and deliberately decoupled from the type and module name.
    /// It is part of the semantic-versioning contract: renaming the struct is an
    /// internal refactor, while changing this string silently breaks every consumer
    /// matching on it.
    ///
    /// At a comparison site, read it from the type (`MyCondition::IDENTIFIER`) instead of
    /// writing the string literally.
    const IDENTIFIER: &'static str;

    /// The identity a stored condition *instance* reports.
    ///
    /// This is what [`Diagnostic::identifier`] and [`ParseError::identifier`] return.
    /// The default implementation returns [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER), and
    /// every ordinary condition keeps it: one Rust type, one compile-time identifier.
    ///
    /// Override it only where a compile-time identifier is impossible — in an **adapter
    /// type** for a language binding or embedding, where a single Rust struct carries
    /// conditions defined at run time on the other side of the boundary (conditions
    /// defined in Python, say) and stores each one's identifier in a field. Such an
    /// adapter still declares its own [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER), which
    /// remains the type's identity for type-keyed uses; the override changes only what
    /// stored instances report.
    ///
    /// With both this trait and [`DiagnosticData`] in scope, an unqualified
    /// `.identifier()` call on a concrete condition type is ambiguous, because both
    /// traits supply the method (error E0034). Write `DiagnosticInfo::identifier(&c)` or
    /// `DiagnosticData::identifier(&c)`.
    fn identifier(&self) -> &str {
        Self::IDENTIFIER
    }

    /// This condition's field projection for the serialization boundary.
    ///
    /// It is used when a diagnostic is written out — through
    /// [`techy::serialize`](crate::serialize), or by generic tooling that renders
    /// conditions without knowing their types. It is not an access path for program
    /// logic: a consumer in the same process downcasts to the concrete struct, which
    /// stays the authoritative description of the payload.
    ///
    /// The default returns an empty map.
    /// [`#[derive(DiagnosticInfo)]`](derive@DiagnosticInfo) generates a map of every
    /// field, each converted through [`ToDiagnosticValue`].
    fn serializable_data(&self) -> DiagnosticValue {
        DiagnosticValue::empty_map()
    }
}

mod sealed {
    /// Seals [`DiagnosticData`](super::DiagnosticData).
    ///
    /// The blanket implementation over [`DiagnosticInfo`](super::DiagnosticInfo) is the
    /// only way to obtain it, so every stored condition is a `DiagnosticInfo` type and
    /// comes with a compile-time identifier by default; the per-instance
    /// [`identifier()`](super::DiagnosticInfo::identifier) override is the exceptional
    /// case, for binding and embedding adapter types.
    pub trait Sealed {}
}

impl<T: DiagnosticInfo> sealed::Sealed for T {}

/// The condition payload as [`Diagnostic`] and [`ParseError`] store it: the
/// dyn-compatible form of [`DiagnosticInfo`].
///
/// A value of this trait renders its human message through `Display` and answers its
/// [`identifier`](DiagnosticData::identifier), but says nothing about which condition it
/// is. To read the condition's own fields, downcast it to the concrete type with
/// [`downcast_ref`](trait.DiagnosticData.html#method.downcast_ref); that concrete struct
/// is a condition's identity within one process.
///
/// **Sealed**: the blanket implementation over [`DiagnosticInfo`] is the only one, so
/// implement `DiagnosticInfo` to make a type reportable.
pub trait DiagnosticData:
    Any + fmt::Display + fmt::Debug + Send + Sync + sealed::Sealed
{
    /// The condition's wire identity ([`DiagnosticInfo::identifier`] — for every
    /// ordinary condition, the [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) const).
    fn identifier(&self) -> &str;

    /// The condition's serialization-boundary projection
    /// ([`DiagnosticInfo::serializable_data`]).
    fn serializable_data(&self) -> DiagnosticValue;

    /// Clones the payload from behind the trait object.
    ///
    /// This is what makes [`Diagnostic`] and [`ParseError`] `Clone`; it is backed by the
    /// concrete condition type's own `Clone` implementation.
    fn clone_box(&self) -> Box<dyn DiagnosticData>;
}

impl<T: DiagnosticInfo> DiagnosticData for T {
    fn identifier(&self) -> &str {
        DiagnosticInfo::identifier(self)
    }

    fn serializable_data(&self) -> DiagnosticValue {
        DiagnosticInfo::serializable_data(self)
    }

    fn clone_box(&self) -> Box<dyn DiagnosticData> {
        Box::new(self.clone())
    }
}

impl dyn DiagnosticData {
    /// Returns whether the condition payload is a `T`.
    ///
    /// Use [`downcast_ref`](trait.DiagnosticData.html#method.downcast_ref) when the
    /// payload's fields are wanted as well.
    pub fn is<T: DiagnosticInfo>(&self) -> bool {
        (self as &dyn Any).is::<T>()
    }

    /// Returns the condition payload as its concrete type, or `None` if it is not a `T`.
    ///
    /// This is the way to read a condition's fields. The
    /// [`identifier`](DiagnosticData::identifier) string exists only for boundaries where
    /// Rust types cannot travel.
    pub fn downcast_ref<T: DiagnosticInfo>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

impl Clone for Box<dyn DiagnosticData> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// A small value tree: what a condition's fields project to at the serialization
/// boundary ([`DiagnosticInfo::serializable_data`]).
///
/// The set of variants is deliberately minimal. In particular there is no float variant;
/// a condition with a floating-point field projects it as a string.
///
/// A `DiagnosticValue` converts into a [`SerialValue`](crate::serialize::SerialValue)
/// (the `From` implementations are in [`techy::serialize`](crate::serialize)). With the
/// `serde` cargo feature it implements `Serialize` and `Deserialize` through that
/// conversion, so it renders exactly as the corresponding `SerialValue` does, and reading
/// back accepts only the kinds this tree can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticValue {
    /// No value.
    Null,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A string.
    Str(String),
    /// An ordered list of values.
    List(Vec<DiagnosticValue>),
    /// An ordered map of named values.
    Map(Vec<(String, DiagnosticValue)>),
}

impl DiagnosticValue {
    /// Returns an empty [`Map`](DiagnosticValue::Map).
    ///
    /// This is the default projection of a condition that serializes no fields — the
    /// value returned by the default
    /// [`serializable_data`](DiagnosticInfo::serializable_data).
    pub fn empty_map() -> DiagnosticValue {
        DiagnosticValue::Map(Vec::new())
    }
}

/// Converts one field of a condition into its [`DiagnosticValue`] projection.
///
/// This is the bridge behind the
/// [`serializable_data`](DiagnosticInfo::serializable_data) implementation that
/// [`#[derive(DiagnosticInfo)]`](derive@DiagnosticInfo) generates: the derive routes
/// every field of the condition through this trait.
///
/// Implementations are provided for the types condition payloads normally hold —
/// booleans, integers, `char`, strings, `Option`, slices and `Vec`, references, and
/// `DiagnosticValue` itself.
///
/// A field whose type does not implement this trait is rejected by the compiler at the
/// field's own declaration, so whether a payload can be serialized is decided by trait
/// bounds rather than by a check inside the macro. A field-less enum gets an
/// implementation from [`#[derive(ToDiagnosticValue)]`](derive@ToDiagnosticValue), which
/// projects the variant name in kebab case; any other type can implement the trait by
/// hand.
pub trait ToDiagnosticValue {
    /// Returns this value's projection for the serialization boundary.
    fn to_diagnostic_value(&self) -> DiagnosticValue;
}

impl ToDiagnosticValue for bool {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        DiagnosticValue::Bool(*self)
    }
}

/// Integers that fit `i64` losslessly.
macro_rules! int_to_diagnostic_value {
    ($($t:ty),*) => {$(
        impl ToDiagnosticValue for $t {
            fn to_diagnostic_value(&self) -> DiagnosticValue {
                DiagnosticValue::Int(i64::from(*self))
            }
        }
    )*};
}
int_to_diagnostic_value!(i8, i16, i32, i64, u8, u16, u32);

/// Integers that may exceed `i64`: the projection saturates at `i64::MAX` rather than
/// failing or panicking.
macro_rules! saturating_int_to_diagnostic_value {
    ($($t:ty),*) => {$(
        impl ToDiagnosticValue for $t {
            fn to_diagnostic_value(&self) -> DiagnosticValue {
                DiagnosticValue::Int(i64::try_from(*self).unwrap_or(i64::MAX))
            }
        }
    )*};
}
saturating_int_to_diagnostic_value!(u64, usize);

impl ToDiagnosticValue for isize {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        // isize is at most 64 bits on every supported target; the cast is lossless.
        DiagnosticValue::Int(*self as i64)
    }
}

impl ToDiagnosticValue for char {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        DiagnosticValue::Str(String::from(*self))
    }
}

impl ToDiagnosticValue for str {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        DiagnosticValue::Str(String::from(self))
    }
}

impl ToDiagnosticValue for String {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        DiagnosticValue::Str(self.clone())
    }
}

impl<T: ToDiagnosticValue> ToDiagnosticValue for Option<T> {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        match self {
            Some(value) => value.to_diagnostic_value(),
            None => DiagnosticValue::Null,
        }
    }
}

impl<T: ToDiagnosticValue> ToDiagnosticValue for [T] {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        DiagnosticValue::List(self.iter().map(T::to_diagnostic_value).collect())
    }
}

impl<T: ToDiagnosticValue> ToDiagnosticValue for Vec<T> {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        self.as_slice().to_diagnostic_value()
    }
}

impl<T: ToDiagnosticValue + ?Sized> ToDiagnosticValue for &T {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        (**self).to_diagnostic_value()
    }
}

impl ToDiagnosticValue for DiagnosticValue {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        self.clone()
    }
}

/// Projects as a map of the failed `reference`, the `message`, and a `cause_chain`
/// list: the `Display` of each [`Error::source`](core::error::Error::source) hop,
/// outermost first.
///
/// This is the serialization face of the payload of the
/// [`UnresolvableSourceReference`](crate::core::constructs::UnresolvableSourceReference)
/// condition.
// The implementation is here rather than in the source module: the error module may
// depend on source types, never the reverse.
impl ToDiagnosticValue for crate::source::ResolveError {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        let mut chain: Vec<DiagnosticValue> = Vec::new();
        let mut cause = core::error::Error::source(self);
        while let Some(error) = cause {
            chain.push(DiagnosticValue::Str(error.to_string()));
            cause = error.source();
        }
        DiagnosticValue::Map(alloc::vec![
            ("reference".to_string(), DiagnosticValue::Str(self.reference().to_string())),
            ("message".to_string(), DiagnosticValue::Str(self.message().to_string())),
            ("cause_chain".to_string(), DiagnosticValue::List(chain)),
        ])
    }
}

/// Projects as the rendered cause chain: a list holding the error's own `Display`,
/// then that of each [`Error::source`](core::error::Error::source) hop, outermost
/// first.
///
/// `Arc<dyn Error + Send + Sync>` is the shape a condition's optional underlying-cause
/// field takes — see [`HookFailed::cause`], which follows the
/// [`ResolveError`](crate::source::ResolveError) pattern. Sharing the error through an
/// `Arc` is what keeps such a condition `Clone`.
impl ToDiagnosticValue for Arc<dyn core::error::Error + Send + Sync + 'static> {
    fn to_diagnostic_value(&self) -> DiagnosticValue {
        let mut chain: Vec<DiagnosticValue> = Vec::new();
        chain.push(DiagnosticValue::Str(self.to_string()));
        let mut cause = core::error::Error::source(&**self);
        while let Some(error) = cause {
            chain.push(DiagnosticValue::Str(error.to_string()));
            cause = error.source();
        }
        DiagnosticValue::List(chain)
    }
}

/// One frame of a parse traceback: what the parse had descended into, and where.
///
/// The [`title`](TraceFrame::title) is already rendered for display (`group ‘{’`,
/// `argument #1 of ‘\frac’`), and the [`span`](TraceFrame::span) is the source location
/// the parse descended at.
///
/// [`Diagnostic`] and [`ParseError`] store a list of these, **innermost first**, as a
/// snapshot of the frames that were open when the problem was reported. The recovery
/// entry point,
/// [`ParseContext::recover`](crate::core::constructs::ParseContext::recover), takes that
/// snapshot from the session's live frame stack.
///
/// Unlike the live [`Frame`](crate::core::Frame), a `TraceFrame` is not generic over the
/// language — only over the source origin — so diagnostics produced by parses of
/// different languages can be collected together.
#[derive(Debug, Clone)]
pub struct TraceFrame<O: SourceOrigin = Option<String>> {
    title: String,
    span: SourceSpan<O>,
}

impl<O: SourceOrigin> TraceFrame<O> {
    /// Creates a traceback frame from an already-rendered title and the location the
    /// parse descended at.
    pub fn new(title: impl Into<String>, span: SourceSpan<O>) -> TraceFrame<O> {
        TraceFrame { title: title.into(), span }
    }

    /// The rendered frame title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Where in the source the parse descended into this frame.
    pub fn span(&self) -> &SourceSpan<O> {
        &self.span
    }
}

/// How severe a [`Diagnostic`] is.
///
/// The derived ordering ranks `Note < Warning < Error`, so filtering on a threshold
/// ("warnings and above") is a comparison.
///
/// Severity is independent of the [`Recovery`] policy. Problems reported through the
/// recovery entry point are always recorded at [`Error`](Severity::Error); the library's
/// [`Warning`](Severity::Warning) and [`Note`](Severity::Note) diagnostics come from
/// places that never abort a parse, such as
/// [`DescentLimitApproaching`](crate::core::constructs::DescentLimitApproaching) or
/// [`ProviderCommandsShadowedByEscape`](crate::core::specs::ProviderCommandsShadowedByEscape).
/// Presets and embedders can record diagnostics at any severity through
/// [`Diagnostic::warning`] and [`Diagnostic::note`].
///
/// Only error-severity diagnostics count toward
/// [`Diagnostics::has_errors`](Diagnostics::has_errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational note.
    Note,
    /// Something suspicious that did not prevent parsing, and produced no recovery.
    Warning,
    /// A genuine error: the document is wrong, or a parse could not do what was asked.
    ///
    /// Under [`Recovery::Tolerant`] such a condition is recorded here instead of
    /// aborting the parse; under [`Recovery::Strict`] it becomes a [`ParseError`].
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        })
    }
}

/// One problem reported during a parse: a condition, its severity, and where it
/// occurred.
///
/// A tolerant parse collects these in a [`Diagnostics`] collection, reached as the
/// [`diagnostics`](crate::core::ParseResult::diagnostics) field of the
/// [`ParseResult`](crate::core::ParseResult). Code outside the parser creates them with
/// [`error`](Diagnostic::error), [`warning`](Diagnostic::warning),
/// [`note`](Diagnostic::note), or [`new`](Diagnostic::new).
///
/// The condition is stored as structured data rather than as a message string: read its
/// fields by downcasting [`data`](Diagnostic::data), get the human wording from
/// [`message`](Diagnostic::message), and a full report with position and traceback from
/// [`render`](Diagnostic::render).
///
/// [`frames`](Diagnostic::frames) is the traceback of parse frames that were open when
/// the problem was reported; the recovery entry point,
/// [`ParseContext::recover`](crate::core::constructs::ParseContext::recover), attaches
/// it.
///
/// `Diagnostic` deliberately does not implement `PartialEq`, since conditions are
/// compared through a trait object that hides their type. Compare
/// [`identifier()`](Diagnostic::identifier) and the fields of the downcast condition
/// instead.
#[derive(Debug, Clone)]
pub struct Diagnostic<O: SourceOrigin = Option<String>> {
    severity: Severity,
    data: Box<dyn DiagnosticData>,
    span: SourceSpan<O>,
    /// Traceback snapshot, innermost first.
    frames: Vec<TraceFrame<O>>,
}

impl<O: SourceOrigin> Diagnostic<O> {
    /// Creates a diagnostic from a condition, at the given severity and span.
    ///
    /// The traceback is empty; a parse attaches frames through the recovery entry point,
    /// [`ParseContext::recover`](crate::core::constructs::ParseContext::recover).
    ///
    /// [`error`](Diagnostic::error), [`warning`](Diagnostic::warning) and
    /// [`note`](Diagnostic::note) are the shorthands for the three severities.
    pub fn new(
        severity: Severity,
        condition: impl DiagnosticInfo,
        span: SourceSpan<O>,
    ) -> Self {
        Diagnostic { severity, data: Box::new(condition), span, frames: Vec::new() }
    }

    /// Creates a diagnostic at [`Severity::Error`].
    pub fn error(condition: impl DiagnosticInfo, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Error, condition, span)
    }

    /// Creates a diagnostic at [`Severity::Warning`].
    pub fn warning(condition: impl DiagnosticInfo, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Warning, condition, span)
    }

    /// Creates a diagnostic at [`Severity::Note`].
    pub fn note(condition: impl DiagnosticInfo, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Note, condition, span)
    }

    /// Assembles a diagnostic from parts already prepared by the recovery path: a
    /// condition boxed after the driver's refinement pass, and a frame snapshot.
    pub(crate) fn from_parts(
        severity: Severity,
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<O>,
        frames: Vec<TraceFrame<O>>,
    ) -> Self {
        Diagnostic { severity, data, span, frames }
    }

    /// The severity of this diagnostic.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The condition that was reported.
    ///
    /// Downcast it with
    /// [`downcast_ref`](trait.DiagnosticData.html#method.downcast_ref) to read the
    /// condition's own fields.
    pub fn data(&self) -> &dyn DiagnosticData {
        &*self.data
    }

    /// The condition's identifier string, for boundaries where Rust types cannot travel.
    ///
    /// For every ordinary condition this is the type's
    /// [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) constant; see
    /// [`DiagnosticInfo::identifier`] for the adapter-type exception. Compare it against
    /// `T::IDENTIFIER` rather than against a string literal.
    pub fn identifier(&self) -> &str {
        self.data.identifier()
    }

    /// The human-readable message, rendered from the condition's `Display`.
    ///
    /// This allocates a fresh `String` on each call; the condition is the stored value,
    /// the message is derived from it.
    pub fn message(&self) -> String {
        self.data.to_string()
    }

    /// Where in the source the condition occurred.
    pub fn span(&self) -> &SourceSpan<O> {
        &self.span
    }

    /// The parse frames that were open when the condition was recorded, innermost first.
    ///
    /// Empty when the diagnostic was recorded outside a parse descent. Format them with
    /// [`format_traceback`].
    pub fn frames(&self) -> &[TraceFrame<O>] {
        &self.frames
    }

    /// Renders a human-readable, multi-line report of this diagnostic.
    ///
    /// The report holds the message, the position (line and column, plus the source's
    /// origin label when it has one), the traceback of open blocks
    /// ([`format_traceback`]), and the source's provenance chain, as `included from …` or
    /// `synthesized from …` lines.
    ///
    /// Line and column numbers are computed through a [`LineIndexCache`] created for this
    /// call and dropped with it. Use [`render_with`](Diagnostic::render_with) to supply a
    /// cache that persists across calls, or [`Diagnostics::render_all`] for a whole
    /// collection.
    pub fn render(&self) -> String {
        self.render_with(&mut LineIndexCache::new())
    }

    /// [`render`](Diagnostic::render) with a caller-supplied [`LineColProvider`]
    /// answering the line and column lookups.
    ///
    /// Pass a [`LineIndexCache`] kept across renders, so that each source is indexed only
    /// once however many diagnostics are rendered from it, or an editor's own incremental
    /// line table.
    pub fn render_with(&self, line_cols: &mut impl LineColProvider<O>) -> String {
        render_report(line_cols, &self.to_string(), &self.span, &self.frames)
    }
}

impl<O: SourceOrigin> fmt::Display for Diagnostic<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity, self.data)
    }
}

/// The [`Diagnostic`]s recorded during one parse, in the order the parse reported them.
///
/// This is the type of the [`diagnostics`](crate::core::ParseResult::diagnostics) field
/// of a [`ParseResult`](crate::core::ParseResult), and is where a caller looks after a
/// tolerant parse. Ask [`has_errors`](Diagnostics::has_errors) whether anything serious
/// happened, [`iter`](Diagnostics::iter) over the diagnostics, print them all with
/// [`render_all`](Diagnostics::render_all), and use
/// [`sorted_by_position`](Diagnostics::sorted_by_position) for a report that reads along
/// the document instead of following the parse.
///
/// To find specific problems, [`conditions`](Diagnostics::conditions) yields the
/// condition payloads of one concrete type, and
/// [`with_identifier`](Diagnostics::with_identifier) selects by identifier string.
///
/// # Bounded retention
///
/// At most [`limit`](Diagnostics::limit) diagnostics are stored — the
/// [`DEFAULT_LIMIT`](Diagnostics::DEFAULT_LIMIT), unless the collection was created with
/// [`with_limit`](Diagnostics::with_limit).
///
/// The cap exists because tolerant parsing of degenerate input can produce one diagnostic
/// per byte, which would otherwise mean unbounded allocation.
///
/// Diagnostics pushed beyond the cap are dropped, but not forgotten: they are counted by
/// [`suppressed`](Diagnostics::suppressed), reported by
/// [`render_all`](Diagnostics::render_all) as an "… and N more" line, and error-severity
/// ones still make [`has_errors`](Diagnostics::has_errors) true.
///
/// # Deserialized diagnostics
///
/// Diagnostics read back through [`techy::serialize`](crate::serialize), inside a
/// deserialized parse result, have lost their original Rust types and carry
/// [`DeserializedCondition`](crate::serialize::DeserializedCondition)s. Match those with
/// [`with_identifier`](Diagnostics::with_identifier); a
/// [`conditions::<T>()`](Diagnostics::conditions) call yields nothing for them.
#[derive(Debug, Clone)]
pub struct Diagnostics<O: SourceOrigin = Option<String>> {
    items: Vec<Diagnostic<O>>,
    /// Maximum number of retained diagnostics.
    limit: usize,
    /// Diagnostics pushed beyond `limit` (counted, not stored).
    suppressed: usize,
    /// Error-severity pushes, retained *and* suppressed — keeps `has_errors` truthful
    /// when errors arrive after the cap.
    error_count: usize,
}

impl<O: SourceOrigin> Diagnostics<O> {
    /// The retention cap a collection gets when none is specified.
    ///
    /// It is generous for any human- or tool-facing report, and small enough that
    /// degenerate tolerant-mode input cannot exhaust memory. Choose a different one with
    /// [`with_limit`](Diagnostics::with_limit).
    pub const DEFAULT_LIMIT: usize = 1000;

    /// Creates an empty collection with the
    /// [`DEFAULT_LIMIT`](Diagnostics::DEFAULT_LIMIT) retention cap.
    pub fn new() -> Self {
        Diagnostics::with_limit(Diagnostics::<O>::DEFAULT_LIMIT)
    }

    /// Creates an empty collection retaining at most `limit` diagnostics.
    ///
    /// Pushes beyond the cap are counted as [`suppressed`](Diagnostics::suppressed)
    /// instead of being stored. The collection a parse fills is created for you; set its
    /// cap through
    /// [`ParseDriver::diagnostics_limit`](crate::core::ParseDriver::diagnostics_limit).
    ///
    /// A limit above `i64::MAX` — `usize::MAX`, meaning "no cap" — is accepted here, but
    /// a parse result holding such a collection cannot be serialized, because the
    /// serialized form's integers are `i64`. See
    /// [`ParseResultSerialization`](crate::serialize::ParseResultSerialization).
    pub fn with_limit(limit: usize) -> Self {
        Diagnostics { items: Vec::new(), limit, suppressed: 0, error_count: 0 }
    }

    /// Assembles a collection from the parts a serialized collection recorded — the
    /// entry point for reading one back.
    ///
    /// The parts are what a collection reports about itself: `items` are the diagnostics
    /// it stored, in the order [`iter`](Diagnostics::iter) yields them; `limit` is the
    /// retention cap they were stored under ([`limit`](Diagnostics::limit));
    /// `suppressed` is the number of pushes dropped beyond that cap
    /// ([`suppressed`](Diagnostics::suppressed)); and `error_count` is the number of
    /// error-severity pushes in all, stored and suppressed together
    /// ([`error_count`](Diagnostics::error_count)).
    ///
    /// Use this to rebuild a collection from a serialized form — the one
    /// [`techy::serialize`](crate::serialize) writes, a wire format of your own, or a
    /// language binding handing the counts back. A collection filled diagnostic by
    /// diagnostic is built with [`with_limit`](Diagnostics::with_limit) and
    /// [`push`](Diagnostics::push) instead.
    ///
    /// # Errors
    ///
    /// The parts must satisfy the invariants [`push`](Diagnostics::push) maintains, and
    /// [`InconsistentDiagnosticCounts`] is returned when they do not:
    ///
    /// - `items.len() <= limit`;
    /// - `suppressed > 0` only when `items.len() == limit`;
    /// - `error_count` is at least the number of error-severity `items`, and at most that
    ///   number plus `suppressed`.
    pub fn from_parts(
        items: Vec<Diagnostic<O>>,
        limit: usize,
        suppressed: usize,
        error_count: usize,
    ) -> Result<Self, InconsistentDiagnosticCounts> {
        let retained_errors =
            items.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).count();
        let consistent = items.len() <= limit
            && (suppressed == 0 || items.len() == limit)
            && retained_errors <= error_count
            && error_count <= retained_errors.saturating_add(suppressed);
        if !consistent {
            return Err(InconsistentDiagnosticCounts {
                retained: items.len(),
                retained_errors,
                limit,
                suppressed,
                error_count,
            });
        }
        Ok(Diagnostics { items, limit, suppressed, error_count })
    }

    /// Appends a diagnostic.
    ///
    /// Once [`limit`](Diagnostics::limit) diagnostics are stored, further ones are
    /// dropped and only counted: they raise [`suppressed`](Diagnostics::suppressed), and
    /// an error-severity one still makes [`has_errors`](Diagnostics::has_errors) true.
    pub fn push(&mut self, diagnostic: Diagnostic<O>) {
        if diagnostic.severity == Severity::Error {
            self.error_count += 1;
        }
        if self.items.len() < self.limit {
            self.items.push(diagnostic);
        } else {
            self.suppressed += 1;
        }
    }

    /// The number of diagnostics stored, not counting
    /// [`suppressed`](Diagnostics::suppressed) ones.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns whether the parse reported nothing at all — nothing stored, and nothing
    /// suppressed.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.suppressed == 0
    }

    /// The retention cap this collection was created with, beyond which diagnostics are
    /// counted rather than stored.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The number of diagnostics dropped because the retention cap was reached.
    pub fn suppressed(&self) -> usize {
        self.suppressed
    }

    /// Returns whether the parse reported any error-severity diagnostic, stored or
    /// suppressed.
    ///
    /// This is the check to make on a tolerant parse before treating its tree as a clean
    /// parse of the input.
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// The number of error-severity diagnostics reported, stored and suppressed
    /// together — the count [`has_errors`](Diagnostics::has_errors) tests against zero.
    pub fn error_count(&self) -> usize {
        self.error_count
    }

    /// Iterates over the stored diagnostics in the order the parse recorded them.
    ///
    /// For document order, use
    /// [`sorted_by_position`](Diagnostics::sorted_by_position).
    pub fn iter(&self) -> core::slice::Iter<'_, Diagnostic<O>> {
        self.items.iter()
    }

    /// The diagnostics whose condition answers the given
    /// [`identifier`](Diagnostic::identifier), in recording order.
    ///
    /// Take the identifier from the condition type (`T::IDENTIFIER`) rather than writing
    /// it as a literal. This is the way to select
    /// [deserialized diagnostics](Diagnostics#deserialized-diagnostics), whose original
    /// types are gone; within one process, [`conditions`](Diagnostics::conditions) is the
    /// typed equivalent.
    pub fn with_identifier<'a>(
        &'a self,
        identifier: &'a str,
    ) -> impl Iterator<Item = &'a Diagnostic<O>> {
        self.items.iter().filter(move |d| d.identifier() == identifier)
    }

    /// The recorded conditions of concrete type `T`, in recording order.
    ///
    /// Each stored condition is downcast to `T` and the ones that are not a `T` are
    /// skipped. Iterate with [`iter`](Diagnostics::iter) instead when the span or
    /// severity is needed alongside the condition.
    pub fn conditions<T: DiagnosticInfo>(&self) -> impl Iterator<Item = &T> {
        self.items.iter().filter_map(|d| d.data().downcast_ref::<T>())
    }

    /// The stored diagnostics as a slice, in recording order.
    pub fn as_slice(&self) -> &[Diagnostic<O>] {
        &self.items
    }

    /// The stored diagnostics ordered by source position, for a report that reads along
    /// the document.
    ///
    /// A parse records diagnostics in the order it hits them, which nested descents and
    /// deferred recoveries can permute relative to the document. This view re-sorts them
    /// by source and then by span start, so that within each source they appear in
    /// document order.
    ///
    /// Sources themselves are ordered by first appearance in the recorded sequence, and
    /// matched by `Arc` identity. That is the only cross-source claim made: a document
    /// can be parsed from several sources at once (an `\input`-like inclusion attaches
    /// another one), and a total position order across separate sources has no meaning.
    ///
    /// The sort is stable, so diagnostics at equal positions keep their recording order.
    /// The collection itself is unchanged; the returned vector borrows from it.
    pub fn sorted_by_position(&self) -> Vec<&Diagnostic<O>> {
        let mut sources: Vec<&Arc<Source<O>>> = Vec::new();
        let mut keyed: Vec<(usize, usize, usize)> =
            Vec::with_capacity(self.items.len());
        for (arrival, diagnostic) in self.items.iter().enumerate() {
            let source = diagnostic.span().source();
            let source_index = sources
                .iter()
                .position(|known| Arc::ptr_eq(known, source))
                .unwrap_or_else(|| {
                    sources.push(source);
                    sources.len() - 1
                });
            keyed.push((source_index, diagnostic.span().start(), arrival));
        }
        // The arrival index is part of the key, so equal positions keep recovery
        // order (and the unstable sort is de-facto stable).
        keyed.sort_unstable();
        keyed.into_iter().map(|(_, _, arrival)| &self.items[arrival]).collect()
    }

    /// Renders every stored diagnostic into one human-readable report.
    ///
    /// The report holds the [`Diagnostic::render`] blocks in recording order, separated
    /// by blank lines, and ends with an "… and N more" line when diagnostics were
    /// [`suppressed`](Diagnostics::suppressed) beyond the retention cap.
    ///
    /// This is the call to use for a whole collection. All positions are resolved through
    /// one [`LineIndexCache`], created for this call, which indexes each distinct source
    /// once (sources matched by `Arc` identity); calling
    /// [`render`](Diagnostic::render) in a loop instead rebuilds the line index for every
    /// diagnostic. Provenance chains are resolved through the same cache, so each
    /// including document is also indexed once.
    ///
    /// Use [`render_all_with`](Diagnostics::render_all_with) to supply a cache that
    /// persists across reports.
    pub fn render_all(&self) -> String {
        self.render_all_with(&mut LineIndexCache::new())
    }

    /// [`render_all`](Diagnostics::render_all) with a caller-supplied
    /// [`LineColProvider`] answering the line and column lookups.
    ///
    /// Pass a [`LineIndexCache`] kept across reports, or an editor's own incremental line
    /// table.
    pub fn render_all_with(&self, line_cols: &mut impl LineColProvider<O>) -> String {
        let mut out = String::new();
        for (i, diagnostic) in self.items.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            out.push_str(&render_report(
                line_cols,
                &diagnostic.to_string(),
                &diagnostic.span,
                &diagnostic.frames,
            ));
        }
        if self.suppressed > 0 {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&format!(
                "… and {} more diagnostics (retention limit of {} reached)",
                self.suppressed, self.limit,
            ));
        }
        out
    }
}

// Deliberately no `Extend`/`FromIterator` yet (no merging consumer exists). A future
// impl must merge `suppressed` and `error_count` alongside `items` — plain item
// concatenation would silently launder suppressed errors.
impl<O: SourceOrigin> Default for Diagnostics<O> {
    fn default() -> Self {
        Diagnostics::new()
    }
}

impl<O: SourceOrigin> IntoIterator for Diagnostics<O> {
    type Item = Diagnostic<O>;
    type IntoIter = alloc::vec::IntoIter<Diagnostic<O>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, O: SourceOrigin> IntoIterator for &'a Diagnostics<O> {
    type Item = &'a Diagnostic<O>;
    type IntoIter = core::slice::Iter<'a, Diagnostic<O>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// The failure of [`Diagnostics::from_parts`]: the parts given contradict one another,
/// so no collection filled through [`Diagnostics::push`] could have produced them.
///
/// The fields report the parts as they were given: `retained` diagnostics were supplied,
/// `retained_errors` of them of error severity, under a retention cap of `limit`, with
/// `suppressed` pushes dropped beyond the cap and `error_count` error-severity pushes in
/// all. The invariants they must satisfy are listed on
/// [`from_parts`](Diagnostics::from_parts).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InconsistentDiagnosticCounts {
    /// The number of diagnostics supplied.
    pub retained: usize,
    /// How many of them have error severity.
    pub retained_errors: usize,
    /// The retention cap supplied.
    pub limit: usize,
    /// The supplied number of pushes dropped beyond the cap.
    pub suppressed: usize,
    /// The supplied number of error-severity pushes.
    pub error_count: usize,
}

// Hand-written wording: the counts need number agreement, which a message format
// string cannot express, and the closing clause names the invariant that failed.
impl fmt::Display for InconsistentDiagnosticCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let InconsistentDiagnosticCounts {
            retained,
            retained_errors,
            limit,
            suppressed,
            error_count,
        } = *self;
        write!(
            f,
            "the diagnostics collection's counts are inconsistent: {retained} diagnostic{} \
             supplied ({retained_errors} of error severity) under a retention limit of \
             {limit}, with {suppressed} suppressed and {error_count} error{} in all — ",
            if retained == 1 { "" } else { "s" },
            if error_count == 1 { "" } else { "s" },
        )?;
        if retained > limit {
            write!(f, "more diagnostics were supplied than the retention limit allows")
        } else if suppressed > 0 && retained != limit {
            write!(
                f,
                "diagnostics were suppressed although the retention limit was not reached"
            )
        } else if retained_errors > error_count {
            write!(f, "more of the supplied diagnostics are errors than were reported in all")
        } else {
            write!(
                f,
                "more errors were reported in all than the supplied errors plus the \
                 suppressed diagnostics can account for"
            )
        }
    }
}

impl core::error::Error for InconsistentDiagnosticCounts {}

/// Whether a parse aborts at the first problem or records it and carries on.
///
/// This is the parse-wide policy, set on the parse driver — for the preset, as the
/// argument of [`LatexlikeDriver::new`](crate::latexlike::LatexlikeDriver::new) — and
/// applied wherever a construct parser reports a condition.
///
/// The two policies are described in full under
/// [tolerant parsing](crate::error#tolerant-parsing); the guide chapter
/// [Running the parser](crate::guide::parsing#strict-versus-tolerant) shows both in
/// action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Abort at the first problem: it is returned as a [`ParseError`], and no tree is
    /// produced.
    Strict,
    /// Record each problem as a [`Diagnostic`] and continue parsing.
    ///
    /// The parser repairs the input where the problem was detected, as that condition
    /// type's documentation describes — at the token level, by continuing from the
    /// recovery token the error carries. The parse succeeds and returns a best-effort
    /// tree alongside the diagnostics.
    Tolerant,
}

/// The error that ends a parse: the `Err` of
/// [`Language::parse`](crate::core::Language::parse).
///
/// It arises in two situations: a problem reported under [`Recovery::Strict`], which is
/// escalated instead of recorded, and a condition no policy can absorb — a failure in
/// consumer-supplied code ([`HookFailed`]) or a contract violation by an extension
/// ([`ImplementationError`](crate::core::constructs::ImplementationError)), both of which
/// abort even a tolerant parse.
///
/// **An `Err` always means the parse stopped.** Nothing continues past a `ParseError`: it
/// carries no recovery payload, and propagates out of every construct parser it passes
/// through. Recovery, when it happens, happens where the problem was detected, through
/// [`ParseContext::recover`](crate::core::constructs::ParseContext::recover); a sub-parse
/// that merely ended early reports that as ordinary data
/// ([`StopCause`](crate::core::constructs::StopCause)), not as an error.
///
/// A `ParseError` holds the same three things a [`Diagnostic`] does — the condition
/// ([`data`](ParseError::data)), the `Arc`-based [`SourceSpan`]
/// ([`span`](ParseError::span)), and the traceback ([`frames`](ParseError::frames)) — so
/// it is self-contained and outlives the parse. [`render`](ParseError::render) formats
/// all of it as a report.
///
/// `ParseError` deliberately does not implement `PartialEq`, since conditions are
/// compared through a trait object that hides their type. Compare
/// [`identifier()`](ParseError::identifier) and the fields of the downcast condition
/// instead.
#[derive(Debug, Clone)]
pub struct ParseError<O: SourceOrigin = Option<String>> {
    data: Box<dyn DiagnosticData>,
    span: SourceSpan<O>,
    /// Traceback snapshot, innermost first.
    frames: Vec<TraceFrame<O>>,
}

impl<O: SourceOrigin> ParseError<O> {
    /// Creates a parse error from a condition and the span it occurred at.
    ///
    /// The traceback is empty. A parse attaches frames through the recovery entry point,
    /// [`ParseContext::recover`](crate::core::constructs::ParseContext::recover), or
    /// explicitly with [`with_frames`](ParseError::with_frames).
    pub fn new(condition: impl DiagnosticInfo, span: SourceSpan<O>) -> ParseError<O> {
        ParseError { data: Box::new(condition), span, frames: Vec::new() }
    }

    /// Creates a parse error from a token error's kind — how a tokenization failure
    /// becomes a parse abort.
    ///
    /// A built-in token condition is stored as the parse error's condition; a
    /// [`TokenErrorKind::Custom`] payload is
    /// unwrapped and stored directly, rather than boxed a second time, so downcasting
    /// finds the original condition type either way.
    pub fn from_token_error(kind: TokenErrorKind, span: SourceSpan<O>) -> ParseError<O> {
        ParseError { data: kind.into_condition(), span, frames: Vec::new() }
    }

    /// Assembles a parse error from parts already prepared by the recovery path: a
    /// condition boxed after the driver's refinement pass, and a frame snapshot.
    pub(crate) fn from_parts(
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<O>,
        frames: Vec<TraceFrame<O>>,
    ) -> ParseError<O> {
        ParseError { data, span, frames }
    }

    /// Attaches a traceback snapshot, replacing any frames already stored.
    ///
    /// This is for the sites that abort directly instead of going through the recovery
    /// entry point; the frames normally come from
    /// [`ParserSession::snapshot_frames`](crate::core::ParserSession::snapshot_frames).
    /// Inside a construct parser, use
    /// [`ParseContext::attach_hook_frames`](crate::core::constructs::ParseContext::attach_hook_frames)
    /// instead: it snapshots and attaches a hook-returned error's frames in one call.
    pub fn with_frames(mut self, frames: Vec<TraceFrame<O>>) -> ParseError<O> {
        self.frames = frames;
        self
    }

    /// The condition that was reported.
    ///
    /// Downcast it with
    /// [`downcast_ref`](trait.DiagnosticData.html#method.downcast_ref) to read the
    /// condition's own fields.
    pub fn data(&self) -> &dyn DiagnosticData {
        &*self.data
    }

    /// The condition's identifier string, for boundaries where Rust types cannot travel.
    ///
    /// For every ordinary condition this is the type's
    /// [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) constant; see
    /// [`DiagnosticInfo::identifier`] for the adapter-type exception. Compare it against
    /// `T::IDENTIFIER` rather than against a string literal.
    pub fn identifier(&self) -> &str {
        self.data.identifier()
    }

    /// The human-readable message, rendered from the condition's `Display`.
    ///
    /// This allocates a fresh `String` on each call; the condition is the stored value,
    /// the message is derived from it.
    pub fn message(&self) -> String {
        self.data.to_string()
    }

    /// Where in the source the condition occurred.
    pub fn span(&self) -> &SourceSpan<O> {
        &self.span
    }

    /// The parse frames that were open at the moment of the abort, innermost first.
    ///
    /// Empty when the abort happened outside a parse descent. Format them with
    /// [`format_traceback`].
    pub fn frames(&self) -> &[TraceFrame<O>] {
        &self.frames
    }

    /// Renders a human-readable, multi-line report of this error: message, position,
    /// traceback, and provenance chain, as [`Diagnostic::render`] does.
    ///
    /// Line and column numbers are computed through a [`LineIndexCache`] created for this
    /// call and dropped with it; [`render_with`](ParseError::render_with) takes a
    /// persistent one.
    pub fn render(&self) -> String {
        self.render_with(&mut LineIndexCache::new())
    }

    /// [`render`](ParseError::render) with a caller-supplied [`LineColProvider`]
    /// answering the line and column lookups, like [`Diagnostic::render_with`].
    pub fn render_with(&self, line_cols: &mut impl LineColProvider<O>) -> String {
        render_report(line_cols, &format!("error: {}", self.data), &self.span, &self.frames)
    }
}

impl<O: SourceOrigin> fmt::Display for ParseError<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.data, f)
    }
}

impl<O: SourceOrigin> core::error::Error for ParseError<O> {}

/// Condition: consumer-supplied code the library called reported that it failed to do
/// its own work.
///
/// The code in question is a hook — a [`ParseDriver`](crate::core::ParseDriver) method, a
/// [`Lang`](crate::core::Lang) hook, a descent-state callback such as
/// [`GroupChildState::Compute`](crate::core::constructs::GroupChildState::Compute) — and
/// the failure is operational: an input/output failure, or a runtime failure inside an
/// embedding, such as an exception raised by code behind a language binding. The hook
/// returns it on a [`ParseError`], and the parse aborts whatever the recovery policy is.
///
/// # Choosing this condition
///
/// A failing hook has three distinct things it can say, and this is one of them:
///
/// - **`HookFailed`** — the hook's own code failed while doing its work. It says nothing
///   about the document, and nothing about library contracts.
/// - [`ImplementationError`](crate::core::constructs::ImplementationError) — the
///   extension violated a library contract. That is a bug to fix in the extension, not an
///   operational failure.
/// - Any other condition type — a problem **in the parsed document**. Report it through
///   the recovery entry point,
///   [`ParseContext::recover`](crate::core::constructs::ParseContext::recover), wherever
///   the hook's signature allows, so that a tolerant parse can record it and continue.
///
/// # The underlying error
///
/// [`cause`](HookFailed::cause) optionally holds the error that caused the failure,
/// shared behind an `Arc` — the same shape
/// [`ResolveError`](crate::source::ResolveError) uses. The `Arc` is what keeps this
/// condition `Clone`: clones share one cause by reference count.
///
/// A consumer can walk the chain from that field, through
/// [`Error::source`](core::error::Error::source), or downcast it to the concrete error
/// type. The serialized projection renders the `detail` string and the cause chain as
/// text.
#[derive(Debug, Clone, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.hooks.hook-failed",
    message = "extension hook reported a failure: {detail}"
)]
pub struct HookFailed {
    /// Human-readable description of the failure.
    pub detail: String,
    /// The underlying error, if the hook has one to attach.
    ///
    /// `None` when the `detail` string says everything. Attach one through
    /// [`new`](HookFailed::new)'s second argument, or with
    /// [`with_cause`](HookFailed::with_cause).
    pub cause: Option<Arc<dyn core::error::Error + Send + Sync + 'static>>,
}

impl HookFailed {
    /// Attaches the underlying error, storing it on the
    /// [`cause`](HookFailed::cause) field.
    ///
    /// The `detail` string stays the rendered summary. Unlike
    /// [`new`](HookFailed::new)'s second argument, this takes the concrete error by value
    /// and wraps it in the `Arc` itself, which is what a not-yet-shared error value
    /// needs.
    pub fn with_cause(
        mut self,
        cause: impl core::error::Error + Send + Sync + 'static,
    ) -> Self {
        self.cause = Some(Arc::new(cause));
        self
    }
}

/// The shared body of the `render`/`render_with` family: headline, position, traceback,
/// and provenance chain, with every line/column lookup answered by `line_cols`.
///
/// Routing all lookups through one provider is what makes
/// [`Diagnostics::render_all`] cost O(N + k) rather than O(k·N): one line-starts table
/// per distinct source, however many positions ask for one.
fn render_report<O: SourceOrigin>(
    line_cols: &mut impl LineColProvider<O>,
    headline: &str,
    span: &SourceSpan<O>,
    frames: &[TraceFrame<O>],
) -> String {
    let mut msg = String::from(headline);
    msg.push_str("\n  at: ");
    msg.push_str(&format_position_with(span, line_cols));

    let traceback = format_traceback_with(frames, line_cols);
    if !traceback.is_empty() {
        msg.push('\n');
        msg.push_str(&traceback);
    }

    for provenance in span.source().provenance_chain() {
        match provenance {
            SourceProvenance::Primary => {}
            SourceProvenance::Resolved { reference, triggered_at } => {
                msg.push_str(&format!(
                    "\n  included from {} ({})",
                    format_position_with(triggered_at, line_cols),
                    reference,
                ));
            }
            SourceProvenance::Synthesized { description, triggered_at } => {
                msg.push_str(&format!(
                    "\n  synthesized from {} ({})",
                    format_position_with(triggered_at, line_cols),
                    description,
                ));
            }
        }
    }

    msg
}

/// Formats the start of a span for display, as `@ (line 2, col 5) [origin]`.
///
/// The `[origin]` part is omitted when the source's origin has no label.
///
/// When line and column information is unavailable for the position — the content lies
/// past the [`LineIndexCache`] scan cap, for instance — the result falls back to the raw
/// byte position, as `@ char pos 42 (no line info)`.
///
/// This builds a [`LineIndexCache`] for the call and drops it again. Pass a persistent one
/// to [`format_position_with`], or render a whole collection with
/// [`Diagnostics::render_all`], which shares a single cache across all positions.
pub fn format_position<O: SourceOrigin>(span: &SourceSpan<O>) -> String {
    format_position_with(span, &mut LineIndexCache::new())
}

/// [`format_position`] with a caller-supplied [`LineColProvider`] answering the line and
/// column lookup — a persistent [`LineIndexCache`], or an editor's own incremental line
/// table.
pub fn format_position_with<O: SourceOrigin>(
    span: &SourceSpan<O>,
    line_cols: &mut impl LineColProvider<O>,
) -> String {
    let source = span.source();
    let pos_str = match line_cols.line_col(source, span.start()) {
        Some((line, col)) => format!("@ (line {}, col {})", line, col),
        None => format!("@ char pos {} (no line info)", span.start()),
    };

    match source.origin().label() {
        Some(label) => format!("{} [{}]", pos_str, label),
        None => pos_str,
    }
}

/// Formats a traceback — the [`TraceFrame`]s of [`Diagnostic::frames`] or
/// [`ParseError::frames`], innermost first — as one line per open block.
///
/// Each frame's position and origin label come from its own source, so a traceback that
/// crosses an included document reads correctly. Returns an empty string when `frames` is
/// empty.
///
/// [`Diagnostic::render`] and [`ParseError::render`] already append this to their
/// reports; call it directly to format a traceback on its own. It builds a
/// [`LineIndexCache`] for the call and drops it again;
/// [`format_traceback_with`] takes a persistent one.
///
/// # Example output
///
/// ```text
/// Open blocks:
///   @ (line 8, col 1): environment ‘document’
///   @ (line 5, col 3): argument #1 of ‘\section’
/// ```
pub fn format_traceback<O: SourceOrigin>(frames: &[TraceFrame<O>]) -> String {
    format_traceback_with(frames, &mut LineIndexCache::new())
}

/// [`format_traceback`] with a caller-supplied [`LineColProvider`] answering the line and
/// column lookups — a persistent [`LineIndexCache`], or an editor's own incremental line
/// table.
pub fn format_traceback_with<O: SourceOrigin>(
    frames: &[TraceFrame<O>],
    line_cols: &mut impl LineColProvider<O>,
) -> String {
    if frames.is_empty() {
        return String::new();
    }

    let mut result = String::from("Open blocks:");
    for frame in frames {
        result.push_str("\n  ");
        result.push_str(&format_position_with(&frame.span, line_cols));
        result.push_str(": ");
        result.push_str(&frame.title);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use std::sync::Arc;

    /// A third-party-style condition — the extension surface demonstration: a plain
    /// data struct, a `Display` for the wording, and a `DiagnosticInfo` impl.
    /// Deliberately hand-written (no derive): this exercises the manual trait path and
    /// the defaulted `serializable_data()` (see
    /// `serializable_data_defaults_to_an_empty_map`).
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestCondition {
        detail: String,
    }

    impl TestCondition {
        fn new(detail: &str) -> TestCondition {
            TestCondition { detail: detail.to_string() }
        }
    }

    impl fmt::Display for TestCondition {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.detail)
        }
    }

    impl DiagnosticInfo for TestCondition {
        const IDENTIFIER: &'static str = "test.error.test-condition";
    }

    /// A second condition type, for filtering/downcast tests.
    #[derive(Debug, Clone, PartialEq, Eq, crate::error::DiagnosticInfo)]
    #[diagnostic(
        id = "test.error.other-condition",
        message = "other condition",
        no_constructor
    )]
    struct OtherCondition;

    /// A binding-adapter-shaped condition: one Rust type carrying conditions defined
    /// at runtime on the other side of an embedding boundary, overriding
    /// `identifier()` per instance — the exceptional case the method exists for.
    #[derive(Debug, Clone)]
    struct AdapterCondition {
        identifier: String,
        detail: String,
    }

    impl fmt::Display for AdapterCondition {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.detail)
        }
    }

    impl DiagnosticInfo for AdapterCondition {
        const IDENTIFIER: &'static str = "test.error.adapter";
        fn identifier(&self) -> &str {
            &self.identifier
        }
    }

    #[test]
    fn an_adapter_condition_reports_its_per_instance_identifier() {
        let source = arc_source("Hello");
        let diagnostic = Diagnostic::error(
            AdapterCondition {
                identifier: "pylang.custom.my-condition".to_string(),
                detail: "adapter-carried condition".to_string(),
            },
            SourceSpan::new(&source, 0..1),
        );
        // The stored instance answers its runtime identifier…
        assert_eq!(diagnostic.identifier(), "pylang.custom.my-condition");
        // …preserved across the dyn clone path…
        assert_eq!(diagnostic.clone().identifier(), "pylang.custom.my-condition");
        // …while the adapter type keeps its const and the in-process type identity.
        assert_eq!(AdapterCondition::IDENTIFIER, "test.error.adapter");
        let condition = diagnostic.data().downcast_ref::<AdapterCondition>().unwrap();
        assert_eq!(condition.detail, "adapter-carried condition");
    }

    #[test]
    fn ordinary_conditions_still_answer_their_const_identifier() {
        // The default `identifier()` body answers the const — shipped conditions and
        // `T::IDENTIFIER`-keyed matching are unaffected by the method's existence.
        let ordinary = TestCondition::new("x");
        assert_eq!(DiagnosticInfo::identifier(&ordinary), TestCondition::IDENTIFIER);
        let source = arc_source("Hello");
        let diagnostic =
            Diagnostic::error(TestCondition::new("x"), SourceSpan::new(&source, 0..1));
        assert_eq!(diagnostic.identifier(), TestCondition::IDENTIFIER);
    }

    fn origin(url: &str) -> Option<String> {
        Some(url.to_string())
    }

    fn arc_source(content: &str) -> Arc<Source> {
        Arc::new(Source::new(content))
    }

    #[test]
    fn diagnostic_render_includes_line_info() {
        let source = arc_source("Hello\n\\unknown\nworld");
        let span = SourceSpan::new(&source, 6..14);
        let diagnostic = Diagnostic::error(TestCondition::new("unknown macro"), span);

        let rendered = diagnostic.render();
        assert!(rendered.contains("error: unknown macro"));
        assert!(rendered.contains("line 2"));
    }

    #[test]
    fn sorted_by_position_is_source_major_span_start_minor_and_stable() {
        let first = arc_source("first source content");
        let second = arc_source("second source content");
        let mut diagnostics: Diagnostics = Diagnostics::new();
        // Recovery order deliberately scrambled: positions out of order within
        // each source, sources interleaved.
        for (source, range, tag) in [
            (&first, 10..12, "f10"),
            (&second, 5..6, "s5"),
            (&first, 0..2, "f0"),
            (&first, 10..11, "f10-later"), // equal start: keeps recovery order
            (&second, 0..1, "s0"),
        ] {
            diagnostics.push(Diagnostic::error(
                TestCondition::new(tag),
                SourceSpan::new(source, range),
            ));
        }

        let tags: Vec<String> = diagnostics
            .sorted_by_position()
            .into_iter()
            .map(|diagnostic| diagnostic.message())
            .collect();
        // `first` appeared first: its diagnostics lead, in span order; the equal
        // starts keep their recovery order (stable).
        assert_eq!(tags, ["f0", "f10", "f10-later", "s0", "s5"]);

        // The view borrows: the collection itself keeps recovery order.
        assert_eq!(diagnostics.iter().next().unwrap().message(), "f10");
    }

    #[test]
    fn diagnostic_accessors() {
        let source = arc_source("Hello\nWorld\nTest");
        let span = SourceSpan::new(&source, 10..15);
        let diagnostic = Diagnostic::error(TestCondition::new("test"), span);

        assert_eq!(diagnostic.span().start(), 10);
        assert_eq!(diagnostic.span().end(), 15);
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.message(), "test");
        assert!(diagnostic.frames().is_empty());
        // The two identities ([§dd-dr:errors]): the identifier string on the wire…
        assert_eq!(diagnostic.identifier(), TestCondition::IDENTIFIER);
        // …and the concrete type in-process, reached by downcast.
        assert!(diagnostic.data().is::<TestCondition>());
        let condition = diagnostic.data().downcast_ref::<TestCondition>().unwrap();
        assert_eq!(condition.detail, "test");
        assert!(diagnostic.data().downcast_ref::<OtherCondition>().is_none());
    }

    #[test]
    fn diagnostic_clone_preserves_the_payload() {
        let source = arc_source("Hello");
        let diagnostic =
            Diagnostic::error(TestCondition::new("boom"), SourceSpan::new(&source, 0..1));
        let clone = diagnostic.clone();
        assert_eq!(clone.identifier(), TestCondition::IDENTIFIER);
        assert_eq!(clone.data().downcast_ref::<TestCondition>().unwrap().detail, "boom");
    }

    #[test]
    fn serializable_data_defaults_to_an_empty_map() {
        let condition = TestCondition::new("x");
        assert_eq!(
            DiagnosticInfo::serializable_data(&condition),
            DiagnosticValue::empty_map()
        );
        let data: Box<dyn DiagnosticData> = Box::new(condition);
        assert_eq!(data.serializable_data(), DiagnosticValue::Map(Vec::new()));
    }

    #[test]
    fn diagnostic_render_with_origin() {
        let source: Arc<Source> = Arc::new(
            Source::new("Hello\n\\unknown\nworld").with_origin(origin("test.tex")),
        );
        let span = SourceSpan::new(&source, 6..14);
        let diagnostic = Diagnostic::error(TestCondition::new("unknown macro"), span);

        let rendered = diagnostic.render();
        assert!(rendered.contains("line 2"));
        assert!(rendered.contains("[test.tex]"));
    }

    #[test]
    fn diagnostic_render_walks_provenance_chain() {
        let document: Arc<Source> = Arc::new(
            Source::new(r"\input{main.tex}").with_origin(origin("document.tex")),
        );
        let main: Arc<Source> = Arc::new(Source::resolved(
            "\\mycommand\n",
            "main.tex",
            SourceSpan::entire(&document),
        ));
        let expanded: Arc<Source> = Arc::new(Source::synthesized(
            "expansion text",
            "macro expansion",
            SourceSpan::new(&main, 0..10),
        ));

        let diagnostic =
            Diagnostic::error(TestCondition::new("boom"), SourceSpan::new(&expanded, 0..9));
        let rendered = diagnostic.render();

        assert!(rendered.contains("error: boom"));
        assert!(rendered.contains("synthesized from"));
        assert!(rendered.contains("(macro expansion)"));
        assert!(rendered.contains("included from"));
        assert!(rendered.contains("(main.tex)"));
        assert!(rendered.contains("[document.tex]"));
    }

    #[test]
    fn diagnostics_collection() {
        let source = arc_source("Hello");
        let mut diagnostics: Diagnostics = Diagnostics::new();
        assert!(diagnostics.is_empty());
        assert!(!diagnostics.has_errors());

        diagnostics.push(Diagnostic::warning(
            TestCondition::new("odd"),
            SourceSpan::new(&source, 0..1),
        ));
        assert!(!diagnostics.has_errors());

        diagnostics.push(Diagnostic::error(
            TestCondition::new("bad"),
            SourceSpan::new(&source, 1..2),
        ));
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.has_errors());

        let severities: Vec<Severity> = diagnostics.iter().map(|d| d.severity()).collect();
        assert_eq!(severities, vec![Severity::Warning, Severity::Error]);
    }

    #[test]
    fn diagnostics_identifier_filtering_and_typed_access() {
        let source = arc_source("Hello");
        let mut diagnostics: Diagnostics = Diagnostics::new();
        diagnostics
            .push(Diagnostic::error(TestCondition::new("a"), SourceSpan::new(&source, 0..1)));
        diagnostics.push(Diagnostic::error(OtherCondition, SourceSpan::new(&source, 1..2)));
        diagnostics
            .push(Diagnostic::error(TestCondition::new("b"), SourceSpan::new(&source, 2..3)));

        assert_eq!(diagnostics.with_identifier(TestCondition::IDENTIFIER).count(), 2);
        assert_eq!(diagnostics.with_identifier(OtherCondition::IDENTIFIER).count(), 1);
        assert_eq!(diagnostics.with_identifier("test.error.nope").count(), 0);

        let details: Vec<&str> = diagnostics
            .conditions::<TestCondition>()
            .map(|c| c.detail.as_str())
            .collect();
        assert_eq!(details, ["a", "b"]);
    }

    #[test]
    fn diagnostics_default_limit_applies() {
        let diagnostics: Diagnostics = Diagnostics::new();
        assert_eq!(diagnostics.limit(), Diagnostics::<Option<String>>::DEFAULT_LIMIT);
        assert_eq!(diagnostics.suppressed(), 0);
    }

    #[test]
    fn diagnostics_limit_counts_suppressed_pushes() {
        let source = arc_source("abcd");
        let mut diagnostics: Diagnostics = Diagnostics::with_limit(2);
        for i in 0..4 {
            diagnostics.push(Diagnostic::error(
                TestCondition::new("boom"),
                SourceSpan::new(&source, i..i + 1),
            ));
        }

        // Two retained, two counted; the collection is decidedly not empty.
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics.suppressed(), 2);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.has_errors());
        assert_eq!(diagnostics.iter().count(), 2);

        let rendered = diagnostics.render_all();
        assert!(rendered
            .contains("… and 2 more diagnostics (retention limit of 2 reached)"));
    }

    #[test]
    fn has_errors_sees_errors_beyond_the_limit() {
        let source = arc_source("ab");
        let mut diagnostics: Diagnostics = Diagnostics::with_limit(1);
        diagnostics.push(Diagnostic::warning(
            TestCondition::new("odd"),
            SourceSpan::new(&source, 0..1),
        ));
        assert!(!diagnostics.has_errors());

        // The error lands beyond the cap: dropped from storage, not from the verdict.
        diagnostics.push(Diagnostic::error(
            TestCondition::new("bad"),
            SourceSpan::new(&source, 1..2),
        ));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn from_parts_restores_a_collection_whose_diagnostics_were_dropped() {
        let source = arc_source("abcd");
        let mut original: Diagnostics = Diagnostics::with_limit(2);
        for i in 0..4 {
            original.push(Diagnostic::error(
                TestCondition::new("boom"),
                SourceSpan::new(&source, i..i + 1),
            ));
        }

        // The parts a serialized form records, handed straight back.
        let items: Vec<Diagnostic> = original.iter().cloned().collect();
        let rebuilt = Diagnostics::from_parts(
            items,
            original.limit(),
            original.suppressed(),
            original.error_count(),
        )
        .unwrap();

        assert_eq!(rebuilt.len(), original.len());
        assert_eq!(rebuilt.limit(), 2);
        assert_eq!(rebuilt.suppressed(), 2);
        assert_eq!(rebuilt.error_count(), 4);
        assert!(rebuilt.has_errors());
        assert_eq!(rebuilt.render_all(), original.render_all());
    }

    #[test]
    fn from_parts_rejects_counts_that_contradict_one_another() {
        let source = arc_source("ab");
        // Two diagnostics, one of them an error.
        let items: Vec<Diagnostic> = vec![
            Diagnostic::error(TestCondition::new("bad"), SourceSpan::new(&source, 0..1)),
            Diagnostic::warning(TestCondition::new("odd"), SourceSpan::new(&source, 1..2)),
        ];
        let refuse = |limit, suppressed, error_count| {
            Diagnostics::from_parts(items.clone(), limit, suppressed, error_count).unwrap_err()
        };

        // More diagnostics than the retention cap allows.
        assert_eq!(
            refuse(1, 0, 1),
            InconsistentDiagnosticCounts {
                retained: 2,
                retained_errors: 1,
                limit: 1,
                suppressed: 0,
                error_count: 1,
            }
        );
        // Suppressed pushes although the cap was not reached.
        assert_eq!(
            refuse(5, 1, 1),
            InconsistentDiagnosticCounts {
                retained: 2,
                retained_errors: 1,
                limit: 5,
                suppressed: 1,
                error_count: 1,
            }
        );
        // Fewer errors in all than there are error-severity diagnostics.
        assert_eq!(
            refuse(5, 0, 0),
            InconsistentDiagnosticCounts {
                retained: 2,
                retained_errors: 1,
                limit: 5,
                suppressed: 0,
                error_count: 0,
            }
        );
        // More errors in all than the retained errors plus the suppressed pushes.
        assert_eq!(
            refuse(2, 1, 3),
            InconsistentDiagnosticCounts {
                retained: 2,
                retained_errors: 1,
                limit: 2,
                suppressed: 1,
                error_count: 3,
            }
        );

        // Each refusal says which invariant failed.
        assert!(refuse(1, 0, 1).to_string().contains("more diagnostics were supplied"));
        assert!(refuse(5, 1, 1).to_string().contains("although the retention limit was not"));
        assert!(refuse(5, 0, 0).to_string().contains("are errors than were reported in all"));
        assert!(refuse(2, 1, 3).to_string().contains("more errors were reported in all"));

        // The counts agree in number with what they count.
        let one = vec![Diagnostic::error(TestCondition::new("bad"), SourceSpan::new(&source, 0..1))];
        let message = Diagnostics::from_parts(one, 0, 0, 1).unwrap_err().to_string();
        assert!(message.contains("1 diagnostic supplied"), "{message}");
        assert!(message.contains("1 error in all"), "{message}");
        assert!(refuse(1, 0, 1).to_string().contains("2 diagnostics supplied"));

        // And the consistent parts are accepted.
        assert!(Diagnostics::from_parts(items, 2, 1, 2).is_ok());
    }

    #[test]
    fn render_all_matches_the_per_diagnostic_renders() {
        // Two sources — one reached through a provenance hop — so the shared index
        // cache is exercised across sources; sharing must not change the output.
        let document: Arc<Source> = Arc::new(
            Source::new("Hello\n\\input{main.tex}").with_origin(origin("document.tex")),
        );
        let main: Arc<Source> = Arc::new(Source::resolved(
            "\\bad\nmore\n",
            "main.tex",
            SourceSpan::new(&document, 6..22),
        ));

        let mut diagnostics: Diagnostics = Diagnostics::new();
        diagnostics.push(Diagnostic::error(
            TestCondition::new("first"),
            SourceSpan::new(&main, 0..4),
        ));
        diagnostics.push(Diagnostic::warning(
            TestCondition::new("second"),
            SourceSpan::new(&main, 5..9),
        ));
        diagnostics.push(Diagnostic::error(
            TestCondition::new("third"),
            SourceSpan::new(&document, 0..5),
        ));

        let separate: Vec<String> = diagnostics.iter().map(|d| d.render()).collect();
        assert_eq!(diagnostics.render_all(), separate.join("\n\n"));
        // No suppression footer when nothing was suppressed.
        assert!(!diagnostics.render_all().contains("… and"));
    }

    #[test]
    fn format_traceback_single_frame() {
        let source = arc_source("Hello\nWorld\nTest");

        let frames =
            vec![TraceFrame::new("environment ‘document’", SourceSpan::new(&source, 6..11))];

        let traceback = format_traceback(&frames);
        assert_eq!(traceback, "Open blocks:\n  @ (line 2, col 1): environment ‘document’");
    }

    #[test]
    fn format_traceback_multiple_frames_innermost_first() {
        let source = arc_source("Hello\nWorld\nTest\nMore");

        let frames = vec![
            TraceFrame::new("callable ‘\\textbf’", SourceSpan::new(&source, 12..16)),
            TraceFrame::new("environment ‘document’", SourceSpan::new(&source, 6..11)),
            TraceFrame::new("callable ‘\\section’", SourceSpan::new(&source, 0..5)),
        ];

        let traceback = format_traceback(&frames);
        assert_eq!(
            traceback,
            "Open blocks:\n  @ (line 3, col 1): callable ‘\\textbf’\n  @ (line 2, col 1): environment ‘document’\n  @ (line 1, col 1): callable ‘\\section’"
        );
    }

    #[test]
    fn format_traceback_empty_frames() {
        let frames: Vec<TraceFrame> = vec![];
        assert_eq!(format_traceback(&frames), "");
    }

    #[test]
    fn format_traceback_with_origin() {
        let source: Arc<Source> = Arc::new(
            Source::new("Hello\nWorld\nTest").with_origin(origin("test.tex")),
        );

        let frames =
            vec![TraceFrame::new("environment ‘document’", SourceSpan::new(&source, 6..11))];

        let traceback = format_traceback(&frames);
        assert_eq!(
            traceback,
            "Open blocks:\n  @ (line 2, col 1) [test.tex]: environment ‘document’"
        );
    }

    #[test]
    fn render_appends_the_traceback() {
        let source = arc_source("Hello\nWorld");
        let diagnostic = Diagnostic::from_parts(
            Severity::Error,
            Box::new(TestCondition::new("boom")),
            SourceSpan::new(&source, 6..7),
            vec![TraceFrame::new("group ‘{’", SourceSpan::new(&source, 0..1))],
        );
        let rendered = diagnostic.render();
        assert!(rendered.contains("error: boom"));
        assert!(rendered.contains("Open blocks:\n  @ (line 1, col 1): group ‘{’"));
    }

    #[test]
    fn parse_error_carries_the_condition() {
        let source = arc_source("Hello");
        let err: ParseError =
            ParseError::new(TestCondition::new("boom"), SourceSpan::new(&source, 0..2));
        assert_eq!(err.identifier(), TestCondition::IDENTIFIER);
        assert_eq!(err.message(), "boom");
        assert_eq!(err.data().downcast_ref::<TestCondition>().unwrap().detail, "boom");
        assert_eq!(err.to_string(), "boom");
        assert!(err.frames().is_empty());
        assert!(err.render().contains("error: boom"));
        assert!(err.render().contains("line 1"));
    }

    // --- HookFailed: the general operational hook-failure condition ------------------

    /// The innermost hop of the test cause chain.
    #[derive(Debug)]
    struct UnderlyingIo;

    impl fmt::Display for UnderlyingIo {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("disk unavailable")
        }
    }

    impl core::error::Error for UnderlyingIo {}

    /// A two-hop cause chain: the embedding's failure, caused by `UnderlyingIo`.
    #[derive(Debug)]
    struct EmbeddingFailure {
        source: UnderlyingIo,
    }

    impl fmt::Display for EmbeddingFailure {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("embedding call failed")
        }
    }

    impl core::error::Error for EmbeddingFailure {
        fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    #[test]
    fn hook_failed_constructs_and_renders() {
        let plain = HookFailed::new("state seed unavailable", None);
        assert_eq!(HookFailed::IDENTIFIER, "core.hooks.hook-failed");
        assert_eq!(DiagnosticInfo::identifier(&plain), HookFailed::IDENTIFIER);
        assert_eq!(
            plain.to_string(),
            "extension hook reported a failure: state seed unavailable"
        );
        assert!(plain.cause.is_none());

        // The derive's constructor also takes the cause positionally (an
        // already-shared `Arc`); `with_cause` is the by-value sugar.
        let arc: Arc<dyn core::error::Error + Send + Sync> = Arc::new(UnderlyingIo);
        let positional = HookFailed::new("boom", Some(arc));
        assert_eq!(positional.cause.as_ref().unwrap().to_string(), "disk unavailable");
    }

    #[test]
    fn hook_failed_cause_chain_is_reachable_through_a_parse_error() {
        let source = arc_source("Hello");
        let condition = HookFailed::new("embedding call failed", None)
            .with_cause(EmbeddingFailure { source: UnderlyingIo });
        let err: ParseError =
            ParseError::new(condition, SourceSpan::new(&source, 0..1));
        assert_eq!(err.identifier(), "core.hooks.hook-failed");

        // Downcast to the concrete condition, then walk the chain off the field.
        let condition = err.data().downcast_ref::<HookFailed>().unwrap();
        let cause = condition.cause.as_ref().unwrap();
        assert_eq!(cause.to_string(), "embedding call failed");
        let hop = core::error::Error::source(&**cause).unwrap();
        assert_eq!(hop.to_string(), "disk unavailable");
        assert!(hop.source().is_none());

        // Clones share the cause by reference count (the `Arc` pattern).
        let clone = condition.clone();
        assert!(Arc::ptr_eq(cause, clone.cause.as_ref().unwrap()));
    }

    #[test]
    fn hook_failed_serializable_data_projects_the_cause_chain() {
        let plain = HookFailed::new("boom", None);
        assert_eq!(
            DiagnosticInfo::serializable_data(&plain),
            DiagnosticValue::Map(vec![
                ("detail".to_string(), DiagnosticValue::Str("boom".to_string())),
                ("cause".to_string(), DiagnosticValue::Null),
            ])
        );

        let chained =
            HookFailed::new("boom", None).with_cause(EmbeddingFailure { source: UnderlyingIo });
        assert_eq!(
            DiagnosticInfo::serializable_data(&chained),
            DiagnosticValue::Map(vec![
                ("detail".to_string(), DiagnosticValue::Str("boom".to_string())),
                (
                    "cause".to_string(),
                    DiagnosticValue::List(vec![
                        DiagnosticValue::Str("embedding call failed".to_string()),
                        DiagnosticValue::Str("disk unavailable".to_string()),
                    ])
                ),
            ])
        );
    }

    #[test]
    fn the_with_variants_match_their_transient_shorthands() {
        // The shorthand-not-second-path contract: a caller-held persistent cache
        // produces byte-identical reports, across repeated renders (entries are
        // reused, never rebuilt — content is immutable).
        let document: Arc<Source> = Arc::new(
            Source::new("Hello\n\\input{main.tex}").with_origin(origin("document.tex")),
        );
        let main: Arc<Source> = Arc::new(Source::resolved(
            "\\bad\nmore\n",
            "main.tex",
            SourceSpan::new(&document, 6..22),
        ));
        let mut diagnostics: Diagnostics = Diagnostics::new();
        diagnostics.push(Diagnostic::from_parts(
            Severity::Error,
            Box::new(TestCondition::new("boom")),
            SourceSpan::new(&main, 0..4),
            vec![TraceFrame::new("group ‘{’", SourceSpan::new(&document, 6..7))],
        ));
        let error: ParseError =
            ParseError::new(TestCondition::new("halt"), SourceSpan::new(&main, 5..9));

        let mut cache = crate::source::LineIndexCache::new();
        for _ in 0..2 {
            assert_eq!(diagnostics.render_all_with(&mut cache), diagnostics.render_all());
            let diagnostic = diagnostics.iter().next().unwrap();
            assert_eq!(diagnostic.render_with(&mut cache), diagnostic.render());
            assert_eq!(error.render_with(&mut cache), error.render());
            assert_eq!(
                format_position_with(diagnostic.span(), &mut cache),
                format_position(diagnostic.span())
            );
            assert_eq!(
                format_traceback_with(diagnostic.frames(), &mut cache),
                format_traceback(diagnostic.frames())
            );
        }
    }

    #[test]
    fn format_position_char_pos_fallback() {
        // A source too large for line indexing falls back to raw byte positions, with
        // a parenthetical noting that no line/column is available. The parenthetical
        // deliberately names no cause: a `None` from a `LineColProvider` can have any
        // provider-specific reason.
        let content = "a\n".repeat(DEFAULT_MAX_TEST_LEN);
        let source = arc_source(&content);
        let span = SourceSpan::new(&source, 42..43);

        let formatted = format_position(&span);
        assert_eq!(formatted, "@ char pos 42 (no line info)");
    }

    // Exceeds the default max scan length of 500_000 bytes ("a\n" is 2 bytes).
    const DEFAULT_MAX_TEST_LEN: usize = 300_000;

    /// The derive works *inside* the defining crate too (via `extern crate self as
    /// techy` in lib.rs) — the in-crate migration of the built-in conditions relies on
    /// this. The full derive surface is exercised from the consumer side in
    /// tests/derive_conditions.rs.
    #[derive(Debug, Clone, PartialEq, Eq, crate::error::DiagnosticInfo)]
    #[non_exhaustive]
    #[diagnostic(id = "test.error.derived-condition", message = "derived: {detail}")]
    struct DerivedCondition {
        detail: String,
    }

    #[test]
    fn derive_works_in_crate() {
        let condition = DerivedCondition::new("boom");
        assert_eq!(DerivedCondition::IDENTIFIER, "test.error.derived-condition");
        assert_eq!(condition.to_string(), "derived: boom");
        assert_eq!(
            DiagnosticInfo::serializable_data(&condition),
            DiagnosticValue::Map(vec![(
                "detail".to_string(),
                DiagnosticValue::Str("boom".to_string())
            )])
        );
    }
}
