# Source Management & AST Architecture

This document summarizes the design decisions for techy's source management,
AST representation, and lifetime architecture. It is the result of a detailed
design discussion (March 2026).

---

## Overview

The architecture addresses seven concerns:

1. **Source content abstraction** — the parser doesn't know where content comes from
2. **Pluggable source resolution** — users customize content lookup (files, URLs, databases, or nothing)
3. **Source location tracking** — byte offsets during parsing, line/column on demand for display
4. **Dynamic source creation** — macro expansion and `\input{}` produce new sources mid-parse, with provenance tracking
5. **Rust-idiomatic content reading** — cursor over `&str` with mark/rewind, not byte-level streaming
6. **Clean lifetime model** — minimal lifetime parameters, self-contained nodes
7. **Post-processing support** — tree transformations produce new trees without copying sources or creating lifetime chains

---

## Core Types and Ownership

### Ownership Hierarchy

```
FLMEnvironment                         (long-lived, reusable across parses)
 |  owns: macro/env specs, parse settings
 |  specs are Arc-wrapped for sharing with nodes
 |
 +---> ParserSession<'env>             (transient, exists during one parse)
 |      borrows: &'env FLMEnvironment
 |      creates: Arc<Source> for each source
 |      builds: NodeTree (mutable)
 |      consumed by .finish() to produce:
 |
 +---> ParseResult<'env>              (immutable result of one parse)
 |      owns: NodeTree
 |      borrows: &'env FLMEnvironment
 |      (sources are owned by Arc in each node's SourceSpan)
 |
 +---> NodeRef<'pr>                   (lightweight proxy, Copy)
        borrows: &'pr ParseResult
        resolves node indices; source content accessed via Arc
```

### FLMEnvironment — reusable parse configuration

Stores macro/environment/specials specifications, parse settings, and any
other configuration that is shared across multiple parse runs. Does **not**
store sources or parse results, so it accumulates no memory across runs.

```rust
pub struct FLMEnvironment {
    // macro specs, environment specs, specials specs, settings, ...
    // Specs are Arc-wrapped for sharing with nodes.
}
```

Lifetime: controlled by the user. Typically lives for the duration of an
application or processing session.

### ParseResult — one parse's output

Owns the node tree produced by a single parse. Borrows from
`FLMEnvironment` so that spec lookups remain available during AST analysis.
Sources are not owned here — they are carried by each node's `SourceSpan`
via `Arc<Source>`.

```rust
pub struct ParseResult<'env> {
    env: &'env FLMEnvironment,
    nodes: NodeTree,
    root: NodeIndex,
}
```

When `ParseResult` is dropped, nodes are freed. Sources are freed when
no remaining `SourceSpan` (in any tree, original or transformed) references
them.

### NodeRef — the user-facing proxy

Users never interact with raw node data. They receive `NodeRef` values —
lightweight, `Copy` proxies that resolve internal node indices through the
`ParseResult`:

```rust
#[derive(Copy, Clone)]
pub struct NodeRef<'pr> {
    result: &'pr ParseResult<'pr>,
    index: NodeIndex,
}

impl<'pr> NodeRef<'pr> {
    pub fn span_content(&self) -> &'pr str { ... }
    pub fn source_origin(&self) -> &'pr str { ... }
    pub fn children(&self) -> impl Iterator<Item = NodeRef<'pr>> { ... }
    pub fn kind(&self) -> &'pr NodeKind { ... }
    pub fn macro_spec(&self) -> Option<&'pr MacroSpec> { ... }
    pub fn parsing_state(&self) -> &'pr ParsingState { ... }
}
```

This gives ergonomic, safe access to source content, provenance, specs, and
tree structure — all through method calls on a Copy type.

---

## Node Representation

### NodeTree — flat, immutable node storage

The AST is stored as a flat `Vec<NodeData>`. This is cache-friendly, avoids
per-node heap allocation, and makes the tree trivially serializable. Tree
structure is encoded via index ranges (each node stores the index range of its
children in the same vec).

```rust
struct NodeData {
    kind: NodeKind,
    span: SourceSpan,                    // Arc<Source> + start + end
    children: Range<u32>,
    parsing_state: Arc<ParsingState>,
    // For macro/env nodes:
    // spec: Arc<MacroSpec> / Arc<EnvironmentSpec> / etc.
}
```

After parsing, the NodeTree is frozen (moved from `ParserSession` into
`ParseResult`). No mutation is possible through `NodeRef`.

Node indices are internal to the `NodeTree` and only resolved through
`NodeRef`, which always has access to the `ParseResult`. Rust's borrow
checker enforces that `NodeRef` cannot outlive `ParseResult`, so indices
are always valid when accessed.

