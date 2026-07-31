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
- [~] **Phase 2b — Decision sessions by access tier** (in progress; **T1/T2 session
  RULED 2026-07-31** — brief T1T2_BRIEF.md, rulings T1T2_RULINGS.md + decision log +
  six new DESIGN_RATIONALE entries; **T3 session RULED 2026-07-31** — brief
  T3_BRIEF.md, rulings T3_RULINGS.md + decision log + seven new DESIGN_RATIONALE
  entries; both P1 deferred placements ruled → **Phase 3 topology unblocked**;
  remaining: T4, T5, recompose, Tier-C batch).
  Per-item
  rulings (promote / keep-off-root / pub(crate)) over the 66 no-usage-signal items
  (SYNTHESIS §3); trap fixes F5a–d (T2 session); cursor primitive F7 + \input wiring F8
  + FS-trait option (T4 session); SimpleLang role + on-ramp cliffs F10 (T3 session);
  sugar batch (wishlist; distribute); T5 session: transformation-infra scope, FFI-driven
  API needs (owned handles vs lifetimes), reconstruction guarantees.
- [ ] **Phase 3 — Apply + harden** (agents in worktrees, merged locally): apply rulings;
  guards: cargo-semver-checks baseline, missing_docs → deny (already at zero warnings).
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
- **\input / file-system resolution**: leaning AGAINST a separate std-tools crate
  (frameworks own their I/O policy). Evaluate instead: logic stays in techy (no_std),
  embedder implements a minimal filesystem-interface trait (SourceResolver pattern).
  T4-session item, ties to friction F8.

## Working files

Repo (durable), all under dev-docs/api-review/:
- PLAN.md — this file (master).
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
- walkthroughs/{consumer,extender,langdesign,tooling,framework}/ — FRICTION.md +
  API-SURFACE.md (+ example code; framework/ adds FRAMEWORK-ANALYSIS.md) per persona.

Scratchpad (session of 2026-07-28/29, survives on disk; copy durables into repo):
`/private/tmp/claude-501/-Users-philippe-projects-techy/3b71ab8b-6cf7-4ab7-83d6-1a1d982076fb/scratchpad/api-review/`
— raw rustdoc JSON + extraction scripts, per-agent PROGRESS.md checkpoint files, and the
runnable walkthrough cargo projects (consumer/, extender/extender-examples/,
langdesign/notely/, tooling/techy-tooling/, framework/ pending).

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
- (pending) **NEXT: 2b T4 session** (then T5; recompose design session; Tier-C
  batch). Prepare the T4 brief exactly as for T1/T2/T3 (background agent, every
  claim re-verified against current code, output copied to dev-docs/api-review/,
  presented point-by-point; interim rulings file updated every round, hard
  structural points first). T4 inputs/agenda: POLICY_BRIEF routing (cursor
  primitive F7; `\input` wiring F8 — leaning embedder filesystem-trait, see
  Companion projects; source-model polish); P4 riders: `\input` engine wiring +
  resolver move `Language` → driver + reverse-lookup naming (`SourcePos`,
  deepest-node/covering-slice); P5 rider: **wire-identifier rename slate**
  (concept-named areas — 14 of 18 core.* identifiers use internal file names; the
  resolution-concept areas are now definable per [§dd-dr:resolution-extraction]);
  walkthroughs/tooling/. T5 agenda additions from T3: D acceptance (FLM probe
  re-run), driver knobs / extension seam, pillar-signature sufficiency for
  post-parse state synthesis, restage interaction, wish-20 `stage_invocation`
  signature (co-designed with `restage_invocation` + builder-`add` ergonomics).
