# Claude Conversation Export

- Session: `3ef12e64-c86f-4feb-895d-b9d229ffb041`
- Project: `/Users/philippe/projects/techy/-claude/worktrees/subphase-7-5-latexlike`
- Exported: 2026-07-19T01:18:19

---

**User:**

/code-review

---

**Assistant:**

Starting the xhigh-effort code review: first I'll gather the diff for this branch, then fan out finder agents.

The range diff vs main is empty and the working tree is clean. Let me check the branch state to find the right review scope.

Diff saved (1034 lines). Let me read it to understand the change before launching the finder angles.

All 10 finder angles are now running in parallel — 5 correctness angles (line-by-line scan, removed-behavior audit, cross-file tracing, Rust pitfalls, wrapper/facade correctness) plus reuse, simplification, efficiency, altitude, and CLAUDE.md-conventions angles. Each will return up to 8 candidate findings on the subphase 7.5 latexlike preset diff.

I'll collect their candidates as they complete, then dedupe and run the verification pass (Phase 2), a gap sweep (Phase 3), and report the final ranked findings.

---

**User:**

<task-notification>
<task-id>afea08cd1e6e44713</task-id>
<tool-use-id>toolu_01XU4sAnmscTDjraDhBe8CVa</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/afea08cd1e6e44713.output</output-file>
<status>completed</status>
<summary>Agent "Angle: CLAUDE.md conventions" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>All checks are complete. Verification summary:

- **Governing CLAUDE.md files**: only the repo-root `CLAUDE.md` exists (no `/Users/philippe/.claude/CLAUDE.md`, no `techy/CLAUDE.md`, `techy/CLAUDE.local.md`, `techy/src/CLAUDE.md`, or `techy/src/latexlike/CLAUDE.md`).
- **Panic policy** ("Use Result&lt;T,E&gt; consistently — never panic in lib code on input"; DESIGN_RATIONALE §"Panic policy", line 2448): every `unwrap`/`unwrap_err` in the new files sits inside `#[cfg(test)] mod tests` or the module doctest; the non-test code paths in `driver.rs` (let-else → `Unresolved`, `Ok/Ok(None)/Err` match), `mod.rs` (`initial_state_data`, `default_token_rules`, `base_package`), and `node_ref.rs` (`?` on `group_delimiters`, `Option` returns) contain no panic, `expect`, `unreachable!`, or indexing. Clean.
- **Naming**: shipped names (`Latexlike`, `LatexlikeDriver`, `GroupType::{Content,Math}`, `CallableType::{Macro,Environment,Specials}`, `Mode::{#[default] Text, Math}`, all three `#[non_exhaustive]`, `default_token_rules()`, `base_package()`, `MathStyle::{Inline,Display}`, the five inherent `NodeRef` accessors, no crate-root re-exports) match the updated NAMING_STRATEGY.md preset section exactly. No `Latex`-prefix violations (the `Latexlike*` names are the documented preset names).
- **DESIGN_RATIONALE §3.13 coverage**: all five checkpoint decisions in the Phase7Execution.md 7.5 row (single Math class, no Bracket/`[]` rule, bare module-scoped non-exhaustive enums with no re-exports, `"base"` seed package, inherent NodeRef sugar) have corresponding DECIDED entries in §3.13.
- **Tests**: `LatexlikeDriver::new` (the_default_driver_is_strict), `default_token_rules` (seed test asserts `seed.rules() == &amp;default_token_rules()` plus all parse tests), `base_package` (base_specials/ligature/unload tests plus provider_names check), and all five NodeRef accessors (math_style_reads_the_recorded_delimiters, callable_name_accessors_filter_by_invocation_form; `environment_name` has only negative coverage, but environment nodes cannot be produced until 7.6, and it is asserted in a test) are covered.

No clear violations of any governing CLAUDE.md rule.

[]</result>
<usage><subagent_tokens>56469</subagent_tokens><tool_uses>6</tool_uses><duration_ms>167296</duration_ms></usage>
</task-notification>

---

**Assistant:**

Conventions angle came back clean — no CLAUDE.md violations: the panic policy holds (all unwraps are test-only), shipped names match NAMING_STRATEGY.md's preset section, DESIGN_RATIONALE §3.13 covers all five checkpoint decisions, and the new public API has test coverage. Zero candidates from this angle.

Waiting on the other nine finder angles.

---

**User:**

<task-notification>
<task-id>a5922cd8664fd2204</task-id>
<tool-use-id>toolu_01LCvLRmZVdyExuhg6STesx7</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a5922cd8664fd2204.output</output-file>
<status>failed</status>
<summary>Agent "Angle: reuse check" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>Now let me check the existing codebase for helpers these new files might duplicate.</result>
</task-notification>

---

**User:**

<task-notification>
<task-id>a2d9eec7611fa11db</task-id>
<tool-use-id>toolu_019r46sWTf5YdoVrcdquoKKQ</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a2d9eec7611fa11db.output</output-file>
<status>failed</status>
<summary>Agent "Angle C: cross-file tracer" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>The tokenizer's precedence checks out (prefix table is longest-first regardless of list order; group delimiters are checked before commands, so `\(`/`\[` win over the `\` escape). Now the spec and NodeRef contracts.</result>
</task-notification>

---

**User:**

<task-notification>
<task-id>a4770fc411415cf10</task-id>
<tool-use-id>toolu_012Sq1KEJxEqyDLFUC1azs7b</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a4770fc411415cf10.output</output-file>
<status>failed</status>
<summary>Agent "Angle: efficiency check" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
</task-notification>

---

**User:**

<task-notification>
<task-id>a1e5bdc777b10ebb7</task-id>
<tool-use-id>toolu_01SGJwMsQoiiULCnHoEup7Qu</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a1e5bdc777b10ebb7.output</output-file>
<status>failed</status>
<summary>Agent "Angle D: Rust pitfalls" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
</task-notification>

---

**User:**

<task-notification>
<task-id>a3ed25e8c1a5fc298</task-id>
<tool-use-id>toolu_01Sak8pPsgGcP34yHUEgfE7t</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a3ed25e8c1a5fc298.output</output-file>
<status>failed</status>
<summary>Agent "Angle: simplification check" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
</task-notification>

---

**User:**

<task-notification>
<task-id>ab14b5bc6b33ae44a</task-id>
<tool-use-id>toolu_01XtQpKjPu9JcG3a4oiL5o6k</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/ab14b5bc6b33ae44a.output</output-file>
<status>completed</status>
<summary>Agent "Angle B: removed-behavior audit" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>Audit complete. Every removed ARCHITECTURE §8 promise was traced: `StateExt`/`Event` (explicitly amended), `MACRO`/`ENVIRONMENT`/`SPECIALS` (delivered as CamelCase variants), `[]` group type (explicitly re-decided; `OptionalGroupArgumentParser` + `temporary_groups` verified at `techy/src/constructs/argument_parsers.rs:578`), type aliases (explicitly deferred), standard Library / `\newcommand` (rescheduled to the std spec-database port, which is documented as deferred out of Phase 7 at Phase7Execution.md:36), `as_math()` sugar (delivered renamed), MacroSpec-family/`\begin`/verbatim (7.6–7.7), mode-aware SpecLookup (delivered as `Package::set_visible_modes`). TODO_Big's "LibraryStack arch revisit" was genuinely resolved (DESIGN_RATIONALE.md:3121, open-question #7 struck through, outcome in §3.4; ScopeStack landed 7.3) — not a silent drop. Phase7Execution's 7.5 row claims verified by execution: 21 tests + module doctest all pass, cargo doc clean; NAMING_STRATEGY's claimed method/variant names all match the shipped code. pylatexenc sources confirm the base package exactly matches latex-base + nonascii-specials categories (`^`/`_` are a commented-out TODO in pylatexenc itself).

