**Subphase 6.3 is complete: 220 tests + 1 doc test green** (baseline was 193 + 1), `cargo build` clean, `cargo doc` warning-free. The progress table in Phase6Execution.md is updated.

## What shipped

**Groups** — `GroupParser` (`src/constructs/group_parser.rs`), driven by the now-real `GroupOpen` arm of `NodesParser`: derived interior state carrying `expecting_group_close`, recursion with the exact `(group_type, close)` stop pairing (consume-at-match), `Group` node with span-backed delimiters, structural state revert. Recovery per the decisions: EOF inside → diagnostic + empty `close`; mismatched close inside → diagnostic + close **without consuming** (unwinding); root stray close → diagnose + skip, demonstrated by a root-driver test.

**`check_tree_invariants()`** (`src/node/invariants.rs`, exported publicly): structural sanity (single parent, reachability, BFS ranges), partition of `List`/`Group` interiors and the root, callable children-block contiguity + region tiling + content-parent subtree checks, and `Spanned` residency with exact positional relations. Applied retroactively: the test harness now sizes the root `List` to the parsed extent and runs the checker over **every** tree in the 6.2/6.3 suite, plus the node module's hand-built callable example (so the region path isn't dead code until 6.4).

**Child-state design session items pinned to 6.3** (DESIGN_RATIONALE §3.6):
- `ChildStateSpec { group, invocation }` with `Inherit`/`Fixed`/`Compute` — the `group` arm is live (tested with the chars-except-groups revert case and a per-class `Compute` policy); `invocation` is defined but consulted only from 6.4, as recorded.
- The session derivation seam, shipped whole: `Lang::SessionExt` + `Lang::observe_transition`, `ParserSession::derived_state` (observed, never memoized), and the ptr-keyed `group_interior_state` memo. Tests pin the doctrine: `{a}{b}` produces two observed transitions but **one** `finalize_transition` run, sibling interiors share one `Arc`, and pass-through policies preserve pointer identity.

## Deviations / judgment calls (flagged for your sign-off, none contradict a recorded decision)

1. **`GroupParser` takes `(open_span, rule)` and the dispatch arm consumes the trigger token.** The decided uniform `parse(&mut self, cx)` signature can't relate a stored `Token<'s>` to the context's reader lifetime (borrowck rejects it), and re-peeking inside the parser would re-tokenize under the policy base — exactly the desync the consume-flag amendment forbids. Arm-side consumption keeps the token consumed under the state that tokenized it.
2. **Root diagnose-and-skip lives in the root driver** (test harness now; `Language::parse` when it arrives in Phase 7) — there is no root-driver *type* in Phase 6 by decision 13. The skipped byte is dropped from the tree, so that one test is exempt from the checker (the "test aid, not builder law" clause anticipates this).
3. **`SessionExt` bounds are `Debug + Default + Send + Sync`** — the ARCHITECTURE sketch said `Debug + Default`; I added `Send + Sync` for consistency with the trait's "all associated types are Send + Sync" contract so sessions stay movable across threads.

If you're happy with these, the next session picks up **6.4 — Invocation dispatch + `make_invocation_parser` + `StdInvocationParser`**.
