# Migrating from pylatexenc

techy covers the same ground as the parsing layer of the Python library
[pylatexenc][pyl] — the [latexwalker][pyl-latexwalker-mod],
[latexnodes][pyl-latexnodes-mod], and [macrospec][pyl-macrospec-mod]
modules. It is not a port. What carries over: definition-driven parsing
(the parser consumes a command's arguments as its definition declares
them), a tree of nodes recording positions and parsing state, error
recovery that keeps parsing. What is deliberately different: techy's
engine has no built-in LaTeX behavior — the familiar syntax lives in the
[`latexlike`](crate::latexlike) preset, and everything on techy's side of
this chapter is that preset unless stated otherwise; no default
definitions database ships; recovered problems are returned as data; and
positions are spans that carry their source.

This chapter maps the main concepts and spells out the differences a
pylatexenc user would otherwise get wrong; it is deliberately short, not
exhaustive — the [Introduction](crate::guide::introduction) and the User
Guide present techy on its own terms. Mappings cover both pylatexenc 2 and
pylatexenc 3; where the generations differ, the generation is named. Links
go to the pylatexenc 3 documentation, which also documents the
still-supported pylatexenc 2 interfaces.

## Concept map

| pylatexenc | techy | notes |
|---|---|---|
| [`LatexWalker`][pyl-LatexWalker] | [`Language`](crate::core::Language) | the walker holds one document; a `Language` is reused across documents |
| [`get_latex_nodes()`][pyl-get-latex-nodes], [`parse_content()`][pyl-parse-content] (pylatexenc 3) | [`parse()`](crate::core::Language::parse) → [`ParseResult`](crate::core::ParseResult) | tree plus diagnostics; no `(nodelist, pos, len)` tuples |
| the node classes of [`latexnodes.nodes`][pyl-nodes-mod] | [`NodeKind`](crate::core::node::NodeKind), read via [`NodeRef`](crate::core::node::NodeRef) | a closed set of five kinds |
| [`LatexMacroNode`][pyl-LatexMacroNode] / [`LatexEnvironmentNode`][pyl-LatexEnvironmentNode] / [`LatexSpecialsNode`][pyl-LatexSpecialsNode] | the one [`Callable`](crate::core::node::NodeKind::Callable) kind | invocation form is data on the node, not a class |
| [`LatexMathNode`][pyl-LatexMathNode] | a [`Group`](crate::core::node::NodeKind::Group) node with a math group class | math is not a node kind |
| [`LatexNodeList`][pyl-LatexNodeList] | [`List`](crate::core::node::NodeKind::List) nodes; [`NodeSlice`](crate::core::node::NodeSlice) | a `List` is a real node: the tree root, an environment body |
| [`MacroSpec`][pyl-MacroSpec] / [`EnvironmentSpec`][pyl-EnvironmentSpec] / [`SpecialsSpec`][pyl-SpecialsSpec] | [`MacroSpec`](crate::latexlike::MacroSpec) / [`EnvironmentSpec`](crate::latexlike::EnvironmentSpec) / [`SpecialsSpec`](crate::latexlike::SpecialsSpec) | same names, same roles; the name or trigger is a registration key, not stored in the spec |
| [`LatexContextDb`][pyl-LatexContextDb], [`get_default_latex_context_db()`][pyl-default-db] | [`Package`](crate::core::specs::Package)s on a stack of [scopes](crate::guide::concepts_overview#scopes-and-packages); **no default database** | load packages, or push a scope for a region, instead of filtering or extending a context — [see below](#no-default-definitions-database) |
| argument-spec strings ([`std_macro`][pyl-std-macro], [`LatexStandardArgumentParser`][pyl-LatexStandardArgumentParser]) | argument codes: [`argument_specs`](crate::latexlike::argument_specs), [`argument_specs_from_str`](crate::latexlike::argument_specs_from_str) | pylatexenc's compact strings accepted verbatim |
| [`ParsingState`][pyl-ParsingState] | [`ParsingState`](crate::core::ParsingState) | techy's also owns the [`TokenRules`](crate::core::TokenRules) — the counterpart of pylatexenc 3's tokenization attributes (escape character, delimiters, comment marker) |
| [`ParsingStateDelta`][pyl-ParsingStateDelta] (pylatexenc 3) | [`ParsingStateDelta`](crate::core::ParsingStateDelta) | same concept, same name |
| the [`tolerant_parsing`][pyl-LatexWalker] flag | [`Recovery`](crate::error::Recovery) policy + [`Diagnostics`](crate::error::Diagnostics) | recovered problems become data |
| [`pos` / `pos_end`][pyl-LatexNode] (pylatexenc 2: `pos` / `len`) | [`SourceSpan`](crate::source::SourceSpan) | a byte range together with its source |
| parser objects ([`latexnodes.parsers`][pyl-parsers-mod], pylatexenc 3) | [construct parsers](crate::guide::construct_parsers): specs supply the [`ConstructParser`](crate::core::constructs::ConstructParser) for their invocations | [The parsing model](crate::guide::parsing_model) explains the delegation |
| `\input` read at the latex2text stage ([`set_tex_input_directory()`][pyl-set-tex-input-directory]) | [`SourceResolver`](crate::source::SourceResolver), at parse time | [see below](#latex2text-latexencode-and-input) |
| [`LatexNodes2Text`][pyl-LatexNodes2Text] ([`latex2text`][pyl-latex2text-mod]) | — not part of techy | [see below](#latex2text-latexencode-and-input) |

## One `Language`, many documents

A [`LatexWalker`][pyl-LatexWalker] is constructed around the string it
parses, and its parsing methods take positions within that string. techy
inverts the arrangement: a [`Language`](crate::core::Language) holds no
document at all — it bundles what outlives one parse, the *driver*
(parse-time behavior) and the initial
[parsing state](crate::guide::concepts_overview#parsing-state-and-deltas)
(token rules, mode, and the definitions you load) — and each document is
passed to [`parse()`](crate::core::Language::parse).

Where pylatexenc fills in defaults (the default definitions database,
tolerant parsing switched on), techy makes both choices explicit
constructor arguments:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

// One `Language` per language, reused for every document:
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial().expect("seed state"),
);

let result = language.parse(r"Hello $x+y$ \undefined").unwrap();

// Math parses as a *group* node carrying a math group class:
let math = result.tree.root().child(1).unwrap();
assert!(math.is_group());
assert!(math.is_math_group());
assert_eq!(math.span_content(), "$x+y$");

// Recovered problems are data on the result (`\undefined` resolves to nothing):
assert_eq!(result.diagnostics.len(), 1);
```

## Node kinds: one callable kind, no math node

pylatexenc gives each construct its own node class in
[`latexnodes.nodes`][pyl-nodes-mod] (pylatexenc 2 exposes the same classes
in `latexwalker`). techy's tree has a closed set of five node kinds
([`NodeKind`](crate::core::node::NodeKind)), and two of the mappings are
not one-to-one:

- **Macros, environments, and specials are one node kind.**
  [`LatexMacroNode`][pyl-LatexMacroNode],
  [`LatexEnvironmentNode`][pyl-LatexEnvironmentNode], and
  [`LatexSpecialsNode`][pyl-LatexSpecialsNode] all map to the single
  `Callable` kind: a **callable** is anything invoked by name in the source
  ([callable specs](crate::guide::concepts_overview#callable-specs-and-arguments)),
  and the three pylatexenc classes differ by *invocation form*, which techy
  records as data on the node
  ([`CallableData`](crate::core::node::CallableData)) rather than as a
  class distinction. On latexlike trees,
  [`macro_name()`](crate::core::node::NodeRef::macro_name) and its
  environment/specials siblings give the form-specific view;
  [`name()`](crate::core::node::NodeRef::name) answers for any callable.
- **There is no math node.** Where pylatexenc parses `$…$` into a dedicated
  [`LatexMathNode`][pyl-LatexMathNode] with a `displaytype` attribute, the
  latexlike preset parses it as a *group* — the `Group` kind — whose group
  class is a math class
  ([`is_math_group()`](crate::core::node::NodeRef::is_math_group)); the
  inline/display distinction is recorded on the group as its
  [`MathGroupForm`](crate::latexlike::MathGroupForm), and every node also
  records the parsing mode it was parsed under. `\begin{equation}` behaves
  as in pylatexenc: an environment (a callable), never a math group — in
  techy, its *definition* switches the body to math mode
  ([the specs chapter](crate::guide::specs) shows the one-liner).

The other mappings in the [concept map](#concept-map) are direct;
[Node trees](crate::guide::node_trees) is the full tour of the tree and
its consumers.

## No default definitions database

Construct a [`LatexWalker`][pyl-LatexWalker] without a `latex_context`
argument and it silently uses [`get_default_latex_context_db()`][pyl-default-db], whose
`'latex-base'` category alone covers the bulk of standard LaTeX. techy
deliberately ships almost nothing: the only built-in definitions are
`\begin` and `\end` ([`builtin_package`](crate::latexlike::builtin_package)).
Every other macro, environment, or specials sequence resolves to nothing
until you register a definition for it — and an unresolvable command is a
reported problem, in strict and tolerant parsing alike. What to do
instead:

- **Register the definitions your application needs.** A definition is a
  *spec* — a value describing a callable's arguments and behavior —
  registered in a [`Package`](crate::core::specs::Package);
  [Defining macros, environments, and specials](crate::guide::specs) is the
  chapter.
- **For quick starts and debugging**, the opt-in
  [`minidefs::minilatex_package()`](crate::latexlike::minidefs::minilatex_package)
  provides a handful of familiar definitions — deliberately a toy, not a
  database.

## Positions are spans that carry their source

A pylatexenc node stores integer positions into the parsed string —
[`pos` and `pos_end`][pyl-LatexNode] (pylatexenc 2: `pos` and `len`) —
with the string itself held by the walker. A techy node instead carries a
[`SourceSpan`](crate::source::SourceSpan): a byte range plus a shared
reference to the [`Source`](crate::source::Source) it points into, so
[`span_content()`](crate::core::node::NodeRef::span_content) resolves the
original text with no walker and no external lookup. Two habits do not
port:

- **Offsets count UTF-8 bytes.** pylatexenc's positions are Python string
  indices, which count characters; techy's spans are byte ranges. Position
  arithmetic carried over unchanged is wrong on any non-ASCII document.
- **A position is only meaningful together with its source.** A techy tree
  can span several sources — `\input`-style inclusion parses the referenced
  content into the same tree — so equal byte ranges from different sources
  are different locations. The span carries its source for exactly this
  reason, and span equality is source identity plus byte range.

## Tolerant parsing produces diagnostics

Both libraries can recover from problems and keep parsing; the difference
is what you learn about it. In pylatexenc, `tolerant_parsing` is switched
on by default, and an ignored error leaves only a log message — the caller
gets no record of what was recovered. In techy the policy is an explicit
choice, the [`Recovery`](crate::error::Recovery) argument of the driver:

- **`Recovery::Strict`** — parsing stops at the first problem: `parse`
  returns `Err` with a [`ParseError`](crate::error::ParseError).
- **`Recovery::Tolerant`** — parsing continues and `parse` returns `Ok`:
  the tree covers the whole input with each problem site repaired by that
  problem's documented recovery, and every problem is recorded on the
  result as a structured [`Diagnostic`](crate::error::Diagnostic) —
  severity, exact source span, and a typed condition payload to match on,
  never a message string to parse.

Check [`has_errors()`](crate::error::Diagnostics::has_errors) before
treating tolerant output as clean.
[Running the parser](crate::guide::parsing) covers the policies and the
diagnostics toolkit.

## Argument specification strings

pylatexenc 2 declares a macro's arguments as a single string of `*`, `{`,
and `[` characters ([`std_macro`][pyl-std-macro]); pylatexenc 3 also
accepts the `xparse`-style letters `m`, `o`, `s`, … as standard argument
types ([`LatexStandardArgumentParser`][pyl-LatexStandardArgumentParser]).
techy uses the same `xparse`-style codes and accepts the pylatexenc 2
characters as aliases. The list form
[`argument_specs`](crate::latexlike::argument_specs) takes one code per
argument (`["o", "m"]` — and `["[", "{"]` declares the same arguments);
[`argument_specs_from_str`](crate::latexlike::argument_specs_from_str)
accepts the compact whole-spec strings pylatexenc definitions are written
in, verbatim (`"om"`, `"[{"`). The full code table — including the trap to
know about: the mandatory `m` argument keeps TeX's single-expression
fallback, exactly as in pylatexenc — is on
[`argument_specs`](crate::latexlike::argument_specs).

## latex2text, latexencode, and `\input`

techy corresponds to pylatexenc's parsing layer only.
[`LatexNodes2Text`][pyl-LatexNodes2Text] — the
[`latex2text`][pyl-latex2text-mod] LaTeX-to-plain-text converter — is not
part of techy; a comparable converter is planned as a separate companion
project, and this guide makes no promises about it. What techy does ship
is the machinery such a converter builds on:
[`recompose`](crate::recompose) folds a parsed tree into any composed
value (plain text, HTML, regenerated source), and
[`extract`](crate::extract) answers the everyday text-extraction
questions. [`latexencode`][pyl-latexencode-mod] (Unicode text into LaTeX
escape sequences) has no techy counterpart.

One responsibility moved between layers: in pylatexenc, `\input` files are
read during the latex2text stage
([`set_tex_input_directory()`][pyl-set-tex-input-directory]); in techy,
inclusion happens at *parse time* — the opt-in
[`input_macro_spec`](crate::latexlike::input_macro_spec) definition asks
the application-supplied [`SourceResolver`](crate::source::SourceResolver)
for the referenced content and parses it into the same tree at the
invocation point.
[The specs chapter](crate::guide::specs#resolving-external-sources-input-like-inclusion)
has the standard filesystem recipe.

Read next: [Learn techy by example](crate::guide::learn_by_example) — the
shipped behavior in small, complete, compile-checked examples.

[pyl]: https://pylatexenc.readthedocs.io/
[pyl-latexwalker-mod]: https://pylatexenc.readthedocs.io/en/latest/latexwalker/
[pyl-latexnodes-mod]: https://pylatexenc.readthedocs.io/en/latest/latexnodes/
[pyl-macrospec-mod]: https://pylatexenc.readthedocs.io/en/latest/macrospec/
[pyl-LatexWalker]: https://pylatexenc.readthedocs.io/en/latest/latexwalker/#pylatexenc.latexwalker.LatexWalker
[pyl-get-latex-nodes]: https://pylatexenc.readthedocs.io/en/latest/latexwalker/#pylatexenc.latexwalker.LatexWalker.get_latex_nodes
[pyl-parse-content]: https://pylatexenc.readthedocs.io/en/latest/latexwalker/#pylatexenc.latexwalker.LatexWalker.parse_content
[pyl-default-db]: https://pylatexenc.readthedocs.io/en/latest/latexwalker/#pylatexenc.latexwalker.get_default_latex_context_db
[pyl-nodes-mod]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.nodes/
[pyl-LatexNode]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.nodes/#pylatexenc.latexnodes.nodes.LatexNode
[pyl-LatexMacroNode]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.nodes/#pylatexenc.latexnodes.nodes.LatexMacroNode
[pyl-LatexEnvironmentNode]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.nodes/#pylatexenc.latexnodes.nodes.LatexEnvironmentNode
[pyl-LatexSpecialsNode]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.nodes/#pylatexenc.latexnodes.nodes.LatexSpecialsNode
[pyl-LatexMathNode]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.nodes/#pylatexenc.latexnodes.nodes.LatexMathNode
[pyl-LatexNodeList]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.nodes/#pylatexenc.latexnodes.nodes.LatexNodeList
[pyl-MacroSpec]: https://pylatexenc.readthedocs.io/en/latest/macrospec/#pylatexenc.macrospec.MacroSpec
[pyl-EnvironmentSpec]: https://pylatexenc.readthedocs.io/en/latest/macrospec/#pylatexenc.macrospec.EnvironmentSpec
[pyl-SpecialsSpec]: https://pylatexenc.readthedocs.io/en/latest/macrospec/#pylatexenc.macrospec.SpecialsSpec
[pyl-LatexContextDb]: https://pylatexenc.readthedocs.io/en/latest/macrospec/#pylatexenc.macrospec.LatexContextDb
[pyl-std-macro]: https://pylatexenc.readthedocs.io/en/latest/macrospec/#pylatexenc.macrospec.std_macro
[pyl-ParsingState]: https://pylatexenc.readthedocs.io/en/latest/latexnodes/#pylatexenc.latexnodes.ParsingState
[pyl-ParsingStateDelta]: https://pylatexenc.readthedocs.io/en/latest/latexnodes/#pylatexenc.latexnodes.ParsingStateDelta
[pyl-parsers-mod]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.parsers/
[pyl-LatexStandardArgumentParser]: https://pylatexenc.readthedocs.io/en/latest/latexnodes.parsers/#pylatexenc.latexnodes.parsers.LatexStandardArgumentParser
[pyl-latex2text-mod]: https://pylatexenc.readthedocs.io/en/latest/latex2text/
[pyl-LatexNodes2Text]: https://pylatexenc.readthedocs.io/en/latest/latex2text/#pylatexenc.latex2text.LatexNodes2Text
[pyl-set-tex-input-directory]: https://pylatexenc.readthedocs.io/en/latest/latex2text/#pylatexenc.latex2text.LatexNodes2Text.set_tex_input_directory
[pyl-latexencode-mod]: https://pylatexenc.readthedocs.io/en/latest/latexencode/