Three residual findings:

```json
[
  {
    "file": "techy/src/latexlike/driver.rs",
    "line": 97,
    "summary": "New test module imports `crate::state::Lang` which is never used, producing a compiler warning (`cargo test`/`cargo build --tests` warn), breaking the zero-warnings bar the project recorded for the preceding subphase (Phase7Execution.md 7.4 row: 'zero warnings').",
    "failure_scenario": "Run `cargo test -p techy --no-run` → 'warning: unused import: `crate::state::Lang` --&gt; techy/src/latexlike/driver.rs:97'; the 7.5 row silently omits the zero-warnings claim its predecessors made, and the build no longer meets it."
  },
  {
    "file": "ARCHITECTURE.md",
    "line": 1096,
    "summary": "The old §8 promise of default 'standard specials strings (`~ &amp; # ^ _` …)' is silently narrowed: the new text and shipped base_package() deliver only `&amp;`, `~`, and the ligatures — `#`, `^`, `_` are neither delivered, re-scheduled, nor recorded as rejected anywhere in the updated docs (DESIGN_RATIONALE §3.13 discusses only the `[]`/Bracket drop).",
    "failure_scenario": "A reader of the old plan expects `^`/`_` (math scripts — an explicit open TODO even in pylatexenc's latex-base category, _defaultspecs.py lines 293-294) to be preset specials; parsing '$x^2$' with the shipped preset yields plain chars 'x^2' with no specials node and no doc records that this behavior was decided against or deferred."
  },
  {
    "file": "techy/src/lib.rs",
    "line": 51,
    "summary": "The crate-root module list labels the latexlike preset '**In progress (Phase 7.5)**' while Phase7Execution.md's 7.5 row, landed in the same commit, declares 7.5 '✅ done (July 2026)' — the two docs contradict each other on the subphase's status.",
    "failure_scenario": "A reader of the rustdoc front page concludes Phase 7.5 is unfinished (and may distrust the shipped latexlike API), while the execution log says it is complete; the bullet should scope 'in progress' to the preset's remaining 7.6–7.7 items, not to Phase 7.5."
  }
]
```</result>
<usage><subagent_tokens>85496</subagent_tokens><tool_uses>27</tool_uses><duration_ms>810363</duration_ms></usage>
</task-notification>

---

**User:**

<task-notification>
<task-id>a8070ee258154d2a2</task-id>
<tool-use-id>toolu_01NVg1MujvTcWgKsSfp4c2zD</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a8070ee258154d2a2.output</output-file>
<status>failed</status>
<summary>Agent "Angle: altitude check" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>The worktree path itself contains "latexlike", which broke my filter. Let me redo.</result>
</task-notification>

---

**Assistant:**

Two angles are in: the conventions angle was clean, and the removed-behavior audit returned 3 candidates (an unused test import causing a warning, the silently-narrowed `# ^ _` specials promise in ARCHITECTURE.md, and a lib.rs/Phase7Execution status contradiction). However, six finder agents stalled on an infrastructure watchdog timeout mid-work. I'll resume them where they left off.

All six stalled finder agents have been resumed from their transcripts — they'll pick up where they stopped and return their candidate lists. Still waiting on those six plus the two angles that never stalled (line-by-line scan and wrapper-correctness). Once all candidates are in, I'll dedupe and run the verification pass.

---

**User:**

<task-notification>
<task-id>a4770fc411415cf10</task-id>
<tool-use-id>toolu_014qdcqyjNENdkewk94zN5LW</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a4770fc411415cf10.output</output-file>
<status>completed</status>
<summary>Agent "Angle: efficiency check" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>All candidates verified against the core implementations. Summary of the verification:

1. **`Latexlike::scan_specials` fold frequency** — the tokenizer gates the hook on the state's cached filter (`techy/src/token/reader.rs:227`: `state.trigger_chars().may_start(c)` before `L::scan_specials`), so the ScopeStack fold runs only at trigger-character positions, not per input char. The fold itself (`scopes/mod.rs:1103`) iterates providers with zero allocation on the miss path; `Package::scan_specials` is a linear `starts_with` over a longest-first-sorted list, allocating only the `Arc::clone` on a hit. This is the designed multi-provider longest-match contract, and the documented mitigation (TriggerChars cache) exists.

2. **`specials_trigger_chars` "cached per frozen state"** — the claim is accurate: `ParsingState::freeze_with_table` (`state/parsing_state.rs:205-214`) computes `L::specials_trigger_chars(&amp;data)` once at freeze and stores it on the state; the tokenizer reads the cached copy. The union's String allocations happen per state transition, not per token, and transitions are already StateData-clone-scale work (and pre-date this diff — 7.3 code).

