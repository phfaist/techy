# lang-features: Design Spec and Milestone Plan

Working scaffolding for the "lang-features" project (compile-time language features on
`Lang`). This directory mirrors the removed dev-docs/api-review precedent and is deleted
at M4 closure (retained in git history).

All Design Spec points below are USER-RULED decisions — implement, don't relitigate.

## Design Spec

Compile-time language features declared on `Lang`. Roster (exhaustive over the parsing
state — every TokenRules block is a feature, plus Scopes): Whitespace, Paragraphs,
Groups, Commands, Comments, Specials, ForbiddenChars, Scopes.

Vocabulary — three spellings of "off", each with its own word:

- **absent** (compile-time: the language has no such feature),
- **disabled** (scoped runtime: `enabled` false, data preserved),
- **empty** (constitutive: no rules data).

ForbiddenChars deliberately has no `enabled` flag (existing recorded ruling: one
trivially restorable string needs no runtime gate — that ruling concerns the runtime
axis only and coexists with compile-time absence).

### New public items (all in techy::core, flat; M2 milestone)

- `trait LangFeatures: 'static` with members
  `type Whitespace/Paragraphs/Groups/Commands/Comments/Specials/ForbiddenChars/Scopes: FeaturePresence`.
- `trait FeaturePresence` (sealed — closed two-valued vocabulary; use the
  private-supertrait sealing pattern): `const PRESENT: bool` + GAT `type Store<T>`
  (bounds as needed for Clone/Debug/Send/Sync/Default composition).
- Markers `FeaturePresent` (Store<T> = T, transparent — this is load-bearing: concrete
  languages with a feature present write plain struct literals) and `FeatureAbsent`
  (Store<T> = a ZST; PhantomData<T> is acceptable if it composes, else a dedicated ZST).
- Bundles `AllLangFeatures` (all present), `NoLangFeatures` (all absent).
- Eight Lang subtraits with blanket impls: `LangHasWhitespace`, `LangHasParagraphs`,
  `LangHasGroups`, `LangHasCommands`, `LangHasComments`, `LangHasSpecials`,
  `LangHasForbiddenChars`, `LangHasScopes`. Preferred shape:
  `trait LangHasGroups: Lang where Self::Features: LangFeatures<Groups = FeaturePresent> {}`
  — verify by compile prototype that the equality propagates so bounded generic code
  writes plain struct literals; fallback spelling if it does not: direct bounds
  `where L::Features: LangFeatures<Groups = FeaturePresent>` at use sites (record which
  was used).
- `Lang` gains `type Features: LangFeatures` (breaking one-liner for each hand-written
  Lang impl; the TrivialLang blanket impl at src/state/lang.rs:455 sets
  `AllLangFeatures` for all test languages; Latexlike and the LatexlikeLang family PIN
  `Features = AllLangFeatures` — user ruling: the latexlike family uses all features).

### Lattice edges (compiler-enforced)

- `LangHasParagraphs: LangHasWhitespace` (supertrait edge — mirrors the existing
  runtime check at src/token/reader.rs:304);
- verbatim family (`verbatim_state_delta`, `VerbatimArgumentParser`,
  `VerbatimBodyParser`) and the temporary-group-minting argument parsers require
  LangHasGroups;
- `ScopeStack::push`, `ParsingStateDelta::{push_provider, scope_op}`, `ScopeOp`
  construction require LangHasScopes.
- Callables do NOT imply scopes.

### M1 regrouping (no compile gating yet) — sub-structs in src/token/rules.rs

- `WhitespaceRules` (existing type, gains `enabled: bool`; keeps `chars: Arc<str>`)
- `ParagraphRules { enabled: bool }` (absorbs today's
  `enable_multi_newline_paragraphs`)
- `GroupRules<L> { enabled: bool, rules: Vec<Arc<GroupRule<L>>>, temporary: Vec<Arc<GroupRule<L>>>, expecting_close: Option<Arc<GroupRule<L>>> }`
  (absorbs enable_groups, groups, temporary_groups, expecting_group_close —
  expecting_group_close is INSIDE the groups block per user ruling; preserve its
  "positional data, not gated by enabled" doc semantics)
- `CommandRules { enabled: bool, rules: Vec<Arc<CommandRule>> }`
- `CommentRules { enabled: bool, rules: Vec<Arc<CommentRule>> }`
- `SpecialsRules { enabled: bool }`
- `ForbiddenCharsRules { chars: Arc<str> }` (NO enabled field)

`TokenRules<L>` fields become exactly: whitespace, paragraphs, groups, commands,
comments, specials, forbidden_chars — one per feature. `GroupRule`/`CommandRule`/
`CommentRule` unchanged. `TokenRules::empty()` unchanged in name/meaning; each
sub-struct gets an `empty()` named constructor mirroring it (no Default on rules
sub-structs except where one already exists — WhitespaceRules currently derives
Default; reviewer decides keep-or-drop for consistency and logs it).

