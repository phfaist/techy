# Phase 7 Execution Plan

**Status: DRAFT for review — plan session, July 2026.**
Companion to ARCHITECTURE.md §9 Phase 7; where this document amends that one-line scope, this
document wins. Decision *rationales* from the plan session are recorded in DESIGN_RATIONALE.md
during subphase 7.0 — this file holds the working contract and the execution order.

---

## 1. Scope

ARCHITECTURE.md §9 planned Phase 7 as "the `latexlike` preset". The plan session reshaped it:
two core mechanisms were pulled *into* the phase ahead of the preset (both are prerequisites
for clean preset wiring), and the standard-library data port moved *out*.

**In scope:**

1. **`parsing_mode`** — mode as first-class parsing-state data (`L::ModeId`).
2. **`ParseDriver`** — the Lang-provided parse-behavior object: construct-parser provision,
   descent deltas, recovery policy; migration of parse-time hooks off `Lang`; `ParseContext`
   reshaped to a data struct + plumbing.
3. **Scope-stack redesign** — `SpecsProvider` / `Package` / `Scope` / `ScopeStack` replacing
   `Library`/`LibraryStack`; definition/stack delta ops; fallback-as-provider.
4. **`Language<L>` + `parse()`** — the runtime bundle and convenience entry (deferred from
   Phase 6; the preset + acceptance tests are the consumer that demonstrates the need).
5. **`latexlike` preset** (S2, `techy::latexlike`) — default token rules, math modes,
   environments via the `\begin`/`\end` composition, verbatim, the argument-code factory,
   a *minimal test-driven* spec set, `NodeRef` accessor sugar.
6. **Extraction/view API** (the R7 work package) — `descendants()`, named argument-node
   accessors, content-extraction helpers.
7. **Acceptance tests** — a ported slice of pylatexenc's walker test suite + tolerant-parsing
   behavior tests.

**Out of scope (deferred, recorded):**

- The **standard spec-database port** (pylatexenc `_defaultspecs.py` categories). Only the
  minimal set the ported tests reference is registered, test-side. The full port waits until
  the new scope stack has proven itself in use.
- **Parse-level `\newcommand`/`\newenvironment`** (the delta plumbing it needs ships in 7.3;
  the specs themselves wait for the std-library phase).
- **`^`/`_`/`#` sub/superscript specials** (std-library material; pylatexenc leaves them as
  chars too, so ported tests are unaffected).
- **`\global\def`-style global definitions** — needs upward propagation of definition ops
  through parser return channels; mechanism sketched in the rationale, not built. Interior
  mutability of scopes was considered and rejected (breaks state immutability and the
  reader-memoization contract).
- Argument codes **`e{…}`** (N3 — record shape unsettled) and **`AnyDelimited`** (N2);
  parsers N4–N6 (ParserLibraryParity.md).
- The **per-invocation-`Box` micro-benchmark** obligation (Phase6Execution §6.7) stays open;
  it may slot into 7.9 if convenient, otherwise remains unscheduled.

---

## 2. Design decisions from the plan session (July 2026)

Working contract; rationale entries land in DESIGN_RATIONALE.md in 7.0. All names are subject
to the usual naming review at the implementing subphase.

### D1 — `ParseDriver`: the Lang-provided parse-behavior object

- New defaulted-methods-only core trait `ParseDriver<L: Lang>`; bound into the Lang bundle as
  an associated type: `type Driver: ParseDriver<Self>` (SimpleLang defaults to a core
  `StdParseDriver`). `ParseContext` gains `pub driver: &'a L::Driver` — *concretely typed
  through `L`*, so preset parsers reach preset helper methods (e.g. a `LatexlikeDriver::
  load_package("amsmath")`) with zero downcasts, while generic code sees only the trait.
- **Placement doctrine** (record in DESIGN_RATIONALE): `Lang` keeps hooks belonging to layers
  callable outside or below a driven parse — `initial_state_data`, `finalize_transition`
  (state layer; `derived()` is out-of-parse-callable), `scan_specials`/`specials_trigger_chars`
  (tokenizer layer), `finalize_node` (builder/transform layer). **Everything that only runs
  while a parse is driven lives on the driver** (instance methods, `&self`, config-capable).
