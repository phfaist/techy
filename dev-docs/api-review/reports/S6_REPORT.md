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
- [ ] M2 — the door + the bundle + the two conditions
- [ ] M3 — parse-law checker `Attached`-scoping
- [ ] M4 — preset `input_macro_spec` + multi-source acceptance (I-18)
- [ ] M5 — line/col ownership (`line_of`, `line_col_span`, `LineIndexCache`,
      `LineColProvider`, `_with` variants, scan-len raise)
- [ ] M6 — docs + records + closure (full gate run)

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
- **D-plan-8** (realization): the attached slot is named `"attached"` (the
  door/bundle/role family vocabulary) and its ext is minted via
  `BodySlotExt::make_body()` — the slot is the node's body on the ext axis
  (the only generic slot-ext mint; the T5 slot-roles amendment sanctions the
  Attached-body pairing explicitly), `role: SlotRole::Attached` on the role
  axis.
- **D-plan-9** (consequence of the ruled door shape): the shipped
  `input_macro_spec` is **state-transparent** — included content's after-effect
  deltas do not continue into the includer (the door returns nodes only; a
  delta cannot be reconstructed from an exit state). Documented loudly;
  state-propagating `\input` is custom-composition work (T5-G's
  "preset-configurable" reading).
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
