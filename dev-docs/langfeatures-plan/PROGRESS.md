# lang-features: Progress

- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/agent-a01480fa109b19cef`
- **Branch**: `lang-features`
- **Plan**: see `dev-docs/langfeatures-plan/PLAN.md` (Design Spec is user-ruled; do not relitigate)

## Status

| Stage | Description | Status |
|-------|-------------|--------|
| D | Decision record [§dd-dr:lang-features] + ARCHITECTURE refs + superseded-names + CompileTimeFeatureGates.md status line | done (7a113e8 + fixes d28a3e9) |
| M1 | TokenRules/Overrides regrouped into per-feature blocks (pure reshaping, behavior identical) | in-progress |
| M2 | `Lang::Features` + const gating | pending |
| M3 | Uniform storage gating (FeaturePresence::Store) | pending |
| M4 | Docs, coherence sweep, closure (delete this directory) | pending |

## Log

(compact completion notes appended by each subagent: what was done, files touched, anything surprising)

- **Supervisor: baseline + sizing** — pre-M1 baseline on the `todo` base commit
  432c1a9 is green: `cargo build` ok, `cargo test --workspace` 884 passed / 0
  failed / 4 ignored. NOTE for all agents: workspace layout — every `src/...`
  path in PLAN.md means `techy/src/...` (crates: `techy/`, `techy-derive/`).
  Integration tests: techy/tests/{acceptance,recompose_oracle,derive_conditions}.rs.
  Field-access migration sites (grep of the old TokenRules/Overrides field names):
  ~290 hits; heaviest files: state/delta.rs (~72), state/parsing_state.rs (~54),
  token/rules.rs (~36), engine/state_memo.rs (~24), latexlike/driver.rs (~29),
  engine/mod.rs (~11), constructs/nodes_parser.rs (~18), constructs/verbatim_parser.rs
  (~10), token/reader.rs (~14); the rest scattered. One-time unsandboxed
  `cargo fetch` was needed to populate ~/.cargo registry (clap for a dev-dep);
  builds/tests are sandboxed since.

- **Stage D implementer** — decision record written. New entry
  `[§dd-dr:lang-features]` added at the end of the "Parsing state and deltas"
  topic (`dev-docs/DESIGN_RATIONALE.md`, before "## Specs and scopes"): the
  compile-time axis + three motivations (memory measured-and-dropped, citing
  dev-docs/extra/CompileTimeFeatureGates.md), three spellings of off,
  exhaustive eight-feature roster with the ForbiddenChars two-axis supplement
  and Paragraphs-as-own-feature (reader runtime check promoted to
  `LangHasParagraphs: LangHasWhitespace`), independent gates + enforced edges
  (REJECTED closed tiers), total reads / bounded writes / crate-owned stores
  (REJECTED open implementation substitution — memo hash/equality stays
  crate-owned), gated overrides (REJECTED silent no-ops), sub-struct-granular
  struct-update pitfall, transparent present store, the naming set,
  LatexlikeLang pins AllLangFeatures. Cross-links added to
  [§dd-dr:token-rules-data]/[§dd-dr:data-vs-traits] to pre-empt a perceived
  contradiction (values stay data; only presence is compile-time).
  ARCHITECTURE.md: entry referenced from the [§dd-arch:state] decisions list
  and as new naming principle 8 in [§dd-arch:naming]. Superseded-names: one
  new bullet (`Gate`/`On`/`Off`; bare `Present`/`Absent`/`Has*`/`Features`;
  plus the public-vocabulary word ban ruled by the user — spelled out only in
  the register row itself). CompileTimeFeatureGates.md got the
  adopted-with-modifications status line pointing at the entry. Docs only, no
  src/ changes. Nothing surprising; one judgment call: the [§dd-arch:state]
  reference lives in the decisions list (not body prose) because ARCHITECTURE
  describes present-day structure and the M1–M3 code does not exist yet —
  M2/M4 should promote it into body text when it does.

- **Stage D reviewer** — reviewed commit 7a113e8 against PLAN.md Stage D, the
  DR entry template, Documentation_Structure.md, and the source tree. Findings:
  0 blocker, 1 should-fix, 2 nit.
  - should-fix (DESIGN_RATIONALE.md, [§dd-dr:enable-flags] ~line 855): the old
    entry glosses the constitutive/empty off as "(the language has no such
    feature)" — now the definition of *absent*; conflicts with the new entry's
    "the three words are never interchanged". Needs an amendment note.
  - nit (DESIGN_RATIONALE.md, superseded-names row): bare `Features` ban reads
    as covering the spec-mandated `Lang::Features` associated type; clarify
    scope (standalone item names in the hub).
  - nit (DESIGN_RATIONALE.md, [§dd-dr:lang-features] naming paragraph): the
    "gate would fuse the two axes" rejection sits next to the entry's own
    compile-time "code/storage gating" prose; scope the rejection to item
    names (`Gate`/`On`/`Off`).
  All content points of PLAN Stage D item 1 present; names match the spec
  exactly; no invented API; label spelled consistently everywhere; "facet"
  only in the ban row; no pre-existing banned name reintroduced; both
  ARCHITECTURE references present in matching style; CompileTimeFeatureGates.md
  status line accurate. Facts verified against source: reader.rs:303-305 dual
  flag check, state_memo.rs:144/199 Arc-identity field walk, driver.rs:205-207
  exhaustive literal, lang.rs:455 TrivialLang blanket, verbatim terminator
  Arc<GroupRule<L>>, temporary-group-minting parsers, rules.rs "enable_*
  feature gates" rustdoc, exploration doc Gate/On/Off + 4-8 % measurements +
  six-feature original roster.

- **Stage D implementer** — review fixes applied (3 findings): [§dd-dr:enable-flags]
  constitutive gloss corrected to "no rules data" + amendment note pointing at
  [§dd-dr:lang-features]; superseded-names ban scoped to standalone item names
  (`Lang::Features` excluded); Gate/On/Off rejection scoped to item names, prose
  "gating" unaffected.

- **M1 implementer A** — rules.rs + delta.rs regrouped per PLAN M1. THIS COMMIT IS
  INTERMEDIATE-RED: core migration pending (implementers B and C); all `cargo check`
  primary errors point into their files (state_memo, parsing_state, reader,
  prefix_table, list_reader, constructs/*, scopes, engine, latexlike); zero primary
  errors/warnings in rules.rs or delta.rs. Files touched: techy/src/token/rules.rs,
  techy/src/state/delta.rs, plus minimal facade re-export lines only in
  techy/src/token/mod.rs, techy/src/state/mod.rs, techy/src/core/mod.rs (new types
  mirror the existing WhitespaceRules/TokenRulesOverrides paths; one canonical public
  path each, via techy::core).
  - Rules sub-structs (rules.rs): `WhitespaceRules { enabled, chars }` (kept its
    existing `Default` derive — reviewer to rule keep-or-drop), `ParagraphRules
    { enabled }`, `GroupRules<L> { enabled, rules, temporary, expecting_close }`
    (manual Clone/Debug/PartialEq/Eq like TokenRules, to avoid `L:` bounds),
    `CommandRules { enabled, rules }`, `CommentRules { enabled, rules }`,
    `SpecialsRules { enabled }`, `ForbiddenCharsRules { chars }` (no `enabled`,
    per [§dd-dr:enable-flags]). Each has `empty()`; `TokenRules::empty()` now composes
    them (same value as before). TokenRules fields: whitespace, paragraphs, groups,
    commands, comments, specials, forbidden_chars — all pub.
  - Accessors on TokenRules, names exactly as spec'd (checked against
    [§dd-arch:naming]; no deviations): whitespace_enabled() -> bool,
    whitespace_chars() -> &str, paragraphs_enabled() -> bool, groups_enabled() -> bool,
    group_rules() -> &[Arc<GroupRule<L>>], temporary_group_rules() ->
    &[Arc<GroupRule<L>>], expecting_group_close() -> Option<&Arc<GroupRule<L>>>,
    commands_enabled() -> bool, command_rules() -> &[Arc<CommandRule>],
    comments_enabled() -> bool, comment_rules() -> &[Arc<CommentRule>],
    specials_enabled() -> bool, forbidden_chars() -> &str. Return-type choices: `&str`
    (not `&Arc<str>`) for whitespace_chars/forbidden_chars — generic read sites use
    `.contains(c)`/deref-to-str only; Arc-cloning sites are construction sites
    (latexlike), which use field paths per the plan's accessor rule, and `&str` has a
    trivial neutral answer for M3. Note: `forbidden_chars` is both a field (the block)
    and a method (the chars) — spec-mandated names; Rust namespaces keep them apart.
  - Overrides (delta.rs — TokenRulesOverrides lives there, so the seven sub-override
    structs do too): WhitespaceOverrides { enabled, chars }, ParagraphOverrides
    { enabled }, GroupOverrides<L> { enabled, rules, temporary, expecting_close }
    (manual impls incl. Default, no `L:` bounds), CommandOverrides/CommentOverrides
    { enabled, rules }, SpecialsOverrides { enabled }, ForbiddenCharsOverrides
    { chars }. All-None Default on each; `disable()` on the six gated blocks
    (ForbiddenCharsOverrides excepted). `disable_all()` = the six `disable()`s +
    ForbiddenCharsOverrides::default() — exact old semantics (same six gates, nothing
    else). merge_from/apply stay on TokenRulesOverrides (same visibility as before) and
    now delegate to pub(crate) per-block merge_from/apply — leaf-level semantics
    identical, and the per-block methods give M2 a natural per-feature gating seam.
  - Doc redistribution: every old per-field rustdoc moved verbatim-modulo-links onto
    the sub-struct fields (expecting_close keeps its "positional data, not gated"
    contract; temporary keeps the full scoped-lifecycle narrative). TokenRules keeps
    the detection-priority contract and the two-spellings-of-off narrative (renamed
    section "Per-feature `enabled` gates"); third spelling referenced only in a non-doc
    `//` comment pointing at [§dd-dr:lang-features] (public rustdoc never cites dd-dr
    labels — matches existing crate practice). Struct-update pitfall documented on
    TokenRulesOverrides (own section) with the `..GroupOverrides::disable()` recipe,
    and cross-referenced from disable_all()'s doc. Two glosses updated to the amended
    [§dd-dr:enable-flags] vocabulary: constitutive off now glossed "(no rules data)",
    not "(the language has no such feature)" — that wording now defines *absent*.
    CommandRule/CommentRule docs: "an empty [rules list] disables recognition" reworded
    to "means no … recognition" (word discipline: "disabled" = flag off only).
  - Tests: delta.rs's disable_all test updated to the new field paths — assertion
    values unchanged (Some(false) x6, default+flips equality). rules.rs had no unit
    tests before and has none now.
  - Surprises: none beyond the module-doc sentence "none of these types implement
    `Default`" already being contradicted by WhitespaceRules's derive pre-change; left
    as-is for the reviewer's Default ruling.

## Questions for user

(genuine design ambiguities; the most conservative spec-consistent option was chosen and is noted here)

## Hand-off notes

(state a fresh supervisor needs beyond PLAN.md + this file + git log)
