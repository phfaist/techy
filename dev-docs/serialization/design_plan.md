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
- **D7 — The bridge** (feature-gated): serde `Serializer`/`Deserializer` impls over
  `SerialValue` (the `serde_json::Value` pattern: a private serializer type produces
  a `SerialValue`; `&'de SerialValue` implements `Deserializer<'de>`), exposed as
  `to_value<T: Serialize + ?Sized>(&T)` / `from_value<'de, T: Deserialize<'de>>(&'de SerialValue)`
  (borrowed input is the primary form since `deserialize_object` receives
  `&SerialValue`; settled at M1). Index newtypes and
  byte payloads are intercepted so they map to the dedicated variants (clarified
  2026-08-16: indices via ONE sentinel newtype-struct name wrapping the
  `(table, index)` pair — see the D11 revision; bytes via serde's native
  `serialize_bytes`/`deserialize_bytes` channel plus a techy-shipped
  `#[serde(with = …)]` helper module, so no dependency such as `serde_bytes` is
  needed). The bridge enforces D5 policy mechanically (floats, non-string map keys,
  `u64` overflow → `SerialValueError`; `SerialValueError` is UNCONDITIONAL — the
  internal derive's read conversions report shape failures through it too).
  Implementer payload structs use serde derives + explicit renames.
- **D8 — Core wire-struct conversions are unconditional** and therefore cannot use the
  bridge: a small internal derive in techy-derive (precedent: the existing
  `ToDiagnosticValue` derive) generates to/from-`SerialValue` conversions for core's
  wire structs. Two mechanisms coexist by stratum: internal derive for core wire
  structs; serde bridge for implementer payloads (their crates enable the feature).
  Rendering-parity convention (M1 review, 2026-08-16): an absent `Option` field is
  an OMITTED key in both mechanisms — the internal derive omits it natively;
  serde-derived payload/vocabulary structs must carry
  `#[serde(skip_serializing_if = "Option::is_none")]` (+ `#[serde(default)]` on
  read) on every `Option` field so both mechanisms render identically (P7). A present
  `null` reads as `None` in both.

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
  Associated `type Index`: a newtype satisfying the `SerialIndex` bound
  (`Copy + Eq + Hash + Debug` + to/from-`(TableId, u32)` conversions; the bound
  trait's full item list is M2's to pin). **REVISED 2026-08-16 (user-approved):**
  typed index newtypes carry BOTH parts — `{table: TableId, index: u32}` — not a
  bare `u32`. Reason: a session-assigned ordinal `TableId` (D5) cannot be known
  statically by a `u32` newtype, so a context-free bridge (`to_value`, D7) and the
  unconditional internal derive (D8) could not turn a bare `SpecIndex(u32)` into
  `SerialValue::Index { table, index }`; carrying the table makes the conversion
  trivial and context-free, and lets the reader validate `idx.table()` against the
  table it expects (wrong-table index → clean `DeserializeError`). Typed indices are
  minted only by the session (`cx.intern`), so the pair is always consistent on the
  write side. The rejected alternative — a context-aware bridge with a table-name
  registry threaded into `to_value`/the derive — made the free bridge second-class
  and added a name-collision surface. Bridge mechanics: the newtypes' serde impls use
  ONE fixed sentinel newtype-struct name wrapping the `(u32, u32)` pair; the bridge
  intercepts that sentinel. (Audit fix: the earlier spelling `SerializedIndex`
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
  (error names both entries — clarified 2026-08-16 after M2: on the WRITE side the
  error names both TABLES plus identifiers when known, because post-order index
  assignment means neither in-progress object has a position yet; the READ side
  names both entries by position), and reader-side in-progress + recursion-depth
  guards (untrusted input). Typed positions are SESSION-scoped values (D11
  REVISED: they carry the holding session's `TableId`); a position crosses sessions
  only as (table name, `u32`) and is rebuilt on the reading side with
  `TableHandle::position(u32)` — M6's `ParseResult`/stream helpers hand out
  reader-side positions.

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
  (`ParsingState`, `NodeTree`, `Diagnostic`; NOT `Source` — settled at M3: the
  embed/reference decision is source-driver configuration, so the homogeneous
  source driver does the work directly per D11 and `Source` carries no plain impl). Non-participating types owe a
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
  1.86; fallback if it ever failed: drop the where-clause AND the contexts' struct
  bound `L: SerializableLang` (the where-clauses are what make the bounded
  context types well-formed in the signatures), relying on context
  unconstructibility alone (same practical semantics). Sequencing: M0 lands
  `SerializableLang` as a bare marker (`pub trait SerializableLang: Lang {}`); M3
  adds its BOUNDS. **REVISED 2026-08-16 (user-approved, "option B"):** the codecs are
  supplied by ASSERTION, not by trait items. Two value-level capability traits,
  parallel to D13/D14, cover values embedded inline in a payload (an *object* is a
  table entry referenced by index; a *value* is embedded data):
  `SerializableValue<L: Lang>` (`fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError> where L: SerializableLang`)
  and `DeserializableValue<L: Lang>: Sized` (`fn deserialize_value(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError> where L: SerializableLang`).
  Core implements both for `()`, `bool`, the integer widths, `String`,
  `Option<String>` (the default `SourceOrigin`); a preset implements them for its own
  vocabulary and ext types (blanket over `L: Lang`, so a lang reusing those types
  gets the codecs free); `SerializableLang` becomes a marker with associated-type
  bounds in supertrait position (`Lang<ModeId: SerializableValue<Self> +
  DeserializableValue<Self>, CallableTypeId: …, GroupTypeId: …, Event: …, StateExt: …,
  SessionExt: …, SourceOrigin: …, NodeExts: NodeExtTypes<NodeExt: …, ArgumentExt: …,
  SlotExt: …>, InvocationSyntax: …>`; stable ≥1.79 ≤ MSRV 1.86 — M3 probes the nested
  elaboration; fallback: restate the bounds as where-clauses on the scaffolding
  drivers). Reasons: P3/P4 (type owners own codecs), D28(a) shrinks to an empty impl
  block plus impls on the lang's own types, `TrivialLang`-style langs opt in with an
  empty impl. Rejected: ~11 associated-fn pairs on `SerializableLang` (boilerplate
  delegation for `()`/`Option<String>` in every preset; no useful defaults possible).
  The codec surface still covers the lang's closed vocabulary types and its ext types
  (reached via the `Lang::NodeExts: NodeExtTypes` bundle and `StateExt` — see the
  state/lang.rs anchor), plus `InvocationSyntax`, `SourceOrigin`, `SessionExt`. Throughout this plan,
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
  others). **Refined at M5 (2026-08-17, supervisor proposal, user informed):** the
  stamp is `SpecProvenance { provider: Weak<dyn SpecsProvider<L>>, callable_type,
  key }` — the address INSIDE the provider (registered name, or trigger for specials)
  travels with the stamp because a spec does not know its own name(s); a spec Arc
  registered under several names carries the first address (any address resolves to
  the same instance).
- **D20 — Owner-decided granularity; the standard latexlike split.** Packages emit
  identity payloads (`{provider: ProviderIndex, ct, name}`-shaped, via provenance) and
  identity-resolve on read against the caller's environment. `Scope` (dynamic
  `\newcommand` definitions) full-dumps: definitions as `SpecIndex` refs to recipe
  entries. `\newcommand` specs serialize constructor recipes (`{args, opt_default,
  body}`), NOT internals: **no serialization on `ArgumentParser` or
  `EnvironmentBehavior`** — factories rebuild specs through their constructors, which
  re-create parsers/behaviors internally. **Scoping note (M5, 2026-08-17):**
  `\newcommand` does not exist in the crate yet (acceptance.rs: deferred), and no
  shipped spec type is generically recipe-able (argument parsers are opaque
  behaviors; nothing records argument codes) — so at M5 package specs serialize by
  IDENTITY only, recipes exist only for the self-contained types (`ParagraphBreakSpec`,
  `EndSpec`, `BeginSpec`, `InputMacroSpec`), an unstamped instance of a non-recipe
  type is a clear write error, `Scope`/`FallbackProvider` full-dump is implemented
  and tested with those, and the `\newcommand` recipe type is the obligation of
  whoever implements `\newcommand` (D20 stays the design). The reading environment
  for provider identity is a scaffolding value type (`KnownProviders<L>`, name
  pending user confirmation) stored in the session's user data, which becomes a
  small type-keyed map (`user_data::<T>()` looks up by type) so consumers keep
  their own data alongside; missing providers can be supplied by memoized recipes
  (latexlike registers its builtin/minilatex packages that way, since
  `builtin_package()` mints a fresh Arc per call).
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
  with a bounds error — **REVISED 2026-08-16 (user-approved), applied in M1 (small
  follow-up to M0's default body):** the read default is fail-closed — a
  `Some(value)` payload reaching the DEFAULT body is an error ("the writer's spec
  serialized a custom argument-spec payload but this spec type has no override" — a
  spec-type mismatch/environment drift, P5), so the default returns
  `self.arguments().get(index).cloned()` only for `None` input. `index: usize` in
  signatures (the wire stores `u32`).
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
  Clarified after the M4 review (2026-08-17): the wire node list must be EXACTLY the
  set reachable from the root — every non-root node claimed once by a children
  range, and the rebuilt node count re-checked — because the builder's convenience
  of dropping unreachable staged nodes must never hide malformed input.
- **D23 — Annotations & ext serialize with context.** `NodeTree<L, A>` annotations are
  often `SourceSpan`s (extract/transform mint them), so `A` payloads may need
  `cx.intern_*` — a plain `A: Serialize` bound can never suffice. Mechanism: per-call
  or session-registered annotation codec (TypeId-keyed), with a serde-bridge default
  for plain-data `A`; lang ext types (NodeExt/ArgumentExt/SlotExt/StateExt,
  InvocationSyntax) implement `SerializableValue`/`DeserializableValue` (D17
  REVISED). latexlike's
  `StdEnvironmentSideSyntax::name_group_rule` (`Arc<GroupRule>`) is inlined.
  **Lang-opaque span rule (M4 review, 2026-08-17 — supervisor decision, user may
  veto):** language payloads (invocation syntax, ext values, annotations) carry OWNED
  text only — the tree writer materializes the invocation syntax against the node's
  source (`InvocationSyntax::materialized`) before encoding it, and `TextContent`'s
  public value-trait codec is owned-only both ways (a `spanned` form inside a
  language payload is a typed read error). Reason: a value codec receives no node
  and cannot validate offsets, and the builder deliberately skips Callable payloads
  (`check_spanned_contents`), so an offset span in a language payload would be an
  unvalidated panic path (`TextContent::resolve`). Ext values are already
  source-independent by `NodeTree::materialize`'s contract (documented on the tree
  driver). Node text of core payloads (Chars/Group/Comment) stays offset-based (D25)
  and is validated against the node's source by the reader and the builder. The
  public `Span` value-trait impl was removed for the same reason.
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
| `Serializable*` / `Deserializable*` | one-directional capabilities | `SerializableObject`, `DeserializableObject`, `SerializableValue`, `DeserializableValue`, `SerializableLang` |
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

- **D28** — A preset/framework owes exactly: (a) an (empty) `SerializableLang` impl
  plus `SerializableValue`/`DeserializableValue` impls for its vocabulary and ext
  types (D17 REVISED); (b) `SerializableObject` impls for its participating spec/provider
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
  rendering (name string vs ordinal), the canonical base64 form for `Bytes`; the
  region wire form's implicit content-frame discriminator (`content_parent == the
  callable's own index` ⇔ in-region — consider an explicit tag, M4 review); the M5
  identifiers/keys (`core.provider-spec` — reviewer suggests `core.spec-identity`,
  `core.package`, `core.scope`, `core.fallback-provider`, `core.error-spec`,
  `latexlike.begin/end/paragraph-break/input`, the `key: {name}|{trigger}` shape,
  `register_core_readers` vs `register` naming); the
  `Option` asymmetry between derive-omitted keys and verbatim `SerialValue` fields
  (`WireSource.origin` `None` renders `null` while `digest: Option` omits its key —
  M3 review); also
  whether `TableId` keeps its name (§3.G says `…Id` = process-local identity, yet a
  `TableId` travels on the wire inside `SerialValue::Index` AND as
  `SegmentTable::id` — reviewer-noted tension, 2026-08-16).
- **Q5 (M5) — Package-builder API shape** for provenance stamping (`new_cyclic`
  threading; whether core `Package` construction changes or only the latexlike
  builder). **RESOLVED at M5 (2026-08-17; implemented as proposed; user may still
  amend names):**
  additive core builder `Package::new_shared(name, |pkg| …) -> Arc<Package<L>>`
  (`Arc::new_cyclic`; the package keeps its own `Weak<dyn SpecsProvider<L>>` and
  hands out stamps via `provenance_for(callable_type, key)`); concrete spec types
  gain `with_provenance(SpecProvenance)` builders; latexlike's `define_macro`/
  `define_environment` helpers and its packages stamp automatically; `Package::get_specials`
  added for read-side resolution; packages built the old way keep parsing but their
  specs are not serializable (documented).
- **Q6 (M2) — Segment/stream container details**: version placement (first segment
  only?), JSONL conventions, end-of-stream marker or not, `take_segment`/
  `push_segment` final names.
  **RESOLVED (M2 proposal, user-accepted 2026-08-16):**
  - **Version in EVERY segment.** `Segment { version, tables }`; a `pub const
    Segment::VERSION: u32 = 1`; `push_segment` validates `version == VERSION` and
    rejects any other with `DeserializeError::UnsupportedVersion { found, expected }`.
    Rationale: every segment is then an independently valid, self-describing value —
    a stream can be split into per-file/per-message pieces with no shared preamble,
    and a truncated stream's surviving segments each still validate. The few bytes of
    a repeated integer are negligible next to that (P7's canonical-form discipline
    is unaffected — the version is real data, not representation).
  - **JSONL = one segment per line; no end-of-stream marker.** The stream ends when
    the input ends (EOF, connection close, last file); each line is a whole segment
    read independently in order. No sentinel/footer record, so a stream can be
    appended to by appending lines, and a partial write costs only its own last line.
    The in-memory `Segment` type is format-agnostic; JSONL is a convention over it
    (`serde_json::to_string(&seg)` per line), not a type — the engine emits/absorbs
    `Segment` values and never touches an encoder.
  - **Final method names kept: `take_segment` / `push_segment`.** `take_` matches the
    house drain-and-advance sense (it drains the pending-emission tail and advances
    the emitted mark); `push_` matches absorb-into-my-tables. `SerdeSession::empty()`
    is the M2 constructor (D9 reserves `new()` for the standard-tables one).
  - **Directory: every registered table appears, in registration order,** each with
    its name, the writer's `TableId`, the start position, and its new entries (empty
    list if none). The full directory (not just changed tables) lets the reader map
    every writer table id that any entry references — including ids belonging to
    tables with no new entries this segment — by name; the cost is a handful of
    empty-entry records. A reader matches tables by NAME (registration order may
    differ) and translates every `SerialValue::Index` from the writer's ids to its
    own before materializing.
  - **Additional invariant surfaced while implementing:** `push_segment` requires the
    absorbing session to have NO entries pending emission (nothing interned since the
    last `take_segment`) — a segment continues the stream the session has emitted, so
    the natural order is absorb-all-then-append (`DeserializeError::UnemittedEntries`).
  - **Stream identity is a caller obligation (v1):** the segments pushed into one
    session must belong to ONE stream, in order; the engine checks contiguity
    (`start == len`) but cannot detect a foreign segment whose `start` happens to
    match. No stream-identity field in v1; revisit if a use case needs enforcement.
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

