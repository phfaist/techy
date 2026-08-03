# Phase 3 — S3 report: node core (identity, annotations, ext minting, slot roles, navigation, validation)

Branch `phase3-s3-node-core`, based on `api-review` @ c45f126.

## Progress (live — updated at every milestone)

- [x] 0. Implementation plan written + committed (this document)
- [x] A. Tree tags + annotations core (`TreeTag`, `NodeId` identity, `TreeCore`/`Arc`, `annotate`, accessors; parent table + single-source flag stored)
- [ ] B. Ext minting (`make_node_ext` + `StagedChildren`, tier-2 deletion, hook-free 6-param `add`, `cx.stage_node`, `ParserSession::builder` → pub(crate), extract minting)
- [ ] C1. Slot roles + ext demands (`SlotRole`, `BodySlotExt`, `body()`, preset `SlotExt` claim, record arities, std parsers `where ArgumentExt<L>: Default`)
- [ ] C2. Constructor reshapes (`ArgumentSpec::new/new_unnamed` + `IntoArgumentParser`, `StdCallableSpec::new(IntoIterator)`, `ParsedArguments::new`/`ParsedSlots::new`)
- [ ] D. Level-0 `restage_node` (+ copy.rs rebased on it)
- [ ] E. Navigation (`parent`/`index_in_parent`, `SourcePos`, `start_pos`/`end_pos`, `Span::contains`, `node_at`, `covering_slice`, `tree()` pub)
- [ ] F. Slices single-source whole-run contract (fast-path flag wired)
- [ ] G. Validation (`validate_tree` + `TreeViolation`, `check_tree_invariants` → pub(crate) wrapper, Attached byte-tiling exclusion, S6 TODO)
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

## What landed per work-order item

(to be filled as milestones complete)

## Signature table (old → new)

(to be filled)

## Delegated-arity decisions

(to be filled)

## Gate results

(to be filled)

## Churn stats

(to be filled)

## Deviations / ambiguities

(final classification at close; live list above)

## Riders noticed for later stages

(to be filled)
