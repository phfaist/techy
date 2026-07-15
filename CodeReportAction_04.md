# Action 04 — Panic policy in library code

**Status: RESOLVED (user decision + sweep executed, July 2026).** The recorded policy is
DESIGN_RATIONALE.md §3.8 ("Panic policy"); CLAUDE.md rule 4 was reworded to match. The
decision is *stricter* than this report's Option A: panics are allowed only for invariants
verifiable **independent of outer-layer behavior** — a documented-contract violation (e.g.
by a buggy hook or custom parser) returns an `Err`, so the builder's panic-on-caller-bug
policy (which this report recommended keeping) was **converted to `Result`**:
`NodeTreeBuilder::{add, add_with_ext, finish}` return `NodeBuildError` (all checks
always-on, including the previously debug-only tiling/spanned-content ones), lifted by
parsers into `ParseError`/`ImplementationError` aborts that bypass tolerant recovery.
Approved exception class: indexing-style accessors (`NodeTree::node`/`nodes_in`,
`Span::slice`, `TextContent::resolve`, `ChildRegion`'s resolved-only accessors) keep
documented panics with non-panicking companions (`NodeTree::get`, `Span::get`). The
`unreachable!`s and locally-guarded `expect`s below were kept; `test_node_stop` and the
invocation/body span read-backs were softened; `skip_whitespace` degrades gracefully;
`Span::len` saturates; the `prefix_table.rs` `expect` was removed structurally.

CLAUDE.md rule 4 says: *use `Result<T,E>` consistently, never panic in lib code (except
in unreachable paths)*. The codebase contains a handful of `unreachable!`/`expect`
invariant assertions plus one documented panic-on-caller-bug policy (the node builder).
Each was verified genuinely unreachable today, but they accumulated site by site without
a recorded convention, and reviews keep re-flagging them. Decide once, record it in
DESIGN_RATIONALE, and apply uniformly.

## Inventory

### `unreachable!` — invariant assertions (real panics in release)

| site | claim | verified? |
|---|---|---|
| `token/reader.rs`, `detect_group_delimiter`: `(None, None) => unreachable!("prefix table entries always carry a direction")` | `PrefixTable::for_rules`'s `add` closure is the only entry constructor and always fills one direction on a fresh entry; fields private, no public constructor | yes |
| `constructs/nodes_parser.rs:282` (flush arm) | same class — structurally impossible state | yes |
| `constructs/group_parser.rs`, `StopCause::NodeCondition` arm | `GroupParser` builds its `StopSpec` via `at_token(...)`, which sets `node: None`, so `NodesParser` can never return `NodeCondition` | yes |

### `expect` — invariant read-backs

| site | note |
|---|---|
| `constructs/nodes_parser.rs:375`, `test_node_stop`: `staged.get(id).expect("the node was just staged")` | the one non-debug `expect` on the hot path; graceful alternative: treat missing id as "condition did not fire" (`false`) |
| `token/prefix_table.rs`, `for_rules`: `entries.last_mut().expect("just pushed")` and `entry.delim.chars().next().expect("empty delimiters were skipped")` | **both removable at zero cost**: push-then-index by known position; `if let Some(c) = …` for the first-char loop |
| `constructs/invocation_parser.rs`, builder read-back `.expect("the child was just staged")` | established builder-readback idiom, covered by the builder's documented policy (below) |
| `token/reader.rs` (two `.expect("… checked above")`) | guarded by explicit bounds checks immediately above |

### Documented panic-on-caller-bug policies (sanctioned, keep)

- `NodeTreeBuilder` asserts (region tiling, staging order, content ranges) — release
  `assert!`s with descriptive messages, documented as the builder's contract. These are
  the enforcement mechanism for `ArgumentParser` implementor obligations.
- `NodeTree::node()` / `NodeRef::data()` raw indexing — panic on a foreign/out-of-range
  id, consistent with the builder policy. (A non-panicking `NodeTree::get(id) ->
  Option<NodeRef>` escape hatch for consumers holding ids of unknown provenance is a
  separate API addition, tracked with the read-API work.)

### Unguarded public functions (slice panics on bad input)

- `skip_whitespace` is `pub` (crate-root re-exported) and slices `content[pos..]` with
  no documented precondition — an out-of-range or mid-char `pos` panics with a bare
  slice-index message. Needs either a documented precondition or a debug_assert.
- `Span::slice()` panics on out-of-range/non-boundary spans; it is reachable from public
  node accessors via `TextContent::resolve` (e.g. a transform that re-sources a node
  without materializing content). The panic is documented; a non-panicking companion
  `Span::get(content) -> Option<&str>` is the rule-conforming addition (one immediate
  consumer exists in `node/invariants.rs`, which hand-rolls it).
- `Span::len()` is an unchecked subtraction: on an inverted span (constructible via the
  type's `pub` fields) it wraps to near-`usize::MAX` in release while `is_empty()`
  returns `false`, surfacing far away as an opaque slice panic. Options:
  `saturating_sub` (benign zero) vs `debug_assert!(start <= end)` in `len()` (names the
  violation where it matters). Judgment call: papering over vs failing loudly.

## The decision

Option A — **sanction the class** (recommended): record in DESIGN_RATIONALE that
*verified-unreachable invariant assertions (`unreachable!`, `expect` with a stated
invariant) are permitted in lib code and are exempt from rule 4*, on the grounds that
they assert structural impossibilities and a silent wrong answer would be worse. Then:

- keep the three `unreachable!`s and the guarded `expect`s as-is;
- still remove the two `prefix_table.rs` `expect`s (free, no loss of clarity);
- optionally soften `test_node_stop` to the graceful `false` (hot path, cheap);
- document `skip_whitespace`'s precondition;
- add `Span::get`; pick the `len()` hardening flavor;
- update CLAUDE.md rule 4's wording to match the recorded policy.

Option B — **strict**: replace `unreachable!`s with conservative fallbacks (e.g. fold
`NodeCondition` into the `EndOfInput` handling; treat a directionless prefix entry as a
miss) and all invariant `expect`s with graceful degradation. Cost: each site trades a
loud impossible-state failure for a silent behavioral quirk if the invariant ever breaks
— arguably worse for debugging, and it blurs real contracts.

The builder's panic-on-caller-bug policy stays either way (it is documented, load-bearing
for extension-author feedback, and consistent with arena-API conventions).

## Consequence

Once decided, reviews stop re-flagging these one at a time; new code has a rule to
follow. The mechanical sweep is < 1 hour either way.
