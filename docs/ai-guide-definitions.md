# AI guide: definitions

Condensed reference: defining macros, environments, and specials for
latexlike languages. Compressed from
[Defining macros, environments, and specials](crate::guide::specs) (the full
chapter). Terms: a **callable** is anything invoked by name in the source
(macros, environments, specials in LaTeX vocabulary); a **spec** is a value
describing a callable's arguments and behavior; a **package** is an immutable
collection of definitions; the **parsing state** is the immutable snapshot of
everything that can vary during a parse (definitions, token rules, mode), and
a **delta** ([`ParsingStateDelta`](crate::core::ParsingStateDelta)) is a
plain value describing a state change.

techy ships almost no definitions: only `\begin`/`\end`
([`builtin_package`](crate::latexlike::builtin_package)). Everything else —
`\emph` included — is unresolvable until registered. Opt-in toy package for
prototyping only:
[`minidefs::minilatex_package()`](crate::latexlike::minidefs::minilatex_package).

## Registration

Build a [`Package`](crate::core::specs::Package), register specs, seed the
initial parsing state with it. Two registration forms:

| Form | Use |
|---|---|
| [`define_macro(name, codes)`](crate::core::specs::Package::define_macro) / [`define_environment(name, codes)`](crate::core::specs::Package::define_environment) | one-liners; pair callable type and spec type correctly by construction |
| [`insert(callable_type, name, spec)`](crate::core::specs::Package::insert) / [`insert_specials(callable_type, trigger, spec)`](crate::core::specs::Package::insert_specials) | general form; needed for non-default argument configurations, behaviors, custom specs |

```rust
use techy::core::{Language, ParsingState};
use techy::core::specs::Package;
use techy::error::Recovery;
use techy::latexlike::{
    argument_specs_named, CallableType, Latexlike, LatexlikeDriver, MacroSpec,
};

let mut package: Package<Latexlike> = Package::new("mydefs");
package.define_macro("emph", ["m"]).unwrap();
package.define_environment("quotation", ["o"]).unwrap();
// General form, with named arguments (unlocks by-name access):
package.insert(
    CallableType::Macro,
    "cite",
    MacroSpec::new(argument_specs_named([("o", "detail"), ("m", "keys")]).unwrap()),
);

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]),
);
let result = language.parse(r"\cite[p. 7]{knuth}").unwrap();
let cite = result.tree.root().child(0).unwrap();
assert_eq!(
    cite.argument_content_nodes_named("keys").unwrap().unwrap().source_text(),
    Some("knuth"),
);
```

