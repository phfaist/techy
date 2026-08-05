# Phase 4 — G5 report: verification + final audit

Branch `phase4-g5-audit` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a637426ea387790c7`,
branched from `api-review` @ c172cab, which contains 996e49f = G4 merged).
Status: **M0 (plan)** — subsequent sections fill in per milestone.

Note on branch point: the worktree's HEAD at agent start was a stale commit
(2110bbb, predating the api-review series); the branch was reset to the
local `api-review` tip c172cab (996e49f + the user's own TODO_Big.md commit)
before any work, per the brief.

## Plan (Milestone 0 — resume from here if interrupted)

Governing inputs read: PHASE4_PLAN.md (Method rules; the G5 stage bullet
incl. the G2-review additions; Protocol; Gates — binding), DOC_GAPS.md
(#1 RESOLVED at G2; #2 and #3 OPEN), Documentation_Structure.md (wiring
rules, four-step chapter wiring, anchor discipline, acronym rule).

Stage character: UNLIKE earlier Phase-4 stages, G5 may edit rustdoc — doc
comments only, zero code-semantics changes; `scripts/check_semver.sh` stays
green, test counts unchanged (758+30+8+21+1 lib/suite, 66 doctests + 2
ignored). Method rules in force: every new rustdoc sentence grounded in
verified facts or existing documentation, NEVER assumed behavior; no
metaphors, no review-coined jargon, no dev-docs references in user-facing
text, superseded names stay dead; non-trivial findings are REPORTED, not
fixed. Do not modify PLAN.md, PHASE4_PLAN.md, or guide chapter content
beyond trivial audit fixes.

### M1 — DOC_GAPS #2 (WebAssembly mention)

`wasm32-unknown-unknown` target is installed in this worktree (verified via
`rustup target list --installed`). Re-verify
`cargo build --target wasm32-unknown-unknown -p techy` passes, then extend
the crate-level `## no_std` rustdoc section (techy/src/lib.rs, currently
lines 13–18) with a 1–2-sentence WebAssembly mention consistent with
introduction.md's "Where techy runs" claims: builds for WebAssembly
targets; the target must support atomics (sources shared as `Arc`); techy
performs no input/output — the host supplies all input. Mark #2 RESOLVED.

### M2 — DOC_GAPS #3 (panic-contract sentence)

Add a short crate-level passage (2–4 sentences) stating: parsing never
panics on document input — problems surface as diagnostics or an `Err`;
every fallible seam returns `Result`. Verified constraint (rustdoc of the
precondition-assert value constructors read: `Span::new` span.rs:27–29,
`Token::new` token.rs:151–154, plus `SourceSpan::new`/`SourcePos::new`
source.rs:233/374, `skip_whitespace` reader.rs:91 — all added by the
user's own ruling commit 5611d2b today): each states an ALL-BUILDS panic on
caller contract violation. The crate-level passage must be phrased so both
are true — document input never reaches those panics; the asserts guard
programming errors in calling code. Mark #3 RESOLVED.

### M3 — process-flavored rustdoc sweep

Mechanical sweep executed at M0 over ALL doc-comment lines (10,311 lines,
`///` + `//!`, techy/src + techy-derive/src) with pattern sets:
checkpoint / bare N.N checkpoint numbers / years / month names / phase /
ruled|ruling / review / walkthrough / persona / Action-N / session /
decided / Wish-N / S|G|T-number stage refs / §dd- labels / supersed* /
stratum / D-plan / rehom* / tier / milestone / sign-off / sketch / draft /
acceptance / "to be revisited". techy-derive/src: zero hits.

Fix list (public-rendering sites only; "keep the technical content, drop
the process reference"). The five sites named by the G2 review are marked
(G2):

1. (G2) extract.rs:5 — module doc "(decided at the 7.8 checkpoint)"
2. (G2) extract.rs:103 — `ExtractError` "(decided at the 7.8 checkpoint)"
3. (G2) latexlike/mod.rs:440 — `default_token_rules` "(decided at the 7.5
   checkpoint)"
4. (G2) latexlike/mod.rs:493 — `builtin_package` "decided at the 7.6
   checkpoint"
5. (G2) source/resolver.rs:44–45 — `SourceResolver` "decided July 2026,
   matching the other stored extension traits"
6. extract.rs:16 — heading "# Builders mint real trees (the 7.8 \"builder
   route\")" → drop the parenthetical (grep verified: NO rustdoc/guide
   link targets this heading's anchor; only dev-docs use "builder route")
7. extract.rs:730 — `parse_keyval` "(the 7.8 no-knobs decision)"
8. source/resolver.rs:22 — "(Action-05)"
9. spec/callable.rs:43–44 — `CallableSpec` "(slots session)"
10. spec/callable.rs:56–57 — `CallableSpec` "decided July 2026"
11. spec/structure.rs:156–157 — `ArgumentParser::can_match_empty` "(slots
    session; …)"
12. constructs/child_state.rs:63–65 — `InvocationChildState` "Defined with
    the 6.3 policy struct; **consulted from 6.4** … (per decided
    semantics, …)"
13. constructs/child_state.rs:82 — field doc "; active since 6.3"
14. constructs/child_state.rs:84–85 — field doc "consulted from 6.4"
15. visit.rs:68 — module doc "the ruled role semantics"
16. scopes/mod.rs:308–310 — `ScopeOp` "replacing Phase 4's
    `push_libraries`" (also reintroduces a superseded name — drop)
17. engine/driver.rs:159 — `ParseDriver::resolve_command` "that asymmetry
    is decided" → "deliberate"
18. latexlike/mod.rs:1 — module doc first line "(S2)" stratum label
19. latexlike/mod.rs:55 — "is a later phase" (roadmap wording)
20. latexlike/mod.rs:446 — "(decided for determinism …)" → "(deliberate: …)"
21. latexlike/driver.rs:41–42 — `ParagraphBreakStyle` "(decided with the
    7.9 acceptance work)"
