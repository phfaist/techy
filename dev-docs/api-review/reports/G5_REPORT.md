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

## M2 — DOC_GAPS #3: DONE

- New crate-level `## Panics` section in techy/src/lib.rs (between
  `## no_std` and `## The public modules`), 3 sentences: never panics on
  document input (diagnostics / `Err`; every fallible seam returns
  `Result`); the precondition-assert value functions panic in all builds
  on caller contract violation (examples linked: `Span::new`,
  `SourceSpan::new`); those guard programming errors in calling code — no
  document content can trigger them.
- Grounding: read the rustdoc of all named precondition-assert functions
  (span.rs:27–29 + extend_to, token.rs:151–154, source.rs:231–235 and
  371–374, reader.rs:89–91) — each states the all-builds panic; wording
  chosen so the crate passage and the item pages are simultaneously true.
- DOC_GAPS #3 marked RESOLVED with the verification trail.
- Incidental sweep find while grepping "panic": error.rs:197 references
  "CLAUDE.md panic policy" in a `///` — on a PRIVATE `macro_rules!`
  definition (not `#[macro_export]`; never rendered) → leave-list.

## M3 — sweep execution: DONE

All 32 planned public-rendering sites fixed exactly as listed in the M0
plan (same numbering); no additional sites surfaced during execution
beyond the M2 incidental (error.rs:197 CLAUDE.md reference — private
macro, leave-list). Notable wording choices:

- Site 6 (extract.rs heading): now `# Builders mint real trees` — anchor
  change verified safe (zero inbound `#`-fragment links in rustdoc or
  docs/; "builder route" occurs only in dev-docs).
- Site 16 (`ScopeOp`): dropping "replacing Phase 4's `push_libraries`"
  also removes a resurfaced superseded name.
- Site 24 (`VerbatimArgumentParser`): the dangling "(module docs)" pointer
  (private page) replaced by the PUBLIC [`verbatim_state_delta`] anchor,
  whose own doc states that both verbatim parsers derive their reading
  states through it — no behavior assumed.
- Site 27 (`OptionalGroupArgumentParser`): the "(to be revisited …)"
  roadmap note became a timeless "(subject to revision — …)" caveat,
  keeping the documented divergence fact and the possible mechanism.
- Site 28 (`EnvironmentSyntax`): the "earlier accumulator shape …
  superseded" history became the equivalent timeless rationale ("A record
  that scanned its own sides — a mutate-in-place accumulator — would not
  work: …"), same three technical points.
- Sites 18/29/30/31/32: internal stratum labels ("S0", "S2", "stratum")
  removed from PUBLIC module/item docs (plain "module"/"layer"/"layering"
  wording); internal-module uses of the stratum vocabulary left untouched.

Post-fix verification: both sweep scripts re-run — every remaining hit is
in the leave-list (private `//!` module docs, `#[cfg(test)]`/test-support
items, private items/fields/methods, `//` code comments, dd-label
pointers, false positives). Candidate left additionally noted:
"footgun" (extract.rs `parse_keyval` doc, pre-existing informal metaphor
in a sentence I edited for site 7) — left: metaphor cleanup in rustdoc
beyond process wording is outside the M3 mandate; reported here for the
supervisor.

## M4 — audit results: DONE

### 1. Docs build

`rm -rf target/doc && cargo docs` after all G5 edits: **zero warnings**
(deny lints for missing_docs / broken intra-doc links in force).

### 2. Wiring audit — PASS

- docs/: all 18 chapter files + guide.md present (19 .md files +
  rustdoc-header.html; no strays).
- lib.rs guide block: 18 submodule declarations, order User → Developer →
  AI, matching the files one-to-one.
- GUIDE_PAGES (docs/rustdoc-header.html): **19 entries** — `["",
  "Overview"]` + 18 chapters, grouped by the three comment-marked sections
  in the ruled order; slugs match the module names.
- docs/guide.md: all 18 chapters indexed, one-sentence description each,
  same order. One stale sentence found and fixed (trivial audit fix):
  "Chapters still being written appear below with a short placeholder
  page." — no unwritten chapters remain since G4.

### 3. Length audit — ALL WITHIN TARGETS

| Chapter | Bytes | Target | Verdict |
|---|---|---|---|
| introduction.md | 6,602 | ≤ ~10–15 kB | PASS |
| language-syntax.md | 12,490 | ≤ ~10–15 kB | PASS |
| node-trees.md | 9,008 | ≤ ~10–15 kB | PASS |
| specs.md | 15,945 | ≤ ~20 kB | PASS |
| parsing.md | 8,768 | ≤ ~10–15 kB | PASS |
| learn-by-example.md | 29,561 | ≤ ~30 kB | PASS |
| concepts-overview.md | 11,924 | soft ~30 kB | PASS |
| parsing-model.md | 15,910 | soft ~30 kB | PASS |
| construct-parsers.md | 20,638 | soft ~30 kB | PASS |
| custom-lang.md | 16,438 | soft ~30 kB | PASS |
| integration.md | 6,382 | soft ~30 kB | PASS |
| pylatexenc-migration.md | 17,431 | soft ~30 kB | PASS |
| ai-guide.md | 16,420 | ≤ ~30 kB | PASS |
| ai-guide-definitions.md | 13,396 | ≤ ~60 kB | PASS |
| ai-guide-trees.md | 14,584 | ≤ ~60 kB | PASS |
| ai-guide-custom-lang.md | 18,254 | ≤ ~60 kB | PASS |
| ai-guide-embedding.md | 8,249 | ≤ ~60 kB | PASS |
| ai-guide-pylatexenc.md | 13,344 | ≤ ~60 kB | PASS |
| guide.md (landing) | 5,048 → 4,966 after stale-sentence fix | (none) | — |