- **Full parse-behavior migration**: `resolve_command`, `make_paragraph_break_node`,
  `refine_diagnostic` (folds into the driver's recover path), `observe_transition` move from
  `Lang` to `ParseDriver`. Accepted asymmetry: Specials resolution stays Lang-side (token
  time), Command resolution is driver-side (parse time).
- **Construct provision**: `make_nodes_parser`, `make_group_parser`, and a
  `make_invocation_parser` interception (default = delegate to
  `invocation.spec.make_invocation_parser(...)`) on the driver. *Every* descent call site
  (NodesParser GroupOpen arm, GroupParser interior, EnvironmentBodyParser body, argument
  parsers, top-level drive) routes through `ParseContext` wrappers (`cx.parse_nodes(…)`,
  `cx.parse_group(…)`) so a customization applies uniformly.
- **Descent delta channel**: `driver.group_interior_delta(prev, rule) ->
  Option<ParsingStateDelta<L>>`, pure/deterministic per `(state, rule)`; merged into the
  memoized `session.group_interior_state` derivation (memo cache stays in session; hook runs
  on memo miss only). With D2, the math plug is one line: math group rules return
  `ParsingStateDelta::new().mode(MathInline)`.
- **Recovery policy moves to the driver** (off `ParserSession`, which stays pure
  scratch/output: builder, diagnostics, frames, memo, SessionExt). Trait default
  `recovery() -> Recovery` = Strict; `StdParseDriver { recovery }` carries the knob;
  overriding the driver's `recover` enables custom policies.
- **`ParseContext` helper split**: policy helpers (`recover`, `probe_token`) are *defined* on
  the driver with thin delegating sugar kept on cx; invariant-bearing plumbing
  (`parse_scoped`, `with_frame`, `implementation_error`) stays as non-overridable
  `ParseContext` methods.

### D2 — `parsing_mode`: mode as first-class state data

- `type ModeId: Copy + Eq + fmt::Debug + Send + Sync` on `Lang` (the established closed
  per-language vocabulary pattern, alongside `GroupTypeId`/`CallableTypeId`; SimpleLang
  default `()`), stored as a plain field `StateData.mode: L::ModeId`, settable via a new
  delta channel `ParsingStateDelta.mode: Option<L::ModeId>`.
- Mode is *not* lookup-specific: the scope stack reads it for package visibility, and the
  preset uses it for content interpretation broadly (text/math; verbatim/listing candidates).
- **The driver may *initiate* mode changes** (e.g. via `group_interior_delta`);
  **`Lang::finalize_transition` *interprets* them** (disable features, adjust rules —
  comparing `prev.mode()` with the incoming override). The consequence hook must NOT be on
  the driver: out-of-parse `derived()` has no driver, and states must be a pure function of
  base + delta (airtightness + memoization). Mode-shaped transitions no longer need
  `L::Event`; events remain for non-modal semantics.
- Preset consequence: `latexlike`'s `StateExt` likely needs no `in_math_mode` field — the
  mode field is the single source of truth.

### D3 — Scope stack: `SpecsProvider` / `Package` / `Scope`

Replaces `Library`/`LibraryStack` (module `library/` reworked in place). Entries are
**all-`dyn`**: the stack is a `Vec<Arc<dyn SpecsProvider<L>>>`, searched innermost-first.

```rust
pub trait SpecsProvider<L: Lang>: fmt::Debug + Send + Sync {
    fn name(&self) -> &str;
    fn retrieve_spec(&self, query: &CallableQuery<'_, '_, L>, state: &ParsingState<L>)
        -> Result<Option<Arc<dyn CallableSpec<L>>>, ProviderError>;  // Ok(None) = not here
    fn scan_specials<'s>(&self, state: &ParsingState<L>, content: &'s str, pos: usize)
        -> Result<Option<SpecialsMatch<'s, L>>, ProviderError>;      // default Ok(None)
    fn specials_trigger_chars(&self) -> TriggerChars;                // default none; unioned at freeze
    fn with_definitions(&self, ops: &[DefinitionOp<L>])
        -> Result<Arc<dyn SpecsProvider<L>>, ProviderError>;         // default Err(not mutable)
    fn iter_symbols(&self) -> Option<Box<dyn Iterator<Item = SymbolEntry<L>> + '_>>; // default None
}
```

- **Standard impls**: `Package` — immutable, built once, hashbrown map, loaded wholesale
  (preset drivers expose helpers like `load_package(name) -> Arc<Package>`, called by preset
  parsers when *building* deltas — the state choke point never needs the driver); mode
  **visibility is a `Package` field** checked inside its own `retrieve_spec` (the stack is
  visibility-blind). `Scope` — definition target, BTreeMap, implements `with_definitions`;
  functional `with_…` returns a fresh provider (`Arc::make_mut` is unavailable on `dyn` —
  this IS the copy-on-write mechanism). Scoped reversion stays structural (outer states hold
  the old Arcs); **scopes are created lazily on first `Define`**, not eagerly per group —
  identical semantics, zero churn.
- **No `Masked` outcome.** "Defined to be an error" is expressed as an ordinary definition:
  an `ErrorCallableSpec` (core-provided utility) whose invocation parser emits a diagnostic.
  Shadowing with it suppresses lower entries and the fallback purely by search order.
  `Remove` ops delete entries from `Scope`s only; "removing" a `Package` definition =
  shadow-with-error-spec or `Unload`.
- **Fallbacks live in the stack**: an ordinary bottom provider answering any name of its
  registered callable types (standard impl provided). `ScopeStack` itself carries **no**
  fallback map and **no longer implements `SpecsProvider`** (stacks don't nest — kills the
  Phase 4 nested-fallback-preemption hazard instead of re-mitigating it).
- **Resolution failure is structured**: exhausting the stack returns a miss carrying the
  searched provider names (→ the `UnresolvableCommand` "searched: …" detail); a provider
  `Err` aborts the fold and propagates.
- **Delta ops** replace `push_libraries`: stack ops (`Push`/`Load`, `Unload{name}`,
  `Replace{name, provider}`, `ReplaceStack`) + definition ops routed to a named provider's
  `with_definitions` (`Define{scope, callable_type, name, spec}`, `Remove{…}`). Ops carry
  `Arc`s directly (no name→package registry in core; a `Load("amsmath")`-by-name delta can
  come later via driver helpers).
- `Lang::scan_specials` stays the tokenizer hook; the preset's impl folds over the stack via
  a core `ScopeStack::scan_specials` helper; `Lang::specials_trigger_chars` = union over
  providers, cached at state freeze as today.

### D4 — `EnvironmentSpec` body customization

Defaulted `make_body_parser()` method (pylatexenc's `make_body_parser` precedent): default
drives the core `EnvironmentBodyParser` with the spec's body state delta applied; `verbatim`
overrides it. Since `Any`-downcast reaches concrete types only, the preset uses the sanctioned
funnel pattern (DESIGN_RATIONALE §3.4): a concrete wrapper spec type holding
`Arc<dyn …Behavior>` with the defaulted method on the inner trait.

### D5 — Earlier session decisions this plan builds on

The `\begin` composition is preset-owned over core building blocks (`read_rigid_name_group`,
`parse_declared_arguments`, `EnvironmentBodyParser`) — decided pre-session, rehearsed
test-side in `environment_parser.rs`. Verbatim follows the pinned recipe (features-off derived
state + `expecting_group_close` terminator rule; modern-pylatexenc node shapes: group +
chars). Environment invocation-level takeover stays closed; per-environment variation flows
through the spec surface.

---

## 3. Subphases

Each subphase ends `cargo build && cargo test` green (workspace: `techy` + `techy-derive`),
with new machinery tested and DESIGN_RATIONALE.md updated where a decision was made in flight.
Subphases marked **[checkpoint]** contain design points needing user sign-off before or during
implementation — ask, don't assume.

### 7.0 — Docs & setup
- Record the plan-session decisions (D1–D4 + the deluxe-model rationale, rejected
  alternatives: skeletal/minimal, Masked, closed entry enum, eager scopes, interior-mutable
  scopes) in DESIGN_RATIONALE.md; update ARCHITECTURE.md §9 Phase 7 scope line + §specs/§state
  sketches only where now misleading (keep it terse per the doc-hygiene rule); update
  NAMING_STRATEGY.md deltas (`ParseDriver`, `SpecsProvider`, `Package`/`Scope`/`ScopeStack`,
  `ModeId`, `parsing_mode`); update CLAUDE.md pointer (Phase6Execution → Phase7Execution).
- ParserLibraryParity.md: mark N1 (math plug) as resolved-by-design (D1+D2), note scope cuts.

### 7.1 — `parsing_mode` (state core)
- `Lang::ModeId` + `StateData.mode` + `ParsingStateDelta.mode` + builder sugar `.mode(…)`;
  SimpleLang default; `finalize_transition` docs updated (interprets mode changes);
  `dbg!`-visibility; tests: mode transitions, finalize seeing prev/new mode, memo behavior.
- Small and independent; lands first so 7.2/7.3 can use it.

### 7.2 — `ParseDriver` (constructs/engine core) **[checkpoint: naming pass on the trait surface]**
- Trait + `StdParseDriver` + `Lang::Driver` + `ParseContext.driver`; hook migration off
  `Lang` (D1 list) incl. `Recovery` off `ParserSession`; cx helper split; descent wrappers
  `cx.parse_nodes`/`cx.parse_group` + factory routing at every descent site;
  `group_interior_delta` merged into `session.group_interior_state`.
- Mechanical but broad: every Phase 6 test that builds a `ParseContext`/`ParserSession`
  updates. Test langs grow trivial drivers via SimpleLang defaults where possible.
- Tests: default-driver equivalence (all Phase 6 suites still green), a custom driver
  exercising each factory + `group_interior_delta` (mode-entering group), recovery-policy
  override, preset-helper-method access pattern.

### 7.3 — Scope stack (`library/` rework) **[checkpoint: derived() fallibility; specials fold rule; ProviderError/miss shapes]**
- `SpecsProvider` trait + `Package` + `Scope` + fallback provider + `ErrorCallableSpec`;
  `ScopeStack` rework (no nesting, structured miss); `DefinitionOp` + stack ops in
  `ParsingStateDelta` (replacing `push_libraries`); consumers migrated (`resolve_command`
  impls, test-side `\begin` composition, `scan_specials` fold helper + trigger-char union).
- **Checkpoint items**: (a) `derived()` return type once ops can fail — lean: `Result`, with
  the session seam mapping failures to diagnostics; decide error taxonomy (embedder input vs
  implementation error). (b) Specials fold rule — lean: longest match wins, ties innermost
  (pylatexenc `---`-beats-`--` parity). (c) `ProviderError` / miss-report shapes.
  (d) `iter_symbols` item type (or defer introspection to the view-API subphase).
- Tests: shadowing, visibility-by-mode, CoW scope semantics across group nesting, fallback
  ordering, error-spec shadowing suppressing fallback, provider errors, delta ops incl.
  failure paths, specials via providers (multi-provider longest-match).

### 7.4 — `Language<L>` + `parse()` (engine)
- The runtime bundle: seeds the initial `Arc<ParsingState>` (default `TokenRules`, scope
  stack, mode, ext) — reconciled with `Lang::initial_state_data()` (Language defaults win;
  the Lang hook remains the no-`Language` path); owns the driver instance; `SourceResolver`
  wiring (per DESIGN_RATIONALE §3.3); `session()` and `parse(input) -> ParseResult` driving
  reader → NodesParser → root list → `finish()`.
- Ownership per ARCHITECTURE §engine: `Language` long-lived; `ParserSession<'env>` borrows;
  `ParseResult` owns tree + diagnostics.
- Tests: define-once-parse-many, resolver round trip, strict vs tolerant via driver config.

### 7.5 — `latexlike` preset core **[checkpoint: preset naming batch — type ids, mode ids, driver name, module items]**
- Module `techy::latexlike` (S2): `Latexlike` ZST; `GroupTypeId`/`CallableTypeId`/`ModeId`
  enums (Brace/Bracket/MathInline/MathDisplay/…; Macro/Environment/Specials; Text/Math…);
  `LatexlikeDriver`; default `TokenRules` (`\` escape, `{}`, `[]` option groups, `$ $$ \( \[`
  math groups with `expecting_group_close` disambiguation, `%` comments) — modeled on the
  `latex_rules()` test helper; math via `group_interior_delta` → mode delta →
  `finalize_transition`; `resolve_command` via the scope stack; specials scan via providers
  (`~`, `&`, ligature specials as data); minimal spec set registered test-side only.
- `NodeRef` preset sugar: `as_math()`-style views, environment/macro accessors over
  `Callable` nodes.
- Tests: tokenization defaults, math-mode entry/exit incl. `$a$$b$` vs `$$…$$`, mode-visible
  packages, comment/paragraph behavior.

### 7.6 — Environments
- Preset spec types via the funnel pattern: `MacroSpec`/`EnvironmentSpec`/`SpecialsSpec`
  constructor helpers producing `StdCallableSpec`-or-wrapper specs; `EnvironmentSpec` with
  body state delta field + defaulted `make_body_parser()` (D4); `BeginSpec` dispatcher +
  `end` spec for orphan-`\end` diagnostics — promoting the test-side composition
  (`environment_parser.rs`) into the preset; starred names as separate spec entries.
- Tests: adapt the §G environment matrix to the real preset; recovery matrix re-verified
  under `Latexlike`; `check_tree_invariants` throughout.

### 7.7 — Verbatim + argument-code factory
- Verbatim per the pinned recipe: `\verb` delimited parser (auto-matched delimiter, depth
  counter), `verbatim` environment via `make_body_parser` override (gobbles the single
  newline after `\begin{verbatim}`), producing group+chars shapes; unknown-verbatim-EOF
  recovery.
- The argument-code factory (N8): a preset constructor function, code string in →
  `Vec<Arc<ArgumentSpec>>` out, eager, `Err` on malformed codes. Codes now: `m`/`{`, `o`/`[`,
  `s`/`*`, `t<c>`, `r<c1><c2>`, `d<c1><c2>` (via `temporary_groups`), `v`/`v<c1><c2>`
  (delimited verbatim). Deferred: `e{…}`, `AnyDelimited`.
- Tests: port slices of pylatexenc's `test_latexnodes_parsers_verbatim.py` and
  `test_latexnodes_parsers_stdarg.py` (minus deferred codes).

### 7.8 — Extraction/view API (R7) **[checkpoint: API shape mini-session before coding]**
- Document-order `descendants()`; named argument accessors (`argument_nodes_named` family);
  content-extraction helpers incl. the `split_at_chars`-style comma-list helper (N5);
  whatever `iter_symbols` surface was deferred from 7.3.
- Shape needs a short design session (it's the pylatexenc-style argument-access package;
  cross-tree id/remapping questions from DESIGN_RATIONALE §3.5 lurk here).

### 7.9 — Acceptance suite
- Port the selected `test_2_latexwalker.py` slice, adapted to the minimal spec set and techy
  node shapes: `test_get_latex_nodes`, `…_comments`, `…_environment`,
  `…_maybe_optional_arg`, `…_mathmodes` (+ `dollardollar` token boundaries),
  `test_verbatim`/`test_lstlisting_*` (modern shapes), `test_errors`,
  `test_error_dangling_missing_args_*`, `test_invalid_environment_macros_*`,
  `test_bug_issueno49`/`57`. Skipped with reasons: `test_parsing_state_changes`
  (\newcommand — deferred), anything needing the full std DB.
- Tolerant-parsing behavior tests (strict vs tolerant matrices over the suite);
  `check_tree_invariants` on every parse; final docs pass (guide stubs get preset examples
  if cheap); optionally the §6.7 per-invocation-Box micro-benchmark.

---

## 4. Verification

- Per subphase: `cargo build && cargo test` green across the workspace; new public items
  doc-commented; `no_std` discipline holds (`cargo build` with default features; no `std`
  imports outside tests).
- Phase exit: the 7.9 acceptance suite green in strict AND tolerant modes;
  `check_tree_invariants` clean on every acceptance parse; `cargo doc` builds without
  warnings; DESIGN_RATIONALE.md carries an entry per checkpoint decision; ParserLibraryParity
  table statuses updated.

## 5. Progress

| Subphase | Status | Notes |
|---|---|---|
| 7.0 docs & setup | — | |
| 7.1 parsing_mode | — | |
| 7.2 ParseDriver | — | |
| 7.3 scope stack | — | |
| 7.4 Language + parse() | — | |
| 7.5 preset core | — | |
| 7.6 environments | — | |
| 7.7 verbatim + arg codes | — | |
| 7.8 view API | — | |
| 7.9 acceptance suite | — | |
