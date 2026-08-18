# AI guide

Orientation for an AI assistant working on a project that uses techy. This
page stands alone for the everyday flow; load at most one sub-chapter with
it (pointer table at the end). Dense by design; every rule stated here is
documented in full on the linked API item or chapter.

**What techy is.** A Rust parser toolkit for LaTeX-like markup languages:
it parses source text into a **node tree** (an Abstract Syntax Tree) that
programs read, transform, and convert. The engine has no privileged
language concepts — no built-in math mode, `{`/`}`, `%`, or `\`; the
familiar LaTeX behavior is a preset ([`latexlike`](crate::latexlike))
built from public extension points, and custom languages use the same
machinery. The crate is `no_std`-friendly (`core` + `alloc` only) and
performs no input/output of its own.

Recurring terms: a **callable** is anything invoked by name in the source
— macro, environment, specials, in LaTeX vocabulary. A **spec** is a value
describing a callable's arguments and behavior; a **package** is an
immutable collection of definitions. The **parsing state** is an immutable
snapshot of everything that can vary during a parse (definitions, token
rules, mode); definitions resolve through a stack of scopes searched
innermost-first, and can change mid-parse.

## Module topology

| Module | What lives there | Key types |
|---|---|---|
| [`techy::source`](crate::source) | source model | [`Source`](crate::source::Source), [`Span`](crate::source::Span), [`SourceSpan`](crate::source::SourceSpan), [`SourceProvenance`](crate::source::SourceProvenance), [`SourceResolver`](crate::source::SourceResolver), [`MapResolver`](crate::source::MapResolver), [`LineIndexCache`](crate::source::LineIndexCache) |
| [`techy::error`](crate::error) | diagnostics | [`Diagnostic`](crate::error::Diagnostic), [`Diagnostics`](crate::error::Diagnostics), [`ParseError`](crate::error::ParseError), [`Severity`](crate::error::Severity), [`Recovery`](crate::error::Recovery), [`DiagnosticInfo`](crate::error::DiagnosticInfo) |
| [`techy::extract`](crate::extract) | text extraction over parsed trees | [`content_as_chars`](crate::extract::content_as_chars), [`split_at_chars`](crate::extract::split_at_chars), [`parse_keyval`](crate::extract::parse_keyval) |
| [`techy::visit`](crate::visit) | read-only structural traversal | [`TreeWalker`](crate::visit::TreeWalker), [`NodeVisitor`](crate::visit::NodeVisitor), [`VisitFlow`](crate::visit::VisitFlow) |
| [`techy::transform`](crate::transform) | tree→tree transformation | [`TreeRestager`](crate::transform::TreeRestager), [`RestageVisitor`](crate::transform::RestageVisitor), [`Restage`](crate::transform::Restage), [`RestageContext`](crate::transform::RestageContext) |
| [`techy::recompose`](crate::recompose) | tree→value recomposition | [`TreeRecomposer`](crate::recompose::TreeRecomposer), [`Recomposer`](crate::recompose::Recomposer), [`Recompose`](crate::recompose::Recompose) |
| [`techy::core`](crate::core) | machinery hub: language contract, state, tokens, engine | [`Lang`](crate::core::Lang), [`ParsingState`](crate::core::ParsingState), [`ParsingStateDelta`](crate::core::ParsingStateDelta), [`TokenRules`](crate::core::TokenRules), [`Language`](crate::core::Language), [`ParseDriver`](crate::core::ParseDriver), [`ParseResult`](crate::core::ParseResult), [`TrivialLang`](crate::core::TrivialLang) |
| [`techy::core::specs`](crate::core::specs) | defining callables | [`CallableSpec`](crate::core::specs::CallableSpec), [`ArgumentSpec`](crate::core::specs::ArgumentSpec), [`SpecsProvider`](crate::core::specs::SpecsProvider), [`Package`](crate::core::specs::Package), [`Scope`](crate::core::specs::Scope) |
| [`techy::core::constructs`](crate::core::constructs) | construct parsing | [`ConstructParser`](crate::core::constructs::ConstructParser), [`ParseContext`](crate::core::constructs::ParseContext), [`ArgumentParser`](crate::core::constructs::ArgumentParser), the standard parsers + their diagnostic conditions |
| [`techy::core::node`](crate::core::node) | the node tree | [`NodeTree`](crate::core::node::NodeTree), [`NodeKind`](crate::core::node::NodeKind), [`NodeRef`](crate::core::node::NodeRef), [`NodeSlice`](crate::core::node::NodeSlice), [`GroupData`](crate::core::node::GroupData), [`CallableData`](crate::core::node::CallableData), [`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder) |
| [`techy::latexlike`](crate::latexlike) | the LaTeX-behavior preset | [`Latexlike`](crate::latexlike::Latexlike), [`LatexlikeDriver`](crate::latexlike::LatexlikeDriver), [`MacroSpec`](crate::latexlike::MacroSpec), [`EnvironmentSpec`](crate::latexlike::EnvironmentSpec), [`SpecialsSpec`](crate::latexlike::SpecialsSpec), [`argument_specs`](crate::latexlike::argument_specs), [`source_recomposer`](crate::latexlike::source_recomposer), [`minidefs`](crate::latexlike::minidefs), [`input_macro_spec`](crate::latexlike::input_macro_spec) |

