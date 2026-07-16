# Action 06 — Smaller standalone decisions

**Status: open — independent items, each needing a short decision or a contained fix.
None blocks the others.**

## 1. `SourceCursor` / `SourceContent`: superseded machinery in the shop window

Grepped crate-wide: `SourceCursor` appears only in `Source::cursor()` and its own
tests; `SourceContent` only in `source/content.rs`. The intended consumer — the
tokenizer — went another way: `StdTokenReader` holds `content: &'s str` and scans the
`str` directly. So the module doc's "forward scanning with mark/rewind" bullet
describes machinery nothing uses.

- `SourceContent` unused is *intended* (mmap backing deferred until a real need), but
  the deferral is oversold: `source.rs` claims the backing can later move behind the
  trait "without changing this type's public API", yet `Source.content` is a concrete
  `String` field and the change would add a second type parameter to `Source`,
  propagating through `SourceSpan<O>`, `SourceProvenance<O>`, `SourceResolver<O>`,
  `Diagnostic`/`ParseError`, and `ParseContext.source`. Soften the promise.
- `SourceCursor` is genuinely superseded, not deferred. Decide: (a) keep and re-label
  the doc bullet as "offered to embedders writing a custom `TokenReader`; the standard
  reader scans `&str` directly", or (b) retire it along with `Source::cursor()` (whose
  generic `C` parameter nothing exercises). ~130 lines either way.

## 2. Delete `impl Clone for ParsingState` (dead code, identity hazard)

`src/state/parsing_state.rs`. Verified: removing the impl still compiles the whole
crate (`cargo check --all-targets`). Two reasons to drop it:

- It is the only construct in the state layer that can produce two distinct `Arc`s that
  ought to be one (identity fork → memo misses; silent divergence between the state a
  node records and the one its siblings use). Everything else returns values and lets
  the engine own identity.
- `Arc<T>::make_mut` requires `T: Clone`, so the impl makes
  `Arc::make_mut(&mut cx.state)` compile — from inside `state/` that is an in-place
  mutation of a "frozen" state; removing `Clone` makes it un-writable.

If `Clone` is wanted for future transform APIs, keep it and say so in a doc line;
otherwise delete. (Explicit user sign-off requested since it is a code removal.)

## 3. `Diagnostics`: unbounded accumulation, and no way to render a collection

- **Unbounded growth**: `Diagnostics` is a bare `Vec` + `push`. In tolerant mode —
  the mode editors/linters use — degenerate input produces one diagnostic per byte
  (e.g. a file of forbidden chars); a 10 MB input can allocate on the order of a
  gigabyte of identical messages. Cheap fix: `Diagnostics::with_limit(n)` where pushes
  beyond `n` increment a `suppressed: usize` counter surfaced as "… and N more".
  A few lines; turns an unbounded-memory failure mode into a bounded one.
- **O(k·N) rendering**: `format_position` constructs a fresh `LineIndex` per call and
  drops it — `Source` caches nothing — so rendering k diagnostics over an N-byte source
  is O(k·N) (quadratic in document size for a fixed error rate), and provenance chains
  multiply it (one fresh index per hop, the parent document rescanned once per
  diagnostic in every included file). There is also *no API at all* to render a
  `Diagnostics` collection — callers must loop. Fix both at once:
  `Diagnostics::render_all()` (or a small `DiagnosticRenderer`/`PositionFormatter`)
  holding one `LineIndex` per distinct source, matched by `Arc::as_ptr` — the same
  pointer-keyed idiom the engine's memo uses. Keep `format_position` as the one-shot
  convenience. A cache on `Source` itself is blocked dep-free (`alloc` has no `Mutex`;
  `OnceCell` would cost `Sync`); the renderer is the right home.

## 4. Optional-group bracket protection: a documented precondition is missing

