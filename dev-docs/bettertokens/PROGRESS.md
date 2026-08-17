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
- **Status**: implemented — awaiting review. Date: 2026-08-17.
- **Commits** (`git log --oneline main..bt-3a-view`, newest first; the PROGRESS update
  itself follows this list):

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
| `techy/src/token/reader.rs` | `TokenEdge::ContentStart` (fifth variant, between `Start` and `End`) with the `≤`-ordering rustdoc; `TokenReader::token_kind<'t>(&self, &'t Token<'_, L>) -> TokenKindView<'t, L> where 's: 't` (required, rustdoc per §1.6/§1.15) and its `StdTokenReader` impl; the trait's "Positions, edges, and spans" section and `move_to`'s docs restated for five edges; four new unit tests |
| `techy/src/token/list_reader.rs` | `token_kind` (same interpretation, issued-token check first); `EVERY_EDGE` and the lockstep edge matrix cover `ContentStart`; three new tests (view lockstep, the comment's delimiter/content as edges, a forged token rejected) |
| `techy/src/token/mod.rs`, `techy/src/core/mod.rs` | facades export `TokenKindView` (temporary — 3b renames it `TokenKind`); the token module's design highlights say what a token *is* is a reader answer too |
| `techy/src/scopes/mod.rs` | `CallableQuery<'a, L>` (the `'s` is gone): `token: Option<&Token>` → `token_kind: Option<TokenKindView<'a, L>>`, `with_token` → `with_token_kind`, rustdoc per §1.10; one new unit test |
| `techy/src/engine/driver.rs` | `ParseDriver::resolve_command`, `CommandResolver::resolve_command` and `resolve_command_in_scopes` take `token_kind: TokenKindView<'_, L>`; the query is built with `with_token_kind`; rustdoc on why a resolver sees the view |
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
techy/src/latexlike/invocation_syntax.rs:127:        match invocation.kind {
techy/src/latexlike/environments.rs:828:            self.invocation.kind
techy/src/latexlike/environments.rs:1045:        let command_end = match self.invocation.kind {
techy/src/constructs/argument_parsers.rs:352:        let spelling = match invocation.kind {
techy/src/constructs/mod.rs:1274:            .field("kind", &self.kind)
techy/src/constructs/nodes_parser.rs:211:        match self.kind {
techy/src/constructs/nodes_parser.rs:763:        let matches = match &cond.kind {
techy/src/constructs/nodes_parser.rs:1293:            .field("kind", &self.kind)
techy/src/scopes/mod.rs:2702:        assert!(error.kind.to_string().contains("scan broke"));
techy/src/serialize/**, techy/src/node/**, techy/tests/derive_conditions.rs (node and
diagnostic data, not tokens — elided here for length)
```

**No token-kind read is left.** The nine remaining lines are: `invocation.kind` (the
view field this stage adds — read, not computed), the `Debug` impls of `Invocation` and
`TokenStopCondition`, `UnusableRecoveryToken.kind` and `TokenStopCondition.kind` (the
parsers' own condition enums), and a `SpecialsScanError.kind` assertion. The `serialize`
/ `node` hits are `NodeData.kind` and wire-record fields.

Token *span* / *whitespace* reads outside the token module:

```
$ grep -rn "token\.span\|tok\.span\|\.pre_space\b\|\.post_space()" techy/src techy/tests docs \
    | grep -v "^techy/src/token/"
techy/src/latexlike/arguments.rs:727:        assert_eq!(m.post_space(), Some(" "));
techy/src/latexlike/invocation_syntax.rs:530,535,586,643,733
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
