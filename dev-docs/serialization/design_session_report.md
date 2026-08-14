# Serialization design sessions — report

**STATUS: TRANSIENT, NON-NORMATIVE COMPANION** to `design_plan.md` (same folder, same
lifecycle: deleted at M7 after the durable content is folded into DESIGN_RATIONALE and
ARCHITECTURE). The plan is normative — it says *what* to build. This report says *why*,
and records the routes we explored and abandoned so nobody re-walks them. If this
report and the plan ever disagree, the plan wins and the disagreement is a bug to fix.

Sessions: interactive user + assistant design discussions, 2026-08-13.

---

## 1. Governing philosophies

These emerged or were confirmed during the sessions; every decision in the plan's §3
traces to one or more of them.

**P1 — The struct is the schema; serialization is a derived projection.** Inherited
from [§dd-dr:serialized-schema] (diagnostics) and extended: wire structs pin the
public schema as Rust types in one place; `SerialEntry`/`SerialValue` are transport,
not schema. No hand-written shipped schema documents that can drift; the eventual
schema description (M6) is generated/reviewed from the wire structs.

**P2 — Instance, not lookup.** Serialization captures the object the parser actually
got; it never records "how to look this up again" at the core level. Lookups
(`retrieve_spec`, specials scanning) are parse-time events — token in hand,
environment of that moment — and nothing in their contracts makes them stable
addressing schemes. The canonical counterexample is `\today`: a provider may
legitimately answer differently later, or differently without the token. Every
resurrection mechanism must reproduce the *instance* (by identity in the reading
environment, or by constructor recipe), never re-run a query and hope.

**P3 — Type-blind foundation; everything is a registered object kind.** The engine
knows sessions, tables, `u32` indices, `SerialValue`, and one driver trait with
uniform method names. Core's own types (sources, states, specs, providers, trees,
diagnostics) are registered on it by the scaffolding layer with exactly the same
mechanism a framework would use. Method names never vary by object kind; the
foundation context exposes only generic interning, with type-specific sugar
(`cx.intern_spec`) as extension traits in the layer that is allowed to know.

**P4 — Serialization knowledge belongs to whoever owns the type.** What level of
detail resurrects a `Package` ("the name is enough") vs. a `\newcommand` spec ("the
constructor arguments") vs. a framework spec backed by a dynamically loaded class —
only the type's author knows. Core supplies machinery (tables, references, sessions,
policies) and never interprets anyone's payload. This is also the trust boundary:
dynamic definition loading happens inside a framework's own resolver for its own
namespace, under its own policy — core never loads anything.

**P5 — Reads are fail-closed total validation.** Wire data is untrusted input:
every index bounds-checked, every invariant re-established (regions re-resolved
builder-style, parent tables recomputed), digests verified, unknown identifiers and
missing environment objects are clean typed errors naming the culprit. No panics on
any input (panic policy rule 3 applies in full). A corrupt cache degrades to a
reparse, never to silently wrong offsets or a half-alive object.

**P6 — Wire identity is chosen vocabulary.** Identifiers, payload keys, table names
follow [§dd-dr:wire-identifier-stability]: deliberately chosen strings, hard-stable,
never Rust type names or accidental field identifiers. Whoever mints a name owns its
stability (core for core vocabulary, frameworks for theirs).

**P7 — Canonical-form discipline.** The frozen public contract is the canonical JSON
rendering, so `SerialValue` contains nothing the canonical rendering cannot
round-trip distinguishably. This single principle decided: no floats, no sized-int
variants (they all render as the same JSON number → unequal values with identical
bytes → equality/dedup/golden-file poison), string-keyed maps, and a pinned base64
form for `Bytes`.

**P8 — Capability intrinsic, dependency optional; additivity is sacred.** The value
tree, contexts, engine, and capability traits are dependency-free plain Rust and
exist unconditionally; the `serde` cargo feature gates only rendering. Enabling a
cargo feature must never add obligations to a crate that didn't opt in — this killed
every design where the feature changed trait surfaces.

**P9 — Deterministic emission.** Insertion/traversal order everywhere; hash-map
iteration order never reaches the wire. Required by golden files and
content-addressed caches; enforced per driver.

