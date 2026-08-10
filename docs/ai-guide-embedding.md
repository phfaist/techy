# AI guide: embedding

Condensed reference: embedding techy in a larger system — bindings to other
programming languages, threading facts, multi-source parsing, tooling entry
points, `no_std` builds, streaming output. Compressed from
[Integration: tooling, embedding, and bindings](crate::guide::integration)
and the [Introduction](crate::guide::introduction#where-techy-runs) (the
full chapters). Terms: a parse produces an immutable
[`NodeTree`](crate::core::node::NodeTree), read through borrowed
[`NodeRef`](crate::core::node::NodeRef) proxies; a
[`Source`](crate::source::Source) owns one unit of source content, shared
as `Arc<Source>`; a **span** ([`SourceSpan`](crate::source::SourceSpan))
is a byte range paired with its `Arc<Source>`.

## Bindings and threading facts

| Fact | Detail |
|---|---|
| Hold trees + ids, not node references | [`NodeRef`](crate::core::node::NodeRef) borrows its tree and cannot be stored in a binding object. The persistent handle is `Arc<NodeTree>` + [`NodeId`](crate::core::node::NodeId) (8-byte `Copy`); re-resolve via [`NodeTree::node`](crate::core::node::NodeTree::node) (ids the tree minted) or [`NodeTree::get`](crate::core::node::NodeTree::get) (unknown provenance). Ids carry their tree layout's tag ([`TreeTag`](crate::core::node::TreeTag)): stale/foreign ids are detected in every build, never silently resolve wrong; pre-check with [`NodeId::tree_tag`](crate::core::node::NodeId::tree_tag) vs [`NodeTree::tree_tag`](crate::core::node::NodeTree::tree_tag). Whole trees check with [`validate_tree`](crate::core::node::validate_tree); its [`TreeViolation`](crate::core::node::TreeViolation) reports have a public [`new`](crate::core::node::TreeViolation::new) for testing handlers |
| Trees are `Send + Sync` | Trees are immutable; share one `Arc<NodeTree>` across threads. Per-type thread-safety facts are the documented auto-trait (`Send`/`Sync`) listings on each rustdoc page |
| Visitor callbacks need not be thread-safe | [`NodeVisitor`](crate::visit::NodeVisitor), [`RestageVisitor`](crate::transform::RestageVisitor), and [`annotate`](crate::core::node::NodeTree::annotate) callbacks run synchronously on the calling thread and deliberately carry **no `Send`/`Sync` bounds** (a bound would wall off single-threaded foreign-function callbacks). Cuts both ways: no thread-safety wrapper needed, and a running walk is not handed to another thread |
| Match `Severity` exhaustively | [`Severity`](crate::error::Severity) is a three-variant exhaustive enum, `Note < Warning < Error` by derived ordering — map all three, no wildcard arm. The core parser currently emits only `Error`; `Note`/`Warning` exist for presets and embedders |
| Synthesizing nodes post-parse | Fabricated nodes need coherent recorded parsing states: feed the preset's behavior functions the same inputs the parse-time driver feeds them, with [`ParsingStateStack::from_node_ancestors`](crate::core::ParsingStateStack::from_node_ancestors) recovering the enclosing-state stack from a parsed node — no parse session anywhere. Details: [`math_group_interior_delta`](crate::latexlike::math_group_interior_delta), [`exit_math_context_delta`](crate::latexlike::exit_math_context_delta) |
| Never panics on document input | Problems surface as [`Diagnostics`](crate::error::Diagnostics) or `Err` ([`ParseError`](crate::error::ParseError)); every fallible operation returns a `Result` |
| Runtime-identified conditions | Conditions defined in the host language ride one Rust adapter type overriding the defaulted [`DiagnosticInfo::identifier()`](crate::error::DiagnosticInfo::identifier) per instance — the documented binding-adapter exception; every ordinary condition keeps the compile-time `IDENTIFIER` const |

## Multi-source parsing and include policy

