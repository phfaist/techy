# Learn techy by example

A tour of the `latexlike` preset — the familiar LaTeX behavior assembled from techy's
generic core — in small, complete, runnable examples. Every code block on this page is
compiled and executed as a doctest, so what you read here is what the library does.
The examples use `unwrap()` for brevity; real embedders will want to keep the
`Result`s (every fallible operation returns one — techy never panics on input). Where
pylatexenc is mentioned, the repository's acceptance suite
(`techy/tests/acceptance.rs`) pins the parity claim.

## Your first parse

A [`Language`](crate::core::Language) bundles everything that outlives one parse:
the seed parsing state and the parse driver (which carries the optional source
resolver for `\input`-like lookups). Define it once, parse
many documents. The [`Latexlike`](crate::latexlike::Latexlike) defaults give you the
canonical tokenization (`\` commands, `{…}` groups, `$…$` math, `%` comments) and the
`"_builtin"` package (the `\begin`/`\end` environment dispatch):

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("Hello {brave} world!").unwrap();

// The tree's root is a List node covering the whole input; its children are the
// top-level content.
let root = result.tree.root();
assert_eq!(root.child_count(), 3);

// `NodeRef::summary()` renders a compact one-line description per node — the
// assertion/logging companion you will see throughout this page.
let shapes: Vec<String> = root.children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, ["chars(Hello )", "group(Content { })", "chars( world!)"]);
```

No definitions are registered by default — techy deliberately ships no LaTeX
definitions database — so `\emph` starts out as an *unresolvable command*.
Everything below registers what it needs; that is the intended embedder workflow.
For quick experiments,
[`minidefs::minilatex_package()`](crate::latexlike::minidefs::minilatex_package)
ships a toy package (`\emph`, `\textbf`, `\textit`, the list environments, and the
typography specials) you can load explicitly.

## Reading nodes: kinds, spans, provenance

Every node knows exactly where it came from. Spans are byte ranges into the source,
and the original text is always reachable — reading a node's source spelling needs
no lookup tables:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("one {two} three").unwrap();
let group = result.tree.root().child(1).unwrap();

assert!(group.is_group());
assert_eq!(group.span().range(), 4..9);
assert_eq!(group.span_content(), "{two}");           // the node's exact source text
assert_eq!(group.group_delimiters(), Some(("{", "}")));
assert_eq!(group.child(0).unwrap().chars(), Some("two"));

// Sibling runs are `NodeSlice` views with *exact* covering spans: the preset's
// sibling spans are adjacent, so the covering span is the run's own text (span
// tiling — `Lang::OBEYS_SPAN_TILING`).
let children = result.tree.root().children();
assert_eq!(children.span().unwrap().range(), 0..15);
assert_eq!(children.source_text(), Some("one {two} three"));

// Document-order traversal of everything *beneath* a node — `descendants()`
// excludes the starting node itself:
let texts: Vec<&str> = result
    .tree
    .root()
    .descendants()
    .filter_map(|node| node.chars())
    .collect();
assert_eq!(texts, ["one ", "two", " three"]);
```

## Defining macros

Definitions live in [`SpecsProvider`](crate::core::specs::SpecsProvider)s on the parsing
state's scope stack. The everyday provider is a [`Package`](crate::core::specs::Package):
immutable, built once, loaded wholesale. Register a
[`MacroSpec`](crate::latexlike::MacroSpec) under
[`CallableType::Macro`](crate::latexlike::CallableType) and build the language's seed
with the package via
[`ParsingState::lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages):

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec};
use techy::core::specs::Package;

let mut package = Package::new("mydefs");
// Argument structures come from xparse-like codes, one code string per argument:
// `o` = optional `[…]`, `m` = mandatory `{…}` (with the single-expression
// fallback). Compact whole-spec strings go through `argument_specs_from_str`.
package.insert(
    CallableType::Macro,
    "cite",
    MacroSpec::new(argument_specs(["o", "m"]).unwrap()),
);

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

