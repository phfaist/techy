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
- [ ] M2 — Environments machinery over `LLL`
      (EnvironmentBehavior/EnvironmentSpec/VerbatimBehavior/BeginSpec/EndSpec/
      EnvironmentInvocationParser generic; environment_form via the role trait)
- [ ] M3 — `MacroSpec<LLL>` + `argument_specs<LLL>` + paragraph-break
      name-as-written / canonical spec
- [ ] M4 — Parse-law payload checks + FLM probe adaptation + regression sweep
- [ ] M5 — Docs + closure (DR status lines, ARCHITECTURE, superseded-names sweep,
      full gates, stage summary)

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
- S8 note (recorded, not S5's to decide): a *malformed* terminator (`\end y` —
  command consumed alone) records no end-side facts, so payload-only reemission
  of that recovered shape cannot reproduce the consumed `\end` bytes; the
  tolerant oracle matrix (S8) must exclude or special-case it, or a partial
  end-side record must be added then.

## Handoff notes

(if interrupted, fill in: state, remaining milestones, gotchas)
