# Phase 3 — S10 report: hardening, guards, audit (FINAL stage)

Branch `phase3-s10-hardening` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a7cafe0b21311a57c`,
branched from `api-review` @ c6cd171). Status: PLAN COMMITTED — implementation
in progress.

Baseline counts at branch point: 740 lib + 30 acceptance + 21 oracle +
8 derive-conditions + 1 derive + 36 doctests (2 ignored pre-existing).

## Governing inputs read

- PHASE3_PLAN.md § Protocol + § S10; PLAN.md decision log (NEXT bullet + the
  Tier-C and recompose "Phase 3 checklist additions" blocks).
- T5_RULINGS.md (C2 recorded form; §I sweep rows), T5_BRIEF.md §C2,
  [§dd-dr:preset-driver-pillars] amendment (the third recorded C2 form).
- reports/S5_REPORT.md (design-revision rider R8: pre-existing debug_assert
  siblings → S10), reports/S1_REPORT.md (202-item surface-audit method).
- [§dd-dr:panic-policy], [§dd-dr:stability-rubric], [§dd-dr:superseded-names];
  INVENTORY.md + all rulings files as the ruled roster.

## C2 recorded forms (verbatim anchors)

- T5_RULINGS §C: "C2 driver residue verified as ruled (+ Phase 3 checklist
  item: acceptance asserts residue ≤ ~30 Lang + ~12 driver lines)".
- T5_BRIEF §C2: "the acceptance run at Phase 3 should assert the residue stays
  ≤ the recorded ~30 lines (Lang) + ~12 lines (driver)".
- [§dd-dr:preset-driver-pillars] amendment: "(~30-line Lang + ~12-line driver
  residue; the Phase 3 acceptance run asserts it)".

Reading: the assertion belongs to the Phase 3 *acceptance run* (this stage's
audit), not to a permanent line-counting test (the "~" tolerances make a hard
in-repo assert brittle and no record calls for a shipped guard). Realization:
a documented audit in this report with exact counted numbers for (a) the
preset's own `impl Lang for Latexlike`, (b) `LatexlikeDriver`'s `ParseDriver`
hook delegation bodies, and (c) the FLM projection probe's residue
(walkthroughs/framework/flm_projected.rs `impl Lang for Flm` +
`impl ParseDriver<Flm> for FlmDriver`). Recorded as a delegated decision
(D-plan-1 below).

## Milestones

- **M1 — plan** (this commit).
  Also: kick off the `cargo install cargo-semver-checks` background build
  immediately (it is the long pole for M6).
- **M2 — panic-policy sweep, constructs conversions.** The named sites +
  siblings where a `ParseContext`/`Err` channel exists. Conversion pattern =
  the S5-established one: the guard becomes an
  `Err(cx.implementation_error(...))` path (implementation errors bypass the
  recover funnel by design); infallible spec constructors stay infallible and
  the emptiness/shape contract is checked at parse time. Candidate list from
  recon (final classification in the M3 table):
  - environment_parser.rs:512+515 (custom-reader re-peek guards — the two
    sites the rider names), :565 (reader-position guard `debug_assert_eq`),
    :627 + group_parser.rs:173 (pass-through delta from the driver-factory
    content loop — custom drivers are outer-layer), :666 +
    group_parser.rs:196 (`StopCause::NodeCondition` arm from the factory
    parser), :974 (parse_scoped delta), :1085 `.expect("raw body reads as
    chars")` + :1089 verbatim-kind `unreachable!` (reader-dependent token
    kinds).
  - argument_parsers.rs:552+793 (`any_of` empty-rules guards), :893 (empty
    marker), :462 (`region_with_last_as_content` emptiness);
    embellishments_parser.rs:85-86 (empty markers list / empty marker);
    tack_on_parser.rs:134 (duplicate field names). Constructors/builders keep
    their signatures; the check moves to the parse path.
  - reader.rs:100 (`L::scan_specials` match-end guard — Lang hook output; the
    release behavior today can loop forever on a zero-width match; convert to
    the reader's `Err` channel if a suitable TokenErrorKind shape exists,
    else escalate-or-justify in the table).
  Each converted site gains an Err-path test where practical (a misbehaving
  reader/driver/spec that previously tripped the debug_assert must now
  produce the implementation-error diagnostic).
