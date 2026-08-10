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
| M3 | Uniform storage gating (FeaturePresence::Store) | done (f6cb54e + fc33dee + 81726d7 + d3da11b + review ab11dfd + final fix/gate commit; branch `lang-features-m3`, ready to fast-forward into `lang-features`) |
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

- **Supervisor: M3 prep (design mechanics + baseline numbers)** — decisions taken
  before dispatching implementers (conservative, PLAN- and DR-consistent; all are
  implementation mechanics under the ruled spec, recorded here for review):
  - **M2-tree size baseline** (bytes, 64-bit, measured at 351c95b via a temporary
    probe test, then deleted; identical for all-present and NoLangFeatures — nothing
    collapses at M2): TokenRules 176, TokenRulesOverrides 184, ParsingStateDelta 240,
    StateData 200, ParsingState 232, ScopeStack 24. M3 gates: all-present sizes must
    stay exactly these; NoLangFeatures gated blocks collapse to 0.
  - **GAT payload bound relaxed**: `Store<T: Clone + Debug>: Clone + Debug` —
    `Default` dropped from both sides. Forced: the M3 payloads are the seven rules
    sub-structs (deliberately no `Default`, M1 reviewer ruling) and
    `Arc<PrefixTable<L>>` (no `Default`). Construction moves to `store_with` (below).
    `Clone`/`Debug` stay ON the GAT: generic code (state clones in `derived()`)
    must clone stores without feature bounds.
  - **Projection surface on `FeaturePresence`** (new trait fns — the only way generic
    unbounded code can read/write through a store, since `if X::PRESENT` does not
    narrow types): `store_with(impl FnOnce() -> T) -> Self::Store<T>` (present:
    calls it; absent: `PhantomData`, closure unused), `store_get(&Store<T>) ->
    Option<&T>`, `store_get_mut(&mut Store<T>) -> Option<&mut T>`,
    `store_into_inner(Store<T>) -> Option<T>` (for by-value merge paths). Public
    (they must be callable through the public `FeaturePresence` bound; the trait
    stays sealed so the two markers remain the only impls). Names checked against
    [§dd-arch:naming]: `store_` prefix answers "get *what*?" next to the sibling
    `Store` GAT; `get`/`get_mut` = the crate's non-panicking-Option convention.
  - **`ParsingStateDelta::scope_ops` is store-gated** (`Scopes::Store<Vec<ScopeOp<L>>>`),
    not merely method-bounded: the field is `pub`, so bounding only
    `scope_op`/`push_provider` would leave literal construction as a reachable
    violating path and the ruled error-channel retirement could not proceed
    (`ScopeOp` is an enum with public variants — its *construction* cannot itself
    be bounded without an unspec'd shape break; a free-floating `ScopeOp` value for
    a scopes-absent language stays representable but unusable: every place that
    could hold or apply one is gated or bounded).
  - **Derived-cache accessors return `Option`**: `ParsingState::prefix_table() ->
    Option<&PrefixTable<L>>`, `trigger_chars() -> Option<&TriggerChars>` (`None` iff
    the feature is absent; a present-but-disabled state still returns `Some` of the
    empty table/filter, as frozen). Reason: the fields collapse per PLAN, and no
    generic `'static` empty `PrefixTable<L>` can exist to back an unchanged `&`
    return (generic statics are impossible; `Vec`/`String` payloads bar promotion).
    Consistent with the accessor doctrine's "absent → false/empty/None". Public
    breaking change — joins the expected-breaking list.
  - **Task decomposition**: (A) features.rs projection surface + TokenRules storage
    gating + accessor internals + generic-read/mutate fallout + lang_features.rs
    seed rework; (B) delta.rs overrides + scope_ops gating + error-channel
    retirement (incl. DeriveError field removal) + state_memo + derived-cache
    collapse + engine fallout + verbatim-family LangHasGroups bounds (forced
    immediately by the override gating) + temporary-group-minting argument parsers;
    (C) ScopeStack inner-Vec gating + LangHasScopes bounds on mutating entry points
    + flagged-site sweep (finalize_transition, detect_group_delimiter,
    group_interior_state) + lang_features.rs scope-test rework + compile-fact
    additions; (D) size regression tests + numbers into PROGRESS. Then full-diff
    review, fixes, gates. Sequenced strictly A → B → C → D (overlapping files:
    parsing_state.rs, tests/lang_features.rs).

