# Compile-time feature gates on `Lang` — study, plan, and a critical appraisal

Status update: **adopted with modifications** — the decision record is
DESIGN_RATIONALE.md [§dd-dr:lang-features], which governs wherever this file and
that entry disagree (notably: the feature roster grew to eight, including
Paragraphs and ForbiddenChars; the naming here was superseded, cf.
[§dd-dr:superseded-names]; and the §2.3 measurements are the basis on which the
memory argument was dropped as a motivation).

Status: **exploration, not authoritative.** This file lives in `dev-docs/extra/`
(per Documentation_Structure.md: "exploration of some wilder ideas"). It defines no
`[§dd-*]` label, amends no pillar document, and decides nothing. If any of it is
adopted, the decision gets a DESIGN_RATIONALE entry of its own.

Date: 2026-08-06. Measured against the tree at `e6c8551` (post API review).

Question studied: can the token-rules features (whitespace, groups, specials,
comments, commands) and the scopes/packages subsystem be gated at compile time
through `Lang` — associated consts or associated types — for smaller binaries and
smaller parsing states, *without* a drastic public-API change?

Related prior work: `dev-docs/extra/GateFeaturesOptimizedLangs.md` (2026-07-19)
studied the same question on the pre-API-review tree. This report re-measures on
the current tree, adds a verified prototype of the storage-gating mechanism, and
— the part it did not cover — argues both sides of whether the work is worth doing
at all, plus the public-API delta of doing it. The two analyses are kept
separate: the appendix audits the older one (what still holds, what is stale, what
it had that this study missed) rather than merging them.

---

## 1. Method

All numbers below are measured on this tree, x86-64 Linux, release profile
(`lto = true`, `codegen-units = 1`), binaries stripped, unless marked *estimate*.

- **Sizes**: `size_of` from a temporary integration test.
- **Binary**: two temporary examples — a chars-only `TrivialLang` parse and a
  `Latexlike` parse — measured against a `std` hello-world baseline, plus a
  per-symbol breakdown via `nm --size-sort -S`.
- **Live state count**: a temporary example parsing a synthetic LaTeX document
  (text, bold, nested groups, inline math, comments, an `itemize` environment,
  emphasis) repeated *N* times, walking the tree in storage order and counting
  distinct `Arc<ParsingState>` pointers against node count and peak RSS
  (`VmHWM`).

All probe files were deleted; the tree is unchanged by this study.

---

## 2. What the features cost today

### 2.1 Per-state memory

| type | `TrivialLang` | `Latexlike` |
|---|---|---|
| `TokenRules<L>` | 144 B | 144 B |
| `ScopeStack<L>` | 24 B | 24 B |
| `ParsingState<L>` | **200 B** | **208 B** |
| `ParsingStateDelta<L>` | 208 B | 208 B |

`TokenRules`'s 144 B decompose almost exactly by feature:

| feature | bytes | fields |
|---|---|---|
| groups | 56 | `groups` 24, `temporary_groups` 24, `expecting_group_close` 8 |
| commands | 24 | `commands` |
| comments | 24 | `comments` |
| whitespace | 16 | `whitespace.chars` (`Arc<str>`) |
| forbidden chars | 16 | `forbidden_chars` |
| specials | 0 | (gate bool only — data lives in the providers) |
| the six gate bools | 6 (+2 pad) | |

Note the pattern: `Lang::StateExt = ()` and `ModeId = ()` already cost zero bytes
for languages that don't use them. **The crate already gates two state members by
ZST collapse.** Everything proposed below generalizes that existing move rather
than introducing a new idea.

### 2.2 Binary

Release, stripped, `.text` minus a 299 KB `std` hello-world baseline:

| build | `.text` | techy's share |
|---|---|---|
| chars-only `TrivialLang` (every gate off, empty scope stack) | 469 KB | **~170 KB** |
| `Latexlike` | 703 KB | ~404 KB |

Of the minimal build's 170 KB, 88 KB is in symbols demangling to `techy::*` (the
rest is inlined/monomorphized `core`/`alloc`/`fmt` glue attributed elsewhere):

| module | bytes |
|---|---|
| constructs | 34 479 |
| scopes | 13 268 |
| state | 11 699 |
| token | 9 315 |
| engine | 7 289 |
| error | 6 373 |
| node | 3 199 |
| source | 1 719 |
| spec | 663 |

The instructive part is *what a language with no callables at all still links*:

| symbol | bytes |
|---|---|
| `NodesParser::parse` | 16 549 |
| `Scope::insert` | 7 151 |
| `ParsingState::derived` | 6 696 |
| `StdTokenReader::peek` | 5 339 |
| `GroupParser::parse` | 5 255 |
| `state_memo::hash_key` | 2 847 |
| `Scope::with_definitions` | 1 998 |
| `TokenRulesOverrides::{clone, merge_from}` | 2 300 |

