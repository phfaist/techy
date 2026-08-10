//! Pluggable source resolution for `\input`-like external references.
//!
//! Per the crate's no_std policy, no file-system-backed resolver is provided here: an
//! embedder that wants to read files (or fetch URLs, query a database, …) implements
//! [`SourceResolver`] on its side, where the I/O capability lives.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::any::Any;
use core::fmt;

use super::origin::SourceOrigin;
use super::source::{Source, SourceSpan};

/// Resolves an external reference (e.g. a file name from an `\input`-like construct) to
/// the referenced **content**. Resolvers are configured on the parse driver
/// (`with_source_resolver`) and reached through its `source_resolver` accessor;
/// `None` there — the default — is the canonical "resolves nothing" (no lookup,
/// no I/O).
///
/// **The resolver returns content, not a [`Source`]**:
/// the caller mints the `Source` — see [`resolve_source_reference`] — stamping the include-site
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
/// **Thread safety is part of the contract** (`Send + Sync` supertraits, matching the
/// other stored extension traits —
/// [`CallableSpec`](crate::spec::CallableSpec)'s note applies): resolvers are stored in
/// long-lived, shareable language bundles. `resolve` takes `&self`, so a caching
/// implementation needs interior mutability — under this contract that means locks or
/// atomics (`Mutex`/`RwLock`/`OnceLock`, or `spin` on `no_std`), not `RefCell`/`Cell`.
///
/// **Downcasting is part of the contract** (`Any` supertrait;
/// [`CallableSpec`](crate::spec::CallableSpec)'s downcasting note applies): a consumer
/// recovers a resolver's concrete type from the stored `Arc<dyn SourceResolver<O>>` or
/// `&dyn SourceResolver<O>`.
pub trait SourceResolver<O: SourceOrigin = Option<String>>: Send + Sync + Any {
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

// Forwarding impls, so borrowed/boxed resolvers plug in wherever an
// `impl SourceResolver` is expected without newtype shims. The `Any` supertrait
// requires `Self: 'static`, so the reference impl covers `&'static R` only and the
// box impl requires `R: 'static`. There is deliberately no
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

impl<O: SourceOrigin, R: SourceResolver<O> + ?Sized + 'static> SourceResolver<O> for Box<R> {
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

/// Sealed conversion into a shared [`SourceResolver`] — the argument contract of the
/// shipped drivers' `with_source_resolver(…)` builders, following the crate's one
/// Arc-removal conversion idiom: a resolver passed **by value** is shared internally
/// (`Arc::new`), while an already-shared **`Arc<R>`** or **`Arc<dyn
/// SourceResolver<O>>`** passes through as-is — no `Arc::new` at any call site, and
/// no double-wrap of pre-shared resolvers.
///
/// Sealed: the three impls are the whole vocabulary; downstream code implements
/// [`SourceResolver`], never this trait. (The `M` parameter is a sealed inference
/// marker distinguishing the three argument shapes — it never needs to be named.)
pub trait IntoSourceResolver<O: SourceOrigin, M>: sealed::Sealed<O, M> {
    /// Convert into the shared resolver handle the drivers store.
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

/// Resolve `reference` through `resolver` and mint the [`Source`] — the call-site
/// composition the parser will use. Provenance is stamped **here, in core**:
/// each call produces a fresh `Source` whose provenance records *this* `triggered_at`,
/// which is what keeps a twice-included file's diagnostics pointing at the right
/// include site (see the trait docs).
pub fn resolve_source_reference<O: SourceOrigin, R: SourceResolver<O> + ?Sized>(
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

/// The ready-made include-cycle-plus-depth check a [`SourceResolver`] calls with `?`
/// before answering an `\input`-like reference — recursion control stays **embedder
/// policy** (the core never interprets references and performs no recursion
/// checking; legitimate self-inclusion exists, e.g. `.dtx` self-documenting files),
/// and this helper makes the common policy a one-liner.
///
/// The check is keyed on **origins**, not on provenance reference strings — two
/// spellings can name one file, and the *primary* source (which has no reference)
/// participates through the origin its creator minted. The division of labor:
///
/// - `target_key` is the already-canonicalized key of the source *about to be
///   included* — the resolver computes the canonical name during resolution anyway
///   (an absolute path, a normalized URL);
/// - `triggered_at` locates the triggering construct; the chain walked is its
///   source's [`including_sources`](Source::including_sources) — the would-be
///   includer and every source above it, primary included;
/// - `origin_key` converts a chain source's origin into the same key space —
///   a cheap conversion **when the resolver mints canonical origins** (the
///   documented invariant this helper rests on: resolved sources must carry the
///   canonical name as their origin, and the embedder mints the primary source
///   with a suitable canonical origin too). A `None` key skips that chain entry
///   (nothing comparable recorded).
/// - `max_depth`, when `Some`, bounds the *inclusion depth* of the new source:
///   the primary is depth 0, a source it includes is depth 1, and so on; the
///   check fails when the new source's depth would exceed `max_depth`.
///
/// A detected cycle and an exceeded depth produce distinct
/// [`ResolveError`] messages. The error's `reference` field carries an origin
/// label when one exists (the key type `K` is not required to render itself):
/// for a cycle, the offending chain source's; for a depth overflow, the
/// immediate includer's (the target itself has no renderable origin yet). A
/// resolver wanting its own reference spelling maps the error before returning
/// it.
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

/// What a [`SourceResolver`] returns: the referenced content, plus origin metadata for
/// the [`Source`] the caller mints (see [`resolve_source_reference`]).
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
/// underlying error (e.g. an `io::Error`'s kind).
///
/// `Clone`, deliberately: **techy error types stay uniformly `Clone`** — error
/// values travel into diagnostics, which are `Clone` throughout — and
/// out-of-crate information (the cause, whose concrete type techy cannot clone)
/// sits behind an `Arc`, cloned by refcount.
#[derive(Debug, Clone)]
pub struct ResolveError {
    reference: String,
    message: String,
    cause: Option<Arc<dyn core::error::Error + Send + Sync + 'static>>,
}

impl ResolveError {
    /// Create a resolve error for `reference` with a human-readable cause.
    pub fn new(reference: impl Into<String>, message: impl Into<String>) -> Self {
        ResolveError { reference: reference.into(), message: message.into(), cause: None }
    }

    /// Attach the underlying error (available via
    /// [`Error::source`](core::error::Error::source); the `message` string stays the
    /// rendered summary). Shared internally (`Arc`), which is what keeps the whole
    /// error `Clone` — see the type docs.
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

    /// Human-readable cause of the failure.
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
        // The `Any` supertrait: a stored `Arc<dyn SourceResolver>` gives its concrete
        // type back through dyn-to-`Any` upcasting.
        let mut map = MapResolver::new();
        map.insert("chapter.tex", "chapter content");
        let resolver: Arc<dyn SourceResolver> = Arc::new(map);
        let any: &dyn Any = &*resolver;
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