- Redistribute the existing field rustdoc (it is among the crate's best — preserve
  content) onto the sub-structs; TokenRules keeps the detection-priority contract and
  the spellings-of-off narrative (two spellings now; third added at M2).
- Total-read accessors on `TokenRules<L>` become THE generic read path (load-bearing
  for M3): whitespace_enabled(), whitespace_chars(), paragraphs_enabled(),
  groups_enabled(), group_rules(), temporary_group_rules(), expecting_group_close(),
  commands_enabled(), command_rules(), comments_enabled(), comment_rules(),
  specials_enabled(), forbidden_chars(). Check final accessor names against
  [§dd-arch:naming]. Rule, grep-enforced at review: generic core code (src/ outside
  latexlike, outside rules.rs/delta.rs themselves) reads token-rules data ONLY via
  these accessors; the latexlike preset (pinned AllLangFeatures) and construction/test
  sites may use field paths and literals.
- `TokenRulesOverrides<L>` mirrors:
  `WhitespaceOverrides { enabled: Option<bool>, chars: Option<Arc<str>> }`,
  `ParagraphOverrides { enabled: Option<bool> }`,
  `GroupOverrides<L> { enabled: Option<bool>, rules: Option<Vec<..>>, temporary: Option<Vec<..>>, expecting_close: Option<Option<Arc<GroupRule<L>>>> }`,
  `CommandOverrides`, `CommentOverrides` (analogous),
  `SpecialsOverrides { enabled: Option<bool> }`,
  `ForbiddenCharsOverrides { chars: Option<Arc<str>> }`.
  All-None Default on each; per-feature `disable()` constructors
  ({ enabled: Some(false), ..Default::default() }) on the seven that have `enabled`
  (Paragraphs included; ForbiddenChars excepted); `TokenRulesOverrides::default()` and
  `disable_all()` keep exact current semantics (disable_all flips whitespace,
  paragraphs, groups, commands, comments, specials — same six runtime gates as today;
  forbidden_chars untouched). Update merge_from and apply. Document the
  sub-struct-granularity struct-update pitfall on TokenRulesOverrides (a field literal
  replaces the WHOLE feature block that a base like disable_all() set up — e.g. the
  verbatim recipe must use `..GroupOverrides::disable()` inside the groups literal).
- Preserve the exit-math exhaustive-literal idiom (src/latexlike/driver.rs:205-207
  comment "Exhaustive literal on purpose"): the rewritten site must remain exhaustive
  at BOTH levels (all TokenRulesOverrides fields, and all fields inside each
  sub-override literal) so a new field anywhere still breaks that build until the
  restore-or-exclude decision is made.
