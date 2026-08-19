# techy Architecture

The present-day structure of the techy library: what exists, how it fits together, and
the design principles its public surface follows. The *reasons* behind individual
decisions — arguments, rejected alternatives, open questions — live in the companion
[DESIGN_RATIONALE.md](DESIGN_RATIONALE.md).

## How to use and maintain this document [§dd-arch:self-meta]

- This document describes the **present day**: no history, no dates, no implementation
  phases (git history records how we got here). If a change makes a statement here
  stale, fix the statement in the same change.
- Every heading carries an immutable label (`[§dd-arch:<name>]`); cross-references — from
  the Design Rationale, from `//` code comments, from this document to the register — use
  bare bracketed labels (`cf. [§dd-dr:panic-policy]`), never file paths or section
  numbers. Label rules and the documentation system as a whole are specified in
  `Documentation_Structure.md` at the repository root.
- Each topical section ends with a **"Decisions behind this section"** list naming the
  Design Rationale entries that back it. Every register entry, whatever its status, must
  be referenced at least once from this document; when an entry is added there, add its
  reference here in the same change (the maintenance rules live in [§dd-dr:self-meta]).
- Code snippets appear only where a relationship is not visible in any single source
  file; the concrete API surface is the rustdoc, not this document.
- Keep this file at roughly ~50KB. Detail that outgrows a section belongs in the
  register (why) or in rustdoc (what, exactly).
- The documentation system’s own decision record lives in the register’s
  documentation topic ([§dd-dr:documentation]): rustdoc-hosted guides
  ([§dd-dr:rustdoc-guides]) and this two-pillar, label-based structure itself
  ([§dd-dr:docs-restructure]).
- **No reference to plans or execution phases**: This document **NEVER** contains
  references to temporary project plans, benchmark results, or execution phases.


## Library design principles [§dd-arch:lib-design-principles]

How the library behaves and what its public surface promises. Each principle names the
register entries holding the full argument; the implementation-side counterparts are the
register’s meta-principles ([§dd-dr:impl-design-principles]), and patterns that were
considered and rejected are collected in [§dd-dr:rejected-patterns]. The dependency
policy topic: [§dd-dr:dependencies].

1. **Data-driven where possible, trait-driven where necessary.** Anything that can vary
   *during a parse* — delimiters, escape characters, enabled features, specials
   recognition inputs — is plain stored data in the parsing state, changed only through
   reified deltas at a single transition choke point ([§dd-arch:state]). Traits are
   reserved for genuine *behavior* extension points: token readers, construct parsers,
   definition providers, source resolution, callable specs, and the per-language
   transition customizer. This one principle resolves most "how generic should X be"
   questions. (Cf. [§dd-dr:data-vs-traits].)

2. **One generic parameter.** A single `Lang` trait bundles all compile-time
   customization. Every core type takes one `L: Lang` parameter — never five. Simple
   usage stays visibly generics-free through preset zero-sized types and type aliases.
   (Cf. [§dd-dr:one-generic-param].)

3. **No privileged language concepts in the core.** No math mode, no `{`/`}`, no `%`,
   no `\` anywhere in the engine: all of it is data in the parsing state or definitions
   in scope-stack packages. The familiar LaTeX behavior lives in a *preset*
   ([§dd-arch:latexlike]). (Cf. [§dd-dr:no-privileged-concepts].)

4. **Zero-copy by default; logical content is first-class.** Tokens reference source
   content by byte spans. Node *textual content* is `TextContent`: span-backed when it
   came from parsing, owned when synthesized or normalized — the span is provenance,
   not the content's storage. Identity-bearing data (callable names) is always owned.
   (Cf. [§dd-dr:zero-copy].)

5. **Closed structural core, open payloads.** The engine knows a small fixed set of
   *structural* shapes — chars, group, callable invocation, comment, list — with no
   `Custom` variant and no open-ended node trait objects. Semantics attach through
   specs; custom data attaches through the `Lang`-supplied ext types (uniform per-node,
   plus the argument- and slot-record exts), orthogonal to structural identity.
   (Cf. [§dd-dr:closed-core-open-payloads],
   [§dd-dr:closed-node-kind].)

6. **Zero mandatory runtime dependencies; `no_std`-friendly core.** Hand-written
   `Display`/`Error` impls; no logging — library conditions flow through the
   diagnostics sink. The library builds with `core` + `alloc` only (`Arc` requires a
   target with atomics) and performs no I/O: anything needing an OS capability lives on
   the embedder's side behind a trait (`SourceResolver`). Exceptional, case-by-case
   additions of widely-used `no_std`-capable crates are permitted — `hashbrown`, backing
   the engine's derivation memo, is the only instance. (Cf. [§dd-dr:minimal-dependencies],
   [§dd-dr:no-std], [§dd-dr:map-containers], [§dd-dr:goals].)

7. **`Result` everywhere, panics never** — with first-class tolerant parsing (recovery
   tokens, a diagnostics sink, detection-site recovery) rather than a bolted-on flag.
   Panics are reserved for verifiably unreachable invariants, plus a small set of
   user-approved indexing-style accessors with non-panicking companions. The full
   contract: [§dd-dr:panic-policy]; the recovery model: [§dd-arch:errors].

Deliberate non-goals — techy is not a TeX engine (no catcodes, no expansion), and
streaming/incremental parsing is out of scope — are recorded with their reasons in
[§dd-dr:non-goals]; the project-level goals they serve (FLM as the target substrate,
extensibility without forking, low footprint) in [§dd-dr:goals].

# Overview of the library architecture [§dd-arch:arch]

The crate is organized in **three strata**:

```
S2  presets      latexlike (module; [§dd-arch:latexlike]); later: flm (separate crate)
S1  core         ONE mutually-recursive stratum, organized as topic modules:
                   Lang (+ NodeExtTypes) · state/ (ParsingState, deltas) · token/
                   (the Tokenization bundle + StdTokenization, the Token/StreamPosition
                   aliases, StdToken, the TokenKind view, TokenEdge, StdStreamPosition,
                   TokenRules, TokenReader, StdTokenReader) · spec/ +
                   scopes/ (CallableSpec; SpecsProvider, Package, Scope, ScopeStack)
                   · node/ (NodeTree, NodeKind, NodeRef) · constructs/ (ConstructParser
                   + standard parsers) · engine/ (Language<L>, ParseDriver,
                   ParserSession, ParseResult) · serialize/ (SerdeSession, the
                   capability traits, standard-table drivers; [§dd-arch:serialization])
                 Modules are topics for navigation, NOT dependency ranks.
S0  foundation   Lang-free true DAG:
                   source/ (Source, SourceSpan, SourceProvenance, SourceResolver,
                   LineIndex, plain byte-range Span, TextContent) · error.rs
                   (span-based structured diagnostics, recovery policy)
```

**Three enforced rules** — each mechanically checkable — replace any notion of "each
layer depends only on lower ones":

1. **S0 never names `Lang`.** (Checkable by imports.) S0 is the part testable without
   inventing a language, and where the zero-copy/no_std discipline bites hardest.
2. **S1 never names a preset.** (Checkable by imports.) The boundary behind design
   principle 3.
3. **The runtime ownership graph is acyclic.** (Checkable by inspecting struct fields.)
   nodes → {states, specs, sources}; states → specs; specs → parsers; sources → sources;
   no runtime value references nodes. This generalizes the source topic's
   cycle-prevention invariant.

A strict layer ladder conflates three different graphs ([§dd-dr:three-strata]): the
**type/signature graph** is cyclic inside S1 — harmlessly, and every cycle edge is
itself a decided feature; the **runtime ownership graph** must stay acyclic (rule 3);
the **build order** is a topological order over concrete machinery, DAG-shaped even
where signatures are mutually recursive.

Within S1 the useful distinction is not vertical but by **role**: plain data
(`StateData`, `NodeKind`, `TokenRules`, …); contracts (the dyn extension-point traits —
`TokenReader`, `SpecsProvider`, `CallableSpec`, `ArgumentParser`, `ConstructParser`,
`SourceResolver` — plus `Lang` and `ParseDriver`); standard machinery (`StdTokenReader`,
`Package`/`Scope`, `NodesParser`, …); orchestration (`Language`, `ParserSession`).

**Public export topology:**
the public API is exposed through re-export facades with one canonical path per
item — `techy::{source, error, extract, transform, visit, recompose, serialize}`
top-level, `techy::core` as a flat machinery hub with extracted satellites
`core::{token, constructs, specs, node}`, `techy::latexlike` — and internal
src modules are private (internal reorganization is never public-breaking).
The token satellite is bounded by its own placement rule: `core::token` holds
what a token reader produces, consumes and answers with — the token and
stream-position types, the `TokenReader` trait and the standard reader, the scan
helpers, the token rules the reader reads together with the overrides that change
them mid-parse and the caches derived from them, the types the specials-scan hooks
answer with, and the token conditions and errors. The hub keeps the `Lang` trait
(its associated types and hooks), the parsing state and its deltas, and the engine.
The extraction itself, with its item-by-item resolution: [§dd-dr:core-token-facade].
The topic modules sketched above describe the *internal* organization only. Full
decision incl. the specs-vs-hub author-side/run-side rule and rejected shapes:
[§dd-dr:public-namespace-topology]; the top-level standing of `techy::recompose`
and `techy::visit`: [§dd-dr:recompose-machinery], [§dd-dr:visit-engine].

**Stability rubric:** everything `pub` is one stability class under one semver
discipline — no unstable tier; access tiers are expressed by placement and guides,
not stability levels. The freeze is currently soft: until frameworks actually build
on techy, important discovered shortcomings may still be fixed breakingly; from that
adoption on, the freeze is hard. The guards: `missing_docs` is a workspace `deny`
lint, and `scripts/check_semver.sh` runs cargo-semver-checks against the
`api-baseline` git branch — a movable branch deliberately advanced at each version
bump, not a tag. Full decision: [§dd-dr:stability-rubric]. Surface leanness
follows a graduation policy: no test-support feature tier — embedders write their
own fixtures, and an item proven genuinely indispensable graduates to real public
API; parsing states stay crate-frozen even for embedders
([§dd-dr:embedding-feedback-policy]).

Decisions behind this section (full topic: [§dd-dr:crates]): [§dd-dr:three-strata],
[§dd-dr:public-namespace-topology] (export facades, one canonical path, hub +
extracted subsets), [§dd-dr:core-token-facade] (the token subset as the fourth
satellite, and its placement rule),
[§dd-dr:stability-rubric] (one stability class, soft freeze),
[§dd-dr:public-visibility-sweep] (the completed per-item pub-vs-pub(crate) sweep),
[§dd-dr:embedding-feedback-policy] (graduation over convenience surface; the
declined accessor batch), [§dd-dr:workspace-layout] (virtual workspace, crates in
subfolders), [§dd-dr:decisions] (how the register itself is organized).

## Sources and spans [§dd-arch:source]

`Arc<Source>`-based ownership: nodes and errors carry `SourceSpan` (an `Arc<Source>`
plus byte range), so trees and diagnostics are self-contained — transformed trees
outlive the `ParseResult` they came from, with no lifetime chains. The plain byte-range
`Span` (`Copy`, no `Arc`) also lives here: errors and readers use it independently of
tokenization. `TextContent` (span-backed or owned logical text) is likewise S0.

- **Provenance lives on `Source`, not on every location**: a provenance enum
  (`Primary` / `Resolved` / `Synthesized`, with `triggered_at: SourceSpan`
  back-references) — one hop per source, forming a walkable provenance tree for error
  reports. The reference graph is strictly layered (sources reference only sources), so
  `Arc` cycles are impossible by type definition. "Which node triggered this
  synthesized source" is a higher-level concern: a session-owned registry (general
  direction decided, details open — [§dd-dr:source-node-registry]).
- **Content resolution is pluggable**: the `SourceResolver` trait serves `\input`-like
  lookups; resolvers are configured on the parse driver (`with_source_resolver`, via
  the sealed `IntoSourceResolver` conversion), and an unconfigured driver resolves
  nothing (`None` — no zero-sized placeholder type). Resolvers return *content* (the
  caller mints the `Source`), are `Send + Sync`, and recursion/cycle policy belongs to
  the embedder ([§dd-dr:resolver-contract]) — with `Source::including_sources()` and
  the canned, origin-keyed `check_include_chain` as the one-line policy tools
  ([§dd-dr:include-chain-helpers]). `ResolveError` is `Clone` like every techy error
  type; its out-of-crate cause sits behind an `Arc`.
- **Line/column is lazy and display-only**: the parser works purely in byte offsets.
  Who computes and caches is layered ([§dd-dr:line-col-ownership]): the borrowing
  `LineIndex` view answers transient queries (`line_col`, `line_of`,
  `line_col_span`); persistence belongs to whoever holds a `LineIndexCache` (one
  owned line-starts table per source, keyed by `Arc` identity — entries never
  invalidate); and the rendering entry points accept any `LineColProvider` through
  their `_with` variants (the no-argument forms are transient-cache shorthand).
- **`Span` has private fields**; every mutator preserves `start <= end` (the monotone
  `extend_to`, the order-agnostic `cover`).
- The origin type is generic without `Lang` (`Source<O: SourceOrigin = Option<String>>`);
  the S1 core plugs `L::SourceOrigin` into that parameter.

Decisions behind this section (full topic: [§dd-dr:sources-and-spans]): [§dd-dr:arc-source-ownership],
[§dd-dr:provenance-on-source], [§dd-dr:source-node-registry], [§dd-dr:lazy-line-col],
[§dd-dr:source-resolver], [§dd-dr:resolver-contract], [§dd-dr:origin-genericity],
[§dd-dr:source-content-boundary], [§dd-dr:source-cursor-retired] (the retired cursor
seam), [§dd-dr:span-extend-to], [§dd-dr:include-chain-helpers]
(`including_sources` + `check_include_chain`; recursion stays embedder policy),
[§dd-dr:line-col-ownership] (consumer-held `LineIndexCache` + the
`LineColProvider` rendering seam).

## Tokens [§dd-arch:token]

A **token** is a value produced by a token reader, held and passed on by construct
parsers, and read by nobody but the reader that produced it. Its type is the language's
own — `Token<L>`, bounded `Clone + Debug + PartialEq + Send + Sync` — and it is
**opaque**: a parser asks the reader *what* a token is and *where* it is
([§dd-dr:token-opacity]). Tokens are transient, they never enter the node tree, and
tokenization copies nothing out of the source ([§dd-dr:zero-copy-tokens]).

A language declares its whole tokenization as **one** associated type,
`Lang::Tokenization: Tokenization<Self>` — a lifetime-free zero-sized bundle naming the
token type (`Tokenization::Token`), the stream-position type
(`Tokenization::StreamPosition`) and the static factory `make_token_reader(source)` that
builds the reader for one parse. `Token<L>` and `StreamPosition<L>` are type aliases
projecting through it; there is no marker trait. `StdTokenization` is the standard bundle
— `StdToken<L>`, `StdStreamPosition`, `StdTokenReader` — and what `TrivialLang` and every
language of this crate declares ([§dd-dr:tokenization]).

The reader answers three families of questions:

- **What a token is** — `token_kind(&token)` returns `TokenKind<'t, L>`, the closed *view*
  enum: single `Char`s (never runs), `GroupOpen`/`GroupClose` (with the matched
  delimiter, and for the open the resolved rule), `Command` (name and the escape
  character that fired), `Specials` (its full resolution), whole `Comment`s (start
  delimiter and content), `ParagraphBreak`, and a terminal idempotent `EndOfStream`. The
  variants carry the written spellings as `&str` and **no spans**. The view borrows the
  token — and the reader's content, for a reader that scans borrowed content — never the
  reader itself, so a parser may hold a view while it goes on reading and moving.
