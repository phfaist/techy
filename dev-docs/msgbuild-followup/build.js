export const meta = {
  name: 'techy-msgbuild-followup',
  description: 'techy follow-up build: add the decided error-source fields, fix the decided bugs, add the decided tests, record, check, hand over',
  whenToUse: 'Run by the supervisor session of dev-docs/msgbuild-followup/PLAN.md, in the checkout of a cloud container, once Philippe has decided the decisions of the selected units',
  phases: [
    { title: 'Preflight', detail: 'checkout state, disk, tools, status of every decision, baseline of the continuous-integration set' },
    { title: 'Units', detail: 'per unit: implementer, documenter, reviewer, fixer; one writer at a time, at most two agents at once' },
    { title: 'Records', detail: 'design-rationale entry, ARCHITECTURE reference; review' },
    { title: 'Checks', detail: 'continuous-integration set; one fix round; re-run' },
    { title: 'Handover', detail: 'write and commit dev-docs/msgbuild-followup/handover.md' },
  ],
}

// ---------------------------------------------------------------------------
// Arguments (PLAN.md section 6). Rulings are NOT passed here: they are the
// "Status:" lines and the amendments of PLAN.md section 2, which every agent
// reads, so that prompts stay unchanged and a resume keeps its cache.
// ---------------------------------------------------------------------------
const A = args || {}
if (!A.checkout || !A.branch) {
  throw new Error('args.checkout (absolute path of the checkout) and args.branch are required')
}
const CK = A.checkout
const BR = A.branch
const SKIP = new Set(A.skip || [])
const MODEL = 'opus' // standing ruling S3: every agent runs on Opus
const PLAN = `${CK}/dev-docs/msgbuild-followup/PLAN.md`
const HANDOVER_FILE = `${CK}/dev-docs/msgbuild-followup/handover.md`
const AGENTDEF = `${CK}/.claude/agents/documenter.md`
const LOGS = 'target/msgbuild-followup-logs'
const STASH = 'msgbuild-followup leftover'

// Work units (PLAN.md section 4.1). Each unit executes its decisions as ruled
// in the plan. `texts`: a documenter writes the rustdoc and the message texts
// after the implementer. `last`: the unit runs after every other unit.
const DEFAULT_UNITS = [
  { key: 'F1', title: 'The kind of an implementation error', decisions: ['F1'], texts: true },
  { key: 'F2', title: 'The limit of a descent refusal', decisions: ['F2'], texts: true },
  { key: 'F3', title: 'The kind of a resolve error', decisions: ['F3'], texts: true },
  { key: 'B2', title: 'The reference of include-chain errors', decisions: ['B2'], texts: true },
  { key: 'F6', title: 'The annotation type mismatch', decisions: ['F6'], texts: true },
  { key: 'B1', title: 'The table-full bound', decisions: ['B1'], texts: true },
  { key: 'B3', title: 'The content-parent mapping', decisions: ['B3'], texts: false },
  { key: 'B4', title: 'The placeholder command', decisions: ['B4'], texts: true },
  { key: 'TS', title: 'The tests', decisions: ['T1', 'T2', 'T3'], texts: false, last: true },
]

function selectUnits() {
  const extra = Array.isArray(A.extra_units) ? A.extra_units : []
  for (const u of extra) {
    if (!u || !u.key || !u.title || !Array.isArray(u.decisions) || !u.decisions.length) {
      throw new Error('each entry of args.extra_units needs key, title and a non-empty decisions array')
    }
  }
  const all = [...DEFAULT_UNITS, ...extra]
  const keys = all.map((u) => u.key)
  const dup = keys.filter((k, i) => keys.indexOf(k) !== i)
  if (dup.length) throw new Error(`duplicate unit keys: ${dup.join(', ')}`)
  let chosen = all
  if (Array.isArray(A.units)) {
    const unknown = A.units.filter((k) => !keys.includes(k))
    if (unknown.length) throw new Error(`args.units names unknown units: ${unknown.join(', ')} (known: ${keys.join(', ')})`)
    chosen = all.filter((u) => A.units.includes(u.key))
  }
  // Fixed order: the listed order, with the units marked `last` at the end.
  return [...chosen.filter((u) => !u.last), ...chosen.filter((u) => u.last)]
}
const UNITS = selectUnits()

