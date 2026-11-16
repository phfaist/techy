# The message follow-up build: plan for the supervisor session

This plan specifies one build of techy, run in a cloud container by a supervisor session
through one workflow (`dev-docs/msgbuild-followup/build.js`). The build follows the
message build, which rewrote every diagnostic and error message and removed every test
assertion that compared message wording. It does three things, each only as far as
Philippe has decided it:

- it adds structured fields and variants that identify the source of an error, where a
  test could tell two cases apart only through message text;
- it fixes the suspected bugs that the documenters of the message build reported;
- it closes the gaps in test coverage that the agents of the message build reported,
  where a test passes Philippe's bar.

The folder `dev-docs/msgbuild-followup/` holds this plan, the workflow script and, at the
end, the handover. It needs no other plan. It is deleted once the build is merged
(section 7).

Read this plan in full before launching the workflow. Every build agent reads sections 2
and 5 before its first command.

## 1. The request, in Philippe's words

Philippe's request after the message build, verbatim:

> * TokenKind display is fine.
> * Pin a propest for 1.86 in the simplest way possible that solves this problem without any ugly workaround.
>
> Start a fable agent with the following tasks:
>
> * (1) study which fields are worth adding to diagnostic tests to identify error/diagnostic sources clearly from structure, and that could be useful beyond tests (D4);
> * (2) study the possible bugs identified by the documenters and recommend specific fixes;
> * (3) study whether it is worth following the documenter's suggestion to add a test;
> * (4) Create a workflow script that executes these recommendations with opus agents.

The first two bullets are handled outside this build. The `TokenKind` notation is
ratified. The supervisor holds proptest at a series that supports Rust 1.86
(`proptest = "~1.11"` in `techy/Cargo.toml`, commit `873ab99`; `Cargo.lock` is not
tracked). Tasks (1) to (3) are the decisions of section 2. Task (4) is `build.js`.

Philippe's ruling on task (3), given while this plan was written, verbatim:

> Yes, consider the potential gaps in test coverage identified by the documenters. But keep a fairly high bar for adding a test, I want to avoid a proliferation of tests that don't have a clear purpose.

Earlier words of Philippe that govern this build:

- "Tests should not pin specific diagnostic or message wordings."
- On the form of a message: "Capital letter first."

## 2. Decisions

**Status.** Each decision below has a line `Status:`. It reads `PROPOSED` until Philippe
rules. The supervisor then rewrites it to `DECIDED, option (x).`, adds Philippe's words if
he gave any, and commits the plan. Nothing else marks a decision as decided.

**Only decided decisions are executed.** The supervisor does not launch the workflow while
a decision that a selected unit depends on is `PROPOSED` (section 4 lists the decisions of
each unit). The preflight agent reads every status and the workflow halts if one is
missing. Each unit's first agent reads the status of its decisions again and returns
"blocked" if one is not `DECIDED`. A decision that no unit depends on does not block the
launch.

A ruling that Philippe gives during the build is added at the end of this section as a
dated amendment, in Philippe's words, and committed (section 7, step 3). The workflow's
prompts do not carry rulings: every agent reads this section.

### Standing rulings (DECIDED)

- **S1. Message rules.** Every message text follows the rules (I) to (VIII) of Part 3 of
  `.claude/agents/documenter.md` and the entry `[§dd-dr:message-wording]` of
  `dev-docs/DESIGN_RATIONALE.md`: one line, a capital letter first, no final period,
  names quoted between ‘ and ’. A fragment inserted into a sentence takes the case of its
  position.
- **S2. Tests never pin wording.** No test compares a text that library code produces
  with a fixed expected text. A test checks the identifier or the type of a condition,
  its fields, span, severity and frames, the number and order of diagnostics, the node
  tree. A comparison of two library outputs with each other pins no wording. A number
  that a text renders may be extracted and compared (decision T1 rules on this).
- **S3. Every agent runs on Opus.** The workflow sets the model of every agent.
- **S4. No design decision by an agent.** An agent that meets a design question (a change
  of behavior or of the public API beyond its decision, a name, a new field, variant,
  dependency or feature) keeps the existing state, finishes what does not depend on the
  answer, and reports the question with context, options and a recommendation.
