# Phase 4 — G4 report: AI Guide

Branch `phase4-g4-ai-guide` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a283fa102a3175a1f`,
branched from `api-review` @ 855adb7). Status: **M0 (plan)**.

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
