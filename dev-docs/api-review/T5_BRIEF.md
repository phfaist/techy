# Phase 2b — T5 Decision-Session Brief (framework-builder persona)

Prepared 2026-07-31. Inputs: PLAN.md decision log (P1–P5 + T1/T2 + T3 + T4 rulings all
binding; the pending "NEXT: 2b T5 session" item is the charter), P4_RULING.md deferred
agenda (restage detailing), T3_RULINGS C+G (wish-20 commitment) + D (T5 scope split),
T4_RULINGS B/E/F + sweep (resolver datum, Copy/Eq drop, honest-slice notes,
recompose routing), T1T2_RULINGS E4 (pillar rider), POLICY_BRIEF §Routing (T5 line),
walkthroughs/framework/ (FRICTION.md, FRAMEWORK-ANALYSIS.md, API-SURFACE.md),
DESIGN_RATIONALE entries as cited. Every code claim re-verified against the working
tree at commit **4c324c7** (file:line cited; paths relative to `techy/src/`). Verified:
`git diff e5b994b..4c324c7 -- techy/` is **empty** — no crate source has changed since
the T3 brief's verification base, so all T3/T4-brief citations remain literally valid.
The brief recommends; all rulings are the user's.

**Probe re-runs (this brief's persona evidence).** The Phase-1b runnable probes
survive in the old scratchpad and were **copied and re-run against 4c324c7**
(`api-review-t5/probes/rerun/`): `boundary`, `reaccess`, `latex2text`,
`reconstruct`, `transform`, `flm_lang` all compile and pass with the walkthrough's
results (byte-faithful reconstruction, `RegionAlreadyResolved` on naive re-stage,
silently stale middle-node `source_text()`, span-equality id recovery); the
`flm_reuse` negative probe still fails with the same four monomorphism errors
(`LatexlikeDriver: ParseDriver<Latexlike>` only, `TokenRules<Latexlike>`,
`ScopeStack<Latexlike>`, `MacroSpec: CallableSpec<Latexlike>`) — the preset-fork
cliff is current code. The **projected** FLM code under the ruled shape is
`api-review-t5/probes/flm_projected.rs` (point C).

**Reading key — unapplied rulings.** The code is still pre-application for
P2/P3/P4/P5/E4 and the T1/T2, T3, T4 sessions (verified at 4c324c7: tier-2 ext types
and `finalize_node` still exist, state/lang.rs:39–63 + :345–354; `NodeTree` has no
annotation parameter and a debug-only tag, node/tree.rs:126–134; `NodeId` excludes
the tag from `Eq`/`Hash`, tree.rs:65–93; `finish()` computes then drops the parent
table, node/builder.rs:234 → :270–274; `ParserSession::builder` is `pub`,
engine/mod.rs:141; `copy_subtree_into` is `pub(crate)`, node/copy.rs:34; `ParsedSlot`
still has `new`/`named` with `Default`-filled ext, node/arguments.rs:336–345;
`SimpleLang` and `CommandResolution::resolve_via_scopes` still present, lang.rs:368,
engine/driver.rs:446; `LatexlikeDriver` is still `Copy + Eq` and Latexlike-monomorphic,
latexlike/driver.rs:63–70 + :92). Every "current code" citation is the pre-ruling
state, with ruled changes layered on explicitly. Constraints honored: one canonical
path + specs=author-side/hub=run-side (P1); fix-the-real-API (P2); role traits +
pillars + `LLL` (P3); the frozen [§dd-dr:transform] entries (P4); one stability class,
soft freeze (P5); shorthand-not-second-path (T1/T2); "techy error types uniformly
`Clone`, out-of-crate info behind `Arc`" (T4). Format per point: Context → Evidence →
Options → **Recommendation** → Cost.

**Stale-claim ledger (found while verifying; details in the points):**
1. [§dd-dr:latexlike-generalization]'s pillar list includes "the `finalize_node`
   spec-dispatch" — under P4 `finalize_node` is deleted and the preset's
   `make_node_ext` is the trivial `()` one-liner (preset `NodeExts` is `()` except
   `SlotExt`), so **no such pillar will exist**; the entry needs an amendment note at
   session close (harmless: nothing consumed it).
2. FRAMEWORK-ANALYSIS.md (cross-cutting, "Works" list) records "`Lang::finalize_node`
   runs on every re-staged node (idempotence is part of its contract — the hook is
   transform-aware by design)" — **reversed by P4**: restaged copies carry cloned
   exts verbatim, the mint runs once, the idempotence contract is deleted
   ([§dd-dr:ext-minting]). The walkthrough claim was true then and is counterfactual
   under the rulings; the sweep (I) must not re-import it.
3. The walkthrough deliverables and probes speak `MathStyle`/`math_style()` and the
   8-type `NodeExtTypes` bundle — superseded by P3 (`MathGroupForm`/`math_form()`)
   and P4 (3-member bundle). Expected pre-ruling staleness; noted so no name leaks
   back through the sweep.
4. P3's ruled role-trait set (`LatexlikeGroupType`/`LatexlikeCallableType`/
   `LatexlikeMode`, [§dd-dr:latexlike-generalization]) has **no Event member**, but
   the T1/T2-E4 preset wiring makes the `\text`-restore an *event* the LLL-generic
   argument factory must mint and the driver must recognize — a gap between two
   rulings, not code staleness (point C1; the FLM projection hits it directly).
5. SYNTHESIS wish 20's sketch `stage_callable(cx, &invocation, children, slots, end)`
   is superseded by T3's committed sketch `cx.stage_invocation(&invocation,
   arguments, slots, children, end_pos)` (name + the arguments parameter). Expected;
   noted so nobody quotes the old arity.
6. Old-scratchpad status: PLAN's "framework/ pending" note under the scratchpad
   heading is stale — `walkthroughs/framework/{probes/,techy-py/}` exist there and
   were reused (header above).

---

## A. Restage detailing (P4 deferred agenda — the big structural point)

**Context.** P4 ruled the architecture ([§dd-dr:restage]): `techy::transform`
module; visitor top-down over the frozen input, staging bottom-up into a
`NodeTreeBuilder<L, B>`; `Restage<B> { Continue(B), Emit(Vec<BuildId>) }` with
Continue-always-descends; region-aware ops returning opaque bundles; read
frozen / write staged; raw builder underneath; no `finish()` id map. T5 rules the
**exact types**: entry point, callback contract, error strategy, bundle shapes,
region-edit policies, the content-replacement helper, builder-`add` ergonomics,
annotation accessors, `annotate()` order, the `Continue` name, Split/KeyVals
rebasing, and the slot-role edge details. The op signatures are explicitly "not
frozen until then" ([§dd-dr:restage] revisit clause).

**Evidence — the substrate at 4c324c7.** The generalized arithmetic exists as
`copy.rs`: `copy_subtree_into` (pub(crate), node/copy.rs:34–42) recursion staging
bottom-up with an old-index→`BuildId` map (:40, needed for `InChildrenOf` content
parents), and `restage_region` translating one resolved region back to staging
coordinates (:87–111) — including the two designation reconstructions
(`InRegion` via offset re-basing :96–98, `InChildrenOf` via the ids map :100–108).
The builder contract it must satisfy: staged-only regions, exact tiling, claimed-once
children (builder.rs:146–197); errors are `NodeBuildError` (builder.rs:496–580) and a
failed `add` **poisons** the builder (builder.rs:87–89). `ParsedArgument` records are
`{spec, region: Option<ChildRegion>, ext}` (arguments.rs:232–242), `ParsedSlot`
`{name, region, ext}` (:325–334; P4 adds `role`). The environment shape proves node
spans may extend past the last child (scaffolding is rigid syntax, not nodes —
environment_parser.rs:774ff), which A1's span rule must honor.

### A1. Entry point, callback contract, error strategy

The recursion ops (`restage_subtree`, and A2's region ops, which all run the visitor
over interior nodes) must re-invoke the visitor from *inside* a visitor call. A bare
closure cannot name itself, so the callback is a **trait** with explicit
self-passing, plus a closure blanket for the common non-reentrant pass:

```rust
// techy::transform — names checked against [§dd-arch:naming] + [§dd-dr:superseded-names]
pub enum Restage<B> { Descend(B), Emit(Vec<BuildId>) }        // variant name → A7

