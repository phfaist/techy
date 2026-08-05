# Phase 3 — S10 report: hardening, guards, audit (FINAL stage)

Branch `phase3-s10-hardening` (worktree
`/Users/philippe/projects/techy/.claude/worktrees/agent-a7cafe0b21311a57c`,
branched from `api-review` @ c6cd171). Status: **COMPLETE** — milestones M1–M7
done, all gates green, audits clean; awaiting review + sign-off + merge.

Baseline counts at branch point: 740 lib + 30 acceptance + 21 oracle +
8 derive-conditions + 1 derive + 36 doctests (2 ignored pre-existing).
Final counts: **751 lib** (+11: ten Err-path tests + the staged-id
degradation pin) + 30 + 21 + 8 + 1 + 36 (2 ignored pre-existing) — all green.

## Final gate table (run at stage tip)

| Gate | Result |
|---|---|
| `cargo build` | PASS, 0 warnings (under `missing_docs = deny`) |
| `cargo test` | PASS — 751 + 30 + 21 + 8 + 1 + 36 doctests (2 pre-existing ignored), 0 failed |
| `rm -rf target/doc && cargo docs` | PASS — 0 warnings/errors under deny |
| `BASELINE_REV=HEAD scripts/check_semver.sh` | PASS — 196 checks pass, 58 skip |
| Surface audit | PASS — 283 pages, 0 duplicates, exact roster (E1) |
| Rider sweep | PASS — 0 MISS (E2) |
| Superseded names | CLEAN (E3; one guide-variable rename applied) |

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
| constructs/argument_parsers.rs:667(+849/852/862/870-block) and chars_group_parser.rs:182 staged-id `.expect("just staged")` read-backs | driver-factory-returned `BuildId` (unbranded Copy) | graceful degradation to zero-child/identity answers per the policy's recorded staged-id rule (bogus id still lands in the region and `builder.add` diagnoses it); chars_group_parser routes through the shared `staged_child_count` helper (review-fix commit — the M2 commit had missed this one site); degradation pinned by test `staged_child_count_degrades_on_a_foreign_build_id` |

Test delta: +11 lib tests (740 → 751; ten Err-path tests in M2 plus the
staged-id degradation pin added with the review fix). All Err conversions
abort under BOTH recovery modes (implementation errors bypass the recover
funnel — asserted by running the new tests under `Recovery::Tolerant`).

### Sites consciously left (with justification)

Rule-1 (verifiably unreachable crate-internal invariant, invariant stated):

- extract.rs:507 (`split_pieces` mints parts only for Chars nodes — same-fn
  minting), extract.rs:756 (`max_split=1` arithmetic), extract.rs:189/522
  (`piece sub-ranges on char boundaries by construction`), extract.rs:247/529
  (`chars kind resolves text` — pieces are minted for Chars nodes only, the
  :507 invariant), extract.rs:1135 (`len 1` checked the line above).
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
  D-plan-4; amended by user ruling at sign-off — movable BRANCH, not tag**):
  the baseline is the **git branch `api-baseline`**, minted by the
  supervising session/user on the api-review/main commit where Phase 3
  lands (a ref minted from this stage branch would point at a pre-merge
  commit — deliberately NOT created here); moved deliberately
  (`git branch -f`) at each 0.x version bump per the rubric's discipline. Procedure documented in the
  script header + the [§dd-dr:stability-rubric] applied note (added this
  stage).

## M7 — full audit

### E1. Public-surface audit vs the ruled roster — PASS (0 misses)

Method = S1's: script over `target/doc/techy/**` real item pages (fresh build;
redirect stubs filtered). Result: **283 item pages, zero duplicate public
paths** (no ident+kind reachable at two module paths; the only same-name pairs
are the two trait+derive-macro doublings in `error`, the std pattern).
Per-module counts:

| Module | Items | | Module | Items |
|---|---|---|---|---|
| root | 1 (`VERSION`) | | `latexlike` | 46 |
| `core` | 41 | | `latexlike::minidefs` | 1 |
| `core::constructs` | 60 | | `recompose` | 8 |
| `core::node` | 33 | | `source` | 19 |
| `core::specs` | 28 | | `transform` | 7 |
| `error` | 16 (incl. 2 derive pages) | | `visit` | 4 |
| `extract` | 19 | | | |

