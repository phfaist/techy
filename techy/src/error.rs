//! Span-based diagnostics, the tolerant-parsing policy, and the parse abort error
//! ([`ParseError`]).
//!
//! Diagnostics carry Arc-based [`SourceSpan`]s, so they are self-contained and outlive the
//! parse that produced them — no `'src` lifetime spreads through error signatures. Library
//! conditions are reported through diagnostics (accumulated by the parser session, available
//! on the parse result), never through a logging side channel.
//!
//! # Structured conditions
//!
//! [`Diagnostic`] and [`ParseError`] carry a structured **condition payload**
//! (`Box<dyn DiagnosticData>`) plus span and traceback frames — no message string, no kind
//! enum. The human message is a pure function of the payload (its `Display`); machine
//! consumers downcast to the concrete condition type (the in-process identity) or compare
//! [`identifier()`](DiagnosticData::identifier) strings (the wire identity, for boundaries
//! where types cannot go). Condition types are plain public-field data structs defined next
//! to the construct that detects them; third-party conditions are structurally identical
//! citizens — implement [`DiagnosticInfo`] on a data struct and it flows through the same
//! carriers.
//!
//! The token-level error type carrying a recovery token (`TokenError`) lives with `Token` in
//! the token layer; the [`Recovery`] policy that governs it is defined here, as is
//! [`ParseError`] — the construct-parsing abort, which deliberately carries **no**
//! recovery payload.

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

/// A structured parse condition, as implemented on a plain public-field data struct.
///
/// Implementors write a data struct (public fields, `#[non_exhaustive]` + constructor for
/// semver headroom, ordinary `Clone`/`Debug` derives), a `Display` impl for the human
/// wording — the message is a pure function of the payload — and this trait: the wire
/// [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) plus, optionally,
/// [`serializable_data`](DiagnosticInfo::serializable_data). The dyn-compatible facade
/// [`DiagnosticData`] is blanket-implemented for every `DiagnosticInfo` type; consumers
/// downcast to the concrete struct for typed access.
pub trait DiagnosticInfo: Any + Clone + fmt::Display + fmt::Debug + Send + Sync {
    /// Wire/config identity, namespaced `<crate-or-lang>.<area>.<condition>` (library
    /// conditions use `core.<area>.*`; presets and downstream languages use their own
    /// namespace). Semver-stable; deliberately decoupled from the type/module name — a
    /// struct rename is an internal refactor, an identifier change is a silent break.
    const IDENTIFIER: &'static str;

    /// The identity a stored condition *instance* answers — what
    /// [`Diagnostic::identifier`]/[`ParseError::identifier`] report. The default
    /// answers [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER), and every ordinary
    /// condition keeps that default: one Rust type, one compile-time identifier
    /// remains the norm.
    ///
    /// Overriding is for the exceptional case where a compile-time identifier is
    /// impossible: binding/embedding **adapter types**, where a single Rust struct
    /// carries conditions defined at runtime on the other side of the boundary
    /// (e.g. Python-defined conditions carried by one Rust adapter type), answering
    /// a per-instance identifier stored in the value. An overriding adapter still
    /// declares its [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) const — that stays
    /// the type's own identity for type-keyed uses; the override changes only what
    /// stored instances report.
    ///
    /// With both this trait and [`DiagnosticData`] in scope, an unqualified
    /// `.identifier()` call on a concrete condition type is ambiguous (both traits
    /// supply the method — E0034); use the qualified spelling,
    /// `DiagnosticInfo::identifier(&c)` or `DiagnosticData::identifier(&c)`.
    fn identifier(&self) -> &str {
        Self::IDENTIFIER
    }

    /// Serialization-boundary projection only (JSON output, generic tooling) — never a
    /// programmatic access path: consumers downcast to the concrete type instead. The
    /// default is an empty map; the authoritative schema is the Rust struct itself.
    fn serializable_data(&self) -> DiagnosticValue {
        DiagnosticValue::empty_map()
    }
}

