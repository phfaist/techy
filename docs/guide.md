# Guide

Narrative documentation for techy: usage, concepts, and design patterns. Each chapter is
a sub-page of this module; the markdown sources live in `docs/` in the repository.

Chapters:

- [Learn techy by example](crate::guide::learn_by_example) — a tour of the
  `latexlike` preset in small, complete, compile-checked examples: parsing, defining
  macros and environments, math modes, verbatim, specials, strict vs. tolerant
  recovery, and content extraction.
- [The parsing model](crate::guide::parsing_model) — *(stub — content to be
  written)*.

(Future for these guides, excerpt from Claude: """The reassuring part for your current setup: nothing you'd do now gets locked in. Doctests and intra-doc link checking happen at the source level via cargo test / cargo doc regardless of which tool renders the HTML, and your narrative pages are plain markdown files that MyST/Sphinx (or mdBook) can consume nearly verbatim. So the hedge is simply: keep docstrings idiomatic markdown, keep prose in docs/*.md — already the case. And before reaching for Sphinx at all, note rustdoc's output is reskinnable to a fair degree (--extend-css, --theme, --html-in-header) — often enough to fix "rigid" without changing toolchains.""")
