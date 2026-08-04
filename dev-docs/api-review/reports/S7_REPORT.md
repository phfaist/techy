# Phase 3 — S7 stage report: transform (restage) + extract annotations

Branch `phase3-s7-transform` off api-review 29db6da. Implementer worktree:
`/Users/philippe/projects/techy/.claude/worktrees/agent-a82b7f6baa6fb02d2`.

Ruling inputs: PHASE3_PLAN § Protocol + § S7; DESIGN_RATIONALE [§dd-dr:restage],
[§dd-dr:restage-ops], [§dd-dr:extract-annotations], [§dd-dr:node-annotations],
[§dd-dr:slot-roles], [§dd-dr:tree-tags], [§dd-dr:ext-minting],
[§dd-dr:tree-validation], [§dd-dr:superseded-names]; T5_RULINGS §§ A1–A9;
P4_RULING §§ 4–6; S3_REPORT (level-0 `restage_node`, D-C1 record shapes);
S5/S6 signature tables (current surface).

Baseline (must not regress): 661 lib + 30 acceptance + 8 derive-conditions +
1 derive + 28 doctests; `cargo build` 0 warnings; `cargo docs` clean.

## Progress

- [x] M1 — this plan (committed before any code work)
- [x] M2 — transform core: module, `Restage`, `RestageVisitor` + closure blanket,
      `RestageError<E>`, driver (`restage` entry, Descend/Emit, replacement map,
      `ContentParentDropped`), basic tests (9 tests; 670 lib green)
- [x] M3 — region ops + bundles (`RestagedArgument`/`RestagedSlot`,
      `restage_subtree`/`restage_children`/`restage_argument[_named]`/`restage_slot`/
      `restage_invocation`/`builder()`), argument-swap acceptance test
      (7 more tests; 677 lib green)
- [x] M4 — content-swap helpers (`restage_argument_with_content`/
      `restage_slot_with_content`) + tests (5 more tests; 682 lib green)
- [x] M5 — extract annotation minting: generalized copy machinery, the four
      producer triples, part contexts, input genericity, caller updates, tests
      (5 more tests; 687 lib + 30 doctests green)
- [x] M6 — docs + records + gates (rustdoc pass, DR status lines, ARCHITECTURE,
      CLAUDE.md, lib.rs, guide pages; superseded sweep; full gate run; closure)

## Design synthesis (records → today's code)

### A. `techy::transform` — the streaming restage driver

New top-level public module (`techy/src/transform/` — `pub mod transform;` in
lib.rs, the `extract` pattern: the module itself is the facade; internal file
split invisible).

**Entry** ([§dd-dr:restage], [§dd-dr:restage-ops] A1):

```rust
pub fn restage<L, A, B, V>(
    tree: &NodeTree<L, A>,
    visitor: &mut V,
) -> Result<NodeTree<L, B>, RestageError<V::Error>>
where L: Lang, V: RestageVisitor<L, A, B> + ?Sized
```

Walks the frozen input top-down (visitor decides before descent), stages
bottom-up into an internal `NodeTreeBuilder<L, B>`, finishes on the root's
replacement. The root must restage to exactly **one** node — `Emit` of zero or
several nodes for the root is an error (no auto-wrap possible: a synthesized
`List` would need an annotation the driver cannot conjure — the single-pathway
rule; see D-plan-3).

**Visitor** (A1, A7):

```rust
pub trait RestageVisitor<L: Lang, A, B> {
    type Error;
    fn restage(&mut self, node: NodeRef<'_, L, A>,
               cx: &mut RestageContext<'_, L, A, B>)
        -> Result<Restage<B>, Self::Error>;
}
pub enum Restage<B> { Descend(B), Emit(Vec<BuildId>) }
```

plus the closure blanket `impl … for F where F: FnMut(NodeRef<'_, L, A>,
&mut RestageContext<'_, L, A, B>) -> Result<Restage<B>, E>` (E constrained
through the `FnMut` output projection). No `Send`/`Sync` bounds anywhere (H(b)).
`Descend(b)` ALWAYS descends (safety invariant: the only unvisited subtree is
under an explicit `Emit`); `Emit(nodes)` = callback staged the replacement
itself, empty = drop, no automatic descent. Descent is structural and
role-blind: `Attached` and `Hidden` slot children are visited uniformly (A9(v)
doc sentence).

**Errors** (A1, A3): one generic enum, `#[non_exhaustive]` (crate error-enum
precedent: `NodeBuildError`, `ExtractError`, `TreeViolation`):