mod sealed {
    /// Seals [`DiagnosticData`](super::DiagnosticData): the blanket impl over
    /// [`DiagnosticInfo`](super::DiagnosticInfo) is the only way in, so every stored
    /// condition is a `DiagnosticInfo` type. The const-identifier discipline is the
    /// default that comes with it; the per-instance
    /// [`identifier()`](super::DiagnosticInfo::identifier) override is the
    /// exceptional case (binding/embedding adapter types).
    pub trait Sealed {}
}

impl<T: DiagnosticInfo> sealed::Sealed for T {}

/// The dyn-compatible facade over [`DiagnosticInfo`] — what [`Diagnostic`] and
/// [`ParseError`] store.
///
/// **Sealed**: the blanket impl over `DiagnosticInfo` is the only implementation.
/// Downcast to the concrete condition type via
/// [`downcast_ref`](trait.DiagnosticData.html#method.downcast_ref) (dyn trait upcasting
/// to `dyn Any`; the concrete data struct is the one in-process identity of a condition).
pub trait DiagnosticData:
    Any + fmt::Display + fmt::Debug + Send + Sync + sealed::Sealed
{
    /// The condition's wire identity ([`DiagnosticInfo::identifier`] — for every
    /// ordinary condition, the [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) const).
    fn identifier(&self) -> &str;

    /// The condition's serialization-boundary projection
    /// ([`DiagnosticInfo::serializable_data`]).
    fn serializable_data(&self) -> DiagnosticValue;

    /// Clone the payload behind the dyn facade (backed by the concrete type's ordinary
    /// `Clone`).
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
    /// Whether the condition payload is a `T`.
    pub fn is<T: DiagnosticInfo>(&self) -> bool {
        (self as &dyn Any).is::<T>()
    }

    /// Downcast the condition payload to its concrete type — the in-process identity
    /// (the [`identifier`](DiagnosticData::identifier) string exists only for boundaries
    /// where types cannot go).
    pub fn downcast_ref<T: DiagnosticInfo>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

impl Clone for Box<dyn DiagnosticData> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Minimal alloc-only value tree for the serialization boundary
/// ([`DiagnosticInfo::serializable_data`]).
///
/// Deliberately barebones: no float variant — serialize floats
/// as strings if ever needed. A JSON emission helper over this tree is deferred,
/// non-breaking work.
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
    /// An empty [`Map`](DiagnosticValue::Map) — the default projection of a condition
    /// that serializes nothing.
    pub fn empty_map() -> DiagnosticValue {
        DiagnosticValue::Map(Vec::new())
    }
}

/// Conversion of one payload field into its [`DiagnosticValue`] projection — the
/// serialization bridge behind `#[derive(DiagnosticInfo)]`'s generated
/// [`serializable_data`](DiagnosticInfo::serializable_data).
///
/// Implemented for the payload primitives below (booleans, integers, `char`, strings,
/// `Option`, slices/`Vec`, references, and `DiagnosticValue` itself). The derive routes
/// every field through this trait, so a field of any other type is rejected by the
/// *compiler* with an error at the field's declaration — serializability of the whole
/// payload is enforced by trait bounds, not by a macro-side type check. Field-less
/// payload enums implement it via `#[derive(ToDiagnosticValue)]` (the kebab-cased
/// variant name); anything else can implement it by hand.
pub trait ToDiagnosticValue {
    /// This value's serialization-boundary projection.
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

/// Integers that may exceed `i64`: saturate (never panic — CLAUDE.md panic policy).
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

/// A [`ResolveError`](crate::source::ResolveError) projects as a map of its
/// `reference`, its `message`, and the rendered `cause-chain` (each
/// [`Error::source`](core::error::Error::source) hop's `Display`, outermost
/// first) — the serialization face of the
/// [`UnresolvableSourceReference`](crate::constructs::UnresolvableSourceReference)
/// condition's payload. The impl lives here, not in the source module: the error
/// module may reach down to source types, never the reverse (strict layering).
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
            ("cause-chain".to_string(), DiagnosticValue::List(chain)),
        ])
    }
}

