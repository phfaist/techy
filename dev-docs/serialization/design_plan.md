# Serialization — design plan

**STATUS: TRANSIENT WORKING DOCUMENT.** This folder (`dev-docs/serialization/`) exists
only for the duration of the serialization project. It is the source of truth for the
design and the implementation plan while the project runs. When the project completes,
the durable outcome is recorded as DESIGN_RATIONALE entries + ARCHITECTURE sections (see
M7), and this folder is deleted. Do not cite this file from permanent docs.

DESIGN_RATIONALE.md and ARCHITECTURE.md must NOT be touched before M7.

Design converged in interactive user sessions (2026-08-13). Decisions below are settled
unless marked OPEN. Settled decisions may be revised in light of new evidence (e.g. an
obstacle the design didn't anticipate) — record the revision and its reason in §3 and
the progress log; escalate to the user in case of doubt. Agent sessions must not
silently deviate from a D-number. The design conversations behind this plan are NOT
required reading — §3 is the complete decision record; if §3 seems ambiguous, that is
a plan bug: fix the plan, escalating to the user if the ambiguity is substantive.
A non-normative companion, `design_session_report.md` (same folder), records the
philosophies, rejected patterns, and false routes behind these decisions — read it
before proposing any design change, so dead ends are not re-walked.

---

## 1. Mission

Provide serialization/deserialization for the objects techy consumers handle — node
trees (with annotations), parsing states, sources, specs/providers, diagnostics — via
serde, behind an optional `serde` cargo feature (which gates the serde dependency and
rendering only — the capability itself is unconditional, D1), with a schema that will
become public and frozen at v1 (not frozen yet).

### Canonical use cases (the design must serve all six)

1. **Tooling cache** — chunks/documents parsed and cached on disk; sources exist
   externally (reference + digest); same tool version both ends; binary format; corrupt
   cache degrades to reparse (reader = total validator, never panics). Read-then-append
   sessions are first-class (absorb yesterday's segments, emit today's).
2. **Full dump/load** — e.g. FLM serializes a full parse result to render later;
   self-contained (embedded sources); long-lived; version field + compatibility policy.
3. **IPC** — parser/formatter service feeding e.g. a website builder; batches of trees
   sharing tables; short-lived payloads.
4. **Golden-file / snapshot tests** — readable, diffable, deterministic output.
5. **Debug/inspection dumps** — write-only, human-readable.
6. **Cross-language consumers** — deferred; motivates the public-schema policy but is
   not a v1 requirement.

---

## 2. Architecture: three strata over one data path

```
┌───────────────────────────────────────────────────────────────────────┐
│ presets & frameworks:  SerializableLang impls; SerializableObject /  │
│   DeserializableObject impls for their types; one namespace resolver │
│   per framework; vocabulary; provenance-stamping package builders    │
├───────────────────────────────────────────────────────────────────────┤
│ core scaffolding:  drivers for the standard tables (sources, states, │
│   specs, providers, trees, diagnostics); wire structs; context       │
│   extension traits (cx.intern_spec(…) sugar); typed indices          │
├───────────────────────────────────────────────────────────────────────┤
│ foundation (type-blind):  SerialValue, SerialEntry, SerialIndex,     │
│   SerializableObject, DeserializableObject, ObjectSerdeDriver,       │
│   SerdeSession, contexts, segments, errors, generic interning        │
└───────────────────────────────────────────────────────────────────────┘
```

The foundation never names sources, states, or specs; every object kind — core's
included — is registered on it identically. Core traits reference foundation types, so
the foundation is the lower stratum in the reference graph. The foundation context
exposes only generic interning (`cx.intern(table_handle, arc) -> Index`); spec-aware
sugar (`cx.intern_spec`, `cx.intern_provider`) is an extension trait in the scaffolding
layer.

Data path (write; read is the reverse with validation staged at each rim):

```
live object ──SerializableObject impl──▶ wire struct ──▶ SerialValue ──▶ table entry
              (WireState, WireNode, …)                    ({id,data} or bare data)
        ──▶ Segment ──#[cfg(feature="serde")]──▶ serde_json / postcard / …
```

---

## 3. Decision register

### A. Dependency, gating & schema policy

- **D1 — The `serde` cargo feature gates RENDERING ONLY.** `SerialValue`, `SerialEntry`,
  contexts, the engine, and all capability traits/methods are unconditional,
  dependency-free plain Rust (`no_std`+`alloc`). The feature gates: `Serialize`/
  `Deserialize` impls (for `Segment`, `SerialValue`, `DiagnosticValue`), the
  `to_value`/`from_value` bridge, and serde derives on vocabulary/payload types.
  Dependency: `serde = { version = "1", optional = true, default-features = false,
  features = ["alloc", "derive"] }`. Consequence: a session can produce in-memory
  `Segment`s without the feature; only encoding to a format needs it. Feature
  additivity is preserved because enabling the feature adds zero obligations to any
  implementer. techy depends only on serde; format crates are the consumer's choice.
- **D2 — Public schema, frozen at v1 (not yet).** The public contract is the abstract
  structure + its canonical JSON rendering. Symbolic names everywhere (explicit serde
  renames chosen deliberately, never Rust identifiers by accident); binary encodings
  are private same-version pairings. Until freeze: version field, breaking changes
  allowed. Identifier/table-name/payload-key stability follows
  [§dd-dr:wire-identifier-stability]; implementer payload keys are the implementer's
  stability obligation.
- **D3 — No serde derives on live model types.** Live types never gain serde bounds;
  serialization goes through the wire model. Serde derives are allowed on:
  `SerialValue`/`Segment` (feature-gated impls), index newtypes, vocabulary types,
  and implementer payload structs. Core wire structs do NOT carry serde derives —
  their only conversion mechanism is the internal derive (D8); `Segment` alone is
  the public wire type with serde impls.
- **D4 — Vocabulary: "segment" and "stream"; the word "document" is banned** (in API,
  rustdoc, and prose — FLM owns it). A stream is a sequence of ≥1 segments; indices
  are stream-scoped. "resurrect/dump/load/revive" are banned from public API and
  rustdoc; the verb discipline is serialize/deserialize.

### B. The value model

- **D5 — `SerialValue`** (foundation, unconditional, separate from `DiagnosticValue`):
  `Null | Bool(bool) | Int(i64) | Str(String) | Bytes(Vec<u8>) | List(Vec<_>) |
  Map(Vec<(String, _)>) | Index { table: TableId, index: u32 }`. Map is
  order-preserving with string keys. **No floats** and no sized-int variants: variants
  that collapse in the canonical JSON rendering cannot round-trip distinguishably and
  poison equality/dedup/golden files; the bridge maps every Rust int width onto `Int`
  with range checks. `Bytes` renders as base64 in JSON (exact canonical form: Q3).
  `DiagnosticValue` is unchanged (it gains only a feature-gated `Serialize` impl).
  `TableId` is a foundation ordinal newtype (`TableId(u32)`), assigned by the
  session in deterministic table-registration order; whether the JSON rendering of
  `Index` shows the ordinal or the table name is Q3 — the in-memory type is fixed
  either way, so M0 is not blocked on Q3.
- **D6 — `SerialEntry { identifier: Cow<'static, str>, data: SerialValue }`** — the
  in-memory return of every write. `Cow<'static, str>`: zero-cost for the static
  literal common case, owned escape hatch for instance-derived identifiers (dynamic
  adapter types); borrowed-from-self is impossible (entries outlive the call) and
  `Arc<str>` would allocate for literals to optimize cloning that never happens.
  Homogeneous table drivers omit the identifier from the wire (the table implies the
  type); heterogeneous drivers write `{identifier, data}`. Homogeneous core types
  still return a real constant identifier (e.g. `"core.state"`; exact strings Q3) —
  never an empty string — which their drivers may debug-assert and do not emit.
- **D7 — The bridge** (feature-gated): `SerialValue` implements serde's
  `Serializer`/`Deserializer` over itself (the `serde_json::Value` pattern), exposed
  as `to_value<T: Serialize>` / `from_value<T: DeserializeOwned>`. Index newtypes and
  byte payloads are intercepted by newtype-struct name so they map to the dedicated
  variants. The bridge enforces D5 policy mechanically (floats, non-string map keys,
  `u64` overflow → `SerialValueError`). Implementer payload structs use serde derives
  + explicit renames.
- **D8 — Core wire-struct conversions are unconditional** and therefore cannot use the
  bridge: a small internal derive in techy-derive (precedent: the existing
  `ToDiagnosticValue` derive) generates to/from-`SerialValue` conversions for core's
  wire structs. Two mechanisms coexist by stratum: internal derive for core wire
  structs; serde bridge for implementer payloads (their crates enable the feature).

### C. Foundation engine

- **D9 — `SerdeSession<L>`: one session type, read and write unified.** Holds the
  tables and BOTH direction maps (Arc-ptr → index for writing, index → Arc for
  reading), registrations (drivers, resolvers), and a caller user-data slot. Reading
  then appending is the natural flow (use case 1): absorb segments, add trees, emit a
  segment containing only new entries. Construction requires `L: SerializableLang`.
  Core ships a standard-tables constructor (sources, states, specs, providers, trees,
  diagnostics pre-wired); presets/frameworks extend it. `SerdeSession::<L>::new()` IS
  that standard-tables constructor (§6 usage); if M2 finds a need for empty/custom
  composition, that is a separate constructor to name then.
- **D10 — Segments.** The unit of emission contains only table entries new since the
  previous emission, plus the trees/diagnostics added in it. Indices are stream-scoped,
  append-only, assigned in deterministic insertion order (never hash order — `Package`
  internals are hashbrown; anything derived from them must be explicitly ordered —
  this determinism rule binds every driver, core and custom). Segment methods (provisional, finalized M2):
  `take_segment()` / `push_segment(...)`. JSONL is the canonical stream rendering;
  segments may equally be separate files/payloads consumed in order. `Segment` is the
  one public wire struct (feature-gated serde impls).
- **D11 — `ObjectSerdeDriver`: one per table, uniform method names**
  (`serialize_object` / `deserialize_object` — names never vary by object kind).
  Associated `type Index`: a `u32` newtype satisfying the `SerialIndex` bound
  (`Copy + Eq + Hash + Debug` + to/from-`u32` conversions; the bound trait's full
  item list is M2's to pin). (Audit fix: the earlier spelling `SerializedIndex`
  violated the §3.G naming families; the associated type is `Index`, its bound is
  `SerialIndex`.) The typed newtypes
  (`SourceIndex`/`StateIndex`/`SpecIndex`/`ProviderIndex`/`TreeIndex`) are defined by
  the SCAFFOLDING layer next to its drivers (M2/M3) — never in foundation `value.rs`,
  which stays type-blind and holds only `TableId`, the `SerialIndex` bound, and the
  wire `Index{table, index}` form. Homogeneous tables (sources, states, trees,
  diagnostics) implement the driver directly; trait-object tables
  (`dyn CallableSpec`, `dyn SpecsProvider`) instantiate a generic dispatching driver
  (write: call the object's own vtable method; read: identifier registry +
  resolvers). The driver's `serialize_object`/`deserialize_object` deliberately
  mirror the object-level method names — the same conceptual operation at two
  levels: a dispatching driver's implementation calls the object's own
  `SerializableObject::serialize_object`; homogeneous drivers do the work directly.
  Custom tables: implementers register additional drivers (framework-shared
  resources referenced from annotations/ext payloads); table names follow identifier
  stability discipline.
- **D12 — Cycle rules.** The live strong-Arc graph is acyclic by construction
  (back-edges are `Weak`, D19). The wire reference graph must also be acyclic
  (read-side materialization of immutable values cannot tie knots): pairing convention
  (identity-resolved providers ↔ provenance payloads on their specs; full-dumped
  providers ↔ self-contained recipe payloads), a writer cycle check per segment
  (error names both entries), and reader-side in-progress + recursion-depth guards
  (untrusted input).

### D. The capability traits (write and read halves)

- **D13 — `SerializableObject<L>`: the universal write half.**

  ```rust
  pub trait SerializableObject<L: Lang> {
      fn serialize_object(&self, cx: &mut SerializeContext<'_, L>)
          -> Result<SerialEntry, SerializeError>
      where
          L: SerializableLang,
      {
          Err(SerializeError::unsupported())
      }
  }
  ```

  Supertrait on `CallableSpec` and `SpecsProvider` (unconditionally — that puts the
  method in the vtable for type-erased dispatch; additivity-safe because everything is
  defaulted and dependency-free). Plain trait impls on concrete core types
  (`ParsingState`, `Source`, `NodeTree`, `Diagnostic`). Non-participating types owe a
  one-line stub impl (`impl<L: Lang> SerializableObject<L> for BeginSpec<L> {}`),
  greppable and self-documenting. **Write-side registration does not exist**; ALL
  writing goes through this method, called by each table's driver. A blanket impl is
  impossible (it would forbid overriding, by coherence).
- **D14 — `DeserializableObject<L: SerializableLang>`: the opt-in read half.**

  ```rust
  pub trait DeserializableObject<L: SerializableLang>: Sized {
      type Output;   // Self for recipes; Arc<dyn …> for identity resolution
      fn deserialize_object(value: &SerialValue, cx: &mut DeserializeContext<'_, L>)
          -> Result<Self::Output, DeserializeError>;
  }
  ```

  Implemented by concrete types only; **never a supertrait** — structurally impossible
  (`Sized` + associated type + receiverless constructor make it non-object-safe, which
  would destroy `dyn CallableSpec`) and unnecessary (reads dispatch on identifiers,
  not objects). `Output = Self` for recipe types; `Output = Arc<dyn SpecsProvider<L>>`
  for identity resolution (e.g. `Package` looks its name up in `cx.user_data()` and
  returns the caller's existing Arc). Non-participants implement nothing.
- **D15 — Read dispatch for heterogeneous tables: exact map → namespace resolvers →
  fail-closed error.** `register_type::<C: DeserializableObject<L>>()` feeds the exact
  map. A framework registers ONE resolver for its identifier prefix
  (`register_resolver("mycustom.", …)`); a resolver returns the same erased entry
  currency the exact map holds, and the driver memoizes it per identifier (expensive
  definition loading happens once per stream). The "entry currency" is a concrete
  foundation type — a **read entry**: an erased handle (working name `ReadEntry`;
  final name M2) wrapping one deserialize function for one identifier, produced
  EITHER by `register_type::<C>()` from `C`'s `DeserializableObject` impl OR
  constructed by a resolver (e.g. wrapping a dynamically loaded definition in an
  adapter type). §3.J's rejection of "closure-pair registries" targets hand-written
  `ser_fn`/`de_fn` pairs as the PUBLIC registration API, not this internal currency. Dynamic type loading (an identifier
  like `"mycustom.packagexyz.yyy"` → look up package, load symbol definition, build an
  adapter entry) lives entirely in resolvers: core never loads anything; each resolver
  enforces its own trust policy for its namespace; no resolvers registered = clean
  unknown-identifier error. Two sanctioned wirings, both supported: (i) static
  identifier + dynamic payload internals (PREFERRED default — one registered entry
  whose `deserialize_object` consults `cx.user_data()`); (ii) dynamic identifiers +
  resolver (escape hatch for genuinely open type universes; the write side then
  overrides the emitted identifier per instance via `SerialEntry`).
- **D16 — No write-side resolvers.** Write dispatch is per Rust type via the vtable;
  the process's type set is compile-time closed, and a write resolver could only try
  downcasts against types it already knows — a registration list in disguise. The
  entry-currency design keeps a write resolver chain purely additive if a concrete
  need ever materializes (none constructible today).
- **D17 — `SerializableLang`: the lang-level gate.** An unconditional trait (no cargo
  gating — gating it would break feature additivity). Supplies the lang's vocabulary
  and ext codecs in `SerialValue` terms (hand-written for the small closed enums —
  unconditional; the preset's serde derives on vocab types additionally exist under
  the feature for bridge/payload use). Bounds `SerdeSession::<L>` construction and
  appears as a method-level `where L: SerializableLang` on `serialize_object` and the
  D21 argument pair — for a non-serializable lang the methods are statically
  uncallable AND unreachable (no context value can be constructed): vacuous by type
  and by construction. **No `SerializableObjects` LangFeature**: conditional
  supertraits are inexpressible in Rust, coherence blocks disjoint blanket impls, and
  `FeaturePresence` pays off only when data layouts change with presence —
  serialization stores no lang-carried data. M0 carries a compile test pinning
  vacant-vtable behavior (`dyn CallableSpec<L>` for non-serializable `L`) at MSRV
  1.86; fallback if it ever failed: drop the where-clause, rely on context
  unconstructibility alone (same practical semantics). Sequencing: M0 lands
  `SerializableLang` as a bare marker (`pub trait SerializableLang: Lang {}`); M3
  adds its items — the codec surface for the lang's closed vocabulary types and its
  ext types (reached via the `Lang::NodeExts: NodeExtTypes` bundle and `StateExt` —
  see the state/lang.rs anchor), plus `InvocationSyntax`. Throughout this plan,
  "vocabulary types" means the lang's closed enums — its `CallableTypeId`/
  `GroupTypeId`/`ModeId`/`Event` types (the `ClosedVocabulary` impls; for latexlike:
  `CallableType`, `GroupType`, `MathGroupForm`, `Mode`, `Event`).

### E. Specs & providers

- **D18 — Instance-not-lookup principle.** Serialization captures the instance the
  parser got, never a lookup to re-run later. No core-level re-query, no write-time
  replay, no enumeration-based reverse maps (`iter_symbols` is tooling, not a lookup
  contract; `retrieve_spec` is a parse-time event — think `\today`, whose answer may
  differ later and without the token).
- **D19 — Provenance stamp.** Concrete spec types (core `StdCallableSpec`; latexlike
  `MacroSpec`, `EnvironmentSpec`, `SpecialsSpec`, …) carry
  `provenance: Option<Weak<dyn SpecsProvider<L>>>` — a field, not a trait item.
  `Option` because on-the-fly specs (`\newcommand`-minted) have none; `Weak` because a
  strong Arc would close an ownership cycle with the package's strong spec Arcs
  (leak); stamped at construction inside `Arc::new_cyclic` (builder API: Q5;
  `Weak<Concrete>` coerces to `Weak<dyn SpecsProvider<L>>`). Process-local like tree
  tags — never wire material; it feeds `serialize_object` (identity data +
  `cx.intern_provider`). Upgrade failure → error or the implementer's recipe fallback.
  Records the constructing provider (correct even when the Arc is reachable via
  others).
- **D20 — Owner-decided granularity; the standard latexlike split.** Packages emit
  identity payloads (`{provider: ProviderIndex, ct, name}`-shaped, via provenance) and
  identity-resolve on read against the caller's environment. `Scope` (dynamic
  `\newcommand` definitions) full-dumps: definitions as `SpecIndex` refs to recipe
  entries. `\newcommand` specs serialize constructor recipes (`{args, opt_default,
  body}`), NOT internals: **no serialization on `ArgumentParser` or
  `EnvironmentBehavior`** — factories rebuild specs through their constructors, which
  re-create parsers/behaviors internally.
- **D21 — `ParsedArgument::spec`: index rule + spec-level hook pair.** Default: the
  i-th parsed argument's `ArgumentSpec` Arc must be pointer-equal to
  `spec.arguments()[i]`; the wire stores only the index; revive =
  `arguments()[i].clone()`, bound-checked (environment drift → clean load error at the
  node). Custom callable specs override a defaulted, `SerializableLang`-bounded method
  pair on `CallableSpec` itself (spec-specific, so that is its legitimate home):
  `serialize_argument_spec(index, &Arc<ArgumentSpec<L>>, cx) ->
  Result<Option<SerialValue>, _>` and `deserialize_argument_spec(index,
  Option<&SerialValue>, cx) -> Result<Arc<ArgumentSpec<L>>, _>` (the latter runs on
  the freshly materialized spec). The DEFAULT BODIES implement the rule themselves,
  not the tree driver: the write default pointer-compares against
  `self.arguments().get(index)` and returns `Ok(None)` on match / the out-of-band
  error on mismatch; the read default is `self.arguments().get(index).cloned()`
  with a bounds error. `index: usize` in signatures (the wire stores `u32`).
  Out-of-band argument specs without an override = v1 write error naming node and
  callable. `ParsedSlot` is structural — no hook.

### F. Trees, states, sources, diagnostics

- **D22 — Tree tags are never wire material** (existing law [§dd-dr:tree-tags],
  tree.rs doc). Drop on write; the reader mints a fresh tag via the normal path and
  restamps `RegionState::Resolved.tree_tag`. The wire stores regions in staged-like
  form — the FULL staged information: child ranges AND the content designation (the
  `ContentNodes`-equivalent; exact wire shape is M4 design inside this rule); the
  reader re-runs builder-style resolution so region invariants are re-established by
  construction. Parent table and `single_source`
  recomputed, never trusted. Consumer `NodeId`s do not survive round-trips (durable
  node identity rides in annotations). Everything read is untrusted: bounds checks,
  invariant validation, typed errors, no panics (panic policy rule 3 in full).
- **D23 — Annotations & ext serialize with context.** `NodeTree<L, A>` annotations are
  often `SourceSpan`s (extract/transform mint them), so `A` payloads may need
  `cx.intern_*` — a plain `A: Serialize` bound can never suffice. Mechanism: per-call
  or session-registered annotation codec (TypeId-keyed), with a serde-bridge default
  for plain-data `A`; lang ext types (NodeExt/ArgumentExt/SlotExt/StateExt,
  InvocationSyntax) go through `SerializableLang`'s codecs. latexlike's
  `StdEnvironmentSideSyntax::name_group_rule` (`Arc<GroupRule>`) is inlined.
- **D24 — States: always serialized, interned.** Per state: token rules in full (rule
  payloads are small; all sections optional on the wire — feature-agnostic; reader
  errors if the target lang lacks a used feature), mode + ext via lang codecs, scope
  stack as `ProviderIndex` list. Derived caches (prefix tables, trigger chars) never
  hit the wire — the constructor rebuilds them. Standalone state serialization is
  first-class (cache keying). `ParsingStateDelta` never hits the wire (deltas live
  only inside `ArgumentSpec`s, which are not serialized — D21).
- **D25 — Sources: embed or reference.** Per source at write time: embedded text, or
  reference `{origin label, length, digest}` with `digest = {algorithm: String,
  bytes: Bytes}` — digest function and verifier are caller-supplied (techy neither
  picks nor implements a hash); digest optional per source. Read-side resolution via
  a caller resolver + verification; a changed file becomes a clean load failure, not
  silently wrong offsets. `TextContent::Spanned` stays offset-based (never force
  `materialize()`); provenance edges serialize as source refs (acyclic); spans are
  always kept — there is no "no spans" mode.
- **D26 — Diagnostics.** Wire = severity + identifier + `DiagnosticValue` data (the
  existing `serializable_data()` channel) + span + rendered trace frames. Revive as an
  adapter `DiagnosticData` keyed by identifier (anticipated by error.rs). Same
  stream/tables as the trees they belong to (shared sources). `DiagnosticValue` is a
  strict subset of `SerialValue` (no `Bytes`/`Index`): core provides an
  unconditional, infallible `DiagnosticValue → SerialValue` embedding used by this
  driver; the reverse conversion exists only inside this driver and rejects
  `Bytes`/`Index` inputs as validation errors. `ParseResult`
  (tree + diagnostics + session ext) gets a convenience wrapper (multi-tree root
  underneath).

### G. Naming (Q2 — RESOLVED)

Three families, applied strictly:

| Family | Meaning | Members |
|---|---|---|
| `Serde*` | genuinely bidirectional machinery | `SerdeSession`, `ObjectSerdeDriver`, dispatching drivers (`SpecSerdeDriver`, `ProviderSerdeDriver`) |
| `Serializable*` / `Deserializable*` | one-directional capabilities | `SerializableObject`, `DeserializableObject`, `SerializableLang` |
| `Serial*` | wire-side data | `SerialValue`, `SerialEntry`, `SerialIndex` (the bound trait) |

Typed table positions form a fourth, suffix-based family: `…Index` newtypes
(`SourceIndex`, `SpecIndex`, …) named by table, satisfying `SerialIndex`; the
driver's associated type is `type Index` (see D11 — `SerializedIndex` was an audit-
caught family violation and is superseded).

