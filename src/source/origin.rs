//! Source origin metadata: where a source's content nominally comes from.
//!
//! The origin is *display metadata* used in diagnostics. It is distinct from
//! [`SourceProvenance`] (`super::SourceProvenance`), which structurally records how the
//! source entered the parse and points back to the triggering location.

use alloc::borrow::Cow;
use alloc::string::String;
use core::fmt::Debug;

/// Origin metadata attached to a [`Source`](super::Source).
///
/// Implementors supply a human-readable label for diagnostics. `Default` provides the
/// "unknown origin" value: sources are created with the default origin, and creators that
/// know more (e.g. a resolver that fetched content from a URL) attach it via
/// [`Source::with_origin`](super::Source::with_origin).
///
/// The default origin type is `Option<String>`: conventionally the URL the content was
/// obtained from if available, and `None` otherwise (e.g. content handed in directly as a
/// string, or content synthesized while parsing — the latter is described by the source's
/// [`SourceProvenance`](super::SourceProvenance), which every source carries). Once the
/// `Lang` trait exists (Phase 3+), a language definition selects its origin type via
/// `L::SourceOrigin`.
pub trait SourceOrigin: Debug + Clone + Default {
    /// Short human-readable label shown in diagnostics (e.g. a URL).
    ///
    /// Returns `None` if there is nothing meaningful to display; diagnostics then omit the
    /// origin bracket entirely.
    fn label(&self) -> Option<Cow<'_, str>>;
}

/// The default origin type: conventionally the URL the content was obtained from (if
/// available), `None` when unknown.
impl SourceOrigin for Option<String> {
    fn label(&self) -> Option<Cow<'_, str>> {
        self.as_deref().map(Cow::Borrowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_string_origin_labels() {
        assert_eq!(None::<String>.label(), None);
        assert_eq!(
            Some(String::from("https://example.com/main.tex")).label().as_deref(),
            Some("https://example.com/main.tex")
        );
    }

    #[test]
    fn default_origin_is_none() {
        assert_eq!(<Option<String>>::default(), None);
    }
}
