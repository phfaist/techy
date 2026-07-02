# techy Architecture Plan

**Status: PROPOSAL — for discussion, July 2026.**
Written after a full review of all strategy documents, the current (non-compiling, mid-refactor)
source tree, and the pylatexenc sources. Where this document conflicts with older documents,
this document reflects the newer proposal; nothing here is final until discussed.

Decision points that need explicit sign-off are marked **[DECISION n]** and collected at the end.

**Decisions 1 and 3 were discussed and RESOLVED, July 2026.**
Decision 1 (parsing-state design): materialized state + transition choke point ("Option C");
see §L2 for the design and §4 for the recorded rationale. Decision 3 (node representation):
unified `Callable` kind + two-tier ext + `TextContent` ("Option F"); see §L3/§L4 for the design
and §4b for the recorded rationale. The remaining decision points are still open.

---

## 1. Assessment of where things stand

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
layer-by-layer with each layer compiling and tested before the next. (§9.)

---

## 2. Design principles

Derived from your stated goals and the decided parts of the existing documents:

1. **Data-driven where possible, trait-driven where necessary.** Anything that can vary *during a
   parse* (delimiters, escape chars, enabled features) is plain stored data in the parsing state,
   changed only through reified deltas at a single transition choke point (§L2). Traits are
   reserved for genuine *behavior* extension points: token readers, construct parsers, spec
   lookup, source resolution, and the per-language transition customizer.
   This single principle resolves most of the "how generic should X be" questions.

2. **One generic parameter.** A single `Lang` trait bundles all compile-time customization.
   Every core type takes one `L: Lang` parameter, never five.

3. **No privileged language concepts in the core.** No math mode, no `{`/`}`, no `%`, no `\` in
   the engine. All of it is data in the parsing state or definitions in libraries. The familiar
   LaTeX behavior lives in a *preset* (§8).

4. **Zero-copy by default; logical content is first-class.** Tokens reference source content by
   byte spans. Node *textual content* is `TextContent` (§L4): span-backed when it came from
   parsing, owned when synthesized or normalized — the span is provenance, not the content's
   storage. Identity-bearing data (callable names) is always owned.

5. **Closed structural core, open payloads.** The engine knows a small fixed set of *structural*
   shapes (chars, group, callable invocation, comment, list) — no `Custom` variant, no
   open-ended node trait objects. Semantics attach through specs; custom data attaches through
   per-node and per-kind ext types supplied by `Lang` (§L4), orthogonal to structural identity.

6. **Zero mandatory dependencies.** Hand-written `Display`/`Error` impls instead of `thiserror`;
   no `log` — library conditions flow through the diagnostics sink instead. **[DECISION 5 — decided]**

7. **`Result` everywhere, panics never**, with first-class tolerant parsing (recovery tokens,
   diagnostics sink) rather than a bolted-on flag.

---

## 3. The layered architecture

Strict layering; each layer depends only on lower ones. Arrows in the ownership graph only point
downward (this is also the Arc-cycle-prevention invariant from SOURCE_ARCHITECTURE.md).

```
L7  presets/           latexlike preset; later: flm (separate crate)
L6  engine/            Language<L>, ParserSession, ParseResult, NodeRef
L5  constructs/        ConstructParser trait + standard construct parsers
L4  node/              NodeTree (flat), NodeKind<L>, CallableData, TextContent, ext payloads
L3  spec/ + library/   CallableSpec (de-keyed), StdCallableSpec, CallableTypeId, Library, LibraryStack
L2  state/             ParsingState<L>: stored TokenRules + libraries + L::StateExt; derived() + deltas
L1  token/             Token<'s> (span-based), TokenReader trait, StdTokenReader
L0  source/            Source, SourceSpan, SourceProvenance, SourceResolver, cursor, LineIndex
    error.rs           spans-based diagnostics, recovery tokens
