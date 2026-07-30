# API surface touched by the final walkthrough code

Legend: **[root]** = also re-exported at the crate root (`techy::X`) — my code used the
module path because that is what the guide teaches; **[module]** = reachable *only* via
the module path. "Used as" notes field/method access on values of that type.

## Types and constructors

| Item | Reach | Used as |
|---|---|---|
| `techy::engine::Language` | [root] | `Language::<Latexlike>::new(driver)`, `::default()` (in earlier drafts), `.with_provider(Arc<Package>)`, `.parse(&str)` |
| `techy::engine::ParseResult` | [root] | public fields `.tree`, `.diagnostics` |
| `techy::error::Recovery` | [root] | `Recovery::Strict`, `Recovery::Tolerant` |
| `techy::error::ParseError` | [root] | `Display` (`{err}`), `.render()` |
| `techy::error::Diagnostic` | [root] | `.severity()`, `.message()`, `.span()` |
| `techy::error::Severity` | [root] | via `Display` of `severity()` (never named in code) |
| `techy::error::Diagnostics` | [root] | `.len()`, `.is_empty()`, `.iter()`, `.has_errors()`, `.render_all()` |
| `techy::latexlike::Latexlike` | [module] (deliberate) | type parameter `Language<Latexlike>` |
| `techy::latexlike::LatexlikeDriver` | [module] | `LatexlikeDriver::new(Recovery)` |
| `techy::latexlike::CallableType` | [module] | `CallableType::Macro`, `CallableType::Environment` |
| `techy::latexlike::MacroSpec` | [module] | `MacroSpec::new(Vec<Arc<ArgumentSpec>>)` |
| `techy::latexlike::EnvironmentSpec` | [module] | `EnvironmentSpec::new(Vec::new())` |
| `techy::scopes::Package` | [root] | `Package::new("walkthrough")`, `.insert(type, name, Arc<spec>)` |
| `techy::node::NodeTree` | [root] | `.root()`, `.node_count()` |
| `techy::node::NodeRef` | [root] | see accessor list below |
| `techy::node::NodeKind` | [root] | matched: `Chars { .. }`, `Group(_)`, `Callable(_)`, `Comment { .. }`, `List { .. }` |
| `techy::node::NodeSlice` | [root] | `.iter()` (from `children()`), `IntoIterator` (into `content_as_chars`) |
| `techy::node::Descendants` | [root] | as `Iterator` from `.descendants()` |
| `techy::source::SourceSpan` | [root] | `.source()`, `.start()`, `.end()`, `.range()` |
| `techy::source::Source` | [root] | `.line_index()` |
| `techy::source::LineIndex` | [root] | `.line_col(usize) -> Option<(usize, usize)>` (`&mut self`) |

## Functions

| Item | Reach | Used as |
|---|---|---|
| `techy::latexlike::argument_specs` | [module] | `argument_specs(["o", "m"]).unwrap()` |
| `techy::error::format_position` | [root] | `format_position(&SourceSpan) -> String` |
| `techy::node::extract::content_as_chars` | [module] (`extract` fns are NOT root re-exported) | `content_as_chars(NodeSlice) -> Result<Cow<str>, _>` |

## `NodeRef` accessors used

Core (`techy::node`): `kind()`, `span()`, `span_content()`, `chars()`, `name()`,
`child_count()`, `child(i)` (early drafts), `children()`, `descendants()`,
`summary()`, `is_group()`, `is_comment()`, `group_delimiters()`,
`argument_content_nodes(i)`.

Latexlike sugar (inherent on `NodeRef<'_, Latexlike>`, `techy::latexlike`):
`macro_name()`, `environment_name()`, `is_math_group()`.

## Count

~24 types/functions + ~17 methods/accessors ≈ **41 distinct API names** for the six
T1 tasks (plus `std::sync::Arc`, which the registration API forces into scope).

## Wished it existed

1. `techy::latexlike::parse(src)` / `parse_tolerant(src)` — one-call parse returning
   `ParseResult`, no `Language`/driver/turbofish knowledge needed for task 1.
2. A standard LaTeX definitions package (`\emph`, `\textbf`, `\cite`, `\item`,
   `itemize`, `enumerate`, …) — even a small, explicitly incomplete one — so the
   first realistic parse doesn't start with hand-rolled spec registration.
3. `extract::plain_text(nodes)` — document-level text extraction with a documented
   (even naive) policy for callables and specials, instead of the
   `descendants().filter_map(chars)` idiom that silently drops `~`/`--`.
4. `NodeRef::line_col()` (or `SourceSpan::line_col()`) — collapse the
   span → source → line_index → line_col chain; and a non-`&mut` `LineIndex` API.
5. `NodeKind::label()` (or `Display`) — a stable kind name for tree dumps, avoiding
   consumer matches over boxed payload variants.
6. Ergonomic spec registration: `Package::insert` taking `impl Into<Arc<_>>`, and/or
   `MacroSpec::with_args("om")`-style shorthand — removing per-definition
   `Arc::new(...)`/`.unwrap()` noise.
7. `Diagnostics::sorted_by_position()` (or documented source-order iteration) for
   editor/CLI display.
8. `Language::with_recovery(Recovery)` — toggle strict/tolerant without rebuilding
   the driver by hand.
