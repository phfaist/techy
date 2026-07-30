# API surface touched — extender persona (T2)

Every fully-qualified techy item the final code (`extender-examples/src/main.rs` +
`src/bin/probes.rs`) touches. "root" = also re-exported at the crate root
(`techy::X`); "module-only" = reachable only via the module path shown.

## Types / functions imported

| Item | Availability |
|---|---|
| `techy::engine::Language` | root (`techy::Language`) |
| `techy::error::Recovery` | root |
| `techy::latexlike::Latexlike` | module-only |
| `techy::latexlike::LatexlikeDriver` | module-only |
| `techy::latexlike::CallableType` | module-only |
| `techy::latexlike::MacroSpec` | module-only |
| `techy::latexlike::EnvironmentSpec` | module-only |
| `techy::latexlike::argument_specs` | module-only |
| `techy::latexlike::argument_specs_from_str` | module-only |
| `techy::scopes::Package` | root |
| `techy::spec::ArgumentSpec` | root |
| `techy::state::ParsingStateDelta` | root |

Note: nothing from `latexlike` (the extender's home module) is re-exported at the
root; most of what *is* at the root (token, constructs, engine internals) the
extender never touches.

## Methods / fields used (by receiver)

- `Language`: `default()`, `new(driver)`, `with_provider(Arc<dyn SpecsProvider>)`,
  `parse(&str)`
- `ParseResult` (implicit, via `parse`): fields `.tree`, `.diagnostics`
- `NodeTree`: `root()`
- `NodeRef` (incl. latexlike extensions): `child(i)`, `children()`, `summary()`,
  `chars()`, `is_group()`, `span_content()`, `macro_name()`, `environment_name()`,
  `arguments()`, `argument_content_nodes(i)`, `argument_content_nodes_named(name)`,
  `body()`
- `NodeSlice`: `iter()`, `get(i)`, `len()`, `source_text()`
- `ParsedArguments`: `get(i)`, `len()`
- `ParsedArgument`: `is_provided()`
- `Diagnostics`: `has_errors()`, `iter()`
- `Diagnostic`: `severity()`, `identifier()`, `span()`, `render()`
- `SourceSpan`: `range()`
- `ParseError`: `Display`, `identifier()`, `render()`
- `Package`: `new(name)`, `insert(callable_type, name, spec)`
- `MacroSpec`: `new(Vec<Arc<ArgumentSpec>>)`
- `EnvironmentSpec`: `new(...)`, `with_body_delta(delta)`
- `ArgumentSpec`: `new(parser)`, `named(name)`, field `.parser`
- `ParsingStateDelta`: `new()`, `push_provider(provider)`
- `LatexlikeDriver`: `new(Recovery)`

Rough count: 12 imported items + ~40 methods/fields ≈ **~50 distinct names**, of
which the tasks-1-to-3 happy path needs about **20**.

## Wished it existed

- `Package::define_macro(name, codes)` / `define_environment(name, codes)` — a
  one-liner for the dominant shape (no `Arc`, no `CallableType`, no spec type).
- Insert-time rejection (or debug warning) of definition names that start with the
  escape character (`"\greet"`), or a resolve error that mentions near-miss keys.
- `MacroSpec::empty()` (or `Default`) for zero-argument macros.
- A named-arguments variant of the code factory, e.g.
  `argument_specs([("o", "greeting"), ("m", "name")])`.
- An argument code for "mandatory braced group, **no** single-expression fallback".
- `EnvironmentSpec::with_body_provider(Arc<Package>)` — the `with_provider`-style
  sugar at environment level, so scoped definitions don't require
  `state::ParsingStateDelta`.
- A canned text-mode-argument helper in `latexlike` (the `\text`/`\mbox` shape),
  replacing the guide's four-internal-imports recipe.
- A distinguishable return (or at least a documented contract) for
  `argument_content_nodes`: absent optional vs. out-of-range index.
- `Language::with_providers(impl IntoIterator<...>)` (minor).
- A generic `callable_name()` beside the three per-type name getters (minor).