Resolution model: definitions live on a stack of providers searched
innermost-first
([scopes and packages](crate::guide::concepts_overview#scopes-and-packages)).
Loading a package that redefines a name shadows the definition below it —
that is the entire override model; there is no separate mechanism.

## Argument codes

One code string per argument, `xparse`-style, resolved eagerly by
[`argument_specs`](crate::latexlike::argument_specs) (list form,
`["o", "m"]`) or
[`argument_specs_from_str`](crate::latexlike::argument_specs_from_str)
(compact whole-spec string, `"om"` — the form pylatexenc's spec database
uses). A malformed code is an `Err` at construction time, not at parse time.
Full table (from the [`argument_specs`](crate::latexlike::argument_specs)
documentation):

| Code | Argument |
|---|---|
| `m` or `{` | mandatory `{…}` group, **with** the single-expression fallback (`\frac12` reads two one-character arguments) |
| `o` or `[` | optional `[…]` group |
| `s` or `*` | optional `*` marker |
| `t<c>` | optional single-character marker `<c>` |
| `r<c1><c2>` | required group delimited `<c1>`…`<c2>` (no expression fallback) |
| `d<c1><c2>` | optional group delimited `<c1>`…`<c2>` |
| `v` / `v<c1><c2>` | delimited verbatim (`\verb`-style); auto-matched or prescribed delimiters |
| `e{<chars>}` | embellishments: one marker per character, each followed by an expression, any order, each at most once |
| `AnyDelimited` / `AnyDelimitedOptional` | group delimited by any of `{}` `[]` `()` `<>` (list-form-only word codes) |
| `BracedOnly` | mandatory content-class group with the expression fallback **off** (list-form-only word code) |

**Names**: [`argument_specs_named`](crate::latexlike::argument_specs_named)
takes `(code, name)` pairs; named arguments unlock
[`argument_content_nodes_named`](crate::core::node::NodeRef::argument_content_nodes_named)
and siblings — the robust access path (its error contract distinguishes a
misspelled name from an absent argument).

**Typed alternative**: each code resolves to a configured argument-parser
value ([`GroupArgumentParser`](crate::core::constructs::GroupArgumentParser),
[`OptionalGroupArgumentParser`](crate::core::constructs::OptionalGroupArgumentParser),
…) which you can build directly into
[`ArgumentSpec`](crate::core::specs::ArgumentSpec)s — that is also how
per-argument parsing-state deltas are attached (a `\text`-style argument
that restores the enclosing non-math context: recipe on
[`Event::ExitMathContext`](crate::latexlike::Event)).

## Spec types

| Type | Registered under | Notes |
|---|---|---|
| [`MacroSpec`](crate::latexlike::MacroSpec) | `CallableType::Macro` | argument list as plain data |
| [`EnvironmentSpec`](crate::latexlike::EnvironmentSpec) | `CallableType::Environment` | arguments parsed after `\begin{name}`, plus body behavior |
| [`SpecialsSpec`](crate::latexlike::SpecialsSpec) | `CallableType::Specials`, via [`insert_specials`](crate::core::specs::Package::insert_specials) | trigger sequence is the registration key, not stored in the spec; longest trigger match wins |

These are conveniences: any
[`CallableSpec`](crate::core::specs::CallableSpec) implementation can be
registered, including specs that take over parsing of their invocation
entirely (`\verb`-like) — see
[Custom construct parsers](crate::guide::construct_parsers).

## Environment body behavior

An environment's definition can change how its whole body parses.

**Mode change** —
[`with_body_delta`](crate::latexlike::EnvironmentSpec::with_body_delta)
installs a delta for the body's extent (`equation`-like → math mode):

```rust
use techy::core::{Language, ParsingState, ParsingStateDelta};
use techy::core::specs::Package;
use techy::error::Recovery;
use techy::latexlike::{CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, Mode};

let mut package = Package::new("mydefs");
package.insert(
    CallableType::Environment,
    "equation",
    EnvironmentSpec::new(Vec::new())
        .with_body_delta(ParsingStateDelta::new().mode(Mode::Math)),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]),
);
let result = language.parse(r"\begin{equation}x+y\end{equation}").unwrap();
let body = result.tree.root().child(0).unwrap().body().unwrap();
assert_eq!(body.get(0).unwrap().parsing_state().mode(), Mode::Math);
```

**Verbatim body** — for bodies that must not be tokenized at all, register
[`EnvironmentSpec::from_behavior`](crate::latexlike::EnvironmentSpec::from_behavior)
with [`VerbatimBehavior`](crate::latexlike::VerbatimBehavior) (reads raw
text up to the literal `\end{name}`); general custom body handling is the
[`EnvironmentBehavior`](crate::latexlike::EnvironmentBehavior) trait. The
`\verb` macro shape is not a behavior but an argument code: `v`.

## Scoped and body-scoped definitions

Definitions need not be document-global. An environment body can push its
own package — the definitions revert structurally when the body ends
(shipped exemplar: [`minidefs`](crate::latexlike::minidefs)' `\item`,
defined only inside `itemize`/`enumerate` bodies):

```rust
use std::sync::Arc;
use techy::core::specs::{Package, ScopeOp, SpecsProvider};
use techy::core::{Language, ParsingState, ParsingStateDelta};
use techy::error::Recovery;
use techy::latexlike::{CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver};

let mut note_defs: Package<Latexlike> = Package::new("mydefs.note");
note_defs.define_macro("note", ["m"]).unwrap();
let note_defs: Arc<dyn SpecsProvider<Latexlike>> = Arc::new(note_defs);

let mut package: Package<Latexlike> = Package::new("mydefs");
package.insert(
    CallableType::Environment,
    "notes",
    EnvironmentSpec::new(Vec::new()).with_body_delta(
        ParsingStateDelta::new().scope_op(ScopeOp::Push(note_defs)),
    ),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]),
);
assert!(language.parse(r"\begin{notes}\note{a}\end{notes}").is_ok());
assert!(language.parse(r"\note{a}").is_err()); // gone outside the body
```

Mid-parse changes at the sibling level (`\newcommand`-style constructs):
the mutable-by-replacement provider is
[`Scope`](crate::core::specs::Scope), addressed by
[`ScopeOp`](crate::core::specs::ScopeOp) /
[`DefinitionOp`](crate::core::specs::DefinitionOp) operations carried on
parsing-state deltas returned by construct parsers.

**Mode-restricted visibility**: a whole package or a single entry can be
visible only in certain parsing modes (math-only scripts, text-only
ligatures) —
[`set_visible_modes`](crate::core::specs::Package::set_visible_modes),
[`insert_in_modes`](crate::core::specs::Package::insert_in_modes).

## Traps

All documented on the API items; all silent:

| Trap | Fact | Defense |
|---|---|---|
| Escape character in a registered name | Command tokens carry the name *without* the escape character; registering `"\\emph"` instead of `"emph"` can never match — the definition is silently unreachable | The unresolved-command diagnostic suggests escape-prefixed near-misses; a parse-initialization check warns when *all* of a provider's commands are escape-shadowed. See [`Package::insert`](crate::core::specs::Package::insert) |
| `m` single-expression fallback | A missing `{…}` group is not diagnosed; the argument silently swallows whatever sibling expression follows | Use the word code `BracedOnly` where arguments are machine-written or config-like |
| No spec-type/callable-type cross-check | [`insert`](crate::core::specs::Package::insert) accepts a macro-shaped spec under the environment type — deliberately legitimate (its arguments parse after `\begin{name}`, body takes default handling), so no error flags a *mistaken* pairing | Use the one-liners ([`define_macro`](crate::core::specs::Package::define_macro) / [`define_environment`](crate::core::specs::Package::define_environment)); they make the pairing structural |

## `\input`-like inclusion

techy performs no input/output. Wiring has two opt-in halves — without
either, nothing resolves:

1. **The definition**:
   [`input_macro_spec`](crate::latexlike::input_macro_spec)
   ([`InputMacroSpec`](crate::latexlike::InputMacroSpec)) — never preloaded;
   its two mandatory constructor choices (whether state changes inside the
   included file persist past the `\input`; how the attached content is
   marked) are documented on the item.
2. **The resolver**: a [`SourceResolver`](crate::source::SourceResolver)
   configured on the driver
   ([`with_source_resolver`](crate::latexlike::LatexlikeDriver::with_source_resolver)).
   The core never interprets reference strings — path semantics and
   recursion policy belong to the resolver
   ([`check_include_chain`](crate::source::check_include_chain) is the
   ready-made cycle-and-depth check). In-memory resolver for tests:
   [`MapResolver`](crate::source::MapResolver). The standard filesystem
   recipe (doc-tested `DirectoryResolver`) is in
   [the specs chapter](crate::guide::specs#resolving-external-sources-input-like-inclusion).

The resolved content parses at the invocation point into the same tree
(retrieved via the callable's attached slot,
`slot_content_nodes_named("attached")`):

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
    ParsingState::lang_initial_with_packages([package]),
);
let result = language.parse(r"a \input{preamble.tex} b").unwrap();
let input = result.tree.root().child(1).unwrap();
assert_eq!(input.span_content(), r"\input{preamble.tex}");
assert_eq!(
    input.slot_content_nodes_named("attached").unwrap().source_text().unwrap(),
    "hello {world}",
);
```

## Beyond latexlike

For a language with its own callable vocabulary, use the general contracts
directly: [`CallableSpec`](crate::core::specs::CallableSpec),
[`ArgumentSpec`](crate::core::specs::ArgumentSpec),
[`SpecsProvider`](crate::core::specs::SpecsProvider) — entry point
[`core::specs`](crate::core::specs); see
[AI guide: custom languages](crate::guide::ai_guide_custom_lang).
