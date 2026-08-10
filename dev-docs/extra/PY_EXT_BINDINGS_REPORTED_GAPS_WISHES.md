# Feedback note to techy — gaps and wishes

A review of the techy crate written from the outside, by building its Python
bindings end to end. Binding a library is an unusually thorough review of it:
every contract gets exercised, every doc page gets read under pressure, and
every awkward seam shows up as awkward binding code.

**This file is feedback, never an edit.** Nothing in `../../techy` is ever
modified from this repository.

---

## Summary

#### Provenance

`techy-py` binds **all 280 public items** of techy's inventory — 241 with a
direct Python counterpart and 39 recorded as deliberate reductions — across
eight milestones (M0–M7) between the crate's `source`/`error` foundations and
its custom-language seam, ending with an author-side surface where a Python
object can be a `CallableSpec`, a `SpecsProvider`, a `ConstructParser`, an
`EnvironmentBehavior`, a `Recomposer`, a `RestageVisitor` or a `Lang` hook
table. The gate at the end of M7 was 1727 Python tests and 120 Rust tests
green, including a port of techy's own `recompose_oracle.rs` and an executable
oracle over every code block, table row and pitfall bullet in both AI guides.
Roughly a dozen agents wrote the entries below as they hit them, one per issue,
in parallel workstreams; **this M8 pass merged the duplicates, verified the
load-bearing claims against techy's current source, and sorted the result by
severity.** Where a claim turned out to be stale or wrong it is struck and
corrected in place rather than deleted, so a citation still lands somewhere.

**78 entries**: 73 gaps, 4 positive findings, and 1 that techy has since fixed.

#### Severity, counted

| Severity | Count | Means |
|---|---|---|
| **blocking** | 4 | Made a binding impossible, killed the host process, or left `unsafe` over live borrows as the only route. No safe workaround exists outside techy. |
| **major** | 13 | Forced a documented deviation from the binding's own mapping doctrine, or a permanent compromise visible in the Python API. |
| moderate | 31 | Cost real work — a re-implementation, a mirrored code path, a parallel data structure, an hour of source-diving — or produced a worse Python surface that is still honest. |
| minor | 25 | Polish: a naming asymmetry, a missing derive, a one-method gap, a doc sentence. |
| none | 5 | Four positive findings, and one gap techy has closed. |

Every entry carries a severity with a justification clause, so a rating can be
argued with rather than taken on trust. The ratings are deliberately
bottom-heavy: 56 of 73 gaps are moderate or minor, because a note where
everything is urgent is a note nobody can act on.

#### The seven findings worth reading first

| # | Finding | Severity |
|---|---|---|
| 1 | **`{{{{…}}}}` is a `SIGSEGV`.** No nesting-depth budget, no depth accessor, no descent veto. Measured: 234 nested groups parse, 235 kills the process. Not a panic — an unrecoverable stack overflow that no `catch_unwind` can contain. → *techy has no nesting-depth policy anywhere* | blocking |
| 2 | **Any document starting with a non-ASCII character panics `display_tree`.** A four-line resume-point bug in `LineIndex::extend_line_starts_up_to`; `"é"`, `"—x{y}"`, `"😀"` all crash the pretty-printer everyone reaches for first. Re-checked at M8: still live. → *`display_tree` panics on any document whose first character is multi-byte* | blocking |
| 3 | **Twenty extension hooks answer a bare value with no way to report a failure**, across six seams and four milestones. Fourteen others have a `Result`. The binding invented five different swallow-and-report policies, and both of M7's blocking review defects landed in that machinery. → *An extension hook that fails has nowhere to say so* | major |
| 4 | **One supertrait, four times.** Of the five extension traits techy stores behind `Arc<dyn _>`, only `CallableSpec` has `Any`, so the other four are one-way doors — and one of them costs measurable throughput on every `\input`-driven document. → *Of the five extension traits held as `dyn`, only `CallableSpec` has an `Any` supertrait* | major |
| 5 | **`Language::new` will not take the `Arc` an embedder is forced to hold**, and there is no `ParsingState::data()` and no `from_data()`. The only workaround forks the state identity — defeating the very invariant that non-`Clone` exists to protect. → *`ParsingState` is identity-bearing by design and an embedder can only hold an `Arc`* | major |
| 6 | **`NodeTree::slice(range)` — one method, asked for identically in M2 and M5**, costing ~60 lines on the read surface, ~120 plus a second execution path in `extract`, and a lost O(1) fast path. → *There is no public `NodeTree` → `NodeSlice` constructor from a range* | major |
| 7 | **The reemit oracle cannot falsify the doctrine it exists to prove.** A deliberate span-shortcut recomposer — the exact implementation the contract forbids — passes **38 of 38** equality inputs. One test in the file catches it, incidentally. About fifteen lines would fix it. → *The reemit oracle cannot falsify the doctrine it exists to prove* | moderate |

#### Themes — where 78 symptoms collapse into a handful of fixes

These are the most useful thing in this note. Each is one decision made once and
felt many times, so each is one change that closes several entries.

**1. `pub(crate)` on an accessor whose type, siblings and docs are all public.**
The single largest root cause. **20 entries** name such a keyword, and in
**12 of them the wish is, in whole or in part, nothing but flipping it** — "make
this `pub`", or export it behind a `test-support` feature. The items, each
checked against techy's source at M8:

`NodeSlice::new` (`node/slice.rs:45`), `NodeTree::is_single_source` (`:186`),
`NodeId::new` (`node/tree.rs:71`), `NodeTree::make_id` (`:233`),
`NodeTree::tree_tag` (`:228`), `NodeTree::annotations` the field (`:164`),
`NodeRef::source` (`node/node_ref.rs:143`), `copy_subtree_into`
(`node/mod.rs:65`), `ChildRegion::staged` (`node/arguments.rs:193`),
`ParsingStateDelta::merge_from` / `is_empty` (`state/delta.rs:267`, `:249`),
`ParsingStateStack::push` / `pop` / `innermost` (`state/stack.rs:115`, `:120`,
`:125`), `ConcatPieces::into_parts` (`recompose/mod.rs:293`),
`Diagnostic::from_parts` (`error.rs:393`), `ParserSession::state_stack`
(`engine/mod.rs:225`), `recompose::context::drive` (`pub(super)`,
`recompose/context.rs:37`), and — behind `#[cfg(test)]` as well —
`check_tree_invariants` (`node/invariants.rs:534`) and `ParsingState::new`
(`state/parsing_state.rs:139`).

In almost every case the *type* is public, the *inverse* accessor is public, and
the in-crate caller is the only reason for the keyword. These are the cheapest
fixes in the note by a wide margin: no design decision, no new concept, no API
to name.

**2. A borrow where a garbage-collected host can only hold an owner.** techy's
own doctrine — states are identity-bearing, trees are frozen, views are
transient — is right, and it is implemented with borrows that an FFI boundary
cannot cross. `&ParsingState<L>` in four hooks, `ParsingState` by value in
`Language::new`, `&Source` from `including_sources`, `&[A]` for annotations,
`SymbolEntry<'a>` borrowed from the provider, `&dyn EnvironmentBehavior`,
`&'p str` in the body parsers. **The tell is that techy already agrees with the
wish in the places it happens to be convenient**: `ParseDriver::probe_token`,
`Lang::make_node_ext` and both `ChildState::Compute` arms take
`&Arc<ParsingState<L>>` and cross whole, identity included. The four hooks that
take the bare borrow do not. Nothing about the design needs to change — only
which of two shapes techy already uses gets used consistently.

**3. Lifetimes on types an embedding must hold, and the `unsafe` they force.**
**All seven `unsafe` blocks in this binding are on this seam**, and nowhere
else: `RestageContext<'t>` and `RecomposeContext<'t>` (phantom lifetimes over
nothing reachable — two blocks a `Default` impl would delete),
`ParseContext<'a, 's>` (three live borrows — the one whose SAFETY comment is
genuinely hard), `StagedChildren<'b>` (a borrow of an arena about to
reallocate), plus the owned mirrors that `LineIndex<'c>`, `NodeSlice<'_>` and
`SymbolEntry<'a>` forced instead. A `#[pyclass]` cannot carry a lifetime, and
neither can a JS handle, a C struct or a Lua userdatum; this is the tax on every
non-Rust embedding, not just this one.

**4. No error channel on extension hooks — twenty of them.** Filed six times
across four milestones before anyone counted. Treated as one gap it is the
note's largest single request; see finding 3.

**5. Build-and-consume surfaces with no read direction.** `ConcatPieces` (six
setters, zero getters), a staged `ChildRegion` (three accessors that panic),
`StagedNodes` (countable, not iterable), `StagedChildren` (no ids),
`MapResolver` (no `get`, no `len`), `Lang::SessionExt` (dropped by
`ParserSession::finish`), `NodeKind` in flight (no owned projection). Each is
right inside the crate, where a value is produced and immediately consumed by
code that already knows what it is. Each is fatal to a binding, which has to
*materialize* the value as an object before anything can consume it. **The
question worth asking of any new type is "can a caller read back what it just
built?"** — five entries answer no.

**6. techy's own test fixtures are exactly what an embedder needs, and they are
`#[cfg(test)]`.** `check_tree_invariants` and
`check_latexlike_tree_invariants` (the crate's central promise about parsed
trees); the hand-tree builder that is the only way to produce a `TreeViolation`,
whose validator is documented as being *for FFI embedders*; the private
`AfterEffectSpec` that is the only producer of a sibling after-effect, without
which `input_macro_spec`'s `persist_state` parameter cannot be exercised at all.
A `test-support` cargo feature would close all three.

#### The cheapest wins

If only a handful of changes are made, these buy the most per line:

* `pub` on `NodeSlice::new` (or `NodeTree::slice(range) -> Option<..>`) — closes
  a **major** entry that two milestones filed identically.
* `pub` on `ConcatPieces::into_parts` — one keyword; closes a **major** entry
  and deletes a ~90-line mirror plus the corpus test holding it in step.
* `impl Default for RecomposeContext` — one line; deletes an `unsafe` block.
* `Any` on `SpecsProvider`, `ArgumentParser`, `EnvironmentBehavior`,
  `SourceResolver` — one word, four times; closes a **major** entry and five
  distinct symptoms.
* `ParsingState::data()` and `Language::new(impl Into<Arc<ParsingState<L>>>)` —
  two lines, no call site changes; closes a **major** entry.
* Deleting six words from `format_position_with`'s no-line-info message — stops
  every consumer-supplied `LineColProvider` from rendering a factual untruth.
* One clause on `Package::get`, one on `Language::parse`, one on
  `SourceRecomposer`, one on `make_node_ext` — four doc sentences, four entries.

#### What was verified, and what was wrong

Claims here were written across eight milestones, so at M8 the load-bearing ones
were re-checked against techy's current source — every blocking and major entry,
plus every entry whose claim was strong enough that a maintainer might act on it
without checking: **about fifty discrete claims**. Nine needed correction, and
each correction is in the entry it belongs to:

* **`techy exports no version constant` is fixed** — `lib.rs:238` now has
  `pub const VERSION`. Struck.
* **`StrayGroupClose` (and its siblings) *can* be constructed outside techy.**
  The entry's premise was false: `#[derive(DiagnosticInfo)]` emits a public
  `new()` unless `no_constructor` is given, and all five of those structs have
  one. Struck, with the conclusion it supported withdrawn. (There are also five,
  not six.)
* **A public `NodeTree` → `NodeSlice` constructor does exist** —
  `covering_slice(&span)`. The gap is the *range*-taking one; title and wish
  restated.
* `CallableSpec` is not the only `Any`-bearing trait in the crate
  (`DiagnosticInfo` has it too); the true claim is narrower.
* The "`ArgumentParser` has no `Any` supertrait" text is an implementation
  comment, not rustdoc.
* `lang_initial` is not the only public data→state freeze.
* `make_invocation_parser` builds no parser — it delegates.
* `make_node_ext`'s arena borrow **cannot** escape a safe Rust implementation;
  it is a documentation gap, not a soundness one.
* Three condition names, not two, appear both shipped and as test fixtures.

Everything else checked out, including the two that most deserved doubt: the
`display_tree` panic is still live at `source/line_index.rs:105-132`, and there
is still no depth budget, no `ParseConfig`, no depth accessor and no descent
veto anywhere in the crate.

---

## How to read this note

**Entries cite each other by title, never by number.** Titles are stable and
survive a re-sort; ordinals do not. They did not: this file's ordinal citations
were correct against the *rendered* heading count and one low against a raw
`grep '^### '`, because the entry-format template below sits inside a code fence
and counts as a heading to a grep but not to a reader. Two ways of counting, two
answers, and no way to tell which an entry meant. The note is now free of both.

`### ` at the start of a line is an **entry heading and nothing else** — the
summary's own sub-headings are `####`. So `grep -c '^### '` answers **82**: the
78 counted entries, the one template line in the fenced block below, and the
**3** Addendum entries at the foot of the file, which are deliberately outside
the counts and the histogram. *(The figure read 79 until the Addendum landed;
corrected by the M8 review, which counted it.)*

**Entry order is by severity, then by rough impact within a severity.** The
sections are `Blocking`, `Major`, `Moderate`, `Minor`, and last a short section
of positive findings and one closed gap.

**A struck entry is kept, not deleted.** Where techy has fixed something, or a
later milestone found an entry's premise wrong, the entry stays with a
strike-through heading and a note saying what changed — so a citation from
techy-py's own records lands somewhere and the correction is visible next to
what it corrects.

#### What belongs here

| Kind | Trigger |
|---|---|
| **doc gap** | You could not answer a question from techy's docs and had to read its source, or you read the docs and got it *wrong* the first time |
| **doc friction** | The answer was in the docs but in a place you would not naturally look, or split across pages, or the example did not cover the real case |
| **design friction** | The binding needed materially more code, an unsafe block, a cache, or a workaround, that a different techy design would have made unnecessary |
| **missing feature** | Something a consumer plainly needs that techy does not offer, and the binding had to synthesize |
| **API asymmetry** | Two things that ought to mirror each other do not (naming, `Result` vs `Option`, presence of a `_named` twin, a missing `_with` variant, …) |
| **contract risk** | A rule that is silent when violated — the kind of thing techy's own "Traps" tables collect |

Do **not** add: Rust-specific ergonomics that have no bearing on consumers, or
anything already recorded in techy's own `TODO_Big.md` / open-questions register
(check before writing — the AI guide's pitfalls index and the "Traps" tables in
`docs/ai-guide-definitions.md` already name many known sharp edges; a *known*
trap is only worth an entry if binding it revealed something new).

#### Entry format

```
### <short title>

- **Kind**: doc gap | doc friction | design friction | missing feature | API asymmetry | contract risk
- **Where**: the techy item or doc page (canonical path, e.g. `techy::core::node::NodeRef::body`)
- **Found while**: M<n>, building <what>
- **What happened**: what you expected, what you found, how long it cost
- **Wish**: the concrete change that would have helped — a doc sentence, a method,
  a renamed parameter, a different default
- **Workaround taken**: what the binding does instead (link the file/line)
- **Severity**: blocking | major | moderate | minor — <why, from this entry's own evidence>
```

Keep entries short and concrete. One entry per issue; if the same issue bites
twice, add a "hit again" line and name the second surface, rather than opening a
second entry — the instances are the evidence, and they are worth more gathered
than scattered.

---

## Blocking

Made a binding impossible, killed the host process, or left `unsafe` over
live borrows as the only route. No safe workaround exists outside techy.

### techy has no nesting-depth policy anywhere, so a deep document is a SIGSEGV — no budget, no accessor, no veto that costs less than owning the driver

- **Kind**: **bug** (crash on valid input) / missing feature
- **Where**: `techy::core::Language::parse` — the construct-parser recursion
  (`techy::core::constructs`), reached for any nesting construct: groups, math,
  environment bodies, argument groups. Also
  `techy::core::ParseDriver::make_nodes_parser` / `make_group_parser` /
  `make_invocation_parser` (`engine/driver.rs:379`, `:400`, `:423`),
  `techy::core::constructs::ParseContext` (`constructs/mod.rs:118`),
  `techy::core::ParserSession` (private `frames`), and
  `techy::transform::context::drive`
