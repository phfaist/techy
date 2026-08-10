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

| Per nesting level | x86-64 release | x86-64 debug | wasm32 release |
|---|---|---|---|
| `{…}` group | **4.31 KB** | 34.95 KB | **2.14 KB** |
| `\emph{…}` macro argument | **7.55 KB** | 47.66 KB | — |
| `\cite[…]` optional argument | **7.50 KB** | 45.59 KB | — |
| `\begin{itemize}…\end{itemize}` | **7.94 KB** | 47.66 KB | **3.67 KB** |
| — `walk()` over the parsed tree | 0.096 KB | — | — |
| — `recompose()` over the parsed tree | 0.290 KB | — | — |
| — `validate_tree()` / `Debug` / `drop` | *iterative, 0* | — | — |

wasm32 costs about half of x86-64 per level: the shadow stack only holds *address-taken*
locals, and the dominant aggregates are themselves half-size there (§6.5).

Maximum nesting depth before abort:

| target / stack | `{…}` | `\begin{itemize}` |
|---|---|---|
| x86-64 debug, libtest thread (`cargo test`) | **61** | **46** |
| x86-64 release, 1 MiB | 241 | 131 |
| x86-64 release, 2 MiB | 484 | 263 |
| x86-64 release, 8 MiB (Linux main thread) | 1 946 | 1 057 |
| **wasm32 release, 1 MiB (the wasm default)** | **491** | **286** |

Four findings:

1. **Nothing bounds the recursion.** No stack budget, no depth limit. The only `max_depth` in
   the crate is `resolve_source_reference`'s *inclusion* depth — a different axis, and
   explicitly documented as embedder policy. Input nesting is unbounded, so overflow is
   reachable from any untrusted input.
2. **Stack overflow is not a catchable failure.** It aborts the process: no `Result`, no
   `catch_unwind`, no diagnostic. This is a strictly worse outcome than the panics
   CLAUDE.md rule 4 / [§dd-dr:panic-policy] already forbid on input, and it is a DoS
   vector for any embedder parsing untrusted `.tex`.
3. **One object family dominates: the pass-through delta.** Every object over 100 B on
   the cycle is `Option<ParsingStateDelta<L>>` (208 B), `NodesOutcome<L>` (264 B, of
   which 208 *is* that delta), or the return pair built from both (472 B). In
   `NodesParser::parse`'s debug frame those three account for **15 424 B of 25 376 —
   61 %** — and nearly every one of those slots carries `None`: every `Some(delta)`
   return site in the crate is inside a `mod tests` (§3).
4. **Bound the stack by measuring it, not by counting levels.** The only hard requirement
   is *never abort the process; always return an `Err`*, and a stack-consumption check at
   the descent funnel delivers exactly that — in pure `core`, so it works on wasm and
   `no_std` too. A nesting-depth limit cannot: to be reproducible across builds it would
   have to bind at ≤46 (the debug/libtest figure above), which no LaTeX parser can ship,
   and set anywhere sane it rejects valid documents in production without protecting the
   debug case. It is optional language policy, not the mechanism (§5 A, A′).

Separately, and larger than anything the library can do: **an embedder can simply give
the parse a bigger stack**. Measured on today's unmodified code — 2 000 nested
environments on a 16 MiB thread, 8 000 on 64 MiB, 30 000 on 256 MiB, 60 000 nested groups
on 256 MiB (§5 C′). `stack_size` reserves address space, so the pages only commit if
touched.

Measured remedy experiments (each applied alone to the pristine tree, then reverted):

| change | x86-64 release | x86-64 debug | wasm32 release |
|---|---|---|---|
| `NodesOutcome::after_effects` → `Option<Box<…>>` | −22 % (241 → 309) | −25 % (30 → 40) | **−38 %** (491 → 797) |
| **+ return-pair delta boxed** | **−42 %** (241 → 416) | — | **−52 %** (491 → **1 022**) |
| `ParseError` payload boxed | ~0 % | −17 % | — |
| `#[cold] #[inline(never)]` on the recovery entry points | none (241 → 239) | — | — |