/// An `Arc`-shared error value — the shape of a condition's optional
/// underlying-cause field ([`HookFailed::cause`] and the
/// [`ResolveError`](crate::source::ResolveError) pattern it follows) — projects
/// as the rendered cause chain: a list of the
/// error's own `Display`, then each
/// [`Error::source`](core::error::Error::source) hop's, outermost first.
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

/// One frame of a parse traceback snapshot: a rendered title (`group ‘{’`,
/// `argument #1 of ‘\frac’`) and the source location the parse descended at.
///
/// Snapshots are taken from the session's live frame stack by the recovery entry
/// point ([`ParseContext::recover`](crate::constructs::ParseContext::recover))
/// and stored **innermost first** on [`Diagnostic`] and
/// [`ParseError`]; unlike the live [`Frame`](crate::engine::Frame), a `TraceFrame` is
/// `L`-free — generic over the source origin only — so errors aggregate across languages.
#[derive(Debug, Clone)]
pub struct TraceFrame<O: SourceOrigin = Option<String>> {
    title: String,
    span: SourceSpan<O>,
}

impl<O: SourceOrigin> TraceFrame<O> {
    /// A snapshot frame with an already-rendered title.
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
/// The derived `Ord` ranks `Note < Warning < Error` for threshold filtering ("warnings
/// and above"). The core parser currently emits only `Error` diagnostics; `Note` and
/// `Warning` are constructor-supported for presets and embedders (lint-ish conditions)
/// and gain core producers only when such conditions arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational note.
    Note,
    /// Something suspicious that did not prevent parsing.
    Warning,
    /// A genuine error (in tolerant mode, recorded instead of aborting the parse).
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

/// A single condition reported during parsing, anchored to a source location.
///
/// Carries a structured condition payload — no message string; the human message is
/// rendered from the payload on demand ([`message`](Diagnostic::message)) — plus a
/// traceback snapshot ([`frames`](Diagnostic::frames), attached by the recovery
/// entry point, [`ParseContext::recover`](crate::constructs::ParseContext::recover)).
/// No `PartialEq`: tests compare [`identifier()`](Diagnostic::identifier) and downcast
/// fields.
#[derive(Debug, Clone)]
pub struct Diagnostic<O: SourceOrigin = Option<String>> {
    severity: Severity,
    data: Box<dyn DiagnosticData>,
    span: SourceSpan<O>,
    /// Traceback snapshot, innermost first.
    frames: Vec<TraceFrame<O>>,
}

impl<O: SourceOrigin> Diagnostic<O> {
    /// Create a diagnostic from a condition (no traceback frames; parses attach them
    /// through the recovery entry point,
    /// [`ParseContext::recover`](crate::constructs::ParseContext::recover)).
    pub fn new(
        severity: Severity,
        condition: impl DiagnosticInfo,
        span: SourceSpan<O>,
    ) -> Self {
        Diagnostic { severity, data: Box::new(condition), span, frames: Vec::new() }
    }

    /// Create an error-severity diagnostic.
    pub fn error(condition: impl DiagnosticInfo, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Error, condition, span)
    }

