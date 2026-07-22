//! [`TextContent`]: logical textual content, span-backed or owned.

use alloc::boxed::Box;
use alloc::string::String;

use super::span::Span;

/// Logical textual content of a node payload — the *content* is first-class; a span is
/// provenance, not the content's storage (ARCHITECTURE.md [§dd-arch:nodes]).
///
/// Content that came from parsing is [`Spanned`](TextContent::Spanned) (zero-copy: a byte
/// range into the source the carrying node's `SourceSpan` refers to); content that was
/// synthesized, transformed, or normalized is [`Owned`](TextContent::Owned).
///
/// Invariant (builder-enforced, debug-asserted): a `Spanned` value refers into the source
/// of its node's own `SourceSpan`. A transform that replaces a node's span must
/// materialize its content first.
///
/// # Equality
///
/// `TextContent` deliberately implements no `PartialEq`: logical-text equality of a
/// `Spanned` value requires the source content, so a structural `==` would be a footgun
/// (`Spanned(2..4)` vs `Owned("ab")` may denote identical text). Compare resolved `&str`s
/// (via [`resolve`](TextContent::resolve) or node-level accessors) instead.
#[derive(Clone, Debug)]
pub enum TextContent {
    /// A byte range into the carrying node's own source — zero-copy parser output.
    Spanned(Span),
    /// Content stored directly — synthesized, transformed, or normalized.
    Owned(Box<str>),
}

impl TextContent {
    /// Empty owned content.
    pub fn empty() -> TextContent {
        TextContent::Owned(Box::from(""))
    }

    /// The logical text, resolving a [`Spanned`](TextContent::Spanned) value against the
    /// content of the source it refers into.
    ///
    /// # Panics
    ///
    /// Panics if a `Spanned` range is out of bounds for `source_content` or not on `char`
    /// boundaries (same contract as `&source_content[range]`) — a broken invariant, not a
    /// recoverable condition.
    pub fn resolve<'a>(&'a self, source_content: &'a str) -> &'a str {
        match self {
            TextContent::Spanned(span) => span.slice(source_content),
            TextContent::Owned(text) => text,
        }
    }

    /// An always-[`Owned`](TextContent::Owned) copy with the same logical text
    /// (see [`resolve`](TextContent::resolve) for the `source_content` contract).
    pub fn materialized(&self, source_content: &str) -> TextContent {
        match self {
            TextContent::Spanned(span) => TextContent::Owned(Box::from(span.slice(source_content))),
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

    #[test]
    fn spanned_resolution() {
        let content = "hello world";
        let tc = TextContent::from(Span::new(6, 11));
        assert!(!tc.is_owned());
        assert_eq!(tc.resolve(content), "world");
    }

    #[test]
    fn owned_resolution_ignores_source() {
        let tc = TextContent::from("synthesized");
        assert!(tc.is_owned());
        assert_eq!(tc.resolve("unrelated"), "synthesized");
    }

    #[test]
    fn materialized_preserves_logical_text() {
        let content = "hello world";
        let spanned = TextContent::from(Span::new(0, 5));
        let owned = spanned.materialized(content);
        assert!(owned.is_owned());
        assert_eq!(owned.resolve(""), "hello");

        let already_owned = TextContent::from(String::from("abc")).materialized("");
        assert_eq!(already_owned.resolve(""), "abc");
    }

    #[test]
    fn empty_is_owned_empty() {
        let tc = TextContent::empty();
        assert!(tc.is_owned());
        assert_eq!(tc.resolve(""), "");
    }
}