let result = language.parse(r"see \cite[Lemma 3]{Author}!").unwrap();
let cite = result.tree.root().child(1).unwrap();
assert_eq!(cite.macro_name(), Some("cite"));
// The per-form getters (`macro_name()`, `environment_name()`, `specials_name()`)
// filter by invocation form; the generic `name()` answers for any callable.
assert_eq!(cite.name(), Some("cite"));
assert_eq!(cite.span_content(), r"\cite[Lemma 3]{Author}");

// Arguments are self-describing records: which are provided, and where their
// content nodes live.
assert!(cite.arguments().unwrap().get(0).unwrap().is_provided());
let optional = cite.argument_content_nodes(0).unwrap();
assert_eq!(optional.source_text(), Some("Lemma 3"));
let mandatory = cite.argument_content_nodes(1).unwrap();
assert_eq!(mandatory.source_text(), Some("Author"));
```

The same registration in one line:
[`define_macro`](crate::core::specs::Package::define_macro) /
[`define_environment`](crate::core::specs::Package::define_environment) are preset
shorthands over exactly this `insert` operation — not a second registration model —
pairing the callable type and spec type correctly by construction:

```rust
use techy::core::specs::Package;
use techy::latexlike::Latexlike;

let mut package: Package<Latexlike> = Package::new("mydefs");
package.define_macro("cite", ["o", "m"]).unwrap();
package.define_environment("enumerate", ["o"]).unwrap();
```

The codes are shorthand, not the only spelling: each code resolves to a configured
argument-parser value you can also build directly (that is how per-argument state
deltas and non-default options are attached), names come from
[`argument_specs_named`](crate::latexlike::argument_specs_named), and the full
code table — including `BracedOnly`, the mandatory group *without* the
single-expression fallback — is on
[`argument_specs`](crate::latexlike::argument_specs).

Absent optional arguments are recorded, not invented — and a trailing optional at end
of input is no error (a pylatexenc-parity fix):

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec};
use techy::core::specs::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "item",
    MacroSpec::new(argument_specs(["o"]).unwrap()),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

let result = language.parse(r"\item plain").unwrap();
let item = result.tree.root().child(0).unwrap();
assert!(!item.arguments().unwrap().get(0).unwrap().is_provided());
// The blank after `\item` is the trigger token's own post-space, nothing more:
assert_eq!(item.post_space(), Some(" "));
assert_eq!(item.span_content(), r"\item ");
```

Providers shadow innermost-first: pushing a package that redefines a name wins over
what sits below it — that is the whole scoping model, there is no separate override
mechanism.

## Math modes

Math is not a core concept — it is preset data. The default rules declare `$…$`,
`$$…$$`, `\(…\)`, and `\[…\]` as delimiter pairs of the single
[`GroupType::Math`](crate::latexlike::GroupType) class, and the driver's descent
delta parses their interiors in [`Mode::Math`](crate::latexlike::Mode). Inline vs.
display is the group's [`MathGroupForm`](crate::latexlike::MathGroupForm) — typed
class payload each delimiter rule declares at registration, read back by
[`math_form`](crate::core::node::NodeRef::math_form) (no delimiter table; a custom
registered pair carries its declared form like the built-ins):

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver, MathGroupForm, Mode};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse(r"a $x+y$ b \[z\]").unwrap();

let inline = result.tree.root().child(1).unwrap();
assert!(inline.is_math_group());
assert_eq!(inline.math_form(), Some(MathGroupForm::Inline));
// The interior was parsed in math mode; the node itself sits in the surrounding
// text-mode content. Every node records the state it was parsed under.
assert_eq!(inline.child(0).unwrap().parsing_state().mode(), Mode::Math);
assert_eq!(inline.parsing_state().mode(), Mode::Text);

