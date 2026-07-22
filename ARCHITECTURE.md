# techy Architecture Plan

**Status: PROPOSAL — for discussion, July 2026.**
Written after a full review of all strategy documents, the current (non-compiling, mid-refactor)
source tree, and the pylatexenc sources. Where this document conflicts with older documents,
this document reflects the newer proposal; nothing here is final until discussed.

Decision points that need explicit sign-off are marked **[DECISION n]** and collected at the end.

**All decision points (1–7) were discussed and RESOLVED, July 2026.**
Decision 1 (parsing-state design): materialized state + transition choke point ("Option C");
see §state for the design and §4 for the recorded rationale. Decision 3 (node representation):
unified `Callable` kind + two-tier ext + `TextContent` ("Option F"); see §specs/§nodes for the design
and §4b for the recorded rationale. Decisions 2 (`Lang` + `Language<L>` naming), 4 (defer
`Rc`/`Arc` genericity), 5 (zero mandatory dependencies), and 7 (rebuild phase-by-phase) were
accepted as proposed. Decision 6: no `ConflictStrategy` is accepted; the deferred `SpecLookup`
semantics were settled in the Phase 4 design session (July 2026) — see §specs.

**Revised July 2026:** the strict L0–L7 layer ladder originally used to present §3 was
replaced by a three-strata dependency model (S0 foundation / S1 core / S2 presets) with three
enforced rules — see §3 and DESIGN_RATIONALE.md §3.11 (decision 8 in §11).

---

## 1. Assessment of where things stand

(This is outdated!)

Three generations of design coexist in the repo:

1. **Sonnet-generated exploration** (`pylatexenc_to_rust_strategy.md`,
   `TRAIT_BASED_ARCHITECTURE.md`, `PROPOSALS.md`, `ALIGNMENT_AUDIT.md`, TODO/QUICKSTART/etc.).
   Useful as an inventory of pylatexenc's feature set, but the architectural proposals are weak:
   `Box<dyn Node>` with downcasting, `can_parse`/`priority` parser registries, `TypeId`-keyed
   `Any` maps. These patterns are Java-in-Rust and should be discarded.

2. **Your state-based rethink** (`PARSING_STRATEGY.md`, Jan 2026): no privileged parsing modes,
   a `LanguageSpecification` trait bundling associated types, minimal structural tokens,
   construct parsers as the unit of extensibility. These are the right core ideas and this plan
   builds on them.

3. **Your source-management design** (`SOURCE_ARCHITECTURE.md`, Mar 2026): `Arc<Source>` spans,
   flat `NodeTree`, `NodeRef` proxies, `SourceResolver`, provenance. This is the most refined
   document in the repo and is adopted here essentially as-is.

The current `src/` is exploratory pseudo-Rust that does not compile. The most valuable idea in it
is the tolerant-parsing/recovery-token mechanism in `error.rs`/`stringreader.rs`. The per-facet
parsing-state trait experiment (`state/mod.rs` + `parsingstatedatatrait.rs` macro) is, I will
argue below, a dead end — see §4.

**Recommendation: treat the current `src/` as a quarry, not a foundation.** Rebuild
phase-by-phase with each phase compiling and tested before the next. (§9.)

---

## 2. Design principles

Derived from your stated goals and the decided parts of the existing documents:

1. **Data-driven where possible, trait-driven where necessary.** Anything that can vary *during a
   parse* (delimiters, escape chars, enabled features) is plain stored data in the parsing state,
   changed only through reified deltas at a single transition choke point (§state). Traits are
   reserved for genuine *behavior* extension points: token readers, construct parsers, spec
   lookup, source resolution, and the per-language transition customizer.
   This single principle resolves most of the "how generic should X be" questions.

2. **One generic parameter.** A single `Lang` trait bundles all compile-time customization.
   Every core type takes one `L: Lang` parameter, never five.

3. **No privileged language concepts in the core.** No math mode, no `{`/`}`, no `%`, no `\` in
   the engine. All of it is data in the parsing state or definitions in libraries. The familiar
   LaTeX behavior lives in a *preset* (§8).

4. **Zero-copy by default; logical content is first-class.** Tokens reference source content by
   byte spans. Node *textual content* is `TextContent` (§nodes): span-backed when it came from
   parsing, owned when synthesized or normalized — the span is provenance, not the content's
   storage. Identity-bearing data (callable names) is always owned.

5. **Closed structural core, open payloads.** The engine knows a small fixed set of *structural*
   shapes (chars, group, callable invocation, comment, list) — no `Custom` variant, no
   open-ended node trait objects. Semantics attach through specs; custom data attaches through
   per-node and per-kind ext types supplied by `Lang` (§nodes), orthogonal to structural identity.

6. **Zero mandatory dependencies; `no_std`-friendly core.** Hand-written `Display`/`Error`
   impls instead of `thiserror`; no `log` — library conditions flow through the diagnostics
   sink instead. **[DECISION 5 — decided]** The library builds with `core` + `alloc` only
   (`#![no_std]`; `Arc` requires a target with atomics): no file I/O, no `std`-only
   collections, `core::error::Error` (MSRV 1.81). Anything needing an OS capability — e.g.
   reading files for `\input`-like resolution — lives on the embedder's side behind a trait
   (`SourceResolver`). **[decided July 2026]**

7. **`Result` everywhere, panics never**, with first-class tolerant parsing (recovery tokens,
   diagnostics sink) rather than a bolted-on flag.

---

## 3. The architecture: three strata, three rules

*(Revised July 2026. An earlier draft presented a strict L0–L7 layer ladder here; review showed
its middle "layers" form one strongly-connected component **by design** — every cycle edge is a
decided feature: state stores libraries (`\newcommand`); lookup takes the state (mode-aware
`SpecLookup`); specs carry their invocation parser (the pylatexenc escape hatch); parsers build
nodes and derive states; nodes record their parse-time state and spec. No renumbering can make
that region a DAG, so the ladder was replaced by the strata below, whose boundaries are real.
Full argument: DESIGN_RATIONALE.md §3.11.)*

```
S2  presets      latexlike (module; §8); later: flm (separate crate)
S1  core         ONE mutually-recursive stratum, organized as topic modules:
                   Lang (+ NodeExtTypes) · state/ (ParsingState, deltas) · token/ (Token<'s, L>,
                   TokenRules, TokenReader, StdTokenReader) · spec/ + scopes/
                   · node/ (NodeTree, NodeKind) · constructs/ (ConstructParser + std parsers)
                   · engine/ (Language<L>, ParserSession, ParseResult, NodeRef)
                 Modules are topics for navigation, NOT dependency ranks.
S0  foundation   Lang-free true DAG:
                   source/ (Source, SourceSpan, SourceProvenance, SourceResolver, cursor,
                   LineIndex, plain byte-range Span) · error.rs (span-based diagnostics,
                   recovery) · TextContent
```

*(Revised after the July 2026 token-design review: the token topic is wholly S1 — tokens
are generic over `L` (a `Specials` token carries its resolved spec) and token errors may
grow state context; `Span` moved to the source topic. See DESIGN_RATIONALE.md §3.2.)*

Three enforced rules replace "each layer depends only on lower ones" — each mechanically
checkable, unlike the old ladder (which the design violated by intention):

1. **S0 never names `Lang`.** (Checkable by imports.) S0 is the part testable without inventing
   a language, and where the zero-copy/no_std discipline bites hardest.
2. **S1 never names a preset.** (Checkable by imports.) The boundary behind principle 3.
3. **The runtime ownership graph is acyclic.** (Checkable by inspecting struct fields.)
   nodes → {states, specs, sources}; states → specs; specs → parsers; sources → sources;
   no runtime value references nodes. This generalizes the Arc-cycle-prevention invariant of
   §source and is what the old "arrows point downward" rule was really about.

The disentangling insight: the ladder conflated three different graphs. The **type/signature
graph** is cyclic inside S1 — harmlessly: traits are just signatures, `dyn` references are how
a recursive knot is tied, and cross-module cycles within one crate are idiomatic Rust. The
**runtime ownership graph** must stay acyclic (rule 3). The **build order** (§9) is a
topological order over *concrete machinery*, which stays DAG-shaped even where signatures are
mutually recursive — stubs bridge the knot (Phase 2's tokenizer runs against a hardcoded
`TokenRules` precisely because the scanning core is knot-free).

Within S1 the useful distinction is not vertical but by **role**: plain data (`StateData`,
`NodeKind`, `TokenRules`, …); contracts (the dyn extension-point traits — `TokenReader`,
`SpecLookup`, `CallableSpec`, `ConstructParser`, `SourceResolver` — plus `Lang`); standard
machinery (`StdTokenReader`, `Library`, `NodesParser`, …); orchestration (`Language`,
`ParserSession`). Module = topic, not stratum.

The rest of this section walks the topics bottom-up; the former layer labels survive only as
section names (§source, §token, §state, §specs, §nodes, §constructs, §engine).

### source (S0) — adopt SOURCE_ARCHITECTURE.md

Exactly as decided in March: `Arc<Source>`-based `SourceSpan`, provenance enum
(`Primary` / `Resolved` / `Synthesized`) with `triggered_at: SourceSpan` back-references,
`SourceResolver` trait (`NoResolver` ZST default), standalone lazy `LineIndex`. Also home
of the plain byte-range `Span` (`Copy`, no `Arc`; moved here from the token topic, July
2026 — it is used by errors independently of tokenization). (The March plan's
`SourceContent` backing trait and mark/rewind cursor were retired July 2026, Action 06 —
the tokenizer scans `&str` directly; DESIGN_RATIONALE §3.1.)

One correction to the current `source.rs`: the per-location `via: [SourceLocationVia]` chain is
removed. Provenance belongs on `Source` (one hop per synthesized/included source), not on every
location — that is both cheaper and structurally cycle-free. The existing
`SourceLocationAnalyzer` becomes the standalone `LineIndex` utility (its lazy line-start logic
and traceback formatting are worth keeping).

`Source` is generic over `L::SourceOrigin` only through the `Lang` parameter; the default origin
type is `Option<String>` — conventionally the URL the content was obtained from, `None` when
unknown or when the content was synthesized (every source additionally carries a
`SourceProvenance` recording *how* it entered the parse). *(Revised July 2026: an earlier
name-plus-kind enum was dropped as too rigid — see DESIGN_RATIONALE.md.)*

**Arc-cycle prevention is structural, not a discipline.** `Source`, `SourceSpan`, and
`SourceProvenance` may reference other sources, never nodes; the reference graph is strictly
layered (nodes → sources/specs/state; sources → sources), so cycles are impossible by type
definition — verifiable by inspecting the fields of the source types. "Which node triggered
this source" is tracked at a higher level: `ParserSession` keeps a **synthetic-source
registry** tracking the synthesized/resolved sources it creates and the nodes that created
them. General direction decided; how the registry refers to nodes and its exact lifecycle are
to be decided. Note that `Weak<T>`, Rust's usual cycle-breaker, is not applicable here anyway:
nodes live in a flat `Vec<NodeData>`, not behind per-node `Arc`s, so there is nothing to point
a `Weak` at.

### token (S1)  *(reworked July 2026 — token-design review, DESIGN_RATIONALE.md §3.2)*

