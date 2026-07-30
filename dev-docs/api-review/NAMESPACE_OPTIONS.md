# NAMESPACE_OPTIONS — Public export topology for techy (Phase 2a-prep)

Written 2026-07-29 for the Phase 2a policy session. Inputs: PLAN.md (2026-07-29 reframe:
techy = backend for frameworks; exports follow access-tier logic; restructuring allowed
NOW, zero dependents), INVENTORY.md (205 items), SYNTHESIS.md (tier data, F2),
ARCHITECTURE.md ([§dd-arch:naming], [§dd-arch:arch], [§dd-arch:latexlike]),
DESIGN_RATIONALE ([§dd-dr:superseded-names], [§dd-dr:naming], [§dd-dr:three-strata]),
techy/src/lib.rs, techy-derive/src, docs/ guides. **Everything here is a proposal for
the user's decision; recommendations are labeled as such.**

## 0. Facts the evaluation stands on (all verified this session)

- **F-a. Zero name collisions in a flat S0/S1 facade.** Script over the rustdoc JSON
  (180 S0/S1 items, latexlike excluded): the only same-name pairs are
  `DiagnosticInfo` and `ToDiagnosticValue` as trait + derive macro — different Rust
  namespaces, deliberately paired (serde pattern), already coexisting in `error`. Also
  zero collisions between core names and latexlike names. A flat facade compiles today
  with no renames.
- **F-b. The wire vocabulary already says `core`.** Diagnostic identifiers are
  `core.token.forbidden-char`, `core.nodes_parser.unresolvable-command`,
  `core.environment_parser.missing-terminator`, …; the preset uses
  `latexlike.environments.orphan-end`. "core" vs "latexlike" is the established,
  eventually-semver-frozen area split (SYNTHESIS F9).
- **F-c. techy-derive emits textual paths** `::techy::error::{DiagnosticInfo,
  ToDiagnosticValue, DiagnosticValue}` and `::techy::__private::{String, Vec}`
  (techy-derive/src/to_value.rs:48–50, diagnostic_info.rs:155–180). These must resolve
  in *downstream* crates under whatever topology is chosen.
- **F-d. latexlike is already a facade.** `latexlike/mod.rs` re-exports flat names from
  7 internal files (`arguments.rs`, `driver.rs`, `spec.rs`, …); nobody spells
  `techy::latexlike::spec::MacroSpec`. The facade pattern is in-repo, working precedent.
- **F-e. Root-as-path traffic ≈ 0.** SYNTHESIS: 3 of 140 root re-exports were ever
  accessed via root path (all T4: `format_position`, `format_traceback`,
  `UnresolvableCommand`); guides teach module paths exclusively; 76/140 root
  re-exports untouched by any persona (F2 "curation inverted").
- **F-f. Migration surface is small.** `use techy::` spellings to update on any
  restructure: 20 lines in src doctests, 25 in techy/tests/, 52 in docs/; internal
  code uses `crate::` paths (unaffected by public-module privatization); no
  `#[macro_export]` macros exist.
- **F-g. Tier sizes are steep and real.** T1=24 items, ∪T2=29, ∪T3=67, ∪T4=73 (of
  205). T1 is a strict subset of T2∪T3∪T4. The 18-item 3+-persona core = parse entry +
  result + diagnostics + node reading + `Package` + the 6-name latexlike happy path.
- **F-h. pylatexenc's parser-side defs database is small**: 633 lines / ~253 specs
  (`latexwalker/_defaultspecs.py`). (The 2000-line latex2text database is
  *conversion*-side — it belongs to techy-totext, not techy.)

## 1. Preliminaries that de-risk every option

### P1. Route ALL derive-emitted paths through `__private` (recommended regardless of option)

Today the derive emits `::techy::error::…` (F-c). If `error` stops being a public
module, downstream derive expansions break. Rather than re-pointing the derive at
whatever public topology wins, adopt the serde discipline: `__private` (already
`#[doc(hidden)]`) re-exports everything generated code needs
(`DiagnosticInfo`, `ToDiagnosticValue`, `DiagnosticValue`, `String`, `Vec`), and the
derive emits only `::techy::__private::…`. Effect: **criterion 6 stops constraining the
topology decision entirely, now and forever** — future reshuffles never need derive
lockstep again. Cost: ~10 emitted-path sites in techy-derive + 3 `pub use` lines in
`__private`. This is worth doing even under O1.

### P2. What actually breaks a public path (taxonomy for criterion 2)

- *Internal file moves* (splitting `constructs/` files, merging `spec` into `scopes`
  internally) break nothing under any option **if** public modules are re-export
  facades rather than the literal src tree. Today's `pub mod source;` etc. expose the
  literal tree, so internal topic reorganization is public-breaking under O1.
- *Moving an item between public modules* breaks every spelling through the old module.
  Root re-exports (`pub use constructs::StopSpec` → `techy::StopSpec`) are
  **location-independent**: the root name survives any internal move. Ironically O1's
  untaught root layer is its only reshuffle-stable surface (F-e).
- *Renames* break everything under every option — out of scope here (naming review).
- *Signatures* are not path-fragile: Rust resolves paths at definition site; only
  *textual* paths (derive output — P1; guides, doctests — criterion 4) and downstream
  `use`/turbofish spellings matter.

### P3. Additive promotion (the asymmetry that shapes O3)

