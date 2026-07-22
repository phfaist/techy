I want to review and restructure the entire project documentation.

# Documentation pillars and organization

There are to be the following documentation pillars, which are sorted into
user-facing pillars and developer-facing pillars:

## Guides (user-facing).

Guides explain the overall architecture and philosophy of the
library and how to use it.  Guide through the fundamental concepts of the
library; guide through the parsing model; "quick start" guide for parsing
latex-like content; guide for developing your own custom language (that
follows techy's philosophy) with custom features etc. Etc. etc.

## API documentation directly in code (user-facing).

Documents call contracts, prerequisites, best use practices, and pointers to
how specific functions/structs can solve a particular problem when relevant and
appropriate.

## ARCHITECTURE.md (developer-facing).

To be moved to `dev-docs/ARCHITECTURE.md`.  Explains the present-day structure
of the library (e.g. strata); high-level design principles; specific outcomes
of design decisions, pointers to specific sections of DESIGN_RATIONALE.md (see
below).

Does NOT contain history of how decisions were reached or rejected alternatives.
No dates. No "Assessment of where things stand". Code snippets only when
absolutely necessary and not directly visible by looking up the relevant
source files.  Organized into logical sections that cover the different aspects
of the library.
  
Each section has a markdown header (leading #, ##, ###) with
a relevant title and a label `§dd-arch:<name>` (e.g. "§dd-arch:arguments-slots")
for easy cross-reference with `dd` standing for "developer-docs".  Labels
enable easy reorganization of the documentation without invalidating section
numbers.

Aim to keep ARCHITECTURE.md at around ~50KB size.

This document is structured as follows:
```
# How to use and maintain this document [§dd-arch:self-meta]
...

# Library design principles [§dd-arch:lib-design-principles]
...

# Overview of the library architecture [§dd-arch:arch]
...
## Node trees (example heading only) [§dd-arch:node-trees]
...
## Construct parsers (example heading only) [§dd-arch:construct-parsers]
...

```


## DESIGN_RATIONALE.md (developer-facing).

To be moved to `dev-docs/DESIGN_RATIONALE.md`.  Tracks the reasons behind
individual design decisions, along with rejected alternatives, for future
reference and to ensure overall consistency of design decisions with our
fundamental design principles.

Where ARCHITECTURE explains *how* the library is structured and how it behaves,
the DESIGN_RATIONALE explains *why* the library is structured in that way.
ARCHITECTURE records the design principles for public-facing APIs, which
are interpreted as "how the user should expect this library to behave"
(*how* the library behaves).  DESIGN_RATIONALE records the design principles
we follow to implement this library - the *why* this library is implmented
in this way and *why* this particular design was singled out.
  
History of design decisions can be useful if a decision was made and was
later reversed, since this information can serve to highlight a conscious
choice was made with options carefully considered.  But otherwise history of
decisions should be omitted.

Each section has a markdown header (leading #, ##, ###) with
a relevant title and a label `§dd-dr:<name>` (e.g. "§dd-dr:group-argument-parser")
for easy cross-reference with `dd` standing for "developer-docs" and `dr` standing
for the "design rationale" document.  Labels enable easy reorganization of the
documentation without invalidating section numbers, and the `dd-dr:` enables
cross-referencing across documents.
  
This document is carefully organized as follows:
```
# How to use and maintain this document [§dd-dr:self-meta]
...

# Implementation design principles [§dd-dr:impl-design-principles]

(Aka "meta-principles")

...

# Decision register [§dd-dr:decisions]
...

### Sources and spans [§dd-dr:sources-and-spans]
...

**Arc-based source ownership** — DECIDED (user-led).
Nodes carry ...


```


## Documentation guidelines

### Document cross-referencing

User-facing documents are generated with rustdoc using `cargo docs` (an alias for
`cargo doc --workspace --no-deps`).  Use internal standard rustdoc cross-references,
use inline doctests for examples, and follow rustdoc best practices for documentation.
The guides and API documentation are compiled together and may freely
cross-reference each other using rustdoc references that are checked by the compiler.

User-facing may generally NOT reference developer docs.  In user-facing docs, 
developer docs may NOT be referred to to document library behavior or API structure.
If a user is likely to be confused about why an API call is of a particular form, or
about design choices in the API, a self-contained explanation is to be provided in the
user-facing documentation itself.  A reference to developer-docs may only be included
in cases where significant additional information is available in the dev-docs and
would be inappropriate to include in the user-facing docs.  In such cases, clearly
reference as (for example):
`(cf. developer docs "ARCHITECTURE", Sources and spans, §dd-arch:sources-and-spans)`.
This should only be necessary in exceptional cases.

Developer docs may cross-reference other developer docs using
the dedicated `§dd-*:*` label structure.  It is important that ARCHITECTURE reference
relevant sections in DESIGN_RATIONALE to back up specific design choices.
Cross-references between developer docs (or internally in a developer-doc) can be
simply typed with the bare label in brackets, for instance as "cf. [§dd-dr:design].".

General rule: Each rationale-backed design decision listed in DESIGN_RATIONALE should
be referenced at least once at a relevant location in ARCHITECTURE.


# PLAN AND ACTION ITEMS FOR DOCUMENTATION REDESIGN

...

- what should we do with NAMING_STRATEGY?  Any important information there?  Keep design principles
  in ARCHITECTURE, drop individual names now that the code is actually implemented?
