//! Source origin metadata: where a source's content nominally comes from.
//!
//! The origin is display metadata, shown in diagnostics. It is distinct from
//! [`SourceProvenance`](super::SourceProvenance), which records how the source entered
//! the parse and points back at the triggering location.

use alloc::borrow::Cow;
use alloc::string::String;
use core::fmt::Debug;

/// Origin metadata attached to a [`Source`](super::Source): a human-readable label for
/// diagnostics.
///
/// Every source is created with the default origin, which stands for "origin unknown";
/// whoever knows more — a resolver that fetched the content from a URL, say — attaches it
/// with [`Source::with_origin`](super::Source::with_origin).
///
/// The default origin type is `Option<String>`, conventionally the URL the content was
/// obtained from, and `None` when there is none: content handed in directly as a string,
/// or content synthesized while parsing. What such a source *is* remains described by its
/// [`SourceProvenance`](super::SourceProvenance). A language definition selects the origin
/// type it uses through its own associated type.
///
/// The `Send + Sync` bounds are required because sources are shared as `Arc<Source>`,
/// matching the thread-safety contract of the other stored extension traits.
pub trait SourceOrigin: Debug + Clone + Default + Send + Sync {
    /// A short human-readable label shown in diagnostics, such as a URL.
    ///
    /// Returns `None` if there is nothing meaningful to display; diagnostics then omit the
    /// origin bracket entirely.
    fn label(&self) -> Option<Cow<'_, str>>;
}

/// The default origin type: conventionally the URL the content was obtained from, and
/// `None` when unknown.
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
