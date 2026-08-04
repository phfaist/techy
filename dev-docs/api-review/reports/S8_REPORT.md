# Phase 3 — S8 stage report: visit + recompose + oracle suite

Branch `phase3-s8-visit-recompose` off api-review bc25770 (= 6e4516c "S7 signed
off + merged" + the S8 stage-log commit; setup note below). Implementer worktree:
`/Users/philippe/projects/techy/.claude/worktrees/agent-a26231eb892efd5bd`.

Setup note: the brief said the api-review tip is 6e4516c; the actual tip is
bc25770, whose sole diff is the S8-launched stage-log entry in PHASE3_PLAN.md
(committed after the brief was written). The stage branch bases on bc25770 so the
process log rides along; not a deviation.

Ruling inputs: PHASE3_PLAN § Protocol + § S8; RECOMPOSE_RULINGS.md (Rounds 1, 2,
A–D — the central frozen record); DESIGN_RATIONALE [§dd-dr:recompose-machinery]
(central), [§dd-dr:visit-engine] (central), [§dd-dr:recompose] (+ amendments),
[§dd-dr:invocation-syntax] (S5-M6-revised substrate), [§dd-dr:restage-ops] (incl.
the recompose-session mirror amendment), [§dd-dr:slot-roles],
[§dd-dr:superseded-names]; T5_RULINGS §H (per-node doctrine) + §F (slice
contracts); reports/S5_REPORT.md (shipped invocation-syntax surface + the
malformed-terminator S8 flag), reports/S6_REPORT.md (multi-source surface),
reports/S7_REPORT.md (the shipped `RestageError`/`RestageContext` mirror anchor).

Baseline (must not regress): 689 lib + 30 acceptance + 8 derive-conditions +
1 derive + 30 doctests (2 ignored pre-existing); `cargo build` 0 warnings;
`cargo docs` clean.

## Progress

- [x] M1 — this plan (committed before any code work)
- [ ] M2 — `techy::visit`: walk + NodeVisitor + VisitFlow + VisitContext +
      scoped-children kernel + tests
- [ ] M3 — `techy::recompose` core: ComposePiece, Recompose/ConcatPieces,
      Recomposer, RecomposeError, driver + instruction lowering,
      core_source_instruction + machinery tests
- [ ] M4 — RecomposeContext region ops + wrapping-contract + streaming tests
- [ ] M5 — preset SourceRecomposer + source_recomposer() + preset tests
- [ ] M6 — oracle acceptance suite (strict + tolerant + multi-source matrices)
- [ ] M7 — records + docs + gates + closure

## Design synthesis (records → code)

### A. `techy::visit` — the shared traversal engine ([§dd-dr:visit-engine])

New top-level public module, `techy/src/visit.rs` (`pub mod visit;` in lib.rs —
the extract pattern; internal layout invisible).

- **`VisitFlow { Descend, SkipChildren, Stop }`** — plain exhaustive enum
  (`Clone, Copy, Debug, PartialEq, Eq`).
- **`NodeVisitor<L: Lang, A>`**:
  `fn enter(&mut self, node: NodeRef<'_, L, A>, cx: &VisitContext<'_, L, A>)
  -> VisitFlow` + defaulted no-op
  `fn exit(&mut self, node: NodeRef<'_, L, A>, cx: &VisitContext<'_, L, A>)`.
  The walk is **infallible**: the read-only engine has no failable ops and the
  ruled sketch (`enter(node, depth) -> VisitFlow`) shows no `Result`; a visitor
  with error conditions carries them in its own `&mut self` fields and returns
  `Stop` (D-plan-2). No `Send`/`Sync` bounds (the [§dd-dr:restage-ops]
  argument transfers).
- Blanket impl for enter-only closures:
  `impl<L, A, F: FnMut(NodeRef<'_, L, A>, &VisitContext<'_, L, A>) -> VisitFlow>
  NodeVisitor<L, A> for F` (the S7 closure-annotation inference note carries
  over into the rustdoc).
- **`VisitContext<'t, L, A>`** — engine bookkeeping ONLY: `depth()` (0 at the
  walk's start node) and `tree()` (the walked tree). NO user state — the
  three-channel discipline documented on the module: run-spanning state = the
  visitor's/recomposer's `&mut self`; fold accumulation = driver locals + call
  stack; downward context = the argument-threaded `S`; a walk needing scoped
  state IS a `Recomposer` with `Piece = ()`.
- **Entry `visit::walk(node: NodeRef<'_, L, A>, visitor: &mut V)`** — the free
  fn (the vetoed `NodeRef::walk` was a *placement* veto; the free fn taking the
  start `NodeRef` is its substitute and gives subtree walks — the T4 walker
  origin's "walk everything under here with structure"; whole tree =
  `walk(tree.root(), v)`; D-plan-1). Preorder; `exit` fires after a node's
  (possibly skipped) children for `Descend`/`SkipChildren`; `Stop` aborts the
  whole walk immediately (no further enters or exits; D-plan-3).
- **Walk is role-blind** — `Attached` and `Hidden` slot children are visited
  like any others (debug honesty), in deliberate, documented contrast to
  `Concat`'s content-scoped default (the ruled read/compose asymmetry).
- **The one descent kernel** (crate-internal, hosted here):
  `pub(crate) fn scoped_children<'t, L, A>(node, include_attached: bool,
  include_hidden: bool) -> impl Iterator<Item = NodeRef<'t, L, A>>` — for a
  callable, children whose global index lies in an excluded-role slot region are
  skipped; the walk calls it with `(true, true)` (role-blind), the recompose
  driver with the `ConcatPieces` scope flags — both traversals are clients of
  the same kernel, which is what makes uniform lowering possible.

### B. `techy::recompose` — the meaning-free Piece fold ([§dd-dr:recompose-machinery])

New top-level public module, `techy/src/recompose/` (`mod.rs` + `context.rs` +
`tests.rs`).

- **`ComposePiece`**: `trait ComposePiece: Clone { fn empty() -> Self;
  fn append(&mut self, other: Self); }` — the piece monoid; techy impls
  `String` and `()` (streaming = a recomposer-held writer with `Piece = ()`;
  **no sink concept**). The `Clone` requirement is `sep`'s per-gap duplication
  (the ruled revisit trigger documented).
- **`Recomposer<L: Lang, A>`**: associated `State` / `Piece: ComposePiece` /
  `Error` +
  `fn recompose_node(&mut self, node: NodeRef<'_, L, A>, state: &Self::State,
  cx: &mut RecomposeContext<'_, L, A>)
  -> Result<Recompose<Self::Piece, Self::State>, Self::Error>`.
  No `Send`/`Sync`; no closure blanket (none ruled; three associated types make
  closure inference hopeless and recomposers are typically stateful —
  realization note).
- **`Recompose<P, S> { Emit(P), Concat(ConcatPieces<P, S>) }`** — the
  instruction enum.
- **`ConcatPieces<P, S>`** — the joiner payload
  (`head + child₁ + sep + … + childₙ + tail`): private fields
  `head`/`sep`/`tail: P`, `state: Option<S>`, `include_attached`/
  `include_hidden: bool`; chainable constructors — seed
  `ConcatPieces::children()` (empty head/sep/tail, inherited state, default
  scope), then `.wrap(head, tail)` / `.join(sep)` (both `impl Into<P>`) /
  `.with_state(state)` (the optional derived state; children fold under it
  instead of inheriting the parent's) / `.include_attached()` /
  `.include_hidden()`. **Default scope skips `Attached` AND `Hidden`** slot
  children (plain children + `Content` regions); widening is the explicit
  opt-in.
- **Entry**: `recompose::recompose(tree: &NodeTree<L, A>, state: R::State,
  recomposer: &mut R) -> Result<R::Piece, RecomposeError<R::Error>>` — the
  accepted `recompose::recompose` stutter. The root's downward state is a
  mandatory argument (the three-channel record's own words — "the
  argument-threaded `S`" — and the [§dd-dr:language-init] mandatory-seed
  precedent; no `Default` bound demanded; `()` for stateless recomposers;
  D-plan-4). Parameter order: state before the recomposer, so the
  recomposer stays last like restage's visitor (D-plan-4).
- **The driver**: ask `recompose_node(node, state, cx)`; `Emit(p)` → `p`;
  `Concat(cp)` → fold `head + Σ(child pieces, sep-joined) + tail` over the
  scoped children (the shared kernel), each child recursing under the derived
  state (`cp.state`) or the inherited `state`. The fold composes values —
  nothing is staged, nothing writes.
- **The wrapping contract** (documented + tested): instructions lower against
  the **outermost** recomposer — the driver holds exactly one recomposer and
  every `Concat` descent re-enters it, so a wrap-intended recomposer returns
  instructions (delegating via a plain inner `recompose_node` call) and never
  descends explicitly; its overrides then apply at every depth. Contrast with
  restage recorded (a takeover visitor stages explicitly).
- **`RecomposeError<E>`** — mirrors `RestageError<E>` (the S7 anchor), the
  applicable variants keeping their exact names, fields, derives
  (`#[non_exhaustive]`; `Clone`/`Debug`/`PartialEq`/`Eq` conditional on `E`;
  `Display where E: Display`; `Error where E: Error + 'static`):
  - `Recomposer(E)` — the callback-failure variant (restage: `Visitor(E)`;
    the mirrored *pattern* is variant-named-after-the-failing-trait —
    `RestageVisitor` → `Visitor`, `Recomposer` → `Recomposer`; D-plan-5).
  - `UnknownArgumentName { node, name }`, `ArgumentIndexOutOfRange { node,
    index, count }`, `SlotIndexOutOfRange { node, index, count }`,
    `NotACallable { node }` — exact mirrors (op-misuse group).
  - `UnknownSlotName { node, name }` — new, forced by the ruled
    `_slot_content_named` op (restage has no by-name slot op; named after
    `UnknownArgumentName`'s pattern; D-plan-6).
  - `NoBodySlot { node }` — new, forced by the ruled `recompose_body` op
    (D-plan-6).
  - **Omitted with the argument** (D-plan-7): `Build` (nothing is staged into a
    builder), `ContentParentDropped` (no records are rebuilt — the fold reads,
    never re-anchors), `ArgumentAbsent` (recompose has no `_with_content`
    helpers; an absent argument recomposes as the empty piece — it contributed
    no bytes), `RootNotSingular` (a fold always produces exactly one piece).
- **`RecomposeContext<'t, L, A>`** — self-passing helper methods, surface kept
  minimal (no builder, no replacement map; a `PhantomData` input-tree anchor,
  the `RestageContext` pattern). The ruled roster (restage-family mirror; ops
  take the sub-fold's state and recomposer, recomposer last; nodes may come
  from any tree):
  - `recompose_argument(node, index, state, recomposer)` → the whole region's
    piece (noise + wrapper syntax + content — the region nodes in source
    order); an **absent argument yields the empty piece** (presence semantics:
    it contributed no bytes; no error — mirrors restage's presence-transfers).
  - `recompose_argument_named(node, name, state, recomposer)` — unknown name =
    `Err(UnknownArgumentName)` (the `_named` convention).
  - `recompose_argument_content(node, index, state, recomposer)` /
    `recompose_argument_content_named(…)` — the designated content nodes only.
  - `recompose_slot_content(node, index, state, recomposer)` /
    `recompose_slot_content_named(node, name, state, recomposer)` — the ruled
    roster names the `_named` form; the positional sibling completes the
    crate's `_named`-beside-positional convention (D-plan-8).
  - `recompose_body(node, state, recomposer)` (bound-where-used
    `SlotExt<L>: BodySlotExt`) — the body slot's content piece;
    `NoBodySlot` when no slot's ext reports body.
  All ops drive the full instruction-lowering fold over their nodes with the
  *passed* recomposer (self-passing keeps the wrapping contract intact).
- **`core_source_instruction`** (free fn, `recompose` module):
  `fn core_source_instruction<'t, L: Lang, A, P, S>(node: NodeRef<'t, L, A>)
  -> Option<Recompose<P, S>> where P: ComposePiece + From<&'t str>` — the
  core-provided instruction for source-faithful emission of a node **from its
  own recorded payload** (per-node doctrine; span-backed *payload* resolves
  against the node's own span's source — permitted; `span_content()` is never
  consulted):
  - `Chars` → `Emit(content)`;
  - `Comment` → `Emit(start + content + post_space)`;
  - `Group` → `Concat(children().wrap(open, close))`;
  - `List` → `Concat(children())`;
  - `Callable` → **`None`** (declines — the payload is Lang-owned).
- **Targeted replacement is NOT a mechanism** — module docs section: the
  wrapper pattern (a wrapping recomposer overrides the targeted nodes and
  delegates the rest; no span fast path) + the documented restage→recompose
  pipeline (transform first, then reemit).
- **Reading contract** in the module docs, verbatim from the rulings:
  permitted — any field of the node's own payload, incl. resolving span-backed
  payload (`TextContent::Spanned`); forbidden — resolving any span content,
  the node's own span included, against the source; no span fast path (no
  freshness signal exists to gate one); `span_content()` stays a consumer
  affordance the recomposer never uses; the word "honest" does not appear.

### C. Preset `SourceRecomposer<LLL>` ([§dd-dr:recompose-machinery], latexlike)

New `techy/src/latexlike/recompose.rs`; exports `SourceRecomposer`,
`source_recomposer`, and its error type from `latexlike/mod.rs`.

- **`SourceRecomposer<LLL: LatexlikeLang = Latexlike>`** — public PhantomData
  ZST (the `BeginSpec` pattern); constructor free fn
  `latexlike::source_recomposer::<LLL>() -> SourceRecomposer<LLL>`.
- `impl<LLL: LatexlikeLang, A> Recomposer<LLL, A> for SourceRecomposer<LLL>`:
  `State = ()`, `Piece = String`, `Error = SourceRecomposeError`;
  **instruction-only** (never descends explicitly; needs no scope call at all —
  the default scope already skips `Attached`/`Hidden`):
  - non-callables → `core_source_instruction` (covers all four core-complete
    kinds);
  - callables → read the payload through the fifth role trait
    (`LatexlikeInvocationSyntax`), checking coherence against
    `callable_type` (`LatexlikeCallableType` roles):
    - macro form → `Concat(children().wrap(escape_char + name + post_space,
      ""))` — the recorded trigger spelling as head, arguments/children fold
      behind it;
    - environment form → `Concat(children().wrap(env.write_begin(name,
      source), env.write_end(name, source)))` — the `Env` type owns its own
      re-emission (the writer pair is exactly why `Concat` has separate
      head/tail); an empty end side writes `""` (recovered shapes reemit what
      was recorded);
    - specials form → `Emit(name)` — name-as-written IS the spelling
      (paragraph-break `Specials` nodes included);
    - **coherence mismatch** (payload arm vs `callable_type` role) →
      `Err(SourceRecomposeError::IncoherentInvocationSyntax { … })` — the
      ruled coherence error variant (the Round-2 "honest cost" made
      diagnosable; D-plan-9 for the exact shape).
- **`SourceRecomposeError`** — `#[non_exhaustive]`, one variant for now
  (`IncoherentInvocationSyntax`), uniform-Clone derives + `Display`/`Error`.
- The preset owns recomposition accuracy = what the parse records (accuracy
  doctrine in the rustdoc; byte-exactness rests on payload completeness).

### D. Oracle acceptance suite (R15)

New integration test file `techy/tests/recompose_oracle.rs` — public-API-only
(the acceptance-suite principle: anything the oracle cannot reach is an API
gap), plus in-module unit tests for machinery details.

- **Strict matrix**: for each representative input, parse under
  `Recovery::Strict`, `recompose(&tree, (), &mut source_recomposer())`, assert
  output == input. Constructs: macros with/without post-space (`\emph  x`,
  `\emph{x}`, `@`-escape), arguments (mandatory/optional/absent-optional,
  `\frac 1 2` single tokens, star markers, inter-argument noise), environments
  (std, `\begin {itemize}` tolerated spacing, nested, with arguments),
  verbatim (environment + `\verb|…|`), specials (ligatures, `~`), paragraph
  breaks (BOTH `ParagraphBreakStyle`s), groups (`{…}`, nested), math (all
  four delimiter pairs, inline + display), comments (incl. post-space and
  end-of-input comments), and a mixed kitchen-sink document.
- **Tolerant matrix**: parse malformed inputs under `Recovery::Tolerant`;
  reemit == input expected to hold for: unterminated environment (end side
  empty → `write_end` `""`), terminator mismatch
  (`\begin{A}x\begin{B}y\end{A}` — the inner env consumed nothing it does not
  reemit), unclosed group (close recorded empty), stray close (recovered as
  `Chars`), orphan `\end` (content-preserving recovery), forbidden chars in
  math. **The malformed-terminator exception** (the S5 flag): `\end` consumed
  alone records no end-side facts and no node carries those bytes
  (environment_parser.rs recovery: consume the command alone + close), so
  payload-only reemission elides exactly the consumed `\end` spelling — the
  case is **excluded from the equality matrix and pinned by a dedicated
  test** asserting the exact elided output, with the flag cited (the records
  anticipated this: accuracy is coupled to parse-*recording* accuracy, and
  the recovered shape records less than it consumed; D-plan-10 — no
  escalation, the records delegate the accounting).
- **Multi-source matrix** (rides S6's I-18 surface): an `\input` parse via a
  public `SourceResolver`; root reemit == the includer's bytes (the
  `Attached` slot skipped by the default scope — `\input{file}` reemits as
  spelled); recomposing the attached body's nodes (via the slot ops)
  reproduces the included source's bytes; `include_attached()` widening
  demonstrated.
- Machinery-level tests (in-module, M2–M5): fold shapes (join/wrap/empty
  children), state threading (derived vs inherited), scope widening,
  streaming `Piece = ()`, the wrapping contract (deep override through a
  wrapper over `SourceRecomposer`), op-misuse errors, error transport,
  coherence error, materialized-tree reemission (source-independent
  byte-faithful reconstruction), `core_source_instruction` per-kind +
  callable decline.

### E. Records + docs

- DR status lines / applied notes: [§dd-dr:recompose-machinery] +
  [§dd-dr:visit-engine] → applied (Phase 3 S8, with application specifics);
  [§dd-dr:recompose] applied note (the fold landed; oracle in place);
  [§dd-dr:invocation-syntax] status line (the reemit oracle suite landed, S8);
  [§dd-dr:restage-ops] mirror amendment applied note (the actual
  `RecomposeError` roster deltas); [§dd-dr:slot-roles] applied note (the one
  role-sensitive site, Concat's default scope, landed S8).
- ARCHITECTURE.md: [§dd-arch:arch] public-topology passage (visit + recompose
  now applied); [§dd-arch:nodes] "Still ruled, not yet applied" passage →
  applied-S8 rewrite; [§dd-arch:engine] checked (no recompose claims — touch
  only if invalidated); [§dd-arch:latexlike] SourceRecomposer bullet +
  generalization tracker.
- CLAUDE.md: facade list gains `techy::visit` + `techy::recompose`.
- lib.rs module list docs; docs/ guide pages checked (learn-by-example and
  concepts-overview get the minimal recompose/visit mentions where
  invalidated).
- Full rustdoc on everything new (missing_docs stays zero); superseded-names
  sweep (`Bit`/`ComposeBit`, `ConcatSpec`/`ConcatParts`, `walk_tree`/
  `recompose_tree`, `new_for_invocation`, "span-verbatim", sink vocabulary,
  `VisitCx`/`RecomposeCx`, "honest" in rustdoc).

## File map

- `techy/src/visit.rs` — NEW: module docs, `walk`, `NodeVisitor` + blanket,
  `VisitFlow`, `VisitContext`, `scoped_children` kernel (pub(crate)), tests.
- `techy/src/recompose/mod.rs` — NEW: module docs (reading contract, wrapping
  contract, targeted-replacement pattern, three-channel pointer),
  `ComposePiece` (+ String/() impls), `Recompose`, `ConcatPieces`,
  `Recomposer`, `RecomposeError`, `core_source_instruction`, re-exports.
- `techy/src/recompose/context.rs` — NEW: `recompose` entry, the driver +
  lowering, `RecomposeContext` + region ops.
- `techy/src/recompose/tests.rs` — NEW: machinery tests.
- `techy/src/latexlike/recompose.rs` — NEW: `SourceRecomposer`,
  `source_recomposer`, `SourceRecomposeError`, preset tests.
- `techy/src/latexlike/mod.rs` — module wiring + exports + module-doc line.
- `techy/src/lib.rs` — `pub mod visit; pub mod recompose;` + module list docs.
- `techy/tests/recompose_oracle.rs` — NEW: the oracle matrices.
- Records/docs: DESIGN_RATIONALE.md, ARCHITECTURE.md, CLAUDE.md, docs/ pages,
  this report.

## Test plan (acceptance from § S8 + design-forced cases)

1. Visit: preorder order + enter/exit pairing + depth values; SkipChildren
   (no child enters, exit still fires); Stop (immediate abort, no further
   events); role-blind walk over a Content+Attached+Hidden slot fixture;
   closure-blanket smoke; subtree walk from a mid node.
2. Recompose machinery: Emit/Concat fold shapes (wrap, join incl. single-child
   no-sep and empty-children head+tail-only); derived-state threading vs
   inheritance; default scope skips Attached AND Hidden + each widening flag;
   `Piece = ()` streaming recomposer (writer in `&mut self`); error transport.
3. `core_source_instruction`: per-kind emission on parsed trees; callable
   decline (`None`).
4. Region ops: argument region vs content; named variants + unknown-name
   errors; slot content by index/name; `recompose_body` + `NoBodySlot`;
   `NotACallable`; index out of range.
5. Wrapping contract: a wrapper over `SourceRecomposer` overriding a target
   buried several levels down — the override applies at depth (lowering
   against the outermost), and the wrapper never descends explicitly.
6. SourceRecomposer: per-construct reemission (macros incl. post-space and
   `@`-escape, arguments, environments incl. spacing pathologies, verbatim,
   specials, paragraph breaks both styles, math, groups, comments); coherence
   error on a hand-built mismatched payload; materialized-tree reemission.
7. Oracle matrices per § D (strict / tolerant incl. the malformed-terminator
   pin / multi-source).

## Milestones

Commit per milestone (`P3-S8 M<k>: <what>`); each lands green (build + lib
tests at minimum; full gates at M7).

- M1: this plan.
- M2: `techy::visit` (module + kernel + tests).
- M3: `techy::recompose` core (piece monoid, instruction enum + chainable
  constructors, trait, error enum, entry + driver + lowering over the shared
  kernel, `core_source_instruction`; machinery tests).
- M4: `RecomposeContext` region ops + wrapping-contract test + streaming test
  + op-misuse error tests.
- M5: preset `SourceRecomposer` (+ constructor + coherence error) + preset
  reemission tests incl. materialize-through.
- M6: the oracle suite (strict + tolerant + multi-source matrices; the
  malformed-terminator pin).
- M7: records + docs + full gates + closure tables in this report.

## Risks

1. **Scope-kernel arithmetic**: excluded-slot ranges are global node-index
   ranges; children iteration must skip whole regions — slots are few, so a
   per-node collect is fine; dedicated fixture with Content + Attached +
   Hidden slots on one callable.
2. **Wrapping-contract test fidelity**: must discriminate outermost-lowering
   (a deep target under a delegated node must hit the wrapper) — a
   nested fixture with the override target buried two levels down.
3. **Oracle breadth vs parse quirks**: tolerant shapes may reveal recording
   gaps beyond the flagged malformed-terminator case; each such finding is
   pinned + recorded (deviation or escalation per the rules), never silently
   patched around.
4. **`From<&'t str>` lifetime plumbing** in `core_source_instruction` —
   resolve-then-convert against the node's own source; `String` satisfies it
   for any lifetime.

## Deviations / delegated decisions (running list — for user sign-off)

- **D-plan-1** (realization, record-consistent): `walk` takes the start
  `NodeRef` (not `&NodeTree`) — the vetoed spelling was the *method*
  placement (`NodeRef::walk`; core cannot name the engine), and the T4 origin
  of the walker (structured replacement for `Descendants::with_depth`)
  demands subtree walks; whole tree = `walk(tree.root(), v)`. `recompose`
  keeps the ruled `&NodeTree` entry shape.
- **D-plan-2** (realization, under-determined): the walk is infallible — no
  `Error` associated type on `NodeVisitor`; the ruled sketch returns bare
  `VisitFlow`, the engine has no failable ops, and error-carrying walks
  return `Stop` with the error in visitor state.
- **D-plan-3** (realization, under-determined): `exit` fires after a node's
  children for `Descend` and immediately for `SkipChildren`; `Stop` aborts
  the whole walk with no further `enter`/`exit` calls. `exit` receives the
  same `(node, cx)` pair as `enter` (symmetry; the pre-context sketch had
  `exit(node)`).
- **D-plan-4** (realization, forced by the ruled state model): the entry is
  `recompose(tree, state, recomposer)` — the root's downward state is a
  mandatory explicit argument ("argument-threaded S" is the ruling's own
  vocabulary; no hidden `Default` demand, the [§dd-dr:language-init]
  precedent); parameter order keeps the recomposer last (the restage
  visitor-last convention), and the region ops follow
  (`recompose_argument(node, index, state, recomposer)`).
- **D-plan-5** (naming, mirror-pattern argument): the callback-failure variant
  is `RecomposeError::Recomposer(E)` — the restage mirror names its variant
  after the failing trait (`RestageVisitor` → `Visitor`), and the recompose
  trait is `Recomposer`; a literal `Visitor(E)` would misname the party (the
  visit walker's `NodeVisitor` is a different trait that cannot even fail).
- **D-plan-6** (FORCED by the ruled op roster): two variants with no restage
  source — `UnknownSlotName { node, name }` (the ruled
  `recompose_slot_content_named` op needs the unknown-name `Err` the `_named`
  convention prescribes; patterned on `UnknownArgumentName`) and
  `NoBodySlot { node }` (the ruled `recompose_body` op on a callable whose
  slots' exts report no body).
- **D-plan-7** (FORCED, mirror-what-applies): `RestageError` variants omitted
  from `RecomposeError` — `Build` (the fold stages nothing),
  `ContentParentDropped` (no records are rebuilt or re-anchored),
  `ArgumentAbsent` (no `_with_content` helpers exist here; an absent argument
  recomposes as the empty piece — presence semantics mirror
  restage_argument's no-error absence), `RootNotSingular` (a fold always
  yields exactly one piece).
- **D-plan-8** (realization, convention-completing): the positional
  `recompose_slot_content(node, index, …)` sibling accompanies the ruled
  `_slot_content_named` — a `_named` accessor without its positional sibling
  would be unprecedented in the crate (the `_named` convention names the
  by-name variant OF a positional op; restage's own mirror is
  `restage_slot(node, index, …)`).
- **D-plan-9** (delegated shape): the coherence error is
  `SourceRecomposeError::IncoherentInvocationSyntax { node: NodeId,
  callable_form: &'static str, payload_arm: &'static str }` — role and arm as
  static labels (the generic `CallableTypeId` is not nameable on a
  non-generic error type without dragging `LLL` onto it).
- **D-plan-10** (record-determined accounting of the S5 flag): the tolerant
  oracle matrix excludes the malformed-terminator shape from the equality
  matrix and pins it in a dedicated test asserting the exact recorded-only
  reemission (input minus the consumed-alone `\end` spelling) — the accuracy
  doctrine couples reemission to what the parse *records*, and this recovery
  deliberately records less than it consumed; adding a partial end-side
  record now would be a new recording decision no ruling ordered.

(Further entries appended as implementation forces them.)
