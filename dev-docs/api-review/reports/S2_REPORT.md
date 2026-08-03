# Phase 3 — S2 report: engine init + resolver strategy

Branch `phase3-s2-engine-init` (worktree, branched from `api-review` @ 8d03608).
Four commits: 6a067f5 (named constructors), 0d9047e (resolution extraction),
09b153c (language init + driver reshape + sealed conversions), f03adc5 (docs
churn). All gates green.

## What was done (by work-order item)

1. **Language init** ([§dd-dr:language-init], T1T2 §C1, T3 §A+F):
   `Language::new(driver, initial_state)` with the initial state a mandatory
   by-value `ParsingState<L>` (Arc'd internally). `ParsingState::initial()` →
   `lang_initial()`; NEW infallible `ParsingState::lang_initial_with_packages(
   impl IntoIterator<Item: IntoSpecsProvider<L>>)`. REMOVED: `Language::
   with_provider`, `with_seed_delta`, `with_resolver`, `resolver()`,
   `Language::resolve_source`, `impl Default for Language<L>`,
   `LatexlikeDriver::default()`, `StdParseDriver::default()`. `Language` is now
   constructor + `parse` + `parse_source` + `initial_state()`/`driver()`
   accessors (the pre-existing `parse_source(Arc<Source>)` was KEPT — it is part
   of the T4-amended ruled surface; nothing new was added for S6). The delta
   idiom is `ParsingState::lang_initial().derived(&delta)?` — demonstrated in
   the reshaped `Language` tests and the guide.

2. **Sealed conversions** (T1T2 §C2/E1a): `IntoSpecsProvider<L>` in
   `core::specs` — impls exactly as ruled: `Package<L>` by value, `Arc<P>`,
   `Arc<dyn SpecsProvider<L>>` (no marker needed; compiles as ruled). Spec-side
   sibling `IntoCallableSpec<L, M>` (name derived from the delegated
   "trait-name family with the `IntoSpecsProvider` precedent" clause,
   T3_BRIEF:529) on `Package::insert`/`insert_in_modes`/`insert_specials`/
   `insert_specials_in_modes`, with the ruled parameter-order flip:
   `insert_specials(callable_type, trigger, spec)` (and the `_in_modes` twin
   aligned). The infallibility argument is preserved and documented verbatim on
   `lang_initial_with_packages` ("Why this cannot fail": the seed never runs
   `finalize_transition`; direct pushes involve no by-name scope ops; the
   transition choke point untouched). See **Deviations D1/D2** for the
   coherence-forced mechanism details.

3. **Resolution extraction** (T3 §H, [§dd-dr:resolution-extraction]): free
   `pub fn resolve_command_in_scopes<L: Lang>(state, token, callable_type) ->
   CommandResolution<L>` public at `techy::core::specs` (internally in
   engine/driver.rs beside `CommandResolution`, matching the S1 precedent that
   internal layout is organizational only); `CommandResolution::
   resolve_via_scopes` removed. No did-you-mean detail added (S9).
   `LatexlikeDriver` and the nodes_parser test langs delegate to the free fn.

4. **Driver reshape** ([§dd-dr:command-resolver], TIERC R4, T4 §B1):
   - `trait CommandResolver<L: Lang>: Debug + Send + Sync` in the hub
     (`techy::core`), one method mirroring `ParseDriver::resolve_command`;
     `impl CommandResolver<L> for ()` resolves nothing and carries the
     not-implemented detail message **verbatim** (it is pinned by an existing
     test); `ParseDriver::resolve_command`'s trait default now delegates to
     `()` — one source of truth for the message.
   - `StdParseDriver<R = (), O: SourceOrigin = Option<String>>` with
     `new(recovery, command_resolver)` (resolver mandatory, by value, no
     `Default`/`Clone` bounds on the constructor), fields
     `recovery`/`command_resolver`/`source_resolver` all `pub` (per R4
     "fields stay pub"), chainable `with_source_resolver(…)`;
     `impl<L, R: CommandResolver<L>> ParseDriver<L> for StdParseDriver<R,
     L::SourceOrigin>`. Test spelling `StdParseDriver::new(Recovery::Strict,
     ())` used throughout. See **Deviation D3** for the second type parameter.
   - `ScopesCommandResolver<L> { pub command_type: L::CallableTypeId }` in
     `core::specs`, one-line delegation to `resolve_command_in_scopes`
     (constructed by braced literal — no constructor was ruled).
   - `ParseDriver::source_resolver(&self) -> Option<&dyn
     SourceResolver<L::SourceOrigin>>` defaulted accessor (default `None`,
     "this language resolves nothing"); `StdParseDriver` and `LatexlikeDriver`
     gained the `Option<Arc<dyn SourceResolver<…>>>` field +
     `with_source_resolver` builder with the sealed conversion (by-value →
     internal Arc; `Arc<R>`/`Arc<dyn>` pass through, pointer-identity
     asserted in tests). `LatexlikeDriver`'s field is **private** per the T5
     amendment on [§dd-dr:preset-driver-pillars] ("resolver field private …
     the two policy knobs stay `pub`"); `StdParseDriver`'s is `pub` per R4.
   - The ruled ASYMMETRY is documented in `StdParseDriver`'s rustdoc (own
     section) AND as a code comment at the field pair; the accessor's rustdoc
     and `LatexlikeDriver`'s field doc cross-reference it.
   - Driver `Copy`/`Eq`/`PartialEq` dropped on both shipped drivers (`Clone +
     Debug` kept; `Debug` manual — the `dyn` field is shown by presence only).
     The mooted Copy/Eq comment (latexlike/driver.rs, minted paragraph-break
     spec) rewritten; minting behavior unchanged.
   - `StdParseDriver` doc carries the resolver-choice guidance sentence pairing
     `()` with `TrivialLang`-style test use and `ScopesCommandResolver` with
     command-bearing languages.
   - `ScopesResolvingDriver` does not exist anywhere (grep-verified).

5. **`NoResolver` deleted** (TIERC R1): type, impl, test, and every doc mention
   gone; register entry already existed (Tier-C block) — no duplicate added.

6. **Named constructors** (T3 §B2 + wish 21): `TokenRules::empty()` +
   `StateData::empty()` (all-empty values exactly as ruled; deliberately not
   `Default`; the default `initial_state_data` body is now `StateData::empty()`
   — one source of truth, pinned by the existing neutrality test);
   `TokenRulesOverrides::disable_all()` (all six gates `Some(false)`;
   `verbatim_state_delta` re-expressed as `disable_all()` + terminator). New
   tests: `disable_all` shape, `lang_initial_with_packages` order/infallibility
   + all three conversion shapes, `ScopesCommandResolver` hit/miss,
   `StdParseDriver`/`LatexlikeDriver` source-resolver plumbing incl.
   no-double-wrap pointer identity.

7. **Docs churn**: README quick-start rewritten to the ruled idiom and
   **compile-verified against the built rlib** (compiled + ran, assertion
   passes). The `engine/driver.rs` doctest updated. docs/learn-by-example.md:
   every `Language::default()`/`with_provider` block rewritten to
   `Language::new(LatexlikeDriver::new(Recovery::…), ParsingState::
   lang_initial[_with_packages](…))`; spec registrations showcase the by-value
   `insert` (Arc::new dropped); `insert_specials` args flipped. ARCHITECTURE.md:
   three passages updated (Language bundle, seed airtightness, pluggable
   resolution). DESIGN_RATIONALE: applied/partially-applied status notes on
   [§dd-dr:language-init] (applied), [§dd-dr:with-provider] (supersession
   confirmed, removal applied), [§dd-dr:command-resolver] (applied, with the
   type-parameter application detail), [§dd-dr:resolution-extraction] (applied;
   did-you-mean deferred to S9 noted), [§dd-dr:input-wiring] (**partially**
   applied — driver side only; door/bundle/conditions/preset spec = S6),
   [§dd-dr:registration-ergonomics] (ruling 1 applied + coherence-marker
   mechanism note; rulings 2–3 ride S9), [§dd-dr:takeover-staging-sugar]
   (item 1 applied), [§dd-dr:on-ramp-defaults] (ruling 1 applied),
   [§dd-dr:source-resolver] (NoResolver deletion applied + the forced
   `Arc<R>`-forwarding-impl removal recorded), [§dd-dr:preset-driver-pillars]
   (Copy/Eq strike + private-resolver-field applied to the monomorphic driver).
   Superseded-names register: ONE new block (language-init application) adding
   `with_provider`/`with_seed_delta` and `Default for Language` /
   `LatexlikeDriver::default` / `StdParseDriver::default`;
   `ParsingState::initial`, `ScopesResolvingDriver`, `NoResolver`,
   `NoCommandResolver`, `with_resolver` were already covered (no duplicates).

## Signature table (old → new, public surface)

| Old | New |
|---|---|
| `Language::new(driver: L::Driver)` | `Language::new(driver: L::Driver, initial_state: ParsingState<L>)` |
| `Language::with_seed_delta(delta) -> Result<Self, DeriveError<L>>` | REMOVED (idiom: `ParsingState::lang_initial().derived(&delta)?` before construction) |
| `Language::with_provider(Arc<dyn SpecsProvider<L>>) -> Result<…>` | REMOVED (idiom: `lang_initial_with_packages([pkg])`) |
| `Language::with_resolver(impl SourceResolver + 'static)` | REMOVED (driver `with_source_resolver`) |
| `Language::resolver(&self) -> &Arc<dyn SourceResolver<…>>` | REMOVED (`ParseDriver::source_resolver`) |
| `Language::resolve_source(&self, reference, triggered_at) -> Result<Arc<Source<…>>, ResolveError>` | REMOVED (compose accessor + free `resolve_source_reference`) |
| `impl Default for Language<L> where L::Driver: Default` | REMOVED |
| `ParsingState::initial()` | `ParsingState::lang_initial()` |
| — | `ParsingState::lang_initial_with_packages(impl IntoIterator<Item: IntoSpecsProvider<L>>) -> ParsingState<L>` (infallible) |
| — | `pub trait IntoSpecsProvider<L>` (sealed; `core::specs`) |
| — | `pub trait IntoCallableSpec<L, M>` (sealed; `core::specs`) |
| — | `pub trait IntoSourceResolver<O, M>` (sealed; `techy::source`) |
| `Package::insert(ct, name, spec: Arc<dyn CallableSpec<L>>)` | `Package::insert<M>(ct, name, spec: impl IntoCallableSpec<L, M>)` |
| `Package::insert_in_modes(ct, name, Arc<dyn …>, modes)` | `Package::insert_in_modes<M>(ct, name, impl IntoCallableSpec<L, M>, modes)` |
| `Package::insert_specials(trigger, ct, Arc<dyn …>)` | `Package::insert_specials<M>(ct, trigger, impl IntoCallableSpec<L, M>)` — order flipped |
| `Package::insert_specials_in_modes(trigger, ct, Arc<dyn …>, modes)` | `Package::insert_specials_in_modes<M>(ct, trigger, impl IntoCallableSpec<L, M>, modes)` |
| `CommandResolution::resolve_via_scopes(state, token, ct)` | free `resolve_command_in_scopes(state, token, ct)` at `core::specs` |
| — | `pub trait CommandResolver<L>: Debug + Send + Sync` (hub) + `impl for ()` |
| — | `pub struct ScopesCommandResolver<L> { pub command_type }` (`core::specs`) |
| `struct StdParseDriver { pub recovery }` (`Debug, Clone, Copy, PartialEq, Eq`, `Default`) | `struct StdParseDriver<R = (), O: SourceOrigin = Option<String>> { pub recovery, pub command_resolver, pub source_resolver }` (`Clone where R: Clone`, manual `Debug`) |
| `StdParseDriver::new(recovery)` | `StdParseDriver::new(recovery, command_resolver)` |
| `StdParseDriver::default()` | REMOVED |
| — | `StdParseDriver::with_source_resolver<M>(impl IntoSourceResolver<O, M>)` |
| `LatexlikeDriver` (`Debug, Clone, Copy, PartialEq, Eq`, `Default`) | `LatexlikeDriver` (`Clone`, manual `Debug`; + private `source_resolver` field, `with_source_resolver<M>`) |
| `LatexlikeDriver::default()` | REMOVED |
| — | `ParseDriver::source_resolver(&self) -> Option<&dyn SourceResolver<L::SourceOrigin>>` (defaulted, `None`) |
| `ParseDriver::resolve_command` default (inline message) | default delegates to `()`'s `CommandResolver` impl (same message, one source of truth) |
| `impl<O, R: SourceResolver<O> + ?Sized> SourceResolver<O> for Arc<R>` | REMOVED (see D2; `&R`/`Box<R>` forwarding impls stay) |
| `struct NoResolver` + its `SourceResolver` impl | DELETED |
| — | `TokenRules::empty()`, `StateData::empty()`, `TokenRulesOverrides::disable_all()` |

## Gate results

- `cargo build`: clean, 0 warnings.
- `cargo test`: 538 lib + 30 + 8 + 1 + 26 doctests + acceptance — all pass,
  0 failed. No behavioral expectation changed except init-idiom call sites and
  two test renames (`the_default_driver_is_strict` →
  `the_recovery_knob_is_explicit`; `default_language_uses_the_default_driver`
  removed with the `Default` impls; resolver round-trip test rewritten over the
  driver accessor).
- `rm -rf target/doc && cargo docs`: clean, 0 warnings/errors.
- Grep sweeps over src/tests/docs/README: zero hits for `Language::default`,
  `StdParseDriver::default`, `with_provider`, `with_seed_delta`,
  `resolve_via_scopes`, `NoResolver`, `ScopesResolvingDriver`,
  `ParsingState::initial(`.
- README snippet: extracted, compiled against `target/debug/libtechy.rlib`,
  executed — assertion passes.

## Call-site churn

34 files, +1362/−585. ~30 `Language` construction sites (src tests, acceptance
suite, guide, doc examples) moved to the ruled idioms; 10 `insert_specials`
sites flipped; ~14 `ParsingState::initial()` sites renamed; 12
`StdParseDriver::new`/`default` sites re-arified; acceptance harness gained
`with_recovery_plus`, latexlike test_support gained `with_package` (thin
test-only wiring replacing the removed builder chains).

## Deviations / ambiguities (each was forced or explicitly delegated; flagged for review)

- **D1 — inference-marker parameter on two sealed traits**
  (spec/callable.rs `IntoCallableSpec<L, M>`; source/resolver.rs
  `IntoSourceResolver<O, M>`). The ruled trio (blanket by-value impl + `Arc<S>`
  + `Arc<dyn>` pass-through) is rejected by Rust coherence on Lang-/origin-
  generic traits (E0119: downstream `impl CallableSpec<TheirL> for Arc<…>` is
  orphan-legal, so the blanket and the Arc impls overlap; verified by compiler
  and minimized in a scratch crate). Realization: a sealed, never-named marker
  type parameter (`ByValue`/`SharedConcrete`/`SharedDyn` in the private sealed
  modules) disambiguates the impls — every ruled call shape works, sealing
  holds, no double-wrap, zero runtime cost. Visible cost: a `<M>` type
  parameter on `insert*`/`with_source_resolver` signatures. `IntoSpecsProvider`
  needs no marker (its by-value impl is the concrete `Package<L>`, exactly as
  ruled). Recorded on [§dd-dr:registration-ergonomics].
- **D2 — `SourceResolver`'s `Arc<R>` forwarding impl removed**
  (source/resolver.rs). With it present, `Arc<R>: SourceResolver` holds, which
  makes the ruled no-double-wrap `Arc` pass-through impls of
  `IntoSourceResolver` incoherent AND would make every `Arc` argument ambiguous
  under the marker scheme. The impl was unruled convenience plumbing; `&R` and
  `Box<R>` forwarding stay; the one in-crate consumer was rewritten (`&*arc`).
  Recorded on [§dd-dr:source-resolver].
- **D3 — `StdParseDriver` grew a second defaulted parameter**
  `O: SourceOrigin = Option<String>` (engine/driver.rs). The entry's spelling
  `StdParseDriver<R = ()>` cannot hold the ruled `Option<Arc<dyn
  SourceResolver<…>>>` field for arbitrary `L::SourceOrigin`; the work order's
  alternative `StdParseDriver<L, R = ()>` would break the ruled annotation-free
  `type Driver = StdParseDriver` (decisive reason 3 of
  [§dd-dr:command-resolver]). The defaulted-`O` shape preserves both;
  `impl … ParseDriver<L> for StdParseDriver<R, L::SourceOrigin>`. Consequence:
  a *standalone* binding needs `let d: StdParseDriver = …` (alias defaults do
  not drive expression inference); the ruled spelling is annotation-free in
  every `type Driver =`/`Language::new` position. Recorded on
  [§dd-dr:command-resolver].
- **D4 — `IntoCallableSpec` name**: not literally ruled; chosen per the
  explicitly delegated clause ("decide the trait-name family with C2's
  `IntoSpecsProvider` precedent at application", T3_BRIEF:529) and the E1a
  "sibling trait" wording.
- **D5 — `LatexlikeDriver.source_resolver` private** while `StdParseDriver`'s
  is `pub`: TIERC R4 rules StdParseDriver "fields stay pub"; the T5 amendment
  on [§dd-dr:preset-driver-pillars] rules the preset driver's "resolver field
  private behind `with_resolver`/`source_resolver()`, the two policy knobs stay
  `pub`". Both applied as written; the asymmetry is the records'.
- **D6 — `Language::parse_source` kept**: the work order says a `parse_source`
  entry point "arrives in stage S6, do NOT add it", but the method already
  existed and is part of the T4-amended ruled surface ("new + parse +
  parse_source + accessors"). Nothing added; the existing method kept
  unchanged. If S6 intends a different `parse_source`, it lands there.
- **Not done, deliberately**: `Scope::insert` (and `FallbackProvider::set`)
  still take `Arc<dyn CallableSpec<L>>` — E1a names only
  `Package::insert`/`insert_specials`/`…_in_modes`. Flagged as a rider (below).

## Riders noticed for later stages

- **S4/S9**: `Scope::insert` / `FallbackProvider::set` are now the only
  Arc-demanding registration doors — a candidate for the same
  `IntoCallableSpec` treatment when a session rules it.
- **S9**: `define_macro`/`define_environment` one-liners (E1b) will further
  shorten the guide's registration blocks (already Arc-free after this stage);
  the did-you-mean miss detail slots into `resolve_command_in_scopes`'s miss
  arm.
- **S6**: `resolve_source_reference`'s doc references the driver accessor
  composition; the door/bundle/conditions complete the wiring. The `MapResolver`
  suggestion for deterministic-failure tests (Tier-C `NoResolver` rationale) is
  what the reshaped tests use.
- **S4**: `latexlike/mod.rs` still carries the `### PhF` review notes on
  `base_package` (rename to `_builtin`, specials relocation — T1T2 B rider);
  untouched here.
- **Observation**: `TrivialLang`'s doc sentence "the default driver resolves
  nothing" now has a precise mechanism (`R = ()`); wording already matches.
