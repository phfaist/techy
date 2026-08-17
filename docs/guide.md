# Guide

Techy is a fast, extensible parser toolkit for parsing a LaTeX-like language
as markup, written in Rust. It builds a
[node tree](crate::guide::concepts_overview#the-node-tree)
— an Abstract Syntax Tree — from LaTeX-like source code, which you can then
analyze, transform, serialize, or convert into other representations.

The engine itself is based on a generic core which has no hard-coded
privileged language concepts.  The
familiar LaTeX behavior (e.g., `\command` macros, `{…}` groups)
is provided by a preset, the
[`latexlike`](crate::latexlike) module, and custom LaTeX-like languages are
defined with the same public machinery.

The crate is [`no_std`-friendly with
minimal dependencies](crate#crate-dependencies-features-and-panic-policy).

This guide is the narrative documentation; the crate modules are the API
reference. Each chapter is a sub-page of this module, and the Markdown sources
live in `docs/` in the repository. The chapters are grouped into a **User
Guide** (using techy as it ships), a **Developer Guide** (extending techy with
your own parsers and languages, and embedding it elsewhere), and an **AI
Guide** (condensed chapters optimized for loading into an AI assistant's
context).  The AI Guide might also be useful for humans already familiar
with techy's philosophy and who might prefer a condensed, summarized map
of the library.

**New to techy? Read the [Introduction](crate::guide::introduction) first** —
it explains what the library is for and where each part of this guide fits.

## User Guide

- [Introduction](crate::guide::introduction) — the library's intent, target
  users, and capabilities, and the different levels at which techy can be
  used.
- [Language syntax](crate::guide::language_syntax) — what a "latexlike"
  language is: macros, environments, specials, comments, and groups, with
  definitions that can change during the parse.
- [Node trees](crate::guide::node_trees) — what a parse produces: the node
  tree and its node kinds, and what techy can do with a parsed tree
  (extraction, transformation, recomposition, visiting).
- [Defining macros, environments, and specials](crate::guide::specs) — how to
  define commands and their behavior for latexlike languages: packages,
  convenience constructors, and the spec types.
- [Running the parser](crate::guide::parsing) — running a parse: strict versus
  tolerant error recovery, the parser settings, the initial parsing state, and
  working with diagnostics.
- [Learn techy by example](crate::guide::learn_by_example) — a tour of the
  `latexlike` preset in small, complete, compile-checked examples: parsing,
  defining macros and environments, math modes, verbatim, specials, strict
  vs. tolerant recovery, and content extraction.

## Developer Guide

- [Concepts overview](crate::guide::concepts_overview) — the main concepts of
  the library in one place; a look-up page that other documentation links
  into.
- [The parsing model](crate::guide::parsing_model) — how parsing is executed
  and delegated: the parsing entry points, construct parsers, and the spec
  traits.
- [Custom construct parsers](crate::guide::construct_parsers) — how to write
  your own parser for a syntactic construct, including argument parsing and
  taking over parsing entirely.
- [Defining a custom language](crate::guide::custom_lang) — specifying the
  aspects of a custom language: callable and group types, extension types
  that attach custom information to nodes, and how to extend what the
  `latexlike` preset already implements.
- [Integration: tooling, embedding, and bindings](crate::guide::integration)
  — building tools on top of techy and embedding it in other environments,
  including bindings to other programming languages.
- [Serializing parses](crate::guide::serialize) — writing parse results,
  trees, states, and diagnostics into a format-independent value model and
  rebuilding them elsewhere: caching, inter-process exchange, snapshot tests.
- [List of panicking items](crate::guide::panics) — maintained exhaustive list
  of items in the crate that can panic.
- [Migrating from pylatexenc](crate::guide::pylatexenc_migration) — the main
  concept mappings between the Python library
  [pylatexenc](https://pylatexenc.readthedocs.io/) and techy.

## AI Guide

- [AI guide](crate::guide::ai_guide) — a condensed orientation to the whole
  library — module map, common task recipes, pitfalls — written to be loaded
  into an AI assistant's context, with pointers to the sub-chapters below.
- [AI guide: definitions](crate::guide::ai_guide_definitions) — condensed
  reference for defining macros, environments, and specials.
- [AI guide: node trees](crate::guide::ai_guide_trees) — condensed reference
  for reading, navigating, transforming, and recomposing node trees.
- [AI guide: custom languages](crate::guide::ai_guide_custom_lang) — condensed
  reference for implementing a custom language, its token rules, and its
  construct parsers.
- [AI guide: embedding](crate::guide::ai_guide_embedding) — condensed
  reference for embedding techy: bindings and threading facts, multi-source
  parsing, tooling entry points, and `no_std` use.
- [AI guide: serialization](crate::guide::ai_guide_serialize) — condensed
  reference for serializing parses and reading them back: the write and read
  sequences, registration obligations, errors, invariants, and how a language
  or framework of your own opts in.
- [AI guide: pylatexenc migration](crate::guide::ai_guide_pylatexenc) —
  condensed pylatexenc-to-techy mapping tables.
