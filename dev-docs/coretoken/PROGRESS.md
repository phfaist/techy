# `core::token` and the scan helpers — progress log

Companion to PLAN.md (normative). One section per stage; the orchestrator and the
stage agents append here. Statuses: started / implemented / reviewed / merged.

## Stage 0 — `core::token` facade (`ct-0-facade` ← `main` @ 46cc56a)

- Worktree: `.claude/worktrees/ct-0-facade`
- Status: implemented (awaiting review)
- Commits: `e446b61` (facade), `a794842` (topology lists), `2503eee` (guide links),
  `f404068` (rustdoc links, doctests, tests), `519a0ec` (`TriggerChars` wording), the
  review-round-1 fixes, plus this log entry.

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
  README.md CLAUDE.md` exits 1 (no match). `grep -rn "core::token::" techy/src
  techy/tests docs README.md CLAUDE.md | wc -l` — 106 lines (15 in `techy/src`, 13 in
  `techy/tests`, 78 in `docs`, 0 in `README.md` and `CLAUDE.md`, which name the module
  but no item path).
- `git diff --stat main..HEAD` — no change under `techy/src/state/`; under
  `techy/src/token/` only the three doctest `use` lines (`reader.rs` +4/-3,
  `rules.rs` +2/-1, `tokenization.rs` +3/-2), all inside `///` blocks.
- `BASELINE_REV=main sh scripts/check_semver.sh` — 4 major categories, 38 entries, all
  of them `techy::core::<moved item>` path breaks and nothing else (the 41 moved items
  minus `Token`, `StreamPosition`, `TokenResult`, which are type aliases and not
  linted). Against the frozen `api-baseline` the same script reports 19 major
  categories, dominated by breaks that predate this branch.

### Review round 1 — required fixes applied

The reviewer passed every gate and required four fixes; three wording improvements
were adopted with them.

1. The placement rule is now verbatim against the amended §1.2 (`ca81cff`, which
   dropped the "contract" wording): "the `TokenReader` trait and the standard reader"
   and "the `Lang` trait (its associated types and hooks)". The sentence keeps the
   `core` hub link around the word "hub"; no other word differs from the plan.
2. `README.md` (the crates.io readme) — its facade list said "three satellites" and
   put the token items in the `techy::core` bullet. The bullet now names the state and
   the engine only, the list says "four satellites", and a `techy::core::token`
   sub-bullet stands before `core::specs`.
3. The two `TokenReader` sections item 4b names are hyperlinked by anchor, verified
   against the generated page (both anchors exist and both links appear on the facade
   page).
4. This log: the corrections below and the re-measured `core::token::` count.
5. Wording: "one block per tokenization feature" (`LangFeatures` has eight members and
   `Scopes` has no rules block), "the caches a parsing state derives at each state
   transition" (`TriggerChars` comes from the `Lang::specials_trigger_chars` hook over
   the state data, not from the rules), and "spelled everywhere else through the two
   aliases" for `Token<L>`/`StreamPosition<L>` (the word `docs/custom-lang.md` uses).
