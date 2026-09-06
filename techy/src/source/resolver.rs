//! Pluggable resolution of `\input`-like external references to source content.
//!
//! [`SourceResolver`] is the trait an embedder implements; [`MapResolver`] is a ready-made
//! implementation over an in-memory map, for tests and preloaded setups.
//! [`resolve_source_reference`] is the call that runs a resolver and builds the resulting
//! [`Source`], and [`check_include_chain`] is the usual guard against runaway include
//! recursion.
//!
//! Because the crate uses only `core` and `alloc`, no file-system-backed resolver is
//! provided here: an embedder that wants to read files, fetch URLs, or query a database
//! implements [`SourceResolver`] on its own side, where those capabilities are available.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::any::Any;
use core::fmt;

use super::origin::SourceOrigin;
use super::source::{Source, SourceSpan};

/// Resolves an external reference — a file name from an `\input`-like construct, say — to
/// the content it names.
///
/// Implement this trait to give a parse access to content beyond the document it started
/// from. A resolver is configured on the parse driver with
/// [`with_source_resolver`](crate::core::StdParseDriver::with_source_resolver) and read
/// back through [`ParseDriver::source_resolver`](crate::core::ParseDriver::source_resolver);
/// a driver without one — the default — resolves nothing and performs no lookup at all.
///
/// A resolver returns content, not a [`Source`]. The caller builds the source and stamps
/// the provenance of *this* include site on it; [`resolve_source_reference`] is that call.
/// A file included twice therefore gets a separate `Source` per include site, each
/// recording its own trigger, so diagnostics inside either inclusion show the right
/// include chain. An implementation may freely cache the content it fetches, since content
/// does not depend on where it was requested from, and it cannot corrupt provenance
/// because provenance never passes through it.
///
/// `reference` is the reference string exactly as written. The parser never interprets it:
/// no path semantics, no canonicalization. `triggered_at` locates the construct that asked
/// for it, which a resolver may use as context — resolving a relative path against the
/// including source's origin, for instance.
///
/// # Recursion is the embedder's responsibility
///
/// A resolver whose output can reach itself (`a.tex` containing `\input{a.tex}`) makes
/// unbounded include recursion possible, and the parser performs no recursion checking,
/// consistent with never interpreting references. An embedder that needs a bound — a
/// command-line driver reading real files — enforces its own cycle check or depth limit.
/// [`check_include_chain`] is the ready-made version, and
/// [`Source::provenance_chain`] exposes every enclosing include record for a hand-written
/// one.
///
/// # Thread safety and downcasting
///
/// The `Send + Sync` supertraits are part of the contract, because resolvers are stored in
/// long-lived, shareable language bundles (the same reasoning as for
/// [`CallableSpec`](crate::core::specs::CallableSpec)). Since `resolve` takes `&self`, a
/// caching implementation needs interior mutability, and under this contract that means
/// locks or atomics — `Mutex`, `RwLock`, `OnceLock`, or the `spin` crate on `no_std` — not
/// `RefCell` or `Cell`.
///
/// The `Any` supertrait is likewise part of the contract: a consumer can recover a
/// resolver's concrete type from the stored `Arc<dyn SourceResolver<O>>` or
/// `&dyn SourceResolver<O>` by downcasting.
pub trait SourceResolver<O: SourceOrigin = Option<String>>: Send + Sync + Any {
    /// Resolves `reference` to its content, plus the origin metadata for the source the
    /// caller will build.
    ///
    /// # Errors
    ///
    /// Returns a [`ResolveError`] describing why the reference could not be resolved —
    /// there is no such file, reading it failed, the reference is malformed. The construct
    /// that asked for the content reports it as an
    /// [`UnresolvableSourceReference`](crate::core::constructs::UnresolvableSourceReference)
    /// diagnostic.
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan<O>,
    ) -> Result<ResolvedContent<O>, ResolveError>;
}

