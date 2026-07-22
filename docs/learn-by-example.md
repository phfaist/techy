# Learn techy by example

A tour of the `latexlike` preset — the familiar LaTeX behavior assembled from techy's
generic core — in small, complete, runnable examples. Every code block on this page is
compiled and executed as a doctest, so what you read here is what the library does.
The examples use `unwrap()` for brevity; real embedders will want to keep the
`Result`s (every fallible seam returns one — techy never panics on input).

The material follows the Phase 7.9 acceptance suite (`techy/tests/acceptance.rs`),
which ports a slice of pylatexenc's `latexwalker` test suite; where pylatexenc is
mentioned, that suite pins the parity claim.

## Your first parse

A [`Language`](crate::engine::Language) bundles everything that outlives one parse:
the seed parsing state, the parse driver, the source resolver. Define it once, parse
many documents. The [`Latexlike`](crate::latexlike::Latexlike) defaults give you the
canonical tokenization (`\` commands, `{…}` groups, `$…$` math, `%` comments) and the
`"base"` package (`\begin`/`\end` dispatch plus the standard specials):

```rust
use techy::engine::Language;
use techy::latexlike::Latexlike;

let language: Language<Latexlike> = Language::default();
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

No definitions are registered by default — the standard macro database is a later
phase — so `\emph` is an *unresolvable command* out of the box. Everything below
registers what it needs; that is the intended embedder workflow today.

## Reading nodes: kinds, spans, provenance

Every node knows exactly where it came from. Spans are byte ranges into the source,
and the original text is always reachable — level-1 recomposition needs no lookup
tables:

```rust
use techy::engine::Language;
use techy::latexlike::Latexlike;

let language: Language<Latexlike> = Language::default();
let result = language.parse("one {two} three").unwrap();
let group = result.tree.root().child(1).unwrap();

assert!(group.is_group());
assert_eq!(group.span().range(), 4..9);
assert_eq!(group.span_content(), "{two}");           // the node's exact source text
assert_eq!(group.group_delimiters(), Some(("{", "}")));
assert_eq!(group.child(0).unwrap().chars(), Some("two"));

// Sibling runs are `NodeSlice` views with *exact* covering spans (the tree's span
// partition invariant makes this precise, not approximate).
let children = result.tree.root().children();
assert_eq!(children.span().unwrap().range(), 0..15);
assert_eq!(children.source_text(), Some("one {two} three"));

// Document-order traversal of everything beneath a node:
let texts: Vec<&str> = result
    .tree
    .root()
    .descendants()
    .filter_map(|node| node.chars())
    .collect();
assert_eq!(texts, ["one ", "two", " three"]);
```

## Defining macros

Definitions live in [`SpecsProvider`](crate::scopes::SpecsProvider)s on the parsing
state's scope stack. The everyday provider is a [`Package`](crate::scopes::Package):
immutable, built once, loaded wholesale. Register a
[`MacroSpec`](crate::latexlike::MacroSpec) under
[`CallableType::Macro`](crate::latexlike::CallableType) and push the package onto a
language's seed with
[`Language::with_provider`](crate::engine::Language::with_provider):

```rust
use std::sync::Arc;
use techy::engine::Language;
use techy::latexlike::{argument_specs, CallableType, Latexlike, MacroSpec};
use techy::scopes::Package;

let mut package = Package::new("mydefs");
// Argument structures come from xparse-like codes: `o` = optional `[…]`,
// `m` = mandatory `{…}` (with the single-expression fallback). The list-and-join
// spelling anticipates the factory's move to a per-argument list signature.
package.insert(
    CallableType::Macro,
    "cite",
    Arc::new(MacroSpec::new(argument_specs(&["o", "m"].join(" ")).unwrap())),
);

let language = Language::<Latexlike>::default()
    .with_provider(Arc::new(package))
    .unwrap();

let result = language.parse(r"see \cite[Lemma 3]{Author}!").unwrap();
let cite = result.tree.root().child(1).unwrap();
assert_eq!(cite.macro_name(), Some("cite"));
assert_eq!(cite.span_content(), r"\cite[Lemma 3]{Author}");

// Arguments are self-describing records: which are provided, and where their
// content nodes live.
assert!(cite.arguments().unwrap().get(0).unwrap().is_provided());
let optional = cite.argument_content_nodes(0).unwrap();
assert_eq!(optional.source_text(), Some("Lemma 3"));
let mandatory = cite.argument_content_nodes(1).unwrap();
assert_eq!(mandatory.source_text(), Some("Author"));
```

Absent optional arguments are recorded, not invented — and a trailing optional at end
of input is no error (pylatexenc issue #57's regression test lives in the acceptance
suite):

```rust
use std::sync::Arc;
use techy::engine::Language;
use techy::latexlike::{argument_specs, CallableType, Latexlike, MacroSpec};
use techy::scopes::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "item",
    Arc::new(MacroSpec::new(argument_specs(&["o"].join(" ")).unwrap())),
);
let language = Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap();

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
display is a *delimiter* fact, read back by
[`math_style`](crate::node::NodeRef::math_style):

```rust
use techy::engine::Language;
use techy::latexlike::{Latexlike, MathStyle, Mode};

let language: Language<Latexlike> = Language::default();
let result = language.parse(r"a $x+y$ b \[z\]").unwrap();

let inline = result.tree.root().child(1).unwrap();
assert!(inline.is_math_group());
assert_eq!(inline.math_style(), Some(MathStyle::Inline));
// The interior was parsed in math mode; the node itself sits in the surrounding
// text-mode content. Every node records the state it was parsed under.
assert_eq!(inline.child(0).unwrap().parsing_state().mode(), Mode::Math);
assert_eq!(inline.parsing_state().mode(), Mode::Text);

let display = result.tree.root().child(3).unwrap();
assert_eq!(display.math_style(), Some(MathStyle::Display));
assert_eq!(display.group_delimiters(), Some((r"\[", r"\]")));
```

`$a$$b$` is two inline groups, not a display group — at a close position the expected
closer wins (pylatexenc's dollar-boundary parity):

```rust
use techy::engine::Language;
use techy::latexlike::Latexlike;

let language: Language<Latexlike> = Language::default();
let result = language.parse("$a$$b$").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, ["group(Math $ $)", "group(Math $ $)"]);
```

A macro like `\text{…}` must parse its argument back in *text* mode even inside
display math. That is per-argument data: an
[`ArgumentSpec`](crate::spec::ArgumentSpec) carries an optional parsing-state delta
(pylatexenc's `args_math_mode`), which here resets the mode and restores the math
delimiters as openers:

```rust
use std::sync::Arc;
use techy::constructs::GroupArgumentParser;
use techy::engine::Language;
use techy::latexlike::{
    default_token_rules, CallableType, GroupType, Latexlike, MacroSpec, Mode,
};
use techy::scopes::Package;
use techy::spec::ArgumentSpec;
use techy::state::{ParsingStateDelta, TokenRulesOverrides};

