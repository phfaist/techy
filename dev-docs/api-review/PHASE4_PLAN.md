# Phase 4 — Guides: Execution Plan  [v3 — RULED 2026-08-05; in execution]

Working scaffolding (deleted with this directory at review completion). Governs the
Phase 4 deliverables. Master status stays in PLAN.md. All structure/size/rehoming
decisions below are user rulings (2026-08-05 session, two rounds); the supervisor
holds merge authorization (see § Protocol).

## Method rules (binding for every drafting agent)

- **Written from public documentation only**: chapter content comes from public
  documentation — the rendered `cargo docs` output, doc comments, signatures, and
  existing guide pages. Agents never derive behavioral claims from implementation
  bodies. Every behavioral claim in a guide must be traceable to a documentation
  sentence or demonstrated by a compiling, running doctest in the chapter itself.
  The migration chapter additionally reads the pylatexenc *Python* sources
  (`$HOME/Research/util/pylatexenc/`) and links every referenced pylatexenc concept
  to https://pylatexenc.readthedocs.io/ (verify links resolve). No exploring
  outside the techy and pylatexenc root folders.
- **NEVER ASSUME code behavior** (user). If documented behavior seems weird,
  misaligned with the library's intent, or possibly in tension with a
  ruling/design rationale → file a CHECK entry in DOC_GAPS.md (this directory);
  if documentation is incomplete → file a GAP entry; never write guide text from
  an assumption. Entries are resolved by dedicated verification agents (promptly,
  or in G5), not by drafting-agent guesswork.
- **No duplication of API rustdoc**: guides complement and synthesize; they are
  structured by use case / target audience. API-item-local explanations (recipes
  for one function, contract details) live in that item's rustdoc; the guide gets
  at most a one-sentence pointer. Corollary (user ruling): **extensive
  information on API use belongs in the corresponding module's documentation**
  (techy::transform, techy::visit, techy::recompose, techy::extract, …) — guide
  chapters give the high-level picture and point there.
- **REMOVE, DO NOT SUMMARIZE** (user): if a chapter exceeds its length target,
  entire explanations are removed — either without trace (below the "keep"
  significance threshold) or replaced by a one-sentence pointer to the API items
  documenting the feature; the pointer itself obeys all clarity rules. No
  telegraphic prose to squeeze under limits.
- **Length targets (user)**: User Guide chapters ≤ ~10–15 kB, with two ruled
  exceptions — learn-by-example.md ≤ ~30 kB, specs.md ≤ ~20 kB. Developer Guide
  chapters: the length they need under a high keep-threshold, soft cap ~30 kB.
  AI Guide: root ≤ ~30 kB (≈8k tokens); sub-chapters ≤ ~60 kB each, same
  distillation discipline.
- **Writing rules (user, standing)**: no metaphors; no jargon coined during the
  review; every technical term defined before use at a location easy to find from
  any use site (link or repeat the definition); prioritize what the likeliest
  readers of the page need, first; short snippets where they make explanations
  more precise; assume NO familiarity with the code base, the rest of the API, or
  pylatexenc; link important concepts to the concepts-overview page; established
  documentation best practices.
- **Documentation_Structure.md governs form**: chapters live in docs/, wired in
  four steps (file; `guide` module block in techy/src/lib.rs; `GUIDE_PAGES` in
  docs/rustdoc-header.html; index in docs/guide.md); `rust`-fenced examples run
  as doctests; intra-doc links compiler-checked; published heading slugs
  immutable; user-facing pages never reference dev-docs (four-case repair rule).
- Guides print only post-Phase-3 paths and wire identifiers (P5 ruling).
- **Soft freeze in force**: Phase 4 is documentation-only (rustdoc additions
  included). Any API shortcoming exposed by drafting → escalate to the user.

## Chapter map (RULED)

**Landing page — docs/guide.md**: 2–3 paragraphs summarizing what techy is and
does, then the guide structure with a one-sentence description per chapter,
pointing to introduction.md as the obvious next read. (Chapters not yet written
appear in the index with their one-sentence description and a "being written"
stub page.)

**User Guide** (≤ ~10–15 kB unless stated):

