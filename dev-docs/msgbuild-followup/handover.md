# Handover: the message follow-up build

## State

- Branch: `claude/pensive-mayer-dqtd5i` (not pushed by the build).
- Start commit: `208cd98`. Last commit before this handover: `8572857`.
- 21 commits, 44 files changed. The tree is clean.

## Decisions and their status

Every decision is DECIDED; none is PROPOSED.

| Id | Status | Executed by |
|---|---|---|
| F1 | DECIDED, option (a), with the full variant list | unit F1 |
| F2 | DECIDED, option (a), all seven carriers (breaking) | unit F2 |
| F3 | DECIDED, option (b), with `NotFound` | unit F3 |
| F4, F5, F7, F8, F9 | DECIDED, option (a): do not add | no work (recorded as declined) |
| F6 | DECIDED, option (a) | unit F6 |
| B1 | DECIDED, option (a) | unit B1 |
| B2 | DECIDED, option (a) (breaking) | unit B2 |
| B3 | DECIDED, option (a) | unit B3 |
| B4 | DECIDED, option (a) | unit B4 |
| B5, B6 | DECIDED, option (a): keep | no work (recorded) |
| T1 | DECIDED, option (a), by its ruling | unit TS |
| T2 | DECIDED, option (a): no test | unit TS (nothing to do) |
| T3 | DECIDED, option (a), "keep no" | unit TS |

## Commits per unit

- F1: `1235daa`, `f50682b`, `5782300`, `9807827`
- F2: `0ea2b31`, `3fff576`
- F3: `a1d43b0`, `4c8775d`
- B2: `417b6c9`, `6bd195c`
- F6: `e6b42a5`, `86ab4ef`
- B1: `e3d9bd7`, `2cc19ad`
- B3: `3a0fc5f`
- B4: `3091690`, `8c18727`
- TS: `b167221`, `e3af9f7`
- Records: `6696ebc`, `8572857`

## What was done

- F1: `ImplementationErrorKind` (11 variants) on `ImplementationError`; every library place carries the kind of the decision's table; 11 tests assert the kind.
- F2: `DescentLimit` on `DescentRefusal`, `DescentWarning` and the seven carriers; 15 tests check the limit; one test of its serialized form.
- F3: `ResolveErrorKind { Other, NotFound, IncludeCycle, IncludeDepthExceeded }` with `kind()`/`with_kind()`; `MapResolver` sets `NotFound`.
- B2: `check_include_chain` takes `reference: &str` first; both errors carry it.
- F6: `DeserializeError::AnnotationTypeMismatch` replaces the `Failed` of `tree_of_object`.
- B1: one table bound (u32::MAX entries) on both sides, through `table_len_after`; one guarding test.
- B3: one private helper `dropped_replaced_by` at both places; the unreachable arm is gone.
- B4: a placeholder `Command` token is reported as `UnusableRecoveryToken` (kind `Command`) and parsed as chars; one test.
- TS: T1 by its ruling; T3's two corrections (message comparison dropped in the resolver test, bounded loop in `visit.rs`).
- Records: `[§dd-dr:error-source-fields]` in DESIGN_RATIONALE, referenced from ARCHITECTURE; stale statements corrected in place.

## Changes of the public API

Breaking (marked **B**):

- **B** `techy::core::DescentRefusal`, `techy::core::DescentWarning`: new public field `limit`.
- **B** `techy::core::constructs::DescentLimitExceeded::new`, `DescentLimitApproaching::new`: gain the parameter `limit`.
- **B** `DescentLimitExceeded` variants of `techy::visit::WalkError`, `techy::transform::RestageError`, `techy::recompose::RecomposeError`, `techy::serialize::SerializeError`, `techy::serialize::DeserializeError`: gain the field `limit`.
- **B** `techy::source::check_include_chain`: new first parameter `reference: &str`; its errors carry the reference as written.

Not breaking:

- New: `techy::core::DescentLimit`; `techy::core::constructs::ImplementationErrorKind`; `ImplementationError::kind`, `::with_kind` (`new` keeps its signature); `ParseContext::implementation_error_with_kind`; `techy::source::ResolveErrorKind`, `ResolveError::kind`, `::with_kind`; `DeserializeError::AnnotationTypeMismatch`; `UnusableRecoveryTokenKind::Command`.

## Changes of serialized data

Keys only grow; no golden and no worked example changed.