// A mandatory `{…}` argument whose interior parses in text mode, with `$…$` (etc.)
// re-enabled — inside math, the preset forbids nested math delimiters, and this
// delta statically undoes that for the argument's extent.
let text_mode_argument = Arc::new(
    ArgumentSpec::new(Arc::new(GroupArgumentParser::new(GroupType::Content)))
        .with_state_delta(
            ParsingStateDelta::new().mode(Mode::Text).rules(TokenRulesOverrides {
                groups: Some(default_token_rules().groups),
                forbidden_chars: Some("".into()),
                ..TokenRulesOverrides::default()
            }),
        ),
);

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "text",
    Arc::new(MacroSpec::new(vec![text_mode_argument])),
);
let language = Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap();

let result = language.parse(r"\[ x \text{if $y<0$} \]").unwrap();
let math = result.tree.root().child(0).unwrap();
let text = math.child(1).unwrap();
assert_eq!(text.macro_name(), Some("text"));

let argument = text.argument_content_nodes(0).unwrap();
assert_eq!(argument.get(0).unwrap().parsing_state().mode(), Mode::Text);
// …and the nested `$…$` inside the text-mode argument is math again:
let nested = argument.get(1).unwrap();
assert!(nested.is_math_group());
assert_eq!(nested.child(0).unwrap().parsing_state().mode(), Mode::Math);
```

## Environments

`\begin{name} … \end{name}` is a preset composition: `\begin` and `\end` are ordinary
macro entries of the base package whose parsers dispatch the environment's own spec —
an [`EnvironmentSpec`](crate::latexlike::EnvironmentSpec) registered under
[`CallableType::Environment`](crate::latexlike::CallableType). The parsed environment
is a callable node whose *body* is a slot:

```rust
use std::sync::Arc;
use techy::engine::Language;
use techy::latexlike::{argument_specs, CallableType, EnvironmentSpec, Latexlike};
use techy::scopes::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Environment,
    "enumerate",
    Arc::new(EnvironmentSpec::new(argument_specs(&["o"].join(" ")).unwrap())),
);
let language = Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap();

