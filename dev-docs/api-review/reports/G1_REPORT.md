# Phase 4 — G1 report: skeleton + landing + introduction + concepts

Branch `phase4-g1-skeleton` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a16640bd4b26a4196`,
branched from `api-review` @ 36ed4e9). Status: **IN PROGRESS** — plan committed
(Milestone 0).

Note on branch point: the worktree's HEAD at agent start was a stale commit
(2110bbb, predating the api-review series); the branch was reset to the local
`api-review` tip 36ed4e9 (Phase 4 plan v3 ruled) before any work, per the brief
("branched from the current HEAD, which is api-review").

## Plan (Milestone 0 — resume from here if interrupted)

Governing inputs read: PHASE4_PLAN.md (Method rules, Chapter map, Protocol,
Gates — binding), Documentation_Structure.md (four-step wiring, cross-reference
rules, no dev-docs references from user-facing pages), docs/guide.md,
docs/concepts-overview.md, docs/parsing-model.md, docs/rustdoc-header.html
(GUIDE_PAGES format), techy/src/lib.rs (crate-level rustdoc + `guide` module
block). learn-by-example.md is G2 scope — not modified.

### Milestone 1 — chapter skeleton (commit `P4-G1: chapter skeleton + wiring`)

Create as stubs (heading + `*(This chapter is being written.)*`, nothing else):
docs/introduction.md, language-syntax.md, node-trees.md, specs.md, parsing.md,
construct-parsers.md, custom-lang.md, integration.md, pylatexenc-migration.md,
ai-guide.md, ai-guide-definitions.md, ai-guide-trees.md, ai-guide-custom-lang.md,
ai-guide-embedding.md, ai-guide-pylatexenc.md. Normalize docs/parsing-model.md
to the same stub style.

Wire every chapter (new + existing) through all four steps:
(a) file in docs/; (b) `#[doc = include_str!(...)] pub mod <snake_case> {}` in
the `guide` block of techy/src/lib.rs; (c) GUIDE_PAGES entry in
docs/rustdoc-header.html ordered: Overview (landing) first, then User Guide in
plan order (introduction, language-syntax, node-trees, specs, parsing,
learn-by-example), then Developer Guide (concepts-overview, parsing-model,
construct-parsers, custom-lang, integration, pylatexenc-migration), then AI
Guide (ai-guide root + five sub-chapters); (d) index entries in docs/guide.md
(landed with Milestone 2's rewrite).

Planned module names: introduction, language_syntax, node_trees, specs,
parsing, learn_by_example (exists), concepts_overview (exists), parsing_model
(exists), construct_parsers, custom_lang, integration, pylatexenc_migration,
ai_guide, ai_guide_definitions, ai_guide_trees, ai_guide_custom_lang,
ai_guide_embedding, ai_guide_pylatexenc. Module declarations in lib.rs follow
the same reading order as GUIDE_PAGES.

### Milestone 2 — landing page (commit `P4-G1: landing page`)

Rewrite docs/guide.md: 2–3 paragraphs on what techy is/does drawn from the
crate-level rustdoc (no new claims); then three index sections (User Guide /
Developer Guide / AI Guide), each chapter an intra-doc link
(`crate::guide::<module>`) with a one-sentence reader-facing description
derived from the PHASE4_PLAN chapter map; explicit pointer that introduction
is the next read. concepts-overview indexed under Developer Guide. No process
references in the page.

### Milestone 3 — introduction.md (commit `P4-G1: introduction chapter`)

Per chapter-map row: intent, target users, capabilities; use levels
(ready-made latexlike parser with custom definitions → low-level parsing
extensions); use realms (self-contained executable; embeddable no_std /
WebAssembly build — cite the crate-level rustdoc's no_std section; use inside a
PyO3 Python extension — modest claim, file a CHECK in DOC_GAPS if more than
the docs state is needed); the three-part guide structure. NOT the
embedder/bindings findings (integration.md, G3). Target well under 10–15 kB.
All behavioral claims traceable to public documentation.

### Milestone 4 — concepts-overview.md (commit `P4-G1: concepts overview expansion`)

Expand each of the 14 existing sections into a compact self-contained
definition (a few sentences, present tense, links to the embodying API items).
Headings frozen (published anchors); no deletions; reorder only if truly
necessary (not planned). Every explanation traceable to public documentation
(module docs, item rustdoc, crate root). Remove the "(This page starts as a
skeleton…)" placeholder note. Target ≤ ~12 kB.

Claim sourcing method for M2–M4: read rendered rustdoc / doc comments only
(module-level //! blocks, item /// blocks, signatures); no behavioral claims
from function bodies. Anything not supported → DOC_GAPS entry + drop or weaken
the claim.

### Milestone 5 — DOC_GAPS + report closure (commit `P4-G1: DOC_GAPS register + report`)

Create dev-docs/api-review/DOC_GAPS.md: header explaining GAP vs CHECK entry
types and the format `## <N>. [GAP|CHECK] <title>` with fields Raised-by,
Question/Claim, Why it matters, Status. Seed with the ruled CHECK (every
diagnostic condition type's rustdoc page must visibly display its stable
identifier string) + entries accumulated in M2–M4. Finish this report (file/
size table, gate results, deviations, reviewer notes) and commit.

### Gates (before final commit)

`cargo build` (clean); `cargo test` (all suites); `cargo test --doc`;
`rm -rf target/doc && cargo docs` (zero warnings); `scripts/check_semver.sh`;
verify each of the ~16 chapters renders under techy::guide in target/doc and
appears in the GUIDE_PAGES sidebar list.

## Results

(to be filled at Milestone 5)
