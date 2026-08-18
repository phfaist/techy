# Span tiling — progress

Plan: `dev-docs/spantiling/PLAN.md`. Protocol: PLAN §8. Every subagent runs on Opus.

| Stage | Branch | Base | Worktree | Status |
|---|---|---|---|---|
| 1 contract surface | `st-1-contract` | `main` | `.claude/worktrees/st-1-contract` | reviewed |
| 2 parsers | `st-2-parsers` | `st-1-contract` | `.claude/worktrees/st-2-parsers` | implemented |
| 3a scripted reader | `st-3a-scripted` | `st-1-contract` | `.claude/worktrees/st-3a-scripted` | reviewed |
| 4 consumers | `st-4-consumers` | `st-1-contract` | `.claude/worktrees/st-4-consumers` | implemented |
| 3b tests | `st-3b-tests` | `main` (after 2, 3a) | `.claude/worktrees/st-3b-tests` | planned |
| 5 record | `st-5-record` | `main` (after all) | `.claude/worktrees/st-5-record` | planned |

## Stage 1 — contract surface

Status: **reviewed** (branch `st-1-contract`, worktree
`.claude/worktrees/st-1-contract`; commits `dc082d1`, `9942bab`, plus this file).

Review PASS, no blocking fixes; four non-blocking wording fixes applied: clause 7's
enforcement sentence narrowed to what the content loop actually checks (the token it is
about to add to a pending chars run, not every consecutive pair); clause 8's "no gaps"
rephrased to the bytes between the earlier token's `End` edge and the later token's
`Start` edge (the earlier token's own spelling also lies between the two `Start` edges);
the const doc's `false` arm now says several sources are the typical but not the only
reason to declare it (open question 2 below, answered); "the engine's descent guard"
linked as [`DescentGuard`](crate::engine::DescentGuard).

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

Status: **implemented** (branch `st-2-parsers`, worktree
`.claude/worktrees/st-2-parsers`; commits `df520fe`, `e40424d`, `8e3ffa3`, `048c6b9`,
`0df3bc9`, plus this file).

### Files changed

- `techy/src/constructs/nodes_parser.rs` (**R1**) — the pending chars run becomes a
  named `PendingRun<L> { start, end, text: Option<String> }` (`text` is `Some` exactly
  when `!L::OBEYS_SPAN_TILING`), with a hand-written `Debug`. `take_pre_space` and
  `extend_run` (which now receives the classified character) accumulate the extension's
  text from the reader's answers about *that* token; `chars_run_node` turns a run into
  the `Chars` kind + span — `TextContent::Owned(text)` where a text was accumulated,
  `TextContent::Spanned(span.span())` (the exact run slice, as before) otherwise. The
  position check in `extend_run_to` stays **unconditional** and its message names the
  actual violation ("the token's StartBeforePreSpace edge … is not the position the
  stream stood at when the token was peeked … — the token reader violates the
  `TokenReader` contract (a peeked token starts where the peek happened; moving to an
  edge sets the position)"); the `what` parameter is gone with the old wording (both
  call sites extend from the token's `StartBeforePreSpace` edge, which the message
  names). Docs: invariant 1 states both content representations; the closing sentence
  is "For a language that obeys span tiling these give span tiling: …"; the three other
  occurrences of "partition invariant" as a name (module docs, `flush_for_token_stop`,
  two test-side comments) are rewritten.
- `techy/src/constructs/mod.rs` (**R2**, **carry-over A**, **carry-over B**) —
  `comment_node_kind` records the three sub-spans through `node_text_content` (the
  argument that they tile the token fails at a seam); `node_text_content`'s doc names
  `OBEYS_SPAN_TILING = false` and the seam case, and states that the rule assumes
  nothing about tokens; `stage_invocation`'s no-explicit-end arm gets a
  `None if !L::OBEYS_SPAN_TILING` arm that routes through `invocation_span_within` from
  the trigger's start position to `position_here()` — the tiled arm is untouched — with
  the doc's three end cases extended by the `false` case and the `Err` paragraph scoped
  to a tiled language. New `pub(crate)` helpers `push_pre_space_text` and
  `push_token_text` (the per-token owned-text recipe, D4), shared by R1 and R4.
- `techy/src/constructs/verbatim_parser.rs` (**R4**) — `read_raw_content` accumulates
  the content's text under `!L::OBEYS_SPAN_TILING`: per consumed token the R1 recipe
  (pre-space + spelling + post-space), including a nested close read as content by the
  pairing rule, plus the pre-space of the terminator / end-of-stream token (which lies
  before `content_end` and is content). `RawContentEnd` carries it as `content_text`,
  and `raw_content_text` records `Owned`/`Spanned` at both content sites (the delimited
  argument and the environment body). Module docs state the two representations; the
  "partition invariants" phrase is gone.
- `techy/src/constructs/environment_parser.rs` — wording only ("the gap-free tiling
  contract" → "span tiling, where the language obeys it").
- `techy/src/latexlike/invocation_syntax.rs` (**R3**) — `from_invocation`'s macro arm
  records `post_space` as `Spanned` under `true` and `Owned` under `false`, with the
  comment rewritten (the "sound because the node starts at this very token" argument
  holds under tiling only) and the impl doc stating both.
- `techy/src/node/invariants.rs` (**R6**) — `check_tree_invariants` returns after
  `validate_tree` when `!L::OBEYS_SPAN_TILING`; docs rename "parse-tree law" to
  "span-tiling law" throughout and state the gate; the two byte-accounting assertion
  messages say "span tiling" instead of "partition invariant" (the `should_panic`
  expectation follows).
- `techy/src/latexlike/invariants.rs` (**R6**) — `check_latexlike_tree_invariants`
  gates its payload pins the same way (they are byte accounting too), with the docs
  renamed and the gate explained.

### R2 — "node span == fact span, bare `Spanned` kept" (checked list)

Complete against `grep -n "NodeKind::chars(\|TextContent::Spanned("` over
`techy/src/constructs/` and `techy/src/latexlike/`, test modules excluded:

| Site | What | Verdict |
|---|---|---|
| `constructs/attached_source.rs:192` | the attached-source placeholder chars node | node span == fact span → `Spanned` kept |
| `constructs/argument_parsers.rs:207` | `stage_pre_space`: a pre-space-only chars node | node span == fact span → `Spanned` kept |
| `constructs/argument_parsers.rs:263` | expression-position chars over one token | node span == fact span → `Spanned` kept |
| `constructs/argument_parsers.rs:307`, `:318` | unresolvable / failed command fallback over one token | node span == fact span → `Spanned` kept |
| `constructs/argument_parsers.rs:999` | `MarkerArgumentParser`'s chars node (**several tokens**) | `Spanned` kept per R2/R5 — flagged, open question 1 |
| `constructs/nodes_parser.rs:908` | `recover_as_chars` over one token's span | node span == fact span → `Spanned` kept |
| `constructs/verbatim_parser.rs:785` | the gobbled leading newline (one token) | node span == fact span → `Spanned` kept |
| `latexlike/driver.rs:267` | the paragraph-break `Chars` shape over the break span | node span == fact span → `Spanned` kept |
| `latexlike/environments.rs:862` | malformed-`\begin` chars fallback over the trigger | node span == fact span → `Spanned` kept |
| `latexlike/environments.rs:1066` | orphan-`\end` recovery chars (**several tokens**) | `Spanned` kept — flagged, open question 1 |
| `constructs/environment_parser.rs:992`, `:1194` | inside `mod tests` (a test language's raw-body parser) | test code; goes through the dispatch already |
| `constructs/mod.rs:142` | `node_text_content` itself | the rule |
| `constructs/nodes_parser.rs:620`, `verbatim_parser.rs:177`, `latexlike/invocation_syntax.rs:148` | the three sites this stage changed (R1, R4, R3) | dispatch on the const |

The single-token facts already going through `node_text_content` before this stage —
group delimiters (`group_parser.rs`), verbatim delimiters (`verbatim_parser.rs`),
embellishment markers (`embellishments_parser.rs`), the environment sides
(`latexlike/invocation_syntax.rs` `from_parsed`) — are unchanged (D1).

### R3 grep — other pre-staging payload builders

`grep -rn "impl.*FromInvocation" techy/src techy/tests` → two impls: `()` (records
nothing) and `InvocationSyntaxData` (the R3 site). `grep -rn
"macro_form\|specials_form\|environment_form\|from_parsed("` → `macro_form` has no
in-crate call site (the standard path is `from_invocation`); `specials_form`
(`latexlike/driver.rs:286`) records nothing; `environment_form`
(`latexlike/environments.rs:1007`) wraps `from_parsed`, which already receives the
node span and goes through `node_text_content`. No further change needed.

### R5 grep — the §1.4 gate

`grep -rn "tokens\.source_span_within\|tokens\.source_span_describing" techy/src
techy/tests` outside `techy/src/token/` returns exactly the four lines inside the two
`ParseContext` helpers (`constructs/mod.rs:267`, `:269`, `:449`, `:451`) plus the
delegating lines of the five test readers. Every node/body/name span goes through
`cx.source_span_within` / `invocation_span_within`; after carry-over A there is no
longer a site building a node span with `SourceSpan::new` from a child's coordinates
either.

### R7 grep — concrete `Latexlike` impls

`grep -rn "for Latexlike\b" techy/src/latexlike/*.rs` → three blocks: `impl Lang for
Latexlike` (associated types + the seed state), `impl LatexlikeLang for Latexlike`
(`check_parse_start`), `impl SerializableLang for Latexlike` (marker). None builds node
data or spans; every preset parser is generic over `LLL: LatexlikeLang` and reaches the
declaration through the shared helpers. Nothing to make generic.

### Tests added

- `constructs/nodes_parser.rs`:
  `a_language_that_does_not_obey_span_tiling_parses_the_same_tree` — the input
  `"a b{c d}%note\n\ne \foo f \arg{g} h"` (chars runs, group, comment, paragraph break,
  a zero-argument macro with post-space, a macro with a `{…}` argument) parsed under
  `CmdLang` (tiled) and `RelaxedStdLang` (`false`) through `run_both` (so both readers
  agree in both runs): identical `shapes` (kinds, spans, resolved text), identical node
  counts, all chars content `Spanned` under `true` and `Owned` under `false` except the
  paragraph-break node (whose span is the fact's own span), `validate_tree` on both.
  `a_relaxed_chars_run_owns_the_text_the_reader_answered` pins the owned run text
  (leading and trailing whitespace included).
  `a_token_not_starting_where_it_was_peeked_is_an_implementation_error` — the new
  `SlippingReader` answers a `StartBeforePreSpace` edge one byte off; the same
  implementation error, with the same detail, under a tiled and a relaxed language.
- `constructs/verbatim_parser.rs`:
  `verbatim_content_is_owned_where_the_language_does_not_obey_span_tiling` — `\verb|a b|`
  under both declarations; owned content that reads back as the tiled span slice, the
  delimiters still spans, `validate_tree` OK.
- `latexlike/invocation_syntax.rs`:
  `the_macro_post_space_is_owned_where_the_language_does_not_obey_span_tiling` — the
  payload built directly from an `Invocation` over `"\foo  x"`: `Spanned(4..6)` under
  the tiled language, `Owned("  ")` under the relaxed one.
- `node/invariants.rs`:
  `a_language_that_does_not_obey_span_tiling_is_held_to_the_all_trees_law_only` — the
  sibling-gap tree that `rejects_a_gap_between_siblings` panics on passes the oracle
  under `RelaxedStdLang`.

Test-language plumbing (reuse, no duplicate): `constructs::tests` is now
`pub(crate) mod tests` (test builds only) and exports `RelaxedStdLang`, its tiled twin
`PlainLang`, `RELAXED_MACRO`, `relaxed_driver`, `min_rules` and `state`.
`RelaxedStdLang`'s driver became
`StdParseDriver<ScopesCommandResolver<RelaxedStdLang>>` so tests can define macros for
it the way they do for a tiled language (the two Stage 1 tests were updated to build it
through `relaxed_driver`). `verbatim_parser::tests::rules` became generic over the
language for the same reason.

### Gate results (verbatim, run from the worktree)

```
### cargo build
   Compiling techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.53s

### cargo test --workspace
test result: ok. 1073 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.69s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 86 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 12.82s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

### cargo test --workspace --all-features
test result: ok. 1112 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.73s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 87 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 15.97s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

### cargo clippy --workspace --all-targets -- -D warnings
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.86s

### cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.18s

### rm -rf target/doc && cargo docs --all-features
 Documenting techy-derive v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy-derive)
 Documenting techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.18s
   Generated /Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/target/doc/techy/index.html and 1 other file
```

Baseline: Stage 1 ran 1067 / 1106 lib tests; Stage 2 adds 6 (the five new tests plus
the `constructs::tests` split is unchanged) — 1073 / 1112. No existing test changed its
expectations except `node::invariants::rejects_a_gap_between_siblings`, whose
`should_panic` string follows the renamed assertion message. `cargo docs --all-features`
emits no warnings.

### §1.9 decisions

- **D1 as confirmed by the user (2026-08-19)**: single-token facts keep the node-data
  rule in both cases; multi-token content (R1, R4) and the pre-staging payload (R3) are
  owned under `false`.
- **D4 as written**: the owned text comes from the reader's own answers about each
  token — `source_span_between(tok, StartBeforePreSpace, Start).content()`, the
  `TokenKind::Char(c)` spelling (or a group close's `delim` where the verbatim pairing
  rule reads one as content), and
  `source_span_between(tok, End, EndPastPostSpace).content()`. The recipe lives once, in
  `constructs::push_token_text`.
- The position check of `extend_run_to` stays unconditional (user ruling, 2026-08-19);
  only its message and the surrounding docs changed.

### Deviations from §1

1. **`extend_run_to` lost its `what` parameter.** §1.5 R1 prescribes a message that
   names the token's `StartBeforePreSpace` edge; both call sites extend from exactly
   that edge, so the "which extension" string the old message carried had nothing left
   to add. The prescribed wording is used verbatim.
2. **R4's `environment_parser.rs ~1189–1193` is test code.** The line the plan points
   at is inside `environment_parser.rs`'s `mod tests` (a test language's raw-body
   parser, which already computes its spans through `cx.source_span_within`). The
   production environment verbatim body is `VerbatimBodyParser` in
   `verbatim_parser.rs ~760` — changed. Likewise `verbatim_parser.rs ~736` (listed
   under R4) is the gobbled leading newline: one token, node span == fact span, so it
   keeps the bare span under R2.
3. **No environment test through the preset.** §3 step 8 makes it optional ("if
   cheap"); a `LatexlikeLang` declaring `false` is Stage 3b's subject, and the preset's
   environment path is exercised generically by the R3 unit test and the
   `stage_invocation` coverage in the nodes-parser comparison. The relaxed *verbatim*
   body path is covered through `VerbatimArgumentParser`.
4. **Test-language sharing changed `constructs::tests` to `pub(crate) mod tests`** and
   gave `RelaxedStdLang` a scope-stack command resolver (see "Tests added"). The
   alternative — a second relaxed language per test module — was rejected as the
   duplication the stage brief forbids.

### Open questions for the user

1. **Two multi-token chars nodes keep `Spanned` under `false`** (both listed as
   "checked" per R2, both flagged here): the marker argument's node
   (`MarkerArgumentParser`, `constructs/argument_parsers.rs:999` — a multi-character
   marker is read one `Char` token at a time) and
   the orphan-`\end` recovery node (`latexlike/environments.rs:1066`). Their span is
   their own fact, so the all-trees law holds (residency), and §1.5 R2/R5 leaves them
   alone; but under `OBEYS_SPAN_TILING = false` the content then reads back as the
   *described* span's text rather than as what was consumed, which is exactly the
   inaccuracy R1/R4 exist to avoid. Cheap fixes exist (the marker's text is
   `self.marker` by construction; the recovery node would need the R1 accumulation).
   Should they follow R1/R4, or is "as described" acceptable for a recovery/noise node?
2. **`recover_as_chars` and the other one-token fallbacks record `Spanned`** even though
   the node's span is a reader answer about a token that may have edges in two sources
   at a seam. `SourceSpan::span()` names one range of one source, so residency holds by
   construction; flagging it only because the seam case makes "the token's span" less
   obviously the token's text than it reads.
3. **`docs/panics.md` untouched** (as in Stage 1): no new panicking public item; the
   owned-text accumulation only calls `SourceSpan::content()`, whose bounds come from
   the reader's own answer.

## Stage 3a — scripted reader

Status: **reviewed** (branch `st-3a-scripted`, worktree
`.claude/worktrees/st-3a-scripted`; commits `cd4c638`, `4055974`, plus this file).

### Files changed

- **`techy/src/token/scripted_reader.rs` (new, `cfg(test)`, `pub(crate)`)** — §1.7 in
  full:
  - `ScriptedTokenization` (ZST) with `Token = ScriptedToken<L>` and
    `StreamPosition = ScriptedPosition` — the crate's first tokenization whose types are
    not the standard ones. `make_token_reader` has no script to read (a script is runtime
    data), so it answers a reader over an *empty* stream on the given source: one
    `EndOfStream` token, no content. Documented on the type.
  - `ScriptedPosition` = `(entry index, TokenEdge)` in canonical form. Canonicalization
    folds the place past an entry onto the place before the next one (`(i,
    EndPastPostSpace)` → `(i + 1, StartBeforePreSpace)`, repeated while the entry is
    zero-width) and, within one entry, maps edges that fall on one offset to the earliest
    of them. `(n, StartBeforePreSpace)`, `n` = the number of entries, names the place past
    the last entry. So `==` answers "same place", and clauses 2 and 7 hold at seams by
    construction.
  - `ScriptedToken<L>` carries the entry index, the source index, the standard token the
    scan produced, and `peeked_at` — the position the peek happened at, which the token's
    `StartBeforePreSpace` edge reports (clause 2 then holds for *every* position `peek`
    serves, the fixed-script gaps included).
  - `ScriptedReader<'s, L>` built from segments `&[(&'s Arc<Source<…>>, Range<usize>)]`,
    tokenized at construction with `StdTokenReader::scan_token_at` under the given state;
    one inner `StdTokenReader` per source is kept (deduplicated by `Arc::ptr_eq`) for
    `token_kind` and for the source a span is qualified with. Middle segments' end-of-stream
    tokens are dropped; the last segment's is the final entry (synthesized where the
    segment ends before its source does). Answers per §1.7: `peek` (fixed script),
    `move_to`/`move_to_position` in any direction, canonical `position_at`/`position_here`,
    `source_span_between` (entry's source + edge offsets, either order),
    `source_position_at`, `source_span_within` (`Some` iff ordered, one source throughout,
    boundaries gap-free), `source_span_describing` (the recommended shape).
  - `ScriptedReader::broken_at_seams` — the deliberately broken variant (a flag set by a
    second constructor): `position_at(tok, EndPastPostSpace)` where a seam follows `tok`
    answers the *non-canonical* `(i, EndPastPostSpace)`, so the two sides of the seam are
    two values and the clause-7 corollary fails. The stream it serves is unchanged
    (reading canonicalizes), so Stage 3b can run the same script through both readers.
  - `RelaxedLang` — the §1.7 test language (`Tokenization = ScriptedTokenization`,
    `OBEYS_SPAN_TILING = false`), `pub(crate)` for Stage 3b.
  - 19 unit tests (below).
- **`techy/src/token/mod.rs`** — `#[cfg(test)] mod scripted_reader;` and the `cfg(test)`
  `pub(crate)` re-export of `RelaxedLang`, `ScriptedPosition`, `ScriptedReader`,
  `ScriptedToken`, `ScriptedTokenization` (next to `TokenListReader`'s, with the same
  "internal test infrastructure" note).
- **`techy/src/token/reader.rs`** — the enabling change, behavior-preserving (see
  *Deviations* 1): `peek_impl` became `pub(crate) fn scan_token_at(start, state,
  recovery_for)` and the token interpretation moved from the trait method into the
  inherent `pub(crate) fn token_kind_of`, whose only bound is `L: Lang`.

### Gate results (verbatim, run from the worktree, after the rebase onto `main`)

```
### cargo build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.60s
### cargo test --workspace
test result: ok. 1090 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 86 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 15.51s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
### cargo test --workspace --all-features
test result: ok. 1129 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.50s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 87 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 21.98s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
### clippy
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-3a-scripted/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.69s
### clippy --all-features
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-3a-scripted/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.25s
### docs
 Documenting techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-3a-scripted/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.46s
   Generated /Users/philippe/projects/techy/.claude/worktrees/st-3a-scripted/target/doc/techy/index.html and 1 other file
```

The baseline on `main` (Stages 1 and 4) is 1071 / 1110 lib tests; Stage 3a adds the 19
unit tests of the new module (1090 / 1129) and changes no existing test.
`cargo docs --all-features` emits no warnings. (Note: rustdoc does not build `cfg(test)` code, so the new module's intra-doc
links are not covered by the docs gate.)

### The 19 unit tests

Canonical form and the contract: `consecutive_tokens_meet_at_one_position_across_a_seam`
(clause 7's corollary on a real walk of a splice script — every consecutive pair, seams
included — plus "where the stream stood is where the next peek happened"),
`edges_that_fall_on_one_offset_are_one_position`, `moving_to_a_position_sets_the_position`
(clause 7 at all five edges, both halves), `peeking_twice_at_one_position_reads_one_token`
(clause 1), `un_consuming_a_token_at_a_seam_returns_to_the_trigger`,
`rewinding_into_an_exhausted_segment_reads_it_again` (clause 3),
`end_of_stream_is_terminal_and_idempotent`, `a_chain_of_two_sources_reads_as_one_stream`,
`the_view_of_a_token_comes_from_the_source_it_was_scanned_from` (`token_kind` and spans
across segments), `a_position_reports_the_coordinate_of_the_entry_it_stands_before`.
Spans: `a_span_within_stops_at_a_seam_while_a_described_span_covers_the_chain`,
`a_described_span_of_a_splice_covers_what_the_stream_read_in_that_source`,
`a_hole_in_one_source_has_no_span_within_but_a_described_span`. Gaps, the broken variant
and the scripting errors: `peeking_inside_a_token_yields_the_following_entry`,
`the_broken_variant_reports_the_two_sides_of_a_seam_as_two_positions`,
`the_language_side_reader_serves_an_empty_stream`,
`a_middle_segment_ending_in_whitespace_is_rejected`,
`a_segment_ending_inside_a_token_is_rejected`, `a_position_from_another_script_is_rejected`.

### Decisions taken

- **S1 — the reader is built directly, not through `make_token_reader`** (§1.7 leaves the
  choice open). `Tokenization::make_token_reader` receives only a source and no state, so
  it cannot tokenize a script; it answers a reader over an empty stream, documented on
  `ScriptedTokenization`. Stage 3b builds `ScriptedReader::new(segments, &state)` and
  drives parsers with it through `ParseContext`/`ParserSession`, or hands it out from a
  `ParseDriver::make_token_reader` override whose driver instance holds the pre-built
  reader (the driver has the state at that point, the language does not).
- **S2 — `source_position_at` reports the entry's own coordinate.** §1.7's parenthetical
  ("the coordinate of entry `i` at `StartBeforePreSpace`") is what is implemented: a
  canonical position reports the offset of the entry it stands before, in *that entry's*
  source. At a seam that is the start of the incoming token in the source it comes from.
  The contract's "recommended: the outer/resume coordinate" does not apply literally here:
  the reader's segments are a flat chain, with no outer source to resume into. Documented
  on the method.
- **S3 — a run that ends exactly at a seam has no `source_span_within`.** This follows
  from the canonical form (the place past the last token of one source *is* the place
  before the first token of the next, and that place's coordinate lies in the next
  source), and it is what makes the tiled-language counter-test of Stage 3b T1 fire. Two
  unit tests pin it. A `within` that stays strictly inside one segment still answers its
  exact range.
- **S4 — `RelaxedLang` lives in `scripted_reader.rs`** (`pub(crate)`), the one place a
  language declaring `Tokenization = ScriptedTokenization` can sit next to what it names.
  §1.7 lists it as a Stage 3a/3b language; `RelaxedStdLang` (Stage 1) stays where it is.
- **S5 — panics are the report for a broken script** (§1.7 allows the assert): no
  segments, a range that is not a valid range of its source, content that does not
  tokenize, a segment ending inside a token, a middle segment ending in whitespace, a
  token or position with an out-of-range index. Each is a mistake in the test's own code.
  The module's `# Panics` section says so; no library code is affected.

### Deviations from §1

1. **`techy/src/token/reader.rs` was changed** (§4 lists only `scripted_reader.rs` and
   `token/mod.rs`). It had to be: `StdTokenReader`'s scan and its token interpretation
   were reachable only through bounds that a language with its own tokenization cannot
   satisfy (`L::Tokenization: Tokenization<L, Token = StdToken<L>, StreamPosition =
   StdStreamPosition>` — the very thing §1.7 asks the scripted reader *not* to be), so
   §1.7's "tokenized with `StdTokenReader`" and §9's "delegate `token_kind` to the
   per-source inner reader" were not compilable as they stood. The change is
   behavior-preserving and small: `peek_impl` → `pub(crate) scan_token_at(start, state,
   recovery_for)` (the scan offset is a parameter, and the one step needing the language's
   own token/position types — building the `TokenRecovery` of a recoverable failure — is a
   hook; `peek` passes its own position and the standard recovery), the same bound dropped
   from the four scan helpers, and `token_kind`'s body moved into the inherent
   `token_kind_of` with `L: Lang` as its only bound (the trait method delegates). The
   existing suites are unchanged and pass (commit `cd4c638` alone: 1067 lib tests, as
   before).
2. **`ScriptedToken` carries `peeked_at`** beyond §1.7's "entry index plus whatever the
   reader needs". It is what keeps clause 2 exact where a peek happens at a position that
   is not an entry's start: at `Start` (the pre-space already passed — a very common
   parser move) the entry is served with its pre-space clipped, and at a position inside a
   token proper (`ContentStart`/`End`, the `\verb` idiom) the *following* entry is served,
   as `TokenListReader` does. In both cases the token reports the peek position as its
   `StartBeforePreSpace` edge, so the fixed-script fidelity gap never becomes a contract
   violation.
3. **A `ScriptSegment<'s, L>` type alias** for the segment tuple, because
   `clippy::type_complexity` rejects the bare tuple in a signature. Private to the module;
   tests write the tuples literally.

### Open questions — answered at review

1. **Does Stage 3b need the script reachable from `Language::parse`?** — **No.** The
   reviewer's finding: T1–T12 are all reachable the way the two-reader agreement suites
   drive parsers, through `ParseContext`/`ParserSession` over a directly built
   `ScriptedReader` (T11 needs a session so that its report can be rendered). Should a
   full-engine route ever be wanted, it is a `ParseDriver::make_token_reader` override on
   a driver holding the script and the seed state — but `Language<L>` holds `L::Driver`
   and `RelaxedLang::Driver = StdParseDriver`, so that route needs either a driver change
   on `RelaxedLang` or a second test language. Nothing here blocks Stage 3b.
2. **`source_position_at` at a seam** — **confirmed as built** (S2): the incoming entry's
   own coordinate, since the scripted reader's segments are a flat chain with no outer
   source to resume into. Now stated in the module's *Positions* section as well as on the
   method.

### Review

Stage 3a reviewed: **PASS**, no blocking fixes; the reviewer endorsed the seam semantics
(S3 — `within` answers by the end position's source; keep it) and the `reader.rs`
refactor. Suggestions applied: `mod scripted_reader;` moved into `token/mod.rs`'s
alphabetical order; `source_span_within`'s doc now spells out the ends-at-a-seam case and
why answering the outgoing source would contradict `source_position_at`; the module's
*Positions* section now states what `source_position_at` answers at a seam; the two open
questions above answered. Rebased onto `main` (`fae69fa`, Stages 1 and 4).

## Stage 4 — consumers

Status: **reviewed** (branch `st-4-consumers`, worktree
`.claude/worktrees/st-4-consumers`; commits `51f05ad`, `57060ca`, `25bc44c`,
`a169ce3`, `17f2ac9`, `bdecb5c`, plus this file).

Review **PASS**, no blocking fixes; the four non-blocking suggestions are applied
(`node_at` and the shared descent carry `covering_slice`'s conditional; the split
examples name `content_as_chars` as the content reader beside `source_text`'s
coordinates; the three byte-exactness claims in `docs/custom-lang.md`,
`docs/ai-guide-trees.md` and `docs/learn-by-example.md` name span tiling; the
`false` case in `latexlike/recompose.rs` is phrased as the const's definition has
it — parsers assume nothing about where tokens come from — instead of "several
sources"). Rebased onto `main` at `d525660` (Stage 1 merged, including its own
wording commit); the rebase was clean, and `cargo test --workspace` and
`cargo docs --all-features` were re-run after it (results below).

### Files changed

- `techy/src/node/slice.rs` — module docs and `NodeSlice::span`/`source_text`: the
  covering span is exact on a tree parsed from a language that obeys span tiling
  (sibling spans are adjacent there); for a language with `OBEYS_SPAN_TILING = false`,
  and on restaged or synthesized trees, it is the covering span of the recorded
  coordinates and may include bytes no node of the run claims. `None` across sources
  as before, and the "a parsed tree's sibling runs always answer" sentence now names
  the condition. Both docs point at node data for content.
- `techy/src/node/node_ref.rs` — `span` (what the coordinates mean under each
  declaration) and `span_content` (it answers the coordinates, not the node's data;
  the two agree under span tiling; content accessors and `recompose` named as the
  alternatives).
- `techy/src/node/tree.rs` — `covering_slice` (children that do not tile the queried
  stretch: restaged trees *and* parses of a language with `OBEYS_SPAN_TILING = false`),
  and the private `covering_child_run`/`binary_candidate_run`/`verify_covering_run`
  docs plus the run-adjacency comment (no "gap-free" as a contract name).
- `techy/src/node/kind.rs` — the `CallableData::invocation_syntax` illustration: the
  preset's recorded post-space is a sub-range of the node's span *under span tiling*.
- `techy/src/extract.rs` — module docs (partials cut from owned content named for all
  three producers of owned content; "parse-tree byte accounting" → "the byte accounting
  of span tiling"; a new paragraph stating that every helper reads node data and never
  the text a span points at), `piece_span` and `split_at_chars` doc precision, and four
  audit tests over a new `owned_tree` fixture (hand-built through `NodeTreeBuilder` with
  `NodeKind::chars(TextContent::Owned(..))` under a stub source that spells
  `<generated>`, so a helper reading content through a span cannot pass by accident).
- `techy/src/recompose/mod.rs` — the reading contract gains the `OBEYS_SPAN_TILING =
  false` line: a source reemitter reemits the tree as stored and claims no
  byte-equality with any one source.
- `techy/src/latexlike/recompose.rs` — `SourceRecomposer`'s accuracy paragraph: the
  byte-exactness claim is stated for a language that obeys span tiling (the preset
  included); a family member declaring `false` is reemitted as stored.
- `techy/src/node/mod.rs`, `techy/src/engine/language.rs` — one test comment each,
  wording sweep only ("partition invariant" as a name).
- `docs/learn-by-example.md`, `docs/node-trees.md`, `docs/ai-guide-trees.md` — wording
  sweep and the conditional on the exactness / byte-exact-reemission claims.

Not touched, deliberately: `techy/src/node/invariants.rs` and
`techy/src/latexlike/invariants.rs` (Stage 2 R6 gates the oracle *and* renames its docs
to "the span-tiling law" — editing the same lines here would only conflict),
`constructs/`, `token/`, `dev-docs/ARCHITECTURE.md`, `dev-docs/DESIGN_RATIONALE.md`.

### Extract audit (§1.6, PLAN §6)

Method: read the documented contract of every public item of `techy::extract`, run its
doctests (`cargo test --doc extract` — 2 passed: the module example and
`split_at_chars`), inspect every code path that pattern-matches `TextContent` for what
it does with `Owned` input, and add a unit test feeding owned chars content through the
public entry points. **Result: no code change was needed** — every documented answer
holds for `Owned` content, because every helper reads node data
(`NodeRef::chars` → `TextContent::resolve` against the node's own source). Three doc
imprecisions were fixed (below).

| Item | What the doc promises | What the code does for `Owned` | Evidence |
|---|---|---|---|
| `content_as_chars` | chars text, comments skipped, groups/lists recursed; `Cow::Borrowed` for a single contiguous piece | `piece_text` → `NodeRef::chars()`, which resolves owned text borrowed from the node; the accumulator keeps the borrow | new `content_as_chars_reads_owned_chars_content` (borrowed answer asserted; the node's span spells `<generated>`), module doctest |
| `split_at_chars` (+ `_drop_annotations`, `_keep_annotations`) | segments at top-level separators; boundary partials are fresh `Chars` nodes; a partial of owned content keeps the whole original node's span | `stage_piece` (~519) has an explicit `Owned` arm: owned sub-text via `text.get(sub)`, node span unnarrowed; `piece_span` (~207) narrows only `Spanned` | new `split_at_chars_cuts_owned_content_and_keeps_node_provenance`; existing `split_partials_of_owned_content_keep_whole_node_provenance` |
| `SplitAtChars::{len,is_empty,segment,segments,tree,into_tree}` | segment views over a derived tree whose sibling spans do not tile | index-only; content storage is irrelevant | same test (3 segments over mixed owned/copied input) |
| `SplitAtCharsPart` / `KeyValsPart` (`original`, `is_partial`, `partial_text`, index) | the cut piece's text and its original node | `partial_text` is `piece_text` — node data | same test (minted `Some("k1")` from owned content) |
| `parse_keyval` (+2) | keys flattened and trimmed, values raw in source order, `key=` vs `key` | keys through `pieces_as_chars`, values through `stage_piece`'s `Owned` arm | new `keyval_reads_owned_content` |
| `KeyVals::{len,is_empty,keyval,get,iter,tree,into_tree}` | entry table access, last-wins `get` | table lookups, content-independent | same test |
| `KeyVals::get_combined_with` | every occurrence concatenated with a synthesized separator node | `copy_subtree_into` clones the kind as stored (owned stays owned); the separator is already `Owned` | same test (`"1;2"`) |
| `KeyValEntry::{key,value,value_content}` | raw value; the lone-group unwrap | structural (`is_group` + `children()`) | same test (`legend` → `"a,b"`) |
| `split_embellishments` (+2) | one entry per group, marker as key, noise skipped | `is_run_noise` reads `chars()` (owned whitespace is noise); values are whole-node copies | new `run_readers_read_owned_content` |
| `split_tack_on_fields` (+2) | one entry per field invocation, recorded name as key, argument content as value | `node.name()`/`argument_content_nodes` + whole-node copies | same test |
| `ExtractError` (+ `Display`, `Error`) | error shapes | unchanged by content storage | existing tests |

Non-obvious paths checked while auditing: `covering` (~383) falls back to `first` alone
across sources or for an inverted pair, and `anchor` (~362) falls back to the first
node's own span when the slice has no single-source covering span — so a run whose
nodes sit in different sources (possible under `OBEYS_SPAN_TILING = false`) never
produces a bogus span and never panics. The `expect`s inside `piece_text`/`stage_piece`
are on sub-ranges `split_pieces` computed from *that same* logical text, so they are
char-boundary-sound for owned and span-backed content alike (no new panic risk).

Doc fixes made (never "best effort"): (a) `piece_span` said "best available
provenance" — it now states the fact (the whole node's span, unnarrowed, because owned
text has no byte mapping into the source to subdivide); (b) the module docs said
partials are "span-backed into the *same* source (exact sub-spans, zero-copy text)"
without qualification, and named materialized trees as the only source of owned
content — both now state the two arms and all three producers of owned content
(materialize, a transform pass, a parse with `OBEYS_SPAN_TILING = false`); (c)
`split_at_chars`'s "fresh boundary partials with exact sub-spans" is now conditional.

### `serialize` / `transform` / `visit` (§1.6: no change expected)

Read; **no tiling claim found**, nothing edited.

- `serialize`: `serialize/mod.rs` and the tree driver make no span-tiling claim. The
  tree driver's "a region that does not tile the child list"
  (`serialize/drivers/tree.rs:14`) is *index* tiling — the all-trees law, unchanged by
  this plan — and the same file records that `TextContent` is "owned text only" on the
  wire, so owned content round-trips by construction.
- `transform`: "verbatim" throughout means *copied node*, not source bytes; the restage
  driver already treats trees as untiled.
- `visit`: purely structural; the one "contiguous runs" mention
  (`visit.rs:357`) is about slot regions in the child list.

### Grep gates

- Consumers reading content through a node span (checklist item (d)):
  `grep -rn "span_content()\|source_text()\|\.span()\.content()\|span()\.source()\.content()" techy/src techy/tests`
  — outside `#[cfg(test)]` modules and doc examples, the only hits are the coordinate
  accessors themselves (`node/slice.rs`, `node/node_ref.rs`). No recompose, extract,
  transform, visit, or serialize code path reads text through a span.
- Superseded phrases (§1.8) in this stage's files: none left (verified with
  `grep -rni "partition invariant\|gap.free\|parse.tree law\|span-contiguous"` over
  `node/slice.rs`, `node/node_ref.rs`, `node/tree.rs`, `node/mod.rs`, `extract.rs`,
  `recompose/`, `latexlike/recompose.rs`, `visit.rs`, `transform/`, `serialize/`,
  `engine/language.rs`, `docs/`).

### Wording sweep — every occurrence found

| Occurrence | Phrase | Action |
|---|---|---|
| `node/slice.rs:12,120` | "partition invariant", "span-contiguous" | rewritten (span tiling, conditional) |
| `node/tree.rs:557,584` | "gap-free tiling" | rewritten ("adjacent spans") |
| `node/mod.rs:784` | "partition invariant" (test comment) | rewritten ("span tiling") |
| `engine/language.rs:475` | "the root partition invariant" (test comment) | rewritten ("the root's span tiling") |
| `extract.rs:30` | "parse-tree byte accounting" | rewritten ("the byte accounting of span tiling") |
| `docs/learn-by-example.md:75` | "the tree's span partition invariant" | rewritten (span tiling, preset named) |
| `docs/node-trees.md:23`, `docs/ai-guide-trees.md:13,56,233` | "exact byte range", "byte-exact for parsed trees" | conditional added |
| `docs/custom-lang.md:376` | "byte-exact re-emission is possible exactly to the extent the language records spelling facts here" | conditional added (review suggestion 3) |
| `docs/ai-guide-trees.md:259`, `docs/learn-by-example.md:697` | "byte-exact for parsed trees" (doctest comments) | rewritten ("trees parsed from a language that obeys span tiling") |
| `techy/src/extract.rs:92` doctest, `docs/learn-by-example.md:636`, `docs/ai-guide-trees.md:133`, `docs/ai-guide.md:155` | `segment().source_text()` taught as the segment reader | caveat added pointing at `content_as_chars` (review suggestion 2) |
| `token/reader.rs:2641` | "partition invariant" (test comment) | **left** — `token/` is Stage 1's file (see open question 3) |
| `constructs/nodes_parser.rs:38,54,590,621,636,682,1653,1794`, `constructs/verbatim_parser.rs:37`, `constructs/environment_parser.rs:700` | "partition invariant", "in-order, gap-free token contract" | **left for Stage 2** (R1/R6 rewrite these) |
| `node/invariants.rs:4,7,11,41,557,821,832,1213`, `latexlike/invariants.rs:1,31` | "parse-tree law", "partition invariant" (message text + test) | **left for Stage 2** (R6 renames and gates the oracle) |
| `dev-docs/ARCHITECTURE.md:536`, `dev-docs/DESIGN_RATIONALE.md:992,2144,2284,2417,2555,2682,2693,4578,7196` | "partition invariant", "parse-tree law" | **left for Stage 5** |
| `TODO_Big.md:17` | "Gap-free chars-run contract: relax it …" | **left** — the tracker item this project implements (see open question 3) |
| `dev-docs/archive/*`, `dev-docs/bettertokens/*` | various | **left** — historical documents |

`span-contiguous` is *not* a superseded phrase: it is part of the span-tiling
definition (`Lang::OBEYS_SPAN_TILING`) and stays in `node/invariants.rs`,
`constructs/invocation_parser.rs`, `latexlike/environments.rs`.

### Gate results (verbatim, run from the worktree)

```
### cargo build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.03s
### cargo test --workspace
test result: ok. 1071 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 86 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 13.40s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
### cargo test --workspace --all-features
test result: ok. 1110 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.53s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 87 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 21.66s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
### cargo clippy --workspace --all-targets -- -D warnings
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-4-consumers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.25s
### cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-4-consumers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.88s
### rm -rf target/doc && cargo docs --all-features
    Checking serde v1.0.229
 Documenting techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-4-consumers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.75s
   Generated /Users/philippe/projects/techy/.claude/worktrees/st-4-consumers/target/doc/techy/index.html and 1 other file
```

Stage 1's base ran 1067 / 1106 lib tests; Stage 4 adds exactly the 4 audit tests
(`content_as_chars_reads_owned_chars_content`,
`split_at_chars_cuts_owned_content_and_keeps_node_provenance`,
`keyval_reads_owned_content`, `run_readers_read_owned_content`). `cargo docs
--all-features` emits no warnings — no broken intra-doc links among the new
cross-references.

### Deviations from §1.6

1. **`node/invariants.rs` left untouched** (the prompt allowed docs-only edits there if
   the sweep required it). Stage 2's R6 renames exactly those doc lines *and* gates the
   oracle; two branches editing the same lines would only produce a conflict. Its
   "parse-tree law" / "partition invariant" occurrences are listed above as Stage 2's.
2. **Two files outside the stage's list were swept** — one test comment each in
   `node/mod.rs` and `engine/language.rs` (both said "partition invariant"). No other
   stage owns them, and the phrase is superseded; the edits are comment-only.
3. **`node/kind.rs` doc line added** (not in the stage's file list): the
   `CallableData::invocation_syntax` docs illustrate the preset's post-space payload as
   "a sub-range of the node's span", which is a span-tiling fact. The illustration is
   now conditional. Stage 2's R3 touches `latexlike/invocation_syntax.rs`, not this
   file.
4. **The audit fixture is hand-built as §6 asks, and the audit additionally uses
   `materialize()`** for the *copied* parts (groups, a macro invocation), so that the
   copied subtrees carry owned payloads too without hand-building groups and callables.
5. **No new item-level doc was added for `SplitAtChars::segment`**: the segment
   accessors make no exactness claim of their own, and the two rules that matter (the
   partial-provenance rule and "helpers read node data") are in the module docs, which
   every producer's docs already point at.

### Open questions for the user

1. **`source_text()` on a segment cut from owned content.** The documented (and now
   tested) answer is the whole *original* node's span text — for the fixture,
   `"<generated>"` rather than the segment's `"k1"`. That is exactly what §1.6's
   consumers rule prescribes (the coordinate accessors answer what the coordinates
   say), and content is available through `chars()`/`content_as_chars`. Confirm this is
   the intended long-term answer, rather than recording an empty span (or no span) for
   a partial of owned content.
2. **How far the conditional should go in the user guide.** The guide chapters describe
   the preset, which obeys span tiling; Stage 4 added the condition only where a claim
   would otherwise be wrong for another language (`node-trees.md`, `ai-guide-trees.md`,
   `learn-by-example.md`). The remaining chapters keep unconditional preset-flavored
   wording. Say if a broader pass is wanted (`parsing-model.md`, `pylatexenc-migration.md`).
3. **Two stragglers outside every stage's file list**: `token/reader.rs:2641` (a test
   comment saying "the tolerant parse keeps the partition invariant" — `token/` was
   Stage 1's and is Stage 3a's file) and `TODO_Big.md:17` ("Gap-free chars-run contract:
   relax it for a reader that serves one parse from several sources" — the tracker item
   this project implements, presumably retired in Stage 5). Both left untouched;
   orchestrator's call.

## Stage 3b — tests

## Stage 5 — record

## Orchestrator log

- 2026-08-19: plan written and committed to `main`.
- 2026-08-19: Stage 3a implemented on `st-3a-scripted` (see its section).
