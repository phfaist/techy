# FRICTION — T4 "tooling author" walkthrough (editor integration / linter / indexer persona)

Persona bar applied throughout: **logical reach, structured organization** — an advanced
user who expects to compose primitives, not to be handed everything, but who notices when
a primitive every tool needs is missing.

Method note (honesty): the outside-in rule was kept. I read README.md, docs/*.md, and —
inside `techy/src/**` — doc comments and public signatures only, via an extraction script
(`sigext.py` in this directory) that prints doc comments and item/impl signature lines and
skips function bodies. Private *field declarations* and test *function names* were visible
in the extraction; no function body was read or relied on. Zero implementation-body reads
were needed to complete any task — that is itself a strong result for the documentation.
Two facts were nevertheless discovered by compiling/running rather than from any doc page
I had read; both are logged below as (minor) doc gaps.

---

## Task 1 — hover primitive (kind, span, text, line/col per node)

**What worked well (worth keeping exactly as is)**
- `NodeRef::span()` (Arc-carrying `SourceSpan`) + `NodeRef::span_content()` is the
  perfect hover primitive: exact byte range and exact original text with no lookup
  table, on every node. `NodeSlice::span()`/`source_text()` being *exact* (documented
  partition invariant) means multi-node selections are equally trustworthy.
- The `Span` vs `SourceSpan` vs `TextContent` division is clear **and the rationale is
  written down where you meet each type** (source/mod.rs stratum doc; `TextContent`'s
  no-`PartialEq` note). I never once confused which type applies where: `Span` =
  transient byte math, `SourceSpan` = stored location, `TextContent` = logical text.
  The `SourceSpan::span()` / `SourceSpan::new(&arc, span)` bridge is exactly the right
  two-way door. No redundancy complaint; the mirrored `start/end/len/range` accessors
  on both types are necessary duplication.
- `Source::line_index()` returning an index preconfigured with the source's line/column
  offsets is a nice touch.

**Friction**
1. **No kind-name accessor.** For a generic "outline" or logging tool, getting a plain
   structural label requires matching `NodeKind` by hand (5 arms). `summary()` exists but
   mixes content into the string and is explicitly not a stability contract. A
   `NodeKind::name(&self) -> &'static str` (or similar discriminant label) is a small
   gap every generic tool will re-fill.
2. **`descendants()` yields no depth.** It's the right document-order walk, but an
   outline/hover ancestry printer needs depth, so I re-implemented the walk with
   `children()` recursion. A `(depth, NodeRef)` variant (or `Descendants::depth()`)
   would serve tooling directly.
3. **No span→line/col-range helper.** Every node report composes
   `line_col(span.start())` + `line_col(span.end())` by hand. Fine, but a
   `LineIndex::line_col_span(&mut self, Span) -> Option<((usize,usize),(usize,usize))>`
   convenience would trim the most common tooling boilerplate. Related: the library's
   `format_position` renders only the *start* of a span.
4. **Borrowck stumble (minor, once):** `Arc::clone(node.span().source()).line_index()`
   in one expression is rejected (E0716) — `LineIndex<'c>` borrows the content, so the
   Arc needs its own `let` binding. Obvious in hindsight; the guide never shows the
   pattern "get a LineIndex from a node's source", which is *the* tooling pattern
   (the only shown pattern starts from a `source` variable you already own).
5. `LineIndex::line_col` takes `&mut self` — a read-only-looking query needing `mut`
   surprises for a moment; the laziness is documented, so this is cosmetic. The
   100 000-byte `max_scan_len` default (silent `None` beyond it) is documented and
   adjustable, but tooling authors should be loudly pointed at it: an editor over a
   large file silently loses all line/col info unless it calls `set_max_scan_len`.
6. Observation, not a bug: a callable node's children cover only argument regions —
   the trigger spelling (`\section`) and an environment's `\begin{…}`/`\end{…}` are
   inside the callable's span but under no child. Correct design (recorded on the
   node), but see Task 2 for where it needs documenting.

## Task 2 — position → innermost node + ancestry

**Missing feature (the big one for this persona):** there is **no position→node query**
in the public API, and **no `NodeRef::parent()`/`ancestors()`**. Both halves of the
"cursor" primitive are absent:
- Innermost-node-at-offset: hand-rolled in ~20 lines by descending from the root and
  picking the covering child per level (the span partition invariant makes this correct
  and unambiguous — credit to the invariant *being documented* on `NodeSlice::span`).
  Every editor integration will write exactly this loop, with exactly these subtleties:
  half-open containment, empty-span nodes never match, offsets inside a callable's
  trigger token or environment terminator resolve to the callable with no deeper child,
  offset == len is outside every span. A `tree.node_at(offset)`-style helper (or a
  documented recipe) would capture those subtleties once.
- Ancestry: because `NodeRef` has no parent link, the chain must be *recorded during*
  the descent. That is fine top-down, but it composes badly with the other traversal:
  if you found a node via `descendants()` (e.g. "every `\cite`"), you cannot ask for
  its context — you must re-descend by span. Verdict: felt like a genuinely missing
  feature, not like reasonable composition left to the user.

**What worked well:** `children()` + spans made the hand-rolled version short,
deterministic, and correct on the first run (the one compile error was the Task-1
borrowck stumble repeated). Children being in source order also means the linear
child scan could become a binary search with no API change.

## Task 3 — multi-source inclusion + provenance

**Missing feature / doc gap (the other big one):** the resolver seam exists and is
excellent, but **nothing connects it to parsing**. The latexlike preset ships no
`\input`-like construct, so resolution never triggers during a parse; `Language`
carries the resolver (`with_resolver`, `resolve_source`) but the only way to use it is
the embedder-driven loop I wrote: parse, scan the tree for `\input` callables, resolve,
`parse_source` the result, recurse — producing a *forest* of `ParseResult`s the
embedder must correlate. That may well be the intended v1 workflow, but no page says
so; "how do I do `\input`?" has no documented answer (the resolver docs describe the
*lookup*, not the *wiring*). Note also that node/mod.rs mentions "mixed-origin trees"
as a design capability — from the outside I cannot see any public path that produces
one today. An example/guide chapter for the include workflow is the single most
valuable doc addition for this persona.

**What worked well (better than expected)**
- `Language::resolve_source` → `parse_source` composition is clean, and the
  provenance model is genuinely well designed: content-only resolvers, core-stamped
  per-include-site `SourceProvenance::Resolved`, documented cycle-freedom, and the
  explicit note that recursion limits are the embedder's job *with* the pointer to
  `provenance_chain()` for implementing one. The design rationale is right there in
  the trait docs — exemplary.
- `MapResolver::with_reference_as_origin()` made multi-file reports self-describing
  with one call.
- `Diagnostic::render`/`ParseError::render` **already render the include chain**
  ("included from @ (line 2, col 1) [chapter1.tex] …") with zero effort from me. My
  hand-rolled "included from X which was included from Y" walk over
  `provenance_chain()` was easy (public variant fields, documented iteration order:
  self first, ending at `Primary`).

**Minor friction**
- `extract::content_as_chars` returns `Cow<'_, str>`; the guide's `assert_eq!` examples
  hide the type, so the first `collect()` into `String` fails to compile. Signature-level
  discovery, one `into_owned()` — recording only because a guide snippet that *stores*
  the result would prevent it.
- Vocabulary near-collision, navigable but worth a glance: `resolve_source` (free fn)
  vs `Language::resolve_source` (same composition — fine), `ResolvedContent` (resolver
  output) vs `SourceProvenance::Resolved` (provenance variant) vs `ResolvedCallable`
  (engine, unrelated to sources). Context disambiguates; no wrong turn taken.

## Task 4 — compiler-style diagnostics rendering

**Provided vs hand-rolled** (the persona's core question):
- Library-provided: severity (+ `Ord` for thresholds), structured payloads with
  documented public fields, downcast access (`Diagnostics::conditions::<T>()` — very
  pleasant), stable wire identifiers, message text, exact spans (the stray-`}`
  diagnostic spans exactly the delimiter), traceback frames with rendered titles,
  `format_position`, `format_traceback`, per-diagnostic `render()`, collection-level
  `render_all()` with documented O(N + k) index sharing, bounded retention with
  honest `suppressed()`/`has_errors()` semantics.
- Hand-rolled: (a) compact `file:line:col:` position format — `format_position`'s
  `@ (line 2, col 5) [origin]` shape is fixed, and editors/CI want the file first and
  machine-splittable; (b) the **source-line excerpt with caret/underline** — the one
  render feature a "compiler-style" report needs that the library lacks; and to build
  it, (c) the line's byte range: `LineIndex` answers offset→(line, col) but has no
  `line_range(line)`/`line_of(offset)` inverse, so I re-scanned for `\n` boundaries
  manually. One `line_of(offset) -> Range<usize>` method would make caret rendering
  trivial and is my top small-API wish.
- **Doc gap found by running:** the wire-identifier `<area>` segment is the *internal
  module name* (`core.nodes_parser.unresolvable-command`,
  `core.group_parser.unclosed-group`). I guessed `core.parse.*` and lost a
  compile-run cycle. Two issues: no doc page lists the core identifiers (a registry
  table — identifier ↔ condition type ↔ meaning — is the obvious fix), and naming
  semver-stable identifiers after internal module layout sits oddly with the
  documented "decoupled from the type/module name" principle: renaming
  `nodes_parser.rs` is supposed to be an internal refactor, yet the identifiers would
  have to keep the old area name forever.
- Discoverability: condition *types* live next to their parsers in `constructs` and
  are root-re-exported; finding "which type is the unresolved-command condition" meant
  scanning the `constructs` re-export list for a likely name (`UnresolvableCommand` —
  found quickly, but a conditions index would remove the guesswork).
- Traceback frames: empty on top-level diagnostics (documented), populated and
  correctly innermost-first on nested errors; `TraceFrame::title/span` are exactly
  enough to re-render a custom traceback. No friction.

## Task 5 — reuse / re-parse / span-stability contracts (docs survey)

What the docs **do** promise, clearly: `Language` is "define once, parse many", owns no
per-parse state, `Send + Sync`; `ParserSession` is transient, one parse, no reuse;
trees are immutable, transformations build new trees; results are self-contained (no
lifetime back to `Language` or source store); sources are immutable once created;
`SourceSpan` equality is identity-based (same `Arc`) + range; `NodeId`s are only
meaningful for the minting tree (debug-tagged).

What tooling authors will ask and the docs **don't** address:
- **Incremental / subrange re-parse:** nothing anywhere (guide, module docs) about
  re-parsing a subrange or reusing prior results after an edit. Full re-parse per edit
  is evidently the model; saying so explicitly (even "non-goal for now") would save
  every evaluator this search.
- **Span stability across re-parses:** deducible but never stated. Verified by probe
  (task5_reparse.rs): `parse_source(same Arc)` twice → corresponding spans compare
  equal; `parse(content)` twice → equal ranges, *unequal* spans (fresh anonymous
  `Source` per call, identity equality). Consequence worth one doc sentence: an editor
  that wants to correlate anything across parses must mint its own `Arc<Source>` and
  use `parse_source`, never `parse`.
- Positive: the "define once, parse many" + self-contained-results contracts are
  exactly the right foundation for a long-lived tooling process, and they are stated.

---

## Cross-cutting verdicts

- **Names hard to find:** almost none — the concepts-overview page maps concept→item
  well. Exceptions logged: condition-type ↔ identifier mapping (Task 4), and the
  include-workflow entry point (Task 3, a workflow rather than a name).
- **Internals leaking:** only the identifier `<area>`=module-name issue (Task 4).
  Otherwise the surface felt deliberately curated (e.g. `NodeSlice` constructors
  crate-private, `summary()` explicitly non-contractual).
- **Redundancy:** the span/position family (`Span`, `SourceSpan`, `Range` bridges,
  `TextContent`, `LineIndex`) is well-partitioned with rationale written at each seam;
  no confusion in practice. Free-fn vs method `resolve_source` duplication is benign.
- **Missing features, ranked for this persona:**
  1. position→node query + parent/ancestry access (Task 2);
  2. an `\input` wiring story — construct, recipe, or explicit "embedder-driven" doc
     (Task 3);
  3. `LineIndex::line_of(offset) -> Range<usize>` (line text for caret rendering,
     Task 4);
  4. caret/underline source-excerpt renderer (nice-to-have; 3 makes it easy to hand-roll);
  5. kind-label accessor and depth-aware descendants (Task 1, small);
  6. a core-conditions/identifier registry page (Task 4);
  7. an explicit re-parse/span-stability paragraph (Task 5).
- **Doc gaps / trial-and-error log:** implementation bodies read: **zero**. Facts
  learned only by compile/run: `content_as_chars` returns `Cow` (signature-visible,
  guide-invisible); wire identifier area = module name (nowhere stated). Everything
  else — including subtle contracts like span partition, provenance ordering,
  render_all's index sharing, diagnostics retention cap — was learned from doc
  comments where the item lives, which is exactly where an advanced persona looks.
