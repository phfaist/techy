# Phase 3 — S9 stage report: preset definitions + consumer polish (T1/T2 batch)

Worktree: `/Users/philippe/projects/techy/.claude/worktrees/agent-a13b2549c0a026aaf`
Branch: `phase3-s9-preset-defs` (off api-review f1f53f2).

Baseline verified before any change: 726 lib + 30 acceptance + 21 oracle +
8 derive-conditions + 1 derive + 33 doctests (2 ignored pre-existing);
`cargo build` 0 warnings.

## Ruling inputs

- PHASE3_PLAN.md § Protocol + § S9.
- T1T2_RULINGS.md §§ B (rider 1 + main), A1–A4, C, E (E1b/E2/E5/E6), D.
- T3_RULINGS.md § E2 (ClosedVocabulary "provide, don't require"; A1(iv)
  bound-where-used check fn + parse-init wiring — routed here from S4), § B
  (on-ramp context), § H (resolution extraction).
- DESIGN_RATIONALE: [§dd-dr:base-package] (+T1/T2 amendment), [§dd-dr:minidefs],
  [§dd-dr:registration-ergonomics], [§dd-dr:named-argument-errors],
  [§dd-dr:argument-factory-additions], [§dd-dr:diagnostics-position-sort],
  [§dd-dr:resolution-extraction], [§dd-dr:wire-identifier-stability] (reserved
  `core.specs.provider-commands-shadowed-by-escape`), [§dd-dr:on-ramp-defaults],
  [§dd-dr:iter-symbols], [§dd-dr:superseded-names].
- reports/S4_REPORT.md (A1(iv) routing; base-package/minidefs generalization
  routed here), S2_REPORT.md (registration/sealed-conversion substrate),
  S7/S8_REPORT.md (restage `_named` error precedent; oracle harness).

Interpretation note (records win over paraphrase): the launch brief's phrase
"warn when a provider's definitions are ALL unreachable because every escape
char they'd need is *disabled*" is a paraphrase drift; the frozen records
(T1T2 A1(iv), [§dd-dr:registration-ergonomics] measure (b), the reserved
identifier `…shadowed-by-escape`) uniformly specify: warn when **all (≥1) of a
provider's command definitions start with the escape char**. The frozen version
is implemented.

## Implementation plan

### M1 — Base package reshape + `latexlike::minidefs`

1. `base_package()` → **`builtin_package<LLL: LatexlikeLang>() -> Package<LLL>`**
   (generalization routed here by S4), package name `"base"` → `"_builtin"`,
   contents slimmed to the `\begin`/`\end` dispatch pair only. `&` deleted from
   shipped definitions entirely. Doc rewrite (what must be preloaded for any
   latexlike parse; unload key `"_builtin"`).
2. New file `techy/src/latexlike/minidefs.rs`, `pub mod minidefs`; single public
   item `minilatex_package<LLL: LatexlikeLang>() -> Package<LLL>` (plus the
   compiler-demanded ext bounds, expected `ArgumentExt<LLL>: Default`; recorded
   if any more are forced). Contents (package `"minilatex"`):
   - `\emph`/`\textbf`/`\textit` = `MacroSpec` `"m"` (fallback on);
   - `itemize`/`enumerate` = `EnvironmentSpec::new(vec![])` +
     `.with_body_delta(ScopeOp::Push(item_package))` pushing one shared inner
     package `"minilatex.item"` defining `\item` = `MacroSpec` `"o"` — the
     body-scoped-definitions exemplar;
   - the moved specials: `~` (every mode) + ligatures ``` `` ```/`''`/`--`/`---`
     (seed-mode-only via `LLL::initial_state_data().mode` — see D-plan-2), one
     shared zero-argument `SpecialsSpec` instance.
   - NO binding reference from any other latexlike module (the `pub mod`
     declaration only); grep-checked at closure.
