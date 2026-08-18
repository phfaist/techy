# Span tiling — progress

Plan: `dev-docs/spantiling/PLAN.md`. Protocol: PLAN §8. Every subagent runs on Opus.

| Stage | Branch | Base | Worktree | Status |
|---|---|---|---|---|
| 1 contract surface | `st-1-contract` | `main` | `.claude/worktrees/st-1-contract` | implemented |
| 2 parsers | `st-2-parsers` | `st-1-contract` | `.claude/worktrees/st-2-parsers` | planned |
| 3a scripted reader | `st-3a-scripted` | `st-1-contract` | `.claude/worktrees/st-3a-scripted` | planned |
| 4 consumers | `st-4-consumers` | `st-1-contract` | `.claude/worktrees/st-4-consumers` | planned |
| 3b tests | `st-3b-tests` | `main` (after 2, 3a) | `.claude/worktrees/st-3b-tests` | planned |
| 5 record | `st-5-record` | `main` (after all) | `.claude/worktrees/st-5-record` | planned |

## Stage 1 — contract surface

Status: **implemented** (branch `st-1-contract`, worktree
`.claude/worktrees/st-1-contract`; commits `dc082d1`, `9942bab`, plus this file).

### Files changed

- `techy/src/state/lang.rs` — new associated const `Lang::OBEYS_SPAN_TILING: bool = true`
  (placed after the last associated type, `Driver`), carrying the canonical definition of
  span tiling per §1.2: the interior tiling of `List`/`Group` children, the
  span-contiguity of a `Callable`'s children block, the payload pins, the "one span per
  node holds either way" note, the "it is a fact, not a choice" paragraph, and the two
  regimes with their consequences. No mode name is coined; the `false` case is always
  spelled "a language with `OBEYS_SPAN_TILING = false`".
- `techy/src/token/reader.rs` —
  - new **required** trait method `TokenReader::source_span_describing(begin, end) ->
    SourceSpan<L::SourceOrigin>` (no default body), documented per §1.3;
  - contract clause 7 (moving sets the position, with the consecutive-token corollary and
    the note that the content loop checks it under both declarations) and clause 8 (one
    source, in reading order, without gaps — required under `OBEYS_SPAN_TILING = true`,
    with the enforcement pointer and the `false` fallback);
  - a new `# Seams` section: what a seam is, the trigger position as the first new-source
    token's `StartBeforePreSpace` edge, the resume position past an exhausted source, who
    chooses the position value and the `source_position_at` coordinate, tokens with edges
    in two sources, runs crossing seams → owned content; plus the four further rules
    (termination is the reader's, positions/tokens stay valid in exhausted sources,
    `SourceProvenance::Synthesized` for expansion sources with no `Frame` pushed,
    `EndOfStream` = end of the whole input);
  - the "Locations leave a reader in exactly one form … several sources" sentence and
    clause 3 now name `OBEYS_SPAN_TILING = false` as the condition;
  - `StdTokenReader::source_span_describing`: the exact range when ordered, the empty
    span at `begin` for an inverted pair (D2);
  - the "Writing a reader over standard tokens" doc example gains the delegating line;
  - tests: `a_described_span_is_the_exact_range_and_never_absent`,
    `consecutive_tokens_meet_at_one_position`, and
    `move_to_lands_at_each_of_the_five_edges` extended with the `move_to_position` half
    of clause 7.
- `techy/src/token/list_reader.rs` — `TokenListReader::source_span_describing` (same rule
  as `StdTokenReader`, with the issued-position checks); tests: a describing-span test on
  ordered/equal/inverted pairs and a `source_span_describing` equality added to the
  lockstep suite.
- `techy/src/token/tokenization.rs` — the `Tokenization::Token` and
  `Tokenization::StreamPosition` docs' multi-source sentences now name
  `Lang::OBEYS_SPAN_TILING = false` (and, for positions, the seam equality);
  `CountingReader` gains the delegating implementation.
- `techy/src/constructs/mod.rs` — `ParseContext::source_span_within` (pub) and
  `invocation_span_within` (private) dispatch on `L::OBEYS_SPAN_TILING`: unchanged under
  `true`, `Ok(self.tokens.source_span_describing(begin, end))` under `false` (never an
  error). Both docs state the two arms; the public one says the described span is a
  coordinate hint from which no content and no structure may be derived. Tests: the
  compile-time default assert, the `RelaxedStdLang` test language (`false` over
  `StdTokenization`), its trivial parse, and the dispatch test. The `min_rules`/`state`
  test helpers became generic over the language.
- `techy/src/constructs/argument_parsers.rs` (`BrokenReader`),
  `techy/src/constructs/environment_parser.rs` (`FlakyReader`),
  `techy/src/constructs/nodes_parser.rs` (`StuckRecoveryReader`, `TabooReader`),
  `techy/tests/lang_features.rs` (`CommentEmittingReader`) — the delegating
  implementation only (what compiling requires; `nodes_parser.rs` is otherwise untouched,
  Stage 2 owns it).