- `techy/src/serialize/` (the public `techy::serialize` facade module itself, on the
  `source`/`error` own-facade pattern: `pub mod serialize` with private submodules and
  re-exports at the module root — one canonical path per item, per
  [§dd-dr:public-namespace-topology]; corrected 2026-08-16 — an earlier wording asked
  for both `pub(crate) mod serialize` and `pub mod serialize`, which cannot coexist). **Unconditional** (D1): `value.rs`
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
pub trait SerialIndex: Copy + Eq + Hash /* + Debug, to/from (TableId, u32) — D11 REVISED; M2 pins items */ {}
pub trait SerializableLang: Lang</* M3: associated-type bounds requiring SerializableValue + DeserializableValue on the vocab/ext types (D17 REVISED) */> {}

pub trait SerializableValue<L: Lang> {                     // embedded values (D17 REVISED)
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
        where L: SerializableLang;
}
pub trait DeserializableValue<L: Lang>: Sized {
    fn deserialize_value(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
        where L: SerializableLang;
}

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
let tree = r.tree(trees.position(0))?;                  // positions are session-scoped: rebuilt on the reader side
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
- **M3 — Sources & states.** The `SerializableValue`/`DeserializableValue` traits +
  core impls + `SerializableLang` bounds (D17 REVISED); source driver
  (embed/reference, digest callbacks, provenance edges), TextContent/Span handling;
  state driver (rules wire structs, mode/ext via the value traits, scope stacks as
  ProviderIndex); the specs/providers tables instantiated (dispatching driver, no
  real impls yet) and `SerdeSession::new()` with the standard tables so far; context
  extension traits; `SerializableObject`/`DeserializableObject` impls for
  `Source`/`ParsingState`. Acceptance: state + source round-trips with Arc identity
  preserved (`same_source`, shared states); digest-mismatch failure test.
- **M4 — Trees.** Tree driver + wire structs; staged regions + builder-style
  re-resolution; fresh tags; D21 index rule + hook pair exercised; annotation/ext
  codecs (D23); multi-tree. Acceptance: parse → serialize → deserialize →
  deep-compare (structure, resolved text, spans, state/spec identity) on the test
  corpus; hostile-input battery (bad indices, non-tiling regions, wire cycles, deep
  recursion). (Scheduling note 2026-08-17: the REAL latexlike corpus round-trip runs
  at M5, when the spec/provider impls exist; M4 ran toy-lang parses + hand-built
  trees through the same harness.)
- **M5 — Specs, providers, latexlike.** Provenance stamp + `new_cyclic` builder (Q5);
  dispatching drivers; latexlike `SerializableLang` + object impls (pkg-spec identity,
  self-contained recipes for the recipe-able types, Scope/FallbackProvider
  full-dump, environments/specials); `register` helper (no namespace resolver was
  needed — all identifiers are static; D28(d) is satisfied by the helper chaining
  `register_core_readers`); vocab derives. Acceptance (as scoped by the D20 note —
  `\newcommand` does not exist yet): real latexlike round-trips including
  environments and the nested `minilatex.item` package; Scope full-dump with
  recipe-able specs; a `\today`-style dynamic-spec test proving instance-not-lookup
  (D18); unregistered-identifier, dead-Weak, missing-provider and unstamped-spec
  failure tests. DONE 2026-08-17 (reviewed, Opus 5: APPROVE WITH NITS).
- **M6 — Diagnostics, ParseResult, streaming, freeze prep.** Diagnostic driver +
  adapter revive; ParseResult wrapper; JSONL streaming; **Q3 wire vocabulary naming
  pass with the user**; Q7. Acceptance: full ParseResult round-trip; a written draft
  schema description (input for the v1 freeze).
- **M7 — Hardening + permanent docs.** Golden files; proptest round-trip properties;
  cost bounds on wire-controlled quantities (M3 review: the prefix table build is
  O(n²) in the number of group rules — dedup via hash map or bound the rule count;
  line/column offsets read unvalidated — sanity bound or saturating arithmetic in
  `LineIndex`);
  a nesting-depth bound for `SerialValue`'s serde `Deserialize` (and `Segment`'s,
  AND the unconditional `Segment::from_serial_value` — recursive clone/drop of deep
  values, M2 review) so formats without their own recursion limit (binary use case
  1) cannot overflow the stack on hostile input — depth-carrying `DeserializeSeed`
  or an equivalent (M1 review finding: serde_json's own limit protects JSON;
  postcard does not);
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