**P10 — Sharing is semantic and must survive.** `SourceSpan` equality is
Arc-identity-based; states are memoized and shared; specs are shared between nodes
and providers. Interning tables (write: ptr→index; read: index→Arc) exist to
preserve identity relationships exactly, including across trees in one stream.

**P11 — Process-local values are never wire material.** Tree tags (existing law,
[§dd-dr:tree-tags]), `NodeId`s, `Weak` provenance stamps, Arc addresses. Fresh tags
are minted on read; consumers needing durable node identity use annotations.

**P12 — Words are scoped.** "document" belongs to FLM; "construct" belongs to
`core::constructs`; `…Ref` means borrowing handle (`NodeRef`), `…Id` means
process-local identity (`NodeId`) — so wire positions are `…Index`. Public API and
rustdoc use the serialize/deserialize verb pair only; the session shorthand
(dump/load/revive/resurrect) never appears in shipped text.

---

## 2. The exploration arc

A chronological account of how the design moved, focusing on turning points. Each
dead end below is also in the quick catalog (§3).

### 2.1 What the live model forbids (initial survey)

The opening survey established the constraints everything else obeys:

- **Trait objects are load-bearing everywhere**: `CallableData::spec:
  Arc<dyn CallableSpec>`, `ArgumentSpec::parser: Arc<dyn ArgumentParser>`, every
  node's state → `ScopeStack` → `Arc<dyn SpecsProvider>`. Direct serde derives on
  live types are impossible, not merely undesirable.
- **Arc sharing is semantic** (P10), so naive per-node emission would both explode
  size (each node reaches a full state and source) and break identity on read.
- **Tags are already law**: tree.rs documents tags as process-local, never wire.
- **Interior mutability is essentially absent** — a snapshot serializer is sound.
- **Borrowed views** (`NodeRef`, `NodeSlice`, `LineIndex`, `CallableQuery`) can never
  serialize; you serialize the owning tree.
- The crate had **no cargo features at all** yet; [§dd-dr:dependencies] had already
  earmarked serde as the first optional one.

First-order conclusions that never changed: explicit wire model, never derives on
live types (D3); interning tables; drop-and-remint tags; validating reader.

### 2.2 First shape: wire structs + symbolic references

The first proposed architecture was a flat "snapshot document": interning tables +
wire structs + a writer and a validating reader, with specs referenced *symbolically*
(callable type + name) and re-resolved on read through the rehydrated scope stack.
The wire structs, tables, segments-to-be, and the validation stance all survived to
the final design. The symbolic re-resolution did not — it became the longest saga of
the sessions (§2.4).

### 2.3 Use cases fix the requirements

The six-use-case canon (plan §1) was fixed early and did real work:

- The **tooling cache** forced: reference-mode sources with digests, standalone
  state serialization (cache keying includes the initial state), read-then-append
  sessions, fail-closed corrupt-cache behavior, and binary-format viability.
- **Full dump/load (FLM)** forced: embedded sources, version field, long-lived
  compatibility posture.
- **IPC** forced: multi-tree roots with shared tables, batching, and surfaced the
  public-schema question early ("is the peer always Rust?").
- **Golden files** forced determinism (P9) and the readable canonical rendering.
- The user then set the schema posture: public, symbolic names, aims to freeze at
  v1 — which promoted every wire name to chosen vocabulary (P6) and banned
  representation choices that don't survive JSON (P7).

The "shared information between writer and reader" question produced the
embed-vs-reference axis per table and the caller-supplied digest (techy neither
picks nor implements a hash — P4 applied to hashing).

Two early ideas dissolved here: a "structure-only mode" omitting states (no real
use case once states proved cheap — they're interned and few; and its state-query
footgun was real), and with it my sloppy term "zero-environment" (retracted: the
lang and its machinery are always needed; only the *spec registry* was optional in
that mode).

### 2.4 The spec resurrection saga

The central question: given `Arc<dyn CallableSpec>` in a tree, what goes on the wire
such that the reading process reliably gets the same (or an equivalent) instance?
Seven successive mechanisms:

1. **Symbolic name + scope-stack re-query on read.** Rejected by the user as
   fragile: `retrieve_spec` takes a token and a state; nothing guarantees a later,
   token-less query returns the same spec.
2. **Writer-derived provenance via `iter_symbols` reverse maps** ("which interned
   provider owns this Arc?"). Rejected: enumeration is a tooling interface, not a
   lookup contract; providers can synthesize specs no enumeration can see; and a
   writer-side recomputation can silently disagree with what happened at parse time.
3. **Parser-recorded resolution provenance** (the parser stamps "found in provider
   P" on the node at lookup time). Considered seriously — it records the truth at
   the moment it is known — but rejected: it doesn't fix the actual problem, because
   the *read-time* query still runs later and token-less; it adds hot-path
   bookkeeping and node payload for one consumer; and once write-time verification
   exists the record is redundant.
4. **Verified write-time replay** (my proposal: the writer re-runs the lookup
   in-process, pointer-compares, and only then writes an "ask provider P for name N"
   recipe). Killed by the user's `\today` argument, which crystallized P2: a recipe
   *verified today* is still a recipe *executed later*; for any time- or
   context-dependent provider it resurrects "what the environment would say now,"
   not "what was parsed." No write-time check can promise read-time behavior.
   → **Instance-not-lookup became a governing principle**, and every query-shaped
   mechanism (including an instrumented `retrieve_spec`) left the design.
5. **The known-objects map** (caller supplies eager bidirectional Arc↔key tables for
   packages and their specs; writer emits keys). Architecturally sound — it is
   flmdump's `$flmenv` mechanism generalized — but rejected on economics: O(environment)
   registration (thousands of specs) to serve O(stream) need (dozens used), plus a
   composed-string key format when structured payloads are strictly better.
6. **Hook-owned serialization with closure registries** (`register(identifier,
   ser_fn, de_fn)` on dispatching drivers). The *ownership* idea (P4) was right and
   survived; the closure interface was rejected as an ad-hoc second API. Interim
   replacement: a `SerdeObject` trait implemented by concrete types +
   `register_type::<C>()` generating downcast shims — declarative, but still
   requiring write-side registration and per-framework `register_all()` batching.
7. **Self-description via the core traits** (final). The user found `register_all()`
   ugly and reopened the door to methods on core traits, DiagnosticValue-style. The
   earlier prohibition dissolved once its two supports were removed: the original
   objection had been to *type-dependent method names* (uniform `serialize_object`
   satisfies the layering principle), and the cfg-gating awkwardness vanished with
   the realization that **`SerialValue` and the whole engine need no serde
   dependency** (P8) — so the methods can exist unconditionally. Three shapes were
   compared: (A) defaulted methods duplicated on each core trait; (B) a capability
   accessor returning `Option<&dyn …>`; (C) a supertrait. C won: single canonical
   trait, uniform-by-construction names, upcast-based shared machinery (trait
   upcasting stabilized exactly at the MSRV, 1.86), and self-documenting one-line
   stubs; B collapses into an indirect A because the spec-specific argument-pair
   methods must sit on `CallableSpec` anyway.

Write dispatch therefore ended registration-free (the vtable is the dispatch), and
the irreducible read-side registry (identifier → constructor — unavoidable in any
language) is packaged as one namespace resolver per framework.

### 2.5 The gating saga

Three attempts to make the serialization surface conditional, two impossible, one
successful:

1. **Cargo-feature-gating the supertrait or the trait definition** (trivial trait +
   blanket impl when off; supertrait under `#[cfg]`): both violate **feature
   additivity** — crate A implements `CallableSpec` without the feature; crate B
   anywhere in the graph enables it; crate A stops compiling. Any design where a
   feature adds obligations is dead on arrival.
2. **A `SerializableObjects` LangFeature gating the supertrait**: not expressible —
   supertrait lists are fixed at trait definition (no "associated traits" in Rust),
   and the blanket-impl-for-absent-langs workaround fails coherence (stable overlap
   checking does not use associated-type-equality where-clauses for disjointness).
   Also conceptually misplaced: `FeaturePresence` pays off when *data layouts*
   change with presence (`Store<T>` → ZST), and serialization stores no lang-carried
   data anywhere.
3. **The successful form**: unconditional supertrait + **method-level
   `where L: SerializableLang`** + unconstructible contexts. For a non-serializable
   lang the method is statically uncallable (bound in the signature, shown by
   rustdoc) *and* unreachable (no `SerializeContext<L>` value can exist, because
   sessions require the bound) — vacuous by type and by construction. One diligence
   item survives: an M0 compile test for vacant vtable entries
   (`dyn CallableSpec<L>` with the bound unsatisfied) at MSRV 1.86, with a zero-risk
   fallback (drop the where-clause, keep unconstructibility).

### 2.6 Dynamic type universes

The user's requirement: an identifier like `"mycustom.packagexyz.yyy"` — a symbol in
a custom package for a future framework — must be resolvable by *loading* the
definition (e.g. a Python class via bindings) at read time. Design: read dispatch is
a chain — exact map → **namespace resolvers** → fail-closed error. A resolver owns a
prefix, does its own loading under its own trust policy, and returns the same erased
entry currency the exact map holds; the driver memoizes per identifier. This is
flmdump's `_import_class` made safe: core never loads anything; wire data can only
reach a resolver that explicitly claimed its namespace. The write side needed one
adjustment (per-instance identifiers for adapter types — hence `SerialEntry`'s
`Cow`), and the symmetric idea — **write-side resolvers** — was examined and
rejected: write dispatch is per Rust type, the process's type set is compile-time
closed, and a write resolver could only try downcasts against types it already knows
— a registration list in disguise. (The scenario that motivated it — "tons of
packages to register" — was a per-instance/per-type confusion: a thousand packages
are one `Package<L>` type, and instance identity flows through payload data.)

### 2.7 Sessions and segments

Originally separate writer/reader objects. The user's cache scenario (read
yesterday's segments, append today's trees) showed the read-side and write-side maps
must be the *same tables* — so one `SerdeSession` owns tables + both direction maps,
with reading and writing as method groups. Segments were designed as the top-level
unit from the start of the incremental discussion: new-entries-only, append-only
stream-scoped indices, a stream = ≥1 segments (JSONL canonical), one-shot dump =
single segment. "Document" was banned mid-session when FLM's ownership of the word
was pointed out.

### 2.8 The value model

`DiagnosticValue` was the precedent (own value tree because `serde::Serialize` is
not dyn-compatible and erased-serde is a dependency). `SerialValue` is deliberately
separate (diagnostics must not carry table references; independent evolution).
Decisions in sequence: no floats (P7 + no use case — hook authors encode numerics
exactly); per-kind ref variants (`SpecRef`, `SourceRef`, …) collapsed into a single
`Index{table, index}` once the typed-index mechanism moved to the drivers
(`ObjectSerdeDriver::SerializedIndex`, per the user's suggestion — foundation stays
type-blind, Rust-level type safety preserved); sized ints rejected (P7 — they cannot
round-trip through canonical JSON); `Bytes` added late for digests and opaque
payloads (base64 canonical form pinned at Q3). The bridge (serde
Serializer/Deserializer over `SerialValue`, the serde_json::Value pattern) replaced
a planned `ToSerialValue` public derive at the user's suggestion — implementers get
full serde ergonomics; policy enforcement (floats, keys, overflow) became mechanical
at the conversion boundary. The internal derive then *returned* in a different role:
core wire-struct conversions must be unconditional (P8: sessions work featureless),
so they use a techy-derive internal derive on the `ToDiagnosticValue` precedent.

### 2.9 `ParsedArgument::spec`

The user anticipated the i-th pointer-equality rule (parsed argument i's
`ArgumentSpec` Arc is a clone of `spec.arguments()[i]`), then spotted the hole: a
fully custom `CallableSpec` may attach argument specs from anywhere. Resolution: the
default writes only the index (pointer-checked at write, bound-checked at read —
environment drift becomes a clean load error at the node); custom specs override a
defaulted method pair on `CallableSpec` itself — legitimate home, since the spec is
the authority that produced those arguments via `make_invocation_parser`. A pleasant
consequence: `ArgumentSpec`s (and the `ParsingStateDelta`s inside them) never hit
the wire at all.

### 2.10 Provenance and the two cycle domains

For package-owned specs, the hook needs to know its package. Name-stamping fails (no
universal rule that a name identifies an Arc — the user's point); Arc-stamping
creates a true ownership cycle (Package strongly holds specs) and leaks. Resolution:
`provenance: Option<Weak<dyn SpecsProvider<L>>>` stamped at construction via
`Arc::new_cyclic`; `Option` because on-the-fly specs have none; upgrade failure is
an honest hook error (the `Result` signature was the user's correction — hooks fail
for many reasons, e.g. "only preset-builtin packages supported"). The analysis split
cycles into two domains that must not be conflated: the **live strong-Arc graph**
(kept acyclic by `Weak` back-edges) and the **wire reference graph** (must be
acyclic because read-side materialization of immutable values cannot tie knots) —
handled by the pairing convention (identity providers ↔ provenance payloads;
full-dump providers ↔ self-contained recipes), a writer cycle check, and reader
guards.

### 2.11 Prior art: flmdump.py

Reviewed at the user's request (558 lines; FLM's Python round-trip serializer).
Adopted, in our idiom: resource tables with reference pointers (theirs keyed by
`id(obj)` — ours use dense deterministic indices); ambient-environment references
(`$flmenv` — generalized into identity payloads + `cx.user_data()`);
environment-as-factory reconstruction; hard version check; distinguishing "not
serialized" from "was None". Rejected as anti-patterns: reflection-based field dumps
(no schema anywhere — renames silently change the format); dynamic class import
named by wire data (`module:Class` → importlib — a code-execution surface; our
resolvers are the safe form); half-alive loaded objects (a parsing state whose spec
database is a sentinel *class* that explodes far from the load site — our reads are
total or fail); and a live drift bug (the dumper writes `{"$flmenv":
"environment"}`, the loader accepts only `''` — write/read as hand-maintained
mirrors with no shared schema), which mandated round-trip property tests and golden
files from day one.

### 2.12 The naming journey

Settled through several rounds, ending in the three-family rule (plan §3.G):
`Serde*` strictly for bidirectional machinery, `Serializable*`/`Deserializable*` for
one-directional capabilities, `Serial*` for wire data. Along the way: "Codec"
rejected by the user (text-encoding connotation) in favor of `…SerdeDriver`
(matching the house "Driver" precedent: `ParseDriver`, `LatexlikeDriver`);
`SerdeObject` renamed `SerializableObject` when its one-directionality made "Serde"
a false promise; `…Index` chosen over `…Ref`/`…Id` because both are taken with
conflicting semantics (P12); `Cow<'static, str>` over `Arc<str>`/`&str` for
identifiers (zero-cost static common path + owned escape hatch; `Arc<str>` allocates
for literals to optimize cloning that never happens; borrowed-from-self would infect
table lifetimes); "segment"/"stream" replacing "document" everywhere including
prose; sessions named `SerdeSession` (correctly bidirectional). Public wire strings
remain Q3.

---

## 3. Rejected patterns — quick catalog

| Pattern | Why rejected | Principle |
|---|---|---|
| serde derives on live types | trait objects; Arc semantics; schema-refactor coupling | P1, P10 |
| erased-serde / typetag | dependency; link-time global registries; Rust type names as wire identity | P6, P8 |
| Symbolic re-query via scope stack | read-time query ≠ parse-time lookup | P2 |
| `iter_symbols` reverse maps | enumeration is not a lookup contract; recomputation can disagree with the parse | P2, P4 |
| Parser-recorded provenance | records parse-time truth; cannot promise read-time behavior; hot-path cost | P2 |
| Verified write-time replay | verification today ≠ validity later (`\today`) | P2 |
| Known-objects map | O(environment) setup for O(stream) need; composed string keys | P4 |
| Closure registries (`ser_fn`/`de_fn`) | ad-hoc second interface; behavior belongs on the type | P4 |
| `register_type` + `register_all()` for writing | registration burden with no capability gain once objects self-describe | P4, P8 |
| Capability accessor (`Option<&dyn …>`) | collapses into an indirect version of trait methods; the spec-specific pair needs `CallableSpec` anyway | — |
| Cargo-gated trait surface (any form) | feature additivity violation (two-crate scenario) | P8 |
| `SerializableObjects` LangFeature | conditional supertraits inexpressible; coherence blocks blanket disjointness; no data-layout change to gate | P8 |
| Write-side resolvers | closed compile-time type set; downcast lists are registries in disguise | — |
| `Detached` wire variant + evidence in core | placeholder semantics belong to implementers' factories, documented in their vocabulary | P4, P5 |
| Floats / sized ints in `SerialValue` | cannot round-trip through canonical JSON | P7 |
| Structure-only mode (omit states) | no surviving use case; state-query footgun | — |
| "No spans" mode | `SourceSpan` mandatory per node; reference-mode sources already keep text out of files | — |
| Public `ToSerialValue` derive | serde bridge gives full ecosystem ergonomics instead (internal derive kept for core wire structs) | P8 |
| `Document` type / the word "document" | FLM owns the word | P12 |
| Reflection field-dumping, `id()` keys, wire-named dynamic imports (flmdump) | no schema; nondeterminism; code-execution surface | P1, P5, P9 |

Recorded reversals (assistant positions corrected during the sessions, kept for
honesty): "no serialization methods on domain objects" (superseded — the real
objection was type-varying names and cfg-gating, both dissolved); "write-side
self-description is an asymmetry footgun" (withdrawn — write-anywhere /
read-needs-environment is serialization's inherent shape, mitigated by fail-closed
reads); the `ToSerialValue` public derive (replaced by the bridge, then revived
internally for core wire structs).

