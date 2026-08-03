# Phase 3 — S3 report: node core (identity, annotations, ext minting, slot roles, navigation, validation)

Branch `phase3-s3-node-core`, based on `api-review` @ c45f126.

## Progress (live — updated at every milestone)

- [x] 0. Implementation plan written + committed (this document)
- [x] A. Tree tags + annotations core (`TreeTag`, `NodeId` identity, `TreeCore`/`Arc`, `annotate`, accessors; parent table + single-source flag stored)
- [x] B. Ext minting (`make_node_ext` + `StagedChildren`, tier-2 deletion, hook-free 6-param `add`, `cx.stage_node`, `ParserSession::builder` → pub(crate), extract minting)
- [x] C1. Slot roles + ext demands (`SlotRole`, `BodySlotExt`, `body()`, preset `SlotExt` claim, record arities, std parsers `where ArgumentExt<L>: Default`) — done (successor agent 1; see Deviation D-C1: absent arguments carry no ext)
- [x] C2. Constructor reshapes (`ArgumentSpec::new/new_unnamed` + `IntoArgumentParser`, `StdCallableSpec::new(IntoIterator)`, `ParsedArguments::new`/`ParsedSlots::new`) — done (successor agent 1)
- [x] D. Level-0 `restage_node` (+ copy.rs rebased on it) — done (successor agent 2)
- [x] E. Navigation (`parent`/`index_in_parent`, `SourcePos`, `start_pos`/`end_pos`, `Span::contains`, `node_at`, `covering_slice`, `tree()` pub) — done (successor agent 2)
- [x] F. Slices single-source whole-run contract (fast-path flag wired) — done (successor agent 2)
- [x] G. Validation (`validate_tree` + `TreeViolation`, `check_tree_invariants` → pub(crate) wrapper, S6 TODO for the Attached byte-tiling scoping — SUPERVISOR-RESOLVED: the exclusion itself is parse-law-side and lands at S6; `validate_tree` does no byte accounting) — done (successor agent 3)
- [ ] H. Consumer polish (`display_tree`, `NodeKind::as_str`)
- [ ] I. Docs (rustdoc sweeps, DR status lines, superseded-names verification, guide pages/CLAUDE.md)
- [ ] Gates (build 0 warnings / test / docs clean / grep gates / README rlib)

## Implementation plan (digest of all ruling inputs — written before implementation)

Inputs read in full: PHASE3_PLAN.md §Protocol+§S3; P4_RULING.md pts 1–5, 6(level-0),
8, 10, 11, 12; T5_RULINGS §A1/A5/A6/A9/B/F1–F4/H(b)(c)/G; T3_RULINGS §C+G (+§H
context); T1T2_RULINGS §D/§E5; T4_RULINGS §E+D/§F rows 26–29; TIERC_RULINGS Round 3
Theme C; DR entries: tree-tags, node-annotations, ext-minting, slot-roles,
tree-navigation, tree-validation, display-tree, registration-ergonomics,
named-first-constructors, restage-ops (level-0 half), extract-annotations (boundary
scoping), input-attachment (navigation/validator consequences), invocation-syntax
(S5 exclusions), superseded-names register.
Code surveyed: all of `techy/src/node/*`; `state/lang.rs`; `spec/*`; `engine/mod.rs`;
`engine/language.rs` (root staging); `constructs/mod.rs` (ParseContext) +
`invocation_parser.rs` in full; `latexlike/{mod,environments,spec}.rs` (Lang impl,
slot minting, CallableData literals); `extract.rs` (piece/segment staging machinery);
`source/{span,source,mod}.rs`; facades `core/{node,mod}.rs`; `tests/acceptance.rs`
usage of `check_tree_invariants` (3 sites); churn greps: 26 `session.builder` sites,
45 `ArgumentSpec::new` sites, 10 `.named(` sites, 7 `ParsedSlot::named` sites.

### Milestone A — tree tags + annotations core (tree.rs rewrite; ripple through node_ref/slice)

Ruled shape:
- `TreeTag` public newtype over `u32` (Copy/Eq/Ord/Hash/Debug; no public constructor),
  minted per layout by the existing wrapping global counter made **always-on**
  (`core::sync::atomic::AtomicU32` — no_std-clean); wrap-around documented (misuse
  detector, never an addressing mechanism; tags process-local, never wire material).
  Bare `Range<u32>` regions stay untagged.
- `NodeId { index: u32, tree_tag: TreeTag }` (8 bytes, Copy), tag **participates in
  derived Eq/Ord/Hash** (derive on the ruled field order `{ index, tree_tag }`;
  ordering axis not ruled — derived order documented as total, unspecified detail).
  Existing `index()` accessor kept; add public `tree_tag()` (smallest surface making
  the ruled newtype meaningful — report as delegated detail).
- `NodeTree<L, A = ()> = { core: Arc<TreeCore<L>>, annotations: Vec<A> }`;
  `TreeCore` pub(crate) `{ nodes: Vec<NodeData<L>>, parent: Vec<u32>, tree_tag:
  TreeTag, single_source: bool }` — parent table + single-source flag stored NOW so
  `finish()` is touched once; public accessors land in E/F.
- `node()` keeps the panicking own-tree contract — tag assert now in ALL builds;
  `get()` rejects foreign ids in release builds too.
- `annotate::<B>(f)`: zero-copy (`Arc::clone` the core, SAME tag), callback
  `FnMut(NodeRef<'_, L, A>) -> B` run in **storage order** with the loud doc
  sentence; input untouched (`&self`); NO `Send`/`Sync` bounds on the callback.
- Accessors `NodeRef::annotation() -> &'t A`, `NodeTree::annotations() -> &[A]`
  (storage-order slice), no setter.
- `A` bounds `Clone + Debug + Send + Sync`, NO `Default`: realized
  **bound-where-used** (the crate's own doctrine; struct-level bounds would ripple
  noise through every view type): `Clone for NodeTree where A: Clone` (documented
  O(annotations)), `Debug where A: Debug`, Send/Sync auto; ruled contract documented
  on the `A` parameter. Report as realization note.
- `materialize()` keeps the tag (layout-preserving); `clone()` shares the core.
- `ChildRegion`'s resolved tree-tag stamp becomes always-on `TreeTag`;
  `content_parent()` mints tagged `NodeId`s.
- Ripple `A = ()` defaults: `NodeRef<'t, L, A = ()>`, `NodeSlice`, `NodeSliceIter`,
  `Descendants` — every existing spelling keeps compiling. `ParseResult`/
  `ParserSession`/extract spellings unchanged (defaults). Extract stays `A = ()` in
  AND out this stage (input genericity rides S7 with the callback triples).
- `NodeTree` gains pub(crate) `nodes()` accessor to contain the `tree.nodes` →
  `tree.core.nodes` churn (invariants.rs, node_ref.rs, builder.rs).
- Builder gains the `A = ()` parameter NOW (parallel `annotations: Vec<A>` — the
  staged arena `Staged<L>` itself stays A-free so B's `StagedChildren` can be
  A-free per the `make_node_ext` signature); `add`/`add_with_ext` temporarily kept
  on the `A = ()` impl so A compiles standalone; B replaces them.
- `NodeId` Debug gains the tag (`NodeId(1@5)`); update the one asserting test.

Files: node/tree.rs (rewrite), node/node_ref.rs, node/slice.rs, node/arguments.rs
(ChildRegion), node/builder.rs, node/mod.rs exports (+`TreeTag`), core/node.rs
facade, node/invariants.rs (accessor churn only).
Tests: tag participates in Eq/Hash (same-index ids of two trees differ; one map keyed
across trees); `get()` foreign-id rejection with an in-range id (release-grade
semantics); clone/materialize/annotate share the tag (id interchangeability);
zero-copy annotate via `Arc::ptr_eq` on the cores (in-crate test reaches `core`);
storage-order annotate callback order.

### Milestone B — ext minting (the big cut)

