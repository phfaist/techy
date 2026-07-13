# Action 07 — Test backlog

**Status: open — no design decisions needed for tier 1; it can be implemented as a
batch. Tiers 2–3 are cheap insurance, batched by file. Items marked ⚠ depend on a
pending design decision and should wait for it.**

Convention notes: tests live in each file's `#[cfg(test)]` module (or the established
neighbor, e.g. `node/mod.rs` for the node topic). Several expected outcomes below were
verified by scratch experiment and are recorded so the test can assert them directly.

## Tier 1 — decided behaviors currently protected by nothing

These pin load-bearing contracts where a regression today passes the whole suite.

1. **Recovery always advances** (`token/error.rs` — the file has zero tests).
   Iterate `StdTokenReader` over a set of failing inputs; for each `TokenError` assert
   `recovery.resume_pos > err.span().start`, `resume_pos <= content.len()`, and
   `content.is_char_boundary(resume_pos)`. Include **multi-byte** cases: a multi-byte
   forbidden char (e.g. `→`) — the existing test uses `%`, so a bug writing
   `resume_pos: pos + 1` instead of `span.end` passes today — and a multi-byte escape
   char (e.g. `§`) dangling at EOF.
2. **`recovery: None` policy** (`nodes_parser` + `constructs/mod.rs`): a token error
   without a recovery token aborts even under `Recovery::Tolerant`, *without* a
   diagnostic. Zero producers in-crate (drive it via a custom `Lang::scan_specials`
   returning a bare `TokenError`); test both modes.
3. **`try_peek`'s documented contract — both arms** (nothing anywhere produces a
   tokenizer error at an argument-probe position; every construct test module hardcodes
   `forbidden_chars: String::new()`). Verified expected outcomes: with
   `forbidden_chars = "#"` and macro `\x{arg}`, input `\x#{a}`:
   - Tolerant: exactly two diagnostics, `["missing mandatory argument …",
     "character is forbidden here: '#' (U+0023)"]` — one report, not two, not zero;
     tree invariants pass; reader ends at 6.
   - Strict: aborts with `ParseErrorKind::Token(ForbiddenChar { ch: '#' })`.
   Also: a probe token error inside an *optional* argument (the one call site where
   `try_peek` runs under a swapped derived state).
4. **Freeze-after-finalize ordering** (`state/parsing_state.rs`): a `Lang` whose
   `finalize_transition` pushes a `GroupRule`, asserting
   `derived.prefix_table().match_at("$x").is_some()`. Verified: swapping the
   finalize/freeze lines passes every existing test today — this is the only test that
   would catch it.
5. **`TriggerChars` cache invalidation** (`state/parsing_state.rs`): a delta toggling
   `enable_specials` off **and back on** re-bakes the filter; and a `Lang` whose
   `specials_trigger_chars` reads `data.libraries`/`data.ext`, showing a
   `push_library`/ext delta rebuilds it. The `TriggerChars` half of the
   cache-invalidation design is currently entirely untested.
6. **Whole-tree verbatim recomposition** (`node/` — validates the read API's central
   claim and prototypes the Phase 7 traversal): a recursive pre-order leaf walk over
   parser-produced trees asserting
   `concat(leaf.span_content()) + callable post-spaces == source`, run over the
   `nodes_parser`/`environment_parser` corpus inputs.
7. **`scan_specials` returning `Err`, end-to-end** (`token/reader.rs`): the `?` at the
   specials scan is the sole implementation of "scanner errors participate in the
   recovery protocol" and nothing drives it — with recovery (tolerant adopts placeholder
   + resumes) and without (see item 2).

## Tier 2 — untested decided behaviors, by file

**`constructs/child_state.rs` nesting shapes** (behavior verified correct by trace;
needs pinning — two of these are currently covered only by a test that is really about
`unwrap_lone_group`):
- `\item[a{b]c}d]` — brace protection in a non-lone position (children: chars `a`,
  group `{b]c}`, chars `d`; 0 diagnostics).
