# Introduction

techy is a fast, extensible parser toolkit for LaTeX-like markup languages,
written in Rust. It reads marked-up source text — LaTeX itself, or any language
built from similar ingredients — and builds a *node tree*: a structured,
inspectable representation of the document (an Abstract Syntax Tree) that your
program can analyze, transform, or convert into other formats.

A design decision shapes the whole library: the parsing engine has **no
privileged language concepts**. There is no built-in math mode and no
hard-coded meaning for `{`, `}`, `%`, or `\`. The familiar LaTeX behavior is
provided by a *preset* — the [`latexlike`](crate::latexlike) module — built
entirely from the same public extension points that are available to you. If
you need a variant language, or a markup language that only resembles LaTeX,
you define it with the same machinery instead of working around a parser
with LaTeX behavior built in.

## What techy does

**Parsing.** A [`Language`](crate::core::Language) bundles everything that
outlives one parse — the initial [parsing
state](crate::guide::concepts_overview#parsing-state-and-deltas), the parse
driver, and an optional source resolver — and its `parse()` entry point produces a
[`ParseResult`](crate::core::ParseResult). Parsing can be *strict* (stop at the
first problem) or *tolerant* (recover and continue), governed by the
[`Recovery`](crate::error::Recovery) policy; problems are reported as
structured [diagnostics](crate::guide::concepts_overview#diagnostics-and-tolerant-parsing),
not prose strings.

**The node tree.** A parse produces a flat, immutable
[`NodeTree`](crate::core::node::NodeTree). Its structure is a small, closed set
of node kinds — character runs, groups, callable invocations, comments, lists
([`NodeKind`](crate::core::node::NodeKind)) — read through
[`NodeRef`](crate::core::node::NodeRef) proxies. A *callable* is anything
invoked by name in the source: what LaTeX vocabulary calls macros,
environments, and specials.

**Working with parsed trees.** The consumer toolkit lives in dedicated
top-level modules: content extraction in [`extract`](crate::extract), read-only
traversal in [`visit`](crate::visit), tree-to-tree transformation in
[`transform`](crate::transform), and tree-to-value recomposition (rendering a
tree into text or any other composed value) in [`recompose`](crate::recompose).
The [Node trees](crate::guide::node_trees) chapter gives the overview.

**Definitions that live in scopes.** Macros, environments, and specials are
described by *specs* (declarative descriptions of a callable's arguments and
behavior), organized in [`Package`](crate::core::specs::Package)s and resolved
through nested [`Scope`](crate::core::specs::Scope)s with lexical shadowing —
and definitions can change during the parse, as in LaTeX. See
[Defining macros, environments, and specials](crate::guide::specs).

**Multiple sources.** techy performs no input/output of its own. Content
lookup for `\input`-like constructs is delegated to the calling application
through the [`SourceResolver`](crate::source::SourceResolver) trait; resolved
content parses into the same tree at the inclusion point, so trees spanning
several sources are supported directly.

## Ways to use techy

techy is designed to be picked up at the level your project needs:

1. **Use the ready-made preset.** Parse LaTeX-like documents with the
   [`Latexlike`](crate::latexlike::Latexlike) language and register your own
   macro, environment, and specials definitions. The opt-in
   [`minidefs`](crate::latexlike::minidefs) package supplies a handful of
   familiar definitions for prototyping. Most applications stay at this level;
   [Learn techy by example](crate::guide::learn_by_example) shows it end to
   end.
2. **Customize how individual constructs parse.** A spec can supply its own
   argument parsers, or take over parsing of an invocation entirely, through
   the [`ConstructParser`](crate::core::constructs::ConstructParser) contract
   — see [Custom construct parsers](crate::guide::construct_parsers).
3. **Define a language.** Implement the [`Lang`](crate::core::Lang) contract —
   token rules, modes, extension types, a parse driver — to specify a custom
   LaTeX-like language; or extend the preset itself: a language with its own
   vocabularies joins the `latexlike` family (the
   [`LatexlikeLang`](crate::latexlike::LatexlikeLang) umbrella) instead of
   forking it. See [Defining a custom language](crate::guide::custom_lang).

At every level, the same tree-consumer toolkit (`extract`, `visit`,
`transform`, `recompose`) applies to the parse output.

## Where techy runs

**In an ordinary application.** The common case: a Rust executable or service
that parses documents and extracts, converts, or rewrites content.

**In embedded and WebAssembly builds.** The crate is `no_std`-friendly: it
depends only on `core` and `alloc` (sources are shared as `Arc`, so the target
must support atomics), and it performs no input/output of its own. This makes
techy suitable for constrained targets, including WebAssembly builds, where the
host supplies all input.

**Inside a Python extension.** techy can back a Python extension module
written in Rust (for example with PyO3) like any other Rust dependency. For
each type, thread-safety facts are part of the documented API: every rustdoc
page lists the type's auto traits (`Send`/`Sync`) — the parse output
[`NodeTree`](crate::core::node::NodeTree), for instance, shows `Send` and
`Sync` in its listing. The
[Integration](crate::guide::integration) chapter covers embedding and bindings
in depth.

## How this guide is organized

- The **User Guide** (this chapter through
  [Learn techy by example](crate::guide::learn_by_example)) covers using techy
  as it ships: the latexlike syntax, the node tree, defining macros and
  environments, and running the parser.
- The **Developer Guide** covers extending techy — custom construct parsers,
  custom languages, embedding and bindings, and the migration path from the
  Python library [pylatexenc](https://pylatexenc.readthedocs.io/) — plus the
  [Concepts overview](crate::guide::concepts_overview), the look-up page that
  the rest of the documentation links into.
- The **AI Guide** condenses the same material into chapters written to be
  loaded into an AI assistant's context.

Read next: [Language syntax](crate::guide::language_syntax), for what a
latexlike language looks like — or jump straight to
[Learn techy by example](crate::guide::learn_by_example) if you prefer code
first.
