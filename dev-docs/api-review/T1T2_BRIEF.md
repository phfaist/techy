# Phase 2b — T1/T2 Decision-Session Brief (consumer + extender personas)

Prepared 2026-07-31. Inputs: PLAN.md decision log (P1–P5 rulings binding),
POLICY_BRIEF.md §Routing, SYNTHESIS.md (F5, wishlist), walkthroughs/{consumer,extender}/,
DESIGN_RATIONALE entries as cited. Every code claim re-verified against the working
tree at commit 0947f53 (file:line cited). The brief recommends; all rulings are the
user's. Constraints honored: one canonical path + specs=author-side/hub=run-side (P1);
no facade fns/builder (P2); everything pub is one stability class under the soft freeze
(P5) — **each accepted item below is a permanent stable name**; Result-not-panic
[§dd-dr:panic-policy]; superseded names stay dead. Format per point:
Context → Evidence → Options → **Recommendation** → Cost.

---

## A. The F5 traps — the review's worst individual finding (T2)

All four traps share one shape: an everyday extender action succeeds silently and
misbehaves later, or two different situations produce the same answer.

### A1. Registering `"\greet"` (with backslash) is accepted, then unfindable

**Context.** Definitions are stored under *normalized* names — for latexlike, the bare
name without the escape character. Nothing checks this at insert time; at parse time the
user gets "cannot resolve command `\greet` (searched providers: p, base)" — actively
misleading, since package `p` *does* contain it, under the wrong key.

**Evidence.** `Package::insert` accepts any string as name, no validation
(techy/src/scopes/mod.rs:654–661); "normalized spelling" is stated but defined only on
`CallableQuery::name` ("normalization is the caller's (preset's) business",
scopes/mod.rs:94) where no extender looks. The miss detail is built by
`CommandResolution::resolve_via_scopes` (techy/src/engine/driver.rs:439–465).

**Options.**
1. Core insert-time validation — **wrong layer**: `Package<L>` is Lang-generic; the
   escape character is a `TokenRules` fact the author-side specs layer cannot know (P1
   boundary).
2. Preset-side validation inside the new `define_macro`/`define_environment` one-liners
   (E1): reject or strip a leading escape char with a clear error.
3. Near-miss detail on the failure path: on a miss, the searched-providers detail also
   reports a key differing only by escape prefix ("provider 'p' defines 'greet' —
   commands are registered without the escape character"). Lookup cost only on the
   already-cold miss path.
4. Doc-only: loud callout on `Package::insert` + the guide.
5. Do nothing.

**Recommendation: 2 + 3 + 4 together** — authoring time, moment of confusion, and
readers, without a core validation that would be wrong for non-latexlike Langs. Note:
3 touches `resolve_via_scopes`, whose extraction-to-specs P1 deferred — rule the
*policy* here; the wording lands wherever that extraction puts the code.
**Cost.** Small preset + resolver code; raw `Package::insert` stays unvalidated by
design (documented).

### A2. The single-expression fallback ambushes `\greet{name}`-style macros

**Context.** The `m` argument code is TeX-faithful: `\greet word` is *not* an error —
the mandatory argument silently takes `w` (`\frac12` behavior). Extenders defining
braced-argument macros expect "missing braces" to be diagnosed, and there is no
argument code for "mandatory braced group, NO fallback".

**Evidence.** `m` = "a `{…}` content group, or the single-expression fallback"
(techy/src/latexlike/arguments.rs:117). The **capability already exists in core**:
`GroupArgumentParser::with_expression_fallback(false)`
(techy/src/constructs/argument_parsers.rs:551), and "class form + fallback off" is
called out as a supported techy extension (argument_parsers.rs:493–495). Only the
latexlike *code spelling* and the doc callout are missing. `r<c1><c2>` is no substitute:
`r{}` mints custom-delimiter semantics, not the standard content group
(arguments.rs:121; T2 FRICTION Task 4.1).

**Options.**
1. New **word code** (list-form only; precedent `AnyDelimited`, arguments.rs:126–134)
   mapping to `GroupArgumentParser::new(Content).with_expression_fallback(false)`.
   Name candidates: `"GroupOnly"`, `"BracedOnly"`, `"StrictGroup"`. Avoid reusing an
   xparse letter with different semantics (`g` is xparse's deprecated *optional* brace
   group — actively misleading).
2. Single-char code (e.g. `M`) — compact-string compatible but near-invisible next to
   `m`, and not xparse vocabulary either.
3. Doc-only loud callout on `m` (guide + factory table). 4. Do nothing.

**Recommendation: 1, plus the doc callout in any case.** The fallback stays `m`'s
default (deliberately TeX/pylatexenc-faithful, [§dd-dr:expression-fallback]); the word
code gives the no-fallback shape a first-class spelling without touching `m`.
**Cost.** One more stable word code (P5); list-form-only asymmetry (already true of
`AnyDelimited`).

### A3. `argument_content_nodes(i)`: absent optional and index-typo both give `None`

**Context.** `argument_content_nodes(0)` (absent optional) and
`argument_content_nodes(7)` (no such argument) are indistinguishable; a typo'd index
silently reads as "argument absent". The distinguishing path exists:
`arguments().get(i)` → `is_provided()`.

**Evidence.** node/node_ref.rs:294–302 (`region.as_ref()?` conflates all `None`
sources); the sibling `argument_nodes` **doc was already fixed** — it now spells out
"`None` for non-callables, out-of-range indices, and absent arguments (consult
`arguments` to distinguish)" (node/node_ref.rs:276–279); the `content_nodes` pair and
the `_named` pair lack the sentence. `ParsedArgument::is_provided`
(node/arguments.rs:256) is the discriminator.