`OptionalGroupArgumentParser`'s brace-protection rule ("a brace group inside `[…]`
protects a `]` in its interior") works by reverting non-same-rule child groups to the
argument state, where `]` is an ordinary character. **If a preset registers `[`/`]` as
a real group class in the base rules** — a configuration the code's own comment
anticipates ("prepended so it wins ties against a same-spelling rule already in
scope") — the revert state still carries a bracket class and the guarantee silently
degrades. Measured with `[`/`]` added to base `TokenRules::groups` as a separate rule:

```
\item[{a]b}]  →  Tolerant: "unclosed group: expected '}'",
                 stray '}' escapes to the root loop        (mangled)
\item[a]      →  clean                                     (unaffected)
```

Inside the brace group, `]` tokenizes as a `GroupClose` with no matching open →
unwinding. Note the planned "temporary group rules, stripped in the derivation path"
mechanism does **not** cover this: stripping removes the *temporary* rule; the
offending rule here is a permanent one.

Decide: (a) minimum — record the precondition on `OptionalGroupArgumentParser` and in
DESIGN_RATIONALE §3.6 ("the minted close delimiter must not be a group delimiter of the
argument state, or brace protection degrades to unwinding"); (b) principled — make the
revert state actively suppress the close spelling (a design change to the revert-state
derivation). Also worth 4–5 lines in `child_state.rs`'s module header either way: the
file that owns the policy never mentions the bracket-balancing rule that is its only
production consumer, nor the documented one-bracket-level depth limit.

## 5. Project-doc drift (mechanical once confirmed)

Stale type names and shapes in the authoritative docs — each would misdirect a future
session:

- `ARCHITECTURE.md:392` — `ArgumentSpec<L> = { parser: ArgumentParserSpec<L>, … }`;
  `ArgumentParserSpec` no longer exists (it is `Arc<dyn ArgumentParser<L>>`).
- ARCHITECTURE.md, same `§specs` sketch block — the sketched `CallableSpec` lacks the
  `Send + Sync` supertraits; the factory signature is missing the `'s` lifetime the
  code carries; and `StdCallableSpec` is described as "the two structure specs +
  optional parser override" while the override lives on the *trait default*, not a
  field (the shipped substitution is fine — update the sketch).
- `NAMING_STRATEGY.md:16/42/148` — still name `ArgumentParserSpec` as current S1
  vocabulary (line 150 correctly records it as superseded);
  `NAMING_STRATEGY.md:89` — "standard parsers are preset-provided" (they shipped in
  the core, parameterized; the preset supplies one-liners in Phase 7).
- `CLAUDE.md:20` — "spec: Extensibility (MacroSpec, EnvironmentSpec, ContextDb)"; all
  three are superseded (`CallableSpec`/`StdCallableSpec`, `Library`/`LibraryStack`).
  `CLAUDE.md:31` still names `ArgumentParserSpec` as current vocabulary.
- `DESIGN_RATIONALE.md` — `TextContent` living in `source/` (S0) rather than `node/` is
  decided (ARCHITECTURE records it) and correct (it is the one Lang-free member of the
  node-payload vocabulary, depending only on `Span`), but the decision register never
  explains it — every review re-litigates. One paragraph in §3.5 closes it permanently.

## 6. Small diagnostics/API loose ends

- **`group_parser` diagnostic wording**: the mismatched/stray-close path emits
  `"unclosed group: expected '{close}'"` — same wording as the EOF path, pointing at
  the stray token. Distinct wording (`"mismatched group close: expected …"`) helps
  users tell the two recovery situations apart. Cosmetic.
- **`Severity::Note` and `Severity::Warning` have no producer** anywhere outside
  `error.rs`'s own tests, and `Severity`'s `Ord` (presumably for "warnings and above")
  has no consumer or test. Either wire severity levels to something or note the intent.
- **Test name collision with domain vocabulary**: `spec/mod.rs`'s
  `argument_parsers_and_state_deltas_have_a_slot` reads as if about `SlotSpec` (it
  declares `vec![]` slots; it is about the mid-granularity extension point). Rename,
  e.g. `custom_argument_parser_and_delta_need_no_invocation_parser`.
- **`Diagnostics` trait hygiene**: no `Extend`/`FromIterator` (no merging need today —
  note only).
