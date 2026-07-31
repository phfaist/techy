# Phase 2b — T3 session: interim rulings (full working detail)

Session started 2026-07-31. Brief: T3_BRIEF.md (verified against e5b994b). Updated
every round; durable DESIGN_RATIONALE entries + PLAN.md decision-log line are written
at session close. All rulings are the user's.

## H — resolution extraction + family placement (RULED, round 1)

**Ruling: shape 1, with a naming amendment.**

- Extraction ratified (completes P1 deferred item (a)): the std command-resolution
  body becomes a standalone free function in **`core::specs`**:
  `pub fn resolve_command_in_scopes<L: Lang>(state, token, callable_type)
  -> CommandResolution<L>`.
- **Name (user amendment to the brief): `resolve_command_in_scopes`** — "in", not
  "via". The brief's `resolve_command_via_scopes` and the current spelling
  `CommandResolution::resolve_via_scopes` (engine/driver.rs:446–470) are superseded
  spellings → superseded-names register at session close. The associated fn on the
  result enum is removed (one canonical path).
- **The whole resolution family moves to `core::specs` beside the fn**:
  `CommandResolution`, `ResolvedCallable` (today engine), `CallableQuery`,
  `CallableSyntax`, `SearchedProviders` (today scopes). Boundary reading recorded:
  placement by what the items are *for* (defining/organizing definition lookups —
  author-side vocabulary); `ParseDriver::resolve_command` returning a specs type is
  an accepted cross-boundary signature reference per the P1 entry.

**Recorded interactions** (bind later rounds/sessions):
- Wish 18b's generic driver (G) wraps exactly this fn.
- T1/T2 A1(ii) did-you-mean detail lands in this fn's miss arm; A1(iv) parse-init
  warning is separate but shares the enumeration machinery (→ E2).
- **Phase 3 topology application is now UNBLOCKED** (was waiting on this ruling).
- T4 wire-identifier slate: the resolution-condition `<area>` names the resolution
  concept this ruling defines; nodes_parser conditions can now be slated.
- Driver-name consistency check due at G: `ScopeResolvingDriver` vs the ruled
  "in scopes" verb phrase.

## D — preset-driver architecture (RULED, round 2)

**Ruling: option 3 — both, layered.** Pillar functions are the substance;
**`LatexlikeDriver<LLL>`** is the canned assembly whose hook bodies are precisely
the one-line pillar delegations (`PhantomData<LLL>`; manual impls keep `Copy`/`Eq`).
Not a dual path (component vs building blocks — the `StdCallableSpec` /
`impl CallableSpec` relationship; the struct contains no behavior the pillars don't).

**Pillar inventory** (final naming at application, E-consistent):
- `math_group_interior_delta::<LLL>(base, rule)` — the math plug; forbidden set
  derived from the removed math-class rules, never a restated `'$'` literal (per
  [§dd-dr:latexlike-generalization]).
- `exit_math_context_delta::<LLL>(…)` — **user amendment to the E4 pillar rider**
  (was `restore_text_context_delta`): the delta is defined by *exiting math
  context* — look up the first non-math group in the (enclosing-state) stack and
  restore that context — NOT by constructing/naming 'text' mode directly. The old
  name presumed a text-mode target; superseded-names addition + amendment note on
  the T1/T2 E4 durable records ([§dd-dr:enclosing-state-stack]) at session close.
- `make_paragraph_break_node::<LLL>(style, state, token)`.
- `resolve_command` = H's `resolve_command_in_scopes` + the macro role (no separate
  pillar needed).

**T3/T5 scope split recorded**: T3 rules the architecture (pillars + generic
struct + inventory + `LLL` parameter). T5 keeps: FLM probe re-run as acceptance
(P3 criterion); extra framework knobs / extension seam beyond
write-your-own-over-pillars; pillar-signature sufficiency for post-parse state
synthesis (E4 transform tie-in); restage interaction.

**Open interaction flagged for E**: with exit-math defined by enclosing-context
lookup, does the mode role trait still need a text-mode *constructor* accessor,
or only the math constructor + predicates?

## E — role-accessor naming + ClosedVocabulary (RULED, round 3)

**E1 — accessor names RULED**:
- Callable role trait: constructor accessors **`macro_callable()` /
  `environment_callable()` / `specials_callable()`** (role + vocabulary noun — the
  group trait's recorded pattern; dissolves the `macro` keyword problem without
  workaround spelling). Predicates `is_macro()` / `is_environment()` /
  `is_specials()`. Coherence contracts mirror math-form's
  (`CT::macro_callable().is_macro() == true`, …). Rejected: `r#macro()`
  (raw-identifier hostility), `macro_()` (asymmetric limp), `macro_kind()`
  (NodeKind collision), `macro_type()` (stutter + TypeId-suffix re-import) →
  superseded-names at close.