- 2026-08-17 — M6 implementer agent — **M6 complete** on `techy-serialize` (worktree
  `.claude/worktrees/techy-serialize`). Commits: `b65f4a1` the M5 review nits (matched
  variant names in `expect_unit_variant`, `missing_standard_table` naming the missing
  table in `register_core_readers`/`latexlike::serialize::register`,
  `KnownProviders::resolve` CHECKS a recipe-built provider's `name()` against the key
  (decision: check, not just document — `DeserializeError::Failed` naming both),
  `Package::clone` wire consequence with the `ProviderRecipe` cross-reference,
  `StdCallableSpec::provenance()` getter, "loaded data" → "part of the reading
  program's own configuration" ×3, tree driver "Reading never panics", the D18 counter
  re-asserted after the round trip, a feature-gated pinned `(len, fnv1a64)` rendering
  digest of the determinism input as the cross-process pin); `034fef2` the diagnostic
  and parse-result drivers; `92e2002` the stream tests; `b408849` the two transient
  docs; plus the docs commit carrying this entry. Verified: `cargo build`/`test` green
  with and without `--features serde` (1001/1035 unit — +26/+28 in
  `serialize/drivers/diagnostic_tests.rs`, +4 latexlike parse-result tests, +1 pinned
  digest —, 30+8+13+23+1 integration plus the feature-gated 4-test
  `tests/serialize_stream.rs`, 75/76 doctests); `rm -rf target/doc && cargo docs` clean
  both states; clippy clean on all new code. **What M6 built:**
  - **Diagnostics table (D26; ordinal 5, `"diagnostics"`, homogeneous
    `core.diagnostic`)** — `serialize/drivers/diagnostic.rs` + `wire/diagnostic.rs`:
    `DiagnosticSerdeDriver<L>` over `Diagnostic<L::SourceOrigin>`, `DiagnosticIndex`;
    plain-trait `SerializableObject`/`DeserializableObject` impls on
    `Diagnostic<L::SourceOrigin>` (D13's list) the driver delegates to; wire
    `{severity, identifier, data, message, span, frames: [{title, span}]}` — `severity`
    a wire enum `note|warning|error` (+ `Severity` value-trait impls), `data` the
    `DiagnosticValue` embedded verbatim, **`message`** the write-time `Display` (the
    supervisor's addition — flagged for Q3), spans through the M3 span helpers (sources
    shared with the tree). `impl From<DiagnosticValue> for SerialValue` and
    `From<&DiagnosticValue>` (unconditional, infallible); the reverse conversion is
    private to the driver and rejects `Bytes`/`Index` anywhere inside with the new
    typed **`DeserializeError::UnrepresentableDiagnosticValue { kind }`**; feature-gated
    `impl serde::Serialize for DiagnosticValue` delegating through the embedding (one
    rendering path); **no `Deserialize` for `DiagnosticValue`** — nothing needs it (the
    reader goes `SerialValue` → validated conversion). Read revives an ADAPTER condition:
    public **`DeserializedCondition { identifier, data, message }`** (`new`, `data()`,
    `Display` = the stored message; `impl DiagnosticInfo` with `const IDENTIFIER =
    "core.serialization.deserialized-condition"` and the per-instance `identifier()`
    override — exactly [§dd-dr:runtime-condition-identity]'s exception), rebuilt via
    `Diagnostic::from_parts`; documented on the driver, the type, and the module: a
    diagnostic read back answers the written `identifier()`, `serializable_data()`, and
    `message()` (renders identically), `Diagnostics::with_identifier` finds it,
    `downcast_ref::<Original>()` is `None` and `conditions::<Original>()` yields nothing
    — the identifier is the contract across the boundary. Sugar
    **`DiagnosticSerialization`** (`serialize_diagnostic(&Diagnostic) ->
    DiagnosticIndex` — a diagnostic is a VALUE, wrapped in a fresh `Arc` per call like a
    tree; `diagnostic(DiagnosticIndex) -> Diagnostic` owned clone), session-level like
    `TreeSerialization`.
  - **ParseResult wrapper (D26; ordinal 6, `"parse-results"`, homogeneous
    `core.parse-result`)** — `drivers/parse_result.rs` + `wire/parse_result.rs`:
    `ParseResultSerdeDriver<L>` over `ParseResult<L>`, `ParseResultIndex`, plain-trait
    object impls on `ParseResult<L>`; wire `{tree: TreeIndex, diagnostics: {items:
    [DiagnosticIndex], limit, suppressed, error_count}, session_ext: <SessionExt value>}`
    — **the collection is a NESTED object** (mirrors `Diagnostics`, P1; the brief
    sketched a flat form — flagged for Q3). Write: the tree cloned into a fresh
    `Arc<dyn Any>` and interned into `trees` (unit annotation), each diagnostic cloned
    into a fresh Arc and interned (values), the session ext through its value
    conversion. Read: the tree through the trees table (a non-`()`-annotation entry is
    the tree sugar's `Failed` error naming both identifiers — the downcast factored
    into `pub(crate) tree_of_object`), the diagnostics cloned out of their entries, the
    ext through `DeserializableValue`, then the counts VALIDATED against the invariants
    `Diagnostics::push` maintains (`retained <= limit`; `suppressed > 0 ⇒ retained ==
    limit`; `retained_errors <= error_count <= retained_errors + suppressed`) — the new
    typed **`DeserializeError::InconsistentDiagnosticCounts { retained, retained_errors,
    limit, suppressed, error_count }`** — and rebuilt with the new `pub(crate)
    Diagnostics::from_parts(items, limit, suppressed, error_count)`. **Decision
    (recorded):** the brief's "bound-check the counts as u32-sized sanity" was replaced
    by these invariant checks (strictly stronger; a large-but-consistent `limit` is
    harmless — a cap allocates nothing — and is accepted; a `usize` beyond `i64` cannot
    be written at all: `IntegerOutOfRange`, so a `Diagnostics::with_limit(usize::MAX)`
    "no cap" is NOT serializable — a limitation flagged for the user). New public
    `Diagnostics::error_count()`. **Deviations from the brief (recorded):** the sugar
    is `serialize_parse_result(&Arc<ParseResult<L>>)` (interned BY IDENTITY — the same
    Arc twice yields the existing position, unlike a tree/diagnostic) and
    `parse_result(ParseResultIndex) -> Arc<ParseResult<L>>` (the shared Arc, NOT an
    owned clone) — because `Lang::SessionExt` is `Debug + Default + Send + Sync` and NOT
    `Clone`, a `ParseResult` can neither be cloned into an Arc from a borrow nor cloned
    out of one; documented on the trait. Sugar **`ParseResultSerialization`**.
    `StandardTables` gains `diagnostics` and `parse_results`; **D9's six standard
    tables became SEVEN** (plan patch for the supervisor: `SerdeSession::new()` registers
    sources(0) … trees(4), diagnostics(5), parse-results(6)); every pinned segment
    rendering gained the two empty directory rows; the tree round-trip harness finds the
    trees table by name (a five-table `empty()` composition still works).
  - **JSONL streaming (Part 3)** — a documented CONVENTION, no encoder in techy:
    `techy::serialize` module docs ("Streams as JSON Lines": one `Segment` per line via
    `serde_json::to_string`, read per line with `serde_json::from_str::<Segment>` and
    pushed in order; each line independently valid yet stream-scoped; EOF ends the
    stream; the same for any framing format; reading-then-appending shares
    environment-held objects, not equal live objects made anew) and the feature-gated
    integration test `techy/tests/serialize_stream.rs` (public API only): two latexlike
    parse results as two lines, a second session reads them, appends a third parse
    (packages resolved by identity through a `KnownProviders` holding the language's
    seed providers → 0 new provider entries; the fresh parse's states are new entries —
    identity, not equality), a third session reads all three lines (sharing asserted:
    A/B share the seed state Arc; A/C's `\emph`/`\textit` specs' provider is one
    package instance); a lone second line parses as a `Segment` but is
    `SegmentOutOfOrder`; a truncated last line loses only itself; the same stream as
    postcard length-prefixed frames, and the two renderings decode to equal segments.
    **No helper proposed** — one line of `serde_json` per segment needs no wrapper.
  - **Freeze prep (Part 4)** — `dev-docs/serialization/wire_vocabulary.md` (the complete
    inventory: envelope/directory keys, the seven table names + ordinals, every entry
    identifier, every key per object kind with file:line anchors, enum strings, the
    latexlike vocabulary + serde renames + spec forms, the condition identifiers with
    their derive-emitted projection keys, reserved JSON forms + base64 + compact-rendering
    names, the `Option` omitted-vs-`null` map, the `Index`/`TableId`/content-frame
    questions, a table of names that violate a scheme or read badly, the consolidated
    OPEN list, and the **Q7 proposals**: adopt a zero-wire-byte check `arguments.len()
    == spec.arguments().len()` on read — today the reader catches only an index BEYOND
    the reading spec's declared count, not a reading spec with MORE arguments; consider
    a language-identity string; skip node counts/source lengths/segment digests for v1)
    and `dev-docs/serialization/schema_draft.md` (the abstract structure per table/entry
    with one worked example per object kind cut from ONE real tolerant latexlike parse
    `\e{x} {`, the segment/directory, stream conventions, the compatibility-policy
    placeholder). Documented in passing: the writer WRITES a provided argument's unit ext
    as `"ext": null` (the M4 log's "omits the key" describes the reader's tolerance) —
    the tree.rs comment corrected.
  - **Tests** (both feature states): `drivers/diagnostic_tests.rs` (a `DiagLang` toy with
    `SessionExt = u32`) — every severity round-trips as a `DeserializedCondition`
    (identifier, projection, message, `to_string`, `render`, span, frames; original type
    not downcastable; the adapter's own `IDENTIFIER`); a real tolerant parse's
    diagnostic; found by identifier not by type; sources shared with the tree (one source
    entry; `same_source` after reading); a diagnostic is a value (two entries); parse
    results with diagnostics + a non-unit session ext, clean, with suppressed pushes
    (counts survive), interned by identity and read back shared, through the general
    `intern`; the embedding, `Severity` strings, feature-gated `DiagnosticValue`
    rendering; hostile: unknown severity, `Bytes`/`Index` inside the projection, frame
    span out of range, span into the wrong table, span beyond the source, wrong-shaped
    entry, diagnostics list out of range / wrong table, a parse result naming a
    `String`-annotated tree, every inconsistent-count case (+ consistent edits incl. a
    `1 << 40` cap read fine; a negative count is `IntegerOutOfRange`), a wrong-shaped
    session ext, the sugar naming missing tables, determinism, a pinned JSON of one
    diagnostic + one parse-result entry. `latexlike/serialize_tests.rs`: strict parse
    results, tolerant parse results with tracebacks (`\emph{x`, `\begin{A}\emph{x\end{A}`,
    lists; the traceback-presence expectations pinned per input), one source entry
    shared by tree, diagnostics, and frames.
  **Provisional wire names (Q3):** tables `diagnostics`, `parse-results`; identifiers
  `core.diagnostic`, `core.parse-result`, condition `core.serialization.deserialized-condition`;
  diagnostic keys `severity` (`note`/`warning`/`error`), `identifier`, `data`, `message`,
  `span`, `frames` (`title`, `span`); parse-result keys `tree`, `diagnostics` (`items`,
  `limit`, `suppressed`, `error_count`), `session_ext`. **Public API surface (new):**
  `techy::serialize::{DiagnosticSerdeDriver, DiagnosticIndex, DiagnosticSerialization,
  DeserializedCondition, ParseResultSerdeDriver, ParseResultIndex,
  ParseResultSerialization}`; `StandardTables::{diagnostics, parse_results}`;
  `DeserializeError::{UnrepresentableDiagnosticValue { kind }, InconsistentDiagnosticCounts
  {…}}`; `impl From<DiagnosticValue> for SerialValue`, `impl From<&DiagnosticValue> for
  SerialValue`; feature-gated `impl Serialize for DiagnosticValue`;
  `SerializableValue`/`DeserializableValue` impls on `Severity`;
  `SerializableObject`/`DeserializableObject` impls on `Diagnostic<L::SourceOrigin>` and
  `ParseResult<L>`; `Diagnostics::error_count()`; `StdCallableSpec::provenance()`;
  `KnownProviders::resolve` now checks the recipe-built name (behavior change).
  Crate-internal: `Diagnostics::from_parts`, `tree_of_object`, `missing_standard_table`,
  `STANDARD_TABLE_NAMES`, `expect_unit_variant(name: &str)`. Next: M6 review → user Q3/Q7
  session over `wire_vocabulary.md` (+ `schema_draft.md`) → rename pass → M7. Blockers:
  none.
- 2026-08-17 — M5 implementer agent — **M5 complete** on `techy-serialize` (worktree
  `.claude/worktrees/techy-serialize`). Commits: `698f738` the M4 review leftovers
  (two pinned hostile node-text span tests — a `Chars` content span out of range, a
  group delimiter span off a char boundary, both builder-rejected at the node; the
  dead `A: Clone` bound on `rebuild_tree` dropped; a `TreeSerdeDriver` Panics section
  for the writer's `InvocationSyntax::materialized` on a live tree violating the
  `Spanned` invariant); `fa1d821` provenance stamps, shared packages,
  `KnownProviders`, the core and latexlike spec/provider serialization; `cd863bd`
  the feature-gated vocab serde derives + the M5 test battery; `faae9b0` module docs,
  the Scope/FallbackProvider notes, the `Flavored` (foreign family member) round
  trip; plus the docs commit carrying this entry. Verified: `cargo build`/`test`
  green with and without `--features serde` (972/1003 unit — 28/30 in
  `latexlike/serialize_tests.rs`, +2 tree tests, +1 engine test, +1 latexlike test —,
  30+8+13+23+1 integration, 75/76 doctests); `rm -rf target/doc && cargo docs` clean
  both states; clippy clean on all new code. **What M5 built** (plan §7 M5, rulings
  R1–R6):
  - **Provenance (D19, Q5 RESOLVED as R2)** — `techy/src/scopes/provenance.rs`:
    `SpecProvenance<L> { provider: Weak<dyn SpecsProvider<L>>, callable_type:
    L::CallableTypeId, key: DefinitionKey }` (`new`, `provider()` (upgrade),
    `callable_type()`, `key()`; `Clone`, `Debug` naming the provider) and
    `DefinitionKey::{Name(Box<str>), Trigger(Box<str>)}` (`as_str`, `Display`) — a
    name and a trigger are separate keys (separate package stores; no collision), so
    the stamp records which. `Package::new_shared(name, build: impl FnOnce(&mut
    Package<L>)) -> Arc<Package<L>>` (`Arc::new_cyclic`; the package keeps its own
    `Weak<dyn SpecsProvider<L>>` in a private `self_weak` field), `is_shared()`,
    `provenance_for(ct, name)` / `provenance_for_specials(ct, trigger)` (`None` for
    an unshared package; nothing checks the key is defined — the caller inserts the
    stamped spec under it), `get_specials(ct, trigger)`; `Package::clone` yields an
    UNSHARED package (its stamps would name the original); `Debug` shows `shared`.
    Concrete spec types carry `provenance: Option<SpecProvenance<L>>` +
    `with_provenance(stamp) -> Self` + `provenance() -> Option<&_>`: latexlike
    `MacroSpec`, `SpecialsSpec`, `EnvironmentSpec`, `BeginSpec`, `InputMacroSpec`
    (private fields); core `StdCallableSpec` has a **`pub provenance` field** +
    `with_provenance` (no getter) — **deviation from R2's "private field", recorded:**
    a private field would have removed the documented struct-literal form
    (`StdCallableSpec { arguments, .. }` — functional update needs every field
    visible), which ~18 in-crate sites and external code use; the type is plain data
    with a `pub arguments` field already, and the ruling's intent (a field the impl
    reads, not a trait item, plus a builder) is met. `EndSpec` and
    `ParagraphBreakSpec` carry **no** stamp field (unit types: their self-contained
    form reproduces an equivalent instance; `EndSpec` keeps `Copy`) — R1's "may
    prefer identity when stamped" applied to `BeginSpec` and `InputMacroSpec` only.
    latexlike's `define_macro`/`define_environment` stamp automatically when the
    package is shared (no stamp otherwise — nothing to stamp with); `builtin_package()`
    and `minidefs::minilatex_package()` now return **`Arc<Package<LLL>>`** built with
    `new_shared` and stamped (the `\begin` `BeginSpec` stamped; `EndSpec` unstamped;
    minilatex's shared typography `SpecialsSpec` instance carries ONE stamp — the
    tie's — since a stamp names one address of the instance and any address resolves
    to it); new `minidefs::minilatex_item_package()` (a fresh, shared, stamped
    `minilatex.item` per call; `minilatex_package()` nests one); call sites updated
    (`[minilatex_package(), Arc::new(testdb())]`; `Latexlike::initial_state_data`
    pushes the shared builtin; test_support's `macro_package` returns a shared Arc).
    An unstamped spec of an identity-only type is `SerializeError::MissingProvenance
    { spec: &'static str }` (names the type; "built outside a shared package — see
    Package::new_shared"); a dead `Weak` is `SerializeError::ProviderDropped
    { callable_type: String (Debug), key: DefinitionKey }`.
  - **Core object impls (D20, R3, R4)** — `serialize/drivers/specs.rs` +
    `serialize/wire/specs.rs`: `SpecProvenance<L>` implements `SerializableObject`
    (the identity form: `{provider: ProviderIndex, callable_type: <value>, key:
    {name: …} | {trigger: …}}` under **`core.provider-spec`**; interns the upgraded
    provider) and `DeserializableObject` (`Output = Arc<dyn CallableSpec<L>>`: reads
    the provider position, downcasts to `Package<L>` (`Any`), `get`/`get_specials` by
    the key — the very Arc; a provider that is not a `Package` is
    `DeserializeError::Failed` ("custom provider types register their own reader");
    an absent definition is `DeserializeError::MissingDefinition { provider,
    callable_type, key }`); the shared body `serialize_stamped_spec(provenance,
    type_name, cx)` (crate-internal) serves `StdCallableSpec` and the latexlike
    identity-only types. `Package`: identity `{name}` under **`core.package`**; read
    → `cx.user_data::<KnownProviders<L>>()` → held provider by name, else its recipe
    (built now; the session's providers slot IS the memo — every reference to that
    entry shares the Arc; a second entry of the same name builds again), else
    `DeserializeError::MissingProvider { name }` (also when no `KnownProviders` is
    set). `Scope`: in full `{name, definitions: [{callable_type, name, spec:
    SpecIndex}]}` under **`core.scope`**, BTreeMap order (ct, then name);
    read → `Scope::new` + `insert`, a definition listed twice is `Failed`.
    `FallbackProvider`: `{name, fallbacks: [{callable_type, spec}]}` under
    **`core.fallback-provider`**; read → `new` + `set`, a duplicate ct is `Failed`.
    `ErrorCallableSpec` (not generic over `L`, so it cannot hold a stamp; plain
    data): self-contained `{detail?}` under **`core.error-spec`**. New public
    accessors `Scope::definitions()` and `FallbackProvider::fallbacks()` (ordered
    iterators) feed the writers.
  - **The reading environment (R3)** — `KnownProviders<L>` (public,
    `techy::serialize`): `new`, `insert(impl IntoSpecsProvider<L>)` (keyed by
    `provider.name()`; returns the replaced Arc), `get(name)`,
    `register_recipe(name, impl ProviderRecipe<L> + 'static)`, `recipe(name)`,
    `resolve(name) -> Result<Option<Arc<dyn SpecsProvider<L>>>, _>` (held, else
    built by recipe, else `None`), `provider_names()`, `recipe_names()`; `Default`,
    `Debug`. `ProviderRecipe<L>: Send + Sync { fn build(&self) -> Result<Arc<dyn
    SpecsProvider<L>>, DeserializeError> }` with a blanket impl for `F: Fn() -> P +
    Send + Sync, P: IntoSpecsProvider<L>` — so a function item is a recipe
    (`known.register_recipe("minilatex", minilatex_package::<Latexlike>)`); a
    fallible recipe implements the trait itself. `SerdeSession::set_user_data` /
    `user_data::<T>()` are now a **type-keyed map** (one value per type; setting a
    type replaces that type's value only — the crate's `KnownProviders` and a
    framework's own environment coexist). `register_core_readers(&mut SerdeSession<L>)
    -> Result<(), RegistrationError>` (public, `techy::serialize`) registers the five
    core readers on the specs/providers tables (`RegistrationError::UnknownTableName
    { name: String }` NEW when a standard table is missing). **Decision:**
    `SerdeSession::new()` does NOT pre-register them (plan §6 usage + D28(d): a
    language's `register` helper chains its dependencies' — calling both a
    language's helper and the core helper on one session is a `DuplicateIdentifier`
    error, documented).
  - **latexlike (D28, R5)** — `techy/src/latexlike/serialize.rs` (public module
    `techy::latexlike::serialize`): `impl SerializableLang for Latexlike {}` (empty —
    D17 REVISED); hand-written, unconditional value conversions blanket over `L:
    Lang` for `CallableType` (`macro`/`environment`/`specials`), `MathGroupForm`
    (`inline`/`display`), `GroupType` (`content` / `{math: <form>}` / `verbatim`),
    `Mode` (`text`/`math`), `Event` (`exit-math-context`), `BodyMarker` (`{body:
    bool}`), `InvocationSyntaxData<Env>` (`{macro: {escape_char, post_space}}` /
    `{environment: <Env value>}` / `specials`; generic over `Env: SerializableValue +
    DeserializableValue`), `StdEnvironmentSyntax<L>` (`{begin, end?}`; `end` omitted
    when `None`), `StdEnvironmentSideSyntax<L>` (`{escape_char, command_word,
    post_space, name_group_rule}` — the two texts through `TextContent`'s owned-only
    conversion, the rule INLINED through `GroupRule<L>`'s and read back into a fresh
    `Arc` — D23). Object impls: `MacroSpec`/`SpecialsSpec`/`EnvironmentSpec` identity
    only (`MissingProvenance` otherwise); `BeginSpec` identity when stamped, else
    **`latexlike.begin`** `{end_command_name}`; `EndSpec` **`latexlike.end`** `{}`;
    `ParagraphBreakSpec` **`latexlike.paragraph-break`** `{}`; `InputMacroSpec`
    identity when stamped, else **`latexlike.input`** `{persist_state,
    attached_slot_ext: <SlotExt value>}` (rebuilt through `input_macro_spec`; new
    getters `persist_state()`, `attached_slot_ext()`); the unit forms are the empty
    map (read: any key is an error). `register<LLL>(&mut SerdeSession<LLL>) ->
    Result<(), RegistrationError>` (bounds: `LLL: LatexlikeLang + SerializableLang`,
    `SlotExt<LLL>: BodySlotExt`, `ArgumentExt<LLL>: Default` — the spec types'
    `CallableSpec` bounds) calls `register_core_readers` then registers the four
    latexlike readers; `register_package_recipes(&mut KnownProviders<LLL>)` adds the
    `_builtin` recipe (`builtin_package::<LLL>`); **`minidefs::register_package_recipes`**
    adds `minilatex` and `minilatex.item` (kept in `minidefs` so the module stays
    dead-strippable — no other latexlike module references it). Recorded: a
    recipe-built `minilatex.item` is a distinct instance from the one nested inside a
    recipe-built `minilatex` (its `itemize` body delta) — consistent within the
    reader, only observable by re-parsing with a rebuilt state; a program that wants
    exact identity inserts the Arcs it holds. **D28(d) — no namespace resolver is
    registered:** every identifier is static (`core.*`, `latexlike.*`), so `register`
    satisfies (d) through `register_type`; no dynamic identifiers exist yet.
    Feature-gated serde derives with explicit renames on `CallableType`, `GroupType`,
    `MathGroupForm`, `Mode`, `Event`, `BodyMarker` (D28(f)); a test pins parity with
    the value conversions (P7). `Flavored` (the test family member) opts in with the
    empty impl and round-trips through `register::<Flavored>`.
  - **`\newcommand` (R1, recorded):** whoever implements `\newcommand` owes a dedicated
    spec type with a `{args, opt_default, body}` self-contained form (D20); no
    argument-code recording was added to `ArgumentSpec`.
  - **Tests** (`latexlike/serialize_tests.rs`, both feature states): the oracle
    corpus through the M4 harness with a latexlike session factory (macros with and
    without post-space, every argument shape incl. star/optional/absent and a core
    `StdCallableSpec` in a latexlike package, environments incl. recorded spacing and
    nesting, minilatex lists with `\item` resolved in the body-pushed item package
    (the body state's innermost provider is `minilatex.item`; both `\item` nodes share
    one spec), specials, groups/math/comments, verbatim env + `\verb`, paragraph
    breaks in both styles (`ParagraphBreakSpec` rebuilt; one Arc per break, as
    minted), the kitchen sink, tolerant recoveries, `\input` across sources with a
    `MapResolver`); identity — stamped specs read back as the environment's package
    instances (and the states' builtin is the environment's very Arc); recipes build
    the providers the environment does not hold (`_builtin`/`minilatex`/
    `minilatex.item`; the read spec is the built package's instance; a tie resolved by
    trigger), a held provider takes precedence, a recipe is built once per entry and
    shared by every reference (two trees, one stream); typed failures — missing
    provider (with and without any `KnownProviders`), missing definition (an
    environment package lacking the writer's definition), unregistered identifier (a
    session without `register`), unstamped specs of every identity-only type
    (`MacroSpec`, `SpecialsSpec`, `EnvironmentSpec`, `StdCallableSpec`), a dropped
    provider; self-contained forms (`BeginSpec` custom pair, `EndSpec`, an unstamped
    `\input` rebuilt fresh) and a stamped `BeginSpec` preferring identity; **D18 (R6)**
    — a math-only definition (`insert_in_modes`) resolves by identity while the
    text-mode query answers nothing, and a `CountingProvider` whose `retrieve_spec`
    answers spec A once and spec B afterwards reads back the parsed instance A with
    the counter untouched (its specs emit their own `test.counted` entries — the
    custom-provider identity route); `Scope` + `FallbackProvider` in full inside a
    real scope stack `[fallback, builtin, oracle, scope]` (identity survives inside
    the scope; the error spec's detail; the tree's node holds the scope's Arc); a
    duplicate scope definition rejected; a hostile state whose stack is [odd Scope,
    FallbackProvider, recipe minilatex] freezes (`specials_trigger_chars` total), an
    unknown callable type is a `Value` error, a spec identity naming a non-package
    provider is refused; determinism; parity + a pinned JSON rendering under the
    feature. Also `user_data_holds_one_value_per_type` (engine tests) and the M4
    leftovers' two span tests.
  **Provisional wire names (Q3):** identifiers `core.provider-spec`, `core.error-spec`,
  `core.package`, `core.scope`, `core.fallback-provider`, `latexlike.begin`,
  `latexlike.end`, `latexlike.paragraph-break`, `latexlike.input`; keys `provider`,
  `callable_type`, `key` (`name` | `trigger`), `name`, `definitions`, `spec`,
  `fallbacks`, `detail`, `end_command_name`, `persist_state`, `attached_slot_ext`,
  `body`, `escape_char`, `post_space`, `begin`, `end`, `command_word`,
  `name_group_rule`; vocab strings as listed above. **Public API surface (new):**
  `core::specs::{SpecProvenance, DefinitionKey}`; `Package::{new_shared, is_shared,
  provenance_for, provenance_for_specials, get_specials}`; `Scope::definitions`,
  `FallbackProvider::fallbacks`; `StdCallableSpec::{provenance (pub field),
  with_provenance}`; latexlike `MacroSpec/SpecialsSpec/EnvironmentSpec/BeginSpec/
  InputMacroSpec::{with_provenance, provenance}`, `InputMacroSpec::{persist_state,
  attached_slot_ext}`; `builtin_package`/`minilatex_package` now `-> Arc<Package<LLL>>`
  (breaking; call sites updated), `minidefs::{minilatex_item_package,
  register_package_recipes}`; `techy::latexlike::serialize::{register,
  register_package_recipes}`; `techy::serialize::{KnownProviders, ProviderRecipe,
  register_core_readers}`; `SerdeSession::set_user_data`/`user_data` semantics (map by
  type); errors `SerializeError::{MissingProvenance, ProviderDropped}`,
  `DeserializeError::{MissingProvider, MissingDefinition}`,
  `RegistrationError::UnknownTableName`; `SerializableObject`/`DeserializableObject`
  impls on `Package`, `Scope`, `FallbackProvider`, `ErrorCallableSpec`,
  `SpecProvenance`, `StdCallableSpec`, and the seven latexlike spec types;
  `SerializableValue`/`DeserializableValue` impls on the latexlike vocabulary, ext,
  and invocation-syntax types; `impl SerializableLang for Latexlike`; feature-gated
  serde derives on the vocab types + `BodyMarker`. Next: M5 review → M6
  (diagnostics, ParseResult, streaming, Q3 naming pass). Blockers: none.
- 2026-08-17 — M4 fix-pass agent — **M4 fix pass complete** on `techy-serialize`
  (worktree `.claude/worktrees/techy-serialize`). Commits: `953810c`
  `SerializeError::InNode` / `DeserializeError::InNode { node, callable, cause }`
  (mirroring `InTable`/`InEntry`; the table/entry wrappers may now carry an `InNode`
  whose own cause is another table's location); `e4480f3` the tree driver fix pass;
  `7956503` the tests. **Blocking findings:** B1 — the reader tracks child claims
  while staging: every non-root wire node must be listed by exactly one node stored
  before it (unclaimed → typed error naming the node; the builder's drop-unreachable
  convenience can no longer accept a shortened root range), plus a post-`finish`
  node-count re-check (`Internal`); B2 — every per-node failure, write and read
  (payload, argument-spec hooks, nested table interning/reading, annotations), is
  wrapped in `InNode` — the D21 out-of-band write error now reads "…table `trees`:
  …node #1 (callable `x`)…: argument #1…"; B3 — the content-parent message states
  the real contract (a descendant, stored AFTER the callable; out-of-range and
  stored-before cases distinguished). **The lang-opaque span rule (D23/D25
  refinement, plan patch pending):** `TextContent`'s public value conversion is
  owned-only both ways (a `Spanned` value is a typed error on write and on read —
  the conversion receives no node to validate a range against); the tree writer
  materializes each callable's invocation syntax against the node's source
  (`InvocationSyntax::materialized`) before converting it, so a span-backed
  post-space round-trips as owned text (the deep-compare compares materialized
  invocation syntaxes); the public `Span` value-trait impls (unused; a bare span in a
  payload cannot be validated) are REMOVED; the ext-values contract ("must not carry
  node-relative spans", as `NodeTree::materialize` states) is documented on
  `TreeSerdeDriver` and `register_annotation`; the node's own text payloads
  (Chars/Group/Comment) stay span-backed on the wire, validated by the reader +
  builder. **Nits:** builder errors reach users with wire node positions and the
  `NodeBuildError` kept as `Failed { cause }`; an ext on an absent argument is
  rejected; `tree::<WrongA>()` names the stored identifier and the requested type; a
  non-tree object interned into the trees table is named as such (`TreeSerdeDriver`
  docs state `Object = dyn Any + Send + Sync` accepts `NodeTree<L, A>` only); the
  trees table now registers `core.tree` ITSELF (the registry is created with it —
  decision: driver-level, so `SerdeSession::empty()` + `register_table(TreeSerdeDriver::new())`
  needs no extra call; `register_core_tree` gone); unused test imports; "ride as"
  wording. **Tests (+14, reviewer probes included):** unreachable nodes, root claimed
  as a child, content parent outside its region / stored before its callable / out
  of range, region content out of bounds, overlapping regions, ext on an absent
  argument, annotation-count mismatch, non-tree object, 60k-deep chain round trip,
  session composed from `empty()`, node-location asserts on write and read errors,
  the three span-rule tests (toy `SyntaxLang` with a `TextContent`-carrying
  invocation syntax). Not changed (recorded): the reader accepts any wire order in
  which children follow their parent (not only breadth-first) — safe, since regions
  map `content_parent` through the staging map; the wire form is unchanged (Q3 note
  C on an explicit content-frame tag remains open). Verified: `cargo test` green
  without/with `--features serde` (940/969 unit — the M4 entry's "954" was 955 —,
  30+8+13+23+1 integration, 72/73 doctests); `rm -rf target/doc && cargo docs` clean
  both states; clippy clean in `serialize/`. Next: M5 (specs, providers, latexlike).
  Blockers: none.
- 2026-08-17 — M4 implementer agent — **M4 complete** on `techy-serialize` (worktree
  `.claude/worktrees/techy-serialize`). Commits: `58d4c20` M3 review nits (char
  mismatch uses `kind_name()`; source/object/mod doc wording; two state-reading
  tests — a missing section reads as a present feature's empty rules; an
  `expecting_close` matching no rule reads with a fresh handle); `d544935` the tree
  driver + wire structs + annotation codecs + `serialize_tree`/`tree` sugar; `dd0ed0b`
  the deep-compare helper, the round-trip harness, and the test corpus + hostile
  battery (+ the M3 pinned-segment snapshot gains the empty `trees` directory row);
  `6ce4ecc` module-doc pass for the trees table; `3227193` the feature-gated
  `register_serde_annotation`. Verified: `cargo build`/`test` green with and without
  `--features serde` (926/954 unit incl. the 27/29-test tree battery, 30+8+13+23+1
  integration, 72/73 doctests); `rm -rf target/doc && cargo docs` clean both states;
  clippy clean on all new code (the pre-existing `never_loop` in `latexlike/mod.rs`
  is untouched). **What M4 built** (plan §7 M4, `serialize/wire/tree.rs`,
  `serialize/drivers/tree.rs`, `serialize/tree_support.rs`,
  `serialize/drivers/tree_tests.rs`):
  - **Trees table (ordinal 4, `"trees"`, heterogeneous by annotation type).**
    `TreeSerdeDriver<L>` with `type Object = dyn Any + Send + Sync`,
    `type Index = TreeIndex`. The entry identifier names the ANNOTATION type's codec
    (D23): `()` pre-registered as `"core.tree"` (annotations omitted on the wire);
    other `A`s via `TableHandle::register_annotation::<A>(session, id)` (the
    `SerializableValue`/`DeserializableValue` value traits ARE the codec — an `A` that
    is a `SourceSpan` interns its source through `cx`), plus feature-gated
    `register_serde_annotation::<A>` for plain-data `A: Serialize + DeserializeOwned +
    …` through the bridge. Write dispatches on `(**object).type_id()`; read on
    `entry.identifier`; unregistered type → typed `SerializeError`, unknown identifier
    → `DeserializeError::UnknownIdentifier { table: "trees", … }`. The registry rides
    on a generalized `TableRegistry` trait (the old `ReadDispatchState`, renamed and
    widened to `pub(crate)` so a custom driver can keep its own registrations); the
    trees registry memoizes nothing.
  - **Sugar (extension trait `TreeSerialization` on `SerdeSession<L>`, session-level
    only — the context forms were unused, skipped):** `serialize_tree(&NodeTree<L, A>)
    -> Result<TreeIndex, _>` (wraps the tree in a fresh `Arc<dyn Any>` and interns —
    every call a new entry, trees are values; documented) and `tree::<A>(TreeIndex) ->
    Result<NodeTree<L, A>, _>` (downcast; wrong `A` → `Failed`).
  - **Wire form (`WireTree { nodes: [WireNode…] in storage order, annotations:
    Option<[value…]> }`, annotations omitted for `()`).** Per node: `kind` (chars |
    group | callable | comment | list), `span` (the M3 `WireSpan {source, start,
    end}`), `state` (`StateIndex`), `ext` (`NodeExt` value conversion), `children`
    (storage range `[start,end)`). Callable payload: `callable_type`/`invocation_syntax`
    (value conversions), `name`, `spec` (`SpecIndex`), `arguments: [{region?, ext?,
    spec_payload?}]`, `slots: [{name?, region, role, ext}]`. `TextContent`, `Span`,
    `SlotRole` gained context-free internal wire codecs and (delegating) value-trait
    codecs; `GroupRule<L>` a value-trait codec — the M5-inlining surface of D23.
  - **Region coordinate choice (Q3-relevant, decided + recorded):** a region is
    `{children: [start,end) offsets into the callable's OWN child list, content:
    [start,end) offsets, content_parent: u32 storage index}`. `content_parent ==` the
    callable's storage index ⇔ `InRegion` (offsets within the region's node list);
    otherwise ⇔ `InChildrenOf` (offsets within that node's children). The reader maps
    `content_parent` through its staging map to a `BuildId`, so correctness does NOT
    depend on storage-index preservation — only the presence of every referenced node.
  - **Reader = builder-style rebuild (D22):** validates node count and each `children`
    range (in bounds, strictly after the parent, each child claimed once — the builder
    re-checks), stages in REVERSE storage order (children before parents), per node:
    source/state/spec via `cx`, values via the value traits, span validated by the M3
    `deserialize_span` before `SourceSpan::new`, `TextContent` spans validated
    `start<=end` before `Span::new` then residency-checked by the builder; then
    `finish(root)` (fresh tag, parent table + `single_source` recomputed) and
    `validate_tree` as a final defense-in-depth check. Iterative throughout (no
    recursion proportional to tree size); the child-collection `Vec` is capacity-bounded
    by the validated node count, so a huge `children.end` errors without allocating.
  - **D21 pair exercised:** default index rule (write returns `Ok(None)`, read clones
    `arguments()[i]`), an overriding toy spec (`OobSpec` round-trips an out-of-band
    argument spec), out-of-band without an override → write `ArgumentSpecOutOfBand`
    inside `InTable{"trees"}`, an unexpected payload at the default →
    `UnexpectedArgumentSpecPayload`, an index past `arguments().len()` →
    `ArgumentIndexOutOfRange`. **Deviation (recorded):** the write error is NOT wrapped
    to name the node+callable (plan §7-step-3 wording) — wrapping in `Failed` would
    forfeit the typed variant the tests match on; the typed variant + `InTable{"trees"}`
    location is kept instead. Per-node naming would need a dedicated wrapper/variant;
    left as a small follow-up.
  - **Argument ext presence follows the region, not an `Option` key:** a provided
    argument whose ext serializes to `Null` (the unit ext) omits the wire key and reads
    the ext back from `Null` (so `Some(Null)` vs `None` does not corrupt it) — the one
    non-obvious wire subtlety.
  - **Test support (reused by M5):** `serialize/tree_support.rs` (`#[cfg(test)]
    pub(crate)`) — `assert_trees_equivalent` (node count; kind + resolved payload text;
    region fields; slot names/roles; `Debug` of specs/exts/invocation-syntax; spans by
    offset + source content; states by `Debug`; the `Arc::ptr_eq` topology classes of
    states/specs/sources; parent tables; `single_source`; annotations via callback) and
    the generic `round_trip_tree<L, A>(setup, tree, eq)` harness (serialize → segment →
    JSON under the feature → new session → `push_segment` → `tree()` → deep-compare).
    M5 reuses the harness with a latexlike `setup` and parsed inputs.
  - **Corpus-at-M5 scheduling note:** the M4 corpus is toy-lang parses (a `ParseLang`
    with `{}`/`\`/`%` rules resolving `\hi`/`\emph{x}` through a toy provider) plus
    hand-built trees covering every node kind and region shape (`InRegion` +
    `InChildrenOf`, all three slot roles, absent + token + group arguments, nested
    callables, multi-source, `Owned` text, `SourceSpan` and plain-data annotations).
    The REAL latexlike corpus runs at M5 (its spec/provider `SerializableObject`/
    `DeserializableObject` impls do not exist yet); M5 only adds inputs to the existing
    harness.
  **Provisional wire names (Q3):** table `trees`; identifier `core.tree`; node keys
  `kind`, `span`, `state`, `ext`, `children` (`start`/`end`); kind variants `chars`,
  `group`, `callable`, `comment`, `list`; group keys `group_type`/`open`/`close`;
  callable keys `callable_type`/`name`/`spec`/`arguments`/`slots`/`invocation_syntax`;
  argument keys `region`/`ext`/`spec_payload`; slot keys `name`/`region`/`role`/`ext`
  (`content`/`attached`/`hidden`); region keys `children`/`content`/`content_parent`;
  `TextContent` `spanned {start,end}`/`owned`; tree keys `nodes`/`annotations`.
  **Public API surface (new):** `TreeSerdeDriver<L>` (`new`, `Default`); `TreeIndex`;
  `TreeSerialization` (`serialize_tree`, `tree`); `TableHandle::register_annotation`,
  `TableHandle::register_serde_annotation` (feature-gated); `StandardTables.trees`
  field; `SerdeSession::new` now registers the trees table (ordinal 4);
  `RegistrationError::DuplicateAnnotationType { table }`; `SerializableValue`/
  `DeserializableValue` impls for `TextContent`, `Span`, `SlotRole`, `GroupRule<L>`;
  `SerializableObject`/`DeserializableObject` impls — none new on core public types (the
  tree driver interns states/specs/sources, none needing a new object impl). Crate-
  internal only: `NodeData` re-export widened from `#[cfg(test)]` to unconditional
  `pub(crate)` (the tree driver walks the flat storage); the `TableRegistry` trait
  rename; `SerdeSession::{table_index, table_name}` widened to `pub(crate)`; both
  contexts' `session_mut` widened to `pub(crate)`. Next: M4 review → M5 (specs,
  providers, latexlike). Blockers: none.

- 2026-08-17 — M3 implementer agent — **M3 complete** on `techy-serialize` (worktree
  `.claude/worktrees/techy-serialize`). Commits: `1d25304` the D17 REVISED value
  traits + `SerializableLang` bounds; `f401f3b` source & state drivers, standard
  tables, by-kind accessors, tests; plus the docs commit carrying this entry.
  Verified: `cargo build`/`test` green with and without `--features serde` (897/924
  unit incl. the 19/20-test drivers battery, 30+8+13+23+1 integration, 72/73
  doctests); `rm -rf target/doc && cargo docs` clean both states; clippy clean on
  the new code (pre-existing findings elsewhere untouched). **What M3 built:**
  - **Part 0 (D17 REVISED):** `SerializableValue<L: Lang>` /
    `DeserializableValue<L: Lang>` in `serialize/object.rs`, core impls blanket over
    `L: Lang` for `()`↔`Null`, `bool`, every integer width (`Int`, range-checked on
    read), `String`↔`Str`, `Option<T>` (`None`↔`Null`, `Some`↔`T`'s form — covers the
    default `SourceOrigin`), `Vec<T>`↔`List`. `SerializableLang` now carries the
    associated-type bounds in supertrait position, nested `NodeExts: NodeExtTypes<…>`
    form included. **Probe result:** the nested form compiles AND elaborates on the
    installed rustc 1.97 (a fn bounded only by `SerializableLang` calls
    `serialize_value` on `L::ModeId` and on the bundle's exts; a lang whose `ModeId`
    lacks the codec cannot opt in — E0277) — no where-clause fallback needed; MSRV
    1.86 still not locally verifiable (no 1.86 toolchain; the feature is stable since
    1.79). `TrivialLang`-based langs opt in with an empty impl (M2's `ToyLang`, M0's
    `OptedInLang` unchanged; `NeverSerializableLang` still does not implement it).
    Also `SourceSpan<L::SourceOrigin>` implements both value traits (its source
    interned by position, range validated on read) — the first value that refers to
    a table object, and what D23's span-shaped annotations will use.
  - **Sources (D25)** — `serialize/drivers/source.rs` + `wire/source.rs`:
    `SourceSerdeDriver<L>` (table `"sources"`, identifier `"core.source"`,
    `SourceIndex`); wire struct `{origin, provenance, line_number_offset,
    column_number_offset, text}` with `provenance = "primary" | {"resolved":
    {reference, triggered_at}} | {"synthesized": {description, triggered_at}}`,
    `triggered_at = {source: <position>, start, end}` (provenance chains are
    source references, acyclic; the parent source is interned first — post-order),
    and `text = {"embedded": <str>} | {"referenced": {length, digest?}}`,
    `digest = {algorithm, bytes: <Bytes>}` (an enum field, so "exactly one form" is
    validated by the derive). Caller-supplied, on the driver: `SourceTextPolicy<O>`
    (`text_form(&Source<O>) -> Result<SourceTextForm, SerializeError>`;
    `SourceTextForm::{Embedded, Referenced { digest: Option<SourceDigest> }}`;
    `SourceDigest { algorithm: String, bytes: Vec<u8> }`) and
    `SourceTextSupplier<O>` (`source_text(&ReferencedSource<O>) -> Result<String,
    DeserializeError>` + `digest_matches(&SourceDigest, &str) -> Result<bool,
    DeserializeError>` — `Ok(false)` = mismatch (the driver mints the typed error),
    `Err` = cannot check, e.g. unknown algorithm); `ReferencedSource<O>` = origin,
    provenance (already rebuilt), length, digest, offsets (getters). Defaults: embed
    all; referenced source without a supplier → `NoSourceTextSupplier`. The driver
    checks `text.len() == length` itself. Reads rebuild
    `Source::new(text).with_origin(..).with_provenance(..).with_line_column_number_offsets(..)`;
    every span is validated (`start <= end <= len`, both on char boundaries) BEFORE
    `SourceSpan::new`. `Arc` interning preserves `same_source` across the stream.
    **No plain-trait `SerializableObject`/`DeserializableObject` impls for `Source`:**
    the embed/reference decision is driver configuration (policy/supplier), so an
    object-level impl would either ignore it or reach into the driver — the driver
    does the work directly (D11's homogeneous route). `ParsingState<L>` DOES carry
    both plain-trait impls (`Output = ParsingState<L>`); the state driver delegates.
  - **States (D24)** — `drivers/state.rs` + `wire/state.rs`: `StateSerdeDriver<L>`
    (table `"states"`, identifier `"core.state"`, `StateIndex`); wire struct
    `{rules, mode, ext, scopes}` — `rules` = the seven sections, each `Option`al
    and written only when the lang's feature store is present (`whitespace {enabled,
    chars}`, `paragraphs {enabled}`, `groups {enabled, rules: [{group_type, open,
    close}], temporary: [..], expecting_close?: {…}}`, `commands {enabled, rules:
    [{escape_char (one-char string), name_chars}]}`, `comments {enabled, rules:
    [{start}]}`, `specials {enabled}`, `forbidden_chars {chars}`); `mode`/`ext`/
    `group_type` are `SerialValue`s produced by the lang's value conversions
    (the internal derive has no generics, so lang-typed parts ride as verbatim
    `SerialValue` fields); `scopes: [<provider position>]` outermost first (insertion
    order). Read: a section for a feature the target lang declares absent →
    `DeserializeError::FeatureAbsent { feature }`; a MISSING section for a present
    feature → that section's `empty()` (decided: the plan says all sections are
    optional; the empty block is the neutral value and what a lang without data
    would have written) — documented on the driver; rebuild via
    `ParsingState::new(data)` (promoted from `#[cfg(test)]` to plain `pub(crate)`,
    docs updated) — freezes and rebuilds `prefix_table`/`trigger_chars`, does NOT run
    `finalize_transition`; scope stack via new `pub(crate)
    ScopeStack::from_providers(Vec<Arc<dyn SpecsProvider<L>>>) -> Option<_>` (`None`
    for a non-empty stack under a scopes-less lang → `FeatureAbsent { feature:
    "scopes" }`); `expecting_close` re-linked to the value-equal rebuilt `Arc` in
    `temporary` (searched first) then `rules` (the nice-to-have — done; the derived
    temporary-scope check works on a rebuilt state, tested). `ParsingStateDelta`
    and the derived caches never hit the wire. Wire traits gained `char` (a
    one-character string; strict on read).
  - **Standard tables (D9)** — `drivers/standard.rs`: `SerdeSession::<L>::new()`
    registers `"sources"`(0), `"states"`(1), `"specs"`(2), `"providers"`(3) — the
    last two `DispatchingSerdeDriver` instances over `dyn CallableSpec<L>` /
    `dyn SpecsProvider<L>` behind the type aliases `SpecSerdeDriver<L>` /
    `ProviderSerdeDriver<L>` (§3.G's dispatching-driver names) with `SpecIndex` /
    `ProviderIndex`; NO readers registered (M5's job; M3 tests register a toy
    provider through `register_type`). `SerdeSession::with_source_driver(driver)`
    is the same constructor with a configured source driver (house precedent for
    `with_x(arg)` constructors: `Diagnostics::with_limit`, `GroupArgumentParser::with_rule`);
    `impl Default for SerdeSession` = `new()`. `SerdeSession::standard_tables() ->
    Option<StandardTables<L>>` (`#[non_exhaustive]` struct of the four handles,
    Copy) — found by name+driver type through the new generic
    `SerdeSession::table_handle::<D>(name) -> Option<TableHandle<D>>` (also on
    both contexts): the one lookup mechanism, usable by any custom driver that
    refers to another table without holding its handle. `empty()` kept.
  - **Context/session extension traits (§2 sugar):** `StandardTableInterning<L>`
    (`intern_source`, `intern_state`, `intern_spec`, `intern_provider`) implemented
    for `SerdeSession<L>` AND `SerializeContext<'_, L>`; `StandardTableReading<L>`
    (`source`, `state`, `spec`, `provider`) for `SerdeSession<L>` AND
    `DeserializeContext<'_, L>` — thin wrappers looking the standard table up by
    name; a missing table is the new `SerializeError::UnknownTableName { name }` /
    the (doc-generalized) `DeserializeError::UnknownTableName { name }`. The
    standard drivers themselves use these accessors (no handles stored in drivers),
    so they work in any session that registers the standard tables by name.
  - **New errors (D27):** `SerializeError::UnknownTableName { name: String }`;
    `DeserializeError::{SpanOutOfBounds { start, end, len }, SpanNotOnCharBoundary
    { start, end }, FeatureAbsent { feature: &'static str }, NoSourceTextSupplier {
    origin: Option<String> }, SourceLengthMismatch { origin, expected, found },
    SourceDigestMismatch { origin, algorithm }}` — every source failure names the
    origin label; the session's `InEntry` wrapper adds table + position. Supplier
    errors pass through unwrapped (their own words, inside `InEntry`).
  - **Tests** (`drivers/tests.rs`, both feature states): embedded round trip
    (origin, offsets, provenance, same-Arc reads); provenance chains (A ← B ← C, D
    also ← A; post-order positions; `same_source` and `Arc::ptr_eq` after read;
    chain iterators); referenced round trips with/without a toy `toy-sum` digest via
    a toy supplier; the supplier receives everything the entry recorded; typed
    failures (no supplier, length mismatch, digest mismatch, supplier failure,
    refused algorithm, failing write policy, `InEntry`/`InTable` locations); hostile
    spans/shapes (end beyond, start > end, inside a char, negative offset, position
    beyond the table, wrong table, bad text-form variant, unknown key, non-map);
    `SourceSpan` as a value (two spans → one entry); full-state round trip (all
    seven sections incl. temporary + expecting_close, `TokenRules ==`, re-linked
    `expecting_close`, providers identity-resolved from user data, prefix table
    and trigger chars rebuilt, further derivation drops temporaries); seed/empty
    state; shared states/providers keep identity; feature-absent refusals
    (`whitespace` section into `NoLangFeatures`, non-empty scopes into a scopes-less
    lang; a no-section state reads fine; a `NoLangFeatures` writer writes no
    section); bad provider references (`IndexOutOfRange`, `UnknownIdentifier`,
    environment miss); hostile state shapes; determinism (two sessions → equal
    segments); core value impls incl. range checks; `new()` order + `empty()`
    accessors; pinned JSON rendering of one source (referenced + digest, plus a
    synthesized child) and one state under the feature.
  **Provisional wire names (Q3):** tables `sources`/`states`/`specs`/`providers`;
  identifiers `core.source`/`core.state`; source keys `origin`, `provenance`
  (`primary`/`resolved`/`synthesized`, `reference`, `description`, `triggered_at`,
  span keys `source`/`start`/`end`), `line_number_offset`, `column_number_offset`,
  `text` (`embedded`/`referenced`, `length`, `digest`, `algorithm`, `bytes`); state
  keys `rules` (`whitespace`, `paragraphs`, `groups`, `commands`, `comments`,
  `specials`, `forbidden_chars`; `enabled`, `chars`, `rules`, `temporary`,
  `expecting_close`, `group_type`, `open`, `close`, `escape_char`, `name_chars`,
  `start`), `mode`, `ext`, `scopes`. **Public API surface (new):**
  `SerializableValue`, `DeserializableValue` (+ `SerializableLang` bounds);
  `SourceSerdeDriver` (`new`, `with_text_policy`, `with_text_supplier`, `Default`),
  `SourceIndex`, `SourceTextForm`, `SourceTextPolicy`, `SourceTextSupplier`,
  `ReferencedSource` (getters), `SourceDigest` (`new`, pub fields);
  `StateSerdeDriver` (`new`, `Default`), `StateIndex`; `SpecSerdeDriver`,
  `ProviderSerdeDriver` (aliases), `SpecIndex`, `ProviderIndex`; `StandardTables`;
  `SerdeSession::{new, with_source_driver, standard_tables, table_handle}`,
  `Default for SerdeSession`; `SerializeContext::table_handle`,
  `DeserializeContext::table_handle`; `StandardTableInterning`,
  `StandardTableReading`; the error variants above; value-trait impls on
  `SourceSpan`, object-trait impls on `ParsingState`. Next: M3 review → M4 (trees;
  `serialize_span`/`deserialize_span` in `drivers/source.rs` are the `pub(crate)`
  span helpers for node spans) → M5 (real spec/provider impls; readers on the
  specs/providers tables). Blockers: none.
- 2026-08-16 — M2 fix-pass agent — **M2 fix pass** on `techy-serialize` (worktree
  `.claude/worktrees/techy-serialize`), applying the M2 review's three blocking
  findings, the user-approved rename, and the nits. Commits: `0ee9b20`
  `SerdeSession::resolve`/`DeserializeContext::resolve` → **`object`** ("returns the
  object stored at a position, rebuilding it from its wire entry on first use" — now
  defined in the module vocabulary; "resolve/resolver" means identifier resolution
  only, `IdentifierResolver::resolve` stays), typed positions declared
  **session-scoped** on `SerialIndex`/`TableId`/`TableHandle`/`Segment`/`object`/
  `WrongTable` (a position travels between sessions as `(table name, u32)` —
  `SerialIndex::index` + `ObjectSerdeDriver::table_name`), new
  `TableHandle::position(index: u32) -> D::Index` (rebuild on the receiving side; no
  bounds check, validated on use; `L` inferred from the driver's impl), remapping
  tests (reader with a permuted registration order: every nested reference
  translated, sharing preserved; read-then-append across a permuted reader into a
  third order; `position` across two sessions); `961e366` **B1** — a failed
  rebuilding restores `Slot::Pending(stored value)` on every error path (driver
  error, malformed heterogeneous entry, driver downcast), so a nested failure a
  referring driver swallows cannot poison the slot as `InProgress` (which misreported
  as a self-naming `ReferenceCycle` and let `push_segment` return `Ok` with an
  unrebuilt entry); the eager pass meets the still-pending entry in its own turn and
  the push fails with the driver's error; post-pass defense-in-depth check → new
  `DeserializeError::Internal { detail }` (a bug of this crate, reported not
  panicked); test with a driver that swallows nested failures; `f2e72f8` read
  dispatch memoizes resolver **declines** as well as readers (D15, memo map separate
  from registered readers; a kept reader still makes a later registration a
  `DuplicateIdentifier`, a kept decline does not), rollback of a failed push also
  **forgets the memoized answers** recorded during it (session-level journal + erased
  `ReadDispatchState::forget_memo`), `DispatchingSerdeDriver::deserialize_object`
  finds its readers **by its own table name** in the context's session (works from
  any deserialization context; `DeserializeContext::current()` dropped), "fail-closed"
  and "namespace" defined at first use; `a167957` `register_table` refuses
  `homogeneous_identifier() == Some("")` (new
  `RegistrationError::EmptyHomogeneousIdentifier { table }`), `take_segment`
  invariant explicit (`debug_assert!` + `saturating_sub`), `push_segment` doc states
  the one-stream-in-order caller obligation (also in the module vocabulary and on
  `Segment`), `Segment::to_serial_value`'s `expect` invariant stated in one line,
  homogeneous/heterogeneous defined in the module vocabulary, test gaps filled
  (`UnexpectedIdentifier`, write-side `DescentLimitExceeded`,
  `DuplicateSegmentTable`, `DuplicateIdentifier`, longest-prefix-then-registration-
  order with overlapping resolvers + registered-reader precedence, direct
  `register_reader`/`ObjectReader::new`). **New public items** (for the naming
  check-in): `TableHandle::position`, `DeserializeError::Internal { detail }`,
  `RegistrationError::EmptyHomogeneousIdentifier { table }`. Verified: `cargo
  build`/`test` green with and without `--features serde` (878/904 unit incl. the
  34-test engine battery, 30+8+13+23+1 integration, 72/73 doctests); `rm -rf
  target/doc && cargo docs` clean both states; clippy clean on the serialize code.
  Deliberately unchanged: a segment listing an EMPTY part for a table the reader
  has not registered is still `UnknownTableName` (a reader must know every table of
  its writer by name — kept as M2 documented it; the new read-then-append test
  registers the extra table on every side); the `UnknownTable` reports on the two
  driver-downcast paths (unreachable by construction; not switched to `Internal`).
  Next: M2 re-review of the fix pass → user naming check-in → M3/M4.
- 2026-08-16 — M2 implementer agent — **M2 complete** on `techy-serialize` (worktree
  `.claude/worktrees/techy-serialize`). Commits: `8cb629b` M1 review nits (bridge's
  `&SerialValue` now implements the integer `deserialize_*` methods itself with range
  checks → `IntegerOutOfRange { target }`; `deserialize_struct` requires a `Map`;
  write-side length-hint preallocation capped like the read side; variant rename
  `ArgumentSpecPayloadUnexpected`→`UnexpectedArgumentSpecPayload`, Display "does not
  override"; doc wording — "wire" defined once, "byte-string channel" dropped,
  "externally tagged" glossed, "transparent in every format" softened, `to_value`
  payload cautions for `Index`/`Option`); `7f595ed` the engine; `b7ffcbf` feature-
  gated segment/position rendering tests + clippy tidy. Verified: `cargo build`/`test`
  green with and without `--features serde` (865/891 unit incl. the 24-test engine
  battery, 30+8+13+23+1 integration, 72/73 doctests); `rm -rf target/doc && cargo
  docs` clean both states; clippy clean on all new code (pre-existing `never_loop` in
  latexlike untouched). **What M2 built** (foundation engine, plan §6 module layout —
  `serialize/engine/{session,driver,context,dispatch,segment,tests}.rs`, plus
  `serialize/serial_index.rs`):
  - `SerdeSession<L>` (D9): tables in registration order, each a driver `Arc<dyn Any>`
    + name + homogeneous-identifier + `Vec<Slot>` (Pending(wire)|InProgress|
    Materialized(`Arc`)) + pointer→position map + pending-emission outbox; user-data
    slot; per-run `StdDescentGuard` (shared with the parser's, configured via
    `with_descent_guard_init`); write-side in-progress stack. Constructor `empty()`
    (D9 keeps `new()` for the standard-tables one). `intern`/`resolve`/`take_segment`/
    `push_segment`; `set_user_data`/`user_data`.
  - `ObjectSerdeDriver<L>` (D11): `type Object: ?Sized`, `type Index: SerialIndex`,
    `table_name`, `homogeneous_identifier`, `serialize_object`/`deserialize_object`.
    `TableHandle<D>`: `TableId` + `PhantomData`, Copy/Eq/Hash, validated per session
    (ordinal + driver `TypeId`). `TableId`s are registration ordinals (D5).
  - Write path (D12): pointer hit → existing position; miss → in-progress marker,
    driver call (may recurse), THEN assign position (post-order → backward refs);
    re-entering an in-progress object → `ReferenceCycle` naming both tables.
  - `Segment { version, tables: Vec<SegmentTable { name, id, start, entries }> }`
    (D10): entries the STORED wire form (bare data homogeneous, `{id,data}` hetero via
    an internal `WireEntry` derive); `take_segment` drains each table's outbox and
    advances; `push_segment` validates (version==`Segment::VERSION`==1, table-by-NAME,
    `start`==current len, no unemitted entries anywhere, table-full, dup-table),
    rewrites every `SerialValue::Index` writer-id→reader-ordinal (iterative, no
    recursion), appends Pending, materializes eagerly with lazy on-demand resolution;
    ANY error rolls back (truncate slots, drop new pointer entries) and the session
    stays usable. Unconditional to/from-`SerialValue`; feature-gated serde DELEGATES
    to `SerialValue`'s impls (one rendering path).
  - Read dispatch (D11/D15): `DispatchingSerdeDriver<L, dyn T, I>` — write via the
    object's own `SerializableObject::serialize_object` (supertrait vtable); read =
    registered `ObjectReader`s (exact map) → `IdentifierResolver`s (longest matching
    prefix, then registration order; answer memoized per identifier) → fail-closed
    `UnknownIdentifier`. `TableHandle::register_type::<C>(session, id, wrap)` /
    `register_reader` / `register_resolver`. No write-side resolvers (D16).
  - `SerialIndex` bound gained `from_parts`/`table`/`index`; the `serial_index!`
    macro (crate-root export, canonical path `techy::serialize::serial_index`) defines
    a position newtype with derives + the wire traits + feature-gated serde via M1's
    index sentinel, reaching techy internals through `techy::__private` (promoted
    `ToSerialValue`/`FromSerialValue`/`index_from_serial_value` and, feature-gated,
    `serialize_index`/`deserialize_index`/`serde`). `TableId::ordinal()` now `pub`.
  - Errors (D27): `SerializeError`/`DeserializeError` extended, `#[non_exhaustive]`,
    each Display names the culprit; location wrappers `InTable`/`InEntry`; a `Failed
    { detail, cause: Option<Arc<dyn Error>> }` variant mirroring `HookFailed` —
    **this forfeits derived `PartialEq`/`Eq` on both error enums** (dropped; tests use
    `matches!`), as D27/the brief permit. `From<SerialValueError>` both ways. New
    `RegistrationError` (still `PartialEq`).
  - Contexts (D17): `SerializeContext`/`DeserializeContext` now wrap the session
    borrow + guard; public `intern`/`resolve` + `user_data`; still constructible only
    for `L: SerializableLang`.
  **Decisions / provisional shapes:** (1) `homogeneous_identifier` mismatch is a
  runtime `UnexpectedIdentifier` error, not a debug-assert (stricter, panic-free —
  driver bug reported, never panicked). (2) A typed position serialized DIRECTLY by a
  serde format is its bare newtype pair (`[t,i]`); the `{"$index":[t,i]}` canonical
  form appears only when the position rides inside a `SerialValue` (as it always does
  on the wire, inside a `Segment`) — pinned by a test. (3) `push_segment` requires the
  whole session to have no pending-emission entries (absorb-all-then-append); looser
  per-table checking was considered and rejected as unclear. (4) Read dispatch
  prefix rule fixed: longest matching prefix wins, registration order breaks ties
  (documented). (5) Q6 PROPOSED (§4): version in every segment, JSONL one-per-line no
  EOF marker, `take_segment`/`push_segment` kept, full directory every segment. Next:
  M2 review → user naming check-in (the public API surface list is in the M2 report)
  → M3 (sources & states) / M4 (trees), internally parallel. Blockers: none —
  awaiting the user's naming confirmation on the new public items before M3/M4 lean on
  them.
- 2026-08-16 — M1 implementer agent — **M1 complete** on `techy-serialize` (worktree
  `.claude/worktrees/techy-serialize`). Commits: `a987229` M0 review nits
  (saturating index Display, doc wording, "reading environment" defined in the
  module docs); `0b6914d` D21 read default fail-closed (`Some(_)` payload →
  `DeserializeError::ArgumentSpecPayloadUnexpected { index }`, checked before the
  bounds check); `3d2d4e9` `SerialValueError` (unconditional) + `bridge.rs` +
  `render.rs` + `base64.rs` (feature-gated) + dev-deps `serde_json`/`postcard`
  (tests only) + the serde test battery; `55171a6` wire traits + internal derives
  (D8); `d9d8751` docs (lib.rs feature note, module overview, `SerialValue`
  rendering section). Verified: `cargo build`/`test` green with and without
  `--features serde` (844 / 866 unit incl. proptest identity, 74 integration, 70 /
  71 doctests), `rm -rf target/doc && cargo docs` clean both states, clippy clean
  on the new code (lib + tests). **Names picked:** bridge `to_value` / `from_value`
  (`Deserializer<'de> for &'de SerialValue`, `is_human_readable() == true`); bytes
  helper module `techy::serialize::serial_bytes` (`#[serde(with = …)]`; strict:
  reads a byte string only); index sentinel `INDEX_SENTINEL =
  "techy::serialize::Index"` + `pub(crate) serialize_index(table, index, s)` /
  `deserialize_index(d) -> (TableId, u32)` (`TableId::ordinal()` added); value
  sentinel `VALUE_SENTINEL = "techy::serialize::SerialValue"` (see below);
  `SerialValueError` variants `FloatRejected`, `NonStringMapKey`,
  `IntegerOutOfRange { value: String, target: &'static str }`, `TypeMismatch {
  expected: Cow<'static, str>, found: &'static str }` (`found` = kind name via
  `pub(crate) SerialValue::kind_name()`: null/bool/int/str/bytes/list/map/index),
  `MissingField { name }`, `UnknownField { name, expected }`, `DuplicateField {
  name }` (added beyond the brief's list — strict reads reject repeated keys),
  `UnknownVariant { name, expected }`, `Custom(String)`; wire traits
  `wire::ToSerialValue` / `wire::FromSerialValue` (crate-private, in
  `serialize/wire/mod.rs`; the derives are re-exported there), attribute
  `#[serial(name = "…")]`, byte-string field newtype `wire::SerialBytes(Vec<u8>)`.
  **Decisions / provisional shapes:** (1) the bridge answers
  `is_human_readable() == true` (the value model is the in-memory form of the
  canonical JSON, so third-party types take their text forms); `SerialValue`'s own
  serde impls wrap the value in the `VALUE_SENTINEL` newtype struct (transparent in
  every format) and choose the rendering by the format's `is_human_readable()`; the
  bridge intercepts the sentinel and re-serializes/reads the payload in
  non-human-readable mode, unwrapping the compact form's enum name — so
  `to_value(&v) == v` and `from_value::<SerialValue>(&v) == v` hold exactly (tested
  by proptest), and a `SerialValue` field inside a payload is verbatim. (2) Canonical
  JSON as briefed (`{"$bytes": b64}`, `{"$index": [table, index]}`, `$`→`$$` key
  escaping, fail-closed reader; hand-written strict base64); compact = externally
  tagged enum (`Bytes` via `serialize_bytes`, `Map` as a serde map, `Index` via the
  index sentinel); postcard exercises it. (3) Bridge read strictness: `Bytes` are
  read only from `Bytes` (a `Vec<u8>` without `serial_bytes` reads only a `List`);
  `deserialize_f32/f64` → `FloatRejected` (symmetric with writes); an `Index`
  reaching `deserialize_any` (foreign/untyped visitors) is a `TypeMismatch` (least
  committing — a synthetic `$index` map was the alternative); trailing list
  elements / map entries a visitor leaves unread are errors; the index sentinel
  accepts only a real `Index` on read (not a `List` pair). (4) Internal derive:
  `to_serial_value` is FALLIBLE (`Result<SerialValue, SerialValueError>`) so every
  integer width can be a wire field with an error — never truncation — for values
  outside `i64` (M2/M3: `SerializeError` will need a `From<SerialValueError>`
  variant); `Option` omission is trait-based (`is_absent_field` /
  `from_serial_field`), not syntactic, and a present `Null` also reads as `None`;
  supported: named-field structs, enums with unit / newtype / struct variants
  (tuple variants rejected — use named fields); generated code uses
  `crate::serialize::…` paths (derive is techy-internal; `__private` untouched);
  `ensure_no_generics` in techy-derive gained a `reason` argument. (5) D7 note: the
  "by newtype-struct name" interception is the index sentinel; bytes use serde's
  native bytes channel + `serial_bytes` (no `serde_bytes` dependency). (6) M2's
  typed indices: implement serde via `serialize_index`/`deserialize_index` (feature
  gated) and the wire traits by hand (`SerialValue::Index` ↔ `{table, index}`), as
  the test doubles `TestIndex` (serde_tests.rs) and `TestPosition`
  (wire/tests.rs) show. (7) The wire module carries a module-level
  `#![allow(dead_code)]` until the first non-test wire structs (M3). Sandbox note:
  fetching the new dev-deps needed one `cargo fetch` outside the sandbox (registry
  cache write). Next: M1 review → M2 (engine). Blockers: none.
- 2026-08-17 — supervisor (main session) — M5 reviewed (Opus 5; APPROVE WITH NITS —
  one doc line; hostile-probe crate + cross-process determinism clean; nits folded
  into the M6 brief). Plan patches: Q5 RESOLVED, §7 M5 bullet re-scoped, Q3 list
  extended with the M5 identifiers. Design questions queued for the user: bare
  `SerdeSession::new()` does not pre-register core readers (reviewer judges it
  correct: language `register` helpers chain `register_core_readers`; needs the
  user's ack), `EndSpec`/`ParagraphBreakSpec` unstamped asymmetry, `KnownProviders`
  name, `builtin_package()`/`minilatex_package()` now returning `Arc<Package>`
  (breaking under the soft freeze — necessary for stamping), `StdCallableSpec.provenance`
  as a `pub` field, the `ArgumentExt: Default` bound on latexlike `register`. Next:
  M6.
- 2026-08-17 — supervisor (main session) — M4 reviewed (Opus 5; REQUEST CHANGES:
  unreachable wire nodes silently dropped, missing node/callable location context,
  one inverted message, plus a plan gap: span-backed fields inside lang-opaque
  payloads were unvalidated) → fix pass landed (see its entry) → re-verification.
  Plan patches: D22 exact-node-set rule, D23 lang-opaque span rule (owned text
  only; writer materializes invocation syntax), Q3 content-frame tag note, M4
  corpus-at-M5 note. Naming-pass items queued: `TreeSerialization` bundles both
  directions while M3's sugar traits are per direction. Next: M5 (awaiting the
  user's rulings on identity-only package specs, `Package::new_shared` stamping,
  typed user-data map + `KnownProviders`).
- 2026-08-17 — supervisor (main session) — M3 reviewed (Opus 5; APPROVE WITH NITS,
  hostile-input harness clean; nits folded into the M4 brief). Plan patches: D13
  `Source` has no plain impl (driver-direct), M7 cost bounds (prefix table O(n²),
  offsets), Q3 `Option`-rendering asymmetry note. M5 forward risk recorded for its
  brief: `L::specials_trigger_chars` runs on wire-controlled `StateData` once a
  preset opts in — must be total. Naming taste question queued for the user:
  `StandardTableInterning`/`StandardTableReading` (activity nouns) vs the house's
  agent/capability trait names. Next: M4 (trees).
- 2026-08-17 — supervisor (main session) — M2 reviewed by an Opus 5 reviewer
  (REQUEST CHANGES: poisoned-slot bug, untested table remapping, session-scoped
  positions undocumented) → fix pass landed (see its entry) → re-verification.
  User rulings: naming list accepted with `resolve` → `object` (session + context
  read accessor; `IdentifierResolver::resolve` stays); Q6 accepted as proposed;
  errors without `PartialEq` accepted. Plan patches: D12 write-side cycle wording +
  session-scoped positions, Q6 stream-identity obligation, Q3 `TableId` note, M7
  depth bound covers `Segment::from_serial_value`, §6 usage sketch. Open flags for
  the user (non-blocking): `TableHandle::register_reader` (a direct read-entry
  registration route beyond D15's two — kept), `serial_index!` having two public
  paths (`macro_rules!` export limitation; alternative: proc-macro in techy-derive).
  Next: M3.
- 2026-08-16 — supervisor (main session) — user rulings folded in: D17 REVISED
  ("option B": `SerializableValue`/`DeserializableValue` value traits +
  `SerializableLang` as bounds — D17/D23/D28/§3.G/§6/M3 texts patched); reviewer
  agents run on the Opus 5 model from the M2 review on. M2 implementer reported
  complete (see its entry); next: M2 review → user naming check-in on M2's public
  API → M3.
- 2026-08-16 — supervisor (main session) — M1 reviewed (APPROVE WITH NITS; nits
  folded into the M2 brief). Plan patches from the M1 review: D7 signature form
  (borrowed `from_value`), §6 `SerialIndex` comment, D8 Option-omission parity
  convention, M7 owns the `SerialValue` deserialize depth bound. Next: M2 (engine).
- 2026-08-16 — supervisor (main session) — plan revisions folded in after user
  approval: D11 typed indices carry `{table: TableId, index: u32}` (bridge/derive
  context-freedom — see D11 text); D21 read default made fail-closed for a `Some(_)`
  payload (applied in M1); D7 wording clarified (index sentinel + native bytes
  channel; `SerialValueError` unconditional). Process note: the supervisor role is
  held by the main session itself (implementer + reviewer agents per milestone,
  compact reports only). MSRV 1.86 not locally verifiable (only rustc 1.97
  installed) — flagged, not acted on. Next: M0 review report → M1.
- 2026-08-16 — M0 implementer agent — **M0 complete** on `techy-serialize` (worktree
  `.claude/worktrees/techy-serialize`). Commits: `a6bd995` cargo `serde` feature
  (optional dep, `[features] serde = ["dep:serde"]`, nothing cfg-gated yet);
  `00fed0f` `techy::serialize` module skeleton (value.rs / error.rs / object.rs /
  engine/mod.rs; facade + crate-doc "Cargo features" note in lib.rs); `3d924e6`
  `SerializableObject<L>` supertrait on `CallableSpec`/`SpecsProvider`, the D21 pair
  with the pinned default bodies, one-line stubs on all 12 non-test types + 18
  test-module types + the construct-parsers guide doctest; `dbdfadf` unit tests
  (vacant vtable via `NeverSerializableLang`, gate defaults through `dyn` for an
  opted-in test lang, D21 default bodies, value equality, error Display); plus this
  log entry (docs wording pass on `SerialValue`). Verified: `cargo build`/`test`
  green with and without `--features serde` (837 unit + 74 integration + 70
  doctests), `cargo docs` clean both states, clippy reports nothing in the new code
  (pre-existing findings elsewhere untouched). **Provisional shapes / decisions for
  later milestones:** (1) module wiring — the plan's "`pub(crate) mod serialize` +
  `pub mod serialize` facade" cannot both exist under one name at the crate root, so
  `serialize` follows the `source`/`error` own-facade pattern (public module, private
  submodules, re-exports; public paths unchanged: `techy::serialize::X`); (2)
  `SerializeError { Unsupported, ArgumentSpecOutOfBand { index, count } }` and
  `DeserializeError { ArgumentIndexOutOfRange { index, count } }`, both
  `#[non_exhaustive]` + `Clone, Debug, PartialEq, Eq` + hand-written Display/Error;
  the D27 "any implementer reason" variant is NOT added (no M0 caller) — M2 decides
  its shape; note that `HookFailed`'s `detail + Option<Arc<dyn Error>>` shape would
  forfeit derived `PartialEq`/`Eq`; (3) `TableId::new(u32)` is `pub(crate)` with
  `#[allow(dead_code)]` until the session (M2) mints ids; no getter yet; (4) contexts
  are `struct …Context<'a, L: SerializableLang> { _shell: PhantomData<&'a mut L> }`
  with `#[cfg(test)] pub(crate) fn shell()` constructors — M2 replaces the field and
  constructors with the session borrow; (5) the `deserialize_argument_spec` default
  ignores `value` exactly as D21 pins; whether a `Some(_)` payload reaching the
  default (writer overrode, reader's spec type did not — a spec-type mismatch) should
  be a fail-closed error is a question for M4 (would be a D21 refinement, escalate);
  (6) MSRV: the vacant-vtable test passes on rustc 1.97 (installed toolchain); no
  1.86 toolchain was available offline, so the D17 MSRV claim is not locally
  re-verified (vacant vtable slots for methods with unsatisfied where-clauses long
  predate 1.86 — low risk; an MSRV CI check is the durable fix). Next: M0 review →
  M1 (bridge + internal derive). Blockers: none.
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