- `\item[{[}]` — a protected *open* bracket (first thing to break if the revert state
  ever retains the minted rule).
- ⚠ `\item[a[{x]y}b]` — the decided depth-2 pitfall ("mangles with diagnostics":
  `unclosed group: expected '}'`, stray `}` escapes to root). Blocked on a harness
  variant: `try_run` asserts `stop == EndOfInput`, and this input stops with a root
  `UnexpectedGroupClose`.
- `\item[a[\m{b]c}]]` — `InvocationChildState::Fixed` does *not* ride into a nested
  bracket level (where the revert stops applying).

**`token/token.rs`** (no test module):
- `post_space()` returns the empty span **at `span.end`** for every non-post-space kind
  (the position is recorded verbatim into callable nodes; a misplaced empty span
  resolves to `""` and slips past every existing assertion).
- The `PartialEq` asymmetry: `Specials` specs compare by `Arc::ptr_eq` (equal-but-
  distinct Arcs ⇒ unequal); `GroupOpen` rules compare structurally (distinct-but-equal
  Arcs ⇒ equal). A silent flip would quietly change what every
  `assert_eq!(token.kind, …)` in the suite means.

**`source/span.rs`** (contract almost entirely untested):
- multi-byte slice: `Span::new(0, 2).slice("éx") == "é"`;
- `#[should_panic]` non-char-boundary slice: `Span::new(0, 1).slice("é")`;
- `#[should_panic]` out-of-bounds slice;
- boundary case `Span::empty(content.len()).slice(content) == ""` (EOS spans sit there);
- `#[cfg(debug_assertions)]` inverted-span constructor assert (`Span::new(7, 3)`).

**`token/prefix_table.rs`**:
- cross-rule open/close merge — string `X` opens rule A and closes rule B — asserting
  the merged entry count (`entries().len() == 1`); the merged entry type's raison
  d'être, untested;
- earlier-rule-wins for the **close** slot (only the open slot is tested);
- multi-byte UTF-8 delimiters (e.g. `«…»` vs `$$` — byte-length sorting + the reader's
  `pos + delim.len()` arithmetic);
- strict-prefix miss: only `$$` registered ⇒ `match_at("$x") == None`;
- `enable_groups: false` at unit level (`entries()` **and** `first_chars()` empty);
- the documented `entries()` ordering (longest first, ties keep declaration order).

**`token/reader.rs`**:
- forbidden-char priority: a forbidden `$` that is also a group delimiter must still
  tokenize as `GroupOpen` (the check is deliberately last; the interesting half of the
  priority is unexercised);
- `TriggerChars::Any` through `peek` end-to-end (the fully-dynamic-scanner path);
- two `CommandRule`s sharing an escape char ("earlier entries win");
- comments with `enable_whitespace: false` (comment gets empty post-space; the
  terminating `\n` surfaces as a `Char` token — a decided interaction);
- `expecting_group_close` with an empty `close` (the `!close.is_empty()` guard is
  load-bearing against a zero-width `GroupClose` loop);
- `move_past(tok, false)` on a `Comment` (the other post-space-carrying kind);
- a paragraph break at byte 0 through `peek`.

**`token/list_reader.rs`**:
- the documented `\verb`/in-span fidelity divergence: at a `move_past(tok, false)`
  position inside a token's span, assert `TokenListReader` yields the *following*
  listed token AND that `StdTokenReader` on the same content differs — turning the
  prose contract into an executable one.

