//! [`TextContent`]: logical textual content, span-backed or owned.

use alloc::boxed::Box;
use alloc::string::String;

use super::origin::SourceOrigin;
use super::source::Source;
use super::span::Span;

/// Logical textual content of a node payload — the *content* is first-class; a span is
/// provenance, not the content's storage.
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
/// `Spanned` value requires the source content, so a structural `==` would give
/// misleading answers (`Spanned(2..4)` vs `Owned("ab")` may denote identical text).
/// Compare resolved `&str`s (via [`resolve`](TextContent::resolve) or node-level
/// accessors) instead.
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

    /// The logical text, resolving a [`Spanned`](TextContent::Spanned) value against
    /// `source` — the source the span refers into (the carrying node's **own**
    /// source, per the `Spanned` invariant: in a multi-source tree each node's
    /// payload resolves against its own span's source, never an ambient one).
    /// For content read off a parsed node, that source is
    /// `node.span().source()` ([`SourceSpan::source`](super::SourceSpan::source)).
    ///
    /// # Panics
    ///
    /// Panics if a `Spanned` range is out of bounds for `source`'s content or not on
    /// `char` boundaries (same contract as `&content[range]`) — a broken invariant,
    /// not a recoverable condition.
    pub fn resolve<'a, O: SourceOrigin>(&'a self, source: &'a Source<O>) -> &'a str {
        match self {
            TextContent::Spanned(span) => span.slice(source.content()),
            TextContent::Owned(text) => text,
        }
    }

    /// An always-[`Owned`](TextContent::Owned) copy with the same logical text
    /// (see [`resolve`](TextContent::resolve) for the `source` contract).
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
