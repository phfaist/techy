# Design Rationale & Decision Log

**Status: LIVING DOCUMENT.** Companion to [ARCHITECTURE.md](ARCHITECTURE.md).

Where ARCHITECTURE.md says *what* the architecture is, this document records *why* — the
arguments, trade-offs, and rejected alternatives behind each decision, plus questions still
open. Its purpose is to let a future session (human or agent) pick up design work without
re-deriving or accidentally re-litigating settled arguments, and without mistaking open
questions for settled ones.

This document supersedes the `DECISIONS.md` log proposed in ARCHITECTURE.md §10.

---

## 0. How to use and maintain this document

**For agents and future sessions:**

- Check an entry's **Status** before touching related code: `DECIDED` means don't re-litigate
  without new evidence; `OPEN` means the user has not signed off — ask, don't assume;
  `DEFERRED` means intentionally postponed — don't implement it speculatively.
- Every entry has a **Revisit if** clause. If that condition arises, raising the issue is
  welcome; otherwise treat the decision as settled.
- When a discussion produces a new decision or overturns an old one, **append or amend an entry
  here in the same session** — with date, rationale, and the alternatives considered. An
  undocumented decision will be re-argued from scratch in six months.
- Documentation precedence when documents conflict: this file and ARCHITECTURE.md >
  SOURCE_ARCHITECTURE.md > NAMING_STRATEGY.md > everything in `docs/archive/`. Newer beats
  older; user-authored beats generated.

**Process rules for this project** (from CLAUDE.md, restated because they govern design work):

- The user wants a high degree of control over design decisions. Propose with rationale and
  clearly marked decision points; do not silently decide.
- Never remove or "fix" user-written experimental code without confirming intent — argue
  against it in writing first (see §3.3 for the worked example of how this went well).
- Most of the codebase is provisional; docs describing decided architecture are more
  authoritative than code that hasn't been reviewed yet.

---

## 1. Project-level goals and constraints

These are the fixed points everything else serves (user-stated, July 2026):

