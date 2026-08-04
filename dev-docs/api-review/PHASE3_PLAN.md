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
- **Interruption resilience (2026-08-03, after two session-limit kills)**: implementer
  agents write + commit their full implementation plan into the stage report FIRST,
  before any code work, then commit at every coherent milestone — an interrupted run
  loses at most one milestone. A limit-killed agent is RESUMED (same agent, transcript
  context intact) once the limit resets — never relaunched from scratch.
- **Oversized stages are relayed, not carried (2026-08-03, user-prompted)**: when an
  implementer's context balloons (≈400k+), it finishes its current milestone, commits
  handoff notes, and stops; the remaining milestones go to fresh successor agents
  working SEQUENTIALLY in the same worktree/branch off the committed plan (parallel
  agents rejected for intra-stage work: the milestones share files). One independent
  review still covers the whole stage diff at the end.
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

### S2 — Engine init + resolver strategy  [status: DONE — merged 2026-08-03]

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

### S3 — Node core: identity, annotations, ext minting, roles, navigation  [status: DONE — merged 2026-08-04]

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

### S4 — Preset generalization + state-stack events  [status: DONE — merged 2026-08-04]

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
  [§dd-dr:preset-driver-pillars], T3 §D, T5 §D+E. **User amendment 2026-08-04**:
  the restore excludes the transient gates `expecting_group_close` and
  `temporary_groups` (never restored).
- E4 enclosing-state stack: session-held stack; owning `ParsingStateStack`
  (`from_states`, `from_node_ancestors` — scan semantics); fallible
  `finalize_transition` (→ `DeriveError`; seed exempt); `cx.derive_state` /
  `cx.with_derived_state`; `ParseDriver::resolve_state_event(&event,
  &ParsingStateStack)`; preset restore event + exit-math wiring; two-class event
  contract docs; `\text` guide-recipe forbidden_chars fix.
  [§dd-dr:enclosing-state-stack], T1T2 §E4, T5 §E.
- `ClosedVocabulary` stays opt-in (no supertrait). A1(iv) — the bound-where-used
  check fn AND its parse-init wiring — routed WHOLLY to S9 (supervisor
  instruction at S4 launch, confirmed; lands beside its condition type in the F5
  batch). T3 §E2.
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
  parse-init all-escape-char provider warning — INCL. the A1(iv)
  bound-where-used check function + its parse-init wiring (T3 §E2 realization;
  routed here from S4) — reserved identifier `core.specs.…`, wording here;
  `Package::insert` doc callout; `"BracedOnly"` word code; `_named` accessors →
  `Result<Option<NodeSlice>, E>`; A4 docs. T1T2 §A1–A4.
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

## How to resume with a fresh session (cleared context)

1. Read PLAN.md (master), then THIS FILE fully. Stage statuses above are
   authoritative; each stage's full detail lives in
   `dev-docs/api-review/reports/S<N>_REPORT.md` (implementation plan, signature
   tables, deviations, handoff notes) — read the report of the last DONE stage
   plus any IN PROGRESS one before acting.
2. Per-stage cycle (established, works): (a) IMPLEMENTER agent with worktree
   isolation, branch `phase3-s<N>-<slug>`, brief = § Protocol + § S<N> + the
   ruling inputs listed in that section + relay discipline (plan-first commit;
   commit per milestone; if context balloons, finish milestone, commit handoff
   notes, STOP for a successor) + gates + report file. (b) REVIEWER agent — NO
   worktree isolation; it works read-only in the implementer's worktree path;
   re-runs all gates, verifies the diff against the ruling records, produces a
   deviation verdict table (FORCED / DEFENSIBLE / OVERREACH). (c) Deviations go
   to the USER for sign-off BEFORE merge — hard rule; trivial doc-only
   should-fixes may be applied pre-sign-off. (d) Merge: update stage status +
   log here; commit process files on `api-review` (primary checkout,
   /Users/philippe/projects/techy); `git rebase api-review` INSIDE the stage
   worktree (NOT in the primary checkout — repeated past mistake); `git merge
   --ff-only <branch>` in the primary checkout; verify `cargo build` +
   `cargo test` on merged api-review; `git worktree remove <path>` +
   `git branch -d <branch>` (a "could not lock config file" warning during
   worktree removal is cosmetic).
