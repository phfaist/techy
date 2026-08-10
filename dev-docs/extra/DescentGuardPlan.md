# Descent guard — working plan & progress log

*Working scaffolding for the descent-guard effort (2026-08-10). Removed when the work
lands (precedent: the langfeatures-plan scaffolding). Companion analysis:
`TechyParsingStackDepth.md`. All decisions below are user-ruled in the design session
of 2026-08-10 unless marked (assumed).*

## Ruled decisions (summary)

1. **`parse_construct()` is the single MUST entry point for all sub-parsing** (the
   pylatexenc `walker.parse_content` analog). Contract is normative: a
   `ConstructParser` runs only through it. Plain-Rust recursion bypassing it is
   undetectable by design; documented as such.
2. `state: None` means **clone the current state** — identical swap/restore scoping
   as `Some(Arc::clone(&cx.state))`; never "skip scoping" (caller-decides law +
   uniform enclosing-state stack).
3. `parse_nodes`/`parse_group` stay as **one-line delegates** (factory +
   `parse_construct` fused = the uniform-routing contract); their rustdoc must open
   with a prominent thin-wrapper statement. `parse_scoped` is **removed** (goes into
   [§dd-dr:superseded-names]).
4. **`DescentGuard` guard type is a generic on the driver** (`ParseDriver` gains only
   `type DescentGuard: DescentGuard;`); the **init value lives on `Language`**
   (`with_descent_guard_init`), mirroring seed-state placement. Session hosts the
   per-parse instance; lazy `Default` fallback + public install seam for the
   hand-built-`ParseContext` path. Consumers needing swappable drivers per Lang:
   multiple Lang markers or a multiplexer driver type ([§dd-dr:parse-driver] revisit
   clause).
5. Guard refusal = **`DescentLimitExceeded`, aborts under any recovery policy**
   (distinct condition, not `ImplementationError`). No tolerant fallback in v1.
6. **Default budget: `DEFAULT_STACK_BUDGET = 250 * 1024` in all builds** —
   deliberately tight in debug (~10–14 levels) as the nudge to configure explicitly.
7. **Headroom is library-owned, applied internally by the guard for
   `ComputedStackBudget` only**: budget = probe() − `HEADROOM`.
   `HEADROOM = 64 * 1024` (recommended and executed; user ruling pending only if
   they object — flag in report). Fixed budgets get **no** headroom subtraction
   (they are consumption caps, not physical-stack measurements).
8. **50 % early warning**: at the first descent crossing 50 % of the budget, **only
   when running on the unconfigured built-in default**, latched once per parse —
   condition `DescentLimitApproaching`, warning severity, emitted immediately (not
   at parse end).
9. Delta boxing: `ConstructParser::parse` returns
   `(Self::Output, Option<Box<ParsingStateDelta<L>>>)`;
   `NodesOutcome::after_effects: Option<Box<…>>`.
10. Deferred (not in this effort): compile-time witness parameter on
    `ConstructParser::parse`; tolerant per-site fallbacks; consumer-traversal
    (`walk`/`recompose`) guards — the docs note the parse-side budget does not bound
    hand-built trees or traversals on smaller threads.

## Names (checked against [§dd-arch:naming])

