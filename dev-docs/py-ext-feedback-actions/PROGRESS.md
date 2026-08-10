# Execution progress log

**Purpose: interruption recovery.** Update as items complete and always at stage
boundaries. A fresh session must be able to continue from this file + `git log`
alone. Keep the "Resume here" line current; record any deviation from PLAN.md and
any execution-time micro-ruling in the notes column (with the commit that took it).

Commit convention: small per-item commits on this branch
(`worktree-py-ext-feedback-plan`), messages prefixed `pyext-<item>:`.

**Resume here:** EXECUTING (started 2026-08-10 on user signal; branch rebased onto
main at d52fe4c — dev-docs cleanup landed). Stage 1 in progress; next: 1.2–1.7
(delegated).

| Item | Status | Notes |
|---|---|---|
| 1.1 LineIndex multi-byte resume fix | **done** | boundary-advance on resume point (unclamped — offset==len queries rely on past-the-end computed_end); 3 regression tests |
| 1.2 ChildRegion::staged() pub + Panics docs | **done** | staged() pub with full doc; three accessors' Panics sections point at is_resolved()/staged() guards; test covers fresh (Some) vs finished-tree (None) |
| 1.3 Include-chain message: written reference | pending | |
| 1.4 Oracle falsifiability test | pending | |
| 1.5 format_position_with message | pending | |
| 1.6 scan_specials bounds → Err | pending | |
| 1.7 drop criterion + README bench line | pending | |
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
| 3.0 HookFailed condition (with cause field) | pending | |
| 3.1 Tier A signatures (+ lang_initial ripple) | pending | |
| 3.2 Tier B signatures (+ observe_transition sink) | pending | |
| 3.3 Tier C infallibility docs | pending | |
| 3.4 stage_invocation Err not panic | pending | |
| 3.5 ParseDriver::diagnostics_limit() | pending | |
| 3.6 ParseResult returns SessionExt | pending | |
| **Stage 3 gate** (build/test/docs + hot-path size check) | pending | |
| 4.1–4.14 doc batch | pending | 4.2 GroupRule: keep PartialEq, document |
| **Stage 4 gate** (docs build, link check) | pending | |
| 5 closure (api-baseline; rationale entries AFTER cleanup agent done; courtesy notes; delete folder) | pending | ARCHITECTURE/DESIGN_RATIONALE untouchable until cleanup agent finishes |