let display = result.tree.root().child(3).unwrap();
assert_eq!(display.math_form(), Some(MathGroupForm::Display));
assert_eq!(display.group_delimiters(), Some((r"\[", r"\]")));
```

`$a$$b$` is two inline groups, not a display group — at a close position the expected
closer wins (pylatexenc's dollar-boundary parity):

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("$a$$b$").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, ["group(Math(Inline) $ $)", "group(Math(Inline) $ $)"]);
```

A macro like `\text{…}` must parse its argument back in *text* mode even inside
display math. That is per-argument data: an
[`ArgumentSpec`](crate::core::specs::ArgumentSpec) can carry a parsing-state
delta, which for this shape spells the preset's exit-math-context event —
restoring the enclosing non-math context, whatever it is, for exactly that
argument. The full worked recipe is on
[`Event::ExitMathContext`](crate::latexlike::Event)'s documentation.

## Environments

`\begin{name} … \end{name}` is a preset composition: `\begin` and `\end` are ordinary
macro entries of the `"_builtin"` package whose parsers dispatch the environment's own spec —
an [`EnvironmentSpec`](crate::latexlike::EnvironmentSpec) registered under
[`CallableType::Environment`](crate::latexlike::CallableType). The parsed environment
is a callable node whose *body* is a slot:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{
    argument_specs, CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver,
};
use techy::core::specs::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Environment,
    "enumerate",
    EnvironmentSpec::new(argument_specs(["o"]).unwrap()),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

let result = language.parse(r"\begin{enumerate}[(i)] a b \end{enumerate}").unwrap();
let env = result.tree.root().child(0).unwrap();
assert_eq!(env.environment_name(), Some("enumerate"));
assert_eq!(env.argument_content_nodes(0).unwrap().source_text(), Some("(i)"));

// `body()` selects the marked body slot. It answers `None` for non-callables
// and for callables without a body slot (an ordinary macro node) — a `Some`
// with zero nodes, by contrast, is an empty body.
let body: Vec<String> = env.body().unwrap().iter().map(|node| node.summary()).collect();
assert_eq!(body, ["chars( a b )"]);
```

Registration deliberately performs no spec-type/callable-type cross-check — the
composition owns the environment parse and any spec's declared arguments
contribute; see [`Package::insert`](crate::core::specs::Package::insert) and
[the specs chapter](crate::guide::specs#registration-pitfalls).

An environment's definition can install a parsing-state delta for its body's whole
extent — a mode change (`equation` entering math mode; see
[the specs chapter](crate::guide::specs#the-spec-types)) or a **scope operation**:
definitions that exist only inside the body. The shipped exemplar is
[`minidefs`](crate::latexlike::minidefs)' `\item`, whose list environments push an
inner package for their bodies:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::minilatex_package;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([minilatex_package()]).expect("seed state"),
);

// Inside a list body, `\item` resolves (here with its optional argument)…
let result = language.parse(r"\begin{itemize}\item[a] x\end{itemize}").unwrap();
let body: Vec<String> = result.tree.root().child(0).unwrap().body().unwrap()
    .iter().map(|node| node.summary()).collect();
assert_eq!(body, ["Macro(item)", "chars( x)"]);

// …outside a list body, the very same name is undefined.
assert!(language.parse(r"\item x").is_err());
```

## Verbatim

Raw regions never tokenize their content. `\verb` is a macro whose argument is the
`v` code (auto-matched delimiter), and `verbatim` is an environment with the
[`VerbatimBehavior`](crate::latexlike::VerbatimBehavior) body — both produce
group+chars shapes with the raw text as ordinary chars content:

```rust
use std::sync::Arc;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{
    argument_specs, CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, MacroSpec,
    VerbatimBehavior,
};
use techy::core::specs::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "verb",
    MacroSpec::new(argument_specs(["v"]).unwrap()),
);
package.insert(
    CallableType::Environment,
    "verbatim",
    EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default())),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

// `\verb|…|`: everything between the delimiters is raw — `%`, `\`, `{`, `$` inert.
let result = language.parse(r"\verb|a%\x{| z").unwrap();
let verb = result.tree.root().child(0).unwrap();
assert_eq!(
    verb.argument_content_nodes(0).unwrap().source_text(),
    Some(r"a%\x{"),
);

// The `verbatim` environment: the body runs to the literal `\end{verbatim}`; the
// newline right after `\begin{verbatim}` is gobbled (staged, but not body content).
let result = language
    .parse("\\begin{verbatim}\na % b \\x{\n\\end{verbatim}")
    .unwrap();
let env = result.tree.root().child(0).unwrap();
let body = env.body().unwrap();
assert_eq!(body.get(0).unwrap().chars(), Some("a % b \\x{\n"));
```

## Specials

Specials are trigger character sequences resolved through the scope stack. The seed
ships none — typography interpretation is definitions content, not parsing
substrate — but the opt-in
[`minidefs::minilatex_package()`](crate::latexlike::minidefs::minilatex_package)
carries the familiar set (`~` and the text-only typography ligatures ``` `` ```,
`''`, `--`, `---`); the scan takes the longest match, and per-entry mode
visibility keeps the ligatures out of math:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::minilatex_package;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([minilatex_package()]).expect("seed state"),
);
let result = language.parse("x---y--z").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, ["chars(x)", "Specials(---)", "chars(y)", "Specials(--)", "chars(z)"]);
```

Your own specials are package entries too — including ones that take arguments:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, SpecialsSpec};
use techy::core::specs::Package;

let mut package = Package::new("mydefs");
package.insert_specials(
    CallableType::Specials,
    "_",
    SpecialsSpec::new(argument_specs(["m"]).unwrap()),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

let result = language.parse("x_{down}").unwrap();
let sub = result.tree.root().child(1).unwrap();
assert_eq!(sub.specials_name(), Some("_"));
assert_eq!(sub.argument_content_nodes(0).unwrap().source_text(), Some("down"));
```

## Paragraph breaks

A whitespace run containing two or more newlines is a paragraph break. By default it
becomes a whitespace-only chars node; the specials-node shape (as in pylatexenc's
current major version) is one driver flag away
([`ParagraphBreakStyle`](crate::latexlike::ParagraphBreakStyle) — a driver
emission policy, deliberately not package data: the tokenizer detects paragraph
breaks before the specials scan could ever run):

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver, ParagraphBreakStyle};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict)
        .with_paragraph_break_style(ParagraphBreakStyle::Specials),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("one\n\ntwo").unwrap();

let break_node = result.tree.root().child(1).unwrap();
// The node's name is the actual whitespace run as written, and its span covers
// the same run. Identify paragraph-break nodes by spec identity — the stamped
// spec is the canonical `techy::latexlike::ParagraphBreakSpec`, recognized by
// `Any`-downcast — never by a name spelling.
assert_eq!(break_node.specials_name(), Some("\n\n"));
assert_eq!(break_node.span().range(), 3..5);
```

## Strict vs. tolerant parsing

The recovery policy lives on the driver. Strict parses abort on the first error
(`parse` returns `Err`); tolerant parses record diagnostics, apply a documented
recovery, and keep going — and every diagnostic carries an exact source span:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

// Strict: an unresolvable command aborts.
let strict: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let err = strict.parse(r"a \foo b").unwrap_err();
assert!(err.to_string().contains("cannot resolve command ‘\\foo’"));

// Tolerant: the command recovers as chars, the parse completes, the diagnostic is
// on the result.
let tolerant: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial().expect("seed state"),
);
let result = tolerant.parse(r"a \foo b").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, ["chars(a )", r"chars(\foo )", "chars(b)"]);
assert_eq!(result.diagnostics.len(), 1);
let diagnostic = result.diagnostics.iter().next().unwrap();
assert_eq!(diagnostic.span().content(), r"\foo ");
```