Also settled: contexts `SerializeContext`/`DeserializeContext` (house "…Context"
suffix per RestageContext/RecomposeContext/VisitContext); errors `SerializeError` /
`DeserializeError` / `SerialValueError`; bridge `to_value`/`from_value` (serde_json
precedent); emission unit `Segment`, sequence "stream"; typed indices use the
`…Index` suffix (`…Ref` means borrowing handles — `NodeRef`; `…Id` means
process-local identity — `NodeId`; a wire table position is neither); method names
`serialize_object`/`deserialize_object`/`serialize_argument_spec`/
`deserialize_argument_spec`; registration `register_type`/`register_resolver`.
Facade: `techy::serialize`. "Construct"-based names are off-limits
(`core::constructs` owns that word). Public wire field/enum strings: Q3.

### H. Errors

- **D27 — Failure surface.** Write: `SerializeError` — hook failures (default
  "unsupported by this type", or any implementer reason) wrapped by the writer with
  location context (callable name, node span, table index); wire-cycle detection;
  out-of-band argument specs. Read: `DeserializeError` — validation (bounds, shape,
  invariants, digest), unknown identifier (names the identifier), resolver/factory
  failures, missing environment objects (names the key). Bridge: `SerialValueError`.
  All `Result`, no panics on any input, including hostile segments.

