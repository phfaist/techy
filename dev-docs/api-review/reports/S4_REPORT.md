# Phase 3 — S4: Preset generalization + state-stack events — stage report

Branch `phase3-s4-preset-generalization` (worktree; fast-forwarded to `api-review`
tip b311770 — the worktree had been cut from a stale commit). Implementer report per
the relay discipline: plan first, one commit minimum per milestone, Progress updated
each.

## Progress

- [x] M0 — implementation plan committed (this file)
- [x] M1 — Math group form (core + preset): `GroupType::Math(MathGroupForm)`,
      `math_form()` sugar, `MATH_DELIMITERS` dissolved, `MathStyle`/`math_style`
      deleted (build/tests green; grep gate clean)
- [x] M2 — Role traits + `LatexlikeLang` umbrella (+ preset `Event` enum,
      `default_token_rules::<LLL>`, generic NodeRef sugar) — build/tests green,
      0 warnings; note: a few rustdoc links reference M3/M4 items
      (`resolve_state_event`, `derive_state`, `exit_math_context_delta`) and
      resolve once those milestones land (docs gate runs at M5)
- [x] M3 — E4 core machinery: session state stack, `ParsingStateStack`, fallible
      `finalize_transition` (+ `FinalizeError` folded into `DeriveError`),
      `resolve_state_event` hook, `cx.derive_state` / `with_derived_state` /
      `with_parsing_state` — build/tests green (588 lib tests), 0 warnings
- [x] M4 — Preset pillars + `LatexlikeDriver<LLL>` + exit-math wiring + parity
      tests — build/tests green (596 lib tests), 0 warnings
- [x] M5 — Docs + closure: guide `\text` recipe fix, DR status lines,
      superseded-names, ARCHITECTURE passages, full gate run, stage summary
      (all gates green — see Consolidated stage summary)

## Ruling inputs (digest)

- PHASE3_PLAN.md §Protocol + §S4; PLAN.md decision log 2026-07-30 (P3 RULED).
- DESIGN_RATIONALE: [§dd-dr:latexlike-generalization] (+ T3/T5/recompose
  amendments), [§dd-dr:math-group-form], [§dd-dr:preset-driver-pillars] (+ T4
  Copy/Eq strike + T5 amendments), [§dd-dr:enclosing-state-stack] (+ T3/T5
  amendments), [§dd-dr:group-taxonomy] amendment, [§dd-dr:math-no-nesting],
  [§dd-dr:argument-factory-additions], [§dd-dr:superseded-names].
- T1T2_RULINGS §E4 in full (five-iteration history + ruled restore wiring).
- T3_RULINGS §D (pillar inventory/layering; `exit_math_context_delta` amendment),
  §E (accessor names; mode trait trimmed; ClosedVocabulary NOT a supertrait).
- T5_RULINGS §C1 (`LatexlikeEvent`), §E (owning `ParsingStateStack`, constructors,
  scan semantics, signature ripple, two-component-recipe doc, paragraph-break
  parse-side-only doc), §D (no new driver knobs; S2 comment rewrite verify-only).

Out of scope, per the task's hard constraints (routing recorded):
- Fifth role trait `LatexlikeInvocationSyntax` → S5. Spec types
  (`MacroSpec`/`EnvironmentSpec`/environments machinery) stay monomorphic → S5.
- A1(iv) escape-char parse-init check function → **skipped entirely; routed to S9**
  (per the stage instructions; PHASE3_PLAN's S4 bullet mentions it, the launch
  instructions override).
- Base-package contents / `"_builtin"` rename / minidefs → S9. `input_macro_spec`
  → S6. `InvocationSyntax` → S5. Package contents untouched.

## Implementation plan

### M1 — Math group form (core + preset)

Ruled shape ([§dd-dr:math-group-form], [§dd-dr:group-taxonomy] amendment):
- `latexlike::MathGroupForm { Inline, Display }` — **exhaustive** (no
  `#[non_exhaustive]`), `Copy/Eq/Hash/Debug`; rustdoc carries the exhaustiveness
  argument and the payload-admission rule (parse-behavior-invariant; semantically
  universal; declared at registration, never derived from spellings).
- `GroupType::Math` → `GroupType::Math(MathGroupForm)` (enum stays
  `#[non_exhaustive]`; parse wiring stays single-armed — `Math(_)` matches once).
