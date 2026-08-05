# Phase 4 — G3 report: Developer Guide

Branch `phase4-g3-dev-guide` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a7881e287c04a0cd7`,
branched from `api-review` @ 52932b2). Status: **M0–M4 COMPLETE** — all
gates green; M5 (pylatexenc-migration.md) awaits the SUCCESSOR agent, then
review + merge.

Note on branch point: the worktree's HEAD at agent start was a stale commit
(2110bbb, predating the api-review series); the branch was reset to the local
`api-review` tip 52932b2 (G2 merged) before any work, per the brief.

Stage split (per brief): THIS agent covers M0–M4 (parsing-model,
construct-parsers, custom-lang, integration). A SUCCESSOR agent writes M5
(docs/pylatexenc-migration.md) in this same worktree afterwards — the M5
section below records the plan for that successor; this agent does not touch
pylatexenc-migration.md.

## Plan (Milestone 0 — resume from here if interrupted)

Governing inputs read: PHASE4_PLAN.md (Method rules, Chapter map Developer
Guide rows, G3 stage scope, Protocol, Gates — binding),
Documentation_Structure.md (wiring, cross-referencing, concepts-overview
anchor scheme, doctest conventions), the landed G1/G2 pages (guide.md,
introduction.md, concepts-overview.md, language-syntax.md, node-trees.md,
specs.md, parsing.md — voice and context; NOT to be modified), DOC_GAPS.md
(entry format; append-only, never renumber).

Standing method discipline for every milestone: chapters written from public
documentation only (module `//!` docs, item `///` docs, signatures, rendered
docs) or demonstrated by compiling doctests in the chapter itself; no claims
from implementation bodies; uncertainties become DOC_GAPS entries. No
duplication of API rustdoc — synthesize and point; extensive API-use
information belongs in module docs, chapters give the picture and point
there. REMOVE-not-summarize on length overrun (soft cap ~30 kB per chapter;
integration.md deliberately short). Writing rules: no metaphors, no
review-coined jargon, define every technical term before use (link or
repeat), likeliest-reader-first ordering, link major concepts to
concepts-overview (stable heading anchors), never reference dev-docs/ or the
review process in user-facing pages. Audience: extends techy (writes
construct parsers, defines languages, embeds the library); has read the User
Guide (link when leaning on it) but NOT the code base. Superseded names must
not appear ([§dd-dr:superseded-names] grep before final commit).
Documentation-only stage: markdown-only for this agent — a rustdoc gap is a
DOC_GAPS entry, not an edit; `scripts/check_semver.sh` stays green. Wiring
for all chapters landed in G1 (stubs) — only file content changes; verify
four-step wiring at gate time. Do not modify: PLAN.md, PHASE4_PLAN.md, any
G1/G2 chapter, ai-guide stubs, pylatexenc-migration.md, any code.

### Milestone 1 — docs/parsing-model.md (commit `P4-G3: parsing-model chapter`)

Soft cap ~30 kB; aim lower. The map of how parsing is executed and delegated
— the reader comes away knowing:

- What happens on `Language::parse`: the session, then the content-dispatch
  loop (NodesParser) selecting a construct parser by token kind and
  definition lookup.
- How a resolved definition (spec) supplies the parser for its invocation —
  the spec traits' role (CallableSpec's parser factory).
- How parsing state flows: immutable states, reified deltas, the single
  derivation point (`ParsingState::derived` + `finalize_transition`), and
  where a construct's after-effect delta goes (returned to the caller, who
  applies it).
- How problems flow: the diagnostics funnel, recovery at the detection site,
  strict vs tolerant, implementation errors as a separate path.
- Where the extension seams are: drivers, specs, construct parsers, the
  language itself — each with a pointer to the chapter/API docs that cover
  it.

This chapter is the map, not the reference: point to `core` /
`core::constructs` module docs for contracts. Sources: engine rustdoc
(Language, ParserSession, ParseDriver, ParseResult, Frame), core::constructs
facade + NodesParser/ConstructParser/ParseContext docs, ParsingState/
ParsingStateDelta/Lang docs, error module docs, concepts-overview anchors.