---

## Source Management

### Source and SourceSpan — Arc-based ownership

Each source is wrapped in `Arc<Source>` and referenced directly by node spans.
This makes nodes self-contained: a node's `SourceSpan` can resolve its content
without any external store or lookup table.

```rust
pub struct Source {
    content: String,   // or other backing (see SourceContent trait)
    origin: String,
    provenance: SourceProvenance,
    line_offset: usize,
    column_offset: usize,
}

#[derive(Clone)]
pub struct SourceSpan {
    source: Arc<Source>,
    start: usize,
    end: usize,
}

impl SourceSpan {
    pub fn content(&self) -> &str {
        &self.source.content[self.start..self.end]
    }

    pub fn origin(&self) -> &str {
        &self.source.origin
    }
}
```

During parsing, the `ParserSession` creates `Arc<Source>` instances as sources
are loaded or synthesized. Nodes clone the `Arc` (cheap atomic increment).
Most nodes in a single parse share the same `Arc<Source>`, so the refcount
overhead is minimal.

### Why Arc for sources

The decisive factor is **post-processing**. Tree transformations produce
new trees that mix nodes from the original tree (unchanged) with new or
modified nodes. Without `Arc`:

- **Index-based sources** tie nodes to a specific `SourceStore`. A
  transformed tree would need to copy the store, or borrow it (creating
  a lifetime dependency chain across transformations). After N transforms,
  all N intermediate results must stay alive.
- **Lifetime-based references** have the same chaining problem, plus the
  self-referential struct issue (nodes can't borrow from a sibling
  `SourceStore` field in the same struct).

With `Arc<Source>`, transformed trees are independent. Nodes from the
original tree carry their source references with them. The original
`ParseResult` can be dropped while the transformed tree lives on.
Multiple independent transformations of the same tree each work without
coordination. Source content is shared (not copied) via the Arc.

