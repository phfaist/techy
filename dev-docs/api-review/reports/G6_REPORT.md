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

Method: every doc-comment line of techy/src + techy-derive/src (10,320 lines)
pattern-swept; all 13 human guide chapters read in full; public-page map taken
from a fresh `cargo docs` build (renders: crate root, core + its three
satellite facades, error, extract, visit, recompose, transform, source,
latexlike, latexlike::minidefs, the guide pages, techy_derive). `//!` docs of
`pub(crate)` topic modules and `#[cfg(test)]` item docs do not render and are
deliberately NOT edited (developer-facing; outside this stage's scope) — all
listed sites below are rendered ones unless marked (guide).

### Vocabulary-cluster decisions (REPLACE)

| Cluster | Classification | Decision (consistent replacement) |
|---|---|---|
| "staging door" / "the one door" / "canonical door" | metaphor (coined) | Name the method and state the contract plainly: [`ParseContext::stage_node`] is "the **single staging entry point**: every parsed node enters the tree through it (and it is the one automatic `Lang::make_node_ext` site)". "The two staging doors" → "The two staging entry points". |
| "sanctioned splice door" (restage_node) | metaphor (coined) | "`restage_node` is the supported entry point for cross-tree assembly — it accepts nodes from any tree **by contract**" ("sanctioned" → "supported"/"by contract"). |
| the attached-source "door" (parse_attached_source running text) | metaphor (coined) | Name the method ("`parse_attached_source`" / "this entry point") per sentence. |
| "behavior door" (invocation_syntax.rs) | metaphor | "through the parser-factory override (`make_invocation_parser`)". |
| "recover funnel" / "the funnel" | metaphor (coined) | "the **recovery entry point**", defined at [`ParseContext::recover`] ("every problem a construct parser detects in the source is reported through this one method, which applies the driver's recovery policy") and at [`ParseDriver::recover`] ("the recovery hook"). Running-text sites name the method or say "the recovery entry point"; "bypasses the funnel" → "ignores the recovery policy" (exact semantics). |
| "funnel pattern" (EnvironmentSpec/EnvironmentBehavior — a DIFFERENT use) | metaphor (coined) | "the concrete **wrapper**": EnvironmentSpec is the one concrete wrapper type through which the `\begin` composition reaches the open set of `EnvironmentBehavior`s (`Any` downcasts hit concrete types only). |
| "choke point" (transition/derivation) | metaphor (coined) | "the **single derivation point**" (matches parsing-model.md's existing "There is exactly one derivation point"): `ParsingState::derived` = the sole constructor of non-initial states; `ParseContext::derive_state` = "the parser-facing derivation entry point"; "the bare choke point" → "`ParsingState::derived` called directly (no driver present to lower events)". |
| "seam" | metaphor (jargon; Feathers coinage) | Context-specific plain terms: "extension point", "interception point", "customization point", or name the trait/method ("every fallible operation returns a `Result`"; "the trait the rendering entry points accept"). |
| "scaffolding" (environment begin/end syntax) | undefined coined term | "the **begin/end syntax**" / "the `\begin{name}` syntax" (facts: "begin/end syntax facts"); generic uses ("escape character, post-space, environment scaffolding") → "an environment's begin/end syntax". |
| "pillar functions" / "the pillars" | metaphor (coined) | "the preset's **behavior functions**" (the driver's whole behavior as public `LLL`-generic free functions); per-function: "the math-interior behavior function", "the exit-math behavior function", "the paragraph-break behavior function". |
| "umbrella (trait)" (LatexlikeLang) | metaphor | "the **language-family trait**". |
| "footgun" | jargon (slang) | Plain rewording per site ("a recurring source of mistakes"; "would give misleading results"). |
| "hot path" / "hot success path" / "cold path" / "hot-path filter" | performance jargon | Plain description per site ("the per-token scanning loop", "once per construct during normal parsing", "rendered only at snapshot time", "fast pre-filter"). |
| "escape hatch" | metaphor | "the full-takeover **route**" (matches construct-parsers.md's existing wording). |
| "happy path" | metaphor (jargon) | "in normal use" / "in the common case". |
| "load-bearing" | metaphor | "essential" (with the reason stated). |
| "airtight" / "smuggle" | metaphor | Plain restatement ("no state can bypass the derivation point"; "would put policy inside the transition mechanism"). |
| "doctrine" (span-stability, accuracy, noise-ownership, placement) | coined term | "rule" ("the span-stability rule", "the accuracy rule", …). |
| "canned" | metaphor (informal) | "ready-made" / "standard". |
| "conjure(d)" | metaphor | "invent(ed)". |
| "swallow(s)" | metaphor | "consume(s)". |
| "gobbled … out" in prose NOT tied to the API name | metaphor | plain "consumed"/"designated out" — but see the `gobble` KEEP below for text tied to the API name. |
| "baked in" (the enable-gate wording) | metaphor | "applied at freeze time" (with the no-branching consequence stated). |
| "satellite(s)" (modules) | metaphor | "submodules". "crisp boundaries" → "clear boundaries". |
| "story" ("the … story on one page") / "mutually recursive heart" | metaphor | "flow" / "mutually recursive cluster". |
| "residue" ("no ancestry residue") / "dies with the session" | metaphor-flavored | "it is dropped with the session — no ancestry data survives into parsed material". |
| "wall off" / "cuts both ways" / "out of the box" / "on the table" / "the trap bites" | idioms | plain restatements per site. |
| "the node ext mint" (noun, custom-lang.md heading) | coined noun | "The node ext hook" (the verb "mint" is kept — see below). |

### KEEP decisions (with justification)

| Term | Justification |
|---|---|
| mint (verb, 153 sites) | Ordinary dictionary verb used with its literal sense (create a fresh value/id); standard technical usage (minting tokens/identifiers); accepted project vocabulary (used in the user-maintained CLAUDE.md topology). Noun uses ("the … mint") are rewritten. |
| splice (verb/adjective) | Standard sequence-editing vocabulary (std's `Vec::splice`); "splice door" is gone per the cluster above. |
| sugar ("accessor sugar", "syntactic sugar") | Ubiquitous PL vocabulary; also in CLAUDE.md topology. |
| first-class | Ubiquitous PL vocabulary. |
| gobble (VerbatimBodyParser docs) | Bound to the public method name `with_gobble_leading_newline` (identifier; code rename out of scope this stage); its meaning is precisely defined at the method ("staged as leading whitespace but designated out of the content"). Prose uses away from that API are replaced. |
| trap / pitfall | Ordinary dictionary words used literally ("a documented trap"); only "where the trap bites" is reworded. |
| recipe | Ordinary dictionary word, standard docs usage. |
| monolith | Standard software-architecture vocabulary. |
| for free | Standard technical idiom ("comes for free"), widely understood. |
| worked example | Plain English. |
| plug in / plug-in / data plug is REPLACED where "the (math/data) plug" names a thing (→ behavior function / channel), KEPT as the ordinary verb "plugs into". |
| RAII (1 site) | Ubiquitous Rust vocabulary (used by the Rust book/std docs). |
| AST | Expanded at first use at the crate root and in introduction.md ("Abstract Syntax Tree (AST)"); ubiquitous for the audience. |
| FFI | Near-ubiquitous for the Rust audience; the two argument-context sites spell "foreign-function" adjacent to it; expanded parenthetically where bare. |
| Acronym sweep result | Guides + rustdoc contain no other niche acronyms (checked: FFI, RAII, AST, plus a CamelCase-outlier scan of all chapters — remaining all-caps tokens are type names and AI/API/ASCII/UTF-8/HTML/LaTeX/TeX). |

### Finding (b) — dev-doc references in rendered rustdoc (10 sites; all case (B))

token/token.rs:154 (`Token::new`), token/reader.rs:91 (`skip_whitespace`),
token/reader.rs:181 (`StdTokenReader::move_to_pos`), source/span.rs:29
(`Span::new`), source/source.rs:233 (`SourceSpan::new`), source/source.rs:374
(`SourcePos::new`), spec/mod.rs:84 (named-first constructor family),
constructs/argument_parsers.rs:675 (staged-id degradation note),
constructs/nodes_parser.rs:479, node/builder.rs:78 (ext-minting). The six
panic-family sites get a link to the crate-level Panics section; the others
get one self-contained rationale sentence. Guides contain zero dev-doc
references (grep-verified).

### Finding (c) — footgun

extract.rs:728 (`parse_keyval`) and source/text_content.rs:24 (rendered);
node/slice.rs:209 is a non-rendered `//` code comment (left).

### Finding (a) — "(module docs)"-style pointers (classification)

FINE (target page public; no change needed): all sites in extract.rs (7),
visit.rs, recompose/mod.rs + recompose/context.rs (`super` = public
recompose), transform/mod.rs + bundles.rs + context.rs (`self`/`super` =
public transform), engine/mod.rs:168? no — see broken list.

BROKEN (public item, pointer targets a private module page) — the fix per
site is (i) repoint to the public `core::constructs` facade after expanding
it to carry the three module-level contracts it is publicly claimed to carry
(two-tier ownership; caller-applies-deltas state threading; the
`Err`-means-abort error contract), (ii) repoint to a public item that carries
the content, or (iii) make the sentence self-contained:

- token/rules.rs:203 — self-contain ("default language" explanation).
- spec/callable.rs:104, constructs/mod.rs:799, engine/driver.rs:413,
  constructs/invocation_parser.rs:154 — "StdInvocationParser's module docs"
  → the contract summary moves onto `StdInvocationParser`'s own item docs;
  pointers repoint to the item.
- constructs/mod.rs:129 (state-threading), :760 (`Err` contract), :769
  (two-tier) — repoint to the expanded [`core::constructs`] facade.
- constructs/argument_parsers.rs:135, nodes_parser.rs:292 + :644,
  embellishments_parser.rs:69, environment_parser.rs:433 + :498,
  group_parser.rs:101, tack_on_parser.rs:75 + :97,
  chars_group_parser.rs:63/96/103/111, latexlike/environments.rs:464 + 589,
  latexlike/input.rs:76 + 141, node/arguments.rs:93, node/builder.rs:249,
  engine/driver.rs:73, engine/mod.rs:168 + 327 (`state_memo` is not public)
  — self-contain (state the needed fact inline).
- source/source.rs:423 "module-level invariant" — verify the public source
  facade states it; repoint or self-contain accordingly.
- Guide claims that the `core::constructs` module documentation carries the
  contracts (construct-parsers.md 13–14, 40–41; parsing-model.md 181, 231,
  283) — become true via the facade expansion (i).

Non-rendered pointer sites left unchanged: token/list_reader.rs:41 and
engine/state_memo.rs:95 (items not public), all `//!` sites in private
modules.

### Process residue in rendered docs (fix in M4)

extract.rs:162 "(7.8 decision …)"; environment_parser.rs:437 "per decision 8"
and :498 "per decision 8 (module docs)"; driver.rs:196's inaccurate
"builder's region-tiling assert panics" (see M3 findings). Test-only and
private-module residue (Phase/D-plan/S1/6.x labels) is not rendered and is
left.

## M2 — findings

(to be filled)

## M3 — panic roster

(to be filled)

## M5 — closure

(to be filled)