- **Found while**: M5 (hardening the consumer libraries' own recursion guards,
  and binding `techy::transform`), M6 (giving `techy.parse` a `max_depth=`, and
  answering "can an embedder refuse a pathologically nested document from
  inside a construct parser?")
- **What happened**: techy recurses once per tree level with **no bound**, and
  the crash floor is low. Bisected in subprocesses from the Python bindings on
  an 8 MiB main-thread stack, macOS/arm64, debug build:

  | document | result |
  |---|---|
  | `"{" * 234 + "}" * 234` | parses |
  | `"{" * 235 + "}" * 235` | **SIGSEGV** (exit 139) |

  Re-measured independently, same machine: 230 and 234 exit 0; 240 and 250 exit
  139. **Publish it as a bracket, not a constant** — "measured 234/235 on this
  machine (darwin, default stack, debug build)" — because it moves with the
  build profile, the platform's default stack size, and every construct on the
  recursion path. It is written that way deliberately: three different point
  estimates (~350, 300, 200) coexisted in this project's own notes until an M5
  review pass compared them, which is what a stack ceiling does to anyone who
  measures it once and quotes it later.

  Two hundred-odd nested groups is not an adversarial input — it is a generated
  document, a machine-translated one, or a file with a runaway brace. There is
  no diagnostic, no `Err`, no unwind: the process dies. techy's own doctrine is
  *panics-never on document input*, and this is worse than a panic, because a
  panic can at least be contained at an FFI boundary (`catch_unwind`) while a
  stack overflow cannot be contained at all. `transform::restage`'s `drive` has
  the same per-level shape, and so does `recompose`; both are reachable with a
  tree a consumer built.

  **The gap is narrower and sharper than "techy does no recursion checking",**
  and it is worth stating precisely, because for `\input` chains techy's
  omission is a *considered position*, not an oversight:
  `constructs/attached_source.rs:17-22` says "Recursion control is deliberately
  **not** here … An embedder that wants a cycle or depth bound enforces it in
  its resolver", `source/resolver.rs:36-40` says the same,
  `check_include_chain(…, max_depth)` is the sanctioned hook, and there is a
  test called `core_performs_no_recursion_checking_self_inclusion_is_legal`.
  That is a defensible design: the core never interprets reference strings, and
  self-inclusion is legal.

  The gap is that **the same reasoning is silently extended to plain syntactic
  nesting, where none of it holds.** `{{{{…}}}}` needs no reference
  interpretation and no policy judgement; there is no resolver in the picture to
  put the bound in; there is no embedder hook at all short of owning the driver;
  and the failure is not a diagnosable refusal but a `SIGSEGV`. Re-checked at
  M8: there is still no `max_depth` parse option, no `ParseConfig`, no depth
  accessor and no descent veto. The only `max_depth` in the crate is
  `check_include_chain`'s, and the only `depth()` is `VisitContext::depth()` —
  a post-parse read on a finished tree.

  **A binding *can* bound it, and the shape it is forced into is the second
  half of this gap.** An earlier filing of this entry said the crash was "not
  something a binding can fix"; that was wrong, and *why* it was wrong is the
  request. The three `ParseDriver` construct factories are on every descent path
  (`parse_nodes` and `parse_group` route through them "uniformly",
  `constructs/mod.rs:657`/`:679`, and `make_invocation_parser` is documented as
  the driver's "uniform veto/wrap point"), and in techy's non-test code the two
  that build parsers directly — `NodesParser::new` and `GroupParser::new` — are
  called **nowhere else**: every other `::new` call site is inside a
  `#[cfg(test)]` module. So a driver that wraps each returned
  `Box<dyn ConstructParser<..>>` in a counting `Drop` guard, and answers a
  parser that returns `Err` once the budget is spent, gets an exact bound.
  Measured across a 22-shape corpus (groups, brackets, minted
  `r()`/`AnyDelimited` delimiters, math, environments, macro and specials
  invocations, embellishment chains, mixed nests, and a self-referential
  `\input` loop): every shape that used to crash now raises, and the four flat
  shapes that only *look* nested are still admitted.

  The friction is in what that costs, and who can pay it:

  1. **It requires implementing `Lang::Driver` in Rust.** An embedder using
     `LatexlikeDriver` as shipped cannot do this at all — which is every
     embedder except one that has already taken over the driver for other
     reasons. The Python binding could only do it because `PyDriver` is its own
     type forwarding all fourteen hooks (`PROGRESS.md` D32).
  2. **The count is in descents, not stack.** A brace group is 2 descents, a
     macro-with-argument 3, a specials-with-delimited-argument 4, and the
     stack each buys differs — the environment path constructs its
     `EnvironmentBodyParser` directly rather than through a factory, so it is
     the shape that gets the least counting for the most stack. Measured on an
     8 MiB main-thread stack, the crash floor spans 355–544 descents across the
     corpus. That 1.5× spread is livable (the same corpus spans 4.4× in *source
     levels*), but it means the default is an empirical constant an embedder
     must re-measure per platform, not a property techy states.
  3. **The refusal has no condition of its own.** The only public route to an
     abort that survives `Recovery::Tolerant` is
     `ParseContext::implementation_error`, whose `ImplementationError` says
     "an extension violated a library contract" — which is not what happened.
     The binding therefore raises the real exception itself and discards
     techy's error, so techy's diagnostic channel reports nothing.
  4. **It costs a forwarding frame per descent**, on the recursion path, which
     is the one place where an extra frame is least welcome. (Measured: it
     moved the crash floor by 0–1 source level out of 118–514.)

  **And there is nothing to read.** `ParseContext`'s own doc says it bundles the
  five parse inputs "giving the API one place to grow (**depth limits**,
  cancellation)" — the intended home is already designated, and it is empty.
  From outside the crate the only proxy is `session.snapshot_frames().len()`,
  which (a) counts *construct frames*, not recursion — a construct that pushes
  no frame is invisible — and (b) **renders every frame title** and clones every
  span to produce a number, because `frames` is private (its doc: "the push/pop
  balance is an invariant"), `state_stack()` is `pub(crate)`, and
  `snapshot_frames` is the only public reader. The binding exposes it as
  `cx.frame_depth()`, named for what it measures rather than for what a caller
  wants.

  A **pre-scan cannot be made sound at all**, so "validate the input first" is
  not an answer: five constructs recurse with nothing a scanner can see —
  expression-position invocation chains, delimiters minted from argument-code
  data at parse time, runtime-declared group rules, user-defined specials (the
  *lowest* crash floor in the measured corpus and the shape a scanner can least
  see), and `\input` chains, where the document that recurses for ever is nine
  characters long. Running every parse on a huge-stack thread is worse than it
  sounds too: it moves consumer callbacks off the calling thread, breaking
  techy's own documented "visitors run synchronously on the calling thread".
- **Wish**: a **`max_nesting_depth`** policy, with a documented default and a
  diagnostic when it is exceeded. The natural homes, in order of preference:
  1. **A depth budget on `ParseContext`** — the type's doc comment
     (`constructs/mod.rs:115-117`) and `ParseContext::new` (`:141-145`) both
     already name depth limits as its reason to exist. Checked wherever the
     parser descends, configured off the driver or a parse config, it would let
     every embedder — not only one that owns `Lang::Driver` — set a bound, and
     let it be denominated in whatever techy knows recursion costs rather than
     in a proxy the embedder has to calibrate. It is also where techy's own
     doctrine puts customization: in data ([§dd-dr:data-vs-traits]), which
     argues for `ParsingState` / `TokenRules` as the carrier.
  2. **A `ParseDriver::enter_descent(&self, depth) -> Result<(), ParseError>` /
     `exit_descent` pair**, called from `parse_nodes` / `parse_group` /
     `parse_attached_source`. This is what the factory interception
     approximates, minus the `Drop`-guard bookkeeping, minus the forwarding
     frame, and with a depth techy computes.
  3. **At minimum, a cheap `ParserSession::frame_depth() -> usize`** (free — it
     is the half of `snapshot_frames` that does not allocate) and a public
     condition for "refused to descend further", so an embedder that does own
     the driver can count and diagnose without allocating and without borrowing
     `ImplementationError`'s meaning.

  For `transform`, the same request in its own terms: a
  `RestageError::TooDeep { depth }`, or an explicit stack of pending levels
  instead of native recursion. Failing all of it, **one line in the crate-level
  *Panics* list** saying that the parser and both drivers recurse per tree level
  and that the caller owns the stack budget — so an embedder knows it must
  impose a limit of its own.

  The condition should be an ordinary tolerant-recoverable one where that is
  safe (stop descending, emit the diagnostic, keep the tree), which is the shape
  techy already has for every other malformed-input condition — but note that
  the binding deliberately makes it an **abort** under both policies, because a
  recovery that continues from a depth refusal continues towards the crash.
- **Workaround taken**: `techy-py`'s `src/depth.rs` — the factory interception
  described above, wired into `src/lang.rs`'s three `make_*_parser` forwards,
  with the budget installed per parse by `src/engine.rs`'s `PyLanguage::run`
  and surfaced as `techy.parse(max_depth=…)` / `Language(max_depth=…)`.
  Default 128 descents, measured at a factor of 2.77 below the smallest crash
  floor on this platform (`PROGRESS.md` D201, with the full table).
  On the consumer side, `restage` counts **tree levels** and raises
  `RecursionError` above 98; `recompose` and `walk` count **nested ops**, so a
  re-entrant callback is bounded and *tree depth is not* — a 3000-level tree
  walks and folds with no guard fired. That is safe today only because every
  tree a consumer can be handed comes from a bounded producer
  (`PROGRESS.md` §D200), and the producer this entry is about is the one whose
  bound techy does not have.
- **Severity**: **blocking** — an unauthenticated crash on valid input, in the
  one entry point every consumer calls, that no `catch_unwind` can contain. The
  binding closes it only by owning a Rust driver, which most embedders cannot
  do, and only by borrowing another condition's meaning to report the refusal.

> **Merged M8** from four entries: the M5 `transform`-side filing, the M5
> canonical crash entry, M6's "bounded from outside techy" (the *veto* side,
> filed after building the workaround), and M6's "`ParseContext` names depth
> limits and has no way to observe depth" (the *read* side). They were filed by
> four workstreams across two milestones and cross-referenced rather than
> merged, because entries were cited by ordinal; they are one missing policy.
> **Corrected M8**: an earlier filing said `NodesParser`/`GroupParser` are built
> by "the three factories" — `make_invocation_parser` builds neither, it
> delegates to `CallableSpec::make_invocation_parser`. All three are still
> interception points.


### `display_tree` panics on any document whose first character is multi-byte

- **Kind**: **bug** (panic on valid input)
- **Where**: `techy::source::line_index::LineIndex::extend_line_starts_up_to`
  (`src/source/line_index.rs:121`), reached from
  `techy::node::display::display_tree` → `Renderer::line` → `LineIndex::line_col`
- **Found while**: M4, validating that the Python-rebuilt test definition set
  produces the same trees as the deleted Rust scaffolding — the comparison
  rendered every tree with `NodeTree::display()`
- **What happened**: `line_col(off)` records `computed_end = off + 1`, and the
  *next* call slices `self.content[computed_end..end_at]`. `off + 1` is not a
  char boundary whenever the character at `off` is multi-byte, so the second
  call panics:

  ```
  start byte index 1 is not a char boundary; it is inside 'é' (bytes 0..2)
  ```

  `display_tree` always asks for the root span's start (byte 0) first, so **any
  document starting with a non-ASCII character panics**: `"é"`, `"—x{y}"`,
  `"😀"`. `"aé"` is fine — the trigger is the *first* character, or more
  generally any `line_col(k)` whose `k + 1` falls inside a character followed by
  a call for a larger offset. It is a debug/formatting path, but it is a panic
  reachable from safe, documented API on valid UTF-8, and across an FFI boundary
  it becomes a host-language crash rather than an error.
- **Wish**: make `extend_line_starts_up_to` resume from a boundary — e.g. scan
  `self.content[..end_at]` from `computed_end` after
  `floor_char_boundary`, or keep `computed_end` on a char boundary when it is
  stored (`content.ceil_char_boundary(up_to + 1)`). The loop already produces
  `char_indices`, so only the slice start is wrong.
- **Workaround taken (M5, revised)**: **contained, not avoided.** The panic
  cannot be prevented from outside techy — `display_tree` builds its own
  `LineIndex` and always asks for byte 0 first — but it can be kept from
  escaping as a `PanicException`, which derives from `BaseException` and
  therefore slips through `except Exception:` and reads to a Python user like an
  interpreter crash rather than a library failure. `techy.core.node.display_tree`
  and `NodeTree.display()` now wrap the call in `catch_unwind` and raise
  `techy.TechyError` naming this bug and the workaround (prefix the document with
  an ASCII character). This is sound because `display_tree` builds and drops its
  whole `Renderer` — every `LineIndex` in it — inside the call, so an unwind
  leaves nothing shared half-mutated. Pinned as a **ratchet** by
  `tests/test_nodes.py::test_display_contains_techys_multibyte_renderer_panic`:
  when techy fixes this, that test fails and the containment comes out.
  M4's note that "the bindings' own tests use ASCII fixtures for the display
  path" is why M2 did not catch it. `techy.source.LineIndexCache`,
  `NodeRef::summary` and `Diagnostics::render_all` are confirmed *not* affected
  (re-measured in M5).
- **Severity**: **blocking** — one panic per non-ASCII document on the
  pretty-printer everyone reaches for first, and across an FFI boundary a panic
  is a host-language `BaseException` at best and a crash at worst. Every
  consumer that embeds techy has to write this same `catch_unwind`, which is
  the argument for fixing the four-line resume-point bug upstream.


### `ParseContext`'s two lifetimes force an `unsafe` block on every FFI embedding, and unlike `RestageContext`'s they name real borrows

- **Kind**: design friction
- **Where**: `techy::core::constructs::ParseContext` (`src/constructs/mod.rs:118`)
- **Found while**: M6, lending `&mut ParseContext` into Python for one
  construct-parser call
- **What happened**: a `#[pyclass]` field cannot carry a lifetime, so the proxy
  that Python holds must name `ParseContext<'static, 'static, L>` while techy
  hands the parser a `ParseContext<'a, 's, L>`; `&mut T` is invariant in `T`, so
  the widening needs a cast and therefore an `unsafe` block. This crate already
  has three such blocks for `RestageContext` and `RecomposeContext` — but those
  two carry their lifetime as `PhantomData`, and techy's own field docs say so
  ("the context itself stores no borrow of it"), which makes the erasure an
  erasure *over nothing reachable* and the SAFETY argument two lines long.
  `ParseContext` is different: `tokens: &'a mut dyn TokenReader<'s, L>`,
  `session: &'a mut ParserSession<L>` and `driver: &'a L::Driver` are three live
  borrows, and `'s` names borrowed source text that reaches the token facts a
  parser reads. The argument had to be re-derived from scratch, and it now rests
  on a *discipline* the compiler cannot check — that no value borrowed from the
  context escapes the closure that touched it — because once the lifetime is
  erased, `'static` satisfies every bound and no `R: 'static` helper can catch
  a returned `StagedNodes<'static, _>`.
- **Wish**: nothing about the two lifetimes is used *by the context's own
  methods* — every op takes its inputs explicitly. If `ParseContext`'s fields
  were reachable through accessors that re-borrow from `&mut self` (`fn
  tokens(&mut self) -> &mut dyn TokenReader<'_, L>`), the struct would need no
  lifetime parameters at all and every FFI embedding of the construct-parser
  seam would become `unsafe`-free. Failing that, a `ParseContext::lend_to(f:
  impl FnOnce(&mut ParseContext<'_, '_, L>) -> R) -> R` on techy's side would at
  least put the one unavoidable cast in the library that can justify it.
- **Workaround taken**: `lend_parse_context` in `src/constructs.rs` — the cast,
  behind a `ScopedLease` whose `Drop` runs before the function returns (and
  while unwinding), with a **four-part** SAFETY argument: confinement, the
  lease's drop point, an audited classification of every value the proxy can
  return, and a borrow counter plus `!Send` token. Part 3 is the one that is
  discipline rather than proof, so it is enforced by a test that scans the
  file's own `#[pymethods]` blocks and fails the build on an unclassified
  member (`PROGRESS.md` D300, D301).
- **Severity**: **blocking** — binding the construct-parser seam at all
  requires erasing two lifetimes that name three live borrows, and no safe
  route exists: the cast is unavoidable from outside techy, and what keeps it
  sound is a discipline the compiler cannot check, where a mistake is a
  use-after-free rather than a failing test. Rated above the two
  phantom-lifetime erasures for exactly that reason — those erase nothing
  reachable


### A condition whose identity is only known at runtime cannot exist

- **Kind**: design friction
- **Where**: `techy::error::DiagnosticInfo::IDENTIFIER` (associated const) +
  `techy::error::DiagnosticData` (sealed by `sealed::Sealed`)
- **Found while**: M1, building the `Diagnostic` constructor and looking ahead
  to Python-authored conditions
- **What happened**: `IDENTIFIER` is an associated **const**, so a condition's
  wire identity is fixed per Rust *type*; `DiagnosticData` is sealed, so the
  blanket impl over `DiagnosticInfo` is the only way to get a payload into a
  `Diagnostic`. Together these make a runtime-identified condition
  unrepresentable — and that is precisely what a binding, a plugin host or a
  scripting layer needs: a Python class carrying `IDENTIFIER = "myapp.x.y"`
  cannot be wrapped in an adapter, because the adapter is *one* Rust type and
  would need *one* identifier for every class it ever wraps. This is the only
  place in techy where "third-party conditions are structurally identical
  citizens" (the `error` module docs) stops being true: it holds for third-party
  *Rust* crates, not for third-party *runtimes*.
- **Wish**: keep `IDENTIFIER` as the ergonomic default but let the trait method
  win — e.g. give `DiagnosticInfo` a `fn identifier(&self) -> &str { Self::IDENTIFIER }`
  that the blanket `DiagnosticData` impl forwards to. Nothing changes for the 25
  shipped conditions (or for `T::IDENTIFIER` matching), and one type can then
  carry a runtime identity. Unsealing `DiagnosticData` would work too but gives
  up the const-identifier discipline; the defaulted method keeps it.
- **Workaround taken**: `techy.conditions.Condition` (the generic fallback) is a
  read-only *view* — it can be built from any payload for display and mapping
  access, but `techy.error.Diagnostic(severity, condition, span)` refuses it
  with an explanatory `TypeError` (`src/conditions.rs::make_diagnostic`).
  `API_MAPPING.md` §5's "custom conditions from Python" is blocked on this.
- **Severity**: **blocking** — it is the one gap that removed a planned
  capability outright: `API_MAPPING.md` §5's "custom conditions from Python"
  cannot be built at all, and no workaround exists on either side of the
  boundary


---

## Major

Forced a documented deviation from the binding's own mapping doctrine, or a
permanent compromise visible in the Python API. A workaround exists, and it
costs something a user can see.

### An extension hook that fails has nowhere to say so — the same hole on six seams, twenty times

- **Kind**: design friction / contract risk
- **Where**: `Lang` (`initial_state_data`, `specials_trigger_chars`,
  `make_node_ext`); `ParseDriver` (`recovery`, `resolve_state_event`,
  `group_interior_delta`, `refine_diagnostic`, `make_paragraph_break_node`,
  `observe_transition`, `source_resolver`, and the three parser factories);
  `techy::core::constructs::TokenStopKind::Predicate`, `StopSpec::node`,
  `GroupChildState::Compute`, `InvocationChildState::Compute`;
  `techy::core::specs::CallableSpec::make_invocation_parser`;
  `techy::latexlike::EnvironmentBehavior::body_state_delta`;
  `techy::recompose::ComposePiece::append`;
  `techy::source::LineColProvider::line_col`
- **Found while**: M3 (`EnvironmentBehavior`), M5 (`ComposePiece`), M6
  (`make_invocation_parser`, `ChildStateSpec`), M7 (`StopSpec`/`ChildStateSpec`
  in full, and the `Lang`/driver hook table) — four milestones reaching the same
  wall from six different modules
- **What happened**: techy's extension points split cleanly in two. Fourteen
  hooks can report a failure — `ConstructParser::parse`,
  `ArgumentParser::parse_argument`, `Lang::finalize_transition`,
  `Lang::scan_specials`, `SpecsProvider::retrieve_spec`,
  `Recomposer::recompose_node`, `RestageVisitor::restage`,
  `SourceResolver::resolve` and friends. **Twenty answer a bare value.**

  For a Rust implementor that is a non-issue: an impl that cannot fail simply
  does not, and "pure" is a property the signature enforces. Across an FFI
  boundary it is a wall, because the *implementation is user code in another
  language* and can fail for reasons the hook knows nothing about — an
  `AttributeError`, a typo in a comparison, a `KeyError` in the author's own
  lookup table, a `KeyboardInterrupt`. `API_MAPPING.md` §3 rule 4 forbids
  swallowing an exception, so every one of these needed a policy invented from
  nothing. The roster, with what the binding was forced into at each:

  | Seam | Mute hooks | What the binding does |
  |---|---|---|
  | `Lang` (3) + `ParseDriver` (9) | 12 | park the exception and re-raise it from the enclosing `Language` operation, which always has a `PyResult` (D336) |
  | `StopSpec` / `ChildStateSpec` | 4 (`Predicate`, `node`, both `Compute` arms) | report through `sys.unraisablehook`, answer the documented no-answer value — `False`, or the inherited state (D314) |
  | `CallableSpec::make_invocation_parser` | 1 | answer a **stub parser** whose `parse` immediately returns `Err(cx.implementation_error(…))` — the failure is reported one call later than it happened |
  | `EnvironmentBehavior::body_state_delta` | 1 | `sys.unraisablehook`, answer `None` |
  | `ComposePiece::append` | 1 | stash the exception in a per-fold slot and abort at the next `recompose_node`, which *does* have a channel — so a handful of extra `append`s run after the failure, and the recomposer sees one more `recompose_node` call than it logically should |
  | `LineColProvider::line_col` | 1 | answer `None` — which techy then renders as one specific and untrue cause (see *A `None` from any `LineColProvider` renders as "line-index scan limit exceeded"*) |

  The count is twenty because `ParseDriver::observe_parse_start` is the one
  near-miss: it returns `()` but receives `diagnostics: &mut Diagnostics<…>`
  and its own doc says it "may record", so it has a report channel — just not a
  return-typed one. That is the shape the other twelve want. *(For scale: a
  sweep of every `dyn`-object trait in the crate finds **31** entry points that
  answer a bare value, 40 crate-wide once the pure value traits are counted.
  The twenty above are the ones a foreign-language implementation can actually
  be installed at.)*

  Two of these are worse than "the exception has nowhere to go", and they are
  the ones to look at first.

  **The stop arms answer a decision, not a value.** A predicate that was
  supposed to stop and instead raised produces a *longer node run* rather than
  a diagnostic — `False` means "keep parsing". The parse succeeds and the tree
  is wrong.

  **`initial_state_data` is worse still**, because it runs before a parse
  exists: a broken seed is currently *parsed with*, and the binding has to run
  the whole parse to completion and discard it in order to report the failure.

  The cost of the workarounds is measurable. Getting the `Lang`/driver side
  right needed a **stack** of per-operation frames (a single thread-local slot
  let a nested `Language` operation re-raise an enclosing parse's exception
  while the enclosing one returned normally), a first-wins rule for two hooks
  raising in one scope, a borrow discipline that survives `write_unraisable`
  re-entering the module, and a `sys.unraisablehook` floor — four rulings and
  ~60 lines (`PROGRESS.md` D340–D342) for something a `Result` would have made
  free. **Both of M7's blocking review defects landed in that machinery**, not
  in the hooks it serves.
- **Wish**: `-> Result<_, ParseError<L::SourceOrigin>>` on every hook that runs
  inside a parse, matching `ConstructParser::parse` and
  `ArgumentParser::parse_argument`, which already have it.
  `initial_state_data` is the odd one — it runs before a parse exists — and
  `Result<StateData<Self>, FinalizeError>` fits it, since `Language::new` could
  surface the failure exactly as `derived()` does. For `ComposePiece::append`,
  `fn append(&mut self, other: Self) -> Result<(), Self::Error>` with an
  associated `type Error = Infallible` default, threaded into `RecomposeError`
  as a `Piece(E)` variant, keeps every existing impl compiling. The
  `StopSpec`/`ChildStateSpec` arms are tier-2 parser temporaries, so the churn
  is confined to the parser that installs them.

  If any of these must stay infallible, **say so on the hook**. Half the cost
  here was not the missing channel but not knowing whether its absence was a
  constraint or an oversight — and after four milestones the answer is visibly
  "neither": it is a default nobody chose. `ComposePiece` is the clearest case
  to settle in one sentence, since techy's own two impls (`String` and `()`)
  genuinely cannot fail.
- **Workaround taken**: per the table above — `src/lang.rs` (`HookScope`, the
  parked-exception stack), `src/constructs.rs` (the stub parser, the
  stop/compute arms), `src/latexlike.rs` (`PyEnvironmentBehaviorAdapter`),
  `src/recompose.rs` (the per-fold error slot). Each has a test asserting the
  exception is *reported* rather than swallowed.
- **Severity**: **major** — twenty hooks, six seams, four milestones, one
  missing `Result`. It is the single largest source of invented policy in this
  binding, and the only gap in the note whose workaround was itself the source
  of blocking defects.

> **Merged M8** from six entries filed independently in M3, M5, M6 and M7
> (`EnvironmentBehavior::body_state_delta`; `ComposePiece::append`;
> `CallableSpec::make_invocation_parser`; the `ChildStateSpec` descent policy;
> `TokenStopKind::Predicate` + `StopSpec::node`; and the `Lang`-hook entry that
> counted the seams). Each filing had already noticed it was the *n*-th
> instance — "third instance of this shape", "the fourth and fifth", "this
> entry brings the count to four seams" — which is what made it one gap rather
> than six. The roster above is re-counted against techy's source at M8, which
> is where `observe_parse_start`'s diagnostics sink turned up.


### Of the five extension traits held as `dyn`, only `CallableSpec` has an `Any` supertrait — so four of them are one-way doors

- **Kind**: API asymmetry
- **Where**: `techy::core::specs::CallableSpec` (`spec/callable.rs:71`, has
  `Any`) against `techy::core::specs::SpecsProvider` (`scopes/mod.rs:437`),
  `techy::core::constructs::ArgumentParser` (`spec/structure.rs:130`),
  `techy::latexlike::EnvironmentBehavior` (`latexlike/environments.rs:186`) and
  `techy::source::SourceResolver` (`source/resolver.rs:50`) — none of which
  have it. Also `EnvironmentSpec::behavior()` and
  `ParseDriver::source_resolver()`, which hand out `&dyn`
- **Found while**: M3 (`SpecsProvider`, `EnvironmentBehavior`,
  `ArgumentParser`), M4 (the GIL-release proof, twice) — four surfaces, two
  milestones
- **What happened**: `CallableSpec: fmt::Debug + Send + Sync + Any`, and its
  docs explain why — a preset recovers its concrete spec type from a stored
  `Arc<dyn CallableSpec<L>>` by downcasting. That one supertrait is what lets
  the binding hand `node.spec` back as the *most specific* Python class, and
  hand a Python-implemented spec back **as the original object the user
  registered**. Four sibling traits are held the same way and lack it, so each
  is a value that goes in and cannot come out. The four instances, in the order
  they were hit:

  1. **`SpecsProvider`** (M3). A stored `Arc<dyn SpecsProvider<L>>` is opaque:
     nothing can tell a `Package` from a `Scope` from a third-party provider
     once it is on a stack. Directly observable — a Python spec pushed into a
     package and read back off a node answers the very object that was
     registered; a Python provider pushed onto a scope stack and read back can
     only answer a generic `SpecsProviderView`. Same shape of extension, two
     different round-trip guarantees.
  2. **`EnvironmentBehavior`** (M3), and it bites twice, because
     `EnvironmentSpec` additionally stores its behaviour in a private
     `Arc<dyn EnvironmentBehavior<LLL>>` that `behavior()` exposes only as
     `&dyn`. So a binding can neither downcast the behaviour to its concrete
     type nor clone the handle: `spec.behavior` cannot answer
     `VerbatimBehavior` for a verbatim environment, cannot answer the Python
     object `from_behavior` was given, and cannot be re-registered on another
     spec.
  3. **`ArgumentParser`** (M3). `ArgumentSpec::parser` is an
     `Arc<dyn ArgumentParser<L>>`, so a parser read back off a spec is opaque:
     nothing can tell a `GroupArgumentParser` from an
     `OptionalGroupArgumentParser` from a custom one. The information exists —
     the `argument_specs` code factory produces configured instances of the
     standard parsers — and is simply not recoverable, so
     `spec.arguments[0].parser` cannot answer the concrete Python class the way
     `node.spec` answers `MacroSpec`.
  4. **`SourceResolver`** (M4), where it stops being an ergonomics problem and
     starts costing throughput. The bindings release the GIL for a parse only
     when they can prove no Python code is reachable from it. Everything else
     in that proof is precise — a process-wide census of live Python adapters
     plus an exact question to the driver's hook table — but
     `ParseDriver::source_resolver()` hands back `Option<&dyn SourceResolver>`
     with no `Any` and no discriminant, so techy's own `MapResolver` (Rust,
     cannot call Python, perfectly safe to detach for) is indistinguishable
     from the binding's `PySourceResolverAdapter`. The flag therefore answers
     `False` for **any** configured resolver, including techy's own. That is
     §Known deviations **row 10**.

  A fifth consequence follows from (1) and does not look like the same problem
  until you trace it: **a `SpecsProvider` cannot be identified once it is
  inside a `ParsingState`** (M4). A binding must know whether a frozen state can
  reach host-language code, because that decides GIL release. `ScopeStack`
  hands back providers as `Arc<dyn SpecsProvider<L>>`; `iter_symbols` cannot
  help either, since a provider is allowed to answer "cannot enumerate". The
  reachability answer therefore has to be *carried* alongside every object that
  can enter a state, which no embedder can do without instrumenting its own
  constructors — hence the process-wide census, which is sound but pessimistic:
  one live Python spec anywhere disables GIL release for every language built
  while it lives (`PROGRESS.md` D30, and §Known deviations row 22 as the same
  shape reaching the hook table).
- **Wish**: add `Any` to `SpecsProvider`, `ArgumentParser`,
  `EnvironmentBehavior` and `SourceResolver`, with the rationale paragraph
  `CallableSpec` already carries. **One supertrait settles all four**, and it
  costs nothing: every one of these is already implicitly `'static`, because
  each is held as `Arc<dyn Trait>` = `… + 'static`. Two small additions round
  it out: `EnvironmentSpec::behavior_arc(&self) -> Arc<dyn EnvironmentBehavior<LLL>>`
  beside the borrowing `behavior()`, and — if `SourceResolver: Any` is not
  wanted — a `fn source_resolver_is_pure(&self) -> bool` hint on `ParseDriver`
  that a binding can override.
- **Workaround taken**: `crate::specs::provider_to_py` always answers a
  `SpecsProviderView`; `EnvironmentSpec.behavior` answers a read-only
  `EnvironmentBehaviorView` holding the *spec's* `Arc` and re-deriving the
  borrow per call, with registering a view refused by an explanatory
  `TypeError`; `techy.core.specs.ArgumentParserView` is an opaque handle
  exposing `can_match_empty` and techy's `Debug`; `Language(driver=…)` with any
  source resolver reports `releases_the_gil == False` and parses attached. Each
  asymmetry is documented on the class that carries it.
- **Severity**: **major** — four extension points are one-way doors where the
  fifth is not, and one of them costs measurable throughput on `\input`-driven
  documents (§Known deviations row 10). The fix is one word, four times.

> **Merged M8** from five entries filed in M3 and M4. The M4 entry on
> `source_resolver` had already written the merge's conclusion — "it is the same
> fix already wished for `SpecsProvider`, `CallableSpec` and `ArgumentParser`,
> and one supertrait would settle all four".
>
> **Corrected M8**: the M3 entries read as if `CallableSpec` were the only
> `Any`-bearing trait in the crate. It is not — `DiagnosticInfo`
> (`error.rs:53`) carries `Any` too, and so does the sealed `DiagnosticData`
> (`:84`). The claim that survives is narrower and still true: among the traits
> an embedder implements and techy stores behind `Arc<dyn _>`, `CallableSpec`
> is the only one. Also corrected: the sentence "`IntoArgumentParser`'s doc even
> spells out that `ArgumentParser` has no `Any` supertrait" — that text is real
> and verbatim, but it is an implementation `//` comment at
> `spec/structure.rs:207-208`, not rustdoc a reader will meet.


### `ParsingState` is identity-bearing by design and an embedder can only hold an `Arc` — but nothing takes one, nothing produces one from data, and nothing reads the data back out

- **Kind**: design friction / missing capability
- **Where**: `techy::core::Language::new` (`engine/language.rs:84`);
  `techy::core::ParsingState` (`state/parsing_state.rs:74`) — no `data()`, no
  `from_data`, no `Clone`, no `into_arc`; `techy::core::StateData`
