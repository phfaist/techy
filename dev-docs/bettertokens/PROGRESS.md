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
- **Status**: implemented — awaiting review. Date: 2026-08-17.
- **Commits** (`git log --oneline main..bt-2a-core`, oldest last):

```
<this commit> bettertokens: PROGRESS.md — Stage 1 merged, Stage 2a implemented
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
running 1034 tests
test result: ok. 1034 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.69s
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
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 955 filtered out; finished in 0.05s

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
4. **The latexlike `Specials` paragraph-break shape synthesizes a token.** The hook now
   receives only the break's span, but the payload still comes from
   `FromInvocation::from_invocation(&Invocation { token, .. })`, so the behavior function
   rebuilds a `ParagraphBreak` token from the span it was handed. This is an interim: in
   Stage 3b `from_invocation` also needs a *reader*, which this hook does not have — see
   open question 2.
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
   this path by construction.

### Open questions

1. **No existing test's expected node span changed.** The §1.9 rule was implemented as
   written and the whole suite passes unmodified (1034 unit + 75 integration + 84
   doctests), including the environment/`\input`/expression-position span assertions and
   the parse-tree byte-partition oracle. Nothing to rule on — recorded because §1.9 asked
   for it explicitly.
2. **The paragraph-break hook and `FromInvocation` (deviation 4).** In the final design
   `from_invocation(invocation, tokens)` needs a reader and an `L::Token`, and
   `make_paragraph_break_node` has neither. Stage 3b must decide how a callable-shaped
   paragraph break mints its invocation-syntax payload — a `Default`-ish payload, a
   dedicated hook parameter, or handing the hook the token after all. The interim
   (rebuild a token from the span) keeps today's behavior exactly and is confined to
   `latexlike/driver.rs`.
3. **`GroupParser`'s extra lifetime** (deviation 1) is churn that 3b undoes. If a
   reviewer prefers, the alternative is for `GroupParser` to store the open token's
   `SourceSpan` plus its `Start` position instead of the token — no lifetime, but it
   diverges from §1.9's "`GroupParser::new(open: L::Token, rule)`" and would have to be
   put back in 3b.
