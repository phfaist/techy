# Claude Conversation Export

- Session: `0d0647c0-ad0b-47ba-8cff-cf5d11ebb6a4`
- Project: `/Users/philippe/projects/techy/-claude/worktrees/subphase-7-5-latexlike`
- Exported: 2026-07-19T01:06:30

---

**User:**

I'm interested in having fully gated language features (specials, callables, groups, temporary groups ...) with corresponding memory saving in ParsingState/TokenRules fields for users that don't need them (e.g. for a simple web app that cares about web payload size).  If they don't need libraries/scopes, have the compiler omit them and don't include the relevant fields in the parsing state object.  I want you to perform an analysis of how this could be possible and to suggest solutions.  I do want to keep the code clean and avoid shipping multiple libraries; I'm thinking the gating should be via generics in Lang.  Suggest solutions for review.

---

**Assistant:**

I'll analyze the codebase first — Lang, ParsingState, TokenRules, the scope stack, and the new Latexlike preset — then write up gating strategies via generics for your review. No code changes, so I'll work in place.

Reading the core files now — `Lang`, `ParsingState`, `TokenRules`, the scope stack, and `Language`.

Now reading `TokenRules`, the reader hot path, and skimming the scopes module and Latexlike preset.

Let me check the DESIGN_RATIONALE on the enable-gates decision and skim the latexlike preset's `Lang` impl, then measure actual struct sizes.

Now let me measure the actual per-state memory footprint with a scratch crate (path-dependency on the worktree's techy crate, built in the job's tmp dir).

The job tmp dir isn't in the sandbox write allowlist; using `$TMPDIR` instead.

You've hit your session limit · resets 11:30pm (Europe/Athens)

---

**User:**

continue

---

**Assistant:**

Analysis complete. No code was changed — measurements ran in a scratch crate under `$TMPDIR` with a path dependency on this worktree's `techy`.

# Feature gating via `Lang` generics — analysis and proposed solutions

## What I measured first (grounding numbers)

