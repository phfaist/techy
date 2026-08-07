# Stack depth of the parsing engine

*Exploration report — no library code changed. Measures how much **stack** one level of
input nesting costs, why the parser overflows at depths a real document can reach, and
what the options are. Companion to `TechyParsingMemoryFootprint.md`, which covered
**heap** bytes per document; this one is about **stack bytes per nesting level**.*

---

## 0. Executive summary

The report is correct, and the failure is not exotic: **a plain `cargo test` aborts on
61 nested `{}` or 46 nested `\begin{itemize}`** — not with a `ParseError`, but with
`fatal runtime error: stack overflow` (SIGABRT), taking the whole test binary down.

The cause is not deep recursion. It is that **one nesting level costs 4.3–8.0 KB of
stack in release and 35–48 KB in debug**, against a `walk()` of the same tree at
**96 B/level**. The parser spends 45–80× more stack per level than a plain traversal of
what it produces.

| Per nesting level | release (`opt-level=3`) | debug |
|---|---|---|
| `{…}` group | **4.31 KB** | 34.95 KB |
| `\emph{…}` macro argument | **7.55 KB** | 47.66 KB |
| `\cite[…]` optional argument | **7.50 KB** | 45.59 KB |
| `\begin{itemize}…\end{itemize}` | **7.94 KB** | 47.66 KB |
| — `walk()` over the parsed tree | 0.096 KB | — |
| — `recompose()` over the parsed tree | 0.290 KB | — |
| — `validate_tree()` / `Debug` / `drop` | *iterative, 0* | — |

Maximum nesting depth before abort:

| stack | `{…}` | `\begin{itemize}` |
|---|---|---|
| debug, libtest thread (`cargo test`) | **61** | **46** |
| release, 1 MiB (wasm32 default) | 241 | 131 |
| release, 2 MiB | 484 | 263 |
| release, 8 MiB (Linux main thread) | 1 946 | 1 057 |

Four findings:

1. **There is no nesting-depth limit anywhere in the engine.** The only `max_depth` in
   the crate is `resolve_source_reference`'s *inclusion* depth — a different axis, and
   explicitly documented as embedder policy. Input nesting is unbounded, so overflow is
   reachable from any untrusted input.
2. **Stack overflow is not a catchable failure.** It aborts the process: no `Result`, no
   `catch_unwind`, no diagnostic. This is a strictly worse outcome than the panics
   CLAUDE.md rule 4 / [§dd-dr:panic-policy] already forbid on input, and it is a DoS
   vector for any embedder parsing untrusted `.tex`.
3. **No single type is to blame.** The largest value in the cycle is 472 B; the frames
   are 1.5–1.8 KB because 13–27 *distinct* medium-sized values are live at once
   (§3). There is no one-line fix.
4. **A depth limit alone is not sufficient.** A limit low enough to be safe in a debug
   build on a 2 MiB test thread is ~40 levels — low enough to reject legitimate
   documents. Either the limit is generous and debug builds still abort, or it is safe
   and it rejects real input. Getting both requires bringing the per-level constant down
   as well (§5).

---

## 1. Method

- **Per-level cost**: each trial runs in a subprocess (an overflow aborts the process),
  on a thread with an explicit `stack_size`; the parent binary-searches the deepest
  input that survives. `bytes/level = stack_size / max_depth`. Harness:
  `techy/examples/stack_probe.rs`, `cargo run --release --example stack_probe -- <kib>`.
- **Cross-check**: the same figure falls out at a 500× different scale — parsing on a
  512 MiB stack survives 124 735 group levels = 4 300 B/level, against 4 311 B/level
  measured at 1 MiB.
- **Frame census**: temporary `stackmark!` instrumentation recorded the stack pointer at
  the top of each function in the descent cycle; deltas between consecutive marks give
  the per-level frame chain. Reverted after measurement.
- **Frame sizes**: `sub $N,%rsp` in each prologue, read from `objdump -d` of the release
  binary, plus callee-saved pushes and the return address.
- **Type sizes**: `size_of` from a temporary in-crate test module (so `pub(crate)` types
  were reachable). Reverted after measurement.
- **`cargo test` thresholds**: separate `#[test]` fns at fixed depths, run one at a time
  (an overflow kills the binary, so a bisection cannot share a process).
- gdb was not usable in the measurement container (ptrace restrictions); the census
  instrumentation replaced it.

---

## 2. The recursion cycle

For a plain `{…}` group, one nesting level is exactly this chain (release; measured
frame deltas, total **4 288 B**, against 4 311 B/level measured end-to-end):

