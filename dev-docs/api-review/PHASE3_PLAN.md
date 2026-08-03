# Phase 3 — Apply + Harden: Execution Plan

Working scaffolding (deleted with this directory at review completion). Governs the
application of ALL Phase 2a/2b rulings. Master status stays in PLAN.md; this file
holds the stage breakdown, per-stage inputs, and acceptance gates.

## Protocol

- **One stage = one worktree branch** (`phase3-s<N>-<slug>`), executed by an agent,
  supervised and gate-checked by the session; stages land **serially** into
  `api-review` (parallelism only within a stage on disjoint modules).
- **Merge authorization (user, 2026-08-03)**: the supervising session may commit and
  merge a stage into `api-review` once confident the stage completed successfully
  (all gates green + diff review against the stage's ruling inputs). Agents still
  never merge; they commit only on their stage branch.
- Gates for every stage: `cargo build` + `cargo test` + `rm -rf target/doc && cargo
  docs` green; no `missing_docs` warnings introduced; superseded names
  (DESIGN_RATIONALE [§dd-dr:superseded-names]) must not reappear; behavior changes
  only where a ruling says so.
- **Context discipline (user, 2026-08-03)**: the supervising session stays lean so
  it can carry all ten stages — detailed verification is ALSO delegated: per stage,
  an independent reviewer agent (fresh context, same worktree) re-runs the gates and
  checks the full diff against the stage's ruling inputs, reporting pass/deviations
  with file/line pointers. The supervisor reads compact reports, resolves flagged
  points (escalating ambiguities to the user), and merges.
- Each stage updates the rustdoc it touches AND the passages of ARCHITECTURE.md /
  CLAUDE.md / docs/ guide pages it invalidates (labels immutable; content current).
- Authoritative inputs per stage: the DESIGN_RATIONALE entries + the frozen rulings
  files cited. INVENTORY.md is the public-item roster (Tier-C rulings override).
  On ANY ambiguity or contradiction between records: stop and surface to the user —
  no silent design decisions (CLAUDE.md rule 1).

## Stages

### S1 — Topology + mechanical renames  [status: DONE — merged 2026-08-03]

Pure relocation/rename; zero behavior change; tests pass modulo paths/identifiers.

- C5 facade topology per [§dd-dr:public-namespace-topology]: internal src modules
  become private; public paths exclusively via facades
  `techy::{source, error, extract}`, `techy::core` (flat hub: Lang/state, token,
  engine), `techy::core::{constructs, specs, node}`, `techy::latexlike` (unchanged).
  ALL root re-exports deleted (single canonical path; root keeps `VERSION`,
  `__private`, `guide`).
- Ruled placement overrides: resolution family (`CommandResolution`,
  `ResolvedCallable`, `CallableQuery`, `CallableSyntax`, `SearchedProviders`) →
  `core::specs`; `ArgumentParser` + `ParsedArgumentNodes` → `core::constructs`;
  `FrameRole` → hub beside `Frame`/`FrameTitle`; `PrefixEntry` pub beside
  `PrefixTable` (hub); `ProvenanceChain`/`ResolvedContent` stay `source`;
  node extract helpers → top-level `techy::extract`.
- `NodeData` → `pub(crate)` (Tier-C Theme C; `check_tree_invariants` demotion waits
  for S3's `validate_tree`).
- Derive `__private` rider: techy-derive emits only `::techy::__private::…` paths.
- Wire-identifier slate applied (frozen table, T4_RULINGS §A; 22 strings + tests).
- Renames: `SimpleLang` → `TrivialLang` (+ repositioned docs, T3_RULINGS §A); free
  `resolve_source` → `resolve_source_reference` (Tier-C R3).
- `VERSION` rustdoc sentence (Cargo package version, always valid semver).
- Inputs: [§dd-dr:public-namespace-topology], [§dd-dr:wire-identifier-stability],
  [§dd-dr:trivial-lang], [§dd-dr:public-visibility-sweep], T4_RULINGS §A,
  TIERC_RULINGS Rounds 1/2/4 + R3, T3_RULINGS §H + sweep, INVENTORY.md,
  NAMESPACE_OPTIONS.md (derive-rider + churn facts).
- Acceptance: public-surface audit — every INVENTORY item reachable at exactly the
  ruled path and no other; identifier-asserting tests updated to the slate; docs
  build clean.

### S2 — Engine init + resolver strategy  [status: pending]

- P2 Language-init: `Language::new(driver, initial_state)` (initial state
  mandatory); `ParsingState::initial()` → `lang_initial()`;
  `lang_initial_with_packages(…)` infallible; `with_provider`/`with_seed_delta`
  removed; `Default for Language`, `LatexlikeDriver::default()`,
  `StdParseDriver::default()` removed. [§dd-dr:language-init] + T1T2 §C + T3 §A+F.
- Sealed-conversion idiom: `IntoSpecsProvider` (packages arg, Arc-free) + the
  spec-side sibling on `Package::insert`/`insert_specials` (param-order flip).
  T1T2 §C2/E1a.
- T3-H extraction: free `resolve_command_in_scopes` in `core::specs`;
  `CommandResolution::resolve_via_scopes` removed. [§dd-dr:resolution-extraction].
- Tier-C R4 driver reshape: `trait CommandResolver<L>` (+ supertraits
  Debug+Send+Sync) with `()` no-op impl (keeps helpful detail message);
  `StdParseDriver<R = ()>`, constructor `new(recovery, command_resolver)`;
  `ScopesCommandResolver { command_type }` → `core::specs`;
  `ParseDriver::source_resolver()` accessor (T4-B1) + `Option<Arc<dyn
  SourceResolver>>` fields + chainable sealed-conversion `with_source_resolver`;
  `NoResolver` deleted; asymmetry rationale in rustdoc AND code comment;
  resolver-choice doc sentence pairing with `TrivialLang`. [§dd-dr:command-resolver],
  [§dd-dr:input-wiring] (driver side only — the parse door is S6).
- Named constructors: `TokenRules::empty()`, `StateData::empty()`,
  `TokenRulesOverrides::disable_all()` (T3 §B2 + wish 21).
- Rider from S1: README.md quick-start still uses `Language::default()` — update
  it (and the `engine/driver.rs:342` doctest) when the `Default` impls go.
- Acceptance: seed-infallibility contract intact (no `finalize_transition` on the
  seed); `StdParseDriver::new(Recovery::Strict, ())` test spelling; call-site sweep
  complete.

### S3 — Node core: identity, annotations, ext minting, roles, navigation  [status: pending]

- Tree tags always-on: `TreeTag(u32)` in `NodeId` `Eq`/`Ord`/`Hash`; foreign-id
  rejection. [§dd-dr:tree-tags].
- Annotations: `NodeTree<L, A = ()>` over `Arc`-shared core; zero-copy
  `annotate::<B>(f)` (storage order + loud doc); accessors `NodeRef::annotation`,
  `NodeTree::annotations`; `A: Clone + Debug + Send + Sync`, no `Default`.
  [§dd-dr:node-annotations], T5 §A6.
- Ext minting: `finalize_node` deleted; required `Lang::make_node_ext` with
  `StagedChildren` (subtree-deep, descent-only); tier-2 per-kind exts REMOVED;
  `NodeExt` loses `Default`; hook-free single positional `NodeTreeBuilder::add`
  (six params, T5-A5); `ParserSession::builder` → `pub(crate)`; staging via
  `cx.stage_node()` (+ `cx.staged_nodes()` read view); `ArgumentExt` carried by
  `ParsedArgumentNodes` (std parsers `where ArgumentExt<L>: Default`); `SlotExt`
  demanded at `ParsedSlot` construction; `BodySlotExt::{is_body, make_body}` +
  `NodeRef::body()`; preset claims `SlotExt`. [§dd-dr:ext-minting].
- `SlotRole { Content, Attached, Hidden }` (exhaustive) on `ParsedSlot`; `Attached`
  excluded from parent byte-tiling. [§dd-dr:slot-roles], T5 §A9.
- Spec/arg constructor reshapes landing with the ext arities: `ArgumentSpec::new(
  parser, name)`/`new_unnamed` (`.named()` removed, sealed parser conversion);
  `StdCallableSpec::new(impl IntoIterator)`; `ParsedSlot::new(region,
  name)`/`new_unnamed`; `ParsedArguments::new(Vec)`/`ParsedSlots::new(Vec)`.
  T3 §C+G.
- Navigation: stored parent table; `NodeRef::parent()`/`index_in_parent()`;
  `SourcePos` (+ `SourceSpan::start_pos()/end_pos()`, `Span::contains`);
  `NodeTree::node_at(&SourcePos)` (deepest, half-open, per-source descent);
  `NodeTree::covering_slice(&SourceSpan)`; `NodeRef::tree()` pub; NO `ancestors()`.
  [§dd-dr:tree-navigation], T4 §E+D.
- Slices: `span()`/`source_text()` single-source whole-run contract (word "honest"
  banned from rustdoc); `finish()` single-source fast-path flag. T5 §F1.
- `validate_tree` (all-trees law, `Result`, `TreeViolation` non_exhaustive) in
  `core::node`; `check_tree_invariants` → `pub(crate)` panic-assert wrapper over it
  (one implementation; informative messages). [§dd-dr:tree-validation], Tier-C §C.
- Level-0 `NodeTreeBuilder::restage_node(node, replacements, content_parents,
  annotation)` (cross-tree sanctioned). T5 §A1.
- `display_tree(node) -> String` free fn (box-drawing, line/col, source-change
  lines). [§dd-dr:display-tree]. `NodeKind::as_str()` (T1T2 §E5).
- Acceptance: navigation/validator/annotation tests; no old ext API anywhere.

### S4 — Preset generalization + state-stack events  [status: pending]

- P3: per-vocabulary role traits (callable accessors `macro_callable()`/
  `environment_callable()`/`specials_callable()` + `is_*`; mode trimmed to
  `math_mode()`+`is_math()`; `LatexlikeEvent` (T5-C1); group/math-form);
  `LatexlikeLang` umbrella (defaulted behavior methods, NO blanket impl); `LLL`
  parameter; `GroupType::Math(MathGroupForm)` (exhaustive), `math_form()` sugar;
  `MATH_DELIMITERS` dissolved into `default_token_rules`; preset `NodeExts` stays
  `()` except `SlotExt` (body marker). [§dd-dr:latexlike-generalization],
  [§dd-dr:math-group-form].
- Pillar functions + `LatexlikeDriver<LLL>` (one-line delegations; `Copy`/`Eq`
  dropped; knobs stay recovery/paragraph_break_style pub + resolver behind
  builder): `math_group_interior_delta` (+ two-component recipe doc),
  `exit_math_context_delta`, `make_paragraph_break_node` (parse-side-only doc).
  [§dd-dr:preset-driver-pillars], T3 §D, T5 §D+E.
- E4 enclosing-state stack: session-held stack; owning `ParsingStateStack`
  (`from_states`, `from_node_ancestors` — scan semantics); fallible
  `finalize_transition` (→ `DeriveError`; seed exempt); `cx.derive_state` /
  `cx.with_derived_state`; `ParseDriver::resolve_state_event(&event,
  &ParsingStateStack)`; preset restore event + exit-math wiring; two-class event
  contract docs; `\text` guide-recipe forbidden_chars fix.
  [§dd-dr:enclosing-state-stack], T1T2 §E4, T5 §E.
- `ClosedVocabulary` stays opt-in; A1(iv) bound-where-used check fn wired at
  parse init (warning emission itself may slip to S9 with the F5 batch). T3 §E2.
- Acceptance: preset behavior parity tests (math entry/exit, paragraph breaks);
  Lang-residue trend toward the C2 assertion (final check S10).

### S5 — Invocation syntax + staging sugar  [status: pending]

- `Lang::InvocationSyntax` assoc type; core data-bound trait (named at
  application, aligned with the ext-bound family; `materialized(source_content)`;
  `()` impl); opt-in `FromInvocation`/`from_invocation`; `CallableData.post_space`
  → `invocation_syntax` field; `Invocation` bundle carries trigger-token facts;
  minted by the invocation parser. RECOMPOSE_RULINGS Round 2.
- Latexlike `InvocationSyntax<Env = StdEnvironmentSyntax<L>>` enum
  (`Macro { escape_char, post_space }` / `Environment(Env)` / unit `Specials` —
  name = spelling as written); per-side env record `{ escape_char, command_word,
  post_space, name_group_rule: Arc<GroupRule<L>> }`; `EnvironmentSyntax` trait,
  accumulator shape (b), `write_begin`/`write_end`; `EnvironmentInvocationParser`
  generic over LLL (scanning delegated, resolution/arguments composition-owned);
  verbatim std-facts path; fifth role trait `LatexlikeInvocationSyntax`.
  [§dd-dr:invocation-syntax].
- driver.rs:127 canonical paragraph-break spec object (spec identity load-bearing).
- `cx.stage_invocation(invocation, arguments, slots, children, end_pos)` — bundle
  carries the `InvocationSyntax` value; in-crate `StdInvocationParser` + tack-on
  collapse onto it; environment parsers stay on `stage_node`.
  [§dd-dr:takeover-staging-sugar], T5 §B.
- kind.rs invariant-3 rewording; parse-law checker's callable arm reads the
  invocation-syntax payload.
- Acceptance: FLM projected probe (walkthroughs/framework/flm_projected.rs)
  re-checked against the new surface; parse-law oracle passes on the strict matrix.

### S6 — \input wiring + multi-source + line/col  [status: pending]

- Door `cx.parse_attached_source(source, state, parser)` (caller-supplied parser;
  fresh inner context; content nodes only; local stray-close recovery; traceback
  `Frame`); bundle `attach_source_reference` beside it; conditions
  `NoSourceResolver` (`core.sources.no-resolver`) + `UnresolvableSourceReference`
  (`core.sources.unresolvable-reference`). [§dd-dr:input-wiring].
- `ResolveError` → `Clone` (`Option<Arc<dyn Error + Send + Sync>>` cause;
  `with_cause`); principle: techy error types uniformly Clone.
  [§dd-dr:resolver-contract].
- `Source::including_sources()`; `check_include_chain(target_key, triggered_at,
  origin_key, max_depth)` in `techy::source` (origin-keyed incl. primary; cycle vs
  depth messages). [§dd-dr:include-chain-helpers]. NO recursion control in core.
- Preset opt-in `input_macro_spec::<LLL>()` (never preloaded; brief-form logic via
  the helpers). No input caching (T5 §G) — docs only.
- Line/col ownership (T4 §F): `LineIndex::line_of` (with line number),
  `line_col_span`; `LineIndexCache<O>`; `LineColProvider` trait + `_with` render
  variants; `DEFAULT_MAX_SCAN_LEN` → 500_000. [§dd-dr:line-col-ownership].
- Parse-law checker `Attached`-scoping (per-source byte accounting).
- Acceptance: multi-source reconstruction tests (T5 I-18); include-chain policy
  tests (self-include legal; helper detects cycles/depth); line/col cache tests.

### S7 — Transform: restage + extract annotations  [status: pending]

- `techy::transform`: `RestageVisitor` trait + closure blanket (no `Send`);
  `RestageError<E>` (`Clone where E: Clone`); `Restage::{Descend(B), Emit}`
  (Descend always descends); bundles `RestagedArgument`
  (`provided`/`absent`), `RestagedSlot::new(name, role, nodes, content, ext)`;
  ops `restage_subtree`/`restage_children`/`restage_argument[_named]`/
  `restage_slot`/`restage_invocation`/`builder()`;
  `restage_argument_with_content`/`restage_slot_with_content`; no silent repair
  (`ContentParentDropped`); descends uniformly into `Attached` AND `Hidden`.
  [§dd-dr:restage], [§dd-dr:restage-ops].
- Extract annotation minting: general `A → B` callback owns the bare name per
  producer (`split_at_chars(nodes, sep, f)` + `_drop_annotations`/
  `_keep_annotations`; same triple for `parse_keyval`, `split_embellishments`,
  `split_tack_on_fields`); `Split` → `SplitAtChars<L, B = ()>`;
  `KeyVals<L, B = ()>`; opaque part-context (`original()`, `is_partial()`,
  `partial_text()`; accessor names at application). [§dd-dr:extract-annotations].
- Acceptance: argument-swap restage round-trip; annotation-flow tests; extract
  triples on all four producers.

### S8 — Visit + recompose + oracle suite  [status: pending]

- `techy::visit`: free `walk`; `NodeVisitor`; `VisitFlow`; `VisitContext` (engine
  bookkeeping only; walk role-blind). [§dd-dr:visit-engine].
- `techy::recompose`: `recompose` entry; `Recomposer` (State/Piece/Error; no
  Send/Sync); `Recompose::{Emit, Concat(ConcatPieces)}` (head/sep/tail; chainable
  `children()`/`wrap()`/`join()`; default scope skips `Attached` AND `Hidden`;
  `include_attached()`/`include_hidden()`); `ComposePiece` monoid (String, ());
  wrapping contract (outermost recomposer); `RecomposeContext` restage-mirror
  helpers; `core_source_instruction`; `RecomposeError` mirrors `RestageError`;
  preset `SourceRecomposer<LLL>` + `source_recomposer()`; targeted replacement =
  wrapper pattern + documented restage→recompose pipeline.
  [§dd-dr:recompose-machinery], RECOMPOSE_RULINGS Rounds A–D.
- In-crate oracle acceptance suite: reemit == input (strict + tolerant matrices;
  multi-source rides S6's tests). R15.
- Acceptance: oracle green across the matrices.

### S9 — Preset definitions + consumer polish (T1/T2 batch)  [status: pending]

- Base package: slim to `\begin`/`\end`; rename `"base"` → `"_builtin"`,
  `base_package()` → `builtin_package()`; `&` removed entirely; `~` + ligatures →
  minidefs. [§dd-dr:base-package] amendment.
- `latexlike::minidefs`: `minilatex_package::<LLL>()` (\emph, \textbf, \textit,
  itemize, enumerate, scoped \item via inner `"minilatex.item"` package + the
  moved specials). [§dd-dr:minidefs].
- F5 measures: did-you-mean detail in `resolve_command_in_scopes` miss arm;
  parse-init all-escape-char provider warning (reserved identifier
  `core.specs.…`, wording here); `Package::insert` doc callout; `"BracedOnly"`
  word code; `_named` accessors → `Result<Option<NodeSlice>, E>`; A4 docs.
  T1T2 §A1–A4.
- Sugar: `define_macro`/`define_environment` on `Package<LLL>`;
  `argument_specs_named`; `Diagnostics::sorted_by_position()`. T1T2 §E1b/E2/E6.
- Acceptance: specials/ligature tests load minilatex; trap-fix tests.

### S10 — Hardening, guards, audit  [status: pending]

- C2 residue assertion (Lang ≤ ~30 + driver ≤ ~12 lines acceptance check).
- `missing_docs` → deny (workspace lint); full clean `cargo docs`.
- cargo-semver-checks baseline established (freeze onset per
  [§dd-dr:stability-rubric]).
- Public-surface audit vs the ruled roster (INVENTORY + all rulings); grep sweep
  for every "Phase 3" rider in DESIGN_RATIONALE/rulings files — each either done
  or consciously routed; superseded-names sweep (none reintroduced).
- PLAN.md Phase 3 checkbox + decision-log entry.

## Stage log

- 2026-08-03: PHASE3_PLAN.md created; S1 launched (worktree branch
  `phase3-s1-topology`).
- 2026-08-03: S1 implemented (6 commits; all gates green; 202-item surface audit
  exact). Independent review: conformant, 1 doc-only blocker (README missed) +
  should-fix (derive doc identifier). Fixes verified and committed (7th commit;
  finished by the supervising session after the implementer hit a session usage
  limit). Reports: reports/S1_REPORT.md. Merged into api-review.
