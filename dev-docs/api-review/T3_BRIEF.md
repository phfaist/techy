# Phase 2b — T3 Decision-Session Brief (language-designer persona)

Prepared 2026-07-31. Inputs: PLAN.md decision log (P1–P5 + T1/T2 rulings binding),
POLICY_BRIEF.md §Routing (T3 line), SYNTHESIS.md (F10/F11, wishes 8, 17–22, §3),
walkthroughs/langdesign/ (+ framework/ FLM probes for point D), TODO_Big.md (SimpleLang
item), DESIGN_RATIONALE entries as cited. Every code claim re-verified against the
working tree at commit e5b994b (file:line cited; paths relative to `techy/src/`).
The brief recommends; all rulings are the user's.

**Reading key — unapplied rulings.** The code is still pre-application for P2/P3/P4/E4
and the T1/T2 session (verified: no `LatexlikeLang`, `make_node_ext`, `lang_initial`,
`resolve_state_event`, `minidefs`, `IntoSpecsProvider` anywhere in src). Every
"current code" citation below is therefore the pre-ruling state, with the already-ruled
changes layered on explicitly where they matter. Constraints honored: one canonical
path + specs=author-side/hub=run-side (P1); fix-the-real-API, no shortcut accessors
abandoned at first need (P2); `Lang` stays whole, pillar functions compose, `LLL`
convention (P3); every accepted name is permanently stable under the soft freeze (P5);
the T1/T2 **shorthand-not-second-path principle** (a shorter spelling of the *same*
operation is legitimate; a second *way* is not). Format per point:
Context → Evidence → Options → **Recommendation** → Cost.

---

## A. SimpleLang role (TODO_Big.md:16; F10a; wish 18)

**Context.** TODO_Big.md:16 asks: does `SimpleLang` have a use? rename `TrivialLang`?
document as test-mainly? make it private/crate-internal? The T3 walkthrough
read-and-rejected it (the only such item besides `StdParseDriver`, SYNTHESIS §3
starred list).

**Evidence (verified).**
- `SimpleLang` is a marker trait (state/lang.rs:368) with a blanket
  `impl<T: SimpleLang> Lang for T` (lang.rs:370–380): all nine associated types
  defaulted (`GroupTypeId`/`CallableTypeId` = `u32`, the rest `()`/`Option<String>`,
  `Driver = StdParseDriver`). Documented as "the workaround for associated-type
  defaults being unstable" (lang.rs:360–362) — that root cause is still live in Rust
  today.
- **The dead-end is real and structural**: `StdParseDriver` implements only
  `recovery()` (engine/driver.rs:365–369); the default `resolve_command` resolves
  nothing, returning an `Unresolved` whose detail names the fix
  (driver.rs:165–178). The blanket impl makes `SimpleLang` and *any* customization
  mutually exclusive (lang.rs:366–367 states this) — the first command, the first
  real id enum, the first hook forces the full 9-type `Lang` jump (~80 lines in the
  walkthrough; FRICTION F2).
- **In-crate usage is heavy and test-only**: 10 `impl SimpleLang for …` sites, all in
  `#[cfg(test)]` modules (token/list_reader.rs:155, token/prefix_table.rs:173,
  constructs/group_parser.rs:244, constructs/nodes_parser.rs:1049, node/mod.rs:85 +
  :1218, node/invariants.rs:415, engine/mod.rs:433, scopes/mod.rs:1489,
  state/parsing_state.rs:361) plus doc examples (engine/language.rs:43–50). Zero
  non-test in-crate consumers.
- **P4 raises its value**: `make_node_ext` becomes a *required* `Lang` method
  ([§dd-dr:ext-minting]; P4_RULING.md:70–75 — "REQUIRED method (SimpleLang blankets
  `()`)"). Post-P4, a one-line test lang is impossible *without* SimpleLang (or a
  hand-written `()`-returning stub); external construct-parser/tooling authors
  writing unit tests are exactly the persona that wants it.
- External usefulness beyond tests is empirically nil: T3 rejected it, T1/T2/T4 never
  touched it, T5 wrote a full custom Lang.

