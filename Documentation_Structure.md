# Project documentation organization

## Documentation pillars - user-facing and developer-facing

There are to be the following documentation pillars, which are sorted into
user-facing pillars and developer-facing pillars:

### Guides (user-facing).

Guides are user-facing documentation.  See important information on
cross-referencing below, especially regarding attempts to cross-reference
information in dev-docs.

Guides explain the overall architecture and philosophy of the
library and how to use it.  Guide through the fundamental concepts of the
library; guide through the parsing model; "quick start" guide for parsing
latex-like content; guide for developing your own custom language (that
follows techy's philosophy) with custom features etc. Etc. etc.

### API documentation directly in code (user-facing).

Code rustdoc API documentation is user-facing documentation.  See important
information on cross-referencing below, especially regarding attempts to
cross-reference information in dev-docs.

Documents call contracts, prerequisites, best use practices, and pointers to
how specific functions/structs can solve a particular problem when relevant and
appropriate.


### ARCHITECTURE.md (developer-facing).

ARCHITECTURE.md is developer-facing.

To be moved to `dev-docs/ARCHITECTURE.md`.  Explains the present-day structure
of the library (e.g. strata); high-level design principles; specific outcomes
of design decisions, pointers to specific sections of DESIGN_RATIONALE.md (see
below).

Does NOT contain history of how decisions were reached or rejected alternatives.
No dates. No "Assessment of where things stand". Code snippets only when
absolutely necessary and not directly visible by looking up the relevant
source files.  Organized into logical sections that cover the different aspects
of the library.

ARCHITECTURE does NOT contain explanations of implementation phases, which are
a historical detail of how the library was implemented - that information is
accessible through git commit history anyways.
  
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


### DESIGN_RATIONALE.md (developer-facing).

DESIGN_RATIONALE.md is developer-facing.

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

#### Arc-based source ownership [§dd-dr:arc-source-ownership]

Status: DECIDED (user-led).

Nodes carry ... (explanation)

Rejected alternatives: ... (if applicable, include brief explanation why rejected)

Revisit if: ... (if applicable)

```


## Documentation guidelines

### Audience: Both Humans and AI

Documentation should be designed and optimized both for HUMAN and for AI consumption.
Cross-references should be easy for an AI to follow (cf scheme above).  Explanations
should be concise, logical, and self-contained that humans can clearly connect
the explanation with their current mental model and expectations.


### Documentation writing style

Explanations should be concise and clear, yet provide all the necessary context to
understand the individual parts of the explanation without intimate knowledge of
all of techy's features and internals.  Redudancy in this aspect is good.

Do NOT use acronyms other than extremely widely-used and widely-understood ones.
(Prefer "WebAssembly" to "WASM"; use "Design Rationale" instead of "DR";
exceptions okay if reader is highly likely to know the acronym AND is easily
guessed from context, e.g. "JPEG image", "MD file".)



### Document cross-referencing

User-facing documents are generated with rustdoc using `cargo docs` (an alias for
`cargo doc --workspace --no-deps`).  Use internal standard rustdoc cross-references,
use inline doctests for examples, and follow rustdoc best practices for documentation.
The guides and API documentation are compiled together and may freely
cross-reference each other using rustdoc references that are checked by the compiler.
See technical information on cross-referencing within rustdoc in the
'technical considerations' section below.

User-facing may generally NOT reference developer docs.  In user-facing docs, 
developer docs may NOT be referred to to document library behavior or API structure.
If a user is likely to be confused about why an API call is of a particular form, or
about design choices in the API, a self-contained explanation is to be provided in the
user-facing documentation itself.  A reference to developer-docs may only be included
in exceptional cases (requires user approval for individual cases) where significant
additional information is available in the dev-docs and would be inappropriate to
include in the user-facing docs.  See next paragraph.

Here's how to handle a misplaced reference from user-facing pages (e.g. code API) to dev-docs.  This applies to existing such references, or if an agent or human is tempted to include such a reference.  Do this instead:
- One guide is called `concepts-overview.md`.  Here, we list all the main concepts in rustdoc-referenceable form (headings? objects? whatever works).  E.g., 'node tree', 'scope', 'parsing state', 'Lang generic' or similar, ...
- For each reference in the user API docs to some dev-doc section, or in case of being tempted to do so:  (A) Is this information about a core, user-facing feature of the library (e.g. scope, Lang, ParsingState)?  If so, do not reference ARCHITECTURE or DESIGN_RATIONALE, but reference instead the relevant concept in `concepts-overview.md` using a standard rustdoc link.  (B) Is this information explain the reason for a detail or quirk of the public API (e.g. why a function call has a particular shape or struct has particular form etc.)?  If so, summarize the rationale behind this design decision.  This information may be repeated multiple times at various locations (say <~ 5 times); if the same rationale is to be repeated more than that many times at different locations, identify the key location in the user API where this information is most relevant, and include a rustdoc cross-reference to that location at all the other locations inviting the user to consult that location for details about this design decision.  (C) Is this information purely about an internal implementation detail, with a high likelihood that this information is useless to anyone other than library maintainers?  Then include the information as a regular comment (not rustdoc) at the relevant locations, and in this case, include an explanation in the regular code comment and include a reference to the dev-docs as (for instance)  `Explanation of the issue/design decision/rationale ..., cf. [§dd-arch:sources-and-spans]`.  (D) Does this situation not clearly fit in either case (A), (B), (C)?  Then ask the user for an exception; if granted, include a dev-docs reference (as an exception) in the user-facing docs, using the format (for instance) `(cf. developer docs "ARCHITECTURE", Sources and spans, §dd-arch:sources-and-spans)`.


Developer docs may cross-reference other developer docs using
the dedicated `§dd-*:*` label structure.  It is important that ARCHITECTURE reference
relevant sections in DESIGN_RATIONALE to back up specific design choices.
Cross-references between developer docs (or internally in a developer-doc) can be
simply typed with the bare label in brackets, for instance as "cf. [§dd-dr:design].".

General rule: Each rationale-backed design decision listed in DESIGN_RATIONALE should
be referenced at least once at a relevant location in ARCHITECTURE.


## Technical considerations

### Guides included in rustdoc

... [@CLAUDE: INSERT TECHNICAL EXPLANATION HERE] ...

### Cross-references within rustdoc (guides and code API docs)

### Dev-Docs labels

- Exact placement syntax, so greps are uniform: label last on the heading line, e.g. ## Sources and spans [§dd-arch:source].

- Uniqueness: label names unique per document; the dd-arch:/dd-dr: prefix identifies the document — which is why a bare label needs no file path and no section number. That path-independence is the property making references immune to file moves and reorganizations; worth stating as the design intent, since your case-(C) format already relies on it.

- Immutability: once assigned, never renamed or reused; if a section splits, the label stays with the primary successor; removal only after a grep shows zero references.

- Granularity: (almost) every #/##/###/#### heading carries a labels for cross-referencing.  Keep labels as short as possible but sufficiently expressive that they can easily be kept immutable without clashing with other labels.



## PLAN AND ACTION ITEMS FOR DOCUMENTATION REDESIGN

Plan to make documentation align with the above guidelines ... ...

TODO ... ...

- what should we do with NAMING_STRATEGY?  I think it should be archived out of sight.
  Before that: Any important information there?  Keep guidelines/principles behind naming
  in ARCHITECTURE, drop individual name tables now that the code is actually implemented?
