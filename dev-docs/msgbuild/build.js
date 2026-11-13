export const meta = {
  name: 'techy-msgbuild',
  description: 'techy build: reassess wording-pinning tests, rewrite all diagnostic and error messages with their docs, record, check, hand over',
  whenToUse: 'Run by the supervisor session of dev-docs/msgbuild/PLAN.md, in the checkout of a cloud container',
  phases: [
    { title: 'Preflight', detail: 'checkout state, disk, tools, baseline of the continuous-integration set' },
    { title: 'Tests', detail: 'per unit: implementer (one writer at a time), reviewer, fixer' },
    { title: 'Tests sweep', detail: 'find remaining wording-pinning tests; one extra round; final report' },
    { title: 'Messages', detail: 'per unit: documenter + wording-failure test fix, rules + fact reviewers, fixer' },
    { title: 'Messages sweep', detail: 'find remaining texts that break the rules; one extra round; final report' },
    { title: 'Consistency', detail: 'cross-unit terminology review and one fixer' },
    { title: 'Records', detail: 'design-rationale entry, references, dev-docs quotations; review' },
    { title: 'Checks', detail: 'continuous-integration set; one fix round; re-run' },
    { title: 'Handover', detail: 'write and commit dev-docs/msgbuild/handover.md' },
  ],
}

// ---------------------------------------------------------------------------
// Arguments (PLAN.md section 6). Rulings given during the build are NOT passed
// here: they are committed to PLAN.md section 2, which every agent reads, so
// that prompts stay unchanged and a resume keeps its cache.
// ---------------------------------------------------------------------------
const A = args || {}
if (!A.checkout || !A.branch) {
  throw new Error('args.checkout (absolute path of the checkout) and args.branch are required')
}
const CK = A.checkout
const BR = A.branch
const SKIP = new Set(A.skip || [])
const MODEL = 'opus' // standing ruling: every agent runs on Opus
const PLAN = `${CK}/dev-docs/msgbuild/PLAN.md`
const INV = `${CK}/dev-docs/msgbuild/inventory.json`
const AGENTDEF = `${CK}/.claude/agents/documenter.md`

// Work units (PLAN.md section 4). Authoritative here; PLAN.md summarizes them.
const DEFAULT_TEST_UNITS = [
  { key: 'T1', title: 'Diagnostics core and report rendering',
    files: ['techy/src/error.rs', 'techy/src/state/parsing_state.rs', 'techy/tests/derive_conditions.rs'],
    notes: 'Mostly text-machinery tests (decision D1): switch them to fixture conditions and check structure, not fixed layout words. Comparisons of two library outputs with each other stay (section 4.1 step 1).' },
  { key: 'T2', title: 'Serialization of diagnostics, goldens, streams',
    files: ['techy/src/serialize/drivers/diagnostic_tests.rs', 'techy/tests/serialize_golden.rs', 'techy/tests/golden/serialize/schema_example.json', 'techy/tests/golden/serialize/read_then_append.json', 'techy/tests/serialize_stream.rs', 'techy/src/latexlike/serialize_tests.rs', 'techy/src/serialize/serde_tests.rs'],
    notes: 'Decision D2 (method stated there) for the goldens and the serialized-JSON expectations. Round-trip equalities (assert_diagnostics_equivalent, render_all of the original against the read-back) stay, except in tests/serialize_golden.rs, where the read-back comes from the masked golden: there D2 says what the read-back checks compare. serde_json error texts are third-party wording (decision D4).' },
  { key: 'T3', title: 'Serialization drivers and engine',
    files: ['techy/src/serialize/drivers/tree_tests.rs', 'techy/src/serialize/drivers/tests.rs', 'techy/src/serialize/engine/tests.rs', 'techy/src/serialize/tests.rs'],
    notes: 'DeserializeError::Failed details are free text (decision D4).' },
  { key: 'T4', title: 'Nodes parser, groups, span tiling',
    files: ['techy/src/constructs/nodes_parser.rs', 'techy/src/constructs/span_tiling_tests.rs', 'techy/src/constructs/content_reader_tests.rs', 'techy/src/constructs/chars_group_parser.rs', 'techy/src/constructs/group_parser.rs'] },
  { key: 'T5', title: 'Environments (core and preset)',
    files: ['techy/src/constructs/environment_parser.rs', 'techy/src/latexlike/environments.rs'] },
  { key: 'T6', title: 'Argument, terminated, verbatim, tack-on parsers; attached sources; construct framework',
    files: ['techy/src/constructs/argument_parsers.rs', 'techy/src/constructs/terminated_parser.rs', 'techy/src/constructs/verbatim_parser.rs', 'techy/src/constructs/tack_on_parser.rs', 'techy/src/constructs/attached_source.rs', 'techy/src/constructs/mod.rs'] },
  { key: 'T7', title: 'Engine, descent guard, token layer',
    files: ['techy/src/engine/descent_guard.rs', 'techy/src/engine/language.rs', 'techy/src/engine/mod.rs', 'techy/src/token/reader.rs', 'techy/src/token/scan.rs', 'techy/src/token/token.rs'],
    notes: 'Descent-guard and ImplementationError details are free text (decision D4). TokenKind Display is pinned in token/token.rs and token/reader.rs. should_panic tests: decision D11.' },
  { key: 'T8', title: 'Preset, scopes, sources',
    files: ['techy/src/latexlike/arguments.rs', 'techy/src/latexlike/input.rs', 'techy/src/latexlike/invocation_syntax.rs', 'techy/src/latexlike/minidefs.rs', 'techy/src/latexlike/mod.rs', 'techy/src/latexlike/spec.rs', 'techy/src/scopes/mod.rs', 'techy/src/source/resolver.rs'],
    notes: 'ResolveError cycle versus depth is distinguished only by text (decision D4). latexlike/spec.rs pins the preset frame titles (the scan missed it).' },
  { key: 'T9', title: 'Tree tools, acceptance tests, guide doctests, derive crate',
    files: ['techy/src/node/invariants.rs', 'techy/src/node/mod.rs', 'techy/src/recompose/tests.rs', 'techy/src/transform/tests.rs', 'techy/tests/acceptance.rs', 'docs/learn-by-example.md', 'techy-derive/src/value_derive.rs', 'techy-derive/src/serial_value.rs'],
    notes: 'Guide doctests: decision D3. Derive rejection tests: decision D4. FinalizeError naming the event is free text (decision D4). node/invariants.rs has should_panic tests whose expected text is TreeViolationKind Display: decision D11.' },
]