// ---------------------------------------------------------------------------
// One writer at a time. The schedule below already starts at most one writing
// agent at a time; serial() makes the write turn explicit and guards a later
// edit of the schedule. serial() is never called from inside serial().
// ---------------------------------------------------------------------------
let writeTail = Promise.resolve()
function serial(fn) {
  const run = writeTail.then(() => fn())
  writeTail = run.then(() => undefined, () => undefined)
  return run
}

const J = (x) => JSON.stringify(x, null, 1)
const arr = (x) => (Array.isArray(x) ? x : [])

// ---------------------------------------------------------------------------
// A missing result stops the run (PLAN.md section 6). STOP is set by the first
// agent that returns nothing or whose call fails; from then on no agent is
// started. The agent that may still be running finishes, then the script
// throws, naming the step, so that a resume continues from that step.
// ---------------------------------------------------------------------------
let STOP = null
function stopMessage() {
  return `RUN STOPPED at step "${STOP.step}": ${STOP.reason}. No agent was started after it. Find the cause (usage limit, container restart), stash the uncommitted changes of a stopped writer (git -C ${CK} stash push -u -m "${STASH}"), then resume with the same run id and exactly the same args (PLAN.md section 6).`
}
function stopAt(name, reason) {
  if (!STOP) {
    STOP = { step: name, reason }
    log(stopMessage())
  }
  return new Error(stopMessage())
}
async function step(name, prompt, opts) {
  if (STOP) throw new Error(`step ${name} was not started: ${stopMessage()}`)
  let r = null
  try {
    r = await agent(prompt, { ...opts, label: name, model: MODEL })
  } catch (e) {
    throw stopAt(name, `the agent call failed (${e && e.message ? e.message : e})`)
  }
  if (r === null || r === undefined) {
    throw stopAt(name, 'the agent returned no result')
  }
  return r
}
function throwIfStopped() {
  if (STOP) throw new Error(stopMessage())
}

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

// ---------------------------------------------------------------------------
// Schemas
// ---------------------------------------------------------------------------
const STR_ARR = { type: 'array', items: { type: 'string' } }

const DECISION_STATUS = {
  type: 'array',
  items: {
    type: 'object',
    properties: {
      id: { type: 'string', description: 'the decision id, e.g. F1, B2, T3' },
      status: { type: 'string', enum: ['DECIDED', 'PROPOSED', 'NOT-FOUND'] },
      status_line: { type: 'string', description: 'the "Status:" line of the decision, copied exactly' },
    },
    required: ['id', 'status', 'status_line'],
  },
}

const PREFLIGHT = {
  type: 'object',
  properties: {
    ok: { type: 'boolean' },
    start_commit: { type: 'string' },
    branch_seen: { type: 'string' },
    disk_free: { type: 'string' },
    tools: { type: 'string', description: 'cargo/rustc versions; installed targets; toolchain 1.86.0; cargo-semver-checks; api-baseline branch; the proptest requirement in techy/Cargo.toml' },
    decisions: DECISION_STATUS,
    baseline: { type: 'array', items: { type: 'object', properties: {
      command: { type: 'string' }, passed: { type: 'boolean' },
      failures: { ...STR_ARR, description: 'failing tests, or "file:line: lint/warning" entries, or the error' },
    }, required: ['command', 'passed', 'failures'] } },
    problems: STR_ARR,
  },
  required: ['ok', 'start_commit', 'decisions', 'baseline', 'problems'],
}

const UNIT_RESULT = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['done', 'partial', 'blocked', 'not-needed'] },
    decisions_read: DECISION_STATUS,
    commits: STR_ARR,
    summary: { type: 'string' },
    api_changes: { type: 'array', items: { type: 'object', properties: {
      item: { type: 'string' }, change: { type: 'string' }, breaking: { type: 'boolean' },
    }, required: ['item', 'change', 'breaking'] } },
    serialized_data_changes: STR_ARR,
    behavior_changes: STR_ARR,
    tests_restored: { type: 'array', items: { type: 'object', properties: {
      file: { type: 'string' }, test: { type: 'string' }, now_checks: { type: 'string' },
    }, required: ['file', 'test', 'now_checks'] } },
    tests_added: { type: 'array', description: 'new test functions only (standing ruling S6)', items: { type: 'object', properties: {
      file: { type: 'string' }, test: { type: 'string' },
      purpose: { type: 'string', description: 'one sentence: the behavior that the test guards' },
      checks: { type: 'string' },
    }, required: ['file', 'test', 'purpose', 'checks'] } },
    tests_changed_or_dropped: { type: 'array', items: { type: 'object', properties: {
      file: { type: 'string' }, test: { type: 'string' }, action: { type: 'string' }, reason: { type: 'string' },
    }, required: ['file', 'test', 'action', 'reason'] } },
    classification_notes: { ...STR_ARR, description: 'places whose classification was not obvious, and places left unclassified' },
    goldens_regenerated: { type: 'boolean' },
    open_questions: { ...STR_ARR, description: 'design questions (plan S4): context, options, recommendation' },
    leftovers_stashed: STR_ARR,
    checks: STR_ARR,
    remaining: { type: 'string' },
    notes: { type: 'string' },
  },
  required: ['status', 'decisions_read', 'commits', 'summary', 'checks'],
}