Ruled shape:
- DELETE `Lang::finalize_node` + its idempotence contract; DELETE the tier-2 per-kind
  ext system: `NodeExtTypes` shrinks to `{ NodeExt, ArgumentExt, SlotExt }`;
  `CharsNodeExt`…`ListNodeExt` aliases deleted; `NodeKind` purely structural:
  `Chars { content }`, `Comment { content, start, post_space }`, unit `List`;
  `GroupData`/`CallableData` lose `ext` fields. `NodeExt` bound
  `Clone + Debug + Send + Sync` (NO `Default`). Sequencing only: `ArgumentExt`/
  `SlotExt` keep `Default` until C1 so B compiles standalone.
- REQUIRED `Lang::make_node_ext(kind: &NodeKind<Self>, span:
  &SourceSpan<Self::SourceOrigin>, state: &Arc<ParsingState<Self>>, children:
  StagedChildren<'_, Self>) -> NodeExt<Self>`; `TrivialLang` blanket supplies `()`;
  every in-crate `impl Lang` (Latexlike + ~12 test langs) writes the trivial body.
  `Lang`'s "all methods have working defaults" doc claim gains the one exception.
- `StagedChildren<'b, L>`: subtree-deep, **descent-only** view (private: arena slice
  + child-id slice). API: `len`/`is_empty`/`get(i) -> Option<StagedChildView>`/
  `iter()`; `StagedChildView` exposes `kind`/`ext`/`span`/`parsing_state` +
  `children() -> StagedChildren` (recursive descent — the grandchild-depth read);
  NO siblings/ancestors/arbitrary-id access, NO BuildId exposure. Constructed via
  public `NodeTreeBuilder::staged_children(&self, &[BuildId])` (the transform-side
  recipe needs it). Unstaged ids: `get` → None / `iter` skips (the subsequent `add`
  diagnoses `ChildNotStaged`; never panic).
- `NodeTreeBuilder<L, A>` hook-free/mode-free with exactly ONE staging method:
  `add(kind, span, parsing_state, children, ext, annotation)` (positional, ruled
  order identity→provenance→context→structure→lang→consumer); `add_with_ext`
  deleted.
- `ParserSession::builder` field → pub(crate); `ParseContext::stage_node(kind, span,
  state, children) -> Result<BuildId, NodeBuildError>` = THE one automatic minting
  site (mints via `make_node_ext`, annotation `()`); public read view
  `ParseContext::staged_nodes()` added; ~26 `cx.session.builder.add(...)` sites in
  constructs/engine churn to `cx.stage_node(...)`; parser-side
  `session.builder.staged_nodes()` reads → `cx.staged_nodes()`.
- Transform-side minting = the explicit two-line recipe, NO wrapper helper — also no
  private in-crate wrapper: extract writes the recipe inline at its 4 mint sites.
- extract.rs: boundary partials + synthesized `List` wrappers mint properly via
  `make_node_ext` (the default-ext approximation dies); copied nodes keep cloned
  exts (copy.rs path).
- copy.rs `add_with_ext` call → 6-param `add` with cloned ext + `()` annotation
  (full restage_node rebase happens in D); its "finalize_node runs again" doc dies.
- WHO/WHEN sentence on stage_node/builder: *`make_node_ext` runs inside
  `cx.stage_node()` during parsing, and wherever a transform author writes the call
  explicitly; nowhere else, ever.* Restaged copies carry cloned exts verbatim.

Files: state/lang.rs; node/{mod,kind,builder,copy}.rs; engine/{mod,language}.rs;
constructs/{mod,nodes_parser,invocation_parser,environment_parser,group_parser,
argument_parsers,chars_group_parser,embellishments_parser,tack_on_parser,
verbatim_parser}.rs; latexlike/{mod,driver,environments,spec}.rs; extract.rs;
spec/callable.rs (downcast doc names finalize_node); core/{mod,node}.rs exports.
Tests: rewrite the `FinalizeLang` suite as `make_node_ext` minting (descendant-count
ext via `StagedChildren`, incl. a **grandchild-depth read**); rewrite nodes_parser's
`finalize_node_populates_callable_ext_through_the_dispatch_loop` to prove automatic
minting through `cx.stage_node`; tier-2 test content deleted; tier-1 store/read
reworked over `make_node_ext`.

### Milestone C1 — slot roles, body marking, ext demands on records

Ruled shape:
- `pub enum SlotRole { Content, Attached, Hidden }` — EXHAUSTIVE (deliberately NOT
  non_exhaustive, T5-A9(iii)), Copy/Clone/Debug/PartialEq/Eq/Hash + `Default` =
  `Content` (the "conceptual default"). Field `role` on `ParsedSlot`. Docs:
  Content = constitutive; Attached = derived/reconstructible from the invocation
  (excluded from the parent's byte-tiling — declaration replaces source-change
  inference); Hidden = framework-defined, "no recomposition, no byte accounting" —
  NOT read-invisibility (readers/extract role-blind; doc note per T5-A9(ii)).
- `trait BodySlotExt { fn is_body(&self) -> bool; fn make_body() -> Self; }` in
  node/arguments.rs beside `ParsedSlot` (home = slot-record domain; report).
- `NodeRef::body()` re-specified: the FIRST slot whose `ext.is_body()` under
  `where SlotExt<L>: BodySlotExt` — ext axis ONLY, no role conjunction (doc
  sentence, T5-A9(i)); "slot 0" stops being load-bearing.
- `NodeExtTypes::{ArgumentExt, SlotExt}` lose `Default` (the bundle carries none).
- `ParsedArgumentNodes` → generic `ParsedArgumentNodes<L>` + `pub ext:
  ArgumentExt<L>`; constructor `new(nodes, content, ext)` (payload-first; delegated
  arity). `ArgumentParser::parse_argument` return type updated.
- `ParsedArgument::provided(spec, region, ext)` / `absent(spec, ext)` (ext appended
  last = consumer-data-last; `absent` demanding the ext is forced by the field
  losing `Default` — delegated arity, report).
- Std argument parsers + `parse_declared_arguments` + `StdInvocationParser` gain
  `where ArgumentExt<L>: Default` (bound-where-used); they fill `Default::default()`.
- `ParsedSlot` gains `role: SlotRole` + the non-defaultable ext; constructors (C2
  shape): `new(region, name, role, ext)` / `new_unnamed(region, role, ext)`
  (payload-first, named/unnamed split — delegated arity; `ParsedSlot::named`
  REMOVED).
- Latexlike claims `SlotExt`: bundle `LatexlikeNodeExts` (NodeExt = (), ArgumentExt
  = (), SlotExt = `BodyMarker`); `BodyMarker` = the preset body-marker type (private
  `body: bool`; `not_body()` constructor; `BodySlotExt` impl supplies `make_body()`;
  no `Default`); environments.rs mints the body slot via `BodySlotExt::make_body()`
  + `SlotRole::Content` (written through the trait so S4's LLL genericization is
  mechanical).