22. latexlike/environments.rs:400 — `EnvironmentSpec` "(the decided
    permanent boundary; …)"
23. latexlike/arguments.rs:284–285 — `argument_specs_from_str` "(a later
    phase's porting target)"
24. constructs/verbatim_parser.rs:215 — `VerbatimArgumentParser` "read per
    the pinned recipe (module docs) … the decided group + chars shape" →
    ground the reading-state reference in the PUBLIC
    [`verbatim_state_delta`] doc (which states both parsers derive their
    reading states through it), drop "decided"
25. constructs/verbatim_parser.rs:105–107 — `verbatim_state_delta` "The
    pinned verbatim recipe … (see the module docs)" — the parenthetical
    points at a PRIVATE module page; drop it (the public doc is
    self-contained), reword "pinned recipe"
26. constructs/argument_parsers.rs:757–758 — `OptionalGroupArgumentParser`
    "supersedes the briefly-shipped LaTeX-style first-`]`-closes rule"
    (history) → drop, keep "(pylatexenc parity)"
27. constructs/argument_parsers.rs:768–769 — "(to be revisited with the
    preset argument-parser helpers, …)" → timeless caveat (a future helper
    may change this), keeping the documented divergence fact
28. latexlike/invocation_syntax.rs:255–258 — `EnvironmentSyntax` "(An
    earlier accumulator shape … it was superseded — …)" → timeless
    rationale, same technical content
29. source/mod.rs:1 — public facade module doc "S0 — source management:"
30. source/mod.rs:3 — "This stratum provides:"
31. source/mod.rs:44–45 — "S0 itself never depends on `Lang`, preserving
    the strict stratum layering"
32. error.rs:272–273 — `ToDiagnosticValue for ResolveError` "not in the
    source stratum … (stratum layering)" → plain "module"/"layering"
    wording

Candidates deliberately LEFT (with why):

- All hits in PRIVATE module docs (`//!` of non-facade modules — they do
  not render into public documentation; internal docs may legitimately
  carry process context): latexlike/spec.rs:4, latexlike/node_ref.rs:5–6,
  latexlike/environments.rs:8,27, latexlike/invariants.rs:8,
  constructs/child_state.rs:3,10,13,18, constructs/nodes_parser.rs:5–10,
  constructs/embellishments_parser.rs:28–29, constructs/tack_on_parser.rs:8–9,
  spec/mod.rs:20–21, engine/mod.rs:8, engine/state_memo.rs:16,
  token/mod.rs:3,17-area, token/error.rs:13, constructs/mod.rs:1.
- All hits in TEST code / cfg(test) items / test-support (never rendered):
  latexlike/test_support.rs:3,5, latexlike/input.rs:405–407 ("Ruling A" —
  doc on a test helper fn), latexlike/invariants.rs:61,
  node/invariants.rs:531,552 (both `#[cfg(test)]` fns),
  constructs/environment_parser.rs:903,1224–1227 (test-lang specs),
  constructs/verbatim_parser.rs:638, constructs/nodes_parser.rs:1108+,
  3416, argument_parsers.rs:1035, engine/mod.rs:572, transform/tests.rs:398,
  spec/mod.rs:84 (doc on a `#[test]` fn).
