# P4 RULING — Transformation & navigation surface (frozen 2026-07-31)

Status: **RULED by the user** (interactive session, 6 rounds + confirmation round).
This file is the *working* detail record for the application sessions (Phase 2b/3); the
**durable** records are the DESIGN_RATIONALE.md entries listed at the bottom (this
directory is deleted when the review completes). Nothing here is applied to code yet.

Scope: everything from POLICY_BRIEF.md §P4 (T5 need #1) plus the design questions that
surfaced while ruling it: annotations, tree tags, ext minting, the restage driver,
recompose, slot roles, `\input` anticipation, navigation.

---

## The ruling (12 points)

### 1. Annotations — `NodeTree<L, A = ()>`

- Second, defaulted generic parameter on the tree: the **annotation** type `A`. One
  value per node, **uniform across kinds** (consumers use enums inside `A` for
  kind-shaped data), chosen by the *consumer* per processing stage. `Lang` never sees
  `A`; the parser emits `A = ()`; `ParseResult` spelling unchanged (default).
- Storage: **parallel `Vec<A>`** indexed by node index over an **`Arc`-shared node
  core** — `NodeTree = { core: Arc<TreeCore<L>>, annotations: Vec<A> }`;
  `TreeCore = { nodes: Vec<NodeData<L>>, tree_tag }`. `NodeData` stays
  single-generic (supersedes the original `NodeData<L, ProcessedNodeData>` sketch).
- `annotate::<B>(f)` produces `NodeTree<L, B>` sharing the core: **zero `NodeData`
  cloned**, only the new annotation vector is allocated; the input tree is untouched;
  same-layout stages share the core **and the tree tag**, so their `NodeId`s are
  interchangeable (ids identify *layout*, not stage). `NodeTree::clone()` becomes
  O(annotations). (This answers "how is annotating zero-copy when trees are frozen":
  the frozen thing is the core, shared behind the `Arc`; a new thin `NodeTree` value
  is created per stage.)
- Bounds: `A: Clone + Debug + Send + Sync` — **no `Default`** (every annotation value
  is supplied explicitly; see point 6's single-pathway rule).
- Extract-built trees (`Split`/`KeyVals`) produce `A = ()`. **Recorded later option**:
  rebase the extract builders on the restage mechanism so they can keep/map
  annotations of annotated input trees (user, confirmation round; decide in 2b).
- FFI: bindings fix one concrete `A` for the whole pipeline (e.g. a PyObject-slot
  type) — dynamic typing inside one monomorphization.
- Name: "annotations" (the working name `ProcessedNodeData` is superseded — it
  collides with `NodeData` in the same scope).

### 2. Tree tags — always-on, part of `NodeId` identity

- `TreeTag` newtype over **`u32`** (user ruled u32 over u64), minted per layout by the
  existing wrapping global counter, in **all builds**. Term: **`tree_tag`** (not
  `tree_identifier` — overpromises addressability).
- `NodeId = { index: u32, tree_tag: TreeTag }` (8 bytes, `Copy`); the tag
  **participates in `Eq`/`Ord`/`Hash`** (debug and release agree again). Ids from
  different trees are distinct values → maps can span trees; old-tree ids stored in
  new-tree annotations are unambiguous.
- `NodeTree::get()` rejects foreign ids in release builds; `node()` keeps the
  panicking own-tree contract. Layout-preserving copies (`clone`, `materialize`,
  `annotate`) share the tag.
- Wrap after 2^32 layouts accepted and documented: the tag is a **misuse detector,
  never an addressing mechanism** (resolution always goes through an explicit tree).
  Tags are process-local — never wire material. Bare `Range<u32>` region values stay
  untagged.

### 3. Ext system — population is initialization

- **Principle**: an ext is minted exactly once, at creation, by the party with the
  knowledge. No "default-initialized, populated later" state anywhere.
- **Tier-2 per-kind node exts are REMOVED** (`CharsNodeExt`…`ListNodeExt` deleted;
  `NodeKind` becomes purely structural — no ext fields in `Chars`/`GroupData`/
  `CallableData`/`Comment`/`List`). Per-kind lang data = an enum inside tier-1
  `NodeExt`, coherence enforced at the single minting point. The considered
  alternative `NodeDataExt { uniform, per_kind: enum }` (parallel bundle) is
  dominated: same representable-mismatch cost as removal, plus the ceremony.
- **`Lang::make_node_ext` replaces `Lang::finalize_node`** (which is deleted, along
  with its idempotence contract):
  ```rust
  fn make_node_ext(kind: &NodeKind<Self>, span: &SourceSpan<Self::SourceOrigin>,
                   state: &Arc<ParsingState<Self>>, children: StagedChildren<'_, Self>)
                   -> NodeExt<Self>;   // REQUIRED method (SimpleLang blankets `()`)
  ```
  Value-return; `kind` by shared ref (the hook cannot change the kind — the &mut and
  the consume-and-return variants were both rejected). No parent access (impossible:
  staging is bottom-up; downward context is `StateExt`'s job — the user's point). No
  `StagedNodes` whole-forest view: **`StagedChildren` is subtree-deep, descent-only**
  — child views resolve *their* children recursively (needed to read argument content
  at grandchild depth, e.g. computing `{domain,key}` from `\ref{fig:abc}`), but expose
  no siblings/ancestors/unrelated staged nodes.
- **`NodeExt` loses its `Default` bound** (`Clone + Debug + Send + Sync`), which is
  what *forces* `make_node_ext` to be a required method — a feature, not a wart: a
  lang with a real ext type must say how it's initialized.
- **`NodeTreeBuilder` is hook-free and mode-free** with exactly one staging method:
  `add(kind, span, parsing_state, children, ext, annotation)`. It demands ready
  values. (The earlier `add()`+`add_with_ext()` pair and the `for_parsing()`
  hook-firing-mode constructor are both rejected shapes.)
- **Parse staging goes exclusively through `ParseContext::stage_node(kind, span,
  state, children)`** — the ONE automatic `make_node_ext` site (mints, then
  `builder.add(…, ext, ())`). **`ParserSession::builder` becomes `pub(crate)`**
  (persona sweep found no legitimate external mutable need; a public read view
  `cx.staged_nodes()` stays for node-based stop predicates). ~25 internal call sites
  churn from `cx.session.builder.add(...)` to `cx.stage_node(...)`.
- **Transform-side minting is the explicit two-line recipe** (call
  `L::make_node_ext`, then `builder.add(...)`) — deliberately NO wrapper helper: the
  explicit spelling is the finer control (inspect/adjust the minted ext between the
  two lines) and cannot be reached by someone who doesn't know what it does.
- WHO/WHEN in one sentence: *`make_node_ext` runs inside `cx.stage_node()` during
  parsing, and wherever a transform author writes the call explicitly; nowhere else,
  ever.* Restaged copies carry cloned exts verbatim as frozen parse facts — never
  re-minted.
- Knock-on: `split_at_chars` boundary partials are now properly minted via
  `make_node_ext` (the "partials carry default ext" approximation disappears).
- **`ArgumentExt` is KEPT** (user, final round — the body-marking story proved slot
  exts load-bearing late; arguments deserve the same open door, e.g. a future
  `BodySlotExt`-analog trait bound on `ArgumentExt`). Minting: the **`ArgumentParser`
  output carries the ext** (`ParsedArgumentNodes` gains the field; record constructor
  demands it); custom parsers mint their own; the **standard parsers are defined only
  `where ArgumentExt<L>: Default`** (conditional bound-where-used, the
  `ClosedVocabulary` pattern) — a std parser's knowledge about your ext *is*
  "nothing", and its bound says so. The bundle itself carries no `Default` bound.
- **`SlotExt`**: demanded at `ParsedSlot` construction; generic preset machinery mints
  via `BodySlotExt::make_body()`; custom `EnvironmentBehavior`s pass their values. No
  Lang hook, no `Default` anywhere.
- Ext bundle final shape: `NodeExtTypes = { NodeExt, ArgumentExt, SlotExt }`.

### 4. Staging paths — summary table

| Path | Who mints `NodeExt` | Annotation |
|---|---|---|
| parse: `cx.stage_node(...)` | automatic `make_node_ext` | `()` |
| restage copy (driver / `restage_*` ops) | cloned from the old node | from the visitor (point 6) |
| transform new node | explicit `make_node_ext` recipe (or bespoke value) | explicit argument |
| extract partial (in-crate) | explicit `make_node_ext` call | `()` (extract trees are `A = ()`) |

### 5. Id correspondence — origin by convention (no `finish()` map)

- No BuildId→NodeId map from `finish()`; no auto-provenance scaffolding. The
  `WithTransformedTreeNodeProvenance`-style trait + wrapper was **rejected** (user):
  merges and subtree replacements make "the" original id a fiction no mechanism can
  choose semantics for; convention beats mechanism.
- The framework puts an old `NodeId` field in its **own annotation type** and tags it
  along (`Ann { origin: node.id(), … }`); it holds the old `NodeTree` anyway.
  techy contributes: always-tagged ids (safe to hold cross-tree), the old `NodeRef`
  in the visitor's hand at annotation time, and a documented recipe (incl. the O(n)
  old→new inversion walk over the new tree's annotations).
- Per-node `Arc<NodeTree>` references were rejected (type-chaining across stages;
  lifetime-pinning of every pipeline stage). Vocabulary: **"original node"** —
  "provenance" and "origin" alone are source-model vocabulary
  (`SourceProvenance`/`SourceOrigin`).

### 6. Transform surface — `techy::transform`, streaming restage

- In-crate **module** `techy::transform` (companion crate rejected; `techy-totext`
  is the external-consumer proof instead). Vocabulary: **restage** ("copy" is banned —
  it misreads as bulk-subtree-copy; `add_subtree` as a public op is superseded).
- **Level 0 primitive** (in `core::node`): single-node copy with a per-child mapping
  ("old child → new BuildIds that replaced it"), translating the callable's
  argument/slot region records — generalized copy.rs. Bulk subtree copy is the
  degenerate recursion, not the primitive.
- **Level 1 driver**: visitor invoked **top-down on the frozen input tree**, staging
  **bottom-up** into a `NodeTreeBuilder<L, B>`; per node the callback returns
  ```rust
  enum Restage<B> { Continue(B),        // driver restages node over children's results;
                                        // visitor CONTINUES through every child subtree
                    Emit(Vec<BuildId>) } // callback staged the replacement itself
                                        // (empty = drop); no automatic descent
  ```
  **Safety invariant: `Continue` always descends** — the only way a child subtree
  goes unvisited is an explicit `Emit` for its ancestor. There is no shallow-keep to
  reach by accident.
- **Annotations, single pathway** (user's redesign): no run-level mapper; *every*
  restaged node's annotation passes through the visitor — as `Continue(b)`, or as an
  explicit argument to the staging ops the callback invokes. Mandatory by
  construction (`A_old` ≠ `A_new`; "keep annotation" is not expressible — good by
  design).
- **Region-aware context ops** (crate owns ALL region arithmetic):
  `restage_subtree(node)` (full visitor over the subtree, its root included);
  `restage_children(node)`; `restage_argument(node, index_or_name)` /
  `restage_slot(node, i)` → **restaged-region bundles** (new BuildIds + record data —
  spec, name, provided/absent, content designation — in bundle-relative staging
  coordinates); `restage_invocation(node, arguments, slots, annotation)` (restages the
  invocation data over bundles **in the order given**, retiling records — argument
  swap `\a{1}{2}` → `\a{2}{1}` is two `restage_argument` calls + one reordered
  `restage_invocation`); raw `builder()` access under everything (canned ops are
  conveniences, not the power boundary — programmatic staging is always available;
  merges = parent-level callback takeover).
- **Read frozen / write staged** (final-round ruling): callbacks *inspect the frozen
  input* — full read API + `techy::extract` tools (`content_as_chars` on
  `argument_content_nodes`, spans, `parent()`) — and *produce* staged output; the
  staged side is **write-only** (BuildIds + bundles, deliberately opaque). Verified
  there is no meaningful staged-side read need: decisions precede restaging
  (top-down); whatever a callback stages it just made (facts carry in closure state
  or annotations); full read semantics are impossible pre-`finish` anyway
  (unresolved regions, no layout). Frameworks needing to inspect transform output
  finish the tree and run another pass — multi-stage is deliberately cheap (that's
  what annotations/zero-copy `annotate`/shared tags are for). Accepted boundary: a
  `Continue` parent never sees child results (take over via `Emit` +
  `restage_children`, or use two passes).
- Rejected: fixed atomic ops (add/drop/splice/rebuild) as the ceiling — not powerful
  enough (user); the driver's fixed job is only order mediation + region-preserving
  reassembly. Swapped-argument trees put sibling spans out of source order: legal in
  transform trees (exempt from the parse-law), honest `None`s from slice spans.
- Read-side honesty riders: **transform-tier validator** (structure + region tiling +
  `TextContent` residency, minus parse-law byte accounting); `NodeRef::tree()` made
  public.

### 7. Recompose — `techy::recompose` (direction ruled; own design session)

- Generic tree fold assembling output text; consumer supplies per-node logic; a typed
  **recomposition state threads downward** into children. Two shipped strategies
  prove it: **span-verbatim** (exact bytes, gap-filling — the latexpp path, works on
  tolerant-recovery output) and **node-data spelling** (pylatexenc
  `latexnodes/_latex_recomposer.py` precedent — `LatexNodesLatexRecomposer`; core
  provides the walk, the **latexlike preset provides trigger spellings**; on a
  `materialize()`d tree this touches no `Source` at all — fully source-independent
  byte-faithful reconstruction, per the T5-C finding that node data reconstructs
  everything except trigger spellings).
- latex2text = "a recomposition whose per-node logic emits text, not LaTeX":
  **mechanism in techy, content in techy-totext** (consistent with the PLAN's
  rejection of elaborate in-techy plain-text extraction).
- Strategies key on `SlotRole` (verbatim skips `Attached` by definition — the
  invocation text IS the recomposition; descending is the explicit "expansion"
  option; `Hidden` never participates).
- **Own planning session** for the design (open: direct fold vs
  transform-to-chars-then-concatenate; state threading; output sink type; targeted
  replacements). Top-level module `techy::recompose` (with `transform`, amends the
  P1 topology's top-level roster).

### 8. Slot roles + body marking

- `pub enum SlotRole { Content, Attached, Hidden }`, field on `ParsedSlot`, default
  `Content`. `Content` = constitutive (unrecoverable if discarded); `Attached` =
  derived/redundant, reconstructible from the invocation (`\input` content);
  `Hidden` = framework/callable-defined, ignored by techy core.
- **`Attached` slots are excluded from the parent's byte-tiling** (declaration
  replaces source-change inference in the validator); structural child-list tiling
  stays role-independent.
- Body marking is a **different axis**, via the slot ext:
  `trait BodySlotExt { fn is_body(&self) -> bool; fn make_body() -> Self; }` —
  `NodeRef::body()` picks the slot whose ext reports `is_body()` under a
  bound-where-used; "slot 0" stops being load-bearing. Forking frameworks implement
  the trait on their own `SlotExt` and all preset machinery keeps working.
- **P3 amendment (per-member restatement)**: the preset's `NodeExts` stays `()` for
  the node/argument members; **`SlotExt` is claimed by the preset** for the body
  marker. Rejected alternative: body-by-slot-name ("body" string — stringly-typed).
- 2b details: `body()` × `SlotRole` filter; extract readers vs `Hidden`;
  `#[non_exhaustive]`?; variant naming (`Attached` vs `Derived`).

### 9. `\input` anticipation — multi-source parse trees are first-class

- Intended shape: the callable's spec parser resolves the reference → **sub-parses
  the resolved source into the SAME builder**, staged as an **`Attached` slot** of
  the `\input` callable. Decisive: copy-free AND semantically forced — included
  content must parse under the state *at the `\input` point*, which the running
  session has. (Separate-parse-then-restage-splice stays possible via the primitives
  for caching frameworks; state-correctness caveat on their head.)
- Sibling-run source-coherence holds naturally (the callable's own span is its
  invocation in the includer's source; only slot children live in the included
  source, and they are siblings *of each other*) — the middle-node staleness hazard
  stays exclusive to transform-spliced trees; the single-source `finish()` flag is a
  fast-path bit, NOT a semantic tier (multi-source ≠ degraded).
- Validator: byte-accounting scoped per source via the `Attached` role (point 8).
  `node_at`: per-source descent already gives the right answers (query in the
  includer stops at the `\input` node; query in the included source finds content).
  Recompose: verbatim is per-source (emits `\input{file}`, not the content).
- **Resolver moves from `Language` to the `ParseDriver`** (direction recorded;
  placement doctrine — parse-time instance behavior; amends [§dd-dr:language-init]'s
  expected surface: `Language` collapses toward the constructor alone). Engine
  wiring (sub-parse spawning, the resolver move) designed in the **T4 session**
  (friction F8).

### 10. Navigation

- **Parent table**: the `Vec<u32>` `finish()` already computes is kept on the tree
  (4 bytes/node; reverses [§dd-dr:iter-storage-order]'s decline — consumers exist
  now: FFI gap #4, T4's F7, pass-style renderers). `NodeRef::parent()`,
  `index_in_parent()` (O(1): own index − parent's block start).
- **`SourcePos<O> { source: Arc<Source<O>>, pos: usize }`** — new source-model type,
  analogous to `SourceSpan` (constructor, accessors, Debug, line/col via
  `LineIndex`; `SourceSpan::start_pos()/end_pos()` conveniences). Points to a single
  location; avoids reading `(source, pos)` as unrelated arguments.
- **`node_at(&SourcePos)`**: the **deepest** node whose span contains the offset —
  half-open containment (`start ≤ pos < end`), empty spans never match; descend only
  into children whose span is in the **query's source**; only exact per-node spans
  are trusted (robust on spliced trees — degrades to the shallowest honest answer).
  Offsets inside a node but in no child (group delimiters, trigger spellings)
  resolve to that node. Span query: the **minimal covering sibling run**
  (`NodeSlice`) within the deepest containing node list. Binary search over
  span-sorted siblings opportunistically, linear fallback; NO offset→node index
  table (premature).
- **Honest slices**: `NodeSlice::span()/source_text()` verify per-run source
  uniformity; the `finish()` single-source flag is the O(1) fast path.

### 11. Bounds summary

`NodeExt`: `Clone + Debug + Send + Sync` (no `Default`). `A` (annotations): `Clone +
Debug + Send + Sync` (no `Default`). `ArgumentExt`/`SlotExt`: `Clone + Debug + Send +
Sync`; `Default` on `ArgumentExt` only as the std-parser eligibility opt-in;
`SlotExt` needs no `Default` at all. Tier-2 types: deleted. `Lang` doc claim "all
methods have working defaults" gains the one exception (`make_node_ext`).

### 12. Naming decisions & bans from this session

Adopted: annotations / `annotate()` / `annotation()` (accessor names 2b);
`make_node_ext`; `StagedChildren`; `tree_tag` / `TreeTag`; restage vocabulary
(`Restage::Continue` kept, competition open — alternates listed: `Descend`, `Keep`,
`Retain`, `Auto`); `BodySlotExt` / `make_body`; `SlotRole::{Content, Attached,
Hidden}`; `SourcePos`; "original node" for cross-tree tracking.
Superseded (recorded in [§dd-dr:superseded-names]): `finalize_node` (and the interim
`populate_ext`/`populate_node_ext`); the tier-2 `<Kind>NodeExt` family +
`NodeDataExt`; `ProcessedNodeData`; `WithTransformedTreeNodeProvenance` /
`WithOriginalNode`; `add_subtree`/"copy" transform vocabulary;
`NodeTreeBuilder::for_parsing()`; `tree_identifier`; node-level
"provenance"/"origin".

---

## Deferred agenda (routing)

- **2b T5 session**: restage callback/return/bundle exact types & error strategy;
  region-edit policies (drop empties a region / removes a content parent — which
  repairs the driver offers); `stage_argument_like`-style content-replacement helper;
  builder-`add` ergonomics (params struct?) ; annotation accessor naming; `annotate()`
  traversal order; `Restage` variant final name; Split/KeyVals-on-restage option
  (with T1/T2); extract readers vs `Hidden` slots.
- **Recompose session** (own item): fold architecture, state threading, output sink,
  replacements.
- **T4 session**: `\input` engine wiring (sub-parse into the same builder), resolver
  move `Language` → driver, `SourcePos`/lookup method naming, LineIndex helpers F6.
- **Application (Phase 3)**: all of the above lands together with the P1 topology
  move (which already waits on the 2b resolution-family design).

## Durable records (DESIGN_RATIONALE.md)

Topic section [§dd-dr:transform] with entries: [§dd-dr:node-annotations],
[§dd-dr:tree-tags], [§dd-dr:ext-minting], [§dd-dr:restage], [§dd-dr:recompose],
[§dd-dr:slot-roles], [§dd-dr:input-attachment], [§dd-dr:tree-navigation]; amendment
notes on [§dd-dr:finalize-node], [§dd-dr:node-id-provenance],
[§dd-dr:closed-node-kind], [§dd-dr:parsed-arguments], [§dd-dr:slot-ext],
[§dd-dr:iter-storage-order], [§dd-dr:staging-builder], [§dd-dr:read-api],
[§dd-dr:source-resolver], [§dd-dr:language-init], [§dd-dr:public-namespace-topology],
[§dd-dr:latexlike-generalization]; superseded-names additions.
