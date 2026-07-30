# FRICTION — Phase 1b T5 framework-builder walkthrough

Persona: framework builder wrapping techy behind a Python (PyO3) API; archetypes
(A) latex2text-class, (B) FLM-class, (C) latexpp-class.

All probes live in `probes/` (plain Rust) and `techy-py/` (PyO3 module + smoke test),
both under this directory. techy is depended on via `path = "/Users/philippe/projects/techy/techy"`.

---

## Part 1 — FFI boundary

### Boundary table (compile-probed in probes/src/bin/boundary.rs; all results are
compile-time assertions that built cleanly, except the negatives noted)

`L = Latexlike`, `O = Option<String>` (= `Latexlike::SourceOrigin`).

| Type | 'static | Send | Sync | Notes |
|---|---|---|---|---|
| `Language<L>` | yes | yes | yes | share behind `Arc` in bindings; build once |
| `ParseResult<L>` | yes | yes | yes | plain struct: `{ tree, diagnostics }`, both owned |
| `NodeTree<L>` | yes | yes | yes | `Clone`; flat frozen storage |
| `NodeRef<'t, L>` | **no** (E0310/E0597) | yes (any 't) | yes (any 't) | `Copy`; cannot be stored in a Python object — see negative probe |
| `NodeSlice<'t, L>` | **no** | yes | yes | same story |
| `Descendants<'t, L>` | **no** | — | — | iterator, borrow-bound |
| `NodeId` | yes | yes | yes | `Copy + Eq + Ord + Hash`; the FFI handle half |
| `BuildId` | yes | yes | yes | `Copy`; staging-side id |
| `Diagnostics<O>` | yes | yes | yes | iterable, `as_slice()` |
| `Diagnostic<O>` | yes | yes | yes | `Clone`; fully extractable (below) |
| `ParseError<O>` | yes | yes | yes | strict-mode error; identifier/message/span/frames |
| `TraceFrame<O>` | yes | yes | yes | owned title + span |
| `SourceSpan<O>` | yes | yes | yes | `Clone`; Arc'd source inside |
| `Span` | yes | yes | yes | plain byte range |
| `Source<O>` / `Arc<Source<O>>` | yes | yes | yes | |
| `SourceProvenance<O>` | yes | yes | yes | |
| `TextContent` | yes | yes | yes | `Spanned`/`Owned` two-variant enum (exhaustively matchable) |
| `LineIndex<'c>` | **no** (borrows content) | yes | — | compute line/col on demand, don't store |
| `Package<L>` / `Scope<L>` / `ScopeStack<L>` | yes | yes | yes | |
| `ParsingState<L>` / `Arc<ParsingState<L>>` | yes | yes | yes | |
| `ParsingStateDelta<L>` | yes | yes | yes | |
| `NodeTreeBuilder<L>` | yes | yes | yes | |
| `ParserSession<L>` | yes | yes | yes | |
| `NodeKind<L>` / `GroupData<L>` / `CallableData<L>` / `ParsedArguments<L>` | yes | yes | yes | `NodeKind: Clone` |
| `LatexlikeDriver` | yes | yes | yes | |
| `ArgumentSpec<L>` / `StdCallableSpec<L>` | yes | yes | yes | |
| `TokenRules<L>` | yes | yes | yes | |

Negative probe (probes/src/bin/noderef_escape.rs.negative-probe): storing
`NodeRef<'static>` from an owned parse fails with E0597 — "`result.tree` does not
live long enough … this usage requires that `result.tree` is borrowed for `'static`".
Exactly the situation a naive Python node wrapper would create.

**Verdict: the boundary is exemplary.** Everything a binding must *own* is
`'static + Send + Sync`; only the ephemeral proxies are lifetime-bound, and each has an
owned, `Copy` re-entry token (`NodeId`).

### Re-access pattern (probes/src/bin/reaccess.rs — all verified at runtime)

- `NodeRef::id() -> NodeId` and `NodeTree::get(NodeId) -> Option<NodeRef>` /
  `NodeTree::node(NodeId) -> NodeRef` round-trip works. A Python handle =
  `Arc<NodeTree>` + `NodeId`; every method re-derives the `NodeRef` locally.
- Navigation by id: children (`NodeRef::children()` → ids), argument/slot/body slices,
  `iter_storage_order`, `descendants`, `nodes_in(range)` — all reachable after
  re-derivation. Verified through the handle type in the probe and in the real PyO3
  module.
- `tree.clone()` and `tree.materialize()` preserve layout **and** the debug provenance
  tag, so existing `NodeId`s stay valid on the copies (verified).
- **Gap: no parent link.** `NodeRef` has no `parent()`; `NodeTree` stores no parent
  table. A binding must precompute `HashMap<usize, NodeId>` from a full walk (probe
  does this; O(n), fine — but every consumer will write it).
- **Gap: no `index_in_parent()`/sibling step.** Position among siblings must be found
  by linear scan of the parent's children.
