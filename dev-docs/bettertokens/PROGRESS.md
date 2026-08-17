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
- **Status**: implemented — awaiting review. Date: 2026-08-17.
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