**`node/invariants.rs`** (the checker's hardest code has zero negative tests):
`#[should_panic]` tests staging a deliberately-broken **callable**: (a) a region-tiling
gap; (b) a post-space whose `end` misses the first child; (c) a node claimed by two
children ranges; (d) an unreachable node. Also no *positive* callable-bearing tree is
built in the file itself.

**`source/resolver.rs`**:
- the multi-file end-to-end test the seam exists for: map-resolve `a.tex`, resolve
  `b.tex` *from a span inside `a`*, assert the 3-element `provenance_chain()` and the
  two-line `Diagnostic::render()` output (also first coverage of a resolver attaching
  an origin via `with_origin`);
- small: `From<BTreeMap>`, `insert` replaces, `ResolveError::message()`, object safety
  (`&dyn SourceResolver<…>`) pinned by a compile test.

**`spec/`**:
- `Send + Sync` static assertions for `Arc<dyn ArgumentParser<L>>`, `ArgumentSpec<L>`,
  `SlotSpec<L>`, `StdCallableSpec<L>` (the module declares thread safety a core
  contract; nothing fails today if a supertrait is dropped);
- flyweight sharing: `Arc::ptr_eq` after `ArgumentSpec::clone`; one spec `Arc`
  registered under two names;
- `make_invocation_parser` default and override exercised *in the spec module* (the
  trait's entire behavioral surface — currently only indirect coverage);
- `StdCallableSpec::default()`/`Clone`, `SlotSpec::default()`/`with_state_delta`.

## Tier 3 — smaller gaps

- **`error.rs`**: `ParseError` has zero tests in its own file (`render()`, `span()`,
  `kind()`, `Display`); `ParseErrorKind::Token(..)`'s `Display` never tested; custom
  line/column offsets (`with_line_column_number_offsets`) never exercised through
  `format_position` — the feature's whole point; `Severity` `Display`/`Ord`;
  `Diagnostics` `as_slice`/`Default`/both `IntoIterator`s; `Diagnostic::note`. Nit: the
  fallback test hardcodes the 100 000-byte scan threshold and misnames a repeat count
  `DEFAULT_MAX_TEST_LEN`.
- **`invocation_parser.rs`**: (a) zero-declared-arguments callable asserting
  trigger-only span + `post_space` (`\x` in `\x y`: `span == 0..2`, `post_space == ""`,
  sibling `" y"`); (b) the single-character command headline case (`\& b`: empty
  post-space, trailing space becomes sibling content) — the exact behavior the
  post-space amendment turns on, only tested indirectly. Multi-char variant `\x y`
  asserting `post_space == " "`.
- **`nodes_parser.rs`**: `consume = true` for `ParagraphBreak`/`Predicate` stop kinds
  (post-space handling differs); node stop condition firing on a dispatched
  *invocation* (groups are covered); the deferred unresolvable-command diagnostic when
  a node condition pre-empts it (assert deferred, not lost).
- **`engine/mod.rs`**: the nested memo case — a returned interior state subsequently
  passed back as a `base` (the reuse path the ptr-key soundness argument rests on);
  assert `observe_transition` receives the correct `prev`/`new` (tests only count).
- **`state/parsing_state.rs`**: "exactly one `finalize_transition` run per derivation"
  with a counting `Lang` (delta carrying two events + rules overrides ⇒ one run); a
  delta pushing **two** libraries pinning the documented innermost ordering; a
  *normalizing* `Lang` through the seed path (`initial().derived(&empty)` vs
  `initial()` — the seed-coherence contract's observable shape).
- **`state/lang.rs`-adjacent**: a specials trigger colliding with a group delimiter /
  escape char / comment start (documented silent-never-fires precedence);
  `ArgumentExt` non-`()` somewhere (declared and plumbed, never exercised).
- **`argument_parsers.rs`**: a comment appearing in argument noise while comments are
  disabled by the argument's delta (noise-scan token classified under the argument
  state — believed fine, untested corner).
- **`group_parser.rs`**: optionally make the file self-covering (unwind, strict aborts,
  `with_child_states`, nested groups, ambiguous-`$`) — all covered today by
  `nodes_parser` integration tests, so lowest priority.

## Suggested batching

Tier 1 as one PR-sized change (items 1–7, ~15 tests, no production-code changes except
possibly a tiny test-lang helper). Tier 2 by file as touch-opportunities arise or as a
second batch. Tier 3 opportunistically. The ⚠ depth-2 pitfall test needs the harness
variant first; write it together with the `child_state` decision work.