Files: node/arguments.rs, node/node_ref.rs, spec/structure.rs, constructs/* (std
parsers), latexlike/{mod,environments}.rs, state/lang.rs (bundle), facades, tests.
Tests: `body()` via ext marker incl. a non-slot-0 body; role stored/read; records
constructible for a non-Default-ext lang.

### Milestone C2 — constructor reshapes riding the ext arities

- Sealed `IntoArgumentParser<L, M>` in spec/structure.rs (family precedent
  `IntoCallableSpec`; the inference-marker realization is the S2-D1 user-approved
  precedent; impls: `P: ArgumentParser<L>` by value, `Arc<P>`,
  `Arc<dyn ArgumentParser<L>>`).
- `ArgumentSpec::new(parser, name: impl Into<Box<str>>)` + `new_unnamed(parser)`;
  `.named()` builder REMOVED; `.with_state_delta()` stays.
- `StdCallableSpec::new(arguments: impl IntoIterator<Item = ArgumentSpec<L>>)`
  (specs by value, Arc'd inside; sites needing shared Arcs use the pub field).
- `ParsedArguments::new(Vec)` + `ParsedSlots::new(Vec)`; the `From<Vec>` impls stay.
- Churn: ~45 `ArgumentSpec::new` sites (Arc'd parsers pass through the conversion),
  10 `.named(` sites; preset spec constructors (`MacroSpec::new` etc.) keep their
  current shapes — preset reshapes are S4/S9 scope.
Files: spec/{structure,callable,mod}.rs, latexlike/arguments.rs, test call sites.

### Milestone D — level-0 restage primitive

- `NodeTreeBuilder::restage_node<AOld>(node: NodeRef<'_, L, AOld>, replacements:
  &[Vec<BuildId>], content_parents: impl Fn(NodeId) -> Option<BuildId>, annotation:
  A) -> Result<BuildId, NodeBuildError>` in core::node. Single-node copy: children =
  `replacements` flattened in order (length must equal `child_count()` → new
  non_exhaustive `NodeBuildError` variant `ReplacementsLengthMismatch`); callable
  argument/slot records translated to staging coordinates via prefix sums over
  replacement lengths (region extents recomputed under dropped/replaced/multiplied
  children); `InRegion` content ranges re-based the same way; `InChildrenOf`
  parents mapped through `content_parents` (None → new variant
  `ContentParentUnmapped { parent: NodeId }`), child-offset ranges carried verbatim
  relative to the mapped parent (add()/finish() re-validate). Ext CLONED verbatim
  (never re-minted); span/state cloned; annotation from the caller.
- **Cross-tree sanctioned**: accepts a NodeRef from ANY tree; NO same-tree
  debug-assert, EVER (documented as the sanctioned splice door). No `Send` bounds
  on the callback parameter.
- copy.rs rebased: `copy_subtree_into` = the degenerate recursion over
  `restage_node` (singleton replacements; the ids map as the content_parents fn).
- NO visitor/driver/ops/bundles/`RestageError` (S7).
Tests: child dropped from a region (region shrinks; empty-region provided survives);
child multiplied; cross-tree restage; unmapped content parent → Err; length
mismatch → Err; copy-subtree behavior unchanged (existing tests).

### Milestone E — navigation

- Parent table (stored in A) exposed: `NodeRef::parent() -> Option<NodeRef>`,
  `index_in_parent() -> Option<usize>` (O(1): own index − parent's block start).
  NO `ancestors()` — the one-line
  `core::iter::successors(node.parent(), |n| n.parent())` recipe in `parent()`'s
  rustdoc. `NodeRef::tree()` → pub.
- `SourcePos<O = Option<String>> { source: Arc<Source<O>>, pos: usize }` in
  techy::source beside `SourceSpan`: `new(&Arc<Source<O>>, pos)`, accessors
  `source()`/`pos()`, Clone/PartialEq/Eq/Debug; line/col via the existing
  `source().line_index()` route (documented; no new method — SourceSpan parity).
- `SourceSpan::start_pos()`/`end_pos()` (end_pos doc sentence: exclusive — one past
  the last byte). `Span::contains(pos)`: half-open `start <= pos < end`; empty spans
  never match (documented explicitly).
- `NodeTree::node_at(&SourcePos<L::SourceOrigin>) -> Option<NodeRef>`: deepest node
  whose span contains the offset. Algorithm (realizes the ruled semantics on
  multi-source trees): recursive from the root — a node whose span is in the
  query's source either contains the offset (match → refine ONLY into same-source
  children; different-source children never entered from a matching node → an
  includer query stops at the `\input` node) or prunes its subtree (exact spans
  trusted); a node in a DIFFERENT source is never a match but its children are
  searched (the route to attached-source content). Offsets in a node but in no
  child resolve to that node.
- `NodeTree::covering_slice(&SourceSpan<L::SourceOrigin>) -> Option<NodeSlice>`:
  minimal covering sibling run within the deepest containing node list. Descend
  while a single same-source child contains the whole query; at the deepest such
  node, the minimal child run covering the query is the answer; if the children
  cannot cover it (query bytes in delimiters/trigger), the covering node itself as
  a single-node run (within its parent's list); empty query spans resolve by
  half-open point containment. Binary search over span-sorted siblings
  opportunistically (partition_point candidates, verified locally: edge overlap +
  coverage + minimality; linear fallback on any local-check failure); NO
  offset→index table.
Files: source/{source,span,mod}.rs (or a new pos.rs), node/{tree,node_ref}.rs,
facades. Tests: parent/index_in_parent; the successors-recipe walk; node_at
deepest / gap-offset (group-delimiter offset → the group node) / empty-span never
matches / per-source; covering_slice single-node, multi-sibling run,
delimiter-overlap → single-node parent run, empty query, no-cover → None.

### Milestone F — slices

- `NodeSlice::span()`/`source_text()` answer ONLY for whole-run single-source
  slices: full-run uniformity verification (every node's span source ptr-eq the
  first's), short-circuited by the `finish()`-computed single-source fast-path flag
  on `TreeCore`; `source_text()` gains the ordering guard for contract parity;
  `None` = no single-source answer. Rustdoc states the concrete condition ("the run
  lies within a single source") — the word "honest" MUST NOT appear.
Tests: middle-node-foreign-source run → None from both (built via restage splice);
fast path on a parsed tree.

### Milestone G — validation

- `pub fn validate_tree<L, A>(&NodeTree<L, A>) -> Result<(), TreeViolation>` in
  node/invariants.rs, exported at core::node. The all-trees law ONLY: structural
  sanity (children in-bounds, after-parent, single-parent, root unparented, all
  reachable, Chars/Comment childless), regions resolved + region tiling of the
  child list (role-independent) + content ranges inside content parents + content
  parent inside its own region's subtree, `TextContent` residency (valid
  char-boundary range of the node's own source — residency only, NO positional
  pins). Explicitly MINUS parse-law byte accounting (no byte partition, no
  children-share-parent's-source, no sibling source order). Returns `Err`, never
  panics (panic policy).
- `TreeViolation { node: Option<NodeId>, kind: TreeViolationKind }` — both
  non_exhaustive, Clone + Debug + Display (full detail) + Error.
- `check_tree_invariants` → **pub(crate)**: panic-assert wrapper =
  validate_tree's Err panicked with full violation detail + the parse-law
  byte-accounting extras (interior partition, positional payload pins, callable
  children-block span contiguity + post-space pin, children-share-parent's-source)
  kept as the in-crate test-side oracle with today's messages.
- The **Attached byte-tiling exclusion** lands NOW in the parse-law callable arm of
  the same invariants.rs implementation: children belonging to an `Attached`
  slot's region are skipped by the byte contiguity/containment/source checks
  (structural child-list tiling stays role-independent in validate_tree). Code
  TODO referencing S6 for per-source byte accounting of attached children.
- tests/acceptance.rs (integration crate) switches its 3 call sites to the public
  `validate_tree(&tree)` — forced by the ruled pub(crate) demotion (integration
  tests cannot reach pub(crate); report as forced consequence).
Tests: violation cases (unreachable/two-parent via hand-shapes where reachable,
region tiling, content containment, residency), a passing spliced shape that the
parse-law extras would reject, Attached-exclusion (attached-slot children in
another source pass the wrapper's callable arm).

### Milestone H — consumer polish

- Free `pub fn display_tree<L, A>(node: NodeRef<'_, L, A>) -> String` in a new
  node/display.rs, exported at core::node: one line per node — box-drawing guides
  (`├── `/`└── `/`│   `), `summary()`, line/col position (internal per-source
  `LineIndex`, cached per source Arc within the call), source label printed only
  when it CHANGES from the previous line (initial source omitted); format
  explicitly NOT a stability contract; ignores annotations.
- `NodeKind::as_str() -> &'static str` → "Chars"/"Group"/"Callable"/"Comment"/
  "List".
Tests: smoke (line count = subtree size; contains summaries + guides; line/col
present), as_str.

### Milestone I — docs

- Rustdoc on everything touched (the ruled sentences: WHO/WHEN minting, storage
  order loud sentence, exclusive end, single-source slice condition, wrap-around
  note, cross-tree restage sanction, body() ext-axis sentence, Hidden ≠
  read-invisibility, Lang required-method exception).
- DR status-line updates (surgical, honest scoping) on entries FULLY applied here:
  tree-tags; node-annotations (accessor half — extract-callback half stays S7);
  ext-minting; slot-roles (S4 generic machinery pending — scope note);
  named-first-constructors; tree-navigation; tree-validation; display-tree;
  restage-ops (level-0-only note); span-extend-to (contains landed).
- Superseded-names register: verify/add "at application" entries for:
  finalize_node, the tier-2 `<Kind>NodeExt` family, ProcessedNodeData,
  tree_identifier, add_subtree, ancestors, `ArgumentSpec::named`,
  `ParsedSlot::named`, `add_with_ext` (if register-worthy per its bar).
- CLAUDE.md / docs/ guide pages: update passages presenting now-changed API
  (grep finalize_node, `.named(`, builder access, check_tree_invariants).
- README snippet rlib check.

### Known risks / ambiguities spotted (to be resolved or reported)

1. "Attached byte-tiling exclusion lands inside validate_tree" (work order) vs.
   validate_tree = all-trees law MINUS byte accounting (DR/T5-F2): resolved by
   landing the exclusion in the parse-law arm of the SAME invariants.rs
   implementation (the wrapper's extras) — validate_tree itself has no byte-tiling
   to exclude. Interpreted; flagged for review.
2. acceptance.rs cannot call pub(crate) `check_tree_invariants` → switches to
   `validate_tree` (parse-law coverage narrows there; in-src suites keep the full
   oracle). Forced by the ruling.
3. `ParsedArgument::absent` must also demand the ext (the field loses Default) —
   composed arity `absent(spec, ext)`; the std path fills via its `Default` bound.
   Delegated-arity decision.
4. `A`-bounds realization: bound-where-used rather than struct bounds (crate
   doctrine); the ruled bound sentence documented on the parameter.
5. `TreeTag`/`NodeId::tree_tag()` public reachability — smallest surface making
   the ruled newtype meaningful. Delegated detail.
6. `StagedChildren` behavior on unstaged ids (get → None, iter skips; the
   subsequent add() diagnoses). Delegated detail (never-panic policy).
7. New `NodeBuildError` variants for restage_node (`ReplacementsLengthMismatch`,
   `ContentParentUnmapped`) — names not ruled; the enum is non_exhaustive.
   Delegated naming.
8. `BodyMarker` / `LatexlikeNodeExts` names — preset-side, not ruled; follow the
   LatexlikeDriver precedent. Delegated naming.
9. Latexlike stays monomorphic this stage (LLL generalization = S4); preset body
   minting written through `BodySlotExt` so S4 is mechanical.

## Handoff notes (written at the A+B stop point — read together with the plan above)

State at handoff: commits `4b91a01` (plan), `052e096` (milestone A), `3028fc3`
(milestone B). Gates at this point: `cargo build` 0 warnings; `cargo test` all
green (542 lib + 30 acceptance + 8 + 1 derive + 26 doctests, 2 doctests ignored —
one pre-existing, one the new ```ignore recipe example on
`NodeTreeBuilder::staged_children`); `rm -rf target/doc && cargo docs` clean.
Grep gates already clean for: `finalize_node`, `add_with_ext`, the tier-2
`<Kind>NodeExt` family (src + tests + docs/ + README). NOT yet checked/relevant:
`\.named(`, `ParsedSlot::named`, `ancestors()` (those fall in C1/C2/E).

### Discoveries / decisions inside delegated room (A+B)

1. **`TreeCore` fields beyond the ruled sketch**: `parent: Vec<u32>`
   (`NO_PARENT = u32::MAX` sentinel at root; finish() pass 1 already computed it)
   and `single_source: bool` are ALREADY stored and populated by `finish()` —
   milestone E only adds `NodeRef::parent()`/`index_in_parent()` accessors over
   `tree.core.parent`, and milestone F only reads `tree.core.single_source`.
2. **`A`-bounds realized bound-where-used** (plan risk #4): `Clone for
   NodeTree where A: Clone`, `Debug where A: Debug`; ruled bound sentence
   documented on the `NodeTree` type docs ("Annotations: the second generic
   parameter" section). `materialize()` is `where A: Clone` and layout-preserving.
3. **Builder annotations parallel vec**: `Staged<L>` deliberately does NOT carry
   `A` — that keeps `StagedNodes`/`StagedChildren` A-free (required: they appear
   in `make_node_ext`'s and stop predicates' signatures, and `Lang` never sees
   `A`). Don't move the annotation into `Staged` in later milestones.
4. **`NodeId` Debug format** is now `NodeId(1@17)` (index@tag); one test asserts
   the `NodeId(1@` prefix. `NodeId` derives Eq/Ord/Hash on field order
   `{ index, tree_tag }` (index-major order; not ruled, documented as total).
   `NodeId::tree_tag()` is public (plan risk #5).
5. **`NodeTree::node()` asserts the tag in ALL builds now** (the "panicking
   own-tree contract" made fully enforceable); `NodeRef::new` keeps only a
   debug_assert (internal call sites are same-tree by construction).
6. **`StagedChildren` unstaged-id behavior** (plan risk #6): `get` → `None`,
   `iter` skips; documented on the type; test
   `staged_children_is_descent_only_and_skips_unstaged_ids`.
7. **`cx.stage_node` signature**: returns `Result<BuildId, NodeBuildError>` so
   the existing `.map_err(|e| cx.implementation_error(e, span))` idiom at all 27
   converted sites still reads the same.
8. **The generic test harness in nodes_parser.rs** (`try_run`'s root staging,
   around line 1290) stages its root `List` via the explicit recipe with
   `L::make_node_ext` — it is generic over test langs with real ext types, so
   `(), ()` literals don't type there. Same pattern anywhere a generic-L test
   stages manually.
9. **Interim left for C1** (deliberate): `NodeExtTypes::ArgumentExt` /
   `SlotExt` still carry `Default`, and `ParsedArgument::provided/absent`,
   `ParsedSlot::new/named`, `ParsedArgumentNodes` still Default-fill exts. The
   `NodeExtTypes` rustdoc header already states the FINAL no-Default doctrine
   (one commit of doc/impl skew — C1 resolves it by dropping the two bounds and
   applying the ruled arities). The `ArgumentExt` assoc-type doc already
   describes the `where ArgumentExt<L>: Default` std-parser realization C1 must
   implement.
10. **`ext: ()` literals in `StateData { … }` are STATE exts** — do not confuse
    with the removed node exts when grepping (the 18 removed `ext:` lines were
    all inside `CallableData`/`GroupData` literals; a brace-matching pass was
    needed because plain grep windows miss the deep literals).

### Exact churn sites remaining (C1..I), verified by grep at handoff

- `.named(` builder calls: 10 sites — spec/mod.rs:95,170;
  constructs/argument_parsers.rs:1451,1806; node/mod.rs:358,360,362 and the
  named_accessors test (~1428-1432, now shifted). All become
  `ArgumentSpec::new(parser, "name")`.
- `ParsedSlot::named`: 7 sites — constructs/environment_parser.rs:769,911
  (region shifted after edits); node/mod.rs (4 sites); latexlike/environments.rs
  (the body-slot mint, search `ParsedSlot::named`).
- `ArgumentSpec::new(`: ~45 sites crate-wide (all currently pass
  `Arc::new(parser)` — the sealed conversion accepts `Arc<P>`, so
  `new_unnamed(Arc::new(p))` compiles unchanged; prettifying to by-value is
  optional churn).
- `StdCallableSpec::new(` sites pass `Vec<Arc<ArgumentSpec>>` today; the ruled
  `IntoIterator<Item = ArgumentSpec<L>>` (by value) breaks the sites that Arc
  and SHARE the spec with `ParsedArgument::provided` records (node/mod.rs tests,
  spec/mod.rs tests): those should switch to the pub `arguments` field literal
  (`StdCallableSpec { arguments: vec![...] }`) to keep sharing.
- `check_tree_invariants` in tests/acceptance.rs: 3 call sites (219, 226, 392)
  must switch to public `validate_tree` when G demotes it (integration crate
  cannot see pub(crate)).
- `NodeRef::body()` current impl = `slot_content_nodes(0)` sugar — C1 replaces
  it with the `is_body()` scan under `where SlotExt<L>: BodySlotExt`; its
  callers: latexlike tests + node/mod.rs tests (`body()` in slots_and_body,
  region_level_slot_content_has_no_content_parent).
- docs/ guide pages: `docs/guide.md` / `learn-by-example.md` etc. were NOT
  grepped for stale builder/`.named(` idioms beyond the finalize_node sweep
  (which found nothing in docs/) — milestone I must grep
  `\.named(|builder|check_tree_invariants|ancestors` over docs/ and README.
- CLAUDE.md's `core::node` line mentions `check_tree_invariants`? (not checked —
  verify at I; its architecture section otherwise still accurate after A+B).

### In-flight knowledge for specific milestones

- **C1 body-marker naming**: plan says `BodyMarker` + `LatexlikeNodeExts`
  (delegated names, LatexlikeDriver precedent). The preset's mint in
  environments.rs should be written through `BodySlotExt::make_body()` (trait
  call, type-inferred) so S4's LLL genericization is textual-only.
- **C1 `where ArgumentExt<L>: Default` targets**: GroupArgumentParser,
  OptionalGroupArgumentParser, MarkerArgumentParser, ExpressionParser,
  CharsGroupArgumentParser, EmbellishmentsArgumentParser,
  TackOnFieldsArgumentParser, VerbatimArgumentParser (their `ArgumentParser`
  impls), plus free `parse_declared_arguments` and `StdInvocationParser`'s
  `ConstructParser` impl (both construct `ParsedArgument::provided/absent`).
- **D restage_node**: copy.rs's `copy_node` is the intended degenerate-recursion
  rewrite target; its region translation helper `restage_region` is the code the
  primitive generalizes (prefix sums over replacement lengths replace the
  uniform `- base` shift). New `NodeBuildError` variants planned:
  `ReplacementsLengthMismatch`, `ContentParentUnmapped { parent: NodeId }`
  (enum is non_exhaustive).
- **E node_at/covering_slice**: the exact descent algorithm (incl. the
  foreign-source search phase vs same-source refine phase) is spelled out in the
  plan's Milestone E — it was derived carefully against the multi-source rulings;
  implement as written there.
- **G validate_tree**: plan risk #1 (where the Attached exclusion lands) is the
  one point the reviewing session may want to double-check with the user; the
  chosen reading: validate_tree = all-trees law with NO byte checks; the
  parse-law extras stay inside the pub(crate) `check_tree_invariants` wrapper in
  the same invariants.rs, and THAT is where the Attached byte-tiling exclusion +
  `// TODO(S6)` land.
- **Doctest count**: adding ```rust doctests (e.g. on `SourcePos`) will shift
  the doctest totals; the ignored-count baseline is 2.

### Additions at the C1+C2 stop point (successor agent 1 → D–I successors)

State: commits `52a4d94` (C1), `2ceafcb` (C2); all gates green (see Gate
results). Lib test count now 548 (was 542 at A+B; +4 C1, +2 C2).

1. **D-C1 is pending user confirmation** (see Deviations): `ParsedArgument.ext`
   is `Option<ArgumentExt<L>>`, `absent(spec)` takes no ext, and
   `parse_declared_arguments`/`StdInvocationParser` carry NO
   `ArgumentExt: Default` bound (only the 8 std argument parsers do). If the
   supervisor/user overrules, the compile-blocking chain documented in D-C1
   must be re-litigated first — do not "fix" it back mechanically.
2. **Milestone D (restage/copy)**: unaffected by D-C1 — copy.rs clones whole
   `ParsedArgument`/`ParsedSlot` records (`Clone` covers `role` + optional
   ext). S7's `RestagedArgument::absent(spec)` now maps 1:1 onto
   `ParsedArgument::absent(spec)`.
3. **Milestone I (superseded names)**: register additions due at application
   for `ArgumentSpec::named` (builder) and `ParsedSlot::named` — both are now
   gone from code; also verify `SlotRole`-adjacent vocabulary (`Derived` was
   considered-and-closed, T5-A9(iv)).
4. **Milestone I (DR status lines)**: [§dd-dr:slot-roles],
   [§dd-dr:named-first-constructors], [§dd-dr:registration-ergonomics] (the
   `IntoArgumentParser` extension), and the ArgumentExt/SlotExt half of
   [§dd-dr:ext-minting] are now APPLIED (C1/C2) — status lines must note the
   D-C1 shape (absent-ext `Option`) if confirmed.
5. **`NodeRef::body()` is now bounded** (`where SlotExt<L>: BodySlotExt`).
   Generic code over arbitrary `L` (milestones E/H: navigation, display_tree)
   must not call `body()` without the bound — `display_tree` should use the
   structural accessors only.
6. **`impl BodySlotExt for ()`** exists (is_body = true → body() = first slot
   on no-ext langs). Test langs need nothing special; a lang wanting "no body"
   semantics needs a real marker ext.
7. **Sealed-marker modules**: spec/callable.rs and spec/structure.rs each have
   a private `mod sealed` with `ByValue`/`SharedConcrete`/`SharedDyn` —
   file-local by design (S2 pattern); don't unify them.
8. **Line numbers in the earlier churn lists have shifted** (C1/C2 edits in
   node/mod.rs, argument_parsers.rs, environment_parser.rs); re-grep rather
   than trusting the A+B offsets. `check_tree_invariants` acceptance.rs sites
   for milestone G are now at lines 219/226/392 ± a few — re-grep.

### Additions at the D+E+F stop point (successor agent 2 → the G–I successor)

State: commits `340e9f8` (D), `34c85f7` (E), `5ed9524` (F) on top of `d812c74`.
Gates at this point: `cargo build` 0 warnings; `cargo test` 565 lib + 30
acceptance + 8 + 1 derive green; doctests 27 passed / 2 ignored (the +1 is the
new `SourcePos` doctest; ignored baseline unchanged); `rm -rf target/doc &&
cargo docs` clean; grep gates: zero `ancestors()` (src + docs/ + README), zero
`add_subtree`, zero "honest" in slice.rs.

1. **`restage_node` lives in copy.rs** (an `impl NodeTreeBuilder` block there —
   the region-translation home), not builder.rs; the public path is unchanged
   (`core::node::NodeTreeBuilder::restage_node`). copy.rs module docs were
   rewritten around the primitive; the internal names `copy.rs` /
   `copy_subtree_into` / `copy_node` were deliberately kept (crate-internal;
   the superseded-names ban is on public transform vocabulary — worth a
   sentence in milestone I's superseded-names verification if the register's
   bar is read more strictly).
2. **New `NodeBuildError` variants** (milestone D, delegated names, plan risk
   #7): `ReplacementsLengthMismatch { children, replacements }` and
   `ContentParentUnmapped { parent: NodeId }` — NodeId in a builder-error
   variant is new (the enum previously carried only `BuildId`s); Display arms
   name `restage_node` explicitly.
3. **The builder's "exactly one staging method" doc** got a parenthetical:
   `restage_node` is documented as *not* a second staging path (it clones and
   lowers onto `add`). Keep that sentence intact in any G–I doc sweeps.
4. **`deepest_containing` + `covering_child_run`/`binary_candidate_run`/
   `partition_point`/`verify_covering_run`** are private free fns at the bottom
   of tree.rs — shared by `node_at`/`covering_slice`. `verify_covering_run`
   is the authority on what a child run must satisfy (same-source, edge
   overlap+coverage, exact tiling `end == next start`, minimality); any
   covering failure degrades to the covering node as a single-node run, and
   `deepest_containing` prunes same-source non-containing subtrees (exact
   spans trusted). If G's validator work touches sibling-span expectations,
   note the tiling equality here is *verification*, not an invariant claim —
   restaged trees legitimately fail it and degrade.
5. **`NodeTree` in-crate accessors** now include `parent_table()` and
   `is_single_source()` (beside `nodes()`/`tree_tag()`/`make_id()`); use them
   rather than reaching through `tree.core` in new code (G's validator should
   too).
6. **Pre-existing "honest" occurrences** (the "accepted honest cost" idiom in
   builder.rs, arguments.rs, kind.rs, environment_parser.rs, node/mod.rs and
   docs/learn-by-example.md) were left alone: T5-F1's ban is scoped to the
   slice `span()`/`source_text()` rustdoc contracts ("honest slices" session
   jargon), and those files use a different, older idiom. Milestone I should
   consciously decide whether the ban is read wider (unruled — if so, surface
   to the user rather than silently rewording).
7. **`input_like_tree()` test helper** (node/mod.rs tests): a two-source
   `\input`-like shape (Attached slot, body `List` in a resolved source) —
   reusable for G's Attached-exclusion tests instead of building a new shape.
8. **Doc-link phrasing note**: `parent()`'s rustdoc contains the
   `iter::successors` recipe but deliberately never spells the rejected
   iterator's name (grep gate); keep it that way in I's sweeps. `SourcePos`'s
   rustdoc says "the query currency for position lookups over parsed trees"
   without naming `NodeTree` (S0 stays node-layer-free); the node_at/
   covering_slice rustdoc links point *at* source types, not the reverse.
9. **DR status lines due at I** for this chunk: [§dd-dr:tree-navigation]
   (applied — incl. the T4/T5 amendments' spellings), [§dd-dr:span-extend-to]
   (contains landed with docs+tests in the same commit; `overlaps` NOT added —
   `covering_slice`'s impl did not want it), [§dd-dr:restage-ops] (level-0
   half applied; visitor/ops/errors S7), and the honest-slices amendment under
   [§dd-dr:tree-navigation] (applied).

## What landed per work-order item

- **Item 1 (tree tags)** — landed in milestone A (commit 052e096): always-on
  `TreeTag` over a wrapping `core` atomic; `NodeId { index, tree_tag }` with tag
  in Eq/Ord/Hash; `get()` foreign-id rejection in all builds; `node()` panicking
  own-tree contract enforced via the tag; layout-preserving copies share the tag;
  wrap-around/misuse-detector/never-wire documented; regions stay untagged.
- **Item 2 (annotations)** — landed in milestone A: `NodeTree<L, A = ()>` over
  `Arc<TreeCore<L>>` + parallel `Vec<A>`; zero-copy `annotate` (storage order,
  loud doc, no Send bounds); `NodeRef::annotation()` / `NodeTree::annotations()`;
  no setter; O(annotations) clone; defaulted-A ripple through all view types;
  `ParseResult` spelling unchanged; extract stays `A = ()`.
- **Item 3 (ext minting)** — landed in milestone B (commit 3028fc3): see the
  commit message / handoff notes; the `ArgumentExt`/`SlotExt` halves (Default
  removal, `ParsedArgumentNodes` ext carriage, std-parser bounds, `SlotExt`
  demanded at `ParsedSlot` construction, `BodySlotExt` + preset claim) landed in
  milestone C1 (successor agent 1) — with deviation D-C1 on the absent-argument
  ext.
- **Item 4 (slot roles)** — landed in milestone C1: exhaustive `SlotRole` on
  `ParsedSlot` (default `Content`), `Hidden` read-blindness doc note, `Attached`
  byte-tiling exclusion documented on the variant (the validator arm itself is
  milestone G's).
- **Item 6, level-0 half (restage primitive)** — landed in milestone D (successor
  agent 2): `NodeTreeBuilder::restage_node(node, replacements, content_parents,
  annotation)` in copy.rs (public method on the builder; `core::node` path
  unchanged) — generic `AOld` on the input `NodeRef`, positional `&[Vec<BuildId>]`
  replacements, prefix-sum region translation (regions shrink/grow; emptied
  regions stay provided; `InRegion` content re-based through the same sums;
  `InChildrenOf` parents mapped through the callback, child offsets carried
  verbatim), ext/span/state cloned (ext **verbatim, never re-minted**),
  cross-tree sanctioned in rustdoc (no same-tree assert — the sanctioned splice
  door sentence), no `Send` bounds. New non_exhaustive `NodeBuildError` variants
  `ReplacementsLengthMismatch { children, replacements }` /
  `ContentParentUnmapped { parent: NodeId }` (delegated names, plan risk #7).
  copy.rs's `copy_subtree_into` rebased as the degenerate recursion (singleton
  replacements; old-id map keyed by `NodeId` now, not raw index; the old
  `expect` on a missing content parent became the `ContentParentUnmapped` `Err`
  path — panic-policy win, behavior unchanged for real copies). NO
  visitor/driver/ops/bundles/`RestageError` (S7). 8 new tests (drop/empty/
  multiply regions, cross-tree splice, unmapped parent, length mismatch,
  cloned-ext verbatim on MintLang, caller-supplied annotations).
- **Item 7 (navigation)** — landed in milestone E (successor agent 2):
  `NodeRef::parent()`/`index_in_parent()` (O(1) over the stored parent table; the
  `iter::successors` ancestry recipe in `parent()`'s rustdoc — the rejected
  iterator's name does not appear in docs, keeping the grep gate clean);
  `NodeRef::tree()` → pub; `SourcePos<O = Option<String>>` in `techy::source`
  beside `SourceSpan` (`new` with SourceSpan-parity debug asserts incl. `pos ==
  len` valid, `source()`/`pos()`, Clone/PartialEq(identity)/Eq/Debug, line/col
  documented via the `source().line_index()` route with a doctest);
  `SourceSpan::start_pos()`/`end_pos()` (exclusive-end sentence);
  `Span::contains(pos)` (half-open, empty-never-match, doc + tests in the same
  commit per [§dd-dr:span-extend-to]); `NodeTree::node_at(&SourcePos)` and
  `NodeTree::covering_slice(&SourceSpan)` in tree.rs over a shared
  `deepest_containing` per-source descent (exact plan algorithm: same-source
  match refines only through same-source children — includer queries stop at the
  `\input` node; foreign-source nodes are traversed, never matched);
  `covering_slice` computes the minimal child run with opportunistic binary
  search (`partition_point` candidates verified locally: same-source, edge
  overlap, edge coverage, gap-free tiling, minimality vs both neighbors) and a
  verified linear fallback; unverifiable runs degrade to the covering node as a
  single-node run; empty queries resolve by point containment; NO offset→index
  table; NO `ancestors()`. 8 new tests (parent/index + successors walk; node_at
  deepest/gap-offset/empty-span/foreign-source; per-source descent over an
  `\input`-like two-source tree incl. covering_slice; covering_slice
  single/multi/delimiter-fallback/empty/no-cover).
- **Item 8 (slice contracts)** — landed in milestone F (successor agent 2):
  `NodeSlice::span()`/`source_text()` now verify the **whole run** (private
  `is_single_source_run()`: O(1) via the stored `TreeCore.single_source` flag,
  else a scan of every node of the run); `source_text()` gained the ordering
  guard (`first start ≤ last end`) for contract parity; both rustdocs state the
  concrete condition ("the whole run lies within a single source") — the word
  "honest" appears nowhere in slice.rs; `None` = no single-source answer; the
  per-node-accessors-stay-valid sentence included. Slice module doc updated.
  Test: a foreign-source *middle* sibling spliced via `restage_node` — endpoints
  agree, whole run does not → `None` from both; sub-runs avoiding the foreign
  node still answer (scan path); parsed tree answers via the flag (asserted).
- **Item 5 (constructor reshapes)** — landed in milestone C2:
  `ArgumentSpec::new(parser, name)`/`new_unnamed` over sealed
  `IntoArgumentParser<L, M>` (S2-D1 marker realization; `'static` spelled on the
  by-value/`Arc<P>` impls — `ArgumentParser` has no `Any` supertrait);
  `.named()` removed; `StdCallableSpec::new(impl IntoIterator<Item =
  ArgumentSpec<L>>)` (shared-Arc sites moved to the pub-field literal);
  `ParsedSlot::new/new_unnamed` (C1); `ParsedArguments::new`/`ParsedSlots::new`
  (`From<Vec>` kept).
- **Item 9 (validation)** — landed in milestone G (successor agent 3): public
  `validate_tree<L, A>(&NodeTree<L, A>) -> Result<(), TreeViolation>` in
  invariants.rs, exported `core::node` — the all-trees law ONLY: structural sanity
  (in-bounds/after-parent/single-parent/reachability/root-unparented +
  Chars/Comment childless), region tiling in node-index space (regions resolved,
  tile the children block role-independently, content inside content parents,
  content parent inside its own region's subtree), `TextContent` residency
  (char-boundary range of the node's own source — NO positional pins). Explicitly
  NO byte accounting (multi-source/spliced trees pass — tested on the
  `input_like_tree` shape). Never panics: recomputes the parent relation instead
  of trusting the stored table, `.get()`-guards the content-parent index (an
  out-of-range recorded parent reports `ContentParentOutsideSubtree`, doc'd on the
  variant). `TreeViolation { node: Option<NodeId>, kind: TreeViolationKind }` —
  both non_exhaustive, Clone/Debug/PartialEq/Eq + Display (full detail, "node N:
  …" prefix) + `core::error::Error`; 15 kind variants named after the
  `NodeBuildError` vocabulary where checks correspond (`RegionNotTiling`,
  `ChildrenNotInRegions`, `SpannedContentInvalid`, `ContentParentOutside*`).
  `check_tree_invariants` → `pub(crate)` **`#[cfg(test)]`** panic-assert wrapper:
  `validate_tree(...)` Err → `panic!("tree invariant violated: {violation}")`,
  then the parse-law byte-accounting extras (interior partition, positional pins,
  callable contiguity, children-share-parent's-source) with today's exact
  messages — ONE implementation of the shared checks; the `// TODO(S6):` for the
  `Attached` byte-accounting scoping sits on the parse-law arm (supervisor-resolved
  reading: the exclusion lands at S6 with the `\input` wiring). tests/acceptance.rs
  switched to public `validate_tree(...).unwrap()` — **11 sites, not the 3 the
  earlier handoff listed** (a second block at lines 819–1028 had accrued).
  Public-doc mentions of the demoted name reworded (engine/language.rs
  `parse_source` recovery note; extract.rs module doc — now cites `validate_tree` +
  "parse-tree byte accounting"). 8 new tests (violations: multiple-parents,
  unreachable, region-tiling, residency; byte-accounting-free pass; multi-source
  pass; wrapper detail panic; valid/materialized accepted).
- Items 10–11: not started (handoff).

## Signature table (old → new) — A+B portion

| Old | New |
|---|---|
| `NodeTree<L>` (monolithic struct) | `NodeTree<L, A = ()>` = `{ core: Arc<TreeCore<L>>, annotations: Vec<A> }` |
| — | `NodeTree::annotations() -> &[A]`, `NodeTree::annotate<B>(f) -> NodeTree<L, B>` |
| — | `NodeRef::annotation() -> &'t A` |
| `NodeId(u32 [, u32 debug-only])`, index-only Eq/Ord/Hash | `NodeId { index: u32, tree_tag: TreeTag }`, tag in Eq/Ord/Hash; `tree_tag()` pub |
| — | `pub struct TreeTag(u32)` (no public constructor) |
| `NodeTree::get` (debug-only tag check) | rejects foreign ids in every build |
| `NodeExtTypes` (8 assoc types, all `Default`) | `{ NodeExt (no Default), ArgumentExt, SlotExt }` (last two lose `Default` in C1) |
| `Lang::finalize_node(&mut kind, &mut ext, span, state, &[BuildId], &StagedNodes)` (defaulted) | DELETED → required `Lang::make_node_ext(&kind, &span, &state, StagedChildren<'_>) -> NodeExt` |
| `NodeKind::Chars { content, ext }` / `Comment { …, ext }` / `List { ext }`; `GroupData.ext`; `CallableData.ext` | ext fields deleted; `List` is a unit variant |
| `NodeTreeBuilder<L>::add(kind, span, state, children)` + `add_with_ext(…, ext)` | `NodeTreeBuilder<L, A>::add(kind, span, parsing_state, children, ext, annotation)` (the only staging method) |
| — | `NodeTreeBuilder::staged_children(&self, &[BuildId]) -> StagedChildren<'_, L>` |
| — | `StagedChildren` / `StagedChildView` (descent-only staged views) |
| `ParserSession.builder` (pub field) | pub(crate) |
| `cx.session.builder.add(…)` (parser idiom) | `cx.stage_node(kind, span, state, children)` (mints + stages, annotation `()`) |
| `cx.session.builder.staged_nodes()` | `cx.staged_nodes()` |

## Signature table (old → new) — C1/C2 portion

| Old | New |
|---|---|
| — | `pub enum SlotRole { Content (default), Attached, Hidden }` (exhaustive; Copy/Eq/Hash/Default) |
| — | `pub trait BodySlotExt { is_body(&self) -> bool; make_body() -> Self }` + unit impl for `()` (every slot reports body) |
| `ParsedSlot { name, region, ext }` | `ParsedSlot { name, region, role: SlotRole, ext }` |
| `ParsedSlot::new(region)` (unnamed, Default-ext) | `ParsedSlot::new(region, name, role, ext)` (named form) |
| `ParsedSlot::named(name, region)` | REMOVED → `ParsedSlot::new(region, name, role, ext)` |
| — | `ParsedSlot::new_unnamed(region, role, ext)` |
| `ParsedArgument.ext: ArgumentExt<L>` (Default-filled) | `ParsedArgument.ext: Option<ArgumentExt<L>>` (`Some` ⟺ provided; see D-C1) |
| `ParsedArgument::provided(spec, region)` | `provided(spec, region, ext)` |
| `ParsedArgument::absent(spec)` | unchanged arity — now stores `ext: None` (D-C1) |
| `ParsedArgumentNodes { nodes, content }` (L-free) | `ParsedArgumentNodes<L> { nodes, content, ext: ArgumentExt<L> }` + `new(nodes, content, ext)` |
| `ArgumentParser::parse_argument -> …Option<ParsedArgumentNodes>` | `…Option<ParsedArgumentNodes<L>>` |
| std argument parsers: `impl<L: Lang> ArgumentParser<L> for …` | `… where ArgumentExt<L>: Default` (all 8: Group/OptionalGroup/Marker/Expression/CharsGroup/Embellishments/TackOnFields/Verbatim) |
| `NodeExtTypes::{ArgumentExt, SlotExt}: … + Default` | `Default` dropped (bundle carries none) |
| `NodeRef::body()` = `slot_content_nodes(0)` | first slot with `ext.is_body()`, `where SlotExt<L>: BodySlotExt` (ext axis only — doc sentence) |
| `Latexlike::NodeExts = ()` | `= LatexlikeNodeExts` (`NodeExt = ()`, `ArgumentExt = ()`, `SlotExt = BodyMarker`) |
| — | `latexlike::BodyMarker` (`not_body()`; `BodySlotExt` impl supplies `make_body()`; no Default) |
| `ArgumentSpec::new(parser: Arc<dyn ArgumentParser<L>>)` (unnamed) + `.named(name)` builder | `ArgumentSpec::new(parser, name)` + `new_unnamed(parser)` via sealed `IntoArgumentParser<L, M>`; `.named()` REMOVED; `.with_state_delta()` stays |
| — | sealed `IntoArgumentParser<L, M>` (by-value / `Arc<P>` / `Arc<dyn ArgumentParser<L>>`; S2-D1 marker idiom) |
| `StdCallableSpec::new(Vec<Arc<ArgumentSpec<L>>>)` | `new(impl IntoIterator<Item = ArgumentSpec<L>>)` (by value, Arc'd inside; shared-Arc sites use the pub `arguments` field) |
| `ParsedArguments`/`ParsedSlots`: `From<Vec>` only | + `new(Vec)` constructors (`From<Vec>` stays) |

## Signature table (old → new) — D/E/F portion

| Old | New |
|---|---|
| — | `NodeTreeBuilder::restage_node<AOld>(node: NodeRef<'_, L, AOld>, replacements: &[Vec<BuildId>], content_parents: impl Fn(NodeId) -> Option<BuildId>, annotation: A) -> Result<BuildId, NodeBuildError>` (cross-tree sanctioned; ext cloned verbatim) |
| — | `NodeBuildError::{ReplacementsLengthMismatch { children, replacements }, ContentParentUnmapped { parent: NodeId }}` |
| `copy_subtree_into` (bespoke recursion + region shift) | same signature, now the degenerate recursion over `restage_node` (NodeId-keyed id map) |
| `NodeRef::tree()` pub(crate) | pub |
| — | `NodeRef::parent() -> Option<NodeRef>` / `index_in_parent() -> Option<usize>` (O(1); successors recipe in `parent()` docs; no ancestry iterator) |
| — | `pub struct SourcePos<O = Option<String>>` in `techy::source` (`new(&Arc<Source<O>>, pos)`, `source()`, `pos()`; Clone/PartialEq(identity)/Eq/Debug) |
| — | `SourceSpan::start_pos()` / `end_pos()` (exclusive-end doc sentence) |
| — | `Span::contains(pos) -> bool` (half-open; empty spans never match) |
| — | `NodeTree::node_at(&SourcePos<L::SourceOrigin>) -> Option<NodeRef>` (deepest containing; per-source descent) |
| — | `NodeTree::covering_slice(&SourceSpan<L::SourceOrigin>) -> Option<NodeSlice>` (minimal covering sibling run; binary search + verified fallback) |
| `NodeSlice::span()`/`source_text()` (first/last endpoint check) | whole-run single-source verification (O(1) `finish()` flag fast path) + ordering guard on both |

## Delegated-arity decisions

1. **`ParsedSlot::new(region, name, role, ext)` / `new_unnamed(region, role, ext)`** —
   payload-first (region leads, the T3 §C6b mirror), then name (the named/unnamed
   split per [§dd-dr:named-first-constructors]), then role, ext last
   (consumer-data-last, matching the builder-`add` axis order
   identity→structure→consumer).
2. **`ParsedArgumentNodes::new(nodes, content, ext)`** — payload-first, ext last.
3. **`ParsedArgument::provided(spec, region, ext)`** — ext appended last; `absent(spec)`
   takes NO ext (deviation D-C1 — see below; matches T5-A2's ruled
   `RestagedArgument::absent(spec)` mirror).
4. **`impl BodySlotExt for ()`** with `is_body() = true` / `make_body() = ()`: the only
   coherent unit impl (`make_body().is_body()` must hold — coherence sentence added to
   the trait docs); consequence documented: on no-ext languages `body()` degenerates
   to the first slot, preserving the old slot-0 behavior for `TrivialLang`-style
   langs. Delegated-room call, flagged for review.
5. **Preset names `BodyMarker` / `LatexlikeNodeExts`** — as sketched in the plan
   (LatexlikeDriver naming precedent).
6. **`where ArgumentExt<L>: Default` placement** — on the 8 standard argument-parser
   impls ONLY (the ruling's literal target). NOT on `parse_declared_arguments` /
   `StdInvocationParser`: with D-C1 they need no bound (provided-arm ext comes from
   the parser output, absent-arm stores none), and bounding them would break
   `CallableSpec::make_invocation_parser`'s unconditional default body (see D-C1).
7. **`TreeViolationKind` name + variant slate** (milestone G) — the ruling names only
   `TreeViolation { node, kind, … }`; the kind enum follows the `NodeKind`/`TokenKind`
   `-Kind` convention, variants reuse `NodeBuildError`'s vocabulary where the checks
   correspond. Both types also derive PartialEq/Eq (the `NodeBuildError` precedent;
   violation tests assert exact values).
8. **`check_tree_invariants` is `#[cfg(test)]` on top of `pub(crate)`** (milestone G) —
   after the demotion every remaining caller is `#[cfg(test)]` code (the in-src test
   suites + `latexlike::test_support`, itself cfg(test)); without the gate the lib
   build carries dead code (a 0-warnings gate violation). The ruled "test-utility
   role" realized literally; the fn stays `pub(crate)` for crate-wide test use.

## Gate results

At the F stop point (successor agent 2; commits through F):
- `cargo build`: clean, 0 warnings.
- `cargo test`: 565 lib + 30 acceptance + 8 + 1 (derive) green; doctests 27
  passed / 2 ignored (baseline; +1 passing from the `SourcePos` doctest).
- `rm -rf target/doc && cargo docs`: clean, 0 warnings.
- Grep gates: zero `ancestors()` (src + docs/ + README — the recipe rustdoc
  deliberately never spells the name), zero `add_subtree`, zero "honest" in
  slice.rs (the F1-scoped ban; pre-existing "honest cost" idiom elsewhere
  flagged in handoff note 6).
- Ruled tests present: restage region translation (drop/empty/multiply),
  cross-tree restage, cloned-ext verbatim, unmapped parent Err, length-mismatch
  Err; parent/index_in_parent + successors walk; node_at deepest/empty-span/
  gap-offset/multi-source-descent; covering_slice minimal-run/delimiter-
  fallback/empty/no-cover; slice `None` on a restage-spliced mixed run;
  `Span::contains` edges.

At the C2 stop point (successor agent 1; commits through C2):
- `cargo build`: clean, 0 warnings.
- `cargo test`: 548 lib + 30 acceptance + 8 + 1 (derive) green; doctests 26
  passed / 2 ignored (baseline).
- `rm -rf target/doc && cargo docs`: clean, 0 warnings.
- Grep gates (src + tests + docs/ + README): zero `.named(` builder calls
  (only the `get_named`/`*_nodes_named` accessor family matches remain), zero
  `ParsedSlot::named`, zero `stage_argument_like`; `finalize_node`/
  `add_with_ext`/tier-2 family still zero.

## Churn stats

C1+C2 (on top of the A+B stats above):
- 35 single-arg `ArgumentSpec::new(…)` → `new_unnamed(…)`; 10 `.named("x")`
  chains → `ArgumentSpec::new(parser, "x")`; the doc/acceptance showcase sites
  prettified to by-value (no `Arc::new`).
- 7 `ParsedSlot::named` → `ParsedSlot::new(region, name, role, ext)`.
- 21 `StdCallableSpec::new` sites: 4 → the by-value iterator form, 16 → the
  pub-field literal (they share `Arc<ArgumentSpec>`s with parsed records), 1 →
  `StdCallableSpec::default()`.
- ~20 `ParsedArgument::provided` sites gained the ext argument; 9
  `ParsedArgumentNodes` literals → `new(nodes, content, ext)`; 8 std
  argument-parser impls gained `where ArgumentExt<L>: Default`.

## Deviations / ambiguities

- **D-C1 — absent arguments carry no ext** (`ParsedArgument.ext:
  Option<ArgumentExt<L>>`; `absent(spec)` unchanged in arity; the plan's spelling
  `absent(spec, ext)` + `where ArgumentExt<L>: Default` on
  `parse_declared_arguments`/`StdInvocationParser` NOT applied). The plan's
  spelling is **uncompilable**, verified two ways:
  1. `CallableSpec::make_invocation_parser`'s ruled-unchanged default body
     (`Box::new(StdInvocationParser::new(invocation))`) must type-check for every
     `L: Lang`; a conditional `impl … ConstructParser<L> for StdInvocationParser
     where ArgumentExt<L>: Default` fails that coercion (E0277), and putting the
     bound on the trait method ripples it through `ParseDriver` + the dispatch
     loops — the whole engine would demand `ArgumentExt: Default`, contradicting
     the ruled custom-parsers-mint-their-own open door.
  2. The expression-position bare-callable guard (argument_parsers.rs, dispatch
     path, generic over every `L`) constructs `ParsedArgument::absent` entries —
     an ext demand there bounds the core dispatch loop the same way.
  Resolution grounds: T5-A2 rules the transform mirror as
  `RestagedArgument::provided(spec, nodes, content, ext)` / **`absent(spec)`** —
  no ext on absent — and S7's generic restage driver could never conjure a
  non-Default ext for a hand-built absent bundle; P4 pt 3's bound sentence targets
  "the standard parsers" (the ArgumentParser impls), which keep it. Semantics:
  `ext` is `Some` exactly for provided arguments ("nothing was parsed, so nothing
  was minted" — population-is-initialization); documented on the field. The
  representable `{region: Some, ext: None}` literal mismatch is the accepted cost
  (constructors are the canonical path). **Flagged for supervisor/user
  confirmation**, S2-D1/D2/D3 precedent (forced deviation, implemented + flagged).

(final classification at close; live list above)

## Riders noticed for later stages

(to be filled)
