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
silently deviate from a D-number.

---

## 1. Mission

Provide serialization/deserialization for the objects techy consumers handle — node
trees (with annotations), parsing states, sources, specs/providers, diagnostics — via
serde, behind an optional `serde` cargo feature, with a schema that will become public
and frozen at v1 (not frozen yet).

### Canonical use cases (the design must serve all six)

1. **Tooling cache** — chunks/documents parsed and cached on disk; sources exist
   externally (reference + digest); same tool version both ends; binary format; corrupt
   cache degrades to reparse (reader = total validator, never panics).
2. **Full dump/load** — e.g. FLM serializes a full parse result to render later;
   self-contained (embedded sources); long-lived; version field + compatibility policy.
3. **IPC** — parser/formatter service feeding e.g. a website builder; batches of trees
   sharing tables; short-lived payloads.
4. **Golden-file / snapshot tests** — readable, diffable, deterministic output.
5. **Debug/inspection dumps** — write-only, human-readable.
6. **Cross-language consumers** — deferred; motivates the public-schema policy but is
   not a v1 requirement.

---

## 2. Architecture: three layers

```
┌────────────────────────────────────────────────────────────────────┐
│ implementer layer: preset/framework hooks, factories,             │
│   custom object tables (session.register_object_type(...))        │
├────────────────────────────────────────────────────────────────────┤
│ core serializers: sources, parsing states, specs, providers,      │
│   node trees, diagnostics — registered object kinds on the engine;│
│   "wire structs" = their typed entry schemas                      │
├────────────────────────────────────────────────────────────────────┤
│ engine: sessions & segments; named typed tables; interning by Arc │
│   pointer identity; append-only global indices; ref tokens;       │
│   deterministic order; cycle checks; SerialValue + serde bridge   │
└────────────────────────────────────────────────────────────────────┘
```

The engine is a general session/segment-based store for object graphs with custom
dump/revive hooks. Core types are implemented **on top of** it, as registered object
kinds — not as special cases inside it.

Public facade: `techy::serialize`. Internal module: `techy/src/serialize/` (pub(crate),
facade re-exports only — [§dd-dr:public-namespace-topology] discipline).

---

## 3. Decision register

Settled decisions. Each has a short rationale; the full discussion history lives in the
2026-08-13 design sessions and will be condensed into DESIGN_RATIONALE at M7.

### Dependency & schema policy

- **D1 — Optional serde dependency.** Cargo feature `serde` (off by default), dep
  `serde = { version = "1", optional = true, default-features = false, features =
  ["alloc", "derive"] }` — crate is `no_std`+`alloc`. Everything new is behind
  `#[cfg(feature = "serde")]`. First feature flag in the crate (anticipated by
  [§dd-dr:dependencies]). techy depends only on serde itself; format crates
  (serde_json, postcard, …) are chosen by consumers.
- **D2 — Public schema, frozen at v1 (not yet).** The public contract is the abstract
  document structure plus its canonical JSON rendering. Symbolic names, not integers:
  every wire enum variant and field name gets an explicit serde rename, chosen
  deliberately (same discipline as [§dd-dr:wire-identifier-stability]). Binary
  encodings (postcard etc.) are private same-version pairings; the frozen thing is the
  JSON shape. Until freeze: version field, breaking changes allowed.
- **D3 — No serde derives on live types.** Live model types (NodeTree, ParsingState,
  CallableData, …) never gain serde bounds. Serialization goes through an explicit
  wire model (typed entry schemas + conversions). Reasons: trait objects are
  load-bearing throughout the live model; Arc sharing is semantic (identity-based
  SourceSpan equality); schema stability must be decoupled from refactors.
  Derives are allowed on: wire structs, SerialValue, ref tokens, vocabulary types
  (preset id/ext types), implementer payload structs.

### Engine

- **D4 — Session/segment engine as the foundation.** Write and read are *sessions*.
  A **segment** is the unit of emission: it contains only table entries that are new
  since the previous segment, plus the trees/dumps added in it. A document is a
  sequence of ≥1 segments; the one-shot dump is a single-segment document. Segments
  may be separate JSON payloads/files emitted and read in order (JSONL is the
  canonical stream rendering).