3. Sweep every `"base"`/`base_package` reference: `Latexlike::initial_state_data`,
   rustdoc across latexlike (mod/driver/input/environments/spec), tests
   (provider-name assertions, `Unload { name: "base" }`, "searched providers:
   base" strings), docs/learn-by-example.md.
4. Test moves (ruled adaptation, coverage preserved): the seed-specials tests in
   latexlike/mod.rs move to minidefs tests loading minilatex; new default-shape
   tests pin that a base-only parse emits `~`/ligatures as plain chars and `&`
   as a plain char everywhere (removed entirely); text-only ligature visibility
   + longest-match + `~`-in-math re-asserted under minilatex; \item resolves
   inside list bodies only (unresolvable outside).
5. Oracle + acceptance adaptations: specials/ligature rows load
   `minilatex_package()` (oracle language gains it outermost, testdb innermost
   so testdb's `itemize` keeps shadowing); acceptance tests exercising `~`
   shapes adapted the same way.

### M2 — F5 traps: did-you-mean, insert callout, A4 docs, BracedOnly

1. **Did-you-mean** in the `resolve_command_in_scopes` miss arm (detail on the
   existing `Unresolved`/`UnresolvableCommand`, no new condition): after the
   searched-providers detail, scan the providers innermost-first via
   `iter_symbols(callable_type, state.mode())` (symbols, not vocabularies — no
   `ClosedVocabulary` dependency, per [§dd-dr:iter-symbols] amendment):
   - the initial-escape-char case: an entry named `{escape_char}{name}` →
     "provider ‘p’ defines ‘\greet’ — command names are registered without the
     escape character";
   - a small edit-distance check (Levenshtein ≤ 1, ≤ 2 for longer names; capped
     suggestion count) → "did you mean ‘…’ (provider ‘p’)?".
   Providers that cannot enumerate are skipped (fallback-provider limitation
   accepted and recorded). Tests: escape-trap hint, typo suggestion, no-fire
   under a fallback provider (resolution succeeds), searched-providers detail
   retained.
2. **`Package::insert` loud callout** (A1(iii)): normalized-name contract — the
   registered name never includes the escape character; deliberate absence of
   insert-time validation (escape chars can change mid-parse; leading-escape
   names can be intended) with a pointer to the two catch-where-it-bites
   measures.
3. **A4 docs**: the spec-type/callable-type cross-check absence is
   documented-legitimate (composition owns Environment parsing; the spec
   contributes argument structure) — sentence on `Package::insert` +
   guide passage.
4. **`"BracedOnly"` word code** (list form only) in the argument-code factory:
   `GroupArgumentParser::new(content_group()).with_expression_fallback(false)`
   — a mandatory *content-class* group, no fallback ("braced" = the class's
   delimiters, not literal `{}`). Doc table row + loud callout on `m` (fallback
   on, TeX-faithful; BracedOnly is the no-fallback alternative). Tests:
   BracedOnly parses a content group (incl. custom-delimiter content class),
   missing group diagnosed with nothing swallowed, compact-string form is
   an unknown code `B` (word codes are list-form only).

### M3 — A1(iv): the all-escape-chars provider warning + wiring

1. **Condition type** `ProviderCommandsShadowedByEscape` beside the specs
   conditions (scopes/mod.rs, exported at `techy::core::specs`), derive
   `DiagnosticInfo`, frozen identifier
   **`core.specs.provider-commands-shadowed-by-escape`** + identifier-asserting
   test. Payload: `provider: String`, `callable_type: String` (Debug-rendered),
   `count: usize`, `example: String`, `escape_chars: String`. Display wording
   (delegated decision, drafted): "all {count} ‘{callable_type}’ definitions of
   provider ‘{provider}’ begin with an escape character (e.g. ‘{example}’) —
   command names are registered without the escape character, so none of them
   can resolve".
2. **Check function** beside the condition (exported at `core::specs`):
   `check_provider_commands_shadowed_by_escape<L: Lang>(state, source,
   &mut Diagnostics<L::SourceOrigin>) where L::CallableTypeId:
   ClosedVocabulary, L::ModeId: ClosedVocabulary` — for each provider on the
   seed stack and each callable type in `ALL`: union the enumerated names over
   all modes (providers answering `None` are skipped — gracefully absent); if
   all (≥1) start with one of the state's command escape chars, push one
   Warning diagnostic (span: empty at source start). Fires regardless of
   fallback providers.
3. **Parse-init wiring**: new defaulted core hook
   `ParseDriver::observe_parse_start(&self, source, seed, &mut Diagnostics)`
   (default no-op; the `observe_transition` family), called once by
   `Language::parse_source` before the root loop (attached-source sub-parses
   are deliberately not re-checked). Preset: defaulted no-op
   `LatexlikeLang::check_parse_start(…)` (the overridable-behavior-default
   roster); `Latexlike` overrides it with the unconditional call (the
   monomorphic path — bounds hold concretely); `LatexlikeDriver<LLL>::
   observe_parse_start` delegates to `LLL::check_parse_start`. A family member
   with enumerable vocabularies opts in with the same one-line override; a
   non-enumerable one changes nothing (see D-plan-3).
4. Tests: all-escape package warns (identifier + payload + Warning severity +
   parse still succeeds), mixed package silent, fires with a fallback provider
   present, specials-only provider silent, `_builtin` never warns, direct-call
   path for frameworks.

### M4 — A3: `_named` accessors return `Result`

1. New error enum in `core::node`: **`NamedAccessError`** — `NotACallable`,
   `UnknownArgumentName { name }`, `UnknownSlotName { name }`; Display +
   `core::error::Error` + Clone/PartialEq; never panics
   ([§dd-dr:panic-policy]).
2. `NodeRef::argument_nodes_named` / `argument_content_nodes_named` →
   `Result<Option<NodeSlice>, NamedAccessError>`: `Err` = not-a-callable /
   name not among the spec's declared arguments; `Ok(None)` = declared but
   absent; `Ok(Some)` = present.
3. `slot_content_nodes_named` → `Result<NodeSlice, NamedAccessError>` (slots
   have no declared-but-absent state — see D-plan-4).
4. Straggler check result (after S3/S7): transform's `restage_argument_named`
   already errors on unknown names (S7, `RestageError::UnknownArgumentName`) —
   compliant; `ParsedArguments::get_named`/`ParsedSlots::get_named` stay plain
   map lookups (`None` = unknown name only, no conflation — documented with a
   pointer); the NodeRef trio above are the stragglers.
5. Indexed pair docs (A3 docs-only half): replicate the `argument_nodes`
   contract sentence on all indexed accessors + explicit pointer to the
   `_named` forms as the discriminating alternative.
6. Call-site sweep (tests, doctests, guide) + new tests for all five outcomes.

### M5 — Sugar (E1b / E2 / E6)

1. **`define_macro`/`define_environment`** as inherent methods on
   `Package<LLL>` written in the preset (the [§dd-dr:inherent-preset-sugar]
   in-crate mechanism; shorthand-not-second-path principle restated in docs):
   `define_macro(&mut self, name, codes) -> Result<Option<Arc<dyn
   CallableSpec<LLL>>>, ArgumentCodeError>` = `insert(macro_callable(), name,
   MacroSpec::new(argument_specs(codes)?))`; `define_environment` the same
   over `EnvironmentSpec` + `environment_callable()`. Codes = the list form
   (the factory's primary input; word codes included). NO escape-char
   validation (A1(i)).
2. **`argument_specs_named([("o","greeting"), ("m","name")])`**: sibling
   factory; internals refactored so code scanning yields the parser and each
   entry point wraps (`new_unnamed` vs `new(parser, name)`); same error
   grammar, index = pair index.
3. **`Diagnostics::sorted_by_position() -> Vec<&Diagnostic<O>>`**: stable sort
   by (source in first-appearance order — `Arc` identity, span start);
   documented as source order *within each source* (total cross-source order
   deliberately not claimed). Tests incl. a two-source parse.

### M6 — Records + docs + closure

- DR notes: applied notes on [§dd-dr:base-package], [§dd-dr:minidefs],
  [§dd-dr:registration-ergonomics] (rulings 2–3), [§dd-dr:named-argument-errors],
  [§dd-dr:argument-factory-additions] (items 1–2), [§dd-dr:diagnostics-position-sort],
  [§dd-dr:resolution-extraction] (miss-arm detail), [§dd-dr:iter-symbols]
  (A1(iv) landed), [§dd-dr:wire-identifier-stability] (reserved identifier now
  applied). Superseded-names: `"base"`/`base_package()` already registered
  (T1/T2 bullet) — verified, no re-additions needed unless new rejections
  arise.
- ARCHITECTURE.md: latexlike section (minidefs module line; `_builtin`;
  generalization paragraph loses "still monomorphic: base_package/minidefs");
  specs passages (did-you-mean, parse-init check + hook, one-liners); node
  (_named Result), error (sorted_by_position) as applicable.
- CLAUDE.md facade list: minidefs mention on the latexlike bullet.
- docs/learn-by-example.md: `"base"` → `"_builtin"` passage; specials examples
  load minilatex; one-liner mention.
- Gates: `cargo build` (0 warnings), `cargo test` (all green; count changes
  recorded), `rm -rf target/doc && cargo docs` clean; sweep greps:
  `base_package`, `Package::new("base")` / `"base"` as the preset package
  name, `resolve_via_scopes`, other superseded spellings.

## Deviations / delegated decisions (running list — user sign-off at stage end)

- **D-plan-1 (delegated wording/naming)**: the A1(iv) condition type name
  `ProviderCommandsShadowedByEscape`, check-fn name
  `check_provider_commands_shadowed_by_escape` (the `check_include_chain`
  precedent), Display wording as drafted in M3; the warning granularity is
  **per provider × callable type** — core cannot know which types are
  command-resolved, and pooling all types would let one correct specials
  entry silence an all-mistyped macro table (the frozen text's "command
  definitions" read as "the definitions of one name-resolved vocabulary").
- **D-plan-2 (realization, forced by a rulings interaction)**: minidefs'
  text-only ligature visibility is expressed as **the language's seed mode**
  (`LLL::initial_state_data().mode`): T3 trimmed `text_mode()` from the mode
  role trait (no text-mode constructor exists for generic `LLL`), while the
  minidefs ruling requires "same mode visibilities as today". For `Latexlike`
  the seed mode IS `Mode::Text`, so the shipped behavior is exactly the ruled
  one; generically it reads "visible in the document-base mode". Flagged
  prominently for sign-off (resolvable without revising either ruling, hence
  not escalated as a rulings tension).
- **D-plan-3 (realization)**: the A1(iv) parse-init wiring mechanism — trait
  impls cannot add per-method bounds, so a generic `LatexlikeDriver` method
  cannot state `ClosedVocabulary` "narrowly at the call site"; realized as the
  defaulted no-op `LatexlikeLang::check_parse_start` behavior method
  (family members opt in monomorphically, where the bound trivially holds) +
  the new defaulted core hook `ParseDriver::observe_parse_start` called once
  at parse initialization. The hook is the minimal core seam any parse-init
  diagnostics wiring needs.
- **D-plan-4 (under-determined detail)**: `slot_content_nodes_named` returns
  `Result<NodeSlice, E>` (no `Option`): the ruled `Result<Option<…>, E>` shape
  encodes "declared but absent", which slots do not have (`ParsedSlot.region`
  is total) — an always-`Some` option would force a dead arm on every caller.
  The DR entry names only the two argument accessors; the slot accessor is
  brought in line on the unknown-name-is-an-error principle.
- **D-plan-5 (under-determined detail)**: `sorted_by_position` returns
  `Vec<&Diagnostic<O>>` (borrowing; zero-clone), not an owned `Diagnostics`
  (which would misrepresent limit/suppressed bookkeeping on a re-sorted copy).
- **D-plan-6 (under-determined detail)**: `define_macro`/`define_environment`
  take the code **list form** (`["o", "m"]` — the factory's primary input per
  [§dd-dr:argument-specs-list-primary], word codes reachable); compact strings
  stay the `argument_specs_from_str` + `insert` spelling. Return
  `Result<Option<Arc<dyn CallableSpec<LLL>>>, ArgumentCodeError>` (preserving
  `insert`'s replaced-spec answer).

(Further entries appended as encountered.)

## Milestone log

- **M1 (03ee6e1)** — base package reshape + minidefs. `builtin_package<LLL>()`
  (`"_builtin"`, begin/end only; `SlotExt<LLL>: BodySlotExt` bound from
  `BeginSpec`); `&` gone; new `latexlike::minidefs::minilatex_package<LLL>()`
  (6 unit tests incl. the body-scoped `\item` exemplar); ruled test moves
  (specials → minidefs; oracle + extract/node_ref/invocation_syntax/scopes test
  helpers load minilatex; seed-shape pins); guide passages; generic test
  packages renamed off `"base"`. Tests 726 → 729 lib; doctests 33 → 34
  (minidefs example).
- **M2 (8dae7ba)** — did-you-mean miss detail (escape-prefixed callout +
  Levenshtein ≤ 1/2 capped suggestions, innermost-first, enumerable providers
  only; appended to the searched-providers detail); `Package::insert`
  normalized-name callout + A4 no-cross-check passage + guide sentence;
  `BracedOnly` (+ loud `m` fallback callout; content-class test with a custom
  `«»` pair). 733 lib.
- **M3 (67e0cc1)** — A1(iv): `ProviderCommandsShadowedByEscape`
  (`core.specs.provider-commands-shadowed-by-escape`, identifier test) +
  `check_provider_commands_shadowed_by_escape` in `core::specs`; new defaulted
  `ParseDriver::observe_parse_start` called once by `Language::parse_source`;
  `LatexlikeLang::check_parse_start` default + `Latexlike` override +
  `LatexlikeDriver` delegation. Tests: payload/severity/identifier, mixed-table
  silence, fires-despite-fallback, per-type separation. 736 lib.
- **M4 (54676c4)** — `NamedAccessError`; `argument_nodes_named` /
  `argument_content_nodes_named` → `Result<Option<NodeSlice>, _>`;
  `slot_content_nodes_named` → `Result<NodeSlice, _>`; indexed-accessor docs +
  `get_named` pointers; call-site sweep; five-outcome tests. 736 lib.
- **M5 (791810a)** — `Package<LLL>::define_macro`/`define_environment`;
  `argument_specs_named`; `Diagnostics::sorted_by_position()`. 740 lib;
  35 doctests (define_macro example).
- **M6 (this commit)** — DR applied notes ([§dd-dr:base-package],
  [§dd-dr:minidefs], [§dd-dr:registration-ergonomics], [§dd-dr:iter-symbols],
  [§dd-dr:named-argument-errors], [§dd-dr:argument-factory-additions],
  [§dd-dr:diagnostics-position-sort], [§dd-dr:resolution-extraction],
  [§dd-dr:wire-identifier-stability]); ARCHITECTURE passages (seed package /
  minidefs / one-liners bullets, factory line, generalization-complete note,
  driver-hook roster, specs decision line, by-name read contract, diagnostics
  ordering); CLAUDE.md latexlike facade line; final gates + sweeps.

## Signature/surface delta (public API)

| was | is |
|---|---|
| `latexlike::base_package() -> Package<Latexlike>` (pkg `"base"`: begin/end + `&`/`~`/ligatures) | `latexlike::builtin_package<LLL>() -> Package<LLL>` where `SlotExt<LLL>: BodySlotExt` (pkg `"_builtin"`: begin/end only) |
| — | `latexlike::minidefs` (module); `minilatex_package<LLL>() -> Package<LLL>` where `ArgumentExt<LLL>: Default` |
| — | `latexlike::argument_specs_named(impl IntoIterator<Item = (impl AsRef<str>, impl Into<Box<str>>)>) -> Result<Vec<Arc<ArgumentSpec<LLL>>>, ArgumentCodeError>` |
| — | `Package<LLL>::define_macro(name, codes) -> Result<Option<Arc<dyn CallableSpec<LLL>>>, ArgumentCodeError>` (+ `define_environment`; inherent, preset-side, `ArgumentExt<LLL>: Default` on the methods) |
| — | `"BracedOnly"` word code (list form only) |
| `NodeRef::argument_nodes_named(&self, name) -> Option<NodeSlice>` | `-> Result<Option<NodeSlice>, NamedAccessError>` (same for `argument_content_nodes_named`) |
| `NodeRef::slot_content_nodes_named(&self, name) -> Option<NodeSlice>` | `-> Result<NodeSlice, NamedAccessError>` |
| — | `core::node::NamedAccessError` (`NotACallable` / `UnknownArgumentName` / `UnknownSlotName`; `#[non_exhaustive]`) |
| — | `core::specs::ProviderCommandsShadowedByEscape` (id `core.specs.provider-commands-shadowed-by-escape`; fields provider / callable_type / count / example / escape_chars) |
| — | `core::specs::check_provider_commands_shadowed_by_escape<L>(state, source, &mut Diagnostics) where CallableTypeId: ClosedVocabulary, ModeId: ClosedVocabulary` |
| — | `ParseDriver::observe_parse_start(&self, source, seed, &mut Diagnostics)` (defaulted no-op; called once per `parse_source`) |
| — | `LatexlikeLang::check_parse_start(source, seed, &mut Diagnostics)` (defaulted no-op; `Latexlike` overrides with the check) |
| — | `Diagnostics::sorted_by_position(&self) -> Vec<&Diagnostic<O>>` |
| `resolve_command_in_scopes` miss detail = searched providers | + did-you-mean hint (escape-prefixed callout; ≤ 3 edit-distance suggestions) |

Behavior changes (all ruled): seed-only parses emit `~`/ligatures as plain chars
and `&` everywhere; `Unload` key is `"_builtin"`; a parse whose seeded provider
has an all-escape-shadowed table now records one Warning diagnostic; unresolved
commands may carry a longer detail string.

## Gates (final, at M6)

- `cargo build`: clean, **0 warnings** (test targets included via
  `cargo test --no-run`).
- `cargo test`: **740 lib + 30 acceptance + 21 oracle + 8 derive-conditions +
  1 derive + 35 doctests (2 ignored pre-existing)** — baseline was
  726/30/21/8/1/33; +14 lib (6 minidefs + 2 did-you-mean + 2 BracedOnly +
  2 A1(iv)-parse-level + 1 scopes A1(iv) + 2 sugar, net of the 4→1 moved
  specials tests), +2 doctests (minidefs, define_macro).
- `rm -rf target/doc && cargo docs`: clean.
- Sweeps: `base_package` / `Package::new("base")` — zero in src/tests/docs
  (historical mentions remain only inside DESIGN_RATIONALE/rulings records,
  which document the rename); `resolve_via_scopes`, `GroupOnly`, `StrictGroup`,
  `with_body_provider`, `text_mode_argument()`/`text_argument_state_delta()`
  factories — zero. (Note: `docs/learn-by-example.md` has a pre-existing S4
  *local variable* `text_mode_argument` binding the ruled event-recipe spec —
  not the rejected factory; left as shipped by S4.)
- minidefs dead-strippability: `grep` confirms no reference to `minidefs` from
  any other latexlike module beyond the `pub mod` declaration and doc links.

## Final deviation list (for sign-off)

- **D-plan-1** (delegated naming/wording — per the records' "wording is yours"):
  condition `ProviderCommandsShadowedByEscape`, fn
  `check_provider_commands_shadowed_by_escape`, Display wording as shipped;
  warning granularity **per provider × callable type** (core cannot identify
  "command" types; pooling would let one correct specials entry silence an
  all-mistyped macro table).
- **D-plan-2** (realization; flagged prominently): minidefs ligature visibility
  = the language's **seed mode** (`LLL::initial_state_data().mode`) — the T3
  mode-role trim removed the text-mode constructor; for `Latexlike` the seed
  mode is `Mode::Text`, so shipped behavior is exactly the ruled one.
- **D-plan-3** (realization): A1(iv) wiring = new defaulted core hook
  `ParseDriver::observe_parse_start` + defaulted
  `LatexlikeLang::check_parse_start` (trait impls cannot state per-method
  bounds; family members opt in monomorphically). Attached-source sub-parses
  deliberately do not re-fire the hook.
- **D-plan-4** (under-determined detail): `slot_content_nodes_named` returns
  `Result<NodeSlice, E>` without the `Option` layer (slots have no
  declared-but-absent state; an always-`Some` option would force a dead arm).
- **D-plan-5** (under-determined detail): `sorted_by_position` returns a
  borrowing `Vec<&Diagnostic<O>>` (an owned re-sorted `Diagnostics` would
  misrepresent its limit/suppressed bookkeeping).
- **D-plan-6** (under-determined detail): `define_macro`/`define_environment`
  take the code **list form** (word codes reachable; compact strings stay the
  `argument_specs_from_str` + `insert` spelling); they return `insert`'s
  replaced-spec answer inside the `Result`.
- **D-plan-7** (application detail): the did-you-mean scan bounds — suggestions
  capped at 3, Levenshtein threshold 1 (missed name ≤ 4 chars) else 2,
  innermost-first dedup by name; the escape-prefixed callout reports the first
  (innermost) defining provider. Wording as shipped ("provider ‘p’ defines
  ‘\greet’ — command names are registered without the escape character";
  "did you mean ‘…’ (provider ‘…’)?").
- **D-plan-8** (compiler-forced bounds): `builtin_package` carries
  `SlotExt<LLL>: BodySlotExt` (from `BeginSpec`'s `CallableSpec` impl);
  `minilatex_package` + the one-liners carry `ArgumentExt<LLL>: Default` (from
  the argument-code factory / std parsers).

No escalations: no rulings tension surfaced — the closest call (D-plan-2) is
resolvable inside both rulings and is flagged for explicit sign-off attention.

Interpretation note (records vs. launch-brief paraphrase) restated: the A1(iv)
warning implements the frozen "all definitions start with the escape char"
semantics, not the brief's "escape char disabled" paraphrase (see the top of
this report).

## Status

COMPLETE — all six milestones landed; gates green at every milestone and at
closure. Worktree:
`/Users/philippe/projects/techy/.claude/worktrees/agent-a13b2549c0a026aaf`.
