# AI guide: pylatexenc migration

Condensed mapping tables: the Python library [pylatexenc][pyl] (generations
2 and 3) → techy. Compressed from
[Migrating from pylatexenc](crate::guide::pylatexenc_migration) (the full
chapter, with the discussion). Scope facts: techy covers pylatexenc's
*parsing layer* ([latexwalker][pyl-latexwalker-mod],
[latexnodes][pyl-latexnodes-mod], [macrospec][pyl-macrospec-mod]); it is
not a port; everything on techy's side below is the
[`latexlike`](crate::latexlike) preset unless stated otherwise. Links go
to the pylatexenc 3 documentation, which also documents the
still-supported pylatexenc 2 interfaces; where the generations differ, the
generation is named.

## Core concept map

| pylatexenc | techy | Notes |
|---|---|---|
| [`LatexWalker`][pyl-LatexWalker] | [`Language`](crate::core::Language) | walker holds one document; a `Language` holds none and is reused across documents |
| [`get_latex_nodes()`][pyl-get-latex-nodes], [`parse_content()`][pyl-parse-content] (pylatexenc 3) | [`parse()`](crate::core::Language::parse) → [`ParseResult`](crate::core::ParseResult) | tree + diagnostics; no `(nodelist, pos, len)` tuples |
| node classes of [`latexnodes.nodes`][pyl-nodes-mod] | [`NodeKind`](crate::core::node::NodeKind) via [`NodeRef`](crate::core::node::NodeRef) | closed set of five kinds — table below |
| [`MacroSpec`][pyl-MacroSpec] / [`EnvironmentSpec`][pyl-EnvironmentSpec] / [`SpecialsSpec`][pyl-SpecialsSpec] | [`MacroSpec`](crate::latexlike::MacroSpec) / [`EnvironmentSpec`](crate::latexlike::EnvironmentSpec) / [`SpecialsSpec`](crate::latexlike::SpecialsSpec) | same names, same roles; name/trigger is the registration key, not stored in the spec |
| [`LatexContextDb`][pyl-LatexContextDb] | [`Package`](crate::core::specs::Package)s on a stack of [scopes](crate::guide::concepts_overview#scopes-and-packages) | load packages / push scopes instead of filtering or extending a context |
| [`get_default_latex_context_db()`][pyl-default-db] | **no counterpart — deliberate** | techy ships only `\begin`/`\end` ([`builtin_package`](crate::latexlike::builtin_package)); register your own definitions, or load the toy [`minidefs::minilatex_package()`](crate::latexlike::minidefs::minilatex_package) for prototyping |
| argument-spec strings ([`std_macro`][pyl-std-macro], [`LatexStandardArgumentParser`][pyl-LatexStandardArgumentParser]) | [`argument_specs`](crate::latexlike::argument_specs), [`argument_specs_from_str`](crate::latexlike::argument_specs_from_str) | pylatexenc's compact strings accepted verbatim — table below |
| [`ParsingState`][pyl-ParsingState] | [`ParsingState`](crate::core::ParsingState) | techy's also owns the [`TokenRules`](crate::core::token::TokenRules) (the counterpart of pylatexenc 3's tokenization attributes: escape character, delimiters, comment marker) |
| [`ParsingStateDelta`][pyl-ParsingStateDelta] (pylatexenc 3) | [`ParsingStateDelta`](crate::core::ParsingStateDelta) | same concept, same name |
| the [`tolerant_parsing`][pyl-LatexWalker] flag | [`Recovery`](crate::error::Recovery) + [`Diagnostics`](crate::error::Diagnostics) | table below |
| [`pos` / `pos_end`][pyl-LatexNode] (pylatexenc 2: `pos` / `len`) | [`SourceSpan`](crate::source::SourceSpan) | byte range + its source — warnings below |
| parser objects ([`latexnodes.parsers`][pyl-parsers-mod], pylatexenc 3) | [construct parsers](crate::guide::construct_parsers): the spec supplies the [`ConstructParser`](crate::core::constructs::ConstructParser) for its invocations | delegation model: [The parsing model](crate::guide::parsing_model) |
| `\input` read at the latex2text stage ([`set_tex_input_directory()`][pyl-set-tex-input-directory]) | [`SourceResolver`](crate::source::SourceResolver) + [`input_macro_spec`](crate::latexlike::input_macro_spec), at **parse time** | resolved content parses into the same tree at the invocation point |
| [`LatexNodes2Text`][pyl-LatexNodes2Text] ([`latex2text`][pyl-latex2text-mod]) | **not part of techy** | comparable converter planned as a separate companion project, no promises; the machinery it would build on ships: [`recompose`](crate::recompose) (tree → any composed value), [`extract`](crate::extract) (text extraction) |
| [`latexencode`][pyl-latexencode-mod] | **no counterpart** | — |

## Node classes

All node-class references: [`latexnodes.nodes`][pyl-nodes-mod] (pylatexenc
2 exposes the same classes in `latexwalker`).