### I. Preset / framework obligations (latexlike is the template)

- **D28** — A preset/framework owes exactly: (a) a `SerializableLang` impl (vocab +
  ext codecs); (b) `SerializableObject` impls for its participating spec/provider
  types + one-line stubs for the rest; (c) `DeserializableObject` impls for the same;
  (d) ONE namespace resolver + a `register(&mut session)` helper (which chains its
  dependencies' helpers); (e) the provenance-stamping package builder; (f) serde
  derives + explicit renames on its vocabulary types (feature-gated, for
  bridge/payload use). Environment nodes need nothing special — they are core
  `Callable` nodes (`callable_type == Environment`).

### J. Rejected designs & recorded reversals (do not reintroduce)

- **Rejected:** erased-serde/typetag (dependency; link-time global registries; Rust
  type names as wire identity). Core-level lookup resurrection in any form —
  symbolic re-query, verified write-time replay, parser-recorded resolution
  provenance (all fall to D18; replay validates today's answer, not read-time's).
  Enumeration-based reverse maps over `iter_symbols`. The eager "known-objects map"
  (O(environment) setup for O(stream) need). Closure-pair registries (`ser_fn`/
  `de_fn`). A `Detached` wire variant + evidence machinery in core (implementers
  build placeholder semantics into their own factories if they want them). Write-side
  `register_all()` batch registration (superseded by D13). `SerializableObjects` as a
  LangFeature (D17). Floats and sized ints in `SerialValue` (D5). Supertrait gating
  by cargo feature or lang feature (additivity / expressibility — D17). Borrowed or
  `Arc<str>` identifiers (D6). A public `Document` type or the word "document" (D4).
- **Reversals (recorded for honesty):** (a) "no serialization methods on domain
  objects" → superseded by the uniform-name supertrait once names were uniform and
  availability unconditional; (b) "write-side self-description is an asymmetry
  footgun" → withdrawn: write-anywhere/read-needs-environment is serialization's
  inherent shape, mitigated by fail-closed reads; (c) a public `ToSerialValue` derive
  for implementer payloads → replaced by the serde bridge (D7), while an *internal*
  derive returns for core wire structs (D8).

---

## 4. Open questions

- **Q3 (M6) — Wire vocabulary naming pass** (freeze-relevant, pre-v1): every public
  field name and enum string (core + latexlike), the `Index` table discriminant
  rendering (name string vs ordinal), the canonical base64 form for `Bytes`.
- **Q5 (M5) — Package-builder API shape** for provenance stamping (`new_cyclic`
  threading; whether core `Package` construction changes or only the latexlike
  builder).
- **Q6 (M2) — Segment/stream container details**: version placement (first segment
  only?), JSONL conventions, end-of-stream marker or not, `take_segment`/
  `push_segment` final names.
- **Q7 (M6) — Read-side verification levels**: which optional sanity checks (e.g.
  argument-count evidence) are worth their wire bytes; bounds checks are the D21
  baseline.
- (Q1 resolved: uniform `SerialValue`-mediated entries for v1 — rendering-identical,
  so a typed fast path stays a schema-invisible later optimization. Q2 resolved: §3.G.
  Q4 dissolved: no cfg-gated trait methods exist in the design.)

---

## 5. Illustrative segment sketch (JSON rendering; names NOT final — Q3)

```json
{
  "version": 1,
  "sources": [
    { "text": "Hello \\emph{world}.", "origin": "intro.tex", "provenance": "primary" },
    { "ref": { "origin": "chapter1.tex", "len": 48210,
               "digest": { "algorithm": "sha256", "bytes": "b64:…" } } }
  ],
  "providers": [
    { "id": "latexlike.package", "data": { "name": "base-formatting" } },
    { "id": "core.scope", "data": { "name": "toplevel",
        "definitions": [ { "ct": "macro", "name": "abc",
                           "spec": { "$": ["specs", 1] } } ] } }
  ],
  "specs": [
    { "id": "latexlike.pkg-spec",
      "data": { "provider": { "$": ["providers", 0] }, "ct": "macro", "name": "emph" } },
    { "id": "latexlike.macro-recipe",
      "data": { "args": 3, "opt_default": "x", "body": "…" } }
  ],
  "states": [
    { "rules": { "…": "…" }, "mode": "text", "ext": null,
      "scopes": [ { "$": ["providers", 0] }, { "$": ["providers", 1] } ] }
  ],
  "trees": [ { "nodes": [ { "kind": "chars", "src": 0, "start": 0, "end": 6,
                            "state": 0, "…": "…" } ],
               "annotations": null } ],
  "diagnostics": []
}
```

Note: state entries render bare (homogeneous table — no per-entry identifier);
spec/provider entries render `{id, data}` (heterogeneous). A later segment in the same
stream contains only new table entries plus new trees, referencing earlier indices.

---

## 6. Implementation architecture

### Module layout

- `techy/src/serialize/` (pub(crate) internal module; public facade `techy::serialize`
  in lib.rs — a re-export facade, one canonical path per item, per
  [§dd-dr:public-namespace-topology]). **Unconditional** (D1): `value.rs`
  (SerialValue, SerialEntry, TableId, the SerialIndex bound — NOT the typed index
  newtypes, which live beside their drivers per D11), `error.rs` (SerializeError,
  DeserializeError, SerialValueError — SerialValueError's variants land with the
  bridge in M1), `object.rs` (SerializableObject, DeserializableObject,
  SerializableLang), `engine/` (SerdeSession, tables, ObjectSerdeDriver, segments,
  contexts, interning, resolvers, cycle/depth guards — M0 lands
  `SerializeContext`/`DeserializeContext` here as opaque shells with no public API
  so `object.rs` signatures compile; M2 gives them their real surface), `wire/`
  (core wire structs +
  internal-derive conversions), `drivers/` (source/state/tree/diagnostic drivers;
  spec/provider dispatching drivers; context extension traits). **Feature-gated**:
  `bridge.rs` (serde Serializer/Deserializer over SerialValue, to_value/from_value),
  serde impls for Segment/SerialValue/DiagnosticValue, vocab derives.
- Touches outside the module: supertraits + stub impls (`CallableSpec`,
  `SpecsProvider`, all in-crate spec/provider types); the D21 method pair on
  `CallableSpec`; provenance field on concrete spec types + builder threading (Q5);
  techy-derive: the internal to/from-SerialValue derive (D8); latexlike:
  `latexlike::serialize` (SerializableLang impl, object impls, resolver, register
  helper, vocab derives).

### Pinned signatures (Q2-resolved; bodies illustrative)

```rust
pub enum SerialValue { Null, Bool(bool), Int(i64), Str(String), Bytes(Vec<u8>),
                       List(Vec<SerialValue>), Map(Vec<(String, SerialValue)>),
                       Index { table: TableId, index: u32 } }
pub struct SerialEntry { pub identifier: Cow<'static, str>, pub data: SerialValue }
pub struct TableId(u32);                 // session-assigned, registration order (D5)
pub trait SerialIndex: Copy + Eq + Hash /* + Debug, to/from u32; M2 pins items */ {}
pub trait SerializableLang: Lang { /* M0: bare marker; M3 adds vocab/ext codecs (D17) */ }

pub trait SerializableObject<L: Lang> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, L>)
        -> Result<SerialEntry, SerializeError>
    where L: SerializableLang
    { Err(SerializeError::unsupported()) }
}

pub trait DeserializableObject<L: SerializableLang>: Sized {
    type Output;
    fn deserialize_object(value: &SerialValue, cx: &mut DeserializeContext<'_, L>)
        -> Result<Self::Output, DeserializeError>;
}

// supertraits (unconditional):
//   pub trait CallableSpec<L: Lang>:  … + Any + SerializableObject<L> { …
//       + serialize_argument_spec / deserialize_argument_spec (defaulted, L: SerializableLang) }
//   pub trait SpecsProvider<L: Lang>: … + Any + SerializableObject<L> { … }

// usage:
let mut s = SerdeSession::<Latexlike>::new();       // L: SerializableLang; std tables wired
s.serialize_tree(&tree)?;                            // interns sources/states/specs transitively
let seg = s.take_segment();                          // in-memory Segment, featureless
#[cfg(feature = "serde")] let json = serde_json::to_string(&seg)?;

let mut r = SerdeSession::<Latexlike>::new();
r.set_user_data(env);
latexlike::serialize::register(&mut r);              // resolver + std entries, one line
r.push_segment(seg)?;                                // validate + materialize
let tree = r.tree(tree_index)?;
```

### Key source anchors (as of 2026-08-13; line numbers drift — re-verify before edits)

- Tree tags & storage: `techy/src/node/tree.rs` — tag mint + never-wire law :17-90
  (doc law :39-42), `TreeCore` :113, `NodeData` (crate-private) :101,
  `materialize()` :422. Builder tag mint/regions: `techy/src/node/builder.rs`
  :255-386. Invariant checker: `techy/src/node/invariants.rs` (`validate_tree`).
- Regions & arguments: `techy/src/node/arguments.rs` — `ChildRegion`/`RegionState`
  (embedded `tree_tag`!) :107-124, `ParsedArgument` :231, `ParsedSlot` :415.
- Callable payload: `techy/src/node/kind.rs` — `CallableData` (spec Arc, name,
  invocation_syntax) :187ff.
- Sources: `techy/src/source/source.rs` — `Source` (private fields, not Clone) :22,
  `SourceSpan` (Arc identity equality) :228/:313; `TextContent`
  `techy/src/source/text_content.rs` :29.
- State: `techy/src/state/parsing_state.rs` — `StateData` :19, `ParsingState`
  (private fields, eager derived caches) :64-107; delta `techy/src/state/delta.rs`
  :532; rules `techy/src/token/rules.rs` :349.
- Specs: `techy/src/spec/callable.rs` — `CallableSpec` trait :76 (gains supertrait +
  D21 pair), `StdCallableSpec` :226 (gains provenance field); `ArgumentSpec`
  `techy/src/spec/structure.rs` :234.
- Providers: `techy/src/scopes/mod.rs` — `SpecsProvider` :460 (gains supertrait),
  `Package` :834 (hashbrown maps — determinism!), `Scope` :1175, `ScopeStack` :1497,
  `ScopeOp` :335.
- Diagnostics channel: `techy/src/error.rs` — `serializable_data` :85,
  `DiagnosticValue` :167, adapter-identifier precedent :63-81; `ToDiagnosticValue`
  derive in techy-derive (template for the D8 internal derive).
- latexlike: `techy/src/latexlike/mod.rs` vocab enums :169-283;
  `invocation_syntax.rs` :74/:199 (`Arc<GroupRule>` inside node payload);
  `environments.rs` `EnvironmentSpec` :464.
- Facade wiring: `techy/src/lib.rs` :145-155 (facades), :235 (`__private`).
- Lang trait & ext bundling: `techy/src/state/lang.rs` — `Lang` trait,
  `NodeExtTypes` bundle :43-64/:244, `ClosedVocabulary` :563 (the D17 codec surface
  mirrors these; ext types are reached through the bundle, not flat associated
  types).
- Manifests: root `Cargo.toml` (`rust-version = "1.86"` — the D17 MSRV claim),
  `techy/Cargo.toml` (currently NO `[features]` section — M0 creates it; dev-deps =
  proptest only, so the M0 vtable test is a plain `#[test]`, no trybuild).
- Background only (do not replicate): FLM's round-trip serializer
  `~/Research/util/flm/flm/flmdump.py` (adopted/rejected tricks are already folded
  into §3); pylatexenc sources `~/Research/util/pylatexenc/`.

### Build & verify (every milestone, both feature states)

```bash
cargo build && cargo test                     # feature off (default)
cargo build --features serde && cargo test --features serde
cargo docs                                    # rm -rf target/doc first when verifying links
```

---

## 7. Milestones

All development happens on the long-lived branch **`techy-serialize`** (created by the
user; own it). Commit regularly — small, coherent commits, so the git log doubles as a
recovery record. Auxiliary branches only when genuinely necessary (e.g. parallel
agents editing the same files), merged back into `techy-serialize` promptly. Work runs
in worktrees, never the primary checkout. Each milestone is reviewed by a reviewer
agent against this plan before the next begins, and ends with: tests green both with
and without the feature, `cargo docs` clean, progress log updated. `techy-serialize`
merges into main at project completion (M7), per the local practice: rebase
`techy-serialize` onto `main`, then fast-forward-merge into `main` — no PRs; run the
merge outside the sandboxed primary checkout.

- **M0 — Capability traits & gating skeleton.** Cargo feature wiring (rendering-only,
  D1); `value.rs` types + `error.rs` (unconditional); `SerializableObject`/
  `DeserializableObject` definitions; `SerializableLang` as the D17 bare marker;
  `SerializeContext`/`DeserializeContext` as opaque engine shells (§6); supertraits
  added to `CallableSpec` (spec/callable.rs:76) and `SpecsProvider`
  (scopes/mod.rs:460) + stub impls crate-wide — that means the ~12 non-test
  spec/provider types AND the ~16 impls inside `#[cfg(test)]` modules (`cargo test`
  must compile; generic forms vary, e.g.
  `impl<LLL: LatexlikeLang> SerializableObject<LLL> for MacroSpec<LLL> {}`); the D21
  method pair (defaulted per D21's pinned bodies); **the vacant-vtable test**: a
  plain compile-pass `#[test]` (no trybuild — not in deps) defining a permanent
  test-only `NeverSerializableLang` documented as never implementing
  `SerializableLang`, coercing a stub spec to
  `&dyn CallableSpec<NeverSerializableLang>` and calling a non-gated method — this
  pins D17's vacant-vtable behavior and stays meaningful after M5 precisely because
  that lang never opts in. Rustdoc caution: root lints deny broken intra-doc links,
  so M0 doc comments must not forward-reference M2+ types. Acceptance: builds +
  tests green both feature states; stubs compile for every in-crate spec/provider
  type including test modules; the vtable test passes.
- **M1 — Bridge & internal derive.** serde Serializer/Deserializer over SerialValue;
  index/bytes newtype interception; policy errors (floats, keys, overflow); the
  techy-derive internal to/from-SerialValue derive for wire structs (D8). Acceptance:
  bridge round-trips + rejection tests; JSON rendering snapshots (incl. Bytes/base64).
- **M2 — Engine.** `SerdeSession`, tables, `ObjectSerdeDriver`, typed indices, both
  direction maps, segments (take/push), custom-table + resolver registration, cycle
  check, depth guard, determinism. Tested standalone with toy object kinds. Resolve
  Q6. Acceptance: multi-segment round-trip with sharing preserved; read-then-append;
  cycle/bounds/depth failure tests; deterministic-output test.
- **M3 — Sources & states.** Source driver (embed/reference, digest callbacks,
  provenance edges), TextContent/Span handling; state driver (rules wire structs,
  mode/ext via SerializableLang codecs, scope stacks as ProviderIndex); context
  extension traits; `SerializableObject`/`DeserializableObject` impls for
  `Source`/`ParsingState`. Acceptance: state + source round-trips with Arc identity
  preserved (`same_source`, shared states); digest-mismatch failure test.
- **M4 — Trees.** Tree driver + wire structs; staged regions + builder-style
  re-resolution; fresh tags; D21 index rule + hook pair exercised; annotation/ext
  codecs (D23); multi-tree. Acceptance: parse → serialize → deserialize →
  deep-compare (structure, resolved text, spans, state/spec identity) on the test
  corpus; hostile-input battery (bad indices, non-tiling regions, wire cycles, deep
  recursion).
- **M5 — Specs, providers, latexlike.** Provenance stamp + `new_cyclic` builder (Q5);
  dispatching drivers; latexlike `SerializableLang` + object impls (pkg-spec identity,
  macro recipes, Scope full-dump, environments/specials); namespace resolver +
  `register` helper; vocab derives. Acceptance: real latexlike round-trips including
  `\newcommand` scopes and environments; a `\today`-style dynamic-spec test proving
  instance-not-lookup (D18); unregistered-identifier and dead-Weak failure tests.
- **M6 — Diagnostics, ParseResult, streaming, freeze prep.** Diagnostic driver +
  adapter revive; ParseResult wrapper; JSONL streaming; **Q3 wire vocabulary naming
  pass with the user**; Q7. Acceptance: full ParseResult round-trip; a written draft
  schema description (input for the v1 freeze).
- **M7 — Hardening + permanent docs.** Golden files; proptest round-trip properties;
  rustdoc pass per the user's documentation-clarity rules (user-facing rustdoc: no
  metaphors, no undefined jargon, coined terms defined on first use; error/Panics
  sections exhaustive — target: no panics on any input); performance sanity (large
  stream, many segments). Then, with user review:
  DESIGN_RATIONALE entries + ARCHITECTURE sections/cross-references (including the
  §3.J reversals as superseded-decisions), CLAUDE.md pointer updates if warranted,
  delete `dev-docs/serialization/`.

Dependency notes: M1 needs M0; M2 needs M1; M3/M4 need M2 (and are internally
parallelizable across agents); M5 needs M3+M4; M6 needs M5.

---

## 8. Process & orchestration

- **Worktrees always** — implementer agents never edit the primary checkout (user
  runs concurrent agents there). Worktrees check out `techy-serialize` directly (or
  an auxiliary branch when several agents must edit the same files concurrently;
  merge auxiliaries back promptly).
- **Roles.** Supervisor (per milestone or per 2–3 milestones): decomposes this plan's
  milestone into scoped implementer briefs, keeps its own context lean (reads compact
  reports, not diffs), relays child reports, nudges stalled children. Implementers:
  scoped tasks with explicit file lists + the relevant D-numbers from §3. Reviewers:
  verify each stage diff against this plan + naming principles + panic policy +
  the documentation-clarity rules (as spelled out in M7 — no metaphors, no undefined
  jargon in user-facing rustdoc); produce compact findings reports.
- **Token discipline.** Briefs point to plan sections by number instead of restating
  them; implementers read only the files their task touches; reviewers get diffs and
  the D-register, not the conversation history; supervisors summarize child output
  before it reaches the main context.
- **Recovery after interruption.** State = this file (§3/§4 decisions, §7 milestone
  acceptance) + §9 progress log + `git log --oneline techy-serialize` + worktree
  list. Any fresh session must be able to resume from those alone; that is the bar
  for progress-log entries.
- **Escalation.** OPEN questions (§4), any new design fork, any deviation from a
  D-number → user, not agent discretion. Naming of public items → user (Q3).
  Note: CLAUDE.md rule 7 (record design outcomes in DESIGN_RATIONALE immediately) is
  deliberately deferred to M7 for this project — this plan is the interim record; do
  not "fix" that by editing the permanent docs early.

---

## 9. Progress log

Newest first. Every working session appends: date, actor, milestone, what changed
(branch/commits), what's next, blockers.

- 2026-08-14 — cold-read audit by a context-free agent (grade B) + patches: pinned
  `TableId`, `SerializableLang` M0-marker/M3-items sequencing, M0 context shells,
  the read-entry ("entry currency") definition, D21 default bodies, staged-region
  wire content, DiagnosticValue→SerialValue embedding, homogeneous identifier
  contract, `type Index`/`SerialIndex` naming fix (was `SerializedIndex` — §3.G
  family violation), typed-indices stratum (scaffolding, not value.rs), M0 stub
  scope incl. `#[cfg(test)]` impls, vacant-vtable test specified
  (`NeverSerializableLang`, plain #[test]), docs-clarity rules spelled out inline
  (they lived outside the repo), ff-merge practice defined, anchors extended
  (state/lang.rs, manifests) and lib.rs range corrected. Companion
  `design_session_report.md` added (non-normative: philosophies, rejected patterns,
  false routes). Next: open M0.
- 2026-08-13 — full plan rewritten around the converged architecture (capability
  traits `SerializableObject`/`DeserializableObject` with supertrait write dispatch;
  unified `SerdeSession`; resolver-based read dispatch; `SerializableLang` gate;
  rendering-only cargo gating; `Bytes` in SerialValue; three-family naming — Q2
  RESOLVED; Q1 resolved, Q4 dissolved). Primary checkout returned to `main`;
  `techy-serialize` is owned by project agents from here on. Next: open M0.
- 2026-08-13 — plan adjustments (user): settled decisions are revisable on new
  evidence (escalate in doubt); single long-lived branch `techy-serialize` replaces
  branch-per-milestone (auxiliary branches only for genuine parallel-edit needs).
- 2026-08-13 — plan drafted from interactive design sessions; no implementation
  started; M0 not begun.
