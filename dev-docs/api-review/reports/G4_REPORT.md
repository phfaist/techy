# Phase 4 — G4 report: AI Guide

Branch `phase4-g4-ai-guide` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a283fa102a3175a1f`,
branched from `api-review` @ 855adb7). Status: **M0–M7 COMPLETE** — all
gates green after M6; awaiting stage review + merge.

Note on branch point: the worktree's HEAD at agent start was a stale commit
(2110bbb, predating the api-review series); the branch was reset to the local
`api-review` tip 855adb7 (G3 merged) before any work, per the brief.

## Plan (Milestone 0 — resume from here if interrupted)

Governing inputs read: PHASE4_PLAN.md (Method rules, AI Guide chapter-map
rows, Protocol, Gates — binding), Documentation_Structure.md (wiring,
doctest conventions; the six AI chapter files exist as wired stubs from G1),
ALL finished human guide chapters (guide.md, introduction.md,
concepts-overview.md, language-syntax.md, node-trees.md, specs.md,
parsing.md, learn-by-example.md, parsing-model.md, construct-parsers.md,
custom-lang.md, integration.md, pylatexenc-migration.md — the SOURCE
MATERIAL this stage compresses), the module docs of techy::{extract, visit,
transform, recompose, core, core::specs, core::constructs, core::node,
latexlike, source, error}, DOC_GAPS.md (append-only, never renumber).

What the AI Guide is (ruled): six chapters written to be loaded into an AI
assistant's context when it works on a project using techy. Optimize for an
AI reader: dense, compressed explanations; tables over prose where they
carry the same information; canonical paths spelled out; high signal per
token (a user pays for every token an agent loads). Compression is
sanctioned HERE (unlike the human guides) — but every clarity rule holds:
technical terms defined before use (or linked to concepts-overview), no
metaphors, no review-coined jargon, nothing ambiguous. Dense is not cryptic.

Standing method discipline for every milestone: PUBLIC DOCUMENTATION ONLY
(the human guides ARE public documentation — compress from them;
module/item rustdoc for anything they don't cover; never implementation
bodies). Every behavioral claim doc-traceable or demonstrated by a
compiling doctest; uncertainties → DOC_GAPS entries, never assumptions.
Every `rust` code block compiles and runs as a doctest (`no_run` only where
I/O demands it) — code is the densest honest medium for an AI reader.
Self-containment discipline: an agent may load ONLY the root, or the root +
one sub-chapter; the root stands alone for the everyday flow; each
sub-chapter re-states (one line) or links any term it leans on. Superseded
names must not appear ([§dd-dr:superseded-names] grep before final commit).
NEVER reference dev-docs/ or the review process. Markdown-only stage: do
not modify PLAN.md, PHASE4_PLAN.md, any human-guide chapter, any code
(rustdoc needs → DOC_GAPS). Published stub headings (`# AI guide`,
`# AI guide: definitions`, `# AI guide: node trees`,
`# AI guide: custom languages`, `# AI guide: embedding`,
`# AI guide: pylatexenc migration`) are kept — heading slugs are immutable.

Length: root ≤ ~30 kB HARD, aim lower; sub-chapters ≤ ~60 kB each, aim FAR
lower, distillation discipline (REMOVE, do not summarize into telegraphic
prose). Report a tokens estimate (bytes/4) per chapter at the gates.

Milestone order: sub-chapters first, root LAST (written against their final
content, so its pitfalls index and pointer table are accurate). One commit
per milestone, `P4-G4: <what>`. Context ≈400k+ → finish milestone, commit
handoff notes here, STOP.

### Milestone 1 — docs/ai-guide-definitions.md (commit `P4-G4: ai-guide-definitions chapter`)