```rust
pub enum RestageError<E> {
    Build(NodeBuildError),
    ContentParentDropped { callable: NodeId, parent: NodeId, replaced_by: Option<usize> },
    Visitor(E),
    // op-misuse variants forced by the ruled fallible ops (D-plan-1):
    UnknownArgumentName { node: NodeId, name: String },
    ArgumentIndexOutOfRange { node: NodeId, index: usize, count: usize },
    SlotIndexOutOfRange { node: NodeId, index: usize, count: usize },
    NotACallable { node: NodeId },
    ArgumentAbsent { node: NodeId, index: usize },
    RootNotSingular { count: usize },
}
```

`derive(Debug, Clone, PartialEq)` (Clone/PartialEq conditional on E via derive —
the ruled `Clone where E: Clone`), `Display where E: Display`,
`Error where E: Error + 'static` with `source()`. `ContentParentDropped`'s
Display points at the takeover route (Emit + `restage_invocation`/raw builder);
`replaced_by` is `None` when the parent was never individually restaged (an
ancestor was taken over via `Emit`), `Some(0)` dropped, `Some(n≥2)` multiplied —
re-anchoring is ill-defined in every case (A3). Exact variant/field names to be
finalized at implementation and recorded here (they anchor S8's mirror).

**Error plumbing** (realization detail, D-plan-2): visitor-taking ops return
`Result<T, RestageError<V::Error>>`; visitor-free ops (`restage_invocation`,
the `_with_content` helpers) are generic over `E`, returning
`Result<T, RestageError<E>>` (E inferred at the `?` site; no extra `From` impls,
so inference is unambiguous). Two documented usage patterns: a non-reentrant
closure picks an error type it can convert op errors into (e.g. its own enum,
or `RestageError<Infallible>`-shaped types); a reentrant trait visitor declares
its own error enum with one `From<RestageError<Self>>` impl (boxed recursive
variant). The driver-side rule stays exactly as ruled: op failures a visitor
propagates surface as `Visitor(E)`; driver-detected failures surface as their
own variants.

**Context + ops** (A2): `RestageContext<'t, L, A, B>` holds the input tree
reference (lifetime anchor), the private `NodeTreeBuilder<L, B>`, and the
replacement map `HashMap<NodeId, Replaced>` (`One(BuildId)` | `Count(usize)`)
recorded for every restaged node — the `content_parents` oracle for level-0
translation and the `ContentParentDropped` diagnosis. Ops (nodes may come from
ANY tree — the level-0 cross-tree sanction lifts to the ops):

- `restage_subtree(node, visitor) -> Result<Vec<BuildId>, RestageError<V::Error>>`
  — the full visitor over the subtree, root included;
- `restage_children(node, visitor) -> Result<Vec<BuildId>, …>` — per-child
  drives, concatenated;
- `restage_argument(node, index, visitor) -> Result<RestagedArgument<L>, …>` /
  `restage_argument_named(node, name, visitor)` (unknown name = `Err`) — drives
  the region's nodes, translates the record to bundle-relative staging
  coordinates (prefix sums; `InRegion` re-based, `InChildrenOf` mapped through
  the replacement map — dropped/multiplied parent = `ContentParentDropped`);
  an absent argument yields `RestagedArgument::absent(spec)` (presence
  transfers);
- `restage_slot(node, index, visitor) -> Result<RestagedSlot<L>, …>`;
- `restage_invocation(node, arguments: Vec<RestagedArgument<L>>,
  slots: Vec<RestagedSlot<L>>, annotation: B) -> Result<BuildId, RestageError<E>>`
  — rebuilds the callable over the bundles **in the order given**, retiling
  records (running child offsets); name/type/spec/invocation-syntax/span/state/
  ext cloned from `node`; the swap paradigm `\a{1}{2}` → `\a{2}{1}`;
- `builder() -> &mut NodeTreeBuilder<L, B>` — raw access underneath everything
  (the canned ops are conveniences, not the power boundary; the explicit
  `make_node_ext` two-line recipe for new nodes applies here).

**Bundles** (A2): opaque-but-constructible, same field vocabulary as the
records:

```rust
RestagedArgument::provided(spec: Arc<ArgumentSpec<L>>, nodes: Vec<BuildId>,
                           content: ContentNodes, ext: ArgumentExt<L>)
RestagedArgument::absent(spec)                     // ext-free, mirrors D-C1
RestagedSlot::new(name, role, nodes, content, ext) // ruled arity, name first
RestagedSlot::new_unnamed(role, nodes, content, ext) // T3 named-first mirror
```

**No silent repair** (A3): a region whose members were all dropped restages as
provided-with-empty-region (level-0 already does this); a dropped/multiplied
`InChildrenOf` content parent is `ContentParentDropped` (the driver upgrades
level-0's `ContentParentUnmapped` with cause + remedy).

**Content-swap helpers** (A4): `restage_argument_with_content(node, index,
content: Vec<BuildId>, annotation: B)` / `restage_slot_with_content(…)`
`where B: Clone` — produce bundles; wrapper syntax and noise restaged verbatim
*by contract* (never through the visitor), content swapped, designation
re-anchored onto the copied content parent (`InRegion` designations re-ranged
in place). The verbatim copies' annotations are clones of the explicit
`annotation` argument — the single-pathway rule demands an explicit channel the
ruled 3-arg spelling did not name (D-plan-4). Changing noise = the visitor op
or the hand-built bundle (both-taking helper stays rejected). A content parent
that is itself a callable breaks its own records' tiling and surfaces as
`Build(…)` from the staged add — documented, not specially repaired.

**Read frozen / write staged** (module + type docs): callbacks inspect the
frozen input (full read API + `techy::extract`), staged output is write-only;
a `Descend` parent never sees its children's results (takeover or two passes);
multi-stage is deliberately cheap. Origin tracking is the documented
annotation convention (`Ann { original: node.id(), … }` + the O(n) inversion
walk recipe) — "original node" vocabulary, never "provenance"/"origin" alone.

### B. Extract annotation minting ([§dd-dr:extract-annotations], A8)

- `copy_subtree_into` (node/copy.rs, crate-internal) generalized:
  `copy_subtree_into<L, AOld, B>(builder: &mut NodeTreeBuilder<L, B>,
  node: NodeRef<'_, L, AOld>, annotate: &mut impl FnMut(NodeRef<'_, L, AOld>) -> B)`.
- The four producers gain input genericity (`NodeSlice<'_, L, A>` for any A —
  the A8 rider) and the general callback owning the bare name; result types
  renamed/extended: `Split` → `SplitAtChars<L, B = ()>`; `KeyVals<L, B = ()>`.

  ```rust
  split_at_chars(nodes, sep, f) -> Result<SplitAtChars<L, B>, ExtractError>
  split_at_chars_drop_annotations(nodes, sep)          // B = ()
  split_at_chars_keep_annotations(nodes, sep)          // A→A, A: Clone + Default
  // same triple: parse_keyval, split_embellishments, split_tack_on_fields
  ```

  `_keep_annotations`: clone-through from `original()`, `A::default()` for
  synthesized nodes (bound-where-used only).
- Part contexts (one per result family, opaque, accessor-based):
  `SplitAtCharsPart<'t, L, A>` and `KeyValsPart<'t, L, A>` (lifetime/param
  details at implementation); the callback is invoked once per **staged output
  node** (copies, boundary partials, synthesized wrappers/roots). Accessors
  under the inclusion test: `original() -> Option<NodeRef<'t, L, A>>` (`None`
  exactly for synthesized nodes: segment/value `List` wrappers and the root),
  `is_partial() -> bool`, `partial_text() -> Option<&str>` (cut-piece info;
  partials arise only where separators cut through chars nodes), and the
  discriminant: `segment_index() -> Option<usize>` on `SplitAtCharsPart` /
  `entry_index() -> Option<usize>` on `KeyValsPart` (`None` for the root).
  Accessor names fixed at application per the delegation (D-plan-5); no
  `key()` accessor (the ruled roster is original + partial info + index; keys
  are recoverable from the result).
- `KeyVals::get_combined_with` returns an annotation-free tree
  (`NodeTree<L, ()>`) — it is a reader convenience, not one of the four
  producers (D-plan-6).
- Boundary doc sentence: the callback mints annotations only — vetoing or
  modifying nodes is restage's job.
- In-crate callers (`constructs/embellishments_parser.rs`,
  `constructs/tack_on_parser.rs`, `latexlike/arguments.rs`, extract's own
  doctest/tests, docs/learn-by-example.md) move to the `_drop_annotations`
  spelling or supply callbacks where the test gains coverage.

### C. Records + docs

- DR status lines: [§dd-dr:restage] + [§dd-dr:restage-ops] +
  [§dd-dr:extract-annotations] → applied (Phase 3 S7); [§dd-dr:node-annotations]
  extract-side note; [§dd-dr:slot-roles] restage-descent clause application
  note where its status line tracks stages.
- ARCHITECTURE.md: [§dd-arch:nodes] "Still ruled, not yet applied" passage
  split (transform + extract annotations now applied; recompose/visit remain);
  the "future `techy::transform`" line in the topology passage.
- CLAUDE.md + lib.rs: facade lists gain `techy::transform`; extract line
  renames `Split` → `SplitAtChars`.
- docs/learn-by-example.md: split usage updated to the new spelling.
- Full rustdoc on everything new (missing_docs stays zero).

## File map

- `techy/src/transform/mod.rs` — module docs (contracts above), `pub use`,
  `Restage`, `RestageVisitor` + blanket, `RestageError`.
- `techy/src/transform/context.rs` — `RestageContext`, driver recursion, ops,
  content-swap helpers, `restage` entry.
- `techy/src/transform/bundles.rs` — `RestagedArgument`, `RestagedSlot`.
- `techy/src/transform/tests.rs` — the test suite (swap round-trip, annotation
  flow, no-silent-repair, descent uniformity, helper contracts, error paths).
- `techy/src/lib.rs` — `pub mod transform;` + module list docs.
- `techy/src/node/copy.rs` — generalized `copy_subtree_into`.
- `techy/src/extract.rs` — triples, part contexts, renames, input genericity,
  test updates.
- Records/docs files per § C.

## Test plan (acceptance from § S7 + design-forced cases)

1. Argument-swap round-trip: `\a{1}{2}` → `\a{2}{1}` via a reentrant trait
   visitor (two `restage_argument` calls + one reordered `restage_invocation`);
   verify content/order, `validate_tree` green (sibling spans out of source
   order are legal in transform trees).
2. Annotation-flow: origin-convention pass (`Descend(Ann { original })`);
   explicit annotations through ops/raw builder on Emit paths.
3. Extract triples on all four producers: general mint, `_drop_annotations`
   parity, `_keep_annotations` clone-through + `Default` on synthesized nodes;
   partial facts and segment/entry indices; input genericity (re-split an
   annotated tree).
4. No-silent-repair: emptied region survives provided-with-empty-region;
   dropped and multiplied content parents → `ContentParentDropped` (message
   mentions the takeover route).
5. Uniform descent into `Attached` and `Hidden` slot children.
6. Op misuse errors: unknown argument name, index out of range, non-callable,
   `_with_content` on an absent argument; root dropped/multiplied at the entry.
7. Content-swap helpers: wrapper + noise verbatim, designation re-anchored,
   deep content parent (`[{…}]`-style), `InRegion`-designated argument.
8. Closure-blanket smoke test (`restage(&tree, &mut |node, cx| …)`) — also the
   probe for the recorded inference-fallback flag.

## Milestones

Commit per milestone (`P3-S7 M<k>: <what>`); each lands green (build + lib
tests at minimum; full gates at M6).

- M1: this plan.
- M2: transform core (mod.rs skeleton, Restage, visitor + blanket, error enum,
  context + drive recursion, entry; tests: identity restage, annotation flow,
  drop, root errors, closure smoke test).
- M3: bundles + region ops + restage_invocation + builder(); swap acceptance
  test + op-misuse tests + descent-uniformity tests.
- M4: content-swap helpers + tests.
- M5: extract reshape (copy generalization, triples, part contexts, renames,
  caller/doc updates in-crate; tests).
- M6: records + docs + full gates + closure tables in this report.

## Risks

1. **Closure-blanket inference** (the recorded flag): HRTB inference for
   two-parameter generic closures is fragile; if `restage(&tree, &mut |node, cx| …)`
   needs parameter-type annotations in practice, that is tolerable; if it fails
   outright, the recorded fallback (fixed-error shape) is a flag-level change —
   report, don't re-session.
2. **Visitor-taking ops + `?` ergonomics** — addressed by D-plan-2's two
   patterns; watch the tests for awkwardness worth flagging.
3. **`_with_content` recursion** over paths to deep content parents (the
   `[{arg with ]}]` shape) — mirrors level-0 arithmetic; dedicated tests.
4. **Extract churn**: the four producers' signature changes ripple into
   in-crate tests and docs; mechanical but broad.

## Deviations / delegated decisions (running list — for user sign-off)

- **D-plan-1** (FORCED by the ruled op contracts): `RestageError` carries
  op-misuse variants beyond the ruled three (`UnknownArgumentName` — the ruled
  unknown-name `Err` needs a type — plus index-out-of-range, non-callable,
  absent-argument, and root-not-singular variants; panic policy: contract
  violations return `Err`, and the ruled three variants cannot express these).
  The ruled three keep their exact ruled names; S8 mirrors whatever this
  roster is (the recompose amendment anticipates the anchor role).
  `#[non_exhaustive]` per the crate's uniform error-enum precedent.
- **D-plan-2** (realization, under-determined by records): op error plumbing —
  visitor-taking ops return `RestageError<V::Error>`; visitor-free ops generic
  `RestageError<E>`; no library `From` impls between nestings (keeps `?`
  inference unambiguous); the two documented visitor error patterns.
- **D-plan-3** (FORCED): the root's replacement must be exactly one staged node
  (dedicated error variant) — a driver-synthesized wrapper would need an
  annotation the single-pathway rule forbids conjuring.
- **D-plan-4** (FORCED by the single-pathway rule): the `_with_content` helpers
  take an explicit `annotation: B` argument (`B: Clone`, cloned onto every
  verbatim-restaged wrapper/noise node) — the ruled 3-arg spelling named no
  annotation channel, but every restaged node's annotation must pass through
  the visitor or an explicit op argument.
- **D-plan-5** (delegated at ruling: final names): part contexts
  `SplitAtCharsPart` / `KeyValsPart` (aligned with the ruled result-type
  rename; the working name `SplitPart` would resurrect the superseded bare
  `Split`); accessors `original()`, `is_partial()`, `partial_text()`,
  `segment_index()` / `entry_index()`. No `key()` accessor (outside the ruled
  roster).
- **D-plan-6** (realization, record silent): `KeyVals::get_combined_with`
  builds an annotation-free (`A = ()`) result tree; a callback parameter stays
  additive later (it is not one of the four ruled producers).
- **D-plan-7** (M2; realization the paradigm strip pass forces): the driver
  **translates** `InChildrenOf` content ranges through the content parent's own
  children replacements when the parent was driver-restaged (`Descend`), so an
  interior drop/multiplication inside a `{…}` wrapper keeps the enclosing
  callable's record meaning "the replacements of the designated children" —
  without this, dropping any node inside a group argument breaks the record
  (`ContentOutOfBounds`), killing the one-line strip pass the rulings protect.
  Realized crate-internally as
  `NodeTreeBuilder::restage_node_with_content_mapping` +
  `ContentParentMapping { Verbatim, Translate }` (node/copy.rs) so the ONE
  region arithmetic is shared; the *public* level-0 `restage_node` keeps its
  ruled verbatim-carry contract unchanged (it is the all-`Verbatim`
  specialization). Ranges into single-node `Emit` takeovers stay verbatim
  (the visitor chose the replacement's shape; re-validated at staging).

(Further entries appended as implementation forces them.)

### M2 realization notes

- `RestageError` variant roster as planned (D-plan-1): `Build`,
  `ContentParentDropped { callable, parent, replaced_by: Option<usize> }`,
  `Visitor`, `UnknownArgumentName { node, name }`,
  `ArgumentIndexOutOfRange { node, index, count }`,
  `SlotIndexOutOfRange { node, index, count }`, `NotACallable { node }`,
  `ArgumentAbsent { node, index }`, `RootNotSingular { count }` (the last five
  land with their ops in M3/M4). Derives `Debug, Clone, PartialEq, Eq`
  (conditional on E), `Display where E: Display`,
  `Error where E: Error + 'static`.
- `RestageContext` carries no borrow of the input tree; `'t`/`A` are anchored
  via `PhantomData<&'t NodeTree<L, A>>` (ops accept nodes from any tree, so a
  stored borrow would be misleading).
- The replacement map (`Replaced { Restaged { id, prefix }, One, Count }`)
  records every driven node; `Descend` entries carry the replacement-length
  prefix sums (the D-plan-7 translation table).
- **Closure-blanket inference finding** (the recorded T5 flag): the
  `restage(&tree, &mut |node, cx| …)` spelling works when the closure's two
  parameter *types* are annotated (`|node: NodeRef<'_, Latexlike>, cx: &mut
  RestageContext<'_, Latexlike, (), B>|`) — the elided lifetimes are accepted
  as higher-ranked. Fully unannotated closures do not infer (no expected-type
  propagation through a generic `V`), and a closure's `E` needs one annotated
  `Ok::<_, E>`/turbofish when only inferable from context. Judged tolerable
  (annotations, not restructuring); fn items need nothing. The fixed-error
  fallback is NOT triggered — flagged here per the ruling instead of
  re-sessioning.

### M3 realization notes

- The ops land as planned (§ A); shared tail `RestageContext::restage_region`
  (private) drives a resolved region's nodes and translates the content
  designation to bundle-relative coordinates under the same three-way policy as
  the driver (translate through `Descend`-restaged parents, verbatim into
  single-node `Emit` takeovers, `ContentParentDropped` otherwise).
- `RestagedArgument` internally allows a provided region with a missing ext:
  `restage_argument` reproduces verbatim what an incoherent hand-built input
  record carries (`ParsedArgument` fields are pub, so region-Some/ext-None is
  representable) rather than panicking or repairing; the public
  `provided(…)` constructor still demands the ext (the ruled arity). Below
  deviation grade — recorded for the reviewer.
- `restage_invocation` documents that bundles define the new child list
  exhaustively (children of the input callable outside every bundle are not
  part of the replacement).
- The documented reentrant error pattern (`struct OpError(Box<RestageError<
  OpError>>)` + one `From` impl) is exercised by every M3 test visitor; `?`
  propagation through both op families works with unique inference.
- The swap test additionally pins: bundle reordering moves whole records
  (spec + name travel with content — `arguments().get(0).name() ==
  Some("closing")` after the swap), and sibling spans out of source order pass
  `validate_tree`.

### M4 realization notes

- Helper signatures as planned (D-plan-4): `restage_argument_with_content(node,
  index, content: Vec<BuildId>, annotation: B)` /
  `restage_slot_with_content(…)`, both `where B: Clone`, returning bundles.
  The record's spec/ext (argument) and name/role/ext (slot) carry over
  verbatim — the "one-line spec/ext transcription" the rejected both-taking
  helper would have duplicated happens inside the helper.
- Verbatim copies route through the generalized crate-internal
  `copy_subtree_into(builder, node, annotate)` (pulled forward from the M5
  plan: node/copy.rs now takes an annotation-mint callback; extract.rs and
  node/mod.rs test call sites pass `&mut |_| ()` until M5 threads the real
  extract callbacks).
- Documented helper boundary: a wrapper chain containing a *callable* (whose
  own records would need retiling around the swap) is outside the helper's
  contract and surfaces as `Build` (the staged add rejects the callable's
  already-resolved records); the visitor route or a hand-built bundle covers
  those. Content parents deeper than the region node (the `\o[{…}]` lone-group
  unwrap shape) are handled via path descent with the swap at the innermost
  node.
- Empty-but-provided content ranges splice at their anchored position
  (`\a{}{2}` fill test).
- Test-relevant parse fact: whitespace after the command word is the macro's
  `post_space` payload (S5), NOT region noise — inter-argument whitespace is
  the region-noise case the wrapper/noise tests pin (`\a{1} {2}`).

### M5 realization notes

- Producer signatures land per the plan (§ B): the general forms take
  `impl FnMut(&SplitAtCharsPart<'t, L, A>) -> B` /
  `impl FnMut(&KeyValsPart<'t, L, A>) -> B`; the callback fires once per staged
  output node (copies via the generalized `copy_subtree_into` mint route,
  boundary partials, synthesized wrappers/roots). Unlike the restage visitor's
  generic-`V` seam, the `impl FnMut` parameter gives closures full
  expected-type inference — no annotations needed at call sites (the fn
  doctest shows the bare `|part| …` spelling).
- Part contexts: `SplitAtCharsPart<'t, L, A = ()>` / `KeyValsPart<'t, L, A = ()>`
  (D-plan-5 names), opaque wrappers over one internal `PartFacts` currency;
  the internal mint plumbing is `&mut dyn FnMut(PartFacts…) -> B` to keep the
  shared helpers' signatures small. `is_partial()` answers via
  `partial_text().is_some()`; the `_keep_annotations` mints are shared free
  fns (`keep_annotation`/`keep_keyval_annotation`).
- `Split` → `SplitAtChars<L, B = ()>`; `KeyVals<L, B = ()>` + `KeyValEntry<'k,
  L, B = ()>` (the entry view rides the rename; segment/value accessors return
  `NodeSlice<'_, L, B>`).
- Entry-index semantics: the index counts *entries* (source order, duplicates
  included), not value lists — an entry without a value consumes an index with
  no minted nodes (pinned by the tack-on smoke test).
- `content_as_chars` + the internal piece machinery generalized over the input
  annotation type (`A: 't` bound where the compiler demands it);
  `copy_subtree_into` gained the named input-tree lifetime its stored-`NodeRef`
  callbacks need.
- Call-site sweep: constructs/{embellishments,tack_on}_parser.rs tests,
  latexlike/arguments.rs tests, extract's own tests/doctests, and
  docs/learn-by-example.md moved to `_drop_annotations` (semantics unchanged);
  the guide's prose notes the callback-taking bare names.
- Annotation-flow coverage: the general mint pinned end-to-end on
  split_at_chars (originals, partial text, segment indices, synthesized-node
  `None`s) and parse_keyval (entry indices, cut text, input genericity +
  producer composition); embellishments general+keep; tack-on general+keep
  smoke (its value path shares `stage_segment_list`/`finish_keyvals` with
  keyval, and its `_drop` form runs the pre-existing positive-path tests in
  constructs/tack_on_parser.rs).

## Consolidated stage summary (M6 closure)

### Outcome

All §S7 scope items landed, no escalations: no rulings tension surfaced; every
judgment call is queued as D-plan-1..7 above. The stage is COMPLETE pending
review + user sign-off of the deviation list.

### Signature table (new/changed public surface)

| Item | Signature / shape |
|---|---|
| `transform::restage` | `restage<L, A, B, V>(tree: &NodeTree<L, A>, visitor: &mut V) -> Result<NodeTree<L, B>, RestageError<V::Error>> where L: Lang, V: RestageVisitor<L, A, B> + ?Sized` — top-down visits, bottom-up staging; root must restage to exactly one node |
| `transform::RestageVisitor<L, A, B>` | `type Error; fn restage(&mut self, node: NodeRef<'_, L, A>, cx: &mut RestageContext<'_, L, A, B>) -> Result<Restage<B>, Self::Error>`; blanket impl for `FnMut(NodeRef<'_, L, A>, &mut RestageContext<'_, L, A, B>) -> Result<Restage<B>, E>`; no `Send`/`Sync` bounds |
| `transform::Restage<B>` | `enum { Descend(B), Emit(Vec<BuildId>) }` (`Clone` where `B: Clone`, `Debug`) — Descend ALWAYS descends (role-uniform incl. `Attached`/`Hidden` slot children); Emit = callback-staged replacement, empty = drop, no automatic descent |
| `transform::RestageError<E>` | `#[non_exhaustive] enum { Build(NodeBuildError), ContentParentDropped { callable: NodeId, parent: NodeId, replaced_by: Option<usize> }, Visitor(E), UnknownArgumentName { node, name: String }, ArgumentIndexOutOfRange { node, index, count }, SlotIndexOutOfRange { node, index, count }, NotACallable { node }, ArgumentAbsent { node, index }, RootNotSingular { count } }`; derives `Clone, Debug, PartialEq, Eq` (conditional on E); `Display where E: Display` (ContentParentDropped names the takeover route); `Error where E: Error + 'static` |
| `transform::RestageContext<'t, L, A, B>` | `builder() -> &mut NodeTreeBuilder<L, B>`; `restage_subtree(node, visitor)` / `restage_children(node, visitor)` → `Result<Vec<BuildId>, RestageError<V::Error>>`; `restage_argument(node, index, visitor)` / `restage_argument_named(node, name, visitor)` → `Result<RestagedArgument<L>, …>`; `restage_slot(node, index, visitor)` → `Result<RestagedSlot<L>, …>`; `restage_invocation<E>(node, Vec<RestagedArgument<L>>, Vec<RestagedSlot<L>>, annotation: B) -> Result<BuildId, RestageError<E>>` (bundles in the order given, records retiled); `restage_argument_with_content<E>(node, index, content: Vec<BuildId>, annotation: B) where B: Clone` / `restage_slot_with_content<E>` → bundles; input nodes may come from any tree; re-driving a node is legal (map keeps the latest) |
| `transform::RestagedArgument<L>` | `provided(spec: Arc<ArgumentSpec<L>>, nodes: Vec<BuildId>, content: ContentNodes, ext: ArgumentExt<L>)` / `absent(spec)` (ext-free, mirrors D-C1); accessors `spec()`, `is_provided()`, `nodes()`; `Debug` |
| `transform::RestagedSlot<L>` | `new(name: impl Into<Box<str>>, role: SlotRole, nodes: Vec<BuildId>, content: ContentNodes, ext: SlotExt<L>)` / `new_unnamed(role, nodes, content, ext)`; accessors `name()`, `role()`, `nodes()`; `Debug` |
| `extract::split_at_chars` | `<'t, L, A, B>(nodes: NodeSlice<'t, L, A>, sep: &str, annotate: impl FnMut(&SplitAtCharsPart<'t, L, A>) -> B) -> Result<SplitAtChars<L, B>, ExtractError>` + `split_at_chars_drop_annotations(nodes, sep)` (`B = ()`) + `split_at_chars_keep_annotations(nodes, sep)` (`A: Clone + Default`, bound only there) |
| `extract::parse_keyval` / `split_embellishments` / `split_tack_on_fields` | same triples; general callbacks `impl FnMut(&KeyValsPart<'t, L, A>) -> B`; results `KeyVals<L, B>` |
| `extract::SplitAtChars<L, B = ()>` | renamed from `Split`; segment API over the `NodeTree<L, B>` backing tree (`segment`/`segments` return `NodeSlice<'_, L, B>`) |
| `extract::KeyVals<L, B = ()>` / `KeyValEntry<'k, L, B = ()>` | gain the annotation parameter; `get_combined_with -> Result<Option<NodeTree<L>>, ExtractError>` (annotation-free result, D-plan-6) |
| `extract::SplitAtCharsPart<'t, L, A = ()>` / `KeyValsPart<'t, L, A = ()>` | opaque part contexts: `original() -> Option<NodeRef<'t, L, A>>` (`None` exactly for synthesized wrappers/root), `is_partial()`, `partial_text() -> Option<&'t str>`, `segment_index()` / `entry_index() -> Option<usize>` (`None` for the root); `Debug` |
| `extract::content_as_chars` | input-generic: `<'t, L, A: 't>(nodes: impl IntoIterator<Item = NodeRef<'t, L, A>>)` |
| crate-internal | `copy_subtree_into<'t, L, AOld, B>(builder, node: NodeRef<'t, L, AOld>, annotate: &mut impl FnMut(NodeRef<'t, L, AOld>) -> B)`; `NodeTreeBuilder::restage_node_with_content_mapping` + `ContentParentMapping { Verbatim, Translate }` (D-plan-7; public `restage_node` unchanged) |

### Acceptance-test outcomes (§S7 acceptance)

- **Argument-swap round-trip**: `\a{1}{2}` → `\a{2}{1}` via the reentrant trait
  visitor (two `restage_argument` calls + one reordered `restage_invocation`);
  content/spec/name travel together, sibling spans out of source order,
  `validate_tree` green. PASS (`argument_swap_round_trip`).
- **Annotation-flow**: origin convention (identity restage minting
  `Origin { original }` per node), explicit op-argument annotations
  (`content_swap_annotations_flow_explicitly` pins visitor-Descend, helper,
  and caller-staged channels per node). PASS.
- **Extract triples on all four producers**: general/drop/keep exist and are
  exercised; part facts pinned (originals, partials + cut text, segment/entry
  indices, synthesized `None`s, keep-through defaults); input genericity by
  splitting annotated trees and composing producers. PASS.
- Driver policy tests: no-silent-repair (emptied region provided-with-empty;
  dropped content parent diagnosed with the takeover route), role-uniform
  descent (Content/Attached/Hidden fixture), Emit-no-descent, root-not-singular,
  visitor error transport, op-misuse Errs, duplication via `restage_subtree`,
  unwrap via `restage_children`, hand-built bundles, deep-wrapper content swap.
  PASS (21 transform tests total).

### Gate results (final full run)

- `cargo build` (and `--tests`): 0 warnings, 0 errors (workspace
  `missing_docs = "warn"` ⇒ zero missing docs).
- `cargo test`: **687 lib** (baseline 661 + 26) + 30 acceptance + 8
  derive-conditions + 1 derive + **30 doctests** (baseline 28 + the transform
  module example + the `split_at_chars` example; 2 ignored pre-existing) — all
  green.
- `rm -rf target/doc && cargo docs`: clean — no missing_docs, no broken
  intra-doc links.
- Superseded-names sweep: clean — no `Restage::Continue/Keep/Retain/Auto`, no
  `stage_argument_like`, no `add_subtree`, no `copied_from`, no
  `_with_annotations`, no `WithTransformedTreeNodeProvenance`, no
  `check_transform_tree_invariants`/`validate_parse_tree`, no bare `Split`
  result type. (The pre-existing crate-*internal* `copy_subtree_into` helper
  name stays: it is the extract copy machinery's internal spelling from S3,
  not public transform vocabulary — the ban targets "copy" as the public
  transform-op vocabulary.)
- Behavior changes only where ruled: the four extract bare names changed arity
  (the ruled general-form flip); everything else is additive.

### Commits

- 3ca0912 P3-S7: implementation plan
- 8b56c06 P3-S7 M2: transform core — Restage/RestageVisitor/RestageError + driver
- a2afc14 P3-S7 M3: region ops + bundles + argument-swap acceptance
- be45849 P3-S7 M4: content-swap helpers
- a5a62de P3-S7 M5: extract annotation minting — triples, part contexts,
  SplitAtChars rename, input genericity
- (this commit) P3-S7 M6: docs + records + closure

### Churn

6 stage commits; 16 files, +3506/−196 (transform module ~2270 lines incl.
~960 test lines; extract.rs +720/−196 spread; copy.rs refactor; records/docs:
DESIGN_RATIONALE, ARCHITECTURE, CLAUDE.md, lib.rs, learn-by-example.md,
S7_REPORT.md).
