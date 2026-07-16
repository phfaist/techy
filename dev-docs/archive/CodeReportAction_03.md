# Action 03 — Hot-path performance and per-parse retention

**Status: open — one coherent theme: per-transition cost, memo scaling, and unused
hot-path guards. No correctness bugs; all items verified by measurement or experiment.**

Background: "faster than Python" is an explicit design goal; CLAUDE.md also says no
premature optimization. The items below are the places where the *existing* design
discipline ("cache derived data at state freeze"; "memoize by Arc identity") was left
incomplete, so they are cheap to finish and need no new architecture. A profiling/
benchmark harness (item 6) should probably come first to keep this honest.

## 1. Every state derivation deep-clones `StateData` and rebuilds both caches

`ParsingState::derived()` (`src/state/parsing_state.rs`) does `self.data.clone()` →
clones `whitespace.chars: String`, `groups: Vec<Arc<_>>`, every `CommandRule`
(each with its own `name_chars: String`, 52 bytes for a latexlike alphabet), every
`CommentRule` (a `String` each), `forbidden_chars: String`, the `LibraryStack`'s
`Vec<Arc<dyn SpecLookup>>` **and** its `fallbacks: BTreeMap` (a real allocating map
clone for data set once at seeding). Then `freeze()` rebuilds the `PrefixTable` from
scratch (a `String` allocation per delimiter entry) and re-runs
`L::specials_trigger_chars`. Order ~15–20 allocations per transition.

This happens on **every group descent** (memoized per `(base, rule)`, but every distinct
base pays), on **every optional-argument probe** (unmemoized — see item 3), and on every
argument/slot delta. Because nodes record their parse-time state, each distinct state is
retained for the lifetime of the tree.

Two safe fixes:

- **Skip the `PrefixTable` rebuild when its inputs are unchanged.** It depends only on
  `(enable_groups, groups)`. The dominant transition — the group interior — overrides
  only `expecting_group_close`, which is deliberately *not* a table input. Hold the
  table as `Arc<PrefixTable<L>>` and, after finalize, reuse the parent's `Arc` when
  `enable_groups` matches and the `groups` vecs are elementwise `Arc::ptr_eq`. Pure win,
  no new trait bounds, no semantic change. (No analogous generic rule exists for
  `TriggerChars`: its inputs include `L::StateExt`, which carries no `Eq` bound — the
  per-transition cost expectation is now documented on the hook instead.)
- **Make the rules data cheap to clone**: `Arc<str>` for `name_chars` /
  `whitespace.chars` / `forbidden_chars`; `Arc` the `LibraryStack::fallbacks` map. Turns
  the `StateData` clone into refcount bumps and collapses the retention of N
  near-identical states.

Doc note: the claim "both derivations are cheap relative to a transition"
(`parsing_state.rs` module doc, echoed in DESIGN_RATIONALE §3.3) is near-circular — the
cache rebuild *is* a large fraction of the transition's cost. Soften once fixed.

## 2. The engine's group-interior memo is a linearly-scanned, never-evicting `Vec`

`ParserSession::group_interior_memo` (`src/engine/mod.rs`) is a
`Vec<(Arc<ParsingState>, Arc<GroupRule>, Arc<ParsingState>)>` scanned with
`.iter().find(...)` on every group descent — O(n) per descent, O(n²) across a parse,
and it holds strong `Arc`s to every keyed base *and* interior state for the whole
session.

- Fix the scan with a `HashMap` keyed on
  `(Arc::as_ptr(base) as usize, Arc::as_ptr(rule) as usize)`.
- **Constraint:** the ABA soundness of pointer keys depends on the memo *pinning* its
  key `Arc`s (an entry keeps the allocation alive, so a live `Arc` that `ptr_eq`s a
  stored key is necessarily the same state). Keys must therefore stay held — the
  retention is intrinsic to the keying scheme; only the scan cost is fully fixable.
  Eviction would need a different key design. A session is one transient parse, so
  retention is bounded per-parse; decide consciously whether that is acceptable.

## 3. Every optional argument misses the memo and permanently grows it

`OptionalGroupArgumentParser` (`src/constructs/argument_parsers.rs`): the minted
`Arc<GroupRule>` is *not* the culprit — it lives in the `ArgumentSpec` (tier 1,
Arc-shared) and is a good memo key. The problem is the **contents state**:
`cx.session.derived_state(&cx.state, &delta)` is the deliberately-never-memoized seam,
so it mints a fresh `Arc<ParsingState>` per occurrence — which then becomes the *base*
key `GroupParser` passes to `group_interior_state`. Guaranteed memo miss + one
permanently-retained entry per optional argument. Measured (instrumented scratch copy):

| input | `derived()` calls | memo entries |
|---|---|---|
| `\item[a]` | 2 | 1 |
| `\item x` (argument absent) | 1 | 0 |
| `\item[a]\item[b]\item[c]\item[d]` | 8 | 4 |
| `{a}{b}{c}{d}` (four sibling brace groups) | 1 | 1 |
| `\item[a[b[c[d]]]]` | 5 | 4 |

Note row 2: an *absent* optional argument still pays a full state derivation for the
probe — and `\item` with no option is the common case in real documents.

