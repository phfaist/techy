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
   whitespace handling is on. *(Superseded in Stage 2, deviation 1: it now calls
   `check_pos` up front like the other six.)*
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

## Stage 2 — guide and record (`ct-2-record` ← `ct-1-scan` @ 5940d3b)

- Worktree: `.claude/worktrees/ct-2-record`
- Status: implemented (awaiting review)
- Commits: `20695c8` (`docs/custom-lang.md`), `433a9b1` (`CLAUDE.md`), `2e999ae` (the
  three adopted Stage 1 advisories), `8c90d6d` (ARCHITECTURE), `aa5651d` + `abbb8f0` +
  `591eb17` (DESIGN_RATIONALE: the entries, a banned word dropped, a wording polish),
  `6fdba4b` (the superseded-marker on Stage 1's `skip_whitespace` note), `f6d129d`
  (the review-round-1 fixes), plus this log entry.
- `TODO_Big.md` was **not** touched (user has uncommitted edits to it): what §4 would
  have deferred there is in "Notes for the user" below and in the two new entries'
  Revisit clauses.

### What changed, per file

- `docs/custom-lang.md` — the tokenization-behavior paragraph gains two sentences after
  the "Keeping the standard token type …" one: the several-sources case (a token type of
  one's own wrapping standard tokens, one inner `StdTokenReader` per source,
  `scan_std_token_at` to read and `token_kind_of_std_token` to interpret, with the
  `TokenReader` *Seams* section linked by anchor) and the own-token-kinds case (the scan
  helpers, defined in passing as "free functions that each recognize one construct at a
  position and return its byte spans", linking `core::token`'s *Writing a token reader*
  section). Both anchors were checked against the generated pages.
- `CLAUDE.md` — the `techy::core::token` bullet names the seven scan helpers and their
  three match values; `skip_whitespace` moved from the reader group into that list, so it
  is named once.
- `dev-docs/ARCHITECTURE.md` — [§dd-arch:arch]: the satellites are
  `core::{token, constructs, specs, node}`, the placement rule of `core::token` is
  recorded verbatim, and the section's decision list names [§dd-dr:core-token-facade].
  [§dd-arch:token]: the public path of the concrete token shapes is `techy::core::token`
  (it still said `techy::core`); the "custom reader over standard tokens" bullet gains
  the several-sources sentence (one inner reader per source, the two promoted methods, and
  the `OBEYS_SPAN_TILING = false` requirement); a new bullet, "The scanning primitives are
  public", lists the seven helpers, their source-free match values, the dispatcher as
  their composition, and the family's `pos` panic; the closing decision list gains both
  new labels. The S1 sketch was left alone — it lists internal topic modules, not public
  paths.
- `dev-docs/DESIGN_RATIONALE.md` — two new entries and three amendments:
  - [§dd-dr:core-token-facade] (in `## Crate organization and dependency model`, right
    before [§dd-dr:stability-rubric]): the placement rule verbatim, why the extraction
    happens now (the token topic is a third of the hub and growing; the `constructs`
    satellite is the same four-part shape), the four straddling families resolved item by
    item (the delta's rules overrides, the state's `PrefixTable`/`TriggerChars`, the
    tokenization declaration a `Lang` names, the specials-hook answer types) with the
    carrying item staying in the hub behind an accepted cross-facade signature reference,
    the accepted path breaks under the soft freeze, the rejected `core::tokenscan`
    helper-only namespace and the rejected uncut straddle, and the Revisit clause.
  - [§dd-dr:public-namespace-topology] gains a *Reversal note (2026-08-19, user)* — the
    only dated line in the file's new text — plus a `techy::core::token` entry in its
    layout list and a hub bullet that no longer claims the token machinery.
  - [§dd-dr:scan-helpers] (at the end of `## Tokens and tokenization`): the two reuse
    cases and which items serve each, source-free match values (why: one helper set for
    all sources), minimal inputs, the absent-feature branch, the family's `pos`
    requirement with the shared up-front check (ahead of every feature gate, so the
    family behaves identically whatever the rules say), the division of labor with the
    composing reader, D3's second precondition with its reason and the reserved
    `Option<Result<…>>` fallback, the two promoted method names, and the three rejected
    alternatives (a public shape-returning dispatcher, `Result` per helper for a bad
    `pos`, a validated cursor newtype).
  - [§dd-dr:superseded-names] gains one bullet with the seven retired private member
    names and what replaced each.
  - [§dd-dr:panic-policy] rule 3(b) gains the family line once — the four value functions,
    then the seven scan helpers (with `scan_command`'s second precondition), then the
    seven `StdToken` constructors — worded as `docs/panics.md` words it; its
    "deliberately infallible" justification now also covers the three helpers that do have
    an error channel, and the `skip_whitespace` consequence bullet became the family's.
- `docs/panics.md`, `techy/src/core/token.rs`, `techy/src/token/scan.rs` — the three
  adopted Stage 1 advisories (see the deviations below for the `skip_whitespace` one).

### Gates (final tree, `f6d129d`, all run in the worktree)

- `cargo test --workspace` — `1170 passed; 0 failed; 0 ignored` for the lib (Stage 1's
  1169 plus the one new `skip_whitespace` test), then `30`, `9`, `14`, `23`, `0`, `0`,
  `0`, `1` for the integration binaries, doc-tests `86 passed; 0 failed; 4 ignored`,
  `techy_derive` doc-tests `0 passed; 0 failed; 2 ignored`. All `test result: ok.`
- `cargo test --lib token::scan` — `45 passed; 0 failed; 0 ignored`.
- `cargo clippy --workspace --all-targets -- -D warnings` — exit 0, zero `warning`/`error`
  lines. Same with `--all-features` — exit 0, zero lines.
- `rm -rf target/doc && cargo docs --all-features` — exit 0; output is `Documenting
  techy-derive`, `Documenting techy`, `Finished`, `Generated` and nothing else. Same for
  plain `cargo docs` (also after `rm -rf target/doc`).
- Link checks on the generated pages: `id="writing-a-token-reader"` exists on
  `core/token/index.html`, and `guide/custom_lang/index.html` carries
  `href="../../core/token/index.html#writing-a-token-reader"`,
  `href="../../core/token/trait.TokenReader.html#seams--readers-that-serve-several-sources-at-one-nesting-level"`,
  and the two `struct.StdTokenReader.html#method.…` links.
  `core/token/fn.skip_whitespace.html` now carries `id="panics"`, like
  `fn.scan_comment.html`.
- `git grep -n 'dd-dr:core-token-facade'` — the DESIGN_RATIONALE heading (line 7063),
  three further DESIGN_RATIONALE citations, and four ARCHITECTURE references (lines 155,
  176, 275, 377), plus the two PLAN.md lines.
- `git grep -n 'dd-dr:scan-helpers'` — the DESIGN_RATIONALE heading (line 1134), four
  further DESIGN_RATIONALE citations, and three ARCHITECTURE references (lines 339, 360,
  375), plus one PLAN.md line.
- The placement rule is byte-identical (whitespace-normalized, with the facade's
  `[hub](crate::core)` link read as "hub") in all three places: `core/token.rs`'s
  "What lives here", ARCHITECTURE [§dd-arch:arch], and the new DESIGN_RATIONALE entry.
- Banned-word grep over every added line of `git diff ct-1-scan..HEAD` — 0 hits for
  each of "door", "funnel", "mint", "trigger token", "vocabulary", "facts",
  "load-bearing", "straggler". (One "next door" in the first draft of the facade entry
  was removed in `abbb8f0`.) The word "contract" appears on two added lines, both stating
  the requirement in the same sentence: the panic-policy rule-3(b) rewrap and
  `skip_whitespace`'s `# Panics` section (the wording the other six helpers use).
- `§dd-` labels in user-facing documentation: `grep -rn '§dd-' docs` — 0 hits. In
  `techy/src`, 111 hits, of which 8 are in doc-comment form; all 8 sit on private items
  and none reaches the rendered documentation —
  `grep -rl '§dd-' target/doc --exclude-dir=src` finds 0 pages (the 31 pages under
  `target/doc/src/` are the source-code view, which shows the source as written). The 8
  are pre-existing and untouched here:
  `techy/src/token/scan.rs:534` (private `checked_scan_error`, added in Stage 1),
  `spec/mod.rs:87`, `serialize/tests.rs:25`, `serialize/wire/tests.rs:384`,
  `constructs/argument_parsers.rs:696`, `constructs/nodes_parser.rs:649` and `:699`,
  `latexlike/serialize_tests.rs:6`.

### Deviations from §1 and §4 (all recorded, none silent)

1. **`skip_whitespace` no longer "moves unchanged"** (§1.4 said it does; adopted Stage 1
   review advisory). Two changes: its docs state the panic under a `# Panics` heading,
   in the form the six other helpers use, and its body calls the shared `check_pos` up
   front, before the feature/enabled early return. Reason: an enforcement gap against its
   own documented promise — the docs said "in all builds", but a call with whitespace
   handling absent or disabled returned `pos` unchecked. The family rule is now uniform
   and is recorded as such in [§dd-dr:scan-helpers] and in [§dd-dr:panic-policy]'s
   consequence bullet. One `#[should_panic]` test covers the whitespace-disabled path
   (`skip_whitespace_panics_on_an_invalid_pos_with_whitespace_disabled`); the
   `content.get(pos..)` fallback panic in the body is gone, since `check_pos` has already
   validated the offset. `scan.rs`'s private module docs needed no change: they said each
   helper states the panic "in its `# Panics` section", which is true only now.
2. **`TODO_Big.md` untouched** (§4's file list allows it "if anything is deferred"): the
   user has uncommitted edits there. The deferred items are in "Notes for the user".
3. **Two ARCHITECTURE lines beyond §4 step 2's three bullets**: the token section's
   "public path: `techy::core`" (now `techy::core::token` — Stage 0 missed it and it was
   wrong as it stood), and the topology section's decision list, which gains
   [§dd-dr:core-token-facade] beside [§dd-dr:public-namespace-topology].
4. **The placement rule ends its own sentence in ARCHITECTURE**, with the label citation
   in a following sentence ("The extraction itself, with its item-by-item resolution:
   [§dd-dr:core-token-facade]."), so that the rule stays verbatim to the last period.

### Review round 1 — required fixes applied (`f6d129d`)

The reviewer passed every gate and required three corrections in
`dev-docs/DESIGN_RATIONALE.md`; four wording items were adopted with them.

1. [§dd-dr:panic-policy]: "the three helpers that do have one" → the **two** that do,
   named — `scan_command` and `scan_specials_trigger` (the other five answer `usize` or
   `Option`).
2. [§dd-dr:scan-helpers]: the reason `token_kind` is out of reach was wrong and
   contradicted the entry it cited. The `TokenReader` implementation of `StdTokenReader`
   is bound on the *token type*
   (`L::Tokenization: Tokenization<L, Token = StdToken<L>, StreamPosition = StdStreamPosition>`,
   `techy/src/token/reader.rs:823-827`), never on `Lang<Tokenization = StdTokenization>`;
   the entry now uses the wording ARCHITECTURE and the facade docs already carry.
3. [§dd-dr:scan-helpers]'s title is PLAN §4's: "The scan helpers: public,
   token-agnostic recognition primitives; the standard reader as their composition"
   (label unchanged).
4. `docs/custom-lang.md`: the *Seams* section is "on the `TokenReader` page" — the
   nearest antecedent of "the same page" was `StdTokenReader`.
5. `docs/custom-lang.md`: a scan helper returns "its byte spans plus the rule or spec
   that matched", which is how `core::token`, ARCHITECTURE and the two entries define a
   match value.
6. ARCHITECTURE [§dd-arch:token] and `techy/src/core/token.rs`: "one reader serving
   several sources at one nesting level requires `OBEYS_SPAN_TILING = false`", in both
   places — a nested `\input` parse reads a second source under `true`, so the old
   "reading from several sources in one parse" was too broad.
7. This log: the commit list and the gate-transcript label name the final commit.

### Open questions

- None new. The user decisions still outstanding are collected in "Notes for the user".

## Notes for the user (collected)

1. **Semver: the path breaks stand and `api-baseline` was not moved.** All 41 moved items
   change their public path (`techy::core::<item>` → `techy::core::token::<item>`), which
   is deliberate under the soft freeze ([§dd-dr:stability-rubric]) and recorded in
   [§dd-dr:core-token-facade]. Measured after Stage 0 with `BASELINE_REV=main`:
   38 path-break entries, all of them `techy::core::<moved item>`; the three type aliases
   (`Token`, `StreamPosition`, `TokenResult`) are not linted by cargo-semver-checks, which
   accounts for the difference from 41. Nothing else broke, and Stages 1 and 2 only add
   items and widen two methods' visibility. Moving `api-baseline` and deciding a version
   number are your deliberate acts after the merge; nothing in this project does either.
2. **D3 — `scan_command`'s second precondition.** Implemented as PLAN §1.8 specifies: the
   function asserts in all builds that `rule.escape_char` stands at `pos`, with the
   message "the rule's escape character 'X' does not stand at pos N". The panic exception
   you granted names only an invalid `pos`, so this is the one point that goes beyond the
   wording of the grant. It is documented in the function's `# Panics` section, in
   `docs/panics.md`'s family bullet, in [§dd-dr:panic-policy] rule 3(b) and in
   [§dd-dr:scan-helpers], and one test covers it. Please confirm — or choose the recorded
   fallback, `-> Option<Result<CommandMatch, EndOfStreamAfterEscape>>` with `None` for the
   mismatch, which is a small change to the function and to the one arm of the dispatcher
   that calls it.
3. **`skip_whitespace` was changed, though §1.4 said it moves unchanged** (Stage 2
   deviation 1 above): a `# Panics` heading and the shared `check_pos` called before the
   feature gate, so all seven helpers enforce the documented `pos` requirement
   identically. Behavior changes only for calls that were already violating the
   documented requirement, and only from "returns `pos`" to "panics", which is what the
   docs promised all along. Reversible in one commit if you would rather keep the old
   body.
4. **`README.md`'s facade list is still incomplete** — it names `techy::source`,
   `techy::error`, `techy::extract`, `techy::core` (with its four satellites) and
   `techy::latexlike`, and omits `techy::transform`, `techy::visit`, `techy::recompose`
   and `techy::serialize`. Pre-existing, unrelated to this project, and deliberately not
   touched here (Stage 0 only added the `core::token` satellite to that list).
5. **Deferred by PLAN §7, none of it done** (and none of it in `TODO_Big.md`, which was
   left alone): (a) a public "nearest valid offset" utility for anchoring a report at an
   invalid position — today a private method of `StdTokenReader`, and a custom reader
   validating its own caller's offsets may want it; (b) `StdTokenReader::source()`
   visibility — an open item that predates this project; (c) an end-of-content or
   `is_forbidden_char` helper — deliberately not provided (one-liners the dispatcher keeps
   inline); (d) the `api-baseline` move / version bump of note 1.
6. **Two smaller Stage 1 observations were adopted here and need no decision**:
   `docs/panics.md`'s justification clause now reads as plain English, and
   `core::token`'s module docs say "one of the item groups that rule places here".
   `CLAUDE.md`'s `core::token` bullet now names the whole helper family, as Stage 1
   suggested.