- PRIVATE items' `///` docs (not rendered): extract.rs:161–164 (`Piece`,
  "7.8 decision"), constructs/group_parser.rs:108–115 (private field,
  "The 6.5 motivating consumer…"), constructs/nodes_parser.rs:637–639
  (private method `recover_as_chars`, "lands in 6.4"),
  nodes_parser.rs:1120,1132,3719,3806 (private/test).
- `//` code comments — OUT of scope by the brief (e.g.
  state/parsing_state.rs:298, token/reader.rs:383+, engine/driver.rs:152,
  latexlike/mod.rs:577, group_parser.rs:82, spec/mod.rs:185).
- DESIGN_RATIONALE `[§dd-dr:…]` pointers in PUBLIC rustdoc: the
  panic-policy family (token.rs:154, reader.rs:91,181, span.rs:29,
  source.rs:233,374, argument_parsers.rs:675, nodes_parser.rs:479) was
  added/ratified by the user's own ruling commit 5611d2b TODAY — the
  wording is user-approved; not process-flavored in the M3 sense; left
  untouched. Same treatment for builder.rs:78 ([§dd-dr:ext-minting]) —
  changing the dev-docs-reference pattern is a policy question
  (Documentation_Structure four-case repair), not a wording fix; REPORTED
  (see M4 findings), not fixed.
- "tier-1"/"tier-2"/"two-tier ownership model" vocabulary on public items:
  KEPT — publicly defined (docs/construct-parsers.md § "The trait, and the
  two-tier ownership model"; ai-guide-custom-lang.md).
- "is decided at parse time by the preset" (token/rules.rs:69,
  token/token.rs:19) — ordinary English, not process. KEPT.
- "Implementer obligations" (state/lang.rs:347,382) — trait-implementer
  sense. KEPT. "May fail recoverably" / "May record" / "U+2028" — month
  and year regex false positives. KEPT.

### M4 — final audits (planned procedure)

1. Full clean docs build: `rm -rf target/doc && cargo docs` — zero
   warnings.
2. Wiring audit: 18 chapter files in docs/; 18 submodule declarations in
   the lib.rs guide block; GUIDE_PAGES = 19 entries (landing + 18) in the
   ruled order User → Developer → AI; docs/guide.md indexes all 18 with a
   one-sentence description each.
3. Length audit: byte size of every chapter vs target (user ≤ ~10–15 kB;
   learn-by-example ≤ ~30 kB; specs ≤ ~20 kB; dev soft ~30 kB; AI root
   ≤ ~30 kB, subs ≤ ~60 kB); table below.
4. Fragment-anchor audit (scripted): every `#fragment` in intra-doc links
   across the 18 chapters resolved against the rendered HTML heading ids;
   list of checked links below.
5. Terminology sweep over docs/*.md: metaphors, review-coined jargon,
   acronym-rule violations (Documentation_Structure: no acronyms except
   extremely widely understood ones). Trivial hits fixed; rest reported.
6. Superseded-names sweep over docs/ + all rustdoc files touched in
   M1–M3, against dev-docs/DESIGN_RATIONALE.md [§dd-dr:superseded-names].
7. External-link audit: readthedocs URLs in pylatexenc-migration.md and
   ai-guide-pylatexenc.md (28 definitions each); sample of 6 fetched.
8. DOC_GAPS.md final state: all 3 entries RESOLVED.

### M5 — closure

Full gate run (`cargo build` 0 warnings; `cargo test` counts unchanged;
`rm -rf target/doc && cargo docs` 0 warnings; `scripts/check_semver.sh`);
complete this report (audit tables, changed-site list, deviations); DRAFT
the PLAN.md Phase-4-complete decision-log entry + checkbox change as TEXT
in this report (NOT applied — supervisor applies closure records at
merge). Commit.

Gates run at every milestone that touches techy/src or docs/.

## M1 — DOC_GAPS #2: DONE

- `cargo build --target wasm32-unknown-unknown -p techy`: PASS (exit 0,
  clean finish; target installed in this worktree).
- techy/src/lib.rs `## no_std` section: one WebAssembly sentence appended
  ("In particular the crate builds for WebAssembly targets such as
  `wasm32-unknown-unknown`, where the host supplies all input."), wording
  aligned with introduction.md's "Where techy runs" paragraph; also
  "no I/O" → "no input/output" in the same sentence I extended (acronym
  rule alignment; introduction.md already spells it out).
- DOC_GAPS #2 marked RESOLVED with the verification trail.
- Gates: see gate table in M5 (run per milestone; all green at M1).

## M2 — DOC_GAPS #3 (fills in after execution)

## M3 — sweep execution (fills in after execution)

## M4 — audit results (fills in after execution)

## M5 — closure (fills in after execution)
