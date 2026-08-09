# lang-features: Progress

- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/agent-a01480fa109b19cef`
- **Branch**: `lang-features`
- **Plan**: see `dev-docs/langfeatures-plan/PLAN.md` (Design Spec is user-ruled; do not relitigate)

## Status

| Stage | Description | Status |
|-------|-------------|--------|
| D | Decision record [§dd-dr:lang-features] + ARCHITECTURE refs + superseded-names + CompileTimeFeatureGates.md status line | done (7a113e8 + fixes d28a3e9) |
| M1 | TokenRules/Overrides regrouped into per-feature blocks (pure reshaping, behavior identical) | done (d7f480f + 9bb4ac9 + 286edf2 + final fix/gate commit) |
| M2 | `Lang::Features` + const gating | in-progress |
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

- **M1 implementer B** — core (non-latexlike) src migrated to the per-feature blocks.
  THIS COMMIT IS INTERMEDIATE-RED: latexlike + integration tests pending (implementer
  C); `cargo check` and `cargo check --tests` errors point ONLY into
  techy/src/latexlike/** (98 at --tests, 37 lib-only); zero errors and zero warnings in
  the migrated scope. rules.rs/delta.rs untouched (no minimal changes needed). Files
  touched (lib + in-file unit tests): token/{reader,prefix_table,list_reader}.rs,
  state/{parsing_state,lang}.rs, engine/{state_memo,language,mod}.rs, constructs/
  {nodes_parser,argument_parsers,chars_group_parser,verbatim_parser,group_parser,
  environment_parser,attached_source,child_state}.rs, scopes/mod.rs,
  node/{mod,display,invariants}.rs. token/token.rs, state/stack.rs, engine/driver.rs
  needed no code change (driver.rs's one doc link resolves to the accessor).
  - Accessor-only rule: all generic-core TokenRules reads go through the accessors
    (grep-verified: remaining `.<block>.<field>` paths in scope are #[cfg(test)]
    construction/mutation, override-struct field paths — overrides have no accessors —
    or the one item-d site below). reader.rs:304's dual check is now
    `!(rules.paragraphs_enabled() && rules.whitespace_enabled())`; skip_whitespace kept
    its `&TokenRules<L>` signature (private helper `paragraph_continues` now takes
    `ws_chars: &str`).
  - Item-d mutate/ownership sites kept (M2/M3 gating seams), exactly one:
    state/parsing_state.rs `derived()` — `data.rules.groups.temporary.clear()` (the
    temporary-group scope stripping; in-code comment added). Everything else that
    writes rules blocks is delta application (delta.rs, not this scope) or test code.
  - state_memo.rs hash_key/keys_eq: rewritten field-by-field in the ORIGINAL
    (pre-regrouping) field order — same Arc-identity keying (arc_addr/str_addr/
    Arc::ptr_eq), same coverage (old enable_whitespace ↦ whitespace.enabled, old
    whitespace-chars payload ↦ whitespace.chars, …, expecting_close still hashed last),
    no field dropped or added; comment added documenting order preservation and that
    gated-absent blocks hash as nothing only at M3.
  - Override construction updated to sub-override shapes; sites that were bare
    `enable_X: Some(false)` now use the per-block `X::disable()` (identical values);
    verbatim_state_delta uses the PLAN's `..GroupOverrides::disable()`-inside-groups
    recipe verbatim (struct-update pitfall comment at the site).
  - Behavior identity: pure reshaping; no assertion value changed (diff-audited: the
    54 touched assert lines carry literal-for-literal identical expected values).
  - Docs touched only where old field names appeared: stale intra-doc links/labels
    retargeted (temporary_groups → temporary_group_rules / GroupRules::temporary;
    TokenRules::commands → command_rules; enable_* prose → "the X gate" wording) in
    reader/prefix_table/parsing_state/lang/scopes/child_state/group_parser/
    argument_parsers/verbatim_parser/chars_group_parser/environment_parser. One
    pre-existing "facet" in reader.rs module doc reworded to "feature block" (banned
    word, was adjacent to a stale link being fixed anyway).
  - Surprises: none structural. Test fn names still spell `enable_commands_off…` etc.
    (names, not field paths; left for the reviewer to rule on). pylatexenc kwarg
    mentions (`enable_comments`, `enable_groups=False`, …) kept where they name
    pylatexenc's API, reworded where they described ours.

- **M1 implementer C** — latexlike preset + integration tests migrated; workspace
  GREEN (closes the M1 code phase). `cargo build` clean, `cargo test --workspace`
  totals exactly the baseline: 884 passed / 0 failed / 4 ignored (per-target
  758+30+8+21+1+66 passed, 2+2 ignored); zero warnings (forced full rebuild of
  lib+tests); `cargo docs` clean (fresh target/doc, no link warnings). Files
  touched: src/latexlike/{driver,mod,environments,input,invocation_syntax,
  arguments,node_ref}.rs, tests/{acceptance,recompose_oracle}.rs (test_support,
  lang, spec, minidefs, recompose, invariants needed nothing; tests/
  derive_conditions.rs untouched).
  - Exhaustive-literal site (driver.rs exit_math_context_delta): rewritten
    exhaustive at BOTH levels — the TokenRulesOverrides literal spells all seven
    block fields, each sub-override literal spells all of its fields, no
    `..Default::default()`/`..disable()` anywhere in it; the comment now states
    the both-levels intent. Field-by-field audit against the old literal: the
    same eleven restores land (gates + data + forbidden chars), the same two
    transients stay `None` (groups.temporary, groups.expecting_close — old
    temporary_groups/expecting_group_close), transient-gate comments kept with
    the 2026-08-04 user-ruling citation.
  - Construction sites use struct literals/field paths per the preset's pinned
    convention (default_token_rules is a full two-level literal); non-construction
    reads use accessors where clearest (forbidden_chars(), commands_enabled(),
    group_rules()); test override literals spread from the sub-override bases
    (`..GroupOverrides::default()` etc.); bare `enable_comments: Some(false)`
    sites became `CommentOverrides::disable()` (identical values), the one
    `Some(true)` site an explicit literal over `..CommentOverrides::default()`.
  - Behavior identity: no assertion value changed (diff-audited — all changed
    assert lines are access-syntax only). The one judgment call:
    driver.rs's `restored.rules().groups == text_context.rules().groups` (old
    Vec-vs-Vec compare) migrated to `.groups.rules` on both sides, NOT a
    whole-block compare, to keep the assertion's scope bit-for-bit.
  - Docs touched only where old names appeared: paragraph-gate links →
    `ParagraphRules::enabled`; `TokenRules::forbidden_chars` link retargeted to
    `ForbiddenCharsRules::chars` (the name is now field+method on TokenRules —
    ambiguous as a rustdoc link); temporary_groups links/prose →
    temporary_group_rules / GroupRules::temporary; the math-interior doc's
    override-field mention now names the groups-block
    `GroupOverrides::expecting_close`.
  - No changes outside scope: rules.rs/delta.rs and all implementer-B files
    untouched. Surprises: none; no compile blocker required touching
    earlier-migrated files.

- **M1 reviewer** — full-diff review of 88079cd..286edf2 against PLAN M1. Findings:
  0 blocker, 1 should-fix (resolved by the item-8 ruling below), 2 nit. Gates re-run
  independently: `cargo test --workspace` 884/0/4 (exactly baseline), fresh
  `cargo docs` zero warnings.
  - should-fix (techy/src/token/rules.rs:9): module doc claims "none of these types
    implement `Default`" while WhitespaceRules still derives it — pre-existing
    contradiction, spec-delegated to the reviewer; resolved by the ruling (drop the
    derive; the sentence then becomes true with no edit).
  - nit (reader.rs:849,1194,1447,1605,1618; parsing_state.rs:665,829): test fn names
    still spell old vocabulary (`enable_commands_off_…`, `temporary_groups_are_…`,
    `enable_groups_flag_…`). Names, not field paths — but rename in the fix pass
    (e.g. `commands_gate_off_…`, `temporary_group_rules_are_…`) so M4's
    old-name grep comes back clean.
  - nit (token/error.rs:43,78): plain-text `` `TokenRules::forbidden_chars` `` — not
    stale (the spelling now names the accessor), optionally sharpen to
    `ForbiddenCharsRules::chars`.
  - **RULING (item 8, WhitespaceRules `Default`): DROP the derive.** Rationale:
    (a) consistency — the six sibling rules sub-structs deliberately have none, and
    `default()` == `empty()` is a second spelling of the same value, contra the
    one-canonical-path discipline; (b) crate doctrine — `TokenRules::empty` rustdoc
    rejects `Default` by name (silent zeroing of future fields via `..Default`), and
    the superseded-names register already records removing `Default` back doors
    (`Default for Language<L>`); (c) the hazard is now real: the struct has two
    fields and gains a gated store at M3; (d) usage is exactly three call sites, all
    `#[cfg(test)]` (token/prefix_table.rs:182, node/mod.rs:109, scopes/mod.rs:1705)
    — replace with `WhitespaceRules::empty()`, identical value; (e) semver: the type
    already breaks in M1 (new pub field), so keeping the derive buys nothing — add
    "`Default` removed from `WhitespaceRules`" to the expected-breaking list.
  - Everything else verified clean: sub-struct/override shapes, empty()/disable()/
    disable_all() semantics, the 13 accessors (spec names exactly),
    skip_whitespace signature, expecting_close placement + positional-data doc;
    old-field sweep — no code survivors (residual text = accessor names,
    pylatexenc API mentions, the fn-name nit); accessor-only rule holds in generic
    core (exceptions are #[cfg(test)], construction sites, override field paths, and
    the one commented parsing_state.rs:219 seam); state_memo hash_key/keys_eq
    field-by-field identical to pre-regrouping (same coverage, original order,
    expecting_close last, same Arc-identity keying, M3 comment present); exit-math
    literal exhaustive at both levels with the same eleven restores and two
    transient `None`s as before; no test assertion literal changed anywhere in the
    diff; doc redistribution content-preserved (checked against 88079cd), pitfall
    section + recipe in place, no dd-dr labels in public rustdoc, no banned names,
    no "facet"; facade exports exactly one canonical path per new type via
    techy::core, no extra surface; no undocumented behavior change (only equivalent
    rewrites: De Morgan in detect_paragraph_break, private `paragraph_continues`
    takes `&str`, override constructions value-identical).

