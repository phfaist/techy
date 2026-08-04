# Phase 3 — S5: Invocation syntax + staging sugar — stage report

Branch `phase3-s5-invocation-syntax` (worktree; cut from `api-review` tip 60dfd2b).
Implementer report per the relay discipline: plan first, at least one commit per
milestone, Progress updated each milestone.

## Progress

- [x] M0 — implementation plan committed (this file)
- [x] M1 — Core recording channel + staging sugar + latexlike payload (grew per
      plan note: the `EnvironmentSyntax` trait, the fifth role trait +
      umbrella bounds, and the `StdEnvironmentSyntax` recording all landed here —
      the sugar/tests need them; the begin scan already delegates to
      `parse_begin` on the concrete record). 608 lib tests (+11), 0 warnings.
- [x] M2 — Environments machinery over `LLL` (commit b8233a4):
      `EnvironmentBehavior<LLL = Latexlike>` (defaulted trait param),
      `EnvironmentSpec<LLL>`, `VerbatimBehavior<LLL>`, `BeginSpec<LLL>`/`EndSpec<LLL>`
      (PhantomData ZSTs + `new()`), `EnvironmentInvocationParser<LLL>`/
      `OrphanEndParser<LLL>`; begin scan via the family's Env
      (`LatexlikeInvocationSyntax::Env::parse_begin`); payload staged via
      `environment_form`; `SlotExt<LLL>: BodySlotExt` bound-where-used on the
      composition + BeginSpec's CallableSpec impl; `EnvironmentInvocation` spelling
      fields; verbatim literal composed from them. 608 lib tests green,
      0 warnings. M2 addendum (successor 1): foreign-LLL environment smoke test
      landed — `Flavored` hoisted to the tests-module level (shared fixture),
      sibling test `a_foreign_family_member_parses_environments_and_verbatim`
      registers `BeginSpec::<Flavored>`/`EndSpec::<Flavored>` + itemize/verbatim
      specs and asserts begin/end payload facts (write_begin/write_end round
      trips, verbatim std end facts) + `body()` through the `()` slot ext.
      609 lib tests, 0 warnings.
- [x] M3 — `MacroSpec<LLL>` + `argument_specs<LLL>` + paragraph-break
      name-as-written / canonical spec: `MacroSpec<LLL = Latexlike>` (SpecialsSpec
      pattern, manual Debug/Clone/Default); `argument_specs`/`argument_specs_from_str`
      + helpers over `LLL` via role-trait constructors (`content_group()` for
      `m`/minted rules/lone-group unwrap, `verbatim_group()` for the `v` codes;
      minted delimiter spellings unchanged) with the compiler-forced
      `ArgumentExt<LLL>: Default` bound (D-plan-16); `ParagraphBreakSpec` ZST
      beside `ParagraphBreakStyle` (blanket `CallableSpec<LLL>` impl, "specials"
      frame vocabulary, identity = type identity per D-plan-8);
      `make_paragraph_break_node` + `ParseDriver` hook gained `source_content:
      &str` (D-plan-7; core default, nodes_parser call site, latexlike pillar +
      driver delegation, MarkDriver test override, engine/mod.rs test); the
      Specials arm records the actual whitespace run as name (both the synthetic
      Invocation's and the node's) and stamps `ParagraphBreakSpec`;
      ParagraphBreakStyle::Specials rustdoc rewritten (canonical-"\n\n"
      superseded). Tests: run-as-name + spec-downcast identity (pillar unit,
      latexlike end-to-end, acceptance), foreign-member MacroSpec/argument_specs
      incl. `v`-code verbatim group. base_package untouched (monomorphic — S9).
      610 lib tests, 0 warnings.
- [x] M4 — Parse-law payload checks + FLM probe adaptation + regression sweep:
      `check_invocation_syntax_payload` filled (D-plan-12 downcast to the
      default-Env latexlike enum; Macro spelling-prefix + post-space positional
      pins — childless arm repaired to containment, D-plan-17; Specials
      name-as-written prefix pin; Environment write_begin prefix / write_end
      suffix byte pins); four discriminating should_panic tests
      (invariants.rs `payload_pins` module: hand-built Latexlike trees with
      diverging payloads); the whole in-crate suite passes under the active pins
      (the positive direction — every latexlike parse runs the oracle). FLM
      probe adapted (edit list below). 614 lib tests, 0 warnings.
- [x] M5 — Docs + closure: environments.rs doc-link fixes (the M2-era
      `CallableType::…` references — qualified `super::`); DR status lines
      ([§dd-dr:invocation-syntax] applied-S5, [§dd-dr:takeover-staging-sugar]
      items 2 (S3, retroactive) + 3 (S5), [§dd-dr:span-invariants] amendment
      applied note incl. the D-plan-17 containment arm,
      [§dd-dr:environment-scaffolding] supersession applied note,
      [§dd-dr:latexlike-generalization] applied-scope); ARCHITECTURE passages
      (division-of-labor node facts, span-invariants bullet
      reconstruct→record applied, nodes recompose-session tracker, engine
      applied tracker, latexlike generalization tracker + NodeRef sugar
      roster); learn-by-example paragraph-break passage (canonical-"\n\n"
      superseded); superseded-names sweep clean (no `CallSyntax` /
      `CallableNodeInvocationSyntax` / `new_for_invocation`; no core
      CallableData post_space; token/comment post_space vocabulary sanctioned);
      CLAUDE.md / README / other guide pages unaffected (grep-verified). Full
      gates green (below).

## Ruling inputs (digest)

- PHASE3_PLAN.md §Protocol + §S5; PLAN.md decision log 2026-08-03/04 (recompose
  session) + the NEXT bullet's recompose-session checklist additions.
- DESIGN_RATIONALE: [§dd-dr:invocation-syntax] (central), [§dd-dr:takeover-staging-sugar]
  (+ T5 amendment: the ruled `stage_invocation` signature), [§dd-dr:environment-scaffolding]
  (supersession note), [§dd-dr:span-invariants] (invariant-3 amendment),
  [§dd-dr:ext-minting] (one staging door), [§dd-dr:restage-ops] (symmetry by
  vocabulary, not arity), [§dd-dr:latexlike-generalization] (+ recompose amendment:
  fifth role trait), [§dd-dr:slot-roles], [§dd-dr:recompose-machinery] (S8 consumer
  context), [§dd-dr:superseded-names].
- RECOMPOSE_RULINGS.md Round 2 in full (CallSyntax rejection; accuracy doctrine;
  two-trait split; env record; Specials Option 1; env-payload mechanism, shape (b);
  Round A–D naming: `FromInvocation`/`from_invocation`, bound-trait fallback name).
- T5_RULINGS.md §B (stage_invocation: end_pos rule, no overrides, symmetry) +
  T5_BRIEF §B (the site inventory: StdInvocationParser + argument_parsers.rs:358
  collapse; environment sites stay on the door).
- reports/S4_REPORT.md (role traits, LatexlikeLang, pillars, LatexlikeDriver<LLL>,
  SpecialsSpec<LLL>; routings into S5: fifth role trait, MacroSpec/environments/
  argument_specs generalization, FLM probe re-run).

## Design synthesis (what the records fix, and how it maps onto today's code)

### Core recording channel