let result = language.parse(r"\begin{enumerate}[(i)] a b \end{enumerate}").unwrap();
let env = result.tree.root().child(0).unwrap();
assert_eq!(env.environment_name(), Some("enumerate"));
assert_eq!(env.argument_content_nodes(0).unwrap().source_text(), Some("(i)"));

let body: Vec<String> = env.body().unwrap().iter().map(|node| node.summary()).collect();
assert_eq!(body, ["chars( a b )"]);
```

An environment can install a parsing-state delta for its body's whole extent —
`equation` entering math mode is one line:

```rust
use std::sync::Arc;
use techy::engine::Language;
use techy::latexlike::{CallableType, EnvironmentSpec, Latexlike, Mode};
use techy::scopes::Package;
use techy::state::ParsingStateDelta;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Environment,
    "equation",
    Arc::new(
        EnvironmentSpec::new(Vec::new())
            .with_body_delta(ParsingStateDelta::new().mode(Mode::Math)),
    ),
);
let language = Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap();

let result = language.parse(r"\begin{equation}x+y\end{equation}").unwrap();
let env = result.tree.root().child(0).unwrap();
let body_chars = env.body().unwrap().get(0).unwrap();
assert_eq!(body_chars.chars(), Some("x+y"));
assert_eq!(body_chars.parsing_state().mode(), Mode::Math);
```

## Verbatim

Raw regions never tokenize their content. `\verb` is a macro whose argument is the
`v` code (auto-matched delimiter), and `verbatim` is an environment with the
[`VerbatimBehavior`](crate::latexlike::VerbatimBehavior) body — both produce
group+chars shapes with the raw text as ordinary chars content:

```rust
use std::sync::Arc;
use techy::engine::Language;
use techy::latexlike::{
    argument_specs, CallableType, EnvironmentSpec, Latexlike, MacroSpec, VerbatimBehavior,
};
use techy::scopes::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "verb",
    Arc::new(MacroSpec::new(argument_specs(&["v"].join(" ")).unwrap())),
);
package.insert(
    CallableType::Environment,
    "verbatim",
    Arc::new(EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default()))),
);
let language = Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap();

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

Specials are trigger character sequences resolved through the scope stack. The base
package ships pylatexenc's standard set (`~`, `&`, and the text-only typography
ligatures ``` `` ```, `''`, `--`, `---`); the scan takes the longest match, and
per-entry mode visibility keeps the ligatures out of math:

```rust
use techy::engine::Language;
use techy::latexlike::Latexlike;

let language: Language<Latexlike> = Language::default();
let result = language.parse("x---y--z").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, ["chars(x)", "Specials(---)", "chars(y)", "Specials(--)", "chars(z)"]);
```

Your own specials are package entries too — including ones that take arguments:

```rust
use std::sync::Arc;
use techy::engine::Language;
use techy::latexlike::{argument_specs, CallableType, Latexlike, SpecialsSpec};
use techy::scopes::Package;

let mut package = Package::new("mydefs");
package.insert_specials(
    "_",
    CallableType::Specials,
    Arc::new(SpecialsSpec::new(argument_specs(&["m"].join(" ")).unwrap())),
);
let language = Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap();

let result = language.parse("x_{down}").unwrap();
let sub = result.tree.root().child(1).unwrap();
assert_eq!(sub.specials_name(), Some("_"));
assert_eq!(sub.argument_content_nodes(0).unwrap().source_text(), Some("down"));
```

## Paragraph breaks

A whitespace run containing two or more newlines is a paragraph break. By default it
becomes a whitespace-only chars node; pylatexenc-modern's specials shape is one driver
flag away ([`ParagraphBreakStyle`](crate::latexlike::ParagraphBreakStyle) — a driver
emission policy, deliberately not package data: the tokenizer detects paragraph
breaks before the specials scan could ever run):