---

## 4. Why the pillars hold (convergence rationale)

- **Supertrait write half (D13)** is the unique intersection of: dyn dispatch needed
  (unknown concrete types inside trees), uniform names (P3), no registration (user
  requirement), no cfg-gating (P8), overridability (kills blanket impls). Options A/B
  and every registry variant each violate at least one.
- **Registry/resolver read half (D14/D15)** is irreducible: identifier → constructor
  needs a map in any language; the design choices were only *what an entry is*
  (a type's own impl, not a closure — P4) and *how frameworks package entries*
  (one namespace resolver — ergonomics + the dynamic-loading requirement).
- **`SerializableLang` + where-clause gating (D17)** is the only expressible
  per-lang gate (2.5); it also subsumes what the LangFeature would have said.
- **Session unification (D9)** follows from the cache use case alone; everything
  else (segments, append-only indices, determinism) follows from use cases 1/3/4.
- **Instance-not-lookup (D18)** is the load-bearing philosophical commitment; if it
  is ever relaxed, most of §2.4's dead ends come back to life. Don't.
- **Weak provenance (D19)** is forced by: hooks need write-side identity (P4), names
  don't identify Arcs, strong Arcs leak, interior mutability is banned — `Weak` +
  `new_cyclic` is the remaining point in that constraint space.

---

## 5. Lessons worth keeping (meta)

1. **Verified-at-write ≠ valid-at-read.** Any mechanism that re-executes a query
   later must be judged at its *execution* time, not its verification time.
2. **Enumeration interfaces are not contracts.** If a trait's purpose is query, do
   not build correctness on its iteration convenience.
3. **Feature additivity kills conditional trait surfaces.** If a cargo feature would
   change what implementers must write, the design is wrong; make capability
   intrinsic and gate the dependency-bearing rim instead.
4. **The canonical rendering disciplines the value model.** Ask of every variant:
   does it survive a round-trip through the frozen rendering distinguishably?
5. **Per-type vs per-instance confusion generates phantom requirements.** "Tons of
   packages" looked like a registration burden until dispatch-by-type vs
   identity-by-data were separated.
6. **Two cycle domains.** Object-graph cycles (fix with `Weak`) and wire-reference
   cycles (fix with conventions + checks) are different problems; solving one does
   not solve the other.
7. **Hand-maintained write/read mirrors drift** (flmdump's `$flmenv` bug). Schema in
   one place (wire structs) + round-trip tests from day one.
8. **Name families do design work.** `Serde*`/`Serializable*`/`Serial*` and
   `Ref`/`Id`/`Index` carry real semantic distinctions; policing them prevents
   category errors, not just aesthetic ones.