- **M3 implementer A** — projection surface + TokenRules storage gating landed
  (crate GREEN; tasks B/C/D untouched: overrides, scope_ops, ScopeStack, state_memo,
  derived caches, size tests all as before).
  - **features.rs**: `Store` GAT bound relaxed to `Clone + fmt::Debug` on BOTH sides
    (forced: the M3 payloads include the seven rules sub-structs — deliberately no
    `Default`, M1 reviewer ruling — and `Arc<PrefixTable<L>>`); the four supervisor-
    named projection fns added to the sealed `FeaturePresence` (`store_with`,
    `store_get`, `store_get_mut`, `store_into_inner`), implemented on both markers,
    each with plain present/absent rustdoc. `Store` rustdoc: reservation paragraph
    removed, the M3 storage roster stated (seven rules blocks, override blocks,
    scope-op list + scope stack, two derived caches), equality-not-promised kept,
    Default-not-promised added. compile_checks: 18-payload roster (the seven blocks,
    the seven override blocks imported via crate::state, `Vec<ScopeOp<FeatLang>>`,
    `Vec<Arc<dyn SpecsProvider<FeatLang>>>`, `Arc<PrefixTable<FeatLang>>`,
    `TriggerChars`) + a `projection_round_trip::<P>` probe instantiated for both
    markers — all const/type-level, no new `#[test]`.
  - **rules.rs**: the seven `TokenRules<L>` fields are the fully spelled
    `<<L::Features as LangFeatures>::X as FeaturePresence>::Store<XRules>` projections
    (no public alias); `empty()` builds every field via `store_with` and its doc says
    it answers for every language (present → block `empty()`, absent → zero-sized
    store); all 13 accessors keep exact signatures and project via
    `store_get`/`.and_then`/`.map_or` with the spec'd neutral answers; `Clone`/`Debug`
    unchanged and bound-free (the GAT carries them); `PartialEq`/`Eq` gained the
    recorded per-impl `Store<…>: PartialEq/Eq` where-clauses (×7 each). Docs: one
    absent-storage sentence per field; TokenRules narrative extended with the storage
    fact + a "constructing rules for a language with absent features" section whose
    example is a compiled doctest — **test totals therefore 900, not 899** (the one
    count change; the doctest is the recipe made checkable).
  - **derived() seam** (parsing_state.rs): the temporary-strip
    `data.rules.groups.temporary.clear()` now goes through `store_get_mut` if-let
    INSIDE the kept `if Groups::PRESENT` guard — kept, not folded: the guard
    compile-eliminates the whole enforcement block (incl. the `ends_temporary_scope`
    computation), which the if-let alone would not.
  - **delta.rs `apply_to_present_features`**: each PRESENT branch routes
    `&mut rules.X` through a `store_get_mut` if-let (no unwrap/expect — comment notes
    the branch guarantees `Some`); the else-carries-data structure and absent-name
    collection byte-identical (task B retires that channel).
  - **JUDGMENT CALL (review me): `LatexlikeLang` now pins
    `Features = AllLangFeatures` in its supertrait `Lang<…>` bound**
    (latexlike/lang.rs, + one doc sentence). Forced by transparency:
    `default_token_rules` and `exit_math_context_delta` are generic over
    `LLL: LatexlikeLang` and write plain literals/field paths, and the impl-level
    pins (Latexlike, Flavored) do not normalize stores under a generic `LLL`. This is
    the ruled "the LatexlikeLang family pins AllLangFeatures" made load-bearing at
    the trait; both existing impls already comply. Semver: a hypothetical external
    `LatexlikeLang` impl with different Features (nonsensical under the ruling) now
    breaks — add to the expected-breaking list at the M3 gate run.
  - **Generic bare-L test helpers** constructing full TokenRules literals got
    `Features = crate::state::AllLangFeatures` equality bounds (their languages all
    declare it): reader.rs `latex_rules`, parsing_state.rs `base_rules`,
    nodes_parser.rs `rules`, scopes/engine/node `min_rules`, plus the engine/node
    `state` helpers that call them. Concrete-language literals everywhere else
    compile UNCHANGED (transparency held — no stop condition hit).
  - **Inference fallout** (expected, access-syntax only): a gated field is an
    associated-type projection and no longer drives type inference — 14 reader.rs
    test locals annotated (`TokenRules<TestLang>`, one `TokenRules<SpecialsLang>`)
    and two turbofishes `default_token_rules::<Latexlike>()` (tests/acceptance.rs,
    latexlike/node_ref.rs). Zero assertion-value changes at those sites. Task B
    heads-up: gating the override blocks will hit the same inference sites in
    override-construction code.
  - **tests/lang_features.rs seed rework**: `support::fully_populated_rules()`
    DELETED (helper, not a test — unwritable by design now); PlainCharsLang seeds
    `TokenRules::empty()`, GroupsOnlyLang a populated groups literal (same values as
    before, `[`…`]` temporary rule included) + `..TokenRules::empty()`,
    CommandsWithoutScopesLang populated whitespace + commands + `..empty()`. Module
    doc and comments now say: carrying data for an absent feature is a compile-time
    error; the runtime pins remain as the behavior record. One test renamed
    (`fully_populated_rules_still_parse…` →
    `every_construct_spelling_parses_as_plain_character_content`); no test deleted;
    transitional error-channel tests untouched beyond what compiles.
  - **Assertion-value deviation, forced and flagged**: five assertions read
    absent-feature SEED data through accessors — such data is now unrepresentable
    (the point of M3), so they flip to the documented neutral answers:
    `commands_enabled()` true→false (×2, plain_chars), `comments_enabled()`
    true→false (commands disable_all test), `forbidden_chars()` "@"→"",
    `whitespace_chars()` " \t\n"→"" (plain_chars). Every parse-outcome assertion
    (outlines, spans, tokens, error identifiers/messages, feature reports) is
    value-identical.
  - **Gates**: `cargo build` clean; `cargo test --workspace` **900 passed / 0
    failed / 4 ignored** (per-target 758+30+8+15+21+1+67; 2+2 ignored — the 899
    baseline + the one new rules.rs doctest above, no other count change);
    `cargo check --workspace --tests` zero warnings on a forced full rebuild; fresh
    `rm -rf target/doc && cargo docs` zero warnings.
  - **Surprises**: only the inference fallout (projections don't unify backward);
    everything else — including `X::store_with(XRules::empty)` fn-item inference and
    the where-claused equality impls — behaved as the M2 prototype predicted.

- **M3 implementer B** — overrides/scope_ops storage gating landed; transitional error
  channel retired (user ruling 2026-08-10, M3 instruction). Crate GREEN. Tasks C/D
  untouched (ScopeStack inner Vec, finalize_transition/detect_group_delimiter flagged
  sites, size tests all as before).
  - **delta.rs**: the seven `TokenRulesOverrides<L>` fields and
    `ParsingStateDelta::scope_ops` are the fully spelled `Store<…>` projections (no
    public alias). `Default`/`disable_all()` build per field via
    `store_with(XOverrides::default/disable)` (disable_all value bit-for-bit under
    all-present languages — the delta.rs unit test passes untouched);
    `merge_from` (per-struct + delta) and `apply` use matched projections
    (`store_get_mut` on self, `store_into_inner` on the owned side; both stores share
    the presence marker, no unwrap/expect anywhere); `PartialEq`/`Eq` carry the
    per-impl `Store<…>: PartialEq/Eq` where-clauses ×7 (rules.rs pattern);
    `is_empty()` reworked per block under `store_get` (new pub(crate)
    `TokenRulesOverrides::is_empty`); `scope_op`/`push_provider` gained
    `where L: LangHasScopes` (transparent store under the bound — plain push);
    `apply_overrides` returns `Vec<ScopeOpError>` again, scope ops applied under the
    `store_get` projection (absent: no arm). The seven per-block override structs are
    UNCHANGED as types; their `disable()` constructors stay presence-blind.
  - **Unreachability verification (precondition for the retirement)**: with the
    gating in place, absent-feature override data and scope ops for a scopes-absent
    language are unrepresentable — checked every path: (a) public field literals —
    the fields are typed as the store projections; for an absent feature that is
    `PhantomData<XOverrides>`/`PhantomData<Vec<ScopeOp<L>>>`, which carries no data;
    (b) crate constructors — `Default`/`disable_all()`/`new()` go through
    `store_with`, which never fabricates a value for an absent feature; (c) merge
    paths (`merge_from` ×2, `lower_state_events`) — matched projections, absent
    merges nothing; (d) builders — `scope_op`/`push_provider` bounded by
    `LangHasScopes`; (e) deserialization — none (no serde anywhere in techy);
    (f) `ScopeOp` values themselves stay constructible but nothing generic can hold
    or apply one for a scopes-absent language (the supervisor-recorded stance). No
    remaining reachable path found → the channel was retired in full:
    `TokenRulesOverrides::apply` infallible again (`# Errors` gone,
    `apply_to_present_features` deleted), `AbsentFeatureOverrideError` removed
    (type + impls + facade re-exports + every doc mention — grep of techy/ and
    dev-docs/ is clean), `DeriveError.absent_overrides` field removed ("at least one
    of" invariant back to two sources; Display/Debug/docs updated),
    `recover_derive_failure`/`derive_state`/`derived()`/`derived_state` docs restored
    to the two-source story (engine/mod.rs "Overrides-only deltas cannot fail"
    restored verbatim from the pre-M2 text).
  - **Derived caches collapsed** (parsing_state.rs): `prefix_table` →
    `Groups::Store<Arc<PrefixTable<L>>>`, `trigger_chars` →
    `Specials::Store<TriggerChars>`; `freeze`/`freeze_with_table` build via
    `store_with` (the internal `freeze_with_table` signature now takes the store);
    the E-flagged freeze-time `L::specials_trigger_chars` call under absent Specials
    is retired — `store_with` never runs the closure for an absent feature (comment
    at the site); the `derived()` table-reuse path clones the store. Public
    accessors now `prefix_table() -> Option<&PrefixTable<L>>` /
    `trigger_chars() -> Option<&TriggerChars>`: `None` iff the feature is absent,
    present-but-disabled still `Some` of the frozen empty table/filter (rustdoc says
    exactly that) — joins the expected-breaking list. Call sites: reader.rs specials
    branch (`is_some_and` under the kept const guard), prefix-table match
    (`state.prefix_table()?.match_at(rest)?`), nodes_parser `group_close_type`
    (`and_then`); test sites use `.unwrap()`/`.expect("all-present test language")`,
    assertion VALUES untouched.
  - **engine + memo**: state_memo `hash_key`/`keys_eq` walk each block under
    `store_get` in lockstep, original field order, expecting_close still last
    (keys_eq via a local `stores_eq` helper whose impossible mixed arm answers
    `false` — conservative miss); M1/M2 comment updated (the absent block "does not
    even exist" now); Arc-identity keying byte-for-byte for present features;
    scope_ops is not hashed (the memoizable guard now projects it —
    `store_get(...).map_or(true, is_empty)`). `group_interior_state` force-sets
    expecting_close through `store_get_mut` (reachable only under the guarded
    GroupOpen arm — identical behavior for groups-present languages).
  - **Lattice bounds (the PLAN's exhaustive list, nothing beyond)**: LangHasGroups on
    `verbatim_state_delta`, `VerbatimArgumentParser` (inherent + ArgumentParser
    impls), `VerbatimBodyParser` (inherent + ConstructParser impls),
    `GroupArgumentParser` (inherent + ArgumentParser impls — judgment call: the type
    carries the bound as a whole; its rule form mints temporaries via
    `probe_minted_group`, also bounded, and the class form is a group argument
    either way), `OptionalGroupArgumentParser` (inherent + ArgumentParser impls).
    One plain rustdoc sentence on each. NOT bounded (projection instead, per the
    exhaustive-list rule): `CharsGroupArgumentParser::contents_delta` (writes
    groups-override data but the type is not in the ruled list — groups writes via
    `store_get_mut`, the commands/specials/comments blocks via `store_with`;
    one turbofish `store_get_mut::<GroupOverrides<L>>` for inference),
    `VerbatimArgumentParser::delimiter_probe_delta`'s non-groups blocks
    (`store_with` — only the groups store is transparent under LangHasGroups),
    `group_interior_state` (above). `ExpressionParser`/`MarkerArgumentParser`/
    embellishments/tack-on parsers carry no group data → unbounded.
  - **FORCED GAT AMENDMENT (task-A surface, review me)**: `FeaturePresence::Store`
    bounds tightened to `T: Clone + Debug + Send + Sync` on both sides (M2a had
    deliberately no explicit Send/Sync). Forced by the delta gating:
    `CallableSpec: Send + Sync` holds `ArgumentSpec<L>` which holds
    `Option<ParsingStateDelta<L>>`; under a bare `L: Lang` the auto-trait derivation
    cannot see through the GAT, so without the promise `StdCallableSpec`'s blanket
    impl (and every generic spec) fails E0277 ×16. All 18 roster payloads satisfy
    the bounds (compile_checks' `assert_store_payload` now requires them);
    both markers' stores are Send/Sync whenever `T` is, so the promise is free.
    FeaturePresence rustdoc's "no explicit Send/Sync" paragraph replaced by the
    reasoned bound sentence on `Store`.
  - **tests/lang_features.rs accounting (15 → 10 `#[test]` fns; every retirement is
    the ruled unrepresentability)**:
    `plain_chars::override_data_for_every_feature_is_reported_in_declaration_order`
    → DELETED (the all-eight-violations delta is a type error now; positive record
    kept by `an_empty_delta_derives_cleanly` + the M2d disable_all test);
    `an_in_parse_scope_op_aborts_as_an_implementation_error_not_a_panic` → DELETED
    (the `\def` after-effect delta needs `push_provider` under absent Scopes;
    replacement: `feature_composition::add_scope_op` positive LangHasScopes fact);
    `an_in_parse_override_carrying_comments_data_aborts_the_same_way` → DELETED
    (the `\raw` comments literal is a type error; replacement:
    `overrides_for_present_features_apply_cleanly` + module comment);
    `scope_ops_error_out_of_parse_and_the_rest_of_the_delta_still_applies` →
    DELETED (`.scope_op` bounded; its positive half lives on in the rework below,
    value "Z" unchanged);
    `explicit_data_for_an_absent_feature_errors_while_an_all_none_block_stays_silent`
    → REWORKED into `overrides_for_present_features_apply_cleanly` (the error half
    is unrepresentable; the apply half kept, assertion values identical);
    `verbatim_state_delta_errors_at_application_under_absent_groups` → DELETED
    (`verbatim_state_delta::<CommandsWithoutScopesLang>` no longer compiles;
    replacement: `feature_composition::mint_verbatim_delta` positive LangHasGroups
    fact). Support module: `AfterEffectSpec`/`AfterEffectParser` and the
    `\def`/`\raw`/`\verb` resolver arms removed (no test routes deltas through them
    anymore); `FixedTableResolver` keeps `\mark`. All M2d disable_all tests and
    every all-present behavior test keep exact assertion values.
  - **dev-docs**: dated M3 follow-through amendment appended to
    [§dd-dr:lang-features]'s 2026-08-10 amendment (channel unreachable → removed per
    the ruling's M3 instruction); no other dev-docs sentence presents the error type
    as current (grep clean).
  - **Expected-breaking additions for the M3 gate run** (semver deferred to closure
    per prep): `prefix_table`/`trigger_chars` return `Option`;
    `TokenRulesOverrides` field types + `ParsingStateDelta.scope_ops` type are now
    store projections; `scope_op`/`push_provider` bounded `LangHasScopes`; verbatim
    family + `GroupArgumentParser`/`OptionalGroupArgumentParser` bounded
    `LangHasGroups`; `TokenRulesOverrides::apply` returns `()` again;
    `AbsentFeatureOverrideError` + `DeriveError.absent_overrides` removed;
    `FeaturePresence::Store` GAT bounds gained `Send + Sync`.
  - **Gates**: `cargo build` clean; `cargo test --workspace` **895 passed / 0
    failed / 4 ignored** (per-target 758+30+8+10+21+1+67; 2+2 ignored — task A's 900
    minus exactly the 5 lang_features retirements above, no other count change);
    `cargo check --workspace --tests` zero warnings on a forced rebuild; fresh
    `rm -rf target/doc && cargo docs` zero warnings (no dangling links to the
    removed type).
  - **Surprises**: the Send/Sync GAT amendment (above) was the only structural one;
    plus one E0282 (a projected field no longer drives inference in
    `chars_group_parser` — turbofish, same class as task A's fallout).

- **M3 implementer C** — ScopeStack storage gating + `LangHasScopes` on the mutating
  entry points + flagged-site sweep. Crate GREEN. Task D untouched (size regression
  tests still pending).
  - **scopes/mod.rs**: `ScopeStack<L>`'s private `stack` field is now the fully
    spelled `Scopes::Store<Vec<Arc<dyn SpecsProvider<L>>>>` projection;
    `new()`/`Default` build via `store_with(Vec::new)` (unbounded — every language
    constructs the empty stack inside `StateData`). One private read helper
    `entries() -> &[Arc<dyn SpecsProvider<L>>]`
    (`store_get(...).map_or(&[], ...)`) routes every read-only method —
    `providers`, `provider_names`, `SearchedProviders`' Display (via
    `provider_names`/`is_empty`), `len`, `is_empty`, `retrieve_spec`,
    `scan_specials`, `specials_trigger_chars`, `iter_symbols` — so absent = the
    permanently empty stack, every fold/search giving its empty-stack answer
    (`check_provider_commands_shadowed_by_escape` already reads via `providers()`,
    no change needed). Clone unchanged (store clone); Debug keeps the "providers"
    field label (absent prints the zero-sized marker). `push()` gained
    `where L: LangHasScopes` (body unchanged — the store is transparent under the
    bound), one plain rustdoc sentence; struct-level rustdoc paragraph states the
    storage gating. Private helpers `route_definition`/`innermost_position` now take
    the projected `&mut [..]`/`&[..]` (borrow-friendly under `apply_op`'s
    projection).
  - **`apply_op` stays unbounded** (PLAN's bound list is exhaustive; the generic
    delta path in delta.rs calls it under bare `L: Lang`): body projects via
    `store_get_mut`; `None` (scopes absent) returns the NEW
    `ScopeOpError::ScopesAbsent` (unit variant, additive under `#[non_exhaustive]`;
    Display: "the language declares the scope stack absent; no scope operation can
    apply"; variant rustdoc records it is reachable only by direct `apply_op` calls
    — a delta can never carry scope ops for such a language). Supervisor-decided
    loud failure, recorded under Questions below.
  - **FORCED BOUND (review me): `ParsingState::lang_initial_with_packages` gained
    `where L: LangHasScopes`** — it pushes providers onto the seed's stack under a
    bare `L: Lang`, which no longer compiles with `push` bounded; a scope-mutating
    entry point per PLAN M3's bound line. Silent package drop and a runtime error in
    the documented-infallible constructor were both ruled out by the design entry.
    Every existing caller is a concrete all-present language (latexlike, tests,
    extract doctest); doc sentence added pointing scopes-absent languages at
    `lang_initial()`. Joins the expected-breaking list with `push`.
  - **Flagged-site sweep** (all closed, little code): `Lang::finalize_transition` —
    nothing in src assumes it can touch an absent block (the fields are zero-sized
    stores since M3a/b); its rustdoc carries no M2-era claim. reader.rs
    `detect_group_delimiter` — no stale caveat comment existed; the expecting-close
    read goes through `expecting_group_close()` (accessor answers `None` under
    absent Groups) and the prefix-table comment is already M3-accurate.
    `group_interior_state` — verified B's projection + comment in place. Retired
    transitional channel: grep of techy/src + techy/tests for
    `absent_overrides`/`AbsentFeatureOverrideError`/"hash as nothing only at
    M3"/"at M3" promises is CLEAN (B got them all). No direct `.scopes.<field>`
    paths outside scopes/mod.rs (all access via stack methods; `StateData.scopes`
    stays a plain `ScopeStack<L>` field, correct). Fixed four M2-era "whatever the
    rules data says" doc phrasings that now describe unrepresentable data:
    `skip_whitespace` doc, `StdTokenReader` struct doc, `read_comment` doc
    (reader.rs), `PrefixTable::for_rules` doc — reworded to "no rules data exists
    for it" (state_memo.rs's since-M2/since-M3 history comment is present-tense
    accurate and kept).
  - **tests/lang_features.rs**: `feature_composition` gained the positive
    `ScopeStack::push` compile fact (`push_onto_scope_stack<L: LangHasScopes>`,
    instantiated with the all-present language). Two new runtime pins in
    `commands_without_scopes`:
    `the_scope_stack_of_a_scopes_absent_language_is_permanently_empty`
    (`scopes()` answers `is_empty()`, `providers().is_empty()`, `len() == 0`) and
    `apply_op_on_a_scopes_absent_stack_reports_scopes_absent_without_panicking`
    (fresh stack + `ScopeOp::Push` → `Err(ScopeOpError::ScopesAbsent)`, stack still
    empty). Module doc extended by one sentence (the stack itself is storage-gated).
    Existing assertion values untouched; support module needed no construction
    changes (`ScopeStack::new()` stayed unbounded).
  - **Expected-breaking additions for the M3 gate run**: `ScopeStack::push` and
    `ParsingState::lang_initial_with_packages` bounded `LangHasScopes`;
    `ScopeOpError::ScopesAbsent` is additive (`#[non_exhaustive]`).
  - **Gates**: `cargo build` clean; `cargo test --workspace` **897 passed / 0
    failed / 4 ignored** (per-target 758+30+8+12+21+1+67; 2+2 ignored — task B's 895
    plus exactly the 2 new runtime pins); `cargo check --workspace --tests` zero
    warnings on a forced rebuild; fresh `rm -rf target/doc && cargo docs` zero
    warnings, the `ScopeOpError` page carries the new variant.
  - **Surprises**: only the `lang_initial_with_packages` forced bound (above);
    the store projections, the transparent `push` body, and the borrow-driven
    helper re-shaping all behaved as B's delta.rs patterns predicted.

- **Supervisor: M3d — static size regression tests** — new `mod storage_collapse`
  in techy/tests/lang_features.rs, all `const` asserts (compile-time; no runtime
  test added — totals stay 897/0/4). Collapse checks (platform-independent):
  `TokenRules`/`TokenRulesOverrides`/`ScopeStack`/`StateData`/`ParsingState` of
  `PlainCharsLang` (NoLangFeatures) are all **0 bytes**; `ParsingStateDelta` is
  **32** (64-bit) — only the ungated `mode`/`ext`/`events` remain. Transparency
  checks (`#[cfg(target_pointer_width = "64")]`): all-present sizes pinned to the
  recorded M2 numbers. **The two number sets** (bytes, 64-bit; M2 measured at
  351c95b, M3 at 81726d7 — all-present column identical before/after, the
  transparency requirement):

  | type | M2 all-present | M2 NoLangFeatures | M3 all-present | M3 NoLangFeatures |
  |------|------|------|------|------|
  | TokenRules | 176 | 176 | 176 | 0 |
  | TokenRulesOverrides | 184 | 184 | 184 | 0 |
  | ParsingStateDelta | 240 | 240 | 240 | 32 |
  | StateData | 200 | 200 | 200 | 0 |
  | ParsingState | 232 | 232 | 232 | 0 |
  | ScopeStack | 24 | 24 | 24 | 0 |

  Gates: `cargo test --workspace` 897/0/4 (per-target 758+30+8+12+21+1+67; 2+2
  ignored); `cargo check --workspace --tests` zero warnings. Nothing surprising:
  the collapse landed exactly at the predicted values on first compile.

- **M3 reviewer** — full-diff review of 351c95b..d3da11b (M3a f6cb54e, M3b fc33dee,
  M3c 81726d7, M3d d3da11b) against PLAN M3, the M3-prep mechanics decisions,
  [§dd-dr:lang-features] as amended, [§dd-dr:panic-policy] rule 3,
  [§dd-dr:superseded-names], [§dd-arch:naming], and the 2026-08-10 ruling's M3
  instruction. Findings: **0 blocker, 0 should-fix, 3 nit**. Gates re-run
  independently, all green.
  - nit (techy/src/token/rules.rs:300–323): the "Constructing rules for a language
    with absent features" doctest's example language is a `TrivialLang`
    (`AllLangFeatures`), so the compiled example never exercises an absent feature —
    the recipe is correct and identical either way, but the heading promises more
    than the doctest checks; one comment line in the example noting this (or a
    genuinely partial doctest language) would close the gap.
  - nit (techy/tests/lang_features.rs:463): test name
    `braces_parse_as_group_nodes_while_other_rules_data_is_inert` still describes
    the M2 populated seed — since M3a there is no "other rules data" to be inert
    (the body comment is already accurate); rename in the fix pass (e.g.
    `…_while_other_constructs_read_as_plain_content`).
  - nit (techy/tests/lang_features.rs:425,497,587): three disable_all test comments
    still say absent features' blocks "stay all-`None`" — since M3 those fields are
    zero-sized stores, not all-`None` blocks (delta.rs's own rustdoc says it right).
  - Checklist-area verdicts, one line each:
    - **Projections**: all 7+7 rules/override blocks, `scope_ops`, `ScopeStack`'s
      inner Vec, and both derived caches are fully spelled
      `<… as FeaturePresence>::Store<…>`; no public alias anywhere.
    - **Accessors**: the 13 keep exact signatures with the spec'd neutral answers;
      `scopes()` unchanged; `prefix_table()`/`trigger_chars()` return `Option` with
      rustdoc saying exactly `None`-iff-absent / disabled-but-present →
      `Some` of the frozen empty value.
    - **Transparency**: latexlike untouched except the trait pin (driver.rs not in
      the diff); concrete languages still write plain literals; generic test
      helpers gained `Features = AllLangFeatures` equality bounds (type-level
      only, no semantic change).
    - **Error-channel retirement**: grep-clean (the one dev-docs mention is the
      dated removal-record amendment); `apply` infallible; `DeriveError` back to
      the two-source invariant with Display/Debug/docs consistent;
      `apply_overrides` → `Vec<ScopeOpError>`; unreachability independently
      confirmed — fields are store projections, every constructor routes
      `store_with` (`default`/`disable_all`/`new`/`Default`-for-delta), merges use
      matched projections (`merge_from` ×2, `lower_state_events`), builders
      bounded, no serde; a free `ScopeOp` stays constructible but nothing can hold
      or apply one (`apply_op` answers `ScopesAbsent`). No blocker.
    - **Lattice bounds**: exactly the PLAN list + private `probe_minted_group` +
      the forced `lang_initial_with_packages`; no over-bounding
      (`CharsGroupArgumentParser::contents_delta` and `group_interior_state`
      project instead), no under-bounding (the only temporaries-minting site is the
      bounded probe; environment_parser.rs:1109's expecting_close literal is
      `#[cfg(test)]` on a concrete language); `LangHasParagraphs: LangHasWhitespace`
      untouched (features.rs:282).
    - **state_memo**: hash_key/keys_eq in lockstep under the same `store_get`
      projections, original field order, expecting_close last in both (commented);
      Arc-identity keying byte-identical for present features; the memoizable guard
      projects `scope_ops`; `stores_eq`'s impossible mixed arm answering `false` is
      sound (same-language keys share the marker; a miss, never a false hit).
    - **disable_all**: `store_with(X::disable)` is value-identical to M2d's if/else
      for every language (present → `disable()`; absent → nothing exists, matching
      the old all-`None`-carries-nothing); the delta.rs unit test is untouched;
      rustdoc carries no stale error mention.
    - **Size tests**: const asserts match the recorded table (collapse 0×5 + delta
      32; all-present 176/184/240/200/232/24 under the 64-bit cfg); comments
      accurate.
    - **Panic policy**: zero new panic/unwrap/expect/unreachable in lib code (all
      new `unwrap`/`expect` are `#[cfg(test)]`); projections are if-let/match
      throughout — no unwrap-because-PRESENT anywhere.
    - **Naming**: the four `store_*` fns match the recorded prep decision and the
      crate's non-panicking `get`/`get_mut` convention; `ScopesAbsent` uses the
      absent vocabulary; no superseded name and no "facet" anywhere in the diff.
    - **Docs-clarity**: no dd-dr labels in public rustdoc; the TokenRules
      three-spellings narrative now includes storage; the DR M3 amendment is dated,
      entry-styled, accurate; the `Send + Sync` GAT bound is documented on `Store`
      with the old "deliberately no Send/Sync" paragraph fully replaced.
    - **Behavior identity**: every src-diff assert change is access-syntax only
      (`.unwrap()`/`.expect` on the new Option accessors, type annotations, two
      turbofishes); recompose_oracle.rs untouched; lang_features.rs is 15 → 12
      `#[test]` + 1 doctest — the five M3b retirements are each the ruled
      unrepresentability with replacements accounted (compile facts in
      `feature_composition`, the reworked apply-cleanly test with value "Z" kept),
      and the surviving flipped assertions (`whitespace_chars()` "",
      `!commands_enabled()`, `!comments_enabled()`) each read now-unrepresentable
      absent-feature seed data — forced, with all parse-outcome assertions
      value-identical.
    - **Public surface**: exactly the four `FeaturePresence` fns + GAT bound
      change, the gated field types, the Option accessors, the item-5 bounds,
      `ScopesAbsent`, and the three removals — a grep of pub-item changes shows
      nothing else; one canonical path preserved (the core facade only dropped the
      removed error's re-export).
  - Judgment calls, all endorsed: **Send+Sync GAT** (forced —
    `CallableSpec: Send + Sync` holds deltas through `ArgumentSpec`; the promise is
    free on both markers and compile-pinned on all 18 roster payloads);
    **`ScopeOpError::ScopesAbsent`** (`apply_op` must stay unbounded for the
    generic delta path; silent no-op and panic both ruled out; additive under the
    verified `#[non_exhaustive]`); **Option cache accessors** (the recorded prep
    decision, rustdoc exact); **scope_ops store-gating** (the field is `pub` —
    method bounds alone would leave literal construction as a violating path);
    **LatexlikeLang pin** (the ruled family pin made load-bearing at the trait,
    forced by generic-`LLL` literal transparency; both impls comply);
    **lang_initial_with_packages bound** (a scope-mutating entry point per PLAN's
    bound line — the documented-infallible contract bars an error channel and
    loud-failure bars a silent package drop).
  - **Gates** (re-run): `cargo build` clean; `cargo test --workspace` **897 passed
    / 0 failed / 4 ignored** (per-target 758+30+8+12+21+1+67; 2+2 ignored);
    `cargo check --workspace --tests` zero warnings on a forced rebuild; fresh
    `rm -rf target/doc && cargo docs` zero warnings — rendered-HTML spot-check: the
    TokenRules page shows the fully spelled gated field types, the FeaturePresence
    page carries the four projection fns, the ScopeOpError page carries
    `ScopesAbsent`, and no `AbsentFeatureOverrideError` remains anywhere under
    target/doc. (`check_semver.sh` deferred to closure per the M3b note; the
    M3b/M3c expected-breaking additions are recorded above.)

- **Supervisor: M3 fix pass + closure gates** — the M3 reviewer found 0 blocker /
  0 should-fix / 3 nit; all three applied directly (small mechanical edits): the
  rules.rs construction-recipe doctest now carries a hidden comment noting its
  example language is all-present (the recipe itself is presence-generic); the
  groups_only test renamed `braces_parse_as_group_nodes_while_other_constructs_read_as_plain_content`
  (old name described the retired populated seed); three disable_all test comments
  reworded off "stay all-`None`" (absent fields are zero-sized stores). Final
  gates, all green AFTER the fixes: `cargo build` clean; `cargo test --workspace`
  **897 passed / 0 failed / 4 ignored** (758+30+8+12+21+1+67; 2+2 ignored — the
  884 pre-M2 baseline intact; lang_features.rs at 12 tests + 1 doctest after the
  ruled M3 retirements, all accounted in the M3a/M3b logs); `cargo check
  --workspace --tests` zero warnings; fresh `rm -rf target/doc && cargo docs` zero
  warnings; `scripts/check_semver.sh` — see the M3 expected-breaking list below
  (one-time note: a sandboxed `cargo fetch` failure required one unsandboxed
  fetch, same as the M1 environment note). The open Comment-arm question was left
  untouched per the standing instruction (no new ruling appeared in this file
  during M3).

### M3 expected-breaking list (vs `api-baseline`; baseline NOT moved)

`check_semver.sh` after M3: 196 checks, 194 pass, 2 fail — the SAME two
categories as M1, and one M2 row retired:
1. `constructible_struct_adds_field`: exactly M1's five rows again
   (`WhitespaceRules.enabled`, `TokenRules.paragraphs`, `TokenRules.specials`,
   `TokenRulesOverrides.paragraphs`, `TokenRulesOverrides.specials`). The M2 row
   `DeriveError.absent_overrides` is GONE — the field was removed at M3 with the
   transitional error channel (net zero against the baseline, per the 2026-08-10
   ruling's M3 instruction).
2. `struct_pub_field_missing`: unchanged from M1 (the 8 old `TokenRules` fields +
   the mirrored 8 on `TokenRulesOverrides`).

Breaking but NOT surfaced by the tool (M3 additions):
- `TokenRules`/`TokenRulesOverrides` public field TYPES changed again: every
  feature block is now the fully spelled
  `<<L::Features as LangFeatures>::X as FeaturePresence>::Store<…>` projection
  (transparent — identical literals/reads — for all-present languages);
- `ParsingStateDelta.scope_ops` field type is the Scopes-gated store;
  `ParsingStateDelta::{scope_op, push_provider}` gained `where L: LangHasScopes`;
- `ScopeStack::push` and `ParsingState::lang_initial_with_packages` gained
  `where L: LangHasScopes`;
- `ParsingState::prefix_table()` / `trigger_chars()` now return `Option<&…>`
  (`None` iff the feature is absent);
- `verbatim_state_delta`, `VerbatimArgumentParser`, `VerbatimBodyParser`,
  `GroupArgumentParser`, `OptionalGroupArgumentParser` gained `L: LangHasGroups`;
- `LatexlikeLang` supertrait now spells the ruled family pin
  (`Lang<Features = AllLangFeatures>`).

Reverted M2 breaks (the surface now matches the baseline again):
`TokenRulesOverrides::apply` is infallible; `DeriveError.absent_overrides`
removed; `AbsentFeatureOverrideError` removed entirely (was additive at M2).
Post-baseline API reshaped (new at M2, so not baseline-relevant):
`FeaturePresence::Store` payload bound is now `Clone + Debug + Send + Sync`
(`Default` dropped); the trait gained the four projection fns. Additive:
`ScopeOpError::ScopesAbsent` (enum is `#[non_exhaustive]`). Carried unchanged
from M1/M2: `Default` removed from `WhitespaceRules`; `Lang` requires
`type Features` (the M2 headline). Nothing in the semver output is unexplained.

## Questions for user

(genuine design ambiguities; the most conservative spec-consistent option was chosen and is noted here)

- **`ScopeOpError` gained the unit variant `ScopesAbsent` (M3c).**
  `ScopeStack::apply_op` must stay unbounded — the generic delta-application path
  (delta.rs `apply_overrides`) calls it under a projection over bare `L: Lang`,
  where a `LangHasScopes` bound cannot be proven — so when its store projection
  answers `None` (the language declares the scope stack absent) it needs a runtime
  answer. A silent no-op is ruled out by [§dd-dr:lang-features] (loud failure for
  authored absent-feature data); a panic is ruled out by [§dd-dr:panic-policy]
  rule 3. The conservative option taken: a new error variant — additive, the enum
  is `#[non_exhaustive]` — reachable only by calling `apply_op` directly on such a
  stack (a delta can never carry scope ops for one; the scope-op list is
  storage-gated). Display: "the language declares the scope stack absent; no scope
  operation can apply". Flagged for review; pinned by
  `apply_op_on_a_scopes_absent_stack_reports_scopes_absent_without_panicking`.

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

### M3 → M4 hand-off (supervisor, M3 closure)

- Stages D, M1, M2, M3 are DONE. M3 lives on branch `lang-features-m3` (worktree
  agent-abcf5ade530ed3cd0), branched off `lang-features` at 351c95b — it
  fast-forwards cleanly into `lang-features` (no merges/pushes were made, per
  rules; the main session integrates). The M4 successor should branch off the
  integrated chain in a FRESH worktree.
- M4 scope: PLAN.md "Stage M4" — language-author guide section on declaring
  features (docs-clarity rules), rustdoc coherence sweep, final gate run with
  rendered-HTML link verification, superseded-names grep, delete
  dev-docs/langfeatures-plan/ (retained in git history).
- Guide-section material the M4 author should cover (all now real): declaring
  `Lang::Features` (bundle choice or a custom unit struct), the three spellings
  of off, the transparent-store guarantee (all-present code writes plain
  literals), the `..TokenRules::empty()` construction recipe for partial
  languages, the `LangHas*` bounds a language author may hit
  (verbatim/group-argument parsers, scope mutation), the `Option`-returning
  cache accessors, and the storage-collapse numbers (PROGRESS M3d table).
- M4-sweep candidates, carried + new: dev-docs/ARCHITECTURE.md:697 pre-existing
  stale line ("`disable_all` and the collection constructors remain pending
  their stage" — contradicts [§dd-dr:takeover-staging-sugar] applied notes);
  promote [§dd-arch:state]'s reference to [§dd-dr:lang-features] from the
  decisions list into body prose (Stage-D judgment call — the code now exists);
  ARCHITECTURE's state/token topic prose still describes ungated storage where
  it describes `TokenRules` fields at all (verify; M3 touched only
  DESIGN_RATIONALE).
- Open user questions (see "Questions for user"): the optional nodes_parser
  Comment-arm guard (pending main-session ruling, untouched at M3 per
  instruction); the `ScopeOpError::ScopesAbsent` variant (M3c conservative
  choice, flagged for review).
- Test totals after M3: **897 passed / 0 failed / 4 ignored**
  (884 pre-M2 baseline + 12 lang_features tests + 1 rules.rs doctest; the M2c/M2d
  transitional pins retired at M3 are accounted test-by-test in the M3a/M3b log
  entries). Storage numbers: the M3d table above (all-present unchanged from M2;
  NoLangFeatures collapses to 0 except the delta's ungated 32).
- Environment note (unchanged): one-time unsandboxed `cargo fetch` may be needed
  after dependency updates; everything else runs sandboxed.

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