Tokens are **transient, span-based, zero-copy**, and generic over `L: Lang`
(`Clone`, not `Copy` — a `Specials` token carries an `Arc`):

```rust
pub struct Token<'s, L: Lang> {
    pub kind: TokenKind<'s, L>,
    pub span: Span,           // includes post_space where the kind has one
    pub pre_space: Span,      // content whitespace before the token — a span, not a String
}

pub enum TokenKind<'s, L: Lang> {
    Char(char),                                             // single character — never runs
    GroupOpen  { delim: &'s str, rule: Arc<GroupRule<L>> },  // resolved rule travels with the token
    GroupClose { delim: &'s str },
    Command    { name: &'s str, escape_char: char, post_space: Span },  // \foobar␣ — rules-data driven
    Specials   { callable_type: L::CallableTypeId, name: &'s str,       // scan-hook driven; carries
                 spec: Arc<dyn CallableSpec<L>> },                      //   its full resolution [6.4]
    Comment    { content: &'s str, post_space: Span },      // whole comment, sans newline
    ParagraphBreak,           // \n\s*\n run; span = first..last newline
    EndOfStream,              // terminal, idempotent; pre_space = final whitespace
}
```

Key points:

- Tokens are **structural and minimal**, and *parse-time-resolved* tokens carry **no
  invocation forms**: no macro/environment/specials taxonomy and no `CallableTypeId` on
  `Command` tokens. `\begin` is a `Command` like `\foobar`; environment recognition is
  entirely a parse-time preset concern. Terminology stack: **command** (token-level
  syntactic form) → **callable** (parse-level concept) → **macro/environment/specials**
  (preset-level invocation flavors). (`Specials` is the deliberate exception — next
  bullet.)
- **Two callable-trigger kinds, split by production mechanism.** `Command` is recognized
  from `CommandRule { escape_char, name_chars }` *data* (delta-changeable; fires
  unconditionally — unknown names resolve at parse time to fallback specs; no lookup at
  token time). `Specials` is recognized by the `Lang::scan_specials` *hook* — recognition
  *is* resolution: the `SpecialsMatch` returns name **and** the full resolution
  (`callable_type` + spec, the `ResolvedCallable` pair — amended July 2026, Phase 6.4)
  together, so scanning/lookup mismatches are impossible. The hook is gated by the
  state-cached `TriggerChars` first-character filter (`Lang::specials_trigger_chars`).
- **Syntactic vs. content whitespace.** `pre_space` (every token) is *content* whitespace —
  it belongs to the document flow. Post-space exists only where tokenization syntax
  consumes whitespace — multi-character `Command` names and `Comment` line ends — and is
  stored in those variants (`Token::post_space()` accessor). One primitive,
  `skip_whitespace`, enforces the paragraph rule for pre- and post-space alike: skipped
  whitespace never consumes a newline of a `\n\s*\n` sequence
  (`TokenRules::enable_multi_newline_paragraphs`).
- `peek` always returns a token: `EndOfStream` is terminal and idempotent, and its
  `pre_space` reports trailing whitespace so it reaches the node tree without the nodes
  parser touching raw content.
