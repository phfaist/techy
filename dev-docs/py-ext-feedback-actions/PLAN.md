# PY-bindings feedback — implementation plan

**Provisional.** This folder is working material for acting on
`dev-docs/extra/PY_EXT_BINDINGS_REPORTED_GAPS_WISHES.md` (the gap report written by the
Python-bindings project). Delete the whole folder once the plan has been executed and
its decisions are recorded in DESIGN_RATIONALE/ARCHITECTURE.

**Status: all rulings received (2026-08-10, three rounds); plan COMPLETE.
Execution starts only on the user's explicit signal** — a parallel agent is still
working on the dev-docs cleanup; do not begin Stage 1 until told to.
The descent-guard work is merged into main (`1659057`; record `[§dd-dr:descent-guard]`),
so the original precondition is satisfied. All file:line references below were
re-verified against merged main (`69038d8`): still, **re-locate by symbol name at
execution time, never by line number**.

**Active constraint:** a parallel agent is currently cleaning up
`dev-docs/ARCHITECTURE.md` and `dev-docs/DESIGN_RATIONALE.md`. **Do not touch those two
files** until that work completes. Everything in this plan that writes rationale
entries lives in Stage 5 and waits for it.

**Dev-docs content rules (user directive):** both documents now carry maintenance
rules at their tops (`[§dd-arch:self-meta]`, `[§dd-dr:self-meta]`) dictating what
content may appear — **obey them strictly**. In particular: ARCHITECTURE describes
the present day only and **never** references temporary project plans (this plan
included), benchmark results, or execution phases; register entries carry a Status
and a "Revisit if" clause, no dates in the entry body; every new register entry gets
its ARCHITECTURE reference **in the same change**; labels are immutable addresses.

**Execution conduct (user directives):**
- Work on this worktree/branch (`worktree-py-ext-feedback-plan`); **commit liberally**
  as work proceeds — small, per-item commits, messages prefixed `pyext-<item>:`
  (e.g. `pyext-1.3: …`), so any interruption loses minutes, not stages.
- **Record progress in `PROGRESS.md`** (this folder) as items complete, and always at
  stage boundaries: mark item status, note any deviation from this plan and any
  execution-time micro-ruling taken, and keep a one-line "resume here" pointer
  current. A fresh session must be able to continue from `PROGRESS.md` + git log
  alone.
- After each stage: `cargo build`, `cargo test`, `cargo docs` (with
  `rm -rf target/doc` when verifying links). Stage 3 is the one deliberately
  breaking stage — land it as a single unit so implementors (the bindings included)
  adapt once. The API freeze is still soft — this plan is the sanctioned
  fine-tuning window.

Naming placeholders flagged below get a [§dd-arch:naming] check at execution; final
call is the user's where flagged. `HookFailed` is ruled (no longer a placeholder).

---

## Stage 1 — bug fixes

### 1.1 `LineIndex` multi-byte resume bug

- **Where:** `techy/src/source/line_index.rs`, `extend_line_starts_up_to`.
- **Bug:** the function records `computed_end = up_to + 1` after a scan, and the next
  call resumes by slicing `self.content[computed_end..]`. When the byte at `up_to`
  begins a multi-byte UTF-8 character, `up_to + 1` is inside that character and the
  slice panics. `display_tree` always asks for offset 0 first, so any document whose
  *first* character is multi-byte (`"é"`, `"—x{y}"`, `"😀"`) panics the pretty-printer.
- **Fix (~4 lines):** keep `computed_end` on a character boundary — after the scan
  loop, advance `new_computed_end` forward to the next boundary
  (`while new_computed_end < content.len() && !content.is_char_boundary(new_computed_end)`),
  clamped to the scan window (`end_at`). (`str::ceil_char_boundary` is still unstable;
  write the loop.)
- **Tests:** `display_tree` over `"é"`, `"—x{y}"`, `"😀"`; direct `LineIndex` sequence
  `line_col(0)` then `line_col(k)` on multi-byte content; agreement check between
  incremental queries at every byte offset and a fresh index over the same content.
- **Courtesy note for the bindings project on completion:** their
  `test_display_contains_techys_multibyte_renderer_panic` ratchet test is designed to
  fail when this lands, so they can remove their containment.

### 1.2 `ChildRegion::staged()` becomes public