1. **Flexibility** — minimal hard-coded decisions in the core.
2. **Extensibility** — custom parsers, token readers, specs, node payloads without forking.
3. **FLM target** — the [FLM project](https://github.com/phfaist/flm) will be redesigned on top
   of this library. Every core design must pass the "can FLM do X through public extension
   points?" test (see ARCHITECTURE.md §6 for the fit check).
4. **Low footprint** — minimal dependencies, small compiled artifact.
5. **No-compromise quality** — clean logical structure preferred over expedient shortcuts;
   clean slate, no pylatexenc backwards-compatibility baggage.
6. **Openness to revision** — better arguments win, including against the user's own earlier
   experiments; but through discussion, not unilateral change.

Corollaries: `Result<T, E>` everywhere and no panics in library code; tests accompany
functionality; public APIs documented with examples; no over-engineering or premature
optimization (a goal in tension with 1–2; §2 explains how the tension is resolved).

---

## 2. Meta-principles

These heuristics resolved most individual decisions below. When facing a new design question,
try these first.

### 2.1 Data where values change at runtime; traits where behavior changes

The single most load-bearing principle (July 2026). Anything a *state delta* may need to change
mid-parse — delimiters, escape characters, enabled features, specials strings — must be plain
data in the parsing state. A value behind a compile-time associated type cannot be changed by a
runtime delta, so facet-traits for such values are structurally wrong, not merely inelegant.
Traits are reserved for genuine behavior extension points: `TokenReader`, `ConstructParser`,
`SpecLookup`, `SourceResolver`, `CallableSpec`.

Litmus test for "should X be a trait?": *could two implementations differ in control flow, not
just in the values they return?* If they only differ in values, X is data.

### 2.2 One generic parameter, defaults everywhere

Generic customization is bundled into a single `Lang` trait with associated types, threaded as
one `L: Lang` parameter. History shows why: two prior designs independently drifted into
parameter proliferation (a 9-parameter `ParsingStateData` struct in the WIP code; `Parser<N, S,
C>` in the generated trait-architecture doc). Proliferation is the natural failure mode of
Rust generics and must be resisted structurally, not by discipline. Simple users must be able
to write code with zero visible generics (ZST preset lang + type aliases).

### 2.3 No privileged language concepts in the core

The engine knows nothing of math mode, `{`/`}`, `%`, or `\`. pylatexenc hard-codes
`in_math_mode: bool` into its core `ParsingState`; techy deliberately does not — "math mode" is
a preset-level state extension, `$…$` is just a configured group type, and mode-aware definition
lookup happens because `SpecLookup` receives the full parsing state. Rationale: this is what
makes the library a *toolkit for LaTeX-like languages* (and a viable FLM substrate) rather than
a LaTeX parser with escape hatches.

Guard rail: any proposal that adds a language-specific field to a core type should instead add
it to a preset's `StateExt`, `NodeData`, or library definitions.

### 2.4 Closed structural core, open payloads

The set of *structural shapes* (chars, group, callable invocation, comment, list) is a closed
enum; extensibility lives in payloads (`Custom(L::NodeData)`), specs (trait objects chosen at
definition time), and state extensions. Rationale: exhaustive pattern matching and
serializability are user priorities; `Box<dyn Node>` + downcasting sacrifices both to gain a
kind of openness nobody needs (new *structure* is rare; new *semantics* is common, and
semantics attach to payloads and specs).

### 2.5 Zero-copy by default

Tokens and nodes reference source content by byte spans; owned `String`s appear only where
content genuinely differs from any source slice (synthesized content). Transient borrow
lifetimes (tokens borrowing the current source) are fine as long as they never enter the AST.

### 2.6 Deterministic dispatch over registry scanning

Parsing dispatch follows data: token kind → construct parser, name → library lookup → spec →
invocation parser. Never "ask every registered parser if it can_parse() and pick by priority" —
that design makes behavior depend on registration order and hides dispatch logic in scattered
predicates. If syntax needs to enter the pipeline, it enters as data (a specials string, a
group type, a spec) or as an explicit replacement of a well-defined component.

---

## 3. Decision register

Format: **Status** (DECIDED / PROPOSED / OPEN / DEFERRED) · date · decision · why · rejected
alternatives · revisit-if.

### 3.1 Sources and spans

**Arc-based source ownership** — DECIDED (user-led design discussion, March 2026;
SOURCE_ARCHITECTURE.md).
Nodes carry `SourceSpan { Arc<Source>, start, end }`; specs and parsing states are likewise
`Arc`-wrapped in nodes. The decisive argument is **post-processing**: tree transformations
produce new trees mixing old and new nodes, and Arc makes nodes self-contained — transformed
trees outlive the original `ParseResult`, with no lifetime chains across N transformations.
Cost: ~1ns refcount bump per node — negligible.
*Rejected:* `SourceId` + store (circumvents borrow checking; id meaningless without its store);
lifetime `'src` on all AST types (self-referential struct problem + transform chaining);
index-based spans with a `SourceStore` in `ParseResult` (ties nodes to one result).
*Revisit if:* profiling ever shows Arc traffic mattering (then see §3.7 pointer genericity).

**Provenance lives on `Source`, not on every location** — PROPOSED (July 2026 plan).
`SourceProvenance` (`Primary`/`Resolved`/`Synthesized` with `triggered_at: SourceSpan`) is one
hop per *source*, forming a provenance tree walkable for error reports. The WIP code's
per-location `via: [SourceLocationVia]` vector paid per-token/per-node cost for information
that is constant per source. Removing it also structurally prevents Arc cycles: the invariant
is *source types never reference node types* (reference graph strictly layered:
nodes → sources/specs/state; sources → sources).
*Revisit if:* a use case needs per-node provenance distinct from its source's provenance
(e.g. token-level macro-expansion tracing à la TeX).

**Line/column is a lazy, standalone utility** — DECIDED (March 2026, refined July 2026).
The parser works purely in byte offsets; `LineIndex` computes line starts lazily and only for
display (errors, diagnostics). The lazy-extension logic and traceback formatting in the current
`source.rs` are worth porting.
*Rationale:* upfront line indexing costs O(source) on every parse for data usually never read.

**Pluggable content resolution** — DECIDED (March 2026).
`SourceResolver` trait for `\input`-like lookups; `NoResolver` is a ZST so a no-I/O build pays
nothing. `SourceContent` trait abstracts backing storage so mmap can arrive later without
parser changes (DEFERRED until a real need). No file-system resolver is shipped (no_std
policy, §3.9): an embedder implements `SourceResolver` on its side, where the I/O capability
lives; the in-memory `MapResolver` covers tests and fully preloaded setups.

**Origin genericity without `Lang` (Phase 1)** — DECIDED (user, July 2026, Phase 1 kickoff);
**default origin simplified to an optional URL string** — REVISED (user, July 2026).
`Source<O: SourceOrigin = Option<String>>` takes the origin type as a plain, defaulted type
parameter; `SourceSpan`/`SourceProvenance`/`SourceResolver`/`Diagnostic` carry the same
parameter. When `Lang` arrives (Phase 3+), higher layers plug `L::SourceOrigin` into this
parameter — L0 never depends on `Lang`, preserving the strict layering of ARCHITECTURE.md §3.
The `SourceOrigin` trait provides only `label()` (diagnostics display) on top of
`Debug + Clone + Default`. The default origin type is `Option<String>`: conventionally the
URL the content was obtained from, `None` when unknown or when the content was synthesized.
The division of labor: origin is optional *display metadata about where content was
obtained*; `SourceProvenance` — which every source carries — is the *structural* record of
how it entered the parse, and it (not the origin) holds synthesis descriptions and
resolution references. One inference consequence of the defaulted parameter: bare
`Source::new(…)` cannot infer `O`, so simple usage annotates (`let src: Arc<Source> = …`)
until the Phase-3+ type aliases make it moot.
*Rejected:* a concrete-now/genericize-in-Phase-3 approach (would retrofit a type parameter
through every L0 signature later). Also rejected, in the July 2026 revision: the first-cut
`StdSourceOrigin` enum (`Unknown` / `Named { name, kind: File | Snippet | Resolved |
Synthesized | Other }`). Its kind taxonomy was too detailed and too rigid for the intended
generality (where does content fetched from a database fall?), it partially duplicated
provenance (`SourceOriginKind::Resolved` vs `SourceProvenance::Resolved` answered the same
question twice), and the `File` kind clashed with the no_std policy (§3.9). The trait's
`synthesized()`/`resolved()` origin constructors went with it: generic machinery no longer
*mints* origins — a source starts with the default ("unknown") origin, and a creator that
actually knows a URL attaches it via `with_origin`.

**`SourceContent` is a trait boundary, not (yet) a `Source` parameter** — DECIDED (user,
July 2026, Phase 1 kickoff). The trait exists (implemented by `str` and `String`) and
`SourceCursor<'s, C: SourceContent + ?Sized = str>` is generic over it, but `Source` stores a
concrete `String`, with all content access behind methods so the backing can later become
generic (mmap) without changing the public API. Explicitly: keep the enabling pattern, do not
implement mmap until a real need.

### 3.2 Tokens and tokenization

**Tokens are minimal and structural** — DECIDED (user, PARSING_STRATEGY.md, Jan 2026).
A token identifies *what to parse next* (macro name, group open/close, comment start, specials
candidate, chars, paragraph break) and nothing more. Notably there is **no
`BeginEnvironment(name)` token** — `\begin` is an ordinary macro token, and environment
recognition is a construct-parser concern (preset registers `\begin`/`\end` specs). This is a
deliberate departure from pylatexenc, whose tokenizer bakes in environment syntax.
*Rationale:* keeps the tokenizer language-agnostic (§2.3) and moves all semantics to the
spec/parser layer where it is extensible.

**Zero-copy tokens with ephemeral lifetime** — PROPOSED (July 2026).
`Token<'s>` holds `TokenKind<'s>` with `&'s str` slices plus `Span`s; `pre_space` is a `Span`,
not a `String` (the current WIP allocates a `String` per token for whitespace — pure waste).
The `'s` lifetime never enters the AST.
*Revisit if:* a streaming token source can't expose stable slices (then the `SourceContent`
boundary is the place to solve it, not the token type).

