# Defining macros, environments, and specials

techy ships almost no definitions of its own (see
[Language syntax](crate::guide::language_syntax#no-definitions-ship-by-default)):
the macros, environments, and specials your documents use are registered by
you. This chapter shows how, for latexlike languages: a definition is a
**spec** — a value describing a callable's arguments and behavior — registered
under a name in a **package**, and packages are loaded into the parsing state
the parser starts from.

## Packages and registration

A [`Package`](crate::core::specs::Package) is an immutable collection of
definitions: build it once with inserts, then load it wholesale. The quickest
registrations are the one-liners
[`define_macro`](crate::core::specs::Package::define_macro) and
[`define_environment`](crate::core::specs::Package::define_environment),
which take the name and the argument codes (next section). The general form
is [`insert`](crate::core::specs::Package::insert), which takes the callable
type ([`CallableType`](crate::latexlike::CallableType): macro, environment,
or specials), the name, and a spec value you build yourself — that is where
non-default argument configurations and behaviors come in. Loading happens
when the language is built:
[`ParsingState::lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages)
seeds the initial parsing state with your packages.

```rust
use techy::core::{Language, ParsingState};
use techy::core::specs::Package;
use techy::error::Recovery;
use techy::latexlike::{
    argument_specs_named, CallableType, Latexlike, LatexlikeDriver, MacroSpec,
};

let mut package: Package<Latexlike> = Package::new("mydefs");

// One-liners: callable type and spec type paired correctly by construction.
package.define_macro("emph", ["m"]).unwrap();
package.define_environment("quotation", ["o"]).unwrap();

// The same registration spelled out — here with *named* arguments:
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

Definitions resolve through a stack of providers searched innermost-first
([scopes and packages](crate::guide::concepts_overview#scopes-and-packages)):
loading a package that redefines a name shadows the definition below it —
that is the entire override model. A package, or any single entry in it, can
also be restricted to certain parsing modes (math-only scripts, text-only
ligatures): see
[`set_visible_modes`](crate::core::specs::Package::set_visible_modes) and
[`insert_in_modes`](crate::core::specs::Package::insert_in_modes).

## The spec types

The preset ships three declarative spec types, one per callable type:

- [`MacroSpec`](crate::latexlike::MacroSpec) — a macro's argument list as
  plain data.
- [`EnvironmentSpec`](crate::latexlike::EnvironmentSpec) — an environment's
  arguments (parsed after `\begin{name}`) plus its **body behavior**: how the
  region up to `\end{name}` is handled.
- [`SpecialsSpec`](crate::latexlike::SpecialsSpec) — a specials callable's
  argument list; the trigger sequence is the registration key, passed to
  [`insert_specials`](crate::core::specs::Package::insert_specials), not
  stored in the spec.

```rust
use techy::core::{Language, ParsingState};
use techy::core::specs::Package;
use techy::error::Recovery;
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, SpecialsSpec};

let mut package = Package::new("mydefs");
package.insert_specials(
    CallableType::Specials,
    "_",
    SpecialsSpec::new(argument_specs(["m"]).unwrap()),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]),
);

let result = language.parse("x_{down}").unwrap();
let sub = result.tree.root().child(1).unwrap();
assert_eq!(sub.specials_name(), Some("_"));
assert_eq!(sub.argument_content_nodes(0).unwrap().source_text(), Some("down"));
```

An environment's definition can change how its whole body parses.
[`with_body_delta`](crate::latexlike::EnvironmentSpec::with_body_delta)
installs a [parsing-state
delta](crate::guide::concepts_overview#parsing-state-and-deltas) for the
body's extent — an `equation`-like environment entering math mode is one
line:

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

For bodies that must not be tokenized at all — `verbatim`-style environments
— register an
[`EnvironmentSpec::from_behavior`](crate::latexlike::EnvironmentSpec::from_behavior)
with the shipped [`VerbatimBehavior`](crate::latexlike::VerbatimBehavior),
which reads the body as raw text up to the literal `\end{name}`; custom body
handling in general goes through the
[`EnvironmentBehavior`](crate::latexlike::EnvironmentBehavior) trait.

These types are conveniences, not requirements: any
[`CallableSpec`](crate::core::specs::CallableSpec) implementation can be
registered, including specs that take over parsing of their invocation
entirely (`\verb`-like constructs) — see
[Custom construct parsers](crate::guide::construct_parsers).

## Argument codes

Argument structures are declared with short codes, one code per argument, in
the style of LaTeX's `xparse` package:
[`argument_specs(["o", "m"])`](crate::latexlike::argument_specs) declares an
optional `[…]` argument followed by a mandatory one. The most common codes:
`m` (mandatory `{…}` group), `o` (optional `[…]` group), `s` (optional `*`
marker), `v` (delimited verbatim, `\verb`-style). The full code table — the
`t`/`r`/`d`/`e` forms and the word codes — is on
[`argument_specs`](crate::latexlike::argument_specs); the compact whole-spec
string form (`"om"`, as used by pylatexenc's spec database) is
[`argument_specs_from_str`](crate::latexlike::argument_specs_from_str). A
malformed code is reported immediately at construction time, not at parse
time.

**Names**: [`argument_specs_named`](crate::latexlike::argument_specs_named)
takes `(code, name)` pairs, and named arguments unlock the by-name access
family
([`argument_content_nodes_named`](crate::core::node::NodeRef::argument_content_nodes_named)
and siblings) — the robust access path, whose error contract distinguishes a
misspelled name from a merely absent argument.

**The single-expression fallback — a documented trap.** The `m` code keeps
TeX's fallback: if no `{…}` group follows, a single expression is taken
instead (`\frac12` reads two one-character arguments). That also means a
*missing* group is not diagnosed — the argument silently consumes whatever
sibling content follows. Where arguments are machine-written or config-like
and the fallback is unwanted, use the word code `BracedOnly`: the same
mandatory group with the fallback off (see the code table on
[`argument_specs`](crate::latexlike::argument_specs)).

**The typed alternative.** The code factory is convenience, never a
requirement: each code resolves to a configured argument parser
([`GroupArgumentParser`](crate::core::constructs::GroupArgumentParser),
[`OptionalGroupArgumentParser`](crate::core::constructs::OptionalGroupArgumentParser),
…), and you can build
[`ArgumentSpec`](crate::core::specs::ArgumentSpec)s from those parser types
directly — that is also how per-argument parsing-state deltas are attached
(a `\text`-style argument that leaves math mode, for instance; see the
[`ArgumentSpec`](crate::core::specs::ArgumentSpec) documentation).

## Scoped and body-scoped definitions

A definition need not be document-global. Because definitions live on the
scope stack of the changeable parsing state, an environment can carry
definitions that exist *only inside its body*: give the environment a body
delta that pushes a package, and the definitions revert when the body ends.
The shipped exemplar is [`minidefs`](crate::latexlike::minidefs)' `\item`,
defined only inside `itemize`/`enumerate` bodies. The same technique with
your own vocabulary:

```rust
use std::sync::Arc;
use techy::core::specs::{Package, ScopeOp, SpecsProvider};
use techy::core::{Language, ParsingState, ParsingStateDelta};
use techy::error::Recovery;
use techy::latexlike::{CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver};

// `\note` lives in its own package…
let mut note_defs: Package<Latexlike> = Package::new("mydefs.note");
note_defs.define_macro("note", ["m"]).unwrap();
let note_defs: Arc<dyn SpecsProvider<Latexlike>> = Arc::new(note_defs);

// …which the `notes` environment's body pushes onto the scope stack.
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

// Inside the body, `\note` resolves; past `\end{notes}`, it is gone again.
assert!(language.parse(r"\begin{notes}\note{a}\end{notes}").is_ok());
assert!(language.parse(r"\note{a}").is_err());
```

Definitions can also be added and removed mid-parse at the sibling level
(`\newcommand`-style constructs): the mutable-by-replacement provider is
[`Scope`](crate::core::specs::Scope), addressed by
[`ScopeOp`](crate::core::specs::ScopeOp) /
[`DefinitionOp`](crate::core::specs::DefinitionOp) operations carried on
parsing-state deltas.

## Registration pitfalls

Two silent traps and one non-check, all documented on the API items:

- **Never include the escape character in a registered name.** Register
  `"emph"`, not `"\\emph"`: command tokens carry their name *without* the
  escape character, so an escape-prefixed registration can never match — the
  definition is silently unreachable. techy defends where the mistake happens: an
  unresolved command's diagnostic suggests escape-prefixed near-misses, and a
  parse-initialization check warns when all of a provider's commands are
  escape-shadowed. See [`Package::insert`](crate::core::specs::Package::insert).
- **The `m` code's single-expression fallback** can silently consume sibling
  content when a group was intended — see
  [the argument-codes section](#argument-codes) above.
- **Registration performs no spec-type/callable-type cross-check —
  deliberately.** A plain macro-shaped spec registered under the environment
  type is legitimate: its declared arguments parse after `\begin{name}` and
  the body takes the default handling. The one-liners
  ([`define_macro`](crate::core::specs::Package::define_macro) /
  [`define_environment`](crate::core::specs::Package::define_environment))
  make the correct pairing structural in the common case. See
  [`Package::insert`](crate::core::specs::Package::insert).

## Resolving external sources: `\input`-like inclusion

techy performs no input/output of its own; reading files is the calling
application's capability, plugged in through the
[`SourceResolver`](crate::source::SourceResolver) trait. A resolver turns a
reference string (`chapter-one.tex`) into content; it is configured on the
driver
([`with_source_resolver`](crate::latexlike::LatexlikeDriver::with_source_resolver)),
and without one, nothing is ever resolved. The core never interprets
reference strings — path semantics, and recursion policy, belong to the
resolver ([`check_include_chain`](crate::source::check_include_chain) is the
ready-made cycle-and-depth check). For tests and preloaded setups there is
the in-memory [`MapResolver`](crate::source::MapResolver).

The standard filesystem recipe — resolving references against a base
directory on disk:

```rust,no_run
use std::path::PathBuf;
use techy::core::specs::Package;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{
    input_macro_spec, BodyMarker, CallableType, Latexlike, LatexlikeDriver,
};
use techy::source::{
    check_include_chain, ResolveError, ResolvedContent, SourceResolver, SourceSpan,
};

/// Resolves references against a base directory on disk.
struct DirectoryResolver {
    base: PathBuf,
}

impl SourceResolver for DirectoryResolver {
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan,
    ) -> Result<ResolvedContent, ResolveError> {
        let path = self.base.join(reference);
        // The canonical path doubles as the include-chain key and the origin label.
        let canonical = path
            .canonicalize()
            .map_err(|error| ResolveError::new(reference, error.to_string()))?;
        // Refuse include cycles and runaway nesting (recursion policy is ours).
        check_include_chain(
            &canonical,
            triggered_at,
            |origin: &Option<String>| origin.as_ref().map(PathBuf::from),
            Some(20),
        )?;
        let content = std::fs::read_to_string(&canonical)
            .map_err(|error| ResolveError::new(reference, error.to_string()))?;
        Ok(ResolvedContent::new(content)
            .with_origin(Some(canonical.to_string_lossy().into_owned())))
    }
}

let mut package: Package<Latexlike> = Package::new("mydefs");
package.insert(
    CallableType::Macro,
    "input",
    input_macro_spec(false, BodyMarker::not_body()),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict)
        .with_source_resolver(DirectoryResolver { base: PathBuf::from("book-src") }),
    ParsingState::lang_initial_with_packages([package]),
);
let result = language.parse(r"\input{chapter-one.tex}").unwrap();
```

The `\input` definition itself is the opt-in
[`input_macro_spec`](crate::latexlike::input_macro_spec)
([`InputMacroSpec`](crate::latexlike::InputMacroSpec)): the referenced
content is parsed at the invocation point into the same tree, and the two
mandatory constructor choices — whether state changes made inside the
included file persist past the `\input`, and how the attached content is
marked — are documented on the item.

## Beyond latexlike

Everything above is the latexlike path: preset spec types registered under
the preset's three callable types. For a language with its own callable
vocabulary, the same machinery is used directly — the general contracts are
[`CallableSpec`](crate::core::specs::CallableSpec),
[`ArgumentSpec`](crate::core::specs::ArgumentSpec),
[`SpecsProvider`](crate::core::specs::SpecsProvider), and the
[`core::specs`](crate::core::specs) module is the entry point; the Developer
Guide chapters [The parsing model](crate::guide::parsing_model) and
[Defining a custom language](crate::guide::custom_lang) cover that route.

Read next: [Running the parser](crate::guide::parsing) — recovery policies,
parser settings, and working with diagnostics.