```
ParseContext::parse_nodes            528 B   ← parse_scoped + with_parsing_state inlined in
  NodesParser::parse                1552 B
    ParseContext::parse_group       ~370 B   ← inlined, still allocates slots
      GroupParser::parse            1840 B   ← with_frame inlined in
        ParseContext::parse_nodes            ← recurse
```

`NodesParser::parse` (1 552 B) and `GroupParser::parse` (1 840 B) are **79 % of the
per-level cost**; the `ParseContext` plumbing is the remaining 21 %.

Macro arguments and environments add an `StdInvocationParser::parse` (624 B) plus an
argument parser (`GroupArgumentParser::parse_argument`, 944 B) or an
`EnvironmentInvocationParser::parse` (1 744 B) + `EnvironmentBodyParser::parse`
(2 064 B) to the same cycle — hence 7.5–8.0 KB instead of 4.3 KB.

Two notes on scope:

- **The recursion is a public extension point.** `ConstructParser::parse(&mut self, cx)`
  is documented to call `cx.parse_nodes(…)` / `cx.parse_group(…)` reentrantly
  (`docs/construct-parsers.md`, `docs/ai-guide-custom-lang.md`). Outer-layer parsers sit
  *inside* the cycle. This is what rules out trampolining (§5, option D).
- **`\input` recursion rides the same stack.** `parse_attached_source` builds a fresh
  inner `ParseContext` over the attached content but shares the session, and the core
  performs no recursion checking by design (embedder policy, `source/resolver.rs`). A
  self-including source recurses on the stack like any other nesting.

---

## 3. Why the frames are 1.5–1.8 KB

Not one big value. The largest type in the cycle is the construct-parser return itself:

| type | size |
|---|---|
| `Result<(NodesOutcome<L>, Option<ParsingStateDelta<L>>), ParseError>` | **472** |
| `NodesParser<'_, L>` | 352 |
| `NodesOutcome<L>` | 264 |
| `Result<(BuildId, Option<ParsingStateDelta<L>>), ParseError>` | 216 |
| `ParsingStateDelta<L>` / `ParsingState<L>` | 208 |
| `Token<'_, L>` | 88 |
| `Frame<L>` | 80 |
| `ParseError` | 64 |
| `StopSpec<'_, L>` / `ChildStateSpec<'_, L>` | 48 each |

Counting address-taken stack slots in the disassembly: **`GroupParser::parse` holds 13
distinct slots** (largest gaps 240, 208, 192, 120, 112, 80, 80, 80 B) and
**`NodesParser::parse` holds 27** across 2 907 instructions. Both functions are large
and branch-heavy — `NodesParser::parse` alone has 20 call sites to `outcome`, 18 to
`flush_through`, and inlines the condition-building and message-formatting paths
(`alloc::fmt::format::format_inner` ×6, `snapshot_frames` ×4) into arms that are cold in
every real parse. LLVM's stack colouring does not merge those slots, so every level pays
for every arm.

**Negative result worth recording**: marking the recovery *entry points*
(`ParseContext::recover`, `recover_boxed`, `implementation_error`) `#[cold]
#[inline(never)]` changed nothing (241 → 239 levels, i.e. noise). Those already sit
behind an out-of-line `driver.recover` call. If outlining is going to help, it has to be
applied to the *arm bodies* that construct conditions and format strings, not to the
funnel — and it must be measured, not assumed. There are currently **zero** `#[cold]` or
`#[inline(never)]` attributes in the crate.

---

## 4. Where this bites

- **The test suite.** Any acceptance test that nests past ~46 environments or ~61 groups
  kills the whole binary. This is what the original report hit.
- **wasm32.** The default linear-memory stack is 1 MiB → ~131 environment levels in
  release. wasm32 is not installed in this container, so the *behaviour* on overflow
  there is unverified and should be checked before relying on it — a trap and a silent
  write past the shadow stack are very different failure modes.
- **`no_std` embedders**, who typically have far less than 1 MiB.
- **Untrusted input.** `{{{{…}}}}` ×2 000 is 4 KB of input and aborts an 8 MiB-stack
  process. There is no way for an embedder to defend against this today, because the
  library exposes no limit and the failure is not catchable.

Legitimate LaTeX does nest — `document` > `figure` > `center` > `tabular` > `{}` >
`\textbf{}` is depth 6 before any content — so a real document reaching depth 20–30 is
unremarkable. Depth 46 in a debug build is not a comfortable margin.

---

## 5. Options

**A. Add a nesting-depth limit. (Recommended, and needed regardless of the rest.)**

The engine already counts descent depth: `ParserSession` maintains the enclosing-state
stack (`ParsingStateStack`), pushed and popped by `ParseContext::with_parsing_state` —
the single funnel every descent passes through (`parse_scoped` → `with_parsing_state`;
argument parsing calls it directly). `.len()` is the counter; the session is shared with
nested `parse_attached_source` contexts, so the same counter covers include recursion for
free. Enforcement is a check in one place.

