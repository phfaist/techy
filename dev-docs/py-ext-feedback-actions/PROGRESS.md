# Execution progress log

**Purpose: interruption recovery.** Update as items complete and always at stage
boundaries. A fresh session must be able to continue from this file + `git log`
alone. Keep the "Resume here" line current; record any deviation from PLAN.md and
any execution-time micro-ruling in the notes column (with the commit that took it).

Commit convention: small per-item commits on this branch
(`worktree-py-ext-feedback-plan`), messages prefixed `pyext-<item>:`.

**Resume here:** EXECUTING (started 2026-08-10 on user signal; branch rebased onto
main at d52fe4c — dev-docs cleanup landed). Stage 1 COMPLETE (gate green); next:
Stage 2 (2.1–2.8).

| Item | Status | Notes |
|---|---|---|
| 1.1 LineIndex multi-byte resume fix | **done** | boundary-advance on resume point (unclamped — offset==len queries rely on past-the-end computed_end); 3 regression tests |
| 1.2 ChildRegion::staged() pub + Panics docs | **done** | staged() pub with full doc; three accessors' Panics sections point at is_resolved()/staged() guards; test covers fresh (Some) vs finished-tree (None) |
| 1.3 Include-chain message: written reference | **done** | hand-written Display (derive can't call error.message()); micro-ruling: typographic quotes ‘…’ matching sibling NoSourceResolver, tail = ResolveError::message() not full Display (would repeat the prefix); rewriting-resolver test added |
| 1.4 Oracle falsifiability test | **done** | drops the middle \emph node via restage (Emit(vec![])), reemits "one  three" from "one \emph{two} three" — span-copying would resurrect the dropped bytes |
| 1.5 format_position_with message | **done** | fallback now "@ char pos N (no line info)" — no cause named; assertion updated; no other test pinned the old wording |
| 1.6 scan_specials bounds → Err | **done** | content.get(pos..) guard in Package::scan_specials; spelling = Custom+ImplementationError (the token reader's shipped match-end-validation precedent — no new TokenErrorKind variant needed); micro-rulings: guard runs before the visibility gate (caller bugs not maskable by mode), error span clamped to content.len() (stays liftable to SourceSpan); pos contract documented on trait method + impl; only in-crate impl with the slice (ScopeStack folds, defaults ignore pos; test-only impls out of scope); 3 tests |
| 1.7 drop criterion + README bench line | **done** | criterion removed from dev-deps; README's whole Performance section removed (it contained only the cargo bench instructions — an empty header would remain otherwise); Cargo.lock untracked, nothing else |
| **Stage 1 gate** (build/test/docs) | **green** | cargo build clean; cargo test all suites 0 failed (lib 783, integration 30+22+13+8, doc-tests 68 passed/3 ignored); cargo docs from fresh target/doc, zero warnings |
| 2.1 + Any on four traits | **done** | +Any on SpecsProvider/ArgumentParser/EnvironmentBehavior/SourceResolver; CallableSpec rationale generalized, short pointer note on each trait; stale structure.rs no-Any comment removed; micro-ruling: SourceResolver's borrowed/boxed forwarding impls take the `'static` the Any supertrait forces (`&'static R`, `Box<R: 'static>`) — the plan's accepted non-'static break; 4 downcast round-trip tests |
| 2.2 Language::new Into<Arc> | **done** | `initial_state: impl Into<Arc<ParsingState<L>>>`; every by-value call site compiles unchanged; doc sentence on identity preservation; Arc::ptr_eq test via a parsed node's state |
| 2.3 NodeTree::slice(range) validated | **done** | O(1) parent-table check (parent(start)'s children block must contain range.end); root only as 0..1; in-bounds empty ranges answer Some (incl. start==node_count); NodeSlice::new stays pub(crate); no-TreeTag caveat documented like nodes_in; round-trip/cross-parent/root/empty/out-of-bounds tests over the hand-built example tree |
| 2.4 MacroSpec::with_after_effect | **done** | builder + private `after_effect` field; make_invocation_parser wraps StdInvocationParser, returns boxed delta (post-descent shape); both AfterEffectSpec test copies replaced by the public path — engine's finding #1–#3 tests ported from the private MacroLang scaffolding to Latexlike (MacroLang/MacroDriver deleted; finding #3 seeds via lang_initial_with_packages().derived() to keep the single-`]` premise); micro-ruling: the private field ends external MacroSpec struct-literal construction (new/default/builder cover it); tests: sibling effect + scoping, two-effect merge, persist_state on/off (existing, now public-path) |
| 2.5 TreeViolationKind/TokenKind as_str | **done** | both `pub const fn`, bare variant name (data-carrying variants answer the name only); exhaustive-match (no `_` arm) test per enum; micro-ruling: written as `const fn` although the NodeKind::as_str pattern is plain `fn` (plan text named const; strictly more general, no cost) |
| 2.6 NodeTree::tree_tag() pub | **done** | visibility flip + doc sentence naming the NodeId pre-check use before the always-on `node()` assert |
| 2.7 TreeViolation::new + MalformedBegin | **done** | TreeViolation::new(node, kind) (struct stays #[non_exhaustive]); no_constructor dropped from MalformedBegin (the only shipped condition carrying it — test-only conditions keep theirs); construct-and-match + Display test; MalformedBegin::new() doc-test |
| 2.8 DiagnosticInfo::identifier() | **done** | defaulted method answering Self::IDENTIFIER; blanket DiagnosticData forwards to the method; docs scope the override to binding/embedding adapter types per ruling (const stays required, remains the type's own identity); sealing comment softened; adapter round-trip + shipped-conditions-unaffected tests |
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