// Compile-time pin: the trait must stay object-safe (drivers may store
// `Arc<dyn SourceResolver<O>>` rather than a generic parameter).
const _: fn(&dyn SourceResolver) = |_| {};

// Forwarding impls, so borrowed/boxed resolvers plug in wherever an
// `impl SourceResolver` is expected without newtype shims. The `Any` supertrait
// requires `Self: 'static`, so the reference impl covers `&'static R` only (the
// box impl is unrestricted: `R: SourceResolver<O>` already implies `R: 'static`
// through `Any`). There is deliberately no
// `Arc<R>` forwarding impl: shared resolvers travel as `Arc<dyn SourceResolver<O>>`
// through the sealed [`IntoSourceResolver`] conversion, whose pass-through impls for
// `Arc<R>`/`Arc<dyn …>` (no double-wrap) would conflict with a blanket-covered
// `Arc<R>: SourceResolver`.

impl<O: SourceOrigin, R: SourceResolver<O> + ?Sized> SourceResolver<O> for &'static R {
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan<O>,
    ) -> Result<ResolvedContent<O>, ResolveError> {
        (**self).resolve(reference, triggered_at)
    }
}

impl<O: SourceOrigin, R: SourceResolver<O> + ?Sized> SourceResolver<O> for Box<R> {
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan<O>,
    ) -> Result<ResolvedContent<O>, ResolveError> {
        (**self).resolve(reference, triggered_at)
    }
}

mod sealed {
    use super::{SourceOrigin, SourceResolver};
    use alloc::sync::Arc;

    // Inference markers: they let the by-value blanket coexist with the Arc
    // pass-through impls (trait coherence would otherwise reject the pair on an
    // origin-generic trait). Callers never name them — the marker parameter is
    // inferred; each argument shape matches exactly one impl.
    pub struct ByValue;
    pub struct SharedConcrete;
    pub struct SharedDyn;

    pub trait Sealed<O, M> {}
    impl<O: SourceOrigin, R: SourceResolver<O> + 'static> Sealed<O, ByValue> for R {}
    impl<O: SourceOrigin, R: SourceResolver<O> + 'static> Sealed<O, SharedConcrete> for Arc<R> {}
    impl<O: SourceOrigin> Sealed<O, SharedDyn> for Arc<dyn SourceResolver<O>> {}
}

/// Sealed conversion into a shared [`SourceResolver`]: the argument contract of the
/// drivers' `with_source_resolver(…)` builders.
///
/// A resolver passed by value is wrapped in an `Arc` internally, while an
/// already-shared `Arc<R>` or `Arc<dyn SourceResolver<O>>` is passed through unchanged.
/// No call site needs to write `Arc::new`, and a resolver that is already shared is not
/// wrapped a second time.
///
/// The trait is sealed: these three conversions are the whole vocabulary, and downstream
/// code implements [`SourceResolver`] instead. The `M` parameter is a sealed marker that
/// lets type inference tell the three argument shapes apart; it never has to be named.
pub trait IntoSourceResolver<O: SourceOrigin, M>: sealed::Sealed<O, M> {
    /// Converts into the shared resolver handle a driver stores.
    fn into_source_resolver(self) -> Arc<dyn SourceResolver<O>>;
}

impl<O: SourceOrigin, R: SourceResolver<O> + 'static> IntoSourceResolver<O, sealed::ByValue>
    for R
{
    fn into_source_resolver(self) -> Arc<dyn SourceResolver<O>> {
        Arc::new(self)
    }
}

impl<O: SourceOrigin, R: SourceResolver<O> + 'static>
    IntoSourceResolver<O, sealed::SharedConcrete> for Arc<R>
{
    fn into_source_resolver(self) -> Arc<dyn SourceResolver<O>> {
        self
    }
}

impl<O: SourceOrigin> IntoSourceResolver<O, sealed::SharedDyn> for Arc<dyn SourceResolver<O>> {
    fn into_source_resolver(self) -> Arc<dyn SourceResolver<O>> {
        self
    }
}