A stray `}` at top level follows the same pattern — strict aborts, tolerant diagnoses
it, stages the consumed delimiter as a chars node, and resumes:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let tolerant: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial().expect("seed state"),
);
let result = tolerant.parse("a}b").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, ["chars(a)", "chars(})", "chars(b)"]);
assert_eq!(result.diagnostics.len(), 1);
```

Match conditions by their *type*, never by spelling out an identifier string —
[the parsing chapter](crate::guide::parsing#working-with-diagnostics) shows the
idioms (`is::<T>()`, `downcast_ref::<T>()`, `T::IDENTIFIER`).

## Rendering diagnostics

Diagnostics render into readable reports with line/column positions, and the same
line/column machinery answers for any node
([`LineIndexCache`](crate::source::LineIndexCache) — parsing itself works purely
in byte offsets; line/column is a display-time computation). Iteration yields
diagnostics in the order the parse hit them;
[`sorted_by_position`](crate::error::Diagnostics::sorted_by_position) re-sorts
for reports that read along the document:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};
use techy::source::LineIndexCache;

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("first line\nsee \\foo here").unwrap();

// One readable report for the whole collection — message and position:
let report = result.diagnostics.render_all();
assert!(report.contains("cannot resolve command ‘\\foo’"));
assert!(report.contains("(line 2, col 5)"));

// Line/column for any node, through a consumer-held cache (each source is
// indexed once, ever):
let mut line_cols = LineIndexCache::new();
let node = result.tree.root().child(1).unwrap();
assert_eq!(
    line_cols.line_col(node.span().source(), node.span().start()),
    Some((2, 5)),
);
```

## Including other sources

techy performs no input/output itself: `\input`-style inclusion is the opt-in
[`input_macro_spec`](crate::latexlike::input_macro_spec) plus a
[`SourceResolver`](crate::source::SourceResolver) you supply on the driver (a
filesystem recipe is in
[the specs chapter](crate::guide::specs#resolving-external-sources-input-like-inclusion);
here, the in-memory [`MapResolver`](crate::source::MapResolver)). The resolved
content parses at the invocation point, into the same tree:

```rust
use techy::core::specs::Package;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{
    input_macro_spec, BodyMarker, CallableType, Latexlike, LatexlikeDriver,
};
use techy::source::MapResolver;

let mut resolver = MapResolver::new();
resolver.insert("preamble.tex", "hello {world}");

let mut package: Package<Latexlike> = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "input",
    input_macro_spec(false, BodyMarker::not_body()),
);
let language = Language::new(
    LatexlikeDriver::new(Recovery::Strict).with_source_resolver(resolver),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

let result = language.parse(r"a \input{preamble.tex} b").unwrap();
let input = result.tree.root().child(1).unwrap();
// The invocation's own span lives in the including document…
assert_eq!(input.span_content(), r"\input{preamble.tex}");
// …while the attached content was parsed out of the resolved source — a
// multi-source tree, retrieved by slot name:
assert_eq!(
    input.slot_content_nodes_named("attached").unwrap().source_text().unwrap(),
    "hello {world}",
);
```

## Extracting content

The [`extract`](crate::extract) helpers answer the everyday
"give me the *text*" questions. `content_as_chars` flattens chars and groups (and
fails honestly on anything that is not text); `split_at_chars` splits a node list at
a separator with grouped content protected; `parse_keyval` reads
`key=value,…` content. The `_drop_annotations` spellings used below are the
plain-output shorthands (the bare names take an annotation-minting callback — see
the [`extract`](crate::extract) module docs):

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec};
use techy::extract;
use techy::core::specs::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "usetikzlibrary",
    MacroSpec::new(argument_specs(["m"]).unwrap()),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

// A comma-separated list argument:
let result = language.parse(r"\usetikzlibrary{arrows,shapes.geometric,calc}").unwrap();
let node = result.tree.root().child(0).unwrap();
let list = node.argument_content_nodes(0).unwrap();

assert_eq!(extract::content_as_chars(list).unwrap(), "arrows,shapes.geometric,calc");

