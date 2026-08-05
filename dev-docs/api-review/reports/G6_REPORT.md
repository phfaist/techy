# Phase 4 — G6 report: escalated-finding fixes + exhaustive Panics roster + full clarity sweep

Branch `phase4-g6-clarity` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a02801c5917c4b420`,
branched from `api-review` @ bc412d3 = Phase 4 complete, G5 merged).
Status: **M0 (plan)** — inventory and edits follow in M1–M5.

Note on branch point: the worktree's HEAD at agent start was a stale commit
(2110bbb); the branch was reset to the local `api-review` tip bc412d3 before
any work, per the brief.

## Plan (Milestone 0 — resume from here if interrupted)

User rulings in force (2026-08-05): (1) fix all three escalated findings —
documentation text may be reworded freely (the "never undo my edits" caution
is lifted for documentation text this stage); (2) full clarity sweep of ALL
user-facing documentation — no metaphors, no jargon, no non-ubiquitous
acronyms, terms defined before use at findable locations — with docs/ai-guide*.md
exempt (except minimal same-term consistency renames where a rustdoc
vocabulary rename would leave an AI-guide term dangling); (3) the crate-level
Panics section becomes the exhaustive, item-by-item list of user-exposed
panicking API, and Documentation_Structure.md gets a brief note naming it as
such, to be maintained.

Scope facts established at M0:

- **What renders publicly** (sweep domain): `//!` docs of the public modules
  (lib.rs; core/mod.rs + core/{constructs,node,specs}.rs facades; error.rs;
  extract.rs; visit.rs; recompose/mod.rs; transform/mod.rs; source/mod.rs;
  latexlike modules) and `///` docs on publicly re-exported items. `//!` docs
  of `pub(crate)` topic modules (src/token, src/state, src/constructs, …) do
  NOT render publicly — they are developer-facing and out of sweep scope,
  which is also the root cause of finding (a): "(module docs)" pointers on
  re-exported items that point at pages invisible in the public build.
  Classification of each pointer (public target = fine, private target =
  broken) will be verified against the rendered HTML (`cargo docs`).
- **Human guide chapters** (13 files, in scope): guide, introduction,
  concepts-overview, language-syntax, node-trees, specs, parsing,
  learn-by-example, parsing-model, construct-parsers, custom-lang,
  integration, pylatexenc-migration.
- **Initial term counts** (doc-comment lines in techy/src + techy-derive/src /
  human guides / AI guides): funnel 38/5/0 · seam 25/5/1 · choke point 19/1/0 ·
  scaffold 19/1/0 · pillar 18/5/4 · splice 9/1/1 · gobble 8/1/0 ·
  sanction 7/0/1 · staging door 4/3/2 · umbrella 3/3/0 · hot path 3/0/0 ·
  footgun 2/0/0 · residue 2/0/0 · escape hatch 2/0/0 · happy path 1/1/0 ·
  load-bearing 1/0/0 · airtight 1/0/0 · smuggle 1/0/0 · satellite 2 ·
  story 2. Provisional keeps (final call in M1 with per-term justification):
  mint (153 — ordinary dictionary verb used literally, accepted project
  vocabulary), sugar (16 — "syntactic sugar" is ubiquitous PL vocabulary),
  first-class (9 — ubiquitous PL vocabulary).
- **Finding (b) inventory** (dev-doc references in doc comments): 10 sites —
  token/token.rs:154, token/reader.rs:91+181, source/span.rs:29,
  source/source.rs:233+374, spec/mod.rs:84, constructs/argument_parsers.rs:675,
  constructs/nodes_parser.rs:479, node/builder.rs:78. No `§dd-`/dev-docs
  references in the guide chapters (verified by grep). Repair is
  Documentation_Structure.md case (B): short self-contained rationale; the
  panic-policy family links to the crate-level Panics section written in M3.
- **Finding (c)**: `footgun` at extract.rs:728 (parse_keyval) and
  text_content.rs:24; a third occurrence at node/slice.rs:209 is a plain `//`
  code comment (not rendered; out of scope).

### Milestones

- **M1 — violation inventory (this file, committed before any mass edit)**:
  read the guide chapters fully and the public-module docs fully; classify
  every hit (metaphor / undefined jargon / acronym / process-residue); decide
  one consistent replacement per vocabulary cluster (staging door, splice
  door, pillar functions, funnel, choke point, seam, scaffolding, umbrella,
  gobble, …); record keeps with one-line justifications; run an acronym sweep;
  build the full site table below.
- **M2 — the three findings**: (a) repair every "(module docs)"-style pointer
  whose target page is invisible publicly (repoint to a public carrier or
  make the sentence self-contained); (b) replace the 10 dev-doc references
  with self-contained rationale (panic family → link to crate-level Panics);
  (c) covered by the footgun rewording.
- **M3 — Panics**: derive the complete public panic roster (grep `# Panics`
  in public rustdoc; cross-check against DESIGN_RATIONALE [§dd-dr:panic-policy]
  as a read-only completeness oracle); rewrite the crate-level `## Panics`
  section as the exhaustive, item-linked list grouped by the two families
  (always-on precondition asserts; indexing-style accessors with
  non-panicking companions), keeping the current opening sentence; add the
  maintenance note to Documentation_Structure.md.
- **M4 — apply the sweep** per the M1 table: rustdoc first, then guides, then
  minimal AI-guide same-term renames; meaning preservation absolute (each
  metaphor that carried a contract gets the contract stated plainly); update
  the table with applied/kept.
- **M5 — closure**: cargo build 0 warnings; cargo test counts unchanged
  (758+30+8+21+1; 66 doctests + 2 ignored); rm -rf target/doc && cargo docs
  zero warnings; scripts/check_semver.sh green; rendered-HTML grep — no swept
  term, no `§dd-` reference in public pages (AI-guide pages excluded for
  sweep terms); superseded-names check on all replacement vocabulary;
  complete this report.

Method rules: meaning preservation absolute; replacement text in plain,
complete sentences with terms defined before use or linked; no new metaphors;
no assumed behavior; PLAN.md / PHASE4_PLAN.md untouched; code semantics
untouched (doc text only).

## M1 — violation inventory

(to be filled)

## M2 — findings

(to be filled)

## M3 — panic roster

(to be filled)

## M5 — closure

(to be filled)
