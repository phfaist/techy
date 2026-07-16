//! Pluggable source resolution for `\input`-like external references.
//!
//! Per the crate's no_std policy, no file-system-backed resolver is provided here: an
//! embedder that wants to read files (or fetch URLs, query a database, …) implements
//! [`SourceResolver`] on its side, where the I/O capability lives.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::fmt;

use super::origin::SourceOrigin;
use super::source::{Source, SourceSpan};

/// Resolves an external reference (e.g. a file name from an `\input`-like construct) to
/// the referenced **content**. The parser will be generic over the resolver (wired in
/// Phase 7 — nothing calls [`resolve`](SourceResolver::resolve) yet); [`NoResolver`] is
/// the zero-sized, zero-cost default for builds that must not perform any lookup or I/O.
///
/// **The resolver returns content, not a [`Source`]** (decided July 2026, Action-05):
/// the caller mints the `Source` — see [`resolve_source`] — stamping the include-site
/// provenance (`SourceProvenance::Resolved { reference, triggered_at }`) itself. A
/// twice-included file thereby gets a *distinct* `Source` per include site, each
/// recording its own trigger, so diagnostics inside either inclusion render the right
/// include chain. Implementations are free to cache the content they fetch (content is
/// `triggered_at`-independent and may safely be shared); they cannot corrupt
/// provenance, which never passes through their hands.
///
/// `reference` is the reference string exactly as written; the **core never interprets
/// it** (no path semantics, no canonicalization — deliberate). `triggered_at` locates
/// the triggering construct: context a resolver may use (e.g. resolving a relative path
/// against the including source's origin) — that interpretation is resolver business.
///
/// **Recursion is the embedder's responsibility.** A resolver reachable from its own
/// output (`a.tex → \input{a.tex}`) makes unbounded include recursion possible; the
/// core performs no recursion checking, in line with never interpreting references.
/// An embedder that needs a bound (a command-line driver reading real files) enforces
/// its own include-depth limit or cycle check —
/// [`Source::provenance_chain`] exposes every enclosing
/// `Resolved { reference, triggered_at }` record for exactly this.
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits, decided July
/// 2026, matching the other stored extension traits —
/// [`CallableSpec`](crate::spec::CallableSpec)'s note applies): resolvers are stored in
/// long-lived, shareable language bundles. `resolve` takes `&self`, so a caching
/// implementation needs interior mutability — under this contract that means locks or
/// atomics (`Mutex`/`RwLock`/`OnceLock`, or `spin` on `no_std`), not `RefCell`/`Cell`.
pub trait SourceResolver<O: SourceOrigin = Option<String>>: Send + Sync {
    /// Resolve `reference` to its content (plus origin metadata for the source the
    /// caller will mint), or explain why it cannot be resolved.
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan<O>,
    ) -> Result<ResolvedContent<O>, ResolveError>;
}

// Compile-time pin: the trait must stay object-safe (drivers may store
// `Arc<dyn SourceResolver<O>>` rather than a generic parameter).
const _: fn(&dyn SourceResolver) = |_| {};

// Forwarding impls, so borrowed/boxed/shared resolvers plug in wherever an
// `impl SourceResolver` is expected without newtype shims.

impl<O: SourceOrigin, R: SourceResolver<O> + ?Sized> SourceResolver<O> for &R {
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

impl<O: SourceOrigin, R: SourceResolver<O> + ?Sized> SourceResolver<O> for Arc<R> {
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan<O>,
    ) -> Result<ResolvedContent<O>, ResolveError> {
        (**self).resolve(reference, triggered_at)
    }
}

/// Resolve `reference` through `resolver` and mint the [`Source`] — the call-site
/// composition the parser will use (Phase 7). Provenance is stamped **here, in core**:
/// each call produces a fresh `Source` whose provenance records *this* `triggered_at`,
/// which is what keeps a twice-included file's diagnostics pointing at the right
/// include site (see the trait docs).
pub fn resolve_source<O: SourceOrigin, R: SourceResolver<O> + ?Sized>(
    resolver: &R,
    reference: &str,
    triggered_at: &SourceSpan<O>,
) -> Result<Arc<Source<O>>, ResolveError> {
    let resolved = resolver.resolve(reference, triggered_at)?;
    Ok(Arc::new(
        Source::resolved(resolved.content, reference, triggered_at.clone())
            .with_origin(resolved.origin),
    ))
}

/// What a [`SourceResolver`] returns: the referenced content, plus origin metadata for
/// the [`Source`] the caller mints (see [`resolve_source`]).
#[derive(Debug, Clone)]
pub struct ResolvedContent<O: SourceOrigin = Option<String>> {
    /// The resolved content.
    pub content: String,
    /// Origin metadata (display metadata for diagnostics — conventionally the URL or
    /// path the content was obtained from); `O::default()` when the resolver knows
    /// nothing more.
    pub origin: O,
}

impl<O: SourceOrigin> ResolvedContent<O> {
    /// Resolved content with the default ("unknown") origin.
    pub fn new(content: impl Into<String>) -> ResolvedContent<O> {
        ResolvedContent { content: content.into(), origin: O::default() }
    }