const DEFAULT_MESSAGE_UNITS = [
  { key: 'M1', title: 'Token layer',
    files: ['techy/src/token/error.rs', 'techy/src/token/reader.rs', 'techy/src/token/scan.rs', 'techy/src/token/specials.rs', 'techy/src/token/token.rs'],
    items: 'Conditions EndOfStreamAfterEscape and ForbiddenChar; the Display texts of TokenError/TokenErrorKind, SpecialsScanError and TokenKind; detail strings built in these files.' },
  { key: 'M2', title: 'Group, argument, terminated, tack-on, verbatim and embellishment parsers',
    files: ['techy/src/constructs/group_parser.rs', 'techy/src/constructs/argument_parsers.rs', 'techy/src/constructs/terminated_parser.rs', 'techy/src/constructs/tack_on_parser.rs', 'techy/src/constructs/verbatim_parser.rs', 'techy/src/constructs/embellishments_parser.rs', 'techy/src/constructs/chars_group_parser.rs', 'techy/src/constructs/invocation_parser.rs'],
    items: 'Conditions UnclosedGroup (with the UnclosedGroupFound field enum), MissingMandatoryArgument, ExpectedExpressionArgument, ExpectedArgumentOpener, MissingArgumentTerminator, the tack-on repeated-field condition, UnterminatedVerbatim, ExpectedVerbatimDelimiter; frame titles and detail strings built in these files; prose quotations of these messages.' },
  { key: 'M3', title: 'Environments (core and preset) and preset spec frame titles',
    files: ['techy/src/constructs/environment_parser.rs', 'techy/src/latexlike/environments.rs', 'techy/src/latexlike/spec.rs'],
    items: 'Conditions EnvironmentTerminatorMismatch, MalformedEnvironmentTerminator, MissingEnvironmentTerminator (with MissingTerminatorFound), MalformedBegin, UnknownEnvironment, OrphanEnd; the frame titles of the preset specs; detail strings in these files. Decision D12 fixes the preset frame-title terms and what OrphanEnd names.' },
  { key: 'M4', title: 'Nodes parser, root parser, command resolution texts',
    files: ['techy/src/constructs/nodes_parser.rs', 'techy/src/constructs/root_parser.rs', 'techy/src/engine/driver.rs', 'techy/src/scopes/mod.rs'],
    items: 'The five conditions of nodes_parser.rs (stray group close, expression callable requiring content, UnresolvableCommand, CommandResolutionFailed, UnusableRecoveryToken with UnusableRecoveryTokenKind); detail strings in nodes_parser.rs and root_parser.rs. Also the texts that UnresolvableCommand and CommandResolutionFailed embed, which this unit owns (rule VIII): the detail composed in engine/driver.rs (the default resolver detail and did_you_mean_hint) and the Display of SearchedProviders in scopes/mod.rs. CommandResolutionFailed also embeds the Display of ProviderError (through the resolution detail built in engine/driver.rs), which M5 owns and words before you: compose around it. Do not change the other texts of engine/driver.rs (M7) and scopes/mod.rs (M5, M6).' },
  { key: 'M5', title: 'Construct framework: implementation errors, scope operations, descent limits, node building',
    files: ['techy/src/constructs/mod.rs', 'techy/src/scopes/mod.rs', 'techy/src/engine/descent_guard.rs'],
    items: 'Conditions ImplementationError, ScopeOpFailed, DescentLimitExceeded, DescentLimitApproaching; NodeBuildError Display; detail strings in constructs/mod.rs. Also the texts these conditions embed, which this unit owns (rule VIII): the chain ScopeOpFailed → ScopeOpError → ScopeStackError → ProviderError (the Displays of ScopeOpError, ScopeStackError and ProviderError in scopes/mod.rs; CommandResolutionFailed of M4 embeds ProviderError too, so word ProviderError so that it also fits there) and the detail strings of the descent guard (engine/descent_guard.rs; WalkError (M8) and the serialization errors (M10) embed them too). Do not change the other texts of scopes/mod.rs (M4, M6).' },
  { key: 'M6', title: 'Specs, scopes, attached sources, source resolution, input',
    files: ['techy/src/scopes/mod.rs', 'techy/src/scopes/provenance.rs', 'techy/src/constructs/attached_source.rs', 'techy/src/source/resolver.rs', 'techy/src/spec/callable.rs', 'techy/src/latexlike/input.rs'],
    items: 'Conditions ProviderCommandsShadowedByEscape, CallableDefinedAsError, NoSourceResolver, UnresolvableSourceReference, InvalidSourceReferenceArgument (with InvalidReferenceReason); the error Displays of scopes and provenance (DefinitionKey, OriginLabel, …) except SearchedProviders (M4), ScopeOpError, ScopeStackError and ProviderError (M5); ResolveError; the input error; frame titles of spec/callable.rs, attached_source.rs and latexlike/input.rs; detail strings in these files.' },
  { key: 'M7', title: 'Engine, report rendering, diagnostics core, parsing state',
    files: ['techy/src/error.rs', 'techy/src/engine/mod.rs', 'techy/src/engine/driver.rs', 'techy/src/engine/language.rs', 'techy/src/state/parsing_state.rs', 'techy/src/latexlike/driver.rs'],
    items: 'Condition HookFailed; the Displays of Diagnostic, Severity, InconsistentDiagnosticCounts, ParseError and the engine error types; the fixed texts of rendered reports (render, render_all, format_traceback, format_position: "included from", "… and N more"; the heading "Inside:" is fixed by decision D12); frame titles of engine/mod.rs and latexlike/driver.rs; detail strings of engine/driver.rs (except the resolver detail and did_you_mean_hint, owned by M4) and engine/language.rs. Decision D10 for the case of embedded texts.' },
  { key: 'M8', title: 'Tree tools: node building and invariants, transform, visit, recompose, extract',
    files: ['techy/src/node/builder.rs', 'techy/src/node/invariants.rs', 'techy/src/node/node_ref.rs', 'techy/src/transform/mod.rs', 'techy/src/visit.rs', 'techy/src/recompose/mod.rs', 'techy/src/extract.rs'],
    items: 'The Displays of the builder errors, TreeViolationKind, NamedAccessError, RestageError, WalkError, RecomposeError, ExtractError, and their rustdoc. WalkError embeds the descent-guard details owned by M5.' },
  { key: 'M9', title: 'Preset: arguments, invocation syntax, recompose, preset module',
    files: ['techy/src/latexlike/arguments.rs', 'techy/src/latexlike/invocation_syntax.rs', 'techy/src/latexlike/recompose.rs', 'techy/src/latexlike/mod.rs', 'techy/src/latexlike/minidefs.rs'],
    items: 'ArgumentCodeError, the invocation-syntax error, the preset recompose error, detail strings in latexlike/mod.rs; their rustdoc.' },
  { key: 'M10', title: 'Serialization errors',
    files: ['techy/src/serialize/error.rs'],
    items: 'The Displays of SerializeError, DeserializeError, RegistrationError, SerialValueError (and its name-list helper), SessionDeriveError; their rustdoc and variant docs. The descent-limit variants embed the descent-guard details owned by M5; the mismatch texts embed the expected-value phrases of serialize/object.rs owned by M11. About 70 texts: keep within your context limit and return "partial" if needed.' },
  { key: 'M11', title: 'Serialization drivers, engine, wire, objects, rendering',
    files: ['techy/src/serialize/drivers/diagnostic.rs', 'techy/src/serialize/drivers/tree.rs', 'techy/src/serialize/drivers/specs.rs', 'techy/src/serialize/engine/dispatch.rs', 'techy/src/serialize/engine/session.rs', 'techy/src/serialize/render.rs', 'techy/src/serialize/bridge.rs', 'techy/src/serialize/wire/mod.rs', 'techy/src/serialize/wire/tree.rs', 'techy/src/serialize/object.rs', 'techy/src/serialize/base64.rs'],
    items: 'The documentation of DeserializedCondition (it displays the stored message unchanged); detail strings built in these files; the expected-value phrases of value_mismatch in serialize/object.rs (embedded by the DeserializeError mismatch text of M10: keep them fragments that fit "expected …"); Base64Error Display; other texts listed for these files in inventory.json.' },
  { key: 'M12', title: 'Derive crate',
    files: ['techy-derive/src/lib.rs', 'techy-derive/src/diagnostic_info.rs', 'techy-derive/src/serial_value.rs', 'techy-derive/src/to_value.rs', 'techy-derive/src/value_derive.rs'],
    items: 'The compile-error texts that the derives emit (their reader is a developer), and the rustdoc of the derives where it quotes messages (the derive example copies the message of UnresolvableCommand: make it match the wording M4 chose).' },
]