/// Resolves `reference` through `resolver` and builds the [`Source`] for the result.
///
/// This is the composition a parser performs at an `\input`-like construct, and the place
/// where provenance is stamped: every call produces a fresh `Source` recording *this*
/// `triggered_at`, which is what keeps a twice-included file's diagnostics pointing at the
/// right include site (see [`SourceResolver`]). The new source carries the origin metadata
/// the resolver supplied and, where the resolver set them, its line and column number
/// offsets ([`ResolvedContent::line_number_offset`]).
///
/// # Errors
///
/// Returns the [`ResolveError`] the resolver produced, unchanged.
pub fn resolve_source_reference<O: SourceOrigin, R: SourceResolver<O> + ?Sized>(
    resolver: &R,
    reference: &str,
    triggered_at: &SourceSpan<O>,
) -> Result<Arc<Source<O>>, ResolveError> {
    let resolved = resolver.resolve(reference, triggered_at)?;
    let source = Source::resolved(resolved.content, reference, triggered_at.clone())
        .with_origin(resolved.origin);
    // The source's own defaults stand wherever the resolver set no offset.
    let line_number_offset =
        resolved.line_number_offset.unwrap_or(source.line_number_offset());
    let column_number_offset =
        resolved.column_number_offset.unwrap_or(source.column_number_offset());
    Ok(Arc::new(source.with_line_column_number_offsets(line_number_offset, column_number_offset)))
}

/// The ready-made include-cycle and include-depth check, called with `?` inside a
/// [`SourceResolver`] before it answers an `\input`-like reference.
///
/// Bounding include recursion is the embedder's policy — the parser never interprets
/// references and performs no recursion checking, and some self-inclusion is legitimate,
/// as in `.dtx` self-documenting files — but the common policy is this one, so this helper
/// makes it a one-liner.
///
/// The check compares source *origins*, not the reference strings recorded in provenance:
/// two spellings can name one file, and the primary source, which has no reference at all,
/// still takes part through the origin its creator gave it. The arguments divide the work
/// as follows:
///
/// - `target_key` is the canonical key of the source about to be included; a resolver
///   computes that canonical name — an absolute path, a normalized URL — during resolution
///   anyway.
/// - `triggered_at` locates the triggering construct. The chain examined is its source's
///   [`including_sources`](Source::including_sources): the would-be includer and every
///   source above it, up to and including the primary one.
/// - `origin_key` converts a chain source's origin into the same key space. This is cheap
///   *provided the resolver gives resolved sources their canonical name as their origin*,
///   and the embedder gives the primary source a suitable canonical origin too; the whole
///   check rests on that. Returning `None` skips that chain entry, meaning nothing
///   comparable was recorded for it.
/// - `max_depth`, when `Some`, bounds the inclusion depth of the new source. The primary
///   source is depth 0, a source it includes is depth 1, and so on.
///
/// # Errors
///
/// Returns a [`ResolveError`] when the target is already on its own include chain, and a
/// different one when including it would exceed `max_depth`; the two messages are
/// distinct.
///
/// The error's [`reference`](ResolveError::reference) field carries an origin label where
/// one exists, since the key type `K` is not required to be printable: for a cycle, the
/// label of the offending source on the chain; for a depth overflow, that of the immediate
/// includer, the target having no renderable origin yet. A resolver that prefers its own
/// spelling of the reference maps the error before returning it.
pub fn check_include_chain<O: SourceOrigin, K: PartialEq>(
    target_key: &K,
    triggered_at: &SourceSpan<O>,
    origin_key: impl Fn(&O) -> Option<K>,
    max_depth: Option<usize>,
) -> Result<(), ResolveError> {
    let mut depth: usize = 0;
    for source in triggered_at.source().including_sources() {
        depth += 1;
        if origin_key(source.origin()).is_some_and(|key| key == *target_key) {
            return Err(ResolveError::new(
                source.origin().label().unwrap_or_default(),
                "include cycle detected: this source is already on its own \
                 include chain",
            ));
        }
    }
    // `depth` now counts the chain sources (includer … primary), which is exactly
    // the new source's inclusion depth (primary = 0 ⇒ its inclusions = 1, …).
    if let Some(max_depth) = max_depth {
        if depth > max_depth {
            return Err(ResolveError::new(
                triggered_at.source().origin().label().unwrap_or_default(),
                alloc::format!(
                    "include depth {} exceeds the maximum of {}",
                    depth, max_depth
                ),
            ));
        }
    }
    Ok(())
}