- **Mode role trait TRIMMED (user)**: **no `text_mode()` constructor** — only
  **`math_mode()`** + **`is_math()`** (no `is_text()` either). Rationale: the only
  known text-mode-constructor consumer was the old restore-to-text pillar, which
  the D amendment redefined as `exit_math_context_delta` (enclosing-context lookup,
  never conjures a mode value). Smaller required surface on foreign vocabularies.

**E2 — ClosedVocabulary RULED: NO supertrait — "provide, don't require"** (user;
revises the brief's recommendation after re-verification):
- Verified: zero shipped fns take the bound today; it is the documented opt-in
  tooling bound (lang.rs:391–396) — the brief's claim that A1(ii) needs it was
  WRONG: at the `resolve_command_in_scopes` miss arm, `callable_type` and mode are
  in hand; the ruled did-you-mean minimum enumerates *symbols*, not vocabularies.
  Cross-vocabulary suggestions (beyond the ruled minimum) are the only upgrade
  that would want `ALL` — not pursued.
- `ClosedVocabulary` stays opt-in: no role-trait supertrait, no `Lang` bound, no
  `LatexlikeLang` umbrella bound.
- **A1(iv) realization**: public bound-where-used check function in core
  (`where L::CallableTypeId: ClosedVocabulary, L::ModeId: ClosedVocabulary`);
  monomorphic-Latexlike path calls it unconditionally; generic-`LLL` wiring states
  the bound narrowly at the one call site (exact wiring point at application);
  frameworks may call it at their own parse entry. Non-enumerable vocabularies:
  the warning is gracefully absent (best-effort diagnostics, not semantics).
- Record in the durable entry: A1(ii) has no enumeration dependency.

## A + F — SimpleLang role + StdParseDriver::default() (RULED, round 4)

**A RULED: option 1 — keep public, rename `SimpleLang` → `TrivialLang`**,
repositioned as the test/prototype lang: docs state the contract ("the trivial
language: for tests and machinery experiments; the default driver resolves
nothing; any customization means implementing `Lang` directly") + the guide
sentence F2 asked for. The on-ramp job moves to the real fixes (B, wish 18b) —
P2 fix-the-real-API doctrine. Wish 18a (overridable-Driver SimpleLang) REJECTED
(escalation argument: each mirrored hook grows it toward a parallel `Lang`).
`SimpleLang` → superseded-names at close ("Simple" over-promised an on-ramp).
Revisit trigger recorded: stable Rust associated-type defaults would dissolve the
trait. Churn: 10 `#[cfg(test)]` impl sites + doc examples.

**F RULED: remove `StdParseDriver::default()`** (engine/driver.rs:359–363). C1's
argument transfers verbatim (recovery is the only field; `Default` hides the one
policy knob); after C1 no `L::Driver: Default` consumer remains (verified).
Spelling: `StdParseDriver::new(Recovery::Strict)`. Churn rides C1's sweep;
[§dd-dr:language-init] amendment gets a completion note at close.

## B — on-ramp (RULED round 5; B2 name settled in round 5b)

**B2 RULED: accept both constructors — `TokenRules::empty()` +
`StateData::empty()`** (user; `neutral` rejected, `empty` chosen round 5b —
matches the verified all-empty contents, in-crate `empty()` precedent, keeps
"disabled" vocabulary reserved for the gate-action family incl. wish 21's
`disable_all()`). Semantics fixed: the all-empty value (every gate false, every
collection/string empty, empty `ScopeStack`, default mode/ext — verified
lang.rs:206–227); the default `initial_state_data` body is re-expressed over it
(one source of truth, pinned by the neutrality test parsing_state.rs:528–538);
NOT `Default` impls (struct-update `..Default::default()` silently zeroes future
fields; named constructor documents intent).

**B3 RULED: keep `scan_specials`/`specials_trigger_chars` defaults AS-IS
(recognize nothing) + document loudly + guide example code** (user; rejects the
brief's default-scope-fold recommendation). Rationale (user): simple-by-default —
a tiny lang must not override to *remove* behavior; dead-code elimination favors
opt-in; want simple + add features, not complicated + remove. Real consumers
mostly sit behind frameworks (FLM, latex2text) that have already plugged these
hooks. Alternatives considered and closed: (1) move hooks to the driver —
structurally impossible without a strata violation (`scan_specials` is called by
the token reader, token/reader.rs:228, which holds only `ParsingState`; the
driver is engine-stratum); (2) error-returning default and (3) required hook —
both rejected (user). Application: loud pairing callout on both hook docs (the
silent trigger-chars trap, lang.rs:310–312) + the F1 guide chapter carries the
standard two-line delegation recipe.

**Wish 18b RULED: accept — `ScopesResolvingDriver` (user naming; plural
"Scopes")**: `struct ScopesResolvingDriver<L: Lang> { recovery: Recovery,
command_type: L::CallableTypeId }`; `resolve_command` = one-line delegation to
H's `resolve_command_in_scopes(state, token, self.command_type)`; everything
else trait defaults. Home: engine hub, beside `StdParseDriver`. Component, not a
shortcut tier (outgrowing it = writing your own `ParseDriver`, the normal path).
Docs say which to reach for vs `TrivialLang`/`StdParseDriver` (test carrier).

## C + G — F11 helpers + wishlist leftovers (RULED round 6; wish 20 + wish-8
naming settled in round 6b)

**Wish 21 RULED: accept — `TokenRulesOverrides::disable_all()`** (all six gates
`Some(false)`, exactly title.rs:56–64's hand-built literal). On the overrides
type (state), composes: `verbatim_state_delta` = `disable_all()` +
`expecting_group_close` (one source of truth); fields tweakable after. Cleanly
separated from B2's `empty()` (two-spellings-of-off: disable = gate action on
existing rules; empty = constitutively nothing).

**Wish 22 RULED: accept — `ParsedArguments::new(Vec)` + `ParsedSlots::new(Vec)`**
(discoverable constructors; the `From<Vec>` impls stay as conversion plumbing;
P4-stable — ext story is per-element).

**Wish 8 RULED (direction): accept the narrow form — extend the sealed-conversion
idiom** (T1/T2 C2/E1 family, one Arc-removal pattern crate-wide) to
`ArgumentSpec::new` (parser by value or pre-Arc'd) and `StdCallableSpec::new`
(`impl IntoIterator<Item = …ArgumentSpec>`). Descriptor-enum factory REJECTED
(second weaker vocabulary frozen forever). **User rider: push consumers to NAME
arguments** — constructor takes the name; anonymous becomes the marked, longer
spelling. Shape settled round 6b (see below); `.named()` builder then superseded
(one-canonical-path: no two ways to set the name).

**Round 6b settlements:**
- **Wish 20 GRANTED, commitment-only** (user): a canned invocation-staging helper
  WILL exist as a `ParseContext` method wrapping the P4 staging door
  (`cx.stage_node`) — not a second door; sketch
  `cx.stage_invocation(&invocation, arguments, slots, children, end_pos)`
  (builds `CallableData` from invocation + trigger — 4 of 7 fields are
  transcriptions; computes the node span; stages; returns the id). **Signature
  ruled in T5** alongside `restage_invocation` bundles + builder-`add` ergonomics
  (shared field vocabulary + region semantics; P4_RULING routing).
- **Wish 8 shape RULED**: `ArgumentSpec::new(parser, name: impl Into<Box<str>>)`
  + `ArgumentSpec::new_unnamed(parser)` — encouraged path shortest; "unnamed" is
  the crate's existing word (structure.rs:147), `new_anonymous` rejected (no new
  synonym). Parser param takes the sealed conversion (no `Arc::new`);
  `.named()` builder REMOVED → superseded; `.with_state_delta()` stays.
  `StdCallableSpec::new(impl IntoIterator<Item = …>)` as briefed.
- **`ParsedSlot` MIRRORED (user)**: `ParsedSlot::new(region, name)` +
  `ParsedSlot::new_unnamed(region)`; `ParsedSlot::named` REMOVED → superseded.
  Param order payload-first (mirrors `ArgumentSpec::new`; flips today's
  name-first `named`, deliberate). Final arities land with the P4 application
  (SlotExt non-defaultable — the `ext: Default::default()` fill dies there
  anyway; verified arguments.rs:336–345). `ParsedArgument` needs no change:
  it carries no own name (name lives on its `Arc<ArgumentSpec>`,
  arguments.rs:244–248 — family consistent).

## Sweep (RULED, round 7 — session complete 2026-07-31)

- **P1 deferred item (b) RULED (user): the `ArgumentParser` trait lives in the
  `constructs` satellite** (`techy::core::constructs` under the P1/C5 topology;
  user spelling "techy::constructs"). Grounds: parsing contract beside its
  implementations (`ConstructParser` precedent); `ArgumentSpec`'s
  `Arc<dyn ArgumentParser>` is an accepted cross-boundary signature reference
  (P1 allowance, both directions); placement follows substance (H precedent:
  lookup → specs; parsing → constructs). **Both P1 deferred items are now ruled —
  the Phase 3 topology is fully specified.**
- Resolved-by-prior list confirmed (user): nothing re-litigated.
- T5 handoffs confirmed (user), routed to PLAN.md T5 agenda: D acceptance = FLM
  probe re-run; driver knobs / extension seam; pillar-signature sufficiency for
  post-parse state synthesis; restage interaction; wish-20 `stage_invocation`
  signature co-designed with `restage_invocation` + builder-`add` ergonomics.