Reconciliation against INVENTORY (202 expected after the S1 accounting) +
rulings, by scripted diff then item-by-item review:

- **Every INVENTORY item present at exactly its ruled home**, modulo exactly
  the ruled removals/renames: `NodeData`/`check_tree_invariants` pub(crate)
  (S1/S3), `NoResolver` removed (S2), the 5 per-kind node-ext aliases +
  `NodeDataExt` removed (S3 ext-minting), `SimpleLang`→`TrivialLang`,
  `resolve_source`→`resolve_source_reference`, `Split`→`SplitAtChars`,
  `MathStyle`→`MathGroupForm`, `base_package`→`builtin_package`. Ruled
  placement overrides verified in place: resolution family + `ScopesCommandResolver`
  in `core::specs`; `ArgumentParser`+`ParsedArgumentNodes` in
  `core::constructs`; `FrameRole` + `PrefixEntry` in the hub;
  `ProvenanceChain`/`ResolvedContent` in `source`; extract helpers at
  top-level `extract`.
- **All 100 items beyond INVENTORY traced to ruled stage additions**, each at
  its ruled home — S2 (sealed conversions `IntoSpecsProvider`/
  `IntoCallableSpec`/`IntoSourceResolver`, `CommandResolver`,
  `ScopesCommandResolver`, `resolve_command_in_scopes`), S3 (`TreeTag`,
  `SourcePos`, `validate_tree` + `TreeViolation`/`TreeViolationKind`,
  `StagedChildren`/`StagedChildView`, `BodySlotExt`, `SlotRole`,
  `display_tree`, `IntoArgumentParser`), S4 (`LatexlikeLang` + the role
  traits, `MathGroupForm`, `Event`, `FinalizeError`, `ParsingStateStack`, the
  three pillars, `LatexlikeNodeExts`, `BodyMarker`), S5 (`InvocationSyntax`
  trait at the hub, `FromInvocation`, `InvocationSyntaxData`,
  `EnvironmentSyntax` + `StdEnvironmentSyntax`/`StdEnvironmentSideSyntax`,
  `EnvironmentBeginSyntaxData`/`EnvironmentTerminatorSyntaxData`,
  `ParagraphBreakSpec`, `LatexlikeInvocationSyntax`), S6 (`NoSourceResolver`/
  `UnresolvableSourceReference`, `check_include_chain`, `IncludingSources`,
  `LineIndexCache`/`LineColProvider`, `format_position_with`/
  `format_traceback_with`, `input_macro_spec`/`InputMacroSpec`,
  `AttachedSourceOutcome`), S7 (`techy::transform` ×7, the extract
  producer triples + part contexts ×19-page module), S8 (`techy::visit` ×4,
  `techy::recompose` ×8, `SourceRecomposer`/`source_recomposer`/
  `SourceRecomposeError`), S9 (`builtin_package`, `minidefs::minilatex_package`,
  `NamedAccessError`, `ProviderCommandsShadowedByEscape` +
  `check_provider_commands_shadowed_by_escape`, `argument_specs_named`).
  (`attach_source_reference`/`parse_attached_source`/`stage_invocation` are
  `ParseContext` methods — no item pages, correctly.)
- Module topology exactly the ruled set: `source`, `error`, `extract`,
  `transform`, `visit`, `recompose`, `core`, `core::{constructs,specs,node}`,
  `latexlike`, `latexlike::minidefs`, root `VERSION` + `__private` +
  `guide` (the latter two excluded by design).

### E2. Rider sweep — all DONE or ROUTED, 0 MISS

Grep base: DESIGN_RATIONALE.md, all rulings files, PLAN.md's two checklist
blocks + NEXT bullet, reports S1–S9, `TODO`-marker sweep over src (zero hits).

