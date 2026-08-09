# lang-features: Progress

- **Worktree**: `/Users/philippe/projects/techy/.claude/worktrees/agent-a01480fa109b19cef`
- **Branch**: `lang-features`
- **M3 worktree**: `/Users/philippe/projects/techy/.claude/worktrees/agent-abcf5ade530ed3cd0`
  (branch `lang-features-m3`, off `lang-features`; fast-forwards into the chain at merge time)
- **Plan**: see `dev-docs/langfeatures-plan/PLAN.md` (Design Spec is user-ruled; do not relitigate)

## Status

| Stage | Description | Status |
|-------|-------------|--------|
| D | Decision record [§dd-dr:lang-features] + ARCHITECTURE refs + superseded-names + CompileTimeFeatureGates.md status line | done (7a113e8 + fixes d28a3e9) |
| M1 | TokenRules/Overrides regrouped into per-feature blocks (pure reshaping, behavior identical) | done (d7f480f + 9bb4ac9 + 286edf2 + final fix/gate commit) |
| M2 | `Lang::Features` + const gating | done (d5d6aa8 + d673edf + c955741 + a0bf306 + 54cbe7d + final fix/gate commit) |
| M3 | Uniform storage gating (FeaturePresence::Store) | in progress (worktree agent-abcf5ade530ed3cd0, branch lang-features-m3) |
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