techy performs no input/output of its own. `\input`-like content lookup is
delegated to the embedder through
[`SourceResolver`](crate::source::SourceResolver) (configured on the
driver; unconfigured = nothing resolves); resolved content parses into the
same tree at the inclusion point, so one tree can span several sources.
Every source records its
[`SourceProvenance`](crate::source::SourceProvenance) (primary / resolved
/ synthesized) with a back-reference to the triggering span — a provenance
chain walkable for error reporting. Recursion and cycle policy stays with
the embedder:
[`Source::including_sources`](crate::source::Source::including_sources)
and [`check_include_chain`](crate::source::check_include_chain) are the
ready-made policy tools (cycle + depth check). Wiring recipe and the
in-memory [`MapResolver`](crate::source::MapResolver): see
[AI guide: definitions](crate::guide::ai_guide_definitions) and the
filesystem recipe in
[the specs chapter](crate::guide::specs#resolving-external-sources-input-like-inclusion).

## Tooling entry points

**Navigation by position** (each method's page has the exact containment
and multi-source rules):
[`NodeTree::node_at`](crate::core::node::NodeTree::node_at) — point query,
deepest node whose span contains the position (a position on a delimiter
or trigger spelling resolves to that node; answered per source);
[`NodeTree::covering_slice`](crate::core::node::NodeTree::covering_slice)
— span query, minimal covering sibling run; ancestors via
[`NodeRef::parent`](crate::core::node::NodeRef::parent).

**Line/column is the consumer's.** Parsing works purely in byte offsets;
line/column is display-time. The persistent form is the consumer-held
[`LineIndexCache`](crate::source::LineIndexCache) — one line-starts table
per source, keyed by source identity, never invalidated (content is
immutable) — also the natural bindings-side handle; the diagnostics
`_with` rendering entry points (e.g.
[`render_all_with`](crate::error::Diagnostics::render_all_with)) accept
it. Editor tools with their own incremental line tables implement
[`LineColProvider`](crate::source::LineColProvider) and plug into the same
entry points. Bound: content longer than the scan cap (default
500 000 bytes,
[`set_max_scan_len`](crate::source::LineIndexCache::set_max_scan_len)) is
not indexed — queries answer `None`, renderers fall back to byte
positions.

**Re-parses and span stability.** To correlate positions across parses
(re-parse per keystroke, diffing parse attempts): hold your own
`Arc<Source>` and call
[`parse_source`](crate::core::Language::parse_source), never
[`parse`](crate::core::Language::parse) — `parse` mints a fresh anonymous
[`Source`](crate::source::Source) per call, and source comparisons are
**identity-based** ([`SourceSpan`](crate::source::SourceSpan) /
[`SourcePos`](crate::source::SourcePos) equality compares the `Arc`, not
the text), so positions from two `parse` calls never correlate even on
identical content. Holding the source also keeps the
[`LineIndexCache`](crate::source::LineIndexCache) entry valid across
attempts.

```rust
use std::sync::Arc;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};
use techy::source::{LineIndexCache, Source};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial().expect("seed state"),
);

// Hold the source yourself; parse it; spans now correlate across parses.
let source = Arc::new(Source::new("first line\nsee {here}").with_origin(
    Some("main.tex".to_string()),
));
let result = language.parse_source(Arc::clone(&source)).unwrap();

// Line/column through a consumer-held cache (each source indexed once, ever):
let mut line_cols = LineIndexCache::new();
let node = result.tree.root().child(1).unwrap();
assert_eq!(node.span_content(), "{here}");
assert_eq!(
    line_cols.line_col(node.span().source(), node.span().start()),
    Some((2, 5)),
);

// A second parse of the SAME Arc<Source> yields correlating spans:
let again = language.parse_source(Arc::clone(&source)).unwrap();
assert_eq!(
    again.tree.root().child(1).unwrap().span(),
    node.span(),
);
```

## `no_std` and WebAssembly

The crate is `no_std`-friendly: it depends only on `core` and `alloc`
(sources are shared as `Arc`, so the target must support atomics) and
performs no input/output of its own — the host supplies all input. This
makes it suitable for constrained targets, including WebAssembly builds,
and for backing a Python extension module written in Rust (for example
with PyO3) like any other Rust dependency.

## Streaming recomposition

[`TreeRecomposer`](crate::recompose::TreeRecomposer) (the tree→value fold — see
[AI guide: node trees](crate::guide::ai_guide_trees)) returns a value;
there is deliberately no sink type. To stream output instead of building a
string: a recomposer that holds its writer in its own `&mut self` and
composes `Piece = ()` — it writes as nodes are entered, and the unit
pieces compose for free. See the streaming section of the
[`recompose`](crate::recompose) module documentation.