const TEST_UNITS = A.test_units || DEFAULT_TEST_UNITS
const MESSAGE_UNITS = A.message_units || DEFAULT_MESSAGE_UNITS

// ---------------------------------------------------------------------------
// One writer at a time: every agent that edits files, runs cargo or commits
// runs inside serial(). Reviewers and sweeps are read-only and run concurrently.
// serial() is never called from inside serial().
// ---------------------------------------------------------------------------
let writeTail = Promise.resolve()
function serial(fn) {
  const run = writeTail.then(() => fn())
  writeTail = run.then(() => undefined, () => undefined)
  return run
}

const J = (x) => JSON.stringify(x, null, 1)
const arr = (x) => (Array.isArray(x) ? x : [])

// Merge a first result with its continuation: arrays concatenated, booleans
// OR-ed, other fields from the continuation (status, remaining, …).
function mergeResults(r, r2) {
  if (!r) return r2
  if (!r2) return r
  const m = {}
  for (const k of new Set([...Object.keys(r), ...Object.keys(r2)])) {
    if (Array.isArray(r[k]) && Array.isArray(r2[k])) m[k] = [...r[k], ...r2[k]]
    else if (typeof r[k] === 'boolean' && typeof r2[k] === 'boolean') m[k] = r[k] || r2[k]
    else m[k] = r2[k] !== undefined ? r2[k] : r[k]
  }
  return m
}

// parallel() turns a thrown unit into null: keep the unit visible.
function keepUnits(results, units, what) {
  return results.map((r, i) => {
    if (r) return r
    log(`${what} ${units[i] && units[i].key}: the unit threw (agent error); recorded as failed`)
    return { unit: units[i] || { key: `?${i}`, title: '?', files: [] }, error: 'unit threw (agent error)' }
  })
}

// ---------------------------------------------------------------------------
// Schemas
// ---------------------------------------------------------------------------
const STR_ARR = { type: 'array', items: { type: 'string' } }

const PREFLIGHT = {
  type: 'object',
  properties: {
    ok: { type: 'boolean' },
    start_commit: { type: 'string' },
    branch_seen: { type: 'string' },
    disk_free: { type: 'string' },
    tools: { type: 'string', description: 'cargo/rustc versions; installed targets; toolchain 1.86.0; cargo-semver-checks; api-baseline branch; jq; gh' },
    baseline: { type: 'array', items: { type: 'object', properties: {
      command: { type: 'string' }, passed: { type: 'boolean' },
      failures: { ...STR_ARR, description: 'failing tests, or "file:line: lint/warning" entries, or the error' },
    }, required: ['command', 'passed', 'failures'] } },
    problems: STR_ARR,
  },
  required: ['ok', 'start_commit', 'baseline', 'problems'],
}

const TEST_RESULT = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    commits: STR_ARR,
    tests: { type: 'array', items: { type: 'object', properties: {
      file: { type: 'string' }, test: { type: 'string' },
      action: { type: 'string', enum: ['rewritten', 'dropped', 'switched-to-fixture', 'kept-unchanged'] },
      reason: { type: 'string' }, now_checks: { type: 'string' },
    }, required: ['file', 'test', 'action', 'reason'] } },
    needs_structured_field: { type: 'array', items: { type: 'object', properties: {
      file: { type: 'string' }, test: { type: 'string' }, item: { type: 'string' },
      distinction_lost: { type: 'string' }, proposal: { type: 'string' },
    }, required: ['file', 'test', 'distinction_lost', 'proposal'] } },
    non_test_code_changed: STR_ARR,
    leftovers_stashed: STR_ARR,
    open_questions: STR_ARR,
    checks: STR_ARR,
    remaining: { type: 'string' },
    notes: { type: 'string' },
  },
  required: ['status', 'commits', 'tests', 'checks'],
}

const REVIEW = {
  type: 'object',
  properties: {
    verdict: { type: 'string', enum: ['ok', 'fix'] },
    findings: { type: 'array', items: { type: 'object', properties: {
      severity: { type: 'string', enum: ['blocking', 'should-fix', 'nit'] },
      file: { type: 'string' }, line: { type: 'integer' },
      issue: { type: 'string' }, fix: { type: 'string' },
    }, required: ['severity', 'file', 'issue', 'fix'] } },
    design_questions: STR_ARR,
    notes: { type: 'string' },
  },
  required: ['verdict', 'findings', 'design_questions'],
}

const DOC_RESULT = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    commits: STR_ARR,
    first_commit_before: { type: 'string' },
    items_written: STR_ARR,
    unchanged_texts: STR_ARR,
    removed_content: STR_ARR,
    contradictions: STR_ARR,
    proposals: STR_ARR,
    review_notes: STR_ARR,
    devdocs_references: STR_ARR,
    devdocs_quotes_left: STR_ARR,
    unsure_statements: STR_ARR,
    difficulties: STR_ARR,
    open_questions: { ...STR_ARR, description: 'design questions (plan R4): context, options, recommendation' },
    leftovers_stashed: STR_ARR,
    checks: STR_ARR,
    wording_test_failures: { type: 'array', items: { type: 'object', properties: {
      file: { type: 'string' }, test: { type: 'string' }, detail: { type: 'string' },
    }, required: ['file', 'test'] } },
    regenerated_worked_example: { type: 'boolean' },
    samples: { type: 'array', items: { type: 'object', properties: {
      before: { type: 'string' }, after: { type: 'string' },
    }, required: ['before', 'after'] } },
    remaining: { type: 'string' },
    notes: { type: 'string' },
  },
  required: ['status', 'commits', 'items_written', 'checks', 'wording_test_failures'],
}

const SWEEP = {
  type: 'object',
  properties: {
    units: { type: 'array', items: { type: 'object', properties: {
      key: { type: 'string' }, title: { type: 'string' }, files: STR_ARR,
      items: { type: 'string' }, notes: { type: 'string' },
    }, required: ['key', 'title', 'files', 'items'] } },
    notes: { type: 'string' },
  },
  required: ['units', 'notes'],
}

const SIMPLE_RESULT = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    commits: STR_ARR, summary: { type: 'string' }, open_questions: STR_ARR, checks: STR_ARR,
    leftovers_stashed: STR_ARR, notes: { type: 'string' },
  },
  required: ['status', 'commits', 'summary'],
}