### Milestone 2 — docs/construct-parsers.md (commit `P4-G3: construct-parsers chapter`)

Soft cap ~30 kB. How to write a custom construct parser, from the public
contracts only:

- The ConstructParser trait's shape; what a parser receives — the
  ParseContext (token reading, node staging, state derivation,
  diagnostics/implementation-error channels).
- What a parser returns: output plus the optional after-effect delta for the
  caller.
- The Invocation route: how a spec takes over parsing of its invocation; the
  staging doors (`stage_node`, `stage_invocation`) and when each applies.
- Argument parsing: the ArgumentParser contract, how argument specs carry
  parsers, ParsedArgumentNodes.
- Raising conditions: custom condition types via the DiagnosticInfo derive,
  recovery-at-detection expectations, the implementation-error path for
  contract violations.

At least one complete compile-checked doctest of a working custom parser (a
small takeover parser ideal). Every claim traced to rustdoc or demonstrated
in the doctest. Sources: core::constructs rustdoc (ConstructParser,
ParseContext, ArgumentParser, ParsedArgumentNodes, condition types),
core::specs rustdoc (CallableSpec parser factory, ArgumentSpec),
error/derive rustdoc (DiagnosticInfo derive), latexlike spec docs for the
takeover route framing.

### Milestone 3 — docs/custom-lang.md (commit `P4-G3: custom-lang chapter`)

Soft cap ~30 kB. Specifying a custom language — knobs GROUPED BY FEATURE,
pointing to `Lang`'s API docs (ruled: do not duplicate Lang's API doc):

- Vocabularies: callable types, group types; what the latexlike role traits
  add for languages joining the preset family.
- Modes.
- Token rules and the specials-recognition hooks — including the documented
  silent-trap pointer: specials need both hooks wired.
- Extension types attaching custom information to nodes/arguments/slots (the
  ext family, `make_node_ext`).
- Language state and `finalize_transition` — with the ruled short
  replay-granularity note: when a construct forwards a merged after-effect
  delta (as the shipped `\input` state-persistence does, per its docs),
  `finalize_transition` sees the merged delta once, not each original
  operation; order-sensitive customizers must account for that. Trace to
  shipped docs; if the public docs do not state it → DOC_GAPS entry and
  write only what they support.
- Drivers: the ParseDriver hooks; command resolution (`CommandResolver`,
  `ScopesCommandResolver`).
- The two on-ramps: `TrivialLang` for machinery experiments; extending the
  preset — what `LatexlikeLang` requires vs what `Latexlike` already
  implements, the pillar functions as the reuse route.
- End with a pointer to the FLM-style projection idea: a custom language
  reusing preset driver/spec types.

Sources: Lang trait rustdoc (the knob inventory), TokenRules/specials-hook
docs, ext-family docs, ParseDriver + command-resolution docs, TrivialLang
docs, latexlike LatexlikeLang/pillar-function docs.

### Milestone 4 — docs/integration.md (commit `P4-G3: integration chapter`)

SHORT by ruling — a pointer chapter, well under the cap: key non-obvious
embedder/bindings findings AND tooling starting points, each a few sentences
plus API links; depth stays in rustdoc. Cover exactly:

- The owned-handle pattern for bindings (`Arc<NodeTree>` + `NodeId`,
  re-resolve via the tree).
- Visitors and `annotate` are not `Send` — what that means for embedders.
- The synthesized-node recipe pointer (pillar functions +
  `ParsingStateStack::from_node_ancestors`).
- `Severity` match-exhaustiveness.
- `LineIndexCache` as the bindings-side line/col handle.
- Streaming recomposition (recomposer-held writer, `Piece = ()`).
- Navigation starting points (`node_at`, `covering_slice`, `parent`).
- Line/col ownership (`LineIndexCache`, `LineColProvider`, the scan-length
  bound).
- The re-parse/span-stability rule: correlate positions across parses by
  holding your own `Arc<Source>` and calling `parse_source`, never `parse`.

