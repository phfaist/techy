# Message build: handover

## State

- Branch: `claude/pensive-mayer-dqtd5i` (not pushed by the build agents).
- Start commit: `54dc138`. Last work commit: `9e07008`; this handover is committed on top.
- 67 work commits. Every unit of both work packages finished with status "done"; no unit
  failed. Both final sweeps report nothing left.

## Commits per work package and unit

Tests work package (D5: completed before the messages work package):
T1 `16a7a59` · T2 `5b8909d` · T3 `c4f8ac4` · T4 `2da008b` `616e225` · T5 `bfc483d` `1c59a89`
· T6 `98d3d7d` · T7 `463c8c8` · T8 `8d9b7c4` · T9 `ebf4f42` `e606d67` · TX1 `9ff8a0d`.

Messages work package:
M1 `82f7537` `711e93c` `bcd7b17` `81ed260` `b64cbc5` · M2 `562db37` `7419483` `39f92c8`
`0aacc43` `3ff12a1` `0326fc6` · M3 `f5a1fbc` `ad4ccd8` `c685439` · M4 `63baa04` `421cbbc`
`8ce958c` · M5 `db5634b` `c85a228` `4886058` `ae0ffad` `89a7cdc` `37a41d8` · M6 `a4b4849`
`1649674` `ac9b95f` `8c5cea5` · M7 `e900176` `1ef0a59` `aaf2087` · M8 `4cf6d98` `13739ee`
`729db81` `b15b1e5` · M9 `da7183a` `9ec54ef` `6de0df5` `54bf4ba` `c85ae56` · M10 `c071e7a`
`808aed8` · M11 `db1acd4` `6c02065` `54faf46` (test fix) · M12 `a3afbbb` `7b7a887` · MX1
`59c745f` `78b67e3` · MX2 `d8d7615`.

Consistency fix: `64f1860` `030f9c8` `91479f8` (`030f9c8` alone leaves `BuildId::number`
unused, a dead-code warning; `91479f8` uses it). Records: `7ddd75e` `9e07008`.

## What was done

- T1: diagnostics report/traceback tests check structure and numbers, not layout words (`error.rs`, `parsing_state.rs`).
- T2: serialization goldens mask `message` (D2); serde-path errors compared with the expected library error's Display.
- T3: tree-driver, engine and serialize tests check variants and data, not wording.
- T4: nodes-parser, span-tiling, content-reader, group and chars-group tests check conditions, data and frame spans/titles from the spec.
- T5: environment tests (core and preset) check conditions; frames told apart by spec titles.
- T6: argument, terminated, verbatim, tack-on, attached-source and `constructs/mod.rs` tests downcast conditions.
- T7: descent-guard, language, engine, token reader/scan/token tests check guard state, conditions and data.
- T8: preset arguments/input/invocation-syntax/minidefs/mod/spec, scopes and resolver tests.
- T9: invariants, node, recompose, transform, acceptance, guide doctests (D3) and derive rejection tests (D4).
- TX1: visit.rs walk refusals compared with the guard's own detail.
- M1: token conditions, token-layer details, `TokenKind` Display as message fragments, control-character escaping.
- M2: unclosed-group, missing-argument, terminated, tack-on, verbatim conditions and parser details; worked example regenerated.
- M3: environment terminator conditions, preset environment conditions, `BeginSpec` detail.
- M4: nodes/root parser conditions and details, command-resolution details, `SearchedProviders`, did-you-mean hint.
- M5: construct framework conditions, scope-op/provider errors, descent-guard details, `NodeBuildError`.
- M6: escape-shadowing warning, error-callable error, inclusion errors, `ResolveError`, `NoReferenceArgumentError`.
- M7: `HookFailed`, diagnostics-count error, render suppression line, position fallback, `DeriveError`, `FinalizeError`.
- M8: `TreeViolation(Kind)`, `NamedAccessError`, `RestageError`, `WalkError`, `RecomposeError`, `ExtractError`.
- M9: `ArgumentCodeError`, `EnvironmentSyntaxError`, `SourceRecomposeError`, preset `ExitMathContext` refusal.
- M10: all of `serialize/error.rs` (`SerializeError`, `DeserializeError`, `RegistrationError`, `SerialValueError`).
- M11: serialization driver, wire, bridge, render and base64 texts; `DeserializedCondition` rustdoc.
- M12: compile errors of the derives, the "Compile errors" rustdoc.
- MX1: `CommandResolutionFailed` detail is the `ScopeStackError` Display.
- MX2: dropped the doubled "Invalid node tree:" prefix; aligned builder fragments.
- Consistency: 13 should-fix findings applied across units (wording alignment, node numbers in builder errors).
- Records: `[§dd-dr:message-wording]` with its ARCHITECTURE reference, "Diagnostic messages" in `Documentation_Structure.md`, dev-docs quotations updated.