**Boxing the delta roughly doubles the available depth on wasm32** — 491 → 1 022 nested
groups, 286 → 579 environments — at no fixed-stack cost, which is the lever that matters
where the stack is a link-time constant. It pays off more there than on x86-64 for the
reason in §6.5. The `+ return-pair` row is a mechanical
`Option<ParsingStateDelta<L>>` → `Option<Box<…>>` sweep: 17 files, 9 hand-fixes.

*(An earlier draft of this report concluded "no single type is to blame". That was wrong:
it was based on release-build slot counts before the DWARF and debug-slot census in §3.)*

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
- **Frame sizes**: `sub $N,%rsp` in each prologue, read from `objdump -d`, plus
  callee-saved pushes and the return address. **Debug frames need care**: functions with
  large frames get LLVM's inline stack-probe prologue (`sub $0x1000,%rsp; movq
  $0x0,(%rsp)` repeated, then a final `sub $rem,%rsp`), so the frame is the *sum* of the
  probes plus the remainder — reading only the first `sub` under-reports
  `NodesParser::parse` as 4 104 B instead of 25 376 B.
- **Named stack objects**: parsed out of DWARF (`llvm-dwarfdump --debug-info` on the debug
  build) — every `DW_TAG_variable` / `formal_parameter` with a `DW_OP_fbreg` location,
  joined to its type's `DW_AT_byte_size`. Concrete instantiations of generic functions
  carry no `DW_AT_linkage_name`; they point back to the abstract DIE via
  `DW_AT_specification`, which must be followed or the function looks empty.
- **Unnamed slots**: all distinct `lea off(%rsp)` offsets in a function body, histogrammed
  by the gaps between consecutive offsets — this is what exposes the repeated 472/264/208 B
  temporaries that DWARF does not name.
- **Type sizes**: `size_of` from a temporary in-crate test module (so `pub(crate)` types
  were reachable). Reverted after measurement.
- **`cargo test` thresholds**: separate `#[test]` fns at fixed depths, run one at a time
  (an overflow kills the binary, so a bisection cannot share a process).
- **wasm32**: a `cdylib` exporting `try_depth(kind, depth)`, built with
  `RUSTFLAGS="-C link-arg=-zstack-size=1048576"` and driven from Node 22 with the same
  binary search. Each trial needs a **fresh `WebAssembly.Instance`** — a trap leaves the
  module's memory unusable, so reusing one silently poisons every later trial. Type sizes
  for wasm32 come from `const _: [(); 0] = [(); size_of::<T>()];`, read out of the
  resulting type error (no runtime needed). Both harnesses live outside the repository.
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

In **debug** nothing inlines, so the chain has more distinct frames — and one of them
dominates:

| frame | debug | (release) |
|---|---:|---:|
| `ParseContext::parse_nodes` | 224 | 528 |
| `ParseContext::parse_scoped` + closure | 96 | *inlined* |
| `ParseContext::with_parsing_state` | 368 | *inlined* |
| **`NodesParser::parse`** | **25 376** | 1 552 |
| `ParseContext::parse_group` | 224 | ~370 |
| `ParseContext::parse_scoped` + closure | 96 | *inlined* |
| `ParseContext::with_parsing_state` | 368 | *inlined* |
| **`GroupParser::parse`** | **5 136** | 1 840 |
| `ParseContext::with_frame` | 752 | *inlined* |
| `GroupParser::parse::{{closure}}` | 160 | *inlined* |
| **total** | **32 800** | 4 288 |

against 33.8–35.0 KB/level measured end-to-end (the binary search brackets depth 30/31),
so the table accounts for essentially all of it. **`NodesParser::parse` alone is 77 % of a
nesting level in debug.**

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

## 3. What is actually on the frames

Every object over ~100 B on the cycle is one of three, and they nest:

| object | size | of which |
|---|---:|---|
| `Option<ParsingStateDelta<L>>` | **208** | `TokenRulesOverrides<L>` — ~15 all-`Option` fields (`Option<Vec<Arc<GroupRule<L>>>>`, `Option<Vec<Arc<CommandRule>>>`, `Option<Arc<str>>`, `Option<Option<Arc<GroupRule<L>>>>`, …) plus two `Vec`s |
| `NodesOutcome<L>` | **264** | `Vec<BuildId>` 24 + `StopCause` 24 + `Arc<ParsingState>` 8 + **`after_effects: Option<ParsingStateDelta<L>>` 208** |
| `(Output, Option<ParsingStateDelta<L>>)` return pair | **472** | 264 + 208 |

Everything else on the path is below the threshold: `NodesParser<'_, L>` 352 (heap — it is
a `Box<dyn ConstructParser>`), `TokenRecovery` 96, `Token<'_, L>` 88, `Frame<L>` 80,
`NodeKind<L>` 72, `ParseError` 64, `StopSpec` / `ChildStateSpec` 48 each.

**Named locals (DWARF, debug build).** `ParseContext::parse_nodes` has 4 locals totalling
40 B and `parse_group` 6 totalling 64 B — nothing over 100 B; their frames are almost
entirely the sret slot for the 472 B return value. `GroupParser::parse` has 21 locals
(1 140 B): `outcome` 264, `delta` 208, `frame` 80, five 64 B `residual`s
(`Result<Infallible, ParseError>`, one per `?`), `data` 56, `stop` 48, `child_states` 48.
`NodesParser::parse` has 62 locals (2 504 B), topped by `_delta` 208, `recovery` 96, two
`token` 88, two `kind` 72, and eight 64 B `residual`s.

**Unnamed slots are where the mass is.** `NodesParser::parse`'s debug frame holds **178
distinct address-taken slots spanning 24 872 B**, and the gap histogram is unambiguous:

```
   472 B × 16     the (NodesOutcome, Option<Delta>) return value      7 552 B
   264 B × 18     NodesOutcome                                        4 752 B
   208 B × 15     Option<ParsingStateDelta>                           3 120 B
                                                          15 424 B = 61 % of the frame
```

They come from one expression repeated 17 times in the function body:

```rust
return Ok((self.outcome(&cx.state, StopCause::NodeCondition), None));
//         ◄── 264 B  NodesOutcome        (own slot, ×18)
//    ◄────── 472 B  the Ok tuple/Result  (own slot, ×16)
```

**Nearly every one of those slots carries `None`.** Seventeen sites spell `return
Ok((self.outcome(…), None))`; `NodesParser` binds the group descent's delta as `_delta`
and drops it (`// groups have no after-effect`); `GroupParser` returns an
`ImplementationError` if it is `Some`; and **every `Some(delta)` return site in the crate
is inside a `mod tests`** (`nodes_parser.rs` 2690/2803, `argument_parsers.rs` 2119,
`latexlike/input.rs` 798). No production parser has ever produced one.

Debug is ~8× worse than release only because `-C opt-level=0` gives each of those 49
occurrences its own slot instead of colouring them into a handful. The *object* is the
same in both profiles, which is why the one measured fix helped both (§0).

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
- **wasm32.** The default linear-memory stack is 1 MiB → **286 environment levels / 491
  group levels**, measured. Overflow **traps cleanly** (`RuntimeError: memory access out
  of bounds`, on every trial under Node 22) rather than writing past the shadow stack —
  so being near the edge is safe, and a budget check (§5 A) has a real fault to sit in
  front of rather than racing silent corruption. Worth re-confirming on another runtime
  before depending on it. Unlike a native thread, the wasm stack is a **link-time
  constant** (`-C link-arg=-zstack-size=N`) reserved outright in linear memory, so
  raising it is a permanent cost — which is exactly why the per-level constant (§5 B)
  matters most here.
- **`no_std` embedders**, who typically have far less than 1 MiB.
- **Untrusted input.** `{{{{…}}}}` ×2 000 is 4 KB of input and aborts an 8 MiB-stack
  process. There is no way for an embedder to defend against this today, because the
  library bounds the recursion in no way at all and the failure is not catchable.