Every claim doc-traced; anything the docs don't state → DOC_GAPS entry, not
prose. Sources: node/tree rustdoc (NodeId, node_at, covering_slice), visit/
transform annotate docs, node-synthesis pillar docs +
ParsingStateStack::from_node_ancestors, error::Severity docs,
source::LineIndexCache/LineColProvider docs, recompose module docs
(streaming), Language::parse/parse_source docs.

### After M4 — gates + closure (commit `P4-G3: report closure — gates`)

Run all gates: `cargo build`; `cargo test` (all suites); `cargo test --doc`;
`rm -rf target/doc && cargo docs` (zero warnings); `scripts/check_semver.sh`;
byte-size table per chapter vs caps; superseded-names sweep over touched
files; four-step wiring verification. Update this report: milestone log,
gates table, size table, DOC_GAPS delta, deviations, per-chapter scrutiny
notes for the reviewer, handoff notes for the M5 successor. Commit and STOP
— M5 belongs to the successor.

### Milestone 5 — docs/pylatexenc-migration.md (SUCCESSOR AGENT — not this one)

Short, NOT exhaustive: main core concepts only + non-obvious mappings between
pylatexenc (v2 and v3, one page) and techy; every pylatexenc concept linked
to its https://pylatexenc.readthedocs.io/ page (verify links resolve). The
migration agent is the only one reading the pylatexenc Python sources
(`$HOME/Research/util/pylatexenc/`); no exploring outside the techy and
pylatexenc root folders. All method rules above apply unchanged; commit
`P4-G3: pylatexenc-migration chapter`; then re-run gates and append closure
notes to this report.

Interruption rule: if context balloons (~400k+ tokens), finish the current
milestone, commit handoff notes here, and stop for a successor.

## Milestone log

- M0: this plan. (Committed before other work.)
- M1: docs/parsing-model.md written, 15,911 bytes (soft cap ~30 kB); 0
  doctests (map chapter — every claim doc-traced instead). Sources: engine
  rustdoc (engine/mod.rs module + ParserSession/ParseResult docs,
  language.rs Language/parse/parse_source docs, driver.rs ParseDriver +
  CommandResolver/ScopesCommandResolver/resolve_command_in_scopes docs),
  constructs/mod.rs (ParseContext, ConstructParser, module-level state/error
  contracts), nodes_parser.rs (module docs, NodesOutcome/StopCause),
  spec/callable.rs (CallableSpec factory), state (Lang, ParsingState::derived,
  delta docs), concepts-overview anchors.
- M2: docs/construct-parsers.md written, 20,638 bytes (soft cap ~30 kB); 1
  compile-checked doctest — a complete `\until …;` takeover parser
  (custom CallableSpec + make_invocation_parser override, raw reading via
  verbatim_state_delta + probe_token, a custom DiagnosticInfo-derived
  condition with detection-site tolerant recovery, staging via stage_node +
  stage_invocation with a body-marked Content slot, tolerant + missing-
  terminator assertions). Sources: constructs/mod.rs (ParseContext methods,
  two-tier model, Invocation), invocation_parser.rs (StdInvocationParser
  contract, parse_declared_arguments), spec/structure.rs (ArgumentParser,
  ParsedArgumentNodes, ArgumentSpec), spec/callable.rs (requires_content,
  factory), verbatim_parser.rs (verbatim_state_delta), techy-derive lib.rs
  (derive-generated items), error.rs (DiagnosticInfo).
  One mid-write correction caught by the doctest itself: a callable's
  children must be tiled by argument/slot regions (NodeBuildError
  "regions must tile the child list"), so the example mints a ParsedSlot —
  which also let it demonstrate BodySlotExt/`body()`.