3. Interruptions: session-limit kills → RESUME the same agent by sending it a
   message after the limit resets (transcript context survives; never relaunch
   from scratch). Safety-flag kills → relaunch a FRESH agent with the same
   brief (resuming may re-trigger the flag).
4. Commit messages: `P3-S<N>: <what>` on stage branches; "API review Phase 3:
   <what>" for process-file commits on api-review; harness trailers apply.
5. Additional Phase 3 obligations beyond the stage sections: the "Phase 3
   checklist additions" consolidated in PLAN.md's decision log (NEXT bullet),
   plus per-stage riders recorded in the stage sections and reports. S10 audits
   ALL of them.
6. NEXT ACTION when resuming: check the S4 status line below — if not yet
   merged, finish its cycle first (see stage log). Then launch S5 per § S5.

## Stage log

- 2026-08-03: PHASE3_PLAN.md created; S1 launched (worktree branch
  `phase3-s1-topology`).
- 2026-08-03: S1 implemented (6 commits; all gates green; 202-item surface audit
  exact). Independent review: conformant, 1 doc-only blocker (README missed) +
  should-fix (derive doc identifier). Fixes verified and committed (7th commit;
  finished by the supervising session after the implementer hit a session usage
  limit). Reports: reports/S1_REPORT.md. Merged into api-review.
- 2026-08-03: S2 launched (worktree branch `phase3-s2-engine-init`).
- 2026-08-03: S2 implemented (5 commits; gates green). Review verdict: conformant,
  no blockers; deviations D1 (sealed-conversion inference markers), D2 (`Arc<R>`
  forwarding impl removed for the no-double-wrap pass-through), D3
  (`StdParseDriver<R = (), O = Option<String>>` origin param) — all verified
  FORCED (compiler arguments reproduced) and **user-confirmed 2026-08-03**;
  D4 `IntoCallableSpec` delegated-name accepted; D5/D6 not deviations.
  Should-fix ([§dd-dr:resolver-contract] smalls line) applied. Reports:
  reports/S2_REPORT.md. Merged into api-review.
- 2026-08-03: S3 launched (worktree branch `phase3-s3-node-core`). Implementer
  killed by session limit after ingest; resumed after reset (plan-first commit
  discipline added). At ~450k tokens (user-prompted), converted to a relay:
  milestones A (tree tags + annotations) + B (ext minting) landed green by the
  original agent (4 commits incl. plan + handoff notes); milestones C1–I to fresh
  sequential successors in the same worktree. Attached byte-tiling flag resolved
  from the records: exclusion belongs to the parse-law side (S6 rider);
  `validate_tree` does no byte accounting (T5 §F2/F5).
- 2026-08-03: S3 successor 1 launched (milestones C1 slot roles + C2 constructor
  reshapes).
- 2026-08-03: S3 C1+C2 landed green (commits 52a4d94/2ceafcb/d812c74). Deviation
  **D-C1** queued for user sign-off at stage end: `ParsedArgument` ext =
  `Option<ArgumentExt<L>>` (`Some` ⟺ provided), `absent(spec)` ext-free —
  compiler-forced (universal absent-construction sites cannot mint) and mirrors
  the ruled `RestagedArgument::absent(spec)`. Delegated decisions queued with it:
  record arities (payload-first `new(region, name, role, ext)` family),
  `impl BodySlotExt for ()` (is_body = true; body() degenerates to slot 0 on
  no-ext langs). Successor 2 launched (D level-0 restage_node + E navigation +
  F slice contracts).