const CHECKS = {
  type: 'object',
  properties: {
    all_passed: { type: 'boolean' },
    results: { type: 'array', items: { type: 'object', properties: {
      command: { type: 'string' }, passed: { type: 'boolean' }, summary: { type: 'string' },
    }, required: ['command', 'passed', 'summary'] } },
    not_run: { type: 'array', items: { type: 'object', properties: {
      command: { type: 'string' }, reason: { type: 'string' },
    }, required: ['command', 'reason'] } },
    failures: { type: 'array', items: { type: 'object', properties: {
      command: { type: 'string' }, where: { type: 'string' }, error: { type: 'string' },
      kind: { type: 'string', enum: ['wording-test', 'test', 'clippy', 'doc', 'compile', 'other'] },
      preexisting: { type: 'boolean' },
    }, required: ['command', 'where', 'error', 'kind', 'preexisting'] } },
  },
  required: ['all_passed', 'results', 'not_run', 'failures'],
}

const HANDOVER = {
  type: 'object',
  properties: {
    handover_path: { type: 'string' }, last_commit: { type: 'string' },
    summary: { type: 'string' }, open_points: STR_ARR,
  },
  required: ['handover_path', 'last_commit', 'summary', 'open_points'],
}

// ---------------------------------------------------------------------------
// Prompt pieces
// ---------------------------------------------------------------------------
const BASE = `You are a build agent of the techy "msgbuild" build, running in a cloud container.
Checkout: ${CK} (branch ${BR}). Plan: ${PLAN}.
Before your first command, read the plan's sections 2 ("Decisions", including any dated amendments) and 5 ("Rules for every build agent") and follow them. Read other plan sections when this prompt points you to them.`

const READONLY = `You are READ-ONLY: do not edit files, do not run cargo, do not commit. Another agent may be editing the working tree while you work, so read changes with \`git -C ${CK} show <sha>\` and files as committed with \`git -C ${CK} show HEAD:<path>\` (or at a given commit).`

// Set after the preflight.
let START = null
let BASELINE_FAILING = []
let BASELINE_ALL = []

function writer() {
  return `You hold the write turn: you are the only agent that edits files, runs cargo or commits right now. Do not push.
Begin with \`git -C ${CK} status --porcelain\`. If the tree has changes that you did not make (left by an agent that was stopped), stash them with \`git -C ${CK} stash push -u -m "msgbuild leftover"\`, list them in leftovers_stashed (or notes), and continue on the clean tree. End with a clean tree: commit your work or revert it.
Failures that existed before the build (the preflight's baseline): ${J(BASELINE_FAILING)}. Do not fix them and do not count them as yours; report one only if your change touches it.`
}

function startLine(kind) {
  return `The build's start commit is ${START}. \`git -C ${CK} log --oneline ${START}..HEAD -- <your files>\` shows the commits of earlier phases and other units as well. A commit that does exactly your task (${kind}) may come from an interrupted earlier run of this same task: if there is one, continue from the current state, and include the work of those commits in your result (read it from their diffs) as if you had done it.`
}

// ---------------------------------------------------------------------------
// Tests work package (PLAN.md section 4.1)
// ---------------------------------------------------------------------------
function testPrompt(u, prior) {
  return `${BASE}
${writer()}

YOUR TASK: test reassessment, unit ${u.key} ("${u.title}"), by the procedure of the plan's section 4.1 and decisions D1 to D4 and D11.
Files of the unit: ${arr(u.files).join(', ')}.
${u.items ? `Items: ${u.items}\n` : ''}${u.notes ? `Notes: ${u.notes}\n` : ''}
Hints: ${INV} lists, under "tests" → "tests", the tests that a scan found, with a first classification (filter by "file"). The scan is a hint: verify every entry against the code, and search your files yourself for tests it missed (the plan's section 3 names what the scan left out).
${startLine(`the test reassessment of unit ${u.key}`)}
${prior ? `\nAn earlier agent returned status "partial" for this unit. Its result:\n${J(prior)}\nContinue the remaining work.\n` : ''}
Return the structured result.`
}

function failuresPrompt(owner, failures) {
  return `${BASE}
${writer()}

YOUR TASK: reassess exactly the following tests, by the procedure of the plan's section 4.1 and decisions D1 to D4 and D11. They fail because ${owner} changed the wording of library messages, and they compare that wording. They may lie in any file. Do not touch other tests, and do not change any message text.
Failing tests:
${J(failures)}
Return the structured result.`
}

function testReviewPrompt(u, res) {
  return `${BASE}
${READONLY}

YOUR TASK: review the test reassessment of ${u.key} ("${u.title}")${arr(u.files).length ? `, files ${arr(u.files).join(', ')}` : ''}, against the plan's section 4.1 and decisions D1 to D4 and D11.
The implementer's result:
${J(res)}

Check:
1. Each dropped test: read the old code in the commit. Did the test check anything meaningful for the library's function beyond the wording of a library text? If yes, the drop is a blocking finding (say what behavior is lost).
2. Each rewritten or switched test: does it check the behavior that matters (identifier or condition type, field values, span, severity, frames, count and order of diagnostics, node tree, structure of a report) directly, and no wording of a library text? Is it at least as strong as the old test for that behavior? Comparisons of two library outputs with each other are allowed (section 4.1 step 1).
3. Decisions D1 to D4 and D11 applied as written; no non-test code, message text or public API changed (test-only fixtures and helpers are fine); the Rust version rule of section 5 respected.
4. No test in the reviewed files still compares the wording of a library text (grep the files at HEAD).
5. Each "needs_structured_field" entry: is the lost distinction real, and is the proposal sensible?
Severity: blocking (coverage lost, wrong test), should-fix (rule broken), nit. A point that needs Philippe's decision goes into design_questions, not into findings.`
}

function testFixPrompt(u, res, rev) {
  return `${BASE}
${writer()}

YOUR TASK: apply the review findings of severity "blocking" and "should-fix" to the test reassessment of ${u.key} ("${u.title}"), following the plan's section 4.1 and decisions D1 to D4 and D11. Do not act on design questions; list them in open_questions. If a finding is wrong (it contradicts the code or the plan), do not apply it; explain why in notes.
The implementer's result:
${J(res)}
The review:
${J(rev)}
Return the structured result (list in "tests" only the tests you changed in this pass).`
}

async function withContinuation(make, opts) {
  let r = await serial(() => agent(make(null), opts))
  if (r && r.status === 'partial') {
    const r2 = await serial(() => agent(make(r), { ...opts, label: `${opts.label}:cont` }))
    r = mergeResults(r, r2)
  }
  return r
}

// Review a test change and fix it if needed (used for units and for wording failures).
async function reviewTestChange(u, res, ph, label) {
  if (!res || !arr(res.commits).length) return { rev: null, fix: null }
  const rev = await agent(testReviewPrompt(u, res),
    { label: `${label}:review`, phase: ph, schema: REVIEW, model: MODEL })
  let fix = null
  if (rev && arr(rev.findings).some((f) => f.severity !== 'nit')) {
    fix = await serial(() => agent(testFixPrompt(u, res, rev),
      { label: `${label}:fix`, phase: ph, schema: TEST_RESULT, model: MODEL }))
  }
  return { rev, fix }
}

