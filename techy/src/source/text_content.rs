//! The node payload text type [`TextContent`]: a byte range into a source, or an owned
//! string.

use alloc::boxed::Box;
use alloc::string::String;

use super::origin::SourceOrigin;
use super::source::Source;
use super::span::Span;

/// The logical text of a node payload, stored either as a byte range into a source or as
/// an owned string.
///
/// Text that came from parsing is [`Spanned`](TextContent::Spanned): a byte range into
/// the source that the carrying node's [`SourceSpan`](super::SourceSpan) points into, so
/// no copy is made. Text that was synthesized, transformed, or normalized is
/// [`Owned`](TextContent::Owned). Either form is read with
/// [`resolve`](TextContent::resolve), which is what the node accessors of
/// [`core::node`](crate::core::node) call.
///
/// A `Spanned` value always refers into the source of its own node's `SourceSpan`. The
/// tree builder enforces this and debug builds assert it, so a transformation that
/// replaces a node's span must first turn the payload into an `Owned` value with
/// [`materialized`](TextContent::materialized).
///
/// # Equality
///
/// `TextContent` deliberately implements no `PartialEq`. Deciding whether two values
/// hold the same text requires the source content, so a structural comparison would give
/// misleading answers: `Spanned(2..4)` and `Owned("ab")` may well denote the same text.
/// Compare the resolved `&str`s instead, obtained through
/// [`resolve`](TextContent::resolve) or through the node accessors.
#[derive(Clone, Debug)]
pub enum TextContent {
    /// A byte range into the carrying node's own source; this is what parsing produces.
    Spanned(Span),
    /// Text stored directly, for synthesized, transformed, or normalized content.
    Owned(Box<str>),
}

impl TextContent {
    /// Empty content, as an [`Owned`](TextContent::Owned) value.
    pub fn empty() -> TextContent {
        TextContent::Owned(Box::from(""))
    }

    /// The logical text, resolving a [`Spanned`](TextContent::Spanned) value against
    /// `source`.
    ///
    /// `source` must be the source the span refers into, which is the carrying node's
    /// own source: in a tree spanning several sources, each node's payload resolves
    /// against its own span's source and never against an ambient one. For content read
    /// off a parsed node, that source is `node.span().source()`
    /// ([`SourceSpan::source`](super::SourceSpan::source)).
    ///
    /// An [`Owned`](TextContent::Owned) value ignores `source` entirely.
    ///
    /// # Panics
    ///
    /// Panics if a `Spanned` range is out of bounds for `source`'s content, or if either
    /// end falls inside a multi-byte character — the same contract as `&content[range]`.
    /// This means the invariant above is broken, which no parsed input can cause; it is
    /// not a recoverable condition.
    pub fn resolve<'a, O: SourceOrigin>(&'a self, source: &'a Source<O>) -> &'a str {
        match self {
            TextContent::Spanned(span) => span.slice(source.content()),
            TextContent::Owned(text) => text,
        }
    }

    /// A copy of this content holding the same text as an
    /// [`Owned`](TextContent::Owned) value.
    ///
    /// Use this before detaching a payload from the source its span points into. The
    /// `source` argument and the panic condition are those of
    /// [`resolve`](TextContent::resolve).
    pub fn materialized<O: SourceOrigin>(&self, source: &Source<O>) -> TextContent {
        match self {
            TextContent::Spanned(span) => {
                TextContent::Owned(Box::from(span.slice(source.content())))
            }
            TextContent::Owned(text) => TextContent::Owned(text.clone()),
        }
    }

    /// Whether the content is [`Owned`](TextContent::Owned).
    pub fn is_owned(&self) -> bool {
        matches!(self, TextContent::Owned(_))
    }
}

impl From<Span> for TextContent {
    fn from(span: Span) -> TextContent {
        TextContent::Spanned(span)
    }
}

impl From<&str> for TextContent {
    fn from(text: &str) -> TextContent {
        TextContent::Owned(Box::from(text))
    }
}

impl From<String> for TextContent {
    fn from(text: String) -> TextContent {
        TextContent::Owned(text.into_boxed_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(content: &str) -> Source {
        Source::new(content)
    }

    #[test]
    fn spanned_resolution() {
        let src = source("hello world");
        let tc = TextContent::from(Span::new(6, 11));
        assert!(!tc.is_owned());
        assert_eq!(tc.resolve(&src), "world");
    }

    #[test]
    fn owned_resolution_ignores_source() {
        let tc = TextContent::from("synthesized");
        assert!(tc.is_owned());
        assert_eq!(tc.resolve(&source("unrelated")), "synthesized");
    }

    #[test]
    fn materialized_preserves_logical_text() {
        let empty = source("");
        let spanned = TextContent::from(Span::new(0, 5));
        let owned = spanned.materialized(&source("hello world"));
        assert!(owned.is_owned());
        assert_eq!(owned.resolve(&empty), "hello");

        let already_owned = TextContent::from(String::from("abc")).materialized(&empty);
        assert_eq!(already_owned.resolve(&empty), "abc");
    }

    #[test]
    fn empty_is_owned_empty() {
        let tc = TextContent::empty();
        assert!(tc.is_owned());
        assert_eq!(tc.resolve(&source("")), "");
    }
}