- **D5 — Named typed tables, interning by Arc pointer identity.** Each object kind
  gets a table. Indices are global across a document and append-only: once assigned,
  an index is stable and later segments reference it. Writer sessions keep
  Arc-ptr → index maps across `add_*` calls; reader sessions keep index → Arc maps,
  so sharing and identity (`same_source`, shared states/specs) survive round-trips,
  including across trees.
- **D6 — Determinism.** Emission order is insertion/traversal order, never hash-map
  iteration order (`Package` internals are hashbrown — anything derived from them must
  be explicitly ordered). Required by use case 4 and by content-addressed caches.
- **D7 — Ref tokens.** Cross-entry references are plain serializable index newtypes:
  `SourceRef(u32)`, `StateRef(u32)`, `SpecRef(u32)`, `ProviderRef(u32)`, plus
  `(table, index)` refs for custom tables. Hooks obtain them via explicit
  `cx.intern_*(&arc)` calls and embed them as ordinary fields in payload structs.
- **D8 — Cycle rules.** The live *strong* Arc graph is acyclic by construction (no
  interior mutability; back-edges are `Weak`, see D19). The *wire reference graph*
  must also be acyclic because read-side materialization of immutable values cannot
  tie knots: the writer runs a cycle check over emitted references per segment
  (error names both entries); the reader keeps an in-progress guard during
  materialization plus a recursion depth guard (untrusted input).
- **D9 — Custom tables.** Implementers register additional object kinds on a session
  (an `ObjectSerializer`-style trait object: dump = object + cx → SerialValue; revive
  = SerialValue + cx → object). Needed e.g. for framework-shared resources referenced
  from annotations/ext payloads. Table names are implementer identifiers under the
  wire-identifier stability discipline.
- **D10 — Untrusted-input reading.** Deserialization is a validating boundary: every
  index bound-checked, node-tree invariants re-established (see D13), parent table and
  `single_source` recomputed, never trusted. All failures are typed errors; no panics
  (panic policy rule 3 applies in full).

### SerialValue & payloads

