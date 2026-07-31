# Phase 2b — T1/T2 session rulings (in progress)

Working record, P4_RULING.md-style: each point as ruled by the user, in session order.
Durable records (DESIGN_RATIONALE entries + amendments) are written when the session
closes; this file is the authoritative interim record. Brief: T1T2_BRIEF.md.

## B rider 1 — base-package contents move + rename (RULED)

Adopting the user's in-code notes (latexlike/mod.rs:302,317–319), with these rulings:

1. **Move adopted**: the specials leave the base package. `~` and the ligatures
   (`` `` ``, `''`, `--`, `---`) move into `minidefs`'s `"minilatex"` package. The
   default-preset behavior change is accepted (base-only parse emits them as plain
   chars); acceptance-suite tests that exercise specials load minilatex.
2. **`&` is removed entirely** — not moved to minilatex, gone from the preset's
   specials altogether.
3. **Base package renamed `"base"` → `"_builtin"`** — internal-flavored name; holds
   only what must be preloaded for any latexlike parse (`\begin`/`\end` dispatch).
   Unload-by-name key changes accordingly. Superseded-names entry: `"base"` (as the
   preset seed-package name).

Durable-record plan: amendment to [§dd-dr:base-package]; application detail under
[§dd-dr:minidefs]; superseded-names addition.

## B main — minidefs layout, spec choices, generic rider (RULED)

- **Layout/surface**: one file `latexlike/minidefs.rs` → `techy::latexlike::minidefs`;
  one public item, a fn returning bare `Package` (no Arc), explicit activation only,
  dead-strippable. **Fn is named for the package, NOT generic `package()`** — keeps
  room for a second mini-style package in the future (user). Name `minilatex()` vs
  `minilatex_package()`: user offered both; recommendation `minilatex_package()`
  (matches `base_package()` precedent, self-standing when imported) — confirm.
- **Spec choices**: as briefed — \emph/\textbf/\textit `MacroSpec` `"m"` fallback-on;
  itemize/enumerate `EnvironmentSpec` no args + body delta pushing the inner \item
  package; \item `MacroSpec` `"o"`; inner package named `"minilatex.item"`. Plus the
  rider-1 specials: `~` (text+math) and ligatures (text-only), same mode visibilities
  as today.
- **Generic rider**: YES — target `pub fn minilatex_package<LLL: LatexlikeLang>()
  -> Package<LLL>` (modulo the fn-name confirmation); ships Latexlike-shaped if it
  lands before the P3 application, generalizes with it; stable name lands once.
- **Open (flagged, not yet ruled)**: does the fn `base_package()` follow the
  `"_builtin"` rename (→ `builtin_package()`)? Superseded-names discipline suggests
  yes.

- **Naming confirmed**: `minidefs::minilatex_package()`; `base_package()` follows the
  rename → `builtin_package()`.

## A1–A4 — F5 traps (RULED)