6. `core/mod.rs` — the hub's top-level list has a one-line **Tokens** bullet pointing
   at the [`token`] submodule again (§1.2's "replaced by a sentence pointing at
   `token`"), and its summary line and state bullet say "`Lang` trait" / "Language
   trait and state".

### Deviations from §1 (all recorded, none silent)

1. §1.2 item 4b is written without links to `StdTokenReader::scan_std_token_at` and
   `token_kind_of_std_token`: both are still `pub(crate)` in Stage 0 and
   `broken_intra_doc_links` is `deny`. Everything else of item 4b is there: the
   delegating wrapper, and the several-sources case (one inner `StdTokenReader` per
   source, the `Lang::OBEYS_SPAN_TILING = false` requirement named), with both named
   sections of the `TokenReader` page hyperlinked by anchor —
   `TokenReader#writing-a-reader-over-standard-tokens` and
   `TokenReader#seams--readers-that-serve-several-sources-at-one-nesting-level`.
   Stage 1 adds the two method links per §1.3.
2. §1.2 items 4c and 5 are the one-line placeholder §2 step 1 prescribes ("The scan
   helpers are described with the functions themselves."). Consequence to fix in Stage
   1: the placement rule (recorded verbatim) names "the scan helpers" before anything
   defines the term, and the family's `pos` precondition is not stated yet.
3. The tour has six bullets as §1.2 item 3 asks, while §1.2's item list has eight
   groups: the rules data, its `*Overrides`, and the caches derived from them are one
   "Token rules" bullet with two sub-bullets (the grouping the placement rule itself
   uses). All 41 items are linked exactly once.
4. One topology list beyond §1.2's three was updated: `docs/ai-guide.md`'s "Module
   topology" table, which ends with "Every item has exactly one canonical public path
   (the paths above)" and would otherwise have been wrong.
5. Four doc lines were re-wrapped where `token::` pushed them past the file's ~88-column
   wrap (three regions in `docs/panics.md`, one in `docs/concepts-overview.md`). Three
   lines that consist of a single link stay 92–94 columns and were left alone
   (`docs/custom-lang.md` 326 and 332, `docs/construct-parsers.md` 80), as does the
   96-column section link added by review fix 3 (`core/token.rs` 77).
6. `core/mod.rs`'s state bullet keeps its rules-override mention, retargeted as
   `token::TokenRulesOverrides` (dropping it would have lost the pointer that a state
   delta carries the overrides; leaving it unqualified would have broken the link).

### Open questions

- None blocking. Two items for later stages, already in the plan: `docs/panics.md`'s
  "Five value functions and the seven span-taking …" count sentence and the
  `skip_whitespace` bullet are Stage 1 (§1.6), and the tokenization paragraphs of
  `docs/custom-lang.md` are Stage 2.

## Stage 1 — promoted methods + scan helpers (`ct-1-scan` ← `ct-0-facade` @ 35eab33)

- Worktree: `.claude/worktrees/ct-1-scan`
- Status: implemented (awaiting review)
- Commits: `dababe4` (the two promoted methods), `ddc5721` (`scan.rs` with
  `skip_whitespace` moved), `21c6e00` (the helpers and match types), `a61f077` (the
  dispatcher recomposed), `ccf006c` (`docs/panics.md`), `4935a3b` (the helper tests),
  `8809724` (the facade module docs), `1f4cae0` (a stale mention of a deleted method
  name), plus this log entry.

### What changed, per file

- `techy/src/token/scan.rs` (new, 1335 lines) — the scan-helper file. `skip_whitespace`
  and its private companion `paragraph_continues` moved in unchanged, then the six
  helpers and three match types of §1.4: `scan_paragraph_break`,
  `scan_group_delimiter` + `GroupDelimiterMatch` (with `span()`/`rule()` and
  hand-written `Clone`/`Copy`/`Debug`/`PartialEq`/`Eq` carrying no `L:` bounds, as
  `TokenKind`'s do), `command_rule_at`, `scan_command` + `CommandMatch`, `scan_comment`
  + `CommentMatch`, `scan_specials_trigger`. Two private items: `check_pos`, which
  raises the family's `pos` panic in one place with one wording, and
  `checked_scan_error`, the span-validating half of the reader's former
  `lift_specials_scan_error`. Its `mod tests` holds 44 tests (the three moved
  `skip_whitespace` ones included).
- `techy/src/token/reader.rs` (−196 lines net) — `scan_token_at` →
  `pub fn scan_std_token_at` and `token_kind_of` → `pub fn token_kind_of_std_token`,
  both with the docs of §1.3 (priority order, where `start` may point, which failures
  carry a recovery; who needs the interpretation and how a foreign token reads).
  `scan_std_token_at`'s body is the composition of §1.5, and the five private methods
  are deleted. The `TokenReader` docs' *Writing a reader over standard tokens* section
  gains the sentence pointing at the two methods; the module docs say the scanning core
  is composed of the scan helpers instead of private methods.
- `techy/src/token/mod.rs` — `mod scan;` added; `skip_whitespace` re-exported from
  `scan` instead of `reader` (public path unchanged), together with the nine new items.
- `techy/src/core/token.rs` — the nine new items exported flat; module docs completed:
  the definition of "scan helper" right after the placement rule, the two promoted
  methods named and linked in way (b), way (c) written out (the seven helpers with
  their match types, the composition sentence, the three steps that are a line each
  rather than a helper), and a new closing section stating the family's `pos`
  requirement once.
- `techy/src/token/scripted_reader.rs` — the three call sites renamed (one `match`
  re-wrapped to stay inside the file's line width).
- `docs/panics.md` — the `skip_whitespace` bullet becomes the family bullet of §1.6;
  the lead sentence counts precisely and its justification clause now also covers a
  helper that answers about the content it was handed.
- `techy/src/constructs/nodes_parser.rs` — one doc comment that pointed at
  `detect_group_delimiter` now links `core::token::scan_group_delimiter`.

### Gates (final tree, `1f4cae0`)

- `cargo build` — `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`.
- `cargo test --workspace` — `1169 passed; 0 failed; 0 ignored` for the lib, then `30`,
  `9`, `14`, `23`, `0`, `0`, `0`, `1` for the integration binaries, doc-tests
  `86 passed; 0 failed; 4 ignored`, `techy_derive` doc-tests `0 passed; 0 failed; 2
  ignored`. All `test result: ok.` (Stage 0's lib count was 1128; +41 = the 44 new
  `scan.rs` tests less the 3 that moved out of `reader.rs`.)
- `cargo test --workspace --all-features` — `1208 passed; 0 failed; 1 ignored`, then
  `30`, `9`, `14`, `23`, `3`, `2`, `5`, `1`, doc-tests `87 passed; 0 failed; 4 ignored`,
  derive doc-tests `0 passed; 0 failed; 2 ignored`. All `test result: ok.`
- `cargo test --lib token::scan` — `44 passed; 0 failed; 0 ignored; 1125 filtered out`.
- `cargo clippy --workspace --all-targets -- -D warnings` — exit 0, zero warning or
  error lines. Same with `--all-features`.
- `rm -rf target/doc && cargo docs --all-features` — exit 0, no rustdoc output beyond
  `Documenting`/`Finished`/`Generated`. Same for plain `cargo docs`.
  `target/doc/techy/core/token/` holds 50 item pages (41 + the 9 new ones), the seven
  `fn.*.html` pages are the helper family, and the facade page's links to
  `StdTokenReader#method.scan_std_token_at` and `#method.token_kind_of_std_token`
  resolve.
- The seven superseded names, grepped whole-word over `techy/src`
  (`grep -rnw "detect_paragraph_break\|detect_group_delimiter\|read_command\|
  read_comment\|lift_specials_scan_error\|scan_token_at\|token_kind_of"`, written on
  one line): exit 1, no match. Likewise over `docs README.md CLAUDE.md techy/tests`.
- The Stage-0 §1.2 grep (`core::<moved item>` over `techy/src techy/tests docs
  README.md CLAUDE.md`) still exits 1.
- Banned-word grep over every added line of `git diff ct-0-facade..HEAD` — no match.
  The word "contract" appears only in the `# Panics` sections (which state the
  requirement in the same sentence, the wording `skip_whitespace` already used) and in
  comments moved verbatim from the deleted method bodies.
- `git diff ct-0-facade..HEAD -- techy/src/token/reader.rs` inside `mod tests`: two
  hunks only — the import list (three names the file no longer needs outside its tests)
  and the removal of the three `skip_whitespace` tests that moved to `scan.rs`. No test
  body was edited; the two renamed methods are not called from `reader.rs`'s tests at
  all (they go through the `TokenReader` trait).

### Decisions under §1.8

D1, D2, D4, D5, D6, D7, D8 as written; no deviation. D3 is implemented as specified and
flagged below. On D7 no doctest was added — the optional one was not worth the fixture a
helper example needs (a `TokenRules` value), and the module docs link the compiling
examples the `TokenReader` and `Tokenization` pages already carry.

### Deviations from §1 (all recorded, none silent)

1. **The family panic is raised up front, by a shared `check_pos`.** §1.4 requires an
   invalid `pos` to panic in all builds but does not say where; the moved bodies would
   have panicked from a `content[pos..]` slice, with std's wording, and only in the
   branches that reach a slice (a helper whose feature is disabled returns before
   touching the content). One private `check_pos` at the top of each of the six new
   helpers makes the panic unconditional and gives the whole family the wording
   `skip_whitespace` already had ("out of bounds or not a char boundary"), which is what
   the `#[should_panic(expected = "char boundary")]` tests match on. `skip_whitespace`
   itself is untouched (§1.4: it moves unchanged), so it alone still panics only when
   whitespace handling is on.
2. **`reader.rs`'s `mod tests` import list changed** (two lines). Deleting the five
   private methods left `TokenRules`, `CommandRule`, `EndOfStreamAfterEscape` and
   `SpecialsScanError` unused in the file's own code; their rustdoc links are now
   written with an explicit `super::` target, and the test module — which used them
   through `use super::*` — imports the three its fixtures need. No test body was
   touched. §3's gate wording ("no edits inside `mod tests` other than the two method
   renames") did not anticipate this; the alternative was keeping four imports alive
   with an `allow` attribute.
3. **The three `skip_whitespace` tests moved out of `reader.rs`** — prescribed by §3
   step 2 and D6, and therefore also an edit inside `mod tests`. Their bodies are
   unchanged; `scan.rs`'s own fixtures (a `TrivialLang`-based `TestLang` and the same
   `latex_rules`) replace `reader.rs`'s.
4. **`docs/panics.md`'s lead sentence lost the word "value"**: "Four span and position
   functions, the seven scan helpers of `core::token`, and the seven span-taking
   `StdToken` constructors". Its justification clause also gained "or answer about the
   content they were handed rather than about a mistake in calling code", since three of
   the seven helpers do have an error channel and "deliberately infallible" would have
   been wrong about them. §1.6 asked for the count; this is the same sentence made
   true.
5. **One file outside §3's list was edited**: `techy/src/constructs/nodes_parser.rs`,
   whose `group_close_type` doc comment named `detect_group_delimiter`. Leaving it would
   have kept a superseded name alive and failed the stage's own grep.

### Open questions and flags for the user

- **D3 (flag required by §1.8).** `scan_command` asserts, in all builds, that
  `rule.escape_char` stands at `pos`, with the message "the rule's escape character 'X'
  does not stand at pos N". The panic exception the user granted names only an invalid
  `pos`, so this second precondition is the one point of Stage 1 that goes beyond the
  wording of the grant. It is documented in the function's `# Panics` section and in the
  `docs/panics.md` family bullet, and one test covers it. The fallback if the user
  objects stays available: `-> Option<Result<CommandMatch, EndOfStreamAfterEscape>>`
  with `None` for the mismatch.
- Deviation 1 above (the up-front `check_pos`) is an implementation choice inside §1.4's
  rule, not a design question, but it does make the family's panic behavior uniform in a
  way the plan text left open — worth a glance.
- `skip_whitespace` states its panic in its opening prose rather than under a `# Panics`
  heading, which is where the six new helpers state theirs. §1.4 requires its docs to
  move unchanged, so it was left alone; giving it the heading too would make the family
  read alike on the rendered pages, and is a one-line change if wanted.
- `CLAUDE.md`'s `techy::core::token` bullet lists `skip_whitespace` by name and does not
  yet mention the rest of the helper family. §1.6 assigns that file to Stage 0, so it was
  not touched here; Stage 2 could add three words.
- No semver measurement was re-run for this stage: it adds public items and widens two
  methods' visibility (both minor), and breaks no path. Stage 0's note stands.

## Stage 2 — guide and record (`ct-2-record` ← `ct-1-scan`)

- Status: not started

## Notes for the user (collected)

- semver: path breaks for the 41 moved items (soft freeze); `api-baseline` not moved.
  Measured after Stage 0 (`BASELINE_REV=main`): 38 path-break entries, all of them
  `techy::core::<moved item>`; the three type aliases (`Token`, `StreamPosition`,
  `TokenResult`) are not linted by cargo-semver-checks. Nothing else broke.
