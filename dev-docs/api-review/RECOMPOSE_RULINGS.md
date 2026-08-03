# Recompose Design Session — Interim Rulings

Session opened 2026-07-31. Brief: RECOMPOSE_BRIEF.md (verified against 3ae9c67).
Updated every round (session pattern). Durable records at close: DESIGN_RATIONALE
amendments/entries per the brief's closing list.

Proposed round order (brief §Session logistics):
1. Point 5 — doctrine operationalized + gap inventory → R7, R8, R15
2. Point 6 — trigger-spelling residue (S1, escape char, Hidden reconciliation,
   knock-ons) → R9–R12
3. Point 1 — architecture (direct fold) → R1, R2
4. Point 7 — read-only walker → R13
5. Point 2 — state threading → R3, R4
6. Point 3 — output sink → R5
7. Point 4 — targeted replacement → R6
8. Point 8 — Attached-exclusion rule → R14
9. Naming sweep — Qs 1–7

## Recomposer discussion (user-requested; in progress)

- Round A (2026-08-03, pending ruling): shared visit engine ACCEPTED in
  direction walker-on-recomposer-core (user challenge upheld; brief's
  separation argument only refuted recompose-on-walker). Placement: user
  vetoes core::node → candidates techy::visit / techy::walker /
  techy::traverse; restage stays techy::transform (confirmed, P4).
  Decoupling directive (user): recompose machinery is meaning-free — it
  composes generic *bits* (default strings) over the visit; source
  recomposition is ONE Recomposer impl (latexlike), never a machinery
  default → Bit-generic value-fold design presented (sink concept dropped
  from machinery; streaming = recomposer-held writer with Bit=()).
  RULED (2026-08-03): RecomposeCx with self-passing methods accepted (user;
  surface kept minimal). Instruction enum RULED with **user amendment**:
  `Concat { head, sep, tail }` / `ConcatWith { state, head, sep, tail }`
  (joiner shape: head + child₁ + sep + … + childₙ + tail) + constructor
  shorthands (children()/wrap(head,tail)/join(sep)); **ComposeBit option
  (a) RULED** (empty/append monoid trait, techy impls String + (); gains a
  `Clone` requirement — sep is duplicated per gap). Visit scaffold shape
  accepted. VisitCx-state question RULED (user "Good", 2026-08-03): the
  engine context holds NO user state — three-channel discipline stands
  (recomposer/visitor `&mut self` fields for run-spanning state; driver
  locals + call stack for fold accumulation; argument-threaded S for
  downward context); a walk needing scoped state = a Recomposer with
  Bit=(); VisitCx carries engine bookkeeping only (depth, tree access,
  cross-tree guards). **Trait design CLOSED.**
- Round B (2026-08-03) RULED: **wrapping contract** recorded (instructions
  lower against the outermost recomposer → layering is free; wrap-intended
  recomposers return instructions, never descend explicitly).
  **ConcatSpec adopted** (single Concat(ConcatSpec) variant: head/sep/tail +
  optional derived state + scope; chainable constructors). **User
  amendment: Concat skips `Attached` by DEFAULT** (widening is the opt-in:
  include_attached(); presented completion pending confirm: Hidden also
  default-skipped per its ruled "generic machinery ignores" definition →
  SourceRecomposer needs no scope call at all; walk stays role-blind —
  the read/compose asymmetry IS the ruled role semantics). **Core helper
  RULED** (instruction-returning free fn, B: ComposeBit + From<&str>).
  **SourceRecomposer RULED** (public SourceRecomposer<LLL>, State=(),
  Bit=String, instruction-only, coherence error variant). User addition:
  mirror the restage helper family → RecomposeCx argument/slot helpers
  (roster presented). Targeted replacement = wrapper pattern +
  documented restage→recompose pipeline; session point 8 folded in.
- Round C (2026-08-03) RULED: **Hidden default-skip confirmed** (Concat
  default scope = plain children + Content regions; widening explicit via
  include_attached()/include_hidden(); read/compose role asymmetry frozen
  as the ruled semantics). **RecomposeContext helper roster RULED**
  (recompose_argument / _argument_content / _named variants (Result per
  the _named convention) / _slot_content_named / recompose_body — final
  spellings at application, restage-family mirror). **Fifth role trait
  RULED** as sketched: `type Env: EnvironmentSyntax<L>` + form
  constructors (macro_form/environment_form/specials_form) + accessors
  (macro_syntax/environment_syntax/is_specials) on the syntax type;
  **EnvironmentSyntax gains spelling writers write_begin/write_end**
  (the Env type owns its own re-emission — accuracy doctrine made
  literal; source_content param resolves span-backed fields). Design
  complete; naming sweep next.