Legitimate LaTeX does nest — `document` > `figure` > `center` > `tabular` > `{}` >
`\textbf{}` is depth 6 before any content — so a real document reaching depth 20–30 is
unremarkable. Depth 46 in a debug build is not a comfortable margin.

---

## 5. Options

The hard requirement is narrow: **never abort the process; always return an `Err`.**
Everything past that is policy.

**A. Bound the stack by measuring it. (Recommended — the mechanism.)**

Stash the stack anchor at parse entry; at each descent, compare against a local's address
and refuse when the consumed span exceeds the budget:

```rust
// parse entry, into the session
let anchor = &0u8 as *const u8 as usize;
// each descent (with_parsing_state — the one funnel every descent passes through:
// parse_scoped → with_parsing_state, and argument parsing calls it directly)
let here = &0u8 as *const u8 as usize;
if anchor.abs_diff(here) > budget {
    return Err(/* StackBudgetExhausted, through the recovery entry point */);
}
```

Pure `core` — no libc, no `std` — so it works on wasm and `no_std`, where a
`stacker`-style platform query cannot go. `abs_diff` keeps it growth-direction-agnostic.
The session is shared with nested `parse_attached_source` contexts, so `\input` recursion
is covered for free. Cost is an address-of, a subtract and a compare per descent — noise
against 4–8 KB of frame setup.

This technique is already validated at the needed precision: it is exactly the
`stackmark!` instrumentation of §1, which produced every per-level figure in this report
and agreed with the independent end-to-end measurements across a 500× scale change.

Two implementation questions, neither of them a reason to prefer something else:

1. **Headroom.** The check runs *after* the frame is pushed, so the budget must sit far
   enough below the real limit to absorb the deepest unchecked stretch — including
   whatever a third-party `ConstructParser` burns between two descents. The margin is a
   guess; a conservative one is nearly free when the stack is large.
2. **Who supplies the number.** `core` cannot portably discover a thread's stack size.
   Either the embedder passes it (natural — they chose the stack size), or a `std`-gated
   path derives it from the platform (`pthread_getattr_np`,
   `GetCurrentThreadStackLimits`, as `stacker::remaining_stack` does).

**A′. A nesting-depth limit — optional policy, not the mechanism.**

*Superseded position.* An earlier draft of this report recommended a depth limit as the
primary fix, "needed regardless of the rest", on the grounds that it gives a
build-independent accept/reject boundary. **That argument is wrong and should not be
re-litigated.** With both mechanisms present the real boundary is
`min(depth_limit, budget)`, so the limit delivers reproducibility only if it binds in
*every* build — including debug on a 2 MiB libtest thread, which tops out at **46 nested
environments** (§0). A reproducible limit would therefore have to be ≤46, which no LaTeX
parser can ship. Set it somewhere sane instead (say 256) and debug builds still fail at 46
via the budget: reproducibility is gone, and all that has been added is a ceiling *tighter
than the machine's* where everything was already fine, and *looser than the machine's*
where it was not. It rejects valid documents in production without protecting the case it
was introduced for.

