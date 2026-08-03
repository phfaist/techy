# Phase 3 — S1 report: topology + mechanical renames

Branch `phase3-s1-topology` (worktree, branched from `api-review` @ 9858949).
Stage: pure relocation/renaming, zero behavior change. All gates green.

## What was done

1. **Renames** (commit b853159): `SimpleLang` → `TrivialLang` everywhere (trait,
   blanket impl, ~10 cfg(test) impl sites, all rustdoc mentions), with the ruled
   repositioned rustdoc contract ([§dd-dr:trivial-lang]): the trivial language, for
   tests and machinery experiments; the default driver resolves nothing; any
   customization means implementing `Lang` directly. Adjacent "quick-start"
   on-ramp phrasing removed from `Lang`/`ClosedVocabulary` doc mentions. Free
   `resolve_source` → `resolve_source_reference` (same signature/behavior; Tier-C
   R3); `Language::resolve_source` method untouched (leaves in a later stage).
2. **Wire-identifier slate** (commit 2nd): all 16 renamed rows of the frozen
   T4_RULINGS §A table applied (attribute strings in the `#[diagnostic(id = …)]`
   derives); the 6 KEEP rows untouched. Verified: no test or doc asserts any of
   the renamed strings, so no test updates were needed (the only identifier-
   asserting tests use `test.derive.*` / `test.engine.*` locals). The new
   `core.sources.*` conditions are NOT added (S6).
3. **Derive `__private` rider** (commit 3rd): `__private` gains
   `pub use crate::error::{DiagnosticInfo, DiagnosticValue, ToDiagnosticValue}`
   (beside the existing `String`, `Vec`); all 9 `::techy::error::…` emission
   sites in techy-derive (to_value.rs, diagnostic_info.rs) now emit
   `::techy::__private::…` only. `::core::…` paths in emitted Display impls are
   extern-crate absolute and unaffected. Derive tests (techy/tests/
   derive_conditions.rs) pass unchanged in expansion behavior.