- M3: docs/custom-lang.md written, 16,408 bytes (soft cap ~30 kB); 1
  doctest (TrivialLang on-ramp). Knobs grouped by feature, each pointing to
  Lang's API docs per the ruling (no duplication of the Lang page).
  Includes: the specials both-hooks silent trap (traced to
  scan_specials/specials_trigger_chars implementer obligations), the ruled
  finalize_transition replay-granularity note — traced to public docs
  end-to-end: NodesOutcome::after_effects ("merged into one delta …
  later field overrides win, scope ops and context-free events concatenate"),
  InputMacroSpec's persist_state section ("merged into one record … returned
  as the \input invocation's own after-effect"), NodesParser module docs
  (after-effect applied session-mediated = one derivation), and
  Lang::finalize_transition ("run exactly once per derived() call") — so no
  DOC_GAPS entry was needed; drivers + the CommandResolver strategy; the two
  on-ramps (TrivialLang; LatexlikeLang role traits + what Latexlike
  implements, pillar functions as the reuse route); closing
  projection-pattern pointer (phrased without the FLM acronym per the
  no-acronyms rule; the generic-over-LLL signatures of
  LatexlikeDriver/MacroSpec/EnvironmentSpec/SpecialsSpec are the trace).
- M4: docs/integration.md written, 6,382 bytes (SHORT by ruling — pointer
  chapter); 0 doctests. All nine ruled items covered, each a few sentences
  + API links: owned-handle (NodeRef borrow contract + NodeId 8-byte Copy +
  node()/get() + TreeTag "misuse detector, never addressing"), visitors/
  annotate no-Send (visit.rs NodeVisitor + transform RestageVisitor "no
  Send/Sync bounds … would wall off single-threaded FFI callbacks"),
  Severity exhaustive (no non_exhaustive attribute; derived Ord ranking
  documented), synthesized-node recipe (latexlike driver.rs module docs +
  ParsingStateStack::from_node_ancestors), streaming recomposition
  (recompose module "No sink" section), navigation (node_at/covering_slice
  method docs + NodeRef::parent), line/col ownership (LineIndexCache +
  LineColProvider + the 500 000-byte scan cap), re-parse/span-stability
  (parse = anonymous Source per call; SourceSpan/SourcePos identity-based
  equality; LineIndexCache docs naming "the span-stability doctrine").
  The two ruled line/col bullets (bindings-side handle; tooling ownership)
  are covered in one subsection to avoid duplication.

## Gates (run after M4)

| Gate | Result |
|---|---|
| `cargo build` | PASS (clean) |
| `cargo test` (all suites) | PASS — 758 lib + 30 acceptance + 8 derive_conditions + 21 recompose_oracle + 1 techy-derive |
| `cargo test --doc` | PASS — 51 doctests (2 ignored), incl. this stage's 2 new guide doctests (construct-parsers 1, custom-lang 1) |
| `rm -rf target/doc && cargo docs` | PASS — zero warnings (broken-intra-doc-links deny in force) |
| `scripts/check_semver.sh` | PASS — "no semver update required" (196 checks pass) |
| Four-step wiring | Intact for all four chapters (files; lib.rs guide block; GUIDE_PAGES; guide.md index — wired in G1, only file contents changed) |
| Superseded-names sweep | CLEAN over the four chapters (three grep batches over the [§dd-dr:superseded-names] register; the only regex hits were the canonical names `ParsingStateDelta` and `lang_initial()`) |
| Markdown-only discipline | No code or rustdoc files touched; the only non-docs/ change is this report |

## Chapter size table

| File | Bytes | Target | Status |
|---|---|---|---|
| docs/parsing-model.md | 15,911 | soft cap ~30 kB, aim lower | OK |
| docs/construct-parsers.md | 20,638 | soft cap ~30 kB | OK |
| docs/custom-lang.md | 16,408 | soft cap ~30 kB | OK |
| docs/integration.md | 6,382 | SHORT by ruling | OK |

## DOC_GAPS delta

- No new entries. Every chapter claim traced to a public documentation
  sentence or demonstrated by a compiling doctest; the two claims the plan
  flagged as potential gaps resolved to documented facts: (a) the
  replay-granularity note (M3 trace chain above), (b) the span-stability
  rule (SourceSpan/SourcePos identity-based equality + Language::parse
  "anonymous in-memory Source" + LineIndexCache's named doctrine).
- #2 (WebAssembly rustdoc mention) and #3 (crate-level panic-contract
  sentence): untouched (G5 scope).

## Deviations / items for the supervisor

1. Stale worktree HEAD at agent start (2110bbb) — reset to api-review tip
   52932b2 before branching, per the brief. Procedural.
2. M4's two ruled line/col bullets (the T5 I-9 "bindings-side handle" and
   the T4 "line/col ownership" items) are one subsection, not two — same
   types, two audiences; the subsection addresses both. Flagged in case the
   reviewer reads "cover exactly" as demanding separate renderings.
3. The M3 projection-pattern pointer avoids the "FLM" acronym
   (Documentation_Structure no-acronyms rule) and describes the pattern
   generically ("a semantic markup language that projects parsed documents
   into its own content model"). The rustdoc itself does use "FLM" in a few
   places; that inconsistency is pre-existing and not user-facing-guide
   material — left alone.
4. The construct-parsers doctest's condition uses the namespace
   `mydefs.until.missing-terminator` — following the DiagnosticInfo
   IDENTIFIER contract ("presets and downstream languages use their own
   namespace") and the specs.md `mydefs` package-name convention.
5. Not done (out of scope): no G1/G2 chapter touched; no ai-guide stub
   touched; pylatexenc-migration.md untouched (successor's); no code or
   rustdoc changes (markdown-only stage held).

## Per-chapter scrutiny notes for the reviewer

- parsing-model.md: the claims densest in doc-tracing are the root-loop
  description (Language::parse_source doc comment), the after-effect
  paragraph (constructs/mod.rs module docs + NodesParser module docs), and
  the implementation-error path (ParseContext::implementation_error +
  ImplementationError docs). The chapter deliberately has no doctest.
- construct-parsers.md: the doctest is the load-bearing artifact — it
  compiles and runs against Latexlike and asserts spans, child shape,
  body() access, and the custom condition's identity. The `text`-fenced
  trait shape at the top is a signature paraphrase, not compiled; verify it
  against the real ConstructParser declaration.
- custom-lang.md: the replay-granularity paragraph is the highest-scrutiny
  passage (rulings-sensitive); its four-link trace chain is in the M3 log
  entry above. The specials-trap paragraph compresses the two implementer
  obligations from scan_specials + specials_trigger_chars.
- integration.md: each bold-lead paragraph corresponds 1:1 to a ruled item;
  the span-stability paragraph makes the strongest synthesized claim
  ("positions from two parse calls never correlate even on identical
  content") — composed from documented identity-based equality + the
  documented anonymous-Source behavior of parse().

## Handoff notes for the M5 successor

- Worktree: `/Users/philippe/projects/techy/.claude/worktrees/agent-a7881e287c04a0cd7`,
  branch `phase4-g3-dev-guide` (do NOT create a new branch; continue on it).
  HEAD at handoff = the M4 closure commit.
- Your milestone: docs/pylatexenc-migration.md ONLY (see the M5 section
  above; chapter-map row: short, NOT exhaustive, v2 and v3 on one page,
  every pylatexenc concept linked to its resolving
  https://pylatexenc.readthedocs.io/ page). You are the only agent allowed
  to read the pylatexenc Python sources (`$HOME/Research/util/pylatexenc/`);
  do not explore outside the techy and pylatexenc roots.
- The stub heading is `# Migrating from pylatexenc` — keep it (published
  heading; guide.md and integration.md link the chapter; integration.md's
  closing "Read next" points at you).
- All method rules apply unchanged (public-docs-only on the techy side;
  REMOVE-not-summarize; writing rules; superseded-names sweep; DOC_GAPS
  append-only). Commit as `P4-G3: pylatexenc-migration chapter`; then
  re-run the full gate set (build, tests, doctests, clean cargo docs,
  check_semver, sizes) and append your milestone log, gate results, and
  size row to this report before the final commit.
- Terminology anchors you can rely on from the finished chapters: "spec",
  "construct parser", "staging", "after-effect delta", "callable" are all
  defined in parsing-model.md/construct-parsers.md — link rather than
  redefine. techy-side API spellings you need are all post-Phase-3
  canonical paths (verify with `cargo docs`, never from memory).
- External-link verification (readthedocs) is NOT covered by cargo docs —
  verify resolvability yourself and record how in the report.