- 2026-08-03: S3 D+E+F landed green (commits 340e9f8/34c85f7/5ed9524/c74efcf).
  "honest cost" idiom question supervisor-resolved: T5-F1 ban is scoped to the
  slice contracts (targets the session-coined term), pre-existing ordinary-English
  uses stay. Successor 3 launched (G validate_tree + wrapper, H display_tree +
  as_str, I docs + stage closure + full gate run).
- 2026-08-04: S3 implementation COMPLETE (successor 3 resumed after a second
  limit kill, no work lost; commits 2e54bc8/59cef95/ba60299). 14 commits total,
  ~44 files, +5000/−1250, 542→576 lib tests, all gates green. Whole-stage
  independent review launched (full diff + consolidated deviation list).
- 2026-08-04: S3 review verdict: merge-ready after sign-off, 0 blockers, 1
  should-fix (unused test import — applied, 8f11869); restage translation
  arithmetically verified; D-C1 compiler chains reproduced; both
  supervisor-resolved readings CONFIRMED; relay seams coherent. **User confirmed
  D-C1 + all delegated decisions 2026-08-04.** Merged into api-review.
- 2026-08-04: S4 launched (worktree branch `phase3-s4-preset-generalization`;
  relay discipline from the outset; milestones M1 math-form, M2 role traits +
  LatexlikeLang, M3 E4 state-stack machinery, M4 pillars + LatexlikeDriver<LLL>,
  M5 docs/closure; A1(iv) check fn routed to S9).
- 2026-08-04: S4 implemented in one run (8 commits, 596 lib tests, gates green).
  First reviewer killed by a false-positive safety flag post-gates; fresh
  reviewer relaunched. Verdict: merge-ready after sign-off, 0 blockers;
  behavioral analyses (a) whole-rules restore sound via the group-descent
  invariant (3 probe tests) and (b) patch-merge precedence consistent with the
  E4 record; deviation table D-plan-1..10 + applied specifics all
  recommended-accept; should-fix discriminating «» close-expectation parity
  test added (2eacab3, 597 lib tests).
- 2026-08-04 (user): S4 deviations CONFIRMED, with one **ruling amendment**:
  the exit-math restore must NOT restore transient/temporary gates —
  **`expecting_group_close` and `temporary_groups` are excluded** from the
  whole-`TokenRules` restore (amends T1T2-E4's "whole TokenRules of the found
  state"; durable record = amendment note on [§dd-dr:enclosing-state-stack]).
  Fix applied on the stage branch before merge; then merged into api-review.
- 2026-08-04: S5 launched (worktree branch `phase3-s5-invocation-syntax` off
  api-review 60dfd2b; relay discipline from the outset; scope = §S5 + the
  recompose-session checklist additions in PLAN.md's decision log + the S4
  routings: MacroSpec/environments LLL generalization, fifth role trait,
  FLM-probe re-check).
- 2026-08-04: S5 relay conversion at ~600k tokens (user-prompted): original
  implementer landed M0–M2 green (4 commits through 4725da0; 608 lib tests;
  core InvocationSyntax channel + stage_invocation + latexlike payload +
  environments-over-LLL) + committed handoff notes; deviations D-plan-1..15
  queued (D-plan-2 engine-wide FromInvocation bound spread flagged as the
  main sign-off item). Successor 1 launched in the same worktree for
  M2-addendum + M3–M5.
- 2026-08-04: S5 implementation COMPLETE (successor 1 finished M2-addendum +
  M3–M5 in one run; 9 commits through b56bf0c; 614 lib tests; gates green;
  FLM probe adapted — six itemized edits). New deviations D-plan-16
  (compiler-forced ArgumentExt Default bound on the generalized argument-code
  factory) + D-plan-17 (handoff post-space-pin repair vs the T5-B end_pos
  takeover shape). S8 flag recorded: malformed terminators record no
  end-side facts (tolerant oracle matrix must account). Whole-stage
  independent review launched (full diff 60dfd2b..b56bf0c + D-plan-1..17
  verdict table).