- **S5. Texts fixed by recorded decisions.** The traceback heading `Inside:`, the terms of
  the preset frame titles ("macro ‘\frac’", "argument #1 of macro ‘\frac’", "specials
  ‘~’", "environment ‘align’") and the content of the `OrphanEnd` message keep their
  wording.
- **S6. A high bar for a new test.** Philippe's ruling of section 1. A test is added only
  if one sentence states its purpose, and that purpose is a behavior that matters for the
  function of the library, that no existing test guards, and that can break without a
  test failing. One test per behavior; no near-duplicates. A check that fits an existing
  test is added there as an assertion, not as a new test. This holds for every unit.

### Fields that identify the source of an error (task 1)

Philippe's criterion: a field is worth adding if it identifies the source of an error
clearly from structure, and is useful beyond tests (to a program that handles errors, to
tools, to serialized diagnostics, to rendering).

Facts that hold for every decision of this group:

- The data keys of a condition may grow, never change
  (`[§dd-dr:wire-identifier-stability]`).
- `#[derive(DiagnosticInfo)]` generates `new()` with one parameter per field
  (`techy-derive/src/diagnostic_info.rs:362`). A new field therefore changes the signature
  of the generated `new()`, unless the condition writes its constructor by hand
  (`no_constructor`).
- The existing field enums are fieldless, `Copy`, `#[non_exhaustive]`, and derive
  `ToDiagnosticValue`, which serializes the variant name in kebab case
  (`UnclosedGroupFound`, `MissingTerminatorFound`, `UnusableRecoveryTokenKind`,
  `InvalidReferenceReason`).
- Naming principle 5 of `[§dd-arch:naming]` reserves `…Kind` for closed enums. The error
  types already use it for `#[non_exhaustive]` enums (`TokenErrorKind`,
  `TreeViolationKind`, `UnusableRecoveryTokenKind`). The names below follow that use. No
  proposed name appears in `[§dd-dr:superseded-names]`.
- The serialization goldens record one diagnostic, `core.groups.unclosed-group`. No
  decision below changes its data, so no golden and no worked example changes.

#### F1. `ImplementationErrorKind` on `ImplementationError`

Status: PROPOSED.

- **Context.** `ImplementationError` (`techy/src/constructs/mod.rs:1722`) has one field,
  `detail: String`. About 45 places in library code construct it, through
  `ParseContext::implementation_error` (`mod.rs:1599`) or `ImplementationError::new`.
- **What the tests lost.** Ten tests can only check that some `ImplementationError`
  occurred: `nodes_parser.rs:3202` and `:6217`; `span_tiling_tests.rs:357`, `:384`, `:453`
  and `:715`; `terminated_parser.rs:1524`; `constructs/mod.rs:2177` and `:2473`;
  `latexlike/invocation_syntax.rs:1113`. The span-tiling tests exist to tell clause 8 of
  the `TokenReader` contract from clause 7. They pass today if the wrong check fires.
- **Judgment.** The kind identifies the source: the requirement that library code checked.
  Beyond tests, it gives logs and serialized diagnostics a stable value to group reports
  of faulty extensions, and the enum lists in one place the requirements that the library
  checks. A program cannot handle one kind differently from another: every kind is a
  fault in extension code. `[§dd-dr:structured-diagnostics]` states that "a wording
  difference that is not worth a field in the payload is not worth existing"; this
  condition is the one whose payload is prose only.
- **Options.** (a) The enum of the table below. (b) A smaller enum: `Other`,
  `MisconfiguredParser`, `InvalidParserResult`, `TokenReaderFault`, `InvalidPosition`,
  `NodeBuildFailed`; it restores five of the ten tests and cannot tell clause 7 from
  clause 8. (c) Do not add; the tests keep checking the type and the span.
- **Recommendation.** (a).
- **Breaking.** No. `ImplementationError` is `#[non_exhaustive]`, and `new(detail)` keeps
  its signature. The data of `core.constructs.implementation-error` gains the key `kind`.

Shape of option (a):

- `pub enum ImplementationErrorKind`, in `techy/src/constructs/mod.rs`, exported from
  `techy::core::constructs`; fieldless, `Copy`, `#[non_exhaustive]`, derives
  `ToDiagnosticValue`.
- `ImplementationError` gains `pub kind: ImplementationErrorKind`, declared after `detail`.
- The constructor is written by hand (`no_constructor`): `new(detail)` sets `Other`;
  `with_kind(self, kind) -> Self` sets the kind.
- `ParseContext::implementation_error(detail, span)` is unchanged and sets `Other`. A
  second method, `implementation_error_with_kind(kind, detail, span)`, sets the kind.
- The message is unchanged.

| Variant | The violated requirement | Places (lines at `c89273e`) |
|---|---|---|
| `Other` | none of this list (the default) | lifted errors of other types: `environment_parser.rs:1290`, `latexlike/environments.rs:981`, `:1093`, `latexlike/input.rs:564`, `mod.rs:1431` |
| `MisconfiguredParser` | a parser or a spec is configured with values that its documentation excludes | `argument_parsers.rs:857`, `:1173`, `:1306`; `embellishments_parser.rs:132`, `:139`; `tack_on_parser.rs:184`; `terminated_parser.rs:497`; `latexlike/environments.rs:905`; `latexlike/input.rs:538` |
| `UnexpectedStateDelta` | a parser that a hook created returns a state delta to a caller that accepts none | `group_parser.rs:335`; `terminated_parser.rs:554`; `environment_parser.rs:957`; `latexlike/environments.rs:1050` |
| `InvalidStopCause` | a nodes parser returns a stop cause that its stop specification excludes | `group_parser.rs:365`; `terminated_parser.rs:588`; `environment_parser.rs:1008`; `root_parser.rs:155`, `:163` |
| `UnexpectedToken` | the token reader returns a token that the features of the language or the parsing state exclude | `nodes_parser.rs:1348`, `:1380`, `:1406`, `:1480`, `:1515`; `root_parser.rs:126`; `verbatim_parser.rs:328` |
| `StalledTokenReader` | the stream position does not advance past a token | `nodes_parser.rs:1216` |
| `InconsistentTokenReader` | a second read of one position gives another answer, or a failed read moves the reader | `environment_parser.rs:802`, `:818`, `:880` |
| `TokenPositionMismatch` | a token does not begin at the position of its peek (clause 7) | `nodes_parser.rs:858`, reported at `:905`, `:931`, `:1304` |
| `IncoherentPositions` | two stream positions do not delimit one range of one source (clause 8) | `mod.rs:348` |
| `InvalidPosition` | a position or a span lies outside the content, off a character boundary, or ends before it starts | `token/reader.rs:892`; `token/scan.rs:561`, `:594`; `scopes/mod.rs:1356`; `mod.rs:921` |
| `NodeBuildFailed` | the node builder rejects staged nodes | `mod.rs:1628`; `engine/language.rs:469`; `latexlike/environments.rs:946` |

A place that fits no variant keeps `Other` and is listed in the unit's result. The unit
adds no variant on its own.

Tests that regain a distinction: `nodes_parser.rs:3202` (`StalledTokenReader`), `:6217`
and `span_tiling_tests.rs:715` (`TokenPositionMismatch`), `span_tiling_tests.rs:357`,
`:384`, `:453` and `mod.rs:2177` (`IncoherentPositions`), `terminated_parser.rs:1524`
(`MisconfiguredParser`), `mod.rs:2473` and `invocation_syntax.rs:1113`
(`InvalidPosition`), `scopes/mod.rs:2838` (`InvalidPosition`).

#### F2. `DescentLimit` on the refusals and warnings of the descent guard

Status: PROPOSED.

- **Context.** `DescentRefusal` and `DescentWarning`
  (`techy/src/engine/descent_guard.rs:113`, `:132`) hold only `detail: String`. The guard
  knows which limit applies (`:389-439`): the built-in default stack budget, a configured
  stack budget, or a depth limit. Seven carriers copy the `detail`: the conditions
  `DescentLimitExceeded` and `DescentLimitApproaching` (`constructs/mod.rs:1781`,
  `:1806`), and the variant `DescentLimitExceeded { detail }` of `WalkError`
  (`visit.rs:354`), `RestageError` (`transform/mod.rs:364`), `RecomposeError`
  (`recompose/mod.rs:569`), `SerializeError` and `DeserializeError`
  (`serialize/error.rs:179`, `:561`).
- **What the tests lost.** Which limit refused, and its value: `engine/language.rs:999`,
  `:1013`, `:1037`; `engine/descent_guard.rs:491`; `recompose/tests.rs:1027`;
  `transform/tests.rs:1186`; `visit.rs:815`. `language.rs:1072` no longer checks the
  computed budget (100 bytes) from start to finish.
- **Judgment.** The limit identifies the source of a refusal. Beyond tests, a program can
  act on it: tell its developer to configure a limit (default budget), report "nested
  deeper than N levels" in its own words (depth limit), or retry on a thread with more
  stack (stack budget). The message uses interface terms ("stack budget") that a document
  author can see; with the field, a program can word its own text.
- **Options.** (a) Add the field to the two guard types and to all seven carriers. (b) Add
  it to the two guard types and the two conditions only; the tests of recompose,
  transform and visit then regain only the warning. (c) Do not add; the guard-level tests
  keep checking the private state of the guard.
- **Recommendation.** (a).
- **Breaking.** Yes, for both (a) and (b). `DescentRefusal` and `DescentWarning` are
  public structs with public fields and are not `#[non_exhaustive]`. The generated `new()`
  of the two conditions gains a parameter. Under (a), five enum variants gain a field.
  The data of the two conditions gains the key `limit`.

Shape:

```rust
/// in techy/src/engine/descent_guard.rs, exported from techy::core
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DescentLimit {
    /// The built-in default stack budget: the program configured no limit.
    DefaultStackBudget { bytes: usize },
    /// A stack budget that the program configured (fixed or computed).
    StackBudget { bytes: usize },
    /// A depth limit that the program configured.
    Depth { levels: usize },
}
```

- Each carrier gains `limit: DescentLimit`, declared after `detail`. `detail` stays.
- A computed budget whose probe answers `None` reports `StackBudget` with the default
  number of bytes: the program did configure a limit.
- `ToDiagnosticValue` is written by hand: a map with `kind` (`default-stack-budget`,
  `stack-budget`, `depth`) and `bytes` or `levels`.
- The name follows the ruling that the guard's vocabulary is "descent"
  (`[§dd-dr:superseded-names]`).

Tests that regain a distinction: all eight named above.

#### F3. `ResolveErrorKind` on `ResolveError`

Status: PROPOSED.

- **Context.** `ResolveError` (`techy/src/source/resolver.rs:358`) has a reference, a
  message and an optional cause. `check_include_chain` (`:241`) returns it for two cases
  that only the message tells apart: a cycle and a depth overflow.
- **What the tests lost.** `resolver.rs:650`, `:660` and `:674` tell the two cases apart by
  their setup and by `assert_ne!` on two messages. `latexlike/input.rs:1116` cannot state
  that the diagnosed refusal is a cycle.
- **Judgment.** The kind identifies the source of a refusal. Beyond tests, a program can
  treat a cycle, a depth overflow and a failed lookup differently, and the serialized
  `UnresolvableSourceReference` carries the kind.
- **Options.** (a) A fieldless enum with `Other`, `IncludeCycle`, `IncludeDepthExceeded`.
  (b) As (a), plus `NotFound`, set by `MapResolver`. (c) A typed cause attached with
  `with_cause`; it needs no change of `ResolveError`, but doubles the text in the cause
  chain. (d) Do not add.
- **Recommendation.** (a). `NotFound` is the likely next variant; the enum is
  `#[non_exhaustive]`, so it can follow when a consumer asks.
- **Breaking.** No. The fields of `ResolveError` are private. The serialized map of a
  `ResolveError` gains the key `kind`.

Shape of option (a):

- `pub enum ResolveErrorKind { Other, IncludeCycle, IncludeDepthExceeded }` in
  `techy/src/source/resolver.rs`, exported from `techy::source`; `Copy`,
  `#[non_exhaustive]`.
- `ResolveError::kind(&self) -> ResolveErrorKind` and `with_kind(self, kind) -> Self`.
  `new` sets `Other`.
- `check_include_chain` sets the two kinds.
- The projection in `techy/src/error.rs:426` adds `kind` after `message`, in kebab case
  (`other`, `include-cycle`, `include-depth-exceeded`). It is written there by hand,
  because the source module may not depend on the error module.

Tests that regain a distinction: the four named above check `kind()`.

#### F4. Structured hint on `UnresolvableCommand`

Status: PROPOSED.

- **Context.** `UnresolvableCommand.detail` (`techy/src/constructs/nodes_parser.rs:88`) is
  composed in `engine/driver.rs:1144-1150` from the searched providers and the
  did-you-mean hint (`:1162`). The channel is `CommandResolution::Unresolved { detail }`
  (`driver.rs:1072`).
- **What the tests lost.** `latexlike/mod.rs:958` and `:1096` cannot tell the
  escape-registration hint from a suggestion, nor read the suggested names.
  `nodes_parser.rs:3447` and `:3503` cannot tell the default resolver's detail from
  another.
- **Judgment.** This is the most useful structure for tools: an editor can offer the
  suggested names as fixes. It is also the largest change. It alters the public channel
  between a resolver and the parser, and it meets four open points of the message build:
  provider names shown to document authors, the message composed from three templates,
  one condition per command under the `()` resolver, and the name `SearchedProviders`.
- **Options.** (a) Defer to a design session on `UnresolvableCommand`; the tests stay as
  they are (T3, row 1). (b) Add now: fields `searched_providers: Vec<String>`,
  `escape_registered_in: Option<String>` and `suggestions: Vec<CommandSuggestion>` on the
  condition, the same data on `CommandResolution::Unresolved`, and the message rendered
  from them. (c) Do not add.
- **Recommendation.** (a).
- **Breaking.** (a) and (c): no. (b): yes (`CommandResolution::Unresolved` gains fields;
  `UnresolvableCommand::new` gains parameters).

No unit of this build executes option (b): it needs its own plan.

#### F5. A condition `InvalidSpecialsScanPosition`

Status: PROPOSED.

- **Context.** `Package::scan_specials` (`techy/src/scopes/mod.rs:1330-1358`) reports a
  position outside the content, or off a character boundary, as an `ImplementationError`.
  The test `scopes/mod.rs:2838` tells the two cases apart by the span.
- **Judgment.** The error is a fault of the caller. No program acts on the difference
  between the two cases, and the position and the content already determine it.
- **Options.** (a) Do not add. (b) A dedicated condition with `pos`, `content_len` and a
  reason.
- **Recommendation.** (a). With F1, the test also checks the kind `InvalidPosition`.
- **Breaking.** (a): no. (b): no, but one more wire identifier.

#### F6. `DeserializeError::AnnotationTypeMismatch`

Status: PROPOSED.

- **Context.** `tree_of_object` (`techy/src/serialize/drivers/tree.rs:614-637`) returns
  `DeserializeError::Failed` with a free text when a tree is read as a tree of another
  annotation type. The reading program can cause this, and so can data whose parse result
  names a tree of another annotation type.
- **What the tests lost.** `serialize/drivers/diagnostic_tests.rs:800` and
  `serialize/drivers/tree_tests.rs:988` cannot read the stored and the requested
  identifier.
- **Judgment.** This is a typed access that failed, like `WrongTable`
  (`serialize/error.rs:534`), which is a variant with data. A reader that supports several
  annotation types can read the stored identifier and choose the right type.
- **Options.** (a) Add the variant. (b) Do not add; the tests accept any `Failed`.
- **Recommendation.** (a).
- **Breaking.** No: a new variant of a `#[non_exhaustive]` enum.

Shape: `AnnotationTypeMismatch { stored: Option<String>, requested: &'static str,
requested_identifier: Option<String> }`. `stored` and `requested_identifier` are
annotation identifiers, `None` when no identifier is registered for the type; `requested`
is the Rust type name. It replaces the `Failed` of `tree.rs:633`.

#### F7. Other variants for deserialization and the tree driver

Status: PROPOSED.

- **Context.** The read side of the tree driver reports about fifteen malformed-data cases
  as `DeserializeError::Failed` with a free text (`serialize/drivers/tree.rs:371-1132`),
  and the specs driver four more (`serialize/drivers/specs.rs:208-580`). The write side
  does the same for a wrong object in the trees table and for span-backed text
  (`tree.rs:265`, `:1112`). The serde reader reports a second entry in a reserved object
  as a custom text (`serialize/render.rs:390`). The proposals were one variant per case:
  `UnclaimedNode`, `ContentParentInvalid`, `ExtOnAbsentArgument`,
  `AnnotationCountMismatch`, `DuplicateDefinition`, `SpecIdentityOutsidePackage`,
  `SerializeError::UnregisteredTreeType`, `SpannedTextInPayload`,
  `SerialValueError::ReservedObjectExtraEntry`, and the wire position on builder causes.
- **Judgment.** These tests check that malformed data is refused, at the right node, and
  without a panic. They still check that: the location wrappers `InNode` and `InEntry`
  are data, and the builder and validation failures keep a typed cause
  (`NodeBuildError`, `TreeViolation`). Which of two checks refuses the data is not
  behavior that a program can use. A program can use one distinction: malformed data
  against a misconfigured reading program. `Failed` mixes the two; the documenter of the
  serialization drivers proposed to split it.
- **Options.** (a) Do not add the variants; put the split of `Failed` on the list for the
  API review of serialization. (b) One variant `MalformedTree` with a
  `#[non_exhaustive]` reason enum of about twelve variants. (c) One variant per case.
- **Recommendation.** (a).
- **Breaking.** (a): no. (b) and (c): no (new variants), but a large public surface.

#### F8. Rejection reasons of the derives

Status: PROPOSED.

- **Context.** The tests of `techy-derive` check that both derives reject a faulty input
  and accept the corrected one. A compile error text is the only output of a proc macro.
- **Options.** (a) Do not add. (b) An internal enum of reasons, used only by tests.
- **Recommendation.** (a): the enum would serve tests only.
- **Breaking.** No.

#### F9. The event of a `FinalizeError`

Status: PROPOSED.

- **Context.** The preset has one event (`ExitMathContext`), so naming the event
  distinguishes nothing today.
- **Options.** (a) Do not add; reconsider when a second event exists. (b) Add a field.
- **Recommendation.** (a).
- **Breaking.** No.

### Suspected bugs (task 2)

#### B1. `TableFull`: the two bounds differ by one

Status: PROPOSED.

- **Evidence.** Confirmed by reading. The write side refuses position `u32::MAX`
  (`techy/src/serialize/engine/session.rs:544-547`), so a table holds at most `u32::MAX`
  entries. `push_segment` requires `end < u32::MAX` (`:748-752`), so it accepts at most
  `u32::MAX - 1` entries. A writer can fill a table that no reader accepts. Two comments
  (`:625`, `:786`) state the read-side bound for both sides. No test covers either check:
  the case needs 2³² − 1 entries.
- **Options.** (a) One bound, `u32::MAX` entries, on both sides: `push_segment` accepts
  `end <= u32::MAX`. The two checks go through one private helper, and a unit test checks
  the helper at the bound. (b) The stricter bound on both sides. (c) Leave as is; the
  texts already state each bound.
- **Recommendation.** (a): it matches `SerializeError::TableFull` and the rule for the
  number of tables (`session.rs:356`).
- **Behavior and API.** Behavior changes only for a segment that brings a table to exactly
  `u32::MAX` entries. No API change. The documentation and the message of
  `DeserializeError::TableFull` (`serialize/error.rs:720`, `:1016`) follow.

#### B2. `check_include_chain`: the depth error names the including source

Status: PROPOSED.

- **Evidence.** Confirmed by a test that was run and removed. For `c.tex` included from
  `b.tex` beyond the depth limit, the error displays as `Failed to resolve source reference
  ‘b.tex’: Inclusion depth limit exceeded (maximum depth 2)`, and `reference()` returns
  `b.tex` (`techy/src/source/resolver.rs:259-268`). The rustdoc states this (`:236-240`),
  and a test asserts it (`:680`). The diagnostic `UnresolvableSourceReference` is correct:
  it prints the reference as written.
- **Cause.** The function does not receive the reference.
- **Options.** (a) Add a first parameter `reference: &str`; both errors store it. (b) Leave
  as is; with F3 a program identifies the case by its kind.
- **Recommendation.** (a). Every resolver has the reference at hand (see the example in
  `docs/specs.md:319`).
- **Behavior and API.** (a) is a breaking change: the signature of a public function
  changes, and `reference()` returns the reference as written in both cases. The tests
  `resolver.rs:650-700` and `latexlike/input.rs:1105`, `:1150`, and the guide example,
  follow.

#### B3. `ContentParentDropped`: an arm maps to `Some(1)` and cannot be reached

Status: PROPOSED.

- **Evidence.** Confirmed by reading. `restage_over`
  (`techy/src/transform/context.rs:877-887`) maps a content parent recorded as `Restaged`
  or `One` to `replaced_by: Some(1)`. The builder returns `ContentParentUnmapped` only when
  the mapping closure returned `None` (`techy/src/node/copy.rs:356-357`), and the closure
  returns `None` only for `Count` or for no record. The value would give the message "was
  replaced by 1 node: a content parent must be restaged to exactly one node". The other
  place that builds this error (`context.rs:826-833`) maps every other case to `None`.
- **Options.** (a) One private helper for both places: `Count(n)` gives `Some(n)`,
  everything else `None`. The `Display` arm for `Some(1)` stays, because the field is
  public. (b) Leave as is.
- **Recommendation.** (a). The change is five lines and removes a state that contradicts
  the message.
- **Behavior and API.** None. Only an unreachable path changes.

#### B4. A placeholder `Command` token is reported as an unknown command

Status: PROPOSED.

- **Evidence.** Confirmed by a test that was run and removed. A custom token reader that
  recovers with a placeholder `Command` token gets `core.specs.unresolvable-command`,
  "Unknown command ‘\c’" (`techy/src/constructs/nodes_parser.rs:1417-1418`). The resolver
  never ran. Placeholder `Specials` and `GroupOpen` tokens are reported as
  `UnusableRecoveryToken` (`:1489-1492`, `:1524-1527`). The standard token reader returns
  only `Char` placeholders.
- **Options.** (a) Add `UnusableRecoveryTokenKind::Command`; `NodesParser` reports a
  placeholder command as `UnusableRecoveryToken`, with the spelling as written, and
  recovers it as characters like the other two kinds. (b) Leave as is.
- **Recommendation.** (a).
- **Behavior and API.** Behavior changes for custom token readers only: another
  identifier is reported. The API change is one variant of a `#[non_exhaustive]` enum and
  one value (`command`) of the serialized `kind`. Not breaking. A placeholder `GroupClose`
  token ends the run as a stray closing delimiter; this build does not change that.

#### B5. `builder_failure`: the fallback to a builder id

Status: PROPOSED.

- **Evidence.** `techy/src/serialize/drivers/tree.rs:931-934`. Every id that the reader
  hands to the builder comes from its own staging map, so the fallback is not reached
  through the reader. It is a harmless text fallback.
- **Options.** (a) Keep. (b) Replace with `unreachable!`.
- **Recommendation.** (a): not a bug. A text fallback is safer than a panic
  (`[§dd-dr:panic-policy]`).

#### B6. `MissingTerminatorFound::UnreadableToken` in `MissingArgumentTerminator`

Status: PROPOSED.

- **Evidence.** The terminated parser constructs only `EndOfInput` and `StrayGroupClose`
  (`techy/src/constructs/terminated_parser.rs:568`, `:581`); only the verbatim body parser
  constructs `UnreadableToken` (`verbatim_parser.rs:931`). The `Display` arm
  (`terminated_parser.rs:113`) is required: the match is exhaustive, and the condition is
  public and can be constructed with any value.
- **Options.** (a) Keep. (b) Split the enum per condition.
- **Recommendation.** (a): not a bug.

Other reports, studied and not bugs: `NodeBuildError::TooManyNodes` for a child offset
that does not fit `u32` (`transform/context.rs:429-435`, `:797`) is the same limit; the
fallback text "a tree of an unregistered annotation type" (`tree.rs:624`) is defensive
like B5; the serde reader refuses `f32` through `visit_f64`. One report concerns wording
only: the root parser blames the token reader for a `GroupClose` under a language without
groups (`root_parser.rs:126`), although a custom nodes parser can cause it.

### Tests (task 3)

Philippe's ruling of section 1 confirms the scope: every gap in test coverage that an
agent of the message build reported. S6 states the bar. Most verdicts below are "do not
add": most reported gaps concern how a text presents data that a test already checks.

#### T1. The 1-based numbering of arguments in frame titles

Status: PROPOSED.

- **Gap.** Reported by the reviewers of two test units. Two places render the index of an
  argument as a number from 1 in a frame title: `techy/src/spec/callable.rs:210` (the
  default title) and `latexlike/spec.rs:44` (the preset titles). The message build removed
  the assertions on these texts. A change from `index + 1` to `index` now passes every
  test.
- **Is the number data?** `FrameRole::Argument { index }` is data
  (`spec/callable.rs:30-39`), and its documentation states "rendered 1-based". A recorded
  `TraceFrame` holds only the rendered title (`error.rs:482`): the number in the title is
  the only way a reader, or a tool that reads serialized frames, learns which argument
  the frame concerns. A test can check it without comparing words: it extracts the
  decimal numbers from the title and compares them with `index + 1`. The message build
  did the same for the closing line of a report.
- **Purpose of the check.** A frame title numbers an argument from 1, as `FrameRole`
  documents.
- **Options.** (a) Two assertions in existing tests, no new test: in
  `latexlike/spec.rs:447`, the number in each argument title is `index + 1` (indices 0
  and 1); in `constructs/argument_parsers.rs:2603`, the innermost frame of the missing
  second argument carries the number 2. (b) No check.
- **Recommendation.** (a). With it, Philippe confirms that a number that a text renders
  counts as data, not as wording.
- **Not included.** Four serialization texts also number arguments from 1
  (`serialize/error.rs:331`, `:888`, `:895`, `serialize/drivers/tree.rs:1035`). No check:
  these errors carry the index as a field, which tests check; the number in the message
  repeats it.
- **Breaking.** No.

#### T2. The preset frame terms

Status: PROPOSED.

- **Gap.** Reported by the reviewer of the preset tests. The words "macro", "environment"
  and "specials" in the preset frame titles are fixed by recorded decisions (S5), and no
  test guards them. No structure carries them: a test can guard them only by comparing
  words.
- **Options.** (a) No test: the recorded decisions and S5 guard the words. (b) An
  exception to S2: one test per recorded text, with a comment that names the decision
  label. (c) Carry the role of a frame as data in `TraceFrame`; this is a design change of
  its own and is not part of this build.
- **Recommendation.** (a). A changed word breaks no program. The test
  `latexlike/spec.rs:447` still checks that the titles of a macro and of a specials
  differ and contain the name as written.
- **Breaking.** No.

#### T3. The other reported gaps

Status: PROPOSED.

Philippe may change a verdict in his ruling.

| # | Reported gap | Verdict and reason |
|---|---|---|
| 1 | `nodes_parser.rs:3447`, `:3503` no longer tell the default resolver's detail from another (review) | do not add: that a resolver's detail reaches the condition is guarded by `latexlike/mod.rs:958`; what the default detail says is wording |
| 2 | The title of a group frame is not checked to quote the opening delimiter (review) | do not add: the tests check the span of the frame, which locates the group; the quoted text is presentation |
| 3 | The place of a frame title in `render()` is not checked (review) | do not add: the report tests of `error.rs` guard the layout |
| 4 | `latexlike/environments.rs:1657` no longer checks that its condition is the first diagnostic (review) | do not add: the order of that diagnostic is not what the test is for |
| 5 | `constructs/attached_source.rs:603` does not count the frames (review) | do not add: the span identifies the frame |
| 6 | `attached_source.rs:709` no longer finds the reference in the message (review) | do not add: the fields and the test at `:743` guard it |
| 7 | `engine/language.rs:1072` no longer checks the computed budget from start to finish (review) | no new test: F2 restores the check in that test |
| 8 | No test checks the role of a frame (review) | do not add: a recorded frame has no role data (T2, option c) |
| 9 | `scopes/mod.rs:2455` no longer checks the rendering of an empty scope stack (review) | do not add: wording |
| 10 | `serialize/drivers/tests.rs:511` and `serialize/tests.rs:281` no longer find the fixture's origin and name in the rendering (review) | do not add: the fields are checked; the rendering repeats them |
| 11 | The method name in `UnexpectedArgumentSpecPayload` and the label of `SpannedContentInvalid` are unchecked (implementer) | do not add: wording; the variant and the span identify the case |
| 12 | `tests/serialize_golden.rs` no longer compares origin and provenance in the read-back (review) | do not add: the golden pins both, and the round-trip tests compare the full rendering |
| 13 | The guide example "Rendering diagnostics" checks neither the identifier nor the position (review) | do not add: a guide example illustrates; the unit tests guard the behavior |
| 14 | `RestageError::ContentParentDropped` with `replaced_by: None` has no test (documenter) | do not add: the refusal runs through the branch that `transform/tests.rs:286` and `:1036` guard, and the builder validates the content parent again; only the value `None` is unguarded, and its test needs a tree built by hand. This is the closest call |
| 15 | A mismatched closing delimiter that passes through several nested groups has no test (documenter) | do not add: `nodes_parser.rs:4449` and `argument_parsers.rs:2360` guard one level and the hand-over to the enclosing level; each further level runs the same code |
| 16 | The specials case of `IncoherentInvocationSyntax` was not run (documenter) | do not add: `latexlike/recompose.rs:499` guards the check |
| 17 | The derive tests no longer check the hint for a generic type (implementer) | do not add: wording (F8) |

Two existing tests get a correction. Neither adds a test.

- `source/resolver.rs:670` compares the messages of two cycle errors. The comparison adds
  no coverage, and it fails if a message ever quotes the reference. Remove it.
- `visit.rs:797` waits for a refusal in an unbounded loop. A defect would make the test
  suite hang. Bound the loop.

- **Options.** (a) Add no test; apply the two corrections. (b) Change nothing.
- **Recommendation.** (a).
- **Breaking.** No.

### Amendments

None.

## 3. Facts

- The message build left 33 cases where a test lost a distinction. F1 covers ten tests,
  F2 eight, F3 four, F6 two.
- B2 and B4 were reproduced with tests that were then removed. B1 and B3 were confirmed
  by reading; B1 cannot be reproduced at its size.
- The line numbers of section 2 are those of commit `c89273e`. Commit `873ab99` changed
  only `techy/Cargo.toml`.
- `cargo +1.86.0 check --workspace --all-features --all-targets` passes at `873ab99`, with
  ten dead-code warnings in test code that predate this build.
- The toolchain 1.86.0 and the target `wasm32-unknown-unknown` are installed in the
  container. The branch `api-baseline` is absent, so the semver check cannot run there.
- The agent type `documenter` is defined in `.claude/agents/documenter.md`, which is
  tracked in git.

## 4. The work

The workflow runs the phases Preflight, Units, Records, Checks and Handover, in this
order.

### 4.1 Units

`build.js` defines the units (`DEFAULT_UNITS`), in this order. Each unit executes its
decisions as ruled.

| Unit | Decisions | Subject | Text step |
|---|---|---|---|
| F1 | F1 | the kind of an implementation error | yes |
| F2 | F2 | the limit of a descent refusal | yes |
| F3 | F3 | the kind of a resolve error | yes |
| B2 | B2 | the reference of include-chain errors | yes |
| F6 | F6 | the annotation type mismatch | yes |
| B1 | B1 | the table-full bound | yes |
| B3 | B3 | the content-parent mapping | no |
| B4 | B4 | the placeholder command | yes |
| TS | T1, T2, T3 | the tests | no |

A unit whose ruling selects an option without work returns "not-needed" and changes
nothing. A ruling that needs work for which no unit exists is written as an amendment that
defines a new unit (key, decisions, the work), and the supervisor passes that unit in
`extra_units` (section 6).

**Steps of a unit.**

1. **Implementer** (holds the write turn). It reads the decisions of the unit. If one is
   not `DECIDED`, it returns "blocked". Otherwise it does the work of section 4.2.
2. **Documenter** (agent type `documenter`, holds the write turn; units with a text step
   only). It writes the rustdoc of the items that the implementer added or changed, and
   the message texts that changed, by its own rules. It changes no behavior and no test.
3. **Reviewer** (read-only). It reads the commits and checks: fidelity to the decision as
   ruled (names, shape, variants, places); correctness; the tests, including S6 (a test
   without a clear purpose, or a near-duplicate, is a finding); the texts against Parts 2
   and 3 of `.claude/agents/documenter.md`; that nothing unrelated changed.
4. **Fixer** (holds the write turn), if the review has a finding of severity "blocking" or
   "should-fix".

**One writer at a time.** Only the agent that holds the write turn edits files, runs cargo
or commits. The reviewer of a unit runs while the next unit's implementer and documenter
work; no other agents run at the same time. At most two agents run at once.

### 4.2 What an implementer does

For a unit that adds a field or a variant:

1. The change itself, exactly as the decision's shape states it. The classification of
   places follows the decision's table.
2. The tests that the decision lists regain their distinction: each checks the new data.
   An existing test of another place that the change classifies may gain one assertion
   on the new data.
3. New tests by S6 only. The restored assertions are the tests of the new data
   (CLAUDE.md, rule 8). A new test is written for a behavior that no existing test
   reaches: for F2, one test of the hand-written serialized form of `DescentLimit`; for
   F3, one assertion on `kind` in the existing test of the serialized `ResolveError`
   (`constructs/attached_source.rs:825`). No test repeats what the derive already
   guarantees for every condition.
4. A first version of the rustdoc of each new item, so that the workspace compiles under
   `missing_docs`. The documenter rewrites it.
5. The guides in `docs/` that show a changed signature or a changed item (for B2:
   `docs/specs.md`). `docs/panics.md` if documented panics change.
6. The serialization goldens: run `cargo test --features serde -p techy --test
   serialize_golden`. Regenerate a golden (`UPDATE_GOLDEN=1`) only if the data of a
   diagnostic that it records changed, and regenerate the worked example of
   `dev-docs/serialize_schema.md` by the recipe in that file in the same case. None is
   expected.
7. The result states each change of the public API, whether it is breaking, each change of
   serialized data, each new test with its purpose in one sentence, and each place whose
   classification was not obvious.

For a bug unit: the fix as ruled, and one guarding test where the decision names one. B1:
one test of the shared bound ("the write side and the read side accept the same number of
entries"). B4: one test with a placeholder `Command` token ("a placeholder command is
reported as unusable and parsed as characters"), next to the tests at
`nodes_parser.rs:2943` and `:2968`. B2: the existing tests follow the new signature. B3:
no test.

For the unit TS:

- T1, option (a): a test helper that extracts the decimal numbers of a text, and the two
  assertions that the decision names. The name used in a title contains no digit.
- T2: nothing under option (a).
- T3, option (a): the two corrections. If an earlier unit already rewrote one of the two
  tests, the correction is adapted to the present code or reported as done.
- No new test function, and no comparison of wording (S2, S6).

### 4.3 Records

One implementer, then one reviewer and one fixer if needed.

1. One entry in `dev-docs/DESIGN_RATIONALE.md` under "Errors and diagnostics", after
   `[§dd-dr:message-wording]`, labeled `[§dd-dr:error-source-fields]`, by the entry
   template (`Status: DECIDED (user-led).`, the decision, `Rejected alternatives:`,
   `Revisit if:`), short, without dates. It states the criterion (a field that identifies
   the source of an error from structure and is useful beyond tests), each field and
   variant that was added with its purpose, and each proposal that Philippe declined with
   its reason. It quotes Philippe's rulings from section 2 exactly. It does not refer to
   this plan or to the build.
2. A reference to the entry in `dev-docs/ARCHITECTURE.md`, in the line that lists the
   decisions behind the diagnostics section, and a mention of each new field where
   ARCHITECTURE describes its type.
3. Statements of other entries that a decided change makes false are corrected in place
   (for instance the signature of `check_include_chain`, the placeholder kinds of
   `UnusableRecoveryToken`).
4. `dev-docs/serialize_schema.md` is checked against the changed serialized data.
5. Checks: the label is unique (grep); the entry is referenced from ARCHITECTURE.

Decisions that are still `PROPOSED` are not recorded; the handover lists them.

### 4.4 Checks

The continuous-integration set, run once by the checks agent, one command at a time:

- `cargo test --workspace` and `cargo test --workspace --all-features`;
- `cargo clippy --workspace --all-targets -- -D warnings` and the same with
  `--all-features`;
- `rm -rf target/doc`, then `cargo docs --all-features` with `RUSTDOCFLAGS` unset;
- `cargo check -p techy --target wasm32-unknown-unknown` and the same with
  `--all-features`;
- `cargo +1.86.0 check --workspace --all-features --all-targets`; it is expected to pass
  (section 3), with the dead-code warnings in test code that predate this build;
- `sh scripts/check_semver.sh`: not run in the container (no branch `api-baseline`);
  listed as not run. Philippe runs it locally. It will report the breaking changes of F2
  and B2 if they are decided.

A failure counts as preexisting when the preflight's baseline shows it. New failures go to
one fixer; the set runs again once.

### 4.5 Handover

`dev-docs/msgbuild-followup/handover.md`, at most about 150 lines, committed. Sections, in
this order: State (branch, start commit, last commit); Decisions and their status; Commits
per unit; What was done (one line per unit); Changes of the public API, with the breaking
ones marked; Changes of serialized data; Checks (results, what did not run, preexisting
failures); Open points for Philippe (design questions of the agents, findings that no
fixer resolved, decisions still `PROPOSED`), each with context, options and a
recommendation; Units that returned "not-needed", "blocked" or "partial"; Leftover changes
that agents stashed (`git stash list`); `// ###` notes found; How to continue.

## 5. Rules for every build agent

These rules add to `CLAUDE.md` (which every agent receives) and, for documenters, to
their agent definition.

- **Checkout and git.** Work in the checkout and on the branch that your prompt names.
  Never create or switch a branch, never merge, never push: the supervisor session pushes.
  Run every git command as `git -C <checkout> ...`. Give absolute paths to tools; never
  write an absolute path of the machine into code, documentation or a commit message.
- **The write turn.** Only an agent whose prompt says that it holds the write turn edits
  files, runs cargo or commits. It starts by checking `git status --porcelain`: changes
  that it did not make (left by a stopped agent) are stashed with
  `git stash push -u -m "msgbuild-followup leftover"` and reported. It ends with a clean
  tree. A read-only agent reads changes with `git show <sha>` and files with
  `git show HEAD:<path>`, since the working tree may be changing.
- **Your turn ends only with your result.** Never start a command in the background, and
  never end your turn to wait for one: ending it kills every command that still runs, and
  the build stops.
- **Scope.** Do the work of your decision and nothing else: no rename, no reformatting,
  no unrelated fix, no change of `Cargo.toml` or `Cargo.lock`. Never remove code that
  seems unused; report it. Report what else you find.
- **Design questions.** S4.
- **Baseline failures.** Your prompt lists the failures that existed before the build.
  Do not fix them and do not count them as yours; report one only if your change touches
  it. This overrides "Fix every failure and every warning" in the documenter's
  instructions for these failures.
- **Cargo.** Unless `env` shows `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=0`,
  prefix every cargo command with both. Use the checkout's own `target/`. Run every cargo
  command in the foreground: one Bash call per command, with the tool's timeout at its
  maximum (600000 ms), all output redirected to a fresh log under
  `target/msgbuild-followup-logs/`, and the exit status recorded
  (`cargo … > <log> 2>&1; echo "exit=$?" > <log>.exit`). Then grep the log
  (`test result:`, `error`, `warning`, `FAILED`); never read a whole log. If the Bash tool
  reports that a command ran past the timeout and was moved to the background, wait for
  it with one foreground call (`until [ -f <log>.exit ]; do sleep 10; done`, maximum
  timeout) and repeat that call until the file exists; do not poll in any other way. One
  cargo command at a time. Before the first build, check the free disk (`df -h`); below
  5 GB, delete `target/debug/incremental`; if still short, stop and return status
  "blocked".
- **Scoped checks.** During the units, every check is scoped to the crates that the step
  touched: `cargo test -p <crate> --all-features`,
  `cargo clippy -p <crate> --all-targets --all-features -- -D warnings`, and, when rustdoc
  changed, `RUSTDOCFLAGS="-D warnings --html-in-header docs/rustdoc-header.html" cargo doc
  -p <crate> --no-deps --all-features`. Only the preflight and the checks phase run
  workspace-wide commands.
- **Rust version.** The workspace's minimum Rust version is 1.86, and the
  continuous-integration workflow builds all targets with it: no `let` chains and no API
  newer than 1.86, in tests too.
- **Panics.** Library code returns an error on every violation of a documented
  requirement; it does not panic (`[§dd-dr:panic-policy]`).
- **Commits.** Commit at regular intervals, each commit compiling and tested, with
  `git -C <checkout> commit -F target/msgbuild-followup-logs/commit-msg.txt`. Subject in
  the imperative; a short body that says what changed and why; no reference to this plan,
  the build, its units or the developer docs; no identity override; end with the
  attribution lines that your harness's system reminder gives.
- **Writing.** US English. No reference to former behavior ("previously", "no longer",
  "used to", "instead of", "now" or "new" in that sense) in code, rustdoc, messages or
  guides. No reference to the developer docs in user-facing text. Do not run `cargo fmt`;
  match the surrounding code's style. Message texts and the rustdoc of conditions and
  error types follow S1 and Parts 2 and 3 of `.claude/agents/documenter.md`.
- **Tests.** S2 and S6. Before you add a test, state its purpose in one sentence; if you
  cannot, or if an existing test guards the behavior, do not add it. Prefer an assertion
  in an existing test to a new test. Never weaken a test silently: list every dropped or
  rewritten test in your result.
- **Context.** Keep your context under about 150,000 tokens: grep, then read the ranges
  you need; never read a generated file or a long test file whole. If your context grows
  large before your work is complete, commit the compiling, tested state and return
  status "partial" with a precise account of what remains.
- **Interrupted runs.** Your prompt gives the build's start commit. The commits after it
  include those of other units. A commit that does exactly your task may come from an
  interrupted earlier run of your task: continue from the current state, and report the
  work of such commits in your result as if you had done it.

## 6. The workflow

`dev-docs/msgbuild-followup/build.js`. Arguments:

```
{ "checkout": "/absolute/path/of/the/checkout",   // required
  "branch": "the container's branch",             // required
  "units": ["F1", "B4"],    // optional: the units to run (default: all)
  "extra_units": [ { "key": "F5", "title": "...", "decisions": ["F5"], "texts": true } ],
                            // optional: units that an amendment of section 2 defines
  "skip": ["records"],      // optional: phases to skip (records, checks)
  "start_commit": "..."     // optional: only for a fresh run after a halt (below)
}
```

Rulings are not arguments: they are the `Status:` lines and the amendments of section 2.

**Preflight.** The first agent confirms a clean checkout on the branch, the presence of
this plan and of the documenter definition, the free disk and the installed tools. It
reports the status and the ruling of every decision of section 2. It runs the baseline:
the commands of section 4.4 that can run. The workflow halts if the checkout is unusable,
or if a decision of a selected unit is not `DECIDED`. Baseline failures do not halt it.
With `start_commit`, the baseline runs at that commit, in a detached worktree.

**Size.** With all units, the workflow starts 30 agents if no review asks for a fix and
the checks pass, and 42 if every review asks for a fix and the checks need a fix round. A
continuation of an implementer, and a reassessment of tests after a documenter, add one
agent each (58 in the extreme case). At most two agents run at once, of which one writes.
The run takes several hours.

**A missing result stops the run.** If an agent returns no result, or the call fails (for
instance at the account's usage limit), the workflow starts no further agent, lets the one
other agent that may be running finish, and ends with an error that names the step.

**Resume after an interruption.** A run that stopped on a missing result, or that was
stopped by hand, or whose container restarted (the journal of a run survives a restart),
is resumed with
`Workflow({scriptPath: "<checkout>/dev-docs/msgbuild-followup/build.js", resumeFromRunId: "<run id>", args: <exactly the same args>})`.
The runtime returns the cached results of the agents that finished, and runs the others.
Before the resume, stash the uncommitted changes of a stopped writer:
`git -C <checkout> stash push -u -m "msgbuild-followup leftover"`. Agents that run again
over work already committed follow the interrupted-run rule of section 5.

**A fresh run after a halt.** A halt is a result, not an interruption: the preflight found
the checkout unusable or a decision pending, or a unit returned "blocked". A resume would
return the same cached result. Remove the cause, then launch a fresh run (no
`resumeFromRunId`), with `units` naming the units that remain and `start_commit` set to
the first run's start commit.

**Return value.** The status (`complete`, `complete-with-open-items`, `halted`), the start
commit, the status of each unit, the handover agent's result, whether the final checks
passed and what did not run.

## 7. The supervisor session

1. **Prerequisites.** Read this plan in full. Present the decisions of section 2 to
   Philippe. Write each answer into the decision's `Status:` line (`DECIDED, option (x).`,
   with his words if any) and commit the plan on the container's branch; the preflight
   halts on uncommitted changes. Choose the units to run: a unit is selected only if all
   its decisions are `DECIDED`. Confirm that `techy/Cargo.toml` holds proptest at a series
   that supports Rust 1.86, that the checkout contains `.claude/agents/documenter.md`,
   that the agent
   type `documenter` is available in this session, and that multi-agent orchestration is
   enabled with a workflow size setting of at least 60 agents.
2. **Launch.** Determine the checkout (`git rev-parse --show-toplevel`) and the branch
   (`git branch --show-current`), then run
   `Workflow({scriptPath: "<checkout>/dev-docs/msgbuild-followup/build.js", args: {"checkout": "<checkout>", "branch": "<branch>", "units": [...]}})`.
   Note the run id and the start commit that the workflow logs.
3. **A ruling during the build.** Stop the run (the task's stop control). Stash leftover
   changes of the stopped writer (section 6). Add the ruling to the end of section 2 as a
   dated amendment in Philippe's words, and commit it. Resume with the same run id and
   exactly the same args. Agents that start after the resume read the amendment; finished
   agents are not rerun. If the ruling invalidates finished work, say so to Philippe and
   agree on a fresh run for the units to redo.
4. **A stopped run.** If the workflow ends with an error that names a step, find the cause
   (usage limit, container restart), wait if needed, stash leftovers, and resume
   (section 6). If it returns `halted`, follow "A fresh run after a halt".
5. **After the run.** Read the handover. Push the branch (`git push -u origin <branch>`;
   on a network error retry up to four times with growing waits). Report to Philippe: what
   was done, the breaking changes, the open points, and the checks that did not run (the
   semver check runs only on his machine).
6. **Merge.** Philippe merges the branch into `main` after running
   `sh scripts/check_semver.sh` locally and moving `api-baseline` if he accepts the
   breaking changes.
7. **Closing the folder.** The session that Philippe tells about the merge moves the
   handover's open points and the decisions still `PROPOSED` to the section "Specific
   things - from Claude [CLAUDE]" of `TODO_Big.md`, deletes `dev-docs/msgbuild-followup/`,
   commits and pushes.