**Per-state memory today** (arm64; wasm32 is roughly half, since it's mostly pointers):

| Type | Size |
|---|---|
| `ParsingState<MiniLang>` | 200 B |
| `TokenRules<MiniLang>` (inside it) | 144 B |
| `ScopeStack` | 24 B |
| `TriggerChars` cache | 24 B |
| `ParsingStateDelta<MiniLang>` | 208 B |
| `TokenRulesOverrides<MiniLang>` | 152 B |

**Wasm payload** (release, your workspace lto/codegen-units profile, plus `panic="abort"`, `opt-level="z"`, `strip=true` — the profile a web app would actually ship):

| Probe | wasm size | techy's share |
|---|---|---|
| baseline (std runtime only, no techy) | 21.7 KB | — |
| tokenizer + state layer only (`StdTokenReader` loop, `SimpleLang`) | 37.5 KB | ~16 KB |
| full pipeline, minimal `SimpleLang` (all gates off, no scopes, no callables) | 122.7 KB | ~101 KB |
| full pipeline, `Latexlike` | 146.7 KB | ~125 KB |

A per-symbol breakdown of the native minimal-Lang binary attributes techy's code roughly as: token ~29 KB, constructs ~23 KB, engine ~22 KB, state ~18 KB, spec ~14 KB, node ~13 KB, scopes ~9 KB, error ~9 KB (native arm64 is a bit fatter than wasm, but the proportions carry over).

Three findings frame everything:

1. **A language that uses nothing still ships everything.** A `SimpleLang` whose seed has every gate off and an empty scope stack pays ~101 KB of techy wasm code, of which only ~16 KB is the tokenizer+state layer. The other ~85 KB is the construct-parser/engine/spec/scopes machinery.
2. **Unused *presets* already cost zero.** The minimal build does not contain one byte of `latexlike` — monomorphization is lazy, so the crate-is-generic-over-`L` architecture already gives you "pay only for the Langs you instantiate". The problem is confined to *core subsystems kept reachable by runtime gates*.
3. **Why they stay reachable:** the `enable_*` gates are runtime `bool`s and the rules are runtime `Vec`s, so LLVM cannot prove the command arm of the nodes-parser dispatch, `resolve_command`, the invocation/argument parsers, the spec machinery, or the scope-op apply path dead — even under full LTO. Notably, ~7 KB of that is just *drop glue* for `ParsingStateDelta`/`TokenRulesOverrides` fields the minimal Lang never populates.

## The key mechanism: where a compile-time gate must be visible

Type-level gating only strips code if the gate is a monomorphization-time constant **at every reachability site**, not just where the data lives. Gating `TokenRules.commands` alone would save almost nothing: `TokenKind::Command` still exists as an enum variant, so the dispatch arm in `nodes_parser.rs:752` (and through it `resolve_command`, `make_invocation_parser`, the spec module) stays live. The sites that matter:

- `reader.rs`: the command (:217), comment (:406), whitespace/paragraph (:96, :272) branches; the specials scan path; `PrefixTable::for_rules`.
- `nodes_parser.rs`: the `TokenKind::Command`, `GroupOpen`, specials, and paragraph-break dispatch arms — gating these is what kills the invocation/spec subsystem, with no changes needed in the driver (its factory defaults become unreachable and are never codegen'd).
- `parsing_state.rs::derived`: the temporary-group stripping logic and the scope-op apply path (scope ops are what pull in `Scope`/`Package`/BTreeMap code, via lazy scope creation).
- `delta.rs::apply_overrides` per field; `language.rs`'s stray-group-close arm.

An honest corollary: `Lang::scan_specials` is already statically dispatched, so a Lang that keeps the default pays nearly nothing for specials *code* today — specials gating is mostly about the 24-byte `TriggerChars` cache and the scan call-site.

## Solution options

### Option A — capability consts on `Lang` (code-size win, minimal churn)

```rust
pub trait Lang: Sized + 'static {
    const FEATURES: FeatureSet = FeatureSet::ALL;   // associated-const defaults are stable
    // … everything else unchanged
}
pub struct FeatureSet {
    pub whitespace: bool, pub paragraphs: bool, pub groups: bool,
    pub temporary_groups: bool, pub commands: bool, pub comments: bool,
    pub specials: bool, pub callables: bool, pub scopes: bool,
}
```

Every site above becomes `if L::FEATURES.commands && rules.enable_commands { … }`. Monomorphized MIR const-folds the branch; the dead arm and everything only it reaches is never codegen'd.

- **Pros:** no API shape change (struct literals, tests, deltas all untouched); existing `Lang` impls keep compiling (defaulted const); the guards are low-noise; trivially extended later.
- **Cons:** no memory saving — fields, and their drop glue/PartialEq/Debug code, remain. Needs a documented semantics for "lying" data (const-off wins; data is inert — same spirit as gate-`false` vs. empty-data today).
- **Estimated effect:** minimal-Lang techy wasm ~101 KB → very roughly 45–60 KB (estimate, not measured — the invocation/spec/scopes chain and most reader branches go; engine/node/error cores stay).

### Option B — facet associated types (memory + unrepresentability + the rest of the code)

The full version of your idea, following the `NodeExtTypes` precedent — one bundle associated type so `Lang` grows by one item, not nine:

```rust
pub trait Lang {
    type Features: LangFeatures<Self>;   // SimpleLang → FullFeatures
}
pub trait LangFeatures<L: Lang> {
    type Commands:  CommandsFeature;        // = CommandRules | ()
    type Groups:    GroupsFeature<L>;       // = GroupRules<L> | ()   (owns temporary_groups + expecting_group_close + PrefixTable cache)
    type Specials:  SpecialsFeature;        // = SpecialsGate | ()    (owns enable flag + TriggerChars cache)
    type Scopes:    ScopesFeature<L>;       // = ScopeStack<L> | ()   (owns delta's scope_ops type too)
    // whitespace/comments: see granularity question below
}
```

Each facet trait carries `const ENABLED: bool` (so Option A's DCE mechanism is the same, expressed through the type), the accessors the reader/engine need, a `type Overrides: Default` for the delta mirror, and fallible install operations for parse-time writers. `StateData` fields become facet types; for a gated-off Lang they are ZSTs.

- **Pros over A:** fields, their drop glue (~7 KB measured in the delta types alone), and their clone/eq/hash code disappear; a gated-off Lang *cannot construct* command rules or scope ops — misconfiguration becomes a compile error rather than inert data; deltas shrink (they're cloned into `DeriveError`, hashed by the memo, and built per descent — smaller helps speed too).
- **Cons:** the churn is real and concentrated in exactly the types you've most carefully polished: `TokenRules`/`TokenRulesOverrides` struct literals stop working in their current form (every test constructs them literally), `derived()`'s temporary-group logic moves behind the groups facet, and docs/semver surface grows per facet. Also, adding an associated type without a default breaks every existing `Lang` impl by one line (associated-type defaults are still unstable — the reason `SimpleLang` exists).

### Option C — coarse tiers instead of independent facets

`type Syntax: SyntaxTier` with three or four closed bundles (chars-only; chars+groups+comments; +callables; +scopes). Kills the 2⁹ combinatorial test/doc surface, matches the realistic user populations, and fits the "closed structural core" doctrine — at the price of users who want an unanticipated combination (e.g. commands-without-groups) being out of luck.

### Option D — cargo features: wrong tool here, but a complement

Feature flags are global per build, so two Langs in one binary couldn't differ; they scatter `#[cfg]` through exactly the code you want clean; and they can't express "this *Lang* lacks scopes". They remain right for whole optional modules (serde — already Decision 4; potentially the `latexlike` preset as a default-on feature), orthogonal to this design.

### Sub-option worth noting — per-Lang reader type

`TokenReader` is already a trait, but `Language::parse_source` hardcodes `StdTokenReader`. A `type Reader` on `Lang` would let a chars-only web Lang plug a trivial scanner. Given the tokenizer is only ~16 KB of the problem I'd not do this *for size*, but you may want the hook anyway for other reasons; gating inside `StdTokenReader` via A/B covers the size goal.

## Subtleties the design must answer (found while tracing)

1. **A dependency lattice exists among features.** Temporary groups are minted *by argument parsers at parse time* (the optional-`[…]` machinery), so callables-with-optional-arguments ⇒ groups. Environments ⇒ commands. But **callables do *not* imply scopes**: a driver can resolve commands from a fixed table — precisely your web-app case (fixed command set, no `\newcommand`, scopes gated off). This argues for keeping *callables* and *scopes* as separate gates, as your message suggested. Under Option B the lattice can be enforced with bounds on the std argument parsers (a Lang gating groups off simply can't name parsers that mint temporaries — compile error at driver-assembly time, which is the right place). Under Option A it's a documented contract whose violation must surface as an `Err` through the recover funnel, per the panic policy — never a panic.
2. **The delta must mirror the gating** (`TokenRulesOverrides`, `scope_ops`) or the memory/code win evaporates — this is the single biggest churn driver in Option B.
3. **Derived caches belong to their facets.** `prefix_table` (8 B + rebuild code) goes with groups; `trigger_chars` (24 B) with specials; the freeze-time gate baking already in place generalizes cleanly.
4. **Off gains a third spelling.** Today's doctrine: gate-`false` = scoped off, empty data = constitutive off. Type/const-off becomes "the language doesn't have this feature *at all*" — stronger than both, and the existing two keep their meaning for features the Lang does have. The DESIGN_RATIONALE §3.2 entry extends rather than reopens.
5. **The memo gets cheaper for free** — gated-off delta fields hash as nothing; no soundness interaction since the gate is a compile-time constant of `L`.
6. **Memory is the secondary win.** Even full Option B takes a chars+groups Lang's state from 200 B to ~130 B, a truly minimal one to ~40 B (halve for wasm32). States scale with transitions, so this is tens-to-hundreds of KB on big documents — nice, not transformative. The payload win and the unrepresentability are the headline; worth being honest about this in the rationale.

## Recommendation

Phase it, and keep the granularity coarse:

1. **Phase 1 — Option A** (`const FEATURES: FeatureSet`, defaulted all-on), guarding the enumerated sites. Near-zero churn, immediately measurable with the probes I built, and it forces the full audit of reachability sites — which is exactly the audit Option B needs anyway. Route every check through the one const so a later facet migration re-points sites without re-finding them.
2. **Phase 2 — Option B for the two facets where the type-level payoff is real:** `Scopes` (biggest single subsystem: spec+scopes code, the `scope_ops` delta field, the lazy-`Scope` chain) and `Groups` (the largest per-state data block: two `Vec`s + expected-close + prefix table, plus the temporary-group logic in `derived()`). Leave whitespace/paragraphs/comments/specials const-only — their data is a few dozen bytes and the const already strips their code; facet types there would be churn for its own sake.
3. Ship a `FeatureSet`-combination test matrix of ~4 representative Langs (all-on = the existing suite; all-off; groups-only; callables-without-scopes) rather than the full lattice.

Projected end state for the minimal web Lang: ~101 KB of techy wasm down to an estimated ~45–55 KB, per-state memory roughly halved, and one library — no cfg forest, no second crate. Independent of all this: the size-tuned profile (`panic="abort"`, `opt-level="z"`, `strip`) cut every probe ~40% — worth documenting for web embedders regardless of gating.

Open questions for you before anything moves: (a) A-then-B phasing vs. straight to facets; (b) facet granularity — my coarse proposal vs. your original finer list (specials/callables/groups/temporaries each independent); (c) naming — `FeatureSet`/`LangFeatures` (matches §3.2's "feature gates" vocabulary) vs. `Caps`/`Syntax`; (d) whether commands-without-groups-style combinations matter enough to rule out Option C entirely. Happy to turn this into a dev-docs proposal file or prototype Phase 1 on a branch once you've weighed in.

result: analysis of Lang-generic feature gating delivered — measured wasm/memory baselines (minimal Lang ships ~101 KB techy code, ~85 KB of it runtime-gate-reachable machinery), identified the dispatch-arm reachability mechanism, and proposed a phased const-then-facet design for review.

---

**User:**

/md-export dev-docs/extra/GateFeaturesOptimizedLangs.md

---

**Assistant:**

Exporting this conversation to `dev-docs/extra/GateFeaturesOptimizedLangs.md`.

The script didn't find this session's transcript under the worktree's project directory. Let me check what's available.

The transcript is stored under the main project directory, not the worktree path. Retrying with `--cwd`.
