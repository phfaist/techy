# Tier-C Batch — Interim Rulings (session of 2026-08-03)

Working record, updated every round (established session pattern). Brief:
TIERC_BRIEF.md (verified against 6326db2). Durable records land in
DESIGN_RATIONALE.md + PLAN.md decision log at session close.

## Round 1 — forced-public ratification block (RULED 2026-08-03: confirmed)

All items below stay **pub-and-stable**. Each sits inside a used public
signature (or was already committed by a prior ruling); demotion would have
required demoting used API. Homes per the P1 C5 topology.

- Scopes/provider machinery → `core::specs`: `ScopeOp`, `DefinitionOp`,
  `SymbolEntry`, `ProviderError`, `ScopeStackError`, `ScopeOpError`.
- Token error family → `techy::core`: `TokenError`, `TokenErrorKind`,
  `TokenRecovery`, `PrefixTable` (+ companion `PrefixEntry`, forced pub,
  lives beside `PrefixTable`; old root/module inconsistency dissolves).
- Parse-dispatch types → `core::constructs`: `NodesOutcome`, `StopSpec`,
  `TokenStopCondition`, `TokenStopKind`, `StopCause`, `ChildStateSpec`,
  `GroupChildState`, `InvocationChildState`.
- Parse-traceback family: `Frame`, `FrameTitle` → `techy::core`;
  `FrameRole` keep pub (placement = Round 4).
- Misc: `NodeExtTypes` (hub; P4 reshape is application work), `DeriveError`
  (hub), `ParsedArgumentNodes` (keep pub; placement = Round 4).
- Node module forced/committed keeps → `core::node`: `GroupData`,
  `NodeSliceIter`, `NodeBuildError`, `StagedNodes` (P4 `cx.staged_nodes()`
  read view), `StagedNodeView` (`StopSpec.node` predicate).

## Round 2 — conditions doctrine block (RULED 2026-08-03: confirmed)

All 22 items stay **pub-and-stable**.

- 17 shipped condition types + payload enums, producer-side homes per P1,
  identifiers per the frozen T4-A slate: `CommandResolutionFailed`,
  `UnclosedGroup`, `UnclosedGroupFound`, `StrayGroupClose`,
  `ExpectedExpressionArgument`, `ExpressionCallableRequiresContent`,
  `MissingEnvironmentTerminator`, `MissingTerminatorFound`,
  `EnvironmentTerminatorMismatch`, `MalformedEnvironmentTerminator`,
  `ScopeOpFailed`, `UnusableRecoveryToken`, `UnusableRecoveryTokenKind`,
  `ImplementationError`, `EndOfStreamAfterEscape`, `ForbiddenChar`,
  `CallableDefinedAsError`. Rationale ratified: a demoted condition would
  still raise its frozen wire identifier but be unmatchable by type —
  breaking the F9 typed-matching contract silently.
- 5 diagnostics-defining items → `techy::error`: `DiagnosticData`,
  `DiagnosticValue`, `ToDiagnosticValue` (trait), `DiagnosticInfo` (derive),
  `ToDiagnosticValue` (derive). Rationale ratified: downstream condition
  vocabularies (`flm.*`) are a planned, ruled scenario (P5 first-segment
  rule) and need this surface.

## Round 3 — genuine judgment calls (RULED 2026-08-03, two items pending)

- **Theme A — shipped implementations of public contracts: all six keep
  pub-and-stable** (user: "keep as recommended"): `Scope`, `FallbackProvider`,
  `ErrorCallableSpec` (→ `core::specs`); `StdTokenReader` (→ `techy::core`);
  `NodesParser`, `GroupParser` (→ `core::constructs`).
- **Theme B — `skip_whitespace`: keep pub** (user: "for the stated reason" —
  the paragraph rule is subtle shared semantics deserving one public source of
  truth over N transcriptions). Home `techy::core`.
- **Theme C — `NodeData` → `pub(crate)`** (user: agreed; zero public
  signatures, `NodeRef` is the read API). **`check_tree_invariants` →
  `pub(crate)`**, with the user-ruled implementation shape: ONE canonical
  check implementation = the public `validate_tree` (T5-F); Phase 3 refactors
  invariants.rs so `check_tree_invariants` becomes a `pub(crate)` panic-assert
  wrapper over `validate_tree`'s `Result` (no duplicated invariant logic; the
  error must carry violation detail so panic messages stay informative).
  Demotion rides the same Phase-3 commit that adds `validate_tree` (no
  public-checker gap).
- **Theme D — `VERSION`: keep pub** as the compile-time
  `pub const VERSION: &str = env!("CARGO_PKG_VERSION")` at the crate root
  (ecosystem-standard idiom; getter and structured-components forms rejected —
  consumers wanting structure parse with the `semver` crate). Phase-3 rustdoc
  sentence: Cargo package version, always valid semver.

## Round 4 — placement flags (RULED 2026-08-03)