- **A1 (escape-char registration trap)**:
  - (i) validation in the one-liners **REJECTED** (user): escape chars can change
    mid-parse in advanced langs; a leading escape char may be fully intended
    (`@greet` + `\makeatletter`-type situations; `@greet` defined before `@` becomes
    the escape char). No escape-char validation anywhere — incl. in `define_macro`/
    `define_environment` if E1 lands.
  - (ii) **ACCEPTED, generalized to "did you mean?"**: on a resolution miss, iterate
    the declared symbols advertised by the scopes (`iter_symbols` family) and report
    near-misses — at minimum an initial-escape-char check ("provider 'p' defines
    'greet'"), possibly a small Levenshtein-distance check. Detail wording lands
    wherever the deferred resolution-family extraction puts `resolve_via_scopes`.
    **Known limitation (user-flagged)**: an in-stack fallback provider makes
    resolution succeed, so the miss path never fires — accepted; partly mitigated
    by (iv).
  - (iii) **ACCEPTED**: loud doc callout on `Package::insert` (normalized-name
    contract).
  - (iv) **NEW measure (user)**: at parse-initialization time (diagnostics sink
    available, `TokenRules` escape char known — the layering-correct moment), check
    the seeded providers and emit a clear warning diagnostic when *all* (≥1) of a
    provider's command definitions start with the escape char. Fires regardless of
    fallback providers.
- **A2 (no-fallback argument code)**: **ACCEPTED**, word code **`"BracedOnly"`**
  (list form only). Semantics clarified by user: it accepts any *content-class
  group* per the parsing state — if `<`/`>` are the declared content-group
  delimiters, `<arg>` is accepted; "braced" refers to the class's delimiters, not
  literal `{}`. Maps to `GroupArgumentParser::new(Content).with_expression_fallback(false)`.
  Doc callout on `m` in any case.
- **A3 (None conflation, indexed accessors)**: **docs only** for the indexed pair,
  explicitly pointing to the `_named` alternative. **NEW sub-ruling (user)**: the
  `_named` accessors must return an **error for a misspelled/unknown name**, not a
  silent `None` — proposed shape (to confirm): `Result<Option<NodeSlice>, E>` where
  `Err` = not-a-callable / name not in spec, `Ok(None)` = declared-but-absent,
  `Ok(Some)` = present. Error type per panic policy (never panic).
- **A4 (spec/type cross-check)**: **docs only** confirmed — surface the existing
  contract (guide + cross-ref from `Package::insert`). See session notes for why the
  mismatch is coherent (composition owns Environment parsing; the spec contributes
  only argument structure).

- **A3 signature confirmed**: `_named` accessors return `Result<Option<NodeSlice>, E>`
  (`Err` = not-a-callable / unknown name; `Ok(None)` = declared-but-absent; error
  type named at application). **A4 confirmed** (docs only; composition owns
  Environment parsing — spec contributes argument structure only,
  environments.rs:344–350).

## C1–C2 — Language-init application details (RULED)

- **C1**: remove `Default for Language<L>` AND `LatexlikeDriver::default()` (P2's
  initial-state-mandatory principle; the driver's recovery knob must be explicit).
  `StdParseDriver::default()` stays for now — fate rides the T3 session.
- **C2**: **YES** — sealed conversion trait **`IntoSpecsProvider`** (user-confirmed
  name); `lang_initial_with_packages(impl IntoIterator<Item: IntoSpecsProvider<L>>)`
  accepts `Package<L>` by value, `Arc<P>`, `Arc<dyn SpecsProvider<L>>`. Same idiom
  reused for `Package::insert` (E1a) — one Arc-removal pattern crate-wide.

## E — sugar batch (RULED except E4/E5 naming)

- **E1 RULED, all three**: (a) `Package::insert`/`insert_specials`/`…_in_modes` take
  the sealed-conversion treatment (sibling trait to `IntoSpecsProvider`, spec-side) —
  no `Arc::new` at call sites; parameter-order flip fixed
  (`insert_specials(callable_type, trigger, spec)`); (b) preset one-liners
  `define_macro`/`define_environment` as inherent methods on `Package<LLL>`,
  `Result`-returning, NO escape-char validation (A1(i)). **User principle recorded**:
  a shorthand spelling of *the same operation* is not a one-canonical-path violation —
  the rule targets different *ways* (model-level duplicates like `with_provider`),
  not shorter spellings of the same way.
- **E2 RULED**: sibling factory `argument_specs_named([("o","greeting"),…])`.
- **E3 RULED**: `with_body_provider` REJECTED (abandoned-at-first-need; minidefs +
  guide cover discoverability).
- **E4 RULED** (design evolved through five iterations): canned text-mode-argument factory
  REJECTED as proposed — (a) doesn't compose (text-mode *optional* argument would
  need a second factory); (b) the guide recipe it would codify is BUGGY: statically
  resets `forbidden_chars` to `""` (clobbers embedder forbidden chars — the driver's
  own math-entry code explicitly preserves them, driver.rs:161–166) and statically
  resets `groups` to `default_token_rules().groups` (clobbers custom group rules).
  Design iterations: (1) canned factory — rejected (doesn't compose, codifies the
  buggy recipe); (2) preset finalize-derivation with declared/effective split —
  rejected (StateExt duplication / Enabled-flag statefulness / mode-keyed TokenRules
  all flawed); (3) per-`GroupRule` `visible_modes` — REJECTED by user: too specific
  to the math/text use case (why groups and not comments/whitespace?), and it plants
  a semantic interpretation of "mode" in core, deliberately unclaimed there.
  (4) parent-state chain (`ParsingState` keeps an Arc'd enclosing pointer) —
  **WITHDRAWN by user concern**: even with the enclosing-not-derivation-source
  refinement (which bounds memory to nesting depth and is cycle-free by
  construction — immutable states can only reference pre-existing states), it bakes
  parse history into a value type: nodes record parse-time states, so the finished
  tree would pin state ancestry — residue in the parsed material, contra the P4
  philosophy (navigation via a side table on the tree, not pointers in values).
  (5) **RULED: live state stack on the parse machinery** — states stay exactly as
  they are; the *session* (per-parse runtime — not the shared-policy driver) keeps
  the stack of enclosing states, push/pop at the same descent points as the
  traceback frame stack ([§dd-dr:parse-traceback] precedent), scoped
  `with_parsing_state(closure)` form for takeover parsers, innermost-first
  iteration (current state first). The engine already implicitly retains these
  states (group exit restores the outer Arc) — the stack materializes them. Zero
  post-parse residue (stack dies with the session). Entry side of math unchanged
  (filter + forbidden-merge, ForbiddenChar diagnostic). Preset policy: restore =
  whole `TokenRules` of the found state; no-text-state fallback = outermost (seed)
  state. Helper `text_argument_state_delta()`: skip.
  **Restore wiring (RULED, user-confirmed as proposed):**
  - `Lang::finalize_transition` KEPT — placement doctrine (lang.rs:175–182): it is
    what keeps out-of-parse `derived()` calls coherent (P2's blessed embedder
    idiom); mode-shaped transitions need no events at all (delta.mode is the
    signal). Made **fallible** (`-> Result`, folded into `DeriveError`; default
    `Ok(())`): a context-requiring event reaching bare `derived()` errors loudly
    instead of being silently ignored. Seed still never runs it (P2 infallibility
    untouched).
  - **`cx.derive_state(&delta)`** on `ParseContext` (+ scoped
    `cx.with_derived_state(&delta, f)`): lowers context-dependent events via the
    new driver hook **`ParseDriver::resolve_state_event(&event, &StateStackView)
    -> Option<ParsingStateDelta>`** (default `None` = context-free, left for
    `finalize_transition`), merges the patches, removes the lowered events, then
    calls plain `derived()` — one choke point preserved. Per-event policy on the
    driver, event loop inside the one cx method (parsers never iterate events).
  - Rejected: `cx.finalize_parsing_state(data, prev, events)` (exposes the
    crate-owned data→state assembly at parser altitude, lang.rs:187–192); per-event
    cx methods `delta_for_derived_event`/`state_derived_for_event` (merge burden +
    ordering pitfalls at every call site).
  - Preset wiring: `Latexlike::Event` gains a restore variant; `\text`'s
    `ArgumentSpec` delta = `.event(RestoreEnclosingTextContext)` (name at
    application); `LatexlikeDriver::resolve_state_event` walks innermost-first to
    the first `mode() == Text` state (else outermost), patches its whole
    `TokenRules` + `mode(Text)`. Core learns nothing about modes.
  - **Pillar-function rider (user)**: the latexlike driver's event logic (math
    entry AND text restore) is extracted into **public functions** (post-P3:
    `LLL`-generic pillar fns; the driver hooks are one-line delegations) so
    **post-parse processing can synthesize coherent recorded states** — e.g. a
    transform creating child nodes emulating "enter math" / "restore text"
    ([§dd-dr:transform] tie-in: synthesized/restaged nodes need coherent
    parse-time states).
  - Doc revision rides application: delta.rs:130 + `Lang::Event` get the two-class
    contract (context-free → finalize_transition; context-dependent → driver
    lowering via cx.derive_state; loud error in bare derived()).
- **E5 RULED**: **`NodeKind::as_str()`** (user-confirmed; `label` rejected — sounds
  user-provided/dynamic; `kind_as_string` rejected — stutter + allocation
  connotation).
- **E6 RULED**: `Diagnostics::sorted_by_position()` (returning-adjective form), this
  session; sort key = (source in first-appearance order, span start).
- **Wish 15**: no API — `NodeRef::name()` covers it; guide gap. **Wish 8** → T3.

## D — debug tree visualizer (RULED)

- **A free public function `display_tree(node) -> String`** — deliberately NOT a
  `NodeRef`/`NodeTree` method (lean; trivially dead-code-eliminated when unused).
  Home: the node group (`core::node` under P1).
- One line per node: box-drawing prefix + `summary()` + position as **line/col**
  (not byte offsets; internal per-source `LineIndex`).
- **Multi-source trees**: print the source name when it *changes* from the previous
  line's source; the initial source name is omitted.
- Output format explicitly not a stability contract (same caveat as `summary()`).
  v1 ignores P4 annotations (possible later variant).