- Round D — naming sweep RULED (2026-08-03/04): Batch A confirmed with one
  **user rename: `Bit`→`Piece`, `ComposeBit`→`ComposePiece`** (Bit's
  binary connotation; `Fragment` recorded-considered [DocumentFragment
  precedent], `Part` considered, `Output` rejected — collides with
  `ConstructParser::Output`); accordingly **`ConcatPieces`** (user) for
  the instruction payload (replacing ConcatSpec — "Spec" is author-side
  vocabulary — and the interim ConcatParts). All other recommendations
  ACCEPTED: **B1** `VisitContext`/`RecomposeContext` (spelled-out Context
  per ParseContext convention); **B2** `recompose::recompose` stutter
  accepted (module=domain whose sole operation shares its name);
  `visit::walk` single entry — `walk_tree`/`recompose_tree` REJECTED on
  one-canonical-path; **B4** trait `FromInvocation`, method
  `from_invocation` (user's `new_for_invocation` spelling superseded);
  **B5** core bound trait named at application, aligned with the
  ext-bound family (fallback `InvocationSyntaxData`); **B6**
  `core_source_instruction`; **B7** RecomposeError variants mirror
  RestageError exactly. Remaining Batch-A names stand as listed.
  **SESSION DESIGN COMPLETE — close-out records drafting launched.**

## Rulings

### Round 1 — doctrine operationalized (2026-08-01)

- **R7 RULED, user-simplified** (replaces the brief's permits/forbids list):
  *Permitted*: reading any field of the node's own payload. *Forbidden*: the
  recomposer resolving any span content — including the node's own span —
  against the source. The brief's permit #2 (own-span subtree emission inside
  the recomposer) is thereby REJECTED. The state-derived-spelling fallback
  rider dissolves: trigger spelling will be payload data (Round 2), full stop.
  **Clarification RULED (2026-08-01)**: resolving the node's provenance
  `SourceSpan` content (`span_content()`) is forbidden to the recomposer;
  resolving span-*backed payload* `TextContent::Spanned` is permitted — an
  internal detail of how a content field is stored (parse trees recompose
  zero-copy). User rationale recorded for the no-fast-path consequence: a
  tree carries no reliable "still fresh from parse" signal, so a span
  shortcut could never be safely gated anyway.
- **R8 RULED, user-reframed**: `span_content()` remains a public consumer
  affordance (level-1 lookup); the recomposer simply never uses it. No named
  "span-verbatim" strategy; no span fast path inside the shipped handler
  (consequence for Round 7 / R6(a) noted: byte-exactness of targeted
  replacement rests entirely on payload completeness after Round 2).
- **R15 ACCEPTED**: in-crate oracle acceptance suite (reemit == input; strict +
  tolerant matrices; multi-source rides T5 I-18). Sharpened by R7-as-ruled:
  the oracle now certifies payload completeness with no span crutch; it can
  only pass once Round 2's recordings land (Phase 3 sequencing).
- **Knock-on routed to Round 2**: `ParagraphBreakStyle::Specials` nodes are
  payload-normalizing (name = canonical `"\n\n"`, latexlike/driver.rs:29–46) —
  under payload-only emission, reemit != input for that configuration; needs a
  payload-completeness decision alongside escape char + scaffolding.

### Round 2 — trigger-spelling storage (2026-08-03)

- **CallSyntax slot role REJECTED outright (user)** — and with it the brief's
  S1 Hidden-slot form (R9), the escape-char core field (R10), the
  Hidden-emission carve-out (R11), and the order-free-tiling builder change
  (R12). Reasons recorded: duplicates information (macro/environment names in
  scaffolding bytes); cannot be a preset-agnostic recomposition mechanism
  (core cannot reconstruct preset-owned constructs); makes transforms
  hazardous (renames require synchronized spelling updates). `SlotRole` stays
  the ruled three-variant enum; `Hidden` stays reserved; builder tiling check
  stays as-is.
- **Accuracy doctrine RULED (user)**: the *preset* (Lang), not core, owns
  recomposition accuracy — byte-exact vs up-to-noise vs loose is the preset's
  choice, implemented by what invocation-syntax information it records in
  node payload, in logical canonical form. Recomposition accuracy is coupled
  to parse-recording accuracy; recomposition reads raw node payload only (no
  hidden slots, no side channels) — extends the Round 1 doctrine.
- **Mechanism direction (user proposal, analysis presented 2026-08-03)**:
  new Lang-associated invocation-syntax type (working name
  `Lang::InvocationSyntax`) stored as a `CallableData` field, replacing core
  `post_space` (and never adding escape_char to core); minted by the
  invocation parser; distinct from `CallableNodeExt` (parse-level syntax vs
  preset-logic info). Latexlike: enum ~ {Macro, Environment, Specials}.
  Specials = option 2: `name` stays canonical, `literal_form` records actual
  bytes. Paragraph-break spec fix directive (user): driver.rs:127 must not
  mint anonymous `SpecialsSpec::default()` per break — a definite,
  identifiable paragraph-break spec object, fixed in all cases.
  `Recomposer` trait (restage-visitor-shaped) + preset `source_recomposer`;
  `techy::recompose(tree, recomposer)` entry. Env parsing: strict default,
  noise-tolerant swappable later.
  Refinements RULED (2026-08-03):
  1. **Two-trait split RULED**: required core data-bound trait on
     `Lang::InvocationSyntax` (Clone + Debug + Send + Sync + 'static +
     `materialized(&self, source_content) -> Self`; `()` impl trivial) +
     separate opt-in constructor trait for the std staging sites
     (StdInvocationParser + the specials site), `new_for_invocation`-shaped,
     implemented for `()` by techy.
  2. **Environment record RULED**: per side { escape_char, command_word,
     post_space, name-group } with the name group recorded as
     **`Arc<GroupRule<L>>` cloned from the matched token** (user
     counterproposal, verified sound: `TokenKind::GroupOpen` carries the
     matched rule Arc, token.rs:45–53; rule open/close Strings are the exact
     matched bytes, rules.rs:42–50; the name group can never exist in
     delimiter-diverged form — malformed begin takes the chars-recovery path,
     environments.rs:478–493 — so rule==bytes always; Arc is
     source-independent → exempt from materialize; also records the group
     *class*, which byte-recording would lose). Record is Lang-generic
     (`StdEnvironmentSyntax<L>`). End-side facts reported back by the body
     parser (terminator consumer, environments.rs:545–549).
  3. **Specials = Option 1 (user, reversing the earlier Option-2 lean)**:
     `name` = actual invocation spelling always (matches the macro rule:
     '\foo' vs '\fooooo' both spec-resolved by prefix still record the name
     as written). `Specials` variant becomes a **unit variant** (no
     literal_form — no two-field rename hazard). Paragraph-break Specials
     nodes record the actual whitespace run as `name`; the canonical-"\n\n"
     contract is superseded; identification is by **spec identity** — the
     canonical paragraph-break spec object (driver.rs:127 fix) is now
     load-bearing, not just hygiene.
  4. **Env-payload specification mechanism — CONVERGED DESIGN (user
     consolidation, 2026-08-03, pending final confirmation)**: everything
     anchors on **Env itself**; the LLL defaulted-method tier is DROPPED
     (user worry: too many customization entry points on Lang — upheld).
     Single customization entry = the Lang's choice of `InvocationSyntax`
     type. Preset enum generic with type-parameter default
     `InvocationSyntax<Env = StdEnvironmentSyntax<L>>`; fifth role trait on
     the syntax type carries `type Env` + form constructors + accessors
     (details → dedicated recomposer discussion). NEW env-syntax trait
     implemented by Env types consolidates scanning + payload construction;
     `EnvironmentInvocationParser` becomes generic (over LLL), delegating
     begin/end *scanning* to Env while resolution + argument parsing stay
     composition-owned (parse_begin must return the name info the
     composition needs). Shape options presented: (a) artifacts (assoc
     types + create_syntax_payload — user sketch) vs (b) accumulator
     (parse_begin → (NameInfo, Self) with end side empty; parse_end(&mut
     self) fills it; recommended — zero extra associated types, the
     intermediate state doubles as the synthesis constructor's shape).
     **Shape (b) accumulator RULED (user, 2026-08-03).**
     Same-record/different-tolerance = newtype over StdEnvironmentSyntax
     (replaces the dropped LLL-method tier). Verbatim caveat (verified):
     the verbatim terminator is one literal GroupClose token (rules
     replaced, close = full `\end{name}` string, verbatim_parser.rs:5–24,
     106–123) — end-scanning delegation cannot apply to raw bodies; the
     verbatim path records std end facts from the matched literal via the
     one std-facts method the trait keeps.
  5. **`Recomposer` trait name RULED**; module/entry-fn naming must mirror
     transform::restage (module = domain, fn = operation, no stutter) —
     exact names + source_recomposer + fifth-role-trait details go to a
     **user-requested dedicated interactive discussion** (P4 ruled the
     module name `techy::recompose`; user wrote `techy::recomposer` — treat
     the module name as open in that discussion). Round 3 (architecture =
     direct fold, transform-to-chars dead) is settled in substance.
  Honest costs accepted (user): Lang-agnostic tooling sees only
  name + span of foreign callables (by design); variant/callable_type
  coherence unenforced (recomposer error variant); +1 Lang associated type.