The near-universal precedent (serde_json's `recursion_limit`, XML entity-depth caps,
CPython's `sys.setrecursionlimit`) counts levels because those implementations **could not
portably measure the stack** — not because depth is the right resource to bound; CPython
has been moving toward real C-stack checks for exactly this reason. That constraint does
not apply here, so neither does the precedent.

A depth limit remains defensible in one narrow role: when a *language definition* wants a
depth cap as a semantic property, so a conforming document gets the same answer from every
tool. That belongs to a preset or an embedder, not to the engine. If offered at all, it
should be **off by default**.

| | depth limit | stack budget |
|---|---|---|
| bounds | nesting levels | bytes actually consumed |
| reproducible across builds | only if ≤ the worst build's capacity (~46) | no |
| adapts to per-construct cost (group 4.3 KB, environment 8 KB) | no | **yes** |
| covers a third-party parser's own frames | no | **yes** |
| role | optional language policy | **the mechanism** |

**B. Get the big objects off the return path** (§6 works through the storage strategies).
Independent of A: the budget stops the abort, but a smaller per-level constant is what
makes deep documents actually *parse*. In payoff order:

1. **Take `Option<ParsingStateDelta<L>>` out of the construct-parser return pair** — not
   shrink it, remove it. It is `None` at every production site (§3), so the slots exist
   only to carry nothing. This changes a **decided** public signature
   ([§dd-dr:parsers-engine]) — the user's call.
2. **`NodesOutcome::after_effects` → `Option<Box<…>>`** — genuinely owned data a caller
   may propagate, so it cannot become a borrow. Measured alone: −22 % release, −25 % debug.
3. **Outline the cold arms** of `NodesParser::parse` and `GroupParser::parse` into
   `#[inline(never)]` helpers — the condition-construction and formatting bodies, not
   the recovery funnel (§3's negative result).

**Measured**, boxing both deltas (B.2 extended to the return pair, in lieu of B.1's
channel): x86-64 release −42 % per level (241 → 416 group levels), **wasm32 release −52 %
(491 → 1 022)**. See §6.5 for why wasm gains more. On a native target, raising the stack
still buys more than any of this (§5 C′) — 8 KB → 2 KB is 4×, while 8 MiB → 256 MiB is
32× — but where the stack is a link-time constant, B is the only lever there is.

**C. Grow the stack on demand (`stacker`-style).** Rejected as a library dependency:
needs `std` and per-architecture assembly, does not work on wasm or `no_std`, and adds a
dependency against [§dd-dr:dependencies].

**C′. Give the parse a bigger stack — the embedder's lever, and the most effective one.**
Not a library change at all, and it dominates everything the library can do:

```rust
std::thread::Builder::new().stack_size(256 << 20)
    .spawn(move || lang.parse(source))?.join().unwrap()?
```

`stack_size` *reserves address space*; pages commit lazily on first touch, so a 256 MiB
reservation costs a few pages for a document that nests 30 deep. Measured on today's
unmodified code: **2 000 nested environments on 16 MiB, 8 000 on 64 MiB, 30 000 on
256 MiB, 60 000 nested groups on 256 MiB** — all `ok`. On wasm the equivalent is
link-time, not runtime: `-C link-arg=-zstack-size=N` raises the 1 MiB default, but it is
linear memory reserved outright, not lazily committed.

This is the right first answer for an embedder parsing input they control. It does *not*
substitute for A: a bigger stack turns "abort at 300" into "abort at 30 000" without ever
producing a handleable error, which is why A is still the mechanism for untrusted input.

**D. Eliminate the recursion (explicit descent stack).** Rejected as impractical, not
merely expensive: the recursion *is* the extension contract (§2). Trampolining would
require every outer-layer `ConstructParser` to become a resumable state machine, which
trades the library's stated extensibility pillar for a bound that A already provides at
a fraction of the cost.

**E. Consumer-side traversals.** `walk` (96 B/level) and `recompose` (290 B/level) also
recurse without limits, and are public entry points that accept *any* `NodeTree` —
including one built through `NodeTreeBuilder` rather than produced by a bounded parse.
`validate_tree`, `Debug` and `Drop` are already iterative. `restage` was not measured; it
is structurally recursive like `recompose`. Low priority — the constants are 15–45×
smaller — but worth a note in the docs that a parse-side budget does not bound a
hand-built tree.

**Suggested order: A first** — it is the correctness fix, and the only item that turns an
uncatchable abort into an `Err`. **C′ is what an embedder should reach for today**, and
costs nothing to document. **B.1–B.2 measured incrementally** after that, for the
platforms where C′ is unavailable (wasm, `no_std`) and to widen the margin everywhere
else.

---

## 6. Storage strategies for the big objects

Four ways to get the 208/264/472 B family off the frames were worked through. Summary
first, argument after:

| strategy | slot on the frame | alloc per `Some` | verdict |
|---|---:|---|---|
| today | 208 B | — | — |
| `Option<Box<Delta>>` | 8 B | 1 malloc+free | **works**; measured on `after_effects` |
| session channel, `Option<&Delta>` | 0–8 B | 0 (reused buffer) | **best for the return pair** |
| session `Vec` pool + handles | 4–8 B | 0 (amortised) | equivalent to boxing; buys nothing here |
| session bump allocator, `Box<T, A>` | **16 B** | 0 (amortised) | rejected |

### 6.1 The session channel (recommended for the return pair)

Drop the delta from `(Output, Option<ParsingStateDelta<L>>)`; the producer reports it
through the session, the caller reads it back:

```rust
// producer (rare):   cx.report_after_effect(delta);
// caller (every level):
if let Some(d) = cx.pending_after_effect() {      // -> Option<&ParsingStateDelta<L>>, 8 B
    cx.state = cx.derive_state_recording(d, &mut self.after_effects)?;
}
```

The accessor **must hand out a reference**. An earlier sketch of this used
`take_after_effect() -> Option<ParsingStateDelta<L>>`, which gives the caller ownership
and re-materialises the 208 B on its frame — defeating the point entirely.

A reference suffices because every consumer in the crate is already reference-based:
`ParseContext::derive_state(&mut self, delta: &ParsingStateDelta<L>)` and
`derive_state_recording(&mut self, delta: &…, record: &mut Option<…>)`. The one owned copy
lands in `record` = `&mut self.after_effects`, a field of `NodesParser` — which the driver
factory hands out as `Box<dyn ConstructParser>`, so that accumulator is **already on the
heap**.

**Main takeaway:** the win is not relocating the caller's local. It is that a channel has
no `None` to construct and no `None` to receive, and the ~32 delta-sized slots per level
that carry `None` (§3) simply cease to exist. The rare genuine `Some` was never on a frame
to begin with.

Semantics are preserved: [§dd-dr:immutable-state-deltas]'s caller-decides-scope law still
holds, because the caller still decides — it reads a channel instead of destructuring a
tuple. The cost is that "did the caller consume it?" becomes a runtime invariant rather
than a destructuring, so it wants the closure-scoped-guard treatment the frame and
enclosing-state stacks already get, with an `ImplementationError` on imbalance (the
existing `delta.is_some()` check, relocated).

### 6.2 Session `Vec` pool with handles

Architecturally the most consistent option — `ParserSession` already hosts exactly this
shape in `frames: Vec<Frame<L>>` and `state_stack: ParsingStateStack<L>`, both private,
both closure-scoped push/pop with a documented balance invariant. And the general argument
is sound and worth recording: **a session-hosted `Vec` moves depth-proportional growth off
the guard-page-limited stack onto the heap**, where the bound is RAM rather than a
SIGSEGV. That reasoning applies to any object whose count scales with nesting depth.

For *this* object, though, it is equivalent to boxing on the stack axis, and its only extra
benefit — skipping the malloc — is worth nothing, because `Some` never occurs in
production code (§3). It would buy a handle-validity/LIFO invariant in exchange for
eliminating an allocation that does not happen.

### 6.3 Session bump allocator with `Box<T, A>` — rejected

The API is available: `allocator_api` is still unstable (verified on rustc 1.94.1, issue
#32838), but **`allocator-api2` v0.2.21 is already in techy's runtime dependency graph**
via `hashbrown 0.15.5` and ships a stable `Box<T, A>` / `Vec<T, A>`. So the mechanism
would cost no new crate for the API itself. It still fails, for four independent reasons:

- **`Box<T, A>` stores the allocator handle inline**, so it grows 8 B (`Global`, a ZST) to
  16 B (`&'s Bump`) — working against the stack goal. `Arc<Bump>` keeps 8 B at the cost of
  refcount traffic.
- **The lifetime infects the public API**: `Box<Delta, &'s Bump>` ⇒ `NodesOutcome<'s, L>`
  ⇒ `ConstructParser` gains `'s` ⇒ every extension-point signature, which is exactly what
  [§dd-dr:one-generic-param] guards against.
- **"Repackage surviving boxes into global-allocator boxes when the root parser returns"
  is the tell.** On this path it is a **no-op**: nothing survives — deltas are applied via
  `derive_state` and dropped mid-parse, and `NodesOutcome::nodes` is flattened into the
  tree's own storage by `NodeTreeBuilder::finish()`. Extending the arena to where the
  allocation volume actually is — `NodeKind::Group(Box<GroupData<L>>)` and
  `Callable(Box<CallableData<L>>)`, one box per node — makes it an **O(nodes) deep copy
  with both representations live**, on top of the ~1.9× parse-time peak from
  `TechyParsingMemoryFootprint.md`. There is no middle case where it pays.
- **Destructors.** A bump arena's appeal is dropping the region wholesale, but
  `ParsingStateDelta` owns global allocations and refcounts (§3's field list). Resetting
  without running each box's `Drop` leaks every one of those `Vec`s and `Arc`s — and a leak
  is a failure mode [§dd-dr:panic-policy] cannot surface as an `Err`. Tracking them to
  drop them individually is a pool with extra steps.

The one place the instinct would be right is a different library shape: parse into one
contiguous arena and return a self-contained blob with no global-allocator interior. That
is a ground-up decision about `NodeTree`'s representation — and it would trade away the
`Arc<ParsingState>` sharing that nodes rely on — not a retrofit.

### 6.4 The `Vec<BuildId>` arena — a real win, but on the *allocation* axis

Separate from the stack question. Per `{…}` level the parse allocates:

| allocation | per level |
|---|---:|
| `Box<dyn ConstructParser<…> + 'p>` from `make_nodes_parser` / `make_group_parser` | 2 |
| `Vec<BuildId>` per node with children (moved into `Staged`, retained until `finish()`) | 1 |
| the pass-through delta | 0 |

The `Vec` wants a session-hosted arena — but **a naive append-only arena is wrong**,
because a parent's children are not contiguous: in `{a{b}c}` the outer loop pushes `a`,
then the nested descent stages `{b}` and appends *its* children, then the outer pushes `c`.
Two regions are needed:

- `session.child_scratch: Vec<BuildId>` — LIFO scratch. Each descent records a watermark;
  a nested descent always drains back to *its* watermark before the parent pushes again, so
  each descent's children genuinely are contiguous here.
- `builder.child_arena: Vec<BuildId>` — append-only. At stage time, copy
  `child_scratch[watermark..]` in, store a `Range<u32>` in `Staged`, truncate the scratch.

Two plain `Vec`s — no allocator API, no lifetimes in public types. It removes the
per-descent malloc *and* the per-staged-node `Staged::children` malloc (the one
`TechyParsingMemoryFootprint.md` §0.4 flagged), takes `NodesOutcome::nodes` and
`Staged::children` from 24 B to 8 B, and converges on the representation `finish()`
already computes — it currently *derives* `ranges: Vec<Range<u32>>` from the per-node
vecs, so staging in ranges removes a conversion rather than adding one.

Costs: the watermark balance becomes a runtime invariant (closure-scoped guard, `Err` on
imbalance — not a panic); and `stage_node(kind, span, state, children: Vec<BuildId>)` is a
*public* extension affordance today, so extension parsers would move to a scratch handle.
That API change is the bulk of the work, not the arena. It barely moves the stack needle
on its own (24 → 8 B against a ~33 KB level) — these stay two independent changes.

### 6.5 Why boxing pays off more on wasm32

Measured sizes on both targets (wasm32 via the `size_of` const-assert trick, §1):

| type | wasm32 | x86-64 |
|---|---:|---:|
| `TokenRulesOverrides<L>` | 80 | — |
| `ParsingStateDelta<L>` | 108 | 208 |
| `Option<ParsingStateDelta<L>>` | 108 | 208 *(niche-optimised — same size as the payload on both)* |
| `NodesOutcome<L>` | 136 | 264 |
| — of which `after_effects` | **108 (79 %)** | 208 (79 %) |
| `(NodesOutcome, Option<Delta>)` | 244 | 472 |
| `Result<(…), ParseError>` | 244 | 472 *(the `Err` arm fits inside)* |
| `Option<Box<ParsingStateDelta<L>>>` | **4** | 8 |
| `(NodesOutcome, Option<Box<Delta>>)` | 140 | — |

Two effects compound. The aggregates are roughly half-size on wasm32 (4-byte pointers),
but the *proportion* the delta occupies is identical — 79 % of `NodesOutcome` on both. And
the wasm shadow stack holds only **address-taken** locals, where x86-64 frames also carry
spilled scalars and register-pressure temporaries. So the same objects make up a larger
share of what a wasm frame actually costs, and removing them helps more: −52 % per level
on wasm32 against −42 % on x86-64, for the identical change.

Boxing both takes `NodesOutcome` 136 → 32 and the return pair 244 → ~36 on wasm32.

**Takeaway for a small wasm build**: boxing is not merely the *simplest* option, it is the
best-value one — a mechanical diff with no semantic change, doubling available depth at no
fixed-stack cost. The §6.1 channel remains better in principle (it removes the `None`s
rather than shrinking them) but it is an extension-API change against a decided signature,
so it is a later refinement, not a prerequisite.

One caveat to record with the change: boxing trades a rare heap allocation for the stack
saving, and today that allocation *never happens* (§3 — no production parser returns
`Some`). A future preset that produces pass-through deltas on a hot path would change that
arithmetic.

---

## 7. Open questions

- **Where does the budget value come from?** Embedder-supplied (they chose the stack
  size), a conservative built-in default, or a `std`-gated platform query? A default that
  is wrong in either direction is worse than requiring the number.
- **How much headroom** below the real limit, given the check runs after the frame is
  pushed and a third-party `ConstructParser` can burn an unbounded amount between two
  descents?
- **Does the budget check belong on by default?** It costs an address-of and a compare per
  descent, but it turns a hard abort into an `Err` — the argument for opt-out rather than
  opt-in.
- Should a depth limit be offered *at all* alongside it, as off-by-default language policy
  (§5 A′), or left entirely to presets and embedders?
- Is `B.1` — removing the pass-through delta from the return pair in favour of the §6.1
  session channel — acceptable against the decided [§dd-dr:parsers-engine] signature? And
  is the runtime consume-obligation an acceptable trade for the compile-time destructuring?
- wasm32 overflow was measured to trap cleanly under Node 22 (§4); confirm on the
  runtimes you actually ship against before the budget check depends on it.
- Is the §6.4 `Vec<BuildId>` scratch+arena worth its extension-API change on allocation
  grounds alone, given it barely affects stack depth?

Once a direction is chosen, this warrants a `DESIGN_RATIONALE.md` entry with an
`ARCHITECTURE.md` reference (CLAUDE.md rule 7); as an exploration report it follows the
`TechyParsingMemoryFootprint.md` precedent and adds none yet.

**2026-08-10 — direction chosen and implemented; the durable record is
[§dd-dr:descent-guard]** (which resolves the questions above). Shipped: the
`parse_construct` descent funnel, the `DescentGuard` on driver/`Language`/session
(measured stack budget as the mechanism, depth limit as deterministic policy), and
the boxed pass-through deltas.

---

## 8. Reproducing

```bash
cargo run --release --example stack_probe -- 1024   # per-level cost at a 1 MiB stack
cargo run           --example stack_probe -- 1024   # the debug figures
```

The probe covers both axes: `PARSE` binary-searches input nesting depth per construct,
`CONSUME` parses a deep tree on a 512 MiB stack and then runs `walk` / `recompose` /
`validate_tree` / `Debug` / `drop` on the constrained stack.

The wasm32 figures need an out-of-tree `cdylib` (≈40 lines: a `try_depth(kind, depth)`
export plus a Node driver that instantiates fresh per trial), built with

```bash
RUSTFLAGS="-C link-arg=-zstack-size=1048576" \
  cargo build --release --target wasm32-unknown-unknown
```