- **Supervisor: M1 fix pass + gates** — applied the reviewer's ruling and both nits
  directly (small mechanical changes): `Default` derive dropped from
  `WhitespaceRules` (rules.rs), its three `#[cfg(test)]` call sites switched to
  `WhitespaceRules::empty()` (prefix_table.rs, node/mod.rs, scopes/mod.rs); seven
  test fns renamed off the old vocabulary (reader.rs: commands/comments/specials/
  groups `*_gate_off_*`, parsing_state.rs: `temporary_group_rules_are_prefix_table_inputs`,
  `groups_gate_rebakes_the_prefix_table`); token/error.rs docs sharpened to
  `ForbiddenCharsRules::chars`. Gates, all green: `cargo build` ok; `cargo test
  --workspace` 884 passed / 0 failed / 4 ignored (exact baseline); fresh
  `rm -rf target/doc && cargo docs` zero warnings, new-type pages generated;
  `scripts/check_semver.sh` — see expected-breaking list below.

### M1 expected-breaking list (vs `api-baseline`; do NOT move the baseline)

`check_semver.sh` reports exactly two failed major categories, both the spec'd
M1 reshaping surface:
1. `constructible_struct_adds_field`: `WhitespaceRules.enabled`,
   `TokenRules.paragraphs`, `TokenRules.specials`, `TokenRulesOverrides.paragraphs`,
   `TokenRulesOverrides.specials`.