A `Lang` that can never define a callable pays 9 KB for `Scope`'s insert and
copy-on-write machinery, because `derived()` → `apply_op` → lazy scope creation is
reachable from a runtime `Vec` of ops. That is the shape of the whole problem.

**Drop glue is a measurable slice of it.** techy types account for 12 692 B of
`drop_in_place` in the minimal build, of which **4 026 B** is attributable to
gate-able data (`TokenRulesOverrides` 479, `TokenRules` 377,
`Vec<ParsingStateDelta>` 352, `ScopeOp` 270, `Vec<Arc<dyn SpecsProvider>>` 215,
`Scope` 207, …). This matters for the phasing: drop glue is generated from *field
types*, so const gating alone does not remove it — it is one of the few binary
wins that genuinely requires Phase 2. (The predecessor put this at ~7 KB "in the
delta types alone" on arm64; on x86-64 against this tree the whole gate-able-data
family is 4 KB. Worth re-measuring on the real target before it is used as an
argument.)

**Unused *presets* already cost zero.** No byte of `latexlike` appears in the
minimal build — the crate is generic over `L`, so monomorphization is already lazy.
The gating budget is confined to core subsystems that runtime gates keep reachable:
at most those 88 KB, of which perhaps **40–60 KB** looks recoverable for a
maximally minimal language (*estimate*, not measured).

### 2.3 How many states does a real parse hold?

This is the number the memory case rests on, and neither this study's predecessor
nor the current docs had it. Synthetic LaTeX document, `Latexlike` + `minilatex`:

| repeats | source | nodes | distinct states | peak RSS |
|---|---|---|---|---|
| 1 | 233 B | 36 | 7 | 2.5 MB (baseline) |
| 200 | 46.6 KB | 7 200 | 604 | 5.9 MB |
| 1000 | 233 KB | 36 000 | 3 004 | 19.6 MB |

Strictly linear: **~84 distinct states per 1000 nodes, ~1 state per 78 bytes of
source.** The derivation memo deduplicates well but does not collapse the count —
it grows with the document.

Three readings of the same table, and the third is the one that matters:

1. States are not a rounding error: 3 004 × ~224 B (state + `Arc` header) ≈ 0.7 MB,
   and with each state's cloned `Vec`/`String` sub-allocations and allocator
   overhead, realistically 1–1.5 MB.
2. But peak RSS for that parse is **19.6 MB** — a 73× expansion over the source.
   States are roughly **4–8 % of the parse's peak footprint.** The node tree and
   parse-time scratch dominate by an order of magnitude. Cutting `ParsingState`
   from 200 B to 40 B moves total memory by low single-digit percent.
3. **The saving is inversely correlated with the features a language uses.** The
   flagship preset needs all of them, so it saves nothing. The languages that
   *would* gate features off are exactly the ones whose states are already cheapest
   in absolute terms — their `Vec`s are empty and their `TriggerChars` is the empty
   string, so gating recovers 144 B of mostly-empty headers per state and no
   sub-allocations at all.

Point 3 is the single most important finding in this report, and it did not survive
first contact with measurement: **the memory motivation is the weakest of the
three.** The payload motivation and the unrepresentability motivation are the real
ones.

---

## 3. What Rust allows

Two mechanisms, and the goals need both:

- **Associated consts gate code.** `const ENABLE_GROUPS: bool` monomorphizes to a
  literal; `if L::ENABLE_GROUPS { … }` const-folds and the dead arm is never
  codegen'd. Associated-const **defaults are stable**, so this is purely additive
  to `Lang`.
- **Associated types gate storage.** There is no way to select a field's *type*
  from a `bool`, so memory gating strictly requires associated types. Associated-
  type **defaults are unstable**, so a new `type Groups: Gate;` on `Lang` breaks
  every existing impl (`Latexlike`, `Flavored`, ~6 in tests).

The fix for the second is the crate's own precedent: **bundle the gates behind one
associated type**, exactly as `Lang::NodeExts` bundles `NodeExtTypes` "to keep
`Lang` small". Existing impls then grow by one line, and `TrivialLang`'s blanket
impl absorbs it for every test language.

```rust
pub trait LangFeatures: 'static {
    type Whitespace: Gate;  type Groups: Gate;    type Commands: Gate;
    type Comments: Gate;    type Specials: Gate;  type Scopes: Gate;
}
// `Groups` owns the whole group block: rules, temporary rules, and the
// expected-close slot — see §5.1.
pub trait Lang { type Features: LangFeatures; /* … */ }
```

Each `Gate` carries `const ENABLED: bool`, so one declaration drives both
mechanisms and no site has to be re-found when storage gating follows const
gating.

Cargo features are the wrong tool and remain so: they are global per build, so two
`Lang`s in one binary could not differ, and they would scatter `#[cfg]` through
exactly the code this crate keeps clean.

