# Phase 2b — T5 session: rulings (FROZEN — session complete 2026-07-31)

Session run 2026-07-31; brief T5_BRIEF.md verified at 4c324c7; all points ruled.
Durable records: DESIGN_RATIONALE new entries [§dd-dr:restage-ops],
[§dd-dr:extract-annotations], [§dd-dr:tree-validation] + amendments on
[§dd-dr:restage], [§dd-dr:node-annotations], [§dd-dr:recompose],
[§dd-dr:slot-roles], [§dd-dr:input-attachment], [§dd-dr:tree-navigation],
[§dd-dr:enclosing-state-stack], [§dd-dr:preset-driver-pillars],
[§dd-dr:latexlike-generalization], [§dd-dr:takeover-staging-sugar] +
superseded-names T5 block + ARCHITECTURE refs ([§dd-arch:nodes] section).
PLAN.md decision log holds the headline summary; this file holds the full
working detail.

## Ruled

- **C1 (headline, pre-ruled ahead of round C)**: fourth role trait
  **`LatexlikeEvent`** adopted as briefed — constructor + recognizer for the
  exit-math-context event (`exit_math_context()` / `is_exit_math_context()`),
  bound on `LatexlikeLang::Event`, coherence contract mirroring `math_form`,
  defaulted-method evolution posture. Amendment on
  [§dd-dr:latexlike-generalization] at close.
- **E (direction + naming RULED)**: the exit-math pillar KEEPS the stack
  parameter type (user: more descriptive than a bare iterator; preserves the E4
  hook + T3-D pillar signatures structurally). The type becomes **owning**
  (`Vec<Arc<ParsingState<L>>>`; the session stores its live stack as one and
  lends `&` to hooks) and **constructible outside the session**. **Name RULED:
  `ParsingStateStack`** (user; `ParsingStateDelta`-precedent specificity;
  `StateStackView` + `StateStack` → superseded-names). Constructors:
  `from_states(Vec<Arc<ParsingState<L>>>)` + **`from_node_ancestors(node)`**
  (user; walks node's own recorded state first, then parents outward via the
  P4 parent table — innermost-first/current-state-first, matching E4's ruled
  convention). Ripple: `resolve_state_event(&event, &ParsingStateStack)`,
  `exit_math_context_delta(&ParsingStateStack)`; amendment notes on
  [§dd-dr:enclosing-state-stack] + [§dd-dr:preset-driver-pillars]. Remaining
  for round E: the math two-component recipe (pillar delta + descent
  invariant) doc obligation; walk-vs-session-stack equivalence note (Arc-equal
  duplicates harmless for the first-non-math scan).