    /// Attach origin metadata.
    pub fn with_origin(mut self, origin: O) -> ResolvedContent<O> {
        self.origin = origin;
        self
    }
}

/// Failure to resolve an external source reference.
///
/// Carries human-readable strings (the primary interface — a failed `\input` renders
/// into a diagnostic as text) plus an optional structured
/// [`cause`](ResolveError::with_cause) exposed through
/// [`core::error::Error::source`], so an embedder can walk the chain or downcast the
/// underlying error (e.g. an `io::Error`'s kind). Not `Clone`: the boxed cause is
/// single-owner.
#[derive(Debug)]
pub struct ResolveError {
    reference: String,
    message: String,
    cause: Option<Box<dyn core::error::Error + Send + Sync + 'static>>,
}

impl ResolveError {
    /// Create a resolve error for `reference` with a human-readable cause.
    pub fn new(reference: impl Into<String>, message: impl Into<String>) -> Self {
        ResolveError { reference: reference.into(), message: message.into(), cause: None }
    }

    /// Attach the underlying error (available via
    /// [`Error::source`](core::error::Error::source); the `message` string stays the
    /// rendered summary).
    pub fn with_cause(
        mut self,
        cause: impl core::error::Error + Send + Sync + 'static,
    ) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    /// The reference that failed to resolve.
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Human-readable cause of the failure.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot resolve source reference '{}': {}", self.reference, self.message)
    }
}

impl core::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.cause.as_ref().map(|cause| &**cause as &(dyn core::error::Error + 'static))
    }
}

/// Resolver that always fails: for parsers with no source-resolution capability (no I/O,
/// no lookup tables). Zero-sized, so it costs nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoResolver;

impl<O: SourceOrigin> SourceResolver<O> for NoResolver {
    fn resolve(
        &self,
        reference: &str,
        _triggered_at: &SourceSpan<O>,
    ) -> Result<ResolvedContent<O>, ResolveError> {
        Err(ResolveError::new(reference, "source resolution is not enabled"))
    }
}

/// Resolver backed by an in-memory map from reference strings to content — for tests,
/// preloaded database extracts, or any fully preloaded setup.
///
/// Serves origin types constructible from the reference string (`O: From<String>`,
/// which the default `Option<String>` satisfies); by default resolved sources carry the
/// unlabeled `O::default()` origin, and
/// [`with_reference_as_origin`](MapResolver::with_reference_as_origin) switches to
/// labeling each source with its reference, making multi-file diagnostics
/// self-describing.
#[derive(Debug, Clone, Default)]
pub struct MapResolver {
    contents: BTreeMap<String, String>,
    reference_as_origin: bool,
}

impl MapResolver {
    /// Create an empty map resolver.
    pub fn new() -> Self {
        MapResolver::default()
    }

    /// Register `content` for `reference`, replacing any previous entry.
    pub fn insert(&mut self, reference: impl Into<String>, content: impl Into<String>) {
        self.contents.insert(reference.into(), content.into());
    }

    /// Label each resolved source's origin with its reference string.
    pub fn with_reference_as_origin(mut self) -> Self {
        self.reference_as_origin = true;
        self
    }
}

impl From<BTreeMap<String, String>> for MapResolver {
    fn from(contents: BTreeMap<String, String>) -> Self {
        MapResolver { contents, reference_as_origin: false }
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
    fn no_resolver_always_fails() {
        let trigger = trigger_span();
        let result = NoResolver.resolve("chapter.tex", &trigger);
        let err = result.unwrap_err();
        assert_eq!(err.reference(), "chapter.tex");
        assert!(err.to_string().contains("chapter.tex"));
    }

    #[test]
    fn resolve_source_mints_the_source_with_this_triggers_provenance() {
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter content");

        let trigger = trigger_span();
        let resolved = resolve_source(&resolver, "chapter.tex", &trigger).unwrap();

        assert_eq!(resolved.content(), "chapter content");
        // The reference is recorded in the provenance; the origin stays at its default
        // (`None`) since reference-as-origin labeling is off.
        assert_eq!(resolved.origin().label(), None);
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

        let a = resolve_source(&resolver, "chapter.tex", &first).unwrap();
        let b = resolve_source(&resolver, "chapter.tex", &second).unwrap();

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
        let resolved = resolve_source(&resolver, "chapter.tex", &trigger).unwrap();
        assert_eq!(resolved.origin().label().as_deref(), Some("chapter.tex"));
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
    fn forwarding_impls_resolve_through_shared_and_dyn_handles() {
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter content");

        let trigger = trigger_span();
        let arc: Arc<dyn SourceResolver> = Arc::new(resolver);
        let resolved = resolve_source(&arc, "chapter.tex", &trigger).unwrap();
        assert_eq!(resolved.content(), "chapter content");
        // And through a plain borrow.
        let borrowed = &arc;
        assert!(borrowed.resolve("chapter.tex", &trigger).is_ok());
    }
}