- Known migration sites (~330 field accesses):
  src/token/{rules,reader,prefix_table,list_reader,token}.rs,
  src/state/{delta,parsing_state,stack}.rs,
  src/engine/{state_memo,language,driver}.rs, src/constructs/*.rs, src/scopes/mod.rs,
  src/latexlike/*.rs, plus unit tests in those files and integration tests.
  skip_whitespace keeps its `&TokenRules<L>` signature. state_memo hash_key/keys_eq
  (src/engine/state_memo.rs:144,199) walk overrides field-by-field — update to the
  sub-struct shape, preserving exact hash/equality semantics (Arc-identity keying;
  document that gated-absent fields will hash as nothing at M3, not now).
- Behavior must be bit-for-bit identical: this milestone is a pure reshaping. No test
  may change its assertion values; tests change only in construction/access syntax.

## Milestone Plan

### Stage D — decision record (BEFORE M1 code)

1. New DESIGN_RATIONALE.md entry, label `[§dd-dr:lang-features]` (labels are immutable
   — follow the file's entry template and maintenance rules; read
   Documentation_Structure.md and 2-3 existing entries first to match form). Content:
   the compile-time feature axis and its motivations (field organization;
   unrepresentability; soft-freeze window — the memory argument was measured and
   dropped, see dev-docs/extra/CompileTimeFeatureGates.md); three spellings of off; the
   exhaustive-roster rule (every TokenRules block + Scopes) with the ForbiddenChars
   two-axis clarification (no runtime gate ≠ compile presence; supplement, not
   reversal, of the recorded no-enable ruling); Paragraphs as its own feature (own
   token kind, detection fn, dispatch arm, driver hook; runtime edge at reader.rs:304
   promoted to supertrait edge); independent gates + compiler-enforced lattice edges
   (REJECTED: closed tiers); the embryo shape — total reads, bounded writes,
   crate-owned stores (REJECTED: open per-feature implementation substitution, because
   the derivation memo's hash/equality soundness contract must stay crate-owned); gated
   overrides (REJECTED: silent no-ops for absent features); sub-struct-granular
   struct-update as a documented pitfall; transparent Present store as a design
   requirement; the naming set with rationale (absent/disabled/empty word split;
   "Lang*"/"Feature*" prefixes because bare Present/Absent/Has* are too generic for the
   flat core hub); LatexlikeLang pins AllLangFeatures.
2. ARCHITECTURE.md: reference the new entry from the appropriate section(s) (the
   state/Lang topic; a naming-section note for the new vocabulary) per the manual grep
   discipline described in the pillar docs.
3. superseded-names additions to [§dd-dr:superseded-names]: `Gate`/`On`/`Off` (collide
   with the runtime "feature gate" vocabulary), bare `Present`/`Absent`/`Has*`/
   `Features` spellings (too generic for the flat hub), "facet" as public vocabulary.
4. Add a status line at the top of dev-docs/extra/CompileTimeFeatureGates.md:
   adopted-with-modifications, pointing at [§dd-dr:lang-features].

Reviewer checks the entry against the template, the clarity rules, and this spec.
Commit: `lang-features: D — decision record [§dd-dr:lang-features]`.

### Stage M1 — TokenRules/Overrides regrouping (suggested decomposition)

1. Implementer A: rules.rs + delta.rs — sub-structs, overrides, accessors, doc
   redistribution, empty()/disable()/disable_all()/merge_from/apply. Compile of the two
   files' own tests may be deferred to task 2.
2. Implementer B: core src migration (token/, state/, engine/, constructs/, scopes/) to
   the new shapes; generic reads via accessors only.
3. Implementer C: latexlike preset migration (exhaustive-literal idiom preserved) + all
   tests (unit + integration).
4. Reviewer pass over the full M1 diff (spec conformance; grep for old field paths;
   accessor-only rule in generic core code; docs-clarity; behavior identity —
   assertion values unchanged).
5. Fixes, then gates: build, test, docs (link check), check_semver.sh with
   expected-breaking list recorded in PROGRESS.md. Final commit:
   `lang-features: M1 — TokenRules/Overrides regrouped into per-feature blocks`.

### Stage M2 — `Lang::Features` + const gating (successor supervisor)

- Normalization prototype first. Const guards at: reader
  command/comment/whitespace/paragraph/specials/forbidden branches +
  PrefixTable::for_rules; nodes_parser dispatch arms (Command, GroupOpen, specials,
  paragraph); parsing_state derived() temporary-group stripping + scope-op apply; delta
  apply_overrides; state_memo hash_key/keys_eq; engine/language.rs stray-close arm.
- Semantics: absent wins over runtime data; a violated contract returns Err through the
  recovery funnel, never panics ([§dd-dr:panic-policy] rule 3).
- Four representative test languages: all-on (existing suite), NoLangFeatures,
  groups-only, callables-without-scopes.
- New rustdoc: the third spelling of off.

### Stage M3 — uniform storage gating (successor supervisor)

- FeaturePresence::Store across all seven rules sub-structs and their overrides, plus
  ScopeStack's inner Vec (public signatures unchanged, including
  ParsingState::scopes()); derived caches collapse with their features (prefix table
  with Groups, trigger chars with Specials); lattice-edge bounds land on the verbatim
  family, temporary-group-minting argument parsers, scope-mutating entry points; static
  size_of regression tests (TokenRules of a NoLangFeatures lang collapses to
  (near-)zero; all-on sizes unchanged).

### Stage M4 — docs + closure (successor supervisor)

- Language-author guide section on declaring features (docs-clarity rules); rustdoc
  coherence sweep; final gate run with rendered-HTML link verification;
  superseded-names grep; delete dev-docs/langfeatures-plan/ (retained in git history,
  per the api-review-scaffolding precedent).

## Repo rules that bind every stage

- Result<T,E> everywhere per [§dd-dr:panic-policy] rule 3 (no new panics without user
  approval); check dev-docs/ARCHITECTURE.md [§dd-arch:naming] before any naming; never
  reintroduce names from DESIGN_RATIONALE.md [§dd-dr:superseded-names]; docs-clarity:
  no metaphors/jargon/undefined terms in any user-facing docs (rustdoc, guides); the
  word "facet" is banned from all public names and docs (internal shorthand only).
- Gates for the final commit of each milestone: `cargo build`, `cargo test`,
  `cargo docs` (run `rm -rf target/doc` first and verify no broken intra-doc links in
  output), `scripts/check_semver.sh` (compares against the `api-baseline` branch —
  document the expected breaking list in PROGRESS.md; do NOT update api-baseline).
- Prefer green (building, passing) commits; intermediate red commits inside a milestone
  are acceptable only if noted in PROGRESS.md.
- No pushes. No merges to main. All work stays on the `lang-features` branch.
