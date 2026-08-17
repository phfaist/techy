# Better tokens — progress log

Companion to `PLAN.md` (§8, "State on disk"). This file is the resumable state of the
plan's execution: a fresh session must be able to continue from `PLAN.md` +
`PROGRESS.md` + `git log` alone.

**One section per stage**, appended as the stage starts and updated as it advances. Each
section records:

- **Branch** and **worktree** the stage's implementer works in (the plan's branch chain:
  `bt-probe`, `bt-1-positions`, `bt-2a-core`, `bt-2b-rest`, `bt-3a-view`,
  `bt-3b-opaque`, `bt-4-final`, `bt-5-docs`).
- **Status**: `started` → `implemented — awaiting review` → `reviewed` → `merged`
  (with the merge commit or the note that nothing is merged, as for Stage 0).
- **Gate results**, verbatim (the commands the stage's own section of `PLAN.md`
  prescribes, with their output lines and counts).
- **Decisions taken under §1.16** — the small decisions with pre-agreed defaults; each
  one an implementer actually hit gets a line here saying what was chosen.
- **Open questions**: anything not covered by §1.16, including the standing §1.17
  rulings, with their answers and dates once given. An implementer never decides these.

Nothing in this file supersedes `PLAN.md`; where they disagree, the plan wins and the
discrepancy is an open question.

---

## Stage 0 — compiler probe (§2)

- **Branch**: `bt-probe` (off `main` at `b528eea`; work started at `fb8dd23` and was
  rebased).
- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/bt-probe`.
- **Status**: reviewed and merged (`main` 7825789, docs-only cherry-pick). Date:
  2026-08-17.
- **What exists on the branch**: the standalone crate `bettertokens-probe/` (its own
  workspace via an empty `[workspace]` table, zero dependencies; the root `Cargo.toml`
  is untouched) with `src/mock/` (the mocked §1 vocabulary) and `src/p1.rs` … `src/p8.rs`
  (the eight probes), plus this file and `PROBE_REPORT.md`. **Only the two documents are
  merged**; the probe crate stays on `bt-probe`, which is discarded (§2).

### Gate results (in `bettertokens-probe/`, toolchain cargo/rustc 1.97.0)

```
$ cargo check
    Checking bettertokens-probe v0.0.0 (…/bt-probe/bettertokens-probe)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.36s

$ cargo test
   Compiling bettertokens-probe v0.0.0 (…/bt-probe/bettertokens-probe)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.54s
     Running unittests src/lib.rs (target/debug/deps/bettertokens_probe-3d5c170d45b87b63)

running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests bettertokens_probe

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

No warnings. Both gates green; the report is written.

### Findings

Full write-up: **`PROBE_REPORT.md`** (per probe: verdict, the signatures actually used,
the compiler errors for the shapes that do not compile, and a "Settled spellings"
section the later stages copy from).

Summary: P1–P7 **PASS**; P8 **FAIL for the literal §1.8 impl header, PASS** with the two
associated-type bounds rustc suggests
(`L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>`).
No §9 fallback is needed: the `TokenReader` trait is object-safe as spelled in §1.6,
`token_kind` keeps its `&self` receiver and `where 's: 't` clause, and
`StdTokenReader<'s, O>` stays generic over the origin rather than the language.

### Decisions taken under §1.16