let split = extract::split_at_chars_drop_annotations(list, ",").unwrap();
// `source_text()` answers a segment's recorded coordinates — its content here, since
// the preset is span-tiled; `extract::content_as_chars(segment)` reads the content
// itself, whatever the segment was cut from.
let libraries: Vec<&str> =
    split.segments().map(|segment| segment.source_text().unwrap()).collect();
assert_eq!(libraries, ["arrows", "shapes.geometric", "calc"]);
```

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec};
use techy::extract;
use techy::core::specs::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "includegraphics",
    MacroSpec::new(argument_specs(["o", "m"]).unwrap()),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

// Keyval options, grouped values protected:
let result = language
    .parse(r"\includegraphics[width=5cm,label={fig,main}]{fig.pdf}")
    .unwrap();
let node = result.tree.root().child(0).unwrap();
let keyvals = extract::parse_keyval_drop_annotations(node.argument_content_nodes(0).unwrap()).unwrap();

assert_eq!(keyvals.len(), 2);
let width = keyvals.get("width").unwrap();
assert_eq!(width.value().unwrap().source_text(), Some("5cm"));
// The grouped value's *content* view sees inside the braces:
let label = keyvals.get("label").unwrap();
assert_eq!(label.value_content().unwrap().source_text(), Some("fig,main"));
```

## Transforming and recomposing

Trees are frozen; editing is a **restage** pass
([`transform::TreeRestager`](crate::transform::TreeRestager)) that stages a new tree while
walking the input, and
[`TreeRecomposer`](crate::recompose::TreeRecomposer) folds any tree into a value — with the
preset's [`source_recomposer`](crate::latexlike::source_recomposer) reemitting a
tree's exact source spelling from its recorded facts. Together they are the
edit-and-write-back pipeline:

```rust
use techy::core::node::NodeRef;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{source_recomposer, Latexlike, LatexlikeDriver};
use techy::recompose::TreeRecomposer;
use techy::transform::{Restage, RestageContext, TreeRestager};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let input = language.parse("one % secret\ntwo {three}").unwrap().tree;

// Reemission reads recorded facts only — byte-exact for trees parsed from a
// language that obeys span tiling, the preset included:
let full =
    TreeRecomposer::new(&mut source_recomposer()).recompose(&input, ()).unwrap();
assert_eq!(full, "one % secret\ntwo {three}");

// Drop every comment node; carry everything else over unchanged:
let cleaned = TreeRestager::new(
    &mut |node: NodeRef<'_, Latexlike>,
          _cx: &mut RestageContext<'_, Latexlike, (), ()>| {
        Ok::<_, core::convert::Infallible>(if node.is_comment() {
            Restage::Emit(Vec::new()) // emit nothing: the node is dropped
        } else {
            Restage::Descend(())
        })
    },
)
.restage(&input)
.unwrap();

let stripped =
    TreeRecomposer::new(&mut source_recomposer()).recompose(&cleaned, ()).unwrap();
assert_eq!(stripped, "one two {three}");
```

Both drivers have more to them — replacement staging, region edits, downward
state, custom piece types — covered in the
[`transform`](crate::transform) and [`recompose`](crate::recompose) module docs;
[`visit::TreeWalker`](crate::visit::TreeWalker) is their read-only sibling for
structure-aware analysis passes.

## Where to go from here

- [Node trees](crate::guide::node_trees) maps the whole tree-consumer toolkit;
  [Defining macros, environments, and specials](crate::guide::specs) is the full
  definitions chapter; [Running the parser](crate::guide::parsing) covers
  recovery, settings, and diagnostics in depth.
- The Developer Guide starts at
  [The parsing model](crate::guide::parsing_model) — the state/derivation
  machinery these examples lean on — and continues to custom construct parsers
  and custom languages.
- The repository's acceptance suite (`techy/tests/acceptance.rs`) is this page's
  bigger sibling: span-exact ports of pylatexenc's walker tests and
  error-recovery matrices in both recovery modes.