/// What a [`SourceResolver`] returns: the referenced content, plus the origin metadata and
/// optional line and column number offsets for the [`Source`] the caller builds from it.
///
/// Start from [`new`](ResolvedContent::new) and add what is known with
/// [`with_origin`](ResolvedContent::with_origin) and the offset setters. See
/// [`resolve_source_reference`] for how the source is built.
#[derive(Debug, Clone)]
pub struct ResolvedContent<O: SourceOrigin = Option<String>> {
    /// The resolved content.
    pub content: String,
    /// Origin metadata shown in diagnostics, conventionally the URL or path the content
    /// was obtained from; `O::default()` when the resolver knows nothing more.
    pub origin: O,
    /// The line number offset for the source built from this content
    /// ([`Source::with_line_column_number_offsets`]), or `None` to keep that source's
    /// default.
    ///
    /// A resolver that hands over only part of what it read — a file whose leading
    /// front-matter block it consumed itself, say — sets this so that line numbers in
    /// diagnostics still match the original file. The value is the offset itself, not an
    /// increment: a resolver that removed `n` leading lines from 1-indexed content sets
    /// `1 + n`.
    ///
    /// Byte offsets and spans stay relative to the content handed over; only line and
    /// column numbering shifts.
    pub line_number_offset: Option<usize>,
    /// The column number offset for the source built from this content, or `None` to keep
    /// that source's default (see
    /// [`line_number_offset`](ResolvedContent::line_number_offset)).
    pub column_number_offset: Option<usize>,
}

impl<O: SourceOrigin> ResolvedContent<O> {
    /// Resolved content with the default ("unknown") origin and no line or column number
    /// offsets of its own.
    pub fn new(content: impl Into<String>) -> ResolvedContent<O> {
        ResolvedContent {
            content: content.into(),
            origin: O::default(),
            line_number_offset: None,
            column_number_offset: None,
        }
    }

    /// Attaches origin metadata.
    pub fn with_origin(mut self, origin: O) -> ResolvedContent<O> {
        self.origin = origin;
        self
    }

    /// Sets the [`line_number_offset`](ResolvedContent::line_number_offset).
    pub fn with_line_number_offset(mut self, line_number_offset: usize) -> ResolvedContent<O> {
        self.line_number_offset = Some(line_number_offset);
        self
    }

    /// Sets the [`column_number_offset`](ResolvedContent::column_number_offset).
    pub fn with_column_number_offset(mut self, column_number_offset: usize) -> ResolvedContent<O> {
        self.column_number_offset = Some(column_number_offset);
        self
    }
}

/// Failure to resolve an external source reference, returned by
/// [`SourceResolver::resolve`].
///
/// The [`reference`](ResolveError::reference) and [`message`](ResolveError::message)
/// strings are the primary interface, because a failed `\input` is rendered into a
/// diagnostic as text. An implementation may additionally attach the underlying error with
/// [`with_cause`](ResolveError::with_cause); it is exposed through
/// [`core::error::Error::source`], so an embedder can walk the error chain or downcast to
/// inspect it — the kind of an `io::Error`, for instance.
///
/// The type is `Clone`, like every error type in this crate, because error values travel
/// into diagnostics and those are `Clone` throughout. The attached cause comes from
/// outside the crate and cannot be cloned by value, so it is held behind an `Arc` and
/// cloned by reference count.
#[derive(Debug, Clone)]
pub struct ResolveError {
    reference: String,
    message: String,
    cause: Option<Arc<dyn core::error::Error + Send + Sync + 'static>>,
}

