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
content genuinely differs from any source slice (synthesized content). Transient borrow
lifetimes (tokens borrowing the current source) are fine as long as they never enter the AST.

## Deterministic dispatch over registry scanning [§dd-dr:deterministic-dispatch]

Parsing dispatch follows data: token kind → construct parser, name → library lookup → spec →
invocation parser. Never "ask every registered parser if it can_parse() and pick by priority" —
that design makes behavior depend on registration order and hides dispatch logic in scattered
predicates. If syntax needs to enter the pipeline, it enters as data (a specials string, a
group type, a spec) or as an explicit replacement of a well-defined component.

---

## Non-goals [§dd-dr:non-goals]

Decided intentional limitations (PROPOSALS.md §4 gap analysis, in `dev-docs/archive/`):

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

Format: **Status** (DECIDED / PROPOSED / OPEN / DEFERRED) · date · decision · why · rejected
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
*(Amended — API-review T4 session: ownership layering, the persistent
`LineIndexCache`, and the `LineColProvider` seam: [§dd-dr:line-col-ownership];
this entry's doctrine is unchanged.)*

#### Pluggable content resolution [§dd-dr:source-resolver]

Status: DECIDED.

`SourceResolver` trait for `\input`-like lookups; `NoResolver` is a zero-sized type so a
no-I/O build pays nothing. No file-system resolver is shipped (no_std policy,
[§dd-dr:dependencies]): an embedder implements `SourceResolver` on its side, where the I/O
capability lives; the in-memory `MapResolver` covers tests and fully preloaded setups.
*(The `SourceContent` backing-abstraction half this entry originally carried was later
retired — cf. [§dd-dr:source-cursor-retired].)*
*(Direction recorded — API-review P4: the resolver instance moves from `Language` to
the `ParseDriver` (parse-time instance behavior, the placement doctrine); wiring
designed in the 2b T4 session. [§dd-dr:input-attachment].)*
*(Wiring landed — API-review T4 session: [§dd-dr:input-wiring]. `NoResolver`'s
default-slot role is replaced by the driver accessor's `None`; its own fate rides
the Tier-C batch.)*

#### Origin genericity without `Lang` [§dd-dr:origin-genericity]

Status: DECIDED (user; later revised — the default origin is a plain optional
URL string).

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
Rejected alternatives: a concrete-now/genericize-in-Phase-3 approach (would retrofit a type parameter
through every L0 signature later). Also rejected, in the July 2026 revision: the first-cut
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
serve — and the `TokenReader` contract needs deliberately *bidirectional* `move_to_pos`
(`TokenRecovery::resume_pos` moves forward), which the backward-only, debug-asserted
`rewind` actively resists. `SourceContent` fell with the cursor: as designed
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

`Span`'s `start`/`end` went private with the
`start()`/`end()` accessors, closing the gap where the `start <= end` invariant was only
advisory (`new` debug-asserts; the fields allowed silent violation). The one mutation
pattern the lib actually used — growing a chars-run/marker span rightward — became
`extend_to(end)` (debug-asserted monotone), so every mutator now preserves the
invariant. `cover(other)` (byte-range union, min/max so it is order- and
overlap-agnostic) was added at the same time. Consistency with `SourceSpan` (private +
accessors + validating constructor) decided it over the `std::ops::Range` precedent
(public fields, no invariant) — the honest alternative that was considered and
rejected. `contains`/`overlaps` are deliberately **not** added: whichever empty-span
semantics they pick will be silently depended on, so they arrive only with a consumer,
pinned by docs + tests in the same commit. Bridging: `SourceSpan::new` accepts
`impl Into<Range<usize>>` (so a `Span` passes directly; `From<Span> for Range<usize>`)
and `SourceSpan::span()` is the inverse — `span.rs` itself stays ignorant of
`SourceSpan` (dependency direction preserved).

*(Amended — API-review T4 session: `contains(pos)` lands — `node_at`
([§dd-dr:tree-navigation]) is the consumer the deferral awaited; empty-span
semantics ruled (an empty span contains nothing), pinned by docs + tests in the
same commit. `overlaps` remains deferred, unless `covering_slice`'s
implementation wants it.)*

#### `SourceResolver` contract batch: content-returning, `Send + Sync`, no core recursion checking [§dd-dr:resolver-contract]

Status: DECIDED (user, Action-05 session; settled before any consumer existed).

- **`resolve()` returns `ResolvedContent { content, origin }`; the caller mints the
  `Source`** (the `resolve_source` composition). Rationale: provenance lives on the
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
  (no path semantics, no canonicalization) and performs no recursion checking; the
  std/I/O command-line driver enforces its own include-depth/cycle policy, with
  `Source::provenance_chain()` as the ready-made tool. Documented on the trait.
- **`ResolveError` = strings + optional structured cause**: human-readable
  `reference`/`message` stay the primary interface (a failed `\input` flattens into a
  diagnostic anyway); an optional `Box<dyn core::error::Error + Send + Sync>` cause
  travels the standard `Error::source()` chain so embedders can downcast (e.g.
  `io::Error` kind). Consequence: `ResolveError` is no longer `Clone` (single-owner
  box; nothing relied on it).
- Smalls: forwarding impls (`&R`/`Box<R>`/`Arc<R>`), a compile-time object-safety pin
  (drivers may store `Arc<dyn SourceResolver>`), `MapResolver::with_reference_as_origin`
  (its blanket impl narrows to `O: From<String>` — a convenience type may narrow;
  exotic origins write their own ten-line resolver).

*(Amended — API-review T4 session: (1) the cause field becomes
`Option<Arc<dyn core::error::Error + Send + Sync>>` and `ResolveError` derives
`Clone` again — principle recorded: **techy error types stay uniformly `Clone`;
out-of-crate information sits behind the `Arc`**; `Error::source()` downcasting is
unaffected, `with_cause` wraps with `Arc::new`. (2) "No core recursion checking"
SURVIVES the engine now recursing on its own stack ([§dd-dr:input-wiring]) —
reaffirmed for `.dtx`-style legitimate self-inclusion; the embedder's policy tools
are [§dd-dr:include-chain-helpers].)*

#### Include-chain tools: `including_sources` + `check_include_chain`; recursion stays embedder policy [§dd-dr:include-chain-helpers]

Status: DECIDED (user, API-review T4 session).

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
  -> Result<(), ResolveError>`** (home: the source topic) — the canned
  cycle-plus-depth check a resolver calls with `?`. Keying design (user-driven):
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

Status: DECIDED (user, API-review T4 session).

Who computes and caches line/column stays LAYERED, never the `Source`: the parse
computes nothing ([§dd-dr:lazy-line-col] holds); the diagnostics renderer keeps its
per-call cache; **persistence belongs to whoever holds a `LineIndexCache<O>`** — a
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
entry points gain `_with(&mut impl LineColProvider)` variants, the no-argument
forms remaining as transient-cache shorthand (shorthand-not-second-path). Editor
tools with incremental line tables — surviving per-keystroke re-parses that mint
new `Source`s — plug in without recomputation: the Arc-keyed cache's
edit-invalidation limit is answered at the right layer.

Query-surface additions ruled with the ownership: `LineIndex::line_of(offset) ->
Option<(usize, Range<usize>)>` (line number + byte range — the caret/underline
path; the inverse `line_range(line_no)` skipped — no demonstrated consumer,
additive later); `line_col_span(impl Into<Range<usize>>)`; `DEFAULT_MAX_SCAN_LEN`
raised 100 000 → **500 000** (still bounded; the loud docs on silent `None` past
the bound stay).

Rejected alternatives: a `Source`-owned lazy cache (blocked dep-free — `alloc` has
no `Mutex`, `OnceCell` costs `Sync`; recorded at the renderer cache since its
introduction); `Source` as a `Lang` trait or a `SourceAnalyzer` associated type on
`Lang` (the source model is deliberately Lang-free — [§dd-dr:origin-genericity] is
load-bearing for Lang-free rendering and tooling; and precompute/lazy/incremental
are strategies of one pure function — no consumer is generic over them);
per-node/per-span `line_col()` methods (hidden per-call index build, O(k·N); the
bind-the-`Arc` one-index pattern is the guide example); a shipped caret/underline
renderer (presentation policy frozen forever under P5 for a ~10-line hand-roll
once `line_of` exists; `format_position`'s output shape is documented as not a
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

Supersedes four earlier proposals — uniform `post_space`, maximal-run `Chars`,
`Ok(None)` at end of stream — each recorded below as rejected; the token topic moves
wholly into S1.
Final model: `Token<'s, L> { kind, span, pre_space }` with `TokenKind<'s, L>` =
`Char(char)` | `GroupOpen`/`GroupClose` | `Command { name, post_space }` |
`Specials { name, spec: Arc<dyn CallableSpec<L>> }` | `Comment { content, post_space }` |
`ParagraphBreak` | `EndOfStream`. The decisions, each with the argument that carried it:

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
  returns a `SpecialsMatch` carrying name **and** the full resolution — `callable_type` +
  spec, the `ResolvedCallable` pair — in one call: scanning/lookup normalization or scoping mismatches are
  impossible by construction, and unknown-name fallback is the scan's own business (a
  `Specials` token's spec is never absent). It is a
  `Lang` hook (the `finalize_transition` precedent), *not* a per-library protocol and
  *not* a swappable dyn object in the state: the hook receives the state, so it can
  dispatch on `ext` and pushed libraries — everything a swapped object could express,
  without a state field. Hot-path guard: `Lang::specials_trigger_chars(&StateData)`
  reports possible first characters (`TriggerChars`; `Any` = conservative fallback for
  dynamic scanners), cached per state instance like the `PrefixTable` and consulted before
  any dyn call. The scan returns `TokenResult`, so scanner errors participate in the
  recovery-token protocol.
- **Syntactic vs. content whitespace** — the principle that decides every whitespace
  placement question: *pre-space is content whitespace* (belongs to the document flow;
  becomes whitespace chars nodes, [§dd-dr:nodes]), *post-space is syntactic whitespace* (consumed by
  the construct's syntax, ignored as content, reproduced verbatim in recomposition).
  Post-space exists only where *tokenization syntax* consumes whitespace — multi-character
  `Command` names (whitespace terminates the name) and `Comment`s (the newline terminates
  the content) — and is stored **in those variants**, not as a uniform `Token` field
  (`Token::post_space()` accessor serves `move_past`'s `skip_post_space` flag). Groups
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
  `Arc<dyn CallableSpec<L>>` (tokens are `Clone`, not `Copy`), and `TokenError<'s, L>` may
  grow state context. Tokens remain transient `'s`-bound engine internals; the genericity
  never enters the AST. `Span` — a generic byte range used by errors and cursors
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
  invariant, flag-free `move_past`/`move_to`, and a field-free `EndOfStream`.)
- *Uniform `post_space: Span` on every token* — post-space is a per-kind
  syntactic fact; the WIP's variant-embedded instinct was right, and the uniform field's
  only justification (the `skip_post_space` flag) is served by an accessor.
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

#### Zero-copy tokens with ephemeral lifetime [§dd-dr:zero-copy-tokens]

Status: DECIDED (upheld through the token redesign).

`Token<'s, L>` holds `&'s str` slices plus `Span`s; `pre_space`/post-space are `Span`s, not
`String`s. The `'s` lifetime never enters the AST.
Revisit if: a streaming token source can't expose stable slices (that calls for a
chunked-content reader design — see [§dd-dr:sources-and-spans]'s `SourceContent` retirement entry — not a
change to the token type).

#### `TokenReader` is the behavior extension point for tokenization [§dd-dr:token-reader]

Status: DECIDED (user).

`StdTokenReader` is driven by the parsing state (rules data + cached tables + the
`scan_specials` hook); anyone needing genuinely different tokenization *behavior*
(catcode-like schemes, non-textual sources) implements the trait. `peek` deliberately
receives `&ParsingState<L>`, not `&TokenRules` — a catcode-like reader keeps its tables in
`L::StateExt` ([§dd-dr:crates]). The peek/move_past/move_to protocol with `skip_post_space` /
`rewind_pre_space` flags follows pylatexenc's proven `LatexTokenReaderBase` design; the
flags are not vestigial (`\verb`-style parsers reposition before swallowed post-space).
**Peek idempotence contract:** repeated peeks at one position with the *same state
instance* return the same result; implementations may memoize keyed on (position, `Arc`
identity) — sound because states are immutable and `derived()` always mints a new `Arc`. A
different state, however trivially derived, voids the obligation. (`StdTokenReader` does
not memoize yet — no premature optimization; the contract permits it.)

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

`Command { name, escape_char: char, post_space }`.
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

1. *`TokenKind::Comment` carries `start: Span`* (the matched start delimiter — mirrors
   `NodeKind::Comment`: which delimiter fired is a per-instance fact). The content span is
   `start.end..post_space.start`; consumers must **never** reconstruct it from
   `content.len()` — the previous `post_space.start - content.len()` arithmetic (duplicated
   in the nodes parser and the noise scan) silently assumed `content` was sliced verbatim
   from the source, and a custom reader that normalizes content would underflow it: a
   lib-code panic reachable from a legitimate impl of a public trait.
2. *Dangling-escape recovery uses a `Char(escape_char)` placeholder* spanning the escape
   byte (`resume_pos` = its span end). The byte joins the pending chars run, so the tolerant
   parse keeps the partition invariant — consistent with [§dd-dr:errors]'s recovery principle (markup
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
4. *`move_to_pos(pos: usize)` is a required `TokenReader` method*, replacing the deleted
   `resume_at` helper (which synthesized a zero-width `EndOfStream` marker and called
   `move_to` — bypassing `StdTokenReader`'s bounds/char-boundary guards and silently
   imposing a "`move_to` must be span-derived" contract on implementors). Deliberately
   **no default body**: a positional move is a distinct capability every reader must
   answer for, not a marker-token trick to inherit. The std readers' trait impls delegate
   to their guarded inherent versions (the inherent forms remain — calling through the
   generic trait needs `L` pinned; the delegation keeps the two from diverging).
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
   construct parsers make no forward parsing decision from raw content;
   `ParseContext::source` exists for staging `SourceSpan`s (and slicing the text of
   spans already consumed through tokens, e.g. an environment name — span rendering, not
   scanning). Cost accepted: char-at-a-time reads are slower than a substring search,
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

#### `TokenListReader` demoted to internal test infrastructure [§dd-dr:token-list-reader-demoted]

Status: DECIDED (user).