- `Lang::InvocationSyntax` — new associated type on `Lang` (state/lang.rs), bounded
  by the new **`InvocationSyntaxData`** trait: `Clone + Debug + Send + Sync +
  'static` + `materialized(&self, source_content: &str) -> Self`. techy implements
  it for `()`. The trait lives beside `Lang`/`NodeExtTypes` in state/lang.rs (the
  NodeExtTypes placement precedent: constituent of the compile-time bundle),
  exported from the `techy::core` hub.
- **`FromInvocation<L>`** — opt-in constructor trait in constructs/mod.rs beside
  `Invocation` (it consumes `&Invocation`, which state cannot name):
  `fn from_invocation(invocation: &Invocation<'_, '_, L>) -> Self`; implemented for
  `()`; exported via `techy::core::constructs`. The `Invocation` bundle already
  carries the trigger token (`invocation.token`), so the constructor sees exactly
  what was matched (scope item 5) — no bundle field is added (the chicken-egg with
  `from_invocation(&Invocation)` forbids one; see D-plan-2/3).
- `CallableData<L>`: `post_space: TextContent` REPLACED by
  `invocation_syntax: L::InvocationSyntax`. `materialized()` routes through the
  bound trait (scope item 6 falls out of the existing
  `NodeTree::materialize` → `NodeKind::materialized` chain).
- kind.rs invariant-3 rewording: the old post_space field doc (the token-only
  post-space contract) is replaced by the invocation-syntax channel doc; the
  Comment payload doc's "Mirrors CallableData::post_space" cross-reference is
  reworded standalone.
- `NodeRef::post_space()` (core) is removed with the field; core gains the payload
  accessor `NodeRef::invocation_syntax() -> Option<&L::InvocationSyntax>`; the
  latexlike NodeRef sugar gains `post_space()` over the payload (Macro → recorded
  value; Environment/Specials → `Some("")`, preserving prior read semantics for
  latexlike trees — environments recorded empty post-space before).
- `validate_tree`'s callable arm drops the (now Lang-opaque) post-space residency
  read; the parse-law checker's callable arm reads the invocation-syntax payload
  via `Any`-downcast to the shipped payload types (M4).

### Staging sugar

