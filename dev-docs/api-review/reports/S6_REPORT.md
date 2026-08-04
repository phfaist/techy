# Phase 3 — S6: `\input` wiring + multi-source + line/col — stage report

Branch `phase3-s6-input-wiring` off `api-review` @ 4113aa8. Governing inputs:
PHASE3_PLAN.md § Protocol + § S6; DESIGN_RATIONALE entries [§dd-dr:input-wiring]
(central), [§dd-dr:resolver-contract] (+ Clone amendment),
[§dd-dr:include-chain-helpers], [§dd-dr:line-col-ownership],
[§dd-dr:input-attachment], [§dd-dr:source-resolver], [§dd-dr:lazy-line-col],
[§dd-dr:slot-roles], [§dd-dr:span-invariants], [§dd-dr:origin-genericity],
[§dd-dr:wire-identifier-stability], [§dd-dr:superseded-names]; T4_RULINGS §§ B, C,
F; T5_RULINGS § G + I-18; S5_REPORT signature tables (the surface built on).

## Progress

- [x] Plan committed (this file)
- [x] M1 — `ResolveError` Clone + include-chain helpers
- [x] M2 — the door + the bundle + the two conditions
- [x] M3 — parse-law checker `Attached`-scoping
- [x] M4 — preset `input_macro_spec` + multi-source acceptance (I-18)
- [x] M5 — line/col ownership (`line_of`, `line_col_span`, `LineIndexCache`,
      `LineColProvider`, `_with` variants, scan-len raise)
- [x] M6 — docs + records + closure (full gate run)

## Design synthesis (records → today's code)

### The door (T4-B2, [§dd-dr:input-wiring])

`ParseContext::parse_attached_source(source, state, parser) ->
ConstructParserResult<L, Vec<BuildId>>` in a new internal file
`techy/src/constructs/attached_source.rs` (internal layout free; public path
`techy::core::constructs` via the `ParseContext` surface).