const DOC_RESULT = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    commits: STR_ARR,
    items_written: STR_ARR,
    contradictions: STR_ARR,
    proposals: STR_ARR,
    unsure_statements: STR_ARR,
    open_questions: { ...STR_ARR, description: 'design questions (plan S4): context, options, recommendation' },
    leftovers_stashed: STR_ARR,
    checks: STR_ARR,
    wording_test_failures: { type: 'array', items: { type: 'object', properties: {
      file: { type: 'string' }, test: { type: 'string' }, detail: { type: 'string' },
    }, required: ['file', 'test'] } },
    regenerated_worked_example: { type: 'boolean' },
    remaining: { type: 'string' },
    notes: { type: 'string' },
  },
  required: ['status', 'commits', 'items_written', 'checks', 'wording_test_failures'],
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
    breaking_changes: { ...STR_ARR, description: 'each change of the public API that you judge breaking' },
    design_questions: STR_ARR,
    notes: { type: 'string' },
  },
  required: ['verdict', 'findings', 'design_questions'],
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
// Prompt pieces. No prompt states a ruling: agents read them in the plan.
// ---------------------------------------------------------------------------
const BASE = `You are a build agent of the techy "msgbuild-followup" build, running in a cloud container.
Checkout: ${CK} (branch ${BR}). Plan: ${PLAN}.
Before your first command, read in the plan: the opening of section 2 ("Decisions"), its standing rulings S1 to S6, its "Amendments" at the end, the decisions that this prompt names (in full), and section 5 ("Rules for every build agent"). Follow them. Read other plan sections when this prompt points you to them.
Your turn ends only with your structured result. Never start a command in the background, and never end your turn to wait for one: that ends the build. Run cargo by the rule "Cargo" of section 5 (foreground, maximum timeout, log under ${LOGS}/, exit status recorded).`

const READONLY = `You are READ-ONLY: do not edit files, do not run cargo, do not commit. Another agent may be editing the working tree while you work, so read changes with \`git -C ${CK} show <sha>\` and files as committed with \`git -C ${CK} show HEAD:<path>\` (or at a given commit).`

// Set after the preflight.
let START = null
let BASELINE_FAILING = []
let BASELINE_ALL = []

function writer() {
  return `You hold the write turn: you are the only agent that edits files, runs cargo or commits right now. Do not push, do not switch branches.
Begin with \`git -C ${CK} status --porcelain\`. If the tree has changes that you did not make (left by an agent that was stopped), stash them with \`git -C ${CK} stash push -u -m "${STASH}"\`, list them in leftovers_stashed, and continue on the clean tree. End with a clean tree: commit your work or revert it.
Failures that existed before the build (the preflight's baseline): ${J(BASELINE_FAILING)}. Do not fix them and do not count them as yours; report one only if your change touches it.`
}

function startLine(kind) {
  return `The build's start commit is ${START}. \`git -C ${CK} log --oneline ${START}..HEAD\` shows the commits of other units as well. A commit that does exactly your task (${kind}) may come from an interrupted earlier run of this same task: if there is one, continue from the current state, and include the work of those commits in your result (read it from their diffs) as if you had done it.`
}

function decisionList(u) {
  return u.decisions.length === 1 ? `decision ${u.decisions[0]}` : `decisions ${u.decisions.join(', ')}`
}