- The `'s` lifetime is ephemeral (borrows the current source unit's content); it never
  enters the AST. Nodes store `SourceSpan` (Arc-based).
- `TokenReader<L>` is the extension point for genuinely different tokenization *behavior*
  (catcode-like schemes keep their tables in `L::StateExt`, which only the full state
  exposes — hence `peek(&mut self, &ParsingState<L>)`, never `&TokenRules`):

```rust
pub trait TokenReader<'s, L: Lang> {
    fn peek(&mut self, state: &ParsingState<L>) -> TokenResult<'s, L, Token<'s, L>>;
    fn move_past(&mut self, tok: &Token<'s, L>, skip_post_space: bool);
    fn move_to(&mut self, tok: &Token<'s, L>, rewind_pre_space: bool);
    fn pos(&self) -> usize;
    // provided: next() = peek + move_past
}
```

  Contract: `peek` is idempotent per (position, state *instance*); implementations may
  memoize on `Arc` pointer identity (states are immutable). A different state — even one
  derived with an empty delta — voids the obligation.

**[FUTURE REVIEW — precompiled-table merging.]** Detection currently consults several
per-state structures: the `PrefixTable` (group delimiters), the `TriggerChars` filter
(specials), and per-rule checks for command escapes and comment starts. It may be worth
merging the starting characters of comments, group delimiters, and known specials/commands
into one precompiled table per state. Deliberately not done yet — revisit when the nodes
parser exists and the hot loop can be profiled (noted July 2026, user request; also
DESIGN_RATIONALE.md §6).

### state (S1) — parsing state  *(Decision 1 — RESOLVED, July 2026)*

Parsing state is **materialized data behind a single transition choke point** ("Option C" of
the design discussion recorded in §4). All stored fields are private; the public read surface
is getter methods over plain fields; and the *only* way a non-initial state comes into
existence is `derived()`.

```rust
pub struct ParsingState<L: Lang> {
    data: StateData<L>,          // private — getters are the public surface
    prefix_table: PrefixTable,   // derived caches, rebuilt when a state is frozen
    trigger_chars: TriggerChars, //   (eager, not OnceLock-lazy: no_std — see DESIGN_RATIONALE §3.3)
}

pub struct StateData<L: Lang> {
    pub rules: TokenRules,          // tokenization rules — plain stored data
    pub libraries: LibraryStack<L>, // definitions visible here (extendable mid-parse: \newcommand) [Phase 4]
    pub ext: L::StateExt,           // language-specific state (e.g. FLM's math mode)
}

pub struct TokenRules {
    // (amended July 2026: every major feature has an enable_* gate next to its data —
    // scoped disable/re-enable without carrying the rules; DESIGN_RATIONALE §3.2.
    // Gate false = scoped off, empty data = constitutive off.)
    pub enable_whitespace: bool,               // off = whitespace chars are plain content
    pub whitespace: WhitespaceRules,
    pub enable_multi_newline_paragraphs: bool, // \n\s*\n = paragraph break (token + skip rule)
    pub enable_groups: bool,                   // gates the delimiter table, NOT expecting_group_close
    pub groups: Vec<Arc<GroupRule<L>>>,        // {…}, […], $…$, $$…$$, \[…\] — delimiter pair + group class
    pub enable_commands: bool,
    pub commands: Vec<CommandRule>,            // escape-char syntaxes; empty = disabled
    pub enable_comments: bool,
    pub comments: Vec<CommentRule>,            // start delimiters (to end of line); empty = disabled
    pub enable_specials: bool,                 // gates the scan hook (baked into TriggerChars at freeze)
    pub forbidden_chars: String,               // no gate: one trivially restorable string
    pub expecting_group_close: Option<Arc<GroupRule<L>>>,  // ambiguous-delimiter disambiguator; ungated
    // NB: no specials strings — specials recognition is the Lang::scan_specials hook (§token)
}
```

(`TokenRules` is S0 data, *defined* in the token topic and merely *stored* here — shown for
context.)

**Deltas are reified override values** — pylatexenc's "changed kwargs", typed — and double as
the argument of the copy-with-style transition:

```rust
pub struct ParsingStateDelta<L: Lang> {
    pub rules: TokenRulesOverrides,           // every field an Option — None = unchanged
    pub push_libraries: Vec<Arc<dyn SpecLookup<L>>>,
    pub ext: Option<L::StateExt>,             // whole-value replacement; generic code leaves it None
    pub events: Vec<L::Event>,                // semantic transitions (preset/FLM territory)
}

impl<L: Lang> ParsingState<L> {
    /// The sole constructor of non-initial states — the transition choke point.
    pub fn derived(&self, delta: &ParsingStateDelta<L>) -> ParsingState<L> {
        let mut data = self.data.clone();
        delta.apply_overrides(&mut data);                     // internal, pre-freeze
        L::finalize_transition(&mut data, self, &delta.events);
        ParsingState::freeze(data)                            // caches rebuilt lazily
    }
}
```

*(Amended July 2026, Phase 7 plan session: `StateData` gains a first-class **parsing
mode** — `mode: L::ModeId`, a third closed per-language vocabulary — with a matching
`ParsingStateDelta.mode` override channel; deltas initiate mode changes (e.g. the
driver's group descent-delta for math groups) and `Lang::finalize_transition` interprets
them. `libraries: LibraryStack<L>` becomes the redesigned scope stack (§specs).
DESIGN_RATIONALE.md §3.3/§3.4.)*

Properties, roughly in decreasing order of importance:

- **Functional contract, no observable mutation.** `derived()` is state-in/state-out; the
  `&mut` exists only inside it, on a clone nothing else can observe yet.
- **Producer/scope split.** The party producing a change and the party deciding its scope
  differ, which is why the delta must be a standalone value. For *inward* scoping (group or
  math interior) a parser derives the child state itself and drops it when done — inside a
  parse frame through the session seam (`session.derived_state(…)` / keyed memo helpers —
  amended July 2026, DESIGN_RATIONALE §3.6), out of parse via `state.derived(…)` directly
  — reversion is structural (the caller still holds the outer `Arc`); deltas are never
  inverted. For *outward* propagation (`\newcommand`) the parser returns the delta to its
  caller, who applies it to **its own** state for subsequent siblings — a base the producer
  never saw. Construct parsers accordingly return `(output, Option<ParsingStateDelta<L>>)`.
- **Deltas are data, not closures**: mergeable (several changes → one delta → one
  `finalize_transition` run → one `Arc`), inspectable (diagnostics can record *why* a
  transition happened; golden tests can assert on it), and propagatable. A builder
  (`ParsingStateDelta::new().escape('#').event(…)`) keeps construction pleasant; a
  `state.derived_with(|d| …)` sugar can build the struct internally.
- **Cross-cutting rules centralize in `Lang::finalize_transition`** — the customizer hook,
  run exactly once per transition. Example: FLM's "in math mode the escape char is `#`"
  lives in FLM's finalize; the math-open parser only emits an `EnterMath` event, and no
  delta writer anywhere needs to know the rule. This preserves what computed getters would
  have bought, paid once per (rare) transition instead of per (hot) token read.
- **Override policy is the customizer's business, not core law.** When a rule and an explicit
  override interact ("`SetEscapeChar('@')` arrives while in math mode — clobbered at the next
  transition?"), the policy lives inside the finalize function, written by the one author with
  context. Two idioms: *pure normalization* (recompute dependent settings from `ext` + base at
  every transition; idempotent, can't miss a path, clobbers in-scope overrides) vs
  *event-driven* (touch only what each event implies; preserves overrides, must cover every
  contributing path). The default lang's finalize is empty, so the question only exists for
  whoever writes a customizer.
- **Airtightness is structural.** Private fields plus crate-owned constructors — the seed
  comes only from `ParsingState::initial()` (freezing `Lang::initial_state_data()`), every
  other state only from `derived()` — mean the compiler guarantees finalize sees every
  change after the seed. The one non-structural piece is the seed itself: finalize has no
  `prev` to run against there, so the seed's coherence is the `Lang` author's documented
  contract (the hook's docs; the author of `initial_state_data` and of
  `finalize_transition` is the same party).
- **Hot path = plain field reads.** Per-instance caches (the sorted delimiter-prefix table
  with open/close-ambiguity merging, salvaged from the WIP) stay valid for the `Arc`'s
  lifetime because states are immutable — and `dbg!(state)` always shows exactly what the
  tokenizer will do.
- **Math mode does not exist here** — as a *privileged concept*: the first-class `mode`
  field is neutral per-language data (`L::ModeId`). The latexlike preset defines
  `Mode::{Text, Math}` and needs no `StateExt` flag and no `Event`: its driver's
  `group_interior_delta` initiates the mode change, `finalize_transition` may interpret
  it. The core never asks. (Amended July 2026, Phases 7.1/7.5; formerly sketched as
  `StateExt = { in_math_mode, … }` + events.)
- `ParsingState` is immutable and cheaply shareable; the engine wraps it in `Arc` and creates
  a new one only at transitions, so nodes record their parse-time state
  (SOURCE_ARCHITECTURE.md decision, kept). No `TypeId` maps, no `dyn Any`.

### specs (S1) — specs and libraries  *(updated per Decision 3 resolution — §4b)*

The **callable** concept from PARSING_STRATEGY.md, unified and **de-keyed**: a spec records
*callable behavior*, not the form or name under which it is invoked. The invocation form is
**`Lang::CallableTypeId`** — a closed per-language associated type (amended July 2026,
current-level review; formerly an open id interned in `Language`), like `Lang::GroupTypeId`;
the latexlike preset's enum has `MACRO`, `ENVIRONMENT`, `SPECIALS` variants. Naming rule,
systematic across the crate: `…Kind` = closed core enum, exhaustively matchable (`TokenKind`,
`NodeKind`); `…TypeId` = per-language id type on `Lang` (`GroupTypeId`, `CallableTypeId`).

```rust
/// Behavior of anything invocable from the token stream. De-keyed: carries no name and
/// no invocation form; one spec may back several names (\emph and \textit can share).
/// (Supertraits as shipped: Send + Sync — specs live in parsed trees, which stay Send —
/// plus Debug and Any, the downcast channel for Lang::finalize_node; §3.4, Action 05.)
pub trait CallableSpec<L: Lang>: Debug + Send + Sync + Any {
    // (amended July 2026: an argument IS a parser — pylatexenc's LatexArgumentSpec.
    // ArgumentSpec<L> = { parser: Arc<dyn ArgumentParser<L>>, name, parsing_state_delta },
    // Arc-shared so parsed nodes record which spec each argument was parsed against.
    // See DESIGN_RATIONALE §3.4.)
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>];
    /// Would a bare use as a single-token expression argument be malformed? Default
    /// derives from arguments() (any argument whose parser cannot match empty); a
    /// body-bearing takeover spec (\begin, \verb) overrides to true — the only spec-side
    /// channel for "I take material" (slots session, July 2026; DESIGN_RATIONALE §3.6).
    fn requires_content(&self) -> bool;
    /// Factory: a fresh, single-use construct parser for one invocation of this callable,
    /// ownership moved to the caller (amended July 2026, Phase 6 plan session — was a
    /// stored-parser accessor; DESIGN_RATIONALE §3.6). Default: the standard declarative
    /// parser driven by arguments(). Overriding it is the full-takeover escape
    /// hatch (\verb, tabular preambles, FLM constructs) — pylatexenc's most valuable
    /// extensibility property, preserved (its get_node_parser(token) has exactly this
    /// build-a-parser-for-this-token shape).
    fn make_invocation_parser<'a, 's>(&'a self, invocation: Invocation<'a, 's, L>)
        -> Box<dyn ConstructParser<L, Output = BuildId> + 'a>
    where 's: 'a;   // 's = the source borrow the invocation's trigger token carries
    /// Optional recomposition hook (§nodes level 2) for constructs whose custom parser
    /// records per-instance syntax the default recomposer cannot infer.
    // fn recompose(&self, …) -> …   — default covers declaratively-specced callables
}
```

**Arguments vs. slots.** *Arguments* configure an invocation (each `ArgumentSpec` carries
its parser: group-delimited / optional / marker / custom / …) and are declared spec-side.
*Slots* — a parsed callable's content regions — are **record-level vocabulary only**
(slots session, July 2026; supersedes the earlier spec-side `SlotSpec` list): body parsing
needs invocation facts no declarative list can supply (the `\end{name}` back-reference,
the arguments parsed so far — pylatexenc's `make_body_parser(token, nodeargd, …)`
precedent), so a body-bearing spec's `make_invocation_parser` takeover parses the body and
mints the `ParsedSlot { name, region, ext }` records directly. Terminators — where the
pattern may reference the invocation name (`\end{align}` must match the `align` that
opened; a `---` fence closes with `---`) — are *parser* business, not spec vocabulary
(decided July 2026, Phase 6 plan session): the terminator data parameterizes the core
`EnvironmentBodyParser`, and a body state delta is an ordinary field of the preset spec
type that drives the parse (DESIGN_RATIONALE §3.6). A macro has no slots; an environment
has exactly one (its body); specials usually have none but may have any number (a
fence-block construct with `+++` separators is expressible as a specials callable with
multiple slots). The boundary is a guideline, not a theorem (`\verb`'s delimited content
could be argued either way); the spec decides, and the record machinery underneath is
shared, so nothing breaks structurally either way.

The core ships one standard implementation (`StdCallableSpec`: the argument list as plain
data — nothing else; a parser override is not a field but an implementor overriding the
trait's defaulted `make_invocation_parser` on its own spec type). The familiar
`MacroSpec` / `EnvironmentSpec` / `SpecialsSpec` names
survive as concrete spec types in the preset stratum (S2; landed 7.6 — `EnvironmentSpec`
wraps a dyn `EnvironmentBehavior` carrying the body state delta and body-parser choice) —
"macro" and "environment" are invocation forms, not core concepts.

**Libraries** (supersedes `ContextDb`, per PROPOSALS.md but simplified):

```rust
pub struct CallableQuery<'a, 's, L: Lang> {   // decided July 2026 (Phase 4) — see below
    pub callable_type: CallableTypeId,
    pub name: &'a str,
    pub syntax: CallableSyntax,               // Command { escape_char } / Specials / Other
    pub token: Option<&'a Token<'s, L>>,      // when one exists (pre_space/span context)
}

pub trait SpecLookup<L: Lang> {
    fn lookup(&self, query: &CallableQuery<'_, '_, L>, state: &ParsingState<L>)
        -> Option<Arc<dyn CallableSpec<L>>>;
}

pub struct Library<L: Lang> { /* name; nested BTreeMaps: CallableTypeId → name → Arc<dyn CallableSpec<L>> */ }

pub struct LibraryStack<L: Lang> { /* ordered Vec<Arc<dyn SpecLookup<L>>>, innermost last;
                                      per-CallableTypeId fallback map behind resolve() */ }
```

*(Revised July 2026, Phase 7 plan session — **scope-stack redesign**, superseding the
`SpecLookup`/`Library`/`LibraryStack` sketch above and the fallback bullets below: stack
entries become `Arc<dyn SpecsProvider<L>>` (fallible `retrieve_spec`, specials
participation, functional `with_definitions` updates, best-effort introspection), with
standard impls `Package` (immutable, loaded wholesale, mode-visibility field) and `Scope`
(delta-targeted definitions, copy-on-write); fallbacks are ordinary bottom-of-stack
providers and the stack itself no longer nests; `ParsingStateDelta` grows
definition/stack ops replacing `push_libraries`. `CallableQuery` and innermost-wins
shadowing carry over unchanged. Design: Phase7Execution.md D3; rationale:
DESIGN_RATIONALE.md §3.4. Landed 7.3 — the module is renamed `library` → `scopes`,
the state field/getter to `StateData.scopes`/`scopes()`, and `derived()` became
fallible (`DeriveError`); checkpoint resolutions in the §3.4 landed note.)*

- **Keys are `(CallableTypeId, name)`, many-to-one**: several names may map to one shared
  behavior spec (flyweight). Library keys hold the *normalized* name; the node records the
  *invocation spelling* (§nodes) — the right split given de-keyed specs.
- Resolution: ordered stack, innermost/last-added wins (lexical shadowing — matches
  `\newcommand` semantics and group-local definitions). No `ConflictStrategy` enum: shadowing
  *is* the semantic; an optional lint pass can warn on shadowing if wanted. **[DECISION 6 —
  decided, July 2026: no `ConflictStrategy`. The deferred `SpecLookup` semantics were settled
  in the Phase 4 design session (July 2026): `CallableQuery`-based lookup as sketched above —
  rationale in DESIGN_RATIONALE.md §3.4.]**
- **Unknown callables**: per-`CallableTypeId` fallback policy built into `LibraryStack`
  (consulted by `resolve()` only, after the stack misses; the stack's own `SpecLookup` impl is
  stack-only so nested fallbacks cannot preempt outer definitions), returning a **shared
  singleton** spec — possible precisely because specs are de-keyed (nothing instance-specific
  lives in them). Consequence: a callable node's spec is **never `None`** for any callable
  type whose preset registered a fallback.
- **Mode-aware lookup without privileged modes**: `lookup()` receives `&ParsingState<L>`, so a
  preset's `SpecLookup` implementation may dispatch on `state.ext` (e.g. FLM resolving `\vec`
  differently in math mode). The core `Library` type ignores the state. This replaces
  PROPOSALS.md's hard-coded `math_mode_macros` tables, which contradicted the
  no-privileged-modes principle.
- Extending definitions mid-parse = pushing a `Library` onto the stack via
  `ParsingStateDelta::push_libraries`; popping happens naturally because the previous
  `Arc<ParsingState>` is restored when the group ends.

### nodes (S1)  *(Decision 3 — RESOLVED, July 2026; evolution recorded in §4b)*

Flat, frozen, index-based storage (SOURCE_ARCHITECTURE.md), with a **unified callable kind**
and a **two-tier ext system**:

```rust
pub struct NodeTree<L: Lang> { nodes: Vec<NodeData<L>> }

pub struct NodeData<L: Lang> {
    kind: NodeKind<L>,
    ext:  L::NodeExt,                 // tier 1: uniform, on every node (bindings handles, IDs, …)
    span: SourceSpan,                 // Arc<Source> + byte range — provenance
    parsing_state: Arc<ParsingState<L>>,
    children: Range<u32>,             // structural children in the same Vec
}

pub enum NodeKind<L: Lang> {
    Chars    { content: TextContent, ext: L::CharsNodeExt },
    // amended July 2026: delimiters stored on the node (pylatexenc's `delimiters`);
    // group_type: Option<L::GroupTypeId> (None = internal synthesized group).
    Group(Box<GroupData<L>>),
    Callable(Box<CallableData<L>>),   // boxed: Chars dominates the vec; keeps the enum small
    Comment  { content: TextContent, ext: L::CommentNodeExt },  // text sans delimiter/newline
    List     { ext: L::ListNodeExt },
}

pub struct CallableData<L: Lang> {
    pub callable_type: L::CallableTypeId, // invocation form: latexlike MACRO / ENVIRONMENT / SPECIALS
    pub name: Box<str>,                  // invocation spelling; identity ⇒ always owned
    pub spec: Arc<dyn CallableSpec<L>>,  // behavior; shared, de-keyed; never None (§specs fallback)
    pub arguments: ParsedArguments<L>,   // pylatexenc pattern (July 2026): entries carry their
                                         // Arc'd ArgumentSpec + child region (noise + syntax
                                         // + designated content range) + ext — regions session
    pub slots: ParsedSlots<L>,           // 0..n content regions (environment body = 1 slot)
    pub post_space: TextContent,         // reproduced verbatim in recomposition
    pub ext: L::CallableNodeExt,         // tier 2: per-kind, per-instance parse results
}
```

**No `Macro`/`Environment`/`Specials`/`Math`/`Custom` variants.** The structural taxonomy is
exactly PARSING_STRATEGY.md's concept list — chars, groups, callables, comments — and
"environment" is not a core concept: "is this an environment" =
`node.callable_type() == latexlike::CT_ENVIRONMENT` (honest two-level dispatch). `$…$` parses
as a `Group` whose class (`GroupTypeId`) is the preset's math-group class, under the preset's
math-mode state ext; the preset provides `NodeRef` accessor helpers so ergonomics don't suffer. Exhaustive pattern
matching, no downcasting, no `clone_box`, trivially serializable.

**`TextContent` — logical content as first-class data; the span is provenance:**

```rust
pub enum TextContent {
    Spanned(Span),     // range into the node's own source (parser output — zero-copy)
    Owned(Box<str>),   // synthesized, transformed, or normalized content
}
```

Accessors (`NodeRef::chars()`, `::comment()`, `::post_space()`) return `&str` either way;
equality and serialization operate on the logical text. Invariant (builder-enforced,
debug-asserted): `Spanned` refers into the source of the node's own `SourceSpan`; a transform
that replaces a node's span materializes its content first. `NodeTree::materialize()` returns a
**new** tree with every `TextContent` owned (trees stay immutable); source-detaching variants
exist as a concept but are deliberately deferred and de-emphasized.

**Division-of-labor rule (load-bearing):**
- **Library key** `(CallableTypeId, normalized name)` — resolution.
- **Node** — invocation facts: form, spelling, parsed args/slots, `post_space`, per-instance ext.
- **Spec** — shared behavior, stored once (`\ref`-ness lives here, not per node).
- **Parsing state** — context at parse time (math mode, active rules).
- **Uniform `NodeExt`** — cross-cutting per-instance concerns (e.g. a bindings wrapper handle;
  interior-mutable set-once types are the sanctioned idiom inside frozen trees).
- **Ownership rule:** identity (names) is always owned; textual content is `TextContent`.

**Ext plumbing:** the ext types are bundled behind one associated type
(`Lang::NodeExts: NodeExtTypes`) to keep `Lang` small; a `SimpleLang` convenience trait with a
blanket `impl<T: SimpleLang> Lang for T` defaults them all to `()` (associated-type defaults
being unstable). Bounds kept minimal (`Clone + Debug` only where transforms clone nodes) and
spec/parser interfaces kept dyn-safe — deliberate bindings-readiness rules (Python/JS wrappers
must not be obstructed). Layout discipline: keep ext types word-sized (an index or `Arc` into
Lang-owned storage); `CallableData` is boxed so the enum stays small for the dominant `Chars`
case.

**Whitespace and span invariants** (pinned July 2026, Phase 6 plan session — full numbered
statement in DESIGN_RATIONALE.md §3.5):
- A callable's `post_space` is **exactly its trigger token's own syntactic post-space** —
  the name-terminating whitespace of a multi-character command, stopping at any paragraph
  break; nothing beyond the token is ever claimed (amended July 2026, Phase 6.4, user
  decision — supersedes the claim-helper rule; whitespace after `\&` or after a final
  argument is ordinary sibling/region content, as in TeX/pylatexenc). Whitespace elsewhere
  between constructs becomes a whitespace-only `Chars` node (pylatexenc behavior). No
  double counting ⇒ **sibling spans partition the parent's content interior exactly** — no
  gaps; span math trustworthy for tooling and editing. Mechanically checkable via the test
  utility `check_tree_invariants()`.
- A callable's `SourceSpan` **includes** its `post_space` (a `Spanned` post_space is a
  sub-range of the node's own span — trailing for zero-argument callables; between the name
  and the first argument region otherwise). Node span semantics are the public contract and
  are deliberately decoupled from token behavior — tokens are transient engine internals.
- Paragraph breaks surface as their own nodes via `Lang::make_paragraph_break_node`
  (default: whitespace-only `Chars` over the full token span; a preset may return a
  callable-shaped kind — the "specials representation" option survives as this hook).

**Recomposition requirement** (constrains `ParsedArguments` — formerly `ArgsLayout`):
- *Level 1 — verbatim:* a node's own `SourceSpan` → exact original text. Never needs an
  external lookup (the Arc travels with the node); works for detached and mixed-origin trees.
- *Level 2 — Lang-aware quasi-equivalent:* recompose from `(callable_type, name, args, slots,
  post_space, children)` plus the `Language` registries (group/callable type → delimiters and
  invocation form), reproducing parsed text rather than guessing. "Quasi-equivalent" =
  re-tokenizes and re-parses to an equivalent tree under the same `Language` (the validity
  criterion — licenses e.g. inserting a separating space where required). Consequence: **the
  invocation parser must record per-instance syntax choices the spec doesn't determine**
  (optional-arg presence, matched delimiter alternative, chosen verbatim fence, star,
  inter-argument noise) — as ordinary nodes in the argument/slot child regions where textual
  (July 2026 regions session: markers, pre-argument whitespace, and comments are region
  nodes; `ParsedArguments` records the regions + designated content ranges), on `GroupData`
  for group delimiters, *not* in ext: recomposability must not depend on Lang cooperation
  (group delimiters therefore live on the node, July 2026). **Rigid spec-determined syntax
  is the one sanctioned exception** (July 2026, Phase 6 plan session): environment
  `\begin{name}`/`\end{name}` scaffolding is deliberately inflexible (no comments/newlines
  before the name group; unrecorded inline whitespace normalized away) and is
  *reconstructed*, not recorded — deterministic, hence still "reproduce, don't guess"
  (DESIGN_RATIONALE §3.5). Exotic custom parsers use the `CallableSpec::recompose()` hook.

**Two-phase region records — a deliberate runtime invariant (July 2026 regions session).**
`ParsedArguments`/`ParsedSlots` entries hold child *regions* (noise + syntax + designated
content nodes) as ranges of **global node indices** in the final flat layout — which does not
exist while parsers run. Records are therefore staged in `BuildId` coordinates and resolved in
place by `NodeTreeBuilder::finish()`. This is the accepted "honest cost": a phase the type
system can't see, contained because resolution happens in one component at one point, finished
trees cannot contain staged records, and resolved-only accessors panic on staged ones. It buys
the important property that construct parsers build `ParsedArguments` directly with an
unchanged builder API. Full argument: DESIGN_RATIONALE.md §3.5.

Access is only through `NodeRef<'pr>` proxies as designed in March (Copy, resolves indices,
`span_content()`, `children()`, `parsing_state()`, `name()`, `arguments()`, `slots()`,
`body()`, …). Trees are immutable after `ParserSession::finish()`; transformations build new
trees (Arc-shared sources/specs/states make mixed-origin trees free — including *synthetic*
callables and chars nodes, which owned names and `TextContent::Owned` make possible without
fabricating sources). The concrete transformation/visitor APIs are deliberately still
undesigned (post-Phase-6); post-processing may equally produce non-tree outputs (HTML, JSON,
analysis results) by walking the tree via `NodeRef`.

**Indices are `u32` internally (hardcoded).**  Keep u32 because it's the settled ecosystem
choice for arena indices (rustc, cranelift, la-arena) and the id space can't overflow in
practice — memory runs out long before 2^32 nodes. The newtype's private field is the real
abstraction boundary: representation stays out of the public API (index() -> usize), so
changing width later is a small, confined diff, and a typedef or generic parameter would add
indirection or API complexity for flexibility nobody realistically uses. Users who need
narrower ids can compress at their own boundary via the usize seam, and the one safeguard
that matters is a checked usize → u32 conversion at the single mint site.

### constructs (S1) — construct parsers

The single most important trait in the system. To avoid pylatexenc's three-argument threading
(`walker, token_reader, parsing_state`), everything a parser needs rides in one context:

```rust
pub struct ParseContext<'a, 's, L: Lang> {
    pub tokens: &'a mut dyn TokenReader<'s, L>,
    pub source: Arc<Source<L::SourceOrigin>>, // what staging a SourceSpan requires; lives here
                                              //   because tokens/readers deliberately carry only
                                              //   byte spans (added 6.4, user-approved)
    pub state: Arc<ParsingState<L>>,
    pub session: &'a mut ParserSession<L>,   // node building, diagnostics, Recovery policy
}

// lang-first like TokenResult (6.1 adjustment); ParseError is origin-generic
pub type ConstructParserResult<L, T> = Result<T, ParseError<<L as Lang>::SourceOrigin>>;

pub trait ConstructParser<L: Lang> {
    type Output;                              // BuildId, Vec<BuildId>, ParsedArguments, …
    fn parse(&mut self, cx: &mut ParseContext<'_, '_, L>)
        -> ConstructParserResult<L, (Self::Output, Option<ParsingStateDelta<L>>)>;
}
```

*(Amended July 2026, Phase 7 plan session: `ParseContext` gains `driver: &'a L::Driver`
— the `ParseDriver`, home of construct-parser provision, descent deltas, and the
`Recovery` policy (moved off `ParserSession`); descent call sites route through
`cx.parse_nodes(…)`/`cx.parse_group(…)`. See the §engine note; DESIGN_RATIONALE.md §3.6.)*

Construct parsers are **temporaries** (July 2026, Phase 6 plan session): constructed with
their per-use configuration, `&mut self` so working state lives in fields, dropped when the
frame ends — never stored in specs (two-tier ownership model, DESIGN_RATIONALE §3.6).
`Err` means abort; recovery happens at the detection site and abnormal endings travel as
`StopCause` data (DESIGN_RATIONALE §3.8).

**Dispatch is by token kind + library lookup — never by parser registry scanning.** The main
loop (`NodesParser`, pylatexenc's `LatexGeneralNodesParser` + nodes collector):

```
loop:
  tok = tokens.peek(state)
  match tok.kind:
    Char            -> accumulate chars node (whitespace-only chars nodes from pre_space, §nodes)
    ParagraphBreak  -> own node via Lang::make_paragraph_break_node (default: whitespace Chars)
    GroupOpen(rule) -> group parser (derived state expecting_group_close; recurse NodesParser)
    Comment         -> comment node built directly from the token (whole-comment tokens)
    Command(name)   -> Lang::resolve_command (typically via libraries; Unresolved ->
                       diagnose+recover, its optional detail riding the diagnostic)
                       -> spec.make_invocation_parser(invocation).parse(cx)
    Specials(..)    -> make_invocation_parser likewise (resolution — type + spec — on the token)
    GroupClose(t)   -> stop-condition match? stop : StopCause::UnexpectedGroupClose (caller decides)
    EndOfStream     -> materialize trailing-whitespace chars node from pre_space; stop
  returned delta -> session.derived_state(&state, &delta) -> new Arc<ParsingState> for subsequent
                    siblings (session-mediated: the memo/observation seam — amended July 2026,
                    DESIGN_RATIONALE §3.6; ParsingState::derived stays the underlying pure transition)
NodesParser returns (nodes, StopCause) — the caller interprets the ending (§errors).
```

Everything the Sonnet doc wanted from `can_parse()`/`priority()` registries is achieved
data-first: custom syntax enters via (a) specials strings in `TokenRules` + a `SpecialsSpec`,
(b) macro/environment specs with custom invocation parsers, (c) new group types, or — the
nuclear option — (d) a custom `TokenReader` or a custom nodes parser. Deterministic, no
priority races.

Standard construct parsers to implement (mirroring pylatexenc's `latexnodes.parsers`;
list revised July 2026, Phase 6 plan session): `NodesParser` (stop conditions: reified
token-condition enum + tier-2 programmatic predicates; returns `StopCause`), the group
parser, `StdInvocationParser` (the default declarative invocation parser: argument specs →
argument parsers → regions, slots; post_space = the trigger token's own — 6.4 amendment),
the standard `ArgumentParser`
implementations (delimited-group / optional-group / marker / expression fallback — core,
parameterized by group types; the preset one-liner constructor is 7.7's
`latexlike::argument_specs`),
`EnvironmentBodyParser` (core, parameterized: stop-command name, name-group type,
invocation-name back-reference), `ExpressionParser` (single node — `\frac12` acceptance),
and the verbatim family (7.7: `verbatim_state_delta`, `VerbatimArgumentParser`,
`VerbatimBodyParser` — raw regions per the pinned features-off + expected-close recipe).
(No `CommentParser`:
whole-comment tokens made it vestigial — comment nodes come straight from tokens.)

### engine (S1) — orchestration

Per SOURCE_ARCHITECTURE.md, with one renaming: `FLMEnvironment` collides fatally with
`EnvironmentSpec`/`EnvironmentNode`. **[DECISION 2 — decided, July 2026]**:

```rust
pub trait Lang: Sized {                 // the compile-time bundle (was: LanguageSpecification)
    type StateExt:  Clone + Debug + Default;
    type SessionExt: Debug + Default;   // parse-global MUTABLE extension on ParserSession —
                                        // counters, parse-global caches (amended July 2026,
                                        // DESIGN_RATIONALE §3.6 two-level transition doctrine)
    type Event:     Clone + Debug;      // semantic transition events (e.g. FLM's EnterMath)
    type NodeExts:  NodeExtTypes;       // bundle: uniform NodeExt + per-kind <Kind>NodeExt (§nodes) [Phase 5]
    type SourceOrigin: …;

    /// Transition customizer — the choke-point hook of §state. PURE constructor of state
    /// data (what makes derivations memoizable); runs once per unique derivation.
    /// Default: empty.
    fn finalize_transition(new: &mut StateData<Self>, prev: &ParsingState<Self>,
                           events: &[Self::Event]) {}

    /// Per-EVENT transition observation (amended July 2026, DESIGN_RATIONALE §3.6):
    /// called by ParserSession::derived_state on every transition event, memo hits
    /// included — parse-history accumulation lives here, never in finalize_transition.
    /// Observational only: cannot alter the resulting state. Default: no-op. [6.3]
    fn observe_transition(ext: &mut Self::SessionExt, prev: &ParsingState<Self>,
                          new: &ParsingState<Self>, delta: &ParsingStateDelta<Self>) {}

    /// Specials scan (§token): recognition + resolution in one call — the match carries
    /// name AND spec. Typically dispatches to the state's libraries. Default: no specials.
    fn scan_specials<'s>(state: &ParsingState<Self>, content: &'s str, pos: usize)
        -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> { Ok(None) }
    /// Hot-path filter for scan_specials, cached per state (§token). Default: none.
    fn specials_trigger_chars(data: &StateData<Self>) -> TriggerChars { … }

    // Phase 6 plan session hooks (July 2026; DESIGN_RATIONALE §3.6):

    /// Resolve a Command token to (callable_type, spec). Typically dispatches to the
    /// state's libraries via CallableQuery. Returns Resolved / Unresolved { detail } —
    /// the optional detail (why resolution failed: "searched libraries x, y, z", "load
    /// the {amsmath} library") rides the unresolvable-command diagnostic. Default:
    /// Unresolved with a "command resolution is not implemented by this language"
    /// detail (amended July 2026; DESIGN_RATIONALE §3.6).
    fn resolve_command(state: &ParsingState<Self>, token: &Token<'_, Self>)
        -> CommandResolution<Self> { … }
    /// Node kind representing a paragraph break; the core stages it with the token's
    /// span. Default: whitespace-only Chars over the full token span.
    fn make_paragraph_break_node(state: &ParsingState<Self>, token: &Token<'_, Self>)
        -> NodeKind<Self> { … }
    /// Centralized node finalization: run by NodeTreeBuilder::add for EVERY staged node
    /// (all kinds — presets attach spec-derived/uniform ext here; must be idempotent,
    /// transforms re-stage nodes). Default: no-op.
    fn finalize_node(/* &mut kind/ext, span, state, children + staged read view */) {}
}

pub struct Language<L: Lang> {          // the runtime bundle (was: FLMEnvironment)
    driver: L::Driver,                          // the ParseDriver instance
    initial_state: Arc<ParsingState<L>>,        // frozen seed (from Lang::initial_state_data,
                                                //   customized via with_seed_delta)
    resolver: Arc<dyn SourceResolver<L::SourceOrigin>>,  // default NoResolver
}

impl<L: Lang> Language<L> {             // landed shape (Phase 7.4, July 2026)
    pub fn parse(&self, content: impl Into<String>) -> Result<ParseResult<L>, ParseError<…>>;
    pub fn parse_source(&self, source: Arc<Source<…>>) -> Result<ParseResult<L>, ParseError<…>>;
    // advanced path = accessors (no session(): ParserSession is Language-independent
    // scratch, ParserSession::new() is the entry):
    pub fn initial_state(&self) -> &Arc<ParsingState<L>>;
    pub fn driver(&self) -> &L::Driver;
    pub fn resolver(&self) -> &Arc<dyn SourceResolver<…>>;
    pub fn resolve_source(&self, reference: &str, triggered_at: &SourceSpan<…>)
        -> Result<Arc<Source<…>>, ResolveError>;   // feeds parse_source
}
```

*(Phase 7.4 amendment, July 2026: the sketch above shows the landed surface — two named
entry methods replace the never-designed `SourceInput` conversion enum, and the advanced
path is accessors rather than a `session()` method. Root drive loop: `cx.parse_nodes`
under no stop condition, diagnose-and-skip `StrayGroupClose` on a root-level stray close,
root `List` over the entire source. DESIGN_RATIONALE §3.6.)*

*(Phase 6 amendment, July 2026: `Language<L>` itself and the `parse()` convenience entry
point are **deferred** — Phase 6 ships `ParserSession` (builder + diagnostics + `Recovery`)
as the root object, driven directly; `Language` arrives with the phase that demonstrates
the need, Phase 7 at the earliest. Type-id interning stays deferred with it.
DESIGN_RATIONALE §3.6.)*

*(Phase 7 plan session amendment, July 2026: `Language<L>` + `parse()` are **scheduled for
Phase 7**. `Lang` gains `type ModeId` (§state) and `type Driver: ParseDriver<Self>`; the
parse-time hooks sketched above — `resolve_command`, `make_paragraph_break_node`,
`observe_transition`, plus `refine_diagnostic` — migrate to the **`ParseDriver`**
(instance methods; also home of construct-parser provision via
`make_nodes_parser`/`make_group_parser`/`make_invocation_parser` interception, the
`group_interior_delta` descent channel, and the `Recovery` policy, moved off
`ParserSession`). `Lang` keeps the state-, tokenizer-, and builder-layer hooks
(`initial_state_data`/`finalize_transition`, `scan_specials`/`specials_trigger_chars`,
`finalize_node`). Design: Phase7Execution.md D1/D2; rationale: DESIGN_RATIONALE.md
§3.3/§3.6.)*

**Stratum note** (July 2026): the `Lang` trait and its bound-trait `NodeExtTypes` are
*documented* here as the compile-time bundle, but their *definitions* live in the S1 core next
to the state types — `finalize_transition` names `StateData`/`ParsingState`, which fixes their
home; and `NodeExtTypes` stays with `Lang` rather than in `node/`, even though its meaning is
a node concern (moving it there would recreate a cycle for cosmetics). Only `Language<L>` —
the runtime bundle — is genuinely an orchestration type. Its content reaches the lower topics
by **seeding, not dependency**: at session start it constructs the initial
`Arc<ParsingState<L>>` from its defaults (default `TokenRules`, base libraries, default
`StateExt`); from then on the token loop reads the materialized state and never consults
`Language`.

“Define a language once, parse many documents in it” — `Language<L>` owns no per-parse state.
`ParserSession` (transient) builds the `NodeTree`, creates `Arc<Source>`s (resolver,
synthesized sources), collects diagnostics; `finish()` freezes into `ParseResult`.

**Ownership/lifetime model** (March 2026; amended by Phase 6/7.4): `Language<L>` is long-lived
and user-controlled. *The `'env` borrows the March model gave `ParserSession`/`ParseResult` were
dropped when they landed (Phase 6, kept at 7.4): sessions are Language-independent scratch, and
`ParseResult` owns its tree and diagnostics with no `Language` reference — nodes are
self-contained (Arc-wrapped spans, specs, and states), so spec data stays available during AST
analysis from the nodes themselves.* `NodeRef<'pr>` borrows the result. Lifetime parameters stop
at these proxy/result types — node data itself carries **none**, which is precisely what lets
transformed trees outlive the `ParseResult` they came from, and lets sources be freed only when
the last `SourceSpan` in any tree drops.

### Errors and tolerant parsing

- Zero-dep hand-written error types (zero-dep at *runtime*; declaration boilerplate is
  generated by `techy-derive`, below). Every error carries a `SourceSpan` (not a lifetime-bound
  `SourceLocation<'src>` — Arc spans remove the `'src` infection that currently spreads through
  `error.rs`, `Result<'src, T>` etc.).
- Keep and formalize the WIP recovery mechanism: `TokenError { …, recovery: Option<RecoveryToken> }`;
  the session's `Recovery` policy (strict / tolerant) decides whether to record a diagnostic and
  continue with the recovery token or abort. Diagnostics accumulate in the session and are
  available on `ParseResult` even for successful tolerant parses.
- Rich human-readable rendering (line/col via `LineIndex`, provenance chain via
  `SourceProvenance`, open-blocks traceback — the existing `format_traceback` work slots in
  here).

**Condition declaration via derive** *(July 2026 — DESIGN_RATIONALE.md §3.8)*

- `techy-derive` proc-macro sub-crate (syn 2 + quote; **build-time dependency only**, runtime
  stays zero-dep), re-exported from `techy::error` — third-party conditions use the identical
  surface.
- `#[derive(DiagnosticInfo)]` on a condition struct generates:
  - the `DiagnosticInfo` impl, `IDENTIFIER` taken from a **mandatory** `#[diagnostic(id = "…")]`
    (never derived from the type name);
  - a `Display` impl from an **optional** `#[diagnostic(message = "…{field}…")]` format
    string — omitted for conditions whose wording needs a match, conditional, or format cast,
    which hand-write `Display`;
  - the `new()` constructor, uniform `impl Into<FieldType>` parameters (companion of
    `#[non_exhaustive]`; `no_constructor` opt-out for bespoke signatures);
  - `serializable_data()`, mapping every field through `ToDiagnosticValue`, keyed by field
    name.
- `ToDiagnosticValue` (error.rs): the serialization bridge into `DiagnosticValue`, implemented
  for the closed set of payload primitives (bool, integers, `String`/`&str`, `char`, `Option`,
  `Vec`/slices). Any other field type fails the derive with a field-spanned bound error — the
  compiler enforces that every payload field is serializable.
- `#[derive(ToDiagnosticValue)]` for field-less payload enums (`MissingTerminatorFound`, …):
  serializes as the kebab-cased variant name; future per-variant rename attribute for wire
  headroom.
- Hand-written per condition, deliberately: the struct with doc comments, the derive list,
  `#[non_exhaustive]`.

---

## 4. How Decision 1 was resolved (facet traits → A → B → C)

Recorded so the reasoning survives; the resulting design is §state. Four candidates were compared
(discussion of July 2026):

**Per-facet traits (the WIP in an earlier source tree)** — each tokenization facet behind its own trait
+ macro-generated data struct, nine associated types on `ParsingStateTrait`. Rejected: the
facet traits expose only *getters*, so a library-authored standard delta ("set comment start
to `;`") cannot be implemented generically — the associated types are opaque, with no update
contract. Fixing that means adding builder methods to nine traits; the granularity multiplies
generics without adding power over a single state abstraction.

**Option A — concrete state + per-getter `Lang` hooks** for computed settings (e.g. escape
char derived on the fly from math mode). Supports the key use case cheaply, but rejected: the
hooks *patch* the default storage model rather than offering one clean extensible model.

**Option B — whole state behind `L::State: ParsingStateModel`** (trait contract = getters +
standard-delta application; robust storage-based default implementation). Maximally flexible,
but the costs compound: an engine-owned wrapper is needed anyway (derived caches like the
prefix table can't live in the model); the trait needs *laws* (getter purity — load-bearing
for caching; delta locality; a stored-vs-effective semantic so `SetEscapeChar` has defined
meaning under computed values); compound getters need `Cow`-shaped returns because computed
collections have nothing to borrow from; cross-library access to non-standard state needs
capability traits; "default plus one tweak" costs a dozen delegated methods; and
debuggability suffers — effective state is latent in code, so `dbg!(state)` lies.

**Option C — materialized state + transition choke point** (ADOPTED). The deciding question:
*does any effective setting ever change between transitions?* No — by the architecture's own
definition, tokenization behavior is a function of the state, so any change **is** a
transition. Computed-per-read therefore buys no semantic power over recompute-at-transition;
`Lang::finalize_transition` preserves the real benefit of computed getters (cross-cutting
rules centralized in the language definition instead of smeared across delta writers) while
keeping plain-data field reads on the hot path and full state inspectability. What C gives up
versus B is swappable storage layout — speculative for FLM, and recoverable: since the public
read surface is getters over private fields, a model trait could later be introduced *behind
the same getters* without breaking consumers. C keeps B's door open; the reverse is not true.
C is also structurally closest to pylatexenc's battle-tested design (`ParsingState` fields +
derived sub-states with changed kwargs); `finalize_transition` is the generalization
pylatexenc lacked.

A simplification fell out of the follow-up `copy_with` discussion: there is **no mutating
"apply" mechanism and no `StateDelta` trait**. The reified change value
(`ParsingStateDelta<L>`, a struct of optional overrides + events) *is* the argument of
`derived()`; standard changes and semantic events travel in one mergeable, inspectable value.
The change-description must remain a *value* (not a closure, not a direct constructor call)
because producer and scope-decider differ — see the producer/scope split in §state.

What survives from the earlier WIP: the cached sorted delimiter-prefix table
(`cached_prefix_strings`, with the open/close-ambiguity merging — good work, keep it), the
`detect_*` decomposition of the tokenizer (as private methods of `StdTokenReader`), and the
recovery-token mechanism.

---

## 4b. How Decision 3 was resolved (`Custom` variant → unified `Callable` + two-tier ext)

Recorded so the reasoning survives; the resulting design is §specs/§nodes (discussion of July 2026).

- The original proposal — closed structural enum + `Custom(L::NodeData)` variant — conflated
  two needs: *extra per-instance data on a node that IS structurally a group/macro/…* (the
  common case) and *genuinely new structural shapes* (rare; no concrete example survived
  scrutiny — custom constructs are still invocation-, group-, or leaf-shaped). Making `Custom`
  a *sibling* of the structural variants meant attaching data destroyed structural identity: a
  group with custom data stopped being a group to all generic tooling. Rejected alternatives:
  annotation wrapper nodes (re-create the problem one level up) and side tables (break node
  self-containment across tree transforms).
- **Resolution:** ext data orthogonal to structure, in two tiers — uniform `NodeExt` on every
  node plus per-kind `<Kind>NodeExt` — and no `Custom` variant at all. Made affordable by
  **merging Macro/Environment/Specials into one `Callable` kind** (they differ by invocation
  form, not by parsed shape), itself a de-privileging move: "environment" was a preset concept
  wrongly enshrined as a core node kind. The node taxonomy is now isomorphic to
  PARSING_STRATEGY.md's concept list.
- The merge required recording the invocation form somewhere ⇒ interned **`CallableTypeId`**
  (registry pattern shared with `GroupTypeId`; "namespace" was rejected as confusable with
  package/library), which also became the library key space and the per-form unknown-fallback
  hook. Specs were **de-keyed** (behavior only, no name), enabling flyweight sharing across
  names and shared-singleton unknown-specs — so a callable's spec is never `None` with zero
  per-instance allocation.
- **Names are owned** (`Box<str>`): identity-bearing, and span-backed names would force
  synthetic nodes (transforms creating callables — FLM's bread and butter) to fabricate
  sources. The same argument generalized to content fields ⇒ **`TextContent`** (span-backed
  when parsed, owned when synthesized/normalized), which also made normalization representable
  and level-2 recomposition self-contained. `post_space` is kept and reproduced verbatim
  (reproduce, don't guess); the whitespace-as-chars-nodes rule (pylatexenc) restores the exact
  sibling-span partition invariant. Args vs. slots kept as two named concepts over shared
  machinery (macro = args only; environment = one body slot; fence-block specials = several
  slots) — the boundary is a spec-owned guideline, not core law.

---

## 5. Generics strategy

Generic (via the single `L: Lang`):
- `StateExt` — language-specific parsing state (math mode etc.). Typed, no `Any`.
- `Event` — semantic transition events, consumed by `Lang::finalize_transition` (§state).
- `NodeExts` — bundle of node ext types: uniform `NodeExt` (every node) plus per-kind
  `CharsNodeExt` / `GroupNodeExt` / `CallableNodeExt` / `CommentNodeExt` / `ListNodeExt` (§nodes).
- `SourceOrigin` — origin metadata type.

Defaults: `Lang` for the latexlike preset is a ZST (`Latexlike`), and type aliases
(`type LatexParseResult = ParseResult<Latexlike>` …) keep simple usage generics-free.

Deliberately **not** generic (for now):
- **Shared pointer (`Rc` vs `Arc`)** — the `SharedPointer` GAT in SOURCE_ARCHITECTURE.md §Generics
  would infect every signature in the crate for a micro-optimization (uncontended atomic clones
  are ~1ns, and Arcs are cloned once per node, not per byte). Proposal: use `Arc` behind an
  internal alias `pub(crate) type Shared<T> = Arc<T>` so a later swap (or a later GAT layer, or
  a cargo feature) is mechanical. **[DECISION 4 — decided, July 2026]**
- Spec types — extensibility comes from `CallableSpec` being a trait; no need for `Lang` to name
  concrete spec types.
- Content backing is a plain `String` on `Source` (the once-planned `SourceContent` trait
  was retired July 2026 as information-equivalent to `&str` — a UTF-8 mmap can be handed
  in as text by the embedder; DESIGN_RATIONALE §3.1. The old mmap feasibility notes remain
  in `dev-docs/archive/SOURCE_ARCHITECTURE.md`).

---

## 6. FLM fit check

FLM (goal 3) maps onto this architecture as a separate crate, `flm`, with:

- `struct Flm; impl Lang for Flm { type StateExt = FlmState /* math mode, … */; … }`
- FLM feature/environment definitions = `Library<Flm>` collections; FLM's `\begin{align}…`,
  citations, refs, floats = `EnvironmentSpec`/`MacroSpec` with custom invocation parsers where
  needed.
- FLM's render pipeline = post-processing over `ParseResult<Flm>` via `NodeRef` traversal
  (immutable trees, Arc-self-contained nodes — exactly what SOURCE_ARCHITECTURE.md's
  post-processing section was designed for).
- FLM's "environment" (in the pylatexenc-FLM sense: the configured processing setup) =
  `Language<Flm>` plus FLM's own render config on top.
- `\input`-like content resolution = `SourceResolver`; synthesized/expanded content =
  `Synthesized` provenance.

Nothing in FLM appears to require a capability the core lacks; the two things to watch during
implementation are (a) argument-level custom parsers being able to *re-enter* the general nodes
parser with modified state (covered: `ParseContext` + deltas), and (b) node payloads rich enough
for FLM semantic info (covered: per-kind ext types + uniform `NodeExt` + specs being
FLM-defined).

---

## 7. Naming (deltas to NAMING_STRATEGY.md)

Keep all existing decisions (no `Latex` prefixes, …; note July 2026 revisions: `ParsedArguments`
over `Arguments`, spec argument lists over `ArgumentStructureSpec` — NAMING_STRATEGY.md), plus:

| Concept | Name | Replaces |
|---|---|---|
| Compile-time type bundle | `Lang` (trait) | `LanguageSpecification` (too long for a parameter appearing everywhere) |
| Runtime config bundle | `Language<L>` | `FLMEnvironment` (collides with LaTeX environments) |
| Tokenization data | `TokenRules` (stored in `StateData<L>`) | `TokenizationState` / facet traits |
| State change value | `ParsingStateDelta<L>` (overrides-struct + events) | `StateDelta` trait, `StandardDelta` enum |
| State transition | `ParsingState::derived()` | `apply()` / `copy_with()` (adjective form per Rust's `to_uppercase` convention; signals a *transition*, not a field copy) |
| Definition db | `Library`, `LibraryStack`, `SpecLookup` | `ContextDb`, `LibrarySet` (+ dropped `ConflictStrategy`) |
| Construct parser trait | `ConstructParser` | `Parser` trait / `Parsing` (avoids clash with any high-level parser type) |
| Byte range | `Span` (Copy, no Arc) | — |
| Arc-carrying range | `SourceSpan` | `SourceLocation<'src>` |
| Invocation-form registry | `CallableTypeId` (interned in `Language`, like `GroupTypeId`) | "namespace" (confusable with package/library), `CallableKind` |
| Registry naming rule | `…Kind` = closed core enum; `…TypeId` = open `Language`-interned registry | — |
| Node textual payload | `TextContent` (`Spanned` / `Owned`) | owned-`String` fields, pure-span content |
| Node ext types | `NodeExt` (uniform) + `CharsNodeExt`, `GroupNodeExt`, `CallableNodeExt`, … (bundled as `Lang::NodeExts`) | `GroupExt` (too vague), `NodeGroupExt` (wrong parse) |
| Token-level command syntax | `TokenKind::Command`, `CommandRule` | `Macro` token kind, `MacroRules` ("command" per TeX lineage; scales to future non-escape syntaxes, unlike "escape") |
| Comment syntax rule | `CommentRule` (start string; end-of-line terminated) | `CommentRules` |
| Paragraph-break flag | `TokenRules::enable_multi_newline_paragraphs` | `paragraph_breaks`, `double_newline_paragraphs` (renamed July 2026: any run of 2+ newlines is one break; joined the `enable_*` family July 2026, DESIGN_RATIONALE §3.2) |
| Specials recognition | `Lang::scan_specials` → `SpecialsMatch` (name + spec); `TriggerChars` filter | `TokenRules::specials` string list |
| Whitespace primitive | `skip_whitespace` (never consumes a `\n\s*\n` newline) | per-call-site inline logic |

The high-level entry point is `Language::parse()`; whether a convenience `Parser` struct still
exists on top is a bikeshed we can defer.

---

## 8. The latexlike preset (S2)

A module (`techy::latexlike`), not a separate crate. Core landed in Phase 7.5 (checkpoint
decisions in DESIGN_RATIONALE §3.13); items are namespaced, not re-exported at the crate
root. It provides:

- `Latexlike` ZST implementing `Lang` with the three closed vocabularies (bare,
  module-scoped, `#[non_exhaustive]` enums): `GroupType` (`Content`/`Math` — a *single*
  math class; inline vs. display is a delimiter fact read by the `NodeRef::math_style()`
  sugar), `CallableType` (`Macro`/`Environment`/`Specials`), `Mode` (`Text`/`Math`).
  `StateExt = ()` — the first-class `mode` field is the single source of truth.
- `LatexlikeDriver`: the recovery knob, scope-stack `resolve_command` (miss detail =
  searched providers), and the math plug (`group_interior_delta` → mode override for
  math-class rules).
- Default `TokenRules` (`default_token_rules()`): `\` escape, `{}` content groups,
  `$ $$ \( \[` math groups, `%` comments. `[]` is deliberately **not** a group type —
  plain characters; optional arguments recognize them via per-spec `temporary_groups`
  rules (7.5 checkpoint).
- The seed package `"base"` (`base_package()`): pylatexenc's default specials as data
  (`&`, `~`, ligatures ``` `` ``` `''` `--` `---` `` !` `` `` ?` ``), plus the environment
  dispatch pair `begin`/`end` (`BeginSpec`/`EndSpec` — ordinary macro entries, 7.6).
  Everything else — common macros/environments/accents, `\newcommand` producing
  definition deltas (parse-level only, no TeX expansion) — waits for the standard
  spec-database phase.
- The preset spec types (landed 7.6): `MacroSpec`/`SpecialsSpec` (declarative, preset
  traceback vocabulary) and `EnvironmentSpec` — the §3.4 funnel wrapper over a dyn
  `EnvironmentBehavior` (defaulted `arguments`/`body_state_delta`/`make_body_parser`) —
  driven by `BeginSpec`'s composition over the core building blocks
  (`read_rigid_name_group` + `parse_declared_arguments` + `EnvironmentBodyParser`).
  Verbatim landed 7.7: `VerbatimBehavior` (the `make_body_parser` override) and the
  standard-argument factory `argument_specs` mapping xparse-like code strings to
  configured standard `ArgumentParser`s (pylatexenc's `LatexStandardArgumentParser`
  reshaped as a factory; per-code inventory in ParserLibraryParity.md).
- `NodeRef` accessor sugar as inherent methods on `NodeRef<'_, Latexlike>`
  (`is_math_group`, `math_style`, `macro_name`, `environment_name`, `specials_name`).
- Math handled as group class + first-class mode + mode-visible packages, demonstrating
  the pattern FLM will use.

The TeX-compliance non-goals from PROPOSALS.md §4 (catcodes, expansion, conditionals) remain
non-goals; the `TokenReader` trait is the documented escape hatch for anyone who truly needs
catcode-like tokenization.

---

## 9. Implementation phases

Each phase ends with `cargo build && cargo test` green and that topic documented. No phase
starts until the previous phase's API is discussed and settled. The phase sequence is a build
order over *concrete machinery* (the third graph of §3) — DAG-shaped even where S1 signatures
are mutually recursive; stubs bridge the knot (e.g. Phase 2's tokenizer runs against a
hardcoded `TokenRules` value).

- **Phase 0 — decisions & doc hygiene.** Resolve [DECISION 1–7] — ✅ done, July 2026
  (`SpecLookup` semantics deferred, see §11 point 6). Consolidate documents (§10) —
  ✅ done, July 2026 (stale docs archived to `dev-docs/archive/`; `NAMING_STRATEGY.md`
  rewritten per §7).
- **Phase 1 — `source` + `error`.** Rewrite per §source; port the good tests from current
  `source.rs`; provenance, resolver, `LineIndex`, diagnostics types, recovery-token types.
  — ✅ done, July 2026. Origin type = plain defaulted parameter `Source<O: SourceOrigin>`
  with default `Option<String>` (`L::SourceOrigin` plugs in at Phase 3+); crate made
  `no_std`-friendly (core + alloc only, no file-backed resolver, principle 6);
  `SourceContent` kept as an enabling boundary
  (no mmap, `Source` stores `String`; the boundary was later retired — July 2026, Action
  06, DESIGN_RATIONALE §3.1); diagnostics + `Recovery` policy landed here while
  `TokenError`/recovery tokens move to Phase 2 with `Token`. See DESIGN_RATIONALE.md §3.1/§3.8.
- **Phase 2 — `token`.** `Span`, `Token`, `TokenKind`, `TokenReader` trait, `StdTokenReader`
  driven by a hardcoded-for-now `TokenRules` value; delimiter prefix table; exhaustive tokenizer
  tests (port pylatexenc's tokenizer test cases).
  — ✅ done, July 2026 (S0 half). Ships `Span`, `Token<'s>`/`TokenKind<'s>`, `TokenRules`
  (+ `WhitespaceRules`/`MacroRules`/`CommentRules`/`GroupType`/`GroupTypeId`), the derived
  `PrefixTable` (WIP open/close-ambiguity merging salvaged), `TokenError`/`TokenRecovery`
  (recovery tokens, reader policy-free), and `StdTokenReader` with the `detect_*` scanning
  core; pylatexenc's tokenizer test suite ported/adapted (~30 tests). The `TokenReader<L>`
  *trait* moves to Phase 3 with `ParsingState<L>` — defining it against `&TokenRules` now
  would sever the catcode escape hatch (§token stratum split); `StdTokenReader`'s inherent
  `peek(&mut self, &TokenRules, &PrefixTable)` API is shaped to become the trait impl.
  Implementation decisions (uniform `post_space: Span` on `Token` with pylatexenc span
  conventions; maximal-run `Chars` tokens; `TokenRules::expecting_group_close` as the
  data-driven `$…$`/`$$…$$` disambiguator; `peek` → `Ok(None)` at EOF with trailing
  whitespace untokenized) recorded in DESIGN_RATIONALE.md §3.2/§3.8.
  **Superseded in part (July 2026):** the Phase-2 token *model* (maximal-run `Chars`,
  `Macro`/`Specials`/`CommentStart` kinds, uniform `post_space`, `Ok(None)` at EOF, S0
  scanning core) was reworked in Phase 3 following the token-design review —
  DESIGN_RATIONALE.md §3.2. The scanning machinery (prefix table, group logic, recovery,
  span conventions, test corpus) carried over.
- **Phase 3 — `state` + token rework (merged).** — ✅ done, July 2026. Ships `Lang` (with
  the `scan_specials`/`specials_trigger_chars` token hooks; `NodeExts` waits for Phase 5),
  `ParsingState`/`StateData` (rules + ext; `libraries` waits for Phase 4),
  `ParsingStateDelta` + `TokenRulesOverrides` + `derived()` + `Lang::finalize_transition`
  (test langs exercising events and the pure-normalization override idiom), and the
  reworked token module per §token: S1 `Token<'s, L>` with
  `Char`/`Command`/`Specials`/`Comment`/`ParagraphBreak`/`EndOfStream`, `CommandRule`/
  `CommentRule`, the `skip_whitespace` primitive, the `TokenReader<L>` trait +
  `StdTokenReader`, and the `CallableSpec<L>` trait declaration (stub; fleshed out in
  Phase 4). Derived caches built eagerly at freeze (no_std — DESIGN_RATIONALE.md §3.3);
  `Span` relocated to `source`.
- **Phase 4 — `spec` + `library`.** `CallableSpec` (de-keyed), `StdCallableSpec`,
  `ArgumentStructureSpec` + `SlotStructureSpec`, `CallableTypeId` interning,
  `Library`/`LibraryStack`/`SpecLookup` + per-type unknown-fallback policy.
  — ✅ done, July 2026. The deferred `SpecLookup` semantics were settled first
  (`CallableQuery` with `CallableSyntax` + optional token; fallbacks built into
  `LibraryStack`; DESIGN_RATIONALE.md §3.4). `StateData.libraries` +
  `ParsingStateDelta::push_libraries` landed with it. Deliberately deferred to Phase 6,
  with their consumers: the full argument-kind inventory and slot
  separators/terminators (Phase 4 ships skeletons), `invocation_parser()`, and
  `CallableTypeId` *interning* (ids are direct-constructed like `GroupTypeId` until
  `Language<L>` exists). *(Amended July 2026, current-level review: `ArgumentKind` and the
  structure-spec wrappers replaced by the pylatexenc-shaped `ArgumentSpec<L>` model; both
  `…TypeId`s became closed `Lang` associated types — interning cancelled. DESIGN_RATIONALE
  §3.4.)*
- **Phase 5 — `node`.** Flat `NodeTree`, `NodeKind<L>`/`CallableData`, `TextContent`, ext
  bundle (`NodeExtTypes`, `SimpleLang`), `NodeRef`, builder used by the session;
  `materialize()`.
  — ✅ done, July 2026. The deferred args/slots ↔ children encoding was settled first
  (**one node per region**: one child per present argument, one `List` child per slot;
  presence/offsets and per-instance syntax in `ArgsLayout`/`SlotsLayout` —
  DESIGN_RATIONALE.md §3.5). Ships the topic as designed, plus: node spans stay mandatory
  (synthetic-node span representation deferred to the transform API, post-Phase-6); a
  *staging* `NodeTreeBuilder` whose `finish()` lays siblings out contiguously breadth-first
  (direct arena emission can't — §3.5); `TextContent` housed in the source topic (S0), no
  `PartialEq` on it or on node types yet. Per-instance syntax records and possible `Comment`
  recomposition fields grow with their consumers in Phase 6 (DESIGN_RATIONALE.md §6.5).
  *(Amended July 2026, current-level review: `ArgsLayout`/`SlotsLayout` replaced by
  pylatexenc-shaped `ParsedArguments`/`ParsedSlots`; `Group` grew a boxed `GroupData` with
  on-node delimiters. Amended again July 2026, regions session: one child *region* per
  argument/slot — inter-argument noise kept as nodes, `pre_space` removed,
  parser-designated content ranges (`ChildRegion`/`ContentNodes`), two-phase records
  resolved to global node indices by `finish()`. DESIGN_RATIONALE §3.5.)*
- **Phase 6 — `constructs` + engine core.** Execution plan: **Phase6Execution.md** (July
  2026, Phase 6 plan session; supersedes the preliminary notes). Scope as amended there:
  `ParseContext` + `ConstructParser`/`ConstructParserResult` (parsers are temporaries —
  two-tier ownership), `NodesParser` (reified + programmatic stop conditions, `StopCause`),
  group parsing, the `make_invocation_parser` factory + `StdInvocationParser`, standard
  argument parsers (regions/`ContentNodes`), core `EnvironmentBodyParser`; `Lang` hooks
  `resolve_command` / `make_paragraph_break_node` / `finalize_node`; token amendments
  (`escape_char` on `Command`, `multi_newline_paragraphs` rename, token-list reader);
  `Comment` syntax fields; detection-site tolerant recovery; whitespace/span invariants
  pinned + `check_tree_invariants()`. **`Language<L>` and any `parse()` convenience are
  deferred** — `ParserSession` is the root object. End-to-end tests on LaTeX-ish snippets
  under test langs (the real preset is Phase 7).
  DESIGN_RATIONALE §3.2/§3.5/§3.6/§3.8/§3.10.
  — ✅ done, July 2026 (subphases 6.0–6.7; per-subphase record in Phase6Execution.md's
  progress table, amendments in the DESIGN_RATIONALE sections above). Ships the full scope
  as amended in execution: the dispatch loop with all arms + detection-site tolerant
  recovery, stop machinery (`StopCause`; consume switch, 6.2), `ChildStateSpec` + the
  session state-derivation seam with the memoized group-interior states (6.3),
  `Invocation`/`make_invocation_parser`/`StdInvocationParser` with post-space = the
  trigger token's own syntactic post-space (6.4 amendment — no claiming helper), the
  standard argument parsers with balanced optional-group delimiters (6.5 review), the core
  `EnvironmentBodyParser` (rigid scaffolding, decision-8 recovery matrix, 6.6), `Lang`
  hooks, `TokenListReader`, `check_tree_invariants()`, and the §G end-to-end suite.
  Deferred as planned (Phase6Execution.md §4): `Language<L>`/`parse()`, verbatim +
  preset one-liner specs and the `\begin` composition's home (test-side meanwhile),
  content-extraction views; the per-invocation-`Box`
  benchmark check is consciously deferred, not dropped (DESIGN_RATIONALE §3.6). Post-6.6
  sessions already amended the shipped shape in place (slots session: `SlotSpec` deleted,
  composition building blocks promoted `pub`; emptiness surface; condition-derive;
  temporary state-scoped group rules — `TokenRules::temporary_groups`, July 2026 — with
  the optional-group parser detached from `ChildStateSpec`).
- **Phase 7 — driver, scope stack & `latexlike` preset.** Execution plan:
  **Phase7Execution.md** (July 2026 plan session; supersedes the original one-line scope —
  "environments, math modes, verbatim, std library, ported walker tests"). Scope as
  amended there: parsing mode as first-class state data (`L::ModeId`); the `ParseDriver`
  object (`Lang::Driver` — construct-parser provision, descent deltas, recovery policy,
  migrated parse-time hooks); the scope-stack redesign (`SpecsProvider`,
  `Package`/`Scope`, in-stack fallbacks, definition/stack delta ops); `Language<L>` +
  `parse()`; the latexlike preset (environments via `\begin`/`\end` specs, math modes,
  verbatim, argument-code factory) with a **minimal test-driven spec set**; the
  extraction/view API (R7); ported pylatexenc walker acceptance tests + tolerant-parsing
  behavior tests. **Deferred out of Phase 7:** the std spec-database port, parse-level
  `\newcommand`, `^`/`_` specials, `\global` definitions.
  DESIGN_RATIONALE §3.3/§3.4/§3.6.
  — ✅ done, July 2026 (subphase record: Phase7Execution.md §5; exits with the 7.9
  acceptance suite — the ported walker slice green in both recovery modes with
  `check_tree_invariants` on every parse — plus the `docs/learn-by-example.md` guide
  chapter. The §6.7 per-invocation-`Box` micro-benchmark obligation remains open,
  unscheduled.)
- **Phase 8 — FLM spike.** Minimal `Flm` lang in a scratch crate exercising: custom `StateExt`,
  custom node payloads, custom invocation parser, resolver, post-processing traversal. This
  validates goal 3 before FLM proper begins.

---

## 10. Documentation hygiene

Too many overlapping documents; several are stale or superseded. Proposal:

**Keep, as living documents:** `ARCHITECTURE.md` (this file), `DESIGN_RATIONALE.md`,
`NAMING_STRATEGY.md` (✅ updated per §7, July 2026), `CLAUDE.md`, `README.md`.

**Archive to `dev-docs/archive/`** (history, no longer authoritative): `TRAIT_BASED_ARCHITECTURE.md`,
`TRAIT_ARCHITECTURE_QUICKREF.md`, `ALIGNMENT_AUDIT.md`, `PROJECT_SUMMARY.md`, `QUICKSTART.md`,
`DEVELOPMENT.md`, `PARSING_STRATEGY.md` (its decisions are absorbed here),
`pylatexenc_to_rust_strategy.md` and `PROPOSALS.md` (keep accessible as pylatexenc feature
inventory + TeX gap analysis); `SOURCE_ARCHITECTURE.md` — ✅ archived July 2026, its surviving
content folded into §source/§nodes/§engine here and DESIGN_RATIONALE.md §3.1/§3.5 (superseded parts: the
`SharedPointer` GAT by Decision 4, `FLMEnvironment` naming by Decision 2, its open-ended
generic-node-data sketch by Decision 3's ext system).

Decision rationale is tracked in [DESIGN_RATIONALE.md](DESIGN_RATIONALE.md) — a living log of
the arguments, rejected alternatives, and open questions behind each decision (it supersedes
the `DECISIONS.md` idea originally proposed here). New design discussions should append there.

Do not pollute ARCHITECTURE.md and source files with detailed changes following design decisions,
especially not at this stage with frequent design decision changes.  Design change rationales
go in DESIGN_RATIONALE.md.  Only mention a design change in rust source/code-level doc
or in ARCHITECTURE.md if there is a clear reason to do so, e.g., to note a pitfall that is
not obvious.

---

## 11. Collected decision points

1. **RESOLVED (July 2026): materialized state + transition choke point ("Option C").**
   Stored `StateData`/`TokenRules` behind getter-only public surface; `ParsingStateDelta<L>`
   overrides-struct (+ `L::Event`s) as the reified change value; `derived()` as the sole
   constructor of non-initial states; `Lang::finalize_transition` as the customizer for
   cross-cutting rules. Replaces the per-facet trait + macro design in earlier source tree.
   (Design: §state. Rationale, including rejected Options A and B: §4.)
2. **RESOLVED (July 2026): naming.** `Lang` trait + `Language<L>` runtime object (dropping
   `FLMEnvironment` and `LanguageSpecification`). (§7)
3. **RESOLVED (July 2026): unified `Callable` node kind + two-tier ext + `TextContent`
   ("Option F").** Closed structural `NodeKind<L>` (`Chars`/`Group`/`Callable`/`Comment`/
   `List`, no `Custom`); Macro/Environment/Specials merged into `Callable` with a
   `Lang::CallableTypeId` invocation form; de-keyed specs, never-`None` via per-type fallback
   singletons; owned names, `TextContent` content, `post_space` kept; args/slots as two named
   concepts over shared machinery; whitespace-as-chars-nodes + exact sibling-span partition;
   recomposition levels 1+2 as stated requirements constraining `ParsedArguments`. **No core `MathNode`** (math =
   group types + preset state ext). (Design: §specs/§nodes. Rationale: §4b.)
4. **RESOLVED (July 2026): defer `Rc`/`Arc` genericity**; `Arc` behind an internal alias
   for now. (§5)
5. ✅ **DECIDED (July 2026): zero mandatory dependencies.** Drop `thiserror` (hand-written
   `Display`/`Error` impls — our errors need bespoke span/provenance rendering anyway, so the
   derive only covered the trivial part) and drop `log` entirely (library conditions surface
   through the diagnostics sink / `ParseResult`, not a logging side channel; can be reintroduced
   later as an optional feature if internal tracing proves useful).
   **Amended (July 2026, performance review):** a dependency may be added in very specific,
   exceptional cases, for widely used, lightweight, `no_std`-capable crates — decided case by
   case. First (and so far only) instance: `hashbrown` (the implementation inside std's own
   `HashMap`), backing the engine's derivation memo (DESIGN_RATIONALE.md §3.6); `no_std` has
   no `std::collections::HashMap`, and a hand-rolled map would be worse on every axis.
6. **RESOLVED (July 2026): no `ConflictStrategy`** — library resolution = ordered
   stack with lexical shadowing, no built-in mode tables. The deferred `SpecLookup`
   semantics were settled in the Phase 4 design session (July 2026): `CallableQuery`-based
   lookup (explicit `CallableSyntax`, optional token, mode-awareness via the state
   argument) with per-`CallableTypeId` fallbacks built into `LibraryStack`.
   (§specs; rationale in DESIGN_RATIONALE.md §3.4.)
7. **RESOLVED (July 2026): rebuild `src/` phase-by-phase** per §9 rather than repairing the
   current tree in place (salvaging: prefix-table logic, `detect_*` decomposition, recovery
   tokens, source tests, line-index logic).
8. **DECIDED (July 2026): three strata + three rules replace the strict L0–L7 layer ladder.**
   S0 Lang-free foundation / S1 single mutually-recursive core stratum (modules are topics,
   not dependency ranks) / S2 presets; rules: no `Lang` in S0, no preset in S1, acyclic
   runtime ownership graph. Consequences: the `TokenReader<L>` trait is S1 and keeps its
   `&ParsingState<L>` parameter (escape-hatch access to `L::StateExt`); `Lang` +
   `NodeExtTypes` are defined in the core next to the state types, not in `engine/` or
   `node/`; `Language<L>` participates only by seeding the initial state. (Design: §3.
   Rationale: DESIGN_RATIONALE.md §3.11.) *Revised July 2026: the token topic — originally
   split S0 data / S1 trait — moved wholly to S1, and `Span` to the source topic
   (token-design review; DESIGN_RATIONALE.md §3.2).*
9. **DECIDED (July 2026, Phase 7 plan session): `ParseDriver` + first-class parsing mode +
   scope-stack redesign.** Parse-driving behavior (construct-parser provision, the group
   descent-delta channel, recovery policy, and the migrated hooks `resolve_command` /
   `make_paragraph_break_node` / `observe_transition` / `refine_diagnostic`) lives on a
   Lang-provided `ParseDriver` instance (`Lang::Driver`); `Lang` keeps the state-,
   tokenizer-, and builder-layer hooks. `StateData` gains `mode: L::ModeId` (deltas
   initiate mode changes; `finalize_transition` interprets them).
   `Library`/`LibraryStack`/`SpecLookup` are redesigned as `Package`/`Scope`/`ScopeStack`
   over a dyn `SpecsProvider` contract with in-stack fallbacks and definition/stack delta
   ops. (Plan: Phase7Execution.md. Rationale: DESIGN_RATIONALE.md §3.3/§3.4/§3.6.)
