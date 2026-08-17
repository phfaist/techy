# List of panicking items

Parsing never panics on document input: problems in the parsed content surface as
diagnostics or as an `Err` (see [`error`](crate::error)), and every fallible operation
of the API returns a `Result`. The panicking items of the public API are exactly the two
families below; those panics guard against programming errors in calling code — no
document content can trigger them.

**Precondition asserts.** Five value functions and the seven span-taking
[`StdToken`](crate::core::StdToken) constructors document a precondition on their
arguments and panic, in all builds, when calling code violates it. These functions
are deliberately infallible (there is no error channel to prefer), the checks are
cheap, and the immediate panic keeps invalid values unrepresentable instead of
letting them cause misbehavior far from the mistake:

- [`Span::new`](crate::source::Span::new) — requires `start <= end`;
- [`Span::extend_to`](crate::source::Span::extend_to) — requires the new end not to
  precede the span's current end;
- [`SourceSpan::new`](crate::source::SourceSpan::new) — requires the range to lie within
  the source content, on `char` boundaries;
- [`SourcePos::new`](crate::source::SourcePos::new) — requires the offset to lie within
  the source content, on a `char` boundary;
- the token constructors [`StdToken::char`](crate::core::StdToken::char),
  [`group_open`](crate::core::StdToken::group_open),
  [`group_close`](crate::core::StdToken::group_close),
  [`command`](crate::core::StdToken::command),
  [`specials`](crate::core::StdToken::specials),
  [`comment`](crate::core::StdToken::comment) and
  [`paragraph_break`](crate::core::StdToken::paragraph_break) — each requires the
  documented coherence of the spans it is given: the pre-space ends exactly where the
  token's span starts; a post-space (`command`, `comment`) is a trailing sub-range of
  that span; a comment's start delimiter is a leading sub-range of it, ending no later
  than the post-space begins. The eighth constructor,
  [`end_of_stream`](crate::core::StdToken::end_of_stream), takes no span of its own and
  never panics;
- [`skip_whitespace`](crate::core::skip_whitespace) — requires `pos` to lie within the
  content, on a `char` boundary.

**Indexing-style accessors.** Accessors that follow the standard library's
slice-indexing convention: the panicking form is for ids, spans, and regions
obtained from the very tree or source in hand, and each panic is stated in a
"Panics" section on the item's own page; for values of unknown provenance, use
the non-panicking companion:

- [`NodeTree::node`](crate::core::node::NodeTree::node) — panics on an id another tree
  minted (the non-panicking companion is
  [`NodeTree::get`](crate::core::node::NodeTree::get));
- [`NodeTree::nodes_in`](crate::core::node::NodeTree::nodes_in) — panics on a range
  outside the tree's storage;
- [`Span::slice`](crate::source::Span::slice) — panics on a span invalid for the given
  content (the non-panicking companion is [`Span::get`](crate::source::Span::get));
- [`TextContent::resolve`](crate::source::TextContent::resolve) — panics on a stored
  range invalid for the given source's content (a broken invariant, which no
  parsed input can cause; every operation that resolves or materializes a tree's
  span-backed text — among others
  [`NodeRef::chars`](crate::core::node::NodeRef::chars),
  [`NodeRef::group_delimiters`](crate::core::node::NodeRef::group_delimiters),
  [`NodeRef::summary`](crate::core::node::NodeRef::summary),
  [`NodeTree::materialize`](crate::core::node::NodeTree::materialize),
  [`core_source_instruction`](crate::recompose::core_source_instruction), the preset's
  source recomposer, and the tree serialization of [`serialize`](crate::serialize) —
  reaches this panic on a consumer-built tree that breaks the invariant, and on no other
  input);
- [`ChildRegion::children`](crate::core::node::ChildRegion::children),
  [`ChildRegion::content_range`](crate::core::node::ChildRegion::content_range), and
  [`ChildRegion::content_parent`](crate::core::node::ChildRegion::content_parent) —
  panic on a staged (never finished) region, which no finished tree can contain
  (guard with [`ChildRegion::is_resolved`](crate::core::node::ChildRegion::is_resolved);
  the non-panicking companion answering the staged coordinates is
  [`ChildRegion::staged`](crate::core::node::ChildRegion::staged)).

These two families are the complete list of documented panics in the public API;
no other public item panics on documented use.

Read next: back to the [Developer Guide](crate::guide#developer-guide) index —
the other chapters on extending and embedding techy.
