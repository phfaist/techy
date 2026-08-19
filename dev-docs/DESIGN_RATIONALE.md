# Design Rationale & Decision Log

**Status: LIVING DOCUMENT.** Companion to [ARCHITECTURE.md](ARCHITECTURE.md).

Where ARCHITECTURE.md says *what* the architecture is, this document records *why* — the
arguments, trade-offs, and rejected alternatives behind each decision, plus questions still
open. Its purpose is to let a future session (human or agent) pick up design work without
re-deriving or accidentally re-litigating settled arguments, and without mistaking open
questions for settled ones.

---

# How to use and maintain this document [§dd-dr:self-meta]

**For agents and future sessions:**

- Check an entry's **Status** before touching related code: `DECIDED` means don't re-litigate
  without new evidence; `OPEN` means the user has not signed off — ask, don't assume;
  `DEFERRED` means intentionally postponed — don't implement it speculatively.
- Every entry has a **Revisit if** clause. If that condition arises, raising the issue is
  welcome; otherwise treat the decision as settled.
- When a discussion produces a new decision or overturns an old one, **append or amend an entry
  here in the same session** — with the rationale and the alternatives considered (no dates;
  see the maintenance rules). An undocumented decision will be re-argued from scratch in six
  months.
- Documentation precedence when documents conflict: this file and ARCHITECTURE.md >
  `dev-docs/extra/` notes > everything in `dev-docs/archive/` (frozen; not consulted by
  default). Newer beats older; user-authored beats generated.

**Maintenance rules** (the documentation system itself is specified in
`Documentation_Structure.md` at the repository root):

- Heading labels (`[§dd-dr:…]`, `[§dd-arch:…]`) are immutable addresses: never rename or
  reuse one. Before removing a label or retitling a labeled heading, run
  `git grep -n '<label>'` across the repository and retarget every citer first; a label may
  be retired only when nothing references it.
- Every decision entry — whatever its status — must be referenced at least once, at a
  relevant location, from ARCHITECTURE. Add that reference in the same change that adds the
  entry. Periodic check: for each `[§dd-dr:…]` heading label in this file, grep
  ARCHITECTURE.md for it; every miss is a gap to fix.
- Status lines carry who/context, never dates. Dates appear only inside explicitly recorded
  reversal notes, where preserving the history of a reversed decision is the point.
- Cross-references are bare bracketed labels (`cf. [§dd-dr:panic-policy]`) — no file paths,
  no section numbers. User-facing documentation (rustdoc text, guide pages) never references
  the developer documents; the triage rules for such situations live in
  Documentation_Structure.md.

**Process rules for this project** (from CLAUDE.md, restated because they govern design work):

- The user wants a high degree of control over design decisions. Propose with rationale and
  clearly marked decision points; do not silently decide.
- Never remove or "fix" user-written experimental code without confirming intent — argue
  against it in writing first (see [§dd-dr:parsing-state] for the worked example of how this went well).
- Most of the codebase is provisional; docs describing decided architecture are more
  authoritative than code that hasn't been reviewed yet.

**Content rules** for what can appear in this document:

- **Short, self-contained descriptions of policy and rationale**: To-the-point;
  explanations that are not important are entirely omitted.
- **No history**: No trace of history of decisions, other than through rejected
  alternatives.
- **No reference to plans or execution phases on the main branch**: On the `main`
  branch, this document **NEVER** contains references to temporary project plans
  or execution phases.  (Agents may introduce such references on a temporary
  basis, on a work branch, to facilitate their work; they MUST remove them before
  merging into `main`.)

---

**Entry template for future decisions.**

```
#### <Short decision title> [§dd-dr:<short-label>]

Status: <DECIDED|PROPOSED|OPEN|DEFERRED> (<who/context — no date>).

<The decision, one or two sentences, followed by the argument that carried it —
especially the one decisive reason.>

Rejected alternatives: <alternatives considered, each with its killing flaw.>

Revisit if: <concrete condition under which reopening is warranted.>
```

Keep entries short and argumentative — the goal is that a future reader can reconstruct *why*
without replaying the conversation. Record the decisive reason, not every reason.


# Implementation design principles [§dd-dr:impl-design-principles]

## Project-level goals and constraints [§dd-dr:goals]

These are the fixed points everything else serves (user-stated):

1. **Flexibility** — minimal hard-coded decisions in the core.
2. **Extensibility** — custom parsers, token readers, specs, node payloads without forking.
3. **FLM target** — the [FLM project](https://github.com/phfaist/flm) will be redesigned on top
   of this library. Every core design must pass the "can FLM do X through public extension
   points?" test.
4. **Low footprint** — minimal dependencies, small compiled artifact.
5. **No-compromise quality** — clean logical structure preferred over expedient shortcuts;
   clean slate, no pylatexenc backwards-compatibility baggage.
6. **Openness to revision** — better arguments win, including against the user's own earlier
   experiments; but through discussion, not unilateral change.

Corollaries: `Result<T, E>` everywhere and no panics in library code; tests accompany
functionality; public APIs documented with examples; no over-engineering or premature
optimization (a goal in tension with 1–2; the principles below resolve the tension).

---

These heuristics resolve most individual decisions in the register. When facing a new design
question, try these first.

## Data where values change at runtime; traits where behavior changes [§dd-dr:data-vs-traits]

The single most load-bearing principle. Anything a *state delta* may need to change
mid-parse — delimiters, escape characters, enabled features, specials strings — must be plain
data in the parsing state. A value behind a compile-time associated type cannot be changed by a
runtime delta, so facet-traits for such values are structurally wrong, not merely inelegant.
Traits are reserved for genuine behavior extension points: `TokenReader`, `ConstructParser`,
`SpecsProvider`, `SourceResolver`, `CallableSpec`.

Litmus test for "should X be a trait?": *could two implementations differ in control flow, not
just in the values they return?* If they only differ in values, X is data.

## One generic parameter, defaults everywhere [§dd-dr:one-generic-param]

Generic customization is bundled into a single `Lang` trait with associated types, threaded as
one `L: Lang` parameter. History shows why: two prior designs independently drifted into
parameter proliferation (a 9-parameter `ParsingStateData` struct in the WIP code; `Parser<N, S,
C>` in the generated trait-architecture doc). Proliferation is the natural failure mode of
Rust generics and must be resisted structurally, not by discipline. Simple users must be able
to write code with zero visible generics (ZST preset lang + type aliases).

## No privileged language concepts in the core [§dd-dr:no-privileged-concepts]

The engine knows nothing of math mode, `{`/`}`, `%`, or `\`. pylatexenc hard-codes
`in_math_mode: bool` into its core `ParsingState`; techy deliberately does not — "math mode" is
a preset-level state extension, `$…$` is just a configured group type, and mode-aware definition
lookup happens because `SpecsProvider` lookups receive the parsing state. Rationale: this is what
makes the library a *toolkit for LaTeX-like languages* (and a viable FLM substrate) rather than
a LaTeX parser with escape hatches.

Guard rail: any proposal that adds a language-specific field to a core type should instead add
it to a preset's `StateExt`, `NodeData`, or library definitions.

## Closed structural core, open payloads [§dd-dr:closed-core-open-payloads]

The set of *structural shapes* (chars, group, callable invocation, comment, list) is a closed
enum; extensibility lives in payloads (the two-tier ext system of `Lang::NodeExts` — no
`Custom` variant; cf. [§dd-dr:flat-node-tree]), specs (trait objects chosen at definition
time), and state extensions. Rationale: exhaustive pattern matching and serializability are
user priorities; `Box<dyn Node>` + downcasting sacrifices both to gain a kind of openness
nobody needs (new *structure* is rare; new *semantics* is common, and semantics attach to
payloads and specs).

## Zero-copy by default [§dd-dr:zero-copy]

Tokens and nodes reference source content by byte spans; owned `String`s appear only where
content genuinely differs from any source slice (synthesized content). Transient borrows held
by a token reader over the source it is scanning are fine as long as they never enter the AST.

## Deterministic dispatch over registry scanning [§dd-dr:deterministic-dispatch]

Parsing dispatch follows data: token kind → construct parser, name → library lookup → spec →
invocation parser. Never "ask every registered parser if it can_parse() and pick by priority" —
that design makes behavior depend on registration order and hides dispatch logic in scattered
predicates. If syntax needs to enter the pipeline, it enters as data (a specials string, a
group type, a spec) or as an explicit replacement of a well-defined component.

---

## Non-goals [§dd-dr:non-goals]

Decided intentional limitations:

- **techy is not a TeX engine.** No catcode system, no macro expansion engine, no conditional
  (`\if…`) evaluation, no full primitive set. Target use cases are structural parsing for
  conversion, analysis, and tooling — pylatexenc's niche, and FLM's need.
- Escape hatch, documented: anyone needing catcode-like tokenization implements `TokenReader`.
- `\newcommand` **is** supported, but at the parse level (a library-extension delta defining a
  new spec), not as token-stream expansion.
- Deferred: memory-mapped sources (an embedder can already hand in mmap-validated text;
  the `SourceContent` trait seam was retired as information-equivalent to `&str`, [§dd-dr:sources-and-spans]),
  streaming/incremental parsing, `Rc` pointer genericity ([§dd-dr:generics]).

---

# Decision register [§dd-dr:decisions]

Format: **Status** (DECIDED / PROPOSED / OPEN / DEFERRED) · decision · why · rejected
alternatives · revisit-if.

## Sources and spans [§dd-dr:sources-and-spans]

#### Arc-based source ownership [§dd-dr:arc-source-ownership]

Status: DECIDED (user-led design discussion).

Nodes carry `SourceSpan { Arc<Source>, start, end }`; specs and parsing states are likewise
`Arc`-wrapped in nodes. The decisive argument is **post-processing**: tree transformations
produce new trees mixing old and new nodes, and Arc makes nodes self-contained — transformed
trees outlive the original `ParseResult`, with no lifetime chains across N transformations.
Cost: ~1ns refcount bump per node — negligible.
Rejected alternatives: `SourceId` + store (circumvents borrow checking; id meaningless without its store);
lifetime `'src` on all AST types (self-referential struct problem + transform chaining);
index-based spans with a `SourceStore` in `ParseResult` (ties nodes to one result).
Revisit if: profiling ever shows Arc traffic mattering (then see [§dd-dr:generics] pointer genericity).

#### Provenance lives on `Source`, not on every location [§dd-dr:provenance-on-source]

Status: DECIDED (implemented; formerly proposed).

`SourceProvenance` (`Primary`/`Resolved`/`Synthesized` with `triggered_at: SourceSpan`) is one
hop per *source*, forming a provenance tree walkable for error reports. The WIP code's
per-location `via: [SourceLocationVia]` vector paid per-token/per-node cost for information
that is constant per source. Removing it also structurally prevents Arc cycles: the invariant
is *source types never reference node types* (reference graph strictly layered:
nodes → sources/specs/state; sources → sources) — later generalized to the crate-wide
acyclic-runtime-ownership rule (cf. [§dd-dr:three-strata]).
Revisit if: a use case needs per-node provenance distinct from its source's provenance
(e.g. token-level macro-expansion tracing à la TeX).

#### Source→triggering-node mapping lives in a session-owned registry [§dd-dr:source-node-registry]

Status: OPEN (general direction decided, details open).

The reverse question "which node triggered this synthesized/resolved source" is answered by a
higher-level registry owned by `ParserSession`, keeping track of the synthetic sources and the
nodes that created them. How the registry refers to nodes, and its exact lifecycle, are to be
decided (not plain `NodeId`s).
Rejected alternatives: recovering the node by O(n) span search over the tree — works, but an implicit,
lossy lookup where an explicit owned mapping is cheap and direct.

#### Line/column is a lazy, standalone utility [§dd-dr:lazy-line-col]

Status: DECIDED.

The parser works purely in byte offsets; `LineIndex` computes line starts lazily and only for
display (errors, diagnostics).
Rationale: upfront line indexing costs O(source) on every parse for data usually never read.
Ownership layering — the persistent `LineIndexCache` and the `LineColProvider` seam — is
[§dd-dr:line-col-ownership]; this entry's doctrine is unchanged.

#### Pluggable content resolution [§dd-dr:source-resolver]

Status: DECIDED.

`SourceResolver` trait for `\input`-like lookups. No file-system resolver is shipped
(no_std policy, [§dd-dr:dependencies]): an embedder implements `SourceResolver` on its
side, where the I/O capability lives; the in-memory `MapResolver` covers tests and fully
preloaded setups. The resolver instance lives on the `ParseDriver` (parse-time instance
behavior), behind `with_source_resolver` and the `ParseDriver::source_resolver` accessor;
"resolves nothing" is the accessor's `None`. A dedicated always-fail `NoResolver` type
was rejected: it adds nothing over `None` plus an empty `MapResolver` for
deterministic-failure tests ([§dd-dr:public-visibility-sweep]). Engine wiring:
[§dd-dr:input-wiring]; attachment rationale: [§dd-dr:input-attachment]. (The
`SourceContent` backing-abstraction half this entry originally carried was retired —
cf. [§dd-dr:source-cursor-retired].)

#### Origin genericity without `Lang` [§dd-dr:origin-genericity]

Status: DECIDED (user).

`Source<O: SourceOrigin = Option<String>>` takes the origin type as a plain, defaulted type
parameter; `SourceSpan`/`SourceProvenance`/`SourceResolver`/`Diagnostic` carry the same
parameter. The S1 core plugs `L::SourceOrigin` into this parameter — the source topic never
depends on `Lang`, per the Lang-free-foundation rule (S0; cf. [§dd-arch:arch]).
The `SourceOrigin` trait provides only `label()` (diagnostics display) on top of
`Debug + Clone + Default`. The default origin type is `Option<String>`: conventionally the
URL the content was obtained from, `None` when unknown or when the content was synthesized.
The division of labor: origin is optional *display metadata about where content was
obtained*; `SourceProvenance` — which every source carries — is the *structural* record of
how it entered the parse, and it (not the origin) holds synthesis descriptions and
resolution references. One inference consequence of the defaulted parameter: bare
`Source::new(…)` cannot infer `O`, so simple usage annotates (`let src: Arc<Source> = …`)
until preset type aliases make it moot.
Rejected alternatives: a concrete-now/genericize-later approach (would retrofit a type parameter
through every S0 signature). Also rejected, in a recorded revision: the first-cut
`StdSourceOrigin` enum (`Unknown` / `Named { name, kind: File | Snippet | Resolved |
Synthesized | Other }`). Its kind taxonomy was too detailed and too rigid for the intended
generality (where does content fetched from a database fall?), it partially duplicated
provenance (`SourceOriginKind::Resolved` vs `SourceProvenance::Resolved` answered the same
question twice), and the `File` kind clashed with the no_std policy ([§dd-dr:dependencies]). The trait's
`synthesized()`/`resolved()` origin constructors went with it: generic machinery no longer
*mints* origins — a source starts with the default ("unknown") origin, and a creator that
actually knows a URL attaches it via `with_origin`.

#### `SourceContent` is a trait boundary, not (yet) a `Source` parameter [§dd-dr:source-content-boundary]

Status: DECIDED (user; superseded — retired outright, cf. [§dd-dr:source-cursor-retired]).

The trait exists (implemented by `str` and `String`) and
`SourceCursor<'s, C: SourceContent + ?Sized = str>` is generic over it, but `Source` stores a
concrete `String`, with all content access behind methods so the backing can later become
generic (mmap) without changing the public API. Explicitly: keep the enabling pattern, do not
implement mmap until a real need.

#### `SourceCursor`, `Source::cursor()`, and `SourceContent` retired [§dd-dr:source-cursor-retired]

Status: DECIDED (user; reverses [§dd-dr:source-content-boundary], July 2026 — recorded as a
conscious reversal).

The intended consumer went another way: `StdTokenReader` holds `content: &'s str` and scans the `str` directly. Its access
pattern is random-access slicing at arbitrary offsets (`starts_with` at a position,
`find('\n')` from a comment start, longest-match through the `PrefixTable`, whitespace
look-ahead past the current position) — which the cursor's position-local
char-at-a-time primitives (`peek_char`/`next_char`/`advance`/`mark`/`rewind`) do not
serve — and the `TokenReader` protocol needs deliberately *bidirectional*
repositioning (`move_to`/`move_to_position`: a recovery's resume position moves
forward, an absent-argument rewind moves back, [§dd-dr:stream-position]), which the
backward-only, debug-asserted `rewind` actively resists. `SourceContent` fell with the cursor: as designed
(`slice(&self, Range) -> &str` over contiguous valid UTF-8) it is
information-equivalent to `&str` — a UTF-8 memory-mapped file can be handed in as text
by the embedder after one validation pass, so the trait enabled no future it claimed
to; only a genuinely *chunked/streaming* backing would be new, and that is a different
reader design, not a backing swap behind `Source`. `Source` keeps its plain `String` field
with access behind methods.
Rejected alternatives: re-labeling the cursor as an embedder convenience for custom `TokenReader`s
(nothing needs it, and `&str` + `usize` is simpler than a bespoke cursor API).
Revisit if: a genuinely streaming source materializes — design the chunked reader
then, with a content abstraction shaped by its real requirements.

#### `Span` has private fields; in-place growth is the monotone `extend_to` [§dd-dr:span-extend-to]

Status: DECIDED (user).

`Span`'s `start`/`end` are private with `start()`/`end()` accessors, closing the gap where
the `start <= end` invariant was only advisory (`new` debug-asserts; public fields allowed
silent violation). The one mutation pattern the lib actually uses — growing a
chars-run/marker span rightward — is `extend_to(end)` (debug-asserted monotone), so every
mutator preserves the invariant. `cover(other)` is the byte-range union (min/max — order-
and overlap-agnostic). Consistency with `SourceSpan` (private + accessors + validating
constructor) decided it over the honest alternative, the `std::ops::Range` precedent
(public fields, no invariant). Empty-span-sensitive predicates arrive only with a
consumer — whichever empty-span semantics they pick will be silently depended on, so each
is pinned by docs + tests in the same commit: `contains(pos)` exists (an empty span
contains nothing; its consumer is `node_at`, [§dd-dr:tree-navigation]), `overlaps` is
deliberately not added. Bridging: `SourceSpan::new` accepts `impl Into<Range<usize>>`
(so a `Span` passes directly; `From<Span> for Range<usize>`) and `SourceSpan::span()` is
the inverse — `span.rs` itself stays ignorant of `SourceSpan` (dependency direction
preserved).

#### `SourceResolver` contract batch: content-returning, `Send + Sync`, no core recursion checking [§dd-dr:resolver-contract]

Status: DECIDED (user; settled before any consumer existed).

- **`resolve()` returns `ResolvedContent { content, origin }`; the caller mints the
  `Source`** (the `resolve_source_reference` composition). Rationale: provenance lives on the
  `Source` (`Resolved { reference, triggered_at }`) and diagnostics self-render include
  chains from it, so a resolver-cached `Arc<Source>` shared across two include sites
  silently renders the wrong chain inside the second inclusion. Returning content makes
  the trap *unrepresentable* — provenance never passes through implementor hands, and
  resolvers may cache content freely. (Content duplication per include site is inherent
  while `Source.content` is a `String`; switching that private field to `Arc<str>` later
  would remove it without touching this contract.) Rejected alternatives: a documented
  fresh-`Source`-per-resolve contract (an implicit rule a cache silently violates).
- **`Send + Sync` supertraits**, matching every other stored extension trait: resolvers
  live in the long-lived shareable language bundle, and `resolve(&self)` means caching
  needs interior mutability — the bounds pick the thread-safe form (locks/atomics, not
  `RefCell`), stated before implementors exist.
- **Recursion is the embedder's job.** The core never interprets reference strings
  (no path semantics, no canonicalization) and performs no recursion checking — held up
  even against the engine recursing on its own stack ([§dd-dr:input-wiring]), because
  `.dtx`-style legitimate self-inclusion exists. The embedder's policy tools are
  [§dd-dr:include-chain-helpers]. Documented on the trait.
- **`ResolveError` = strings + optional structured cause**: human-readable
  `reference`/`message` stay the primary interface (a failed `\input` flattens into a
  diagnostic anyway); an optional `Arc<dyn core::error::Error + Send + Sync>` cause
  travels the standard `Error::source()` chain so embedders can downcast (e.g.
  `io::Error` kind). Principle recorded here: **techy error types stay uniformly
  `Clone`; out-of-crate information sits behind the `Arc`** (`with_cause` wraps with
  `Arc::new`).
- Smalls: forwarding impls (`&R`/`Box<R>` only — an `Arc<R>` forwarding impl would
  overlap the sealed `IntoSourceResolver` conversion's no-double-wrap `Arc`
  pass-through impls), a compile-time object-safety pin
  (drivers may store `Arc<dyn SourceResolver>`), `MapResolver::with_reference_as_origin`
  (its blanket impl narrows to `O: From<String>` — a convenience type may narrow;
  exotic origins write their own ten-line resolver).

#### Include-chain tools: `including_sources` + `check_include_chain`; recursion stays embedder policy [§dd-dr:include-chain-helpers]

Status: DECIDED (user, API-review session).

Recursion/cycle control for `\input`-style inclusion stays OUT of core — reaffirmed
against the new fact that the engine now recurses on its own stack
([§dd-dr:input-wiring]): legitimate self-inclusion exists (`.dtx` self-documenting
files), so any core depth/cycle mechanism would fight real documents; deep group
nesting is equally unbounded today, and references stay uninterpreted
([§dd-dr:resolver-contract]). Instead, two source-model tools make the embedder's
policy a one-liner:

- **`Source::including_sources()`** — iterator over the chain of including sources
  (self → primary, following `provenance().triggered_at()` hops): the general
  primitive under every cycle/depth/counting policy (`.any(…)`, `.count()`,
  `.filter(…).count()` for `.dtx`-style bounded self-inclusion). The existing
  `provenance_chain()` yields the provenance *records*; this yields the *sources*,
  whose origins carry the comparable names.
- **`check_include_chain<O, K: PartialEq>(target_key: &K, triggered_at:
  &SourceSpan<O>, origin_key: impl Fn(&O) -> Option<K>, max_depth: Option<usize>)
  -> Result<(), ResolveError>`** (home: the source topic, public path `techy::source`) —
  the canned
  cycle-plus-depth check a resolver calls with `?`. Keying design:
  compare **origins**, not provenance reference strings — the primary source
  participates (the embedder mints it with a suitable canonical name); the caller
  passes the already-canonicalized target key (the resolver computes it during
  resolution anyway); `origin_key` is a cheap conversion when the resolver mints
  canonical origins (documented invariant); `None` keys are skipped. Distinct
  messages for cycle vs depth-exceeded.

A `techy::helpers` recipes module was REJECTED (the `util` vague-name problem;
placement stays by logical function — adding a module later is additive, while
dissolving a grab-bag is breaking).

Rejected alternatives: a core include-depth knob + condition on the sub-parse door
(bounds core's stack but fights `.dtx`-legitimate recursion; dropped with its
reserved identifier); provenance-chain cycle *detection* in core (references are
opaque strings — two spellings can name one file; false confidence, and it is
reference interpretation in the contract's sense); keying the check on provenance
reference strings (blind to the primary source).

Revisit if: a class of embedders demonstrably cannot canonicalize origins (a
reference-keyed variant could then be added beside, not instead).

#### Line/col ownership: consumer-held `LineIndexCache` + the `LineColProvider` seam [§dd-dr:line-col-ownership]

Status: DECIDED (user, API-review session).

Who computes and caches line/column stays LAYERED, never the `Source`: the parse
computes nothing ([§dd-dr:lazy-line-col] holds); the diagnostics renderer uses a
transient per-call `LineIndexCache` — one mechanism;
**persistence belongs to whoever holds a `LineIndexCache<O>`** — a
new public cache in the source topic holding one owned line-starts table per
source, keyed by `Arc` identity (entries own `Arc<Source>` + `Vec<usize>`, not the
borrowing `LineIndex` view). Because source content is immutable, an entry never
invalidates, and a tool that keeps its own `Arc<Source>` across parse attempts
(the span-stability doctrine) keeps its cache valid for free. The
`&self`-vs-`&mut` question dissolves: consumer-held means `&mut` is honest;
cross-thread sharing is the consumer's own lock on the std side — techy buys no
no_std synchronization.

**`LineColProvider`** (name over `LineIndexCacheProvider` — the trait provides
line/col *answers*, not caches): single method `line_col(&mut self, source,
offset) -> Option<(usize, usize)>`; implemented by `LineIndexCache`; the rendering
entry points have `_with(&mut impl LineColProvider)` variants, the no-argument
forms remaining as transient-cache shorthand (shorthand-not-second-path). Editor
tools with incremental line tables — surviving per-keystroke re-parses that mint
new `Source`s — plug in without recomputation: the Arc-keyed cache's
edit-invalidation limit is answered at the right layer.

Query-surface additions ruled with the ownership: `LineIndex::line_of(offset) ->
Option<(usize, Range<usize>)>` (line number + byte range — the caret/underline
path; the inverse `line_range(line_no)` skipped — no demonstrated consumer,
additive later); `line_col_span(impl Into<Range<usize>>)`; `DEFAULT_MAX_SCAN_LEN`
is 500 000 (bounded; the loud docs on silent `None` past the bound stay).

Rejected alternatives: a `Source`-owned lazy cache (blocked dep-free — `alloc` has
no `Mutex`, `OnceCell` costs `Sync`; recorded at the renderer cache since its
introduction); `Source` as a `Lang` trait or a `SourceAnalyzer` associated type on
`Lang` (the source model is deliberately Lang-free — [§dd-dr:origin-genericity] is
load-bearing for Lang-free rendering and tooling; and precompute/lazy/incremental
are strategies of one pure function — no consumer is generic over them);
per-node/per-span `line_col()` methods (hidden per-call index build, O(k·N); the
bind-the-`Arc` one-index pattern is the guide example); a shipped caret/underline
renderer (a ~10-line hand-roll once `line_of` exists — presentation policy stays
out of the library; `format_position`'s output shape is documented as not a
contract); non-`&mut` `LineIndex` (interior mutability for a transient local).

Revisit if: a no_std embedder needs shared lazy indexing (the provider seam is
where a lock-free implementation plugs in).

## Tokens and tokenization [§dd-dr:tokens]

#### Tokens are minimal and structural [§dd-dr:minimal-tokens]

Status: DECIDED (user; sharpened by the token-design review).

A token identifies *what to parse next*
(single char, group open/close, command, specials, comment, paragraph break, end of
stream) and nothing more. Notably there is **no `BeginEnvironment(name)` token** —
`\begin` is an ordinary command token, and environment recognition is a construct-parser
concern (preset registers `\begin`/`\end` specs). This is a deliberate departure from
pylatexenc, whose tokenizer bakes in environment syntax.
Rationale: keeps the tokenizer language-agnostic ([§dd-dr:no-privileged-concepts]) and moves all semantics to the
spec/parser layer where it is extensible. ("Minimal" bounds *language knowledge*, not token
extent: a whole-comment token is fine because comment interiors carry no structure the
parser cares about — see the review entry below.)

#### The token-design review: final token model [§dd-dr:token-model]

Status: DECIDED (user-led, three-round design review).

Supersedes earlier proposals — uniform `post_space`, maximal-run `Chars`,
`Ok(None)` at end of stream — each recorded below as rejected.
Final model: a token records its kind, the byte range it covers and the whitespace
preceding it, and the kinds are `Char(char)` | `GroupOpen`/`GroupClose` |
`Command { name, escape_char }` | `Specials { callable_type, name, spec }` |
`Comment { start_delim, content }` | `ParagraphBreak` | `EndOfStream` — the taxonomy the
reader reports through its `TokenKind` view, since a token itself is opaque to parsers
([§dd-dr:token-opacity]). The decisions, each with the argument that carried it:

- **No invocation forms at the token level.** No macro/environment/specials taxonomy and no
  `CallableTypeId` on tokens: `\begin` is a `Command` exactly like `\foobar`; which names
  are macros or environments is resolution *output*, assigned at parse time by the preset.
  Dropping the type id from tokens dissolved the "token says MACRO, node says ENVIRONMENT"
  wart outright. Terminology stack: *command* (token-level syntactic form; TeX lineage) →
  *callable* (parse-level concept; [§dd-dr:flat-node-tree]) → *macro*/*environment*/*specials*
  (preset-level invocation flavors). "Command" over "escape": a future non-escape command
  syntax (`@MARKER@`-style, a possible `CommandRule` extension) would make "escape" a
  misnomer, and "escape token" wrongly connotes escaped-character semantics (`\&` as
  literal `&`). The rule is scoped to tokens whose resolution happens at parse time —
  `Command`. The `Specials` token, whose recognition **is** resolution (next-but-one
  bullet), carries the resolved `callable_type` alongside its spec: a resolution is the
  `(callable_type, spec)` pair — `ResolvedCallable`'s exact shape — and the dispatch
  loop needs both to build an `Invocation`; both fields come from the single scan-time
  resolution site, so the "token says MACRO, node says ENVIRONMENT" wart cannot re-arise.
- **Single-character `Char` tokens** (reverses the earlier maximal-run design). A token is an
  atomic unit of parsed thing, and construct parsers may need char-by-char reading
  (tabular preambles); with runs, a parser wanting one char must split a token or reach
  into its middle. Deletes the conservative stop-set machinery and its "two adjacent
  `Chars` tokens" wart; restores pylatexenc parity. Chars accumulate into nodes at the
  node level, so no downstream cost.
- **Two callable-trigger kinds, split by production mechanism.** `Command` is recognized
  from `CommandRule { escape_char, name_chars }` *data* (several rules may coexist;
  earlier-entry wins on escape-char conflict); `Specials` is recognized by the
  `Lang::scan_specials` *hook*. The split is honest: one mechanism is delta-changeable
  rules data on the hot path (`\makeatletter` changes `name_chars` via generic
  `TokenRulesOverrides`), the other is open-ended, library-driven recognition. The escape
  fires unconditionally — `\undefinedname` is a `Command` token; unknown names resolve at
  parse time to per-form fallback specs — whereas specials fire only on recognized
  strings. `\foobar␣` is **one token** (not a bare `\` trigger token): the scan must read
  the name anyway, peek-level stop conditions (`\end`) need it, and bare triggers would
  leave name bytes covered by no token.
- **Specials: recognition = resolution, owned by the preset.** Specials trigger sets can be
  large and change with library pushes (pylatexenc defers them to the latex context for
  the same reason), so they are *not* enumerated in `TokenRules`. `Lang::scan_specials`
  returns a `SpecialsMatch` carrying the full resolution in one call — `callable_type` +
  spec, the `ResolvedCallable` pair, with the name being the matched text itself
  ([§dd-dr:specials-scan-errors]): scanning/lookup normalization or scoping mismatches are
  impossible by construction, and unknown-name fallback is the scan's own business (a
  `Specials` token's spec is never absent). It is a
  `Lang` hook (the `finalize_transition` precedent), *not* a per-library protocol and
  *not* a swappable dyn object in the state: the hook receives the state, so it can
  dispatch on `ext` and pushed libraries — everything a swapped object could express,
  without a state field. Hot-path guard: `Lang::specials_trigger_chars(&StateData)`
  reports possible first characters (`TriggerChars`; `Any` = conservative fallback for
  dynamic scanners), cached per state instance like the `PrefixTable` and consulted before
  any dyn call. The scan answers `Result<Option<SpecialsMatch<L>>, SpecialsScanError>`; a
  scan error is a language-level condition the reader lifts, source-qualified, into an
  unrecoverable `TokenError`, never a recovery ([§dd-dr:specials-scan-errors]).
- **Syntactic vs. content whitespace** — the principle that decides every whitespace
  placement question: *pre-space is content whitespace* (belongs to the document flow;
  becomes whitespace chars nodes, [§dd-dr:nodes]), *post-space is syntactic whitespace* (consumed by
  the construct's syntax, ignored as content, reproduced verbatim in recomposition).
  Post-space exists only where *tokenization syntax* consumes whitespace — multi-character
  `Command` names (whitespace terminates the name) and `Comment`s (the newline terminates
  the content) — and is stored **in those variants**, not as a uniform `Token` field
  (the reader answers a token's post-space as the range between its `End` and
  `EndPastPostSpace` edges, so no uniform token field is needed —
  [§dd-dr:stream-position]). Groups
  never have post-space (space after `}` is content); specials and single-char commands
  (`\&`) neither. Spec-driven whitespace swallowing beyond this is a parse-level concern
  recorded on nodes.
- **One whitespace primitive: `skip_whitespace`.** With
  `TokenRules::enable_multi_newline_paragraphs` set (pylatexenc:
  `enable_double_newline_paragraphs`), skipped whitespace never contains `\n\s*\n` nor
  consumes a newline belonging to such a sequence — skipping stops *before* the sequence's
  first newline. Used identically for pre-space, command post-space, and comment
  post-space, so "post-space never crosses a paragraph break" holds everywhere by
  construction, and the paragraph-break token is detectable exactly where skipping
  stopped. The flag gates both the skip rule and `ParagraphBreak` emission — one
  phenomenon, one flag.
- **Whole-`Comment` tokens** (reverses the earlier delimiter-only `CommentStart`). The
  parser has no business inside comment content, so granular comment tokenization bought
  nothing; candidates for granularity (block/nested comments) are served by a future
  per-rule terminator extension of `CommentRule` or the `TokenReader` escape hatch.
  `CommentRule { start }` mirrors `CommandRule` (several syntaxes; longest start wins);
  the terminator is end-of-line implicitly, independent of `WhitespaceRules` (`'\n'`
  exactly — `'\r'` gets no special treatment, see [§dd-dr:token-contract-hardening], item 6). Corner
  pinned: `a% c\n\nb` — the comment's terminating newline belongs to a `\n\s*\n` sequence,
  so the comment takes **no** post-space and the `ParagraphBreak` survives as its own
  token (TeX-observable behavior: the blank line still yields `\par`). Consequence:
  `CommentParser` is vestigial — comment nodes come straight from tokens.
- **Terminal `EndOfStream` token; `peek` never returns an `Option`.** `EndOfStream` is
  idempotent and its `pre_space` carries the input's final whitespace, so trailing
  whitespace reaches the node tree through the ordinary token path — the nodes parser
  never reaches around the reader into raw content (which a custom `TokenReader` might not
  meaningfully expose). The dangling-escape-at-end recovery placeholder is
  `Char(escape_char)` — cf. [§dd-dr:token-contract-hardening], item 2.
- **The token topic is wholly S1; tokens are generic over `L`.** `Specials` carries
  `Arc<dyn CallableSpec<L>>` (tokens are `Clone`, not `Copy`), and `TokenError<L>` may
  grow state context. Tokens remain transient engine internals — the standard token
  carries no lifetime of its own ([§dd-dr:token-opacity]) — and the genericity never
  enters the AST. `Span` — a generic byte range used by errors and cursors
  independently of tokenization — moved to the source topic (S0). This supersedes the
  earlier "scanning core is S0" stratum split (cf. [§dd-dr:three-strata]);
  the S0-testability property was traded for the freedom to keep state context in token
  machinery, and a trivial test `Lang` restores testability at negligible cost.

*Rejected along the way (three-round arc: maximal abstraction → whitespace-as-token →
this hybrid):*
- *Unified `Callable` token kind absorbing Command and Specials* — hid that the two are
  produced by different mechanisms (rules data vs. preset hook) and dragged
  `CallableTypeId` into tokens.
- *`CallableTypeId` on tokens / on `CommandRule`* — invocation form is resolution output,
  not tokenization output. (Follow-up noted in [§dd-dr:open-questions]: with several `CommandRule`s, the
  parse-time lookup needs the escape char for disambiguation — pass the token.)
- *Whitespace as its own token* — killed by parser ergonomics: every construct parser's
  peek grows a "maybe whitespace first" case; the pre/post-space encoding localizes that
  cost in the tokenizer. (For fairness: it would have bought a token-span partition
  invariant, flag-free navigation, and a field-free `EndOfStream`.)
- *Uniform `post_space: Span` on every token* — post-space is a per-kind
  syntactic fact; the WIP's variant-embedded instinct was right, and the uniform field's
  only justification (a skip-post-space flag on the move) is served by the reader's
  named token edges.
- *Bare `\` trigger tokens with parser-side name scanning* — the scan reads the name
  anyway; name bytes would belong to no token; stop-condition checks would re-scan per
  peek.
- *Per-library trigger declarations with core-legislated shadowing* (`TriggerDecl` lists,
  cross-library longest-vs-innermost rules, first-char merge protocol) — preset-owned
  scanning made the entire mechanism evaporate from the core.
- *An empty-`Chars` EOF sentinel carrying final whitespace* — incoherent once `Chars`
  became `Char(char)`; the dedicated kind is the honest form.
- *Spec-carrying `Command` tokens* — commands need no lookup at token time; data-only
  tokens keep peeks cheap and the token stream fully `dbg!`-able.

#### A language declares its tokenization as one type [§dd-dr:tokenization]

Status: DECIDED (user).

`Lang` names one associated type for tokenization — `Tokenization: Tokenization<Self>` —
in place of the former pair `Lang::Token` + `Lang::StreamPosition` (both in
[§dd-dr:superseded-names]). The bundle is a lifetime-free zero-sized type declaring three
things: the token type its readers produce (`Tokenization::Token`), the type naming a
place in its token stream (`Tokenization::StreamPosition`), and a static factory
`make_token_reader(source) -> Box<dyn TokenReader<'s, L> + 's>` that builds the reader for
one parse over one source. The two types are spelled `Token<L>` and `StreamPosition<L>`
everywhere else — type aliases projecting through the bundle, following the
`NodeExt<L>`/`ArgumentExt<L>`/`SlotExt<L>` precedent. `StdTokenization` is the standard
bundle (`StdToken<L>`, `StdStreamPosition`, `StdTokenReader`), and what `TrivialLang` and
every language of this crate declares.

The decisive reason: with the reader named by the *language*, a driver no longer pins it,
so one driver implementation serves a language whatever its tokenization is.
`ParseDriver::make_token_reader` becomes defaulted (its body is
`L::Tokenization::make_token_reader(source)`), so every `ParseDriver` item is defaulted
again and `impl ParseDriver<L> for D {}` is a complete driver; `StdParseDriver` drops its
token/position bounds and serves every language; and `LatexlikeLang` no longer pins its
token and position types, so a latexlike language may plug a different reader in while
reusing `LatexlikeDriver`. The hook remains the **per-instance override**: a driver whose
reader needs configuration the driver instance holds overrides it, and a reader needing no
such data belongs on the language instead ([§dd-dr:token-reader-hook]).

Two bound rules follow, and swapping them does not compile:

- `StdTokenization`'s own impl is bounded `L: Lang<Tokenization = StdTokenization>`. The
  equality form there cycles through the very projection the impl is defining and trips
  the recursion limit (E0275).
- Every *other* site that requires standard tokens — `StdTokenReader`'s `TokenReader` impl
  and its scanning core, `TokenListReader`, the scan helpers of the construct-parser test
  suites — states the equality form
  `L::Tokenization: Tokenization<L, Token = StdToken<L>, StreamPosition = StdStreamPosition>`,
  never `Lang<Tokenization = StdTokenization>`. A language with a tokenization type of its
  own whose reader wraps an inner `StdTokenReader` (the documented
  reader-over-standard-tokens pattern) has `Tokenization != StdTokenization` but
  `Token = StdToken<L>`; the pinning form would shut it out.

The marker trait `Token<L>` is deleted with the change: its bounds
(`Clone + Debug + PartialEq + Send + Sync`) sit on `Tokenization::Token` directly, which
is symmetric with `StreamPosition` — that member never had a marker. The name `Token` now
belongs to the alias.

Rejected alternatives: a reader *type* on `Lang` (it would have to be a GAT, since a
reader carries the source lifetime, and that pushes `'s` into every token-holding type;
the `dyn` reader is needed regardless, [§dd-dr:token-opacity]); a constructor on
`TokenReader` (presumes source-only construction, which `TokenListReader` — built from a
token list — already refutes); keeping `Lang::Token`/`Lang::StreamPosition` *beside* the
bundle (two places would state the same fact, with nothing keeping them in agreement);
keeping the marker trait under a new name such as `TokenBase` (it carries no methods, and
`StreamPosition` shows a bound list needs no trait of its own); a supertrait carrying
`Token`/`StreamPosition` with a blanket impl, so that the `L::Token` spelling could stay
(verified not to work: the param-env candidate shadows the blanket impl and the projection
never normalizes — hence the aliases).

Revisit if: a language needs two tokenizations at once — the bundle is one type per
language by construction — or Rust gains a way to normalize the supertrait projection,
which would let `L::Token` return as the spelling.

#### Tokens are opaque; only their reader interprets them [§dd-dr:token-opacity]

Status: DECIDED (user-led, token-layer redesign).

A *token* is a value a construct parser holds and passes on but never reads. Its type is
the language's own — `Token<L>`, the member its `Lang::Tokenization` declares
([§dd-dr:tokenization]), whose bounds ask only for
`Clone + Debug + PartialEq + Send + Sync` — and the reader that produced it answers
the two questions a parser has about it: **what it is** — `TokenReader::token_kind`
returns the `TokenKind<'t, L>` *view*, the closed enum of kinds carrying the written
spellings as `&str` and no spans at all — and **where it is** — `source_span_of` /
`source_span_between` return a `SourceSpan`, `position_at` a stream position
([§dd-dr:stream-position]). `StdToken<L>`, the token of the standard reader, stores byte
ranges and `Arc`s only: no strings, no lifetime. It is built through eight public
constructors, one per kind, so that a custom reader can produce standard tokens; the
ranges it stores are readable only by this crate's own readers.

The decisive reason: a reader may serve one parse from more than one source — a reader
that substitutes a macro's definition into the stream as it reads is the motivating future
case — and then "which source do these offsets belong to" is known only to the reader. A
parser pairing numbers taken off a token with a source of its own produces a valid-looking
wrong location, and no signature says so. An unreadable token removes the possibility
instead of warning against it.

The view borrows the token and — for a reader that scans borrowed content — that content;
never the reader: `fn token_kind<'t>(&self, tok: &'t Token<L>) -> TokenKind<'t, L> where
's: 't`, with the receiver's lifetime deliberately absent from the return type. A
reader-borrowed view would keep the reader borrowed for as long as the view lives, and a
parser that has learned what its trigger is keeps that answer across a sub-parse which
borrows the same reader mutably.

A party that must interpret a token while holding no reader receives the token *and* a
shared reference to its reader, valid for that call: `ParseDriver::resolve_command`,
`CommandResolver::resolve_command`, `resolve_command_in_scopes`,
`TokenStopKind::Predicate`, `GroupChildState::Compute`, `FromInvocation::from_invocation`.
`Invocation` is the resolution result plus the token, with nothing cached beside it.
Definition lookup stays token-free: `CallableQuery` carries the invocation form, the name
and the callable syntax (the escape character, say), so scopes and packages never see a
token — a language that must dispatch on token detail does so in `resolve_command`, before
or instead of consulting the scopes.

Rejected alternatives: an `Arc<Source>` field on one shared concrete token type (a
refcount pair per token, and one token type cannot serve every reader — an expanding
reader's token is not a scanner's); a `Cow` of the source content on the token (the same,
plus copying); a span-only kind enum that parsers resolve against a source of their own
(the "relative to which source?" assumption again, and still unenforceable); a view
borrowing the reader (locks the reader for as long as the view lives, per the paragraph
above); a reader *type* on `Lang` in place of the `dyn` reader (loses object
safety — the context's `&mut dyn TokenReader`, the two-reader agreement suites — and a
reader type carries the source lifetime, so it would have to be a GAT; what the language
names instead is the lifetime-free `Lang::Tokenization` bundle, whose factory returns the
`dyn` reader, [§dd-dr:tokenization]); a cached view stored on `Invocation` (partial token detail
sitting next to the token it came from, and redundant since every consumer holds a reader
— [§dd-dr:invocation-parser-factory]); giving the reader-less hooks the view alone instead
of the token (a view is strictly less than a token plus its reader, so a language taking
over resolution would be limited for no gain); a reader reference stored *inside*
`Invocation` (the invocation is held across the parse that borrows the same reader
mutably — it does not compile).

*Amendment (user, span-tiling design session).* The motivating case — a reader serving one parse from
several sources — is now supported end to end: a language declares whether its parse trees
are span-tiled, and the parsers of one that does not obey span tiling assume nothing about
where tokens come from ([§dd-dr:span-tiling]).

Revisit if: a reader needs per-token data the bounds cannot express — that is a change to
`Tokenization::Token`'s bounds, not a return to readable token fields.

#### Stream positions are opaque and cannot be forged [§dd-dr:stream-position]

Status: DECIDED (user-led, token-layer redesign).

A *stream position* names a place in a reader's token stream. Its type is the language's
own — `StreamPosition<L>`, the member its `Lang::Tokenization` declares
([§dd-dr:tokenization]); for the standard reader `StdStreamPosition`, a byte offset behind
a private field. A parser obtains one only from the reader (`position_here`,
`position_at`) and gives it back to the reader (`move_to_position`, `source_span_within`,
`source_position_at`). There is no public constructor and no arithmetic, so a position
cannot be invented or shifted outside the reader that issued it; the test-only
`TokenListReader` additionally rejects tokens and positions it never issued
([§dd-dr:token-list-reader-demoted]), which is what turns the two-reader agreement suites
into a check against a parser inventing either.

Positions replaced the bare byte offsets the reader and the parsers used to exchange
(named in [§dd-dr:superseded-names]). One number did three jobs — a place in the text, a place in the
token stream, and a quantity to compare for "did the reader move?" — which coincide only
while one parse reads one source. A `SourceSpan` answers the first now, a stream position
the other two.

Navigation is `move_to(&token, edge)` and `move_to_position(&position)`, and nothing else.
`TokenEdge` names the five boundaries of a token in reading order:
`StartBeforePreSpace ≤ Start ≤ ContentStart ≤ End ≤ EndPastPostSpace` — `≤` rather than
`<`, because edges coincide where a token has no preceding whitespace, no leading marker,
or no trailing syntactic whitespace. `ContentStart` is where the token's own content
begins past its leading marker (after a comment's start delimiter, after a command's
escape character; `= Start` for every other kind); it exists so that a comment node's
three sub-spans — delimiter, content, trailing whitespace — are three reader answers
rather than arithmetic over the view's strings.

Positions compare with `==` only: no order exists across sources, and equality is what the
parse loops ask for. `TokenRecovery::resume` is accordingly a stream position, and the
requirement that it move the stream is checked by comparing the reader's position before
and after the move ([§dd-dr:resume-pos-contract]).

Rejected alternatives: bare `usize` offsets (forgeable, and silently paired with the wrong
source once a reader serves several); span-relative navigation with no position type (a
parser that must return to where it stood has nothing to name that place with — the
retired trick of synthesizing a zero-width marker token to move by is where that leads,
[§dd-dr:token-contract-hardening] item 4); `Ord` on positions (there is no cross-source
order to implement, and an order within one source would be a capability a reader
declares, not a blanket bound); deriving a comment's sub-spans from the lengths of the
view's strings (a reader may normalize what it reports as content, so a span never comes
from a string's length).

*Amendment (user, span-tiling design session).* Two contract clauses now pin what positions mean where a
reader serves several sources at one nesting level: *moving sets the position* (clause 7)
plus clause 2 fix where two consecutive tokens meet, at a **seam** between two sources
included — the place on both sides of a seam is one position value, and the reader chooses
that value and the coordinate `source_position_at` reports for it ([§dd-dr:span-tiling]).

Revisit if: a reader must expose more of a position than equality.

#### Zero-copy tokens [§dd-dr:zero-copy-tokens]

Status: DECIDED (upheld through both token redesigns).

Tokenization copies nothing out of the source. `StdToken<L>` holds byte ranges (its own
extent, its preceding whitespace, its post-space and comment-delimiter sub-ranges) plus
the `Arc`s a `GroupOpen` or `Specials` token already carries — no `String`s, and no
lifetime at all; the standard reader slices its borrowed `&'s str` when a parser asks what
a token is, and that borrow never enters the AST ([§dd-dr:token-opacity]). The earlier
revisit condition — a token source that cannot expose stable slices — is answered by
opacity: the language of such a reader declares its own `Tokenization::Token` and the
reader interprets it itself ([§dd-dr:tokenization]), so the token type no longer has to fit
every reader.
Revisit if: the standard reader itself must serve content it cannot slice out of a single
`&str` — the copy-free story is then `StdToken`'s to re-settle, not the trait's.

#### `TokenReader` is the behavior extension point for tokenization [§dd-dr:token-reader]

Status: DECIDED (user).

`StdTokenReader` is driven by the parsing state (rules data + cached tables + the
`scan_specials` hook); anyone needing genuinely different tokenization *behavior*
(catcode-like schemes, non-textual sources) implements the trait. `peek` deliberately
receives `&ParsingState<L>`, not `&TokenRules` — a catcode-like reader keeps its tables in
`L::StateExt` ([§dd-dr:crates]) — and nothing beyond the state reaches it
([§dd-dr:reader-context-purity]). The speculative-`peek` plus repositioning protocol
follows pylatexenc's proven `LatexTokenReaderBase` design; its two boolean flags are
replaced by named token edges — `move_to(&token, edge)` and `move_to_position(&position)`
are the only two ways to move ([§dd-dr:stream-position]) — and the capability the flags
stood for is not vestigial (a `\verb`-style parser repositions at the `End` edge, before
a swallowed post-space).
**Peek idempotence contract:** repeated peeks at one stream position with the *same state
instance* return the same result; implementations may memoize keyed on (position, `Arc`
identity) — sound because states are immutable and `derived()` always mints a new `Arc`. A
different state, however trivially derived, voids the obligation. (`StdTokenReader` does
not memoize yet — no premature optimization; the contract permits it.)

#### The token reader sees only the parsing state [§dd-dr:reader-context-purity]

Status: DECIDED (user).

`TokenReader::peek` receives `&Arc<ParsingState<L>>` and nothing else — no session, no
driver, no parse context. A reader that needs more takes it at construction: from the
language's `Lang::Tokenization` factory ([§dd-dr:tokenization]) for data the *type* knows,
or from a driver overriding `make_token_reader` for data the driver *instance* holds
([§dd-dr:token-reader-hook]).

Decisive reason: the reader is called from inside the parse loop, which holds the session
and the context mutably; passing either back into `peek` asks for a second mutable borrow
of what the caller is already using. The short parameter list also keeps the boundary
honest — a reader tokenizes; it does not stage nodes and does not record diagnostics.

Two consequences for a reader that substitutes content into the stream as it reads (a
macro expander, the motivating future case): the depth of its own substitution is a limit
it owns and reports as a `TokenError` carrying no recovery, not a case for the engine's
parse-depth guard, which counts construct-parser descents ([§dd-dr:descent-guard]); and a
report about content such a reader built is placed through the provenance chain of the
`Source` it built ([§dd-dr:provenance-on-source]), not through frames a reader would push
onto the session.

Rejected alternatives: passing the session or the driver into `peek` (the borrows above,
and it hands the token layer the parse layer's tools).

Revisit if: an expanding reader needs a budget shared with the session rather than one of
its own.

#### Specials scanning reports errors, never recoveries [§dd-dr:specials-scan-errors]

Status: DECIDED (user).

`Lang::scan_specials` — and the `SpecsProvider`/`Package`/`ScopeStack` chain that feeds it
— answers `Result<Option<SpecialsMatch<L>>, SpecialsScanError>`. A match carries the end
offset and the resolved pair (invocation form and spec); an error carries a condition kind
and a byte range in the content the hook was given. Neither carries a recovery token.

Decisive reason: the hook is handed a `&str` and an offset into it. It knows neither the
reader's token type nor its stream positions, so it can name neither a token to continue
with nor a place to continue from — the previous signature let it try, in coordinates only
the reader could interpret. Recovery is the reader's business: the reader lifts a scan
error into an unrecoverable `TokenError` whose span it qualifies with its own source. A
document-level condition a scan can detect is expressed the way the hook expresses
everything else — as a match to a spec whose parser diagnoses it.

Two riders. The specials name is the matched text (`content[pos..end]`) rather than a
field the hook fills separately, so what used to be advice ("should be the matched slice")
is now structure. And the reader checks every offset the hook hands it before using it:
the match end (inside the content, on a character boundary) and, on the error path, the
reported span (in range, both ends on character boundaries) before qualifying it with its
own source. Either violates a precondition documented on the hook, so the reader answers
with an unrecoverable implementation error rather than panicking on the slice
([§dd-dr:panic-policy]).

Rejected alternatives: a hook-produced `TokenRecovery` (expressible only in the hook's own
coordinates, which the reader is free not to share); a scan error the reader may treat as
recoverable by policy (the reader has no placeholder to continue with that the hook could
have meant).

Revisit if: a scan needs to report a condition the parse should continue past — the answer
today is a spec that diagnoses, and reopening this needs a case that spelling cannot
express.

#### Ambiguous group delimiters resolved by data: `expecting_group_close` [§dd-dr:expecting-group-close]

Status: DECIDED (upheld through the group-classes revision).

`$…$`-style group types make one
string both opener and closer (and `$$` vs `$` overlap); pylatexenc resolves this with
privileged math-mode state (`in_math_mode` + `math_mode_delimiter` checked before
longest-match). De-privileged into plain data: `TokenRules::expecting_group_close:
Option<Arc<GroupRule<L>>>` holds the rule whose *close* string takes precedence over all
other matches; a group construct parser sets it (via a state delta) when entering an
ambiguously-delimited group, and the tokenizer compares the rule's close string directly
(no id-to-rule lookup on the hot path). Otherwise the longest `PrefixTable` match wins, read as an
*open* when the string is ambiguous — and a close-only string tokenizes as `GroupClose`
even where syntactically wrong ("it's not the tokenizer's job to report syntax errors",
pylatexenc). **Priority order overall:** paragraph break → expected group close → longest
delimiter → command escapes → comment starts → specials scan → forbidden check → `Char`.
Groups precede commands so escape-led delimiters like `\(` win over command
interpretation; comments precede the specials scan, so a trigger string starting with a
comment delimiter is shadowed (deliberate: deterministic rules data wins over hook
behavior). Reproduces pylatexenc's `$\zeta$$\gamma$` / `$$…$$` behaviors exactly (ported
tests). A class cannot name a pairing once `GroupTypeId` means class — cf. the next
entry.

#### Group classes detached from delimiter spellings: class ids, runtime `GroupRule`s [§dd-dr:group-classes]

Status: DECIDED (user, group-classes session; reframes [§dd-dr:closed-type-ids]).

A group *type* is a language-native class
of "delimited region viewed as one object" — the latexlike preset will declare content-group
and math-group variants — and says nothing about spellings. This is the exact semantic
parallel of `CallableTypeId`: closed invocation *forms* ↔ runtime-registered *callables*;
closed group *classes* ↔ runtime-minted *delimiter rules*. `GroupRule<L>` =
`{ group_type, open, close }` (renamed from `GroupType`, joining
`CommandRule`/`CommentRule`), held as `Arc` in `TokenRules::groups`; any construct parser may
mint a rule mid-parse via a state delta (an optional-argument parser momentarily declaring
`[`…`]`, a custom spec declaring `<`…`>`).
Because a class no longer identifies a pairing, every consumer of the id *as pairing
identity* now uses the rule itself: `GroupOpen` tokens carry `Arc<GroupRule<L>>` — the
tokenizer's resolution (expected close first, then longest match, earlier rules winning ties)
travels with the token, the same make-mismatch-impossible principle as `Specials` carrying
its resolved spec — while `GroupClose` carries only `delim` (the parser knows which close it
expects; a stray close needs no more — and where a consumer needs the close's *class*, e.g.
the `NodesParser` stop condition, it re-resolves it from state: see the stop-conditions
entry); `expecting_group_close` holds the rule; `GroupData.group_type` stays but records
the class.
Rationale: per-pairing identities in a closed enum blocked exactly the extensibility the
delimited-group machinery exists for — a third-party spec cannot add variants to the preset's
enum, so novel delimiters (beamer-style `<…>` overlay arguments, `|…|` forms) were
unrepresentable, and even the preset's own optional-argument parser had to pre-register its
`[…]` pairing. Meanwhile pairing identity never distinguished anything the strings didn't.
The class keeps the typed "is this a math group?" answer (no string comparison) and makes
parse-time behavior data-driven — "entering this group enters math mode" is one class check,
where per-spelling variants (`DollarInline`, `ParenMath`, …) scattered it.
Rejected alternatives: removing `GroupTypeId` entirely (loses typed classification; would have reversed
[§dd-dr:nodes]'s "delimiters-only degenerates to string comparison" rejection); keeping per-pairing ids
with runtime allocation (recreates the deleted interned-id registry).
Revisit if: per-instance group data beyond class + spellings is needed — that is
`GroupNodeExt`'s job, not more id structure.

#### `TokenKind::Command` records its escape character [§dd-dr:command-escape-char]

Status: DECIDED (user).

The view is `Command { name, escape_char }` (the post-space is not view data: it is the
reader's `End..EndPastPostSpace` answer).
Rationale: [§dd-dr:specs]'s lookup design requires `CallableQuery { syntax: Command { escape_char } }`,
the escape char was not recoverable from the token, and the nodes parser must not reach
around the reader into raw content ([§dd-dr:tokens], `EndOfStream` rationale). The tokenizer knows
which `CommandRule` fired; recording it is syntactic fact (which rule fired), not resolution
output — consistent with the no-`CallableTypeId`-on-tokens line. Small test
ripple, accepted.

#### Token-layer contract hardening [§dd-dr:token-contract-hardening]

Status: DECIDED (user, token-contract review).

Six decisions closing contract gaps ahead of third-party `TokenReader`/`Lang`
implementations:

1. *A comment's start delimiter is a per-instance fact the reader answers for* (which
   delimiter fired mirrors `NodeKind::Comment`). The delimiter's span is
   `source_span_between(&token, Start, ContentStart)` and the content's is
   `(ContentStart, End)` — the `ContentStart` edge exists for exactly this
   ([§dd-dr:stream-position]); consumers must **never** reconstruct either from
   `content.len()` — the original `post_space.start - content.len()` arithmetic
   (duplicated in the nodes parser and the noise scan) silently assumed `content` was
   sliced verbatim from the source, and a custom reader that normalizes content would
   underflow it: a lib-code panic reachable from a legitimate impl of a public trait.
2. *Dangling-escape recovery uses a `Char(escape_char)` placeholder* spanning the escape
   byte (the recovery resumes at that placeholder's end). The byte joins the pending chars run, so the tolerant
   parse keeps span tiling — consistent with [§dd-dr:errors]'s recovery principle (markup
   text in a `Chars` node, always with a diagnostic) and with the other content-preserving
   recoveries. Rejected alternatives: the empty `EndOfStream` placeholder (pylatexenc parity) — it
   claimed no bytes while reading resumed past the escape, so the root children did not tile
   the content; the placeholder-vs-drop tradeoff had never actually been weighed when [§dd-dr:tokens]'s
   sentinel was chosen.
3. *`peek`/`next` take `&Arc<ParsingState<L>>`.* The documented memoization key (state
   pointer identity) was unobtainable from `&ParsingState`: a memoizing reader could not
   pin the allocation, and a recycled address would serve a stale token for a different
   rule set (ABA). Every call site already held an `Arc`, so the widening was
   source-compatible in the library; the engine's group-interior memo already pinned its
   key `Arc`s the same way.
4. *Moving the stream to a remembered place is a required `TokenReader` capability*,
   replacing the deleted `resume_at` helper (which synthesized a zero-width `EndOfStream`
   marker and called `move_to` — bypassing `StdTokenReader`'s bounds and character-boundary
   guards and silently imposing a "`move_to` must be span-derived" rule on implementors).
   Deliberately **no default body**: such a move is a distinct capability every reader must
   answer for, not a marker-token trick to inherit. *Reversed in part (2026-08-17, recorded
   as a conscious reversal):* the required method was `move_to_pos(pos: usize)`, taking a
   byte offset; it is retired in favor of `move_to_position(&L::StreamPosition)`, whose
   argument only the issuing reader can produce ([§dd-dr:stream-position]). The capability
   stays required and undefaulted for the reason recorded here; what changed is that the
   place is no longer a number any caller can write down.
5. *No `content()` on the trait — and no raw-content escape hatch at all* (user,
   follow-up). A `\verb`-style verbatim parser reads ordinary `Char` tokens under a
   derived state with every feature gate off (`enable_whitespace/multi_newline_paragraphs/
   commands/comments/groups/specials: false` — all delta-expressible) and
   `expecting_group_close` **replaced** by a rule whose close string is the verbatim
   terminator. The expected close is ungated by `enable_groups` (the `enable_*` flags
   entry, decided interaction 1) and overrides any close expectation inherited from an
   enclosing group (without the replacement, a verbatim region inside a braces group
   would read its body's `}` as `GroupClose`), so it is the single recognizer left
   active: the body arrives as pure `Char` tokens and the terminator — multi-character
   strings included — as one `GroupClose`. The test-side `RawBlockParser` demonstrates
   the recipe for `\raw…\endraw`, inherited-close override test included. Doctrine:
   construct parsers make no forward parsing decision from raw content. A parser obtains
   spans from the reader ([§dd-dr:no-context-source]) and may read the text of a span it
   has already consumed through `SourceSpan::content` — e.g. an environment name — which
   is span rendering, not scanning. Cost accepted: char-at-a-time reads are slower than a substring search,
   and such parsers are testable only against scanning readers (a fixed token list
   cannot re-tokenize under the verbatim state).
6. *`TokenError`'s recovery payload is boxed* (`Option<Box<TokenRecovery>>`): every
   `peek`/`next` returns the `Result`, and the inline payload put the hot type at 104
   bytes for a 72-byte token; boxing lands the allocation on the cold error path only
   (public accessors unchanged). Also: `Display for TokenKind` renders written spellings
   (`Command(\foo)` with the escape char that actually fired; comment content truncated).
   And `'\r'` receives **no special treatment anywhere in the tokenizer** (user,
   follow-up): `'\n'` is the sole line terminator; feeding text-mode-normalized content
   is the embedder's job (the `no_std` core never reads files). pylatexenc's CRLF
   comment quirk — the `'\r'` of a `\r\n` line ending stays inside the comment content —
   is thereby parity-by-doctrine, pinned by a test. Rejected alternatives: moving that `'\r'` into
   comment post-space when declared whitespace (briefly implemented) — special-casing
   one legacy line-ending convention inside the scanning core.

*Amendment (user, span-tiling design session).* The contract has since gained clause 7 (moving sets the
position), clause 8 (one source, in reading order, without gaps — required of the readers of
a language that obeys span tiling) and a *Seams* section for the readers of a language that
does not, which settles four further rules for such a reader: termination is its own
responsibility (an expansion that never ends is an endless token stream), positions and
tokens stay valid inside sources the stream has already left, an expansion's source is
minted with `SourceProvenance::Synthesized` and pushes no `Frame`, and `EndOfStream` means
the end of the *whole* input rather than of an exhausted expansion. `source_span_describing`
joins item 4's family of capabilities a reader must answer for, required and undefaulted for
the same reason ([§dd-dr:span-tiling]).

#### `TokenListReader` demoted to internal test infrastructure [§dd-dr:token-list-reader-demoted]

Status: DECIDED (user).

Compiled under `cfg(test)` only, `pub(crate)`, removed from the
public exports. Every consumer is an in-crate test; its load-bearing role is the
reader-agreement harness (each construct-parser suite runs every parse against
`StdTokenReader` *and* a pre-scanned `TokenListReader` and asserts identical trees, stops,
and diagnostics — the enforcement mechanism for "construct parsers never reach around the
reader"), plus hand-built token lists for engine tests. It carries the second half of that
enforcement too: it **rejects a token or a stream position it did not itself issue**
(tokens matched against its list by extent and kind, positions against the set of offsets
it has handed out), panicking as test infrastructure may. Since only a reader can produce
either value ([§dd-dr:token-opacity], [§dd-dr:stream-position]), a parser that invented one
— or carried one across from another reader — fails the agreement run. Its fixed-list fidelity gap — no
re-tokenization under the peek state, so state-driven parsers like the verbatim recipe
cannot run over it — is fine for a test tool but disqualifies it as a public reader
contract. Rejected alternatives: deleting it outright (loses the lockstep verification); keeping it
public (a maintained API surface nothing external needs).

#### `TokenRules::multi_newline_paragraphs` (renamed from `double_newline_paragraphs`) [§dd-dr:multi-newline-paragraphs]

Status: DECIDED (user).

Any run of two or more newlines (however many, with interleaved inline whitespace) forms
one paragraph break; pylatexenc's "double" name misread as "exactly two". Spelled
`enable_multi_newline_paragraphs` in the `enable_*` family ([§dd-dr:enable-flags]).

#### `enable_*` feature flags on `TokenRules` [§dd-dr:enable-flags]

Status: DECIDED (user, child-state design session follow-up; pylatexenc's
`enable_macros`/`enable_comments`/… pattern).

Every major
tokenization feature gets a boolean gate stored next to its data: `enable_whitespace`,
`enable_multi_newline_paragraphs` (the former `multi_newline_paragraphs`, renamed into the
family), `enable_groups`, `enable_commands`, `enable_comments`, `enable_specials`. Disabled
= the feature's syntax reads as ordinary content characters; the data stays in place, so a
delta can disable a feature and a later delta re-enable it without any party carrying the
original rules — the restore problem wholesale collection overrides cannot solve, because
the re-enabling party (applying a returned delta, or a `ChildStateSpec` policy) typically
never saw the state that held the original `CommandRule`s. Two spellings of "off" are
accepted deliberately: flag `false` is the *scoped* off (data preserved for re-enabling),
empty data is the *constitutive* off (no rules data) — pylatexenc precedent; the third,
compile-time **absent** spelling of "off" is [§dd-dr:lang-features]. Uniformization rider: `whitespace` loses its `Option` (plain `WhitespaceRules` +
`enable_whitespace`), which also removes the `Option<Option<…>>` override wart in
`TokenRulesOverrides`; every flag overrides as a plain `Option<bool>`.
Decided interactions: (1) **`enable_groups` does not gate `expecting_group_close`** — the
expected close is positional data the tokenizer checks *before* the delimiter table, and it
survives the flag: a group interior that disables groups entirely still finds its close
(preserves [§dd-dr:parsers-engine]'s termination guarantee structurally). (2) **`enable_specials` settles the
disable-specials gap (former open question [§dd-dr:open-questions])**: specials *data* stays Lang/library
business, but the gate is rules data — `freeze()` skips `Lang::specials_trigger_chars`
entirely and stores the empty `TriggerChars` filter, so the scan hook is unreachable in
disabled states. (3) Flags bake into the eager per-state caches where possible (empty
`PrefixTable` under `enable_groups: false`, empty `TriggerChars` under
`enable_specials: false`) — zero hot-path cost; the rest are single bool branches replacing
former `Option` checks. (4) `forbidden_chars` deliberately gets **no** flag (one trivially
restorable string, not a feature toggle with a demonstrated scoped-off need);
`expecting_group_close` is positional data, not a feature.
Rationale: the `ChildStateSpec` restricted-state use cases ([§dd-dr:parsers-engine]) need scoped, losslessly
reversible feature disabling, and field-wise wholesale replacement can express "off" but
not "off, remembering what on meant".
Rejected alternatives: keeping `Option<WhitespaceRules>` alongside the flag (three states, two meaning
"off"); `enable_forbidden_chars` (uniformity for its own sake).

#### The scan helpers: public, token-agnostic recognition primitives; the standard reader as their composition [§dd-dr:scan-helpers]

Status: DECIDED (user, design session).

The recognition logic of `StdTokenReader` is public, and public as free functions rather
than as methods. A **scan helper** takes the text being scanned and a byte offset into it, and
answers what one construct looks like at that offset — a **match value**: byte ranges
(plain `Span`s) plus the rule or spec that matched, or nothing. The seven are
`skip_whitespace`, `scan_paragraph_break`, `scan_group_delimiter`, `command_rule_at`,
`scan_command`, `scan_comment` and `scan_specials_trigger`; the match values are
`GroupDelimiterMatch`, `CommandMatch`, `CommentMatch` and the already-public
`SpecialsMatch`. `StdTokenReader::scan_std_token_at` is rewritten as their composition,
so each construct is recognized in exactly one place.

Two reuse cases asked for this, and different items serve them:

- A reader whose token type wraps standard tokens drawn from one or several sources (a
  macro expander) needs no helper: it keeps one inner `StdTokenReader` per source and
  calls the two methods promoted with this decision — `scan_std_token_at` (the standard
  token at an offset, without moving that reader) and `token_kind_of_std_token`
  (interpret one of the standard tokens it stores, under any `L`). The trait method
  `token_kind` is out of reach for such a language, since the `TokenReader`
  implementation of `StdTokenReader` serves only languages tokenized in
  `StdToken`/`StdStreamPosition` ([§dd-dr:tokenization]); the in-crate scripted test
  reader is the proof that these two suffice.
- A reader with token kinds of its own composes the helpers for the constructs it wants
  recognized the way the standard reader recognizes them, and builds its own tokens from
  the answers.

Shape choices that carried it. Match values hold plain `Span`s and never a `SourceSpan`:
a helper is handed a `&str` and knows no `Source`, which is what lets a reader serving
several sources at once use one set of helpers for all of them. Each helper takes the
least it can — `&TokenRules<L>` where the rules suffice, `&ParsingState<L>` only where a
state-derived cache or a `Lang` hook is involved — so no caller has to build a state to
scan. A helper for a construct whose feature the language declares absent answers
"nothing here", and that branch compiles away ([§dd-dr:lang-features]).

The `pos` requirement is the family's one panic: `pos <= content.len()`, on a `char`
boundary, checked in all builds by one shared private routine called at the top of every
helper — ahead of every feature gate, so the family behaves identically whatever the
rules say and whichever branch would have touched the text. It is an approved exception
under [§dd-dr:panic-policy] rule 3 and named in the user-facing panics guide. The
division of labor behind it: the reader that composes the helpers validates the offsets
its own caller hands it once, at its boundary — `scan_std_token_at` reports an invalid
`start` as an implementation error, never a panic — and passes derived offsets on.
`scan_command` asserts one thing more, also in all builds: that `rule.escape_char`
stands at `pos`. A mismatch would otherwise slice mid-character and panic anyway, less
clearly. The fallback kept in reserve, should the granted exception be read as covering
`pos` alone, is `Option<Result<CommandMatch, EndOfStreamAfterEscape>>` with `None` for
the mismatch — an answer no caller can act on, which is why the assert was chosen.

Rejected alternatives: a public dispatcher answering a construct *shape* instead of a
token (it would duplicate what a token's own kind view already carries and blur token
opacity, [§dd-dr:token-opacity]; the two cases above cover the need); `Result` on every
helper for an invalid `pos` (an error channel on otherwise infallible answers, for a
condition no scanned text can cause and no caller can handle); a validated
cursor/offset newtype instead of a bare `usize` (it centralizes the check but changes
`skip_whitespace`'s long-standing signature and makes every caller wrap an offset it
computed itself).

Revisit if: a third reuse case appears that neither the two promoted methods nor the
helpers serve; or a helper genuinely needs the `Source` behind the text — the reader
adds it today, which is exactly what keeps the helpers usable across sources.

## Parsing state and deltas [§dd-dr:parsing-state]

#### Tokenization config is plain data (`TokenRules`), not per-facet traits [§dd-dr:token-rules-data]

Status: DECIDED (proposed as a reversal of the user's earlier code experiment; signed
off and implemented — cf. [§dd-dr:state-option-c]).
The WIP `src/state/` gave each facet (whitespace, groups, macros, comments, …) its own trait +
macro-generated data struct, composed via 9 associated types. The decisive argument against:
**it contradicts the delta system** — these values must change *mid-parse* (math library adds a
`$` group type; verbatim disables everything; `\makeatletter` changes name chars), and values
behind compile-time associated types can't be changed by runtime deltas. Supporting arguments:
facet traits only abstract storage layout, which nothing needs; the macro DSL + 9-way generics
are exactly the proliferation [§dd-dr:one-generic-param] warns about; genuine behavior variation is already covered by
`TokenReader`.
Rejected alternatives: the facet-trait design; also the `TypeId`-keyed `Any` extension map from the
generated docs (runtime-typed, allocation-heavy, unnecessary once `L::StateExt` exists).
Revisit if: a preset needs tokenization *rules* whose evaluation is behavioral, not
value-like — first try expressing it as data; then as a `TokenReader` wrapper.

#### Language-specific state is a typed extension (`L::StateExt`) [§dd-dr:state-ext]

Status: DECIDED (implemented; formerly proposed).

Math mode, FLM flags, etc. live in a compile-time-typed field, not a dynamic map. Type safety
and zero lookup cost; dynamic-language bindings (Python/JS, if ever) can define one `Lang` with
a dynamic `StateExt` — the cost is contained to those bindings instead of taxing all users.

#### Immutable state, explicit deltas, Arc-shared snapshots [§dd-dr:immutable-state-deltas]

Status: DECIDED (pattern inherited from pylatexenc, kept deliberately).

Construct parsers return `(output, Option<delta>)`; the caller applies deltas. The engine
creates a new `Arc<ParsingState>` only at transitions, so all nodes parsed under one state share
one Arc, and nodes record their parse-time state (needed because a name-based spec lookup
*after* parsing would find the wrong spec if definitions changed mid-document). Group-local
state (definitions pushed inside `{…}`) pops naturally by restoring the previous Arc.
*Rationale for parser-returns-delta rather than parser-mutates-state:* the caller decides
scope — whether a delta applies to following siblings or dies with the group.

#### Settings are stored data; dependent settings recomputed at transitions (Option C) [§dd-dr:state-option-c]

Status: DECIDED (user-led).

Every effective setting is a plain field — no getters compute values on read. Cross-cutting or
derived settings (e.g. escape char = `#` in math mode) are recomputed by a single
`Lang::finalize_transition(new, prev, events)` hook that runs when a new state is built. The
delta is a concrete `ParsingStateDelta<L>` value (optional overrides + typed `L::Event`s — the
pylatexenc "changed kwargs"), applied only through `ParsingState::derived()`, the sole
constructor of non-initial states, over private fields — so the recompute choke point is
airtight. `&mut` exists only internally, pre-freeze; the public contract has no mutation.
Rationale: any change to an effective setting *is* by definition a transition, so
compute-per-read buys nothing over recompute-at-transition, while C keeps hot-path field reads,
truthful debuggability (`dbg!(state)` shows real values), and one central finalize function. The
delta is a **struct, not a closure** because producer and scope-decider differ (outward
propagation: `\newcommand`'s delta is applied by callers to base states the producer never saw),
and a struct is mergeable, inspectable, propagatable, and batchable.
Rejected alternatives: Option A (concrete state + per-getter `Lang` hooks — hooks "patch" the storage
model); Option B (whole state behind an `L::State: ParsingStateModel` getter trait — maximally
flexible, but the costs compound: an engine-owned wrapper is needed anyway because derived
caches cannot live in the model; the trait needs laws — getter purity for caching, delta
locality, a stored-vs-effective semantic; compound getters need `Cow`-shaped returns;
"default plus one tweak" costs a dozen delegated methods; and `dbg!(state)` lies because
effective state is latent in code. The swappable storage it buys is speculative and
recoverable later behind the same getter surface, so C keeps B's door open, not vice
versa); a closure-shaped delta (not
mergeable/inspectable/propagatable).
Revisit if: a preset genuinely needs swappable state storage (re-evaluate B behind getters —
C→B is the intended one-way door).
Implementation notes: the derived caches (`PrefixTable`,
`TriggerChars`) are built **eagerly** when a state is frozen, not `OnceLock`-lazily as first
sketched — the crate is `no_std` (`core` has no `OnceLock`; `OnceCell`
would cost `Sync`). Eager rebuilds turned out to be a real fraction of a transition's cost
(performance review), so `derived()` reuses the parent's `PrefixTable`
(`Arc`-held) whenever its inputs — `enable_groups`, and `groups` by elementwise `Arc`
identity — are unchanged; the dominant group-interior transition (only
`expecting_group_close` overridden, deliberately not a table input) always takes the reuse
path. No analogous generic rule exists for `TriggerChars`: its inputs include
`L::StateExt`, which carries no `Eq` bound — the per-transition cost expectation is
documented on the hook instead. `TokenRulesOverrides` collections are replaced
wholesale, not merged — "current group types plus one" is built by the party holding the
current state; merge semantics inside the override would smuggle policy into the choke
point. One honest cost of the specials-scan hook (recorded for fairness): `dbg!(state)` no
longer shows *all* tokenizer behavior — specials recognition sits behind the hook. It
remains true that tokenization is a pure function of the state (libraries live in the
state; a push is a transition), so the Option C argument itself is unharmed.

#### Token-level language hooks live on `Lang`, next to `finalize_transition` [§dd-dr:lang-token-hooks]

Status: DECIDED (token-design review).

`Lang::scan_specials` and `Lang::specials_trigger_chars`
follow the same pattern as the transition customizer: static hooks with working defaults,
receiving the state (or, for the trigger-chars derivation, the `StateData` mid-freeze).
Rationale: the hook receives the state and can therefore dispatch on `ext` and pushed
libraries — everything a swappable scanner object stored in `StateData` could express,
without adding a state field, dyn indirection, and a delta story for swapping it.
Rejected alternatives: an `Arc<dyn SpecialsScan>` field in `StateData`; per-library trigger
declarations with core-legislated cross-library shadowing (see [§dd-dr:tokens] — the preset owns its
scan; the core legislates nothing about trigger precedence).

#### Seed states are crate-frozen `Lang` data: `ParsingState::initial()` [§dd-dr:seed-states]

Status: DECIDED (user, code-review follow-up; closes the seed hole flagged by the state
review).

`ParsingState::new(data)` was `pub`, so any caller could assemble a state that never
passed `finalize_transition` — the one hole in "airtightness is structural". Now the
language provides its canonical seed as *data* (`Lang::initial_state_data() ->
StateData<Self>` — since revised to `Result<StateData<Self>, FinalizeError>`, a
seed hook may refuse: [§dd-dr:hook-fallibility]; default: every syntax gate off,
no libraries, default ext), and the
*crate* owns the data→state freeze (`ParsingState::initial()`); `new()` is
`#[cfg(test)] pub(crate)`. Callers customize the starting point via `derived(delta)` —
which runs finalize — never by assembling a state.
Rationale: the hook returns `StateData`, not `ParsingState`, precisely so out-of-crate
presets can implement it while the freeze stays crate-owned. `finalize_transition` still
does not run on the seed (no `prev` exists), but the obligation shrinks from "any caller
anywhere" to "the `Lang` author's own canonical seed must be coherent" — author-local,
documented on the hook, and mechanically pinnable by asserting
`initial().derived(&empty)` is data-equivalent to `initial()`.
Rejected alternatives: a separate `Lang::finalize_initial(&mut StateData)` hook (two hooks to keep
consistent — the same forgettability the hole had); changing `finalize_transition` to
`prev: Option<&ParsingState>` (taxes every transition-reactive implementor with a `None`
arm that pure normalizers never need); a `Default for TokenRules` to back the hook's
default body (rules.rs deliberately implements no `Default` — the neutral all-off rules
are constructed inline in the hook instead).
*Deferred (user, same session):* generic seed-side registration of `LibraryStack`
*fallbacks* is still inexpressible by delta (a `Lang` author bakes fallbacks into
`initial_state_data`; a *user* of a preset cannot add their own). Resolution folded into
the LibraryStack revisit — deltas became much more expressive about library
manipulation, up to whole-stack replacement in a transition ([§dd-dr:scope-stack]).
Revisit if: the LibraryStack revisit lands (the delta story may then subsume parts of
the seed contract), or a preset needs `finalize_transition`-grade normalization on the
seed itself — the `derived(&empty)`-at-seed trick (one extra freeze at session start)
is the cheap mechanical option before any signature change.

#### Parsing mode is first-class state data: `StateData.mode: L::ModeId` [§dd-dr:first-class-mode]

Status: DECIDED (user; settles the group-interior-state parity gap jointly with the `ParseDriver` entry,
[§dd-dr:parsers-engine]).

`Lang` gains `type ModeId` (`Copy + Eq + Debug + Send + Sync`; `()` under `TrivialLang`) —
the third closed per-language vocabulary after `GroupTypeId`/`CallableTypeId` — stored as
a plain field on `StateData` with a matching `ParsingStateDelta.mode: Option<L::ModeId>`
override channel. Mode is deliberately not lookup-private: the scope stack reads it for
package visibility ([§dd-dr:specs]), and a preset may key any content-interpretation decision on it
(text/math; verbatim/listing-ish modes are candidates).
*Division of labor:* the driver *initiates* mode changes (its `group_interior_delta`
returns a mode-bearing delta for a math group class — the whole math plug is one line of
data); `Lang::finalize_transition` *interprets* them (disable features, adjust rules — it
sees `prev.mode()` and the incoming override). The consequence hook must NOT live on the
driver: `derived()` is callable out-of-parse where no driver exists, and a state must
remain a pure function of base + delta (airtightness, reader memoization) —
driver-dependent states would break both.
*Consequences:* mode-shaped transitions need no `L::Event` (the override is the signal;
events remain for non-modal semantics); the latexlike preset likely needs no
`in_math_mode` in its `StateExt` (single source of truth).
Rejected alternatives: computing the mode at freeze from `ext` (a hidden derivation for what is
honestly plain data); an interior delta or events payload on `GroupRule` (the
data-first candidate — `GroupRule` feeds elementwise prefix-table comparisons and derives
`Eq`, which a delta payload breaks; and cross-rule policy would smear across rule
definitions instead of centralizing in finalize).
Revisit if: a language needs several orthogonal mode axes at once (composite enums
cover the known cases; if they explode, mode may need to become a small struct).
Two bound additions were forced in flight: `ModeId: … + Hash + Default`. `Hash` because the mode override joins the session derivation-memo
key — keyed *by value* (exact — modes are `Copy + Eq` vocabulary), unlike the
identity-keyed rule payloads, so mode-bearing descent deltas stay memoizable (the driver's
math plug depends on this; `GroupTypeId`/`CallableTypeId` carry `Hash` for the same
map-key reason). `Default` supplies the seed's mode in the default
`initial_state_data()` (the exact precedent of `StateExt: Default`); a real language's
`#[default]` variant names its canonical initial mode. The memoizable-delta *gate* is
unchanged (no ext/events/pushes); `ParsingState::mode()` returns by value.

#### Enclosing-state stack on the session; context-dependent events lowered by the driver [§dd-dr:enclosing-state-stack]

Status: DECIDED (user-led, API-review session).

The parse **machinery**, not the state model, keeps the enclosing context:
`ParserSession` maintains a stack of enclosing `ParsingState`s — push/pop at the same
descent points as the traceback frame stack ([§dd-dr:parse-traceback]), a scoped
`ParseContext::with_parsing_state(state, closure)` form for takeover parsers
(`parse_construct`, the single descent entry point, maintains the stack through it
underneath; `with_derived_state(&delta, f)` composes derivation and scoping),
innermost-first iteration
starting at the current state. The engine already retains exactly these states
implicitly (group exit structurally restores the outer `Arc`); the stack only
materializes them, and it dies with the session — **no ancestry residue survives into
parsed material** (nodes record parse-time states; a state-side parent pointer would
freeze parse history into the tree — the same reason node navigation went into a side
table, not into node values).

Event consumption is two-level, split by the placement doctrine (`Lang` = hooks
callable outside a driven parse; driver = driven-parse-only behavior):

- **`Lang::finalize_transition` is kept** — it is what keeps bare `derived()`
  composition coherent (the [§dd-dr:language-init] embedder idiom runs outside any
  session), and mode-shaped transitions don't even need events (`delta.mode` is the
  signal). It is **fallible** (the message-carrying `FinalizeError`, folded into
  `DeriveError`; default `Ok(())`): a
  context-requiring event reaching bare `derived()` errors loudly instead of being
  silently dropped — in-parse, that aborts as an implementation error under any
  recovery policy (extension wiring, not source input). The seed still never runs it.
- **`ParseContext::derive_state(&delta)`** (+ scoped `with_derived_state`) is the
  parser-facing derivation: it lowers context-dependent events through the driver
  hook **`ParseDriver::resolve_state_event(&event, &ParsingStateStack) ->
  Option<ParsingStateDelta>`** (default `None` = context-free, left for
  `finalize_transition`), merges the patches (in event order; the delta's own explicit
  overrides win — the delta author spoke), strips the lowered events, then calls
  plain `derived()` — one choke point preserved. Per-event *policy* lives on the
  driver; the event *loop* lives in the one cx method — parsers never iterate events.
  The lend guarantees current-state-first: `derive_state` pushes the context's current
  state for the hook call when sibling after-effects have evolved it past the
  innermost stack entry. (`ParserSession::derived_state` performs no lowering.)

**`ParsingStateStack`** is the owning, session-independent stack type — it holds
`Vec<Arc<ParsingState<L>>>`; the session stores its live stack as one and lends `&` to
hooks (zero extra cost; the states themselves are never copied). The
`ParsingStateDelta` specificity precedent rules out bare `StateStack`, and "View"
would misname an owning value. It is constructible outside any session:
`from_states(states)` and **`from_node_ancestors(node)`** — the node's own recorded
state first, then parents outward via the stored parent table
([§dd-dr:tree-navigation]), i.e. exactly the innermost-first/current-state-first
order — so post-parse synthesis feeds the same pillar signatures the driver hook feeds
([§dd-dr:preset-driver-pillars]). Contract note: the walk's sequence is not
entry-for-entry the parse-time stack (ancestor chains contain Arc-equal duplicates and
non-group nodes); the documented contract is the **scan semantics** — first non-math
state, outermost fallback — which duplicates cannot affect.

First consumer: the latexlike math-exit ([§dd-dr:argument-factory-additions]) — the
preset pillar **`exit_math_context_delta`**. The delta is defined by *exiting the math
context*: look up the first non-math enclosing group in the stack and restore that
context's `TokenRules` **minus the transient gates** — `expecting_group_close` and
`temporary_groups` are never restored, because they describe in-flight structural
expectations of the abandoned context (which close that context's own group descent
was waiting for; which scoped-lifecycle delimiters were live there), not lexical
context, and restoring them would plant another scope's expectations into the new one
(the derived state inherits both fields from its base as usual, and a following group
descent installs its own expectation through the descent invariant). The delta is
never defined by seeking or naming a text mode as the target — consistently, the mode
role trait carries no text-mode constructor ([§dd-dr:latexlike-generalization]); core
learns nothing about modes. The preset's event logic (math entry, math exit) ships as
**public pillar functions** (`LLL`-generic; the hooks are one-line delegations) so
post-parse processing can synthesize coherent recorded states for constructed nodes —
restaged or synthetic children emulating "enter math"/"exit math"
([§dd-dr:transform]).

Rejected alternatives: per-`GroupRule` mode visibility (plants a semantic reading of
`mode` in core, deliberately unclaimed there — and arbitrarily privileges groups over
comments/whitespace); an `Arc` parent pointer on `ParsingState` (cycle-free and
depth-bounded in the enclosing-pointer refinement, but bakes parse history into a
value type and pins ancestry from every recorded state); a declared/effective rules
split via `StateExt` (duplicates the vocabulary; generic delta writers bypass it);
`Enabled` flags on rules (stateful; conflates who disabled a rule and why);
`TokenRules` keyed by mode (combinatorial duplication; freezes a mode-indexed
structure); `cx.finalize_parsing_state(data, prev, events)` (re-exposes the
crate-owned data→state assembly at parser altitude); per-event cx methods
(`delta_for_derived_event`-style — merge burden and ordering pitfalls at every call
site).

Revisit if: a Lang needs an event resolvable only between the driver lowering and
finalize (an ordering the two-level split cannot express), or post-parse synthesis
needs machinery context beyond what the public pillar functions take as arguments.

#### `TrivialLang` (renamed from `SimpleLang`): the test lang, not an on-ramp [§dd-dr:trivial-lang]

Status: DECIDED (user, API-review session).

The blanket-impl marker trait stays public, renamed **`TrivialLang`**, and is
repositioned honestly: the trivial language — for tests and machinery experiments;
implement `Lang` directly for anything real. The dead-end is structural, not a doc
gap: the blanket impl defaults all nine associated types, so the trait and *any*
customization are mutually exclusive — the first command, real id enum, or hook
forces the full `Lang` implementation. "Simple" over-promised an on-ramp (the
language-designer walkthrough's false start was believing it); the on-ramp job
belongs to the real fixes ([§dd-dr:on-ramp-defaults],
[§dd-dr:scopes-resolving-driver]). What keeps it public: `make_node_ext` is now a
required `Lang` method ([§dd-dr:ext-minting]), so without the blanket even a
throwaway test lang needs a hand-written stub — external construct-parser and
tooling authors writing unit tests are the persona it serves (in-crate: ten
`#[cfg(test)]` impl sites, zero non-test uses).

Rejected alternatives: growing it into a quick-start tier (an overridable `Driver`
associated type fixes exactly one abandonment point; each further mirrored hook
escalates toward a parallel `Lang` with double maintenance); `pub(crate)` demotion
or deletion (kills the one-line test lang for external authors exactly when the
required `make_node_ext` makes hand-rolling heavier).

Revisit if: Rust stabilizes associated-type defaults — the trait then dissolves
entirely (it is documented as that workaround).

#### On-ramp values: `empty()` constructors; recognize-nothing specials defaults stay [§dd-dr:on-ramp-defaults]

Status: DECIDED (user, API-review session).

Two rulings on the from-scratch `Lang` cliff:

1. **`TokenRules::empty()` + `StateData::empty()`** — the all-empty starting value
   (every gate off, every collection/string empty, empty scope stack, default
   mode/ext); the default `initial_state_data` body is re-expressed over it (one
   source of truth), and language authors call-and-tweak instead of transcribing a
   13-field literal from the docs. Deliberately *named constructors*, not `Default`
   impls: `Default` invites `..Default::default()` struct updates that silently
   zero unmentioned fields when fields are added later, and the no-`Default`
   doctrine on `TokenRules` (banning privileged LaTeX values) stays intact — the
   empty value contains none. The name `empty` matches the verified contents and
   keeps "disable(d)" reserved for the gate-action family
   (`TokenRulesOverrides::disable_all()`, [§dd-dr:takeover-staging-sugar]).
2. **`scan_specials`/`specials_trigger_chars` defaults stay recognize-nothing**,
   with loud pairing documentation and a guide recipe — defaulting both hooks to
   the scope-stack fold was REJECTED (user): simple-by-default — a tiny lang must
   not override hooks to *remove* behavior, and opt-in keeps dead code eliminable;
   real consumers mostly sit behind frameworks that have already plugged these
   hooks. Moving the hooks onto the driver is structurally impossible without a
   strata violation: `scan_specials` is called by the token reader, which holds
   only a `ParsingState` — the driver is engine-stratum. The silent trap
   (overriding the scan but not the trigger-chars twin) remains
   documentation-guarded — accepted.

Rejected alternatives: scope-fold default hook bodies, driver-side specials hooks,
error-returning or required-hook variants (all above); `Default` impls;
constructor names `neutral()` (the walkthrough's proposal) and `disabled()`.

Revisit if: evidence accumulates of standalone custom-`Lang` authors (not
framework users) hitting the specials pairing trap despite the docs — the
fold-as-default option is sound in isolation (a `SpecialsMatch` carries its own
resolution, so no vocabulary conjuring is needed) and could be reconsidered.

#### Compile-time language features: `Lang::Features` presence declarations [§dd-dr:lang-features]

Status: DECIDED (user-ruled design spec, lang-features session).

A language declares **at compile time which parsing features it has at all**: `Lang` gains
`type Features: LangFeatures`, where the bundle trait `LangFeatures` carries one presence
declaration per feature (`type Whitespace: FeaturePresence`, …,
`type Scopes: FeaturePresence`). `FeaturePresence` is a sealed two-valued vocabulary
(sealed: closed to outside implementations via a private supertrait) — its only
implementors are the markers `FeaturePresent` and `FeatureAbsent`. It carries
`const PRESENT: bool` (*code* gating: a compile-time-known `false` lets the compiler
eliminate the feature's reader branches and dispatch arms) and the generic associated
type `Store<T>` (*storage* gating: an absent feature's rules data collapses to a
zero-sized type). Ready-made bundles: `AllLangFeatures` (every feature present),
`NoLangFeatures` (every feature absent). Per feature, a subtrait — `LangHasWhitespace`,
`LangHasParagraphs`, `LangHasGroups`, `LangHasCommands`, `LangHasComments`,
`LangHasSpecials`, `LangHasForbiddenChars`, `LangHasScopes`, blanket-implemented for
every `Lang` whose `Features` declares that feature present — is the bound vocabulary
for feature-requiring code. `Latexlike` and the whole `LatexlikeLang` family pin
`Features = AllLangFeatures` (user ruling: the latexlike family uses all features; it
does not participate in gating), and `TrivialLang`'s blanket impl supplies
`AllLangFeatures` for every test language.

This does not reopen [§dd-dr:token-rules-data] or [§dd-dr:data-vs-traits]: rule *values*
stay plain, delta-changeable state data. Only a feature's *presence* moves to compile
time — and presence is exactly what no runtime delta may ever change, because a language
either has a feature or does not.

Three motivations carried the decision, and one explicitly did not:

1. **Field organization.** `TokenRules`'s flat fields (`enable_groups`, `groups`,
   `temporary_groups`, `expecting_group_close`, …) are related only by adjacency and
   naming convention; regrouping them into one sub-struct per feature
   (`WhitespaceRules`, `ParagraphRules`, `GroupRules<L>`, `CommandRules`,
   `CommentRules`, `SpecialsRules`, `ForbiddenCharsRules`) names the relationship and
   stands on its own merits even with no gating at all.
2. **Unrepresentability of unsupported constructs.** A language without groups today
   expresses that as a runtime `bool` plus an empty `Vec` that every layer keeps
   checking; under compile-time absence, constructing group rules for that language is
   a type error. This is the move the crate has already made with `ModeId = ()`,
   `StateExt = ()`, and the closed `NodeKind`.
3. **The soft-freeze window.** [§dd-dr:stability-rubric]: until a framework builds on
   techy in earnest, an important discovered shortcoming may still be fixed breakingly.
   `TokenRules`'s public field layout is exactly the kind of shape that cannot be
   changed once dependents exist; the breaking change is cheap now and never again.

The **memory argument was measured and dropped**: parsing states appear to be 4–8 % of a parse's
peak memory footprint, and the languages that would declare features absent are
precisely those whose states are already cheapest — gating recovers mostly-empty struct
headers. Smaller states are a side effect here, not a motivation.

**Three spellings of "off", each with its own word.** The two-spellings narrative of
[§dd-dr:enable-flags] extends by one; the three words are never interchanged:

- **absent** — compile-time: the language *has no such feature*. Declared via
  `FeatureAbsent`; the feature's storage is a zero-sized type and its code paths are
  eliminated at compile time. Absent wins over any runtime data; an override for an
  absent feature is unrepresentable (below), and a documented-contract violation that
  reaches gated machinery anyway returns an `Err` through the standard recovery path,
  never a panic ([§dd-dr:panic-policy] rule 3).
- **disabled** — scoped runtime: the feature's `enabled` flag is `false` while the data
  stays in place, so a later delta can re-enable it losslessly (the
  [§dd-dr:enable-flags] restore argument, unchanged).
- **empty** — constitutive: the rules data itself is empty (no group rules, empty
  whitespace set); nothing is recognized even with the flag `true`.

"Disable(d)" stays reserved for the runtime action family
(`TokenRulesOverrides::disable_all()`), "empty" for the all-empty constructors
([§dd-dr:on-ramp-defaults]) — hence the third axis needed its own word, "absent".

**The roster is exhaustive over the parsing state**: every `TokenRules` block is a
feature, plus the scope stack — **Whitespace, Paragraphs, Groups, Commands, Comments,
Specials, ForbiddenChars, Scopes**. Two roster points pinned:

- **ForbiddenChars: two independent axes.** [§dd-dr:enable-flags] deliberately gave
  `forbidden_chars` no runtime `enabled` flag (one trivially restorable string needs no
  scoped-off gate). That ruling concerns the runtime axis only and is **supplemented,
  not reversed**, here: having no runtime gate does not make the feature
  compile-time-present for every language. ForbiddenChars is a full roster member on
  the compile-time axis while keeping no runtime flag.
- **Paragraphs is a feature of its own**, not a whitespace sub-flag: it owns a token
  kind (`ParagraphBreak`), a detection function, a dispatch arm, and a driver hook
  (`make_paragraph_break_node`). Its dependence on whitespace — today a runtime check
  in the reader's paragraph-break detection, which bails unless *both*
  `enable_multi_newline_paragraphs` and `enable_whitespace` are on — is promoted to a
  compiler-enforced supertrait edge: `LangHasParagraphs: LangHasWhitespace`.

**Independent declarations; dependencies as compiler-enforced edges.** Each feature is
declared independently. The genuine dependencies among features (the feature lattice — a
partial order: some features are built out of others) are recorded as supertrait or
bound relations, not by closing the combination space: `LangHasParagraphs:
LangHasWhitespace` (above); the verbatim family (`verbatim_state_delta`,
`VerbatimArgumentParser`, `VerbatimBodyParser`) and the argument parsers that mint
temporary group rules require `LangHasGroups` — verbatim is built out of the group
feature: its terminator is an `Arc<GroupRule<L>>` matched by group-close machinery;
scope mutation (`ScopeStack::push`, `ParsingStateDelta::{push_provider, scope_op}`,
`ScopeOp` construction) requires `LangHasScopes`. **Callables deliberately do not imply
scopes**: a driver may resolve commands from a fixed table — a fixed command set with no
`\newcommand` is the motivating case for that independence.

**Reads are total, writes are bounded, the stores stay crate-owned.** The per-feature
accessors on `TokenRules` are the one generic read path and answer for *every* language
— for an absent feature they return the neutral answer (not enabled; empty rule list; no
expected close) — so generic reading code (the reader, the node parsers, the derivation
machinery) carries no feature bounds. Only constructors, setters, and feature-requiring
entry points carry `LangHas*` bounds. And the rules data types behind `Store<T>` remain
crate-defined: a language chooses a feature's *presence*, never a substitute
*implementation*. This accessor discipline is also deliberate option value: if
implementation substitution is ever wanted later, every reader already goes through the
accessor surface, and only constructors and the override channel would move.

**Overrides are gated the same way.** `TokenRulesOverrides` mirrors the per-feature
blocks; an override for an absent feature is unrepresentable — writing one is a compile
error at the site that wrote it (with the zero-sized stores on the override blocks and
the gated `scope_ops` list, a violating delta cannot be written at all, so
`TokenRulesOverrides::apply()` is infallible). `disable_all()` is feature-aware **by
construction**: it consults the `Lang::Features` presence declarations and sets
exactly the present features' blocks to their `disable()` values — absent features
are simply not mentioned by the returned value (under an all-features-present
language: all six gates `Some(false)` plus the cleared forbidden set,
[§dd-dr:takeover-staging-sugar]) — so applying a `disable_all()`-based
delta never reports an
absent-feature violation. Of the crate's own constructors, only `disable_all()`
consults the declarations; the per-block `disable()` constructors stay
presence-blind, since authored use of one on an absent feature is exactly the explicit
data the compile error exists for.

**The present store is transparent — a requirement, not an optimization.** For a present
feature, `Store<T>` *is* `T`, not a wrapper: a concrete language with a feature present
writes plain struct literals for its rules, exactly as today. A wrapper here would tax
every language author for a mechanism only absent features need.

**Documented pitfall: struct update is sub-struct-granular.** With one block per
feature, a struct-update expression replaces *whole feature blocks*: in
`TokenRulesOverrides { groups: GroupOverrides { … }, ..disable_all() }`-style code, the
explicit `groups:` literal replaces the *entire* groups block that `disable_all()` set
up — the inner literal must itself spread from the intended base (the verbatim recipe
must use `..GroupOverrides::disable()` inside its groups literal). The two-level shape
needs two-level care; sites that must notice every new field (the exit-math restore's
deliberately exhaustive literal) stay exhaustive at both levels.

**Naming.** The absent/disabled/empty word split is above. The item names carry
`Lang*`/`Feature*` prefixes — `LangFeatures`, `FeaturePresence`, `FeaturePresent`,
`FeatureAbsent`, `AllLangFeatures`, `NoLangFeatures`, `LangHasWhitespace` …
`LangHasScopes` — because `techy::core` is the flat machinery hub where sibling
vocabulary competes ([§dd-arch:naming] principles 3–4): a bare `Present`, `Absent`,
`Features`, or `Has*` next to `Token`/`ParsingState`/`CallableSpec` answers neither
"present *what*?" nor "features *of what*?". The earlier sketch's spellings (a
`Gate` trait with `On`/`Off` markers) were rejected: "gate" already names the *runtime*
`enable_*` flags in the crate's documentation, and reusing it for the compile-time axis
would fuse the two axes the absent/disabled word split exists to keep apart
([§dd-dr:superseded-names]). The rejection concerns item *names* — `Gate`/`On`/`Off` as
public identifiers; "gating" as ordinary prose for either mechanism is unaffected.

Rejected alternatives: **closed tiers** (one `type Syntax` choosing among a few fixed
bundles — chars-only, +groups/comments, +callables, +scopes): kills the combinatorial
surface but cannot express legitimate combinations — callables-without-scopes, the
motivating fixed-command-table case, falls between tiers; independent declarations plus
enforced edges record the actual dependency structure instead of an approximating
ladder. **Open per-feature implementation substitution** (a language supplying its own
rules type per feature): the derivation memo's soundness contract is the killing flaw —
`state_memo`'s hash and equality walk overrides field by field with `Arc`-identity
keying, and a memo hit substitutes a previous derivation's result, so a
language-supplied comparison that conflates two semantically different overrides yields
silently wrong parse states (no panic, no diagnostic); that hash/equality contract must
stay crate-owned. **Silent no-ops for absent-feature overrides** (leaving
`TokenRulesOverrides` ungated and having `apply` skip inapplicable overrides): a delta
author who writes an override deserves a compile error, not a quietly inert field —
against the crate's loud-failure grain. **Per-feature associated items directly on
`Lang`** (no bundle): breaks every `Lang` impl by eight lines instead of one and
forfeits the one place where the presence vocabulary is named (the `Lang::NodeExts`
bundling precedent). **Cargo features**: global per build — two languages in one binary
could not differ — and they would scatter conditional compilation through code the crate
keeps clean.

Revisit if: a language genuinely needs to substitute its own rules *implementation* per
feature — reopen the substitution axis only with the memo comparison kept crate-owned
(e.g. features exposing crate-compared identity tokens rather than their own hash and
equality); or the independent-declaration test/documentation surface becomes a real
maintenance burden (the remedy is more ready-made bundles next to
`AllLangFeatures`/`NoLangFeatures`, not closed tiers).

## Specs and scopes [§dd-dr:specs]

#### Unified `CallableSpec` with self-supplied invocation parser [§dd-dr:unified-callable-spec]

Status: DECIDED (implemented; generalizes pylatexenc's `CallableSpecBase`).

Macros, environments, and specials are all "callables": a spec describing the invocation's
argument structure (the `ArgumentSpec` list, [§dd-dr:argument-parser-model]) plus an
optional full-takeover invocation parser (the `make_invocation_parser` override). This
preserves pylatexenc's most valuable extensibility property — *a spec can fully take over
parsing its own invocation* — required by `\verb`, tabular preambles, and FLM's richer
constructs. The default `arguments()` returns the neutral callable (no arguments) — the
semantically correct default for fallback singletons and simple specials like `~`, not an
arbitrary one.
Rationale: specs are data + optional behavior, matching [§dd-dr:data-vs-traits].

#### Library stack with lexical shadowing; no `ConflictStrategy` [§dd-dr:lexical-shadowing]

Status: DECIDED (user-led).

Ordered stack, innermost/last wins. Shadowing *is* the intended semantic (`\newcommand`
redefinition, group-local definitions), so a configurable conflict policy
(`FirstWins`/`LastWins`/`Error`) solves a non-problem while complicating resolution; an optional
lint can warn on shadowing if ever wanted.

#### `SpecLookup` receives a `CallableQuery` (query struct), not bare `(ct, name)` [§dd-dr:callable-query]

Status: DECIDED (closes the deferred half of [§dd-dr:lexical-shadowing]).

`lookup(&CallableQuery, &ParsingState<L>) -> Option<Arc<dyn CallableSpec<L>>>`, where
the query carries `callable_type`, `name`, and a `CallableSyntax` (`Command { escape_char }` /
`Specials` / `Other`).
*Why a syntax field:* with several `CommandRule`s in scope, `\foo` and `#foo` both tokenize as
`Command { name: "foo" }`, so the escape character has to travel as data on the query —
providers see no token at all and could not read one if they did.
*Why no token:* scopes and packages look a callable up by name and callable syntax, nothing
more; a language that must dispatch on token details does so in `ParseDriver::resolve_command`,
which receives the token and its reader ([§dd-dr:token-opacity]). The struct form absorbs
future context fields without dyn-trait signature churn.
Rejected alternatives: bare `(ct, name, state)` (forces presets to multiply `CallableTypeId`s to encode
syntax); a token on the query, mandatory or optional (a provider holds no reader, and an opaque
token can only be read through its reader — the token would be dead weight).
*Mode-awareness*, as proposed: the `&ParsingState<L>` parameter lets a preset's lookup dispatch
on `state.ext()` (FLM's `\vec` in math mode); the core `Library` ignores state and syntax
alike. This replaces an earlier proposal's hard-coded `math_mode_macros` tables, which
contradicted [§dd-dr:no-privileged-concepts].
(The lookup contract has since rehomed to `SpecsProvider::retrieve_spec` — fallible,
part of a richer provider trait — with `CallableQuery` and its rationale carried over
unchanged; cf. [§dd-dr:scope-stack].)

#### Argument model: an argument *is* a parser (pylatexenc's `LatexArgumentSpec`) [§dd-dr:argument-parser-model]

Status: DECIDED (user; modeled on pylatexenc's `LatexArgumentSpec`).

`ArgumentSpec<L>` = `{ parser: Arc<dyn ArgumentParser<L>>, name: Option<Box<str>>,
parsing_state_delta: Option<ParsingStateDelta<L>> }`; `CallableSpec` exposes
`&[Arc<ArgumentSpec<L>>]` directly — no wrapper type (empty-slice defaults work for
generic `L` where a `static` wrapper cannot: no generic statics, `Vec` not
const-promotable). The elements are `Arc`-shared so parsed nodes can record which spec
each argument was parsed against ([§dd-dr:nodes]), mirroring pylatexenc's
`arguments_spec_list`. The standard delimited forms (group, optional group, star marker)
are shipped `ArgumentParser` implementations parameterized by group class and rules
([§dd-dr:group-classes]) — pylatexenc's own resolution of the `'{'`/`'['`/`'*'`
shorthands into parser instances. Slots have no spec-side declaration at all: record-level
vocabulary only (cf. [§dd-dr:parsers-engine], the no-spec-side-slots entry).
Rationale: pylatexenc's whole argument ecosystem hangs off this slot, and "just write a
custom invocation parser" is the expensive path the declarative surface exists to avoid.
Reversal record (group-classes session, July 2026): the model first shipped as a hybrid —
standard forms as *data* variants (`Group`/`OptionalGroup`/`Marker`) plus
`Custom(Arc<dyn ArgumentParser>)` — precisely to keep introspection and
recomposition-by-data. Once `GroupTypeId` became a delimiter-detached class, a data
variant could no longer name "the `{…}` argument" (which group class, whose spelling?),
and the introspection argument proved non-load-bearing: recomposition reads nodes and
layouts (delimiters and marker spellings stored as `TextContent`, [§dd-dr:nodes]),
never specs. The previously rejected "every argument is an opaque parser" design was
thereby consciously adopted; the hybrid's terse constructors went with it.
Rejected alternatives: a closed `ArgumentKind` enum (a closed *architecture*, not just a
closed starter inventory — a real regression against pylatexenc); structure-level wrapper
types (`ArgumentStructureSpec`) around the argument list (nothing structure-level to
hold; a wrapper returns only if structure-level fields materialize, e.g. a slot separator
belonging to no single slot).
Costs accepted: spec types generic over `L`; no `PartialEq` on spec types (dyn parser,
state delta) — consistent with node types.

#### `CallableTypeId` and `GroupTypeId` are closed per-`Lang` associated types [§dd-dr:closed-type-ids]

Status: DECIDED (user, current-level review session; replaces the open interned-id registry design).

`Lang::CallableTypeId: Copy + Ord + Hash + Debug` (Ord: library map keys),
`Lang::GroupTypeId: Copy + Eq + Hash + Debug`; a real language defines small enums,
`TrivialLang` defaults both to `u32`. The earlier `Language<L>` interning machinery for these
ids was deleted outright.
Rationale: invocation forms and group-type identities are static per language definition —
nobody registers a new *form* at runtime (new *callables*, yes — via libraries; new
*delimiter spellings*, yes — `GroupType` values in the state's token rules; only the
identity vocabulary is fixed). Closed enums give exhaustive matching in preset code, make
cross-language id mixing a type error, and remove meaningless raw `u32`s ("open IDs floating
around").
Rejected alternatives: keeping the open ids for symmetry — the symmetry was spurious: token *rules* are
runtime state; type *identities* are not.
Revisit if: a genuine runtime-registration need for group/callable types appears (e.g.
catcode-style schemes minting new group types mid-parse) — then that language can use an
integer id type; the associated-type design accommodates it without core changes.
For groups the *Revisit if* later fired — construct parsers do mint delimiter pairs
mid-parse (optional arguments, custom specs). Resolved not by opening the id space (the
rejected registry) but by detaching the closed vocabulary from spellings: `GroupTypeId`
reframed from per-pairing identity to group *class* (cf. [§dd-dr:group-classes]);
`CallableTypeId` untouched, both still closed per-`Lang`. Both id types' bounds carry
`+ Send + Sync` ([§dd-dr:spec-thread-safety]).

#### Thread safety is a core contract: `Send + Sync` supertraits on the dyn spec traits [§dd-dr:spec-thread-safety]

Status: DECIDED (user + Claude, thread-safety session).

`CallableSpec`, `SpecLookup`, and `ArgumentParser` carry `Send + Sync` supertraits; the
bounds this forces propagate to `Lang`'s associated types (`GroupTypeId`, `CallableTypeId`,
`StateExt`, `Event`), all seven `NodeExtTypes` types, and the `SourceOrigin` trait. Result:
`NodeTree`, `ParsingState`, `Token`, deltas, and every spec handle are `Send + Sync` — parse
on one thread and hand the tree off; share preset libraries across parallel parses.
Rationale: `Arc<T>: Send` needs `T: Send + Sync`, and Send-ness is erased at the trait
declaration — without the supertraits a threading consumer has **no safe recourse** (only a
newtype + `unsafe impl Send`, unsound if any spec actually isn't thread-safe), while under
them a single-threaded implementor always has a safe path. And that path is barely longer:
every extension-trait method takes `&self` and specs are `Arc`-shared across nodes, so
mutable implementor state needs interior mutability *regardless* — the bounds never
introduce a wrapper, they only select `Mutex`/`RwLock`/`OnceLock`/atomics (`spin` on
`no_std`) over `RefCell`/`Cell`/`OnceCell`. A survey of realistic non-threadsafe wants found
no blocked use case: a database-backed lookup holds `Mutex<Connection>` (forced by `&self`
anyway — rusqlite/diesel connections are `!Sync`; the lock is invisible next to the query)
and returns plain-data `StdCallableSpec`s; pyo3-backed specs are already `Send + Sync`
(GIL-guarded); the one awkward case is `!Send` script-engine handles (`mlua::Lua`), solved
by mlua's own `send` feature or a dedicated interpreter thread. Also resolves clippy's
`arc_with_non_send_sync` — the crate's global `alloc::sync::Arc` commitment previously paid
atomic refcounts for zero capability — and completes [§dd-dr:parsing-state]'s "OnceCell would make states
non-`Sync`" intent. Contrast pylatexenc: Python's GIL made shared mutable spec state a
non-issue; ports of such specs use locks.
Rejected alternatives: a `sync` cargo feature gating the supertraits (or `Arc` vs `Rc`) — cargo
features are additive and unified across the dependency graph, so a contract-changing
feature forks the extension ecosystem into two incompatible dialects: extension crates must
pick a side, and one crate enabling it imposes it on all (`im`/`im-rc` shipped as *separate
crates* for exactly this reason; rhai's `sync` feature is the cautionary precedent).
Mechanically it also needs duplicated trait definitions or a `MaybeSendSync` helper trait,
plus double CI and docs. Spelling `Arc<dyn … + Send + Sync>` at use sites — same effective
constraint for anything stored in a tree, but two distinct erased types and the
`Box<dyn Error + Send + Sync>` spelling plague.
Revisit if: a compelling single-threaded embedder materializes — then a parallel
`Rc`-based local layer can be added *without* breaking the `Send` world (rowan's `Send`
green tree / deliberately-`!Send` red cursors precedent); the reverse migration (adding
bounds later) would break implementors holding non-`Send` state, which is why the bounds
land now while the API is fluid.

#### `CallableSpec: Any` — downcasting is part of the spec contract; `Lang: 'static` [§dd-dr:spec-downcasting]

Status: DECIDED (user).

The documented preset pattern
(`Lang::finalize_node`: "read the spec, downcast, attach ext") was not expressible —
the trait had no `Any` supertrait. Now it is: `(&*spec as &dyn Any).downcast_ref::<
ConcreteSpec>()` via trait upcasting (stable exactly at our MSRV 1.86; the rehearsal
test performs the real downcast through the dispatch loop). Since `Any` requires
`'static` and generic spec types (`StdCallableSpec<L>`) must satisfy the supertrait,
`Lang` (and `TrivialLang`) gained a `'static` bound — free in practice, a `Lang` is a
unit marker type, and the stored `Arc<dyn CallableSpec<L>>` was implicitly `'static`
all along. **The trait-object case:** `Any` downcasts to concrete types only; a preset
dispatching on an *open* set of spec types (third parties implementing FLM's
`trait FlmSpec: CallableSpec<FlmLang>`) funnels them through one concrete wrapper —
its registration sugar wraps every spec in `FlmSpecBox(Arc<dyn FlmSpec>)` (implementing
`CallableSpec` by delegation), and finalize downcasts to the wrapper to recover
`&dyn FlmSpec`. This costs the preset one indirection and zero core machinery.
*Rejected (for now):* a `Lang`-associated dyn type (`type CallableSpecExt: ?Sized` set
to `dyn FlmSpec`, plus a defaulted `fn lang_ext(&self) -> Option<&L::CallableSpecExt>`
bridge on the spec trait) — expressible and object-safe, but it adds an associated type
to every hand-written `Lang` impl (the `TrivialLang`-cliff cost, cf. [§dd-dr:parsers-engine]) for a need the
wrapper covers; recorded here as the upgrade path if the wrapper proves annoying in
FLM practice. This also unblocks the flagged default-factory escape hatch ([§dd-dr:parsers-engine]): the
dispatch loop *can* now detect `StdCallableSpec` and elide the per-invocation `Box`, if
profiling ever asks for it.

#### Scope-stack redesign: dyn `SpecsProvider` entries, `Package`/`Scope`, in-stack fallbacks [§dd-dr:scope-stack]

Status: DECIDED (user; closes the long-standing LibraryStack-expressiveness question and
supersedes the first-generation `SpecLookup`/`LibraryStack` design — reversal recorded
below).

Driving requirements (user): definition visibility that switches with the parsing mode;
deltas expressive enough to add/remove definitions and load/unload/replace collections up
to wholesale stack replacement; fallbacks no longer delta-inexpressible; deep
customization kept easy. `Library`/`LibraryStack`/`SpecLookup` become:
- **`SpecsProvider`** (dyn, multi-method) — the stack-entry contract: `name()`, fallible
  `retrieve_spec(query, state) -> Result<Option<Arc<dyn CallableSpec>>, _>` (`Ok(None)` =
  not here, continue outward; a misbehaving provider is an `Err`, never a panic),
  specials participation (`scan_specials` + `specials_trigger_chars`, unioned at state
  freeze — pylatexenc's `test_for_specials` precedent meant the trait was never going to
  be single-method), functional `with_definitions(ops)` updates, best-effort
  `iter_symbols()`. All-dyn entries won over a closed entry enum: a well-specified
  multi-method contract keeps generic ops and diagnostics available while admitting
  lazy-loading providers (large spec databases) that closed data precludes. The `with_…`
  methods returning a fresh provider ARE the copy-on-write mechanism — `Arc::make_mut`
  does not exist for `dyn` types.
- **`Package`** (standard impl) — immutable, built once, loaded wholesale (preset driver
  helpers like `load_package(name)` are called by parsers when *building* deltas; the
  state choke point never needs the driver). Mode visibility is a package field checked
  in its own `retrieve_spec`; the stack is visibility-blind.
- **`Scope`** (standard impl) — the definition target; `Define`/`Remove` delta ops
  address a provider by name and route to `with_definitions`. Scopes are created lazily
  on first `Define`; scoped reversion stays structural (outer states hold the old Arcs),
  so lexical scoping falls out of state immutability with zero per-group cost.
- **Fallbacks are ordinary bottom-of-stack providers** (answering any name of their
  callable types; de-keyed specs keep the singletons shareable — an unknown name costs
  no per-instance allocation, and a callable node's spec is never `None` for a type with
  a registered fallback). The stack carries no fallback map, and **no longer implements
  the provider contract itself** — stacks don't nest. Reversal record (July 2026): the
  first-generation design built fallbacks *into* `LibraryStack` and let stacks nest as
  lookups; a nested stack's fallback would then preempt an outer stack's real
  definitions, and the redesign removes that hazard structurally instead of
  re-mitigating it. Exhausting the stack is
  a structured miss carrying the searched provider names (feeding the
  `UnresolvableCommand` "searched: …" detail).
- **No `Masked` outcome** (user): "undefined on purpose" is an ordinary definition — an
  `ErrorCallableSpec` whose invocation parser diagnoses, with a better message than a
  mask could carry. Shadowing with it suppresses lower entries *and* the fallback purely
  by search order (a theorem of ordering, not an extra rule). `Remove` genuinely deletes,
  from `Scope`s only.
Rejected alternatives: evicting definitions from core entirely ("skeletal" — core already owns
`CallableSpec` and the never-`None` node-spec guarantee, and the generic delta channel
for definitions must be core or every `Lang` rebuilds the same machinery where generic
code cannot reach it); the status quo ("minimal" — opaque single-method callbacks
degenerate into multiplexed `resolve_command`: no introspection, no targeted ops, no
removal, no diagnostics); a `Masked` resolution outcome (a third behavior path every
consumer carries forever, for a rarely-wanted operation); a closed entry enum with a
`Custom` escape variant (makes customization second-class and precludes lazy-loading
packages); eager scope-per-group pushes (churn — CoW makes them unnecessary);
interior-mutable scopes for `\global\def` (observable mutation of frozen states; breaks
the reader-memoization contract) — `\global` is DEFERRED, sketched as upward propagation
of definition ops through the existing parser after-effect return channel.
Revisit if: per-definition mode visibility is needed beyond what custom providers
cover, or provider-fold resolution cost shows up in profiles (a freeze-time merged map
à la `PrefixTable` is the prepared answer).
Checkpoint resolutions (implementation-settled):
*(a) Fallibility:* `derived()` returns `Result<ParsingState, DeriveError<L>>`; a delta
without scope ops cannot fail. Failing ops are **skipped** (the rest of the delta still
applies, in order) and collected; `DeriveError` carries the mechanical failure records
**plus the fully derived recovered state and the applied delta** (the
`String::from_utf8` pattern — recovery material rides in the error; the delta is
carried because the group-interior seam derives with a *merged* delta its caller never
built, and a recovering caller must still feed `observe_transition` the true
transition). The error is deliberately *unclassified* — the same failure kind can be an
extension bug or an embedder input error depending on who built the delta, which the
type cannot know; **the seam classifies**: the `ParseContext` derivation sugars route
each failure through the recover funnel as a `ScopeOpFailed` condition (strict parses
abort on the first one; tolerant parses record them and continue under the recovered
state, committing the observation themselves), while out-of-parse callers treat `Err`
as their own input error. User-decided over the abort-only alternative (mapping op
failures to `ImplementationError`, which bypasses tolerance): op failures behave as
recoverable source-style conditions so tolerant parses stay alive. Failed derivations
are never memoized and never observed — the session memo gate extends the old
`push_libraries` exclusion to `scope_ops`, so the memo caches successes only, and a
misbehaving driver descent re-reports per descent (loud, not cached away).
*Follow-up:* the `Err` is large (≥ 424 bytes — it owns a full
state plus the delta), tripping `clippy::result_large_err` at every function returning
it. Accepted as-is, user-decided over `Box<DeriveError>`: the recovery payload is the
point of the type, and `Box`-free signatures are worth the bigger `Result` return
slot. The five returning functions carry a targeted `#[allow]`, with the rationale
documented on `DeriveError` itself.
*(b) Specials fold:* longest match wins, ties innermost — verified as *exact*
pylatexenc parity (`test_for_specials`: a strictly longer match beats an
earlier-searched category, ties keep the first-searched); since equal-length matches at
one position are the same spelling, the tie rule *is* redefinition shadowing.
Provider-side `scan_specials` returns the exact shape of the `Lang` hook it feeds
(`Result<Option<SpecialsMatch<L>>, SpecialsScanError>`), not
`Result<_, ProviderError>` — scanning providers report in the scanning layer's own error
form ([§dd-dr:specials-scan-errors]), and the `ScopeStack::scan_specials` fold propagates
the first `Err` (innermost-first) with no translation. Per-provider trigger chars are deliberately
state-independent and unioned at freeze: a mode-invisible provider's chars stay in the
cached filter and its scan declines instead (conservative superset).
*(c) Shapes:* `ProviderError` is a `#[non_exhaustive]` structured enum (`NotMutable`,
`UndefinedName{name}`, `Failed(message)` — the `NodeBuildError` precedent);
`ScopeStackError { provider, error }` attributes in-stack failures; a resolution miss
is plain `Ok(None)` with the "searched: …" detail composed via
`ScopeStack::searched_providers()` (a `Display` adapter) — the stack is
visibility-blind, so a miss always searched the *whole* stack and the searched set is a
property of the stack, not of one miss (no per-miss allocation).
*(d)* `iter_symbols` was deferred (adding a defaulted trait method later is non-breaking)
and later landed — cf. [§dd-dr:iter-symbols].
*Settled in flight:* module renamed `library` → `scopes` with `StateData.scopes` /
`state.scopes()` (user choice — no type named `Library` survived); delta-level
`ScopeOp` is flat (carries the target scope name) while provider-level `DefinitionOp`
has the routing consumed — the provider *is* the scope; `Define` into an absent scope
name **lazily creates** a fresh `Scope` innermost (the "lazily on first Define"
semantics), while `Unload`/`Replace`/`Remove` of absent names are errors, never silent
no-ops (op builders can always check the visible state first); `with_definitions` is
atomic per call; `Package` carries **specials as plain data** (`insert_specials`,
matched longest-first within the package) — pylatexenc's categories hold specials, and
without it packages would not be wholesale-loadable — with mode visibility checked in
both `retrieve_spec` and `scan_specials`.

#### `iter_symbols`: enumeration with a required type filter; `ClosedVocabulary` [§dd-dr:iter-symbols]

Status: DECIDED (user).

Defaulted `SpecsProvider::iter_symbols(callable_type: L::CallableTypeId, mode:
L::ModeId) -> Option<Box<dyn Iterator<Item = SymbolEntry<'_, L>>>>` — the enumeration
counterpart of `retrieve_spec`'s point queries, closing the earlier deferral. Key points:
- **The mode is passed directly, not a `&ParsingState`** (user question, verified in
  source): visibility is mode-determined at both of `Package`'s grains
  (package-level + per-entry `visible_modes`), so the mode is the whole input. `Scope`
  entries carry no visibility and enumerate under every mode; each provider
  pre-filters, mirroring its own `retrieve_spec`.
- **The type filter is required, not `Option`** (user choice): "list everything" is
  driven per type from outside. Specials definitions are ordinary entries *of their
  recorded type* — both `Package` tables contribute mechanically (the trigger spelling
  is the row's `name`), and core never learns which type "means" specials.
- **`None` = cannot enumerate** (the default — `FallbackProvider` answers *any* name);
  `Some(empty)` = enumerable with nothing visible. `SymbolEntry` is a borrowed
  self-describing row `{ callable_type, name, spec }`.
- **`ScopeStack::iter_symbols` dedups by name, first-visible-wins innermost-first** —
  exactly `retrieve_spec` resolution order. The specials scan-fold's
  longest-match-wins rule is *positional* resolution between distinct triggers, not
  definition shadowing; identical triggers tie innermost, which is the same first-wins
  rule — so one dedup covers both worlds. Unenumerable providers are skipped, not
  errors.
- **`ClosedVocabulary` (`const ALL: &'static [Self]`, in `state`)** closes the
  enumeration gap a required filter creates: `L::CallableTypeId` has no
  list-the-variants bound, so generic whole-scope tooling states the opt-in bound and
  iterates `ALL`. Deliberately **not** required by `Lang`: `TrivialLang` defaults the
  id types to `u32`, which has no value list. The preset implements it for all three
  vocabularies; `#[non_exhaustive]` enums keep `ALL` in sync by same-change
  discipline (compiler can't enforce it). It stays opt-in under the generalized
  preset — **not** a role-trait or `LatexlikeLang` supertrait ("provide, don't
  require", user): no shipped function requires the bound; the did-you-mean miss
  detail needs no vocabulary enumeration (its callable type and mode are already in
  hand at the miss site, [§dd-dr:resolution-extraction]); and the parse-init
  escape-char check ([§dd-dr:registration-ergonomics]) ships as a bound-where-used
  check function, `core::specs::check_provider_commands_shadowed_by_escape` — wired
  through the defaulted core hook `ParseDriver::observe_parse_start` (once per root
  parse) plus a defaulted no-op `LatexlikeLang::check_parse_start` behavior method
  that `Latexlike` overrides with the unconditional call (trait impls cannot state
  per-method bounds, so each family member opts in monomorphically, where its bound
  trivially holds), gracefully absent for non-enumerable vocabularies — a
  best-effort diagnostics nicety, not semantics.
Rejected alternatives: an `Option`al type filter (generic listing without the vocabulary bound —
user preferred always-filtered plus statically listable vocabularies); a
`&ParsingState` parameter (nothing beyond the mode feeds visibility); state-blind
enumeration with visibility data carried on entries (information without a consumer);
excluding specials or a separate `iter_specials` (the recorded-type framing unifies
the tables with no extra surface).

#### Registering callables: conversion idiom, one-liners, no insert-time validation [§dd-dr:registration-ergonomics]

Status: DECIDED (user, API-review session).

Three rulings on the registration surface:

1. **Arc removal via one sealed conversion idiom.**
   `ParsingState::lang_initial_with_packages` takes an `IntoIterator` over a sealed
   **`IntoSpecsProvider`** conversion (accepting `Package<L>` by value, `Arc<P>`, and
   `Arc<dyn SpecsProvider<L>>`); `Package::insert`/`insert_specials`/`…_in_modes` get
   the sibling treatment for specs — `insert(CallableType::Macro, "emph",
   MacroSpec::new(…))` with no `Arc::new` anywhere, pre-shared flyweights still
   accepted. (A plain `Into<Arc<dyn …>>` bound cannot express this: unsized coercion
   is not `From`, and blanket impls hit coherence walls — the sealed trait is the
   mechanism. Coherence pitfall: on a `Lang`-generic conversion trait, a *blanket*
   by-value impl and the `Arc` pass-through impls overlap — downstream
   `impl CallableSpec<TheirL> for Arc<…>` is orphan-legal — so `IntoCallableSpec`,
   and the resolver-side `IntoSourceResolver`, carry a sealed, never-named
   inference-marker type parameter distinguishing the argument shapes;
   `IntoSpecsProvider` needs no marker, its by-value impl being the concrete
   `Package<L>`. No double-wrap.) The `insert` vs `insert_specials` parameter-order
   flip is fixed while
   breaking is free: `insert_specials(callable_type, trigger, spec)`.
2. **Preset one-liners**: `define_macro(name, codes)` / `define_environment(name,
   codes)` as inherent methods on `Package<LLL>` in the latexlike module
   ([§dd-dr:inherent-preset-sugar] precedent), `Result`-returning (argument codes are
   parsed), pairing spec type to callable type correctly by construction. Principle
   recorded (user): **a shorter spelling of the same operation is not a second
   canonical path** — one-canonical-path targets different *ways* (model-level
   duplicates like the removed `with_provider`), not shorthands; these collapse a
   five-name literal ceremony both walkthrough personas flagged.
3. **No insert-time validation — deliberately.** Escape-char checks at registration
   are wrong in principle, not merely wrong-layer: escape characters can change
   mid-parse, and a leading escape char can be intended (`@greet` under
   `\makeatletter`-style situations — or registered before `@` *becomes* an escape
   char). The trap is caught where it bites instead: (a) on a resolution miss, a
   **did-you-mean** detail in `resolve_command_in_scopes`'s miss arm
   ([§dd-dr:resolution-extraction]): after the searched-providers detail, the
   enumerable providers are scanned innermost-first for the escape-prefixed
   registration ("provider 'p' defines '\greet' — command names are registered
   without the escape character") and for capped small-edit-distance suggestions
   (accepted
   limitation: an in-stack fallback provider means the miss path never fires);
   (b) at **parse initialization** — the layering-correct moment: the diagnostics
   sink is live and the `TokenRules` escape char is known — a warning diagnostic
   fires when *all* (≥ 1) of a provider's command definitions start with the escape
   char; (c) a loud normalized-name callout on `Package::insert`. The
   spec-type/callable-type pairing likewise gets **no cross-check**: a mismatched
   registration is documented-legitimate (the environment composition owns the
   parse; the spec contributes argument structure), and the one-liners make correct
   pairing structural on the happy path.

Rejected alternatives: insert-time escape validation (above — also generically
unimplementable: the escape char is a `TokenRules` fact the author-side layer cannot
know); a spec/type cross-check (outlaws documented-legitimate combinations and needs
downcast blacklists); separate conversion traits of different shapes for providers
vs specs (one idiom, learned once).

Revisit if: the did-you-mean scan measurably slows cold miss paths (bound the
iteration), or fallback-provider stacks dominate real deployments (the miss detail
never fires there — the init-time check remains).

#### Command resolution is a standalone `specs` function: `resolve_command_in_scopes` [§dd-dr:resolution-extraction]

Status: DECIDED (user, API-review session; completes the deferred resolver half
of [§dd-dr:public-namespace-topology]).

The standard command-resolution body — build a `CallableQuery` with
`CallableSyntax::Command { escape_char }`, consult `state.scopes().retrieve_spec`,
map hit / clean miss / provider error to `Resolved` / `Unresolved` (with the
`searched_providers` detail) / `Failed` — is extracted from its associated-fn home
on the result enum into a free function in `core::specs`:
`resolve_command_in_scopes<L: Lang>(state, token, callable_type) ->
CommandResolution<L>`, and the whole resolution family (`CommandResolution`,
`ResolvedCallable`, `CallableQuery`, `CallableSyntax`, `SearchedProviders`) is
placed beside it. Grounds: the function's substance is definition lookup — query
construction, provider semantics, miss reporting — author-side vocabulary under
the specs/hub boundary rule; placement follows what the items are *for*, and
`ParseDriver::resolve_command` returning a specs type is an accepted
cross-boundary signature reference. The associated-fn spelling
(`CommandResolution::resolve_via_scopes`) is removed — one canonical path; it was
also a discoverability accident (the walkthrough found it by reading the enum's
docs, not by looking for a resolver). Interactions: the did-you-mean miss detail
([§dd-dr:registration-ergonomics]) lives in this function's miss arm; the strategy
value `ScopesCommandResolver` ([§dd-dr:command-resolver], superseding
[§dd-dr:scopes-resolving-driver]) is its one-line wrapper, placed beside the
family; the resolution-condition wire areas
([§dd-dr:wire-identifier-stability]) name the concept this entry defines.

Rejected alternatives: splitting the family across the boundary (function + query
types in specs, result types in the hub — honors "hub = run-side" more literally
but cuts the family across exactly the seam the topology ruling said should stop
being ambiguous; recorded as the fallback shape); no extraction (the direction was
already ruled in the topology session, and the assoc-fn home is the
discoverability accident above).

Revisit if: a second resolution syntax family (beyond `Command`) wants extracting
— mirror this shape rather than growing this function.

#### Arguments are named at construction: `new(parser, name)` + `new_unnamed` [§dd-dr:named-first-constructors]

Status: DECIDED (user, API-review session).

Naming an argument becomes the primary spelling: `ArgumentSpec::new(parser, name)`
takes the name directly, and the anonymous case is the marked, longer spelling
`ArgumentSpec::new_unnamed(parser)` — pushing spec authors toward named
arguments, which the `_named` accessor family ([§dd-dr:named-argument-errors]) and
the `argument_specs_named` factory already privilege as the robust access path.
The `.named()` builder method is removed (one canonical path: no two ways to set
the name). The parser parameter takes the sealed-conversion treatment (the
[§dd-dr:registration-ergonomics] idiom — by value or pre-`Arc`'d, no `Arc::new`
at call sites), and `StdCallableSpec::new` accepts an `IntoIterator` of specs by
value — the generic-layer registration spelling drops from two `Arc::new`s and
three type names per argument to none. "Unnamed" over "anonymous": the crate's
existing word — no new synonym. **`ParsedSlot` mirrors the convention** (user):
`ParsedSlot::new(region, name)` + `ParsedSlot::new_unnamed(region)`,
`ParsedSlot::named` removed; parameter order is payload-first in both families.
`ParsedArgument` needs no change — it carries no own name (the name lives on its
`Arc<ArgumentSpec>`). Slot-constructor arities follow [§dd-dr:ext-minting]
(`SlotExt` is demanded at construction).

Rejected alternatives: `new(parser, name: Option<…>)` (decorates the *encouraged*
path with `Some(…)` noise — backwards); a descriptor-enum factory
(`[Mandatory(rule), Optional(rule)]` — freezes a second, weaker vocabulary
parallel to the parser types, maintained forever for a rare authoring moment);
leaving `ParsedSlot` unmirrored (an avoidable convention fork between sibling
families).

Revisit if: a real consumer class needs many deliberately-unnamed arguments (the
marked spelling then reads as noise — measure before softening).

## Nodes and the syntax tree [§dd-dr:nodes]

#### Flat `NodeTree` (Vec + index ranges), frozen after parse, `NodeRef` proxy access [§dd-dr:flat-node-tree]

Status: DECIDED.

Cache-friendly, no per-node heap allocation, trivially serializable; `NodeRef`
(Copy, borrows `ParseResult`) makes indices safe by construction — the borrow checker
guarantees a `NodeRef` can't outlive the tree its index points into. Mutation happens only
inside `ParserSession`; `finish()` consumes the session, so there is no mutable/immutable
conflict by design.

#### Closed `NodeKind<L>`: unified `Callable`, two-tier ext, no `Custom` variant [§dd-dr:closed-node-kind]

Status: DECIDED (user-led design discussion; "Option F").

The structural taxonomy is `Chars`/`Group`/`Callable`/`Comment`/`List`;
macro/environment/specials are invocation *forms* (`CallableTypeId` on `CallableData`), not
node kinds; custom data rides in the ext bundle (`Lang::NodeExts: NodeExtTypes` — the
member roster is [§dd-dr:ext-minting], which removed the per-kind node-ext tier this
entry first carried, leaving `NodeKind` purely structural). `NodeExtTypes` is defined
next to `Lang` in the state topic, not in `node/` (moving it would recreate a module cycle for
cosmetics); `TrivialLang`'s blanket impl provides the all-defaults shortcut.
The resolution argument, in full:
- The original proposal — closed structural enum + `Custom(L::NodeData)` variant —
  conflated two needs: *extra per-instance data on a node that IS structurally a
  group/callable/…* (the common case) and *genuinely new structural shapes* (rare; no
  concrete example survived scrutiny — custom constructs are still invocation-, group-, or
  leaf-shaped). Making `Custom` a *sibling* of the structural variants meant attaching data
  destroyed structural identity: a group with custom data stopped being a group to all
  generic tooling.
- The `Callable` merge (macro/environment/specials differ by invocation form, not by parsed
  shape) is itself a de-privileging move — "environment" was a preset concept wrongly
  enshrined as a core node kind — and it made the ext bundle affordable.
- The merge required recording the invocation form somewhere ⇒ `CallableTypeId`, which
  also became the definition key space and the per-form unknown-fallback hook. Specs were
  **de-keyed** (behavior only, no name), enabling flyweight sharing across names and
  shared-singleton unknown-specs — a callable's spec is never `None` with zero
  per-instance allocation.
- **Names are owned** (`Box<str>`): identity-bearing, and span-backed names would force
  synthetic nodes (transforms creating callables — FLM's bread and butter) to fabricate
  sources. The same argument generalized to content fields ⇒ **`TextContent`**
  (span-backed when parsed, owned when synthesized/normalized), which made normalization
  representable and level-2 recomposition self-contained. `post_space` is kept and
  reproduced verbatim (reproduce, don't guess); the whitespace-as-chars-nodes rule restores
  the exact sibling-span tiling. Args vs. slots stay two named concepts over
  shared machinery — the boundary is a spec-owned guideline, not core law.
Rejected alternatives: `trait Node` + `Box<dyn Node>` + `as_any()` downcasting + `clone_box()` (the
generated trait-based design) — loses exhaustive matching, adds per-node boxing, makes
serialization and flat storage impossible, and reintroduces runtime type errors that the
type system should prevent; annotation wrapper nodes (re-create the problem one level up);
side tables (break node self-containment across tree transforms).

#### No core `MathNode` [§dd-dr:no-core-math-node]

Status: DECIDED (consequence of [§dd-dr:no-privileged-concepts] and [§dd-dr:closed-node-kind]).

`$…$` parses as a `Group` with a `$`-delimited `GroupTypeId` under a math-mode state extension;
the latexlike preset provides accessor helpers so ergonomics don't suffer.
Revisit if: preset-level ergonomics prove genuinely painful in practice — the fallback is
preset-defined ext data on the `Group` kind, still not a core variant.

#### `ParsedArguments`/`ParsedSlots`: self-describing argument records [§dd-dr:parsed-arguments]

Status: DECIDED (user; replaces the first-cut `ArgsLayout`/`SlotsLayout` offset maps,
following pylatexenc's `ParsedArguments` pattern).

`ParsedArguments<L>` holds one `ParsedArgument<L>` per spec'd argument:
`{ spec: Arc<ArgumentSpec<L>>, region: Option<ChildRegion>, ext: ArgumentExt<L> }`;
`ParsedSlots<L>` holds `ParsedSlot<L> { name: Option<Box<str>>, region: ChildRegion,
ext: SlotExt<L> }` (slots carry a name, not a spec — spec-side slot declarations do not
exist, cf. [§dd-dr:no-spec-side-slots]). Key points, each argued in the session:
- **Self-describing records.** Every argument entry carries the `Arc`'d spec it was parsed
  against — pylatexenc keeps `arguments_spec_list` next to `argnlist` for exactly this: a
  custom invocation parser may produce an argument structure the callable spec didn't
  declare (`\newcommand`-alikes), and the record must stand alone.
- **Presence lives *inside* the entry** (`region: Option<ChildRegion>`), not as
  `Vec<Option<ParsedArgument>>`: absent optionals keep their spec, so by-name lookup
  distinguishes "not provided" from "no such argument". This zips pylatexenc's two parallel
  lists into one array-of-structs. Presence is `Option`-ness of the region, not node
  existence — an empty provided region is representable.
- **Provided markers are `Chars` nodes** (pylatexenc's `LatexOptionalCharsMarkerParser`
  returns a chars node for `*`): every provided argument has content nodes, and a three-way
  `Absent`/`Present`/`Marker` enum is unnecessary.
- **No stored name→index map**: lookup scans the entries' spec names (argument counts are
  tiny; the specs are the single source of truth). Add a cache only if profiling ever says
  so.
- **Content access is computed — but content membership is recorded.** Extraction
  conveniences stay computed accessors (pylatexenc's `get_content_nodelist()` /
  `get_content_as_chars()`; stored copies would diverge under transforms), while *which
  nodes are content* is recorded per argument, parser-designated
  ([§dd-dr:child-regions]) — eliminating pylatexenc's lone-group unwrap heuristics. The
  extraction-view API: [§dd-dr:read-api]. What *is* stored beyond the region: the
  `ArgumentExt` slot in the `Lang::NodeExts` bundle, for extensions caching derived
  per-argument data (e.g. `{ref_domain, ref_key}` from a `fig:Abc` argument) — minted
  by the argument parser at record creation: the parser output carries it, the record
  constructor demands it, and the standard parsers are conditionally defined
  `where ArgumentExt<L>: Default` ([§dd-dr:ext-minting]).
Rejected alternatives: parallel `specs`/`args` vectors (pylatexenc-literal — an unenforced
length/pointer-consistency invariant and a redundant `Arc` when the spec also sits in the
entry); "layout" as a name (opaque — nobody could say what it referred to).

#### `SlotExt` — slot records carry per-instance ext, symmetric with `ArgumentExt` [§dd-dr:slot-ext]

Status: DECIDED (user).

`ParsedSlot` gains `ext: SlotExt<L>`
(`Lang::NodeExts::SlotExt`, `()` under the no-ext bundle), mirroring
`ParsedArgument.ext`. Rationale: the asymmetry bit exactly where FLM is richest — an
environment's *body* is a slot, and per-instance derived data about a body (tabular cell
structure, enumerate item boundaries) had no home except the whole-callable ext. Added
while cheap: one associated type on the bundle, one field on the record; retrofitting after
downstream `NodeExtTypes` implementors exist would break them all. `SlotExt` values are
demanded at `ParsedSlot` construction (no `Default` path); slots additionally carry a
`SlotRole` with trait-based body marking, the preset claiming the `SlotExt` member
([§dd-dr:ext-minting], [§dd-dr:slot-roles]).

#### `NodeTree::iter` renamed `iter_storage_order`; no `parent` stored in `NodeData` [§dd-dr:iter-storage-order]

Status: DECIDED (user).

The flat iterator yields storage
(breadth-first) order — `a`, `c`, `b` for `a{b}c` — which a name as generic as `iter`
invites consumers to mistake for document order; the rename makes the iteration order
part of the signature. The document-order `descendants()` arrived with the read API
([§dd-dr:read-api]), once it had consumers. Upward navigation was first declined as
not needed, then landed once consumers materialized: `finish()`'s parent vector is
kept on the tree (`parent()`/`index_in_parent()` — [§dd-dr:tree-navigation]). Named
argument-node accessors (`argument_nodes_named` etc.) likewise landed
with the read/extraction package, not piecemeal.

#### Argument/slot child regions with parser-designated content (`ChildRegion`, `ContentNodes`) [§dd-dr:child-regions]

Status: DECIDED (user, regions session; supersedes the earlier one-node-per-argument
encoding and per-argument `pre_space` — the argument encoding's final shape).

A callable's children range is
the concatenation of one contiguous **region** per provided argument, then one per slot. A
region holds the argument's full syntactic extent in source order: leading noise (comment
nodes and whitespace-only `Chars` nodes — `pre_space` is deleted; whitespace before an
argument is a node like everywhere else, matching the whitespace-as-chars rule and
pylatexenc's expression parser), the syntax-bearing node(s) (a `Group` for `{…}`/`[…]` with
delimiters on `GroupData`; a `Chars` node for `\frac 1 2` single tokens and `*` markers,
which **count as content** — pylatexenc parity), and any trailing per-instance syntax.
Records: `ParsedArgument { spec, region: Option<ChildRegion>, ext }` and
`ParsedSlot { name, region, ext }` (the slot record carries `name: Option<Box<str>>`, not
a spec — cf. [§dd-dr:no-spec-side-slots]); a resolved `ChildRegion` =
`{ children: Range<u32>, content_range: Range<u32>, content_parent: NodeId }`, **all in
global node-index coordinates** (the `NodeData.children` system — one coordinate language,
no per-callable base arithmetic). `content_parent` is the node whose child list holds the
content (the callable itself for region-level content): it preserves "the group node of
this argument" / "the body `List` of this slot" without heuristics, and anchors empty
content (`\m{}`). Key points:
- **Content is parser-designated, never heuristically unwrapped.** For `\textbf{abc}` the
  standard parser designates the group's children; for `[{arg with ]}]` the *inner* group's
  children; `content_nodes` reads are plain range slices. pylatexenc comparison (checked in
  its source): its expression parser collects pre-comment/whitespace nodes, but the standard
  argument parser *drops* them by default (`return_full_node_list=False`, spans-only
  recovery), and `get_content_nodelist()` needs a lone-group unwrap plus the
  `unwrap_double_group` hack — both warts die here, and noise is kept out of content's way.
- **Noise ownership is the argument parser's** (pylatexenc-style), *not* a centralized
  `ArgumentsParser` scan: noise policy is inseparable from argument syntax — a verbatim
  argument whose delimiter is the comment char must see raw tokens, and the scan must run
  under the argument's own parsing-state delta. Standard parsers share one noise-scan helper;
  no noise knobs on `ArgumentSpec`. **Absent means zero consumption**: noise
  scanned while searching is rewound and re-parsed as enclosing content (an absent-optional
  probe before a present mandatory re-scans the same noise — by design); abandoned staged
  nodes are dropped by the builder.
- **Two-phase records — the accepted "honest cost".** Global ranges name positions in the
  breadth-first flattened layout, which does not exist while parsers run (a node's final
  index depends on unparsed input — siblings of its ancestors discovered later). So regions
  are *staged* (child offsets into the callable's child list + a `ContentNodes` designation:
  `InRegion(sub-range)` / `InChildrenOf(BuildId, child sub-range)` — contiguity by
  construction, O(1) even for huge slot bodies, empty ranges stay anchored) and *resolved in
  place* by `NodeTreeBuilder::finish()`. The phase is a runtime invariant the type system
  can't see — the same genus as the earlier-rejected set-before-use field protocol —
  accepted here because resolution happens in a single component at a single point, finished
  trees cannot contain staged regions, and the resolved-only accessors panic on staged
  records (a caller bug under the builder's panic policy). Bought with it: parsers build
  `ParsedArguments`/`ParsedSlots` directly and `add()` keeps its signature — no bespoke
  staging API.
- **Builder checks:** hard asserts at `add()` (regions staged / in bounds / ordered /
  non-overlapping; designation sub-ranges within their parent's child list) and at
  `finish()` (content parent reachable and inside its own region's subtree — only checkable
  once the layout exists); debug-assert that regions **tile** the child list exactly (the
  [§dd-arch:nodes] region tiling, mechanically checkable).
- **Consequences accepted:** a callable's child list is the raw-syntax view (child count ≠
  argument count; `\frac 1 2` costs two whitespace `Chars` nodes); an argument has no single
  node identity — transforms and views splice child *ranges* ([§dd-dr:read-api]);
  `NodeRef::argument(i)`/`argument_named()` are replaced by region/content-nodes accessors;
  `ParsedArguments` holds no `TextContent`, so its materialization plumbing is deleted.
  Callable post-space lies outside the region tiling and is whitespace-only by
  construction (trailing comments are never consumed); it rides the invocation-syntax
  payload ([§dd-dr:invocation-syntax]).
- **Slots mirror arguments** (same `ChildRegion` type), keeping the body `List` as the
  content parent (span/state/ext identity; "an empty body exists"); terminator syntax is settled
  separately — rigid scaffolding is reconstructed, cf. [§dd-dr:environment-scaffolding].
Rejected alternatives: centralized noise scanning (breaks verbatim-delimiter arguments; scans under the
wrong state); noise as `TextContent` blobs (comments lose node identity — invisible to
visitors and transforms); a wrapper `List` node per argument (extra node, unnatural shape);
`content_child: u32` marking a single node (can't express content inside groups, multi-node
content, or trailing syntax); a *relative* `content_range` (child offsets cannot name a
group's children — they are not the callable's children); flattening argument delimiters
into sibling syntax nodes (the same braces would get two representations depending on
structural role, and argument values lose their group class); lone-group unwrap accessor
heuristics (parser intent is not reconstructible after the fact); `Vec<BuildId>` content
designation (contiguity by checked contract instead of by construction; O(k) for slot
bodies; empty content loses its anchor); flattening region contents directly into the
children range with `(offset, len)` layout entries (regions lose node identity — no span,
no ext anchor — and visitors see argument and body content indistinguishably mixed);
separate `Vec<NodeId>` region lists inside `CallableData` (duplicates the children
mechanism, exempts callables from the flat-tree contiguity invariant, costs per-callable
allocations).
Revisit if: re-minting the layout-dependent ranges in transforms proves error-prone (by
design, any new tree re-resolves records through its own builder).

#### Group nodes store their delimiters: `NodeKind::Group(Box<GroupData<L>>)` [§dd-dr:group-delimiters]

Status: DECIDED (user; follows pylatexenc's `LatexGroupNode.delimiters`).

`GroupData<L>` = `{ group_type: Option<L::GroupTypeId>,
open: TextContent, close: TextContent, ext }`.
Rationale: a `Group` whose delimiters were only recoverable through the `Language`
registry violated the already-stated rule that recomposability must not depend on `Lang`
cooperation (marker spellings were stored on the node for exactly that reason) — detached
and synthesized groups couldn't recompose; delimiter-sensitive consumers (pylatexenc's
double-group unwrap compares `delimiters[0]`) need the strings directly. `TextContent`, not
`Box<str>`: span-backed zero-copy when parsed, owned when synthesized; empty `close` on
tolerant "close never found" recovery. `group_type` is **kept alongside** the strings as the
typed identity — the group's *class* ([§dd-dr:group-classes]): "is this a math group?"
needs no string comparison, while `$…$` vs `$$…$$` share a class and are distinguished by
the stored delimiter strings — and is `Option` so *internal synthesized groups* — structural
groups corresponding to no language group type — are representable (user amendment). Boxed
for the same reason `CallableData` is: `Chars` must keep dominating the enum size.
`NodeKind::Comment` follows the same pattern: three inline `TextContent` fields
(start delimiter, content, post-space) would make `Comment` the largest variant,
about three times `Chars`, so the payload is the boxed `CommentData` (fields in
source order; no `Lang` parameter, since nothing in it is language-specific) —
comments are rare relative to chars, so the per-comment allocation matches the
group/callable trade.
Rejected alternatives: delimiters-only (pylatexenc-pure — group classification degenerates to string
comparison); registry-only (the inconsistency above).
Revisit if: per-group-node allocation shows up in profiles (then consider inlining a
small-string delimiter pair).
#### Node spans stay mandatory; synthetic-node representation deferred [§dd-dr:mandatory-node-spans]

Status: DECIDED (user).

`NodeData.span: SourceSpan` is non-optional: parse-produced
nodes always have a real span, and level-1 recomposition (span → verbatim text) is
unconditionally available. How *transform-created* nodes represent provenance (empty span
anchored at the insertion point, a `Synthesized`-provenance source, a detached variant, …) is
decided together with the transform/visitor API (still future work).
Rejected alternatives: `Option<SourceSpan>` now — every span consumer grows a `None` case that no
current code path can produce, and `TextContent::Spanned` would be unresolvable on span-less
nodes (forcing an awkward "span-less ⇒ all content owned" side invariant).

#### Staging builder with breadth-first flatten [§dd-dr:staging-builder]

Status: DECIDED.

`NodeTreeBuilder` stages nodes bottom-up with explicit child-id lists; `finish(root)` lays the
tree out breadth-first (root at index 0, each node's children appended as one contiguous
block). Child ids must already be staged — cycles are unrepresentable by construction — and
each node is claimed as a child at most once; staged nodes unreachable from the root are
silently dropped (parsers may abandon speculatively built nodes on tolerant-recovery paths).
Rationale: `children: Range<u32>` requires *sibling*-contiguous storage, and no direct
arena-emission order provides it — recursive descent gives subtree-contiguous layouts
(`G(c1(d1,d2), c2(e1))` emits `d1,d2,c1,e1,c2,G` post-order; `c1` and `c2` are not adjacent).
Staging + flatten is O(n) with one transient copy, and keeps the builder API free of layout
obligations. The builder is hook-free, with a single
`add(kind, span, state, children, ext, annotation)` demanding ready values
([§dd-dr:ext-minting], [§dd-dr:node-annotations]).

#### `TextContent` is S0 and lives in the source topic; no `PartialEq` on node types yet [§dd-dr:text-content-s0]

Status: DECIDED.

Home: `source/text_content.rs` — its `Spanned` variant is a
`Span` into a source, and materialization is a source-content operation; the node topic (S1)
merely uses it. No `PartialEq` on `TextContent`: logical-text equality of a `Spanned` value
requires the source content, so a structural `==` would be a footgun (`Spanned(2..4)` vs
`Owned("ab")` may denote the same text); comparisons go through resolved `&str` (node-level
accessors). Node/layout types likewise ship without `PartialEq` until golden-test needs make
the right equality concrete.

#### `Comment` nodes store their start delimiter and post-space [§dd-dr:comment-delimiters]

Status: DECIDED (user).

`Comment { content, start: TextContent, post_space: TextContent, ext }`; the node's span
covers start delimiter + content + post-space (the token's span convention).
Rationale: with several `CommentRule`s in scope, *which* start delimiter fired and what
syntactic post-space followed (newline + indentation) are per-instance facts; storing both
mirrors `CallableData.post_space` and the recorded-delimiter principle (`GroupData`), making
level-2 recomposition self-contained, synthesized comments included.
Rejected alternatives: recovering either from the span (fails for synthesized comments) or from a
`Language` default (guessing).

#### Environment scaffolding is rigid syntax, reconstructed — not nodes, not a record [§dd-dr:environment-scaffolding]

Status: DECIDED (user; closes the terminator-representation question left open by the
regions session), then **partially superseded** (API-review recompose session): the
reconstruct-don't-record half is reversed — scaffolding facts are recorded as
invocation-syntax payload, cf. [§dd-dr:invocation-syntax] and the closing note
below.

An environment-shaped callable's span covers the whole `\begin{align}…\end{align}` extent
(plus post-space); its children are the argument regions followed by the body `List` — one
contiguous block whose span runs from the first argument region to the body's end. The
`\begin{name}` / `\end{name}` bytes are the block's prefix/suffix complement within the
node's span and are not otherwise represented.
Rationale: the syntax is deliberately **rigid** (a deviation from LaTeX): no comments or
newlines between the begin/end command and its name group — the name group must be the
immediately following token; inline whitespace after `\begin`/`\end` (the command token's
post-space) is tolerated and *not recorded*, an accepted level-2 normalization to the
canonical spelling. Under rigid syntax, reconstruction from `(callable_type, name)` + spec
knowledge is deterministic — "reproduce, don't guess" holds because there is nothing to
guess. Span tiling holds in its callable form: regions tile the child list, the
children block is span-contiguous, and the scaffolding is derivable as the two sub-spans
(node-span start → children start) and (children end → post-space start). A preset that
wants the verbatim scaffolding strings anyway extracts exactly those two sub-spans at
`Lang::finalize_node` time ([§dd-dr:parsers-engine]) and stashes them in node ext.
Rejected alternatives: a `terminator: TextContent` record on `ParsedSlot` (every environment pays
storage for a string that rigid syntax makes reconstructible); terminator as region nodes
(`\end`'s command bytes have no honest node kind — a `Chars` node holding markup would
violate chars-are-content).
Revisit if: a construct's closing syntax is genuinely per-instance-variable (a fence
closing with its own trigger text is fine — that is `name`; a freely chosen close spelling
is not): that construct's parser then records the choice on the node, following the
`GroupData` delimiter precedent.

*What the supersession changed:* the per-node recomposition doctrine
([§dd-dr:recompose]) reversed reconstruct → **record** — the begin/end facts (per
side: escape char, command word, post-space, name-group rule) are recorded on the
node as the environment arm of the Lang-owned invocation-syntax payload
([§dd-dr:invocation-syntax]): the `EnvironmentSyntax`/`StdEnvironmentSyntax` record
is constructed once at staging via `from_parsed(begin, terminator)` with the
spelling writer pair; the composition owns all scanning, and the body parser's
terminator facts flow back through `EnvironmentBody::terminator`. The "tolerated
and *not recorded*" post-space clause no longer holds: the per-side record keeps
it. What stands: the rigid parse syntax (strictness Env-owned — a tolerance
variant is a newtype over `StdEnvironmentSyntax`), and both recorded rejections
above — scaffolding is still neither nodes nor slot records (a `Hidden`-slot
storage design was separately rejected).

#### Whitespace and span invariants pinned [§dd-dr:span-invariants]

Status: DECIDED (user).

1. *Chars accumulation:* `Char` tokens accumulate into maximal `Chars` nodes; a token's
   pre-space (content whitespace) joins the run; the run flushes when any non-`Char`
   construct starts. Pending whitespace with no adjacent chars becomes a whitespace-only
   `Chars` node. Parsed content is always `TextContent::Spanned` (the exact span slice).
2. *Paragraph breaks:* their own nodes, produced via `Lang::make_paragraph_break_node`
   ([§dd-dr:parsers-engine]; default: whitespace-only `Chars` spanning the full token, newlines included);
   never merged into neighboring whitespace nodes (adjacent whitespace-only `Chars` nodes
   are possible and fine — deterministic).
3. *Callable post-space:* **exactly the trigger token's own syntactic post-space** — the
   name-terminating whitespace of a multi-character command, already inside the token's
   span (pylatexenc's `macro_post_space`); nothing beyond it is ever claimed. Storage is
   the Lang-owned invocation-syntax payload ([§dd-dr:invocation-syntax] — latexlike
   records it in `Macro { escape_char, post_space }` and per environment side); the
   recorded fact and its token-only rule are as stated here. Whitespace
   after a single-character command (`\& b`) or after a final argument is ordinary
   sibling/region content, as in TeX. Groups have no post-space (space after `}` is
   content). Comment post-space is the token's (newline + indentation, stopping at
   paragraph breaks). (Reversal record, July 2026: an earlier rule had the invocation parser *claim*
   whitespace beyond the token via a planned `claim_post_space` helper — never shipped,
   and consciously reversed: TeX swallows whitespace only after a control word, so
   claiming more would deviate from both TeX and pylatexenc, and the token-only rule
   keeps a pre-scanned token list faithful. Consequences: for callables with arguments
   the recorded post-space sits between the name and the first argument region — a
   sub-range of the node's span, not necessarily trailing — and environment-shaped
   callables record empty post-space.)
4. *End of stream:* the whitespace before the end-of-stream token — the reader's
   `source_span_between(&tok, StartBeforePreSpace, Start)` answer — materializes as a final
   whitespace-only `Chars` node.
5. *Span tiling:* sibling spans tile the parent's *content interior* exactly —
   `List` bodies, `Group` interiors, the root. For callables: argument/slot regions tile
   the child list (builder-enforced), the children block is span-contiguous, and unrecorded
   rigid scaffolding is the reconstructible complement (previous entry). Checked
   mechanically by a test-utility `check_tree_invariants()` — deliberately a test aid, not
   builder law, so a future construct that legitimately breaks byte-accounting amends a
   test, not the architecture. The core checker is payload-blind; the payload-dependent
   pins — macro spelling + post-space (the childless arm pins containment, not span-end:
   a takeover's `stage_invocation(.., end: Some(&position))` legitimately claims extent past
   the trigger), the specials name-as-written prefix, the environment
   `write_begin`/`write_end` bytes — live in the **preset checker**
   `check_latexlike_tree_invariants`, layered on top ([§dd-dr:invocation-syntax]).

*Amendment (user, span-tiling design session).* The five invariants above are the statement for a language
that obeys span tiling ([§dd-dr:span-tiling]). Under `Lang::OBEYS_SPAN_TILING = false`,
item 1's chars runs record owned text (no single reader answer covers a
multi-token run), item 3's post-space is owned outright (that payload is built before the
node's span exists, so the node-data rule has nothing to test residency against), and item
5's byte accounting does not apply at all — the test oracle then checks the all-trees law alone.

#### Span tiling is a declared property of the language; parsers assume nothing otherwise [§dd-dr:span-tiling]

Status: DECIDED (user, span-tiling design session).

Whether a language's parse trees are **span-tiled** is a static declaration on the language:
the associated const `Lang::OBEYS_SPAN_TILING: bool`, defaulted `true`. The canonical
definition lives on that const; the three components are named here, but the wording is the
const's: the children of every `List` and `Group` node tile the parent's interior (one
source, reading order, no gaps, no overlaps), that a `Callable`'s children block is
span-contiguous within the node's span (with the `Attached`/`Hidden` exclusions of
[§dd-dr:slot-roles]), and that every positional payload sits at its pinned position — a
`Chars` node's content is its whole span, a comment's start delimiter, content and
post-space partition the comment node's span, a group's delimiters are the prefix and the
suffix of its span. That every node carries exactly one span is *not* part of the
definition: that rule is unconditional ([§dd-dr:mandatory-node-spans]).

The problem the declaration solves: the token layer was designed for a reader that serves
one parse from several sources — one that substitutes a macro's definition into the stream
as it reads is the motivating case ([§dd-dr:token-opacity]) — while the tree layer required
gap-free single-source tiling everywhere. About ten parser sites turn two stream positions
into one `SourceSpan` and abort when the pair does not delimit one range of one source, and
the byte accounting of the test oracle rejects the resulting trees; the first third-party
expanding reader hit exactly that abort. The two halves must agree per *language*, because
it is the language's tokenization and its parsers that settle the question, and the answer
is a fact about them rather than a knob a caller turns — hence "obeys". As a const it costs
nothing at run time: each affected parser branches on it and the compiler drops the untaken
arm. It is deliberately not a `LangFeatures` member (that axis declares which *storage* a
language carries, not what its trees guarantee — [§dd-dr:lang-features]) and not a marker
type (breaking, and nothing needs the property at the type level).

The two regimes:

- `true` — every shipped language (the preset included; the in-crate test languages that
  declare `false` are the exception): behavior unchanged, byte for byte. The machinery
  enforces the property (a token stream that breaks it is reported as an implementation
  error) and every span-based accessor answers exactly.
- `false` — the parsers make **no** assumption about where tokens come from: not the source,
  not the reading order, not the absence of gaps.

**The reader describes what it will not delimit.**
`TokenReader::source_span_describing(begin, end) -> SourceSpan` is a required method with no
default body: a reader whose stream cannot be delimited must say what a stretch of it *is*,
and a default would let a reader ship a misleading span it never chose — a missing
implementation is a compile error instead. The answer is the reader's to choose: any span it
considers a useful description of that stretch. The machinery derives nothing from it — no
content, no structure, no ordering; the span becomes the node's span and shows in
diagnostics. Recommended: `begin`'s source, running from `begin` to where the stream last
stood in that source before reaching `end`; where the two positions do delimit one range of
one source, that range. The method always answers (the empty span at `begin` is always
available). `ParseContext::source_span_within` is the public dispatch point — the reader's
`None` is an implementation error for a language that obeys span tiling, and the described
span otherwise; the private `invocation_span_within` mirrors it for the invocation span — so
construct parsers written outside the crate follow the rule without knowing it exists.

**Seams, and what the chars-run check really checks.** The check whose abort that reader hit
is misnamed if it is read as a tiling check: the chars-run loop verifies contract clause 2
(a peeked token's `StartBeforePreSpace` edge is the position the peek happened at) together
with the meaning of `move_to`, and clause 2 is what the reader had violated. The check
therefore stays in force under both declarations, with a message that names the actual
violation. Contract clause 7 (*moving sets the position*) states the other half: with clause
2 it fixes where two consecutive tokens meet — `position_at(next, StartBeforePreSpace) ==
position_at(prev, EndPastPostSpace)` — in every reader, including where `next` is the first
token of another source. A **seam** is such a place, where the next token comes from a
different source than the previous one. The two sides of a seam are therefore *one* position
value; the reader chooses that value and what coordinate it reports for it (the outer
trigger or resume coordinate is the recommended answer, being the one a reader of a
diagnostic can act on). A run of content characters may consequently extend across a seam by
contract, which is exactly why such a run's content cannot be a span and is recorded as
owned text.

**The node-data rule decides content storage, and involves no assumption.** A spelling fact
the reader answered as a span is recorded as `TextContent::Spanned` when it lies in the
node's own source and as `TextContent::Owned` otherwise (`node_text_content`). Both arms
read the very same reader answer; what differs is residency — the property the all-trees law
checks. Single-token facts keep this rule under both declarations, so zero-copy survives
wherever it is sound and no special case appears. What changes under `false` is everything
one reader answer cannot cover: chars runs and verbatim content accumulate owned text token
by token (each token's pre-space, its spelling as the reader classified it, its syntactic
post-space — three answers about that one token). The rule in general form: under `false` no
multi-token `Chars` node records `Spanned` content — recovery nodes and marker-argument
nodes included, since "as described" text is exactly the inaccuracy owned content exists to
avoid. Separately, a payload built *before* the node's span exists — the preset's recorded
macro post-space, which is one reader answer about one token — is owned outright under
`false`, because the node-data rule has no node span yet to test residency against. Text
that *drives* a decision is never read off a described span: an environment name is
accumulated as it is read and answered by `NameGroup::name_text()`, whose
coordinates-and-text pairing is not forgeable (the text field is private; construction goes
through `NameGroup::new`/`with_name_as_read`), and an `\input` reference comes from the
staged argument's node data.

**The consumers rule** (load-bearing, and the reason the two regimes need no consumer-side
switch): techy's consumers obtain content from node *data* — `TextContent` resolved against
the node's own source, names, delimiters, payloads — never from node spans; node spans are
provenance coordinates. The coordinate accessors (`NodeSlice::span`/`source_text`,
`NodeRef::span_content`, `SourceSpan::content`) answer exactly what the coordinates say, and
say so. Consequences: every documented answer of `extract` holds for owned content — after
three doc claims were narrowed to what the code does (`piece_span`, the module docs,
`split_at_chars`) — audited item by item with tests over hand-built owned trees; a source
reemitter re-emits a non-tiled tree *as stored* and claims no byte-equality with any one
source; `validate_tree` — the all-trees law ([§dd-dr:tree-validation]) — is untouched, and
non-tiled parse trees satisfy it; the byte accounting lives only in the test-only oracle,
*the span-tiling law*, which runs the all-trees law alone for a language that declares
`false`.

**Enforcement is testable without an expander.** The in-crate scripted multi-source test
reader (test builds only) serves one parse from a script of segments over several sources —
chains, splices, holes — and is the first in-crate reader with token and position types of
its own. Its positions are kept in a canonical form in which the place past one entry *is*
the place before the next, so contract clauses 2 and 7 hold at seams by construction; a
deliberately broken variant reports the two sides of a seam as two values, and the
parse-level clause-7 test drives it under both declarations
(`techy/src/constructs/span_tiling_tests.rs`, beside the single-source version of the same
check in `constructs/nodes_parser.rs`). Its `source_span_within` answers by the end
position's source, so a run ending exactly at a seam has none: the tiled counter-tests of
that module run seam-crossing and hole-crossing scripts under a tiled twin language and get
the implementation error out of the parser's own run flush — clause 8's enforcement
demonstrated rather than asserted. The preset needed no change whatever to serve a language
of the other regime: a latexlike family member over the scripted tokenization compiles and
parses through the generic driver, specs, syntax record, oracle and source recomposer as
they stand.

Two arms of the crate are right for a reader of this class and reachable by no scanning
reader, and are recorded here rather than left looking accidental. The verbatim recipe
treats the terminator's and the end-of-stream token's pre-space as content, which a reader
re-tokenizing under the recipe state can never produce (that state turns whitespace off);
the scripted reader covers both. `comment_node_kind`'s owned arm needs a comment token with
edges in two sources, which no reader that scans a token inside one segment produces — no
in-crate reader covers it. Both arms are kept deliberately: a reader that splices mid-stream
reaches them.

Costs accepted: under `false` a parse owns its multi-token content (chars runs, verbatim
bodies, environment names) and the pre-staged callable post-space — one allocation per such
node and no zero-copy for content the tree can no longer point at; and two public-API breaks
under the soft freeze ([§dd-dr:stability-rubric]) — `source_span_describing` is a required
trait method (deliberately, above), and `NameGroup` gains a private field, so it is no
longer constructible by struct literal.

Rejected alternatives: a per-*reader* capability flag ("this reader may break the contract")
— the property belongs to the language, the parsers need it at compile time, and a
reader-level flag would let one parse mix the two regimes; flushing chars runs whenever the
source changes — it fixes one of the ten position-pair sites and leaves the remaining
tree-law violations silent; relaxing contract clause 2 at seams — the un-consume and
stop-token behavior every construct parser relies on rests on it; declaring the property in
`LangFeatures`; a marker type instead of a const; a default body for
`source_span_describing`; recording *every* fact as owned text under `false` — the node-data
rule already answers correctly for a single-token fact, so unconditional owning would drop
zero-copy for nothing.

Deferred: a per-driver-instance declaration (the const is per language; a driver that
installs a reader inconsistent with it is still caught by the enforcement); zero-copy
multi-token content under `false` (verify-then-intern); the expanding reader itself, which
lives outside this crate. Also recorded and deliberately not fixed: a traceback frame's
title renders a span as text, and the environment sites hand it a multi-token span, so under
`false` a frame can quote text that was never read (`FrameTitle::Quoted`, and
`FrameTitle::Callable` where a declared argument's frame is built). It is diagnostic
decoration — no lookup and no node data depend on it — and repairing it changes the public
`FrameTitle` (a text field beside the anchor span, or a `TextContent`).

*Amendment (user, span-tiling design session).* An `\input`-style construct's reference
argument carries **plain text**: its content must be plain characters, and an argument
holding anything else (a protective group `\input{{chap.tex}}`, a callable, a comment)
raises the condition `InvalidSourceReferenceArgument`
(`core.sources.invalid-reference-argument`; payload: the closed `InvalidReferenceReason`,
today `NotPlainCharacters` alone) through the recovery policy at the argument's span, with
nothing resolved and nothing attached. What counts is the staged nodes: an unresolvable
command inside the delimiters is recovered as characters and its text becomes the
reference, while a paragraph break staged as a callable node (the preset's
`ParagraphBreakStyle::Specials`) raises the condition. A staged record that does not
resolve is an implementation error, never this condition — a document is not blamed for a
machinery bug. Two consequences, both independent of the declaration: the
reference is read off the staged argument's **node data** under either regime, so the two
share one code path and no span-extent route survives; and no reference is ever taken from
a coordinate span, so the braces-included literal `"{chap.tex}"` is neither resolved nor
reported as an unresolvable reference for a language that obeys span tiling. An *absent*
argument is untouched — the argument parser diagnoses the missing mandatory argument, and
the reference read adds nothing — and a plain-characters reference is unchanged under both
declarations, whitespace inside the delimiters included (characters are taken as read, with
no trimming).

Revisit if: a reader must declare the property per driver instance rather than per language;
or zero-copy content under `false` is demonstrated to matter (verify-then-intern is the
shape); or a construct is found whose legitimate *tiled* behavior the enforcement rejects.

#### Cross-tree `NodeId` misuse: debug-only provenance tags [§dd-dr:node-id-provenance]

Status: DECIDED (user, code-review follow-up session; **superseded** — API review:
tags are now always-on in all builds and part of `NodeId` identity, cf.
[§dd-dr:tree-tags] — the revisit condition below fired).

`NodeTree::node()`'s assert checks *range*, not *provenance*: an in-range id minted by a
different tree silently resolves to whatever node sits at that index — exactly the hazard
of tree transforms, which hold two trees (source + rebuilt) at once. Debug builds now
stamp every tree layout with a tag from a wrapping `static AtomicU32` counter
(`node::tree::next_tree_tag`; `fetch_add` wraps, fine for a heuristic), carry it in
`NodeId` and in resolved `ChildRegion` records, and `debug_assert` the match at the single
choke point `NodeRef::new`. Release builds store and check nothing (`NodeId` stays 4
bytes). The tag is excluded from `NodeId`'s `Eq`/`Ord`/`Hash` so debug and release agree
on id semantics. Layout-preserving copies (`clone()`, `materialize()`) share their
source's tag — their ids are genuinely interchangeable.
Rejected alternatives: the nodes `Vec`'s data pointer as tag (not stable while the builder's vec
grows — ids are minted before the final layout exists — and reused by the allocator after
drop); a debug-only `Box` dummy allocation whose address tags the tree (unique among
*live* trees, but likewise reusable after drop; the counter never repeats short of 2^32
trees). Bare `Range<u32>` node ranges remain uncheckable — they carry no provenance even
in debug; `nodes_in()`'s docs say so.
Revisit if: a public transform surface gives ids/regions a first-class cross-tree remapping story — the
tag then belongs in that design.
The first cross-tree machinery has since landed: the crate-internal
`node::copy::copy_subtree_into` re-stages a finished subtree through the builder,
resolved regions translated back to staging coordinates and re-resolved for the new
layout. Copies get new tags/ids by design (correlation with the original is by span,
same `Arc<Source>`); the tag design is unchanged. A public transform surface remains a
later design.

#### Slot read API: content nodes primary; the wrapper is an explicit, optional accessor [§dd-dr:slot-read-api]

Status: DECIDED (user, code-review follow-up).

The old `slot(i)`/`body()` returned "the node whose children hold the content" — but for
a `ContentNodes::InRegion` designation the builder resolves `content_parent` to the
callable *itself* (there is no wrapper node), so `env.body()` could return `env` and a
naive recursive walker (`walk(n.body())`) would loop forever. Now `body()` returns slot
0's *content nodes* (sugar for `slot_content_nodes(0)`) — shape-agnostic and loop-free by
construction, since a region's content range only ever names strict descendants — and
`slot(i)` is renamed `slot_content_parent(i)`, returning `None` when the content parent
is the callable. This also fixes the naming wobble (`slot(i)` did not return one of what
`slots()` yields) and makes slots symmetric with arguments (node-list accessors first).
The `ChildRegion::content_parent()` *record* accessor is unchanged — records stay
self-describing; only the walker-facing read API refuses to hand back the callable.
*Direction (not yet implemented):* `List` should ultimately never appear as an explicit
*node* in the tree — children are a list of nodes, but a child node itself cannot be a
list; the rare construct that genuinely needs a list child wraps it in a `Group` with
empty delimiters. That removes the body-`List` wrapper (and this accessor's `Option`)
altogether. Until then, environment bodies still build a `List` and
`slot_content_parent(0)` is how consumers reach its span (empty-body anchor) and
parsing state.
Revisit if: the List-free direction lands — `slot_content_parent` then likely
disappears with the wrapper it exposes.

#### Read/extraction API: `NodeSlice` currency, `node::extract` helpers, the builder route [§dd-dr:read-api]

Status: DECIDED (user).

- **`NodeSlice<'t, L>`** — a `Copy` view `{&NodeTree, Range<u32>}` over a contiguous
  sibling run — is what every node-list-returning accessor returns: `children()`, the
  region/content accessors, and the new by-name family (`argument_nodes_named`,
  `argument_content_nodes_named`, `slot_content_nodes_named`). Motivation (user): span
  information belongs **in the return types**, not in a helper recomputing it
  best-effort — `span()`/`source_text()` are *exact* on a tree parsed from a language that obeys span tiling ([§dd-dr:span-tiling])
  (first node's start to last node's end), and `Option`-returning with `None` in
  exactly two honest cases (empty run; cross-source siblings of synthesized trees).
  Iteration via `iter()`/`IntoIterator`; call sites chaining adaptors gained one
  `.iter()`.
- **Helpers are free functions in `node::extract`, not methods on core types** (user):
  the core stays "storage + access", and helpers accrete without touching what a node
  list is. Input split (in-flight consequence): *readers* (`content_as_chars`) take
  `impl IntoIterator<Item = NodeRef>`; *builders* (`split_at_chars`, `parse_keyval`)
  take `NodeSlice`, because an **empty** slice still needs its tree's anchor (state +
  source) to synthesize a result, which a bare iterator cannot provide.
- **The builder route**: splitting cuts *through* chars nodes, and trees are frozen —
  so builder helpers mint a **real `NodeTree`** through `NodeTreeBuilder` (pylatexenc
  parity in mechanism: its `split_at_chars` mints new `LatexCharsNode`s). Whole nodes
  are deep-copied (`copy_subtree_into`; spans/states/specs `Arc`-shared, new ids),
  boundary partials become fresh `Chars` nodes span-backed into the *same* source
  (exact sub-spans, zero-copy text). Result wrappers (`SplitAtChars`, `KeyVals`) own their
  tree privately and expose **primary access** (`segment(i)`, `segments()`,
  `keyval(i)`, `get(name)`) as `NodeSlice` views (user requirement) — one currency,
  so every helper composes with every other (re-split a segment, walk `descendants()`
  of a derived tree). Documented edges: partials of *owned*-content chars nodes
  (materialized trees) keep the whole original node's span as provenance (no byte
  mapping exists to subdivide); partial nodes' ext is minted via `make_node_ext`
  ([§dd-dr:ext-minting]); derived trees'
  sibling spans do not tile (separators omitted) and are exempt from
  `check_tree_invariants`' byte accounting, while *pure* copies satisfy it fully.
- **`parse_keyval` has no policy knobs** (user): entries are recorded **in source
  order with duplicates preserved**, `get(name)` = last occurrence (LaTeX keyval
  override semantics), `value: Option` distinguishes `x` (no `=`, `None`) from `x=`
  (explicitly empty — sharper than pylatexenc, which conflates them via
  `default_value_nodelist`); pylatexenc's `repeated_key_aggregate_action` variants are
  caller one-liners over `iter()`. The lone-value-group unwrap
  (`extract_value_group_contents`) became the `value_content()` *accessor* — the raw
  shape is always kept. Keys flatten via `content_as_chars` and are
  **whitespace-trimmed** (deliberate pylatexenc deviation; LaTeX's keyval packages
  trim). `get_combined_with(key, sep)` (user addition) mints a combined tree over all
  occurrences' values with synthesized separator chars nodes (`Source::synthesized`
  provenance). No insertion-ordered map dependency: the wrapper scans (the
  `ParsedArguments` no-name-map precedent).
- **`descendants()`** (`NodeRef` + `NodeTree` sugar; preorder DFS, self excluded)
  resolves the earlier deferral now that consumers exist (extraction composition, the
  acceptance suite); `iter_storage_order` keeps the breadth-first contrast
  documented.
Rejected alternatives: expanding `NodeRef` into an `InTree`/`AdHoc` enum so split partials could
be tree-less nodes (user's initial sketch, analyzed): ownership cannot attach to the
frozen tree's scope, so results must own storage with views constructed on borrow —
workable, but `id()` loses totality and "a node belonging to no tree" becomes a
permanent core-model tax on every future consumer; a public `Segment`/`SegmentPiece`
second node-list type (user: "I don't like a separate structure for another kind of
node list — that's why we have node lists in the first place"); a slice-level
covering-span *helper* recomputing spans best-effort (user: return types must carry
them); `indexmap`-style ordered-map dependency for keyval; keyval aggregation knobs
(strictly less information than duplicate-preserving entries).

#### `NodeRef::summary()`: the compact node description is core API [§dd-dr:node-summary]

Status: DECIDED (user).

A one-line rendering per node — `chars(ab )`, `group(Math $ $)`, `Macro(emph)`,
`comment( note)`, `list(3)` — promoted from the preset's test support under the dedup mandate: it uses core accessors only (the id types are `Debug`-bounded), so it
is Lang-generic and serves any embedder's tests, logs, and the guide. The format is
documented as human-oriented and **not a stability contract**; structural comparison
(kinds, spans, accessors) remains the exactness tool.
Rejected alternatives: a `Display`-adapter type (the `SearchedProviders` pattern) — heavier API
surface for a test/log utility whose callers want `String` in assertions anyway;
leaving it duplicated test-side (the acceptance suite, the preset tests, and the guide would
carry three copies of the same formatter).

#### `_named` argument accessors: unknown name is an error, absent argument is `None` [§dd-dr:named-argument-errors]

Status: DECIDED (user, API-review session).

`argument_nodes_named`/`argument_content_nodes_named` return `Result<Option<…>, E>`:
`Err` = category error (the node is not a callable, or the name is not among the
spec's declared arguments — the misspelling trap), `Ok(None)` = precisely "declared
but absent", `Ok(Some)` = present. The *indexed* accessors stay pure-`Option` (the
crate-wide Option-on-mismatch idiom), with the `argument_nodes` contract sentence
replicated on all of them and a pointer to the `_named` forms as the distinguishing
alternative. Decisive reason: for a *name*, a silent `None` on a typo is a trap with
no cheap call-site discriminator — and names, unlike indices, are exactly the form the
API recommends; the error is a `Result`, never a panic ([§dd-dr:panic-policy]).
The error type is `core::node::NamedAccessError`
(`NotACallable` / `UnknownArgumentName` / `UnknownSlotName`). The sibling
`slot_content_nodes_named` carries the same error contract but returns
`Result<NodeSlice, E>` **without** the `Option` layer — a recorded slot has no
"declared but absent" state, so an always-`Some` option would force a dead arm on
every caller. `ParsedArguments::get_named`/`ParsedSlots::get_named` stay plain
record lookups (their `None` is unambiguous), documented with pointers here;
transform's `restage_argument_named` carries the same error contract.
Rejected alternatives: `Result` on the indexed accessors too (forks the crate-wide
Option idiom where `arguments().get(i)` + `is_provided()` already discriminates);
panicking on unknown names (this family is the non-panicking companion shape by
design).

#### `display_tree()`: a free debug renderer; `NodeKind::as_str()` [§dd-dr:display-tree]

Status: DECIDED (user, API-review session).

A free public function `display_tree(node) -> String` renders a subtree one line per
node: box-drawing guides + `summary()` + **line/col** positions (internal per-source
`LineIndex`), printing a source name only when it changes from the previous line
(multi-source trees; the initial source is omitted). Deliberately a *free function*,
not a `NodeRef`/`NodeTree` method (user): lean surface, trivially dead-code-eliminated
when unused. The output format is human-oriented and explicitly not a stability
contract (`summary()`'s caveat restated); v1 ignores tree annotations. Companion
accessor **`NodeKind::as_str()`** → `"Chars"`/`"Group"`/`"Callable"`/`"Comment"`/
`"List"` (the visualizer's own need and an independent review wish): `as_str` is the
Rust idiom for a static variant name. Placement: the node read group beside
`summary()` — display, not content extraction (not `extract`); replaces the rejected
elaborate plain-text extraction (that gap belongs to the totext companion project).
Rejected names: `label()` (reads as user-provided/dynamic data), `kind_as_string()`
(stutters as `NodeKind::kind_as_string`; `_string` connotes allocation), `name()`
(sibling collision with `NodeRef::name()`, the callable's spelling).

#### `validate_tree`: the all-trees law as a `Result`, in `core::node` [§dd-dr:tree-validation]

Status: DECIDED (user, API-review session; realizes [§dd-dr:restage]'s validator
rider).

`pub fn validate_tree<L: Lang, A>(tree: &NodeTree<L, A>) -> Result<(),
TreeViolation>` (with `#[non_exhaustive] TreeViolation { node: Option<NodeId>,
kind, … }`) checks the **all-trees law** — what every finished tree must satisfy
regardless of origin: structural sanity (child ranges in-bounds, after-parent,
single-parent, all reachable), region tiling on resolved records (content ranges
within content parents, content-parent-inside-region), `TextContent` residency
(valid char-boundary range of the node's own source), regions resolved.
Deliberately NOT checked: byte partition, children-share-parent's-source, sibling
source order — the span-tiling law, which legitimate transform output (spliced,
reordered, synthesized nodes) breaks by design. It returns `Err`, never panics:
its persona is a framework validating rebuilt/spliced trees at runtime (FFI
included) — the panic policy's outer-layer case; the panicking
`check_tree_invariants` keeps its test-utility role for the span-tiling
law, now `pub(crate)` and re-implemented as a panic-assert wrapper over
`validate_tree`'s `Result` — ONE canonical check implementation, no duplicated
invariant logic, with the returned violation carrying enough detail that the
wrapper's panic message stays as informative as the asserts it replaced
([§dd-dr:public-visibility-sweep]). Its byte accounting is scoped per source via
the `Attached` role, so multi-source parse trees pass it; the two doc pages
cross-reference — all-trees law ⊂ span-tiling law.

**Home `core::node`, not `techy::transform`** (user): the function checks the
universal node-tree law and accepts any tree — transform output is merely its
commonest client; placement follows logical function, not audience. Name
`validate_tree`: the verb deliberately differs from the panicking `check_*`
family because the contract differs; the walkthrough wish-name
`check_transform_tree_invariants` under-claims (parse trees pass too) and is
superseded. A `validate_parse_tree` sibling (all-trees law + the span-tiling law's geometry)
was proposed and **withdrawn** together with the byte-reconstruction guarantee
([§dd-dr:recompose] amendment): the geometric half certifies only that
gap-filling reproduces source *bytes*, while the semantic half — that those bytes
match the tree's *content* — is parse provenance no runtime checker can verify;
a checker that cannot check what its users would believe it checks is a trap.
*Amendment (user, span-tiling design session).* The oracle's byte accounting holds for the parse trees of a
language that obeys span tiling; for a language declaring `OBEYS_SPAN_TILING = false` it
checks the all-trees law alone, and "the span-tiling law" replaces the earlier name
"parse-tree law" throughout ([§dd-dr:span-tiling]).

Revisit if: a runtime consumer genuinely needs the geometric span-tiling check —
additive as a sibling, with the semantic limitation stated on it.

## Tree transformation, annotations, and ext minting [§dd-dr:transform]

The API review's coherent redesign of the post-parse surface, ruled as one
piece — the entries below cross-depend — together with the recompose session's
machinery/payload rulings.

#### Per-tree node annotations: `NodeTree<L, A = ()>` [§dd-dr:node-annotations]

Status: DECIDED (user, API-review session).

Trees gain a second, defaulted generic parameter: the **annotation** type `A` — one
value per node, uniform across kinds, chosen by the *consumer* per processing stage.
`Lang` never sees `A` (the whole parse pipeline is `A`-blind; the parser emits
`A = ()`, so `ParseResult` spellings are unchanged). This is the framework-side
counterpart of the lang-side `NodeExt`: multi-stage pipelines type each stage's
derived data (`NodeTree<L, ()>` → `NodeTree<L, SemInfo>` → …) instead of maintaining
`HashMap<NodeId, T>` side tables that die at every transform boundary (the T5
framework walkthrough's central friction finding).
Storage: annotations live in a per-tree parallel `Vec<A>` — *not* in `NodeData` —
over an `Arc`-shared node core (`NodeTree = { core: Arc<TreeCore<L>>, annotations:
Vec<A> }`). Consequence: `annotate()` (same layout, new annotation type) allocates
only the new annotation vector — zero `NodeData` is cloned, the input tree is
untouched, and all same-layout stages share the core and its tree tag
([§dd-dr:tree-tags]), so their `NodeId`s are interchangeable (ids identify *layout*,
not stage); `NodeTree::clone()` becomes O(annotations). Bounds: `A: Clone + Debug +
Send + Sync`, deliberately **no `Default`** — every annotation value is supplied
explicitly ([§dd-dr:restage]'s single-pathway rule; the parser passes `()`
literally). Extract producers mint output annotations through a general per-part
callback ([§dd-dr:extract-annotations]). Accessors: `NodeRef::annotation()`,
`NodeTree::annotations()`, `annotate()` in storage order with the loud doc sentence
([§dd-dr:restage-ops]); the read types carry the defaulted `A` parameter
(`NodeRef<'t, L, A = ()>`, `NodeSlice`, `Descendants`, the extract helpers — every
pre-existing spelling keeps compiling). FFI note: a binding fixes one
concrete `A` for its pipeline (e.g. a PyObject-slot type) — dynamic typing inside a
single monomorphization; Rust frameworks use typed per-stage `A`. Per-kind
annotation typing is intentionally absent — a consumer uses an enum inside `A`
([§dd-dr:ext-minting]'s tier-2 argument applies).
Rejected alternatives: status-quo side tables (no transform survival, no library
support); routing through `Lang::NodeExts` (forces the preset-fork cliff, welds
pipeline data to the language definition, and allows one type forever instead of one
per stage); a wrapper `AnnotatedTree<L, A>` over `Arc<NodeTree>` (splits the
node-list currency, and the *builder* still needs a typed channel, so transform
outputs degrade to `(tree, Vec<A>)` pairs — the side-table problem with extra
steps); `Box<dyn Any>` per node (untyped, per-node allocation); storing `A` inline
in `NodeData<L, A>` (the original sketch — forfeits zero-copy re-annotation);
annotations on argument/slot records (declined — per-node only, kept simple).
Revisit if: adding the parameter later were the question — it isn't; the reason to
decide *now* is that the builder, restage, and extract surfaces are shaped around it.

#### Extract producers mint annotations: general callback + suffixed shorthands [§dd-dr:extract-annotations]

Status: DECIDED (user, API-review session; supersedes the earlier `A = ()` extract
clause of [§dd-dr:node-annotations]).

The four extract producers that build backing trees — `split_at_chars`,
`parse_keyval`, `split_embellishments`, `split_tack_on_fields` (the tree is built
eagerly inside the producer, so the annotation mint lives there; `into_tree`
stays a field move) — each ship three spellings, **the general form owning the
bare name** (user: the canonical path is the general one; shorthands carry the
suffixes):

- `split_at_chars(nodes, sep, f) -> SplitAtChars<L, B>` — general per-part
  `A → B` mint;
- `split_at_chars_drop_annotations(nodes, sep)` — the `B = ()` shorthand;
- `split_at_chars_keep_annotations(nodes, sep)` — `A → A` clone-through,
  bound-where-used `A: Clone + Default`.

Clone-through as the *default* was withdrawn on user counterexamples: measure-like
annotations (say, a recorded chars length) go silently stale across a split — an
op knowingly minting wrong values fails API hygiene even if post-fixable — and
output annotations can be *split-semantic* (entry/part discrimination), information
only the op holds at mint time. The callback is the consumer-side mirror of
`make_node_ext` (consumer-owned data ⇒ consumer callback, [§dd-dr:ext-minting]
symmetry), and it erases the `Clone`/`Default` bounds from the general path —
synthesized nodes are just another callback call. The producers are input-generic
over the annotation type.
Part context: one opaque accessor-based struct per op —
`SplitAtCharsPart`/`KeyValsPart` — with accessors admitted under the inclusion
test, *only what the
callback cannot recover itself*: the original node (`original() ->
Option<NodeRef>`, `None` exactly for the synthesized `List` wrappers/roots;
`copied_from` rejected — partials are cut, not copied), partial-piece info
(`is_partial()`/`partial_text()`), and `segment_index()`/`entry_index()`.
KeyVals keys are plain strings, not nodes — no key-side annotations arise.
The result struct is kept — it owns the backing tree and the segment view API; a
bare-`NodeTree` return would promote the root-List-of-segment-Lists shape into
frozen contract — and renamed **`Split` → `SplitAtChars<L, B = ()>`** (std
producer-fn precedent: `SplitWhitespace`, `CharIndices`; the name carries the
load-bearing semantics — only chars nodes are cut, groups protect their
interior). `KeyVals<L, B = ()>` keeps its concept name (three producers share
it). Boundary recorded: the callback is **annotation minting only** — vetoing or
modifying nodes is restage's job.
Rejected alternatives: clone-through as default (above); a callback parameter on
the bare name only, taxing the `()` case (resolved by the name flip instead);
`SplitAtChars::split()`-style operation methods (the free fn already performed
the split — the struct is the result, not the operation).
Revisit if: a fifth producer materializes trees — it adopts the same triple.

#### Always-on tree tags: `TreeTag` joins `NodeId` identity [§dd-dr:tree-tags]

Status: DECIDED (user, API-review session; supersedes the debug-only scheme of
[§dd-dr:node-id-provenance] — that entry's revisit condition fired).

Every tree layout mints a `TreeTag` (newtype over `u32`, from the existing wrapping
global counter) in **all builds**; `NodeId` becomes `{ index: u32, tree_tag:
TreeTag }` (8 bytes, `Copy`) and the tag **participates in `Eq`/`Ord`/`Hash`** — ids
minted by different trees are different values, so one map can key ids from several
trees, and an old tree's `NodeId` stored inside a new tree's annotation is
unambiguous (the enabling substrate of [§dd-dr:restage]'s origin-by-convention).
The old exclusion from `Eq`/`Hash` existed only because tags were debug-only; with
tags everywhere, debug and release agree again. `NodeTree::get()` genuinely
rejects foreign ids in release builds (the binding-pattern caveat disappears);
`node()` keeps its panicking own-tree contract. Layout-preserving copies (`clone`,
`materialize`, `annotate` stages) share the tag — their ids are interchangeable by
design. Terminology: **`tree_tag`** (the compound answers "tag of what?"); a newtype
so signatures cannot confuse tags with indices. Wrap policy: `u32` wraps after 2^32
layouts per process — accepted and documented, because the tag is a **misuse
detector, never an addressing mechanism** (resolution always goes through an
explicit tree; a collision only matters when a stale-id bug already exists). Tags
are process-local — never wire/persistence material. Bare `Range<u32>` values
(`ChildRegion` ranges, `nodes_in`) remain untagged, as before.
Rejected alternatives: keeping tags debug-only (release-build transforms multiply
trees and silently resolve foreign ids — the exact hazard); `u64` tags for a hard
uniqueness guarantee (16-byte ids for a guarantee nothing may rely on anyway — user
ruled `u32`); `tree_identifier` as the term (overpromises addressability).
Revisit if: a use case wants ids as *global* handles without the tree in hand —
that needs a registry design, not a wider tag.

#### Ext minting: population is initialization — `make_node_ext` replaces `finalize_node` [§dd-dr:ext-minting]

Status: DECIDED (user, API-review session; supersedes [§dd-dr:finalize-node] and
the two-tier ext half of [§dd-dr:closed-node-kind]).

**Principle: an ext is minted exactly once, at creation, by the party with the
knowledge — no "default-initialized, populated later" state exists anywhere in the
ext system.** The pieces, each argued in the session:

- **Tier-2 per-kind node exts are removed** (`CharsNodeExt`…`ListNodeExt` deleted;
  `NodeKind` becomes purely structural — no ext fields inside
  `Chars`/`GroupData`/`CallableData`/`Comment`/`List`, finally matching
  [§dd-dr:closed-node-kind]'s own "orthogonal to structural identity" claim). A lang
  wanting per-kind data uses an enum inside tier-1 `NodeExt`; coherence is enforced
  at the single minting point by the one author who owns both sides. Accepted costs:
  a kind/ext mismatch becomes representable (mitigated by the single minting point);
  enum exts cost discriminant + largest variant on every node (mitigated by the
  keep-exts-word-sized guidance); typed per-kind accessors disappear (post-parse
  consumers use annotations, [§dd-dr:node-annotations]). The in-between
  `NodeDataExt { uniform, per_kind: enum }` bundle is dominated: it pays removal's
  coherence cost *plus* the six-type ceremony.
- **`Lang::make_node_ext(kind: &NodeKind, span, state, children: StagedChildren)
  -> NodeExt`** (since revised to return `Result<_, NodeBuildError>` — a refused
  mint is `ExtMintFailed`, an error at the builder level because the hook also
  runs for consumer-built trees; [§dd-dr:hook-fallibility]) replaces
  `finalize_node` — value-return, `kind` by shared reference
  (the hook cannot change the kind; both `&mut kind` and consume-and-return were
  rejected for exactly that reach). The **idempotence contract is deleted**: the hook
  runs once per node, at parse staging, never on restaged copies (their exts travel
  as frozen parse facts, cloned verbatim). No parent parameter — impossible by
  construction (staging is bottom-up; the parent doesn't exist yet); downward
  context is `StateExt`'s job. **`StagedChildren`** replaces the
  `(children: &[BuildId], staged: &StagedNodes)` pair: a **subtree-deep,
  descent-only** view — child views resolve *their* children recursively (required:
  argument content sits at grandchild depth, e.g. computing `{domain, key}` from
  `\ref{fig:abc}`) but expose no siblings/ancestors/unrelated staged nodes.
- **`NodeExt` loses its `Default` bound** (`Clone + Debug + Send + Sync`) — a
  tier-1 ext value can only be minted by the hook or cloned from a node; there is no
  third path, not even an explicit `Default::default()`. Forced consequence, kept as
  a feature: `make_node_ext` is a **required** `Lang` method (techy cannot conjure a
  default body without `Default`); `TrivialLang`'s blanket impl supplies the `()`
  version, `Latexlike` writes the one-liner, and a lang that declares a real ext
  type *must* say how it is initialized.
- **`NodeTreeBuilder` is hook-free and mode-free**, with exactly one staging method:
  `add(kind, span, parsing_state, children, ext, annotation)` — it demands ready
  values. (Rejected shapes: the `add()`/`add_with_ext()` pair with a `Default`-fill
  path — callers of the builder are no more literate about a language's exts than
  parsers were; a `for_parsing()` hook-firing constructor mode — the
  misuse-by-accident vector.)
- **Parse staging goes exclusively through `ParseContext::stage_node(kind, span,
  state, children)`** — the ONE automatic `make_node_ext` site — and
  **`ParserSession::builder` becomes `pub(crate)`** (persona sweep found no
  legitimate external mutable need; a public read view stays for node-based stop
  predicates). This *strengthens* the old "no node escapes, no parser cooperation"
  property: the choke point moves from inside the builder to the only staging door
  parsers have — a third-party construct parser physically cannot stage an
  unpopulated node.
- **Transform-side minting is the explicit two-line recipe** (call
  `L::make_node_ext`, then `builder.add(…)`) — deliberately no wrapper helper: the
  explicit spelling is the finer control (inspect/adjust the minted ext between the
  two lines) and cannot be reached by someone who doesn't know what it does. WHO/WHEN
  in one sentence: *`make_node_ext` runs inside `cx.stage_node()` during parsing,
  and wherever a transform author writes the call explicitly; nowhere else, ever.*
  Knock-on: `split_at_chars` boundary partials are now properly minted in-crate via
  the hook (the "partials carry default ext" approximation disappears).
- **`ArgumentExt` is kept** (user — the body-marking story proved slot exts
  load-bearing late; arguments get the same open door, e.g. a future
  `BodySlotExt`-analog marking trait). Minting: the **`ArgumentParser` output
  carries the ext** (the parser is the knowledge-holder; the record constructor
  demands the value); custom parsers mint their own; the **standard parsers are
  defined only `where ArgumentExt<L>: Default`** — a conditional bound-where-used
  (the `ClosedVocabulary` pattern): a std parser's knowledge about your ext *is*
  "nothing", and its bound says so; the bundle itself carries no `Default`.
  Absent arguments store no ext (`ParsedArgument.ext: Option<ArgumentExt<L>>`;
  the bundle constructor is `absent(spec)`).
- **`SlotExt` is demanded at `ParsedSlot` construction**; generic preset machinery
  mints via `BodySlotExt::make_body()` ([§dd-dr:slot-roles]); custom
  `EnvironmentBehavior`s pass their values. No Lang hook, no `Default`.
- Ext bundle final shape: `NodeExtTypes = { NodeExt, ArgumentExt, SlotExt }`.
  `Lang`'s "all methods have working defaults" doc claim gains its one exception.

Rejected alternatives (beyond the inline ones): a Lang-global `make_argument_ext`
consulted by std parsers (spec-local knowledge forced through Lang-global dispatch —
knowledge and hook in different places); moving the node hook into `NodeExtTypes`
(bundle would need an `L` parameter — reshape for no gain).
Revisit if: a `Lang` needs per-node work at *transform* time — that is what
annotations are for, not a reason to resurrect re-running hooks.

#### `techy::transform`: the streaming restage driver [§dd-dr:restage]

Status: DECIDED (user, API-review session — direction, shape, and contracts; the
exact op surface is [§dd-dr:restage-ops]).

Tree→tree transformation is a **streaming restage**: an in-crate top-level module
`techy::transform` whose driver walks the frozen input tree with a user callback
invoked **top-down** (decide before descending — a buried `\includegraphics` is seen
before its subtree is committed) while staging **bottom-up** into a
`NodeTreeBuilder<L, B>` (children before parents; the driver mediates the two
orders). Vocabulary is `restage_*` throughout — "copy" is banned (it misreads as
bulk-subtree-copy; the earlier public-`add_subtree` framing is superseded). Layers:

- **Level 0 primitive** (in `core::node`): single-node copy with a per-child mapping
  ("old child → the new `BuildId`s that replaced it"), translating the callable's
  argument/slot region records — the generalization of the crate-internal copy.rs
  arithmetic (two-phase records, `InChildrenOf` designations, offset re-basing,
  region extents recomputed under dropped/replaced/multiplied children). Bulk
  subtree copy is the degenerate recursion over it, not the primitive.
- **The callback contract**: per node it returns
  `Restage<B> { Descend(B), Emit(Vec<BuildId>) }`. `Descend(b)` = the driver
  restages this node over its children's results with annotation `b`, and **the
  visitor continues through every child subtree** — the safety invariant: the only
  way a child subtree goes unvisited is an explicit `Emit` for its ancestor (no
  shallow-keep exists to reach by accident). `Emit(nodes)` = the callback staged the
  replacement itself (empty = drop); no automatic descent. The name `Descend`
  states the always-descends invariant in itself; `Continue` said too little,
  `Keep`/`Retain` actively suggested the shallow-keep misreading, `Auto` was vague
  (all four superseded).
- **Annotations, single pathway** (user's redesign, replacing a run-level mapper
  closure): *every* restaged node's annotation passes through the visitor — as
  `Continue(b)` or as an explicit argument to the staging ops the callback invokes.
  Mandatory by construction: `A_old` and `A_new` are different types, so "keep the
  annotation" is not even expressible — good by design; the origin-id convention is
  the one-liner `Descend(Ann { origin: node.id(), … })`.
- **Region-aware context ops** (the crate owns *all* region arithmetic):
  `restage_subtree(node)` (the full visitor over that subtree, its root included);
  `restage_children(node)`; `restage_argument(node, index_or_name)` /
  `restage_slot(node, i)` → **restaged-region bundles** (new ids + the record's
  spec/name/presence/content-designation, in bundle-relative staging coordinates);
  `restage_invocation(node, arguments, slots, annotation)` — restages the invocation
  data over bundles **in the order given**, retiling the records (argument swap
  `\a{1}{2}` → `\a{2}{1}` = two `restage_argument` calls + one reordered
  `restage_invocation`; swapped sibling spans out of source order are legal in
  transform trees). Raw `builder()` access underneath everything: the canned ops are
  conveniences, not the power boundary — arbitrary programmatic staging (and the
  explicit `make_node_ext` recipe for new nodes) is always available; merges are
  parent-level callback takeover. A `Descend`-restaged parent's `InChildrenOf`
  content ranges are *translated* by the driver through its own replacements —
  without this, an interior drop inside a `{…}` wrapper breaks the enclosing
  record and kills the one-line strip pass.
- **Read frozen / write staged**: callbacks inspect the *frozen input* — the full
  read API and the `techy::extract` tools — and produce staged output; the staged
  side is write-only. Verified no meaningful staged-side read need exists: decisions
  precede restaging (top-down); whatever a callback stages it just made (facts carry
  in closure state or annotations); full read semantics are impossible pre-`finish`
  anyway (unresolved regions, no layout, no `NodeRef`). Frameworks that must inspect
  transform output finish the tree and run another pass — multi-stage is
  deliberately cheap ([§dd-dr:node-annotations]). Accepted boundary: a `Continue`
  parent never sees its children's results (take over via `Emit` +
  `restage_children`, or use two passes).
- **Origin tracking is a convention, not scaffolding**: the framework puts an old
  `NodeId` in its own annotation type ([§dd-dr:tree-tags] makes that safe); techy
  contributes the old-`NodeRef`-in-hand callback and a documented recipe (incl. the
  O(n) old→new inversion walk). The auto-provenance trait
  (`WithTransformedTreeNodeProvenance`-style, with wrapper type and tracking entry
  point) was **rejected**: merges and subtree replacements make "the" original id a
  fiction whose semantics no mechanism can choose for the framework. Per-node
  `Arc<NodeTree>` references were rejected separately (type-chaining across stages;
  lifetime-pinning every pipeline stage alive). Vocabulary: **"original node"** —
  "provenance"/"origin" alone belong to the source model.
- Riders: the **transform-tier validator** (structure + region tiling + `TextContent`
  residency, *minus* the span-tiling law's byte accounting) is `validate_tree` in
  **`core::node`** — placement follows what it checks, not its commonest client
  ([§dd-dr:tree-validation]); `NodeRef::tree()` is public.

Rejected alternatives: a companion crate (version skew during co-evolution;
`techy-totext` is the external-consumer proof instead); a fixed atomic-op vocabulary
(add/drop/splice/rebuild) as the ceiling (user: not powerful enough — the driver's
fixed job is only order mediation and region-preserving reassembly); a `finish()`
BuildId→NodeId map (helps only callers who separately tracked BuildIds; the
annotation channel is strictly more direct).
Revisit if: a real FLM pass finds the bundle shapes insufficient — the layering
(primitive / driver / raw builder) is the stable part; the op signatures are
[§dd-dr:restage-ops].

#### Restage op surface: visitor trait, generic errors, constructible bundles [§dd-dr:restage-ops]

Status: DECIDED (user, API-review session; completes [§dd-dr:restage]'s deferred
detailing).

The exact types of the restage driver:

- **Callback = trait + closure blanket.** `RestageVisitor<L, A, B> { type Error;
  fn restage(&mut self, node: NodeRef<'_, L, A>, cx: &mut RestageContext<'_, L, A, B>)
  -> Result<Restage<B>, Self::Error> }`, with a blanket impl for `FnMut` closures.
  The trait exists because the region ops re-enter the visitor from *inside* a
  visitor call (`cx.restage_argument(node, i, self)`) — a closure cannot pass
  itself; the blanket keeps `restage(&tree, &mut |node, cx| …)` for non-reentrant
  passes. **No `Send`/`Sync` bounds anywhere on visitors or `annotate()`
  callbacks** — the driver runs them synchronously on the calling thread; the bound
  would be a demand on callers buying techy nothing, and it would wall off
  GIL-bound FFI callbacks (the primary binding consumer). Parallel variants, if ever
  wanted, are new entry points with their own bounds (the `&mut` visitor contract
  is inherently serial).
- **Errors generic, not boxed**: `restage(tree, visitor) -> Result<NodeTree<L, B>,
  RestageError<V::Error>>` with `RestageError<E> { Build(NodeBuildError),
  ContentParentDropped { … }, Visitor(E), … }` — plus op-misuse variants (the
  unknown-name `Err` and the panic policy force them) and a `RootNotSingular`
  entry-point variant. The framework's own error type rides
  through typed; `Clone where E: Clone` keeps the uniform-Clone principle
  conditionally. Fixed `Arc<dyn Error>` boxing rejected (loses typing for nothing);
  infallible visitors rejected (panic policy).
- **Bundles are opaque but constructible**: `RestagedArgument::provided(spec,
  nodes, content, ext)` / `::absent(spec)`; `RestagedSlot::new(name, role, nodes,
  content, ext)`. The constructor IS the general take-both form — staged nodes plus
  the `ContentNodes` designation, the same field vocabulary the
  `ParsedArgument`/`ParsedSlot` records carry. Ops: `restage_subtree`,
  `restage_children`, `restage_argument[_named]` (unknown name = `Err` — the
  named-accessor doctrine transfers), `restage_slot`,
  `restage_invocation(node, arguments, slots, annotation)`, raw `builder()`.
- **Region-edit policy: no silent repair.** A drop that empties a region restages
  as provided-with-empty-region (absent ≠ empty is parser semantics; true absence
  is the explicit `absent(spec)`); a dropped `InChildrenOf` content parent is
  `RestageError::ContentParentDropped`, whose message points at the takeover route
  — the same law the builder enforces at construction time, with better diagnosis
  (and `Emit(vec![a, b])` replacements make any auto-re-anchor ill-defined even in
  principle). Auto-flip-to-absent and re-anchoring rejected (silently change what
  the record *means*); forbidding region-member drops rejected (kills the one-line
  strip pass).
- **Content-replacement helpers**: `restage_argument_with_content(node, i,
  content)` + `restage_slot_with_content` — wrapper syntax and noise restaged
  verbatim *by contract*, content swapped, designation re-anchored; they take an
  explicit `annotation: B` argument (the single-pathway rule needs a channel for
  the verbatim-restaged wrapper/noise copies). Changing noise
  uses the visitor op (noise flows through the visitor) or the hand-built bundle;
  a both-taking helper was rejected as a second path duplicating the constructor
  modulo a one-line spec/ext transcription. The working name
  `stage_argument_like` is superseded.
- **Level-0 primitive**: `NodeTreeBuilder::restage_node(node, replacements:
  &[Vec<BuildId>], content_parents: impl Fn(NodeId) -> Option<BuildId>,
  annotation)` in `core::node` — positional per-child replacement slices (length
  checked against `child_count()`); **cross-tree by contract**: it accepts a
  `NodeRef` from *any* tree and a same-tree debug assertion may never be added —
  this is the sanctioned splice door.
- **Builder `add` stays positional** (six parameters, order: identity, provenance,
  context, structure, lang-data, consumer-data; nearly every mis-ordering is a type
  error). A params struct is additive later if real transform code demonstrates
  confusion; the reverse is breaking, and struct-update sugar would reintroduce
  exactly the partial-initialization reading [§dd-dr:ext-minting] killed.
- **Annotation accessors**: `NodeRef::annotation() -> &A`,
  `NodeTree::annotations() -> &[A]` (storage-order slice — also the FFI
  bulk-export shape); no setter (trees frozen; re-annotate via `annotate`).
  `annotate()` runs its callback in **storage order**, documented loudly — a
  stateful closure must not assume document order (a document-order variant buys
  no structure for an order-walk cost; consumers wanting it read off
  `descendants()` first).
- **Restage descends into `Attached` and `Hidden` slot children uniformly** — the
  driver is structural, never role-conditional; protective verbatim treatment of
  attached content is one explicit visitor arm ([§dd-dr:slot-roles] amendment).

Rejected alternatives are recorded inline per point.
Revisit if: the closure blanket's `E` inference proves awkward in practice —
the recorded fallback is the fixed-error shape; a flag-level change, not a
re-session. (The recompose surface mirrors this entry deliberately —
[§dd-dr:recompose-machinery].)

#### `techy::recompose`: recomposition as a downward-state fold [§dd-dr:recompose]

Status: DECIDED (user, API-review session — direction and scope; recompose session
— details. The detailed records are [§dd-dr:invocation-syntax] (the payload axis),
[§dd-dr:recompose-machinery] (the fold machinery), and [§dd-dr:visit-engine] (the
shared traversal engine)).

Recomposition (tree → output text) is a generic fold in a top-level
`techy::recompose` module: the consumer supplies per-node logic; a typed
**recomposition state threads downward** into children; the framework is agnostic
about *how* nodes recompose.

**Per-node recomposition doctrine (user):** spans give provenance, not output
location — recomposition reconstructs each node from its **own recorded data** (a
chars node contributes its content; a callable/environment its scaffolding from the
recorded invocation-syntax payload) and never performs **inter-node** span
arithmetic ("apparent gaps" between siblings resurrect deleted content on any
transformed tree) nor reads source text beyond a node's own recorded content.
"Span-verbatim" is deliberately *not* a shipped strategy: the recomposer never
resolves span content, the node's own span included — no span fast path (a tree
carries no reliable freshness signal to gate one); `span_content()` stays a public
consumer affordance the recomposer simply never uses. Consequently there is **no
framework-facing byte-reconstruction guarantee**: the span-tiling law's byte accounting is
an in-crate acceptance-suite *oracle* — reassembling the input from a fresh parse
proves lossless parsing — and parse-output span semantics stay documented for the
analyze-only/span-patch tooling architecture with the provenance warning
(structural edits void inter-node span arithmetic). An interim
`validate_parse_tree` proposal was withdrawn with the guarantee
([§dd-dr:tree-validation]).

The node-data strategy — reconstruction from recorded facts, pylatexenc's
`latexnodes._latex_recomposer` precedent — is the ONE preset **`SourceRecomposer`**:
the core provides the walk, the **latexlike preset provides the trigger
spellings** via the invocation-syntax payload; on a `materialize()`d tree this
touches no `Source` at all — fully source-independent byte-faithful
reconstruction. latex2text is "a recomposition
whose per-node logic emits text, not LaTeX": the **mechanism lives here, the content
(handler databases, unicode tables, layout) in techy-totext** — consistent with
rejecting elaborate in-techy plain-text extraction. There is **no sink concept** in
the machinery (value fold; streaming = a recomposer-held writer with
`Piece = ()`). Strategies key on `SlotRole` ([§dd-dr:slot-roles]): source
re-emission skips `Attached` by definition (the invocation text *is* the
recomposition; descending is the explicit expansion option); `Hidden` never
participates. The read-only structural walker (`enter(node, depth) -> VisitFlow
{ Descend, SkipChildren, Stop }` + `exit(node)`) is the walk vocabulary, designed
once and shared ([§dd-dr:visit-engine]); a `Descendants::with_depth()` iterator
adapter was rejected because flat iteration loses structure — `descendants()`
itself stays, legitimate for structure-free queries.

Rejected alternatives: a `Hidden`-slot scaffolding store
(`"begin_tokens"`/`"end_tokens"` noise slots) — rejected together with the
`CallSyntax` role: trigger spelling is recorded *payload*
(`Lang::InvocationSyntax`), never slots.

#### Invocation syntax is recorded payload: `Lang::InvocationSyntax` [§dd-dr:invocation-syntax]

Status: DECIDED (user, API-review recompose session; supersedes the
reconstruct-don't-record half of [§dd-dr:environment-scaffolding] and the core
`post_space` storage of [§dd-dr:span-invariants] invariant 3).

**Accuracy doctrine (user):** the *preset* (the `Lang`), not core, owns
recomposition accuracy — byte-exact vs up-to-noise vs loose is the preset's choice,
implemented by what invocation-syntax information it records in node payload, in
logical canonical form. Recomposition accuracy is thereby coupled to
parse-recording accuracy: recomposition reads **raw node payload only** — no hidden
slots, no side channels (extending [§dd-dr:recompose]'s per-node doctrine). The
in-crate oracle acceptance suite (`techy/tests/recompose_oracle.rs`,
public-API-only: reemit == input over strict + tolerant + multi-source matrices)
certifies payload completeness with no span crutch. One recorded-less-than-consumed
recovery — the malformed environment terminator, whose `\end` is consumed alone and
recorded nowhere — is excluded from the equality matrix and pinned by a dedicated
elision test, per this entry's own accuracy coupling.

The mechanism: a Lang-associated invocation-syntax type,
**`Lang::InvocationSyntax`**, stored as a `CallableData` field **replacing the core
`post_space` field** (and no `escape_char` is ever added to core); minted by the
invocation parser; a parse-level-syntax channel, distinct from the lang's node ext
(preset-logic info). Two-trait split:

- the **required core data-bound trait `InvocationSyntax<L>`** on
  `Lang::InvocationSyntax` (L-parameterized — the `ParseDriver<L>` precedent):
  `Clone + Debug + Send + Sync + 'static` plus
  `materialized(&self, &Source) -> Self`
  (the `()` impl is trivial; the `&Source` parameter — origin via
  `L::SourceOrigin` — replaces a bare content `&str`, which was a multi-source
  wrong-string hazard; `NodeTree::materialize` resolves each node against its own
  span's source, and `TextContent::resolve`/`materialized` and the spelling
  writers thread `&Source` the same way);
- the **opt-in constructor trait `FromInvocation`** with `from_invocation`,
  consulted by the std staging sites (`StdInvocationParser` + the specials site)
  and implemented for `()` by techy.

**The latexlike payload** is the enum **`InvocationSyntaxData<Env =
StdEnvironmentSyntax<L>>`** — it IS the data holder, the
`CallableData`/`NodeData` family. (The trait/enum names were swapped relative to
an earlier ruling; [§dd-dr:superseded-names] pins the old role assignments
against returning.) The variants:

- `Macro { escape_char, post_space }`.
- `Environment(Env)` — the std record holds per side `{ escape_char, command_word,
  post_space, name_group_rule: Arc<GroupRule<L>> }`, the name group recorded as the
  **rule `Arc` cloned from the matched token** (user counterproposal, verified
  sound: `TokenKind::GroupOpen` carries the matched rule Arc; the
  rule's open/close `String`s are the exact matched bytes; the name
  group can never exist in delimiter-diverged form — a malformed begin takes the
  chars-recovery path — so rule == bytes always; the Arc
  is source-independent, hence exempt from `materialized`; and it records the group
  *class*, which byte-recording would lose). End-side facts are reported back by
  the body parser (the terminator consumer).
- `Specials` — a **unit variant**; ruled (user, reversing an earlier
  literal_form lean): `name` is the actual invocation spelling *always*, matching
  the macro rule (`\foo` vs `\fooooo`, both spec-resolved by prefix, both record
  the name as written) — no second field, no two-field rename hazard.
  Paragraph-break `Specials` nodes record the actual whitespace run as `name` (a
  canonical-`"\n\n"` contract is superseded); identification is by **spec
  identity** — the definite, canonical `ParagraphBreakSpec` object (the latexlike
  driver must not mint an anonymous per-break spec; that rule is load-bearing,
  not hygiene).

**Env consolidation** (user): everything anchors on the Env type — a defaulted
`LLL`-method tier was dropped (user worry upheld: too many customization entry
points on `Lang`); the single customization entry is the Lang's choice of
`InvocationSyntax` type. **`EnvironmentSyntax<L>`**, implemented by Env
types, is the **pure record contract**:
`from_parsed(begin: EnvironmentBeginSyntaxData<L>, terminator:
Option<EnvironmentTerminatorSyntaxData<L>>) -> Self` plus the **spelling writer
pair `write_begin`/`write_end`** — the Env type owns its own re-emission (the
accuracy doctrine made literal; kept as a *pair* because `Concat` head/tail and
the span-tiling law's prefix/suffix pins need the two sides separately — a fused
`recompose_environment` writer is a rejected shape; `write_end` on an empty end
side returns `""`, since reemitting nothing reproduces the recovered input). The
composition (`EnvironmentInvocationParser`, generic over `LLL`) owns ALL
scanning — `read_rigid_name_group` called directly; resolution + argument
parsing composition-owned — and constructs the payload exactly once, at staging;
the body parser is the terminator consumer, its facts flowing back through
core's two-sided facts channel (`EnvironmentBeginSyntaxData<L>` /
`EnvironmentTerminatorSyntaxData<L>`; `StdEnvironmentSideSyntax` is the std
record's own component type, off the trait surface). An accumulator trait shape
(`parse_begin -> (NameInfo, Self)` + `parse_end(&mut self)`) was rejected: the
body parser consumes the terminator, so `parse_end` never scanned anything, and
mutate-in-place shape-locked custom Env types into the standard flow's call
sequence. **Tolerance is a parser concern**: a family member wanting looser
begin/end syntax swaps the invocation/body parser through the behavior door —
the record records what its parser consumed; it does not encode a scanning
policy. A non-command begin trigger is a documented-contract implementation
error (no degenerate-spelling fallback arms): std environments are
command-initiated, and a custom trigger shape needs its own composition + Env
type. Verbatim caveat (verified): the verbatim terminator is one literal
`GroupClose` token (rules replaced; close = the full `\end{name}` string) —
end-scanning delegation cannot apply to raw bodies. The facts channel still
carries std `Scanned` end facts across it: `VerbatimBodyParser` is *given* the
terminator's pieces and reports them back itself ([§dd-dr:verbatim-family]), so
`StdEnvironmentSyntax` transcribes one arm and synthesizes nothing. Its other
arm — a bare `Literal` terminator, which a custom `make_body_parser` may still
report — has no command-plus-name-group spelling to transcribe and no field to
keep the literal in: it records a placeholder command word that re-emits
visibly wrong, rather than a plausible-looking guess (a record whose end side
cannot be accurate must not look accurate).

**The fifth role trait** joins the [§dd-dr:latexlike-generalization] roster:
`LatexlikeInvocationSyntax`, on the syntax type — `type Env:
EnvironmentSyntax<L>`, form constructors
(`macro_form`/`environment_form`/`specials_form`), accessors
(`macro_syntax`/`environment_syntax`/`is_specials`).

Honest costs accepted (user): Lang-agnostic tooling sees only name + span of
foreign callables (by design); variant/`callable_type` coherence is unenforced (a
recomposer error variant reports it); +1 `Lang` associated type.

Rejected alternatives: a **`CallSyntax` fourth `SlotRole`** storing trigger
spelling in slots — rejected outright (user): it duplicates information the node
already owns (macro/environment names re-appear in scaffolding bytes); it cannot be
a preset-agnostic recomposition mechanism (core cannot reconstruct preset-owned
constructs); and it makes transforms hazardous (a rename requires synchronized
spelling updates). With it fall the brief's `Hidden`-slot scaffolding storage
(`"begin_tokens"`/`"end_tokens"` span-backed `Chars` nodes), the `escape_char` core
`CallableData` field, the `Hidden`-emission carve-out, and the order-free-tiling
builder change: `SlotRole` stays the ruled three-variant enum, `Hidden` stays
reserved, and the builder tiling check stays as-is ([§dd-dr:slot-roles]).

The span-tiling law's payload pins live in the preset checker
`check_latexlike_tree_invariants` (latexlike-side; one call = the core checker +
the pins; core's callable arm is payload-blind — [§dd-dr:span-invariants]), which
also pins foreign family members (downcast to
`InvocationSyntaxData<StdEnvironmentSyntax<LLL>>`, not just the default-Env
enum). The body parser's pass-through-delta check is an implementation-error
path, not an assert ([§dd-dr:panic-policy]).

Revisit if: a construct's invocation syntax cannot be expressed as per-node
recorded payload — that is a new axis to design, not a reason to resurrect
slot-side scaffolding storage.

#### Recompose machinery: the meaning-free `Piece` fold with instruction lowering [§dd-dr:recompose-machinery]

Status: DECIDED (user, API-review recompose session; completes [§dd-dr:recompose]'s
deferred detailing).

The recompose machinery is **meaning-free** (user decoupling directive): it
composes generic *pieces* over the visit; source recomposition is ONE `Recomposer`
implementation (latexlike's), never a machinery default. Architecture = **direct
value fold** — the transform-to-chars-then-concatenate alternative is dead as a
mechanism, surviving only as the documented restage→recompose pipeline pattern.
There is **no sink concept** in the machinery: streaming is a recomposer-held
writer with `Piece = ()`.

- **Reading contract (user-simplified; replaces the brief's permits/forbids
  list):** *permitted* — reading any field of the node's own payload, including
  resolving span-*backed payload* (`TextContent::Spanned`), an internal detail of
  how a content field is stored (parse trees recompose zero-copy). *Forbidden* —
  the recomposer resolving any span content, **including the node's own span**,
  against the source. No span fast path exists in the shipped recomposer: a tree
  carries no reliable "still fresh from parse" signal, so a span shortcut could
  never be safely gated (user rationale). `span_content()` remains a public
  *consumer* affordance (level-1 lookup); the recomposer simply never uses it.
  Byte-exactness rests entirely on payload completeness
  ([§dd-dr:invocation-syntax]).
- **`Recomposer` trait**: `State`/`Piece`/`Error` associated types +
  `recompose_node`; **no `Send`/`Sync` bounds** (the [§dd-dr:restage-ops]
  argument transfers). State is consumer-defined and threaded downward by
  explicit descent; run-spanning state lives in the recomposer's `&mut self`
  fields (the three-channel discipline, [§dd-dr:visit-engine]). Entry:
  `recompose::recompose(tree, state, recomposer)` — a mandatory root-state
  argument (the argument-threaded `S` made explicit), recomposer last per the
  restage visitor-last convention.
- **Instruction enum** `Recompose { Emit(Piece), Concat(ConcatPieces) }`.
  `ConcatPieces` is the joiner payload — `head + child₁ + sep + … + childₙ +
  tail` (user amendment) — plus optional derived state and scope, built by
  chainable constructors (`children()`/`wrap(head, tail)`/`join(sep)`).
- **`ComposePiece`**: the piece monoid (`empty`/`append`; techy impls `String`
  and `()`); it carries a `Clone` requirement — `sep` is duplicated per gap
  (option (a), ruled).
- **The wrapping contract**: instructions lower against the **outermost**
  recomposer, so layering is free; wrap-intended recomposers return instructions
  and never descend explicitly (contrast restage, where a takeover visitor stages
  explicitly — [§dd-dr:restage-ops]).
- **Default `Concat` scope skips `Attached` AND `Hidden`** (plain children +
  `Content` regions); widening is the explicit opt-in
  (`include_attached()`/`include_hidden()`). This operationalizes the ruled role
  semantics — reads are role-blind, recompose is the one role-sensitive site
  ([§dd-dr:slot-roles]); `SourceRecomposer` needs no scope call at all; the walk
  stays role-blind ([§dd-dr:visit-engine]) — the read/compose asymmetry is
  frozen as ruled.
- **`RecomposeContext`** (spelled-out Context per the `ParseContext` convention):
  self-passing helper methods, surface kept minimal; the argument/slot roster
  mirrors the restage family — `recompose_argument` / `_argument_content` /
  `_named` variants (`Result`, per the `_named` convention) /
  `recompose_slot_content` + `_slot_content_named` / `recompose_body`.
  **`RecomposeError`** mirrors `RestageError` *for what applies*: `Recomposer(E)`
  named for the failing trait (the `RestageVisitor` → `Visitor` pattern), the
  op-misuse group verbatim, plus two variants the recompose ops force with no
  restage source (`UnknownSlotName`, `NoBodySlot`);
  `Build`/`ContentParentDropped`/`ArgumentAbsent`/`RootNotSingular` are omitted
  as analogue-free — the fold stages nothing, re-anchors nothing, has no
  `_with_content` helpers (an absent argument composes the empty piece), and
  always yields one piece.
- **`core_source_instruction`**: the instruction-returning free helper for the
  core-complete kinds (`B: ComposePiece + From<&str>`); it declines callables —
  their payload is Lang-owned ([§dd-dr:invocation-syntax]).
- **`SourceRecomposer<LLL>`** (public; constructor `latexlike::
  source_recomposer()`): the preset source re-emission — `State = ()`,
  `Piece = String`, instruction-only, plus a coherence error variant
  (variant/`callable_type` mismatch). Its specials arm emits name-as-head over
  the children rather than a bare name — specials specs can declare arguments
  whose regions must follow.
- **Targeted replacement** = the wrapper pattern (a wrapping recomposer overrides
  the targeted nodes; no span fast path) + the documented restage→recompose
  pipeline.
- Naming: **`Piece`** over `Bit` (binary connotation; `Fragment` recorded
  considered — DocumentFragment precedent; `Part` considered; `Output` rejected —
  collides with `ConstructParser::Output`); the **`recompose::recompose` stutter
  is accepted** (module = a domain whose sole operation shares its name);
  `recompose_tree` rejected on one-canonical-path.

Rejected alternatives: a machinery-level sink type/parameter (the fold returns
values; streaming is a recomposer concern); `ConcatSpec` ("Spec" is author-side
vocabulary) and the interim `ConcatParts` — the payload is `ConcatPieces`.

Revisit if: a real consumer's piece type cannot satisfy `Clone` — the per-gap
`sep` duplication is the one place the monoid demands it.

#### `techy::visit`: one shared traversal engine for walk and recompose [§dd-dr:visit-engine]

Status: DECIDED (user, API-review recompose session; realizes the read-only walker
routed by [§dd-dr:recompose]).

Contract points: `walk` takes the start
`NodeRef` — the earlier veto was the *method* placement, and the walker's origin
demands subtree walks (whole tree = `walk(tree.root(), v)`); the walk is
infallible (`enter` returns bare `VisitFlow`; error-carrying
visitors `Stop` with the error in their own state); `exit` fires after a node's
children for `Descend` and immediately for `SkipChildren`, receives the same
`(node, cx)` pair as `enter`, and `Stop` aborts with no further calls;
`VisitContext` carries exactly `depth()` + `tree()`. The one descent kernel is
the crate-internal `scoped_children(node, include_attached, include_hidden)` —
the walk calls it role-blind, the recompose driver with the `ConcatPieces`
scope flags.

The read-only structural walker and the recompose driver share **one traversal
engine**, in direction **walker-on-recompose-core** (user challenge upheld: the
brief's separation argument refuted only the reverse — recompose cannot be built on
a plain read walker, because the fold composes values and threads state,
capabilities enter/exit walking deliberately lacks). Home: **`techy::visit`**, a
top-level module — the user vetoed `core::node`; strata consequence: entries are
free functions, and there is no `NodeRef::walk` (core cannot name the techy-level
engine). `validate_tree` is unaffected and stays `core::node`
([§dd-dr:tree-validation]) — placement follows logical function in both rulings.

- **`NodeVisitor`**: `enter` returning `VisitFlow { Descend, SkipChildren, Stop }`
  + defaulted `exit`; blanket impl for enter-only closures; single entry
  **`visit::walk`** (`walk_tree` rejected on one-canonical-path).
- **`VisitContext`** carries **engine bookkeeping only** — depth, tree access,
  cross-tree guards — and NO user state. The **three-channel discipline** stands
  (user): run-spanning state = the visitor's/recomposer's `&mut self` fields; fold
  accumulation = driver locals + the call stack; downward context = the
  argument-threaded `S`. A walk needing scoped state IS a `Recomposer` with
  `Piece = ()` — no fourth channel exists.
- All descent funnels through one `each_child` kernel — the recompose driver and
  the walk are clients of the same engine, which is what makes the wrapping
  contract's uniform lowering possible ([§dd-dr:recompose-machinery]).
- **`walk` is role-blind** — it visits everything, `Attached` and `Hidden` slot
  children included (debug honesty, [§dd-dr:slot-roles]) — in deliberate contrast
  to `Concat`'s content-scoped default: the read/compose asymmetry IS the ruled
  role semantics, not an accident of implementation.

Rejected alternatives: recompose-on-walker (above); `core::node` as the home (user
veto); `walk_tree`/`recompose_tree` twin entries (one-canonical-path); a
`Descendants::with_depth()` adapter (already rejected in T4 — flat iteration loses
structure; `descendants()` itself stays for structure-free queries).

Revisit if: an engine need tempts user state into `VisitContext` — the ruled
answer is the three-channel discipline, not context growth.

#### Slot roles and trait-based body marking [§dd-dr:slot-roles]

Status: DECIDED (user, API-review session; amends
[§dd-dr:latexlike-generalization]'s "preset keeps `NodeExts = ()`" per-member).

`ParsedSlot` gains `role: SlotRole { Content, Attached, Hidden }` (default
`Content`). `Content` = constitutive — the node's meaning is incomplete without it
(environment body); `Attached` = derived/redundant — reconstructible from the
invocation itself (`\input`'s resolved content, [§dd-dr:input-attachment]);
`Hidden` = framework/callable-defined attachments techy core ignores (no
recomposition, no byte accounting; semantics via slot name + spec). Load-bearing
consequence: **`Attached` slots are excluded from the parent's byte-tiling** —
declaration replaces source-change inference in the validator — while structural
child-list tiling stays role-independent. Precisely: children in an `Attached`
slot's region are excluded from the including callable's
children-in-source/contiguity checks and carry their own per-source accounting
(one source per attached region, contiguous within it); `Hidden`-region children
carry no byte accounting at all; the remaining children must be contiguous
across the excluded regions.
Body-ness is a **separate axis** on the slot *ext*:
`trait BodySlotExt { fn is_body(&self) -> bool; fn make_body() -> Self; }` —
environment machinery mints body slots via `make_body()` (the trait is also the
*generic* minting mechanism the `LatexlikeDriver<LLL>` generalization needs);
`NodeRef::body()` returns the content of the slot whose ext reports `is_body()`,
under a bound-where-used (`where SlotExt<L>: BodySlotExt`) — "slot 0" stops being
load-bearing. A framework forking the ext bundle implements the trait on its own
`SlotExt` and every preset mechanism keeps working. Consequence, ruled consciously:
`Latexlike` declares a real body-marker `SlotExt`, so the preset's `NodeExts` bundle
is `()` per-member for node/argument only — **`SlotExt` is claimed by the preset**.
Edge rulings: `body()` filters on
the ext axis alone — no hidden `role == Content` conjunction (a framework's
`Attached`-body choice must not become silently unfindable; one doc sentence
instead); readers and extract stay **role-blind everywhere except recompose** —
`Hidden` means "no recomposition, no byte accounting", never invisibility to
reads (structural walks and `display_tree` show reality — debug honesty);
`SlotRole` is **exhaustive** (match-heavy consumers — validators, recompose
strategies, FFI mappings; a fourth role changes byte-accounting semantics and
must be a conscious breaking change, the [§dd-dr:math-group-form] argument);
**`Attached`** over `Derived` (the shipped door names
`parse_attached_source`/`attach_source_reference` already teach the vocabulary);
restage descends into `Attached`/`Hidden` children uniformly
([§dd-dr:restage-ops]). A fourth **`CallSyntax` role was
rejected outright** — `SlotRole` stays the three-variant enum, and techy itself
mints NO `Hidden` slots (`Hidden` stays reserved for frameworks; trigger/scaffolding
spelling is invocation-syntax *payload*, [§dd-dr:invocation-syntax]). The one
role-sensitive site is concrete: `Concat`'s default scope is plain children +
`Content` regions — `Attached` AND `Hidden` skipped, widening explicit via
`include_attached()`/`include_hidden()` — while the walk stays role-blind
([§dd-dr:recompose-machinery], [§dd-dr:visit-engine]).

Rejected alternatives: body-by-slot-name (the `"body"` string — stringly-typed);
body-by-position (slot 0 — positional convention as API contract).

#### `\input` attachment: same-builder sub-parse; multi-source trees are first-class [§dd-dr:input-attachment]

Status: DECIDED (user, API-review session — direction and tree-level
consequences; engine wiring: [§dd-dr:input-wiring]).

The `\input` implementation: the callable's spec parser resolves the
reference and **sub-parses the resolved source into the same builder**, staged as an
**`Attached` slot** of the `\input` callable. Decisive: copy-free, and semantically
forced — included content must parse under the parsing state *at the `\input`
point*, which the running session has naturally. (Separate-parse-then-restage-splice
stays possible via the transform primitives for frameworks that want caching, with
the state-correctness caveat on their heads.) Tree-level consequences, verified in
the session:
- **Sibling-run source-coherence holds naturally**: the `\input` callable's own span
  is its invocation in the *includer's* source; only its slot children live in the
  included source, and those are siblings *of each other* — so every sibling run in
  a parse tree stays single-source even under nested inputs. The middle-node
  staleness hazard ([§dd-dr:tree-navigation]'s honest-slices fix) remains exclusive
  to transform-spliced trees; the single-source `finish()` fast-path flag is an
  optimization bit, **not** a semantic tier — a multi-source parse tree is not a
  degraded tree.
- The span-tiling law's byte accounting is scoped per source via the `Attached` role
  ([§dd-dr:slot-roles]); recompose is per-source (verbatim emits `\input{file}`,
  not the content — expansion is an explicit strategy choice); `node_at`'s
  per-source descent already yields the right answers on both sides of the boundary.
- **The resolver lives on the `ParseDriver`**, not `Language`:
  resolution is parse-time instance behavior, which the placement doctrine
  ([§dd-dr:parse-driver]) puts on the driver — amending [§dd-dr:language-init]'s
  expected surface (`Language` collapses toward the constructor alone; cf.
  [§dd-dr:source-resolver]).

**Input caching is neither implemented nor recommended** (user-identified flaw in
the cache-then-splice recipe): `\input` can return a **modified parsing state**
to the caller — the included content's delta sequence continues into the rest of
the including document (the preamble-defines-macros case) — so a document parsed
with attachment off is wrongly-stated downstream of every state-modifying
`\input`, and included files must in general be read on the spot at parse time.
The include documentation presents the splice recipe only under the explicit
precondition that the framework's `\input` does not modify caller state. The
state-feedback case is concrete in the shipped spec: `input_macro_spec` takes a
mandatory **`persist_state: bool`**, realized as merged after-effect deltas
returned by the door's outcome bundle and forwarded through the ordinary sibling
channel ([§dd-dr:input-wiring]) — so the splice recipe's state-transparency
precondition is a per-registration fact (`persist_state: false`), never a preset
guarantee. Riders: the level-0 primitive stays the sanctioned
cross-tree door ([§dd-dr:restage-ops]); latexpp's verbatim output path needs no
splicing at all — recompose emits `\input{file}` per source, so per-file
pipelines compose without tree merging.

*Amendment (user, span-tiling design session).* "Every sibling run in a parse tree stays single-source" is
a statement about a language that obeys span tiling ([§dd-dr:span-tiling]); a language
declaring `OBEYS_SPAN_TILING = false` can produce a run whose nodes lie in several sources,
which is one of the two honest `None` cases of the run accessors. The `Attached`-role
scoping of the byte accounting is unchanged.

Revisit if: the sub-parse-into-same-builder mechanics prove
unworkable — the fallback is the restage-splice route, accepting its copy cost.

#### Parent links, `SourcePos` lookup, and read-side honesty [§dd-dr:tree-navigation]

Status: DECIDED (user, API-review session).

- **Parent table stored**: the `Vec<u32>` that `finish()` already computes for
  region resolution is kept on the tree (4 bytes/node; reverses
  [§dd-dr:iter-storage-order]'s decline now that consumers exist — the FFI gap,
  the editor-cursor wish, pass-style renderers). `NodeRef::parent()` and an O(1)
  `index_in_parent()` (own index minus the parent's block start), both
  `Option`-returning; `ancestors()` REJECTED — tree visiting is top-down and an
  ancestry walk has zero trap surface
  (`iter::successors(node.parent(), |n| n.parent())`); the one-line recipe lives
  in `parent()`'s rustdoc.
- **`SourcePos<O> { source: Arc<Source<O>>, pos: usize }`** — a source-model
  type, analogous to `SourceSpan`, pointing to a *single location* (constructor,
  `source()`/`pos()` accessors, `Debug`, line/col via `LineIndex`;
  `SourceSpan::start_pos()`/`end_pos()`
  conveniences, with the exclusive-end doc sentence). Chosen over bare
  `(source, pos)` arguments (reads as two unrelated
  bits) and over empty-`SourceSpan` encoding (reads oddly). Vocabulary note: the
  editor-cursor lookup here and the retired char-scanning `SourceCursor`
  ([§dd-dr:source-cursor-retired]) are disjoint concepts sharing a word.
- **Point lookup** `NodeTree::node_at(&SourcePos)`: the **deepest** node whose span contains
  the offset — half-open containment (`start ≤ pos < end`, empty spans never match);
  descend only into children whose span lies in the **query's source** (per-source
  answers on multi-source trees, [§dd-dr:input-attachment]); only exact per-node
  spans are trusted, never inferred covering spans — robust on transform-spliced
  trees, degrading to the shallowest honest answer. An offset inside a node but in
  none of its children (group delimiters, trigger spellings) resolves to that node;
  ancestors come free via `parent()`. **Span lookup**:
  `NodeTree::covering_slice(&SourceSpan)` — the name carries the one fact callers
  must know: the result may cover *more* than the query — returns the minimal
  covering sibling
  run (`NodeSlice`, the node-list currency) within the deepest containing node list.
  Binary search over span-sorted siblings opportunistically, linear fallback.
- **Honest slices** (contract-final): `NodeSlice::span()`/`source_text()` answer
  only when the **whole run** lies in a
  single source — uniformity verified across the run instead of trusting
  first/last-node agreement (a replaced *middle* node
  no longer yields silently stale text), with the `finish()` single-source flag as
  the O(1) fast path; `source_text()` carries `span()`'s ordering guard so the two
  contracts read identically; `None` = no single-source answer; per-node accessors
  stay valid on any tree (a node's own span is its provenance). Doc-vocabulary
  rule (user): the word "honest" must not appear in the rustdoc contracts — state
  the concrete condition ("the run lies within a single source"); "honest slices"
  stays internal design-record vocabulary.
- `NodeRef::tree()` is public. The read-side structural walk is the free function
  `walk` in the top-level `techy::visit` module — trait `NodeVisitor`
  (enter/exit) + `VisitFlow`, one engine shared with the recompose driver; no
  `NodeRef::walk` (strata: core cannot name the techy-level engine);
  `descendants()` stays the flat stream ([§dd-dr:visit-engine]).
Rejected alternatives: a build-on-demand `ParentMap` helper (the table is free at
`finish()` and trees are immutable — no staleness to manage); an offset→node index
table (premature); parent-dependent data in `make_node_ext` (impossible bottom-up —
see [§dd-dr:ext-minting]).
Revisit if: profiling shows the per-node parent word or the honest-slice scans
mattering — both have obvious opt-out designs, neither worth pre-building.

## Construct parsers, dispatch, engine [§dd-dr:parsers-engine]

*Section-wide note: the construct-parser return pair's delta side is
**boxed** — `ConstructParser::parse` returns `(Self::Output,
Option<Box<ParsingStateDelta<L>>>)`, and `NodesOutcome::after_effects` is
`Option<Box<ParsingStateDelta<L>>>` — a decided per-frame stack-cost measure: the
pass-through delta family dominated the recursion cycle's stack frames while
nearly always carrying `None`. Rationale and scope: [§dd-dr:descent-guard].
Entries below that spell the pair unboxed predate the boxing.*

#### Single-context parsing API (`ParseContext`) [§dd-dr:parse-context]

Status: DECIDED (implemented; formerly proposed).

Bundles token reader + state + session handle + the language's driver, avoiding
pylatexenc's three-argument threading through every parser. One place to extend later
(e.g. depth limits, cancellation). Factory-created parsers
(`make_invocation_parser(&self, invocation)`, the `ArgumentParser` entry points) have no
constructor through which a caller could thread any of it, which is what the context
solves. It carries **no** source handle: locations come from the reader, which is the only
party that knows which source a token's offsets belong to ([§dd-dr:no-context-source]).
`NodesParser::new`/`GroupParser::new` likewise take no source — single source of truth.

#### `ParseContext` carries no source handle [§dd-dr:no-context-source]

Status: DECIDED (user-led, token-layer redesign).

A construct parser has no `Arc<Source>` to pair a byte range with. Every `SourceSpan` it
stages comes from the reader — one token's span, the span between two of that token's
edges, or `source_span_within(begin, end)` over two stream positions — or from another
`SourceSpan` it already holds. The empty span at the current position, the anchor most
diagnostics want, is `cx.here()`.

Decisive reason: with a source on the context, the natural spelling — build a
`SourceSpan` from the context's source and a byte range taken off a token — is also the
wrong one as soon as a reader serves a parse from more than one source, and nothing in
the types says which spans it is wrong for. Removing the handle makes the wrong span unspellable instead of merely discouraged
([§dd-dr:token-opacity]).

The node tree is unaffected: a node's span is still one single-source `SourceSpan`, and
sub-spans recorded in node data are still byte ranges relative to that span. What changed
is that a reader answer becomes node data only after a `same_source` check against the
node's span — one spelling, `node_text_content`, records `TextContent::Spanned` when the
check passes and `TextContent::Owned` otherwise, and a site that cannot own its text
records the item as absent instead. Under the standard reader every check passes; it costs
an `Arc` pointer comparison and states the pairing where it used to be assumed.

`stage_invocation`'s end rule follows the same line: an explicit end is a stream position;
otherwise the node ends where the last staged child ends when that child lies in the
trigger's source, and at the current stream position in every other case (no child, or a
child from elsewhere).

Rejected alternatives: keeping the field for convenience and documenting when it is safe
(the documentation would be the only guard, and the unsafe use is the shorter one to
write); a reader accessor answering "the current source" (the same problem one level down
— "current" is not the source a given token came from).

Revisit if: never for the handle. The `same_source` clauses are what a reader serving
several sources will exercise; they decide there what today they only confirm.

#### Dispatch by token kind + library lookup [§dd-dr:token-kind-dispatch]

Status: DECIDED (implemented; formerly proposed).

See [§dd-dr:deterministic-dispatch].
Rejected alternatives: `can_parse()`/`priority()` parser registries (registration-order-dependent,
scattered dispatch logic, priority races).

#### `Language<L>` owns no per-parse state [§dd-dr:stateless-language]

Status: DECIDED (user-led).

Long-lived, reusable across parses, accumulates no memory. Sessions are
transient; results are frozen.

#### Two-tier ownership: stored specs are immutable data; construct parsers are temporaries [§dd-dr:parser-temporaries]

Status: DECIDED (user).

Tier 1, *stored* behavior objects (specs; `ArgumentParser`s inside `ArgumentSpec`):
`Arc`-shared, `Send + Sync`, immutable; every per-use input arrives as arguments of their
entry points (`&self`). Tier 2, *engine* construct parsers (`NodesParser`, the group parser,
invocation parsers, body parsers): short-lived values constructed with their per-use
configuration where they are needed, free to borrow (`'s` content, token refs),
`parse(&mut self, …)` so working state may live in fields, dropped when the frame ends. No
`Send + Sync`, no `'static`, no `OnceLock`/`static` gymnastics — those pressures existed
only in designs that *stored* engine parsers.
Rationale: mutable working state and stored sharing are incompatible without locks; giving
each tier one job removes the conflict. Closures (stop predicates) are thereby confined to
tier 2 — specs stay data ([§dd-dr:data-vs-traits]).

#### `CallableSpec::make_invocation_parser` — a factory moving a fresh parser to the caller [§dd-dr:invocation-parser-factory]

Status: DECIDED (user; a third option superseding both sketched ones).

```rust
fn make_invocation_parser<'a>(
    &'a self,
    invocation: Invocation<'a, L>,  // callable_type, name, spec, the triggering token
) -> Box<dyn ConstructParser<L, Output = BuildId> + 'a> {
    Box::new(StdInvocationParser::new(invocation))
}
```

The dispatch loop resolves the callable (`Lang::resolve_command`, or the spec riding on a
`Specials` token), builds the `Invocation`, calls the factory, runs `parser.parse(cx)`,
drops the parser. Overriding the factory is the full-takeover escape hatch (`\verb`,
tabular preambles, FLM constructs).
Rationale: all parser logic lives in construct parsers — specs only *supply* parser
objects; the invocation travels inside the parser instance, so `ConstructParser::parse`
keeps one uniform signature; and this is exactly pylatexenc's `get_node_parser(token)`
shape (a parser instance built for that token), with ownership made explicit.
Rejected alternatives: a defaulted `parse_invocation(&self, cx, &Invocation)` method on the spec
(fuses factory and call — parsing methods on specs); a cached `Arc<dyn ConstructParser>` in
the spec with the pending `Invocation` in a `ParseContext` field (a set-before-use protocol
spanning every spec and every dispatch — the regions two-phase records accepted that genus
of invariant only because it is contained in one component at one point); a generic
`with_invocation_parser(inv, closure)` (stack allocation, but kills `dyn CallableSpec`
object safety).
Revisit if: the per-invocation `Box` allocation shows up in parse-throughput profiles
(run a micro-benchmark; [§dd-dr:open-questions]). If it ever matters, the dispatch
loop can special-case the default path without touching the trait. (The benchmark check
was consciously deferred, not dropped — user decision; the obligation stands open,
unscheduled.)
*(Composition finding:* a composition running
*inside* `parse(cx)` cannot build a **new** `Invocation` for a construct it resolves
mid-parse — `Invocation.name` borrows the matched name, which comes from the reader's view
of a token the composition does not hold ([§dd-dr:token-opacity]). So a two-level dispatch
— a `\begin` spec's parser calling the resolved environment spec's own
`make_invocation_parser` — does not work with the `Invocation` shape; the standard
composition instead drives `EnvironmentBodyParser` directly under the resolved spec. A
*stored* triggering token, by contrast, is handed back to the context's reader without
difficulty now that a token carries no lifetime of its own: a takeover repositioning past
its trigger's swallowed post-space writes
`cx.tokens.move_to(self.invocation.token, TokenEdge::End)`.)*

#### `Lang::finalize_node`: one centralized finalization hook in the builder [§dd-dr:finalize-node]

Status: DECIDED (user; supersedes a spec-level `finalize_invocation` proposal —
pylatexenc's `CallableSpec.finalize_node` precedent. **Superseded** — API review:
replaced by parse-once ext minting, [§dd-dr:ext-minting]).

Called inside `NodeTreeBuilder::add` for **all** nodes (every kind, not just callables),
before the staging checks; receives mutable access to the node's parts (kind, uniform ext,
span, state) plus a read-only view of already-staged nodes (so a callable's hook can
inspect its children — e.g. extract scaffolding sub-spans, [§dd-dr:nodes]); default: no-op.
Rationale: the builder is the single mutation boundary, so hooking there guarantees *no
node escapes finalization* — no parser cooperation required, transforms and tests included.
A preset delegates to spec-specific behavior itself (FLM's `Lang` sees a `Callable`, reads
`data.spec`, downcasts, attaches its `flm_specinfo`-like ext — the `Any`-supertrait
contract, [§dd-dr:specs]; downcasting to the preset's own spec *trait* goes through the
concrete-wrapper pattern recorded there), so the core needs no spec-level hook at all;
and *uniform* per-node initialization (fields every node of a language carries) gets a
natural home, which a callables-only spec hook could never provide.
*Consequences:* the hook must tolerate re-staging (transform-built trees pass nodes through
a new builder — finalization runs again on already-finalized data; implementations must be
idempotent); it runs on speculatively staged nodes that may be abandoned (harmless — they
drop unreachable); the builder grows a small staged-node read view (also wanted by
node-based stop predicates, below).
Rejected alternatives: spec-level finalize in core (callables-only; custom invocation parsers must
remember to call it); a `ParseContext`-side helper (forgettable, and transforms bypass it).
*(Superseded — API review: the hook becomes the value-returning, parse-time-only
`Lang::make_node_ext`; the idempotence contract and run-on-transforms behavior are
deleted. The `ParseContext`-side placement rejected above is essentially the shape now
adopted — its "forgettable" loophole closed by making `ParserSession::builder`
crate-private, so parsers cannot stage around it. [§dd-dr:ext-minting].)*

#### `Lang::resolve_command` hook [§dd-dr:resolve-command-hook]

Status: DECIDED (user; return type refined — next two entries).

`Command` tokens resolve through
`fn resolve_command(&self, state, token: &L::Token, tokens: &dyn TokenReader<'_, L>) ->
Result<CommandResolution<L>, ParseError<…>>` (the hook lives on `ParseDriver`, not `Lang`;
`Resolved(ResolvedCallable { callable_type, spec })` / `Unresolved { detail }`). It typically
dispatches to the state's scopes, building the query from the token's view —
`tokens.token_kind(token)` answers `Command { name, escape_char }`, which becomes
`CallableQuery { name, syntax: Command { escape_char }, … }` ([§dd-dr:token-opacity]).
An `Unresolved` answer → the nodes parser diagnoses and recovers ([§dd-dr:errors]).
Specials need no hook: recognition = resolution; the token already carries its spec.
Rationale: the dispatch loop needs `(CallableTypeId, spec)` for command tokens and the
core cannot know a preset's type ids; follows the `scan_specials` precedent (a `Lang` hook,
recognition kept close to resolution).

#### `CommandResolution` carries a failure `detail`; the default hook reports itself [§dd-dr:resolution-detail]

Status: DECIDED (user).

Two needs, one channel. (1) Forgetting to implement `resolve_command` has no compile-time signal: a
language that enables commands but keeps the default hook saw every command fail with a
bare "cannot resolve", nothing pointing at the actual cause. (2) Resolvers that *are*
implemented often know why a name failed and had nowhere to say it ("searched libraries
x, y, z"; "load the {amsmath} library for this command"). Decided: the hook's return type
is `CommandResolution` — `Resolved(ResolvedCallable)` or `Unresolved { detail:
Option<String> }` — and the dispatch sites hand the detail to the `UnresolvableCommand`
condition (field `detail`, serialized; hand-written `Display` appends it parenthetically).
The trait default answers `Unresolved` with a core-provided detail ("command resolution is
not implemented by this language's driver — implement `ParseDriver::resolve_command` or use
a preset"), so the forgot-the-hook wall names its own cause; `From<Option<ResolvedCallable>>` maps
lookup misses to detail-less `Unresolved`, so implemented resolvers stay bare unless they
opt in. The detail ships in **all** builds — it is precise by construction (only the
answering resolver produces it), so debug-gating buys nothing.
*Why not the debug-build warning first envisioned:* the crate is `no_std` with no `std`
feature (no `eprintln!`), the dependency policy routes conditions through diagnostics, not
logging; Rust cannot detect "default method not overridden", and the workaround — a global
`AtomicBool` the default body sets and dispatch sites consult — would make a hook
side-effectful (against the hooks' purity doctrine) with racy cross-`Lang` attribution.
*Rejected along the way:* a `resolver_unimplemented: bool` on the condition (subsumed: the
general string is strictly more expressive, and "unimplemented" is just one detail value);
a separate `Unknown`/`Unimplemented` variant pair (once the detail string exists the
distinction is prose, not structure — and "Unknown" would be a misnomer for hints like
"load {amsmath}", where the name *is* known); an unconditional hint on every unresolvable
command (wrong and noisy for real languages, and every preset would have to scrub it via
`refine_diagnostic`); docs-only (the runtime wall stays).

#### `CommandResolution::Failed`: operational failure distinct from a clean miss [§dd-dr:resolver-failure]

Status: DECIDED (user, review follow-up).

`resolve_command` returns three outcomes: `Resolved`, `Unresolved { detail }` (a clean
miss — the name is defined nowhere the query saw), and `Failed { detail }` (a definition
*provider* errored while answering — a broken or unavailable source). The dispatch sites
diagnose `Failed` as a distinct condition — `CommandResolutionFailed`
(`core.specs.command-resolution-failed`), separate from `UnresolvableCommand` —
recovering the same way (span-backed chars). The shared scope-stack resolver
`CommandResolution::resolve_via_scopes` (the one home for the preset and the test langs)
maps a provider `Err` to `Failed`, where the per-driver copies previously flattened it into
`Unresolved`.
Rationale: tooling and `refine_diagnostic` can now tell "command unknown" from "resolver
broken" by condition identity rather than string-sniffing the detail; this mirrors the
`ScopeOpFailed` precedent, which likewise gives operational scope-stack failures their own
condition. `CommandResolution` is `#[non_exhaustive]`, so the added variant is non-breaking
downstream (the wildcard obligation already stands).
*Distinct from* the earlier-rejected `Unknown`/`Unimplemented` variant pair: that split was
along *miss-reason prose* (subsumed by the detail string); this one is along *miss vs.
operational error* — an outcome axis a detail string cannot carry to a consumer keying on
the condition id.
(Later revised, [§dd-dr:hook-fallibility]: `resolve_command` returns
`Result<CommandResolution<L>, ParseError<_>>` — `Failed` remains the
diagnose-and-recover outcome; an `Err` aborts. The outcome axis and the abort
channel are distinct and both documented on the hook.)

#### `Lang::make_paragraph_break_node` hook [§dd-dr:paragraph-break-hook]

Status: DECIDED (user; upgraded from "core default only").

`fn make_paragraph_break_node(&self, state, break_span: &SourceSpan<L::SourceOrigin>) ->
NodeKind<L>` on `ParseDriver`; default: a whitespace-only `Chars` kind,
`TextContent::Spanned` over the full break span (newlines included). The *core* stages the
returned kind with the span it passed in and the current state — a driver cannot stage nodes
itself. `break_span` is the break's span as the reader answers it, so a callable-shaped kind
takes the break's spelling from `break_span.content()` (the preset's specials-formed break
does exactly that, with `LatexlikeInvocationSyntax::specials_form()` as its payload).
Rationale: paragraph-break representation belongs to the preset;
returning a `NodeKind` keeps callable-shaped paragraph breaks (FLM) expressible without a
redesign, while the default preserves the whitespace-as-chars invariant ([§dd-dr:nodes]).

#### Stop conditions: reified values, tier-2 predicates; abnormal endings are data [§dd-dr:stop-conditions]

Status: DECIDED (user; pylatexenc-informed).

`NodesParser` accepts a stop specification with two independent
triggers, mirroring pylatexenc's well-tested pair:
- *token condition* — a small closed enum (`Command(name)`,
  `GroupClose(group_type, close)`, `ParagraphBreak`, …) **or** a programmatic predicate
  (`Fn(&L::Token, &dyn TokenReader<'_, L>) -> Result<bool, ParseError<_>>` — fallible,
  [§dd-dr:hook-fallibility]; the predicate holds no reader of its own, so it is handed
  the one that produced the token, [§dd-dr:token-opacity]);
- *node condition* — a programmatic predicate consulted after each node is assembled,
  receiving (node count, view of the just-staged node) — covers pylatexenc's
  `stop_nodelist_condition` uses (stop-after-one-node, `LatexSingleNodeParser`).
Semantics pinned: a token-condition match, by default, leaves the token **unconsumed**
(the caller peeks it) — or consumes it when the condition's `consume` flag is set, a
declarative switch that is never pylatexenc's `handle_stop_condition_token` interpretation
hook (consume amendment below); a node-condition match includes the triggering node and
stops after it; conditions are
tested only at the parser's own nesting level (nested groups are consumed whole by the
group parser, so an `\end` inside a brace group cannot terminate an environment body).
`NodesParser` returns its `StopCause` — `TokenCondition { span, after }` /
`NodeCondition` / `EndOfInput` / `UnexpectedGroupClose { span, after }` — and the *caller*
decides which causes are errors ([§dd-dr:errors]). Both token-bearing causes carry the
matched token's `SourceSpan` **and** `after`, the stream position just past it: a caller
that wants to skip the token can no longer compute that place from the span
([§dd-dr:stream-position]).
Deliberate deviations from pylatexenc: the node predicate sees (count, last node), not the
whole node list on every iteration (pylatexenc's `stop_nodelist_condition(nodelist)`
invites O(n²) rescans); predicates live only in tier-2 parser temporaries, never in spec
data ([§dd-dr:data-vs-traits]).
*`GroupClose` matches the exact `(group_type, close)` pairing, not either field alone:* both must hold, because each guards a distinct
false match. `close` disambiguates delimiters *within* a class — where `{`/`}` and `[`/`]`
share one `GroupTypeId`, a group opened with `{` must not stop at a stray `]`. `group_type`
disambiguates classes *within* a delimiter — if a mid-stream state delta re-classes `}` to
close a math group, that `}` must not close the enclosing brace group (wait for one that is
a brace close again). The close token carries only `delim` (the pairing's type is a
property of the *open*, which alone establishes group identity; the close merely matches —
see the group-token entry), so the class is re-resolved from the current state at match
time — the same state the tokenizer used, so a reclassifying delta is honored. A close that
matches neither field is left unconsumed and surfaces as `UnexpectedGroupClose` for the
caller to adjudicate.
*Token stop conditions carry a `consume` switch; `StopCause` reports the matched span:* `TokenStopCondition` became a struct
`{ kind: TokenStopKind, consume: bool }` (the closed enum renamed `TokenStopKind`), so the
flag binds to the presence of a token condition — an orphan `consume` with no condition is
unrepresentable. On a match `NodesParser` either leaves the token (`consume = false`,
reader parked at the token's `Start` edge) or takes it whole (`consume = true`, moving to
its `EndPastPostSpace` edge, past any syntactic post-space). Two reasons over the earlier always-unconsumed rule: (a) the common
closer parsers (a group parser consumes its `}`, …) stop hand-writing the consume line; and
(b) **atomicity** — consuming at match time uses the exact state that matched, whereas
leave-then-re-peek re-tokenizes at `span.start` under whatever state is *now* current, which
can reclassify the delimiter (a delta that drops the close rule makes `}` come back a
`Char`, desynchronizing the caller). A post-hoc consume helper cannot fix this — it, too,
re-peeks. `StopCause` accordingly split `StopConditionMet` into `TokenCondition`
(token stop) and `NodeCondition` (node stop), and `UnexpectedGroupClose` carries a span: the
group parser builds its `Spanned` close delimiter from that span, which it can no
longer re-peek once the token is consumed. No `consumed` field — the cause discriminant plus
the caller's own `consume` already determine it. Consume always moves to the token's
`EndPastPostSpace` edge:
a *closing* token has no *content* space to preserve — a command's trailing space is its
name-terminating **syntactic** post-space (a sub-range inside `span`, absorbed by the macro),
and a `GroupClose` reports no post-space at all (any following whitespace is already the next
token's `pre_space`) — so a `leave_post_space` knob has no correct caller and does not exist.
Scope pitfall: the flag only expresses consume decisions knowable from the stop token *at
match time*; the environment-body terminator (close-without-consuming on a name mismatch,
below) hinges on the name group *after* `\end`, invisible at the stop point, so it stays
manual post-stop lookahead regardless of the flag.
*The token condition wins outright; the pre-stop flush does not consult the node
condition:* the two triggers can collide — a token
match flushes the pending chars run (stop token's pre-space included) as a final node,
and that node could satisfy the node condition. Decided: that flush stages **without
invoking the node predicate**, and the cause is `TokenCondition`. Consulting it could
change nothing (the parse ends either way), and *honoring* a match instead would leave a
`consume = true` stop token unconsumed — forfeiting exactly the at-match-time atomicity
the consume flag exists for. Since the predicate is a stateful `FnMut` (latching
conditions are the norm), even a consulted-but-ignored call is an observable side effect:
the caller's closure would end the parse believing its condition fired while the cause
reads `TokenCondition`. pylatexenc is no precedent here — it checks the nodelist
condition on the deferred pending-chars flush inside `finalize()`, setting *both*
met-flags (an ambiguous cause) and re-raising `ReachedStoppingCondition` out of
`process_tokens`'s `finally`, which `LatexGeneralNodesParser` never catches: the
collision is a latent control-flow leak upstream, unhit only because no caller combines
both conditions this way.
Rejected alternatives: a declarative stop-condition language in spec data (terminators are
parser business — [§dd-dr:slot-terminators]); closure storage in specs; a consume *callback* handed to the stop
predicate (or a `Stop { consume }` predicate return) — it only adds per-match branching
inside one heterogeneous predicate (rare), reaches neither the declarative variants (the
common `GroupClose`) nor the post-lookahead case, would force a second consume mechanism
alongside the static flag, and turns a pure condition into a reader-mutating actor;
deferred until a real dynamic-consume consumer appears, and even then localized to the
`Predicate` variant.

#### Slot terminators are parser business; the environment-body parser is core, parameterized [§dd-dr:slot-terminators]

Status: DECIDED (user; settles the terminator question against both sketched options).

No declarative terminator vocabulary enters core spec data.
The data of the rejected declarative design (stop-command name, name-group type,
match-invocation-name) becomes the *constructor parameters* of a core
`EnvironmentBodyParser`: it runs `NodesParser` with a stop condition on the terminator
command, verifies the name back-reference (`\end{align}` matches the `align` that opened),
stages the body `List`, and leaves post-space claiming to the invocation parser driving it.
Environments remain zero-custom-code for spec authors — the preset's `EnvironmentSpec`
instantiates the parser from data. Verbatim bodies need no terminator-state
doctrine at all: a verbatim construct's parser reads raw content itself and never runs
`NodesParser` — the "which state scans the terminator" question dissolves.
Rationale: a declarative terminator spec re-creates a parser-description language inside
spec data for exactly one consumer, while the parameterized parser expresses the same
constructs with the same zero user code; core placement is legitimate because every
parameter is data — no privileged spellings ([§dd-dr:no-privileged-concepts]).
Rejected alternatives: `SlotTerminatorSpec`/`StopConditionSpec` as core spec data;
stop-before-terminator with preset-owned consumption (weakened the declarative
path and left terminator syntax neither recorded nor reconstructible).
(`SlotSpec` itself was later deleted — no spec-side slot declarations at all,
cf. [§dd-dr:no-spec-side-slots]; this ruling's core — terminator data as
`EnvironmentBodyParser` constructor parameters — stands, with the body state delta
rehomed to the preset spec type that drives the parse.)
The shipped constructor is
`EnvironmentBodyParser::new(trigger_span, invocation_name, stop_command_name,
name_group_type)` — two approved adjustments to the sketched parameter set. `trigger_span`
anchors the missing-terminator diagnostic at the `\begin{…}` that opened (the
`GroupParser` unclosed-at-open precedent); `invocation_name` is always required — every
terminator diagnostic names the environment — with the name *check* a builder switch,
`with_match_invocation_name(bool)`, default true (disabled = any rigid name group closes).
"Rule/type" resolved to **type**, matching `GroupArgumentParser`'s parameterization.
Behavior pins: body and terminator are both read under `cx.state` =
the slot's state (caller-scoped, like arguments), and the body `List` records that state —
the interior state is the honest one for a delimiter-less region (a `Group`, by contrast,
records the outer state; the environment node itself records the invocation's base state).
A token error mid-terminator follows the probe rule: strict aborts; tolerant treats
the position as a malformed terminator without diagnosing the token error — the enclosing
loop re-reads it and applies its own recovery, no double report.

#### Terminator mismatch recovery: close without consuming [§dd-dr:terminator-mismatch-recovery]

Status: DECIDED (user).

`\begin{A}…\begin{B}…\end{A}`: B's body parser stops at `\end`,
reads the name, sees the mismatch → diagnostic ("missing `\end{B}`"), closes B **without
consuming** the terminator, and returns; the unwinding lets A's parser find and consume its
own `\end{A}`. An orphan `\end` eventually reaches the root nodes parser as an ordinary
command and takes the unresolvable-command recovery ([§dd-dr:errors]). A *malformed* terminator
(`\end` not followed by its name group) is diagnosed, **consumed**, and closes the
environment — leaving it unconsumed would cascade the same malformed token through every
enclosing level. Loop safety: every level either consumes the token or unwinds out of its
own frame; the root always consumes.
Accepted consequence: "was this environment properly terminated?" lives in `Diagnostics`,
not on the node — a preset wanting it on the node flags it in ext via `Lang::finalize_node`.
Pinned details: the malformed-terminator "consume" is the terminator **command alone**, its post-space included — whatever follows re-parses as
enclosing content (`\end[y]` → sibling `[y]`; `\end{ A }` → sibling group). The command is
the token whose re-cascading this decision forbids; consuming beyond it would eat content
on a guess. And a stray group close *inside the body* — a case the original recovery list
omitted — resolves by the loop-safety rule: missing-terminator diagnostic + close
**without consuming**, the `GroupParser` unwinding analog; the stray close then reaches a
level that claims it (an enclosing group consumes it silently — one honest diagnostic
total; at the root, the root's diagnose-and-skip adds its own).

#### `ChildStateSpec`: per-use descent-state policy on `NodesParser` [§dd-dr:child-state-spec]

Status: DECIDED (user, child-state design session; ports pylatexenc's `make_child_parsing_state`).

`NodesParser` gains per-use configuration alongside `StopSpec` (same tier-2 borrowed-config
role): `ChildStateSpec { group: GroupChildState<'p, L>, invocation:
InvocationChildState<'p, L> }`, each `Inherit` (default — today's behavior) |
`Fixed(Arc<ParsingState<L>>)` | `Compute(&'p dyn Fn(…) -> Arc<ParsingState<L>>)`. The
descending arms resolve the child construct parser's **base state** through it: the
`GroupOpen` arm through `group` (compute context: the open `Token`, which carries `delim` +
the resolved `Arc<GroupRule<L>>`), the `Command`/`Specials` arms through `invocation`
(compute context: the `&Invocation` — callable type, name, spec, trigger token). Motivating
use case: a macro argument parsed chars-except-groups — a delta-restricted state (commands/
comments cleared, groups kept) whose group interiors *revert to the outer state*:
`group: Fixed(outer)`.
Decided semantics: (1) *resolution precedes policy* — `Lang::resolve_command` runs under the
loop's own `cx.state`, coherent with the state that tokenized the token, and what makes the
resolved spec available to the callback (pylatexenc's hook likewise runs post-resolution,
receiving the node class); (2) *the policy provides the base; spec deltas stack on top* —
`ArgumentSpec`/`SlotSpec` `parsing_state_delta` derive from the policy's answer, so a
caller's rule applies "before the callables add their own deltas"; (3) *one level deep* —
the `NodesParser`s recursed into by group/invocation parsers default to `Inherit`; note that
group-delimited *arguments* of an invocation are reached by the `invocation` policy (they
are parsed inside the invocation parser), not by `group`; (4) *sibling deltas unaffected* —
still applied to the loop's own `cx.state` ([§dd-dr:parsing-state]'s outward-propagation design already
blesses applying a delta to a base the producer never saw); (5) *states pass as-is* —
`Arc` in, `Arc` out: `Inherit`/`Fixed`/pass-through `Compute` never force a `derived()`,
and returning the same `Arc` preserves pointer identity (the [§dd-dr:tokens] identity-keyed
memoization argument stays sound); (6) *callbacks are pure `&dyn Fn`* (like
`TokenStopKind::Predicate`, unlike the node condition's deliberate `FnMut`): a descent
policy whose answer depends on call order would be fragile, and `Fixed` covers the
stateless case. (Re-examined and upheld against session access — cf. [§dd-dr:session-derivation]:
precompute-and-select expresses context-dependent
policies purely, with full `Arc` sharing.)
Pitfalls recorded: group termination self-heals under any policy base (the group parser
sets `expecting_group_close` on the interior state, which takes tokenizer precedence), but
environment bodies do not — a base that cannot tokenize `\end` runs the body to
`EndOfInput`, surfacing as `StopCause` for the caller to diagnose; and "disable specials"
was not delta-expressible when this was decided — settled by `enable_specials`
([§dd-dr:enable-flags]).
Rationale: descent-state control is the one pylatexenc state hook with no techy
equivalent, and pure parser composition (run `NodesParser` to a `GroupOpen` stop,
invoke the group parser directly under the other state, loop) — while it works and remains
the escape hatch — re-implements the stitching at every use site; the knob makes the
common case declarative.
Rejected alternatives: three fields keyed by token kind (`command` + `specials` would be near-identical
enums; the real split is the descent pathway, and `Invocation` already carries
`callable_type`); routing through `StateExt` + `finalize_transition` on a group-entry event
(can only *reconstruct* rules, never restore the actual outer `Arc`, and makes a generic
chars-except-groups argument parser depend on `Lang` cooperation); letting the policy
influence *resolution* (would resolve under a state other than the one that tokenized the
token, and voids the callback's resolved-spec context).

#### Group interior states are memoized in the session [§dd-dr:state-memoization]

Status: DECIDED (user, child-state design session; conditional go — "adopt if straightforward" — and it is).

The
group parser keeps always-derive *semantics* (every interior state carries its
`expecting_group_close`: the uniform invariant, and the recognition guarantee for the close),
but the derivation is deduplicated through a memo in `ParserSession`: a small vector of
`(base, rule, interior)` `Arc` triples matched by `Arc::ptr_eq`, consulted only for the pure
expecting-close derivation (no spec or policy delta in play). The memo is exposed as a
*narrowly-typed* session helper (`group_interior_state(base, rule)` or similar) — the
general session wrapper `derived_state` exists for instrumentation but **never** memoizes
(see the session-mediated derivation entry below): `state.derived(&delta)` remains the
underlying pure transition, and memoization exists only where the derivation inputs have
`Arc` identity —
an arbitrary `ParsingStateDelta` has no equality (`L::StateExt`/`L::Event` are not
`PartialEq`; `Arc<dyn SpecLookup>` has none), and non-recurring derivations gain nothing
from a memo anyway. (Spec-side deltas *do* have identity — `Arc<ArgumentSpec<L>>` carries
its `parsing_state_delta` — so a `(base, spec)`-keyed entry kind is a possible later
extension, strictly profiling-driven.) Sibling `{…}` groups under one
state then share a single interior `Arc` — one `StateData` clone per `(base, rule)` instead
of per group descent, the dominant state-cloning cost in deep documents. Entries hold their
key `Arc`s alive, so pointer keys cannot be reused (no ABA hazard). Consequences:
`Lang::finalize_transition` runs once per `(base, rule)`, not once per descent — its
contract is already a pure function of `(data, prev, events)`; and `derived()` itself still
always mints a new `Arc` (the memo sits in the group parser, calling it less often), so the
[§dd-dr:tokens] peek-idempotence argument is untouched — shared interior `Arc`s only *raise* the hit
rate of any future identity-keyed reader memo.
Rationale: `ParsingState` cannot host the memo — its derived caches are eager by decision
(no_std: no `OnceLock`, and `OnceCell` would cost `Sync`) — but `ParserSession` is the
parse's designated mutable surface, `&mut`-threaded, needing no synchronization.
Rejected alternatives: a memo inside `ParsingState` (above); skipping derivation when the close is
already table-resolvable in the base state (saves the same clones but leaves children of
plain brace groups recording a state whose `expecting_group_close` is `None` — the memo
gets the savings without the semantic wrinkle).
Revisit if: profiling shows the linear memo scan or memory growth under pathological
nesting (one entry per depth level) warrants a map or a cap — or 6.3 implementation
friction appears, in which case ship plain always-derive and flag for performance review.
(Later generalized, performance review: the memo lives uniformly in `derived_state` —
cf. [§dd-dr:memoized-derivations]; `group_interior_state` stays as a shape-guaranteed
wrapper, and the linear `Vec` scan became a `hashbrown` map.)

#### Session-mediated derivation is the in-parse standard; transitions have two levels [§dd-dr:session-derivation]

Status: DECIDED (user, child-state design session follow-up; extends the memo entry above).

Within a parse frame, construct parsers obtain derived states through the session:
`ParserSession::derived_state(&base, &delta) -> Arc<ParsingState<L>>` — the unkeyed general
form, instrumented but **never memoized** (an arbitrary delta has no identity, per the memo
entry) — plus the narrowly-typed keyed helpers where derivation inputs have `Arc` identity
(`group_interior_state(base, rule)`; a `(base, Arc<ArgumentSpec>)` form possible later).
Pinned invariant: **the session layer is data-equivalent to `ParsingState::derived()`** —
it may dedup and observe, never alter the resulting state. `derived()` stays public and
pure and remains the sole real constructor; out-of-parse code (initial states, tests,
post-parse tree transforms) keeps calling it directly. The standard is scoped to parse
frames and enforced by convention — sound because only gracefully-degrading features live
on the session layer: a bypassed memo is a missed dedup, a bypassed observation a missed
count, never a wrong state.
The two levels exist because two different questions get asked at a transition:
*constructing the data* is `Lang::finalize_transition` (inside `derived()`, unchanged) — a
pure function of (base data, delta, events), structurally airtight, and memoizable
*because* pure: it runs once per unique derivation, **not** once per transition event.
*Observing the event* is `observe_transition(&mut L::SessionExt, &mut
Diagnostics<_>, prev, new, &delta) -> Result<(), ParseError<_>>` (home: the
driver, [§dd-dr:parse-driver]; sink and abort channel: [§dd-dr:hook-fallibility]),
called by `derived_state` on **every** transition event, memo hits included.
Parse-history accumulation ("how many times did the parse enter math mode") belongs to the
second level; in finalize it would be doubly wrong — states revert structurally, so a
state-side counter yields nesting depth at best, and the memo skips finalize on repeats,
so counts under-report. (Mirrors pylatexenc's walker-level parsing-state event handler.)
`Lang` gains **`SessionExt`** (parse-global mutable extension, `Default`-initialized,
stored on `ParserSession`) — the preset-owned mutable object, and the home for parse-global
caches. The ext-cache pattern under pure finalize: expensive shared tables are computed at
the *delta-producing site* (construct parsers have the session) and shipped `Arc`'d inside
the delta (ext replacement or event payload); finalize installs them cheaply. Structural
sharing covers the rest — `StateData::clone` preserves `Arc` identity, and whole-state memo
hits share ext caches wholesale.
The seam ships whole — the memo never exists without its observation channel.
Rationale: one seam hides memo storage, gives transition provenance a place to reach
`Diagnostics` (the "deltas are inspectable data" promise), and sees memo-hit transitions —
which no state-level hook can, by construction.
*Costs accepted:* two derivation idioms coexist, separated by documented scope; Rust has no
stable associated-type defaults, so every manual `Lang` impl writes `type SessionExt = ();`
(`TrivialLang` absorbs it).
Rejected alternatives: `get_derived_state` naming (the crate's first `get_` prefix; `derived_state`
chosen — adjective form matching `ParsingState::derived`); giving `finalize_transition`
session access (forfeits the memo and breaks data-equivalence of out-of-session
derivations); session access for `ChildStateSpec::Compute` callbacks (a policy hook could
stage nodes and emit diagnostics, and the loop's borrows tangle — purity upheld:
precompute-and-select covers context-dependent policies with full `Arc` sharing, and the
designated first relaxation, should a consumer demand latching state, is `Fn` → `FnMut` on
the node-condition precedent, not session injection).
(Later revised, performance review: "never memoizes" no longer holds — rules-only
deltas are memoized inside `derived_state` itself, cf. [§dd-dr:memoized-derivations].
The original reasoning — arbitrary deltas have no identity — survives as the *gate*:
deltas carrying ext/events/library pushes still always derive fresh.)

#### Rules-only derivations memoized uniformly in `derived_state`; `hashbrown` adopted [§dd-dr:memoized-derivations]

Status: DECIDED (user, performance review; supersedes the never-memoize rule of the two
entries above — `derived_state` is the single memoization seam, narrow helpers wrappers
over it).

The gate is decidable without payload equality — the insight that unblocked this: the
three fields that kill general delta comparability (`ext: Option<L::StateExt>`, `events`,
`push_libraries`) only need **emptiness** checks. A delta carrying none of them is a pure
rules transition, and `derived()` is a pure function of (base data, delta, events) by
`finalize_transition`'s purity contract. Keys are (base state by `Arc` identity,
`TokenRulesOverrides` with payloads by `Arc` identity and gates by value): pointer-equal
implies value-equal, so identity keying can only miss, never falsely hit. Enabled by
making every override payload `Arc`-shaped: `CommandRule`/`CommentRule` symmetrized with
`GroupRule` (`Vec<Arc<…>>`, inner `String`s stay plain), `Arc<str>` for
`whitespace.chars`/`forbidden_chars` — independently motivated, since the `StateData`
clone at every transition becomes refcount bumps (a consequence for `Lang` authors:
`finalize_transition` rewrites a shared rule via `Arc::make_mut`, clone-on-write).
Consequences: the optional-argument probe (`\item[a]`, and the more common bare `\item`)
hits the memo from its second occurrence under a given loop state — the measured worst
case (four `\item[a]` siblings: 8 derivations, 4 permanent misses) collapses with no
argument-parser cooperation. `group_interior_state` remains as a thin wrapper that
guarantees a memoizable delta shape *by construction*: hand-built hot deltas can silently
fall off the memo path (one added event kills dedup with no warning, a perf cliff no test
catches), so the wrapper makes group-descent dedup a compile-time contract rather than an
emergent property of delta shape.
*Retention:* entries pin their key `Arc`s for the session's lifetime — deliberate and
load-bearing, not a leak to fix: pinning is what makes pointer keys ABA-sound (an evicted
key's address could be reused by a new state, silently returning a wrong memoized state).
Accepted (user): a session is one transient parse, and most memoized states end up pinned
by the node tree anyway.
*Dependency:* `hashbrown` (std's own `HashMap` implementation, no_std) — the first
mandatory dependency; the dependency policy ([§dd-dr:minimal-dependencies]) amended
accordingly. Probes are
allocation-free (`Equivalent`-keyed lookup; the owned key is materialized only on
insert). Pitfall recorded: a **hash may never replace the stored key** — equality on the
key is what makes collisions harmless; a hash-only "key" would return a wrong state on
collision with no diagnostic.
Rejected alternatives: restructuring `ParsingStateDelta` for the memo's sake — the flat all-`Option`
struct is already the canonical sparse form (one slot per field, no ordering, no
duplicates), and a stored what-changed mask desyncs against the public fields and breaks
struct-literal construction (E0451).
*Companions (same session):* `PrefixTable::first_chars` **removed** (dead public API
describing the rejected maximal-run design, [§dd-dr:token-model]; premature to wire in
as a `match_at` guard — [§dd-dr:open-questions] item 1b can reintroduce a merged table
if profiling demands); `PrefixTable` reuse across derivations (see the
[§dd-dr:parsing-state] implementation notes); the benchmark harness
([§dd-dr:open-questions] obligation) consciously deferred, not dropped.
Revisit if: profiling shows memo overhead dominating on non-recurring deltas, or
per-parse memory growth hurts on pathological documents — eviction is unsound with
pointer keys, so that would need a different key design.

#### Optional-group arguments balance their delimiters [§dd-dr:optional-group-balancing]

Status: DECIDED (user; supersedes the LaTeX-style first-`]`-closes rule briefly shipped —
reversal recorded below).

`OptionalGroupArgumentParser`'s minted `GroupRule` is in force for the argument's
whole extent — the probing peek *and* the group's contents, not just the opening
delimiter: `\item[with[recursive[use]of]brackets]` parses as **one** argument whose
contents hold nested `[…]` group nodes. The two-sided child-descent rule that makes this
coherent: a nested group opened by the *minted rule* keeps the contents state (the rule
then rides the inherited states of deeper levels — that is what balances recursively),
while every **other** child descent — a brace group, an invocation — reverts to the
argument's own state, where `]` is an ordinary character: braces protect
(`[{arg with ]}]`, the [§dd-dr:nodes] designation example, unchanged), and an invocation's own
arguments inside the option see no bracket rule (`\item[\m{a]b}]`). This is exactly
pylatexenc's `LatexDelimitedGroupParserInfo.make_child_parsing_state` ("group with same
delimiter → keep contents parsing state; else → the outer, original parsing state"),
expressed through `ChildStateSpec` ([§dd-dr:child-state-spec]); `GroupParser` gained a per-use `with_child_states` config so the parser that
scopes a group's interior can steer its descents (default stays `Inherit`; decided
semantics 3 — one-level-deep policies — is untouched: this is per-use configuration at
the level that scopes it, not propagation). Shapes verified empirically against
pylatexenc 3.0a33 (`[with[recursive[use]of]brackets]` → identical node spans;
`[{arg with ]}]` → protected; `[ {a} ]` → three content nodes, no unwrap).
Rejected alternatives: LaTeX's first-unprotected-`]` rule (TeX delimited-parameter matching, xparse
`o`/`O` arguments) — implemented first for LaTeX parity and reverted on user review:
pylatexenc parity is the exit criterion, balanced matching is what document tooling
expects, and the brace-protection idiom survives either way.
*Pitfall recorded:* the protection policy rides one bracket level, as in pylatexenc —
whose depth-2 behavior already contradicts its own docstring (`\item[a[{x]y}b]` mangles
silently there: the nested group comes back childless; checked against 3.0a33). techy
mangled the same pathological shape *with* diagnostics — closed by
[§dd-dr:temporary-group-rules], which supersedes this entry's `ChildStateSpec` wiring
entirely.

#### Brace protection presupposes the close spelling is not a language group delimiter [§dd-dr:brace-protection-limits]

Status: DECIDED (user).

The revert-to-argument-state rule above protects `]` inside `[{arg with ]}]`
because the reverted state reads `]` as an *ordinary character*. If a language's
**base** rules class `[`/`]` as a genuine group pairing, the reverted state reads `]`
as a real close-only `GroupClose` — and `\item[{a]b}]` then **genuinely fails** (the
brace group surfaces `UnclosedGroup`/stray-close unwinding, with diagnostics), exactly
like `{a]b}` anywhere else in that language. Intended, not degradation: the revert
idiom restores the language's own reading; it never overrides the language. The clean
`\item[a]` case is unaffected either way (the minted rule is prepended, winning the
same-spelling tie in the contents state). Note the temporary-group-rules mechanism
(next entry) does not change this: stripping removes *temporary* rules, and the
bracket pairing here is a permanent base rule. Pinned by
`brackets_as_language_groups_defeat_brace_protection_by_design` (argument_parsers.rs).
Rejected alternatives: making the revert state actively suppress the close spelling — `]` would
then parse differently inside a brace group under an option than in every other brace
group of the same language: an inconsistency masquerading as robustness.

#### Temporary group rules: a state-scoped delimiter lifecycle at the derivation choke point [§dd-dr:temporary-group-rules]

Status: DECIDED (user; supersedes the optional parser's `ChildStateSpec` wiring — the
only vehicle that reaches depth N, since the outer `Arc` sits N frames up, unknowable to
the descending site, and caller-side descent policies are one level deep by design).

`TokenRules` gains **`temporary_groups`**, a second rules list that tokenizes
exactly like `groups` (same `enable_groups` gate; listed *first* in the `PrefixTable`,
so temporaries win same-spelling ties — the minted-rule "prepended wins" semantics) but
whose lifecycle is scoped in state data: `ParsingState::derived()` — after
`apply_overrides`, before `finalize_transition` — clears the carried-over temporaries
whenever the delta installs an `expecting_group_close` that is not one of the base's
temporary rules by `Arc` identity (`Some(None)` clears too; a delta that explicitly
overrides `temporary_groups` is exempt — the delta author spoke). Every group descent
passes through that derivation, so entering the temporary rule's own group keeps it
(nested `[…]` balance recursively) while entering any other group drops it for that
whole subtree: brace protection at depth N — `\item[a[b{c]}]]` parses correctly, beyond
pylatexenc. Nothing restores the rule after the inner group closes — scope reversion is
structural (`\item[a{b}[c]d]` pinned). Nested minted rules scope by the exemption:
`\item[\mm[x]]`'s inner parser *replaces* the temporaries for the inner argument's
extent.
*Sub-rulings:* **(i) stripping site**
= the `derived()` core rule, not `group_interior_state` delta construction: extensions
install expected closes through hand-built deltas (the `\verb`-idiom raw-block test
pattern), in-parse and out-of-parse derivations must agree (the session stays
data-equivalent to raw `derived()`), and the thin group-descent delta leaves the
derivation memo untouched — the rule is a pure function of `(base, delta)`, so identity
keying stays sound and memo hits return the already-stripped state. **(ii) encoding** =
the rules list, not a `transient` flag on `GroupRule`: temporariness is a property of
the rule's *installation in a state*, not of the rule value; the minted delta becomes
one `Arc` instead of a whole-`groups` clone, and the strip an `is_empty` check. **No
`Lang` gate for now** (user; reconsider later): Rust cannot const-gate a field's
existence without type gymnastics, non-use is already free (empty list end-to-end), and
infallible `derived()` has no `Err` channel for an unsupported-feature violation.
**(iii) `OptionalGroupArgumentParser` detaches from `ChildStateSpec`** (plain
`GroupParser` descent; the mechanism itself stays for genuinely per-level policies —
the chars-except-groups pattern). The group half was exactly data-equivalent to
inherit-plus-strip; dropping the invocation half (`Fixed(argument_state)`) is a
deliberate, narrow **divergence from pylatexenc** (user-accepted): an invocation inside
`[…]` now inherits the minted rule for its *non-group* token consumption (a bare `]` in
expression-argument position reads as a stray close, not a char), while its
group-delimited arguments protect via stripping at every depth (`\item[\m{a]b}]`
unchanged, re-pinned as `optional_child_invocation_brace_arguments_protect_by_stripping`).
To revisit when the preset argument-parser helpers are defined: a mandatory-argument
parser could reset `groups` to exactly the delimiters it wants to see, or reset the
temporaries itself, through its own deltas.
Rejected alternatives: a `Lang` callback recording a `StateExt` flag (a core parser's
parsing correctness must not depend on `Lang` cooperation; `finalize_transition` stays
reserved for genuine language semantics); session-side stripping (the session layer is
pinned data-equivalent to `derived()` and may never alter a resulting state).
*Pitfall recorded:* `temporary_groups` is a **prefix-table input** — `derived()`'s
table-reuse check compares it elementwise like `groups`; omitting that would reuse a
stale table across a strip and keep tokenizing the dead delimiters (pinned by
`temporary_group_rules_are_prefix_table_inputs`).

#### `parse_construct` and `probe_token` replace hand-rolled swap/restore [§dd-dr:parse-scoped]

Status: DECIDED (user).

The `cx.state` swap/restore protocol was correct at every one of its
seven lib sites, but the correctness was per-site discipline (restore **before** the
`?`), and the probe site had to hold a `Result` un-`?`-ed across the restore.
The scoped-parse entry point — the pylatexenc
`walker.parse_content(parser, …, parsing_state)` analog, deliberately on the *context*
(the session lacks tokens and source; the top-level drive later landed on `Language` —
[§dd-dr:language-parse-api]) — makes the restore structural. It is
**`parse_construct`**, the single normative descent
entry point (with an optional frame and the descent-guard check,
[§dd-dr:descent-guard]; `state: None` = clone the current state; the superseded
name `parse_scoped` is pinned in [§dd-dr:superseded-names]); the closure-shaped
scoping lives on as `with_parsing_state` — a state-scoping utility, not a descent
point. The returned delta stays
**unapplied** (the [§dd-dr:parsing-state]/[§dd-dr:parsers-engine] caller-applies law; an auto-applying driver would be wrong
for group interiors). Frame-opening stays separate (`with_frame` composes around it;
argument frames wrap two sub-operations, not one parse). The peek-shaped sites that are
not sub-parses are covered by `probe_token(&state)` — the former `try_peek` as a public
method, with the state now an **explicit parameter**: that is what dissolved the probe
site's swap entirely (it existed only because `try_peek` read `cx.state`), and it is the
public face of the argument-probe protocol (tolerant ⇒ `Ok(None)` without diagnosing or
consuming; unrecoverable or strict ⇒ abort). `ParserSession::snapshot_frames` went
public with it (custom parser code building its own `ParseError`s needs the traceback);
`push_frame`/`pop_frame` stay crate-private — `with_frame` remains the only stack
mutation path. It is ordering enforcement, not unwind safety: the crate is `no_std`, an
unwind tears down the borrowed context, and a `Drop` guard would be over-engineering.

#### No spec-side slots: slots are pure record-level vocabulary [§dd-dr:no-spec-side-slots]

Status: DECIDED (user, slots session; supersedes the same session's earlier
"slots mirror arguments" lean).

The mirror died on the **invocation-facts problem**:
body parsing needs facts only the running invocation has — the environment name for the
`\end{name}` back-reference, potentially the arguments parsed so far — which no
spec-declared per-slot parser list can receive through a `parse_argument`-shaped entry
point. pylatexenc is the confirming precedent: `EnvironmentSpec.make_body_parser(token,
nodeargd, arg_parsing_state_delta)` configures the body directly on the spec; there is no
slot-spec list anywhere in pylatexenc. So the callable spec's sanctioned parser (the
`make_invocation_parser` takeover) parses the body with whatever parsers it chooses to
drive internally and **directly populates the `ParsedSlot` records**. Arguments and slots
are the same thing at the **record** level (region + name + ext), not at the spec/parser
level — "slots" become pure node vocabulary. Consequences shipped together:
- `ParsedSlot` reshaped: `{ spec: Arc<SlotSpec<L>>, region, ext }` →
  `{ name: Option<Box<str>>, region, ext }` (user: definitely include the name; kept
  `Option` — environments may not bother, fence-block multi-slot constructs may name
  several). Self-describing records ([§dd-dr:nodes]) *preserved*, not weakened: standing alone now
  means carrying the name directly — the spec pointer bought nothing else, since
  `SlotSpec` had no other tool-visible payload. Deliberate asymmetry with
  `ParsedArgument`, which keeps its `Arc<ArgumentSpec>` (parser/name/delta are worth
  pointing at).
- The **slots trap disappears by construction**: with `slots()` gone there is nothing to
  declare that `StdInvocationParser` won't parse — its implementation-error arm and the
  pinned test are deleted ([§dd-dr:errors] consequence list amended).
- The body state delta (pylatexenc's `make_body_parsing_state_delta`) rehomes to the
  preset spec type that drives the parse — the preset's `EnvironmentSpec` holds it as an
  ordinary field, read back by its own composition (the test-lang `EnvSpec` rehearses
  this through the [§dd-dr:specs] `Any`-supertrait downcast). The core never interpreted it.
- `StdCallableSpec::new(arguments)` is single-list (a free ergonomics win); the guard's
  `!slots().is_empty()` clause is replaced by the spec-level emptiness method (next
  entry), which the removal makes *more* load-bearing.
- Composition building blocks promoted to `pub`:
  `parse_declared_arguments` (the shared argument half) and `read_rigid_name_group` (+
  `NameGroup`) — a `\begin`-shaped takeover now assembles from public parts. A
  `ParsedSlots`-assembly helper was judged unnecessary for now: what remains hand-rolled
  is a few lines of offset bookkeeping.
Where the standard `\begin` composition lives was left open deliberately and settled
later: preset-owned — cf. [§dd-dr:begin-composition].

#### The emptiness surface: `can_match_empty()` + `requires_content()` [§dd-dr:emptiness-surface]

Status: DECIDED (user; names user-decided; pylatexenc precedent:
`LatexParserBase.contents_can_be_empty`, consulted by its expression parser).
- `ArgumentParser::can_match_empty()` — can this argument be satisfied consuming
  nothing, i.e. is *absent* a valid outcome rather than a diagnosed recovery? Optional
  group and `*` marker: `true`; mandatory group and expression: `false`. (Name chosen
  over `contents_can_be_empty` — "contents" reads oddly for an argument *parser* — and
  over `may_be_absent` — "absent" is the record-level word.) **Default `true`**, the
  pylatexenc base-class polarity, chosen by failure-mode asymmetry: a custom mandatory
  parser that forgets to override merely loses the guard diagnostic (its callable
  dispatches fully and parses greedily in expression position — pylatexenc's own
  behavior), while a wrongly-`false` default would spuriously diagnose *valid* input.
  The standard optional parsers state the `true` explicitly (load-bearing, not
  incidental).
- `CallableSpec::requires_content()` — would this invocation, appearing bare as a
  single-token expression argument, be malformed? Default derives from the declarative
  surface: `arguments().iter().any(|a| !a.parser.can_match_empty())`. Negative polarity
  deliberately matches the derivation and the override ergonomics: a takeover spec that
  declares nothing but consumes plenty (`\begin`, `\verb`) overrides with a natural
  `true` — and with `SlotSpec` gone this method is the **only** channel for a
  body-bearing spec to say "I take material".
- The expression guard switches from `!arguments().is_empty() || !slots().is_empty()`
  to `spec.requires_content()`. Behavior changes pinned in tests: a callable taking
  only emptiable arguments is now *valid* bare in expression position and dispatches in
  full (`\frac\mymacro 2` with an optional-only `\mymacro` — pylatexenc parity; if the
  optional is provided, the bare callable swallows it, also pylatexenc); a bare
  `\begin` in expression position is *diagnosed* once the dispatcher spec overrides —
  a deliberate, documented divergence from pylatexenc, which dispatches the environment
  as the argument.
- The condition was renamed with the semantics: `ExpressionCallableTakesArguments` →
  `ExpressionCallableRequiresContent` (identifier
  `core.arguments.expression-callable-requires-content`), message "…it requires
  content (arguments or a body)" — the old "it takes arguments" would be a false
  message for a body-bearing takeover that declares none. (An implementation-forced
  naming consequence, user-signed-off.)

#### The `\begin` composition is preset-owned; core contributes building blocks [§dd-dr:begin-composition]

Status: DECIDED (user; settles the home question left open by
[§dd-dr:no-spec-side-slots]).
The standard `\begin`/`\end` composition (rehearsed test-side in
`environment_parser.rs`) rehomes to the latexlike preset: the preset
registers a `BeginSpec` dispatcher whose invocation parser contains minimal/no scanning
code of its own — it reads the rigid name group (`read_rigid_name_group`), resolves the
environment's spec from the state's libraries under the preset's ENVIRONMENT callable
type, parses declared arguments (`parse_declared_arguments`), drives the core
`EnvironmentBodyParser`, and assembles the callable node. The *notion* of "environment"
is preset property; core owns each individual parsing task as data-parameterized
machinery (the [§dd-dr:no-privileged-concepts] ground `EnvironmentBodyParser`'s core placement already stands on).
Consequences made explicit:
- **Invocation-level takeover is out; amending `Invocation` is declined.** An
  environment spec's own `make_invocation_parser` is never invoked (the
  `Invocation<'s>` composition finding stands as a permanent boundary, not a bug to fix); all
  per-environment variation flows through the preset's `EnvironmentSpec` surface.
  That surface is *not* constrained to plain data (user, same session — "spec-as-data"
  would overstate the ruling): behavior-shaped customization via defaulted methods is
  legitimate, per [§dd-dr:data-vs-traits] and the factory precedent of `make_invocation_parser` itself.
  The body-parsing choice (verbatim-like bodies) settled on the defaulted
  `make_body_parser()` method (pylatexenc's `EnvironmentSpec.make_body_parser`) over a
  plain field.
- **`EnvironmentBodyParser` keeps its name** (rename raised and reconsidered, user):
  its contract is hardwired to the rigid COMMAND + CHARS_GROUP terminator shape, and
  environments are the one role it is designed for — a generic name
  (`TerminatedBodyParser`) would over-promise arbitrary terminator conditions. Honest
  single-purpose labeling beats false generality; strata rule 2 constrains imports,
  not descriptive vocabulary.
- **`read_rigid_name_group` stays a value-returning scaffolding helper**, deliberately
  separate from the node-staging chars-group *argument* parser (pylatexenc's
  `LatexCharsGroupParser` analog) that the preset's standard library adds for `\label`-style
  chars-only arguments: scaffolding is reconstructed, never recorded ([§dd-dr:nodes]), so the
  name reader must not stage nodes — the two roles differ in kind, not configuration.
- Preset-side strays that rehome with the composition: registering an `end` spec so an
  orphan `\end` diagnoses well; the name reader's rigidity contract (trigger post-space
  tolerated and normalized away — `\begin {itemize}`) is a documented knob, not a
  behavior change.
Rejected alternatives: a core "read-marked-delimited-content" callable spec generalizing
environments (declared arguments + body up to constructor-specified terminator
syntax). The killing flaw is not lookup — a core spec could query a parameterized
`L::CallableTypeId` generically — but *interpretation*: the body delta and body
customization live on the preset's concrete spec type (the slots-session rehoming) and
are invisible through the core `CallableSpec` trait, so a core driver would need
spec-side body vocabulary back in core — exactly what the no-spec-side-slots ruling
rejected. No plausible second consumer either: fence-block-style constructs wire their
own parsers.
Revisit if: a second core-worthy consumer of command+name-group termination
materializes (the generalization/name question reopens), or `EnvironmentSpec`'s
body-customization design finds it genuinely needs invocation-level takeover.

#### Parser-library gap list settled; tack-on fields parse as a construct [§dd-dr:parity-gap-list]

Status: DECIDED (user, parser-library survey).

Key rulings and their reasons:
- **Tack-on information-field macros** (`\label` after `\section`, pylatexenc's
  `LatexTackOnInformationFieldMacrosParser`): a construct parser, *not* a
  postprocessing pass, for two reasons (user). First, postprocessing means tree surgery
  over siblings, where the parser gets the association for free at parse time —
  attaching the `\label` calls directly to the `\section` invocation node. Second,
  postprocessing forces `\label` to be a primary language command with defined
  attach-to-*something* behavior everywhere; recognizing it only where a spec requests
  tack-ons lets a language cleanly disallow that LaTeX quirk.
- **The `LatexMathParser` lesson**: presets need an easy *pluggable* way to attach an
  interior state change/event to a group class (math mode entering on `$…$`); the
  direction is a contents-parsing-state/state-delta plug. `ChildStateSpec` is not that
  mechanism — it is per-use call-site config and deliberately one-level-deep (decided
  semantics 3 above). The plug's shape settled on neither sketched candidate: the `ParseDriver`'s
  `group_interior_delta` hook returning a parsing-mode delta ([§dd-dr:parse-driver],
  [§dd-dr:first-class-mode]).
- **Ready-made argument-parser conveniences are wanted even where composition
  suffices** (user): a multi-delimited group parser (any of several delimiter pairs at
  one argument position — port pylatexenc's contents-state subtlety of keeping only
  default delimiters plus the encountered pair) and an embellishment parser (xparse
  `e{tokens}`-type), which subsumes generalizing `MarkerArgumentParser` beyond the
  single-literal `*` case. A node-staging chars-group parser is likewise wanted,
  deliberately distinct from `read_rigid_name_group`: the environment-name reader is
  value-returning scaffolding (reconstructed, never recorded, [§dd-dr:nodes]), while the
  chars-group parser stages nodes for `\label{…}`-style chars-only argument groups.
- **Comma-separated chars list**: discarded as a construct parser in favor of a
  split-at-chars read/extraction helper over parsed children (pylatexenc's own
  docstring recommends this route).
Revisit if: the tack-on parser's absorption of following siblings turns out to
interact badly with enclosing stop conditions in practice, or a preset's interior-state
plug proves to need more context than the group rule/class provides.

#### The deferred parity argument parsers landed [§dd-dr:parity-parsers]

Status: DECIDED (user).

The decisions and their reasons:
- **Embellishment record shape — one `ParsedArgument`, structure inside** (the survey's flagged
  question): per-embellishment-char `ParsedArgument` entries are structurally
  unreachable — source order is free (`\op_{b}^{a}`) while `parse_declared_arguments`
  runs one spec at a time in declaration order, so a `^`-spec already reported absent
  could never be revisited; expressing xparse's per-char slots would need a
  multi-record argument seam for this one consumer. Instead pylatexenc's shape: one
  classless wrapper `Group` per matched pair (`GroupData::untyped`, open = marker,
  close empty), content = the wrapper run, and by-marker access as a *read-side*
  helper (`extract::split_embellishments`). Per-char access thus costs one helper
  call, not an API change.
- **Embellishment matching semantics** (user): noise before a marker; between marker and
  expression, plain **whitespace only** (revised — the first cut allowed nothing; pylatexenc's
  `allow_pre_space` and TeX's `x^ 2` decided the relaxation).
  The pair stays atomic: a violated pair (`\op^` at EOF, a comment or paragraph
  break after the marker) rewinds *whole* and ends the run silently — a lone marker
  char is ordinary content nearly everywhere, so a diagnostic would misfire on
  legitimate input. The tolerated whitespace is staged *inside* the wrapper as its
  leading noise node and filtered out of `split_embellishments` values (which stay
  noise-free). Each marker at most once (xparse; pylatexenc's removal loop agrees);
  **longest match** among available markers — a deliberate divergence from
  pylatexenc, whose accumulate-until-equal check makes `'` permanently shadow `''`.
  Markers are `Char`-token sequences: a specials-claimed spelling does not match
  (state-dependent tokenization is the law — the latexlike `''` ligature outranks a
  `'` marker in text mode, not in math mode where the ligature is invisible).
- **The multi-delimited form folds into the existing argument parsers** (the user's own `### PhF` note:
  `Rules(Vec<…>)` supersedes the scalar `Rule`): `GroupArgumentParser::any_of` /
  `OptionalGroupArgumentParser::any_of`, no new type — pylatexenc's separate
  multi-delim class dissolves. The ported contents subtlety maps onto the
  temporary-groups lifecycle as **two derivations** (shared `probe_minted_group`):
  probe under all pairs as temporaries, contents under the matched pair only (single
  configured rule: one derivation, unchanged). Everything else falls out
  of the existing machinery: nesting = same-rule descent keeps temporaries, brace
  protection = other-rule descent strips them (at any depth — stronger than
  pylatexenc, which mangles depth-two shapes), and the base state's own rules staying
  live inside is the decided "stripping restores the language's own reading" rule.
  Word codes `AnyDelimited`/`AnyDelimitedOptional` are **list-form-only** factory
  elements (a compact string would read `A` as a code; pylatexenc too only spells
  them as whole `arg_spec` strings).
- **`CharsGroupArgumentParser` — restriction is contents-only, math off is
  data-driven, descent restores outer by default**: leading noise scans under the
  outer state (which is why the parser exists rather than
  `ArgumentSpec::parsing_state_delta` on a plain `m` argument — the spec delta covers
  the probe too, so a pre-`{` comment would kill the match with comments off).
  There is no math gate to switch: with nested groups on, the contents keep only
  group rules *of the entered class* — math pairs are another class and drop away
  purely data-driven; with nested groups off, `enable_groups` clears and the ungated
  expected close still ends the group (verbatim-recipe precedent) — pylatexenc's
  first-close-wins `enable_groups=False` behavior for free. Nested interiors restore
  the outer, unrestricted state by default (user, the
  `\cite{key:value,manual:{… \emph{Title} …}}` case: chars at level one, full
  richness in braced values) — carried by the `ChildStateSpec` chars-except-groups
  policy the child-state session anticipated; `with_restricted_descent` keeps
  chars-only at depth. No argument code (pylatexenc has none; programmatic wiring).
- **`TackOnFieldsArgumentParser` — an ArgumentParser staging real `Callable`
  nodes**: FLM's `label_arg` settles the integration (the tack-on parser is the
  callable's *last declared argument*; attachment = the argument's region, zero
  invocation-parser changes). Fields are configured `name → Arc<dyn CallableSpec>`
  pairs plus a `callable_type`; recognition never consults the scope stack (the
  decided no-`\label`-as-language-command reason survives), and dispatch routes
  through `ParseDriver::make_invocation_parser` — so the staged field node
  self-describes (spec, own `ParsedArguments`), frames and accessors work, and
  pylatexenc's group-wrapper hack is dropped. Two byte-keeping divergences (user):
  a repeated non-repeatable field is diagnosed (`RepeatedTackOnField`) **and kept**
  (pylatexenc parses-and-discards, which would break span tiling), and
  noise **between** fields is scanned as region noise (pylatexenc stops absorbing at
  a comment; techy's noise-ownership doctrine says scan), with a failed probe
  rewinding only its own scan. Multiplicity is per field
  (`with_field`/`with_repeatable_field`) — multiple `\label`s after `\section` are a
  legitimate, opt-in shape.
- **Both run readers return `KeyVals`** (user): `split_embellishments` (marker key /
  argument value) and `split_tack_on_fields` (field-name key / provided-argument
  content value) reuse the keyval result type wholesale — duplicate-preserving source
  order, last-wins `get`, `value_content` lone-group unwrap, `get_combined_with` —
  via the shared `finish_keyvals` tail. A field invocation providing no argument
  records no value (`None`), mirroring keyval's `draft` vs. `label=` distinction.
Revisit if: a consumer needs per-embellishment diagnostics for dangling markers
(`\op^` silently unmatching), or a field spec needs takeover-level access to the
absorbing invocation (the configured spec sees only its own invocation).

#### `ParseDriver`: parse behavior is a Lang-provided instance [§dd-dr:parse-driver]

Status: DECIDED (user).

New core trait `ParseDriver<L>`, every method defaulted but one (`StdParseDriver` = the
trivial impl carrying the `Recovery` knob), bound into the bundle as `Lang::Driver`;
`ParseContext` gains `driver: &'a L::Driver`. The field is concretely typed through `L`,
so preset parsers reach preset helper methods (a future `LatexlikeDriver::load_package`)
fully typed — no downcasts; generic code sees only the trait. The driver owns:
- **construct provision** — `make_nodes_parser` / `make_group_parser` / a
  `make_invocation_parser` interception defaulting to the spec's own factory. Every
  descent site (dispatch-loop GroupOpen arm, group interiors, environment bodies,
  argument parsers, top-level drive) routes through `ParseContext` wrappers
  (`cx.parse_nodes`/`cx.parse_group`), so one override applies everywhere — the
  "custom nodes parser" nuclear option becomes a supported seam;
- **the group descent-delta channel** — `group_interior_delta(prev, rule)`, pure per
  `(state, rule)`, merged into the memoized `session.group_interior_state` derivation
  (the cache stays in session; the hook runs on memo miss only). With [§dd-dr:parsing-state]'s parsing
  mode this closes the group-interior-state parity gap;
- **recovery policy** — `Recovery` leaves `ParserSession`, which returns to pure
  scratch/output (builder, diagnostics, frames, memo, `SessionExt`); overriding the
  driver's recover path admits richer policies than the strict/tolerant enum;
- **the migrated parse-time hooks** — `resolve_command`, `make_paragraph_break_node`,
  `refine_diagnostic` (folds into the recover path), `observe_transition`.
*The load-bearing placement doctrine:* `Lang` keeps hooks of layers callable outside or
below a driven parse — `initial_state_data`/`finalize_transition` (state layer:
`derived()` is out-of-parse-callable), `scan_specials`/`specials_trigger_chars`
(tokenizer layer), `finalize_node` (builder/transform layer). Everything that only runs
while a parse is driven lives on the driver — instance methods, so behavior can carry
configuration that static `Lang` hooks never could. Accepted asymmetry: specials
resolution stays `Lang` (token time); command resolution is driver (parse time).
*`ParseContext` doctrine:* cx returns to a data struct (tokens, state, session, driver —
no source handle, [§dd-dr:no-context-source]). Policy helpers (`recover`, `probe_token`) are defined on the driver with thin
delegating sugar kept on cx; invariant-bearing plumbing (`parse_construct` — the
single normative descent entry point, its frame folding absorbing the separate
`with_frame` composition at descent sites — `with_frame`,
`implementation_error`) stays as non-overridable cx methods — pairing invariants must
not be overridable. Every trait item is defaulted, `make_token_reader` included since
the language declares its tokenization ([§dd-dr:tokenization], [§dd-dr:token-reader-hook]);
the descent guard is engine-fixed, not a driver item ([§dd-dr:descent-guard]).
Rationale: the session-purity argument (user) — `ParserSession` is organized scratch
space, and a parser *provider* conceptually drives the parse; it was misfiled there, as
was `Recovery`. One seam for provision + one home for parse behavior + typed preset
helper access were unreachable from static hooks or a session field.
Rejected alternatives: a session-installed `dyn` provider (a second customization surface beside
`Lang`; preset helpers invisible behind the trait object — Any-funnel required); more
static `Lang` hooks (no instance configuration; `Lang` was accreting parse behavior
foreign to its layers); overridable `parse_scoped`/`with_frame` (invariant footgun).
*Cost accepted:* every existing `ParseContext`/`ParserSession::new(recovery)` call site
updated — mechanical but broad.
Revisit if: a real consumer needs runtime driver swapping for one `Lang` (add a dyn
override on top of the associated-type default), or per-invocation `Box` provision shows
up in profiles (the [§dd-dr:open-questions] benchmark obligation).
In-flight decisions (user-checkpointed):
**Module home `engine::driver`** (user choice over constructs); `CommandResolution`/
`ResolvedCallable` relocated there next to `resolve_command` (crate-root re-exports
keep `techy::…` paths; module paths changed `state::` → `engine::`). **The
group-interior memo is a second, dedicated session map** keyed `(base, rule)` by `Arc`
identity: "hook runs on memo miss only" needs a pre-hook probe key, and sharing the
rules-only memo would let a hand-built expecting-close delta collide with a driver-augmented
descent under one key (unsound). Entries store the *merged* delta so
`observe_transition` sees the true delta on hits; keying on `(base, rule)` also keeps
descents deduplicated when the driver's delta carries events/ext (sound — the hook is
pure per `(base, rule)`). The canonical `expecting_group_close` is forced *after* the
driver's delta merges: the descent invariant is not driver-displaceable. **The
memoized helpers stay non-overridable `ParserSession` methods taking `&L::Driver`**
(user-confirmed after discussion): the memo is per-parse mutable state — driver-hosted
it would need hot-path locking, retain `Arc`-pinned entries across parses, and leak
derivations between concurrent documents ("define once, parse many") — and memo
soundness + observe-on-every-transition are invariants, not policy, so they must not
be trait-overridable; `cx.derived_state(&delta)`/`cx.group_interior_state(&rule)`
sugar (base = `cx.state`, the only shape call sites use) hides the driver parameter.
**Box-per-descent accepted** for the `make_nodes_parser`/`make_group_parser` defaults
(uniform with the per-invocation Box; [§dd-dr:open-questions] benchmark covers it; a fast path could hide
behind the same cx wrappers later). `ParserSession::new()` takes no arguments
(`Default` added); `ParserSession::recover` takes the policy per call — the channel a
custom driver `recover` uses for per-condition decisions. The default
`resolve_command` detail now names `ParseDriver::resolve_command`.

#### `make_token_reader`: the driver's per-instance reader override [§dd-dr:token-reader-hook]

Status: DECIDED (user).

`ParseDriver::make_token_reader(&'s self, source) -> Box<dyn TokenReader<'s, L> + 's>`
builds the reader for a parse, and both construction sites go through it — the root parse
and each attached (included) source — so one implementation covers a whole parse,
inclusions included. The language fixes the *types*, and names the reader, through
`Lang::Tokenization` ([§dd-dr:tokenization]); this hook supplies the *instance*, and may
hand it configuration the driver holds. The `make_*` spelling is the factory-hook naming
rule ([§dd-dr:naming]).

It is **defaulted**, like every other `ParseDriver` item: the default body is
`L::Tokenization::make_token_reader(source)`, and the standard one-liner
`Box::new(StdTokenReader::new(source))` lives once, in `StdTokenization`'s impl. Overriding
is for a reader that needs data only the driver instance has; a reader needing none belongs
on the language, as its `Tokenization`.

Rejected alternatives: a reader argument on the parse entry point (an attached source
builds its reader mid-parse, where such an argument never reaches); a reader field on the
session (a reader borrows the source it scans, and one parse builds one reader per source
— the session is the wrong lifetime and the wrong multiplicity).

*Reversal note (2026-08-18, user).* This entry previously recorded the hook as the one
`ParseDriver` item **without** a default, and rejected "a factory on `Lang`" on the grounds
that it would put an instance where `Lang` declares data types and would land the cost on
every `impl Lang`. Both are superseded by [§dd-dr:tokenization]: the factory is *static*, so
no instance lands on `Lang`; it lives on a separate bundle type, so the cost is one
associated type per `impl Lang` — replacing two — and the reader stops being pinned by the
driver. The old "revisit if Rust gains specialization" clause is likewise void: the default
body needs no specialization, only the projection through the bundle.

Revisit if: a reader must be chosen per *parse* rather than per language or per driver —
the hook takes `&self` and the source, and nothing else.

#### `ScopesResolvingDriver`: the canned command-resolving driver component [§dd-dr:scopes-resolving-driver]

Status: SUPERSEDED (user, API review) — the component struct is
replaced by a pluggable strategy parameter on the one canned driver:
[§dd-dr:command-resolver]. The on-ramp analysis below (core cannot default
`resolve_command`; the command-type field is the missing datum) carries over
unchanged — only the packaging changed.

A core-provided driver component closes the last on-ramp gap a trait default
cannot: `ScopesResolvingDriver<L: Lang> { recovery: Recovery, command_type:
L::CallableTypeId }`, whose `resolve_command` is a one-line delegation to
`resolve_command_in_scopes(state, token, self.command_type)`
([§dd-dr:resolution-extraction]); everything else stays trait defaults. Core
cannot default `resolve_command` itself — it cannot conjure the language's command
`CallableTypeId`; the field *is* the missing datum (contrast specials, where the
provider supplies the resolved type with the match). The language-designer
walkthrough's entire hand-written driver was literally this expression plus the
recovery knob. It is a component, not a shortcut tier: `StdParseDriver`'s sibling
with one more field, constructed from real inputs; a language outgrowing it writes
its own `ParseDriver` — the normal path, nothing abandoned. Home: the engine hub,
beside `StdParseDriver` (which keeps its pure-recovery test-carrier role;
`StdParseDriver::default()` is removed — [§dd-dr:language-init] amendment).

Rejected alternatives: defaulting `resolve_command` to scope resolution
(impossible without the command-type datum, above); the names
`ScopeResolvingDriver` (user chose the plural — the stack is "scopes"),
`ScopesDriver` (vague), `StdScopeDriver` (`Std` adds nothing).

Revisit if: languages with several command-syntax callable types appear (a
single-field component then under-serves; today they write their own driver).

#### `CommandResolver`: the pluggable resolve-command strategy on `StdParseDriver` [§dd-dr:command-resolver]

Status: DECIDED (user, API review; supersedes
[§dd-dr:scopes-resolving-driver]).

Command resolution plugs into the one canned driver as a strategy value instead of
shipping one driver struct per behavior: `trait CommandResolver<L: Lang>:
fmt::Debug + Send + Sync` (a single method mirroring
`ParseDriver::resolve_command`), carried by **`StdParseDriver<R = ()> { recovery,
command_resolver: R, source_resolver: Option<Arc<dyn …>> }`**. `()` implements the
trait as "resolves nothing" (inheriting the hook default's helpful
not-implemented detail message); `ScopesCommandResolver { command_type:
L::CallableTypeId }` — home `core::specs`, beside `resolve_command_in_scopes` and
the resolution family it packages — is the one-line scope-stack delegation the
superseded ruling shipped as a whole struct. Decisive reasons: (1) one driver
struct instead of near-identical siblings each duplicating the recovery knob and
the [§dd-dr:input-wiring] source-resolver field; (2) the defaulted-`()`-type-
parameter idiom is established house style (`NodeTree<L, A = ()>`,
[§dd-dr:node-annotations]; the `()` invocation-syntax impl,
[§dd-dr:invocation-syntax]); (3) `HashMap`/`RandomState`-precedented ergonomics —
`type Driver = StdParseDriver` and `StdParseDriver::new(Recovery::Strict, ())`
stay annotation-free.

Constructor doctrine (user): `new(recovery, command_resolver)` — the command
resolver is a mandatory by-value argument (`R` inferred; no `Default`/`Clone`
bounds anywhere); the source resolver initializes `None` and is set via the
chainable `with_source_resolver(…)` builder taking a sealed-conversion argument
(a by-value resolver is `Arc`'d internally, a pre-made `Arc` passes through — the
[§dd-dr:registration-ergonomics] Arc-removal idiom; renames
[§dd-dr:input-wiring]'s `with_resolver` builder now that two resolvers coexist on
one struct). Fields stay `pub`.

**The ruled asymmetry — storage matches the consumption seam** (documented in
rustdoc and at the field pair, per user directive): the command resolver is part
of the language *definition* (fixed when `type Driver = …` is written), consumed
monomorphized through the concretely-typed `ParseContext::driver` on the
per-command-token hot path — a generic parameter is collected in full. The source
resolver is an *embedding-environment* capability (varies per deployment/run),
consumed only through the type-erased `ParseDriver::source_resolver` accessor on
the once-per-`\input` cold path — a generic parameter there would be erased at
its only point of use, while costing a none-placeholder type and `None`-inference
noise. Proliferation guard (recorded): `resolve_command` is the ONLY hook that
gets a strategy seam — it is the sole `ParseDriver` hook that is both
non-defaultable for a real command-bearing language and has more than one canned
behavior worth shipping; no other hook grows one.

Rejected alternatives: the two-component shape (superseded above — duplicated
knobs, one more public type); `NoCommandResolver` as the no-op (a named unit
where house style says `()`; one more name frozen forever); both resolvers
generic (the erased-at-the-seam argument above); a three-argument
`new(recovery, command_resolver, source_resolver)` (a bare `None` third argument
fails type inference under a generic parameter — the setter shape never spells
`None` at all).

Final type shape: `StdParseDriver<R = (), O: SourceOrigin = Option<String>>`.
The second defaulted parameter exists because the
`Option<Arc<dyn SourceResolver<…>>>` field needs the origin type while
`type Driver = StdParseDriver` must stay annotation-free (decisive reason 3; the
impl is `impl<L, R: CommandResolver<L>> ParseDriver<L> for StdParseDriver<R,
L::SourceOrigin>`; a standalone binding needs the alias-defaults annotation —
`let d: StdParseDriver = StdParseDriver::new(Recovery::Strict, ())`). All three
fields are `pub` ("fields stay `pub`"), so struct-literal construction is
possible; `new()` + `with_source_resolver` is the intended path.

Revisit if: languages with several command-syntax callable types appear (they
write a custom `CommandResolver` — the point of the seam), or a second hook
genuinely meets both proliferation-guard criteria.

#### Takeover staging sugar: `disable_all`, collection constructors, a committed invocation helper [§dd-dr:takeover-staging-sugar]

Status: DECIDED (user, API-review session).

Three shorthand rulings on the takeover-parser ceremony — all shorter spellings of
the same operations (the [§dd-dr:registration-ergonomics]
shorthand-not-second-path principle):

1. **`TokenRulesOverrides::disable_all()`** — the overrides value setting every
   present feature's block to its `disable()` value: the raw-state block every
   rest-of-line and verbatim-like parser hand-builds. `disable_all()` means
   *every* feature off — the six gated blocks flip `enabled: Some(false)`;
   `forbidden_chars` has no gate, so its off is its inactive data, the empty
   forbidden set (`ForbiddenCharsOverrides::disable()`, `chars: Some("")`; the
   verbatim consequence — an outlawed character reads as raw content — is
   recorded at [§dd-dr:verbatim-family]). Lives on the overrides type so it
   composes — `verbatim_state_delta` itself becomes `disable_all()` plus its
   terminator (one source of truth), and parsers tweak fields afterwards.
   Feature-aware by construction — it mentions exactly the features the
   language declares present, and can never fail ([§dd-dr:lang-features]).
2. **`ParsedArguments::new(Vec)` / `ParsedSlots::new(Vec)`** — discoverable
   constructors for what only `From<Vec<_>>` impls provided (the walkthrough
   found them by grepping, not on the types' doc pages); the `From`s stay as
   conversion plumbing.
3. **The canned invocation-staging helper** — a `ParseContext` method
   wrapping the one staging door (`cx.stage_node`, [§dd-dr:ext-minting]):
   `cx.stage_invocation(&invocation, arguments: ParsedArguments<L>, slots:
   ParsedSlots<L>, children: Vec<BuildId>, end: Option<&L::StreamPosition>) ->
   ConstructParserResult<L, BuildId>` —
   builds the `CallableData` (four of its seven fields are transcriptions from
   the invocation and trigger), computes the node span, mints the
   invocation-syntax payload via `FromInvocation`, stages, returns the id.
   `end: None` = the standard rule — the last staged child's span end when that child
   lies in the trigger's source, otherwise the current stream position; `Some` serves
   takeovers whose consumed extent outruns their last child (rest-of-line, heredoc
   shapes). **No
   `callable_type`/`name` overrides**: the helper is the transcription-case
   shorthand only — the environment takeover overrides both and its span outruns
   its children ([§dd-dr:environment-scaffolding]), so environment-class
   composition stays on the canonical `cx.stage_node` door (in-crate:
   `StdInvocationParser` and the tack-on parser sit on the helper; the
   environment parsers stay on the door). Parse-side/restage-side symmetry is by
   **vocabulary, not arity** ([§dd-dr:restage-ops]): the parse side passes
   caller-tiled records plus a flat child list, the restage side driver-tiled
   bundles — who owns the region arithmetic differs by design, and the two
   signatures are not to be "unified". No ext/annotation parameters (`stage_node`
   mints the ext; parse annotations are `()`).

Rejected alternatives: `all_off()`/`raw()` for the overrides constructor (the
crate's off-vocabulary is "disable"; "raw" too clever); a terminator-less
`verbatim_state_delta` sibling (the overrides constructor composes instead of
multiplying delta helpers).

Revisit if: the staging-door shape itself changes (the helper follows it).

#### `override_all`: the wholesale-install counterpart of `disable_all` [§dd-dr:override-all]

Status: DECIDED (user, API-review session).

**`TokenRulesOverrides::override_all(&TokenRules<L>)`** — the overrides value setting
every present feature's block to its new per-block `override_all(&block)` value, so
applying it makes the target's rules equal to the source's. The counterpart of
`disable_all()` ([§dd-dr:takeover-staging-sugar]) along the same axes: feature-aware by
construction, infallible, and living on the overrides type so it composes with a
struct update. It is what a party holding a rules value it wants installed elsewhere
hand-builds — the shape `exit_math_context_delta` spells out by hand
([§dd-dr:enclosing-state-stack]).

**The two transient group fields are never carried**, at both levels: `temporary` and
`expecting_close` stay `None` in `GroupOverrides::override_all` and therefore in the
whole-value constructor. This generalizes the math-exit ruling — they describe in-flight
structural expectations of one live parse position, not lexical context — into a single
family rule, and it keeps the whole-value constructor exactly the composition of the
seven block constructors (a `GroupOverrides::override_all` carrying all four fields
would force the whole-value one to un-set two of them, so the same name would mean two
things one level apart). A parser that wants an expected close installs it on top:
`GroupOverrides { expecting_close: Some(Some(rule)), ..GroupOverrides::override_all(&r) }`
— the shape verbatim staging already writes over `disable()`.

Consequence accepted: the exhaustive-literal tripwire of the hand-built sites (a new
field in any block breaks their build until a carry-or-exclude decision is made) does
not extend to `override_all`, which silently carries a new field. The gain — one
audited definition of "install these rules" — was judged worth it; `exit_math_context_delta`
keeps its hand-written literal and its tripwire.

Rejected alternatives: `set_rules()` (mutator verb for what is a constructor, and
"rules" restates the type name), `set_all()`, `from_rules()` (idiomatic but silent
about *all* fields being set — the point of the constructor); a group block carrying
all four fields (above); keeping the seven block constructors private or omitting them
(they are what makes the documented struct-update idiom work for the new base, and the
per-block `disable()` family already sets the precedent).

Revisit if: a caller genuinely needs a verbatim group-block copy including the
transients — the block constructor would then grow an explicit second spelling, never
a changed `override_all`.

#### `\input` engine wiring: driver resolver accessor + the `parse_attached_source` door [§dd-dr:input-wiring]

Status: DECIDED (user, API-review session; realizes [§dd-dr:input-attachment]).

- **Resolver surface**: defaulted accessor `ParseDriver::source_resolver(&self) ->
  Option<&dyn SourceResolver<L::SourceOrigin>>`, default `None` ("this language
  resolves nothing"); shipped drivers carry an `Option<Arc<dyn …>>` field + the
  chainable `with_source_resolver(…)` builder ([§dd-dr:command-resolver]).
  Consequence ruled consciously: the field drops `Copy`/`Eq`
  on resolver-carrying drivers (nothing relied on driver `Copy`; strikes the
  keep-`Copy`/`Eq` clause of [§dd-dr:preset-driver-pillars]). The
  behavior-method variant (a `resolve_reference` hook) lost: carriers still need
  the storage field, and the accessor keeps stored-object composition.
- **The door**: `ParseContext::parse_attached_source(source, state, parser) ->
  ConstructParserResult<L, Vec<BuildId>>` — the *caller supplies the construct
  parser* driving the sub-parse (user amendment; `\input`-style inclusion passes
  the root nodes-parse shape). Internals: a fresh inner context (the outer
  reader's lifetime pins the outer source), same session/builder (`BuildId`s are
  session-global), local stray-close recovery (an included file's stray `}` never
  unwinds the includer), a traceback `Frame`. The door stages content nodes only —
  slot assembly stays the invocation parser's job (the one-staging-door doctrine
  holds). Resolution stays OUTSIDE the door (accessor → free
  `resolve_source_reference` →
  door), so caching frameworks substitute either half; the free fn is the
  canonical composition.
- **`attach_source_reference(cx, reference, at, state, parser)`** (core, beside
  the door): the resolve-diagnose-attach bundle — kept despite its size as the
  single raising site for the two failure conditions, so diagnostics wording is
  uniform across every `\input`-variant spec and framework. Conditions:
  **`NoSourceResolver`** (`core.sources.no-resolver`) and
  **`UnresolvableSourceReference`** (`core.sources.unresolvable-reference`,
  payload: reference + the `ResolveError` — `Clone` again per the
  [§dd-dr:resolver-contract] amendment). The family's third condition,
  **`InvalidSourceReferenceArgument`** (`core.sources.invalid-reference-argument`),
  is defined beside them but raised by the invocation parser that *reads* the
  reference argument, whose content must be plain characters
  ([§dd-dr:span-tiling]): anything else is diagnosed at the argument's span and
  no resolution is attempted.
- **`Language` collapses**: `with_resolver`, `resolver()`, and
  `Language::resolve_source` leave — completing [§dd-dr:language-init]'s expected
  surface (`new(driver, initial_state)` + `parse` + `parse_source` + accessors).
- **Door signature details**: the parser parameter is `&mut P where P:
  ConstructParser<L, Output = NodesOutcome<L>> + ?Sized` — the ruled return plus
  the local stray-close recovery require the nodes-run outcome vocabulary. The
  door returns **`AttachedSourceOutcome<L> { nodes: Vec<BuildId>, after_effects:
  Option<Box<ParsingStateDelta<L>>> }`**: `NodesOutcome` exports the merged
  record of the sibling after-effect deltas the run applied, each component
  recorded in its **effective, as-applied** form — context-dependent events
  lowered into their override patches before recording — merged last-writer-wins
  per field with scope ops (and any context-free events) concatenated in
  application order; the door merges across resumed runs. Both `after_effects`
  channels are boxed ([§dd-dr:descent-guard]): the door's frame stays live
  across the whole nested include parse, and moving the already-boxed
  `NodesOutcome` record into the bundle removes an unbox/rebox round trip on the
  persist path; the surfaces ruled NOT boxed — the driver hooks
  (`group_interior_delta`/`resolve_state_event`),
  `ArgumentSpec::parsing_state_delta`, `EnvironmentBehavior::body_state_delta` —
  are consumed in frames that unwind before recursion descends, so boxing them
  buys no per-level stack. `attach_source_reference` is a `ParseContext` method
  returning `Option<AttachedSourceOutcome<L>>` (`None` =
  diagnosed-and-recovered, nothing attached); `NoSourceResolver` carries the
  `reference`.
- **The preset construct is opt-in, never preloaded**: the public
  `InputMacroSpec<LLL>` (the `MacroSpec` pattern), constructor
  **`latexlike::input_macro_spec::<LLL>(persist_state: bool, attached_slot_ext:
  SlotExt<LLL>)`** — both parameters mandatory, embedders decide consciously (an
  always-on `\input` under a
  resolver-less driver would just diagnose every use); embedders insert it into
  their own package. `persist_state: true` forwards the bundle's merged delta as
  the invocation's own after-effect through the existing sibling channel (the
  preamble-defines-macros case; nested inclusions compose outward); `false`
  keeps the transparent behavior. The attached slot is named `"attached"`,
  `Attached` on the role axis; the shipped spec **mints no body-ness** — the
  slot's ext is the embedder-supplied constructor value, cloned per invocation
  (the preset recipe passes `BodyMarker::not_body()`; a body-marked ext remains
  a framework option `body()` finds — [§dd-dr:slot-roles] — never the shipped
  default). Its body is the brief form the helpers exist for — argument
  text → `attach_source_reference` → `Attached` slot — so `\input[options]{file}`
  / `\input*{f1,f2,f3}` variants are easy custom-spec work (the form-specific
  parts stay in the spec).

Rejected alternatives: resolver as a per-parse argument (re-litigates the ruled
direction, and the construct parser mid-descent holds only `cx`);
`cx.parse_source` as the door name (collides with `Language::parse_source` under
a different contract — sibling-vocabulary rule); a core-generic
resolve-then-attach *argument parser* (speculative before a second consumer — the
door + bundle are the reusable parts).

The free resolve-and-diagnose composition is named
**`resolve_source_reference`** (user: the fn drives the bookkeeping around a
*delegated* resolution; the name uses the ruled "source reference"
vocabulary — family: `attach_source_reference`, `UnresolvableSourceReference` —
and the resolver parameter carries the delegation visibly).

Revisit if: a framework needs several resolvers per driver (the accessor
signature admits dispatch behind it), or a
splice-a-cached-parse affordance changes the caching-framework route.

#### `Language<L>` + `parse()`: the runtime bundle's landed surface [§dd-dr:language-parse-api]

Status: DECIDED (user; four API-shape decisions on the long-deferred runtime bundle —
staged deliberately: `ParserSession` alone shipped first, `Language` only once consumers
demonstrated the need).

`Language<L>` = `{ driver: L::Driver, initial_state:
Arc<ParsingState<L>> }`, long-lived, owning no
per-parse state ([§dd-dr:stateless-language]).
- **Entry points are two named methods, not a `SourceInput` enum** (rejecting an older
  sketch's `parse(impl Into<SourceInput>)`): `parse(content: impl
  Into<String>)` mints an anonymous `Source`; `parse_source(Arc<Source<O>>)` takes a
  pre-minted source (origin/provenance intact — the `resolve_source_reference` round
  trip feeds
  it). A conversion enum whose only job is overloading buys one method name at the
  price of a public type; named methods are self-documenting.
- **Construction seeds from `Lang::initial_state_data()` and customizes by deriving**:
  `new(driver)` + fallible `with_seed_delta(delta) -> Result<_, DeriveError<L>>` (the
  sanctioned seed-customization path — runs `finalize_transition`, so language
  invariants hold over customized seeds; fallible since scope ops can fail; a failing op
  drops the bundle under construction — an embedder build-time bug, not a source
  condition) + `with_resolver(…)` (default `NoResolver`); `Default` where `L::Driver:
  Default`. Wholesale `StateData` replacement deferred until a consumer demonstrates
  the need. The `Lang` hook remains the seed source for `Language`-less parses.
  *(Construction surface since revised — [§dd-dr:language-init]: the initial state is a
  mandatory `new` argument; the `new(driver)` + fallible-customizer shape described in
  this bullet is superseded.)*
- **The advanced path is accessors, not a `session()` method**: `initial_state()`/
  `driver()`; the sketch's `session()` dropped — `ParserSession` carries no `Language` borrow and `ParserSession::new()` is
  argument-free, so a `Language::session()` would return exactly that (misleading
  discoverability sugar). `ParseResult` likewise stays borrow-free (nodes are
  self-contained; results outlive the bundle).
- **The root drive loop promotes the rehearsed pattern** (the `nodes_parser` test
  `root_driver_skips_a_stray_close_and_continues`): loop `cx.parse_nodes` under
  `StopSpec::none()` (through the driver factory — the uniform-routing contract covers
  the top-level site); on `UnexpectedGroupClose` diagnose the new core
  condition **`StrayGroupClose { delim }`** through the recover funnel, consume, and
  resume; on `EndOfInput` stage the root `List` over `SourceSpan::entire` and
  `finish()`. The condition lives in `constructs::nodes_parser` **next to
  `StopCause`** (user choice over engine-side placement): the stop cause announces
  the situation, and custom root drivers driving `NodesParser` directly reuse the
  condition without importing from `engine`. Reusing `UnclosedGroup` was rejected —
  its data shape (`expected_close`) describes an *open* group's missing close, which
  doesn't exist at the root. `TokenCondition`/`NodeCondition` stops at the root (no
  conditions were set) are nodes-parser contract violations → implementation error,
  aborting under any policy; `finish()` failures map through `ImplementationError`
  likewise. Each tolerant stray-close skip runs **under the loop's evolved state**:
  `NodesOutcome<L>` exports the segment's exit state (`state:
  Arc<ParsingState<L>>`), the root threads it into its ambient `cx.state` — so the
  recover funnel (and any `refine_diagnostic`) and the resume both see the state the
  loop actually reached — and the diagnosed delimiter is the stop span sliced from
  the source, never a re-peek.

Follow-up fix (review-driven): the recorded quirk — each tolerant resume re-entered
under the frozen **seed**, with the stray token re-peeked under it — fired all three
of its latent failure modes once after-effect deltas became reachable at the root
(agent code review; tests
`engine::language::a_stray_close_of_a_delimiter_a_sibling_delta_added_recovers_tolerantly`
and siblings). A close only the evolved state knows (`>` added by a sibling's delta)
re-tokenized under the seed as a plain char, misfiring the `GroupClose` contract
check into a spurious `ImplementationError` abort even in tolerant mode; a
seed-known shorter delimiter (`]` where the evolved state matched `]]`) was reported
and skipped wrong, surfacing a second spurious stray; and definitions established
before the skip were silently rolled back across it. Two-part fix. (1)
`NodesOutcome` became `NodesOutcome<L>` and exports the loop's live state at the
stop — the sanctioned channel for callers that resume content at the stop position;
the pass-through **delta** channel stays `None` (the exit state is finalized data —
returning merged deltas instead was rejected because the root would re-derive,
re-running fallible scope ops and re-firing `observe_transition` on a second,
divergent path). (2) The root's re-peek is gone: `StopCause::UnexpectedGroupClose`'s
span covers the delimiter exactly as matched — the tokenizer *defines*
`GroupClose::delim` as the span's slice — so the root slices the source at the span.
Carrying a `delim: String` on the stop cause was considered and rejected as a
redundant copy that could disagree with its own span; re-peeking even under the
correct evolved state was rejected as re-tokenization whose correctness silently
rests on reader determinism. Pitfall worth remembering: "recover under the right
state" includes the **diagnosis**, not just the resume — `ParseDriver::recover` and
`refine_diagnostic` receive `cx.state`, so the root must thread the exit state
*before* funneling the condition. Revisit if: a new non-root caller resumes content
at a stop position — it must thread `outcome.state` the same way, not its own copy
of the entry state.

Second follow-up (acceptance-gate driven): the skip's remaining quirk — the consumed
delimiter's bytes were dropped from the tree, an "accepted tolerant byte-accounting
break" — fell to the acceptance suite's invariant gate on its first
real document: any tolerant root recovery (direct, or reached through an
environment-body unwind, as in the ported pylatexenc `test_errors` document)
produced a tree failing `check_tree_invariants`' partition check, colliding with the
exit criterion "invariants clean on every acceptance parse". The skip now
**stages the consumed delimiter as a `Chars` node** under the loop's evolved state —
the markup-in-chars recovery artifact every other tolerant recovery already produces
(orphan `\end`, malformed `\begin`, forbidden characters) — so the root partition
holds across skips. Exempting recovered parses from the invariant was rejected: the
partition is the byte-accounting contract exactness consumers (`NodeSlice::span`)
build on, and a "recovered trees are less true" carve-out would silently spread to
every downstream walker.

#### `Language::with_provider`: push-a-provider seed sugar [§dd-dr:with-provider]

Status: DECIDED (user).

The dominant seed customization — "define a package, add it to the language" — gets a
first-class spelling: `with_provider(provider)` ≡
`with_seed_delta(ParsingStateDelta::new().push_provider(provider))`, fallible like the
derive path underneath. Promoted from the preset's test support under the dedup mandate (genuinely multi-purpose helper code becomes public API instead of being
duplicated into the integration-test crate).
Rationale: the delta spelling buries the everyday operation under two concepts
(delta + scope op); every guide example and suite fixture reads better as one call.
Rejected alternatives: an infallible signature via `expect` ("Push cannot fail today") — fragile
against future push semantics and against whatever `finalize_transition` does in the
derivation; the `Result` mirrors `with_seed_delta` honestly.
*(Superseded — [§dd-dr:language-init]:
`ParsingState::lang_initial_with_packages` is the infallible spelling of the same
everyday operation at the seed itself, where no derivation runs; `with_provider`
and `with_seed_delta` are removed.)*

#### Language construction: explicit initial state, seed+packages path [§dd-dr:language-init]

Status: DECIDED (user, API-review policy session; supersedes the
construction bullet of [§dd-dr:language-parse-api] and [§dd-dr:with-provider]).

`Language::new(driver, initial_state)` takes the initial `ParsingState` as a
**mandatory** argument; `ParsingState::initial()` is renamed **`lang_initial()`** (it is
the *Lang's* notion of the initial parsing state), joined by
**`ParsingState::lang_initial_with_packages(vec![…])`**, which constructs the seed with
the given packages/scopes pre-pushed. Canonical initialization:
`Language::new(LatexlikeDriver::new(recovery),
ParsingState::lang_initial_with_packages(vec![…]))`.
Decisive reasons: (1) crafting the initial state must not *require* the delta
machinery — everyday setup routes through plain construction; (2) the path is
**infallible**, verified against the state model: the seed never ran
`finalize_transition` anyway (its coherence is the language author's contract — see
`ParsingState::initial`'s docs), and pushing providers directly into the seed's scope
stack involves no by-name scope ops (the derive path's only failure source); the
transition choke point ([§dd-dr:state-option-c]) is untouched — packages-at-seed is not
a transition, and `freeze` rebuilds the derived caches from the augmented data; (3) **no
shortcut accessors** that users must abandon the instant they need one more option — the
constructor asks for the real inputs, kept cheap by the two `lang_initial*` helpers.

Consequences (the full collapse): `with_provider` and `with_seed_delta` are
removed — seed customization moves *before* construction (the delta idiom is
`Language::new(driver, ParsingState::lang_initial().derived(&delta)?)`); the
resolver surface (`with_resolver`, `resolver()`, `Language::resolve_source`)
leaves with the resolver's move to the driver ([§dd-dr:input-wiring]); `Default
for Language<L>` is **removed** (it reintroduces the implicit seed by the back
door, and the turbofish spelling was itself walkthrough friction), as are
`LatexlikeDriver::default()` and `StdParseDriver::default()` (strict-vs-tolerant
is the driver's one policy knob — it must be explicit; after the `Default for
Language` removal no `L::Driver: Default` consumer remains — the spelling is
`StdParseDriver::new(Recovery::Strict, ())`). The surface is
`new(driver, initial_state)` + `parse` + `parse_source` + accessors. The
packages argument takes the sealed `IntoSpecsProvider` conversion —
`lang_initial_with_packages([minidefs::minilatex_package(), my_pkg])`, no Arc
noise ([§dd-dr:registration-ergonomics]).

(Later revised, [§dd-dr:hook-fallibility]: `Lang::initial_state_data` may refuse
a bad seed, so the `lang_initial*` constructors return
`Result<_, FinalizeError>`. Decisive reason (2) survives narrowed — the packages
push itself involves no by-name scope ops and the transition choke point stays
untouched; the seed hook is the only failure source. Later amended,
[§dd-dr:embedding-feedback-policy]: `initial_state` is `impl
Into<Arc<ParsingState<L>>>` — a shared handle seeds by identity, and every
by-value call site compiles unchanged.)

Rejected alternatives: preset-level `parse()`/`parse_tolerant()` facade functions and a
configuration builder (shortcut accessors — abandoned at the first configuration need;
fix the real constructor instead); keeping seed customization delta-only (buries the
everyday setup under delta + scope-op concepts and makes it fallible for no reason the
everyday case can trigger).
Revisit if: a Lang emerges whose seed coherence genuinely requires a finalize-style hook
over the package-augmented seed — then that hook becomes an explicit, documented opt-in
on the seed-construction path, not a return to mandatory delta routing.

#### The descent guard: one descent entry point, a per-parse recursion limiter [§dd-dr:descent-guard]

Status: DECIDED (user, descent-guard design session).

Parsing recursion was unbounded, and stack overflow is **not a catchable failure**:
it aborts the process — no `Result`, no diagnostic — a strictly worse outcome than
the input-triggered panics [§dd-dr:panic-policy] already forbids, and a
denial-of-service vector for any embedder parsing untrusted input. Measurements
showed one syntactic nesting level costs
4.3–8.0 KB of stack in release and 35–48 KB in debug, so a plain `cargo test`
aborts on 61 nested `{}`. Two structural decisions plus one storage decision close
this:

**1. `parse_construct` is the single normative descent entry point.**
`ParseContext::parse_construct(parser, state, frame)` (the pylatexenc
`walker.parse_content` analog) is the method every descent — one `ConstructParser`
running another over the same input — MUST go through. One funnel is what lets the
engine attach its per-descent bookkeeping (enclosing-state stack, optional
traceback frame, the guard check) uniformly, with no per-site cooperation. The
contract's stated limit: plain-Rust recursion that bypasses the funnel is
undetectable by design — documented, not enforceable. Details ruled with it:
- `state: None` means **clone the current state** — identical swap/restore scoping
  and the same enclosing-state stack entry as `Some(Arc::clone(&cx.state))`, never
  "skip the scoping": whether a sub-parse runs under a different state is decided
  by the caller's argument, not by the presence of scoping (the caller-decides
  law), and the enclosing-state stack stays uniform across every descent.
- **Frame folding**: `frame: Option<Frame<L>>` is pushed around the whole descent
  and popped on the `Ok` and `Err` paths alike (errors are values, not unwinds);
  the former hand-composed `with_frame(frame, |cx| cx.parse_scoped(…))` sites
  collapse into one call. `parse_scoped` is removed — its scoping role is absorbed
  (`with_parsing_state` remains the non-descent state-scoping utility;
  [§dd-dr:parse-scoped] amendment; the name goes to [§dd-dr:superseded-names]).
- `parse_nodes`/`parse_group` stay as **one-line delegates**: driver factory +
  `parse_construct` fused — the uniform-routing contract (one driver override
  applies to every descent site) — with prominent thin-wrapper rustdoc.
- The two remaining direct `ConstructParser::parse` dispatch sites under a
  hand-pushed frame (the expression-position invocation dispatch and the tack-on
  field dispatch) were migrated into `parse_construct(parser, None, Some(frame))`
  under the MUST rule. Accepted behavior delta: each site now also pushes an
  enclosing-state stack entry for the descent's duration — an `Arc`-identical
  duplicate of the current state, harmless under `ParsingStateStack`'s documented
  scan semantics — restoring the "same descent points as the frame stack" symmetry
  those sites were missing.

**2. `StdDescentGuard`: a per-run object asked before every descent.** The
guard type is engine-fixed: every run uses `StdDescentGuard`, typed concretely
on `ParserSession` and `Language` (fully monomorphized; every `ParseDriver` item
is defaulted, so `impl ParseDriver<L> for D {}` is a complete driver). The
`DescentGuard` trait states the contract the engine drives the guard through;
wiring in another implementation is deliberately not offered — a driver-chosen
guard type (`ParseDriver::DescentGuard` as the trait's one required item, plus a
`StdParseDriver` type parameter to carry it) is rejected: exactly one real
implementation exists, and the associated type taxed every driver impl with
ceremony buying nothing. Reintroduce a type knob only if a second real guard
implementation materializes. The init *value* lives on `Language`
(`with_descent_guard_init`), mirroring seed-state placement — configuration on
the long-lived bundle. The
per-parse *instance* lives on the session: `parse_source` installs it eagerly (the
standard guard measures its stack reference point at true parse entry, on the
parsing thread), a hand-built `ParseContext` gets a lazy `Default`-init fallback at
the first descent, and `ParserSession::install_descent_guard` is the public seam
for hand-built sessions that want a configured guard.
- **The measured stack budget is the mechanism; the depth limit is deterministic
  policy.** `StdDescentGuard`'s budget modes estimate consumption by address
  distance from the init-time reference point — bounding the resource that is
  actually exhausted. The `DepthLimit` mode counts **engine descents**, which run
  ~2× the syntactic nesting depth (a group costs its own descent plus the content
  run over its interior) — deterministic across platforms and build profiles, the
  right mode for tests and format policies, but silent on actual stack use.
- **`StdDescentGuard::DEFAULT_STACK_BUDGET` = 250 KiB in all builds** —
  deliberately tight in debug (roughly ten to fourteen syntactic levels): an
  untuned deep parse fails early with a refusal that names
  `with_descent_guard_init`, instead of consuming an unknown amount of stack.
- **`StdDescentGuard::HEADROOM` = 64 KiB, library-owned, `ComputedStackBudget`
  only**: budget = probe() − HEADROOM — reserve for the work between two descent
  checks and for the error path after a refusal. The Fixed/Computed asymmetry is
  deliberate: a computed budget is a *physical-stack measurement* that must leave
  room to act on a refusal, while a fixed budget is a caller-chosen *consumption
  cap* — subtracting library headroom from it would silently repurpose the
  caller's number.
- **The 50% early warning is latched and unconfigured-only**: at the first descent
  crossing half the budget, only when running on the unconfigured built-in
  default, once per parse — condition `DescentLimitApproaching`, warning severity,
  emitted immediately at the current position (not at parse end), pointing at the
  configuration entry point.
- **A refusal is `DescentLimitExceeded` and aborts under any recovery policy** — a
  distinct condition, not `ImplementationError` (it is an input/configuration
  limit, not an extension bug), with the live traceback including the just-pushed
  frame. No tolerant fallback in v1: past the limit there is no safe way to
  continue.
- Naming: "descent" is established public vocabulary; "Stack"-only names were
  rejected (collision with the crate's data-stack vocabulary: scope stack, frame
  stack, enclosing-state stack). Implementation note: `StdDescentGuardInit`'s four
  ruled modes (`FixedStackBudget`/`ComputedStackBudget`/`DepthLimit`/`Off`) live on
  a private enum behind snake_case constructors — Rust enum variants cannot carry
  the required private "unconfigured" mark, so construction is constructor-only.

**3. Delta boxing.** `ConstructParser::parse` returns `(Self::Output,
Option<Box<ParsingStateDelta<L>>>)` and `NodesOutcome::after_effects` is
`Option<Box<ParsingStateDelta<L>>>`: the pass-through delta family dominated the
recursion cycle's frames (the measurement: 61% of `NodesParser::parse`'s debug
frame) while nearly every slot carries `None` — boxing moves the 208-byte value
behind a pointer exactly where it rides the recursion.
`AttachedSourceOutcome::after_effects` is boxed too: the door's accumulator sits on a frame that
stays live across the whole nested include parse, and moving the already-boxed
`NodesOutcome` record into the bundle also removes an unbox/rebox round trip on
the persist path. Explicitly ruled NOT boxed (same ruling): the driver hooks
(`group_interior_delta`/`resolve_state_event`),
`ArgumentSpec::parsing_state_delta`, and `EnvironmentBehavior::body_state_delta`
— their values are consumed in frames that unwind before recursion descends, so
boxing them buys no per-level stack.

Rejected alternatives: a depth limit as *the* mechanism (a level count says
nothing about actual stack use — per-level cost varies ~8× across build profiles
and targets, so any count is wrong in one of them; it survives as the
deterministic policy mode); `dyn` guard storage on the session (the check runs on
every descent — the associated type keeps it monomorphized at the cost of exactly
one line per driver); a guard parameter on `Language::parse` (per-call
configuration for a per-language policy; `Language` is the configuration home,
mirroring the seed state); warning on every unconfigured parse
regardless of depth (noise on every casual shallow parse; replaced by the
self-describing refusal text plus the once-latched half-budget warning);
callback-owned headroom (every probe author would have to know the engine's
between-checks consumption — that number is the library's to own); stacker-style
stack *growing* (`stacker::maybe_grow` segments) — unavailable to a `no_std` core,
and it enlarges the resource instead of enforcing a policy on untrusted input;
trampolining / an explicit heap descent stack (a whole-engine restructuring that
forfeits the plain recursive parser-authoring model — third-party
`ConstructParser`s recurse in ordinary Rust regardless).

Accepted costs: the two migrated dispatch
sites push an extra `Arc`-identical enclosing-state stack entry each (above); the
unconfigured default warns at roughly five to eight syntactic levels in debug
builds (the intended nudge, but visible in test suites — deep-nesting fixtures
configure `depth_limit`/a larger budget/`off` explicitly); and the guard cannot
see plain-Rust recursion that bypasses the funnel.

Deferred, not rejected: a compile-time witness parameter on
`ConstructParser::parse` (would make the funnel structurally unavoidable);
tolerant per-site fallbacks for a refusal. Consumer-traversal guards are their
own decision: [§dd-dr:traversal-builders].

Revisit if: a refusal shows up in legitimate documents under a properly computed
budget (the headroom constant or the ~2× descent factor is then miscalibrated), or
a framework needs the deferred witness parameter.

#### Traversal drivers are builders, and every traversal is depth-guarded [§dd-dr:traversal-builders]

Status: DECIDED (user, traversal-builders design session).

The three tree-traversal entry points are **builder-shaped drivers**, not free
functions: `TreeWalker::new(&mut visitor).walk(node)`,
`TreeRestager::new(&mut visitor).restage(&tree)`,
`TreeRecomposer::new(&mut recomposer).recompose(&tree, state)`. Decisive
reason: a free function's arity is frozen while run configuration grows (the
descent guard below, future walk features), and a driver value with `with_*`
methods speaks the configuration grammar `Language` already uses. Details ruled
with it:

- **`new()` constructors, not pub tuple structs** — a pub field would freeze the
  layout publicly; private fields let `with_*` grow indefinitely.
- **The visitor is held by `&mut` borrow** (a lifetime on the driver), never by
  value: visitors accumulate run-spanning results in `&mut self` that the caller
  reads back after the run; by-value storage would need a give-back channel.
  `?Sized` (dyn visitors) and the closure blankets are unaffected.
- **Run inputs ride the terminal call, configuration the `with_*` methods**:
  `walk(node)` takes a `NodeRef` (the whole tree is `walk(tree.root())`; subtree
  walks stay expressible); recompose's root state is a run input.
- **Every traversal run is depth-guarded**: each driver creates a per-run
  `StdDescentGuard` from its own `with_descent_guard_init` — engine-fixed type,
  per-run init, exactly the parse's arrangement ([§dd-dr:descent-guard]). The
  guard wraps the drive recursion (`try_enter`/`exit` around each level; `exit`
  on success and error paths alike, none for a refused descent); for
  restage/recompose it lives **in the run context**, so the re-entrant region
  ops are counted too. A traversal costs exactly one descent per tree nesting
  level (a parse costs ~2× per syntactic level). A refusal is
  `RestageError`/`RecomposeError::DescentLimitExceeded`; `walk` returns
  `Result<(), WalkError>` with the same single variant — **visitors stay
  infallible**, only the guard can fail a walk.
- **The warning channel is the visitor**: the three-channel discipline places
  run-spanning consumer state in the visitor's `&mut self`, so all three
  visitor traits carry a defaulted no-op hook,
  `observe_descent_warning(&mut self, DescentWarning)` (the driver hooks'
  `observe_*` notification vocabulary); closures inherit the default.
- **The guard's self-describing texts are owner-agnostic**: they speak of "the
  run" and name `with_descent_guard_init` with its owners (Language and the
  three drivers), so the unconfigured default's advice is right inside a
  traversal too.

Accepted costs: call sites carry the builder ceremony (the terminal-method names
keep reading like the old calls); `walk` is fallible (`.unwrap()` in the common
case); a *single* restage op that copies a subtree verbatim (the
`_with_content` helpers' wrapper copies) recurses over that subtree between two
guard checks — documented on `TreeRestager::with_descent_guard_init`.

Rejected alternatives: free-function entry points (frozen arity — every new run
parameter is a breaking change or a second entry point), and keeping them
alongside the builders (dual canonical paths); an iterative (heap-stack) `walk`
to preserve infallibility — it would bound only the walk while
restage/recompose still need the guard for their re-entrant ops, splitting the
mechanism story for one signature; a `Result`-shaped warning return or a sink
closure on the builder (the visitor already owns the run's consumer state; a
sink borrows against it).

Revisit if: builders accumulate enough configuration that a shared config
struct beats per-driver `with_*` methods, or the between-checks copy recursion
shows up in practice.

#### Hook fallibility: `Result` where a real failure exists; documented infallibility elsewhere [§dd-dr:hook-fallibility]

Status: DECIDED (user; prompted by the first external embedding, whose hook
implementations can fail for reasons a bare-value signature cannot report).

The extension hooks split deliberately: **fourteen hooks return `Result`; seven
return bare values on purpose, with the rationale in their rustdoc.** The sorting
question, applied per hook: *is a failure of this hook a diagnosis of the
document, an operational failure or defect in the embedder's code — or is a
neutral answer always sound?* A hook is fallible where a genuine failure case
exists even for pure-Rust implementors — the natural body calls fallible
machinery (both `ChildStateSpec` `Compute` arms call `derived()`; a lazily-loaded
definition provider can fail to load inside `resolve_command`; a seed assembled
from configuration can be invalid in `initial_state_data`) — or where the
infallible signature silently corrupted the parse (a failing stop predicate
answering `false` means "keep parsing": the parse *succeeds* and the tree is
silently wrong). The fallible roster: the two `ChildStateSpec::Compute` arms,
`TokenStopKind::Predicate` and the node-stop predicate, `Lang::initial_state_data`
(erring `FinalizeError`, surfacing through the fallible
`lang_initial`/`lang_initial_with_packages` seed constructors),
`Lang::make_node_ext` (builder-level: `NodeBuildError::ExtMintFailed` — the
builder also runs for consumer-built trees, where no parse context exists), the
parser factories (`ParseDriver::make_nodes_parser`/`make_group_parser`/
`make_invocation_parser` and `CallableSpec::make_invocation_parser` — an `Err`
means "could not build the parser", distinct from the descent guard's depth
refusal, [§dd-dr:descent-guard]), `ParseDriver::resolve_command` (with
`CommandResolver` in step), `resolve_state_event`,
`EnvironmentBehavior::body_state_delta`, and `observe_transition`. In-parse hooks
err with `ParseError<L::SourceOrigin>`; the consultation site attaches the live
traceback when the error carries no frames (hooks have no session access); an
`Err` aborts under any recovery policy ([§dd-dr:err-means-abort]).

**The three-way condition split**, documented on every fallible hook: the
condition `HookFailed` (`core.hooks.hook-failed`; `{ detail, cause:
Option<Arc<dyn Error + Send + Sync>> }`, the cause-chain field modeled on
`ResolveError`'s) reports *operational* failures of consumer-supplied hook code —
I/O gone wrong, a runtime exception in embedder code; `ImplementationError` keeps
its verbatim meaning (extension *contract* violation) and is never reused for
operational failures; document diagnoses carry ordinary domain conditions.
`resolve_command` keeps `CommandResolution::Failed` as the diagnose-and-recover
outcome ([§dd-dr:resolver-failure]); its `Err` channel is the abort axis, not a
replacement for it.

**`observe_transition` is a dual channel**: it takes the diagnostics sink (`&mut
Diagnostics`, already public API in exactly this position on
`observe_parse_start`) *and* returns `Result`. Sink = record document-level
observations without affecting the parse (an error-severity diagnostic does not
abort — aborting is `recover`'s business); `Err` = abort on a truly problematic
state. The data half is `L::SessionExt`, which `ParseResult` returns as
`session_ext` — accumulating into the session extension is the hook's documented
purpose, so the value must be readable after the parse.

**The seven deliberately infallible hooks** — `recovery`, `refine_diagnostic`,
`make_paragraph_break_node`, `source_resolver()`, `specials_trigger_chars`,
`ComposePiece::append`, `LineColProvider::line_col` — each state in their rustdoc
that the infallibility is deliberate and why (a pure policy read; an identity
fallback is always sound; a fixed default node is always answerable; failure
belongs on `SourceResolver::resolve`, which is fallible; a conservative superset
is always answerable; both shipped piece types genuinely cannot fail; `Option`
*is* the no-answer channel), plus the recommended course for embedding code that
can still fail internally: report through the embedding's own channel and answer
the documented neutral value. The documentation obligation is load-bearing: an
absent `Result` must read as a constraint, never as an oversight.

**Cost**: the statically dispatched hooks (`Lang`, `L::Driver`, specs) are
monomorphized, so an infallible body lets the compiler fold the `Result` channel
away — the channel is free where it is unused. The `&dyn Fn` callbacks
(`Predicate`, the node stop, both `Compute` arms) materialize their `Result` on
every consultation; the measured sizes, pinned by an in-crate test:
`Result<bool, ParseError<_>>` and `Result<Arc<ParsingState<_>>, ParseError<_>>`
are byte-identical to the bare `ParseError` (the `Ok` payload fits the error
type's niches; 64 bytes, one cache line, smaller than the content loop's
pre-existing per-token plumbing), so the `Err` arm is not boxed — the pin test
says re-measure before restructuring.

Rejected alternatives: a `Result` on every bare-value hook uniformly (churn
without a failure story — the neutral-answer hooks would gain an abort channel
with no sound abort semantics); a session-level "abort with implementation
error" escape reachable from hooks (a second error path bypassing the typed
condition channel); factories deferring failure into a parser whose `parse()`
errs (reports one call later than the failure, and every embedding invents its
own stub parser); the names `ExtensionError` (collides with the
`NodeExt`/`StateExt`/`SessionExt` extension-*data* vocabulary) and
`OperationalError` (vague; database-API flavor) for the new condition —
`HookFailed` matches the register's event-style condition names
(`StrayGroupClose`, `MalformedBegin`, `DescentLimitExceeded`); an `Arc`'d cause
field on `ExtMintFailed` (would cost `NodeBuildError` its derived `PartialEq`,
matched by in-crate and downstream assertions, and the parse-side lift
stringifies anyway — an implementation renders its cause chain into `detail`).

Revisit if: an infallible hook acquires a genuine failure case with no sound
neutral answer, or the pinned hot-path size test shows the `Err` arm outgrowing
a cache line (then box it, following the boxed-deltas discipline of
[§dd-dr:descent-guard]).

---

#### A group may leak an after-effect: the `GroupAfterEffectsFn` hook [§dd-dr:group-after-effects]

Status: DECIDED (user, this session).

`GroupParser` carries an optional per-descent hook
(`new_with_after_effects`, a borrowed `&'p dyn Fn`) that maps the interior content run's
merged after-effect record (`NodesOutcome::after_effects`) to the group's own after-effect
for its caller; with no hook it returns `None`, the unchanged scoped-descent behavior. The
`GroupOpen` arm of the content loop applies and records that delta exactly as it does an
invocation's, so an escaping effect holds for the following siblings *and* joins the
enclosing run's record. The motivating construct is TeX's `\gdef` — a definition whose
whole point is to outlive the group it is written in — which the delta channel could not
express at all before, group scoping being structural (the evolved interior state is simply
dropped with the descent).

Three points carried the shape:

- **Installed through `make_group_parser`, not per descent site.** A language gets the hook
  at *every* group, which is what makes an escape compose outward through nested groups: one
  hook per level, each seeing the level below's escape in its own record. A per-site opt-in
  would leak at one depth and swallow at the next.
- **The hook is a filter over ops, because the record has no provenance.** The record is one
  merged delta (`merge_from`: rules last-writer-wins, scope ops concatenated), so it cannot
  say which construct contributed what. A `\gdef`-vs-`\def` split is therefore expressed
  *structurally* — the language tags its own ops, `\gdef` defining into a globally-named
  scope via `ScopeOp::Define` and `\def` into a local one, and the hook keeps the
  globally-targeted ops. Rules/mode/ext overrides carry no such tag; for those a hook can
  only answer all or nothing. This is a real expressiveness limit and is documented as one.
- **Fallible and borrowed, matching the descent-policy callbacks.** `Err` aborts under any
  recovery policy with the traceback attached at the descent site (the hook has no session
  access) — the panic policy leaves outer-layer code no other channel ([§dd-dr:panic-policy]
  rule 3). `&'p dyn Fn` over a bare `fn` pointer for the same reason as
  [§dd-dr:child-state-spec]'s compute arms: a driver must be able to close over its own
  configuration (which scope name counts as global), and `GroupParser<'p, L>` already
  carries `'p`.

The hook receives four things: the matched `GroupRule` (so a language may let an effect
escape `{…}` but not `$…$`), the interior's *initial* state, the interior's *exit* state
(`NodesOutcome::state` — the only reader of what the interior actually defined; it dies with
the descent immediately after), and the record itself **by value**, so a hook filters in
place rather than cloning.

Rejected alternatives: *always propagate the record and let the caller filter* — the caller
is the content loop, which has no idea what the language means by "global", and every
language without a `\gdef` would pay for a filter it does not want. *A boolean
`persist_state` knob* as `\input` carries — right for inclusion, where the choice is genuinely
all-or-nothing per call site, but useless here: the whole difficulty is that one group holds
both escaping and non-escaping definitions. *A `ScopeOp` marked "global" that the core
recognizes* — a privileged language concept in the core ([§dd-dr:no-privileged-concepts]);
scope naming already expresses it as data.

Known remaining hole: group-delimited **arguments** still drop what their groups leak. They
descend through `cx.parse_group` like anything else, but an `ArgumentParser` has no
after-effect channel to its caller, so `\gdef` inside `\foo{…}` does not escape. Closing it
means giving the argument route a delta channel — a separate decision, not taken here.

Revisit if: a language needs escapes out of argument extents or environment bodies (the
argument channel above), or the op-tagging filter proves too weak for a real `\gdef` — the
next step would be per-op provenance in the record, which was deliberately not built
speculatively.

## Generics strategy [§dd-dr:generics]

#### Defer `Rc`/`Arc` genericity [§dd-dr:defer-rc-arc]

Status: DECIDED (user-led).

The `SharedPointer` GAT sketched in SOURCE_ARCHITECTURE.md would infect nearly every signature
in the crate to save ~1ns uncontended atomic increments that happen once per node, not per
byte. Use `Arc` behind an internal alias (`pub(crate) type Shared<T> = Arc<T>`) so a later swap
is mechanical.
Revisit if: profiling on real workloads shows refcount traffic, or a wasm/embedded target
genuinely needs `Rc`.

#### What is generic (via `Lang`) and what is not [§dd-dr:lang-genericity-scope]

Status: DECIDED (working doctrine; formerly proposed).

Generic (via `Lang`): the extension types (`StateExt`, `SessionExt`, the `NodeExts`
bundle), the closed id vocabularies (`GroupTypeId`, `CallableTypeId`, `ModeId`),
`SourceOrigin`, `Event`, and the `Driver`. Not generic: spec types (extensibility comes
from `CallableSpec` being a trait), the pointer type (above), content backing (a plain
`String` on `Source`; the `SourceContent` seam was retired,
[§dd-dr:sources-and-spans]). Every proposed new `Lang` associated type should be
challenged against [§dd-dr:data-vs-traits]/[§dd-dr:one-generic-param] first.

## Errors and diagnostics [§dd-dr:errors]

#### Panic policy: `Result` everywhere; panics only for verifiably unreachable invariants [§dd-dr:panic-policy]

Status: DECIDED (user, code review; refines the original one-line CLAUDE.md constraint).

Four rules:

1. Panics are allowed only for **verifiably unreachable** code — impossibility guaranteed by
   this crate's own structure (a bounds check in the same function; a private constructor
   that always establishes the invariant), *independent of anything outer layers do*:
   problematic user input, a buggy `Lang` hook in a preset, or a misbehaving custom
   argument/construct parser must never panic a core routine. Written as
   `unreachable!`/`expect` with the invariant stated in the message.
2. The violation of a documented input contract is **not by itself** a reason to panic — it
   returns an `Err` (translatable to, e.g., a Python exception by a wrapper).
3. Individual exceptions require explicit user approval (escalated case by case).
   They stay few, concern deep, often-used code that primary users typically never
   call directly, and follow a std-standard policy. Approved:
   (a) *indexing-style accessors* — `NodeTree::node`/`nodes_in`, `Span::slice`,
   `TextContent::resolve`, and `ChildRegion`'s resolved-only accessors keep their
   documented panics **with non-panicking companions** (`NodeTree::get`, `Span::get`,
   `ChildRegion::staged`)
   — the std `Index`-vs-`get` convention: the panicking form for ids/spans the caller
   obtained from this very tree/source, the `Option` form for values of unknown
   provenance; (b) *always-on precondition asserts* on the four
   deep value functions `Span::new`, `Span::extend_to`, `SourceSpan::new` and
   `SourcePos::new`, on the seven scan helpers of `core::token`
   (`skip_whitespace`, `scan_paragraph_break`, `scan_group_delimiter`,
   `command_rule_at`, `scan_command`, `scan_comment`, `scan_specials_trigger` — each
   requires `pos` to lie within the content it is handed, on a `char` boundary, and
   `scan_command` additionally requires `rule.escape_char` to stand at `pos`;
   [§dd-dr:scan-helpers]), and on the seven span-taking `StdToken`
   constructors, which inherit the same slot for the span coherence each one asserts
   (the eighth, `StdToken::end_of_stream`, takes no span and never panics): a
   documented-contract violation panics in every build — these functions are either deliberately infallible (no
   `Err` channel exists to prefer) or, for the two helpers that do have one —
   `scan_command` and `scan_specials_trigger` — report through it about the scanned
   content and not about their caller's mistake; the checks are O(1), and the always-on
   panic keeps invalid values unrepresentable where the release alternative was unspecified
   misbehavior or a later cryptic panic far from the cause (the std str/slice-indexing
   convention). Each site documents the all-builds panic in its rustdoc with a pointer
   to rule 3, pinned by `should_panic` tests; invalid `Span`/`StdToken`/`SourceSpan`/`SourcePos` values are thereby
   unrepresentable through the public API (`TokenListReader::new` is `cfg(test)`-only test
   infrastructure and keeps a debug assert).
4. Everything else returns an error.

Consequences applied with the decision:
- `NodeTreeBuilder::{add, add_with_ext, finish}` validate their contract and return
  `NodeBuildError`; the previously debug-only exact-tiling and spanned-content checks are
  always-on error paths (one validation regime in every build — extension authors get clean
  errors, not debug panics). A builder whose `add` errored is *poisoned*; the build must be
  abandoned. The builder's internal post-validation read-backs remain `expect`s — rule 1:
  **validate at the boundary, assert inside**.
- Construct parsers lift `NodeBuildError` into a `ParseError` carrying the
  `ImplementationError` condition (`core.constructs.implementation-error`) via
  `ParseContext::implementation_error`, deliberately bypassing the recover funnel: an
  implementation bug **aborts even under tolerant recovery** (tolerance promises a
  best-effort tree for bad *input*, not tolerance of buggy extensions), and no
  `Lang::refine_diagnostic` pass applies. (The former `StdInvocationParser` slots check is gone with the spec-side slot
  list — nothing declarable was left for it to catch, [§dd-dr:no-spec-side-slots].)
- Staged-id read-backs whose id passed through outer-layer hands degrade gracefully (the
  node-stop test treats a missing id as "condition did not fire"; invocation/body span
  read-backs fall back to the trigger/body start). No silently-wrong tree results: the
  bogus id still lands in `builder.add`'s child list, where it is diagnosed.
- The scan helpers panic on an invalid `pos` (rule-3(b)), `skip_whitespace` included —
  a debug assert with a return-unchanged release fallback was consciously superseded
  there — and the check runs at the top of each helper, ahead of every feature gate, so
  the family behaves identically whatever the rules say ([§dd-dr:scan-helpers]);
  `Span::len`'s
  saturation is defensive only, since inverted spans are unrepresentable under
  rule-3(b)'s asserted constructors; `ParserSession::finish` returns
  `Result<ParseResult, NodeBuildError>`.
- Every remaining guard on outer-layer input is an `Err`
  implementation-error path: the environment-terminator re-peek and
  reader-position guards, the driver-factory pass-through-delta and stop-cause
  guards, the spec-author emptiness/distinctness guards of the standard argument
  parsers (constructors stay infallible; the check runs where the parser runs),
  the `Lang::scan_specials` match-end guard and a single reader-position
  validation at `StdTokenReader::peek` (both as unrecoverable
  `TokenErrorKind::Custom` implementation errors), and the chars-run contiguity
  guards. The staged-id read-backs of the standard argument parsers (group,
  optional-group, chars-group) follow the staged-id degradation rule through one
  shared zero-child-answer helper.
- `check_tree_invariants` is exempt: a documented test utility whose *purpose* is
  asserting — panicking is its API. `debug_assert!` remains fine for crate-internal
  invariants but is not a substitute for boundary validation of outer-layer input.

Rationale: an invariant assertion that can only fire on a core bug is better loud than
silently wrong; but a panic reachable through an extension author's mistake turns their bug
into a crash of the host application — an error naming the violated contract is strictly
more useful, in every build profile.
Rejected alternatives: sanctioning the builder's panic-on-caller-bug policy (an earlier
review recommendation) — it violates rule 2's "outer layers must not panic the core";
`Option`-returning tree accessors everywhere — clutters every legitimate traversal for a
misuse the `get` companions already cover; keeping the six value functions
debug-asserted with release fallbacks (superseded in a recorded reversal — the
functions are deliberately infallible, so rule 2's "return an `Err` instead" has no
channel to prefer, and the real release-mode alternative was unspecified misbehavior
or a later cryptic panic).
Revisit if: profiling shows the always-on builder validation measurably costs on the hot
staging path (all checks are O(1) per region/payload today).

#### Errors carry Arc-based `SourceSpan`, not `'src` lifetimes [§dd-dr:arc-error-spans]

Status: DECIDED (implemented; formerly proposed).

The first-cut `ParseError<'src>` / `Result<'src, T>` spread a lifetime through every
signature and prevented errors from outliving the parse. Arc spans fix both at negligible
cost (errors are rare and cold).

#### Tolerant parsing via recovery tokens + diagnostics sink [§dd-dr:tolerant-parsing]

Status: DECIDED (implemented; formalizes the user's original mechanism — one of the
salvaged pieces).

Tokenizer errors may carry a recovery token; a session-level `Recovery` policy (strict /
tolerant) decides whether to record a diagnostic and continue or abort. Diagnostics
accumulate on the session and remain available on `ParseResult` even for successful
tolerant parses.
Rationale: tolerant parsing is a first-class requirement for document tooling (FLM,
linters, editors), not an afterthought flag; and a diagnostics sink is the API-honest
replacement for logging side channels (see [§dd-dr:dependencies]).
Concrete shape: `TokenError<L>` = structured `TokenErrorKind` (closed enum:
end-of-stream-after-escape, forbidden-char — replaces pylatexenc's stringly
`error_type_info`) + a `SourceSpan` + `Option<TokenRecovery<L>>`, where `TokenRecovery` =
placeholder token + an explicit `resume` stream position (explicit rather than derived
from the token: a custom source's placeholder need not end where reading resumes, and the
explicit position carries the advancement requirement; the built-in recoveries all resume
at their placeholder's end — the dangling-escape placeholder is a `Char(escape_char)`
covering the escape byte). A token error's location is **source-qualified at the reader**:
only the reader knows which source its offsets belong to, so it is the party that can
name one ([§dd-dr:no-context-source]); the session boxes the condition into a
`Diagnostic` around that span. The reader itself is policy-free: it always returns `Err` with the
recovery attached, and the session's `Recovery` policy decides (the original per-reader
`tolerant_parsing` flag is superseded).
Rejected alternatives: a token-agnostic `TokenError<R>` designed blind against no real
tokenizer.

#### Detection-site recovery; `Err` means abort [§dd-dr:err-means-abort]

Status: DECIDED (user).

Three rules:
1. *Recovery happens where the problem is detected.* `ParseContext` exposes the `Recovery`
   policy and the diagnostics sink behind a helper (tolerant: record the diagnostic and
   continue; strict: return `Err`). Token errors continue with their `TokenRecovery` token
   (the content loop repositions the reader to the recovery's resume position); each
   parse-level condition
   defines its recovery at its site — unresolvable command: diagnostic + span-backed
   chars-node fallback (markup text in a `Chars` node is an accepted tolerant-recovery
   artifact, always accompanied by a diagnostic); missing mandatory argument: absent +
   diagnostic; unmatched group close at the root: diagnostic + skip; terminator mismatch:
   close-without-consuming ([§dd-dr:parsers-engine]).
2. *Abnormal endings of sub-parses are data, not errors.* `NodesParser` returns a
   `StopCause`; only the caller knows whether EOF-before-`\end{align}` is an error. Nobody
   ever continues *past* an `Err` — which is what keeps the reader position and the state
   `Arc`s coherent through recovery, by construction.
3. *`Err(ParseError)` = strict-mode abort or genuinely unrecoverable.* It carries no
   recovery payload and bubbles freely. State deltas from an abandoned parse are dropped
   unless the recovering site explicitly returns one; abandoned staged nodes are dropped by
   the builder (designed for this).
Rejected alternatives: pylatexenc's recovery-attributes-on-exceptions (`recovery_nodes`,
`recovery_at_token`, `recovery_past_token`, caller-applied repositioning) — a workaround
for having no context object, and exactly the caller/callee reader-state ambiguity that
rule 2 eliminates.

#### A recovery's resume position must move the reader; violations abort even in tolerant mode [§dd-dr:resume-pos-contract]

Status: DECIDED (user, code-review follow-up).

The content loop's recovery arm
is the one arm that consumes no token, so its termination rests entirely on
`TokenRecovery::resume` repositioning the reader away from the failed read's start. The
one in-crate producer class — readers — satisfies this, but the requirement is reachable
by third-party code through a custom `TokenReader::peek`, and a violating resume position
was demonstrated to hang `NodesParser` in release builds while growing the diagnostics
sink unboundedly. It is stated on `TokenRecovery::resume` and enforced at the adoption
site (`nodes_parser.rs` content loop): the loop compares the reader's position before and
after `move_to_position(&recovery.resume)`, and an unchanged position aborts the parse
with the token error as a `ParseError` — *even in tolerant mode*, whose promise is a
best-effort tree, not tolerance of non-termination; an abort is the doctrine-blessed
failure mode (no panic, rule 3 above). The check is equality, not order: stream positions
compare only for equality ([§dd-dr:stream-position]). The guard lives at the adoption site
and not inside the move, because repositioning is deliberately bidirectional (it is also
the absent-argument and environment-name rewind), so the move itself can assert nothing
about direction. Since the specials hook can no longer return a recovery at all
([§dd-dr:specials-scan-errors]), a reader is the only party that can violate this.
*Noted for the future:* contract violations by extension-point code are
a different *category* from malformed input, and the error model may eventually want to
distinguish them (e.g. a `ParseError` vs. an `ImplementationError`/contract-violation
kind), so callers can tell "your document is broken" from "your `Lang`/reader is broken".
Today both surface as `ParseError` (here: the token error's kind and span); revisit if
more contract guards accumulate.

#### Structured diagnostics: condition payloads, not prose [§dd-dr:structured-diagnostics]

Status: DECIDED (user + design sessions; supersedes the "grow `ParseErrorKind` variants"
intent).

`Diagnostic` and `ParseError` carry a
structured condition payload `Box<dyn DiagnosticData>` plus span and traceback frames — no
`message: String` field, no kind enum, and no string-message constructors
(`Diagnostic::error("…", span)` is removed, with no ad-hoc escape type). The human message is
a pure function of the payload (its `Display`); a wording difference that is not worth a
field in the payload is not worth existing. Condition types are plain public-field data
structs defined **next to the construct that detects them** (group conditions in the group
parser, environment conditions with the environment helper, token conditions in the token
layer, FLM's in FLM) — third-party conditions are structurally identical citizens.
Rationale: the tolerant path — the one tools consume — reduced every condition to
severity + sentence + span, and `ParseErrorKind::Syntax { message }` had become the only
parse-level kind, `format!`-ing away exactly the fields a linter/LSP needs. A kind *enum*
cannot be made right: core-owned variants would privilege construct-level vocabulary
("environment" is not a core concept, [§dd-dr:no-privileged-concepts]), and `#[non_exhaustive]` extension is crate-only,
so downstream languages would stay stringly forever — the same disease one level out. On the
message side, the "same condition, subtly different context" case decomposes without
remainder: a semantic difference belongs in a payload field (tools want it too); a positional
difference is what frames render. This conforms to [§dd-dr:closed-core-open-payloads]: the *structure*
(severity/payload/span/frames) stays closed; the openness is payload-level, like specs, and
serializability is preserved (see the serialization entry below). Exhaustive matching over an
open-ended condition space is not meaningful — consumers handle what they know, via
identifier or downcast. Severity stays a separate field (conditions do not choose it; the
recover funnel records errors), and the `Diagnostic*` nomenclature deliberately leaves room
for warnings later. The contract-violation category noted above also gets its mechanism for
free: ordinary condition types under e.g. `core.contract.*`.
Rejected alternatives: promoting recurring conditions to enum variants (an earlier
proposal — layering and extension flaws above); a `Lang(L::ErrorKind)` static arm (spreads
`L` into `Diagnostic`/`ParseError`, blocking cross-language aggregation, and callables need
dyn anyway since specs live as `Arc<dyn CallableSpec<L>>`); a message-override `String` on
`Diagnostic` (two truths: prose drifts from data, and re-renderers must ignore one of them).

#### `DiagnosticInfo` (implementor) / `DiagnosticData` (dyn facade) split [§dd-dr:diagnostic-info-data-split]

Status: DECIDED (user + design sessions).

Implementors write a plain data struct (pub fields, `#[non_exhaustive]` + constructor for semver headroom, ordinary `Clone`/`Debug` derives), a
`Display` impl for the wording, and a `DiagnosticInfo` impl: `const IDENTIFIER: &'static str`
plus a defaulted `serializable_data()`. The dyn-compatible facade `DiagnosticData`
(`identifier()`, `serializable_data()`, `clone_box()`) is blanket-implemented for every
`DiagnosticInfo` type and **sealed** — the blanket impl is the only way in.
Rationale: the split is forced (an associated const makes a trait non-dyn-compatible) and
buys everything else: `clone_box` boilerplate vanishes (the blanket impl uses the ordinary
`Clone` derive), the identifier is a compile-time constant, and sealing keeps the
blanket impl the only way in — the const-identifier discipline is the norm, with
one narrow, adapter-scoped `identifier()` method override
([§dd-dr:runtime-condition-identity]). Downcasting targets the data struct itself — one type, one
identity.
Rejected alternatives: macro-generated wrapper types implementing the dyn trait (Rust separates data
from impls, so no wrapper is needed; a wrapper would make downcasts target the wrapper,
splitting each condition's identity in two); getters over pub fields (invariant-free
records).
(Unsealing `DiagnosticData` later is non-breaking; re-sealing is not.)

#### Condition-declaration derive: `#[derive(DiagnosticInfo)]`, syn accepted [§dd-dr:diagnostic-derive]

Status: DECIDED (user + design session; the generated surface is documented on the
derive itself).

Key points from the discussion: **(1)** derive over a `macro_rules!` DSL — the struct stays
plain Rust (rustdoc, IDE, "the struct is the schema" stays literally true), and a DSL would
be a third declaration style to migrate once the derive lands. **(2)** Serializability is
enforced by the *compiler*, not the macro: `serializable_data()` routes fields through the
`ToDiagnosticValue` helper trait, so the macro never parses types and an unserializable field
is a field-spanned bound error. Field names as wire keys is safe coupling — renaming a `pub`
field is already breaking, so the cadences align (unlike type-name vs. identifier). **(3)**
Enum fields cannot be covered by the struct's derive: a proc macro cannot see other items'
definitions, and shared payload enums would emit duplicate impls (coherence). The
annotation-free alternative — an autoref-specialization `Debug` fallback — was rejected: it
dissolves the enforcement and couples wire output to unstable `Debug` formatting. Hence
`#[derive(ToDiagnosticValue)]` on payload enums (one word on an existing derive line).
**(4)** Why syn is acceptable despite the zero-dep stance: it is build-time only (runtime
stays zero-dep), and the alternative was examined seriously — a hand-rolled derive over raw
`proc_macro` is feasible (spans live on raw token trees), but the requirements that decided
it were error-message quality (spanned validation at every attribute site, field-spanned
generation) and scope (constructor, message DSL, `serializable_data`, the enum derive):
together they re-derive syn's machinery at ~600+ hand-rolled lines that grow with every
grammar extension.

#### Two identities: the type in-process, an explicit string on the wire [§dd-dr:condition-identities]

Status: DECIDED (user + design sessions).

In-process identity is the concrete type (downcast via `Any` — collision-proof, compiler-checked at producer and consumer); the string `identifier()` exists
only for boundaries where types cannot go (JSON output, linter config, logs). Identifiers are
hand-chosen, namespaced `<layer-or-preset>.<area>.<condition>` (`core.token.*`,
`core.groups.*`, … for library conditions, areas naming concepts;
`<preset-name>.<namespaced-name>` for presets and downstream languages),
exposed as `pub const IDENTIFIER` so consumers compare against the const rather than a
literal. Identifiers and serialization field names are semver-stable API surface:
the strings are frozen independently of
future code moves.
Rationale: no compiler mechanism yields a stable wire identity — `type_name` has an
explicitly unstable format and encodes module paths (a refactor must not break a user's
linter config), and `TypeId` differs per build and is not serializable. Wire naming is
convention-based in every ecosystem (rustc lints, ESLint rules, LSP codes); what convention
*can* get is hardening: single-definition consts and a documented namespace rule.
Rejected alternatives: deriving the identifier from the type name (the two have different change
cadences — a struct rename is an internal refactor, a wire-id change is a silent break; the
derive macro will *require* the id attribute; the narrowly-scoped per-instance
override for binding/embedding adapters is [§dd-dr:runtime-condition-identity]);
method name `diagnostic_identifier()`
(stutters as `DiagnosticData::diagnostic_identifier`; the trait context already qualifies,
[§dd-dr:naming]); a per-`Lang` `diagnostic_catalog()` with a uniqueness test (maintenance work to keep in
sync, and namespace prefixes already prevent collisions — can be
added later without breakage).
The full stability semantics
(identifier hard-stable, data keys additive, wording excluded) and the
defining-vocabulary ownership rule are [§dd-dr:wire-identifier-stability].

#### Serialization is a derived projection; the struct is the schema [§dd-dr:serialized-schema]

Status: DECIDED (user + design sessions).

`serializable_data() -> DiagnosticValue` (a minimal alloc-only value tree: null/bool/int/string/list/map) serves serialization boundaries and generic tooling
only — programmatic consumers downcast to the typed struct; there is no stringly-keyed access
API anywhere. The method is defaulted (empty) so the trait ships before the serialization
work. No hand-written shipped schemas.
Rationale: `serde::Serialize` is not dyn-compatible (the ecosystem workaround,
`erased-serde`, is a dependency — [§dd-dr:dependencies]), hence the own value tree. The authoritative schema
is the Rust struct itself. pylatexenc's `error_type_info` weakness was ad-hoc dicts assembled
at every raise site; here the keys are written once, adjacent to the struct fields, and the
eventual derive macro generates them from the field names. A shipped machine schema would be
a third representation that drifts from the other two; if external consumers ever need one,
the derive generates it from the same source of truth.

#### Parse traceback: an explicit frame stack on `ParserSession` [§dd-dr:parse-traceback]

Status: DECIDED (user + design sessions).

`Vec<Frame<L>>` on the session, maintained by a closure-scoped
`cx.with_frame(frame, |cx| …)` at the descent points (invocation, argument, group interior,
environment body); the recover funnel snapshots the live stack into every `Diagnostic` and
`ParseError` as `L`-free `TraceFrame<O>`s (rendered `title: String` + `SourceSpan<O>`),
innermost first — this finally produces `format_traceback`'s input and renders as
pylatexenc-style "while parsing …" tracebacks (exactly LSP `relatedInformation` shape). Live
frames allocate nothing: `FrameTitle<L>` stores *mechanisms, not a construct taxonomy* — a
`&'static str` label, a quoted source slice, or an `Arc<dyn CallableSpec<L>>` + role whose
title is produced only at snapshot time via a new defaulted, dyn-compatible
`CallableSpec::stack_frame_title(…)` hook.
Rationale: pylatexenc attaches `open_contexts` in `except` clauses as exceptions bubble;
techy's tolerant diagnostics are recorded at the detection site and never bubble (rule 1
above), so the context must already exist when the condition fires — an explicit stack. That
is strictly better: non-aborting diagnostics get full tracebacks too, from a single
attachment point. It lives on the *session* because the alternatives fail hard:
`ParsingState` is `Arc`-shared, snapshotted into nodes, and `mem::replace`d on descent (group
parser) — a stack there is lost or aliased; the token reader tracks lexical position, not
descent. The session is the unique-per-parse mutable object and already owns the consumer
(the diagnostics sink). Hot/cold asymmetry: frames are pushed on the *success* path, O(every
construct), so they must be allocation-free (`Arc` bumps only), while condition payloads are
cold, so boxing is free — eager `String` titles would be an allocation per descent.
Closure-scoped rather than an RAII guard because a guard would hold `&mut cx` against the
parser body. The live frame may be `L`-generic (the session already is), but the snapshot
must be `O`-generic only, or `L` re-enters `Diagnostic` through the back door. "Frame", not
"context": `ParseContext` already owns that word ([§dd-dr:naming] sibling-vocabulary rule).
Rejected alternatives: frames in `ParsingState` (above); structured machine fields on frames (frames are
the human-facing projection — machine data belongs in the condition payload; title + span is
what tools need); wrapping-on-bubble (the tolerant path never bubbles).

#### `ParseDriver::refine_diagnostic` hook [§dd-dr:refine-diagnostic-hook]

Status: DECIDED (user + design sessions; the hook's home is the driver,
[§dd-dr:parse-driver]).

`fn refine_diagnostic(Box<dyn DiagnosticData>, &ParsingState<L>) -> Box<dyn DiagnosticData>`,
default identity, applied exactly once in the recover funnel (at the `ParseContext` level,
where the state is in scope). A driver can replace a generic condition with its own — FLM
maps a forbidden-`$` token condition to a `DollarMathDisabled { … }` whose `Display` explains
the config option — and the replacement is *structured*, so tools see (and can attach
quickfixes to) the refined condition, not just better prose. The original condition's fields
can be embedded in the refined type where faithfulness matters.
Rationale: a presentation-only hook improves messages but hides the real condition from
machines; refinement serves both needs with one mechanism, and wording stays a pure function
of the payload. State-dependent information the message needs is baked into the payload's
fields at refine time — errors stay self-contained after the parse (no `Arc<ParsingState>`
inside errors, no lazy `L`-dependent rendering).
Rejected alternatives: `L::format_message(&payload, &state) -> Option<String>` (subsumed by refinement;
a second wording path would reintroduce drift).

#### Token layer joins the same model [§dd-dr:token-diagnostics]

Status: DECIDED (user + design sessions).

The two
`TokenErrorKind` variants become plain condition structs (`EndOfStreamAfterEscape`,
`ForbiddenChar`, each a `DiagnosticInfo` impl) wrapped by the enum, which gains
`Custom(Box<dyn DiagnosticData>)` for `Lang::scan_specials`; the enum loses `Copy`
(accepted), and `TokenError::kind()` returns a reference. The lift into diagnostics boxes the
built-in structs and *unwraps* `Custom`; a named `ParseError::from_token_error(…)`
constructor replaces the lift currently duplicated at `try_peek` (`constructs/mod.rs`) and
the content-loop recovery arm (`nodes_parser.rs`).
Rationale: a specials scan reports conditions of the language's own, which
tokenizer-internal kinds could only misname; one extension mechanism (`DiagnosticData`)
serves both layers, while the token layer keeps a concrete matchable enum for the recovery
protocol. The scan itself is outside that protocol — it reports errors and never
recoveries ([§dd-dr:specials-scan-errors]), so its condition travels in a
`SpecialsScanError` that the reader lifts into an unrecoverable `TokenError`.

#### `Diagnostics` retention is capped; `render_all` shares line indices [§dd-dr:diagnostics-retention]

Status: DECIDED (user).

Two bounded-resource fixes in one: (i) `Diagnostics` retains at most `limit` items (`with_limit(n)`;
`DEFAULT_LIMIT` = 1000 via `new()`) — in tolerant mode degenerate input produces one
diagnostic per byte, and an unbounded `Vec` turns a 10 MB input into ~GB of identical
messages. Pushes beyond the cap are *counted* (`suppressed()`, surfaced by `render_all`
as "… and N more") and still feed `has_errors()` (an error-count field covers retained
and suppressed pushes alike), but are dropped; `is_empty()` is false whenever anything
was pushed, retained or not. (ii) `Diagnostics::render_all()` renders the whole
collection through **one `LineIndex` per distinct source**, matched by `Arc` pointer
identity (the engine-memo idiom; sound because the borrowed spans pin their sources) —
per-diagnostic `render()` builds a fresh index per position, making k diagnostics over
an N-byte source O(k·N), with provenance chains multiplying the rescans. The cache
(`SourceIndexCache`) lives in the renderer, not on `Source`: a lazily-populated cache on
the shared `Source` is blocked dep-free (`alloc` has no `Mutex`; `OnceCell` would cost
`Sync`). `format_position` stays as the documented one-shot convenience. The
cap is per-parse driver policy: `ParseDriver::diagnostics_limit()` (defaulted
`None` = `DEFAULT_LIMIT`) seeds the session's sink in `Language::parse_source`,
before `observe_parse_start` so the cap governs the whole parse; hand-built
sessions apply `with_limit` themselves through the public `diagnostics` field.
Rejected alternatives: an unbounded default (the failure mode is silent and input-controlled), and
a public `DiagnosticRenderer` type (no second consumer yet; `render_all` covers the
need — promote the cache if one appears).

#### Wire identifiers: stable namespace, concept-named areas, owner = defining vocabulary [§dd-dr:wire-identifier-stability]

Status: DECIDED (user, API-review policy session).

`IDENTIFIER` strings are semver-stable under the same rubric and soft freeze as public
paths ([§dd-dr:stability-rubric]) — they are wire/config material (match tables,
suppression lists, serialized logs), and changing one is a *silent* break, worse than a
path break. Per condition, exactly this is the contract:

- the **identifier string** — hard-stable;
- the **`serializable_data` keys** — stable with additive-only growth (the structs are
  `#[non_exhaustive]`; [§dd-dr:serialized-schema]);
- the **`Display` wording** — explicitly *not* stable: human messages may improve at
  any time; consumers match on `T::IDENTIFIER` or downcast, never on message text
  ([§dd-dr:condition-identities]).

Two naming rules complete the scheme `<owner>.<area>.<condition>`:

1. **The `<area>` segment names a construct concept or subsystem** (`token`, `specs`,
   `environments`, `arguments`, …) — never a file, module, or type name. This repaired
   an earlier scheme whose `core.*` identifiers used internal *file names* as areas
   (`core.nodes_parser.*`, `core.argument_parsers.*`, …), contradicting the decoupling
   promise documented on `IDENTIFIER` itself — both review personas who guessed
   identifiers guessed concept names and lost a cycle.
2. **The first segment names the *defining vocabulary*** — whoever declares the
   condition type: techy machinery `core.*`, the preset `latexlike.*`, a downstream
   language its own namespace (e.g. `flm.*`). Ruled explicitly: a foreign `Lang`
   reusing preset pillar functions ([§dd-dr:latexlike-generalization]) emits
   `latexlike.*` conditions inside its own parses, and that is correct — the identifier
   names the machinery that raised the condition, not the language being parsed.

One-time re-homing rides with the preset-generalization application: a condition type
relocated preset→core gets its identifier re-homed pre-freeze; post-freeze, identifiers
never move again even when their declaring types do (the decoupling promise).

Rejected alternatives: lang-dependent identifiers (a method instead of the const —
destroys typed matching via `T::IDENTIFIER`, the sealed-facade blanket impl, and the
registry/doc story); a code-side identifier registry (rejected in
[§dd-dr:public-namespace-topology] — the rustdoc `DiagnosticInfo` implementors listing
plus a guide table serve the need); stable message wording (freezes prose for no
consumer value — identifier plus payload is the contract).

**The frozen slate** (the guide table prints exactly these; identifier-asserting
tests sit beside the condition types). Area `specs` absorbs command resolution AND
a former `scopes` area (user: "resolution of what?" — it also disambiguates
against *source* resolution, `core.sources.*`; the wire vocabulary tracks the
public `core::specs` home from [§dd-dr:resolution-extraction]):
`core.specs.{unresolvable-command, command-resolution-failed,
callable-defined-as-error, scope-op-failed, provider-commands-shadowed-by-escape}`
(the last is the parse-init warning, condition type
`ProviderCommandsShadowedByEscape`);
`core.groups.{unclosed-group, stray-group-close}`;
`core.environments.{terminator-mismatch, malformed-terminator,
missing-terminator}`;
`core.arguments.{missing-mandatory-argument, expected-expression-argument,
expression-callable-requires-content, repeated-tack-on-field}` (a bare
`repeated-field` was too vague outside its own area);
`core.recovery.unusable-recovery-token`;
`core.verbatim.{unterminated-verbatim, expected-verbatim-delimiter}`;
`core.token.{end-of-stream-after-escape, forbidden-char}`;
`core.constructs.implementation-error`;
`core.sources.{no-resolver, unresolvable-reference, invalid-reference-argument}`
([§dd-dr:input-wiring]);
`latexlike.environments.*` ×3. Segment policy: keep segments unchanged
(self-descriptive when quoted alone). The preset→core re-homing rider was
verified empty.

Revisit if: the soft-freeze condition of [§dd-dr:stability-rubric] arises; or a
downstream language needs to re-namespace an inherited condition (that would need a
deliberate identifier-mapping design, not an ad-hoc exception).

#### `Diagnostics::sorted_by_position()` — narrow, source-major [§dd-dr:diagnostics-position-sort]

Status: DECIDED (user, API review).

Diagnostics arrive in recovery order, not source order; `sorted_by_position()`
(returning-adjective form) is a borrowing view (`-> Vec<&Diagnostic<O>>`; the
collection itself keeps recovery order) with a stable sort — equal positions keep
recovery order — by (source in first-appearance order, span start),
documented as source order *within each source*. Narrow by design: a total "position
order" is ill-defined across multi-source parse trees, which are first-class
([§dd-dr:input-attachment]). Both `IntoIterator` impls already exist — the
walkthrough claim to the contrary was a doc gap, not an API gap.

#### Runtime condition identity: the defaulted `identifier()` override for embedding adapters [§dd-dr:runtime-condition-identity]

Status: DECIDED (user; a scoped relaxation of the const-identifier discipline).

`DiagnosticInfo` carries a defaulted method `fn identifier(&self) -> &str {
Self::IDENTIFIER }`, and the sealed `DiagnosticData` blanket impl forwards
through the *method* rather than the const. Overriding it is documented as the
exceptional case where a compile-time identifier is impossible: binding/embedding
**adapter** types — one Rust type carrying conditions defined at runtime in a
host language (e.g. Python-defined conditions), each instance answering its own
identity. Everything else keeps the const: `IDENTIFIER` remains required and
remains the type's own identity, typed matching via `T::IDENTIFIER` is untouched,
and the const-identifier discipline stays the norm
([§dd-dr:condition-identities]); adapter-minted identifiers fall under the same
namespace and stability rules as any downstream vocabulary
([§dd-dr:wire-identifier-stability]).

Rejected alternatives: unsealing `DiagnosticData` (an adapter would bypass
`DiagnosticInfo` entirely — losing the blanket impl's `clone_box` coherence and
the one-type-one-identity story; unsealing is also one-way — re-sealing is
breaking — while the defaulted method is additive and narrow); one wrapper
condition type per foreign condition (foreign conditions are runtime data; there
is no Rust type per condition to write); lang-dependent identifiers everywhere
(rejected in [§dd-dr:condition-identities] and still rejected — this entry
changes the *instance* channel for adapters only, not the identity model).

Accepted cost: downstream code with both `DiagnosticInfo` and `DiagnosticData`
in scope finds an unqualified `.identifier()` call on a *concrete* condition
type ambiguous (E0034) where it previously resolved uniquely; the qualified
spellings (`DiagnosticInfo::identifier(&c)` / `DiagnosticData::identifier(&c)`)
resolve it.

Revisit if: overrides appear outside binding/embedding adapter types (the norm
is eroding — tighten the docs or reconsider the seam), or runtime-minted
identifiers need collision policing across embeddings (that would need a
deliberate namespace-registration design, not an ad-hoc exception).

## Serialization [§dd-dr:serialization]

The decisions behind `techy::serialize` ([§dd-arch:serialization]). The serialized
form itself is described in `dev-docs/serialize_schema.md`; the user-facing contract
is the `techy::serialize` rustdoc.

#### Capability traits: supertrait write dispatch, registry read dispatch [§dd-dr:serialize-capability-traits]

Status: DECIDED (user-led design sessions).

Serialization is a capability of the object, not a derive on the live type: the write
half is `SerializableObject`, a defaulted supertrait of `CallableSpec` and
`SpecsProvider` (so it is reachable through the trait objects trees hold — the concrete
type behind `Arc<dyn CallableSpec>` is unknown at write time — and needs no
registration; a non-participant writes an empty impl); the read half is the opt-in
`DeserializableObject` on concrete types, dispatched by identifier through readers a
language registers once per session plus prefix-keyed resolvers for open type sets
(fail-closed: an unknown identifier is an error). Embedded values use the parallel
`SerializableValue`/`DeserializableValue` pair, and `SerializableLang` — an item-less
trait whose bounds require both of every type a language supplies — gates the whole
capability per language: contexts exist only for such a language, so the methods are
statically uncallable for any other. Decisive reasons: serde derives on live types are
impossible (trait objects everywhere, `Arc` sharing is semantic) and would couple the
schema to internal layout; a per-type write registry buys nothing once objects
self-describe; the trait surface is unconditional, so enabling the `serde` cargo
feature adds no obligation to any implementer (feature additivity).

Rejected alternatives: erased-serde/typetag (dependency, link-time registries, Rust
type names as wire identity); a `SerializableObjects` `Lang` feature (conditional
supertraits are inexpressible; no data layout changes with presence); cargo-gated
trait surfaces (additivity violation); closure-pair registries (`ser_fn`/`de_fn`) as
the public registration API; write-side resolvers (a compile-time closed type set —
a downcast list in disguise); a public `ToSerialValue` derive for implementer payloads
(the serde bridge serves that; the internal derive stays for core wire structs).

Revisit if: a concrete need for write-side dispatch by something other than the
object's own type appears (the reader-entry design keeps a write resolver chain purely
additive).

#### Instance, not lookup: identity through `Weak` provenance stamps [§dd-dr:instance-not-lookup]

Status: DECIDED (user-led design sessions).

Serialization captures the object the parser actually got — never "how to look it up
again". A spec that holds parsers is written **by identity**: a `SpecProvenance` stamp
(`Weak<dyn SpecsProvider>` + callable type + definition key), handed out by
`Package::new_shared`, becomes a reference to the provider's entry plus the key,
resolved on reading in the reader's own environment (`KnownProviders`: held packages by
name, recipes for the rest) to the very instance that package holds; self-contained
types write a constructor recipe. Decisive reason: a lookup (`retrieve_spec`,
specials scanning) is a parse-time event whose answer may legitimately differ later or
without the token — the `\today` case — so re-running it, even after verifying it at
write time, validates today's answer, not read-time validity. `Weak` because a strong
back-reference would close an ownership cycle with the package's spec `Arc`s; a stamp
is process-local and never wire material.

Rejected alternatives: symbolic re-query through the rehydrated scope stack;
enumeration-based reverse maps over `iter_symbols` (enumeration is not a lookup
contract); parser-recorded resolution provenance (parse-time truth, hot-path cost);
verified write-time replay; an eager "known-objects map" (O(environment) setup for an
O(stream) need).

Revisit if: this principle is ever relaxed — most of the rejected routes come back
with it. Don't.

#### The value model and the canonical-rendering discipline; the feature gates rendering only [§dd-dr:serial-value-model]

Status: DECIDED (user-led design sessions; `Map` order and the `$`-key rule ruled by
the user).

`SerialValue` holds exactly what the canonical JSON rendering round-trips
distinguishably: null, bool, `i64` (every integer width; out of range is an error),
string, bytes (pinned base64 form), list, ordered string-keyed map (equality
order-sensitive, keys never beginning with `$` — reserved for `$bytes`/`$index`, no
escaping), and a table reference — no floats, no sized-int variants, so two values
render identically exactly when they are equal (golden files, content-addressed caches,
dedup). Nesting is bounded (`MAX_NESTING_DEPTH` = 64, enforced at every read rim and
on the writer). The `serde` cargo feature gates **rendering only**: the value model,
the engine, and the capability traits are dependency-free plain Rust; core wire
structs convert through an internal derive, implementer payloads through the serde
bridge (`to_value`/`from_value`); an absent `Option` field is an omitted key in both.

Rejected alternatives: floats and sized ints in the value (collapse in the rendering
→ unequal values with identical bytes); a `$$` escape for user `$`-keys (a typed error
instead); a public schema document maintained by hand (the wire structs are the
schema; the description is generated/checked from them).

Revisit if: a consumer needs a numeric type the model cannot hold without breaking the
canonical-form law (a string encoding is the expected answer), or a format without its
own recursion limit needs a different bound.

#### Session-scoped positions, segments, and streams [§dd-dr:serialize-sessions-segments]

Status: DECIDED (user-led design sessions; segment envelope, `main`, and `profile`
ruled by the user).

One `SerdeSession` type serves both directions: it interns by `Arc` identity into
per-kind tables (write once, share on read), emits **segments** (the entries new since
the previous emission, every table in a directory by name and writer-side id, a
version in every segment, an optional main entry, a caller-declared profile in `meta`),
and absorbs them in order (validated as untrusted input; translated by table name;
absorb-all-then-append). Positions are stream-scoped on the wire and session-scoped as
typed values in Rust code (a typed position carries its session's `TableId`; between
sessions a position travels as table name + `u32`). Decisive reasons: the cache use
case (read yesterday's stream, append today's parses without rewriting what is held)
and the sharing law (identity must survive across trees in one stream) fix the design;
JSON Lines with a version per segment makes every line an independently valid value.

Rejected alternatives: separate reader/writer types (reading then appending is the
natural flow); a version in the first segment only (a split or truncated stream would
lose it); an end-of-stream marker (appending would need rewriting); a stream-identity
field (deferred: the caller's obligation, narrowed by the profile).

Revisit if: a use case needs enforced stream identity or a reader for older layout
versions (a version bump then comes with a read-old/convert/refuse policy).

## Dependencies [§dd-dr:dependencies]

#### Absolute minimal mandatory dependencies [§dd-dr:minimal-dependencies]

Status: DECIDED (user).

`thiserror` and `log` are removed; the runtime is dependency-free. The considerations
that led there:

- **`thiserror`.** It generates exactly the `Display`/`Error` impls one would write by hand —
  zero runtime difference either way. The trade is: dropping it removes a proc-macro dependency
  chain (`syn`/`quote`/`proc-macro2`) from every downstream user's cold build and makes the
  crate dependency-free on paper; keeping it saves hand-written boilerplate (~1 match arm per
  error variant for `Display`, plus `impl Error`). For a parser library the error surface is
  small and stable, so the boilerplate is bounded (tens of lines, written once) — this is *not*
  the "reimplementing a library" kind of bloat, it's writing out what the macro expands to.
  But the user is right that it's a real authoring cost with zero runtime benefit. Genuinely a
  judgment call about dependency hygiene vs. convenience.
- **`log`.** Different situation: the question is not "which logging dep" but **whether a
  library should log at all**. The only current use is one `warn!` (source too big to compute
  line info) — information that rightly belongs in the API's return values or the diagnostics
  sink ([§dd-dr:errors]), where callers can actually react to it. A library that communicates through its
  API needs no logging facade. Dropped (feature-gate later if a concrete debugging need
  appears).
- Not under discussion: heavier deps. Nothing in the design needs regex, serde (could be an
  optional feature later), or unicode tables beyond `char` methods.

Amendment (performance review): a dependency may be added in very specific, exceptional
cases — widely used, lightweight, `no_std`-capable crates, decided case by case. First
(and so far only) instance: `hashbrown` (the implementation inside std's own `HashMap`),
backing the engine's derivation memo ([§dd-dr:memoized-derivations]); `no_std` has no
`std::collections::HashMap`, and a hand-rolled map would be worse on every axis.

#### `no_std`-friendly, alloc-only [§dd-dr:no-std]

Status: DECIDED (user).

The library must build without
`std`; allocation is fine (`#![cfg_attr(not(test), no_std)]` + `extern crate alloc` in
`lib.rs`; tests keep `std` for convenience). Consequences: no I/O anywhere in the library —
the file-reading `FileResolver` was removed (an embedder implements `SourceResolver` where
the I/O capability lives), and the `File` origin kind fell with it (see [§dd-dr:sources-and-spans]); `alloc`
collections only (`MapResolver` uses `BTreeMap`, not `HashMap`); error types implement
`core::error::Error`, which sets MSRV 1.81 (`rust-version` in `Cargo.toml`); `Arc` comes
from `alloc::sync`, so targets must support atomics. A plain `cargo build` compiles the
library with `no_std` active and thus guards the policy without a bare-metal CI target.

#### Map containers after hashbrown (`BTreeMap` vs `HashMap`) [§dd-dr:map-containers]

Status: DECIDED in part (user + discussion).

hashbrown entered the tree for the engine's `StateMemo` (which needed its `Equivalent` borrowed-key seam); that does not make it the default map. Choose per map by
use: `MapResolver` and the `CallableTypeId`-keyed maps stay `BTreeMap`; `Library`'s inner
name→spec map is the one hash-worthy candidate (string keys, one lookup per callable
invocation, potentially hundreds of entries) but is deferred to the planned structural revisit
of `library`.
Rationale: the `CallableTypeId`-keyed maps hold a handful of entries, where a `BTreeMap`
lookup is one or two integer comparisons — hashing gains nothing. Two non-obvious costs of
hashing to weigh whenever this is reopened: (a) iteration order becomes nondeterministic and
varies per process (hashbrown's default foldhash seeds from a static's address under
`no_std`), which any future "list defined names" API or snapshot test would inherit — sort at
the boundary if so; (b) `no_std` hash seeding has no OS entropy, so if untrusted documents
ever *insert* into a map (e.g. `\newcommand` definitions into a runtime library), collision
DoS becomes theoretically possible, whereas `BTreeMap` guarantees O(log n) worst case.
*Also decided:* public APIs must not name a concrete map type — `MapResolver`'s
`From<BTreeMap<String, String>>` was generalized to `From<I: IntoIterator<Item = (String,
String)>>` so the backing container stays an implementation detail; exposing
`hashbrown::HashMap` in a signature would couple the public API to hashbrown's 0.x semver
churn (0.14→0.15 already swapped default hashers).
Revisit if: profiling flags `Library` name-lookup cost, or the `library` structural revisit
lands.


## Naming [§dd-dr:naming]

The durable naming principles (generic over specific, specificity over brevity, the
systematic Id-naming rule, `make_*` factories, and the rest) live in ARCHITECTURE
[§dd-arch:naming]; this topic records naming decisions and their reasons.

Decided conventions:

- **No `Latex` prefixes** — the library is markup-generic (`Token`, not `LatexToken`).
- **Specificity over brevity** where confusion is possible: `ParsingStateDelta` not
  `StateDelta`.
- **Collision avoidance beats tradition**: `Language<L>` replaces the early
  `FLMEnvironment` (fatal collision with `EnvironmentSpec`/`EnvironmentNode`);
  `ConstructParser` avoids clashing with any high-level `Parser` type; `Lang` replaces
  `LanguageSpecification` (too long for a parameter appearing in nearly every signature).
- **Session rulings**: `ConstructParserResult<T>` (= `Result<T, ParseError>`) over the
  sketched `ParseOutcome` — unambiguous next to the engine-level `ParseResult`; clarity
  over brevity. `NodesParser` over `ContentParser` — the regions session gave "content"
  a precise technical meaning (`ContentNodes`, designated argument/slot content) that a
  general nodes parser has nothing to do with. `StopCause` for the parser-returned ending
  cause; `Invocation` for the resolved-invocation value; `make_*` for factory hooks
  (`make_invocation_parser`, `make_paragraph_break_node`).

When naming something new: check the naming principles ([§dd-arch:naming]), then ask
"does this collide with or shadow an existing concept in LaTeX terminology or in this
codebase?"

#### `ParsedArguments`, not `Arguments`: a context-principle reversal [§dd-dr:parsed-arguments-naming]

Status: DECIDED (user; reverses the earlier "context makes the qualifier redundant"
call — a recorded reversal, July 2026).

The original ruling (Dec 2025) chose `Arguments` over `ParsedArguments`: context — a
field on parsed nodes — made the qualifier redundant. Reversed at the current-level
review: the spec-side argument vocabulary (`ArgumentSpec`, `ArgumentParser`) now coexists
wherever the parsed records appear, so the parsed-side types carry the distinguishing
prefix; pylatexenc parity favors `ParsedArguments` too. The lesson generalizes into the
principles: "context determines names" holds only while no sibling vocabulary competes
in the same scope.

#### Superseded names — do not reintroduce [§dd-dr:superseded-names]

Status: DECIDED (distilled from the archived NAMING_STRATEGY.md registry; the full
old-to-new table stays there and in git history).

Names that look natural and were consciously rejected or replaced — reintroducing one
re-opens a settled argument:

- `LatexToken`, `LatexWalker`, `LatexNode`, … — no `Latex` prefixes in the core; LaTeX
  names live in the preset.
- `FLMEnvironment`, `LanguageSpecification` — replaced by `Language<L>` and `Lang`.
- `ContextDb`, `LibrarySet`, `ModeContext`, `ConflictStrategy` — definitions are
  `SpecsProvider`/`Package`/`Scope`/`ScopeStack` with lexical shadowing; no flat
  namespace, no mode tables, no conflict policies ([§dd-dr:lexical-shadowing],
  [§dd-dr:scope-stack]).
- `SpecLookup`, `Library`, `LibraryStack`, `push_libraries`, the `library` module — the
  first-generation definition vocabulary, fully replaced by the scope-stack redesign.
- `TokenType`, `TokenKind::Macro`/`MacroRules`, `CommentStart`, maximal-run `Chars` —
  the token model's rejected shapes ([§dd-dr:token-model]).
- `MathNode`, `MacroNode`/`EnvironmentNode`/`SpecialsNode`, a `Custom` node variant —
  the closed `NodeKind` with `Callable` covers them ([§dd-dr:closed-node-kind],
  [§dd-dr:no-core-math-node]).
- `StateDelta` (trait), `apply()`/`copy_with()`, per-facet state traits — deltas are
  reified `ParsingStateDelta` values applied via `derived()` ([§dd-dr:state-option-c]).
- `ArgumentKind`, `ArgumentStructureSpec`, `ArgsLayout`/`SlotsLayout`, `SlotSpec` — the
  argument model's superseded shapes ([§dd-dr:argument-parser-model],
  [§dd-dr:no-spec-side-slots]).
- "namespace" / `CallableKind` — `CallableTypeId` (the Id-naming rule); `GroupExt` /
  `NodeGroupExt` — `GroupNodeExt`.
- `Parser` (trait), `ContentParser`, `ParseOutcome` — `ConstructParser`, `NodesParser`,
  `ConstructParserResult`.
- `util` (public module), `parsing` (as public namespace name), `definitions`/`defs`
  (as the core specs-group name), a central `conditions` registry module — the public
  export topology's rejected names and shapes ([§dd-dr:public-namespace-topology]).
- `ParsingState::initial()` — renamed `lang_initial()` (the seed is the *Lang's* notion
  of the initial state; [§dd-dr:language-init]); `latexlike::defs` (as a module name —
  overclaims; the toy package module is `minidefs`, [§dd-dr:minidefs]).
- `MathStyle` / `NodeRef::math_style()` — renamed `MathGroupForm` / `math_form()`
  ("style" collides with typesetting style: `$\displaystyle …$` is display-*style*
  math in an inline-*form* group; [§dd-dr:math-group-form]); with them the
  `MATH_DELIMITERS` table — dissolved into `default_token_rules` (the form is
  declared class payload, never read infrastructure).
- `InitialStateDataProvider` / `StateTransitionFinalizer` / `SpecialsProvider` /
  `NodeFinalizer` (a facet-decomposed `Lang`), `Latexlike<X: LatexlikeExt>` (the
  plugin-slot preset) — weighed and rejected during preset generalization
  ([§dd-dr:latexlike-generalization]).
- `finalize_node` (and the interim names `populate_ext`/`populate_node_ext`) — the
  parse-once minting hook is `make_node_ext`; the tier-2 per-kind ext family
  (`CharsNodeExt`…`ListNodeExt`) and the `NodeDataExt` parallel bundle — removed
  outright; `NodeTreeBuilder::for_parsing()` — the rejected hook-firing builder
  mode; `add_with_ext` — the `add`/`add_with_ext` pair folded into the single
  six-parameter `add` at application ([§dd-dr:ext-minting]).
- `ProcessedNodeData` — the annotation parameter's working name (collides with
  `NodeData` in the same scope; the vocabulary is *annotations*,
  [§dd-dr:node-annotations]); `tree_identifier` — the tag term is `tree_tag`
  ([§dd-dr:tree-tags]).
- `WithTransformedTreeNodeProvenance`/`WithOriginalNode` (the rejected
  auto-provenance trait), `add_subtree`/`copy_subtree` and "copy" as transform
  vocabulary — restaging ([§dd-dr:restage]); node-level cross-tree tracking says
  *original node* — never "provenance"/"origin", which belong to the source model
  (`SourceProvenance`/`SourceOrigin`).
- From the API review: `"base"` and `base_package()` — the seed
  package is `"_builtin"`/`builtin_package()` ([§dd-dr:base-package] amendment);
  minidefs fn name `package()` — it is `minilatex_package()`; `NodeKind::label()`/
  `kind_as_string()` — the accessor is `as_str()` ([§dd-dr:display-tree]);
  argument-code names `GroupOnly`/`StrictGroup` — the code is `BracedOnly`;
  `with_body_provider` (rejected abandoned-at-first-need sugar),
  `text_mode_argument()`/`text_argument_state_delta()` (text restore is an event,
  not a factory; [§dd-dr:argument-factory-additions]); as *shapes*: per-`GroupRule`
  mode visibility and a `ParsingState` parent pointer
  ([§dd-dr:enclosing-state-stack]).
- From the API review: `SimpleLang` — renamed `TrivialLang` ("Simple"
  over-promised an on-ramp; [§dd-dr:trivial-lang]);
  `CommandResolution::resolve_via_scopes` (the associated-fn spelling, and the
  interim `resolve_command_via_scopes`) — the extracted resolver is
  `resolve_command_in_scopes` ([§dd-dr:resolution-extraction]);
  `restore_text_context_delta` — the pillar is `exit_math_context_delta`
  ([§dd-dr:enclosing-state-stack] amendment); role-accessor spellings
  `r#macro()`/`macro_()`/`macro_kind()`/`macro_type()` — the family is
  `macro_callable()`/`environment_callable()`/`specials_callable()`;
  `text_mode()`/`is_text()` — trimmed from the mode role trait
  ([§dd-dr:latexlike-generalization]); constructor names `neutral()`/
  `disabled()` — the empty starting values are `TokenRules::empty()`/
  `StateData::empty()`; `all_off()`/`raw()` — the gate-off overrides value is
  `disable_all()` ([§dd-dr:on-ramp-defaults], [§dd-dr:takeover-staging-sugar]);
  `new_anonymous` — the unnamed-constructor spelling is `new_unnamed`; the
  `ArgumentSpec::named()` builder and `ParsedSlot::named()` constructor — names
  move into `new(…, name)` ([§dd-dr:named-first-constructors]);
  `ScopeResolvingDriver`/`ScopesDriver`/`StdScopeDriver` — the component is
  `ScopesResolvingDriver` ([§dd-dr:scopes-resolving-driver]).
- From the API review: `techy::helpers` (a recipes module — the `util`
  problem under another name; placement stays by logical function);
  `resolution` as a wire-identifier area (the area is `specs` — "resolution of
  what?") and the file-named areas `nodes_parser`/`environment_parser`/
  `argument_parsers`/`verbatim_parser`/`group_parser`/`tack_on_parser` (the
  frozen slate; [§dd-dr:wire-identifier-stability]);
  `ancestors()`/`Ancestors` (rejected — `parent()` + `iter::successors`;
  [§dd-dr:tree-navigation]); `Descendants::with_depth()` (patched flat
  iteration's structure loss at the wrong layer — the read walker belongs to the
  recompose session; [§dd-dr:recompose]); `NodeRef::line_col()`/
  `SourceSpan::line_col()` and `LineIndex::line_range(line_no)`
  (rejected/skipped — [§dd-dr:line-col-ownership]); `LineIndexCacheProvider` —
  the seam is `LineColProvider` (provides answers, not caches);
  `cx.parse_source` as the sub-parse door name (collides with
  `Language::parse_source` under a different contract — the door is
  `parse_attached_source`; [§dd-dr:input-wiring]).
- From the API review: `stage_argument_like` — the content-replacement
  helper is `restage_argument_with_content` (+ the `_slot_` twin;
  [§dd-dr:restage-ops]); `Restage::Continue`/`Keep`/`Retain`/`Auto` — the
  variant is `Descend` ([§dd-dr:restage]); `StateStackView`/
  `StateStack` — the owning type is `ParsingStateStack`
  ([§dd-dr:enclosing-state-stack]); `Split` — the split result type is
  `SplitAtChars`, and the interim `_with_annotations` spellings — the general
  callback form owns the bare producer name, shorthands carry
  `_drop_annotations`/`_keep_annotations` ([§dd-dr:extract-annotations]);
  `copied_from()` — the part-context accessor is `original()` (partials are cut,
  not copied); `check_transform_tree_invariants` and the withdrawn
  `validate_parse_tree` — the runtime checker is `validate_tree`
  ([§dd-dr:tree-validation]).
- From the API-review recompose session: `CallSyntax` (a proposed fourth
  `SlotRole`) and the `"begin_tokens"`/`"end_tokens"` `Hidden`-slot scaffolding
  storage — rejected outright: trigger spelling is invocation-syntax *payload*
  ([§dd-dr:invocation-syntax]); with them `escape_char` as a core `CallableData`
  field (rejected), and `post_space` as a core `CallableData` field — the fact
  moves into the invocation-syntax payload; "span-verbatim" — retired as a
  strategy name (no named span strategy exists; [§dd-dr:recompose]);
  the canonical-`"\n\n"` paragraph-break `name` — superseded by name-as-written +
  spec-identity identification; `CallableNodeInvocationSyntax` — the payload
  type is `InvocationSyntaxData` (named `InvocationSyntax` until the
  recorded role swap, see below); `new_for_invocation` — the constructor
  trait/method is `FromInvocation`/`from_invocation`; `Bit`/`ComposeBit` — the
  piece vocabulary is `Piece`/`ComposePiece` (`Fragment`/`Part` recorded
  considered; `Output` rejected — collides with `ConstructParser::Output`);
  `ConcatSpec` ("Spec" is author-side vocabulary) and the interim `ConcatParts`
  — the instruction payload is `ConcatPieces`; `VisitCx`/`RecomposeCx` —
  spelled-out `VisitContext`/`RecomposeContext` (the `ParseContext` convention);
  `walk_tree`/`recompose_tree` — rejected on one-canonical-path (`visit::walk`,
  `recompose::recompose`; [§dd-dr:visit-engine],
  [§dd-dr:recompose-machinery]).
- From the API review: `ScopesResolvingDriver` — the component
  struct is replaced by the strategy parameter
  (`StdParseDriver<ScopesCommandResolver<…>>`; [§dd-dr:command-resolver]), and
  `NoCommandResolver` — the no-op command resolver is `()`; `resolve_source`
  (the free fn) — renamed `resolve_source_reference`, and the mechanism-first
  candidates `delegate_resolve_source`/`call_resolve_source` (a verb name says
  what the caller gets, not the internal wiring; [§dd-dr:input-wiring]); `with_resolver` (the shipped-driver builder) —
  `with_source_resolver`; `NoResolver` — removed entirely (`None` is the
  canonical "resolves nothing"; [§dd-dr:source-resolver]).
- From the invocation-syntax design revision ([§dd-dr:invocation-syntax]) —
  **role-swap pins**: `InvocationSyntaxData` as the *core
  bound-trait* name, and `InvocationSyntax` as the *latexlike payload-enum*
  name — both names survive, with swapped roles (the trait is the
  L-parameterized `InvocationSyntax<L>`, the enum is the data holder
  `InvocationSyntaxData<Env>`); the old role assignments must not return.
  Also: `EnvironmentSideSyntax` (bare) — the std record's component type is
  `StdEnvironmentSideSyntax`; `EnvironmentTerminatorFacts` — the end-side facts
  type is `EnvironmentTerminatorSyntaxData`; `parse_begin`/`parse_end`/
  `record_std_end_facts` as `EnvironmentSyntax` method names — the record
  contract is `from_parsed(begin, terminator)` plus the writer pair, scanning
  is composition-owned; `recompose_environment` (a single fused environment
  writer) — rejected shape: the writer PAIR stays (`Concat` head/tail and
  the span-tiling law's prefix/suffix pins need the sides separately).
- From the language-init revision ([§dd-dr:language-init]):
  `Language::with_provider`/`Language::with_seed_delta` — seed customization
  moves *before* construction (`ParsingState::lang_initial().derived(&delta)?`;
  packages via the infallible `lang_initial_with_packages`); `Default for
  Language<L>` / `LatexlikeDriver::default()` / `StdParseDriver::default()` —
  removed (an implicit seed by the back door, and the recovery knob — the
  driver's one mandatory policy input — must be an explicit `new` argument).
- From the lang-features design session ([§dd-dr:lang-features]): `Gate` (trait)
  with `On`/`Off` markers — an earlier sketch's spellings colliding with the
  runtime "feature gate" vocabulary (the `enable_*` flags), fusing the two axes
  the absent/disabled word split keeps apart; bare `Present`/`Absent`/`Has*`/
  `Features` spellings — too generic for the flat `techy::core` hub ("present
  *what*? features *of what*?"; the ban covers *standalone item names* in the
  hub — trait-scoped associated types like `Lang::Features`, already qualified
  by their trait, are excluded); the adopted names are
  `FeaturePresent`/`FeatureAbsent`/`LangHas*`/`LangFeatures`; and "facet" as
  public vocabulary — banned from all public names and documentation (internal
  shorthand only; user ruling).
- From the descent-guard design session ([§dd-dr:descent-guard]): `parse_scoped` —
  superseded by **`parse_construct`**, the single descent entry point (the scoping
  role without descent obligations is `with_parsing_state`); and "Stack"-only
  names for the guard family — they collide with the crate's data-stack
  vocabulary (scope stack, frame stack, enclosing-state stack); the vocabulary is
  *descent* (`DescentGuard`, `DescentLimitExceeded`, `DescentLimitApproaching`).
- From the serialization design ([§dd-dr:serialization]): `SerializedIndex` — the
  driver's associated type is `Index` and its bound `SerialIndex` (the `Serial*`
  wire-data family; `…Index` = a table position, neither a `…Ref` handle nor a
  process-local `…Id`); `resolve` as the read accessor — positions are read with
  `object`/`tree`/`parse_result` ("resolve" is source- and command-resolution
  vocabulary); `ReadEntry` — the read-side unit is `ObjectReader`; the `$$` escaping
  of user map keys beginning with `$` — a typed error (`ReservedMapKey`) instead;
  `Document` (type) and "document" as public vocabulary for the parsed content — FLM
  owns the word; the units are *segment* and *stream*, the content is
  "text"/"input"/"source"; the verbs dump/load/revive/resurrect — the public verb
  pair is serialize/deserialize; `register_all()`/write-side registration and
  `SerializableObjects` as a `LangFeature` — objects self-describe, the lang gate is
  `SerializableLang` ([§dd-dr:serialize-capability-traits]); a public `ToSerialValue`
  derive — the serde bridge ([§dd-dr:serial-value-model]).
- From the token-layer redesign ([§dd-dr:token-opacity], [§dd-dr:stream-position],
  [§dd-dr:no-context-source]): `Token<'s, L>` as a struct with a lifetime — the token
  type is `StdToken<L>`, opaque and lifetime-free, and `Token` now names the type
  **alias** `Token<L>` ([§dd-dr:tokenization]); with it `Token::new` — one constructor per kind
  (`StdToken::char`, `group_open`, …); `TokenKind` variants carrying spans — the
  stored-token field names `TokenKind::Comment::{start, post_space}` and
  `TokenKind::Command::post_space` (the view carries written spellings, and a span is a
  reader answer between two `TokenEdge`s), and the interim view name `TokenKindView`
  (the view *is* `TokenKind`); `TokenReader::{pos, move_to_pos, move_past,
  move_to(&token, bool)}` and `StdTokenReader::{pos, move_to_pos}` — the two moves are
  `move_to(&token, edge)` and `move_to_position(&position)`, and the interim
  `move_to_edge`; `TokenRecovery::resume_pos` — the field is `resume`, a stream
  position; `ParseContext::source` — there is no source handle;
  `stage_invocation(.., end_pos: Option<usize>)` — the end is `Option<&L::StreamPosition>`;
  `SpecialsMatch<'s, L>` and `SpecialsMatch::name` — the name is the matched text;
  `TokenResult<'s, L, T>`, `Invocation<'a, 's, L>` and `CallableQuery<'a, 's, L>` — none
  of the three carries a content lifetime any more; `CallableQuery::token` +
  `CallableQuery::with_token` (a token handed to a party with no reader to read it with)
  and their view-carrying successors `CallableQuery::token_kind` +
  `CallableQuery::with_token_kind` and `resolve_command(.., token_kind: TokenKind)` — the
  resolve chain receives `(&L::Token, &dyn TokenReader<'_, L>)` and the query carries
  name and callable syntax only; `Invocation::kind` — a cached view beside its own
  token; bare-view hook signatures for `TokenStopKind::Predicate` and
  `GroupChildState::Compute` — both take the token and its reader;
  `resolve_command(.., token: &Token)` without the reader;
  `make_paragraph_break_node(.., token, source_content)` — the hook takes the break's
  `SourceSpan`; and `probe_token(.., source, ..)` — no source parameter.
- From the tokenization-bundle change ([§dd-dr:tokenization]): the `Lang` associated types
  `Lang::Token` and `Lang::StreamPosition` — a language declares one `Lang::Tokenization`
  instead, and the two types are read off it as `Token<L>`/`StreamPosition<L>`; the marker
  **trait** `Token<L>` (`pub trait Token<L: Lang>: Clone + …`) and the considered rename
  `TokenBase` — the bounds sit on `Tokenization::Token`, and the name `Token` belongs to
  the alias; a `Lang::TokenReader` associated type (a reader *type* on `Lang`) — the
  language names the lifetime-free bundle, whose factory returns the `dyn` reader.
- From the `core::token` extraction and the scan-helper family
  ([§dd-dr:core-token-facade], [§dd-dr:scan-helpers]), the seven private
  `StdTokenReader` member names retired when their logic became public: `scan_token_at`
  — the promoted method is `scan_std_token_at` ("std" says the token it answers is a
  `StdToken<L>` for any `L`); `token_kind_of` — `token_kind_of_std_token`, which keeps
  the `token_kind` stem of the trait method it implements; `detect_paragraph_break` and
  `detect_group_delimiter` — the free `scan_paragraph_break` and
  `scan_group_delimiter` ("scan" is the family's verb for recognizing at a position
  without moving); `read_command` and `read_comment` — `scan_command` and
  `scan_comment`, for the same reason ("read" suggested the advance the helpers never
  make); `lift_specials_scan_error` — the private span-validating half is
  `checked_scan_error`, and the specials step a reader calls is
  `scan_specials_trigger`, deliberately not named after the `Lang::scan_specials` hook
  it wraps.
- From the span-tiling declaration ([§dd-dr:span-tiling]), three phrases: "partition
  invariant" as the name of the sibling-span property — the property is **span tiling**,
  and a tree with it is **span-tiled**; "in-order, gap-free token contract" as the name of
  the chars-run position check — that check verifies contract clauses 2 and 7 (a peeked
  token starts where the peek happened; moving sets the position), while reading order and
  gaps are clause 8; and "parse-tree law" — the test-only byte-accounting oracle is **the
  span-tiling law**, beside the all-trees law of `validate_tree`, and its in-crate helper is
  `check_span_tiling_node`. No mode name is coined
  for the other regime either: documentation says "a language with `OBEYS_SPAN_TILING =
  false`".

## Crate organization and dependency model [§dd-dr:crates]

#### Three strata + three rules replace the strict L0–L7 layer ladder [§dd-dr:three-strata]

Status: DECIDED (user-led).

S0 *foundation* (Lang-free, a true DAG: source, error/diagnostics, `Span`/`Token`/`TokenKind`,
`TokenRules` + `PrefixTable` + the concrete scanning core, `TextContent`); S1 *core* (a single
mutually-recursive stratum: `Lang` + `NodeExtTypes`, state, spec/library, node, constructs,
engine — modules are topics for navigation, not dependency ranks); S2 *presets*. Three
enforced rules: (1) S0 never names `Lang` (import-checkable); (2) S1 never names a preset
(import-checkable); (3) the runtime ownership graph is acyclic — nodes → {states, specs,
sources}; states → specs; specs → parsers; sources → sources; no runtime value references
nodes (field-inspection-checkable).
Rationale: the discussion started from "`Language<L>` is listed at L6 but its information is
needed at L1/L2" (answer: `Language` only *seeds* the initial `ParsingState`; the hot loop
reads materialized state) and ended with the decisive observation that the middle layers form
a strongly-connected component **by intention** — every cycle edge is itself a decided
feature: state stores libraries (`\newcommand`), lookup takes the state (mode-aware
`SpecLookup`), specs carry their invocation parser (the pylatexenc escape hatch), parsers
build nodes and derive states, nodes record their parse-time state and spec. Hence no
renumbering could restore a DAG, and "L3 shall not use L5" was a law the design already
violated deliberately. The confusion dissolves once three graphs the ladder conflated are
separated: the *type/signature* graph (cyclic inside S1, harmless — traits are signatures,
`dyn` references tie the knot, cross-module cycles within one crate are idiomatic Rust); the
*runtime ownership* graph (must stay acyclic — rule 3, generalizing [§dd-dr:sources-and-spans]'s
sources-never-reference-nodes invariant); and the *build order* (which sequenced concrete machinery DAG-shaped even where
signatures are mutually recursive — the tokenizer first ran against a hardcoded
`TokenRules`). Within S1 the useful distinction is by
*role* — data / contracts / standard machinery / orchestration — not by rank.
Consequences worth pinning:
- `TokenRules` and `PrefixTable` are *defined* in the token topic and merely *stored*/cached
  by `ParsingState`. *(Later revised, token-design review: the token topic is wholly
  **S1** — tokens are generic over `L` (`Specials` carries its spec) and token errors may
  grow state context; the earlier "scanning core is S0" split is superseded, and `Span`
  moved to the source topic. S0-testability was traded for state-context freedom; a trivial
  test `Lang` restores it at negligible cost. See [§dd-dr:token-model].)*
- The `TokenReader<L>` trait keeps `&ParsingState<L>` (not `&TokenRules`) in `peek`:
  it is the documented catcode escape hatch, and such a reader keeps its tables in
  `L::StateExt`; narrowing to the rules would sever the escape hatch from language state.
- `Lang` and `NodeExtTypes` are defined in the core next to the state types
  (`finalize_transition` names `StateData`/`ParsingState`, fixing their home); `NodeExtTypes`
  does not move into `node/` despite its meaning being a node concern — that would recreate a
  cycle for cosmetics.
- `Language<L>` contributes at exactly one moment: seeding the initial state (default rules,
  base libraries, default ext) at session start.
Rejected alternatives: renumbering/reshuffling layers (no assignment makes an SCC a DAG); collapsing
everything into one stratum (loses the two boundaries that are real and checkable: the
Lang-free line and the preset line); moving `TokenReader` "up a layer" away from `Token`
(the trait/impl split by rank served no invariant and read as unnatural); narrowing the
`TokenReader` contract to `&TokenRules` to keep it "L1" (see above).
Revisit if: the crate is ever split into multiple crates — crate boundaries force true
DAGs; S0 is the natural split candidate, while S1 cannot be split along topic lines.

---

#### Repo layout: virtual workspace, every crate in its own subfolder [§dd-dr:workspace-layout]

Status: DECIDED (user-led).

The root `Cargo.toml` is a virtual manifest (`[workspace]` only); `techy` lives
in `techy/`, alongside `techy-derive/`, with a future CLI/instantiation crate as a third
sibling. Shared metadata (version, edition, `rust-version`, authors, license, repository) is
inherited via `[workspace.package]`; profiles live in the root manifest (the only place
they are honored). Rationale: the previous root-package layout (`techy` at the repo root
hosting `[workspace]`) is fine for "lib + satellite" (cf. thiserror, regex) but degrades at
three crates: root-level `cargo build`/`test` target the root package only, silently
skipping other members, whereas a virtual workspace targets all members by default; and
with the package root equal to the repo root, every repo-level file (ARCHITECTURE.md,
dev-docs/, TODO_Big.md, …) is a packaging candidate needing a perpetually-honest `exclude`
list — with a subfolder, `cargo package` ships exactly the crate's files. This is the
serde/tokio/clap layout, and serde is precisely our shape (lib + derive companion).
Non-obvious pitfalls pinned during the move: (1) a virtual root has no `edition` to infer
the dependency resolver from, so `resolver = "2"` must be explicit — v1 would unify
features across the no_std-leaning core and std-linking members; (2) the guide sources live in the
repository-root `docs/` (user-decided: workspace-level documentation, one home), pulled
in via `../../docs/…` includes — `#[cfg(doc)]` keeps the includes out of normal
builds, so `cargo package` verification passes, but a docs.rs build (which sets
`--cfg doc`) would miss the files: packaging the guide sources is an acknowledged open
point for the publish stage; (3) `readme = "../README.md"`
works from a subfolder (cargo copies it into the package). The CLI "linking std" is
orthogonal to layout — governed per-crate by features, not folder placement. Rejected alternatives:
keeping the root-package layout until the CLI lands (the move only gets more expensive);
a `crates/` super-directory (needless nesting at three crates; plain siblings suffice).

---

#### Public export topology: facades, one canonical path, hub + extracted subsets [§dd-dr:public-namespace-topology]

Status: DECIDED (user-led, API-review policy session).

The public API is exported exclusively through **re-export facades** — internal src
modules become private, so internal file organization is permanently invisible to
public paths — with **exactly one canonical path per item**, chosen by *logical
function/use* (never by frequency of use, never mirroring internal layout). Layout:

- `techy::source`, `techy::error` — the S0 data models, top-level.
- `techy::extract`, `techy::transform` ([§dd-dr:restage]), `techy::visit`
  ([§dd-dr:visit-engine]), `techy::recompose` ([§dd-dr:recompose]) — consumer tool
  libraries over node trees, top-level.
  The top level is thus *role-based*: data models and consumer tool libraries up top,
  machinery in `core`, preset in `latexlike`.
- `techy::core` — flat hub holding the mutually-recursive heart: `Lang`/state, engine
  (entry, result, sessions, drivers, command resolution).
- `techy::core::token` — the tokenization library (token and stream-position types, the
  `TokenReader` trait and the standard reader, the scan helpers, the rules data with its
  overrides and derived caches, the token conditions), extracted from the hub by
  [§dd-dr:core-token-facade].
- `techy::core::constructs` — the construct-parsing library (dispatch, standard
  parsers, their conditions).
- `techy::core::specs` — defining callables: callable specs, the argument model,
  providers/packages, scopes.
- `techy::core::node` — the node trees: reading, payloads, building.
- `techy::latexlike` — unchanged; presets namespace their own conditions.
- techy-derive emits only `::techy::__private::…` paths (serde discipline), removing
  the derive crate from all topology considerations.

The decisive structural argument: **extract only subsets with crisp boundaries; the
straddle families stay in the hub, uncut.** Since S1 is one mutually-recursive stratum
by decision ([§dd-dr:three-strata]), any public split is a navigation taxonomy, and the
items that resist assignment are exactly the decided cycle edges (conditions,
engine types, token data vs runtime, the argument model, `Lang` itself). The
hub-and-satellites shape keeps `Language::parse() → ParseResult` on one page, dissolves
every forced coin-flip a full partition creates, and pre-absorbs the known revision
candidates (spec+scopes now one public group; node read/build one group — required by
the transformation surface, which consumes the read side and produces through
the builder in one API).

**The specs/hub boundary rule (user-endorsed): `specs` is author-side — what you write
to define callables and organize definitions; the hub is run-side — state, tokens,
engine, resolution.** Known judgment calls at that interface (`FrameRole`,
`SearchedProviders`, `CallableQuery`) and the resolution family
(`CommandResolution`/`ResolvedCallable`): their ambiguity was read as a symptom
of wiring, not taxonomy — the standard command-resolution-via-scopes is extracted
into the standalone `resolve_command_in_scopes` (home: `specs`,
[§dd-dr:resolution-extraction]), and the resolution family rests naturally beside
that resolver. The `ArgumentParser` trait lives in `core::constructs`, beside
`ConstructParser` and the shipped argument-parser implementations (a parsing
contract; `ArgumentSpec`'s `Arc<dyn ArgumentParser>` is an accepted
cross-boundary signature reference). Cross-boundary
*signature* references (deltas naming `SpecsProvider`, state holding the scope stack)
are accepted as unavoidable; what matters is that item placement itself is unambiguous.

**Conditions stay producer-side** (construct conditions in `constructs`, token
conditions in the hub, scope conditions in `specs`, preset conditions in the preset). A
central registry module was rejected: the family is *open* (custom parsers define new
condition types in their own crates — a registry could never be exhaustive), it couples
a central module to every parser's internals, and it splits error logic away from the
parser it belongs to. The registry *need* (identifier ↔ type reference) is served for
free by rustdoc's `DiagnosticInfo` implementors listing plus a guide table.

Rejected alternatives: **root re-exports / curated common tier** (dual paths violate
one-canonical-path; empirically near-unused as access paths anyway); **structured
facade mirroring the nine topics** (freezes the topic taxonomy — the axis already
revised once and with live revision candidates); **pipeline split**
(define → parse → result re-litigates the L0–L7 ladder: stage is a property of use
moments, not items — `ParsingState`, `ScopeStack`, the builder each live in several
stages); **`techy::util`** (the canonical vague name; S0's models are not utilities);
**`definitions`/`defs` as the specs-group name** (collides with the planned
`latexlike::defs` standard-definitions database — the database's claim to the word is
stronger); **`parsing` as facade name** (forks path vocabulary from the wire
identifiers' `core.*` areas); **a conditions registry module** (above).

*Reversal note (2026-08-19, user).* The token subset no longer stays in the hub. This
entry counted "token data vs runtime" among the decided cycle edges and kept the token
items uncut in `techy::core`; the revisit clause below ("the hub grows uncomfortably
large") is what fired, and the subset is extracted into `techy::core::token` under an
explicit placement rule that cuts that straddle — [§dd-dr:core-token-facade], which also
records the item-by-item resolution and the accepted path breaks. The layout list above
is the post-extraction one; everything else in this entry stands, including the
one-canonical-path rule the extraction obeys (each moved item is reachable at exactly
one new path).

Revisit if: a future public item genuinely belongs to two groups; the hub grows
uncomfortably large (extracting a further subset is breaking — weigh before the first
external dependent); or the crate is split (S0/topic modules convert to crate
re-exports losslessly — the facade model is what makes that lossless).

---

#### `core::token`: the token subset extracted from the hub; the placement rule [§dd-dr:core-token-facade]

Status: DECIDED (user, design session).

The token items leave the flat `techy::core` hub for a fourth satellite,
`techy::core::token`, bounded by this placement rule: *`core::token` holds what a token
reader produces, consumes and answers with — the token and stream-position types, the
`TokenReader` trait and the standard reader, the scan helpers, the token rules the reader
reads together with the overrides that change them mid-parse and the caches derived from
them, the types the specials-scan hooks answer with, and the token conditions and errors.
The hub keeps the `Lang` trait (its associated types and hooks), the parsing state and
its deltas, and the engine.* Forty-one items move; every one keeps its name.

Why now, when [§dd-dr:public-namespace-topology] deliberately left the token subset in
the hub: the token topic has since grown — the tokenization bundle
([§dd-dr:tokenization]), the per-feature rules blocks with their overrides, and now nine
public scanning items ([§dd-dr:scan-helpers]) — to a third of the hub's items and
counting, which is that decision's own revisit condition. The extracted shape is also
not a new one: the sibling satellite `core::constructs` holds a trait, its shipped
implementations, its helpers and its conditions together, and the token topic has the
same four parts.

The rule cuts the "token data vs runtime" straddle that had kept the subset in the hub,
by asking who *reads* an item rather than who carries it. Four families were the
ambiguous cases, and all four resolve into `core::token`: the rules overrides a
`ParsingStateDelta` carries; the `PrefixTable`/`TriggerChars` caches a `ParsingState`
derives; the tokenization declaration a `Lang` names; and the
`SpecialsMatch`/`SpecialsScanError` a `Lang` hook answers with. In each case the carrying
item stays in the hub and its signature names a `core::token` type across the boundary —
the accepted kind of cross-facade reference (`Lang::make_node_ext` already names
`core::node` types).

Accepted cost: every `techy::core::<token item>` path breaks. That is deliberate inside
the soft-freeze window ([§dd-dr:stability-rubric]); dependent projects adapt on their own
schedule, and the extraction by itself implies no version bump and no baseline move.

Rejected alternatives: a helper-only namespace (`core::tokenscan`) beside token items
left in the hub — the trait in the hub with its implementation library one level down is
the asymmetry `constructs` avoids, and it doubles the places a reader author has to look;
leaving the straddle uncut and adding the nine new scanning items to the hub (ninety flat
items, and "token data vs runtime" is not a line anyone reading the API could draw).

Revisit if: a token item genuinely belongs to two facades — an item that no reader reads,
produces or answers with would need a rule this one does not supply.

#### API stability rubric: one stability class, soft freeze until framework adoption [§dd-dr:stability-rubric]

Status: DECIDED (user, API-review policy session).

Everything `pub` — outside the `#[doc(hidden)] __private` derive plumbing — is **one
stability class under one semver discipline**: no experimental/unstable tier, no
unstable feature gates. Access tiers (consumer/extender/language-designer/tooling/
framework) are expressed by module placement and guide structure
([§dd-dr:public-namespace-topology]), never by stability level. Consequence for the
API-review per-item rulings: an item survives as `pub` only if it is worth stabilizing;
anything else becomes `pub(crate)`.

The freeze is a **discipline with a soft onset, not a hard date**. It takes effect when
the API-review restructuring lands (guarded by a cargo-semver-checks baseline), and
from then on breaking changes are deliberate, baseline-visible, version-bumped events
(at 0.x: breaking → 0.(x+1).0, additive → 0.x.(y+1); 1.0 when the first framework
builds on techy in earnest). But the user explicitly rejected treating the post-review
API as untouchable: **until significant frameworks are actually being built on techy,
an important discovered shortcoming may still be fixed breakingly** — correcting a flaw
before dependents exist is cheaper than carrying it forever. The review's success
criterion is that *framework development* never forces a restructuring; the hard freeze
begins with that development, not with the review's end. Guides print paths and wire
identifiers only post-restructuring, so published material never teaches a
pre-freeze name.

Guards in place: `missing_docs` is a workspace `deny` lint (promoted at zero
warnings), and the cargo-semver-checks baseline is realized the unpublished-crate
way — a git revision, not a registry version: `scripts/check_semver.sh` runs
`cargo semver-checks check-release -p techy --baseline-rev api-baseline`, where
`api-baseline` is a git *branch* moved deliberately at each version bump (user
ruling: a movable branch, not a tag, so the baseline follows deliberate API
adjustments during the soft freeze); the script clears `RUSTDOCFLAGS` because the
workspace's rustdoc-header injection uses a root-relative path that scratch
builds cannot resolve. The per-item rulings this rubric's consequence clause
called for are complete — [§dd-dr:public-visibility-sweep].

Rejected alternatives: an unstable/experimental tier (an escape hatch that invites
exactly the future restructuring the review exists to prevent, and a dual-status
ambiguity against the one-canonical-path principle); a hard freeze at the end of the
review (prematurely enshrines flaws found between review and adoption, for no
dependent's benefit); stability tiers A/B distinguished by surface (they carried the
same semver discipline anyway — the distinction collapsed once tiering moved into
module placement).

Revisit if: a framework starts depending on techy in earnest — from that moment the
freeze is hard and breaking changes need migration paths and dependent coordination.

#### The public-visibility sweep: pub-vs-pub(crate) rulings for the walkthrough-untouched items [§dd-dr:public-visibility-sweep]

Status: DECIDED (user, API review; completes the per-item ruling
clause of [§dd-dr:stability-rubric]).

The 76 root re-exports untouched by all five persona walkthroughs were ruled
item-by-item: **73 keep `pub`-and-stable; `NodeData` and `check_tree_invariants`
→ `pub(crate)`; `NoResolver` removed**. The batch's empirical finding, recorded
because it supports the keep rulings: "no usage signal" overwhelmingly meant
*signature closure of the used API* — most items are forced pub (named in
signatures of items the walkthroughs did use: returns, public fields,
trait-method parameters) or doctrine-bound (shipped condition types under the
frozen wire-identifier slate + typed matching + the implementors-page doc story;
the condition-defining surface the intended downstream `flm.*` vocabularies
require).
Notable per-item rationales:

- **`NodeData` → `pub(crate)`**: the only node-module item in zero public
  signatures — `NodeRef` is the read API, the builder the write API; nothing
  reachable disappears.
- **`check_tree_invariants` → `pub(crate)`**, re-implemented over
  `validate_tree` ([§dd-dr:tree-validation]).
- **`VERSION` stays** the crate root's compile-time `&str` const — the ecosystem
  idiom when a crate exposes its own version; a `version()` getter is the
  wrapped-C-library idiom, and structured `(major, minor, patch)` consts have no
  convention behind them (consumers parse with the `semver` crate — the string is
  guaranteed valid semver by Cargo). Concrete consumer: bindings version
  reporting.
- **`FrameRole` homes in the hub** beside `Frame`/`FrameTitle`, not `core::specs`
  (user): the frame vocabulary is engine-wide — groups, environments, and
  invocations all mint frames — so it is traceback vocabulary a spec hook
  references, not spec vocabulary.
- **Parsed-residue placement rule** (user): parser-*contract* residue follows its
  trait (`ParsedArgumentNodes` → `core::constructs` with `ArgumentParser`);
  *stored* built-node containers stay `core::node` (`ParsedArguments`,
  `ParsedArgument`, `ParsedSlots`, `ParsedSlot`, `ChildRegion`, `ContentNodes`).
- **`skip_whitespace` stays pub**: the paragraph rule (never silently consume a
  paragraph break) is subtle shared semantics deserving one public source of
  truth over N hand-transcriptions in custom tokenizers.
- **Shipped implementations of public contracts stay pub** (`Scope`,
  `FallbackProvider`, `ErrorCallableSpec`, `StdTokenReader`, `NodesParser`,
  `GroupParser`): a public seam whose only shipped implementation is invisible
  inverts the seam's purpose (drivers could replace but never reuse/wrap the
  standard behavior); `Scope` in particular is the load-bearing carrier of
  runtime (`\newcommand`-class) definitions despite zero walkthrough use.
- Free `resolve_source` renamed **`resolve_source_reference`**
  ([§dd-dr:input-wiring]); `Diagnostics::into_vec` — never existed;
  recorded as reject-do-not-add (`iter().cloned().collect()` +
  `sorted_by_position()` cover it).

Rejected alternatives: demoting never-downcast condition types (the diagnostic
would still raise its frozen wire identifier but become unmatchable by type —
silently breaking the typed-matching contract in favor of exactly the
string-matching the documentation plan exists to prevent); demoting the two
derive re-exports (every downstream condition author re-writes the boilerplate
techy wrote a derive to avoid — and techy itself uses it 32×); a spartan-root
`VERSION` demotion (reversible, but the cost of keeping is ~zero and the
bindings consumer is concrete).

Revisit if: a demoted item acquires a real external consumer (re-publishing is
additive), or a framework's usage evidence contradicts a keep's stated consumer
story at hard-freeze time.

#### Embedding-feedback policy: graduation over convenience surface; the declined batch [§dd-dr:embedding-feedback-policy]

Status: DECIDED (user; batch ruling over the first external embedding's filed
API requests).

Three policy anchors, each closing a class of requests:

- **Parsing states stay crate-frozen.** `ParsingState::data()`/`from_data()`
  are declined: states are shared by handle — a data-equal copy is a
  *different* state — and a public data→state freeze would force deciding what
  identity and provenance an externally assembled state has
  ([§dd-dr:seed-states] holds unweakened). The sanctioned paths: `derived()`
  from one state to the next, and `Language::new(driver, impl
  Into<Arc<ParsingState<L>>>)`, which seeds a parse pipeline from an
  already-shared handle with its identity preserved ([§dd-dr:language-init]
  amendment) — closing the real symptom behind the request.
- **`ConcatPieces` stays build-only.** The read direction (`into_parts`) is
  declined: the type is a build-only instruction, and publishing the read side
  freezes an internal six-part shape as API. A wrapping recomposer that must
  inspect a delegate's instruction maintains its own structure.
- **No `test-support` cargo feature.** Embedders write their own test
  fixtures; anything genuinely indispensable graduates to real public API
  instead of shipping behind a feature gate (`validate_tree` is the model,
  [§dd-dr:tree-validation]; graduated under the policy: `TreeViolation::new`,
  so consumers can manufacture values that exercise their violation-handling
  code, and `MacroSpec::with_after_effect`, which replaced in-crate test-only
  scaffolding with the public declarative route). The in-crate invariant
  checkers stay internal — the byte-accounting span-tiling law is deliberately *not*
  the all-trees law.

Rejected alternatives (the declined accessor batch — none urgent, all cut for
leanness; each is additive later if a framework consumer materializes):
`NodeTree::id_at(index)`; `Diagnostic::with_frames`; `NamedAccessError`
accessors; `ArgumentCodeError` accessors; `KeyValEntry::value_node`;
`slot_content_parent_named`; a public `DEFAULT_MAX_SCAN_LEN`/`max_scan_len()`;
a public `copy_subtree_into`; `NodeRef::invocation_syntax_materialized`; a
parse-start warning for a half-wired specials trigger/scan pair; `Default` or
public constructors for `RestageContext`/`RecomposeContext` (forecloses the
one-place-to-grow reserve); de-lifetiming `ParseContext`; `PartialEq` on
`Diagnostic`; `&Arc<ParsingState>` on the four state-reading hooks;
`Package::get` trigger fallback; `recompose_from`; `TriggerChars::None`; an
owning `LineIndex` (that is `LineIndexCache`); an `ExpectedClose` enum;
`KeyVals::into_parts`; `Clone` on `RestagedArgument`; `StagedChildren::ids()`;
blanket visibility flips of tree internals (`NodeTree::make_id`, `NodeId::new`,
`ParserSession::state_stack`, …).

Revisit if: a declined item acquires a concrete framework consumer (graduation
is additive), or a second, independent embedding files the same gap.

## Documentation [§dd-dr:documentation]

#### Narrative docs included with rustdoc, not a separate site [§dd-dr:rustdoc-guides]

Status: DECIDED (user-led).

API documentation is rustdoc; narrative pages (usage, concepts, design patterns — the role
of pylatexenc's Sphinx pages) are markdown files in `docs/`, rendered as doc-only modules
under `techy::guide` via `#[cfg(doc)]` + `#[doc = include_str!(...)]` in `lib.rs` (the clap
pattern).
Rationale: one site and one search index, and — decisively — compiler-checked intra-doc
links plus doctest-compiled examples: during the ongoing review-and-rename churn, links and
code samples in a separate book would silently rot, whereas rustdoc breaks the docs build
instead. Zero extra toolchain; docs.rs hosts it on publish.
Rejected alternatives: mdBook alongside rustdoc — proper book navigation, but a second toolchain and
unchecked book→API links. Not precluded: the `docs/*.md` sources move into mdBook chapters
nearly verbatim if the narrative later outgrows rustdoc.
Revisit if: the guide needs ordered, book-style chapter navigation that rustdoc's
module-shaped layout can't carry.

#### Two-pillar developer docs with immutable label cross-references [§dd-dr:docs-restructure]

Status: DECIDED (user-led).

The project documentation is organized per `Documentation_Structure.md` (repository
root): user-facing rustdoc (guides + API documentation, self-contained, never citing
developer documents except individually-approved cases) and developer-facing
`dev-docs/ARCHITECTURE.md` (*how*; present-day, no history, no dates) +
`dev-docs/DESIGN_RATIONALE.md` (*why*; this register). Cross-references use immutable
bracketed heading labels (`[§dd-arch:…]`, `[§dd-dr:…]`) instead of section
numbers; every register entry, whatever its status, is referenced at least once from
ARCHITECTURE; label integrity is manual grep discipline (the maintenance rules,
[§dd-dr:self-meta]). Status lines carry who/context, never dates; dates survive only
inside explicit reversal records. NAMING_STRATEGY.md is archived: its principles moved
to ARCHITECTURE [§dd-arch:naming], its reversal and do-not-reintroduce material into
[§dd-dr:naming].
Rationale: numeric section references (about 190 in Rust sources alone) broke on every
reorganization, and history mixed into present-day description made ARCHITECTURE
misleading about what currently exists.
Rejected alternatives: keeping numeric sections (renumbering invalidates every citer); a
separate documentation site for the guides (already rejected — previous entry);
automated label-integrity tooling for now (a checker script or test can come later; the
manual procedure is cheap at the current change rate).
Revisit if: label collisions or topic splits become frequent enough that the flat label
namespace or the manual discipline hurts.

## The latexlike preset [§dd-dr:latexlike]

#### The preset's group taxonomy is two classes: `Content` and `Math` [§dd-dr:group-taxonomy]

Status: DECIDED (user).

`GroupType` has a *single* math class covering `$…$`, `$$…$$`, `\(…\)`, `\[…\]`; inline
vs. display is neither a class nor a mode. As first ruled, display-ness was a delimiter
fact, read off the
node's recorded delimiters by a table-backed preset sugar (pylatexenc parity:
`LatexMathNode.displaytype` is likewise
delimiter-derived) — since superseded, see the closing note.
Rationale: the class taxonomy cuts at parse-behavior joints, and inline and display math
parse identically — same interior `Mode::Math`, same definition visibility — so a split
would do no parse-time work; it would also break the class/mode symmetry (three classes
over two modes).
Rejected alternatives: a sketched `MathInline`/`MathDisplay` split (typed display-ness that a
rule author declares — its one real advantage: embedder-registered math delimiters would
classify themselves, where the delimiter table answers `None`); a `Bracket` class and
`[]` in the default rules — `[`/`]` are plain characters in LaTeX outside
optional-argument positions (`a [b] c` is text), and `OptionalGroupArgumentParser`
recognizes them through its own per-spec `temporary_groups` rule, so neither the class nor
the base rule has a consumer (user-caught; an earlier sketch listed both).
The revisit condition (typed display-ness on custom math delimiters) fired during
the API review: display-ness is now
typed **class payload**, `Math(MathGroupForm)`, declared by the rule author, with
parse wiring still single-armed; the `MathStyle` delimiter-table sugar is superseded.
See [§dd-dr:math-group-form].

#### Inside math the math delimiters stop opening; a stray `$` is forbidden [§dd-dr:math-no-nesting]

Status: DECIDED (user, review).

`LatexlikeDriver::group_interior_delta` for a math rule returns, besides `mode(Mode::Math)`,
a `TokenRulesOverrides` derived from the **outer** state: the interior's group rules are the
outer rules minus the `Math` openers, and `$` is merged into the outer `forbidden_chars`.
The descent invariant still installs `expecting_group_close`, so the current group's close
works; a `$` that is *not* that close is a forbidden-char diagnostic, not the opener of a
nested inline group. Example: tolerant `$$a$b$$` is one display group over `a$b` (one
diagnostic), never a display group left unclosed around a spurious inner `$…$`.
Rationale: LaTeX forbids nested math; without this a lone `$` inside display math opened a
fresh inline group and consumed the trailing `$$` as two separate closes, leaving the
display group unclosed (the surprising tree the review flagged). Deriving from the outer
state (not the seed) preserves any embedder rule changes in force at the `$`, and *merging*
(not replacing) `forbidden_chars` keeps the embedder's forbidden set. `\(`/`\[` inside math,
their openers likewise gone, fall through to the command path (a stray single-char command)
— acceptable.
Rejected alternatives: leaving the math openers active in math (the pre-review behavior, with its
unclosed-group trees); a bespoke "no nested math" condition for `\(`/`\[` (the generic
unresolvable-command / forbidden-char diagnostics already localize the error).

#### Preset vocabulary names are bare and module-scoped [§dd-dr:preset-vocabulary]

Status: DECIDED (user).

`GroupType`/`CallableType`/`Mode` with short variants (`Content`/`Math`;
`Macro`/`Environment`/`Specials`; `Text`/`Math`), reading as `latexlike::Mode::Math`;
preset items are **not** re-exported at the crate root. All three enums are
`#[non_exhaustive]` (`GroupType::Verbatim` arrived exactly so); in-crate matches stay
exhaustive on purpose, so a new variant surfaces every site.
Rationale: the context-determines-names principle ([§dd-arch:naming]) — no sibling
vocabulary competes, since the core
has only the *associated types* (`Lang::GroupTypeId` …), never concrete types with these
names; the module path disambiguates everywhere else.
Rejected alternatives: `Latex`-/`Latexlike`-prefixed enum names (length that does no disambiguation
work inside a namespaced preset); the `MACRO`/`ENVIRONMENT`/`SPECIALS` spelling (an
artifact of the u32-const test era, not Rust variant style).

#### The seed ships a `"_builtin"` package; typography specials are definitions content [§dd-dr:base-package]

Status: DECIDED (user; the seed's scope was consciously narrowed during the API
review — reversal recorded below).

`Latexlike::initial_state_data()` seeds the scope stack with one package
**`"_builtin"`** (`builtin_package::<LLL>()`, `LLL`-generic) holding what any
latexlike parse must preload — the `\begin`/`\end` dispatch, visible in all modes
so math environments still open in math. Droppable wholesale by name
(`ScopeOp::Unload`), shadowable per-trigger by pushing a provider.
Macro/environment definitions deliberately stay out until the std-DB port.

The typography specials — `~` and the ligatures ``` `` ```, `''`, `--`, `---`,
`` !` ``, `` ?` `` (pylatexenc's *latex-base* + *nonascii-specials* categories) —
live in `minidefs`'s `"minilatex"` package, the ligatures registered **text-mode
only** (they carry no math meaning — inside `$…$` they stay plain chars; the
per-entry mode gate, [§dd-dr:mode-visibility]); `&` is not defined by the preset
at all. A builtin-only parse emits these triggers as plain chars — the deliberate
positioning: typography interpretation is definitions content, not parsing
substrate; pylatexenc default-shape parity for these triggers requires loading
minilatex (pinned by seed-default-shape tests). Reversal record: the seed first
shipped as a `"base"` package carrying the typography specials itself, for
out-of-the-box pylatexenc parity; that shape was consciously superseded by the
positioning above, and the fn followed the rename (`base_package()` →
`builtin_package()`). One deliberate parity exception either way: the `\n\n`
paragraph-break special of pylatexenc's *latex-paragraph* category is omitted —
a multi-newline break is a whitespace chars node here
(`enable_multi_newline_paragraphs`), not a specials node. The multi-character
ligatures exercise the longest-match fold (`---` beats `--`) in real defaults
rather than only in tests.
Rejected alternatives: an empty seed stack (`\begin`/`\end` must be preloaded for
any latexlike parse); seeding only `&`/`~` (leaves the fold's only real-data
consumer test-side).

#### Per-definition mode visibility on `Package` — the fine gate under `set_visible_modes` [§dd-dr:mode-visibility]

Status: DECIDED (user).

`Package::insert_in_modes`/`insert_specials_in_modes` attach an optional mode list to a
*single* definition; `retrieve_spec`/`scan_specials` check it against `ParsingState::mode`
under the pre-existing package-level `set_visible_modes` — **both** gates must admit the
mode (`None` = every mode the package is visible in). One loadable, unloadable package can
then hold text-only ligatures and (later) math-only `^`/`_` scripts together.
Rationale: a package must be able to keep `\begin`/`\end`-class entries visible in
math while hiding text ligatures there — package-level visibility alone cannot express
that without splitting one package into several names, which would break the
single-name `Unload` contract and the specials-as-one-category model. Per-entry visibility is the minimal mechanism that
keeps one package. The trigger-char union deliberately stays mode-blind (a hidden entry's
first chars remain in the filter; its scan declines) — the established caching contract.
Rejected alternatives: multiple mode-scoped seed packages (changes the unload semantics, multiplies
seed names); a whole-package flip to text-only (would hide `\begin`/`\end` in math too).

#### Default whitespace is the ASCII set, not Unicode-aware [§dd-dr:ascii-whitespace]

Status: DECIDED (user).

`default_token_rules()` sets `WhitespaceRules.chars` to the six ASCII whitespace
characters (space, tab, `\n`, `\r`, vertical tab, form feed); a Unicode space (NBSP
U+00A0, U+2028, …) is ordinary content, diverging from pylatexenc's Unicode-aware
`str.isspace()` (which swallows e.g. an NBSP after `\emph` as post-macro space, yielding a
different node shape).
Rationale: the `WhitespaceRules` model is a fixed char-set membership test, and an ASCII
set is deterministic and needs no Unicode tables; the divergence is narrow (only exotic
Unicode spaces in a source) and now recorded rather than silent.
Rejected alternatives: matching pylatexenc by widening `WhitespaceRules` to a `char::is_whitespace`
predicate — deferred as an unforced core-model change; revisit if real inputs demand it.

#### `NodeRef` preset sugar is inherent, not an extension trait [§dd-dr:inherent-preset-sugar]

Status: DECIDED (user).

The accessors (`is_math_group`, `math_form`, `macro_name`, `environment_name`,
`specials_name`) are inherent methods, written in the preset
module — legal because the preset shares the crate with `node` — and generic over
the family (`impl<LLL: LatexlikeLang> NodeRef<'_, LLL, A>`), reading vocabulary
through the role traits, so an in-crate family member gets the sugar with no
extension trait either.
Rationale: zero-import ergonomics on the majority path; an out-of-crate language (FLM)
must use an extension trait regardless, and that pattern needs no in-tree demonstration.
Rejected alternatives: a `LatexNodeRefExt` trait for the preset (a `use` tax on every consumer, buying
only symmetry with a constraint the preset does not have).

#### `\begin`/`\end` dispatch is scope-stack data: ordinary `Macro` entries of `"_builtin"` [§dd-dr:begin-end-dispatch]

Status: DECIDED (user).

`BeginSpec` (the environment composition) and `EndSpec` (orphan-`\end` diagnostics) are
registered under `begin`/`end` in the seed package like any definition — resolvable
through the unchanged `LatexlikeDriver::resolve_command`, shadowable, and unloadable
(`Unload("_builtin")` removes the environment dispatch; pinned in a test).
Consequence: the `Invocation` arrives typed `Macro`, so the composition stamps
`CallableType::Environment` (and the environment's own name and spec) on the staged
node itself — the dispatcher's identity appears nowhere in the tree.
Rationale: the direction is "everything through the stack" (even specials are
data); a hardcoded `resolve_command` arm would be the one un-shadowable definition in
the language.
Rejected alternatives: the test-lang rehearsal's driver arm (`if name == "begin"`), which made
`\begin` structural syntax.

#### The environment pair's command names are definitions data [§dd-dr:environment-command-names]

Status: DECIDED (user).

`BeginSpec::new(end_command_name)` carries the terminator command's name (`"end"`);
the opening command's name is the entry's own registration name. No machinery spells
either: the composition hands the terminator name to the body parsers through
`EnvironmentInvocation::end_command_name`, so both the tokenized
`EnvironmentBodyParser`'s stop condition and the raw `VerbatimBodyParser`'s composed
terminator follow the definition — the escape character and the name group delimiters
already came from the invocation as written ([§dd-dr:verbatim-family]).
`builtin_package` picks `begin`/`end` by writing those two names; a language spelling
the pair `\open`/`\shut` is a package, not a fork.
Rationale: the argument that made the dispatch pair scope-stack data
([§dd-dr:begin-end-dispatch]) — a constant in preset code is the one part of the
language no definition can shadow.

The pairing is unenforced: the body parsers match the terminator name against command
tokens, while `EndSpec` is reached by resolution under its registration name. A
mismatch degrades to diagnosed outcomes (bodies run to the end of their input; stray
terminators resolve as unknown commands), so the invariant is documented on
`BeginSpec::new` rather than checked — nothing can check it before a parse, and at
parse time it would fire on documents that never terminate anything.

Consequently the preset's conditions quote **what the source said**, never a canonical
spelling: `MalformedBegin` carries the opening command as written (escape character
included, the trigger's post-space excluded) and `OrphanEnd` the terminator's whole
consumed extent. `OrphanEnd` no longer names the opening command at all ("no
environment ‘align’ is open here", not "no matching `\begin{align}`") — the orphan
site knows its own name only, the pairing running in the opening direction alone.
Core's environment conditions need nothing: they name environments (`{expected}`,
`{environment}`) and spell no command. `MissingEnvironmentTerminator` keeps that
silence deliberately — `EnvironmentBodyParser` knows the stop *word* but never the
escape character or name group delimiters (those exist only in a terminator actually
read), so it cannot quote the terminator it wanted.

Rejected alternatives: an associated const on `LatexlikeLang` (compile-time, hence
unshadowable and unable to serve two pairs in one language — [§dd-dr:data-vs-traits]);
the name on `EnvironmentBehavior` (per-environment variation of the *terminator* buys
nothing, and would let one environment's body stop on a word no other recognizes); a
symmetric `EndSpec::new(begin_command_name)` so `OrphanEnd` can keep naming `\begin`
(a second unenforced pairing bought purely for message wording); conditions
reconstructing their spelling from pieces (escape character + word + delimiters — four
fields that can still drift from the bytes, where the anchored source slice is exact by
construction).

Revisit if: a language needs the terminator's name to vary per environment, or
begin/end mismatches prove hard enough to diagnose that a registration-time check earns
its place.

#### `EnvironmentSpec` wraps a dyn `EnvironmentBehavior`; `with_body_delta` adapts [§dd-dr:environment-spec-surface]

Status: DECIDED (user; executes the [§dd-dr:spec-downcasting] funnel and
[§dd-dr:begin-composition]'s defaulted `make_body_parser()`).

The concrete wrapper `EnvironmentSpec` is the registration/downcast target (implements
`CallableSpec` by delegation, titles frames "environment ‘align’"); the inner trait
carries the behavior as defaulted methods — `arguments()`, `body_state_delta(…)`
(owned return: behaviors may compute it), `make_body_parser(…)` (default: the core
`EnvironmentBodyParser` through the rigid `\end{name}` terminator). Hooks receive an
`EnvironmentInvocation` facts struct (`trigger_span`, `name`, `name_span`, plus the
spellings a takeover body composes its terminator from: `escape_char`,
`name_group_open`/`name_group_close`, `end_command_name`) —
`#[non_exhaustive]`, grown by field as consumers demand; the parsed arguments were
deliberately left out until a behavior needs them (pylatexenc's `nodeargd` precedent
noted; adding a field is non-breaking). `EnvironmentSpec::new(arguments)` builds a
private declarative behavior; `.with_body_delta(delta)` wraps the *current* behavior in
a delta-overriding adapter — total for custom behaviors too, no fallible builder, no
second delta field. A non-`EnvironmentSpec` registration under
`CallableType::Environment` is legitimate: its declared arguments parse and the body
takes the default handling (the funnel downcast simply misses).
Rejected alternatives: a delta field on the wrapper next to the behavior (two sources of truth); a
`Result`-returning builder gated on the behavior being the standard one (ergonomics
tax on the 99% case).

#### `MacroSpec`/`SpecialsSpec` are real types, not constructor functions [§dd-dr:concrete-spec-types]

Status: DECIDED (user).

Both are `StdCallableSpec`-shaped declarative types whose `stack_frame_title` speaks
the preset vocabulary ("macro ‘\frac’", "argument #1 of macro ‘\frac’",
"specials ‘~’"); `builtin_package()`'s specials use a shared `SpecialsSpec`.
Generic specs remain first-class everywhere.
Rationale: functions returning `StdCallableSpec` would leave tracebacks saying
"callable ‘…’" — the vocabulary hook exists precisely for presets — and concrete preset
types are stable downcast targets for later `finalize_node` work.

#### Orphan-`\end` recovery: dispatch-time diagnosis, chars over the consumed extent [§dd-dr:orphan-end-recovery]

Status: DECIDED (user).

Inside a body, `\end` is the stop condition and never reaches resolution, so a
*dispatched* `\end` is always an orphan: `EndSpec`'s parser reads the rigid name group
when present, records `OrphanEnd` (quoting the terminator as written and naming the
environment when the name parsed, [§dd-dr:environment-command-names]),
and tolerantly stages the consumed extent as one `Chars` node — `\end{name}` whole, so
`{name}` is not re-parsed as a stray group. Preset condition ids are namespaced
**`latexlike.environments.*`** (`malformed-begin`, `unknown-environment`, `orphan-end`;
user-chosen over `latexlike.begin.*`/`latexlike.end.*`). Implementation fact worth
remembering: the tolerant chars fallbacks (malformed `\begin`, nameless orphan `\end`)
must cover the trigger's syntactic *post-space* too — the token span includes it, and
trimming it would break the sibling span tiling; the earlier rehearsal had the same
shape. The body-unwind path that leaves a stray `}` for the root recovers cleanly: the
root stages the consumed delimiter as a `Chars` node (cf. [§dd-dr:language-parse-api],
second follow-up).

#### The verbatim family: recipe → production parsers, group+chars shapes [§dd-dr:verbatim-family]

Status: DECIDED (user, parser-library survey).

`constructs::verbatim_parser` promotes the pinned recipe ([§dd-dr:token-contract-hardening], item 5; the
test-side `RawBlockParser`): `verbatim_state_delta(rule)` is the recipe as data (every
feature off via `disable_all()` — the six gates and the cleared forbidden set
([§dd-dr:takeover-staging-sugar]) — and
`expecting_group_close` **replaced**), and the two production
parsers drive it — `VerbatimArgumentParser` (delimited `\verb|…|`; `ArgumentParser`,
the `v` codes) and `VerbatimBodyParser` (raw environment contents up to a terminator;
produces `EnvironmentBody`, pluggable via `make_body_parser`).
Points settled in flight:

- *Delimiter discovery reads one raw char under a second, narrower delta*: whitespace
  scanning stays on (pre-delimiter blanks are ordinary region noise; pylatexenc
  `skip_space_chars` parity — a paragraph break may precede the delimiter), everything
  else off, and the inherited close expectation **cleared** — `\verb}x}` works inside a
  braces group. Comments are deliberately *not* noise here: `%` is a valid delimiter
  (the [§dd-dr:nodes] "noise policy is inseparable from argument syntax" case in the flesh).
- *Node shapes* (modern-pylatexenc parity): `\verb` stages a `Group` (class = a
  language verbatim class; delimiters as written; close **empty** on EOF recovery)
  holding one raw `Chars` child — omitted when empty, techy-wide convention — with
  content = the group's children. The raw chars nodes record the **verbatim state**
  they were read under (pylatexenc marks its verbatim chars nodes the same way); the
  group/list wrappers record the surrounding state.
- *Depth counter* (pylatexenc parity): with paired delimiters, nested opens (plain
  `Char`s under the recipe state) deepen and closes (`GroupClose`s) surface —
  `\verb{a{b}c}` is one region; identical delimiters end at the first closer.
- *Terminator matching is literal*: `\end {verbatim}` does not terminate (string-search
  parity), and verbatim does not nest. Whatever shape a caller states the terminator
  in, the parser reads up to **one raw string** — the terminator is an expected group
  close, so a raw body never tokenizes it.
- *The terminator is stated in pieces, not pre-composed* (`VerbatimBodyTerminator`,
  ruled 2026-08-11): a caller supplies either a bare `Literal` string, or a
  `StopEnvironmentCommand` — escape character, stop command name, name group rule,
  and the invocation name the terminator back-references. The parser composes the
  raw string from the pieces *and* reports them back on `EnvironmentBody::terminator`
  as `EnvironmentTerminatorSyntaxData::Scanned` facts (spans laid over the matched
  terminator in composition order, empty post-space), the same arm the tokenized
  `EnvironmentBodyParser` reports — so a recording consumer needs no raw-body arm and
  keeps span-backed end facts ([§dd-dr:invocation-syntax]). A `Literal` terminator has
  no such structure and reports only its span; a record that cannot store a bare
  literal (latexlike's `StdEnvironmentSyntax`) is then inaccurate by construction,
  which is why the preset's `VerbatimBehavior` states the pieces instead. Every piece
  comes off the invocation — the escape character and the name group delimiters *as
  written*, the stop command name from the dispatching spec
  ([§dd-dr:environment-command-names]) — so a language re-ruling the escape character
  or renaming its terminator needs no behavior of its own.
- *A tolerated unreadable token* inside a committed verbatim region ends it like
  EOF (diagnosed unterminated/missing-terminator); the enclosing loop re-reads the
  error and applies its own token recovery — the probe protocol, two true
  diagnostics accepted. `disable_all()` clears the
  forbidden set ([§dd-dr:takeover-staging-sugar]), so the standard
  reader has nothing left to reject under the recipe state — a language-outlawed
  character reads as raw content — and this ending exists only for custom
  readers. A reader yielding any *other* token kind under the recipe
  state is an implementation-error abort (panic policy: contract violations `Err`).

`GroupType::Verbatim` joins the preset vocabulary (the reserved variants slot); **no `Mode::Verbatim`** — verbatimness is rules-borne (a derived-state fact
scoped to the region), not a mode the scope stack or content interpretation keys on.
Rejected alternatives: a char-level reader API (pylatexenc `next_chars`; the recipe already
delivers per-byte `Char` tokens); a shared "base parser" type with pluggable stop
conditions (two users, one private loop helper + the public delta builder suffice).

#### `EnvironmentBody.content`: the body parser designates the slot's content [§dd-dr:environment-body-content]

Status: DECIDED (user).

`EnvironmentBody` gains a `content: ContentNodes` field (and drops `Copy`); the
`\begin` composition (and the test composition) mints the `"body"` slot record from
it instead of designating all-children itself. Forced by the newline gobble: pylatexenc
*drops* the newline right after `\begin{verbatim}` from its chars node, but techy trees
keep every byte — so the gobbled newline is **staged as a leading whitespace `Chars`
node inside the body `List` and designated out of the content**. Putting it anywhere
else breaks an invariant: excluded from the `List`'s span it either gaps the callable's
children block (arguments before the body, the `lstlisting[opts]` shape — invariant 3)
or un-tiles the `List` interior (invariant 2). The standard `EnvironmentBodyParser`
designates all children (behavior unchanged); "which body nodes are content" is the
staging parser's knowledge, exactly as for arguments ([§dd-dr:nodes] parse-time designation).
Rejected alternatives: letting the newline ride the scaffolding gap (works only for argument-less
environments; would weaken invariant 3 to legalize the rest); an `Option`al designation
field (the default parser knows its answer — make every producer say it).

#### The argument-code factory: `latexlike::argument_specs` [§dd-dr:argument-specs-factory]

Status: DECIDED (user, parser-library survey).

A preset **function**, `&str` in → `Result<Vec<Arc<ArgumentSpec>>, ArgumentCodeError>`
out, eager. The single code string concatenates codes
(pylatexenc's list form is not mirrored), so the grammar is pinned: optional whitespace
*between* codes; parameters follow their code immediately and may not be whitespace;
**`v` takes two delimiter characters exactly when a non-whitespace character follows
directly** — a bare auto-`v` stands last or before whitespace (`"v {"`), and `"v{"` is
a loud `TruncatedCode`, never a silent misparse. Per-code resolution as landed:

- `m`/`{` → `GroupArgumentParser::new(Content)` — *refining the survey's
  `ExpressionParser` row*: the class parser is the decided parse-time realization of
  pylatexenc's `'{'`+`unwrap_double_group` semantics (content = group children), is
  what every preset test/doctest already used, and carries `ExpressionParser` inside as
  its fallback engine.
- `o`/`[`, `d<c1><c2>` → `OptionalGroupArgumentParser` with a minted `Content` rule
  **and lone-`{…}`-group unwrapping on** (the accessor-default parity choice).
- `r<c1><c2>` → the new **rule form** `GroupArgumentParser::with_rule` (the survey's
  "per-use constructors remain to be written"): same temporary-rule state scoping as
  the optional parser — nested pairs balance, braces protect — but mandatory and with
  **no expression fallback** (pylatexenc's required-delimited has none; `\m x` under
  `r()` diagnoses missing-mandatory and leaves `x` alone). Asymmetry accepted for now:
  `r` does no protective-group unwrapping (the class form never did either); revisit
  with the accessor work if extraction wants it.
- `s`/`*`, `t<c>` → `MarkerArgumentParser`; `v`/`v<c1><c2>` →
  `VerbatimArgumentParser::new(GroupType::Verbatim)` (+ `.with_delimiters`).

Factory specs carry no names and no per-argument deltas (attach via `ArgumentSpec`
builders). No flyweight cache and no singletons: specs are built once per language.
`e{…}` embellishments and `AnyDelimited` stay deferred with their parsers.
Rejected alternatives: accepting a `&[&str]` list-of-codes signature alongside (one grammar, one
entry; the string form covers the deferred `e{…}` shape too when it arrives).
(Reversed — the list form is now primary; cf.
[§dd-dr:argument-specs-list-primary].)

#### `GroupArgumentParser`: the expression fallback is an orthogonal knob [§dd-dr:expression-fallback]

Status: DECIDED (user, follow-up session).

Previously variant-implied (class form: always; rule form: never), the fallback is now
a stored `bool` with a builder setter (`with_expression_fallback`); the constructors
keep the parity defaults (`new` → on, pylatexenc's `'{'` acceptance; `with_rule` → off,
pylatexenc's required-delimited). Semantics are **uniform across forms**: when no group
of the delimited form opens, the argument is the next single expression, parsed under
the **plain argument state** — in the rule form the minted rule is *not* in force
during the fallback (its spellings read as the language reads them; installing it there
would manufacture stray-close tokens for a group that never opened). Absence diagnoses
missing-mandatory with the knob on or off (the fallback engine is the shared
expression core, not `ExpressionParser::parse_argument`, so the condition never flips
with the flag). The two new combinations are techy extensions: class + off ("a real
group or a diagnosed missing argument" — pylatexenc's `'{'` cannot say this) and
rule + on (opt-in; the `r` code's pinned no-fallback default constrains the *code*,
not the parser's capability ceiling — the code still resolves to the off default).
*Motivation:* the knob documents the fallback property on the type's own surface, and
composes with the anticipated `Rules(Vec<…>)` multi-delimiter generalization (fallback
as an orthogonal knob, not a variant property). Alongside, the content-designation
contrast — delimited form: the group's *children* (delimiters are argument syntax);
`ExpressionParser` and the fallback: the expression *node* itself, delimiters included
— is now documented on both parser types (it was previously only implicit in [§dd-dr:nodes] and
the parity table).

#### Paragraph-break emission is a driver flag: `ParagraphBreakStyle` [§dd-dr:paragraph-break-style]

Status: DECIDED (user).

`with_paragraph_break_style`: `Chars` (default — the core hook's whitespace-chars
shape, pylatexenc-legacy's) or `Specials` (pylatexenc-modern's shape — a
`Specials`-formed callable named by the **canonical** `"\n\n"` vocabulary key, its
span covering the actual whitespace run, its argument-less `SpecialsSpec` minted per
break). Node-level only: the token stays `ParagraphBreak`, and the emitted name lives
in no provider, so it is invisible to `iter_symbols` enumeration.
Rationale: (user) paragraph breaks are special enough to warrant a dedicated driver
flag; correlating the shape with package contents would be error-prone and
counterintuitive — and factually dead configuration: the tokenizer detects paragraph
breaks within leading whitespace, *before* the specials scan can run, so a
package-registered `"\n\n"` specials entry could never fire.
Rejected alternatives: probing the scope stack for a `"\n\n"` entry inside
`make_paragraph_break_node` (the first sketch — package-correlated behavior, plus a
swallowed `ProviderError` in a hook with no diagnostic channel); reordering the
tokenizer's detection priority (specials before paragraph breaks) — tangles
whitespace skipping for one preset feature; caching the spec `Arc` on the driver —
would cost `LatexlikeDriver` its `Copy`/`Eq` config-value nature to save a
negligible per-break allocation (specs are behavior, never compared).
Revisit if: a *scoped* shape switch is ever wanted — the flag is driver-global by
design; per-scope suppression already exists orthogonally through the
paragraphs block's `enabled` gate (verbatim's features-disabled state uses it).

#### The acceptance suite: an integration-crate port of pylatexenc's walker slice [§dd-dr:acceptance-suite]

Status: DECIDED (user).

`techy/tests/acceptance.rs`, public API only — an integration test crate, so
anything the port cannot reach is an API gap by construction (chosen over a
`#[cfg(test)]` module reusing `test_support`; the promotions above are the dedup
half of that decision). Conventions: descriptive behavior-named tests with
`pylatexenc:` provenance comments; span-exact `{range} {summary}` outlines; every
happy-path input parsed under **both** recovery modes with tree identity asserted;
`check_tree_invariants` on every parse. The referenced specs are registered
test-side (`testdb`), with two parity mechanisms worth remembering: `\text`/`\mbox`
carry per-argument text-mode deltas (pylatexenc's `args_math_mode` as ordinary
`ArgumentSpec::with_state_delta` data — restoring the math openers and un-forbidding
`$` statically), and the `test_errors` document resolves its unregistered macro
names through a bottom-of-stack `FallbackProvider` pushed via `ScopeOp::ReplaceStack`
(pylatexenc parity: unknown macros parse as argument-less nodes rather than
erroring; simultaneously the fallback machinery's acceptance exercise). Argument-code
call sites are list-shaped (`args(&["o", "m"])`), anticipating the factory's
list-of-codes signature ([§dd-dr:argument-specs-list-primary]).

#### `argument_specs` goes list-primary; the compact string is `argument_specs_from_str` [§dd-dr:argument-specs-list-primary]

Status: DECIDED (user; revises [§dd-dr:argument-specs-factory] and reverses its
list-signature rejection — a recorded reversal, July 2026).

One code string per argument is the primary signature — `argument_specs(["o", "{"])`,
generic `I: IntoIterator, I::Item: AsRef<str>` (the `Command::args` idiom; `&str`
itself is not `IntoIterator`, so a stray compact string is a compile error, never a
misparse). Each element holds exactly one code with its parameter characters;
surrounding whitespace is tolerated, anything more is `TrailingCode`, and an empty
element is `EmptyCode` (an empty *list* still declares zero arguments). The compact
whole-spec grammar survives unchanged as `argument_specs_from_str` — pylatexenc's
default spec database and FLM's feature definitions stay directly portable — and the
`v` whitespace-disambiguation rule is now purely a property of that compact grammar
(`["v"]` vs `["v||"]` needs none: this quirk leaving the primary API's contract is
half the motivation). `ArgumentCodeError` locates errors with **both**
`index: Option<usize>` (the list element; `None` from the compact string, and plain
`usize` on the list-only `TrailingCode`/`EmptyCode` variants) and `offset: usize`
(byte offset within that particular string) — user's call, keeping byte-exact
reporting in both forms. Twin functions, not a conversion-trait overload: matches the
`_named`-accessor precedent (no polymorphic input types), and coherence would force
enumerated per-type impls anyway. Internally one scanner (`scan_code`) reads a single
code for both entry points, so the grammar cannot drift.
Rejected alternatives: a typed `ArgumentCode` enum as the primary currency (duplicates the
parser vocabulary one level up; hand-built `ArgumentSpec`s with concrete parsers are
already the fully-typed path — the factory's value *is* the compact codes).

#### `latexlike::minidefs`: a toy definitions package, deliberately not a database [§dd-dr:minidefs]

Status: DECIDED (user, API-review policy session).

`techy::latexlike::minidefs` ships a single package, `"minilatex"`, mirroring only the
handful of LaTeX commands one reaches for automatically: `\emph`, `\textbf`, `\textit`,
`itemize`, `enumerate` — with `\item` available inside the two list environments (the
natural fit is the body-scoped-definitions mechanism, making minidefs its in-tree
exemplar). The *positioning* is the decision: minidefs is a **debug/prototyping tool**,
nothing more — just enough to test the machinery and skip setup overhead on a first
run. Decisive reason: the latexlike preset configures a parser *so that it can parse
latexlike content*, not so that it can parse LaTeX documents; anything techy ships
would fall short of a true package-structured database capable of realistic documents
(figures, theorems, proofs), while frameworks built on techy (FLM, a latex2text
successor — [§dd-dr:goals]) will roll exactly the package structure they want.
Constraint: **no binding reference to `minidefs` from any other latexlike module**, so
the compiler trivially dead-strips it from builds that never import it.
Rejected alternatives: a pylatexenc-parity standard-definitions database in techy —
whether as in-crate module, cargo feature, or companion crate — rejected at the
positioning level, not the mechanics level (the database belongs to the frameworks
above techy); naming the module `defs` (overclaims — it is precisely *not* the
definitions database that name suggests).
Revisit if: a genuinely shared cross-framework definitions layer emerges — that would
be its own crate with its own owner, not a techy module.

Shape: one file
`latexlike/minidefs.rs`; a single public item **`minilatex_package()`** — named for
the package, not a generic `package()`, keeping room for future mini-siblings —
`LLL`-generic per [§dd-dr:latexlike-generalization], returning a
bare `Package` (it carries the
argument-code factory's `ArgumentExt<LLL>: Default` bound); activation always
explicit. Specs: `\emph`/`\textbf`/`\textit` =
`MacroSpec` `"m"` (fallback on); `itemize`/`enumerate` = `EnvironmentSpec` with a
body delta pushing the inner `"minilatex.item"` package defining `\item` (`"o"`) —
the body-scoped exemplar. Per [§dd-dr:base-package], minilatex also
carries `~` and the ligatures, their
"text-mode-only" visibility expressed generically as the language's **seed
mode** (`LLL::initial_state_data().mode` — the mode role trait deliberately has no
text-mode constructor, [§dd-dr:latexlike-generalization]; for
`Latexlike` the seed mode is `Mode::Text`).

#### Argument-code and factory additions: `BracedOnly`, named factory, text-restore event [§dd-dr:argument-factory-additions]

Status: DECIDED (user, API review).

Two additions to the latexlike argument vocabulary, and one reshaped wish:

1. **`"BracedOnly"` word code** (list form only; the `AnyDelimited` precedent): a
   mandatory *content-class* group with the expression fallback **off** —
   `GroupArgumentParser::new(Content).with_expression_fallback(false)`. "Braced"
   names the class's delimiters, not literal `{}`: with `<`/`>` declared as the
   content-group delimiters, `<arg>` is accepted. `m` itself stays TeX-faithful
   (fallback on, [§dd-dr:expression-fallback]) and gains a loud doc callout.
2. **`argument_specs_named([("o","greeting"), ("m","name")])`** as a sibling
   factory: `ArgumentSpec::named` exists but composing it meant rebuilding specs by
   hand — the docs recommend names while the API fought them; a single
   tuple-accepting factory hits blanket-impl coherence walls, so the deliberate
   list/compact duality gains a named sibling.
3. **Text-mode arguments are an event, not a factory.** The `\text{…}` recipe is
   an `ArgumentSpec` state delta carrying the preset event
   `latexlike::Event::ExitMathContext` (the `.event(…)` argument recipe) —
   composable with every argument shape, optional included. An older guide recipe
   was repaired: it statically reset `forbidden_chars` and `groups`, clobbering
   embedder customizations. The event's pillar is `exit_math_context_delta`,
   restoring the first *non-math* enclosing context (never seeking a text-mode
   state), minus the transient gates — semantics and pillar functions:
   [§dd-dr:enclosing-state-stack].

Rejected alternatives: a canned `text_mode_argument()` factory (composes with
nothing — a text-mode *optional* argument would need a second factory; codifies the
buggy recipe); a `text_argument_state_delta()` helper (barely shorter than the delta
it wraps; one more permanently-stable name); code names `GroupOnly`/`StrictGroup`; a
single-char code (near-invisible next to `m`); reusing xparse's `g` (means a
deprecated *optional* brace group — actively misleading).

Revisit if: compact-string parity for `BracedOnly` is demanded by real spec tables.

#### The latexlike preset generalizes over a `Lang` family: role traits + `LatexlikeLang` [§dd-dr:latexlike-generalization]

Status: DECIDED (user, API-review policy session — direction and shape; detailed
design in later review sessions).

Every latexlike preset component — `LatexlikeDriver`, `MacroSpec`/`SpecialsSpec`, the
environments machinery (`EnvironmentSpec`/`BeginSpec`/`EndSpec`/`EnvironmentBehavior`/
`VerbatimBehavior`), `argument_specs`, `default_token_rules`, `builtin_package`,
`minidefs`, the `NodeRef` sugar — becomes generic over a preset `Lang` family
(conventional parameter `LLL`), erasing the **preset-fork cliff** (a language needing
its own node exts/state/modes had to implement `Lang` and thereby forfeited every
preset component; the ext system served only full forks). The audit finding that
carried the shape: the preset's `Latexlike`-coupling is almost entirely *vocabulary
threading* — only two genuine LaTeX facts live in logic (the `$` forbidden-char merge,
the math-delimiter table). Mechanism, three layers:

- **Per-vocabulary role traits**, implemented by the vocabulary types themselves
  (method-based): `LatexlikeGroupType` (`content_group()`, `math_group(form)`,
  `verbatim_group()`, classifier `math_form()`, predicate `is_math()` —
  [§dd-dr:math-group-form]); `LatexlikeCallableType` — role accessors
  **`macro_callable()` / `environment_callable()` / `specials_callable()`** with
  predicates `is_macro`/`is_environment`/`is_specials` (the
  role-plus-vocabulary-noun pattern, which dissolves the `macro` keyword problem
  as a side effect — `r#macro`/`macro_`/`macro_kind`/`macro_type` rejected);
  `LatexlikeMode` — trimmed to `math_mode()` + `is_math()`, deliberately no
  text-mode constructor and no `is_text` (the only known consumer was the
  restore-to-text pillar, re-specified as `exit_math_context_delta`,
  [§dd-dr:enclosing-state-stack]); **`LatexlikeEvent`** — constructor +
  recognizer for the exit-math-context event
  (`exit_math_context()`/`is_exit_math_context()`, coherence contract mirroring
  `math_form`), bound on `LatexlikeLang::Event`, because the exit-math delta is
  an *event* the `LLL`-generic argument factory must mint in the host's own
  `Event` type and the driver must recognize — a preset-side event wrapper would
  violate vocabulary-stays-the-host's-own, and an event-less design cannot exist
  (the patch depends on the enclosing stack at use time); and
  **`LatexlikeInvocationSyntax`** — implemented by the Lang's invocation-syntax
  payload type (`type Env: EnvironmentSyntax<L>`, form constructors, accessors —
  [§dd-dr:invocation-syntax]) — so the preset's staging sites and
  `SourceRecomposer` work over any `LLL`. techy implements all five for its own
  `GroupType`/`CallableType`/`Mode`/`Event`/`InvocationSyntaxData`, so a language
  adopting the preset vocabulary as its
  associated types satisfies the bounds with zero code; a language with extended
  vocabularies implements them itself, which *guarantees* the preset-required values
  exist while leaving the enum open for its own additions. `ClosedVocabulary` is
  **not** a role-trait supertrait — "provide, don't require"
  ([§dd-dr:iter-symbols]).
- **`LatexlikeLang`**, the umbrella: `trait LatexlikeLang: Lang<GroupTypeId:
  LatexlikeGroupType, CallableTypeId: LatexlikeCallableType, ModeId: LatexlikeMode>`,
  carrying **defaulted behavior methods** for language-level statics (e.g. the
  math-interior adjustment generalizing the `$` merge — the default must derive the
  forbidden set from the math-class rules being removed, never restate a literal
  `'$'`; the math-delimiter data behind `default_token_rules`). Deliberately **no
  blanket impl** (it would make the defaults un-overridable by coherence); opting in
  is `impl LatexlikeLang for Flm {}`. Evolution posture (feeds the stability rubric): the
  initial required surface freezes at stabilization; future roles/behaviors arrive as
  defaulted methods delegating to existing ones (non-breaking); a fallback-less new
  role is a conscious breaking change.
- **`Lang` stays whole; pillar functions are the composition mechanism.** The preset
  ships every `Lang`-hook behavior as a public `LLL`-generic function
  (`latexlike::initial_state_data`, the `finalize_node` spec-dispatch,
  `default_token_rules`, `builtin_package`), and a framework's `Lang` impl delegates in
  one line per hook, augmenting freely (`finalize_node`: preset dispatch, then own ext
  attachment). The residue (~30 lines: associated types + one-line bodies) is
  irreducible by the strata rule — S1 never names a preset ([§dd-dr:three-strata]), so
  preset behavior can only enter core-called hooks through the framework's own bridge
  code; no trait topology removes it.

The preset keeps `NodeExts = ()` per-member for node/argument — the ext budget
belongs to the framework built on top; preset semantics encode in the *vocabulary*
(role traits), never in the ext system — while `SlotExt` is claimed by the preset
for trait-based body marking ([§dd-dr:slot-roles]); the preset's `make_node_ext`
is the trivial `()` mint.

Rejected alternatives:

- **Extraction-only lifting** (free-function cores, types stay monomorphic) — the
  cliff is mostly *types* (spec types, environments machinery); functions cannot lift
  trait impls.
- **Plugin-slot preset** (`Latexlike<X: LatexlikeExt = ()>`) — pure sugar once the
  role traits exist, walls off vocabulary extension, and adds a second way to be a
  latexlike-family language against the one-canonical-path ruling
  ([§dd-dr:public-namespace-topology]). Reconsider at a real FLM probe only if the
  `Lang`-impl residue proves heavy.
- **Decomposing `Lang` into facet traits** (`LangTypes` + `InitialStateDataProvider` +
  `StateTransitionFinalizer` + `SpecialsProvider` + `NodeFinalizer`), in all three
  Rust realizations: the *supertrait* reading delivers nothing (a subtrait cannot
  default a supertrait's methods and the orphan rule blocks preset-side impls, so
  `impl SpecialsProvider for MyLang {}` reaches only the core-neutral defaults the
  whole `Lang` already gives); *marker-gated blankets* (`impl<T: UseLatexlikeSpecials>
  SpecialsProvider for T`) have exactly one blanket slot per facet trait crate-wide
  (competing with the `TrivialLang` quick-start blanket), are wholesale-only with a
  coherence cliff at the first customization, and cannot be replicated by downstream
  frameworks for *their* extenders (orphan rule, uncovered type parameter); *strategy
  associated types* (`type Specials: SpecialsProvider<Self>` naming preset ZSTs)
  genuinely plug in but split the coherence-coupled hook pairs across authors (seed ↔
  `finalize_transition`, scan ↔ trigger-chars — "both hooks have the same author" is
  the documented soundness argument), founder on unstable associated-type defaults
  (every non-`TrivialLang` language names 4–5 more types; the on-ramp cliff
  steepens), and win nothing for the dominant preset-plus-own-additions mode
  (`finalize_node`), where a wrapper ZST wraps the same delegation body. Regret
  asymmetry: a framework can adopt the strategy pattern privately today with zero
  techy support, while un-decomposing a public `Lang` is breaking.
- **Role mapping via associated consts, `From<GroupType>` bounds, or equality bounds**
  (`L: Lang<GroupTypeId = GroupType, …>`) — consts and `From` cannot express
  payload-carrying roles (`math_group(form)`) nor defaulted-method evolution;
  equality bounds freeze hosts to the preset enums (that shortcut falls out of the
  role traits for free via techy's own impls).

Acceptance test: the FLM compile probe —
a custom `Lang` with node exts reusing driver, spec types, token rules, and the
builtin package.

Revisit if: a real ecosystem of interchangeable facet implementations materializes
(strategy traits are then addable without breakage), or a required role with no
sensible default becomes unavoidable (accepted as a conscious breaking change).

#### `GroupType::Math(MathGroupForm)`: inline/display is typed class payload [§dd-dr:math-group-form]

Status: DECIDED (user, API-review policy session; supersedes the delimiter-fact
half of [§dd-dr:group-taxonomy] — that entry's revisit condition fired).

`GroupType` becomes `{ Content, Math(MathGroupForm), Verbatim }`, with
`MathGroupForm { Inline, Display }` a **closed (exhaustive) enum**; the rule author
declares the form at `GroupRule` registration, and the preset sugar becomes
`NodeRef::math_form()` = `group_type()?.math_form()` — no table, no string matching,
no state lookup, correct for embedder-registered and mid-parse-minted delimiters. The
`MATH_DELIMITERS` table dissolves into `default_token_rules` (rule construction, its
only remaining consumer) and stops existing as read infrastructure.
The decisive structural argument: **`group_type` is the one datum that already flows
from rule registration into the stored tree** (rule → token match → `GroupData`), so
the class payload needs no new plumbing — every alternative home has to build some.
Supporting principles: the node tree must contain all logical information about the
parsed content (inline/display was previously recoverable only by re-matching recorded
delimiter text against a static preset table — effectively re-parsing a source token —
and failed on custom delimiters); pylatexenc parity (`LatexMathNode.displaytype`) is
restored. [§dd-dr:group-taxonomy]'s operative concern survives intact: parse wiring
stays single-armed (`Math(_)` matches once; interior delta, visibility, and
forbidden-char logic are form-blind).

**Payload-admission rule** (so class payloads don't become a dumping ground): a
payload is admissible only when it is (a) parse-behavior-invariant (a single wiring
arm), (b) semantically universal for downstream consumers, and (c) declared at rule
registration, never derived from delimiter spellings. Inline/display passes all
three; a hypothetical `Content(BraceKind)` fails (b).

Role-trait shape ([§dd-dr:latexlike-generalization]): constructor + classifier —
`math_group(form: MathGroupForm) -> Self` and `math_form(self) ->
Option<MathGroupForm>` with the coherence contract `math_group(f).math_form() ==
Some(f)`, plus the predicate `is_math(self)` defaulting to `math_form().is_some()`.
The split is deliberate: the driver's math plug keys on `is_math` (parse behavior),
readers key on `math_form` (presentation); an extending language with a math-like
class that has no inline/display presentation overrides `is_math` to decouple them.

Naming: **`MathGroupForm`, not `MathStyle`** — "style" collides with typesetting
style (fonts, script level, `\displaystyle`): `$\displaystyle …$` renders
display-*style* math inside an inline-*form* group. The type names the form in which
the math group appears, not how its content is typeset. Exhaustive because renderers
match on it constantly and a wildcard arm on every consumer is a permanent tax
against a third form nobody can name.

Boundary note: `\begin{equation}` records no form — it is an environment (its body
enters `Mode::Math` via the body delta; no math *group* node exists), and the logical
information is the environment name. The completeness principle is not violated
there.

Rejected alternatives: a per-`LLL` delimiter→form table (fixes the hard-coding but
keeps read-time string matching and stays blind to dynamically registered rules — no
principled read-time lookup exists once the rule set is state-dynamic);
`GroupNodeExt` (nominally the "kind-specific per-instance parse result" home, but it
still needs a rule-side source for the form, and the preset claiming the ext budget
would force an ext-composition story onto every framework — the preset stays
`NodeExts = ()`); two bare classes `MathInline`/`MathDisplay` (forks the parse wiring
— the original [§dd-dr:group-taxonomy] concern; the payload keeps one arm).

Revisit if: a third math-group form with distinct downstream semantics is identified
(the closed enum makes adding it a conscious breaking change, accepted).

#### The preset driver: pillar functions + generic `LatexlikeDriver<LLL>` assembly [§dd-dr:preset-driver-pillars]

Status: DECIDED (user, API-review session).

The component the generalization ruling left open resolves as **both, layered** —
the same shape [§dd-dr:latexlike-generalization] chose for `Lang` (whole type +
pillar composition): the driver's behavior ships as public `LLL`-generic **pillar
functions**, and **`LatexlikeDriver<LLL>`** is the canned assembly whose hook
bodies are precisely the one-line delegations (`PhantomData<LLL>`; it carries
exactly 7 one-line hook bodies; shipped drivers keep `Clone + Debug` — the
optional source-resolver field drops `Copy`/`Eq`, on which nothing in-crate
relied; the resolver field is private behind
`with_source_resolver`/`source_resolver()`, the two policy knobs `pub`).
Pillar inventory: `math_group_interior_delta` (the math plug —
forbidden set derived from the removed math-class rules, never a restated `'$'`;
its rustdoc documents the **two-component** math-interior obligation — the
pillar's delta **plus** the engine's `expecting_group_close` descent invariant; a
composed `…interior_state()` helper was rejected as a two-line composition,
wrong for languages overriding the math plug),
`exit_math_context_delta` (taking **`&ParsingStateStack`**,
[§dd-dr:enclosing-state-stack] — constructible post-parse via
`from_node_ancestors`, so post-parse state synthesis is served without a
session), and
`make_paragraph_break_node` (documented parse-side-only — synthesis stages
`Chars` directly and never mints tokens); `resolve_command` composes
`resolve_command_in_scopes` ([§dd-dr:resolution-extraction]) with the macro role —
no separate pillar. Why both: structs cannot be partially overridden and subtraits
cannot re-default supertrait methods (the recorded facet-decomposition flaw), so
pillars are the only mechanism serving a framework wanting
preset-behavior-plus-one-custom-hook (FLM's documented `refine_diagnostic`
posture); pillars alone would make the plain-Latexlike consumer hand-write ~30
delegation lines for nothing. Not a dual path: component vs building blocks is the
`StdCallableSpec`-vs-`impl CallableSpec` relationship — the struct contains no
behavior the pillars don't. Driver knobs: **nothing
added** — `recovery`/`paragraph_break_style`/resolver are orthogonal config
values and every other behavior difference is a different driver over the
pillars; a `with_group_interior_delta` closure knob was rejected (re-grows a
behavior-carrying driver; the pillars compose in a custom driver — one doc
sentence at the struct). The FLM
projection confirmed the pillar inventory covers every non-default hook body
(25 code lines of Lang delegation residue, 7 driver delegation one-liners —
both within envelope).

Rejected alternatives: generic struct only (a customize-one-hook framework
wraps-and-delegates ~12 trait methods or forks the bodies — the cliff returns one
level up); pillars only (every adopt-wholesale consumer pays the delegation
boilerplate for nothing).

Revisit if: a real FLM build finds a hook whose pillar signature cannot serve
post-parse state synthesis (the pillar, not the struct, is then the thing to fix).

## Rejected patterns — do not reintroduce [§dd-dr:rejected-patterns]

Quick-reference list of patterns that have been considered and rejected. Each links the section
holding the full argument.

- **`Box<dyn Node>` + `Any` downcasting + `clone_box`** ([§dd-dr:closed-node-kind]) — loses exhaustive matching, adds
  per-node boxing, and makes flat storage and serialization impossible.
- **`can_parse()`/`priority()` parser registries** ([§dd-dr:parsers-engine], [§dd-dr:deterministic-dispatch]) — behavior depends on
  registration order and dispatch logic scatters across predicates.
- **`TypeId`-keyed `Any` maps for state/node extensions** ([§dd-dr:parsing-state]) — runtime-typed and
  allocation-heavy; `L::StateExt`/`L::NodeData` do the same statically.
- **Per-facet parsing-state traits (9 associated types)** ([§dd-dr:parsing-state]) — values behind compile-time
  associated types can't be changed by runtime deltas; also textbook generic proliferation.
- **Whole state behind an `L::State: ParsingStateModel` getter trait** ([§dd-dr:state-option-c], Option B) —
  abstracts storage nothing needs, at real cost: the engine still needs a wrapper for derived
  caches; trait laws (getter purity, delta locality, stored/effective split) silently *become*
  the design; compound getters need `Cow` shapes; ext access needs capability traits; "default
  plus one tweak" means delegation boilerplate; and `dbg!(state)` lies because effective values
  are computed on read. Option C (stored data + `finalize_transition`) gets the same
  centralization with truthful debugging and hot-path field reads.
- **Closure-shaped state deltas** ([§dd-dr:parsing-state]) — a delta must stay a reified value so it can be
  merged, inspected, and propagated to base states its producer never saw (outward
  `\newcommand` propagation); a closure supports none of that.
- **Hard-coded math-mode definition tables in libraries** ([§dd-dr:specs]) — violates [§dd-dr:no-privileged-concepts];
  state-receiving `SpecsProvider` lookups cover it without built-in modes.
- **`ConflictStrategy` for library resolution** ([§dd-dr:specs]) — shadowing *is* the intended semantic
  (`\newcommand`, group-local defs), not a conflict to configure away.
- **`SourceId` into an external store** ([§dd-dr:sources-and-spans]) — circumvents borrow checking; the id is
  meaningless without its store.
- **`'src` lifetimes on AST/error types** ([§dd-dr:sources-and-spans], [§dd-dr:errors]) — self-referential structs and
  lifetime chains across N tree transformations; Arc spans fix both at negligible cost.
- **Per-location provenance chains (`via` vectors)** ([§dd-dr:sources-and-spans]) — pay per-node cost for information
  that is constant per source; provenance lives on `Source`.
- **Byte-level `Read`/`BufRead` streaming** — the parser needs
  lookahead/backtrack, so it wants a cursor over `&str`, not a byte stream.
- **Tokenizer-level environment recognition (`\begin{…}` tokens)** ([§dd-dr:tokens]) — bakes language
  semantics into the tokenizer; `\begin` is an ordinary command, environments are a parser concern.
- **Invocation-form ids (`CallableTypeId`) on tokens resolved at parse time** ([§dd-dr:tokens]) —
  invocation form is resolution output, not tokenization output; carrying it on `Command`
  tokens re-creates the "token says MACRO, node says ENVIRONMENT" wart. (Scoped: does
  *not* apply to `Specials`, where recognition *is* resolution — the token carries the
  full `ResolvedCallable` pair, [§dd-dr:token-model].)
- **Whitespace as its own token kind** ([§dd-dr:tokens]) — every construct parser's peek grows a
  "maybe whitespace first" case; pre/post-space spans localize the cost in the tokenizer.
- **Uniform `post_space` field on `Token`** ([§dd-dr:tokens]) — post-space is a per-kind syntactic
  fact (commands, comments); the reader's token edges answer it, the field taxed every
  token.
- **Maximal-run `Chars` tokens** ([§dd-dr:tokens]) — a token is an atomic unit; run-splitting
  machinery (conservative stop sets) bought speed the node level didn't need and cost
  char-by-char construct parsing.
- **Specials trigger strings enumerated in `TokenRules`** ([§dd-dr:tokens], [§dd-dr:parsing-state]) — trigger sets can
  be large and library-driven; recognition belongs to the preset (`Lang::scan_specials`,
  name + spec in one call), guarded by the cached `TriggerChars` filter.
- **A strict dependency ladder through the crate's middle (the old L2–L6 layering)** ([§dd-dr:crates]) —
  the middle is a strongly-connected component by intention (each cycle edge is a decided
  feature); enforce the three real rules (Lang-free foundation, preset line, acyclic runtime
  ownership) instead of a fictional ranking.

---

## Open questions [§dd-dr:open-questions]

Current list — remove entries as they are settled (move the outcome into
[§dd-dr:decisions]):

- **Precompiled-table merging (`PrefixTable`++)** (known as item 1b): detection consults
  several per-state structures (the group-delimiter `PrefixTable`, the specials
  `TriggerChars`, per-rule command-escape and comment-start checks). Worth evaluating a
  single merged first-character/prefix table per state once the hot loop can be profiled
  (user request; also flagged at [§dd-arch:token]). Not a design blocker.
- **`CompactString`**: plain `String` initially; whether a small-string optimization ever
  pays for delimiter/specials storage is a profiling question, not a design question.
- **The per-invocation `Box` micro-benchmark** — deferred, unscheduled, consciously kept
  open: measure per-invocation and per-descent `Box` provision cost
  ([§dd-dr:invocation-parser-factory], [§dd-dr:parse-driver]); a fast path can hide
  behind the `cx` wrappers if profiling ever asks.
- **Diagnostic identifier scheme**: provisional (`core.<area>.*` /
  `<preset-name>.<namespaced-name>`, [§dd-dr:errors]); a final naming/identifier pass
  is due before a public release makes the strings semver surface.