- `ParseContext::stage_invocation(&invocation, arguments: ParsedArguments<L>,
  slots: ParsedSlots<L>, children: Vec<BuildId>, end_pos: Option<usize>) ->
  ConstructParserResult<L, BuildId>` — the ruled T5-B signature; `None` = std end
  rule (last staged child's span end, else trigger end); transcribes
  callable_type/name/spec from the bundle and mints the payload via
  `from_invocation(&invocation)` under `where L::InvocationSyntax:
  FromInvocation<L>`; stages through the one door (`stage_node`), maps builder
  errors through `implementation_error`. NO callable_type/name overrides.
- In-crate collapses: `StdInvocationParser::parse` and the expression-position
  bare-callable site (argument_parsers.rs `dispatch_expression_invocation`'s
  requires_content arm — T5_BRIEF's ":358" site, the rulings' "tack-on"
  shorthand). Environment compositions (core test composition + latexlike) stay on
  the canonical `stage_node` door.
- **Compiler-forced bound spread** (D-plan-2): `StdInvocationParser` is produced by
  the *defaulted* factory `CallableSpec::make_invocation_parser`, which is reached
  through the *defaulted* `ParseDriver::make_invocation_parser` from the std
  dispatch loop. A trait's default body compiles under the trait's own bounds, so
  the `FromInvocation` bound must appear as method-level where clauses on the
  defaulted factory chain and propagates through the loop's impls up to the parse
  entry points. Expected roster: `CallableSpec::make_invocation_parser`,
  `ParseDriver::{make_invocation_parser, make_nodes_parser, make_group_parser}`
  (method where clauses); `NodesParser`, `GroupParser`, `EnvironmentBodyParser`
  impls; `ParseContext::{parse_nodes, parse_group, stage_invocation}`;
  `Language::{parse, parse_source}`; the std argument parsers that dispatch
  invocations (Expression/Group/Optional/CharsGroup/Embellishments/TackOn — their
  bounds discharge at `ArgumentSpec` construction, the ArgumentExt: Default
  precedent). `()` + the latexlike impl keep every in-crate lang compiling.

### Latexlike payload

- `latexlike::InvocationSyntax<Env = StdEnvironmentSyntax<Latexlike>>` — enum
  `Macro { escape_char: char, post_space: TextContent }` / `Environment(Env)` /
  unit `Specials`. No `L` type parameter (an unused `L` is uninstantiable on an
  enum — D-plan-4); family members write
  `InvocationSyntax<StdEnvironmentSyntax<Flm>>`.
- `FromInvocation` impl (any `Env`): Command trigger → `Macro` (escape char +
  span-backed post-space from the token); everything else → `Specials` (name is
  already the spelling as written on `CallableData.name`). The Environment arm is
  never minted by from_invocation — environments stay on the door.
- `InvocationSyntaxData` impl: Macro materializes post_space; Environment delegates
  to the Env (via the supertrait, next point); Specials is unit.
- `EnvironmentSideSyntax<L>` — the ruled per-side record `{ escape_char: char,
  command_word: TextContent, post_space: TextContent, name_group_rule:
  Arc<GroupRule<L>> }`. `StdEnvironmentSyntax<L> { begin: EnvironmentSideSyntax<L>,
  end: Option<EnvironmentSideSyntax<L>> }` — accumulator shape (b): end empty until
  filled.
- **`EnvironmentSyntax<L: LatexlikeLang>: InvocationSyntaxData`** (supertrait gives
  the data bounds + materialization; the rule Arc is source-independent, exempt):
  - `fn parse_begin(cx, trigger: &Token<'_, L>) ->
    ConstructParserResult<L, Option<(NameGroup<L>, Self)>>` — the begin-side scan
    (rigid name group via the core building block), `None` = malformed, nothing
    consumed; on success the accumulator has the begin side filled (trigger facts +
    the matched rule Arc), end side empty.
  - `fn parse_end(&mut self, end: EnvironmentSideSyntax<L>)` — fills the end side
    from the facts the body parser (the terminator consumer) reported back.
  - `fn record_std_end_facts(&mut self, command_word: &str)` — the ONE std-facts
    method: the verbatim path (literal terminator, no tokenized scan exists)
    synthesizes the end side from the begin side's facts + the command word.
  - `fn write_begin(&self, name: &str, source_content: &str) -> String` /
    `write_end(…)` — the spelling writers; `write_end` emits nothing when the end
    side is empty (recovered shapes had no terminator — reemit == input holds).
- **End-side facts channel (core)**: `NameGroup` becomes `NameGroup<L>` carrying
  the matched `rule: Arc<GroupRule<L>>` (cloned from the GroupOpen token; `Copy`
  dropped); `EnvironmentBody<L>` gains
  `terminator: Option<EnvironmentTerminatorFacts<L>>` with
  `enum EnvironmentTerminatorFacts<L> { Scanned { escape_char: char, command_word:
  Span, post_space: Span, name_group: NameGroup<L> }, Literal { span: Span } }` —
  filled by `EnvironmentBodyParser::finish_terminator` (Scanned, on the clean
  consume path only) and `VerbatimBodyParser` (Literal). Mismatch/malformed/missing
  report `None` → the end side stays empty (see the S8 note below).
- **`EnvironmentInvocationParser<LLL>`** (latexlike composition): begin scan
  delegated to `Env::parse_begin`; resolution (under
  `LLL::CallableTypeId::environment_callable()`) + argument parsing stay
  composition-owned; after the body parse the terminator facts map to
  `parse_end` (Scanned) / `record_std_end_facts` (Literal) / nothing (None); the
  node stages via the door with
  `invocation_syntax: LLL::InvocationSyntax::environment_form(env)`.
- Environments machinery over `LLL`: `EnvironmentBehavior<LLL>`,
  `EnvironmentSpec<LLL = Latexlike>`, `VerbatimBehavior<LLL = Latexlike>`,
  `BeginSpec<LLL = Latexlike>`/`EndSpec<LLL = Latexlike>` (PhantomData ZSTs),
  `default_body_parser<LLL>` (content_group() role constructor);
  `EnvironmentInvocation` gains the spelling fields the generalized verbatim
  literal composition needs (escape_char, name-group open/close — its docs
  sanction growth by field).
- **Fifth role trait** `LatexlikeInvocationSyntax<L>` (latexlike/lang.rs):
  `type Env: EnvironmentSyntax<L>`; constructors `macro_form(escape_char,
  post_space)` / `environment_form(env)` / `specials_form()`; accessors
  `macro_syntax(&self) -> Option<(char, &TextContent)>` /
  `environment_syntax(&self) -> Option<&Self::Env>` / `is_specials(&self)`.
  Joins the umbrella: `LatexlikeLang: Lang<…, InvocationSyntax:
  LatexlikeInvocationSyntax<Self> + FromInvocation<Self>>` (the FromInvocation
  addition is D-plan-9).
- `Latexlike::InvocationSyntax = InvocationSyntax` (default Env).

### Paragraph breaks (driver.rs fix, Specials Option 1)

- New `ParagraphBreakSpec` ZST implementing `CallableSpec<LLL>` for every family
  member — the definite, identifiable paragraph-break spec object; identity =
  type identity (downcast), since a per-`LLL` `Arc` singleton is unrealizable
  under `no_std` (D-plan-8). Never an anonymous `SpecialsSpec::default()`.
- The Specials-style node records the **actual whitespace run** as `name`
  (canonical-`"\n\n"` superseded) — requires source content at mint time, so
  `ParseDriver::make_paragraph_break_node` gains a `source_content: &str`
  parameter (forced ripple, D-plan-7); the pillar consults `from_invocation`
  through a synthetic `Invocation` for the break token (the "specials site"
  consultation), which yields `Specials` for the latexlike enum.

## Milestones (each commits green)

### M1 — Core channel + staging sugar + latexlike payload minimum

Core: `InvocationSyntaxData` + `Lang::InvocationSyntax` (+ TrivialLang `()`;
every in-crate `impl Lang` gains `type InvocationSyntax = ();` — ~37 sites);
`FromInvocation` + `()` impl; `CallableData` field swap + materialized;
kind.rs rewording; `NodeRef` accessor swap; `NameGroup<L>` + rule;
`EnvironmentBody` terminator-facts channel + `finish_terminator`/verbatim fill;
invariants.rs restructure (drop the opaque post-space reads; payload pins → M4);
`stage_invocation` + the two site collapses; the bound spread (compiler-guided).
Latexlike: the enum + records + `InvocationSyntaxData`/`FromInvocation` impls;
`Latexlike::InvocationSyntax`; NodeRef `post_space()` sugar; environments.rs
records begin/end facts by direct construction (still monomorphic);
verbatim path fills std end facts directly; pillar Specials arm mints via
`from_invocation` (name still canonical until M3); umbrella bound additions
LAND HERE if M1 needs them for the pillar (else M2).
Tests: stage_invocation end_pos None/Some; macro spelling round-data; env
begin/end facts presence (incl. verbatim); materialize-through; `()` impls.

Files: state/lang.rs, constructs/mod.rs, constructs/invocation_parser.rs,
constructs/argument_parsers.rs, constructs/environment_parser.rs,
constructs/verbatim_parser.rs, constructs/nodes_parser.rs,
constructs/tack_on_parser.rs, constructs/embellishments_parser.rs,
constructs/chars_group_parser.rs, constructs/group_parser.rs, node/kind.rs,
node/node_ref.rs, node/invariants.rs, node/display.rs, node/mod.rs (tests),
node/builder.rs (docs), engine/driver.rs, engine/language.rs, engine/mod.rs
(tests), spec/callable.rs, spec/mod.rs, scopes/mod.rs, token/reader.rs (test
langs), state/parsing_state.rs (test langs), core/mod.rs + core/constructs.rs
(exports), latexlike/{mod,spec,driver,environments,node_ref}.rs,
tests/acceptance.rs.

### M2 — EnvironmentSyntax + fifth role trait + machinery over LLL

`EnvironmentSyntax<L>` trait + `StdEnvironmentSyntax` impl;
`LatexlikeInvocationSyntax<L>` + enum impl + umbrella bounds;
`EnvironmentBehavior<LLL>`/`EnvironmentSpec<LLL>`/`VerbatimBehavior<LLL>`/
`BeginSpec<LLL>`/`EndSpec<LLL>`/`EnvironmentInvocationParser<LLL>`/
`default_body_parser<LLL>`; `EnvironmentInvocation` spelling fields; begin-scan
delegation (`parse_begin`); write_begin/write_end + tests (reemit parity for
`\begin {itemize}`-style spellings; verbatim std end facts); foreign-LLL
environment smoke test.

### M3 — MacroSpec<LLL> + argument_specs<LLL> + paragraph-break fix

`MacroSpec<LLL = Latexlike>` (manual impls, SpecialsSpec pattern);
`argument_specs`/`argument_specs_from_str`/word codes over `LLL` (role-trait
constructors; verbatim codes via `verbatim_group()`); `ParagraphBreakSpec`;
name-as-written + `make_paragraph_break_node` source_content param (core hook +
call site + test drivers + pillar); ParagraphBreakStyle::Specials doc rewrite;
tests (run recorded as name; spec identity; both styles; base_package stays
monomorphic — S9).

### M4 — Parse-law payload checks + FLM probe + regression sweep

invariants.rs callable arm: downcast payload pins — Macro spanned post-space
positional pin (the reworded invariant 3); Specials name-prefix pin; Environment
begin-prefix/end-suffix byte pins (write_begin/write_end against the span's
bytes). FLM probe (dev-docs walkthrough) adapted to the S5 surface (+ S4 drift:
ParsingStateStack). Regression sweep per the acceptance list.

### M5 — Docs + closure

Rustdoc sweep (StdInvocationParser module docs' post-space section; builder/add
docs naming post_space; token docs cross-refs); DR status lines
([§dd-dr:invocation-syntax], [§dd-dr:takeover-staging-sugar] items 2–3,
[§dd-dr:span-invariants] amendment, [§dd-dr:environment-scaffolding],
[§dd-dr:latexlike-generalization] applied-scope update); superseded-names sweep
(CallSyntax, core post_space field, canonical-"\n\n",
CallableNodeInvocationSyntax, new_for_invocation); ARCHITECTURE passages
(nodes/latexlike component rosters); CLAUDE.md check; guide pages
(learn-by-example post_space passage; parsing-model if it names the field);
README check; full gates; consolidated summary (signature table, deviations,
commits, churn).

## Risks

- The bound spread (D-plan-2) is the churn driver; the compiler guides the
  roster. Mitigation: land it in one commit with the `()` impls first.
- Umbrella supertrait cycle (`LatexlikeLang` ⇄ `LatexlikeInvocationSyntax<Self>`
  ⇄ `EnvironmentSyntax<L: LatexlikeLang>`): same shape as `Lang::Driver:
  ParseDriver<Self>`; if the associated-type-bounds position rejects `Self`, fall
  back to a trait where clause.
- environments.rs borrow choreography (env accumulator vs. behavior body-parser
  borrows): mitigated by cloning the begin-side rule Arc into a local before the
  body parse and calling `parse_end` after the parser is dropped.
- Latexlike tests asserting `post_space()` semantics: preserved via the sugar
  (`Some("")` for environments/specials).
- Paragraph-break hook signature change ripples through custom test drivers.

## Deviations / delegated decisions (running list — for user sign-off)

- D-plan-1 (delegated naming): the core bound trait is **`InvocationSyntaxData`**
  — the ruled fallback applies: the ext-bound family has no *named* bound traits
  to mirror (NodeExt/ArgumentExt/SlotExt are type aliases with inline bounds on
  `NodeExtTypes`). Home: state/lang.rs beside `Lang`/`NodeExtTypes` (same
  placement rationale), exported from the `techy::core` hub. No core type alias
  for `L::InvocationSyntax` (unlike the exts, it is a direct associated type).
- D-plan-2 (compiler-forced): the `FromInvocation` bound-where-used propagates
  through the defaulted factory chain to the engine entry points (chain: the
  default body of `CallableSpec::make_invocation_parser` constructs
  `StdInvocationParser`; that default is reached via the defaulted
  `ParseDriver::make_invocation_parser` from the dispatch loop; a trait's default
  body compiles under the trait's own bounds, so method-level where clauses are
  the only realization short of requiring `FromInvocation` on the associated type
  — which the two-trait-split ruling forbids). Consequence: driving the std
  engine (`Language::parse`) requires `L::InvocationSyntax: FromInvocation<L>`;
  `()` and the latexlike enum satisfy it for every in-crate lang.
- D-plan-3 (delegated design): `FromInvocation` lives in `core::constructs`
  beside `Invocation` (it consumes `&Invocation`; the state module cannot name
  it); signature `from_invocation(&Invocation<'_, '_, L>) -> Self`. The
  `Invocation` bundle itself is unchanged (it already carries the trigger token);
  no minted-value field is added — `from_invocation(&Invocation)` forbids the
  chicken-egg, and a bundle field would force minting into the dispatch loop.
- D-plan-4 (delegated design): the latexlike enum is
  `InvocationSyntax<Env = StdEnvironmentSyntax<Latexlike>>` — no `L` parameter
  (an otherwise-unused `L` is uninstantiable on an enum; the preset lang anchors
  the only expressible concrete default). Family members name their Env
  explicitly.
- D-plan-5 (delegated design): `EnvironmentSyntax<L>: InvocationSyntaxData` —
  the payload-member data bounds and `materialized` come from the core trait
  (the name-group rule Arc stays exempt, per the ruling's source-independence
  argument).
- D-plan-6 (delegated realization): end-side "scanning delegation" is realized
  as **facts reporting**, per the entry's own "end-side facts are reported back
  by the body parser (the terminator consumer)": core `NameGroup<L>` gains the
  matched rule Arc (loses `Copy`); `EnvironmentBody<L>` gains
  `terminator: Option<EnvironmentTerminatorFacts<L>>`
  (`Scanned {…}` from the tokenized flow / `Literal { span }` from verbatim);
  `parse_end(&mut self, end: EnvironmentSideSyntax<L>)` fills from Scanned
  facts; `record_std_end_facts(&mut self, command_word: &str)` (the one
  std-facts method) synthesizes from the begin side for Literal terminators.
  End-side *tolerance* would need a body-parser seam — noted as future work,
  not built (begin-side tolerance is available through `parse_begin` today).
- D-plan-7 (forced ripple): `ParseDriver::make_paragraph_break_node` gains a
  `source_content: &str` parameter — Specials Option 1 records the run as the
  node's owned `name`, and the hook had no source access.
- D-plan-8 (delegated realization): the canonical paragraph-break spec object is
  the dedicated ZST **`ParagraphBreakSpec`** implementing `CallableSpec<LLL>`
  for every family member; identity is **type identity** (Any-downcast) — a
  per-`LLL` shared `Arc` singleton is unrealizable (`no_std`, generic statics).
- D-plan-9 (delegated design): the umbrella's InvocationSyntax bound is
  `LatexlikeInvocationSyntax<Self> + FromInvocation<Self>` — the preset's
  staging sites (paragraph-break pillar; the LLL-generic machinery reaching the
  std engine) otherwise need per-item conditional impls, and any family member
  driving the std engine needs `FromInvocation` regardless (D-plan-2).
- D-plan-10 (delegated naming): the per-side record is
  **`EnvironmentSideSyntax<L>`** (fields exactly as ruled).
- D-plan-11 (delegated design): core `NodeRef::post_space()` is deleted with the
  field; core gains `NodeRef::invocation_syntax()`; the latexlike sugar provides
  `post_space()` over the payload (Macro → recorded; Environment/Specials →
  `Some("")`, preserving prior latexlike read semantics).
- D-plan-12 (delegated realization, strata tension flagged): the in-crate
  parse-law checker's callable arm reads the payload via `Any`-downcast to the
  shipped payload types (`()`, the latexlike enum) — `#[cfg(test)]`-only code in
  core referencing the preset; alternative (a latexlike-side pin helper) loses
  the single-oracle property. Flagged for review.
- D-plan-13 (delegated signatures): spelling writers
  `write_begin(&self, name: &str, source_content: &str) -> String` (+
  `write_end`) — value-returning (S8's Piece monoid consumes Strings).
- D-plan-14 (delegated design): `EnvironmentInvocation` gains the spelling
  fields the generalized verbatim literal composition needs
  (`escape_char`, name-group open/close) — its `#[non_exhaustive]` docs sanction
  growth by field.
- D-plan-15 (routed from S4): `MacroSpec<LLL = Latexlike>` and
  `argument_specs`/`argument_specs_from_str` generalize over `LLL`.
- D-plan-16 (compiler-forced, successor 1): the generalized argument-code
  factory fns (`argument_specs`, `argument_specs_from_str`, and their private
  helpers) carry `where ArgumentExt<LLL>: Default` — they construct the std
  argument parsers, whose `ArgumentParser<L>` impls discharge that bound at
  spec construction (the recorded ArgumentExt-precedent from D-plan-2's bound
  spread). Consequence: error-shape unit tests whose result never reaches a
  typed spec need a `::<Latexlike, _>` turbofish (the factory has no defaulted
  type parameter — Rust fns cannot default them); ordinary embedder use infers
  `LLL` from the receiving `MacroSpec`/`Package`.
- D-plan-17 (handoff-choice repair, successor 1): the handoff's Macro
  childless-post-space pin ("ends the node's span when childless" — the old
  invariant-3 arm) contradicts the T5-B `end_pos: Some` rule the same stage
  landed: the stage_invocation takeover test
  (latexlike/invocation_syntax.rs `RestOfLineSpec`, `\title The Title\n` →
  childless span 0..16, trigger post-space 6..7) is exactly the sanctioned
  consumed-extent-outruns-children shape, and the `==`-pin panics on it.
  Record-consistent repair implemented: the Macro arm pins the **spelling
  fact** instead (escape char + name-as-written as the span's byte prefix —
  the acceptance's "macro spelling facts" verification), the post-space start
  (immediately after that spelling), and the end — `==` first child's start
  when children exist, `<=` span end (containment) when childless, since the
  oracle cannot distinguish the std childless shape (trailing) from a
  takeover's claimed extent. Std shapes lose no coverage: their trigger end IS
  the span end, and the start pin is exact.
- S8 note (recorded, not S5's to decide): a *malformed* terminator (`\end y` —
  command consumed alone) records no end-side facts, so payload-only reemission
  of that recovered shape cannot reproduce the consumed `\end` bytes; the
  tolerant oracle matrix (S8) must exclude or special-case it, or a partial
  end-side record must be added then.

## FLM probe adaptation (M4 — every edit recorded)

dev-docs/api-review/walkthroughs/framework/flm_projected.rs, adapted to the
landed S1–S5 surface (the file remains a projection as a whole — the S9
registration one-liners and the S7 restage pass keep their markers):

1. Header rewritten: landed-vs-projected status per construct; `[IS]` source tag
   added ([§dd-dr:invocation-syntax] + Round 2, landed S5).
2. Import drift: `LatexlikeLangBoundsEtc /* placeholder */` dropped from the
   `techy::core` use (the umbrella is `latexlike::LatexlikeLang`);
   `builtin_package`/`minidefs` marked S9.
3. The `[T5?]` LatexlikeEvent GAP block replaced by the landed trait's actual
   impl for `FlmEvent` (`exit_math_context()`/`is_exit_math_context()` — ruled
   T5 C1, landed S4).
4. Lang impl gains `type InvocationSyntax =
   latexlike::InvocationSyntax<latexlike::StdEnvironmentSyntax<Flm>>;` with the
   D-plan-4 (explicit Env, no `L` param on the enum) and D-plan-9/D-plan-2
   (umbrella bound; `FromInvocation` via the enum's standard-site constructor —
   zero payload code to drive the std engine) notes.
5. `resolve_state_event` S4 drift fixed: `StateStackView<'_, Flm>` →
   `ParsingStateStack<Flm>`, `stack.states()` → `stack` (the `[T5?] point E`
   marker resolved — the pillar consumes the owning stack directly).
6. Registration comment updated: `MacroSpec<Flm>` + `argument_specs::<Flm, _>`
   landed (D-plan-15/16); the `define_macro` one-liner stays an S9 projection.

Outcome: the S5-relevant construct (a foreign family member whose payload is the
latexlike enum over its own environment record, driving the std engine and the
`LLL`-generic environments machinery) is compile-and-run-checked in-crate by the
`Flavored` tests (latexlike/mod.rs); the probe file matches that landed shape
line-for-line on those constructs.

## Handoff notes (relay — written at the supervisor's ~600k-token cutoff)

**State**: M0–M2 complete and committed (6433232 plan, 28e9574 M1, b8233a4 M2);
working tree clean; `cargo build` / `cargo build --tests` 0 warnings;
`cargo test` all green (608 lib + 30 acceptance + 8 derive-conditions + 1 derive
+ 27 doctests). No half-done files. `rm -rf target/doc && cargo docs` has NOT
been run yet in this stage — expect intra-doc-link fallout in the new rustdoc
(check `crate::latexlike::...` links written from core modules, e.g. in
state/lang.rs and node/kind.rs, and `ParagraphBreakSpec` links written in
latexlike/invocation_syntax.rs BEFORE that type exists — it lands in M3; the
enum doc in invocation_syntax.rs references `super::ParagraphBreakSpec`
already, so M3 must land before the docs gate passes, or the link must be
temporarily de-linked if M3 is reordered).

**What the successor continues with** (original plan sections below still stand;
follow the realization choices already made):

- **M2 addendum (small)**: foreign-LLL environment smoke test — extend the
  `the_generic_driver_serves_a_foreign_family_member` test (latexlike/mod.rs)
  or add a sibling: `Flavored` (whose `InvocationSyntax =
  InvocationSyntax<StdEnvironmentSyntax<Flavored>>`) parses
  `\begin{itemize}…\end{itemize}` and a verbatim environment; requires
  registering `BeginSpec::<Flavored>::new()`/`EndSpec::<Flavored>::new()` in a
  package (Flavored's initial_state_data has an empty scope stack — push a
  package with begin/end + the env specs) and asserting begin/end payload facts
  + `body()` (works: `Flavored`'s SlotExt is `()`, which implements
  BodySlotExt with `is_body() == true` — slot 0 degenerates to the body,
  the S3-D ruling).
- **M3 — MacroSpec<LLL> + argument_specs<LLL> + paragraph-break fix** (plan
  below). Concrete choices already fixed: `MacroSpec<LLL: LatexlikeLang =
  Latexlike>` mirrors SpecialsSpec (pub `arguments` field, `new()`, manual
  Debug/Clone/Default); `argument_specs`/`argument_specs_from_str`/word codes
  generalize via role-trait constructors (`content_group()`,
  `verbatim_group()` for the `v` codes; the minted optional-group rules keep
  their spellings). Paragraph-break fix: new `ParagraphBreakSpec` ZST
  (NON-generic; `impl<LLL: LatexlikeLang> CallableSpec<LLL> for
  ParagraphBreakSpec`; home: latexlike/driver.rs beside ParagraphBreakStyle;
  frame title "specials ‘…’"; requires_content false), minted per break
  (`Arc::new(ParagraphBreakSpec)` — identity is TYPE identity via downcast,
  D-plan-8); `ParseDriver::make_paragraph_break_node` gains a
  `source_content: &str` parameter (D-plan-7 — update: engine/driver.rs trait
  + default, the nodes_parser call site `cx.driver.make_paragraph_break_node(
  &cx.state, &token)` → pass `cx.source.content()` (note: compute before the
  call — cx borrow choreography is fine, `source` is a field), the latexlike
  pillar + LatexlikeDriver impl, and the test driver override in
  nodes_parser tests (~line 1510)); the pillar's Specials arm then uses
  `name: &source_content[token.span.range()]` for BOTH the synthetic
  Invocation's name and the node name (name-as-written), keeps the
  from_invocation consultation, and swaps `SpecialsSpec::default()` →
  `ParagraphBreakSpec`. Update ParagraphBreakStyle::Specials rustdoc (the
  canonical-"\n\n" sentences are SUPERSEDED — name = the actual run; identify
  by spec downcast; also fix the driver.rs test asserting `"\n\n"`) and the
  latexlike/driver.rs paragraph_break_pillar test. Extract helpers
  (content_as_chars) key on node kind, unaffected.
- **M4 — parse-law payload checks + FLM probe** (plan below). The stub to fill:
  `check_invocation_syntax_payload` in node/invariants.rs (cfg(test); already
  called from the callable arm). Realization: downcast
  `(&callable.invocation_syntax as &dyn core::any::Any)` to
  `crate::latexlike::InvocationSyntax` (the default-Env Latexlike type) — on
  hit: Macro arm → if post_space is `Spanned`, assert it ends at the first
  child's span start (or the node's span end when childless) and starts ≥ span
  start (the old invariant-3 positional pin); Specials arm → assert
  `source[span.start .. span.start + name.len()] == name` (name-as-written
  prefix pin; for the Specials paragraph-break style name == the whole span);
  Environment arm → assert `write_begin(name, source_content)` is a byte
  prefix of the node's span slice, and when the end side is Some, that
  `write_end(name, source_content)` is its byte suffix. `()` payloads and
  foreign types: skip. This is D-plan-12 (strata tension: cfg(test)-only
  core→latexlike reference — flagged for review). FLM probe: adapt
  dev-docs/api-review/walkthroughs/framework/flm_projected.rs — add
  `type InvocationSyntax = latexlike::InvocationSyntax<StdEnvironmentSyntax<Flm>>;`
  to the Lang impl, fix the S4 drift (`StateStackView` → `ParsingStateStack`,
  the resolve_state_event signature note, the [T5?] markers for
  LatexlikeEvent are now ruled/landed), note `FromInvocation` is satisfied via
  the latexlike enum impl; record every probe edit in this report.
- **M5 — docs + closure** (plan below). Additional to the plan: the
  `cargo docs` link fallout above; rustdoc sweep must include
  invocation_parser.rs module docs (already reworded), builder.rs `add` docs
  if they name post_space, token/token.rs cross-refs, StdInvocationParser
  contract docs; grep sweep MUST cover the superseded names listed in the plan
  (`CallSyntax`, core `post_space` field spelling `pub post_space` on
  CallableData, canonical-`"\n\n"`, `CallableNodeInvocationSyntax`,
  `new_for_invocation`) — note `post_space` as a *field of the latexlike
  payload* and comment/token vocabulary is sanctioned; only the core
  CallableData field name is superseded.

**Gotchas encountered (do not re-derive)**:

- The FromInvocation bound spread is exactly as recorded in D-plan-2; its full
  landed roster (grep `FromInvocation<L>` / `FromInvocation<LLL>`):
  CallableSpec::make_invocation_parser + ParseDriver::{make_invocation_parser,
  make_nodes_parser, make_group_parser} (method where clauses);
  NodesParser::dispatch_invocation (method) + its ConstructParser impl;
  GroupParser/EnvironmentBodyParser ConstructParser impls (+ the
  EnvironmentBodyParser inherent impl block holding parse_body);
  ParseContext::{stage_invocation, parse_nodes, parse_group};
  Language::{parse, parse_source}; parse_expression_node +
  dispatch_expression_invocation (free fns); ArgumentParser impls for
  Expression/Group/OptionalGroup/CharsGroup/Embellishments/TackOn (beside
  their ArgumentExt: Default clause); nodes_parser test harness fns
  (try_run/try_run_with/run_both/run_both_with).
- Derives on L-generic types spuriously demand `L: Clone`/`L: Debug` — every
  new L-generic record got manual Clone/Debug impls (NameGroup,
  EnvironmentTerminatorFacts, EnvironmentBody, EnvironmentSideSyntax,
  StdEnvironmentSyntax, EnvironmentSpec, BeginSpec, EndSpec,
  StdEnvironmentBehavior, VerbatimBehavior, BodyDeltaOverride).
- The `Env` alias inside EnvironmentInvocationParser::parse is a local
  `type Env<LLL> = <<LLL as Lang>::InvocationSyntax as
  LatexlikeInvocationSyntax<LLL>>::Env;` — keep it, the spelled-out form is
  unreadable.
- environments.rs borrow choreography: `let name_group_rule =
  Arc::clone(&name_group.rule);` is pinned BEFORE building
  EnvironmentInvocation (its `name_group_open/close` borrow from that local);
  the body parser is dropped before `parse_end` runs.
- Core tests that asserted `NodeRef::post_space()` on ()-payload test langs had
  those assertions REMOVED (the fact is no longer recorded there); the
  latexlike payload tests in latexlike/invocation_syntax.rs cover the facts.
  Do not resurrect them.
- The sandbox refuses compound shell commands with here-docs piping into
  python in some shapes — write scripts into the scratchpad dir and run
  `python3 <script>` instead.

**Where the new tests live**: latexlike/invocation_syntax.rs `mod tests` —
macro escape/post-space recording (incl. `@` escape), specials unit arm +
name-as-written, environment begin/end facts + write_begin/write_end
round-trips, verbatim std end facts, unterminated/mismatch end-side-empty,
materialize-through, fifth-role-trait coherence, stage_invocation end-rule
tests (std rule + `end_pos: Some` via a rest-of-line takeover spec).

## Consolidated stage summary (M5 closure)

### Outcome

All milestones implemented (M0–M2 by the original implementer, M2-addendum–M5 by
successor 1), all gates green. The trigger spelling of every callable is now
recorded Lang-owned payload: the core channel (`Lang::InvocationSyntax` +
`InvocationSyntaxData` + opt-in `FromInvocation`) replaces the core `post_space`
field; the latexlike enum records macro escape/post-space, per-side environment
scaffolding (through the `EnvironmentSyntax` accumulator with its spelling
writers), and unit `Specials` under the name-as-written doctrine — paragraph
breaks included (actual whitespace run as name, canonical `ParagraphBreakSpec`
identity). Takeover staging collapsed onto the committed `stage_invocation`
shorthand; `MacroSpec`, the environments machinery, and `argument_specs`
generalized over `LLL`; the parse-law oracle pins the recorded spellings against
the node bytes.

### Signature table (new/changed public surface)

| Item | Signature / shape |
|---|---|
| `core::InvocationSyntaxData` | trait: `Clone + Debug + Send + Sync + 'static` + `materialized(&self, source_content: &str) -> Self`; implemented for `()` (D-plan-1; home state/lang.rs, `techy::core` export) |
| `Lang::InvocationSyntax` | new associated type, `: InvocationSyntaxData`; `()` on every in-crate non-preset lang |
| `core::constructs::FromInvocation<L>` | opt-in constructor trait: `from_invocation(&Invocation<'_, '_, L>) -> Self`; implemented for `()` and the latexlike enum (D-plan-3) |
| `CallableData<L>` | `post_space: TextContent` **replaced** by `invocation_syntax: L::InvocationSyntax` |
| `NodeRef::invocation_syntax` | `-> Option<&'t L::InvocationSyntax>` (core); core `NodeRef::post_space()` **deleted** (D-plan-11) |
| latexlike `NodeRef::post_space` | preset sugar over the payload: Macro → recorded post-space; Environment/Specials → `Some("")` |
| `ParseContext::stage_invocation` | `(&invocation, ParsedArguments<L>, ParsedSlots<L>, Vec<BuildId>, end_pos: Option<usize>) -> ConstructParserResult<L, BuildId>` where `L::InvocationSyntax: FromInvocation<L>` (T5-B; `None` = last child's end else trigger end; no overrides) |
| bound spread | `L::InvocationSyntax: FromInvocation<L>` method-level where clauses through the defaulted factory chain to `Language::{parse, parse_source}` (D-plan-2; roster in the handoff notes) |
| `constructs::NameGroup<L>` | gains `rule: Arc<GroupRule<L>>` (matched name-group rule; `Copy` dropped) |
| `constructs::EnvironmentBody<L>` | gains `terminator: Option<EnvironmentTerminatorFacts<L>>`; `enum EnvironmentTerminatorFacts<L> { Scanned { escape_char, command_word, post_space, name_group }, Literal { span } }` (D-plan-6) |
| `latexlike::InvocationSyntax<Env = StdEnvironmentSyntax<Latexlike>>` | `enum { Macro { escape_char: char, post_space: TextContent }, Environment(Env), Specials }` (unit Specials, name-as-written; D-plan-4) |
| `latexlike::EnvironmentSideSyntax<L>` | `{ escape_char: char, command_word: TextContent, post_space: TextContent, name_group_rule: Arc<GroupRule<L>> }` (D-plan-10) |
| `latexlike::EnvironmentSyntax<L>` | `: InvocationSyntaxData` (D-plan-5) — `parse_begin(cx, trigger) -> ConstructParserResult<L, Option<(NameGroup<L>, Self)>>`, `parse_end(&mut self, EnvironmentSideSyntax<L>)`, `record_std_end_facts(&mut self, command_word)`, `write_begin/write_end(&self, name, source_content) -> String` (D-plan-13) |
| `latexlike::StdEnvironmentSyntax<L>` | `{ begin: EnvironmentSideSyntax<L>, end: Option<EnvironmentSideSyntax<L>> }` — accumulator shape (b) |
| `latexlike::LatexlikeInvocationSyntax<L>` | fifth role trait: `type Env: EnvironmentSyntax<L>`; `macro_form/environment_form/specials_form`; `macro_syntax/environment_syntax/is_specials`; umbrella bound `InvocationSyntax: LatexlikeInvocationSyntax<Self> + FromInvocation<Self>` (D-plan-9) |
| environments over `LLL` | `EnvironmentBehavior<LLL = Latexlike>`, `EnvironmentSpec<LLL = Latexlike>`, `VerbatimBehavior<LLL = Latexlike>`, `BeginSpec<LLL = Latexlike>`/`EndSpec<LLL = Latexlike>` (PhantomData ZSTs); `EnvironmentInvocation` gains `escape_char`, `name_group_open`, `name_group_close` (D-plan-14) |
| `latexlike::MacroSpec` | `MacroSpec<LLL: LatexlikeLang = Latexlike>` (SpecialsSpec pattern: pub `arguments`, `new()`, manual Debug/Clone/Default; D-plan-15) |
| `latexlike::argument_specs` | `fn argument_specs<LLL, I>(codes: I) -> Result<Vec<Arc<ArgumentSpec<LLL>>>, ArgumentCodeError> where LLL: LatexlikeLang, ArgumentExt<LLL>: Default, …` (+ `argument_specs_from_str<LLL>`; role-trait group classes; D-plan-15/16) |
| `latexlike::ParagraphBreakSpec` | ZST beside `ParagraphBreakStyle`; `impl<LLL: LatexlikeLang> CallableSpec<LLL>`; "specials" frame vocabulary; identity = type identity via downcast (D-plan-8) |
| `latexlike::make_paragraph_break_node` | `fn <LLL: LatexlikeLang>(ParagraphBreakStyle, &ParsingState<LLL>, &Token<'_, LLL>, source_content: &str) -> NodeKind<LLL>`; Specials arm records the actual run as name and stamps `ParagraphBreakSpec` (D-plan-7) |
| `ParseDriver::make_paragraph_break_node` | `fn (&self, &ParsingState<L>, &Token<'_, L>, source_content: &str) -> NodeKind<L>` (default unchanged in behavior) |
| parse-law oracle | `check_invocation_syntax_payload` (cfg(test)): Macro spelling-prefix + post-space pins (childless containment, D-plan-17), Specials name prefix, Environment `write_begin`/`write_end` byte pins (D-plan-12) |

### Deviations

D-plan-1 … D-plan-17 (running list above) — D-plan-16 and D-plan-17 added by
successor 1; all queued for user sign-off. The S8 note (malformed terminator
records no end facts → tolerant oracle matrix must exclude/special-case) stands.

### Gate results (final full run)

- `cargo build` and `cargo build --tests`: 0 warnings, 0 errors.
- `cargo test`: 614 lib + 30 acceptance + 8 derive-conditions + 1 derive +
  27 doctests — all green (2 ignored doctests are pre-existing).
- `rm -rf target/doc && cargo docs`: clean — no broken intra-doc links, no
  missing_docs warnings.
- Superseded-names sweep: clean (`CallSyntax`, `CallableNodeInvocationSyntax`,
  `new_for_invocation`, core `CallableData.post_space`, canonical-`"\n\n"` name
  claims — none present; token/comment `post_space` vocabulary and the latexlike
  payload field are sanctioned).

### Commits (60dfd2b → HEAD)

- 6433232 P3-S5: implementation plan
- 28e9574 P3-S5 M1: Lang::InvocationSyntax channel + stage_invocation + latexlike payload
- b8233a4 P3-S5 M2: environments machinery over LLL
- 4725da0 P3-S5: relay handoff notes (M0–M2 done; M3–M5 to a successor)
- a888682 P3-S5 M2 addendum: foreign-LLL environment smoke test
- d339c0c P3-S5 M3: MacroSpec<LLL> + argument_specs<LLL> + paragraph-break name-as-written
- 5f362bb P3-S5 M4: parse-law payload pins + FLM probe adaptation
- ac12223 P3-S5 M5: docs + closure — DR status lines, ARCHITECTURE passages,
  guide fix, doc links, gates green, stage summary
- (plus this commit-hash fix)

### Churn

42 files changed, +3123/−469 (whole stage, docs included; reviewer-corrected
count — the earlier "39 files/~2960/~430" was bookkeeping drift); code portion
(techy/src + techy/tests): 37 files, ~2340 insertions, ~410 deletions.

## M6 — USER-RULED DESIGN REVISION (2026-08-04 session; successor 2)

Implements the interactively ruled revision of the S5 invocation-syntax surface
(PHASE3_PLAN.md stage log, entry "S5 DESIGN-REVISION SESSION RULED" — the
authoritative summary; this section is the implementation plan committed first
per the relay discipline). Rulings R1–R9; escalation, not silent resolution, on
any suspected inconsistency (the new rulings-revision protocol rule).

### Implementation plan

Commit steps (each green):

1. **Plan** (this section).
2. **R1 + R5 + R6 — name swap + `&Source` threading** (one commit; the trait's
   `materialized` signature entangles the rename with the threading):
   - Core bound trait `InvocationSyntaxData` → **`InvocationSyntax<L: Lang>`**
     (state/lang.rs; bounds unchanged; method
     `materialized(&self, source: &Source<L::SourceOrigin>) -> Self`); Lang bound
     `type InvocationSyntax: InvocationSyntax<Self>` (legal: bound paths resolve
     to the trait, the associated type is only nameable as
     `Self::InvocationSyntax`); blanket `impl<L: Lang> InvocationSyntax<L> for ()`.
     Exports: state/mod.rs + core/mod.rs.
   - Latexlike payload enum `InvocationSyntax<Env>` → **`InvocationSyntaxData<Env>`**
     (it IS the data holder — NodeData/CallableData family); all uses ripple:
     the core-trait impl becomes
     `impl<L: Lang, Env: InvocationSyntax<L>> InvocationSyntax<L> for InvocationSyntaxData<Env>`,
     `FromInvocation`/fifth-role impls, latexlike/mod.rs exports + Latexlike's
     `type InvocationSyntax = InvocationSyntaxData;`, Flavored tests, payload-pin
     downcasts, doc links in node/kind.rs + state/lang.rs, FLM probe line
     (`latexlike::InvocationSyntaxData<latexlike::StdEnvironmentSyntax<Flm>>`).
   - `EnvironmentSideSyntax` → **`StdEnvironmentSideSyntax`** (R5; off the trait
     surface — the std record's own component type, stays pub).
   - `StdEnvironmentSyntax`'s core-trait impl is the **diagonal**
     `impl<L: Lang> InvocationSyntax<L> for StdEnvironmentSyntax<L>` (D-plan-19
     below).
   - `TextContent::resolve`/`materialized` take `source: &Source<O>`
     (method-generic `<O: SourceOrigin>`); internally `span.slice(source.content())`.
     Ripple: node/kind.rs materialized chain (`NodeKind`/`GroupData`/`CallableData`
     take `&Source<L::SourceOrigin>`), node/tree.rs `materialize` passes
     `data.span.source()` (each node's OWN source — the multi-source-correctness
     point), node/node_ref.rs accessors (`source_content()` helper → the span's
     source), latexlike/node_ref.rs `post_space()`, node/invariants.rs `text_len`,
     engine/mod.rs + latexlike tests, text_content.rs unit tests.
   - Writers `write_begin`/`write_end` + the side record's `write`/`materialized`
     take `&Source<L::SourceOrigin>` (amends D-plan-13's `source_content` param).
3. **R2 + R3 + R4 + R8 — trait reduction + facts channel + composition owns
   scanning** (one commit):
   - constructs/environment_parser.rs: `EnvironmentTerminatorFacts` →
     **`EnvironmentTerminatorSyntaxData`** (same variants/fields); NEW
     **`EnvironmentBeginSyntaxData<L: Lang>`** `{ escape_char: char, command_word:
     Span, post_space: Span, name_group: NameGroup<L> }` (manual Clone/Debug per
     the file pattern); `EnvironmentBody.terminator` type + docs; verbatim fill
     site + core/constructs.rs exports ripple.
   - latexlike/invocation_syntax.rs: `EnvironmentSyntax<L: LatexlikeLang>:
     InvocationSyntax<L>` reduced to
     `from_parsed(begin: EnvironmentBeginSyntaxData<L>, terminator:
     Option<EnvironmentTerminatorSyntaxData<L>>) -> Self` + the writer PAIR
     (S8's Concat head/tail + the checker's prefix/suffix pins need separate
     pieces). `parse_begin`/`parse_end`/`record_std_end_facts` DIE.
     `StdEnvironmentSyntax::from_parsed`: begin transcribed; `Some(Scanned)` →
     end transcribed; `Some(Literal)` → std-shaped end synthesized from the begin
     side + the `end` command word (`END_COMMAND_NAME`; absorbs the old
     `record_std_end_facts` logic); `None` → end empty. `write_end` on an empty
     end side returns `""` (contract + rustdoc rationale KEPT).
   - latexlike/environments.rs `EnvironmentInvocationParser::parse`: validate the
     trigger is `TokenKind::Command` FIRST — non-command trigger =
     documented-contract violation → `Err(cx.implementation_error(..))`; rustdoc
     the contract ("std environments are command-initiated; custom trigger shapes
     need their own composition + Env type"). Composition scans the rigid name
     group itself (`read_rigid_name_group`; `Ok(None)` → the same MalformedBegin
     chars fallback), builds `EnvironmentBeginSyntaxData`, parses
     arguments/body as today, constructs the payload ONCE at staging via
     `Env::from_parsed(begin, body.terminator)`. BOTH `'\u{0}'` arms die (the
     invocation_syntax.rs arm with `parse_begin`; the `EnvironmentInvocation.
     escape_char` arm becomes transcription from the validated token — field
     stays `char`).
   - R8: `debug_assert!(passthrough.is_none(), …)` (environments.rs, after
     `parse_scoped`) → the implementation-error path (behavior-supplied body
     parsers are outer-layer input). Pre-existing siblings untouched (S10 rider,
     recorded in PHASE3_PLAN §S10).
4. **R7 — payload pins move to the preset**: delete
   `check_invocation_syntax_payload` + the `use crate::latexlike::…` from
   node/invariants.rs (core's callable arm goes payload-blind; the rest of the
   parse law keeps); NEW cfg(test) module `latexlike/invariants.rs` with
   `check_latexlike_tree_invariants` = core `check_tree_invariants` + the payload
   pins (adapted to the renamed types + `&Source` writers), `pub(crate)` use from
   latexlike/mod.rs — mirroring core's exact mechanism (`pub(crate)` +
   `#[cfg(test)]`; techy/tests never reached the parse-law checker — they call
   the public `validate_tree` — so there is nothing to preserve for tests/:
   D-plan-18). The 4 discriminating should_panic pin tests move there. Latexlike
   call sites (latexlike/{mod,spec,test_support,arguments,environments,
   invocation_syntax}.rs) switch to the preset checker; core-side sites stay.
5. **R9 — docs + records + gates**: rustdoc sweep over every touched item (trait
   docs re-written for the reduced shape; tolerance story: "tolerance is a parser
   concern — swap the body/invocation parser via the behavior door; the record
   records" — replacing the same-record/different-tolerance newtype clauses);
   DR [§dd-dr:invocation-syntax] amendment note (accumulator (b) SUPERSEDED by
   `from_parsed` — the internal contradiction: the body parser is the terminator
   consumer, end-side scanning delegation was illusory and shape-locking;
   writers take `&Source`; name swap; specials-trigger error; tolerance
   amendment; D-plan-12 → Option B); [§dd-dr:environment-scaffolding] +
   [§dd-dr:span-invariants] applied-note touch-ups (method names; checker now
   preset-side); [§dd-dr:recompose-machinery] checked (keeps the pair — only the
   `&Source` param mention needed); superseded-names additions (the two OLD ROLE
   assignments; `EnvironmentSideSyntax`; `EnvironmentTerminatorFacts`;
   `parse_begin`/`parse_end`/`record_std_end_facts` as EnvironmentSyntax
   methods; single-writer `recompose_environment` as rejected shape);
   ARCHITECTURE S5 passages (payload enum name; recording-contract phrasing);
   this report's M6 closure (signature rows, deviations, gates).

### New deviations (continuing the numbering)

- D-plan-18 (delegated realization): the preset parse-law checker is
  `check_latexlike_tree_invariants` in the cfg(test)-only module
  `latexlike/invariants.rs`, `pub(crate)`-used from latexlike/mod.rs — the exact
  mechanism of core's `check_tree_invariants` (node/invariants.rs +
  node/mod.rs). No tests/-facing door is added: integration tests never had
  parse-law access (they use the public `validate_tree`, which never carried the
  payload pins), so the minimal equivalent is the in-crate mirror.
- D-plan-19 (delegated realization): `StdEnvironmentSyntax<L>` (and the side
  record) satisfy the L-parameterized core trait **diagonally** —
  `impl<L: Lang> InvocationSyntax<L> for StdEnvironmentSyntax<L>` — not for all
  `(L, L2)` pairs: a lang's environment record materializes against that lang's
  own source-origin type; the broader impl would sanction cross-lang payload
  reuse with nothing to gain.