```

### L0 — source (adopt SOURCE_ARCHITECTURE.md)

Exactly as decided in March: `Arc<Source>`-based `SourceSpan`, provenance enum
(`Primary` / `Resolved` / `Synthesized`) with `triggered_at: SourceSpan` back-references,
`SourceResolver` trait (`NoResolver` ZST default), `SourceContent` trait over the backing
storage, cursor with mark/rewind, standalone lazy `LineIndex`.

One correction to the current `source.rs`: the per-location `via: [SourceLocationVia]` chain is
removed. Provenance belongs on `Source` (one hop per synthesized/included source), not on every
location — that is both cheaper and structurally cycle-free. The existing
`SourceLocationAnalyzer` becomes the standalone `LineIndex` utility (its lazy line-start logic
and traceback formatting are worth keeping).

`Source` is generic over `L::SourceOrigin` only through the `Lang` parameter; the default origin
type is a small enum (name string + kind), not a bare `String`.

### L1 — token

Tokens are **transient, span-based, zero-copy**:

```rust
/// Plain byte range within one Source. Copy. Used everywhere during parsing.
pub struct Span { pub start: usize, pub end: usize }

pub struct Token<'s> {
    pub kind: TokenKind<'s>,
    pub span: Span,
    pub pre_space: Span,      // whitespace before the token — a span, not a String
}

