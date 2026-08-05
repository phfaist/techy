# Phase 4 — G3 report: Developer Guide

Branch `phase4-g3-dev-guide` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a7881e287c04a0cd7`,
branched from `api-review` @ 52932b2). Status: **IN PROGRESS** — Milestone 0
(this plan).

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

## Gates

(to be filled at closure)

## Chapter size table

(to be filled at closure)

## DOC_GAPS delta

(to be filled at closure)

## Deviations / items for the supervisor

1. Stale worktree HEAD at agent start (2110bbb) — reset to api-review tip
   52932b2 before branching, per the brief. Procedural.

## Handoff notes for the M5 successor

(to be filled at closure)
