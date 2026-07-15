# Plan — Slot parsing and the callable convenience surface

**Status: A and B executed (2026-07-15); A.5 and C remain open.** This collects
Action-05 items B.1/B.2/B.3/B.4 into one design package. User decisions of 2026-07-15:
A.2 — keep the name, suggested `{ name: Option<Box<str>>, region, ext }` structure; B.1
named **`can_match_empty()`**; B.2 named **`requires_content()`**, polarity matched to
the derivation.

**Executed (2026-07-15, recorded in DESIGN_RATIONALE §3.6 — the no-spec-side-slots and
emptiness-surface entries; 323 tests green):**

- A.1 deletion sweep: `SlotSpec`, `CallableSpec::slots()`, `StdCallableSpec.slots` (+
  the `StdInvocationParser` implementation-error arm and its pinned test), the guard's
  slots clause; module docs rewritten to the record-level story (spec/structure.rs,
  spec/mod.rs, invocation_parser.rs, environment_parser.rs; ARCHITECTURE.md §specs,
  NAMING_STRATEGY.md rows).
- A.2 reshape: `ParsedSlot { name: Option<Box<str>>, region, ext }` with
  `new(region)` / `named(name, region)` constructors; `get_named`/`name()` kept.
- A.3: the test-lang `EnvSpec { arguments, body_delta }` rehearses the rehoming (read
  back via the `Any`-supertrait downcast); Phase 7's `EnvironmentSpec` follows suit.
- A.4 (partial): `parse_declared_arguments` and `read_rigid_name_group` (+ `NameGroup`)
  promoted to `pub` and re-exported from `constructs`. A `ParsedSlots`-assembly helper
  judged unnecessary for now — the remaining hand-rolled part is a few lines of offset
  bookkeeping.
- B: `ArgumentParser::can_match_empty()` (default `true`, pylatexenc base polarity;
  standard parsers: optional group/marker `true` explicit, mandatory group/expression
  `false`) + `CallableSpec::requires_content()` (default = the derivation below) +
  guard rewired. Behavior pins: optional-only callable valid bare in expression
  position (dispatches in full; swallows a provided optional — pylatexenc parity);
  `BeginSpec`/`RawBlockSpec` override → bare use diagnosed (documented divergence).
  Condition renamed `ExpressionCallableTakesArguments` →
  `ExpressionCallableRequiresContent` (message/semantics follow-through — **flagged
  for sign-off**, see the session report).

**Still open:** A.5 (where the standard `\begin` composition lives — stays test-side
meanwhile) and C (builder sugar vs. Phase 7 one-liners; crate-root re-exports; the
`parse_argument` builder-obligations doc).

## Decided directions (from the Action-05 discussion, 2026-07-15; A revised 2026-07-15)

