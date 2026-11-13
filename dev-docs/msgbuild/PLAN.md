# The message build: plan for the supervisor session

This plan specifies one build of techy, run in a cloud container by a supervisor session
through one workflow (`dev-docs/msgbuild/build.js`). The build does two things:

- it reassesses every test that compares the wording of a message, and drops or rewrites
  it so that no test pins wording;
- it rewrites every diagnostic message and error message of the library, with the
  documentation of the conditions and error types that own them, by the rules of the
  documenter agent (`.claude/agents/documenter.md`).

The folder `dev-docs/msgbuild/` holds this plan, the workflow script, the inventory that a
scan produced (`inventory.json`), and, at the end, the handover. It is deleted once the
build is merged (section 7).

Read this plan in full before launching the workflow. Every build agent reads sections 2
and 5 before its first command.

## 1. The request, in Philippe's words

- 2026-11-12: "I want to fix all techy's diagnostic messages, and maybe some doc, too."
- 2026-11-13, on rule (VII) of the message rules: "Capital letter first."
- 2026-11-13: "Tests should not pin specific diagnostic or message wordings. Any existing
  such tests should be critically reassessed (not by the documenter agent, but by another
  worker/implementer agent running on the Opus model) to assess whether they test
  anything meaningful beyond the specific wording: if no, the test is dropped; if yes, the
  test is rewritten to test the specific behavior that is important for the library's
  function and not the specific wordings."
- 2026-11-13: "I'd like one workflow to run in a cloud container with a ultracode build
  script. I'll start a supervisor session which launches the whole build, including
  documenters and implementers to fix tests."

## 2. Decisions

The supervisor session does not launch the workflow while a decision below is marked
PROPOSED: it asks Philippe first. A ruling that Philippe gives during the build is added at
the end of this section as a dated amendment, in Philippe's words, and committed (section
7, step 3). The workflow's prompts do not carry rulings: every agent reads this section,
so an amendment reaches every agent that starts after it.

Rulings:

- **R1. Message rules.** Every message follows the rules (I) to (VIII) of Part 3 of
  `.claude/agents/documenter.md`. Rule (VII) holds as written: a capital letter first,
  although `Diagnostic`'s `Display` prints the severity before the message
  (`error: Unclosed group …`).
- **R2. Tests never pin wording.** Philippe's words in section 1. The documenter agent
  never changes a test or a golden; implementer agents reassess the tests.
- **R3. Every agent runs on Opus** (standing ruling); the workflow sets the model of every
  agent.
- **R4. No design decision by an agent** (CLAUDE.md, "Ask before taking design
  decisions"). An agent that meets a design question (a change of behavior or of the
  public API, a new field or variant, a new dependency or feature, a rename, the wording
  of a text that a recorded decision fixes) does not decide it: it keeps the existing
  state, finishes what does not depend on the answer, and reports the question with its
  context, the options and a recommendation. The handover collects these questions.

Decisions for this build:

- **D1. Tests of the text machinery** (DECIDED). A test whose purpose is the text
  mechanism itself (the layout of a rendered report, the traceback, the "… and N more"
  line, the `{field}` interpolation of the derive) keeps checking text, but the text of a
  fixture condition defined in the test with its message written there. It checks the
  structure of a report (the order of its parts, the positions, the number of lines, the
  fixture's texts) and not the fixed words of the layout, such as `Inside:`. A comparison
  of two library outputs with each other (a round trip, the equality of two renderings)
  pins no wording and stays (section 4.1, step 1).
- **D2. The serialization goldens** (DECIDED). A golden records severity, identifier,
  data, span and frames of a diagnostic, never its message. Method: the golden harness
  (`techy/tests/serialize_golden.rs`) converts each segment with
  `Segment::to_serial_value()` (its maps keep their key order), replaces the value of
  every `message` field of a diagnostic entry with a fixed placeholder, and rebuilds the
  segment with `Segment::from_serial_value`, before writing a golden and before comparing
  with one. Do not mask through `serde_json::Value`: without the `preserve_order` feature
  it sorts the keys, and the goldens would silently stop pinning the key order. The
  read-back checks compare severity, identifier, data, span and frames (the rendered text
  of a read-back diagnostic shows the placeholder). One test checks that a serialized
  `message` equals the `Display` of its condition. The serialized-JSON expectation in
  `serialize/drivers/diagnostic_tests.rs` is handled the same way, or builds its expected
  `message` from the condition's `Display`.
- **D3. The guide doctests** (DECIDED). The doctests of `docs/learn-by-example.md` that
  assert message text are tests: the test implementer rewrites their asserts to check the
  identifier or the data. The documenter of the owning message unit adapts the
  surrounding prose if needed.
- **D4. Free-text cases** (DECIDED). Some tests can tell two cases apart only through
  text: the `detail` of `ImplementationError`, the descent-guard details, `ResolveError`
  (inclusion cycle against depth limit), `FinalizeError` naming the event,
  `DeserializeError::Failed` details, `serde_json` error texts (third-party wording), and
  the compile errors of the derives (a proc macro's error text is its only output). In
  this build such a test checks the identifier, the variant and the structured data that
  exist. A derive rejection test checks that both derives reject the input, and
  optionally that the corrected input is accepted; it does not check the blamed span
  (the tests build their input with `parse_quote!`, and `proc-macro2` lacks the
  `span-locations` feature). The build adds no field, variant, dependency or feature.
  Each case where a meaningful distinction becomes untested is listed in the
  implementer's result ("needs a structured field", with a proposal); the handover
  collects them for a follow-up build.
- **D5. Order** (DECIDED). The tests work package runs to completion before the messages
  work package starts, so that the message rewrite meets no test that pins wording.
- **D6. Scope of "maybe some doc"** (DECIDED). The build rewrites the rustdoc of every
  condition and error type whose texts it rewrites (the type, its fields and its
  variants), and the prose that quotes a rewritten message. It does not rewrite other
  documentation.
- **D7. Records** (DECIDED; requires D8). Section 4.3: one design-rationale entry with
  its ARCHITECTURE reference, a short section "Diagnostic messages" in
  `Documentation_Structure.md`, and the quotations of reworded messages in `dev-docs/`
  brought up to date. The entry and the new section name Part 3 of
  `.claude/agents/documenter.md` as the place of the message rules, which is sound only
  if that file is tracked (D8).
- **D8. The documenter definition is tracked in git** (DECIDED). `.claude` is listed in
  `.gitignore`; `.claude/agents/documenter.md` is added with `git add -f` so that the
  container has it (section 7, step 0).
- **D9. Open points after the merge** (DECIDED). The handover's open points, and the list
  of D4 cases, go to the section "Specific things - from Claude [CLAUDE]" of
  `TODO_Big.md` when the folder is closed (section 7).
- **D10. The case of embedded texts** (DECIDED). A text that is itself a complete
  message (the `Display` of a wrapped error, the `detail` of `HookFailed` or
  `ImplementationError`) follows rule (VII) and begins with a capital letter, as in FLM's
  sample `Misconfigured block-structure hook ‘h’: The hook failed to place a node for
  ‘e’`. A fragment inserted into a sentence (the display of a field enum such as "end of
  input", the expected-value phrases of `serialize/object.rs`) takes the case of its
  position. A frame title is a noun phrase after a position (`1:5: macro ‘\frac’`) and
  stays lowercase.
- **D11. Panic messages** (DECIDED). Panic messages of precondition asserts are not
  messages in the sense of this build: the build does not rewrite them, and a
  `#[should_panic(expected = "…")]` test whose expected text comes from such a panic
  message stays. A `should_panic(expected)` test whose expected text comes from the
  `Display` of an error type that the build rewrites (the `TreeViolationKind` tests of
  `node/invariants.rs`) pins that wording and is reassessed by section 4.1, for instance
  by checking the violation through the non-panicking checker.
- **D12. Texts fixed by recorded decisions** (DECIDED). Three texts are fixed by entries
  of `dev-docs/DESIGN_RATIONALE.md`: the traceback heading `Inside:`
  (`[§dd-dr:diagnostic-outer-frames]`); the terms of the preset frame titles, "macro
  ‘\frac’", "argument #1 of macro ‘\frac’", "specials ‘~’", "environment ‘align’"
  (`[§dd-dr:concrete-spec-types]`, `[§dd-dr:environment-spec-surface]`); and the content
  of the `OrphanEnd` message, which names only the environment that is not open
  (`[§dd-dr:environment-command-names]`). These keep their wording and content; a change is a
  design question (R4).

Amendments:

- **2026-11-13.** Philippe: "Mark D1-D12 decided, commit, push main only." D1 to D12 are
  decided as written above. The branch `api-baseline` stays on Philippe's machine: the
  semver check does not run in the container, and Philippe runs
  `sh scripts/check_semver.sh` locally before merging (section 7, steps 4 and 5).

## 3. Facts from the inventory

A read-only scan of the code (2026-11-13, at commit `022d9ec`) produced
`inventory.json`, with four lists: `cond.conditions`, `texts.items`, `tests.tests` and
`quotes.quotes`. They carry line numbers of that commit: they are hints, and every agent
verifies them.

- **Conditions.** 32 diagnostic conditions outside test code, all in `techy` (31 derived,
  one hand-written: `DeserializedCondition`, which displays the stored message of a
  deserialized diagnostic). 16 messages come from the `message` attribute, 16 from a
  hand-written `Display`. 21 can be caused by a document author, 7 only by a developer
  (`HookFailed`, `ImplementationError`, `ScopeOpFailed`, `CommandResolutionFailed`,
  `UnusableRecoveryToken`, `NoSourceResolver`, `ProviderCommandsShadowedByEscape`), 4 by
  either. Field enums whose display ends up in a message: `MissingTerminatorFound`,
  `UnclosedGroupFound`, `UnusableRecoveryTokenKind`, `InvalidReferenceReason`.
- **Composed messages across files.** `UnresolvableCommand` embeds a detail composed in
  `engine/driver.rs` (the default resolver detail, `did_you_mean_hint`) from the
  `Display` of `SearchedProviders` (`scopes/mod.rs`); `CommandResolutionFailed` embeds
  the `Display` of `ProviderError` through the resolution detail built in
  `engine/driver.rs`; `ScopeOpFailed` embeds the chain `ScopeOpError` → `ScopeStackError`
  → `ProviderError`; `DescentLimitExceeded` and `DescentLimitApproaching`, `WalkError` and
  the descent-limit variants of the serialization errors embed the details of
  `engine/descent_guard.rs`; the mismatch texts of `DeserializeError` embed the
  expected-value phrases of `serialize/object.rs`. `HookFailed` is constructed in
  `constructs/mod.rs` as well as by consumer hooks. Section 4.2 assigns each embedded text
  to one unit.
- **Other texts.** About 175 variant texts of error-type `Display`s (the serialization
  errors account for about 70: `SerializeError`, `DeserializeError`, `RegistrationError`,
  `SerialValueError`), 43 detail strings built at call sites, 15 frame titles, 7 fixed
  texts of rendered reports, 7 compile-error texts of the derives.
- **Tests.** 241 test functions in 47 files compare message text: 148 check a behavior
  through the wording (about 70 which condition fired, about 25 a field value, 12
  `HookFailed` details, about 40 free-text cases, see D4), 34 check only the wording, 50
  test the text machinery (D1; about 13 of them are round-trip comparisons, which stay),
  9 compare only text that the test itself defines. Two goldens
  (`techy/tests/golden/serialize/*.json`) and one serialized-JSON expectation
  (`serialize/drivers/diagnostic_tests.rs`) record a message. Two doctests of
  `docs/learn-by-example.md` assert message text. The scan left out: asserts on frame
  titles alone (`latexlike/spec.rs`), `#[should_panic(expected = …)]` tests (44 in 9
  files; D11), and asserts on the `TokenKind` display (`token/token.rs`,
  `token/reader.rs`).
- **Quotations.** 52 places quote a message in prose: 29 in rustdoc (10 in `error.rs`, 7
  in `techy-derive/src/lib.rs`), 4 in guides, 19 in `dev-docs/` (15 in
  `DESIGN_RATIONALE.md`, 1 in `ARCHITECTURE.md`, 3 in `serialize_schema.md`). In
  `serialize_schema.md`, one quotation is part of the worked example, which an ignored
  test generates (regenerate it, do not edit it); two are a hand-written example of frame
  titles. No AI guide, README or CLAUDE.md quotes a message. Some quotations are already
  stale (for instance the "searched: …" detail of the scopes).
- **Not on GitHub.** The branch `api-baseline` exists only on Philippe's machine, so
  `scripts/check_semver.sh` cannot run in the container, and the semver job of the
  continuous-integration workflow skips itself.

## 4. The work

The workflow runs the phases Preflight, Tests, Tests sweep, Messages, Messages sweep,
Consistency, Records, Checks and Handover, in this order. The work units are defined in
`build.js` (`DEFAULT_TEST_UNITS`, `DEFAULT_MESSAGE_UNITS`), which is authoritative; the
lists below summarize them.

**One writer at a time.** The agents of a phase run concurrently, but only one agent at a
time holds the *write turn*: it alone edits files, runs cargo and commits. Implementers,
documenters, fixers, the records agent, the checks agent and the handover agent take the
write turn in turn. Reviewers and sweep agents are read-only and read commits with
`git show`.

### 4.1 Tests work package

Units: T1 diagnostics core and report rendering (`error.rs`, `parsing_state.rs`,
`tests/derive_conditions.rs`); T2 serialization of diagnostics, goldens and streams; T3
serialization drivers and engine; T4 nodes parser, groups, span tiling; T5 environments;
T6 argument, terminated, verbatim and tack-on parsers, attached sources, construct
framework; T7 engine, descent guard, token layer; T8 preset (including the frame titles
of `latexlike/spec.rs`), scopes, sources; T9 tree tools, acceptance tests, guide
doctests, derive crate.

Per unit: one implementer (with one continuation if it returns "partial"), one reviewer,
and one fixer if the review has a finding of severity "blocking" or "should-fix". After
all units, a sweep agent searches the whole repository for tests that still pin wording;
what it finds runs through one more round of units, and a final sweep reports what
remains.

The procedure for each test:

1. **What pins wording.** A test pins wording when it compares any part of a text that
   library code produces (a message, the `Display` of an error type, a detail string, a
   frame title, a rendered report, a derive compile error) with a fixed expected text:
   with `assert_eq!`, `assert!`, `contains`, `starts_with`, a golden file, a
   serialized-JSON expectation, a snapshot, or `#[should_panic(expected = …)]` (D11). Text
   that the test writes itself (the message of a fixture condition, a fixture detail that
   library code only passes through) is not library wording. A comparison of two outputs
   of the library with each other (a round trip, the equality of two renderings) pins no
   wording: keep it.
2. **What the test is for.** Read the test's name, comments and setup.
3. **Meaningful beyond the wording?** Meaningful are: which condition is reported
   (identifier or type), the values of its fields, its span, severity and frames, the
   number and order of diagnostics, that an error occurs at all, the recovery result in
   the node tree, the structure of a rendered report (D1). The wording itself is not.
4. **If not: drop the test.** Remove helpers that become unused.
5. **If yes: rewrite the test** to check that behavior directly, for instance with
   `Diagnostic::identifier()`, `Diagnostics::with_identifier`,
   `Diagnostics::conditions::<T>()` or a downcast to the condition type, and comparisons
   of fields, spans, severity, frames or nodes. Keep every check of the old test that does
   not concern wording. Keep the test's name if it still describes the test.
6. **Special cases:** D1 (text machinery), D2 (goldens and serialized JSON), D3 (guide
   doctests), D4 (free-text cases), D11 (panics).
7. **Limits.** Change test code only: no library code outside `#[cfg(test)]`, no message
   text, no public API, no `Cargo.toml`. Test-only fixtures and helpers are allowed.
   Never restore a wording assertion.
8. **Checks:** `cargo test -p <crate> --all-features` (or `--test <name>` for one
   integration test file, plus `--doc` for the guide doctests) and
   `cargo clippy -p <crate> --all-targets --all-features -- -D warnings`.
9. **Result:** one entry per test (rewritten, dropped, switched to a fixture, kept
   unchanged) with the reason and what it now checks, and the "needs a structured field"
   list of D4.

### 4.2 Messages work package

Units: M1 token layer; M2 group, argument, terminated, tack-on, verbatim and embellishment
parsers; M3 environments and preset spec frame titles; M4 nodes parser, root parser and
the command-resolution texts; M5 construct framework (implementation errors, scope
operations, descent limits, node building); M6 specs, scopes, attached sources, source
resolution, input; M7 engine, report rendering, diagnostics core, parsing state; M8 tree
tools; M9 preset arguments, invocation syntax, recompose; M10 serialization errors; M11
serialization drivers, engine, wire, objects, rendering; M12 derive crate.

Per unit: one documenter (agent type `documenter`, with one continuation on "partial").
If the documenter lists tests that fail only because they compare wording, a test
implementer reassesses them by section 4.1 within the same write turn, so that the next
writer starts from a passing state; a reviewer checks that reassessment. Then two
read-only reviewers in parallel (one checks the texts against the rules, one checks the
facts against the code), and one documenter fixer if they report a finding of severity
"blocking" or "should-fix". After all units, a sweep, one more round, a final sweep, and
one cross-unit consistency review with one fixer.

The scope of a message unit: every user-facing text in the unit's files (the messages of
conditions, the `Display` texts of error types, detail strings, frame titles, fixed report
texts, derive compile errors), the rustdoc of the conditions and error types defined there
with their fields and variants (D6), and the quotations of these texts in guides and
rustdoc elsewhere.

**Ownership of texts.** A text belongs to the unit whose files contain it, including the
detail strings passed to a condition at a call site (for instance the details of
`ImplementationError` built in the parser files). The exceptions are exactly these, each
stated in the `items` of the units concerned in `build.js`:

- the command-resolution detail and `did_you_mean_hint` (`engine/driver.rs`) and the
  `Display` of `SearchedProviders` (`scopes/mod.rs`) belong to M4;
- the `Display`s of `ScopeOpError`, `ScopeStackError` and `ProviderError`
  (`scopes/mod.rs`) and the descent-guard details (`engine/descent_guard.rs`) belong to
  M5; M4 (`CommandResolutionFailed`), M8 and M10 embed some of them and compose around
  them;
- the expected-value phrases of `serialize/object.rs` belong to M11, and M10's mismatch
  texts embed them (D10).

The owning unit decides the wording by rule (VIII). A unit whose files contain a text
owned by another unit leaves that text alone.

A documenter regenerates the worked example of `dev-docs/serialize_schema.md` when it
changes a message that the example shows, and lists other quotations in `dev-docs/`
without changing them (section 4.3 handles them).

### 4.3 Records

One implementer, then one reviewer and one fixer if needed:

1. A design-rationale entry under "Errors and diagnostics" (`[§dd-dr:errors]`), after
   `[§dd-dr:structured-diagnostics]`, labeled `[§dd-dr:message-wording]`, following the
   entry template (`Status: DECIDED (user-led).`, the decision, `Rejected alternatives:`,
   `Revisit if:`), short, without dates. It states: messages follow the rules (I) to
   (VIII) kept in Part 3 of `.claude/agents/documenter.md`, with a capital letter first
   although the severity is printed before the message (R1), and the case rule of D10;
   tests never pin the wording of a library text (Philippe's words of section 1), with
   the handling of text machinery (D1) and goldens (D2); cases that only a message
   distinguished are tracked as follow-ups (one sentence, no list). Rejected alternatives:
   lowercase messages in the style of rustc (Philippe: "Capital letter first."); wording
   assertions kept as regression guards (message wording is not part of the contract,
   `[§dd-dr:wire-identifier-stability]`). Revisit if: a consumer needs a case
   distinction that only a message shows. No reference to this plan or to the build.
2. A reference to the new entry in `dev-docs/ARCHITECTURE.md`, in the line that lists the
   decisions behind the diagnostics section.
3. A short section "Diagnostic messages" in `Documentation_Structure.md`, after
   "Documentation writing style": output strings and messages obey the same clarity rules
   as all user-facing documentation; diagnostic and error messages follow the rules of
   Part 3 of `.claude/agents/documenter.md`; tests never pin message wording
   (`[§dd-dr:message-wording]`).
4. The quotations of reworded messages anywhere in `dev-docs/` except `archive/` and
   `extra/`, updated to the new wording where they illustrate the present messages;
   quotations inside an explicitly recorded reversal note stay. In
   `dev-docs/serialize_schema.md`: the worked example regenerated if no documenter did
   it; the hand-written frame-title example checked against the present titles; the
   paragraph that says the golden file pins the example byte for byte brought in line
   with D2 (the goldens mask the `message` values).
5. Checks: the label is unique (grep), the new entry is referenced from ARCHITECTURE.

### 4.4 Checks

The continuous-integration set, run once by the checks agent, one command at a time:

- `cargo test --workspace` and `cargo test --workspace --all-features`;
- `cargo clippy --workspace --all-targets -- -D warnings` and the same with
  `--all-features`;
- `rm -rf target/doc`, then `cargo docs --all-features` with `RUSTDOCFLAGS` unset;
- when installed: `cargo check -p techy --target wasm32-unknown-unknown` and the same
  with `--all-features`, and `cargo +1.86.0 check --workspace --all-features
  --all-targets`; otherwise listed as not run;
- `sh scripts/check_semver.sh` only if the branch `api-baseline` exists in the clone and
  `cargo-semver-checks` is installed; otherwise listed as not run (section 7, step 4).

A failure counts as preexisting when the preflight's baseline shows it, or when its
command was not in the baseline and its file has no commit since the start commit. New
failures go to one fixer; the set runs again once.

### 4.5 Handover

`dev-docs/msgbuild/handover.md`, at most about 250 lines, committed. Sections, in this
order: State (branch, start commit, last commit); Commits per work package and unit; What
was done (one line per unit); Provisional choices taken, with reasons; Checks (results,
what did not run, preexisting failures); Open points for Philippe (at most three, each
with context, options and a recommendation, then a list of the others, including the
design questions of the reviewers and the review findings that no fixer resolved);
Proposals of the documenters (names, suspected API design problems); Contradictions
between the old documentation and the code; Tests that lost a case distinction (D4), with
the proposed fields; Remaining work reported by the final sweeps and units that failed;
Quotations left in `dev-docs/`; Leftover changes that agents stashed (`git stash list`);
`// ###` notes found; two before-and-after samples per work package; How to continue.

## 5. Rules for every build agent

These rules add to `CLAUDE.md` (which every agent receives) and, for documenters, to their
agent definition.

- **Checkout and git.** Work in the checkout and on the branch that your prompt names.
  Never create or switch a branch, never merge, never push: the supervisor session pushes.
  Run every git command as `git -C <checkout> ...`. Give absolute paths to tools; never
  write an absolute path of the machine into code, documentation or a commit message.
- **The write turn.** Only an agent whose prompt says that it holds the write turn edits
  files, runs cargo or commits. It starts by checking `git status --porcelain`: changes
  that it did not make (left by a stopped agent) are stashed with
  `git stash push -u -m "msgbuild leftover"` and reported. It ends with a clean tree. A
  read-only agent reads changes with `git show <sha>` and files with
  `git show HEAD:<path>`, since the working tree may be changing.
- **Baseline failures.** Your prompt lists the failures that existed before the build.
  Do not fix them and do not count them as yours; report one only if your change touches
  it. This overrides "Fix every failure and every warning" in the documenter's
  instructions for these failures.
- **Cargo.** Unless `env` shows `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=0`,
  prefix every cargo command with both. Use the checkout's own `target/`. Run every cargo
  command in the background with all output redirected to a log under
  `target/msgbuild-logs/`, wait for the completion notification, then grep the log
  (`test result:`, `error`, `warning`, `FAILED`); never read a whole log, never poll with
  sleep. One cargo command at a time. Before the first build, check the free disk
  (`df -h`); below 5 GB, delete `target/debug/incremental`; if still short, stop and
  return status "blocked".
- **Scoped checks.** During the work packages, every check is scoped to the crates that
  the step touched (sections 4.1 and 4.2, and the documenter's own instructions). Only the
  preflight and the checks phase (section 4.4) run workspace-wide commands.
- **Rust version.** The workspace's minimum Rust version is 1.86, and the
  continuous-integration workflow builds all targets with it: no `let` chains and no API
  newer than 1.86, in tests too.
- **Commits.** Commit at regular intervals, each commit compiling and tested, with
  `git -C <checkout> commit -F target/msgbuild-logs/commit-msg.txt`. Subject in the
  imperative; a short body that says what changed and why; no reference to this plan, the
  build, its units or the developer docs; no identity override; end with the attribution
  lines that your harness's system reminder gives.
- **Writing.** US English. No reference to former behavior ("previously", "no longer",
  "used to", "instead of", "now" or "new" in that sense) in code, rustdoc, messages or
  guides. No reference to the developer docs in user-facing text. Do not run `cargo fmt`;
  match the surrounding code's style.
- **Tests.** Never pin the wording of a library text (R2). Never weaken a test silently:
  list every dropped or rewritten test in your result.
- **Design questions.** R4.
- **Context.** Keep your context under about 150,000 tokens: grep, then read the ranges
  you need; never read a generated file, `inventory.json` or a long test file whole.
  Query `inventory.json` with `jq` if it exists, otherwise with `python3 -I -c` and the
  `json` module. If your context grows large before your work is complete, commit the
  compiling, tested state and return status "partial" with a precise account of what
  remains.
- **Interrupted runs.** If your prompt gives the build's start commit: the commits after
  it include those of earlier phases and other units. Only a commit that does exactly your
  task may come from an interrupted earlier run of your task: continue from the current
  state, and report the work of such commits in your result as if you had done it.

## 6. The workflow

`dev-docs/msgbuild/build.js`. Arguments:

```
{ "checkout": "/absolute/path/of/the/checkout",   // required
  "branch": "the container's branch",             // required
  "start_commit": "...",    // optional: only for a fresh run after a lost run id
  "skip": ["tests"],        // optional: phases to skip (tests, messages, records, checks)
  "test_units": [...], "message_units": [...]     // optional: replace the default units
}
```

Rulings are not arguments: they are amendments to section 2 (section 7, step 3).

**Preflight.** The first agent confirms a clean checkout on the branch, the presence of
this plan and of the documenter definition, the free disk, and the installed tools (it
installs the toolchain 1.86.0 and the WebAssembly target with `rustup` if it can). It runs
the baseline: the commands of section 4.4 that can run, and the documenter's documentation
check for both crates, and records the failures of each command. The workflow halts if
the checkout is unusable; baseline failures do not halt it. With `start_commit`, the
baseline runs at that commit, in a detached worktree.

**Size.** The workflow starts about 90 to 110 agents (at most about 140 if every review
asks for a fix), of which at most one writes at a time; it runs for many hours. The
session's workflow size setting must allow this.

**Resume.** A stopped run is resumed with
`Workflow({scriptPath: "<checkout>/dev-docs/msgbuild/build.js", resumeFromRunId: "<run id>", args: <exactly the same args>})`.
The runtime returns cached results for the longest unchanged prefix of `agent()` calls;
every call after the first changed one runs live. Since the prompts do not depend on
rulings, the calls before the stop stay cached. Agents that run again over work already
committed follow the interrupted-run rule of section 5. Changing `start_commit` changes
the first prompt, so everything runs live; changing `skip` or the units changes the calls
after the preflight. Change `args` only for a fresh run after a lost run id, with
`start_commit` set to the original start commit and `skip` naming the phases that are
complete.

**Return value.** The status (`complete`, `complete-with-errors`, `halted`,
`handover-missing`), the start commit, the units that failed, the handover agent's
result, whether the final checks passed and what did not run, and the status of each
unit.

## 7. The supervisor session

0. **Before the container starts (on Philippe's machine).** Commit `dev-docs/msgbuild/`
   and `.claude/agents/documenter.md` (with `git add -f`, D8) on `main`, and push `main`.
   The container's branch must start from that commit. To have the semver check run in
   the container and in the continuous-integration workflow, also push the branch
   `api-baseline`; otherwise Philippe runs `sh scripts/check_semver.sh` locally before
   merging.
1. **Prerequisites.** Read this plan in full. Confirm that no decision of section 2 is
   PROPOSED; otherwise ask Philippe, write each answer into section 2 (mark the decision
   DECIDED, or amend it in Philippe's words), and commit the change on the container's
   branch before the launch (the preflight halts on uncommitted changes). Confirm that
   the checkout contains
   `.claude/agents/documenter.md` and that the agent type `documenter` is available in
   this session (if the file arrived after the session started, restart the session).
   Confirm that multi-agent orchestration is enabled for this session (Philippe's prompt
   says "ultracode" or names this plan) and that the workflow size setting allows about
   140 agents.
2. **Launch.** Determine the checkout (`git rev-parse --show-toplevel`) and the branch
   (`git branch --show-current`), then run
   `Workflow({scriptPath: "<checkout>/dev-docs/msgbuild/build.js", args: {"checkout": "<checkout>", "branch": "<branch>"}})`.
   Note the run id and the start commit that the workflow logs.
3. **A ruling during the build.** Stop the run (the task's stop control). Inspect
   `git status`: stash leftover changes of the stopped writer with
   `git stash push -u -m "msgbuild leftover"`. Add the ruling to the end of section 2 as a
   dated amendment in Philippe's words, and commit it. Resume with the same run id and
   exactly the same args. Agents that start after the resume read the amendment; finished
   agents are not rerun. If the ruling invalidates finished work, say so to Philippe and
   agree on a follow-up (for instance a later run with `message_units` naming the units to
   redo).
4. **After the run.** Read the handover. Push the branch (`git push -u origin <branch>`;
   on a network error retry up to four times with growing waits). If `gh` is available,
   check the continuous-integration run of the push (`gh run list --branch <branch>`);
   otherwise ask Philippe to look at its minimum-Rust-version and WebAssembly jobs. Report
   to Philippe: what was done, the provisional choices, the open points, the checks that
   did not run (the semver check runs only on Philippe's machine unless `api-baseline` was
   pushed), and the reminder to tell a session once the branch is merged.
5. **Merge.** Philippe merges the branch into `main` (locally, rebase and fast-forward),
   after running the checks that the container could not run.
6. **Closing the folder.** The session that Philippe tells about the merge moves the
   handover's remaining open points and the D4 list to the section "Specific things - from
   Claude [CLAUDE]" of `TODO_Big.md` (D9), deletes `dev-docs/msgbuild/`, commits and
   pushes.