Compiled under `cfg(test)` only, `pub(crate)`, removed from the
public exports. Every consumer is an in-crate test; its load-bearing role is the lockstep
reader-agreement harness (each construct-parser suite runs every parse against
`StdTokenReader` *and* a pre-scanned `TokenListReader` and asserts identical trees, stops,
and diagnostics — the enforcement mechanism for "construct parsers never reach around the
reader"), plus hand-built token lists for engine tests. Its fixed-list fidelity gap — no
re-tokenization under the peek state, so state-driven parsers like the verbatim recipe
cannot run over it — is fine for a test tool but disqualifies it as a public reader
contract. Rejected alternatives: deleting it outright (loses the lockstep verification); keeping it
public (a maintained API surface nothing external needs).

#### `TokenRules::multi_newline_paragraphs` (renamed from `double_newline_paragraphs`) [§dd-dr:multi-newline-paragraphs]

Status: DECIDED (user).

Any run of two or more newlines (however many, with interleaved inline whitespace) forms one paragraph break; the old name misread as
"exactly two". *(Later joined the `enable_*` family as `enable_multi_newline_paragraphs`
— cf. [§dd-dr:enable-flags].)*

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
empty data is the *constitutive* off (the language has no such feature) — pylatexenc
precedent. Uniformization rider: `whitespace` loses its `Option` (plain `WhitespaceRules` +
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
StateData<Self>`, default: every syntax gate off, no libraries, default ext), and the
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

Status: DECIDED (user; settles parity item N1 jointly with the `ParseDriver` entry,
[§dd-dr:parsers-engine]).

`Lang` gains `type ModeId` (`Copy + Eq + Debug + Send + Sync`; `()` under `SimpleLang`) —
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
honestly plain data); an interior delta or events payload on `GroupRule` (the N1
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

Status: DECIDED (user-led, API-review T1/T2 session; application with the review batch).

The parse **machinery**, not the state model, keeps the enclosing context:
`ParserSession` maintains a stack of enclosing `ParsingState`s — push/pop at the same
descent points as the traceback frame stack ([§dd-dr:parse-traceback]), a scoped
`with_parsing_state(closure)` form for takeover parsers, innermost-first iteration
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
  signal). It becomes **fallible** (folded into `DeriveError`, default `Ok(())`): a
  context-requiring event reaching bare `derived()` errors loudly instead of being
  silently dropped. The seed still never runs it.
- **`ParseContext::derive_state(&delta)`** (+ scoped `with_derived_state`) is the
  parser-facing derivation: it lowers context-dependent events through the new driver
  hook **`ParseDriver::resolve_state_event(&event, &StateStackView) ->
  Option<ParsingStateDelta>`** (default `None` = context-free, left for
  `finalize_transition`), merges the patches, strips the lowered events, then calls
  plain `derived()` — one choke point preserved. Per-event *policy* lives on the
  driver; the event *loop* lives in the one cx method — parsers never iterate events.

First consumer: the latexlike text-restore ([§dd-dr:argument-factory-additions]) —
the driver walks to the nearest text-mode state (else the outermost) and restores its
whole `TokenRules`; core learns nothing about modes. The preset's event logic (math
entry, text restore) ships as **public pillar functions** (post-generalization
`LLL`-generic; the hooks are one-line delegations) so post-parse processing can
synthesize coherent recorded states for constructed nodes — restaged or synthetic
children emulating "enter math"/"restore text" ([§dd-dr:transform]).

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

*(Amended — API-review T3 session: the text-restore pillar is renamed and
re-specified as **`exit_math_context_delta`** — the delta is defined by *exiting
the math context*: look up the first non-math enclosing group in the stack and
restore that context, never by seeking or naming a text mode as the target
(consistently, the mode role trait carries no text-mode constructor —
[§dd-dr:latexlike-generalization] amendment). The "nearest text-mode state"
wording above reads accordingly; `restore_text_context_delta` is a superseded
name.)*

*(Amended — API-review T5 session: the hook's stack view becomes the owning,
session-independent type **`ParsingStateStack`** — it holds
`Vec<Arc<ParsingState<L>>>`; the session stores its live stack as one and lends
`&` to hooks (zero extra cost; the states themselves are never copied). The
`ParsingStateDelta` specificity precedent rules out bare `StateStack`, and
"View" misnames an owning value — `StateStackView`/`StateStack` both superseded.
It is constructible outside any session: `from_states(states)` and
**`from_node_ancestors(node)`** — the node's own recorded state first, then
parents outward via the stored parent table ([§dd-dr:tree-navigation]), i.e.
exactly the ruled innermost-first/current-state-first order — so post-parse
synthesis feeds the same pillar signatures the driver hook feeds
([§dd-dr:preset-driver-pillars] amendment). Contract note: the walk's sequence
is not entry-for-entry the parse-time stack (ancestor chains contain Arc-equal
duplicates and non-group nodes); the documented contract is the **scan
semantics** — first non-math state, outermost fallback — which duplicates cannot
affect.)*

#### `TrivialLang` (renamed from `SimpleLang`): the test lang, not an on-ramp [§dd-dr:trivial-lang]

Status: DECIDED (user, API-review T3 session).

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

Status: DECIDED (user, API-review T3 session).

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
redefinition, group-local definitions), so a configurable conflict policy (PROPOSALS.md's
`FirstWins`/`LastWins`/`Error`) solves a non-problem while complicating resolution; an optional
lint can warn on shadowing if ever wanted.

#### `SpecLookup` receives a `CallableQuery` (query struct), not bare `(ct, name)` [§dd-dr:callable-query]

Status: DECIDED (closes the deferred half of [§dd-dr:lexical-shadowing]).

`lookup(&CallableQuery, &ParsingState<L>) -> Option<Arc<dyn CallableSpec<L>>>`, where
the query carries `callable_type`, `name`, a `CallableSyntax` (`Command { escape_char }` /
`Specials` / `Other`), and `token: Option<&Token>`.
*Why a syntax field:* with several `CommandRule`s in scope, `\foo` and `#foo` both tokenize as
`Command { name: "foo" }`, and the escape character is **not** recoverable from the token alone
— a token carries spans and borrowed substrings, not access to the source content behind them.
So the syntax context must be explicit data on the query.
*Why the token too (and why `Option`):* lookups may want `pre_space`/span context (user
request); it is optional because specials resolution happens *inside* the scan hook before any
token exists, and synthesized invocations never have one. The struct form absorbs future
context fields without dyn-trait signature churn.
Rejected alternatives: bare `(ct, name, state)` (forces presets to multiply `CallableTypeId`s to encode
syntax); a mandatory `&Token` parameter (lifetime noise on a dyn trait, and inconsistent —
sometimes there is no token).
*Mode-awareness*, as proposed: the `&ParsingState<L>` parameter lets a preset's lookup dispatch
on `state.ext()` (FLM's `\vec` in math mode); the core `Library` ignores state, syntax, and
token alike. This replaces PROPOSALS.md's hard-coded `math_mode_macros` tables, which
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
`SimpleLang` defaults both to `u32`. The planned `Language<L>` interning machinery for these
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
`Lang` (and `SimpleLang`) gained a `'static` bound — free in practice, a `Lang` is a
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
to every hand-written `Lang` impl (the SimpleLang-cliff cost, cf. [§dd-dr:parsers-engine]) for a need the
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
one position are the same spelling, the tie rule *is* redefinition shadowing. One
deviation from the plan-session sketch: provider-side `scan_specials` returns
`TokenResult` (the exact shape of the `Lang` hook it feeds), not
`Result<_, ProviderError>` — scanning providers keep the tokenizer's recoverable-error
protocol, and the `ScopeStack::scan_specials` fold propagates the first `Err`
(innermost-first) with no translation. Per-provider trigger chars are deliberately
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
  iterates `ALL`. Deliberately **not** required by `Lang`: `SimpleLang` defaults the
  id types to `u32`, which has no value list. The preset implements it for all three
  vocabularies; `#[non_exhaustive]` enums keep `ALL` in sync by same-change
  discipline (compiler can't enforce it).
Rejected alternatives: an `Option`al type filter (generic listing without the vocabulary bound —
user preferred always-filtered plus statically listable vocabularies); a
`&ParsingState` parameter (nothing beyond the mode feeds visibility); state-blind
enumeration with visibility data carried on entries (information without a consumer);
excluding specials or a separate `iter_specials` (the recorded-type framing unifies
the tables with no extra surface).

*(Amended — API-review T3 session: `ClosedVocabulary` stays opt-in under the
generalized preset — **not** a role-trait or `LatexlikeLang` supertrait
("provide, don't require", user): no shipped function requires the bound; the
did-you-mean miss detail needs no vocabulary enumeration (its callable type and
mode are already in hand at the miss site, [§dd-dr:resolution-extraction]); and
the parse-init escape-char check ([§dd-dr:registration-ergonomics]) ships as a
bound-where-used check function — wired unconditionally on the monomorphic preset
path, narrowly bounded at the generic-`LLL` wiring point, and gracefully absent
for non-enumerable vocabularies (a best-effort diagnostics nicety, not
semantics).)*

#### Registering callables: conversion idiom, one-liners, no insert-time validation [§dd-dr:registration-ergonomics]

Status: DECIDED (user, API-review T1/T2 session).

Three rulings on the registration surface:

1. **Arc removal via one sealed conversion idiom.**
   `ParsingState::lang_initial_with_packages` takes an `IntoIterator` over a sealed
   **`IntoSpecsProvider`** conversion (accepting `Package<L>` by value, `Arc<P>`, and
   `Arc<dyn SpecsProvider<L>>`); `Package::insert`/`insert_specials`/`…_in_modes` get
   the sibling treatment for specs — `insert(CallableType::Macro, "emph",
   MacroSpec::new(…))` with no `Arc::new` anywhere, pre-shared flyweights still
   accepted. (A plain `Into<Arc<dyn …>>` bound cannot express this: unsized coercion
   is not `From`, and blanket impls hit coherence walls — the sealed trait is the
   mechanism.) The `insert` vs `insert_specials` parameter-order flip is fixed while
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
   **did-you-mean** detail iterates the scopes' advertised symbols
   ([§dd-dr:iter-symbols]) and reports near-misses — at minimum the
   initial-escape-char case, optionally a small edit-distance check (accepted
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

*(Amended — API-review T3 session: (a) the did-you-mean detail's home is the miss
arm of the extracted `resolve_command_in_scopes`
([§dd-dr:resolution-extraction]); it enumerates *symbols*, not vocabularies — no
`ClosedVocabulary` dependency. (b) The parse-init all-escape-char warning is
realized as a public bound-where-used check function (`where
L::CallableTypeId: ClosedVocabulary, L::ModeId: ClosedVocabulary`) —
[§dd-dr:iter-symbols] amendment.)*

#### Command resolution is a standalone `specs` function: `resolve_command_in_scopes` [§dd-dr:resolution-extraction]

Status: DECIDED (user, API-review T3 session; completes the deferred resolver half
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
([§dd-dr:registration-ergonomics]) lives in this function's miss arm;
`ScopesResolvingDriver` ([§dd-dr:scopes-resolving-driver]) is its one-line
wrapper; the resolution-condition wire areas
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

Status: DECIDED (user, API-review T3 session).

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
`Arc<ArgumentSpec>`). Final slot-constructor arities land with the ext-minting
application ([§dd-dr:ext-minting] makes `SlotExt` non-defaultable).

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
node kinds; custom data rides in the two-tier ext bundle (`Lang::NodeExts: NodeExtTypes` —
uniform `NodeExt` + per-kind `<Kind>NodeExt`, all bounded `Clone + Debug + Default`; the
`Default` gives builders their no-ext value, mirroring `StateExt`). `NodeExtTypes` is defined
next to `Lang` in the state topic, not in `node/` (moving it would recreate a module cycle for
cosmetics); `SimpleLang` + blanket impl provides the all-defaults shortcut.
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
  enshrined as a core node kind — and it made the two-tier ext affordable.
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
  the exact sibling-span partition invariant. Args vs. slots stay two named concepts over
  shared machinery — the boundary is a spec-owned guideline, not core law.
Rejected alternatives: `trait Node` + `Box<dyn Node>` + `as_any()` downcasting + `clone_box()` (the
generated trait-based design) — loses exhaustive matching, adds per-node boxing, makes
serialization and flat storage impossible, and reintroduces runtime type errors that the
type system should prevent; annotation wrapper nodes (re-create the problem one level up);
side tables (break node self-containment across tree transforms).

*(Amended — API-review P4: the tier-2 per-kind ext half of this entry is superseded —
per-kind node exts are removed and `NodeKind` becomes purely structural,
[§dd-dr:ext-minting]. The closed structural taxonomy, the `Callable` merge, de-keyed
specs, owned names, and `TextContent` are untouched.)*

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
  per-argument data (e.g. `{ref_domain, ref_key}` from a `fig:Abc` argument) — populated
  by custom argument parsers or `Lang::finalize_node`.
Rejected alternatives: parallel `specs`/`args` vectors (pylatexenc-literal — an unenforced
length/pointer-consistency invariant and a redundant `Arc` when the spec also sits in the
entry); "layout" as a name (opaque — nobody could say what it referred to).

*(Amended — API-review P4: `ArgumentExt` is minted by the argument parser at record
creation — the parser output carries it, the record constructor demands it, and the
standard parsers are conditionally defined `where ArgumentExt<L>: Default`;
`Lang::finalize_node` no longer exists as a populate-later path. [§dd-dr:ext-minting].)*

#### `SlotExt` — slot records carry per-instance ext, symmetric with `ArgumentExt` [§dd-dr:slot-ext]

Status: DECIDED (user).

`ParsedSlot` gains `ext: SlotExt<L>`
(`Lang::NodeExts::SlotExt`, `()` under the no-ext bundle), mirroring
`ParsedArgument.ext`. Rationale: the asymmetry bit exactly where FLM is richest — an
environment's *body* is a slot, and per-instance derived data about a body (tabular cell
structure, enumerate item boundaries) had no home except the whole-callable ext. Added
while cheap: one associated type on the bundle, one field on the record; retrofitting after
downstream `NodeExtTypes` implementors exist would break them all.

*(Amended — API-review P4: `SlotExt` values are demanded at `ParsedSlot` construction
(no `Default` path); slots additionally gain a `SlotRole` and trait-based body
marking, with the preset claiming the `SlotExt` member. [§dd-dr:ext-minting],
[§dd-dr:slot-roles].)*

#### `NodeTree::iter` renamed `iter_storage_order`; no `parent` stored in `NodeData` [§dd-dr:iter-storage-order]

Status: DECIDED (user).

The flat iterator yields storage
(breadth-first) order — `a`, `c`, `b` for `a{b}c` — which a name as generic as `iter`
invites consumers to mistake for document order; the rename makes the iteration order
part of the signature. The document-order `descendants()` arrived with the read API
([§dd-dr:read-api]), once it had consumers. Upward navigation (`parent: u32` in
`NodeData`, `parent()`/`next_sibling()`/`ancestors()`) was considered and declined as not
needed — the transient parent vector `finish()` computes for region resolution stays
transient. Named argument-node accessors (`argument_nodes_named` etc.) likewise landed
with the read/extraction package, not piecemeal.

*(Amended — API-review P4: the parent-navigation half is reversed — `finish()`'s
parent vector is now kept on the tree (`parent()`/`index_in_parent()`), consumers
having materialized; the `iter_storage_order` rename stands. [§dd-dr:tree-navigation].)*

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
  [§dd-arch:nodes] partition invariant, mechanically checkable).
- **Consequences accepted:** a callable's child list is the raw-syntax view (child count ≠
  argument count; `\frac 1 2` costs two whitespace `Chars` nodes); an argument has no single
  node identity — transforms and views splice child *ranges* ([§dd-dr:read-api]);
  `NodeRef::argument(i)`/`argument_named()` are replaced by region/content-nodes accessors;
  `ParsedArguments` holds no `TextContent`, so its materialization plumbing is deleted.
  `CallableData.post_space` deliberately stays a field: it lies outside the region tiling
  and is whitespace-only by construction (trailing comments are never consumed).
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
obligations.

*(Amended — API-review P4: the builder becomes hook-free with a single
`add(kind, span, state, children, ext, annotation)` demanding ready values; the
staging semantics recorded here — bottom-up, claim-once, breadth-first flatten,
unreachable dropped — are unchanged. [§dd-dr:ext-minting], [§dd-dr:node-annotations].)*

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
regions session; **superseded** — API-review recompose session: the
reconstruct-don't-record half is reversed — scaffolding facts are recorded as
invocation-syntax payload, cf. [§dd-dr:invocation-syntax] and the amendment note
below).

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
guess. The partition invariant holds in its callable form: regions tile the child list, the
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

*(Amended — API-review recompose session, SUPERSESSION: the per-node recomposition
doctrine ([§dd-dr:recompose] amendments) reversed reconstruct → **record** — the
begin/end facts (per side: escape char, command word, post-space, name-group rule)
are recorded on the node as the environment arm of the Lang-owned invocation-syntax
payload ([§dd-dr:invocation-syntax]). What stands: the rigid parse syntax (with
strictness now Env-owned — a tolerance variant is a newtype over
`StdEnvironmentSyntax`), and both recorded rejections above — scaffolding is still
neither nodes nor slot records (the recompose session separately rejected the
`Hidden`-slot storage design). The "tolerated and *not recorded*" post-space clause
no longer holds: the per-side record keeps it.)*

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
   span (pylatexenc's `macro_post_space`); nothing beyond it is ever claimed. Whitespace
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
4. *End of stream:* `EndOfStream.pre_space` materializes as a final whitespace-only `Chars`
   node.
5. *Partition invariant:* sibling spans partition the parent's *content interior* exactly —
   `List` bodies, `Group` interiors, the root. For callables: argument/slot regions tile
   the child list (builder-enforced), the children block is span-contiguous, and unrecorded
   rigid scaffolding is the reconstructible complement (previous entry). Checked
   mechanically by a test-utility `check_tree_invariants()` — deliberately a test aid, not
   builder law, so a future construct that legitimately breaks byte-accounting amends a
   test, not the architecture.

*(Amended — API-review recompose session: invariant 3's *storage* moves — the core
`CallableData.post_space` field is replaced by the Lang-owned invocation-syntax
payload ([§dd-dr:invocation-syntax]); the recorded fact and its token-only rule are
unchanged (latexlike records it in `Macro { escape_char, post_space }` and per
environment side). The kind.rs invariant-3 rewording and the parse-law checker's
callable arm (byte accounting now reads the invocation-syntax payload) are Phase 3
application items.)*

#### Cross-tree `NodeId` misuse: debug-only provenance tags [§dd-dr:node-id-provenance]

Status: DECIDED (user, code-review follow-up session; **superseded** — API-review P4:
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
  best-effort — `span()`/`source_text()` are *exact* by the [§dd-arch:nodes] partition invariant
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
  (exact sub-spans, zero-copy text). Result wrappers (`Split`, `KeyVals`) own their
  tree privately and expose **primary access** (`segment(i)`, `segments()`,
  `keyval(i)`, `get(name)`) as `NodeSlice` views (user requirement) — one currency,
  so every helper composes with every other (re-split a segment, walk `descendants()`
  of a derived tree). Documented edges: partials of *owned*-content chars nodes
  (materialized trees) keep the whole original node's span as provenance (no byte
  mapping exists to subdivide); partial nodes carry default ext; derived trees'
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

*(Amended — API-review P4: extract-built trees are `NodeTree<L, ()>`
([§dd-dr:node-annotations]); boundary partials are minted via `make_node_ext` instead
of carrying default exts ([§dd-dr:ext-minting]); rebasing the `Split`/`KeyVals`
builders on the restage mechanism so they can keep/map annotations of annotated input
trees is a recorded later option ([§dd-dr:restage]; decide in 2b).)*

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

Status: DECIDED (user, API-review T1/T2 session).

`argument_nodes_named`/`argument_content_nodes_named` return `Result<Option<…>, E>`:
`Err` = category error (the node is not a callable, or the name is not among the
spec's declared arguments — the misspelling trap), `Ok(None)` = precisely "declared
but absent", `Ok(Some)` = present. The *indexed* accessors stay pure-`Option` (the
crate-wide Option-on-mismatch idiom), with the `argument_nodes` contract sentence
replicated on all of them and a pointer to the `_named` forms as the distinguishing
alternative. Decisive reason: for a *name*, a silent `None` on a typo is a trap with
no cheap call-site discriminator — and names, unlike indices, are exactly the form the
API recommends; the error is a `Result`, never a panic ([§dd-dr:panic-policy]).
Rejected alternatives: `Result` on the indexed accessors too (forks the crate-wide
Option idiom where `arguments().get(i)` + `is_provided()` already discriminates);
panicking on unknown names (this family is the non-panicking companion shape by
design).

#### `display_tree()`: a free debug renderer; `NodeKind::as_str()` [§dd-dr:display-tree]

Status: DECIDED (user, API-review T1/T2 session).

A free public function `display_tree(node) -> String` renders a subtree one line per
node: box-drawing guides + `summary()` + **line/col** positions (internal per-source
`LineIndex`), printing a source name only when it changes from the previous line
(multi-source trees; the initial source is omitted). Deliberately a *free function*,
not a `NodeRef`/`NodeTree` method (user): lean surface, trivially dead-code-eliminated
when unused. The output format is human-oriented and explicitly not a stability
contract (`summary()`'s caveat restated); v1 ignores tree annotations. Companion
accessor **`NodeKind::as_str()`** → `"Chars"`/`"Group"`/`"Callable"`/`"Comment"`/
`"List"` (the visualizer's own need and an independent T1+T4 wish): `as_str` is the
Rust idiom for a static variant name. Placement: the node read group beside
`summary()` — display, not content extraction (not `extract`); replaces the rejected
elaborate plain-text extraction (that gap belongs to the totext companion project).
Rejected names: `label()` (reads as user-provided/dynamic data), `kind_as_string()`
(stutters as `NodeKind::kind_as_string`; `_string` connotes allocation), `name()`
(sibling collision with `NodeRef::name()`, the callable's spelling).

#### `validate_tree`: the all-trees law as a `Result`, in `core::node` [§dd-dr:tree-validation]

Status: DECIDED (user, API-review T5 session; realizes [§dd-dr:restage]'s validator
rider; application pending).

`pub fn validate_tree<L: Lang, A>(tree: &NodeTree<L, A>) -> Result<(),
TreeViolation>` (with `#[non_exhaustive] TreeViolation { node: Option<NodeId>,
kind, … }`) checks the **all-trees law** — what every finished tree must satisfy
regardless of origin: structural sanity (child ranges in-bounds, after-parent,
single-parent, all reachable), region tiling on resolved records (content ranges
within content parents, content-parent-inside-region), `TextContent` residency
(valid char-boundary range of the node's own source), regions resolved.
Deliberately NOT checked: byte partition, children-share-parent's-source, sibling
source order — the parse-tree law, which legitimate transform output (spliced,
reordered, synthesized nodes) breaks by design. It returns `Err`, never panics:
its persona is a framework validating rebuilt/spliced trees at runtime (FFI
included) — the panic policy's outer-layer case; the panicking
`check_tree_invariants` keeps its declared test-utility role for the parse-tree
law (and must scope its byte accounting per source via the `Attached` role once
`\input` lands, or every multi-source parse tree fails it; the two doc pages
cross-reference — all-trees law ⊂ parse-tree law).
**Home `core::node`, not `techy::transform`** (user): the function checks the
universal node-tree law and accepts any tree — transform output is merely its
commonest client; placement follows logical function, not audience. Name
`validate_tree`: the verb deliberately differs from the panicking `check_*`
family because the contract differs; the walkthrough wish-name
`check_transform_tree_invariants` under-claims (parse trees pass too) and is
superseded. A `validate_parse_tree` sibling (all-trees law + parse-law geometry)
was proposed and **withdrawn** together with the byte-reconstruction guarantee
([§dd-dr:recompose] amendment): the geometric half certifies only that
gap-filling reproduces source *bytes*, while the semantic half — that those bytes
match the tree's *content* — is parse provenance no runtime checker can verify;
a checker that cannot check what its users would believe it checks is a trap.
Revisit if: a runtime consumer genuinely needs the geometric parse-law check —
additive as a sibling, with the semantic limitation stated on it.

## Tree transformation, annotations, and ext minting [§dd-dr:transform]

The API-review P4 session's coherent redesign of the post-parse surface, ruled as one
piece — the entries below cross-depend — plus the 2b T5 session's exact-type
detailing and the recompose session's machinery/payload rulings. None is applied yet
(application in the review's Phase 3, together with the P1 topology move). Working
detail for the application sessions: `dev-docs/api-review/P4_RULING.md`,
`T5_RULINGS.md`, and `RECOMPOSE_RULINGS.md` (process files, deleted when
the review completes — these entries are the durable record).

#### Per-tree node annotations: `NodeTree<L, A = ()>` [§dd-dr:node-annotations]

Status: DECIDED (user, API-review P4 session; application pending).

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
literally). Extract-built trees (`Split`/`KeyVals`) produce `A = ()`; rebasing the
extract builders on the restage mechanism so they can keep/map annotations is a
recorded later option (note on [§dd-dr:read-api]). FFI note (T5): a binding fixes one
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

*(Amended — API-review T5 session: accessors ruled — `NodeRef::annotation()`,
`NodeTree::annotations()`, `annotate()` in storage order with the loud doc
sentence ([§dd-dr:restage-ops]); the read types gain the defaulted `A` parameter
at application (`NodeRef<'t, L, A = ()>`, `NodeSlice`, `Descendants`, the extract
helpers — every existing spelling keeps compiling). The "extract-built trees
produce `A = ()`" clause and its recorded later option are **superseded**: the
user ruled annotation handling into present scope — extract producers mint output
annotations through a general callback ([§dd-dr:extract-annotations]).)*

#### Extract producers mint annotations: general callback + suffixed shorthands [§dd-dr:extract-annotations]

Status: DECIDED (user, API-review T5 session; supersedes the `A = ()` extract
clause of [§dd-dr:node-annotations]; application pending).

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
synthesized nodes are just another callback call.
Part context: one opaque accessor-based struct per op (`SplitPart`/`KeyValsPart`
working names), accessors admitted under the inclusion test — *only what the
callback cannot recover itself*: the original node (`original() ->
Option<NodeRef>`, `None` exactly for the synthesized `List` wrappers/roots;
`copied_from` rejected — partials are cut, not copied), partial-piece info,
segment/entry index. KeyVals keys are plain strings, not nodes — no key-side
annotations arise. Final accessor names land at the application naming pass.
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

Status: DECIDED (user, API-review P4 session; supersedes the debug-only scheme of
[§dd-dr:node-id-provenance] — that entry's revisit condition fired).

Every tree layout mints a `TreeTag` (newtype over `u32`, from the existing wrapping
global counter) in **all builds**; `NodeId` becomes `{ index: u32, tree_tag:
TreeTag }` (8 bytes, `Copy`) and the tag **participates in `Eq`/`Ord`/`Hash`** — ids
minted by different trees are different values, so one map can key ids from several
trees, and an old tree's `NodeId` stored inside a new tree's annotation is
unambiguous (the enabling substrate of [§dd-dr:restage]'s origin-by-convention).
The old exclusion from `Eq`/`Hash` existed only because tags were debug-only; with
tags everywhere, debug and release agree again. `NodeTree::get()` now genuinely
rejects foreign ids in release builds (the T5 binding-pattern caveat disappears);
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

Status: DECIDED (user, API-review P4 session; supersedes [§dd-dr:finalize-node] and
the two-tier ext half of [§dd-dr:closed-node-kind]; application pending).

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
  -> NodeExt`** replaces `finalize_node` — value-return, `kind` by shared reference
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
  default body without `Default`); `SimpleLang`'s blanket impl supplies the `()`
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

Status: DECIDED (user, API-review P4 session — direction, shape, and contracts;
exact types/naming in the 2b T5 session; application pending).

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
  `Restage<B> { Continue(B), Emit(Vec<BuildId>) }`. `Continue(b)` = the driver
  restages this node over its children's results with annotation `b`, and **the
  visitor continues through every child subtree** — the safety invariant: the only
  way a child subtree goes unvisited is an explicit `Emit` for its ancestor (no
  shallow-keep exists to reach by accident). `Emit(nodes)` = the callback staged the
  replacement itself (empty = drop); no automatic descent. (`Continue` kept as the
  name for now; alternates recorded: `Descend`, `Keep`, `Retain`, `Auto`.)
- **Annotations, single pathway** (user's redesign, replacing a run-level mapper
  closure): *every* restaged node's annotation passes through the visitor — as
  `Continue(b)` or as an explicit argument to the staging ops the callback invokes.
  Mandatory by construction: `A_old` and `A_new` are different types, so "keep the
  annotation" is not even expressible — good by design; the origin-id convention is
  the one-liner `Continue(Ann { origin: node.id(), … })`.
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
  parent-level callback takeover.
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
- Riders: a **transform-tier validator** (structure + region tiling + `TextContent`
  residency, *minus* the parse-law byte accounting) fills the gap between the
  builder's checks and `check_tree_invariants`' parse-tree law; `NodeRef::tree()`
  becomes public.

Rejected alternatives: a companion crate (version skew during co-evolution;
`techy-totext` is the external-consumer proof instead); a fixed atomic-op vocabulary
(add/drop/splice/rebuild) as the ceiling (user: not powerful enough — the driver's
fixed job is only order mediation and region-preserving reassembly); a `finish()`
BuildId→NodeId map (helps only callers who separately tracked BuildIds; the
annotation channel is strictly more direct).
Revisit if: the 2b T5 detailing finds the bundle shapes insufficient for a real FLM
pass — the layering (primitive / driver / raw builder) is the stable part, the op
signatures are not frozen until then.

*(Amended — API-review T5 session: the revisit clause fired and the op surface is
ruled — exact types in [§dd-dr:restage-ops]. `Restage::Continue` is finalized as
**`Descend(B)`** — the name states the always-descends invariant in itself;
`Continue` said too little, `Keep`/`Retain` actively suggested the shallow-keep
misreading, `Auto` was vague (all four superseded). The transform-tier validator
rider landed as `validate_tree` in **`core::node`**, not `techy::transform` —
placement follows what it checks, not its commonest client
([§dd-dr:tree-validation]).)*

#### Restage op surface: visitor trait, generic errors, constructible bundles [§dd-dr:restage-ops]

Status: DECIDED (user, API-review T5 session; completes [§dd-dr:restage]'s deferred
detailing; application pending).

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
  GIL-bound FFI callbacks (the primary T5 consumer). Parallel variants, if ever
  wanted, are new entry points with their own bounds (the `&mut` visitor contract
  is inherently serial).
- **Errors generic, not boxed**: `restage(tree, visitor) -> Result<NodeTree<L, B>,
  RestageError<V::Error>>` with `RestageError<E> { Build(NodeBuildError),
  ContentParentDropped { … }, Visitor(E) }` — the framework's own error type rides
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
  verbatim *by contract*, content swapped, designation re-anchored. Changing noise
  uses the visitor op (noise flows through the visitor) or the hand-built bundle;
  a both-taking helper was rejected as a second path duplicating the constructor
  modulo a one-line spec/ext transcription. P4's working name
  `stage_argument_like` superseded.
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
Revisit if: the closure blanket's `E` inference proves awkward at application —
the recorded fallback is the fixed-error shape; a flag-level change, not a
re-session.

*(Amended — API-review recompose session, vocabulary alignment: the recompose
surface mirrors this entry deliberately — `RecomposeError` variants mirror
`RestageError` exactly; `RecomposeContext`'s argument/slot helper roster mirrors
the restage family; no `Send`/`Sync` bounds, same argument. One recorded contrast:
a restage takeover stages its subtree explicitly, while a wrap-intended recomposer
returns instructions that lower against the *outermost* recomposer and never
descends explicitly — the wrapping contract; [§dd-dr:recompose-machinery].)*

#### `techy::recompose`: recomposition as a downward-state fold [§dd-dr:recompose]

Status: DECIDED (user, API-review P4 session — direction and scope; detailed design
DEFERRED to its own planning session).

Recomposition (tree → output text) is a generic fold in a top-level
`techy::recompose` module: the consumer supplies per-node logic; a typed
**recomposition state threads downward** into children; the framework is agnostic
about *how* nodes recompose. Two shipped strategies prove the mechanism:
**span-verbatim** (exact bytes via spans + gap-filling — the latexpp path, verified
byte-faithful in the T5 walkthrough incl. tolerant-recovery nodes) and **node-data
spelling** (reconstruction from recorded facts, pylatexenc's
`latexnodes._latex_recomposer` precedent — the core provides the walk, the
**latexlike preset provides the trigger spellings**, which are the only facts node
data lacks; on a `materialize()`d tree this touches no `Source` at all — fully
source-independent byte-faithful reconstruction). latex2text is "a recomposition
whose per-node logic emits text, not LaTeX": the **mechanism lives here, the content
(handler databases, unicode tables, layout) in techy-totext** — consistent with
rejecting elaborate in-techy plain-text extraction. Strategies key on `SlotRole`
([§dd-dr:slot-roles]): verbatim skips `Attached` by definition (the invocation text
*is* the recomposition; descending is the explicit expansion option); `Hidden` never
participates.
Open for the dedicated session: direct fold vs transform-to-chars-then-concatenate;
state-threading model; output sink type; targeted-replacement integration.
Revisit if: the dedicated session overturns details — the module, the two
strategies, and mechanism-not-content are the ruled part.

*(Amended — API-review T4 session: the dedicated session also owns the read-only
structural walker (`enter(node, depth) -> VisitFlow { Descend, SkipChildren,
Stop }` + `exit(node)`) — a `Descendants::with_depth()` iterator adapter was
rejected because flat iteration loses structure, and the walker is recompose's
skeleton, so the walk vocabulary is designed once, there. `descendants()` itself
stays: flat iteration is legitimate for structure-free queries.)*

*(Amended — API-review T5 session, binding inputs for the dedicated session:
(1) **Per-node recomposition doctrine** (user): spans give provenance, not output
location — recomposition reconstructs each node from its **own recorded data** (a
chars node contributes its content; a callable/environment its scaffolding from
escape char + name + post_space + recorded delimiters) and never performs
**inter-node** span arithmetic ("apparent gaps" between siblings resurrect
deleted content on any transformed tree) nor reads source text beyond a node's
own recorded content. The span-verbatim strategy above is re-examined under this
constraint in the dedicated session — its sound domain is unmodified parse trees.
(2) Consequently there is **no framework-facing byte-reconstruction guarantee**
(the walkthrough-era gap-filling framing is not promised API): the parse-law byte
accounting stays an in-crate acceptance-suite *oracle* — reassembling the input
from a fresh parse proves lossless parsing — and parse-output span semantics stay
documented for the analyze-only/span-patch tooling architecture with the
provenance warning (structural edits void inter-node span arithmetic). The
interim `validate_parse_tree` proposal was withdrawn with the guarantee
([§dd-dr:tree-validation]). (3) Trigger-spelling residue on the session agenda:
node data lacks some scaffolding spellings (pathological `\begin  {name}`
spacing, multi-escape-char languages). The user's sketch — the environment
parser storing the scaffolding noise as **`Hidden` slots** (e.g.
`"begin_tokens"`/`"end_tokens"`, precise form TBD) — turns scaffolding spelling
into node data, a further argument for the per-node direction.)*

*(Amended — API-review recompose session: the dedicated session ruled; detailed
records: [§dd-dr:invocation-syntax] (the payload axis), [§dd-dr:recompose-machinery]
(the fold machinery), [§dd-dr:visit-engine] (the shared traversal engine).
Superseded above: the **two-strategies sentence** — "span-verbatim" is retired as a
named strategy (the recomposer never resolves span content, the node's own span
included; no span fast path — a tree carries no reliable freshness signal to gate
one; `span_content()` stays a public consumer affordance the recomposer simply never
uses); the open sink question — there is **no sink concept** in the machinery (value
fold; streaming = a recomposer-held writer with `Piece = ()`); and amendment (3)'s
`Hidden`-slot scaffolding sketch — **rejected** together with the `CallSyntax` role:
trigger spelling is recorded *payload* (`Lang::InvocationSyntax`), never slots. The
node-data strategy survives as the ONE preset `SourceRecomposer`; the parse-law byte
accounting stays an in-crate acceptance-suite oracle, now certifying payload
completeness with no span crutch.)*

#### Invocation syntax is recorded payload: `Lang::InvocationSyntax` [§dd-dr:invocation-syntax]

Status: DECIDED (user, API-review recompose session; supersedes the
reconstruct-don't-record half of [§dd-dr:environment-scaffolding] and the core
`post_space` storage of [§dd-dr:span-invariants] invariant 3; application pending).

**Accuracy doctrine (user):** the *preset* (the `Lang`), not core, owns
recomposition accuracy — byte-exact vs up-to-noise vs loose is the preset's choice,
implemented by what invocation-syntax information it records in node payload, in
logical canonical form. Recomposition accuracy is thereby coupled to
parse-recording accuracy: recomposition reads **raw node payload only** — no hidden
slots, no side channels (extending [§dd-dr:recompose]'s per-node doctrine). The
in-crate oracle acceptance suite (reemit == input; strict + tolerant matrices;
multi-source rides the T5 I-18 obligation) certifies payload completeness with no
span crutch — it can only pass once these recordings land (Phase 3 sequencing).

The mechanism: a new Lang-associated invocation-syntax type,
**`Lang::InvocationSyntax`**, stored as a `CallableData` field **replacing the core
`post_space` field** (and no `escape_char` is ever added to core); minted by the
invocation parser; a parse-level-syntax channel, distinct from the lang's node ext
(preset-logic info). Two-trait split:

- the **required core data-bound trait** on `Lang::InvocationSyntax`: `Clone +
  Debug + Send + Sync + 'static` plus `materialized(&self, source_content) -> Self`
  (the `()` impl is trivial) — final name at application, aligned with the
  ext-bound family (fallback `InvocationSyntaxData`);
- the **opt-in constructor trait `FromInvocation`** with `from_invocation`,
  consulted by the std staging sites (`StdInvocationParser` + the specials site)
  and implemented for `()` by techy.

**The latexlike payload** is an enum with a type-parameter default,
`InvocationSyntax<Env = StdEnvironmentSyntax<L>>`:

- `Macro { escape_char, post_space }`.
- `Environment(Env)` — the std record holds per side `{ escape_char, command_word,
  post_space, name_group_rule: Arc<GroupRule<L>> }`, the name group recorded as the
  **rule `Arc` cloned from the matched token** (user counterproposal, verified
  sound: `TokenKind::GroupOpen` carries the matched rule Arc, token.rs:45–53; the
  rule's open/close `String`s are the exact matched bytes, rules.rs:42–50; the name
  group can never exist in delimiter-diverged form — a malformed begin takes the
  chars-recovery path, environments.rs:478–493 — so rule == bytes always; the Arc
  is source-independent, hence exempt from `materialized`; and it records the group
  *class*, which byte-recording would lose). End-side facts are reported back by
  the body parser (the terminator consumer, environments.rs:545–549).
- `Specials` — a **unit variant**; Option 1 (user, reversing an earlier
  literal_form lean): `name` is the actual invocation spelling *always*, matching
  the macro rule (`\foo` vs `\fooooo`, both spec-resolved by prefix, both record
  the name as written) — no second field, no two-field rename hazard.
  Paragraph-break `Specials` nodes record the actual whitespace run as `name`; the
  canonical-`"\n\n"` contract is superseded; identification is by **spec
  identity** — the definite, identifiable paragraph-break spec object (directive:
  the latexlike driver must not mint an anonymous `SpecialsSpec::default()` per
  break, driver.rs:127; that fix is now load-bearing, not hygiene).

**Env consolidation** (user): everything anchors on the Env type — a defaulted
`LLL`-method tier was dropped (user worry upheld: too many customization entry
points on `Lang`); the single customization entry is the Lang's choice of
`InvocationSyntax` type. A new **`EnvironmentSyntax<L>` trait**, implemented by Env
types, consolidates begin/end *scanning* and payload construction in the
**accumulator shape (b)**: `parse_begin -> (NameInfo, Self)` with the end side
empty; `parse_end(&mut self)` fills it — zero extra associated types, and the
intermediate state doubles as the synthesis constructor's shape.
`EnvironmentInvocationParser` becomes generic over `LLL`, delegating scanning to
Env while resolution + argument parsing stay composition-owned (`parse_begin`
returns the name info the composition needs). Same-record/different-tolerance is a
newtype over `StdEnvironmentSyntax` (strict default; noise-tolerant swappable).
Verbatim caveat (verified): the verbatim terminator is one literal `GroupClose`
token (rules replaced; close = the full `\end{name}` string,
verbatim_parser.rs:5–24, 106–123) — end-scanning delegation cannot apply to raw
bodies; the verbatim path records std end facts from the matched literal via the
one std-facts method the trait keeps. `EnvironmentSyntax` also carries the
**spelling writers `write_begin`/`write_end`** — the Env type owns its own
re-emission (the accuracy doctrine made literal); a `source_content` parameter
resolves span-backed fields.

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

Revisit if: a construct's invocation syntax cannot be expressed as per-node
recorded payload — that is a new axis to design, not a reason to resurrect
slot-side scaffolding storage.

#### Recompose machinery: the meaning-free `Piece` fold with instruction lowering [§dd-dr:recompose-machinery]

Status: DECIDED (user, API-review recompose session; completes [§dd-dr:recompose]'s
deferred detailing; application pending).

The recompose machinery is **meaning-free** (user decoupling directive): it
composes generic *pieces* over the visit; source recomposition is ONE `Recomposer`
implementation (latexlike's), never a machinery default. Architecture = **direct
value fold** — the P4 transform-to-chars-then-concatenate alternative is dead as a
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
  `recompose::recompose(tree, recomposer)`.
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
  `_slot_content_named` / `recompose_body` — final spellings at application.
  **`RecomposeError`** variants mirror `RestageError` exactly.
- **`core_source_instruction`**: the instruction-returning free helper for the
  core-complete kinds (`B: ComposePiece + From<&str>`); it declines callables —
  their payload is Lang-owned ([§dd-dr:invocation-syntax]).
- **`SourceRecomposer<LLL>`** (public; constructor `latexlike::
  source_recomposer()`): the preset source re-emission — `State = ()`,
  `Piece = String`, instruction-only, plus a coherence error variant
  (variant/`callable_type` mismatch).
- **Targeted replacement** = the wrapper pattern (a wrapping recomposer overrides
  the targeted nodes; no span fast path) + the documented restage→recompose
  pipeline; the P4 integration question and the session's Attached-exclusion
  point close here.
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
routed by [§dd-dr:recompose]'s T4 amendment; application pending).

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

Status: DECIDED (user, API-review P4 session; amends
[§dd-dr:latexlike-generalization]'s "preset keeps `NodeExts = ()`" per-member;
application pending).

`ParsedSlot` gains `role: SlotRole { Content, Attached, Hidden }` (default
`Content`). `Content` = constitutive — the node's meaning is incomplete without it
(environment body); `Attached` = derived/redundant — reconstructible from the
invocation itself (`\input`'s resolved content, [§dd-dr:input-attachment]);
`Hidden` = framework/callable-defined attachments techy core ignores (no
recomposition, no byte accounting; semantics via slot name + spec). Load-bearing
consequence: **`Attached` slots are excluded from the parent's byte-tiling** —
declaration replaces source-change inference in the validator — while structural
child-list tiling stays role-independent.
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
Rejected alternatives: body-by-slot-name (the `"body"` string — stringly-typed);
body-by-position (slot 0 — positional convention as API contract).
Revisit if: 2b details (does `body()` also filter on `role == Content`? extract
readers vs `Hidden`? `#[non_exhaustive]`? `Attached` vs `Derived` naming) change the
edges — the enum, the exclusion rule, and the trait are the ruled part.

*(Amended — API-review T5 session, the listed edges ruled: `body()` filters on
the ext axis alone — no hidden `role == Content` conjunction (a framework's
`Attached`-body choice must not become silently unfindable; one doc sentence
instead); readers and extract stay **role-blind everywhere except recompose** —
`Hidden` means "no recomposition, no byte accounting", never invisibility to
reads (structural walks and `display_tree` show reality — debug honesty);
`SlotRole` is **exhaustive** (match-heavy consumers — validators, recompose
strategies, FFI mappings; a fourth role changes byte-accounting semantics and
must be a conscious breaking change, the [§dd-dr:math-group-form] argument);
**`Attached` confirmed** over `Derived` (T4's shipped door names
`parse_attached_source`/`attach_source_reference` already teach the vocabulary);
restage descends into `Attached`/`Hidden` children uniformly
([§dd-dr:restage-ops]).)*

*(Amended — API-review recompose session: a fourth **`CallSyntax` role was
rejected outright** — `SlotRole` stays the three-variant enum, and techy itself
mints NO `Hidden` slots (`Hidden` stays reserved for frameworks; trigger/scaffolding
spelling is invocation-syntax *payload*, [§dd-dr:invocation-syntax]). The one
role-sensitive site is made concrete: `Concat`'s default scope is plain children +
`Content` regions — `Attached` AND `Hidden` skipped, widening explicit via
`include_attached()`/`include_hidden()` — while the walk stays role-blind
([§dd-dr:recompose-machinery], [§dd-dr:visit-engine]).)*

#### `\input` attachment: same-builder sub-parse; multi-source trees are first-class [§dd-dr:input-attachment]

Status: DECIDED (user, API-review P4 session — direction and tree-level
consequences; engine wiring designed in the 2b T4 session, friction F8).

The anticipated `\input` implementation: the callable's spec parser resolves the
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
- The parse-law validator scopes byte-accounting per source via the `Attached` role
  ([§dd-dr:slot-roles]); recompose is per-source (verbatim emits `\input{file}`,
  not the content — expansion is an explicit strategy choice); `node_at`'s
  per-source descent already yields the right answers on both sides of the boundary.
- **The resolver moves from `Language` to the `ParseDriver`** (direction recorded):
  resolution is parse-time instance behavior, which the placement doctrine
  ([§dd-dr:parse-driver]) puts on the driver — amending [§dd-dr:language-init]'s
  expected surface (`Language` collapses toward the constructor alone; supersedes
  the `with_resolver` remainder of [§dd-dr:source-resolver]'s wiring).
Revisit if: the T4 wiring session finds the sub-parse-into-same-builder mechanics
unworkable — the fallback is the restage-splice route, accepting its copy cost.

*(Amended — API-review T5 session: the caching parenthetical above is closed —
input caching is neither implemented nor recommended. User-identified flaw in
the cache-then-splice recipe: `\input` can return a **modified parsing state**
to the caller — the included content's delta sequence continues into the rest of
the including document (the preamble-defines-macros case; preset-configurable
via how the `\input` spec is defined) — so a document parsed with attachment off
is wrongly-stated downstream of every state-modifying `\input`, and included
files must in general be read on the spot at parse time. Phase 4's include
chapter gets a short discussion of these challenges and presents the splice
recipe only under the explicit precondition that the framework's `\input` does
not modify caller state. Riders: the level-0 primitive stays the sanctioned
cross-tree door ([§dd-dr:restage-ops]); latexpp's verbatim output path needs no
splicing at all — recompose emits `\input{file}` per source, so per-file
pipelines compose without tree merging.)*

#### Parent links, `SourcePos` lookup, and read-side honesty [§dd-dr:tree-navigation]

Status: DECIDED (user, API-review P4 session; application pending; method naming in
2b).

- **Parent table stored**: the `Vec<u32>` that `finish()` already computes for
  region resolution is kept on the tree (4 bytes/node; reverses
  [§dd-dr:iter-storage-order]'s decline now that consumers exist — the T5 FFI gap,
  T4's F7 cursor wish, pass-style renderers). `NodeRef::parent()` and an O(1)
  `index_in_parent()` (own index minus the parent's block start).
- **`SourcePos<O> { source: Arc<Source<O>>, pos: usize }`** — a new source-model
  type, analogous to `SourceSpan`, pointing to a *single location* (constructor,
  accessors, `Debug`, line/col via `LineIndex`; `SourceSpan::start_pos()`/`end_pos()`
  conveniences). Chosen over bare `(source, pos)` arguments (reads as two unrelated
  bits) and over empty-`SourceSpan` encoding (reads oddly).
- **Point lookup** `node_at(&SourcePos)`: the **deepest** node whose span contains
  the offset — half-open containment (`start ≤ pos < end`, empty spans never match);
  descend only into children whose span lies in the **query's source** (per-source
  answers on multi-source trees, [§dd-dr:input-attachment]); only exact per-node
  spans are trusted, never inferred covering spans — robust on transform-spliced
  trees, degrading to the shallowest honest answer. An offset inside a node but in
  none of its children (group delimiters, trigger spellings) resolves to that node;
  ancestors come free via `parent()`. **Span lookup**: the minimal covering sibling
  run (`NodeSlice`, the node-list currency) within the deepest containing node list.
  Binary search over span-sorted siblings opportunistically, linear fallback.
- **Honest slices**: `NodeSlice::span()`/`source_text()` verify per-run source
  uniformity instead of trusting first/last-node agreement (a replaced *middle* node
  no longer yields silently stale text); the `finish()` single-source flag is the
  O(1) fast path.
Rejected alternatives: a build-on-demand `ParentMap` helper (the table is free at
`finish()` and trees are immutable — no staleness to manage); an offset→node index
table (premature); parent-dependent data in `make_node_ext` (impossible bottom-up —
see [§dd-dr:ext-minting]).
Revisit if: profiling shows the per-node parent word or the honest-slice scans
mattering — both have obvious opt-out designs, neither worth pre-building.

*(Amended — API-review T4 session, names finalized:
`NodeTree::node_at(&SourcePos)`; `NodeTree::covering_slice(&SourceSpan)` (the name
carries the one fact callers must know — the result may cover *more* than the
query); `NodeRef::parent()`/`index_in_parent()` → `Option`; `SourcePos` accessors
`source()`/`pos()`; `SourceSpan::start_pos()`/`end_pos()` (exclusive-end doc
sentence); `NodeRef::tree()` goes pub. `ancestors()` REJECTED — tree visiting is
top-down and an ancestry walk has zero trap surface
(`iter::successors(node.parent(), |n| n.parent())`); the one-line recipe lives in
`parent()`'s rustdoc. Vocabulary note: F7's "cursor primitive" (this entry's
editor-cursor lookup) and the retired char-scanning `SourceCursor`
([§dd-dr:source-cursor-retired]) are disjoint concepts sharing a word. `Span`
gains `contains(pos)` with the ruled empty-span semantics —
[§dd-dr:span-extend-to]'s awaited consumer.)*

*(Amended — API-review T5 session: the honest-slices bullet is contract-final —
`NodeSlice::span()`/`source_text()` answer only when the **whole run** lies in a
single source (uniformity verified across the run unless the `finish()`
single-source flag short-circuits); `source_text()` gains `span()`'s ordering
guard so the two contracts read identically; `None` = no single-source answer;
per-node accessors stay valid on any tree (a node's own span is its provenance).
Doc-vocabulary rule (user): the word "honest" must not appear in the rustdoc
contracts — state the concrete condition ("the run lies within a single source");
"honest slices" stays internal design-record vocabulary.)*

*(Amended — API-review recompose session: the read-side structural walk (T4's
routed walker) lands as the free function `walk` in the top-level `techy::visit`
module — trait `NodeVisitor` (enter/exit) + `VisitFlow`, one engine shared with
the recompose driver; no `NodeRef::walk` (strata: core cannot name the
techy-level engine). `descendants()` stays the flat stream.
[§dd-dr:visit-engine].)*

## Construct parsers, dispatch, engine [§dd-dr:parsers-engine]

#### Single-context parsing API (`ParseContext`) [§dd-dr:parse-context]

Status: DECIDED (implemented; formerly proposed).

Bundles token reader + state + session handle, avoiding pylatexenc's three-argument threading
through every parser. One place to extend later (e.g. depth limits, cancellation).
`ParseContext` also carries
`source: Arc<Source<L::SourceOrigin>>` — the source the token spans refer into, which
staging a node's `SourceSpan` requires. Factory-created parsers
(`make_invocation_parser(&self, invocation)`, later `ArgumentParser` entry points) have no
constructor through which a caller could thread it, and it cannot ride on tokens or
readers: the token layer deliberately carries only transient byte spans ([§dd-dr:errors] — no
`Arc`-span infection; a reader-side accessor would force `StdTokenReader` origin-generic
and `TokenListReader` to carry a source it doesn't have). The construct-parser layer is
where byte spans become `Arc`-backed source spans, so the context is the honest carrier.
`NodesParser::new`/`GroupParser::new` carry no redundant `source` parameters —
single source of truth.

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
    invocation: Invocation<'a, /* 's, */ L>,  // callable_type, name, spec, trigger token
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
*inside* `parse(cx)` cannot mint a **new** `Invocation` for a construct it resolves
mid-parse — `Invocation.name: &'s str`, and the `'s` source content is unreachable through
`cx` (the source is `Arc`-owned; tokens and readers carry only byte spans, [§dd-dr:errors]). So a
two-level dispatch — a `\begin` spec's parser calling the resolved environment spec's own
`make_invocation_parser` — does not work with the `Invocation` shape; the standard
composition instead drives `EnvironmentBodyParser` directly under the resolved spec.
Relatedly, a *stored* trigger token cannot be handed back to `cx.tokens` (the uniform
`parse` signature cannot tie it to the context's reader), so the
takeover post-space reposition idiom is expressed positionally:
`move_to_pos(token.post_space().start())`.)*

#### `Lang::finalize_node`: one centralized finalization hook in the builder [§dd-dr:finalize-node]

Status: DECIDED (user; supersedes a spec-level `finalize_invocation` proposal —
pylatexenc's `CallableSpec.finalize_node` precedent. **Superseded** — API-review P4:
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
*(Superseded — API-review P4: the hook becomes the value-returning, parse-time-only
`Lang::make_node_ext`; the idempotence contract and run-on-transforms behavior are
deleted. The `ParseContext`-side placement rejected above is essentially the shape now
adopted — its "forgettable" loophole closed by making `ParserSession::builder`
crate-private, so parsers cannot stage around it. [§dd-dr:ext-minting].)*

#### `Lang::resolve_command` hook [§dd-dr:resolve-command-hook]

Status: DECIDED (user; return type refined — next two entries).

`Command` tokens
resolve through `fn resolve_command(state, &token) -> CommandResolution<Self>`
(`Resolved(ResolvedCallable { callable_type, spec })` / `Unresolved { detail }`);
typically dispatches to the state's libraries via
`CallableQuery { syntax: Command { escape_char }, … }` — the token now carries its escape
char ([§dd-dr:tokens]). An `Unresolved` answer → the nodes parser diagnoses and recovers ([§dd-dr:errors]).
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
not implemented by this language — implement `Lang::resolve_command` or use a preset"),
so the forgot-the-hook wall names its own cause; `From<Option<ResolvedCallable>>` maps
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
(`core.nodes_parser.command-resolution-failed`), separate from `UnresolvableCommand` —
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

#### `Lang::make_paragraph_break_node` hook [§dd-dr:paragraph-break-hook]

Status: DECIDED (user; upgraded from "core default only").

`fn make_paragraph_break_node(state, &token) -> NodeKind<Self>`; default: a
whitespace-only `Chars` kind, `TextContent::Spanned` over the full token span (newlines
included). The *core* stages the returned kind with the token's span and the current state —
a `Lang` cannot stage nodes itself.
Rationale: paragraph-break representation belongs to the preset;
returning a `NodeKind` keeps callable-shaped paragraph breaks (FLM) expressible without a
Phase 7 redesign, while the default preserves the whitespace-as-chars invariant ([§dd-dr:nodes]).

#### Stop conditions: reified values, tier-2 predicates; abnormal endings are data [§dd-dr:stop-conditions]

Status: DECIDED (user; pylatexenc-informed).

`NodesParser` accepts a stop specification with two independent
triggers, mirroring pylatexenc's well-tested pair:
- *token condition* — a small closed enum (`Command(name)`,
  `GroupClose(group_type, close)`, `ParagraphBreak`, …) **or** a programmatic predicate
  (`Fn(&Token) -> bool`);
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
`NodesParser` returns its `StopCause` — `TokenCondition { span }` / `NodeCondition` /
`EndOfInput` / `UnexpectedGroupClose { span }` — and the *caller* decides which causes are
errors ([§dd-dr:errors]).
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
reader parked at `span.start`) or takes it whole (`consume = true`, `move_past` past any
syntactic post-space). Two reasons over the earlier always-unconsumed rule: (a) the common
closer parsers (a group parser consumes its `}`, …) stop hand-writing the consume line; and
(b) **atomicity** — consuming at match time uses the exact state that matched, whereas
leave-then-re-peek re-tokenizes at `span.start` under whatever state is *now* current, which
can reclassify the delimiter (a delta that drops the close rule makes `}` come back a
`Char`, desynchronizing the caller). A post-hoc consume helper cannot fix this — it, too,
re-peeks. `StopCause` accordingly split `StopConditionMet` into `TokenCondition { span }`
(token stop) and `NodeCondition` (node stop), and `UnexpectedGroupClose` carries a `span`: the
group parser builds its `Spanned` close delimiter from that span, which it can no
longer re-peek once the token is consumed. No `consumed` field — the cause discriminant plus
the caller's own `consume` already determine it. Consume is always `move_past(token, true)`:
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
*Observing the event* is the new `Lang::observe_transition(&mut L::SessionExt, prev, new,
&delta)`, called by `derived_state` on **every** transition event, memo hits included.
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
(`SimpleLang` absorbs it).
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
`temporary_groups_are_prefix_table_inputs`).

#### `parse_scoped` and `probe_token` replace hand-rolled swap/restore [§dd-dr:parse-scoped]

Status: DECIDED (user).

The `cx.state` swap/restore protocol was correct at every one of its
seven lib sites, but the correctness was per-site discipline (restore **before** the
`?`), and the probe site had to hold a `Result` un-`?`-ed across the restore.
`parse_scoped(state, &mut parser)` — the pylatexenc
`walker.parse_content(parser, …, parsing_state)` analog, deliberately on the *context*
(the session lacks tokens and source; the top-level drive later landed on `Language` —
[§dd-dr:language-parse-api]) — makes the restore structural; the returned delta stays
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
  `core.nodes_parser.expression-callable-requires-content`), message "…it requires
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

Status: DECIDED (user, parser-library survey; the full per-parser table lives in
dev-docs/archive/ParserLibraryParity.md).

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

#### The deferred parity parsers N2/N3/N4/N6 landed [§dd-dr:parity-parsers]

Status: DECIDED (user; the full per-parser record lives in
dev-docs/archive/ParserLibraryParity.md).

The decisions and their reasons:
- **N3 record shape — one `ParsedArgument`, structure inside** (the survey's flagged
  question): per-embellishment-char `ParsedArgument` entries are structurally
  unreachable — source order is free (`\op_{b}^{a}`) while `parse_declared_arguments`
  runs one spec at a time in declaration order, so a `^`-spec already reported absent
  could never be revisited; expressing xparse's per-char slots would need a
  multi-record argument seam for this one consumer. Instead pylatexenc's shape: one
  classless wrapper `Group` per matched pair (`GroupData::untyped`, open = marker,
  close empty), content = the wrapper run, and by-marker access as a *read-side*
  helper (`extract::split_embellishments`). Per-char access thus costs one helper
  call, not an API change.
- **N3 matching semantics** (user): noise before a marker; between marker and
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
- **N2 folds into the existing argument parsers** (the user's own `### PhF` note:
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
- **N4 `CharsGroupArgumentParser` — restriction is contents-only, math off is
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
- **N6 `TackOnFieldsArgumentParser` — an ArgumentParser staging real `Callable`
  nodes**: FLM's `label_arg` settles the integration (the tack-on parser is the
  callable's *last declared argument*; attachment = the argument's region, zero
  invocation-parser changes). Fields are configured `name → Arc<dyn CallableSpec>`
  pairs plus a `callable_type`; recognition never consults the scope stack (the
  decided no-`\label`-as-language-command reason survives), and dispatch routes
  through `ParseDriver::make_invocation_parser` — so the staged field node
  self-describes (spec, own `ParsedArguments`), frames and accessors work, and
  pylatexenc's group-wrapper hack is dropped. Two byte-keeping divergences (user):
  a repeated non-repeatable field is diagnosed (`RepeatedTackOnField`) **and kept**
  (pylatexenc parses-and-discards, which would break the partition invariants), and
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

New core trait `ParseDriver<L>`, defaulted methods only (`StdParseDriver` = the trivial
impl carrying the `Recovery` knob), bound into the bundle as `Lang::Driver`;
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
  mode this closes parity item N1;
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
*`ParseContext` doctrine:* cx returns to a data struct (tokens, source, state, session,
driver). Policy helpers (`recover`, `probe_token`) are defined on the driver with thin
delegating sugar kept on cx; invariant-bearing plumbing (`parse_scoped`, `with_frame`,
`implementation_error`) stays as non-overridable cx methods — pairing invariants must
not be overridable.
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

#### `ScopesResolvingDriver`: the canned command-resolving driver component [§dd-dr:scopes-resolving-driver]

Status: DECIDED (user, API-review T3 session).

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

#### Takeover staging sugar: `disable_all`, collection constructors, a committed invocation helper [§dd-dr:takeover-staging-sugar]

Status: DECIDED (user, API-review T3 session; the invocation helper's signature is
deliberately deferred to the T5 session).

Three shorthand rulings on the takeover-parser ceremony — all shorter spellings of
the same operations (the [§dd-dr:registration-ergonomics]
shorthand-not-second-path principle):

1. **`TokenRulesOverrides::disable_all()`** — the overrides value with all six
   `enable_*` gates `Some(false)`: the raw-state block every rest-of-line and
   verbatim-like parser hand-builds. Lives on the overrides type so it composes —
   `verbatim_state_delta` itself becomes `disable_all()` plus its terminator (one
   source of truth), and parsers tweak fields afterwards.
2. **`ParsedArguments::new(Vec)` / `ParsedSlots::new(Vec)`** — discoverable
   constructors for what only `From<Vec<_>>` impls provide today (the walkthrough
   found them by grepping, not on the types' doc pages); the `From`s stay as
   conversion plumbing.
3. **A canned invocation-staging helper WILL exist** as a `ParseContext` method
   wrapping the one staging door (`cx.stage_node`, [§dd-dr:ext-minting]) — sketch:
   `cx.stage_invocation(&invocation, arguments, slots, children, end_pos)`,
   building the `CallableData` (four of its seven fields are transcriptions from
   the invocation and trigger), computing the node span, staging, returning the
   id. Committed now; the *signature* is ruled in the T5 session together with the
   transform-side `restage_invocation` bundles and builder ergonomics
   ([§dd-dr:restage]), so the parse-side and transform-side spellings share field
   vocabulary and region semantics — fixing it against the pre-ext-minting surface
   would guarantee rework.

Rejected alternatives: `all_off()`/`raw()` for the overrides constructor (the
crate's off-vocabulary is "disable"; "raw" too clever); a terminator-less
`verbatim_state_delta` sibling (the overrides constructor composes instead of
multiplying delta helpers); ruling the helper signature now (above).

Revisit if: the T5 restage detailing changes the staging-door shape itself (the
helper follows it).

*(Amended — API-review T5 session, the committed helper's signature ruled:
`cx.stage_invocation(&invocation, arguments: ParsedArguments<L>, slots:
ParsedSlots<L>, children: Vec<BuildId>, end_pos: Option<usize>) ->
ConstructParserResult<L, BuildId>`. `end_pos: None` = the std rule — last staged
child's span end, else the trigger's end; `Some` serves takeovers whose consumed
extent outruns their last child (rest-of-line, heredoc shapes). **No
`callable_type`/`name` overrides**: the helper is the transcription-case
shorthand only — the environment takeover overrides both and its span outruns
its children ([§dd-dr:environment-scaffolding]), so environment-class
composition stays on the canonical `cx.stage_node` door (in-crate:
`StdInvocationParser` and the tack-on parser collapse onto the helper; the
environment parsers stay on the door). Parse-side/restage-side symmetry is by
**vocabulary, not arity** ([§dd-dr:restage-ops]): the parse side passes
caller-tiled records plus a flat child list, the restage side driver-tiled
bundles — who owns the region arithmetic differs by design, and the two
signatures are not to be "unified". No ext/annotation parameters (`stage_node`
mints the ext; parse annotations are `()`).)*

#### `\input` engine wiring: driver resolver accessor + the `parse_attached_source` door [§dd-dr:input-wiring]

Status: DECIDED (user, API-review T4 session; realizes [§dd-dr:input-attachment]).

- **Resolver surface**: defaulted accessor `ParseDriver::source_resolver(&self) ->
  Option<&dyn SourceResolver<L::SourceOrigin>>`, default `None` ("this language
  resolves nothing"); shipped drivers gain an `Option<Arc<dyn …>>` field +
  `with_resolver(…)`. Consequence ruled consciously: the field drops `Copy`/`Eq`
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
  holds). Resolution stays OUTSIDE the door (accessor → free `resolve_source` →
  door), so caching frameworks substitute either half; the free fn becomes the
  canonical composition once `Language::resolve_source` leaves.
- **`attach_source_reference(cx, reference, at, state, parser)`** (core, beside
  the door): the resolve-diagnose-attach bundle — kept despite its size as the
  single raising site for the two failure conditions, so diagnostics wording is
  uniform across every `\input`-variant spec and framework. Conditions:
  **`NoSourceResolver`** (`core.sources.no-resolver`) and
  **`UnresolvableSourceReference`** (`core.sources.unresolvable-reference`,
  payload: reference + the `ResolveError` — `Clone` again per the
  [§dd-dr:resolver-contract] amendment).
- **`Language` collapses**: `with_resolver`, `resolver()`, and
  `Language::resolve_source` leave — completing [§dd-dr:language-init]'s expected
  surface (`new(driver, initial_state)` + `parse` + `parse_source` + accessors).
- **The preset construct is opt-in, never preloaded**:
  `latexlike::input_macro_spec::<LLL>()` (an always-on `\input` under a
  resolver-less driver would just diagnose every use); embedders insert it into
  their own package. Its body is the brief form the helpers exist for — argument
  text → `attach_source_reference` → `Attached` slot — so `\input[options]{file}`
  / `\input*{f1,f2,f3}` variants are easy custom-spec work (the form-specific
  parts stay in the spec).

Rejected alternatives: resolver as a per-parse argument (re-litigates the P4
direction, and the construct parser mid-descent holds only `cx`);
`cx.parse_source` as the door name (collides with `Language::parse_source` under
a different contract — sibling-vocabulary rule); a core-generic
resolve-then-attach *argument parser* (speculative before a second consumer — the
door + bundle are the reusable parts).

Revisit if: a framework needs several resolvers per driver (the accessor
signature admits dispatch behind it), or the T5 restage detailing adds a
splice-a-cached-parse affordance that changes the caching-framework route.

#### `Language<L>` + `parse()`: the runtime bundle's landed surface [§dd-dr:language-parse-api]

Status: DECIDED (user; four API-shape decisions on the long-deferred runtime bundle —
staged deliberately: `ParserSession` alone shipped first, `Language` only once consumers
demonstrated the need).

`Language<L>` = `{ driver: L::Driver, initial_state:
Arc<ParsingState<L>>, resolver: Arc<dyn SourceResolver<O>> }`, long-lived, owning no
per-parse state ([§dd-dr:stateless-language]).
- **Entry points are two named methods, not a `SourceInput` enum** (rejecting an older
  sketch's `parse(impl Into<SourceInput>)`): `parse(content: impl
  Into<String>)` mints an anonymous `Source`; `parse_source(Arc<Source<O>>)` takes a
  pre-minted source (origin/provenance intact — the `resolve_source` round trip feeds
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
  `driver()`/`resolver()`; the sketch's `session()` dropped — `ParserSession` carries no `Language` borrow and `ParserSession::new()` is
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
*(Superseded, pending application confirmation — [§dd-dr:language-init]:
`ParsingState::lang_initial_with_packages` is the infallible spelling of the same
everyday operation at the seed itself, where no derivation runs; `with_provider` is
expected to be removed with the construction revision.)*

#### Language construction: explicit initial state, infallible seed+packages path [§dd-dr:language-init]

Status: DECIDED (user, API-review policy session; supersedes the construction bullet of
[§dd-dr:language-parse-api] and — pending application confirmation — [§dd-dr:with-provider]).

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
Expected consequences (proposed; confirm at application): `with_provider` and
`with_seed_delta` become redundant — seed customization moves *before* construction
(`Language::new(driver, ParsingState::lang_initial().derived(delta)?)` covers the delta
path), collapsing the builder surface to the constructor plus `with_resolver`
(orthogonal: resolver, not state); the `Default` impl's fate and the packages argument's
ergonomics (avoiding `Arc` noise) are application-time details.
Rejected alternatives: preset-level `parse()`/`parse_tolerant()` facade functions and a
configuration builder (shortcut accessors — abandoned at the first configuration need;
fix the real constructor instead); keeping seed customization delta-only (buries the
everyday setup under delta + scope-op concepts and makes it fallible for no reason the
everyday case can trigger).
Revisit if: a Lang emerges whose seed coherence genuinely requires a finalize-style hook
over the package-augmented seed — then that hook becomes an explicit, documented opt-in
on the seed-construction path, not a return to mandatory delta routing.
*(Amended — API-review P4: `with_resolver` is expected to leave `Language` too — the
resolver moves to the driver ([§dd-dr:input-attachment]), collapsing the surface
toward the constructor alone.)*

*(Amended — API-review T1/T2 session, application details ruled: `Default for
Language<L>` is **removed** (it reintroduces the implicit seed by the back door, and
the turbofish spelling was itself walkthrough friction), as is
`LatexlikeDriver::default()` (strict-vs-tolerant is the driver's one policy knob —
it must be explicit; `StdParseDriver::default()` stays pending the language-designer
session). The packages argument takes the sealed `IntoSpecsProvider` conversion —
`lang_initial_with_packages([minidefs::minilatex_package(), my_pkg])`, no Arc noise
([§dd-dr:registration-ergonomics]).)*

*(Amended — API-review T3 session: `StdParseDriver::default()` is **removed** too —
after the `Default for Language` removal no `L::Driver: Default` consumer remains,
and `recovery` is the driver's only field, so a `Default` existed solely to hide
the one policy knob. The spelling is `StdParseDriver::new(Recovery::Strict)`.)*

*(Amended — API-review T4 session, collapse complete: `with_resolver`,
`resolver()`, and `Language::resolve_source` leave with the resolver's move to the
driver ([§dd-dr:input-wiring]) — the surface is `new(driver, initial_state)` +
`parse` + `parse_source` + accessors.)*

---

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

Status: DECIDED (user, Action-04 review; refines the original one-line CLAUDE.md constraint).

Four rules:

1. Panics are allowed only for **verifiably unreachable** code — impossibility guaranteed by
   this crate's own structure (a bounds check in the same function; a private constructor
   that always establishes the invariant), *independent of anything outer layers do*:
   problematic user input, a buggy `Lang` hook in a preset, or a misbehaving custom
   argument/construct parser must never panic a core routine. Written as
   `unreachable!`/`expect` with the invariant stated in the message.
2. The violation of a documented input contract is **not by itself** a reason to panic — it
   returns an `Err` (translatable to, e.g., a Python exception by a wrapper).
3. Individual indexing-style exceptions require explicit user approval. Approved:
   `NodeTree::node`/`nodes_in`, `Span::slice`, `TextContent::resolve`, and
   `ChildRegion`'s resolved-only accessors keep their documented panics **with
   non-panicking companions** (`NodeTree::get`, `Span::get`) — the std `Index`-vs-`get`
   convention: the panicking form for ids/spans the caller minted from this very
   tree/source, the `Option` form for values of unknown provenance.
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
- `skip_whitespace` returns `pos` unchanged on an invalid `pos` (debug-asserted);
  `Span::len` saturates on inverted spans, `is_empty` consistent with it;
  `ParserSession::finish` returns `Result<ParseResult, NodeBuildError>`.
- `check_tree_invariants` is exempt: a documented test utility whose *purpose* is
  asserting — panicking is its API. `debug_assert!` remains fine for crate-internal
  invariants but is not a substitute for boundary validation of outer-layer input.

Rationale: an invariant assertion that can only fire on a core bug is better loud than
silently wrong; but a panic reachable through an extension author's mistake turns their bug
into a crash of the host application — an error naming the violated contract is strictly
more useful, in every build profile.
Rejected alternatives: sanctioning the builder's panic-on-caller-bug policy (the Action-04 report's
original recommendation) — it violates rule 2's "outer layers must not panic the core";
`Option`-returning tree accessors everywhere — clutters every legitimate traversal for a
misuse the `get` companions already cover.
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
Concrete shape: `TokenError<'s>` = structured `TokenErrorKind` (closed enum:
end-of-stream-after-escape, forbidden-char — replaces pylatexenc's stringly
`error_type_info`) + byte `Span` + `Option<TokenRecovery<'s>>`, where `TokenRecovery` =
placeholder token + an explicit `resume_pos` (explicit rather than derived from the
token: a custom source's placeholder need not end where reading resumes, and the explicit
position carries the advancement contract; the built-in recoveries all resume at their
placeholder's span end — the dangling-escape placeholder is a `Char(escape_char)`
covering the escape byte). Token-level errors carry plain `Span`s, not `SourceSpan`s —
they are transient like tokens; the session converts whatever it reports into Arc-span
`Diagnostic`s. The reader itself is policy-free: it always returns `Err` with the
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
   (the reader is already repositioned via `resume_pos`); each parse-level condition
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

#### `resume_pos` must advance the reader; violations abort even in tolerant mode [§dd-dr:resume-pos-contract]

Status: DECIDED (user, code-review follow-up).

The content loop's recovery arm
is the one arm that consumes no token, so its termination rests entirely on `resume_pos`
repositioning the reader strictly past the failed read's start. Both in-crate producers
satisfy this, but the contract is reachable by third-party code through two public
extension points (a custom `TokenReader::peek`, a `Lang::scan_specials` returning a
`TokenRecovery`), and a violating `resume_pos` was demonstrated to hang `NodesParser` in
release builds while growing the diagnostics sink unboundedly. The contract is now stated
on `TokenRecovery::resume_pos` and enforced at the adoption site (`nodes_parser.rs`
content loop): if the reader did not advance after the positional move (`move_to_pos`;
[§dd-dr:token-contract-hardening], item 4), the parse aborts with the
token error as a `ParseError` — *even in tolerant mode*, whose promise is a best-effort
tree, not tolerance of non-termination; an abort is the doctrine-blessed failure mode
(no panic, rule 3 above). The guard lives at the adoption site and not inside the move
because `move_to_pos` is deliberately bidirectional (it is also the absent-argument and
environment-name rewind), so it can assert nothing about direction.
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
Rejected alternatives: promoting recurring conditions to enum variants (the original Action-01
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
`Clone` derive), the identifier is a compile-time constant, and sealing enforces the
const-identifier discipline. Downcasting targets the data struct itself — one type, one
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
hand-chosen, namespaced `<layer-or-preset>.<area>.<condition>` (provisional scheme:
`core.token.*`, `core.nodes_parser.*`, … for library conditions, areas mirroring
today's modules; `<preset-name>.<namespaced-name>` for presets and downstream languages),
exposed as `pub const IDENTIFIER` so consumers compare against the const rather than a
literal. Identifiers and serialization field names are semver-stable API surface: although
the provisional scheme mirrors today's module areas, the strings are frozen independently of
future code moves.
Rationale: no compiler mechanism yields a stable wire identity — `type_name` has an
explicitly unstable format and encodes module paths (a refactor must not break a user's
linter config), and `TypeId` differs per build and is not serializable. Wire naming is
convention-based in every ecosystem (rustc lints, ESLint rules, LSP codes); what convention
*can* get is hardening: single-definition consts and a documented namespace rule.
Rejected alternatives: deriving the identifier from the type name (the two have different change
cadences — a struct rename is an internal refactor, a wire-id change is a silent break; the
derive macro will *require* the id attribute); method name `diagnostic_identifier()`
(stutters as `DiagnosticData::diagnostic_identifier`; the trait context already qualifies,
[§dd-dr:naming]); a per-`Lang` `diagnostic_catalog()` with a uniqueness test (maintenance work to keep in
sync, and namespace prefixes already prevent collisions — can be
added later without breakage).

*(Amended — API-review P5: the provisional module-mirroring areas are to be replaced by
concept-named areas before the stability freeze; the full stability semantics
(identifier hard-stable, data keys additive, wording excluded) and the
defining-vocabulary ownership rule are recorded in [§dd-dr:wire-identifier-stability].)*

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

#### `Lang::refine_diagnostic` hook [§dd-dr:refine-diagnostic-hook]

Status: DECIDED (user + design sessions).

`fn refine_diagnostic(Box<dyn DiagnosticData>, &ParsingState<L>) -> Box<dyn DiagnosticData>`,
default identity, applied exactly once in the recover funnel (at the `ParseContext` level,
where the state is in scope). A `Lang` can replace a generic condition with its own — FLM
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
Rationale: `scan_specials` participates in the recovery protocol but could only lie with
tokenizer-internal kinds; one extension mechanism (`DiagnosticData`) serves both layers,
while the token layer keeps a concrete matchable enum for the recovery protocol.

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
`Sync`). `format_position` stays as the documented one-shot convenience.
Rejected alternatives: an unbounded default (the failure mode is silent and input-controlled), and
a public `DiagnosticRenderer` type (no second consumer yet; `render_all` covers the
need — promote the cache if one appears).

#### Wire identifiers: stable namespace, concept-named areas, owner = defining vocabulary [§dd-dr:wire-identifier-stability]

Status: DECIDED (user, API-review policy session P5; the concrete area-rename slate is
pending in the API-review T4 decision session).

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

1. **The `<area>` segment names a construct concept or subsystem** (`token`, `scopes`,
   `environments`, `arguments`, …) — never a file, module, or type name. This repairs
   friction F9: most current `core.*` identifiers use internal *file names* as areas
   (`core.nodes_parser.*`, `core.argument_parsers.*`, …), contradicting the decoupling
   promise documented on `IDENTIFIER` itself, and both API-review personas who guessed
   identifiers guessed concept names and lost a cycle. The rename slate is decided in
   the T4 session (the `nodes_parser` conditions interact with the deferred
   resolution-family extraction, [§dd-dr:public-namespace-topology]) and lands with the
   API-review application, before guides print any identifier.
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

Revisit if: the soft-freeze condition of [§dd-dr:stability-rubric] arises; or a
downstream language needs to re-namespace an inherited condition (that would need a
deliberate identifier-mapping design, not an ad-hoc exception).

*(Amended — API-review T4 session, THE SLATE RULED (frozen; lands in Phase 3
before guides print). Area `specs` absorbs command resolution AND the former
`scopes` area (user: "resolution of what?" — also disambiguates against *source*
resolution, `core.sources.*`; the wire vocabulary now tracks the public
`core::specs` home from [§dd-dr:resolution-extraction]; supersedes this entry's
illustrative `scopes` example). Renames:
`core.specs.{unresolvable-command, command-resolution-failed,
callable-defined-as-error, scope-op-failed}`;
`core.groups.{unclosed-group, stray-group-close}`;
`core.environments.{terminator-mismatch, malformed-terminator,
missing-terminator}`;
`core.arguments.{missing-mandatory-argument, expected-expression-argument,
expression-callable-requires-content, repeated-tack-on-field}` (the last segment
renamed from `repeated-field` — too vague outside its own area);
`core.recovery.unusable-recovery-token`;
`core.verbatim.{unterminated-verbatim, expected-verbatim-delimiter}`.
Keeps: `core.token.end-of-stream-after-escape`, `core.token.forbidden-char`,
`core.constructs.implementation-error`, `latexlike.environments.*` ×3. New:
`core.sources.{no-resolver, unresolvable-reference}` ([§dd-dr:input-wiring]).
Reserved: `core.specs.provider-commands-shadowed-by-escape` (the parse-init
warning; wording at application). The preset→core re-homing rider was verified
empty. Segment policy: keep segments unchanged (self-descriptive when quoted
alone). The guide table prints exactly these.)*

#### `Diagnostics::sorted_by_position()` — narrow, source-major [§dd-dr:diagnostics-position-sort]

Status: DECIDED (user, API-review T1/T2 session).

Diagnostics arrive in recovery order, not source order; `sorted_by_position()`
(returning-adjective form) sorts by (source in first-appearance order, span start),
documented as source order *within each source*. Narrow by design: a total "position
order" is ill-defined across multi-source parse trees, which are first-class
([§dd-dr:input-attachment]). Both `IntoIterator` impls already exist — the
walkthrough claim to the contrary was a doc gap, not an API gap.

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
  math in an inline-*form* group; [§dd-dr:math-group-form]).
- `InitialStateDataProvider` / `StateTransitionFinalizer` / `SpecialsProvider` /
  `NodeFinalizer` (a facet-decomposed `Lang`), `Latexlike<X: LatexlikeExt>` (the
  plugin-slot preset) — weighed and rejected during preset generalization
  ([§dd-dr:latexlike-generalization]).
- `finalize_node` (and the interim names `populate_ext`/`populate_node_ext`) — the
  parse-once minting hook is `make_node_ext`; the tier-2 per-kind ext family
  (`CharsNodeExt`…`ListNodeExt`) and the `NodeDataExt` parallel bundle — removed
  outright; `NodeTreeBuilder::for_parsing()` — the rejected hook-firing builder
  mode ([§dd-dr:ext-minting]).
- `ProcessedNodeData` — the annotation parameter's working name (collides with
  `NodeData` in the same scope; the vocabulary is *annotations*,
  [§dd-dr:node-annotations]); `tree_identifier` — the tag term is `tree_tag`
  ([§dd-dr:tree-tags]).
- `WithTransformedTreeNodeProvenance`/`WithOriginalNode` (the rejected
  auto-provenance trait), `add_subtree`/`copy_subtree` and "copy" as transform
  vocabulary — restaging ([§dd-dr:restage]); node-level cross-tree tracking says
  *original node* — never "provenance"/"origin", which belong to the source model
  (`SourceProvenance`/`SourceOrigin`).
- From the API-review T1/T2 session: `"base"` and `base_package()` — the seed
  package is `"_builtin"`/`builtin_package()` ([§dd-dr:base-package] amendment);
  minidefs fn name `package()` — it is `minilatex_package()`; `NodeKind::label()`/
  `kind_as_string()` — the accessor is `as_str()` ([§dd-dr:display-tree]);
  argument-code names `GroupOnly`/`StrictGroup` — the code is `BracedOnly`;
  `with_body_provider` (rejected abandoned-at-first-need sugar),
  `text_mode_argument()`/`text_argument_state_delta()` (text restore is an event,
  not a factory; [§dd-dr:argument-factory-additions]); as *shapes*: per-`GroupRule`
  mode visibility and a `ParsingState` parent pointer
  ([§dd-dr:enclosing-state-stack]).
- From the API-review T3 session: `SimpleLang` — renamed `TrivialLang` ("Simple"
  over-promised an on-ramp; [§dd-dr:trivial-lang]);
  `CommandResolution::resolve_via_scopes` (the associated-fn spelling, and the
  interim `resolve_command_via_scopes`) — the extracted resolver is
  `resolve_command_in_scopes` ([§dd-dr:resolution-extraction]);
  `restore_text_context_delta` — the pillar is `exit_math_context_delta`
  ([§dd-dr:enclosing-state-stack] amendment); role-accessor spellings
  `r#macro()`/`macro_()`/`macro_kind()`/`macro_type()` — the family is
  `macro_callable()`/`environment_callable()`/`specials_callable()`;
  `text_mode()`/`is_text()` — trimmed from the mode role trait
  ([§dd-dr:latexlike-generalization] amendment); constructor names `neutral()`/
  `disabled()` — the empty starting values are `TokenRules::empty()`/
  `StateData::empty()`; `all_off()`/`raw()` — the gate-off overrides value is
  `disable_all()` ([§dd-dr:on-ramp-defaults], [§dd-dr:takeover-staging-sugar]);
  `new_anonymous` — the unnamed-constructor spelling is `new_unnamed`; the
  `ArgumentSpec::named()` builder and `ParsedSlot::named()` constructor — names
  move into `new(…, name)` ([§dd-dr:named-first-constructors]);
  `ScopeResolvingDriver`/`ScopesDriver`/`StdScopeDriver` — the component is
  `ScopesResolvingDriver` ([§dd-dr:scopes-resolving-driver]).
- From the API-review T4 session: `techy::helpers` (a recipes module — the `util`
  problem under another name; placement stays by logical function);
  `resolution` as a wire-identifier area (the area is `specs` — "resolution of
  what?") and the file-named areas `nodes_parser`/`environment_parser`/
  `argument_parsers`/`verbatim_parser`/`group_parser`/`tack_on_parser` (the
  applied slate; [§dd-dr:wire-identifier-stability] amendment);
  `ancestors()`/`Ancestors` (rejected — `parent()` + `iter::successors`;
  [§dd-dr:tree-navigation] amendment); `Descendants::with_depth()` (patched flat
  iteration's structure loss at the wrong layer — the read walker belongs to the
  recompose session; [§dd-dr:recompose] amendment); `NodeRef::line_col()`/
  `SourceSpan::line_col()` and `LineIndex::line_range(line_no)`
  (rejected/skipped — [§dd-dr:line-col-ownership]); `LineIndexCacheProvider` —
  the seam is `LineColProvider` (provides answers, not caches);
  `cx.parse_source` as the sub-parse door name (collides with
  `Language::parse_source` under a different contract — the door is
  `parse_attached_source`; [§dd-dr:input-wiring]).
- From the API-review T5 session: `stage_argument_like` — the content-replacement
  helper is `restage_argument_with_content` (+ the `_slot_` twin;
  [§dd-dr:restage-ops]); `Restage::Continue`/`Keep`/`Retain`/`Auto` — the
  variant is `Descend` ([§dd-dr:restage] amendment); `StateStackView`/
  `StateStack` — the owning type is `ParsingStateStack`
  ([§dd-dr:enclosing-state-stack] amendment); `Split` — the split result type is
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
  strategy name (no named span strategy exists; [§dd-dr:recompose] amendment);
  the canonical-`"\n\n"` paragraph-break `name` — superseded by name-as-written +
  spec-identity identification; `CallableNodeInvocationSyntax` — the payload
  type is `InvocationSyntax`; `new_for_invocation` — the constructor
  trait/method is `FromInvocation`/`from_invocation`; `Bit`/`ComposeBit` — the
  piece vocabulary is `Piece`/`ComposePiece` (`Fragment`/`Part` recorded
  considered; `Output` rejected — collides with `ConstructParser::Output`);
  `ConcatSpec` ("Spec" is author-side vocabulary) and the interim `ConcatParts`
  — the instruction payload is `ConcatPieces`; `VisitCx`/`RecomposeCx` —
  spelled-out `VisitContext`/`RecomposeContext` (the `ParseContext` convention);
  `walk_tree`/`recompose_tree` — rejected on one-canonical-path (`visit::walk`,
  `recompose::recompose`; [§dd-dr:visit-engine],
  [§dd-dr:recompose-machinery]).

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

Status: DECIDED (user-led, API-review policy session; application pending — the
restructure is scheduled within the API review, after the resolver-extraction design
below).

The public API is exported exclusively through **re-export facades** — internal src
modules become private, so internal file organization is permanently invisible to
public paths — with **exactly one canonical path per item**, chosen by *logical
function/use* (never by frequency of use, never mirroring internal layout). Layout:

- `techy::source`, `techy::error` — the S0 data models, top-level.
- `techy::extract` — consumer tool-library over node trees, top-level; future
  `techy::transform` (tree-transformation infrastructure) joins it as a sibling.
  The top level is thus *role-based*: data models and consumer tool libraries up top,
  machinery in `core`, preset in `latexlike`.
- `techy::core` — flat hub holding the mutually-recursive heart: `Lang`/state, token
  machinery, engine (entry, result, sessions, drivers, command resolution).
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
the planned transformation surface, which consumes the read side and produces through
the builder in one API).

**The specs/hub boundary rule (user-endorsed): `specs` is author-side — what you write
to define callables and organize definitions; the hub is run-side — state, tokens,
engine, resolution.** Known judgment calls at that interface (`FrameRole`,
`SearchedProviders`, `CallableQuery`) and the resolution family
(`CommandResolution`/`ResolvedCallable`): their current ambiguity is read as a symptom
of wiring, not taxonomy — the standard command-resolution-via-scopes is to be extracted
into a single standalone function that `Lang`s opt into (expected home: `specs`), after
which the ambiguous items rest naturally beside that resolver. Their final placement
waits for that design. Likewise deferred: `ArgumentParser` trait in `specs` vs
`constructs` (a case exists for `constructs`, beside `ConstructParser`). Cross-boundary
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

Revisit if: a future public item genuinely belongs to two groups; the hub grows
uncomfortably large (extracting a further subset is breaking — weigh before the first
external dependent); or the crate is split (S0/topic modules convert to crate
re-exports losslessly — the facade model is what makes that lossless).

---

*(Amended — API-review P4: `techy::recompose` (recomposition, [§dd-dr:recompose])
joins `techy::transform` ([§dd-dr:restage]) in the role-based top level.)*

*(Amended — API-review T3 session: both deferred placements are ruled — the
resolution family moves to `core::specs` beside the extracted
`resolve_command_in_scopes` ([§dd-dr:resolution-extraction]), and the
`ArgumentParser` trait goes to `core::constructs`, beside `ConstructParser` and
the shipped argument-parser implementations (a parsing contract; `ArgumentSpec`'s
`Arc<dyn ArgumentParser>` is an accepted cross-boundary signature reference). The
topology is fully specified; the Phase 3 application is unblocked.)*

---

#### API stability rubric: one stability class, soft freeze until framework adoption [§dd-dr:stability-rubric]

Status: DECIDED (user, API-review policy session P5).

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

Rejected alternatives: an unstable/experimental tier (an escape hatch that invites
exactly the future restructuring the review exists to prevent, and a dual-status
ambiguity against the one-canonical-path principle); a hard freeze at the end of the
review (prematurely enshrines flaws found between review and adoption, for no
dependent's benefit); stability tiers A/B distinguished by surface (they carried the
same semver discipline anyway — the distinction collapsed once tiering moved into
module placement).

Revisit if: a framework starts depending on techy in earnest — from that moment the
freeze is hard and breaking changes need migration paths and dependent coordination.

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
vs. display is neither a class nor a mode. Display-ness is a delimiter fact, read off the
node's recorded delimiters by the preset sugar `NodeRef::math_style()` →
`MathStyle::{Inline, Display}` (pylatexenc parity: `LatexMathNode.displaytype` is likewise
delimiter-derived).
Rationale: the class taxonomy cuts at parse-behavior joints, and inline and display math
parse identically — same interior `Mode::Math`, same definition visibility — so a split
would do no parse-time work; it would also break the class/mode symmetry (three classes
over two modes).
Rejected alternatives: the plan sketch's `MathInline`/`MathDisplay` split (typed display-ness that a
rule author declares — its one real advantage: embedder-registered math delimiters would
classify themselves, where `math_style()`'s table answers `None`); a `Bracket` class and
`[]` in the default rules — `[`/`]` are plain characters in LaTeX outside
optional-argument positions (`a [b] c` is text), and `OptionalGroupArgumentParser`
recognizes them through its own per-spec `temporary_groups` rule, so neither the class nor
the base rule has a consumer (user-caught; the original plan listed both).
Revisit if: a consumer needs typed display-ness on custom math delimiters (the split
stays open under `#[non_exhaustive]`).
*(Amended — the revisit condition fired during the API review (typed display-ness for
custom/dynamic math delimiters; T5/FLM + preset generalization): display-ness is now
typed **class payload**, `Math(MathGroupForm)`, declared by the rule author, with
parse wiring still single-armed; the `MathStyle` delimiter-table sugar is superseded.
See [§dd-dr:math-group-form].)*

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

#### The seed ships a `"base"` package: pylatexenc's default specials as data [§dd-dr:base-package]

Status: DECIDED (user; package name user-chosen).

`Latexlike::initial_state_data()` seeds the scope stack with one package `"base"` holding
zero-argument specials for `&`, `~`, ``` `` ```, `''`, `--`, `---`, `` !` ``, `` ?` `` —
pylatexenc's default context (its *latex-base* + *nonascii-specials* categories). Droppable
wholesale by name (`ScopeOp::Unload`), shadowable per-trigger by pushing a provider.
Macro/environment definitions deliberately stay out until the std-DB port. The typography
ligatures (``` `` ```, `''`, `--`, `---`, `` !` ``, `` ?` ``) are registered **text-mode
only** (they carry no math meaning — inside `$…$` they stay plain chars); `&` and `~` are
visible in every mode (the per-entry mode gate, [§dd-dr:mode-visibility]).
`\begin`/`\end` stay all-modes so math environments still open in math.
Rationale: out-of-the-box parity with pylatexenc's default node shapes for these
triggers — with one deliberate exception: the `\n\n` paragraph-break special of
pylatexenc's *latex-paragraph* category is omitted, so a multi-newline break is a
whitespace chars node here (`enable_multi_newline_paragraphs`), not a specials node. The
multi-character ligatures exercise the longest-match fold (`---` beats `--`) in real
defaults rather than only in tests.
Rejected alternatives: an empty seed stack (purest, but `~`/`&` would parse as plain chars out of the
box — silent divergence from pylatexenc); seeding only `&`/`~` (leaves the fold's only
real-data consumer test-side).

*(Amended — API-review T1/T2 session: the seed package is renamed **`"_builtin"`**
and slimmed to what any latexlike parse must preload — the `\begin`/`\end` dispatch.
`&` is removed from the preset's specials entirely; `~` and the ligatures move to
`minidefs`'s `"minilatex"` package (same specs and mode visibilities). A base-only
parse thus emits these triggers as plain chars — the deliberate positioning
correction: typography interpretation is definitions content, not parsing substrate;
pylatexenc default-shape parity for these triggers now requires loading minilatex.
The fn follows the rename: `base_package()` → `builtin_package()`. The
pylatexenc-parity rationale above is superseded to this extent.)*

#### Per-definition mode visibility on `Package` — the fine gate under `set_visible_modes` [§dd-dr:mode-visibility]

Status: DECIDED (user).

`Package::insert_in_modes`/`insert_specials_in_modes` attach an optional mode list to a
*single* definition; `retrieve_spec`/`scan_specials` check it against `ParsingState::mode`
under the pre-existing package-level `set_visible_modes` — **both** gates must admit the
mode (`None` = every mode the package is visible in). One loadable, unloadable package can
then hold text-only ligatures and (later) math-only `^`/`_` scripts together.
Rationale: the base package must keep `\begin`/`\end` visible in math while hiding the
text ligatures there — package-level visibility alone cannot express that without splitting
`"base"` into several names, which would break the single-name `Unload("base")` contract
and the specials-as-one-category model. Per-entry visibility is the minimal mechanism that
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

The accessors (`is_math_group`, `math_style`, `macro_name`, `environment_name`,
`specials_name`) are inherent methods on `NodeRef<'_, Latexlike>`, written in the preset
module — legal because the preset shares the crate with `node`.
Rationale: zero-import ergonomics on the majority path; an out-of-crate language (FLM)
must use an extension trait regardless, and that pattern needs no in-tree demonstration.
Rejected alternatives: a `LatexNodeRefExt` trait for the preset (a `use` tax on every consumer, buying
only symmetry with a constraint the preset does not have).

#### `\begin`/`\end` dispatch is scope-stack data: ordinary `Macro` entries of `"base"` [§dd-dr:begin-end-dispatch]

Status: DECIDED (user).

`BeginSpec` (the environment composition) and `EndSpec` (orphan-`\end` diagnostics) are
registered under `begin`/`end` in the seed package like any definition — resolvable
through the unchanged `LatexlikeDriver::resolve_command`, shadowable, and unloadable
(`Unload("base")` removes environments along with the specials; pinned in a test).
Consequence: the `Invocation` arrives typed `Macro`, so the composition stamps
`CallableType::Environment` (and the environment's own name and spec) on the staged
node itself — the dispatcher's identity appears nowhere in the tree.
Rationale: the phase's direction is "everything through the stack" (even specials are
data); a hardcoded `resolve_command` arm would be the one un-shadowable definition in
the language.
Rejected alternatives: the test-lang rehearsal's driver arm (`if name == "begin"`), which made
`\begin` structural syntax.

#### `EnvironmentSpec` wraps a dyn `EnvironmentBehavior`; `with_body_delta` adapts [§dd-dr:environment-spec-surface]

Status: DECIDED (user; executes the [§dd-dr:spec-downcasting] funnel and
[§dd-dr:begin-composition]'s defaulted `make_body_parser()`).

The concrete wrapper `EnvironmentSpec` is the registration/downcast target (implements
`CallableSpec` by delegation, titles frames "environment ‘align’"); the inner trait
carries the behavior as defaulted methods — `arguments()`, `body_state_delta(…)`
(owned return: behaviors may compute it), `make_body_parser(…)` (default: the core
`EnvironmentBodyParser` through the rigid `\end{name}` terminator). Hooks receive an
`EnvironmentInvocation` facts struct (`trigger_span`, `name`, `name_span`) —
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
"specials ‘~’"); `base_package()`'s specials switched to a shared `SpecialsSpec`.
Generic specs remain first-class everywhere.
Rationale: functions returning `StdCallableSpec` would leave tracebacks saying
"callable ‘…’" — the vocabulary hook exists precisely for presets — and concrete preset
types are stable downcast targets for later `finalize_node` work.

#### Orphan-`\end` recovery: dispatch-time diagnosis, chars over the consumed extent [§dd-dr:orphan-end-recovery]

Status: DECIDED (user).

Inside a body, `\end` is the stop condition and never reaches resolution, so a
*dispatched* `\end` is always an orphan: `EndSpec`'s parser reads the rigid name group
when present, records `OrphanEnd` (message quoting `\end{name}` when the name parsed),
and tolerantly stages the consumed extent as one `Chars` node — `\end{name}` whole, so
`{name}` is not re-parsed as a stray group. Preset condition ids are namespaced
**`latexlike.environments.*`** (`malformed-begin`, `unknown-environment`, `orphan-end`;
user-chosen over `latexlike.begin.*`/`latexlike.end.*`). Implementation fact worth
remembering: the tolerant chars fallbacks (malformed `\begin`, nameless orphan `\end`)
must cover the trigger's syntactic *post-space* too — the token span includes it, and
trimming it would break the sibling partition invariant; the earlier rehearsal had the same
shape. The body-unwind path that leaves a stray `}` for the root recovers cleanly: the
root stages the consumed delimiter as a `Chars` node (cf. [§dd-dr:language-parse-api],
second follow-up).

#### The verbatim family: recipe → production parsers, group+chars shapes [§dd-dr:verbatim-family]

Status: DECIDED (user; parity item N7).

`constructs::verbatim_parser` promotes the pinned recipe ([§dd-dr:token-contract-hardening], item 5; the
test-side `RawBlockParser`): `verbatim_state_delta(rule)` is the recipe as data (all six
feature gates off + `expecting_group_close` **replaced**), and the two production
parsers drive it — `VerbatimArgumentParser` (delimited `\verb|…|`; `ArgumentParser`,
the `v` codes) and `VerbatimBodyParser` (environment contents up to a **literal**
terminator string; produces `EnvironmentBody`, pluggable via `make_body_parser`).
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
  parity), and verbatim does not nest. The preset's `VerbatimBehavior` composes the
  terminator as `\` + `end{name}` — the preset's canonical spellings, same doctrine as
  `BEGIN_COMMAND_NAME`; a language re-ruling the escape char must supply its own
  behavior.
- *A tolerated unreadable token* (forbidden char) inside a committed verbatim region
  ends it like EOF (diagnosed unterminated/missing-terminator); the enclosing loop
  re-reads the error and applies its own token recovery — the probe protocol, two
  true diagnostics accepted. A reader yielding any *other* token kind under the recipe
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

Status: DECIDED (user; parity item N8).

A preset **function**, `&str` in → `Result<Vec<Arc<ArgumentSpec>>, ArgumentCodeError>`
out, eager, per the plan-session shape. The single code string concatenates codes
(pylatexenc's list form is not mirrored), so the grammar is pinned: optional whitespace
*between* codes; parameters follow their code immediately and may not be whitespace;
**`v` takes two delimiter characters exactly when a non-whitespace character follows
directly** — a bare auto-`v` stands last or before whitespace (`"v {"`), and `"v{"` is
a loud `TruncatedCode`, never a silent misparse. Per-code resolution as landed:

- `m`/`{` → `GroupArgumentParser::new(Content)` — *refining the survey table's
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
`e{…}` [N3] and `AnyDelimited` [N2] stay deferred with their parsers.
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
`enable_multi_newline_paragraphs` gate (verbatim's features-off state uses it).

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

*(Amended — API-review T1/T2 session, application ruling: one file
`latexlike/minidefs.rs`; a single public item **`minilatex_package()`** — named for
the package, not a generic `package()`, keeping room for future mini-siblings;
target signature `LLL`-generic per [§dd-dr:latexlike-generalization] — returning a
bare `Package`; activation always explicit. Specs: `\emph`/`\textbf`/`\textit` =
`MacroSpec` `"m"` (fallback on); `itemize`/`enumerate` = `EnvironmentSpec` with a
body delta pushing the inner `"minilatex.item"` package defining `\item` (`"o"`) —
the body-scoped exemplar. Per the [§dd-dr:base-package] amendment, minilatex also
carries `~` and the text-mode ligatures.)*

#### Argument-code and factory additions: `BracedOnly`, named factory, text-restore event [§dd-dr:argument-factory-additions]

Status: DECIDED (user, API-review T1/T2 session).

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
3. **Text-mode arguments are an event, not a factory.** The `\text{…}` recipe
   becomes an `ArgumentSpec` state delta carrying a preset restore event — 
   composable with every argument shape, optional included. The old guide recipe is
   repaired: it statically reset `forbidden_chars` and `groups`, clobbering embedder
   customizations. Restore semantics — nearest enclosing text-mode state (else the
   outermost), whole `TokenRules` — and the public pillar functions:
   [§dd-dr:enclosing-state-stack].

Rejected alternatives: a canned `text_mode_argument()` factory (composes with
nothing — a text-mode *optional* argument would need a second factory; codifies the
buggy recipe); a `text_argument_state_delta()` helper (barely shorter than the delta
it wraps; one more permanently-stable name); code names `GroupOnly`/`StrictGroup`; a
single-char code (near-invisible next to `m`); reusing xparse's `g` (means a
deprecated *optional* brace group — actively misleading).

Revisit if: compact-string parity for `BracedOnly` is demanded by real spec tables.

*(Amended — API-review T3 session: item 3's restore semantics are re-specified —
the event's pillar is `exit_math_context_delta`, restoring the first *non-math*
enclosing context rather than seeking a text-mode state;
[§dd-dr:enclosing-state-stack] amendment.)*

#### The latexlike preset generalizes over a `Lang` family: role traits + `LatexlikeLang` [§dd-dr:latexlike-generalization]

Status: DECIDED (user, API-review policy session P3 — direction and shape; detailed
design and application in the 2b T3/T5 sessions).

Every latexlike preset component — `LatexlikeDriver`, `MacroSpec`/`SpecialsSpec`, the
environments machinery (`EnvironmentSpec`/`BeginSpec`/`EndSpec`/`EnvironmentBehavior`/
`VerbatimBehavior`), `argument_specs`, `default_token_rules`, `base_package`,
`minidefs`, the `NodeRef` sugar — becomes generic over a preset `Lang` family
(conventional parameter `LLL`), erasing T5's **preset-fork cliff** (a language needing
its own node exts/state/modes had to implement `Lang` and thereby forfeited every
preset component; the ext system served only full forks). The audit finding that
carried the shape: the preset's `Latexlike`-coupling is almost entirely *vocabulary
threading* — only two genuine LaTeX facts live in logic (the `$` forbidden-char merge,
the math-delimiter table). Mechanism, three layers:

- **Per-vocabulary role traits**, implemented by the vocabulary types themselves
  (method-based): `LatexlikeGroupType` (`content_group()`, `math_group(form)`,
  `verbatim_group()`, classifier `math_form()`, predicate `is_math()` —
  [§dd-dr:math-group-form]), `LatexlikeCallableType` (macro/environment/specials
  roles), `LatexlikeMode` (text/math roles). techy implements all three for its own
  `GroupType`/`CallableType`/`Mode`, so a language adopting the preset enums as its
  associated types satisfies the bounds with zero code; a language with extended
  vocabularies implements them itself, which *guarantees* the preset-required values
  exist while leaving the enum open for its own additions.
- **`LatexlikeLang`**, the umbrella: `trait LatexlikeLang: Lang<GroupTypeId:
  LatexlikeGroupType, CallableTypeId: LatexlikeCallableType, ModeId: LatexlikeMode>`,
  carrying **defaulted behavior methods** for language-level statics (e.g. the
  math-interior adjustment generalizing the `$` merge — the default must derive the
  forbidden set from the math-class rules being removed, never restate a literal
  `'$'`; the math-delimiter data behind `default_token_rules`). Deliberately **no
  blanket impl** (it would make the defaults un-overridable by coherence); opting in
  is `impl LatexlikeLang for Flm {}`. Evolution posture (feeds the P5 rubric): the
  initial required surface freezes at stabilization; future roles/behaviors arrive as
  defaulted methods delegating to existing ones (non-breaking); a fallback-less new
  role is a conscious breaking change.
- **`Lang` stays whole; pillar functions are the composition mechanism.** The preset
  ships every `Lang`-hook behavior as a public `LLL`-generic function
  (`latexlike::initial_state_data`, the `finalize_node` spec-dispatch,
  `default_token_rules`, `base_package`), and a framework's `Lang` impl delegates in
  one line per hook, augmenting freely (`finalize_node`: preset dispatch, then own ext
  attachment). The residue (~30 lines: associated types + one-line bodies) is
  irreducible by the strata rule — S1 never names a preset ([§dd-dr:three-strata]), so
  preset behavior can only enter core-called hooks through the framework's own bridge
  code; no trait topology removes it.

The preset keeps `NodeExts = ()`: the whole ext budget belongs to the framework built
on top; preset semantics encode in the *vocabulary* (role traits), never in the ext
system.

Rejected alternatives:

- **Extraction-only lifting** (free-function cores, types stay monomorphic) — the
  cliff is mostly *types* (spec types, environments machinery); functions cannot lift
  trait impls.
- **Plugin-slot preset** (`Latexlike<X: LatexlikeExt = ()>`) — pure sugar once the
  role traits exist, walls off vocabulary extension, and adds a second way to be a
  latexlike-family language against the one-canonical-path ruling
  ([§dd-dr:public-namespace-topology]). Reconsider at the 2b FLM probe only if the
  `Lang`-impl residue proves heavy.
- **Decomposing `Lang` into facet traits** (`LangTypes` + `InitialStateDataProvider` +
  `StateTransitionFinalizer` + `SpecialsProvider` + `NodeFinalizer`), in all three
  Rust realizations: the *supertrait* reading delivers nothing (a subtrait cannot
  default a supertrait's methods and the orphan rule blocks preset-side impls, so
  `impl SpecialsProvider for MyLang {}` reaches only the core-neutral defaults the
  whole `Lang` already gives); *marker-gated blankets* (`impl<T: UseLatexlikeSpecials>
  SpecialsProvider for T`) have exactly one blanket slot per facet trait crate-wide
  (competing with the `SimpleLang` quick-start blanket), are wholesale-only with a
  coherence cliff at the first customization, and cannot be replicated by downstream
  frameworks for *their* extenders (orphan rule, uncovered type parameter); *strategy
  associated types* (`type Specials: SpecialsProvider<Self>` naming preset ZSTs)
  genuinely plug in but split the coherence-coupled hook pairs across authors (seed ↔
  `finalize_transition`, scan ↔ trigger-chars — "both hooks have the same author" is
  the documented soundness argument), founder on unstable associated-type defaults
  (every non-`SimpleLang` language names 4–5 more types; the F10 on-ramp cliff
  steepens), and win nothing for the dominant preset-plus-own-additions mode
  (`finalize_node`), where a wrapper ZST wraps the same delegation body. Regret
  asymmetry: a framework can adopt the strategy pattern privately today with zero
  techy support, while un-decomposing a public `Lang` is breaking.
- **Role mapping via associated consts, `From<GroupType>` bounds, or equality bounds**
  (`L: Lang<GroupTypeId = GroupType, …>`) — consts and `From` cannot express
  payload-carrying roles (`math_group(form)`) nor defaulted-method evolution;
  equality bounds freeze hosts to the preset enums (that shortcut falls out of the
  role traits for free via techy's own impls).

Routed to 2b (T3/T5 unless noted): role-accessor naming incl. the `macro` keyword
wrinkle ([§dd-arch:naming] session); `ClosedVocabulary` as role-trait supertrait?;
`latexlike.*` wire identifiers emitted inside foreign-`Lang` parses (P5); generic
`LatexlikeDriver<LLL>` vs extracted driver-core helpers; generic
`minidefs::package::<LLL>()` (T1/T2). Acceptance test: re-run T5's FLM compile probe —
a custom `Lang` with node exts reusing driver, spec types, token rules, and base
package.

Revisit if: a real ecosystem of interchangeable facet implementations materializes
(strategy traits are then addable without breakage), or a required role with no
sensible default becomes unavoidable (accepted as a conscious breaking change).

*(Amended — API-review P4: "the preset keeps `NodeExts = ()`" is restated per-member —
the node and argument members stay `()`, while `SlotExt` is claimed by the preset for
trait-based body marking. [§dd-dr:slot-roles].)*

*(Amended — API-review T3 session, routed items ruled: role-accessor names are
**`macro_callable()` / `environment_callable()` / `specials_callable()`** with
predicates `is_macro`/`is_environment`/`is_specials` — the group trait's
role-plus-vocabulary-noun pattern, which dissolves the `macro` keyword problem as
a side effect (`r#macro`/`macro_`/`macro_kind`/`macro_type` rejected). The mode
role trait is **trimmed to `math_mode()` + `is_math()`** — no text-mode
constructor and no `is_text`: the only known text-mode-constructor consumer was
the restore-to-text pillar, re-specified as `exit_math_context_delta`
([§dd-dr:enclosing-state-stack] amendment). `ClosedVocabulary` is **not** a
role-trait supertrait — "provide, don't require" ([§dd-dr:iter-symbols]
amendment). The driver question is ruled in [§dd-dr:preset-driver-pillars].)*

*(Amended — API-review T5 session: (1) the role-trait roster gains a fourth
member, **`LatexlikeEvent`** — constructor + recognizer for the
exit-math-context event (`exit_math_context()` / `is_exit_math_context()`,
coherence contract mirroring `math_form`), bound on `LatexlikeLang::Event`.
Without it the generalization re-opens a cliff of exactly the kind it exists to
close: the E4 text-restore is an *event* the `LLL`-generic argument factory must
mint in the host's own `Event` type and the driver must recognize
([§dd-dr:enclosing-state-stack]); a preset-side event wrapper would violate
vocabulary-stays-the-host's-own, and an event-less design cannot exist (the
patch depends on the enclosing stack at use time — that context-dependence is
the E4 design). (2) The pillar list's "the `finalize_node` spec-dispatch" is
corrected: P4 deleted `finalize_node` ([§dd-dr:ext-minting]) — no such pillar
exists; the preset's `make_node_ext` is the trivial `()` mint, and its only
claimed ext is the `SlotExt` body marker ([§dd-dr:slot-roles]).)*

*(Amended — API-review recompose session: the role-trait roster gains a fifth
member, **`LatexlikeInvocationSyntax`** — implemented by the Lang's
invocation-syntax payload type: `type Env: EnvironmentSyntax<L>`, form
constructors (`macro_form`/`environment_form`/`specials_form`), accessors
(`macro_syntax`/`environment_syntax`/`is_specials`) — so the preset's staging
sites and `SourceRecomposer` work over any `LLL`; [§dd-dr:invocation-syntax].)*

#### `GroupType::Math(MathGroupForm)`: inline/display is typed class payload [§dd-dr:math-group-form]

Status: DECIDED (user, API-review policy session P3; supersedes the delimiter-fact
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

Status: DECIDED (user, API-review T3 session; detailing shared with the T5
session).

The component the generalization ruling left open resolves as **both, layered** —
the same shape [§dd-dr:latexlike-generalization] chose for `Lang` (whole type +
pillar composition): the driver's behavior ships as public `LLL`-generic **pillar
functions**, and **`LatexlikeDriver<LLL>`** is the canned assembly whose hook
bodies are precisely the one-line delegations (`PhantomData<LLL>`; manual impls
keep `Copy`/`Eq`). Pillar inventory: `math_group_interior_delta` (the math plug —
forbidden set derived from the removed math-class rules, never a restated `'$'`),
`exit_math_context_delta` ([§dd-dr:enclosing-state-stack] amendment),
`make_paragraph_break_node`; `resolve_command` composes
`resolve_command_in_scopes` ([§dd-dr:resolution-extraction]) with the macro role —
no separate pillar. Why both: structs cannot be partially overridden and subtraits
cannot re-default supertrait methods (the recorded facet-decomposition flaw), so
pillars are the only mechanism serving a framework wanting
preset-behavior-plus-one-custom-hook (FLM's documented `refine_diagnostic`
posture); pillars alone would make the plain-Latexlike consumer hand-write ~30
delegation lines for nothing. Not a dual path: component vs building blocks is the
`StdCallableSpec`-vs-`impl CallableSpec` relationship — the struct contains no
behavior the pillars don't. Scope split: the T3 session ruled the architecture
(pillars + generic struct + the inventory); the T5 session keeps the FLM-probe
acceptance run, extra framework knobs / extension seams, pillar-signature
sufficiency for post-parse state synthesis, and restage interaction.

Rejected alternatives: generic struct only (a customize-one-hook framework
wraps-and-delegates ~12 trait methods or forks the bodies — the cliff returns one
level up); pillars only (every adopt-wholesale consumer pays the delegation
boilerplate for nothing).

Revisit if: the T5 FLM probe finds a hook whose pillar signature cannot serve
post-parse state synthesis (the pillar, not the struct, is then the thing to fix).

*(Amended — API-review T4 session: the keep-`Copy`/`Eq` clause is struck — the
optional resolver field ([§dd-dr:input-wiring]) drops `Copy`/`Eq` on
resolver-carrying drivers ("why would we want `Copy`/`Eq` on the driver?" — no
in-crate reliance exists); shipped drivers keep `Clone + Debug`.)*

*(Amended — API-review T5 session, the shared-scope items ruled:
`exit_math_context_delta` takes **`&ParsingStateStack`**
([§dd-dr:enclosing-state-stack] amendment) — constructible post-parse via
`from_node_ancestors`, so the state-synthesis rider is served without a session;
the math-interior recipe is a documented **two-component** obligation on
`math_group_interior_delta`'s rustdoc — the pillar's delta **plus** the engine's
`expecting_group_close` descent invariant (a composed `…interior_state()` helper
rejected: a two-line composition, and wrong for languages overriding the math
plug); `make_paragraph_break_node` is documented parse-side-only (synthesis
stages `Chars` directly and never mints tokens). Driver knobs: **nothing
added** — `recovery`/`paragraph_break_style`/resolver are orthogonal config
values and every other behavior difference is a different driver over the
pillars; a `with_group_interior_delta` closure knob was rejected (re-grows a
behavior-carrying driver; the pillars compose in a custom driver — one doc
sentence at the struct); resolver field private behind
`with_resolver`/`source_resolver()`, the two policy knobs stay `pub`. The T5 FLM
projection confirmed the pillar inventory covers every non-default hook body
(~30-line Lang + ~12-line driver residue; the Phase 3 acceptance run asserts
it); the [§dd-dr:latexlike-generalization] pillar list's "`finalize_node`
spec-dispatch" is corrected there.)*

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
- **Byte-level `Read`/`BufRead` streaming** (dev-docs/archive/SOURCE_ARCHITECTURE.md) — the parser needs
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
  fact (commands, comments); an accessor serves `move_past`, the field taxed every token.
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
