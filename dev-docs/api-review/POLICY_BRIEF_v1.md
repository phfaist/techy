# Phase 2a Policy Brief — the high-level API decisions

> **v1, PARTIALLY SUPERSEDED (2026-07-29).** After the scope reframe (techy = backend
> for frameworks; T5 persona added; exports follow access-tier logic, not frequency;
> restructuring allowed now), the following no longer stand as written: D1's
> keep-as-is recommendation (a `techy::core`/`techy::parsing` facade namespace is under
> active evaluation — see NAMESPACE_OPTIONS.md), D2's frequency-based curation rule
> (replaced by tier logic), and D3's "few obvious calls" framing (scaled back; the
> std-definitions idea now also weighs a separate crate/`latexlike::defs`). D4's rubric
> survives with T5 added. See PLAN.md (master) — v2 of this brief is written after
> Phase 1b (T5 walkthrough) and the namespace evaluation land. The evidence sections
> below remain valid.

Decision aid for the Phase 2a session. Evidence citations refer to SYNTHESIS.md (§) and
the walkthrough FRICTION/API-SURFACE files. Four decisions (D1–D4); each with options,
evidence, and a recommendation. Rulings go to the PLAN.md decision log + a
DESIGN_RATIONALE entry once made.

## Evidence snapshot (from SYNTHESIS.md)

- 205 public items; the four personas' union touched 73 (36%). Tier sizes are real and
  steep: T1 = 24, T1∪T2 = 29, +T3 = 67, +T4 = 73.
- T1 uniqueness is zero — the consumer surface is a strict subset of what T2–T4 need.
- 18-item "everyone needs this" core (used by 3+ personas): `Language`, `ParseResult`,
  `Recovery`, `ParseError`, `Diagnostic`, `Diagnostics`, `Package`, `NodeRef`,
  `NodeSlice`, `SourceSpan`, `NodeTree`, `NodeKind`, + the 6-name latexlike happy path
  (`Latexlike`, `LatexlikeDriver`, `CallableType`, `MacroSpec`, `EnvironmentSpec`,
  `argument_specs`).
- Of 140 root re-exports: 76 (54%) touched by nobody; only 3 were ever *accessed through
  the root path* (all T4). Every guide teaches module paths; personas followed suit.
  Root's only measured effect on T1 was negative (autocomplete flood, F2).
- The 9 module-only items personas needed (latexlike ×7, `extract::content_as_chars`,
  `GroupArgumentParser`) caused no reach failures.
- T3's verdict: submodule organization maps 1:1 onto a language designer's decisions —
  meets the "logical reach" bar; the entry path (not the structure) is what falls short.

## D1 — Namespace topology

**Question.** Keep the current nine top-level modules, or introduce groupings such as
`techy::core::*` + `techy::latexlike::*`?

**Options.**
- (a) Keep the nine modules exactly as-is.
- (b) Introduce a `techy::core` (or similar) umbrella re-export namespace over S0/S1,
  making the S1/S2 split visible in paths (`techy::core::NodeRef` vs
  `techy::latexlike::MacroSpec`).

**Evidence.** T3 explicitly rated the current organization as meeting the bar; no persona
misfiled anything or complained about module *placement* (complaints were curation and
guides). (b) adds a second path to every S0/S1 item (aliasing = the redundancy this
review is trying to remove) or, done as a move, violates the no-restructuring constraint.

**Recommendation: (a).** The strata story (S0/S1/S2) lives in docs, not paths. The
latexlike half of the original idea is already true (`techy::latexlike::*` is namespaced
by design and worked well in all walkthroughs).

## D2 — Crate-root re-export policy

**Question.** What does `techy::<Name>` offer? Today: 140 names ≈ the whole non-preset API.

**Options.**
- (a) Status quo (flat 140).
- (b) Empty root: no type re-exports; root has only facade helpers (D3) and modules.
  One rule, no curation debt; every type has exactly one path.
- (c) Curated root: the ~18-item empirical core (minus latexlike, which stays
  namespaced) + facade helpers at root; everything else module-path only.
  Root ≈ 12–15 names ≈ "what every techy program touches".
- (d) (b) or (c) plus a `techy::prelude` glob-import module for the curated set.

**Evidence.** Root as an access path is empirically almost dead (3 uses, 2 of them the
`format_*` fns); 54% of root names went untouched by all four personas; T1's friction
(autocomplete flood) is caused by the breadth. But dual-pathing costs the *docs* too:
every guide must pick a canonical spelling. The walkthroughs' de-facto canon is module
paths everywhere except arguably `techy::format_position`/`format_traceback`.