    /// Create a warning-severity diagnostic.
    pub fn warning(condition: impl DiagnosticInfo, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Warning, condition, span)
    }

    /// Create a note-severity diagnostic.
    pub fn note(condition: impl DiagnosticInfo, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Note, condition, span)
    }

    /// Assemble a diagnostic from already-boxed parts — the recover funnel's entry
    /// (post-refinement payload, snapshotted frames).
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

    /// The structured condition payload. Downcast to the concrete type for typed access.
    pub fn data(&self) -> &dyn DiagnosticData {
        &*self.data
    }

    /// The condition's wire identity ([`DiagnosticInfo::identifier`] — for every
    /// ordinary condition, the [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) const).
    pub fn identifier(&self) -> &str {
        self.data.identifier()
    }

    /// The human-readable message, rendered from the condition payload's `Display`.
    pub fn message(&self) -> String {
        self.data.to_string()
    }

    /// Where in the source the condition occurred.
    pub fn span(&self) -> &SourceSpan<O> {
        &self.span
    }

    /// The traceback snapshot at the moment the condition was recorded, innermost first
    /// (empty when recorded outside a parse descent).
    pub fn frames(&self) -> &[TraceFrame<O>] {
        &self.frames
    }

    /// Render a human-readable multi-line report: message, position (line/column via
    /// a transient [`LineIndexCache`], origin label), the traceback
    /// ([`format_traceback`]), and the source's provenance chain (`included from …` /
    /// `synthesized from …`). Shorthand for
    /// [`render_with`](Diagnostic::render_with) over a fresh cache.
    pub fn render(&self) -> String {
        self.render_with(&mut LineIndexCache::new())
    }

    /// [`render`](Diagnostic::render) with a caller-supplied [`LineColProvider`]
    /// answering the line/column lookups — a persistent [`LineIndexCache`] reused
    /// across renders (each source indexed once, ever), or an editor tool's own
    /// incremental line table.
    pub fn render_with(&self, line_cols: &mut impl LineColProvider<O>) -> String {
        render_report(line_cols, &self.to_string(), &self.span, &self.frames)
    }
}

impl<O: SourceOrigin> fmt::Display for Diagnostic<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity, self.data)
    }
}

