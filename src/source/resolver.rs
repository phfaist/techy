//! Pluggable source resolution for `\input`-like external references.
//!
//! Per the crate's no_std policy, no file-system-backed resolver is provided here: an
//! embedder that wants to read files (or fetch URLs, query a database, …) implements
//! [`SourceResolver`] on its side, where the I/O capability lives.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use super::origin::SourceOrigin;
use super::source::{Source, SourceSpan};

/// Resolves an external reference (e.g. a file name from an `\input`-like construct) to a
/// new [`Source`].
///
/// Implementations create the source with
/// [`SourceProvenance::Resolved`](super::SourceProvenance::Resolved)
/// pointing back at `triggered_at` — the constructor
/// [`Source::resolved`] does this. A resolver that knows where the content was obtained from
/// (conventionally a URL) attaches it via [`Source::with_origin`]. The parser is generic
/// over the resolver; [`NoResolver`] is the zero-sized, zero-cost default for builds that
/// must not perform any lookup or I/O.
pub trait SourceResolver<O: SourceOrigin = Option<String>> {
    /// Resolve `reference` to a new source, or explain why it cannot be resolved.
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan<O>,
    ) -> Result<Arc<Source<O>>, ResolveError>;
}

/// Failure to resolve an external source reference.
#[derive(Debug, Clone)]
pub struct ResolveError {
    reference: String,
    message: String,
}

impl ResolveError {
    /// Create a resolve error for `reference` with a human-readable cause.
    pub fn new(reference: impl Into<String>, message: impl Into<String>) -> Self {
        ResolveError { reference: reference.into(), message: message.into() }
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

impl core::error::Error for ResolveError {}

/// Resolver that always fails: for parsers with no source-resolution capability (no I/O,
/// no lookup tables). Zero-sized, so it costs nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoResolver;

impl<O: SourceOrigin> SourceResolver<O> for NoResolver {
    fn resolve(
        &self,
        reference: &str,
        _triggered_at: &SourceSpan<O>,
    ) -> Result<Arc<Source<O>>, ResolveError> {
        Err(ResolveError::new(reference, "source resolution is not enabled"))
    }
}

/// Resolver backed by an in-memory map from reference strings to content — for tests,
/// preloaded database extracts, or any fully preloaded setup.
#[derive(Debug, Clone, Default)]
pub struct MapResolver {
    contents: BTreeMap<String, String>,
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
}

impl From<BTreeMap<String, String>> for MapResolver {
    fn from(contents: BTreeMap<String, String>) -> Self {
        MapResolver { contents }
    }
}

impl<O: SourceOrigin> SourceResolver<O> for MapResolver {
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan<O>,
    ) -> Result<Arc<Source<O>>, ResolveError> {
        match self.contents.get(reference) {
            Some(content) => Ok(Arc::new(Source::resolved(
                content.clone(),
                reference,
                triggered_at.clone(),
            ))),
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
    fn map_resolver_resolves_known_references() {
        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter content");

        let trigger = trigger_span();
        let resolved = resolver.resolve("chapter.tex", &trigger).unwrap();

        assert_eq!(resolved.content(), "chapter content");
        // The reference is recorded in the provenance; the origin stays at its default
        // (`None`) since the map resolver has no URL to report.
        assert_eq!(resolved.origin().label(), None);
        match resolved.provenance() {
            SourceProvenance::Resolved { reference, triggered_at } => {
                assert_eq!(reference, "chapter.tex");
                assert_eq!(triggered_at, &trigger);
            }
            other => panic!("expected Resolved provenance, got {:?}", other),
        }
    }

    #[test]
    fn map_resolver_fails_on_unknown_reference() {
        let resolver = MapResolver::new();
        let trigger = trigger_span();
        assert!(SourceResolver::<Option<String>>::resolve(&resolver, "missing.tex", &trigger)
            .is_err());
    }
}
