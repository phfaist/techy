# Friction log — T1 "document consumer" walkthrough

Persona: first-time user, wants to parse LaTeX and inspect the result. Sources used:
README.md, docs/guide.md, docs/learn-by-example.md, docs/concepts-overview.md, plus
`pub fn` signatures and doc comments in `techy/src/**` (standing in for rustdoc, which
a real user would build with `cargo docs`). No implementation body was *needed* to
complete any task (see "Doc gaps" per task for near-misses).

Overall verdict up front: the persona succeeded on all six tasks, the code compiled on
the first `cargo build`, and every assertion passed on the first run. That is a very
good outcome for a pre-1.0 library — `docs/learn-by-example.md` deserves most of the
credit; it is the single reason first-try success was possible. The friction below is
real but mostly of the "ergonomic tax" kind, not the "impossible/undiscoverable" kind.

---

## Task 0 (setup): building a usable Language

**Awkward:**
- **No standard macro database.** To parse even a modest realistic snippet, the user
  must hand-register `\emph`, `\cite`, `\item`, and `itemize` before anything works.
  The guide is upfront that this is a later phase, but for the T1 persona this is the
  single largest onboarding cost: the "hello world" for real LaTeX is ~25 lines of
  package building before the first `parse()` call.
- **`Arc` in the registration API.** The simplest possible definition is
  `package.insert(CallableType::Macro, "emph", Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())))`
  — four nested constructors, one `Arc`, one `unwrap`, per macro. The shared-ownership
  strategy (an internal storage concern) leaks into every user's first lines of code.
  An `insert` that takes `impl Into<Arc<...>>` (or a `MacroSpec::args("o","m")`-style
  shorthand that is already `Arc`'d) would remove the noise without changing the model.
- **`with_provider` returns `Result`.** Why can pushing a package onto a fresh default
  language fail? The consumer has no mental model for this and cargo-cults `.unwrap()`.
  (Presumably a seed-derivation seam; the signature makes the simple path pay for it.)
- **Recovery configuration spans three modules**: `techy::error::Recovery` +
  `techy::latexlike::LatexlikeDriver::new` + `techy::engine::Language::new`. That the
  recovery policy lives *on the driver* is learnable only from the guide; nothing in
  `Language`'s obvious surface says "to get tolerant parsing, construct a driver".
  Wished: `Language::<Latexlike>::tolerant()` / `.with_recovery(Recovery::Tolerant)`.

**Names hard to find/guess:** `argument_specs` (I looked for "signature", "args",
"ArgSpec" first; found it only in the guide). The `["o", "m"]` xparse codes are
compact but opaque without the guide — an enum alternative (`Arg::Optional`,
`Arg::Mandatory`) would be self-documenting for non-xparse users.

**Two ways to do the same thing:** `argument_specs(["o", "m"])` vs
`argument_specs_from_str("om")` — two spellings of the same concept in the same
module; a T1 user cannot tell when to prefer which.

---

## Task 1: parsing a realistic snippet

**Worked well:** `language.parse(SRC)` → `ParseResult { tree, diagnostics }` is exactly
the right shape; public fields (not accessors) on `ParseResult` are pleasant.

**Awkward:**
- The guide's examples all import via full module paths
  (`techy::engine::Language`, `techy::scopes::Package`, `techy::error::Recovery`, …),
  so I needed **six `use` lines across five modules** for the basics. Only when
  auditing `lib.rs` afterwards did I notice nearly all core items are *also*
  re-exported at the crate root (`techy::Language`, `techy::Package`,
  `techy::Recovery`, `techy::NodeKind`, …). **Redundancy:** every core item is
  reachable two ways, the docs consistently use the deeper one, and the root
  re-export list (~100 items, including `StagedNodes`, `NodeTreeBuilder`, `BuildId`,
  `UnusableRecoveryTokenKind`…) floods autocomplete with machinery a T1 user should
  never see. Either the guide should showcase the root re-exports for consumer-facing
  items, or the root should re-export a curated consumer subset only.
- `Language::<Latexlike>::default()` vs `Language::new(driver)` — the turbofish on
  `default()` is required and slightly ugly; a `Latexlike::language()` or
  `latexlike::language()` constructor would read better.

**Missing:** a one-call convenience for the whole task, e.g.
`techy::latexlike::parse(src)` / `parse_tolerant(src)` returning `ParseResult`. The
persona's first minute currently requires understanding `Language`, `Latexlike`,
drivers, and providers.

---

## Task 2: tree dump

**Worked well:** `NodeRef` is a good proxy: `kind()`, `span()`, `span_content()`,
`children()`, `child_count()`, `name()` made the recursive dump ~20 lines.
`summary()` is a gift for debugging.

**Awkward:**
- **No public kind-label accessor.** To print "Chars/Group/Callable/…" I had to match
  on `NodeKind` myself, and the match patterns expose internals: `Chars { .. }` struct
  variant with an `ext` field, `Group(Box<GroupData>)` / `Callable(Box<CallableData>)`
  boxed payloads. A `NodeKind::label() -> &'static str` (or `Display`) would keep the
  enum's storage shape out of consumer code. (`summary()` exists but its format is a
  debugging aid, not a stable "kind" answer.)