Every item has exactly one canonical public path (the paths above).

## The everyday flow

```text
Package(s)  ──→  ParsingState::lang_initial_with_packages([...])  ─┐  (definitions)
LatexlikeDriver::new(Recovery::Strict|Tolerant)  ───────────────────┤  (parse-time behavior)
                                                                    ▼
                 Language::new(driver, initial_state)      — reused across documents
                 language.parse(text)                      — one call per document
                    │
                    ├─ Err(ParseError)                     — strict-mode abort
                    ▼
                 ParseResult { tree: NodeTree, diagnostics: Diagnostics }
                 tree.root() → NodeRef → children()/descendants()/arguments()/body()
```

Node structure is the closed [`NodeKind`](crate::core::node::NodeKind):
`Chars` (character run), `Group` (`{…}`, `$…$`; math is a group class, not
a node kind), `Callable` (macro/environment/specials — one kind, the form
is data), `Comment`, `List` (root, environment bodies). Every node records
its exact byte span into its source and the parsing state it was parsed
under.

## Recipes

**Parse a string.** techy ships almost no definitions (only
`\begin`/`\end`): plain text, groups, math, comments parse out of the box;
any other command needs registration (next recipe).

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("Hello {brave} $x+y$ world % bye").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, [
    "chars(Hello )", "group(Content { })", "chars( )",
    "group(Math(Inline) $ $)", "chars( world )", "comment( bye)",
]);
```

**Register a macro and an environment, parse, read the tree.** Argument
codes: `m` = mandatory `{…}`, `o` = optional `[…]`, `s` = `*` marker, `v` =
verbatim; full table in
[AI guide: definitions](crate::guide::ai_guide_definitions#argument-codes).

```rust
use techy::core::{Language, ParsingState};
use techy::core::specs::Package;
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let mut package: Package<Latexlike> = Package::new("mydefs");
package.define_macro("cite", ["o", "m"]).unwrap();
package.define_environment("enumerate", ["o"]).unwrap();

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);
let result = language
    .parse(r"\cite[p. 7]{knuth} \begin{enumerate}[(i)] a \end{enumerate}")
    .unwrap();

// Callable node: name + arguments by index (named arguments also exist).
let cite = result.tree.root().child(0).unwrap();
assert_eq!(cite.name(), Some("cite"));           // any callable
assert_eq!(cite.macro_name(), Some("cite"));     // macros only
assert_eq!(cite.argument_content_nodes(0).unwrap().source_text(), Some("p. 7"));
assert_eq!(cite.argument_content_nodes(1).unwrap().source_text(), Some("knuth"));

// Environment node: same Callable kind; the body is a marked slot.
let env = result.tree.root().child(2).unwrap();
assert_eq!(env.environment_name(), Some("enumerate"));
assert_eq!(env.argument_content_nodes(0).unwrap().source_text(), Some("(i)"));
assert_eq!(env.body().unwrap().get(0).unwrap().chars(), Some(" a "));