- **Where a token is** — `source_span_of(&token)` and `source_span_between(&token, a, b)`
  answer `SourceSpan`s. `TokenEdge` names the five boundaries of a token in reading
  order: `StartBeforePreSpace ≤ Start ≤ ContentStart ≤ End ≤ EndPastPostSpace`, where
  `ContentStart` is where the token's own content begins past its leading marker (after a
  comment's start delimiter, after a command's escape character; `= Start` otherwise).
  Edges may coincide.
- **Where the stream stands** — `position_here()` and `position_at(&token, edge)` answer
  values of `StreamPosition<L>`, opaque and obtainable only from the reader;
  `source_position_at` and `source_span_within(&begin, &end)` turn positions back into
  text locations ([§dd-dr:stream-position]). Repositioning is `move_to(&token, edge)` and
  `move_to_position(&position)`, and nothing else.

`StdToken<L>` is the token of the standard reader and of every language that uses it: it
stores byte ranges and `Arc`s only — no strings, no lifetime — behind eight public
constructors, one per kind, and its ranges are readable only by this crate's readers.
`StdStreamPosition` is the matching stream position, a byte offset with no public
constructor and no arithmetic. The concrete shapes live in `src/token` (public path:
`techy::core::token`, the facade of the whole token topic —
[§dd-dr:core-token-facade]); the token taxonomy with every argument is
[§dd-dr:token-model].

- **No invocation forms at the token level**: no macro/environment taxonomy and no
  `CallableTypeId` on `Command` tokens — `\begin` is a `Command` exactly like
  `\foobar`; which names mean what is resolution output, assigned at parse time.
  Terminology stack: **command** (token-level syntactic form) → **callable**
  (parse-level concept) → **macro/environment/specials** (preset-level invocation
  flavors).
- **Two callable-trigger kinds, split by production mechanism.** `Command` is
  recognized from `CommandRule` *data* (delta-changeable; fires unconditionally —
  unknown names resolve at parse time to fallback specs). `Specials` is recognized by
  the `Lang::scan_specials` *hook*, where recognition **is** resolution: the match
  carries the full `(callable_type, spec)` pair and the name is the matched text, so
  scan/lookup mismatches are unrepresentable. The hook hides behind the state-cached
  `TriggerChars` first-character filter, works on a `&str`, and reports failures as
  `SpecialsScanError` — plain errors, never recoveries ([§dd-dr:specials-scan-errors]).
- **Group tokens carry their resolved rule**: `GroupOpen` holds the winning
  `Arc<GroupRule<L>>` (class + spellings), the same make-mismatch-impossible principle;
  `GroupClose` carries only the delimiter string. Group *classes*
  (`Lang::GroupTypeId`) are detached from delimiter spellings; delimiter pairs are
  runtime rules data, declarable mid-parse ([§dd-dr:group-classes]).
- **Syntactic vs. content whitespace**: the whitespace preceding a token (its
  *pre-space*, between the `StartBeforePreSpace` and `Start` edges) is content whitespace
  and becomes nodes; post-space (between `End` and `EndPastPostSpace`) exists only where
  tokenization syntax consumes whitespace — multi-character command names, comment line
  ends. One primitive, `skip_whitespace`, enforces the paragraph rule everywhere:
  skipped whitespace never consumes a newline of a `\n\s*\n` sequence.
- **`TokenReader<'s, L>` is the behavior extension point** (catcode-like schemes keep
  their tables in `L::StateExt` — hence `peek` receives the full
  `&Arc<ParsingState<L>>`, and nothing beyond the state:
  [§dd-dr:reader-context-purity]). The trait has no associated types, so it stays
  object-safe and a parse context holds `&mut dyn TokenReader<'s, L>`. Reading is `peek`
  (produce the token at the current position, leaving the stream where it is) and the
  defaulted `next` (`peek` plus `move_to(&token, EndPastPostSpace)`). Its documented
  contract: `peek` is speculative and idempotent per (stream position, state *instance*)
  and implementations may memoize on `Arc` pointer identity; a peeked token's
  `StartBeforePreSpace` edge is the position the peek happened at; every token and every
  position a reader hands out stays usable for the rest of the parse, and positions
  compare with `==` only; interpretation stays with the issuing reader (or one over the
  same content), a foreign token being a caller-contract violation the standard reader
  cannot detect; a token kind belonging to an absent language feature is never produced;
  and `source_span_between` is indifferent to the order of its two edges. Two further
  clauses and a *Seams* section carry the span-tiling half of the contract — what a
  reader promises about sources, reading order and gaps, and what positions mean where
  one reader serves several sources at a single nesting level — beside the required
  `source_span_describing` ([§dd-arch:span-tiling]). A last clause fixes loop
  termination: every token but the terminal `EndOfStream` advances the stream (its
  `EndPastPostSpace` and `StartBeforePreSpace` positions differ), which the standard
  sibling loop enforces uniformly for all token sources — a consumed token that left the
  position unchanged is an implementation error, not a hang; an empty *span* stays
  legitimate, what the clause requires being two distinct positions
  ([§dd-dr:token-progress-guard]). A reader is
  installed by the language, as its `Lang::Tokenization` ([§dd-dr:tokenization]); a
  driver whose reader needs configuration the driver instance holds overrides the defaulted
  `ParseDriver::make_token_reader` instead ([§dd-arch:engine],
  [§dd-dr:token-reader-hook]).
- **A custom reader over standard tokens** does not reimplement interpretation: it keeps
  an inner `StdTokenReader` over the same content, builds tokens with the `StdToken`
  constructors, and delegates `token_kind`, the span answers and the position answers to
  the inner reader. Such a language declares a `Tokenization` of its own while keeping
  `Token = StdToken<L>` and `StreamPosition = StdStreamPosition`, which is why in-crate
  code requiring standard tokens bounds on that equality and never on
  `Lang<Tokenization = StdTokenization>` ([§dd-dr:tokenization]). A reader whose
  token type is its *own* but stores standard tokens read from one or several
  sources (a macro expander) keeps one inner `StdTokenReader` per source and reuses
  that reader's two public scanning methods directly — `scan_std_token_at` and
  `token_kind_of_std_token` — since the trait implementation for `StdTokenReader`
  serves only languages tokenized in `StdToken`/`StdStreamPosition`; one reader
  serving several sources at one nesting level requires `OBEYS_SPAN_TILING = false`
  ([§dd-dr:scan-helpers], [§dd-arch:span-tiling]).
  `TokenListReader` — internal test infrastructure only — is the second in-crate
  reader: the two-reader agreement harness runs every construct-parser parse
  against both and, since the list reader rejects tokens and positions it never issued,
  catches a parser that invents either ([§dd-dr:token-list-reader-demoted]).
- Tokenization priority: paragraph break → expected group close → longest delimiter →
  command escapes → comment starts → specials scan → forbidden check → `Char`. The
  ambiguous-delimiter case (`$…$`) is resolved by data
  ([§dd-dr:expecting-group-close]), not by privileged mode state.
