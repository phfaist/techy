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

## M2+M3 — panic-policy sweep (COMPLETE)

Method: scripted extraction of every `debug_assert!`/`assert!`/`panic!`/
`unreachable!`/`.unwrap()`/`.expect(` site in techy/src + techy-derive/src
with `#[cfg(test)]`-region tracking (brace-scoped, verified by spot checks);
~4000 test-code sites exempt (tests assert by design; includes the whole
`transform/tests.rs`, `recompose/tests.rs`, `latexlike/test_support.rs`,
`latexlike/invariants.rs` cfg(test) files and the cfg(test)-gated
`check_tree_invariants` wrapper, whose purpose is panicking — policy-exempt).
Non-test sites classified below. Conversion pattern = the S5-established
implementation-error path (`cx.implementation_error(...)` /
`TokenErrorKind::Custom(ImplementationError)`, both aborting even under
tolerant recovery).

### Converted sites (outer-layer input; behavior change ruled by the policy)

| Site (pre-change) | Guarded input | Conversion |
|---|---|---|
| constructs/environment_parser.rs:512 `debug_assert!(false, "stop token disappeared on re-peek")` | custom reader re-peek (rider-named site) | `Err(cx.implementation_error(...))`; test `a_reader_dropping_the_stop_token_on_repeek_...` |
| constructs/environment_parser.rs:515 kind-change re-peek `debug_assert!` | custom reader re-peek (rider-named site) | validated destructure → `Err(...)`; the now-unreachable `_ => None` facts arm removed; test `a_reader_changing_the_stop_token_on_repeek_...` |
| constructs/environment_parser.rs:565 `debug_assert_eq!(cx.tokens.pos(), after_command)` | custom reader position drift after a failed name-group read | `if != → Err(cx.implementation_error(...))` |
| constructs/environment_parser.rs:627 + group_parser.rs:173 `debug_assert!(delta.is_none())` | driver-factory content-loop parser (custom driver) | `if delta.is_some() → Err(...)` |
| constructs/environment_parser.rs:666 + group_parser.rs:196 `unreachable!(NodeCondition)` | driver-factory content-loop parser's stop cause | `Err(...)` (pattern-identical to the root-loop guards at engine/language.rs:196/203, which carry the Bogus-driver test) |
| constructs/argument_parsers.rs:552/793 `any_of` empty-rules `debug_assert!` (constructors) | spec author (rider-named class) | constructors infallible + parse-time `Err(...)`; tests `empty_any_of_rule_set_...` / `empty_optional_any_of_rule_set_...` |
| constructs/argument_parsers.rs:893 empty-marker `debug_assert!` | spec author | parse-time `Err(...)`; test `empty_marker_is_an_implementation_error` |
| constructs/embellishments_parser.rs:85/86 empty-markers `debug_assert!`s | spec author | parse-time `Err(...)`; tests `an_empty_marker_list_...` / `an_empty_marker_...` |
| constructs/tack_on_parser.rs:134 duplicate-field-name `debug_assert!` (builder) | spec author | parse-time `Err(...)`; test `duplicate_field_names_...` |
| token/reader.rs:231 `scan_specials` match-end `debug_assert!` | `Lang::scan_specials` hook output (release hazard: zero-width match = infinite loop) | `Err(TokenError::new(Custom(ImplementationError), .., None))` — unrecoverable, aborts both modes; test `scan_specials_invalid_match_end_...` |
| token/reader.rs:179–180 `move_to_pos` debug asserts | custom recoveries' `resume_pos` / caller-held tokens via `move_past` (release hazard: slice panic at next scan) | single-boundary validation at `peek_impl` entry → unrecoverable Custom ImplementationError; `move_to_pos` asserts removed (one validation regime); test `invalid_reader_position_...` |
| constructs/nodes_parser.rs:484/501 chars-run contiguity `debug_assert!`s | custom reader token stream (release hazard: silently-wrong covering span) | `take_pre_space`/`extend_run` → `Result<(), String>`, lifted at the three cx-holding call sites into `cx.implementation_error` |
| constructs/argument_parsers.rs:667(+849/852/862/870-block, chars_group_parser.rs:182) staged-id `.expect("just staged")` read-backs | driver-factory-returned `BuildId` | graceful degradation to zero-child/identity answers per the policy's recorded staged-id rule (bogus id still lands in the region and `builder.add` diagnoses it) |

Test delta: +10 lib tests (740 → 750). All conversions abort under BOTH
recovery modes (implementation errors bypass the recover funnel — asserted by
running the new tests under `Recovery::Tolerant`).

### Sites consciously left (with justification)

Rule-1 (verifiably unreachable crate-internal invariant, invariant stated):