### 4. Fragment-anchor audit — 44/44 PASS (scripted)

Script: extract every markdown link with a `#fragment` (inline +
reference-style) from all 19 docs/*.md files; resolve
`crate::guide::<module>#frag` to target/doc/techy/guide/<module>/index.html
and check the fragment against the rendered `id="…"` set. Re-run on the
fresh post-edit docs build: **44 links checked, 44 OK, 0 failures** (no
non-guide fragment targets, no same-page fragments). Distinct fragments
verified: concepts_overview #the-node-tree, #scopes-and-packages,
#parsing-state-and-deltas, #callable-specs-and-arguments,
#sources-and-spans, #construct-parsers, #diagnostics-and-tolerant-parsing;
specs #the-spec-types, #registration-pitfalls,
#resolving-external-sources-input-like-inclusion; parsing
#working-with-diagnostics; introduction #where-techy-runs;
language_syntax #no-definitions-ship-by-default; construct_parsers
#a-complete-takeover-parser; ai_guide_definitions #argument-codes, #traps,
#input-like-inclusion. (Full 44-link list in the M4 script output; each
row is chapter → link.)

### 5. Terminology sweep over docs/*.md — CLEAN (no fixes needed)

- Acronym scan (all `[A-Z]{2,}` tokens): every hit is either on the
  allowed list (AST — defined in guide.md as "Abstract Syntax Tree"; API,
  HTML, URL, ASCII, AI; LaTeX/TeX; PyO3, FLM — proper names; CT_MACRO — a
  code identifier), a code identifier (`LLL` type parameter, only inside
  code spans; `T::IDENTIFIER`), or emphasis capitals in the AI guide
  ("SAME", "AND", "ONE" — compression emphasis, sanctioned style). No
  "WASM", "DR", or other violations.
- Metaphor scan (footgun/under the hood/magic/journey/on-ramp/heart
  of/glue/…): one hit, "boilerplate" (construct-parsers.md:226) — kept:
  established programming vocabulary, not a review coinage. ("door"
  vocabulary is a FORCED keep per the G4 record: shipped module-doc
  vocabulary.)
- No review-coined jargon found (checkpoint/persona/walkthrough/stratum/
  S0-S2/rider/tier-misuse: zero hits in docs/).

### 6. Superseded-names sweep — CLEAN

Register: dev-docs/DESIGN_RATIONALE.md [§dd-dr:superseded-names] (~110
names/shapes). Scripted word-boundary sweep over docs/*.md + all 17
rustdoc files touched in M1–M3. 27 raw hits, all benign on inspection:

- `LatexWalker`/`LatexNode` in the two migration chapters — pylatexenc's
  OWN class names being mapped (with readthedocs link definitions); not
  techy vocabulary reintroduction.
- "Split …"/"Ancestors of …" — English words in prose/comments, not the
  rejected `Split`/`Ancestors` types.
- "no `SlotSpec`" (spec/structure.rs:17, internal module doc) and "no
  `ConflictStrategy`" (scopes/mod.rs:20 internal + :1402 public) —
  deliberate NEGATIVE mentions documenting the design by contrast;
  pre-existing, survived the Phase-3 stage reviews; kept.
- "Library conditions are reported…" (error.rs:5) — English "library".

### 7. External-link audit — PASS (6/6 sample)

Both files carry **28** readthedocs definitions each (27 `[pyl-…]` + 1
`[pyl]` root), and the two 27-entry `[pyl-…]` sets are byte-identical
(diff-verified), matching the G3/G4 records. Sampled 6 URLs (every fifth
definition + the root), fetched live:

| URL | Result |
|---|---|
| /en/latest/latexwalker/ | OK — LatexWalker + get_latex_nodes + parse_content present |
| /en/latest/latexnodes.nodes/ | OK — LatexNode + LatexEnvironmentNode present |
| /en/latest/macrospec/ | OK — EnvironmentSpec + LatexContextDb present |
| /en/latest/latexnodes/ | OK — ParsingStateDelta present |
| /en/latest/latex2text/ | OK — LatexNodes2Text.set_tex_input_directory present |
| https://pylatexenc.readthedocs.io/ | OK — landing page, four-module overview |

(Anchor-item presence confirmed on each fetched page, covering the
`#pylatexenc.…` fragments of the sampled definitions.)

### 8. DOC_GAPS.md final state — ALL RESOLVED

| Entry | State |
|---|---|
| #1 CHECK condition-identifier display | RESOLVED (G2) |
| #2 CHECK WebAssembly mention | RESOLVED (G5 M1) |
| #3 GAP panic-contract sentence | RESOLVED (G5 M2) |

No OPEN entries remain.

### Non-trivial findings reported, NOT fixed (supervisor attention)

1. Public rustdoc referencing "(module docs)" of PRIVATE modules: the
   pattern "…(module docs)" on re-exported items sometimes points at an
   internal module page invisible in the public build (fixed the two
   verbatim_parser cases via a public anchor / self-containment in M3;
   e.g. constructs/mod.rs:769 "tier-2 **temporaries** (module docs)"
   remains, though the tier vocabulary itself is publicly defined in the
   construct-parsers guide chapter). A systematic repointing is beyond
   trivial scope.
2. `[§dd-dr:…]` references in public rustdoc (panic-policy family +
   builder.rs:78 ext-minting): the panic-policy wording is the user's own
   same-day ruling text (5611d2b); whether the dev-docs-pointer pattern
   should be repaired per Documentation_Structure's four-case rule is a
   policy question for the user — left untouched.
3. "footgun" (extract.rs parse_keyval doc) — informal metaphor in public
   rustdoc, pre-existing; left (outside M3's process-wording mandate).

## M5 — closure (fills in after execution)
