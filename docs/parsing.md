# Running the parser

This chapter covers the run itself: constructing a
[`Language`](crate::core::Language), choosing between strict and tolerant
error recovery, the settings you can adjust, and working with the diagnostics
a parse reports.

## The `Language` bundle and the parse entry points

A [`Language`](crate::core::Language) bundles everything that outlives one
parse: the **driver** (parse-time behavior — the recovery policy and, when
configured, the source resolver) and the frozen **initial parsing state**
(token rules, mode, and the definitions loaded at the start). Define it once,
parse many documents:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("Hello {world}!").unwrap();
assert_eq!(result.tree.root().child_count(), 3);
assert!(result.diagnostics.is_empty());
```

Two entry points:

- [`parse(content)`](crate::core::Language::parse) — parse a string as an
  anonymous in-memory source; the everyday call.
- [`parse_source(source)`](crate::core::Language::parse_source) — parse a
  pre-minted [`Source`](crate::source::Source), when you want the source to
  carry an origin label (a file name for diagnostics) or provenance; build it
  with [`Source::new`](crate::source::Source::new) and
  [`with_origin`](crate::source::Source::with_origin) and share it as
  `Arc<Source>`.

Both return `Result`: `Ok` is a [`ParseResult`](crate::core::ParseResult) —
the parsed [`tree`](crate::core::ParseResult) plus the
[`diagnostics`](crate::core::ParseResult) recorded along the way — and `Err`
is a [`ParseError`](crate::error::ParseError), meaning the parse aborted. A
`ParseResult` holds no reference to the `Language`; results outlive their
bundle.

## Strict versus tolerant

The [`Recovery`](crate::error::Recovery) policy, set on the driver, decides
what happens at the first problem:

- **`Recovery::Strict`** — the parse aborts: `parse` returns `Err` with a
  [`ParseError`](crate::error::ParseError) describing the first problem
  encountered.
- **`Recovery::Tolerant`** — a diagnostic is recorded, a documented recovery
  is applied where the problem was detected, and parsing continues. `parse`
  returns `Ok`; the result carries a best-effort tree *and* the diagnostics.

Tolerant output means exactly that: the tree covers the whole input, with
each problem site repaired by that condition's documented recovery — an
unresolvable command is staged as its literal characters, a stray `}` at the
top level is consumed as a character node, and so on (the recovery is part of
each condition's documentation). The tree remains well-formed and
accounts for every input byte; check
[`diagnostics.has_errors()`](crate::error::Diagnostics::has_errors) before
treating it as clean.

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

// Strict: an unresolvable command aborts the parse.
let strict: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
assert!(strict.parse(r"a \foo b").is_err());

// Tolerant: the parse completes; the command recovered as literal characters.
let tolerant: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial().expect("seed state"),
);
let result = tolerant.parse(r"a \foo b").unwrap();
assert_eq!(result.tree.root().child_count(), 3);
assert_eq!(result.diagnostics.len(), 1);
assert!(result.diagnostics.has_errors());
```

## The settings

**On the driver** ([`LatexlikeDriver`](crate::latexlike::LatexlikeDriver)):
the [`Recovery`](crate::error::Recovery) policy (constructor argument); the
source resolver for `\input`-like lookups
([`with_source_resolver`](crate::latexlike::LatexlikeDriver::with_source_resolver)
— see [the specs chapter](crate::guide::specs#resolving-external-sources-input-like-inclusion));
and the paragraph-break representation
([`with_paragraph_break_style`](crate::latexlike::LatexlikeDriver::with_paragraph_break_style)).

**In the initial parsing state**: which definitions are loaded
([`lang_initial`](crate::core::ParsingState::lang_initial) for the bare seed,
[`lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages)
to add your packages — see
[Defining macros, environments, and specials](crate::guide::specs)). Anything
beyond that — changed token rules, a different starting mode — is expressed
by deriving a customized seed before constructing the `Language`:
[`ParsingState::derived`](crate::core::ParsingState::derived) applies a
[`ParsingStateDelta`](crate::core::ParsingStateDelta) (with, for example,
[`TokenRulesOverrides`](crate::core::token::TokenRulesOverrides)) to the seed state.
There is deliberately no parallel settings object: everything the parse can
vary mid-run lives in the [parsing
state](crate::guide::concepts_overview#parsing-state-and-deltas), and the
initial state is just the first one.

## Working with diagnostics

Each [`Diagnostic`](crate::error::Diagnostic) carries a severity
([`Severity`](crate::error::Severity)), an exact source span, a traceback of
parse frames, and — centrally — a structured **condition payload**: a typed
value describing what happened, from which the human-readable message is
derived. [`ParseError`](crate::error::ParseError), the strict-mode abort,
carries the same information.

**Rendering.** [`Diagnostic::render`](crate::error::Diagnostic::render)
produces a human-readable multi-line report (message, line/column position,
traceback, include chain);
[`Diagnostics::render_all`](crate::error::Diagnostics::render_all) renders a
whole collection efficiently. For repeated rendering against the same
sources, the `_with` variants
([`render_all_with`](crate::error::Diagnostics::render_all_with)) accept a
persistent [`LineIndexCache`](crate::source::LineIndexCache), so line tables
are computed once.

**Order.** Iteration yields diagnostics in *recovery order* — the order the
parse hit them, which nested descents can permute relative to the document.
For reports that read along the source, use
[`sorted_by_position`](crate::error::Diagnostics::sorted_by_position).
Retention is bounded (a cap against degenerate tolerant-mode input):
[`suppressed()`](crate::error::Diagnostics::suppressed) counts anything
dropped beyond it.

**Matching conditions.** Every condition is a concrete public type (for
example
[`UnresolvableCommand`](crate::core::constructs::UnresolvableCommand)), and
each type carries a semver-stable identifier string for boundaries where
types cannot travel (logs, wire formats). The rule: **match conditions via
the type — `is::<T>()`, `downcast_ref::<T>()`,
[`conditions::<T>()`](crate::error::Diagnostics::conditions) — or via
`T::IDENTIFIER`; never spell an identifier as a string literal.** The full
roster of shipped condition types is the implementors listing on the
[`DiagnosticInfo`](crate::error::DiagnosticInfo) trait page; each condition
type's own page displays its identifier string (the rendered `IDENTIFIER`
value in its `DiagnosticInfo` implementation) alongside its recovery
behavior. (A condition type's identity is its compile-time `IDENTIFIER`
const; the defaulted
[`DiagnosticInfo::identifier`](crate::error::DiagnosticInfo::identifier)
method exists only for binding/embedding adapter types that carry a
runtime identity — its documentation scopes the override.)

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

let diagnostic = result.diagnostics.iter().next().unwrap();
assert_eq!(diagnostic.severity(), Severity::Error);
assert_eq!(diagnostic.span().content(), r"\foo ");

// Match by type — never by a literal identifier string:
assert!(diagnostic.data().is::<UnresolvableCommand>());
// At a string boundary, the identifier comes from the type:
assert_eq!(diagnostic.identifier(), UnresolvableCommand::IDENTIFIER);
// Typed access to the payload's fields:
let condition = diagnostic.data().downcast_ref::<UnresolvableCommand>().unwrap();
assert_eq!(condition.name, "foo");
```

Third-party conditions are first-class: implement
[`DiagnosticInfo`](crate::error::DiagnosticInfo) on your own data struct
(there is a derive) and it flows through the same carriers — relevant once
you write custom construct parsers (see the
[Developer Guide](crate::guide::construct_parsers)).

Read next: [Learn techy by example](crate::guide::learn_by_example) — the
whole toolkit in small, complete, compile-checked examples.