async function runTestUnit(u, ph) {
  const label = `tests:${u.key}`
  const res = await withContinuation(
    (prior) => testPrompt(u, prior),
    { label: `${label}:implement`, phase: ph, schema: TEST_RESULT, model: MODEL })
  if (!res) { log(`${label}: implementer returned nothing`); return { unit: u, error: 'implementer returned nothing' } }
  const { rev, fix } = await reviewTestChange(u, res, ph, label)
  log(`${label}: ${res.status}; ${arr(res.tests).length} tests handled; review ${rev ? rev.verdict : (arr(res.commits).length ? 'missing' : 'not needed')}${fix ? `; fix ${fix.status}` : ''}`)
  return { unit: u, res, rev, fix, reviewMissing: !rev && arr(res.commits).length > 0 }
}

// ---------------------------------------------------------------------------
// Messages work package (PLAN.md section 4.2)
// ---------------------------------------------------------------------------
function docPrompt(u, prior) {
  return `${BASE}
${writer()}

YOUR TASK: a rewrite, unit ${u.key} ("${u.title}"), by your instructions and the plan's section 4.2 (scope of a message unit, ownership of embedded texts) and decisions D6, D10 and D12.
Files of the unit: ${arr(u.files).join(', ')}.
Items: ${u.items}
${u.notes ? `Notes: ${u.notes}\n` : ''}
Hints: ${INV} lists the conditions (under "cond" → "conditions"), the other user-facing texts ("texts" → "items") and the prose quotations of messages ("quotes" → "quotes") that a scan found; filter by file. The scan is a hint: verify every entry and search your files yourself.
The tests of the build no longer compare the wording of messages (section 4.1 ran before you). If, after your changes, a test fails because it compares wording, do not change it: list it in wording_test_failures; an implementer reassesses it right after you.
${startLine(`the message rewrite of unit ${u.key}`)}
${prior ? `\nAn earlier documenter returned status "partial" for this unit. Its result:\n${J(prior)}\nContinue the remaining work.\n` : ''}
Return the structured result; its fields follow the list "Your result" of your instructions. Put design questions (the plan's R4) in open_questions, with context, options and a recommendation.`
}

function rulesReviewPrompt(u, res) {
  return `${BASE}
${READONLY}

YOUR TASK: review the texts written in unit ${u.key} ("${u.title}") against the style rules.
First read ${AGENTDEF}, Parts 2, 3 and 4 (the documentation rules, the rules on diagnostic messages, the samples), and the plan's decisions D6, D10 and D12.
The documenter's result:
${J(res)}
Read each of its commits. Check every message and text that the commits changed, and every one that the documenter reported as unchanged:
- rules (I) to (VIII): what is wrong stated directly with the item it concerns; "Missing X" for an expected thing that is absent; no recovery and no explanation of the program; fixes only when not obvious, written as the text to type; the author's terms for a message a document author can cause, interface terms only for one that only a faulty program or extension can cause; specifics quoted as written; no dependent clauses; one line, capital letter first (D10 for embedded texts), no final period; quotation marks ‘ and ’ (U+2018 and U+2019), never ASCII apostrophes or backticks; one consolidated composition.
- documentation blocks: for a condition, the first sentence states the error as the person who causes it sees it, followed by the identifier in parentheses (the bare identifier only when nothing stands between); one fact per sentence; established terms; conventional verbs, no component described as a person; no metaphors or invented jargon; no negative examples, not even a short instance of the mistake; no reference to former behavior or to the developer docs; none of the banned words; every "the X" identifiable; only what the reader needs; links to related items.
- decision D12: the texts it fixes keep their wording.
Severity: blocking (false, misleading, or a hard rule broken), should-fix (a rule broken), nit (taste). Quote the text in "issue" and give the corrected text in "fix". A point that needs Philippe's decision goes into design_questions.`
}

function factReviewPrompt(u, res) {
  return `${BASE}
${READONLY}

YOUR TASK: fact-check unit ${u.key} ("${u.title}") of the message rewrite.
The documenter's result:
${J(res)}
Read each of its commits. For every statement in the changed rustdoc, guide text and messages, verify it against the code: under which input and by which parser or function the condition is reported; who can cause it; what a tolerant parse puts into the node tree in its place; what each field contains; which defaults apply; that each intra-doc link points to the item meant; that each example shows correct usage and would compile. Verify that each format string or Display still uses the right fields and that every value the message needs is available.
Verify that the diff changes no behavior. Allowed are: documentation comments; string literals and format strings of user-facing texts wherever they are built (condition messages, Display bodies, detail strings at call sites, frame titles, fixed report texts, derive compile errors); a change of how a message is composed when rule (VIII) asks for one composed message; guide prose; the generated example of dev-docs/serialize_schema.md. No test file, no change of control flow, types or public API. Verify the contradictions that the documenter reported.
Severity: blocking (a false statement or a behavior change), should-fix, nit. Quote the text in "issue" and give the corrected text in "fix". Points that need Philippe's decision go into design_questions.`
}

