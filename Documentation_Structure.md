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

This pillar covers all workspace crates built into the documentation bundle:
`techy` and `techy-derive` — the derive companion crate's rustdoc is user-facing
documentation under the same rules.

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
  
Each section has a markdown header (leading #, ##, ###, ####) with
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
we follow to implement this library - the *why* this library is implemented
in this way and *why* this particular design was singled out.
  
History of design decisions can be useful if a decision was made and was
later reversed, since this information can serve to highlight a conscious
choice was made with options carefully considered.  But otherwise history of
decisions should be omitted.

Decision entries carry no dates: a `Status:` line records the status and the
who/context that led to the decision (e.g. `Status: DECIDED (user-led).`), never
a date.  Dates appear only inside explicitly recorded reversal notes, where
preserving the history of a reversed decision is the point.

Each section has a markdown header (leading #, ##, ###, ####) with
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
should be concise, logical, and self-contained so that humans can clearly connect
the explanation with their current mental model and expectations.


### Documentation writing style

Explanations should be concise and clear, yet provide all the necessary context to
understand the individual parts of the explanation without intimate knowledge of
all of techy's features and internals.  Redundancy in this aspect is good.

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
- One guide is called `concepts-overview.md`.  It lists all the main concepts of the library — e.g. 'node tree', 'scope', 'parsing state', 'Lang generic', and similar — one `##` heading per concept, each with a concise, self-contained explanation.  A concept is referenced with a standard rustdoc link to the page's heading, e.g. `[parsing state](crate::guide::concepts_overview#parsing-state)`; heading slugs are immutable once published (see 'Technical considerations' below).  The page must list every main concept from the start — a concept's section may begin as a brief placeholder — so that references always have stable targets.  Where a single API item canonically embodies the concept, prefer linking that item directly.
- For each reference in the user API docs to some dev-doc section, or in case of being tempted to do so:  (A) Is this information about a core, user-facing feature of the library (e.g. scope, Lang, ParsingState)?  If so, do not reference ARCHITECTURE or DESIGN_RATIONALE, but reference instead the relevant concept in `concepts-overview.md` using a standard rustdoc link.  (B) Does this information explain the reason for a detail or quirk of the public API (e.g. why a function call has a particular shape or struct has particular form etc.)?  If so, summarize the rationale behind this design decision.  This information may be repeated multiple times at various locations (say <~ 5 times); if the same rationale is to be repeated more than that many times at different locations, identify the key location in the user API where this information is most relevant, and include a rustdoc cross-reference to that location at all the other locations inviting the user to consult that location for details about this design decision.  (C) Is this information purely about an internal implementation detail, with a high likelihood that this information is useless to anyone other than library maintainers?  Then include the information as a regular comment (not rustdoc) at the relevant locations, and in this case, include an explanation in the regular code comment and include a reference to the dev-docs as (for instance)  `Explanation of the issue/design decision/rationale ..., cf. [§dd-arch:sources-and-spans]`.  (D) Does this situation not clearly fit in either case (A), (B), (C)?  Then ask the user for an exception; if granted, include a dev-docs reference (as an exception) in the user-facing docs, using the format (for instance) `(cf. developer docs "ARCHITECTURE", Sources and spans, §dd-arch:sources-and-spans)`.


Developer docs may cross-reference other developer docs using
the dedicated `§dd-*:*` label structure.  It is important that ARCHITECTURE reference
relevant sections in DESIGN_RATIONALE to back up specific design choices.
Cross-references between developer docs (or internally in a developer-doc) can be
simply typed with the bare label in brackets, for instance as "cf. [§dd-dr:design].".

General rule: EVERY decision entry listed in DESIGN_RATIONALE — regardless of status
(DECIDED, PROPOSED, OPEN, or DEFERRED) — must be referenced at least once, at a
relevant location, from ARCHITECTURE.  No exceptions: open and deferred decisions in
particular would otherwise get lost in the masses.  This rule is enforced by manual
discipline: when adding, moving, or removing an entry or a label, grep the repository
for the label and keep the ARCHITECTURE references consistent (the procedure is
recorded in DESIGN_RATIONALE's "How to use and maintain this document" section).


## Related files

- README, CLAUDE, TODO_Big: live independently and are not considered to be part of the project documentation
covered here.
- dev-docs/archive/ - historical files, outdated, stale references. Do not touch.
- dev-docs/extra/ - exploration of some wilder ideas. Possibly outdated. Do not touch.
- Documentation_Structure.md (this file) - the standing specification of the project
  documentation system; lives at the repository root.


## Technical considerations

### Guides included in rustdoc

Guide chapters are plain Markdown files living in `docs/` at the repository root.
They are compiled into the rustdoc output as *documentation-only modules*:
`techy/src/lib.rs` pulls them in through a custom `guide` module with relevant
`#[cfg(doc)] #[doc=include_str!(...)]` directives.  Each chapter renders as a
page under `techy::guide`, with `docs/guide.md` as the
landing page.  The `#[cfg(doc)]` gate keeps these modules out of compiled code
entirely; rustdoc sets `--cfg doc` both when rendering documentation and when
collecting doctests, so the chapters exist only for documentation purposes, yet
their fenced `rust` code blocks are compiled and executed by `cargo test --doc`
like any other doctest. Every example in a guide is therefore compile-checked by
default; opt out deliberately with a `text` or `ignore` fence tag.

Markdown pulled in through `#[doc = include_str!(...)]` is processed exactly as if
it had been written as a documentation comment at that spot in `lib.rs`: standard
rustdoc intra-doc links — e.g. `[`ParsingState`](crate::state::ParsingState)` —
resolve and are checked by the compiler (enable the `rustdoc::broken_intra_doc_links`
lint as `deny` so link rot fails the build). One caveat: only the item-path part of
a link is verified; a `#fragment` anchor pointing at a Markdown heading — as used
for concepts, e.g. `crate::guide::concepts_overview#parsing-state` — is not
validated by the compiler. Heading anchors are therefore kept stable by discipline,
exactly like the `§dd-*` labels. Where an API item canonically represents a
concept, link to the item itself.

The documentation sidebar shows the chapters in a pinned "Guide" section through
a customized behavior in `docs/rustdoc-header.html`.  This file contains a
hand-maintained list of guide pages (`GUIDE_PAGES`).

Adding a chapter requires the following steps:

1. create `docs/<chapter>.md`;
2. declare its submodule in the `guide` block of `techy/src/lib.rs`;
3. add it to `GUIDE_PAGES` in `docs/rustdoc-header.html` (otherwise the chapter is
   missing from the sidebar);
4. list it in the chapter index of `docs/guide.md`.

Build with `cargo docs` (alias for `cargo doc --workspace --no-deps`); run all guide
examples with `cargo test --doc`. When verifying links, delete `target/doc` first —
rustdoc merges over stale output and can mask rot.


### Cross-references within rustdoc (guides and code API docs)

Available link forms (all standard rustdoc):

- API items: `[`ParsingState`]` or `[`ParsingState`](crate::state::ParsingState)`,
  including path-qualified forms.  Resolved and checked by the compiler; with the
  `rustdoc::broken_intra_doc_links` lint set to `deny`, a broken link fails the
  documentation build.
- Guide pages: link the chapter's module path, e.g.
  `[the parsing model](crate::guide::parsing_model)`.  Compiler-checked as well.
- Headings within a page: append the heading's anchor slug (lowercase, hyphens),
  e.g. `crate::guide::concepts_overview#parsing-state`.  The module-path part is
  checked; the `#fragment` part is not — treat published heading slugs as
  immutable, exactly like the `§dd-*` labels.

Conventions:

- Guides and API documentation may link each other freely in both directions;
  they form one documentation bundle.
- Where a single API item canonically embodies a concept, link the item; use the
  `concepts-overview.md` headings for cross-cutting concepts that no single item
  represents.
- `techy-derive` is documented in the same `cargo docs` build and its rustdoc is
  user-facing documentation under the same rules.  One limitation: `techy-derive`
  does not depend on `techy`, so its documentation cannot use intra-doc links to
  `techy` items; write such mentions as plain code spans (e.g. `techy::error`)
  and keep them minimal.

### Dev-Docs labels

- Exact placement syntax, so greps are uniform: label last on the heading line, e.g. ## Sources and spans [§dd-arch:source].

- Uniqueness: label names unique per document; the dd-arch:/dd-dr: prefix identifies the document — which is why a bare label needs no file path and no section number.  This path-independence is deliberate: references survive file moves and reorganization; the case-(C) comment format relies on it.

- Immutability: once assigned, never renamed or reused; if a section splits, the label stays with the primary successor; removal only after a grep shows zero references.

- Granularity: every #/##/###/#### heading carries a label for cross-referencing.  Keep labels as short as possible but sufficiently expressive that they can easily be kept immutable without clashing with other labels.