**Recommendation: (c) without prelude**, with the curation rule stated once: *root
re-export = used by the 3+-persona core AND not preset-specific*. That yields roughly:
`Language`, `ParseResult`, `ParseError`, `Recovery`, `Diagnostic`, `Diagnostics`,
`Package`, `NodeTree`, `NodeRef`, `NodeSlice`, `NodeKind`, `SourceSpan` (+ `Source`,
`format_position`/`format_traceback` as judgment calls) — every one earns its
autocomplete slot. (b) is the fallback if curation debates drag; a prelude (d) adds a
third spelling for glob-importers and is easy to add later, hard to remove — skip for
now. Demotion mechanics: plain removal of `pub use` lines (pre-1.0, sanctioned lever);
items stay public at their module paths. Whether any become `pub(crate)` entirely is
Phase 2b, per-item.

## D3 — The T1 facade (one-call entry)

**Question.** What is minute-one techy? Today it's `Language` + `Latexlike` +
`LatexlikeDriver` + provider knowledge (F4), ~25 lines before the first parse of
realistic input (F3), 24 items for basic tasks vs the "few obvious calls" goal.

**Options.**
- (a) Free functions in the preset: `latexlike::parse(src) -> Result<ParseResult, _>` +
  `latexlike::parse_tolerant(src)`; configuration stays on `Language`.
- (b) (a) + a small builder for the next step up:
  `latexlike::parser().package(p).tolerant().parse(src)` — bridges facade → `Language`
  without a cliff.
- (c) Facade at crate root instead (`techy::parse`) — rejected in place: it would
  privilege LaTeX in the engine namespace, against the crate's core identity.

**Evidence.** Wishes #1/#2 (T1); F4; T2's "two activation idioms read as two models";
T3's F10a shows the same cliff pattern one tier up (the fix there is different — D4/2b).

**Recommendation: (b).** The builder is what makes the facade honest — without it, the
first package a user adds throws them off the facade entirely. Names/signatures designed
in the T1 session of Phase 2b (per naming principles [§dd-arch:naming]).

**Companion decision — standard-definitions package (wish #3, new capability).** A
`latexlike`-shipped package of common LaTeX definitions (`\emph`, `\cite`, `itemize`, …),
explicitly incomplete. Without it, "parse realistic LaTeX" starts with 25 lines of
registration ceremony regardless of facade. pylatexenc ships exactly this (its default
macro database is arguably its most-used feature) — parity argument. Recommend:
**in scope**, sized during the T1 session (extent: pylatexenc's `macrospec` defaults as
reference list).

## D4 — Stability semantics per tier

**Question.** What does "stable" promise where, so Phase 2b has a rubric?

**Proposed rubric (recommendation).**
- **Tier A (hard-stable):** the D2 root set + latexlike happy path + facade — the T1∪T2
  surface (~29 items + sugar added by 2b). Breaking changes: never (post-review).
- **Tier B (stable-advanced):** the T3/T4 increments (~44 items: token/state/spec/
  scopes/constructs/engine machinery + provenance layer). Same semver discipline, but
  documented as advanced; guides gate them behind "designing a language?" / "building
  tools?" doors.
- **Tier C (per-item review in 2b):** the 66 no-signal items (SYNTHESIS §3, minus the 10
  starred implicit-use caveats). Each gets one of: promote (real API, walkthroughs just
  didn't need it — e.g. the diagnostics-*defining* surface `DiagnosticData`/
  `ToDiagnosticValue`, unexercised because no persona defined a custom condition);
  keep-public-demote-from-root; or `pub(crate)`. Not a bulk demotion: the walkthroughs
  sampled four task sets, not all uses.
- **Wire identifiers** (diagnostic `IDENTIFIER` strings) are declared a stable namespace
  *now*, which forces the F9 fix — the `<area>` segment currently equals the internal
  file name (`core.nodes_parser.*`); renaming a file would break a stable string. Needs
  a naming rule decided before guides print identifiers. (Detail ruling in the error
  session of 2b; the *principle* — identifiers are semver-stable — is the 2a decision.)

## Not in 2a (routed to 2b sessions)

- Per-item rulings on the 66 no-signal items (all sessions, by module ownership).
- Facade/builder naming; std-definitions package extent (T1 session).
- Trap fixes F5a–d: insert-time validation, no-fallback argument code, `None` conflation,
  spec/type cross-check (T2 session — F5 is the review's worst individual finding).
- Cursor primitive (`node_at`, `parent()`/`ancestors()`) — T4 session; likely needs a
  parent-index design decision (flat tree stores no parent links today).
- `\input` wiring story (T4 session; construct vs blessed-loop).
- Sugar batch: wishes 5–8, 12–22, 26–30 (distribute to owning sessions).
- SimpleLang's role (T3 session; ties to existing TODO_Big item).