- **D11 — SerialValue.** Own value tree in `techy::serialize`, separate from
  `DiagnosticValue` (which is unchanged; it additionally gains a plain `Serialize`
  impl under the feature for the diagnostics dump path). Variants: null / bool / int
  (i64) / string / list / map (string keys, order-preserving) + the ref-token
  variants (D7). **No floats** (exactness; NaN; no identified use case — numeric
  parameters are the hook author's problem to encode exactly).
- **D12 — Serde bridge instead of a custom derive.** `SerialValue` implements serde's
  `Serializer`/`Deserializer` over itself (the `serde_json::Value` pattern), exposed
  as `to_value<T: Serialize>` / `from_value<T: DeserializeOwned>`. Implementer payload
  structs use `#[derive(Serialize, Deserialize)]` + explicit renames. Ref tokens are
  intercepted by newtype-struct name so they map to the dedicated variants, not bare
  ints. The bridge *enforces* D11 policy mechanically: floats, non-string map keys,
  out-of-range u64 → conversion error. No `ToSerialValue` proc-macro.

### Node trees

- **D13 — Tree tags are never wire material** (existing law: [§dd-dr:tree-tags],
  tree.rs:39-42). Drop on write; reader mints a fresh tag via the normal path. Wire
  node references are bare u32 indices. `RegionState::Resolved.tree_tag`
  (arguments.rs:124) must carry the fresh tag: the wire stores regions in staged-like
  form (child ranges) and the reader re-runs builder-style resolution so region
  invariants are re-established by construction. Consequence (document it): consumer
  `NodeId`s do not survive round-trips; durable node identity rides in annotations.
- **D14 — Annotations & ext are serialized with context.** `NodeTree<L, A>`
  annotations often *are* source spans (extract/transform mint them), so `A` payloads
  may need `cx.intern_*`. Mechanism: per-call annotation codec (dump/revive closures
  or a small trait), with a bounds-based default via the serde bridge for plain-data
  `A`. Same approach for lang ext types (NodeExt/ArgumentExt/SlotExt/StateExt,
  InvocationSyntax) — the lang's serialization-support trait supplies codecs, with
  serde-bridge defaults (see D22).
- **D15 — ParsedArgument::spec: index rule + CallableSpec hook pair.** Default: the
  i-th parsed argument's `ArgumentSpec` Arc must be pointer-equal to
  `spec.arguments()[i]`; the wire stores nothing beyond the index; revive =
  `arguments()[i].clone()`, bound-checked. Custom callable specs (whose
  `make_invocation_parser` builds exotic parsed arguments) override a defaulted,
  feature-gated method pair on `CallableSpec`: write side (emit per-argument payload
  or confirm standard; default errors on pointer mismatch), revive side (the
  `populate`-style hook: supply `Arc<ArgumentSpec>` for parsed argument k; default =
  i-th). `ParsedSlot` is structural (no spec reference) — no hook needed.

### States, specs, providers

- **D16 — States always serialized, interned.** Per state: token rules in full
  (rule payloads are small; inline), mode, ext (via lang codec), scope stack as a
  list of `ProviderRef`s. Derived caches (prefix tables, trigger chars) never hit the
  wire — the state constructor rebuilds them eagerly. Standalone state serialization
  is first-class (cache keying, use case 1). Wire rules sections are all optional
  (feature-agnostic wire; reader errors if the target Lang lacks a used feature).
- **D17 — Specs/providers: hook-owned, single-form entries.** Wire entry =
  `{ identifier: String, data: SerialValue }` for both tables. The hook and its
  factory jointly own serialization and resurrection end to end; core never
  interprets payloads. **Instance-not-lookup principle**: serialization captures the
  instance the parser got, never a lookup to re-run later — no core-level re-query,
  no write-time replay, no enumeration-based reverse maps (`iter_symbols` is tooling,
  not a lookup contract; `retrieve_spec` is a parse-time event — think `\today`).
- **D18 — Hooks return Result.** Defaulted, feature-gated methods on `CallableSpec`
  and `SpecsProvider`: `fn serialize_spec(&self, cx) -> Result<SerializedSpec,
  SpecSerializeError>` (default: "unsupported by this type" error). Hooks may fail
  for any reason (e.g. "only preset-builtin packages supported"). The writer wraps
  every hook error with location context (callable name, span, table index). Read
  side: caller-assembled factory registry, identifier → factory closure
  `(SerialValue, &mut cx) -> Result<Arc<dyn …>>`; no global/link-time registration
  (erased-serde/typetag remain rejected). Failure surface: write hook error; read
  unknown-identifier / factory error / validation error. No Detached variant, no
  evidence machinery in core — an implementer wanting placeholder semantics builds it
  into their own factory as documented behavior of their vocabulary.
- **D19 — Provenance stamp.** Concrete spec types carry
  `provenance: Option<Weak<dyn SpecsProvider<L>>>` (a field on `StdCallableSpec`,
  `MacroSpec`, `EnvironmentSpec`, … — not a trait field; `CallableSpec` is a trait).
  Optional because on-the-fly specs (e.g. `\newcommand`-minted) have none. `Weak`
  because a strong `Arc` would close an ownership cycle with the package's strong
  spec Arcs (leak). Stamped at construction inside `Arc::new_cyclic` (package-builder
  API change; `Weak<Concrete>` coerces to `Weak<dyn SpecsProvider<L>>`). Process-local
  like tree tags — never serialized; it feeds the hook (identity data +
  `cx.intern_provider`). Upgrade failure is a hook error ("spec outlived its defining
  provider") or triggers the implementer's recipe fallback. Records the
  *constructing* provider; correct even if the spec is reachable via other providers.
- **D20 — Granularity is owner-decided; standard latexlike split.** Packages:
  identity payloads (e.g. `{"pkg": …, "ct": …, "name": …}` — implementer-schema
  structs, D12), resolved by the framework's factory against its own environment;
  key/name stability is the registrant's obligation. `Scope` (dynamic `\newcommand`
  definitions): full recipe dumps — definitions as `SpecRef`s to `Owned` recipe
  entries. `\newcommand` spec recipes store constructor-level data
  (`{args: 3, opt_default: …, body: …}`), NOT internals: **no hooks on
  `ArgumentParser` or `EnvironmentBehavior`** — factories rebuild specs through
  their constructors, which re-create parsers/behaviors internally.
  Pairing convention (avoids D8 wire cycles): identity-resurrected providers pair
  with provenance payloads on their specs; full-dumped providers pair with
  self-contained recipe payloads. The latexlike defaults satisfy it naturally.
- **D21 — Write-context user data.** The write session carries a caller-supplied
  user-data slot so framework hooks can consult their environment (the stampless
  alternative to D19). Read factories are caller-constructed closures and capture
  naturally.

### Langs, presets, sources, diagnostics

- **D22 — Preset obligations (latexlike as template).** (a) serde derives + explicit
  renames on vocabulary/data types: `CallableType`, `Mode`, `GroupType`,
  `MathGroupForm`, `Event`, `BodyMarker`, `InvocationSyntaxData`,
  `StdEnvironmentSyntax`/`StdEnvironmentSideSyntax` (its `Arc<GroupRule>` inlines),
  state/session ext types. Preset vocabulary is part of the public schema; the lang
  owns its names. (b) hook impls + a factory-registration helper for its spec/provider
  types. (c) the provenance-stamping package builder. Environments need nothing
  special — they are core Callable nodes (`callable_type == Environment`).
  Core defines the bound-set once (a `SerializeLang`-style support trait with
  serde-bridge defaults) so signatures stay readable.
- **D23 — Sources: embed or reference.** Per source, chosen at write time: embedded
  text, or reference `{origin label, length, digest}` resolved on read via a
  caller-supplied resolver + verifier. Digest = `{algorithm: String, bytes}` —
  caller-supplied function on both ends; techy neither picks nor implements the hash;
  digest optional per source (caller's validation policy). `TextContent::Spanned`
  stays offset-based against interned sources (never force `materialize()`).
  Provenance edges (`SourceProvenance`) serialize as source-table refs (acyclic).
  `Span` offsets: always kept — there is no "no spans" mode (a mandatory `SourceSpan`
  per node; reference-mode sources already keep text out of the file).
- **D24 — Diagnostics.** Wire = severity + identifier + `DiagnosticValue` data
  (existing `serializable_data()` channel) + span (source refs) + trace frames
  (rendered `title: String` + span). Revive as an adapter type implementing
  `DiagnosticData` keyed by identifier (anticipated by error.rs:63-81). Lossy on Rust
  type identity, faithful on wire identity — matches
  [§dd-dr:wire-identifier-stability]. Diagnostics live in the same document as their
  trees (shared source table).
- **D25 — Multi-tree root.** Documents hold N trees + shared tables; `ParseResult`
  (tree + diagnostics + session ext) is a convenience wrapper over that.

---

## 4. Open questions (resolve with user before/at the flagged milestone)

- **Q1 (M1) — Engine entry encoding.** Uniform SerialValue-mediated entries for all
  tables (simplest, one path; recommended for v1) vs. typed-serde fast path for core
  tables (smaller binary encodings). The rendered JSON is identical either way, so
  this is schema-invisible and can be optimized later.
- **Q2 (M0) — Naming pass.** Public names throughout the facade: session types
  (WriteSession/ReadSession vs Dumper/Loader vs …), `ObjectSerializer`,
  `SerializedSpec`, `SpecSerializeError`, ref token names, `to_value`/`from_value`,
  segment/document vocabulary. Check [§dd-arch:naming]; user approval required
  (CLAUDE.md rule). `SerialValue` itself is user-approved.
- **Q3 (M6) — Wire vocabulary naming pass** (freeze-relevant, pre-v1): every field
  name and enum string in the public JSON, core + latexlike.
- **Q4 (M3) — cfg-gated defaulted trait methods** on `CallableSpec`/`SpecsProvider`:
  confirm this mechanism (vs. a side-registry) once the traits are touched. Current
  plan: cfg-gated defaulted methods (consistent within a compilation; keeps hook next
  to the type).
- **Q5 (M5) — Package-builder API shape** for provenance stamping (`new_cyclic`
  threading), including whether core `Package` construction changes or only the
  latexlike builder.
- **Q6 (M4) — Segment/stream container details**: segment header contents (version in
  first segment only?), JSONL conventions, end-of-document marker or not.
- **Q7 (M6) — Read-side verification levels**: what optional sanity checks (e.g.
  argument-count evidence) are worth their wire bytes. Core currently plans
  bound-checks only (D15); revisit with real dumps.

---

## 5. Illustrative document sketch (JSON rendering, names NOT final — Q3)

```json
{
  "version": 1,
  "sources": [
    { "text": "Hello \\emph{world}.", "origin": "intro.tex", "provenance": "primary" },
    { "ref": { "origin": "chapter1.tex", "len": 48210,
               "digest": { "algorithm": "sha256", "bytes": "9f2c…" } } }
  ],
  "providers": [
    { "id": "latexlike.package", "data": { "name": "base-formatting" } },
    { "id": "core.scope", "data": { "name": "document",
        "definitions": [ { "ct": "macro", "name": "abc", "spec": { "$spec": 1 } } ] } }
  ],
  "specs": [
    { "id": "latexlike.pkg-spec",
      "data": { "provider": { "$provider": 0 }, "ct": "macro", "name": "emph" } },
    { "id": "latexlike.newcommand",
      "data": { "args": 3, "opt_default": "x", "body": "…" } }
  ],
  "states": [
    { "rules": { "…": "…" }, "mode": "text", "ext": null,
      "scopes": [ { "$provider": 0 }, { "$provider": 1 } ] }
  ],
  "trees": [ { "nodes": [ { "kind": "chars", "src": 0, "start": 0, "end": 6,
                            "state": 0, "…": "…" } ],
               "annotations": null } ],
  "diagnostics": []
}
```

A later segment in the same document contains only *new* table entries plus new trees,
referencing earlier indices.

---

## 6. Implementation architecture

- `techy/src/serialize/` (pub(crate) internal module; new facade `techy::serialize` in
  lib.rs; whole facade `#[cfg(feature = "serde")]`; feature added to techy/Cargo.toml
  per D1; CI/test story must cover both feature states).
  - `value.rs` — SerialValue + serde Serialize/Deserialize impls + the bridge
    (Serializer/Deserializer over SerialValue, ref-token interception, policy
    enforcement). Largest single mechanical chunk; independent of everything else.
  - `engine/` — session (write/read), tables, interning maps, segments, ref tokens,
    ObjectSerializer registration, cycle check, depth guard, determinism.
  - `wire/` — core wire structs (sources, states, rules, nodes, regions, arguments,
    diagnostics) + conversions. The schema lives here.
  - `tree.rs`, `state.rs`, `source.rs`, `diag.rs` — core object-kind serializers
    (registered on the engine), incl. node revive pipeline: kinds → spans (SourceRef)
    → StateRef → SpecRef → staged regions re-resolution (fresh tag, D13) →
    annotations/ext codecs → invariant validation.
  - `error.rs` — the typed error surface (write errors with location context; read
    errors: validation / unknown identifier / factory / resolver / digest).
- Touches outside the module (all feature-gated): defaulted hook methods on
  `CallableSpec`/`SpecsProvider` (D18) + the parsed-argument pair (D15); provenance
  field on concrete spec types + builder threading (D19); cfg_attr derives on
  latexlike vocabulary (D22); `Serialize` impl for `DiagnosticValue` (D11);
  `latexlike::serialize` helpers (factory registrations, package stamping).

---

## 7. Milestones

All development happens on the long-lived branch **`techy-serialize`** (created by the
user; own it). Commit regularly — small, coherent commits, so the git log doubles as a
recovery record. Auxiliary branches only when genuinely necessary (e.g. parallel
agents editing the same files), merged back into `techy-serialize` promptly. Work runs
in worktrees, never the primary checkout. Each milestone is reviewed by a reviewer
agent against this plan before the next begins, and ends with: tests green both with
and without the feature, `cargo docs` clean, progress log updated. `techy-serialize`
merges into main at project completion (M7), per the local rebase + ff-merge practice
(no PRs).

- **M0 — Skeleton + naming session.** Feature flag wiring; empty facade; CI both
  feature states; resolve Q2 with the user (blocking for public type names; internal
  work may proceed with provisional names). Acceptance: builds with/without feature.
- **M1 — SerialValue + bridge.** value.rs complete: value tree, serde impls, bridge,
  ref-token interception, policy errors. Acceptance: bridge round-trip tests (incl.
  rejection tests for floats/non-string keys/u64 overflow); JSON rendering snapshot.
- **M2 — Engine.** Sessions, tables, interning, segments, custom-table registration,
  cycle check, depth guard, determinism. Tested standalone with toy object kinds (no
  core types yet). Acceptance: multi-segment round-trip with sharing preserved;
  cycle/bounds/depth failure tests; deterministic output test.
- **M3 — Sources + states.** Source table (embed/reference, digest callbacks,
  provenance edges, LineIndex-free), TextContent, Span; state table (rules wire
  structs, mode, ext codec, scope stack as ProviderRefs); hook traits + registry
  (D18) and provider serialization; resolve Q4. Acceptance: state + source
  round-trips with Arc identity preserved (`same_source`, shared states).
- **M4 — Node trees.** Tree writer + validating reader (D13 tag/region story, D15
  argument rule + hook pair, annotations/ext codecs D14, invariants validation D10);
  multi-tree documents; resolve Q6. Acceptance: parse → dump → load → deep-compare
  (structure, resolved text, spans, state/spec identity) on the test corpus;
  hostile-input tests (bad indices, non-tiling regions, cycles, deep recursion).
- **M5 — latexlike.** Vocabulary derives/renames; provenance stamp + builder change
  (Q5); hooks + factories for MacroSpec/EnvironmentSpec/SpecialsSpec/newcommand-style
  specs; `latexlike::serialize` registration helper; InvocationSyntax wire.
  Acceptance: real latexlike documents round-trip including `\newcommand` scopes and
  environments; `\today`-style dynamic-spec test proving instance-not-lookup.
- **M6 — Diagnostics + ParseResult + streaming polish.** DiagnosticValue Serialize;
  diagnostic dump/revive adapter; ParseResult wrapper; JSONL streaming; Q3 wire
  vocabulary naming pass with user; Q7. Acceptance: full ParseResult round-trip; a
  written-out draft schema description document (input for the eventual freeze).
- **M7 — Hardening + permanent docs.** Golden files; proptest round-trip properties;
  rustdoc pass (docs-clarity rules; exhaustive error/Panics sections — target: no
  panics on any input); performance sanity (large doc, many segments). Then, with
  user review: DESIGN_RATIONALE entries + ARCHITECTURE sections/cross-references,
  CLAUDE.md pointer updates if warranted, delete `dev-docs/serialization/`.

Dependency notes: M1 ∥ M0-naming can overlap; M2 needs M1; M3/M4 need M2; M5 needs
M3+M4; M6 needs M5. Within M3, sources and states are parallelizable across agents.

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
  docs-clarity rules; produce compact findings reports.
- **Token discipline.** Briefs point to plan sections by number instead of restating
  them; implementers read only the files their task touches; reviewers get diffs and
  the D-register, not the conversation history; supervisors summarize child output
  before it reaches the main context.
- **Recovery after interruption.** State = this file (§3/§4 decisions, §7 milestone
  acceptance) + §9 progress log + `git log --oneline techy-serialize` + worktree
  list. Any fresh session must be able to resume from those alone; that is the bar
  for progress-log entries.
- **Escalation.** OPEN questions (§4), any new design fork, any deviation from a
  D-number → user, not agent discretion. Naming of public items → user (Q2/Q3).

---

## 9. Progress log

Newest first. Every working session appends: date, actor, milestone, what changed
(branch/commits), what's next, blockers.

- 2026-08-13 — plan adjustments (user): settled decisions are revisable on new
  evidence (escalate in doubt); single long-lived branch `techy-serialize` replaces
  branch-per-milestone (auxiliary branches only for genuine parallel-edit needs).
- 2026-08-13 — plan drafted from interactive design sessions; no implementation
  started; M0 not begun. Next: user review of this plan; then resolve Q2 (naming)
  and open M0.
