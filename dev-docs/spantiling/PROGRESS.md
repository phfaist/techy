# Span tiling — progress

Plan: `dev-docs/spantiling/PLAN.md`. Protocol: PLAN §8. Every subagent runs on Opus.

| Stage | Branch | Base | Worktree | Status |
|---|---|---|---|---|
| 1 contract surface | `st-1-contract` | `main` | `.claude/worktrees/st-1-contract` | reviewed |
| 2 parsers | `st-2-parsers` | `st-1-contract` | `.claude/worktrees/st-2-parsers` | planned |
| 3a scripted reader | `st-3a-scripted` | `st-1-contract` | `.claude/worktrees/st-3a-scripted` | planned |
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

## Stage 3a — scripted reader

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