/// An ordered collection of [`Diagnostic`]s, as accumulated over one parse.
///
/// Retention is **bounded**: at most [`limit`](Diagnostics::limit)
/// diagnostics are stored — [`DEFAULT_LIMIT`](Diagnostics::DEFAULT_LIMIT) unless the
/// collection was created with [`with_limit`](Diagnostics::with_limit). In tolerant mode
/// degenerate input can produce one diagnostic per byte; the cap turns that unbounded
/// allocation into a bounded one. Pushes beyond the cap are counted
/// ([`suppressed`](Diagnostics::suppressed), surfaced by
/// [`render_all`](Diagnostics::render_all) as "… and N more") and still feed
/// [`has_errors`](Diagnostics::has_errors), but the diagnostics themselves are dropped.
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
    /// The default retention cap (see [`with_limit`](Diagnostics::with_limit)) — generous
    /// for any human- or tool-facing report, small enough that degenerate tolerant-mode
    /// input cannot balloon memory.
    pub const DEFAULT_LIMIT: usize = 1000;

    /// Create an empty collection with the [`DEFAULT_LIMIT`](Diagnostics::DEFAULT_LIMIT)
    /// retention cap.
    pub fn new() -> Self {
        Diagnostics::with_limit(Diagnostics::<O>::DEFAULT_LIMIT)
    }

    /// Create an empty collection retaining at most `limit` diagnostics; pushes beyond
    /// the cap are counted as [`suppressed`](Diagnostics::suppressed) instead of stored.
    /// A driven parse's sink is seeded through
    /// [`ParseDriver::diagnostics_limit`](crate::engine::ParseDriver::diagnostics_limit).
    pub fn with_limit(limit: usize) -> Self {
        Diagnostics { items: Vec::new(), limit, suppressed: 0, error_count: 0 }
    }

    /// Append a diagnostic. Beyond the retention cap the diagnostic is dropped and only
    /// counted (see the type docs).
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

    /// Number of diagnostics retained (excludes [`suppressed`](Diagnostics::suppressed)
    /// ones).
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no diagnostics were recorded at all — retained *or* suppressed.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.suppressed == 0
    }

    /// The retention cap this collection was created with.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Number of diagnostics pushed beyond the retention cap (counted, not stored).
    pub fn suppressed(&self) -> usize {
        self.suppressed
    }

    /// Whether any pushed diagnostic — retained or suppressed — has
    /// [`Severity::Error`].
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Iterate over the diagnostics in the order they were recorded.
    pub fn iter(&self) -> core::slice::Iter<'_, Diagnostic<O>> {
        self.items.iter()
    }

    /// The diagnostics whose condition carries the given wire
    /// [`identifier`](Diagnostic::identifier), in recording order.
    pub fn with_identifier<'a>(
        &'a self,
        identifier: &'a str,
    ) -> impl Iterator<Item = &'a Diagnostic<O>> {
        self.items.iter().filter(move |d| d.identifier() == identifier)
    }

    /// The recorded condition payloads of concrete type `T`, in recording order —
    /// downcast-based typed access (pair with [`iter`](Diagnostics::iter) when the
    /// span/severity is needed alongside).
    pub fn conditions<T: DiagnosticInfo>(&self) -> impl Iterator<Item = &T> {
        self.items.iter().filter_map(|d| d.data().downcast_ref::<T>())
    }

    /// The diagnostics as a slice.
    pub fn as_slice(&self) -> &[Diagnostic<O>] {
        &self.items
    }

    /// The retained diagnostics **sorted by source position**: diagnostics arrive
    /// in *recovery* order (the order the parse hit them, which nested descents
    /// and deferred recoveries can permute), and this view re-sorts them by
    /// (source, span start) — **source order within each source**, for reports
    /// that read along the document.
    ///
    /// Sources are ordered by **first appearance** in the recorded sequence
    /// (matched by `Arc` identity). Deliberately narrow: a total "position order"
    /// across sources is ill-defined on multi-source parse trees (`\input`
    /// attachments are first-class), so no cross-source claim is made beyond the
    /// stable first-appearance grouping. The sort is stable — equal positions
    /// keep recovery order.
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

    /// Render every retained diagnostic as one human-readable report — the
    /// [`Diagnostic::render`] blocks in recording order, blank-line separated, followed
    /// by an "… and N more" line when pushes were
    /// [`suppressed`](Diagnostics::suppressed) beyond the retention cap.
    ///
    /// Positions are formatted through one transient [`LineIndexCache`] shared across
    /// the whole report (one line-starts table per distinct source, matched by `Arc`
    /// identity), so rendering k diagnostics over an N-byte document scans it once,
    /// not k times — per-diagnostic [`render`](Diagnostic::render)
    /// calls in a loop rebuild the index for every position (fine for a handful; this is
    /// the API for whole collections). Provenance chains benefit the same way: each
    /// including document is indexed once for the whole report. Shorthand for
    /// [`render_all_with`](Diagnostics::render_all_with) over a fresh cache.
    pub fn render_all(&self) -> String {
        self.render_all_with(&mut LineIndexCache::new())
    }

    /// [`render_all`](Diagnostics::render_all) with a caller-supplied
    /// [`LineColProvider`] answering the line/column lookups — a persistent
    /// [`LineIndexCache`] reused across reports, or an editor tool's own
    /// incremental line table.
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

/// Tolerant-parsing policy: what to do when an error carries a recovery possibility.
///
/// Under `Strict`, the parse aborts on the first error. Under `Tolerant`, a diagnostic is
/// recorded and parsing continues with the error's recovery token where one is available
/// (the recovery-token mechanism itself arrives with the token layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Abort on the first error.
    Strict,
    /// Record a diagnostic and continue whenever recovery is possible.
    Tolerant,
}