- **Parser parameter type (delegated to application by T4-B2 — "exact parameter
  type at application")**: `parser: &mut P where P: ConstructParser<L, Output =
  NodesOutcome<L>> + ?Sized` — the `parse_scoped` vocabulary with the output
  pinned to the nodes-run shape. Rationale: the ruled return type
  (`Vec<BuildId>`) plus the ruled *local stray-close recovery* require the door
  to see run outcomes (`StopCause`) and to resume — only the
  `NodesOutcome` vocabulary carries both. The default `\input` choice passes
  `&mut *driver.make_nodes_parser(StopSpec::none(), ChildStateSpec::inherit())`
  (the root nodes-parse shape). Recorded as D-plan-1.
- **Internals**: fresh `StdTokenReader` over `source.content()` (the fresh inner
  context — the outer reader's borrow of the outer source is untouched; the
  inner reader lives inside the call); inner `ParseContext` over the **same
  session** (reborrowed `&mut *self.session`) and driver — `BuildId`s stay
  session-global; a traceback `Frame` (`FrameTitle::Static("attached source")`,
  span = the source's `provenance().triggered_at()` clone, falling back to the
  attached source's start for provenance-less sources — the rendered report's
  provenance-chain section names the reference, so the static title suffices
  without a new allocation-carrying `FrameTitle` variant; D-plan-2) wrapping the
  whole inner drive.
- **The drive loop** mirrors `Language::parse_source`'s root loop: run
  `parser` via `parse_scoped(current_state, parser)`; extend the collected
  nodes; re-anchor the inner ambient state on `outcome.state`; on
  `UnexpectedGroupClose` diagnose `StrayGroupClose` through the recover funnel
  (strict aborts — an `Err` bubbles; tolerant records), consume the delimiter,
  stage it as a span-backed `Chars` node (markup-in-chars artifact — per-source
  byte accounting stays exact), and **re-invoke the same parser** (documented
  contract: a parser driven through the door must tolerate re-invocation after
  a stop — the standard `NodesParser` drains its working state at every return,
  so it does); on `EndOfInput` break; on `TokenCondition`/`NodeCondition`
  break as well — the caller-supplied parser's own stop conditions are its
  business, unlike the root loop which knows it set none (D-plan-3).
- The door stages **content nodes only** — no wrapper `List`, no slot: slot
  assembly stays the invocation parser's job (one staging door holds).
- No `FromInvocation` bound on the door itself (the parser is supplied, not
  built from the factory).

### The bundle (T4 rounds 1b/1c, [§dd-dr:input-wiring])

`ParseContext::attach_source_reference(&mut self, reference, at, state, parser)
-> ConstructParserResult<L, Option<Vec<BuildId>>>` — a **method** beside the
door (T4-1c "home: core, beside `parse_attached_source` on the `ParseContext`
surface"; the DR's `(cx, …)` spelling is read as including the receiver;
D-plan-4). The single resolve-diagnose-attach raising site:

1. `self.driver.source_resolver()` — `None` → raise **`NoSourceResolver`**
   (`core.sources.no-resolver`, payload `reference: String`; D-plan-5 on the
   payload, which B left unspecified) at `at` through the recover funnel;
   tolerant continuation returns `Ok(None)` ("nothing attached").
2. `resolve_source_reference(resolver, reference, at)` — `Err(e)` → raise
   **`UnresolvableSourceReference`** (`core.sources.unresolvable-reference`,
   payload `reference: String` + `error: ResolveError` — plain, `Clone` again)
   likewise; tolerant → `Ok(None)`.
3. `Ok(source)` → `self.parse_attached_source(source, state, parser)` →
   `Ok(Some(nodes))`.

Conditions declared in `constructs/attached_source.rs` (producer-side rule),
exported via `techy::core::constructs`; derive-based, with
`impl ToDiagnosticValue for ResolveError` added in `error.rs` (stratum
direction: error may reach down to source, not vice versa) rendering
reference + message + the `Error::source()` cause chain — the ruled
`serializable_data` content. Identifier-asserting tests beside them (S1-slate
style).

### `ResolveError` → `Clone` (T4-1c, [§dd-dr:resolver-contract] amendment)

`cause: Option<Box<dyn Error + Send + Sync>>` → `Option<Arc<dyn Error + Send +
Sync>>`; `#[derive(Clone)]` restored (manual field construction unaffected);
`with_cause` wraps with `Arc::new`; `Error::source()` unchanged (deref through
the `Arc`). Rustdoc records the ruled principle: **techy error types stay
uniformly `Clone`; out-of-crate information sits behind the `Arc`**.

### Include-chain helpers ([§dd-dr:include-chain-helpers])

In `techy::source`:

- `Source::including_sources(&self) -> IncludingSources<'_, O>` — iterator over
  `&Source<O>`: self first, then each including source via
  `provenance().triggered_at()` hops, ending at the primary (the sibling of
  `provenance_chain()`, yielding *sources* whose origins carry the comparable
  names). Lives in `source/source.rs` beside `provenance_chain`.
- `check_include_chain<O: SourceOrigin, K: PartialEq>(target_key: &K,
  triggered_at: &SourceSpan<O>, origin_key: impl Fn(&O) -> Option<K>,
  max_depth: Option<usize>) -> Result<(), ResolveError>` — in
  `source/resolver.rs` (returns `ResolveError`; the resolver-policy helper).
  Origin-keyed **including the primary** (walks
  `triggered_at.source().including_sources()`); `None` keys skipped; distinct
  messages for cycle vs depth-exceeded; the minted `ResolveError`'s
  `reference` field is the matched source's origin label when present, else
  empty (K is not `Display`-bounded by the ruled signature — the caller's
  resolver can `map_err` its own reference in; D-plan-6). Depth = number of
  `triggered_at` hops of the *would-be-included* source (primary = 0);
  `Some(max_depth)` errors when the new source's depth would exceed it.
  no_std-clean (core + alloc only).

NO recursion control in core: the door itself never counts or compares
(`.dtx` self-inclusion stays legal — pinned by a core-level self-include test).

### Preset `input_macro_spec` (T4-B4, [§dd-dr:input-wiring])

New internal file `techy/src/latexlike/input.rs`:

- `pub struct InputMacroSpec<LLL: LatexlikeLang = Latexlike>` (public spec type
  — the `MacroSpec`/`SpecialsSpec` pattern: manual Debug/Clone; a stable
  downcast target; D-plan-7 on the type's existence, the record names only the
  constructor) holding the one mandatory braced-argument spec.
- `pub fn input_macro_spec<LLL: LatexlikeLang>() -> InputMacroSpec<LLL>` with
  bounds-where-used `ArgumentExt<LLL>: Default` (std group argument parser) and
  `SlotExt<LLL>: BodySlotExt` (slot-ext minting, below). **Never preloaded** —
  not in `base_package`; embedders insert it into their own package under their
  macro callable type and any name.
- `CallableSpec` impl: `arguments()` = the one `{…}` spec; frame title "macro
  ‘\input’" (`frame_title("macro", …)`); `make_invocation_parser` returns the
  brief-form composition:
  1. `parse_declared_arguments(cx, spec, name_span)` (the shared loop);
  2. **argument text**: the provided argument's group interior slice from
     `cx.source` (staged group node's span minus its recorded delimiters, via
     `cx.staged_nodes()`); a small private helper keeps the spec body brief —
     no new public helper minted (not ruled);
  3. `at` = the whole invocation span (trigger start → last argument end) —
     what provenance `triggered_at` records and diagnostics highlight;
  4. `cx.attach_source_reference(reference, &at, Arc::clone(&cx.state), &mut
     *cx.driver.make_nodes_parser(StopSpec::none(), ChildStateSpec::inherit()))`;
  5. `Some(nodes)` → append to children; one slot
     `ParsedSlot::new(region, "attached", SlotRole::Attached,
     BodySlotExt::make_body())` — the attached slot **is** the node's body slot
     (body-ness is the ext axis, recorded independently of the role;
     [§dd-dr:slot-roles]'s T5 amendment explicitly contemplates the
     Attached-body pairing, and `BodySlotExt` is the only generic slot-ext
     minting mechanism; D-plan-8). `None` (diagnosed-and-recovered) → no slot.
  6. `cx.stage_invocation(&invocation, arguments, slots, children,
     Some(args_end))` — explicit end: the node's span is its invocation in the
     *includer's* source; the last child (attached content) lives in another
     source, so the std last-child rule must not apply.
- **State-transparent by design** (D-plan-9): the attached content parses under
  the state at the `\input` point; its internal after-effect deltas do **not**
  continue into the includer (the ruled door shape returns nodes only, and the
  `ConstructParser` after-effect channel is a delta no one can reconstruct
  from an exit state). Documented loudly on the spec, with the
  state-propagating variant named as custom-composition work.
- Rustdoc carries the no-caching discussion (T5 §G digest): why parse-time
  reading on the spot is the only generally-correct behavior (a
  state-modifying `\input` invalidates any parse-without-attachment cache),
  with the cached-splice recipe deferred to the Phase 4 include chapter under
  its explicit state-transparency precondition.

### Parse-law checker `Attached`-scoping (T5-F5, [§dd-dr:slot-roles])

`node/invariants.rs` `check_parse_law_node`, callable arm (the `TODO(S6)`
spot): partition the children block by slot role —

- children inside an `Attached` slot's region: **excluded** from the including
  callable's children-in-source and span-contiguity checks; instead their own
  per-source accounting: all children of one attached region share one source
  (each other's, not the parent's) and are span-contiguous among themselves
  (their subtrees recurse through the per-node loop as usual);
- children inside a `Hidden` slot's region: excluded from byte accounting
  entirely — the ruled `Hidden` semantics ("no recomposition, no byte
  accounting", [§dd-dr:slot-roles]); techy mints none, a hand-built tree test
  pins it (D-plan-10);
- remaining children (arguments + `Content` slots): today's checks, with
  contiguity measured across the *remaining* sequence (the excluded regions
  are invisible to the parent's byte accounting — declaration replaces
  source-change inference).

Core checker stays payload-blind (S5 split); the preset checker
(`check_latexlike_tree_invariants`) inherits the scoping by layering.

### Line/col ownership (T4 §F, [§dd-dr:line-col-ownership])

`source/line_index.rs`:

- `LineIndex::line_of(&mut self, offset) -> Option<(usize, Range<usize>)>` —
  the line number (same numbering/offset conventions as `line_col`) plus the
  line's byte range, **excluding the line terminator** (the caret/underline
  path slices displayable content); the range end is found by a direct
  `find('\n')` scan from the line start.
- `LineIndex::line_col_span(&mut self, impl Into<Range<usize>>) ->
  Option<((usize, usize), (usize, usize))>` — `line_col` of start and end
  (end = the exclusive end offset's position); `Some` only when both answer.
- `DEFAULT_MAX_SCAN_LEN` 100_000 → **500_000** (private const; docs updated —
  the loud silent-`None` docs stay; the `error.rs` too-long-source test's
  content grows past the new bound).
- `pub struct LineIndexCache<O: SourceOrigin = Option<String>>` — persistent
  consumer-held cache: entries own `Arc<Source<O>>` + `Vec<usize>` line-starts
  (computed eagerly per source on first touch, bounded by the scan cap),
  keyed by `Arc` identity (linear scan — reports touch few sources); API
  mirrors `line_col`/`line_of`/`line_col_span` (each taking `source:
  &Arc<Source<O>>` first) + `new`/`Default`/`set_max_scan_len` (the bound must
  stay adjustable, as on `LineIndex`; D-plan-11); entries never invalidate
  (content immutable); line/column offsets applied from each entry's source.
- `pub trait LineColProvider<O: SourceOrigin>` — single method
  `line_col(&mut self, source: &Arc<Source<O>>, offset: usize) ->
  Option<(usize, usize)>`; implemented by `LineIndexCache<O>`.

`error.rs` rendering seams gain `_with(&mut impl LineColProvider<O>)` variants
— `Diagnostic::render_with`, `ParseError::render_with`,
`Diagnostics::render_all_with`, `format_position_with`,
`format_traceback_with` (provider parameter last) — with the no-argument forms
as transient-cache shorthand over a fresh `LineIndexCache` (which **replaces**
the private borrowing `SourceIndexCache`; its blocked-dep-free code comment
moves with the mechanism). Shared helpers extracted so `LineIndex` and the
cache entries compute from one line-starts search body.

## File map

| File | Work |
|---|---|
| techy/src/source/source.rs | `including_sources` + `IncludingSources` |
| techy/src/source/resolver.rs | `ResolveError` Clone/Arc-cause; `check_include_chain` |
| techy/src/source/line_index.rs | `line_of`, `line_col_span`, scan-len raise, `LineIndexCache`, `LineColProvider` |
| techy/src/source/mod.rs | exports |
| techy/src/constructs/attached_source.rs | NEW: door + bundle + 2 conditions + tests |
| techy/src/constructs/mod.rs | module + re-exports |
| techy/src/core/constructs.rs | facade exports |
| techy/src/error.rs | `ToDiagnosticValue for ResolveError`; `_with` render variants; `SourceIndexCache` → `LineIndexCache` |
| techy/src/node/invariants.rs | `Attached`/`Hidden` scoping of the parse-law byte accounting |
| techy/src/latexlike/input.rs | NEW: `InputMacroSpec` + `input_macro_spec` + tests |
| techy/src/latexlike/mod.rs | module + exports |
| dev-docs/DESIGN_RATIONALE.md | status lines / applied notes |
| dev-docs/ARCHITECTURE.md | source-topic + engine passages |
| CLAUDE.md / docs/*.md | facade item lists / invalidated passages |

## Milestones

1. **M1** — source topic: `ResolveError` Clone (+ principle rustdoc),
   `including_sources`, `check_include_chain` (+ unit tests: chain walk, cycle
   incl. primary participation, depth, `None`-key skip, distinct messages).
2. **M2** — the door + bundle + conditions (+ `ToDiagnosticValue for
   ResolveError`); engine-level tests: attached parse round trip (BuildIds
   session-global, per-source spans), stray-close-in-included-source recovery
   (never unwinds the includer; strict aborts), no-resolver + unresolvable
   diagnostics (span at `at`, tolerant continuation), identifier pins,
   self-include legality at core level.
3. **M3** — parse-law checker scoping + hand-built tests (attached region
   own-source accounting; Hidden exclusion; violations still caught).
4. **M4** — preset `input_macro_spec` + I-18 acceptance tests (per-source
   slices/spans, `Attached` slot shape, preset payload-pin oracle green on
   multi-source trees, nested/self include, never-preloaded pin, tolerant vs
   strict matrix).
5. **M5** — line/col package (+ tests: `line_of` boundaries incl. first/last
   line + empty source, `line_col_span`, cache fresh-vs-cached agreement,
   per-source isolation, `render*_with` parity with the no-arg forms,
   scan-len raise reflected).
6. **M6** — rustdoc sweep, DR status lines ([§dd-dr:input-wiring],
   [§dd-dr:resolver-contract], [§dd-dr:include-chain-helpers],
   [§dd-dr:line-col-ownership], [§dd-dr:slot-roles] S6 rider,
   [§dd-dr:input-attachment], [§dd-dr:wire-identifier-stability] slate note),
   ARCHITECTURE/CLAUDE.md/guide passages, closure tables, full gates.

## Risks

- The door's re-invocation contract on caller-supplied parsers (D-plan-3):
  pinned by rustdoc + the recovery test; `NodesParser` verified re-invocable
  (working state drains at every return).
- Borrow shape of the fresh inner context (session reborrow + new reader):
  compiles in principle (shorter-lived inner `'a`); if the dyn-reader lifetime
  fights, fall back to driving the loop body in a nested scope.
- `stage_invocation(end_pos: Some)` with foreign-source children: verified
  against the S5 childless-containment pin (the macro payload pin checks
  containment, not span-end — a takeover claiming extent past the trigger is
  sanctioned).
- Eager per-source table in `LineIndexCache` vs `LineIndex` laziness: bounded
  by the scan cap; recorded in docs (persistence is the cache's point).

## Deviations / delegated decisions (running list — for user sign-off)

- **D-plan-1** (delegated by T4-B2 "exact parameter type at application"): the
  door's parser parameter is `&mut P where P: ConstructParser<L, Output =
  NodesOutcome<L>> + ?Sized` — the ruled `Vec<BuildId>` return and the ruled
  local stray-close recovery both require the nodes-run outcome vocabulary
  (`StopCause` + exit state); `parse_scoped` precedent for `&mut P + ?Sized`.
- **D-plan-2** (under-determined): the door's traceback frame is
  `FrameTitle::Static("attached source")` anchored at the attached source's
  `provenance().triggered_at()` (fallback: the attached source's start). No
  new owned-string `FrameTitle` variant: frames are allocation-free to build
  by design, and the rendered report's provenance-chain section already names
  the reference.
- **D-plan-3** (under-determined): the door treats `TokenCondition` /
  `NodeCondition` stops as "the parser finished its run" (break), not as
  implementation errors — the door, unlike the root loop, does not know the
  caller's stop spec. Stray-close resume re-invokes the same parser instance;
  the re-invocation contract is documented on the door.
- **D-plan-4** (spelling reconciliation): `attach_source_reference` is a
  `ParseContext` **method** (T4-1c "on the `ParseContext` surface"); the DR's
  `(cx, …)` argument spelling is read as naming the receiver. Return type
  `Option<Vec<BuildId>>`: `None` = diagnosed-and-recovered (tolerant), the
  spec stages no attached slot.
- **D-plan-5** (payload under-determined): `NoSourceResolver` carries
  `reference: String` (the message names what failed to resolve; symmetric
  with its sibling's reference field).
- **D-plan-6** (realization): `check_include_chain`'s minted `ResolveError`
  uses the matched/failing source's origin label as `reference` when present
  (else empty) — `K` is not `Display`-bounded by the ruled signature.
- **D-plan-7** (realization): the constructor's return type is the new public
  `InputMacroSpec<LLL>` (the preset spec-type pattern: stable downcast target,
  preset traceback vocabulary). The record names only `input_macro_spec()`.
- **D-plan-8** — **SUPERSEDED-BY-RULING (user, 2026-08-04)**: the shipped
  `\input` must NOT overload the preset's body marker. New shape (M7): the
  attached slot stays named `"attached"` with `role: SlotRole::Attached`, but
  its ext is an **embedder-supplied constructor value**
  (`input_macro_spec(…, attached_slot_ext: SlotExt<LLL>)`, cloned per
  invocation); the spec never calls `BodySlotExt::make_body()`. Shipped
  registrations pass `BodyMarker::not_body()` — `body()` returns `None`,
  retrieval is `slot_content_nodes_named("attached")`; a body-marked ext
  remains a framework option that `body()` finds (T5 findability clause,
  pinned by test).
- **D-plan-9** — **RESOLVED AS RULINGS AMENDMENT (user, 2026-08-04)**:
  `persist_state: bool` (mandatory) on `input_macro_spec` decides whether
  included state changes persist past the `\input`; mechanism = merged
  after-effect deltas (NOT state diffing), carried by the door's new outcome
  bundle and forwarded through the existing sibling channel when `true`
  (`false` = the previously shipped transparent behavior). See the M7 section
  below for the machinery.
- **D-plan-10** (ruled-elsewhere realization): the parse-law checker also
  excludes `Hidden`-slot children from byte accounting entirely — the ruled
  `Hidden` semantics from [§dd-dr:slot-roles]/T5-A9, landed here because the
  same role partition drives both exclusions.
- **D-plan-11** (realization): `LineIndexCache` carries `set_max_scan_len`
  beside the ruled query mirror — the bound must stay adjustable (F6 kept it
  on `LineIndex`); an abandoned entry is re-admitted when a raised bound
  allows it, mirroring `LineIndex::set_max_scan_len`.
- **D-plan-12** (realization): `line_of`'s returned range excludes the line
  terminator (`\n`); the offset may equal the range end (a position on the
  newline itself, or end-of-content). First/last/empty-source boundaries
  pinned by tests.
- **D-plan-13** (realization): the `\input` spec's one argument is **named**
  `"reference"` (the named-first constructor doctrine; self-describing record —
  `argument_content_nodes_named("reference")` reads the reference back). Its
  parser is the standard `{` shape (`GroupArgumentParser::new(content_group)`,
  expression fallback on — `\input a` takes the one-expression reference `a`,
  the pylatexenc `'{'`-code convention).
- **D-plan-14** (realization; scope narrowed by the 2026-08-04 ruling): the
  door still discards the sub-parse's **pass-through** delta channel (the
  standard nodes parser returns `None` there by convention), but the run's
  *applied* after-effects are no longer lost — they travel as the outcome
  bundle's merged record (`AttachedSourceOutcome::after_effects`), and
  forwarding is the caller's choice (M7).

## Consolidated stage summary (M6 closure)

### Outcome

All six milestones landed in one run, gates green throughout. The `\input`
engine wiring is complete ([§dd-dr:input-wiring] fully applied): the
`parse_attached_source` door sub-parses a resolved source into the running
session over a fresh inner reader with local stray-close recovery and an
"attached source" traceback frame; `attach_source_reference` beside it is the
single resolve-diagnose-attach raising site of the two new `core.sources.*`
conditions; `ResolveError` is `Clone` again (Arc-backed cause; the uniform-Clone
principle recorded in rustdoc); the include-chain policy tools
(`Source::including_sources`, origin-keyed `check_include_chain` incl. the
primary, distinct cycle/depth messages) landed in `techy::source` with core
recursion checking still absent (self-inclusion pinned legal); the preset ships
the opt-in, never-preloaded `input_macro_spec::<LLL>()` whose brief-form
composition stages the resolved content as the `Attached` body slot; the
parse-law checker scopes byte accounting per source through the slot roles; and
the line/col ownership design landed (`line_of`, `line_col_span`,
`LineIndexCache`, `LineColProvider`, `_with` render variants, scan cap
500 000).

### Signature table (new/changed public surface)

| Item | Signature / shape |
|---|---|
| `ParseContext::parse_attached_source` | `(&mut self, source: Arc<Source<L::SourceOrigin>>, state: Arc<ParsingState<L>>, parser: &mut P) -> ConstructParserResult<L, Vec<BuildId>>` where `P: ConstructParser<L, Output = NodesOutcome<L>> + ?Sized` (D-plan-1); fresh inner reader/context, same session/builder; local stray-close recovery (diagnose + consume + chars + re-invoke); `TokenCondition`/`NodeCondition` stops end the run (D-plan-3); traceback frame `Static("attached source")` at `triggered_at` (D-plan-2); pass-through delta discarded (D-plan-14) |
| `ParseContext::attach_source_reference` | `(&mut self, reference: &str, at: &SourceSpan<L::SourceOrigin>, state, parser) -> ConstructParserResult<L, Option<Vec<BuildId>>>` — method home (D-plan-4); `None` = diagnosed-and-recovered |
| `constructs::NoSourceResolver` | condition `{ reference: String }`, id `core.sources.no-resolver` (D-plan-5); derive-based, `PartialEq`/`Eq` |
| `constructs::UnresolvableSourceReference` | condition `{ reference: String, error: ResolveError }`, id `core.sources.unresolvable-reference`; message = the `ResolveError` rendering; no `PartialEq` (the error isn't) |
| `error::ToDiagnosticValue for ResolveError` | projection map `reference`/`message`/`cause-chain` (the `Error::source()` chain rendered; impl lives in error.rs — stratum direction) |
| `source::ResolveError` | `Clone`; `cause: Option<Arc<dyn Error + Send + Sync>>`; `with_cause` wraps with `Arc::new`; uniform-Clone principle in rustdoc |
| `Source::including_sources` | `(&self) -> IncludingSources<'_, O>` iterator over `&Source<O>`, self → primary |
| `source::check_include_chain` | `<O: SourceOrigin, K: PartialEq>(target_key: &K, triggered_at: &SourceSpan<O>, origin_key: impl Fn(&O) -> Option<K>, max_depth: Option<usize>) -> Result<(), ResolveError>` — origin-keyed incl. primary; `None` keys skipped; distinct cycle/depth messages; error reference = origin label or empty (D-plan-6) |
| `latexlike::InputMacroSpec<LLL = Latexlike>` | public spec type (D-plan-7); `CallableSpec` under bounds-where-used `ArgumentExt<LLL>: Default + SlotExt<LLL>: BodySlotExt`; one mandatory `{…}` argument named `"reference"` (D-plan-13) |
| `latexlike::input_macro_spec` | `<LLL: LatexlikeLang>() -> InputMacroSpec<LLL> where ArgumentExt<LLL>: Default` — never preloaded; no-caching discussion in rustdoc (T5 §G digest); state-transparent (D-plan-9) |
| `\input` staged shape | callable span = invocation in the includer (`stage_invocation(.., Some(end))`); slot `"attached"`, `SlotRole::Attached`, ext `BodySlotExt::make_body()` (D-plan-8); slot present iff a source was attached (empty file ⇒ empty slot) |
| parse-law checker | callable byte accounting partitioned by slot role: parent-source sequence contiguous across excluded regions; `Attached` regions own per-source accounting; `Hidden` regions none (D-plan-10) |
| `LineIndex::line_of` | `(&mut self, offset) -> Option<(usize, Range<usize>)>` — line number + terminator-free range (D-plan-12) |
| `LineIndex::line_col_span` | `(&mut self, impl Into<Range<usize>>) -> Option<((usize,usize),(usize,usize))>` — both ends or `None` |
| `source::LineIndexCache<O = Option<String>>` | persistent per-source cache (Arc-identity keyed, owned line-starts tables); mirrors `line_col`/`line_of`/`line_col_span` with `source` first; `new`/`Default`/`set_max_scan_len` (D-plan-11) |
| `source::LineColProvider<O = Option<String>>` | trait: `line_col(&mut self, source: &Arc<Source<O>>, offset) -> Option<(usize, usize)>`; implemented by `LineIndexCache` |
| render `_with` variants | `Diagnostic::render_with`, `ParseError::render_with`, `Diagnostics::render_all_with`, `format_position_with`, `format_traceback_with` — `&mut impl LineColProvider<O>` last; no-arg forms = transient-cache shorthand; internal `SourceIndexCache` replaced by `LineIndexCache` |
| `DEFAULT_MAX_SCAN_LEN` | 100 000 → 500 000 (private const; docs updated, silent-`None` warnings kept) |

### Acceptance-test outcomes

- **Multi-source reconstruction (T5 I-18)**: `latexlike::input` tests — per-source
  slices/spans (invocation span in the includer, body slice single-source in the
  attached source, root children rebuild the includer's bytes, body rebuilds the
  attached bytes), `Attached` slot shape (name/role/body marker), S5 payload-pin
  oracle (`check_latexlike_tree_invariants`) green on every multi-source tree,
  nested inclusion with walkable chains, empty-file slot, expression-fallback
  reference. PASS.
- **Include-chain policy**: self-include legal at the core level
  (`core_performs_no_recursion_checking_self_inclusion_is_legal`);
  `check_include_chain` unit tests (cycle incl. primary participation, depth
  overflow with distinct message, `None`-key skip); the resolver-side policy
  recipe test (`a_policy_resolver_turns_self_inclusion_into_a_diagnosed_cycle`).
  PASS.
- **Line/col**: fresh-vs-cached agreement at every offset, per-source isolation
  by Arc identity (distinct offset conventions), `line_of` boundaries
  (first/last line, trailing newline, empty source, end-of-content), scan-cap
  re-admission, `_with`-vs-shorthand render parity across repeated renders,
  fallback message beyond the raised cap. PASS.
- **Stray-close recovery**: engine-level (`a_stray_close_in_the_attached_source_
  recovers_locally` — tolerant continues, strict aborts, frame recorded) and
  preset-level (`a_stray_close_in_the_included_file_never_unwinds_the_includer`
  — the enclosing group closes at its own delimiter). PASS.

### Gate results (final full run)

- `cargo build` and `cargo build --tests`: 0 warnings, 0 errors.
- `cargo test`: 654 lib + 30 acceptance + 8 derive-conditions + 1 derive +
  28 doctests — all green (2 ignored doctests pre-existing; 614 → 654 lib,
  27 → 28 doctests: the `input_macro_spec` example).
- `rm -rf target/doc && cargo docs`: clean — no missing_docs, no broken links.
- Superseded-names sweep: clean — no `cx.parse_source` (the door is
  `parse_attached_source`), no `Language::resolve_source`/`with_resolver`
  shapes, no `techy::helpers`, no `LineIndexCacheProvider`, no
  `SourceIndexCache` residue, no `line_range(line_no)`, no `ancestors()`.
- Behavior changes only where ruled: the parse-law checker scoping (T5-F5
  rider), the scan-cap raise (T4-F6), the render internals (mechanism swap —
  output pinned byte-identical by the parity test).

### Commits

- 1e0197c P3-S6: implementation plan
- f5aab12 P3-S6 M1: ResolveError Clone (Arc cause) + including_sources +
  check_include_chain
- 14d36f9 P3-S6 M2: parse_attached_source door + attach_source_reference bundle
  + the two core.sources conditions
- ac08a03 P3-S6 M3: parse-law checker per-source byte accounting via slot roles
- 4e81e2f P3-S6 M4: latexlike input_macro_spec + multi-source I-18 acceptance
- 7571862 P3-S6 M5: line/col ownership package
- (+ this commit) P3-S6 M6: docs + records + closure

### Churn

Whole stage (4113aa8..HEAD, this commit included): 17 files, +2796/−150.
Code portion (techy/src): 11 files (2 new — constructs/attached_source.rs,
latexlike/input.rs), +2240/−129. Records/docs: DESIGN_RATIONALE.md (7 entries
touched), ARCHITECTURE.md (4 passages), CLAUDE.md (source facade line),
docs/ guide (2 pages), this report.

## M7 — the two S6 design-revision rulings (user, 2026-08-04)

Governing input: the PHASE3_PLAN stage-log entry "S6 DESIGN-REVISION RULINGS"
(2026-08-04). Ruling A supersedes D-plan-8 (the preset's Attached-body pairing);
Ruling B resolves D-plan-9 as a rulings amendment (`persist_state` via merged
after-effect deltas, amending T4-B2's bare `Vec<BuildId>` door return).

### Plan

**M7-a — core mechanics (Ruling B, engine half):**

1. `ParsingStateDelta::merge_from(&mut self, later)` (crate-private, mirroring
   the merge shape `lower_state_events` already open-codes): rules via
   `TokenRulesOverrides::merge_from` (later's `Some` fields win), `scope_ops`
   concatenated in application order, `mode`/`ext` last-writer-wins, `events`
   concatenated. Plus an emptiness probe for the `Option`-`None` spelling.
2. `ParseContext` capture seam: a crate-private sibling of `derive_state`
   (`derive_state_recording(delta, record: &mut Option<ParsingStateDelta<L>>)`)
   that lowers events exactly as `derive_state` does, commits the derivation,
   and — only when the transition commits — merges the **effective, as-applied
   delta** into `record`. `derive_state` keeps its public shape and shares the
   commit path.
3. `NodesOutcome<L>` gains `after_effects: Option<ParsingStateDelta<L>>` — the
   merged record of the sibling after-effect deltas the run applied (`None` =
   none were). `NodesParser` accumulates via the capture seam in
   `dispatch_invocation`, drains the field at every return (the re-invocation
   contract), and the "no current consumer of a merged delta" rustdoc note is
   rewritten as consumed.
4. The door returns an outcome bundle: `pub struct AttachedSourceOutcome<L>
   { nodes: Vec<BuildId>, after_effects: Option<ParsingStateDelta<L>> }` in
   `constructs/attached_source.rs`, merged across resumed runs;
   `attach_source_reference` returns `Option<AttachedSourceOutcome<L>>`.
   Exports through `techy::core::constructs`. D-plan-14's "discarded" note
   becomes "exported on the bundle; forwarding is the caller's choice".
5. Tests: door-level `after_effects` is `None` for a no-after-effect included
   run (persist test e); engine literal `NodesOutcome` constructions updated.

**M7-b — preset (Ruling A + Ruling B, spec half):**

6. `input_macro_spec::<LLL>(persist_state: bool, attached_slot_ext:
   SlotExt<LLL>) -> InputMacroSpec<LLL>` — both parameters mandatory, no
   defaults. The spec stores the ext value (its `Clone`/`Debug`/`Send`/`Sync`
   come from the `NodeExtTypes::SlotExt` bounds) and clones it per invocation
   into the `ParsedSlot`; the `SlotExt<LLL>: BodySlotExt` bounds drop
   (`make_body` is no longer called). Shipped registrations in tests/doctests
   pass `BodyMarker::not_body()`.
7. `persist_state == true`: the invocation parser returns the bundle's merged
   delta as its own after-effect through the existing sibling channel;
   `false`: returns `None` (today's transparent behavior). Rustdoc "# State
   handling" rewritten (transparent-vs-persisting choice, the
   preamble-defines-macros paradigm case, the no-caching interaction — T5-G's
   rationale STRONGER: the shipped spec itself can now feed state back).
8. Tests: shipped registration ⇒ `body()` is `None`, retrieval via the slot
   named `"attached"` (`slot_content_nodes_named`); one framework-choice test
   (body-marked ext ⇒ findable Attached-body slot — the T5 findability
   clause); persist tests (a) paradigm definition-then-use, (b) `false` leaves
   the includer untouched, (c) nested composition to the primary, (d) merge
   order (later field override wins; scope pushes in order), all driven by
   minimal test specs returning after-effect deltas (the engine
   `AfterEffectSpec` pattern; no shipped construct produces one).

**M7-c — records + gates:** DR applied notes ([§dd-dr:input-wiring] amendment
note "user-ruled 2026-08-04: outcome bundle + persist_state";
[§dd-dr:input-attachment]; the body-pairing sentences), ARCHITECTURE
attached-source passage, D-plan-8 marked SUPERSEDED-BY-RULING, D-plan-9/14
updated, new D-plans below, M7 closure tables, full gates.

### Escalation check (Ruling B's composition clause)

Per-component sequential composition under the ruled semantics: rules
overrides — exact (each `Some` field wholesale-replaces, so last-writer-wins
reproduces sequential application); scope ops — exact (concatenation in
application order); `mode`/`ext` — last-writer-wins as ruled (`ext` is
whole-value replacement; Latexlike's `finalize_transition` is not customized,
so the single-transition replay is exact for the shipped preset); events —
context-dependent events are lowered into field patches *before* the record is
taken (the capture point is the effective delta), so the record is event-free
in every shipped configuration. No component fails to compose — no escalation.
The one extension-case nuance (context-free events of custom Langs) is
D-plan-17 below, resolved toward exactness, not approximation.

### New deviations / delegated decisions (M7)

- **D-plan-15** (delegated: the bundle name): `AttachedSourceOutcome<L>`, the
  `parse_attached_source`/`NodesOutcome` sibling vocabulary; fields `nodes` +
  `after_effects`. The delta field is named **`after_effects`** on both the
  bundle and `NodesOutcome` — `state_delta` was rejected because it would sit
  beside `NodesOutcome::state` and read as "the entry→state diff", which the
  merged record deliberately is not (deltas, not diffing).
- **D-plan-16** (delegated: parameter naming/order):
  `input_macro_spec(persist_state: bool, attached_slot_ext: SlotExt<LLL>)` —
  the ruled `persist_state` name first (the ruling names it), the ext
  parameter named for the slot it lands on (the `"attached"` slot's ext).
- **D-plan-17** (realization: the record's exact capture semantics): the
  record accumulates the **effective delta as applied** — post event-lowering,
  merged only when the transition commits. Context-free events that survive
  lowering (none exist in any shipped Lang) are recorded concatenated in
  application order rather than dropped: by the `Lang::Event` contract they
  are position-independent ("consumed wherever the delta is applied"), so
  recording them preserves exact composition where dropping them would
  approximate — the ruling's "event-free record" premise holds verbatim for
  every shipped configuration. Failing scope ops stay in the record (the
  `DeriveError::delta` "as applied" notion; ScopeOpError carries no op index
  to strip by): a persisted replay may re-attempt and re-diagnose them at the
  includer — documented, and inherent to the ruled merged-delta mechanism.

### M7 closure

**What changed (code):**

- `state/delta.rs`: crate-private `ParsingStateDelta::merge_from` (sequential
  composition: rules last-writer-wins via `TokenRulesOverrides::merge_from`,
  scope ops + events concatenated, `mode`/`ext` last-writer-wins) and
  `is_empty` (the `None` spelling's probe).
- `constructs/mod.rs`: crate-private
  `ParseContext::derive_state_recording(delta, record)` — the capture seam:
  lowers events exactly like `derive_state` (shared `commit_derivation` tail),
  merges the effective as-applied delta into `record` only when the
  transition commits. `parse_nodes`'s resume-bridge rustdoc gains the
  propagating-bridge obligation (merge `after_effects` across runs).
- `constructs/nodes_parser.rs`: `NodesOutcome<L>` gains **`after_effects:
  Option<ParsingStateDelta<L>>`**; `NodesParser` accumulates through the
  capture seam in `dispatch_invocation` and drains the field at every return
  (re-invocation contract holds); the "no current consumer of a merged delta"
  note is consumed and rewritten.
- `constructs/attached_source.rs`: new public **`AttachedSourceOutcome<L> {
  nodes, after_effects }`** (manual Debug/Clone, the `NodesOutcome` pattern);
  `parse_attached_source` returns it (T4-B2 amendment per the ruling), merging
  `after_effects` across resumed runs; `attach_source_reference` returns
  `Option<AttachedSourceOutcome<L>>`. Exported via `techy::core::constructs`.
- `latexlike/input.rs`: `input_macro_spec::<LLL>(persist_state: bool,
  attached_slot_ext: SlotExt<LLL>)` — both mandatory; the spec stores the ext
  and clones it per invocation into the `ParsedSlot`; the
  `SlotExt<LLL>: BodySlotExt` bounds dropped from the spec and its invocation
  parser; `persist_state: true` returns the bundle's merged delta as the
  invocation's after-effect (existing sibling channel), `false` returns
  `None`. Rustdoc: new ext section (embedder decides body-ness; findability
  clause cited), "# State handling — `persist_state` decides" (paradigm case,
  nested composition), no-caching section strengthened (the shipped spec can
  now feed state back). Doctest updated to the two-parameter registration +
  slot-name retrieval.

**Signature-table rows (new/changed):**

| Item | Signature / shape |
|---|---|
| `constructs::AttachedSourceOutcome<L>` | `{ nodes: Vec<BuildId>, after_effects: Option<ParsingStateDelta<L>> }`; manual Debug/Clone (D-plan-15) |
| `ParseContext::parse_attached_source` | return `ConstructParserResult<L, AttachedSourceOutcome<L>>` (was `Vec<BuildId>`; user-ruled 2026-08-04) |
| `ParseContext::attach_source_reference` | return `ConstructParserResult<L, Option<AttachedSourceOutcome<L>>>` |
| `constructs::NodesOutcome` | new field `after_effects: Option<ParsingStateDelta<L>>` — merged effective as-applied sibling deltas, `None` = none (D-plan-17) |
| `latexlike::input_macro_spec` | `<LLL>(persist_state: bool, attached_slot_ext: SlotExt<LLL>) -> InputMacroSpec<LLL> where ArgumentExt<LLL>: Default` — the `BodySlotExt` bound gone (D-plan-16) |
| `\input` staged shape | slot `"attached"`/`Attached` unchanged; ext = the constructor value cloned per invocation; `body()` finds it only under a body-marked ext (framework choice) |

**Tests:** 660 lib (+6: door bundle-`None`; preset findability under a
body-marked ext; persist (a) definition-then-use, (b) transparent leaves the
includer untouched (single includer-side `UnresolvableCommand`, included-file
use still resolves), (c) nested composition to the primary, (d) merge order —
later `enable_comments` override wins, later scope push innermost). Persist
test (e) is the door-level bundle-`None` test
(`a_run_without_after_effects_bundles_none`). All prior S6 suites pass under
the not-body shipped registration (retrieval switched to
`slot_content_nodes_named("attached")`; `body()` pinned `None`).

**Records:** [§dd-dr:input-wiring] 2026-08-04 amendment note (outcome bundle +
persist_state + embedder-supplied ext); [§dd-dr:input-attachment] rider (the
"preset-configurable" case now concrete; no-caching stance strengthened);
[§dd-dr:slot-roles] needs no edit — it never claimed the preset pairing (its
findability clause is now exactly the shipped semantics); ARCHITECTURE
attached-source bullet rewritten; D-plan-8 SUPERSEDED-BY-RULING, D-plan-9
resolved as rulings amendment, D-plan-14 narrowed.

**Gates (full run at M7 close):** `cargo build` + `cargo build --tests` 0
warnings; `cargo test` all green — 660 lib + 30 acceptance + 8
derive-conditions + 1 derive + 28 doctests (2 pre-existing ignored);
`rm -rf target/doc && cargo docs` 0 warnings, links clean; superseded-names
sweep clean (incl. no `make_body` in the shipped spec path — only the
framework-choice test and the doc reference). No escalations: the composition
clause was checked per component (see "Escalation check" above) — nothing
fails sequential composition.

**M7 commits:** c921363 revision plan; fb8662c wip (machinery + bundle + spec
reshape compiling); 6b17598 preset persist_state + embedder-supplied slot ext,
tests (a)-(e) green; (+ this commit) records + docs + closure.