Adding a root re-export for an item that already lives in a public namespace is
**additive** (semver-minor; the usual glob-import caveat: a new root name can collide
with a downstream `use techy::*;` — treated as minor per API evolution guidelines,
and `use techy::*` is exactly what curation makes reasonable to discourage).
Removing or moving a public path is **breaking**. Consequence: the safe long-term
policy is *"every item's permanent home is the machinery namespace; curation happens
by adding root re-exports on top"*. Items can be promoted later at zero cost; they can
never be cheaply demoted. Start conservative at root.

---

## 2. O1 — Baseline: status quo (for comparison only)

9 public topic modules mirroring the src tree + 140 root re-exports + `latexlike`
(module-only) + `node::extract` (module-only) + `VERSION` + `guide`.

```rust
pub mod constructs; pub mod engine; pub mod error; pub mod latexlike; pub mod node;
pub mod scopes; pub mod source; pub mod spec; pub mod state; pub mod token;
pub use constructs::{ChildStateSpec, /* …30 */};
pub use engine::{CommandResolution, /* …9 */};
/* … 140 root re-exports total … */
pub const VERSION: &str = …;
```

**Scores.**
1. *Tier-logic fit: 1/5.* Inverted (F2): staging machinery floods root autocomplete
   while the T1/T2 home (`latexlike`, `extract`) is the only deep-path-mandatory
   surface. 76/140 root names touched by nobody.
2. *Reshuffle freedom: 2/5.* The 9 public modules ARE the internal tree; every topic
   reorganization is public-breaking twice (module path + guide spelling). Concrete:
   moving stop conditions (`StopSpec`, `StopCause`, `TokenStopCondition`,
   `TokenStopKind`) out of `constructs` into their own module breaks
   `techy::constructs::StopSpec`; merging `spec` into `scopes` (they are jointly
   presented as one topic in ARCHITECTURE/CLAUDE.md already) breaks 6 paths. Only the
   root layer survives moves — the one surface nobody uses as a path.
3. *Rustdoc: 2/5.* Topic sidebar is good; the root page is a 140-name flood that
   buries `Language` between `CallableDefinedAsError` and `ChildRegion`.
4. *Guide-spelling stability: 2/5.* Guides teach `techy::engine::Language`,
   `techy::state::ParsingStateDelta` — the fragile spellings.
5. *FFI/framework stability: 2/5.* Framework crates (`flm`, techy-totext, PyO3 shims)
   would import module paths (that is what the docs teach) and transitively re-teach
   them; every internal reorganization ripples.
6. *derive/no_std: 5/5* (paths valid as-is; P1 still advisable).
7. *Migration now: 5/5* (zero).
8. *Naming: 3/5.* No new names; but keeps the state SYNTHESIS documents as inverted,
   and keeps the `PrefixEntry`/condition-family visibility inconsistencies.

**Verdict**: fails the review's stated goal (never being forced to restructure later)
— any post-1.0 topic-boundary change is breaking. Included only as the yardstick.

---

## 3. O2 — Single facade: all S0/S1 machinery public ONLY via one namespace

Internal modules become private (`mod source;` …); one public namespace re-exports
everything; `techy::latexlike` unchanged. The facade is a *re-export layer*, never the
src tree — internal organization becomes permanently invisible.

### 3.0 The facade's name: `core` vs `parsing` (applies to O2 and O3)

Superseded-names register: neither name (nor `util`, `machinery`, `extract`, `defs`)
is reserved or rejected — no reintroduction problem either way.

**For `core`:**
- **Wire alignment (F-b)**: diagnostic identifiers already say `core.*` /
  `latexlike.*`, and F9 wants identifiers semver-frozen. `techy::core` vs
  `techy::latexlike` makes paths and wire vocabulary one system. `parsing` would
  make the same stratum carry a third name (paths "parsing", wire "core",
  ARCHITECTURE "S0/S1").
- **Prose alignment**: ARCHITECTURE and CLAUDE.md already speak of "the core"
  ("no privileged language concepts in the core"; S1 is literally labeled "core").
  Adopting `core` for S0+S1 jointly needs only a one-line ARCHITECTURE touch-up
  ("the public namespace `techy::core` covers S0+S1").
- Short; heavy-traffic segment (T3 spells it hundreds of times).
- Precedent: preset-vs-core is exactly the boundary design principle 3 enforces.

**Against `core` (honest):**
- **Extern-prelude shadowing.** `core` is a Rust built-in crate. Full paths
  (`techy::core::Span`) never conflict, and `use techy::core::Span;` is fine. But a
  downstream `use techy::core;` followed by bare `core::…` in the same module now
  resolves to *techy's* module (imports beat the extern prelude), and at a crate root
  that does both, rustc raises E0659 ambiguity. Failure mode is loud (compile error),
  never silent, but it is a real paper cut for precisely techy's no_std audience, who
  write bare `core::…` paths. In-crate: techy's own lib.rs currently has no bare
  `use core::…` (verified); submodules are unaffected (2018 path rules).
- "Core of what?" — principle 2 (specificity). Defensible under principle 4 (inside
  crate `techy`, context disambiguates) *except* that the sibling vocabulary
  competing in scope is Rust's own `core` crate — the exact situation principle 4
  warns about.

**For `parsing`:** descriptive; zero collision with anything; reads naturally
("the parsing machinery").

**Against `parsing`:** near-vacuous *for this crate* — everything in techy is parsing;
`latexlike` is also parsing; `extract` arguably isn't. It answers "what does techy
do", not "which part of techy is this" — the namespace's actual job. And it forks the
vocabulary from the wire identifiers (or forces renaming ~13 frozen-to-be identifiers
to `parsing.*` now, enlarging the wire-stability decision).