Every in-crate `TokenReader` implementation is covered: `grep -rn "fn
source_span_describing" techy/src techy/tests` gives 10 hits = the trait declaration + 8
implementations + the doc example, matching the §1.3 list exactly.

The §1.4 grep gate is already clean on this branch: `grep -rn
"tokens\.source_span_within\|tokens\.source_span_describing" techy/src techy/tests`
outside `techy/src/token/` returns only the four lines inside the two `ParseContext`
helpers.

### Gate results (verbatim, run from the worktree)

```
### cargo build
   Compiling techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-1-contract/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.44s

### cargo test --workspace
test result: ok. 1067 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 86 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 21.73s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

### cargo test --workspace --all-features
test result: ok. 1106 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.55s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 87 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 22.32s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

### clippy
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-1-contract/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.09s
### clippy --all-features
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-1-contract/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.14s
### docs
 Documenting techy-derive v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-1-contract/techy-derive)
 Documenting techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-1-contract/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.52s
   Generated /Users/philippe/projects/techy/.claude/worktrees/st-1-contract/target/doc/techy/index.html and 1 other file
```

Baseline for comparison: `main` at `fb3d39c` ran 1062 / 1101 lib tests. Stage 1 adds 5
(`a_described_span_is_the_exact_range_and_never_absent` in both reader modules,
`consecutive_tokens_meet_at_one_position`,
`a_language_that_does_not_obey_span_tiling_parses_over_the_standard_reader`,
`source_span_within_describes_the_stretch_when_the_language_does_not_obey_tiling`).
`cargo docs --all-features` emits no warnings at all — no broken intra-doc links.

### §1.9 decisions

- **D2 taken as written**: `StdTokenReader::source_span_describing` answers the empty
  span at `begin` for an inverted pair. `TokenListReader` follows the same rule.
- **D3 taken as written**: under `false`, an inverted same-source pair given to
  `ParseContext::source_span_within` is not an error — the described span is returned.
  Covered by `source_span_within_describes_the_stretch_when_the_language_does_not_obey_tiling`.
- **D5 taken as written**: the `Lang::OBEYS_SPAN_TILING` doc is the canonical definition;
  every other site links to it rather than restating it.
- D1 and D4 are Stage 2's; nothing here anticipates them.

### Deviations from §1

1. **`SourceProvenance::Synthesized` field name.** §1.3 writes `Synthesized { by,
   triggered_at }`; the actual variant is `Synthesized { description, triggered_at }`
   (`techy/src/source/source.rs`). The contract text names the real fields.
2. **Test-language name and placement.** §2 step 5 asks for "a test `Lang` with `false`"
   without naming it; the language is called `RelaxedStdLang` (the §1.7 register name for
   exactly this language) and lives in `techy/src/constructs/mod.rs`'s test module, next
   to the dispatch it exercises. If Stage 2 wants a shared one, it can move this
   definition rather than add a second.
3. **The parse test asserts structure only** (kinds of the root's children +
   `validate_tree`), as §2 step 5 permits: content representation under `false` is
   Stage 2's subject.
4. **Two extra tests beyond step 5**, both cheap and directly tied to the new normative
   text: `consecutive_tokens_meet_at_one_position` (clause 7's corollary on a real token
   walk) and the `source_span_describing` equality inside the existing `TokenListReader`
   lockstep suite.
5. **`min_rules`/`state` in `constructs::tests` became generic** over the language. A
   test-helper signature change only; no behavior change.

### Open questions for the user

1. **Where clause 7's corollary is enforced.** The clause says the content loop reports a
   violation as an implementation error "whatever the language's `OBEYS_SPAN_TILING`
   says" — which matches §1.5 R1 ("the position check stays in both cases"), but Stage 1
   only *documents* it. Today `extend_run_to`'s check runs unconditionally, so the
   sentence is already true; Stage 2 must keep it that way when it rewrites the message.
   Flagging it so the wording and the code do not drift apart.
2. **Nothing yet stops a language from declaring `false` while its `Lang::Tokenization`
   is `StdTokenization`** (that is exactly what `RelaxedStdLang` does, deliberately — the
   same-source relaxed case of §1.7). Confirm that is intended as a supported
   configuration and not merely a test convenience; the const doc currently describes the
   `false` case in terms of readers that serve several sources, which reads as if that
   were the only reason to declare it.
3. **`docs/panics.md` was not touched.** `source_span_describing` bottoms out in
   `SourceSpan::new`, whose always-on assert is already the listed exception, and the
   `TokenReader` contract clause 4 already covers foreign positions. Confirm no new entry
   is wanted.

## Stage 2 — parsers

## Stage 3a — scripted reader

## Stage 4 — consumers

## Stage 3b — tests

## Stage 5 — record

## Orchestrator log

- 2026-08-19: plan written and committed to `main`.