3. **`resolve_command` miss detail** — `searched_providers().to_string()` / `error.to_string()` allocate only on the miss/error path, and the detail is consumed on both recovery paths: `nodes_parser.rs:779-783` feeds it into `UnresolvableCommand` which becomes the strict error *and* the tolerant diagnostic (the diff's own test asserts "searched providers: alphapkg, base" appears in the tolerant diagnostic). No path builds it and drops it.

4. **`default_token_rules()` / `initial_state_data()` frequency** — called once per `Language::new` (`engine/language.rs:84`, `Arc::new(ParsingState::initial())`); every parse shares the seed Arc (`parse_source` clones the Arc only; the "define once parse many" test pins this). The second call in the seed test is test-only.

5. **`base_package()` insert re-sort** — `insert_specials` re-sorts per insert (`scopes/mod.rs:607`), but that's 8 entries once per Language construction; trivial.

6. **`group_interior_delta` delta allocation** — hook is invoked only on group-interior memo miss (7.2 memo keyed `(base, rule)`), so the `ParsingStateDelta` allocation is once per (state, rule) pair, not per group node.

No wasted work with real hot-path cost is introduced by this diff; every documented cache it claims actually exists in the core.

[]</result>
<usage><subagent_tokens>70085</subagent_tokens><tool_uses>2</tool_uses><duration_ms>93147</duration_ms></usage>
</task-notification>

---

**User:**

<task-notification>
<task-id>a3ed25e8c1a5fc298</task-id>
<tool-use-id>toolu_01EkLHL1xnrSX2EAYyNUi5NZ</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a3ed25e8c1a5fc298.output</output-file>
<status>completed</status>
<summary>Agent "Angle: simplification check" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>Compiler is clean — no unused imports or dead code. All candidates verified against codebase precedent:

- `new(recovery)` + manual `Default`: exactly mirrors `StdParseDriver` (techy/src/engine/driver.rs:349-366) — same pub-field struct, same `new`, same manual Default (Recovery derives no `Default`, so deriving isn't available without touching core). Consistent, not complexity.
- `let _ = base;`: the established idiom throughout src (15+ occurrences in state/lang.rs, engine/driver.rs, scopes/mod.rs, node/); `_base` would deviate from house style.
- The `group` local fn: `GroupRule` has no `new` constructor (only manual Clone/Debug/PartialEq impls in token/rules.rs), so the 1-line local helper is the minimal form.
- The three `*_name` accessors: node/node_ref.rs itself repeats near-identical match bodies per accessor (`chars`/`comment`, `group_type`/`group_delimiters`); 3 small bodies with distinct docs match the module's own style — a `name_if(CallableType)` helper is not the codebase's pattern.
- `#[non_exhaustive]` in-crate: does real work for downstream crates (FLM is planned out-of-crate) and the docs explicitly state the in-crate exhaustiveness is intentional.
- `strict()` test helper: 1-line named intent, symmetric with `tolerant()`, used at many call sites.

[]</result>
<usage><subagent_tokens>63542</subagent_tokens><tool_uses>1</tool_uses><duration_ms>108556</duration_ms></usage>
</task-notification>

---

**Assistant:**

You've hit your session limit · resets 11:30pm (Europe/Athens)

---

**User:**

<task-notification>
<task-id>a8070ee258154d2a2</task-id>
<tool-use-id>toolu_01Kze2LNyVw86cDmjoXe58Kb</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a8070ee258154d2a2.output</output-file>
<status>failed</status>
<summary>Agent "Angle: altitude check" failed: Agent terminated early due to an API error: You've hit your session limit · resets 11:30pm (Europe/Athens)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
</task-notification>

---

**User:**

<task-notification>
<task-id>a2d9eec7611fa11db</task-id>
<tool-use-id>toolu_01KeTsiRek5Y9ptkvZKqrWPH</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a2d9eec7611fa11db.output</output-file>
<status>failed</status>
<summary>Agent "Angle C: cross-file tracer" failed: Agent terminated early due to an API error: You've hit your session limit · resets 11:30pm (Europe/Athens)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>NodeRef and specials contracts check out. Let me look at the StdCallableSpec default invocation parser and then run the test suite to validate the remaining call contracts.</result>
</task-notification>

---

**User:**

<task-notification>
<task-id>a166337c002140c03</task-id>
<tool-use-id>toolu_01TQCU6DkFtKxA6jbz6ZwMGv</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a166337c002140c03.output</output-file>
<status>failed</status>
<summary>Agent "Angle E: wrapper correctness" failed: Agent terminated early due to an API error: You've hit your session limit · resets 11:30pm (Europe/Athens)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>Now let me check the tokenizer's specials call pattern and `TriggerChars`.</result>
</task-notification>

---

**User:**

<task-notification>
<task-id>a77ce784dcaa5be96</task-id>
<tool-use-id>toolu_01VDNi38HKGRDsVACk4zam8i</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a77ce784dcaa5be96.output</output-file>
<status>failed</status>
<summary>Agent "Angle A: line-by-line scan" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>I've read the diff. Now let me read the actual repo files for the new module and the core APIs they depend on.</result>
</task-notification>

---

**User:**

<task-notification>
<task-id>a5922cd8664fd2204</task-id>
<tool-use-id>toolu_01J5qTAgATXAZTKgWQrgnYRj</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a5922cd8664fd2204.output</output-file>
<status>failed</status>
<summary>Agent "Angle: reuse check" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
</task-notification>

---

**User:**

<task-notification>
<task-id>a1e5bdc777b10ebb7</task-id>
<tool-use-id>toolu_01VLXbYt6mkhSSGuYvPPVfLX</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a1e5bdc777b10ebb7.output</output-file>
<status>failed</status>
<summary>Agent "Angle D: Rust pitfalls" failed: Agent stalled: no progress for 600s (stream watchdog did not recover)</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
</task-notification>

---

**Assistant:**

You've hit your session limit · resets 11:30pm (Europe/Athens)

---

**User:**

continue

---

**Assistant:**

Restating where things stand: the user said to continue. Four angles have completed (conventions, efficiency, simplification all clean; removed-behavior audit returned 3 candidates). Six angles died mid-work — three on watchdog stalls, three on a session limit that has since reset. I'll resume all six and meanwhile run the test suite myself to ground-truth one of the existing candidates (the claimed unused-import warning).

The claimed unused-import warning is now confirmed directly: forcing a recompile prints `warning: unused import: crate::state::Lang --> techy/src/latexlike/driver.rs:97`. That candidate is verified by build output.

The altitude angle also finished, adding two candidates: the non-Command-token arm of `resolve_command` silently absorbs a caller contract violation with `detail: None` (the trait's sanctioned pattern is a self-diagnosing detail string), and the four math-delimiter spellings are duplicated between `math_style()` and `default_token_rules()` with nothing linking the two copies against drift. It cleared the other altitude concerns: the 7.6 `\begin` dispatch plan needs only one inserted branch, and `base_package()`'s literal is appropriate seed data.

That's six candidates so far. Still waiting on four angles: line-by-line, cross-file tracer, Rust pitfalls, and reuse.

---

**User:**

<task-notification>
<task-id>a2d9eec7611fa11db</task-id>
<tool-use-id>toolu_01LFFim5yJ18Lajnuz7XJTNu</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a2d9eec7611fa11db.output</output-file>
<status>completed</status>
<summary>Agent "Angle C: cross-file tracer" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>The dispatch loop only ever passes `Command` tokens to `resolve_command` (nodes_parser.rs:752-761), so the driver's defensive let-else fallback is a dead-but-safe path, and the `Err → Unresolved{detail}` mapping matches the established in-repo resolver pattern (the `CommandResolution` API has no operational-error channel).

All angle checkpoints are now traced:

- **ScopeStack/Package**: `push` adds innermost; `"base"` is the sole seed push, so later seed-delta pushes shadow it — matching `base_package()`'s docs and the `"searched providers: alphapkg, base"` test (innermost-first via `provider_names().rev()`). `retrieve_spec`'s query (`CallableType::Macro` + `Command{escape_char}`) matches `Package::insert` storage keyed `(callable_type, name)` — `Package` ignores syntax, correct. `insert_specials` re-sorts longest-first on every insert, so the base package's mixed-length insertion order is fine; the shared single-`Arc` spec is the documented many-to-one flyweight contract.
- **StdCallableSpec::new(Vec::new())**: zero declared arguments → `parse_declared_arguments` returns empty children; `StdInvocationParser` stages a childless `Callable` node over the trigger span. Finalizes correctly for both Specials and Macro invocations.
- **group_interior_delta memo safety**: `GroupInteriorMemo` is keyed on `(base, rule)` **Arc pointer identities** with keys owning their Arcs (no ABA). A base state in a different mode is necessarily a different `Arc`, so the unconditional `Mode::Math` override can never be replayed under a wrong base — and it is pure in `rule` alone, a fortiori pure in `(base, rule)`. Safe.
- **TokenRules literal / tokenizer precedence**: `PrefixTable::for_rules` sorts entries longest-first (prefix_table.rs:94), so listing `$` before `$$` is harmless — `$$ab$$` opens as `$$`; the `$a$$b$` case is resolved by the expected-close-first rule (reader.rs:310-319). `\(`/`\[` are checked as group delimiters *before* the `\` escape command rule (reader.rs:211). Base specials triggers (`&amp; ~ `` '' -- --- !` ?` ``) collide with no group/command/comment starter, satisfying the lowest-precedence caveat. `expecting_group_close: None`, empty `forbidden_chars`/`temporary_groups` are the correct neutral seed values.
- **Lang obligations**: no `finalize_transition` override → seed trivially coherent (pinned by test); `scan_specials` fold satisfies the `SpecialsMatch::end` obligations (non-empty prefix triggers → boundary-aligned); `specials_trigger_chars` is a pure, mode-blind conservative superset per the provider contract.
- **NodeRef accessors**: `group_type()`/`group_delimiters()`/`callable_type()`/`name()`/`chars()`/`comment()` all return `Option&lt;&amp;'t …&gt;` with the lifetimes the sugar relies on; `math_style()`'s delimiter table matches the recorded open delimiter and documents the `None`-for-unknown-delimiters case.

No defect found in the new module against any callee's actual contract.

[]</result>
<usage><subagent_tokens>159258</subagent_tokens><tool_uses>4</tool_uses><duration_ms>254953</duration_ms></usage>
</task-notification>

---

**Assistant:**

The cross-file tracer finished clean — no contract violations. Notably it verified some things that bear on existing candidates: the group-interior memo is keyed on Arc pointer identity so the unconditional math-mode override is safe; the tokenizer's prefix table sorts longest-first so `$` vs `$$` ordering is harmless; and `CommandResolution` has no operational-error channel, meaning the `Err → Unresolved{detail}` mapping matches the established in-repo resolver pattern (relevant context for verifying the two `resolve_command` candidates).

Still waiting on three angles: line-by-line, Rust pitfalls, and reuse.

---

**User:**

<task-notification>
<task-id>a5922cd8664fd2204</task-id>
<tool-use-id>toolu_01A1xT7czY2VTK52B8Lo4nxB</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a5922cd8664fd2204.output</output-file>
<status>completed</status>
<summary>Agent "Angle: reuse check" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>Reuse scan complete. Verified: TokenRules/WhitespaceRules deliberately have no `Default` (rules.rs doc says the preset is the canonical supplier, so `default_token_rules()` is the intended home, not duplication); no pre-existing MathStyle-like enum or delimiter classifier exists; the engine's `StdParseDriver::resolve_command` default resolves nothing by design. Three genuine duplication findings remain.

[
  {
    "file": "techy/src/latexlike/driver.rs",
    "line": 51,
    "summary": "LatexlikeDriver::resolve_command hand-rolls the same command-token-to-scope-stack dispatch (let-else on TokenKind::Command, CallableQuery::new(...).with_token(token), retrieve_spec match) that already exists as the documented 'preset resolution pattern' helper resolve_macro_via_scopes in techy/src/constructs/nodes_parser.rs:966.",
    "failure_scenario": "Two copies of the query-build-and-dispatch dance now exist (the test helper is #[cfg(test)] and typed to CallableTypeId=u32, so neither can call the other) and they have already drifted: the test helper returns detail:None on a miss while the preset adds searched_providers() detail. Any future change to query construction (e.g. syntax disambiguation fields) must be made in both places; promoting one generic helper (e.g. on ScopeStack or next to CommandResolution in engine::driver) would let the preset and the 6.4 test langs share it."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 249,
    "summary": "The shape/root_shapes/parse_shapes test formatters are a second private shape-string vocabulary; engine/language.rs tests already define the same idea as shapes() (techy/src/engine/language.rs:340) producing the same 'chars(...)' grammar.",
    "failure_scenario": "The crate now has two per-module, lang-typed shape-string formatters with diverging grammars ('group' in language.rs vs 'group(Content { })' here), plus duplicated strict()/tolerant() constructor pairs (language.rs:331/335 vs mod.rs:239/243). Each future test module (7.6 environments, 7.7 verbatim) will re-derive its own variant; a small shared #[cfg(test)] shape helper generic over L (dispatching on the generic NodeRef accessors already used here) would give one grammar for all tree-shape assertions."
  },
  {
    "file": "techy/src/latexlike/node_ref.rs",
    "line": 88,
    "summary": "The language() test helper re-implements the 'package with one zero-arg macro + Language seeded via push_provider' dance that mod.rs tests already define as alpha_package/with_alpha (techy/src/latexlike/mod.rs:396/409), and re-spells Language::default() as Language::new(LatexlikeDriver::default()).",
    "failure_scenario": "The Package::new + insert(CallableType::Macro, name, StdCallableSpec::new(Vec::new())) + with_seed_delta(ParsingStateDelta::new().push_provider(...)) sequence is written twice within the new preset module; a shared pub(super) #[cfg(test)] helper in latexlike/mod.rs (e.g. alpha_package generalized to take the macro name) would serve both sibling files, and line 95 can use the Language&lt;Latexlike&gt; Default impl (as mod.rs:240 already does) instead of spelling out the driver."
  }
]</result>
<usage><subagent_tokens>88685</subagent_tokens><tool_uses>24</tool_uses><duration_ms>371933</duration_ms></usage>
</task-notification>

---

**Assistant:**

The reuse angle returned 3 cleanup candidates: `resolve_command` duplicates a scope-stack dispatch pattern that exists as a test helper (`resolve_macro_via_scopes` in nodes_parser.rs, already drifted from the preset copy), the shape-string test formatters duplicate a similar helper in engine/language.rs tests with a diverging grammar, and node_ref.rs's test `language()` helper re-implements the package-seeding dance from its sibling mod.rs tests. It cleared the non-findings: TokenRules deliberately has no Default (the preset is the documented canonical supplier), and no MathStyle-like classifier pre-exists.

Nine candidates total now. Two angles left: line-by-line and Rust pitfalls.

---

**User:**

<task-notification>
<task-id>a1e5bdc777b10ebb7</task-id>
<tool-use-id>toolu_019wdWb5RnjXkp5WcJRXoAii</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a1e5bdc777b10ebb7.output</output-file>
<status>completed</status>
<summary>Agent "Angle D: Rust pitfalls" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>Compile check passes clean (confirms no inherent-method collisions and that all new non-test code satisfies the `#![cfg_attr(not(test), no_std)]` build with alloc-only imports).

Scan complete. Summary of what was verified against each pitfall class:

- **UTF-8/byte indexing**: all preset trigger strings, whitespace chars, and `name_chars` are ASCII; `Package::scan_specials` slices at a tokenizer-guaranteed char boundary and uses byte-safe `starts_with`; `TriggerChars` is char-based; the reader's command path uses `len_utf8()` and char iteration. No defect.
- **Arc sharing (one spec, 8 triggers)**: `TokenKind::Specials` equality is `name == name &amp;&amp; Arc::ptr_eq(spec)` — names differ per trigger, and the seed spec is shared per-`Language`, so nothing identity-keyed breaks. All other `ptr_eq` sites are group-rule or test code.
- **`#[non_exhaustive]` claims**: accurate — no in-crate effect (matches stay exhaustive as documented and intended), downstream gets the standard wildcard obligation; variants remain constructible.
- **Default consistency**: `Recovery` has no `Default` derive, so `LatexlikeDriver`'s manual `Default` is required, and `Mode`'s `#[default] Text` matches the explicit `Mode::Text` seed.
- **PartialEq semantics**: `TokenRules`/`GroupRule` have manual *structural* `PartialEq` (Arc contents, not pointers), so the seed test `seed.rules() == &amp;default_token_rules()` is sound.
- **Inherent-impl coherence**: none of the five sugar names exist on generic `NodeRef` (and E0592 would have failed the clean `cargo check`).
- **`let _ = base;`**: the trait contract requires purity in `(base, rule)` with a memo keyed on both — a base-independent impl is trivially compliant.
- **Raw strings**: `r"\("` etc. are the intended backslash+delimiter two-char sequences.
- **Recovered groups**: `group_delimiters` always returns `Some` for group nodes (close = `""` on tolerant recovery), so `math_style()` still classifies unclosed `$…` correctly.

One real (documentation-level) inconsistency surfaced, directly invalidated by this diff's own design decision:

```json
[
  {
    "file": "techy/src/engine/driver.rs",
    "line": 255,
    "summary": "ParseDriver::group_interior_delta rustdoc still cites the pre-7.5 sketch — 'a math-class rule returns ParsingStateDelta::new().mode(MathInline)' interpreted by Lang::finalize_transition — but this diff's checkpoint eliminated MathInline/MathDisplay (single Mode::Math, no finalize_transition override), leaving the trait's canonical example referencing an identifier that exists nowhere in the crate.",
    "failure_scenario": "A driver implementor reads the group_interior_delta doc, searches the crate for MathInline to copy the referenced pattern, finds nothing (grep 'MathInline' matches zero code), and either writes a bogus Mode variant or wires an unnecessary finalize_transition override; if the doc line were ever promoted to a doctest it would fail to compile. The diff amended the equivalent stale bullet in ARCHITECTURE.md but missed this in-code reference."
  }
]
```</result>
<usage><subagent_tokens>95484</subagent_tokens><tool_uses>17</tool_uses><duration_ms>412036</duration_ms></usage>
</task-notification>

---

**Assistant:**

The Rust-pitfalls angle finished with one finding — a stale-doc inconsistency the diff itself created: `ParseDriver::group_interior_delta`'s rustdoc in engine/driver.rs still cites the pre-7.5 sketch (`mode(MathInline)` + `finalize_transition`), but this diff's checkpoint eliminated `MathInline`; the diff fixed the equivalent stale bullet in ARCHITECTURE.md but missed the in-code one. All the classic pitfall classes (UTF-8 boundaries, Arc identity, PartialEq semantics, non_exhaustive claims, method collisions) verified clean.

Ten candidates. Only the line-by-line angle remains, then verification.

---

**User:**

<task-notification>
<task-id>a77ce784dcaa5be96</task-id>
<tool-use-id>toolu_014m8UKzosrkbP3AY9WiGVEF</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a77ce784dcaa5be96.output</output-file>
<status>completed</status>
<summary>Agent "Angle A: line-by-line scan" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>Confirmed — the rendered HTML shows the mangling: `&lt;code&gt; &lt;/code&gt; &lt;code&gt;, `''`, `--`, `---, &lt;/code&gt;` with stray backticks around `!` and `?`. The scan is complete: all other paths verified correct (prefix table is longest-first so `$$` beats `$` at open; reader checks groups before commands so `\(`/`\[` open math; specials triggers don't collide with group/command/comment starts; the 8-trigger set exactly matches pylatexenc's `latex-base` + `nonascii-specials` categories; `resolve_command` follows the established in-repo error pattern; `group_delimiters` resolves source text so the `math_style` table is sound; all 21 tests + doctest pass).

```json
[
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 210,
    "summary": "Invalid CommonMark in base_package() rustdoc: the double-backtick ligature is written as `` `` `` (2-backtick delimiters around 2 backticks), so the middle backtick run closes the code span early and the rendered doc is mangled.",
    "failure_scenario": "cargo doc renders the base_package docs as 'ligature specials &lt;code&gt; &lt;/code&gt; &lt;code&gt;, `''`, `--`, `---, &lt;/code&gt; !&lt;code&gt; ``, `` ?&lt;/code&gt; ``' — the backtick ligature shows as a space-only code span, the `''`/`--`/`---` entries are swallowed into a stray literal code span, and !`/?` render as garbage with leftover backticks (verified in target/doc/techy/latexlike/fn.base_package.html). Fix: use 3-backtick delimiters, e.g. ``` `` ```."
  },
  {
    "file": "ARCHITECTURE.md",
    "line": 1097,
    "summary": "Same broken markdown construct in §8: 'ligatures `` `` `` `''` `--` `---` `` !` `` `` ?` ``' mispairs its backtick runs, mangling the rendered specials list.",
    "failure_scenario": "Any CommonMark renderer (GitHub, IDE preview) pairs the first two 2-backtick runs into a space-only code span and pairs the third with the opener of `` !` ``, turning ' `''` `--` `---` ' into one literal code chunk and leaving stray backticks — the documented specials list is unreadable."
  },
  {
    "file": "DESIGN_RATIONALE.md",
    "line": 2997,
    "summary": "Same broken `` `` `` construct in the §3.13 'base package' decision entry mangles the rendered trigger list.",
    "failure_scenario": "Rendered markdown shows a space-only code span for the backtick ligature and swallows ', `''`, `--`, `---`, ' into a stray code span (identical CommonMark mispairing as in mod.rs/ARCHITECTURE.md), so the authoritative decision record displays the wrong specials list."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 185,
    "summary": "Lower confidence: default whitespace chars are ASCII-only (' \\t\\n\\r\\x0B\\x0C'), diverging from pylatexenc, whose tokenizer treats Unicode whitespace (NBSP U+00A0, U+2028, etc.) as space — an undocumented parity deviation.",
    "failure_scenario": "Input '\\\\emph\\u{00A0}text' (or any Unicode space after a command): pylatexenc swallows the character as command post-space, while the preset tokenizes it as an ordinary content char starting the following chars node — different node shapes for the same source, and no doc/DESIGN_RATIONALE entry records the deviation as intended."
  }
]
```</result>
<usage><subagent_tokens>114695</subagent_tokens><tool_uses>15</tool_uses><duration_ms>903632</duration_ms></usage>
</task-notification>

---

**Assistant:**

All 10 finder angles are now complete. The line-by-line scan closed with 4 candidates: a broken CommonMark backtick construct mangling the rendered specials list in three places (base_package rustdoc — verified against the generated HTML — plus ARCHITECTURE.md and DESIGN_RATIONALE.md), and a lower-confidence parity deviation (ASCII-only whitespace vs. pylatexenc's Unicode whitespace).

**Phase 2 — dedupe and verify.** Merging the three same-mechanism markdown findings into one, I have 12 distinct candidates. The unused-import warning is already confirmed by my own build run, and the markdown mangling by rendered-HTML inspection. I'll verify the mechanical/factual candidates inline with targeted reads, and spawn verifier agents for the four judgment-heavy ones.

Inline verification done — three more candidates confirmed by direct evidence:

- **Stale rustdoc confirmed:** engine/driver.rs:255 literally reads "`ParsingStateDelta::new().mode(MathInline)`… and `Lang::finalize_transition` interprets the mode change", and `MathInline` appears nowhere else in the crate — the identifier was eliminated by this diff's own checkpoint decision.
- **resolve_command duplication confirmed:** `resolve_macro_via_scopes` exists at nodes_parser.rs:966 under `#[cfg(test)]`, used at three test call sites — the preset re-rolls the same dispatch with drifted detail behavior.
- **Test-helper duplication confirmed:** engine/language.rs's test module has the same `strict()`/`tolerant()` pair and a `shapes()` formatter with a diverging grammar from the new mod.rs helpers.

The four judgment-heavy candidates (specials-promise narrowing, Unicode-whitespace parity, Err-flattening, and the `detail: None` arm) are now with verifier agents.

---

**User:**

<task-notification>
<task-id>a2b8f3a70524975a9</task-id>
<tool-use-id>toolu_016hCgtMkNr9i31nWCLv8kdm</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a2b8f3a70524975a9.output</output-file>
<status>completed</status>
<summary>Agent "Verify whitespace-parity candidate" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>VERDICT: CONFIRMED

pylatexenc's tokenizer is Unicode-whitespace-aware: `impl_peek_space_chars` in `~/Research/util/pylatexenc/pylatexenc/latexnodes/_tokenreader.py` (lines 376–379) loops with `if not c.isspace(): break` — Python's Unicode-aware `str.isspace()` (`'\u00a0'.isspace()` is `True`) — and that same routine supplies post-macro space at lines 525–527 (`post_space, ... = self.impl_peek_space_chars(s, posi, parsing_state)`), so pylatexenc swallows an NBSP after `\emph` as `post_space` while techy's preset (`whitespace: WhitespaceRules { chars: " \t\n\r\u{000B}\u{000C}".into() }`, `techy/src/latexlike/mod.rs:185`) leaves it to become a Chars content node, since `skip_whitespace` does exact membership (`if !ws.chars.contains(c)`, `techy/src/token/reader.rs:111`). No document records the deviation: the preset doc says only "standard whitespace", the diff and Phase7Execution.md never mention the char set or ASCII, and DESIGN_RATIONALE.md's nearest line (2796, "nothing... needs unicode tables beyond `char` methods") is a dependency-hygiene note that if anything points at the Unicode-aware `char::is_whitespace()`, not a recorded ASCII-only decision.</result>
<usage><subagent_tokens>34017</subagent_tokens><tool_uses>14</tool_uses><duration_ms>192448</duration_ms></usage>
</task-notification>

---

**Assistant:**

The whitespace-parity candidate is CONFIRMED: pylatexenc's tokenizer uses Python's Unicode-aware `isspace()` (so an NBSP after `\emph` becomes post-macro space there), while the preset's ASCII-only whitespace set makes it a content char — and no document records this deviation as intended.

Three verifiers still out: specials-promise narrowing, Err-flattening, and the `detail: None` arm.

---

**User:**

<task-notification>
<task-id>a3465ec7fe9fff1ba</task-id>
<tool-use-id>toolu_016Whv2ZhgfkyUeDQ5BWzyYP</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a3465ec7fe9fff1ba.output</output-file>
<status>completed</status>
<summary>Agent "Verify Err-flattening candidate" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>**PLAUSIBLE** — the information loss is real, but the current core API admits no better routing, and this diff does not touch that API. `CommandResolution` (pre-existing from 7.2, in `/Users/philippe/projects/techy/.claude/worktrees/subphase-7-5-latexlike/techy/src/engine/driver.rs` lines 410–422, untouched by the patch) offers exactly two outcomes — `Resolved(ResolvedCallable&lt;L&gt;), Unresolved { detail: Option&lt;String&gt; }` — and `resolve_command(&amp;self, state, token)` has no session/funnel access and no `Result` return, so `Unresolved{detail}` is the only expressible failure; the core loop then unconditionally builds the same condition either way (`UnresolvableCommand::new(*name, *escape_char, detail)`, nodes_parser.rs:783), preserving the rendered `ScopeStackError` prose (provider name included) in both the strict abort error and the tolerant diagnostic, but not the structured type or a distinct condition id. The 7.3 `ScopeOpFailed` precedent does give operational scope-stack failures a distinct condition — but that routing lives in core (`ParseContext::recover_derive_failure`), enabled by `derived()` returning `Result&lt;_, DeriveError&gt;`; note it likewise flattens the structured error to a `detail: String` ("The rendered failure (`ScopeOpError`'s `Display`)"), so even the precedent distinguishes only at the condition-id level. What would settle diff-attributability: whether subphase 7.5 was scoped to amend the core enum — it wasn't (the diff touches only `latexlike/*`, docs, and `lib.rs`), the trait doc explicitly names `detail` as "the place for a resolver to say *why*", and the driver's own doc declares the mapping ("a provider failure reports the provider's error"), so the defect, if accepted, is a pre-existing core API gap warranting a follow-up (an error channel on the `#[non_exhaustive]` enum plus a dedicated condition in the dispatch loop), not a flaw introduced by this diff.</result>
<usage><subagent_tokens>57122</subagent_tokens><tool_uses>13</tool_uses><duration_ms>208324</duration_ms></usage>
</task-notification>

---

**User:**

<task-notification>
<task-id>a1c92b334a08c262c</task-id>
<tool-use-id>toolu_01KekNrHC833wWNoythfqKGL</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a1c92b334a08c262c.output</output-file>
<status>completed</status>
<summary>Agent "Verify specials-narrowing candidate" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>All four verification steps are complete. The evidence is unambiguous.

**REFUTED** — The deferral is explicitly recorded in the updated docs, and the preset has exact parity with pylatexenc. Phase7Execution.md lines 34–42, under the heading "**Out of scope (deferred, recorded):**", states: "**`^`/`_`/`#` sub/superscript specials** (std-library material; pylatexenc leaves them as chars too, so ported tests are unaffected)" — covering all three characters, and ARCHITECTURE.md line 1242 independently records "**Deferred out of Phase 7:** the std spec-database port, parse-level `\newcommand`, `^`/`_` specials, `\global` definitions" (present since the 7.0 commit and retained by this diff). On the parity question, pylatexenc's `$HOME/Research/util/pylatexenc/pylatexenc/latexwalker/_defaultspecs.py` lines 287–294 ship `std_specials('&amp;')` while `#std_specials('^')` / `#std_specials('_')` are commented out behind a "TODO" (and no `std_specials('#')` exists anywhere), so the shipped `base_package()` set (`&amp;`, `~`, ligatures) matches pylatexenc's actual default context exactly — the old §8 line "(`~ &amp; # ^ _` …)" was aspirationally over-broad, and the new §8 corrects it to the documented, deferred-and-recorded reality rather than silently narrowing it.</result>
<usage><subagent_tokens>36816</subagent_tokens><tool_uses>12</tool_uses><duration_ms>235345</duration_ms></usage>
</task-notification>

---

**Assistant:**

Two more verdicts in:

- **Err-flattening: PLAUSIBLE.** The information loss is real, but `CommandResolution` (pre-existing, untouched by this diff) admits no better routing — the rendered error text does survive into both the strict error and the tolerant diagnostic. If accepted, it's a pre-existing core API gap warranting a follow-up (an error channel on the enum + a distinct condition), not a flaw introduced here. Kept at reduced rank per recall rules.
- **Specials narrowing: REFUTED and dropped.** Phase7Execution.md explicitly records "`^`/`_`/`#` sub/superscript specials" under "Out of scope (deferred, recorded)", and pylatexenc itself ships none of them (they're a commented-out TODO in its own defaults) — the old doc line was aspirationally over-broad and the new text corrects it to actual parity.

One verifier left: the `detail: None` non-Command arm.

---

**User:**

<task-notification>
<task-id>a97eae712a29c2415</task-id>
<tool-use-id>toolu_01Pot1kkLE4kEVK3zPcQ3sdR</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a97eae712a29c2415.output</output-file>
<status>completed</status>
<summary>Agent "Verify detail:None candidate" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>REFUTED — this let-else arm is verbatim the pre-existing established pattern, not a new deviation. Three in-repo drivers committed before this diff (subphase 7.3, commit 509d7d4) contain the identical arm `let TokenKind::Command { name, escape_char, .. } = &amp;token.kind else { return CommandResolution::Unresolved { detail: None }; }` — /Users/philippe/projects/techy/.claude/worktrees/subphase-7-5-latexlike/techy/src/constructs/nodes_parser.rs:966-972 even documents it as "The preset resolution pattern, shared by the 6.4 test langs" (also argument_parsers.rs:830-832, environment_parser.rs:575-577; none appear in /tmp/claude-501/review/review-diff.patch). Moreover `detail: None` is a documented, blessed state of the enum, not silent absorption: the field docs in /Users/philippe/projects/techy/.claude/worktrees/subphase-7-5-latexlike/techy/src/engine/driver.rs (~line 422) read "`None` when there is nothing to say beyond 'the name did not resolve'", the `From&lt;Option&lt;ResolvedCallable&gt;&gt;` bridge maps `None → Unresolved { detail: None }` (line 430), and the core itself constructs `Unresolved { detail: None }` at nodes_parser.rs:759 ("no detail: the hook never ran").

On the policy angle: CLAUDE.md rule 4 and DESIGN_RATIONALE §3.8 rule 2 prescribe "returns an `Err`, it does not panic" for contract violations — they govern panic-vs-error, and returning `Unresolved` (the only failure channel this signature has) satisfies that; neither document prescribes a detail string. The trait doc ("Resolve a Command token…") describes the expected input but states no implementor obligation for other kinds, and the trait's own default ignores `token.kind` entirely. A self-diagnosing detail would be a nit-level polish across all four sites, not a defect in this diff.</result>
<usage><subagent_tokens>42565</subagent_tokens><tool_uses>13</tool_uses><duration_ms>249824</duration_ms></usage>
</task-notification>

---

**Assistant:**

The `detail: None` candidate is REFUTED and dropped: the identical let-else arm exists verbatim in three drivers committed before this diff (nodes_parser.rs even documents it as "the preset resolution pattern"), `detail: None` is a documented blessed state of the enum, and the panic policy governs panic-vs-error, not detail strings.

Phase 2 is complete: 10 findings survive (8 CONFIRMED, 1 PLAUSIBLE, 1 low-rank status-wording issue), 2 refuted. Now Phase 3 — a fresh gap-sweep finder that gets the verified list and hunts only for defects not already on it.

The gap-sweep finder is running with the full verified list, hunting only for defects the first pass missed — tokenizer edge cases around `$`/`$$` disambiguation, assertion gaps in the new tests, TokenRules flag semantics, and doc-code drift beyond what's already listed. Once it reports, I'll verify anything new it finds and produce the final ranked findings.

---

**User:**

<task-notification>
<task-id>a10924e5f24fd88bb</task-id>
<tool-use-id>toolu_015Fi4wXyHWTAysbKRfLZjwX</tool-use-id>
<output-file>/private/tmp/claude-501/-Users-philippe-projects-techy--claude-worktrees-subphase-7-5-latexlike/3ef12e64-c86f-4feb-895d-b9d229ffb041/tasks/a10924e5f24fd88bb.output</output-file>
<status>completed</status>
<summary>Agent "Phase 3 gap sweep" finished</summary>
<note>A task-notification fires each time this agent stops with no live background children of its own. The user can send it another message and resume it, so the same task-id may notify more than once.</note>
<result>Gap sweep complete. I verified each candidate empirically: built a scratch probe crate (outside the repo) against the preset to exercise the `$`-disambiguation edges, ran the latexlike suite, `cargo check --tests` (only the already-known warning), `cargo doc` (clean, no broken intra-doc links), and cross-checked pylatexenc's actual behavior (`$HOME/Research/util/pylatexenc`) for macro alphabet (exact match), specials set (exact match), math-nesting edges (`\(a$b$c\)`, `$$a$b$$`, `$a\n\nb$` — all strict-mode parity), and paragraph-break node shape (divergence found). Refuted along the way: name-chars/`*` parity, mode-blind trigger-char union, `enable_specials` gating, prefix-table order dependence (longest-first sort makes vec order irrelevant), `Latexlike` ZST derive needs, `SourceOrigin` consumption.

Four new findings survived verification:

```json
[
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 209,
    "summary": "base_package's rustdoc claims 'the standard specials of pylatexenc's default context' (and DESIGN_RATIONALE.md:3000 claims 'out-of-the-box parity with pylatexenc's default node shapes'), but pylatexenc's default context also ships the '\\n\\n' paragraph specials (latex-paragraph category), which base omits — an undocumented node-shape deviation.",
    "failure_scenario": "Input \"a\\n\\nb\": pylatexenc's default context yields [LatexCharsNode 'a', LatexSpecialsNode '\\n\\n', LatexCharsNode 'b'] (verified against the pylatexenc checkout), while techy yields a plain whitespace Chars node (pinned by paragraph_breaks_split_content) — a consumer porting pylatexenc code that matches on the Specials node for paragraph handling silently gets no matches, and no doc records the deliberate scoping-out."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 458,
    "summary": "No test exercises base-package specials (or % comments) inside math mode, leaving the mode-blind visibility of the seed package — a central preset property, in a suite otherwise heavy on mode-visibility machinery — completely unpinned.",
    "failure_scenario": "A later edit marks base text-only via set_visible_modes(Some(vec![Mode::Text])) (or the scan's visibility filter regresses): \"$a~b---c$\" silently degrades from [chars(a), Specials(~), chars(b), Specials(---), chars(c)] inside the math group (verified current behavior via probe) to one plain chars run, and all 21 latexlike tests plus the doctest stay green."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 439,
    "summary": "Every diagnostic assertion in the new suite checks only count and a message substring — no test asserts a diagnostic's span or severity, and the strict-path assertion at line 446 checks only the 'cannot resolve command' prefix, never that the new searched-providers miss detail rides the strict error too.",
    "failure_scenario": "The recover funnel regresses to emit the unresolvable-command diagnostic with a zero-width or post-space-shifted span, a downgraded severity, or drops resolve_command's Some(detail) on the strict abort path: mode_visibility_gates_package_definitions and unknown_commands_abort_strict_and_recover_tolerant still pass, so editor squiggles/severity filtering break with a green suite."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 370,
    "summary": "The close-before-open rule is pinned only for the benign root-level '$a$$b$' case; the surprising tolerant tree for a lone '$' inside '$$…$$' — where the trailing literal '$$' is split '$'+'$' and the display group is never closed — is unpinned by any test.",
    "failure_scenario": "Tolerant parse of \"$$a$b$$\" (verified via probe): the expected-close rule consumes the first '$' of the final '$$' as the inner inline group's close and reopens a fresh inline group with the second, producing group($$ …)[chars(a), group($ $)[chars(b)], group($ &lt;unclosed&gt;)] plus two unclosed-group diagnostics — the display group never closes despite a literal '$$' present; any future recovery change flips this tree invisibly since strict-mode pylatexenc parity (both error) is the only anchored behavior."
  }
]
```</result>
<usage><subagent_tokens>123519</subagent_tokens><tool_uses>42</tool_uses><duration_ms>1149517</duration_ms></usage>
</task-notification>

---

**Assistant:**

The gap sweep finished and delivered 4 additional findings, each verified empirically by the sweeper itself (it built a scratch probe crate to exercise the parser and cross-checked pylatexenc's actual behavior): base_package's rustdoc claims parity with pylatexenc's default context, which also ships `\n\n` paragraph specials that base omits; and three test-coverage gaps (base specials inside math mode unpinned, no span/severity assertions on diagnostics, and the surprising tolerant tree for `$$a$b$$` unpinned). It also refuted several near-misses along the way (macro alphabet parity, prefix-table order dependence, `enable_specials` gating).

The review is complete. Sanity check: all 10 finder angles ran to completion (after resuming stalled agents), every candidate was verified — by direct build/grep/HTML evidence, a dedicated verifier agent, or the sweeper's empirical probes — 2 candidates were refuted and dropped, and the full test suite passes (389 + 8 tests, doctests green).

**Overall: no functional correctness bug was found in the shipped parse behavior** — the tokenizer precedence, memoization safety, scope-stack contracts, UTF-8 handling, and Arc-sharing were all traced and verified clean. The 14 surviving findings are: two undocumented pylatexenc parity deviations, one compiler warning, three documentation defects, one API-gap follow-up, three test-coverage gaps, and four cleanup/duplication items.

```json
[
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 185,
    "summary": "Default whitespace chars are ASCII-only, silently diverging from pylatexenc's Unicode-aware whitespace handling, and no doc records the deviation.",
    "failure_scenario": "Input \"\\emph\\u{00A0}text\": pylatexenc's impl_peek_space_chars uses str.isspace() (Unicode-aware) and swallows the NBSP as post-macro space, while the preset's exact-membership check over \" \\t\\n\\r\\u{000B}\\u{000C}\" makes it a content char starting the following chars node — different node shapes for the same source, with no DESIGN_RATIONALE entry recording the ASCII-only choice as intended."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 209,
    "summary": "base_package's rustdoc claims \"the standard specials of pylatexenc's default context\", but pylatexenc's default context also ships the \"\\n\\n\" paragraph specials (latex-paragraph category), which base omits — an undocumented node-shape deviation (DESIGN_RATIONALE.md:3000 makes the same parity claim).",
    "failure_scenario": "Input \"a\\n\\nb\": pylatexenc's default context yields [CharsNode 'a', SpecialsNode '\\n\\n', CharsNode 'b'], while techy yields a plain whitespace chars node (pinned by paragraph_breaks_split_content) — a consumer porting pylatexenc code that matches on the paragraph Specials node silently gets no matches, and no doc records the scoping-out."
  },
  {
    "file": "techy/src/latexlike/driver.rs",
    "line": 97,
    "summary": "Unused import `crate::state::Lang` in the test module produces a compiler warning, breaking the zero-warnings bar recorded for the preceding subphases.",
    "failure_scenario": "cargo build --tests -p techy → \"warning: unused import: `crate::state::Lang` --> techy/src/latexlike/driver.rs:97\" (confirmed by forced recompile); Phase7Execution.md's 7.4 row claims \"zero warnings\" and the 7.5 build no longer meets it."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 210,
    "summary": "Invalid CommonMark: the double-backtick ligature written as `` `` `` mispairs its backtick runs, mangling the rendered specials list in base_package's rustdoc and in the same construct at ARCHITECTURE.md:1097 and DESIGN_RATIONALE.md:2997.",
    "failure_scenario": "cargo doc renders base_package's docs with a space-only code span, swallows the `''`/`--`/`---` entries into a stray literal code chunk, and leaves garbage backticks around !` and ?` (verified in target/doc/techy/latexlike/fn.base_package.html); GitHub/IDE previews mangle the two markdown files identically. Fix: 3-backtick delimiters for code spans containing backticks."
  },
  {
    "file": "techy/src/engine/driver.rs",
    "line": 255,
    "summary": "ParseDriver::group_interior_delta's rustdoc still cites the pre-7.5 sketch — \"a math-class rule returns ParsingStateDelta::new().mode(MathInline)\" interpreted by Lang::finalize_transition — but this diff's checkpoint eliminated MathInline (grep matches zero code) and needs no finalize_transition override.",
    "failure_scenario": "A driver implementor reads the trait's canonical example, searches for MathInline to copy the pattern, finds nothing, and either invents a bogus Mode variant or wires an unnecessary finalize_transition override; the diff amended the equivalent stale bullet in ARCHITECTURE.md but missed this in-code reference."
  },
  {
    "file": "techy/src/latexlike/driver.rs",
    "line": 73,
    "summary": "resolve_command flattens operational provider failures (Err(ScopeStackError)) into the same Unresolved shape as an ordinary miss, losing the structured error — a pre-existing CommandResolution API gap this preset now makes reachable (follow-up: an error channel on the #[non_exhaustive] enum + a distinct condition, mirroring 7.3's ScopeOpFailed precedent).",
    "failure_scenario": "A database-backed SpecsProvider returns Err(ProviderError::Failed(\"connection lost\")) while resolving \\emph under Recovery::Tolerant: the parse records an ordinary UnresolvableCommand diagnostic with the error prose stuffed into the free-form detail string and continues — tooling cannot distinguish \"command unknown\" from \"provider broken\", and every subsequent command degrades the same way."
  },
  {
    "file": "techy/src/latexlike/node_ref.rs",
    "line": 42,
    "summary": "The four math-delimiter spellings are duplicated as unlinked string literals in math_style() and default_token_rules() — no shared table, so the two copies can drift.",
    "failure_scenario": "A later phase adds or changes a default math delimiter pair in default_token_rules(): math_style() silently answers None for groups parsed under the preset's own defaults (the documented None escape hatch was meant for embedder-registered delimiters); a single private table both functions read would remove the drift risk without reopening the checkpoint decision."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 458,
    "summary": "No test exercises base-package specials (or % comments) inside math mode, leaving the seed package's mode-blind visibility — a central preset property in a suite otherwise heavy on mode-visibility machinery — unpinned.",
    "failure_scenario": "A later edit marks base text-only via set_visible_modes (or the scan's visibility filter regresses): \"$a~b---c$\" silently degrades from [chars, Specials(~), chars, Specials(---), chars] inside the math group to one plain chars run, and all 21 latexlike tests plus the doctest stay green."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 370,
    "summary": "The close-before-open rule is pinned only for the benign root-level \"$a$$b$\" case; the surprising tolerant-mode tree for a lone \"$\" inside \"$$…$$\" is unpinned by any test.",
    "failure_scenario": "Tolerant parse of \"$$a$b$$\" (verified by probe): the expected-close rule consumes the first '$' of the trailing \"$$\" as the inner inline group's close and reopens a fresh inline group with the second, so the display group never closes despite a literal \"$$\" present (two unclosed-group diagnostics); any future recovery change flips this tree invisibly since only strict-mode behavior is anchored."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 439,
    "summary": "Every diagnostic assertion in the new suite checks only count and a message substring — no test asserts a diagnostic's span or severity, and the strict-path assertion never checks that the searched-providers detail rides the strict error.",
    "failure_scenario": "The recover funnel regresses to emit the unresolvable-command diagnostic with a zero-width/shifted span, a downgraded severity, or drops resolve_command's Some(detail) on the strict abort path: mode_visibility_gates_package_definitions and unknown_commands_abort_strict_and_recover_tolerant still pass, so editor squiggles and severity filtering break with a green suite."
  },
  {
    "file": "techy/src/latexlike/driver.rs",
    "line": 51,
    "summary": "resolve_command hand-rolls the same command-token-to-scope-stack dispatch that exists as the documented \"preset resolution pattern\" test helper resolve_macro_via_scopes (nodes_parser.rs:966), and the two copies have already drifted (detail:None vs searched-providers detail).",
    "failure_scenario": "Any future change to query construction (e.g. syntax disambiguation fields) must be made in both places; the test helper is #[cfg(test)] and typed to CallableTypeId=u32 so neither can call the other — promoting one generic helper (on ScopeStack or next to CommandResolution) would let the preset and the test langs share it."
  },
  {
    "file": "techy/src/lib.rs",
    "line": 51,
    "summary": "The crate-root module list labels the latexlike preset \"**In progress (Phase 7.5)**\" while Phase7Execution.md's 7.5 row, landed in the same commit, declares 7.5 \"✅ done\" — the wording should scope in-progress-ness to the remaining 7.6–7.7 items.",
    "failure_scenario": "A rustdoc front-page reader concludes Phase 7.5 itself is unfinished and distrusts the shipped latexlike API, while the execution log says it is complete; \"core landed in 7.5; environments/verbatim land in 7.6–7.7\" would remove the contradiction."
  },
  {
    "file": "techy/src/latexlike/mod.rs",
    "line": 249,
    "summary": "The shape/root_shapes/parse_shapes test formatters plus the strict()/tolerant() pair are a second private shape-string vocabulary; engine/language.rs tests already define the same idea (shapes(), language.rs:340) with a diverging grammar.",
    "failure_scenario": "Each future test module (7.6 environments, 7.7 verbatim) re-derives its own variant of the same helper with its own grammar; a small shared #[cfg(test)] shape helper generic over L, dispatching on the generic NodeRef accessors already used here, would give one grammar for all tree-shape assertions."
  },
  {
    "file": "techy/src/latexlike/node_ref.rs",
    "line": 88,
    "summary": "The language() test helper re-implements the package-with-one-macro + push_provider seeding dance already defined as alpha_package/with_alpha in the sibling mod.rs tests, and spells out Language::new(LatexlikeDriver::default()) where Language::default() exists.",
    "failure_scenario": "The Package::new + insert + with_seed_delta(push_provider) sequence is written twice within the new preset module; a shared pub(super) #[cfg(test)] helper taking the macro name would serve both files."
  }
]
```

result: /code-review xhigh of subphase 7.5 (latexlike preset) found no functional bugs; 14 findings — 2 undocumented pylatexenc parity deviations (Unicode whitespace, paragraph specials), 1 compiler warning, 3 doc defects, 1 error-channel API gap, 3 test-coverage gaps, 4 duplication cleanups.

---

**User:**

/md-export CodeReviewAgentsFindings.md

---

**Assistant:**

Restating: you want the conversation exported to `CodeReviewAgentsFindings.md`. Running the export now.