Shape, following existing precedent:

- a condition (`NestingTooDeep`, modelled on `UnclosedGroup`) reported through the
  recovery entry point — tolerant parses stage the over-deep content as `Chars` (the
  existing markup-in-chars recovery artifact) and unwind; strict parses abort with the
  traceback. A `Result`, never a panic — rule 4 satisfied;
- the limit configured on the driver, alongside `Recovery`.

Two design questions for you:

1. **Units.** The state-stack counter is in *engine-descent* steps, not user-visible
   nesting: a `{}` level costs two pushes, an environment more. Either the limit is
   documented in descent units (cheap, but the number means little to an embedder), or
   it is counted at `parse_nodes`/`parse_group` only, for a number that matches what a
   user would call "nesting depth".
2. **Default value.** This is the uncomfortable part — see below.

**B. Bring the per-level constant down.** Because a limit safe for a debug build on a
2 MiB test thread is ~40 levels, and one generous enough for real documents (say 256)
still overflows in debug, A alone does not close the hole. Candidates, roughly in
payoff order — all of them need measuring with the probe, not assuming:

1. **Box the pass-through delta** in the construct-parser return pair:
   `Option<ParsingStateDelta<L>>` (216 B) → `Option<Box<…>>` (8 B). It rides *every*
   construct-parser return at every level and is `None` on the overwhelming majority of
   them, so the allocation is rare. This changes a **decided** public signature
   ([§dd-dr:parsers-engine]) — your call.
2. **Shrink or box `NodesOutcome<L>`** (264 B), returned by value through every
   content-loop level.
3. **Outline the cold arms** of `NodesParser::parse` and `GroupParser::parse` into
   `#[inline(never)]` helpers — the condition-construction and formatting bodies, not
   the recovery funnel (§3's negative result).

Realistic expectation: 2×, maybe 3× — enough to make a generous limit safe in release
and a modest one safe in debug. Not enough to make the limit unnecessary.

**C. Grow the stack on demand (`stacker`-style).** Rejected: needs `std` and
per-platform support, does not work on wasm or `no_std`, and adds a dependency against
[§dd-dr:dependencies]. Mentioned only for completeness.

**D. Eliminate the recursion (explicit descent stack).** Rejected as impractical, not
merely expensive: the recursion *is* the extension contract (§2). Trampolining would
require every outer-layer `ConstructParser` to become a resumable state machine, which
trades the library's stated extensibility pillar for a bound that option A already
provides.

**E. Consumer-side traversals.** `walk` (96 B/level) and `recompose` (290 B/level) also
recurse without limits, and are public entry points that accept *any* `NodeTree` —
including one built through `NodeTreeBuilder` rather than produced by a bounded parse.
`validate_tree`, `Debug` and `Drop` are already iterative. `restage` was not measured; it
is structurally recursive like `recompose`. Low priority — the constants are 15–45×
smaller — but worth a note in the docs that a parse-side limit does not bound a
hand-built tree.

**Suggested order: A first** (it converts an uncatchable abort into a diagnostic, which
is the correctness fix), **then B.1–B.2 measured incrementally**, then re-tune A's
default upward with the headroom that buys.

---

## 6. Open questions

- Limit in descent units or user-visible nesting depth (§5, A.1)?
- Default limit, given that debug and release differ by 8× and wasm has 1/8 the stack of
  a Linux main thread? A single constant cannot be right everywhere — is the default
  chosen for release-on-desktop, with embedders expected to lower it, or for the worst
  case, with embedders expected to raise it?
- Is `B.1` (boxing the pass-through delta) acceptable against the decided
  [§dd-dr:parsers-engine] signature?
- Should the same limit cover `\input` recursion, or does that stay embedder policy as
  documented? (The shared session makes covering it nearly free.)
- Verify wasm32 overflow behaviour before quoting the 1 MiB numbers as safe.

Once a direction is chosen, this warrants a `DESIGN_RATIONALE.md` entry with an
`ARCHITECTURE.md` reference (CLAUDE.md rule 7); as an exploration report it follows the
`TechyParsingMemoryFootprint.md` precedent and adds none yet.

---

## 7. Reproducing

```bash
cargo run --release --example stack_probe -- 1024   # per-level cost at a 1 MiB stack
cargo run           --example stack_probe -- 1024   # the debug figures
```

The probe covers both axes: `PARSE` binary-searches input nesting depth per construct,
`CONSUME` parses a deep tree on a 512 MiB stack and then runs `walk` / `recompose` /
`validate_tree` / `Debug` / `drop` on the constrained stack.