- **M2 implementer E** — M2 step 2: const feature guards at every listed reachability
  site (crate GREEN; all guards fold to the old behavior under AllLangFeatures).
  Guard style everywhere: `<L::Features as LangFeatures>::X::PRESENT` in plain `if`s
  (`FeaturePresence` must be imported for `::PRESENT` to resolve — added alongside
  `LangFeatures` in each touched file).
  - **Guard sites, one line each**:
    - token/reader.rs — skip_whitespace early-out (Whitespace), its in-loop paragraph
      check (Paragraphs), detect_paragraph_break (Paragraphs AND Whitespace, atop the
      existing dual runtime check), commands branch (Commands), read_comment
      (Comments), specials branch (Specials, before `trigger_chars().may_start`),
      forbidden-chars branch (ForbiddenChars). Plus a new TokenReader contract bullet:
      absent features' token kinds must never be produced (this documents the contract
      whose violation the dispatch guards report).
    - token/prefix_table.rs — for_rules returns the empty table when Groups is absent
      (same early-out as the runtime gate; rules data contributes nothing).
    - constructs/nodes_parser.rs — Command / Specials / GroupOpen / ParagraphBreak
      arms each return `cx.implementation_error(…)` when their feature is absent
      (guard first thing in the arm, so recovery placeholders of impossible kinds are
      covered too). Funnel choice: with the reader guarded, an arm firing for an
      absent feature means the *token source* (custom reader / recovery token)
      violated its contract — mirrored the file's implementation-bug treatment
      (`cx.implementation_error`, aborts under any policy, never a panic), not
      `recover_as_chars` (which is for source conditions).
    - state/parsing_state.rs — derived()'s temporary-group stripping seam wrapped in
      `if Groups::PRESENT`; the scope-op apply path guarded via delta.rs (below).
    - state/delta.rs — gated-overrides contract (below).
    - engine/state_memo.rs — hash_key and keys_eq skip absent features' blocks, in
      lockstep (same consts guard the same fields in both fns; original field order
      kept, expecting_close still hashed last); M1 comment updated (hash gating now,
      storage collapse still M3). Soundness note recorded in the comment: violating
      deltas fail derivation and failures are never cached, so no absent-block data
      can enter the memo and collide.
    - engine/language.rs — the root stray-close arm (StopCause::UnexpectedGroupClose)
      returns implementation_error when Groups is absent (same funnel as item 3).
  - **Gated-overrides semantics as implemented** ([§dd-dr:lang-features] "gated
    overrides" + "absent wins over runtime data"): "carries data" = the override
    block has any non-`None` field (all-`None` carries nothing — checked as
    `block != Default::default()`). On application (`TokenRulesOverrides::apply` /
    `ParsingStateDelta::apply_overrides`): present features apply as before; an
    absent feature's block is NEVER applied (absent wins), and if it carries data the
    violation is collected into the new error `AbsentFeatureOverrideError`
    (state/delta.rs; `features()` lists the block names). Scope ops count as the
    Scopes feature's data: with Scopes absent, a non-empty `scope_ops` list is the
    same violation ("scopes") and no op applies. The rest of the delta (mode, ext,
    other blocks) still applies; `derived()` folds the report into `DeriveError`
    alongside scope-op failures, with the recovered state carried as usual. In-parse
    (`ParseContext::recover_derive_failure`), the violation aborts as an
    `ImplementationError` under ANY recovery policy — same class as a finalize
    refusal (delta-author wiring bug, not a source condition). What does NOT error:
    all-`None` blocks for absent features; mode/ext overrides; events.
  - **Error-plumbing signature changes** (join the M2 expected-breaking list):
    (1) `TokenRulesOverrides::apply` now returns
    `Result<(), AbsentFeatureOverrideError>` (was `()`); sole caller was
    apply_overrides. (2) `DeriveError` gains pub field
    `absent_overrides: Option<AbsentFeatureOverrideError>` (semver:
    constructible_struct_adds_field now also lists DeriveError.absent_overrides —
    the only new row vs the M1 list); its "at least one of" invariant now spans
    three fields. (3) New public type `AbsentFeatureOverrideError`
    (Debug/Clone/PartialEq/Eq + Display + Error, FinalizeError-style), one canonical
    path via techy::core (additive). Internal: `apply_overrides` returns
    `(Vec<ScopeOpError>, Option<AbsentFeatureOverrideError>)`; private helper
    `TokenRulesOverrides::apply_to_present_features` is the shared gated core.
  - **merge_from choice**: [§dd-dr:lang-features] is silent about delta-to-delta
    merging, so ALL merge_from seams (per-block, TokenRulesOverrides,
    ParsingStateDelta) stay ungated — merging is data plumbing; application is the
    single enforcement point (a merged violating block still errs when applied).
  - **Consequence worth flagging**: `TokenRulesOverrides::disable_all()` carries
    `enabled: Some(false)` for six blocks, so applying it under a partially-absent
    language errors on the absent ones — intended per the entry (an override is an
    override), but the M2 test-language implementer should construct per-feature
    disables, not disable_all(), for partial languages.
  - **Unlisted candidate sites flagged for the reviewer** (deliberately NOT guarded —
    PLAN lists guards exhaustively):
    - reader.rs detect_group_delimiter's expecting-close check: not gated on Groups;
      unreachable via deltas now (they error) but a Lang seed/customizer violating
      its contract could still plant expecting_close data under absent Groups; M3's
      Store collapse makes that unrepresentable.
    - nodes_parser Comment arm (PLAN lists only Command/GroupOpen/specials/
      paragraph): an impossible Comment token would be processed as content instead
      of erroring.
    - nodes_parser token_stop conditions (GroupClose/ParagraphBreak stop kinds) and
      group_close_type: consult group/paragraph machinery unguarded — harmless (the
      kinds cannot be tokenized) but reviewable.
    - parsing_state freeze_with_table: `L::specials_trigger_chars` is still consulted
      when runtime data says enabled even under absent Specials (the reader's guarded
      specials branch makes the result unreachable); a Specials::PRESENT guard there
      would also spare the hook call.
    - engine/mod.rs group_interior_state force-sets groups.expecting_close — under
      absent Groups that delta now errs loudly at derivation (reachable only through
      the guarded GroupOpen arm, so effectively dead); no change made.
  - **Docs touched** (absent behavior stated in one plain sentence each, no dd-dr
    labels in rustdoc): TokenReader contract bullet, StdTokenReader, skip_whitespace,
    detect_paragraph_break, read_comment, PrefixTable::for_rules,
    TokenRulesOverrides::apply (# Errors), ParsingState::derived (three failure
    sources; the "cannot fail" sentence qualified), DeriveError (+ field),
    ParseContext::derive_state # Failures + recover_derive_failure,
    ParserSession::derived_state ("overrides-only deltas cannot fail" corrected —
    they now fail exactly on the absent-feature violation; failures still never
    cached).
  - **Gates**: `cargo build` clean; `cargo test --workspace` 884 passed / 0 failed /
    4 ignored (exact baseline: 758+30+8+21+1+66; 2+2 ignored), zero warnings
    (`cargo check --workspace --tests` warning-free); fresh `rm -rf target/doc &&
    cargo docs` zero warnings, AbsentFeatureOverrideError page generated under
    techy/core/; `scripts/check_semver.sh` = the M1 two categories plus the one
    expected DeriveError row (above).
  - **Surprises**: only that `X::PRESENT` needs `FeaturePresence` in scope after all
    (E0599 with only `LangFeatures` imported — associated-const shorthand resolves
    through the trait, but the trait must be imported); the M2a compile_checks probe
    masked this via `use super::*`.

- **M2 implementer F** — M2 step 3: representative absent-feature test languages
  (crate GREEN; worked under the **2026-08-10 disable_all user ruling**, course-corrected
  mid-task: no test asserts that `disable_all()` errors on absent-feature languages —
  that behavior is being reworked to feature-aware-by-construction by a follow-up
  implementer; absent-feature test code uses per-feature `disable()` constructors and
  explicit literals only).
  - **Bundle compositions chosen**: NoLangFeatures = the existing public bundle
    (language `PlainCharsLang`). groups-only = test-local `GroupsOnlyLangFeatures`
    (Groups present, seven absent; language `GroupsOnlyLang`). callables-without-scopes
    = test-local `CommandsWithoutScopesLangFeatures` (Commands + Whitespace present,
    six absent; language `CommandsWithoutScopesLang`). Whitespace is present because
    command tokenization consumes post-space through `skip_whitespace`; Groups is
    ABSENT because zero-argument callables genuinely need no group machinery — the
    argument parsers that mint temporary group rules are the ones that would (their
    deltas would violate at application under absent Groups today; M3 gives them the
    `LangHasGroups` bound). Both custom bundles are test-local: **no new public API**.
  - **Layout**: one new integration test, techy/tests/lang_features.rs, public facade
    only, mirroring acceptance.rs idioms: `mod support` (the three langs, a
    `FixedTableResolver: CommandResolver` fixed command table, an `AfterEffectSpec`
    that routes a chosen delta through the in-parse funnel, generic
    outline/fingerprint/parse_ok_in helpers), test mods `plain_chars` / `groups_only` /
    `commands_without_scopes`, plus `feature_composition` (const asserts of the
    presence declarations + positive `LangHas*` bound instantiations; "callables do
    not imply scopes" pinned as a compile-time fact). No state_memo unit test added:
    state_memo.rs has no existing test module to follow, and absent-block key data is
    unreachable there by construction (violating deltas fail derivation; failures are
    never cached).
  - **Behaviors pinned** (all langs seeded with FULLY POPULATED TokenRules — every
    gate on, command/group/comment rules, whitespace chars, forbidden `@`, specials
    hooks scanning `~` where declared absent):
    - PlainCharsLang: the whole input (`a\cmd{b} %c\n\nd~e @f`) is ONE chars node —
      no Command/Group/Comment nodes, no paragraph split, no forbidden-char error,
      strict==tolerant, zero diagnostics; a delta carrying data for every block + a
      scope op errors with all EIGHT feature names in declaration order, nothing
      applied (recovered state unchanged, no providers); the empty delta derives
      cleanly.
    - GroupsOnlyLang: braces → Group nodes (span-exact outline, delimiters, interior);
      `\ % ~ @ \n\n` all inert inside chars runs; whitespace-absent reader contract
      pinned at token level: whitespace chars are ordinary `Char` tokens and every
      token's `pre_space` is EMPTY (never folded).
    - CommandsWithoutScopesLang: `\mark` → Callable node from the fixed table
      (span-exact, name, callable_type; no scope stack anywhere); `a\n\nb` = one chars
      run (Paragraphs absent under Whitespace present — the skip_whitespace in-loop
      guard); in-parse scope op (`\def` after-effect `push_provider`) aborts as
      ImplementationError under BOTH policies, message names "scopes", no panic; same
      funnel for comments-data override (`\raw`); out-of-parse `derived()` reports
      `features() == ["scopes"]` with `failures` empty and the present-feature parts
      of the delta still applied to `recovered`; explicit comments data errs
      (`["comments"]`) while an all-None absent block stays silent (whitespace
      override applies).
    - **Course-correction test**: `verbatim_state_delta` under absent Groups errs at
      application time — out-of-parse via `derived()` and in-parse through the funnel
      (`\verb` after-effect; ImplementationError, both policies, no panic). Asserts
      `features().contains("groups")` ONLY: the delta today also flips absent gates
      via `disable_all()` (report is currently paragraphs/groups/comments/specials),
      and that part disappears under the ruled disable_all rework — the assertion is
      deliberately robust to it. M2-transitional per the ruling; M3 replaces this
      with the `LangHasGroups` compile bound.
  - **Production fixes: NONE.** No guard bug surfaced — all 12 tests passed against
    the unmodified M2a/M2b code on the first run.
  - **Gates / totals**: `cargo build` clean; `cargo test --workspace` **896 passed /
    0 failed / 4 ignored** (758+30+8+21+1+66+12 per target; 2+2 ignored) — the 884
    baseline intact plus the 12 new tests; `cargo check --workspace --tests` zero
    warnings; fresh `rm -rf target/doc && cargo docs` zero warnings.
  - **Flags for the reviewer**: (a) `VerbatimArgumentParser` itself errors even
    *earlier* than `verbatim_state_delta` under a groups-absent language — its
    delimiter-probe delta carries paragraphs/groups/commands/comments/specials data;
    not pinned (the M3 bound moots it), the in-parse test routes the exact
    `verbatim_state_delta` value via an after-effect instead. (b) `ScopeStack::push`
    direct mutation stays unguarded at M2 (PLAN lists the delta seams only; M3's
    Store collapse makes it unrepresentable) — tests assert the scope_op/push_provider
    delta paths. (c) The `~` specials hooks are implemented on the specials-absent
    langs and the freeze-time `specials_trigger_chars` call does populate the trigger
    cache (implementer E's unlisted-site flag) — the reader guard makes it
    unreachable, which the `~`-inert assertions confirm.
  - **Surprising**: nothing — every expected span/outline held on first run.

- **M2 implementer G** — M2d: `disable_all()` feature-aware by construction (dated
  USER RULING 2026-08-10, verbatim: "TokenRulesOverrides::disable_all() must disable
  all *available* features and must never be able to fail" — overrides implementer
  E's flagged judgment call that disable_all() errors under partially-absent
  languages).
  - **Rework** (techy/src/state/delta.rs): `disable_all()` consults
    `<L::Features as LangFeatures>::X::PRESENT` per gated block and sets
    `enabled: Some(false)` only for features the language declares present; absent
    features' blocks stay at their all-`None` default (`forbidden_chars` untouched,
    as before). Consequence, stated in rustdoc: applying a `disable_all()`-based
    delta can NEVER produce `AbsentFeatureOverrideError`. Under `AllLangFeatures`
    the returned value is bit-for-bit the old one (six `Some(false)` gates) — the
    existing delta.rs unit test pins it (comment extended to say so). No signature
    change, no new public API, no new panics.
  - **Doc sweep**: disable_all() rustdoc rewritten to the ruled contract ("the
    scoped off for every feature the language has"; absent features simply not
    mentioned by the returned value); the six per-block `disable()` docs now say
    disable_all() sets them up "when the language has the feature";
    `TokenRulesOverrides::apply` # Errors and the `AbsentFeatureOverrideError` docs
    note the error is triggered only by explicitly authored data — disable_all()
    never produces it (the error type itself REMAINS at M2, unweakened);
    verbatim_state_delta's "every tokenization feature gate off" now reads "every
    feature the language has". DR amendments (dated, per the [§dd-dr:enable-flags]
    precedent): [§dd-dr:lang-features] gains the 2026-08-10 ruling note (by
    construction; never fails; loud-failure stance for authored data unchanged);
    [§dd-dr:takeover-staging-sugar] item 1's "all six gates" description amended to
    presence-conditional. F's verbatim-test comment updated (the rework landed; the
    report under CommandsWithoutScopesLang is now exactly ["groups"] — the assertion
    was already robust to this and is untouched). E's "consequence worth flagging"
    log bullet above is superseded by this entry.
  - **Tests** (techy/tests/lang_features.rs, +3, F's 12 untouched): PlainCharsLang —
    disable_all() equals the all-`None` default and derives cleanly; GroupsOnlyLang
    — flips exactly the groups gate, applies cleanly; CommandsWithoutScopesLang —
    flips exactly whitespace+commands, absent blocks all-`None`, applies cleanly
    with absent-data seed untouched (constructed value AND application asserted in
    each).
  - **Gates**: `cargo build` clean; `cargo test --workspace` **899 passed / 0
    failed / 4 ignored** (758+30+8+21+1+66+15 per target; 2+2 ignored — the 896
    baseline intact plus the 3 new tests); `cargo check --workspace --tests` zero
    warnings; fresh `rm -rf target/doc && cargo docs` zero warnings.
  - **Surprises**: none — all guards were already in place from M2b; the rework is
    purely constructor-side, and no production file other than delta.rs (code) and
    verbatim_parser.rs (doc sentence) needed touching.

- **M2 reviewer** — full-diff review of c1d8a62..54cbe7d (five commits: prototype
  verdict, M2a, M2b, M2c, M2d) against PLAN M2, [§dd-dr:lang-features] as amended,
  [§dd-dr:superseded-names], [§dd-dr:panic-policy] rule 3, [§dd-arch:naming], and the
  2026-08-10 disable_all ruling. Findings: **0 blocker, 1 should-fix, 2 nit**. Gates
  re-run independently, all green.
  - should-fix (techy/src/token/reader.rs, `TokenReader` trait contract bullet):
    the new "Absent features yield no tokens" bullet claims the parsing machinery
    "reports an implementation error instead of processing it" for every absent
    feature's token kind and names `Comment` among them — but the nodes_parser
    `Comment` arm is deliberately unguarded (PLAN's arm list is exhaustive:
    Command/GroupOpen/Specials/ParagraphBreak only, per implementer E's log), so a
    contract-violating `Comment` token would be processed into a comment node, not
    reported. Public-contract overclaim: either soften the bullet to the four
    funneled kinds (+ stray GroupClose via the language.rs arm), or a user ruling
    adds a fifth guard beyond the PLAN list.
  - nit (dev-docs/ARCHITECTURE.md:697, PRE-EXISTING, outside this diff): the
    Phase-3 prose "`disable_all` and the collection constructors remain pending
    their stage" contradicts [§dd-dr:takeover-staging-sugar]'s applied notes (item 1
    at S2, item 2 at S3). Adjacent to the newly amended entry; M4-sweep candidate.
  - nit (dev-docs/DESIGN_RATIONALE.md [§dd-dr:lang-features] amendment): "only the
    crate's own constructor consults the declarations" — singular means
    disable_all(), but the six per-block `disable()` constructors are also crate
    constructors and stay presence-blind (correctly so, G's judgment call); one word
    ("the crate's own `disable_all()` constructor") would remove the misreading.
    The delta.rs rustdoc already says it precisely.
  - Everything else verified clean: public inventory exactly PLAN + E's plumbing
    (14 feature items + AbsentFeatureOverrideError + apply→Result +
    DeriveError.absent_overrides; F/G bundles test-local, nothing else new-public,
    one canonical path each via techy::core); FeaturePresence genuinely sealed
    (private-supertrait), LangFeatures open; all eight LangHas* in the ADOPTED
    ATB-supertrait spelling with blanket impls, Paragraphs→Whitespace the only
    edge; GAT bounds Clone+Debug+Default with the PartialEq/Eq drop justified
    (dyn SpecsProvider payload — pinned by the 13-payload compile_checks roster);
    const guards at every PLAN site (reader ×7 incl. skip_whitespace in-loop,
    PrefixTable::for_rules, four nodes_parser arms via implementation_error,
    derived() temporary-strip, delta apply/apply_overrides incl. scope_ops,
    state_memo hash/eq in verified lockstep with original field order + updated M1
    comment, language.rs stray-close) — absent wins over populated runtime data,
    const-foldable `if X::PRESENT` style throughout, zero new
    panic/unwrap/expect/unreachable in lib code (only `const _` asserts in
    cfg(test)); E's unguarded list all genuinely M3-scope (verbatim probe,
    ScopeStack::push, freeze-time trigger cache, group_interior_state — the Comment
    arm is the should-fix's doc side only; also noted for M3: finalize_transition
    can still mutate an absent block directly, same Store-collapse category as
    push); 2026-08-10 ruling implemented faithfully (disable_all feature-aware by
    construction, never fails, ruled rustdoc wording, per-block disable()
    presence-blind without contradicting the ruling, dated DR amendments in both
    entries, disable_all sweep clean of old-behavior claims); test languages match
    PLAN (populated-seed absent-wins, in-parse violations → ImplementationError
    both policies no panic, gated overrides both directions, verbatim transitional
    pin robust to the rework, compositions pinned at type level); rustdoc complete
    (three spellings on TokenRules each defined; subtraits say never-hand-implemented;
    no dd-dr labels in public rustdoc, no banned names, no "facet", spec names
    exact); behavior identity holds (pre-M2 tests changed only by `type Features`
    one-liners; delta.rs disable_all test values untouched).
  - **Gates**: `cargo build` clean; `cargo test --workspace` 899 passed / 0 failed /
    4 ignored (758+30+8+15+21+1+66; 2+2 ignored — baseline 884 + F's 12 + G's 3);
    fresh `rm -rf target/doc && cargo docs` zero warnings;
    `scripts/check_semver.sh` = exactly the M1 two categories plus ONE new row.

### M2 expected-breaking list (vs `api-baseline`; supersedes-extends the M1 list)

Surfaced by the tool (2 failed categories, 194 pass / 58 skip):
1. `constructible_struct_adds_field`: **`DeriveError.absent_overrides` (new at M2)**
   plus M1's five rows (`WhitespaceRules.enabled`, `TokenRules.paragraphs`,
   `TokenRules.specials`, `TokenRulesOverrides.paragraphs`,
   `TokenRulesOverrides.specials`).