| pylatexenc class | techy | Notes |
|---|---|---|
| `LatexCharsNode` | [`Chars`](crate::core::node::NodeKind) | — |
| `LatexGroupNode` | [`Group`](crate::core::node::NodeKind) | payload records delimiters as written + group class ([`GroupData`](crate::core::node::GroupData)) |
| [`LatexMacroNode`][pyl-LatexMacroNode] | [`Callable`](crate::core::node::NodeKind::Callable) | ONE kind for all three: the classes differ by *invocation form*, recorded as data ([`CallableData`](crate::core::node::CallableData)). Form-specific views: [`macro_name()`](crate::core::node::NodeRef::macro_name) / [`environment_name()`](crate::core::node::NodeRef::environment_name) / [`specials_name()`](crate::core::node::NodeRef::specials_name); [`name()`](crate::core::node::NodeRef::name) answers for any callable |
| [`LatexEnvironmentNode`][pyl-LatexEnvironmentNode] | [`Callable`](crate::core::node::NodeKind::Callable) | body via [`body()`](crate::core::node::NodeRef::body) |
| [`LatexSpecialsNode`][pyl-LatexSpecialsNode] | [`Callable`](crate::core::node::NodeKind::Callable) | — |
| [`LatexMathNode`][pyl-LatexMathNode] (`displaytype`) | [`Group`](crate::core::node::NodeKind::Group) with a math group class | **no math node kind**: check [`is_math_group()`](crate::core::node::NodeRef::is_math_group); inline/display is [`MathGroupForm`](crate::latexlike::MathGroupForm) via [`math_form()`](crate::core::node::NodeRef::math_form); every node also records the mode it was parsed under. `\begin{equation}` stays an environment (as in pylatexenc); its *definition* switches the body to math mode ([specs chapter](crate::guide::specs#the-spec-types)) |
| `LatexCommentNode` | [`Comment`](crate::core::node::NodeKind) | comment text + trailing newline recorded separately |
| [`LatexNodeList`][pyl-LatexNodeList] | [`List`](crate::core::node::NodeKind::List) nodes; [`NodeSlice`](crate::core::node::NodeSlice) views | a `List` is a real node: tree root, environment body |

## Argument-spec strings

| Generation | Spelling | techy acceptance |
|---|---|---|
| pylatexenc 2 | single string of `*`, `{`, `[` ([`std_macro`][pyl-std-macro]) | accepted as aliases: `*`→`s`, `{`→`m`, `[`→`o` — `["[", "{"]` ≡ `["o", "m"]`, compact `"[{"` ≡ `"om"` |
| pylatexenc 3 | `xparse`-style letters `m`, `o`, `s`, … ([`LatexStandardArgumentParser`][pyl-LatexStandardArgumentParser]) | same codes: list form [`argument_specs`](crate::latexlike::argument_specs) (one code per element), compact whole-spec strings [`argument_specs_from_str`](crate::latexlike::argument_specs_from_str) (`"om"`), verbatim |

The mandatory `m` argument keeps TeX's single-expression fallback exactly
as in pylatexenc (`\frac12` = two one-character arguments; a missing group
is not diagnosed). Full code table + the fallback-off `BracedOnly` word
code: [AI guide: definitions](crate::guide::ai_guide_definitions#argument-codes).

## Behavior differences (what a pylatexenc habit gets wrong)

| Topic | pylatexenc | techy |
|---|---|---|
| Entry model | walker constructed around one string; methods take positions in it; defaults filled in (default database, tolerant on) | [`Language`](crate::core::Language) holds no document; recovery policy and definitions are explicit constructor arguments; `parse()` per document |
| Positions | Python string indices — **character** counts; `pos`/`pos_end` (2: `pos`/`len`); string held by the walker | [`SourceSpan`](crate::source::SourceSpan) — **UTF-8 byte** range carrying its `Arc` source ([`span_content()`](crate::core::node::NodeRef::span_content) needs no walker). Position arithmetic carried over unchanged is wrong on any non-ASCII document |
| Position identity | positions into the one parsed string | a tree can span several sources (`\input` parses into the same tree): equal byte ranges from different sources are different locations — span equality is source **identity** + range |
| Tolerant parsing | `tolerant_parsing=True` by default; an ignored error leaves only a log message, the caller gets no record | explicit [`Recovery`](crate::error::Recovery): `Strict` → first problem aborts (`Err` with [`ParseError`](crate::error::ParseError)); `Tolerant` → `Ok` with a whole-input tree, each problem site repaired by that condition's documented recovery AND recorded as a structured [`Diagnostic`](crate::error::Diagnostic) (severity, exact span, typed condition payload — match the type, never parse a message). Check [`has_errors()`](crate::error::Diagnostics::has_errors) before treating output as clean |
| Unknown macros | default database usually resolves them | unresolvable command = reported problem (strict and tolerant alike) until you register a definition |
| `\input` | read during latex2text ([`set_tex_input_directory()`][pyl-set-tex-input-directory]) | parse-time, opt-in: [`input_macro_spec`](crate::latexlike::input_macro_spec) + your [`SourceResolver`](crate::source::SourceResolver) ([filesystem recipe](crate::guide::specs#resolving-external-sources-input-like-inclusion)) |

Minimal working translation of the pylatexenc quick-start habit:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

// LatexWalker(text, tolerant_parsing=True).get_latex_nodes() becomes:
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse(r"Hello $x+y$ \undefined").unwrap();

// Math is a *group* with a math class, not a node class:
let math = result.tree.root().child(1).unwrap();
assert!(math.is_group() && math.is_math_group());
assert_eq!(math.span_content(), "$x+y$");

// Recovered problems are data on the result, not log lines:
assert_eq!(result.diagnostics.len(), 1); // \undefined resolves to nothing
```

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