- **A1**: callback = trait `RestageVisitor` (self-passing for reentrant region
  ops) + closure blanket for non-reentrant passes; error strategy = **option 1**
  (generic `V::Error` through `RestageError<E>`; `Clone where E: Clone`);
  fallback to fixed-error only if blanket inference proves awkward at
  application (flag, don't re-session). Level-0 primitive
  `NodeTreeBuilder::restage_node(node, replacements, content_parents,
  annotation)` in `core::node`, positional `&[Vec<BuildId>]` replacements,
  **cross-tree `NodeRef` input sanctioned** (G depends on it). No `Send` bounds
  on visitors/closures (H(b) rider, restated at H).
- **A2**: bundles + ops as briefed — opaque-but-constructible
  `RestagedArgument` (`provided(spec, nodes, content, ext)` / `absent(spec)`),
  `RestagedSlot::new(name, role, nodes, content, ext)` (T3 named-first mirrored
  at application); ops `restage_subtree` / `restage_children` /
  `restage_argument[_named]` (unknown name = `Err`) / `restage_slot` /
  `restage_invocation(node, arguments, slots, annotation)` / `builder()`.
  The constructor IS the general take-both form (nodes + `ContentNodes`
  designation) — user follow-up answered in round 1.
- **A3**: **option 1 — no silent repair**, with the user's framing recorded:
  the driver acts exactly like parse-time construction — legal shapes pass
  (emptied region = provided-with-empty-region; absent stays explicit via
  `RestagedArgument::absent`), broken designation is an **error**, matching the
  builder's content-parent-inside-region law; the dedicated
  `RestageError::ContentParentDropped` variant adds only better diagnosis
  (the driver knows the cause + the takeover remedy; the raw builder would see
  a generic dangling id). Also note: `Emit(vec![a, b])` for an old content
  parent makes any auto-re-anchor ambiguous — repair is unprincipled, not just
  unwanted.
- **A4**: helper stays the **narrow content-swap form**
  `restage_argument_with_content` / `restage_slot_with_content` (wrapper +
  noise verbatim by contract); changing noise uses level 1 (visitor op — noise
  flows through the visitor) or level 3 (`RestagedArgument::provided`
  hand-build, the take-both form). A both-taking helper REJECTED as a second
  path duplicating the constructor modulo a one-line spec/ext transcription.
  `stage_argument_like` → superseded-names at close.
- **A5**: builder `add` stays **positional** (six params; order
  identity→provenance→context→structure→lang→consumer); params struct additive
  later if real confusion appears (one-way-door asymmetry).
- **A6**: annotation accessors (`NodeRef::annotation`, `NodeTree::annotations`,
  no setter) + `annotate()` in **storage order** with the loud doc sentence;
  defaulted `A = ()` ripple confirmed as application scope.
- **A7**: **`Restage::Descend(B)`**; `Continue` working name + `Keep`/`Retain`/
  `Auto` → superseded-names at close.
- **A8 (REVISED by user — in present scope; design converging)**:
  `split_at_chars()`/KeyVals annotation handling is in scope NOW; the
  deferral (and its trigger device) is dead; P4's "extract trees are
  `A = ()`" ([§dd-dr:node-annotations]) amended at close. **Clone-through
  proposal WITHDRAWN** (user counterexamples: measure-like annotations go
  stale across a split — an op knowingly minting wrong values fails API
  hygiene even if post-fixable; and output annotations can be
  *split-semantic* — "key"/"value" — information the op uniquely holds at
  mint time). Direction under convergence (user proposal, agreed in
  structure): **general `A → B` callback form** per op
  (make_node_ext-symmetric mint of consumer data; kills the `Clone`/`Default`
  bounds on the general path — synthesized nodes just get a callback call)
  **+ shorthands** per the shorthand-not-second-path rule. **Names RULED
  (user): the GENERAL form takes the bare name** — per producer:
  `split_at_chars(nodes, sep, f) -> SplitAtChars<L, B>` (general `A→B`),
  `split_at_chars_drop_annotations(nodes, sep)` (`B = ()`),
  `split_at_chars_keep_annotations(nodes, sep)` (`A→A`, bound-where-used
  `A: Clone + Default`; no `B` param). Same triple for `parse_keyval`,
  `split_embellishments`, `split_tack_on_fields` (all four producers build
  the backing tree eagerly in the free fn — extract.rs:432–443 — so the mint
  lives there; `into_tree` stays a field move). Result struct REMAINS
  (owns the backing tree + segment view API), renamed
  **`Split` → `SplitAtChars<L, B = ()>`** (std producer-fn precedent:
  `SplitWhitespace`/`CharIndices`); `KeyVals<L, B = ()>` keeps its name.
  Part-context: opaque per-op struct, accessors under the inclusion test
  ("only what the callback cannot recover itself");
  `original() -> Option<NodeRef>` semantic spec (input node this output node
  derives from; `None` = synthesized `List` wrappers/root) +
  `is_partial()`/`partial_text()`; KeyVals keys are plain strings
  (extract.rs:789–800) so no key-side annotations — discriminant = entry
  index + part info; final accessor names deferred to the application naming
  pass (user). Boundary recorded: annotation minting only — transformation
  is restage's job. **A8 CLOSED** (user confirmed the name flip, the
  parse_keyval-triple correction, and keeping the result struct as
  `SplitAtChars`).
- **E remainder RULED**: (1) two-component math-interior recipe documented on
  the pillar (delta + `expecting_group_close` descent invariant; composed
  `…interior_state()` helper rejected — two-line composition, wrong under
  overridden math plugs); (2) `from_node_ancestors` contract = **scan
  semantics, not stack identity** (Arc-equal duplicates + non-group ancestors
  harmless); (3) paragraph-break pillar documented parse-side-only. E fully
  closed.
- **D RULED — add nothing**: three orthogonal config knobs (`recovery`,
  `paragraph_break_style`, resolver); `with_group_interior_delta` closure-knob
  REJECTED (re-grows a behavior-carrying driver; pillars compose in a custom
  driver — one doc sentence at the struct). Application riders: rewrite the
  mooted `Copy`/`Eq` comment (driver.rs:121–123; minting stays); visibility
  stays recovery/paragraph_break_style `pub`, resolver private behind
  `with_resolver`/`source_resolver()`.