**Options.**
1. **Keep, reposition as the neutral test/prototype lang** — docs state the contract
   ("a command-less language: the default driver resolves nothing; any customization
   means implementing `Lang` directly"), keep public. Rename per TODO_Big:
   `TrivialLang` (describes "no behavior" honestly; "Simple" over-promises an
   on-ramp — the walkthrough's false start was believing it) — or keep `SimpleLang`
   (P4's ruling text already speaks of it by name, though that was not a naming
   ruling; [§dd-dr:superseded-names] has no entry either way).
2. **Promote into a quick-start tier** (wish 18 route a): give `SimpleLang` a
   *required* `type Driver: ParseDriver<Self>` (associated-type defaults being
   unstable, it cannot be a defaulted one). The 1-line form becomes 2 lines; combined
   with a generic scope-resolving driver (B/G, wish 18b) a command language needs no
   driver code. Con: fixes exactly one abandonment point (the driver) — the next
   customization (a mode, a `StateExt`, `initial_state_data` itself, which the blanket
   also monopolizes) still forces the full jump; mirroring more hooks onto
   `SimpleLang` escalates until it *is* `Lang` (a parallel trait, double maintenance).
   The walkthrough's own language also wanted real id enums, which no `SimpleLang`
   variant provides.
3. **Demote to `pub(crate)`** (TODO_Big's "private" option): P5 says pub only if
   worth stabilizing; empirical external use is zero. Con: kills the one-line test
   lang for *external* parser/tooling authors precisely when P4's required
   `make_node_ext` makes hand-rolling a test `Lang` heavier; the crate's own tests
   demonstrate the need tenfold.
4. Delete outright — strictly worse than 3 (the 10 test sites would each grow a full
   `Lang` impl).

**Recommendation: 1** — keep public, rename **`TrivialLang`**, reposition docs as
"the trivial language: for tests and machinery experiments; implement `Lang` directly
for anything real" (plus the guide sentence F2 asked for). The on-ramp job moves to
the real fixes (B): a language author was never going to stay on `u32` ids anyway —
P2's doctrine (fix the real API, don't grow shortcut tiers) applies verbatim. Option
2 is defensible as a rider *on top of* 1 if the user wants the two-line quick start;
recommend against, on the escalation argument. Superseded-names entry for
`SimpleLang` if renamed; note the revisit trigger: stable associated-type defaults
would dissolve the trait entirely.
**Cost.** Mechanical rename churn (10 test sites + docs); the rename is free now,
never later (P5).

---

## B. F10 — on-ramp cliffs for a from-scratch `Lang`

**Context.** F10's three cliffs (SYNTHESIS): (a) the SimpleLang dead-end (→ A);
(b) no callable neutral `TokenRules`/`StateData` values; (c) specials wiring is two
hand-written delegating hooks + a gate, with a documented-but-silent failure mode.
Plus the walkthrough's headline: ~7 concept clusters across 5 modules before "hello
world" (langdesign/FRICTION.md Task 1+2), and F1 (no custom-Lang guide chapter) as
the umbrella doc gap.

### B1. The minimal custom-Lang surface as of the current rulings (inventory, not a decision)

For the session's calibration — what a from-scratch language author must write once
P2/P3/P4/E4 + T1/T2 are applied (none of it is in code yet):

| Item | Status after rulings | Today (verified) |
|---|---|---|
| `Lang`: 9 associated types | unchanged | lang.rs:102–183 |
| `Lang::make_node_ext` | **REQUIRED** (P4; the one required method — `()`-one-liner when `NodeExts = ()`) | doesn't exist; `finalize_node` defaulted (lang.rs:345–354), deleted by P4 |
| `Lang::initial_state_data` | defaulted (neutral), always overridden in practice | lang.rs:206–227 |
| `Lang::finalize_transition` | defaulted; becomes **fallible** (`-> Result`, `DeriveError`; E4) | infallible, lang.rs:255–261 |
| `Lang::scan_specials` / `specials_trigger_chars` | defaulted (recognize nothing) | lang.rs:291–298, :320–323 |
| `ParseDriver`: all methods | all defaulted; +1 new defaulted hook `resolve_state_event(&event, &StateStackView)` (E4); resolver moves onto the driver (P4 → T4 wiring) | 11 defaulted methods, driver.rs:72–333 |
| practically-required driver overrides | `recovery()` + `resolve_command` (any command language) | notely wrote exactly these (notely-src/lang.rs:142–154) |
| Entry | `Language::new(driver, initial_state)`, `ParsingState::lang_initial()` / `lang_initial_with_packages(…)`; no `Default` anywhere on the path (P2 + C1) | `Language::new(driver)` one-arg (engine/language.rs:81), `ParsingState::initial()` (parsing_state.rs:66), `Default for Language` (language.rs:286–293) |

Net effect of the rulings on the cliff: **+1 required method** (`make_node_ext` stub
for `NodeExts = ()` langs), one hook signature now fallible, everything else equal.
The cliff is dominated by (i) the `initial_state_data` literal (13-field `TokenRules`,
rules.rs:124–186, deliberately no `Default` — rules.rs:8–9 — plus 4-field `StateData`,
parsing_state.rs:17–31: ~25 lines in notely, notely-src/lang.rs:62–86) and (ii) the
driver ceremony (~30 lines, notely-src/lang.rs:123–154, whose entire content is a
recovery knob plus one `resolve_via_scopes` line).

### B2. Neutral starting values (wish 17)

**Evidence.** The default `Lang::initial_state_data` *body* constructs the all-off
neutral value inline (lang.rs:206–227) — but a language overriding the method cannot
call the default it is replacing, and `StateData<OtherLang>` doesn't transfer, so the
value is transcribed by hand from the doc (FRICTION F3: ~20 lines + a
transcription-error class). No `TokenRules` constructor exists (verified — no
associated fns at all, rules.rs); `WhitespaceRules` alone derives `Default`
(rules.rs:56).

**Options.**
1. **`TokenRules::neutral()` + `StateData::neutral()`** — the all-gates-off,
   empty-data, empty-scopes value; the default `initial_state_data` body becomes
   literally `StateData::neutral()` (one source of truth); authors call-and-tweak
   (`let mut data = StateData::neutral(); data.rules.enable_commands = true; …`).
   Not a `Default` impl — the no-`Default` doctrine (rules.rs:8–9) bans *privileged
   LaTeX values*, which a neutral value contains none of; naming `neutral` matches
   the in-crate vocabulary ("the most neutral data", lang.rs:202–205; the
   walkthrough independently proposed `StateData::neutral()`). `Default` impls
   instead: rejected — `Default` on `TokenRules` invites `..Default::default()`
   struct updates that silently zero unmentioned fields on later field additions;
   a named constructor documents intent.
2. Only `TokenRules::neutral()` (StateData is 4 fields) — the delta is small but
   `StateData::neutral()` costs nothing and completes the call-and-tweak story
   (mode/ext defaults, empty `ScopeStack`).
3. Doc-only (copy-pasteable snippet in the custom-Lang guide chapter).

**Recommendation: 1** (both constructors; names `neutral()`). Alternative name
`disabled()` rejected: the crate reserves "disabled" for the gate-off spelling of a
*feature* ("two spellings of off", rules.rs:111–119); the neutral value is
constitutively empty, not a disabled configuration.
**Cost.** Two stable names; the `initial_state_data` default body is re-expressed
over them (behavior-identical, pinned by the existing neutrality test,
parsing_state.rs:528–538).

### B3. Specials wiring (wish 19; the F10c trap)

**Evidence.** Standard specials require **three** touch points: override
`Lang::scan_specials` delegating to `ScopeStack::scan_specials`
(scopes/mod.rs:1278–1294), override `Lang::specials_trigger_chars` delegating to
`ScopeStack::specials_trigger_chars` (scopes/mod.rs:1299–1305), and set the
`enable_specials` gate in the rules. Both `ScopeStack` methods' docs literally call
themselves "the standard body of a preset's `Lang::…`" (scopes/mod.rs:1272, :1296).
The preset (latexlike/mod.rs:203–215) and notely (notely-src/lang.rs:90–100) contain
byte-identical delegation pairs. Forgetting the trigger-chars half is a *silent*
failure (documented: an omitted char "silently never fires", lang.rs:310–312).

**Options.**
1. **Make the scope-stack fold the *default* hook bodies**: default
   `Lang::scan_specials` = `state.scopes().scan_specials(state, content, pos)`;
   default `specials_trigger_chars` = `data.scopes.specials_trigger_chars()`. The
   two hooks then ship pre-paired (the "both hooks have the same author" coherence
   argument, [§dd-dr:latexlike-generalization]'s soundness point, now holds *by
   default*); the silent-trap vanishes for the dominant case; a language with no
   specials providers gets `Ok(None)`/empty-union — behavior-identical to today's
   defaults (the fold over an empty stack recognizes nothing, and `enable_specials`
   still gates everything: freeze bakes the empty filter,
   parsing_state.rs:205–215). Latexlike's and notely's overrides both *delete*.
   Layering check: `ScopeStack` is a mandatory `StateData` field
   (parsing_state.rs:23) and specials-per-provider is core `SpecsProvider` API —
   no privileged-concept violation ([§dd-dr:no-privileged-concepts] bans preset
   *values*, not core machinery as default behavior). Truly custom scanners
   override both hooks exactly as today.
2. Ship the standard bodies as free functions and keep the defaults recognize-nothing
   — changes nothing material: the wiring is *already* two one-liners; the cost was
   never the line count but knowing both hooks exist and pair (the silent trap
   survives).
3. A derive/macro mixin — rejected: the crate has exactly one derive family
   (diagnostics) and this is two lines of behavior, not boilerplate at derive scale.
4. Doc-only (guide snippet + louder pairing callout).

**Recommendation: 1.** It is the only option that removes the trap rather than
documenting it, it deletes preset code, and the asymmetry it introduces (specials
default to scope-stack lookup while `resolve_command` does not) is principled: a
`SpecialsMatch` carries its own resolved spec and callable type from the provider
(recognition = resolution, lang.rs:263–267), so the core needs no
`L::CallableTypeId` value — whereas a command-resolution default is impossible
without the lang naming its command form (that gap is wish 18b, → G). Doc revision:
the two hooks' docs flip from "the default recognizes nothing" to "the default is
the scope-stack fold; override both together for custom scanners".
**Cost.** A default-behavior change (semver-visible, free pre-freeze); the
implementer-obligation lists (lang.rs:272–289, :307–319) now describe the
*override* case.

### B4. The rest of the on-ramp (confirmations, no new decisions)

- **The recipe gap is F1's guide chapter** (custom-Lang walkthrough: Lang → seed
  rules → driver → package → `Language::new`) — Phase 4 work; nothing to rule here
  beyond confirming the chapter exists in the plan. The only end-to-end custom-Lang
  examples still live in `#[cfg(test)]` modules (engine/language.rs:306–307,
  engine/mod.rs:413–414) — invisible in rustdoc, unchanged since the walkthrough.
- With A (test-lang repositioning), B2, B3, and G's wish 18b accepted, the notely
  "hello world" shrinks to: two id enums, a `Lang` impl of 9 types +
  `initial_state_data` (call-and-tweak) + a `()` `make_node_ext` stub, no driver
  type at all, `Language::new(ScopeResolvingDriver::new(recovery, Cmd),
  ParsingState::lang_initial())`. That is the cliff reduced to its irreducible
  content (the language's actual decisions) — one canonical path throughout, no
  second tier.

---

## C. F11 — takeover-parser staging boilerplate

**Context.** The T3 stretch task (a rest-of-line `@title` takeover parser) spent
~40 of 132 lines on staging mechanics every takeover parser repeats (FRICTION F6/F7,
wishes 20–22).

**Evidence (all re-verified in notely-src/title.rs against current API).**
- **Raw-state block** (title.rs:56–64): 8 lines flipping all six `enable_*` gates
  via `TokenRulesOverrides` — hand-built because `verbatim_state_delta` demands a
  terminator `GroupRule` (constructs/verbatim_parser.rs:115–126: it installs
  `expecting_group_close`) and rest-of-line has none. Still true; no
  terminator-less sibling exists.
- **Staging ceremony** (title.rs:83–123): two `cx.session.builder.add(…)` calls, a
  7-field `CallableData` literal, `ChildRegion::new` index arithmetic
  (node/arguments.rs:128), span bookkeeping. No `stage_callable`-style helper
  exists (verified: the only match is a test-local fn, node/mod.rs:723).
- **`ParsedArguments`/`ParsedSlots` from `Vec`** only via `From` impls
  (node/arguments.rs:306, :393 — found by the persona only through grepping
  `impl From`); `empty()` constructors exist (:275, :363), `ParsedSlot::new/named`
  exist (:338, :343) — the wish is about the two collection types.
- **P4/E4 change this picture** (not yet in code): `ParserSession::builder` goes
  `pub(crate)`; the ONE staging door becomes `cx.stage_node(kind, span, state,
  children)` (auto-mints the node ext) — title.rs's two `builder.add` calls become
  `stage_node` calls; `CallableData` loses its tier-2 `ext` field (per-kind exts
  removed) but `ParsedSlot` construction now *demands* a `SlotExt` value; and the
  enclosing-state stack adds a coherence obligation: a takeover parser descending
  with derived states should use the scoped `with_parsing_state(closure)` form so
  driver event-lowering sees the true stack ([§dd-dr:enclosing-state-stack]). Net:
  the ceremony's *shape* survives P4, slightly re-spelled, plus one new obligation —
  the case for canned helpers gets stronger, and any helper ruled here must be
  specified against the *post-P4* surface, not today's.

**Options and recommendation, per wish** (the shorthand-not-second-path principle
covers all three — each is a shorter spelling of the same staging operation):

- **Wish 21 (terminator-less raw state) — accept as
  `TokenRulesOverrides::disable_all()`**: an associated constructor returning the
  overrides value with all six gates `Some(false)` (exactly title.rs:56–64's
  literal). Placed on the overrides type (state), not as a delta helper in
  constructs: it composes — `verbatim_state_delta` itself becomes
  `disable_all()` + `expecting_group_close` (one source of truth), and a parser
  can still tweak fields after. Name: `disable_all()` (it *is* the gate-off
  spelling — "two spellings of off" says gates, matching B2's naming split);
  alternates `all_off()`, `raw()` (too clever). List-of-uses check: verbatim
  (2 sites, verbatim_parser.rs:341/:535), notely-class takeover parsers.
- **Wish 20 (canned invocation staging) — accept the direction; final shape rides
  the T5 restage detailing.** The right primitive post-P4 is a `ParseContext`
  method (the staging door's sibling; parsers already hold `cx`), roughly
  `cx.stage_invocation(&invocation, arguments, slots, children, end_pos)`,
  building the `CallableData` (callable_type/name/spec/post_space from the
  `Invocation`, constructs), the node span from trigger-start..end, and the
  region/`ContentNodes` plumbing. P4_RULING.md:317 already routes "builder-`add`
  ergonomics (params struct?)" and the `restage_invocation` bundle to T5 — the
  parse-side and transform-side spellings should be co-designed (same field
  vocabulary, same region semantics). T3 rules the *commitment* (a helper will
  exist, on `ParseContext`, wrapping `stage_node` — not a second door); T5 rules
  the signature. Ruling the full shape here against a pre-P4 codebase would
  guarantee rework.
- **Wish 22 (`ParsedArguments::new(Vec)` / `ParsedSlots::new(Vec)`) — accept.**
  Two one-line constructors making the existing `From` impls discoverable from the
  types' own docs pages (the `From`s stay — conversion plumbing for generic
  contexts; `ParsedSlot::new/named` is the in-family precedent). Trivial,
  P4-stable (the records' ext story is per-element, not per-collection).

**Cost.** Three small stable names now + one deferred signature (T5); the guide
staging walkthrough (F1) remains necessary regardless — the helpers shrink it, they
don't replace it.

---

## D. Preset reusability: `LatexlikeDriver<LLL>` vs an extracted driver core (P3 rider, shared with T5)

**Context.** P3 ruled the preset generic over the `LatexlikeLang` family
([§dd-dr:latexlike-generalization]): spec types, environments machinery,
`argument_specs`, `default_token_rules`, `builtin_package`, minidefs, `NodeRef`
sugar all become `LLL`-generic; `Lang` hooks compose via public pillar functions.
The one component left open: **the driver** — does `LatexlikeDriver` itself become
generic (`LatexlikeDriver<LLL>`), or is a driver *core* extracted for frameworks'
own drivers? The FLM split (T5's preset-fork cliff) is the acceptance scenario.

**Evidence (verified).**
- `LatexlikeDriver` today: monomorphic (`impl ParseDriver<Latexlike>`,
  latexlike/driver.rs:92), two Lang-independent config fields (`recovery`,
  `paragraph_break_style`, driver.rs:64–70; `Copy + Eq`), four behavior hooks:
  `recovery()` (:93), `resolve_command` = `resolve_via_scopes(state, token,
  CallableType::Macro)` (:102–108), `make_paragraph_break_node` (:114–134 — the
  `Specials` style constructs a `CallableData` with `CallableType::Specials` + a
  preset `SpecialsSpec`), `group_interior_delta` (:145–180 — the math plug: keys on
  `GroupType::Math`, sets `Mode::Math`, merges `'$'` into the *current* forbidden
  set :163–166).
- Every hook body's Latexlike-coupling is exactly the P3 audit finding — vocabulary
  threading (the roles `macro`, `specials`, `is_math`, `math mode`) plus the one
  literal `'$'` (whose generalization — derive the forbidden set from the removed
  math-class rules, never restate a literal — is already prescribed in
  [§dd-dr:latexlike-generalization]).
- The FLM probe (framework/FRICTION.md §B): `LatexlikeDriver: ParseDriver<Latexlike>`
  only was one of the four captured compile errors constituting the fork cliff. FLM
  also has a *named need to customize the driver*: `refine_diagnostic` is documented
  with FLM's `DollarMathDisabled` as its canonical example (engine/driver.rs:216–223)
  — so "reuse the preset driver wholesale" and "write your own driver" are both real
  FLM postures.
- E4 already ruled half the answer: the driver's event logic (math entry AND text
  restore) is extracted into **public `LLL`-generic pillar functions**, with the
  driver hooks as one-line delegations (T1T2_RULINGS.md E4 pillar-function rider;
  [§dd-dr:enclosing-state-stack]). Structs cannot be partially overridden, and a
  sub-trait cannot re-default supertrait methods (the exact flaw that killed `Lang`
  facet decomposition, [§dd-dr:latexlike-generalization] rejected-alternatives) —
  so pillar functions are the *only* mechanism that serves a framework wanting
  preset-behavior-plus-one-custom-hook.

**Options.**
1. **Generic struct only** (`LatexlikeDriver<LLL>`, hook bodies inline): serves
   adopt-wholesale FLM; abandons customize-one-hook FLM (wrap-and-delegate across
   ~12 trait methods, or fork the bodies — the cliff returns one level up).
2. **Pillar functions only** (driver core extracted; `LatexlikeDriver` stays
   monomorphic or is deleted): every framework writes a ~30-line driver of one-line
   delegations; the plain-Latexlike consumer pays the same 30 lines for no reason.
3. **Both, layered** — the pillar functions are the substance
   (`latexlike::math_group_interior_delta::<LLL>(base, rule)`,
   `restore_text_context_delta::<LLL>(…)` [E4, already ruled],
   `make_paragraph_break_node::<LLL>(style, state, token)`; `resolve_command` is H's
   extracted resolver + the macro role), and **`LatexlikeDriver<LLL>`** is the
   canned assembly whose hook bodies are precisely those one-line delegations
   (+ `PhantomData<LLL>`; manual derives keep `Copy`/`Eq`). Adopt-wholesale = use
   the struct; customize-anything = write your driver over the same pillars the
   struct uses. Not a dual path: component vs building blocks is the
   `StdCallableSpec`-vs-`impl CallableSpec` relationship, and the struct contains
   no behavior the pillars don't.

**Recommendation: 3.** It is the same shape P3 chose for `Lang` (whole type +
pillar composition) and E4 already mandates the pillars; the only genuinely new
ruling is that the struct goes generic rather than staying a Latexlike-only
convenience. **T3/T5 scope split** (per the agenda: don't pre-empt T5): T3 rules
the architecture — pillars + generic struct, hook-per-pillar inventory as above,
`LLL` parameter; **T5 keeps**: the FLM probe re-run as acceptance (P3's criterion),
whether the driver needs additional framework knobs or an extension seam beyond
"write your own over the pillars", pillar-signature sufficiency for post-parse
state synthesis (the E4 transform tie-in), and any restage interaction.
**Cost.** `PhantomData` + manual impls on the driver; the pillar functions are new
permanently-stable names (final naming at application, E-consistent); doc examples
re-spelled.

---

## E. Role-accessor naming (incl. `macro`) and `ClosedVocabulary` as role-trait supertrait (P3 riders)

### E1. Accessor names on the three role traits

**Context.** P3's role traits are method-based; the recorded sketch
([§dd-dr:latexlike-generalization]) names the group trait's accessors
`content_group()`, `math_group(form)`, `verbatim_group()` + classifier
`math_form()` / predicate `is_math()` ([§dd-dr:math-group-form] fixes that trio's
contract). The callable trait needs the macro/environment/specials roles — and
`macro` is a reserved Rust keyword, so the bare constructor name is unavailable.

**Candidates for `LatexlikeCallableType`** (constructor accessors returning `Self`):

| Candidate | Verdict |
|---|---|
| `r#macro()` | Legal but hostile: every call site and doc renders the raw form; no crate precedent; autocomplete/searchability suffer. Reject. |
| `macro_()` | The trailing-underscore convention; asymmetric against `environment()`/`specials()` — the one name that visibly limps. Reject. |
| `macro_kind()` / `macro_type()` | "kind" collides with the `NodeKind` vocabulary; "type" stutters against the implementing type (`CallableType::macro_type()`) and re-imports the `…TypeId` suffix debate. Reject. |
| **`macro_callable()` / `environment_callable()` / `specials_callable()`** | Exactly the group trait's already-recorded pattern (`content_group`, `math_group` — role + vocabulary noun); the keyword problem dissolves as a side effect rather than by workaround; uniform across all three names. **Recommend.** |

Predicates carry no keyword problem: `is_macro()`, `is_environment()`,
`is_specials()` (mirroring `is_math`). Mode trait, same pattern: `text_mode()`,
`math_mode()` + `is_text()`/`is_math()` — note `LatexlikeMode::is_math` and
`LatexlikeGroupType::is_math` coexist on *different* traits/types, which the
sibling-vocabulary rule permits (no shared scope). Coherence contracts mirror
math-form's: `CT::macro_callable().is_macro() == true`, etc. Trait names
themselves (`LatexlikeGroupType`, `LatexlikeCallableType`, `LatexlikeMode`) are
already recorded in the P3 entry; nothing in [§dd-dr:superseded-names] is nearby.
**Cost.** The slight stutter when the host *is* the preset enum
(`CallableType::macro_callable()`) — the same accepted cost as
`GroupType::math_group(...)`; foreign vocabularies (`FlmToken::macro_callable()`)
read cleanly, and they are the point of the exercise.

### E2. `ClosedVocabulary` as a supertrait of the role traits?

**Evidence.** `ClosedVocabulary` (state/lang.rs:397–404): `const ALL`, implemented
by all three preset vocabularies (latexlike/mod.rs:151–163). It is *deliberately
not* a `Lang` associated-type bound (lang.rs:391–396) because `SimpleLang`'s `u32`
ids can't enumerate — the crate's pattern is bound-where-used
([§dd-dr:iter-symbols]). New since P3: two ruled features want enumeration over a
*foreign* `LLL`'s vocabularies — the T1/T2 A1(iv) parse-init warning (sweeps a
provider's definitions: `iter_symbols(callable_type, mode)` needs `ALL` for both
vocabularies to sweep, scopes/mod.rs:1323–1341 requires a type filter by design)
and A1(ii)'s did-you-mean detail (same family). If the role traits don't carry it,
those features either grow `where LLL::CallableTypeId: ClosedVocabulary` bounds on
every function that transitively needs them (bound-noise through the generic
driver/pillar stack) or silently degrade for non-enumerable vocabularies.

**Options.**
1. **Supertrait on the three role traits** (`trait LatexlikeCallableType:
   ClosedVocabulary + …`): a latexlike-family vocabulary is *definitionally* a
   closed enum (the family's premise); preset enums already implement it, so
   adopting them stays zero-code; every `LLL`-generic preset feature gets
   enumeration unconditionally. Cost: one more obligation on a foreign vocabulary
   (maintain `ALL` in sync — the documented footgun, lang.rs:399–403), and it is a
   *preset-family* requirement, not a core one — core `Lang` stays free of it
   (consistent with lang.rs:391–396's reasoning, which is about `SimpleLang`/`u32`,
   a case that cannot be an `LLL` anyway).
2. Bound-where-used throughout the generalized preset — maximal flexibility,
   permanent bound-noise, and each new enumeration-needing feature is a breaking
   bound addition on public functions (vs. free under 1).
3. Supertrait on `LatexlikeLang`'s bounds only (`Lang<CallableTypeId:
   LatexlikeCallableType + ClosedVocabulary, …>`) — same effect as 1 with the
   requirement stated once on the umbrella; marginally less discoverable per
   vocabulary, keeps the role traits usable stand-alone without `ALL`.

**Recommendation: 1** — supertrait on all three role traits. The evolution-posture
math favors it decisively: under P5, adding the bound later is breaking, removing
it later is not; and A1(iv) is already a ruled consumer. Note in the entry that
this narrows nothing real: an `LLL` vocabulary that "cannot enumerate" has no
coherent claim to being a closed latexlike-family vocabulary.
**Cost.** `ALL`-maintenance obligation on extending vocabularies (already the
`ClosedVocabulary` contract for anyone touching `iter_symbols` tooling).

---

## F. `StdParseDriver::default()` fate (routed from T1/T2 C1)

**Context.** C1 removed `Default for Language<L>` and `LatexlikeDriver::default()`
(recovery is the driver's one policy knob; `Default` hides it), explicitly leaving
`StdParseDriver::default()` to this session (T1T2_RULINGS.md C1;
[§dd-dr:language-init] amendment).

**Evidence.** `StdParseDriver { pub recovery }` (engine/driver.rs:347–350),
`new(recovery)` (:354), `Default` = Strict (:359–363, plus the doc example
:339–345). The *only* generic consumer of `L::Driver: Default` was `Default for
Language` (engine/language.rs:286–293) — gone under C1, so after application the
impl serves no bound anywhere in-crate (verified: no other `Driver: Default` site).
Remaining users: the doc example and test convenience.

**Options.** 1. Remove — C1's argument transfers verbatim: `recovery` is
`StdParseDriver`'s *only* field; a `Default` exists solely to hide it. Spelling
becomes `StdParseDriver::new(Recovery::Strict)` (or the pub-field literal).
2. Keep — a test-lang nicety; strict-by-default is the safe direction.

**Recommendation: 1, remove.** Consistency is the point of the routing: after C1,
a surviving `StdParseDriver::default()` would be the single remaining implicit
recovery knob in the crate. The test churn is the same mechanical sweep C1 already
requires. (If A's rename lands, the doc examples get touched anyway.)
**Cost.** Doc/test churn only; pre-freeze.

---

## G. Wishlist sweep — wishes 17–22 + 8 (SYNTHESIS §5)

| # | Wish (one line) | Verified current state | Recommendation |
|---|---|---|---|
| 17 | `StateData::neutral()` / `TokenRules::disabled()` | No constructors exist; default hook body spells the value inline (lang.rs:206–227) | **Accept** as `neutral()` × 2 — ruled in **B2** |
| 18 | Non-dead-end quick start: overridable-`Driver` `SimpleLang` (a) or generic `ScopeResolvingDriver<CT>` (b) | Dead-end verified (A); notely's driver is one `resolve_via_scopes` line + a knob (notely-src/lang.rs:142–154) | **(a) Reject** (A: escalation argument); **(b) Accept** — see below |
| 19 | Packaged specials wiring | Two byte-identical delegation pairs in-tree; silent trigger-chars trap | **Accept as default-hook-bodies** — ruled in **B3** |
| 20 | `stage_callable(cx, …)` takeover helper | No helper (only a test-local fn, node/mod.rs:723); ~40-line ceremony verified (title.rs:83–123) | **Accept direction, `ParseContext` method; signature co-ruled with T5 restage bundles** — **C** |
| 21 | Terminator-less raw-state helper | `verbatim_state_delta` demands a terminator (verbatim_parser.rs:115–126); 8-line hand-built block (title.rs:56–64) | **Accept** as `TokenRulesOverrides::disable_all()` — **C** |
| 22 | `ParsedArguments::new(Vec)` / `ParsedSlots::new(Vec)` | `From<Vec<_>>` only (node/arguments.rs:306, :393), undiscoverable; `empty()`/`ParsedSlot::named` exist | **Accept** both constructors — **C** |
| 8 | Core-level argument-spec shorthand factory (rerouted from T1/T2) | See below | **Accept the narrow form** (conversion idiom), reject a descriptor-code factory — see below |

**Wish 18b — `ScopeResolvingDriver` (name to session).** A core-provided generic
driver: `struct ScopeResolvingDriver<L: Lang> { recovery: Recovery, command_type:
L::CallableTypeId }`, `resolve_command` = `resolve_via_scopes(state, token,
self.command_type)` (the H resolver), everything else trait defaults. The
walkthrough's entire `NotelyDriver` is literally this (API-SURFACE wish 2: "my
whole NotelyDriver is that one expression plus a recovery knob"). It is a
*component*, not a shortcut tier: `StdParseDriver`'s sibling with one more field,
constructed with real inputs (`new(recovery, command_type)`) — nothing to abandon
later, because a lang outgrowing it writes its own driver against the same
`ParseDriver` trait (the normal path, not a different model). Why core can't just
default `resolve_command` this way: the core cannot conjure the lang's command
`CallableTypeId` value — the field *is* the missing datum (contrast B3's specials,
where the provider supplies the type). Home: engine (hub — a run-side component,
beside `StdParseDriver`). Name candidates: `ScopeResolvingDriver` (recommended —
says what it does; `resolve_via_scopes` is the established verb phrase),
`ScopesDriver` (terse, vague), `StdScopeDriver` (the `Std` prefix adds nothing).
Cost: one stable type; `StdParseDriver` keeps its pure-recovery role (test
carrier) — with A's rename, docs should say which to reach for.

**Wish 8 — core argument-spec shorthand.** Evidence: the generic layer's spelling
is `Arc::new(ArgumentSpec::new(Arc::new(GroupArgumentParser::with_rule(rule))))` —
two `Arc::new`s and three type names per argument (~4 lines; spec/structure.rs:148
demands `Arc<dyn ArgumentParser<L>>`, `StdCallableSpec::new` demands
`Vec<Arc<ArgumentSpec<L>>>`, spec/callable.rs:147); the persona called it
acceptable-but-wished (FRICTION Task 3). A codes-style factory cannot exist in core
(codes are preset vocabulary — `argument_specs`' table is macro/optional/star
semantics, latexlike/arguments.rs:115–134), and a descriptor-enum factory
(`[Mandatory(rule), Optional(rule), Marker("*")]`) would freeze a *second*,
weaker vocabulary parallel to the parser types — a taxonomy to maintain forever
for a rare authoring moment. **Recommend the narrow fix instead**: extend the
T1/T2-C2/E1 sealed-conversion idiom (crate rule: one Arc-removal pattern) to this
family — `ArgumentSpec::new(impl Into…ArgumentParser)` accepting a parser by value
or pre-Arc'd, and `StdCallableSpec::new(impl IntoIterator<Item = …ArgumentSpec>)`
accepting specs by value. The spelling becomes
`StdCallableSpec::new([ArgumentSpec::new(GroupArgumentParser::with_rule(rule)).named("url")])`
— zero `Arc::new`, zero new vocabulary, same operation. Plus the guide example the
persona asked for. Cost: one sealed trait (or a widened existing one) — decide the
trait-name family with C2's `IntoSpecsProvider` precedent at application.

---

## H. P1 deferred item (a): extracting std command resolution; resolution-family placement

**Context.** P1 deferred (decision log 2026-07-29): extract
"std-command-resolution-via-scopes" into a standalone opt-in function (expected
home: `specs`), after which the resolution family — `CommandResolution`,
`ResolvedCallable`, `CallableQuery`, `CallableSyntax`, `SearchedProviders` — gets
final placement beside it. **The Phase 3 topology application waits on this**
([§dd-dr:public-namespace-topology]: "the restructure is scheduled … after the
resolver-extraction design"). The agenda asks: verify the flow, sketch the design
space, and recommend whether the ruling lands in T3 or T4.

**Evidence — the current flow (verified).**
- The engine's dispatch calls `ParseDriver::resolve_command(state, token)`
  (engine/driver.rs:165–178; default = `Unresolved` with a teaching detail).
- The standard body is `CommandResolution::resolve_via_scopes(state, token,
  callable_type)` — an associated fn on the result enum (engine/driver.rs:446–470):
  builds a `CallableQuery` (scopes/mod.rs:91–101) with
  `CallableSyntax::Command { escape_char }` (:71–82), calls
  `state.scopes().retrieve_spec(&query, state)`, and maps hit →
  `Resolved(ResolvedCallable)` (driver.rs:376–381), clean miss → `Unresolved`
  carrying `searched_providers()` as detail (:464–466; scopes/mod.rs:1236),
  provider error → `Failed`. Both in-tree drivers and the walkthrough's driver are
  one-line delegations to it (latexlike/driver.rs:107, notely-src/lang.rs:152).
  T3 called it "the single most load-bearing helper".
- Family locations today: `CommandResolution`/`ResolvedCallable` in engine;
  `CallableQuery`/`CallableSyntax`/`SearchedProviders` in scopes; the stale
  T1T2_BRIEF citation (driver.rs:439–465) has drifted to :446–470 — same content.
- Ruled deltas that land *inside* this function: A1(ii) did-you-mean detail on the
  miss path (iterate `iter_symbols`, escape-char near-miss at minimum) — "wording
  lands wherever the deferred extraction puts `resolve_via_scopes`"
  (T1T2_RULINGS A1). A1(iv) (parse-init all-escape-char warning) is a *separate*
  parse-initialization check, not part of this function, but wants the same
  enumeration machinery (→ E2).

**Design space.**
1. **Free fn in `core::specs`** — `pub fn resolve_command_via_scopes<L: Lang>(
   state, token, callable_type) -> CommandResolution<L>`, the whole family moving
   to `specs` beside it. Pro: the P1 entry's own lean ("expected home: specs"; "the
   ambiguous items rest naturally beside that resolver"); the function's substance
   is definition lookup (query construction + provider semantics + miss reporting —
   author-side vocabulary); `ParseDriver::resolve_command` returning a specs type is
   an accepted cross-boundary *signature* reference (the entry's explicit
   allowance). Name: `resolve_command_via_scopes` (a bare free
   `resolve_via_scopes` loses its object; specificity rule). Con: the *result* enum
   is consumed by the run-side dispatch loop — its strongest single tie is the hub;
   mitigated by the boundary rule reading "placement by what the item *is for*
   (defining/organizing lookups) over who calls it".
2. **Same extraction, family stays split** (fn + query/syntax/searched in specs;
   `CommandResolution`/`ResolvedCallable` in the hub as the driver-hook vocabulary).
   Honors "hub = run-side" more literally; but it cuts the family in half across the
   exact seam P1 said should *stop* being ambiguous, and the fn's signature then
   names hub types from specs (inverted dependency in the docs' mental model).
3. **No extraction; move the assoc fn with its enum wherever the family lands.**
   Minimal churn; but P1 already ruled the extraction direction ("is to be
   extracted"), and an associated fn on the *result* type is a discoverability
   accident (T3 found it by reading `CommandResolution`'s docs, not by looking for
   a resolver).

Interactions to record with the ruling, whichever shape: the H fn is what wish 18b's
`ScopeResolvingDriver` wraps; the A1(ii) detail policy lives in its miss arm; T4's
wire-identifier rename slate depends on the outcome ("nodes_parser conditions
interact with the deferred resolution-family extraction", P5 decision log) — the
`UnresolvableCommand`-family `<area>` should name the resolution *concept*, which
this ruling defines.

**Recommendation: rule it in T3, shape 1.** T3 is the persona that authors drivers
and calls this function (the evidence is this walkthrough's); T4's stake is only the
downstream identifier slate, which *needs the extraction ruled first* — deferring to
T4 serializes Phase 3 behind the last session for no informational gain (nothing T4
learns would change the resolver's design; the reverse dependency is strict). If the
session hesitates on specs-vs-hub for the two run-side types, shape 2 is the
fallback that still unblocks Phase 3 — the *function* home and name are the blocking
decisions.
**Cost.** One free fn + family move (pure re-export topology under P1's facade
model); `CommandResolution::resolve_via_scopes` becomes a superseded spelling
(register it); doc sweep on the two driver one-liners.

---

## Resolved by prior rulings — do not re-litigate

- **`latexlike.*` wire identifiers inside foreign-`LLL` parses** — ruled in **P5**
  (identifier names the raising machinery, not the parsed language;
  [§dd-dr:wire-identifier-stability]).
- **Generic `minidefs::minilatex_package::<LLL>()`** — ruled in **T1/T2** (B rider;
  [§dd-dr:minidefs] amendment).
- **Wish 8's preset-side siblings** (registration one-liners, named factory,
  `BracedOnly`) — T1/T2 E1/E2/A2.
- **The E4 design** (enclosing-state stack, `resolve_state_event`, fallible
  `finalize_transition`, pillar-fn rider) — T1/T2; this session only *consumes* it
  (B1 inventory, C obligations, D pillars).
- **`Lang` facet decomposition / plugin-slot preset** — rejected in **P3** with
  recorded killing flaws; A's options were checked against that list (nothing here
  reintroduces them).
- **Curated root / dual paths** — P1; all names above get exactly one home under
  the C5 topology.
- **Stability tiering** of anything accepted here — P5: ordinary stable pub, soft
  freeze.

## Session logistics (proposed order, hard structural first)

Pattern per T1/T2: interim rulings file (`T3_RULINGS.md`) updated every round.

1. **H** — resolution extraction (blocks Phase 3 topology; its output feeds 18b,
   A1(ii), and the T4 identifier slate). Decide: extract? shape 1/2? fn name.
2. **D** — preset-driver architecture (pillars + `LatexlikeDriver<LLL>`; fix the
   T3/T5 boundary explicitly in the ruling).
3. **E** — role-accessor names + `ClosedVocabulary` supertrait (quick; D's pillar
   signatures use the vocabulary, so settle before drafting them).
4. **A + F** — SimpleLang role/rename + `StdParseDriver::default()` (one cluster:
   the test-lang story).
5. **B** — on-ramp: neutral constructors (B2), specials default bodies (B3),
   wish 18b driver; confirm the B1 inventory + guide-chapter dependency.
6. **C + G leftovers** — F11 helpers (20 commitment / 21 / 22), wish 8 narrow form.
7. **Sweep** — resolved-by-prior list confirmation; route the T5 handoffs (D
   acceptance probe, wish-20 signature) into PLAN.md's T5 agenda line.