- **The scanning primitives are public**: the *scan helpers* `skip_whitespace`,
  `scan_paragraph_break`, `scan_group_delimiter`, `command_rule_at`, `scan_command`,
  `scan_comment` and `scan_specials_trigger` are free functions of `core::token`,
  each recognizing one construct at a byte offset in the text handed to it and
  answering a source-free *match value* — plain `Span`s plus the rule or spec that
  matched (`GroupDelimiterMatch`, `CommandMatch`, `CommentMatch`, `SpecialsMatch`) —
  never a token, and never a `Source`. `StdTokenReader::scan_std_token_at` is their
  composition in the priority order above, so each construct is recognized in exactly
  one place; it and `token_kind_of_std_token` are what a reader over standard tokens
  from several sources reuses, while a reader with token kinds of its own composes
  the helpers and builds its own tokens. Every helper requires its `pos` to lie
  within the content on a `char` boundary and panics otherwise — the family's
  approved panic exception ([§dd-dr:scan-helpers], [§dd-dr:panic-policy] rule 3).

Decisions behind this section (full topic: [§dd-dr:tokens]): [§dd-dr:minimal-tokens], [§dd-dr:token-model],
[§dd-dr:tokenization] (the `Lang::Tokenization` bundle, the `Token<L>`/`StreamPosition<L>`
aliases, `StdTokenization`, and the two bound rules),
[§dd-dr:token-opacity] (opaque tokens, the reader-answered view),
[§dd-dr:stream-position] (opaque positions, the five edges, the two moves),
[§dd-dr:zero-copy-tokens], [§dd-dr:token-reader],
[§dd-dr:reader-context-purity] (the reader sees only the parsing state),
[§dd-dr:specials-scan-errors] (the scan hook reports errors, never recoveries),
[§dd-dr:expecting-group-close],
[§dd-dr:group-classes], [§dd-dr:command-escape-char],
[§dd-dr:token-contract-hardening] (the third-party-implementor contract batch),
[§dd-dr:token-list-reader-demoted], [§dd-dr:multi-newline-paragraphs],
[§dd-dr:enable-flags] (per-feature `enabled` gates), [§dd-dr:token-diagnostics],
[§dd-dr:scan-helpers] (the public recognition primitives, the two promoted
`StdTokenReader` methods, and the family's `pos` requirement),
[§dd-dr:core-token-facade] (the `core::token` facade and its placement rule).
A possible future merged first-character table is [§dd-dr:open-questions] item 1b.

## Parsing state [§dd-arch:state]

Parsing state is **materialized data behind a single transition choke point**. All
stored fields are private; the public read surface is getters over plain fields; and
the only way a non-initial state comes into existence is `derived()`. `StateData<L>`
holds the tokenization rules (`TokenRules` — one block per feature, each an `enabled`
gate plus its rules data), the first-class parsing **mode** (`mode: L::ModeId` — a
closed per-language vocabulary), the definition scope stack (`scopes: ScopeStack<L>`),
and the language extension (`ext: L::StateExt`). Above the runtime data, the language
declares **per feature, at compile time, whether the feature exists at all**
([§dd-dr:lang-features]): `Lang::Features` names a `LangFeatures` bundle of presence
declarations — three spellings of "off", never interchanged: *absent* (compile-time),
*disabled* (scoped runtime, data preserved), *empty* (constitutive). An absent
feature's storage collapses to a zero-sized store (its rules block, its override
block; for Scopes, the delta's op list and the stack itself), its code paths are
compile-eliminated, and feature-requiring entry points carry per-feature `LangHas*`
bounds with dependency edges compiler-enforced (`LangHasParagraphs:
LangHasWhitespace`).

- **Deltas are reified override values** (`ParsingStateDelta<L>`: rules overrides, a
  mode channel, scope ops, optional ext replacement, typed `L::Event`s) — data, not
  closures: mergeable, inspectable, propagatable. `derived()` applies a delta, runs
  `Lang::finalize_transition`, and freezes a new state; it is **fallible**
  (`Result<_, DeriveError<L>>`) because scope ops can fail — failing ops are skipped
  and collected — and because `finalize_transition` can refuse the transition
  (`FinalizeError`, folded into the same `DeriveError`); the error carries the
  recovered state plus the applied delta so tolerant callers can continue
  ([§dd-dr:scope-stack], [§dd-dr:enclosing-state-stack]).
- **Events come in two classes** ([§dd-dr:enclosing-state-stack]): *context-free*
  events are consumed by `finalize_transition`; *context-dependent* events (needing
  the enclosing states — the latexlike exit-math restore) are lowered to ordinary
  override patches by the driver hook `ParseDriver::resolve_state_event(&event,
  &ParsingStateStack)` inside `cx.derive_state` and never reach finalize — a
  context-requiring event reaching bare `derived()` is a loud `FinalizeError`,
  never a silent drop. The session keeps the live enclosing-state stack (the
  public, owning `ParsingStateStack`, also constructible post-parse via
  `from_states`/`from_node_ancestors`), pushed/popped at the same descent points as
  the traceback frame stack; it dies with the session — zero residue in parsed
  material.
- **Producer/scope split.** The party producing a change and the party deciding its
  scope differ. Inward scoping (a group interior): the parser derives a child state and
  drops it — reversion is structural, since the caller still holds the outer `Arc`.
  Outward propagation (`\newcommand`): the parser *returns* the delta; the caller
  applies it to its own state for subsequent siblings — a base the producer never saw.
  Construct parsers accordingly return `(output, Option<Box<ParsingStateDelta<L>>>)`
  (boxed so the mostly-`None` pass-through delta rides the parse recursion as one
  pointer — [§dd-dr:descent-guard]), and the caller applies deltas — never the
  producer.
- **Cross-cutting rules centralize in `Lang::finalize_transition`** — a pure function
  of (new data, previous state, events), run exactly once per unique derivation.
  Mode changes are *initiated* by deltas (e.g.
  the driver's math-group descent delta) and *interpreted* by finalize (disable
  features, adjust rules).
- **Airtightness is structural**: private fields, crate-owned freeze, the seed only
  from `ParsingState::lang_initial()` (freezing `Lang::initial_state_data()`, which
  may refuse a bad seed — both `lang_initial*` constructors return
  `Result<_, FinalizeError>`; the `LangHasScopes`-bounded
  `lang_initial_with_packages` pushes providers directly onto the seed's stack),
  everything else only from `derived()`
  ([§dd-dr:seed-states], [§dd-dr:hook-fallibility]).
- **Hot path = plain field reads.** Per-instance caches (the delimiter `PrefixTable`,
  the specials `TriggerChars`) are rebuilt eagerly at freeze, with the `PrefixTable`
  reused across derivations when its inputs are unchanged. Each cache collapses with its feature ([§dd-dr:lang-features]):
  `prefix_table()`/`trigger_chars()` return `Option`, `None` exactly for an absent
  feature (a merely disabled one answers `Some` of the frozen empty value).
  `dbg!(state)` shows exactly what the tokenizer will do (one recorded
  caveat: specials recognition sits behind the scan hook).
- States are immutable and `Arc`-shared; the engine creates a new one only at
  transitions, so all nodes parsed under one state share one `Arc` and record their
  parse-time state.

Decisions behind this section (full topic: [§dd-dr:parsing-state]): [§dd-dr:state-option-c] (the choke-point model and its
rejected alternatives), [§dd-dr:immutable-state-deltas], [§dd-dr:token-rules-data],
[§dd-dr:state-ext], [§dd-dr:lang-token-hooks], [§dd-dr:seed-states],
[§dd-dr:first-class-mode], [§dd-dr:enclosing-state-stack] (session-held enclosing
context; two-level event consumption, fallible `finalize_transition`),
[§dd-dr:temporary-group-rules] (the state-scoped delimiter
lifecycle enforced in `derived()`), [§dd-dr:trivial-lang] (`TrivialLang`, the
all-defaults test lang), [§dd-dr:on-ramp-defaults]
(`TokenRules::empty()`/`StateData::empty()`; specials defaults stay
recognize-nothing), [§dd-dr:lang-features] (the compile-time feature axis).

## Specs and scopes [§dd-arch:specs]

The **callable** concept, unified and **de-keyed**: a `CallableSpec<L>` records
*callable behavior*, not the form or name under which it is invoked. The invocation
form is `Lang::CallableTypeId`, a closed per-language associated type (like
`GroupTypeId` and `ModeId`); one spec may back several names (flyweight sharing).
Supertraits `Debug + Send + Sync + Any` — thread safety is a core contract, and `Any`
is the sanctioned downcast channel for preset finalization
([§dd-dr:spec-thread-safety], [§dd-dr:spec-downcasting]).

- **The spec's declarative surface is its argument list** — `&[Arc<ArgumentSpec<L>>]`,
  where an argument *is* a parser (`Arc<dyn ArgumentParser<L>>` plus optional name and
  per-argument state delta; [§dd-dr:argument-parser-model]). The standard delimited
  forms are shipped parser implementations parameterized by group class and rules.
- **The full-takeover escape hatch** is `make_invocation_parser`: a factory moving a
  fresh single-use construct parser to the caller, invocation facts traveling inside
  the parser instance. Overriding it is how `\verb`, tabular preambles, and FLM's rich
  constructs take over parsing entirely — pylatexenc's most valuable extensibility
  property, preserved. `requires_content()` is the emptiness surface consulted in
  expression-argument position ([§dd-dr:emptiness-surface]).
- **Slots are record-level vocabulary only** ([§dd-dr:no-spec-side-slots]): there is no
  spec-side slot list — body parsing needs invocation facts no declarative list can
  supply, so a body-bearing spec's takeover parses the body and mints the
  `ParsedSlot { name, region, ext }` records directly. Terminators are *parser*
  business, parameterizing the core `EnvironmentBodyParser`
  ([§dd-dr:slot-terminators]).
- **Definitions live in a scope stack** ([§dd-dr:scope-stack]): stack entries are
  `Arc<dyn SpecsProvider<L>>` — a fallible multi-method contract (`retrieve_spec` by
  `CallableQuery`, specials participation, functional `with_definitions` updates,
  best-effort `iter_symbols` enumeration with `ClosedVocabulary` supplying type
  universes; [§dd-dr:iter-symbols]). Standard impls: `Package` (immutable, loaded
  wholesale, mode visibility at package and per-entry grain —
  [§dd-dr:mode-visibility]) and `Scope` (the delta-targeted definition target,
  copy-on-write, created lazily). Resolution is innermost-first lexical shadowing — no
  conflict policies ([§dd-dr:lexical-shadowing]); lookups receive a `CallableQuery`
  (name, form, syntax) plus the state ([§dd-dr:callable-query]).
- **Unknown-callable policy is ordinary data**: per-form fallback singletons sit at the
  bottom of the stack as providers, so a callable node's spec is never `None` for a
  form with a registered fallback; "undefined on purpose" is an `ErrorCallableSpec`
  definition, and stacks do not nest.
- Mid-parse definition changes are delta scope ops (`Define`/`Remove`/`Unload`/…);
  scoped reversion is structural (outer states hold the old `Arc`s).

Decisions behind this section (full topic: [§dd-dr:specs]): [§dd-dr:unified-callable-spec],
[§dd-dr:argument-parser-model], [§dd-dr:closed-type-ids], [§dd-dr:spec-thread-safety],
[§dd-dr:spec-downcasting], [§dd-dr:scope-stack], [§dd-dr:lexical-shadowing],
[§dd-dr:callable-query], [§dd-dr:iter-symbols], [§dd-dr:mode-visibility],
[§dd-dr:no-spec-side-slots], [§dd-dr:slot-terminators], [§dd-dr:emptiness-surface],
[§dd-dr:registration-ergonomics] (`IntoSpecsProvider` conversion, preset one-liners,
no insert-time validation — traps caught at the resolution miss's did-you-mean
detail and the parse-init `check_provider_commands_shadowed_by_escape` instead),
[§dd-dr:resolution-extraction] (the standalone `resolve_command_in_scopes` in
`core::specs`; the resolution family beside it),
[§dd-dr:named-first-constructors] (`new(parser, name)` primary, `new_unnamed`
marked; `ParsedSlot` mirrored).

## Node trees [§dd-arch:nodes]

Flat, frozen, index-based storage with a **unified callable kind** and a **uniform,
parse-once-minted ext**. A `NodeTree<L, A = ()>` is an `Arc`-shared frozen core (one
`Vec<NodeData<L>>`, the stored parent table, the layout's `TreeTag`) plus a
consumer-owned per-node annotation vector `Vec<A>`; every node carries its kind, the
uniform `NodeExt<L>` (minted exactly once at staging by `Lang::make_node_ext`), its
`SourceSpan`, its parse-time `Arc<ParsingState<L>>`, and a contiguous `children:
Range<u32>`. `NodeKind<L>` is closed **and purely structural**:
`Chars`/`Group`/`Callable`/`Comment`/`List` — kind-shaped custom data is an enum
inside the ext. Access goes through `NodeRef` proxies (`Copy`, borrow-checked
against the tree; upward via the stored `parent()`, position-keyed via
`NodeTree::node_at`/`covering_slice` under the whole-run single-source slice
contracts — [§dd-dr:tree-navigation] — and range-keyed via the validated
`NodeTree::slice`, `Some` only for a sibling run); `validate_tree` is the public all-trees-law
checker (a `Result`, never panics); the byte-accounting oracle of the span-tiling law is
an in-crate test utility ([§dd-arch:span-tiling], [§dd-dr:tree-validation]).

- **No `Macro`/`Environment`/`Specials`/`Math`/`Custom` variants.** "Is this an
  environment" is two-level dispatch on `CallableData.callable_type`; `$…$` parses as
  a `Group` of the preset's math class under `Mode::Math`. The full resolution argument
  (why `Custom` died, the `Callable` merge, de-keyed specs, owned names,
  `TextContent`): [§dd-dr:closed-node-kind].
- **Division-of-labor rule (load-bearing)**: definition key `(CallableTypeId,
  normalized name)` → resolution; **node** → invocation facts (form, spelling, parsed
  arguments/slots, the Lang-owned `invocation_syntax` payload, per-instance ext);
  **spec** → shared behavior, stored
  once; **parsing state** → context at parse time; **uniform `NodeExt`** →
  cross-cutting per-instance concerns. Identity (names) is always owned; textual
  content is `TextContent` (span-backed or owned; accessors return `&str` either way;
  `materialize()` returns a fully-owned new tree).
- **Arguments and slots are child *regions*** ([§dd-dr:child-regions]): a callable's
  children range is the concatenation of one contiguous region per provided argument,
  then one per slot — each region holding leading noise (comments, whitespace-only
  `Chars`), the syntax-bearing nodes, and trailing per-instance syntax, with the
  *content* parser-designated (`ContentNodes`), never heuristically unwrapped.
  `ParsedArguments`/`ParsedSlots` are self-describing records
  ([§dd-dr:parsed-arguments]): each argument entry carries its `Arc`'d spec, presence
  is `Option`-ness of the region, markers are `Chars` nodes. Region records are staged
  in builder coordinates and resolved by `finish()` — an accepted two-phase runtime
  invariant, contained in one component.
- **Whitespace and span invariants** (the numbered statement: [§dd-dr:span-invariants]):
  chars accumulate into maximal `Chars` nodes; paragraph breaks are their own nodes
  (via the driver's `make_paragraph_break_node`); a callable's recorded post-space —
  the `Macro` arm of the invocation-syntax payload — is exactly its trigger token's
  own syntactic post-space, nothing beyond; and **sibling spans tile the parent's
  content interior exactly** where the language declares that its trees are span-tiled
  — the byte-accounting exactness consumers build on, and the one part of the statement
  that is conditional ([§dd-arch:span-tiling]). Environment `\begin{name}`/`\end{name}` scaffolding
  is rigid at parse time and *recorded* per side in the payload's `Environment` arm
  ([§dd-dr:invocation-syntax], which supersedes the reconstructed-scaffolding rule
  of [§dd-dr:environment-scaffolding]).
- **Recomposition levels**: level 1 — a node's own `SourceSpan` → the original text,
  exact under span tiling ([§dd-arch:span-tiling]) and with no external lookup; level 2 — Lang-aware quasi-equivalent reproduction from recorded
  facts. Consequence: per-instance syntax choices the spec does not determine live as
  region nodes or on the node itself (group delimiters on `GroupData`, comment start
  delimiters, marker spellings) — recomposability never depends on `Lang` cooperation.
- **The read/extraction surface** ([§dd-dr:read-api]): `NodeSlice` is the node-list
  currency (spans exact under span tiling, [§dd-arch:span-tiling]); `techy::extract` helpers
  (`split_at_chars`, `parse_keyval`, `content_as_chars`) mint real trees through the
  builder route; slot access is content-first ([§dd-dr:slot-read-api]); the
  **by-name** argument/slot accessors return `Result` — an unknown name or a
  non-callable is a `NamedAccessError`, never a silent `None`
  (`Ok(None)` = declared-but-absent; the indexed twins stay pure-`Option`;
  [§dd-dr:named-argument-errors]);
  `NodeRef::summary()` renders compact one-line node descriptions and the free
  `display_tree()` a guided subtree listing ([§dd-dr:display-tree]). Cross-tree id
  misuse is caught in **every build** by the always-on `TreeTag` in `NodeId`
  identity ([§dd-dr:tree-tags], superseding the debug-only scheme of
  [§dd-dr:node-id-provenance]).
- Indices are `u32` behind a private newtype; the one safeguard that matters is the
  checked conversion at the single mint site.
- **Annotations are consumer-owned** ([§dd-dr:node-annotations]): the `A` in
  `NodeTree<L, A = ()>` is a parallel `Vec<A>` over the `Arc`-shared node core,
  re-annotated zero-copy through `annotate()`. The ext system is **parse-once
  minting** ([§dd-dr:ext-minting]): no per-kind node exts; the required
  `Lang::make_node_ext` runs exactly once at staging, the builder demands ready
  ext + annotation (no fill-in-later hook), and parse staging goes only through
  `ParseContext::stage_node`.
- **Slot roles**: every slot carries `SlotRole { Content, Attached, Hidden }`,
  with trait-based body marking (`BodySlotExt`; [§dd-dr:slot-roles]). `\input`
  content attaches as an `Attached` slot of a same-builder sub-parse, making
  multi-source parse trees first-class ([§dd-dr:input-attachment],
  [§dd-dr:input-wiring]); the span-tiling law's oracle scopes its byte accounting per
  source through the roles (`Attached` regions carry their own accounting,
  `Hidden` regions none).
- **Transformation is the top-level `techy::transform`** (full topic:
  [§dd-dr:transform]): the streaming restage driver — `TreeRestager` (the
  builder-shaped entry point, [§dd-dr:traversal-builders]) +
  `RestageVisitor` with a closure blanket; top-down visits, bottom-up staging;
  `Descend` always descends, role-uniformly into `Attached`/`Hidden` slot
  children; read-frozen/write-staged; annotations single-pathway with
  origin-by-convention ([§dd-dr:restage]) — over region-aware context ops,
  constructible `RestagedArgument`/`RestagedSlot` bundles, generic
  `RestageError<E>`, the no-silent-repair edit policy (`ContentParentDropped`),
  narrow content-swap helpers, and the level-0 cross-tree `restage_node`
  primitive ([§dd-dr:restage-ops]). The extract producers mint output
  annotations through a general per-part callback with suffixed shorthands over
  any input annotation type (`SplitAtChars`/`KeyVals` results;
  [§dd-dr:extract-annotations]).
- **Recomposition is the top-level `techy::recompose`** — a meaning-free `Piece`
  value fold with instruction lowering
  (`TreeRecomposer::new(&mut recomposer).recompose(&tree, state)`;
  `Recompose::{Emit, Concat(ConcatPieces)}` with chainable
  `children()`/`wrap()`/`join()`; the `ComposePiece` monoid over `String` and
  `()` — streaming is a recomposer-held writer, no sink concept), bound to the
  per-node doctrine: spans are provenance — no inter-node span arithmetic; the
  recomposer never resolves span content ([§dd-dr:recompose]). Wrap-intended
  recomposers return instructions that lower against the *outermost* recomposer
  (the wrapping contract — targeted replacement is the wrapper pattern + the
  restage→recompose pipeline, not a mechanism); `Concat`'s default scope skips
  `Attached` AND `Hidden` slot children (the one role-sensitive site) with
  explicit widening opt-ins; `RecomposeError` and the `RecomposeContext` op
  roster mirror the restage family ([§dd-dr:recompose-machinery]). The substrate
  is recorded trigger spelling — the `Lang::InvocationSyntax` payload on
  `CallableData` ([§dd-dr:invocation-syntax]); core's span-tiling checker is
  payload-blind, the latexlike checker layers the payload pins. Source
  re-emission is ONE preset recomposer — `latexlike::SourceRecomposer`
  (`source_recomposer()`), reconstructing spelling from recorded facts via the
  invocation-syntax payload and the environment writer pair; the in-crate reemit
  oracle (`techy/tests/recompose_oracle.rs`) certifies payload completeness
  across strict + tolerant + multi-source matrices.
- **The read-only walk and the recompose driver share one traversal engine** in
  the top-level `techy::visit` module (`TreeWalker` + `NodeVisitor`/`VisitFlow`;
  `VisitContext` = engine bookkeeping only, the three-channel state discipline;
  the walk is role-blind — the deliberate read/compose asymmetry;
  [§dd-dr:visit-engine]).
- **All three traversal drivers are builder-shaped and depth-guarded**
  ([§dd-dr:traversal-builders]): `TreeWalker`/`TreeRestager`/`TreeRecomposer`
  hold the visitor by `&mut` borrow, take run configuration through `with_*`
  methods (`with_descent_guard_init`), and run via their terminal
  `walk(node)`/`restage(&tree)`/`recompose(&tree, state)` calls. Each run
  creates its own `StdDescentGuard` (one descent per tree nesting level);
  a refusal is the run's `DescentLimitExceeded`-style error (`WalkError` for
  the otherwise-infallible walk), and the guard's early warning is delivered
  to the visitor's defaulted `observe_descent_warning` hook.

Decisions behind this section (full topic: [§dd-dr:nodes]): [§dd-dr:flat-node-tree], [§dd-dr:closed-node-kind],
[§dd-dr:no-core-math-node], [§dd-dr:parsed-arguments], [§dd-dr:child-regions],
[§dd-dr:slot-ext], [§dd-dr:group-delimiters], [§dd-dr:mandatory-node-spans],
[§dd-dr:staging-builder], [§dd-dr:text-content-s0], [§dd-dr:comment-delimiters],
[§dd-dr:environment-scaffolding], [§dd-dr:span-invariants],
[§dd-dr:named-argument-errors], [§dd-dr:display-tree],
[§dd-dr:node-id-provenance], [§dd-dr:iter-storage-order], [§dd-dr:slot-read-api],
[§dd-dr:read-api], [§dd-dr:node-summary], [§dd-dr:tree-validation]; the
transformation topic ([§dd-dr:transform]): [§dd-dr:node-annotations],
[§dd-dr:tree-tags], [§dd-dr:ext-minting], [§dd-dr:restage], [§dd-dr:restage-ops],
[§dd-dr:recompose], [§dd-dr:recompose-machinery], [§dd-dr:visit-engine],
[§dd-dr:slot-roles], [§dd-dr:input-attachment], [§dd-dr:tree-navigation],
[§dd-dr:invocation-syntax], [§dd-dr:extract-annotations].

## Span tiling [§dd-arch:span-tiling]

Whether a language's parse trees are **span-tiled** is a fact the language declares: the
associated const `Lang::OBEYS_SPAN_TILING: bool`, defaulted `true`. That const's rustdoc is
the canonical definition — the children of every `List` and `Group` node tile the parent's
interior (one source, reading order, no gaps, no overlaps); a `Callable`'s children block is
span-contiguous within the node's span (with the `Attached`/`Hidden` exclusions of
[§dd-dr:slot-roles]); and every positional payload sits at its pinned position (a `Chars`
node's content is its whole span, a comment's start delimiter, content and post-space
partition the comment node's span, a group's delimiters are the prefix and the suffix); the
components are named here, the wording is the const's. "Obeys", because the property follows
from the language's tokenization and its parsers rather than from a knob: it is not a
`LangFeatures` member (that axis declares storage, [§dd-arch:state]) and not a marker type.
That every node carries exactly one span is a separate, unconditional rule.

The two regimes:

- **`OBEYS_SPAN_TILING = true`** — the default, and what every shipped language declares
  (`TrivialLang`, the preset; the in-crate test languages that declare `false` are the
  exception). The machinery enforces the property (a token stream that breaks it is reported
  as an implementation error) and the span-based accessors answer exactly:
  `NodeSlice::span`/`source_text` cover a sibling run with no holes, `NodeRef::span_content`
  reads back the text the node was parsed from, and the preset's source recomposer re-emits
  the input byte for byte.
- **`OBEYS_SPAN_TILING = false`** — the parsers assume nothing about where tokens come from
  (not the source, not the order, not the absence of gaps). A node covering several tokens
  is recorded with the span the reader *describes* (`TokenReader::source_span_describing`,
  required, no default body); content that no single reader answer covers is accumulated as
  owned text token by token (chars runs, verbatim bodies, and an environment name — the last
  read back through `NameGroup::name_text`, since the name drives a lookup), and no
  multi-token `Chars` node records span-backed content. Single-token facts keep the
  node-data rule (`node_text_content`: a span of the node's own source where the fact lies
  in it, the text itself otherwise), except where the payload is built before the node's
  span exists — the preset's recorded macro post-space, owned outright because there is no
  node span yet to test residency against. Node spans are coordinates the parser recorded,
  nothing more.

`ParseContext::source_span_within` is the public dispatch point between the two — the
reader's `None` is an implementation error under `true`, the described span under `false`;
the private `invocation_span_within` mirrors it for the invocation span — so construct
parsers written outside the crate get the behavior without special-casing it.

**The consumers rule** (load-bearing): techy's consumers obtain content from node *data* —
`TextContent` resolved against the node's own source, names, delimiters, payloads — never
from node spans, which are provenance coordinates. The coordinate accessors
(`NodeSlice::span`/`source_text`, `NodeRef::span_content`, `SourceSpan::content`) answer
exactly what the coordinates say, and say so. Every consumer therefore behaves as documented
under either declaration: `extract` reads node data throughout, and a source reemitter
re-emits a non-tiled tree *as stored*, claiming no byte-equality with any one source.

The token-side half of the property is the reader contract ([§dd-arch:token]): clause 7
(moving sets the position) together with clause 2 fixes where two consecutive tokens meet;
clause 8 (one source, in reading order, without gaps) is what the readers of a tiled
language promise; and the contract's *Seams* section states what positions mean where a
reader serves several sources at one nesting level — a **seam** being a place where the next
token comes from a different source, whose two sides are one position value. A chars run may
therefore legitimately cross a seam, which is why such content is recorded as owned text.

**Two laws, two audiences.** `validate_tree` is the public **all-trees law** — structural
sanity, region (index) tiling, `TextContent` residency — which every finished tree satisfies
whatever its origin, non-tiled parse trees included. The byte accounting of span tiling is
the **span-tiling law**, an in-crate test oracle only (`check_tree_invariants`, with the
preset's payload pins layered on it), and it checks the all-trees law alone for a language
declaring `false`. The matching test infrastructure is a scripted multi-source reader (test
builds only) that serves one parse from segments of several sources, with positions in a
canonical form so that contract clauses 2 and 7 hold at seams by construction; a latexlike
family member declaring `false` over that tokenization instantiates the preset unchanged —
driver, specs, syntax record, oracle and source recomposer are all generic over the family.

Documentation coins no name for the second regime: it says "a language with
`OBEYS_SPAN_TILING = false`". The phrasings this vocabulary replaced are listed with the
other rejected names ([§dd-dr:superseded-names]).

Decisions behind this section: [§dd-dr:span-tiling]; the invariants it qualifies —
[§dd-dr:span-invariants], [§dd-dr:tree-validation].

## Construct parsers [§dd-arch:constructs]

Everything a parser needs rides in one context value: `ParseContext` bundles the token
reader, the current state, the session, and the language's driver. It carries **no**
source handle ([§dd-dr:no-context-source]): every `SourceSpan` a parser stages comes from
the reader — one token's span, the span between two of that token's edges, or
`cx.source_span_within(&begin, &end)` over two stream positions — or from a `SourceSpan`
the parser already holds; `cx.here()` is the empty span at the reader's current position,
the anchor most diagnostics use. Node data keeps node-relative byte ranges, so a reader
answer becomes node data through `node_text_content`, which records it as spanned when it
passes a `same_source` check against the node's span and as owned text otherwise. What a
parser may assume about the tokens behind those answers — and what it records where it may
assume nothing — is [§dd-arch:span-tiling].

`ConstructParser::parse(&mut self, cx)`
returns `(output, Option<Box<ParsingStateDelta<L>>>)` — the caller-applies-deltas law;
the delta side is boxed so the mostly-`None` pass-through value rides the parse
recursion as one pointer ([§dd-dr:descent-guard]).
Construct parsers are **temporaries**: constructed with per-use configuration, working
state in fields, dropped with the frame; stored behavior objects (specs, argument
parsers) are immutable `Arc`-shared data — the two-tier ownership model
([§dd-dr:parser-temporaries]).

**Dispatch is by token kind + definition lookup — never by parser-registry scanning**
([§dd-dr:token-kind-dispatch], [§dd-dr:deterministic-dispatch]). The content loop
(`NodesParser`):

```
loop:
  tok = tokens.peek(state)
  match tokens.token_kind(&tok):
    Char            -> accumulate chars run (pre_space joins; whitespace-only runs allowed)
    ParagraphBreak  -> own node via driver.make_paragraph_break_node
    GroupOpen(rule) -> group parser under the session-memoized interior state
    Comment         -> comment node straight from the whole-comment token
    Command(name)   -> driver.resolve_command(state, &tok, tokens)
                       -> spec.make_invocation_parser(invocation)
                       (Unresolved/Failed -> diagnose + span-backed chars recovery)
    Specials(..)    -> make_invocation_parser likewise (resolution rides the token)
    GroupClose      -> stop-condition match? stop : StopCause::UnexpectedGroupClose
    EndOfStream     -> final whitespace chars node from pre_space; stop
  returned delta -> cx.derive_state(&delta) for subsequent siblings
returns (nodes, StopCause) — the caller interprets the ending.
```

- **Stop conditions are reified values plus tier-2 predicates**; abnormal endings are
  `StopCause` data, not errors — only the caller knows whether end-of-input before
  `\end{align}` is a problem ([§dd-dr:stop-conditions]). The predicates are
  fallible: an erring predicate aborts the parse instead of silently continuing
  ([§dd-dr:hook-fallibility]).
- **Where a parse output names a place in the stream, it carries a stream position**, not
  a number: `StopCause`'s two token-bearing causes carry the matched token's `SourceSpan`
  *and* `after` (just past the token), `EnvironmentBody::end` is where the environment's
  extent ends, `NameGroup` pairs the name's `SourceSpan` with the position past its close
  delimiter, and `ArgumentNoise::start` is where the absent-argument rewind returns to.
  A caller resumes with `move_to_position` and computes an extent with
  `cx.source_span_within` ([§dd-dr:stream-position]).
- **`Invocation` is the resolution result plus the token** (invocation form, name, spec,
  and the token that triggered it) — nothing about the token is cached beside it. Hooks
  that must interpret a token while holding no reader are handed the reader for the call:
  the resolution chain (`ParseDriver::resolve_command`, `CommandResolver`,
  `resolve_command_in_scopes`), the token stop predicate, the group child-state compute
  arm, and `FromInvocation::from_invocation` ([§dd-dr:token-opacity]).
- **Descent-state policy** is per-use configuration (`ChildStateSpec`:
  inherit/fixed/compute, one level deep by design — [§dd-dr:child-state-spec]); group
  interiors always carry their `expecting_group_close` and are deduplicated through
  the session memo. Optional-group arguments balance their delimiters via
  state-scoped temporary rules, with brace protection at any depth
  ([§dd-dr:optional-group-balancing], [§dd-dr:temporary-group-rules],
  [§dd-dr:brace-protection-limits]).
- **A group may leak an after-effect** ([§dd-dr:group-after-effects]): by default a group
  returns none — its interior's merged record dies with the descent, which is what makes a
  definition group-scoped — but a language installing `GroupAfterEffectsFn` through
  `make_group_parser` maps that record to the group's own after-effect for its caller (the
  `\gdef` shape). The `GroupOpen` arm applies and records it like an invocation's, so escapes
  compose outward, one hook per nesting level; the argument route still drops them (it has no
  delta channel).
- **Environment bodies** run through the core, parameterized `EnvironmentBodyParser`
  (terminator command + rigid name group + invocation-name back-reference); a
  terminator mismatch closes without consuming, letting enclosing levels claim their
  own terminators ([§dd-dr:terminator-mismatch-recovery]).
- **Every descent goes through one entry point**:
  `cx.parse_construct(parser, state, frame)` is the single, normative (MUST) way
  one construct parser runs another — it scopes the state structurally
  (`state: None` = clone the current state, identical scoping either way — never
  "skip the scoping"), pushes the optional traceback frame around the whole
  descent, maintains the session's enclosing-state stack, and asks the per-parse
  `DescentGuard` whether the parse may go one level deeper.
  `cx.parse_nodes`/`cx.parse_group` are one-line delegates (driver factory +
  `parse_construct` fused — the uniform-routing contract). Plain-Rust recursion
  bypassing the funnel is undetectable by design — a documented rule, not an
  enforceable one ([§dd-dr:descent-guard], [§dd-dr:parse-scoped]).
- **State scoping is structural**: the closure-shaped
  `cx.with_parsing_state(state, f)` replaces hand-rolled swap/restore (a
  state-scoping utility, not a descent point — it also maintains the session's
  enclosing-state stack; `cx.with_derived_state(&delta, f)` composes derivation
  and scoping); `cx.probe_token(&state)` is the public probe protocol
  ([§dd-dr:parse-scoped], [§dd-dr:enclosing-state-stack]).
- **Parsing depth is bounded by the descent guard** ([§dd-dr:descent-guard]): at
  every `parse_construct` descent the session's per-parse guard is consulted — a
  refusal aborts the parse with `DescentLimitExceeded` under any recovery policy
  (past the limit there is no safe way to continue); under the unconfigured
  built-in default, a one-time `DescentLimitApproaching` warning is recorded at
  half the budget. `StdDescentGuard` measures estimated stack consumption against
  a byte budget (`DEFAULT_STACK_BUDGET` = 250 KiB in all builds, deliberately
  tight in debug; `ComputedStackBudget` resolves probe() − `HEADROOM`); its
  `DepthLimit` mode counts engine descents (~2× the syntactic nesting depth;
  a tree traversal costs one per nesting level) as
  the deterministic alternative, and `off()` disables the bound. The consumer
  traversals are guarded the same way, each run under its own driver-configured
  guard ([§dd-dr:traversal-builders]) — so hand-built trees
  (`NodeTreeBuilder`) deeper than any parse limit are refused, not crashed on,
  when traversed.
- **Attached-source parsing** ([§dd-dr:input-wiring]): the
  `cx.parse_attached_source(source, state, parser)` door sub-parses an included
  source into the *same* session/builder over a fresh inner reader — the
  caller-supplied nodes-run parser drives it, stray closes recover locally (an
  included file's stray `}` never unwinds the includer), and a traceback frame
  anchors conditions at the inclusion site. Beside it,
  `cx.attach_source_reference(reference, at, state, parser)` is the single
  resolve-diagnose-attach raising site of two of the three `core.sources.*`
  conditions (`NoSourceResolver`, `UnresolvableSourceReference`); the third,
  `InvalidSourceReferenceArgument`, is defined beside them and raised by the
  invocation parser that reads the reference argument — its content must be plain
  characters, the reference coming from the argument's node data under every
  language ([§dd-dr:span-tiling]). The door returns an
  `AttachedSourceOutcome` — content nodes plus the included run's merged
  after-effect record (`NodesOutcome::after_effects`, the effective as-applied
  deltas merged in application order) — and slot assembly stays the invocation
  parser's job: the preset's opt-in `input_macro_spec(persist_state,
  attached_slot_ext)` stages the nodes as the `Attached` slot under the
  embedder-supplied ext (not-body in the shipped recipe) and, under
  `persist_state: true`, forwards the merged record as the invocation's own
  after-effect through the ordinary sibling channel.
- The standard inventory mirrors pylatexenc's parser library: the group parser,
  `StdInvocationParser`, the standard `ArgumentParser`s (group/optional/marker/
  expression, multi-delimiter `any_of`, chars-group, embellishments, tack-on fields,
  verbatim), `EnvironmentBodyParser`, `ExpressionParser` — the parity survey and its
  per-parser decisions: [§dd-dr:parity-gap-list], [§dd-dr:parity-parsers]. (No
  `CommentParser`: whole-comment tokens made it vestigial.) An argument parser owns
  its region's leading whitespace and comments (the shared `scan_argument_noise`
  step), which is what lets the group parsers' `require_adjacent()` opt-in decline
  that scan — pylatexenc's `allow_pre_space=False` as a parser property, never a
  state delta ([§dd-dr:argument-adjacency]).

Decisions behind this section (full topic: [§dd-dr:parsers-engine]): [§dd-dr:parse-context],
[§dd-dr:no-context-source] (no source handle; spans and positions come from the reader),
[§dd-dr:token-kind-dispatch],
[§dd-dr:parser-temporaries], [§dd-dr:invocation-parser-factory],
[§dd-dr:stop-conditions], [§dd-dr:child-state-spec], [§dd-dr:slot-terminators],
[§dd-dr:terminator-mismatch-recovery], [§dd-dr:optional-group-balancing],
[§dd-dr:brace-protection-limits], [§dd-dr:temporary-group-rules],
[§dd-dr:parse-scoped], [§dd-dr:descent-guard] (the `parse_construct` funnel, the
guard, the boxed pass-through delta), [§dd-dr:emptiness-surface],
[§dd-dr:parity-gap-list], [§dd-dr:parity-parsers], [§dd-dr:expression-fallback],
[§dd-dr:argument-adjacency].

## Engine and sessions [§dd-arch:engine]

`Lang` is the compile-time bundle: the associated types (`StateExt`, `SessionExt`,
`Event`, `ModeId`, `NodeExts`, `SourceOrigin`, `Driver`) plus the hooks of layers
callable outside a driven parse — `initial_state_data`/`finalize_transition` (state
layer), `scan_specials`/`specials_trigger_chars` (tokenizer layer), `make_node_ext`
(builder layer — the one *required* method: it mints each node's ext exactly once at
staging; restaged copies carry their exts verbatim, never re-minted;
[§dd-dr:ext-minting]).
`Lang` and `NodeExtTypes` are *defined* next to the state types (their signatures name
`StateData`/`ParsingState`); only `Language<L>` is genuinely an orchestration type.

**`ParseDriver` is the parse-behavior instance** ([§dd-dr:parse-driver]): everything
that only runs while a parse is driven lives on `Lang::Driver` — the token reader for the
parse (`make_token_reader`, defaulted to the reader the language's `Lang::Tokenization`
names; both construction sites, the root parse and each attached source, go through it, so
one override retokenizes a whole parse — [§dd-dr:tokenization],
[§dd-dr:token-reader-hook]), construct-parser
provision (`make_nodes_parser`/`make_group_parser`/`make_invocation_parser`
interception; one override applies to every descent site through the `cx` wrappers),
the group descent-delta channel (`group_interior_delta` — the math plug is one line of
mode-bearing data), the context-dependent event lowering (`resolve_state_event` over
the lent enclosing-state stack; [§dd-dr:enclosing-state-stack]), the `Recovery`
policy and the recover path, the probing peek (`probe_token`, reading through the reader
it is handed and answering an `Option<Token<L>>` under that policy), and the parse-time
hooks (`resolve_command` — receiving the triggering token together with a shared
reference to the reader that produced it, and answering the three-outcome
`CommandResolution` ([§dd-dr:resolution-detail], [§dd-dr:resolver-failure],
[§dd-dr:token-opacity]) — `make_paragraph_break_node`,
`refine_diagnostic`, `observe_transition`, and the once-per-parse
`observe_parse_start` — parse-initialization diagnostics, e.g. the preset's
all-escape-shadowed provider check). Drivers are instances, so behavior carries
configuration static hooks never could; preset parsers reach preset helper methods
fully typed. **Every item is defaulted** — `impl ParseDriver<L> for D {}` is a complete
driver, tokenization included. The parsing-depth guard is engine-fixed, not a driver choice:
the engine always uses `StdDescentGuard` (the `DescentGuard` trait states its
contract; wiring in another implementation is deliberately not offered). The
guard's per-language **configuration** travels on `Language`
(`with_descent_guard_init`, mirroring seed-state placement), and the per-parse
**instance** lives on the session — installed eagerly at parse entry, created
lazily from `Default` on the hand-built-context path, with
`ParserSession::install_descent_guard` as the
public seam ([§dd-dr:descent-guard]).

**Hook fallibility is a deliberate split** ([§dd-dr:hook-fallibility]): the hooks
with a real failure story return `Result` — the parser factories (an `Err` means
"could not build the parser", distinct from the guard's depth refusal),
`resolve_command`/`resolve_state_event`, the stop predicates and the
`ChildStateSpec` `Compute` arms, `body_state_delta`, `make_node_ext`
(builder-level), `initial_state_data` (through the fallible `lang_initial*` seed
constructors), and `observe_transition` — erring `HookFailed` (operational
failure in consumer code, with an optional cause chain), `ImplementationError`
(contract violation), or a domain condition (document diagnosis), and aborting
under any recovery policy. The remaining hooks (`recovery`, `refine_diagnostic`,
`make_paragraph_break_node`, `source_resolver`, `specials_trigger_chars`,
`ComposePiece::append`, `LineColProvider::line_col`) are deliberately infallible,
each documenting its neutral answer for an embedding whose implementation can
still fail internally. `observe_transition` is a dual channel — a diagnostics
sink for non-aborting observations, `Err` for aborts — with `L::SessionExt` as
the data half, read back from `ParseResult.session_ext`; the driver also picks
the diagnostics retention cap per parse (`diagnostics_limit()`).

**`ParserSession` is pure per-parse scratch**: the node builder, the diagnostics sink,
the live frame stack, the enclosing-state stack (lent to `resolve_state_event`;
dies with the session), the derivation memo, the per-parse `DescentGuard` instance
([§dd-dr:descent-guard]), and `L::SessionExt` (the parse-global
mutable extension). Session-mediated derivation is the in-parse standard
([§dd-dr:session-derivation]): `derived_state` is data-equivalent to
`ParsingState::derived()` — it may dedup and observe, never alter; the
parser-facing choke point is `cx.derive_state`, which lowers context-dependent
events first. Rules-only
derivations are memoized uniformly (identity-keyed, retention accepted;
[§dd-dr:memoized-derivations], [§dd-dr:state-memoization]); `observe_transition` fires
on every transition event, memo hits included — the two-level transition doctrine
(finalize constructs, observe accumulates).

**`Language<L>` is the long-lived runtime bundle** ([§dd-dr:language-parse-api]): the
driver instance and the frozen initial state, both mandatory `new` arguments —
`initial_state: impl Into<Arc<ParsingState<L>>>`, so an already-shared handle
seeds by identity ([§dd-dr:language-init],
[§dd-dr:embedding-feedback-policy]: seeds come from the fallible
`ParsingState::lang_initial()` / `lang_initial_with_packages(…)`, and further
customization derives *before* construction —
`lang_initial()?.derived(&delta)?`). The source resolver lives on the
driver, not here ([§dd-dr:input-wiring]); the descent-guard configuration lives
here, set with the `with_descent_guard_init` builder ([§dd-dr:descent-guard]). Entry points are two named methods —
`parse(content)` and `parse_source(Arc<Source>)` — plus accessors for the advanced
path; the root drive loop diagnoses stray closes through the recover funnel, stages the
consumed delimiter as a `Chars` node, threads the loop's evolved state through
diagnosis and resume, and finishes into a `ParseResult` that owns its tree, its
diagnostics, and the session extension (`session_ext` — the read-back for
`observe_transition` accumulation) with no `Language` reference. "Define a language once, parse many
documents": `Language` owns no per-parse state ([§dd-dr:stateless-language]).

Decisions behind this section: [§dd-dr:language-init] (explicit mandatory initial
state; the seed+packages construction path),
[§dd-dr:token-reader-hook] (`make_token_reader`: the defaulted per-instance override
for a custom reader), [§dd-dr:tokenization] (the language-side declaration it defaults
to),
[§dd-dr:hook-fallibility] (which
hooks return `Result`; the `HookFailed` condition and the three-way condition
split; the deliberately infallible remainder), [§dd-dr:parse-driver],
[§dd-dr:descent-guard] (the engine-fixed `StdDescentGuard`, the
`Language`-held configuration, the session-held instance), [§dd-dr:session-derivation],
[§dd-dr:state-memoization], [§dd-dr:memoized-derivations], [§dd-dr:finalize-node]
(superseded by parse-once ext minting),
[§dd-dr:resolve-command-hook], [§dd-dr:resolution-detail], [§dd-dr:resolver-failure],
[§dd-dr:paragraph-break-hook], [§dd-dr:language-parse-api], [§dd-dr:with-provider],
[§dd-dr:stateless-language], [§dd-dr:command-resolver] (the pluggable
`CommandResolver` strategy — standard value `ScopesCommandResolver` — on
`StdParseDriver`'s `R` parameter, with the generic/dyn resolver asymmetry and
constructor doctrine; supersedes the [§dd-dr:scopes-resolving-driver] component
struct), [§dd-dr:takeover-staging-sugar] (`disable_all`, collection constructors,
the `stage_invocation` helper with its end-position rule), [§dd-dr:override-all]
(`override_all`, its per-block family, and the transient group fields it never
carries), [§dd-dr:input-wiring]
(driver resolver accessor, the `parse_attached_source` door,
`attach_source_reference`), [§dd-dr:ext-minting] (parse staging only via
`ParseContext::stage_node`; the session's builder is crate-private),
[§dd-dr:invocation-syntax] (invocation spelling as recorded `CallableData`
payload, minted at the standard sites via the opt-in `FromInvocation`
constructor).

## Errors and tolerant parsing [§dd-arch:errors]

Zero-dependency, hand-written error types at runtime; declaration boilerplate is
generated by `techy-derive` (build-time only). Every diagnostic carries an Arc-based
`SourceSpan` ([§dd-dr:arc-error-spans]), so errors outlive the parse.

- **Structured conditions, not prose** ([§dd-dr:structured-diagnostics]): `Diagnostic`
  and `ParseError` carry a condition payload (`Box<dyn DiagnosticData>`) plus span and
  traceback frames — no message strings, no kind enum. The human message is a pure
  function of the payload (`Display`); condition types are plain data structs defined
  next to the construct that detects them, and third-party conditions are structurally
  identical citizens. Two identities: the concrete type in-process (downcast), the
  namespaced string identifier on the wire ([§dd-dr:condition-identities]) — the
  const identifier is the norm; binding/embedding adapter types carrying
  conditions defined at runtime in a host language may override the defaulted
  `DiagnosticInfo::identifier()` method per instance
  ([§dd-dr:runtime-condition-identity]);
  serialization is a derived projection through the minimal `DiagnosticValue` tree
  ([§dd-dr:serialized-schema]). The implementor/facade split and the derive:
  [§dd-dr:diagnostic-info-data-split], [§dd-dr:diagnostic-derive]. Identifiers are a
  semver-stable namespace: the string and the data keys are the contract (wording is
  not); `<area>` names a concept, never a file/module/type; the first segment names
  the defining vocabulary — preset conditions keep `latexlike.*` even inside a foreign
  `Lang`'s parse ([§dd-dr:wire-identifier-stability]).
- **Tolerant parsing is first-class** ([§dd-dr:tolerant-parsing]): a tokenizer error
  (`TokenError`) carries a source-qualified location — the reader is the party that knows
  which source its offsets belong to — and may carry a recovery token (`TokenRecovery`,
  whose explicit `resume` is a stream position that must move the reader; violations
  abort even in tolerant mode, [§dd-dr:resume-pos-contract]); the session's `Recovery`
  policy decides record-and-continue versus abort. Diagnostics accumulate on the session, capped with counted
  suppression (the cap is the driver's per-parse `diagnostics_limit()` choice), and
  render collections through one shared `LineIndexCache`
  ([§dd-dr:diagnostics-retention]) — every rendering entry point also has a `_with`
  variant taking a caller-held `LineColProvider` ([§dd-dr:line-col-ownership]).
  Recording order is recovery order; `sorted_by_position()` is the source-major
  re-sorted view (source order within each source, sources by first appearance —
  no total cross-source claim; [§dd-dr:diagnostics-position-sort]).
- **Detection-site recovery; `Err` means abort** ([§dd-dr:err-means-abort]): each
  condition defines its recovery where it is detected (markup-in-a-`Chars`-node is the
  standard tolerant artifact, always with a diagnostic); abnormal sub-parse endings are
  `StopCause` data; nobody continues past an `Err` — which keeps reader position and
  state `Arc`s coherent through recovery by construction. Implementation bugs
  (`ImplementationError`) abort even under tolerant recovery: tolerance promises a
  best-effort tree for bad *input*, not tolerance of buggy extensions
  ([§dd-dr:panic-policy]); operational failures of consumer-supplied hook code are
  the distinct `HookFailed` condition (detail plus optional cause chain) and abort
  likewise — the three-way split (hook failure / contract violation / document
  diagnosis) is documented on every fallible hook ([§dd-dr:hook-fallibility]);
  the descent guard's `DescentLimitExceeded` aborts
  likewise under any policy — past the limit there is no safe way to continue
  ([§dd-dr:descent-guard]).
- **Tracebacks come from an explicit frame stack** on the session
  ([§dd-dr:parse-traceback]): allocation-free live frames, snapshotted into every
  diagnostic as rendered title + span, innermost first — pylatexenc-style "while
  parsing …" reports for aborting *and* non-aborting diagnostics alike. A `Lang` can
  structurally replace a generic condition with its own richer one via
  `refine_diagnostic` ([§dd-dr:refine-diagnostic-hook]).

Decisions behind this section (full topic: [§dd-dr:errors]): [§dd-dr:panic-policy], [§dd-dr:arc-error-spans],
[§dd-dr:tolerant-parsing], [§dd-dr:err-means-abort], [§dd-dr:resume-pos-contract],
[§dd-dr:token-progress-guard], [§dd-dr:structured-diagnostics], [§dd-dr:diagnostic-info-data-split],
[§dd-dr:diagnostic-derive], [§dd-dr:condition-identities], [§dd-dr:serialized-schema],
[§dd-dr:wire-identifier-stability], [§dd-dr:runtime-condition-identity] (the
adapter-scoped `identifier()` override), [§dd-dr:parse-traceback],
[§dd-dr:refine-diagnostic-hook], [§dd-dr:diagnostics-retention],
[§dd-dr:diagnostics-position-sort].

## Serialization [§dd-arch:serialization]

`techy::serialize` converts the objects consumers handle — node trees (with
annotations), parsing states, sources, callable specs and providers, diagnostics,
parse results — to and from a format-independent value model (`SerialValue`), for
caches, inter-process exchange, snapshot tests, and inspection. Three strata sit over
one data path: a **foundation** that knows no object kind (the value model, the
capability traits, `SerdeSession` with its tables, contexts, segments, and errors),
the **core scaffolding** (the drivers and wire structures of the seven standard
tables — sources, states, specs, providers, trees, diagnostics, parse-results — with
typed positions and by-kind extension traits), and the **presets/frameworks** stratum
(a language's `SerializableLang` opt-in, its value conversions and object impls, its
`register` helper for readers). Live object → wire structure → `SerialValue` → table
entry → `Segment`; reading reverses it with validation at every step
([§dd-dr:serialize-sessions-segments]).

The capability is two trait pairs: `SerializableObject` (a supertrait of
`CallableSpec` and `SpecsProvider`, defaulted, so writing dispatches through the trait
object without registration) with the opt-in `DeserializableObject` read through
identifier-keyed readers and prefix resolvers; and `SerializableValue` /
`DeserializableValue` for embedded values, required of every type a language supplies
by the item-less `SerializableLang` bound ([§dd-dr:serialize-capability-traits]).
Interning by `Arc` identity writes each shared object once, so sharing survives; specs
and providers travel **by identity** (a `Weak` provenance stamp handed out by
`Package::new_shared`, resolved in the reader's `KnownProviders`) or in a
**self-contained** form — never as a lookup to re-run ([§dd-dr:instance-not-lookup]).
The value model holds only what the canonical JSON rendering round-trips
distinguishably, and the optional `serde` cargo feature gates rendering alone
([§dd-dr:serial-value-model]). Positions are stream-scoped in segments and
session-scoped in Rust code; a session reads then appends. Every read is fail-closed
validation of untrusted input; the developer description of the serialized form is
`dev-docs/serialize_schema.md`.

Decisions behind this section (full topic: [§dd-dr:serialization]):
[§dd-dr:serialize-capability-traits], [§dd-dr:instance-not-lookup],
[§dd-dr:serial-value-model], [§dd-dr:serialize-sessions-segments].

# Generics strategy [§dd-arch:generics]

Generic, via the single `L: Lang` ([§dd-dr:lang-genericity-scope]): the extension types
(`StateExt`, `SessionExt`, the `NodeExts` bundle — uniform `NodeExt` plus per-kind
exts), the closed id vocabularies (`GroupTypeId`, `CallableTypeId`, `ModeId`),
`SourceOrigin`, `Event`, and the `Driver`. Preset zero-sized types and type aliases
keep simple usage generics-free.

Deliberately **not** generic:

- **The shared pointer** (`Rc` vs `Arc`): a pointer GAT would infect nearly every
  signature to save ~1ns uncontended refcount bumps that happen once per node.
  `Arc` sits behind an internal alias so a later swap is mechanical
  ([§dd-dr:defer-rc-arc]).
- **Spec types** — extensibility comes from `CallableSpec` being a trait.
- **Content backing** — a plain `String` on `Source`; the once-planned trait seam was
  retired as information-equivalent to `&str`.

Every proposed new `Lang` associated type is challenged against
[§dd-dr:data-vs-traits] and [§dd-dr:one-generic-param] first.

Decisions behind this section (full topic: [§dd-dr:generics]): [§dd-dr:defer-rc-arc], [§dd-dr:lang-genericity-scope].

# Naming [§dd-arch:naming]

The durable naming principles (decision record: [§dd-dr:naming]):

1. **Generic over specific** — no `Latex` prefixes anywhere in the core (`Token`, not
   `LatexToken`). The library targets LaTeX-*like* languages; LaTeX-flavored names live
   in the preset.
2. **Specificity matters** — `ParsingStateDelta`, not `StateDelta` (delta of *what*?).
3. **Clarity over brevity** — `TokenResult`, not `TokResult`; `ParsedArguments`, not
   `Arguments` (the spec-side `ArgumentSpec`/`ArgumentParser` vocabulary coexists in
   scope — the recorded reversal: [§dd-dr:parsed-arguments-naming]).
4. **Context determines names** — but only when no sibling vocabulary competes in the
   same scope (the principle the `ParsedArguments` reversal sharpened).
5. **The Id-naming rule** (systematic across the crate): `…Kind` = closed core enum,
   exhaustively matchable (`TokenKind`, `NodeKind`); `…TypeId` = per-language
   *classification*, an associated type on `Lang` (`GroupTypeId` classifies group
   classes, `CallableTypeId` invocation forms — never a delimiter spelling);
   `Lang::ModeId` is the third closed per-language vocabulary but deliberately not a
   `…TypeId`: it names the mode a state *is in*, not a classification of a syntactic
   object.
6. **Transitions read as adjectives** — `ParsingState::derived()`, per Rust's
   `to_uppercase` convention: a *transition* producing a new value, not a field copy.
7. **`make_*` for factory hooks** — hooks that construct and hand over a fresh value:
   `CallableSpec::make_invocation_parser`, `ParseDriver::make_paragraph_break_node`.
8. **Three spellings of "off", each with its own word** ([§dd-dr:lang-features];
   the vocabulary itself: [§dd-arch:state]). The words are never
   interchanged; "disable(d)" stays reserved for the runtime action family
   (`TokenRulesOverrides::disable_all()`), "empty" for the all-empty constructors.
   The compile-time vocabulary carries `Lang*`/`Feature*` prefixes (`LangFeatures`,
   `FeaturePresent`/`FeatureAbsent`, `LangHas*`) — bare `Present`/`Absent`/`Has*`
   are too generic for the flat `techy::core` hub (principles 3–4).

The **terminology stack** is a naming discipline, not just a glossary — each term is
scoped to its stratum, and using one at the wrong level is a naming bug: **command**
(token-level syntactic form: escape char + name; `\begin` is a command, so is
`\foobar`) → **callable** (parse-level concept: anything invocable, resolved to a
`CallableSpec` with a `CallableTypeId` invocation form) → **macro / environment /
specials** (preset-level invocation flavors: the latexlike preset's registered
`CallableTypeId`s — "`\begin` is a command but not a macro").

The span-tiling vocabulary — *span tiling* / *span-tiled*, `OBEYS_SPAN_TILING`,
*seam*, `source_span_describing`, the *span-tiling law* — is defined once, in
[§dd-arch:span-tiling]; no name is coined for a language that does not obey span tiling
(documentation names the declaration instead).

Names that were consciously rejected or replaced must not be reintroduced — the
distilled list with reasons is [§dd-dr:superseded-names]; the full old-to-new registry
stays in git history.

Decisions behind this section (full topic: [§dd-dr:naming]): [§dd-dr:parsed-arguments-naming],
[§dd-dr:superseded-names], [§dd-dr:naming] (the convention decisions).

# The latexlike preset [§dd-arch:latexlike]

A module (`techy::latexlike`), not a separate crate; items are namespaced, never
re-exported at the crate root. The familiar LaTeX behavior, implemented entirely
through the public extension points — the demonstration that the core needs no
privileged concepts, and the pattern FLM will follow (as a separate crate).

- **`Latexlike`** is a zero-sized `Lang` with closed, bare, module-scoped,
  `#[non_exhaustive]` vocabularies ([§dd-dr:preset-vocabulary]): `GroupType`
  (`Content`/`Math(MathGroupForm)`/`Verbatim` — a *single* math class with
  inline/display as typed, exhaustive class payload declared at rule registration,
  [§dd-dr:math-group-form], [§dd-dr:group-taxonomy]), `CallableType`
  (`Macro`/`Environment`/`Specials`), `Mode` (`Text`/`Math`), and `Event`
  (`ExitMathContext` — the context-dependent exit-math restore,
  [§dd-dr:enclosing-state-stack]). `StateExt = ()` — the first-class `mode` field
  is the single source of truth.
- **The language family** ([§dd-dr:latexlike-generalization]): the `LatexlikeLang`
  umbrella (vocabulary bounds + overridable behavior defaults `math_group_rules`/
  `math_interior_forbidden_chars`; no blanket impl) over the per-vocabulary role
  traits `LatexlikeGroupType`/`LatexlikeCallableType`/`LatexlikeMode`/
  `LatexlikeEvent`, implemented by the preset enums — generic preset components
  take `LLL: LatexlikeLang`, and a framework language joins the family instead of
  forking the preset.
- **`LatexlikeDriver<LLL>`** carries the recovery knob and the paragraph-break
  shape flag ([§dd-dr:paragraph-break-style]); every behavior-bearing hook body is
  a one-line delegation to a public `LLL`-generic **pillar function**
  ([§dd-dr:preset-driver-pillars]): scope-stack `resolve_command` under the macro
  role (miss details name the searched providers), `math_group_interior_delta`
  (the math plug — inside math the math-class openers are removed while the
  derived forbidden characters merge in: no nested math,
  [§dd-dr:math-no-nesting]), `exit_math_context_delta` (the event lowering behind
  `resolve_state_event` — restore the innermost non-math enclosing context), and
  `make_paragraph_break_node`.
- **Default token rules**: `\` escape, `{}` content groups, `$ $$ \( \[` math groups,
  `%` comments. `[]` is deliberately **not** a group type — plain characters; optional
  arguments recognize brackets through per-use temporary rules.
- **The seed package `"_builtin"`** (`builtin_package::<LLL>()`,
  [§dd-dr:base-package]): exactly what any latexlike parse must have
  preloaded — the environment dispatch pair `begin`/`end` registered as ordinary
  entries ([§dd-dr:begin-end-dispatch]) — everything goes through the stack; nothing
  is hardcoded, everything is shadowable and unloadable. Both names are this
  package's data: the opening command is named by its registration, the terminator by
  `BeginSpec::new`'s argument, and the preset's conditions quote whichever spellings a
  source used ([§dd-dr:environment-command-names]). Typography specials are
  definitions content, not substrate: they live in `minidefs` (below); `&` was
  removed from the shipped definitions entirely.
- **`latexlike::minidefs`** ([§dd-dr:minidefs]): the opt-in toy package
  `minilatex_package::<LLL>()` — `\emph`/`\textbf`/`\textit`, `itemize`/`enumerate`
  with body-scoped `\item` (an inner `"minilatex.item"` package pushed by the body
  delta — the body-scoped-definitions exemplar), and the moved `~` + ligature
  specials (ligatures visible in the seed mode only). Never preloaded, never
  referenced by other preset modules (dead-strippable); deliberately a
  debug/prototyping tool, not a definitions database.
- **The definition one-liners** (`Package<LLL>::define_macro`/`define_environment`,
  [§dd-dr:registration-ergonomics]): inherent preset methods pairing callable type
  and spec type correctly by construction — shorthands of `insert`, not a second
  registration model. The parse-initialization check
  (`check_provider_commands_shadowed_by_escape` + the
  `LatexlikeLang::check_parse_start` wiring through
  `ParseDriver::observe_parse_start`) and the resolution-miss did-you-mean detail
  are the deliberate replacements for insert-time validation.
- **Preset spec types** ([§dd-dr:concrete-spec-types]): `MacroSpec`/`SpecialsSpec`
  (declarative, preset traceback vocabulary) and `EnvironmentSpec` — the funnel
  wrapper over a dyn `EnvironmentBehavior` with defaulted
  `arguments`/`body_state_delta`/`make_body_parser`
  ([§dd-dr:environment-spec-surface]) — driven by `BeginSpec`'s composition over core
  building blocks ([§dd-dr:begin-composition]). Orphan `\end` diagnoses at dispatch
  time with content-preserving recovery ([§dd-dr:orphan-end-recovery]).
- **Verbatim** ([§dd-dr:verbatim-family]): the features-disabled + expected-close
  recipe as data (`verbatim_state_delta`), with `VerbatimArgumentParser` (`\verb|…|`) and
  `VerbatimBodyParser` (raw environment contents up to a terminator, pluggable via
  `make_body_parser`); the terminator is given as a `VerbatimBodyTerminator` — a bare
  literal string, or the pieces of a stop command back-referencing the invocation name
  (`\end{verbatim}`), which the parser composes into the one raw string it reads up to
  and reports back as standard `Scanned` end facts; body content designation keeps
  every byte while gobbling the post-`\begin{verbatim}` newline out of the content
  ([§dd-dr:environment-body-content]).
- **The argument-code factory** (`argument_specs`, list-primary;
  `argument_specs_named` for `(code, name)` pairs;
  `argument_specs_from_str` for the compact grammar): xparse-like codes resolved to
  configured standard parsers ([§dd-dr:argument-specs-factory],
  [§dd-dr:argument-specs-list-primary], [§dd-dr:expression-fallback]; the
  `BracedOnly` word code is the fallback-off content-class group,
  [§dd-dr:argument-factory-additions]).
- **`NodeRef` sugar** is inherent on `NodeRef` for any family member
  (`impl<LLL: LatexlikeLang> …`: `is_math_group`, `math_form`, `macro_name`,
  `environment_name`, `specials_name`, and `post_space` over the
  invocation-syntax payload — reading vocabulary through the role traits;
  [§dd-dr:inherent-preset-sugar]); default whitespace is the six-character
  ASCII set ([§dd-dr:ascii-whitespace]).
- **`SourceRecomposer<LLL>`** (constructor `source_recomposer()`) is the
  preset's source re-emission — the ONE recomposer reconstructing spelling
  from recorded facts (macro escape + name + post-space; the environment
  record's `write_begin`/`write_end` pair; specials name-as-written), with a
  coherence error for payload/`callable_type` mismatches; accuracy = what the
  parse records, certified by the reemit oracle
  ([§dd-dr:recompose-machinery]).
- **The acceptance suite** (`techy/tests/acceptance.rs`) is a public-API-only
  integration port of pylatexenc's walker tests — anything the port cannot reach is an
  API gap by construction ([§dd-dr:acceptance-suite]).

**Preset generalization is complete** — every preset component is `LLL`-generic:
the role traits under the `LatexlikeLang` umbrella (the fifth role trait is
`LatexlikeInvocationSyntax`, carrying the invocation-syntax payload
`InvocationSyntaxData<Env>` and the `EnvironmentSyntax` record contract —
`from_parsed` + the writer pair, with composition-owned scanning;
[§dd-dr:invocation-syntax]), `default_token_rules::<LLL>`, the spec types
(`MacroSpec`/`SpecialsSpec`/`EnvironmentSpec` with the begin/end composition and
`VerbatimBehavior`), the canonical `ParagraphBreakSpec`, `argument_specs`, the
pillar functions and `LatexlikeDriver<LLL>`, the opt-in `input_macro_spec`,
`SourceRecomposer`, `builtin_package`, and `minidefs::minilatex_package`. `Lang`
itself stays whole.
[§dd-dr:latexlike-generalization], [§dd-dr:math-group-form].

Decisions behind this section (full topic: [§dd-dr:latexlike]):
[§dd-dr:latexlike-generalization] (role traits + `LatexlikeLang`; `Lang` stays whole),
[§dd-dr:preset-driver-pillars] (pillar functions + generic `LatexlikeDriver<LLL>`
assembly),
[§dd-dr:invocation-syntax] (the recorded invocation-syntax payload
`InvocationSyntaxData<Env>`; `EnvironmentSyntax`; fifth role trait
`LatexlikeInvocationSyntax`),
[§dd-dr:recompose-machinery] (the preset `SourceRecomposer`),
[§dd-dr:math-group-form] (`Math(MathGroupForm)` class payload), [§dd-dr:minidefs]
(toy `minilatex` package; deliberately not a definitions database), [§dd-dr:group-taxonomy], [§dd-dr:math-no-nesting],
[§dd-dr:preset-vocabulary], [§dd-dr:base-package], [§dd-dr:mode-visibility],
[§dd-dr:ascii-whitespace], [§dd-dr:inherent-preset-sugar],
[§dd-dr:argument-factory-additions] (`BracedOnly`, named factory, text-restore
event),
[§dd-dr:begin-end-dispatch], [§dd-dr:environment-spec-surface],
[§dd-dr:concrete-spec-types], [§dd-dr:orphan-end-recovery], [§dd-dr:verbatim-family],
[§dd-dr:environment-body-content], [§dd-dr:argument-specs-factory],
[§dd-dr:argument-specs-list-primary], [§dd-dr:expression-fallback],
[§dd-dr:paragraph-break-style], [§dd-dr:acceptance-suite],
[§dd-dr:begin-composition].