**Suggested fix:** a second narrowly-typed keyed helper on the session for the
minted-contents derivation, keyed `(base, Arc<GroupRule>)` — both have stable `Arc`
identity (the rule is spec-held; `argument_state` is a plain `Arc::clone` of the loop
state whenever the spec carries no delta). Collapses the four-`\item` case from 8
derivations / 4 entries to 2 / 1 and makes the group-interior memo hit as a side effect.
Caveat: if the argument spec *does* carry a `parsing_state_delta`, `argument_state` is
itself freshly derived per invocation, so the chain only closes fully if a
`(base, spec)`-keyed argument-state memo lands too. This is the concrete,
measurement-backed case for the "`(base, Arc<ArgumentSpec>)`-keyed entry kind, strictly
profiling-driven" extension DESIGN_RATIONALE §3.6 left open.

**Free companion fix:** the optional parser's `Compute` group-child-state callback
(`argument_parsers.rs`, `keep_or_revert`) has a parameter named `contents` that actually
receives the group **interior** state at the descent site (`nodes_parser.rs` passes
`&cx.state`). Returning the *captured* `contents_state` instead keys nested same-rule
descents on `(contents_state, rule)` — an immediate memo hit (verified: all lib tests
pass unchanged; `\item[a[b[c[d]]]]` drops from 5 to 4 derivations) and fixes a
genuinely misleading name. Requires `Arc::clone(&contents_state)` in the closure.

## 4. `PrefixTable::first_chars` is dead code; `match_at` runs unguarded on every character

`src/token/prefix_table.rs`:

- `first_chars()` has **zero call sites** in the crate. Its rustdoc says it bounds
  "content-character runs" — machinery that was deleted when maximal-run `Chars` tokens
  were rejected in favor of single-character `Char(char)` (DESIGN_RATIONALE §3.2, §4).
  The field is computed, allocated, and cloned at every state freeze for nothing, and
  its doc describes a reversed decision.
- Meanwhile `match_at` — called for every content character via
  `detect_group_delimiter` — is `self.entries.iter().find(|e| rest.starts_with(…))`:
  ~8–14 string comparisons per character for a guaranteed miss on plain text.

Two coherent outcomes (leaving as-is is the worst option — a public API promising a use
that does not exist):

- **Wire it in** as the `match_at` first-character guard — the direct analogue of
  `TriggerChars::may_start`, which guards the specials hook the same way, and the shape
  DESIGN_RATIONALE §6 open question 1b anticipates. If kept: rewrite the doc and mirror
  the sibling API (a `may_start(c) -> bool` predicate, not a raw `&str` getter). This is
  the cheapest available win on the tokenizer's hot loop; the data is already built and
  cached per state.
- **Delete it** (field, accessor, test) and let §6.1b reintroduce a merged table when
  profiling demands it.

## 5. Reader membership tests are linear string scans, per character

`StdTokenReader` (`src/token/reader.rs`): `ws.chars.contains(c)`,
`rules.forbidden_chars.contains(c)`, `rule.name_chars.contains(c)` are all
`str::contains(char)` — substring search per character. The command-name scan is worst:
`\somemacro` pays a 52-char scan per name character. This is textbook per-state derived
data, and the file already has the pattern (`PrefixTable`, `TriggerChars` are computed
at freeze). A cached char-set (ASCII bitmap + fallback) for `whitespace.chars`,
`forbidden_chars`, and each `CommandRule::name_chars` slots into the same seam with no
API change. Do after item 6 confirms it matters. (Minor companion:
`rules.commands.iter().find(|r| c == r.escape_char)` is a per-token linear scan —
negligible for 1–2 rules.)

## 6. The benchmark obligation is unfulfilled

`criterion` is a dev-dependency, but there is **no `benches/` directory and no
`[[bench]]` section**. The per-invocation `Box<dyn ConstructParser>` was accepted with
an explicit "benchmark before Phase 6 closes" obligation (Phase6Execution.md §6.7).
Spec-side per-invocation cost inventory for the record: one `Box` (the factory) + one
`Arc::clone` of the spec into `CallableData` + one `Arc::clone(argument_spec)` per
declared argument. Note: DESIGN_RATIONALE's reassurance that "the dispatch loop can
special-case the default path without touching the trait" is currently not implementable
— the core cannot detect that a spec uses the default factory (no marker method, no
downcast); if Box elision is ever wanted, that detection mechanism must be decided
first. Also per-invocation: a `Vec<BuildId>` per provided argument, immediately
`extend_from_slice`d and dropped — the decided shape; revisit only if the benchmark
flags argument-heavy input.

## 7. Trivial

`#[inline]` on `Span`'s six tiny non-generic accessors (`new`, `empty`, `len`,
`is_empty`, `range`, `slice`) — free, idiomatic for a `Copy` value type, and required
for cross-crate inlining without LTO. `Span` is per-token hot.

## Suggested order

1. Item 6 (benchmark harness — a few representative documents, parse throughput) so the
   rest is measured, not guessed.
2. Item 4 (`first_chars`: wire in or delete — also removes a misleading public doc).
3. Item 1 (`Arc<PrefixTable>` reuse + `Arc<str>` rules data) and item 2 (`HashMap`
   memo) together — they share the retention decision.
4. Item 3 (optional-argument memo; apply the free `Compute` callback fix immediately).
5. Item 5 (char-set caches) if the benchmark shows tokenizer dominance.