function docFixPrompt(u, res, findings) {
  return `${BASE}
${writer()}

YOUR TASK: apply review findings to ${u.key} ("${u.title}")${arr(u.files).length ? `, files ${arr(u.files).join(', ')}` : ''}, by your instructions and the plan's decisions D6, D10 and D12. Apply the findings of severity "blocking" and "should-fix". If a finding is wrong (it contradicts the code or your instructions), do not apply it; say why in notes. If, after your changes, a test fails because it compares wording, do not change it: list it in wording_test_failures.
${res ? `The documenter's result:\n${J(res)}\n` : ''}The findings:
${J(findings)}
Return the structured result (list in items_written only what you changed in this pass).`
}

// A documenter (or a documenter fixer) and the reassessment of the tests that
// its changes broke run in ONE critical section: the next writer starts green.
async function documentThenFixTests(makeDocCall, ownerLabel, ph) {
  return serial(async () => {
    const doc = await makeDocCall()
    let tf = null
    if (doc && arr(doc.wording_test_failures).length) {
      tf = await agent(failuresPrompt(ownerLabel, doc.wording_test_failures),
        { label: `${ownerLabel}:test-fix`, phase: ph, schema: TEST_RESULT, model: MODEL })
    }
    return { doc, tf }
  })
}

async function reviewFailureFix(tf, ownerLabel, ph) {
  if (!tf || !arr(tf.commits).length) return null
  const pseudo = { key: `${ownerLabel} (wording failures)`, title: 'tests that failed after a change of message wording', files: [] }
  return reviewTestChange(pseudo, tf, ph, `${ownerLabel}:test-fix`)
}

async function runMessageUnit(u, ph) {
  const label = `messages:${u.key}`
  const opts = { label: `${label}:document`, phase: ph, schema: DOC_RESULT, agentType: 'documenter', model: MODEL }
  const first = await documentThenFixTests(() => agent(docPrompt(u, null), opts), label, ph)
  let res = first.doc
  let testFixes = [first.tf].filter(Boolean)
  if (res && res.status === 'partial') {
    const cont = await documentThenFixTests(
      () => agent(docPrompt(u, res), { ...opts, label: `${label}:document:cont` }), `${label}:cont`, ph)
    res = mergeResults(res, cont.doc)
    if (cont.tf) testFixes.push(cont.tf)
  }
  if (!res) { log(`${label}: documenter returned nothing`); return { unit: u, error: 'documenter returned nothing' } }

  let reviews = []
  let fix = null
  if (arr(res.commits).length) {
    reviews = await parallel([
      () => agent(rulesReviewPrompt(u, res), { label: `${label}:review-rules`, phase: ph, schema: REVIEW, model: MODEL }),
      () => agent(factReviewPrompt(u, res), { label: `${label}:review-facts`, phase: ph, schema: REVIEW, model: MODEL }),
    ])
    const findings = reviews.filter(Boolean).flatMap((r) => arr(r.findings)).filter((f) => f.severity !== 'nit')
    if (findings.length) {
      const fx = await documentThenFixTests(
        () => agent(docFixPrompt(u, res, findings),
          { label: `${label}:fix`, phase: ph, schema: DOC_RESULT, agentType: 'documenter', model: MODEL }),
        `${label}:fix`, ph)
      fix = fx.doc
      if (fx.tf) testFixes.push(fx.tf)
    }
  }
  const failureReviews = (await parallel(testFixes.map((tf) => () => reviewFailureFix(tf, label, ph)))).filter(Boolean)
  const reviewMissing = arr(res.commits).length > 0 && reviews.filter(Boolean).length < 2
  log(`${label}: ${res.status}; ${arr(res.items_written).length} items; ${reviews.filter(Boolean).flatMap((r) => arr(r.findings)).filter((f) => f.severity !== 'nit').length} findings${fix ? `; fix ${fix.status}` : ''}${testFixes.length ? `; ${testFixes.length} test fix(es)` : ''}`)
  return { unit: u, res, reviews, fix, testFixes, failureReviews, reviewMissing }
}

// ---------------------------------------------------------------------------
// Compact summaries for later prompts and the handover
// ---------------------------------------------------------------------------
function unresolved(reviews, fix) {
  const f = reviews.filter(Boolean).flatMap((r) => arr(r.findings)).filter((x) => x.severity !== 'nit')
  if (!f.length) return []
  return (!fix || fix.status !== 'done') ? f : []
}

function testSummary(results) {
  return results.map((r) => ({
    unit: r.unit.key, status: r.res ? r.res.status : r.error,
    commits: [...arr(r.res && r.res.commits), ...arr(r.fix && r.fix.commits)],
    tests: [...arr(r.res && r.res.tests), ...arr(r.fix && r.fix.tests)],
    needs_structured_field: [...arr(r.res && r.res.needs_structured_field), ...arr(r.fix && r.fix.needs_structured_field)],
    open_questions: [...arr(r.res && r.res.open_questions), ...arr(r.fix && r.fix.open_questions)],
    non_test_code_changed: [...arr(r.res && r.res.non_test_code_changed), ...arr(r.fix && r.fix.non_test_code_changed)],
    notes: r.res ? r.res.notes : null,
    design_questions: arr(r.rev && r.rev.design_questions),
    review_missing: !!r.reviewMissing,
    fix_status: r.fix ? r.fix.status : null,
    fix_notes: r.fix ? r.fix.notes : null,
    unresolved_findings: unresolved([r.rev], r.fix),
    leftovers_stashed: [...arr(r.res && r.res.leftovers_stashed), ...arr(r.fix && r.fix.leftovers_stashed)],
    remaining: r.res ? r.res.remaining : null,
  }))
}

function messageSummary(results) {
  return results.map((r) => {
    const parts = [r.res, r.fix].filter(Boolean)
    const cat = (k) => parts.flatMap((p) => arr(p[k]))
    const tfx = arr(r.testFixes)
    return {
      unit: r.unit.key, status: r.res ? r.res.status : r.error,
      commits: [...cat('commits'), ...tfx.flatMap((t) => arr(t.commits))],
      items_written: cat('items_written'), unchanged_texts: cat('unchanged_texts'),
      removed_content: cat('removed_content'),
      contradictions: cat('contradictions'), proposals: cat('proposals'), review_notes: cat('review_notes'),
      devdocs_references: cat('devdocs_references'), devdocs_quotes_left: cat('devdocs_quotes_left'),
      unsure_statements: cat('unsure_statements'), difficulties: cat('difficulties'),
      open_questions: cat('open_questions'),
      notes: r.res ? r.res.notes : null,
      leftovers_stashed: cat('leftovers_stashed'),
      regenerated_worked_example: parts.some((p) => p.regenerated_worked_example === true),
      samples: cat('samples').slice(0, 2),
      design_questions: arr(r.reviews).filter(Boolean).flatMap((v) => arr(v.design_questions)),
      review_missing: !!r.reviewMissing,
      fix_status: r.fix ? r.fix.status : null,
      fix_notes: r.fix ? r.fix.notes : null,
      unresolved_findings: unresolved(arr(r.reviews), r.fix),
      test_fixes: tfx.map((t) => ({ status: t.status, tests: t.tests, needs_structured_field: t.needs_structured_field, open_questions: t.open_questions, remaining: t.remaining, notes: t.notes })),
      test_fix_reviews: arr(r.failureReviews).map((x) => ({
        verdict: x.rev ? x.rev.verdict : 'missing',
        design_questions: arr(x.rev && x.rev.design_questions),
        unresolved_findings: unresolved([x.rev], x.fix),
        fix_status: x.fix ? x.fix.status : null,
        fix_tests: arr(x.fix && x.fix.tests),
        fix_open_questions: arr(x.fix && x.fix.open_questions),
        fix_needs_structured_field: arr(x.fix && x.fix.needs_structured_field),
      })),
      remaining: r.res ? r.res.remaining : null,
    }
  })
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------
phase('Preflight')
const baselineRev = A.start_commit || 'HEAD'
const pre = await agent(`${BASE}
You are the first agent of the build; nobody else runs while you work. Do not push.

YOUR TASK: preflight of the build, by the plan's section 6 ("Preflight").
1. Confirm that ${CK} is a clean checkout on branch ${BR} (git status, git branch --show-current), and that ${PLAN} and ${AGENTDEF} exist. Report \`git -C ${CK} rev-parse ${baselineRev}\` as start_commit.
2. Check the free disk (df -h) by the rule of section 5.
3. Record in "tools": the versions of cargo and rustc; the installed targets (rustup target list --installed); whether the toolchain 1.86.0 is installed (rustup toolchain list); whether cargo-semver-checks is installed; whether a branch api-baseline exists locally or on origin; whether jq and gh exist. If rustup works and the network allows it, install the toolchain 1.86.0 (--profile minimal) and the target wasm32-unknown-unknown; otherwise record that they are missing.
4. Run the baseline, by the cargo rules of section 5, at ${baselineRev === 'HEAD' ? 'HEAD, in the checkout itself' : `the commit ${baselineRev}, in a detached worktree (\`git -C ${CK} worktree add --detach ${CK}/target/msgbuild-baseline ${baselineRev}\`; remove it afterwards with \`git worktree remove --force\`)`}: every command of the plan's section 4.4 that can run here, plus, for each crate (techy, techy-derive), the documentation check of the documenter (\`RUSTDOCFLAGS="-D warnings --html-in-header docs/rustdoc-header.html" cargo doc -p <crate> --no-deps --all-features\`). For each command record whether it passed and its failures (failing test names; "file:line: lint" for clippy and documentation warnings; the error otherwise).
Set ok to false only if the checkout is not usable (wrong branch, uncommitted changes, plan or agent definition missing, disk too low, the workspace does not compile). Failures of the baseline do not make ok false.`,
  { label: 'preflight', phase: 'Preflight', schema: PREFLIGHT, model: MODEL })
if (!pre || !pre.ok) {
  return { status: 'halted', stage: 'preflight', preflight: pre }
}
START = pre.start_commit
BASELINE_ALL = arr(pre.baseline).map((b) => ({ command: b.command, passed: b.passed, failures: arr(b.failures) }))
BASELINE_FAILING = BASELINE_ALL.filter((b) => !b.passed).map((b) => ({ command: b.command, failures: b.failures }))
log(`start commit ${START}; baseline: ${BASELINE_FAILING.length ? `${BASELINE_FAILING.length} command(s) fail before the build` : 'all pass'}`)

// --- Tests -----------------------------------------------------------------
let testResults = []
let testSweepFinal = null
if (!SKIP.has('tests')) {
  phase('Tests')
  testResults = keepUnits(await parallel(TEST_UNITS.map((u) => () => runTestUnit(u, 'Tests'))), TEST_UNITS, 'tests')

  phase('Tests sweep')
  const sweepPrompt = (round) => `${BASE}
${READONLY}

YOUR TASK: search the whole repository for tests that still compare the wording of a text produced by library code, by the definition of the plan's section 4.1 step 1 and decisions D1 to D4 and D11 (tests that those rules keep as they are do not count). Search test code in techy/src (cfg(test) modules, *_tests.rs), techy/tests (including the goldens), techy-derive, and the fenced examples of docs/*.md.
${round === 1 ? 'Group what you find into units for one more round of reassessment' : 'This is the final check: report what remains, grouped into units'} (keys ${round === 1 ? 'TX1, TX2, …' : 'TZ1, TZ2, …'}; each unit a few files, at most about 25 tests), with the files and the tests in "items". Return an empty list if nothing remains.
Results of the units so far:
${J(testSummary(testResults))}`
  const sweep1 = await agent(sweepPrompt(1), { label: 'tests:sweep-1', phase: 'Tests sweep', schema: SWEEP, model: MODEL })
  if (sweep1 && arr(sweep1.units).length) {
    log(`tests sweep: ${sweep1.units.length} more unit(s)`)
    const extra = keepUnits(await parallel(sweep1.units.map((u) => () => runTestUnit(u, 'Tests sweep'))), sweep1.units, 'tests')
    testResults = [...testResults, ...extra]
    testSweepFinal = await agent(sweepPrompt(2), { label: 'tests:sweep-2', phase: 'Tests sweep', schema: SWEEP, model: MODEL })
    if (testSweepFinal && arr(testSweepFinal.units).length) {
      log(`tests sweep: ${testSweepFinal.units.length} unit(s) remain; recorded for the handover, not reassessed`)
    }
  } else {
    log(sweep1 ? 'tests sweep: nothing remains' : 'tests sweep: the sweep agent returned nothing')
  }
}

// --- Messages ----------------------------------------------------------------
let messageResults = []
let msgSweepFinal = null
let consistency = null
if (!SKIP.has('messages')) {
  phase('Messages')
  messageResults = keepUnits(await parallel(MESSAGE_UNITS.map((u) => () => runMessageUnit(u, 'Messages'))), MESSAGE_UNITS, 'messages')

  phase('Messages sweep')
  const msgSweepPrompt = (round) => `${BASE}
${READONLY}

YOUR TASK: search all library code of both crates (outside test code) for user-facing texts in the sense of the plan's section 4.2 that still break the rules on diagnostic messages (read ${AGENTDEF} Part 3, and decisions D10 and D12), and for documentation of diagnostic conditions or error types that still begins with "Condition:" or with "Error:" followed by a clause. Signs: a message that begins with a lowercase letter or ends with a period, ASCII apostrophes or backticks used as quotation marks, a multi-line message, a dependent clause ("which", "because", "since"), a message pieced together from several templates. Do not report a text that a unit reported in "unchanged_texts" with a reason, a difficulty a unit reported, or a text that D12 fixes.
${round === 1 ? 'Group what you find into units for one more round' : 'This is the final check: report what remains, grouped into units'} (keys ${round === 1 ? 'MX1, MX2, …' : 'MZ1, MZ2, …'}; each unit a few files), with the files and the texts in "items". Return an empty list if nothing remains.
Results of the units so far:
${J(messageSummary(messageResults))}`
  const msweep1 = await agent(msgSweepPrompt(1), { label: 'messages:sweep-1', phase: 'Messages sweep', schema: SWEEP, model: MODEL })
  if (msweep1 && arr(msweep1.units).length) {
    log(`messages sweep: ${msweep1.units.length} more unit(s)`)
    const extra = keepUnits(await parallel(msweep1.units.map((u) => () => runMessageUnit(u, 'Messages sweep'))), msweep1.units, 'messages')
    messageResults = [...messageResults, ...extra]
    msgSweepFinal = await agent(msgSweepPrompt(2), { label: 'messages:sweep-2', phase: 'Messages sweep', schema: SWEEP, model: MODEL })
    if (msgSweepFinal && arr(msgSweepFinal.units).length) {
      log(`messages sweep: ${msgSweepFinal.units.length} unit(s) remain; recorded for the handover`)
    }
  } else {
    log(msweep1 ? 'messages sweep: nothing remains' : 'messages sweep: the sweep agent returned nothing')
  }

  phase('Consistency')
  const crev = await agent(`${BASE}
${READONLY}

YOUR TASK: cross-unit consistency review of the message rewrite. Read ${AGENTDEF} Part 3 and the plan's decisions D10 and D12. Collect every user-facing text of library code at HEAD (all message format strings and Display bodies of conditions and error types, detail strings, frame titles, fixed report texts, derive compile errors) and compare them with each other. Report inconsistencies across units: the same concept worded differently ("end of input" against "end of the input", "command" against "macro" outside the preset, "argument ‘#1’" forms), different quoting of the same kind of item (a command with or without its escape character), the case of embedded texts against D10, the "Missing X" form used for one absent thing but not another, and composed messages whose parts no longer fit together. Check also that the rustdoc of conditions uses one form for its first sentence across units.
Results of the units:
${J(messageSummary(messageResults))}
Severity: should-fix for an inconsistency, nit for taste. Give the file, the text and the corrected text.`,
    { label: 'consistency:review', phase: 'Consistency', schema: REVIEW, model: MODEL })
  const cfind = crev ? arr(crev.findings).filter((f) => f.severity !== 'nit') : []
  let cfix = null
  if (cfind.length) {
    const cu = { key: 'MC', title: 'Cross-unit consistency', files: [...new Set(cfind.map((f) => f.file))] }
    const fx = await documentThenFixTests(
      () => agent(docFixPrompt(cu, null, cfind),
        { label: 'consistency:fix', phase: 'Consistency', schema: DOC_RESULT, agentType: 'documenter', model: MODEL }),
      'consistency:fix', 'Consistency')
    const fr = await reviewFailureFix(fx.tf, 'consistency', 'Consistency')
    cfix = { doc: fx.doc, testFix: fx.tf, testFixReview: fr }
  }
  consistency = { review: crev, fix: cfix }
}

// --- Records -----------------------------------------------------------------
let records = null
if (!SKIP.has('records')) {
  phase('Records')
  const ms = messageSummary(messageResults)
  const d4 = [
    ...testSummary(testResults).flatMap((t) => t.needs_structured_field),
    ...ms.flatMap((m) => m.test_fixes.flatMap((x) => arr(x.needs_structured_field))),
    ...ms.flatMap((m) => m.test_fix_reviews.flatMap((x) => x.fix_needs_structured_field)),
    ...arr(consistency && consistency.fix && consistency.fix.testFix && consistency.fix.testFix.needs_structured_field),
    ...arr(consistency && consistency.fix && consistency.fix.testFixReview && consistency.fix.testFixReview.fix && consistency.fix.testFixReview.fix.needs_structured_field),
  ]
  const recPrompt = (prior, findings) => `${BASE}
${writer()}

YOUR TASK: the records step, by the plan's section 4.3 and decisions D7 and D8.${prior ? ' An earlier records agent worked on this; apply the review findings below.' : ''}
Quotations of old messages in dev-docs that the documenters left unchanged:
${J(ms.flatMap((m) => m.devdocs_quotes_left))}
Whether a documenter regenerated the worked example of dev-docs/serialize_schema.md: ${ms.some((m) => m.regenerated_worked_example)}.
Number of tests that lost a case distinction for lack of a structured field (the list goes to the handover, not to the records): ${d4.length}.
${prior ? `\nEarlier result:\n${J(prior)}\nFindings:\n${J(findings)}\nReport in your result only what you changed in this pass.\n` : startLine('the records step')}
Return the structured result.`
  const rec = await serial(() => agent(recPrompt(null, null), { label: 'records:write', phase: 'Records', schema: SIMPLE_RESULT, model: MODEL }))
  let recRev = null
  let recFix = null
  if (rec && arr(rec.commits).length) {
    recRev = await agent(`${BASE}
${READONLY}

YOUR TASK: review the records step against the plan's section 4.3 and decisions D7 and D8, and against the maintenance rules of dev-docs/DESIGN_RATIONALE.md ("How to use and maintain this document": entry template with Status, Rejected alternatives and Revisit if; no dates outside reversal notes; every entry referenced from ARCHITECTURE; label unique) and Documentation_Structure.md. Check that Philippe's words are quoted exactly as the plan's section 1 gives them.
The result:
${J(rec)}`, { label: 'records:review', phase: 'Records', schema: REVIEW, model: MODEL })
    const rfind = recRev ? arr(recRev.findings).filter((f) => f.severity !== 'nit') : []
    if (rfind.length) {
      recFix = await serial(() => agent(recPrompt(rec, rfind), { label: 'records:fix', phase: 'Records', schema: SIMPLE_RESULT, model: MODEL }))
    }
  }
  records = { rec, recRev, recFix, d4_count: d4.length }
}

// --- Checks ------------------------------------------------------------------
let checks = null
if (!SKIP.has('checks')) {
  phase('Checks')
  const checkPrompt = (round) => `${BASE}
${writer()}

YOUR TASK: run the continuous-integration set of the plan's section 4.4${round === 2 ? ' again, after a fix round' : ''}, one command at a time, by the cargo rules of section 5. Do not fix anything and do not commit.
The full baseline of the preflight (every command it ran, whether it passed, and its failures; a command absent from this list did not run in the baseline):
${J(BASELINE_ALL)}
For each failure, set preexisting to true when the same command failed in the same way in the baseline, or when the command is absent from the baseline and \`git -C ${CK} log ${START}..HEAD -- <file of the failure>\` is empty. A failure of a command that passed in the baseline is never preexisting. Classify each failure; a test that fails only because it compares the wording of a library text is kind "wording-test". Report each optional command that cannot run here in not_run, with the reason.`
  const c1 = await serial(() => agent(checkPrompt(1), { label: 'checks:run-1', phase: 'Checks', schema: CHECKS, model: MODEL }))
  let cfix = null
  let c2 = null
  const newFailures = c1 ? arr(c1.failures).filter((f) => !f.preexisting) : []
  if (newFailures.length) {
    cfix = await serial(() => agent(`${BASE}
${writer()}

YOUR TASK: fix the failures of the continuous-integration set listed below, with the smallest change that keeps the build's intent. A failure of kind "wording-test" is handled by the plan's section 4.1 (reassess the test; never restore a wording assertion). A documentation or clippy failure in text that the build changed is fixed in that text, by the rules of ${AGENTDEF}. Do not change behavior or public API. If a fix would need a design decision, do not make it: list it in open_questions.
Failures:
${J(newFailures)}
Return the structured result.`, { label: 'checks:fix', phase: 'Checks', schema: SIMPLE_RESULT, model: MODEL }))
    c2 = await serial(() => agent(checkPrompt(2), { label: 'checks:run-2', phase: 'Checks', schema: CHECKS, model: MODEL }))
  }
  checks = { first: c1, fix: cfix, second: c2 }
}

