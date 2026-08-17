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
- **Status**: implemented — awaiting review. Date: 2026-08-17.
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
| `techy/src/token/reader.rs` | `TokenEdge`, `StdStreamPosition`; `StdTokenReader<'s, O: SourceOrigin = Option<String>>` built from `&'s Arc<Source<O>>` (new `source()` accessor, `nearest_valid_offset` helper); the P8 impl header and the same bound on the scanning core; the eight new trait methods + contract clauses 1–6 + the custom-reader pattern in the trait rustdoc; error construction through `SourceSpan`; scan-error lift; five new unit tests |
| `techy/src/token/error.rs` | `TokenError::span: SourceSpan<L::SourceOrigin>` (`span()` returns `&SourceSpan`), `TokenRecovery::resume: L::StreamPosition` (was `resume_pos: usize`) with the reworded advancement contract |
| `techy/src/token/specials.rs` | `SpecialsMatch<L>` (no lifetime, no `name`), new `SpecialsScanError { kind, span }` with the rationale in its rustdoc |
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
running 1025 tests
test result: ok. 1025 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.62s
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
test result: ok. 78 passed; 0 failed; 0 ignored; 0 measured; 947 filtered out; finished in 0.00s

$ cargo test -p techy --lib token::list_reader
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 1014 filtered out; finished in 0.00s
  (includes positions_and_spans_match_the_std_reader_in_lockstep and the two
   should-panic negatives)
```

### Semver report (`scripts/check_semver.sh`)

Breaking changes are expected (soft freeze) and were not "fixed".

```
     Summary semver requires new major version: 9 major and 0 minor checks failed
    Finished [   9.283s] techy
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
    seeded with the initial position **and every edge offset of every listed token**,
    and extended by every peeked token's edge offsets and by every
    `position_here`/`position_at` answer. The set lives behind a `RefCell` because the
    position accessors take `&self`.
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
   offending one. No behavior change beyond the anchor.

4. **`StdTokenReader::source()`** (new public inherent accessor) — the in-crate
   delegating test readers need the reader's source to build their own `TokenError`
   spans. It is the natural sibling of the existing `content()` accessor; say the word
   and it can be `pub(crate)`.

5. **`TokenListReader` position validation seeds the issued set with the listed tokens'
   edges** (not only with the initial position, as §1.8's parenthesis suggests).
   Reason: `peek` clips `pre_space`, `move_to_pos` (still present in Stage 1) can put the
   reader anywhere, and the harness compares readers run-by-run; seeding keeps every
   legitimate lockstep test passing while a position taken from a place this reader never
   served is still rejected (the negative test takes one from a std reader over a longer
   stretch of the same source).

### Open questions

1. **The `make_token_reader` default** (deviation 1) — the plan's prescribed default is
   not implementable; the user must choose (A) required on `ParseDriver`, as
   implemented, or (C) a `Lang`-side factory. Everything else in the stage is
   independent of the answer.
2. **`StdTokenReader::source()` visibility** (deviation 4) — public inherent accessor
   or `pub(crate)`? Public reads naturally next to `content()`, and a third-party
   reader wrapping a `StdTokenReader` plausibly wants it.