Defining macros/environments/specials, compressed from specs.md +
language-syntax.md + learn-by-example.md + the argument_specs rustdoc
table. Cover: packages + one-liners (`define_macro`/`define_environment`)
vs `insert`; the argument-codes table (incl. `BracedOnly`, the `m`
single-expression fallback, named arguments via `argument_specs_named`,
compact strings via `argument_specs_from_str`); the spec types
(MacroSpec/EnvironmentSpec/SpecialsSpec); body deltas (math mode via
`with_body_delta`, verbatim via `EnvironmentSpec::from_behavior` +
VerbatimBehavior); scoped/body-scoped definitions (ScopeOp::Push;
Scope/DefinitionOp for `\newcommand`-style); mode-restricted visibility
(`set_visible_modes`/`insert_in_modes`); the F5 trap set (escape-char
registration; fallback swallowing; no spec-type/callable-type cross-check);
`\input` wiring (input_macro_spec + SourceResolver + pointer to the
filesystem recipe in specs.md).

### Milestone 2 — docs/ai-guide-trees.md (commit `P4-G4: ai-guide-trees chapter`)

Reading/navigating/consuming trees, compressed from node-trees.md +
learn-by-example.md + integration.md + the extract/visit/transform/
recompose module docs. Cover: kinds (NodeKind table), accessors, spans,
navigation (node_at/covering_slice/parent, descendants vs walk); extract
(all four producers + the three annotation spellings + callbacks);
transform/restage (visitor contract Descend/Emit, region ops, annotations
as the origin-tracking idiom); recompose (instruction model,
source_recomposer, streaming pattern); the restage→recompose pipeline; the
reconstruction doctrine (per-node recorded facts; spans are provenance,
never re-resolved).

### Milestone 3 — docs/ai-guide-custom-lang.md (commit `P4-G4: ai-guide-custom-lang chapter`)

Implementing a language, compressed from custom-lang.md +
construct-parsers.md + parsing-model.md. Cover: Lang contract summary
(associated-types table; make_node_ext the one required method);
vocabularies + role traits; token rules + the specials double-hook trap;
ext family (population-is-initialization); state ext + finalize_transition
obligations + the replay-granularity note; drivers + CommandResolver;
TrivialLang vs joining the latexlike family (LatexlikeLang, pillar
functions, projection pattern); construct parsers (ConstructParser/
ParseContext essentials, the two staging doors, ArgumentParser contract,
conditions + implementation errors).

### Milestone 4 — docs/ai-guide-embedding.md (commit `P4-G4: ai-guide-embedding chapter`)

Embedding facts, compressed from integration.md + introduction.md +
parsing.md. Cover: bindings/threading (Arc<NodeTree>+NodeId handles,
TreeTag; visitors/annotate not Send; Send/Sync via auto-trait listings;
Severity exhaustive three-variant); multi-source parsing + include-chain
policy (SourceResolver, check_include_chain, provenance); tooling entry
points (node_at/covering_slice/parent; LineIndexCache/LineColProvider +
scan cap; the re-parse/span-stability rule: own Arc<Source> +
parse_source); no_std/WebAssembly facts; streaming recomposition.

### Milestone 5 — docs/ai-guide-pylatexenc.md (commit `P4-G4: ai-guide-pylatexenc chapter`)

The dense mapping tables, compressed/expanded from pylatexenc-migration.md
(prose → table form where the human chapter uses prose): node classes;
spec/database mapping; argument-spec strings incl. the `[`/`{`/`*`
aliases; positions/spans byte-offset warning; tolerant → Recovery +
Diagnostics; latex2text non-mapping + the `\input` layer move. Reuse ONLY
the verified readthedocs URLs from docs/pylatexenc-migration.md (verified
at G3; /en/latest/ — /en/stable/ is 404); a new URL would need its own
verification — plan is to need none.

### Milestone 6 — docs/ai-guide.md ROOT, LAST (commit `P4-G4: ai-guide root chapter`)

HARD budget ≤ ~30 kB, aim lower. Orientation: what techy is (3–4
sentences); module topology (one table: module → what lives there → key
types); the everyday-flow type map (Language/ParsingState/Package → parse
→ ParseResult/NodeTree/NodeRef/Diagnostics); highest-frequency task
recipes as complete minimal doctests (parse a string; register a
macro/environment and parse; read/navigate the tree; extract text; handle
diagnostics incl. match-via-`T::IDENTIFIER`); a pitfalls index (one line
each, pointing to the sub-chapter or API doc that details it); pointer
table to the five sub-chapters + the human guides. Must stand alone for
the everyday flow. Written AGAINST the finished sub-chapters.