impl ResolveError {
    /// Creates a resolve error for `reference`, with `message` explaining the failure in
    /// human-readable terms.
    pub fn new(reference: impl Into<String>, message: impl Into<String>) -> Self {
        ResolveError { reference: reference.into(), message: message.into(), cause: None }
    }

    /// Attaches the underlying error, which becomes available through
    /// [`Error::source`](core::error::Error::source).
    ///
    /// The [`message`](ResolveError::message) string remains the rendered summary. The
    /// cause is stored behind an `Arc`, which is what keeps this error `Clone`.
    pub fn with_cause(
        mut self,
        cause: impl core::error::Error + Send + Sync + 'static,
    ) -> Self {
        self.cause = Some(Arc::new(cause));
        self
    }

    /// The reference that failed to resolve.
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Human-readable description of the failure.
    pub fn message(&self) -> &str {
        &self.message
    }
}

// The wording deliberately differs from the `UnresolvableSourceReference`
// condition's "cannot resolve source reference ‘…’: …" (which renders this
// error's `message()` as its detail): in an error chain that shows both, the
// two sentences stay distinguishable instead of reading as a duplicated prefix.
impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source reference ‘{}’ failed to resolve: {}", self.reference, self.message)
    }
}

impl core::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.cause.as_ref().map(|cause| &**cause as &(dyn core::error::Error + 'static))
    }
}

/// A [`SourceResolver`] backed by an in-memory map from reference strings to content, for
/// tests, preloaded database extracts, and other fully preloaded setups.
///
/// Fill one with [`insert`](MapResolver::insert), or build it from any iterator of
/// `(reference, content)` pairs. It serves any origin type constructible from the
/// reference string (`O: From<String>`, which the default `Option<String>` satisfies).
///
/// Resolved sources carry the unlabeled default origin;
/// [`with_reference_as_origin`](MapResolver::with_reference_as_origin) labels each of them
/// with its reference instead, which makes multi-file diagnostics self-describing.
#[derive(Debug, Clone, Default)]
pub struct MapResolver {
    contents: BTreeMap<String, String>,
    reference_as_origin: bool,
}

impl MapResolver {
    /// Creates an empty map resolver.
    pub fn new() -> Self {
        MapResolver::default()
    }

    /// Registers `content` for `reference`, replacing any previous entry.
    pub fn insert(&mut self, reference: impl Into<String>, content: impl Into<String>) {
        self.contents.insert(reference.into(), content.into());
    }

    /// Labels each resolved source's origin with its reference string.
    pub fn with_reference_as_origin(mut self) -> Self {
        self.reference_as_origin = true;
        self
    }
}

impl<I: IntoIterator<Item = (String, String)>> From<I> for MapResolver {
    fn from(contents: I) -> Self {
        MapResolver { contents: contents.into_iter().collect(), reference_as_origin: false }
    }
}