- **4a — `FrameRole` → `techy::core` (hub)**, beside `Frame`/`FrameTitle`
  (USER OVERRULE of the brief's `core::specs` recommendation). Rationale
  (user): the frame vocabulary is engine-wide, not callable-only — groups
  also create frames. VERIFIED in code: `group_parser.rs:159`,
  `environment_parser.rs:394`, `invocation_parser.rs:117` all mint `Frame`s;
  `FrameTitle` has `Static`/`Quoted`/`Callable` variants. The one
  cross-boundary signature reference now runs specs→hub
  (`CallableSpec::stack_frame_title(role, name)` names it) — within the P1
  allowance.
- **4b — `ParsedArgumentNodes` → `core::constructs`** with its trait
  (user: "definitely not specs"). **User consistency rule, verified**:
  parsed-residue types split by role — parser-CONTRACT residue (returned by
  `ArgumentParser` during parsing) lives in `core::constructs`
  (`ParsedArgumentNodes`, spec/structure.rs:47, the only such type); STORED
  built-node containers (BuildId-designated, resolved in place) live in
  `core::node` (`ChildRegion`, `ParsedArgument`, `ParsedArguments`,
  `ParsedSlot`, `ParsedSlots` — node/arguments.rs:103–356 — plus
  `ContentNodes`, node/arguments.rs:77). Verified consistent; no further
  moves needed.

## Round 5 — riders (all RULED 2026-08-03)

- **R1 — `NoResolver`: REMOVE** (user: original use is gone). Phase 3
  deletes the type entirely: after the T4 resolver-move application nothing
  internal references it (a `pub(crate)` residue would be dead code).
  Superseded-names register gets the entry at application.
- **R2 — `ProvenanceChain` + `ResolvedContent`: keep pub, home
  `techy::source`** (user).
- **R3 — free `resolve_source`: keep, RENAMED `resolve_source_reference`**
  (user; names the input by the ruled "source reference" vocabulary —
  family: `attach_source_reference`, `UnresolvableSourceReference`; the
  resolver parameter carries the delegation). Old name → superseded-names
  at application; [§dd-dr:input-wiring] amendment note.
- **R4 — REOPENED and RE-RULED (user): T3 wish-18b superseded.**
  `ScopesResolvingDriver` (the two-component shape) is replaced by a
  strategy parameter on the one canned driver: `trait CommandResolver<L>`
  (the pluggable body of `ParseDriver::resolve_command` — the only hook
  that gets a strategy seam: sole non-defaultable hook with >1 canned
  behavior); **`StdParseDriver<R = ()>`** with `impl CommandResolver for ()`
  = resolves nothing (user chose `()` over `NoCommandResolver`; the current
  helpful "not implemented" detail message moves into `()`'s impl);
  `ScopesCommandResolver { command_type: L::CallableTypeId }` = one-line
  delegation to `resolve_command_in_scopes`. Names ratified. Homes ratified:
  trait + `()` impl in the hub beside `ParseDriver`/`StdParseDriver`;
  `ScopesCommandResolver` in `core::specs` beside the resolution family.
  Bookkeeping ratified: supersede [§dd-dr:scopes-resolving-driver];
  `ScopesResolvingDriver` → superseded-names; T3-A+F + doc-sentence clauses
  amended; `LatexlikeDriver` untouched. Recorded guard: no strategy-trait
  proliferation. Asymmetry RULED (user): source resolver stays value-level
  dyn (`Option<Arc<dyn …>>`) per the storage-matches-consumption-seam
  analysis; command resolver generic; **the asymmetry rationale must be
  documented in rustdoc and in code** (Phase-3 checklist). Constructor
  RULED (user): `StdParseDriver::new(recovery, command_resolver)` — command
  resolver is a mandatory constructor argument (no `Default`/`Clone`
  bounds); source resolver initialized `None`, set via the chainable
  **`.with_source_resolver(…)`** builder (renames T4-B1's ruled
  `with_resolver` builder — disambiguation now that both resolvers coexist
  on one struct; supersede the old builder name). Arc-free arguments
  everywhere (user): sealed-conversion idiom on `with_source_resolver`
  (by-value resolver → internal Arc; pre-made `Arc<R>`/`Arc<dyn>` pass
  through, no double-wrap); the command resolver is by-value generic — no
  Arc exists to hide. Test spelling: `StdParseDriver::new(Recovery::Strict,
  ())`.
- **R5 — `Diagnostics::into_vec`: do not add** (user).

## Closing sweep (2026-08-03) — session complete

Completeness: all 76 rows of TIERC_BRIEF §4 dispositioned — 12 already-ruled
(confirmed, not re-litigated) + Round 1 (29 forced/committed keeps) + Round 2
(22 doctrine-bound keeps) + Round 3 (10 judgment calls: 8 keep, 2 pub(crate)) +
Round 5 rider items (3: one removal, two keeps); Round 4 ruled placements for
two items already kept in Round 1. **Outcome tally: 73 keep pub-and-stable ·
2 pub(crate) (`NodeData`, `check_tree_invariants`) · 1 removed (`NoResolver`)**
(+ the R5 rejection of a never-added method).

Durable records written this session:
- DESIGN_RATIONALE: new entries **[§dd-dr:public-visibility-sweep]** and
  **[§dd-dr:command-resolver]**; [§dd-dr:scopes-resolving-driver] marked
  SUPERSEDED; amendment notes on [§dd-dr:input-wiring] (rename
  `resolve_source_reference`, builder `with_source_resolver`),
  [§dd-dr:source-resolver] (`NoResolver` removed), [§dd-dr:tree-validation]
  (wrapper demotion), [§dd-dr:resolution-extraction] (`ScopesCommandResolver`
  beside the family), [§dd-dr:stability-rubric] (completion note);
  superseded-names Tier-C block.
- ARCHITECTURE: [§dd-arch:arch] footer gains [§dd-dr:public-visibility-sweep]; engine
  footer swaps in [§dd-dr:command-resolver] (old label still referenced).
- PLAN.md: Phase 2b marked COMPLETE; decision-log entry; Phase-3 checklist
  additions consolidated under the NEXT bullet.

This file and TIERC_BRIEF.md are process files (deleted when the review
completes); the DESIGN_RATIONALE entries above are the durable record.