Other candidates considered and set aside: `machinery` (accurate, verbose, unidiomatic),
`engine` (already taken with a narrower meaning — reuse would be a collision with the
existing topic, principle "collision avoidance beats tradition"), `base` (collides
with the seed package `"base"`), `advanced` (pure tier label, no semantics),
`internals` (connotes not-public-API), `lang` (too narrow; also near `Lang`).

*Recommendation (user decides):* **`core`**, on wire-alignment + prose-alignment; the
extern-prelude caveat is a loud, avoidable edge case. Everything below writes `core`;
substitute `parsing` freely — only criterion 8 scores change.

### 3.1 O2a — flat facade (`techy::core::TokenRules`)

```rust
// lib.rs — internal topic modules: private, freely reorganizable
mod constructs; mod engine; mod error; mod node;
mod scopes; mod source; mod spec; mod state; mod token;

pub mod latexlike;                    // S2 preset, unchanged (already flat, F-d)

/// The S0/S1 machinery: the complete core API in one namespace.
pub mod core {
    pub use crate::source::{LineIndex, MapResolver, NoResolver, ProvenanceChain,
        ResolveError, ResolvedContent, Source, SourceOrigin, SourceProvenance,
        SourceResolver, SourceSpan, Span, TextContent, resolve_source};
    pub use crate::error::{Diagnostic, DiagnosticData, DiagnosticInfo, /* … */};
    pub use crate::token::{CommandRule, /* …20, now incl. PrefixEntry */};
    pub use crate::state::{ClosedVocabulary, Lang, /* …9 */};
    pub use crate::spec::{ArgumentParser, ArgumentSpec, /* …6 */};
    pub use crate::scopes::{Package, Scope, ScopeStack, /* …16 */};
    pub use crate::node::{NodeKind, NodeRef, NodeTree,
        /* …30 + the 8 ext aliases (NodeExt, GroupNodeExt, …) */};
    pub use crate::constructs::{ConstructParser, GroupArgumentParser,
        /* …all 52: dispatch + argument/takeover parsers + all 19 conditions */};
    pub use crate::engine::{CommandResolution, Language, ParseResult, /* …9 */};

    /// Extraction helpers — a function library that keeps its qualifier (see below).
    pub mod extract { pub use crate::node::extract::*; }
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
#[cfg(doc)] pub mod guide { /* unchanged */ }
#[doc(hidden)] pub mod __private { /* + P1 additions */ }
```

Notes on the sketch:
- **Explicit lists, not globs** (`pub use crate::source::*` would work but silently
  auto-exports every future item; the explicit list is the curation record — each new
  public name is a conscious export decision). ~180 names ≈ 40 wrapped lines.