**Options.** 1. Doc contract: replicate that sentence on the three remaining accessors
+ one guide paragraph. 2. Richer return (`Result`/tri-state) — forks the read idiom:
the whole `NodeRef` surface is Option-on-kind-mismatch by design; one Result accessor
would be an island. 3. Panic on out-of-range — against the panic policy's grain (this
family *is* the non-panicking companion shape).

**Recommendation: 1 (do-nothing-plus-docs).** The conflation is the standard cost of
the crate-wide Option idiom; the discriminator is one documented hop away.
**Cost.** The trap survives for non-readers; accepted.

### A4. `MacroSpec` under `CallableType::Environment` is unchecked

**Context.** The spec-type ↔ callable-type pairing is convention; a `MacroSpec`
registered under `Environment` parses without complaint (T2 probes B/C).

**Evidence — the finding is largely overtaken.** The semantics are *defined and
documented as legitimate* on `EnvironmentSpec` (since 2026-07-19 — pre-walkthrough; the
persona missed it because it lives on the type, not on insert or the guide):
"registering an `EnvironmentSpec` under another callable type gets the macro-shaped
default parse, which reads the arguments and no body. A generic non-`EnvironmentSpec`
`CallableSpec` under `CallableType::Environment` is legitimate too"
(techy/src/latexlike/environments.rs:344–350).