function implementPrompt(u, prior) {
  return `${BASE}
${writer()}

YOUR TASK: unit ${u.key} ("${u.title}"): execute ${decisionList(u)} of the plan's section 2, exactly as ruled there, by the plan's sections 4.1 and 4.2.
1. Read the "Status:" line of each of these decisions and copy it into decisions_read. If one does not read "DECIDED", change nothing and return status "blocked".
2. The option that a Status line names is the ruling for that decision, together with any amendment that concerns it. If every ruled option needs no work (for instance "do not add", "keep", "leave as is", "no test"), change nothing and return status "not-needed".
3. Otherwise do the work that the decision and section 4.2 state for the ruled option: names, shape, variants, places and tests exactly as written. If an amendment of section 2 defines this unit, it states the work.
4. Do not decide what the decision leaves open: keep the existing state there and put the question into open_questions (standing ruling S4).
${startLine(`the work of unit ${u.key}`)}
${prior ? `\nAn earlier agent returned status "partial" for this unit. Its result:\n${J(prior)}\nContinue the remaining work.\n` : ''}
Return the structured result. In api_changes, list every change of the public API and say whether it is breaking (a new public field of a struct that is not #[non_exhaustive], a new field of an enum variant, a changed signature or a removed item are breaking). In tests_added, list each new test function with its purpose in one sentence (standing ruling S6); an assertion added to an existing test belongs in tests_restored.`
}

function documentPrompt(u, impl) {
  return `${BASE}
${writer()}

YOUR TASK: new documentation and message texts for unit ${u.key} ("${u.title}"), by your instructions and the plan's standing rulings S1 and S5. The decisions of the unit are ${u.decisions.join(', ')}.
An implementer executed them. Its result:
${J(impl)}
Read each of its commits with \`git -C ${CK} show <sha>\`. Then write, by your rules:
- the rustdoc of every item that the commits added or changed (types, variants, fields, methods), and of each condition or error type whose data changed; the implementer's texts are a first version, so replace them;
- every message text and Display text that the commits added or changed;
- the guide text in docs/ (not the AI guides) that describes an item whose signature or data changed.
The names, the shape and the data are fixed by the decision: do not change behavior, signatures, names, identifiers, data fields, tests or goldens. If a name seems wrong to you, say so in proposals. If a test fails because it compares wording, do not change it: list it in wording_test_failures.
${startLine(`the documentation of unit ${u.key}`)}
Return the structured result; its fields follow the list "Your result" of your instructions. Put design questions (the plan's S4) in open_questions, with context, options and a recommendation.`
}

function wordingFixPrompt(u, failures) {
  return `${BASE}
${writer()}

YOUR TASK: reassess exactly the following tests of unit ${u.key}, by the plan's standing rulings S2 and S6. They fail because the documenter of the unit changed the wording of a library text, and they compare that wording. Rewrite each to check the behavior that matters (identifier or type of the condition, its fields, span, frames, the node tree), or drop it if it checked only wording. Do not change any message text. Do not touch other tests.
Failing tests:
${J(failures)}
Return the structured result (list the tests in tests_changed_or_dropped).`
}

