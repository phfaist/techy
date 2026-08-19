# Span tiling — progress

Plan: `dev-docs/spantiling/PLAN.md`. Protocol: PLAN §8. Every subagent runs on Opus.

| Stage | Branch | Base | Worktree | Status |
|---|---|---|---|---|
| 1 contract surface | `st-1-contract` | `main` | `.claude/worktrees/st-1-contract` | merged |
| 2 parsers | `st-2-parsers` | `st-1-contract` | `.claude/worktrees/st-2-parsers` | merged |
| 3a scripted reader | `st-3a-scripted` | `st-1-contract` | `.claude/worktrees/st-3a-scripted` | merged |
| 4 consumers | `st-4-consumers` | `st-1-contract` | `.claude/worktrees/st-4-consumers` | merged |
| 3b tests | `st-3b-tests` | `main` (after 2, 3a) | `.claude/worktrees/st-3b-tests` | merged |
| 5 record | `st-5-record` | `main` (after all) | `.claude/worktrees/st-5-record` | reviewed |

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

Status: **reviewed** (branch `st-2-parsers`, worktree `.claude/worktrees/st-2-parsers`,
rebased onto `main` at `610c3d5` — Stages 1, 3a and 4 merged; commits `41aba93`,
`66a4058`, `22c275d`, `e7c7e8b`, `28b1ae6`, `3f303e8`, `1d41f4f`, `6e0b839`, `d71fe82`,
plus this file's).

Review PASS with two required fixes, both applied in `d71fe82` (the embellishment
wrapper's node span, the verbatim content's emptiness gate), together with the four
non-blocking suggestions: the `read_name_chars` comment, the superseded "parse law"
wording in this stage's files, two more tests (the latexlike oracle's gate, the
verbatim nested-close and trailing arms), and the PROGRESS corrections below.

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
- `techy/src/constructs/verbatim_parser.rs` (**R4**; emptiness gate from the Stage 2
  review) — `read_raw_content` accumulates
  the content's text under `!L::OBEYS_SPAN_TILING`: per consumed token the R1 recipe
  (pre-space + spelling + post-space), including a nested close read as content by the
  pairing rule, plus the pre-space of the terminator / end-of-stream token (which lies
  before `content_end` and is content). `RawContentEnd` carries it as `content_text`,
  and `raw_content_text` answers the node data at both content sites (the delimited
  argument and the environment body) — as an `Option`, because *whether there is
  content* is decided the same way as the content itself: the span's emptiness under
  `true` (bit-identical behavior), the accumulated text's emptiness under `false`, where
  the span is only a description and could be empty over real content or non-empty over
  none. A parse whose content starts at a seam is where that matters; the reader for
  such a test is Stage 3a's scripted reader, so the test belongs in Stage 3b. Module docs state the two representations; the
  "partition invariants" phrase is gone.