| Obligation (source) | Status |
|---|---|
| Tier-C block: NodeData + check_tree_invariants pub(crate); NoResolver deleted; resolve_source_reference rename; StdParseDriver reshape; FrameRole hub; ParsedArgumentNodes constructs; PrefixEntry beside PrefixTable; VERSION rustdoc sentence | DONE (S1/S2/S3; re-verified in E1) |
| Recompose block: driver.rs:127 canonical paragraph-break spec; `materialized` through the bound trait; stage_invocation bundle amendment; CallableData post_space→invocation_syntax; kind.rs invariant-3 rewording; Invocation trigger-token facts; parse-law callable arm reads the payload; RecomposeError mirrors RestageError; bound trait named at application | DONE (S5 + S5-M6 design revision; S8 for the mirror — user-signed D-plan-5) |
| In-crate oracle suite (reemit == input, strict+tolerant, multi-source) | DONE (S8: 21 tests; multi-source rode S6 I-18) |
| C2 driver-residue assertion (T5 §C2 + [§dd-dr:preset-driver-pillars]) | DONE (S10 M4: 25/7, within envelope; DR applied note updated) |
| F5 parse-law checker `Attached`-scoping (T5 §F5) | DONE (S6, per-source byte accounting) |
| I-18 multi-source reconstruction tests (T5 §I) | DONE (S6) |
| A8 extract input-genericity rides annotation application (T5) | DONE (S7) |
| Slice-contract wording without "honest" (T5 §F1) | DONE (S3). Per the S3-stage supervisor resolution (confirmed by the S3 review, covered by the S3 sign-off), the ban is scoped to the session-coined slice-contract term: the slice contracts (`span()`/`source_text()`) use no "honest". Two pre-existing ordinary-English uses remain by that resolution and stay: node/kind.rs:20, source/line_index.rs:259 |
| `\text` recipe forbidden_chars fix (T1T2) | DONE (S4; guide recipe now event-based — re-verified this stage) |
| S5 rider: pre-existing debug_assert siblings → S10 | DONE (S10 M2/M3, full table above) |
| Wire-identifier slate incl. `core.sources.*` at S6 | DONE (S1 + S6; sweep below confirms no old areas) |
| missing_docs → deny; cargo-semver-checks baseline (PLAN Phase-3 line) | DONE (S10 M5/M6) |
| S1 note: module-header narratives → possible guide promotion | ROUTED (Phase 4 guides) |
| T5 I-9 binding-guide chapter checklist; include-chapter challenges + conditional splice recipe (G); post_space re-emission paragraph (I-10) | ROUTED (Phase 4 — recorded in T5_RULINGS Handoffs) |
| S6 reviewer note: custom-Lang finalize_transition replay granularity | ROUTED (Phase 4 custom-Lang chapter, per the S6 stage log) |
| Migration guides, human/AI guides | ROUTED (Phase 4 — PLAN.md) |

### E3. Superseded-names sweep — CLEAN (1 doc should-fix applied)

Scripted sweep of the full [§dd-dr:superseded-names] register (60 pattern
groups, word-boundary guards, false-positive filters) over techy/src,
techy-derive/src, techy/tests, docs/, README.md, CLAUDE.md, ARCHITECTURE.md,
scripts/. Findings:

- All remaining hits are register-style **negative/historical references**
  ("no `ConflictStrategy`", "not `LatexToken`", "there is no `SlotSpec`",
  "`finalize_node` is replaced by", "replacing Phase 4's `push_libraries`",
  the [§dd-arch:naming] rule statements) — these document the rejections and
  are not reintroductions. Ordinary-English "ancestors" in tree.rs:313 is not
  the rejected `Ancestors` API.
- **One real should-fix, applied**: docs/learn-by-example.md's `\text` recipe
  bound its argument spec to a local named `text_mode_argument` — re-teaching
  the rejected factory spelling ([§dd-dr:argument-factory-additions]).
  Renamed to `text_argument`.
- No old wire-identifier areas (`core.nodes_parser.*` etc.), no
  `core.scopes.*`, no removed constructors/Default impls, no
  `Restage::Continue`, no bare `Split`/`StateStack`/`EnvironmentSideSyntax`,
  no `with_provider`/`with_seed_delta`, no `ParsingState::initial()`.

### Records touched this stage (M5–M7)