- extract.rs:507 (`split_pieces` mints parts only for Chars nodes — same-fn
  minting), extract.rs:756 (`max_split=1` arithmetic), extract.rs:189/522/529
  (`piece sub-ranges on char boundaries by construction`), extract.rs:1135
  (`len 1` checked the line above).
- token/reader.rs:215/364 (`pos < len checked above`), token/reader.rs:361
  (`PrefixEntry` fields private; `PrefixTable::for_rules` is the only
  constructor and always sets a direction).
- engine/mod.rs:218/236 (`with_parsing_state`/`with_frame` pop exactly what
  they pushed — same-fn pairing).
- node/builder.rs:279/296/298/349 (the builder's internal post-validation
  read-backs — explicitly sanctioned by [§dd-dr:panic-policy]: "validate at
  the boundary, assert inside").
- transform/context.rs:493/500/524 (restage path invariants over
  crate-computed regions; S7-review-verified conformant).
- node/display.rs:117 (`just pushed`), latexlike/recompose.rs:149
  (`core_source_instruction` answers every non-callable kind — the arm runs
  only for non-callables), latexlike/minidefs.rs:83/93 (literal in-crate
  argument-code lists), node/node_ref.rs:37 (pub(crate) constructor; public
  boundaries validate ids via `get`).
- constructs/argument_parsers.rs:462 (`region_with_last_as_content`
  emptiness: every `Some`-returning arm of `parse_expression_node`/
  `dispatch_expression_invocation` pushes crate-side before returning).
- techy-derive/diagnostic_info.rs:43/159/198/308/313 (`Fields::Named` ⇒
  `ident` is `Some` — syn structural invariant; proc-macro compile-time
  context besides).

Rule-3 (explicitly approved indexing-style accessors with non-panicking
companions, documented panics):

- node/tree.rs:205/211/270 (`NodeTree::node`/`nodes_in` asserts; `get` is the
  companion), node/slice.rs:46, node/arguments.rs:185 (`ChildRegion`
  resolved-only accessors; `staged()` companion) — all named in the policy's
  approved list.

Recorded-pattern family (debug-asserted caller contract on a value
constructor with graceful/diagnosed release behavior — the shape the policy's
applied consequences sanction for `skip_whitespace` and `Span::len`; no `Err`
channel exists in a value constructor, and the violating value is diagnosed
where it breaks the parse, which M2/M3 made an implementation error for the
reader-side routes):

- source/span.rs:30 (`Span::new`) + :81 (`extend_to`) — documented
  "caller's contract (debug-asserted)"; `len`/`is_empty` saturate.
- source/source.rs:235/242 (`SourceSpan::new`) + :371/377 (`SourcePos::new`)
  — documented "checked in debug builds"; the slice panic beyond is the
  rule-3-approved `content()` documented panic.
- token/token.rs:153-170 (`Token::new` span coherence) — doc sentence added
  this stage stating the contract + degraded-release behavior.
- token/list_reader.rs:55 (`TokenListReader::new` source order) — doc
  sentence added; an out-of-order list now surfaces as the run-contiguity
  implementation error at parse time.
- token/reader.rs:100 (`skip_whitespace`) — the policy's own recorded
  exemplar (returns `pos` unchanged; rustdoc cites the panic policy).

Judgment-call note for the reviewer: converting `Span::new`/`SourceSpan::new`
/`SourcePos::new`/`Token::new`/`TokenListReader::new` to `Result` would be an
unruled breaking reshape of ruled constructor surfaces; the recorded-pattern
reading keeps them debug-checked with the release path now funneling into
implementation errors where the parse consumes the bad value. Flagged as
D-plan-2.

Root-loop observation (no change): engine/language.rs:160 discards the root
content loop's pass-through delta (`_delta`) — benign-silent rather than
validated; the root loop's stop-cause guards (lines 196/203) already follow
the implementation-error pattern. Left as the ruled S6 shape.

## M4 — C2 residue assertion (the Phase 3 acceptance audit) — PASS

Realization per D-plan-1: the acceptance run asserts the numbers here (no
shipped line-counting test). Counted on the frozen FLM projection probe
(walkthroughs/framework/flm_projected.rs — the artifact the T5 C2 ruling
measured, last re-verified against the shipped surface in S5/S9), code lines
only (blank/comment/doc lines excluded), by brace-scoped script.

| Block | Counted | Envelope | Verdict |
|---|---|---|---|
| `impl Lang for Flm` — delegation residue (11 associated types + the three one-line hook delegations `initial_state_data`/`scan_specials`/`specials_trigger_chars`, impl header+close) | **25 lines** | ≤ ~30 (Lang) | PASS |
| — plus FLM's *own* ext-mint feature (`make_node_ext`, 14 lines incl. signature): whole impl | 39 lines | (outside the residue: framework-owned behavior any topology requires) | n/a |
| `impl LatexlikeLang for Flm {}` opt-in | 2 lines | (family opt-in, counted for completeness) | n/a |
| `impl ParseDriver<Flm> for FlmDriver` — delegation one-liner bodies (recovery 1, source_resolver 1, resolve_command 1, group_interior_delta 1, resolve_state_event 3) | **7 delegation lines** (26 code lines whole impl incl. signatures + FLM's own `refine_diagnostic` hook) | ≤ ~12 (driver) | PASS |

Context (the preset's own canonical impls, not the assertion's target): the
shipped `impl Lang for Latexlike` (latexlike/mod.rs:327) is 58 code lines —
the preset carries the canonical behaviors itself (the 17-line
`finalize_transition` loud-refusal arm, the seed construction), which a
delegating framework does not rewrite; `impl ParseDriver<LLL> for
LatexlikeDriver<LLL>` (latexlike/driver.rs:393) is exactly **7 hooks with
one-line bodies each** — the pillar-delegation doctrine holds in the shipped
driver.

## M5 — missing_docs → deny (COMPLETE)

Workspace lint flipped to `deny` (Cargo.toml comment updated; both member
crates inherit via `[lints] workspace = true`). Gates under deny: `cargo
build` 0 warnings; full `cargo test` green (counts unchanged); `rm -rf
target/doc && cargo docs` clean.

## M6 — cargo-semver-checks baseline (COMPLETE)

- Tool installed in this environment: cargo-semver-checks 0.50.0 (`cargo
  install cargo-semver-checks --locked`; the sandboxed install failed on the
  `~/.cargo` registry cache write and was retried unsandboxed).
- Pipeline proven end-to-end on the stage tip (self-comparison,
  `--baseline-rev HEAD`): **196 checks pass, 58 skip, "no semver update
  required"**. First run surfaced a real wrinkle: the workspace's
  `.cargo/config.toml` injects `docs/rustdoc-header.html` via root-relative
  `rustdocflags`, which does not resolve in semver-checks' scratch builds —
  the guard run clears `RUSTDOCFLAGS` (doc-presentation only; no bearing on
  the compared surface).
- Durable guard committed: **`scripts/check_semver.sh`** — runs
  `cargo semver-checks check-release -p techy --baseline-rev api-baseline`
  (override via `BASELINE_REV=<rev>`).
- Baseline realization for an unpublished crate (**delegated decision
  D-plan-4**): the baseline is a **git tag `api-baseline`**, to be pinned by
  the supervising session/user on the api-review/main commit where Phase 3
  lands (a tag minted from this stage branch would point at a pre-merge
  commit — deliberately NOT created here); re-pinned deliberately at each
  0.x version bump per the rubric's discipline. Procedure documented in the
  script header + the [§dd-dr:stability-rubric] applied note (added this
  stage).

## Deviation log

- **D-plan-1 (delegated realization)**: the C2 residue assertion is realized
  as a documented acceptance audit in this report (exact numbers), not a
  shipped line-counting test — grounds: all three recorded forms place the
  assertion in the Phase 3 acceptance run; the "~" tolerances and
  formatting-sensitivity make a permanent test brittle; no record orders a
  shipped guard.
- **D-plan-2 (delegated line)**: the value-constructor debug-assert family
  (`Span::new`, `extend_to`, `SourceSpan::new`, `SourcePos::new`,
  `Token::new`, `TokenListReader::new`) is LEFT debug-asserted under the
  recorded `skip_whitespace`/`Span::len` pattern rather than converted to
  `Result` constructors — a `Result` reshape of these ruled surfaces is
  unruled and breaking; the release-mode consumption routes now funnel into
  implementation errors (M2's reader-boundary and run-contiguity
  conversions). See the M3 table's judgment-call note.
- **D-plan-3 (scope extension, policy-grounded)**: two grep-found sibling
  classes beyond the rider's named examples were converted because they carry
  real release-mode hazards reachable from outer layers: the
  `Lang::scan_specials` match-end guard (infinite-loop hazard) with a
  single-boundary reader-position validation at `peek_impl` (slice-panic
  hazard; `StdTokenReader::move_to_pos`'s redundant debug asserts removed for
  one validation regime), and the chars-run contiguity guards (silently-wrong
  spans). Both use the established channels (Custom token error /
  `cx.implementation_error`).
- **D-plan-4 (delegated realization)**: the semver baseline for the
  unpublished crate = the `api-baseline` git tag pinned at the Phase-3
  landing commit (created by the supervisor/user at merge, not from this
  branch), consumed by `scripts/check_semver.sh` via `--baseline-rev`. See
  the M6 section.

## Handoff notes

(none yet)