- `NodeRef::math_form()` sugar = `group_type()?.math_form()` — table/state-free
  (M1 lands it via direct matching; M2 re-expresses over the role trait).
- `MATH_DELIMITERS` dissolves into `default_token_rules` (rule construction only);
  `MathStyle` + `NodeRef::math_style()` deleted (superseded names — already
  registered in [§dd-dr:superseded-names]).
- `is_math_group()` stays (now `matches!(…, Math(_))`-shaped).

Files: `latexlike/node_ref.rs` (delete `MathStyle`/`MATH_DELIMITERS`/`math_style`,
add `math_form`), `latexlike/mod.rs` (enum payload, `default_token_rules` builds
the four pairs with forms inline, re-export swap), `latexlike/driver.rs` (math
filter arm → `Math(_)`), tests (summary strings become `group(Math(Inline) $ $)`
etc.), `docs/learn-by-example.md` math section (doctest). Order: enum + rules →
node_ref → driver → tests.

Risks: summary-string churn across latexlike tests and the guide doctests; grep
sweep for `GroupType::Math` bare-path patterns.

### M2 — Role traits + `LatexlikeLang` umbrella

Ruled shape ([§dd-dr:latexlike-generalization] + T3 §E + T5 §C1; P3 decision log):
- Four role traits in `techy::latexlike`, method-based, implemented there for the
  preset's own enums (adopting the preset enums = zero code):
  - `LatexlikeGroupType: Copy` — `content_group()`, `math_group(form)`,
    `verbatim_group()`, `math_form(self) -> Option<MathGroupForm>`,
    `is_math(self)` **defaulted** `= math_form().is_some()` (the ruled
    `is_math`/`math_form` split; overriding decouples parse behavior from
    presentation). Coherence contract `math_group(f).math_form() == Some(f)`.
  - `LatexlikeCallableType: Copy + PartialEq` — constructors `macro_callable()` /
    `environment_callable()` / `specials_callable()`; predicates `is_macro()` /
    `is_environment()` / `is_specials()` defaulted by equality with the
    constructor (coherence contracts mirroring math-form's, per T3 §E1).
  - `LatexlikeMode: Copy + PartialEq` — **only** `math_mode()` + `is_math()`
    (defaulted by equality); NO text-mode constructor, NO `is_text` (T3 §E1 trim).
  - `LatexlikeEvent` — `exit_math_context()` constructor +
    `is_exit_math_context(&self)` recognizer (required — events carry no `Eq`
    bound); coherence contract; defaulted-method evolution posture documented.
- Preset event enum `latexlike::Event` (bare, module-scoped per
  [§dd-dr:preset-vocabulary]; `#[non_exhaustive]`) with variant
  `ExitMathContext`; `Latexlike::Event = Event`. (The loud finalize error lands
  in M4 with the fallible hook from M3.)
- `LatexlikeLang` umbrella: `trait LatexlikeLang: Lang<GroupTypeId:
  LatexlikeGroupType, CallableTypeId: LatexlikeCallableType, ModeId:
  LatexlikeMode, Event: LatexlikeEvent>` with **defaulted behavior methods**:
  - `math_group_rules() -> Vec<Arc<GroupRule<Self>>>` — the math-delimiter data
    behind `default_token_rules` (default: the four pairs built via
    `math_group(form)`);
  - `math_interior_forbidden_chars(removed: &[Arc<GroupRule<Self>>]) -> String`
    — the `$`-merge generalization: default derives the set from the removed
    math-class rules (single-character open/close spellings, deduped), never a
    restated `'$'` literal.
  NO blanket impl; `ClosedVocabulary` NOT a supertrait; `impl LatexlikeLang for
  Latexlike {}` is zero-code. Parameter convention `LLL` at use sites.
- `default_token_rules` goes `LLL`-generic (`default_token_rules<LLL:
  LatexlikeLang>() -> TokenRules<LLL>`), consuming the umbrella's delimiter data
  and `content_group()`; existing call sites infer `Latexlike`.
- NodeRef sugar impl generalized: `impl<'t, LLL: LatexlikeLang> NodeRef<'t, LLL>`
  for `is_math_group`/`math_form`/`macro_name`/`environment_name`/`specials_name`
  via the role traits (the DR component list names "the NodeRef sugar" as a
  generalization member; serves foreign LLLs at zero cost).

Files: new `latexlike/lang.rs` (traits + umbrella + Event) or in `mod.rs`;
`latexlike/mod.rs` (impls for the enums, re-exports, `default_token_rules`
generic); `latexlike/node_ref.rs`. Risks: same-name `is_math` on two traits
(group + mode) — distinct Self types, acceptable; associated-type-bounds syntax in
supertrait position (stable).

### M3 — E4 core machinery (state/engine)

Ruled shape ([§dd-dr:enclosing-state-stack] + T1T2 §E4 + T5 §E):
- **`ParsingStateStack<L>`** — public owning type, new `state/stack.rs`, exported
  via the `core` hub: backed by `Vec<Arc<ParsingState<L>>>` (stored
  outermost-first internally; push/pop O(1)); iteration **innermost-first**
  (`iter()`); `from_states(Vec<Arc<ParsingState<L>>>)` (documented input order:
  innermost-first, matching iteration) and `from_node_ancestors(node:
  NodeRef<'_, L>)` (node's own recorded state first, then parents outward via the
  S3 parent table; contract documented as **scan semantics**, not stack identity
  — Arc-equal duplicates and non-group ancestors harmless); `len`/`is_empty`;
  manual `Clone`/`Debug`.
- **Session stack**: private `ParserSession.state_stack: ParsingStateStack<L>`;
  pushed/popped by the scoped-state primitive (the same descent points as the
  frame stack — every state scoping routes through it); dies with the session
  (zero post-parse residue).
- **`ParseContext::with_parsing_state(state, f)`** — the ruled public scoped form
  (T1T2 §E4: "scoped `with_parsing_state(closure)` form for takeover parsers"):
  the former `pub(crate) with_scoped_state` renamed + made pub, now also
  push/popping the session stack around the swap. `parse_scoped` delegates
  (shorthand-of-same-operation, per the recorded E1 principle).
- **`Lang::finalize_transition` fallible**: `-> Result<(), FinalizeError>`
  (default `Ok(())`). New `state::FinalizeError` (message-carrying, `Display` +
  `Error`, mirroring `ScopeOpError`'s altitude). Folded into `DeriveError`: new
  field `pub finalize_error: Option<FinalizeError>`; `failures` doc updated (may
  be empty when `finalize_error` is `Some`; at least one of the two is present);
  `Display` renders both; `recovered` on a finalize failure = the data as the
  hook left it, frozen (best-effort, documented). The seed still NEVER runs the
  hook — S2's infallibility argument on `lang_initial_with_packages` stays
  intact verbatim.
- **`ParseDriver::resolve_state_event(&self, event: &L::Event, stack:
  &ParsingStateStack<L>) -> Option<ParsingStateDelta<L>>`** — defaulted `None`
  (= context-free, left for `finalize_transition`).
- **`ParseContext::derive_state(&delta)`** — the parser-facing derivation (the
  former `cx.derived_state` renamed; ONE choke point, one cx method): when the
  delta carries events, lends the live stack **current state first** (temporarily
  pushes `cx.state` when it differs by Arc identity from the innermost entry;
  scan semantics tolerate the possible duplicate), runs the event loop (parsers
  never iterate events): each event through `resolve_state_event`; lowered
  events removed, their patch deltas merged (patches in event order, the original
  delta's own explicit overrides winning over patches — the delta author spoke);
  then the ordinary session-mediated derivation (memo + observe unchanged;
  event-free effective deltas memoize as usual). Scoped composition
  **`cx.with_derived_state(&delta, f)`** = `derive_state` + `with_parsing_state`.
  `ParserSession::derived_state` keeps its name/behavior (the session seam; docs
  point parsers at `cx.derive_state`).
- In-parse finalize failure (context-requiring event survived to bare
  `derived()`): routed as an **implementation-error abort** (extension wiring
  bug — the driver failed to lower; aborts under any policy). Scope-op failures
  keep today's funnel path.
- Two-class event contract documented on `ParsingStateDelta` (delta.rs events
  field + module docs) and `Lang::Event` (context-free → `finalize_transition`;
  context-dependent → driver lowering via `cx.derive_state`; loud error in bare
  `derived()`).

Files: `state/stack.rs` (new), `state/parsing_state.rs` (`FinalizeError`,
`DeriveError` fold, `derived()`), `state/lang.rs` (hook signature + Event docs),
`state/delta.rs` (docs), `state/mod.rs` + `core/mod.rs` (exports),
`engine/mod.rs` (session field + push/pop + lend accessors),
`engine/driver.rs` (hook), `constructs/mod.rs` (`with_parsing_state`,
`derive_state`, `with_derived_state`, funnel), call-site sweep
(`cx.derived_state` → `cx.derive_state`: nodes_parser, environment_parser,
verbatim_parser, chars_group_parser, argument_parsers, invocation_parser,
latexlike/environments), test-lang `finalize_transition` impls (parsing_state.rs
tests) get `-> Result<…>`/`Ok(())`. New tests: stack construction/iteration
(from_states, from_node_ancestors), session push/pop symmetry, event lowering
(lowered vs. context-free passthrough), loud finalize error out-of-parse,
in-parse abort.

Risks: borrow choreography in `derive_state` (session stack lent while driver
hook runs — driver is an independent `&'a L::Driver`, fine); memo soundness for
lowered deltas (keyed on the *effective* overrides — sound).

### M4 — Preset pillars + generic driver

Ruled shape ([§dd-dr:preset-driver-pillars] + T5 §D/§E + T1T2 §E4 preset wiring):
- Pillar fns (public, `techy::latexlike`, in driver.rs):
  - `math_group_interior_delta<LLL>(base: &ParsingState<LLL>, rule:
    &Arc<GroupRule<LLL>>) -> Option<ParsingStateDelta<LLL>>` — `None` unless
    `rule.group_type.is_math()`; drops the math-class rules from `base`'s groups,
    merges `LLL::math_interior_forbidden_chars(removed)` into `base`'s
    forbidden set, sets `mode(LLL::ModeId::math_mode())`. Rustdoc carries the
    **two-component recipe**: this delta PLUS the engine's
    `expecting_group_close` descent invariant.
  - `exit_math_context_delta<LLL>(stack: &ParsingStateStack<LLL>) ->
    ParsingStateDelta<LLL>` — scans innermost-first to the FIRST state whose
    `mode().is_math()` is false; restores that context: **whole `TokenRules`**
    (every override field `Some(found value)`) + `mode(found.mode())`; fallback
    when every state is math = the outermost (seed) entry; empty stack = the
    empty delta (documented). NEVER names or constructs a text mode.
  - `make_paragraph_break_node<LLL>(style: ParagraphBreakStyle, state:
    &ParsingState<LLL>, token: &Token<'_, LLL>) -> NodeKind<LLL>` — the two
    styles, `specials_callable()` + generic `SpecialsSpec`; rustdoc:
    **parse-side-only** (synthesis stages `Chars` directly, never mints tokens).
- `SpecialsSpec` minimally generalized (`SpecialsSpec<LLL: LatexlikeLang =
  Latexlike>`) — forced by the paragraph-break pillar's Specials arm (an
  `Arc<dyn CallableSpec<LLL>>` must be mintable for any `LLL`); `MacroSpec` and
  the environments machinery stay monomorphic (S5).
- **`LatexlikeDriver<LLL: LatexlikeLang = Latexlike>`**: adds
  `PhantomData<LLL>`; `Clone + Debug` only (no `Copy`/`Eq` — struck per T4;
  verify the S2-rewritten comment, don't redo); knobs unchanged
  (`recovery` + `paragraph_break_style` pub, resolver private behind
  `with_source_resolver`/`source_resolver()`; NO new knobs). Hook bodies are
  precisely one-line pillar delegations:
  `resolve_command` = `resolve_command_in_scopes(state, token,
  LLL::CallableTypeId::macro_callable())`; `group_interior_delta` =
  `math_group_interior_delta(base, rule)`; `make_paragraph_break_node` = the
  pillar; `resolve_state_event` =
  `event.is_exit_math_context().then(|| exit_math_context_delta(stack))`.
- `Latexlike::finalize_transition` override: an `ExitMathContext` event reaching
  it returns `Err(FinalizeError…)` — the loud bare-`derived()` error.
- `\text` recipe: `ArgumentSpec … .with_state_delta(ParsingStateDelta::new()
  .event(Event::ExitMathContext))` — exercised by parity tests.
- Parity tests: math entry/exit restores the enclosing context (incl. nested
  groups: `{a $x \text{y}$ b}` restores the *brace* context, not the seed;
  custom/embedder group rules preserved; embedder `forbidden_chars` survive the
  math-entry merge AND the exit restore); paragraph breaks (both styles, driver
  parity); `\text`-recipe round-trip (text-mode argument inside display math,
  nested `$…$` inside the argument is math again — the guide scenario, now via
  the event).

Files: `latexlike/driver.rs` (struct + impl + pillars), `latexlike/mod.rs`
(Latexlike::finalize_transition, Driver assoc type, exports),
`latexlike/spec.rs` (SpecialsSpec param), tests in driver.rs/mod.rs.

Risks: type-inference fallout of the driver's new parameter at construction
sites (default param covers type positions; value positions infer through
`Language::new`); `SpecialsSpec` default-param churn.

### M5 — Docs + closure

- Guide `docs/learn-by-example.md`: fix the `\text` recipe (the
  forbidden_chars/groups clobber bug — restore via
  `.event(Event::ExitMathContext)`, never a static reset); update the math
  inline/display section to `math_form`.
- Two-class contract docs verified in place (delta.rs, `Lang::Event`,
  `ParseDriver::resolve_state_event`, `cx.derive_state`).
- DESIGN_RATIONALE status lines (honest scoping — applied S4; spec
  types/environments generalization S5; base package/minidefs S9):
  [§dd-dr:enclosing-state-stack], [§dd-dr:latexlike-generalization],
  [§dd-dr:math-group-form], [§dd-dr:preset-driver-pillars],
  [§dd-dr:argument-factory-additions] (recipe repaired note), group-taxonomy
  amendment already present.
- Superseded-names: verify `restore_text_context_delta`, `MathStyle`/
  `math_style`, `StateStackView`/`StateStack`, parent-state-chain,
  per-`GroupRule` `visible_modes` shapes already registered; add only genuinely
  missing (`MATH_DELIMITERS` as a dissolved table name, if absent).
- ARCHITECTURE.md latexlike/state/engine passages (math_style sugar line, the
  latexlike component roster, state-stack/event lowering sentences).
- CLAUDE.md check (latexlike bullet still accurate).
- Full gates: `cargo build` (0 warnings incl. test targets), `cargo test`,
  `rm -rf target/doc && cargo docs` clean, README rlib check
  (`rustc --crate-type rlib` on the README example against the built rlib),
  greps: zero `MATH_DELIMITERS`, `math_style`, `MathStyle`,
  `restore_text_context_delta`, `StateStackView`, `text_mode()` in
  src/tests/docs.
- Consolidated stage summary here (signature table, deviations/delegated list,
  churn stats).

## Deviations / delegated decisions (running list — for user sign-off)

(Filled as encountered; final list in the stage summary.)

- D-plan-1 (application detail): `ParsingStateStack` internal storage
  outermost-first (O(1) session push/pop); `from_states` input + `iter()` output
  order = innermost-first (one documented public convention).
- D-plan-2 (application detail): in `cx.derive_state`, the lent stack gets the
  current `cx.state` pushed on top when it differs by Arc identity from the
  innermost entry — fulfills the ruled "current state first" even after sibling
  after-effects evolved the run's state; possible Arc-equal duplicate is covered
  by the ruled scan-semantics contract.
- D-plan-3 (application detail): patch-merge order in `derive_state` — patches in
  event order, the original delta's explicit overrides win over patches ("the
  delta author spoke", the temporary-groups exemption precedent).
- D-plan-4 (delegated naming): the finalize failure type is `FinalizeError`
  (state module; `DeriveError` compression precedent; "finalize" is unique
  in-crate since `finalize_node` was deleted). `DeriveError` gains
  `finalize_error: Option<FinalizeError>`.
- D-plan-5 (application decision): an in-parse finalize failure aborts as an
  implementation error under any recovery policy (the driver failed to lower a
  context-requiring event — extension wiring bug, not a source condition);
  scope-op failures keep the tolerant funnel path.
- D-plan-6 (forced ripple): `SpecialsSpec` gains the `LLL` parameter
  (defaulted `= Latexlike`) — the ruled `make_paragraph_break_node::<LLL>`
  pillar cannot mint the `Specials`-style node otherwise. `MacroSpec`/
  environments stay monomorphic (S5).
- D-plan-7 (application detail): `LLL::math_interior_forbidden_chars` default =
  the single-character open/close spellings of the removed math-class rules
  (deduped). Reproduces exactly `'$'` under the canonical rules; multi-char and
  escape-led delimiters (`$$`, `\(`) need no forbidding (unreachable once the
  single-char triggers are forbidden / routed to the command path, per
  [§dd-dr:math-no-nesting]).
- D-plan-8 (application detail): `exit_math_context_delta` restores the found
  state's **whole** `TokenRules` literally (every override field, incl.
  `expecting_group_close`/`temporary_groups`); the descent invariant re-installs
  the correct expected close at the next group descent (the documented
  two-component recipe).
- D-plan-9 (application detail): the NodeRef preset sugar impl becomes generic
  over `LLL: LatexlikeLang` (the DR component roster lists "the `NodeRef`
  sugar"); spellings unchanged for `Latexlike` trees.
- D-plan-10 (routing): A1(iv) escape-char check fn skipped entirely → S9, per
  the stage launch instructions (overriding PHASE3_PLAN's S4 bullet).

## Consolidated stage summary

### Outcome

All five milestones implemented in one run, all gates green. The preset now has
its math-form class payload, the four role traits + `LatexlikeLang` umbrella, the
E4 enclosing-state-stack machinery with two-class event lowering, the three pillar
functions, and the `LLL`-generic `LatexlikeDriver` — with parity tests covering
math entry/exit context restore (nested groups, custom rules, embedder forbidden
chars), paragraph breaks, the `\text` event recipe, and a foreign-family-member
smoke test.

### Signature table (new/changed public surface)

| Item | Signature / shape |
|---|---|
| `latexlike::MathGroupForm` | `enum { Inline, Display }` — exhaustive; payload-admission rule in rustdoc |
| `latexlike::GroupType::Math` | `Math(MathGroupForm)` (enum stays `#[non_exhaustive]`) |
| `NodeRef` preset sugar | `impl<'t, LLL: LatexlikeLang, A> NodeRef<'t, LLL, A>`: `is_math_group()`, `math_form() -> Option<MathGroupForm>`, `macro_name()`, `environment_name()`, `specials_name()` (via role traits; `math_style` deleted) |
| `latexlike::Event` | `#[non_exhaustive] enum { ExitMathContext }`; `Latexlike::Event = Event` |
| `latexlike::LatexlikeGroupType` | `: Copy` — `content_group()`, `math_group(form)`, `verbatim_group()`, `math_form(self)`, `is_math(self)` (defaulted `= math_form().is_some()`) |
| `latexlike::LatexlikeCallableType` | `: Copy + PartialEq` — `macro_callable()`/`environment_callable()`/`specials_callable()`; `is_macro()`/`is_environment()`/`is_specials()` (defaulted by constructor equality) |
| `latexlike::LatexlikeMode` | `: Copy + PartialEq` — `math_mode()`, `is_math(self)` (defaulted by equality); NO text-mode vocabulary |
| `latexlike::LatexlikeEvent` | `exit_math_context() -> Self`, `is_exit_math_context(&self) -> bool` (required) |
| `latexlike::LatexlikeLang` | `: Lang<GroupTypeId: …, CallableTypeId: …, ModeId: …, Event: …>` + defaulted `math_group_rules() -> Vec<Arc<GroupRule<Self>>>`, `math_interior_forbidden_chars(&[Arc<GroupRule<Self>>]) -> String`; NO blanket impl; `impl LatexlikeLang for Latexlike {}` |
| `latexlike::default_token_rules` | `fn default_token_rules<LLL: LatexlikeLang>() -> TokenRules<LLL>` |
| `latexlike::SpecialsSpec` | `SpecialsSpec<LLL: LatexlikeLang = Latexlike>` (manual Clone/Debug/Default) |
| `latexlike::math_group_interior_delta` | `fn <LLL: LatexlikeLang>(&ParsingState<LLL>, &Arc<GroupRule<LLL>>) -> Option<ParsingStateDelta<LLL>>` (two-component-recipe rustdoc) |
| `latexlike::exit_math_context_delta` | `fn <LLL: LatexlikeLang>(&ParsingStateStack<LLL>) -> ParsingStateDelta<LLL>` (first-non-math scan; whole-rules + mode; outermost fallback; empty-stack no-op) |
| `latexlike::make_paragraph_break_node` | `fn <LLL: LatexlikeLang>(ParagraphBreakStyle, &ParsingState<LLL>, &Token<'_, LLL>) -> NodeKind<LLL>` (parse-side-only rustdoc) |
| `latexlike::LatexlikeDriver` | `LatexlikeDriver<LLL: LatexlikeLang = Latexlike>` — `PhantomData<fn() -> LLL>`; `Clone + Debug` only; knobs unchanged (`recovery`/`paragraph_break_style` pub, resolver private); every hook a one-line pillar delegation incl. `resolve_state_event` |
| `core::ParsingStateStack` | owning `Vec<Arc<ParsingState<L>>>`-backed; `new()`, `from_states(Vec)` (innermost-first input), `from_node_ancestors(NodeRef)`, `iter()` (innermost-first), `outermost()`, `len()`, `is_empty()`; `Clone`/`Debug`/`Default` |
| `core::FinalizeError` | `new(impl Into<String>)`, `message()`; `Display`/`Error`/`Clone`/`Eq` |
| `Lang::finalize_transition` | `-> Result<(), FinalizeError>` (default `Ok(())`; seed never runs it); two-class contract on `Lang::Event` |
| `core::DeriveError` | + `pub finalize_error: Option<FinalizeError>` (failures may now be empty when it is `Some`; `Display` renders both) |
| `ParseDriver::resolve_state_event` | `fn (&self, &L::Event, &ParsingStateStack<L>) -> Option<ParsingStateDelta<L>>` (default `None`) |
| `ParseContext::derive_state` | replaces `cx.derived_state` — the one parser-facing derivation choke point, with the event-lowering loop inside |
| `ParseContext::with_parsing_state` | `pub fn (state, f)` — the ruled scoped closure form (former `pub(crate) with_scoped_state`), maintains the session stack |
| `ParseContext::with_derived_state` | `pub fn (&delta, f) -> Result<R, …>` — derive + scope composition |

### Deviations / delegated decisions for user sign-off

D-plan-1 … D-plan-10 above (all held as planned), plus applied specifics:

- The session's live stack entries are the scoped states pushed by
  `with_parsing_state` (the descent chain); the lend inside `derive_state`
  guarantees current-state-first by pushing the evolved `cx.state` when it is not
  the innermost entry.
- `SpecialsSpec` keeps its `pub arguments` field; `MacroSpec` untouched
  (monomorphic until S5).
- The driver's `PhantomData` is spelled `PhantomData<fn() -> LLL>` (compiler-forced:
  a bare `PhantomData<LLL>` makes `Send + Sync` depend on the marker type, which
  `Lang` deliberately does not bound).
- Latexlike's in-crate exhaustive match on `Event` in `finalize_transition`
  (preset-vocabulary convention: new variants must surface every site).

Routings recorded: A1(iv) escape-char check fn → S9 (per stage instructions);
`MacroSpec`/environments/`argument_specs` + fifth role trait + FLM probe re-run →
S5; `base_package`/`minidefs` generalization + contents → S9;
`latexlike::initial_state_data`-style Lang-hook pillar fns → with their stages
(S5/S9). T5-D rider verified (not redone): the S2-rewritten `Copy`/`Eq` comment
stands (engine/driver.rs "Driver `Copy`/`Eq` are deliberately gone…"); knobs
unchanged, resolver private.

### Gate results (final run)

- `cargo build`: 0 warnings; `cargo build --tests`: 0 warnings.
- `cargo test`: 596 lib + 30 acceptance + 8 derive-conditions + 1 derive +
  27 doctests — all green (S3 baseline was 576 lib tests; +20).
- `rm -rf target/doc && cargo docs`: clean (no warnings, no broken links).
- README quick-start: compiled + ran against the built `libtechy.rlib`
  (`README-RLIB-OK`).
- Greps over `techy/src`, `techy/tests`, `docs/`, `README.md`: zero
  `MATH_DELIMITERS`, `math_style`, `MathStyle`, `restore_text_context_delta`,
  `StateStackView`, `text_mode()`.

### Commits

1. `668e449` P3-S4: implementation plan (+ `72c020e`-ish checklist fix)
2. `3901284` M1: GroupType::Math(MathGroupForm)
3. `a5eb56d` M2: role traits + LatexlikeLang umbrella
4. `7140631` M3: E4 enclosing-state stack + event lowering
5. `7bc8161` M4: pillars + LatexlikeDriver<LLL> + parity tests
6. (this commit) M5: docs + closure

### Churn

Through M4: 25 files, +2478/−310; M5 adds guide/DR/ARCHITECTURE doc churn and
doc-link fixes (~+130/−60). Lib tests 576 → 596.

## Handoff notes

None — the stage completed in a single run.