**`TokenReader` is the behavior extension point for tokenization** — DECIDED in shape
(user, PARSING_STRATEGY.md; API refined July 2026).
The provided `StdTokenReader` is 100% driven by `TokenRules` data; anyone needing genuinely
different tokenization *behavior* (catcode-like schemes, non-textual sources) implements the
trait. The peek/move_past/move_to protocol (with `rewind_pre_space` / `skip_post_space` flags)
follows pylatexenc's proven `LatexTokenReaderBase` design.
Salvage note: the WIP `detect_*` decomposition and the cached sorted delimiter-prefix table
(with open/close ambiguity merging, e.g. `$` both opening and closing) are good and should be
ported into `StdTokenReader`.

### 3.3 Parsing state and deltas

**Tokenization config is plain data (`TokenRules`), not per-facet traits** — PROPOSED
(July 2026), **reverses the user's most recent code experiment — needs explicit sign-off**
(ARCHITECTURE.md DECISION 1).
The WIP `src/state/` gave each facet (whitespace, groups, macros, comments, …) its own trait +
macro-generated data struct, composed via 9 associated types. The decisive argument against:
**it contradicts the delta system** — these values must change *mid-parse* (math library adds a
`$` group type; verbatim disables everything; `\makeatletter` changes name chars), and values
behind compile-time associated types can't be changed by runtime deltas. Supporting arguments:
facet traits only abstract storage layout, which nothing needs; the macro DSL + 9-way generics
are exactly the proliferation §2.2 warns about; genuine behavior variation is already covered by
`TokenReader`.
*Rejected:* the facet-trait design; also the `TypeId`-keyed `Any` extension map from the
generated docs (runtime-typed, allocation-heavy, unnecessary once `L::StateExt` exists).
*Revisit if:* a preset needs tokenization *rules* whose evaluation is behavioral, not
value-like — first try expressing it as data; then as a `TokenReader` wrapper.

