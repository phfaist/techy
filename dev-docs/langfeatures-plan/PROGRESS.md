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

## Questions for user

(genuine design ambiguities; the most conservative spec-consistent option was chosen and is noted here)

## Hand-off notes

(state a fresh supervisor needs beyond PLAN.md + this file + git log)