- `name()` (the generic callable-name accessor, covering macros, environments and
  specials at once) is *not shown in the guide* — I found it only by scanning
  `node_ref.rs` signatures. It is exactly what a tree dump wants; it should be in the
  guide next to `macro_name()`.
- Environment internals show through: the `itemize` Callable's child is a `List` node
  (the body slot), so a naive dump prints an extra `List` layer the user never wrote.
  `body()` exists as the curated view, but the raw `children()` walk exposes the
  slot mechanism. Not wrong — just needs one guide sentence ("what children() shows
  for an environment").

**Doc gap (minor):** whether `descendants()` includes the start node itself is not
stated in the accessor list I could see; I inferred "excludes self" from the guide
example and my dump output confirmed it. One doc line would settle it.

---

## Task 3: plain text extraction

**Awkward:**
- **No document-level plain-text helper.** `extract::content_as_chars` errors (by
  design, honestly) on any callable, so it cannot answer "give me the text of this
  document" — the one question every T1 consumer asks. The working idiom,
  `root.descendants().filter_map(|n| n.chars()).collect::<String>()`, is short but has
  to be *known*; it lives mid-way through the guide's "Reading nodes" section, not
  under "Extracting content" where I first looked.
- **Specials silently vanish.** `p.~7` extracts as `p.7` (the `~` contributes
  nothing), ligatures like `--` would drop too, and macro arguments are concatenated
  without separators (`p.7knuth84.`). For a "plain text" consumer this is lossy in a
  way that is easy to miss. pylatexenc has latex2text for this; techy has no
  rendering-oriented text extraction yet. Worth an explicit "wished it existed":
  `extract::plain_text(node)` with a documented policy for specials/arguments, even a
  naive one.

**Module depth:** `techy::node::extract::content_as_chars` — the `extract` functions
are *not* re-exported at the crate root (unlike almost everything else), so this is
the one API where the deep path is mandatory. Inconsistent with the rest.

---

## Task 4: finding `\emph` invocations

**Worked well:** best task of the six.
`descendants().filter(|n| n.macro_name() == Some("emph"))` then
`argument_content_nodes(0)` + `content_as_chars` is compact and readable.
`Cow<str>` return is a nice touch.

**Awkward:**
- **Argument indices are spec positions, not "nth mandatory".** For `\cite` with
  `["o", "m"]`, the mandatory argument is index 1; for `\emph` it is index 0. The task
  "extract the first *mandatory* argument" has no direct expression — the user must
  know each macro's spec layout. `argument_content_nodes_named` exists but the
  `o`/`m` codes don't produce names. A `mandatory(0)` / filter-by-parser-kind helper,
  or auto-named codes, would close this.
- **`argument_nodes` vs `argument_content_nodes`** — the with-delimiters vs
  through-the-braces distinction is real and both are needed, but the names alone
  don't teach it; I learned it from a guide example. A doc table on `NodeRef` (or
  names like `argument_outer_nodes`) would help.

---

## Task 5: line/column

**Awkward — the chain is too long.** The conceptual ask is `node.line_col()`. The
actual code is four hops across two modules plus a mutable binding:

```rust
let span = node.span();                       // node -> SourceSpan   (techy::source)
let mut li = span.source().line_index();      // -> Source -> LineIndex
let (line, col) = li.line_col(span.start()).unwrap();  // &mut self, Option
```

- `LineIndex::line_col(&mut self, ..)` — the `&mut` (lazy cache) surprises a consumer
  and infects the calling code with `mut`. Interior mutability or a
  by-value-convenience wrapper would hide this.
- The `Option` return has *two* meanings (offset out of range / content over the
  100 000-byte scan limit). The scan-limit default means `line_col` returns `None` on
  any real-world file >100KB — a silent, surprising failure mode for exactly the
  files where line numbers matter most. At minimum the guide should mention it; a
  `Result` with a "raise the limit" hint would be better.
- `format_position(span)` (one call, does everything) lives in `techy::error`, not
  `techy::source` where I looked first — placement friction; and it only returns a
  preformatted `String`, so a user who wants the *numbers* still walks the long chain.

**Doc gap:** none in substance — `LineIndex`'s doc comments are excellent (offsets,
laziness, scan limit all documented). But nothing links *from* `NodeRef::span()`
toward "and here is how you display it"; I found `Source::line_index()` by scanning
`source.rs` signatures. Discovery ran outside the guide for the whole task.

---

## Task 6: malformed input and diagnostics

**Worked well:** the best-designed area of the consumer surface.
- Strict → `Err(ParseError)` with `message()`, `span()`, `render()`;
  tolerant → `Ok` with `Diagnostics` (`len`, `iter`, `has_errors`, `render_all`) —
  the success/warnings/failure triage is a clean 4-arm match, and
  `has_errors()` is exactly the right primitive.
- `render_all()` output (per-diagnostic position + "Open blocks:" traceback) is
  genuinely production quality; `Severity: Display` gives lowercase tags for free.

**Awkward:**
- **Diagnostic ordering is recovery order, not source order.** My four diagnostics
  arrived as lines 1, 4, 3, 2 (unwind order at EOF). Any user showing them in an
  editor/CLI will want source order; there is no `sort`/`by_position` helper and
  `Diagnostics` exposes no way to reorder short of copying out of `as_slice()`.
- The strict/tolerant asymmetry costs two `Language` values (recovery is baked into
  the driver at construction). Fine for a service; slightly heavy for a CLI that
  wants "--strict" as a flag. A `Language::with_recovery(..)` rebuild helper would do.
- Minor: `Diagnostics` has `iter()` but seemingly no `IntoIterator` for `&Diagnostics`
  (`for d in &result.diagnostics` — I didn't try it after seeing only `iter()` in the
  signatures; if it exists, it's undocumented at the point of use).

**Doc gap:** the guide's "Strict vs. tolerant" section shows `diagnostic.span().content()`
but never `severity()`, `message()`, `render()`, `render_all()`, or `has_errors()` —
the entire display story is only in `error.rs` doc comments. One guide paragraph
("displaying diagnostics to users") would have saved me the signature scan.

---

## Cross-cutting

1. **Module-path depth vs the root re-exports** (see Task 1): the deep paths the
   guide teaches (`techy::engine::Language`, `techy::node::extract::content_as_chars`,
   `techy::latexlike::{...}`) make the library feel bigger than the ~15 names a T1
   user actually needs; meanwhile the crate root re-exports ~100 names, many of them
   clearly S1 machinery. The curation is currently inverted for this persona.
2. **The guide is the load-bearing artifact.** Tasks whose idioms appear in
   learn-by-example.md (1, 2, 4, 6-strict) went instantly; tasks that don't (3's
   document-level text, 5's line/col chain, 6's diagnostic display) each required a
   signature hunt through `src/**`. The correlation is perfect — every doc gap above
   is a missing guide paragraph, not a missing doc comment (doc-comment quality is
   uniformly high).
3. **Panic policy held**: nothing panicked, including the four-failure malformed
   input in tolerant mode with nested unclosed group/environment/math. Recovery
   shapes were sensible (`\unknowncmd` staged as chars, in guide-documented form).