- **`Invocation.kind` stays a field.** §1.16 made it conditional on the probe ("if the
  probe shows a lifetime problem holding a `TokenKind<'a, L>` in the struct, drop the
  field"). P2 shows no such problem: the view survives a `&mut ParseContext` sub-parse
  and is used afterwards. No `Invocation.name: String` copy, no per-invocation
  allocation.
- No other §1.16 item was reached by Stage 0.

### Review round 1 (fixes applied 2026-08-17)

The stage came back NOT READY; every point below is addressed on the branch.

1. `move_to_pos`'s rustdoc was a broken sentence after the `resume_pos` reference was
   removed — rewritten as one clause.
2. The trait's "a fuller example accompanies the token type itself" promised a page that
   does not exist yet — sentence deleted (the full example arrives with `StdToken`).
3. `StdTokenReader`'s default type parameter: **kept**, recorded as deviation 6.
4. `TokenListReader`'s issued-position set is no longer pre-seeded with the listed
   tokens' edges (see the decisions section); `move_to_pos` now issues the offset it is
   given. No test trips the narrower guard.
5. **A panic closed** (CLAUDE.md rule 4): the scan-error lift passed the hook's `Span`
   straight into `SourceSpan::new`, so a `Lang::scan_specials` returning an out-of-range
   or mid-character span would have panicked inside the reader. The new
   `StdTokenReader::lift_specials_scan_error` validates the span first and otherwise
   reports the hook's contract violation the way the reader reports every other one — an
   unrecoverable `TokenErrorKind::Custom(ImplementationError)` naming `scan_specials` and
   quoting the offending span, anchored at the empty span at `nearest_valid_offset(pos)`.
   New test: `a_specials_scan_error_at_an_invalid_span_is_reported_not_panicked` (both
   the out-of-bounds and the mid-character case; neither panics, both abort).
   `SpecialsMatch::end` was **already** validated on the `Ok(Some(m))` path
   (`m.end > pos && m.end <= s.len() && s.is_char_boundary(m.end)`, reported the same
   way), so that hole was already closed — its test is
   `scan_specials_invalid_match_end_is_an_unrecoverable_implementation_error`.
6. New test `a_lifted_specials_scan_error_carries_no_recovery` pins the lift: the error
   is qualified by the reader's source and `recovery()` is `None`.
7. `SpecialsScanError` gained `Display` (condition wording plus the byte range — the only
   locating information it carries) and `core::error::Error`; fields stay public.
8. Contract clause 4 now carries its second half: the std reader cannot detect a foreign
   token and answers from its offsets, while `TokenListReader` rejects what it did not
   issue — which is what makes the lockstep suites a guard.
9. Deviation 3 records that `nearest_valid_offset` also closes a latent panic.
10. Cosmetics: `core/mod.rs`'s over-long re-export line re-wrapped; the stray blank line
    inside `impl ParseDriver<HelperLang> for HelperDriver` removed.

### Open questions

- **None opened by the probe.** Every shape §2 asked about is settled by the compiler,
  and the one failing spelling (P8) has a mechanical fix, not a design choice.
- The two §1.17 rulings were open when Stage 0 started and were **closed by the user on
  `main` while it ran** (commit `b528eea`, 2026-08-17), so they need nothing from this
  stage: **O-1** — `CallableQuery` carries the token's *view*
  (`token_kind: Option<TokenKind<'a, L>>`, by value) and the whole resolve chain takes
  `token_kind: TokenKind<'_, L>`; **O-2** — the user edits `CLAUDE.md` themselves, no
  stage does. Incidentally, Stage 0 is supporting evidence for O-1: `TokenKind<'t, L>` is
  `Copy` and holding it by value in a lifetime-parameterized struct compiles (that is
  exactly `Invocation.kind`, probe P2).

### Notes for later stages

- The worktree's `.cargo/config.toml` passes
  `--html-in-header docs/rustdoc-header.html` to every rustdoc invocation, and cargo
  applies it to a standalone crate created under the worktree too (config discovery walks
  up from the invocation directory; workspace membership is irrelevant). A standalone
  crate therefore needs its own `docs/rustdoc-header.html` or `cargo test`'s doctest step
  fails. The techy crate itself is unaffected.
- Two ergonomic traps the probe hit, both recorded with their errors in
  `PROBE_REPORT.md` (P4, P8): calls on a **concrete** `StdTokenReader` whose only
  argument is `&L::Token`/`&L::StreamPosition` cannot infer `L` (`error[E0284]`), and a
  language-generic wrapper reader cannot delegate with plain method syntax. Binding the
  reader as `&mut dyn TokenReader<'_, TheLang>` fixes both. Construct-parser code is
  unaffected; reader unit tests and the `TokenListReader` harness are not.

---

## Stage 1 — positions, spans, the reader door (§3)

- **Branch**: `bt-1-positions` (off `main` at `7825789`).
- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/bt-1-positions`.
- **Status**: reviewed and merged (`main` d5f37e0). Date: 2026-08-17.
- **Commits** (`git log --oneline main..bt-1-positions`, oldest last):

```
9ab6924 docs: prose for the reader's position answers and the required reader hook
09e6845 token: tests for the reader's position and span answers
2aab028 token: positions, edges and source-qualified spans on `TokenReader`
8fd0b29 token: the specials hook returns plain errors and no name
90625ed token: `StdTokenReader` reads a `Source`, `ParseDriver::make_token_reader`
5f5dd2c core: add `Lang::StreamPosition`
b801b24 token: add `TokenEdge` and `StdStreamPosition`
abbaad4 source: add `SourceSpan::at(&SourcePos)`
```

### What changed, per file

| File | Change |
|---|---|
| `techy/src/source/source.rs` | `SourceSpan::at(&SourcePos)` (§1.13) + doctest + unit test |
| `techy/src/token/reader.rs` | `TokenEdge`, `StdStreamPosition`; `StdTokenReader<'s, O: SourceOrigin = Option<String>>` built from `&'s Arc<Source<O>>` (new `source()` accessor, `nearest_valid_offset` helper); the P8 impl header and the same bound on the scanning core; the eight new trait methods + contract clauses 1–6 + the custom-reader pattern in the trait rustdoc; error construction through `SourceSpan`; scan-error lift; five new unit tests; `lift_specials_scan_error` validates the hook's span before qualifying it, with two more tests |
| `techy/src/token/error.rs` | `TokenError::span: SourceSpan<L::SourceOrigin>` (`span()` returns `&SourceSpan`), `TokenRecovery::resume: L::StreamPosition` (was `resume_pos: usize`) with the reworded advancement contract |
| `techy/src/token/specials.rs` | `SpecialsMatch<L>` (no lifetime, no `name`), new `SpecialsScanError { kind, span }` with the rationale in its rustdoc, plus `Display` and `Error` |
| `techy/src/token/token.rs` | `pub(crate) Token::edge_offset(TokenEdge)` |
| `techy/src/token/list_reader.rs` | `new(source, tokens)`; the new trait methods; issued-token / issued-position validation (panicking) + its rustdoc section; lockstep test and two `#[should_panic]` negatives; test helpers take a source |
| `techy/src/token/mod.rs` | facade: `TokenEdge`, `StdStreamPosition`, `SpecialsScanError`; module prose |
| `techy/src/core/mod.rs` | public facade: the same three names |
| `techy/src/state/lang.rs` | `Lang::StreamPosition` (opacity rustdoc); `scan_specials` re-signature; blanket `TrivialLang` impl sets `StreamPosition` |
| `techy/src/scopes/mod.rs` | `SpecsProvider`/`Package`/`ScopeStack::scan_specials` re-signature; tests updated (`matched.end`, `error.kind`) |
| `techy/src/engine/driver.rs` | `ParseDriver::make_token_reader` (**required**, see deviations); `probe_token` loses `source`; `StdParseDriver`'s impl gains the `StreamPosition = StdStreamPosition` bound |
| `techy/src/engine/language.rs` | `parse_source` builds the reader through `driver.make_token_reader(&source)`; advanced-path doc example follows |
| `techy/src/constructs/attached_source.rs` | same reader-construction route |
| `techy/src/constructs/mod.rs` | `ParseContext::probe_token` drops the source argument |
| `techy/src/constructs/nodes_parser.rs` | content loop: `position_here()` / `move_to_position(&recovery.resume)` with the equality check; `TabooLang` hook now unrecoverable + new `TabooReader`; `StuckRecoveryReader` rewritten as a delegating wrapper; test harnesses (`try_run`, `try_run_with`, `scan`, `run_both`, `run_both_with`) take `&Arc<Source>` |
| `techy/src/constructs/argument_parsers.rs` | `BrokenReader` rewritten as a delegating wrapper; harness takes `&Arc<Source>` |
| `techy/src/constructs/environment_parser.rs` | `FlakyReader` carries the new methods; harness takes `&Arc<Source>` |
| `techy/src/constructs/{verbatim,group}_parser.rs` | reader construction from the test's own source |
| `techy/src/latexlike/{lang,driver,mod}.rs` | `LatexlikeLang` requires `StreamPosition = StdStreamPosition`; `LatexlikeDriver::make_token_reader`; preset `scan_specials` re-signature |
| 56 `impl Lang` sites across `techy/src`, `techy/tests`, `docs/custom-lang.md` | `type StreamPosition = …StdStreamPosition;` |
| ~30 `impl ParseDriver` sites | the `make_token_reader` one-liner |
| `docs/custom-lang.md` | the doctest's `impl Lang` block; the driver paragraph now says `make_token_reader` is the one hook without a default |

No construct parser was ported off the old positional API (that is Stage 2):
`move_past`, the two-flag `move_to`, `move_to_pos`, `pos()`, `Token<'s, L>`'s public
shape and `ParseContext::source` are all unchanged.

### Gate results (verbatim)

```
$ cargo build
   Compiling techy-derive v0.1.0 (…/bt-1-positions/techy-derive)
   Compiling techy v0.1.0 (…/bt-1-positions/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.07s

$ cargo test
     Running unittests src/lib.rs (target/debug/deps/techy-94158093885f6495)
running 1027 tests
test result: ok. 1027 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s
     Running tests/acceptance.rs
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/derive_conditions.rs
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/lang_features.rs
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/recompose_oracle.rs
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/serialize_golden.rs / serialize_perf.rs / serialize_stream.rs
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out (each)
     Running unittests src/lib.rs (techy_derive)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests techy
running 89 tests
test result: ok. 84 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 13.20s
   Doc-tests techy_derive
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
(clean — no warnings, exit 0)

$ rm -rf target/doc && cargo docs
 Documenting techy v0.1.0 (…/bt-1-positions/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.54s
   Generated …/target/doc/techy/index.html and 1 other file
(no broken intra-doc links)
```

**Lockstep harness.** `run_both` is a helper function, not a test name, so
`cargo test run_both` selects nothing (`0 passed; … 1025 filtered out`). It is called
from 59 sites in `techy/src/constructs/nodes_parser.rs`; the suite that exercises it:

```
$ cargo test -p techy --lib constructs::nodes_parser
running 78 tests
test result: ok. 78 passed; 0 failed; 0 ignored; 0 measured; 949 filtered out; finished in 0.05s

$ cargo test -p techy --lib token::list_reader
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 1016 filtered out; finished in 0.01s
  (includes positions_and_spans_match_the_std_reader_in_lockstep and the two
   should-panic negatives)
```

### Semver report (`scripts/check_semver.sh`)

Breaking changes are expected (soft freeze) and were not "fixed".

```
     Summary semver requires new major version: 9 major and 0 minor checks failed
    Finished [   2.790s] techy
```

The nine failing lints:

| Lint | Item |
|---|---|
| `auto_trait_impl_removed` | `BeginSpec` no longer `UnwindSafe`/`RefUnwindSafe` — **pre-existing** on `main` (baseline predates it) |
| `constructible_struct_adds_field` | `StdCallableSpec.provenance` (**pre-existing**); `TokenRecovery.resume` (this stage) |
| `constructible_struct_adds_private_field` | `SpecialsSpec.provenance` (**pre-existing**) |
| `struct_pub_field_missing` | `TokenRecovery::resume_pos`; `SpecialsMatch::name` (this stage) |
| `trait_added_supertrait` | `CallableSpec`/`SpecsProvider` gained `SerializableObject` (**pre-existing**) |
| `trait_associated_type_added` | `Lang::StreamPosition` (this stage) |
| `trait_method_added` | `ParseDriver::make_token_reader` (this stage — see deviations) |
| `trait_method_parameter_count_changed` | `ParseDriver::probe_token` 4 → 3 parameters (this stage) |
| `type_mismatched_generic_lifetimes` | `SpecialsMatch` 1 → 0 lifetime params (this stage) |

### Decisions taken under §1.16

- **`source_span_between` with equal edges** returns the empty span at that edge —
  implemented (`a.min(b)..a.max(b)`) and covered by
  `source_span_between_ignores_edge_order_and_empties_on_equal_edges`.
- **`StdTokenReader` on a foreign token** relies on `SourceSpan::new`'s registered
  always-on assert; no `Option` added. Contract clause 4 says so on the trait.
- **`is_at_end()` kept** on `StdTokenReader`, as §1.16 prescribes; `pos()` and
  `move_to_pos()` also stay (Stage 2 removes them).
- **`TokenListReader` validation rule** (§1.8 asked for it; §9's risk row anticipated
  the clipped-`pre_space` problem, which is real — `peek` clips `pre_space`, so a token
  this reader issued does not compare equal to its list entry):
  - *tokens*: accepted when a listed token has the **same `span` and same `kind`**
    (`pre_space` excluded), or when the token is `EndOfStream` (synthesized past the end
    of the list rather than served from it);
  - *positions*: accepted when the offset is in a `BTreeSet<usize>` of issued offsets,
    seeded with the **initial position alone** and extended by the four edge offsets of
    every token the reader serves, by every `position_here`/`position_at` answer, and by
    every offset the reader is moved to (`move_to_edge`/`move_to_pos`). The set lives
    behind a `RefCell` because the position accessors take `&self`. (First implemented
    with the listed tokens' edges pre-seeded; narrowed on review. **No legitimate test
    trips the narrower guard** — the full suite and both lockstep suites pass unchanged
    — so §9's "compare on span + kind" fallback was not needed for any position case;
    it *is* what the token rule does, for the clipped-`pre_space` reason above.)
  - Violations `panic!` with "was handed a token/position it never issued" — test
    infrastructure only; no new panic in library code.
- **The three test hooks that produced a `TokenRecovery`** (§3 step 5): only one was a
  `scan_specials` hook — `TabooLang` (`nodes_parser.rs`). The other two are *reader*
  impls (`StuckRecoveryReader`, `FlakyReader`), and readers still produce recoveries, so
  they keep theirs. `TabooLang::scan_specials` now returns an unrecoverable
  `SpecialsScanError`, and its test asserts that **both** policies abort with the
  language's own condition. Because that test's other half was "a `Custom` payload
  reaches the diagnostic unwrapped", the recoverable half moved to a new delegating
  reader, `TabooReader`, which reports the same `Custom` condition recoverably — so the
  recovery path stays covered exactly as before (tolerant: one diagnostic, payload
  downcast, placeholder joins the chars run; strict: the condition rides the
  `ParseError`).

### Deviations from §1/§3

1. **`ParseDriver::make_token_reader` is a required method, not a defaulted one.**
   §1.10/§3 step 6 prescribe the default body `Box::new(StdTokenReader::new(source))`.
   That body cannot type-check for an arbitrary `L: Lang`: `StdTokenReader<'s, O>`
   implements `TokenReader<'s, L>` only when `L::StreamPosition = StdStreamPosition`
   (and, from Stage 3b, `L::Token = StdToken<L>`), while a trait method's default body
   must compile for every `L`. The exact error, produced at the two call sites when the
   bound is missing:

   ```
   error[E0271]: type mismatch resolving `<L as Lang>::StreamPosition == StdStreamPosition`
      --> techy/src/constructs/attached_source.rs:157:13
       |
   157 |             &mut reader,
       |             ^^^^^^^^^^^ expected `StdStreamPosition`, found associated type
       |
       = note:       expected struct `StdStreamPosition`
   ```

   A `where L: Lang<StreamPosition = StdStreamPosition>` clause on the *method* would
   make `Language::parse_source::<L>` unable to call it for a generic `L`; there is no
   specialization. Interim, agreed with the orchestrator (2026-08-17), pending a user
   ruling: the method is required, its rustdoc gives the standard one-liner body, and
   every in-crate driver implements it (~30 sites, mostly test drivers). The two generic
   driver impls gained the bound: `impl<L: Lang<StreamPosition = StdStreamPosition>, R:
   CommandResolver<L>> ParseDriver<L> for StdParseDriver<…>`, and `LatexlikeLang` now
   requires `Lang<StreamPosition = crate::token::StdStreamPosition>` (a one-line
   supertrait addition). Alternatives put to the user: **(A)** required method, as
   implemented; **(C)** a `Lang`-side factory (required on `Lang` instead, which keeps
   "an empty `impl ParseDriver` is a complete driver" but costs 57 sites).
   Two prose sites were adjusted minimally: `ParseDriver`'s rustdoc and
   `docs/custom-lang.md`'s driver paragraph.

2. **Test harnesses take `&Arc<Source>` instead of `&str`.** `StdTokenReader::new`
   now borrows a source, so a harness that built its own `Arc<Source>` internally while
   the reader was built from the bare `&str` would have put the reader's spans and the
   context's spans in **two different `Source` instances** — and `SourceSpan` equality is
   identity-based. The four construct-parser harnesses (`try_run` in
   `nodes_parser.rs`, `argument_parsers.rs`, `environment_parser.rs`, plus `scan`) now
   take the source and share one `Arc` with the reader. Same reason for
   `TokenListReader::new`'s test call sites.

3. **`StdTokenReader::nearest_valid_offset`** (new private helper). The invalid-position
   report at `peek` used `Span::empty(start.min(len))`, which is not a legal
   `SourceSpan` when `start` is mid-character — `SourceSpan::new`'s always-on assert
   fires. The error is now anchored at the nearest valid offset at or before the
   offending one. No behavior change beyond the anchor — and it closes a latent panic:
   on `main` a mid-character reader position could not be lifted to a source-qualified
   anchor at all, so the same report would have asserted the moment the token layer
   started building `SourceSpan`s.

4. **`StdTokenReader::source()`** (new public inherent accessor) — the in-crate
   delegating test readers need the reader's source to build their own `TokenError`
   spans. It is the natural sibling of the existing `content()` accessor; say the word
   and it can be `pub(crate)`.

5. ~~`TokenListReader` position validation seeds the issued set with the listed tokens'
   edges~~ — **withdrawn on review**: the set is seeded with the initial position alone,
   exactly as §1.8 prescribes, and grows only as the reader serves tokens, answers
   positions, or is moved. Every test still passes; the forged-position negative (a
   position taken from a std reader over a stretch this list reader never served) still
   fails as it should.

6. **`StdTokenReader<'s, O: SourceOrigin = Option<String>>` keeps a default type
   parameter**, which §1.8 does not mention. Reason (orchestrator ruling, 2026-08-17):
   it mirrors `Source<O = Option<String>>`, `SourceSpan<O = Option<String>>` and
   `SourcePos<O = Option<String>>` — the whole S0 family defaults the origin the same
   way — so `StdTokenReader<'s>` keeps reading as "the reader over an ordinary source",
   and existing `StdTokenReader<'s>` type mentions (e.g. the test readers' `inner`
   fields) stay spelled as they were.

### Open questions

1. **The `make_token_reader` default** (deviation 1) — the plan's prescribed default is
   not implementable; the user must choose (A) required on `ParseDriver`, as
   implemented, or (C) a `Lang`-side factory. Everything else in the stage is
   independent of the answer.
2. **`StdTokenReader::source()` visibility** (deviation 4) — public inherent accessor
   or `pub(crate)`? Public reads naturally next to `content()`, and a third-party
   reader wrapping a `StdTokenReader` plausibly wants it.

---

## Stage 2a — core: the construct-parser layer off bare positions (§4)

- **Branch**: `bt-2a-core` (off `main` at `d5f37e0`, which already contains Stage 1).
- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/bt-2a-core`.
- **Status**: reviewed and merged to `main` (`main` = `0af4276`, fast-forward).
  Date: 2026-08-17.
- **Commits** (`git log --oneline main..bt-2a-core`, oldest last; the newest is this
  PROGRESS update itself, "bettertokens: PROGRESS.md — Stage 2a review round 1"):

```
37d70dd core: anchor the invalid-node-span abort at the trigger
ac9e1d9 latexlike: the paragraph break's payload is its specials form
3f4b41c bettertokens: PROGRESS.md — Stage 1 merged, Stage 2a implemented
65fe206 core: adapt the callers Stage 2a's signatures force
396862a core: the error-callable parser and one renamed-parameter doc comment
741151b core: the root and attached-source loops skip by position
21f588b core: argument parsing on stream positions
a2b6fc7 core: the driver's group and paragraph-break hooks take reader answers
26bdcd4 core: the group parser holds its open token
714f6f5 core: the nodes parser's run, stops and anchors on positions
ef326ab core: ParseContext spans come from the reader
e29adbf token: reader queries take a token of any lifetime
```

The commits are thematic, not individually buildable: the stage changes several
signatures at once (`implementation_error`/`staging_error`, `stage_invocation`,
`parse_group`, `make_group_parser`, `make_paragraph_break_node`, `StopCause`), and
their callers live in the later commits. The branch tip is green on every gate.

### What changed, per file

| File | Change |
|---|---|
| `techy/src/token/reader.rs`, `token/list_reader.rs` | `move_to_edge`, `source_span_between`, `source_span_of`, `position_at` take `&Token<'_, L>` (any lifetime) instead of the reader's own `'s` — see deviation 1 |
| `techy/src/constructs/mod.rs` | `ParseContext::here()` and `::source_span_within()` (+ the "spans come from the reader" clause on the type, the `source` field's doc reworded); `implementation_error`/`staging_error` take a `SourceSpan`; `stage_invocation(end: Option<&L::StreamPosition>)` with §1.9's three cases and the two invalid-span aborts; `invocation_frame` from reader spans; `parse_group(open: &Token<'s, L>, ..)`; descent-guard and derive-failure anchors `self.here()`; `Debug` prints `at` (the reader-answered span) instead of `pos`/`source`; the prose naming `move_past`/`move_to_pos` reworded; 6 new unit tests |
| `techy/src/constructs/nodes_parser.rs` | `StopCause<L>` (`SourceSpan` + `after`, manual `Debug`/`Clone`/`PartialEq`/`Eq`, no longer `Copy`); `NodesOutcome::stop: StopCause<L>`; the chars run as `Option<(L::StreamPosition, L::StreamPosition)>` (`take_pre_space`/`extend_run`/`flush_through`/`flush_for_token_stop` take the token, the shared `extend_run_to` reports the gap with both positions); `stage`/`stage_node` take a `SourceSpan`; every anchor and every move ported; the harness reports its exit position; assertions on a cause's span go through the new `stop_shape` helper; 1 new test (`after` skips the unconsumed token) |
| `techy/src/constructs/group_parser.rs` | `GroupParser<'p, 's, L> { open: Token<'s, L> }`; end position from `StopCause::after` / `position_here()`; node span via `source_span_within`; the recorded delimiters through `same_source` (else `TextContent::Owned`) |
| `techy/src/constructs/argument_parsers.rs` | `ArgumentNoise::start: L::StreamPosition` + `rewind`; `stage_pre_space(cx, nodes, tok)`; free `stage(.., SourceSpan)`; the five `cx.here()` anchors; the marker run on positions; all moves by edge |
| `techy/src/constructs/invocation_parser.rs` | `parse_declared_arguments(.., name: &SourceSpan)`; the argument frame anchored at `cx.here()`; the two prose sentences naming deleted methods reworded |
| `techy/src/constructs/attached_source.rs` | the stray-close arm reads `span.content()`, resumes at `after`, stages with the cause's span; the fixtures take the test's own `Arc<Source>` (`with_context` lends it) |
| `techy/src/engine/language.rs` | the same root-loop arm; the contract-violation anchors; the root staging error anchored at the source's start; the `BogusLang` test cause built from `cx.here()` |
| `techy/src/engine/driver.rs` | `make_group_parser<'p, 's>(open: &Token<'s, L>, ..) where 's: 'p`; `make_paragraph_break_node(state, break_span: &SourceSpan)` |
| `techy/src/scopes/mod.rs` | `ErrorInvocationParser` takes its trigger's span from the reader (recover + chars fallback + staging error) |
| `techy/src/latexlike/invariants.rs` | the `end_pos: Some` doc comment → `end: Some(&position)` |
| **2b-owned, forced by the signatures** — `constructs/{embellishments,environment,tack_on,verbatim,chars_group}_parser.rs`, `latexlike/{driver,environments,input,invocation_syntax}.rs`, `engine/mod.rs`, `docs/construct-parsers.md` | the minimum that compiles: `cx.here()` where the site anchored at `Span::empty(cx.tokens.pos())`, `cx.tokens.source_span_of(&token)` where the span was a whole token's, otherwise the interim wrap `SourceSpan::new(&cx.source, ..)` that 2b's sweep finds; the `stage_invocation`/`stage_pre_space`/`parse_group`/`make_paragraph_break_node` callers ported properly (positions cannot be forged) |

### Gate results (verbatim)

```
$ cargo build
   Compiling techy v0.1.0 (…/bt-2a-core/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.30s

$ cargo test
     Running unittests src/lib.rs (target/debug/deps/techy-94158093885f6495)
running 1035 tests
test result: ok. 1035 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s
     Running tests/acceptance.rs
running 30 tests
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
     Running tests/derive_conditions.rs
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/lang_features.rs
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/recompose_oracle.rs
running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/serialize_golden.rs / serialize_perf.rs / serialize_stream.rs
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out (each)
     Running unittests src/lib.rs (techy_derive)
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests techy
running 89 tests
test result: ok. 84 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 21.51s
   Doc-tests techy_derive
running 2 tests
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo clippy --all-targets -- -D warnings
    Checking techy v0.1.0 (…/bt-2a-core/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.19s
(clean — no warnings, exit 0)

$ rm -rf target/doc && cargo docs
 Documenting techy-derive v0.1.0 (…/bt-2a-core/techy-derive)
 Documenting techy v0.1.0 (…/bt-2a-core/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.46s
   Generated …/target/doc/techy/index.html and 1 other file
(no broken intra-doc links)

$ cargo test -p techy --lib constructs::nodes_parser
running 79 tests
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 956 filtered out; finished in 0.02s

$ cargo test -p techy --lib token::list_reader
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 1023 filtered out; finished in 0.00s
```

### Semver report (`scripts/check_semver.sh`)

Breaking changes are expected (soft freeze) and were not "fixed".

```
    Checking techy v0.1.0 -> v0.1.0 (no change; assume minor)
     Checked [   0.044s] 196 checks: 182 pass, 14 fail, 0 warn, 58 skip
     Summary semver requires new major version: 14 major and 0 minor checks failed
    Finished [   9.631s] techy
```

The 14 failing lints — the nine Stage 1 already reported, plus five from this stage:

| Lint | Item | Stage |
|---|---|---|
| `auto_trait_impl_removed` | `BeginSpec` no longer `UnwindSafe`/`RefUnwindSafe` | pre-existing |
| `constructible_struct_adds_field` | `StdCallableSpec.provenance` (pre-existing); `TokenRecovery.resume` | 1 |
| `constructible_struct_adds_private_field` | `SpecialsSpec.provenance` | pre-existing |
| `struct_pub_field_missing` | `TokenRecovery::resume_pos`; `SpecialsMatch::name` | 1 |
| `trait_added_supertrait` | `CallableSpec`/`SpecsProvider` gained `SerializableObject` | pre-existing |
| `trait_associated_type_added` | `Lang::StreamPosition` | 1 |
| `trait_method_added` | `ParseDriver::make_token_reader` | 1 |
| `trait_method_parameter_count_changed` | `ParseDriver::probe_token` 4 → 3 (Stage 1); `ParseDriver::make_paragraph_break_node` 3 → 2 | 1, **2a** |
| `type_mismatched_generic_lifetimes` | `SpecialsMatch` 1 → 0 (Stage 1); `GroupParser` 1 → 2 lifetimes | 1, **2a** |
| `derive_trait_impl_removed` | `StopCause` no longer derives `Copy` | **2a** |
| `enum_struct_variant_field_added` | `StopCause::{TokenCondition, UnexpectedGroupClose}::after` | **2a** |
| `trait_requires_more_generic_type_params` / `type_requires_more_generic_type_params` | `StopCause` 0 → 1 generic type | **2a** |
| `function_parameter_count_changed` | `techy::latexlike::make_paragraph_break_node` 4 → 3 | **2a** |

### The 2a completion grep

```
$ grep -n "cx\.source\b\|self\.source\b\|SourceSpan::new(&cx\.source\|Span::empty(cx\.tokens\.pos())\|move_to_pos\b\|\.pos()\|move_past\|move_to(&" \
    techy/src/constructs/{mod,nodes_parser,group_parser,argument_parsers,invocation_parser,attached_source}.rs \
    techy/src/engine/language.rs techy/src/engine/driver.rs
techy/src/constructs/argument_parsers.rs:2006:            fn move_past(&mut self, tok: &Token<'s, ArgLang>, skip_post_space: bool) {
techy/src/constructs/argument_parsers.rs:2007:                self.inner_mut().move_past(tok, skip_post_space);
techy/src/constructs/argument_parsers.rs:2010:            fn move_to(&mut self, tok: &Token<'s, ArgLang>, rewind_pre_space: bool) {
techy/src/constructs/argument_parsers.rs:2014:            fn move_to_pos(&mut self, pos: usize) {
techy/src/constructs/argument_parsers.rs:2015:                self.inner_mut().move_to_pos(pos);
techy/src/constructs/argument_parsers.rs:2019:                self.inner().pos()
techy/src/constructs/nodes_parser.rs:2485:            let pos = self.inner().pos();
techy/src/constructs/nodes_parser.rs:2497:        fn move_past(&mut self, tok: &Token<'s, TestLang>, skip_post_space: bool) {
techy/src/constructs/nodes_parser.rs:2498:            self.inner_mut().move_past(tok, skip_post_space);
techy/src/constructs/nodes_parser.rs:2501:        fn move_to(&mut self, tok: &Token<'s, TestLang>, rewind_pre_space: bool) {
techy/src/constructs/nodes_parser.rs:2505:        fn move_to_pos(&mut self, pos: usize) {
techy/src/constructs/nodes_parser.rs:2506:            self.inner_mut().move_to_pos(pos);
techy/src/constructs/nodes_parser.rs:2510:            self.inner().pos()
techy/src/constructs/nodes_parser.rs:2654:            let pos = self.inner.pos();
techy/src/constructs/nodes_parser.rs:2670:        fn move_past(&mut self, tok: &Token<'s, TabooLang>, skip_post_space: bool) {
techy/src/constructs/nodes_parser.rs:2671:            self.inner_mut().move_past(tok, skip_post_space);
techy/src/constructs/nodes_parser.rs:2674:        fn move_to(&mut self, tok: &Token<'s, TabooLang>, rewind_pre_space: bool) {
techy/src/constructs/nodes_parser.rs:2678:        fn move_to_pos(&mut self, pos: usize) {
techy/src/constructs/nodes_parser.rs:2679:            self.inner_mut().move_to_pos(pos);
techy/src/constructs/nodes_parser.rs:2683:            self.inner().pos()
```

Every remaining hit is a **required trait method of a `cfg(test)` delegating reader**
(`BrokenReader`, `StuckRecoveryReader`, `TabooReader`): `TokenReader` still declares
`move_past`, `move_to(tok, bool)`, `move_to_pos` and `pos`, so an implementor must
provide them until 2b deletes them; the bodies only forward to the inner reader. No
production site, no `cx.source`/`self.source` read, no `SourceSpan::new(&cx.source, ..)`,
no `Span::empty(cx.tokens.pos())` anywhere in the 2a-owned files. (`\b` is needed after
`move_to_pos` in the grep — otherwise it also matches the *new* `move_to_position`.) The
`ParseContext.source` field itself remains, declared at `constructs/mod.rs:154` and
initialized at `:194`, read nowhere in these files; 2b removes it.

### Decisions taken under §1.16

- **`StopCause` gains an `L` parameter** with **manual** `Debug`/`Clone`/`PartialEq` and
  a plain `impl Eq` — a derive would demand `L:` bounds. It also loses `Copy`
  (a `SourceSpan` holds an `Arc`), so `NodesOutcome::clone` clones the cause.
- **Chars-run contiguity failure** stays an `ImplementationError`, now naming both
  positions' `Debug` renderings ("… starts at `StdStreamPosition(5)`, which is not
  where the pending chars run ends (`StdStreamPosition(4)`) …").
- **`Invocation.kind` is not added here** — the view arrives in Stage 3a; this stage
  only ports positions and spans, kind matching still reads `token.kind`.
- **`ArgumentNoise` keeps `next: Option<Token<'s, L>>`**, as prescribed.
- Test-only choices: the nodes-parser harness reports the reader's exit position
  *and* its byte offset (`cx.here().start()`), so numeric assertions survive and the
  re-peek tests resume a fresh reader over the same content with
  `move_to_position`; a stop cause's variant + span range are asserted through a
  `stop_shape` helper (the `after` position is opaque and is checked by resuming
  from it instead).

### Deviations from §1/§4

1. **The reader's token-taking queries take `&Token<'_, L>`, not `&Token<'s, L>`**
   (`move_to_edge`, `source_span_between`, `source_span_of`, `position_at`). §1 spells
   them `&L::Token`, which in the final tree carries no lifetime; the mechanical Stage 2a
   transcription `&Token<'s, L>` (the reader's own `'s`) **does not compile** at the sites
   this stage must port. A `ConstructParser::parse` receives `cx: &mut ParseContext<'_, '_, L>`
   whose `'s` is fresh per call and unrelated to the `'s` of the `Invocation`/`Token`
   stored in the parser, and `ParseContext` is invariant in `'s` (`&mut dyn TokenReader<'s, L>`),
   so `cx.tokens.source_span_of(self.invocation.token)` cannot type-check with the tied
   spelling. Since none of these four methods borrows anything from the token (they read
   the reader's record of where it is), the untied spelling is sound, is what §1's
   `&L::Token` means, and disappears in Stage 3b. The tied spelling is kept where it is
   real: `peek`/`next` still return `Token<'s, L>`.
   Consequence: `GroupParser` gains an `'s` (`GroupParser<'p, 's, L>`) and
   `make_group_parser` an `'s` with `'s: 'p`, both dropped again in 3b.
2. **`parse_declared_arguments` takes `name: &SourceSpan<L::SourceOrigin>`** (was
   `name_span: Span`). §1.11 does not list it, but it is in a 2a-owned file and paired a
   bare `Span` with `cx.source` for the argument frames' title. Its three callers are
   adapted (properly in `latexlike/input.rs`, with the interim wrap in the two
   environment parsers, which 2b ports).
3. **`ParseContext`'s `Debug` prints `at` instead of `pos` + `source`** — one
   reader-answered `SourceSpan` carries both facts, and the `source` field it printed is
   removed in 2b.
4. **The latexlike `Specials` paragraph-break payload comes from `specials_form()`**
   (orchestrator ruling, 2026-08-17). The hook now receives only the break's span, and a
   paragraph break *is* a specials-formed callable
   (`callable_type: specials_callable()`), so the behavior function mints its payload
   with `LLL::InvocationSyntax::specials_form()`
   ([`LatexlikeInvocationSyntax`](techy/src/latexlike/lang.rs)) instead of consulting
   `FromInvocation` over a synthesized token. For the preset that is byte-for-byte the
   payload `from_invocation` answered for a `ParagraphBreak` trigger (the unit
   `Specials` variant — its `_ =>` arm), confirmed by the unchanged paragraph-break
   tests (`paragraph_break_pillar_is_the_driver_behavior`,
   `paragraph_breaks_can_emit_specials_nodes`,
   `specials_and_paragraph_breaks_reemit_name_as_written`,
   `paragraph_breaks_round_trip_in_both_styles`). No token, no reader, nothing for a
   later stage to revisit; `name = break_span.content()` is unchanged. *(An earlier
   revision of this branch rebuilt a `ParagraphBreak` token from the span to reach
   `FromInvocation`; that interim is gone.)*
5. **`stage_invocation`'s invalid-span error keeps its own wording** (an
   `invocation_span_within` helper) rather than reusing
   `ParseContext::source_span_within`'s message, because §1.9 pins the "invalid computed
   span" wording (and `latexlike/invocation_syntax.rs` asserts on it).
6. **The bad-end test loses two of its four cases.** `stage_invocation`'s explicit end is
   a stream position now, so an end *outside the content* or *off a character boundary*
   cannot be expressed at all — the reader only ever hands out valid positions. The
   remaining case (an end before the trigger's start, taken from the trigger's own
   pre-space edge) is exercised strict and tolerant, plus once over multi-byte content.
   The `SourceSpan::new` assert that test used to guard against is now unreachable from
   this path by construction. The test also pins the error's **anchor** (the trigger's
   span, `3..8`).

### Open questions

1. **No existing test's expected node span changed.** The §1.9 rule was implemented as
   written and the whole suite passes unmodified (1035 unit + 75 integration + 84
   doctests), including the environment/`\input`/expression-position span assertions and
   the parse-tree byte-partition oracle. Nothing to rule on — recorded because §1.9 asked
   for it explicitly.
2. ~~The paragraph-break hook and `FromInvocation`~~ — **closed** (orchestrator,
   2026-08-17): no reader and no token are needed, because the payload of a
   specials-formed callable is `LLL::InvocationSyntax::specials_form()`. Implemented as
   deviation 4; nothing is left for Stage 3b here.
3. **`GroupParser`'s extra lifetime** (deviation 1) is churn that 3b undoes. If a
   reviewer prefers, the alternative is for `GroupParser` to store the open token's
   `SourceSpan` plus its `Start` position instead of the token — no lifetime, but it
   diverges from §1.9's "`GroupParser::new(open: L::Token, rule)`" and would have to be
   put back in 3b.

### Review round 1 (fixes applied 2026-08-17)

The stage came back READY subject to three required fixes and two adopted
suggestions; all five are on the branch.

1. **This commit list** was written with a placeholder for its own SHA and predates two
   commits. It is exact now: every earlier commit by SHA, the newest one (this PROGRESS
   update) named in the sentence above the block.
2. **The superseded `end_pos` spelling** left the remaining prose and comments:
   `latexlike/invocation_syntax.rs:743, 902, 927`, `docs/construct-parsers.md:229`,
   `docs/ai-guide-custom-lang.md:282` — all now `end` / `end: Some(&position)`.
   `grep -rn "end_pos" techy docs` is **not** empty, and should not be: what remains is
   the unrelated S0 accessor [`SourceSpan::end_pos`](techy/src/source/source.rs)
   (`source.rs:274,348,648,652,653,748,750,751`, `line_index.rs:183` — the sibling of
   `start_pos`, which no stage of this plan renames) and the substring inside the local
   name `end_position` (`latexlike/serialize_tests.rs:529,538`,
   `docs/construct-parsers.md:412,425,428,489`). Filtering those two out leaves nothing:
   `grep -rn "end_pos" techy docs | grep -v end_position | grep -v "source/source.rs" | grep -v line_index.rs`
   prints no lines.
3. **The invalid-computed-span abort is anchored at the trigger again.** The port had it
   at `self.here()`; on `main` it was the trigger's span, which is the meaningful
   location for a report about a construct. `invocation_span_within` and
   `invalid_invocation_span` take the trigger span and anchor there in every raising
   path of `stage_invocation`, and the bad-end test asserts `error.span().range() == 3..8`.
4. *(adopted suggestion)* New test
   `stage_invocation_ignores_a_last_child_staged_in_another_source`: a child staged on a
   second `Arc<Source>` cannot end the node, so the standard rule falls through to the
   reader's position (`0..1`, as for a childless shape).
5. *(adopted suggestion)* The `TakeParser` test parser records its group delimiters
   through `same_source` (else `TextContent::Owned`), modelling §1.12 the way the
   production sites do.

Noted, not acted on: the per-node `SourceSpan` clones belong to 2b's timing gate, and
the remaining `token.pre_space` reads are Stage 3a's.

---

## Stage 2b — the rest of the port, and the old API deleted (§4)

- **Branch**: `bt-2b-rest` (off `bt-2a-core` at `0af4276`, which `main` has since
  fast-forwarded to).
- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/bt-2b-rest`.
- **Status**: reviewed and merged (`main` a8f36a1). Date: 2026-08-17.
- **Commits** (`git log --oneline d736e72..a8f36a1`, newest first — the post-rebase
  SHAs, review polish and the PROGRESS update included):

```
a8f36a1 bettertokens: review polish — one recording rule, sharper progress notes
85e1466 bettertokens: PROGRESS.md — Stage 2a merged, Stage 2b implemented
16f6d60 bettertokens: the parse-throughput probe example
d201e96 core: `ParseContext` holds no source
8f852b7 token: `move_to_edge` is `move_to`
e6d47d6 token: delete the positional navigation API
bcf2ea1 core: the last construct-parser callers and the guides off bare positions
4cdb737 core: the environment and verbatim families on reader answers
```

### What changed, per file

| File | Change |
|---|---|
| `techy/src/constructs/environment_parser.rs` | `NameGroup { name: SourceSpan, end: L::StreamPosition }`; `EnvironmentBody::end: L::StreamPosition`; `EnvironmentBeginSyntaxData`/`EnvironmentTerminatorSyntaxData` fields are `SourceSpan`s; `EnvironmentBodyParser::{new, with_invocation_name_span}` take `SourceSpan`s; `read_rigid_name_group` rewinds with `move_to(&open, StartBeforePreSpace)`, `read_name_chars` runs on positions; the terminator flow uses `position_here()`, an equality drift check and `move_to(&end_token, Start)`; the body's content end is the reader's position at the stop (deviation 1); the test `RawBlockParser` uses `move_to(trigger, End)` |
| `techy/src/constructs/verbatim_parser.rs` | `RawContentEnd<L> { content_end, terminator: Option<SourceSpan>, end }` (all reader answers, §1.16 decision); `VerbatimBodyTerminator::syntax_data(span, end)`; `VerbatimBodyParser` takes `SourceSpan`s; the three `entry`/`move_to_pos(entry)` no-ops deleted (§1.16 decision); the group's delimiters recorded through `node_text_content` |
| `techy/src/constructs/embellishments_parser.rs` | the marker over-scan keeps the best-so-far **token** and returns a `SourceSpan`; the wrapper group's span comes from the marker span and the last staged child (same-source-checked); `move_to(&tok, EndPastPostSpace)` |
| `techy/src/constructs/tack_on_parser.rs`, `chars_group_parser.rs` | the last `move_past` and the repeated-field span (`source_span_of`) |
| `techy/src/constructs/group_parser.rs` | the delimiter recording now calls the shared `node_text_content` (was written out inline in 2a) |
| `techy/src/constructs/mod.rs` | new `pub(crate) fn node_text_content(fact, node_span)` — the one spelling of §1.12's node-data rule; `ParseContext::source` and the `new` parameter removed, the type's rustdoc says so |
| `techy/src/latexlike/environments.rs` | `EnvironmentInvocation<'p, LLL>` carries `SourceSpan`s (manual `Clone`/`Debug`, no longer `Copy`); the composition takes the trigger's command/post-space spans from the reader, computes the node span before the payload and hands it to `from_parsed`; `parse_declared_arguments(&name_group.name)`; `OrphanEndParser` on positions |
| `techy/src/latexlike/invocation_syntax.rs` | `EnvironmentSyntax::from_parsed(begin, terminator, node_span)` — the record converts each source-qualified fact against the node's own span (deviation 2); the test `RestOfLineParser` reads `Char` tokens under a features-off state |
| `techy/src/latexlike/input.rs` | `argument_text_span` answers a `SourceSpan` (same-source-checked), so the `\input` reference is read off it |
| `techy/src/engine/mod.rs` | the test one-char parser stages the reader's span; the context-helper's source parameter dropped |
| `techy/src/token/reader.rs` | `move_past`, the two-flag `move_to`, `move_to_pos` and `pos` deleted from the trait, from `StdTokenReader`'s inherent methods and from its impl; `move_to_edge` renamed `move_to`; `next`'s default is `peek` + `move_to(EndPastPostSpace)`; the movement test restated in edges; test helpers `seek`/`at` (the reader's own module is where positions may be minted from offsets) |
| `techy/src/token/list_reader.rs` | the same four methods and the inherent `move_to_pos` deleted, with their issued-offset bookkeeping; the lockstep movement test restated in edges; the mid-pre-space peek test lands there by consuming a hand-built filler token |
| `techy/src/constructs/{nodes_parser,argument_parsers}.rs`, `techy/tests/lang_features.rs` | the delegating test readers lose the four methods; `StuckRecoveryReader`/`TabooReader` read their offset off `position_here()` |
| `docs/construct-parsers.md`, `docs/ai-guide-custom-lang.md`, `docs/parsing-model.md` | the deleted names are gone: `ParseContext` bundles four inputs, repositioning is `move_to`/`move_to_position`, the `\verb` idiom is `move_to(token, TokenEdge::End)` |
| `techy/examples/bt_timing.rs` | new: the throughput probe (deleted in Stage 4) |

### Gate results (verbatim)

```
$ cargo build
   Compiling techy v0.1.0 (…/bt-2b-rest/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.42s

$ cargo test
     Running unittests src/lib.rs (target/debug/deps/techy-94158093885f6495)
running 1035 tests
test result: ok. 1035 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.68s
     Running tests/acceptance.rs
running 30 tests
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/derive_conditions.rs
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/lang_features.rs
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/recompose_oracle.rs
running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/serialize_golden.rs / serialize_perf.rs / serialize_stream.rs
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out (each)
     Running unittests src/lib.rs (techy_derive)
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests techy
running 89 tests
test result: ok. 84 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 21.93s
   Doc-tests techy_derive
running 2 tests
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
(clean — no warnings, exit 0)

$ rm -rf target/doc && cargo docs
 Documenting techy-derive v0.1.0 (…/bt-2b-rest/techy-derive)
 Documenting techy v0.1.0 (…/bt-2b-rest/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.55s
   Generated …/target/doc/techy/index.html and 1 other file
(no broken intra-doc links)

$ cargo test -p techy --lib constructs::nodes_parser
running 79 tests
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 956 filtered out; finished in 0.04s

$ cargo test -p techy --lib token::list_reader
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 1024 filtered out; finished in 0.00s
```

### Semver report (`scripts/check_semver.sh`)

Breaking changes are expected (soft freeze) and were not "fixed".

```
    Checking techy v0.1.0 -> v0.1.0 (no change; assume minor)
     Checked [   0.045s] 196 checks: 179 pass, 17 fail, 0 warn, 58 skip
     Summary semver requires new major version: 17 major and 0 minor checks failed
    Finished [   3.023s] techy
```

The 17 failing lints — the 14 Stage 1 and 2a already reported, plus three lints this
stage adds entries to:

| Lint | Item | Stage |
|---|---|---|
| `auto_trait_impl_removed` | `BeginSpec` no longer `UnwindSafe`/`RefUnwindSafe` | pre-existing |
| `constructible_struct_adds_field` | `StdCallableSpec.provenance` (pre-existing); `TokenRecovery.resume` (1); **`NameGroup.name`** | 1, **2b** |
| `constructible_struct_adds_private_field` | `SpecialsSpec.provenance` | pre-existing |
| `derive_trait_impl_removed` | `StopCause` no longer `Copy` (2a); **`EnvironmentInvocation` no longer `Copy`** | 2a, **2b** |
| `enum_struct_variant_field_added` | `StopCause::{TokenCondition, UnexpectedGroupClose}::after` | 2a |
| `function_parameter_count_changed` | `techy::latexlike::make_paragraph_break_node` 4 → 3 | 2a |
| **`inherent_method_missing`** | **`StdTokenReader::{pos, move_to_pos}`** | **2b** |
| **`method_parameter_count_changed`** | **`ParseContext::new` 5 → 4** | **2b** |
| `struct_pub_field_missing` | `TokenRecovery::resume_pos`, `SpecialsMatch::name` (1); **`NameGroup::name_span`, `ParseContext::source`** | 1, **2b** |
| `trait_added_supertrait` | `CallableSpec`/`SpecsProvider` gained `SerializableObject` | pre-existing |
| `trait_associated_type_added` | `Lang::StreamPosition` | 1 |
| `trait_method_added` | `ParseDriver::make_token_reader` | 1 |
| **`trait_method_missing`** | **`TokenReader::{move_past, move_to_pos, pos}`** (the two-flag `move_to` keeps the name, with a new signature) | **2b** |
| `trait_method_parameter_count_changed` | `ParseDriver::probe_token` 4 → 3 (1); `make_paragraph_break_node` 3 → 2 (2a); **`EnvironmentSyntax::from_parsed` 2 → 3** | 1, 2a, **2b** |
| `trait_requires_more_generic_type_params` / `type_requires_more_generic_type_params` | `StopCause` 0 → 1 generic type | 2a |
| `type_mismatched_generic_lifetimes` | `SpecialsMatch` 1 → 0 (1); `GroupParser` 1 → 2 (2a) | 1, 2a |

### The sweep (§4)

```
$ grep -rn move_to_edge techy docs
(no output; exit 1)
```

```
$ grep -rn "move_to_pos\|\.pos()\|move_past\|cx\.source\b\|self\.source\b\|&source, " \
    techy/src techy/tests docs
```

Every hit is on the §4 table's legitimate list. Filtering out the two classes that
dominate it — `SourceSpan::new(&source, …)` built from a local `source` binding in
tests and doc examples, and the S0 module's and node module's own files — leaves only:

- `move_to_position` (the loose pattern `move_to_pos` matches the *new* method's name)
  — trait declaration and impls, the delegating test readers, callers, rustdoc;
- the readers' own `self.source` fields: `techy/src/token/reader.rs` (`StdTokenReader`)
  and `techy/src/token/list_reader.rs` (`TokenListReader`);
- `SourcePos::pos()`: `techy/src/source/source.rs`, `techy/src/node/tree.rs`, and two
  new call sites — the reader suite's `at` helper (`token/reader.rs:925`) and the
  verbatim body test's byte-offset readback (`verbatim_parser.rs:1363`);
- `cx.source(wire.source)` at `techy/src/serialize/drivers/source.rs:382` (a
  `DeserializeContext`, an unrelated method);
- `SourceSpan::entire(&source)` in `techy/src/engine/language.rs` (the parse's own
  binding);
- `Error::source`'s `Some(&self.source)` in `techy/src/error.rs:1509` (a `#[cfg(test)]`
  error type's std-trait impl — not on the §4 list because it is not new; it matches
  the pattern by accident).

```
$ grep -rn "cx\.source\b\|self\.source\b" \
    techy/src/constructs techy/src/engine techy/src/latexlike techy/src/scopes
techy/src/latexlike/node_ref.rs:83:            Some((_escape_char, post_space)) => post_space.resolve(self.source()),
```

§4 also asks for the docs-only form of the grep. It is **not** empty, and the two lines
it prints are **not** a regression — both are the *new* names, matched as substrings of
the old ones:

```
$ grep -rn "move_to_pos\|move_past\|\.pos()\|cx\.source" docs
docs/construct-parsers.md:62:([`move_to_position`](crate::core::TokenReader::move_to_position)). Prefer
docs/construct-parsers.md:457:        let content: SourceSpan = cx.source_span_within(&start, &content_end)?;
```

`move_to_pos` matches inside `move_to_position`, and `cx.source` inside
`cx.source_span_within`. Anchoring the two patterns at a word boundary — which is how
the same grep is written for `techy/src` — gives the empty result §4 describes:

```
$ grep -rn "move_to_pos\b\|move_past\|\.pos()\|cx\.source\b" docs
(no output; exit 1)
```

The single hit is `NodeRef::source()` — a node view resolving a `TextContent` against
the node's own source, nothing to do with a parse context or a reader. `cx.source`
itself has **zero** hits anywhere. (The delegating `#[cfg(test)]` readers reach their
inner reader as `self.inner`, so none of them matches this pattern.)

```
$ grep -rn "fn move_to\b\|fn move_to_position\|fn move_past\|fn move_to_pos\|fn pos(" \
    techy/src/token/reader.rs
techy/src/token/reader.rs:186:    fn move_to(&mut self, tok: &Token<'_, L>, edge: TokenEdge);
techy/src/token/reader.rs:194:    fn move_to_position(&mut self, at: &L::StreamPosition);
techy/src/token/reader.rs:721:    fn move_to(&mut self, tok: &Token<'_, L>, edge: TokenEdge) {
techy/src/token/reader.rs:725:    fn move_to_position(&mut self, at: &L::StreamPosition) {
```

**Confirmed**: `TokenReader` now has exactly two navigation methods — `move_to(&tok,
edge)` and `move_to_position(&pos)` — declared once each (186, 194) and implemented once
each by `StdTokenReader` (721, 725).

### Timing check (§4)

`techy/examples/bt_timing.rs` generates a deterministic 5 242 901-byte LaTeX-like
document (LCG seed `0x5eed1234`; plain words, `\emph{…}`/`\textbf{…}`, argument-less
commands, `{groups}`, `% comments`, blank-line paragraph breaks, `\begin{itemize}…
\end{itemize}` blocks) and parses it with `Language<Latexlike>` + `LatexlikeDriver`
under strict recovery, with `builtin_package()` and a small definitions package so the
parse is diagnostic-free. Both trees report the **same** 257 816 root children and 0
diagnostics — an incidental check that the port changed no tree.

- Branch `bt-2b-rest` (ms): **213.0, 214.8, 212.1, 206.6, 193.4** → median **212.1**
- Baseline `main` at `7825789` (ms): **206.3, 189.0, 205.2, 192.2, 210.1** → median
  **205.2**

Slowdown at the median: **+3.4 %** — within the ≤ 10 % acceptance. No optimization
was needed; the per-node `SourceSpan` clones the 2a reviewer flagged and the chars-run
path were left as they are. (Baseline worktree
`/Users/philippe/projects/techy/.claude/worktrees/bt-timing-main`, created detached at
`7825789` with the same example copied in untracked, then
`git worktree remove --force`d.)

### Decisions taken under §1.16

- **`RawContentEnd` shape** (§1.11 leaves it to the implementer): a struct generic in
  `L` with three fields — `content_end: L::StreamPosition`, `terminator:
  Option<SourceSpan>`, `end: L::StreamPosition`. The third field is new: the callers
  need both "where the content stopped" and "where the consumed terminator ended", and
  a `SourceSpan` cannot be turned back into a stream position.
- **The `verbatim_parser.rs` `entry`/`move_to_pos(entry)` pairs are deleted, not
  respelled.** `ParseContext::probe_token` never moves the stream (strict aborts,
  tolerant reports `None` "without diagnosing or consuming"), so all three were no-ops.
  A comment at the parser's entry states the invariant they were guarding ("each
  'argument absent' exit consumes nothing").
- **The node-data conversion has one spelling**: `constructs::node_text_content(fact,
  node_span)` — `TextContent::Spanned(fact.span())` when `fact.same_source(node_span)`,
  `TextContent::Owned(fact.content().into())` otherwise. It replaces 2a's inline form in
  `group_parser.rs` and serves the verbatim group, the embellishment wrapper and the
  environment record.
- **`EnvironmentInvocation` loses `Copy`** (it holds two `SourceSpan`s now) and gains
  the `LLL` parameter, with manual `Clone`/`Debug`. Its one double use in the
  composition clones.

### Deviations from §1/§4

1. **The environment body's content end is the reader's position at the stop, not the
   last staged node's span end.** §1.11 keeps `EnvironmentBody::end` as a stream
   position, but the body list's span was computed from `outcome.nodes.last()`'s
   *source* end — and a source offset cannot become a stream position. The content loop
   leaves every stop token unconsumed at its own `Start` (`nodes_parser.rs`: the
   `TokenCondition`, `EndOfInput` and `GroupClose` arms all `move_to(&token, Start)`
   after flushing the token's pre-space as content), so `position_here()` right after
   `parse_nodes` **is** the end of the last staged node under the gap-free tiling
   contract. The whole suite passes unchanged, including the environment span
   assertions and the byte-partition oracle, so no expected node span moved.
2. **`EnvironmentSyntax::from_parsed` gains a `node_span: &SourceSpan` parameter.**
   §1.11 says the composition converts the terminator facts against the node's span,
   but the conversion happens where `TextContent` is minted, which is inside
   `from_parsed` — a public trait method a third-party `Env` type also implements.
   Passing the node's span in is the smallest change that lets *every* implementation
   apply §1.12's rule; the composition computes the node span before the payload (it
   was already computing the same range one statement later, for staging). The begin
   side is checked too, though under a single-source reader it can never fail.
3. **`EnvironmentBodyParser::new` / `VerbatimBodyParser::new` / their
   `with_invocation_name_span`, `EnvironmentInvocation`'s two span fields, and
   `EnvironmentBeginSyntaxData`'s `command_word`/`post_space` fields take
   `SourceSpan`s.** §1.11 lists only the *terminator* type
   (`EnvironmentTerminatorSyntaxData`) and none of the parser constructors, but every
   one of these was a bare `Span` handed across a seam and paired with `cx.source` on
   the other side — the exact pattern this stage removes. The begin data in particular
   mirrors the terminator data field for field (its own rustdoc says so) and is handed
   to the same `from_parsed`, so leaving it a bare `Span` would have split one record's
   two sides across two conventions. Same reasoning as 2a's deviation 2
   (`parse_declared_arguments`).
4. **`latexlike/input.rs`'s `argument_text_span` answers a `SourceSpan`** (it answered a
   bare `Span` the caller indexed into `cx.source`). It now also answers `None` when the
   argument's content nodes lie in more than one source — there is no single extent to
   report then. Unreachable under the std reader.
5. **`docs/construct-parsers.md`'s `cx.tokens` paragraph and `docs/parsing-model.md`'s
   context bullet were reworded** beyond the strictly forced minimum (they listed the
   five inputs and the three deleted methods). Both are single sentences naming deleted
   things; the FAQ rewrite §1.15 asks for is still Stage 4's.
6. **Two reader-suite test helpers, `seek` and `at`** (`token/reader.rs`), stand in for
   the deleted `move_to_pos`/`pos`. The scanner's own tests must start mid-content and
   read the position back as a number; `StdStreamPosition::at`/`offset` are `pub(crate)`
   and this is their own module. No non-test code mints a position from an offset.
7. **The deletion and the rename are two commits, not one.** §4 asks for the rename
   `move_to_edge` → `move_to` "in ONE commit"; it is one commit (`63c2100`), but it is
   *separate* from the deletion commit (`4b3f29c`) that removes `move_past`, the
   two-flag `move_to`, `move_to_pos` and `pos` — the rename cannot land until the old
   `move_to` name is free, so the two cannot be merged into a single commit. Because of
   that ordering, `docs/construct-parsers.md` line 60 and `token/list_reader.rs`'s
   module doc link `move_to_edge` for exactly one commit (`4b3f29c`) and become
   `move_to` in `63c2100`; `broken_intra_doc_links = deny` forbids linking a method
   that does not exist yet. **At the branch tip both names are correct and
   `grep -rn move_to_edge techy docs` is empty.**

### Open questions

1. **`EnvironmentSyntax::from_parsed`'s new parameter** (deviation 2) is a public trait
   signature change §1 did not spell out. The conservative alternative — leave
   `from_parsed` at two parameters and have the composition pre-convert — cannot work:
   the record is what mints `TextContent`, and the composition has no way to hand it a
   converted `EnvironmentTerminatorSyntaxData` (whose fields are `SourceSpan`s by §1.11).
   Flagged for the reviewer, not decided beyond §1.16.
2. **`TokenListReader::pre_space_clipped_when_peeking_mid_whitespace`** now inserts a
   hand-built filler token so the reader can *land* mid-pre-space through a position it
   issued. With `move_to_pos` gone there is no other way to reach that state from
   outside the reader — which is the point of the design, but it does mean the clipping
   behavior is only reachable in tests through a hand-built list. No action taken.

---

## Stage 3a — the token view (§5)

- **Branch**: `bt-3a-view` (off `main` at `a8f36a1`, which already contains Stages 1,
  2a and 2b).
- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/bt-3a-view`.
- **Status**: reviewed (READY, review round 1 applied); merges together with 3b (§5:
  the temporary name `TokenKindView` must never reach `main`, and 3b removes it).
  Date: 2026-08-17.
- **Commits** (`git log --oneline main..bt-3a-view`, newest first; the PROGRESS update
  and the review-round commit follow this list):

```
507d7da docs: the guide doctest and the new tests for the view
7b94504 core: construct parsers read tokens through the reader's view
d8ef0c1 core: the resolve chain works from the token's view
a118cb9 token: the parser-facing token view and `TokenReader::token_kind`
```

The third commit is the big one: the kind-read port and the two signature changes it
forces (`Invocation.kind`, `FromInvocation`) cannot be split without a non-building
intermediate state. Each commit builds and is green on the gates.

### What changed, per file

| File | Change |
|---|---|
| `techy/src/token/token.rs` | new `TokenKindView<'t, L>` (§1.3's variants and fields, no span fields) with manual `Clone`/`Copy`/`Debug`/`PartialEq`/`Eq`/`Display` and `as_str`; `Token::edge_offset` gains the `ContentStart` arm (comment: the start delimiter's end; command: past the escape character; otherwise the token's start); three new unit tests |
| `techy/src/token/reader.rs` | `TokenEdge::ContentStart` (fifth variant, between `Start` and `End`) with the `≤`-ordering rustdoc; `TokenReader::token_kind<'t>(&self, &'t Token<'_, L>) -> TokenKindView<'t, L> where 's: 't` (required, rustdoc per §1.6/§1.15) and its `StdTokenReader` impl; the trait's "Positions, edges, and spans" section, `move_to`'s docs and contract clause 4 (the foreign-token asymmetry) restated for five edges; seven new unit tests, and the movement test covers the new edge |
| `techy/src/token/list_reader.rs` | `token_kind` (same interpretation, issued-token check first); `EVERY_EDGE` and the lockstep edge matrix cover `ContentStart`; three new tests (view lockstep, the comment's delimiter/content as edges, a forged token rejected) |
| `techy/src/token/mod.rs`, `techy/src/core/mod.rs` | facades export `TokenKindView` (temporary — 3b renames it `TokenKind`); the token module's design highlights say what a token *is* is a reader answer too |
| `techy/src/scopes/mod.rs` | `CallableQuery<'a, L>` (the `'s` is gone): `token: Option<&Token>` → `token_kind: Option<TokenKindView<'a, L>>`, `with_token` → `with_token_kind`, rustdoc per §1.10; one new unit test |
| `techy/src/engine/driver.rs` | `ParseDriver::resolve_command`, `CommandResolver::resolve_command` and `resolve_command_in_scopes` take `token_kind: TokenKindView<'_, L>`; the query is built with `with_token_kind`; rustdoc on why a resolver sees the view |
| `techy/src/serialize/drivers/{tests,tree_tests}.rs`, `techy/src/latexlike/serialize_tests.rs` | the `SpecsProvider` stubs' `CallableQuery<'_, '_, L>` → `CallableQuery<'_, L>` (the dropped `'s`) — nothing else in `techy/src/serialize/**` touches tokens |
| `techy/src/constructs/mod.rs` | `Invocation.kind: TokenKindView<'a, L>` and `name: &'a str` (from the view); `FromInvocation::from_invocation(&Invocation, &dyn TokenReader)`, `stage_invocation` passing `&*self.tokens`; new `pub(crate) comment_node_kind(cx, token)` — the comment node's three sub-spans as three edge answers |
| `techy/src/constructs/nodes_parser.rs` | one `cx.tokens.token_kind(&token)` per loop iteration, matched on in the dispatch and in `token_stop`; `take_pre_space` compares edge positions; `TokenStopKind::Predicate` takes the view; the comment arm stages through `comment_node_kind`; test predicates, harness and `TakeParser` ported |
| `techy/src/constructs/argument_parsers.rs` | the noise scan, `parse_expression_node`, the group-class and minted-group probes and the marker run read the view; the requires-content spelling comes from `invocation.kind`; the two `Invocation` sites set `kind` |
| `techy/src/constructs/child_state.rs` | `GroupChildState::Compute` receives the open token's view (see deviations) |
| `techy/src/constructs/{environment,verbatim,embellishments,tack_on,group,chars_group}_parser.rs` | every kind match through the reader; the "rigid"/"contiguous" pre-space tests become edge comparisons; `read_raw_content`'s close-as-content callback takes the view |
| `techy/src/latexlike/invocation_syntax.rs` | `FromInvocation for InvocationSyntaxData` reads `invocation.kind` and takes the post-space from `tokens.source_span_between(token, End, EndPastPostSpace)`; the rest-of-line test parser reads views; one new unit test |
| `techy/src/latexlike/environments.rs`, `techy/src/latexlike/driver.rs` | the composition's trigger checks read `self.invocation.kind`; the preset's `resolve_command` takes the view |
| `techy/src/engine/mod.rs` | the one-char test parser reads the view; the three `resolve_command` tests pass a `TokenKindView::Command` literal instead of building a token |
| `techy/tests/lang_features.rs` | `CommentEmittingReader` implements `token_kind` by delegation (the documented custom-reader shape, from outside the crate); `FixedTableResolver` takes the view; the groups-only reader test matches on the view and reads pre-space through the reader |
| `docs/construct-parsers.md` | the takeover doctest reads `cx.tokens.token_kind(&token)` and matches `TokenKindView` |

### Gate results (verbatim)

```
$ cargo build
   Compiling techy v0.1.0 (…/bt-3a-view/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.01s
```

```
$ cargo test
     Running unittests src/lib.rs (target/debug/deps/techy-94158093885f6495)
test result: ok. 1050 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s
     Running tests/acceptance.rs (target/debug/deps/acceptance-7b2cb805b784ba43)
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/derive_conditions.rs (target/debug/deps/derive_conditions-71de9c4e022af742)
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/lang_features.rs (target/debug/deps/lang_features-578eec6f554508fe)
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/recompose_oracle.rs (target/debug/deps/recompose_oracle-d968565b496d485e)
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/serialize_golden.rs (target/debug/deps/serialize_golden-cb6930c3a4ed679f)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/serialize_perf.rs (target/debug/deps/serialize_perf-d51c13b362cfaaf8)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/serialize_stream.rs (target/debug/deps/serialize_stream-fb6aa0095a29ae64)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running unittests src/lib.rs (target/debug/deps/techy_derive-630a7db7dcf42893)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests techy
test result: ok. 84 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 26.64s
   Doc-tests techy_derive
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

(1050 unit tests: 1035 at the branch point plus the 15 this stage adds.)

```
$ cargo clippy --all-targets -- -D warnings
    Checking techy v0.1.0 (…/bt-3a-view/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.64s
```

```
$ rm -rf target/doc && cargo docs
 Documenting techy-derive v0.1.0 (…/bt-3a-view/techy-derive)
 Documenting techy v0.1.0 (…/bt-3a-view/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.57s
   Generated …/bt-3a-view/target/doc/techy/index.html and 1 other file
```

Lockstep suites:

```
$ cargo test -p techy --lib constructs::nodes_parser
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 971 filtered out; finished in 0.07s

$ cargo test -p techy --lib token::list_reader
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 1036 filtered out; finished in 0.01s
```

### Semver report (`scripts/check_semver.sh`)

Breaking changes are expected (soft freeze) and were not "fixed".

```
    Checking techy v0.1.0 -> v0.1.0 (no change; assume minor)
     Checked [   0.061s] 196 checks: 179 pass, 17 fail, 0 warn, 58 skip
     Summary semver requires new major version: 17 major and 0 minor checks failed
    Finished [   3.420s] techy
```

The same **17 lints** as Stage 2b — no new lint category. Five of them gain entries
from this stage:

| Lint | Entry added by 3a |
|---|---|
| `constructible_struct_adds_field` | `CallableQuery.token_kind`, `Invocation.kind` |
| `inherent_method_missing` | `CallableQuery::with_token` |
| `struct_pub_field_missing` | `CallableQuery::token` |
| `trait_method_parameter_count_changed` | `FromInvocation::from_invocation` 1 → 2 |
| `type_mismatched_generic_lifetimes` | `CallableQuery` 2 → 1 lifetime params |

Not reported by any lint, but breaking all the same (the tool has no lint for a changed
parameter *type*, a new required trait method on a lifetime-parameterized trait, or a
new enum variant here): `TokenReader::token_kind` (new required method),
`ParseDriver::resolve_command` / `CommandResolver::resolve_command` /
`resolve_command_in_scopes` taking a view, `TokenStopKind::Predicate` and
`GroupChildState::Compute` taking a view, and `TokenEdge::ContentStart`.

### The 3a completion greps

Token *kind* reads outside the token module:

```
$ grep -rn "\.kind\b" techy/src techy/tests docs | grep -v "^techy/src/token/" \
    | grep -v "node\.kind\|\.kind()\|NodeKind\|kind:"
techy/src/constructs/argument_parsers.rs:352:        let spelling = match invocation.kind {
techy/src/constructs/mod.rs:1274:            .field("kind", &self.kind)
techy/src/constructs/nodes_parser.rs:211:        match self.kind {
techy/src/constructs/nodes_parser.rs:763:        let matches = match &cond.kind {
techy/src/constructs/nodes_parser.rs:1293:            .field("kind", &self.kind)
techy/src/latexlike/invocation_syntax.rs:127:        match invocation.kind {
techy/src/latexlike/environments.rs:828:            self.invocation.kind
techy/src/latexlike/environments.rs:1045:        let command_end = match self.invocation.kind {
techy/src/scopes/mod.rs:2627,2628,2630,2638,2640,2720:   error.kind.to_string() …
techy/src/serialize/**, techy/src/node/**, techy/tests/derive_conditions.rs (node and
diagnostic data, not tokens — elided here for length)
```

**No token-kind read is left.** The remaining lines are: `invocation.kind` (the view
field this stage adds — read, not computed), the `Debug` impls of `Invocation` and
`TokenStopCondition`, `UnusableRecoveryToken.kind` and `TokenStopCondition.kind` (the
parsers' own condition enums), and six `SpecialsScanError.kind` assertions. The
`serialize` / `node` hits are `NodeData.kind` and wire-record fields.

Token *span* / *whitespace* reads outside the token module:

```
$ grep -rn "token\.span\|tok\.span\|\.pre_space\b\|\.post_space()" techy/src techy/tests docs \
    | grep -v "^techy/src/token/"
techy/src/latexlike/arguments.rs:727:        assert_eq!(m.post_space(), Some(" "));
techy/src/latexlike/invocation_syntax.rs:530,535,641,698,788
techy/src/latexlike/environments.rs:1247,1288
techy/tests/acceptance.rs:393,397,450,563,606
docs/learn-by-example.md:186
```

Every one is `NodeRef::post_space()` — the latexlike *node* accessor reading the
recorded payload, not a token. **No `Token` field is read outside `techy/src/token/`**,
by any reader impl or otherwise: the five `cfg(test)` delegating readers
(`BrokenReader`, `StuckRecoveryReader`, `FlakyReader`, `TabooReader`,
`CommentEmittingReader`) delegate every question to their inner `StdTokenReader` and
only *construct* tokens (`Token::new(TokenKind::…, span, pre_space)` from their own
locals), so they match neither grep.

### Decisions taken under §1.16

- **`Invocation.kind` is kept as a field** — probe P2 confirmed the shape compiles and
  survives holding the view across a sub-parse (the receiver's lifetime stays out of
  `token_kind`'s return type). No `Invocation.name: String` fallback was needed; `name`
  is now `&'a str` taken from the view.
- **The view's manual impls** are `Clone`/`Copy`/`Debug`/`PartialEq`/`Eq`/`Display`
  (never a derive — probe finding 6), with `Specials` specs compared by `Arc::ptr_eq`
  and `GroupOpen` rules structurally, exactly as the stored `TokenKind` compares.
- **`TokenKindView` lives in `techy/src/token/token.rs`** (§5 leaves the choice open),
  next to the stored kind it mirrors — 3b merges the two.
- **User ruling 2026-08-17: a fifth edge `TokenEdge::ContentStart`** ("where the token's
  own content begins, past its leading marker"), declared between `Start` and `End`, so
  the five offsets satisfy `StartBeforePreSpace ≤ Start ≤ ContentStart ≤ End ≤
  EndPastPostSpace` (`≤`: edges coincide where a kind has no pre-space, no leading
  marker, or no post-space). The comment node's three sub-spans are then three reader
  answers and the parser computes nothing. This replaced the interim the stage brief
  described (deriving the sub-spans from `start_delim.len()` plus a new `TokenReader`
  contract clause); no such clause is on the trait.

### Deviations from §1/§5

1. **Two more reader-less hooks take the view, which §1 does not list.**
   `TokenStopKind::Predicate` (`&dyn Fn(&Token) -> Result<bool, _>` →
   `&dyn Fn(TokenKindView<'_, L>) -> Result<bool, _>`) and `GroupChildState::Compute`
   (`&dyn Fn(&Arc<ParsingState>, &Token) -> …` → `… TokenKindView<'_, L> …`). Both
   receive a token and no reader, and both exist *only* to look at the token's kind —
   the predicate matches kinds, the compute closure keys on the open delimiter's rule.
   Leaving them on `&Token` would have made the stage's completion grep impossible to
   satisfy (their only in-tree callers read `token.kind`) and would leave two hooks that
   3b's opaque token renders useless. The reasoning is ruling O-1's, applied where the
   same situation recurs: what a reader-less party can know about a token is the view.
   Callers: two test predicates and two test compute closures — no production caller.
2. **`comment_node_kind` is a new `pub(crate)` free function** in
   `techy/src/constructs/mod.rs`, beside `node_text_content`. §1 does not name it, but
   the comment staging rule now has two call sites (`nodes_parser.rs`,
   `argument_parsers.rs`) and, with the `ContentStart` edge, one obvious spelling; a
   single home keeps the two in step (the same argument as 2b's `node_text_content`).
3. **`StdTokenReader::token_kind` slices the comment delimiter with
   `content.get(..).unwrap_or("")`, not `content[..]`.** A token this reader never
   issued (contract clause 4 — it cannot detect the violation) may carry a span this
   content does not have; indexing would be a new always-on panic in library code, which
   the panic policy does not allow without a registered exception. The delimiter reads as
   empty instead. `TokenListReader` rejects such a token before it gets that far.
4. **`Invocation.name` is `&'a str`, not `&'s str`.** It comes from the view now, whose
   lifetime is the token's borrow. Since `token: &'a Token<'s, L>` implies `'s: 'a`,
   every existing caller still compiles. 3b drops `'s` entirely.
5. **The `docs/*.md` prose links to `TokenKind`** (`concepts-overview.md`,
   `parsing-model.md`) were left alone: they resolve today and keep resolving in 3b,
   where the view *is* `TokenKind`. Only the intra-doc links in files that no longer
   import the stored kind were repointed at `TokenKindView`
   (`nodes_parser.rs`, `latexlike/driver.rs`, `verbatim_parser.rs`, `group_parser.rs`,
   `embellishments_parser.rs`, `tack_on_parser.rs`).

### Open questions

1. **The two extra view-taking hooks** (deviation 1) are the only judgment call in the
   stage that §1 does not spell out. The conservative alternative — leave both on
   `&Token` — is not viable in 3b, so the change is brought forward here rather than
   deferred; flagged for the reviewer, not decided beyond ruling O-1's principle.
2. **No existing test's expected node span or payload changed.** The whole suite passes
   unmodified, the comment sub-spans included (the `ContentStart` edge reproduces the
   stored token's `start`/`content`/`post_space` exactly). Recorded because the comment
   staging rule changed shape.

### Review round 1 (fixes applied 2026-08-17)

The stage came back READY for 3b to build on, subject to two documentation fixes and
four points of polish; all six are on the branch.

1. **`TokenKindView::Comment`'s rustdoc** still claimed "the token's own bytes in
   order — see the `TokenReader` contract, clause 7". There is no clause 7 (ruling O-3
   replaced that interim rule with the `ContentStart` edge). It now points at the edges
   instead, non-normatively: `Start..ContentStart` for the delimiter,
   `ContentStart..End` for the text.
2. **The reader test comment** repeating "(contract clause 7)" now says "as this reader
   resolves them".
3. **A duplicate comment** left by the port in `argument_parsers.rs`'s marker run (the
   pre-port "Consecutive: no whitespace…" line) is deleted.
4. **The two `resolve_command` call sites** pass the `kind` the loop already computed
   instead of re-querying `cx.tokens.token_kind(…)` — the view is `Copy` and borrows
   nothing from the reader (`nodes_parser.rs`, `argument_parsers.rs`).
5. **The two `move_to` edge tables** (`reader.rs`'s movement test, renamed
   `move_to_lands_at_each_of_the_five_edges`, and `list_reader.rs`'s lockstep matrix)
   list `ContentStart` — offset 3 for the `\vec` trigger, one byte past its escape
   character.
6. **Contract clause 4 states the foreign-token asymmetry**: `token_kind` answers an
   empty slice where a foreign token's offsets fall outside the reader's content (a
   wrong answer beats a new panic in library code), while `source_span_between` hands
   those offsets to `SourceSpan::new`, whose registered always-on assert fires. The
   reasoning previously lived only in a code comment.

Gates re-run at the branch tip, all green: `cargo test` (1050 unit + 30 + 9 + 13 + 23
integration + 84 doctests, 0 failed), `cargo clippy --all-targets -- -D warnings`
(clean), `rm -rf target/doc && cargo docs` (clean).

---

## Stage 3b — the opaque token (§5)

- **Branch**: `bt-3b-opaque` (off `bt-3a-view` at `1e96803`).
- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/bt-3b-opaque`.
- **Status**: reviewed (READY TO MERGE; review round 1 + three user rulings applied).
  Date: 2026-08-17.
- **Merge**: together with `bt-3a-view` (§5) — `TokenKindView` never reaches `main`.
- **Commits** (`git log --oneline bt-3a-view..bt-3b-opaque`, newest first; the
  PROGRESS updates follow the lists):

```
a1a36cb core: resolvers and reader-less hooks take the token and its reader
f19bbe5 token: the test-only accessors are `cfg(test)`, and two review polish items
683525a token: pin the `Lang::Token` contract for the trivial blanket impl
d60d676 docs: the panics list, the guides, and the `Lang` doctest for opaque tokens
ea6ec24 tests: the custom reader over standard tokens, from outside the crate
cd57c08 core: `Lang::Token`, and every layer holds tokens it cannot read
fffa65f token: the opaque `StdToken`, the `Token` contract, and `TokenKind` as the view
```

The first two commits are **one atomic change split for reviewing**: dropping the
token's lifetime forces `Lang::Token`, the 14 dependent types and every call site in
the same step, so only the second of the two builds. Every later commit builds and is
green on the gates.

### What changed, per file

| File | Change |
|---|---|
| `techy/src/token/token.rs` | rewritten: the `Token<L>` marker contract (`Clone + Debug + PartialEq + Send + Sync`); the view renamed `TokenKind<'t, L>` (the stored kind enum deleted, its taxonomy documentation moved onto the view); `StdToken<L>` — private `StdTokenKindData<L>` + `span`/`pre_space`, eight public constructors with the coherence asserts, `pub(crate)` `kind_data`/`span`/`pre_space`/`post_space`/`edge_offset`/`with_pre_space`, manual `Clone`/`Debug`/`PartialEq`/`Eq`, `impl Token<L> for StdToken<L>`; six new tests (all eight constructors' happy paths and edges, the kind-data variants, spec identity/rule structural equality, three `#[should_panic]` — one per assert family — and the `Lang::Token` contract for a trivial lang) |
| `techy/src/token/reader.rs` | the trait in its §1.6 final form over `L::Token`/`TokenResult<L, T>`; the `StdTokenReader` impl header and every scanning-core helper carry `L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>` (probe P8); the scanner builds tokens with the `StdToken` constructors; `token_kind` slices `content` between the token's edges (`Command` name = `ContentStart..End`, `Specials` name = `Start..End`, comment delimiter/text = `Start..ContentStart`/`ContentStart..End`), all through `.get(..).unwrap_or("")`; the "writing a reader over standard tokens" rustdoc gains a compiling wrapper example; ~50 `Token::new` test calls become constructors and every `.kind` assertion goes through a `kind_of(&reader, &token)` helper (the reader's view), with `.span`/`.pre_space` reading through the `pub(crate)` accessors |
| `techy/src/token/list_reader.rs` | holds `Vec<StdToken<L>>`, impl bound gains `Token = StdToken<L>`; `peek` clips pre-space through `with_pre_space`; `check_issued` compares `span()` + `kind_data()`; `token_kind` interprets identically over the same content; tests read through the view |
| `techy/src/token/error.rs` | `TokenError<L>`, `TokenRecovery<L> { token: L::Token, resume }`, `TokenResult<L, T>` — the `'s` is gone |
| `techy/src/token/mod.rs`, `techy/src/core/mod.rs` | facades export `StdToken` and the `Token` **trait** (same public name, different item); `TokenKindView` removed; the module prose says tokens are opaque and pre-space/post-space are reader answers |
| `techy/src/state/lang.rs` | `Lang::Token: Token<Self>` with the opacity rustdoc (§1.2/§1.15); the blanket `impl<T: TrivialLang> Lang for T` names `StdToken<Self>` |
| 17 files with `impl Lang for …` | `type Token = StdToken<Self>;` next to `type StreamPosition` — all 57 sites (53 in `techy/src`, 3 in `techy/tests/lang_features.rs`, 1 in the `docs/custom-lang.md` doctest) |
| `techy/src/engine/driver.rs` | `probe_token -> Option<L::Token>`; `make_group_parser<'p>(&'p self, open: &L::Token, ..)` (the `'s: 'p` clause gone); `make_invocation_parser<'a>`; `StdParseDriver`'s impl bound gains `Token = StdToken<L>` |
| `techy/src/latexlike/lang.rs` | `LatexlikeLang`'s supertrait bound gains `Token = StdToken<Self>` |
| `techy/src/constructs/mod.rs` | `Invocation<'a, L>` (`token: &'a L::Token`), `FromInvocation::from_invocation(&Invocation<'_, L>, &dyn TokenReader<'_, L>)`, `ParseContext::probe_token`/`parse_group`, `comment_node_kind`, `invocation_frame` |
| `techy/src/spec/callable.rs`, `techy/src/latexlike/spec.rs`, `techy/src/constructs/child_state.rs` | `make_invocation_parser<'a>(.., Invocation<'a, L>)`; the compute-closure type takes `&Invocation<'_, L>` |
| the 14 `'s`-dropping types | `ArgumentNoise`, `MintedGroupMatch`, `GroupParser`, `StdInvocationParser`, `ErrorInvocationParser` (`scopes/mod.rs`), `EnvironmentInvocationParser`/`OrphanEndParser` (`latexlike/environments.rs`), `InputInvocationParser`, `AfterEffectInvocationParser`, and the test parsers `DefParser` ×2, `TakeParser`, the test `EnvironmentInvocationParser`, `RawBlockParser`, `RestOfLineParser`/`BadEndParser`. `ParseContext<'a, 's, L>` keeps its `'s` |
| the four `cfg(test)` delegating readers | `BrokenReader`, `StuckRecoveryReader`, `FlakyReader`, `TabooReader` mint with the constructors and delegate interpretation to their inner `StdTokenReader` |
| `techy/tests/lang_features.rs` | `CommentEmittingReader` is the §1.8 wrapper as an outside party writes it: the comment token minted with `StdToken::comment` from spans it computes by scanning `self.inner.content()`, every interpretive method delegated, no field access (there is none); the groups-only reader test drives the standard reader through a `dyn TokenReader` view |
| `docs/panics.md` | the `Token::new` entry replaced by the seven span-taking `StdToken` constructors and their asserts, with `end_of_stream` named as the eighth that takes no span and never panics; the count sentence corrected ("Five value functions and the seven span-taking `StdToken` constructors") |
| `docs/construct-parsers.md` | the takeover doctest's `make_invocation_parser<'a>` and `UntilParser<'a>` |
| `docs/custom-lang.md` | the `impl Lang` doctest gains `type Token = StdToken<Self>` (and the import) |
| `docs/concepts-overview.md` | the one sentence that became false ("zero-copy views of the source") — tokens are opaque values; what one *is* is the reader's answer |

### Gate results (verbatim)

```
$ cargo build
   Compiling techy v0.1.0 (…/bt-3b-opaque/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.43s

$ cargo test
running 1055 tests   test result: ok. 1055 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 30 tests     test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 9 tests      test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 13 tests     test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 23 tests     test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 0 tests × 3  test result: ok. (serialize_golden, serialize_perf, serialize_stream)
running 1 test       test result: ok. 1 passed (techy-derive unit)
running 90 tests     test result: ok. 85 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out  (doctests)
running 2 tests      test result: ok. 0 passed; 0 failed; 2 ignored  (techy-derive doctests)

$ cargo clippy --all-targets -- -D warnings
    Checking techy v0.1.0 (…/bt-3b-opaque/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.62s
(no warnings)

$ rm -rf target/doc && cargo docs
 Documenting techy v0.1.0 (…/bt-3b-opaque/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.45s
   Generated …/target/doc/techy/index.html and 1 other file

$ cargo test -p techy --lib constructs::nodes_parser
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 975 filtered out

$ cargo test -p techy --lib token::list_reader
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 1040 filtered out
```

### Semver report (`scripts/check_semver.sh`)

Breaking, as expected — the report is cumulative against the `api-baseline` branch, so
it still carries Stages 1–2b's removals alongside 3b's:

```
Summary semver requires new major version: 20 major and 0 minor checks failed
```

3b's own entries: `copy_impl_added` (`techy::core::TokenKind` — the view is `Copy`);
`trait_associated_type_added` (`Lang::Token`, plus Stage 1's `Lang::StreamPosition`);
`type_mismatched_generic_lifetimes` for `TokenError`, `TokenRecovery`, `Invocation`,
`StdInvocationParser`, `ArgumentNoise` (and Stage 3a's `CallableQuery`, Stage 1's
`SpecialsMatch`); `trait_method_parameter_count_changed` for
`FromInvocation::from_invocation`; the `struct_missing`/`derive_trait_impl_removed`
families for the old `Token<'s, L>` struct and the stored `TokenKind`. The rest predates
this stage (`StdTokenReader::pos`/`move_to_pos`, `ParseContext::new`,
`ParseDriver::probe_token`/`make_paragraph_break_node`/`make_token_reader`,
`CallableQuery::with_token`, `StopCause<L>`, the `SerializableObject` supertraits).

### The sweep (§5)

```
$ grep -rn "move_to_pos\|resume_pos\|Token::new\|TokenKindView\|end_pos\b\|Token<'s\|TokenResult<'\|TokenError<'\|Invocation<'a, '\|SpecialsMatch<'" techy docs
```

51 hits, all classified as unrelated:

- **`move_to_position`** (41 hits) — the new API; the pattern `move_to_pos` is its
  prefix. No `move_to_pos(` call or definition exists.
- **`end_pos`** (8 hits: `source/source.rs` ×7, prose and its two tests included,
  `source/line_index.rs` ×1) — `SourceSpan::end_pos`, the S0 accessor, untouched by
  this plan.
- **`UnusableRecoveryToken::new`** (2 hits, `nodes_parser.rs`) — a diagnostic
  condition's constructor, matched only because `Token::new` is a substring of it.

No hit for `resume_pos`, `TokenKindView`, `Token<'s`, `TokenResult<'`, `TokenError<'`,
`Invocation<'a, '` or `SpecialsMatch<'`.

### Timing check (§5, repeated once)

`cargo run --release --example bt_timing`, 5 runs each, same 5 242 901-byte document,
same result on both trees (257 816 nodes, 0 diagnostics):

| Tree | Runs (parse_ms) | Median |
|---|---|---|
| `bt-3b-opaque` | 212.8, 212.9, 210.2, 221.6, 216.7 | **212.9 ms** |
| baseline `7825789` (throwaway worktree, example copied in untracked) | 201.4, 195.2, 201.4, 216.8, 201.3 | **201.4 ms** |

**+5.7 %** — within the ≤ 10 % budget and inside 2b's measured +3–7 % band, i.e. 3b adds
nothing measurable on top of the position/edge work. **The reviewer measured
independently** on the same example: a clean interleaved pass gave 189.7 ms on the
branch against 173.7 ms on the baseline (**+9.2 %**), and a noisier pass **+11.6 %** —
so the true figure sits near the top of the budget rather than comfortably inside it,
and the spread between passes exceeds the effect being measured. **Stage 4 re-measures**
(§6's gates) and rules on whether anything is owed. No new per-token `Arc` clone
appeared: a `GroupOpen` token still holds the one `Arc<GroupRule>` the prefix-table
match hands it and a `Specials` token the one `Arc<dyn CallableSpec>` the scan hook
returns, exactly as before; the token now stores *fewer* words (no `&str` pairs). The
throwaway worktree was removed with `git worktree remove --force`.

### Decisions taken under §1.16

- **The doc example that hand-built tokens.** The only such example outside the crate
  was `techy/tests/lang_features.rs`'s `CommentEmittingReader` (a test, not a doc
  example): it keeps hand-building its token, now with `StdToken::comment` and spans it
  computes by scanning the content its inner reader serves. In `docs/*.md` no example
  built a token; the guides' examples go through a reader, so nothing had to be
  rewritten there. New in the trait's rustdoc: a **compiling** wrapper-reader example
  (`MyReader` over `StdTokenReader`, `TrivialLang`), with the six mechanical
  delegations hidden behind `#` so the visible sketch stays short — the shape §1.8
  prescribes and probe P4 recommends showing (the `&dyn TokenReader` helpers).
- **`source_span_between` with equal edges** — unchanged from Stage 1: the empty span
  at that edge.
- **`StdTokenReader::source_span_between` on a foreign token** — unchanged: the
  registered `SourceSpan::new` assert, documented in contract clause 4. `token_kind`
  keeps 3a's asymmetric answer (an empty slice) and now applies it to *every* written
  spelling it slices, since all of them are slices now.
- **`TokenListReader` position validation**, **`StopCause<L>`**,
  **`ArgumentNoise.next`**, **`Invocation.kind` as a field**, **`is_at_end()`**,
  **the chars-run contiguity message** — all unchanged from the earlier stages.

### Deviations from §1/§5

1. **`StdTokenKindData<L>` is a named `pub(crate)` enum**, not an anonymous shape:
   §1.3 says "kind data (…)" and lets the implementer choose the spelling
   ("`kind_data()` (or however the readers need to read the kind data)"). The two
   in-crate readers match on it to build their views, and `TokenListReader` compares it
   for its issued-token check. Not exported anywhere.
2. **One `pub(crate)` accessor beyond §1.3's list: `with_pre_space`.** The test list
   reader clips a served token's pre-space to the current position (its documented
   fidelity rule); with private fields it can no longer assign the field. The method
   returns a clone with the narrowed pre-space and asserts the same coherence the
   constructors do.
3. **`span()`, `pre_space()` and `with_pre_space()` carry `#[allow(dead_code)]`.** The
   crate's two readers answer positions and spans through `edge_offset`, so the three
   direct readings are used only by the token module's own tests and by the
   `cfg(test)` list reader — dead in a non-test build, which `-D warnings` rejects.
   Kept (rather than `cfg(test)`-gated) because §1.3 lists them as the in-crate reader
   accessor set.
4. **`make_group_parser` lost its `'s` parameter**, not only the `'s: 'p` clause: with
   `open: &L::Token` the lifetime had no other use, and an unused lifetime on a trait
   method does not compile.
5. **Two test helpers named `kind_of`** (in `token/reader.rs` and
   `token/list_reader.rs`) wrap `reader.token_kind(&token)` so the readers' own tests
   read a token exactly as a construct parser does. They exist because the P8 inference
   caveat bites in those modules: a call on the *concrete* reader whose only argument
   mentions `L` through `&L::Token` cannot pin `L`. The same caveat is why a handful of
   test call sites bind `let reader: &dyn TokenReader<'_, TheLang> = &r;` first — the
   shape §1.8's documented pattern uses anyway.
6. **`docs/concepts-overview.md` prose was touched** (one sentence), which Stage 4
   otherwise owns: "zero-copy views of the source" became false in this stage, and §5
   requires a sentence that would now be false to be corrected minimally.

### Open questions

1. **None blocking.** No design question outside §1.16 came up; nothing was decided
   beyond the defaults recorded above.
2. **No test's expected node span, payload or diagnostic changed.** The whole suite
   passes unmodified — the lockstep suites included — which is the evidence that
   opacity changed no behavior.
3. **For Stage 4's CLAUDE.md note** (ruling O-2 — no stage edits it): the
   `techy::core` topology line still reads "tokens (Token, TokenKind, TokenRules,
   TokenReader, StdTokenReader)". `Token` is now a trait and `StdToken` is a new
   public item next to it, so that line may deserve `StdToken` — the user's call.

### Review round 1 + rulings O-1b, O-5 and the two reader-less hooks (2026-08-17)

The stage came back **READY TO MERGE after one required fix**; the user issued three
rulings in the same round, all applied here (two commits: `f19bbe5` the fix and the
polish, `a1a36cb` the rulings — they touch the same call sites, so splitting them
further would only produce non-building intermediates).

**Required fix (reviewer).** `StdToken::span`, `pre_space` and `with_pre_space` are
`#[cfg(test)]`, not `#[allow(dead_code)]`: every caller is test-only (the token
module's own tests, the `cfg(test)` list reader), which is now what their comments
say. This supersedes deviation 3 above.

**Suggestions (both applied).** `TokenReader::next` sits directly after `peek` (§1.6's
order); `TokenListReader::tokens()`, dead since the July 2026 demotion to test-only,
is deleted.

**Ruling O-1b (user, superseding O-1's view-only form; §1.10 and §1.17 on `main` at
`511110f`).** "Passing a view is poor design": the driver-level resolvers receive the
**token and a read-only reference to its reader** instead.

- `ParseDriver::resolve_command(&self, state, token: &L::Token, tokens: &dyn TokenReader<'_, L>)`,
  and likewise `CommandResolver::resolve_command` (the `()` and `ScopesCommandResolver`
  impls), `StdParseDriver`'s forwarding, the latexlike driver's override, and
  `resolve_command_in_scopes(state, token, tokens, callable_type)` — which asks
  `tokens.token_kind(token)` and matches `Command { name, escape_char }` exactly as
  before (anything else → `Unresolved`).
- **`CallableQuery<'a, L>` drops the token entirely**: `callable_type`, `name`,
  `syntax` — `token_kind`/`with_token_kind` are gone, and its rustdoc states that
  scopes and packages look up by name and callable syntax while a language that must
  dispatch on token details does so in `ParseDriver::resolve_command`. Stage 3a's
  `with_token_kind` test is deleted; the three test resolvers that attached a
  token/view drop the attachment.
- Callers: `nodes_parser.rs` and `argument_parsers.rs` pass `(&token, &*cx.tokens)` —
  probe P6's shape, and the borrow checker accepts it as written (the shared reborrow
  of `cx.tokens` coexists with the loop's `Copy` view, which borrows the token, not the
  reader). The driver tests in `engine/mod.rs` read a real `\foo` token from a small
  source through a `StdTokenReader` (a local `command_token` helper scans under rules
  with a `\`-command rule) and hand that reader to the hook; `tests/lang_features.rs`'s
  `FixedTableResolver` reads its name with `tokens.token_kind(token)`.
- The 3a form never reaches `main`: 3a and 3b merge together.

**Ruling O-5 (user): `Invocation` drops `kind`.** The field cached a reader answer;
every consumer holds a reader already, so the latexlike `from_invocation`,
`argument_parsers.rs`'s requires-content spelling and `latexlike/environments.rs`'s two
trigger checks call `token_kind` on the spot (`tokens.token_kind(invocation.token)` /
`cx.tokens.token_kind(self.invocation.token)`). The bundle is now "the resolution
result plus the token", which its rustdoc says. No reader parameter was added to
`make_invocation_parser`, and no reader reference was put inside `Invocation` (it is
stored across the invocation parser's own parse, during which the same reader is
mutably borrowed through `cx.tokens` — it cannot compile). One test consequence worth
recording: `from_invocation`'s specials case used to be provable with a `Specials` view
over a `Command` token; it now reads a **real** `~` token (the minilatex package
supplies the trigger), which is a stricter test.

**Ruling (user): the two reader-less hooks take the token and its reader**, on O-1b's
principle. `TokenStopKind::Predicate` is
`&dyn Fn(&L::Token, &dyn TokenReader<'_, L>) -> Result<bool, ParseError>` and
`GroupChildState::Compute` is
`&dyn Fn(&Arc<ParsingState<L>>, &L::Token, &dyn TokenReader<'_, L>) -> Result<Arc<ParsingState<L>>, ParseError>`;
the consultation sites pass `(&token, &*cx.tokens)`. `NodesParser::token_stop` takes
the token and the reader too and queries the view once for its built-in arms — the
one extra `token_kind` call per iteration happens only where a token stop condition is
configured. The four test closures ask the reader themselves. No higher-ranked
lifetime annotation was needed (`dyn Fn(&…)` supplies it).

**§1 deviations from this round:** none. `Invocation.kind` (a §1.16 default and probe
P2's finding) and the view-carrying resolve chain (§1.10 as of the previous plan
revision) are both superseded by the rulings, which the plan on `main` already
records.

### Gates after the review round (verbatim)

```
$ cargo build
   Compiling techy v0.1.0 (…/bt-3b-opaque/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.42s

$ cargo test
running 1054 tests   test result: ok. 1054 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 30 tests     test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 9 tests      test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 13 tests     test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 23 tests     test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 0 tests × 3  test result: ok. (serialize_golden, serialize_perf, serialize_stream)
running 1 test       test result: ok. 1 passed (techy-derive unit)
running 90 tests     test result: ok. 85 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out  (doctests)
running 2 tests      test result: ok. 0 passed; 0 failed; 2 ignored  (techy-derive doctests)

$ cargo clippy --all-targets -- -D warnings
    Checking techy v0.1.0 (…/bt-3b-opaque/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.24s
(no warnings)

$ rm -rf target/doc && cargo docs
 Documenting techy v0.1.0 (…/bt-3b-opaque/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.55s
   Generated …/target/doc/techy/index.html and 1 other file

$ scripts/check_semver.sh
     Summary semver requires new major version: 20 major and 0 minor checks failed

$ cargo test -p techy --lib constructs::nodes_parser
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 975 filtered out

$ cargo test -p techy --lib token::list_reader
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 1040 filtered out
```

The unit count moves 1055 → 1054: the `with_token_kind` test goes with the field it
tested. The semver categories are unchanged from the first run (the same 20; O-1b adds
`method_parameter_count_changed`/`struct_pub_field_missing` entries to families already
failing, and removes `CallableQuery::with_token_kind` — a method Stage 3a introduced,
which never reached `main`).

---

## Stage 4 — final sweep (§6)

- **Branch**: `bt-4-final` (off `main` at `8b25806`, which already contains Stages 1,
  2a, 2b, 3a and 3b).
- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/bt-4-final`.
- **Status**: reviewed and merged (`main` a729f7f). Date: 2026-08-18.
- **Commits** (`git log --oneline main..bt-4-final`, newest first). Two commits are
  not in the list below because they were written after it: this PROGRESS/PLAN update
  itself ("bettertokens: Stage 4 — the final sweep, its numbers, and the plan's
  status", which also deletes the timing example), and one wording fix on top of it
  ("docs: 'source' means the Source type, not the origin of a position").

```
8338d6d TODO_Big: the better-tokens follow-ups left for later
85b8944 docs: the banned words leave the text this project wrote
5dd37de docs: the guides describe opaque tokens and reader-issued positions
ac220d2 docs: the construct-parser guide on tokens, edges and positions
76b6769 core: the chars run asks the reader once per token
```

### What was merged before this stage

Every code stage is on `main`: `7825789` (Stage 0, the probe report and this log),
`d5f37e0` (Stage 1, positions/spans/the reader hook), `0af4276` (Stage 2a, the core
construct-parser layer), `a8f36a1` (Stage 2b, the rest of the port and the old
positional API deleted), `8b25806` (Stages 3a + 3b together, the view and the opaque
token). `main` is at `8b25806` while this stage runs.

### What changed, per file

| File | Change |
|---|---|
| `techy/src/constructs/nodes_parser.rs` | the one optimization (below): `extend_run` asks for two edge positions instead of four, and `token_stop` receives the view the content loop already computed instead of re-querying it; plus wording |
| `docs/construct-parsers.md` | the reader section rewritten as the three questions a parser asks (what a token is, where it is, where the stream stands), with the edges, the two position sources, `cx.source_span_within` and `cx.here()`; the worked example's closing notes point at the spans it stages |
| `docs/concepts-overview.md` | the token concept: which token type a language uses is its own declaration, what a span answer covers (whole token, or between two of the five edges), what a stream position is; the specials sentence reads off the reader's answer |
| `docs/parsing-model.md` | the reader comes from `make_token_reader`; tokens are opaque; command resolution receives the token and its reader |
| `docs/custom-lang.md` | the custom-reader paragraph: the `TokenReader` trait, installed through `make_token_reader`, `Lang::Token`/`Lang::StreamPosition` as the language's choice, and the delegating shape for a reader over standard tokens; the driver section points back at it |
| `docs/ai-guide-custom-lang.md` | the `Lang` table gains the `Token` and `StreamPosition` rows; the context table's reader row lists the three answers and the navigation, with a new row for the two ways to obtain a node's span |
| `techy/src/{token,constructs,engine,latexlike}/*.rs`, `docs/ai-guide-custom-lang.md` | the banned-word sweep over text this project wrote (below) |
| `TODO_Big.md` | the "Better tokens — deferred follow-ups" list (§10 plus what the stage log left open) |
| `techy/examples/bt_timing.rs` | deleted (its purpose ends with this stage's measurement) |

### Gate results (verbatim)

```
$ cargo build
   Compiling techy v0.1.0 (…/bt-4-final/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.32s

$ cargo test
     Running unittests src/lib.rs (target/debug/deps/techy-94158093885f6495)
running 1054 tests
test result: ok. 1054 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.62s
     Running tests/acceptance.rs
running 30 tests
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
     Running tests/derive_conditions.rs
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/lang_features.rs
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/recompose_oracle.rs
running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/serialize_golden.rs / serialize_perf.rs / serialize_stream.rs
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out (each)
     Running unittests src/lib.rs (techy_derive)
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests techy
running 90 tests
test result: ok. 85 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 21.69s
   Doc-tests techy_derive
running 2 tests
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo clippy --all-targets -- -D warnings
    Checking techy v0.1.0 (…/bt-4-final/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.70s
(clean — no warnings, exit 0)

$ rm -rf target/doc && cargo docs
 Documenting techy-derive v0.1.0 (…/bt-4-final/techy-derive)
 Documenting techy v0.1.0 (…/bt-4-final/techy)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.49s
   Generated …/target/doc/techy/index.html and 1 other file
(no broken intra-doc links)

$ cargo test -p techy --lib constructs::nodes_parser
running 79 tests
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 975 filtered out; finished in 0.06s

$ cargo test -p techy --lib token::list_reader
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 1040 filtered out; finished in 0.01s
```

### Semver report (`scripts/check_semver.sh`)

Breaking, as expected for the whole project (soft freeze); nothing was "fixed".

```
    Checking techy v0.1.0 -> v0.1.0 (no change; assume minor)
     Checked [   0.040s] 196 checks: 176 pass, 20 fail, 0 warn, 58 skip
     Summary semver requires new major version: 20 major and 0 minor checks failed
    Finished [   9.671s] techy
```

The 20 failing lints in full, with every item each one reports (three of them —
`auto_trait_impl_removed`, `trait_added_supertrait`, and the `provenance` entries of
the two `constructible_struct_*` lints — predate this project):

- `auto_trait_impl_removed`: `BeginSpec` no longer `UnwindSafe`; `BeginSpec` no longer
  `RefUnwindSafe` *(pre-existing)*
- `constructible_struct_adds_field`: `NameGroup.name`; `StdCallableSpec.provenance`
  *(pre-existing)*; `TokenRecovery.resume`
- `constructible_struct_adds_private_field`: `SpecialsSpec.provenance` *(pre-existing)*
- `copy_impl_added`: `techy::core::TokenKind` (the view is `Copy`)
- `derive_trait_impl_removed`: `EnvironmentInvocation` no longer derives `Copy`;
  `StopCause` no longer derives `Copy`
- `enum_struct_variant_field_added`: `StopCause::TokenCondition::after`;
  `StopCause::UnexpectedGroupClose::after`; `TokenKind::Comment::start_delim`
- `enum_struct_variant_field_missing`: `TokenKind::Command::post_space`;
  `TokenKind::Comment::start`; `TokenKind::Comment::post_space`
- `function_parameter_count_changed`: `techy::latexlike::make_paragraph_break_node`
  4 → 3; `techy::core::specs::resolve_command_in_scopes` 3 → 4
- `inherent_method_missing`: `StdTokenReader::pos`; `StdTokenReader::move_to_pos`;
  `CallableQuery::with_token`
- `method_parameter_count_changed`: `ParseContext::new` 5 → 4
- `struct_missing`: `techy::core::Token` (the struct; the name is now the trait)
- `struct_pub_field_missing`: `NameGroup::name_span`; `ParseContext::source`;
  `TokenRecovery::resume_pos`; `CallableQuery::token`; `SpecialsMatch::name`
- `trait_added_supertrait`: `SpecsProvider` and `CallableSpec` gained
  `SerializableObject` *(pre-existing)*
- `trait_associated_type_added`: `Lang::Token`; `Lang::StreamPosition`
- `trait_method_added`: `ParseDriver::make_token_reader`
- `trait_method_missing`: `TokenReader::move_past`; `TokenReader::move_to_pos`;
  `TokenReader::pos`
- `trait_method_parameter_count_changed`: `ParseDriver::probe_token` 4 → 3;
  `ParseDriver::resolve_command` 2 → 3; `ParseDriver::make_paragraph_break_node` 3 → 2;
  `CommandResolver::resolve_command` 2 → 3; `EnvironmentSyntax::from_parsed` 2 → 3;
  `FromInvocation::from_invocation` 1 → 2
- `trait_requires_more_generic_type_params`: `StopCause` 0 → 1
- `type_mismatched_generic_lifetimes`: `TokenRecovery` 1 → 0; `StdInvocationParser`
  2 → 1; `TokenError` 1 → 0; `CallableQuery` 2 → 1; `Invocation` 2 → 1;
  `ArgumentNoise` 1 → 0; `SpecialsMatch` 1 → 0
- `type_requires_more_generic_type_params`: `StopCause` 0 → 1

### The sweep (§6)

```
$ grep -rn "move_to_pos\b\|resume_pos\|Token::new\b\|TokenKindView\|Token<'s\|TokenResult<'\|TokenError<'\|Invocation<'a, '\|SpecialsMatch<'\|with_token(\|with_token_kind\|move_past\|move_to_edge\|cx\.source\b\|end_pos\b" techy docs
techy/src/source/source.rs:274,348,652,653,748,750,751
techy/src/source/line_index.rs:183
techy/src/serialize/drivers/source.rs:382
techy/src/constructs/nodes_parser.rs:1177,1213
```

Eleven hits, all unrelated to this project's vocabulary:

- **`SourceSpan::end_pos`** (8 hits, `source/source.rs` ×7 including two tests,
  `source/line_index.rs` ×1) — the S0 accessor, sibling of `start_pos`, which no stage
  renamed.
- **`UnusableRecoveryToken::new`** (2 hits, `nodes_parser.rs`) — a diagnostic
  condition's constructor, matched because `Token::new` is a substring of it.
- **`DeserializeContext::source(SourceIndex)`** (1 hit,
  `serialize/drivers/source.rs:382`) — the serialization side's sources-table lookup,
  matched by `cx\.source\b`; a different `cx`, unrelated to the removed
  `ParseContext::source`, and older than this project.

No hit for `move_to_pos`, `resume_pos`, `TokenKindView`, `Token<'s`, `TokenResult<'`,
`TokenError<'`, `Invocation<'a, '`, `SpecialsMatch<'`, `with_token(`,
`with_token_kind`, `move_past` or `move_to_edge`.

`docs/panics.md` was re-checked against the tree and is still exhaustive: the eight
`StdToken` constructors (seven that take spans and assert their coherence through the
two shared helpers `assert_pre_space`/`assert_post_space`, plus `end_of_stream`, which
takes no span and never panics) are the only panicking public items this project added;
`StdStreamPosition::at` is `pub(crate)` and the `TokenListReader` guards are
`cfg(test)`.

### Timing re-measure (release profile)

`cargo run --release --example bt_timing` — the **release** profile throughout, the
same deterministic 5 242 901-byte document, both trees reporting the same 257 816 root
children and 0 diagnostics. Baseline: the pre-project commit `7825789` in a throwaway
worktree (`bt-timing-4`, detached, the example copied in untracked, removed with
`git worktree remove --force` afterwards). Runs interleaved (baseline, branch,
baseline, …) on an otherwise idle machine, no cargo work in parallel, nothing
discarded.

**Before the optimization** (branch tip `8b25806`'s code):

| Series | Baseline (ms) | Branch (ms) | Medians | Slowdown |
|---|---|---|---|---|
| 1 (9 each) | 210.4, 425.7, 183.2, 173.1, 171.2, 178.4, 169.9, 176.2, 173.8 | 218.3, 207.7, 198.1, 187.4, 197.4, 187.6, 190.0, 197.1, 196.4 | 176.2 / 197.1 | **+11.9 %** |
| 2 (9 each) | 181.3, 171.8, 171.5, 168.8, 168.6, 171.6, 171.8, 174.8, 181.2 | 186.0, 195.4, 187.5, 185.1, 190.7, 186.7, 190.7, 197.4, 187.2 | 171.8 / 187.5 | **+9.1 %** |
| pooled (18 each) | | | 173.4 / 190.7 | **+9.9 %** |

Above the 8 % line, so the plan's cheap optimizations were considered. **What was
applied** (one commit, `76b6769`, no public signature touched) is §6's item (b),
"redundant reader queries per token in the chars run", in two places:

- `NodesParser::extend_run` asked the reader for four edge positions per `Char` token
  (`StartBeforePreSpace`, `Start` for the pre-space extension, then `Start` again and
  `EndPastPostSpace` for the character). The run is contiguous by construction, so one
  extension over `StartBeforePreSpace..EndPastPostSpace` says the same thing in two
  questions, and keeps the check that can actually fail (does the run end where this
  token begins). `take_pre_space` is unchanged and still serves the non-`Char` arms.
- `NodesParser::token_stop` re-queried `token_kind` although the content loop computes
  the view one statement earlier; it now takes that view as a parameter. The
  `Predicate` hook still receives `(&L::Token, &dyn TokenReader)`, as its contract says.

Items (a) — the per-node `SourceSpan` clone on the success path of `stage` — and (c)
were **not** applied: the measurement after (b) is below the 8 % line, and (a) costs
one `Arc` refcount pair per staged node (≈ 258 k nodes here) against ≈ 5 M reader
queries per parse, so it is the smaller of the two by a wide margin. Left as they are;
the clone is one line and can be revisited if a future measurement asks for it.

**After the optimization** (branch tip):

| Series | Baseline (ms) | Branch (ms) | Medians | Slowdown |
|---|---|---|---|---|
| A (7 each) | 182.2, 174.2, 172.5, 175.3, 180.7, 171.4, 181.1 | 251.0, 183.1, 187.6, 183.5, 197.9, 183.8, 182.8 | 175.3 / 183.8 | **+4.8 %** |
| B (9 each) | 180.3, 195.3, 187.1, 178.0, 197.3, 177.5, 186.8, 177.1, 175.8 | 189.4, 182.8, 199.2, 218.8, 186.7, 189.0, 189.7, 182.7, 197.8 | 180.3 / 189.4 | **+5.0 %** |
| C (10 each) | 171.1, 171.0, 169.3, 181.0, 171.7, 169.5, 170.6, 168.8, 171.5, 180.3 | 210.9, 184.5, 179.0, 187.0, 177.2, 188.2, 180.3, 176.4, 185.8, 197.1 | 171.1 / 185.2 | **+8.2 %** |
| D (10 each) | 182.4, 182.1, 167.0, 169.9, 237.3, 174.1, 172.3, 169.7, 170.1, 170.3 | 183.1, 177.6, 177.2, 190.3, 179.2, 178.8, 185.6, 179.1, 178.8, 176.0 | 171.3 / 178.9 | **+4.5 %** |
| **pooled (36 each)** | | | **174.8 / 184.2** | **+5.4 %** |

**Verdict: +5.4 % at the pooled median, release profile — within the ≤ 10 %
acceptance.** Series A and C each begin with an inflated branch run (251.0, 210.9)
because the branch binary had just been rebuilt while the baseline binary was warm;
nothing was discarded, and the medians absorb it. The machine's own spread is worth
recording for whoever repeats this: baseline medians ranged 171–180 ms across series,
so a single series is worth about ±3 % of noise, and only the pooled figure should be
quoted.

### Decisions taken under §1.16

None reached: this stage adds no API and takes no design decision. The optimization is
a pure internal restructuring; the documentation changes are wording.

### Deviations from §1/§6

1. **§6's item (b) was applied with an addition §6 does not name**: besides the chars
   run, `token_stop` re-queried the token's kind. Same category (a redundant reader
   query per token), same commit, no signature change outside a private method.
2. **The gap-free failure detail's wording changed** with the merged run extension:
   one message ("the char token with its pre-space starts at …") where there were two
   ("the token's pre-space …" / "the char token …"). No test asserts either wording,
   and the condition remains the same `ImplementationError` with both positions.
3. **Two documentation fixes beyond wording** rode along with the banned-word sweep:
   `group_parser.rs`'s module documentation still said the parser is constructed with
   the opening delimiter's *span* (it takes the token), and `docs/panics.md` was
   verified rather than changed (it was already correct).

### Open questions

1. **None blocking.** No design question came up; no test's expectation changed.
2. **For the user, not for this stage** — the timing figure is a *median of medians*
   on a machine whose own spread (±3 %) is of the same order as the effect. If the
   ≤ 10 % budget is meant to hold on quieter hardware too, the honest statement is
   "about +5 %, with a per-series range of +4.5 % to +8.2 %".

### CLAUDE.md refresh candidates (ruling O-2: no stage edits it)

- **The `techy::core` topology line** (line 26) reads "tokens (Token, TokenKind,
  TokenRules, TokenReader, StdTokenReader)". After the port, `Token` is a **trait**
  (the marker contract on a language's token type), the standard token type is
  **`StdToken`**, and `TokenKind` is the reader's **view**, not a stored enum. Two
  further public names live in the same group: **`TokenEdge`** (the five boundaries)
  and **`StdStreamPosition`** (the standard reader's stream position), plus
  `SpecialsScanError` next to the specials hook. A refreshed line could read: "tokens
  (the Token trait + StdToken, the TokenKind view, TokenEdge, StdStreamPosition,
  TokenRules, TokenReader, StdTokenReader)".
- Nothing else in CLAUDE.md is stale: `token → constructs → node (AST)` (line 12) still
  describes the flow; "Use `Token` not `LatexToken`" (line 42) still names a real
  public item (now the trait); rule 4 already points at `docs/panics.md` rather than
  naming `Token::new`, and `docs/panics.md` is exhaustive.

### Deferred follow-ups (PLAN §10, now also in `TODO_Big.md`)

1. Gap-free chars-run contract relaxation for a reader serving one parse from several
   sources (flush on source change, or a declared may-skip-bytes capability) — needed
   only with an expanding reader.
2. `LatexlikeDriver::with_token_reader(...)` — needed only once a custom reader for the
   latexlike family exists.
3. A public `StdStreamPosition` constructor — graduate on demonstrated need.
4. The expanding reader itself lives in `techy-xp`.
5. From the stage log, not from §10: one round of naming polish over the port's new
   fields (`NameGroup::name` is a span while `EnvironmentInvocation` splits
   `name`/`name_span`; `RawContentEnd::{content_end, end}`), and the visibility of
   `StdTokenReader::source()` (public inherent accessor today — Stage 1 deviation 4,
   its open question never closed).
6. The banned words survive in older rustdoc around the port (the standing
   documentation walk-through in `TODO_Big.md` owns that sweep).

### State of the tree, for a fresh session

The token layer is in §1's end state and every code stage is merged to `main` except
this one, which is complete on `bt-4-final`. Tokens are opaque values a language
declares (`Lang::Token`, `StdToken` for everything shipped); the reader that produced a
token is the only party that interprets it — what it is (`token_kind` → the `TokenKind`
view), where it is (`source_span_of`/`source_span_between` over the five `TokenEdge`s),
and where the stream stands (`position_here`/`position_at` → `Lang::StreamPosition`,
opaque and reader-issued). `ParseContext` has no source handle: spans come from the
reader, through `cx.here()` and `cx.source_span_within()`. Navigation is `move_to(&tok,
edge)` and `move_to_position(&pos)`; the old `pos()`/`move_to_pos`/`move_past` API is
gone. A custom reader is installed through `ParseDriver::make_token_reader`, the one
driver method without a default (ruling O-4). The node tree is untouched by all of
this: node spans are still single-source `SourceSpan`s and node data sub-spans still
node-relative `Span`s. Gates at the branch tip: 1054 unit + 75 integration + 85 doctests
green, clippy clean, documentation builds with no broken links, semver breaking as
expected (20 lints, listed above), parse throughput about +5 % against the pre-project
baseline in a release build. **What remains is Stage 5 (§7)**: the ARCHITECTURE and
DESIGN_RATIONALE entries, on a branch off `main` taken *after* this stage merges, with
the user's explicit approval of the drafted text.

---

## Stage 5 — architecture and rationale documentation (§7)

- **Branch**: `bt-5-docs` (off `main` at `a729f7f`, which contains every code stage).
- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/bt-5-docs`.
- **Status**: review round 1 applied — awaiting re-review. Date: 2026-08-18.
- **Commits** (`git log --oneline main..bt-5-docs`, newest first; the PROGRESS update
  itself follows them):

```
32a14a1 ARCHITECTURE: the token layer as it now stands
0e9b3e9 DESIGN_RATIONALE: the entries that named the retired token API say the current truth
de59bef DESIGN_RATIONALE: the six decisions behind the token-layer redesign
```

- **Files touched**: `dev-docs/ARCHITECTURE.md`, `dev-docs/DESIGN_RATIONALE.md`, this
  file. No code, no `docs/*.md`, no `CLAUDE.md`.

### New DESIGN_RATIONALE entries (§7 items 1–6)

| Label | Title | One line |
|---|---|---|
| `[§dd-dr:token-opacity]` | Tokens are opaque; only their reader interprets them | `Lang::Token` behind the `Token<L>` marker trait; the reader answers what a token is (the `TokenKind` view, spellings and no spans) and where it is; `StdToken` stores ranges and `Arc`s; the view borrows the token, never the reader; reader-less parties get token + reader; `CallableQuery` sees no token |
| `[§dd-dr:stream-position]` | Stream positions are opaque and cannot be forged | `Lang::StreamPosition`, issued only by the reader, no constructor and no arithmetic; the five `TokenEdge`s incl. `ContentStart`; `move_to`/`move_to_position` as the only moves; equality-only comparison; the list reader rejects what it never issued |
| `[§dd-dr:no-context-source]` | `ParseContext` carries no source handle | spans come from the reader (`here`, `source_span_within`, the token span answers); node data converts through `node_text_content` after a `same_source` check; `stage_invocation`'s three-case end rule |
| `[§dd-dr:reader-context-purity]` | The token reader sees only the parsing state | `peek` takes `&Arc<ParsingState<L>>` and nothing else; anything more is taken at construction; an expanding reader owns its own depth limit and leans on source provenance |
| `[§dd-dr:specials-scan-errors]` | Specials scanning reports errors, never recoveries | the hook works on a `&str` and can name neither a token nor a position; the reader lifts a `SpecialsScanError` into an unrecoverable `TokenError`; the name is the matched text; a bad match end is an implementation error, not a panic |
| `[§dd-dr:token-reader-hook]` | `make_token_reader` is where a custom tokenizer is installed | on `ParseDriver`, the one item with no default (a default body cannot type-check for a generic `L`); both construction sites route through it; the standard body is one line |

Every one of the six is referenced from ARCHITECTURE (gate below).

### Amended DESIGN_RATIONALE entries

| Label | What changed |
|---|---|
| `[§dd-dr:source-cursor-retired]` | the bidirectional-repositioning argument now names `move_to`/`move_to_position` instead of `move_to_pos`/`resume_pos` |
| `[§dd-dr:token-model]` | the "final model" line describes the current kind taxonomy reported through the reader's view (no stored spans, no lifetime); the specials match carries the resolution with the name as matched text; post-space is a reader answer between two edges; `TokenError<L>`; the two rejected-alternative lines that named `move_past`/the skip flag |
| `[§dd-dr:zero-copy-tokens]` | rewritten for `StdToken` (ranges + `Arc`s, no strings, no lifetime); the "revisit if" is answered by opacity; title dropped "ephemeral lifetime" |
| `[§dd-dr:token-reader]` | the protocol paragraph: speculative `peek` plus edge-named repositioning replaces the two-flag `move_past`/`move_to`; `peek` takes `&Arc<ParsingState<L>>` and nothing beyond the state; idempotence is per *stream* position |
| `[§dd-dr:token-contract-hardening]` | item 1 rewritten around the `ContentStart` edge (the comment sub-spans are reader answers); item 2's resume wording; **item 4 records the conscious reversal** (2026-08-17: the required positional move is `move_to_position(&L::StreamPosition)`, not `move_to_pos(usize)` — the capability stays required); item 5's doctrine sentence no longer names `ParseContext::source` |
| `[§dd-dr:token-list-reader-demoted]` | now also records the forged-token/position guard (rejects tokens and positions it never issued) as the second half of the agreement harness |
| `[§dd-dr:parse-context]` | **not in §7's table** — it described the removed `source: Arc<Source>` field; rewritten to the four inputs and pointed at `[§dd-dr:no-context-source]` |
| `[§dd-dr:invocation-parser-factory]` | the `Invocation<'a, L>` spelling; the composition finding's second half (a stored token *is* handed back to the reader now — `move_to(self.invocation.token, TokenEdge::End)`) |
| `[§dd-dr:stop-conditions]` | the predicate takes `(&L::Token, &dyn TokenReader)`; `StopCause`'s two token causes carry `after`; consume is the `EndPastPostSpace` edge, the unconsumed park is `Start` |
| `[§dd-dr:panic-policy]` | rule 3(b): five value functions plus the seven span-taking `StdToken` constructors (the eighth takes no span); `Token::new` is gone |
| `[§dd-dr:tolerant-parsing]` | `TokenError<L>` carries a source-qualified `SourceSpan`, `TokenRecovery` an explicit `resume` stream position |
| `[§dd-dr:err-means-abort]` | the content loop repositions to the recovery's resume position |
| `[§dd-dr:resume-pos-contract]` | retitled (the label is unchanged); the whole entry re-spelled for `TokenRecovery::resume` + `move_to_position`, the check is equality, and readers are now the only party that can violate it |
| `[§dd-dr:token-diagnostics]` | **not in §7's table** — it claimed the specials scan participates in the recovery protocol; corrected, with a pointer to the new entry |
| `[§dd-dr:specs]` topic, the scope-stack fold paragraph | **not in §7's table** — provider-side `scan_specials` returns `Result<Option<SpecialsMatch<L>>, SpecialsScanError>`, not `TokenResult` |
| `[§dd-dr:parse-driver]` | **not in §7's table** — "defaulted methods only" and "every trait item is defaulted" corrected to "all but one"; the `ParseContext` field list drops `source` |
| `[§dd-dr:rejected-patterns]` | the uniform-`post_space` bullet no longer says "an accessor serves `move_past`" (this is the line §7's table attributed to `[§dd-dr:preset-driver-pillars]`, whose `####` heading merely precedes it) |
| `[§dd-dr:preset-driver-pillars]` | **untouched after review round 1** — the round-1 title change was reverted to `main`'s wording; see review round 1, fix 12 and open question 1 |
| `[§dd-dr:superseded-names]` | a new bullet with the redesign's superseded names (PLAN §1.14): the `Token<'s, L>` struct and `Token::new`, span-carrying `TokenKind` fields, `TokenKindView` and `move_to_edge` as interim names, `pos`/`move_to_pos`/`move_past`/two-flag `move_to`, `TokenRecovery::resume_pos`, `ParseContext::source`, `end_pos`, `SpecialsMatch<'s, L>`/`::name`, the three `'s`-carrying type spellings, `CallableQuery::token`/`with_token` and the view-only successors, `Invocation::kind`, the bare-view hook signatures, the reader-less `resolve_command`, the old paragraph-break hook and `probe_token`'s source parameter |

### ARCHITECTURE sections touched

| Section | Change |
|---|---|
| `[§dd-arch:token]` | rewritten: a token is opaque and reader-interpreted; the reader's three question families (what a token is — the view; where it is — spans over the five edges; where the stream stands — positions); `StdToken`/`StdStreamPosition`; the trait's contract clauses in one paragraph; the custom-reader-over-standard-tokens pattern and the two-reader agreement harness; `make_token_reader` as the installation point; the four new labels added to the decisions list |
| `[§dd-arch:constructs]` | `ParseContext` without a source handle and how a parser obtains spans and positions; the content loop reads `tokens.token_kind(&tok)` and `resolve_command(state, &tok, tokens)`; a new bullet for the parse outputs that carry stream positions (`StopCause::after`, `EnvironmentBody::end`, `NameGroup`, `ArgumentNoise::start`) and one for `Invocation` = resolution result + token, with the hooks that receive token + reader; `[§dd-dr:no-context-source]` added to the decisions list |
| `[§dd-arch:engine]` | `make_token_reader` in the driver's inventory as the one undefaulted item; `probe_token` named; `resolve_command` receives the token and its reader; "every item is defaulted" corrected; `[§dd-dr:token-reader-hook]` added to the decisions list |
| `[§dd-arch:errors]` | `TokenError` carries a source-qualified location; `TokenRecovery::resume` is a stream position that must move the reader |
| `[§dd-arch:arch]` | the S1 line of the layer diagram lists the current token items |
| `[§dd-arch:naming]` | untouched — no new naming principle emerged (§7 expected none) |

### Gate results (verbatim)

```
$ for l in token-opacity stream-position no-context-source reader-context-purity \
      specials-scan-errors token-reader-hook; do printf "%s: " "$l"; \
      git grep -c "§dd-dr:$l\]" dev-docs/ARCHITECTURE.md; done
token-opacity: dev-docs/ARCHITECTURE.md:4
stream-position: dev-docs/ARCHITECTURE.md:3
no-context-source: dev-docs/ARCHITECTURE.md:2
reader-context-purity: dev-docs/ARCHITECTURE.md:2
specials-scan-errors: dev-docs/ARCHITECTURE.md:2
token-reader-hook: dev-docs/ARCHITECTURE.md:3

$ git grep -n 'bettertokens\|Stage [0-9]\|bt-[0-9]' dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md
(no output; exit 1)

$ git grep -n 'move_to_pos\b\|resume_pos\|Token::new\|Token<.s\|move_past\|cx\.source\b\|token_kind: Option\|with_token\b' \
      dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md
dev-docs/DESIGN_RATIONALE.md:936:   as a conscious reversal):* the required method was `move_to_pos(pos: usize)`, taking a
dev-docs/DESIGN_RATIONALE.md:6335:  [§dd-dr:no-context-source]): `Token<'s, L>` as a struct with a lifetime — the token
dev-docs/DESIGN_RATIONALE.md:6337:  **trait** on `Lang::Token`; with it `Token::new` — one constructor per kind
dev-docs/DESIGN_RATIONALE.md:6342:  (the view *is* `TokenKind`); `TokenReader::{pos, move_to_pos, move_past,
dev-docs/DESIGN_RATIONALE.md:6343:  move_to(&token, bool)}` and `StdTokenReader::{pos, move_to_pos}` — the two moves are
dev-docs/DESIGN_RATIONALE.md:6345:  `move_to_edge`; `TokenRecovery::resume_pos` — the field is `resume`, a stream
dev-docs/DESIGN_RATIONALE.md:6351:  `CallableQuery::with_token` (a token handed to a party with no reader to read it with)
      (line 936 = the reversal note in [§dd-dr:token-contract-hardening] item 4;
       6335-6351 = the [§dd-dr:superseded-names] register — nothing else)

$ git diff main..bt-5-docs | grep "^+" | grep -i -n \
      "\bdoor\b\|funnel\|\bmint\|trigger token\|vocabulary\|\bfacts\b\|footgun\|heart of\|on-ramp\|pillar"
553:+#### The preset driver: public behavior functions + the generic `LatexlikeDriver<LLL>` assembly [§dd-dr:preset-driver-pillars]
      (the sole hit is the immutable label in a retitled heading, as §7 allows)

$ grep -c '^```' dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md
dev-docs/ARCHITECTURE.md:4
dev-docs/DESIGN_RATIONALE.md:4
      (both even; and a scripted check finds 183 `####` entries, 183 distinct labels,
       0 without a `Status:` line)

$ git diff --stat main..bt-5-docs
 dev-docs/ARCHITECTURE.md     | 165 ++++++++++---
 dev-docs/DESIGN_RATIONALE.md | 562 ++++++++++++++++++++++++++++++++-----------
 2 files changed, 553 insertions(+), 174 deletions(-)

$ cargo build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.26s
```

Two notes on the greps. `cx\.source` without a word boundary also matches the *current*
`cx.source_span_within` (two hits in ARCHITECTURE), and `move_to_pos` without one matches
the current `move_to_position` (five hits across both files); the boundaried pattern above
is the one that separates stale names from live ones.

### Decisions taken under §1.16

None: this stage adds no API and takes no design decision. Where a document sentence and
PLAN §1 disagreed, the merged code decided — see the open questions.

### Deviations from §7

1. **The entry list grew by five amendments §7's table does not name**, each because the
   entry stated something the merged code makes false: `[§dd-dr:parse-context]` (the
   removed `source` field), `[§dd-dr:token-diagnostics]` (the scan "participates in the
   recovery protocol"), the scope-stack specials fold inside `[§dd-dr:specs]`
   (provider-side `scan_specials` returning `TokenResult`), `[§dd-dr:parse-driver]`
   ("defaulted methods only", and `source` in the context field list), and
   `[§dd-dr:rejected-patterns]` (the `move_past` accessor line — which is the line §7's
   table attributed to `[§dd-dr:preset-driver-pillars]`; the mapping picked the nearest
   preceding `####`, but the text sits under the `##`-level rejected-patterns list).
2. **§7 item 6's proposed label was `[§dd-dr:token-reader-door]`**; the entry carries
   `[§dd-dr:token-reader-hook]`, since "door" is a banned word and the label is an
   address that cannot be fixed later.
3. **§7 item 2 says "the four `TokenEdge`s"** — there are five since ruling O-3; the
   entry says five.

### Open questions

1. **`[§dd-dr:preset-driver-pillars]`: title fixed, body not.** ~~The heading no longer
   says "pillar", but the entry's body uses the word about fifteen times, and so do
   `techy/src/latexlike/driver.rs`'s section comments and four test names.~~
   **Answered in review round 1 (orchestrator):** the title is reverted to `main`'s
   wording. A title-only rename is inconsistent with a body, a label, and code that keep
   the term, and the pre-existing word is owned by the standing documentation
   walk-through recorded in `TODO_Big.md` — which is where the whole rename belongs.
2. **Where PLAN §1 and the merged code disagreed, the documents describe the code.**
   The three places: the number of token edges (five, §1.17 ruling O-3 — §7 item 2 still
   said four); `Invocation` has no `kind` field and the resolve chain takes the token and
   its reader (rulings O-5 and O-1b — §7 item 1's "cached view" and probe P2's finding are
   recorded as *rejected*, which is what they became); and `make_token_reader` is a
   required method (ruling O-4 — §7 item 6 already says so).
3. **ARCHITECTURE is 78 KB**, against the ~50 KB target its own maintenance section
   states; this stage added about 4 KB, almost all of it the rewritten token section. The
   standing simplification pass in `TODO_Big.md` owns the reduction.

### Review round 1 (fixes applied 2026-08-18)

The stage came back with seven required fixes (stale API spellings that survived the
sweep, all in DESIGN_RATIONALE unless noted), four adopted suggestions, and one
orchestrator ruling. Every claim below was re-verified against the merged tree.

1. `[§dd-dr:callable-query]` — the query no longer carries a token. Field list is
   `callable_type`, `name`, `syntax` (`scopes/mod.rs:106-114`, ruling O-1b); the "why the
   token too" paragraph is replaced by "why no token" (scopes and packages look up by
   name and callable syntax; a language that must dispatch on token details does so in
   `ParseDriver::resolve_command`, which receives the token and its reader); the
   token-carrying form moved to `Rejected alternatives:` with its killing flaw (a
   provider holds no reader and cannot read an opaque token); the "a token carries spans
   and borrowed substrings" clause is gone — the escape character is query data precisely
   because providers see no token; "and token alike" dropped.
2. ARCHITECTURE `[§dd-arch:specs]` lookup line: "(name, form, syntax, optional token)" →
   "(name, form, syntax)".
3. `[§dd-dr:resolve-command-hook]` — the merged shape:
   `resolve_command(&self, state, token: &L::Token, tokens: &dyn TokenReader<'_, L>) ->
   Result<CommandResolution<L>, ParseError<…>>`, with a parenthetical that the hook lives
   on `ParseDriver` (the heading's `Lang::` prefix and the label are untouched), and the
   scopes query built from `tokens.token_kind(token)` (`engine/driver.rs:1001-1005`).
   Follow-on in `[§dd-dr:resolution-detail]`: the quoted default detail string now matches
   the code ("…by this language's driver — implement `ParseDriver::resolve_command`…",
   `engine/driver.rs:712-714`).
4. `[§dd-dr:paragraph-break-hook]` —
   `make_paragraph_break_node(&self, state, break_span: &SourceSpan<L::SourceOrigin>) ->
   NodeKind<L>` on `ParseDriver` (`engine/driver.rs:337`); the core stages the returned
   kind with the span it passed in; a callable-shaped kind takes the break's spelling from
   `break_span.content()` and its payload from `LatexlikeInvocationSyntax::specials_form()`
   (`latexlike/driver.rs:273-288`).
5. `[§dd-dr:command-escape-char]` — the view is `Command { name, escape_char }`
   (`token/token.rs:103-109`); the post-space is named as the reader's
   `End..EndPastPostSpace` answer, not a variant field.
6. `[§dd-dr:takeover-staging-sugar]` — `end: Option<&L::StreamPosition>`, and the
   default rule as implemented (`constructs/mod.rs:357-400`): the last staged child's span
   end **when that child lies in the trigger's source**, otherwise the current stream
   position.
7. `[§dd-dr:span-invariants]` — `stage_invocation(.., end: Some(&position))`; item 4's
   end-of-stream whitespace is a reader answer
   (`source_span_between(&tok, StartBeforePreSpace, Start)`), not a `pre_space` field read.
8. `[§dd-dr:zero-copy]` principle — "transient borrow lifetimes (tokens borrowing the
   current source)" → transient borrows **held by a token reader** over the source it is
   scanning; the standard token has no lifetime.
9. `[§dd-dr:zero-copy-tokens]` — the "answered by opacity" sentence moved into the body;
   `Revisit if:` now states a condition (the standard reader must serve content it cannot
   slice out of a single `&str`).
10. `[§dd-dr:specials-scan-errors]` — the reader validates the hook's **error span** as
    well as the match end before qualifying it with its own source, answering an
    unrecoverable implementation error rather than panicking on the slice
    (`token/reader.rs:622-645`, `:565-584`).
11. `[§dd-arch:token]` — `next` is named (peek + `move_to(&token, EndPastPostSpace)`), so
    the section covers all eleven trait methods.
12. `[§dd-dr:preset-driver-pillars]` — **title reverted to `main`'s wording** (orchestrator
    ruling; the answer to open question 1 above). A title-only rename is inconsistent with
    the label, the body, and `latexlike/driver.rs`; the pre-existing banned word belongs to
    the standing documentation walk-through in `TODO_Big.md`.

One further correction was made while verifying fix 10: `[§dd-dr:token-model]`'s specials
bullet still said "the scan returns `TokenResult`, so scanner errors participate in the
recovery-token protocol" — flatly contradicted by `Lang::scan_specials`
(`state/lang.rs:433-437`) and by the new entry. It now states the real answer type and
points at `[§dd-dr:specials-scan-errors]`.

### Gate results, review round 1 (verbatim)

```
$ for l in token-opacity stream-position no-context-source reader-context-purity \
      specials-scan-errors token-reader-hook; do printf "%s: " "$l"; \
      git grep -c "dd-dr:$l" dev-docs/ARCHITECTURE.md; done
token-opacity: dev-docs/ARCHITECTURE.md:4
stream-position: dev-docs/ARCHITECTURE.md:3
no-context-source: dev-docs/ARCHITECTURE.md:2
reader-context-purity: dev-docs/ARCHITECTURE.md:2
specials-scan-errors: dev-docs/ARCHITECTURE.md:2
token-reader-hook: dev-docs/ARCHITECTURE.md:3

$ git grep -n 'bettertokens\|Stage [0-9]\|bt-[0-9]\|PROBE_REPORT\|compiler probe' \
      dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md
(no output; exit 1)

$ git grep -n 'move_to_pos\|resume_pos\|Token::new\|Token<.s\|move_past\|cx\.source\|token_kind: Option\|with_token\|end_pos\|optional token\|resolve_command(state, &token\|make_paragraph_break_node(state, &token' \
      dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md
ARCHITECTURE.md:247,664 + DESIGN_RATIONALE.md:312,696,709,762,5553    — live
      `move_to_position` (the patterns are unboundaried)
ARCHITECTURE.md:617,665                                              — live `cx.source_span_within`
DESIGN_RATIONALE.md:942,943                                          — the one reversal note
      ([§dd-dr:token-contract-hardening] item 4)
DESIGN_RATIONALE.md:3478                                             — the live S0 accessor
      `SourceSpan::start_pos()/end_pos()` (`source/source.rs:341,348`)
DESIGN_RATIONALE.md:6347,6349,6354,6355,6356,6357,6359,6363,6365     — the
      [§dd-dr:superseded-names] register
(21 lines total; nothing else. The two round-1 spellings
 `resolve_command(state, &token` and `make_paragraph_break_node(state, &token` are gone.)

$ git diff main..bt-5-docs -- dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md \
      | grep "^+" | grep -i -c "\bdoor\b\|funnel\|\bmint\|trigger token\|vocabulary\|\bfacts\b\|footgun\|heart of\|on-ramp\|pillar"
0
      (the round-1 revert removed the sole previous hit — the retitled
       `preset-driver-pillars` heading is identical to `main`'s again, so it no longer
       appears in the diff at all. Run over the whole diff, the remaining hits are this
       PROGRESS file's own record of the ruling and of the rejected `token-reader-door`
       label, which is where that discussion belongs.)

$ grep -c '^```' dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md
dev-docs/ARCHITECTURE.md:4
dev-docs/DESIGN_RATIONALE.md:4
      (both even; the scripted check finds 183 labelled `####` entries — the 184th is the
       maintenance section's template line — 183 distinct labels, 0 without a `Status:`)

$ git diff --stat main..bt-5-docs
 dev-docs/ARCHITECTURE.md          | 169 +++++++---
 dev-docs/DESIGN_RATIONALE.md      | 646 ++++++++++++++++++++++++++++----------
 dev-docs/bettertokens/PROGRESS.md | 163 +++++++++-
 3 files changed, 766 insertions(+), 212 deletions(-)
```