- **A9**: (i) `body()` = ext axis only, no role conjunction (doc sentence);
  (ii) readers/extract **role-blind everywhere except recompose** + doc note
  pinning `Hidden` = recompose/byte-accounting semantics, not read-invisibility;
  (iii) `SlotRole` **exhaustive** (match-heavy consumers; new role = conscious
  breaking change; `MathGroupForm` argument); (iv) **`Attached`** confirmed
  (`Derived` considered-and-closed); (v) restage **descends uniformly into
  `Attached` AND `Hidden`** slot children (user-confirmed; no role-conditional
  driver behavior; protective verbatim-copy is one explicit visitor arm; doc
  sentence at application).

- **B RULED (all three)**: `stage_invocation(invocation, arguments, slots,
  children, end_pos)` adopted as briefed — (1) `end_pos: Option<usize>`,
  `None` = the std rule (last child's end, else trigger end,
  invocation_parser.rs:178–182), `Some` for consumed-extent takeovers;
  (2) NO `callable_type`/`name` overrides — transcription-case shorthand only;
  environment-class composition stays on the canonical `cx.stage_node` door
  (in-crate: `StdInvocationParser` + tack-on collapse onto the helper;
  environment parsers stay on the door); (3) parse/restage symmetry is by
  **vocabulary, not arity** (caller-tiled records vs driver-tiled bundles =
  who owns region arithmetic) — asymmetry-by-design recorded durably so the
  two signatures are never "unified". No ext/annotation params (`stage_node`
  mints; parse annotations `()`). Durable record on
  [§dd-dr:takeover-staging-sugar].

