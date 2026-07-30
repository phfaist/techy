# API-SURFACE — techy items touched by the final T4 walkthrough code

Legend: [R] = also re-exported at the crate root (`techy::Name`); [M] = module path
only (not at the root). Import paths as actually used in the code are shown; the
latexlike preset is deliberately namespaced (never at the root).

## Types, traits, functions imported

| Item | Marker | Used in |
|---|---|---|
| `techy::engine::Language` | [R] | all tasks |
| `techy::engine::ParseResult` | [R] | task3 |
| `techy::error::Diagnostic` | [R] | task4 |
| `techy::error::Recovery` | [R] | task3, task4 |
| `techy::error::Severity` | [R] | task4 |
| `techy::error::format_position` | [R] (called as `techy::format_position` in task4) | task3, task4 |
| `techy::error::format_traceback` | [R] (called as `techy::format_traceback`) | task4 |
| `techy::constructs::UnresolvableCommand` | [R] (imported as `techy::UnresolvableCommand`) | task4 |
| `techy::node::NodeKind` | [R] | task1, task2 |
| `techy::node::NodeRef` | [R] | task1, task2 (and pervasively via inference) |
| `techy::node::extract` (module; `extract::content_as_chars`) | [M] | task3 |
| `techy::scopes::Package` | [R] | task1–4 |
| `techy::source::LineIndex` | [R] | task1 (named in a signature) |
| `techy::source::MapResolver` | [R] | task3 |
| `techy::source::Source` | [R] | task3, task4, task5 |
| `techy::source::SourceProvenance` | [R] | task3 |
| `techy::source::SourceSpan` | [R] | task3 |
| `techy::latexlike::Latexlike` | [M] | all tasks |
| `techy::latexlike::LatexlikeDriver` | [M] | task3, task4 |
| `techy::latexlike::MacroSpec` | [M] | task1–4 |
| `techy::latexlike::EnvironmentSpec` | [M] | task1, task2 |
| `techy::latexlike::CallableType` | [M] | task1–4 |
| `techy::latexlike::argument_specs` | [M] | task1–4 |

Types reached without importing (inference / return types): `techy::node::NodeTree`,
`techy::node::NodeSlice`, `techy::node::Descendants`, `techy::error::Diagnostics`,
`techy::error::ParseError`, `techy::error::TraceFrame`, `techy::source::ResolveError`,
`techy::source::SourceOrigin` (as the default `Option<String>`), `techy::source::Span`
(via `SourceSpan::range` equivalents only — never named).

## Methods / fields / variants actually called

- `Language`: `default`, `new`, `with_provider`, `with_resolver`, `parse`,
  `parse_source`, `resolve_source`
- `ParseResult`: fields `tree`, `diagnostics`
- `Package`: `new`, `insert`
- `MacroSpec::new`, `EnvironmentSpec::new`, `argument_specs`, `LatexlikeDriver::new`
- `NodeTree`: `root`, `node_count`
- `NodeRef`: `kind`, `span`, `span_content`, `child`, `children`, `descendants`,
  `chars`, `id` (debug print), `macro_name`*, `environment_name`*, `is_math_group`*,
  `group_delimiters`, `argument_content_nodes`  (* = latexlike inherent sugar)
- `NodeKind` variants matched: `Chars`, `Group`, `Callable`, `Comment`, `List`
- `NodeSlice`: `iter`, `is_empty`, `span`, `IntoIterator` (`for child in node.children()`)
- `Descendants` as `Iterator` (`find`, `filter`, `map`)
- `Source`: `new`, `with_origin`, `content`, `origin`, `line_index`, `provenance_chain`
- `SourceSpan`: `start`, `end`, `range`, `len`, `content`, `source`, `same_source`,
  `clone`, `PartialEq` (identity semantics probed in task5)
- `SourceProvenance`: variants `Primary`, `Resolved { reference, triggered_at }`,
  `Synthesized { description, triggered_at }` (public-field matching)
- `MapResolver`: `new`, `insert`, `with_reference_as_origin`
- `ResolveError`: `reference`
- `LineIndex`: `line_col`
- `Diagnostics`: `iter`, `len`, `is_empty`, `has_errors`, `suppressed`, `limit`,
  `conditions::<T>`, `render_all`, `IntoIterator for &Diagnostics`
- `Diagnostic`: `severity`, `message`, `span`, `identifier`, `frames`
- `ParseError`: `render` (`Display` implicitly via earlier probes)
- `TraceFrame`: `title`, `span`
- `Severity`: `Display`, derived `Ord` (threshold filter)
- `Recovery`: variants `Strict`, `Tolerant`
- `UnresolvableCommand`: field `name`
- `extract::content_as_chars` (returns `Cow<'_, str>`)

Rough count: **23 imported items + ~9 inferred types + ~70 distinct methods/fields/variants
≈ 100 distinct API names** to build a hover primitive, a cursor query, an include-aware
indexer, and a diagnostics renderer. For an advanced persona this felt proportionate —
no single task needed more than ~35 names, and the names composed predictably.

## Wished it existed

1. `NodeTree::node_at(offset) -> Option<NodeRef>` (innermost covering node) and/or a
   documented recipe; plus `NodeRef::parent()` / `ancestors()` — the two halves of the
   cursor primitive (Task 2).
2. An `\input` wiring story: either a preset construct that triggers the resolver
   mid-parse, or a guide chapter blessing the embedder-driven expand loop (Task 3).
3. `LineIndex::line_of(offset) -> Range<usize>` (or `line_range(line_no)`) — the line
   text needed for caret/underline excerpts (Task 4).
4. A compiler-style excerpt renderer (severity + file:line:col + source line + caret),
   or at least a machine-splittable position format alongside `format_position`.
5. `NodeKind::name() -> &'static str` (stable structural label) and a depth-carrying
   descendants iterator (Task 1).
6. A registry/table of core diagnostic identifiers ↔ condition types (Task 4).
7. One documented paragraph on re-parse: span stability (`parse_source` + own
   `Arc<Source>` vs `parse`), and the (non-)story for incremental parsing (Task 5).
8. Smaller: `LineIndex::line_col_span(span)`; a guide example obtaining a `LineIndex`
   from a *node's* source (the Arc-binding borrowck gotcha).