---

## 4. The shape that keeps the public API still

The predecessor document's main stated cost of the associated-type route was that
`TokenRules`/`TokenRulesOverrides` struct literals "stop working in their current
form (every test constructs them literally)". That cost is avoidable.

**Do not remove fields. Regroup the flat fields into one sub-struct per feature and
gate the sub-struct's *contents*.** A struct all of whose fields are ZSTs is itself
a ZST, so the collapse reaches the whole type while every field path keeps
compiling for every `L`:

```rust
pub struct TokenRules<L: Lang> {
    pub whitespace: WhitespaceRules<L>,   // gated inside
    pub groups: GroupRules<L>,            // enable_groups + groups + temporary_groups
    pub commands: CommandRules<L>,
    pub comments: CommentRules<L>,
    pub forbidden_chars: Arc<str>,        // not worth gating (16 B, no code)
    …
}
```

I prototyped this (GAT-based `Gate::Store<T>`, `Present<T>` / `Absent<T>`; GATs are
stable well inside the 1.86 MSRV) and verified three properties:

| property | result |
|---|---|
| `TokenRules<Full>` | 88 B |
| `TokenRules<Micro>` (groups + comments gated off) | **0 B** |
| `fn count<L: Lang>(r: &TokenRules<L>) -> usize { r.groups.rules().len() }` | compiles for **both**, no added bounds |
| `micro.groups.set_rules(…)` | **compile error**: "trait bound `Off: Enabled` was not satisfied" |

The rule that makes it tractable is the third row: **total reads, bounded writes.**
Getters are total (`&[]` / `None` when the feature is absent), so nothing in the
3 888-line `nodes_parser` needs a `where` clause; only constructors and setters
carry `where …: Enabled`. That distinction is the difference between a contained
refactor and bounds metastasizing through the crate.

Two corollaries cost almost nothing:

- **`ScopeStack<L>`**: gate its inner `Vec`, not its presence in `StateData`.
  `ParsingState::scopes()` keeps its signature and **zero call sites change**; a
  scopes-off language gets a ZST stack and drops the 13.3 KB of scopes code.
- **The derived caches** (`prefix_table`, `trigger_chars`) collapse with the
  groups and specials gates respectively — the freeze-time gate baking already in
  `freeze_with_table` generalizes directly.

And because gated stores are `Default`, `..base_rules()` and
`..TokenRulesOverrides::default()` **keep working unchanged**. Only an explicit
mention of a disabled feature fails to compile — which is the desired refusal, not
collateral damage.

### 4.1 The adjacent axis: open facets instead of a two-valued gate

Question raised 2026-08-06: does this shape also let a language substitute an
*entirely custom* rules type — say a whitespace facet that stores no character set
and always answers `" \t\n"`?

**Not as written.** `Gate::Store<T>` is a two-valued selector over a crate-owned
data type: the language chooses *presence*, not *implementation*. Getting the third
option means replacing the generic `Store<T>` with **per-feature facet traits** —
the crate defines what it needs from a whitespace facet, ships the standard impls,
and the language names any implementor. `Present`/`Absent` then stop being a
separate mechanism and become two impls among N, with `const ENABLED` still driving
the code gating. Prototyped and confirmed to monomorphize:

| facet impl | facet size | `TokenRules` | `Overrides` |
|---|---|---|---|
| `DynamicWhitespace` (today's `Arc<str>` rules) | 24 B | 40 B | 24 B |
| `StaticWhitespace` (chars hardcoded) | **1 B** | 24 B | 1 B |
| `NoWhitespace` (feature absent) | 0 B | 16 B | 0 B |

One machinery function (`skip_whitespace`) compiled once, correct for all three;
`disable_all()` worked for all three through the facet. So the axis is real and the
generalization is small. Four things stand in the way, in rising order of
seriousness:

1. **Scoped disable is not optional, so "static" is 1 byte, not 0.**
   `TokenRulesOverrides::disable_all()` — used by `verbatim_state_delta` and every
   raw-region parser — must be able to switch a feature off for a scope. A facet
   may hardcode its *data* but must still carry the enable bit; one that no-ops
   `set_enabled` would silently keep tokenizing whitespace inside verbatim. (The
   escape hatch is the §5.1 move: bound the raw-region surface on a `Disableable`
   facet marker. That works, but `disable_all` touches six features, so it means
   six bounds on every raw-region entry point.)

2. **The override channel becomes language-parameterized.** `TokenRulesOverrides`'s
   field types turn into associated types, so
   `TokenRulesOverrides { enable_whitespace: Some(false), .. }` stops being
   writable by generic code — it becomes `<Whitespace<L>>::disable_override()`.
   That is a materially bigger public change than §8.2, because it pushes
   projections into the crate's most-used delta type rather than keeping it a
   concrete crate-owned struct.

3. **Cache-invalidation correctness moves to the facet.** `derived()`'s
   prefix-table reuse keys on `Arc::ptr_eq` over `groups`/`temporary_groups`; a
   custom groups facet must answer "are my table inputs unchanged?" itself. Its own
   docs already warn that "a stale reuse here would keep tokenizing the stripped
   delimiters", and there is a test pinning exactly that.

4. **The memo's soundness contract moves to the language author — this is the real
   cost.** `state_memo::{hash_key, keys_eq}` hash and compare overrides *field by
   field, by `Arc` address*, and a memo hit **substitutes a previous derivation's
   result**. With open facets the facet supplies that hash and equality, so a facet
   that conflates two semantically different overrides silently produces wrong
   parse states — not a panic, not a diagnostic. Today that failure is structurally
   impossible because the crate owns every comparison. The crate already treats
   this contract as load-bearing (`finalize_transition` must be pure precisely
   because "that purity is also what makes the memo sound"); handing a piece of it
   to extension authors is a doctrine-level decision, not a refactor.

Also note where the payoff is *not*: memory. 24 B → 1 B per state is noise by §2.3.
The value of this axis is expressiveness — const-folded whitespace tests, a `const`
prefix table, a perfect-hash specials scanner, group rules fixed at compile time —
and the collapse of two mechanisms into one. It is doctrinally consistent
(`Lang::Driver`, `Lang::InvocationSyntax` and `TokenReader` are already
language-chosen types), which is an argument for keeping the door open, not for
walking through it now.

**Actionable consequence for Phase 2, at zero cost:** define the per-feature
accessors (`chars()`, `rules()`, `is_enabled()`, `temporary()`, `expecting_close()`)
as the *only* read path from the start, and treat that set as the facet contract in
embryo. If the concrete `GroupRules<L>` later becomes an associated type, no reader
in the crate or downstream changes — only the constructors and the override
channel. That is the cheap option value; taking the axis itself should be a
separate, later decision (open question 8).

---

## 5. Gotchas found while tracing

### 5.1 `expecting_group_close` is inside the groups gate — and verbatim implies groups

RULED (user, 2026-08-06): **groups are either enabled or disabled; there is no
third state.** `expecting_group_close` belongs to the `Groups` facet, and a
language that gates groups off cannot use verbatim.

This overturns an earlier draft of this report, which proposed a separate
`GroupCloseExpectation` gate on the grounds that `verbatim_state_delta` installs
an expected close with all six *runtime* gates off:

```rust
pub fn verbatim_state_delta<L: Lang>(terminator: Arc<GroupRule<L>>) -> ParsingStateDelta<L> {
    ParsingStateDelta::new().rules(TokenRulesOverrides {
        expecting_group_close: Some(Some(terminator)),
        ..TokenRulesOverrides::disable_all()
    })
}
```

That observation is true and irrelevant. The runtime gate being off inside a
verbatim region says nothing about whether the *language* has groups: the
terminator is an `Arc<GroupRule<L>>`, it is matched by group-close machinery, and
the delimited form stages a `Group` node carrying a `Lang::GroupTypeId`. Verbatim
is built out of the group feature, so verbatim ⇒ groups is a lattice edge
(§5.3), not an argument for splitting the facet. The predecessor document's
placement was right.

The consequence is visible in the public API and is a feature, not a cost:
`verbatim_state_delta`, `VerbatimArgumentParser` and `VerbatimBodyParser` carry an
`Enabled` bound on groups, so a groups-off language cannot name them — the
lattice edge is enforced by the compiler at driver-assembly time rather than
documented and hoped for (appendix item A2).

This also removes a gate from the bundle (§3) and a bullet from Phase 2 (§7): the
facet count drops from seven to six, and `TokenRules`'s entire 56-byte group block
— `groups`, `temporary_groups`, `expecting_group_close` — collapses or survives as
one unit.

### 5.2 Gating the data does not strip the code — gating the dispatch arm does

This is the predecessor's key insight and it holds: `TokenKind::Command` still
exists as an enum variant, so the `nodes_parser` dispatch arm — and through it
command resolution, the invocation parsers, the spec machinery — stays live even
if `TokenRules` has no `commands` field. The sites that actually matter are the
dispatch arms and the reader branches, not the storage. Storage gating buys
memory; only site gating buys binary.

### 5.3 Other traced points

- `state_memo::hash_key` (2.8 KB) walks the rules field by field; it wants both the
  accessor treatment and const skips.
- `derived()`'s 6.7 KB is dominated by the temporary-group stripping rule and the
  prefix-table `Arc`-identity reuse check — the largest single beneficiary of a
  groups gate.
- The feature lattice is real but not total. Known edges: optional-argument
  parsers mint temporary groups, so callables-with-optional-arguments ⇒ groups;
  **verbatim ⇒ groups** (§5.1); environments ⇒ commands. But **callables do not
  imply scopes** (a driver can resolve from a fixed table — the motivating web
  case: a fixed command set, no `\newcommand`). Keep *those* two gates
  independent; enforce the edges with bounds (appendix item A2).
- `TokenRulesOverrides` is the one genuine trade-off. Its memory benefit is ≈ 0
  (deltas are transient). But leaving it ungated means `apply()` silently drops an
  override for an absent feature — against the crate's grain. Gating it via the
  same sub-struct grouping preserves `..default()`; I would gate it, but it is a
  decision, not a derivation.
- "Off" gains a third spelling. Today: gate-`false` = scoped off, empty data =
  constitutive off. Compile-gated = "this language has no such feature at all".
  The existing two keep their meaning for features the language *does* have, so the
  rationale entry extends rather than reopens.

---

## 6. Is this worth implementing at all?

### 6.1 The case against

1. **The memory motivation does not survive measurement.** §2.3: states are 4–8 %
   of a parse's peak footprint, and the languages that would gate features off are
   the ones whose states are already cheapest. If per-state memory is the goal,
   this is a large refactor for a low-single-digit percentage of total memory. The
   cheaper move is elsewhere entirely: the parse expands source by ~73× in peak
   RSS, and nobody has looked at where.
2. **The binary win is real but modest in absolute terms, and its value depends
   entirely on a use case that does not exist yet.** 40–60 KB (*estimate*) off a
   native binary is noise. It is only interesting as **wasm payload**, and there is
   currently no wasm embedder, no FLM-in-browser build, and no payload budget
   anyone has to hit. The predecessor measured 101 KB → an estimated 45–55 KB of
   wasm; those numbers are a year old and were never re-measured.
3. **The churn lands on the most carefully polished types in the crate.**
   `TokenRules`, `TokenRulesOverrides`, `StateData` — roughly 300 field-access
   sites across `src` and tests (groups 57, comments 49, scopes 43, whitespace 37,
   commands 35, `expecting_group_close` 32, `temporary_groups` 30, specials 19),
   plus their doc comments, which are among the crate's best.
4. **It buys a combinatorial test and documentation surface.** Seven independent
   gates is 2⁷ configurations; even a sane matrix of four representative languages
   is a standing CI and maintenance cost, and every `where …: Enabled` bound is a
   line of rustdoc someone has to justify.
5. **Opportunity cost against shipped functionality.** The preset does not yet ship
   macro and environment *definitions* — the pylatexenc default-specs port is
   planned, not written. Optimizing the payload of a parser that cannot yet parse
   standard LaTeX out of the box is optimizing the wrong axis. TODO_Big.md's own
   ordering puts usage-oriented documentation and the API review ahead of this.

### 6.2 The case for

1. **The timing argument is the strongest one, and it is about the window, not the
   feature.** [§dd-dr:stability-rubric] is explicit: the freeze is soft *until a
   framework builds on techy in earnest*, and until then "an important discovered
   shortcoming may still be fixed breakingly — correcting a flaw before dependents
   exist is cheaper than carrying it forever." `TokenRules`'s flat field layout is
   exactly the kind of shape that cannot be changed afterwards. If this is ever
   going to happen, it happens in this window or never. That argues for *deciding*
   now — either way — not for deferring.
2. **Unrepresentability is a design win independent of both size goals.** A
   language that declares no groups currently expresses that as a runtime `bool`
   plus an empty `Vec` that every layer must keep checking. Under gating it becomes
   a type error to construct group rules at all. That is the same move the crate
   already made with `ModeId`, `StateExt`, and the closed `NodeKind` — and it is
   consistent with a codebase that prefers "the compiler catches errors Python
   couldn't".
3. **The API cost is much lower than previously believed.** §4 is the new fact:
   collapse-in-place preserves struct-update syntax, keeps `rules.groups…` paths
   compiling, and keeps every reader of `ParsingState::scopes()` untouched. The
   predecessor rejected the memory route partly on a churn estimate that the
   sub-struct shape invalidates.
4. **It is on the user's own list.** TODO_Big.md's first "big chunk" is this
   feature, phrased in nearly these terms.
5. **The regrouping is an improvement on its own merits.** `enable_groups` /
   `groups` / `temporary_groups` are three adjacent fields related only by
   convention. `GroupRules<L>` names the relationship. That refactor would be
   defensible with no gating at all.

### 6.3 Verdict

**Qualified yes — but only Phase 1 on current evidence, and only if the wasm target
is real before Phase 2.**

Phase 1 (const gating) is worth doing more or less unconditionally: it is additive
to `Lang`, costs no call-site churn, is trivially revertible, and serves the only
well-supported motivation (payload). It also forces the reachability-site audit
that any later storage gating needs anyway.

Phase 2 (storage gating) should **not** be authorized on the memory argument — §2.3
retires that argument. It should be authorized only if, after Phase 1 is measured
on a real wasm build, the remaining data-shaped code is worth ~300 touched sites,
*or* if the unrepresentability argument is judged to stand on its own. That is a
values call about what kind of library this is, not a numbers call, and it is
yours.

The one thing I would not do is leave it undecided indefinitely. The soft-freeze
window is the entire reason this is cheap today.

---

## 7. Suggested plan

Each phase ends in a shippable state, and each has an exit ramp.

**Phase 0 — re-measure the motivating case (half a day).**
Build a wasm probe on the current tree with the size-tuned profile
(`panic = "abort"`, `opt-level = "z"`, `strip = true`; the predecessor found that
profile alone cut every probe ~40 %). Report: baseline, minimal `Lang`, `Latexlike`.
*Go/no-go gate: if nobody can name a payload budget this must meet, stop here and
record the numbers.*

**Phase 1 — const gating, no API shape change (2–3 days).**
- Add `Lang::Features` (one bundle associated type; `TrivialLang` and `Latexlike`
  set it to the all-on bundle — one line each). One associated type rather than
  seven consts, so Phase 2 needs no second migration of the same sites.
- Guard the reachability sites, routing every check through the gate const:
  `reader.rs` (command, comment, whitespace/paragraph, specials branches;
  `PrefixTable::for_rules`), `nodes_parser.rs` (the `Command`, `GroupOpen`,
  specials and paragraph-break dispatch arms — this is what kills the
  invocation/spec chain), `parsing_state.rs::derived` (temporary-group stripping,
  scope-op apply), `delta.rs::apply_overrides`, `state_memo::hash_key`,
  `language.rs`'s stray-close arm.
- Semantics to document: const-off wins over runtime data; data behind a const-off
  gate is inert, never a panic — a violated contract returns `Err` through the
  recovery funnel, per [§dd-dr:panic-policy] rule 3.
- Ship four representative test languages (all-on = the existing suite; all-off;
  groups-only; callables-without-scopes), not the lattice.
- Re-run the Phase 0 probe. *Exit ramp: this phase stands alone permanently.*

**Phase 2 — storage gating for the two facets that pay (1–2 weeks).**
Only `Scopes` and `Groups`; leave whitespace/comments/specials const-only (their
data is a few dozen bytes, and the const already strips their code).
- Regroup `TokenRules`'s flat fields into `GroupRules<L>` / `CommandRules<L>` /
  `CommentRules<L>` / `WhitespaceRules<L>` sub-structs — worth doing even if gating
  stops here.
- Gate contents via `Gate::Store<T>`; total reads, bounded writes (§4).
- `Groups` owns the whole group block including `expecting_group_close`, and the
  verbatim surface carries an `Enabled` bound on it (§5.1).
- Gate `ScopeStack`'s inner `Vec` (no signature changes).
- Mirror the gating in `TokenRulesOverrides` and `ParsingStateDelta::scope_ops`, or
  accept documented silent no-ops. *Decide before starting; it drives the churn.*
- `cargo semver-checks` against the `api-baseline` branch at each step; the
  breaking surface should be exactly the `Lang` impls plus explicit mentions of
  gated-off features.

**Phase 3 — write it down.**
A DESIGN_RATIONALE entry (the "third spelling of off", the total-reads/bounded-
writes rule, the facet-granularity ruling and what was rejected), an ARCHITECTURE
reference for it, and the wasm profile guidance for embedders — which is worth
documenting regardless of whether any of the above happens.

---

## 8. Publicly visible API changes

What an embedder, an extension author, and a language author would actually see.
Semver classes are against the `api-baseline` branch used by
`scripts/check_semver.sh`.

### 8.1 Phase 1 (const gating) — one breaking line, everything else additive

| change | class | who is affected |
|---|---|---|
| `Lang` gains `type Features: LangFeatures` | **breaking** | anyone with a hand-written `impl Lang` — one line each (`Latexlike`, `Flavored`, ~6 test langs) |
| new public items in `techy::core`: `LangFeatures`, `Gate` (with `const ENABLED`), the `On`/`Off` markers, and ready-made bundles (`AllFeatures`, `NoCallables`, `CharsOnly`) | additive | language authors only |
| `TrivialLang`'s blanket impl supplies the all-on bundle | none | every `TrivialLang` user compiles unchanged |

Nothing else moves. No signature changes, no field changes, no behavior change for
any language that keeps all features. The runtime `enable_*` gates keep their
current meaning; the const gate is a stronger statement layered above them.

### 8.2 Phase 2 (storage gating) — the real surface change

**Field regrouping on `TokenRules<L>`** (breaking; the largest single item). Four
flat groups of fields become one sub-struct each:

| today | after |
|---|---|
| `enable_whitespace`, `whitespace`, `enable_multi_newline_paragraphs` | `whitespace: WhitespaceRules<L>` |
| `enable_groups`, `groups`, `temporary_groups`, `expecting_group_close` | `groups: GroupRules<L>` |
| `enable_commands`, `commands` | `commands: CommandRules<L>` |
| `enable_comments`, `comments` | `comments: CommentRules<L>` |
| `enable_specials` | `specials: SpecialsGate<L>` |
| `forbidden_chars` | unchanged (not gated) |

- `WhitespaceRules` **already exists publicly** and would change shape (it holds
  only `chars` today) — breaking for anyone constructing one.
- `GroupRules<L>`, `CommandRules<L>`, `CommentRules<L>`, `SpecialsGate<L>` are new
  public types, each with total getters (`rules()`, `is_enabled()`,
  `temporary()`, `expecting_close()`, …), `Enabled`-bounded setters, and a
  `none()` constructor that works for both gated states.
- `GroupRule` / `CommandRule` / `CommentRule` themselves are **unchanged**.
- `TokenRules::empty()` is unchanged.
- **`..base_rules()` struct-update syntax keeps working.** Only code that names a
  regrouped field by its old path breaks — mechanical and greppable (~300 sites,
  mostly in test helpers).

**`TokenRulesOverrides<L>`** mirrors the regrouping if we gate it (open question
3). `..TokenRulesOverrides::default()` and `disable_all()` keep working; explicit
field mentions change. If we *don't* gate it, its public shape is untouched and
the cost is documented silent no-ops.

**`StateData<L>`** keeps its four public fields (`rules`, `scopes`, `mode`, `ext`)
with unchanged names. But `finalize_transition` implementors mutating rules
directly — `new.rules.enable_comments = …` — switch to
`new.rules.comments.set_enabled(…)`. Breaking for language authors, invisible to
everyone else.

**Unchanged signatures** (worth stating, because this is where the churn was
expected and does not happen):

- `ParsingState::{rules, scopes, mode, ext, prefix_table, trigger_chars}` — all
  identical, including `scopes()` returning `&ScopeStack<L>`, because
  `ScopeStack`'s inner `Vec` is what gets gated.
- `ParsingState::{lang_initial, lang_initial_with_packages, derived}`.
- `PrefixTable`, `TriggerChars`, `Package`, `Scope`, `SpecsProvider`,
  `CallableSpec` — no signature change.
- The whole `techy::{node, visit, transform, recompose, extract, error, source}`
  surface — untouched.

**New compile errors instead of runtime no-ops** (breaking only for languages that
gate something off — i.e. nobody today):

- `ScopeStack::push`, `ParsingStateDelta::{push_provider, scope_op}`, and
  `ScopeOp` construction require `Scopes: Enabled`.
- `verbatim_state_delta`, `VerbatimArgumentParser`, `VerbatimBodyParser` require
  `Groups: Enabled` (§5.1).
- `GroupArgumentParser`, `OptionalGroupArgumentParser` and the other parsers that
  mint temporary group rules require `Groups: Enabled`.
- Setters on any gated-off feature's rules sub-struct.

### 8.3 Net assessment

Phase 1 costs external `Lang` implementors one line and nobody else anything.
Phase 2's blast radius is confined to three types — `TokenRules`,
`TokenRulesOverrides`, `StateData` — plus `Enabled` bounds on the verbatim and
scope-mutating entry points. The read surface that ordinary embedders touch
(`parse`, the node tree, extraction, diagnostics) does not move at all. That is
the strongest practical argument for doing it inside the soft-freeze window rather
than after: the breaking surface is small, well-bounded, and entirely
compiler-detected.

---

## 9. Open questions

1. Is a wasm/browser embedding a real target on a real timeline? Everything in §6.3
   turns on this.
2. Facet granularity: the coarse pair (`Scopes`, `Groups`) proposed here, or the
   full independent set?
3. `TokenRulesOverrides` — gate it (compile-time refusal, more churn) or leave it
   ungated (silent no-ops for absent features)?
4. Naming: `LangFeatures` / `Gate` / `Present` / `Absent` — or something closer to
   the crate's existing vocabulary? [§dd-arch:naming] applies and has not been
   consulted for these.
5. Does the `GroupRules<L>` / `CommandRules<L>` sub-struct regrouping stand on its
   own merits, independent of gating? If yes, it can land first and cheaply.
6. Independent facets, or **closed tiers**? The predecessor's Option C — one
   `type Syntax: SyntaxTier` with three or four closed bundles (chars-only;
   +groups/comments; +callables; +scopes) — kills the combinatorial surface of
   §6.1 point 4 and fits the crate's closed-vocabulary doctrine, at the price of
   users wanting an unanticipated combination. See appendix item A4.
7. Should `Lang` gain a `type Reader`? Orthogonal to gating, but it is the other
   half of TODO_Big.md's swap-out-default-parsers item. See appendix item A5.
8. **Open facets (§4.1) — worth the memo contract?** Substituting a custom rules
   implementation per feature is a strictly more general design and costs nothing
   to keep possible, but taking it hands the derivation memo's hash/equality
   contract to language authors, where a mistake is a silent wrong parse. My
   recommendation: build Phase 2's accessors as the facet contract in embryo,
   decide the axis itself separately, and if it is ever taken, keep the memo's
   comparison crate-owned by having facets expose *identity tokens* rather than
   their own `Hash` impls.

---

## Appendix — audit of the 2026-07-19 exploration

Re-checked against this tree, since that document predates the API review. Its
analysis is kept separate; this is only a delta.

**Still exactly right (verified).**

- Every size it reports reproduces on this tree, to the byte: `ParsingState` 200,
  `TokenRules` 144, `ScopeStack` 24, `TriggerChars` 24, `ParsingStateDelta` 208,
  `TokenRulesOverrides` 152. (arm64 vs x86-64 makes no difference here — the
  contents are pointers and `bool`s.)
- "Unused presets already cost zero" — confirmed; no `latexlike` symbol appears in
  the minimal build.
- The reachability mechanism (§5.2 above): gating storage does not strip code
  because the `TokenKind` dispatch arms stay live. This remains the central
  insight and is why Phase 1 targets sites, not fields.
- Its site inventory is still accurate as a *list* — the reader's command,
  comment, whitespace/paragraph and specials branches; `PrefixTable::for_rules`;
  the `nodes_parser` dispatch arms; `derived()`'s temporary-group and scope-op
  paths; `apply_overrides`; the stray-group-close arm. **The line numbers in it are
  stale** (the tree has moved substantially); use the list, re-find the lines.
- Its recommendation (const gating first, facets only for `Scopes` and `Groups`,
  ~4 representative test languages, size-tuned profile documented regardless)
  matches the plan in §7, reached independently.
- It already flagged memory as "the secondary win… nice, not transformative". §2.3
  does not contradict that — it measures it and sharpens it into an argument for
  dropping the memory motivation outright.
- **Its placement of `expecting_group_close` in the `Groups` facet was right**, and
  an earlier draft of this report wrongly called it an error. Ruled by the user
  2026-08-06 and rewritten as §5.1: the group feature is all-or-nothing, and
  verbatim is built out of it.

**Points it makes that §§1–9 above missed, and that survive re-checking.**

- **A1 — the specials gate is the least valuable one.** `L::scan_specials` is
  statically dispatched from exactly one site (`reader.rs:248` today), so a
  language keeping the default hook already pays almost nothing for specials
  *code*. Gating specials is about the 24 B `TriggerChars` cache and one call
  site — corroborating §7's decision to leave specials const-only.
- **A2 — the feature lattice can be enforced, not just documented.** Under storage
  gating, the standard argument parsers that mint temporary group rules can carry
  `where …: Enabled` bounds, so a groups-off language *cannot name them* — the
  violation becomes a compile error at driver-assembly time, which is the right
  place for it. Under const gating it stays a documented contract whose violation
  must return `Err` through the recovery funnel.
- **A3 — gating cannot destabilize the derivation memo.** The gate is a constant of
  `L`, so it cannot vary within one parse; the memo's `Arc`-identity keying is
  untouched, and gated-off delta fields simply hash as nothing.
- **A4 — Option C, coarse closed tiers**, as an alternative to independent facets.
  Not carried into the plan above; now open question 6.
- **A5 — a per-`Lang` `type Reader`.** `Language::parse_source` still hardcodes
  `StdTokenReader` (`language.rs:142`). Not worth doing *for size* — the tokenizer
  is a small share — but it is the natural hook for a chars-only language and
  overlaps TODO_Big.md's swap-out-default-parsers item; now open question 7.

**Where the two studies disagree, and which to believe.**

Its per-symbol module breakdown (token ~29 KB, constructs ~23, engine ~22, state
~18, spec ~14, node ~13, scopes ~9, error ~9) does not match §2.2's (constructs
34.5, scopes 13.3, state 11.7, token 9.3, engine 7.3, error 6.4, node 3.2, spec
0.7). Different architecture, a year of tree movement, and probably a different
minimal-`Lang` shape and attribution method. **Neither should be trusted as a
budget until Phase 0 re-measures on the actual target.** The disagreement matters:
its numbers say the tokenizer dominates, mine say the construct layer does, and
that changes where Phase 1's guards pay off most.

Its wasm figures (minimal pipeline ~101 KB of techy, `Latexlike` ~125 KB, and the
~40 % cut from the size-tuned profile) are the only measurements of the actual
motivating case that either study has. They are a year old and are exactly what
Phase 0 exists to redo.