- **Found while**: M3 (parsing under a state an embedder built earlier, and
  binding `StateData`), M4 (binding `techy.core.Language`), M7 (binding
  `Lang::finalize_transition`'s `prev` and the preset behaviour functions) —
  three milestones, four symptoms, one missing pair of constructors
- **What happened**: techy is explicit and right about *why* `ParsingState` is
  neither `Clone` nor `PartialEq`: states are **identity-bearing**, `Arc`
  pointer identity keys the derivation memos, and every node links to the state
  that parsed it. Every public producer hands one out either by value
  (`lang_initial`, `lang_initial_with_packages`, `derived`) or behind an `Arc`
  (`node.parsing_state()`, `Language::initial_state()`, `cx.state`). A
  garbage-collected host language can only hold the `Arc` — the same state is
  reachable from a node, from a language and from any number of wrappers.

  And then:

  1. **`Language::new(driver, initial_state: ParsingState<L>)` consumes the
     state by value** and immediately does `Arc::new(initial_state)`. An
     embedder holding an `Arc` cannot hand its state to a `Language` at all.
     The only reachable substitute is
     `state.derived(&ParsingStateDelta::new())`, which is data-equivalent but
     mints a *different identity*, so `node.parsing_state() == language.initial_state()`
     silently becomes false. **This is the one place where techy's own identity
     doctrine and its own constructor signature contradict each other**, and the
     workaround defeats the invariant that forced it.
  2. **There is no `ParsingState::data()`.** `StateData` is a fully public type
     with public fields, a documented `empty()` constructor and a `Clone` impl —
     but a consumer can only reach one through `StateData::empty()` or by
     implementing `Lang` (which a binding fixed to one monomorphization does
     exactly once). `ParsingState` stores one and hands out its four parts
     individually (`rules()`, `scopes()`, `mode()`, `ext()`) with no whole. So
     "the settings of this state as a value I can copy, tweak and derive from"
     means reassembling the struct field by field — precisely the struct-update
     pattern `StateData::empty()`'s own docs warn against, since a field added
     later would be silently dropped instead of failing to compile.
  3. **There is no way back.** The data→state freeze is crate-owned: a hook
     handed a `&ParsingState` can extract its data (every field is cloneable)
     but can never hand one *back*, and a helper that wants a `&ParsingState` —
     like the two preset behaviour functions — is unreachable from a hook even
     when the caller has every ingredient. `ParsingState::new(data)` exists and
     is `#[cfg(test)] pub(crate)`: doubly unreachable.
- **Wish**: three changes, of which the first two are one line each.
  1. `pub fn new(driver: L::Driver, initial_state: impl Into<Arc<ParsingState<L>>>)`.
     `Arc<T>: From<T>` makes every existing call site compile unchanged, and an
     embedder can pass the `Arc` it already owns. (A separate
     `Language::with_shared_state(driver, Arc<ParsingState<L>>)` would do as
     well; the struct already stores `Arc<ParsingState<L>>` internally.)
  2. `ParsingState::data(&self) -> &StateData<L>`, mirroring the four existing
     accessors — a getter over a field the struct already holds. It also gives
     `Lang::finalize_transition`'s `&mut StateData` argument a read-only
     counterpart a consumer can reason about outside the hook.
  3. `ParsingState::from_data(data: StateData<L>) -> Result<Arc<ParsingState<L>>, FinalizeError>`
     — the same freeze `lang_initial` performs, with the customizer run or not
     (both defensible; `lang_initial` does not run it). **This one closes (1)
     too**, since a caller could then describe any seed it wants.

  The mirror wish, less important: `ParsingState::into_arc(self) -> Arc<Self>`.
- **Workaround taken**: `techy.core.Language` **describes** its seed state
  instead of accepting one — `Language(packages=…, deltas=…)` builds it by value
  through `lang_initial[_with_packages]` + `derived` and reads it back from
  `language.initial_state`, which covers every state techy lets a caller
  construct. `Language.from_state(state)` is the escape hatch and is documented
  as forking the identity. That is §Known deviations **row 12**: `Language`
  takes no seed-state object, so `API_MAPPING.md` §10's
  "`Language(driver, state).parse(text)` flow" no longer names a real call.
  `techy.core.StateData` binds `empty()` plus the four read accessors and grew
  a builder surface (`preset()`, `with_rules`/`with_mode`/`with_ext`/
  `with_scopes`/`push_provider`) so a Python `initial_state_data` can describe a
  seed at all (`PROGRESS.md` D29, D40, D332).
- **Severity**: **major** — it makes a documented contract unobservable from any
  `Arc`-holding embedder, and it cost the Python API its most natural
  constructor (§Known deviations row 12).

> **Merged M8** from four entries: M3's `Language::new` filing, M3's
> "`ParsingState` exposes no `data()`", M4's sharper re-filing of
> `Language::new` (which had been marked "highest severity of the note"), and
> M7's "`StateData` cannot be frozen into a `ParsingState` from outside the
> crate". The M7 entry had already spotted it — "three milestones, three
> different symptoms, one missing constructor".
>
> **Corrected M8**: the M7 entry said `lang_initial` is "the only public
> freeze". `lang_initial_with_packages` (`state/parsing_state.rs:125`) is a
> second, and `derived()` is a third for non-initial states. The claim that
> survives is that none of them accepts a `StateData` a caller assembled.


### Every panicking precondition on caller-computed input becomes a precondition re-implemented in a second crate, with no compile-time link and no test that fires

- **Kind**: contract risk
- **Where**: `techy::source::Span::new` / `Span::extend_to` / `Span::slice`,
  `SourceSpan::new`, `SourcePos::new` (`source/span.rs:31,83,121`,
  `source/source.rs:236,375`); `techy::core::skip_whitespace`
  (`token/reader.rs:98`); `techy::core::specs::ScopeStack::scan_specials` and
  `Package`'s `SpecsProvider::scan_specials` impl (`scopes/mod.rs:1004`);
  `techy::core::constructs::ParseContext::stage_invocation`
  (`constructs/mod.rs:255`)
- **Found while**: M1 (making sure no techy panic can cross the FFI boundary),
  M3 (`skip_whitespace`, and adversarially testing `ScopeStack`), M6 (staging a
  callable whose child had been staged with a span before the trigger)
- **What happened**: techy's Panics list is exemplary — it names six precondition
  asserts and five indexing-style accessors, states the rationale ("these panics
  guard against programming errors in calling code — no document content can
  trigger them"), and puts a Panics section on each item's own page. Nothing is
  hidden. The problem is what a foreign-function boundary has to *do* about it:
  a panic unwinding into CPython is undefined behaviour, so every one of these
  must become an exception, which means the binding **re-implements each
  precondition immediately before calling techy** — `start <= end`,
  `end <= content.len()`, `content.is_char_boundary(start)`,
  `content.is_char_boundary(end)`, `end >= self.end()`, `pos` within content and
  on a boundary. Those checks now live in two crates with no compile-time link
  between them.

  **The drift is silent and it has already happened once.** If techy tightens or
  adds a precondition, the binding does not fail to compile and does not fail a
  test: it starts aborting the interpreter on the input that newly violates it.
  M6's review found the mirror image of the same failure — `SourceSpan::new`'s
  assert has *two* halves (in-bounds, then `is_char_boundary` on both ends) and
  only the inverted/out-of-bounds one had been replayed, leaving **eight
  `PanicException` routes reachable off a four-byte source**. The duplication is
  not a theoretical risk; it is a defect generator.

  Four instances, each with its own wrinkle:

  1. **The five span/pos constructors** (M1). `Span::get` shows the shape of the
     fix already exists for `slice`; the constructors have no such twin.
  2. **`skip_whitespace`** (M3) is worth its own line because it is a *free
     function* whose entire job is to take an offset the caller computed. It
     panics explicitly (`panic!("pos {} is out of bounds or not a char
     boundary…")`) rather than through an index, so the intent is unmistakable —
     and the crate already documents that with the gate off it returns `pos`
     unchanged, which is exactly the non-panicking behaviour a `try_` twin
     would give.
  3. **`ScopeStack::scan_specials`** (M3) is the odd one out and closer to a
     bug: `Package::scan_specials` does `&content[pos..]` with **no bounds
     check**, so `scan_specials(state, "héllo", 999)` panics and so does
     `pos = 2` (mid-codepoint). Both methods are `pub`, **neither documents a
     precondition on `pos`**, and — re-checked at M8 — the crate-level Panics
     list does not name them, so this one is outside the family the list
     declares complete. Inside the engine `pos` always comes from the tokenizer,
     so the panic is unreachable there; a direct caller, which the `pub`
     signature invites and which a binding *is*, finds it immediately.
  4. **`ParseContext::stage_invocation`** (M6) is where the family stops being
     an M1 problem. Every consumer API bound in milestones 1–5 got its spans
     *from* techy, so the asserts were unreachable. A construct parser writes
     byte offsets by hand, and `stage_invocation` computes a `SourceSpan` **for**
     you (trigger start → last staged child's span end, `constructs/mod.rs:238-255`):
     stage a child whose span precedes the trigger and techy panics from inside
     a method the caller never passed a span to. The moment construct parsers
     became extensible, an author's ordinary mistake became a
     `PanicException` across the boundary.
- **Wish**: `try_new` twins returning `Option`/`Result` — `Span::try_new`,
  `SourceSpan::try_new`, `SourcePos::try_new`, `Span::try_extend_to`,
  `try_skip_whitespace` — with the panicking versions defined in terms of them.
  Every embedder that is not a Rust application (bindings, servers, editor
  plugins) needs exactly this, and it also makes the preconditions testable from
  outside. For `scan_specials`, either state the precondition on both methods
  and add them to the crate Panics list, or make `Package`'s impl answer
  `Ok(None)` for a `pos` past the end — it already has to handle "no match
  here". For `stage_invocation`, return `Err(ImplementationError)` for a
  computed span it cannot build: it already returns `ConstructParserResult` and
  already lifts `NodeBuildError` that way, so the channel exists and this is a
  contract violation of exactly that kind.

  Stated generally, and this is the request that matters: **the parse-time
  constructors reachable from an extension point should not have panicking
  preconditions.** The M1–M5 read surface can live with them; the M6 author
  surface cannot.
- **Workaround taken**: `check_ordered` / `check_offset` / `checked_source_span`
  / `check_char_boundary` in `src/source.rs`, `check_scan_position` in
  `src/specs.rs`, `py_skip_whitespace` in `src/state.rs` — each called before
  the techy call, each raising `ValueError`, each pinned by a Python test that
  feeds it the exact inputs techy panics on plus a Rust test asserting the guard
  covers them.
- **Severity**: **major** — the workaround duplicates techy's invariants across
  a crate boundary with no tripwire, and the one time it was got half-right it
  left eight crash routes open off a four-byte input.

> **Merged M8** from four entries filed in M1, M3 and M6. The M3
> `skip_whitespace` entry had already named the merge — "the same shape as [the
> span constructors] above, one topic further on".


### There is no public `NodeTree` → `NodeSlice` constructor **from a range**, so every consumer holding `(tree, range)` reimplements the slice accessors

- **Kind**: design friction / API asymmetry
- **Where**: `techy::core::node::NodeSlice::new` (`node/slice.rs:45`,
  `pub(crate)`), against `NodeSlice::range()` (`:90`, public),
  `NodeTree::nodes_in` (`node/tree.rs:269`, public but an iterator),
  `NodeTree::is_single_source` (`:186`, `pub(crate)`) and
  `ChildRegion::children`
- **Found while**: M2 (binding `NodeSlice` as a `collections.abc.Sequence`),
  M5 (binding the four `techy::extract` producers, all of which take a
  `NodeSlice`)
- **What happened**: a binding cannot hold a borrow, so its node-list handle is
  `(tree object, index range)` — which is techy's own bindings advice, "hold
  trees + ids, not node references" (`ai-guide-embedding`). There is no way to
  turn that pair back into a `NodeSlice`. The public surface is *asymmetric*:
  `slice.range()` hands the range **out**, `tree.nodes_in(range)` takes one
  **in** but answers an iterator, `ChildRegion::children()` hands out a bare
  `Range<u32>` that only `nodes_in` accepts, and `NodeSlice` itself is `Copy`
  with a tree borrow and two `u32`s inside, so nothing about the type wants to
  be private. It was paid for twice.

  **M2 — the read surface.** `span()`, `source_text()`, `first()`, `last()` and
  the private `is_single_source_run()` all had to be reimplemented against
  `NodeRef`. Reimplementing `is_single_source_run` also *loses* its O(1) fast
  path, because the flag it reads (`NodeTree::is_single_source`) is
  `pub(crate)` too: the binding scans the run on every `span` read, on every
  tree. That is §Known deviations **row 2** — `NodeSlice.span` / `.source_text`
  ship as properties although the implementation is O(run length).

  **M8 — the number, so the cost is not a matter of opinion.** Benchmarked on a
  1 341-node run: `NodeSlice.span` costs **58.9 µs** where the crate's own O(1)
  path is about **0.94 µs** — a factor of ~63, and as much wall clock as minting
  **79** `Node` handles.  `.source_text` costs 66.5 µs.  Read in a loop over a
  document's slices it is quadratic, and it is the single sharpest performance
  trap this binding has; `docs/performance.md` documents it and
  `benchmarks/test_nodes.py::test_slice_span_is_linear_in_the_run` pins the
  shape.  One `pub` keyword on `is_single_source` removes all of it.

  **M5 — the extract producers.** Every `techy::extract` helper takes a
  `NodeSlice`, so the same missing method cost a second, larger workaround in a
  second module with its own execution path and its own test suite. It is not a
  corner case: the *documented* input of every extract helper is
  `node.argument_content_nodes(i)`, which is a **proper sub-range** of the
  callable's children (`region.content_range()`), so a binding cannot even get
  away with "only whole children runs are supported". And a user writing
  `nodes[1:3]` in Python is writing ordinary Python.

  The M5 workaround is the single largest piece of design work in that module:
  a lookup re-derives the run from the first node's parent by matching the range
  against `children()`, `body()`, `argument_nodes(i)`,
  `argument_content_nodes(i)` and `slot_content_nodes(i)`; anything else is
  staged into a throw-away one-`List` anchor tree whose root carries the input
  tree root's span and state, so techy's `anchor()` computes the same answer.
  Node ids then differ from the caller's, so the input `NodeId` is carried in
  the anchor tree's annotations (for the callback's `part.original`) and a
  parallel-walk map brings error ids home (`PROGRESS.md` §D101).
- **Wish**: `pub fn NodeTree::slice(&self, range: Range<u32>) -> Option<NodeSlice<'_, L, A>>`
  — `None` out of bounds, so it is also the non-panicking `nodes_in`. One method
  closes the asymmetry, restores the fast path, deletes ~60 lines of
  re-implementation on the read side and ~120 plus an entire second execution
  path on the extract side. Making `NodeSlice::new` public with its existing
  assertion would do as well, though the `Option` form is the better shape for
  an embedder that must pre-check anyway. There is no invariant to protect:
  `NodeSlice` makes no claim about its nodes that the accessors' own outputs do
  not already satisfy, and `NodeTree::covering_slice` already hands out a
  `pub(crate)`-constructed one built from a range it computed. Making
  `NodeTree::is_single_source` public alongside would close the O(1) half.
- **Workaround taken**: `PyNodeSlice::{covering, is_single_source_run}` in
  `src/nodes.rs` (M2); the two-path resolution in `src/extract.rs` (M5,
  `PROGRESS.md` §D101).
- **Severity**: **major** — two milestones, two modules, a documented deviation
  and a lost O(1) fast path, all closable by one method whose exact signature
  both filings independently asked for.

> **Merged M8** from two entries filed in M2 and M5, which had converged on the
> same wish *verbatim, down to the signature*. They were kept apart because
> entries were cited by ordinal.
>
> **Corrected M8**: both entries were titled "No public `NodeTree` →
> `NodeSlice` constructor", which is too strong — `NodeTree::covering_slice(&span)`
> (`node/tree.rs:343`) is public and does return a `NodeSlice`, and so do
> `NodeRef::children()`, `body()`, `argument_nodes(i)`,
> `argument_content_nodes(i)` and `slot_content_nodes(i)`. What does not exist
> is a constructor taking the **`Range<u32>` that `NodeSlice::range()` hands
> out** — the round trip, not the type. The title and the wish are restated
> accordingly; everything else in both entries stands.


### `NodeTree`'s annotation vector is reachable only as `&[A]`, so a garbage-collected host can neither own it nor drop it

- **Kind**: missing feature / design friction
- **Where**: `techy::core::node::NodeTree` — `annotations: Vec<A>` is
  `pub(crate)` (`node/tree.rs:164`) and the only accessors are
  `annotations() -> &[A]` (`:283`) and `annotate::<B>()` (`:301`). No
  `annotations_mut`, no `into_annotations`, no `take_annotations` anywhere in
  the crate
- **Found while**: M2 (designing the Python-visible tree), M5 (running every
  `KEEP` and callback producer call in `techy::extract`)
- **What happened**: both accessors are read-only with respect to the existing
  vector. For a Rust consumer that is right — trees are frozen and
  re-annotation mints a new stage. For a **garbage-collected** host language it
  is a hard blocker, and then a recurring tax.

  **M2 — the blocker.** The plan was `A = Option<Arc<Py<PyAny>>>`, so an
  annotation is a Python object and `node.annotation is node` is a reference
  cycle that CPython can only break by calling `tp_clear` on the owner.
  `tp_clear` must *drop* the references — and there is no path from a
  `&NodeTree`, or a `&mut` one, or an `Arc<NodeTree>` shared with every live
  node handle, to dropping the contents of `annotations`. `Vec<A>`'s elements
  are unreachable to the outside forever. The failure mode is silent:
  everything works, and annotated trees with cycles simply leak. The binding
  therefore keeps the techy value at `A = ()` and stores annotations in the
  Python tree object instead — §Known deviations **row 1**, a declared deviation
  from the binding's own mapping doctrine.

  **M5 — the price of that deviation, paid twice per producer call.** With the
  annotations beside the tree rather than in it, every annotation-bearing
  `techy::extract` call round-trips:
  - *in*: the input tree is `NodeTree<L, ()>`, so a `KEEP` run mints
    `core.annotate(|n| (n.id(), ann[i].clone()))` — one vector of the **whole
    tree**, not of the run being split;
  - *out*: the producer answers `NodeTree<L, PyValue>` and the binding needs the
    annotations out and the tree back at `A = ()`. With only `annotations() ->
    &[A]` that is `annotations().to_vec()` (a clone of every annotation) plus a
    second `annotate(|_| ())` pass.

  Neither is expensive in absolute terms — cloning a `PyValue` is an `Arc` bump
  — but both are pure ceremony around a vector the tree already owns and is
  about to drop.
- **Wish**: `NodeTree::into_annotations(self) -> (NodeTree<L, ()>, Vec<A>)`.
  It is the one that matters: a **move**, not a new capability, and it is what
  lets an embedder own the annotation vector without copying it. Because it
  consumes the tree it cannot violate the frozen-tree doctrine, and because it
  hands over ownership it is exactly what a host collector needs in `tp_clear`.
  `annotations_mut(&mut self) -> &mut [A]` or
  `take_annotations(&mut self) -> Vec<A> where A: Default` would each also
  unblock M2; only `into_annotations` also removes M5's round trip.
- **Workaround taken**: `PyNodeTree { core: Arc<NodeTree<L, ()>>, annotations:
  RwLock<Vec<PyValue>> }` in `src/nodes.rs`, which makes the Python object the
  unique owner and lets `__traverse__` and `__clear__` be written correctly;
  `annotations().to_vec()` + `annotate(|_| ())` once per producer call in
  `src/extract.rs`, documented at the call site (`PROGRESS.md` D14, §Known
  deviations row 1).
- **Severity**: **major** — it is the reason the binding's central data type
  deviates from its own mapping doctrine, and the alternative it forecloses
  leaks memory silently.

> **Merged M8** from two entries: M2 filed the blocker as a prediction, M5 filed
> the bill. The M5 entry was explicit that it was evidence for the M2 request
> rather than a new gap.


### Half the hooks take `&Arc<ParsingState<L>>` and half take `&ParsingState<L>`, and only the first half can cross an FFI boundary

- **Kind**: design friction
- **Where**: `techy::core::specs::SpecsProvider::retrieve_spec` /
  `::scan_specials` (`scopes/mod.rs:451`, `:465`);
  `techy::core::ParseDriver::resolve_command` and
  `techy::core::CommandResolver::resolve_command` (`engine/driver.rs:180`,
  `:454`) — all four taking `state: &ParsingState<L>`
- **Found while**: M3 (adapting a Python-implemented `SpecsProvider`), M4
  (binding `CommandResolver` behind `StdParseDriver(command_resolver=…)`)
- **What happened**: `ParsingState` is deliberately not `Clone` and its identity
  is load-bearing, so a host-language object can only hold an
  `Arc<ParsingState<L>>`. A borrowed `&ParsingState` therefore cannot be handed
  across the boundary as the state *object* the host already knows — the very
  parameter techy provides for the hook's own dispatch is the one thing the
  adapter cannot forward.

  **techy already does the right thing in four places**, which is what makes
  this a fixable inconsistency rather than a design constraint:
  `ParseDriver::probe_token`, `Lang::make_node_ext`,
  `GroupChildState::Compute` and `InvocationChildState::Compute` all take
  `&Arc<ParsingState<L>>`, and all four cross whole, identity included. So does
  `ParseContext::state`. The four hooks above take the bare borrow and do not.

  The fallout differs by hook, and the sharper case is the one to weigh:

  * For a **provider**, it is mild. techy's own trait docs say "per-mode
    visibility is the provider's own business, checked against `state.mode()`",
    and the mode crosses fine — so the binding passes the mode. But that is a
    guess licensed by prose rather than by the signature, and a provider that
    wanted `state.rules()` (a lazily-loaded provider keying on the active escape
    characters, say) is simply out of reach.
  * For a **command resolver**, it is sharper: the standard implementation of
    the hook *is* a scope-stack lookup (`resolve_command_in_scopes`), and a
    host-language resolver cannot perform it. The extension point that remains —
    resolve a name from your own table, database or cache — is genuinely useful,
    but it is strictly less than what Rust callers get, and the Python
    `CommandResolver` ABC has to carry a sentence saying so.
- **Wish**: `&Arc<ParsingState<L>>` on all four, matching the four hooks that
  already have it. Every call site holds the `Arc` — `ScopeStack::retrieve_spec`
  / `scan_specials` are reached from paths carrying `ParseContext::state` or
  `Lang::scan_specials`, and the parse loops carry `Arc<ParsingState<L>>` and
  deref it at the call — so this is a signature change with **no data-flow
  change and no cost to Rust callers**. Failing that, if the borrow is
  deliberate, say in the trait docs that `mode()` is the *only* supported
  consultation, so an embedder can rely on it rather than infer it.
- **Workaround taken**: `PySpecsProviderAdapter::retrieve_spec` /
  `scan_specials` call Python with the mode instead of the state
  (`src/specs.rs`, documented on the `techy.core.specs.SpecsProvider` ABC); the
  resolver takes the reduced shape `resolve_command(state, name, escape_char)`
  where the engine passes the **mode** (`PROGRESS.md` D33, `API_MAPPING.md` §9,
  with the "cannot walk the scope stack" sentence on the ABC and a test
  asserting it is there). `ScopesCommandResolver` stays a Rust value and is
  driven in Rust, so the shipped strategy loses nothing.
- **Severity**: **major** — two `API_MAPPING.md` §9 reductions and a permanently
  narrower extension point, against a change techy has already made four times
  elsewhere in the same trait family.

> **Merged M8** from two entries filed in M3 and M4. The M4 entry had already
> named it "the same wall as `SpecsProvider`'s hooks (logged above)".
> **Sharpened M8**: the four hooks that *do* take `&Arc<ParsingState<L>>` were
> found while re-checking the signatures; the earlier entries argued the wish
> from first principles and did not know techy already agreed with them.


### The staged phase is write-only from outside: a caller cannot read back what it built, enumerate what it staged, or address what it was handed

- **Kind**: API gap / API asymmetry
- **Where**: `techy::core::node::ChildRegion` — `staged()` is `pub(crate)`
  (`node/arguments.rs:193`) while `new` (`:127`) and `single` (`:134`) are
  public and `children()`/`content_range()`/`content_parent()` panic on an
  unresolved region (`:185`); `techy::core::node::StagedNodes`
  (`node/builder.rs:396`) — `len`/`is_empty`/`get(BuildId)` and no `iter`/`ids`,
  against a `BuildId` (`:41`) with a private field, no constructor and no
  `index()`; `techy::core::node::StagedChildren` (`:472`) — `iter()` but no
  `ids()`, and `StagedChildView` (`:509`) with no `id()`
- **Found while**: M6 (binding the staging vocabulary `StagedArgument` /
  `StagedSlot`, and the four `Staged*` views), M6 part 2
  (`parse_declared_arguments`), M7 (binding `make_node_ext` and `StopSpec::node`)
- **What happened**: the staging phase is a *composition* vocabulary — a caller
  builds regions, stages nodes and is handed views of what exists so far — and
  in every direction the read half is missing. Three instances:

  1. **A `ChildRegion` a caller built cannot be read back.**
     `ChildRegion::new(children, content)` and `ChildRegion::single(offset)` are
     public, so a caller can build a staged region — but `children()`,
     `content_range()` and `content_parent()` **panic** on one, `is_resolved()`
     only says *that* it is staged, and `staged()` is `pub(crate)`. A value made
     two lines earlier cannot be inspected, and a `Debug` print is the only
     route to its contents. For a binding this is not a nuisance but a
     correctness hazard: a Python `StagedArgument` must expose `.children` and
     `.content` (an authoring record the user is composing wants to be
     inspectable, and a test suite must be able to assert on it), so the binding
     keeps a **parallel copy** of what it passed to `ChildRegion::new` rather
     than reading it back. Two copies of one fact is exactly the drift risk
     techy's own "recomposability must not depend on cooperation" rule exists to
     avoid. It is also one of the crate's few deliberate panics reachable from
     ordinary outside code — `ChildRegion::single(0).children()` panics, and
     nothing in the type's signature says so.

     *Hit again, M6 part 2, from the other direction*:
     `parse_declared_arguments` *answers* `Vec<ParsedArgument<L>>`, and a
     binding cannot re-mint those records because their `ChildRegion`s are
     staged. So the composition helper techy publishes "as a
     takeover-composition building block" can only hand its output straight back
     to `stage_invocation` — the records cross as an opaque bundle
     (`techy.core.constructs.DeclaredArguments`, `PROGRESS.md` D268), and the
     offsets a Python parser might want to adjust are invisible to it.

  2. **`StagedNodes` cannot be enumerated.** It offers `len()`, `is_empty()` and
     `get(BuildId)`, and `BuildId` has no public constructor and no accessor for
     the `u32` it wraps. The two facts together mean the view is usable only by
     a caller that already **holds** the ids: "show me everything staged so far"
     is not expressible even though the arena is right there and `len()` reports
     its size. In-crate that is fine — the parser holds its own ids. Across a
     binding it is the difference between a debuggable view and an opaque one: a
     Python `StagedNodes` gets `__len__` and `get`, and cannot get `__iter__`,
     `__repr__`-with-contents, or the "dump what I have staged" that is the
     first thing anyone reaches for when a hand-driven build produces the wrong
     tree.

  3. **`StagedChildren` exposes no coordinates at all**, and this one costs an
     `unsafe` block. It holds `arena: &'b [Staged<L>]` and
     `children: &'b [BuildId]`, both private, and offers `len`/`is_empty`/
     `get(i)`/`iter()` and nothing else — in particular it will not tell you the
     `BuildId`s it holds, and `StagedChildView` deliberately has no `id()`
     either (unlike `StagedNodeView`, which does). For a Rust caller that is
     right: the descent-only shape is the point, and the ids would be a key into
     the wider `StagedNodes` lookup the view exists to withhold. But it removes
     the cheap route for a garbage-collected host: the sibling view is keyed by
     `BuildId`, so a proxy over `StagedNodes` needs only a `Vec<BuildId>` and a
     validity token — no erasure, no `unsafe`, which is what M6 shipped.
     `StagedChildren` admits neither that nor an owned snapshot (its recursive
     `children()` is the whole value, and copying it eagerly is O(subtree) per
     node minted, i.e. O(n²) over a document), so the binding's seventh
     `unsafe` block is a lifetime erasure addressed by *descent path* (`[i, j]`
     is child `j` of child `i`) because there are no ids to address it by.

  The asymmetry between (2) and (3) is visible from inside techy and points
  both ways: the descent-only view can be walked and the wide one cannot; the
  wide one is addressable and the descent-only one is not.
- **Wish**: three accessors, none of which weakens anything.
  * `ChildRegion`: make `staged()` public, or add
    `fn staged_children(&self) -> Option<Range<u32>>` +
    `fn staged_content(&self) -> Option<&ContentNodes>` — the `Option` shape the
    resolved accessors should arguably have had. Either turns three deliberate
    panics into a phase question the type already knows the answer to.
  * `StagedNodes`: `fn iter(&self) -> impl Iterator<Item = StagedNodeView<'b, L>>`.
    The ids are `0..len` internally, so it costs nothing and needs no new public
    knowledge about `BuildId`.
  * `StagedChildren`: `fn ids(&self) -> &'b [BuildId]`, or `StagedChildView::id()`
    restricted to the ids inside the view. The descent-only guarantee is not
    weakened — a `BuildId` is only usable against a `StagedNodes` the embedder
    would have to be handed separately.
- **Workaround taken**: a duplicated field per staged record and an opaque
  `DeclaredArguments` bundle (`src/constructs.rs`, `PROGRESS.md` D268);
  `StagedNodes` bound with `__len__` + `get` and no iteration; `src/nodes.rs`'
  `erase` — a lifetime erasure behind `conv::lend`, the crate's seventh
  `unsafe` block, with every value the lent view answers an owned copy or a
  proxy carrying the same token, asserted by a source-scanning audit
  (`PROGRESS.md` D310).
- **Severity**: **major** — it forces a duplicated fact per record (the drift
  risk techy's own rules exist to prevent) and one `unsafe` erasure that a
  three-word accessor would delete, on the seam where the borrow is genuinely
  live (see *`make_node_ext` hands out a borrow of the arena it is about to
  grow*).

> **Merged M8** from three entries filed in M6 and M7, plus M6 part 2's
> hit-again note on `parse_declared_arguments`. The M7 entry had already framed
> its own case against the M6 one — "`StagedChildren` admits neither that nor an
> owned snapshot" where `StagedNodes` does.


### `ConcatPieces` can be built and consumed but never read, so an instruction cannot cross an FFI boundary

- **Kind**: design friction
- **Where**: `techy::recompose::ConcatPieces` (`into_parts` is `pub(crate)`; no
  accessors), consumed by `techy::recompose::core_source_instruction` and
  `techy::latexlike::SourceRecomposer::recompose_node`
- **Found while**: M5, binding `core_source_instruction` and the preset source
  recomposer's delegation seam
- **What happened**: `ConcatPieces` has six chainable *setters* (`wrap`, `join`,
  `with_state`, `include_attached`, `include_hidden`, and the `children()` seed)
  and **zero getters**; `into_parts` — the one way to see the head, separator,
  tail, derived state and scope flags — is `pub(crate)`. Inside the crate that is
  exactly right: an instruction is produced and immediately lowered by the
  driver, and nobody in between needs to look. Across a binding it is fatal.

  A `Recompose<P, S>` handed to the Python side has to *become* a Python value —
  the instruction is what a recomposer returns, and `API_MAPPING.md` §2 maps a
  data-carrying enum to "a class with named constructors + accessor properties".
  The `Emit` arm crosses fine (one piece). The `Concat` arm cannot cross at all:
  the binding can neither read the parts to build the Python object, nor let a
  Python wrapper widen them — and widening is the shipped recipe. techy's own
  test writes it:

  ```rust
  match self.inner.recompose_node(node, state, cx)? {
      Recompose::Concat(pieces) if node.name() == Some("input") =>
          Ok(Recompose::Concat(pieces.include_attached())),
      other => Ok(other),
  }
  ```

  In Rust that works because `pieces` is passed straight through. In Python the
  wrapper's `instruction` has to *return* a `Recompose` object, so the binding
  must have materialised one, so it must have read the parts.

  Cost: the preset source instruction (both the core-complete block and the
  latexlike callable rules, ~90 lines) is **mirrored** in the binding crate,
  generic over the piece type, purely to obtain readable parts. techy's own
  implementation still runs for the whole-tree fold, so the two are held together
  by a corpus test that folds the same 22 inputs both ways and compares bytes —
  which is a test that exists only because the value cannot be read.
- **Wish**: make `ConcatPieces` readable. Either `pub fn into_parts(self)`
  (already written, one keyword away), or the six accessors —
  `head()`, `sep()`, `tail()`, `derived_state()`, `includes_attached()`,
  `includes_hidden()`. The getters are the friendlier half: they also make the
  chainable builders inspectable in a debugger and in techy's own tests, where
  today `concat_pieces_constructors_chain` can only assert that the types line
  up, never that `wrap("{", "}")` put `{` in the head.

  The same shape applies one level up: `Recompose<P, S>` has no `is_emit` /
  `piece()` / `pieces()` either, so a consumer that receives an instruction from
  a delegate can only `match` it (fine in Rust) and a binding cannot project it.
- **Workaround taken**: a lifetime-free `SourceShape<P>` mirror in
  `src/recompose.rs`, cross-pinned against techy's `SourceRecomposer` by a
  byte-comparison corpus test in Rust and another in Python.
- **Severity**: **major** — a documented recipe — widening a delegate's
  instruction — is unreachable, and closing it cost a ~90-line mirror of
  techy's own implementation held in step by a byte-comparison corpus test
  (§Known deviations row 17)


### `Diagnostic` has no `PartialEq`, so no binding can make `Diagnostics` a real sequence

- **Kind**: API asymmetry / design friction
- **Where**: `techy::error::Diagnostic` (and `techy::error::TraceFrame`)
- **Found while**: M5, closing the M4 review's open defect — `Diagnostics` is
  documented and registered as a Python `collections.abc.Sequence`, and
  `diagnostics[0] in diagnostics` silently answered `False`
- **What happened**: `Diagnostic` derives `Debug, Clone` and nothing else,
  because its payload is a `Box<dyn DiagnosticData>` and `DiagnosticData` has
  no `PartialEq` supertrait. Every other value type in the crate compares
  (`SourceSpan`, `Severity`, `DiagnosticValue`, `NodeId`, …), so a consumer
  reasonably expects `Vec<Diagnostic>` to support `contains`, dedup, and
  golden-file comparison in tests — and none of it works. For a *binding* it is
  sharper still: a Python collection that is registered as a `Sequence` owes
  `__contains__` / `index` / `count`, and there is no way to implement them
  without inventing an equality the crate does not define.
- **Wish**: `PartialEq` (and `Eq`) on `Diagnostic`, via a
  `fn eq_data(&self, other: &dyn DiagnosticData) -> bool` on `DiagnosticData`
  (or the simpler `DiagnosticData: PartialEq`-by-projection: compare
  `serializable_data()`, which is already `PartialEq + Eq`). `TraceFrame` needs
  the same. That would be *the* definition rather than each consumer's guess,
  and it would let `Diagnostics` grow `contains`/`dedup` in techy itself.
- **Workaround taken**: the binding defines the equality over techy's own
  canonical projection — severity, `SourceSpan` (by source identity plus range,
  the crate's rule), condition identifier, rendered message,
  `DiagnosticData::serializable_data()` and the frame list. It is defensible
  precisely because every ingredient is techy's, but it is a *guess about
  intent*: whether two diagnostics from two parses of the same text "are the
  same diagnostic" is a question only techy can answer normatively, and the
  binding answered "no" to stay consistent with `SourceSpan`. See
  `PROGRESS.md` §D90.
- **Severity**: **major** — a Python collection registered as a `Sequence` owes
  `__contains__`/`index`/`count`, so the binding had to invent an equality
  techy does not define — a semantics decision a consumer should never be
  making (D90)


### `SpecsProvider::iter_symbols` yields rows borrowed from `&self`

- **Kind**: design friction
- **Where**: `techy::core::specs::SpecsProvider::iter_symbols`,
  `techy::core::specs::SymbolEntry`
- **Found while**: M3, adapting a Python-implemented `SpecsProvider`
- **What happened**: the hook returns
  `Option<Box<dyn Iterator<Item = SymbolEntry<'_, L>> + '_>>`, and `SymbolEntry`
  holds `name: &'a str` and `spec: &'a Arc<dyn CallableSpec<L>>` — both borrowed
  from the provider's own storage. A provider that *computes* its answer (a
  foreign-language implementation, a lazily-loaded database — exactly the
  lazy-loading case the trait's doc says the all-dyn design exists to admit)
  has nowhere to put the freshly-built rows: handing back references into a
  `Mutex`-guarded cache needs `unsafe`, and leaking them is worse.
- **Wish**: make the row own its data (`name: Box<str>`, `spec: Arc<dyn CallableSpec<L>>`)
  — one `Arc` clone and one small copy per row, on a path that already allocates
  a `Box<dyn Iterator>` and is used only by diagnostics (`did_you_mean_hint`,
  `check_provider_commands_shadowed_by_escape`) and by `ScopeStack::iter_symbols`,
  which collects into a `Vec` anyway. Alternatively add an owned sibling
  (`symbols(&self, …) -> Option<Vec<OwnedSymbolEntry<L>>>`) with the borrowed one
  as the fast path.
- **Workaround taken**: the Python adapter reads its symbol table **once**, at
  registration, for every reserved `(callable type, mode)` pair, and stores it
  owned (`PySpecsProviderAdapter::symbols` in `src/specs.rs`). A vocabulary value
  created at runtime therefore answers "cannot enumerate", and a provider whose
  definitions change is stale — both documented, neither wanted.
- **Severity**: **major** — the Python surface is permanently narrower: a
  provider's symbol table is read once at registration, so a runtime-registered
  vocabulary value answers "cannot enumerate" and a provider whose definitions
  change is stale — both documented, neither wanted


### `NodeKindData`'s Python-facing twin has no read direction, so a `NodeKind` in flight cannot be shown to an extension

- **Kind**: missing capability
- **Where**: `techy::core::node::NodeKind<L>` handed to
  `Lang::make_node_ext(kind, …)` and answered by
  `ParseDriver::make_paragraph_break_node`
- **Found while**: M7, binding `make_node_ext`
- **What happened**: a node's payload is richly readable *after* the tree exists
  (`NodeRef::group()`, `::callable()`, …) and constructible *before* staging
  (`GroupData::new`, `CallableData { … }`), but the moment techy actually hands a
  `NodeKind` to an extension — `make_node_ext`, which is where a language
  computes its per-node data — there is no owned projection of it. Every reader
  in the crate is a borrow of a finished node. An FFI binding therefore has
  nothing to hand across but the discriminant.
- **Wish**: an owned, read-only view of a `NodeKind`'s payload — even just
  `kind.group_type()`, `kind.callable_name()`, `kind.callable_spec()` as
  `Option<_>` accessors on `NodeKind<L>` itself. techy's own documentation for
  the hook describes the expected use as "match a `Callable`, read its `spec`,
  downcast, compute ext", which is precisely what an embedding cannot do.
- **Workaround taken**: the hook receives the discriminant plus the node's own
  `SourceSpan` (whose `content` is the node's source text verbatim) and the
  staged-children view; `PROGRESS.md` D334.
- **Severity**: **major** — it is the only argument of the only *required*
  `Lang` method that does not cross


### Three public `#[non_exhaustive]` types have no constructor at all, so a validator, a hook and a condition are unreachable from outside

- **Kind**: API asymmetry / missing feature
- **Where**: `techy::core::node::TreeViolation` (`node/invariants.rs:291`);
  `techy::latexlike::EnvironmentInvocation` (`latexlike/environments.rs:159`);
  `techy::latexlike::MalformedBegin` (`latexlike/environments.rs:102`)
- **Found while**: M1 (generating a constructor per condition class), M2
  (testing `validate_tree`'s failing branch), M3 (binding
  `EnvironmentBehaviorView.body_state_delta`)
- **What happened**: `#[non_exhaustive]` blocks struct-literal construction
  outside the defining crate, which is the point. Three public types combine it
  with no constructor of any kind, and each loses something specific:

  1. **`TreeViolation`** — `validate_tree` is documented as "the runtime
     validator for frameworks accepting rebuilt or spliced trees (**FFI
     included**)", which is this binding's exact use, and a downstream consumer
     cannot construct a single test case for its failing branch.
     `NodeTreeBuilder::finish` rejects violating trees (techy's own tests say
     so: "the only way to construct all-trees-law violations, since `finish()`
     rejects them" — via `TreeCore`/`NodeData`, both `pub(crate)`), and
     `TreeViolation` has no inherent impl block at all. **The binding's whole
     `TreeViolation → exception` path therefore ships with zero end-to-end
     coverage.** A validator whose failure branch is unreachable from outside is
     a validator nobody can prove they handle correctly.
  2. **`EnvironmentInvocation`** — six `pub` fields, `Copy` data and two
     `&str`s, "built only by the composition" as its docs say. That is right for
     the *parse* direction, but it also means an embedder can never call
     `behavior.body_state_delta(invocation)` itself: not to test a behaviour it
     wrote, not to inspect what a registered environment would do, not to bind
     the method at all. There is nothing in the record to protect.
  3. **`MalformedBegin`** — a public unit struct marked `#[non_exhaustive]`
     *and* `#[diagnostic(…, no_constructor)]`, so no consumer can ever build
     one: not for a test fixture, not to report the condition from its own
     construct parser. It is the **only shipped condition in that state** — the
     other seven `no_constructor` sites in the crate are all test fixtures — so
     it reads as accidental. (`no_constructor` presumably reads as "a unit
     struct needs no constructor", which is true *inside* the crate only.)
- **Wish**: `pub fn TreeViolation::new(node: Option<NodeId>, kind: TreeViolationKind)`;
  `EnvironmentInvocation::new(trigger_span, name, name_span, escape_char,
  name_group_open, name_group_close)`; and drop `no_constructor` from
  `MalformedBegin` so the derive emits its `new()` like every other shipped
  condition. `#[non_exhaustive]` plus a constructor keeps the field-growth
  freedom the docs want and costs nothing. For `TreeViolation` a `test-support`
  cargo feature exposing the hand-tree builder techy's own tests use would do as
  well, and would serve the invariant-checker request elsewhere in this note.
- **Workaround taken**: `attach_violation()` split out in `src/nodes.rs` so a
  Rust test can pin the exception's carrier protocol, with the
  `TreeViolation → exception` path itself untested;
  `EnvironmentBehaviorView` exposes `arguments` only and says why on the class,
  while the *Python* `EnvironmentInvocation` record is constructible (it is an
  owned copy) so a Python behaviour class stays unit-testable; the
  `condition_classes!` macro in `src/conditions.rs` has a `no_new` arm and the
  `MalformedBegin` class raises a `TypeError` explaining that only a parse can
  produce one.
- **Severity**: **major** — one of the three leaves a validator techy
  specifically recommends to FFI embedders with an untestable failure path, and
  all three are one `pub fn new` away.

> **Merged M8** from four entries filed in M1, M2, M3 and M4. The M2
> `TreeViolation` entry had already noticed: "Same shape as the
> `#[diagnostic(no_constructor)]` entry above."
>
> ~~**`StrayGroupClose` (and its five siblings) cannot be constructed outside
> techy**~~ — **struck M8, the claim is false.** The M4 entry held that the
> `#[non_exhaustive]` condition structs in `constructs/nodes_parser.rs` had no
> constructor, and concluded that `Language::parse_source`'s root content loop
> was therefore un-reimplementable over the public `ParseContext` API. Checked
> against techy's source at M8: there are **five** such structs, not six
> (`UnresolvableCommand` `:103`, `CommandResolutionFailed` `:140`,
> `ExpressionCallableRequiresContent` `:174`, `UnusableRecoveryToken` `:187`,
> `StrayGroupClose` `:335`; the miscounted sixth,
> `UnusableRecoveryTokenKind` `:197`, is a field enum, not a condition) — and
> **all five have public `new()` constructors**, generated by
> `#[derive(DiagnosticInfo)]`, which emits one unless `no_constructor` is given
> (`techy-derive/src/diagnostic_info.rs:306-327`). `#[non_exhaustive]` blocks
> the struct literal; the derived `new()` is the sanctioned door and it is
> there. The entry's conclusion is withdrawn: the root loop *is*
> re-implementable, and `MalformedBegin` above is the genuine instance of what
> that entry described. The entry is struck rather than deleted because
> `API_MAPPING.md` and the M4 record both trace through it.


---

## Moderate

Cost real work — a re-implementation, a mirrored code path, a parallel data
structure, an hour of source-diving — or produced a worse Python surface that
is still honest.

### `Language::parse`'s "anonymous source" is a positional-correlation trap, documented only in the guide

- **Kind**: doc gap
- **Where**: `techy::engine::Language::parse` (`src/engine/language.rs:101–105`)
  vs `docs/ai-guide.md` § Pitfalls index ("`parse()` mints a fresh anonymous
  source per call — positions from two calls never correlate")
- **Found while**: M4, writing the Python counterpart of the everyday-flow
  documentation and deciding what `techy.parse` must warn about
- **What happened**: `Language::parse`'s own rustdoc says it parses "`content` as
  an anonymous in-memory `Source`" and points at `parse_source` for "a
  pre-minted source carrying origin or provenance". Both statements are true and
  neither states the **consequence**: because `SourceSpan` equality is source
  *identity* plus range, a span from one `parse` call can never compare equal to
  a span from another, even for byte-identical text. That is a silent wrong
  answer — the comparison does not fail, it returns `false` — and it is the kind
  of thing a reader hits after re-parsing an edited document and diffing
  positions.

  The rule *is* written down, in the AI guide's pitfalls index, pointing at
  `ai-guide-embedding`. But the AI guide's own preamble says "every rule stated
  here is documented in full on the linked API item", and here the API item is
  the one place it is not. A reader who arrives through rustdoc — which is every
  reader of `Language`, and every binding author — does not meet it.
- **Wish**: two sentences on `Language::parse`, next to the existing
  `parse_source` pointer: that each call mints a *new* source identity, that
  spans across two calls therefore never compare equal, and that holding one
  `Arc<Source>` and calling `parse_source` is what makes positions correlate.
  The same paragraph would serve `Source::new`, which mints the identity.
- **Workaround taken**: stated in three places on the Python side, because a
  binding cannot fix it upstream — the `techy` module docstring (as one of its
  four contracts), `techy.parse`'s docstring, and `techy.core`'s module
  docstring. Pinned by
  `tests/test_engine.py::test_spans_from_two_parse_calls_never_compare_equal`.
- **Severity**: moderate — a silent wrong answer — the comparison does not
  fail, it returns `False` — and the rule is written only in the AI guide,
  which the guide's own preamble says should never happen


### A `None` from any `LineColProvider` renders as "line-index scan limit exceeded"

- **Kind**: contract risk
- **Where**: `techy::error::format_position_with` (`techy/src/error.rs:851-856`);
  contract stated on `techy::source::LineColProvider` (`source/line_index.rs:236-240`)
- **Found while**: M1 review, exercising the `LineColProvider` adapter's
  no-answer path from Python
- **What happened**: `LineColProvider` documents `None` as "no answer available"
  from *any* implementation, for *any* reason — the trait's whole point is that
  "whether they come from a cache, a precomputed table, or a scan is the
  implementation's business". But `format_position_with` renders every `None` as
  `@ char pos N (no line info: line-index scan limit exceeded)`, attributing it
  to one specific cause of one specific implementation. Every consumer-supplied
  provider (an editor's incremental line table, a binding adapter) that answers
  `None` therefore produces a rendered diagnostic that states something factually
  untrue about the document. It also misfires for the shipped `LineIndexCache`
  when the *offset* is out of range rather than the content over the cap — the
  same `None`, the same wrong sentence. The wrongness is silent: the report looks
  authoritative, and the only clue is a cap the caller never set.
- **Wish**: render the cause-neutral form the trait actually promises —
  `@ char pos N (no line info)` — and, if the scan-cap case deserves its own
  wording, let `LineIndexCache` say so through a channel the renderer can see
  (a second method, or a `LineColAnswer` with a `NotIndexed` arm). The one-line
  version of this is just deleting the six words after the colon.
- **Workaround taken**: none available to the binding — the string is produced
  inside techy. `techy.source.LineColProvider`'s Python docstring warns that
  `None` means "no answer" and the caller falls back to byte positions
  (`python/techy/source.py:118-128`), but the rendered text still contradicts it;
  the binding's adapter reaches this path whenever a Python provider raises
  (`src/source.rs:1188-1210`, exception to `sys.unraisablehook`, query answers
  `None`).
- **Severity**: moderate — every consumer-supplied provider that answers `None`
  produces a rendered diagnostic stating something factually untrue, and no
  consumer can fix it — the string is produced inside techy


### `GroupRule`'s `Arc` identity is load-bearing, but nothing on `GroupRule` says so

- **Kind**: contract risk
- **Where**: `techy::core::GroupRule`, `techy::core::TokenRules::temporary_groups`,
  `techy::core::ParsingState::derived`
- **Found while**: M3, binding the temporary-group lifecycle
- **What happened**: `GroupRule` derives `PartialEq`/`Eq` (structural: class +
  `open` + `close`), and `TokenRules::temporary_groups` is a `Vec<Arc<GroupRule<L>>>`
  — so a reader naturally assumes rules are compared by value. They are not, in the
  two places it decides behaviour: `derived()` keeps the temporaries only when the
  installed `expecting_group_close` **is one of them by `Arc::ptr_eq`**, and the
  `PrefixTable` is reused only when the group lists match elementwise by
  `Arc::ptr_eq`. Both rules are stated — clearly — but only in
  `ParsingState::derived`'s "Temporary group rules" section and in the field's own
  doc. `GroupRule`'s type-level docs say the rules are "held behind `Arc` in
  `TokenRules::groups`" for a *different* reason (the tokenizer's resolution travels
  with the token), which actively points away from the identity rule. The
  consequence is a silent one: two `GroupRule`s that compare `==` behave differently
  in a derivation, and `rules.temporary_groups.contains(&rule)` — the obvious check
  — answers the wrong question. techy's own test suite pins the behaviour
  (`derived_scopes_temporary_group_rules`), so this is a documentation-placement
  issue, not a bug.
- **Wish**: one paragraph on `GroupRule` itself — "**identity matters**: two equal
  rules are not interchangeable; `ParsingState::derived` keys the temporary-group
  scope and the `PrefixTable` reuse on `Arc::ptr_eq`, so clone the `Arc`, never the
  value" — plus a cross-reference from `TokenRules::groups`. A `#[must_use]`-style
  nudge is not available, but the sentence costs nothing and lands where a reader
  looking at a rule will see it.
- **Workaround taken**: `techy.core.GroupRule` documents the trap on the class,
  ships a `same_rule(other)` predicate (`Arc::ptr_eq`) so the question the
  derivation asks is askable from Python at all, and
  `tests/test_state.py::test_an_equal_but_distinct_rule_strips_the_temporaries`
  asserts both directions — including that `in` (which uses `==`) says yes where
  the derivation says no.
- **Severity**: moderate — two rules that compare `==` behave differently in a
  derivation and `temporary_groups.contains(&rule)` answers the wrong question
  — a silent wrong answer from the obvious code


### `check_include_chain`'s error makes the resulting diagnostic name the resolved origin, never the reference the user wrote

- **Kind**: design friction (diagnostic quality)
- **Where**: `techy::source::check_include_chain` +
  `techy::constructs::UnresolvableSourceReference`
  (`#[diagnostic(message = "{error}")]`)
- **Found while**: M6, driving `\input` cycles and depth bounds through a parse
- **What happened**: `check_include_chain` builds its `ResolveError` with
  `source.origin().label()` as the error's `reference` — documented, and correct
  for the error in isolation (the target key type `K` need not render itself).
  But `UnresolvableSourceReference` renders its message as `"{error}"`, and
  `ResolveError`'s `Display` is
  `cannot resolve source reference '{reference}': {message}` — the *error's*
  reference. So for the ready-made recipe the user sees

  ```
  cannot resolve source reference '/abs/path/to/book/a.tex':
      include cycle detected: this source is already on its own include chain
  ```

  for a document that says `\input{a}`. The condition **already carries the
  right string** — `UnresolvableSourceReference.reference` is documented as "the
  reference that failed to resolve, as written" and holds `"a"` — it is simply
  not the one rendered. Two different "references" in one diagnostic, and the
  one users read is the one they did not write.

  It bites hardest on exactly the path techy recommends: the docs tell resolvers
  to mint canonical origins so the chain check can compare them, which is what
  makes the rendered label an absolute path.
- **Wish**: render the condition's own field —
  `message = "cannot resolve source reference '{reference}': {error.message}"`,
  or have `attach_source_reference` rewrite the error's reference to the one it
  was called with. The note "a resolver wanting its own reference spelling maps
  the error before returning it" puts the fix on every resolver, when the
  construct that has both strings could do it once.
- **Workaround taken**: none in the binding — the structured field is correct,
  so `diagnostic.data.reference` is what tests and tooling use
  (`tests/test_source.py`, §11). Only the rendered message is affected.
- **Severity**: moderate — it is one format string, and it is the first thing
  an author reads when an include fails


### The reemit oracle cannot falsify the doctrine it exists to prove — 38 of its 38 equality inputs also pass under a span shortcut

- **Kind**: contract risk
- **Where**: `techy/tests/recompose_oracle.rs` (the strict and tolerant
  matrices), against the reading contract on
  [`techy::recompose::Recomposer`](crate::recompose::Recomposer) ("no span
  content is ever resolved; no inter-node span arithmetic")
- **Found while**: M5, porting the oracle and then trying to add a test that
  makes its headline claim *observable* rather than true-by-inspection
- **What happened**: the oracle's module doc states the claim precisely —
  byte-exact reemission "from recorded payload alone … with no span crutch: the
  recomposer never resolves span content". But every input in the equality
  matrices is a **fresh parse of a whole document**, and a parse tree's node
  spans tile the source exactly. So a hypothetical `SourceRecomposer` that took
  the span shortcut instead of reading `invocation_syntax` would satisfy every
  one of them.

  Measured, not assumed. A deliberate falsifier — lists and groups fold as
  usual, every **callable** answers `Recompose::emit(node.span_content())`
  instead of reading its recorded payload — was run over all 38 `assert_reemit`
  / `assert_reemit_tolerant` inputs of the port:

  ```
  span-shortcut recomposer, equality-matrix failures: 0 of 38
  ```

  Zero. The macros with post-space, the recorded-spacing environments, the
  verbatim bodies, the specials, both paragraph-break styles, all seven
  tolerant recoveries — none of them discriminate. The multi-source matrix
  does not discriminate either: an `\input` node's own span covers exactly the
  `\input{…}` invocation, so the shortcut answers the includer's bytes too.

  Exactly **one** test in the file catches it —
  `tolerant_malformed_terminator_elides_the_consumed_end_spelling`, where the
  shortcut answers `"\begin{A}x\end y"` against the oracle's `"\begin{A}xy"` —
  and it catches it *incidentally*, as a side effect of pinning the S5 flag,
  not because it was aimed at the doctrine. Delete or relax that one test (it is
  the one test in the file that is not an equality, so it is also the one most
  likely to be "simplified" by a future refactor) and the entire oracle goes
  green against an implementation that violates the contract it is named for.

  This matters beyond the test file, because the doctrine's *whole* motivation
  is the transformed tree — techy's own docs say it: "on any transformed tree
  the apparent gap between two surviving siblings is exactly the deleted
  content". The oracle contains no transformed tree.
- **Wish**: one more test in `recompose_oracle.rs`, in the doctrine's own terms:
  parse an input, `restage` it dropping a middle node, assert the surviving
  siblings' spans **no longer tile**, and assert the reemission is the survivors
  and nothing between them. It is about fifteen lines and it turns the file from
  "reemission is byte-exact" into "reemission is byte-exact *and* it is not
  reading spans to do it" — which is what the module doc already claims.
- **Workaround taken**: written on the Python side as
  `tests/test_recompose_oracle.py::test_a_recomposer_does_no_inter_node_span_arithmetic`
  — parse `"A% secret\n\begin{itemize}mid\end{itemize}B"`, prune the comment and
  the environment, assert the two survivors' spans are `range(0, 1)` and
  `range(41, 42)` (a 40-byte gap), assert the reemission is exactly `"AB"`, and
  repeat on the `materialize()`d tree where there is no `Source` behind the
  nodes at all. Portable to Rust essentially unchanged.
- **Severity**: moderate — measured, not asserted: 0 of 38 equality inputs
  discriminate, so the file could go green against an implementation that
  violates the contract it is named for. The fix is about fifteen lines in
  techy's own suite


### The specials pair and the specials precedence order are both silent, and both are checkable at registration

- **Kind**: contract risk
- **Where**: `techy::core::Lang::scan_specials` /
  `Lang::specials_trigger_chars`, and the reader's recognition order
- **Found while**: M7, building the double-hook guard
- **What happened**: techy documents two failures of specials recognition and
  calls both silent. (a) A trigger character missing from
  `specials_trigger_chars` means `scan_specials` is never consulted there — no
  error, no diagnostic — and because the set is cached on the frozen state, the
  scan hook is not *slow*, it is *dead*. (b) Specials have the lowest
  recognition precedence, so a trigger overlapping a group delimiter, a command
  escape or a comment start also never fires. Both are knowable without running a
  parse: (a) from the two hooks' presence, (b) from the seed's `TokenRules`.
- **Wish**: a `debug_assert!` or a parse-start check for (b) —
  `LatexlikeLang::check_parse_start` is already the layering-correct place, it
  already has the seed's `TokenRules` and the state's `trigger_chars`, and it
  already records warnings for exactly this class of registration mistake
  (`check_provider_commands_shadowed_by_escape` is its one current tenant). For
  (a), nothing in the trait can enforce a pairing between two defaulted methods,
  but the doc could say plainly that overriding one without the other is always a
  bug.
- **Workaround taken**: the binding raises at attachment time for (a) — in
  **both** directions, since a chars hook with no scan hook is provably a pure
  slowdown — and emits a `UserWarning` at `Language` construction for (b), naming
  each shadowed character and the rule that beats it. Warn rather than raise for
  (b) because token rules are data any delta may change mid-parse, so a trigger
  shadowed in the seed may be free two derivations later (`PROGRESS.md` D333).
- **Severity**: moderate — a silent failure that the library can see coming is
  the one kind worth spending an assert on

> **Hit again, M7 review follow-up — a third silent case, and the cheapest one
> to reach.** (a) above is about a trigger character *missing* from the set. The
> set can also be **empty**: a `Lang` that overrides both members but whose
> `specials_trigger_chars` answers `TriggerChars::Only("")` has a scan hook that
> can never run — for any input, at any position — and techy says nothing. This
> is not hypothetical in a binding: the Python ABC's own default body is
> `return None`, and `None` means "none of them", so the mistake is one missing
> `return` away. Measured shipping silently in M7: 0 `scan_specials` calls, no
> error, no diagnostic, not even under `warnings.simplefilter("error")`. It is
> arguably sharper than (b) — a shadowed character disables one trigger, an
> empty set disables the whole hook. **Wish, for this third case**: a
> `TriggerChars::None` variant distinct from `Only("")` would make "I have no
> triggers" expressible and the check trivial; failing that, the same
> parse-start check proposed for (b) can test the empty case in the same place,
> since the frozen state's `trigger_chars` is already in hand there.
> **Workaround**: the binding warns at `Language` construction
> (`src/engine.rs::warn_about_a_scan_hook_that_can_never_fire`, `PROGRESS.md`
> D343) — a `UserWarning` rather than a refusal for the same reason (b) warns:
> the answer is a fact about *one state*, and a delta may add characters later.
> Only the seed is checked, which is the same residual (b) has.


### `ClosedVocabulary::ALL` is all-or-nothing, so a runtime-extensible language gets a silently partial safety check

- **Kind**: contract risk
- **Where**: `techy::core::ClosedVocabulary` (an associated
  `const ALL: &'static [Self]`) and its one consumer,
  `techy::core::specs::check_provider_commands_shadowed_by_escape`
  (`scopes/mod.rs`)
- **Found while**: M1 (implementing the runtime-interned vocabularies of
  `PyLatexlike`), M7 (landing `CallableType.register()` — where the M1
  prediction was **measured**)
- **What happened**: a binding, or any language whose vocabulary is
  registry-backed, can enumerate its *reserved* members but not the
  runtime-registered ones, because `ALL` is an associated const. The trait is
  documented as opt-in ("provide, don't require"), and the guidance for a
  non-enumerable language is "keep the default and the check is gracefully
  absent" — but there is no guidance for the **partial** case, which is the one
  a dynamic language actually lands in.

  M1 implemented `ALL` with the reserved values only, and recorded that this is
  silently partial. The least-bad choice was the one that quietly lies: skipping
  the impl entirely would have lost the check for the presets too.

  **M7 measured it.** `check_provider_commands_shadowed_by_escape` is one of two
  defences against the registration trap, and the other one
  (`resolve_command_in_scopes`' did-you-mean detail) is muted by an in-stack
  fallback provider — which is the documented reason this one exists. Because it
  enumerates `ALL`, the *identical* shadowed definition table warns under the
  macro callable type and is **silent** under a registered one. Nothing in the
  function's rustdoc mentions the enumeration's completeness as a precondition,
  and nothing in techy can tell it is looking at an incomplete answer.
- **Wish**: a documented answer for open vocabularies, at either level.
  * On the trait: a sentence saying a partial `ALL` is legal and naming the
    consequence ("consumers treat `ALL` as best-effort"), or a defaulted
    `fn is_exhaustive() -> bool { true }` so a consumer can degrade **loudly**
    — the parse-start check could then say "checked the 3 built-in callable
    types only".
  * On the function: either take the callable types to check as a parameter
    (defaulting to `ALL` behind the existing `ClosedVocabulary` bound), or state
    the precondition — "providers are checked for the callable types
    `L::CallableTypeId::ALL` lists; a language whose vocabulary is not closed
    gets a partial check".
- **Workaround taken**: `impl ClosedVocabulary` over the reserved ids in
  `src/lang.rs`, with the incompleteness documented on the Python
  `CallableType.ALL` docstring, on
  `techy.core.specs.check_provider_commands_shadowed_by_escape`'s own docstring
  and in `python/techy/latexlike.py`'s module docstring — Python users are the
  ones who will register new forms. **Measured** rather than assumed by
  `tests/test_custom_lang.py::test_a_registered_callable_type_is_skipped_by_the_escape_check`
  (`PROGRESS.md` D256, D336).
- **Severity**: moderate — a partial defence that reads as total is worse than a
  documented partial one, and every runtime-extensible language built on techy
  lands in exactly this case. The check itself is a diagnostics nicety, which is
  why this is not major.

> **Merged M8** from two entries, six milestones apart: M1 predicted the silent
> partial check from the shape of the const, and M7 measured it firing for one
> callable type and not another on the same definition table. Keeping them
> together is the point — the prediction and its confirmation are one finding.


### `LineIndex<'c>` borrows its content, so no owning consumer can hold one

- **Kind**: design friction
- **Where**: `techy::source::LineIndex`, `techy::source::Source::line_index`
- **Found while**: M1, binding `techy.source.LineIndex`
- **What happened**: `LineIndex<'c>` is the documented "transient view" and
  `LineIndexCache` the "persistent, consumer-held" form, so on paper the pair
  covers both needs. But a *binding* needs a persistent object with `LineIndex`'s
  API: `line_index.line_col(off)`, not `cache.line_col(&source, off)` — the
  Python object `source.line_index()` returns must outlive the call that made it.
  A `LineIndex<'c>` cannot be stored in a `#[pyclass]` (no lifetimes there), and
  there is no owning variant, so the binding's `LineIndex` is *not* a
  `LineIndex`: it is a one-entry `LineIndexCache` plus the source, with the three
  queries forwarded. That works because techy's own `cache_agrees_with_the_fresh_index`
  test pins the two to identical answers, but it silently gives up the
  incremental laziness that is `LineIndex`'s reason for existing, and the
  equivalence is an undocumented invariant the binding now depends on.
- **Wish**: either (a) an owning constructor — `LineIndex::owned(Arc<Source<O>>)`
  or a `LineIndex<S: AsRef<str>>` — so the lazy form is reachable from an owning
  consumer, or (b) one sentence on `LineIndexCache` stating outright that a
  single-source cache is the sanctioned owning replacement for `LineIndex` and
  answers identically. (b) costs nothing and would have removed the doubt.
- **Workaround taken**: `PyLineIndex` in `src/source.rs` holds
  `{ source: Arc<Source>, cache: LineIndexCache }`; the substitution is
  documented on the class.
- **Severity**: moderate — the binding's `LineIndex` is not a `LineIndex` — it
  gives up the incremental laziness the type exists for, and rests on an
  equivalence techy never states


### `Lang::SessionExt` is write-only: nothing survives `ParserSession::finish`

- **Kind**: API asymmetry
- **Where**: `techy::core::ParserSession::ext` / `ParserSession::finish` /
  `techy::core::ParseResult` / `ParseDriver::observe_transition`
- **Found while**: M1, deciding whether `PyLatexlike::SessionExt` can carry
  anything useful
- **What happened**: `observe_transition`'s docs are emphatic about *where*
  parse-history accumulation belongs — "'how many times did the parse enter
  math mode' belongs here, in the session's `SessionExt` — never in
  `finalize_transition`". So we wired a counter. Then we looked for the read
  side and there is none: `ParserSession::finish(self, root)` builds
  `ParseResult { tree, diagnostics }` and drops `self.ext` on the floor, and
  `Language::parse_source` owns the only session a normal parse ever has. The
  documented purpose of the extension point is therefore unreachable from
  outside the parse; the only consumers are in-parse construct parsers reading
  it back through `cx.session.ext`. It took a read of `language.rs` +
  `engine/mod.rs` to be sure it was not just undocumented.
- **Wish**: either a third field `pub ext: L::SessionExt` on `ParseResult` (a
  one-line change in `finish`, and the type is already `Send + Sync`), or one
  sentence on `Lang::SessionExt` and on `observe_transition` saying the ext is
  parse-scoped and deliberately not returned — so a reader knows the
  accumulation is for in-parse consumers only. The example in the
  `observe_transition` docs currently reads as if the count were a result.
- **Workaround taken**: `PySessionExt::transitions` is bumped by
  `PyDriver::observe_transition` (`src/lang.rs`) and asserted by a Rust test
  that calls the hook directly; the Python `ParseResult` will not expose it in
  M4 because it cannot.
- **Severity**: moderate — the documented purpose of a public extension point
  is unreachable from outside a parse, so the Python `ParseResult` cannot carry
  it at all


### `ParseDriver`'s defaults call each other, and there is no wrapper-driver recipe

- **Kind**: contract risk
- **Where**: `techy::core::ParseDriver` (`recover` → `refine_diagnostic` +
  `recovery`; `make_invocation_parser` → `CallableSpec::make_invocation_parser`)
- **Found while**: M1, promoting `PyDriver` from `type PyDriver =
  LatexlikeDriver<PyLatexlike>` to a newtype that wraps it
- **What happened**: `LatexlikeDriver`'s docs recommend composition — "a
  framework wanting different behavior for one hook writes its own `ParseDriver`
  composing the same functions" — and the cheapest form of that is a newtype
  that forwards all fourteen methods to an inner preset driver. Two hazards are
  invisible at the call site. (1) `recover`'s default body calls
  `self.refine_diagnostic(..)`, so forwarding `recover` to the inner driver
  *also* routes refinement through the inner driver; the day the wrapper
  overrides `refine_diagnostic`, its override silently never fires. (2) Symmet-
  rically, forwarding only the methods the inner driver overrides today means a
  future techy release that starts overriding a fifteenth one silently loses it.
  Both are behaviour bugs that surface far from their cause. Working out which
  defaults call which required reading the trait's bodies, not its docs.
- **Wish**: name the intra-trait calls in the per-method docs — one clause on
  `recover` ("the default calls `refine_diagnostic` and `recovery` **through
  `self`**") and on `make_invocation_parser` — plus a short "wrapping a driver"
  paragraph on the trait page stating that a delegating driver must forward
  every method, including the defaulted ones, and re-dispatch through `self`
  wherever it overrides a callee.
- **Workaround taken**: `PyDriver` (`src/lang.rs`) forwards all fourteen
  explicitly, carries the hazard as a doc block on the struct, and is pinned by
  `pydriver_matches_the_preset_driver`.
- **Severity**: moderate — two silent behaviour bugs, each of which surfaces
  far from its cause; the binding pays fourteen forwarding methods plus a
  pinning test to be sure of neither


### No enumerable roster of the shipped condition types

- **Kind**: doc gap
- **Where**: `techy::error::DiagnosticInfo` ("The condition-type roster is the
  implementors listing on `DiagnosticInfo`", `docs/ai-guide.md` § Handle
  diagnostics)
- **Found while**: M1, generating `techy.conditions` — one Python class per
  shipped condition
- **What happened**: the AI guide points at the rustdoc *implementors listing*
  as the roster. That listing does not exist outside a built rustdoc, and it
  does not show each type's `IDENTIFIER` (a const, rendered only on the type's
  own page). To get the 25 shipped conditions with their identifiers I wrote a
  script that walks `techy/src/**/*.rs`, finds every `#[diagnostic(` attribute,
  and pairs it with the following `struct` — and then had to hand-filter the
  eight private/test ones the grep also matches: **33 attributes, 25 public**.
  Three names appear *twice*, once shipped and once as a private test fixture —
  `MalformedBegin` and `UnknownEnvironment` (public in
  `latexlike::environments`, fixtures in `constructs::environment_parser`) and
  `StrayGroupClose` (public at `constructs/nodes_parser.rs:335`, fixture at
  `:3193` in the same file's `#[cfg(test)]` module). About 40 minutes, and the
  result is unverifiable against anything. *(Re-counted at M8 against techy's
  source: still 33 and 25, and still no `SHIPPED_CONDITIONS` const and no
  `docs/` page carrying a single condition identifier.)*
- **Wish**: a documentation page (or a module-level table on `techy::error`)
  listing every shipped condition as `identifier — type — recovery`, in one
  place. It is the single most consumer-facing table in the crate: config files,
  linter allowlists, editor integrations and bindings all need exactly it. A
  `techy::error::SHIPPED_CONDITIONS: &[(&str, &str)]` const (identifier, type
  name) would additionally let downstream tests assert they cover the roster.
- **Workaround taken**: the roster is transcribed by hand into the
  `condition_classes!` invocation in `src/conditions.rs`, and pinned twice —
  a Rust test asserts the count is 25, and `tests/test_conditions.py` carries
  the identifier table so an upstream identifier change fails loudly.
- **Severity**: moderate — about 40 minutes and a hand transcription with no
  oracle; the roster is the most consumer-facing table in the crate and it
  exists only as a rustdoc implementors listing


### `Diagnostic` cannot be assembled from parts, and has no `with_frames`

- **Kind**: API asymmetry
- **Where**: `techy::error::Diagnostic::from_parts` (`pub(crate)`) vs
  `techy::error::ParseError::with_frames` (public)
- **Found while**: M1, binding `Diagnostic` and converting a `ParseError` into a
  diagnostic-shaped carrier
- **What happened**: `ParseError` can be built and then given a traceback
  (`with_frames`, documented for "the direct-abort sites… and custom parser
  code"). `Diagnostic` has the same three parts and the same `from_parts`
  constructor, but `from_parts` is crate-private and there is no `with_frames`
  twin — so a consumer holding a `Box<dyn DiagnosticData>` and a
  `Vec<TraceFrame>` (both public types, both obtainable from any diagnostic via
  `data().clone_box()` and `frames().to_vec()`) cannot rebuild a `Diagnostic`,
  and a consumer-built one can never carry a traceback. Re-creating a diagnostic
  is exactly what a binding, a diagnostic filter or a report merger does.
- **Wish**: `Diagnostic::with_frames(self, frames: Vec<TraceFrame<O>>) -> Self`,
  mirroring `ParseError`'s; optionally make `from_parts` public alongside it.
- **Workaround taken**: `techy.error.Diagnostic(severity, condition, span)` takes
  no `frames` (documented on the constructor), and the Python `ParseError` keeps
  the real `techy::error::ParseError` in a private `_ErrorReport` carrier so
  `error.render()` / `error.render_with()` stay faithful instead of going through
  a lossy `Diagnostic` (`src/errors.rs`).
- **Severity**: moderate — the Python `Diagnostic` constructor can never carry
  a traceback, and the binding keeps a private `ParseError` carrier to avoid a
  lossy round trip


### The diagnostics retention cap cannot be set for a parse

- **Kind**: missing feature
- **Where**: `techy::core::Language::parse` / `parse_source`;
  `techy::error::Diagnostics::with_limit`; `ParserSession::new`
- **Found while**: M1, exposing `Diagnostics.limit` / `.suppressed` to Python
- **What happened**: `Diagnostics::with_limit` exists and the type documents the
  cap carefully, but `ParserSession::new()` hard-codes `Diagnostics::new()` and
  `Language::parse_source` builds its own session — so the `DEFAULT_LIMIT` of
  1000 is the only cap a parse can ever have. An editor integration that wants
  20, or a batch linter that wants everything, has no knob. (The `Recovery`
  policy is on the driver, which is where this belongs too.)
- **Wish**: a `diagnostics_limit` on the driver (next to `recovery`), or a
  `Language::parse_source_with(source, Diagnostics)` overload.
- **Workaround taken**: none exposed; a Python caller that wants a different cap
  builds `techy.error.Diagnostics(limit=n)` and re-`push`es, which re-runs the
  cap logic correctly but costs a pass.
- **Severity**: moderate — a documented, tested knob
  (`Diagnostics::with_limit`) is unreachable for the only code path that
  produces diagnostics in bulk


### `TreeViolationKind` has no `as_str()`, unlike `NodeKind`

- **Kind**: API asymmetry
- **Where**: `techy::core::node::TreeViolationKind` vs `NodeKind::as_str`
- **Found while**: M2, mapping the fifteen variants onto a Python enum
- **What happened**: `NodeKind::as_str()` gives a stable discriminant name, which is
  exactly what a binding needs to map a closed enum across the boundary. The
  `#[non_exhaustive]` `TreeViolationKind` — the enum where forward compatibility
  actually matters — has no such thing, so the mapping is a fifteen-arm `match` plus
  a wildcard, and a variant techy adds later arrives silently as the binding's
  `UNKNOWN` rather than as a name a caller could still log or dispatch on.
- **Wish**: `pub const fn TreeViolationKind::as_str(&self) -> &'static str`, matching
  `NodeKind::as_str`. With it, a binding can pass an unmapped variant through by name
  instead of erasing it.
- **Workaround taken**: fifteen-arm match plus an `UNKNOWN` arm in
  `src/nodes.rs`, pinned by `tree_violation_kind_maps_every_shipped_variant`
  so a *rename* is a compile error.
- **Severity**: moderate — a variant techy adds later arrives as the binding's
  `UNKNOWN` rather than as a name a caller could log or dispatch on, on the one
  enum where `#[non_exhaustive]` says forward compatibility was the point


### The parse-law oracle is `#[cfg(test)] pub(crate)`, so no consumer can assert it

- **Kind**: missing feature
- **Where**: `techy::core::node::invariants::check_tree_invariants` (and
  `latexlike::check_latexlike_tree_invariants`, `latexlike::test_support`)
- **Found while**: M2, writing the test that sibling spans partition a parent's
  content interior exactly
- **What happened**: the span-partition invariant is techy's central promise about
  parsed trees, and techy has a ready-made checker for it — behind `#[cfg(test)]`, so
  a binding that wants to assert "my tree still tiles" has to reimplement it
  (interior = whole span for `List`, span minus the *recorded* delimiters for `Group`;
  the delimiters detail is easy to get wrong and is only stated in a doc comment on a
  private function).
- **Wish**: a `test-support` cargo feature exporting `check_tree_invariants` and
  `check_latexlike_tree_invariants`. Every embedder that builds trees — restage
  consumers above all — wants exactly this assertion in its own test suite.
- **Workaround taken**: `interior()` / `assert_children_tile()` in
  `tests/test_nodes.py`.
- **Severity**: moderate — the crate's central promise about parsed trees is
  checkable only inside the crate, so every embedder that builds trees
  reimplements the checker — including the delimiters detail that is easy to
  get wrong


### `NamedAccessError` is `#[non_exhaustive]` but has no accessors for its own facts

- **Kind**: API asymmetry
- **Where**: `techy::core::node::NamedAccessError`
- **Found while**: M2, mapping the by-name accessors onto `techy.error.UnknownArgumentName`
- **What happened**: an FFI layer has to answer two questions about the error —
  *which* category it is, and *which* name missed — so it can attach a
  machine-readable `.reason`/`.name` to the Python exception instead of making
  callers string-match the message. Both facts are only reachable by matching
  the enum arms and reading their struct fields, and the type is
  `#[non_exhaustive]`, so the required wildcard arm can answer *neither*: a
  techy release that adds an arm silently degrades the binding to a generic
  reason with no name.
- **Wish**: two inherent methods — `fn name(&self) -> Option<&str>` and a
  discriminant accessor (`fn kind(&self) -> NamedAccessErrorKind`, or simply
  `fn is_not_a_callable(&self) -> bool` + `name()`). Then a binding maps the
  error mechanically and a new arm degrades gracefully instead of blindly.
- **Workaround taken**: `named_access_error()` in `src/node_data.rs` matches the
  three arms plus a wildcard that reports `reason = "named_access_error"`.
- **Severity**: moderate — the required wildcard arm can answer neither of the
  error's two facts, so a new arm silently degrades the binding to a reason
  with no name


### `TokenRulesOverrides::expecting_group_close` is `Option<Option<_>>`, and its third state has a consequence documented elsewhere

- **Kind**: design friction
- **Where**: `techy::core::TokenRulesOverrides::expecting_group_close`
- **Found while**: M3, binding `TokenRulesOverrides`
- **What happened**: twelve of the thirteen override fields are `Option<T>` with the
  uniform meaning "`None` = leave unchanged, `Some(v)` = replace". The thirteenth is
  `Option<Option<Arc<GroupRule<L>>>>` with three meanings, and its doc line is
  "Override the expected group close (`Some(None)` clears it)". Two costs. First,
  the target language has one `None`: Python cannot spell both "leave unchanged" and
  "clear", so the binding had to invent a sentinel and teach it. Second — and this
  is the part that cost the time — the *behavioural* difference between the two
  `Some` shapes is not on the field: clearing the expectation ends the
  temporary-group scope exactly as installing a foreign rule does, which is stated
  only in `ParsingState::derived`'s "Temporary group rules" section. A reader of
  the field's one-liner would reasonably think `Some(None)` is the inert choice.
- **Wish**: either (a) a small named enum — `ExpectedClose::{Keep, Clear,
  Install(Arc<GroupRule<L>>)}` — which makes the three states self-documenting and
  gives every binding a natural translation, or (b) two extra sentences on the
  field: what `Some(None)` means for `temporary_groups`, and a pointer to
  `ParsingState::derived`.
- **Workaround taken**: `techy.core.CLEAR`, a module-level sentinel documented on
  the property and on the module, used by both tri-state fields
  (`TokenRulesOverrides.expecting_group_close` and `ParsingStateDelta.ext`);
  `tests/test_state.py::test_expecting_group_close_has_three_distinct_states` and
  `test_the_three_states_behave_differently_in_a_derivation` pin all three.
- **Severity**: moderate — the target language has one `None`, so the binding
  had to invent and teach a `CLEAR` sentinel; the behavioural difference
  between the two `Some` shapes is documented on a different page


### Deltas are documented as "mergeable, inspectable", but `merge_from` and `is_empty` are `pub(crate)` and there is no `PartialEq`

- **Kind**: API asymmetry
- **Where**: `techy::core::ParsingStateDelta::merge_from`,
  `ParsingStateDelta::is_empty`, and the absent `impl PartialEq for ParsingStateDelta`
- **Found while**: M3, binding `ParsingStateDelta`
- **What happened**: the type's own docs open with "Deltas are **values, not
  closures** — mergeable, inspectable, and propagatable to base states their
  producer never saw". Two of those three are unavailable to a consumer:
  `merge_from` (the exact "applying `self` then `later` is reproduced by applying
  the merged value once" semantics an embedder composing after-effects wants) is
  `pub(crate)`, and so is `is_empty`. `TokenRulesOverrides` — the delta's own field
  — derives `PartialEq`/`Eq`, but `ParsingStateDelta` does not, so two deltas cannot
  be compared at all; the blockers there are `ScopeOp` (holds `Arc<dyn CallableSpec>`
  / `Arc<dyn SpecsProvider>`) and `L::Event` (no `PartialEq` bound in `Lang`), both
  real, but neither is mentioned where a reader would look.
- **Wish**: make `merge_from` and `is_empty` public — the semantics are already
  written and tested, and "mergeable" is a promise the docs make. If `PartialEq`
  must stay absent, say so on the type and name the reason, the way
  `ParsingState`'s "no `Clone`" note does so well.
- **Workaround taken**: `techy.core.ParsingStateDelta` reimplements `is_empty`
  field by field, documents that its `__eq__` is object identity and points at
  `delta.rules` (which does have value equality) as the comparable part, and
  offers no `merge` at all. `tests/test_state.py::test_delta_equality_is_identity_because_techy_has_none`.
- **Severity**: moderate — two of the three properties the type's own opening
  sentence promises are `pub(crate)`, so the Python delta offers no `merge` at
  all and reimplements `is_empty`


### `ParsingStateStack` can be built from a list but not grown: `push`/`pop`/`innermost` are `pub(crate)`

- **Kind**: API asymmetry
- **Where**: `techy::core::ParsingStateStack::push`, `::pop`, `::innermost`
- **Found while**: M3, binding `ParsingStateStack`
- **What happened**: the type documents **two** producers as equals — the live
  session stack, and post-parse construction via `from_states` /
  `from_node_ancestors` — so a consumer expects the post-parse half to be a
  first-class citizen. Its public roster is `new`, `from_states`,
  `from_node_ancestors`, `iter`, `outermost`, `len`, `is_empty`: everything except
  the three mutators, which are `pub(crate)` for the session's use. The result is
  that `new()` builds an empty stack a consumer can never add to, and post-parse
  code walking *downward* through a tree (a transform synthesizing nodes as it
  descends, feeding `exit_math_context_delta` at each level) must accumulate a
  `Vec` and rebuild the whole stack with `from_states` at every step, rather than
  pushing and popping as it goes. `from_states` even reverses the vector
  internally, so the incremental machinery is right there.
- **Wish**: make `push`/`pop`/`innermost` public. The type's contract is scan
  semantics, not a private invariant — nothing about a consumer-pushed entry can
  make a first-match scan wrong, which the type docs already argue at length when
  explaining why `Arc`-equal duplicates are harmless.
- **Workaround taken**: `techy.core.ParsingStateStack` binds `ParsingStateStack()`,
  `from_states`, `from_node_ancestors`, `outermost`, `is_empty`, `len()`, indexing
  and iteration, and offers no `push`/`pop`; the docstring points at `from_states`.
- **Severity**: moderate — post-parse code walking downward must rebuild the
  whole stack with `from_states` at every step, and the Python class ships with
  no `push`/`pop`


### `InvocationSyntaxData` is span-backed, but nothing hands out the record together with its source

- **Kind**: design friction
- **Where**: `techy::latexlike::InvocationSyntaxData`,
  `techy::core::node::NodeRef::invocation_syntax`
- **Found while**: M3, binding `node.invocation_syntax` /
  `CallableData.invocation_syntax`
- **What happened**: the record's `post_space` (and `StdEnvironmentSideSyntax`'s
  `command_word` / `post_space`) are `TextContent`, which resolve only against
  the carrying node's own `Source`. `NodeRef::invocation_syntax()` hands out
  `&InvocationSyntaxData` alone; `NodeRef::post_space()` resolves internally and
  is the only reader that works. A binding that surfaces the record as a Python
  *value* — which is the natural shape, since the record is `Clone` and
  self-contained apart from the spans — therefore ships an object whose main
  field cannot be read. Every consumer needs the `(record, source)` pair, and
  nothing in the API expresses that pairing.
- **Wish**: `NodeRef::invocation_syntax_materialized(&self) -> Option<L::InvocationSyntax>`
  (a one-line call of the existing `InvocationSyntax::materialized` against
  `self.source()`), or a `NodeRef::environment_syntax_text()`-style pair of
  readers beside `post_space()`. Either would let a binding hand out a complete
  record without inventing a resolution protocol.
- **Workaround taken**: `src/latexlike.rs`'s one seam,
  `invocation_syntax_to_py(py, syntax, source)`, takes the source and
  materializes every span-backed field as it hands the record out — so the Python
  value is always complete. The source comes from `node.span().source()`, since
  `NodeRef::source()` is `pub(crate)` (see the entry below). The sourceless
  variant was removed during M3 integration: it produced a record whose
  `post_space` raised, and every caller had the source anyway.
- **Severity**: moderate — the record's main field cannot be read from the
  record alone, so the binding had to invent a `(record, source)`
  materialization seam the API does not express


### `copy_subtree_into` is `pub(crate)`, so every consumer that stages a copy re-writes it

- **Kind**: missing feature
- **Where**: `techy::node::copy_subtree_into` (`pub(crate)`), beside the public
  `techy::core::node::NodeTreeBuilder::restage_node`
- **Found while**: M5, building the anchor tree above and re-implementing
  `KeyVals::get_combined_with`
- **What happened**: `restage_node` — the level-0 primitive — is public and
  documented as "the supported route for assembling a new tree out of pieces of
  several others". The bulk operation built on it, "stage a copy of this whole
  subtree", is not. So a consumer that wants the obvious thing writes the same
  20-line recursion techy already has, *including* the non-obvious half: the
  `HashMap<NodeId, BuildId>` that translates `ContentNodes::InChildrenOf`
  content parents, which only works because the bottom-up child-first order
  populates the map before the callable that needs it. Getting that ordering
  wrong is a `ContentParentUnmapped` on documents with `\input`-style attached
  content and on nothing else — i.e. it passes every small test.
- **Wish**: `pub fn copy_subtree_into(builder, node, annotate) -> Result<BuildId,
  NodeBuildError>` exactly as it exists today, re-exported from
  `techy::core::node`. It is already written, already tested, and already the
  thing `extract` uses; the only change is the visibility keyword. `transform`'s
  `RestageContext` region ops are the *supported* route for a restage, but they
  are not reachable outside a restage callback — a builder-level copy is.
- **Workaround taken**: copied the 20 lines into `src/extract.rs`, `HashMap` and
  all, with a comment saying where they came from.
- **Severity**: moderate — the binding copied 20 lines including the
  non-obvious half (the `HashMap` that translates content parents), whose
  ordering bug passes every small test


### `KeyVals` cannot be taken apart and put back together, so `get_combined_with` is unreachable for an owner-splitting binding

- **Kind**: design friction
- **Where**: `techy::extract::KeyVals::into_tree` /
  `techy::extract::KeyVals::get_combined_with`
- **Found while**: M5, binding `KeyVals`
- **What happened**: a Python `KeyVals` must own its tree as the *binding's* tree
  object (that is what makes the annotations collectable — `PROGRESS.md` §D14),
  which means taking `into_tree()` and keeping the entry table separately. But
  `into_tree()` is documented as "the entry table is dropped — extract keys
  first", there is no `into_parts()`, and no constructor takes a tree plus a
  table back. So the moment a consumer owns the tree, `get_combined_with` — the
  one genuinely non-trivial method on the type — becomes unreachable and has to
  be re-derived from `NodeTreeBuilder` + `Source::synthesized` + the
  `copy_subtree_into` above. That is ~45 lines of re-derived semantics whose
  only oracle is techy's own test.
- **Wish**: either `KeyVals::into_parts(self) -> (NodeTree<L, B>, Vec<(Box<str>,
  Option<NodeId>)>)` with a matching constructor, or — better — hoist
  `get_combined_with`'s body into a free function over `(&NodeTree<L, B>,
  &[NodeSlice])`, since it takes nothing from `KeyVals` but the value runs and
  the root's span/state. A binding would then call techy's own implementation
  instead of copying its behaviour.
- **Workaround taken**: re-implemented `get_combined_with` over the minted tree
  and the recorded value ranges, line by line against techy's source, and pinned
  it with techy's own test assertions translated to Python.
- **Severity**: moderate — about 45 lines of re-derived semantics whose only
  oracle is techy's own test


### The re-entrant self-passing recomposer costs every Rust consumer a self-referential error enum that carries no information

- **Kind**: design friction
- **Where**: `techy::recompose::RecomposeContext`'s region ops
  (`recompose_slot_content_named` and its six siblings), against
  `Recomposer::Error`
- **Found while**: M5, porting `recompose_oracle.rs`'s `GrabAttached` — the
  re-entrant self-passing shape, which is the *documented* way to reach nested
  attached content
- **What happened**: the ops are generic over the recomposer's own error type,
  so a recomposer that hands `self` to one of them needs
  `Self::Error: From<RecomposeError<Self::Error>>` — a type that can contain a
  wrapper around itself. techy's own oracle pays it in full: a `GrabError` enum,
  a `Box`, a hand-written `From` impl, and an `#[allow(dead_code)]` because
  nothing ever reads the payloads it carries. Twenty lines of the file's
  `GrabAttached` section, and the doc comment on it has to explain the pattern
  ("the transform suite's `OpError` shape") because it is not guessable.

  What binding it revealed: **none of it is essential complexity.** The Python
  port of the same recomposer is eight lines — `grabbed`, `inner`, and an
  `instruction` that calls the op and delegates — and behaves identically,
  nested inclusion and grab order included. The thirty lines that disappeared
  are entirely error plumbing; zero behaviour was lost. So the tax is
  measurable, it is paid by every Rust consumer who writes this documented
  shape, and it buys an error value that the crate's own test suite has to
  silence a warning about.
- **Wish**: a provided type in `techy::recompose` — a
  `ReentrantError<E>` newtype (or a blanket-implementable helper) that gives
  `From<RecomposeError<ReentrantError<E>>>` once, so a consumer writes
  `type Error = ReentrantError<MyError>` and nothing else. Same for
  `techy::transform`, where `OpError` is the same shape under a different name;
  today each consumer re-derives it from a test-suite example. Failing that, one
  sentence on `RecomposeContext`'s docs pointing at the pattern by name, since
  the ops' own docs do not mention that passing `self` constrains `Self::Error`.
- **Workaround taken**: dropped outright on the Python side —
  `tests/test_recompose_oracle.py::GrabAttached` has no analogue and needs none,
  since Python propagates an exception raised inside `instruction` verbatim. The
  class docstring records that the enum was dropped and why, so the port still
  diffs against the Rust.
- **Severity**: moderate — measured: thirty lines of pure error plumbing per
  Rust consumer who writes the documented shape, buying a value techy's own
  test suite has to `#[allow(dead_code)]`


### The latexlike preset ships no public spec that produces a **sibling** after-effect, so `input_macro_spec`'s `persist_state` cannot be exercised without writing a construct parser

- **Kind**: missing feature
- **Where**: `techy::latexlike::input_macro_spec` (the `persist_state`
  parameter), `techy::latexlike::MacroSpec` / `EnvironmentSpec`; the private
  `AfterEffectSpec` in `techy/src/latexlike/input.rs:805`
- **Found while**: M6, proving `\input` end to end from Python
- **What happened**: `persist_state` decides whether the included run's
  `AttachedSourceOutcome::after_effects` continues past the `\input`. To see the
  difference at all, the *included file* must contain a construct that produces
  a sibling after-effect — and the crate ships exactly one producer:
  `InputMacroSpec::parse` (`latexlike/input.rs:341`), which only **forwards**
  whatever the included run produced. Every site in the crate that originates
  one — every literal `Ok((id, Some(delta)))`, five of them — is inside a
  `#[cfg(test)]` module. So the only shipped producer is the very construct
  under test, nesting `\input` in `\input` never manufactures a first delta, and
  the parameter is unobservable using shipped specs alone.

  techy's own tests know this — they define a private `AfterEffectSpec`
  (`input.rs:805`, one field, one `Ok((id, Some(self.delta.clone())))`) purely
  to make the four `persist_state` tests possible. Every embedder proving the
  same thing has to rediscover and rewrite it.

  The declarative spec classes have no channel for it either:
  `ArgumentSpec::with_state_delta` scopes an argument's content and
  `EnvironmentSpec::with_body_delta` a body; neither is a *sibling* effect.
  A `\newcommand`-style definition — the paradigm case the `persist_state`
  docs name — is therefore not expressible declaratively at all.
- **Wish**: make `AfterEffectSpec` public (or add
  `MacroSpec::with_after_effect(delta)`). It is nine lines, it is already
  written, and it is the declarative half of the mechanism `persist_state`
  documents. It would also give the guide's `\newcommand` example a body.
- **Workaround taken**: the binding proves `persist_state` through M6's
  construct-parser seam — a Python `ConstructParser` returning
  `(build_id, delta)` — in
  `tests/test_source.py::test_gate_persist_state_is_observable_both_ways`. That
  works, but it means a *declarative* parameter can only be demonstrated by an
  embedder who has already taken over parsing, and it was not provable at all
  from the binding's M5 surface (`PROGRESS.md` §D254).
- **Severity**: moderate — a shipped no-default parameter that no shipped spec
  can exercise

> **Hit again, M7 review follow-up.** Proving the *merge* half of the same
> mechanism — that a run's sibling after-effects arrive at `finalize_transition`
> as one collapsed delta — needs **two** originating after-effects inside the
> included file, so the binding had to write the same private `AfterEffectSpec`
> a second time, now as a Python `ConstructParser` returning
> `(build_id, ParsingStateDelta(ext=…))` (`tests/test_custom_lang.py`'s
> `ExtSetter` / `ExtSetterSpec`). A public `MacroSpec::with_after_effect(delta)`
> would have made both the `persist_state` proof and the replay-granularity
> proof three lines of shipped spec each.


### Three `ParseContext` methods appear in **neither** guide chapter: `parse_attached_source`, `attach_source_reference`, `group_interior_state`

- **Kind**: doc gap
- **Where**: `docs/construct-parsers.md` and `docs/ai-guide.md` (and, in fact,
  every file in `docs/`)
- **Found while**: M6, binding the full `ParseContext` surface
- **What happened**: `grep -rl 'parse_attached_source\|attach_source_reference\|group_interior_state' docs/*.md`
  returns **nothing**. The construct-parser chapter is otherwise a complete tour
  of the type — probing, staging, recovery, the scoping methods, `with_frame` —
  so the absence reads as "these are not for you" rather than as an omission.
  They are in fact three of its most consequential operations: the two attached-
  source entry points are the *entire* `\input` story from the parser side, and
  `group_interior_state` is the memoized derivation that `read_rigid_name_group`
  itself uses and that any hand-rolled group descent needs. All three had to be
  found by reading `attached_source.rs` and `mod.rs`.
- **Wish**: one paragraph each in `construct-parsers.md`. For the attached-source
  pair, the paragraph that matters is *which* of the two to use — resolve-and-
  diagnose (`attach_source_reference`) versus you-already-have-the-source
  (`parse_attached_source`) — and that the sub-parse shares the session, so its
  `BuildId`s are yours. For `group_interior_state`, that it is the memoized
  `base + expecting_close + driver descent delta`, and that hand-deriving it
  instead loses the memo *and* the driver's delta.
- **Workaround taken**: all three are bound (`techy.core.constructs.ParseContext`)
  with the contract restated on each method, sourced from the Rust rather than
  from the guide.
- **Severity**: moderate — a documented type with three undocumented methods is
  worse than an undocumented one, because nobody goes looking


### The shipped argument parsers validate their **author's** input when they parse, so the diagnostic points at a document

- **Kind**: design friction
- **Where**: `techy::core::constructs::GroupArgumentParser::any_of`,
  `OptionalGroupArgumentParser::any_of`, `MarkerArgumentParser::new`,
  `EmbellishmentsArgumentParser::new`,
  `TackOnFieldsArgumentParser::with_field`
- **Found while**: M6 part 2, binding the shipped argument parsers
- **What happened**: each of these documents its own precondition as "reported as
  an implementation error when the parser runs" — an empty rule set, an empty
  marker, an empty marker list, a duplicate field name. Every one of those is a
  property of the **constructor's arguments**, known at the call site, and every
  one of them is checked only once a document reaches the parser. The result is
  an `ImplementationError` whose span points into somebody's `.tex` file, raised
  from a definition written in a different module, possibly by a different
  person. Cost: the binding cannot pass those constructors through, because the
  Python author is on the stack at construction and gone by parse time.
- **Wish**: check in the constructor. `any_of` and `EmbellishmentsArgumentParser::new`
  could return `Result`, or — since these are `impl IntoIterator` builders where a
  `Result` would be noisy — `debug_assert!` plus a documented panic would already
  be better than a parse-time diagnostic, because it fires in the author's own
  test run. `with_field`'s duplicate check is the clearest case: it has the whole
  field table in hand.
- **Workaround taken**: refused at construction as a `ValueError` naming what is
  wrong (`src/constructs.rs`, `non_empty_rules` and friends; `PROGRESS.md` D260).
  The binding therefore never reaches techy's own check.
- **Severity**: moderate — it is the difference between an error the author
  sees and an error the author's *user* sees


### `make_node_ext` hands out a borrow of the arena it is about to grow

- **Kind**: contract risk
- **Where**: `techy::state::Lang::make_node_ext(kind, span, state, children:
  StagedChildren<'_, Self>)`, called from
  `ParseContext::stage_node`
- **Found while**: M7, writing the SAFETY argument for the lend above
- **What happened**: the `'b` in `StagedChildren<'b, L>` borrows the
  `NodeTreeBuilder`'s arena, which is a growing `Vec<Staged<L>>`, and the hook
  runs *inside* `stage_node` — one instant before the node whose ext it is
  minting is pushed onto that very `Vec` (`constructs/mod.rs:180-186`:
  `staged_children(&children)`, then `builder.add(...)`).

  For a Rust implementation the borrow checker makes this a non-event, and
  emphatically so: the parameter's `'_` is elided and the return type
  `NodeExt<Self>` carries no lifetime, so a safe implementation **cannot** hold
  the view past the call even if it tries. **This is a documentation gap, not a
  soundness one** — and it is a documentation gap precisely because the one
  audience it endangers is the one the borrow checker is not protecting: an
  implementation that can leak the view (an FFI binding, a hook that stashes it
  in a `thread_local`) holds not "stale data" but a dangling slice pointer after
  the next reallocation. Nothing in the type or the docs says so. The docs
  explain the descent-only restriction in detail, `StagedChildView`'s note that
  its accessors return "`'b`-borrowed data (borrowing the builder, not this
  transient proxy)" is the closest they come, and neither says the builder is
  about to move underneath it.
- **Wish**: one sentence on `make_node_ext` and on `StagedChildren` saying that
  the view borrows a container the caller is about to mutate, so it is valid for
  the call and not one instruction longer. (Compare `ParseContext::new`, which
  does document its intended growth points.) A `#[doc(hidden)] fn assert_not_
  escaped` is not wanted; the sentence is.
- **Workaround taken**: the sentence is written in this binding instead
  (`src/nodes.rs`' `erase` SAFETY comment names it as "the real hazard"), and
  every value the lent view answers is an owned copy or a proxy carrying the
  same token, asserted by a source-scanning audit (`PROGRESS.md` D310).
- **Severity**: moderate — a one-line doc change that would have saved an hour
  of deriving the hazard from the field types


### `RestageContext` and `RecomposeContext` carry a phantom lifetime over data that does not exist, and each costs an FFI embedding one `unsafe` erasure

- **Kind**: API friction
- **Where**: `techy::transform::RestageContext<'t, L, A, B>` — field
  `_input: PhantomData<&'t NodeTree<L, A>>` (`transform/context.rs:107-117`);
  `techy::recompose::RecomposeContext<'t, L, A>` — `_input: PhantomData<&'t …>`
  is its **only** field (`recompose/context.rs:80-84`), and
  `recompose::context::drive` is `pub(super)` (`:37`)
- **Found while**: M5, lending each context into Python for the duration of one
  visitor / recomposer call
- **What happened**: both types document the lifetime as a device rather than a
  borrow — *"a type/lifetime anchor: the context itself stores no borrow of it
  (ops take their input nodes explicitly and accept any tree's)"* — which is
  precisely why the region ops are cross-tree, and is a good design. But a
  binding's context proxy is a `#[pyclass]`, which cannot carry a lifetime, so
  it must store a token over `Context<'static, …>` while the visitor receives a
  `Context<'t, …>`; `&mut T` is invariant in `T`, so the widening is a cast and
  therefore an `unsafe` block. **The binding contains two `unsafe` blocks whose
  entire justification is "the lifetime this type carries is not real."**

  `RecomposeContext` is the sharper of the two, because it is a zero-sized
  token: its whole surface is `&mut self` methods that immediately delegate to
  the private `drive`, and all the machinery exists to lend something that
  carries **no data at all**. It has no `new()` and no `Default`; its sole
  construction site is inside `pub fn recompose`. If it were constructible, a
  binding could mint one per call and the raw pointer, the erasure and the
  use-after-return failure mode would all disappear.

  This is not a complaint about the *shape*. The self-passing op family is the
  right design and the wrapping contract it protects is the reason. It is that
  the ops are gated behind a value the crate will not let anyone else hold, and
  the gate is made of a lifetime that names nothing.
- **Wish**: either would do, and they are one line each.
  * **Drop `'t`** from both (it is a phantom, and both types already document
    that nothing borrows through it) — or, if it is retained as a documentation
    device, say in the type's docs that erasing it is sound because no `&'t`
    data is reachable, so an embedder can cite the crate rather than argue it.
  * **`impl Default for RecomposeContext` / a public `new()`** — the
    `PhantomData` makes it trivially sound. Alternatively make `drive` public,
    or add a `recompose_nodes(tree, range, state, recomposer)` free function:
    the ops are all "fold this node range through this recomposer", which needs
    no context at all.

  Whoever weighs either wish should weigh both; the same fix serves both types.
  Note the contrast with `ParseContext`, whose lifetimes name **real** borrows —
  that one is a different and harder problem, filed separately.
- **Workaround taken**: `conv::lend` + a `ScopedToken` in a
  `#[pyclass(unsendable)]` proxy, with the pointee's lifetime erased to
  `'static` and every op routed through `with_mut`; one `unsafe` block each,
  with a two-fact SAFETY comment.  *(Corrected by the M8 review: the helper is
  named `lend_context` in `src/transform.rs` only — `src/recompose.rs` spells its
  two blocks differently.  Counted at the M8 gate the binding holds **seven**
  `unsafe` blocks in all: `recompose.rs` 2, `conv.rs` 2, `constructs.rs` 1,
  `nodes.rs` 1, `transform.rs` 1, pinned by
  `conv::tests::the_unsafe_inventory_is_what_the_record_says`.  The "two" in this
  entry is the count for **this gap**, not for the crate.)*
- **Severity**: moderate — two `unsafe` blocks whose SAFETY argument is two
  lines long *because* nothing is reachable through the erased lifetime, so the
  risk is low and the ceremony is pure. One line in techy deletes both.

> **Merged M8** from two entries filed by parallel M5 workstreams, each of which
> found the other and wrote a cross-reference: "Sibling of […], the same techy
> design decision seen from the `recompose` side". They were kept apart because
> the wishes differ — one asks for the lifetime to go, the other for the value
> to be constructible — and because entries were cited by ordinal. Both wishes
> are kept above.


### `Lang`'s five hooks are `static`, so a language cannot carry per-instance configuration

- **Kind**: API asymmetry
- **Where**: `techy::core::Lang::initial_state_data` / `finalize_transition` /
  `scan_specials` / `specials_trigger_chars` / `make_node_ext` — all associated
  functions with no `self`, against `Lang::Driver`, which is an **instance**
- **Found while**: M7, binding the custom-language seam
- **What happened**: techy's own placement doctrine says `Lang` keeps the hooks
  of layers callable *outside* a driven parse, and that is exactly right — but it
  makes those hooks unable to see any per-language value. For a Rust language
  that is free: the `Lang` *is* the type, so its configuration is compile-time
  data. For a binding it is the milestone's whole problem: a Python "language" is
  an ordinary object, and there is nothing on the hook's signature to reach it
  from. Note the asymmetry inside techy itself — `Lang::Driver` is documented as
  an instance precisely "so behavior carries configuration", and the five hooks
  next to it cannot.
- **Wish**: no change to the trait, which is right as it stands; what would help
  is a **documented pattern** for the embedding case in
  `docs/guide/custom_lang.md` — one paragraph saying that a dynamic member must
  key these hooks itself, and that the key has to cover `derived()` (out of
  parse) and `NodeTreeBuilder::add` (out of parse *and* out of session), not only
  the parse. That is the thing a reader has to work out from three separate pages
  today.
- **Workaround taken**: a thread-local stack of installed hook tables, pushed for
  the duration of one `Language` operation — construction as well as parse
  (`src/lang.rs`, `HookScope`; `PROGRESS.md` D330). The one behavioural
  consequence is that a `derived()` made outside every `Language` operation sees
  no table, which is documented on the ABC.
- **Severity**: moderate — the design is right and the guidance is missing: the
  binding had to derive a thread-local hook-table discipline covering
  `derived()` and `NodeTreeBuilder::add`, not only the parse, from three
  separate pages


### `make_paragraph_break_node` and `math_group_interior_delta` take borrows they do not use

- **Kind**: API asymmetry
- **Where**: `techy::latexlike::math_group_interior_delta(base: &ParsingState<LLL>, rule)`
  — reads `base.rules()` and nothing else; and
  `techy::latexlike::make_paragraph_break_node(style, state: &ParsingState<LLL>, token, source_content)`
  — whose body opens `let _ = state;`
- **Found while**: M7, binding the three preset behaviour functions so a Python
  driver hook can compose them
- **What happened**: techy documents these three functions as the *reuse* route —
  "`LatexlikeDriver`'s hook bodies are one-line delegations to them and contain no
  behavior these functions do not — a member wanting
  preset-behavior-plus-one-custom-hook writes its own driver composing the same
  functions". A binding is exactly such a member, and it cannot compose them: the
  `&ParsingState` argument is unreachable from an FFI hook (a borrowed state
  cannot become the `Arc` a Python object holds — the same wall as
  `SpecsProvider`'s hooks and `CommandResolver::resolve_command`), so the one
  function that reads only `rules` had to be **copied** rather than called.
  `make_paragraph_break_node` does not read the state at all and is still
  unreachable for a second reason (a live `&Token`).
- **Wish**: take the data the function actually reads. `math_group_interior_delta(&StateData<LLL>, &Arc<GroupRule<LLL>>)`
  would be reachable from every embedding and identical for every Rust caller
  (`state.rules()` is one field away). More generally: a behaviour function
  offered as the reuse route should not take an argument its own body discards.
- **Workaround taken**: `exit_math_context_delta` delegates (its only argument is
  a `ParsingStateStack`, which is built from `Arc`s and crosses whole);
  `math_group_interior_delta` is techy's body re-expressed over `StateData`, with
  `latexlike::tests::the_math_interior_delta_matches_techys` asserting the two
  agree on every arm so the fork is measured rather than assumed;
  `make_paragraph_break_node` is not bound as a function at all (`PROGRESS.md`
  D334).
- **Severity**: moderate — it is the difference between "reuse the preset's
  behaviour" being a call and being a copy


### "Replay granularity" reads as if a nested *descent* inside an included file were invisible to `finalize_transition`; only a *sibling after-effect* is, and only in the forwarding construct's own transition

- **Kind**: doc friction
- **Where**: `docs/ai-guide-custom-lang.md` §"Replay granularity";
  `techy::core::constructs::NodesOutcome::after_effects`
- **Found while**: M7 review follow-up, fixing a binding gate test whose
  docstring claimed more than it measured
- **What happened**: the paragraph says a construct forwarding a merged
  after-effect record "yields a *single* derivation: `finalize_transition` sees
  one transition carrying the merged delta — intermediate values (**a mode
  entered and left inside the included file**) are invisible".  Read once, that
  parenthesis reads as "`$x$` inside an `\input`ed file is invisible to
  `finalize_transition`", and a binding gate proof was written and shipped on
  that reading.  Measured, `finalize_transition` sees the math descent perfectly
  well — `$x$` is a **nested group descent**, i.e. an ordinary child-state
  derivation, and every derivation reaches the customizer wherever it happens.
  What is actually invisible is the intermediate value of a **sibling
  after-effect chain**, and only in the *outer* run's single transition: the
  chain's own transitions inside the included file still fire, one per
  after-effect.  Cost: about an hour, plus a wrong claim shipped in a gate
  proof's docstring for a milestone.
- **Wish**: two sentences.  (a) Name what the parenthesis is *not* about —
  "sibling after-effects, not descents: a group descent inside the included file
  is an ordinary derivation and is seen".  (b) Say which transition collapses —
  "the *forwarding* construct's single transition carries the merged record; the
  included run's own transitions are unchanged".  A three-line worked example
  (two after-effects setting the same field, showing the outer transition
  jumping straight to the later value) would remove the ambiguity entirely.
- **Workaround taken**: the binding now measures the real thing.
  `tests/test_custom_lang.py::test_a_merged_sibling_after_effect_reaches_finalize_transition_collapsed`
  builds two genuine sibling after-effects on one field inside an `\input`ed
  file and asserts the exact transition sequence
  `[(None, None), (None, 'one'), ('one', 'two'), (None, 'two')]` — the last pair
  is the collapse, and `'one'` never appears in it.  The old test's docstring was
  rewritten to say plainly why the nested-group case is not the merged-delta
  case (`PROGRESS.md` D1 disposition, M7 review).
- **Severity**: moderate — about an hour, plus a wrong claim shipped in a gate
  proof's docstring for a whole milestone — the failure mode of a doc sentence
  that reads as more than it says


---

## Minor

Polish: a naming asymmetry, a missing derive, a one-method gap, a doc
sentence. Costs an hour or a paragraph, not a design.

### `Source::including_sources` yields `&Source`, so its output cannot be kept

- **Kind**: design friction
- **Where**: `techy::source::IncludingSources` (`Source::including_sources`)
- **Found while**: M1, binding `Source.including_sources()`
- **What happened**: the iterator is documented as "the general primitive under
  include-chain policies", and for the in-crate policies (`.any(..)`, `.count()`)
  `Item = &'a Source<O>` is right. A consumer that wants to *keep* the chain —
  which is what any binding, GUI or report builder does — needs an
  `Arc<Source<O>>` per hop, and there is no way back from `&Source` to the `Arc`
  that owns it. The chain is walked internally through
  `provenance().triggered_at().map(|span| &**span.source())`, i.e. techy has an
  `&Arc<Source<O>>` in hand at every hop and discards it. `provenance_chain()`
  does not have the problem only because `SourceProvenance` is `Clone`.
- **Wish**: `Item = Arc<Source<O>>` (an `Arc::clone` per hop, which every keeping
  consumer pays anyway and every counting consumer can ignore), or a sibling
  `including_source_arcs()`. Note the first source of the chain is `self`, so the
  method would need `&Arc<Self>` or a free function — which is itself a hint that
  `Source`'s chain accessors want an `Arc`-taking form.
- **Workaround taken**: `PySource::including_sources` in `src/source.rs`
  re-walks the provenance chain by hand to collect `Arc`s, duplicating techy's
  own five-line iterator.
- **Severity**: minor — the binding re-walks the provenance chain by hand,
  duplicating techy's own five-line iterator


### The line-index scan cap is prose-only: no constant, no getter

- **Kind**: missing feature
- **Where**: `techy::source::LineIndex::set_max_scan_len`,
  `LineIndexCache::set_max_scan_len` (`DEFAULT_MAX_SCAN_LEN`, private)
- **Found while**: M1, exposing `LineIndexCache.max_scan_len` in Python
- **What happened**: "500 000 bytes" appears in four doc comments and nowhere in
  the API. A consumer that wants to show the limit, decide whether a document
  will be indexed at all, or restore the default after raising it, has to
  hard-code the literal — and silently drifts if techy ever retunes it. There is
  also no way to read back the value a `set_max_scan_len` call installed.
- **Wish**: `pub const DEFAULT_MAX_SCAN_LEN: usize` plus a `max_scan_len()`
  getter on both types. Three lines, and the number stops being folklore.
- **Workaround taken**: `DEFAULT_MAX_SCAN_LEN` is mirrored in `src/source.rs`
  and exported as `techy.source.DEFAULT_MAX_SCAN_LEN`; the mirror is pinned
  against techy's actual behaviour by the Rust test
  `mirrored_scan_cap_matches_techy` (a source one byte over the default must not
  be indexed), so a retune in techy fails our build instead of drifting.
- **Severity**: minor — a mirrored constant, pinned against techy's actual
  behaviour by a test so a retune fails the build instead of drifting


### `ResolveError` exposes two of its three fields

- **Kind**: API asymmetry
- **Where**: `techy::source::ResolveError` (`reference()`, `message()`, and the
  cause set by `with_cause`)
- **Found while**: M1, carrying a Python exception through the resolver error
  channel
- **What happened**: `reference()` and `message()` are plain getters; the cause
  is reachable only through `core::error::Error::source`, which hands back
  `&(dyn Error + 'static)` — the `Send + Sync` bounds and the `Arc` sharing that
  make the field interesting are both erased. Downcasting still works (that is
  how the binding recovers the Python exception it stashed), so this is not a
  blocker, just an inconsistency that sent us reading the struct definition to
  confirm nothing else was available.
- **Wish**: `pub fn cause(&self) -> Option<&Arc<dyn Error + Send + Sync + 'static>>`,
  matching the other two accessors and letting a consumer re-share the cause.
- **Workaround taken**: `resolve_error_to_pyerr` in `src/source.rs` goes through
  `Error::source` + `downcast_ref::<PyErrCause>()`.
- **Severity**: minor — the fact is recoverable by downcasting through
  `Error::source`; the cost was reading the struct definition to be sure
  nothing else was on offer


### `MapResolver` is write-only

- **Kind**: API asymmetry
- **Where**: `techy::source::MapResolver`
- **Found while**: M1, binding `techy.source.MapResolver`
- **What happened**: the type has `new`, `insert`, `From<IntoIterator>` and
  `with_reference_as_origin`, but nothing to read: no `get`, no `len`, no
  `references()`, no `contains`. In Python the natural shape of an in-memory
  resolver is mapping-like (`len(resolver)`, `"a.tex" in resolver`,
  `resolver["a.tex"]`), and none of it can be offered without either re-storing
  the entries alongside techy's `BTreeMap` or resolving through a dummy span.
  Since `MapResolver` exists for tests and preloaded setups, inspection is
  exactly what its users want.
- **Wish**: `get(&self, reference: &str) -> Option<&str>`, `len`/`is_empty`, and
  `references()` (or simply `impl Deref<Target = BTreeMap<String, String>>`).
- **Workaround taken**: none — `techy.source.MapResolver` ships write-only too,
  with `resolve()` as the only reader.
- **Severity**: minor — the Python `MapResolver` ships write-only too, which is
  honest but is not what the type's users want


### Pure-data value types do not derive `PartialEq`, and nothing says whether the omission is a decision

- **Kind**: API asymmetry
- **Where**: `techy::source::SourceProvenance` (derives `Debug, Clone` only);
  `techy::core::node::ChildRegion` (`node/arguments.rs:102` — `#[derive(Clone,
  Debug)]`), and for the same reason `GroupData` / `CallableData` /
  `ParsedArgument(s)` / `ParsedSlot(s)`
- **Found while**: M1 (deciding `SourceProvenance.__eq__`), M2 (deciding
  `__eq__`/`__hash__` for the payload classes)
- **What happened**: techy is otherwise careful and explicit about equality.
  `SourceSpan` and `SourcePos` both hand-write `PartialEq` (identity + range)
  and document the choice; `TextContent` deliberately has *no* `PartialEq` and
  documents **why** (a structural `==` would give misleading answers). These two
  sit between the poles with neither.

  * `SourceProvenance`'s every field is `Eq` (`String`, `SourceSpan`), so a
    derive would be well-defined and would mean exactly what a reader expects.
  * A resolved `ChildRegion` is exactly `(Range<u32>, Range<u32>, u32, TreeTag)`
    — four `Copy` fields, no behaviour, no interior mutability — and derives
    only `Clone, Debug`.

  The cost is not the missing operator; it is that a binding has to **guess
  whether the omission is a considered "no" or an oversight**, and the two
  answers are visibly different in Python. A binding that wants `region ==
  region` (and a hash, so a region can key a dict) must either invent a
  comparison techy does not bless or fall back to identity — and for an object
  minted fresh on every read, identity means "always `False`", which is a silent
  trap for `x in collection`. Today `source.provenance == source.provenance` is
  `False` for exactly that reason.
- **Wish**: `#[derive(PartialEq, Eq)]` on `SourceProvenance` and
  `#[derive(PartialEq, Eq, Hash)]` on `ChildRegion` (staged vs resolved compares
  unequal, which is right). `GroupData` gaining `PartialEq` where
  `L::GroupTypeId: PartialEq` would help too. **Or**, if equality is deliberately
  withheld, one sentence saying so and naming what to compare instead — the
  `TextContent` treatment. Either resolves it; the current silence is the only
  answer that costs a consumer anything.
- **Workaround taken**: `techy.source.SourceProvenance` compares by identity
  (techy's behaviour, conservatively), with the reason on the class docstring and
  `.kind` / `.reference` / `.description` / `.triggered_at` documented as the
  things to compare; every payload class in `src/node_data.rs` is a *view* keyed
  on `(tree object, NodeId, entry index)`, so equality is "points at the same
  record" rather than "holds equal data".
- **Severity**: minor — a derive or a sentence, and the binding's answer is
  defensible either way. Recorded because both filings had to read the struct
  definition to be sure nothing was on offer.

> **Merged M8** from two entries filed in M1 and M2. The M2 entry had already
> named it: "Same shape as the already-filed `SourceProvenance` entry, but here
> the type is *entirely* comparable data."
>
> Not merged here, and deliberately: `Diagnostic` has no `PartialEq` either, but
> that one is not a missing derive — it needs a trait-level answer on
> `DiagnosticData` — and it broke a documented `Sequence` contract. It is filed
> separately under **major**.


### Condition payloads cannot be generic, so lang-parameterized values are stringified

- **Kind**: design friction
- **Where**: `techy_derive`'s `ensure_no_generics(.., "DiagnosticInfo")`;
  `techy::core::specs::ProviderCommandsShadowedByEscape::callable_type: String`
- **Found while**: M1, giving each condition class typed properties
- **What happened**: `#[derive(DiagnosticInfo)]` rejects generics outright (and
  `DiagnosticInfo: Any` makes `'static` mandatory anyway), so a condition can
  never mention `L::CallableTypeId`, `L::ModeId` or any other `Lang` type. The
  one shipped case stores a rendered `String` instead (`callable_type: "Macro"`)
  and its *field* doc says why — good — but the constraint itself is nowhere on
  `DiagnosticInfo`, so an implementor meets it as a compile error rather than as
  a documented rule. The consequence for consumers is that reacting to *which*
  callable form was shadowed means comparing a `Debug` rendering: the exact
  "never spell an identifier as a string literal" failure mode the
  condition-identity design exists to prevent, one level down. The binding
  inherits the stringification and cannot do better.
- **Wish**: a sentence on `DiagnosticInfo` (not only on the one field that hit
  it) saying payloads are `Lang`-free by construction and lang values must be
  rendered. If it ever becomes worth solving, the escape hatch is a
  `DiagnosticValue`-typed field carrying the vocabulary id alongside the label.
- **Severity**: minor — the constraint is real and correct; what is missing is
  a sentence saying so on the trait rather than on the one field that hit it


### A `NodeId` cannot be minted from a tree and an index

- **Kind**: design friction
- **Where**: `techy::core::node::NodeId::new` / `NodeTree::make_id` (both `pub(crate)`)
- **Found while**: M2, building the `(tree, index)` handle shape the ai-guide's own
  bindings advice prescribes
- **What happened**: the documented binding shape is "hold trees + ids, not node
  references", so every Python handle is `Py<NodeTree>` + an index — and there is no
  public way to turn that index back into a `NodeId`. Ids can only be *read off a
  node*, so the binding resolves an index by going `tree.nodes_in(i..i + 1).next()
  .map(|n| n.id())`, which is O(1) but reads as a riddle at every call site (five of
  them). Cost: ~30 minutes of searching the read surface for the accessor that does
  not exist.
- **Wish**: `pub fn NodeTree::id_at(&self, index: u32) -> Option<NodeId>` — the
  tag-stamping, bounds-checked companion of `NodeId::index()`. It is the exact
  inverse of an accessor that is already public, and it is what makes an index-based
  external handle sound rather than merely possible.
- **Workaround taken**: `id_at()` in `src/nodes.rs`, with the reason on it.
- **Severity**: minor — about 30 minutes, and five call sites that read as a
  riddle


### `NodeTree::tree_tag()` is `pub(crate)` although `NodeId::tree_tag()` is public

- **Kind**: API asymmetry
- **Where**: `techy::core::node::NodeTree::tree_tag` vs `NodeId::tree_tag`
- **Found while**: M2, pre-checking ids before `NodeTree::node` so techy's always-on
  `assert!` never becomes a Python `PanicException`
- **What happened**: the whole point of `TreeTag` is cross-tree misuse detection, and
  a binding doing that check *before* the panic needs the tree's own tag. `NodeId`
  exposes its tag; `NodeTree` does not. The tag is reachable as
  `tree.root().id().tree_tag()` — which allocates a `NodeRef` to read a `u32` field
  the tree owns.
- **Wish**: make `NodeTree::tree_tag()` public. It leaks nothing `NodeId` does not
  already leak, and `TreeTag`'s own docs point at exactly this use.
- **Workaround taken**: `tag_of()` in `src/nodes.rs`.
- **Severity**: minor — one helper in the binding; the tag is reachable through
  `tree.root().id()`, which allocates a `NodeRef` to read a `u32`


### `slot_content_parent` has no `_named` twin, unlike every other region accessor

- **Kind**: API asymmetry
- **Where**: `techy::core::node::NodeRef::slot_content_parent`
- **Found while**: M2, binding the region-accessor roster
- **What happened**: the roster is otherwise perfectly paired — `argument_nodes`
  / `argument_nodes_named`, `argument_content_nodes` /
  `argument_content_nodes_named`, `slot_content_nodes` /
  `slot_content_nodes_named` — with the documented rule that the indexed form
  answers `None` and the by-name form raises on the category mismatch.
  `slot_content_parent` breaks the pattern: it is index-only, and its `None`
  therefore means *three* different things at once (not a callable, index out of
  range, or "content is region-level so no wrapper node exists" — the one that
  is not an error at all). A consumer that has a slot *name* in hand must find
  its index by scanning `slots()` first.
- **Wish**: `slot_content_parent_named(&self, name) -> Result<Option<NodeRef>,
  NamedAccessError>`, matching `slot_content_nodes_named`'s contract.
- **Workaround taken**: bound the indexed form only; the docstring spells the
  three meanings out (`src/node_data.rs`, `PyNode::slot_content_parent`).
- **Severity**: minor — the indexed form is bound and its `None` means three
  different things, one of which is not an error; the docstring spells them out


### `slot_content_parent`'s docs name the `InRegion` shape but no shipped example

- **Kind**: doc friction
- **Where**: `techy::core::node::NodeRef::slot_content_parent`
- **Found while**: M2, trying to write a test for the `None` branch
- **What happened**: the doc is precise about *why* `None` is returned
  ("returning the callable itself would send naive recursive walkers into a
  loop") but never says which construct actually produces a region-level slot.
  The standard environment shape always wraps its body in a `List`, so the
  branch looks untestable; it took a grep of `ParsedSlot::new` across the crate
  to find that `latexlike::input_macro_spec`'s `Attached` slot is the one
  shipped example — and that reaching it needs a `SourceResolver` on the driver.
- **Wish**: one clause on the method — "the shipped example is
  [`input_macro_spec`]'s `attached` slot" — and the same pointer on
  `ContentNodes::InRegion`.
- **Workaround taken**: a Rust `#[test]` that wires `input_macro_spec` +
  `MapResolver` (`src/node_data.rs::tests::slot_content_parent_is_none_for_region_level_content`).
- **Severity**: minor — one clause on the method would have replaced a grep of
  `ParsedSlot::new` across the crate


### `summary()` says "not a stability contract", but techy's own acceptance suite pins it

- **Kind**: doc friction / contract risk
- **Where**: `techy::core::node::NodeRef::summary` (and `display_tree`)
- **Found while**: M2, deciding whether the bindings may normalise the rendering
- **What happened**: both renderers carry an explicit "human-oriented and **not
  a stability contract**" caveat, which reads as licence for a binding to
  pythonize the spelling (`group(math_inline ...)`). It is not: techy's own
  1133-line acceptance suite asserts on these exact strings, and a port of that
  suite into another language forks silently the moment the spellings differ.
  The caveat and the practice point in opposite directions, and only reading
  the test suite resolves it.
- **Wish**: extend the caveat with the actual rule — "the format may change
  between releases; within a release it is exact, and the crate's own tests
  depend on it, so a re-implementation must reproduce it verbatim".
- **Workaround taken**: the bindings forward techy's `summary()` untouched and
  pin the strings verbatim, with a comment naming M4's ported suite
  (`tests/test_node_data.py::test_summary_strings_are_techys_own`); recorded
  crate-side as decision D3.
- **Severity**: minor — the caveat and the practice point in opposite
  directions and only reading techy's test suite resolves it; the binding
  forwards the strings untouched


### `ArgumentCodeError`'s coordinates are shaped differently per variant

- **Kind**: API asymmetry
- **Where**: `techy::latexlike::ArgumentCodeError`
- **Found while**: M3, attaching `.index` / `.offset` / `.code` to the Python
  `ArgumentCodeError`
- **What happened**: the four variants carry `index: Option<usize>`,
  `index: Option<usize>`, `index: usize` and `index: usize` respectively; three
  carry an `offset` and one does not; two name the offending character `code`
  and one names it `trailing`. All of that is *correct* per variant — a
  `TrailingCode` really is list-form-only — but a consumer that wants "where did
  this go wrong?" has to write the whole four-arm match to find out, and every
  such consumer writes the same one. The docs' own summary sentence ("errors
  locate themselves with two coordinates: `index` and `offset`") describes an
  accessor pair that does not exist.
- **Wish**: `impl ArgumentCodeError { pub fn index(&self) -> Option<usize>;
  pub fn offset(&self) -> Option<usize>; pub fn character(&self) -> Option<char>; }`
  — the two coordinates the docs already promise, plus the offending character.
  Six lines, and `#[non_exhaustive]` stops being a per-consumer problem.
- **Workaround taken**: `crate::latexlike::argument_code_error` writes the match
  once and attaches `.reason` / `.index` / `.offset` / `.code` / `.trailing` /
  `.message` to the exception, the `UnknownArgumentName.reason` precedent;
  pinned by `src/latexlike.rs::tests::every_argument_code_error_variant_has_its_own_reason`.
- **Severity**: minor — the binding writes the four-arm match once and attaches
  the coordinates to the exception; every consumer writes the same one


### `NodeRef::source()` is `pub(crate)`, and the public detour is documented nowhere near it

- **Kind**: API asymmetry
- **Where**: `techy::core::node::NodeRef::source` (`node/node_ref.rs`)
- **Found while**: M3, resolving a span-backed `post_space` off a parsed node
- **What happened**: `NodeRef::source()` is exactly the accessor an embedder needs
  — it is what `TextContent::Spanned` resolves against, and `NodeRef::post_space()`
  itself is nothing but `post_space.resolve(self.source())` — but it is
  `pub(crate)`. The public route exists (`node.span().source()`, since
  `SourceSpan::source()` is `pub` and returns `&Arc<Source<O>>`), and it is not
  mentioned anywhere near the payload accessors that need it; the `TextContent`
  docs say "the carrying node's **own** source" without saying how to get it.
  Cost: a compile error deep in a test module, then a hunt through
  `source/source.rs` for the detour.
- **Wish**: make `NodeRef::source()` `pub` (it hands out a shared borrow of an
  `Arc`-held value — nothing is at risk), or, failing that, add one sentence to
  `TextContent::resolve` and to the payload accessors: *"the carrying node's own
  source is `node.span().source()`"*.
- **Workaround taken**: `node.span().source()`, in production
  (`src/node_data.rs`, both `invocation_syntax` accessors) and in
  `src/latexlike.rs::tests::the_source_aware_wrapper_resolves_post_space`, with a
  comment naming this entry.
- **Severity**: minor — the public detour exists (`node.span().source()`) and
  is simply not mentioned near the accessors that need it


### `StdParseDriver`'s command resolver is a by-value generic, which a dynamic language must instantiate once

- **Kind**: design friction (accepted; recorded for completeness)
- **Where**: `techy::core::StdParseDriver<R = (), O: SourceOrigin>` with
  `R: CommandResolver<L>` collected by value
- **Found while**: M4, binding `techy.core.StdParseDriver`
- **What happened**: nothing broke. techy's own docs explain the asymmetry (the
  command resolver is language-*definition* data on the per-command-token hot
  path, so it is monomorphized; the source resolver is an
  *embedding-environment* capability, so it is `Option<Arc<dyn …>>`). A binding
  cannot choose a type at import time, so it instantiates `R` exactly once at a
  `dyn`-dispatching newtype
  (`SharedCommandResolver(Option<Arc<dyn CommandResolver<L>>>)`), paying one
  virtual call per command token. That is the right trade for a host language and
  needs nothing from techy — but it does mean the generic parameter buys the
  binding nothing, and any future *third* strategy point shaped the same way will
  be flattened the same way.
- **Wish**: none. If techy ever wants to make this easier, a blanket
  `impl<L: Lang> CommandResolver<L> for Arc<dyn CommandResolver<L>>` would save
  every embedder the newtype.
- **Severity**: minor — nothing broke, and the binding's one-off `dyn` newtype
  is the right trade for a host language; recorded because the generic buys an
  embedder nothing


### `Package::get` cannot find a specials definition, though `insert_specials` names a callable type and `len` counts them

- **Kind**: API asymmetry
- **Where**: `techy::core::specs::Package::get` (`scopes/mod.rs:947`), against
  `Package::insert_specials` (`:915`), `Package::len` (`:956`) and
  `Package::iter_symbols`
- **Found while**: M4, porting `ai-guide-definitions.md` §Spec types ("trigger
  sequence is the registration key, not stored in the spec") to a Python test
- **What happened**: a package stores specials in a separate `self.specials`
  vector, and `get(callable_type, name)` only searches `self.specs`, so
  `get(CallableType::Specials, "@")` is always `None` — even immediately after
  `insert_specials(CallableType::Specials, "@", spec)`. Three things make that
  the surprising answer rather than the obvious one: `insert_specials` takes a
  `callable_type` (so the reader has already supplied the key `get` wants),
  `len()`'s doc says "specials entries **included**" (so specials *are*
  definitions of this package), and `iter_symbols(CallableType::Specials, mode)`
  *does* list them, using the trigger as the name. `get`'s doc says only
  "visibility-blind: this is the raw data accessor", which reads like the answer
  should be *more* available here, not less. Cost: one confused test iteration.
- **Wish**: one sentence on `Package::get` — "specials are keyed by trigger and
  are not reachable here; use `scan_specials` or `iter_symbols`" — or, better,
  make `get` fall back to a trigger lookup when `callable_type` is the specials
  type, which is unambiguous (the trigger *is* the key) and makes the accessor
  match `len`, `iter_symbols` and the reader's expectation.
- **Workaround taken**: the Python test asserts `get(...) is None` explicitly and
  reaches the definition through `scan_specials` / `iter_symbols`
  (`tests/test_ai_guide.py::test_registration_two_forms_table` and
  `::test_specials_trigger_is_the_registration_key_and_longest_match_wins`).
- **Severity**: minor — one confused test iteration; the accessor's answer is
  correct and surprising


### `KeyValEntry` answers slices but never the value's `NodeId`, so an entry cannot be re-found on the tree

- **Kind**: API asymmetry
- **Where**: `techy::extract::KeyValEntry::value` /
  `KeyValEntry::value_content` (both `Option<NodeSlice>`)
- **Found while**: M5, recording the entry table before `into_tree()`
- **What happened**: `KeyVals` stores each entry's value as a `NodeId` internally
  (`KeyValEntryData::value`) and hands it out only as a `NodeSlice` — the value
  `List`'s *children*, not the `List`. A consumer that owns the tree separately
  therefore cannot ask "which node of the tree is this entry's value wrapper?",
  which is what it needs to re-mint the entry view later; it has to recover the
  child range from the slice and treat that as the identity. It works, but it is
  a derived coordinate standing in for one techy has and does not share.
  (Symmetrically: `SplitAtChars::segment(i)` has the same shape and the same
  gap, though there the segment index is a good enough identity.)
- **Wish**: `KeyValEntry::value_node(&self) -> Option<NodeRef<'k, L, B>>` — the
  value `List` itself. One line, and it makes the entry table re-derivable from
  the tree.
- **Workaround taken**: record `slice.range()` for both `value()` and
  `value_content()` before consuming the `KeyVals`, and rebuild the views from
  the ranges.
- **Severity**: minor — the derived coordinate works; it is a coordinate techy
  has and does not share


### `extract`'s module docs name three spellings per producer but never say which one a binding should surface

- **Kind**: doc friction
- **Where**: `techy::extract` module docs ("Producers mint output annotations")
- **Found while**: M5, mapping twelve Rust functions onto four Python ones
- **What happened**: the module docs are excellent on *what* the three spellings
  do and explicitly name the general form as "owning the bare name", but the
  three exist because Rust needs `A: Clone + Default` bounded in one place and
  not the others — a purely Rust-side constraint. A reader coming from a
  dynamically typed embedding has to work out for themselves that
  `*_keep_annotations` is `split_at_chars(nodes, sep, keep_annotation)` with a
  *supplied* callback and nothing more, i.e. that the triple is one operation.
  techy's own source says it in one line (`fn keep_annotation`); the docs do not.
- **Wish**: one sentence in the "Producers mint output annotations" section —
  "`*_keep_annotations` is the general form with `|part|
  part.original().map(|n| n.annotation().clone()).unwrap_or_default()`; the
  triple exists so the `Clone + Default` bound lands only where it is needed" —
  which tells an embedder immediately that the three collapse into one.
- **Severity**: minor — one sentence would tell an embedder that the three
  spellings are one operation


### A `RestagedArgument` / `RestagedSlot` cannot be taken apart, so a non-Rust embedding must make it consume-once

- **Kind**: API asymmetry
- **Where**: `techy::transform::RestagedArgument` / `RestagedSlot` (accessors
  `spec`, `is_provided`, `nodes`, `name`, `role`)
- **Found while**: M5, binding the bundles as Python objects
- **What happened**: `restage_invocation` takes bundles **by value**, which in
  Rust makes a double use a compile error. A Python object cannot be moved out
  of, so the binding must either clone the bundle or empty it — and neither is
  possible from outside: the bundle exposes its spec, presence and node list but
  never its `ContentNodes` designation or its `ext`, so it cannot be
  reconstructed, and it implements neither `Clone` nor a decomposing `into_*`.
  The binding ends up holding techy's value in a cell and taking it out on first
  use, so a second use is a runtime error where Rust has a compile error. That
  is the right answer, but it is forced rather than chosen.
- **Wish**: either `content(&self) -> Option<&ContentNodes>` + `ext(&self)`
  (making the bundle re-constructible through the public constructors, which is
  all an embedding needs), or `impl Clone for RestagedArgument<L> where
  L::ArgumentExt: Clone` — the fields already are.
- **Workaround taken**: `Mutex<Option<RestagedArgument<L>>>` inside the pyclass,
  taken on use; `.is_consumed` is exposed and a second use raises `ValueError`
  naming the reason.
- **Severity**: minor — the binding holds the value in a cell and takes it on
  first use, so a second use is a runtime error where Rust has a compile error
  — forced rather than chosen


### The transform module's escape hatch is the one part of it an embedding cannot use, and the cross-tree route that replaces it is a passing mention

- **Kind**: doc friction
- **Where**: `techy::transform` module docs ("Read frozen, write staged" and
  "Cross-tree by contract"), `RestageContext::builder`
- **Found while**: M5, deciding what to do about `builder()`
- **What happened**: the module docs present `builder()` as the power boundary
  ("the ready-made ops are conveniences, not the power boundary") and the
  two-line `make_node_ext` + `add` recipe as the way to synthesize a node. Every
  ingredient of that recipe is a Rust type a dynamic embedding must bind
  first — `NodeKind`'s payload constructors, `TextContent`, a `NodeExt` — so
  the escape hatch is exactly the part that does not cross, and an embedding
  that has not bound them has no way to introduce *new* content at all. What
  saves it is the cross-tree contract — parse the new text, then
  `restage_subtree` its nodes in — which the docs state in a four-line section
  at the end, framed as "for assembling a new tree out of pieces of several
  others" rather than as *the* way to insert content.
- **Wish**: one sentence in "Cross-tree by contract" — "this is also how a
  consumer that cannot build a `NodeKind` (an FFI embedding, a plugin) inserts
  new content: parse it, then restage the parsed nodes in" — which names the
  audience that needs the section most. A `RestageContext::restage_copy(node,
  annotation)` (the private `copy_verbatim` it already has) would round it out
  by covering "insert this node without running the visitor over it".
- **Severity**: minor — the cross-tree route that saves it is already
  documented, just not framed as the answer for the audience that needs it most


### `recompose` always folds from the root, so there is no supported way to fold a subtree

- **Kind**: API asymmetry
- **Where**: `techy::recompose::recompose(tree, state, recomposer)` vs
  `techy::visit::walk(node, visitor)`
- **Found while**: M5, binding the two entry points side by side
- **What happened**: `walk` takes a `NodeRef` and covers that node's subtree;
  `recompose` takes a `&NodeTree` and always starts at `tree.root()`. The two
  consumer libraries are documented as siblings — the guide's "Choosing a
  consumer" table lists them one row apart — and their entry points disagree
  about what a run covers, with no note saying why.

  In Rust the asymmetry is invisible enough (`drive` is right there, private but
  obviously the thing). From a binding it is a documented difference a user asks
  about immediately: "why can I `walk(node)` but not `recompose(node)`?" The
  honest answer turns out to be "the region ops are how you fold part of a tree",
  which is a good answer — it is just not written down next to either function.
- **Wish**: `pub fn recompose_from(node: NodeRef<'_, L, A>, state, recomposer)`,
  which is `drive` with a fresh context and is the exact mirror of `walk`; or, if
  the restriction is deliberate (the root's state has nowhere else to come from),
  one sentence on `recompose` saying so and pointing at
  `RecomposeContext`'s ops for partial folds.
- **Workaround taken**: `techy.recompose.recompose` takes a tree only, and its
  docstring points at the context ops; `techy.visit.walk` was widened to accept a
  whole tree as well as a node, so at least the *tree* spelling works on both.
- **Severity**: minor — cited by §Known deviations row 20 as the reason
  `recompose` did not get `walk`'s widening; the honest answer (the region ops)
  is simply not written next to either function


### `SourceRecomposeError`'s coherence check is unreachable from any parse, and its own docs are the only place that says so

- **Kind**: doc friction
- **Where**: `techy::latexlike::SourceRecomposeError::IncoherentInvocationSyntax`
- **Found while**: M5, trying to write a Python-level test for the one error the
  preset recomposer can raise
- **What happened**: the variant's doc says "a hand-built or incoherently
  restaged tree; parses never produce this", which is exactly the fact a test
  author needs — and it is on the *variant*, not on `SourceRecomposer` or
  `source_recomposer`, which is where someone binding the recomposer reads. The
  consequence for a binding is concrete: the error is untestable from the bound
  surface, because hand-building a `NodeTree` is a §9 reduction (`NodeTreeBuilder`
  is deliberately not exposed), so the raise path has to be pinned in the
  binding's *Rust* tests over a hand-staged tree.

  Not a defect — the design is right, and the recomposer being the place an
  incoherent payload surfaces is a good choice. It is that "you cannot reach this
  from a parse" is load-bearing information for anyone writing tests against the
  recomposer, and it is one level deeper than they will look.
- **Wish**: one clause on `SourceRecomposer`'s own docs — "the only failure is
  `SourceRecomposeError`, which no parse can produce (see the variant)" — so a
  reader of the recomposer knows its `Result` is infallible in practice.
- **Workaround taken**: the conversion is pinned in
  `src/recompose.rs::tests::an_incoherent_payload_becomes_the_python_error`,
  which rebuilds techy's own hand-built fixture; the Python test asserts the
  class's shape and says where the raise path lives.
- **Severity**: minor — the fact a test author needs is on the variant rather
  than on the recomposer, one level deeper than they will look

Entries for `dev-docs/TECHY_GAPS_WISHES.md`, from porting
`techy/tests/recompose_oracle.rs` to `tests/test_recompose_oracle.py`.
Append to the "Entries" section, in this file's own format.


### Both construct-parser guide chapters say `stage_node` "returns a `BuildId`"; it returns a `Result`

- **Kind**: doc gap
- **Where**: `docs/construct-parsers.md` §"What a parser receives", and the
  `core::constructs` module docs
- **Found while**: M6, binding `ParseContext::stage_node`
- **What happened**: the prose says `cx.stage_node(kind, span, state, children)`
  "mints the node's language extension and stages the node, **returning its
  `BuildId`**". The signature is `Result<BuildId, NodeBuildError>`, and it is the
  *only* `ParseContext` method whose error is not a `ParseError` — every use
  site in techy lifts it with `.map_err(|e| cx.implementation_error(e, span))`.
  The chapter's own compiled example does exactly that, so the code is right and
  the prose is not; a reader who follows the prose will write a binding that
  offers `NodeBuildError` as something tolerant recovery can swallow, which is
  precisely what `implementation_error`'s documentation says it must never be.
- **Wish**: one clause — "returning its `BuildId`, or a `NodeBuildError` to lift
  with `implementation_error`".
- **Severity**: minor


### `ParseContext::session`'s field doc names four things; three of them are unreachable

- **Kind**: doc gap
- **Where**: `techy::core::constructs::ParseContext::session` — "The session: node
  building, diagnostics, derivation memos, frames."
- **Found while**: M6, binding `ParseContext.session` as a scoped proxy
- **What happened**: the field doc reads as a list of what the session offers, so
  the binding was planned around it. From **outside** techy's crate exactly three
  members are reachable: `diagnostics`, `ext` and `snapshot_frames()`. `builder`
  and `frames` are `pub(crate)`; `state_stack()`, the push/pop pairs and *both*
  derivation memos are private. So "node building", "derivation memos" and
  frames-as-a-stack are not surface at all — they are an internal description of
  the type wearing the syntax of a public one. Cost: a planned surface that had
  to be re-derived from the source, and a Python class narrower than its own
  coverage row first claimed.
- **Wish**: say what a *consumer* can reach — "diagnostics, the language's session
  extension, and a rendered snapshot of the live frame stack; node building and
  the derivation memos are internal" — or make the doc a `#[doc(hidden)]`-style
  internal note. Either is fine; the current sentence is the one shape that
  misleads.
- **Workaround taken**: the Python `ParserSession` binds the three reachable
  members and its own docstring states the gap rather than faking the rest
  (`src/constructs.rs`, `PyParserSession`).
- **Severity**: minor — but it is the *first* thing an embedder reads about the
  type, so it mis-plans work before anything else can correct it


### The two body parsers borrow their names for the parser's whole life, which no FFI embedding can satisfy

- **Kind**: API asymmetry
- **Where**: `techy::core::constructs::EnvironmentBodyParser::new`
  (`invocation_name: &'p str`, `stop_command_name: &'p str`) and
  `VerbatimBodyParser::new` (`invocation_name: &'p str`)
- **Found while**: M6 part 2, binding `EnvironmentBehavior::make_body_parser`
- **What happened**: `VerbatimBodyParser::new` takes its `terminator` as
  `impl Into<String>` — owned — and its `invocation_name` as `&'p str`, borrowed
  for the parser's whole life. The two live one argument apart in the same
  signature. For a Rust caller both are free (the name comes from
  `EnvironmentInvocation<'p>`, which already has the right lifetime); for anything
  whose strings come from outside Rust, the borrowed one forces a wrapper struct
  that owns the `String` and rebuilds techy's parser inside `parse`. That wrapper
  had to be written twice, once per body parser.
- **Wish**: `impl Into<String>` for the names too, or `Cow<'p, str>`. They are two
  short strings per invocation against a parser that is already allocating a body
  `List`; the borrow buys nothing measurable and costs every non-Rust caller a
  shim. (The same asymmetry is visible inside techy: the `terminator` is owned
  *because* "the driving spec hook typically composes it per invocation" — which
  is exactly what a `make_body_parser` override does with the name as well.)
- **Workaround taken**: `OwnedEnvironmentBodyParser` / `OwnedVerbatimBodyParser`
  in `src/constructs.rs` — copy at construction, borrow inside `parse`
  (`PROGRESS.md` D264).
- **Severity**: minor — the workaround is ten lines, but it is ten lines every
  embedding writes


### `TokenKind` has no `as_str`, so every consumer invents its own spellings

- **Kind**: missing API
- **Where**: `techy::token::TokenKind` — compare `NodeKind::as_str`
- **Found while**: M7, binding `techy.tokens.TokenKind`
- **What happened**: `NodeKind` has `as_str()`, and the binding's `NodeKind.name`
  is that string verbatim — so `display_tree`'s output, techy's docs and the
  Python surface all agree by construction. `TokenKind` has `Debug` and a
  hand-written `Display` (which renders the *token's spelling*: `Command(\foo)`,
  `Comment("…")`), and no plain variant name. A binding that wants a stable
  member name has to write the eight strings itself and keep them matching
  techy's variant names by eye; so does any consumer building a table keyed by
  kind, and so does any diagnostic that wants to say which kind it saw.
- **Wish**: `TokenKind::as_str(&self) -> &'static str` answering the variant
  name, exactly as `NodeKind::as_str` does. It is eight lines and it removes a
  silent drift channel: renaming a variant upstream currently leaves every
  consumer's string list compiling and wrong.
- **Workaround taken**: `PyTokenKind::name` spells the eight names in this
  crate, with a Rust test (`every_token_kind_arm_maps_to_its_discriminant`,
  whose `match` is exhaustive so a *new* variant is a compile error) and a Python
  test pinning the list (`PROGRESS.md` D312). A **rename** would still pass both.
- **Severity**: minor — but it is the kind of nit that makes a rename silently
  wrong in three places at once


---

## Closed upstream, and positive findings

One entry techy has already fixed, kept and struck so a citation lands
somewhere, and four findings recorded so they stay true — places where a
contract that could easily have been a gap held exactly as documented under an
independent re-implementation. A future change that quietly broke one of these
would be a regression nobody would think to look for, which is why they are
written down.

### ~~techy exports no version constant~~ — **FIXED UPSTREAM**

> **Struck M8: techy has closed this.** `techy/src/lib.rs:238` now reads
> `pub const VERSION: &str = env!("CARGO_PKG_VERSION");`, with a doc comment,
> exported from the crate root — which is the wish below, verbatim. The entry is
> kept rather than deleted because `src/lib.rs`'s hard-coded `TECHY_VERSION`
> still carries a comment pointing here, and because it is this note's one
> worked example of a gap closing.

- **Kind**: missing feature
- **Where**: `techy` crate root (`techy/src/lib.rs`)
- **Found while**: M0, wiring `techy.techy_version()`
- **What happened**: a binding wants to report the version of the bound crate at
  runtime (`techy.techy_version()`, `techy._techy.__techy_version__`). Cargo
  exports `CARGO_PKG_VERSION` only to the crate itself, not to dependents, so
  there was no way to read techy's version from a dependent crate without
  hard-coding it or parsing the manifest in a build script.
- **Wish**: ~~`pub const VERSION: &str = env!("CARGO_PKG_VERSION");` at the crate
  root.~~ **Shipped.**
- **Workaround taken**: hard-coded `TECHY_VERSION` in `src/lib.rs`, with a
  comment marking it as the single follow-up point for the dependency switch.
  *(An action for this binding, not for techy: read `techy::VERSION` and delete
  the mirror.)*
- **Severity**: none — closed upstream.


### The `"test"` definition set was fully expressible from Python (no gap)

- **Kind**: confirmation, recorded because its absence would have been a gap
- **Where**: `techy::latexlike` spec constructors and `techy::core::specs::Package`
- **Found while**: M4, replacing `techy._techy._testing::test_package()` (a
  hard-coded Rust package) with `tests/conftest.py::testing_package()`
- **What happened**: nothing. Named arguments (`argument_specs_named`), the
  `s o m` shape, zero-argument macros, `v` delimited verbatim arguments,
  environments with and without arguments, `EnvironmentSpec::from_behavior` over
  `VerbatimBehavior::default()` and `insert_specials` all round-trip through the
  public Python surface, and the trees compare **identical** to the Rust-built
  package's, node for node and span for span, across all three definition sets.
  Worth recording: the Rust scaffolding was the only remaining evidence that
  something might not be expressible, and there was nothing.
- **Severity**: none


### Both AI guides reproduce exactly — a positive finding, recorded so it stays true

- **Kind**: doc friction (a note, not a complaint)
- **Where**: `docs/ai-guide.md`, `docs/ai-guide-definitions.md`
- **Found while**: M4, turning every code block, table row and adjacent prose
  claim in both guides into a Python test
- **What happened**: **nothing went wrong.** All eight code recipes, all 11
  argument-code table rows, all 18 pitfall bullets and all 3 trap rows behave
  exactly as documented, down to the six literal `summary()` strings, the byte
  range `0..18`, the trailing space in the `\foo ` diagnostic span, and the
  "list-form-only" restriction on `AnyDelimited` / `AnyDelimitedOptional` /
  `BracedOnly` (which `argument_specs_from_str` does reject, as claimed). The
  only two guide items that could not be reproduced are blocked on the binding's
  own schedule (`techy::extract`, `techy::transform`/`recompose` and
  `input_macro_spec` are a later milestone), not on techy.
- **Wish**: nothing — but the guides are now covered by an executable oracle
  outside techy's own test suite (`techy-py/tests/test_ai_guide.py`, 90 tests),
  so a future change to any of these behaviours will show up as a binding test
  failure. Worth knowing that this second oracle exists; techy's own
  `#[doc]`-tested recipes and this file should stay in step.
- **Workaround taken**: n/a
- **Severity**: none


### The re-entrant visitor contract crosses an FFI boundary exactly as documented — a positive finding

- **Kind**: positive finding (recorded so it stays true)
- **Where**: `techy::transform::RestageContext`'s region ops, `RestageVisitor`
- **Found while**: M5, porting `transform/tests.rs`'s `SwapArguments`,
  `UnwrapGroups`, `Duplicate` and `Reinvoke` visitors to Python
- **What happened**: the ops re-enter the visitor with the *same* `&mut
  RestageContext`, which is the one shape a validity-token binding has to worry
  about: a naive implementation lends the context once per run and then refuses
  the library's own documented usage. Because `drive` re-borrows rather than
  aliasing, lending the context afresh **per visitor call** makes every nested
  op work with no special case — the argument swap, the recursive unwrap, the
  duplicate-and-redrive and the absent-argument reinvoke all ported line for
  line and pass. The trait-not-closure decision (`RestageVisitor` exists
  "because the region ops re-enter the visitor from inside a visitor call — and
  a closure cannot pass itself") is also exactly right for Python, where the
  same distinction exists and the same `self` is what gets passed.
- **Wish**: none. Recorded because a future refactor that made the ops take
  `&self` or cached the context would break an FFI consumer silently.
- **Severity**: none


### `SourceRecomposer`'s doc is complete enough that the whole oracle ported with no source-diving — a positive finding

- **Kind**: positive finding (recorded so it stays true)
- **Where**: `techy::latexlike::SourceRecomposer` / `source_recomposer`
- **Found while**: M5, porting all 21 oracle tests
- **What happened**: the doc on `SourceRecomposer` answers, without being asked,
  every question the port raised: that everything emitted comes from payload and
  which payload (`core_source_instruction` for the core kinds, the
  invocation-syntax record for callables, `write_begin`/`write_end` for
  environment sides); that reemission is byte-exact **including** tolerant
  recovery shapes; that the *one* recorded-less-than-consumed recovery is the
  malformed environment terminator and that its consumed `\end` spelling is not
  reproduced; that it is instruction-only and therefore composes under a
  wrapping recomposer; and that the default `Concat` scope already skips
  `Attached`, with the `\input` example spelled out. All four of those facts are
  separate oracle tests, and every one of them was portable straight from the
  doc — no source reading, no experiment, no surprise. The two "easy to get
  wrong" contracts (the reconstruction doctrine, the role asymmetry) are stated
  on `techy::recompose` and on `Recomposer` in terms strong enough that the
  binding's own docstrings quote them nearly verbatim.
- **Wish**: none. Recorded because the S5 sentence in particular is the kind of
  caveat that gets trimmed as an edge case in a doc cleanup, and it is the only
  written statement of why one oracle test is an inequality.
- **Severity**: none

---

## Addendum — filed at the M8 gate, after the consolidation

*(Findings that arrived from M8's documentation, performance and packaging
workstreams **after** the consolidation pass had merged, sorted and counted the
78 entries above.  They are kept here rather than folded in, so that the counts,
the histogram and the summary stay the ones that were actually verified.*

*Only the **first** is a new gap.  The other two were filed as new entries and the
M8 review caught them duplicating counted entries — the failure mode the whole
consolidation existed to remove, reappearing at the gate in the one section the
consolidation did not own.  They are struck and reduced to pointers rather than
deleted, because a reader who was told there were three should find out what
happened to two of them.  A third finding — a measurement for `NodeSlice::new` /
`is_single_source` — was added to that entry directly, which is what these two
should have been.)*

### techy declares `criterion` and documents `cargo bench`, but ships no benchmarks and no corpus

- **Kind**: missing harness / documentation mismatch
- **Where**: `techy/Cargo.toml` (`criterion` as a dev-dependency), the README's
  `cargo bench` instruction; no `benches/` directory and no `[[bench]]` section
  anywhere in the crate
- **Found while**: M8, building this binding's benchmark suite
- **What happened**: `PLAN.md` asked for "a benchmark suite against a corpus",
  so M8 went looking for techy's.  There is none: the `criterion` dependency is
  unused, `cargo bench` is a no-op, and there is **not one `.tex` or `.flm`
  fixture in the repository** — the largest document anywhere in either
  repository is a 1 776-byte test literal.  techy's own
  `dev-docs/archive/CODE_REVIEW_REPORT.md` already records the gap.  The
  binding therefore had to *write* a corpus (three documents, 124 kB assembled
  from renumbered copies) before it could measure anything, and any number it
  publishes is against its own corpus rather than a shared one — so techy-py's
  figures and any future techy figures will not be comparable.
- **Wish**: a `benches/` directory with one realistic document, or at minimum a
  checked-in corpus other consumers can point at.  A shared corpus is worth more
  than a shared harness.
- **Severity**: minor (for the crate; it cost this binding half a workstream)

### ~~The two traps techy documents as *silent* are both detectable at wiring time~~ — **a "hit again" line, not an entry (M8 review)**

*This duplicated a counted entry above on the same two traps, with the same two
binding mechanisms as evidence, which is what the file's own rule at §Entry
format forbids.  The content belongs there and is repeated here only so a reader
arriving at the Addendum is not sent away empty-handed.*

- **Kind**: documentation / missed diagnostic
- **Where**: `docs/ai-guide.md` §Pitfalls ("Specials in a custom `Lang` need both
  hooks wired"), `docs/ai-guide-definitions.md` (the escape-character trap)
- **Found while**: M7 (the `Lang` hook table), M8 (writing the pitfalls index)
- **What happened**: techy documents both as failing silently — a half-wired
  specials pair does nothing, an escape-character clash resolves to the wrong
  callable — and in both cases the information needed to *say so* is present at
  the moment the language is built.  This binding proves it: a `ParseHooks`
  table that declares one half of the specials pair raises `TypeError` at driver
  attachment, and one that declares both but answers `None` from
  `specials_trigger_chars` warns at `Language(...)`.  Neither needed new
  information, only a check at the seam.
- **Wish**: raise or warn at `Lang` construction rather than documenting the
  silence.  A trap that is documented is still a trap; a trap that is detected
  is a diagnostic.
- **Severity**: moderate

### ~~`ai-guide-custom-lang.md`'s replay-granularity example is wrong~~ — **a "hit again" line, not an entry (M8 review)**

*This duplicated a counted entry above on the same guide page, the same section
and the same refutation, and rated it `minor` where that entry rates it
`moderate`.  The counted entry's rating stands.*

- **Kind**: documentation error
- **Where**: `docs/ai-guide-custom-lang.md`, the replay-granularity section
- **Found while**: M8 (porting the custom-language chapter), confirmed against
  M7's gate proof
- **What happened**: the guide states that "a mode entered and left inside the
  included file is invisible" to `finalize_transition`.  Measured, it is not: a
  `$…$` inside an `\input`-ed file is a nested **descent**, gets its own
  derivation, and reaches `finalize_transition` normally — the observed sequence
  is `[("Text","Text"), ("Text","Math")]`.  M7's own gate proof for the merged
  delta asserted the guide's version and was corrected when it was measured
  (`tests/test_custom_lang.py`), and the M7 review found a second, related
  over-claim in the same proof.  The contract the section is *about* — that the
  after-effect of an inclusion is merged, not replayed per construct — is right;
  the example chosen to illustrate it is not an instance of it.
- **Wish**: replace the example with a sibling after-effect (which really is
  collapsed) rather than a nested descent.
- **Severity**: minor (documentation), but it cost two milestones a wrong
  assertion each, which is the argument for making guide examples doctests
- **Severity note**: this is the second entry in this note about a guide claim
  that a test would have caught; the other is the `display_tree` panic.