/// The abort error of construct parsing: a strict-mode escalation or a genuinely
/// unrecoverable condition.
///
/// **`Err` means abort**: recovery happens where a problem is
/// detected (through the recovery entry point,
/// [`ParseContext::recover`](crate::constructs::ParseContext::recover)),
/// abnormal endings of sub-parses travel as data
/// (`StopCause`), and nobody ever continues *past* a `ParseError` — it
/// carries no recovery payload and bubbles freely. Like [`Diagnostic`], it holds an
/// Arc-based [`SourceSpan`] plus the structured condition payload and the traceback
/// snapshot, so it is self-contained and outlives the parse. No `PartialEq`: tests
/// compare [`identifier()`](ParseError::identifier) and downcast fields.
#[derive(Debug, Clone)]
pub struct ParseError<O: SourceOrigin = Option<String>> {
    data: Box<dyn DiagnosticData>,
    span: SourceSpan<O>,
    /// Traceback snapshot, innermost first.
    frames: Vec<TraceFrame<O>>,
}

impl<O: SourceOrigin> ParseError<O> {
    /// Create a parse error from a condition (no traceback frames; parses attach them
    /// through the recovery entry point,
    /// [`ParseContext::recover`](crate::constructs::ParseContext::recover)).
    pub fn new(condition: impl DiagnosticInfo, span: SourceSpan<O>) -> ParseError<O> {
        ParseError { data: Box::new(condition), span, frames: Vec::new() }
    }

    /// Lift a token error's kind into the abort error: the
    /// built-in token conditions are boxed; a custom payload is unwrapped, never
    /// double-boxed.
    pub fn from_token_error(kind: TokenErrorKind, span: SourceSpan<O>) -> ParseError<O> {
        ParseError { data: kind.into_condition(), span, frames: Vec::new() }
    }

    /// Assemble a parse error from already-boxed parts — the recover funnel's entry
    /// (post-refinement payload, snapshotted frames).
    pub(crate) fn from_parts(
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<O>,
        frames: Vec<TraceFrame<O>>,
    ) -> ParseError<O> {
        ParseError { data, span, frames }
    }

    /// Attach a traceback snapshot (for the direct-abort sites, which do not pass
    /// through the recovery entry point) —
    /// typically [`ParserSession::snapshot_frames`](crate::engine::ParserSession::snapshot_frames),
    /// its public companion for custom parser code.
    pub fn with_frames(mut self, frames: Vec<TraceFrame<O>>) -> ParseError<O> {
        self.frames = frames;
        self
    }

    /// The structured condition payload. Downcast to the concrete type for typed access.
    pub fn data(&self) -> &dyn DiagnosticData {
        &*self.data
    }

    /// The condition's wire identity ([`DiagnosticInfo::identifier`] — for every
    /// ordinary condition, the [`IDENTIFIER`](DiagnosticInfo::IDENTIFIER) const).
    pub fn identifier(&self) -> &str {
        self.data.identifier()
    }

    /// The human-readable message, rendered from the condition payload's `Display`.
    pub fn message(&self) -> String {
        self.data.to_string()
    }

    /// Where in the source the condition occurred.
    pub fn span(&self) -> &SourceSpan<O> {
        &self.span
    }

    /// The traceback snapshot at the moment of the abort, innermost first (empty when
    /// the abort happened outside a parse descent).
    pub fn frames(&self) -> &[TraceFrame<O>] {
        &self.frames
    }

    /// Render a human-readable multi-line report (message, position, traceback,
    /// provenance chain), like [`Diagnostic::render`]. Shorthand for
    /// [`render_with`](ParseError::render_with) over a fresh transient cache.
    pub fn render(&self) -> String {
        self.render_with(&mut LineIndexCache::new())
    }