impl<O: SourceOrigin + From<String>> SourceResolver<O> for MapResolver {
    fn resolve(
        &self,
        reference: &str,
        _triggered_at: &SourceSpan<O>,
    ) -> Result<ResolvedContent<O>, ResolveError> {
        match self.contents.get(reference) {
            Some(content) => {
                let mut resolved = ResolvedContent::new(content.clone());
                if self.reference_as_origin {
                    resolved = resolved.with_origin(O::from(reference.to_string()));
                }
                Ok(resolved)
            }
            None => Err(ResolveError::new(reference, "no entry for this reference")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceProvenance;

    fn trigger_span() -> SourceSpan {
        let main: Arc<Source> = Arc::new(Source::new(r"\input{chapter.tex}"));
        SourceSpan::entire(&main)
    }

    #[test]
    fn dyn_source_resolver_downcasts_to_its_concrete_type() {
        // The `Any` supertrait: the resolver a driver stores comes back type-erased
        // from the accessor a consumer actually uses — `ParseDriver::source_resolver`
        // — and gives its concrete type back through dyn-to-`Any` upcasting.
        use crate::engine::{ParseDriver, StdParseDriver};
        use crate::error::Recovery;

        #[derive(Debug, Clone, Copy)]
        struct PlainLang;
        impl crate::state::TrivialLang for PlainLang {}

        let mut map = MapResolver::new();
        map.insert("chapter.tex", "chapter content");
        let driver: StdParseDriver =
            StdParseDriver::new(Recovery::Strict, ()).with_source_resolver(map);
        let resolver =
            ParseDriver::<PlainLang>::source_resolver(&driver).expect("resolver configured");
        let any: &dyn Any = resolver;
        let concrete = any.downcast_ref::<MapResolver>().expect("downcast to MapResolver");
        let resolved = concrete.resolve("chapter.tex", &trigger_span()).unwrap();
        assert_eq!(resolved.content, "chapter content");
    }

    #[test]
    fn resolve_source_reference_mints_the_source_with_this_triggers_provenance() {
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter content");

        let trigger = trigger_span();
        let resolved = resolve_source_reference(&resolver, "chapter.tex", &trigger).unwrap();

        assert_eq!(resolved.content(), "chapter content");
        // The reference is recorded in the provenance; the origin stays at its default
        // (`None`) since reference-as-origin labeling is off.
        assert_eq!(resolved.origin().label(), None);
        // No offsets set by the resolver: the source keeps its defaults.
        assert_eq!(resolved.line_number_offset(), 1);
        assert_eq!(resolved.column_number_offset(), 1);
        match resolved.provenance() {
            SourceProvenance::Resolved { reference, triggered_at } => {
                assert_eq!(reference, "chapter.tex");
                assert_eq!(triggered_at, &trigger);
            }
            other => panic!("expected Resolved provenance, got {:?}", other),
        }
    }

    /// The reason the resolver returns content rather than a `Source`: a twice-included
    /// file gets a distinct `Source` per include site, each recording its own trigger —
    /// a resolver-side content cache cannot corrupt provenance.
    #[test]
    fn each_include_site_gets_its_own_provenance() {
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter content");

        let main: Arc<Source> =
            Arc::new(Source::new(r"\input{chapter.tex}\input{chapter.tex}"));
        let first = SourceSpan::new(&main, 0..19);
        let second = SourceSpan::new(&main, 19..38);

        let a = resolve_source_reference(&resolver, "chapter.tex", &first).unwrap();
        let b = resolve_source_reference(&resolver, "chapter.tex", &second).unwrap();

        assert!(!Arc::ptr_eq(&a, &b), "distinct sources per include site");
        match (a.provenance(), b.provenance()) {
            (
                SourceProvenance::Resolved { triggered_at: at_a, .. },
                SourceProvenance::Resolved { triggered_at: at_b, .. },
            ) => {
                assert_eq!(at_a, &first);
                assert_eq!(at_b, &second);
            }
            other => panic!("expected Resolved provenance on both, got {:?}", other),
        }
    }

    /// A resolver that hands over a suffix of what it read (a front-matter block it
    /// consumed itself) keeps line numbers true through the offset fields.
    #[test]
    fn resolver_offsets_reach_the_minted_source() {
        struct FrontMatterStripper;
        impl SourceResolver for FrontMatterStripper {
            fn resolve(
                &self,
                _reference: &str,
                _triggered_at: &SourceSpan,
            ) -> Result<ResolvedContent, ResolveError> {
                // Three leading lines removed from 1-indexed content: the first line
                // handed over is line 4 of the file.
                Ok(ResolvedContent::new("body line\nnext").with_line_number_offset(4))
            }
        }

        let resolved =
            resolve_source_reference(&FrontMatterStripper, "doc.tex", &trigger_span()).unwrap();
        assert_eq!(resolved.line_number_offset(), 4);
        assert_eq!(resolved.column_number_offset(), 1, "an unset offset keeps the default");
        assert_eq!(resolved.line_index().line_col(0), Some((4, 1)));
        assert_eq!(resolved.line_index().line_col(10), Some((5, 1)));
    }

    #[test]
    fn map_resolver_builds_from_any_pair_iterator() {
        let resolver = MapResolver::from([
            ("chapter.tex".to_string(), "chapter content".to_string()),
            ("appendix.tex".to_string(), "appendix content".to_string()),
        ]);

        let trigger = trigger_span();
        let resolved = resolve_source_reference(&resolver, "appendix.tex", &trigger).unwrap();
        assert_eq!(resolved.content(), "appendix content");
        // Origin labeling defaults to off, same as `MapResolver::new()`.
        assert_eq!(resolved.origin().label(), None);
    }

    #[test]
    fn map_resolver_fails_on_unknown_reference() {
        let resolver = MapResolver::new();
        let trigger = trigger_span();
        assert!(SourceResolver::<Option<String>>::resolve(&resolver, "missing.tex", &trigger)
            .is_err());
    }

    #[test]
    fn map_resolver_can_label_origins_with_the_reference() {
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter content");
        let resolver = resolver.with_reference_as_origin();

        let trigger = trigger_span();
        let resolved = resolve_source_reference(&resolver, "chapter.tex", &trigger).unwrap();
        assert_eq!(resolved.origin().label().as_deref(), Some("chapter.tex"));
    }

    // --- check_include_chain ([§dd-dr:include-chain-helpers]) -------------------------

    /// A chain `primary("doc") → "a.tex" → "b.tex"`, origins = canonical names, with
    /// a trigger span inside the innermost source.
    fn chain_trigger() -> SourceSpan {
        let primary: Arc<Source> =
            Arc::new(Source::new(r"\input{a.tex}").with_origin(Some("doc.tex".into())));
        let a: Arc<Source> = Arc::new(
            Source::resolved(r"\input{b.tex}", "a.tex", SourceSpan::entire(&primary))
                .with_origin(Some("a.tex".into())),
        );
        let b: Arc<Source> = Arc::new(
            Source::resolved(r"\input{c.tex}", "b.tex", SourceSpan::entire(&a))
                .with_origin(Some("b.tex".into())),
        );
        SourceSpan::entire(&b)
    }

    fn key(origin: &Option<String>) -> Option<String> {
        origin.clone()
    }

    #[test]
    fn check_include_chain_passes_a_fresh_target() {
        let trigger = chain_trigger();
        assert!(check_include_chain(&String::from("c.tex"), &trigger, key, None).is_ok());
        // Depth: the new source would sit at depth 3 (primary = 0) — allowed at 3.
        assert!(check_include_chain(&String::from("c.tex"), &trigger, key, Some(3)).is_ok());
    }

    #[test]
    fn check_include_chain_detects_a_cycle() {
        let trigger = chain_trigger();
        let err =
            check_include_chain(&String::from("a.tex"), &trigger, key, None).unwrap_err();
        assert!(err.message().contains("include cycle"));
        assert_eq!(err.reference(), "a.tex");
    }

    #[test]
    fn check_include_chain_cycles_are_keyed_on_origins_including_the_primary() {
        // The primary participates through the origin its creator minted — the
        // user's point: `\input{doc.tex}` from anywhere inside the tree is a cycle.
        let trigger = chain_trigger();
        let err =
            check_include_chain(&String::from("doc.tex"), &trigger, key, None).unwrap_err();
        assert!(err.message().contains("include cycle"));
        assert_eq!(err.reference(), "doc.tex");
    }

    #[test]
    fn check_include_chain_detects_depth_overflow_with_a_distinct_message() {
        let trigger = chain_trigger();
        let err = check_include_chain(&String::from("c.tex"), &trigger, key, Some(2))
            .unwrap_err();
        assert!(err.message().contains("include depth 3 exceeds the maximum of 2"));
        assert!(!err.message().contains("cycle"));
    }

    #[test]
    fn check_include_chain_skips_none_keys() {
        // A chain whose sources carry no origins: nothing comparable — no cycle is
        // ever reported, whatever the target key.
        let primary: Arc<Source> = Arc::new(Source::new(r"\input{a.tex}"));
        let a: Arc<Source> =
            Arc::new(Source::resolved("x", "a.tex", SourceSpan::entire(&primary)));
        let trigger = SourceSpan::entire(&a);
        assert!(check_include_chain(&String::from("a.tex"), &trigger, key, None).is_ok());
        // The label-less offender is still named in a depth error, by empty reference.
        let err = check_include_chain(&String::from("a.tex"), &trigger, key, Some(1))
            .unwrap_err();
        assert_eq!(err.reference(), "");
    }

    #[test]
    fn resolve_error_is_clone_with_a_shared_cause() {
        #[derive(Debug)]
        struct Underlying;
        impl fmt::Display for Underlying {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("io failure")
            }
        }
        impl core::error::Error for Underlying {}

        // The ruled principle: techy error types stay uniformly Clone; the
        // out-of-crate cause sits behind an Arc, cloned by refcount.
        let err = ResolveError::new("chapter.tex", "cannot read file").with_cause(Underlying);
        let clone = err.clone();
        assert_eq!(clone.reference(), "chapter.tex");
        let source = core::error::Error::source(&clone).expect("cause survives the clone");
        assert!(source.downcast_ref::<Underlying>().is_some());
    }

    #[test]
    fn resolve_error_exposes_its_cause_through_the_error_chain() {
        #[derive(Debug)]
        struct Underlying;
        impl fmt::Display for Underlying {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("file not found (os error 2)")
            }
        }
        impl core::error::Error for Underlying {}

        let err = ResolveError::new("chapter.tex", "cannot read file").with_cause(Underlying);
        // The strings stay the primary interface…
        assert_eq!(err.message(), "cannot read file");
        // …and the structured cause travels the standard chain, downcast included.
        let source = core::error::Error::source(&err).expect("cause attached");
        assert_eq!(source.to_string(), "file not found (os error 2)");
        assert!(source.downcast_ref::<Underlying>().is_some());
        assert!(core::error::Error::source(&ResolveError::new("x", "y")).is_none());
    }

    #[test]
    fn forwarding_impls_resolve_through_borrowed_and_boxed_handles() {
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter content");

        let trigger = trigger_span();
        let arc: Arc<dyn SourceResolver> = Arc::new(resolver);
        // Through the unsized dyn value (the driver-accessor shape)…
        let resolved = resolve_source_reference(&*arc, "chapter.tex", &trigger).unwrap();
        assert_eq!(resolved.content(), "chapter content");
        // …and through a plain borrow of it.
        let borrowed: &dyn SourceResolver = &*arc;
        assert!(borrowed.resolve("chapter.tex", &trigger).is_ok());
    }

    #[test]
    fn into_source_resolver_shares_by_value_and_passes_arcs_through() {
        // By value: the sealed conversion does the sharing — no `Arc::new` at the
        // call site.
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter content");
        let shared: Arc<dyn SourceResolver> = resolver.into_source_resolver();
        let trigger = trigger_span();
        assert!(shared.resolve("chapter.tex", &trigger).is_ok());

        // A pre-shared `Arc<R>` passes through — same allocation, no double-wrap.
        let premade = Arc::new(MapResolver::new());
        let witness = Arc::clone(&premade);
        let converted: Arc<dyn SourceResolver> = premade.into_source_resolver();
        assert!(Arc::ptr_eq(&converted, &(witness as Arc<dyn SourceResolver>)));

        // A pre-shared `Arc<dyn …>` passes through unchanged too.
        let dyn_made: Arc<dyn SourceResolver> = Arc::new(MapResolver::new());
        let witness = Arc::clone(&dyn_made);
        let converted = dyn_made.into_source_resolver();
        assert!(Arc::ptr_eq(&converted, &witness));
    }
}