- **`extract` stays a submodule** even in the flat variant. Its names were designed
  for qualified reading: `extract::content_as_chars`, `extract::Split`,
  `extract::KeyVals`. Flattened, `Split` and `KeyVals` violate principle 2
  (specificity — split of what?) and would need renames, reopening settled names.
  A *function library* is a different beast from a type vocabulary; one submodule does
  not meaningfully re-freeze topology (its topic, "extraction helpers over node
  trees", is stable by nature).
- Everything previously visibility-split is unified: `PrefixEntry` (INVENTORY oddity
  9), the 3 off-root verbatim/tack-on conditions (oddity 2), `GroupArgumentParser`
  and the other argument/takeover parsers, the 8 ext aliases.
- Optional refinement flag (user decision, not baked in): a `core::conditions`
  submodule collecting all ~22 core condition types. Pro: it is the F9
  identifier-registry made manifest in code, and it removes the largest name
  population from the flat page; "condition-ness" is a stable property (they are
  data + `Display`), so it re-freezes nothing that moves. Con: second axis of
  organization; conditions leave their producing parser's side in the docs.

**Name-collision risk (checked, F-a)**: zero today across all 180 items; zero against
latexlike. Forward risk = every future S0/S1 public item must be unique crate-wide
rather than module-wide. In practice principle 2 already demands this (vague names
like `Entry`, `Error`, `Kind` are banned regardless of module), and the register's
history shows collisions get engineered away (`Language` vs `EnvironmentSpec`).
The growth family is conditions (19 and counting), whose names are already
global-styled (`UnclosedGroup`, `StrayGroupClose`). Residual risk: low, and a
collision would surface at facade-compile time, forcing a rename *before* release —
the safe failure direction.

**Scores.**
1. *Tier-logic fit: 3/5.* Two tiers only (machinery vs preset). T1/T2 read
   `techy::core::NodeRef`, `techy::core::Package` — everyday reading vocabulary lives
   in the machinery namespace; "core" reads advanced but is the only path. Tier logic
   is respected coarsely, not curated.
2. *Reshuffle freedom: 5/5.* Any internal move is invisible: stop conditions to their
   own file/module — invisible; `spec`+`scopes` merge — invisible; `node::extract`
   relocated — invisible (facade submodule re-points); splitting S0 into its own
   crate later — invisible (`pub use techy_source::…` from the facade;
   [§dd-dr:three-strata] "revisit if" is covered).
3. *Rustdoc: 3/5.* Crate root: beautiful (guide, latexlike, core, VERSION). The core
   page lists ~180 items grouped only by kind (structs/enums/traits/fns) — browsable
   with Ctrl-F, weak for topic discovery; the guide pages and doc-links must carry the
   topic map (they already do the teaching today, F1). `#[doc(inline)]` on the facade
   re-exports makes docs render in place.
4. *Guide-spelling stability: 5/5.* Guides teach `techy::core::X` /
   `techy::latexlike::Y` — spellings that survive every internal reorganization.
   One-time rewrite of ~97 import lines (F-f).
5. *FFI/framework stability: 4/5.* Frameworks and PyO3 shims import from two stable
   namespaces; `use techy::core::*;` is a legitimate one-line import for generated
   bindings. Not 5: common-vocabulary items (Span, Diagnostic) sit at the same depth
   as staging machinery — no privileged stable "small surface" for bindings authors
   to target.
6. *derive/no_std: 5/5 with P1* (without P1: derive must switch to
   `::techy::core::error`-free paths in the same change — same-workspace lockstep,
   one-time).
7. *Migration now: 3/5.* lib.rs rewrite (~40 facade lines replace 140 re-export
   lines + 9 pub mod → mod); ~97 import-line updates (F-f); P1 (~13 sites); no
   signature changes anywhere.
8. *Naming: 4/5 with `core`* (wire+prose alignment; extern-prelude caveat), *3/5 with
   `parsing`* (vocabulary fork or wire rename). `Split`/`KeyVals` naming preserved by
   keeping `extract` as submodule.

### 3.2 O2b — structured facade (`techy::core::token::TokenRules`)

```rust
mod constructs_impl; /* or same names privately */ …

pub mod latexlike;
pub mod core {
    pub mod source     { pub use crate::source_impl::{Source, Span, /* …14 */}; }
    pub mod error      { pub use crate::error_impl::{Diagnostic, /* …14 */}; }
    pub mod token      { /* …20 */ }
    pub mod state      { /* …9 */ }
    pub mod spec       { /* …6 */ }
    pub mod scopes     { /* …16 */ }
    pub mod node       { /* …38 */ pub mod extract { /* …9 */ } }
    pub mod constructs { /* …52 */ }
    pub mod engine     { /* …9 */ }
}
```

**How much reshuffle freedom does the structure actually sacrifice vs O2a?** The
precise answer: O2b freezes the **topic taxonomy** (which topic each name publicly
belongs to), not the **file tree**. Because the facade is a re-export layer, internal
file moves stay invisible exactly as in O2a — `constructs/` can be split into five
files and `core::constructs::StopSpec` still resolves. What breaks is any change to
the *public assignment*:
- Moving stop conditions out of `constructs` into a public `core::stop` topic —
  breaking (or the facade keeps the stale `constructs` home and the taxonomy rots
  into a lie ARCHITECTURE has to footnote).
- Merging `spec` and `scopes` into one public topic (they are already presented as
  one topic "specs and scopes" in ARCHITECTURE) — breaking.
- Splitting `node` into read-side and build-side topics (INVENTORY oddity 6 pressure)
  — breaking.
- Reclassifying a condition type to live beside a registry — breaking.
All four are *live* candidate reorganizations visible in the current review — this is
not hypothetical freedom. O2b's freeze is smaller than O1's (files free, root flood
gone) but it re-freezes exactly the axis (topic boundaries) that the current 9-module
split has already had to revise once (token S0→S1, `Span` moved to source —
[§dd-dr:three-strata] consequences note).

**Scores.** (1) Tier fit 3/5 — same as O2a. (2) Reshuffle 3/5 — taxonomy frozen, see
above. (3) Rustdoc 4/5 — best browse-by-topic, one extra sidebar level. (4) Guide
stability 3/5 — guides teach `techy::core::state::ParsingStateDelta`; topic
reassignments re-break guides. (5) FFI 3/5 — frameworks mirror a 9-way taxonomy that
techy can no longer change. (6) 5/5 with P1. (7) Migration 3/5 — same scale as O2a.
(8) Naming 4/5 — no new names beyond `core` (topic names are established).

---

## 4. O3 — Tiered namespaces (public paths mirror ACCESS TIERS, not topics)

The user's sketch, made concrete with SYNTHESIS data. Three public levels:

- **`techy::` root — the common vocabulary**: the small set every tier touches.
- **`techy::latexlike` — the preset tier** (T1 entry + T2 extender home), unchanged.
- **`techy::core` — the machinery tier** (language designers, tooling, framework
  internals): **the complete S0/S1 namespace, flat as in O2a**.

Root items are **curated re-exports of items that also live in `core`** (superset
model), not a disjoint partition. This is forced by P3: with a disjoint split,
*promoting* an item to root would mean removing its `core` path — breaking; with the
superset, promotion is purely additive, demotion never happens because root starts
minimal. `core::*` keeps one simple invariant: *the whole machine is here*.

```rust
mod constructs; mod engine; mod error; mod node;
mod scopes; mod source; mod spec; mod state; mod token;

pub mod latexlike;                         // preset tier (T1 entry + T2 home)
pub mod core { /* exactly O2a: complete flat S0/S1 + core::extract */ }

// ---- Common tier: curated re-exports (each also reachable via core::) ----
pub use core::{
    // parse entry + result
    Language, ParseResult,
    // diagnostics consumption (not the defining surface)
    Diagnostic, Diagnostics, Severity, ParseError, Recovery,
    format_position, format_traceback,
    // source + position vocabulary
    Source, Span, SourceSpan, LineIndex,
    // node reading
    NodeTree, NodeRef, NodeSlice, NodeKind,
    // the one extensibility item every tier shares
    Package,
};
pub use core::extract;                     // T1 helper library, root-visible module

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
#[cfg(doc)] pub mod guide { … }
#[doc(hidden)] pub mod __private { … }
```

**Draft common tier: 18 type/fn names + `extract` + `VERSION`.** Assignment logic,
from SYNTHESIS §1/§2 (empirical, not frequency — each entry is *vocabulary that every
access tier speaks*):
- The ten 4-persona items: `Language`, `ParseResult`, `Recovery`, `ParseError`,
  `Diagnostic`, `Diagnostics`, `Package`, `NodeRef`, `NodeSlice`, `SourceSpan`.
- 3-persona non-preset: `NodeTree`, `NodeKind`.
- User-named S0 vocabulary: `Source`, `Span`, `LineIndex`, `Severity` (T1 display +
  T4; `Span` is 2-persona but is the *type vocabulary* of `SourceSpan`/`Token` — the
  pair travels together).
- `format_position`, `format_traceback`: the only items any persona ever reached via
  a root path (F-e); T1+T4.
- `extract` module: T1's mandatory helpers; root visibility resolves the
  inconsistency T1 flagged (the only mandatory deep path in an otherwise dual-pathed
  API — now it is a sanctioned tier-1 module).
- **Deliberately NOT at root** (the judgment calls — full list in Appendix A):
  `ParsingStateDelta`, `ArgumentSpec`, `TokenRulesOverrides` (T2's reach into core is
  a *structured reach* by design), all condition types (empirical T1 downcast usage:
  zero — SYNTHESIS §3), `Descendants`/`NodeSliceIter` (never named, only iterated),
  `TextContent`, `ParsingState`, payload types (`GroupData`, `CallableData`,
  `ParsedArguments`, …), the whole resolver/provenance family (T4 = core tier),
  `SpecsProvider`/`ScopeStack` (T2 stretch + T3).

Optionally (flag for the 2a session, not baked in): `latexlike` re-exports the
extender kit it teaches (`ArgumentSpec`, `ParsingStateDelta`, `TokenRulesOverrides`,
maybe `Package`), making `techy::latexlike` a one-stop T2 home. Pro: T2's F2
complaint ("my home module is the only one requiring full paths") dissolves
completely; matches "S1 never names a preset" (the preset naming S1 is the allowed
direction). Con: second public spelling for 3–4 items; guides must pick one
canonical spelling (they would teach the latexlike one for T2 chapters).

**Scores.**
1. *Tier-logic fit: 5/5.* Paths literally encode the tiers: root = common, latexlike
   = preset/extender, core = machinery. The T1 walkthrough's entire import block
   becomes `use techy::{Language, NodeKind}; use techy::latexlike::…;`.
2. *Reshuffle freedom: 5/5.* Identical to O2a for internals (root names are
   location-independent re-exports, P2; core is flat). The only new frozen surface is
   the curation list itself, which only ever grows (P3).
3. *Rustdoc: 4/5.* Root page = 18 curated items + 3 modules + guide — the landing
   page *is* the T1 story. `core`'s 180-item flat page remains the weak spot (same
   as O2a; mitigated by guide pages and the optional `core::conditions` grouping).
