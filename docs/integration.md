# Integration: tooling, embedding, and bindings

This chapter collects the facts that matter when techy runs inside a larger
system — an editor tool, a service, a WebAssembly host, or bindings to
another programming language. It is deliberately a pointer chapter: each
finding is a few sentences plus the API items that carry the full contract.
The general platform facts (`no_std`, no input/output of its own, thread
safety as documented auto traits) are in the
[Introduction](crate::guide::introduction#where-techy-runs).

## Embedding and bindings

**Hold trees and ids, not node references.** A
[`NodeRef`](crate::core::node::NodeRef) is a `Copy` proxy that *borrows* its
tree — by construction it cannot outlive the storage, which also means a
binding object cannot store one. The persistent handle is the pair: a shared
tree plus a [`NodeId`](crate::core::node::NodeId). Trees are immutable and
`Send + Sync`, so an embedder shares one as `Arc<NodeTree>`; a `NodeId` is
an 8-byte `Copy` value that re-resolves through the tree in hand —
[`NodeTree::node`](crate::core::node::NodeTree::node) for ids the tree
minted, [`NodeTree::get`](crate::core::node::NodeTree::get) for ids of
unknown provenance. Every id carries its tree layout's tag, so a stale or
foreign id is detected in every build instead of silently resolving to the
wrong node ([`TreeTag`](crate::core::node::TreeTag)).

**Visitor callbacks need not be thread-safe.** The traversal and
transformation drivers run their callbacks synchronously on the calling
thread, and their traits deliberately carry **no `Send`/`Sync` bounds** —
[`NodeVisitor`](crate::visit::NodeVisitor),
[`RestageVisitor`](crate::transform::RestageVisitor), and
[`annotate`](crate::core::node::NodeTree::annotate) callbacks alike; the
documented rationale is exactly the embedding case: a bound would wall off
single-threaded foreign-function callbacks. For an embedder this cuts both
ways: callback objects from a binding language need no thread-safety
wrapper, and a running walk is not something to hand to another thread.

**Match `Severity` exhaustively.** [`Severity`](crate::error::Severity) is
a three-variant exhaustive enum (`Note < Warning < Error` by its derived
ordering, for threshold filtering) — a binding can map it exhaustively, with
no wildcard arm hiding future variants. Its documentation notes the core
parser currently emits only `Error`; `Note` and `Warning` exist for presets
and embedders, so map all three.

**Synthesizing nodes after the parse.** Post-parse processing that
fabricates nodes (a transform inserting material the source never had) must
give them coherent recorded parsing states. The documented recipe: feed the
preset's pillar functions the same inputs the parse-time driver feeds them
— with
[`ParsingStateStack::from_node_ancestors`](crate::core::ParsingStateStack::from_node_ancestors)
recovering the enclosing-state stack from a parsed node, no parse session
anywhere. The pillar documentation carries the details (for a synthesized
math interior, [`math_group_interior_delta`](crate::latexlike::math_group_interior_delta)
states the two components to apply;
[`exit_math_context_delta`](crate::latexlike::exit_math_context_delta)
takes the recovered stack directly).

**Streaming recomposition.** The [`recompose`](crate::recompose) fold
returns a value; there is deliberately no sink type in the machinery. To
stream output instead of building a string, the documented pattern is a
recomposer that holds its writer in its own `&mut self` and composes
`Piece = ()` — it writes as nodes are entered, and the unit pieces compose
for free. See the "streaming" section of the
[`recompose`](crate::recompose) module documentation.

## Tooling starting points

**Navigation by position.** The two position queries on
[`NodeTree`](crate::core::node::NodeTree):
[`node_at`](crate::core::node::NodeTree::node_at) answers a point query
with the *deepest* node whose span contains the position (a position on a
group's delimiter or a callable's trigger spelling resolves to that node;
multi-source trees are answered per source), and
[`covering_slice`](crate::core::node::NodeTree::covering_slice) answers a
span query with the minimal covering sibling run. Ancestors of either
answer come free via [`NodeRef::parent`](crate::core::node::NodeRef::parent).
Each method's page states the exact containment and multi-source rules.

**Line/column positions are the consumer's.** Parsing works purely in byte
offsets; line/column is a display concern, computed on demand and owned by
whoever needs it. The persistent form is the consumer-held
[`LineIndexCache`](crate::source::LineIndexCache) — one owned line-starts
table per source, keyed by source identity, never invalidated (content is
immutable) — which is also the natural bindings-side handle for repeated
line/column queries and diagnostic rendering (the `_with` rendering entry
points accept it). The seam under it is
[`LineColProvider`](crate::source::LineColProvider): editor tools with
their own incremental line tables implement the trait and plug into the
same rendering entry points without recomputation. One documented bound to
know: content longer than the configured scan cap (default 500 000 bytes,
[`set_max_scan_len`](crate::source::LineIndexCache::set_max_scan_len)) is
not indexed — queries answer `None` and renderers fall back to raw byte
positions.

**Re-parses and span stability.** To correlate positions across parses —
an editor re-parsing on every keystroke, a tool diffing two parse attempts
— hold your own `Arc<Source>` and parse with
[`parse_source`](crate::core::Language::parse_source), never
[`parse`](crate::core::Language::parse): `parse` mints a fresh anonymous
[`Source`](crate::source::Source) on every call, and source comparisons
are **identity-based** ([`SourceSpan`](crate::source::SourceSpan) and
[`SourcePos`](crate::source::SourcePos) equality compare the `Arc`, not
the text), so positions from two `parse` calls never correlate even on
identical content. Holding the source also keeps a
[`LineIndexCache`](crate::source::LineIndexCache) entry valid across
attempts for free — its documentation calls this the span-stability
doctrine.

Read next: [Migrating from pylatexenc](crate::guide::pylatexenc_migration)
— the concept mappings for readers arriving from the Python library.