// Navigation: spans, flat iteration, position lookup.
assert_eq!(cite.span_content(), r"\cite[p. 7]{knuth}");
assert_eq!(cite.span().range(), 0..18);
let texts: Vec<&str> =
    result.tree.root().descendants().filter_map(|n| n.chars()).collect();
assert_eq!(texts, ["p. 7", "knuth", " ", "(i)", " a "]);
```

**Extract text.**

```rust
use techy::core::{Language, ParsingState};
use techy::core::specs::Package;
use techy::error::Recovery;
use techy::extract;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let mut package: Package<Latexlike> = Package::new("mydefs");
package.define_macro("usetikzlibrary", ["m"]).unwrap();
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);
let result = language.parse(r"\usetikzlibrary{arrows,shapes.geometric}").unwrap();
let arg = result.tree.root().child(0).unwrap().argument_content_nodes(0).unwrap();

// Flatten to a plain string (fails honestly on non-text content):
assert_eq!(extract::content_as_chars(arg).unwrap(), "arrows,shapes.geometric");
// Split at a separator, grouped content protected:
let split = extract::split_at_chars_drop_annotations(arg, ",").unwrap();
// `source_text()` answers recorded coordinates (its content here — span-tiled parse);
// `extract::content_as_chars(segment)` is the content-safe reader.
let items: Vec<&str> = split.segments().map(|s| s.source_text().unwrap()).collect();
assert_eq!(items, ["arrows", "shapes.geometric"]);
```

**Handle diagnostics.** Strict ([`Recovery`](crate::error::Recovery))
aborts at the first problem (`parse` returns `Err`); tolerant records a
[`Diagnostic`](crate::error::Diagnostic) per problem, applies that
condition's documented recovery, and returns `Ok` with a whole-input tree.
The matching rule: **match conditions via the type — `is::<T>()`,
`downcast_ref::<T>()` — or via `T::IDENTIFIER`; never spell an identifier
as a string literal.** The condition-type roster is the implementors
listing on [`DiagnosticInfo`](crate::error::DiagnosticInfo); each type's
page shows its identifier and recovery.

```rust
use techy::core::constructs::UnresolvableCommand;
use techy::core::{Language, ParsingState};
use techy::error::{DiagnosticInfo, Recovery, Severity};
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse(r"a \foo b").unwrap();
assert!(result.diagnostics.has_errors());      // always check before trusting the tree