function reviewPrompt(r) {
  const u = r.unit
  return `${BASE}
${READONLY}

YOUR TASK: review unit ${u.key} ("${u.title}") against ${decisionList(u)} of the plan's section 2 as ruled, and against the plan's sections 4.1 and 4.2.
The implementer's result:
${J(r.impl)}
${r.doc ? `The documenter's result:\n${J(r.doc)}\n` : ''}${r.wordingFix ? `The result of the agent that reassessed tests after the documenter:\n${J(r.wordingFix)}\n` : ''}
Read each commit of these results. Check:
1. Fidelity. Read the "Status:" lines yourself. The names, the shape, the variants, the places and their classification are exactly those of the ruled option; nothing of another option was done; nothing beyond the decision was done. A deviation is blocking, unless the implementer kept the existing state and reported a design question.
2. Correctness. The code does what the decision states at every place it names (check the places against the decision's table or list, at HEAD). No behavior changes beyond the decision. A violated requirement returns an error and does not panic. Nothing needs a Rust version above 1.86.
3. Public API. List in breaking_changes every change that you judge breaking, and compare with the implementer's api_changes and with the decision's "Breaking" statement. A difference is a finding.
4. Tests. Each test that the decision lists regains its distinction and checks data, not wording. The new tests are those that section 4.2 states, and each passes standing ruling S6: one sentence states its purpose, the purpose is a behavior that matters for the function of the library, no existing test guards it, and it can break without a test failing. A new test without such a purpose, a near-duplicate of another test, or a new test where an assertion in an existing test suffices, is a should-fix finding (the fix: remove or merge it). A behavior that the decision changes and that no test guards is a finding too. No test was weakened or dropped without being listed. No test compares the wording of a library text (S2; grep the changed test code).
5. Texts. First read ${AGENTDEF}, Parts 2 and 3. Check every message, Display text and rustdoc block that the commits added or changed against these rules and against S1 and S5.
6. Serialized data and goldens. Data keys only grow. A golden or the worked example of dev-docs/serialize_schema.md changed only if the data of a diagnostic that it records changed.
7. Scope. No rename, no reformatting, no unrelated change, no change of Cargo.toml or Cargo.lock.
Severity: blocking (a deviation from the decision, a wrong behavior, lost coverage, a false statement), should-fix (a rule broken), nit (taste). Quote the code or text in "issue" and give the correction in "fix". A point that needs Philippe's decision goes into design_questions, not into findings.`
}

function fixPrompt(r, findings) {
  const u = r.unit
  return `${BASE}
${writer()}

YOUR TASK: apply review findings to unit ${u.key} ("${u.title}"), which executes ${decisionList(u)} of the plan's section 2 as ruled. Apply the findings of severity "blocking" and "should-fix". If a finding is wrong (it contradicts the code or the plan), do not apply it; say why in notes. Do not act on design questions: list them in open_questions. For a finding on a message, a Display text or rustdoc, first read ${AGENTDEF}, Parts 2 and 3, and write the text by those rules.
The implementer's result:
${J(r.impl)}
${r.doc ? `The documenter's result:\n${J(r.doc)}\n` : ''}The findings:
${J(findings)}
Return the structured result (report only what you changed in this pass).`
}

// ---------------------------------------------------------------------------
// One unit. The implementer, the documenter and the reassessment of tests that
// fail on wording run in ONE critical section: the next writer starts green.
// ---------------------------------------------------------------------------
async function writeUnit(u) {
  return serial(async () => {
    const opts = { phase: 'Units', schema: UNIT_RESULT }
    let impl = await step(`${u.key}:implement`, implementPrompt(u, null), opts)
    if (impl.status === 'partial') {
      const more = await step(`${u.key}:implement:cont`, implementPrompt(u, impl), opts)
      impl = mergeResults(impl, more)
    }
    const r = { unit: u, impl, doc: null, wordingFix: null, rev: null, fix: null }
    const worked = impl.status !== 'blocked' && impl.status !== 'not-needed' && arr(impl.commits).length > 0
    if (u.texts && worked) {
      r.doc = await step(`${u.key}:document`, documentPrompt(u, impl),
        { phase: 'Units', schema: DOC_RESULT, agentType: 'documenter' })
      if (arr(r.doc.wording_test_failures).length) {
        r.wordingFix = await step(`${u.key}:document:test-fix`, wordingFixPrompt(u, r.doc.wording_test_failures), opts)
      }
    }
    return r
  })
}

function needsReview(r) {
  return arr(r.impl.commits).length > 0 || (r.doc && arr(r.doc.commits).length > 0)
}

function reviewUnit(r) {
  return step(`${r.unit.key}:review`, reviewPrompt(r), { phase: 'Units', schema: REVIEW })
}

async function fixUnit(r) {
  const findings = arr(r.rev && r.rev.findings).filter((f) => f.severity !== 'nit')
  if (!findings.length) return
  r.fix = await serial(() => step(`${r.unit.key}:fix`, fixPrompt(r, findings), { phase: 'Units', schema: UNIT_RESULT }))
}

function unitSummary(results) {
  return results.map((r) => {
    const parts = [r.impl, r.wordingFix, r.fix].filter(Boolean)
    const cat = (k) => parts.flatMap((p) => arr(p[k]))
    const findings = arr(r.rev && r.rev.findings).filter((f) => f.severity !== 'nit')
    return {
      unit: r.unit.key, title: r.unit.title, decisions: r.unit.decisions,
      status: r.impl.status,
      decisions_read: arr(r.impl.decisions_read),
      commits: [...cat('commits'), ...arr(r.doc && r.doc.commits)],
      summary: r.impl.summary,
      api_changes: cat('api_changes'),
      serialized_data_changes: cat('serialized_data_changes'),
      behavior_changes: cat('behavior_changes'),
      tests_restored: cat('tests_restored'), tests_added: cat('tests_added'),
      tests_changed_or_dropped: cat('tests_changed_or_dropped'),
      classification_notes: cat('classification_notes'),
      goldens_regenerated: parts.some((p) => p.goldens_regenerated === true),
      open_questions: [...cat('open_questions'), ...arr(r.doc && r.doc.open_questions)],
      documenter: r.doc ? {
        status: r.doc.status, items_written: arr(r.doc.items_written), proposals: arr(r.doc.proposals),
        contradictions: arr(r.doc.contradictions), unsure_statements: arr(r.doc.unsure_statements),
        regenerated_worked_example: r.doc.regenerated_worked_example === true, notes: r.doc.notes,
      } : null,
      review: r.rev ? {
        verdict: r.rev.verdict, breaking_changes: arr(r.rev.breaking_changes),
        design_questions: arr(r.rev.design_questions), notes: r.rev.notes,
      } : null,
      fix_status: r.fix ? r.fix.status : null,
      fix_notes: r.fix ? r.fix.notes : null,
      unresolved_findings: findings.length && (!r.fix || r.fix.status !== 'done') ? findings : [],
      leftovers_stashed: [...cat('leftovers_stashed'), ...arr(r.doc && r.doc.leftovers_stashed)],
      remaining: r.impl.remaining || null,
      notes: r.impl.notes || null,
    }
  })
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------
phase('Preflight')
const baselineRev = A.start_commit || 'HEAD'
const neededDecisions = [...new Set(UNITS.flatMap((u) => u.decisions))]
const pre = await step('preflight', `${BASE}
You are the first agent of the build; nobody else runs while you work. Do not push. Do not change any file of the checkout.

YOUR TASK: preflight of the build, by the plan's section 6 ("Preflight").
1. Confirm that ${CK} is a clean checkout on branch ${BR} (git status, git branch --show-current), and that ${PLAN} and ${AGENTDEF} exist and are tracked in git. Report \`git -C ${CK} rev-parse ${baselineRev}\` as start_commit.
2. Check the free disk (df -h) by the rule of section 5.
3. Record in "tools": the versions of cargo and rustc; the installed targets (rustup target list --installed); whether the toolchain 1.86.0 is installed (rustup toolchain list); whether cargo-semver-checks is installed; whether a branch api-baseline exists locally or on origin; the proptest requirement in techy/Cargo.toml. Install nothing.
4. Read the plan's section 2 in full. For EVERY decision of that section (every heading with an id such as F1, B2, T3, and every decision that an amendment defines), report its id, its "Status:" line copied exactly, and status "DECIDED" if that line begins with "Status: DECIDED", otherwise "PROPOSED". These decisions must be in your list (report "NOT-FOUND" for one that the plan does not contain): ${neededDecisions.join(', ')}.
5. Run the baseline, by the cargo rule of section 5, at ${baselineRev === 'HEAD' ? 'HEAD, in the checkout itself' : `the commit ${baselineRev}, in a detached worktree (\`git -C ${CK} worktree add --detach ${CK}/target/msgbuild-followup-baseline ${baselineRev}\`; remove it afterwards with \`git -C ${CK} worktree remove --force ${CK}/target/msgbuild-followup-baseline\`)`}: every command of the plan's section 4.4 that can run here. For each command record whether it passed and its failures (failing test names; "file:line: lint" for clippy and documentation warnings; the error otherwise).
Set ok to false only if the checkout is not usable (wrong branch, uncommitted changes, plan or agent definition missing, disk too low, the workspace does not compile). Failures of the baseline and pending decisions do not make ok false: the workflow checks the decisions itself.`,
  { phase: 'Preflight', schema: PREFLIGHT })

if (!pre.ok) {
  return { status: 'halted', stage: 'preflight', reason: 'the checkout is not usable', problems: arr(pre.problems), preflight: pre,
    next: 'Remove the cause, then launch a FRESH run (PLAN.md section 6, "A fresh run after a halt").' }
}
const statusOf = {}
for (const d of arr(pre.decisions)) statusOf[d.id] = d
const pending = []
for (const u of UNITS) {
  for (const id of u.decisions) {
    const d = statusOf[id]
    if (!d || d.status !== 'DECIDED') pending.push({ unit: u.key, decision: id, status_line: d ? d.status_line : 'not found in the plan' })
  }
}
if (pending.length) {
  log(`halted: ${pending.length} decision(s) of the selected units are not DECIDED: ${pending.map((p) => `${p.decision} (unit ${p.unit})`).join(', ')}`)
  return { status: 'halted', stage: 'decisions', reason: 'a selected unit depends on a decision that is not DECIDED', pending, decisions: arr(pre.decisions),
    next: 'Ask Philippe, write each ruling into the Status line of its decision, commit the plan, then launch a FRESH run; or select only the units whose decisions are DECIDED (args.units).' }
}
START = pre.start_commit
BASELINE_ALL = arr(pre.baseline).map((b) => ({ command: b.command, passed: b.passed, failures: arr(b.failures) }))
BASELINE_FAILING = BASELINE_ALL.filter((b) => !b.passed).map((b) => ({ command: b.command, failures: b.failures }))
log(`start commit ${START}; units ${UNITS.map((u) => u.key).join(', ')}; baseline: ${BASELINE_FAILING.length ? `${BASELINE_FAILING.length} command(s) fail before the build` : 'all pass'}`)

// --- Units -------------------------------------------------------------------
// Fixed schedule, independent of how long an agent takes, so that a resumed
// run issues the same agent calls in the same order:
//   write(1); [review(1) ∥ write(2)]; fix(1); [review(2) ∥ write(3)]; fix(2); …; review(n); fix(n)
// write(k) = implementer (+ continuation) + documenter (+ test reassessment).
// At most two agents run at once: one writer and one read-only reviewer.
phase('Units')
const results = []
let blocked = null
let prev = null
for (const u of UNITS) {
  const thunks = []
  if (prev) thunks.push(() => reviewUnit(prev))
  thunks.push(() => writeUnit(u))
  const out = await parallel(thunks)
  throwIfStopped()
  if (out.some((x) => x === null || x === undefined)) {
    throw new Error(`unit ${u.key}: a step failed without a recorded cause (a defect of the workflow script); nothing was started after it`)
  }
  if (prev) {
    prev.rev = out[0]
    await fixUnit(prev)
    log(`unit ${prev.unit.key}: ${prev.impl.status}; review ${prev.rev.verdict}${prev.fix ? `; fix ${prev.fix.status}` : ''}`)
  }
  const cur = out[out.length - 1]
  results.push(cur)
  if (cur.impl.status === 'blocked') {
    blocked = cur
    prev = null
    break
  }
  if (needsReview(cur)) {
    prev = cur
  } else {
    prev = null
    log(`unit ${u.key}: ${cur.impl.status}; no commits, no review`)
  }
}
if (prev) {
  prev.rev = await reviewUnit(prev)
  await fixUnit(prev)
  log(`unit ${prev.unit.key}: ${prev.impl.status}; review ${prev.rev.verdict}${prev.fix ? `; fix ${prev.fix.status}` : ''}`)
}
if (blocked) {
  log(`halted: unit ${blocked.unit.key} returned "blocked"`)
  return { status: 'halted', stage: `unit ${blocked.unit.key}`, reason: 'the unit returned "blocked"',
    blocked: { unit: blocked.unit.key, decisions_read: arr(blocked.impl.decisions_read), summary: blocked.impl.summary, notes: blocked.impl.notes },
    start_commit: START, units: unitSummary(results),
    next: `Remove the cause, then launch a FRESH run with args.units naming the units that remain and args.start_commit "${START}" (PLAN.md section 6).` }
}

// --- Records -----------------------------------------------------------------
let records = null
if (!SKIP.has('records')) {
  phase('Records')
  const recPrompt = (prior, findings) => `${BASE}
${writer()}

YOUR TASK: the records step, by the plan's section 4.3.${prior ? ' An earlier records agent worked on this; apply the review findings below.' : ''} Read the whole of the plan's section 2: you record every decision whose Status line reads "DECIDED", with the ruled option and Philippe's words where the plan gives them, and none that reads "PROPOSED".
Results of the units (what was built, the API changes, the serialized data changes):
${J(unitSummary(results))}
${prior ? `\nEarlier result:\n${J(prior)}\nFindings:\n${J(findings)}\nReport in your result only what you changed in this pass.\n` : startLine('the records step')}
Return the structured result.`
  const rec = await serial(() => step('records:write', recPrompt(null, null), { phase: 'Records', schema: SIMPLE_RESULT }))
  let recRev = null
  let recFix = null
  if (arr(rec.commits).length) {
    recRev = await step('records:review', `${BASE}
${READONLY}

YOUR TASK: review the records step against the plan's section 4.3, against the decisions of section 2 as ruled, and against the maintenance rules of dev-docs/DESIGN_RATIONALE.md ("How to use and maintain this document": entry template with Status, Rejected alternatives and Revisit if; no dates outside reversal notes; no reference to a plan or a build; every entry referenced from ARCHITECTURE; label unique). Check that the entry states what the code does at HEAD (names, variants, fields), that Philippe's words are quoted exactly as the plan gives them, and that no decision that is still PROPOSED is recorded as decided.
The result:
${J(rec)}`, { phase: 'Records', schema: REVIEW })
    const rfind = arr(recRev.findings).filter((f) => f.severity !== 'nit')
    if (rfind.length) {
      recFix = await serial(() => step('records:fix', recPrompt(rec, rfind), { phase: 'Records', schema: SIMPLE_RESULT }))
    }
  }
  records = { rec, recRev, recFix }
}

// --- Checks ------------------------------------------------------------------
let checks = null
if (!SKIP.has('checks')) {
  phase('Checks')
  const checkPrompt = (round) => `${BASE}
${writer()}

YOUR TASK: run the continuous-integration set of the plan's section 4.4${round === 2 ? ' again, after a fix round' : ''}, one command at a time, by the cargo rule of section 5. Do not fix anything and do not commit.
The full baseline of the preflight (every command it ran, whether it passed, and its failures):
${J(BASELINE_ALL)}
For each failure, set preexisting to true only when the same command failed in the same way in the baseline. Classify each failure; a test that fails only because it compares the wording of a library text is kind "wording-test". Report each command that cannot run here in not_run, with the reason (the semver check: no branch api-baseline).`
  const c1 = await serial(() => step('checks:run-1', checkPrompt(1), { phase: 'Checks', schema: CHECKS }))
  let cfix = null
  let c2 = null
  const newFailures = arr(c1.failures).filter((f) => !f.preexisting)
  if (newFailures.length) {
    cfix = await serial(() => step('checks:fix', `${BASE}
${writer()}

YOUR TASK: fix the failures of the continuous-integration set listed below, with the smallest change that keeps the decisions of the plan's section 2 as ruled. A failure of kind "wording-test" is handled by standing ruling S2 (rewrite the test to check data; never restore a wording assertion). A documentation or clippy failure in text that the build changed is fixed in that text, by the rules of ${AGENTDEF}. Do not change behavior or public API beyond the decisions. If a fix would need a design decision, do not make it: list it in open_questions.
Failures:
${J(newFailures)}
Return the structured result.`, { phase: 'Checks', schema: SIMPLE_RESULT }))
    c2 = await serial(() => step('checks:run-2', checkPrompt(2), { phase: 'Checks', schema: CHECKS }))
  }
  checks = { first: c1, fix: cfix, second: c2 }
}

// --- Handover ----------------------------------------------------------------
phase('Handover')
const summary = unitSummary(results)
const handover = await serial(() => step('handover', `${BASE}
${writer()}

YOUR TASK: write ${HANDOVER_FILE} by the plan's section 4.5, from the results below, from the plan's section 2 (the status of every decision), from \`git -C ${CK} log --oneline ${START}..HEAD\` and from \`git -C ${CK} stash list\`, then commit it. Run no cargo command.
Preflight: ${J(pre)}
Units: ${J(summary)}
Records: ${J(records)}
Checks: ${J(checks)}
Selected units: ${J(UNITS.map((u) => u.key))}
Skipped phases: ${J([...SKIP])}`,
  { phase: 'Handover', schema: HANDOVER }))

const finalChecks = checks ? (checks.second || checks.first) : null
const openItems = summary.filter((s) => s.status === 'partial' || s.unresolved_findings.length || s.open_questions.length || (s.review && s.review.design_questions.length))
return {
  status: openItems.length || (finalChecks && !finalChecks.all_passed) ? 'complete-with-open-items' : 'complete',
  start_commit: START,
  units: summary.map((s) => ({ unit: s.unit, status: s.status, commits: s.commits.length, fix: s.fix_status,
    unresolved_findings: s.unresolved_findings.length, open_questions: s.open_questions.length })),
  breaking_changes: summary.flatMap((s) => s.api_changes.filter((c) => c.breaking).map((c) => `${s.unit}: ${c.item}: ${c.change}`)),
  handover,
  checks_all_passed: finalChecks ? finalChecks.all_passed : null,
  checks_not_run: finalChecks ? finalChecks.not_run : null,
}
