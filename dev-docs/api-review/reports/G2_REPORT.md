# Phase 4 — G2 report: User Guide

Branch `phase4-g2-user-guide` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a833debc29d055132`,
branched from `api-review` @ f8b2987). Status: **COMPLETE** — Milestones 0–6
done, all gates green; awaiting review + merge.

Note on branch point: the worktree's HEAD at agent start was a stale commit
(2110bbb, predating the api-review series); the branch was reset to the local
`api-review` tip f8b2987 (G1 merged) before any work, per the brief.

## Plan (Milestone 0 — resume from here if interrupted)

Governing inputs read: PHASE4_PLAN.md (Method rules, Chapter map rows for
language-syntax / node-trees / specs / parsing / learn-by-example, G2 stage
scope, Protocol, Gates — binding), Documentation_Structure.md (wiring,
cross-referencing, concepts-overview linking scheme, doctest conventions),
the G1-landed pages (docs/guide.md, docs/introduction.md,
docs/concepts-overview.md — voice and context; NOT to be modified),
DOC_GAPS.md (entry format; #1 to resolve in M4), the current
docs/learn-by-example.md (to be revised in M5, not rewritten).

Standing method discipline for every milestone: chapters written from public
documentation only (module `//!` docs, item `///` docs, signatures, rendered
docs) or demonstrated by compiling doctests in the chapter itself; no claims
from implementation bodies; uncertainties become DOC_GAPS entries (append,
never renumber). No duplication of API rustdoc — synthesize and point.
REMOVE-not-summarize on length overrun. Writing rules: no metaphors, no
review-coined jargon, define terms before use, link major concepts to
concepts-overview (stable heading anchors), never reference dev-docs/ or the
review from user-facing pages. Superseded names must not appear
([§dd-dr:superseded-names] grep before final commit). Documentation-only:
no code changes beyond rustdoc comments; `scripts/check_semver.sh` stays
green. Wiring for all five chapters already landed in G1 (stubs) — only file
content changes are needed; verify the four wiring steps still hold for each
touched chapter at gate time.

### Milestone 1 — docs/language-syntax.md (commit `P4-G2: language-syntax chapter`)

Target ≤ ~10–15 kB. Content per chapter-map row: what a "latexlike" language
is — macros, environments, specials, comments, groups — each with a tiny
source snippet (`text`-fenced source fragments for pure syntax display; small
`rust` doctests where a parse demonstrates the claim); definitions can change
during the parse. State plainly: no definitions ship by default beyond
`\begin`/`\end` (builtin package); opt-in minidefs/minilatex for quick starts
and debugging; defining callables is specs.md's chapter. Close with ~2
paragraphs: latexlike is a preset over the engine's more fundamental concept
set — macros/environments/specials are special cases of "callables" defined by
the `latexlike` preset; a different preset could define other, orthogonal
kinds of callables. Sources: latexlike module rustdoc, concepts-overview
(latexlike preset / callable specs sections), token/state item docs.

### Milestone 2 — docs/node-trees.md + module-doc sufficiency pass (commit `P4-G2: node-trees chapter + module-doc pass`)

Target ≤ ~10–15 kB. Content: what a parse produces — the node tree; the
closed set of node kinds (characters, group, callable invocation, comment,
list) and how they relate; reading via NodeRef/NodeSlice; then a high-level
tour of the four consumer modules — extract, visit, transform, recompose —
one short paragraph each POINTING to the module documentation (ruled: API-use
depth lives in module docs, not guide chapters).

THE PASS: for each of techy::{extract, visit, transform, recompose}, read the
module-level rustdoc and judge: does a reader arriving from the chapter
pointer find a usable API-use narrative (entry point, core types, at least
one worked example)? Where short, expand the MODULE `//!` rustdoc (doc-only),
sourcing every claim from item documentation or compiling doctests — never
implementation bodies; uncertainties → DOC_GAPS. Record per-module verdicts
(sufficient as-is / expanded how) in this report. May split into two commits
(chapter; rustdoc expansions) if that stays cleaner.

### Milestone 3 — docs/specs.md (commit `P4-G2: specs chapter`)