- Debug-build nuance: `NodeId` carries a tree-provenance tag; `get()` on a *different*
  tree returns `None` in debug builds but may silently resolve in release builds
  (documented on `NodeTree::node`/`get`). A binding holding (tree, id) pairs together
  is safe by construction; a binding that lets Python mix ids across documents relies
  on debug-only detection. Worth noting in binding docs.

### Diagnostics extraction (reaccess.rs + techy-py)

Fully owned extraction verified: `severity()` (exhaustive 3-variant enum — bindings can
match it totally), `identifier()` (stable string id, e.g.
`core.nodes_parser.unresolvable-command`), `message()` (owned String), `span()`
(clone; start/end/content/source/origin), `frames()` (owned titles + spans),
`render()`. Line/col via `source().line_index().line_col(pos)` on demand.
`ParseError` (strict mode) has the same surface → clean Python exception mapping.
Structured payloads additionally reachable via `data()`/`downcast_ref::<Condition>()`
(not exercised beyond type-level check — string surface suffices for bindings v1).

### The PyO3 module itself (techy-py/)

Built for real: `techy-py/src/lib.rs` (pyo3 0.25.1, abi3-py39, cdylib) exposing
`parse(str, tolerant=True) -> Document`, `Document.root()/.diagnostics/.node_count`,
`Node.kind_name/.name/.children/.source_text/.span/.line_col/.chars/.argument(i)/.body`,
`PyDiagnostic` (owned fields). `smoke_test.py` passes end-to-end, including a handle
outliving its garbage-collected `Document` (the `Arc` keeps the tree alive).