| File | Content |
|---|---|
| introduction.md | Short. The library's intent, target users, capabilities. The anticipated use levels (ready-made parser for latexlike languages with custom definitions → low-level parsing extensions) and use realms (self-contained executable; embeddable `no_std`/WebAssembly build — verified: alloc-only no_std; inside a PyO3 Python extension). The guide structure (User / Developer / AI). NOT the embedder findings (those live in integration.md). |
| language-syntax.md | What a "latexlike" language is: macros, environments, specials, comments, groups; definitions changeable during the parse. No default definitions beyond `\begin`/`\end`; minidefs/minilatex for quick start and debugging; refer to specs.md for definitions. ~2 closing paragraphs: latexlike is a preset extension of the core concept set — macros/environments/specials are special cases of callables that the `latexlike` preset defines; a different preset could define other, orthogonal kinds of callables. |
| node-trees.md | What the parser produces: the node tree; the node kinds briefly and how they relate; high-level description of what techy can do with node trees (extraction via techy::extract, transformation via techy::transform, recomposition, visiting, …) with pointers into the module docs. Relatively short. |
| specs.md (≤ ~20 kB) | Defining callables and their behavior (macros, environments, specials) for latexlike languages: Packages, convenience one-liners, spec classes (MacroSpec, …); pointer to construct-parsers.md for full custom-parsing takeover; convenience providers incl. InputMacroSpec (`\input`); hosts the **standard-filesystem SourceResolver recipe** (doc-tested; user rehoming). One brief paragraph pointing general (non-latexlike) languages to the Developer Guide + API entry points. |
| parsing.md | Running the parser; strict/tolerant recovery; the direct settings/knobs; initial parsing state; diagnostics — the rule "match conditions via `T::IDENTIFIER` / `is::<T>()`, never literal strings" + a link to the auto-generated DiagnosticInfo implementors listing (NO duplicated identifier table — user ruling; CHECK entry: every condition type's page must display its identifier string, else GAP → rustdoc fix). |
| learn-by-example.md (≤ ~30 kB) | REVISED: a small curated example set based on the existing page, re-curated in light of the five persona walkthroughs, illustrating a large chunk (~60%) of techy's capabilities. |

**Developer/Hacker Guide** (soft cap ~30 kB each):

| File | Content |
|---|---|
| concepts-overview.md | KEPT (user; Documentation_Structure.md-mandated anchor page): the 12 concept sections (plan v3 miscounted 14; corrected after G1) each expanded to a compact, self-contained definition (a few sentences + links; headings frozen). New sections only via user escalation, if a later chapter needs an anchor with no home. Indexed under the Developer Guide. |
| parsing-model.md | The parsing model: how parsing is executed and delegated (parsing entry points, custom construct parsers, spec traits, …). Replaces the stub. |
| construct-parsers.md | How to define a custom construct parser — return types (argument parser? simple construct nodes? …); takeover parsing + `stage_invocation`. |
| custom-lang.md | Specifying the aspects of a custom language: callable and group types, Ext types attaching custom information to nodes, etc. Knobs grouped by feature, pointing to `Lang`'s API doc (no duplication). What LatexlikeLang already implements and how to extend it. Hosts the finalize_transition replay-granularity note (S6) and the specials-wiring trap pointer. |
| integration.md | NEW (user ruling; named `integration` over the sketched `ext` — avoids collision with the API's node-extension `Ext` vocabulary). Guide for tooling, extensions, embedding, and bindings: the key non-obvious embedder/bindings findings (T5 I-9: Arc+NodeId owned-handle pattern; visitors/`annotate` not `Send`; synthesized-node recipe via the pillar functions + `ParsingStateStack::from_node_ancestors`; `Severity` match-exhaustiveness; `LineIndexCache` as the bindings-side line/col handle; streaming recomposition = recomposer-held writer) AND the tooling starting points (navigation `node_at`/`covering_slice`/`parent`; line/col ownership incl. `LineColProvider`; the re-parse/span-stability rule: hold your own `Arc<Source>` + `parse_source`). Each finding = a few sentences + API links; depth stays in rustdoc. |
| pylatexenc-migration.md | Short, NOT exhaustive: main core concepts only + non-obvious mappings between pylatexenc (v2 and v3, one page) and techy; every pylatexenc concept linked to its readthedocs page. |

**AI Guide** (structure adopted by user ruling; optimized for loading into an AI
context — compressed but obeying all clarity rules):

| File | Content |
|---|---|
| ai-guide.md (≤ ~30 kB) | Root: orientation (what techy is, module topology, type map), highest-frequency task recipes (parse → read → extract → diagnose), pitfalls index, pointers to sub-chapters. |
| ai-guide-definitions.md | Defining macros/environments/specials; packages, scopes, argument codes; the F5 trap notes. |
| ai-guide-trees.md | Reading/navigating trees; extract, restage, recompose, visit; annotations; restage→recompose pipeline; reconstruction doctrine. |
| ai-guide-custom-lang.md | Implementing Lang, drivers, token rules, construct parsers; the latexlike generalization (role traits, LLL). |
| ai-guide-embedding.md | Bindings/threading facts; multi-source parsing + include wiring; tooling entry points; no_std. |
| ai-guide-pylatexenc.md | Dense pylatexenc v2/v3 → techy mapping tables (the PLAN.md "AI migration guide" deliverable). |

## Rehoming of routed obligations (RULED)

| Obligation (source) | Ruled home |
|---|---|
| T5 I-9 embedder/bindings findings | integration.md (NOT the introduction) |
| Tooling starting points (T4) | integration.md |
| post_space re-emission note (T5 I-10) | API doc only (the relevant item's rustdoc; policy remains techy-totext's) |
| Std filesystem SourceResolver recipe (T4) | specs.md (doc-tested) |
| `\input` caching challenges + conditional recipe (T5-G) | API doc only (brief — user: less important than v2 made it seem) |
| Wish-23/F9 identifier table | SUPERSEDED: parsing.md states the matching rule + links the DiagnosticInfo implementors listing; CHECK that each condition page shows its identifier |
| Custom-Lang finalize_transition replay granularity (S6) | custom-lang.md |
| Reconstruction doctrine + visit/transform/recompose usage (T5-H, P4) | extensive API-use information in the module docs (techy::recompose, techy::transform, techy::visit, …); node-trees.md gives the overview + pointers |
| T1/T2 doc folds (generic `NodeRef::name()`; `body()` None semantics; `descendants()` self-inclusion; argument-codes enum alternative; body-scoped definitions; F5 traps) | learn-by-example revision + specs.md where use-case-shaped; item-local sentences → rustdoc via GAP entries |
| Module-header narratives promotion (S1) | drafting agents may promote passages into chapters (rustdoc stays authoritative) |

## Stage breakdown (serial landing; worktree protocol)

- **G1 — Skeleton + landing + introduction + concepts**: guide.md reworked into
  the ruled landing page; introduction.md written; concepts-overview.md expanded
  compactly; ALL remaining chapters created as one-line "being written" stubs
  with full four-step wiring (so every cross-chapter link resolves from day one);
  DOC_GAPS.md register created.
- **G2 — User Guide**: language-syntax, node-trees, specs, parsing,
  learn-by-example revision. Includes the module-doc sufficiency pass for
  techy::{extract, transform, visit, recompose} (node-trees.md points there;
  expansions land as rustdoc in this stage) and the two API-doc-only rehomed
  notes (post_space; `\input` caching, brief).
- **G3 — Developer Guide**: parsing-model, construct-parsers, custom-lang,
  integration, pylatexenc-migration. Chapters are disjoint files (wiring landed
  in G1), so parallel drafting agents within the stage are allowed; the
  migration chapter's agent is the only one reading pylatexenc sources.
- **G4 — AI Guide**: root + five sub-chapters, compressed from the finished
  human chapters (hence after G2/G3).
- **G5 — Verification + final audit**: resolve every DOC_GAPS entry (CHECK
  entries verified by agents against code/tests/rulings; GAP entries fixed in
  rustdoc); full link sweep (`rm -rf target/doc && cargo docs`); length-target
  audit (REMOVE rule); terminology/jargon sweep; superseded-names sweep; wiring
  audit; PLAN.md Phase 4 closure entry.

## Protocol (per stage)

- Implementer agent in a worktree, branch `phase4-g<N>-<slug>`; plan-first commit
  into reports/G<N>_REPORT.md, then per-milestone commits; commit messages
  `P4-G<N>: <what>`. Oversized-context relay discipline and
  interruption-resume rules carry over from PHASE3_PLAN § Protocol verbatim, as
  does the rulings-revision escalation rule.
- Independent reviewer agent (read-only, same worktree): re-runs gates; verifies
  every behavioral claim is doc-traceable or doctest-demonstrated; checks length
  targets, writing rules, no-duplication, wiring, DOC_GAPS discipline; verdict
  table to the supervisor.
- **Merge authorization (user, 2026-08-05)**: the supervisor reviews the actual
  documentation text carefully and **commits & merges automatically once
  convinced the documentation is accurate** — no per-stage user sign-off gate.
  Escalations to the user remain mandatory for rulings tensions, API
  shortcomings, and accuracy doubts the records cannot settle. The user reviews
  merged text at leisure; feedback lands as follow-up commits.
- Merge mechanics as Phase 3: rebase inside the stage worktree onto api-review,
  `--ff-only` merge in the primary checkout, gates re-run on the merged tree,
  worktree/branch cleanup.

## Gates (every stage)

- `cargo build` unchanged; full `cargo test` green incl. `cargo test --doc`;
  `rm -rf target/doc && cargo docs` clean under deny lints (missing_docs, broken
  intra-doc links).
- `scripts/check_semver.sh` passes (documentation stages must not move the API).
- Four-step wiring complete for every touched chapter; length targets respected
  via the REMOVE rule.
- No rustdoc duplication introduced; writing rules honored; superseded-names
  sweep clean; zero behavioral assumptions (DOC_GAPS entries instead).

## Status log

- 2026-08-05: v1 (12-chapter map) superseded by user trim rulings; v2 drafted;
  v3 RULED same day (second ruling round): separate introduction.md + minimal
  landing page; learn-by-example ≤ ~30 kB and specs.md ≤ ~20 kB exceptions; AI
  structure/limits adopted as proposed; NEW dev chapter integration.md (named by
  supervisor under pick-better-name license) hosting embedder findings + tooling
  starting points; post_space and `\input`-caching notes to API docs only;
  FS-resolver recipe to specs.md; concepts-overview kept as a dev-guide page;
  supervisor merge authorization granted. Execution begins with G1.