Target ≤ ~20 kB. Content: defining callables (macros, environments, specials)
for latexlike languages — Package registration incl. the convenience
one-liners (define_macro / define_environment family), the spec types
(MacroSpec, EnvironmentSpec, SpecialsSpec), argument codes (incl. the
no-fallback braced-group option and the enum spelling of codes), named
arguments, scoped/body-scoped definitions (minidefs' scoped `\item` as the
shipped exemplar), and the silent-trap callouts the API docs record (e.g.
registering names with the escape character included; the single-expression
argument fallback). Include the ruled standard-filesystem SourceResolver
recipe as a compile-checked example (doctest marked `no_run`), plus one
sentence on InputMacroSpec pointing to its API docs. One brief closing
paragraph pointing general (non-latexlike) languages to the Developer Guide +
core::specs API entry points. Sources: core::specs and latexlike rustdoc
(Package, spec types, argument-code docs), source::SourceResolver docs.

### Milestone 4 — docs/parsing.md + DOC_GAPS #1 (commit `P4-G2: parsing chapter + DOC_GAPS #1`)

Target ≤ ~10–15 kB. Content: running the parser (Language::new +
parse/parse_source); strict vs tolerant recovery and what tolerant output
means; the direct settings/knobs; the initial parsing state (lang_initial,
packages); working with diagnostics — rendering, sorting, and the ruled
matching rule ("match conditions via `T::IDENTIFIER` / `is::<T>()`, never
literal identifier strings") with a link to the auto-generated DiagnosticInfo
implementors listing; NO duplicated identifier table.

RESOLVE DOC_GAPS #1 here: check rendered condition-type pages for visible
identifier strings; if a small mechanical doc line is missing on some pages,
add it (doc-only) and mark RESOLVED; if structural work is needed (derive
changes), leave OPEN with precise findings and write the chapter accordingly.

### Milestone 5 — docs/learn-by-example.md revision (commit `P4-G2: learn-by-example revision`)

Target ≤ ~30 kB. Re-curate the existing tour to illustrate ~60% of techy's
capabilities. Curation inputs (selection signals ONLY — API spellings therein
are stale, predating Phase 3; never copy a name/path from them):
SYNTHESIS.md §4–§5, walkthroughs/*/FRICTION.md. Fold in where natural: the
generic `NodeRef::name()` accessor beside per-type getters; `body()`'s None
semantics; `descendants()` self-inclusion; the argument-codes enum
alternative; a body-scoped-definitions example. All examples compile-checked
(`rust` fences as doctests). REMOVE rule to stay ≤ ~30 kB. Also update the
page's stale/process-flavored framing (e.g. "Phase 7.9 acceptance suite"
wording) per the user-facing writing rules.

### Milestone 6 — two API-doc-only notes + closure (commits `P4-G2: API-doc notes (post_space; input caching)` and `P4-G2: report closure — gates`)

(a) On the documented post-command-whitespace record (latexlike
invocation-syntax macro data, e.g. the post_space accessor/record docs): if
not already stated, add a brief note that source recomposition re-emits the
recorded whitespace verbatim, and that any smarter spacing policy belongs to
a converter built on techy, not to techy. (b) On `input_macro_spec`'s
rustdoc: if not already stated, add a 2–3 sentence caution that included
content is read through the resolver at parse time on every inclusion, and
that caching resolved content is not safe in general because an inclusion can
change the caller's parsing state (grounded in the documented persist-state
behavior). Either note already covered → record "already covered" + location
here instead of duplicating.

Then the gates: `cargo build`; `cargo test` (all suites); `cargo test --doc`;
`rm -rf target/doc && cargo docs` (zero warnings); `scripts/check_semver.sh`;
byte-size table per touched chapter vs target; superseded-names sweep over
touched files. Finish this report: deviation list, per-module pass verdicts,
DOC_GAPS delta, file/size table. Final commit.

Interruption rule: if context balloons (~400k+ tokens), finish the current
milestone, commit handoff notes here, and stop for a successor.

## Module-doc sufficiency pass (M2 verdicts)

Criterion (per stage brief): a reader arriving from node-trees.md's pointer
must find a usable API-use narrative — entry point, core types, at least one
worked example — in the module-level rustdoc.

| Module | Verdict | Basis |
|---|---|---|
| techy::extract | SUFFICIENT as-is | Module doc names both input shapes (readers/builders), the builder route (new-tree minting, edge behaviors), the producer annotation triple (`bare`/`_drop_annotations`/`_keep_annotations`), and carries a compiling doctest (split + compose with content_as_chars). |
| techy::visit | SUFFICIENT as-is | Module doc gives walk + NodeVisitor + VisitFlow + VisitContext, document-order contract, the three-channel state discipline, role-blindness, and a compiling doctest (chars + depth collection). |
| techy::transform | SUFFICIENT as-is | Module doc gives restage + RestageVisitor + Restage::{Descend,Emit} contract, top-down/bottom-up mediation, read-frozen/write-staged, annotation pathway + origin-id convention, region-edit error semantics, cross-tree contract, and a compiling doctest (annotate-with-original restage). |
| techy::recompose | SUFFICIENT as-is | Module doc gives recompose + Recomposer + Recompose/ConcatPieces + ComposePiece, state threading, streaming pattern, Concat role scope, wrapping contract, reading contract, and a compiling doctest (core-source reemitter). |

No rustdoc expansions required; zero DOC_GAPS entries raised by the pass.

## Milestone log

- M0: this plan. (Committed before other work.)
- M1: docs/language-syntax.md written, 12,490 bytes (target ≤ ~10–15 kB);
  2 doctests pass. Sources: latexlike module rustdoc (mod.rs module doc,
  default_token_rules, builtin_package, MathGroupForm, GroupType,
  CallableType, minidefs), token-rule item docs (CommandRule, CommentRule),
  concepts-overview anchors.
- M2: docs/node-trees.md written, 9,008 bytes (target ≤ ~10–15 kB); 1
  doctest passes. Module-doc sufficiency pass: all four modules SUFFICIENT
  (table above), no rustdoc changes needed. Sources: core::node facade doc,
  NodeKind/NodeRef/GroupData/CallableData item docs, latexlike NodeRef sugar
  docs, the four module docs.
- M3: docs/specs.md written, 15,945 bytes (target ≤ ~20 kB); 5 doctests pass
  (incl. the `no_run` filesystem-resolver recipe, compile-checked). Sources:
  core::specs facade + Package/Scope/ScopeOp item docs, latexlike spec.rs
  (spec types + define one-liners), arguments.rs (code factory, fallback
  trap, BracedOnly, named specs), environments.rs (EnvironmentSpec/
  VerbatimBehavior/EnvironmentBehavior), minidefs, source/resolver.rs
  (SourceResolver contract, check_include_chain, MapResolver), input.rs
  (input_macro_spec). The "enum spelling of codes" ruled item is rendered as
  the documented typed alternative (codes resolve to configured argument
  parsers; ArgumentSpec built from parser types directly) plus the word
  codes — there is no argument-code enum in the API, and argument_specs'
  docs state the factory is convenience, never a requirement.
- M4: docs/parsing.md written, 8,753 bytes (target ≤ ~10–15 kB); 3 doctests
  pass. DOC_GAPS #1 RESOLVED (no rustdoc change needed): all 25 public
  condition types' rendered pages display their identifier via the
  derive-generated `impl DiagnosticInfo`'s rendered
  `const IDENTIFIER = "…"`; the DiagnosticInfo trait page's Implementors
  section lists them (mechanical check script; see the register entry).
  Sources: error.rs module + item docs (Diagnostic/Diagnostics/Recovery/
  ParseError, render/sort/cap), engine/language.rs (Language, parse,
  parse_source, recovery paragraph), nodes_parser.rs condition docs,
  latexlike driver builder docs.
- M5: docs/learn-by-example.md revised, 29,561 bytes (target ≤ ~30 kB); 20
  doctests pass. Added sections: Rendering diagnostics (render_all +
  LineIndexCache line/col on a node + sorted_by_position pointer — the T1/T4
  friction signals), Including other sources (input_macro_spec +
  MapResolver — F8 signal), Transforming and recomposing (restage drop-
  comments + source_recomposer round trip — the reconstruction pipeline).
  Ruled folds landed: generic `name()` beside `macro_name()` (Defining
  macros), `body()` None semantics (Environments), `descendants()`
  self-exclusion sentence (Reading nodes), argument-codes typed-alternative
  + BracedOnly + argument_specs_named pointer paragraph (Defining macros),
  body-scoped-definitions example (minidefs `\item`, Environments).
  REMOVE-rule application: first draft was 31,332 bytes; the `\text`
  exit-math doctest block was removed and replaced by a four-line pointer
  paragraph to Event::ExitMathContext's documented recipe (the least
  everyday example; the API item carries the full worked recipe). Also
  removed as now-duplicated in specs.md: the equation body-delta doctest
  (replaced by the body-scoped example + pointer) and the no-cross-check
  paragraph (one-sentence pointer to specs.md + Package::insert). Process
  wording scrubbed ("Phase 7.9", "a later phase", "pylatexenc-modern",
  "level-1 recomposition"); existing heading slugs all kept (three new
  headings added; nothing links into this page's anchors — grepped).
  Curation inputs consulted for signals only (SYNTHESIS §4–§5); no API
  spelling copied from them.
- M6a (post_space note): ADDED — the latexlike invocation-syntax macro-data
  docs (InvocationSyntaxData enum, Macro bullet;
  techy/src/latexlike/invocation_syntax.rs) now state that source
  recomposition re-emits the recorded post-space verbatim and that any
  smarter spacing policy belongs to a converter built on techy, not to
  techy. (The re-emission fact alone was already implied by the module doc's
  "reemitting the exact input bytes" sentence; the policy half was absent —
  hence a brief addition at the record's own docs, doc-only.)
- M6b (input caching note): ALREADY COVERED — `input_macro_spec`'s rustdoc
  carries a dedicated "# No input caching" section
  (techy/src/latexlike/input.rs) stating exactly the ruled content: content
  is read through the resolver at parse time on every inclusion; a
  parse-without-attachment cache is unsound because an inclusion can change
  the caller's parsing state (grounded in the documented persist_state
  behavior); resolvers may freely cache content. One repair made while
  verifying: that section's closing sentence referenced "The guide's include
  chapter", a chapter that does not exist in the ruled Phase 4 chapter map
  (the ruling routed the caching trade-offs to API doc only). Rewrote the
  sentence to keep the brief separate-parse-then-splice condition in the API
  doc itself with no guide reference (doc-only; flagged here as a small
  in-scope deviation — leaving a dangling chapter reference seemed worse).

## Gates (run at M6, after all edits)

| Gate | Result |
|---|---|
| `cargo build` | PASS (clean) |
| `cargo test` (all suites) | PASS — 758 lib + 30 acceptance + 8 derive_conditions + 21 recompose_oracle + 1 techy-derive |
| `cargo test --doc` | PASS — 49 doctests (2 ignored), incl. this stage's 31 new/revised guide doctests (count corrected at review: 2+1+5+3+20) |
| `rm -rf target/doc && cargo docs` | PASS — zero warnings |
| `scripts/check_semver.sh` | PASS — "no semver update required" (196 checks pass) |
| Four-step wiring | Intact for all five chapters (wired in G1; verified lib.rs guide block + GUIDE_PAGES + guide.md index untouched) |
| Superseded-names sweep | CLEAN over all seven touched files (two grep batches over the [§dd-dr:superseded-names] register) |
| DOC_GAPS #1 re-verified on the clean doc build | 25/25 condition pages show their identifier |

## Chapter size table

| File | Bytes | Target | Status |
|---|---|---|---|
| docs/language-syntax.md | 12,490 | ≤ ~10–15 kB | OK |
| docs/node-trees.md | 9,008 | ≤ ~10–15 kB | OK |
| docs/specs.md | 15,945 | ≤ ~20 kB | OK |
| docs/parsing.md | 8,753 | ≤ ~10–15 kB | OK |
| docs/learn-by-example.md | 29,561 | ≤ ~30 kB | OK (REMOVE rule applied once) |

## DOC_GAPS delta

- #1 (condition-page identifiers): OPEN → RESOLVED, no rustdoc change needed
  (evidence in the register entry; mechanical 25/25 check, re-run on the
  clean gate build).
- #2 (WebAssembly rustdoc mention): untouched (G5 scope).
- New entries: none — every chapter claim traced to a documentation sentence
  or is demonstrated by a compiling doctest in the chapter.

## Deviations / items for the supervisor

1. Stale worktree HEAD at agent start (2110bbb) — reset to api-review tip
   f8b2987 before branching, per the brief. Procedural.
2. The "enum spelling of codes" item in the M3 brief: there is no
   argument-code enum in the API. Rendered as the documented typed
   alternative (codes resolve to configured argument parsers; ArgumentSpec
   built from parser types directly) plus the word codes
   (BracedOnly/AnyDelimited…), all sourced from argument_specs' rustdoc. If
   the ruling meant something else, the chapter text is one paragraph to
   adjust.
3. M6b repair of the dangling "guide's include chapter" sentence in
   input_macro_spec's rustdoc (see M6b above) — doc-only, one sentence,
   flagged for review.
4. learn-by-example retains two references to the repository path
   `techy/tests/acceptance.rs` (header + closing section) as the pin for
   pylatexenc-parity claims. It is a repo test path, not a dev-docs
   reference; kept deliberately, easily removable if the reviewer reads the
   writing rules more strictly.
5. Not done (out of scope, noted): none of the G1-frozen pages needed
   changes; no code beyond rustdoc comments was touched (semver gate green).