- `techy/src/constructs/argument_parsers.rs` (**R1/R4 by the user's ruling**) —
  `MarkerArgumentParser`'s node covers one token per marker character; under `false` it
  records `TextContent::Owned(self.marker)`, which is exactly what was read (every token
  was checked to spell the expected character, consecutively). Unchanged under `true`.
- `techy/src/constructs/embellishments_parser.rs` (**Stage 2 review, required fix 1**)
  — the embellishment wrapper's span (see the R5 correction below).
- `techy/src/constructs/environment_parser.rs` (**the name-as-read rule**, user ruling
  2026-08-19) — `read_name_chars` accumulates the name's characters as it reads them
  under `false` (the shared `push_token_text` recipe; the rigid check proves each
  token's pre-space empty). `NameGroup` keeps `name: SourceSpan` as the coordinates and
  gains a **private** `name_as_read: Option<Box<str>>` plus `NameGroup::new`,
  `with_name_as_read` and `name_text()` — the text is read through `name_text()`, which
  answers the characters as read where there are any and the span's content otherwise.
  The private field makes the constructors the only way in; the terminator match in
  this file reads `name_text()`. Also wording ("the gap-free tiling
  contract" → "span tiling, where the language obeys it").
- `techy/src/constructs/verbatim_parser.rs` (**the name-as-read rule**) — the composed
  `StopEnvironmentCommand` terminator's `NameGroup` is built through `NameGroup::new`
  (the only other in-crate construction site). Its pieces are sliced from the matched
  terminator's own single-token span, so the span's content *is* the name under either
  declaration — no name-as-read needed, stated in a comment.
- `techy/src/latexlike/input.rs` (**the name-as-read rule**) — `\input`'s reference is
  the other text that drives a lookup. Under `false` it comes from the staged
  argument's **node data** (the new `argument_text`, folding the content nodes' chars
  payloads) instead of the argument's extent span; under `true` the span path is
  unchanged. `None` for content that is not plain characters — node data has no single
  text there, and a span would be a guess.
- `techy/src/latexlike/environments.rs` (**R1/R4 by the user's ruling**) — the
  orphan-`\end` recovery node covers the trigger and, when one was read, the name group;
  under `false` its content is assembled from what the site has in hand about each piece
  — the trigger's own span (one reader answer about one token, taken from its `Start` so
  the pre-space the content loop already staged stays out) plus the name group's
  delimiters as written (the rule cloned off the matched open token) around the name.
  Unchanged under `true` — except that the *quoted terminator* of the diagnostic is now
  assembled from the same pieces in both cases rather than sliced from the span; for a
  tiled parse the two are the same string, which the existing tiled orphan tests pin
  (`orphan ‘\end{itemize}’`, `orphan ‘\end’`). The name part of that text, the
  `OrphanEnd` condition's name
  and the begin side's lookup name all read `NameGroup::name_text()`; the diagnostic's
  quoted terminator is assembled by the same closure as the recovery text (one edge
  parameter apart — without a name group the quote stops at the command word), so the
  diagnostic quotes what was read.
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

After the rebase onto the Stage 1 review's wording fixes (`d525660`), one follow-up
commit (`f2e6cfa`) generalizes the owned-content docs the same way the const doc was
generalized: multi-token content is owned because the tokens need not form one
contiguous stretch of one source, of which a seam between two sources is one example.

### R2 — "node span == fact span, bare `Spanned` kept" (checked list)

Complete against `grep -n "NodeKind::chars(\|TextContent::Spanned("` over
`techy/src/constructs/` and `techy/src/latexlike/`, test modules excluded:

| Site | What | Verdict |
|---|---|---|
| `constructs/attached_source.rs:192` | the attached-source placeholder chars node | node span == fact span → `Spanned` kept |
| `constructs/argument_parsers.rs:207` | `stage_pre_space`: a pre-space-only chars node | node span == fact span → `Spanned` kept |
| `constructs/argument_parsers.rs:263` | expression-position chars over one token | node span == fact span → `Spanned` kept |
| `constructs/argument_parsers.rs:307`, `:318` | unresolvable / failed command fallback over one token | node span == fact span → `Spanned` kept |
| `constructs/argument_parsers.rs:1010` | `MarkerArgumentParser`'s chars node (**several tokens**) | `Owned` under `false` (user ruling) |
| `constructs/nodes_parser.rs:908` | `recover_as_chars` over one token's span | node span == fact span → `Spanned` kept |
| `constructs/verbatim_parser.rs:785` | the gobbled leading newline (one token) | node span == fact span → `Spanned` kept |
| `latexlike/driver.rs:267` | the paragraph-break `Chars` shape over the break span | node span == fact span → `Spanned` kept |
| `latexlike/environments.rs:862` | malformed-`\begin` chars fallback over the trigger | node span == fact span → `Spanned` kept |
| `latexlike/environments.rs:1097` | orphan-`\end` recovery chars (**several tokens**) | `Owned` under `false` (user ruling) |
| `constructs/environment_parser.rs:992`, `:1194` | inside `mod tests` (a test language's raw-body parser) | test code; goes through the dispatch already |
| `constructs/mod.rs:142` | `node_text_content` itself | the rule |
| `constructs/nodes_parser.rs:620`, `verbatim_parser.rs:177`, `latexlike/invocation_syntax.rs:148` | the three sites this stage changed (R1, R4, R3) | dispatch on the const |

**Multi-token sweep (user ruling, 2026-08-19).** `NodeKind::chars(<span>.span())` was
re-grepped crate-wide for a node covering more than one token — the test is where the
span comes from: two stream positions (`cx.source_span_within`) rather than one token's
`source_span_of`/`source_span_between`. Outside test modules there are none left: the
two sites above now record text, the chars run (R1) and the two verbatim sites (R4)
already did, and every other production site takes one token's span —
`engine/language.rs:246` and `constructs/attached_source.rs:192` (the stop cause's span,
the close delimiter as matched), `scopes/mod.rs:1637` and `latexlike/environments.rs:862`
(the trigger), `latexlike/driver.rs:267` (the paragraph break),
`argument_parsers.rs:207/263/307/318`, `nodes_parser.rs:908`, `verbatim_parser.rs:785`.

**Text-from-coordinates sweep (user ruling, 2026-08-19).** Every `.content()` in
production parser code (`constructs/`, `latexlike/`, `engine/`, `scopes/`, test modules
excluded) classified by how many tokens its span covers:

| Site | Span | Verdict |
|---|---|---|
| `constructs/environment_parser.rs` `read_name_chars` | several `Char` tokens | **fixed**: characters accumulated, `name_text()` |
| `latexlike/environments.rs` orphan `\end` (quote + recovery text) | trigger + name group | **fixed**: assembled per token |
| `latexlike/input.rs:327` `\input` reference | the argument's content nodes | **fixed**: node data under `false` |
| `constructs/mod.rs:143` `node_text_content` | the fact handed in — one token's span at every call site | exact |
| `constructs/mod.rs:160,187` `push_pre_space_text`/`push_token_text` | one token's sub-span | exact (they *are* the recipe) |
| `constructs/environment_parser.rs:151` `name_text()`'s fallback | the name span, only when no text was recorded | exact by construction |
| `constructs/attached_source.rs:187`, `engine/language.rs:239` | the stop cause's span (one token) | exact |
| `latexlike/environments.rs:855` `MalformedBegin` | the trigger command (one token) | exact |
| `latexlike/driver.rs:275` paragraph-break specials name | the break token's span | exact |
| `engine/driver.rs:334` the default paragraph-break node | the break token's span | exact |
| `latexlike/invocation_syntax.rs:149` macro post-space | one token's sub-span | exact (R3) |
| `engine/mod.rs:104,107` frame titles | **the name-group span** for both variants at the environment sites | see the open question |

`extract`, `recompose`, `node/` were left alone: they are consumers, and §1.6 gives them
to Stage 4.

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
delegating lines of the five test readers: every node/body/name span goes through
`cx.source_span_within` / `invocation_span_within`.

**Correction (Stage 2 review).** That grep does not catch a node span built with
`SourceSpan::new` out of a staged child's coordinates, and two sites did exactly that:
`stage_invocation`'s no-explicit-end arm (carry-over A, fixed earlier) and the
embellishment wrapper (`constructs/embellishments_parser.rs`), which took
`marker_span.start()..child.end()` after filtering the child on `same_source`. The
wrapper now mirrors carry-over A: under `false` its span is what the reader describes
for the stretch from the marker's start position to where the stream stands (just past
the expression), through `cx.source_span_within`; the tiled arm is the previous code,
textually unchanged, inside the `true` arm. `grep -rn "SourceSpan::new" techy/src`
outside `token/`, `source/` and test modules now shows no node-span construction from
another node's coordinates.

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
- `constructs/nodes_parser.rs`:
  `a_marker_argument_owns_its_text_where_the_language_does_not_obey_span_tiling` — a
  `\opt**` marker argument under both declarations: same structure and same node span,
  `Owned("**")` against `Spanned`.
- `latexlike/environments.rs`:
  `the_orphan_end_recovery_owns_its_text_where_the_language_does_not_obey_span_tiling` —
  `"a\end{itemize}b"` and `"\end x"` (the name-group arm and the malformed arm) under
  `Latexlike` and under the new `RelaxedLatexlike`: same recovered text, `Owned` against
  `Spanned`, `check_latexlike_tree_invariants` and `validate_tree` OK.

- `constructs/environment_parser.rs`: `a_name_group_answers_the_name_as_read` — the
  accessor's contract directly: recorded characters win over a span that covers
  something else, and the span stays the coordinates.
- `latexlike/environments.rs`:
  `an_environment_name_is_read_exactly_where_the_language_does_not_obey_span_tiling` —
  `\begin{itemize}x\end{itemize}` under `Latexlike` and `RelaxedLatexlike`: the lookup
  resolves (no diagnostics), same node `name`, same span, same shape. Over the standard
  reader the described span happens to be the exact range, so what this pins is the
  accumulation path — the name the lookup and the node see is the one `read_name_chars`
  collected; a reader whose description disagrees is Stage 3b's scripted reader.
- the orphan-`\end` test now also asserts the rendered diagnostic quotes the terminator
  as read (`orphan ‘\end{itemize}’`, and `orphan ‘\end’` for the malformed arm).

- `latexlike/invariants.rs`:
  `a_language_that_does_not_obey_span_tiling_skips_the_payload_pins` — the counterpart
  of the core gate's test: a payload whose escape character and spelling are nowhere in
  the node's bytes (the tree `rejects_a_macro_escape_char_not_in_the_bytes` panics on)
  passes the oracle under `RelaxedLatexlike`, while `validate_tree` still holds.
- `constructs/verbatim_parser.rs`:
  `relaxed_verbatim_content_covers_the_nested_close_and_trailing_arms` — `\verb{a{b}c}`
  (a nested close read as content by the pairing rule, so the loop's group-close arm
  contributes its spelling) and `\verb|ab  |` (characters up to the delimiter): the
  owned text equals the tiled parse's slice in both.

`RelaxedLatexlike` (test-only, now in `latexlike/test_support.rs` — the preset's shared
`#[cfg(test)]` helper module, so `environments.rs` and `invariants.rs` share one
definition) is a
latexlike-family language with the preset's vocabularies and seed and
`OBEYS_SPAN_TILING = false`. It is **R7's concrete demonstration**: the preset's generic
parsers, driver and node-ext types serve a non-tiled family member unchanged. Stage 3b's
`LatexlikeLang` over `ScriptedTokenization` can build on it.

Wording: the superseded "parse law"/"parse-tree law" phrasing is gone from every file
this stage touched (`latexlike/invariants.rs`, `latexlike/mod.rs`,
`latexlike/invocation_syntax.rs`, `latexlike/input.rs`, `node/invariants.rs`). After the
rebase it survives at five in-code comments in `node/mod.rs` (64, 71, 2169),
`node/arguments.rs` (343) and `node/builder.rs` (655) — Stage 4's files, already merged
— left for Stage 5's sweep.

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
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.03s

### cargo test --workspace
test result: ok. 1102 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.64s
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
test result: ok. 1141 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.51s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 87 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 22.13s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

### cargo clippy --workspace --all-targets -- -D warnings
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.05s

### cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.33s

### rm -rf target/doc && cargo docs --all-features
 Documenting techy-derive v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy-derive)
 Documenting techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.34s
   Generated /Users/philippe/projects/techy/.claude/worktrees/st-2-parsers/target/doc/techy/index.html and 1 other file
```

Baseline: `main` at `610c3d5` (Stages 1, 3a and 4) runs 1090 / 1129 lib tests; Stage 2
adds 12 — 1102 / 1141. No existing test changed its
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

### Public API breaks (for the Stage 5 record)

Two, both deliberate, both of the shape `cargo-semver-checks` names:

1. `TokenReader::source_span_describing` is a **required** trait method (Stage 1, §1.3:
   a missing implementation must be a compile error, never a misleading span).
2. `NameGroup` gains a **private** field
   (`constructible_struct_adds_private_field`), so it is no longer constructible by
   struct literal outside the crate: `NameGroup::new` plus `with_name_as_read` replace
   that. The pairing of coordinates and text is exactly what must not be forgeable — a
   span paired with text that disagrees with it would defeat the rule the field exists
   for.

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
3. **No `\begin…\end` environment parse under a relaxed preset language.** §3 step 8
   makes it optional ("if cheap"), and a `LatexlikeLang` over a multi-source reader is
   Stage 3b's subject. The user's ruling did make a relaxed family member necessary for
   the orphan-`\end` site, so `RelaxedLatexlike` now exists (test-only, over
   `StdTokenization`) and covers that path; a full environment body under it is left to
   Stage 3b, which has the reader to make it meaningful. The relaxed *verbatim* body
   recipe is covered through `VerbatimArgumentParser`.
4. **Test-language sharing changed `constructs::tests` to `pub(crate) mod tests`** and
   gave `RelaxedStdLang` a scope-stack command resolver (see "Tests added"). The
   alternative — a second relaxed language per test module — was rejected as the
   duplication the stage brief forbids.

### Open questions for the user

1. **Answered (user, 2026-08-19): follow R1/R4** — under `OBEYS_SPAN_TILING = false` a
   multi-token `Chars` node must not record `Spanned` content, recovery and noise nodes
   included ("as described" text is exactly the inaccuracy R1/R4 exist to avoid). Both
   sites now record text (see "Files changed" and the sweep above), with a test each;
   the crate-wide re-grep found no third site outside test modules.

   The **residual** this raised — the name group's own `name` span being multi-token —
   was ruled on the same day: the name must be exact, since it drives the lookup, the
   node data and the diagnostics. Implemented as the name-as-read rule (see "Files
   changed" and the text-from-coordinates sweep): `NameGroup::name_text()`, the
   accumulation in `read_name_chars`, the assembled orphan quote, and `\input`'s
   reference from node data.

   **New, reported not fixed — for the Stage 5 record.** A frame's title renders a span
   as text, and the environment sites hand it a *multi-token* one, so under `false` a
   traceback frame can quote the wrong text. Both variants are affected:
   `FrameTitle::Quoted { label, name }` (`engine/mod.rs:104`) — fed the name-group span
   by `environment_parser`'s `with_invocation_name_span` and `latexlike/environments`'
   `name_span` — and `FrameTitle::Callable { spec, role, name }` (`engine/mod.rs:107`) —
   fed the same span by `parse_declared_arguments`
   (`constructs/invocation_parser.rs:60`), which is where every declared argument's
   frame title comes from. (`FrameTitle::Callable` is exact at the macro sites, which
   pass one token's span.) It is diagnostic decoration — no lookup, no node data — and
   fixing it changes the public `FrameTitle` (a text field beside the anchor span, or a
   `TextContent`), so it is a decision for the user rather than a Stage 2 fix.
2. **Confirmed (user, 2026-08-19).** `recover_as_chars` and the other one-token
   fallbacks record `Spanned` even though
   the node's span is a reader answer about a token that may have edges in two sources
   at a seam. `SourceSpan::span()` names one range of one source, so residency holds by
   construction; flagging it only because the seam case makes "the token's span" less
   obviously the token's text than it reads.
3. **Confirmed (user, 2026-08-19): `docs/panics.md` untouched** (as in Stage 1): no new panicking public item; the
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

Status: **implemented** (branch `st-3b-tests`, worktree `.claude/worktrees/st-3b-tests`,
base `main` = `4f45340`; commits `c512e65`, `7686985`, `e476621`, `287dfa4`, `d6a3f6c`,
plus this file's).

Tests, plus comment-level changes only. **No production behavior changed**: nothing in
the stage exposed a defect, and the preset instantiated over a non-standard tokenization
with no adjustment at all — which is R7's proof. The non-test edits are the superseded-name
sweep, the rename of the `cfg(test)` byte-accounting helper, and one corrected comment
(all listed below).

### Files changed

- **`techy/src/constructs/span_tiling_tests.rs` (new, `#[cfg(test)]`)** — the core
  tests. Holds `TiledScriptedLang`, the **tiled twin** of `RelaxedLang` (same
  `ScriptedTokenization`, same associated types, `OBEYS_SPAN_TILING` left at its
  default), which is what makes the enforcement counter-tests possible: one script, two
  declarations. Also the shared script rules/seed (whitespace, paragraphs, `{…}` and
  `[…]` groups, `\`-led commands, `%` comments) and the harness `run_nodes` /
  `with_scripted_parse`, which build a `ScriptedReader` from segments, drive the parser
  through `ParseContext`/`ParserSession` (Stage 3a's answer 1 — no full-engine route
  needed), stage a root `List` spanning what the reader describes for the parse, freeze,
  and run **both** oracles on every tree (`validate_tree`, then `check_tree_invariants`
  — test T10).
- **`techy/src/latexlike/span_tiling_tests.rs` (new, `#[cfg(test)]`)** — the preset
  tests, driving the preset's content loop over a `ScriptedReader` through a
  `ParseContext` with a `LatexlikeDriver`.
- **`techy/src/latexlike/test_support.rs`** — `RelaxedScriptedLatexlike`, next to its
  sibling `RelaxedLatexlike`: the same latexlike family member, differing in the
  tokenization alone (`ScriptedTokenization` instead of `StdTokenization`).
- **`techy/src/constructs/mod.rs`, `techy/src/latexlike/mod.rs`** — the two
  `#[cfg(test)] mod` declarations.
- **`techy/src/node/invariants.rs`** — the private `cfg(test)` `check_parse_law_node`
  renamed to `check_span_tiling_node` (`:599`, `:616`), finishing the superseded-name
  sweep in code as well as in prose.
- **`techy/src/latexlike/input.rs:355`** — the reference-read comment corrected: the
  `None` arm is reached not only for an absent argument but also, under
  `OBEYS_SPAN_TILING = false`, for a *provided* argument whose content is not plain
  characters, where nothing is resolved, attached **or diagnosed**. Comment only; the
  behavior question is open question 2.
- The wording sweep (commit `d6a3f6c`, comments and docs only):
  `techy/src/node/mod.rs` (64, 71, 2169), `techy/src/node/arguments.rs` (343),
  `techy/src/node/builder.rs` (655), `techy/src/token/reader.rs` (2664) — the last
  "parse-law"/"parse law" and "partition invariant" occurrences in source, each rewritten
  to name the law the sentence is about (`§1.8`'s superseded-phrases row). A crate-wide
  `grep -rn -e "parse-law" -e "parse law" -e "parse-tree law" -e "partition invariant"
  techy/src techy/tests docs` is now empty.

### Test inventory (T1–T13)

| # | Test | Where |
|---|---|---|
| T1 | `a_chars_run_across_a_seam_is_one_node_owning_the_text_it_read` — `A[0..1]="a"`, `B[0..3]="xyz"`, `A[5..7]=" b"` → one `Chars`, `Owned("axyz b")`, span `A[0..7]` (the recommended describing shape, the hole included), and the span's own content is *not* the node's text | `constructs/span_tiling_tests.rs:332` |
| T1 (counter) | `a_tiled_language_rejects_a_chars_run_across_a_seam` — the same script under `TiledScriptedLang` → implementation error "do not delimit one range of one source" | `constructs/span_tiling_tests.rs:356` |
| T1b | `a_run_ending_at_a_seam_is_owned_and_a_tiled_language_rejects_it` — `A="a"`, `B="{b}"`: the run merely *ends* at the seam. Relaxed: `Owned("a")` + a group. Tiled: the same implementation error — Stage 3a's **S3 pinned deliberately** | `constructs/span_tiling_tests.rs:387` |
| T2 | `a_group_spanning_a_seam_records_the_delimiter_it_cannot_span` — `A="{a"`, `B="b}"` → `Group` @ `A[0..2]`, `open` `Spanned(0..1)`, `close` `Owned("}")`, child `Chars` `Owned("ab")`, re-emits `{ab}` | `constructs/span_tiling_tests.rs:421` |
| T3 | `a_chars_run_across_a_hole_in_one_source_is_one_owned_node` — `A[0..1]="a"`, `A[5..6]="b"` → one `Chars` `Owned("ab")`, describing span `A[0..6]`; the tiled twin rejects the gap too | `constructs/span_tiling_tests.rs:460` |
| T4 | `an_environment_spanning_seams_is_built_and_reemitted_as_stored` — `\begin{itemize}` in A, `body` in B, `\end{itemize}` in A → environment node @ `A[0..28]`, body content `Owned("body")` @ `B[0..4]`, both scaffolding sides `Spanned` (they lie in the node's own source — the node-data rule), re-emits `\begin{itemize}body\end{itemize}` | `latexlike/span_tiling_tests.rs:135` |
| T4 (other arm) | `an_environment_terminator_from_another_source_is_recorded_as_text` — `\end{itemize}` in B while the node's span is described in A → the terminator's `command_word` recorded as `Owned("end")`; re-emission unaffected | `latexlike/span_tiling_tests.rs:186` |
| T4 (name) | `an_environment_name_read_across_a_seam_resolves_the_environment` — `\begin{` in A, `itemize` in B, `}x\end{itemize}` in A → the lookup resolves (no diagnostics), `name() == "itemize"`, body `x`, re-emits `\begin{itemize}x\end{itemize}` | `latexlike/span_tiling_tests.rs:212` |
| T4 (`\input`) | `an_input_reference_read_across_seams_resolves` — `\input{` in A, `chap.tex` in B, `}` in A → the argument's content is `Owned("chap.tex")` and the resolver finds it (a source is attached), which is only possible because `\input` reads the reference off node data | `latexlike/span_tiling_tests.rs:306` |
| T4 (`\input`) | `an_input_reference_that_is_not_plain_characters_is_not_read` — `\input{{chap.tex}}` → the documented `None` answer: the content is a group, nothing is resolved, nothing is attached, and nothing is diagnosed | `latexlike/span_tiling_tests.rs:354` |
| T5 | `an_unconsumed_stop_token_at_a_seam_is_peeked_again_where_it_stands` — `A="ab"`, `B="cd"`, stop on `B`'s first token with `consume = false`. The reader assertions run inside the parse: re-peeking yields that very token, with empty pre-space, at the position the stream stands at (clauses 2/7 through the seam), and its `EndPastPostSpace` edge is the cause's `after`. The stop span is `B[0..1]`; the tree is then finished through the harness, so the run flushed at the stop reads back as `Owned("ab")` @ `A[0..2]` — oracles included | `constructs/span_tiling_tests.rs:492` |
| T6 | `an_optional_argument_probe_matches_across_a_seam` — `\cmd` in A, `[x]z` in B → the option group parses, content `Owned("x")` | `constructs/span_tiling_tests.rs:576` |
| T6 | `an_optional_argument_probe_that_fails_rewinds_across_a_seam` — `\cmd%c` in A, `y` in B: the probe reads A's comment as noise, sees B's `y`, and rewinds **back across the seam into A** (clause 3); the comment is peekable again exactly where it was. Builds no tree (see below) | `constructs/span_tiling_tests.rs:620` |
| T7 | `a_comment_and_a_paragraph_break_in_another_source_become_nodes` — `A="a"`, `B="%note\n\nb"` → four nodes: `Owned("a")`, a comment whose three sub-spans stay `Spanned` (the token lies wholly in the node's own source), the paragraph-break node `Spanned("\n\n")` (its span *is* the fact's span), `Owned("b")`; re-emits the input | `constructs/span_tiling_tests.rs:656` |
| T8 | `a_macro_post_space_is_recorded_as_text_where_the_language_does_not_obey_tiling` — `y` in B, `\foo z` in A → `Macro { escape_char: '\\', post_space: Owned(" ") }`, node span `A[0..5]`, re-emits `y\foo z` | `latexlike/span_tiling_tests.rs:250` |
| T9 | `a_token_not_starting_where_the_stream_stood_is_an_implementation_error` — `ScriptedReader::broken_at_seams` under **both** `RelaxedLang` and `TiledScriptedLang` → the clause-7 message ("is not the position the stream stood at when the token was peeked … violates the `TokenReader` contract"). Builds no tree (see below) | `constructs/span_tiling_tests.rs:726` |
| T10 | folded into both harnesses — every tree here goes through `validate_tree` and then `check_tree_invariants`, which for these languages runs the all-trees law and stops. T7 carries the explicit note that its root children do not even share a source, which the span-tiling law's byte accounting forbids: if the gate were removed, T7 would panic | `constructs/span_tiling_tests.rs:254` and `:836`, `latexlike/span_tiling_tests.rs:99` |
| T11 | `a_diagnostic_in_a_synthesized_source_renders_the_provenance_chain` — `A="x"` then a `Source::synthesized("{y", "macro expansion", A[1..10])`; the group opened in the expansion is never closed, and the tolerant parse's rendered report carries `synthesized from @ (line 1, col 2) (macro expansion)` | `constructs/span_tiling_tests.rs:748` |
| T12 | `recomposing_a_run_across_a_seam_emits_the_text_as_stored` — T1's tree re-emits `"axyz b"` | `constructs/span_tiling_tests.rs:374` |
| T13 | `verbatim_content_starting_at_a_seam_is_staged_as_the_text_it_read` — `A[0..1]="{"`, `B[0..2]="ab"`, `A[4..7]="  }"`: the raw content begins exactly at the seam, and the whitespace before the terminator arrives as its **pre-space** — content by the raw-content loop's rule. Staged `Owned("ab  ")` while the described span is `B[0..2]` = `"ab"`: text and span genuinely disagree, which is what `raw_content_text` exists for | `constructs/span_tiling_tests.rs:841` |
| T13 (twin) | `verbatim_content_running_to_end_of_stream_keeps_the_whitespace_before_it` — the raw-content loop's *other* pre-space arm (`verbatim_parser.rs:251`): `A[0..1]="{"`, `B[0..2]="ab"`, `A[4..6]="  "`, the region ending at end of stream instead of at a terminator. Same `Owned("ab  ")`, the never-found close recorded by the empty-close convention, `UnterminatedVerbatim` diagnosed under tolerant recovery | `constructs/span_tiling_tests.rs:872` |

Every test above finishes a tree and runs both oracles on it, with **two exceptions**
that deliberately have nothing to freeze: the failing-probe half of T6 (the argument is
absent, so the probed noise nodes are never claimed — what the test is about is where the
reader ends up, not what was staged) and T9 (the parse aborts before any node is staged).
The three tiled counter-tests (T1, T1b, T3) likewise end in an error rather than a tree,
by construction.

### Decisions taken

- **B1 — the tiled twin over the scripted reader.** §5's T1 asks for "the same script
  under a *tiled* language over the same reader", which needs a second language:
  `TiledScriptedLang` (`constructs/span_tiling_tests.rs:61`) is `RelaxedLang` with the
  const left at its default. It is used by T1, T1b, T3 and T9.
- **B2 — no `Language::parse` route.** Confirmed as Stage 3a's answer 1 predicted: all of
  T1–T13 are reachable through `ParseContext`/`ParserSession` over a directly built
  reader, including T11 (which needs a session so the recorded diagnostics can be
  rendered). No test driver override, no second driver, nothing added to `RelaxedLang`.
- **B3 — the seed state is shared with the parsers that mint rules.** `Arc` identity is
  what `probe_minted_group` matches on (`Arc::ptr_eq`), and a fixed script cannot be
  re-tokenized under a parser's momentary state, so T6 passes the optional-argument
  parser **the very `[…]` rule the seed carries** (`bracket_rule`, reading it back out of
  the state). The harness therefore takes the seed state as a parameter rather than
  minting a fresh one per call.
- **B4 — the verbatim script is tokenized under a verbatim-shaped seed.** T13's script
  state has every delimiter recognizer off and `}` installed as
  `expecting_group_close` — the shape `verbatim_state_delta` derives — so the fixed
  script serves `VerbatimArgumentParser` faithfully. Whitespace is left **on**, which the
  verbatim state itself turns off: that is exactly what lets the terminator arrive with
  pre-space and so exercises the `push_pre_space_text` arm of `read_raw_content`, which a
  scanning reader can never reach (see the open questions).
- **B5 — `RelaxedScriptedLatexlike` lives in `latexlike/test_support.rs`**, next to
  `RelaxedLatexlike` (Stage 2's shared place for relaxed preset languages), not in the
  test module: the two differ in the tokenization alone and read best side by side.

### R7's proof

`RelaxedScriptedLatexlike` (`latexlike/test_support.rs:171`) is a `LatexlikeLang` with
`Tokenization = ScriptedTokenization` and `OBEYS_SPAN_TILING = false`. It compiled and
parsed **with no change to the preset**: `LatexlikeDriver<LLL>` carries no tokenization
bound, `EnvironmentSpec`/`MacroSpec`/`InputMacroSpec`, `StdEnvironmentSyntax`,
`InvocationSyntaxData`, `check_latexlike_tree_invariants` and `SourceRecomposer` are all
generic over the family and reach the declaration through the shared helpers. The plan's
risk row "`LatexlikeLang` requires more than expected to instantiate over a custom
tokenization" did not materialize.

### Gate results (verbatim, run from the worktree)

```
### cargo build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.62s
### cargo test --workspace
test result: ok. 1122 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.69s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 86 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 13.14s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
### cargo test --workspace --all-features
test result: ok. 1161 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.70s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 87 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 13.25s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
### cargo clippy --workspace --all-targets -- -D warnings
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-3b-tests/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.45s
### cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-3b-tests/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.13s
### rm -rf target/doc && cargo docs --all-features
 Documenting techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-3b-tests/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.63s
   Generated /Users/philippe/projects/techy/.claude/worktrees/st-3b-tests/target/doc/techy/index.html and 1 other file
```

The base `4f45340` has 1102 / 1141 lib tests and the branch has 1122 / 1161: Stage 3b
adds **20** (14 core + 6 preset) and changes no existing test. (The `cargo test --lib
span_tiling` filter matches 28 — it also catches the Stage 2 tests whose names contain
"span tiling".)

### Deviations from §5

1. **T4 grew a second and third case.** §5's script (`\begin` in A, body in B, `\end` in
   A) puts both scaffolding sides in the environment node's own source, so the node-data
   rule keeps them `Spanned` — the rule's *other* arm needed a script where the
   terminator lies elsewhere, which is the second test. The third (the name read across
   a seam) is the Stage 2 reviewer's explicit ask, recorded there as "a reader whose
   description disagrees is Stage 3b's scripted reader".
2. **T7's comment sub-spans are all `Spanned`.** The plan's R2 switched
   `comment_node_kind` to the node-data rule because "at a seam a single token can have
   edges in two sources". A `ScriptedReader` token is scanned inside one segment, so it
   never has edges in two sources: the `Owned` arm of `comment_node_kind` is **not
   reachable with this reader**, and the test asserts the `Spanned` arm and says so. An
   expanding reader that splices mid-token would reach it; nothing here can.
3. **T13 cannot make the described span empty while the text is not.** §5's parenthetical
   ("so the describing span *may* be empty") describes what a reader is permitted to
   answer, not what this one does: the recommended describing shape always covers at
   least the first entry of the stretch, so under `ScriptedReader` an empty span implies
   an empty text. What the test pins instead is the substantive half — span and text
   genuinely disagreeing (`B[0..2]` = `"ab"` against `Owned("ab  ")`) — plus the
   terminator-pre-space arm, which is what `raw_content_text`'s emptiness rule guards
   against in general.
4. **`\input` (T4's optional extra) attaches an empty parse.** The attached sub-parse
   builds its reader through the driver's `make_token_reader`, i.e. the language's own
   tokenization — and `ScriptedTokenization` has no script to give it (documented on the
   type), so it serves an empty stream. The test therefore asserts that a source *was*
   attached (which is the proof that the reference the parser read is the one the
   resolver knows), not what the attached content is.

### Open questions for the user

1. **The verbatim recipe's pre-space arms are unreachable through a scanning reader.**
   `read_raw_content` treats as content both the terminator's pre-space
   (`verbatim_parser.rs:240`) and the end-of-stream token's (`:251`), but
   `verbatim_state_delta` disables whitespace, so `StdTokenReader` gives neither token any
   pre-space: under a scanning reader both arms are dead code. They are live for a reader
   that does not re-tokenize under the recipe state — the scripted one, and a token-list
   or expanding reader in general — which is why they are right to keep. Both are now
   covered (T13 and its twin). Flagged so the Stage 5 record can say so rather than
   leaving them looking accidental. No change proposed.
2. **`\input` with a non-chars reference is silent under `OBEYS_SPAN_TILING = false`.**
   `argument_text` answers `None` for content that is not plain characters, and the call
   site (`latexlike/input.rs:355`) then resolves, attaches and diagnoses nothing — for a
   *provided* argument whose content is a group (`\input{{chap.tex}}`) as much as for an
   absent one. Under a tiled language the same input takes the
   span route and diagnoses an unresolvable reference. Pinned as the documented answer by
   `an_input_reference_that_is_not_plain_characters_is_not_read`, and the comment now
   states today's behavior precisely (see "Files changed"); whether the `None` branch
   should diagnose something is a design decision, not a Stage 3b fix.
3. **No production defect was found.** Nothing in T1–T13 needed a behavior change, and no
   test is `#[ignore]`d.

## Stage 5 — record

Status: **reviewed**, rebased onto `main` at `514154f` (branch `st-5-record`, worktree
`.claude/worktrees/st-5-record`). Review **PASS** with four required fixes and nine
precision items, all applied (see *Review fixes* below). After Stage 3b merged, the two
test claims were made factual and Stage 3b's findings were folded into the entry (see
*Post-Stage-3b* below).

Documentation only — no source file was touched.

### Files changed

- **`dev-docs/DESIGN_RATIONALE.md`**
  - New entry **`[§dd-dr:span-tiling]`** — "Span tiling is a declared property of the
    language; parsers assume nothing otherwise" — in the *Nodes and the syntax tree*
    topic, directly after [§dd-dr:span-invariants] (the entry it qualifies) and before
    [§dd-dr:node-id-provenance]. It records: the decision and why it is a per-language
    declaration; the definition by pointer to the const's rustdoc, with its three
    components named and "one span per node" excluded; the const itself (associated
    const, default `true`, "obeys" = a fact, not a knob; not a `LangFeatures` member,
    not a marker type); the two regimes; the required, undefaulted
    `TokenReader::source_span_describing` with its no-assumptions contract, its
    recommended shape and the single dispatch point
    (`ParseContext::source_span_within`); the seam analysis (the chars-run check is a
    clause-2/7 check and stays in both regimes, clause 7 writes down where consecutive
    tokens meet, the two sides of a seam are one position value, runs may cross a seam,
    hence owned content); the node-data rule (single-token facts `Spanned` iff the fact
    lies in the node's own source, multi-token content and pre-staging payloads owned
    under `false`, the environment name exact through `NameGroup::name_text`); the
    consumers rule (content from node data, coordinate accessors answer coordinates,
    recompose "as stored", `validate_tree` unchanged and satisfied, the byte accounting
    confined to the test-only span-tiling law); the scripted multi-source test reader as
    the enforcement and test tool (canonical seam positions, `within` answering by the
    end position's source); the accepted costs (owned multi-token content, no zero-copy
    for it, two public-API breaks under the soft freeze); the seven rejected
    alternatives; the deferred items, including the `FrameTitle` span-quoting under
    `false` (recorded, not fixed); and the revisit condition.
  - Dated amendment paragraphs (history preserved, labels untouched) on
    **[§dd-dr:span-invariants]** (the five invariants are the tiled statement; items 1
    and 3 record owned text and item 5's accounting does not apply under `false`),
    **[§dd-dr:token-opacity]** (the motivating expanding-reader case is now supported),
    **[§dd-dr:stream-position]** (clause 7 and what positions mean at a seam),
    **[§dd-dr:token-contract-hardening]** (the contract gained clauses 7 and 8, the
    *Seams* section, and `source_span_describing` beside item 4's family),
    **[§dd-dr:input-attachment]** ("every sibling run stays single-source" is a
    statement about a language that obeys span tiling) and **[§dd-dr:tree-validation]**
    (the oracle's gate, and the rename).
  - **[§dd-dr:superseded-names]** gains the three superseded phrases with their
    replacements, and the note that no mode name is coined for the other regime.
  - Wording sweep: every "partition invariant" (8 sites, [§dd-dr:span-invariants]'s
    item 5 among them) and every "parse-law"/"parse-tree law" (12 sites) rewritten to
    the span-tiling vocabulary; the only remaining occurrences are the two that *name*
    the superseded phrases.
- **`dev-docs/ARCHITECTURE.md`**
  - New section **`## Span tiling [§dd-arch:span-tiling]`**, between *Node trees* and
    *Construct parsers*: the declaration and the definition by pointer to the const
    doc; the two regimes with their consequences; the dispatch point; the consumers
    rule of the plan's §1.6; the reader-contract half (clauses 7 and 8, seams); the
    all-trees law versus the test-only span-tiling law, and the scripted reader; the
    vocabulary, with no name coined for the second regime; and the decisions line
    ([§dd-dr:span-tiling], [§dd-dr:span-invariants], [§dd-dr:tree-validation]).
  - Pointers into it from [§dd-arch:token] (after the contract summary),
    [§dd-arch:nodes] (the whitespace-and-span-invariants bullet, whose tiling clause is
    now stated as conditional, plus the recomposition-levels and read-surface bullets),
    [§dd-arch:constructs] (after the node-data rule) and [§dd-arch:naming] (the
    vocabulary lives in one place).
  - Wording sweep: "partition invariant" (1) and "parse-law" (3) gone.
- **`TODO_Big.md`** — the "Gap-free chars-run contract" item under *Better tokens* is
  struck through and marked **DONE**, pointing at [§dd-dr:span-tiling].

### Grep gates

```
$ grep -n "§dd-dr:span-tiling]" dev-docs/ARCHITECTURE.md
707:Decisions behind this section: [§dd-dr:span-tiling]; the invariants it qualifies —

$ python3 -c "<every §dd-dr heading label vs ARCHITECTURE>"
215 heading labels; 0 missing from ARCHITECTURE

$ grep -rni "partition invariant\|parse.law\|parse.tree law\|in-order, gap.free" \
      dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md techy/src techy/tests docs
dev-docs/DESIGN_RATIONALE.md:2907, 6766, 6769   the three sites that *name* the superseded
                                                phrases (the [§dd-dr:tree-validation]
                                                amendment, the [§dd-dr:superseded-names]
                                                bullet)
techy/src/node/invariants.rs:823                "Parse-law point 1" (rustdoc on a private
                                                cfg(test) helper) — see below
```

`ARCHITECTURE.md` is clean, and Stage 3b swept the six source hits this stage had routed to
it (`token/reader.rs`, `node/mod.rs` ×3, `node/arguments.rs`, `node/builder.rs`) and renamed
`check_parse_law_node` to `check_span_tiling_node`. **One occurrence survives that sweep**
because its grep was case-sensitive: `techy/src/node/invariants.rs:823` still begins
"Parse-law point 1". Stage 5 is documentation-only, so it is left for the next
source-touching change (a one-word doc-comment fix: "Span-tiling law, point 1").

Deliberately **not** swept: `dev-docs/DESIGN_RATIONALE.md:611-612` "a token-span partition
invariant" — that is a *token-level* property (whitespace as its own token would have made
the token spans partition the input), unrelated to the tree-level property this project
names, and it stays as written.

### Gate results (verbatim, run from the worktree)

```
### rm -rf target/doc && cargo docs --all-features
 Documenting techy-derive v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-5-record/techy-derive)
 Documenting techy v0.1.0 (/Users/philippe/projects/techy/.claude/worktrees/st-5-record/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.89s
   Generated /Users/philippe/projects/techy/.claude/worktrees/st-5-record/target/doc/techy/index.html and 1 other file

### cargo test --workspace
test result: ok. 1122 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.67s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 86 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 21.63s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Same counts as `main` at `514154f` (1122 lib tests, Stage 3b's twenty included) — nothing
in the crate depends on these files. `cargo docs --all-features` emits no warnings.

### Post-Stage-3b (the record made factual)

The rebase onto `514154f` conflicted only in this file's status table (kept every stage's
row; 1, 2, 3a, 3b, 4 = merged, 5 = reviewed). The entry then changed in four places:

1. **The two test claims are facts now.** The enforcement paragraph names
   `techy/src/constructs/span_tiling_tests.rs` as where the parse-level clause-7 test drives
   the broken scripted variant under both declarations (beside the single-source version of
   the same check in `constructs/nodes_parser.rs`), and says the tiled counter-tests of that
   module run seam-crossing and hole-crossing scripts and get the implementation error out
   of the parser's own run flush — clause 8's enforcement demonstrated rather than asserted.
   Cited by module path, no line numbers.
2. **R7's proof recorded** — the preset serves a latexlike family member over the scripted
   tokenization with no preset change at all (driver, specs, syntax record, oracle and
   source recomposer are generic over the family); the plan's risk row did not materialize.
   One clause added to [§dd-arch:span-tiling] as well.
3. **The two deliberately kept, scanning-unreachable arms** are recorded in their own
   paragraph so they do not read as accidental: the verbatim recipe's terminator and
   end-of-stream pre-space arms (the recipe state turns whitespace off, so a re-tokenizing
   reader never produces that pre-space) and `comment_node_kind`'s owned arm (it needs a
   comment token with edges in two sources). Both are right for a reader that splices
   mid-stream, and both are covered only through the scripted reader.
4. **The `\input` question is recorded as a user decision**, in the entry's deferred
   paragraph and as its own `TODO_Big.md` bullet: under `false` a *provided* reference
   argument that is not plain characters (`\input{{chap.tex}}`) resolves, attaches and
   diagnoses nothing, while the tiled route diagnoses an unresolvable reference for the
   literal `"{chap.tex}"`; the four options are named (recompose-and-resolve, a distinct
   "reference is not plain characters" condition, keep `None`, make the tiled route agree).

Also: [§dd-dr:superseded-names] now names the renamed in-crate helper
(`check_span_tiling_node`).

### Review fixes (all applied)

Required:

1. **The macro post-space had the wrong reason.** It *is* one reader answer
   (`source_span_between(token, End, EndPastPostSpace)`); it is owned under `false`
   because the payload is built before the node's span exists, so the node-data rule has
   no node span to test residency against. Corrected in the [§dd-dr:span-invariants]
   amendment, in the entry's costs paragraph, and in the ARCHITECTURE `false` bullet (the
   entry's node-data paragraph already said it correctly, and now says it separately from
   the token-by-token accumulation).
2. **Typo from the rename** — the duplicated "the" in [§dd-dr:superseded-names]'s
   environment-writer bullet.
3. **Two claims about tests that do not exist on `main` yet** are reworded as design
   intent: the broken scripted variant "exists for the parse-level clause-7 test", and the
   end-position rule "is what lets a tiled counter-test over a seam-crossing script prove
   clause 8's enforcement". To be made factual after Stage 3b lands.
4. **The superseded-phrase gate** now greps the `parse-law` spelling too, lists the six
   source hits routed to Stage 3b, and states the two sites that stay (see *Grep gates*).

Precision:

5. The six amendment notes lost their dates — `*Amendment (user, span-tiling design
   session).*`, matching the status-line style and the documented rule (dates only inside
   reversal notes).
6. "restated nowhere else" / "no other page restates it" → the components are named here,
   the wording is the const's.
7. "the single dispatch point" → "the public dispatch point", with the private
   `invocation_span_within` named as its mirror (both documents).
8. ARCHITECTURE: "every language of this crate" → "every shipped language", with the
   in-crate test languages named as the exception.
9. Ruling 4 is now recorded in full: the [§dd-dr:token-contract-hardening] amendment names
   the four further rules (termination is the reader's; positions and tokens stay valid in
   exhausted sources; `SourceProvenance::Synthesized` with no `Frame` for an expansion's
   source; `EndOfStream` = the end of the whole input).
10. The multi-token ruling is stated in general form — under `false` no multi-token `Chars`
    node records `Spanned` content, recovery and marker nodes included — beside the
    enumeration (both documents).
11. The `extract` claim names its price: the documented answers hold *after* three doc
    claims were narrowed to what the code does (`piece_span`, the module docs,
    `split_at_chars`).
12. `TODO_Big.md` gains a *Span tiling* item for the `FrameTitle::Quoted`/`Callable`
    span-quoting under `false`, pointing at [§dd-dr:span-tiling].
13. Stale line numbers in this section refreshed.

### Deviations

1. **The `Status:` line carries no date.** The stage brief asked for
   `Status: DECIDED (user, 2026-08-19)`; `Documentation_Structure.md` and
   [§dd-dr:self-meta] both rule that status lines carry who/context and **never** dates
   (dates belong only inside explicitly recorded reversal or amendment notes). The entry
   reads `Status: DECIDED (user, span-tiling design session)`, matching the
   [§dd-dr:token-contract-hardening] precedent; the six amendment notes carry the date.
2. **The ARCHITECTURE subsection is a top-level `##` section**, not a `###` inside
   [§dd-arch:nodes]. The file has no `###` level anywhere, and each `##` section ends
   with its own "Decisions behind this section" list; a `###` would have split that
   list. The property is also cross-cutting (tokens, parsers, nodes, consumers), so it
   sits between *Node trees* and *Construct parsers* with pointers from all three.
3. **The `[§dd-dr:tree-validation]` entry was amended too** (not in the brief's list):
   it named the oracle "the parse-tree law" three times, which the naming register
   supersedes, so the rename needed a recorded reason.

### Open questions for the user

1. **One superseded phrase survives in source**: `techy/src/node/invariants.rs:823`
   ("Parse-law point 1"), missed by Stage 3b's case-sensitive sweep. Stage 5 is
   documentation-only; it is a one-word doc-comment fix for the next change that touches
   that file.
2. **The `FrameTitle` defect stays recorded, not fixed** (carried from Stage 2's open
   question 1): under `OBEYS_SPAN_TILING = false` a traceback frame's title can quote
   text that was never read, because it renders a multi-token span. The record names both
   affected variants and the feeding sites and says the repair changes the public
   `FrameTitle`; `TODO_Big.md` now carries it as its own item, pointing at
   [§dd-dr:span-tiling].
3. **`\input` with a non-chars reference under `OBEYS_SPAN_TILING = false`** (Stage 3b's
   open question 2) is recorded, not decided: the entry's deferred paragraph and
   `TODO_Big.md` carry the four options. Today's silence is pinned by a test.
4. **`TODO_Big.md`'s remaining "Better tokens" items** were left as they stand; only the
   chars-run item is this project's. The item just below it
   (`LatexlikeDriver::with_token_reader`) is also PLAN §10 item 4, still deferred.

## Orchestrator log

- 2026-08-19: plan written and committed to `main`.
- 2026-08-19: Stage 3a implemented on `st-3a-scripted` (see its section).