The cost is ~1ns atomic increment per node during parsing (for cloning the
span's Arc), which is negligible compared to the architectural simplicity
it provides for post-processing.

### SourceProvenance — tracking where sources come from

```rust
pub enum SourceProvenance {
    /// Top-level source provided directly by the user.
    Primary,

    /// Resolved from an external reference (e.g., \input{file.tex}).
    Resolved {
        reference: String,
        triggered_at: SourceSpan,
    },

    /// Synthesized during parsing (e.g., macro expansion).
    Synthesized {
        description: String,
        triggered_at: SourceSpan,
    },
}
```

The `triggered_at` field holds a `SourceSpan` (with its own `Arc<Source>`)
pointing back to the source location that caused the new source to be
created. This forms a provenance tree (never a cycle — each new source
points to an older one) that can be walked for error reporting:

```
Error at <macro expansion>:1:5
  expanded from main.tex:42:1 (\mycommand)
    included from document.tex:10:1 (\input{main.tex})
```

### SourceResolver — pluggable content lookup

```rust
pub trait SourceResolver {
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan,
    ) -> Result<Arc<Source>, ResolveError>;
}
```

Implementations:

- **`FileResolver`** — reads files from a base directory
- **`NoResolver`** — always fails; for no-I/O / no-filesystem builds
- **`MapResolver`** — looks up content from a `HashMap` (testing, databases)
- Users implement custom resolvers for URLs, databases, etc.

The parser is generic over the resolver. `NoResolver` is a zero-sized type,
so a parser with no resolution capability has zero overhead from this
abstraction.

The resolver returns `Arc<Source>` directly — no intermediate store needed.

---

## Content Reading

### SourceContent trait — abstraction over backing storage

```rust
pub trait SourceContent {
    fn slice(&self, offset: usize, len: usize) -> &str;
    fn total_len(&self) -> Option<usize>;
}
```

Implemented for `String` (in-memory content) and, in the future, for
memory-mapped files. This is an internal trait — users interact with cursors,
not raw content.

### SourceCursor — parsing reads through a cursor

```rust
pub struct SourceCursor<'s, C: SourceContent> {
    source: Arc<Source>,
    content: &'s C,
    pos: usize,
}

impl<'s, C: SourceContent> SourceCursor<'s, C> {
    pub fn peek(&self, n: usize) -> &str { ... }
    pub fn advance(&mut self, n: usize) { ... }
    pub fn mark(&self) -> usize { self.pos }
    pub fn rewind(&mut self, mark: usize) { self.pos = mark; }
    pub fn is_eof(&self) -> bool { ... }
}
```

The cursor provides forward scanning with mark/rewind for small backtracks
(e.g., tentatively trying to parse `[` as an optional argument delimiter).
This is appropriate for an FLM parser that needs limited lookahead, not a
full random-access model.

The `'s` lifetime is ephemeral — it lives only for the duration of parsing
one source unit. It does not propagate into the AST.

### Memory-mapped files (future)

For huge files, the `SourceContent` trait can be implemented for a
memory-mapped file (`Mmap` from the `memmap2` crate). Key properties:

- **No RAM = file size requirement**: The OS maps file pages into virtual
  address space on demand (typically 4KB pages), evicts under memory pressure,
  and the file on disk serves as the backing store. A 2GB file may have only
  a few MB resident.
- **Sequential access pattern is ideal**: The parser reads forward with
  occasional small backtracks, causing sequential page access with natural
  OS prefetching.
- **Transparent to the parser**: The cursor and token reader use the same
  `SourceContent` interface regardless of whether the backing is `String`
  or `Mmap`.

Deferred until needed — `String` is sufficient initially, and the trait
boundary ensures mmap support can be added without changing parser code.

---

## Line/Column Analysis

Line and column information is computed lazily and only for display (error
messages, diagnostics). The parser works purely with byte offsets.

```rust
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(content: &str) -> Self { ... }
    pub fn line_col(&self, offset: usize, line_offset: usize, col_offset: usize) -> (usize, usize) { ... }
}
```

This is a standalone utility, decoupled from `Source`. It can be computed
on demand per source when formatting errors, or cached in `ParseResult` if
needed frequently.

---

## Arc-Wrapped Data in Nodes: Sources, Specs, and ParsingState

### Problem

Certain data must be captured at parse time in node data because it cannot
be reconstructed or looked up after the fact:

- **Sources** (via `SourceSpan`): Nodes must be able to resolve their
  source content independently, especially after tree transformations
  produce new trees detached from the original `ParseResult`.
- **Specs** (`MacroSpec`, `EnvironmentSpec`, etc.): The active spec depends
  on the local parsing state (e.g., a macro redefined inside a group). A
  name-based lookup at access time would find the wrong spec.
- **ParsingState**: Records the parsing context at the point the node was
  parsed (math mode, active definitions, etc.). Multiple nodes parsed under
  the same state share the same instance; a new instance is created only
  when the state changes.

All three face the same ownership constraints: they can't borrow from
`ParseResult` sibling fields (self-referential struct), and they need to
survive independently across tree transformations.

### Decision: `Arc` for sources, specs, and parsing state

All are wrapped in `Arc` and stored directly in node data:

- **Sources**: Each `SourceSpan` holds `Arc<Source>`. During parsing, most
  nodes share the same Arc (one refcount bump per node). Transformed trees
  carry source references with them.
- **Specs**: `FLMEnvironment` stores `Arc<MacroSpec>`, `Arc<EnvironmentSpec>`,
  etc. Dynamically-created specs (from `\newcommand` etc.) are also
  `Arc`-wrapped. Nodes clone the `Arc` at parse time.
- **ParsingState**: The parser creates a new `Arc<ParsingState>` only at
  state transitions (entering math mode, applying a state delta, etc.) and
  reuses the same `Arc` for all nodes parsed under that state.

`Arc` is justified for all three because:

- All have genuinely shared ownership (environment or parser session +
  multiple nodes + potentially multiple trees after transformation).
- Some instances are created dynamically during parsing.
- Creation is infrequent (once per source/definition/state transition,
  not once per node), so the Arc overhead is negligible.
- No lifetime parameter needed on node data.

---

## Preventing Arc Cycles

### The risk

The current design is acyclic: sources point to older sources (via
`triggered_at: SourceSpan` in provenance), nodes point to sources (via
`SourceSpan`), and nothing points back. But a future refactor could
introduce a cycle — for instance, if a `Source` gained a reference to the
node that triggered its creation. `Arc` cycles are memory leaks in Rust
(reference counts never reach zero).

### Invariant: source types never reference node types

The rule is structural: **`Source`, `SourceSpan`, and `SourceProvenance`
may only reference other sources, never nodes.** This makes cycles
impossible by type definition — you can verify it by inspecting the
fields of `Source` and `SourceProvenance`.

The reference graph is strictly layered:

```
Nodes ──→ Sources (via SourceSpan)
Nodes ──→ Specs (via Arc<MacroSpec>, etc.)
Nodes ──→ ParsingState (via Arc<ParsingState>)
Sources ──→ Sources (via SourceSpan in provenance)
```

No arrows point from sources back to nodes, and no arrows point from
specs or parsing state to nodes or sources. This makes cycles impossible
regardless of how many `Arc`s are involved.

### What if you need "which node triggered this source"?

Store the triggering node's **span** (`SourceSpan`), not the node itself.
A `SourceSpan` identifies a location in a source — it points to a
`Source`, not a `Node`, so no cycle is created. To find the actual node,
search the node tree for nodes whose span covers that location. This is
an O(n) traversal but only needed for diagnostics, not hot paths.

### Why not `Weak<T>`?

Rust's `Weak<T>` (non-owning companion to `Arc<T>`) is the standard tool
for breaking cycles. It's not applicable here because nodes live in a flat
`Vec<NodeData>` inside `NodeTree`, not behind individual `Arc`s — there's
no `Arc<NodeData>` to create a `Weak` to. The layered reference graph is
a simpler and more robust solution.

---

## Post-Processing

Tree transformations and analysis are first-class operations. The `Arc`-based
source ownership is a direct consequence of this requirement.

### Key properties

- **Immutable trees**: The `NodeTree` in a `ParseResult` is immutable.
  Transformations produce a new `ParseResult` with a new `NodeTree`.
- **Self-contained nodes**: Because `SourceSpan`, specs, and parsing state
  are all `Arc`-wrapped, nodes copied from an old tree into a new tree
  carry all their context with them. No dependency on the original
  `ParseResult`.
- **Independent results**: A transformed `ParseResult` can outlive the
  original. Multiple independent transformations of the same tree each
  produce self-contained results. No lifetime chains.
- **Mixed-origin trees**: A transformation could combine nodes from
  different parse runs (e.g., merging documents). `Arc<Source>` handles
  this naturally since each node carries its own source reference.
- **Arbitrary output**: Post-processing can also produce non-tree outputs
  (HTML, JSON, analysis results, etc.) by walking the tree via `NodeRef`.

The specific APIs for tree transformation and visitor patterns are still
under design.

---

## Mutation During Parsing vs. Immutability After

During parsing, the `ParserSession` builds the `NodeTree` mutably and
creates `Arc<Source>` instances as sources are loaded or synthesized.

After parsing, `ParserSession.finish()` consumes the session and produces
an immutable `ParseResult`:

```rust
impl<'env> ParserSession<'env> {
    pub fn finish(self, root: NodeIndex) -> ParseResult<'env> {
        ParseResult {
            env: self.env,
            nodes: self.nodes,
            root,
        }
    }
}
```

The parser is consumed. No mutable/immutable borrow conflict. The result
is frozen.

---

## Typical Usage

```rust
// Long-lived configuration
let env = FLMEnvironment::new(/* specs, settings */);

// Parse run 1
let result = env.parse(input)?;
let root = result.root();
for child in root.children() {
    println!("{}: {}", child.kind(), child.span_content());
}
// result dropped — nodes freed, sources freed if no other references

// Parse run 2 — env reused, no accumulated memory
let result2 = env.parse(other_input)?;
```

---

## Generics and User Customizability

### Principle

Many types presented in this document with concrete types should in practice
be **generic**, allowing users to customize the parser and AST for their
specific use case. This is a core design goal: techy is a toolkit for
LaTeX-like languages, not a fixed LaTeX parser.

### Generic shared pointer type

The document uses `Arc<T>` throughout, but users who don't need thread
safety should be able to use `Rc<T>` instead (avoiding atomic operations).
The shared pointer type should be generic, abstracted behind a trait:

```rust
pub trait SharedPointer: Clone {
    type Pointer<T>: Clone + Deref<Target = T>;
    fn new<T>(value: T) -> Self::Pointer<T>;
}

// Provided implementations
pub struct UseArc;
impl SharedPointer for UseArc {
    type Pointer<T> = Arc<T>;
    fn new<T>(value: T) -> Arc<T> { Arc::new(value) }
}

pub struct UseRc;
impl SharedPointer for UseRc {
    type Pointer<T> = Rc<T>;
    fn new<T>(value: T) -> Rc<T> { Rc::new(value) }
}
```

Types that hold shared pointers are then parameterized:

```rust
pub struct SourceSpan<P: SharedPointer = UseArc> {
    source: P::Pointer<Source>,
    start: usize,
    end: usize,
}
```

The default (`UseArc`) means most users don't need to think about it.
Single-threaded users opt into `UseRc` for a small performance gain.

### Generic node data

The `NodeKind` enum and `NodeData` struct should be generic, allowing users
to:

- **Add custom node variants** for language-specific constructs beyond
  what techy provides out of the box.
- **Attach custom data** to nodes (e.g., semantic annotations, type
  information, rendering hints).
- **Use custom spec types** for domain-specific macro/environment
  definitions.

The exact mechanism (trait-based, enum extension, generic associated types)
is still under design. The key constraint is that the flat `Vec<NodeData>`
storage and `NodeRef` proxy must remain efficient regardless of the
customization.

### Other generic candidates

The following types are also candidates for generics, to be evaluated
during implementation:

- **`Source`**: Generic over content backing (`SourceContent` trait —
  already planned).
- **`ParsingState`**: Users may need custom state fields for
  domain-specific parsing context.
- **`MacroSpec` / `EnvironmentSpec`**: Users may need custom spec
  fields or behavior.
- **`FLMEnvironment`**: Parameterized over the above generics.
- **`ParseResult`**, **`NodeRef`**: Inherit generic parameters from
  the types they contain.

### Trade-off: ergonomics vs flexibility

Heavy use of generics can make type signatures unwieldy. Mitigation
strategies:

- **Defaults on all generic parameters** — most users never specify them.
- **Type aliases** for common configurations (e.g., `type StdParseResult =
  ParseResult<UseArc, StdNodeKind, ...>`).
- **Trait bounds kept minimal** — only require what's actually needed at
  each point.
- **Turbofish avoidance** — design APIs so type inference resolves
  parameters naturally from context.

The goal is that a user who doesn't need customization writes the same code
as if no generics existed, while a user who needs custom node types or
`Rc` instead of `Arc` can opt in without forking the library.

---

## Design Decisions Summary

| Concern | Decision | Rationale |
|---------|----------|-----------|
| Source ownership in nodes | `Arc<Source>` in `SourceSpan` | Self-contained nodes; enables post-processing without source copying or lifetime chains |
| Source resolution | `SourceResolver` trait, generic on parser | Pluggable I/O; `NoResolver` ZST for no-filesystem builds |
| Content reading | `SourceCursor<C: SourceContent>` | Forward scan + mark/rewind; generic over String/mmap |
| AST storage | Flat `Vec<NodeData>` with index ranges | Cache-friendly, no per-node allocation, trivially serializable |
| AST access | `NodeRef` proxy (Copy, index + &ParseResult) | Ergonomic, safe, resolves lazily |
| Line/column | Lazy `LineIndex`, separate from Source | Only computed for display; decoupled utility |
| Spec references in nodes | `Arc<MacroSpec>` etc. | Genuine shared ownership; dynamic creation; survives tree transforms |
| ParsingState in nodes | `Arc<ParsingState>` | Shared across nodes with same state; captured at parse time |
| Environment reusability | `FLMEnvironment` owns no per-parse state | No memory accumulation across runs |
| Lifetime parameters | `ParseResult<'env>`, `NodeRef<'pr>` | Minimal; no lifetime on node data itself |
| Provenance tracking | `SourceProvenance` enum with `SourceSpan` back-references | Chains form a tree for error reporting |
| Post-processing | Immutable trees; transforms produce new trees | `Arc` sharing avoids copies; specific APIs TBD |
| Generics | Core types generic over shared pointer, node kind, specs, state | User customizability without forking; defaults keep simple cases simple |
| Shared pointer | Generic trait (`Arc` default, `Rc` opt-in) | Users who don't need thread safety avoid atomic overhead |
| Future huge files | `SourceContent` trait, mmap deferred | Trait boundary in place; no parser changes needed later |

---

## Rejected Alternatives

### `SourceId` as public type
An opaque index into a `SourceStore` that users would store and pass around.
**Rejected** because it circumvents Rust's lifetime checks — a `SourceId` is
meaningless without its store, and Rust can't enforce that the store is
available.

### Index-based source spans with `SourceStore`
Nodes store `(source_index, start, end)` and a `SourceStore` in
`ParseResult` owns all sources. **Rejected** because it ties nodes to a
specific `ParseResult`: tree transformations must either copy the store,
share it via Arc (defeating the purpose), or create lifetime chains across
transform results. The `Arc<Source>` approach makes nodes self-contained
and transforms independent.

### Lifetime `'src` on all AST types
`SourceSpan<'src>` borrows from the source store, and this lifetime
propagates into every node, token, and error type. **Rejected** for two
reasons: the self-referential struct problem (nodes can't borrow from a
sibling store in the same struct), and the same transform-chaining problem
as index-based spans.

### Byte-level `Read` / `BufRead` streaming
Treating source content as a byte stream. **Rejected** because FLM parsing
needs lookahead and backtracking, which are awkward over `Read`. A `&str`
cursor with mark/rewind is more natural. The `SourceContent` trait still
allows memory-mapped files for huge inputs without byte-level streaming.