4. **C5 facade topology** (commit a3b9086): internal topic modules
   (`constructs`, `engine`, `node`, `scopes`, `spec`, `state`, `token`) flipped
   to `pub(crate)`; ALL root `pub use` re-export blocks deleted; new facades
   `techy::core` (src/core/mod.rs), `techy::core::{specs, constructs, node}`
   (src/core/{specs,constructs,node}.rs) built as pure `pub use crate::…`
   re-export files with fresh module docs; `src/node/extract.rs` physically
   moved to `src/extract.rs` (`techy::extract`); `techy::source`,
   `techy::error`, `techy::latexlike` unchanged as their own facades. Root
   keeps only the facade modules, `VERSION` (with the ruled rustdoc sentence:
   Cargo package version, always valid semver), `#[doc(hidden)] __private`, and
   the `cfg(doc)` `guide` module. `NodeData` demoted to crate-internal (not
   re-exported; Tier-C Theme C). `NoResolver` kept pub in `techy::source`
   (deleted only in S2); `check_tree_invariants` kept pub in `core::node`
   (demotion rides S3's `validate_tree`).
5. **Docs churn** (same commits + docs commit): all doctest/`use` paths in src,
   techy/tests, and docs/ guide pages rewritten to the new topology; broken
   intra-doc links fixed (guide pages' `crate::<old module>::…` links, `spec/
   structure.rs` module links, `node/slice.rs` `super::extract`); lib.rs crate
   docs rewritten to the role-based module map; CLAUDE.md "Key Architecture" +
   "Module organization" blocks updated; ARCHITECTURE.md topology paragraph
   flipped decided→applied plus two current-fact path corrections; two
   DESIGN_RATIONALE status lines updated (see Deviations).
6. **Superseded-names register**: verified complete for S1 — the 2b sessions
   already recorded `SimpleLang`, `resolve_source` (via the Tier-C block),
   the file-named identifier areas, and the topology's rejected names. No
   additions were needed; no duplicates introduced.

## Old-path → new-path mapping

Internal file layout is unchanged except src/node/extract.rs → src/extract.rs.
Public paths:

| Old public path(s) | New canonical path |
|---|---|
| `techy::source::*` + root mirrors | `techy::source::*` (root mirrors deleted); `resolve_source` → `resolve_source_reference` |
| `techy::error::*` + root mirrors (incl. both derive macros) | `techy::error::*` |
| `techy::node::extract::*` | `techy::extract::*` |
| `techy::state::*` + root mirrors (`SimpleLang` → `TrivialLang`) | `techy::core::*` |
| `techy::token::*` + root mirrors (+ `PrefixEntry`, previously module-only) | `techy::core::*` |
| `techy::engine::{Language, ParseResult, ParserSession, ParseDriver, StdParseDriver, Frame, FrameTitle}` + root mirrors | `techy::core::*` |
| `techy::spec::FrameRole` + root mirror | `techy::core::FrameRole` |
| `techy::spec::{CallableSpec, StdCallableSpec, ArgumentSpec}` + root mirrors | `techy::core::specs::*` |
| `techy::scopes::*` (all 16) + root mirrors | `techy::core::specs::*` |
| `techy::engine::{CommandResolution, ResolvedCallable}` + root mirrors | `techy::core::specs::*` |
| `techy::constructs::*` (all 52) + root mirrors | `techy::core::constructs::*` |
| `techy::spec::{ArgumentParser, ParsedArgumentNodes}` + root mirrors | `techy::core::constructs::*` |
| `techy::node::*` (minus extract) + root mirrors, incl. the 8 ext aliases | `techy::core::node::*` |
| `techy::node::NodeData` (+ root) | — (crate-internal) |
| `techy::latexlike::*` | unchanged |
| root `VERSION`, `__private`, `guide` | unchanged |

Wire identifiers: the 16 renames exactly per T4_RULINGS §A (areas `specs`,
`groups`, `environments`, `arguments`, `recovery`, `verbatim`); 6 KEEPs
untouched; R2 include-depth row already dropped by ruling; `core.sources.*`
deferred to S6.

## Audit outcome

Script over `target/doc/techy/**` real item pages (redirect stubs filtered):

- **202 item pages** (was 203; the delta is exactly the `NodeData` demotion),
  **zero duplicate public paths** (no ident+kind reachable at two paths).
- Per-module counts match the ruled roster exactly:
  root 1 (`VERSION`) · `core` 37 · `core::constructs` 54 · `core::node` 29 ·
  `core::specs` 21 · `error` 14 (incl. 2 derive macros sharing trait paths) ·
  `extract` 9 · `latexlike` 23 · `source` 14.
- Item-by-item diff against INVENTORY.md as amended by the Tier-C/T3/T4
  rulings: **no missing items, no extra items**. The only differences from the
  pre-S1 surface are the ruled renames (`TrivialLang`,
  `resolve_source_reference`) and the ruled demotion (`NodeData`).
- Superseded-name sweep over src/tests/docs: no `SimpleLang`, no bare free
  `resolve_source`, no old identifier areas remain.

## Gate results (run in the worktree)

| Gate | Command | Result |
|---|---|---|
| Build | `cargo build` | PASS (no warnings) |
| Tests | `cargo test` | PASS — 534 + 30 + 8 + 1 + 25 unit/integration + doctests, 0 failed (4 pre-existing ignored) |
| Docs | `rm -rf target/doc && cargo docs` | PASS — zero broken-link errors, zero warnings (no new missing_docs) |
| Audit | script (above) | PASS — exact roster, one canonical path each |

## Deviations / notes

- **PHASE3_PLAN.md** was read from the primary checkout (it is not committed on
  `api-review`, so it is absent from this worktree).
- **`TokenResult`** (type alias) is not listed in the work order's hub
  parenthetical but is a public token item in INVENTORY with no demotion ruling
  (Tier-C tally: only `NodeData`/`check_tree_invariants` demoted, `NoResolver`
  removed later). Kept pub → `techy::core::TokenResult` under "all of token".
- **DESIGN_RATIONALE status lines**: two entries printed pending-application as
  current fact after this stage ([§dd-dr:public-namespace-topology] "application
  pending", [§dd-dr:wire-identifier-stability] "slate is pending"). Both got
  minimal status corrections (decided→applied, Phase 3 S1); no prose rewritten,
  no labels touched. Flagging since the mandate said "path corrections" — revert
  is trivial if unwanted.
- **Historical `SimpleLang` mentions** in DESIGN_RATIONALE decision-history
  prose (~9 entries) were left as-is per the register discipline (the rename is
  recorded at [§dd-dr:trivial-lang] and in superseded-names).
- **Module-header narratives**: the old public modules' long rustdoc headers
  (token, state, spec, scopes, engine, constructs, node) now render nowhere
  publicly (their modules are private). The facades carry fresh, more concise
  module docs; the detailed internal headers remain in the source files for
  developers. Phase 4 guide work may want to promote more of that text.
- **Sandbox note**: one `cargo fetch` was run outside the sandbox (dev-deps not
  in the local cargo cache; registry dir is sandbox-read-only). No other
  unsandboxed commands.

## Churn stats

- 44 files changed, +426/−317 lines; 6 commits.
- ~118 `use techy::…` line edits (src doctests + integration tests + guide
  pages), matching NAMESPACE_OPTIONS' ~97-line estimate plus the derive/root
  cleanup.
- techy-derive: 9 emitted-path edits + 1 `__private` extension.
- 16 wire-identifier attribute strings.

## Noticed for later stages

- S2: `NoResolver` deletion (still referenced by `Language::new` internals and
  source docs); `CommandResolution::resolve_via_scopes` removal + free
  `resolve_command_in_scopes` extraction; `Language` init reshape;
  `StdParseDriver::default()` removal (a doctest at engine/driver.rs:342 uses
  `StdParseDriver::default()` — it will need the S2 spelling).
- S3: `check_tree_invariants` → pub(crate) wrapper over `validate_tree`;
  the 8 per-kind ext aliases in `core::node` are removed by the ext-minting
  ruling (they were kept pub here per zero-behavior-change).
- S6: `core.sources.no-resolver` / `core.sources.unresolvable-reference`
  conditions.
- The `guide` module's "parsing model" page is still a stub (pre-existing).