```rust
use techy::engine::Language;
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver, ParagraphBreakStyle};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict)
        .with_paragraph_break_style(ParagraphBreakStyle::Specials),
);
let result = language.parse("one\n\ntwo").unwrap();

let break_node = result.tree.root().child(1).unwrap();
// The node is named by the canonical `"\n\n"` vocabulary key; its span covers the
// actual whitespace run.
assert_eq!(break_node.specials_name(), Some("\n\n"));
assert_eq!(break_node.span().range(), 3..5);
```

## Strict vs. tolerant parsing

The recovery policy lives on the driver. Strict parses abort on the first error
(`parse` returns `Err`); tolerant parses record diagnostics, apply a documented
recovery, and keep going — and every diagnostic carries an exact source span:

```rust
use techy::engine::Language;
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

// Strict (the default): an unresolvable command aborts.
let strict: Language<Latexlike> = Language::default();
let err = strict.parse(r"a \foo b").unwrap_err();
assert!(err.to_string().contains("cannot resolve command ‘\\foo’"));

// Tolerant: the command recovers as chars, the parse completes, the diagnostic is
// on the result.
let tolerant: Language<Latexlike> = Language::new(LatexlikeDriver::new(Recovery::Tolerant));
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
use techy::engine::Language;
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let tolerant: Language<Latexlike> = Language::new(LatexlikeDriver::new(Recovery::Tolerant));
let result = tolerant.parse("a}b").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, ["chars(a)", "chars(})", "chars(b)"]);
assert_eq!(result.diagnostics.len(), 1);
```

## Extracting content

The [`node::extract`](crate::node::extract) helpers answer the everyday
"give me the *text*" questions. `content_as_chars` flattens chars and groups (and
fails honestly on anything that is not text); `split_at_chars` splits a node list at
a separator with grouped content protected; `parse_keyval` reads
`key=value,…` content:

```rust
use std::sync::Arc;
use techy::engine::Language;
use techy::latexlike::{argument_specs, CallableType, Latexlike, MacroSpec};
use techy::node::extract;
use techy::scopes::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "usetikzlibrary",
    Arc::new(MacroSpec::new(argument_specs(&["m"].join(" ")).unwrap())),
);
let language = Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap();

// A comma-separated list argument:
let result = language.parse(r"\usetikzlibrary{arrows,shapes.geometric,calc}").unwrap();
let node = result.tree.root().child(0).unwrap();
let list = node.argument_content_nodes(0).unwrap();

assert_eq!(extract::content_as_chars(list).unwrap(), "arrows,shapes.geometric,calc");

let split = extract::split_at_chars(list, ",").unwrap();
let libraries: Vec<&str> =
    split.segments().map(|segment| segment.source_text().unwrap()).collect();
assert_eq!(libraries, ["arrows", "shapes.geometric", "calc"]);
```

```rust
use std::sync::Arc;
use techy::engine::Language;
use techy::latexlike::{argument_specs, CallableType, Latexlike, MacroSpec};
use techy::node::extract;
use techy::scopes::Package;

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "includegraphics",
    Arc::new(MacroSpec::new(argument_specs(&["o", "m"].join(" ")).unwrap())),
);
let language = Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap();

// Keyval options, grouped values protected:
let result = language
    .parse(r"\includegraphics[width=5cm,label={fig,main}]{fig.pdf}")
    .unwrap();
let node = result.tree.root().child(0).unwrap();
let keyvals = extract::parse_keyval(node.argument_content_nodes(0).unwrap()).unwrap();

assert_eq!(keyvals.len(), 2);
let width = keyvals.get("width").unwrap();
assert_eq!(width.value().unwrap().source_text(), Some("5cm"));
// The grouped value's *content* view sees inside the braces:
let label = keyvals.get("label").unwrap();
assert_eq!(label.value_content().unwrap().source_text(), Some("fig,main"));
```

## Where to go from here

- The [parsing model](crate::guide::parsing_model) page (once written) covers the
  state/derivation machinery these examples lean on: parsing states as immutable
  values, deltas, the scope stack, and the driver.
- The acceptance suite (`techy/tests/acceptance.rs`) is this page's bigger sibling:
  span-exact ports of pylatexenc's walker tests, error-recovery matrices in both
  recovery modes, and the shared test-side spec database pattern.
- `ARCHITECTURE.md` and `DESIGN_RATIONALE.md` in the repository record why the
  library is shaped this way.