pub enum TokenKind<'s> {
    Chars(&'s str),
    Macro { name: &'s str },
    GroupOpen  { delim: &'s str, group_type: GroupTypeId },
    GroupClose { delim: &'s str, group_type: GroupTypeId },
    CommentStart { delim: &'s str },
    Specials { chars: &'s str },
    ParagraphBreak,           // multi-newline; content recoverable from span
}
```

Key points, confirming PARSING_STRATEGY.md decisions:

- Tokens are **structural and minimal**: they identify *what to parse next*, nothing more.
  No `BeginEnvironment(name)` token — `\begin` is just a macro token; environment recognition
  is a construct-parser concern (the latexlike preset registers `\begin`/`\end` specs).
  This is a deliberate departure from pylatexenc and stays.
- The `'s` lifetime is ephemeral (borrows the current source unit's content); it never enters
  the AST. Nodes store `SourceSpan` (Arc-based).
- `TokenReader<L>` is a trait — the extension point for genuinely different tokenization
  *behavior*. The provided `StdTokenReader` is driven entirely by `TokenRules` data from the
  parsing state (§4). Signature sketch:

```rust
pub trait TokenReader<'s, L: Lang> {
    fn peek(&mut self, state: &ParsingState<L>) -> TokResult<'s, Option<Token<'s>>>;
    fn move_past(&mut self, tok: &Token<'s>, skip_post_space: bool);
    fn move_to(&mut self, tok: &Token<'s>, rewind_pre_space: bool);
    fn pos(&self) -> usize;
}
```

  (`next` = provided method: peek + move_past, as in the current WIP — keep.)

### L2 — parsing state  *(Decision 1 — RESOLVED, July 2026)*

Parsing state is **materialized data behind a single transition choke point** ("Option C" of
the design discussion recorded in §4). All stored fields are private; the public read surface
is getter methods over plain fields; and the *only* way a non-initial state comes into
existence is `derived()`.

```rust
pub struct ParsingState<L: Lang> {
    data: StateData<L>,                    // private — getters are the public surface
    prefix_table: OnceLock<PrefixTable>,   // per-instance derived caches, built lazily
}

pub struct StateData<L: Lang> {
    pub rules: TokenRules,          // tokenization rules — plain stored data
    pub libraries: LibraryStack<L>, // definitions visible here (extendable mid-parse: \newcommand)
    pub ext: L::StateExt,           // language-specific state (e.g. FLM's math mode)
}

pub struct TokenRules {
    pub whitespace: Option<WhitespaceRules>,   // None = whitespace handling disabled
    pub macros: Option<MacroRules>,            // escape char, name chars
    pub group_types: Vec<GroupType>,           // {…}, […], $…$, $$…$$, \[…\] — all just group types
    pub comments: Option<CommentRules>,        // start delimiter(s)
    pub paragraph_breaks: bool,
    pub specials: Vec<String>,                 // specials *strings* only; semantics live in libraries
    pub forbidden_chars: String,
}
```

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

Properties, roughly in decreasing order of importance:

- **Functional contract, no observable mutation.** `derived()` is state-in/state-out; the
  `&mut` exists only inside it, on a clone nothing else can observe yet.
- **Producer/scope split.** The party producing a change and the party deciding its scope
  differ, which is why the delta must be a standalone value. For *inward* scoping (group or
  math interior) a parser calls `state.derived(…)` itself and drops the child state when done
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
- **Airtightness is structural.** Private fields + `derived()` as sole constructor mean the
  compiler guarantees finalize sees every change — no documented invariant needed.
- **Hot path = plain field reads.** Per-instance caches (the sorted delimiter-prefix table
  with open/close-ambiguity merging, salvaged from the WIP) stay valid for the `Arc`'s
  lifetime because states are immutable — and `dbg!(state)` always shows exactly what the
  tokenizer will do.
- **Math mode does not exist here.** The latexlike preset defines
  `StateExt = LatexlikeState { in_math_mode: MathMode, … }` plus its `Event` type; its
  math-group parser emits the event. The core never asks.
- `ParsingState` is immutable and cheaply shareable; the engine wraps it in `Arc` and creates
  a new one only at transitions, so nodes record their parse-time state
  (SOURCE_ARCHITECTURE.md decision, kept). No `TypeId` maps, no `dyn Any`.

### L3 — specs and libraries  *(updated per Decision 3 resolution — §4b)*

The **callable** concept from PARSING_STRATEGY.md, unified and **de-keyed**: a spec records
*callable behavior*, not the form or name under which it is invoked. The invocation form is an
interned **`CallableTypeId`**, registered in `Language` exactly like `GroupTypeId`; the
latexlike preset registers `MACRO`, `ENVIRONMENT`, `SPECIALS`. Naming rule, systematic across
the crate: `…Kind` = closed core enum, exhaustively matchable (`TokenKind`, `NodeKind`);
`…TypeId` = open, `Language`-interned, preset-registered (`GroupTypeId`, `CallableTypeId`).

```rust
/// Behavior of anything invocable from the token stream. De-keyed: carries no name and
/// no invocation form; one spec may back several names (\emph and \textit can share).
pub trait CallableSpec<L: Lang> {
    fn arguments(&self) -> &ArgumentStructureSpec;
    fn slots(&self) -> &SlotStructureSpec;
    /// Parser consuming the invocation. The default is built from the two structure
    /// specs; overriding it is the full-takeover escape hatch (\verb, tabular preambles,
    /// FLM constructs) — pylatexenc's most valuable extensibility property, preserved.
    fn invocation_parser(&self) -> &dyn ConstructParser<L, Output = NodeId>;
    /// Optional recomposition hook (§L4 level 2) for constructs whose custom parser
    /// records per-instance syntax the default recomposer cannot infer.
    // fn recompose(&self, …) -> …   — default covers declaratively-specced callables
}
```

**Arguments vs. slots.** All callables can have both. *Arguments* configure (parsed per
`ArgumentStructureSpec`: mandatory / optional / star / verbatim-delimited / …); *slots* contain
content regions (`SlotStructureSpec`: separators and terminators, where terminator patterns may
reference the invocation name — `\end{align}` must match the `align` that opened; a `---` fence
closes with `---`). A macro has no slots; an environment has exactly one (its body); specials
usually have none but may have any number (a fence-block construct with `+++` separators is
expressible as a specials callable with multiple slots). The boundary is a guideline, not a
theorem (`\verb`'s delimited content could be argued either way); the spec decides, and the
machinery underneath is shared, so nothing breaks structurally either way.

The core ships one standard implementation (`StdCallableSpec`: the two structure specs +
optional parser override). The familiar `MacroSpec` / `EnvironmentSpec` / `SpecialsSpec` names
survive as constructor helpers in the preset layer — "macro" and "environment" are invocation
forms, not core concepts.

**Libraries** (supersedes `ContextDb`, per PROPOSALS.md but simplified):

```rust
pub trait SpecLookup<L: Lang> {
    fn lookup(&self, ct: CallableTypeId, name: &str, state: &ParsingState<L>)
        -> Option<Arc<dyn CallableSpec<L>>>;
}

pub struct Library<L: Lang> { /* name; HashMap<(CallableTypeId, Box<str>), Arc<dyn CallableSpec<L>>> */ }

pub struct LibraryStack<L: Lang> { /* ordered Vec<Arc<dyn SpecLookup<L>>>, innermost first */ }
```

- **Keys are `(CallableTypeId, name)`, many-to-one**: several names may map to one shared
  behavior spec (flyweight). Library keys hold the *normalized* name; the node records the
  *invocation spelling* (§L4) — the right split given de-keyed specs.
- Resolution: ordered stack, innermost/last-added wins (lexical shadowing — matches
  `\newcommand` semantics and group-local definitions). No `ConflictStrategy` enum: shadowing
  *is* the semantic; an optional lint pass can warn on shadowing if wanted.
- **Unknown callables**: per-`CallableTypeId` fallback policy on the stack, returning a
  **shared singleton** spec — possible precisely because specs are de-keyed (nothing
  instance-specific lives in them). Consequence: a callable node's spec is **never `None`**.
- **Mode-aware lookup without privileged modes**: `lookup()` receives `&ParsingState<L>`, so a
  preset's `SpecLookup` implementation may dispatch on `state.ext` (e.g. FLM resolving `\vec`
  differently in math mode). The core `Library` type ignores the state. This replaces
  PROPOSALS.md's hard-coded `math_mode_macros` tables, which contradicted the
  no-privileged-modes principle.
- Extending definitions mid-parse = pushing a `Library` onto the stack via
  `ParsingStateDelta::push_libraries`; popping happens naturally because the previous
  `Arc<ParsingState>` is restored when the group ends.

### L4 — nodes  *(Decision 3 — RESOLVED, July 2026; evolution recorded in §4b)*

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
    Group    { group_type: GroupTypeId, ext: L::GroupNodeExt },
    Callable(Box<CallableData<L>>),   // boxed: Chars dominates the vec; keeps the enum small
    Comment  { content: TextContent, ext: L::CommentNodeExt },  // text sans delimiter/newline
    List     { ext: L::ListNodeExt },
}

pub struct CallableData<L: Lang> {
    pub callable_type: CallableTypeId,   // invocation form: latexlike MACRO / ENVIRONMENT / SPECIALS
    pub name: Box<str>,                  // invocation spelling; identity ⇒ always owned
    pub spec: Arc<dyn CallableSpec<L>>,  // behavior; shared, de-keyed; never None (§L3 fallback)
    pub args: ArgsLayout,                // spec-slot refs + presence + per-instance syntax choices
    pub slots: SlotsLayout,              // 0..n content regions (environment body = 1 slot)
    pub post_space: TextContent,         // reproduced verbatim in recomposition
    pub ext: L::CallableNodeExt,         // tier 2: per-kind, per-instance parse results
}
```

**No `Macro`/`Environment`/`Specials`/`Math`/`Custom` variants.** The structural taxonomy is
exactly PARSING_STRATEGY.md's concept list — chars, groups, callables, comments — and
"environment" is not a core concept: "is this an environment" =
`node.callable_type() == latexlike::CT_ENVIRONMENT` (honest two-level dispatch). `$…$` parses
as a `Group` with a `$`-delimited `GroupTypeId` under the preset's math-mode state ext; the
preset provides `NodeRef` accessor helpers so ergonomics don't suffer. Exhaustive pattern
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

**Whitespace and span invariants** (decided in principle; pinned down in phase 6):
- Whitespace immediately following a callable is its `post_space`, **stopping at any paragraph
  break**; whitespace elsewhere between constructs becomes a whitespace-only `Chars` node
  (pylatexenc behavior). No double counting ⇒ **sibling spans partition the parent's interior
  exactly** — no gaps; span math trustworthy for tooling and editing.
- A callable's `SourceSpan` **includes** its `post_space` (so a `Spanned` post_space is a
  trailing sub-range of the node's own span). Node span semantics are the public contract and
  are deliberately decoupled from token behavior — tokens are transient engine internals.
- Paragraph breaks surface as their own nodes; whether as whitespace `Chars` or as a
  specials-type callable (PARSING_STRATEGY.md contemplated `"\n\n"` as specials) is a preset
  decision, phase 7.

**Recomposition requirement** (constrains `ArgsLayout`):
- *Level 1 — verbatim:* a node's own `SourceSpan` → exact original text. Never needs an
  external lookup (the Arc travels with the node); works for detached and mixed-origin trees.
- *Level 2 — Lang-aware quasi-equivalent:* recompose from `(callable_type, name, args, slots,
  post_space, children)` plus the `Language` registries (group/callable type → delimiters and
  invocation form), reproducing parsed text rather than guessing. "Quasi-equivalent" =
  re-tokenizes and re-parses to an equivalent tree under the same `Language` (the validity
  criterion — licenses e.g. inserting a separating space where required). Consequence: **the
  invocation parser must record per-instance syntax choices the spec doesn't determine**
  (optional-arg presence, matched delimiter alternative, chosen verbatim fence, star) — in
  `ArgsLayout`, as `TextContent` where textual, *not* in ext: recomposability must not depend
  on Lang cooperation. Exotic custom parsers use the `CallableSpec::recompose()` hook.

Access is only through `NodeRef<'pr>` proxies as designed in March (Copy, resolves indices,
`span_content()`, `children()`, `parsing_state()`, `name()`, `arguments()`, `slots()`,
`body()`, …). Trees are immutable after `ParserSession::finish()`; transformations build new
trees (Arc-shared sources/specs/states make mixed-origin trees free — including *synthetic*
callables and chars nodes, which owned names and `TextContent::Owned` make possible without
fabricating sources).

### L5 — construct parsers

The single most important trait in the system. To avoid pylatexenc's three-argument threading
(`walker, token_reader, parsing_state`), everything a parser needs rides in one context:

```rust
pub struct ParseContext<'a, 's, L: Lang> {
    pub tokens: &'a mut dyn TokenReader<'s, L>,
    pub state: Arc<ParsingState<L>>,
    pub session: &'a mut ParserSession<'s, L>,   // node building, source creation, resolver, diagnostics
}

pub trait ConstructParser<L: Lang> {
    type Output;                                  // NodeId, ArgsLayout, () …
    fn parse<'s>(&self, cx: &mut ParseContext<'_, 's, L>)
        -> ParseResult<(Self::Output, Option<ParsingStateDelta<L>>)>;
}
```

**Dispatch is by token kind + library lookup — never by parser registry scanning.** The main
loop (`NodesParser`, pylatexenc's `LatexGeneralNodesParser` + nodes collector):

```
loop:
  tok = tokens.peek(state)
  match tok.kind:
    Chars           -> accumulate chars node (incl. whitespace-only chars nodes, §L4)
    ParagraphBreak  -> own node (representation: preset decision, §L4)
    GroupOpen(t)    -> GroupParser(t)                  (delimiters from state.rules)
    CommentStart    -> CommentParser
    Macro(name)     -> libraries.lookup(CT_MACRO, name, state)     (unknown -> fallback singleton)
                         -> spec.invocation_parser().parse(cx)
    Specials(s)     -> libraries.lookup(CT_SPECIALS, s, state) -> …
    GroupClose(t)   -> stop condition / error
  returned delta -> state.derived(&delta) -> new Arc<ParsingState> for subsequent siblings
```

Everything the Sonnet doc wanted from `can_parse()`/`priority()` registries is achieved
data-first: custom syntax enters via (a) specials strings in `TokenRules` + a `SpecialsSpec`,
(b) macro/environment specs with custom invocation parsers, (c) new group types, or — the
nuclear option — (d) a custom `TokenReader` or a custom nodes parser. Deterministic, no
priority races.

Standard construct parsers to implement (mirroring pylatexenc's `latexnodes.parsers`):
`NodesParser` (with stop conditions), `GroupParser`, `CommentParser`,
`CallableInvocationParser` (the default built from argument+slot structure specs; environments
arrive via the preset's `\begin`/`\end` specs), `ArgumentsParser` (+ std argument types),
`SlotsParser` (separators/terminators with invocation-name back-reference), `DelimitedParser`,
`VerbatimParser`, `ExpressionParser` (single node).

### L6 — engine

Per SOURCE_ARCHITECTURE.md, with one renaming: `FLMEnvironment` collides fatally with
`EnvironmentSpec`/`EnvironmentNode`. Proposed **[DECISION 2]**:

```rust
pub trait Lang: Sized {                 // the compile-time bundle (was: LanguageSpecification)
    type StateExt:  Clone + Debug + Default;
    type Event:     Clone + Debug;      // semantic transition events (e.g. FLM's EnterMath)
    type NodeExts:  NodeExtTypes;       // bundle: uniform NodeExt + per-kind <Kind>NodeExt (§L4)
    type SourceOrigin: …;

    /// Transition customizer — the choke-point hook of §L2. Default: empty.
    fn finalize_transition(new: &mut StateData<Self>, prev: &ParsingState<Self>,
                           events: &[Self::Event]) {}
}

pub struct Language<L: Lang> {          // the runtime bundle (was: FLMEnvironment)
    // base libraries, default TokenRules, default StateExt, settings, resolver
}

impl<L: Lang> Language<L> {
    pub fn parse(&self, input: impl Into<SourceInput>) -> Result<ParseResult<L>, …>;
    pub fn session(&self) -> ParserSession<'_, L>;    // advanced path
}
```

“Define a language once, parse many documents in it” — `Language<L>` owns no per-parse state.
`ParserSession` (transient) builds the `NodeTree`, creates `Arc<Source>`s (resolver,
synthesized sources), collects diagnostics; `finish()` freezes into `ParseResult`.

### Errors and tolerant parsing

- Zero-dep hand-written error types. Every error carries a `SourceSpan` (not a lifetime-bound
  `SourceLocation<'src>` — Arc spans remove the `'src` infection that currently spreads through
  `error.rs`, `Result<'src, T>` etc.).
- Keep and formalize the WIP recovery mechanism: `TokenError { …, recovery: Option<RecoveryToken> }`;
  the session's `Recovery` policy (strict / tolerant) decides whether to record a diagnostic and
  continue with the recovery token or abort. Diagnostics accumulate in the session and are
  available on `ParseResult` even for successful tolerant parses.
- Rich human-readable rendering (line/col via `LineIndex`, provenance chain via
  `SourceProvenance`, open-blocks traceback — the existing `format_traceback` work slots in
  here).

---

## 4. How Decision 1 was resolved (facet traits → A → B → C)

Recorded so the reasoning survives; the resulting design is §L2. Four candidates were compared
(discussion of July 2026):

**Per-facet traits (the WIP in `src/state/`)** — each tokenization facet behind its own trait
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
because producer and scope-decider differ — see the producer/scope split in §L2.

What survives from the WIP `src/state/`: the cached sorted delimiter-prefix table
(`cached_prefix_strings`, with the open/close-ambiguity merging — good work, keep it), the
`detect_*` decomposition of the tokenizer (as private methods of `StdTokenReader`), and the
recovery-token mechanism.

---

## 4b. How Decision 3 was resolved (`Custom` variant → unified `Callable` + two-tier ext)

Recorded so the reasoning survives; the resulting design is §L3/§L4 (discussion of July 2026).

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
- `Event` — semantic transition events, consumed by `Lang::finalize_transition` (§L2).
- `NodeExts` — bundle of node ext types: uniform `NodeExt` (every node) plus per-kind
  `CharsNodeExt` / `GroupNodeExt` / `CallableNodeExt` / `CommentNodeExt` / `ListNodeExt` (§L4).
- `SourceOrigin` — origin metadata type.

Defaults: `Lang` for the latexlike preset is a ZST (`Latexlike`), and type aliases
(`type LatexParseResult = ParseResult<Latexlike>` …) keep simple usage generics-free.

Deliberately **not** generic (for now):
- **Shared pointer (`Rc` vs `Arc`)** — the `SharedPointer` GAT in SOURCE_ARCHITECTURE.md §Generics
  would infect every signature in the crate for a micro-optimization (uncontended atomic clones
  are ~1ns, and Arcs are cloned once per node, not per byte). Proposal: use `Arc` behind an
  internal alias `pub(crate) type Shared<T> = Arc<T>` so a later swap (or a later GAT layer, or
  a cargo feature) is mechanical. **[DECISION 4]**
- Spec types — extensibility comes from `CallableSpec` being a trait; no need for `Lang` to name
  concrete spec types.
- Content backing stays behind the already-planned `SourceContent` trait (mmap deferred).

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

Keep all existing decisions (no `Latex` prefixes, `ArgumentStructureSpec`, `Arguments`, …), plus:

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

The high-level entry point is `Language::parse()`; whether a convenience `Parser` struct still
exists on top is a bikeshed we can defer.

---

## 8. The latexlike preset (L7)

A module (`techy::latexlike`), not a separate crate, providing:

- `Latexlike` ZST implementing `Lang` (`StateExt` with `MathMode`, …); registers callable types
  `MACRO` / `ENVIRONMENT` / `SPECIALS` and the standard group types; provides `MacroSpec` /
  `EnvironmentSpec` / `SpecialsSpec` constructor helpers and `NodeRef` accessor sugar
  (`as_math()`-style, environment/macro views over `Callable` nodes).
- Default `TokenRules`: `\` escape, `{}` groups, `[]` optional-argument group type, `$ $$ \( \[`
  math group types, `%` comments, standard specials strings (`~ & # ^ _` …).
- A standard `Library`: common macros/environments/accents (the "easy wins" list from
  PROPOSALS.md §4), `\begin`/`\end` handling, verbatim environments, `\newcommand` producing
  library-extension deltas (parse-level only — no TeX expansion engine, per the decided
  non-goals in PROPOSALS.md).
- Math handled as group types + state ext + mode-aware `SpecLookup`, demonstrating the pattern
  FLM will use.

The TeX-compliance non-goals from PROPOSALS.md §4 (catcodes, expansion, conditionals) remain
non-goals; the `TokenReader` trait is the documented escape hatch for anyone who truly needs
catcode-like tokenization.

---

## 9. Implementation phases

Each phase ends with `cargo build && cargo test` green and that layer documented. No phase
starts until the previous layer's API is discussed and settled.

- **Phase 0 — decisions & doc hygiene.** Resolve [DECISION 1–6]. Consolidate documents (§10).
- **Phase 1 — `source` + `error`.** Rewrite per §L0; port the good tests from current
  `source.rs`; provenance, resolver, `LineIndex`, diagnostics types, recovery-token types.
- **Phase 2 — `token`.** `Span`, `Token`, `TokenKind`, `TokenReader` trait, `StdTokenReader`
  driven by a hardcoded-for-now `TokenRules` value; delimiter prefix table; exhaustive tokenizer
  tests (port pylatexenc's tokenizer test cases).
- **Phase 3 — `state`.** `ParsingState<L>` + `StateData`/`TokenRules`, `ParsingStateDelta` +
  `derived()` + `Lang::finalize_transition`, `Lang` trait with a test-only minimal lang
  (exercising events and a finalize customizer, including the override-policy idioms).
- **Phase 4 — `spec` + `library`.** `CallableSpec` (de-keyed), `StdCallableSpec`,
  `ArgumentStructureSpec` + `SlotStructureSpec`, `CallableTypeId` interning,
  `Library`/`LibraryStack`/`SpecLookup` + per-type unknown-fallback policy.
- **Phase 5 — `node`.** Flat `NodeTree`, `NodeKind<L>`/`CallableData`, `TextContent`, ext
  bundle (`NodeExtTypes`, `SimpleLang`), `NodeRef`, builder used by the session;
  `materialize()`.
- **Phase 6 — `constructs` + `engine`.** `ParseContext`, `NodesParser`, group/comment/callable
  parsers, `ArgumentsParser` + `SlotsParser`, `Language<L>`/`ParserSession`/`ParseResult`;
  pin down the whitespace/span invariants of §L4; end-to-end tests on real LaTeX snippets.
- **Phase 7 — `latexlike` preset.** Environments via `\begin`/`\end` specs, math group types +
  mode-aware lookup, verbatim, std library; tolerant-parsing behavior tests; port a slice of
  pylatexenc's walker test suite as acceptance tests.
- **Phase 8 — FLM spike.** Minimal `Flm` lang in a scratch crate exercising: custom `StateExt`,
  custom node payloads, custom invocation parser, resolver, post-processing traversal. This
  validates goal 3 before FLM proper begins.

---

## 10. Documentation hygiene

Too many overlapping documents; several are stale or superseded. Proposal:

**Keep, as living documents:** `ARCHITECTURE_PLAN.md` (this file → becomes `ARCHITECTURE.md`
once decisions land), `NAMING_STRATEGY.md` (update per §7), `SOURCE_ARCHITECTURE.md` (referenced
by §L0; eventually folded into `ARCHITECTURE.md`), `CLAUDE.md`, `README.md`, `TODO.md`.

**Archive to `docs/archive/`** (history, no longer authoritative): `TRAIT_BASED_ARCHITECTURE.md`,
`TRAIT_ARCHITECTURE_QUICKREF.md`, `ALIGNMENT_AUDIT.md`, `PROJECT_SUMMARY.md`, `QUICKSTART.md`,
`DEVELOPMENT.md`, `PARSING_STRATEGY.md` (its decisions are absorbed here),
`pylatexenc_to_rust_strategy.md` and `PROPOSALS.md` (keep accessible as pylatexenc feature
inventory + TeX gap analysis).

A short `DECISIONS.md` log (date, decision, alternatives, why) would prevent the next
"coming back after a while" from requiring this level of archaeology.

---

## 11. Collected decision points

1. **RESOLVED (July 2026): materialized state + transition choke point ("Option C").**
   Stored `StateData`/`TokenRules` behind getter-only public surface; `ParsingStateDelta<L>`
   overrides-struct (+ `L::Event`s) as the reified change value; `derived()` as the sole
   constructor of non-initial states; `Lang::finalize_transition` as the customizer for
   cross-cutting rules. Replaces the per-facet trait + macro design in `src/state/`.
   (Design: §L2. Rationale, including rejected Options A and B: §4.)
2. **Naming:** `Lang` trait + `Language<L>` runtime object (dropping `FLMEnvironment` and
   `LanguageSpecification`). (§7)
3. **RESOLVED (July 2026): unified `Callable` node kind + two-tier ext + `TextContent`
   ("Option F").** Closed structural `NodeKind<L>` (`Chars`/`Group`/`Callable`/`Comment`/
   `List`, no `Custom`); Macro/Environment/Specials merged into `Callable` with interned
   `CallableTypeId`; de-keyed specs, never-`None` via per-type fallback singletons; owned
   names, `TextContent` content, `post_space` kept; args/slots as two named concepts over
   shared machinery; whitespace-as-chars-nodes + exact sibling-span partition; recomposition
   levels 1+2 as stated requirements constraining `ArgsLayout`. **No core `MathNode`** (math =
   group types + preset state ext). (Design: §L3/§L4. Rationale: §4b.)
4. **Defer `Rc`/`Arc` genericity**; `Arc` behind an internal alias for now. (§5)
5. ✅ **DECIDED (July 2026): zero mandatory dependencies.** Drop `thiserror` (hand-written
   `Display`/`Error` impls — our errors need bespoke span/provenance rendering anyway, so the
   derive only covered the trivial part) and drop `log` entirely (library conditions surface
   through the diagnostics sink / `ParseResult`, not a logging side channel; can be reintroduced
   later as an optional feature if internal tracing proves useful).
6. **Library resolution = ordered stack with lexical shadowing** (no `ConflictStrategy`,
   no built-in mode tables; mode-awareness via `SpecLookup` receiving the state). (§L3)
7. **Rebuild `src/` layer-by-layer** per §9 rather than repairing the current tree in place
   (salvaging: prefix-table logic, `detect_*` decomposition, recovery tokens, source tests,
   line-index logic).