- **C RULED/ACKED in full**: C1 `LatexlikeEvent` (ruled earlier), C2 driver
  residue verified as ruled (+ Phase 3 checklist item: acceptance asserts
  residue ≤ ~30 Lang + ~12 driver lines), C3 projection notes all clean
  (initial_state_data composes ruled-generic parts; ClosedVocabulary
  provide-don't-require honest; std argument parsers via `ArgumentExt:
  Default` bound-where-used; no `finalize_node` pillar — ledger #1 amendment
  at close).

- **F partially RULED**: **F2** `validate_tree` returns `Result` (no panic;
  `TreeViolation { node: Option<NodeId>, kind, … }` non_exhaustive);
  `check_tree_invariants` keeps its panicking test-utility role. **F4** name
  `validate_tree` confirmed. **F5** parse-law-checker `Attached`-scoping rider
  confirmed (rides the input-wiring application; "all-trees law" ⊂
  "parse-tree law" cross-reference). **F3 home re-ruled by user challenge:
  `core::node`, NOT `techy::transform`** — the function checks the all-trees
  law and accepts ANY tree; transform output is merely the common client, and
  P1's placement-by-logical-function puts a node-tree validator in the node
  domain (the brief's transform-audience argument recorded as considered and
  overruled).

- **F1 RULED (F now fully closed)**: `span()`/`source_text()` answer only for
  whole-run single-source slices — full-run uniformity verification with the
  `finish()` single-source fast-path flag; `source_text()` gains the ordering
  guard for contract parity; `None` = no single-source answer. Representation
  of mixed-origin runs unrestricted (per-node accessors always valid).
  **User doc-vocabulary amendment: the word "honest" must NOT appear in the
  rustdoc contracts** — it's vague session jargon; docs state the concrete
  condition ("the run lies within a single source") instead. "Honest slices"
  stays internal design-record vocabulary only.

- **G RULED (user; revises the brief's recipe recommendation)**: input
  caching is neither implemented NOR recommended. **User-identified flaw in
  the proposed recipe**: `\input` can return a **modified parsing state** to
  the caller — the included content's own delta sequence continues into the
  rest of the including document (preset-configurable by how the `\input`
  spec is defined; the preamble-defines-macros case is the paradigm). The
  recipe's parse-without-attachment step silently assumed state-transparent
  `\input`; when it isn't, everything downstream of the `\input` node parses
  in the wrong state — the file must be read **on the spot** at parse time,
  and caching would have to happen parse-side (the rejected door shape).
  Ruling: **no path down this road.** Phase 4 include chapter gets a SHORT
  discussion of the challenges + the splice recipe presented ONLY under the
  explicit precondition that the framework defines `\input` as not modifying
  the caller state. Parse-time cached-splice door stays REJECTED; canned
  transform op stays premature-additive. Riders kept: (a) the cross-tree
  level-0 primitive stays sanctioned (no same-tree debug-assert, ever — A1);
  (b) latexpp's verbatim output path needs no splicing (recompose emits
  `\input{file}` per source; per-file pipelines compose). Amendment note on
  [§dd-dr:input-attachment] at close.

- **I rows 9 + 10 + 17 + 18 RULED**: row 9 binding-guide chapter with the
  recorded contents checklist (incl. the no-`Send` note); row 10 post_space
  re-emission = doc-only, not techy's concern (policy → techy-totext);
  row 17 `Diagnostics::into_vec` → Tier-C batch, lean reject; row 18
  multi-source reconstruction test obligation → Phase 3 acceptance checklist
  (rides the `\input` wiring). Recompose boundary notes accepted as flagged
  (walker vocabulary free; targeted replacement recompose-owned; verbatim
  `Attached`-exclusion recompose-owned).
- **H(a) confirmed** (nothing to decide — P4 ruled modules + topology slots;
  validator home settled by F3). **H(b) RULED**: no `Send`/`Sync` bounds on
  restage-visitor or `annotate()` callback parameters (demand-vs-capability
  rationale recorded; parallel variants additive later with their own
  bounds). **H(c) RULED**: the parse-tree law (per-source byte accounting
  incl. tolerant recovery) becomes a **documented, semver-protected
  guarantee** (NodeTree docs + reconstruction chapter; Phase 4 text, the
  commitment recorded now). User amendment "& enforce via validate_tree" —
  resolution pending (see Open: collides with F2's deliberate exclusion of
  byte accounting from `validate_tree`; proposed sibling
  `validate_parse_tree`).

- **H(c) RULED (user-revised; supersedes the brief's option 1 AND the
  interim validate_parse_tree proposal)**: user doctrine — gap-filling is a
  dangerous guarantee to headline (latexpp transforms immediately, voiding
  it); spans give provenance, not output location; **recomposition is
  per-node**: a chars node contributes its content, a callable/environment
  node reconstructs its own scaffolding from recorded data (escape char +
  name + post_space, delimiters, \begin/\end), and recomposition never
  performs inter-node span arithmetic ("apparent gaps") nor reads back
  source text beyond a node's own recorded content. Ruled shape:
  (1) NO framework-facing reconstruction guarantee; NO `validate_parse_tree`
  (the F2 collision dissolves; `validate_tree`/all-trees law stands as
  ruled); (2) the parse-law stays an **in-crate acceptance-suite oracle**
  (rebuilding input from the tree proves lossless parsing — invisible to
  consumers); (3) parse-output span semantics stay documented for the
  analyze-only/span-patch tooling architecture (T4 persona;
  edits-as-byte-ranges on unmodified parse trees) WITH the provenance
  warning (structural edits void inter-node span arithmetic); (4) the
  **per-node recomposition doctrine is a binding input to the recompose
  session**, where P4's span-verbatim strategy is re-examined under it,
  along with the trigger-spelling residue (scaffolding spellings not stored
  in node data). **User rider on (4)**: the `\begin`/`{name}` (and `\end`)
  scaffolding noise could be stored by the environment parser as **`Hidden`
  slots** (e.g. `"begin_tokens"`/`"end_tokens"`, precise form TBD) — turning
  scaffolding spelling into node data; an additional argument for the
  per-node direction; recompose-session agenda item.

## Handoffs out of this session

- **Recompose session** (next): P4 open Qs + read-only walker + verbatim
  `Attached`-exclusion + the T5 binding inputs — per-node recomposition
  doctrine (no inter-node span arithmetic; no source read-back beyond a node's
  own recorded content; span-verbatim strategy re-examined under it) and the
  trigger-spelling residue (user sketch: environment scaffolding as `Hidden`
  slots `"begin_tokens"`/`"end_tokens"`, precise form TBD).
- **Tier-C batch**: `Diagnostics::into_vec` lean reject (I-17) + previously
  logged riders.
- **Phase 3 application checklist**: C2 driver-residue assertion (≤ ~30 Lang +
  ~12 driver lines); F5 parse-law checker `Attached`-scoping; I-18
  multi-source reconstruction acceptance tests; A8 extract input-genericity
  rides the annotation application; slice-contract wording without the word
  "honest".
- **Phase 4 guide obligations**: binding-guide chapter with the I-9 contents
  checklist (Arc+NodeId handles, no-`Send` visitor note, synthesized-node
  recipe via the pillars, severity exhaustiveness, `LineIndexCache` handle);
  include-chapter challenges discussion + the conditional splice recipe (G);
  post_space re-emission guide paragraph (I-10; policy in techy-totext);
  two-component math-interior recipe on the pillar rustdoc (E).