1. **No slot parser on the spec side — and no `SlotSpec` at all** (user leaning,
   endorsed; supersedes the earlier "slots mirror arguments" direction of the same
   session). The mirror died on the invocation-facts problem: body parsing needs the
   environment name (the `\end{name}` back-reference) and potentially the arguments
   parsed so far — pylatexenc's own `EnvironmentSpec.make_body_parser(token, nodeargd,
   arg_parsing_state_delta)` signature is the confirming precedent (body configuration
   lives directly on the spec; there is no slot-spec list anywhere in pylatexenc). So:
   the callable spec's sanctioned parser (the `make_invocation_parser` takeover) parses
   the body and **directly populates the `ParsedSlot` records** with whatever parsers
   it chooses to drive internally. The mirror principle is revised: arguments and slots
   are the same thing at the **record** level (`region` + `ext` + name), not at the
   spec/parser level — "slots" become pure node vocabulary (the content regions of a
   parsed callable), with no spec-side counterpart.
2. **`\begin` is a takeover spec with an honest emptiness answer** (user decision).
   The standard `\begin` callable spec provides an invocation parser tasked with
   reading the *entire* environment — name, arguments, body — using the name it reads
   to parameterize body parsing/termination. The expression guard consults a spec-level
   emptiness query (item B below) which `BeginSpec` overrides.
3. **`ParsedSlot` carries `ext: SlotExt<L>`** — already implemented (symmetry with
   `ParsedArgument.ext`; DESIGN_RATIONALE §3.5). Unaffected by the `SlotSpec` removal —
   the ext rides on the record.

## A. Removing `SlotSpec` (B.1, revised direction)

### A.1 What gets deleted

- `SlotSpec` (`spec/structure.rs`) and its exports; the "Arguments vs. slots" module-doc
  section rewrites to the record-level story.
- `CallableSpec::slots()` — with it, the **slots trap disappears by construction**
  (nothing to declare that `StdInvocationParser` won't parse; its implementation-error
  arm and the pinned test go too).
- `StdCallableSpec.slots` — `new(arguments)` becomes single-list (a free B.4
  ergonomics win).
- The guard's `!spec.slots().is_empty()` clause (`argument_parsers.rs`) — replaced by
  the spec-level emptiness method (item B), which this removal makes **more
  load-bearing**: it becomes the *only* channel for a body-bearing callable to say "I
  take material". (No behavioral regression from removal alone: the only specs that
  declare slots today are test compositions; `BeginSpec` already declares nothing.)

### A.2 `ParsedSlot` reshape

`{ spec: Arc<SlotSpec<L>>, region, ext }` → `{ name: Option<Box<str>>, region, ext }`.
The name moves onto the record (owned; slots are few per node), keeping
`ParsedSlots::get_named` and the `body()` sugar working. Self-describing-records
(§3.5) is *preserved*, not weakened: for a slot, standing alone means carrying the
name directly — the spec pointer bought nothing else, since `SlotSpec` had no other
tool-visible payload. Deliberate asymmetry with `ParsedArgument` (which keeps its
`Arc<ArgumentSpec>`): argument specs carry parser/name/delta worth pointing at; slot
records have no spec-side counterpart to point at. Sub-question: keep the name
`Option` (recommended — fence-block multi-slot constructs may want names; environments
may not bother) or drop names entirely (slot 0 = body convention, pylatexenc-style)?

### A.3 Where the body state delta goes

`SlotSpec.parsing_state_delta` (pylatexenc's `make_body_parsing_state_delta`) rehomes
to the preset spec type that drives the parse — e.g. Phase 7's `EnvironmentSpec`
struct holds its body delta as an ordinary field, read by its own composition. The
core never interpreted it anyway.

### A.4 What core provides compositions (the B.3 helper question, reshaped)

No slot loop in `StdInvocationParser` — it stays macro-shaped, now without the trap.
Instead, the building blocks a takeover composition assembles:

- `EnvironmentBodyParser` as-is (constructed with terminator syntax + name
  back-reference — the composition has the name in hand when it constructs it);
- `parse_declared_arguments` promoted to `pub` (the shared argument half);
- `read_rigid_name_group` promoted to `pub` (the `\begin{name}` scaffold);
- possibly a small `ParsedSlots`-assembly/region-bookkeeping helper if the remaining
  hand-rolled composition still warrants it — judge after the above land.

### A.5 The standard `\begin` composition

Unchanged question: with A.4's pieces public, the test composition shrinks to
name-read → library lookup → argument loop → body parse → records. Decide where it
lives: core (generic name-indexed "delegating dispatcher") vs. the latexlike preset
(Phase 7 owns the `\begin`/`\end` spelling).

## B. The emptiness surface (B.2)

Two methods, guard rewired (pylatexenc precedent: `LatexParserBase.contents_can_be_empty`,
`parsers/_base.py:107`, consulted by the expression parser at `_expression.py:432`):

1. **On `ArgumentParser`** — can this argument be satisfied consuming nothing?
   (optional group: yes; `*` marker: yes; mandatory group / expression: no).
   Name candidates: `can_match_empty()`, `may_be_absent()`, `contents_can_be_empty()`
   (pylatexenc-literal). Leaning `can_match_empty` — "absent" is the record-level word,
   and pylatexenc's "contents" reads oddly for an argument *parser*.
2. **On `CallableSpec`** — would this invocation, appearing bare (as a single
   expression argument), be malformed? Default derives from the declarative surface
   (arguments only — with `SlotSpec` gone there is no slot list to inspect, which makes
   this method the **only** channel for a body-bearing takeover spec to say "I take
   material"):
   ```rust
   fn requires_material(&self) -> bool {
       self.arguments().iter().any(|a| !a.parser.can_match_empty())
   }
   ```
   Name candidates: `requires_material()`, `needs_arguments_or_slots()` (user's
   strawman; reads oddly once slots are record-only), `invocation_can_be_empty()`
   (positive polarity, pylatexenc-flavored). Note the polarity choice interacts with
   the default: a *takeover* spec that declares nothing but consumes plenty (`\begin`,
   `\verb`) must override toward "requires material" — with the negative-polarity name
   the override is `true` (reads naturally); with positive polarity it's `false`.
3. **The guard** (`argument_parsers.rs`, the `ExpressionCallableTakesArguments` arm)
   switches from `!arguments().is_empty() || !slots().is_empty()` to the spec method.
   Behavior changes to pin in tests: `\frac\mymacro 2` where `\mymacro` takes only an
   optional argument becomes *valid* (optional matches empty — pylatexenc parity);
   `\frac\begin{center}…` becomes *invalid* once `BeginSpec` overrides (a deliberate,
   documented divergence from pylatexenc, which dispatches the environment as the
   argument).

## C. Convenience/ergonomics batch (B.4 + leftovers)

- `StdCallableSpec` builder chain: with slots gone, `new(arguments)` is single-list
  already; decide between an `.argument(…)`/`.with_arguments(…)` appender that
  `Arc`-wraps internally, or declaring `new` plumbing and deferring sugar to the
  Phase 7 preset one-liners (`MacroSpec`/`EnvironmentSpec`/`SpecialsSpec`).
- Crate-root re-exports still missing (Action-05 C): `StdInvocationParser`, the four
  standard argument parsers, `EnvironmentBody`/`EnvironmentBodyParser`,
  `scan_argument_noise`, `stage_pre_space`, `ArgumentNoise` — plus whatever A.4
  promotes (`parse_declared_arguments`, `read_rigid_name_group`).
- Document the `ArgumentParser::parse_argument` builder obligations (Action-05 C) on
  the trait doc.

## Suggested discussion order

A.2 (the `ParsedSlot` reshape — names or not) → A.1 (the deletion sweep; mechanical) →
B naming + guard semantics (small, but has user-visible parse-behavior changes to pin,
and the guard leans on B once the slots clause is gone) → A.4/A.5 (which helpers go
public; where the standard composition lives) → C (mechanical once A/B settle).
