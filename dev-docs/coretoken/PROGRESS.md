# `core::token` and the scan helpers — progress log

Companion to PLAN.md (normative). One section per stage; the orchestrator and the
stage agents append here. Statuses: started / implemented / reviewed / merged.

## Stage 0 — `core::token` facade (`ct-0-facade` ← `main` @ 46cc56a)

- Worktree: `.claude/worktrees/ct-0-facade`
- Status: implemented (awaiting review)
- Commits: `e446b61` (facade), `a794842` (topology lists), `2503eee` (guide links),
  `f404068` (rustdoc links, doctests, tests), plus this log entry.

### What changed, per file

- `techy/src/core/token.rs` (new, 100 lines) — the facade. Module docs in the §1.2
  order: what a token is (pointing at `TokenReader` and `Tokenization`), the placement
  rule verbatim under a "What lives here" heading, "The items, by group" (six bullets,
  all 41 items linked), "Writing a token reader" (the three ways). Two `pub use`
  blocks: 33 items from `crate::token`, the 8 `*Overrides` from `crate::state`.
- `techy/src/core/mod.rs` — `pub mod token;` added; the token `pub use` block removed
  whole and the 8 overrides dropped from the state block (25 state items remain,
  exactly §1.2's "stays" list, re-wrapped). Docs: summary line and flatness paragraph
  no longer claim the token layer, the "Tokens" bullet is gone, the state bullet's
  rules-override mention retargeted to `token::TokenRulesOverrides`, and the submodule
  list is four entries with `token` first.
- `techy/src/lib.rs` — the `core` topology bullet says "four submodules" and gains the
  `core::token` sub-bullet.
- `CLAUDE.md` — public-topology list: a `techy::core::token` bullet between
  `techy::core` and `techy::core::specs`; the `techy::core` bullet and the
  "Module organization" line drop the token items.
- `docs/ai-guide.md` — "Module topology" table: new `techy::core::token` row; the
  `techy::core` row drops "tokens" and `TokenRules`.
- `docs/panics.md`, `docs/concepts-overview.md`, `docs/custom-lang.md`,
  `docs/construct-parsers.md`, `docs/ai-guide-custom-lang.md`,
  `docs/language-syntax.md`, `docs/parsing-model.md`, `docs/parsing.md`,
  `docs/pylatexenc-migration.md`, `docs/ai-guide-pylatexenc.md` — every
  `crate::core::<moved item>` link retargeted to `crate::core::token::<item>`
  (101 replacements over 92 lines, all files together); `docs/construct-parsers.md`
  and `docs/custom-lang.md` each had one code block whose `use techy::core::{…}` mixed
  moved and unmoved items, now two `use` lines.
- `techy/src/serialize/wire/state.rs` — 10 rustdoc links to the rules types
  retargeted (no code change).
- `techy/src/token/reader.rs`, `techy/src/token/rules.rs`,
  `techy/src/token/tokenization.rs` — doctest `use` lines split only; no other change
  in either `techy/src/token/` or `techy/src/state/`.
- `techy/tests/acceptance.rs`, `techy/tests/lang_features.rs`,
  `techy/tests/recompose_oracle.rs`, `techy/tests/derive_conditions.rs` — `use` lines
  split and three `techy::core::StdTokenization` / one `techy::core::ForbiddenChar`
  path retargeted.
- `README.md` — untouched: its only `use techy::core::{Language, ParsingState};` names
  no moved item.

### Gates (final tree, `f404068`)

- `cargo build` — `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`.
- `cargo test --workspace` — `1128 passed; 0 failed; 0 ignored`, then `30`, `9`, `14`,
  `23`, `0`, `0`, `0`, `1` passed for the integration binaries, doc-tests
  `86 passed; 0 failed; 4 ignored`, `techy_derive` doc-tests
  `0 passed; 0 failed; 2 ignored`. All `test result: ok.`
- `cargo test --workspace --all-features` — `1167 passed; 0 failed; 1 ignored`, then
  `30`, `9`, `14`, `23`, `3`, `2`, `5`, `1`, doc-tests `87 passed; 0 failed; 4 ignored`,
  derive doc-tests `0 passed; 0 failed; 2 ignored`. All `test result: ok.`
- `cargo clippy --workspace --all-targets -- -D warnings` — exit 0, no warnings.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — exit 0, no
  warnings.
- `rm -rf target/doc && cargo docs --all-features` — exit 0, no rustdoc output beyond
  `Documenting`/`Finished`/`Generated` (`broken_intra_doc_links` and `missing_docs` are
  `deny`, so a stale link would have failed the build). Same for plain `cargo docs`.
  `target/doc/techy/core/token/` holds exactly 41 item pages; no token-named page is
  left under `target/doc/techy/core/`.
- §1.2 grep — `grep -rEn "core::(<41-item regex>)\b" techy/src techy/tests docs
  README.md CLAUDE.md` exits 1 (no match); `core::token::` occurs on 92 lines.
- `git diff --stat main..HEAD` — no change under `techy/src/state/`; under
  `techy/src/token/` only the three doctest `use` lines (`reader.rs` +4/-3,
  `rules.rs` +2/-1, `tokenization.rs` +3/-2), all inside `///` blocks.
- `BASELINE_REV=main sh scripts/check_semver.sh` — 4 major categories, 38 entries, all
  of them `techy::core::<moved item>` path breaks and nothing else (the 41 moved items
  minus `Token`, `StreamPosition`, `TokenResult`, which are type aliases and not
  linted). Against the frozen `api-baseline` the same script reports 19 major
  categories, dominated by breaks that predate this branch.

### Deviations from §1 (all recorded, none silent)

1. §1.2 item 4b is written without links to `StdTokenReader::scan_std_token_at` and
   `token_kind_of_std_token`: both are still `pub(crate)` in Stage 0 and
   `broken_intra_doc_links` is `deny`. The paragraph describes the delegating wrapper
   fully, and the several-sources case in prose (one inner `StdTokenReader` per source,
   the *Seams* section linked, the `Lang::OBEYS_SPAN_TILING = false` requirement named).
   Stage 1 adds the two method links per §1.3.
2. §1.2 items 4c and 5 are the one-line placeholder §2 step 1 prescribes ("The scan
   helpers are described with the functions themselves."). Consequence to fix in Stage
   1: the placement rule (recorded verbatim) names "the scan helpers" before anything
   defines the term, and the family's `pos` precondition is not stated yet.
3. The tour has six bullets as §1.2 item 3 asks, while §1.2's item list has eight
   groups: the rules data, its `*Overrides`, and the caches derived from them are one
   "Token rules" bullet with two sub-bullets (the grouping the placement rule itself
   uses). All 41 items are linked exactly once.
4. The placement rule is recorded verbatim, so its "the reader contract" survives in a
   sentence that does not state that contract — the banned-word rule for "contract" is
   waived for the verbatim text only. Elsewhere in the new doc lines the topology
   bullets say "the `TokenReader` trait" instead.
5. One topology list beyond §1.2's three was updated: `docs/ai-guide.md`'s "Module
   topology" table, which ends with "Every item has exactly one canonical public path
   (the paths above)" and would otherwise have been wrong.
6. Four doc lines were re-wrapped where `token::` pushed them past the file's ~88-column
   wrap (three regions in `docs/panics.md`, one in `docs/concepts-overview.md`). Three
   lines that consist of a single link stay 92–94 columns and were left alone
   (`docs/custom-lang.md` 326 and 332, `docs/construct-parsers.md` 80).
7. `core/mod.rs`'s state bullet keeps its rules-override mention, retargeted as
   `token::TokenRulesOverrides` (dropping it would have lost the pointer that a state
   delta carries the overrides; leaving it unqualified would have broken the link).

### Open questions

- None blocking. Two items for later stages, already in the plan: `docs/panics.md`'s
  "Five value functions and the seven span-taking …" count sentence and the
  `skip_whitespace` bullet are Stage 1 (§1.6), and the tokenization paragraphs of
  `docs/custom-lang.md` are Stage 2.

## Stage 1 — promoted methods + scan helpers (`ct-1-scan` ← `ct-0-facade`)

- Status: not started

## Stage 2 — guide and record (`ct-2-record` ← `ct-1-scan`)

- Status: not started

## Notes for the user (collected)

- semver: path breaks for the 41 moved items (soft freeze); `api-baseline` not moved.
  Measured after Stage 0 (`BASELINE_REV=main`): 38 path-break entries, all of them
  `techy::core::<moved item>`; the three type aliases (`Token`, `StreamPosition`,
  `TokenResult`) are not linted by cargo-semver-checks. Nothing else broke.