    /// [`render`](ParseError::render) with a caller-supplied [`LineColProvider`]
    /// answering the line/column lookups, like [`Diagnostic::render_with`].
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

/// Condition: an extension hook — consumer-supplied code the library calls, such
/// as a [`ParseDriver`](crate::engine::ParseDriver) method, a
/// [`Lang`](crate::state::Lang) hook, or a descent-state callback
/// ([`GroupChildState::Compute`](crate::constructs::GroupChildState::Compute)) —
/// reported that it failed to do its own work: an input/output failure, or a
/// runtime failure inside an embedding (for example, an exception raised by code
/// behind a language binding). Carried on the [`ParseError`] the hook returns;
/// the parse aborts.
///
/// A failing hook chooses between three distinct answers, and this condition is
/// exactly one of them:
///
/// - **`HookFailed`** — the hook's own code failed while doing its work
///   (input/output, a runtime failure in an embedding). Not a statement about
///   the document or about library contracts.
/// - [`ImplementationError`](crate::constructs::ImplementationError) — an
///   extension implementation violated a library contract: an implementation
///   bug to fix, not an operational failure.
/// - Any other condition type describing a problem **in the parsed document** —
///   reported through the recovery entry point
///   ([`ParseContext::recover`](crate::constructs::ParseContext::recover)) where
///   the hook's signature allows, so tolerant parses can record it and continue.
///
/// The optional [`cause`](HookFailed::cause) field carries the underlying error
/// behind an `Arc` (the [`ResolveError`](crate::source::ResolveError) pattern:
/// the `Arc` is what keeps the condition `Clone` — clones share the cause by
/// reference count). Embedders walk the chain from the field
/// ([`Error::source`](core::error::Error::source) hops) or downcast its
/// concrete type; the serialized projection renders the `detail` and the cause
/// chain as text.
#[derive(Debug, Clone, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.hooks.hook-failed",
    message = "extension hook reported a failure: {detail}"
)]
pub struct HookFailed {
    /// Human-readable description of the failure.
    pub detail: String,
    /// The underlying error, if the hook has one to attach (`None` when the
    /// `detail` string says everything). Attach via [`new`](HookFailed::new)'s
    /// second argument or [`with_cause`](HookFailed::with_cause).
    pub cause: Option<Arc<dyn core::error::Error + Send + Sync + 'static>>,
}

impl HookFailed {
    /// Attach the underlying error (kept on the [`cause`](HookFailed::cause)
    /// field; the `detail` string stays the rendered summary). Sugar over
    /// `new`'s second argument for a not-yet-shared error value: it takes the
    /// concrete error by value and shares it internally (`Arc`).
    pub fn with_cause(
        mut self,
        cause: impl core::error::Error + Send + Sync + 'static,
    ) -> Self {
        self.cause = Some(Arc::new(cause));
        self
    }
}

/// The shared body of the `render`/`render_with` family: headline, position,
/// traceback, provenance chain — line/column lookups answered by `line_cols`
/// (what makes [`Diagnostics::render_all`] O(N + k) instead of O(k·N): one
/// line-starts table per distinct source, however many positions ask).
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

/// Format a span's starting position for display: `@ (line 2, col 5) [origin]`, falling
/// back to a raw byte position — with a short parenthetical explaining that line
/// information is unavailable — whenever the line/column provider answers `None`
/// for the position (content past the [`LineIndexCache`] scan cap is one such
/// reason). The `[origin]` part is omitted when the source's origin has no label.
///
/// One-shot convenience: builds (and drops) a fresh transient cache per call —
/// shorthand for [`format_position_with`]. Rendering a
/// whole collection goes through [`Diagnostics::render_all`], which shares one
/// cache across positions.
pub fn format_position<O: SourceOrigin>(span: &SourceSpan<O>) -> String {
    format_position_with(span, &mut LineIndexCache::new())
}

/// [`format_position`] with a caller-supplied [`LineColProvider`] answering the
/// line/column lookup (a persistent [`LineIndexCache`], an editor's incremental
/// line table).
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

/// Format a traceback snapshot ([`TraceFrame`]s, innermost first) — one line per open
/// block, using each frame's own source for position and origin information.
///
/// Returns an empty string if `frames` is empty. [`Diagnostic::render`] and
/// [`ParseError::render`] append this to their reports. One-shot convenience over
/// a fresh transient cache — shorthand for [`format_traceback_with`].
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

/// [`format_traceback`] with a caller-supplied [`LineColProvider`] answering the
/// line/column lookups (a persistent [`LineIndexCache`], an editor's incremental
/// line table).
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