pub trait RestageVisitor<L: Lang, A, B> {
    type Error;
    fn restage(
        &mut self,
        node: NodeRef<'_, L, A>,                               // frozen input (read side)
        cx: &mut RestageContext<'_, L, A, B>,                  // staged output (write side)
    ) -> Result<Restage<B>, Self::Error>;
}
// Blanket for closures: impl<F, …> RestageVisitor for F where
//   F: FnMut(NodeRef<…>, &mut RestageContext<…>) -> Result<Restage<B>, E>.

pub fn restage<L, A, B, V: RestageVisitor<L, A, B>>(
    tree: &NodeTree<L, A>,
    visitor: &mut V,
) -> Result<NodeTree<L, B>, RestageError<V::Error>>;           // whole-tree canonical entry

pub enum RestageError<E> {
    Build(NodeBuildError),         // builder contract violation → abandon (poisoned)
    ContentParentDropped { … },    // A3's one driver-detected repair-refusal
    Visitor(E),                    // the framework's own error, typed through
}
```

- **Recursion mechanics**: descending ops take the visitor again and a trait
  implementor passes `self` (`cx.restage_subtree(child, self)`) — a stored `&mut F`
  closure would self-reference during its own call; the blanket impl keeps
  `restage(&tree, &mut |node, cx| …)` for passes that never re-enter.
- **Error strategy — options.** (1) **Generic `V::Error` + `RestageError<E>`**
  (sketched): the framework's typed error passes through unboxed;
  `RestageError<E>: Clone where E: Clone` keeps T4's uniform-Clone principle
  conditionally. (2) Fixed `TransformError` with `Other(Arc<dyn Error + Send +
  Sync>)` — one vocabulary, no `E` inference wrinkle in the blanket, but boxes
  every framework error. (3) Infallible visitor — rejected (panic policy).
  **Recommend 1**; fall back to 2 if the blanket's `E` inference proves awkward at
  application (flag, don't re-session).
- **No `Send` bound anywhere on `V`** — an FFI framework drives the visitor with a
  `Py<PyAny>` callback on the caller's thread (H(b)); require only what the driver
  needs (`FnMut`-style `&mut`).
- **Driver internals**: the driver owns the ids map (the copy.rs:40 role) so
  `InChildrenOf` content parents resolve without user bookkeeping; `Emit` for a
  subtree simply leaves the map without entries for unvisited old nodes — reaching
  such a content parent is exactly A3's refusal error.
- **Level-0 primitive** (P4: "in `core::node`", the generalized copy.rs): a
  `NodeTreeBuilder` method so the splice route (G) works without the driver:
  ```rust
  impl<L: Lang, B> NodeTreeBuilder<L, B> {
      pub fn restage_node<A>(
          &mut self,
          node: NodeRef<'_, L, A>,               // may come from ANY tree (G depends on this)
          replacements: &[Vec<BuildId>],         // one entry per old child, in order
          content_parents: impl Fn(NodeId) -> Option<BuildId>,  // deep InChildrenOf anchors
          annotation: B,
      ) -> Result<BuildId, NodeBuildError>       // ext cloned from `node` (P4 point 3)
  }
  ```
  Positional `&[Vec<BuildId>]` over a map (no hashing; order explicit; length must
  equal `node.child_count()` — checked). Alternative: a small `ChildReplacements`
  builder type — more ceremony for the same information; **recommend the slices**,
  with the closure for the (rare) deep-content-parent case. Judgment call.

**Cost.** Two public types (`Restage`, `RestageError`), one trait, one context
type, one free fn, one builder method; the trait's `Error` assoc type is frozen
vocabulary.

### A2. Region bundles + the region ops

**Sketch (recommended).** Bundles are **opaque** (ruled: staged side write-only,
P4 point 6) but constructible, so callbacks can hand-build replacements:

```rust
pub struct RestagedArgument<L: Lang> { /* spec, provided?, nodes, content, ext — all private */ }
impl<L: Lang> RestagedArgument<L> {
    pub fn provided(spec: Arc<ArgumentSpec<L>>, nodes: Vec<BuildId>,
                    content: ContentNodes, ext: ArgumentExt<L>) -> Self;
    pub fn absent(spec: Arc<ArgumentSpec<L>>) -> Self;          // A3's explicit-absence door
}
pub struct RestagedSlot<L: Lang> { /* name, role, nodes, content, ext — private */ }
impl<L: Lang> RestagedSlot<L> {
    pub fn new(name: Option<…>, role: SlotRole, nodes: Vec<BuildId>,
               content: ContentNodes, ext: SlotExt<L>) -> Self;  // mirror T3 named-first at application
}