- **Where:** `techy/src/node/arguments.rs` — `staged()`; public constructors
  `new`/`single`; panicking `resolved()`.
- **Why:** `ChildRegion::new`/`single` publicly construct *staged* regions, and the
  three read accessors (`children()`, `content_range()`, `content_parent()`) panic on
  a staged region while the non-panicking companion `staged()` is `pub(crate)` — the
  panicking accessors lack the public non-panicking companion the panic-policy
  register shape requires.
- **Fix:** flip `staged()` to `pub`; add the missing **Panics** sentence to the three
  accessors (Panics sections are kept exhaustive), pointing at
  `is_resolved()`/`staged()` as the guards.
- **Tests:** `staged()` answers on a freshly constructed region; `None` on a region
  read from a finished tree.

### 1.3 Include-chain error names the reference as written

- **Where:** the `UnresolvableSourceReference` condition's message formatting (locate
  by identifier; reached via `check_include_chain` / resolver failures).
- **Why:** the rendered message names the *resolved origin*, not the reference string
  the document actually wrote, so the diagnostic points at a name the user never
  typed. The structured field already carries the right value; only the message is
  wrong.
- **Fix:** one format string: `cannot resolve source reference '{reference}': …`,
  keeping the resolver's own error message as the detail tail.
- **Tests:** update the rendering assertion; one case where the resolver rewrites the
  reference so written-vs-resolved differ.

### 1.4 Recompose oracle falsifiability test

- **Where:** `techy/tests/recompose_oracle.rs` (test-only; zero API surface).
- **What:** one added test: parse an input, drop a middle node via
  `transform::restage`, regenerate, and assert the reemission equals the surviving
  nodes' recorded content only — the check a span-copying recomposer fails and the
  payload-driven contract passes. (Today every equality assertion is also satisfied
  by the forbidden span-copying implementation.)

### 1.5 `format_position_with` false-cause message

- **Where:** `techy/src/error.rs` (message near :853; assertion near :1333).
- **What:** render `@ char pos N (no line info)` — drop
  `: line-index scan limit exceeded`, which is only one of several reasons a
  `LineColProvider` answers `None` and is untrue for every consumer-supplied provider.

### 1.6 `Package::scan_specials` bounds — return an error

- **Where:** `techy/src/scopes/mod.rs` — `let rest = &content[pos..];` in
  `Package::scan_specials`.
- **Ruling:** invalid `pos` (out of range, or not a character boundary) returns an
  `Err`, in the spirit of `implementation_error` — the caller violated the method's
  contract; no panic, no silent no-match.