**Options.** 1. Insert-time cross-check — would outlaw documented-legitimate
combinations, and core `Package<L>` cannot know preset pairings anyway (A1's layering).
2. Surface the existing contract where extenders look (guide + `Package::insert`
cross-ref); the E1 one-liners make correct pairing structural. 3. Do nothing.

**Recommendation: 2.** No validation — the mismatch is defined behavior; the one-liners
remove the accident vector on the happy path. **Cost.** Docs only.

---

## B. Minidefs application (P2 ruling [§dd-dr:minidefs] + P3 rider)

**Context.** P2 ruled: `techy::latexlike::minidefs`, single package `"minilatex"`,
contents \emph, \textbf, \textit, itemize, enumerate, with \item body-scoped inside the
two list environments (the in-tree exemplar of body-scoped definitions); positioning is
debug/prototyping tool, NOT a database; no binding reference from other latexlike
modules (dead-strippable). This session settles layout, spec choices, and the P3 rider.

**Module layout & surface (recommendation).** One file `techy/src/latexlike/minidefs.rs`
→ public path `techy::latexlike::minidefs` (module under the preset, per ruling; the P1
topology leaves latexlike unchanged). Public surface: **exactly one item**,
`pub fn package() -> Package<Latexlike>` — the P3 decision-log wording already says
`minidefs::package::<LLL>()`, so `package` is the presumptive name ("context determines
names": inside `minidefs::` there is no competing sibling). Return bare
`Package<Latexlike>` (not Arc'd) — matches `base_package()`
(techy/src/latexlike/mod.rs:296) and feeds `lang_initial_with_packages` directly once
C2's conversion lands. Activation is always explicit:
`ParsingState::lang_initial_with_packages(vec![minidefs::package()])` — no auto-seeding
(dead-strip constraint).

**Spec choices (recommendation).**
- `\emph`, `\textbf`, `\textit`: `MacroSpec::new(argument_specs(["m"])…)` — plain `m`,
  fallback on (TeX-faithful; `\emph x` takes `x`).
- `itemize`, `enumerate`: `EnvironmentSpec::new(vec![])` +
  `.with_body_delta(ParsingStateDelta::new().push_provider(item_pkg))`
  (environments.rs:372) — the body-scoped exemplar, exactly T2's stretch-task pattern.
- `\item`: `MacroSpec::new(argument_specs(["o"])…)` (the optional label), defined in a
  shared inner package; its name shows in diagnostics ("searched providers: …") —
  candidates `"minilatex.item"` (recommended: self-explaining in a miss message) or
  `"item"`.

**Rider 1 — the user's fresh in-code notes (commit 0947f53, post-P2) expand the
question.** techy/src/latexlike/mod.rs:302 ("We should not include & here") and
:317–319: rename `"base"` to something internal (`"_base"`/`"_builtin"`/`"_primitive"`)
holding only the mandatory begin/end dispatch, and move the specials (`&`, `~`,
`` `` ``/`''`/`--`/`---`) out of base — "should be included in
techy::latexlike::minidefs/minilatex package instead". To weigh in session: (a) a
default Latexlike parse would no longer treat `~`/ligatures as specials — a preset
behavior change vs pylatexenc's default context; the acceptance suite
(techy/tests/acceptance.rs:176 seeds `base_package()`) needs minilatex loaded for the
affected tests; (b) `&`: dropped or moved to minilatex? The notes read as "moved, and
certainly not in base"; (c) renaming `"base"` changes an unload-by-name key
(mod.rs:293–295) — cheap now, never later. **Recommendation: adopt the notes** — they
apply [§dd-dr:minidefs]'s own positioning argument consistently (typography
interpretation is definitions content, not parsing substrate); `&`, `~`, ligatures move
to minilatex; decide the base rename here (lean `"_base"`). Record as an amendment to
[§dd-dr:base-package] + superseded-names entry.
**Cost.** Default preset output changes for `~`/ligatures/`&` (chars instead of
specials nodes); acceptance-suite churn; no dependents exist yet.

**Rider 2 — generic `minidefs::package::<LLL>()` (P3,
[§dd-dr:latexlike-generalization]).** Contents are pure vocabulary — `MacroSpec`,
`EnvironmentSpec`, `argument_specs`, `push_provider` — all of which P3 already makes
`LLL`-generic; nothing in minidefs is `Latexlike`-specific. **Recommendation: yes,
generic** — `pub fn package<LLL: LatexlikeLang>() -> Package<LLL>`: a debug/prototyping
package is precisely what an FLM-class Lang author wants on day one, and a generic fn
is inherently dead-strippable (monomorphized only on use). Sequencing: land
Latexlike-shaped with the minidefs application if it comes first, generalize in the
same breath as the P3 application (mechanical; the signature above is the target
either way — decide the *target* now so the stable name lands once).
**Cost.** None beyond P3's own machinery.

---

## C. P2 application details ([§dd-dr:language-init] expected-consequence notes)

Current code (unapplied P2, verified): `Language::new(driver)` single-arg
(engine/language.rs:81), `with_seed_delta` (:100), `with_provider` (:122),
`with_resolver` (:130), `Default for Language<L> where L::Driver: Default` (:286–293);
`ParsingState::initial()` (state/parsing_state.rs:66; the seed indeed never runs
`finalize_transition`, :58–68 — P2's infallibility premise holds). Doc examples still
teach the old idioms (e.g. latexlike/arguments.rs:153–155 uses
`Language::<Latexlike>::default().with_seed_delta(…)`) — application must sweep them.

### C1. Fate of the `Default` impls

**Options.** (a) Keep `Language: Default` (becomes
`new(Driver::default(), lang_initial())`); (b) remove it; same question for
`LatexlikeDriver::default()` = `new(Recovery::Strict)` (latexlike/driver.rs:86–90) and
`StdParseDriver::default()`.

**Recommendation: remove `Default for Language<L>`** — P2's principle is
initial-state-MANDATORY; a `Default` reintroduces the implicit seed by the back door,
and `Language::<Latexlike>::default()` was itself T1 friction (turbofish). Also
**remove `LatexlikeDriver::default()`**: strict-vs-tolerant is the driver's one real
policy knob and `Default` hides it. **Keep `StdParseDriver::default()`** for now — its
fate rides the T3 SimpleLang/on-ramp session.
**Cost.** Test/doc churn only (pre-freeze); generic contexts wanting
`L::Driver: Default` lose the preset instance (none in-tree).

### C2. Packages-argument ergonomics for `lang_initial_with_packages`

**Context.** The ruled spelling is
`ParsingState::lang_initial_with_packages(vec![…])`, infallible. Naive signature
`Vec<Arc<dyn SpecsProvider<L>>>` makes users write
`vec![Arc::new(pkg) as Arc<dyn SpecsProvider<_>>, …]` — the exact Arc-and-coercion
noise T1/T2 flagged everywhere (F3), on the API P2 built to be the pleasant path.

**Options.**
1. `Vec<Arc<dyn SpecsProvider<L>>>` — honest, noisy.
2. Generic: `I: IntoIterator, I::Item: <sealed conversion trait>` accepting
   `Package<L>` by value, `Arc<P>`, and `Arc<dyn SpecsProvider<L>>` — spelling becomes
   `lang_initial_with_packages([minidefs::package(), my_pkg])`. (Plain
   `Into<Arc<dyn …>>` bounds don't cover this: unsized coercion isn't `From`, and
   blanket `From<Arc<P>>` impls hit coherence walls — a small sealed trait is the
   clean mechanism.) Heterogeneous mixes fall back to pre-converting; the dominant
   case is homogeneous.
3. Two functions (one for packages, one for dyn providers) — duplicate surface, no.

**Recommendation: 2**, and adopt the *same* conversion idiom for `Package::insert`
(E1) so the crate has one Arc-removal pattern, not two. Trait name decided in session
under [§dd-arch:naming] (candidates: `IntoSpecsProvider`, `IntoSharedProvider`;
specificity favors the former; it is one more permanently-stable pub name — P5).
**Cost.** One sealed pub trait; slightly less obvious signature in rustdoc.

---

## D. Debug ASCII tree visualizer (decision log 2026-07-29)

**Context.** Accepted in principle as the replacement for an elaborate
`extract::plain_text` (rejected — that gap belongs to techy-totext). Today the only
tools are `NodeRef::summary()` — one line, one node (node/node_ref.rs:95–112, format
explicitly not a stability contract) — and the verbose `Debug`. Nothing renders a
subtree shape at a glance; T1 hand-wrote a ~20-line recursive dump (Task 2).

**Scope (recommendation).** One method: given a node, a multi-line preformatted tree of
its subtree — one line per node = tree-guide prefix + `summary()` (+ span offsets,
which cost nothing and answer the next question a debugger asks):

```
list(3)  [0..18]
├── chars(Hello )  [0..6]
├── Macro(emph)  [6..17]
│   └── group(Content { })  [11..17]
└── chars(!)  [17..18]
```

Format human-oriented and **not a stability contract** (summary()'s wording restated).
v1 prints no annotations (`NodeTree<L, A>`, P4) — `Debug` of `A` per line is noise for
`()`; note an annotation column as a possible later variant.

**Placement.** A *read* affordance, not extraction: **a `NodeRef` method** beside
`summary()` — lives with the node read API (P1: `core::node`), found exactly where its
one-line sibling is. `tree.root().render_tree()` covers the whole-tree case; no
`NodeTree` duplicate (one canonical path). Not `techy::extract` (this is display, not
content extraction); not a new `debug` module (vague-name family; `util` superseded).

**Naming** ([§dd-arch:naming]; nothing nearby in [§dd-dr:superseded-names]).
**`render_tree()` recommended** — "render → human-oriented String" is established crate
vocabulary (`Diagnostic::render`, `Diagnostics::render_all`); alternates
`subtree_summary()` (suggests one line), `display_tree()`. Returns `String`.
**Cost.** ~60 lines + tests; one stable method name; a second
not-stability-guaranteed output format (same caveat wording as summary()).

---

## E. Sugar batch — wishlist items 5–16 and 30 (numbering re-verified against SYNTHESIS §5)

Wishes 9/10/11 are F5b/F5a/F5c — ruled in A above. Wish 8 is T3-only (core-level
factory); wish 14 is dead (P2). Per-item, with the P1/P5/naming constraints applied:

| # | Wish (one line) | Recommendation |
|---|---|---|
| 5 | Registration one-liner (`define_macro`), `Arc`-free `insert`, `MacroSpec` code shorthand | **Accept** — see E1 |
| 6 | `MacroSpec::empty()`/`Default` for zero-arg macros | **Fold into E1** (`define_macro("qed", "")`); no standalone name |
| 7 | Named-code factory `[("o","greeting"), ("m","name")]` | **Accept** as sibling fn — see E2 |
| 8 | Core-level argument-spec shorthand (T3's wish) | **Route to T3 session** (core::specs vocabulary; T3 persona owns the evidence) |
| 9 | No-fallback argument code | Ruled in **A2** |
| 10 | Insert-time escape-name rejection / near-miss detail | Ruled in **A1** |
| 11 | Absent-vs-out-of-range distinction | Ruled in **A3** |
| 12 | `EnvironmentSpec::with_body_provider(Arc<Package>)` | **Reject** — see E3 |
| 13 | Canned text-mode-argument helper in latexlike | **Accept** — see E4 |
| 14 | `Language::with_providers([...])` | **Dead** — `with_provider` itself is removed by P2; the list form *is* `lang_initial_with_packages` (C2) |
| 15 | Generic `callable_name()` | **Reject as API — already exists**: `NodeRef::name()` (node/node_ref.rs:252) covers macros/environments/specials; guide gap only (T1 found it only by scanning source) |
| 16 | `NodeKind::label() -> &'static str` | **Accept** — see E5 |
| 30 | `Diagnostics::sorted_by_position()` | **Accept, narrow** — see E6 |

**E1 (wish 5+6).** Two layers, both recommended:
(a) *Fix the real API* (P2 spirit): `Package::insert`/`insert_specials`/`…_in_modes`
take the C2-style sealed conversion (`impl Into…CallableSpec`-like trait) so
`insert(CallableType::Macro, "emph", MacroSpec::new(…))` works — no `Arc::new` at any
call site, pre-Arc'd flyweight sharing still accepted (scopes/mod.rs:654/687 today
demand `Arc<dyn CallableSpec<L>>`).
(b) *Preset one-liners*: `define_macro(name, codes) -> Result<(), ArgumentCodeError>` and
`define_environment(name, codes)` as **inherent methods on `Package<LLL>`** in the
latexlike module (precedent: inherent preset sugar on `NodeRef`,
[§dd-dr:inherent-preset-sugar]; post-P3 the impl block is
`impl<LLL: LatexlikeLang> Package<LLL>`). They validate names (A1), pair spec types
correctly by construction (A4), and `define_macro("qed", "")` covers wish 6
(`argument_specs_from_str("")` already returns an empty vec). Honest con: (b) is a
second registration spelling — the objection that killed `with_provider`. The
distinction to argue: `with_provider` duplicated a *model-level* operation;
`define_macro` collapses a five-name literal ceremony
(`Arc::new(MacroSpec::new(argument_specs([…]).unwrap()))`) both T1 and T2 flagged, in
the module whose job is canned convenience. If (b) falls to one-canonical-path, (a)
alone still cuts the worst noise. While in there: fix the `insert` vs `insert_specials`
parameter-order flip (name 2nd vs trigger 1st, scopes/mod.rs:654/687) — align to
`insert_specials(callable_type, trigger, spec)`; breaking, free now (F3 nit).

**E2 (wish 7).** `ArgumentSpec::named` exists (spec/structure.rs:153) but composing it
with the factory means rebuilding specs around the public `.parser` field
(structure.rs:134) — the docs recommend names, the API fights them. A tuple-accepting
*single* factory hits real coherence walls (a blanket `AsRef<str>` impl and a tuple
impl on one sealed trait conflict). **Recommend a sibling
`argument_specs_named([("o","greeting"),("m","name")])`** (exact name to session;
the factory family already has deliberate list/compact duality, arguments.rs:106–134).

**E3 (wish 12).** `with_body_provider(pkg)` is pure spelling sugar over
`with_body_delta(ParsingStateDelta::new().push_provider(…))` (environments.rs:372) and
gets abandoned the moment a body also needs a mode change — the exact
abandoned-at-first-need pattern P2 rejected. The discoverability problem is real but is
solved by minidefs itself: \item-in-lists is the in-tree exemplar (B), plus a guide
section. **Reject.**

**E4 (wish 13).** The guide's own `\text{…}` recipe needs four vocabularies
(`GroupArgumentParser`, `TokenRulesOverrides`, `default_token_rules`, `GroupType` —
docs/learn-by-example.md:196–220): parser-internals for a top-five macro shape.
**Accept a latexlike factory** returning the canned spec
(`ArgumentSpec` + `with_state_delta`, structure.rs:159, is the existing mechanism).
Name candidates: `text_mode_argument()` (lean), `text_argument_spec()`. Post-P3:
`LLL`-generic like the rest of the factory family.

**E5 (wish 16).** T1 and T4 independently. A kind label requires a 5-arm match exposing
boxed payload shapes today (no label/Display on `NodeKind`, node/kind.rs). **Accept
`NodeKind::label() -> &'static str`** ("Chars"/"Group"/"Callable"/"Comment"/"List").
Not `name()` (collides with `NodeRef::name()` = callable spelling — sibling-vocabulary
rule, [§dd-arch:naming] #4); a `Display` impl on a payload-carrying generic enum is
odd. D's visualizer wants it internally anyway.

**E6 (wish 30).** Diagnostics arrive in recovery order (T1's came 1,4,3,2). Full
"position order" is ill-defined across multi-source trees (P4 makes `\input` forests
first-class). **Accept the narrow helper**: sort key = (source in first-appearance
order, span start), documented as source-order within each source; name candidates
`sorted_by_position()` (lean; adjective convention, naming #6) vs
`sort_by_position(&mut self)`. Deferring to the T4 session (owner of diagnostics
rendering + `\input`) is acceptable too — flag, don't fight. Stale claim corrected:
`Diagnostics` *does* have both `IntoIterator` impls (error.rs:573,582); doc gap only.

---

## F. Sweep — anything else routed to T1/T2

Checked every decision-log entry: the only extra T1/T2-routed item is P3's
`minidefs::package::<LLL>()` rider (in B). Explicitly *not* this session's:
resolution-family extraction/placement (P1 deferral, T3/T4 orbit — only A1's near-miss
*policy* touches it), wire-area renames (T4), role-accessor naming +
`LatexlikeDriver<LLL>` (T3/T5), restage/recompose (T5/dedicated), wishes 24–28 (T4),
wishes 17–22 (T3).

---

## Resolved by 2a — do not re-litigate

- **Wish 1** (`latexlike::parse`/`parse_tolerant`) + the **entry builder** — rejected by
  **P2** [§dd-dr:language-init] (no facade fns/builder; the constructor was fixed
  instead). The original "facade + builder naming" agenda line is void.
- **Wish 2** (`Language::with_recovery`/`tolerant()`) — superseded by **P2**: recovery
  is the driver's constructor argument; `Language` collapses to the constructor
  (+ resolver moving driver-ward per **P4**).
- **Wish 3** (standard definitions database) — rejected by **P2(b)**; replaced by
  minidefs [§dd-dr:minidefs]; the "defs extent" agenda item was deleted by the ruling.
- **Wish 4** (`extract::plain_text`) — rejected 2026-07-29 (decision log): debug tree
  visualizer (D) + scheduled techy-totext companion crate.
- **Wish 14** (`with_providers`) — moot per **P2**: `with_provider`/`with_seed_delta`
  leave the API; the list-taking path is `lang_initial_with_packages` (C2).
- **Curated-root promotion / dual paths** — killed by **P1** (one canonical path,
  [§dd-dr:public-namespace-topology]).
- **Conditions registry module** (F9's registry half) — rejected in **P1**; the
  identifier *rename slate* is T4's (**P5**).
- **Stability tiering** of anything accepted here — settled by **P5**: ordinary stable
  pub under the soft freeze; no experimental tier.

## Proposed order of presentation (most structural first)

1. **B rider 1** — base-package contents move + rename (changes default preset
   behavior; everything else in B hangs off it), then the rest of **B** (layout, spec
   choices, `LLL`-generic rider).
2. **A1–A4** — the F5 traps (headline finding; A1 sets the validation-layering
   principle that A4 and E1 reuse).
3. **C1–C2** — Language-init application details (C2's conversion-trait idiom is
   reused by E1 — decide the idiom once, here).
4. **E** — sugar batch (E1 first: largest and most contentious under
   one-canonical-path; then E2–E6 quickly).
5. **D** — visualizer (self-contained; naming + shape only).
6. **F + resolved-by-2a list** — confirmations, and route wish 8 to T3.