4. *Guide-spelling stability: 5/5.* Guides teach root spellings for common items
   (maximally stable — survive even a future re-curation of `core`'s internals),
   `latexlike::` for the preset, `core::` for machinery chapters. Every spelling is
   reshuffle-proof.
5. *FFI/framework stability: 5/5.* Bindings authors get an explicit, small, stable
   "wire-adjacent" surface (root = the types that cross FFI boundaries: spans,
   diagnostics, trees, results) plus a stable machinery namespace. Framework crates
   re-exporting techy types re-export root paths.
6. *derive/no_std: 5/5 with P1.*
7. *Migration now: 3/5.* O2a's mechanics + ~20 root `pub use` lines + the curation
   decision itself (this document's Appendix A is that decision's worksheet).
8. *Naming: 4/5* with `core` (as O2a); the root layer introduces no new names at all.

**Variant O3-s (structured core):** same root curation + O2b's structured core.
Scores follow O2b on criteria 2/4/5 (taxonomy frozen). Included in the matrix for
completeness; it is dominated by O3 unless rustdoc topic-browsing is weighted heavily.

---

## 5. O4 — Placement of the low-level layer (Span, Source, Diagnostic, …)

Four variants for S0 specifically (interacts with, but is separable from, O2/O3):

- **O4-root (= O3's answer): common S0 vocabulary at `techy::` directly**
  (`Span`, `Source`, `SourceSpan`, `LineIndex`, `Diagnostic`, `Diagnostics`,
  `Severity`, `ParseError`, `Recovery`, `format_*`); the S0 *machinery*
  (resolvers, provenance, `TextContent`, defining-side diagnostics traits) in `core`.
  Argument: these are the types every tier and every framework speaks — per tier
  logic they belong to the topmost tier; per naming principle 4, at root there is no
  competing sibling vocabulary (nothing else at root is called anything like `Span`).
  This split does cut the `source` topic in two path-tiers — but along an
  empirically real line (SYNTHESIS: T4's 6 unique items are exactly the
  provenance/resolver family).
- **O4-mod: keep `source` and `error` as named public modules** (`techy::source::Span`,
  `techy::error::Diagnostic`), machinery elsewhere. Argument for: S0 is a true DAG
  with two stable, well-named topics; it is the designated future crate-split seam
  ([§dd-dr:three-strata] revisit note), and a public module converts to a crate
  re-export losslessly. Argument against: it freezes two topic names *without need*
  (a root re-export converts to a crate re-export just as losslessly); it preserves
  the "which module was Recovery in again?" lookup tax (`Recovery` lives in `error`,
  spans in `source` — T1 called the 3-module scatter out as friction F4); and it
  makes the facade story inconsistent ("everything is behind core… except these two
  modules"), reintroducing the O2b taxonomy-freeze for a third of the API's
  everyday names.
- **O4-util: `techy::util::*`.** Rejected on naming principles: "util" is the
  canonical vague name (principle 2: util of *what*? — it answers nothing); the
  register's whole history moves *away* from junk-drawer names (`ContextDb`,
  `LibrarySet`). ARCHITECTURE's own name for the stratum is "foundation", and S0's
  contents are not utilities — they are the source model and the diagnostics model,
  two of the crate's proudest surfaces (T4: "exemplary"). A `util` module would also
  become a gravity well for future misc items (the exact anti-pattern the tier logic
  is meant to prevent).
- **O4-core: S0 entirely inside `core`, nothing at root** (= pure O2). Coherent,
  maximally uniform; loses the tier signal for the cross-FFI vocabulary; covered by
  O2's scores.

*Recommendation (user decides):* O4-root within O3; O4-core within a pure O2. O4-util
rejected outright; O4-mod only if the user wants `source`/`error` as permanent,
guaranteed-stable topic names (they are the two best candidates if any topic names
are to be frozen — but freezing none is strictly more freedom).

---

## 6. Standard LaTeX definitions database ("defs") — orthogonal placement decision

The planned database of standard LaTeX definitions (`\emph`, `\cite`, `itemize`, …;
T1 wishlist #3; pylatexenc parity F-h). Three placements:

**(i) Separate crate (`techy-latexlike-defs` or similar).**
- Compile cost: truly zero when not depended on. Version lockstep: real but one-way
  (defs depends on techy; techy never depends on defs) — it must track techy's
  breaking releases forever, and *its own* releases must be coordinated for
  latexlike-behavior changes (a new argument-code feature lands in techy, defs wants
  it: two-crate release dance; serde/serde_derive live this and it is a known tax).
- Discoverability: worst of the three — pylatexenc users expect the database
  in-package (pylatexenc precedent: `_defaultspecs.py` ships inside latexwalker);
  T1's onboarding wish was precisely "it's just there".
- Naming: a third workspace crate named `techy-latexlike-defs` is a triple-barrel
  name; `techy-defs` hides that it is preset-specific (a future flm crate would have
  its own database — the database is per-preset by nature).
- no_std: fine either way (data + `Package` construction, no I/O).

**(ii) `techy::latexlike::defs` module (in-crate).**
- Compile cost, honestly: an unused-by-downstream pub module is **not** free at
  compile time — techy's own build (and every downstream build of techy) parses,
  type-checks, and codegens the non-generic table-building fns. Scale check (F-h):
  pylatexenc's parser-side database is 633 lines / ~253 specs; the Rust equivalent
  (a few `fn package_latex_base() -> Package<Latexlike>` builders over static
  tables) is plausibly 1–3k lines — **sub-second compile cost**. Binary cost: unused
  non-generic fns are dead-stripped at link time (and with LTO certainly); a
  constructed-on-demand database has zero static footprint. The user's
  "obvious optimize-out if not even imported" holds for the *binary*, not for
  *compile time* — at this size, immaterial.
- Discoverability: best (sits in the preset namespace beside `base_package`, which is
  its existing tiny sibling — the seed package precedent).
- **Reversibility (the decisive property): (ii) does not foreclose (i).** If the
  database ever grows huge (full CTAN-scale coverage), extract it to an
  optional-dependency crate and keep the path:
  `#[cfg(feature = "defs")] pub use techy_latexlike_defs as defs;` under
  `techy::latexlike` — every `techy::latexlike::defs::…` spelling survives. Starting
  with (i) and folding *in* later is equally possible but pays the two-crate tax up
  front for a 633-line-equivalent database.
- Naming inside: `defs` vs `definitions` vs `packages`. `packages` is semantically
  attractive (the module's contents ARE `Package` values, and the LaTeX analogy —
  `\usepackage{amsmath}` ↔ `defs::amsmath()` — is the intended mental model) but
  collides head-on with Cargo/Rust "packages" in every sentence written about it.
  `definitions` follows principle 3 (clarity over brevity: `TokResult` was rejected);
  `defs` is the compact form and matches the scopes vocabulary (`DefinitionOp`).
  Judgment call for the session: `definitions` is the principles-conformant pick;
  `defs` is defensible as an established CS term rather than an ad-hoc truncation.
- Internal shape suggestion (not part of the topology decision): per-package factory
  fns returning `Package<Latexlike>` (`definitions::latex_base()`,
  `definitions::amsmath()`, …) so consumers pay only for the packages they call —
  which also makes the module trivially dead-strippable.

**(iii) Cargo feature (`features = ["defs"]`, module behind `#[cfg]`).**
- Saves the sub-second compile cost when off; costs a permanent feature-matrix
  dimension (docs.rs metadata, CI matrix, "why is defs missing" support traffic).
  If the feature defaults ON (the discoverable choice), everyone pays the compile
  cost anyway and the gate is pure ceremony; if OFF, T1 onboarding trips over it
  (wishlist #3 was an onboarding wish). Features are also additive-only across the
  dep graph — harmless here, but a knob without a payoff at this scale.

*Recommendation (user decides):* **(ii)** `techy::latexlike::defs` (or
`::definitions`) as a plain module, per-package factory fns, no feature gate;
revisit (i)-via-re-export only if the database's size ever becomes a measured
compile-time problem. Precedent (pylatexenc in-package), discoverability, and
reversibility all point the same way; the compile-cost argument for (i)/(iii) is
real but ~sub-second at the plausible size.

---

## 7. Comparison matrix

Scores 1–5 (5 best). `core`-named facades assumed; with `parsing`, criterion 8 −1.

| # | Criterion | O1 status quo | O2a flat facade | O2b structured facade | O3 tiered (flat core) | O3-s tiered (structured core) |
|---|---|---|---|---|---|---|
| 1 | Tier-logic fit | 1 | 3 | 3 | **5** | 5 |
| 2 | Reshuffle freedom | 2 | **5** | 3 | **5** | 3 |
| 3 | Rustdoc presentation | 2 | 3 | 4 | 4 | **4.5** |
| 4 | Guide-spelling stability | 2 | 5 | 3 | **5** | 3 |
| 5 | FFI / framework stability | 2 | 4 | 3 | **5** | 4 |
| 6 | derive / no_std (with P1) | 5 | 5 | 5 | 5 | 5 |
| 7 | Migration cost now | **5** | 3 | 3 | 3 | 3 |
| 8 | Naming compliance | 3 | 4 | 4 | 4 | 4 |
| | **Sum (unweighted)** | 22 | 32 | 28 | **36** | 31.5 |

The user's own weighting makes the gap larger: criteria 1 and 2 are the stated
principles of the review; criterion 7 is a one-time cost explicitly declared
acceptable ("restructuring allowed NOW"); criterion 6 is neutralized by P1.

Structural observation: **O2a and O3 are the same option at different curation
levels** — O3 = O2a + ~20 additive root re-exports, and O2a can become O3 later
additively (P3). O2b/O3-s are NOT reachable additively from O2a/O3 (inserting topic
submodules under `core` while keeping flat names is possible — adding submodules is
additive — but *removing* the flat names again is not; a both-flat-and-structured
`core` would be a permanent double surface).

---

## Appendix A — Hard cases (items resisting clean tier assignment)

The judgment calls the 2a/2b sessions must ratify. "Core" below = machinery
namespace; "root" = O3 common tier.

| Item(s) | Tension | Proposed placement + reasoning |
|---|---|---|
| `ParsingStateDelta` | T2's scoped-definitions stretch task uses it; machinery-flavored (T3 vocabulary) | Core. T2 reaching into `core` for the *advanced* extender move is exactly "advanced surfaces are a logical, structured reach". Alternative: latexlike extender-kit re-export (§4 flag). Revisit in the T2 session (2b). |
| `ArgumentSpec` | Taught in T2's learn-by-example path; spec-side machinery | Same as `ParsingStateDelta` — core, with the latexlike-kit flag. Note `latexlike::argument_specs()` returns `Vec<ArgumentSpec>` — T2 can hold the value without naming the type. |
| `TokenRulesOverrides` | In the T2 guide example (verbatim-ish states); 13-field twin of `TokenRules` | Core (with its twin — splitting the twins across tiers would be worse than either placement). |
| Condition types (19 constructs + 3 token + `CallableDefinedAsError`) | INVENTORY tagged "(T1 downcast)"; SYNTHESIS found zero T1 downcasts — only T3/T4 via `T::IDENTIFIER` | All in core, none at root — fixes the current 16-at-root/3-off-root family split. Optional `core::conditions` registry module (§3.1 flag; pairs with F9's identifier-registry doc). latexlike's 3 conditions stay in latexlike. |
| `format_position`, `format_traceback` | The only empirical root-path traffic (T4); T1 uses them too | Root (O3). They are presentation vocabulary, not machinery. |
| `Descendants`, `NodeSliceIter` | Returned by root-tier methods; never *named* by any persona (used as `impl Iterator`) | Core only. Naming an iterator type is an advanced move (storing it in a struct); the root method remains callable without the name. |
| `TextContent` | Return of `extract::content_as_chars` (T1-adjacent); construction is T3 | Core, lean call — T1 consumed it via `.resolve()`/`Display` without naming it in the walkthrough. Flag for the T1 session. |
| `ParsingState` | Reachable from `NodeRef::parsing_state()` (T1-adjacent) | Core. Naming it = inspecting parse-time state = machinery move. |
| Payload types (`GroupData`, `CallableData`, `ParsedArguments`, `ParsedArgument`, `ParsedSlot(s)`) | `NodeKind` (root) has variants boxing them; T1 matched variants without naming payload types | Core. A consumer writing `fn f(g: &GroupData)` is doing structured node-tree work — the reach is legitimate. If F12's accessor sugar lands (kind labels, generic names), naming payloads gets rarer still. |
| `SpecsProvider`, `ScopeStack` | `Package` (root) implements/feeds them; T2 stretch + T3 use them | Core. `Package` alone covered every walkthrough's provider needs (SYNTHESIS §3). |
| `extract` fns | T1-mandatory but 9 items incl. types designed for qualified reading | Module `extract`, root-visible under O3 (`pub use core::extract;`), `core::extract` canonical under O2. Never flattened (naming casualties: `Split`, `KeyVals`). |
| `VERSION` | Root-only const, no persona used it | Keep at root (harmless, conventional). |
| `SimpleLang`, `StdParseDriver` | Read-and-rejected by T3 (F10 dead-end) | Core; their redesign/fate is the T3 session's item — placement here is not endorsement. |
| `Latexlike` re-exporting core items (extender kit) | One-stop T2 home vs double spelling | Flagged §4; decide in 2a with the guide plan on the table (which spelling do T2 chapters teach?). |

## Appendix B — Flat-facade collision scan (method + result)

Script over `techy_api.json` (rustdoc JSON, 203 items): grouped all S0/S1 items
(latexlike excluded) by (name, Rust namespace) where namespace ∈ {type, value, macro}.
Result: **no two distinct items share a name in the same namespace**. The only
same-name pairs are `DiagnosticInfo` (trait + derive macro) and `ToDiagnosticValue`
(trait + derive macro) — coexisting namespaces, already co-resident in `error`,
standard pattern. Latexlike's 23 names have zero overlap with core's 180 (so even a
`use techy::core::*; use techy::latexlike::*;` pair of globs is conflict-free today).
Forward policy under any flat facade: crate-wide name uniqueness for S0/S1 items —
already the de-facto standard per naming principle 2; violations fail the facade
build, i.e. are caught before release.

## Appendix C — techy-derive path constraints (criterion 6 detail)

Emitted textual paths (must resolve in downstream crates):
- `::techy::error::ToDiagnosticValue`, `::techy::error::DiagnosticValue`
  (to_value.rs:48–50); `::techy::error::DiagnosticInfo`,
  `::techy::error::ToDiagnosticValue`, `::techy::error::DiagnosticValue`
  (diagnostic_info.rs:155–180); `::techy::__private::{String, Vec}` (both files).
- Under O2/O3 as sketched, `techy::error` ceases to exist publicly → these break
  downstream **unless** P1 lands (route everything through `__private`) or the derive
  is re-pointed at the new public paths in the same commit (same workspace, versions
  in lockstep via `[workspace.package]` — mechanically fine, but P1 removes the
  coupling permanently and is the recommended fix under every option including O1).
- `extern crate self as techy` (lib.rs:75) keeps working under all options; a module
  named `core` at the techy crate root would make a hypothetical crate-root
  `use core::…` in lib.rs ambiguous (E0659) — lib.rs currently has none (verified),
  and submodules are unaffected (2018 edition path rules). Downstream, only the
  pattern `use techy::core;` + bare `core::…` in one scope misbehaves (loud compile
  error, never silent).

## Appendix D — Migration mechanics inventory (criterion 7 detail)

One-time changes for O2a/O3 (O2b similar):
- `techy/src/lib.rs`: 9 `pub mod` → `mod`; delete 140 root re-export names; add
  `pub mod core` facade (~40 lines explicit re-exports); O3 adds ~20 root `pub use`
  lines. Net lib.rs likely *shrinks*.
- techy-derive: P1 — 3 `pub use` lines in `__private`, ~13 emitted-path edits.
- Textual spellings: 20 doctest lines (src), 25 integration-test lines (tests/),
  52 guide lines (docs/) — ~97 mechanical `use`-line edits (F-f).
- No signatures change; no renames required (F-a); `cargo doc` link check + full test
  suite verify. Estimated: a focused half-day, agent-executable in a worktree.

---

## Recommendations (ranked; for the user's decision, not decided)

**R1 — O3 tiered, flat core, `core` name, O4-root, defs as `latexlike::defs`-module.**
Root = ~18 curated common names + `extract` + `latexlike` + `core` + `VERSION`;
`core` = the complete flat S0/S1 machinery; P1 lands with it.
*Decisive trade-off:* you take on one curation judgment now (the Appendix A list) and
in exchange get both of the review's stated goals at once — paths that encode access
tiers AND total internal-reshuffle freedom — plus the best FFI/guide stability story.
The risk is mis-curation, and P3 caps that risk: under-curation is fixable additively,
so the list only needs to be conservative, not perfect.

**R2 — O2a pure single facade (`core` + `latexlike`, nothing else).**
*Decisive trade-off:* zero curation judgment today and maximal uniformity, at the cost
of tier-logic fit — T1 consumers read `techy::core::NodeRef` and the crate root
teaches nothing. Because O3 = O2a + additive re-exports, R2 is also the "decide the
curation list later" version of R1 — a legitimate sequencing choice if the user
prefers the 2b per-item sessions to happen before any root names are committed.

**R3 — O3-s / O2b structured core.**
*Decisive trade-off:* keeps rustdoc browse-by-topic as a first-class navigation
surface, at the cost of freezing the 9-topic taxonomy into the public contract — the
very axis the codebase has already revised once (token stratum, `Span`'s home) and
has live candidates to revise again (spec+scopes merge, node read/build split,
conditions registry). Choose only if doc-page browsability outweighs taxonomy
freedom; the guide pages can carry topic navigation under R1/R2 instead.

Orthogonal riders under every ranked option: P1 (derive → `__private`) — recommended
unconditionally; defs = in-crate `techy::latexlike::defs` (or `definitions`) module,
no feature gate, per-package factory fns (reversible to an optional-dep crate later
without path breaks); `techy::util` rejected on naming principles under all options.