- `core.constructs.implementation-error`: new key `kind` (kebab-case variant name).
- `core.constructs.descent-limit-exceeded` and `descent-limit-approaching`: new key `limit`, a map `{kind, bytes | levels}`.
- The `error` of `UnresolvableSourceReference` (a `ResolveError`): new key `kind` between `message` and `cause_chain`.
- `core.recovery.unusable-recovery-token`: `kind` gains the value `command`.
- The `reference` of a serialized include-chain `ResolveError` is the reference as written.

## Checks

All passed once, no fixer needed: `cargo test --workspace` (1507 passed), with `--all-features` (1559 passed); both clippy runs with `-D warnings`; `cargo docs --all-features` after `rm -rf target/doc`; both wasm32 checks; `cargo +1.86.0 check --workspace --all-features --all-targets`.

- Not run: `sh scripts/check_semver.sh` (no branch `api-baseline`, no `cargo-semver-checks` in the container). Philippe runs it locally; it should report the breaking changes of F2 and B2 listed above.
- Preexisting: the 20 dead-code warnings of the 1.86 check in test code (`techy/src/state/features.rs`, `techy/tests/lang_features.rs`), identical to the baseline.

## Open points for Philippe

1. **ExtMintFailed outside the staging helper.** At `latexlike/environments.rs` (the chars fallback after `MalformedBegin`, about line 949) and `engine/language.rs` (`session.finish`, about line 470), every `NodeBuildError`, `ExtMintFailed` included, becomes an `ImplementationError` of kind `NodeBuildFailed`; elsewhere `ParseContext::staging_error` turns `ExtMintFailed` into `HookFailed`. `staging_error` is `pub(crate)`, so a custom parser (and the closing example of `docs/construct-parsers.md`) must write the split by hand. Options: (a) call `cx.staging_error` at the environments site and check whether `finish` can return `ExtMintFailed`; (b) also make `staging_error` public, or state on `stage_node` that the split is the caller's job; (c) leave it. Recommendation: (a) now, (b) as a separate decision (public API).
2. **Stale guide example.** `docs/construct-parsers.md`, about line 383, still gives "a NodeBuildError from a staging call" as the use of `implementation_error` (kind `Other`), which conflicts with `NodeBuildFailed` and with the guide's own closing example. Options: (a) a documenter pass that points to `implementation_error_with_kind`; (b) leave. Recommendation: (a), aligned with point 1.
3. **Descent messages from `limit`.** The descent conditions still embed the guard's prewritten `detail`. Options: (a) leave; (b) a follow-up decision to compose the messages from `limit` and the stack use. Recommendation: (a) for now.
4. **Wire strings of `ResolveErrorKind`.** Only `other` is checked in the serialized form; the other three kebab-case strings are hand-written and unguarded. Options: (a) leave; (b) one assertion on the projected `kind` of a non-`Other` error in an existing test. Recommendation: (a), unless the wire strings should be guarded.
5. **Records nits not applied** (the fixer applied the two should-fix findings only), in `[§dd-dr:error-source-fields]` and `[§dd-dr:message-wording]`: the changelog-like "Breaking: …" and "(breaking)" notes; "tracked as a follow-up" in `[§dd-dr:message-wording]`; Rejected alternatives that omit F3 without `NotFound`, B2 (b), B1 (b), T2 (b)/(c). Options: (a) apply them in a short dev-docs pass; (b) leave. Recommendation: (a).
6. **Smaller documentation proposals.** `TokenRecovery::token` (`techy/src/token/error.rs`) could say which placeholder kinds `NodesParser` accepts; the `# Errors` section of `SerdeSession::push_segment` could become a list; the `DirectoryResolver` example of `docs/specs.md` could map a missing file to `ResolveErrorKind::NotFound` (kept on `new` as ruled). The field `AnnotationTypeMismatch::stored` is in practice always `Some`; the decided shape keeps `Option`. Recommendation: a later documentation pass; no design change.

## Units that returned "not-needed", "blocked" or "partial"

None. All nine units returned "done".

## Leftover changes stashed

No agent of this build stashed anything. `git stash list` shows one older entry, `stash@{0}: msgbuild leftover` (2026-11-13, before this build; `techy/src/constructs/environment_parser.rs`, 58 insertions, 27 deletions), left by the earlier message build. It was not touched.

## `// ###` notes found

None reported by the agents; a grep of `techy/src` and `docs` finds none.

## How to continue

1. Review the branch and push it (the build did not push).
2. Run `sh scripts/check_semver.sh` locally; expect exactly the breaking changes listed above.
3. Rule on open points 1 to 5; point 1 needs a decision before any code change.
4. Decide what to do with the older stash `msgbuild leftover`.