### Milestone 7 — gates + closure (commit `P4-G4: report closure — gates`)

`cargo build` · `cargo test` · `cargo test --doc` · `rm -rf target/doc &&
cargo docs` (zero warnings) · `scripts/check_semver.sh` · byte-size table
(root vs ~30 kB HARD; sub-chapters vs ~60 kB; tokens estimate bytes/4) ·
superseded-names sweep · four-step wiring check (wired in G1; content-only
changes) · markdown-only discipline check. Update this report: milestone
log, gates table, size table, DOC_GAPS delta, deviations, scrutiny
pointers.

## Milestone log

- M0: this plan. (Committed before other work.)
- M1: docs/ai-guide-definitions.md, 13,384 bytes, 4 doctests (registration
  with one-liners + insert + named-argument access; equation body delta;
  body-scoped ScopeOp::Push package; `\input` via MapResolver + attached
  slot). Full argument-code table compressed from the argument_specs
  rustdoc table (all 11 rows incl. word codes); the three-trap table
  (escape-char registration / `m` fallback / no cross-check) compressed
  from specs.md § Registration pitfalls; mode visibility, Scope/
  DefinitionOp, spec-type table, verbatim body, filesystem-recipe pointer.
- M2: docs/ai-guide-trees.md, 14,518 bytes, 3 doctests (read/navigate;
  extract split + keyval; restage→recompose pipeline). NodeKind table,
  navigation table, all four extract producers + the three annotation
  spellings, walk contract + three-channel discipline + role-blindness,
  restage Descend/Emit table + region-edit error rules + original-node
  idiom + cross-tree splice, recompose instruction model + Concat scope +
  wrapping + streaming + the reconstruction doctrine (all compressed from
  the respective module docs; vocabulary kept verbatim — "original node",
  never provenance, per the transform module's own rule).
- M3: docs/ai-guide-custom-lang.md, 18,254 bytes, 1 doctest (TrivialLang).
  Lang associated-types table (10 rows, one-liners traced to the trait's
  per-item rustdoc; make_node_ext stated as the one required method),
  TrivialLang vs latexlike family (role traits, LatexlikeLang opt-in,
  pillar functions, projection pattern), token rules + the specials
  double-hook silent trap (both obligations), ext family table with
  population-is-initialization, finalize_transition obligations + the
  ruled replay-granularity note (compressed from custom-lang.md's
  doc-traced passage), driver five-concern summary + CommandResolver,
  construct parsers (trait shape text-fence as in the human chapter,
  ParseContext toolkit table, takeover essentials + two staging doors,
  ArgumentParser contract, conditions + ImplementationError), pointer to
  the complete `\until` takeover doctest in construct-parsers.md.
- M4: docs/ai-guide-embedding.md, 8,249 bytes, 1 doctest (parse_source
  with a held Arc<Source> + LineIndexCache line/col + span correlation
  across two parses of the same source — compiled evidence for the
  span-stability rule). Bindings/threading facts table (owned handle +
  TreeTag, Send+Sync trees, no-Send visitors, Severity exhaustive,
  synthesized-node recipe pointer, never-panics-on-input), multi-source +
  include policy, tooling entry points (node_at/covering_slice/parent,
  LineIndexCache/LineColProvider + scan cap, span stability), no_std/
  WebAssembly (compressed from introduction.md § Where techy runs),
  streaming recomposition.
- M5: docs/ai-guide-pylatexenc.md, 13,344 bytes, 1 doctest (the
  quick-start translation: tolerant Language, math-as-group,
  diagnostics-as-data — same evidence shape as the human chapter's).
  Four tables: core concept map (15 rows), node classes (8 rows — the
  three classes the human chapter covers in prose (`LatexCharsNode`,
  `LatexGroupNode`, `LatexCommentNode`) added as rows linked to the
  verified latexnodes.nodes module-page URL, no new anchor URLs
  invented), argument-spec strings (v2 aliases `*`/`{`/`[` explicit),
  behavior differences (6 rows: entry model, byte offsets, span
  identity, tolerant→Recovery+Diagnostics, unknown macros, `\input`
  layer move). All 24 readthedocs URLs reused verbatim from
  docs/pylatexenc-migration.md's G3-verified link block (/en/latest/;
  zero new URLs).
- M6 (root, LAST): docs/ai-guide.md, 16,420 bytes (HARD cap ~30 kB), 4
  doctests (parse; register macro+environment / read arguments / body /
  spans / descendants; extract; diagnostics incl. `is::<T>()`,
  `downcast_ref`, `T::IDENTIFIER`, typed payload field). Orientation
  (4 sentences), module-topology table (11 rows, canonical paths),
  everyday-flow text diagram, four recipes, 18-line pitfalls index (each
  line pointing to the sub-chapter/API item with the detail), pointer
  table to the five sub-chapters + all 13 human-guide pages. Written
  against the finished sub-chapters (anchors `#traps`,
  `#argument-codes`, `#input-like-inclusion` verified in built HTML).
  One doctest correction during writing: the descendants assertion
  initially omitted the environment's optional-argument content `"(i)"`
  — the failing doctest supplied the real shape (also confirming that
  environment name scaffolding does NOT leak into chars descendants).

## Gates (run after M6)

| Gate | Result |
|---|---|
| `cargo build` | PASS (clean) |
| `cargo test` (all suites) | PASS — 758 lib + 30 acceptance + 8 derive_conditions + 21 recompose_oracle + 1 techy-derive |
| `cargo test --doc` | PASS — 66 doctests (2 ignored), incl. this stage's 14 new guide doctests (definitions 4, trees 3, custom-lang 1, embedding 1, pylatexenc 1, root 4) |
| `rm -rf target/doc && cargo docs` | PASS — zero warnings (broken-intra-doc-links deny in force; all new intra-doc links resolve, `NodeRef::specials_name` included) |
| Fragment anchors | All in-page anchors used by the new chapters verified as generated ids in the built HTML: `argument-codes`, `traps`, `input-like-inclusion` (ai_guide_definitions), `a-complete-takeover-parser` (construct_parsers), `resolving-external-sources-input-like-inclusion` (specs), `working-with-diagnostics` (parsing), `scopes-and-packages` (concepts_overview) |
| `scripts/check_semver.sh` | PASS — "no semver update required" (196 checks pass) |
| Four-step wiring | Intact for all six chapters (files; lib.rs guide block lines 152–167; GUIDE_PAGES rows 82–87; guide.md AI Guide index — wired in G1, only file contents changed; published stub headings kept verbatim) |
| Superseded-names sweep | CLEAN — two grep batteries over the [§dd-dr:superseded-names] register; only hits: the canonical `ParsingStateDelta` and pylatexenc-side class references in the migration chapter (`LatexWalker`, `Latex*Node`, `LatexContextDb`) — same classification as the G3 sweep |
| Markdown-only discipline | No code or rustdoc files touched; the only non-docs/ change is this report |

## Chapter size table (root HARD cap ~30 kB; sub-chapters ≤ ~60 kB)

| File | Bytes | Tokens ≈ bytes/4 | Status |
|---|---|---|---|
| docs/ai-guide.md (root) | 16,420 | ~4,100 | OK — 55% of the HARD cap |
| docs/ai-guide-definitions.md | 13,384 | ~3,300 | OK |
| docs/ai-guide-trees.md | 14,518 | ~3,600 | OK |
| docs/ai-guide-custom-lang.md | 18,254 | ~4,600 | OK |
| docs/ai-guide-embedding.md | 8,249 | ~2,100 | OK |
| docs/ai-guide-pylatexenc.md | 13,344 | ~3,300 | OK |
| Total | 84,169 | ~21,000 | root + largest sub-chapter ≈ 8,700 tokens |

## DOC_GAPS delta

- No new entries. Every chapter claim is compressed from a finished human
  guide chapter (public documentation per the ruled method), traced to
  module/item rustdoc, or demonstrated by one of the 14 compiling
  doctests. The embedding chapter's "never panics on document input" row
  restates learn-by-example's public claim; the crate-level rustdoc
  anchor for it is already tracked as DOC_GAPS #3 (OPEN, G5 scope) — no
  duplicate entry filed.
- #2 (WebAssembly rustdoc mention) and #3: untouched (G5 scope). The
  embedding chapter's WebAssembly sentence compresses introduction.md's
  (which #2's build half already verified).

## Deviations / items for the supervisor

1. Stale worktree HEAD at agent start (2110bbb) — reset to api-review tip
   855adb7 before branching, per the brief. Procedural.
2. The pylatexenc node-class table adds three rows the human chapter
   handles as "the other mappings are direct" (`LatexCharsNode`,
   `LatexGroupNode`, `LatexCommentNode`). They are linked to the
   G3-verified latexnodes.nodes module page URL, not to per-class
   anchors, honoring "reuse the verified URLs — do not invent new ones".
3. The embedding chapter includes the synthesized-node recipe pointer
   (one table row) although the ruled ai-guide-embedding content list
   does not name it: it is one of the ruled integration.md items being
   compressed, and it is embedder-audience material. REMOVE-rule
   candidate if the reviewer reads the chapter map as exhaustive.
4. The custom-lang chapter's `ConstructParser` trait shape is a
   `text`-fenced signature paraphrase — same presentation as the human
   construct-parsers.md chapter (not compiled; the real declaration is
   the trace).
5. The root's pitfalls index has 18 entries (the ruled list plus
   sub-chapter-surfaced one-liners: `descendants()` self-exclusion,
   `body()` None-vs-empty, Emit-no-descent, reconstruction doctrine,
   Concat role scope, no-Send visitors, LineIndexCache cap). Each is one
   line + pointer, per the ruled format.
6. Vocabulary note: the trees chapter says "the sanctioned splice door"
   and "staging door(s)" — both are the shipped module docs' own wording
   (transform/mod.rs; constructs docs and the human chapters use
   "staging door" throughout), kept deliberately for term stability with
   the API docs an agent will read next.

## Scrutiny pointers for the reviewer

- ai-guide.md: the everyday-flow text diagram is synthesized (no single
  doc source states it as a diagram) — verify each edge: Package →
  lang_initial_with_packages (specs.md), driver+state → Language::new
  (parsing.md), parse → Result<ParseResult, ParseError> (parsing.md),
  ParseResult fields (parsing.md), root()/children()/descendants()
  (node-trees.md). The pitfalls index lines are each one-line
  compressions — the pointer target carries the authority.
- ai-guide-definitions.md: the argument-code table is the densest
  compression (11 rows from the argument_specs rustdoc table) — check
  row-by-row against the rustdoc; the `o` row drops the rustdoc's
  parenthetical about lone-inner-group protection (kept only on the
  `AnyDelimitedOptional` row's source; REMOVE-rule cut).
- ai-guide-trees.md: the restage and recompose fact lists compress the
  two module docs' section headers nearly 1:1 — the highest-risk
  compression is the region-edit paragraph (provided-with-empty vs
  absent; ContentParentDropped remedy) — trace to transform/mod.rs
  "Region edits: no silent repair".
- ai-guide-custom-lang.md: the Lang associated-types table one-liners
  are compressions of the trait's per-item docs (state/lang.rs) — the
  StateExt "no interior mutability" and Event two-class rows carry
  contract weight; the replay-granularity paragraph re-compresses the
  G3-traced custom-lang.md passage (trace chain in G3_REPORT M3 entry).
- ai-guide-embedding.md: the doctest's final assertion (spans of two
  parses of the same Arc<Source> compare equal) is the strongest new
  demonstration — it is the positive side of the documented
  identity-based-equality rule; the negative side ("never correlate"
  across parse() calls) stays prose, traced to integration.md.
- ai-guide-pylatexenc.md: pylatexenc-side facts are unchanged from the
  G3-verified human chapter (this stage did not re-read pylatexenc
  sources); the compression risk is table-cell truncation — the
  tolerant-parsing row carries the full strict/tolerant contract in one
  cell.