Trait `DescentGuard`; refusal `DescentRefusal { detail: String }`; warning
`DescentWarning { detail: String }`; std impl `StdDescentGuard`; its init enum
`StdDescentGuardInit` with variants `FixedStackBudget { bytes }`,
`ComputedStackBudget { probe: fn() -> Option<usize> }`, `DepthLimit { levels }`,
`Off`; conditions `DescentLimitExceeded` (id
`core.constructs.descent-limit-exceeded`) and `DescentLimitApproaching` (id
`core.constructs.descent-limit-approaching`); constants
`StdDescentGuard::DEFAULT_STACK_BUDGET`, `StdDescentGuard::HEADROOM`. Entry point
`ParseContext::parse_construct`. "Stack"-only names rejected (collide with the
crate's data-stack vocabulary); "descent" is established public vocabulary.

## Part 1 — `parse_construct()` consolidation  [branch: descent-1-funnel]

Pure refactor, zero behavior change.

New non-overridable method on `ParseContext` (`techy/src/constructs/mod.rs`, taking
over `parse_scoped`'s doctrine seat per [§dd-dr:parse-driver]):

```rust
pub fn parse_construct<P>(
    &mut self,
    parser: &mut P,
    state: Option<Arc<ParsingState<L>>>,  // None = Arc::clone(&self.state); same scoping either way
    frame: Option<Frame<L>>,              // pushed around the whole descent
) -> ConstructParserResult<L, (P::Output, Option<ParsingStateDelta<L>>)>
where
    P: ConstructParser<L> + ?Sized,
```

Body order: resolve state → push frame (if `Some`) → `// descent-guard slot (Part 2)`
→ `with_parsing_state(state, |cx| parser.parse(cx))` → pop frame (both paths; errors
are values, not unwinds). Rustdoc carries the MUST contract, the `None` semantics,
frame semantics, and the pylatexenc analogy.

- Remove `parse_scoped`; migrate every caller.
- `parse_nodes(state, stop, child_states)`: body becomes factory +
  `parse_construct(&mut *parser, Some(state), None)`. Signature unchanged.
- `parse_group`: gains `frame: Option<Frame<L>>` parameter; delegates likewise. The
  hand-composed `with_frame(frame, |cx| cx.parse_scoped(…))` at
  `techy/src/constructs/nodes_parser.rs:700` collapses into `cx.parse_group(…,
  Some(frame))`.
- Call sites to migrate: `constructs/mod.rs` (parse_nodes/parse_group bodies),
  `constructs/attached_source.rs:167`, `constructs/environment_parser.rs:1006`,
  `latexlike/environments.rs:746`, `nodes_parser.rs:700`; sweep tests using
  `parse_scoped`.
- `with_parsing_state` / `with_derived_state`: docs updated — state-scoping
  utilities, **not** descent points.
- Prominent thin-wrapper rustdoc on `parse_nodes`/`parse_group` (open with it).
- Guide docs: `docs/construct-parsers.md`, `docs/ai-guide-custom-lang.md` (grep for
  `parse_scoped` / reentrant `parse_nodes` prose; add the MUST rule).
- New tests: `None` ≡ `Some(clone)` observably identical (state restored;
  enclosing-state stack depth identical via a probing parser); a condition recorded
  under a `parse_construct` frame carries that frame in its snapshot.

Gate: `cargo build` + `cargo test` + `cargo docs` green; existing tests unmodified
except mechanical call-site renames.

## Part 2 — `DescentGuard`  [branch: descent-2-guard, off descent-1-funnel]

- New `techy/src/engine/descent_guard.rs`:
  ```rust
  pub trait DescentGuard: Sized {
      type InitArg: Default + Send + Sync;
      fn init(arg: &Self::InitArg) -> Self;   // runs at parse entry on the parse thread
      fn try_enter(&mut self) -> Result<Option<DescentWarning>, DescentRefusal>;
      fn exit(&mut self);
  }
  ```
  `StdDescentGuard` per the names/decisions above. Measured mode: anchor = address
  of a local, captured at `init`; consumption = `anchor.abs_diff(current)`;
  `ComputedStackBudget` resolves budget at init (probe() − HEADROOM; `None` →
  `DEFAULT_STACK_BUDGET`). Default-constructed `StdDescentGuardInit` carries an
  `unconfigured` mark (private field + constructors) driving the 50 % warning and
  the self-describing refusal text. Rustdoc: copy-paste `ComputedStackBudget`
  recipe (`stacker::remaining_stack`; note the `pthread_getattr_np` /
  `GetCurrentThreadStackLimits` DIY equivalents); document `HEADROOM`'s role and
  the Fixed/Computed asymmetry. Pure `core` (crate is `no_std`).
- `ParseDriver` (engine/driver.rs): add `type DescentGuard: DescentGuard;` — the
  only trait addition (custom drivers add exactly one line).
- `StdParseDriver<R, O, G = StdDescentGuard>` via `PhantomData<fn() -> G>`;
  `type DescentGuard = G`. `LatexlikeDriver`: `type DescentGuard = StdDescentGuard`.
  Test drivers in engine/language/nodes tests: one line each.
- `Language`: field `descent_guard_init: <DriverGuard<L> as DescentGuard>::InitArg`
  (default in `new`), builder `with_descent_guard_init(...)`; `parse_source`
  installs `G::init(&self.descent_guard_init)` into the fresh session (eager —
  anchor at true parse entry).
- `ParserSession`: `descent_guard: Option<DriverGuard<L>>` slot; public install
  seam (`install_descent_guard`); `pub(crate) enter_descent(&mut self)` →
  `get_or_insert_with(|| G::init(&Default::default())).try_enter()`; `exit_descent`.
- Hook in `parse_construct` (the Part 1 slot): after frame push, `try_enter`;
  `Ok(Some(w))` → record warning-severity `DescentLimitApproaching` diagnostic at
  the current position; `Err(refusal)` → `DescentLimitExceeded` ParseError (abort
  under any policy) with live traceback incl. the just-pushed frame, frame popped
  before return; `exit` after the closure on both paths.
- Audit existing tests for nesting deeper than ~10 (would trip the tight default in
  debug): configure those `Language`s explicitly (`DepthLimit` / larger
  `FixedStackBudget` / `Off`).
- Tests: depth-mode refusal at N+1 + rebalance; tiny fixed budget turns deep `{{…}}`
  into `Err` (`DescentLimitExceeded`), process alive; default-init refusal detail
  names the default + `with_descent_guard_init`; 50 % warning latches once, only
  under default init; `ComputedStackBudget` with a std-side probe verifies headroom
  subtraction; self-including source via a test resolver hits the guard (shared
  session); `Off` passes deep input; one-line custom-driver compile check.

Gate: build + test + docs green, incl. `no_std`-compat check (no `std` imports).

## Part 3 — Box the pass-through deltas  [branch: descent-3-boxing]

Mechanical sweep (report measured 17 files, 9 hand-fixes):
`ConstructParser::parse` pair → `Option<Box<ParsingStateDelta<L>>>`;
`NodesOutcome::after_effects` → `Option<Box<…>>`; `derive_state_recording`'s
`record` param; `parse_construct`/`parse_nodes`/`parse_group` signatures;
`parse_attached_source`/`persist_state` merge plumbing; test-site `Box::new`s.
Gate: build + test + docs.

## Part 4 — Rationale & bookkeeping  [branch: descent-4-docs]

- New DESIGN_RATIONALE entry (label suggestion: `[§dd-dr:descent-guard]`): funnel
  MUST contract + `None` semantics; guard type-on-driver / value-on-Language split;
  uniform tight default as ruled; HEADROOM ownership + Fixed/Computed asymmetry;
  50 % warning semantics; abort-under-any-policy; boxing; rejected alternatives
  (depth-limit-as-mechanism, dyn guard, `Language::parse` parameter, unconditional
  warning, stacker, trampolining, callback-owned headroom). ARCHITECTURE.md
  reference (grep discipline). Amendment notes on [§dd-dr:parse-driver] and
  [§dd-dr:parsers-engine]. `parse_scoped` → [§dd-dr:superseded-names].
- Pointer from `TechyParsingStackDepth.md` §7 to the entry.
- api-baseline update (fold in the pending lang-features one).
- api-baseline note: StdParseDriver gained a private PhantomData field —
  struct-literal construction is no longer possible downstream; intended path is
  new() + builders (review finding N1).
- Docs-clarity pass per project rules (no jargon in user-facing docs).
- Remove this scaffolding file.

Gate: full build/test/docs/semver run; then rebase-check onto latest main; leave
branch chain for ff-merge (merges not run from the primary checkout).

## Progress log

- [x] 2026-08-10 worktree `.worktrees/descent` created; branch `descent-1-funnel` @ df1d17a; plan committed.
- [x] Part 1 implemented (agent) — gates green
- 2026-08-10 Part 1 landed on `descent-1-funnel`: 331e28b (parse_construct entry
  point), de757c8 (call sites migrated, parse_scoped removed), 7bf0e3f (guide docs +
  funnel tests). build/test/docs green (761 lib tests). Note: the
  `nodes_parser.rs:700` invocation-dispatch site collapses into
  `parse_construct(parser, Some(base), Some(frame))` — not `parse_group` as the
  Part 1 text says (the parser there is the invocation parser, not a group parser);
  no in-crate caller passes `parse_group`'s new `frame` yet. Stale `parse_scoped`
  mentions in ARCHITECTURE/DESIGN_RATIONALE await Part 4.
- [x] Part 1 reviewed (agent) — findings resolved
- 2026-08-10 F1 from Part 1 review: the two remaining direct
  `ConstructParser::parse` dispatch sites under a hand-pushed frame — the
  expression-position invocation dispatch (`argument_parsers.rs`) and the tack-on
  field dispatch (`tack_on_parser.rs`) — migrated to
  `parse_construct(&mut *parser, None, Some(frame))` under ruled decision #1.
  Accepted behavior delta (user-vetoable): each site now also pushes an
  enclosing-state stack entry for the descent's duration — an `Arc`-identical
  duplicate of the current state, harmless per `ParsingStateStack`'s documented
  scan semantics; it restores the documented "same descent points as the frame
  stack" symmetry these sites were missing.
- [x] Part 2 implemented — gates green
- 2026-08-10 Part 2 landed on `descent-2-guard`: 5fb7f63 (pre-step: the two
  remaining direct dispatch sites migrated into `parse_construct` — F1 from the
  Part 1 review, ruled decision #1, accepted enclosing-state-entry delta noted
  above), c0ae04f (`DescentGuard` trait + `StdDescentGuard` module + facade
  export), 244be55 (driver type choice + `Language` init + session slot +
  `parse_construct` hook + the two conditions; test-suite audit: no test nests
  deeper than ~10 syntactic levels, but seven shared `Language` helpers/sites
  (acceptance.rs ×3, chars_group_parser.rs, latexlike/input.rs, latexlike/mod.rs,
  recompose_oracle.rs) tripped the unconfigured default's half-budget warning in
  debug and now configure `depth_limit(64)` explicitly), 6ade982 (end-to-end refusal tests,
  F1 expression-chain + tack-on-chain coverage, self-include guard test).
  build/test/docs green, zero warnings (776 lib tests); no `std::` imports in
  src (the crate stays core+alloc). Implementation note for review:
  `StdDescentGuardInit`'s ruled variant names live on a private mode enum
  behind snake_case constructors (`fixed_stack_budget`/`computed_stack_budget`/
  `depth_limit`/`off`) — Rust enum variants cannot carry the required private
  `unconfigured` mark, so construction is constructor-only. HEADROOM executed
  as ruled (64 KiB, Computed-only); flag to the user per decision #7.
- [ ] Part 2 reviewed — findings resolved
- [ ] Part 3 implemented — gates green
- [ ] Part 3 reviewed — findings resolved
- [ ] Part 4 docs/rationale — gates green
- [ ] Final: rebase onto main, chain ready for ff-merge; scaffolding removed
