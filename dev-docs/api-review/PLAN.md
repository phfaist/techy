# Public API Review — Master Plan & Status

Working scaffolding for the public-API review (RESTRUCTURE_MAP-style: this directory is
deleted when the review completes). **This file is the master plan: a fresh agent/session
resumes the review by reading this file first**, then the files it points to (§ Working
files). Final design decisions are recorded in dev-docs/DESIGN_RATIONALE.md as usual;
this file tracks process state and interim findings.

## Scope framing (revised 2026-07-29)

techy is a **backend library**, not a plug-in end-user tool (nobody has immediate use
for a raw parsed AST). Primary consumers are **frameworks built on top of techy**:

- (i) a rewritten/extended **latex2text**-type conversion tool (replacing pylatexenc's
  latex2text backend);
- (ii) **FLM** (Flexible LaTeX-like Markup);
- (iii) a **latexpp**-style preprocessor / bulk manipulator of LaTeX documents
  (https://github.com/phfaist/latexpp — needs source-faithful reconstruction).

Consequences: the former T1 "few obvious calls" goal is **scaled back** (direct document
consumers are secondary); the framework-builder persona (T5, below) is added and given
heavy weight; guides and API curation follow **access-tier logic, not frequency of use**.

## Goals

1. **Stable API, converged NOW**: techy has zero dependents today — restructuring is
   allowed and *encouraged* now if it prevents being forced into breaking restructuring
   later, once frameworks depend on techy. The success criterion is that framework
   development on top of techy never forces a techy restructuring.
2. **Guides** for humans and AI agents, usable without reading techy internals.
3. **Tier-logical organization**: exports organized by access tier (what level of task
   reaches for them), not by usage frequency. Advanced surfaces must be a logical,
   structured reach (submodules welcome).

## Personas (access tiers)

- **T1 — document consumer**: parse latexlike, walk/query AST, extract, diagnostics.
  (Secondary since the 2026-07-29 reframe, but still a real tier.)
- **T2 — extender**: custom macros/environments via specs, packages, scopes.
- **T3 — language designer**: custom Lang, token rules, construct parsers, state deltas.
- **T4 — tooling author**: source model, spans, provenance, resolvers, line/col.
- **T5 — framework builder** (added 2026-07-29): builds an extensive framework on techy
  (latex2text/FLM/latexpp-class), likely exposing a **Python** API (chosen over JS:
  pylatexenc/FLM ecosystem). Concerns: FFI boundary (ownership/lifetimes/Send/Sync of
  techy types across PyO3), node-tree transformation infrastructure, source-faithful
  reconstruction, definitions databases, long-term path stability.

## Constraints (revised 2026-07-29)

- **Restructuring is in-scope now** (modules, exports, even crate splits) — the point of
  the review is to avoid *future* restructuring. Prior "no restructuring" wording was a
  misunderstanding.
- Renames still follow the naming principles (dev-docs/ARCHITECTURE.md [§dd-arch:naming])
  and the superseded-names register (DESIGN_RATIONALE [§dd-dr:superseded-names]).
- All decisions are the user's; agents/briefs recommend, never rule ("ask before design
  decisions", CLAUDE.md).

## Phases & status

- [x] **Phase 0 — Inventory** (done 2026-07-28): 205 public items (203 techy + 2 derive);
  140 root re-exports; 100% doc coverage; 38 #[non_exhaustive].
  → INVENTORY.md.
- [x] **Phase 1 — Persona walkthroughs T1–T4** (done 2026-07-28): all four personas
  succeeded; API judged capable everywhere, gaps are convenience + docs. Per-persona
  friction logs and API surfaces under walkthroughs/. Headlines: T1 ~41 names
  (needs facade+defs+extract helpers); T2 ~50 (traps F5; ceremony F3); T3 ~55 (organization
  meets the bar, entry path doesn't; SimpleLang dead-end; no custom-Lang guide); T4 ~100
  (zero implementation-body reads; missing cursor primitive + \input wiring; exemplary
  provenance/diagnostics).
- [x] **Phase 1s — Cross-persona synthesis** (done 2026-07-28): SYNTHESIS.md. 73-item
  union of 205; cumulative tiers T1=24 / ∪T2=29 / ∪T3=67 / ∪T4=73; 76/140 root
  re-exports untouched, root-as-path traffic ≈ 0; friction themes F1–F13 (F5 traps worst
  individual finding; F7 cursor primitive; F8 \input wiring); 30-item wishlist (20 pure
  sugar).
- [x] **Phase 1b — T5 framework-builder walkthrough** (done 2026-07-29): deliverables
  copied to walkthroughs/framework/ (FRICTION.md boundary table, FRAMEWORK-ANALYSIS.md,
  API-SURFACE.md; runnable probes + working PyO3 module in scratchpad
  walkthroughs/framework/{probes/,techy-py/}). Verdicts: clean Python API buildable
  TODAY (all ownable types 'static+Send+Sync; Arc<NodeTree>+NodeId→tree.get(id)
  round-trip; PyO3 abi3 smoke test passes); latex2text archetype sufficient (needs defs
  DB; \alpha post_space re-emission gotcha); latexpp archetype VERIFIED byte-faithful
  (span gap-filling, incl. tolerant-recovery nodes; targeted rewrite works); FLM split —
  preset-fork cliff (LatexlikeDriver/MacroSpec/default_token_rules/base_package all
  Latexlike-monomorphic; custom-Lang FLM forfeits the preset; node-ext system reachable
  only via custom Lang). Transform gaps: subtree copy blocked by crate-private region
  translation (RegionAlreadyResolved), no BuildId→NodeId map from finish(), no
  parent()/index_in_parent(), no mixed-origin validator, NodeSlice::source_text() can go
  silently stale on spliced trees. Top-5 ranked needs in FRAMEWORK-ANALYSIS.md.
- [x] **Phase 2a-prep — Namespace topology evaluation** (done 2026-07-29):
  NAMESPACE_OPTIONS.md (674 lines, copied to dev-docs/api-review/). Verified facts:
  flat machinery facade is collision-free today (0 clashes across 180 S0/S1 items, 0 vs
  latexlike's 23); wire diagnostic identifiers already use core.*/latexlike.* area
  prefixes (anchors the name `core` over `parsing`; `core`'s extern-prelude-shadowing
  con is loud-not-silent); techy-derive's emitted ::techy::error::… paths should be
  rerouted through __private unconditionally (removes derive as a topology constraint).
  Structural insight: root re-exports are additive+location-independent, so tiered =
  flat facade + curated root on top; promotion to root is forever additive. Migration
  ≈ 97 use-line edits + lib.rs + ~13 derive sites. Ranked candidates: R1 = tiered
  (~18-name curated root + extract + latexlike + flat techy::core; 36/40), R2 = pure
  flat facade (core+latexlike only; 32), R3 = structured core::{source,…} (28–31.5;
  re-freezes the 9-topic taxonomy, already revised once). Riders: defs as in-crate
  techy::latexlike::defs module, no feature gate (pylatexenc DB ≈ 633 lines/253 specs,
  sub-second compile; reversible to crate later via re-export); techy::util REJECTED on
  naming principles.
- [x] **Phase 2a — Policy session** (interactive, complete 2026-07-31; POLICY_BRIEF.md v2 +
  v1 archived; NAMESPACE_OPTIONS.md + CORE_SPLIT_OPTIONS.md are the P1 evaluations).
  **P1 RULED** (topology: C5 + core::specs — see decision log +
  [§dd-dr:public-namespace-topology]; the briefs' P1/D1/D2 recommendations are
  superseded by the ruling). **P2 RULED** (Language-init revision + minidefs — see
  decision log + [§dd-dr:language-init], [§dd-dr:minidefs]; the briefs' facade/defs
  recommendations are superseded). **P3 RULED** (preset generalization: role traits +
  `LatexlikeLang`, `Lang` stays whole, `GroupType::Math(MathGroupForm)` — see decision
  log + [§dd-dr:latexlike-generalization], [§dd-dr:math-group-form]). **P4 RULED**
  (transformation & navigation — annotations, tree tags, ext minting, restage,
  recompose, slot roles, `\input`, navigation; see decision log + **P4_RULING.md**
  (full working detail) + DESIGN_RATIONALE topic [§dd-dr:transform]). **P5 RULED**
  (stability rubric + wire identifiers: one stability class, soft freeze; see decision
  log + [§dd-dr:stability-rubric], [§dd-dr:wire-identifier-stability]). Routing to 2b
  sessions: POLICY_BRIEF last section, as amended by the decision log.
- [x] **Phase 2b — Decision sessions by access tier** (COMPLETE 2026-08-03; **T1/T2 session
  RULED 2026-07-31** — brief T1T2_BRIEF.md, rulings T1T2_RULINGS.md + decision log +
  six new DESIGN_RATIONALE entries; **T3 session RULED 2026-07-31** — brief
  T3_BRIEF.md, rulings T3_RULINGS.md + decision log + seven new DESIGN_RATIONALE
  entries; both P1 deferred placements ruled → **Phase 3 topology unblocked**;
  **T4 session RULED 2026-07-31** — brief T4_BRIEF.md, rulings T4_RULINGS.md +
  decision log + three new DESIGN_RATIONALE entries incl. the frozen
  wire-identifier slate; **T5 session RULED 2026-07-31** — brief T5_BRIEF.md,
  rulings T5_RULINGS.md + decision log + three new DESIGN_RATIONALE entries;
  **recompose session RULED 2026-08-03/04** — brief RECOMPOSE_BRIEF.md, rulings
  RECOMPOSE_RULINGS.md + decision log + three new DESIGN_RATIONALE entries;
  **Tier-C batch RULED 2026-08-03** — brief TIERC_BRIEF.md, rulings
  TIERC_RULINGS.md + decision log + two new DESIGN_RATIONALE entries).
  Per-item
  rulings (promote / keep-off-root / pub(crate)) over the 66 no-usage-signal items
  (SYNTHESIS §3); trap fixes F5a–d (T2 session); cursor primitive F7 + \input wiring F8
  + FS-trait option (T4 session); SimpleLang role + on-ramp cliffs F10 (T3 session);
  sugar batch (wishlist; distribute); T5 session: transformation-infra scope, FFI-driven
  API needs (owned handles vs lifetimes), reconstruction guarantees.
- [x] **Phase 3 — Apply + harden** (done 2026-08-05; S1–S10 all merged): all
  2a/2b rulings applied; guards live (missing_docs deny; cargo-semver-checks
  vs the movable `api-baseline` branch); full audit exact. Stage plan +
  status + per-stage logs: PHASE3_PLAN.md; reports under
  dev-docs/api-review/reports/.
- [ ] **Phase 4 — Guides** (agent-drafted, user-reviewed), written from public docs only
  (needing source = doc gap):
  - Human guide: docs/ chapters + per-tier cookbook; **framework-builder chapter** incl.
    bindings guidance.
  - AI-agent guide: one dense self-contained doc (task → recipe → code, pitfalls).
  - Migration guides pylatexenc v2/v3 → techy: human (high-level + pointers) and AI
    (dense mapping tables). pylatexenc sources: `$HOME/Research/util/pylatexenc/`.

## Companion projects & feature ideas (tracked here, decided in 2a/2b)

- **Debug tree visualizer in techy**: preformatted-ASCII node-tree display, better than
  summary(). Intent: alleviates T1's plain-text-extraction wish (wishlist #4). An
  elaborate extract::plain_text is REJECTED in spirit: little more than a debug dump,
  yet far short of a real latex2text — that gap belongs to techy-totext.
- **techy-totext companion crate** (scheduled project, separate from this review):
  latex2text-inspired-but-better conversion framework — options, overridable
  definitions, spacing rules, text wrapping, equations, etc.
- **Node-tree transformation framework** (module or crate; assess in T5/2b): general
  infrastructure for tree→tree transformations; to-text then = transformations ending in
  string nodes + concatenation; useful for FLM. Open Qs: immutable flat NodeTree →
  rebuild API, node identity/provenance across transforms.
  CLOSED (P4/T5/recompose sessions): restage ruled ([§dd-dr:restage],
  [§dd-dr:restage-ops]); recompose is a direct value fold — the
  to-text-by-concatenation route is demoted to a documented restage→recompose
  pipeline pattern ([§dd-dr:recompose-machinery]).
- **\input / file-system resolution**: CLOSED (T4 session, 2026-07-31) —
  `SourceResolver` verified to already BE the minimal filesystem-interface trait;
  techy ships nothing beyond the new source-side helpers
  ([§dd-dr:include-chain-helpers]); the std FS-resolver recipe lands in Phase 4's
  include chapter. Engine wiring ruled: [§dd-dr:input-wiring].

## Working files

Repo (durable), all under dev-docs/api-review/:
- PLAN.md — this file (master).
- PHASE3_PLAN.md — Phase 3 execution plan: stage breakdown S1–S10 (worktree
  protocol, per-stage ruling inputs, acceptance gates, stage log).
- PHASE4_PLAN.md — Phase 4 execution plan (RULED 2026-08-05): guide structure
  (User/Developer/AI categories), method rules incl. the no-assumption DOC_GAPS
  protocol, size targets, rehomings, stage breakdown G1–G5, per-stage protocol.
- DOC_GAPS.md — Phase 4 register of documentation gaps (GAP) and
  behavior-verification notes (CHECK); created in G1, fully resolved by G5.
- INVENTORY.md — Phase 0 full item inventory (+ provisional tier tags; see SYNTHESIS §3
  for empirical corrections).
- SYNTHESIS.md — cross-persona matrix, unused-list, friction themes F1–F13, wishlist.
- POLICY_BRIEF.md — **v2 (2026-07-29), current decision brief** (P1–P5 + 2b routing);
  POLICY_BRIEF_v1.md — archived v1.
- NAMESPACE_OPTIONS.md — export-topology evaluation (options, verified facts, R1–R3).
- P4_RULING.md — the frozen P4 ruling in full working detail (12 points + deferred
  agenda + naming decisions); durable records are the DESIGN_RATIONALE
  [§dd-dr:transform] entries it lists.
- T1T2_BRIEF.md — Phase 2b T1/T2 session decision brief (F5 traps, minidefs
  application + base-package rider, P2 application details, visualizer, sugar batch).
- T1T2_RULINGS.md — the frozen T1/T2 session rulings in full working detail (B, A1–A4,
  C1–C2, E1–E6, D incl. the E4 enclosing-state-stack design); durable records are the
  DESIGN_RATIONALE entries listed in the decision log.
- T3_BRIEF.md — Phase 2b T3 session decision brief (prepared 2026-07-31, verified
  against e5b994b: SimpleLang role A, on-ramp F10 B, takeover staging F11 C,
  preset-driver architecture D, role-accessor naming + ClosedVocabulary E,
  StdParseDriver::default F, wishes 17–22 + 8 G, resolution-family extraction H).
- T3_RULINGS.md — the frozen T3 session rulings in full working detail (H, D, E,
  A+F, B, C+G, sweep incl. the ArgumentParser-placement ruling); durable records
  are the DESIGN_RATIONALE entries listed in the decision log.
- T4_BRIEF.md — Phase 2b T4 session decision brief (prepared 2026-07-31, verified
  against 9643d7d: \input wiring + resolver move B, FS-trait closure C,
  wire-identifier rename slate A incl. full 22-condition inventory, navigation
  naming E, cursor reconciliation D, wishlist sweep F).
- T4_RULINGS.md — the frozen T4 session rulings in full working detail (B incl.
  the parser-parameter door amendment + check_include_chain + LineColProvider
  design evolution, C closure, A's frozen slate, E/D naming, F sweep); durable
  records are the DESIGN_RATIONALE entries listed in the decision log.
- T5_BRIEF.md — Phase 2b T5 session decision brief (prepared 2026-07-31, verified
  against 4c324c7 with the Phase-1b probes re-run: restage detailing A, wish-20
  `stage_invocation` B, FLM projection acceptance C incl. the C1 event-role-trait
  gap between P3 and E4, driver knobs D, pillar-signature sufficiency E,
  validator + honest slices F, cached-splice G, scope/FFI/reconstruction H,
  walkthrough sweep I; projected FLM probe copied to
  walkthroughs/framework/flm_projected.rs).
- RECOMPOSE_BRIEF.md — recompose design-session decision brief (prepared 2026-07-31,
  verified against 3ae9c67: substrate table = per-node-kind re-emission inventory,
  doctrine operationalization, trigger-spelling residue design S1, direct-fold
  architecture, walker, state/sink, targeted replacement, Attached-exclusion,
  naming Qs 1–7, recommendations R1–R15).
- RECOMPOSE_RULINGS.md — the frozen recompose-session rulings in full working
  detail (Round 1 doctrine, Round 2 trigger-spelling storage incl. the CallSyntax
  rejection and the InvocationSyntax mechanism, Rounds A–D machinery/walker/scope/
  naming sweep); durable records are the DESIGN_RATIONALE entries listed in the
  decision log.
- T5_RULINGS.md — the frozen T5 session rulings in full working detail (A1–A9
  incl. the user-revised A8 extract-annotation design, B, C, E's
  `ParsingStateStack` design, D, F incl. the `core::node` home ruling, G's
  no-caching closure, H incl. the withdrawn reconstruction guarantee and the
  per-node recomposition doctrine, I sweep); durable records are the
  DESIGN_RATIONALE entries listed in the decision log.
- TIERC_BRIEF.md — Tier-C batch decision brief (prepared 2026-08-03, verified against
  6326db2: 12/76 items already ruled by later sessions; 8 decision groups G1–G8 with
  forced-pub analysis; ~50 forced/doctrine-bound, ~11 genuine judgment calls; riders
  R1–R5; full 76-row sweep table; proposed round order).
- TIERC_RULINGS.md — the frozen Tier-C session rulings in full working detail
  (Rounds 1–2 ratification blocks, Round 3 judgment calls incl. the
  check_tree_invariants-over-validate_tree wrapper shape, Round 4 placements,
  Round 5 riders incl. the reopened-and-re-ruled R4 command-resolver design,
  closing sweep); durable records are the DESIGN_RATIONALE entries listed in the
  decision log.
- walkthroughs/{consumer,extender,langdesign,tooling,framework}/ — FRICTION.md +
  API-SURFACE.md (+ example code; framework/ adds FRAMEWORK-ANALYSIS.md) per persona.

Scratchpad (session of 2026-07-28/29, survives on disk; copy durables into repo):
`/private/tmp/claude-501/-Users-philippe-projects-techy/3b71ab8b-6cf7-4ab7-83d6-1a1d982076fb/scratchpad/api-review/`
— raw rustdoc JSON + extraction scripts, per-agent PROGRESS.md checkpoint files, and the
runnable walkthrough cargo projects (consumer/, extender/extender-examples/,
langdesign/notely/, tooling/techy-tooling/, framework/{probes/,techy-py/}).
The 2026-07-31 T5-brief session's scratchpad (probe re-runs at 4c324c7 +
projected FLM probe) is at
`/private/tmp/claude-501/-Users-philippe-projects-techy/e071f0ca-9642-4b1e-b093-efb9232f838b/scratchpad/api-review-t5/`.

## How to resume with a fresh agent/session

1. Read this file; check phase checkboxes and the Working files list.
2. In-flight agent outputs land in the scratchpad path above (each with its own
   PROGRESS.md checkpoint file — resumable mid-run); copy finished durables into
   dev-docs/api-review/ and update this file's status.
3. Interactive phases (2a/2b): prepare/refresh the brief from the named inputs, present
   options + recommendation, get the user's ruling, record it in the Decision log below
   AND as a DESIGN_RATIONALE.md entry (per its template, with ARCHITECTURE reference).
4. Nothing in this review is committed until the user says so.

## Decision log

- 2026-07-29 (user): scope reframed — techy is a backend for frameworks
  (latex2text/FLM/latexpp-class); T5 persona added (Python bindings); "few obvious
  calls" T1 goal scaled back.
- 2026-07-29 (user): restructuring is allowed NOW (no dependents); the constraint is
  avoiding FUTURE restructuring.
- 2026-07-29 (user): exports follow access-tier logic, not frequency of use.
- 2026-07-29 (user): elaborate in-techy plain-text extraction rejected in favor of a
  debug ASCII tree visualizer (in techy) + scheduled techy-totext companion crate.
- 2026-07-29 (user, P1 partial rulings): **(1) Single canonical path per item** — no
  redundant spellings, no curated-root promotion, path determined by logical
  function/use (not frequency, not internal machinery). NAMESPACE_OPTIONS R1/O3
  (tiered root) REJECTED on this ground. (2) Leaning **single flat facade**, but
  concerned `core` (~180 items) is too big — commissioned critical evaluation of
  splitting core into 2–4 function-based public parts (user sketch: lang-related
  [token, lang, driver…] / parsers library [constructs, expression, std argument
  parsers, verbatim…] / definitions [callable specs, scopes, packages] / node-related
  [nodetree]) → CORE_SPLIT_OPTIONS.md (agent launched). (3) Facade name **`core`**
  over `parsing`. (4) **Derive-`__private` rider ratified** (all techy-derive emitted
  paths through `#[doc(hidden)] __private`).
- 2026-07-29: CORE_SPLIT_OPTIONS.md delivered (copied to dev-docs/api-review/).
  Key results: seven "straddle families" explain all stragglers under every taxonomy
  ([§dd-dr:three-strata] mutual recursion); conditions registry (core::conditions,
  all 22 core condition types) must be decided NOW under every candidate incl. flat
  (one-canonical-path forbids additive retrofit); C2 pipeline rejected on
  three-strata grounds; C1-as-sketched ≈ C4 with hard decisions left implicit
  (~35 stragglers); name repairs: `specs` not `definitions` (collides with planned
  latexlike::defs DB), `parsing` not `parsers`. Ranked: R1 = C4 Variant B
  (source/error top-level + core::{lang,specs,parsing,node} + core::conditions +
  node::extract, engine nine in parsing; ~95–98% obvious-home, largest page 42);
  R2 = C3 two-way (core::{lang,parsing}; ~97% but ~70-name page, coarseness
  permanent); R3 = C0 flat (~150 names, syn-precedented, zero freeze).
  Flat-vs-split is a one-shot symmetric-regret value trade (neither direction
  reachable additively later). Awaiting user ruling on the 6 packaged sub-decisions
  (CORE_SPLIT_OPTIONS §8).
- 2026-07-29 (user): counterproposal **C5 — hub + extracted subsets** (top-level
  source/error/extract, core as flat hub [lang/state/token/specs/scopes/engine],
  satellites core::constructs + core::node, future top-level transform; conditions
  registry REJECTED — open family, producer coupling, error-logic split; F9 doc
  registry via DiagnosticInfo implementors page + guide table instead). Evaluation
  appended to CORE_SPLIT_OPTIONS.md §9: C5 scores highest of all candidates (~36);
  supersedes C4 as recommendation; registry rejection conceded as sound.
- 2026-07-29 (user): **P1 RULED — final topology**: C5 **with `core::specs`
  extracted**. Layout: techy::{source, error, extract} top-level (+ future
  techy::transform); techy::core = flat hub (Lang/state, token, engine incl.
  resolution); satellites core::{constructs, specs, node}; latexlike unchanged;
  conditions producer-side (registry rejected); satellite name `constructs`;
  boundary rule RECORDED: **specs = author-side, hub = run-side**. Durable record:
  DESIGN_RATIONALE **[§dd-dr:public-namespace-topology]** (+ ARCHITECTURE
  [§dd-arch:arch] reference + superseded-names additions: util, parsing-as-namespace,
  definitions-as-specs-group-name, conditions-registry).
  Deferred to 2b (explicitly): (a) **extract std-command-resolution-via-scopes into a
  standalone opt-in function** (expected home: specs) — the resolution family
  (CommandResolution, ResolvedCallable, CallableQuery, CallableSyntax,
  SearchedProviders) gets its final placement beside that resolver AFTER this design;
  (b) ArgumentParser trait: specs vs constructs. **Sequencing consequence: the Phase 3
  topology application waits for (a)** so the resolution family lands once.
- 2026-07-29 (user): **P2 RULED — Language init revision + minidefs** (durable
  records: DESIGN_RATIONALE **[§dd-dr:language-init]** and **[§dd-dr:minidefs]**, with
  ARCHITECTURE footer refs + superseded-names additions; amendment notes appended to
  [§dd-dr:language-parse-api] and [§dd-dr:with-provider]).
  (a) NO facade fns/builder — instead fix the real API: `Language::new(driver,
  initial_state)` with initial_state MANDATORY; rename `ParsingState::initial()` →
  `lang_initial()`; add infallible `ParsingState::lang_initial_with_packages(vec![…])`.
  Verified sound: the seed never ran finalize_transition (parsing_state.rs:58–68), and
  direct provider pushes involve no by-name scope ops → infallibility holds; choke
  point untouched. Expected consequence (confirm at application): `with_provider` +
  `with_seed_delta` removed — delta customization spells
  `Language::new(driver, ParsingState::lang_initial().derived(delta)?)`; surface
  collapses to constructor + `with_resolver`. Open application details: `Default`
  impl fate; packages-arg ergonomics (avoid Arc noise).
  (b) Standard-definitions database REJECTED for techy (positioning: preset parses
  latexlike *content*, not LaTeX *documents*; frameworks roll their own). Instead:
  `techy::latexlike::minidefs`, single package `"minilatex"`: \emph, \textbf,
  \textit, itemize, enumerate (+ \item scoped inside the two list envs — body-scoped
  definitions exemplar). NO binding reference from other latexlike modules
  (dead-strippable). 2b T1/T2 agenda updated: defs-extent item DELETED; minidefs
  application added.
- 2026-07-30 (user): **P3 RULED — latexlike preset generalization** (in scope; shape
  A). Durable records: DESIGN_RATIONALE **[§dd-dr:latexlike-generalization]** and
  **[§dd-dr:math-group-form]** (+ amendment note on [§dd-dr:group-taxonomy],
  superseded-names additions, ARCHITECTURE [§dd-arch:latexlike] note).
  (a) **Per-vocabulary role traits** (method-based, implemented by the vocabulary
  types; techy implements them for its own enums, so adopting the preset enums
  satisfies the bounds with zero code) + **`LatexlikeLang`** umbrella trait with
  defaulted behavior methods (generalizing the `$` merge and the delimiter data);
  parameter convention `LLL`; NO blanket impl (defaults must stay overridable).
  (b) **`Lang` stays whole** — facet decomposition rejected in all three Rust
  realizations (supertrait / marker-blanket / strategy types; recorded with killing
  flaws); preset behaviors ship as public `LLL`-generic **pillar functions**; the
  one-line hook delegation is the irreducible composition mechanism (strata rule).
  (c) **`GroupType::Math(MathGroupForm)`**: inline/display as class payload declared
  at rule registration; `MathGroupForm` exhaustive (`MathStyle`/`math_style()`
  superseded — "style" collides with typesetting style); `math_form()` sugar
  table/state-free; `is_math`/`math_form` split; payload-admission rule recorded;
  `MATH_DELIMITERS` dissolves into `default_token_rules`. Preset stays
  `NodeExts = ()` (the framework owns the ext budget).
  2b agenda additions: role-accessor naming incl. `macro` keyword (T3 naming);
  `ClosedVocabulary` as role-trait supertrait?; `latexlike.*` wire identifiers inside
  foreign-`Lang` parses (P5); `LatexlikeDriver<LLL>` vs extracted driver core (T3/T5);
  generic `minidefs::package::<LLL>()` (T1/T2). Acceptance: re-run T5's FLM probe
  (custom `Lang` with node exts reusing driver, spec types, token rules, base
  package).
- 2026-07-31 (user): **P4 RULED — transformation & navigation** (6-round interactive
  session + confirmation; full detail frozen in **P4_RULING.md**; durable records:
  DESIGN_RATIONALE topic **[§dd-dr:transform]** with entries
  [§dd-dr:node-annotations], [§dd-dr:tree-tags], [§dd-dr:ext-minting],
  [§dd-dr:restage], [§dd-dr:recompose], [§dd-dr:slot-roles],
  [§dd-dr:input-attachment], [§dd-dr:tree-navigation] + amendment notes on 12 prior
  entries + superseded-names additions; ARCHITECTURE notes in [§dd-arch:nodes] and
  [§dd-arch:engine]). Headlines: (1) **annotations** `NodeTree<L, A = ()>` —
  consumer-owned per-node data, parallel-`Vec` over `Arc`-shared core, zero-copy
  `annotate()`; (2) **tree tags always-on** (`TreeTag` u32 in `NodeId` `Eq`/`Hash`);
  (3) **ext minting**: `finalize_node` deleted → required value-returning
  `Lang::make_node_ext` (parse-once, `StagedChildren` subtree-deep view); tier-2
  per-kind node exts REMOVED; non-defaultable `NodeExt`; hook-free single-`add`
  builder; parse staging only via `cx.stage_node()` (`ParserSession::builder` →
  pub(crate)); `ArgumentExt` kept — parser-minted, std parsers `where
  ArgumentExt<L>: Default`; `SlotExt` minted at construction via
  `BodySlotExt::make_body`; (4) **restage driver** in `techy::transform` —
  `Restage::{Continue(B), Emit}` (Continue-always-descends), region bundles +
  `restage_argument/slot/invocation`, read-frozen/write-staged rule,
  origin-by-convention (auto-provenance REJECTED), no `finish()` id map;
  (5) **`techy::recompose`** ratified (downward-state fold; span-verbatim +
  node-data strategies; own design session pending); (6) **`SlotRole`
  {Content, Attached, Hidden}** + trait-based body marking (P3's `NodeExts = ()`
  restated per-member — preset claims `SlotExt`); (7) **`\input`** = same-builder
  sub-parse into an `Attached` slot; multi-source parse trees first-class;
  resolver moves `Language` → driver (P2 surface amendment; T4 session);
  (8) **navigation**: stored parent table + `parent()`/`index_in_parent()`,
  `SourcePos` type, deepest-node/covering-slice reverse lookup, honest slices +
  transform-tier validator. **2b agenda amendments**: T5 session gains restage
  detailing (op/bundle shapes, region-edit policies, builder-`add` ergonomics,
  naming incl. `Restage::Continue` alternates, Split/KeyVals-on-restage option);
  NEW dedicated **recompose design session**; T4 session gains `\input` engine
  wiring + resolver move + lookup naming. Application lands in Phase 3 together
  with the P1 topology move.
- 2026-07-31 (user): **P5 RULED — stability rubric + wire identifiers** (durable
  records: DESIGN_RATIONALE **[§dd-dr:stability-rubric]** and
  **[§dd-dr:wire-identifier-stability]** + amendment note on
  [§dd-dr:condition-identities]; ARCHITECTURE refs in [§dd-arch:arch] and
  [§dd-arch:errors]). (1) **One stability class** for everything `pub` (outside
  `__private`); no unstable/experimental tier; access tiers expressed by placement +
  guides, not stability levels; Tier-C per-item 2b rulings = pub-and-stable vs
  pub(crate). (2) **Soft freeze** (user amendment to the brief): the freeze takes
  effect when the Phase 3 restructuring lands (cargo-semver-checks baseline guard;
  0.x discipline: breaking → 0.(x+1).0), but it is NOT absolute — important
  shortcomings may still be fixed breakingly until significant frameworks are
  actually being built on techy; hard freeze begins with framework adoption. Guides
  print paths/identifiers only post-restructure. (3) **Wire identifiers
  semver-stable** under the same rubric; per condition: identifier hard-stable +
  serializable_data keys additive-only; Display wording explicitly NOT stable.
  `<area>` rule: names a construct concept/subsystem, never a file/module/type name
  (F9 repair — 14 of 18 core.* identifiers currently use internal file names);
  concrete rename slate → 2b T4 session (nodes_parser conditions interact with the
  deferred resolution-family extraction); repair lands in Phase 3 before guides.
  (4) **First segment = defining vocabulary** (core.*/latexlike.*/flm.*); preset
  conditions keep `latexlike.*` inside foreign-`Lang` parses (P3-routed question —
  identifier names the raising machinery, not the parsed language); lang-dependent
  identifiers REJECTED; one-time pre-freeze re-homing rides with the P3 application
  (types relocated preset→core). **Phase 2a complete.**
- 2026-07-31 (user): **2b T1/T2 SESSION RULED** (interactive; full working detail
  frozen in **T1T2_RULINGS.md**; durable records: DESIGN_RATIONALE new entries
  **[§dd-dr:enclosing-state-stack]**, **[§dd-dr:registration-ergonomics]**,
  **[§dd-dr:argument-factory-additions]**, **[§dd-dr:named-argument-errors]**,
  **[§dd-dr:display-tree]**, **[§dd-dr:diagnostics-position-sort]** + amendments on
  [§dd-dr:base-package], [§dd-dr:minidefs], [§dd-dr:language-init] +
  superseded-names additions + ARCHITECTURE footer refs). Headlines:
  (1) **base package → `"_builtin"`/`builtin_package()`**, slimmed to \begin/\end;
  `&` removed entirely; `~` + ligatures move to minilatex (preset default-shape
  change accepted). (2) **minidefs**: `minilatex_package()` (LLL-generic target),
  specs as briefed, inner `"minilatex.item"` package. (3) **F5 traps**: NO
  insert-time validation anywhere (escape chars can change mid-parse; `@greet`
  legitimate) — instead did-you-mean resolution detail + parse-init all-escape-char
  package warning + docs; `"BracedOnly"` word code (content-class group, no
  fallback); `_named` accessors return `Result` (unknown name = error, absent =
  `Ok(None)`); no spec/type cross-check (documented-legitimate). (4) **Language
  init**: `Default for Language` + `LatexlikeDriver::default()` removed; sealed
  `IntoSpecsProvider` conversion (also on `Package::insert`; param-order flip
  fixed). (5) **Sugar**: `define_macro`/`define_environment` one-liners
  (shorthand-not-second-path principle recorded); `argument_specs_named`;
  `NodeKind::as_str()`; `sorted_by_position()`; `with_body_provider` REJECTED;
  wish 15 = guide gap; wish 8 → T3. (6) **E4 (major design)**: text-mode arguments
  via preset restore *event*; **enclosing-state stack on the session** (rejected:
  mode-visibility-on-GroupRule — mode semantics unclaimed in core; ParsingState
  parent pointer — history residue in parsed material); two-level event
  consumption: fallible `finalize_transition` (kept, per placement doctrine) +
  `cx.derive_state` lowering context events via new driver hook
  `resolve_state_event(&event, &StateStackView)`; preset event logic (math entry,
  text restore) extracted as **public pillar functions** so post-parse processing
  can synthesize coherent recorded states (transform tie-in); guide `\text` recipe
  bug (forbidden_chars/groups clobber) fixed at application.
- 2026-07-31 (user): **2b T3 SESSION RULED** (interactive; full working detail
  frozen in **T3_RULINGS.md**; durable records: DESIGN_RATIONALE new entries
  **[§dd-dr:resolution-extraction]**, **[§dd-dr:preset-driver-pillars]**,
  **[§dd-dr:trivial-lang]**, **[§dd-dr:on-ramp-defaults]**,
  **[§dd-dr:scopes-resolving-driver]**, **[§dd-dr:takeover-staging-sugar]**,
  **[§dd-dr:named-first-constructors]** + amendments on
  [§dd-dr:enclosing-state-stack], [§dd-dr:argument-factory-additions],
  [§dd-dr:latexlike-generalization], [§dd-dr:iter-symbols],
  [§dd-dr:registration-ergonomics], [§dd-dr:language-init],
  [§dd-dr:public-namespace-topology] + superseded-names additions + ARCHITECTURE
  footer refs). Headlines: (1) **H**: resolution extraction ratified — free fn
  **`resolve_command_in_scopes`** in `core::specs` (user naming: "in", not "via");
  whole family (`CommandResolution`, `ResolvedCallable`, `CallableQuery`,
  `CallableSyntax`, `SearchedProviders`) moves beside it. (2) **D**: preset driver
  = pillar functions + **`LatexlikeDriver<LLL>`** canned assembly, layered; **user
  amendment**: `restore_text_context_delta` → **`exit_math_context_delta`** (first
  non-math enclosing group in the stack; never names text mode). (3) **E**:
  accessors `macro_callable()`/`environment_callable()`/`specials_callable()` +
  `is_*` predicates; mode role trait trimmed to `math_mode()` + `is_math()` (no
  text-mode constructor); **`ClosedVocabulary` NOT a supertrait** — "provide,
  don't require" (brief's A1(ii)-needs-it claim corrected: no enumeration
  dependency; A1(iv) = bound-where-used check fn). (4) **A+F**: `SimpleLang` →
  **`TrivialLang`**, kept public as the test lang (wish 18a rejected);
  `StdParseDriver::default()` removed. (5) **B**: `TokenRules::empty()` +
  `StateData::empty()` (user naming, not `neutral`); **specials defaults stay
  recognize-nothing** (user; rejects the brief's scope-fold default —
  simple-by-default, opt-in dead-code elimination; move-to-driver closed as a
  strata violation); wish 18b accepted as **`ScopesResolvingDriver`** (user
  naming, plural). (6) **C+G**: `TokenRulesOverrides::disable_all()`;
  `ParsedArguments::new(Vec)`/`ParsedSlots::new(Vec)`; wish 20 commitment-only
  (`cx.stage_invocation`, signature ruled in T5 with the restage bundles); wish 8
  narrow form + **push-to-name rider**: `ArgumentSpec::new(parser, name)` +
  `new_unnamed(parser)`, `.named()` removed, **`ParsedSlot` mirrored**
  (`new(region, name)`/`new_unnamed`). (7) **Sweep**: P1 deferred item (b) ruled —
  **`ArgumentParser` trait → `core::constructs`**; with (a) ruled in H, **both P1
  deferred placements are closed and the Phase 3 topology application is
  unblocked**.
- 2026-07-31 (user): **2b T4 SESSION RULED** (interactive; full working detail
  frozen in **T4_RULINGS.md**; durable records: DESIGN_RATIONALE new entries
  **[§dd-dr:input-wiring]**, **[§dd-dr:include-chain-helpers]**,
  **[§dd-dr:line-col-ownership]** + amendments on
  [§dd-dr:wire-identifier-stability] (THE FROZEN SLATE),
  [§dd-dr:resolver-contract], [§dd-dr:language-init], [§dd-dr:tree-navigation],
  [§dd-dr:span-extend-to], [§dd-dr:preset-driver-pillars], [§dd-dr:recompose],
  [§dd-dr:source-resolver], [§dd-dr:lazy-line-col] + superseded-names T4 block +
  ARCHITECTURE footer refs). Headlines: (1) **B/\input wiring**: driver accessor
  `source_resolver() -> Option<&dyn …>` (driver `Copy`/`Eq` DROPPED — no clear
  reason to keep; T3-D clause struck); door
  `cx.parse_attached_source(source, state, parser)` (**user amendment: caller
  supplies the construct parser**); bundle `attach_source_reference` (core,
  beside the door); TWO failure conditions (`NoSourceResolver`,
  `UnresolvableSourceReference` — `ResolveError` becomes `Clone` via
  `Option<Arc<dyn Error>>` cause, **user principle: techy error types uniformly
  Clone, out-of-crate info behind Arc**); recursion NOT core's job (user; `.dtx`
  legitimate self-inclusion) — instead `Source::including_sources()` +
  `check_include_chain(target_key, triggered_at, origin_key, max_depth)` in
  source (**origin-keyed incl. the primary** — user-driven design);
  `techy::helpers` REJECTED (util-grounds); `Language` collapses to
  `new + parse + parse_source + accessors`; preset `input_macro_spec::<LLL>()`
  opt-in, never preloaded. (2) **C**: `SourceResolver` IS the FS trait; techy
  ships nothing; companion bullet closed. (3) **A/slate FROZEN**: area **`specs`**
  absorbs command resolution AND the old `scopes` area (user: "resolution of
  what?" — also disambiguates vs source resolution); full 22-condition table in
  the [§dd-dr:wire-identifier-stability] amendment; segments kept; P5's "14 of
  18" corrected to 19 core.* (14 file-named). (4) **E/D**: `node_at` /
  `covering_slice` / `parent()` / `index_in_parent()` / `SourcePos::pos()` /
  `start_pos()`/`end_pos()` / `tree()` pub / `Span::contains` now;
  **`ancestors()` REJECTED** (user: top-down visiting; zero trap surface);
  cursor-vocabulary reconciliation recorded (retired `SourceCursor` ≠ F7's
  editor-cursor lookup); F7 CLOSED. (5) **F**: `line_of` **with line number**
  (user amendment) + `line_col_span`; `line_range`/per-node `line_col`/caret
  renderer REJECTED; `Descendants::with_depth()` REJECTED → **read-only walker
  routed to the recompose session** (user: don't reinvent a visitor);
  **line/col ownership design (user-driven)**: consumer-held
  `LineIndexCache` + **`LineColProvider`** rendering seam (editor incremental
  caches plug in); `DEFAULT_MAX_SCAN_LEN` → **500_000** (user); Lang-coupled
  analyzers REJECTED ([§dd-dr:origin-genericity] load-bearing).
- 2026-07-31 (user): **2b T5 SESSION RULED** (interactive; full working detail
  frozen in **T5_RULINGS.md**; durable records: DESIGN_RATIONALE new entries
  **[§dd-dr:restage-ops]**, **[§dd-dr:extract-annotations]**,
  **[§dd-dr:tree-validation]** + amendments on [§dd-dr:restage],
  [§dd-dr:node-annotations], [§dd-dr:recompose], [§dd-dr:slot-roles],
  [§dd-dr:input-attachment], [§dd-dr:tree-navigation],
  [§dd-dr:enclosing-state-stack], [§dd-dr:preset-driver-pillars],
  [§dd-dr:latexlike-generalization], [§dd-dr:takeover-staging-sugar] +
  superseded-names T5 block + ARCHITECTURE refs). Headlines: (1) **A/restage
  ops**: `RestageVisitor` trait + closure blanket (reentrancy needs
  self-passing); generic `RestageError<E>`; **`Restage::Descend`** final name;
  opaque-but-constructible bundles (the constructor is the general take-both
  form); no-silent-repair region policy (`ContentParentDropped`);
  `restage_argument_with_content` helpers; positional builder `add`; level-0
  `restage_node` cross-tree by contract; no `Send` on visitors/`annotate`.
  (2) **A8 (user-revised in session)**: extract producers mint annotations NOW —
  the general `A→B` callback owns the bare name (`split_at_chars(nodes, sep,
  f)`), `_drop_annotations`/`_keep_annotations` shorthands, all four producers;
  `Split` → **`SplitAtChars`**; clone-through default withdrawn on the user's
  stale-annotation counterexample. (3) **B**: `stage_invocation(invocation,
  arguments, slots, children, end_pos: Option<usize>)`; no overrides
  (environments stay on the `stage_node` door); symmetry by vocabulary, not
  arity. (4) **C1**: fourth role trait **`LatexlikeEvent`** closes the P3×E4
  gap; FLM projection otherwise clean. (5) **E (user shape)**: `StateStackView`
  → owning **`ParsingStateStack`** with `from_states` +
  **`from_node_ancestors`** — the pillar keeps the descriptive stack parameter
  and post-parse synthesis works without a session; two-component math recipe
  documented. (6) **D**: no new driver knobs. (7) **F**: **`validate_tree`**
  (all-trees law, `Result`, home **`core::node`** — user overruled the
  transform placement); slice accessors answer only whole-run single-source
  slices ("honest" banned from rustdoc). (8) **G (user)**: input caching
  dropped — `\input` can return modified caller state, so included files must
  be read on the spot; docs get a challenges discussion + a conditional recipe.
  (9) **H (user doctrine)**: NO byte-reconstruction guarantee —
  **recomposition is per-node, never inter-node span arithmetic** (spans =
  provenance); parse-law demoted to in-crate acceptance oracle;
  `validate_parse_tree` withdrawn; the doctrine + the
  `"begin_tokens"`/`"end_tokens"` Hidden-slot sketch are binding recompose
  inputs. (10) **I**: all 20 sweep rows dispositioned (binding-guide chapter
  checklist; post_space doc-only → techy-totext; `into_vec` → Tier-C lean
  reject; multi-source reconstruction tests → Phase 3 checklist).
- 2026-08-03/04 (user): **RECOMPOSE SESSION RULED** (interactive; full working
  detail frozen in **RECOMPOSE_RULINGS.md**; durable records: DESIGN_RATIONALE
  new entries **[§dd-dr:invocation-syntax]**, **[§dd-dr:recompose-machinery]**,
  **[§dd-dr:visit-engine]** + amendments on [§dd-dr:recompose],
  [§dd-dr:slot-roles], [§dd-dr:environment-scaffolding] (SUPERSESSION),
  [§dd-dr:tree-navigation], [§dd-dr:restage-ops], [§dd-dr:span-invariants],
  [§dd-dr:latexlike-generalization] + superseded-names recompose block +
  ARCHITECTURE notes in [§dd-arch:arch], [§dd-arch:nodes], [§dd-arch:engine],
  [§dd-arch:latexlike]). Headlines: (1) **Round 1 doctrine** (user-simplified
  R7): the recomposer may read any field of the node's own payload (span-backed
  `TextContent` resolution permitted as a storage detail) and may NEVER resolve
  span content — the node's own span included — against the source; no span
  fast path (a tree carries no freshness signal); `span_content()` stays a
  consumer affordance; "span-verbatim" retired as a strategy name; R15 in-crate
  oracle suite (reemit == input; strict + tolerant; multi-source rides I-18).
  (2) **CallSyntax slot role REJECTED outright** — and with it the brief's
  R9–R12 (Hidden-slot scaffolding storage, core `escape_char` field,
  Hidden-emission carve-out, order-free tiling); `SlotRole` stays three
  variants. (3) **Accuracy doctrine**: the preset owns recomposition accuracy =
  what the parse records; **`Lang::InvocationSyntax`** as a `CallableData`
  field REPLACING core `post_space`; two-trait split (required core data bound
  incl. `materialized()`, name at application aligned with the ext-bound
  family, fallback `InvocationSyntaxData` + opt-in `FromInvocation`/
  `from_invocation`, techy-implemented for `()`); latexlike enum
  `InvocationSyntax<Env = StdEnvironmentSyntax<L>>` — `Macro { escape_char,
  post_space }` / `Environment(Env)` / `Specials` unit; Specials Option 1
  (name = spelling as written; canonical-`"\n\n"` superseded; identification by
  spec identity — driver.rs:127 fix now load-bearing); per-side environment
  record `{ escape_char, command_word, post_space, name_group_rule:
  Arc<GroupRule<L>> }`; `EnvironmentSyntax` trait, accumulator shape (b),
  spelling writers `write_begin`/`write_end`; fifth role trait
  **`LatexlikeInvocationSyntax`**. (4) **Machinery**: meaning-free `Piece`
  value fold, no sink concept (streaming = recomposer-held writer,
  `Piece = ()`); `Recomposer` (State/Piece/Error, no Send/Sync);
  `Recompose { Emit, Concat(ConcatPieces) }` head/sep/tail + chainable
  constructors; `ComposePiece` monoid; wrapping contract (instructions lower
  against the outermost recomposer); Concat default scope skips Attached AND
  Hidden; `RecomposeContext` restage-mirror helpers; `core_source_instruction`;
  preset `SourceRecomposer<LLL>` + `source_recomposer()`; targeted replacement
  = wrapper pattern + documented restage→recompose pipeline. (5) **Shared
  visit engine**: walker-on-recompose-core direction; top-level `techy::visit`
  (`core::node` vetoed; free-fn `walk`, no `NodeRef::walk`); `NodeVisitor` +
  `VisitFlow`; `VisitContext` = engine bookkeeping only (three-channel state
  discipline); walk role-blind vs Concat's content default — the ruled
  asymmetry. (6) **Naming sweep**: `Bit`→`Piece`, `ConcatSpec`/`ConcatParts`→
  `ConcatPieces`, `VisitContext`/`RecomposeContext` spelled out,
  `recompose::recompose` stutter accepted, `walk_tree`/`recompose_tree`
  rejected, `new_for_invocation`→`from_invocation`, `RecomposeError` variants
  mirror `RestageError` exactly.
- 2026-08-03 (user): **2b TIER-C BATCH RULED — Phase 2b COMPLETE** (interactive;
  full working detail frozen in **TIERC_RULINGS.md**; durable records:
  DESIGN_RATIONALE new entries **[§dd-dr:public-visibility-sweep]** and
  **[§dd-dr:command-resolver]** (supersedes [§dd-dr:scopes-resolving-driver]) +
  amendments on [§dd-dr:input-wiring], [§dd-dr:source-resolver],
  [§dd-dr:tree-validation], [§dd-dr:resolution-extraction],
  [§dd-dr:stability-rubric] + superseded-names Tier-C block + ARCHITECTURE refs).
  Headlines: (1) all 76 no-usage-signal items dispositioned — **73 keep
  pub-and-stable; `NodeData` + `check_tree_invariants` → pub(crate)** (the
  latter re-implemented as a panic-assert wrapper over `validate_tree`'s
  `Result` — user-ruled one-canonical-implementation shape, riding the
  `validate_tree` commit); **`NoResolver` REMOVED** (R1 flipped from lean-keep:
  original default-slot use gone). Forced-pub finding ratified: "no usage
  signal" ≈ signature closure of the used API. (2) Conditions doctrine
  completed: all 17 shipped condition types + the 5 defining items (incl. both
  derive re-exports) keep pub. (3) **R4 REOPENED and re-ruled (major)**:
  `ScopesResolvingDriver` superseded by **`trait CommandResolver<L>`** +
  **`StdParseDriver<R = ()>`** (`()` = resolves nothing;
  `ScopesCommandResolver { command_type }` → core::specs); constructor doctrine
  `new(recovery, command_resolver)` (mandatory by-value strategy, no
  Default/Clone bounds) + chainable sealed-conversion `with_source_resolver`
  (renames T4-B1's `with_resolver`); ruled asymmetry documented in
  rustdoc + code: command resolver generic (consumed monomorphized, hot path) vs
  source resolver value-level dyn (consumed via the type-erased accessor, cold
  path); strategy-seam proliferation guard recorded. (4) Free `resolve_source`
  renamed **`resolve_source_reference`**. (5) Homes: `FrameRole` → hub (user:
  frames are engine-wide — groups mint them too; overrules the brief);
  `ParsedArgumentNodes` → core::constructs with its trait; parsed-residue rule:
  contract residue follows the trait, stored containers stay core::node.
  (6) `VERSION` keeps the ecosystem-idiom compile-time const (getter +
  structured-semver forms rejected). (7) Remaining riders closed:
  `ProvenanceChain`/`ResolvedContent` keep in source; `StdParseDriver` doc
  sentence → resolver-choice guidance; `Diagnostics::into_vec` never existed,
  reject-do-not-add.
- 2026-08-04 (user, Phase 3 S4 application): **E4 restore amendment** — the
  exit-math-context restore excludes transient gates: `expecting_group_close`
  and `temporary_groups` are NOT restored (amends the T1/T2 E4 "whole
  TokenRules of the found state" policy; durable record: amendment note on
  [§dd-dr:enclosing-state-stack]). Phase 3 execution state + resume protocol:
  PHASE3_PLAN.md (S1–S4 merged as of this entry; reviewer-verified, deviations
  user-confirmed per its stage log).
- (2026-08-03 checklist, retained for the record — every item below was
  audited DONE or consciously ROUTED in the S10 sweep, table in
  S10_REPORT.md) **Phase 3 — apply + harden** (all 2b sessions ruled;
  topology and all placements closed). **Phase 3 checklist additions from
  the Tier-C session**: `NodeData` + `check_tree_invariants` → pub(crate) (wrapper shape
  above; violation detail must keep panic messages informative); delete
  `NoResolver` (rides the T4 resolver-move application); rename free
  `resolve_source` → `resolve_source_reference`; `StdParseDriver` reshape per
  [§dd-dr:command-resolver] (CommandResolver supertraits Debug+Send+Sync; `()`
  no-op keeps the helpful not-implemented detail message;
  origin-parameter alignment with T4-B1's `L::SourceOrigin` accessor; asymmetry
  rationale in rustdoc AND a code comment at the field pair; resolver-choice
  doc sentence pairing with `TrivialLang`); `FrameRole` home = hub;
  `ParsedArgumentNodes` home = core::constructs; `PrefixEntry` lives beside
  `PrefixTable`; `VERSION` rustdoc sentence (Cargo package version, always
  valid semver).
  **Phase 3 checklist additions from the recompose session**: driver.rs:127
  canonical paragraph-break spec object (spec identity is load-bearing — no
  anonymous `SpecialsSpec::default()` per break); `materialize()` extended
  through the invocation-syntax bound trait (`materialized(source_content)`);
  T5-B `stage_invocation` signature amendment (the bundle carries the
  `InvocationSyntax` value); `CallableData` `post_space` → `invocation_syntax`
  field swap; kind.rs invariant-3 rewording; the in-crate oracle suite
  (reemit == input; strict + tolerant matrices; multi-source rides T5 I-18);
  the `Invocation` bundle carries trigger-token facts through to
  `from_invocation`; parse-law checker update (the callable arm reads the
  invocation-syntax payload); `RecomposeError` variant names mirror
  `RestageError`; the core invocation-syntax bound trait named at application,
  aligned with the ext-bound family (fallback `InvocationSyntaxData`). Prior
  checklist items stand: C2 driver-residue assertion; F5 parse-law checker
  `Attached`-scoping; A8 extract input-genericity rides the annotation
  application; the `\text` recipe forbidden_chars fix (T1/T2) and all prior
  application riders as logged.
- 2026-08-05 (user sign-offs per stage; supervising session): **Phase 3 —
  apply + harden COMPLETE** (S1–S10 all merged into api-review; landing
  commit 8ed2884; stage log + per-stage detail in PHASE3_PLAN.md /
  reports/S<N>_REPORT.md). S7 transform (restage + extract annotations,
  689→751-test trajectory begins), S8 visit + recompose + the R15 oracle
  suite (reemit == input across strict/tolerant/multi-source matrices),
  S9 preset definitions + consumer polish (`"_builtin"` slim package,
  minidefs/minilatex, F5 trap fixes, sugar), S10 hardening closed the
  phase: C2 residue assertion PASSED (25-line Lang delegation residue on
  the FLM projection, 7 driver delegation one-liners — within the ruled
  ~30/~12 envelopes); panic-policy sweep complete per [§dd-dr:panic-policy]
  + the S5 rider (all outer-layer-input guards now Err implementation-error
  paths or recorded staged-id degradations, +11 tests; value-constructor
  debug asserts kept under the recorded skip_whitespace pattern — site
  table in S10_REPORT; D-plan-2 confirmed at sign-off: document input can
  never reach them, release builds cannot panic there under trait
  customization); `missing_docs` promoted to workspace deny;
  cargo-semver-checks baseline realized as scripts/check_semver.sh against
  the **movable `api-baseline` git branch** (user ruling at sign-off:
  branch over tag; minted on the landing commit, guard proven — 196 checks
  pass); full public-surface audit exact (283 item pages, zero duplicate
  paths, every item at its ruled home); all-riders sweep: every Phase 3
  obligation DONE or consciously ROUTED to Phase 4; superseded-names sweep
  clean. The soft freeze of [§dd-dr:stability-rubric] is IN FORCE from this
  landing. Residual follow-up noted at sign-off (pre-existing, documented,
  not S10 scope): `SourceSpan::content`'s implicit indexing panic —
  candidate for the panic-policy rule-3 approved list or a `get` companion.
  **NEXT: Phase 4 — guides.**
- 2026-08-05 (user, post-Phase-3 ruling): **always-on precondition asserts** —
  the six value-function debug asserts (`Span::new`/`extend_to`, `Token::new`,
  `SourceSpan::new`, `SourcePos::new`, `skip_whitespace`) upgraded to plain
  `assert!` (contract violation panics in ALL builds; the release degradation
  fallbacks are superseded, incl. skip_whitespace's return-unchanged and the
  documented Span::len saturation semantics — the saturation stays as defensive
  code only). The panic policy REMAINS in place; the user articulated the
  governing principle for its rule-3 exceptions (few, individually escalated
  for user approval, deep/often-used code primary users rarely call directly,
  std-standard policy) — recorded in [§dd-dr:panic-policy] rule 3 with the
  approved register as family (b). Rationale: infallible-by-design functions
  have no Err channel, so the release alternative was unspecified misbehavior
  or a later cryptic panic; O(1) checks; invalid values become unrepresentable.
  CLOSES the SourceSpan::content follow-up from the S10 sign-off (unreachable
  by construction). All six rustdocs state the all-builds panic; should_panic
  pins added per assert (7 new tests); CLAUDE.md rule 4 updated.
- 2026-08-05 (user, two ruling rounds): **Phase 4 plan RULED** (PHASE4_PLAN.md
  v3 is the execution record). Headlines: guides = three categories (User /
  Developer / AI) of short, aggressively distilled chapters; volume discipline
  REMOVE-DON'T-SUMMARIZE with size caps (user chapters ~10–15 kB; exceptions
  learn-by-example ≤ ~30 kB, specs.md ≤ ~20 kB; dev soft cap ~30 kB; AI root
  ≤ ~30 kB + five sub-chapters); v1's full framework/tooling/custom-language
  cookbook chapters CUT — embedder findings + tooling starting points land as
  short sections in the NEW dev chapter integration.md; guides never duplicate
  rustdoc — extensive API-use documentation belongs in the module docs
  (transform/visit/recompose/extract); landing page = 2–3-paragraph summary +
  one-sentence chapter index; separate introduction.md; concepts-overview.md
  kept (dev-guide page); rehomings: FS-resolver recipe → specs.md, post_space
  + `\input`-caching notes → API docs only (brief), wish-23 identifier table
  SUPERSEDED by the DiagnosticInfo implementors listing + match-via-IDENTIFIER
  rule in parsing.md; NEVER-ASSUME rule — behavioral uncertainty goes to the
  DOC_GAPS.md GAP/CHECK register, resolved by verification agents; pylatexenc
  references link to readthedocs; **merge authorization: the supervisor
  reviews chapter text and commits/merges once convinced of accuracy** (user
  reviews post-merge). Stages: G1 skeleton+landing+introduction+concepts →
  G2 User Guide → G3 Developer Guide → G4 AI Guide → G5 verification+audit.
