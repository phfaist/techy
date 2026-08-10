# Execution progress log

**Purpose: interruption recovery.** Update as items complete and always at stage
boundaries. A fresh session must be able to continue from this file + `git log`
alone. Keep the "Resume here" line current; record any deviation from PLAN.md and
any execution-time micro-ruling in the notes column (with the commit that took it).

Commit convention: small per-item commits on this branch
(`worktree-py-ext-feedback-plan`), messages prefixed `pyext-<item>:`.

**Resume here:** execution not started. Next action: Stage 1, item 1.1.
Open questions 1–6 (PLAN.md foot) may be answered by the user at any point —
check there before starting Stage 3.

| Item | Status | Notes |
|---|---|---|
| 1.1 LineIndex multi-byte resume fix | pending | |
| 1.2 ChildRegion::staged() pub + Panics docs | pending | |
| 1.3 Include-chain message: written reference | pending | |
| 1.4 Oracle falsifiability test | pending | |
| 1.5 format_position_with message | pending | |
| 1.6 scan_specials bounds → Err | pending | |
| **Stage 1 gate** (build/test/docs) | pending | |
| 2.1 + Any on four traits | pending | |
| 2.2 Language::new Into<Arc> | pending | |
| 2.3 NodeTree::slice(range) validated | pending | |
| 2.4 MacroSpec::with_after_effect | pending | |
| 2.5 TreeViolationKind/TokenKind as_str | pending | |
| 2.6 NodeTree::tree_tag() pub | pending | |
| 2.7 TreeViolation::new + MalformedBegin | pending | |
| 2.8 DiagnosticInfo::identifier() | pending | |
| **Stage 2 gate** (build/test/docs) | pending | |
| 3.0 HookFailed condition | pending | cause-chain: open question 1 |
| 3.1 Tier A signatures (+ lang_initial ripple) | pending | |
| 3.2 Tier B signatures (+ observe_transition sink) | pending | |
| 3.3 Tier C infallibility docs | pending | |
| 3.4 stage_invocation Err not panic | pending | |
| **Stage 3 gate** (build/test/docs + hot-path size check) | pending | |
| 4.1–4.14 doc batch | pending | |
| **Stage 4 gate** (docs build, link check) | pending | |
| 5 closure (api-baseline; rationale entries AFTER cleanup agent done; courtesy notes; delete folder) | pending | ARCHITECTURE/DESIGN_RATIONALE untouchable until cleanup agent finishes |