// --- Handover ----------------------------------------------------------------
phase('Handover')
const handover = await serial(() => agent(`${BASE}
${writer()}

YOUR TASK: write ${CK}/dev-docs/msgbuild/handover.md by the plan's section 4.5, from the results below, from \`git -C ${CK} log --oneline ${START}..HEAD\` and from \`git -C ${CK} stash list\`, then commit it.
Preflight: ${J(pre)}
Tests work package: ${J(testSummary(testResults))}
Tests sweep, final: ${J(testSweepFinal)}
Messages work package: ${J(messageSummary(messageResults))}
Messages sweep, final: ${J(msgSweepFinal)}
Consistency: ${J(consistency)}
Records: ${J(records)}
Checks: ${J(checks)}
Skipped phases: ${J([...SKIP])}`,
  { label: 'handover', phase: 'Handover', schema: HANDOVER, model: MODEL }))

const failedUnits = [...testResults, ...messageResults].filter((r) => r.error).map((r) => r.unit.key)
const finalChecks = checks ? (checks.second || checks.first) : null
return {
  status: !handover ? 'handover-missing' : (failedUnits.length ? 'complete-with-errors' : 'complete'),
  start_commit: START,
  failed_units: failedUnits,
  handover,
  checks_all_passed: finalChecks ? finalChecks.all_passed : null,
  checks_not_run: finalChecks ? finalChecks.not_run : null,
  units: {
    tests: testSummary(testResults).map((t) => ({ unit: t.unit, status: t.status, commits: t.commits.length })),
    messages: messageSummary(messageResults).map((m) => ({ unit: m.unit, status: m.status, commits: m.commits.length })),
  },
}