- **M3 — panic-policy sweep, full classification table.** Scripted extraction
  of every non-test-code `debug_assert!`/`assert!`/`panic!`/`unreachable!`/
  `.unwrap()`/`.expect(` site in techy/src + techy-derive/src; classify each:
  CONVERTED (M2) / LEAVE-rule-1 (verifiably unreachable, invariant stated —
  repair any bare messages) / LEAVE-rule-3 (approved indexing-style accessors)
  / LEAVE-recorded-pattern (the debug-checked value-constructor family:
  `Token::new`, `Span::new`, `SourceSpan::new`, `SourcePos::new`,
  `TokenListReader::new`, reader position/contiguity guards, `NodeRef::new`
  tree-tag guard — same shape as the policy's recorded `skip_whitespace`
  "debug-asserted + graceful/diagnosed release behavior" consequence; no
  `Err` channel exists in a value constructor and release behavior degrades
  to a downstream-diagnosed state, not a crash) / exempt (test code;
  `check_tree_invariants` wrapper — panicking is its API). Full table in
  this report; any site that resists classification → escalate.
- **M4 — C2 residue audit.** Count + record the numbers per the reading
  above; verify ≤ ~30 / ~12; if exceeded → treat as an audit MISS
  (investigate; escalate if not small).
- **M5 — missing_docs → deny.** Workspace lint flip (comment updated); gates:
  `cargo build` 0 warnings, full `cargo test`, `rm -rf target/doc && cargo
  docs` clean under deny.
- **M6 — cargo-semver-checks baseline.** Install (background from M1; crates
  network is allowlisted); prove the pipeline with a self-comparison run
  (`--baseline-rev` on the stage tip); commit a helper script + document the
  exact baseline procedure (baseline = the Phase-3 landing commit on
  api-review, pinned by the user at merge); DR [§dd-dr:stability-rubric]
  applied note. If the tool cannot run here: document + script anyway, say so.
- **M7 — full audit + records + closure.**
  1. Public-surface audit: rebuild docs, re-run the S1 script method over
     `target/doc/techy/**` real item pages; reconcile against INVENTORY as
     amended by Tier-C/T3/T4 rulings + every item added/removed by S2–S9
     (from the stage reports' surface sections). Every item at exactly its
     ruled path, no other.
  2. Rider sweep: grep DESIGN_RATIONALE.md + all rulings files + PLAN.md
     checklist blocks + reports S1–S9 for every "Phase 3" obligation/rider →
     table DONE (where) / ROUTED (where) / MISS. Small misses fixed
     in-stage; larger → STOP + escalate.
  3. Superseded-names sweep (full, final) over src/tests/docs/README.
  4. DR status/applied notes touched by this stage; DRAFT (as text in this
     report, NOT edits to the owned files) the PLAN.md Phase-3-complete
     decision-log entry and the PHASE3_PLAN S10 closure entry.
  5. Final full gate run + closure of this report.

## Gates (every milestone that touches code)

`cargo build` (0 warnings) · `cargo test` (green; counts vs baseline
recorded) · `rm -rf target/doc && cargo docs` (clean; deny from M5 on) ·
superseded-names sweep at M7 · behavior changes only where ruled (the
panic→Err conversions are ruled by [§dd-dr:panic-policy] + the S5 rider).

## Deviation log

- **D-plan-1 (delegated realization)**: the C2 residue assertion is realized
  as a documented acceptance audit in this report (exact numbers), not a
  shipped line-counting test — grounds: all three recorded forms place the
  assertion in the Phase 3 acceptance run; the "~" tolerances and
  formatting-sensitivity make a permanent test brittle; no record orders a
  shipped guard. (Further deviations appended as they arise.)

## Handoff notes

(none yet)