**Language-specific state is a typed extension (`L::StateExt`)** — PROPOSED (July 2026).
Math mode, FLM flags, etc. live in a compile-time-typed field, not a dynamic map. Type safety
and zero lookup cost; dynamic-language bindings (Python/JS, if ever) can define one `Lang` with
a dynamic `StateExt` — the cost is contained to those bindings instead of taxing all users.

**Immutable state, explicit deltas, Arc-shared snapshots** — DECIDED (pattern inherited from
pylatexenc, kept deliberately; March + July 2026).
Construct parsers return `(output, Option<delta>)`; the caller applies deltas. The engine
creates a new `Arc<ParsingState>` only at transitions, so all nodes parsed under one state share
one Arc, and nodes record their parse-time state (needed because a name-based spec lookup
*after* parsing would find the wrong spec if definitions changed mid-document). Group-local
state (definitions pushed inside `{…}`) pops naturally by restoring the previous Arc.
*Rationale for parser-returns-delta rather than parser-mutates-state:* the caller decides
scope — whether a delta applies to following siblings or dies with the group.

**Settings are stored data; dependent settings recomputed at transitions (Option C)** — DECIDED
(user-led, July 2026; ARCHITECTURE.md §L2/§4, Decision 1 RESOLVED).
Every effective setting is a plain field — no getters compute values on read. Cross-cutting or
derived settings (e.g. escape char = `#` in math mode) are recomputed by a single
`Lang::finalize_transition(new, prev, events)` hook that runs when a new state is built. The
delta is a concrete `ParsingStateDelta<L>` value (optional overrides + typed `L::Event`s — the
pylatexenc "changed kwargs"), applied only through `ParsingState::derived()`, the sole
constructor of non-initial states, over private fields — so the recompute choke point is
airtight. `&mut` exists only internally, pre-freeze; the public contract has no mutation.
*Rationale:* any change to an effective setting *is* by definition a transition, so
compute-per-read buys nothing over recompute-at-transition, while C keeps hot-path field reads,
truthful debuggability (`dbg!(state)` shows real values), and one central finalize function. The
delta is a **struct, not a closure** because producer and scope-decider differ (outward
propagation: `\newcommand`'s delta is applied by callers to base states the producer never saw),
and a struct is mergeable, inspectable, propagatable, and batchable.
*Rejected:* Option A (concrete state + per-getter `Lang` hooks — hooks "patch" the storage
model); Option B (whole state behind an `L::State: ParsingStateModel` getter trait — see §4 for
the cost list; the swappable storage it buys is speculative and recoverable later behind a getter
surface, so C keeps B's door open, not vice versa); a closure-shaped delta (not
mergeable/inspectable/propagatable).
*Revisit if:* a preset genuinely needs swappable state storage (re-evaluate B behind getters —
C→B is the intended one-way door).

### 3.4 Specs and libraries

**Unified `CallableSpec` with self-supplied invocation parser** — PROPOSED (July 2026,
generalizing pylatexenc's `CallableSpecBase`).
Macros, environments, and specials are all "callables": name + a parser for their invocation.
The common path is declarative (`ArgumentStructureSpec` list), with an optional custom parser
override. This preserves pylatexenc's most valuable extensibility property — *a spec can fully
take over parsing its own invocation* — required by `\verb`, tabular preambles, and FLM's
richer constructs.
*Rationale:* specs are data + optional behavior, matching §2.1.

**Library stack with lexical shadowing; no `ConflictStrategy`** — DECIDED (July 2026,
ARCHITECTURE.md DECISION 6).
Ordered stack, innermost/last wins. Shadowing *is* the intended semantic (`\newcommand`
redefinition, group-local definitions), so a configurable conflict policy (PROPOSALS.md's
`FirstWins`/`LastWins`/`Error`) solves a non-problem while complicating resolution; an optional
lint can warn on shadowing if ever wanted.
*Deferred:* the `SpecLookup` semantics and behavior are to be discussed (see §6).

**Mode-aware lookup without built-in modes** — PROPOSED (July 2026; part of the deferred
`SpecLookup` discussion, see §6).
`SpecLookup::lookup()` receives `&ParsingState<L>`; a preset's implementation may dispatch on
`state.ext` (FLM resolving `\vec` differently in math mode). The core `Library` ignores the
state. This replaces PROPOSALS.md's hard-coded `math_mode_macros` tables, which contradicted
§2.3.

### 3.5 Nodes and AST

**Flat `NodeTree` (Vec + index ranges), frozen after parse, `NodeRef` proxy access** — DECIDED
(March 2026). Cache-friendly, no per-node heap allocation, trivially serializable; `NodeRef`
(Copy, borrows `ParseResult`) makes indices safe by construction — the borrow checker
guarantees a `NodeRef` can't outlive the tree its index points into. Mutation happens only
inside `ParserSession`; `finish()` consumes the session, so there is no mutable/immutable
conflict by design.

**Closed `NodeKind<L>` enum + `Custom(L::NodeData)` variant** — PROPOSED (July 2026,
ARCHITECTURE.md DECISION 3). See §2.4 for the principle.
*Rejected:* `trait Node` + `Box<dyn Node>` + `as_any()` downcasting + `clone_box()` (the
generated TRAIT_BASED_ARCHITECTURE.md design) — loses exhaustive matching, adds per-node
boxing, makes serialization and flat storage impossible, and reintroduces runtime type errors
that the type system should prevent.

**No core `MathNode`** — PROPOSED (July 2026, consequence of §2.3).
`$…$` parses as a `Group` with a `$`-delimited `GroupTypeId` under a math-mode state extension;
the latexlike preset provides accessor helpers so ergonomics don't suffer.
*Revisit if:* preset-level ergonomics prove genuinely painful in practice — the fallback is a
preset-defined `Custom` node, still not a core variant.

### 3.6 Construct parsers, dispatch, engine

**Single-context parsing API (`ParseContext`)** — PROPOSED (July 2026).
Bundles token reader + state + session handle, avoiding pylatexenc's three-argument threading
through every parser. One place to extend later (e.g. depth limits, cancellation).

**Dispatch by token kind + library lookup** — PROPOSED (July 2026). See §2.6.
*Rejected:* `can_parse()`/`priority()` parser registries (registration-order-dependent,
scattered dispatch logic, priority races).

**`Language<L>` owns no per-parse state** — DECIDED (March 2026, as "FLMEnvironment";
renamed July 2026). Long-lived, reusable across parses, accumulates no memory. Sessions are
transient; results are frozen.

### 3.7 Generics strategy

**Defer `Rc`/`Arc` genericity** — DECIDED (July 2026, ARCHITECTURE.md DECISION 4).
The `SharedPointer` GAT sketched in SOURCE_ARCHITECTURE.md would infect nearly every signature
in the crate to save ~1ns uncontended atomic increments that happen once per node, not per
byte. Use `Arc` behind an internal alias (`pub(crate) type Shared<T> = Arc<T>`) so a later swap
is mechanical.
*Revisit if:* profiling on real workloads shows refcount traffic, or a wasm/embedded target
genuinely needs `Rc`.

**What is generic (via `Lang`) and what is not** — PROPOSED (July 2026).
Generic: `StateExt`, `NodeData`, `SourceOrigin`. Not generic: spec types (extensibility comes
from `CallableSpec` being a trait), pointer type (above), content backing (behind
`SourceContent` trait instead). Every proposed new `Lang` associated type should be challenged
against §2.1/§2.2 first.

### 3.8 Errors and diagnostics

**`Result` everywhere; no panics in library code** — DECIDED (project constraint, CLAUDE.md).

**Errors carry Arc-based `SourceSpan`, not `'src` lifetimes** — PROPOSED (July 2026).
The current `ParseError<'src>` / `Result<'src, T>` spreads a lifetime through every signature
and prevents errors from outliving the parse. Arc spans fix both at negligible cost (errors are
rare and cold).

**Tolerant parsing via recovery tokens + diagnostics sink** — PROPOSED (July 2026, formalizing
the user's WIP mechanism in `error.rs`/`stringreader.rs` — one of the pieces worth salvaging).
Tokenizer errors may carry a recovery token; a session-level `Recovery` policy (strict /
tolerant) decides whether to record a diagnostic and continue or abort. Diagnostics accumulate
on the session and remain available on `ParseResult` even for successful tolerant parses.
*Rationale:* tolerant parsing is a first-class requirement for document tooling (FLM, linters,
editors), not an afterthought flag; and a diagnostics sink is the API-honest replacement for
logging side channels (see §3.9).

**Recovery mechanism split across phases** — DECIDED (user, July 2026, Phase 1 kickoff).
Phase 1 ships the token-independent parts: `Diagnostic`/`Diagnostics`/`Severity` and the
`Recovery` policy enum (strict/tolerant). `TokenError { …, recovery: Option<Token> }` lands in
Phase 2 next to `Token<'s>`, where it can be designed against a real tokenizer.
*Rejected:* a token-agnostic `TokenError<R>` placeholder in Phase 1 (designing the type blind,
then reshaping it in Phase 2 anyway).

### 3.9 Dependencies — **DECIDED** (ARCHITECTURE.md Decision 5; implemented July 2026, Phase 1)

**Absolute minimal mandatory dependencies** — `thiserror` and `log` removed from `Cargo.toml`
(July 2026). The considerations that led there:

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
  sink (§3.8), where callers can actually react to it. A library that communicates through its
  API needs no logging facade. Recommendation: drop, or feature-gate if a concrete debugging
  need appears. This half is nearly free.
- Not under discussion: heavier deps. Nothing in the design needs regex, serde (could be an
  optional feature later), or unicode tables beyond `char` methods.

**`no_std`-friendly, alloc-only** — DECIDED (user, July 2026). The library must build without
`std`; allocation is fine (`#![cfg_attr(not(test), no_std)]` + `extern crate alloc` in
`lib.rs`; tests keep `std` for convenience). Consequences: no I/O anywhere in the library —
the file-reading `FileResolver` was removed (an embedder implements `SourceResolver` where
the I/O capability lives), and the `File` origin kind fell with it (see §3.1); `alloc`
collections only (`MapResolver` uses `BTreeMap`, not `HashMap`); error types implement
`core::error::Error`, which sets MSRV 1.81 (`rust-version` in `Cargo.toml`); `Arc` comes
from `alloc::sync`, so targets must support atomics. A plain `cargo build` compiles the
library with `no_std` active and thus guards the policy without a bare-metal CI target.


### 3.10 Naming

Decided conventions (NAMING_STRATEGY.md, Dec 2025, still in force):

- **No `Latex` prefixes** — the library is markup-generic (`Token`, not `LatexToken`).
- **Specificity over brevity** where confusion is possible: `ParsingStateDelta` not
  `StateDelta`; `ArgumentStructureSpec` not `ArgumentsSpec` (one letter from `ArgumentSpec`).
- **Context makes qualifiers redundant**: `Arguments`, not `ParsedArguments`.
- **Collision avoidance beats tradition**: `Language<L>` replaces March's `FLMEnvironment`
  (fatal collision with `EnvironmentSpec`/`EnvironmentNode`); `ConstructParser` avoids clashing
  with any high-level `Parser` type; `Lang` replaces `LanguageSpecification` (too long for a
  parameter appearing in nearly every signature).

When naming something new: check NAMING_STRATEGY.md, then ask "does this collide with or
shadow an existing concept in LaTeX terminology or in this codebase?"

---

## 4. Rejected patterns — do not reintroduce

Quick-reference list of patterns that have been considered and rejected. Each links the section
holding the full argument.

- **`Box<dyn Node>` + `Any` downcasting + `clone_box`** (§3.5) — loses exhaustive matching, adds
  per-node boxing, and makes flat storage and serialization impossible.
- **`can_parse()`/`priority()` parser registries** (§3.6, §2.6) — behavior depends on
  registration order and dispatch logic scatters across predicates.
- **`TypeId`-keyed `Any` maps for state/node extensions** (§3.3) — runtime-typed and
  allocation-heavy; `L::StateExt`/`L::NodeData` do the same statically.
- **Per-facet parsing-state traits (9 associated types)** (§3.3) — values behind compile-time
  associated types can't be changed by runtime deltas; also textbook generic proliferation.
- **Whole state behind an `L::State: ParsingStateModel` getter trait** (§3.3, Option B) —
  abstracts storage nothing needs, at real cost: the engine still needs a wrapper for derived
  caches; trait laws (getter purity, delta locality, stored/effective split) silently *become*
  the design; compound getters need `Cow` shapes; ext access needs capability traits; "default
  plus one tweak" means delegation boilerplate; and `dbg!(state)` lies because effective values
  are computed on read. Option C (stored data + `finalize_transition`) gets the same
  centralization with truthful debugging and hot-path field reads.
- **Closure-shaped state deltas** (§3.3) — a delta must stay a reified value so it can be
  merged, inspected, and propagated to base states its producer never saw (outward
  `\newcommand` propagation); a closure supports none of that.
- **Hard-coded math-mode definition tables in libraries** (§3.4) — violates §2.3;
  `SpecLookup(state)` dispatching on the state extension covers it without built-in modes.
- **`ConflictStrategy` for library resolution** (§3.4) — shadowing *is* the intended semantic
  (`\newcommand`, group-local defs), not a conflict to configure away.
- **`SourceId` into an external store** (§3.1) — circumvents borrow checking; the id is
  meaningless without its store.
- **`'src` lifetimes on AST/error types** (§3.1, §3.8) — self-referential structs and
  lifetime chains across N tree transformations; Arc spans fix both at negligible cost.
- **Per-location provenance chains (`via` vectors)** (§3.1) — pay per-node cost for information
  that is constant per source; provenance lives on `Source`.
- **Byte-level `Read`/`BufRead` streaming** (SOURCE_ARCHITECTURE.md) — the parser needs
  lookahead/backtrack, so it wants a cursor over `&str`, not a byte stream.
- **Tokenizer-level environment recognition (`\begin{…}` tokens)** (§3.2) — bakes language
  semantics into the tokenizer; `\begin` is an ordinary macro, environments are a parser concern.

---

## 5. Non-goals

Decided intentional limitations (PROPOSALS.md §4 gap analysis, reaffirmed July 2026):

- **techy is not a TeX engine.** No catcode system, no macro expansion engine, no conditional
  (`\if…`) evaluation, no full primitive set. Target use cases are structural parsing for
  conversion, analysis, and tooling — pylatexenc's niche, and FLM's need.
- Escape hatch, documented: anyone needing catcode-like tokenization implements `TokenReader`.
- `\newcommand` **is** supported, but at the parse level (a library-extension delta defining a
  new spec), not as token-stream expansion.
- Deferred, with trait boundaries already in place so no parser changes are needed later:
  memory-mapped sources (`SourceContent`), streaming/incremental parsing, `Rc` pointer
  genericity (§3.7).

---

## 6. Open questions

Current list — remove entries as they are settled (move the outcome into §3):

1. **`SpecLookup` semantics and behavior** (§3.4): deferred from ARCHITECTURE.md
   decision 6 (the no-`ConflictStrategy`/shadowing part is decided). To be discussed before
   Phase 4. (Decision points 1–7 are otherwise signed off, July 2026; §3 entries marked
   PROPOSED become DECIDED as they land.)
2. **`ArgsLayout` / children encoding in flat `NodeData`**: how macro arguments vs environment
   body vs group contents share the `children: Range<u32>` mechanism — deliberately deferred to
   Phase 5 implementation, where real cases will inform it.
3. **Top-level convenience API**: does a thin `Parser` facade exist above `Language::parse()`,
   or is `Language` the entry point? Bikeshed; defer to Phase 6.
4. **`CompactString`**: plain `String` initially; whether a small-string optimization ever pays
   for delimiter/specials storage is a profiling question, not a design question.

---

## 7. Entry template for future decisions

```
**<Short decision title>** — <STATUS> (<who/context>, <month year>).
<The decision, one or two sentences.>
*Rationale:* <the argument that carried it — especially the one decisive reason.>
*Rejected:* <alternatives considered, each with its killing flaw.>
*Revisit if:* <concrete condition under which reopening is warranted.>
```

Keep entries short and argumentative — the goal is that a future reader can reconstruct *why*
without replaying the conversation. Record the decisive reason, not every reason.