let diagnostic = result.diagnostics.iter().next().unwrap();
assert_eq!(diagnostic.severity(), Severity::Error);
assert_eq!(diagnostic.span().content(), r"\foo ");   // exact source span
assert!(diagnostic.data().is::<UnresolvableCommand>());          // match by type
assert_eq!(diagnostic.identifier(), UnresolvableCommand::IDENTIFIER); // string boundary
let condition = diagnostic.data().downcast_ref::<UnresolvableCommand>().unwrap();
assert_eq!(condition.name, "foo");                    // typed payload fields
// Human-readable report with line/column: result.diagnostics.render_all()
```

## Pitfalls index

One line each; the pointer has the detail.

- **No definitions ship** — `\emph` is an unresolvable command until
  registered; the toy `minidefs` package is never preloaded →
  [AI guide: definitions](crate::guide::ai_guide_definitions).
- **Registered names never include the escape character** — `"\\emph"`
  silently never matches; register `"emph"` →
  [definitions § Traps](crate::guide::ai_guide_definitions#traps).
- **`m` swallows a sibling expression when the `{…}` group is missing**
  (TeX fallback, undiagnosed); `BracedOnly` turns it off →
  [definitions § Argument codes](crate::guide::ai_guide_definitions#argument-codes).
- **No spec-type/callable-type cross-check at registration** — use the
  `define_macro`/`define_environment` one-liners →
  [definitions § Traps](crate::guide::ai_guide_definitions#traps).
- **`\input` needs both halves** — the opt-in `input_macro_spec`
  definition AND a `SourceResolver` on the driver; neither is default →
  [definitions § `\input`](crate::guide::ai_guide_definitions#input-like-inclusion).
- **Match conditions by type or `T::IDENTIFIER`, never literal strings**
  → the diagnostics recipe above;
  [Running the parser](crate::guide::parsing#working-with-diagnostics).
- **Tolerant `Ok` is not clean** — check
  [`has_errors()`](crate::error::Diagnostics::has_errors); iteration is
  recovery order, use
  [`sorted_by_position`](crate::error::Diagnostics::sorted_by_position)
  for document order.
- **Spans are UTF-8 byte offsets**, not character counts; equality is
  source *identity* + range →
  [AI guide: pylatexenc migration](crate::guide::ai_guide_pylatexenc).
- **`parse()` mints a fresh anonymous source per call** — positions from
  two calls never correlate; hold `Arc<Source>` + `parse_source` →
  [AI guide: embedding](crate::guide::ai_guide_embedding).
- **`NodeRef` cannot be stored** — persistent handle is
  `Arc<NodeTree>` + `NodeId` →
  [AI guide: embedding](crate::guide::ai_guide_embedding).
- **`descendants()` excludes the starting node itself** →
  [`descendants`](crate::core::node::NodeRef::descendants).
- **`body()`: `None` = no body slot; `Some` with zero nodes = empty body**
  → [AI guide: node trees](crate::guide::ai_guide_trees).
- **`Restage::Emit` performs no automatic descent** — restage wanted
  subtree parts explicitly →
  [AI guide: node trees](crate::guide::ai_guide_trees).
- **A recomposer never resolves span content** — reconstruct from the
  node's recorded payload only; spans are provenance →
  [AI guide: node trees](crate::guide::ai_guide_trees).
- **Recompose's `Concat` skips `Attached`/`Hidden` slot children by
  default; reads (walk, descendants) visit everything** →
  [AI guide: node trees](crate::guide::ai_guide_trees).
- **Visitors and `annotate` callbacks are not `Send`** — single-threaded
  by design → [AI guide: embedding](crate::guide::ai_guide_embedding).
- **Specials in a custom `Lang` need both hooks wired**
  (`scan_specials` + `specials_trigger_chars`), else triggers silently
  never fire →
  [AI guide: custom languages](crate::guide::ai_guide_custom_lang).
- **`LineIndexCache` skips content beyond its scan cap** (default
  500 000 bytes) → [AI guide: embedding](crate::guide::ai_guide_embedding).

## Where to go

Sub-chapters (load the one matching the task):

| Task | Sub-chapter |
|---|---|
| define macros/environments/specials, argument codes, packages, scopes, `\input` | [AI guide: definitions](crate::guide::ai_guide_definitions) |
| read/navigate trees; extract, visit, transform, recompose | [AI guide: node trees](crate::guide::ai_guide_trees) |
| implement a `Lang`, driver, token rules, construct parsers | [AI guide: custom languages](crate::guide::ai_guide_custom_lang) |
| bindings, threading, multi-source, tooling positions, `no_std`, streaming | [AI guide: embedding](crate::guide::ai_guide_embedding) |
| serialize parses, trees, states, diagnostics; read them back; opt a language in | [AI guide: serialization](crate::guide::ai_guide_serialize) |
| translate pylatexenc code or concepts | [AI guide: pylatexenc migration](crate::guide::ai_guide_pylatexenc) |

Human guides (narrative depth): [Guide index](crate::guide) ·
[Introduction](crate::guide::introduction) ·
[Language syntax](crate::guide::language_syntax) ·
[Node trees](crate::guide::node_trees) ·
[Defining macros, environments, and specials](crate::guide::specs) ·
[Running the parser](crate::guide::parsing) ·
[Learn techy by example](crate::guide::learn_by_example) ·
[Concepts overview](crate::guide::concepts_overview) (stable definitions
of every major concept) · [The parsing model](crate::guide::parsing_model)
· [Custom construct parsers](crate::guide::construct_parsers) ·
[Defining a custom language](crate::guide::custom_lang) ·
[Integration](crate::guide::integration) ·
[Migrating from pylatexenc](crate::guide::pylatexenc_migration).