- [§dd-dr:stability-rubric]: applied note (guards realized; script + tag
  procedure).
- [§dd-dr:panic-policy]: applied note (S10 sweep completion; the durable
  summary of converted vs left classes).
- [§dd-dr:preset-driver-pillars]: C2 amendment updated with the asserted
  numbers.
- [§dd-dr:transform] topic header: stale "None is applied yet" → applied
  (S3–S8).
- ARCHITECTURE.md stability-rubric passage: guards-in-place sentence
  (labels untouched).
- docs/learn-by-example.md: the superseded-name variable rename.
- Cargo.toml: missing_docs deny; scripts/check_semver.sh: new.

## DRAFT — PLAN.md decision-log entry (Phase 3 complete) [DO NOT APPLY HERE]

> - 2026-08-05: **Phase 3 — apply + harden COMPLETE** (S1–S10 all merged; stage
>   log + per-stage detail in PHASE3_PLAN.md / reports/S<N>_REPORT.md). S10
>   (hardening, guards, audit) closed the phase: C2 residue assertion PASSED
>   (25-line Lang delegation residue on the FLM projection, 7 driver delegation
>   one-liners — within the ruled ~30/~12 envelopes); panic-policy sweep
>   complete per [§dd-dr:panic-policy] + the S5 rider (all outer-layer-input
>   guards now Err implementation-error paths or recorded staged-id
>   degradations, +11 tests; value-constructor debug asserts kept under the
>   recorded skip_whitespace pattern — site table in S10_REPORT);
>   `missing_docs` promoted to workspace deny;
>   cargo-semver-checks baseline realized as scripts/check_semver.sh against
>   the `api-baseline` git branch — movable, per the user ruling at sign-off
>   (**ACTION: mint the `api-baseline` branch on the Phase-3 landing commit
>   at merge**); full public-surface audit exact (283 item
>   pages, zero duplicate paths, every item at its ruled home; INVENTORY +
>   all-stage reconciliation in S10_REPORT); all-riders grep sweep: every
>   Phase 3 obligation DONE or consciously ROUTED to Phase 4 (table in
>   S10_REPORT); superseded-names sweep clean. The soft freeze of
>   [§dd-dr:stability-rubric] takes effect at this landing. NEXT: Phase 4 —
>   guides.
>
> (Also check the Phase 3 checkbox in § Phases & status: `[x] Phase 3`.)

## DRAFT — PHASE3_PLAN.md S10 closure [DO NOT APPLY HERE]

> ### S10 — Hardening, guards, audit  [status: DONE — merged 2026-08-05]
>
> Stage-log entry:
> - 2026-08-05: S10 implemented (worktree branch `phase3-s10-hardening` off
>   api-review c6cd171; plan-first + per-milestone commits M1–M7). C2 residue
>   audit PASS (25/7 vs ~30/~12); panic-policy sweep complete (12 converted
>   site groups + full leave-table with justifications; 751 lib tests, +11;
>   one review-fix: the chars_group_parser staged-id read-back had been
>   missed and now shares the degradation helper);
>   missing_docs deny green everywhere; cargo-semver-checks 0.50.0 installed,
>   pipeline proven (196 checks pass on self-comparison), durable guard =
>   scripts/check_semver.sh + the movable `api-baseline` branch (minted
>   on the landing commit; user ruling at sign-off: branch over tag); surface audit 283 pages/zero dupes/exact roster;
>   rider sweep 0 MISS; superseded-names sweep clean (one guide-variable
>   rename applied). Deviations D-plan-1..4 (all delegated realizations /
>   policy-grounded scope extensions — none touch ruled shapes). Reports:
>   reports/S10_REPORT.md.

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
- **D-plan-4 (delegated realization; user-amended at sign-off: movable
  branch, not tag)**: the semver baseline for the unpublished crate = the
  `api-baseline` git branch pointed at the Phase-3 landing commit (created
  by the supervisor/user at merge, not from this stage branch; moved with
  `git branch -f` at each deliberate version bump), consumed by
  `scripts/check_semver.sh` via `--baseline-rev`. See the M6 section.

## Handoff notes

(none yet)