2. `struct_pub_field_missing`: the eight old `TokenRules` fields
   (enable_whitespace, enable_multi_newline_paragraphs, enable_groups,
   temporary_groups, enable_commands, enable_comments, enable_specials,
   expecting_group_close) and the mirrored eight on `TokenRulesOverrides`.
Additionally (not surfaced by the tool's lints): `Default` removed from
`WhitespaceRules` (reviewer ruling, M1 fix pass); `TokenRules`/`TokenRulesOverrides`
field types changed to the new sub-structs; new public types
{Whitespace,Paragraph,Group,Command,Comment,Specials,ForbiddenChars}Rules/-Overrides
exported via `techy::core` (additive).

- **Supervisor: M2 normalization prototype (PLAN M2 step "prototype first")** —
  RESULT: the spec's preferred subtrait spelling
  `trait LangHasGroups: Lang where Self::Features: LangFeatures<Groups = FeaturePresent> {}`
  does NOT propagate the equality to users (rustc 1.97.0, E0271 at every
  `L: LangHasGroups` use site: trait where-clauses are obligations on
  implementors, not implied bounds for users). The associated-type-bounds
  spelling in SUPERTRAIT position propagates fully:
  `trait LangHasGroups: Lang<Features: LangFeatures<Groups = FeaturePresent>> {}`
  with blanket impl
  `impl<L: Lang> LangHasGroups for L where L::Features: LangFeatures<Groups = FeaturePresent> {}`.
  Verified by standalone compile+run prototype: under `L: LangHasGroups`,
  generic code (a) writes PLAIN struct literals for a field typed
  `<<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<Vec<u32>>`,
  (b) reads through the equality (`.len()` on the store), and (c) unbounded
  `L: Lang` code can use `<L::Features as LangFeatures>::Groups::PRESENT` as a
  const guard. ADOPTED: the ATB-supertrait spelling — it delivers the preferred
  shape's outcome (single-name `L: LangHasGroups` bounds); the use-site
  fallback (`where L::Features: LangFeatures<Groups = FeaturePresent>` at each
  site) is NOT needed. Prototype source preserved at
  dev-docs/langfeatures-plan/normalization_proto.rs.

- **M2 implementer D** — M2 step 1: new public items landed, all-present everywhere
  (crate GREEN throughout).
  - **Placement**: new internal module `techy/src/state/features.rs` (sibling of
    lang.rs), `mod features;` in state/mod.rs, exported via state/mod.rs → the core
    facade — one canonical path each under `techy::core` (flat), 14 items:
    `FeaturePresence` (sealed via the private-supertrait pattern, matching the
    error.rs `mod sealed { pub trait Sealed {} }` precedent; impls for the two
    markers only), `FeaturePresent`, `FeatureAbsent`, `LangFeatures` (not sealed),
    `AllLangFeatures`, `NoLangFeatures`, and the eight `LangHas*` subtraits in the
    ADOPTED ATB-supertrait spelling with blanket impls. `LangHasParagraphs` carries
    the `LangHasWhitespace` supertrait edge; its blanket impl requires both
    equalities in one bound
    (`LangFeatures<Whitespace = FeaturePresent, Paragraphs = FeaturePresent>`). The
    double Lang mention in `LangHasParagraphs: LangHasWhitespace + Lang<Features: …>`
    compiles fine (rustc 1.97), as the prototype predicted.
  - **GAT bounds chosen**: `type Store<T: Clone + Debug + Default>: Clone + Debug +
    Default`. `PartialEq`/`Eq` DROPPED from the spec's target composition: the Scopes
    payload — ScopeStack's inner Vec, element type `Arc<dyn SpecsProvider<L>>`
    (scopes/mod.rs) — genuinely lacks them (`SpecsProvider` is only
    `fmt::Debug + Send + Sync`; trait objects have no equality; ScopeStack itself
    implements only Clone/Debug/Default, and the derivation memo keys scope data by
    Arc identity, never by `==`). Every other payload (bool, Arc<str>, the three
    `Vec<Arc<…Rule>>`, `Option<Arc<GroupRule>>`, all `Option<…>` override mirrors)
    also satisfies PartialEq+Eq — the drop is solely the scopes payload's doing.
    M3 note: the rules sub-structs' manual PartialEq impls will need per-impl
    `Store<…>: PartialEq` where-clauses (both markers' stores satisfy them —
    `PhantomData` is unconditionally Eq — the GAT just can't promise it).
    `Arc<str>: Default` needs Rust ≥ 1.80; workspace rust-version is 1.86, fine.
    No explicit Send/Sync bounds (both stores track `T`'s auto traits; stated in
    rustdoc). `FeatureAbsent::Store<T> = PhantomData<T>` (composes; no dedicated ZST
    needed). Store carries the not-yet-used-by-any-field reservation note in rustdoc.
  - **Payload verification**: `#[cfg(test)] mod compile_checks` in features.rs —
    `assert_store_payload::<T>` instantiated once per payload (13 types, the
    dyn-provider Vec included) via a fn-pointer `const _`; presence consts, bundle
    members, absent-store ZST + present-store transparency (size_of), and the
    normalization probes (plain struct literal + plain read under
    `L: LangHasGroups`, const guard under bare `L: Lang`, an all-eight-subtraits
    bound satisfied by a TrivialLang) are all `const`/type-level checks —
    deliberately NO new `#[test]` fns, so the 884 baseline stays exact.
  - **`impl Lang for` sites touched (all 40)**: `type Features = AllLangFeatures;`
    on the TrivialLang blanket (state/lang.rs) and on `Latexlike`
    (latexlike/mod.rs, pinned per user ruling, one-line doc on the impl); the 38
    hand-written test impls got the fully-qualified one-liner — token/reader.rs ×4,
    spec/mod.rs ×1, state/parsing_state.rs ×4, constructs/{verbatim_parser,
    argument_parsers, environment_parser, attached_source}.rs ×1 each,
    constructs/nodes_parser.rs ×11, latexlike/mod.rs (`Flavored`) ×1,
    engine/language.rs ×3, engine/mod.rs ×6, node/mod.rs ×3, scopes/mod.rs ×1.
    No `impl Lang` exists in techy/tests or techy-derive (grep-verified); guide
    doctests define languages only via TrivialLang, so nothing else breaks.
  - **Docs**: rules.rs TokenRules narrative extended from two to three spellings of
    "off" (the M1 non-doc `//` pointer replaced with real doc text; absent /
    disabled / empty each defined in place; no dd-dr labels in public rustdoc).
    `Lang::Features` doc'd as the first associated type; TrivialLang's defaults
    parenthetical, state/mod.rs and core/mod.rs module docs extended minimally.
  - **Gates**: `cargo build` clean; `cargo test --workspace` 884 passed / 0 failed /
    4 ignored (exact baseline; 758+30+8+21+1+66; 2+2 ignored), zero warnings;
    fresh `rm -rf target/doc && cargo docs` zero warnings, all 14 new pages
    generated under target/doc/techy/core/.
  - **Surprises**: one — an underscore-*named* const (`const _FOO: …`) does not root
    dead-code liveness, so the payload instantiation list must be an anonymous
    `const _: …` or the assertion fn warns as unused. Everything else behaved
    exactly as the supervisor's prototype predicted.

## Questions for user

(genuine design ambiguities; the most conservative spec-consistent option was chosen and is noted here)

## Hand-off notes

(state a fresh supervisor needs beyond PLAN.md + this file + git log)

- Stages D and M1 are DONE on branch `lang-features` (this worktree); all gates
  green at commit d6054f9. Next stage: M2 per PLAN.md (`Lang::Features` + const
  gating; normalization prototype first; subtrait supertrait-vs-direct-bound
  spelling must be verified by compile prototype and the choice recorded).
- Seams M1 deliberately left for M2/M3: (a) per-block `pub(crate)`
  merge_from/apply methods in techy/src/state/delta.rs are the per-feature
  gating hooks; (b) the single generic-core field-path mutate site is
  parsing_state.rs `derived()` temporary-group `clear()` (commented in code);
  (c) state_memo.rs hash_key/keys_eq carry the comment that gated-absent blocks
  hash as nothing only at M3.
- Path convention: PLAN.md's `src/...` = `techy/src/...` (cargo workspace with
  `techy/` and `techy-derive/`).
- Baseline test totals to preserve while features stay all-on: 884 passed /
  0 failed / 4 ignored (758+30+8+21+1+66; 2+2 ignored).
- `api-baseline` branch untouched (per rules); M1's expected-breaking list is
  recorded above and will grow at M2 (`Lang` gains an associated type — every
  hand-written `Lang` impl breaks by one line).
- One-time environment note: `cargo fetch` had to run unsandboxed once to
  populate ~/.cargo (clap dev-dep); everything since runs sandboxed.