2. `struct_pub_field_missing`: unchanged from M1 (the 8 old `TokenRules` fields +
   the mirrored 8 on `TokenRulesOverrides`).

Breaking but NOT surfaced by the tool (v0.50 has no lint row for either):
- `Lang` gains the required associated type `Features` — every hand-written
  external `Lang` impl breaks by one line (the M2 headline break);
- `TokenRulesOverrides::apply` returns `Result<(), AbsentFeatureOverrideError>`
  (was `()`);
- carried over from M1 (recorded there): `Default` removed from `WhitespaceRules`;
  `TokenRules`/`TokenRulesOverrides` field types changed to the sub-structs.
Additive (non-breaking): the 14 feature items (`FeaturePresence`, `FeaturePresent`,
`FeatureAbsent`, `LangFeatures`, `AllLangFeatures`, `NoLangFeatures`, eight
`LangHas*`) + `AbsentFeatureOverrideError`, all via `techy::core`. Nothing in the
semver output is unexplained.

- **Supervisor: M2 fix pass + closure** — applied the M2 reviewer's findings
  directly (doc-only): TokenReader "Absent features yield no tokens" bullet
  softened to the actually-funneled kinds (Command, GroupOpen, stray
  GroupClose, Specials, ParagraphBreak; Comment not intercepted — see
  Questions for user); DR [§dd-dr:lang-features] amendment's
  singular-"constructor" phrasing sharpened (only `disable_all()` consults
  the declarations; the per-block `disable()` constructors stay
  presence-blind). ARCHITECTURE.md:697 stale-line nit deferred to the M4
  sweep (pre-existing, outside the M2 diff). Final gates re-run after the
  fixes — results in the hand-off section (build/test/docs; semver unchanged
  by doc-only fixes, reviewer's run stands).

## Questions for user

(genuine design ambiguities; the most conservative spec-consistent option was chosen and is noted here)

- **Should nodes_parser's Comment arm get an absent-feature guard?** PLAN M2's
  dispatch-arm list is exhaustive (Command, GroupOpen, specials, paragraph) and
  deliberately omits Comment; the M2 reviewer found the TokenReader rustdoc
  overclaimed ("reports an implementation error" for *every* absent feature's
  token kind, naming Comment). Conservative option taken: the DOC was softened
  to match the code (Comment tokens from a contract-violating reader are not
  intercepted at M2); no fifth guard added beyond the PLAN list. If a Comment
  guard is wanted, it is a one-arm addition mirroring the other four
  (implementation-error condition), best decided before/at M3.

## Hand-off notes

(state a fresh supervisor needs beyond PLAN.md + this file + git log)

### M2 → M3 hand-off (supervisor, M2 closure)

- Stages D, M1, M2 are DONE on branch `lang-features`; all gates green at the
  final M2 commit. The M3 successor should work from a FRESH worktree with a
  branch off this one (e.g. `git switch -c lang-features-m3 lang-features`) —
  do not reuse this worktree.
- M3 scope: PLAN.md "Stage M3" + the ruled M3 bullet below (revert `apply()`
  to infallible, remove `AbsentFeatureOverrideError` once ZST stores +
  `LangHasScopes` bound make the channel unreachable).
- M3 sites flagged during M2 (all logged in the M2 entries above): the
  verbatim family + `VerbatimArgumentParser` delimiter-probe delta
  (LangHasGroups bound moots the transitional application-time error F
  pinned); `ScopeStack::push` direct mutation (Store collapse + LangHasScopes
  bound); `finalize_transition` can still mutate an absent block directly
  (same Store-collapse category); freeze-time `specials_trigger_chars` cache
  populates under absent Specials (collapses with the Specials feature per
  PLAN M3 "derived caches collapse with their features");
  `group_interior_state` (E's list). GAT note: rules sub-structs' manual
  PartialEq impls will need per-impl `Store<…>: PartialEq` where-clauses
  (both markers satisfy them) — the GAT itself is Clone+Debug+Default only.
- M4-sweep candidates: dev-docs/ARCHITECTURE.md:697 pre-existing stale line
  ("`disable_all` and the collection constructors remain pending their
  stage" — contradicts [§dd-dr:takeover-staging-sugar] applied notes); the
  Stage-D judgment call that [§dd-arch:state]'s reference to
  [§dd-dr:lang-features] should be promoted from the decisions list into body
  prose once the code exists (it now does).
- Open user question (see "Questions for user"): optional Comment-arm guard.
- Test totals after M2: 899 passed / 0 failed / 4 ignored
  (baseline 884 + 12 M2c + 3 M2d).

### M2 expected-breaking list (vs `api-baseline`; baseline NOT moved)

`check_semver.sh` after M2: 196 checks, 194 pass, 2 fail — the same two M1
categories, with one new row:
1. `constructible_struct_adds_field`: M1's five rows + NEW
   `DeriveError.absent_overrides`.
2. `struct_pub_field_missing`: M1's 8+8 old field names (unchanged).
Breaking but not surfaced by the tool: `Lang` gains required associated type
`Features` (every hand-written external `Lang` impl breaks by one line — the
M2 headline); `TokenRulesOverrides::apply` now returns `Result`; carried from
M1: `Default` removed from `WhitespaceRules`, field types changed to
sub-structs. Additive: the 14 feature items (LangFeatures, FeaturePresence,
FeaturePresent, FeatureAbsent, AllLangFeatures, NoLangFeatures, eight
LangHas*) + `AbsentFeatureOverrideError`, all via `techy::core`. Nothing in
the semver output is unexplained.

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
- **M3 instruction (part of the 2026-08-10 user ruling)**: once M3's storage
  gating makes a data-carrying absent block unconstructible (ZST stores) and
  `scope_ops` is compile-bounded by `LangHasScopes`, the runtime error channel
  becomes unreachable — M3 must then revert `TokenRulesOverrides::apply()` to
  infallible and remove `AbsentFeatureOverrideError` from the public surface. The
  transitional plumbing must not outlive its milestone; the error type is NOT
  load-bearing. Related, pinned by test (implementer F):
  `verbatim_state_delta` under a groups-absent language errors at application
  until M3's `LangHasGroups` bound lands — accepted transitional behavior.