impl<L: Lang, A, B> RestageContext<'_, L, A, B> {
    pub fn restage_subtree (&mut self, node: NodeRef<'_, L, A>, v: &mut impl RestageVisitor<L, A, B>) -> Result<Vec<BuildId>, …>;
    pub fn restage_children(&mut self, node: NodeRef<'_, L, A>, v: &mut impl …) -> Result<Vec<BuildId>, …>;
    pub fn restage_argument(&mut self, node: NodeRef<'_, L, A>, index: usize, v: &mut impl …) -> Result<RestagedArgument<L>, …>;
    pub fn restage_argument_named(&mut self, node: …, name: &str, v: …) -> Result<RestagedArgument<L>, …>;  // unknown name = Err (T1/T2 A3 doctrine)
    pub fn restage_slot(&mut self, node: …, index: usize, v: …) -> Result<RestagedSlot<L>, …>;
    pub fn restage_invocation(&mut self, node: NodeRef<'_, L, A>,
        arguments: Vec<RestagedArgument<L>>, slots: Vec<RestagedSlot<L>>,
        annotation: B) -> Result<BuildId, …>;                   // retiles in the order given (ruled)
    pub fn builder(&mut self) -> &mut NodeTreeBuilder<L, B>;    // the power boundary (ruled)
}
```

- The region ops run the visitor over the region's nodes (a swapped argument whose
  interior the visitor also rewrites must be visited — P4's crate-owns-all-region-
  arithmetic point), hence the visitor parameter throughout.
- `restage_invocation` transcribes the frozen node's `callable_type`/`name`/`spec`/
  `post_space` (kind.rs:199–226) and clones its `NodeExt` (frozen parse fact, P4
  point 3/4), builds staged `ChildRegion`s by concatenating the bundles **in the
  order given**, and stages through the builder. Passing *fewer* bundles than the
  old record had is legal — records are self-describing (arguments.rs:9–12), the
  new record simply says what the new tree has.
- The `_named` variant returns `Err` on a name the spec never declared (the T1/T2
  A3 unknown-name-is-an-error doctrine transfers; `Ok` needs no `Option` here —
  restaging an *absent* argument yields `RestagedArgument::absent`, uniform).
- **Field vocabulary is shared with parse-side staging by construction**: bundles
  speak `Arc<ArgumentSpec>`, `ContentNodes`, `SlotRole`, exts — the same types
  `ParsedArgument`/`ParsedSlot` carry (arguments.rs:232–352); B consumes this.

**Cost.** Two bundle types with constructors; six context ops. The bundle privacy
is load-bearing (write-only ruling) — going transparent later is additive,
going opaque later is breaking, so opaque is the safe start.

### A3. Region-edit policies (drop empties a region / removes a content parent)

**Evidence.** Tiling admits zero-width regions (builder.rs:158–170: `start == end ==
next` passes), so "drop emptied the region" is representable and validator-clean.
The genuinely broken case: the dropped/emitted-away node was an `InChildrenOf`
**content parent** (the argument's group, whose children were the content —
arguments.rs:86–88) — the designation now dangles.

**Options.**
1. **No silent repair** (recommend): an emptied region restages as
   provided-with-empty-region (honest: the record still says "was provided"; true
   absence is the callback's explicit `RestagedArgument::absent`); a dangling
   content parent is a driver **error** (`RestageError::ContentParentDropped`),
   whose message says "take over the invocation: `restage_argument`/hand-built
   bundles + `restage_invocation`". Mirrors Continue-always-descends: no semantic
   decision is ever made by omission.
2. Auto-repairs: flip emptied-provided→absent, or re-anchor dangling content to
   `InRegion(0..0)`. Rejected: both silently change what the record *means*
   (absent ≠ empty — pylatexenc parity, arguments.rs:22–25; content designation is
   parser semantics, not geometry).
3. Forbid dropping region members outside `Emit`-takeover entirely — rejected:
   kills the one-line "drop every `\comment{…}`" pass that motivates the driver.

**Recommendation: 1.** **Cost.** One error variant; one doc paragraph on the two
policies.

### A4. The content-replacement helper (`stage_argument_like`)

The latexpp/FLM staple: swap an argument's **content**, keep its syntax (wrapper
group, delimiters, noise). Under 1's bundles this is a canned op:

```rust
pub fn restage_argument_with_content(
    &mut self, node: NodeRef<'_, L, A>, index: usize,
    content: Vec<BuildId>,                       // the replacement content nodes
) -> Result<RestagedArgument<L>, …>
```

restaging the region's non-content nodes as-is (noise verbatim; the content-parent
group re-staged around `content` via the level-0 primitive), re-anchoring the
designation. Naming: `restage_argument_with_content` (family-consistent with A2;
**recommended**) over P4's working `stage_argument_like` ("like" says nothing;
"stage" without `re` misses that the wrapper comes from the frozen node) — record
the working name as superseded at close. A slot twin
(`restage_slot_with_content`) is the same arithmetic; include it (environment-body
replacement is FLM's bread and butter). **Cost.** Two ops, no new types.

### A5. Builder-`add` ergonomics (params struct?)

Post-P4 `add(kind, span, parsing_state, children, ext, annotation)` — six
positional parameters (today's five-plus-default: builder.rs:107–131).
**Options.** (1) Keep positional — the order is teachable (identity, provenance,
context, structure, lang-data, consumer-data), the builder "demands ready values"
(P4 point 3) and each argument is load-bearing; a mis-ordered call fails to
type-check for all but `ext`/`annotation` pairs of identical type (rare in
practice: `()`-vs-real). (2) A `NodeParts { … }` params struct — named fields, but
one more forever-stable public type, and struct-update sugar would reintroduce
exactly the partial-initialization reading P4 killed. (3) Typestate/chained
`.with_ext()` — rejected: a forgotten link must be a compile error and would not
be. **Recommend 1** (a params struct is additive later if transform code
demonstrates confusion; the reverse is breaking — one-way-door asymmetry).
**Cost.** None now.

### A6. Annotation accessors + `annotate()` traversal order + the generic ripple

- **Accessors**: `NodeRef::annotation(&self) -> &A` (P4 reserved the name);
  `NodeTree::annotations(&self) -> &[A]` (storage-order slice — cheap, and the
  FFI bulk-export shape); no setter (trees frozen; re-annotate via `annotate`).
- **`annotate`**: `NodeTree<L, A>::annotate<B>(&self, f: impl FnMut(NodeRef<'_, L, A>) -> B)
  -> NodeTree<L, B>` — shares the core `Arc` and tag (ruled). **Order: storage
  order** (index order; the annotations `Vec` fills positionally, zero bookkeeping)
  — documented explicitly, since a *stateful* closure would otherwise assume
  document order. Alternative: document order (preorder) — friendlier for stateful
  closures but needs an order walk + write-by-index for no structural gain; a
  consumer wanting document-order state reads it off `descendants()` first.
  **Recommend storage order + one loud doc sentence.** (The callback takes
  `NodeRef`, so `f` reads the old annotation via `node.annotation()` — no separate
  `&A` parameter; one way to reach it.)
- **Application-scope datum (flag now, no ruling needed)**: `NodeRef`, `NodeSlice`,
  `Descendants`, `NodeSliceIter`, `StagedChildren`-adjacent read types and the
  extract helpers all gain the defaulted `A` parameter (`NodeRef<'t, L, A = ()>`),
  keeping every existing spelling compiling — the same defaulted-parameter trick
  that keeps `ParseResult` unchanged ([§dd-dr:node-annotations]).

**Cost.** Two accessors + one method; the storage-order sentence.

### A7. `Restage::Continue` final name

Candidates (P4 recorded): `Continue` (working), `Descend`, `Keep`, `Retain`,
`Auto`. The safety invariant is *Continue always descends* — the only accident P4
found worth engineering away is a shallow-keep reached unknowingly.
- `Keep`/`Retain`: actively harmful — they read as "keep this subtree as-is",
  i.e. exactly the shallow-keep misreading (children may still be rewritten below).
- `Auto`: vague (auto-what?).
- `Continue`: honest about traversal, silent about the node's fate; needs the doc
  sentence either way.
- **`Descend(B)`**: makes the invariant self-evident in the name — the variant
  *says* the visitor goes into the children; the node's restage-over-results is
  the documented driver default. Pairs cleanly with `Emit` (I staged it myself /
  you descend and restage it).
**Recommend `Descend`**; `Continue` acceptable as status quo; `Keep`/`Retain`/
`Auto` → superseded-names at close. **Cost.** One word; zero consumers yet.

### A8. Split/KeyVals on restage (recorded later option, routed here)

**Evidence.** `Split::into_tree`/`KeyVals::into_tree` (extract.rs:480, :766) build
owned trees through the pub(crate) copy machinery; extract trees are ruled
`A = ()` ([§dd-dr:node-annotations]). Two separable halves:
1. **Forced at application (no decision)**: the extract helpers must become
   generic over the *input* annotation (`NodeSlice<'_, L, A>` in, `A = ()` out) —
   otherwise annotated pipelines cannot call `split_at_chars` at all.
2. **The option**: annotation-*carrying* extract builders (map `A_in → A_out`
   through a caller closure, incl. the split boundary partials that P4 already
   re-mints via `make_node_ext`). **Recommend defer** — no consumer yet, the
   single-pathway rule would force a mapper parameter on every call (a tax on the
   99% `()`-out case), and an annotation-carrying variant is additive later as a
   sibling entry point. Record the deferral in the [§dd-dr:node-annotations]
   later-option note (turning "decide in 2b" into "deferred with trigger:
   first framework that needs annotated split output").
**Cost.** None now; the input-genericity rides the Phase 3 application.

### A9. Slot-role edge details (P4 point 8's 2b list)

| Question | Recommendation | Ground |
|---|---|---|
| `body()` also filter on `role == Content`? | **No** — body-ness is the ext axis alone; a hidden conjunction would make a framework's `Attached`-body choice silently unfindable. Doc sentence instead. | Orthogonal-axes ruling ([§dd-dr:slot-roles]) |
| Extract readers vs `Hidden` slots | **Role-blind everywhere except recompose** (already ruled there): explicit accessors (`slot_content_nodes`, node_ref.rs:327) address what the caller names; structural walks (`descendants()`, `display_tree`) stay structural — debug honesty. One doc note on `Hidden` semantics ("techy core ignores" = recompose/byte-accounting, **not** invisibility to reads). | `Hidden` contract wording |
| `SlotRole` `#[non_exhaustive]`? | **Exhaustive** — consumers (validators, recompose strategies, frameworks) match it constantly; a fourth role is a conscious breaking change. Mirrors the `MathGroupForm` exhaustiveness argument verbatim ([§dd-dr:math-group-form]). | Wildcard-arm tax |
| `Attached` vs `Derived` | **`Attached` — now locked in** by T4's shipped names `parse_attached_source` + `attach_source_reference` ([§dd-dr:input-wiring]): renaming the role would fork the vocabulary the door already teaches. Confirm, record `Derived` as considered-and-closed. | T4 B2 |
| Restage descends into `Attached`/`Hidden` slot children? | **Yes** — restage is structural (all children); a verbatim-latexpp pass that must not touch attached content `Emit`s the callable (or uses recompose, whose strategies are the role-aware layer). Doc sentence. | [§dd-dr:restage] vs [§dd-dr:recompose] split |

---

## B. `cx.stage_invocation` — the wish-20 signature (T3 commitment)

**Context.** T3 C+G granted the helper commitment-only: a `ParseContext` method
wrapping the one staging door (`cx.stage_node`, P4), sketch
`cx.stage_invocation(&invocation, arguments, slots, children, end_pos)`; the
signature was deferred here so parse-side and restage-side spellings share field
vocabulary and region semantics ([§dd-dr:takeover-staging-sugar]).

**Evidence.** The std parser is the exact model (invocation_parser.rs:163–205):
transcribe `callable_type`/`name`/`spec` from the `Invocation`
(constructs/mod.rs:512–523) and `post_space` from the trigger token (:191);
compute the span as `token.span.start()..end` where `end` = last staged child's
span end, falling back to the trigger's end for childless shapes (:178–182); stage;
map builder errors through `implementation_error` (:203). Two verified wrinkles:
(a) the **environment takeover overrides** `callable_type` and `name`
(environments.rs:564: `CallableType::Environment` + the environment's name, not
`begin`) — transcription does not fit it; (b) the environment node's span extends
past its last child (the `\end` scaffolding is rigid syntax, not nodes,
[§dd-dr:environment-scaffolding]) — so a computed-from-children end is *wrong* for
that shape.

**Sketch (recommended).**

```rust
impl<…> ParseContext<'_, '_, L> {
    /// Stage the resolved invocation's callable node: transcribes callable_type/name/
    /// spec/post_space, computes the span, mints the ext (via the stage_node door),
    /// stages, returns the id.
    pub fn stage_invocation(
        &mut self,
        invocation: &Invocation<'_, '_, L>,
        arguments: ParsedArguments<L>,      // staged-coordinate records (caller-tiled)
        slots: ParsedSlots<L>,
        children: Vec<BuildId>,
        end_pos: Option<usize>,             // None = std rule (last child's end, else trigger end)
    ) -> ConstructParserResult<L, BuildId>
}
```

**Options ruled inside the sketch.**
- **`end_pos: Option<usize>`** over always-explicit `usize` (re-imposes the
  boilerplate the helper removes on the 90% macro shape) and over omitting it
  (kills rest-of-line/heredoc takeovers whose consumed extent outruns the last
  child — T3's task-5 title parser is the persona). `None` = precisely the std
  rule (invocation_parser.rs:178–182), documented as such.
- **No `callable_type`/`name` overrides**: the environment-class composition keeps
  using `cx.stage_node` with an explicit `CallableData` — that door *is* the
  canonical path (P4), and the helper is a shorthand of the transcription case
  only (shorthand-not-second-path; adding override parameters would grow it into
  a second `CallableData` literal). In-crate: `StdInvocationParser` and
  argument_parsers.rs:358 collapse onto the helper; environment_parser.rs:774/:915
  and environments.rs:564 stay on the door.
- **Symmetry with `restage_invocation` (A2)** is by shared *vocabulary*, not shared
  arity: both speak `ParsedArguments`/`ParsedSlots`-family records, `ChildRegion`
  staging coordinates, `ContentNodes` designations; the deliberate asymmetry —
  parse side passes caller-tiled records + a flat child list (the natural output
  of the argument loop, invocation_parser.rs:130–143), restage side passes
  driver-tiled bundles — reflects who owns the region arithmetic on each side.
  State this in the durable record so nobody "fixes" it later.
- Ext/annotation: none in the signature — `stage_node` mints the ext (P4), parse
  annotations are `()` (ruled).

**Recommendation.** Adopt the sketch. **Cost.** One method; the `Option` end rule
needs its doc sentence.

---

## C. P3 acceptance — the FLM probe under the ruled generalization

**Context.** P3's acceptance criterion: re-run T5's FLM compile probe (custom
`Lang` with node exts reusing driver, spec types, token rules, base package). The
generalization is unapplied, so a literal re-run is impossible; per the charter,
the **projected** FLM code is written out as
`api-review-t5/probes/flm_projected.rs` (~230 lines, every construct annotated
with its ruling source; the binding compile check happens at Phase 3 acceptance).
The projection exercises: role-trait impl for an *extended* `GroupType` (with
`Math(MathGroupForm)` payload), zero-code adoption of preset `CallableType`/`Mode`,
the 3-member ext bundle with a `BodySlotExt`-implementing slot ext, required
`make_node_ext`, pillar-delegating custom driver (FLM's documented
`refine_diagnostic` posture) incl. the T4 resolver field and E4
`resolve_state_event`, `lang_initial_with_packages` + generic
`minilatex_package::<Flm>()`, and an annotated restage pass using A's proposed
signatures.

**Result: the ruled shape erases every compile error of the negative probe** (all
four monomorphism failures re-verified current at 4c324c7 — header). Residual
gaps the rulings do **not** cover, each a sub-point:

### C1. No Event role trait — the text-restore event is unmintable in a foreign vocabulary

**Evidence.** T1/T2 E4 wires `\text` restoration as an *event*: the preset
`ArgumentSpec` delta carries `.event(RestoreEnclosingTextContext)` and the driver
lowers it via `resolve_state_event` (T1T2_RULINGS E4; [§dd-dr:enclosing-state-stack],
[§dd-dr:argument-factory-additions]). Under P3, the factory that mints that spec
(`argument_specs`/the text-restore factory) is `LLL`-generic — but the event value
must be of **`LLL::Event`**, and the ruled role-trait set (`LatexlikeGroupType` /
`LatexlikeCallableType` / `LatexlikeMode`, [§dd-dr:latexlike-generalization]) has
no Event member: the preset cannot construct, and `LatexlikeDriver<LLL>` cannot
recognize, the event in FLM's own `Event` type. This is a fresh cliff of exactly
the P3 kind (ledger #4).

**Options.**
1. **Fourth role trait** — `LatexlikeEvent { fn exit_math_context() -> Self;
   fn is_exit_math_context(&self) -> bool; }` (constructor + recognizer, coherence
   contract mirroring `math_group(f).math_form() == Some(f)`), implemented by
   techy for the preset's event enum; bound appears on `LatexlikeLang`'s `Event`
   like the other three. Name follows the T3-amended concept (*exit math
   context*, never "restore text" — superseded-names discipline). Recommended.
2. Preset-side event wrapper (`LatexlikeEvent<E>` enum the host embeds) — forces
   every host event through a preset container; violates "vocabulary types stay
   the host's own" that the role traits exist to preserve.
3. Event-less re-design (delta carries the lowered patch directly) — impossible:
   the patch depends on the enclosing stack at *use* time; that context-dependence
   is the entire E4 design.
**Recommendation: 1**, recorded as an amendment on
[§dd-dr:latexlike-generalization] (role-trait roster) at close.
**Cost.** One trait + one bound; the evolution posture (defaulted-method additions)
carries over.

### C2. Driver-residue check (accepted, verify only)

An FLM customizing one hook writes ~12 delegation one-liners
([§dd-dr:preset-driver-pillars] accepts this consciously). The projection confirms
the pillar inventory covers every non-default hook body of the current driver
(latexlike/driver.rs:92–181 — resolution, math plug, paragraph break) plus E4's
event lowering; `recovery`/`probe_token`/`recover`/the three factories are trait
defaults a delegating driver never writes. Nothing to rule; the acceptance run at
Phase 3 should assert the residue stays ≤ the recorded ~30 lines (Lang) + ~12
lines (driver).

### C3. Remaining projection notes (flag, no session time unless challenged)

- **`initial_state_data` pillar**: the projection assumes
  `latexlike::initial_state_data::<LLL>()` (listed in the P3 pillar inventory).
  Its body composes `default_token_rules::<LLL>()` + `builtin_package::<LLL>()`
  push — both already ruled generic. No gap.
- **`ClosedVocabulary`**: FLM's extended `FlmGroupType` need not implement it
  (T3 E2 "provide, don't require" verified honest in the projection — nothing
  preset-side demands enumeration).
- **Std argument parsers**: usable because FLM's `ArgumentExt = ()` satisfies the
  bound-where-used `Default` ([§dd-dr:ext-minting]); an FLM with a real
  non-`Default` `ArgumentExt` writes custom parsers — accepted P4 design,
  restated in the framework guide chapter.
- **Ledger #1** lands here: no `finalize_node` pillar exists to delegate; the
  preset's `make_node_ext` is `()` and FLM's is its own (nothing to compose).

---

## D. Driver knobs / extension seam (`LatexlikeDriver<LLL>` post-rulings)

**Context.** T3 D ruled the layered shape; T4 added the resolver field and dropped
`Copy`/`Eq`; T5 asks: is the knob surface coherent, is anything missing for
FLM/latexpp-class drivers?

**Evidence.** Current fields: `recovery` + `paragraph_break_style`, both `pub`
(latexlike/driver.rs:64–70), constructor + one `with_` builder (:75–83). Post-
rulings the struct is `LatexlikeDriver<LLL> { recovery, paragraph_break_style,
resolver: Option<Arc<dyn SourceResolver<LLL::SourceOrigin>>>, PhantomData<LLL> }`,
`Clone + Debug` ([§dd-dr:input-wiring]; [§dd-dr:preset-driver-pillars] amendment),
implementing six of the (post-E4) thirteen hooks: `recovery`, `resolve_command`,
`make_paragraph_break_node`, `group_interior_delta`, `resolve_state_event`,
`source_resolver`. The in-code comment justifying per-break spec minting by the
driver's "`Copy`/`Eq` config-value nature" (driver.rs:121–123) is mooted by the
T4 drop — keep the minting (still right: specs are behavior, never compared),
rewrite the comment at application.

**Assessment (recommend: coherent, add nothing).**
- The three knobs are orthogonal config values (policy, emission shape, capability);
  every other behavior difference is a *different driver* over pillars — the ruled
  seam. FLM (custom `refine_diagnostic`), latex2text-class (wholesale driver +
  resolver), latexpp-class (tolerant + resolver) all fit without new surface.
- **One candidate knob examined and rejected**: a `with_group_interior_delta(hook)`
  closure knob for frameworks adding a group class with its own descent delta
  (FLM's fenced block). Rejected — it re-grows a behavior-carrying driver (loses
  the config-value nature *semantically*, not just `Copy`), and duplicates the
  ruled answer (write your own driver; the math pillar composes:
  `math_group_interior_delta(base, rule).or_else(|| my_fenced_delta(…))`). This is
  the canned driver's explicit boundary — one doc sentence at the struct.
- Field visibility: keep `recovery`/`paragraph_break_style` `pub`; the resolver
  field stays private behind `with_resolver`/`source_resolver()` (an `Option<Arc
  <dyn …>>` pub field invites identity comparison and partial moves for zero
  benefit). Judgment call, flag only.
**Cost.** None; two comment/doc edits ride the application.

---

## E. Pillar-signature sufficiency for post-parse state synthesis (E4 tie-in)

**Context.** T1/T2 E4's rider: the preset's event logic ships as public pillar
functions so post-parse processing can synthesize coherent recorded states for
constructed nodes ([§dd-dr:enclosing-state-stack]); T3 renamed/re-specified the
restore pillar as `exit_math_context_delta` (first non-math enclosing group in the
stack). T5 verifies the ruled signatures actually serve a transform pass.

**Evidence + the one signature that decides it.**
- **Math entry** — `math_group_interior_delta::<LLL>(base, rule)` (T3 D): a
  synthesizer holds `base` (the parent's recorded state, node_ref.rs:82) and can
  mint the `rule` (`GroupRule { group_type, open, close }` — all pub, constructible
  from the group node's recorded class + delimiters, kind.rs:143–157). Sufficient —
  with one **documented-recipe gap**: the full interior state at parse time is the
  pillar's delta *plus the engine's descent invariant* (`expecting_group_close`
  installed by the descent, driver.rs:139–141 doc; session.group_interior_state,
  engine/mod.rs:308). A synthesizer reproducing recorded-state equality must apply
  both. Options: (i) document the two-component recipe on the pillar (recommend —
  the delta split is real: one half is preset policy, the other core structure);
  (ii) a composed `…interior_state(base, rule)` helper — a second spelling of a
  two-line composition, and wrong for languages overriding the plug. **Recommend
  (i)**; the recipe lands in the pillar's rustdoc + the framework chapter.
- **Exit math** — the decisive signature. At parse time the context is the
  session's enclosing-state stack (`resolve_state_event(&event, &StateStackView)`);
  **post-parse there is no session** — the enclosing context is the *tree*:
  `iter::successors(node.parent(), |n| n.parent()).map(|n| n.parsing_state())`
  (T4's ancestors recipe + the P4 parent table). If the pillar's parameter is the
  session-coupled `StateStackView`, synthesis is locked out — the exact failure
  the rider exists to prevent. **Options.**
  1. **Iterator parameter** (recommend):
     `exit_math_context_delta::<LLL>(states: impl Iterator<Item = &Arc<ParsingState<LLL>>>)
     -> ParsingStateDelta<LLL>` — innermost-first, current state first; the ruled
     semantics (first non-math state's whole `TokenRules` + its mode; outermost
     as fallback) read naturally off any source: the driver hook passes the stack
     view's iterator, a transform passes the ancestors walk. (`-> Delta`, not
     `Option`: the fallback-to-outermost rule means an empty iterator is the only
     degenerate case — take `Option` only if the session decides an empty stack is
     representable at the hook.)
  2. `&StateStackView` parameter + a public constructor from a state slice —
     synthesis then builds a throwaway view; workable but teaches a fake session
     object and freezes the view type into the pillar for no gain.
  **Recommend 1**; `StateStackView` then stays what E4 made it — the *hook*'s
  window, offering `.states()` to feed pillar calls (flm_projected.rs shows the
  wiring).
- **Paragraph break** — `make_paragraph_break_node(style, state, token)` takes a
  `Token`; synthesis neither needs it (a transform stages `NodeKind::chars`
  directly) nor should mint tokens. Out of synthesis scope — no change; one doc
  sentence that this pillar is parse-side only.

**Recommendation.** Rule the iterator signature (option 1) + the two-component
math recipe as doc obligation; record both on [§dd-dr:preset-driver-pillars] /
[§dd-dr:enclosing-state-stack] at close. **Cost.** None beyond the signature
choice — this is the last moment before E4 application freezes it.

---

## F. Honest slices + the transform-tier validator (application details)

**Context.** P4 ruled honest slices (per-run source-uniformity verification with a
`finish()` single-source fast-path flag) and a transform-tier validator
"structure + region tiling + `TextContent` residency, minus parse-law byte
accounting" ([§dd-dr:tree-navigation], [§dd-dr:restage]); T4 recorded application
notes. T5 fixes: what exactly it checks, where it lives, signature/error type.

**Evidence.** `NodeSlice::span()` checks first/last source + span order
(slice.rs:105–112); `source_text()` checks first/last source only (:117–124) — the
probe re-confirmed the stale-middle-bytes lie at 4c324c7 (header). The parse-law
checker `check_tree_invariants` **panics** (test utility by declaration,
invariants.rs:1–5, :17–19) and asserts children-share-parent's-source
(:120–128) — which will reject legitimate `\input` **parse** trees once T4's door
lands.

**The pieces (recommendation per row).**
1. **Honest slices**: `span()`/`source_text()` scan the whole run for one source
   unless the tree's single-source flag is set (flag computed in `finish()` pass 2
   — Arc-identity comparison per node, stored on the core). `source_text()` also
   gains `span()`'s ordering guard (:108 has it, :117–124 does not — harmless
   today via `str::get`'s reversed-range `None`, but make the two accessors'
   contracts read identically). No API change, only contract text: `None` gains
   the "mixed-source run" meaning it already claims (slice.rs:102–104).
2. **The validator**: a **`Result`-returning check, not a panic** — its persona is
   a *framework* validating spliced trees at runtime (FFI boundary included),
   where a panic is the wrong tool (panic policy: library code returns `Err` on
   outer-layer bugs); `check_tree_invariants` keeps its panicking test-utility
   role unchanged.
   ```rust
   // techy::transform
   pub fn validate_tree<L: Lang, A>(tree: &NodeTree<L, A>) -> Result<(), TreeViolation>
   #[non_exhaustive] pub struct TreeViolation { pub node: Option<NodeId>, pub kind: TreeViolationKind, /* detail */ }
   ```
   **Checks** (the all-trees law): structural sanity (children ranges in-bounds,
   after-parent, single-parent, reachable — invariants.rs:53–91's list);
   region tiling incl. content ranges within content parents and
   content-parent-inside-region (:38–41's clauses, on *resolved* records);
   `TextContent` residency (valid char-boundary range of the node's own source)
   **without** the pinned-position clauses; region records resolved (a staged
   region in a finished tree is impossible by construction, but the validator says
   so rather than panicking on `ChildRegion::children()`). **Not checked**: byte
   partition/contiguity, children-in-parent's-source, span source-order — the
   parse law.
3. **Home**: `techy::transform` (the tier that mints mixed-origin trees; its
   audience) — over `core::node` beside the parse-law checker (both defensible;
   the P1 rule "placement by what it is *for*" favors transform: it exists for
   transform outputs). Judgment call.
4. **Name**: `validate_tree` (verb differs from the panicking `check_*` family —
   deliberate, the contract differs) vs `check_transform_tree_invariants`
   (walkthrough's wish-name; long, and "transform tree" under-claims — it accepts
   parse trees too). **Recommend `validate_tree`.**
5. **Parse-law checker under `\input`** (flag for the T4/P4 application, not a T5
   ruling): `check_tree_invariants` must scope its byte accounting per source via
   the `Attached` role (P4 point 9) or every `\input` acceptance test fails; that
   change rides the input-wiring application, and the two checkers' doc pages
   cross-reference ("all-trees law" ⊂ "parse-tree law").

**Cost.** One fn + one error type (+ its `Kind` enum); the honest-slice scans are
O(len) with the O(1) flag fast path (accepted in P4).

---

## G. `\input` splice-a-cached-parse affordance

**Context.** [§dd-dr:input-attachment] keeps separate-parse-then-restage-splice
"possible via the transform primitives for frameworks that want caching, with the
state-correctness caveat on their heads"; [§dd-dr:input-wiring]'s revisit clause
routes the affordance question here. Framework case: latexpp over a multi-file
project, parsing each file once.

**Options.**
1. **Not now — keep it a recipe** (recommend): the level-0 primitive (A1) is
   deliberately **cross-tree** (`restage_node` takes a `NodeRef` from *any* tree —
   the signature note exists for exactly this), so a caching framework splices a
   cached tree's root into its builder in a few lines; the include chapter
   (Phase 4) shows it beside the state-correctness caveat (cached content parsed
   under the cached file's seed, not the `\input`-point state — the framework
   asserts state-independence or re-parses). No new API, nothing to freeze.
2. A parse-time door `cx.attach_cached_tree(&NodeTree<L>, …)` splicing during the
   parse — rejected for now: it blesses the state-*incorrect* shape with a
   first-class spelling (the same-builder sub-parse exists precisely because state
   at the `\input` point is semantically forced), and it would be the only
   parse-path API whose input is a finished tree (layer inversion). A framework
   wanting it composes option 1 inside a custom `\input` spec — the T4 B4 ruling
   already made `\input` specs easy custom work.
3. A transform-side canned op (`restage_attached(node, cached_tree)`) — premature:
   no consumer yet; additive later over the same primitive.
**Recommendation: 1**, with two riders: (a) A1's primitive must not debug-assert
same-tree provenance on its `NodeRef` input (it is the sanctioned cross-tree
door); (b) note in the record that latexpp's *verbatim* output path never needs
the splice at all — recompose emits `\input{file}` per source ([§dd-dr:recompose]),
so per-file processing composes without any tree merging. **Cost.** None; one
guide recipe obligation.

---

## H. Transformation-infra scope + FFI needs + reconstruction guarantees

### H(a). Module vs crate — nothing left to decide

P4 ruled the in-crate module (companion crate rejected, [§dd-dr:restage]) and the
topology entry already names `techy::transform` + `techy::recompose` top-level
([§dd-dr:public-namespace-topology] P4 amendment); the level-0 primitive's home is
`core::node` (P4 point 6). The POLICY_BRIEF T5-routing line ("module vs crate")
predates P4 — confirm-only row. The one open placement inside the scope is F's
validator home (ruled in F).

### H(b). FFI-driven API needs — verdicts re-verified, plus ruled-change deltas

**Re-verified at 4c324c7** (probe re-runs, header): every ownable type
`'static + Send + Sync`; `Arc<NodeTree>` + `NodeId` → `get(id)` round-trip; the
`NodeRef` negative probe still fails as designed. **Under the ruled changes:**
- `NodeTree<L, A>`: bindings fix one concrete `A` (a `Py<PyAny>`-carrying type
  satisfies `Clone + Debug + Send + Sync`); `annotations()` (A6) is the bulk-export
  shape; handles stay `(Arc<NodeTree<L, A>>, NodeId)` — annotation stages *share
  tags*, so ids stay valid across `annotate()` stages by construction
  ([§dd-dr:tree-tags]).
- Tree tags in `Eq`/`Hash`: the walkthrough's release-build caveat
  (FRICTION Part 1, "may silently resolve in release builds" — verified current at
  tree.rs:158–166) **disappears**: `get()` genuinely rejects foreign ids
  everywhere. Binding docs simplify.
- Parent tables kill the per-binding `HashMap<usize, NodeId>` rebuild (FRICTION
  "Gap: no parent link" — closed by P4/T4).
- **New requirement surfaced by this brief (rule it here)**: the restage driver and
  `annotate()` must put **no `Send`/`Sync` bounds on visitor/closure parameters**
  (A1) — a Python-callback-driven transform runs on the caller's thread; a
  gratuitous `Send` would wall off the primary T5 consumer. One sentence in the
  durable record; zero cost in Rust.
- `LineIndex<'c>` stays borrow-bound (fine — compute-on-demand was the walkthrough
  pattern); T4's owned `LineIndexCache` is the persistent-handle answer for
  bindings ([§dd-dr:line-col-ownership]) — note for the binding guide.

### H(c). Reconstruction guarantees — promote the parse-law to a documented contract

**Evidence.** The latexpp verdict rests on an *emergent* property: the byte
partition + tolerant-recovery-stages-consumed-bytes behavior, verified by probe
(reconstruct re-run, header) and gated in-crate by the acceptance suite
([§dd-dr:language-parse-api] second follow-up: "invariants clean on every
acceptance parse" — including the stray-close skip staging its delimiter as
`Chars`). But no *documented promise* exists: the walkthrough called the
gap-filling walk "an invariant assumption the framework re-checks"
(FRAMEWORK-ANALYSIS C), and `check_tree_invariants`' own docs call it "a test
utility, deliberately not builder law" (invariants.rs:1–5).

**Options.**
1. **Document the parse-tree law as a stability item** (recommend): one narrative
   paragraph (NodeTree docs + the reconstruction guide chapter): *every successful
   parse — tolerant recovery included — produces a tree satisfying the parse-tree
   law (the `check_tree_invariants` list, per source under `\input`); byte-exact
   reconstruction by span gap-filling is supported behavior, and regressions are
   semver events under P5's rubric*. This is what latexpp builds on; P5's soft
   freeze covers behavior contracts exactly like names.
2. Leave emergent until recompose ships (the span-verbatim strategy would *embody*
   the promise) — rejected: frameworks build before recompose lands, and the
   promise is about parse output, not about the recompose module.
3. Guarantee more (e.g. `materialize()`+node-data reconstruction) — already
   recorded as the recompose node-data strategy's charter ([§dd-dr:recompose]:
   node data reconstructs everything except trigger spellings, preset provides
   those); nothing further to promise here.
**Recommendation: 1**; the sentence lands with Phase 4, the commitment is recorded
now (amendment note on [§dd-dr:span-invariants] or the stability entry).
**Cost.** Doc obligation only.

---

## I. Framework walkthrough sweep (FRICTION boundary table + FRAMEWORK-ANALYSIS top-5 + API-SURFACE wishes)

Classification: **[R‑x]** resolved by ruling x / **[T5‑y]** on this session's
agenda as point y / **[open]** genuinely open, disposed here / **[rej]** rejected
with reason. Nothing falls through silently.

| # | Item (source) | Class |
|---|---|---|
| 1 | Public subtree-copy / rebuild-visitor (top-5 #1, blocker) | [R‑P4] restage; exact types **[T5‑A]** |
| 2 | Preset generic across Langs (top-5 #2, blocker) | [R‑P3/T3]; acceptance **[T5‑C]** (one residual: C1 event role trait) |
| 3 | `finish()` BuildId→NodeId correspondence (top-5 #3) | [R‑P4] **rejected mechanism**, replaced by origin-by-convention annotations — the probe's span-equality hack becomes `Ann { origin: node.id() }` (verified strictly more direct); not an open gap |
| 4 | Parent navigation + `index_in_parent` (top-5 #4; FFI gap) | [R‑P4+T4] (`parent()`/`index_in_parent()`/`node_at`/`covering_slice` named) |
| 5 | Recomposition helper (top-5 #5) | [R‑P4] `techy::recompose`; design in the **recompose session** (boundary notes below) |
| 6 | Transform validator + honest slices (top-5 #6) | **[T5‑F]** |
| 7 | Stable kind-name strings (top-5 #7) | [R‑T1/T2 E5] `NodeKind::as_str()` |
| 8 | `NodeRef::tree()` public (top-5 #8) | [R‑P4 point 6] |
| 9 | Binding-oriented doc page (top-5 #9) | [open→Phase 4] framework chapter (PLAN Phase 4 names bindings guidance); contents checklist: Arc+NodeId handle pattern, `Send`-free visitor note (H(b)), synthesized-node recipe (E's pillar fns + P4 recipes), severity exhaustiveness, `LineIndexCache` handle |
| 10 | `post_space` re-emission gotcha (A archetype; POLICY_BRIEF T5 routing names it) | [open→Phase 4 + techy-totext] **doc-only**: the model is right (the token's own post-space, kind.rs:213–222 contract) — the central re-emission *policy* is a renderer concern; guide paragraph in the framework/latex2text chapter, default policy shipped in techy-totext. No techy API. |
| 11 | Standard macro database for latex2text (A archetype) | [R‑P2] rejected for techy (minidefs + frameworks/techy-totext own content) |
| 12 | Side tables die at transform boundaries (B archetype) | [R‑P4] annotations |
| 13 | `ArgumentExt` unreachable on the preset (B archetype) | [R‑P3+P4 jointly]: an own-`Lang` FLM now reaches the full ext bundle without forfeiting the preset; a *preset-adopting* consumer still has `()` exts **by design** (the ext budget belongs to the framework's `Lang`, [§dd-dr:latexlike-generalization]) — record as answered-not-granted |
| 14 | Mixed-origin trees: builder accepts / checker rejects (transform probe) | **[T5‑F]** (validator) + parse-law scoping rider (F5) |
| 15 | Stale `source_text()` on spliced trees (transform probe, re-verified) | [R‑P4] honest slices; details **[T5‑F1]** |
| 16 | Cross-tree id debug-only detection (FRICTION re-access) | [R‑P4] always-on tags; FFI delta **[T5‑H(b)]** |
| 17 | `Diagnostics::into_vec()` (API-SURFACE minor wish) | **[open→Tier‑C batch, lean reject]**: `iter().cloned().collect()` over a `len`-known iterator; the walkthrough itself called per-element clone "fine" (FRICTION Part 1); `sorted_by_position()` (T1/T2 E6) covers the ordered-extraction case. Route to the batch so the disposal is recorded, not silent. |
| 18 | Multi-source reconstruction untested edge (C archetype) | [R‑P4/T4] multi-source parse trees first-class + per-source verbatim recompose; the *test* obligation rides Phase 3 acceptance (\input wiring tests) — flag to the application checklist |
| 19 | `EnvironmentBehavior` open trait praise / no change wanted (B) | no action (evidence row) |
| 20 | Build friction rows (CARGO_HOME, macOS linking — environmental) | no action (recorded for the binding guide's appendix) |

**Boundary notes with the recompose session (flag, don't absorb):** (i) the
read-only walker (enter/exit, depth, `VisitFlow`) is recompose's — A1's
`RestageVisitor` deliberately claims only the `Restage`/`restage` vocabulary, so
the walker's names stay free; (ii) targeted-replacement integration (latexpp's
patch shape) is a recompose open Q — T5's A4 content-replacement ops must not be
read as covering it (they rebuild trees; recompose replaces *output spans*);
(iii) the verbatim `Attached`-exclusion rule stays recompose-owned (A9's
restage-descends-into-Attached is the transform-side complement, not a revision).

---

## Resolved by prior rulings — do not re-litigate

- **Transform as in-crate module; recompose module; top-level namespace slots** —
  P4 ([§dd-dr:restage], [§dd-dr:recompose], [§dd-dr:public-namespace-topology]).
- **`finish()` id map + auto-provenance trait** — P4 rejected both;
  origin-by-convention via annotations.
- **Continue-always-descends; read-frozen/write-staged; bundles opaque; raw
  builder as power boundary** — P4 (A only names/details them).
- **`NodeTree<L, A = ()>` storage, bounds, zero-copy `annotate`; always-on
  `TreeTag` in `Eq`/`Hash`; ext-minting principle (`make_node_ext`, tier-2
  removal, `ParserSession::builder` → pub(crate))** — P4.
- **`SlotRole` enum + `Attached` byte-tiling exclusion + `BodySlotExt`** — P4
  (A9 rules only the listed edges).
- **Preset generalization shape (role traits, `LatexlikeLang`, pillars,
  `LatexlikeDriver<LLL>`, `LLL`)** — P3 + T3 D/E; C rules only residuals.
- **Resolver on the driver (`source_resolver()`), the `parse_attached_source`
  door, `attach_source_reference`, the two source conditions, recursion-stays-
  embedder-policy (+ `check_include_chain`)** — P4 + T4 B ([§dd-dr:input-wiring],
  [§dd-dr:include-chain-helpers]).
- **Driver `Copy`/`Eq` dropped** — T4 (D consumes it as a datum).
- **Navigation names (`node_at`, `covering_slice`, `parent()`,
  `index_in_parent()`, `SourcePos`, `ancestors()` rejected)** — P4 + T4 E/D.
- **`stage_invocation` existence** (commitment) — T3 C+G; B rules the signature
  only.
- **Recompose fold architecture, state threading, output sink, targeted
  replacements, read-only walker, verbatim `Attached` exclusion** — the LATER
  recompose session (boundary notes in I).
- **One stability class + soft freeze for everything accepted here** — P5.

## Session logistics (proposed order, hard structural first)

Interim rulings file `T5_RULINGS.md`, updated every round (T1/T2/T3/T4 pattern).

1. **A** — restage detailing: A1 entry/callback/error (largest single decision),
   A2 bundles + ops, A3 region-edit policy, A4 content-replacement helper, A5
   builder-`add`, A6 annotation accessors + order, A7 variant name, A8
   Split/KeyVals, A9 slot-role edges. Output feeds B and G.
2. **B** — `stage_invocation` signature (quick once A2 fixed the shared
   vocabulary; one genuine choice, the `Option<usize>` end rule).
3. **C** — FLM projection acceptance: C1 event role trait (the one structural
   residual), C2/C3 confirmations. Feeds E.
4. **E** — pillar-signature sufficiency: the `exit_math_context_delta` iterator
   parameter (last cheap moment before E4 application), the math-descent recipe.
5. **D** — driver knobs (quick; recommend add-nothing).
6. **F** — honest slices + `validate_tree` (signature, checks, home, name).
7. **G** — splice-a-cached-parse (quick; recommend not-now + the cross-tree
   primitive rider).
8. **H** — scope confirmation, FFI deltas (rule the no-`Send` requirement),
   reconstruction-guarantee commitment.
9. **I** — walkthrough sweep confirmation (rows 9/10/17/18 need explicit nods;
   rest are citations).
10. **Sweep** — resolved-by-prior confirmation; durable-records list
    (DESIGN_RATIONALE: new entry for the restage op surface (or a
    [§dd-dr:restage] completion amendment) + amendments on
    [§dd-dr:latexlike-generalization] (event role trait, ledger #1),
    [§dd-dr:preset-driver-pillars] / [§dd-dr:enclosing-state-stack] (E
    signatures), [§dd-dr:slot-roles] (A9 edges), [§dd-dr:node-annotations]
    (A6/A8), [§dd-dr:takeover-staging-sugar] (B signature),
    [§dd-dr:tree-navigation] (F honest-slice contract),
    [§dd-dr:input-wiring]/[§dd-dr:input-attachment] (G closure),
    [§dd-dr:span-invariants] or [§dd-dr:stability-rubric] (H(c) guarantee);
    superseded-names additions: `stage_argument_like`, rejected `Restage` variant
    alternates, `check_transform_tree_invariants`-as-name if F adopts
    `validate_tree`); PLAN.md updates (T5 line → done; recompose-session boundary
    notes carried over; Tier-C rider: `Diagnostics::into_vec` lean-reject).

**Belongs to other sessions (flag, don't rule here):** the recompose items in I's
boundary notes; Tier-C batch rows (I‑17; plus the T4 handoffs already listed in
PLAN); Phase 3 application checklist items flagged in C2, F5, I‑18; Phase 4 guide
obligations (I‑9, I‑10, G's recipe, H(c)'s paragraph).