- **Fix:** guard with `content.get(pos..)`; on `None`, answer the error path of the
  existing `TokenResult` return. *Execution detail:* the alias errs with
  `TokenError<'s, L>` — pick the token-layer error spelling that carries the
  contract-violation meaning (add a small variant/condition there if none fits;
  mirror `ImplementationError`'s wording, whose meaning — "extension contract
  violation" — is exactly right here). Document the contract on the method.
- **Tests:** out-of-range `pos`; mid-character `pos`; valid boundary positions
  unaffected.

### 1.7 Drop the unused `criterion` dev-dependency and the README `cargo bench` line (ruled 2026-08-10)

- **Where:** `techy/Cargo.toml` (`criterion = "0.5"`), README's `cargo bench`
  mention. There is no `benches/` directory; the documentation mismatch is what
  costs readers time.
- **What:** remove both. A benchmark corpus remains a separate, future decision.

---

## Stage 2 — API additions (non-breaking)

### 2.1 `+ Any` on the four remaining dyn-held extension traits

- **Where:** `SpecsProvider` (`scopes/mod.rs`), `ArgumentParser`
  (`spec/structure.rs`), `EnvironmentBehavior` (`latexlike/environments.rs`),
  `SourceResolver` (`source/resolver.rs`). Precedent with rationale paragraph:
  `CallableSpec` (`spec/callable.rs`).
- **Change:** append `+ Any` to each supertrait list; adapt `CallableSpec`'s
  rationale sentence (a consumer recovers the concrete type from a stored
  `Arc<dyn _>` / `&dyn _` by downcasting). Scope discipline: no `Debug` on
  `SourceResolver`, no `behavior_arc()` accessor — supertraits only.
- **Semver note:** formally a breaking bound for impls on non-`'static` types; every
  storage site is `Arc<dyn _>` (already `'static`), accepted. Record in the
  api-baseline pass.
- **Tests:** one downcast round-trip per trait.

### 2.2 `Language::new` accepts an `Arc`'d state

- **Where:** `engine/language.rs` (post-descent `Language::new` now also seeds
  `descent_guard_init: Default::default()` — the change is confined to the
  `initial_state` parameter).
- **Change:**
  `pub fn new(driver: L::Driver, initial_state: impl Into<Arc<ParsingState<L>>>)`,
  body `initial_state.into()`. Every existing by-value call site compiles unchanged
  (`Arc<T>: From<T>`). Closes the one place where the identity rule for states
  (states are shared by handle; a data-equal copy is a different state) and the
  constructor signature contradicted each other.
- **Docs:** one sentence: passing the shared handle preserves the state's identity.
- **Tests:** seed from an `Arc` obtained from a parsed node's state; assert
  `Arc::ptr_eq(language.initial_state(), &that_arc)`.

### 2.3 `NodeTree::slice(range)` — validated constructor

- **Where:** `node/tree.rs`, beside `covering_slice`. `NodeSlice::new` stays
  `pub(crate)`.
- **Why not a visibility flip:** `NodeSlice` promises a *contiguous run of sibling
  nodes* (span-contiguity, the extract helpers, `span()`/`source_text()` all lean on
  it). An arbitrary range over the flat storage can cross parent boundaries, so a
  public constructor must validate.
- **Change:** `pub fn slice(&self, range: Range<u32>) -> Option<NodeSlice<'_, L, A>>`.
  `Some` iff the range is a sibling run. O(1) check via the parent table: let
  `p = parent(range.start)`; the range must lie within `p`'s children range
  (children ranges are contiguous, so containing the endpoints contains everything
  between). Root special case: a range containing index 0 is only valid as `0..1`.
  Settle at execution: in-bounds *empty* ranges answer `Some` (mirrors what the
  accessors can hand out — empty child runs are real).
- **Docs:** state the sibling-run requirement; `None` for in-bounds non-runs as well
  as out-of-bounds ranges.
- **Tests:** round-trip `tree.slice(node.children().range())`; argument/slot content
  ranges; cross-parent range → `None`; root ranges; empty ranges; out of bounds.

### 2.4 `MacroSpec::with_after_effect`

- **Where:** `latexlike` (`MacroSpec`); the working implementation exists twice as
  `#[cfg(test)] AfterEffectSpec` (`latexlike/input.rs`, `engine/language.rs`).
- **Why:** a macro invocation can leave a parsing-state change behind for its
  *following siblings* (the way `\newcommand` makes a name usable for the rest of the
  group). The machinery is fully shipped — `input_macro_spec`'s `persist_state`
  parameter, the delta-merge path — but no public spec can produce such an effect.
- **Change:** builder-style
  `MacroSpec::with_after_effect(self, delta: ParsingStateDelta<LLL>) -> Self`; the
  invocation parser returns the delta as the sibling after-effect. **Post-descent
  note:** the pass-through after-effects are now boxed
  (`Option<Box<ParsingStateDelta<L>>>` in `NodesOutcome`/`parse_construct`) — follow
  the boxed shape. Source the plumbing from the two test copies, then delete them in
  favor of the public path. Name check against [§dd-arch:naming] at execution.
- **Tests:** through the public API: an after-effect changes state for following
  siblings; the two-effects merge case; `persist_state` on/off through
  `input_macro_spec`.

### 2.5 `TreeViolationKind::as_str` and `TokenKind::as_str`

- **Where:** `node/invariants.rs` (`TreeViolationKind`), token module (`TokenKind`);
  pattern: `NodeKind::as_str` (`pub const fn`, variant name).
- **Why:** both enums are consumed by name in logs/config/matching;
  `TreeViolationKind` is `#[non_exhaustive]`, so hand-written name tables silently
  rot when a variant is added or renamed. `TokenKind`'s generics don't affect a
  variant-name projection.
- **Tests:** exhaustive-match test per enum keeping names in step.

### 2.6 `NodeTree::tree_tag()` becomes public

- **Where:** `node/tree.rs`; `NodeId::tree_tag()` is already public.
- **Change:** flip to `pub` + one doc sentence naming the pre-check use (checking a
  `NodeId` against a tree before an always-on assert).

### 2.7 `TreeViolation::new`; drop `no_constructor` from `MalformedBegin`

- **Where:** `node/invariants.rs` (`TreeViolation`, `#[non_exhaustive]`, public
  fields); the `MalformedBegin` condition's derive attribute.
- **Change:**
  `TreeViolation::new(node: Option<NodeId>, kind: TreeViolationKind) -> Self`
  (keeps `#[non_exhaustive]` field-growth freedom) — consumers of `validate_tree`
  can now manufacture values to test their handling code. Remove `no_constructor`
  from `MalformedBegin` so the derive emits `new()` like the other 24 shipped
  conditions.
- **Tests:** construct-and-match round trip; `MalformedBegin::new()` compiles in a
  doc-test.

### 2.8 `DiagnosticInfo::identifier()` — runtime condition identity

- **Where:** `error.rs` — `DiagnosticInfo` (const `IDENTIFIER`), blanket
  `DiagnosticData` impl.
- **Change:** add `fn identifier(&self) -> &str { Self::IDENTIFIER }` to
  `DiagnosticInfo`; the blanket `DiagnosticData` impl forwards to the *method*
  instead of the const. All 25 shipped conditions and every existing consumer are
  untouched.
- **Docs (per ruling):** state clearly that overriding this method is for the
  exceptional case where a compile-time identifier is impossible — concretely,
  binding/embedding adapter types (e.g. Python-defined conditions carried by one
  Rust adapter type). Everything else keeps the const; the const-identifier
  discipline remains the norm. Update the sealing comment on `DiagnosticData` to
  match (it currently says the seal "enforces the const-identifier discipline").
- **Tests:** an adapter type overriding `identifier()` per instance; shipped
  conditions still answer their consts; `T::IDENTIFIER` matching unaffected.

---

## Stage 3 — hook fallibility sweep (the one breaking stage)

Ruling (2026-08-10): **Tier A and Tier B hooks become fallible; Tier C stays
infallible and gets rationale docs.** Full roster and analysis:
`HOOK_FALLIBILITY.md` (this folder). Land the whole stage as one unit — `ParseDriver`
already took a breaking change in the descent merge (`type DescentGuard`); this is
the second and last planned sweep over these traits, so implementors adapt once.

### 3.0 New condition: `HookFailed` (name **ruled** 2026-08-10)

There is **no existing general error for "consumer-supplied hook code failed
operationally"**: `ImplementationError` means, verbatim, "extension contract
violation" — the wrong statement for an I/O failure or a runtime exception in
embedder code; `ResolveError` is resolver-specific plumbing, not a condition.

- Add one condition struct via the `DiagnosticInfo` derive:
  `HookFailed { detail: String, cause: Option<Arc<dyn Error + Send + Sync>> }`
  (message: `extension hook reported a failure: {detail}`). The cause-chain field
  is **ruled in from the start** (2026-08-10) — the derive emits `new()` over all
  fields, so adding it later would have changed the constructor signature. Field
  shape follows `ResolveError`'s.
- Name rationale on record: `ExtensionError` rejected (collides with the
  `NodeExt`/`StateExt`/`SessionExt` *extension-data* vocabulary); `OperationalError`
  rejected (vague; Python DB-API flavor); `HookFailed` fits the register's
  event-style names (`StrayGroupClose`, `MalformedBegin`, `DescentLimitExceeded`).
- Hooks err with `ParseError<L::SourceOrigin>` carrying whichever condition fits:
  `HookFailed` for operational failures, `ImplementationError` for genuine contract
  violations, any domain condition for document diagnoses. The distinction goes in
  each hook's docs.

### 3.1 Tier A signatures

| Hook | New signature (target) |
|---|---|
| `GroupChildState::Compute` | `&dyn Fn(&Arc<ParsingState<L>>, &Token<'_, L>) -> Result<Arc<ParsingState<L>>, ParseError<L::SourceOrigin>>` |
| `InvocationChildState::Compute` | same, over `&Invocation<'_, '_, L>` |
| `Lang::initial_state_data` | `fn initial_state_data() -> Result<StateData<Self>, FinalizeError>` |
| `ParseDriver::resolve_command` | `-> Result<CommandResolution<L>, ParseError<L::SourceOrigin>>` (also `CommandResolver::resolve_command`, same change — keep the pair in step) |
| `ParseDriver::resolve_state_event` | `-> Result<Option<ParsingStateDelta<L>>, ParseError<L::SourceOrigin>>` |

Ripples to plan for:

- **`initial_state_data`** is called by `ParsingState::lang_initial()`
  (`ParsingState::freeze(L::initial_state_data())`) — so `lang_initial` and
  `lang_initial_with_packages` become `Result`-returning. **Ruled OK 2026-08-10**
  (soft freeze; this is the fine-tuning window). Every seed call site adds a
  `?`/`expect`; update `TrivialLang`, the presets, guides, and doc examples in the
  same commit.
- **Compute arms:** the natural body calls `derived()` (fallible, `DeriveError`);
  give `DeriveError` a documented lift into the parse-side error at the call site
  (check what the in-crate callers of `derived()` do today and match it).

### 3.2 Tier B signatures

| Hook | New signature (target) |
|---|---|
| `TokenStopKind::Predicate` | `&dyn Fn(&Token<'_, L>) -> Result<bool, ParseError<L::SourceOrigin>>` |
| `StopSpec::node` (node-based stop) | same shape |
| `ParseDriver::make_nodes_parser` / `make_group_parser` / `make_invocation_parser` | `-> Result<Box<dyn ConstructParser<…>>, ParseError<L::SourceOrigin>>` |
| `CallableSpec::make_invocation_parser` | same |
| `EnvironmentBehavior::body_state_delta` | `-> Result<Option<ParsingStateDelta<LLL>>, ParseError<LLL::SourceOrigin>>` |
| `Lang::make_node_ext` | `-> Result<NodeExt<Self>, _>` — error type at the **builder** level (it also runs for consumer-built trees, where no parse/span context exists): a `NodeBuildError` variant or equivalent; parse paths lift it the way they already lift `NodeBuildError`. Settle at execution. |
| `ParseDriver::observe_transition` | gains `diagnostics: &mut Diagnostics<L::SourceOrigin>` **and** `-> Result<(), ParseError<L::SourceOrigin>>` |

Notes:

- **Factory `Err` means "could not build the parser"** — depth refusal stays the
  descent guard's business (`DescentLimitExceeded` via `parse_construct`); keep the
  two meanings distinct in the factory docs.
- **`observe_transition` dual channel, document the roles:** the sink records
  document-level observations/diagnoses *without* affecting the parse (recording an
  error-severity diagnostic does not abort — that is `recover`'s business); `Err`
  aborts the parse for a truly problematic state. The sink qualifies because
  `Diagnostics` is already public API in this exact position
  (`observe_parse_start`).

### 3.3 Tier C — infallible, documented

For every Tier C hook (`recovery`, `refine_diagnostic`, `make_paragraph_break_node`,
`source_resolver()`, `specials_trigger_chars`, `ComposePiece::append`,
`LineColProvider::line_col`): one rationale sentence on the hook stating the
infallibility is deliberate and why (see HOOK_FALLIBILITY.md Tier C table), plus the
recommended course of action for embedding/binding code whose implementation can
still fail (report through the embedding's own channel and answer the documented
neutral value: pass the payload through unchanged, answer the default node, answer
the conservative superset, answer `None`).

### 3.4 `ParseContext::stage_invocation`: error, not panic (ruled 2026-08-10)

- **Where:** `constructs/mod.rs` — `stage_invocation` panics on a bad computed span.
- **Change:** return the error path of its existing `ConstructParserResult` instead
  (it already lifts `NodeBuildError`); a bad computed span is a contract violation
  by outer code — `ImplementationError` semantics, exactly the panic-policy rule-4
  shape. Rides this stage since it sits beside `parse_construct` post-descent.
- **Tests:** a construct parser producing an invalid span gets an `Err`, not a
  panic; tolerant recovery does not swallow it (the `implementation_error`
  contract).

### 3.5 `ParseDriver::diagnostics_limit()` (ruled 2026-08-10)

- **Why:** `Diagnostics::with_limit` is public, documented, and tested, but
  `ParserSession` hard-codes `Diagnostics::new()` — no parse can use the cap.
- **Change:** defaulted `fn diagnostics_limit(&self) -> Option<usize> { None }` on
  `ParseDriver`, beside `recovery()`; `ParserSession` seeds its `Diagnostics` from
  it. Non-breaking (defaulted); rides this stage's trait touch.
- **Tests:** a driver with a limit gets a capped `Diagnostics` out of a parse.

### 3.6 `ParseResult` returns the session extension (ruled 2026-08-10)

- **Why:** `observe_transition`'s docs direct parse-history accumulation into
  `Lang::SessionExt`, and `ParserSession::finish` drops it — the documented purpose
  is unreachable from outside. The 3.2 sink covers diagnostics, not data.
- **Change:** `ParseResult` gains the session extension value (public field or
  accessor — match `ParseResult`'s existing style; execution detail). Breaking-ish
  (adds `L::SessionExt` to the result type), which is why it rides the one breaking
  stage.
- **Tests:** an `observe_transition` implementation accumulating into `SessionExt`
  reads its data back off the `ParseResult`.

### 3.7 Stage gates specific to this sweep

- **Hot-path size check:** the `Predicate`/`Compute`/`StopSpec::node` callbacks are
  `&dyn Fn` — dynamically dispatched, so the `Result` channel is *not* optimized out
  there (unlike the statically dispatched `Lang`/driver hooks, where an infallible
  monomorphized body folds the channel away). Check
  `size_of::<Result<bool, ParseError<…>>>` and the parser loop frames; if the `Err`
  arm bloats them, box it (`Box<ParseError>`) — the descent merge boxed the
  pass-through deltas for exactly this reason; follow that discipline.
- Update every in-crate implementor (`TrivialLang`, `LatexlikeDriver`,
  `StdCallableSpec`, preset behaviors), both guides, and the AI-guide examples in
  the same unit.
- api-baseline: this stage is the bulk of the breaking-change record.

---

## Stage 4 — documentation batch

One or two sentences each unless noted. (The two items previously blocked on the
descent merge are now writable.)

1. `Language::parse` (+ `Source::new`): each parse of a bare `&str` creates a fresh
   source identity; spans from different calls never compare equal even for equal
   text.
2. `GroupRule` (**ruled 2026-08-10: keep the `PartialEq` derive, document clearly**):
   state on the type that sharing (`Arc` identity, `Arc::ptr_eq`) is what the
   temporary-group reuse checks and the derivation caches compare — the derived
   structural `==` answers a different question and is not that test. Cross-reference
   from `TokenRules::groups`.
3. `ParseDriver`: name the intra-trait dispatches per method (`recover` calls
   `refine_diagnostic` and `recovery`; `make_invocation_parser` delegates to the
   spec) + a short "wrapping a driver" paragraph — a delegating driver that forwards
   `recover` silently disables its own `refine_diagnostic` override.
4. Guide prose fix: `stage_node` returns a `Result` (`BuildId` or `NodeBuildError` to
   lift via `implementation_error`), not a bare `BuildId`.
5. `ParseContext::session` field doc: describe what an implementor can actually reach
   (diagnostics, ext, `snapshot_frames()`), not internal machinery. (Write against
   the post-descent `ParseContext`.)
6. Replay-granularity clarification in the custom-language guide: the collapsed
   transition is the forwarding construct's, not the included run's; the parenthesis
   is about sibling after-effects, not descents. Add the three-line worked example.
7. Construct-parsers chapter: one paragraph each for `parse_attached_source`,
   `attach_source_reference`, `group_interior_state` (including that the interior
   state is memoized — hand-deriving loses the driver's delta). Fold in
   `parse_construct` cross-references where the chapter's flow changed post-descent.
8. `Lang::make_node_ext` + `StagedChildren`: the view borrows a container the caller
   is about to grow; nothing borrowed may be held past the call.
9. `SourceRecomposer`: its only error is the coherence check no parse output can
   trigger (point at the variant from the type's own docs).
10. `Package::get`: specials are keyed by trigger, not by name — `get(callable_type,
    name)` always answers `None` for specials (and say what to use instead).
11. `slot_content_parent` + `ContentNodes::InRegion`: name `input_macro_spec`'s
    attached slot as the shipped example.
12. `TextContent::resolve` / payload accessors: the carrying source is
    `node.span().source()`.
13. `NodeRef::summary()`: within a release the format is exact and the crate's own
    tests pin it; it may change between releases.
14. `extract` module: the `*_keep_annotations` triple exists so the `Clone + Default`
    bound lands only where needed; the short names are the general form with a
    default callback.

---

## Stage 5 — gates and closure

- `cargo build` / `cargo test` / `cargo docs` (fresh `target/doc`) green.
- Update the api-baseline (fold in the still-pending lang-features baseline update;
  Stage 3 is the bulk of the record).
- **After the ARCHITECTURE/DESIGN_RATIONALE cleanup agent finishes** (do not touch
  those files before): DESIGN_RATIONALE entries for the 2026-08-10 decisions — the
  hook-fallibility ruling (tiers, the `HookFailed` condition, the Tier C
  infallibility rationale), the runtime-identifier relaxation (2.8) and its
  "binding adapters only" scope, the declined register below, and the test-support
  policy. **Strictly per the new content rules** (`[§dd-dr:self-meta]`,
  `[§dd-arch:self-meta]`): entry template with `Status: DECIDED` and a
  "Revisit if" clause, no dates in entry bodies, the ARCHITECTURE reference added
  in the same change, and no mention of this plan, its stages, or its worktree
  anywhere in ARCHITECTURE.
- In the same DESIGN_RATIONALE edit: the panic-policy clause that lists the
  non-panicking companions ("(`NodeTree::get`, `Span::get`)") must gain
  `ChildRegion::staged` (the companion of the `ChildRegion` resolved-only
  accessors, promised by the crate-level panic register since the Stage 1
  review fixes).
- Courtesy notes to the bindings project (1.1 ratchet; the Stage 3 signature sweep).
- Delete `dev-docs/py-ext-feedback-actions/`.

---

## Finalization questions — all resolved (2026-08-10, third round)

1. `HookFailed` cause chain: **in from the start** → 3.0.
2. Diagnostics retention cap: **`ParseDriver::diagnostics_limit()`** → 3.5.
3. `Lang::SessionExt` read-back: **returned in `ParseResult`** → 3.6.
4. `criterion` + README `cargo bench`: **dropped** → 1.7.
5. `GroupRule` `PartialEq`: **kept, documented clearly** → Stage 4 item 2.
6. Deliberately-cut small accessors: **closed as declined** → register below.

## Declined register (user rulings 2026-08-10 — record in DESIGN_RATIONALE at Stage 5)

- **`ParsingState::data()` / `from_data()`** — declined. States stay crate-frozen;
  `derived()` is the sanctioned path from one state to another; the real identity
  symptom is closed by 2.2. A public data→state freeze would force deciding what
  identity and provenance an externally assembled state has.
- **`ConcatPieces` read direction / `into_parts`** — declined. The type is a
  build-only instruction; publishing `into_parts` freezes an internal 6-tuple as
  API; wrappers that need to inspect a delegate's instruction maintain their own
  structure.
- **`test-support` cargo feature** — declined as policy: embedders write their own
  test fixtures; anything genuinely indispensable graduates to real public API
  (`validate_tree` is the model). Resolution under that policy: `AfterEffectSpec`
  → 2.4; the invariant checkers stay internal (the byte-accounting parse-tree law
  is deliberately *not* the all-trees law — `node/invariants.rs` module doc);
  the hand-tree-builder need collapses into 2.7.
- **The small-accessor batch** (closed as declined, third round — none urgent, all
  cut for leanness): `NodeTree::id_at(index)`; `Diagnostic::with_frames`;
  `NamedAccessError` accessors; `ArgumentCodeError` accessors
  (`index`/`offset`/`character`); `KeyValEntry::value_node`;
  `slot_content_parent_named`; `DEFAULT_MAX_SCAN_LEN` const + `max_scan_len()`
  getters; `copy_subtree_into` made public; `NodeRef::invocation_syntax_materialized`;
  a parse-start warning for a half-wired specials trigger-set/scan pair.
- Carried over from the triage: `Default`/public constructors for
  `RestageContext`/`RecomposeContext` (forecloses the "one place to grow" reserve);
  de-lifetiming `ParseContext`; `PartialEq` on `Diagnostic`; `&Arc<ParsingState>` on
  the four state-reading hooks (revisit only if a future sweep reshapes those
  signatures anyway); `Package::get` trigger fallback; `recompose_from`;
  `TriggerChars::None`; an owning `LineIndex` (that is `LineIndexCache`);
  `ExpectedClose` enum; `KeyVals::into_parts`; `Clone` on `RestagedArgument`;
  `StagedChildren::ids()`; blanket visibility flips of tree internals
  (`NodeTree::make_id`, `NodeId::new`, `ParserSession::state_stack`, …).