Build friction (environmental, not techy's fault):
1. sandbox forbids `~/.cargo/registry` writes → set `CARGO_HOME` to scratchpad;
2. macOS extension-module linking needed `.cargo/config.toml` with
   `-C link-arg=-undefined -C link-arg=dynamic_lookup`.
No `unsafe`, no self-referential workaround, no tree copying was needed — the binding
is boring in the best way. Frozen pyclasses work because everything stored is
`Send + Sync`.

techy-side friction found while writing it:
- The binding must invent `kind_name` (match on `is_chars()`… or on `NodeKind`); a
  stable public kind-name string (or a `NodeKind::name()`) would keep bindings and
  debuggers consistent (`summary()` exists but is a debug rendering, not a stable enum
  name).
- `Diagnostic::severity` etc. are fine; `Severity` being exhaustive means adding a
  variant is a breaking change — for bindings that is *good* (total match), just noting
  the contract.
- Nothing on `Diagnostics` gives owned conversion in one call (`into_vec()`); iter +
  clone per element is fine.

---

## Part 2 — probe-level friction (details in FRAMEWORK-ANALYSIS.md)

### (A) latex2text probe (probes/src/bin/latex2text.rs)

- Both handler-attachment models work: framework-side
  `HashMap<(CallableType, String), Handler>` *and* spec-side custom
  `CallableSpec<Latexlike>` types recovered by `Any` downcast (downcast documented as
  contract on the trait). No friction registering custom spec types — all trait
  methods have defaults.
- Gotcha found: the whitespace terminating `\alpha ` is the invocation's `post_space`
  (consumed by the trigger token) — a handler that ignores it renders `$\alpha + x$`
  as "α+ x". API exposes `post_space()` so the handler can re-emit; but *every*
  latex2text author will hit this once. Needs a guide paragraph.
- Recursion into `argument_content_nodes(i)` / `body()` / math groups: clean.
  Absent optional arguments are honest (`argument_content_nodes -> None`).

### (C) reconstruction probe (probes/src/bin/reconstruct.rs)

- **Gap-filling reconstruction is byte-faithful** (313-byte gnarly doc: optional args,
  env-with-args, verbatim env, `\verb`, inline+display math, comments, paragraph
  break, specials, tolerant-recovery of `\foo` and a stray `}`): recurse children,
  copy bytes between child spans from the source. Child spans verified in-order and
  contained in the parent throughout the walk.
- Structure-only reconstruction leaks exactly 10 gaps = the callable trigger
  spellings: `\emph`, `\cite`, `\verb`, `\begin{itemize}`, `\end{itemize}`,
  `\begin{verbatim}`, `\end{verbatim}` — plus specials spellings (`--`, `~`; these are
  recoverable as `name()`, my probe just didn't special-case them). Argument
  delimiters (`{…}`, `[…]`, `\verb|…|`) are all group-delimiter *data* — no leak.
  I.e. the only node-data hole for source-faithful recomposition is the **trigger
  spelling** (escape char + written name + `\begin/\end` wrapping); everything else is
  either child structure or `TextContent` payloads.
- Targeted rewrite (replace one argument group, all other bytes verbatim): works
  first try with the gap-filling emitter.

### (cross-cutting) transform probe (probes/src/bin/transform.rs)

- From-scratch tree building via public `NodeTreeBuilder` works (chars/group/list),
  with `Language::initial_state()` supplying the state and `Source::new` /
  `Source::synthesized(content, description, triggered_at)` supplying spans.
  `synthesized` *requires* a `triggered_at` span — good provenance discipline; for
  fully from-scratch content you use a Primary `Source::new` instead.
- **Naive re-stage of a finished callable fails**: `NodeTreeBuilder::add` returns
  `Err(RegionAlreadyResolved)` when given a finished tree's `CallableData` (regions
  are resolved; the builder demands staged ones). Correct per contract — but it means
  every transform author must write the resolved→staged region translation.
- DIY deep copy with region translation (≈60 lines, mirroring crate-private
  node/copy.rs): works, regions/arguments navigable in the copy, shapes identical.
  Required knowledge: region tiling, `ContentNodes::InRegion` vs `InChildrenOf`
  reconstruction via `content_parent() == node.id()`, child-offset arithmetic. This is
  exactly the "public transform surface is a later phase" hole.
- `NodeRef::tree()` is `pub(crate)` — a `NodeRef` cannot hand back its tree, so any
  helper that resolves `ChildRegion::content_parent()` (a `NodeId`) must have the
  `&NodeTree` threaded through separately.
- Mixed-origin transform tree: the **builder accepts it**, `check_tree_invariants`
  **rejects it** ("child lives in a different source" — the checker is parse-tree law;
  its docs defer mixed-origin transform trees to post-Phase-6). No transform-tree
  validator exists today.
- **Sharp edge**: `NodeSlice::span()`/`source_text()` return `None` only when the
  run's *first and last* nodes differ in source. With a synthesized node in the
  *middle*, `source_text()` silently returns the ORIGINAL source bytes — including the
  material the transform replaced (asserted in the probe). On spliced trees these
  accessors can lie.
- **No BuildId→NodeId correlation from `finish()`**: old→new id maps end at BuildId;
  after `finish` the only re-identification is heuristic (probe re-finds by span
  equality). A transform framework wanting stable old↔new node maps is blocked here.

### (B) FLM probes (probes/src/bin/flm_lang.rs, flm_reuse.rs.negative-probe)

- Custom `Lang` with a real `NodeExts` bundle: tier-1 ext attached in
  `Lang::finalize_node`, read back via `NodeRef::ext()` — the two-tier system IS
  reachable from the public API, verified end to end (parse → ext values present).
- Custom-Lang cost: `initial_state_data()` written from scratch (TokenRules literal —
  12 fields), because **nothing latexlike is reusable across Lang**: captured compile
  errors — `LatexlikeDriver: ParseDriver<Latexlike>` only; `default_token_rules() ->
  TokenRules<Latexlike>`; `base_package() -> Package<Latexlike>`; `MacroSpec:
  CallableSpec<Latexlike>` only. An FLM-class language that wants latexlike behavior
  plus its own exts/modes must fork the preset (or techy must genericize it).
- Side tables on the preset: `HashMap<NodeId, T>` + multi-pass over the immutable
  tree works cleanly (NodeId is Copy+Eq+Hash+Ord). This is the viable FLM path today.

---

## Implementation-body reads log (structural questions only)

1. `node/tree.rs` — NodeTree/NodeId internals: parent links? (none stored); debug
   provenance tags; `get`/`node` semantics; `materialize` tag sharing.
2. `node/node_ref.rs` — grep for `parent`: confirmed no parent accessor;
   `tree()` accessor is `pub(crate)` (E0624 also proved it from outside).
3. `node/copy.rs` — header + skim: subtree copy exists but is `pub(crate)`;
   module docs state "a public transform surface is a later phase's design".
4. `node/builder.rs` — `add_with_ext` staging checks (to learn the staging contract a
   transform must satisfy: claimed children, region tiling, staged-not-resolved).
5. `node/arguments.rs` — `RegionState` staged/resolved duality (to write the DIY
   translation and to know `content_parent()` panics on staged regions).
6. `node/invariants.rs` — full read after the mixed-origin panic: checker is a test
   utility, not builder law; children-in-same-source assertion; partition invariant.
7. `node/slice.rs` — `span()`/`source_text()` bodies after the unexpected
   `Some` on a mixed slice: None only on first/last source mismatch.
8. `state/lang.rs` — Lang trait bounds and defaults (associated types, finalize_node
   contract) — needed to write the custom Lang.
9. `latexlike/mod.rs` lines 172–199 — Latexlike's Lang impl (NodeExts = (),
   SourceOrigin = Option<String>) — determines what a preset user can/can't attach.
10. `source/source.rs` lines 40–110 — Source constructors (synthesized signature).
11. `spec/callable.rs` — CallableSpec trait body (defaults, downcast contract,
    Send+Sync supertraits).
12. `error.rs` — public surface scan only (signatures).
13. `engine/mod.rs` ParseResult struct (fields), `engine/language.rs` signatures.
14. `token/rules.rs` — TokenRules field list (to fill the custom-Lang literal).
15. `latexlike/environments.rs` — public surface scan: `EnvironmentBehavior` is an
    open trait (custom body parsing on the preset), `VerbatimBehavior` implements it.
