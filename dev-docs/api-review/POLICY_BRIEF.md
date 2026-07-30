# Phase 2a Policy Brief (v2, 2026-07-29) — the high-level API decisions

Supersedes POLICY_BRIEF_v1.md (kept for the record; its evidence sections remain valid).
Inputs: INVENTORY.md, SYNTHESIS.md (T1–T4), walkthroughs/framework/ (T5),
NAMESPACE_OPTIONS.md, and the user's 2026-07-29 rulings (backend scope; tier logic over
frequency; restructuring allowed now). Five decisions P1–P5; rulings go to the PLAN.md
decision log + DESIGN_RATIONALE entries.

## Evidence recap (one paragraph)

All five personas succeeded against the current API. T5 (the primary persona post-
reframe) verified: every ownable type is `'static + Send + Sync`; the
`Arc<NodeTree>` + `NodeId` → `tree.get(id)` round-trip makes Python node handles work
with no unsafe and no tree copies (working PyO3 abi3 module + passing smoke test);
byte-faithful reconstruction and targeted rewrite work today via span gap-filling
(latexpp archetype verified exact, including tolerant-recovery nodes); latex2text
archetype is sufficient today modulo a standard macro database. The two structural
findings: the latexlike preset is `Latexlike`-monomorphic (custom-`Lang` FLM forfeits
ALL preset components — the "preset-fork cliff"), and tree *transformation* (subtree
copy into a new tree) hits crate-private region translation (`RegionAlreadyResolved`),
no BuildId→NodeId map from `finish()`, no parent navigation, no mixed-origin validator,
and `NodeSlice::source_text()` silently stale on spliced trees.

## P1 — Namespace topology (from NAMESPACE_OPTIONS.md)

**Options.** R1 tiered: ~18 curated common names at crate root + `techy::latexlike` +
flat `techy::core` carrying the complete S0/S1 machinery; internal modules private →
full reshuffle freedom. R2 pure facade: `core` + `latexlike` only, empty root
(converges to R1 additively later). R3 structured `core::{source,…,engine}`: best topic
browsing, but re-freezes the nine-topic taxonomy as public contract (axis already
revised once; live candidates to revise again).

**Verified facts.** Flat facade is collision-free (0 clashes across 180 S0/S1 items, 0
vs latexlike). Root re-exports are additive/location-independent → curation mistakes in
R1 are fixable by promotion, never forced breaks. Wire diagnostic identifiers already
use `core.*`/`latexlike.*` area prefixes → the name `core` unifies path and wire
vocabularies (`parsing` would fork them); `core`'s extern-prelude-shadowing con fails
loudly, never silently. `techy::util` rejected on naming principles. techy-derive:
reroute all emitted paths through `__private` (mechanical, do under any option).

**Recommendation: R1**, facade named `techy::core`, `extract` reachable per the R1
sketch, derive reroute included. Draft 18-name root list in NAMESPACE_OPTIONS.md §O3;
hard cases (14 rows) go to the 2b sessions.

## P2 — Entry facade + standard definitions

**Recommendation.** (a) `latexlike::parse(src)` / `parse_tolerant(src)` + a small
builder (`latexlike::parser().package(p).tolerant().parse(src)`) bridging facade →
`Language` without a cliff. (b) Standard-definitions database as in-crate
`techy::latexlike::defs` module, no feature gate (pylatexenc's parser-side DB ≈ 633
lines/253 specs, sub-second compile; reversible later to an optional-dep crate via
re-export). Extent + naming designed in the 2b T1/T2 session; T5-A additionally wants
render-side attachment points kept in mind (spec-side `Any`-downcast contract worked).

## P3 — Preset reusability across Langs (restructuring; T5 need #2)

FLM-on-custom-`Lang` today forfeits `LatexlikeDriver`, `MacroSpec`,
`default_token_rules()`, `base_package()` — all `Latexlike`-monomorphic (compile-
probed). FLM-on-the-preset works but cannot reach the two-tier node-ext system.

**Question.** Bring "preset components generic over `Lang` (or otherwise reusable)"
into this review's scope? This is exactly the class of restructuring that becomes
breaking later: FLM is a named target, and the fork-cliff would hit it first.

**Recommendation: in scope.** Direction decided now; design (how generic, what stays
`Latexlike`-only) in the 2b T3/T5 sessions. If deferred, it must be a conscious
accepted-cost entry in DESIGN_RATIONALE, not silence.

## P4 — Transformation & navigation surface (T5 need #1; user's framework idea)

The tree-transformation infrastructure (transforms ending in string nodes +
concatenation = to-text; FLM passes; latexpp rewrites) needs from techy: public
subtree-copy/rebuild support (today crate-private region translation blocks it);
`finish()` old↔new id correspondence; parent navigation (`parent()`/
`index_in_parent()` or a `ParentMap` helper — also the #1 FFI gap and T4's F7); a
supported span-faithful recomposition helper (codify the gap-filling walk T5-C proved
correct); a mixed-origin tree validator + stale-`source_text()` guard.

**Recommendation.** Ratify direction now: these capabilities become public API in techy
(the transformation *framework* itself may still land as module or companion crate —
decided in the 2b T5 session; techy-totext then builds on it). Cursor primitive
`node_at(offset)` (T4 F7) rides along with parent navigation.

## P5 — Stability rubric

As v1 D4, updated: **Tier A** (hard-stable): curated root + `latexlike` happy path +
facade + defs entry points. **Tier B** (stable-advanced): `techy::core` machinery +
provenance layer; same semver discipline, documented as advanced. **Tier C**: the 66
no-usage-signal items — per-item rulings in 2b (promote / core-only / `pub(crate)`).
**Wire identifiers are semver-stable from now**; their `<area>` segment must be
decoupled from internal file names (F9) before guides print them. P1's `core` naming
makes path and wire vocabularies coincide.

## Routing to Phase 2b (five sessions)

- **T1/T2 session**: facade + builder naming; defs extent; F5 traps a–d (insert-time
  validation; no-fallback argument code; `None` conflation; spec/type cross-check);
  sugar wishes 5–16, 30.
- **T3 session**: SimpleLang role (TODO_Big item); on-ramp cliffs F10 (neutral
  TokenRules/StateData values, specials wiring); takeover staging F11; preset
  reusability design (with T5).
- **T4 session**: cursor primitive + parent nav design; `\input` wiring F8 + FS-trait
  option (leaning: logic in techy, embedder implements minimal trait); LineIndex
  helpers F6; identifier registry F9.
- **T5 session**: transformation surface design (P4 details; module vs crate);
  preset-generalization design (P3, with T3); `post_space` re-emission gotcha;
  binding-guide material.
- **Tier-C batch**: the 66 items, grouped by module, one keep/demote list.