## Provisional choices taken, with reasons

- `ImplementationError` message is `Implementation error: {detail}` (M5): the details already name the faulty component. Alternative: `Bug in the language implementation: {detail}`.
- `TokenKind` Display uses fragments (`command ‘\foo’`, `specials ‘~’`, `end of the input`) (M1, `bcd7b17`): it embeds into messages and matches the D12 frame terms. See open point 1.
- Golden placeholder `<message not recorded in golden files>` (T2); one test checks serialized `message` == condition Display.
- serde-path tests assert that `serde_json`'s error text contains the Display of the expected library error (T2): library-vs-library, relies on serde passing custom text through.
- Resolver details of `UnresolvableCommand` are lowercase continuations after a colon; `CommandResolutionFailed` embeds a capitalized `ScopeStackError` Display (M4, MX1, D10).
- Serialization location wrappers keep the `Failed to …: <cause>` prefix chain (M10, option a).
- `TreeViolationKind` Display is a lowercase fragment; `TreeViolation` composes `Invalid tree at node N: …` (M8).
- `HookFailed` reads `Hook failed: {detail}` (no hook-name field) (M7).
- `ExpectedExpressionArgument`: `Missing argument ‘x’ (expected a single character, group or command)` (M2).
- `NoSourceResolver`: `Unsupported inclusion of ‘r’ (missing source resolver)`; `CallableDefinedAsError`: `‘x’ may not be used` (M6).
- `OrphanEnd`: only the capital letter changed (D12).
- Derive compile errors follow rule (VII) (capital, ‘…’), not rustc style (M12).
- Node numbers: `node #N` for wire positions, `node N` for in-memory ids, left as is (consistency fix).
- `UnclosedGroup` StrayClose says "mismatched" (the condition's own term); siblings say "unmatched".

## Checks

Passed: `cargo test --workspace` (with and without `--all-features`), `cargo clippy
--workspace --all-targets -- -D warnings` (with and without `--all-features`), `cargo docs
--all-features` after `rm -rf target/doc`, `cargo check -p techy --target
wasm32-unknown-unknown` (with and without `--all-features`), the rustdoc `-D warnings`
builds of `techy` and `techy-derive`. No fixer was needed.

Preexisting failure: `cargo +1.86.0 check --workspace --all-features --all-targets` fails at
dependency resolution (`proptest@1.12.0` in `Cargo.lock` requires rustc 1.88), as in the
baseline. Not run: `sh scripts/check_semver.sh` (no `api-baseline`, no
`cargo-semver-checks` in the container; Philippe runs it locally).

## Open points for Philippe

1. **`TokenKind` Display notation (recorded decision).** Context: the DESIGN_RATIONALE entry
   on `TokenError`'s boxed recovery recorded `Command(\foo)`; `bcd7b17` changed it to
   `command ‘\foo’` etc., and the records step updated that quotation. Options: (a) keep and
   ratify; (b) revert `bcd7b17` and the quotation. Recommendation: (a); also decide whether
   the opening-delimiter fragment keeps `(group type {:?})`.
2. **Follow-up build for structured fields (D4).** Context: 33 tests lost a case distinction
   (below). Options: one follow-up build adding the grouped fields; pick a subset; leave as
   is. Recommendation: a follow-up build with `ImplementationErrorKind`, `DescentLimit` and
   `ResolveErrorKind` first (they cover most cases), names checked against [§dd-arch:naming].
3. **MSRV check blocked.** Context: `Cargo.lock` pins `proptest@1.12.0` (rustc 1.88), so the
   1.86 compatibility of this build is unverified. Options: downgrade proptest in the lock;
   raise the MSRV; check locally with an adjusted lock. Recommendation: pin a proptest that
   supports 1.86 (`cargo update -p proptest --precise …`) and rerun the MSRV check.

Other points:
- `ImplementationError` prefix (see provisional choices); NodeBuildError texts lifted through it do not name the faulty code.
- `OrphanEnd` wording: replace "Orphan" by "Unmatched", keeping "here" (D12, reviewer: "here" is needed for `\end` inside a group).
- Unknown-command message shows provider names to authors; options: keep, or drop the list and move providers to structured fields (M4).
- `UnresolvableCommand` composed from three templates (rule VIII); a structured detail would allow one template (M4).
- Recovery placeholder `Command` token reported as `UnresolvableCommand` with no detail; add `UnusableRecoveryTokenKind::Command`? (M4).
- `()` resolver: a separate program-facing condition reported once, instead of one `UnresolvableCommand` per command? (M4).
- Case convention of the two resolver details (`Unresolved` lowercase vs `Failed` capitalized) — one convention? (consistency).
- `UnusableRecoveryToken` message form `Misconfigured token reader: …` was applied; confirm (M4).
- `CONFIGURE_HINT` lists all five run types in every default-budget message; specific hint needs a design change (M5).
- Interface terms ("stack budget") in author-reachable default-budget messages: acceptable? (M5).
- `DeriveError` nests three levels when only finalize fails; flatten? (M7).
- `ExtMintFailed` rename ("mint") (M5); `NodeBuildError` doc should name `TooManyNodes` as input-reachable (M8).
- `RestageError::ContentParentDropped` `Some(1)` mapping appears unreachable: keep or drop (M8).
- `TreeViolationKind` Display alone is a lowercase fragment for programs that print it (M8).
- Callable names without escape character (`‘gone’`, `(callable ‘foo’)`, `name ‘x’`) — needs a field (M6, M10, consistency).
- One numbering form for nodes and arguments (`node #N`/`node N`; `argument index N` 0-based vs `argument #N` 1-based).
- 1-based argument numbering and D12 preset frame terms are no longer guarded by any test (T3, T8): exception to R2, or rely on the record?
- Derive compile errors: confirm rule (VII) style; rename `*_NO_GENERICS_REASON`/`reason` to `hint` and take `Option<&str>` (M12).
- `IncoherentInvocationSyntax`: role enum, field renames `callable_role`/`recorded_role`, split into two variants (M9).
- `TableFull` off-by-one between write (`u32::MAX` entries) and `push_segment` (`u32::MAX - 1`) (M10).
- `check_include_chain` depth error stores the includer as `reference`, so `ResolveError` Display names the wrong source (M6).
- Frame title "attached source" → "included source"? (M6).
- `DeserializedCondition` rustdoc uses an ordinary-type opening; confirm (M11).
- `MissingTerminatorFound::UnreadableToken` Display arm unreachable for the terminated parser; keep or split (M2).
- Shared helper for the nodes-parser contract details (M2).
- Unapplied review nits: T4 (group-frame render checks; compare `UnresolvableCommand.detail` with the default resolver output); T9 (`learn-by-example.md` "message and position" comment); records (partial rule list in the entry, backticked label in `Documentation_Structure.md`, overlong line near DESIGN_RATIONALE "forgot-the-hook"); consistency nits (provider naming "definition provider", "Input tree nested too deeply" vs "Tree nested too deeply", inclusion depth wording, read-side tree-shape phrases, `a map (‘…’)` vs `a map (struct ‘…’)` and "(a unit variant has no data)", `UnusableRecoveryToken` kind terms, derive placeholder/empty-name wording, "Source not found").

## Proposals of the documenters

- Fields for quoting what the author wrote (rule V): found closing delimiter in `UnclosedGroup`/`MissingArgumentTerminator` (M2); terminator source text in `EnvironmentTerminatorMismatch`/`MalformedEnvironmentTerminator` (M3); invocation spelling in `CallableDefinedAsError`, invoked command in `InvalidSourceReferenceArgument` (M6); code string in `ArgumentCodeError` (M9); affected callable in `ExpressionCallableRequiresContent` (M4); hook name in `HookFailed` (M7).
- `type_name` fields for `SerializeError::Unsupported` and `RegistrationError::DuplicateAnnotationType`; named callable type for `ProviderDropped`/`MissingDefinition` (M10).
- `expected` field for `ExtractError::UnexpectedContent`; callable names in restage/recompose/extract errors (M8).
- Structured op kind and target on `ScopeOpFailed`; dedicated variant for child-offset overflow (M5).
- Rename `SearchedProviders` (e.g. `ResolutionMissDetail`) (M4); `EndOfStreamAfterEscape` name says "stream" (M1, no rename: wire id).
- Structured `TypeMismatch.expected`; split `DeserializeError::Failed` into malformed-input and misconfiguration variants (M11).
- `BuildId` fallback in `builder_failure` may be unreachable (consistency fix).

## Contradictions between the old documentation and the code

- `MissingMandatoryArgument` was documented as raised only at end of input/paragraph break/close; the code raises it whenever no accepted group opens (M2).
- `UnusableRecoveryToken`: "other kinds are processed as ordinary content" was false for `Command` and `GroupClose` placeholders (M4).
- `NodeBuildError::RegionOutOfBounds`/`ContentOutOfBounds` also cover inverted ranges; `ReplacementsLengthMismatch` also from `restage_staged_node`; `TooManyNodes` also for child-offset overflow (M5).
- `ArgumentCodeError::TrailingCode`/`EmptyCode` also from `argument_specs_named` (M9).
- `format_traceback` example listed frames outermost first and without "macro"; `format_position` fallback said "char pos" for a byte offset (M7).
- `MissingProvenance` implied `Package::new_shared` stamps definitions; it only issues stamps (M10).
- `RestageError::ContentParentDropped.replaced_by` doc omitted `Some(1)`; `ExtractError::Build` claimed every non-mint variant is a bug although `TooManyNodes` is a size limit (M8).

## Tests that lost a case distinction (D4)

33 cases, grouped by proposed field:
- `ImplementationErrorKind` (`#[non_exhaustive]`: NoProgress, PeekPositionMismatch, SpanTilingBroken, IncoherentPositions, InvalidNodeSpan, parser misconfiguration, Other; optionally `component`): `nodes_parser.rs` (2 tests), `span_tiling_tests.rs` (4), `terminated_parser.rs` misconfigurations, `constructs/mod.rs` (2), `invocation_syntax.rs` stage_invocation (T4, T6, T8).
- `DescentLimit` (`DefaultStackBudget{bytes}`/`StackBudget{bytes}`/`Depth{levels}`) on `DescentRefusal`, `DescentWarning`, `DescentLimitExceeded/Approaching`, Walk/Restage/RecomposeError: `language.rs` (3), `descent_guard.rs`, recompose/transform/visit warning tests (T7, T9, TX1).
- `ResolveErrorKind` (`IncludeCycle`, `IncludeDepthExceeded{depth,max_depth}`, Other): `resolver.rs` depth test, `input.rs` cycle test (T8).
- Structured hint on `UnresolvableCommand` (searched providers, escape-registered provider, suggestions; optional reason NotImplemented/Searched/Custom): `latexlike/mod.rs` did-you-mean tests, `nodes_parser.rs` default-detail tests (T4, T8).
- `InvalidSpecialsScanPosition{pos, content_len, reason}`: `scopes/mod.rs` scan_specials test (T8).
- Deserialization variants: `AnnotationMismatch{stored, requested}`, `DuplicateDefinition{scope, callable_type, name}`, `SpecIdentityOutsidePackage{provider}`, `SerialValueError::ReservedObjectExtraEntry{key}` (T2).
- Tree driver: `UnclaimedNode`, wire position on builder causes, `ContentParentInvalid{node, reason}`, `ExtOnAbsentArgument{index}`, `AnnotationCountMismatch{nodes, annotations}`, `SerializeError::UnregisteredTreeType`, `SpannedTextInPayload{span}` (T3).
- Derive rejection reasons: optional test-only internal enum; no public change (T9).
- `FinalizeError` event: no loss today (one `Event` variant) (T8).

## Remaining work and failed units

None. Both final sweeps found nothing left; no unit failed or returned "partial".

## Quotations left in `dev-docs/`

- DESIGN_RATIONALE.md, bare "cannot resolve" (~line 4270): the motivating problem, kept as rationale.
- OrphanEnd fragment (~8312): accurate, omits the "Orphan ‘…’:" prefix.
- `Inside:` (ARCHITECTURE.md ~1142, DESIGN_RATIONALE.md ~6869/6899): fixed by D12.
- "… and N more" and the preset frame titles (~8340): accurate.
- PLAN.md R1 still shows the stale example `error: Unclosed group …` (plan only, not a record).

## Leftover changes that agents stashed

`stash@{0}` "msgbuild leftover" (on `2da008b`): a partial rewrite of the tests of
`techy/src/constructs/environment_parser.rs` (helpers `conditions_of`,
`unresolvable_command_names`, `missing_terminators`), left by a stopped earlier T5 run and
superseded by `bfc483d`/`1c59a89`. No unit reported stashing it. It can be dropped after a
look (`git stash show -p stash@{0}`).

## `// ###` notes found

None (all units, and `git grep '// ###'` at HEAD).

## Before-and-after samples

Tests work package:
- T1 `diagnostics_limit_counts_suppressed_pushes`: before, pinned the footer text; after,
  5 pushes under limit 2, `render_all` is the two reports plus one closing line whose
  numbers are `[3, 2]`.
- T5 `mismatched_terminator_unwinds_without_consuming`: before, counted messages containing
  ‘B’; after, `conditions::<EnvironmentTerminatorMismatch>() == [{expected: B, found: A}]`.

Messages work package:
- M4: `cannot resolve command ‘\gret’ (searched providers: user, base; did you mean ‘greet’
  (provider ‘user’)?)` → `Unknown command ‘\gret’: no definition in providers ‘user’,
  ‘base’; did you mean ‘\greet’ (provider ‘user’)?`
- M6: `every ‘Macro’ definition of provider ‘p’ begins with an escape character followed by
  more characters (‘\’ — 3 definitions, e.g. ‘\greet’): …` → `Misconfigured provider ‘p’:
  all 3 ‘Macro’ definitions begin with the escape character ‘\’ (e.g. ‘\greet’); names
  must be registered without their escape character`

## How to continue

1. Review this handover and the open points; decide 1 to 3 first.
2. Run `sh scripts/check_semver.sh` locally against `api-baseline` before merging.
3. Resolve the MSRV lock question and rerun `cargo +1.86.0 check --workspace --all-features
   --all-targets`.
4. Look at and drop `stash@{0}`.
5. Push the branch; after the merge, move the open points and the D4 list to "Specific
   things - from Claude [CLAUDE]" in `TODO_Big.md` and close the folder (D9).
