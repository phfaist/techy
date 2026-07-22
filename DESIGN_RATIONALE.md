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
  NAMING_STRATEGY.md > everything in `dev-docs/archive/` (which includes SOURCE_ARCHITECTURE.md,
  folded into ARCHITECTURE.md and archived July 2026). Newer beats older; user-authored beats
  generated.

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
enum; extensibility lives in payloads (the two-tier ext system of `Lang::NodeExts` — no
`Custom` variant; ARCHITECTURE.md Decision 3), specs (trait objects chosen at definition
time), and state extensions. Rationale: exhaustive pattern matching and serializability are
user priorities; `Box<dyn Node>` + downcasting sacrifices both to gain a kind of openness
nobody needs (new *structure* is rare; new *semantics* is common, and semantics attach to
payloads and specs).

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
nodes → sources/specs/state; sources → sources) — generalized in July 2026 to the crate-wide
acyclic-runtime-ownership rule (§3.11, rule 3).
*Revisit if:* a use case needs per-node provenance distinct from its source's provenance
(e.g. token-level macro-expansion tracing à la TeX).

**Source→triggering-node mapping lives in a session-owned registry** — general direction
DECIDED, details OPEN (July 2026).
The reverse question "which node triggered this synthesized/resolved source" is answered by a
higher-level registry owned by `ParserSession`, keeping track of the synthetic sources and the
nodes that created them. How the registry refers to nodes, and its exact lifecycle, are to be
decided (not plain `NodeId`s).
*Rejected:* recovering the node by O(n) span search over the tree — works, but an implicit,
lossy lookup where an explicit owned mapping is cheap and direct.

**Line/column is a lazy, standalone utility** — DECIDED (March 2026, refined July 2026).
The parser works purely in byte offsets; `LineIndex` computes line starts lazily and only for
display (errors, diagnostics). The lazy-extension logic and traceback formatting in the current
`source.rs` are worth porting.
*Rationale:* upfront line indexing costs O(source) on every parse for data usually never read.

**Pluggable content resolution** — DECIDED (March 2026).
`SourceResolver` trait for `\input`-like lookups; `NoResolver` is a ZST so a no-I/O build pays
nothing. No file-system resolver is shipped (no_std policy, §3.9): an embedder implements
`SourceResolver` on its side, where the I/O capability lives; the in-memory `MapResolver`
covers tests and fully preloaded setups. *(The `SourceContent` backing-abstraction half this
entry originally carried is retired — see the July 2026 retirement entry below.)*

**Origin genericity without `Lang` (Phase 1)** — DECIDED (user, July 2026, Phase 1 kickoff);
**default origin simplified to an optional URL string** — REVISED (user, July 2026).
`Source<O: SourceOrigin = Option<String>>` takes the origin type as a plain, defaulted type
parameter; `SourceSpan`/`SourceProvenance`/`SourceResolver`/`Diagnostic` carry the same
parameter. When `Lang` arrives (Phase 3+), the S1 core plugs `L::SourceOrigin` into this
parameter — the source topic never depends on `Lang`, per the Lang-free-foundation rule
(S0; ARCHITECTURE.md §3, rule 1).
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
July 2026, Phase 1 kickoff). *(Superseded the same month — retired outright; next entry.)*
The trait exists (implemented by `str` and `String`) and
`SourceCursor<'s, C: SourceContent + ?Sized = str>` is generic over it, but `Source` stores a
concrete `String`, with all content access behind methods so the backing can later become
generic (mmap) without changing the public API. Explicitly: keep the enabling pattern, do not
implement mmap until a real need.

**`SourceCursor`, `Source::cursor()`, and `SourceContent` retired** — DECIDED (user, July
2026, Action-06 review; supersedes the entry above). The intended consumer went another
way: `StdTokenReader` holds `content: &'s str` and scans the `str` directly. Its access
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
reader design, not a backing swap behind `Source`. ~215 lines retired (content.rs,
`Source::cursor()`, the re-exports); `Source` keeps its plain `String` field with
access behind methods.
*Rejected:* re-labeling the cursor as an embedder convenience for custom `TokenReader`s
(nothing needs it, and `&str` + `usize` is simpler than a bespoke cursor API).
*Revisit if:* a genuinely streaming source materializes — design the chunked reader
then, with a content abstraction shaped by its real requirements.

**`Span` has private fields; in-place growth is the monotone `extend_to`** — DECIDED
(user, July 2026, Action-05 session). `Span`'s `start`/`end` went private with
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

**`SourceResolver` contract batch: returns content (not a `Source`), `Send + Sync`,
no core recursion checking, `ResolveError` cause chain** — DECIDED (user, July 2026,
Action-05 session; settled before any consumer exists — the wiring lands on
`Language<L>` in Phase 7).
- **`resolve()` returns `ResolvedContent { content, origin }`; the caller mints the
  `Source`** (the `resolve_source` composition). Rationale: provenance lives on the
  `Source` (`Resolved { reference, triggered_at }`) and diagnostics self-render include
  chains from it, so a resolver-cached `Arc<Source>` shared across two include sites
  silently renders the wrong chain inside the second inclusion. Returning content makes
  the trap *unrepresentable* — provenance never passes through implementor hands, and
  resolvers may cache content freely. (Content duplication per include site is inherent
  while `Source.content` is a `String`; switching that private field to `Arc<str>` later
  would remove it without touching this contract.) *Rejected:* a documented
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
  exotic origins write their own ten-line resolver), and the trait doc now says the
  parser "will be" generic over the resolver until it is.

### 3.2 Tokens and tokenization

**Tokens are minimal and structural** — DECIDED (user, PARSING_STRATEGY.md, Jan 2026;
sharpened by the July 2026 token-design review). A token identifies *what to parse next*
(single char, group open/close, command, specials, comment, paragraph break, end of
stream) and nothing more. Notably there is **no `BeginEnvironment(name)` token** —
`\begin` is an ordinary command token, and environment recognition is a construct-parser
concern (preset registers `\begin`/`\end` specs). This is a deliberate departure from
pylatexenc, whose tokenizer bakes in environment syntax.
*Rationale:* keeps the tokenizer language-agnostic (§2.3) and moves all semantics to the
spec/parser layer where it is extensible. ("Minimal" bounds *language knowledge*, not token
extent: a whole-comment token is fine because comment interiors carry no structure the
parser cares about — see the review entry below.)

**The token-design review: final token model** — DECIDED (user-led, three-round discussion,
July 2026; implemented in the merged Phase 3). Supersedes the four Phase-2 PROPOSED entries
that previously stood here (uniform `post_space`; maximal-run `Chars`; `Ok(None)` at EOF —
each recorded below as rejected) and moves the token topic wholly into S1.
Final model: `Token<'s, L> { kind, span, pre_space }` with `TokenKind<'s, L>` =
`Char(char)` | `GroupOpen`/`GroupClose` | `Command { name, post_space }` |
`Specials { name, spec: Arc<dyn CallableSpec<L>> }` | `Comment { content, post_space }` |
`ParagraphBreak` | `EndOfStream`. The decisions, each with the argument that carried it:

- **No invocation forms at the token level.** No macro/environment/specials taxonomy and no
  `CallableTypeId` on tokens: `\begin` is a `Command` exactly like `\foobar`; which names
  are macros or environments is resolution *output*, assigned at parse time by the preset.
  Dropping the type id from tokens dissolved the "token says MACRO, node says ENVIRONMENT"
  wart outright. Terminology stack: *command* (token-level syntactic form; TeX lineage) →
  *callable* (parse-level concept, Decision 3) → *macro*/*environment*/*specials*
  (preset-level invocation flavors). "Command" over "escape": a future non-escape command
  syntax (`@MARKER@`-style, a possible `CommandRule` extension) would make "escape" a
  misnomer, and "escape token" wrongly connotes escaped-character semantics (`\&` as
  literal `&`). *(Amended July 2026, Phase 6.4, user-approved: this rule is scoped to
  tokens whose resolution happens at parse time — `Command`. The `Specials` token, whose
  recognition **is** resolution (next-but-one bullet), now carries the resolved
  `callable_type` alongside its spec: a resolution is the `(callable_type, spec)` pair —
  `ResolvedCallable`'s exact shape — and the dispatch loop needs both to build an
  `Invocation`. The "token says MACRO, node says ENVIRONMENT" wart cannot re-arise: both
  fields come from the single scan-time resolution site, so there is no second resolution
  to disagree with.)*
- **Single-character `Char` tokens** (reverses Phase 2's maximal runs). A token is an
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
  spec, the `ResolvedCallable` pair (`callable_type` added July 2026, Phase 6.4,
  user-approved) — in one call: scanning/lookup normalization or scoping mismatches are
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
  becomes whitespace chars nodes, §3.5), *post-space is syntactic whitespace* (consumed by
  the construct's syntax, ignored as content, reproduced verbatim in recomposition).
  Post-space exists only where *tokenization syntax* consumes whitespace — multi-character
  `Command` names (whitespace terminates the name) and `Comment`s (the newline terminates
  the content) — and is stored **in those variants**, not as a uniform `Token` field
  (`Token::post_space()` accessor serves `move_past`'s `skip_post_space` flag). Groups
  never have post-space (space after `}` is content); specials and single-char commands
  (`\&`) neither. Spec-driven whitespace swallowing beyond this is a parse-level concern
  recorded on nodes.
- **One whitespace primitive: `skip_whitespace`.** With
  `TokenRules::double_newline_paragraphs` set (name follows pylatexenc's
  `enable_double_newline_paragraphs`), skipped whitespace never contains `\n\s*\n` nor
  consumes a newline belonging to such a sequence — skipping stops *before* the sequence's
  first newline. Used identically for pre-space, command post-space, and comment
  post-space, so "post-space never crosses a paragraph break" holds everywhere by
  construction, and the paragraph-break token is detectable exactly where skipping
  stopped. The flag gates both the skip rule and `ParagraphBreak` emission — one
  phenomenon, one flag.
- **Whole-`Comment` tokens** (reverses Phase 2's delimiter-only `CommentStart`). The
  parser has no business inside comment content, so granular comment tokenization bought
  nothing; candidates for granularity (block/nested comments) are served by a future
  per-rule terminator extension of `CommentRule` or the `TokenReader` escape hatch.
  `CommentRule { start }` mirrors `CommandRule` (several syntaxes; longest start wins);
  the terminator is end-of-line implicitly, independent of `WhitespaceRules` (`'\n'`
  exactly — `'\r'` gets no special treatment, see the Action-02 entry, item 6). Corner
  pinned: `a% c\n\nb` — the comment's terminating newline belongs to a `\n\s*\n` sequence,
  so the comment takes **no** post-space and the `ParagraphBreak` survives as its own
  token (TeX-observable behavior: the blank line still yields `\par`). Consequence:
  `CommentParser` is vestigial — comment nodes come straight from tokens.
- **Terminal `EndOfStream` token; `peek` never returns an `Option`.** `EndOfStream` is
  idempotent and its `pre_space` carries the input's final whitespace, so trailing
  whitespace reaches the node tree through the ordinary token path — the nodes parser
  never reaches around the reader into raw content (which a custom `TokenReader` might not
  meaningfully expose). *(It briefly also served as the recovery placeholder for a dangling
  escape at EOF — Phase 2 used an empty `Chars` token, impossible once `Chars` became
  `Char(char)` — but that recovery dropped the escape byte from the tree; superseded by the
  `Char(escape_char)` placeholder, see the Action-02 token entry below.)*
- **The token topic is wholly S1; tokens are generic over `L`.** `Specials` carries
  `Arc<dyn CallableSpec<L>>` (tokens are `Clone`, not `Copy`), and `TokenError<'s, L>` may
  grow state context. Tokens remain transient `'s`-bound engine internals; the genericity
  never enters the AST. `Span` — a generic byte range used by errors and cursors
  independently of tokenization — moved to the source topic (S0). This supersedes the
  Phase-2-era "scanning core is S0" stratum split (§3.11's consequence bullet, revised);
  the S0-testability property was traded for the freedom to keep state context in token
  machinery, and a trivial test `Lang` restores testability at negligible cost.

*Rejected along the way (three-round arc: maximal abstraction → whitespace-as-token →
this hybrid):*
- *Unified `Callable` token kind absorbing Command and Specials* — hid that the two are
  produced by different mechanisms (rules data vs. preset hook) and dragged
  `CallableTypeId` into tokens.
- *`CallableTypeId` on tokens / on `CommandRule`* — invocation form is resolution output,
  not tokenization output. (Follow-up noted in §6: with several `CommandRule`s, the
  parse-time lookup needs the escape char for disambiguation — pass the token.)
- *Whitespace as its own token* — killed by parser ergonomics: every construct parser's
  peek grows a "maybe whitespace first" case; the pre/post-space encoding localizes that
  cost in the tokenizer. (For fairness: it would have bought a token-span partition
  invariant, flag-free `move_past`/`move_to`, and a field-free `EndOfStream`.)
- *Uniform `post_space: Span` on every token* (Phase 2) — post-space is a per-kind
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

**Zero-copy tokens with ephemeral lifetime** — DECIDED (implemented July 2026, Phase 2;
upheld through the token redesign).
`Token<'s, L>` holds `&'s str` slices plus `Span`s; `pre_space`/post-space are `Span`s, not
`String`s. The `'s` lifetime never enters the AST.
*Revisit if:* a streaming token source can't expose stable slices (that calls for a
chunked-content reader design — see §3.1's `SourceContent` retirement entry — not a
change to the token type).

**`TokenReader` is the behavior extension point for tokenization** — DECIDED (user,
PARSING_STRATEGY.md; trait landed July 2026, Phase 3).
`StdTokenReader` is driven by the parsing state (rules data + cached tables + the
`scan_specials` hook); anyone needing genuinely different tokenization *behavior*
(catcode-like schemes, non-textual sources) implements the trait. `peek` deliberately
receives `&ParsingState<L>`, not `&TokenRules` — a catcode-like reader keeps its tables in
`L::StateExt` (§3.11). The peek/move_past/move_to protocol with `skip_post_space` /
`rewind_pre_space` flags follows pylatexenc's proven `LatexTokenReaderBase` design; the
flags are not vestigial (`\verb`-style parsers reposition before swallowed post-space).
**Peek idempotence contract:** repeated peeks at one position with the *same state
instance* return the same result; implementations may memoize keyed on (position, `Arc`
identity) — sound because states are immutable and `derived()` always mints a new `Arc`. A
different state, however trivially derived, voids the obligation. (`StdTokenReader` does
not memoize yet — no premature optimization; the contract permits it.)

**Ambiguous group delimiters resolved by data: `expecting_group_close`** — DECIDED
(Phase 2, upheld; now read from the state's cached table). `$…$`-style group types make one
string both opener and closer (and `$$` vs `$` overlap); pylatexenc resolves this with
privileged math-mode state (`in_math_mode` + `math_mode_delimiter` checked before
longest-match). De-privileged into plain data: `TokenRules::expecting_group_close:
Option<GroupTypeId>` names the group type whose *close* delimiter takes precedence over all
other matches; a group construct parser sets it (via a state delta) when entering an
ambiguously-delimited group. Otherwise the longest `PrefixTable` match wins, read as an
*open* when the string is ambiguous — and a close-only string tokenizes as `GroupClose`
even where syntactically wrong ("it's not the tokenizer's job to report syntax errors",
pylatexenc). **Priority order overall:** paragraph break → expected group close → longest
delimiter → command escapes → comment starts → specials scan → forbidden check → `Char`.
Groups precede commands so escape-led delimiters like `\(` win over command
interpretation; comments precede the specials scan, so a trigger string starting with a
comment delimiter is shadowed (deliberate: deterministic rules data wins over hook
behavior). Reproduces pylatexenc's `$\zeta$$\gamma$` / `$$…$$` behaviors exactly (ported
tests).
*(Amended July 2026, group-classes session: the field now holds the expected
`Arc<GroupRule<L>>` itself rather than a `GroupTypeId` — a class cannot name a pairing once
`GroupTypeId` means class (next entry); the tokenizer compares the rule's close string
directly, which also deleted the id→rule lookup from the hot path.)*

**Group classes detached from delimiters: `Lang::GroupTypeId` = class, `GroupRule` = runtime
delimiter data, tokens carry the resolved rule** — DECIDED (user, July 2026, group-classes
session; reframes the closed-ids decision of §3.4). A group *type* is a language-native class
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
*Rationale:* per-pairing identities in a closed enum blocked exactly the extensibility the
delimited-group machinery exists for — a third-party spec cannot add variants to the preset's
enum, so novel delimiters (beamer-style `<…>` overlay arguments, `|…|` forms) were
unrepresentable, and even the preset's own optional-argument parser had to pre-register its
`[…]` pairing. Meanwhile pairing identity never distinguished anything the strings didn't.
The class keeps the typed "is this a math group?" answer (no string comparison) and makes
parse-time behavior data-driven — "entering this group enters math mode" is one class check,
where per-spelling variants (`DollarInline`, `ParenMath`, …) scattered it.
*Rejected:* removing `GroupTypeId` entirely (loses typed classification; would have reversed
§3.5's "delimiters-only degenerates to string comparison" rejection); keeping per-pairing ids
with runtime allocation (recreates the deleted interned-id registry).
*Revisit if:* per-instance group data beyond class + spellings is needed — that is
`GroupNodeExt`'s job, not more id structure.

**`TokenKind::Command` records its escape character** — DECIDED (user, July 2026, Phase 6
plan session; Phase 6 notes item C2). `Command { name, escape_char: char, post_space }`.
*Rationale:* §3.4's lookup design requires `CallableQuery { syntax: Command { escape_char } }`,
the escape char was not recoverable from the token, and the nodes parser must not reach
around the reader into raw content (§3.2, `EndOfStream` rationale). The tokenizer knows
which `CommandRule` fired; recording it is syntactic fact (which rule fired), not resolution
output — consistent with the no-`CallableTypeId`-on-tokens line. Small ripple through the
Phase 3 token tests, accepted.

**Token-layer contract hardening (Action 02)** — DECIDED (user, July 2026, Action-02 review
session). Six decisions closing contract gaps ahead of third-party `TokenReader`/`Lang`
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
   parse keeps the partition invariant — consistent with §3.8's recovery principle (markup
   text in a `Chars` node, always with a diagnostic) and with the other content-preserving
   recoveries. *Rejected:* the empty `EndOfStream` placeholder (pylatexenc parity) — it
   claimed no bytes while reading resumed past the escape, so the root children did not tile
   the content; the placeholder-vs-drop tradeoff had never actually been weighed when §3.2's
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
   is thereby parity-by-doctrine, pinned by a test. *Rejected:* moving that `'\r'` into
   comment post-space when declared whitespace (briefly implemented) — special-casing
   one legacy line-ending convention inside the scanning core.

**`TokenListReader` demoted to internal test infrastructure** — DECIDED (user, July 2026,
Action-02 follow-up). Compiled under `cfg(test)` only, `pub(crate)`, removed from the
public exports. Every consumer is an in-crate test; its load-bearing role is the lockstep
reader-agreement harness (each construct-parser suite runs every parse against
`StdTokenReader` *and* a pre-scanned `TokenListReader` and asserts identical trees, stops,
and diagnostics — the enforcement mechanism for "construct parsers never reach around the
reader"), plus hand-built token lists for engine tests. Its fixed-list fidelity gap — no
re-tokenization under the peek state, so state-driven parsers like the verbatim recipe
cannot run over it — is fine for a test tool but disqualifies it as a public reader
contract. *Rejected:* deleting it outright (loses the lockstep verification); keeping it
public (a maintained API surface nothing external needs).

**`TokenRules::multi_newline_paragraphs` (renamed from `double_newline_paragraphs`)** —
DECIDED (user, July 2026, Phase 6 plan session). Any run of two or more newlines (however
many, with interleaved inline whitespace) forms one paragraph break; the old name misread as
"exactly two". *(Superseded naming: joined the `enable_*` family as
`enable_multi_newline_paragraphs` — see the flags entry below.)*

**`enable_*` feature flags on `TokenRules`** — DECIDED (user, July 2026, child-state design
session follow-up; pylatexenc's `enable_macros`/`enable_comments`/… pattern). Every major
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
(preserves §3.6's termination guarantee structurally). (2) **`enable_specials` settles the
disable-specials gap (former open question §6.6)**: specials *data* stays Lang/library
business, but the gate is rules data — `freeze()` skips `Lang::specials_trigger_chars`
entirely and stores the empty `TriggerChars` filter, so the scan hook is unreachable in
disabled states. (3) Flags bake into the eager per-state caches where possible (empty
`PrefixTable` under `enable_groups: false`, empty `TriggerChars` under
`enable_specials: false`) — zero hot-path cost; the rest are single bool branches replacing
former `Option` checks. (4) `forbidden_chars` deliberately gets **no** flag (one trivially
restorable string, not a feature toggle with a demonstrated scoped-off need);
`expecting_group_close` is positional data, not a feature.
*Rationale:* the `ChildStateSpec` restricted-state use cases (§3.6) need scoped, losslessly
reversible feature disabling, and field-wise wholesale replacement can express "off" but
not "off, remembering what on meant".
*Rejected:* keeping `Option<WhitespaceRules>` alongside the flag (three states, two meaning
"off"); `enable_forbidden_chars` (uniformity for its own sake).

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
(user-led, July 2026; ARCHITECTURE.md §state/§4, Decision 1 RESOLVED).
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
Implementation notes (Phase 3, July 2026): the derived caches (`PrefixTable`,
`TriggerChars`) are built **eagerly** when a state is frozen, not `OnceLock`-lazily as the
ARCHITECTURE sketch had it — the crate is `no_std` (`core` has no `OnceLock`; `OnceCell`
would cost `Sync`). Eager rebuilds turned out to be a real fraction of a transition's cost
(the July 2026 performance review), so `derived()` reuses the parent's `PrefixTable`
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

**Token-level language hooks live on `Lang`, next to `finalize_transition`** — DECIDED
(July 2026, token-design review). `Lang::scan_specials` and `Lang::specials_trigger_chars`
follow the same pattern as the transition customizer: static hooks with working defaults,
receiving the state (or, for the trigger-chars derivation, the `StateData` mid-freeze).
*Rationale:* the hook receives the state and can therefore dispatch on `ext` and pushed
libraries — everything a swappable scanner object stored in `StateData` could express,
without adding a state field, dyn indirection, and a delta story for swapping it.
*Rejected:* an `Arc<dyn SpecialsScan>` field in `StateData`; per-library trigger
declarations with core-legislated cross-library shadowing (see §3.2 — the preset owns its
scan; the core legislates nothing about trigger precedence).

**Seed states are crate-frozen `Lang` data: `ParsingState::initial()` +
`Lang::initial_state_data()`** — DECIDED (user, July 2026, code-review follow-up
session; closes the seed hole flagged by the state review).
`ParsingState::new(data)` was `pub`, so any caller could assemble a state that never
passed `finalize_transition` — the one hole in "airtightness is structural". Now the
language provides its canonical seed as *data* (`Lang::initial_state_data() ->
StateData<Self>`, default: every syntax gate off, no libraries, default ext), and the
*crate* owns the data→state freeze (`ParsingState::initial()`); `new()` is
`#[cfg(test)] pub(crate)`. Callers customize the starting point via `derived(delta)` —
which runs finalize — never by assembling a state.
*Rationale:* the hook returns `StateData`, not `ParsingState`, precisely so out-of-crate
presets can implement it while the freeze stays crate-owned. `finalize_transition` still
does not run on the seed (no `prev` exists), but the obligation shrinks from "any caller
anywhere" to "the `Lang` author's own canonical seed must be coherent" — author-local,
documented on the hook, and mechanically pinnable by asserting
`initial().derived(&empty)` is data-equivalent to `initial()`.
*Rejected:* a separate `Lang::finalize_initial(&mut StateData)` hook (two hooks to keep
consistent — the same forgettability the hole had); changing `finalize_transition` to
`prev: Option<&ParsingState>` (taxes every transition-reactive implementor with a `None`
arm that pure normalizers never need); a `Default for TokenRules` to back the hook's
default body (rules.rs deliberately implements no `Default` — the neutral all-off rules
are constructed inline in the hook instead).
*Deferred (user, same session):* generic seed-side registration of `LibraryStack`
*fallbacks* is still inexpressible by delta (a `Lang` author bakes fallbacks into
`initial_state_data`; a *user* of a preset cannot add their own). Resolution folded into
the planned LibraryStack revisit — deltas should become much more expressive about
library manipulation, possibly whole-library replacement in a transition (§6 open
question 7).
*Revisit if:* the LibraryStack revisit lands (the delta story may then subsume parts of
the seed contract), or a preset needs `finalize_transition`-grade normalization on the
seed itself — the `derived(&empty)`-at-seed trick (one extra freeze at session start)
is the cheap mechanical option before any signature change.

**Parsing mode is first-class state data: `StateData.mode: L::ModeId`** — DECIDED (user,
July 2026, Phase 7 plan session; settles ParserLibraryParity.md N1 jointly with the
`ParseDriver` entry, §3.6).
`Lang` gains `type ModeId` (`Copy + Eq + Debug + Send + Sync`; `()` under `SimpleLang`) —
the third closed per-language vocabulary after `GroupTypeId`/`CallableTypeId` — stored as
a plain field on `StateData` with a matching `ParsingStateDelta.mode: Option<L::ModeId>`
override channel. Mode is deliberately not lookup-private: the scope stack reads it for
package visibility (§3.4), and a preset may key any content-interpretation decision on it
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
*Rejected:* computing the mode at freeze from `ext` (a hidden derivation for what is
honestly plain data); an interior delta or events payload on `GroupRule` (the N1
data-first candidate — `GroupRule` feeds elementwise prefix-table comparisons and derives
`Eq`, which a delta payload breaks; and cross-rule policy would smear across rule
definitions instead of centralizing in finalize).
*Revisit if:* a language needs several orthogonal mode axes at once (composite enums
cover the known cases; if they explode, mode may need to become a small struct).
*Landed (subphase 7.1, July 2026)* with two bound additions forced in flight: `ModeId:
… + Hash + Default`. `Hash` because the mode override joins the session derivation-memo
key — keyed *by value* (exact — modes are `Copy + Eq` vocabulary), unlike the
identity-keyed rule payloads, so mode-bearing descent deltas stay memoizable (the D1/7.2
math plug depends on this; `GroupTypeId`/`CallableTypeId` carry `Hash` for the same
map-key reason). `Default` supplies the seed's mode in the default
`initial_state_data()` (the exact precedent of `StateExt: Default`); a real language's
`#[default]` variant names its canonical initial mode. The memoizable-delta *gate* is
unchanged (no ext/events/pushes); `ParsingState::mode()` returns by value.

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

**`SpecLookup` receives a `CallableQuery` (query struct), not bare `(ct, name)`** — DECIDED
(July 2026, Phase 4 design session; closes the deferred half of DECISION 6 / open question
§6.1). `lookup(&CallableQuery, &ParsingState<L>) -> Option<Arc<dyn CallableSpec<L>>>`, where
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
*Rejected:* bare `(ct, name, state)` (forces presets to multiply `CallableTypeId`s to encode
syntax); a mandatory `&Token` parameter (lifetime noise on a dyn trait, and inconsistent —
sometimes there is no token).
*Mode-awareness*, as proposed: the `&ParsingState<L>` parameter lets a preset's lookup dispatch
on `state.ext()` (FLM's `\vec` in math mode); the core `Library` ignores state, syntax, and
token alike. This replaces PROPOSALS.md's hard-coded `math_mode_macros` tables, which
contradicted §2.3.
*(Amended July 2026, Phase 7 plan session: the lookup contract rehomes to
`SpecsProvider::retrieve_spec` — fallible, part of a richer provider trait — with
`CallableQuery` and its rationale carried over unchanged; see the scope-stack redesign
entry below.)*

**Unknown-callable fallbacks are built into `LibraryStack`; its own `SpecLookup` impl is
stack-only** — DECIDED (July 2026, Phase 4 design session; open question §6.1(b)).
The per-`CallableTypeId` fallback singletons live in a map on `LibraryStack`, consulted only by
`resolve()` after the whole stack misses (`lookup()` never consults them). De-keyed specs make
the singletons shareable — "unknown `\foo`" costs no per-instance allocation, and a callable
node's spec is never `None` for any type whose preset registered a fallback (`resolve()` still
returns `Option` for types without one).
*Pitfall that fixed the shape:* `LibraryStack` implements `SpecLookup` for nesting, but with
**stack-only** semantics — if a nested stack's fallbacks answered, an inner fallback would
preempt an *outer* stack's real definitions. Fallback policy belongs to the outermost resolver
alone.
*Storage note:* `Library` keys are nested `BTreeMap`s — the crate is `no_std` and `alloc` has
no `HashMap`; also sidesteps the tuple-key `Borrow` problem. Revisit only if profiling flags
lookup cost.
*(Superseded July 2026, Phase 7 plan session: fallbacks become ordinary bottom-of-stack
providers, and the stack no longer nests as a provider at all — the preemption hazard is
removed rather than mitigated. See the scope-stack redesign entry below.)*

**Phase 4 ships structure-spec skeletons; `invocation_parser()` waits for Phase 6** — DECIDED
(July 2026, Phase 4 design session).
`ArgumentStructureSpec`/`SlotStructureSpec` exist so `CallableSpec` has its declarative surface
and libraries hold real specs, but deliberately minimal: starter `ArgumentKind`
(`Mandatory`/`Optional` by `GroupTypeId`, `Star` marker), name-only `SlotSpec`. The argument
kinds, acceptance semantics (LaTeX's single-token mandatory args), and slot
separators/terminators (invocation-name back-reference) are pinned down in Phase 6, where
`ArgumentsParser`/`SlotsParser` make the requirements concrete — same stub-bridging pattern as
Phase 3's `CallableSpec` declaration. `invocation_parser()` (and the custom-parser override on
`StdCallableSpec`) also waits for Phase 6's `ConstructParser`/`ParseContext`/`NodeId` rather
than inventing a throwaway signature. `CallableSpec`'s default `arguments()`/`slots()` return
the *neutral callable* (no arguments, no slots) — the semantically correct default for fallback
singletons and simple specials like `~`, not an arbitrary one.
*(Amended July 2026, current-level review: the skeleton `ArgumentKind` and the
`ArgumentStructureSpec`/`SlotStructureSpec` wrappers are superseded by the pylatexenc-shaped
argument model — see the two entries below. The neutral-callable defaults and the Phase 6
deferral of `parse_invocation()` stand.)*

**Argument model rebuilt on pylatexenc's `LatexArgumentSpec`: an argument *is* a parser** —
DECIDED (user, July 2026, current-level review session; implements report
2026-07-05 §5.1/§5.2, R1/R2). `ArgumentSpec<L>` = `{ parser: ArgumentParserSpec<L>,
name: Option<Box<str>>, parsing_state_delta: Option<ParsingStateDelta<L>> }`.
`ArgumentParserSpec` keeps the standard delimited forms as *data* variants (`Group` — with
LaTeX's single-expression fallback acceptance, Phase 6 notes Q3 Option A —, `OptionalGroup`,
`Marker`) and adds `Custom(Arc<dyn ArgumentParser<L>>)` as the mid-granularity behavior
escape hatch (chars-only args, comma lists, verbatim args, FLM argument types — without
taking over the whole invocation). `SlotSpec<L>` likewise gains `name` +
`parsing_state_delta` (pylatexenc's `make_body_parsing_state_delta`: verbatim/math bodies).
The `ArgumentParser` trait is a reserved marker until Phase 6 supplies `ParseContext` for
its parse method.
*Rationale:* pylatexenc's whole argument ecosystem hangs off this slot, and "just write a
custom invocation parser" is the expensive path the declarative surface exists to avoid; the
hybrid (data variants + `Custom`) keeps introspection/recomposition-by-data for the common
forms where pylatexenc's parser-objects-everywhere loses them.
*Costs accepted:* spec types become generic over `L`; no `PartialEq` on spec types (dyn
parser, state delta) — consistent with node types.
*Rejected:* closed `ArgumentKind` enum (a closed *architecture*, not just a closed starter
inventory — real regression vs. pylatexenc); every-argument-is-an-opaque-parser
(pylatexenc-pure — loses declarative introspection).
*Revisit if:* the data-variant inventory grows unwieldy (fold variants into shipped standard
`ArgumentParser` impls instead).
*(Amended July 2026, group-classes session: the *Revisit if* fired early and fully — the data
variants are gone; `ArgumentSpec.parser` is `Arc<dyn ArgumentParser<L>>`, pylatexenc's
parser-objects-everywhere, with the terse `group`/`optional_group`/`marker` constructors
removed. Once `GroupTypeId` became a delimiter-detached class (§3.2), the core could no
longer name "the `{…}` argument" in a data variant or constructor — which group class, whose
spelling? The standard forms become preset-provided `ArgumentParser` impls, pylatexenc's own
resolution of the `'{'`/`'['`/`'*'` shorthands into parser instances. The introspection
argument for data variants turned out not to be load-bearing: recomposition reads nodes and
layouts — delimiters and marker spellings stored as `TextContent` per §3.5 — never specs. The
prior *Rejected:* "every-argument-is-an-opaque-parser" is thereby deliberately reversed.)*

**`ArgumentStructureSpec`/`SlotStructureSpec` wrappers dropped; `CallableSpec` exposes
`&[Arc<ArgumentSpec<L>>]` / `&[Arc<SlotSpec<L>>]`** — DECIDED (July 2026, same session).
The elements are `Arc`-shared so parsed nodes can record which spec each argument was parsed
against (see §3.5), mirroring pylatexenc's `arguments_spec_list`. Empty-slice defaults work
for generic `L` where the former `static NONE: ArgumentStructureSpec` cannot (no generic
statics; `Vec` is not const-promotable).
*Revisit if:* structure-level (not per-item) spec fields materialize in Phase 6 — e.g. a
slot-separator field that belongs to no single slot; then a wrapper returns.
*(Amended July 2026, slots session: only the arguments slice remains — `SlotSpec` and
`CallableSpec::slots()` are deleted; slots are record-level vocabulary. See the
no-spec-side-slots entry, §3.6.)*

**`CallableTypeId` and `GroupTypeId` are closed per-`Lang` associated types** — DECIDED
(user, July 2026, current-level review session; replaces the open interned-id registry
design). `Lang::CallableTypeId: Copy + Ord + Hash + Debug` (Ord: library map keys),
`Lang::GroupTypeId: Copy + Eq + Hash + Debug`; a real language defines small enums,
`SimpleLang` defaults both to `u32`. The planned `Language<L>` interning machinery for these
ids is deleted from Phase 6 scope.
*Rationale:* invocation forms and group-type identities are static per language definition —
nobody registers a new *form* at runtime (new *callables*, yes — via libraries; new
*delimiter spellings*, yes — `GroupType` values in the state's token rules; only the
identity vocabulary is fixed). Closed enums give exhaustive matching in preset code, make
cross-language id mixing a type error, and remove meaningless raw `u32`s ("open IDs floating
around").
*Rejected:* keeping the open ids for symmetry — the symmetry was spurious: token *rules* are
runtime state; type *identities* are not.
*Revisit if:* a genuine runtime-registration need for group/callable types appears (e.g.
catcode-style schemes minting new group types mid-parse) — then that language can use an
integer id type; the associated-type design accommodates it without core changes.
*(Amended July 2026, group-classes session: the *Revisit if* fired for groups — construct
parsers do mint delimiter pairs mid-parse (optional arguments, custom specs). Resolved not by
opening the id space (the rejected registry) but by detaching the closed vocabulary from
spellings: `GroupTypeId` reframed from per-pairing identity to group *class* — see the §3.2
group-classes entry. `CallableTypeId` is untouched; both remain closed per-`Lang`.)*
*(Amended July 2026, thread-safety session: both id types' bounds gained `+ Send + Sync` —
see the thread-safety entry below.)*

**Thread safety is a core contract: `Send + Sync` supertraits on the dyn spec traits** —
DECIDED (user + Claude, July 2026, thread-safety session).
`CallableSpec`, `SpecLookup`, and `ArgumentParser` carry `Send + Sync` supertraits; the
bounds this forces propagate to `Lang`'s associated types (`GroupTypeId`, `CallableTypeId`,
`StateExt`, `Event`), all seven `NodeExtTypes` types, and the `SourceOrigin` trait. Result:
`NodeTree`, `ParsingState`, `Token`, deltas, and every spec handle are `Send + Sync` — parse
on one thread and hand the tree off; share preset libraries across parallel parses.
*Rationale:* `Arc<T>: Send` needs `T: Send + Sync`, and Send-ness is erased at the trait
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
atomic refcounts for zero capability — and completes §3.3's "OnceCell would make states
non-`Sync`" intent. Contrast pylatexenc: Python's GIL made shared mutable spec state a
non-issue; ports of such specs use locks.
*Rejected:* a `sync` cargo feature gating the supertraits (or `Arc` vs `Rc`) — cargo
features are additive and unified across the dependency graph, so a contract-changing
feature forks the extension ecosystem into two incompatible dialects: extension crates must
pick a side, and one crate enabling it imposes it on all (`im`/`im-rc` shipped as *separate
crates* for exactly this reason; rhai's `sync` feature is the cautionary precedent).
Mechanically it also needs duplicated trait definitions or a `MaybeSendSync` helper trait,
plus double CI and docs. Spelling `Arc<dyn … + Send + Sync>` at use sites — same effective
constraint for anything stored in a tree, but two distinct erased types and the
`Box<dyn Error + Send + Sync>` spelling plague.
*Revisit if:* a compelling single-threaded embedder materializes — then a parallel
`Rc`-based local layer can be added *without* breaking the `Send` world (rowan's `Send`
green tree / deliberately-`!Send` red cursors precedent); the reverse migration (adding
bounds later) would break implementors holding non-`Send` state, which is why the bounds
land now while the API is fluid.

**`CallableSpec: Any` — downcasting is part of the spec contract; `Lang: 'static`** —
DECIDED (user, July 2026, Action-05 session). The documented preset pattern
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
to every hand-written `Lang` impl (the SimpleLang-cliff cost, §3.6/H.2) for a need the
wrapper covers; recorded here as the upgrade path if the wrapper proves annoying in
FLM practice. This also unblocks the flagged default-factory escape hatch (§3.6): the
dispatch loop *can* now detect `StdCallableSpec` and elide the per-invocation `Box`, if
profiling ever asks for it.

**Scope-stack redesign: dyn `SpecsProvider` entries, `Package`/`Scope` standard impls,
in-stack fallbacks** — DECIDED (user, July 2026, Phase 7 plan session; closes §6 open
question 7 and supersedes the `SpecLookup`/`LibraryStack` entries above).
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
  callable types). The stack carries no fallback map, and **no longer implements the
  provider contract itself** — stacks don't nest, which *removes* the Phase 4
  nested-fallback-preemption hazard instead of re-mitigating it. Exhausting the stack is
  a structured miss carrying the searched provider names (feeding the
  `UnresolvableCommand` "searched: …" detail).
- **No `Masked` outcome** (user): "undefined on purpose" is an ordinary definition — an
  `ErrorCallableSpec` whose invocation parser diagnoses, with a better message than a
  mask could carry. Shadowing with it suppresses lower entries *and* the fallback purely
  by search order (a theorem of ordering, not an extra rule). `Remove` genuinely deletes,
  from `Scope`s only.
*Rejected:* evicting definitions from core entirely ("skeletal" — core already owns
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
*Consequences to settle at implementation (Phase 7.3 checkpoints):* `derived()` likely
becomes fallible (delta ops can fail: non-mutable target, absent provider name); the
specials fold rule across providers (lean: longest match wins, ties innermost —
preserves pylatexenc's `---`-beats-`--`); `ProviderError`/miss-report shapes.
*Revisit if:* per-definition mode visibility is needed beyond what custom providers
cover, or provider-fold resolution cost shows up in profiles (a freeze-time merged map
à la `PrefixTable` is the prepared answer).
*(Landed July 2026, Phase 7.3 — checkpoint session resolutions:)*
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
*(d)* `iter_symbols` is deferred to the 7.8 view-API session (adding a defaulted trait
method later is non-breaking; 7.8's consumers will shape the item type). *(Landed in
7.8 — the `iter_symbols` entry at the end of this section.)*
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

**`iter_symbols`: definition enumeration with a required type filter; `ClosedVocabulary`
supplies the type universe** — DECIDED (user, July 2026, 7.8 checkpoint session).
Defaulted `SpecsProvider::iter_symbols(callable_type: L::CallableTypeId, mode:
L::ModeId) -> Option<Box<dyn Iterator<Item = SymbolEntry<'_, L>>>>` — the enumeration
counterpart of `retrieve_spec`'s point queries, closing the 7.3 deferral. Key points:
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
*Rejected:* an `Option`al type filter (generic listing without the vocabulary bound —
user preferred always-filtered plus statically listable vocabularies); a
`&ParsingState` parameter (nothing beyond the mode feeds visibility); state-blind
enumeration with visibility data carried on entries (information without a consumer);
excluding specials or a separate `iter_specials` (the recorded-type framing unifies
the tables with no extra surface).

### 3.5 Nodes and AST

**Flat `NodeTree` (Vec + index ranges), frozen after parse, `NodeRef` proxy access** — DECIDED
(March 2026). Cache-friendly, no per-node heap allocation, trivially serializable; `NodeRef`
(Copy, borrows `ParseResult`) makes indices safe by construction — the borrow checker
guarantees a `NodeRef` can't outlive the tree its index points into. Mutation happens only
inside `ParserSession`; `finish()` consumes the session, so there is no mutable/immutable
conflict by design.

**Closed `NodeKind<L>`, unified `Callable` kind, two-tier ext, no `Custom` variant
("Option F")** — DECIDED (ARCHITECTURE.md Decision 3, July 2026; implemented July 2026,
Phase 5). The structural taxonomy is `Chars`/`Group`/`Callable`/`Comment`/`List`;
macro/environment/specials are invocation *forms* (`CallableTypeId` on `CallableData`), not
node kinds; custom data rides in the two-tier ext bundle (`Lang::NodeExts: NodeExtTypes` —
uniform `NodeExt` + per-kind `<Kind>NodeExt`, all bounded `Clone + Debug + Default`; the
`Default` gives builders their no-ext value, mirroring `StateExt`). `NodeExtTypes` is defined
next to `Lang` in the state topic, not in `node/` (moving it would recreate a module cycle for
cosmetics); `SimpleLang` + blanket impl provides the all-defaults shortcut. The full
resolution argument (why `Custom` died, de-keyed specs, owned names, `TextContent`) is
recorded in ARCHITECTURE.md §4b and is not duplicated here.
*Rejected:* `trait Node` + `Box<dyn Node>` + `as_any()` downcasting + `clone_box()` (the
generated TRAIT_BASED_ARCHITECTURE.md design) — loses exhaustive matching, adds per-node
boxing, makes serialization and flat storage impossible, and reintroduces runtime type errors
that the type system should prevent.

**No core `MathNode`** — DECIDED (July 2026, consequence of §2.3 and Decision 3).
`$…$` parses as a `Group` with a `$`-delimited `GroupTypeId` under a math-mode state extension;
the latexlike preset provides accessor helpers so ergonomics don't suffer.
*Revisit if:* preset-level ergonomics prove genuinely painful in practice — the fallback is
preset-defined ext data on the `Group` kind, still not a core variant.

**Args/slots ↔ children encoding: one node per region** — DECIDED (user, July 2026, Phase 5
design session; was open question §6.2). A `Callable` node's children range holds one node per
*present* argument (the argument's natural node — typically a `Group`; a `List` wrapper only if
an argument kind ever yields several nodes), followed by one `List` node per slot (an
environment body = one `List` child; an empty body is an empty `List` — a region that exists,
unlike an absent optional argument). `ArgsLayout` maps spec-argument index →
`Absent` / `Present { child offset }` / `Marker { spelling }`; `SlotsLayout` maps slot → child
offset. Per-instance syntax choices the spec doesn't determine (today the marker spelling;
delimiter alternatives, verbatim fences, and slot separators arrive with Phase 6's parsers)
are recorded **in the layouts as `TextContent`**, not in ext types — the level-2 recomposition
requirement must not depend on `Lang` cooperation.
*Rationale:* matches pylatexenc's proven argnlist shape (one node per argument, `None` when
absent); every region has node identity and its own span ("what is the span of `\frac`'s 2nd
argument" is answerable); generic child traversal visits meaningful units; layouts stay small.
*Rejected:* flattening region contents directly into the children range with `(offset, len)`
layout entries — regions lose node identity (no span, no ext attachment point) and visitors
see argument content and body content indistinguishably mixed; separate `Vec<NodeId>` lists
inside `CallableData` — duplicates the children mechanism, exempts callables from the
flat-tree contiguity invariant, and costs per-callable allocations.
*(Amended July 2026, current-level review: the record types `ArgsLayout`/`SlotsLayout` are
superseded by `ParsedArguments`/`ParsedSlots` — see below. The one-node-per-region encoding
itself stands, with one change: provided markers are ordinary `Chars` child nodes.)*
*(Amended again July 2026, regions session: the one-node-per-argument encoding is superseded
by **one child region per argument/slot** — see the `ChildRegion` entry below. One node per
argument gave inter-argument comments no home (`\frac % half⏎{1}{2}`: `pre_space` held only
whitespace), breaking the partition invariant. The span-answerability rationale carries over
(region span = first..last region node); "generic child traversal visits meaningful units" is
deliberately weakened — a callable's child list is now the raw-syntax view, and semantic
access goes through the records.)*

**`ParsedArguments`/`ParsedSlots` replace `ArgsLayout`/`SlotsLayout` (pylatexenc's
`ParsedArguments` pattern)** — DECIDED (user, July 2026, current-level review session).
`ParsedArguments<L>` holds one `ParsedArgument<L>` per spec'd argument:
`{ spec: Arc<ArgumentSpec<L>>, child: Option<u32>, pre_space: TextContent,
ext: ArgumentExt<L> }`; `ParsedSlots<L>` holds `ParsedSlot<L> { spec: Arc<SlotSpec<L>>,
child: u32 }`. Key points, each argued in the session:
- **Self-describing records.** Every entry carries the `Arc`'d spec it was parsed against —
  pylatexenc keeps `arguments_spec_list` next to `argnlist` for exactly this: a custom
  invocation parser may produce an argument structure the callable spec didn't declare
  (`\newcommand`-alikes), and the record must stand alone.
- **Presence lives *inside* the entry** (`child: Option<u32>`), not as
  `Vec<Option<ParsedArgument>>`: absent optionals keep their spec, so by-name lookup
  distinguishes "not provided" from "no such argument". This zips pylatexenc's two parallel
  lists into one array-of-structs; the user's sketched `Vec<Option<…>>` shape is preserved
  one level down.
- **Provided markers are `Chars` nodes** (pylatexenc's `LatexOptionalCharsMarkerParser`
  returns a chars node for `*`): every provided argument has a node, and the three-way
  `Absent`/`Present`/`Marker` layout enum disappears.
- **No stored name→index map**: lookup scans the entries' spec names (argument counts are
  tiny; the specs are the single source of truth). Add a cache only if profiling ever says
  so.
- **Content access is computed, not stored** (pylatexenc's `get_content_nodelist()` /
  `get_content_as_chars()` are accessors): the group node's children *are* the content, and
  stored copies would diverge under transforms. The extraction-view API is the Phase 7 work
  package (report R7). What *is* stored: the new `ArgumentExt` slot in the
  `Lang::NodeExts` bundle, for extensions caching derived data per argument (e.g.
  `{ref_domain, ref_key}` from a `fig:Abc` argument) — populated by custom argument parsers
  or the Phase 6 finalize hook (report R3).
- **Per-instance syntax records** (Q3 Option A): `pre_space` per argument now; slot
  terminator/separator records arrive with `SlotsParser` (Phase 6).
*Rejected:* parallel `specs`/`args` vectors (pylatexenc-literal — an unenforced
length/pointer-consistency invariant and a redundant `Arc` when the spec also sits in the
entry); "layout" as a name (opaque — nobody could say what it referred to).
*Revisit if:* an argument form that is "provided" yet produces no node appears — then
presence needs a flag separate from `child`.
*(Amended July 2026, regions session: `child: Option<u32>` and `pre_space` are replaced by
`region: Option<ChildRegion>` — next entry. The revisit clause above is thereby resolved:
presence is `Option`-ness of the region, no longer tied to node existence (an empty region
is representable). The self-describing-records, presence-inside-the-entry,
markers-as-`Chars`, and no-name-map points stand. The "content access is computed, not
stored" point is refined: extraction conveniences stay computed, but *which nodes are
content* is now recorded per argument — parser-designated, eliminating pylatexenc's
lone-group unwrap heuristics.)*

**`SlotExt` — slot records carry per-instance ext, symmetric with `ArgumentExt`** — DECIDED
(user, July 2026, Action-05 session). `ParsedSlot` gains `ext: SlotExt<L>`
(`Lang::NodeExts::SlotExt`, `()` under the no-ext bundle), mirroring
`ParsedArgument.ext`. Rationale: the asymmetry bit exactly where FLM is richest — an
environment's *body* is a slot, and per-instance derived data about a body (tabular cell
structure, enumerate item boundaries) had no home except the whole-callable ext. Added
while cheap: one associated type on the bundle, one field on the record; retrofitting after
downstream `NodeExtTypes` implementors exist would break them all.

**`NodeTree::iter` renamed `iter_storage_order`; no `parent` stored in `NodeData`** —
DECIDED (user, July 2026, Action-05 session). The flat iterator yields storage
(breadth-first) order — `a`, `c`, `b` for `a{b}c` — which a name as generic as `iter`
invites consumers to mistake for document order; the rename makes the iteration order
part of the signature. A document-order `descendants()` arrives only with the Phase 7
read API, when it has a consumer. Upward navigation (`parent: u32` in `NodeData`,
`parent()`/`next_sibling()`/`ancestors()`) was considered and declined as not needed —
the transient parent vector `finish()` computes for region resolution stays transient.
Named argument-node accessors (`argument_nodes_named` etc.) are deferred to the Phase 7
pylatexenc-style argument-access package rather than added piecemeal.

**Argument/slot child *regions* with parser-designated content, resolved to global node
ranges by the builder (`ChildRegion`, `ContentNodes`)** — DECIDED (user, July 2026, regions
session; supersedes one-node-per-argument and `pre_space`). A callable's children range is
the concatenation of one contiguous **region** per provided argument, then one per slot. A
region holds the argument's full syntactic extent in source order: leading noise (comment
nodes and whitespace-only `Chars` nodes — `pre_space` is deleted; whitespace before an
argument is a node like everywhere else, matching the D1/D4 whitespace-as-chars rule and
pylatexenc's expression parser), the syntax-bearing node(s) (a `Group` for `{…}`/`[…]` with
delimiters on `GroupData`; a `Chars` node for `\frac 1 2` single tokens and `*` markers,
which **count as content** — pylatexenc parity), and any trailing per-instance syntax.
Records: `ParsedArgument { spec, region: Option<ChildRegion>, ext }` and
`ParsedSlot { spec, region: ChildRegion }` *(slots session, July 2026: the slot record's
`spec` is now `name: Option<Box<str>>` — see the no-spec-side-slots entry, §3.6)*; a
resolved `ChildRegion` =
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
  under the argument's own parsing-state delta. Standard parsers share one noise-scan helper
  (Phase 6); no noise knobs on `ArgumentSpec`. **Absent means zero consumption**: noise
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
  can't see — the same genus as the rejected Q2-Option-B set-before-use field protocol —
  accepted here because resolution happens in a single component at a single point, finished
  trees cannot contain staged regions, and the resolved-only accessors panic on staged
  records (a caller bug under the builder's panic policy). Bought with it: parsers build
  `ParsedArguments`/`ParsedSlots` directly and `add()` keeps its signature — no bespoke
  staging API.
- **Builder checks:** hard asserts at `add()` (regions staged / in bounds / ordered /
  non-overlapping; designation sub-ranges within their parent's child list) and at
  `finish()` (content parent reachable and inside its own region's subtree — only checkable
  once the layout exists); debug-assert that regions **tile** the child list exactly (the
  §nodes partition invariant, mechanically checkable).
- **Consequences accepted:** a callable's child list is the raw-syntax view (child count ≠
  argument count; `\frac 1 2` costs two whitespace `Chars` nodes); an argument has no single
  node identity — transforms and views splice child *ranges* (Phase 7 view API);
  `NodeRef::argument(i)`/`argument_named()` are replaced by region/content-nodes accessors;
  `ParsedArguments` holds no `TextContent`, so its materialization plumbing is deleted.
  `CallableData.post_space` deliberately stays a field: it lies outside the region tiling
  and is whitespace-only by construction (trailing comments are never consumed).
- **Slots mirror arguments** (same `ChildRegion` type), keeping the body `List` as the
  content parent (span/state/ext identity; "an empty body exists"); whether terminator
  syntax (`\end{align}`) becomes region nodes or spec-driven records stays open with Q1.
*Rejected:* centralized noise scanning (breaks verbatim-delimiter arguments; scans under the
wrong state); noise as `TextContent` blobs (comments lose node identity — invisible to
visitors and transforms); a wrapper `List` node per argument (extra node, unnatural shape);
`content_child: u32` marking a single node (can't express content inside groups, multi-node
content, or trailing syntax); a *relative* `content_range` (child offsets cannot name a
group's children — they are not the callable's children); flattening argument delimiters
into sibling syntax nodes (the same braces would get two representations depending on
structural role, and argument values lose their group class); lone-group unwrap accessor
heuristics (parser intent is not reconstructible after the fact); `Vec<BuildId>` content
designation (contiguity by checked contract instead of by construction; O(k) for slot
bodies; empty content loses its anchor).
*Revisit if:* re-minting the layout-dependent ranges in transforms proves error-prone (by
design, any new tree re-resolves records through its own builder).

**Group nodes store their delimiters: `NodeKind::Group(Box<GroupData<L>>)`** — DECIDED
(user, July 2026, current-level review session; follows pylatexenc's
`LatexGroupNode.delimiters`). `GroupData<L>` = `{ group_type: Option<L::GroupTypeId>,
open: TextContent, close: TextContent, ext }`.
*Rationale:* a `Group` whose delimiters were only recoverable through the `Language`
registry violated the already-stated rule that recomposability must not depend on `Lang`
cooperation (marker spellings were stored on the node for exactly that reason) — detached
and synthesized groups couldn't recompose; delimiter-sensitive consumers (pylatexenc's
double-group unwrap compares `delimiters[0]`) need the strings directly. `TextContent`, not
`Box<str>`: span-backed zero-copy when parsed, owned when synthesized; empty `close` on
tolerant "close never found" recovery. `group_type` is **kept alongside** the strings as the
typed identity ("is this a math group?" without string comparison; `$…$` vs `$$…$$` share
spellings, not identity) and is `Option` so *internal synthesized groups* — structural
groups corresponding to no language group type — are representable (user amendment). Boxed
for the same reason `CallableData` is: `Chars` must keep dominating the enum size.
*Rejected:* delimiters-only (pylatexenc-pure — group classification degenerates to string
comparison); registry-only (the inconsistency above).
*Revisit if:* per-group-node allocation shows up in profiles (then consider inlining a
small-string delimiter pair).
*(Amended July 2026, group-classes session: `group_type` now records the group's *class*
(§3.2), not a pairing identity — the "is this a math group?" typed check stands unchanged,
while `$…$` vs. `$$…$$` now share a class and are distinguished by the stored delimiter
strings where spelling matters. The delimiters-only rejection stands.)*

**Node spans stay mandatory; synthetic-node representation deferred** — DECIDED (user,
July 2026, Phase 5 design session). `NodeData.span: SourceSpan` is non-optional: parse-produced
nodes always have a real span, and level-1 recomposition (span → verbatim text) is
unconditionally available. How *transform-created* nodes represent provenance (empty span
anchored at the insertion point, a `Synthesized`-provenance source, a detached variant, …) is
decided together with the transform/visitor API, post-Phase-6.
*Rejected:* `Option<SourceSpan>` now — every span consumer grows a `None` case that no
Phase-5/6 code path can produce, and `TextContent::Spanned` would be unresolvable on span-less
nodes (forcing an awkward "span-less ⇒ all content owned" side invariant).

**Staging builder with breadth-first flatten** — DECIDED (July 2026, Phase 5).
`NodeTreeBuilder` stages nodes bottom-up with explicit child-id lists; `finish(root)` lays the
tree out breadth-first (root at index 0, each node's children appended as one contiguous
block). Child ids must already be staged — cycles are unrepresentable by construction — and
each node is claimed as a child at most once; staged nodes unreachable from the root are
silently dropped (parsers may abandon speculatively built nodes on tolerant-recovery paths).
*Rationale:* `children: Range<u32>` requires *sibling*-contiguous storage, and no direct
arena-emission order provides it — recursive descent gives subtree-contiguous layouts
(`G(c1(d1,d2), c2(e1))` emits `d1,d2,c1,e1,c2,G` post-order; `c1` and `c2` are not adjacent).
Staging + flatten is O(n) with one transient copy, and keeps the builder API free of layout
obligations.

**`TextContent` is S0 and lives in the source topic; no `PartialEq` on node types yet** —
DECIDED (July 2026, Phase 5). Home: `source/text_content.rs` — its `Spanned` variant is a
`Span` into a source, and materialization is a source-content operation; the node topic (S1)
merely uses it. No `PartialEq` on `TextContent`: logical-text equality of a `Spanned` value
requires the source content, so a structural `==` would be a footgun (`Spanned(2..4)` vs
`Owned("ab")` may denote the same text); comparisons go through resolved `&str` (node-level
accessors). Node/layout types likewise ship without `PartialEq` until golden-test needs make
the right equality concrete (Phase 6/7).

**`Comment` nodes store their start delimiter and post-space** — DECIDED (user, July 2026,
Phase 6 plan session; closes open question §6.5 / Phase 6 notes Q4, Option A).
`Comment { content, start: TextContent, post_space: TextContent, ext }`; the node's span
covers start delimiter + content + post-space (the token's span convention).
*Rationale:* with several `CommentRule`s in scope, *which* start delimiter fired and what
syntactic post-space followed (newline + indentation) are per-instance facts; storing both
mirrors `CallableData.post_space` and the recorded-delimiter principle (`GroupData`), making
level-2 recomposition self-contained, synthesized comments included.
*Rejected:* recovering either from the span (fails for synthesized comments) or from a
`Language` default (guessing).

**Environment scaffolding (`\begin{name}` / `\end{name}`) is neither child nodes nor a
stored record — rigid syntax, reconstructed** — DECIDED (user, July 2026, Phase 6 plan
session; closes the terminator-representation question left open by the regions session).
An environment-shaped callable's span covers the whole `\begin{align}…\end{align}` extent
(plus post-space); its children are the argument regions followed by the body `List` — one
contiguous block whose span runs from the first argument region to the body's end. The
`\begin{name}` / `\end{name}` bytes are the block's prefix/suffix complement within the
node's span and are not otherwise represented.
*Rationale:* the syntax is deliberately **rigid** (a deviation from LaTeX): no comments or
newlines between the begin/end command and its name group — the name group must be the
immediately following token; inline whitespace after `\begin`/`\end` (the command token's
post-space) is tolerated and *not recorded*, an accepted level-2 normalization to the
canonical spelling. Under rigid syntax, reconstruction from `(callable_type, name)` + spec
knowledge is deterministic — "reproduce, don't guess" holds because there is nothing to
guess. The partition invariant holds in its callable form: regions tile the child list, the
children block is span-contiguous, and the scaffolding is derivable as the two sub-spans
(node-span start → children start) and (children end → post-space start). A preset that
wants the verbatim scaffolding strings anyway extracts exactly those two sub-spans at
`Lang::finalize_node` time (§3.6) and stashes them in node ext.
*Rejected:* a `terminator: TextContent` record on `ParsedSlot` (every environment pays
storage for a string that rigid syntax makes reconstructible); terminator as region nodes
(`\end`'s command bytes have no honest node kind — a `Chars` node holding markup would
violate chars-are-content).
*Revisit if:* a construct's closing syntax is genuinely per-instance-variable (a fence
closing with its own trigger text is fine — that is `name`; a freely chosen close spelling
is not): that construct's parser then records the choice on the node, following the
`GroupData` delimiter precedent.

**Whitespace and span invariants pinned (the §nodes Phase 6 pin-down)** — DECIDED (user,
July 2026, Phase 6 plan session; ARCHITECTURE.md §nodes updated).
1. *Chars accumulation:* `Char` tokens accumulate into maximal `Chars` nodes; a token's
   pre-space (content whitespace) joins the run; the run flushes when any non-`Char`
   construct starts. Pending whitespace with no adjacent chars becomes a whitespace-only
   `Chars` node. Parsed content is always `TextContent::Spanned` (the exact span slice).
2. *Paragraph breaks:* their own nodes, produced via `Lang::make_paragraph_break_node`
   (§3.6; default: whitespace-only `Chars` spanning the full token, newlines included);
   never merged into neighboring whitespace nodes (adjacent whitespace-only `Chars` nodes
   are possible and fine — deterministic).
3. *Callable post-space:* **exactly the trigger token's own syntactic post-space** — the
   name-terminating whitespace of a multi-character command, already inside the token's
   span (pylatexenc's `macro_post_space`); nothing beyond it is ever claimed. Whitespace
   after a single-character command (`\& b`) or after a final argument is ordinary
   sibling/region content, as in TeX. Groups have no post-space (space after `}` is
   content). Comment post-space is the token's (newline + indentation, stopping at
   paragraph breaks). *(Amended July 2026, Phase 6.4, user decision — supersedes the
   original "claimed by the invocation parser via a peek + `move_to` one-call helper"
   rule; the planned `claim_post_space` helper was never shipped. Two arguments: TeX
   swallows whitespace only after a control word, so claiming beyond the token would
   deviate from both TeX and pylatexenc semantics; and the token-only rule keeps
   `TokenListReader` faithful — a claim helper re-peeking after the trigger would read
   whitespace a pre-scanned list cannot re-serve. Consequences: for callables *with*
   arguments (6.5) the recorded post-space sits between the name and the first argument
   region — a sub-range of the node's span but no longer necessarily trailing — and
   environment-shaped callables record empty post-space, the whitespace after `\begin`
   being unrecorded rigid-scaffolding normalization and the whitespace after
   `\end{…}` being sibling content.)*
4. *End of stream:* `EndOfStream.pre_space` materializes as a final whitespace-only `Chars`
   node.
5. *Partition invariant:* sibling spans partition the parent's *content interior* exactly —
   `List` bodies, `Group` interiors, the root. For callables: argument/slot regions tile
   the child list (builder-enforced), the children block is span-contiguous, and unrecorded
   rigid scaffolding is the reconstructible complement (previous entry). Checked
   mechanically by a test-utility `check_tree_invariants()` — deliberately a test aid, not
   builder law, so a future construct that legitimately breaks byte-accounting amends a
   test, not the architecture.

**Cross-tree `NodeId` misuse: debug-only provenance tags** — DECIDED (user, July 2026,
code-review follow-up session).
`NodeTree::node()`'s assert checks *range*, not *provenance*: an in-range id minted by a
different tree silently resolves to whatever node sits at that index — exactly the hazard
of Phase 7 transforms, which hold two trees (source + rebuilt) at once. Debug builds now
stamp every tree layout with a tag from a wrapping `static AtomicU32` counter
(`node::tree::next_tree_tag`; `fetch_add` wraps, fine for a heuristic), carry it in
`NodeId` and in resolved `ChildRegion` records, and `debug_assert` the match at the single
choke point `NodeRef::new`. Release builds store and check nothing (`NodeId` stays 4
bytes). The tag is excluded from `NodeId`'s `Eq`/`Ord`/`Hash` so debug and release agree
on id semantics. Layout-preserving copies (`clone()`, `materialize()`) share their
source's tag — their ids are genuinely interchangeable.
*Rejected:* the nodes `Vec`'s data pointer as tag (not stable while the builder's vec
grows — ids are minted before the final layout exists — and reused by the allocator after
drop); a debug-only `Box` dummy allocation whose address tags the tree (unique among
*live* trees, but likewise reusable after drop; the counter never repeats short of 2^32
trees). Bare `Range<u32>` node ranges remain uncheckable — they carry no provenance even
in debug; `nodes_in()`'s docs say so.
*Revisit if:* Phase 7 gives ids/regions a first-class cross-tree remapping story — the
tag then belongs in that design.
*(Amended July 2026, 7.8: the first cross-tree machinery landed as the crate-internal
`node::copy::copy_subtree_into` — a finished subtree re-staged through the builder,
resolved regions translated back to staging coordinates and re-resolved for the new
layout. Copies get new tags/ids by design (correlation with the original is by span,
same `Arc<Source>`); the tag design is unchanged. A public transform surface remains a
later phase's design.)*

**Slot read API: content nodes are primary; the wrapper node is an explicit, optional
accessor** — DECIDED (user, July 2026, code-review follow-up session).
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
*Revisit if:* the List-free direction lands — `slot_content_parent` then likely
disappears with the wrapper it exposes.

**Read/extraction API (R7): `NodeSlice` as the node-list currency, free-function
`node::extract` helpers, and derived results as real minted trees (the "builder
route")** — DECIDED (user, July 2026, 7.8 checkpoint session).
- **`NodeSlice<'t, L>`** — a `Copy` view `{&NodeTree, Range<u32>}` over a contiguous
  sibling run — is what every node-list-returning accessor returns: `children()`, the
  region/content accessors, and the new by-name family (`argument_nodes_named`,
  `argument_content_nodes_named`, `slot_content_nodes_named`). Motivation (user): span
  information belongs **in the return types**, not in a helper recomputing it
  best-effort — `span()`/`source_text()` are *exact* by the §nodes partition invariant
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
  resolves the Action-05 deferral now that consumers exist (extraction composition,
  the 7.9 acceptance suite); `iter_storage_order` keeps the breadth-first contrast
  documented.
*Rejected:* expanding `NodeRef` into an `InTree`/`AdHoc` enum so split partials could
be tree-less nodes (user's initial sketch, analyzed): ownership cannot attach to the
frozen tree's scope, so results must own storage with views constructed on borrow —
workable, but `id()` loses totality and "a node belonging to no tree" becomes a
permanent core-model tax on every future consumer; a public `Segment`/`SegmentPiece`
second node-list type (user: "I don't like a separate structure for another kind of
node list — that's why we have node lists in the first place"); a slice-level
covering-span *helper* recomputing spans best-effort (user: return types must carry
them); `indexmap`-style ordered-map dependency for keyval; keyval aggregation knobs
(strictly less information than duplicate-preserving entries).

**`NodeRef::summary()`: the compact node description is core API** — DECIDED (user,
July 2026, Phase 7.9 session).
A one-line rendering per node — `chars(ab )`, `group(Math $ $)`, `Macro(emph)`,
`comment( note)`, `list(3)` — promoted from the preset's `test_support` under 7.9's
dedup mandate: it uses core accessors only (the id types are `Debug`-bounded), so it
is Lang-generic and serves any embedder's tests, logs, and the guide. The format is
documented as human-oriented and **not a stability contract**; structural comparison
(kinds, spans, accessors) remains the exactness tool.
*Rejected:* a `Display`-adapter type (the `SearchedProviders` pattern) — heavier API
surface for a test/log utility whose callers want `String` in assertions anyway;
leaving it duplicated test-side (the 7.9 suite, the preset tests, and the guide would
carry three copies of the same formatter).

### 3.6 Construct parsers, dispatch, engine

**Single-context parsing API (`ParseContext`)** — PROPOSED (July 2026).
Bundles token reader + state + session handle, avoiding pylatexenc's three-argument threading
through every parser. One place to extend later (e.g. depth limits, cancellation).
*(Amended July 2026, Phase 6.4, user-approved: `ParseContext` gains
`source: Arc<Source<L::SourceOrigin>>` — the source the token spans refer into, which
staging a node's `SourceSpan` requires. Factory-created parsers
(`make_invocation_parser(&self, invocation)`, later `ArgumentParser` entry points) have no
constructor through which a caller could thread it, and it cannot ride on tokens or
readers: the token layer deliberately carries only transient byte spans (§3.8 — no
`Arc`-span infection; a reader-side accessor would force `StdTokenReader` origin-generic
and `TokenListReader` to carry a source it doesn't have). The construct-parser layer is
where byte spans become `Arc`-backed source spans, so the context is the honest carrier.
`NodesParser::new`/`GroupParser::new` dropped their redundant `source` parameters —
single source of truth.)*

**Dispatch by token kind + library lookup** — PROPOSED (July 2026). See §2.6.
*Rejected:* `can_parse()`/`priority()` parser registries (registration-order-dependent,
scattered dispatch logic, priority races).

**`Language<L>` owns no per-parse state** — DECIDED (March 2026, as "FLMEnvironment";
renamed July 2026). Long-lived, reusable across parses, accumulates no memory. Sessions are
transient; results are frozen.

**Construct parsers are temporaries; stored parser objects are immutable behavior data (the
two-tier ownership model)** — DECIDED (user, July 2026, Phase 6 plan session).
Tier 1, *stored* behavior objects (specs; `ArgumentParser`s inside `ArgumentSpec`):
`Arc`-shared, `Send + Sync`, immutable; every per-use input arrives as arguments of their
entry points (`&self`). Tier 2, *engine* construct parsers (`NodesParser`, the group parser,
invocation parsers, body parsers): short-lived values constructed with their per-use
configuration where they are needed, free to borrow (`'s` content, token refs),
`parse(&mut self, …)` so working state may live in fields, dropped when the frame ends. No
`Send + Sync`, no `'static`, no `OnceLock`/`static` gymnastics — those pressures existed
only in designs that *stored* engine parsers.
*Rationale:* mutable working state and stored sharing are incompatible without locks; giving
each tier one job removes the conflict. Closures (stop predicates) are thereby confined to
tier 2 — specs stay data (§2.1).

**`CallableSpec::make_invocation_parser` — a factory moving a fresh parser to the caller** —
DECIDED (user, July 2026, Phase 6 plan session; settles Phase 6 notes Q2 with a third option
superseding both sketched ones).

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
*Rationale:* all parser logic lives in construct parsers — specs only *supply* parser
objects; the invocation travels inside the parser instance, so `ConstructParser::parse`
keeps one uniform signature; and this is exactly pylatexenc's `get_node_parser(token)`
shape (a parser instance built for that token), with ownership made explicit.
*Rejected:* a defaulted `parse_invocation(&self, cx, &Invocation)` method on the spec
(fuses factory and call — parsing methods on specs); a cached `Arc<dyn ConstructParser>` in
the spec with the pending `Invocation` in a `ParseContext` field (a set-before-use protocol
spanning every spec and every dispatch — the regions two-phase records accepted that genus
of invariant only because it is contained in one component at one point); a generic
`with_invocation_parser(inv, closure)` (stack allocation, but kills `dyn CallableSpec`
object safety).
*Revisit if:* the per-invocation `Box` allocation shows up in parse-throughput profiles
(run a micro-benchmark; see Phase6Execution.md §6.7). If it ever matters, the dispatch
loop can special-case the default path without touching the trait. *(Phase 6 close, July
2026: the pre-close benchmark check this note originally flagged was consciously deferred,
not dropped — user decision, performance-review session; see the state-memo entry's
companions note. The obligation stands open, unscheduled.)*
*(Phase 6.6 finding, recorded for Phase 7's `EnvironmentSpec`:* a composition running
*inside* `parse(cx)` cannot mint a **new** `Invocation` for a construct it resolves
mid-parse — `Invocation.name: &'s str`, and the `'s` source content is unreachable through
`cx` (the source is `Arc`-owned; tokens and readers carry only byte spans, §3.8). So a
two-level dispatch — a `\begin` spec's parser calling the resolved environment spec's own
`make_invocation_parser` — does not work with the current `Invocation` shape; the 6.6
test composition instead drives `EnvironmentBodyParser` directly under the resolved spec.
Relatedly, a *stored* trigger token cannot be handed back to `cx.tokens` (the uniform
`parse` signature cannot tie it to the context's reader — the 6.3 finding), so the
takeover post-space reposition idiom is expressed positionally:
`move_to_pos(token.post_space().start())`.)*

**`Lang::finalize_node` — one centralized finalization hook, run by the builder for every
staged node** — DECIDED (user, July 2026, Phase 6 plan session; supersedes the spec-level
`finalize_invocation` proposal — report R3 / pylatexenc's `CallableSpec.finalize_node`).
Called inside `NodeTreeBuilder::add` for **all** nodes (every kind, not just callables),
before the staging checks; receives mutable access to the node's parts (kind, uniform ext,
span, state) plus a read-only view of already-staged nodes (so a callable's hook can
inspect its children — e.g. extract scaffolding sub-spans, §3.5); default: no-op.
*Rationale:* the builder is the single mutation boundary, so hooking there guarantees *no
node escapes finalization* — no parser cooperation required, transforms and tests included.
A preset delegates to spec-specific behavior itself (FLM's `Lang` sees a `Callable`, reads
`data.spec`, downcasts, attaches its `flm_specinfo`-like ext — the `Any`-supertrait
contract, §3.4; downcasting to the preset's own spec *trait* goes through the
concrete-wrapper pattern recorded there), so the core needs no spec-level hook at all;
and *uniform* per-node initialization (fields every node of a language carries) gets a
natural home, which a callables-only spec hook could never provide.
*Consequences:* the hook must tolerate re-staging (transform-built trees pass nodes through
a new builder — finalization runs again on already-finalized data; implementations must be
idempotent); it runs on speculatively staged nodes that may be abandoned (harmless — they
drop unreachable); the builder grows a small staged-node read view (also wanted by
node-based stop predicates, below).
*Rejected:* spec-level finalize in core (callables-only; custom invocation parsers must
remember to call it); a `ParseContext`-side helper (forgettable, and transforms bypass it).

**`Lang::resolve_command` hook** — DECIDED (user, July 2026, Phase 6 plan session; Phase 6
notes item C1 as sketched; return type amended July 2026, next entry). `Command` tokens
resolve through `fn resolve_command(state, &token) -> CommandResolution<Self>`
(`Resolved(ResolvedCallable { callable_type, spec })` / `Unresolved { detail }`);
typically dispatches to the state's libraries via
`CallableQuery { syntax: Command { escape_char }, … }` — the token now carries its escape
char (§3.2). An `Unresolved` answer → the nodes parser diagnoses and recovers (§3.8).
Specials need no hook: recognition = resolution; the token already carries its spec.
*Rationale:* the dispatch loop needs `(CallableTypeId, spec)` for command tokens and the
core cannot know a preset's type ids; follows the `scan_specials` precedent (a `Lang` hook,
recognition kept close to resolution).

**`CommandResolution` carries a failure `detail` string; the unimplemented default
`resolve_command` reports itself through it** — DECIDED (user, July 2026). Two needs, one
channel. (1) Forgetting to implement `resolve_command` has no compile-time signal: a
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

**A third `CommandResolution::Failed` variant distinguishes an operational resolver
failure from a clean miss** — DECIDED (user, July 2026, Phase 7.5 review follow-up).
`resolve_command` now returns three outcomes: `Resolved`, `Unresolved { detail }` (a clean
miss — the name is defined nowhere the query saw), and `Failed { detail }` (a definition
*provider* errored while answering — a broken or unavailable source). The dispatch sites
diagnose `Failed` as a distinct condition — `CommandResolutionFailed`
(`core.nodes_parser.command-resolution-failed`), separate from `UnresolvableCommand` —
recovering the same way (span-backed chars). The shared scope-stack resolver
`CommandResolution::resolve_via_scopes` (the one home for the preset and the test langs)
maps a provider `Err` to `Failed`, where the per-driver copies previously flattened it into
`Unresolved`.
*Rationale:* tooling and `refine_diagnostic` can now tell "command unknown" from "resolver
broken" by condition identity rather than string-sniffing the detail; this mirrors the 7.3
`ScopeOpFailed` precedent, which likewise gives operational scope-stack failures their own
condition. `CommandResolution` is `#[non_exhaustive]`, so the added variant is non-breaking
downstream (the wildcard obligation already stands).
*Distinct from* the earlier-rejected `Unknown`/`Unimplemented` variant pair: that split was
along *miss-reason prose* (subsumed by the detail string); this one is along *miss vs.
operational error* — an outcome axis a detail string cannot carry to a consumer keying on
the condition id.

**`Lang::make_paragraph_break_node` hook** — DECIDED (user, July 2026, Phase 6 plan
session; Phase 6 notes item C3, upgraded from "core default, hook only if Phase 7 needs
it"). `fn make_paragraph_break_node(state, &token) -> NodeKind<Self>`; default: a
whitespace-only `Chars` kind, `TextContent::Spanned` over the full token span (newlines
included). The *core* stages the returned kind with the token's span and the current state —
a `Lang` cannot stage nodes itself.
*Rationale:* ARCHITECTURE §nodes left paragraph-break representation to the preset;
returning a `NodeKind` keeps callable-shaped paragraph breaks (FLM) expressible without a
Phase 7 redesign, while the default preserves the whitespace-as-chars invariant (§3.5).

**Stop conditions: reified value + tier-2 predicates; abnormal endings are data
(`StopCause`), not errors** — DECIDED (user, July 2026, Phase 6 plan session;
pylatexenc-informed). `NodesParser` accepts a stop specification with two independent
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
errors (§3.8).
Deliberate deviations from pylatexenc: the node predicate sees (count, last node), not the
whole node list on every iteration (pylatexenc's `stop_nodelist_condition(nodelist)`
invites O(n²) rescans); predicates live only in tier-2 parser temporaries, never in spec
data (§2.1).
*`GroupClose` matches the exact `(group_type, close)` pairing, not either field alone
(amended July 2026, Phase 6.2 review):* both must hold, because each guards a distinct
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
*Token stop conditions carry a `consume` switch; `StopCause` reports the matched span
(amended July 2026, Phase 6.2 review):* `TokenStopCondition` became a struct
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
(token stop) and `NodeCondition` (node stop), and `UnexpectedGroupClose` grew a `span`: the
group parser (6.3) builds its `Spanned` close delimiter from that span, which it can no
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
condition (amended July 2026, Phase 6.2 review):* the two triggers can collide — a token
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
*Rejected:* a declarative stop-condition language in spec data (the Q1 ruling: terminators
are parser business); closure storage in specs; a consume *callback* handed to the stop
predicate (or a `Stop { consume }` predicate return) — it only adds per-match branching
inside one heterogeneous predicate (rare), reaches neither the declarative variants (the
common `GroupClose`) nor the post-lookahead case, would force a second consume mechanism
alongside the static flag, and turns a pure condition into a reader-mutating actor;
deferred until a real dynamic-consume consumer appears, and even then localized to the
`Predicate` variant.

**Slot terminators are parser business; the environment-body parser lives in core
`constructs`, parameterized** — DECIDED (user, July 2026, Phase 6 plan session; settles
Phase 6 notes Q1 against both sketched options). `SlotSpec` stays
`{ name, parsing_state_delta }` — no declarative terminator vocabulary in core spec data.
The data of the rejected declarative design (stop-command name, name-group type,
match-invocation-name) becomes the *constructor parameters* of a core
`EnvironmentBodyParser`: it runs `NodesParser` with a stop condition on the terminator
command, verifies the name back-reference (`\end{align}` matches the `align` that opened),
stages the body `List`, and leaves post-space claiming to the invocation parser driving it.
Environments remain zero-custom-code for spec authors — the preset's `EnvironmentSpec`
(Phase 7) instantiates the parser from data. Verbatim bodies need no terminator-state
doctrine at all: a verbatim construct's parser reads raw content itself and never runs
`NodesParser` — the "which state scans the terminator" question dissolves.
*Rationale:* a declarative terminator spec re-creates a parser-description language inside
spec data for exactly one consumer, while the parameterized parser expresses the same
constructs with the same zero user code; core placement is legitimate because every
parameter is data — no privileged spellings (§2.3).
*Rejected:* `SlotTerminatorSpec`/`StopConditionSpec` as core spec data (notes Q1 Option A);
stop-before-terminator with preset-owned consumption (Option B — weakened the declarative
path and left terminator syntax neither recorded nor reconstructible).
*(Amended July 2026, slots session: `SlotSpec` itself is now deleted — slots have no
spec-side declaration at all; see the no-spec-side-slots entry below. The core of this
ruling — terminator data as `EnvironmentBodyParser` constructor parameters — stands
unchanged; the body state delta rehomes to the preset spec type that drives the parse.)*
*(Amended July 2026, subphase 6.6 / Phase 6 close: the shipped constructor is
`EnvironmentBodyParser::new(trigger_span, invocation_name, stop_command_name,
name_group_type)` — two approved adjustments to the sketched parameter set. `trigger_span`
anchors the missing-terminator diagnostic at the `\begin{…}` that opened (the
`GroupParser` unclosed-at-open precedent); `invocation_name` is always required — every
terminator diagnostic names the environment — with the name *check* a builder switch,
`with_match_invocation_name(bool)`, default true (disabled = any rigid name group closes).
"Rule/type" resolved to **type**, matching `GroupArgumentParser`'s parameterization.
Behavior pins from the same review: body and terminator are both read under `cx.state` =
the slot's state (caller-scoped, like arguments), and the body `List` records that state —
the interior state is the honest one for a delimiter-less region (a `Group`, by contrast,
records the outer state; the environment node itself records the invocation's base state).
A token error mid-terminator follows the 6.5 probe rule: strict aborts; tolerant treats
the position as a malformed terminator without diagnosing the token error — the enclosing
loop re-reads it and applies its own recovery, no double report.)*

**Terminator mismatch recovery: close without consuming** — DECIDED (user, July 2026,
Phase 6 plan session). `\begin{A}…\begin{B}…\end{A}`: B's body parser stops at `\end`,
reads the name, sees the mismatch → diagnostic ("missing `\end{B}`"), closes B **without
consuming** the terminator, and returns; the unwinding lets A's parser find and consume its
own `\end{A}`. An orphan `\end` eventually reaches the root nodes parser as an ordinary
command and takes the unresolvable-command recovery (§3.8). A *malformed* terminator
(`\end` not followed by its name group) is diagnosed, **consumed**, and closes the
environment — leaving it unconsumed would cascade the same malformed token through every
enclosing level. Loop safety: every level either consumes the token or unwinds out of its
own frame; the root always consumes.
Accepted consequence: "was this environment properly terminated?" lives in `Diagnostics`,
not on the node — a preset wanting it on the node flags it in ext via `Lang::finalize_node`.
*(Amended July 2026, subphase 6.6: the malformed-terminator "consume" is pinned to the
terminator **command alone**, its post-space included — whatever follows re-parses as
enclosing content (`\end[y]` → sibling `[y]`; `\end{ A }` → sibling group). The command is
the token whose re-cascading this decision forbids; consuming beyond it would eat content
on a guess. And a stray group close *inside the body* — a case the 6.6 plan's recovery
list omitted — resolves by the loop-safety rule: missing-terminator diagnostic + close
**without consuming**, the `GroupParser` unwinding analog; the stray close then reaches a
level that claims it (an enclosing group consumes it silently — one honest diagnostic
total; at the root, the root's diagnose-and-skip adds its own).)*

**No `Language<L>` type in Phase 6; `ParserSession` is the root object** — DECIDED (user,
July 2026, Phase 6 plan session; amends Phase 6 notes item C5 and the §engine timing).
Phase 6 ships `ParserSession` (builder + diagnostics + `Recovery` policy), driven directly
by tests; the `Language<L>` runtime bundle and any `parse()` convenience entry point are
deferred to the phase that demonstrates the need (Phase 7 at the earliest) — convenience
code is not written before its convenience is demonstrable. Consequence: type-id interning
stays deferred exactly as §3.4 recorded (it presupposed `Language`). The "`Language<L>`
owns no per-parse state" principle above is untouched — it binds the type when it arrives.

**`ChildStateSpec`: per-use descent-state policy on `NodesParser`** — DECIDED (user, July
2026, child-state design session; ports pylatexenc's `make_child_parsing_state`).
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
`group: Fixed(outer)`. Timing: the struct and `group` arm land with 6.3; the `invocation`
arm activates with 6.4 alongside `Invocation` itself.
Decided semantics: (1) *resolution precedes policy* — `Lang::resolve_command` runs under the
loop's own `cx.state`, coherent with the state that tokenized the token, and what makes the
resolved spec available to the callback (pylatexenc's hook likewise runs post-resolution,
receiving the node class); (2) *the policy provides the base; spec deltas stack on top* —
`ArgumentSpec`/`SlotSpec` `parsing_state_delta` derive from the policy's answer, so a
caller's rule applies "before the callables add their own deltas"; (3) *one level deep* —
the `NodesParser`s recursed into by group/invocation parsers default to `Inherit`; note that
group-delimited *arguments* of an invocation are reached by the `invocation` policy (they
are parsed inside the invocation parser), not by `group`; (4) *sibling deltas unaffected* —
still applied to the loop's own `cx.state` (§3.3's outward-propagation design already
blesses applying a delta to a base the producer never saw); (5) *states pass as-is* —
`Arc` in, `Arc` out: `Inherit`/`Fixed`/pass-through `Compute` never force a `derived()`,
and returning the same `Arc` preserves pointer identity (the §3.2 identity-keyed
memoization argument stays sound); (6) *callbacks are pure `&dyn Fn`* (like
`TokenStopKind::Predicate`, unlike the node condition's deliberate `FnMut`): a descent
policy whose answer depends on call order would be fragile, and `Fixed` covers the
stateless case. (Re-examined and upheld against session access, July 2026 — see the
session-mediated derivation entry: precompute-and-select expresses context-dependent
policies purely, with full `Arc` sharing.)
Pitfalls recorded: group termination self-heals under any policy base (the group parser
sets `expecting_group_close` on the interior state, which takes tokenizer precedence), but
environment bodies do not — a base that cannot tokenize `\end` runs the body to
`EndOfInput`, surfacing as `StopCause` for the caller to diagnose; and "disable specials"
was not delta-expressible when this was decided — settled immediately after by
`enable_specials` in the `TokenRules` `enable_*` flags decision (§3.2).
*Rationale:* descent-state control is the one pylatexenc state hook with no techy
equivalent, and pure parser composition (run `NodesParser` to a `GroupOpen` stop,
invoke the group parser directly under the other state, loop) — while it works and remains
the escape hatch — re-implements the stitching at every use site; the knob makes the
common case declarative.
*Rejected:* three fields keyed by token kind (`command` + `specials` would be near-identical
enums; the real split is the descent pathway, and `Invocation` already carries
`callable_type`); routing through `StateExt` + `finalize_transition` on a group-entry event
(can only *reconstruct* rules, never restore the actual outer `Arc`, and makes a generic
chars-except-groups argument parser depend on `Lang` cooperation); letting the policy
influence *resolution* (would resolve under a state other than the one that tokenized the
token, and voids the callback's resolved-spec context).

**Group interior states are memoized in the session** — DECIDED (user, July 2026,
child-state design session; conditional go — "adopt if straightforward" — and it is). The
6.3 group parser keeps always-derive *semantics* (every interior state carries its
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
its `parsing_state_delta` — so a `(base, spec)`-keyed entry kind is a possible 6.4+
extension, strictly profiling-driven.) Sibling `{…}` groups under one
state then share a single interior `Arc` — one `StateData` clone per `(base, rule)` instead
of per group descent, the dominant state-cloning cost in deep documents. Entries hold their
key `Arc`s alive, so pointer keys cannot be reused (no ABA hazard). Consequences:
`Lang::finalize_transition` runs once per `(base, rule)`, not once per descent — its
contract is already a pure function of `(data, prev, events)`; and `derived()` itself still
always mints a new `Arc` (the memo sits in the group parser, calling it less often), so the
§3.2 peek-idempotence argument is untouched — shared interior `Arc`s only *raise* the hit
rate of any future identity-keyed reader memo.
*Rationale:* `ParsingState` cannot host the memo — its derived caches are eager by decision
(no_std: no `OnceLock`, and `OnceCell` would cost `Sync`) — but `ParserSession` is the
parse's designated mutable surface, `&mut`-threaded, needing no synchronization.
*Rejected:* a memo inside `ParsingState` (above); skipping derivation when the close is
already table-resolvable in the base state (saves the same clones but leaves children of
plain brace groups recording a state whose `expecting_group_close` is `None` — the memo
gets the savings without the semantic wrinkle).
*Revisit if:* profiling shows the linear memo scan or memory growth under pathological
nesting (one entry per depth level) warrants a map or a cap — or 6.3 implementation
friction appears, in which case ship plain always-derive and flag for performance review.
*Amended (July 2026, performance review):* generalized — the memo now lives uniformly in
`derived_state` (see the rules-only memo entry below); `group_interior_state` stays as a
shape-guaranteed wrapper, and the linear `Vec` scan became a `hashbrown` map.

**Session-mediated derivation is the in-parse standard; transitions have two levels** —
DECIDED (user, July 2026, child-state design session follow-up; extends the memo entry
above). Within a parse frame, construct parsers obtain derived states through the session:
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
Timing: `derived_state`, the keyed group helper, `SessionExt`, and `observe_transition`
land together in 6.3 — the seam ships whole, so the memo never exists without its
observation channel.
*Rationale:* one seam hides memo storage, gives transition provenance a place to reach
`Diagnostics` (the "deltas are inspectable data" promise), and sees memo-hit transitions —
which no state-level hook can, by construction.
*Costs accepted:* two derivation idioms coexist, separated by documented scope; Rust has no
stable associated-type defaults, so every manual `Lang` impl writes `type SessionExt = ();`
(`SimpleLang` absorbs it).
*Rejected:* `get_derived_state` naming (the crate's first `get_` prefix; `derived_state`
chosen — adjective form matching `ParsingState::derived`); giving `finalize_transition`
session access (forfeits the memo and breaks data-equivalence of out-of-session
derivations); session access for `ChildStateSpec::Compute` callbacks (a policy hook could
stage nodes and emit diagnostics, and the loop's borrows tangle — purity upheld:
precompute-and-select covers context-dependent policies with full `Arc` sharing, and the
designated first relaxation, should a consumer demand latching state, is `Fn` → `FnMut` on
the node-condition precedent, not session injection).
*Amended (July 2026, performance review):* "never memoizes" no longer holds — rules-only
deltas are memoized inside `derived_state` itself (next entry). The original reasoning
(arbitrary deltas have no identity) survives as the *gate*: deltas carrying
ext/events/library pushes still always derive fresh.

**Rules-only derivations are memoized uniformly in `derived_state`; retention accepted;
`hashbrown` adopted** — DECIDED (user, July 2026, performance-review session; supersedes
the never-memoize rule of the two entries above — `derived_state` is now the single
memoization seam, narrow helpers are wrappers over it).
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
mandatory dependency; ARCHITECTURE.md Decision 5 amended accordingly. Probes are
allocation-free (`Equivalent`-keyed lookup; the owned key is materialized only on
insert). Pitfall recorded: a **hash may never replace the stored key** — equality on the
key is what makes collisions harmless; a hash-only "key" would return a wrong state on
collision with no diagnostic.
*Rejected:* restructuring `ParsingStateDelta` for the memo's sake — the flat all-`Option`
struct is already the canonical sparse form (one slot per field, no ordering, no
duplicates), and a stored what-changed mask desyncs against the public fields and breaks
struct-literal construction (E0451).
*Companions (same session):* `PrefixTable::first_chars` **removed** (dead public API
describing the rejected maximal-run design, §3.2/§4; premature to wire in as a `match_at`
guard — §6 open question 1b can reintroduce a merged table if profiling demands);
`PrefixTable` reuse across derivations (see the §3.3 implementation notes); the benchmark
harness (Phase6Execution §6.7 obligation) consciously deferred, not dropped.
*Revisit if:* profiling shows memo overhead dominating on non-recurring deltas, or
per-parse memory growth hurts on pathological documents — eviction is unsound with
pointer keys, so that would need a different key design.

**Optional-group arguments balance their delimiters; brace protection via the descent
policy (pylatexenc's `make_child_parsing_state` semantics)** — DECIDED (user, July 2026,
subphase 6.5 review; supersedes the LaTeX-style first-`]`-closes rule briefly shipped in
6.5). `OptionalGroupArgumentParser`'s minted `GroupRule` is in force for the argument's
whole extent — the probing peek *and* the group's contents, not just the opening
delimiter: `\item[with[recursive[use]of]brackets]` parses as **one** argument whose
contents hold nested `[…]` group nodes. The two-sided child-descent rule that makes this
coherent: a nested group opened by the *minted rule* keeps the contents state (the rule
then rides the inherited states of deeper levels — that is what balances recursively),
while every **other** child descent — a brace group, an invocation — reverts to the
argument's own state, where `]` is an ordinary character: braces protect
(`[{arg with ]}]`, the §3.5 designation example, unchanged), and an invocation's own
arguments inside the option see no bracket rule (`\item[\m{a]b}]`). This is exactly
pylatexenc's `LatexDelimitedGroupParserInfo.make_child_parsing_state` ("group with same
delimiter → keep contents parsing state; else → the outer, original parsing state"),
expressed through `ChildStateSpec` — the §3.6 hook that ports that very pylatexenc
method; `GroupParser` gained a per-use `with_child_states` config so the parser that
scopes a group's interior can steer its descents (default stays `Inherit`; decided
semantics 3 — one-level-deep policies — is untouched: this is per-use configuration at
the level that scopes it, not propagation). Shapes verified empirically against
pylatexenc 3.0a33 (`[with[recursive[use]of]brackets]` → identical node spans;
`[{arg with ]}]` → protected; `[ {a} ]` → three content nodes, no unwrap).
*Rejected:* LaTeX's first-unprotected-`]` rule (TeX delimited-parameter matching, xparse
`o`/`O` arguments) — implemented first for LaTeX parity and reverted on user review:
pylatexenc parity is the 6.5 exit criterion, balanced matching is what document tooling
expects, and the brace-protection idiom survives either way.
*Pitfall recorded:* the protection policy rides one bracket level, as in pylatexenc —
whose depth-2 behavior already contradicts its own docstring (`\item[a[{x]y}b]` mangles
silently there: the nested group comes back childless; checked against 3.0a33). techy
mangled the same pathological shape *with* diagnostics — **closed by the
temporary-group-rules entry below** (July 2026), which supersedes this entry's
`ChildStateSpec` wiring entirely.
*Revisit — planned mechanism, direction decided (user, July 2026, follow-up
discussion; since implemented — entry below):* the one-level pitfall is to be closed by
**temporary group rules scoped in state data** — reversion by reconstruction
(stripping), the only vehicle that reaches depth N: the outer `Arc` sits N frames up
and is unknowable to the descending site, and caller-side descent policies are one
level deep by design. Direction pinned: temporariness is reified in **core rules data**
(a `temporary_groups` list next to `TokenRules::groups`, or a `transient` flag on
`GroupRule`) — *not* a `Lang` callback recording a `StateExt` flag (a core parser's
parsing correctness must not depend on `Lang` cooperation — the same ground the
ChildStateSpec entry's `StateExt`-routing rejection stands on; `finalize_transition`
stays reserved for genuine language semantics) and *not* the session (the session layer
is pinned data-equivalent to `derived()` and may never alter a resulting state).
Stripping lives in the **pure derivation path**, keyed on the `expecting_group_close`
change: a derivation installing a *non-temporary* expected close clears the temporary
rules. The trigger self-disambiguates — a nested minted `[` installs the temporary rule
(kept, so brackets keep balancing), a brace installs a normal one (stripped, so braces
protect at any depth) — and remains a pure function of `(base, rule)`, so the session's
derivation memo is untouched. With it, `\item[a[b{c]}]]` parses as expected — beyond
pylatexenc, which mangles exactly that input (3.0a33 checked: childless nested group,
leaked `]`).

**Brace protection presupposes the minted close spelling is not a group delimiter of
the argument state — no active suppression** — DECIDED (user, July 2026, Action-06
review). The revert-to-argument-state rule above protects `]` inside `[{arg with ]}]`
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
*Rejected:* making the revert state actively suppress the close spelling — `]` would
then parse differently inside a brace group under an option than in every other brace
group of the same language: an inconsistency masquerading as robustness.

**Temporary group rules: a state-scoped delimiter lifecycle, enforced at the
derivation choke point** — DECIDED & implemented (user, July 2026; executes the
planned mechanism two entries up and supersedes the optional parser's `ChildStateSpec`
wiring). `TokenRules` gains **`temporary_groups`**, a second rules list that tokenizes
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
*Sub-rulings (the three open questions of the planning entry):* **(i) stripping site**
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
*Pitfall recorded:* `temporary_groups` is a **prefix-table input** — `derived()`'s
table-reuse check compares it elementwise like `groups`; omitting that would reuse a
stale table across a strip and keep tokenizing the dead delimiters (pinned by
`temporary_groups_are_prefix_table_inputs`).

**`ParseContext::parse_scoped` and `ParseContext::probe_token` replace the hand-rolled
state swap/restore and the crate-private `try_peek`** — DECIDED (user, July 2026,
Action-05 session). The `cx.state` swap/restore protocol was correct at every one of its
seven lib sites, but the correctness was per-site discipline (restore **before** the
`?`), and the probe site had to hold a `Result` un-`?`-ed across the restore.
`parse_scoped(state, &mut parser)` — the pylatexenc
`walker.parse_content(parser, …, parsing_state)` analog, deliberately on the *context*
(the session lacks tokens and source; a session-level `parse` remains planned as the
top-level driver entry, Phase 7) — makes the restore structural; the returned delta stays
**unapplied** (the §3.3/§3.6 caller-applies law; an auto-applying driver would be wrong
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

**No spec-side slots: `SlotSpec` and `CallableSpec::slots()` deleted; slots are pure
record-level vocabulary** — DECIDED (user, July 2026, slots session; supersedes the same
session's earlier "slots mirror arguments" lean, and executes plan item A of
PlanSlotsAndConvenienceSurface.md). The mirror died on the **invocation-facts problem**:
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
  several). Self-describing records (§3.5) *preserved*, not weakened: standing alone now
  means carrying the name directly — the spec pointer bought nothing else, since
  `SlotSpec` had no other tool-visible payload. Deliberate asymmetry with
  `ParsedArgument`, which keeps its `Arc<ArgumentSpec>` (parser/name/delta are worth
  pointing at).
- The **slots trap disappears by construction**: with `slots()` gone there is nothing to
  declare that `StdInvocationParser` won't parse — its implementation-error arm and the
  pinned test are deleted (§3.8 consequence list amended).
- The body state delta (pylatexenc's `make_body_parsing_state_delta`) rehomes to the
  preset spec type that drives the parse — Phase 7's `EnvironmentSpec` holds it as an
  ordinary field, read back by its own composition (the test-lang `EnvSpec` rehearses
  this through the §3.4 `Any`-supertrait downcast). The core never interpreted it.
- `StdCallableSpec::new(arguments)` is single-list (a free ergonomics win); the guard's
  `!slots().is_empty()` clause is replaced by the spec-level emptiness method (next
  entry), which the removal makes *more* load-bearing.
- Composition building blocks promoted to `pub` (plan item A.4):
  `parse_declared_arguments` (the shared argument half) and `read_rigid_name_group` (+
  `NameGroup`) — a `\begin`-shaped takeover now assembles from public parts. A
  `ParsedSlots`-assembly helper was judged unnecessary for now: what remains hand-rolled
  is a few lines of offset bookkeeping.
*Open (deliberately):* where the standard `\begin` composition lives — core (a generic
name-indexed delegating dispatcher) vs. the latexlike preset (Phase 7 owns the
`\begin`/`\end` spelling). It stays test-side meanwhile (plan item A.5), as does the C
batch (builder sugar, crate-root re-exports). *(Settled July 2026, Phase 7 plan
session: preset-owned — see the `\begin`-composition entry at the end of this
section.)*

**The emptiness surface: `ArgumentParser::can_match_empty()` +
`CallableSpec::requires_content()`; the expression guard consults the spec** — DECIDED
(user, July 2026, slots session; both names user-decided 2026-07-15; executes plan item B
of PlanSlotsAndConvenienceSurface.md; pylatexenc precedent:
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
  message for a body-bearing takeover that declares none. *(Implementation-forced
  naming consequence, flagged for user sign-off in the session report.)*

**The `\begin` composition is preset-owned; core contributes parameterized building
blocks only; environments customize through their spec (data or hooks), not invocation
takeover** — DECIDED (user, July 2026, Phase 7 plan session; settles the
deliberately-open home question of the no-spec-side-slots entry above).
The standard `\begin`/`\end` composition (rehearsed test-side in
`environment_parser.rs`) rehomes to the latexlike preset in Phase 7: the preset
registers a `BeginSpec` dispatcher whose invocation parser contains minimal/no scanning
code of its own — it reads the rigid name group (`read_rigid_name_group`), resolves the
environment's spec from the state's libraries under the preset's ENVIRONMENT callable
type, parses declared arguments (`parse_declared_arguments`), drives the core
`EnvironmentBodyParser`, and assembles the callable node. The *notion* of "environment"
is preset property; core owns each individual parsing task as data-parameterized
machinery (the §2.3 ground `EnvironmentBodyParser`'s core placement already stands on).
Consequences made explicit:
- **Invocation-level takeover is out; amending `Invocation` is declined.** An
  environment spec's own `make_invocation_parser` is never invoked (the 6.6
  `Invocation<'s>` finding stands as a permanent boundary, not a bug to fix); all
  per-environment variation flows through the preset's `EnvironmentSpec` surface.
  That surface is *not* constrained to plain data (user, same session — "spec-as-data"
  would overstate the ruling): behavior-shaped customization via defaulted methods is
  legitimate, per §2.1 and the factory precedent of `make_invocation_parser` itself.
  For the body-parsing choice (verbatim-like bodies) the user leans to a defaulted
  `make_body_parser()` method (pylatexenc's `EnvironmentSpec.make_body_parser`) over a
  plain field — final shape remains a Phase 7 preset-side design question. *(Settled July
  2026, Phase 7 plan session: the defaulted `make_body_parser()` method, confirmed.)*
- **`EnvironmentBodyParser` keeps its name** (rename raised and reconsidered, user):
  its contract is hardwired to the rigid COMMAND + CHARS_GROUP terminator shape, and
  environments are the one role it is designed for — a generic name
  (`TerminatedBodyParser`) would over-promise arbitrary terminator conditions. Honest
  single-purpose labeling beats false generality; strata rule 2 constrains imports,
  not descriptive vocabulary.
- **`read_rigid_name_group` stays a value-returning scaffolding helper**, deliberately
  separate from the node-staging chars-group *argument* parser (pylatexenc's
  `LatexCharsGroupParser` analog) that Phase 7's std library adds for `\label`-style
  chars-only arguments: scaffolding is reconstructed, never recorded (§3.5), so the
  name reader must not stage nodes — the two roles differ in kind, not configuration.
- Preset-side strays that rehome with the composition: registering an `end` spec so an
  orphan `\end` diagnoses well; the name reader's rigidity contract (trigger post-space
  tolerated and normalized away — `\begin {itemize}`) is a documented knob, not a
  behavior change.
*Rejected:* a core "read-marked-delimited-content" callable spec generalizing
environments (declared arguments + body up to constructor-specified terminator
syntax). The killing flaw is not lookup — a core spec could query a parameterized
`L::CallableTypeId` generically — but *interpretation*: the body delta and body
customization live on the preset's concrete spec type (the slots-session rehoming) and
are invisible through the core `CallableSpec` trait, so a core driver would need
spec-side body vocabulary back in core — exactly what the no-spec-side-slots ruling
rejected. No plausible second consumer either: fence-block-style constructs wire their
own parsers.
*Revisit if:* a second core-worthy consumer of command+name-group termination
materializes (the generalization/name question reopens), or `EnvironmentSpec`'s
body-customization design finds it genuinely needs invocation-level takeover.

**Parser-library gap list vs pylatexenc's `latexnodes.parsers` settled; tack-on
information fields parse as a construct, not postprocessing** — DECIDED (user, July
2026, parser-library survey session; full table with per-parser strategies in
ParserLibraryParity.md). Key rulings and their reasons:
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
  semantics 3 above). The plug's shape (an interior `ParsingStateDelta` on `GroupRule`
  vs a `Lang`/preset hook keyed on `GroupTypeId`) is an open Phase 7 design question.
  *(Settled July 2026, Phase 7 plan session: neither candidate — the `ParseDriver`'s
  `group_interior_delta` hook returning a parsing-mode delta; see the `ParseDriver`
  entry below and §3.3.)*
- **Ready-made argument-parser conveniences are wanted even where composition
  suffices** (user): a multi-delimited group parser (any of several delimiter pairs at
  one argument position — port pylatexenc's contents-state subtlety of keeping only
  default delimiters plus the encountered pair) and an embellishment parser (xparse
  `e{tokens}`-type), which subsumes generalizing `MarkerArgumentParser` beyond the
  single-literal `*` case. A node-staging chars-group parser is likewise wanted,
  deliberately distinct from `read_rigid_name_group`: the environment-name reader is
  value-returning scaffolding (reconstructed, never recorded, §3.5), while the
  chars-group parser stages nodes for `\label{…}`-style chars-only argument groups.
- **Comma-separated chars list**: discarded as a construct parser in favor of a
  split-at-chars read/extraction helper over parsed children (pylatexenc's own
  docstring recommends this route).
*Revisit if:* the tack-on parser's absorption of following siblings turns out to
interact badly with enclosing stop conditions in practice, or a preset's interior-state
plug proves to need more context than the group rule/class provides.

**The deferred parity parsers N2/N3/N4/N6 landed** — DECIDED (user, July 2026, N2–N6
implementation session; full per-parser record in ParserLibraryParity.md, naming rows
in NAMING_STRATEGY.md). The decisions and their reasons:
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
- **N3 matching semantics** (user): noise before a marker, **nothing between marker
  and expression** — the pair is atomic, and a violated pair (`\op^ {a}`, `\op^` at
  EOF, a comment after the marker) rewinds *whole* and ends the run silently: a lone
  marker char is ordinary content nearly everywhere, so a diagnostic would misfire on
  legitimate input; the atomicity also keeps wrapper contents noise-free by
  construction. Each marker at most once (xparse; pylatexenc's removal loop agrees);
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
  configured rule: one derivation, the 7.7 path unchanged). Everything else falls out
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
*Revisit if:* a consumer needs per-embellishment diagnostics for dangling markers
(`\op^` silently unmatching), or a field spec needs takeover-level access to the
absorbing invocation (the configured spec sees only its own invocation).

**`ParseDriver`: parse-driving behavior is a Lang-provided instance, not static hooks or
session state** — DECIDED (user, July 2026, Phase 7 plan session).
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
  ARCHITECTURE "custom nodes parser" nuclear option becomes a supported seam;
- **the group descent-delta channel** — `group_interior_delta(prev, rule)`, pure per
  `(state, rule)`, merged into the memoized `session.group_interior_state` derivation
  (the cache stays in session; the hook runs on memo miss only). With §3.3's parsing
  mode this closes ParserLibraryParity.md N1;
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
*Rationale:* the session-purity argument (user) — `ParserSession` is organized scratch
space, and a parser *provider* conceptually drives the parse; it was misfiled there, as
was `Recovery`. One seam for provision + one home for parse behavior + typed preset
helper access were unreachable from static hooks or a session field.
*Rejected:* a session-installed `dyn` provider (a second customization surface beside
`Lang`; preset helpers invisible behind the trait object — Any-funnel required); more
static `Lang` hooks (no instance configuration; `Lang` was accreting parse behavior
foreign to its layers); overridable `parse_scoped`/`with_frame` (invariant footgun).
*Cost accepted:* every Phase 6 `ParseContext`/`ParserSession::new(recovery)` call site
updates — mechanical but broad.
*Revisit if:* a real consumer needs runtime driver swapping for one `Lang` (add a dyn
override on top of the associated-type default), or per-invocation `Box` provision shows
up in profiles (the §6.7 benchmark obligation).
*Landed (subphase 7.2, July 2026)*, with these in-flight decisions (user-checkpointed):
**Module home `engine::driver`** (user choice over constructs); `CommandResolution`/
`ResolvedCallable` relocated there next to `resolve_command` (crate-root re-exports
keep `techy::…` paths; module paths changed `state::` → `engine::`). **The
group-interior memo is a second, dedicated session map** keyed `(base, rule)` by `Arc`
identity: "hook runs on memo miss only" needs a pre-hook probe key, and sharing the
7.1 memo would let a hand-built expecting-close delta collide with a driver-augmented
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
(uniform with the per-invocation Box; §6.7 benchmark covers it; a fast path could hide
behind the same cx wrappers later). `ParserSession::new()` takes no arguments
(`Default` added); `ParserSession::recover` takes the policy per call — the channel a
custom driver `recover` uses for per-condition decisions. The default
`resolve_command` detail now names `ParseDriver::resolve_command`.

**`Language<L>` + `parse()`: the runtime bundle's landed surface** — DECIDED (user,
July 2026, Phase 7.4 mini-checkpoint; four API-shape decisions on the deferred-from-
Phase-6 type). `Language<L>` = `{ driver: L::Driver, initial_state:
Arc<ParsingState<L>>, resolver: Arc<dyn SourceResolver<O>> }`, long-lived, owning no
per-parse state (March 2026 principle, kept).
- **Entry points are two named methods, not a `SourceInput` enum** (rejecting the old
  §engine sketch's `parse(impl Into<SourceInput>)`): `parse(content: impl
  Into<String>)` mints an anonymous `Source`; `parse_source(Arc<Source<O>>)` takes a
  pre-minted source (origin/provenance intact — the `resolve_source` round trip feeds
  it). A conversion enum whose only job is overloading buys one method name at the
  price of a public type; named methods are self-documenting.
- **Construction seeds from `Lang::initial_state_data()` and customizes by deriving**:
  `new(driver)` + fallible `with_seed_delta(delta) -> Result<_, DeriveError<L>>` (the
  sanctioned seed-customization path — runs `finalize_transition`, so language
  invariants hold over customized seeds; fallible since 7.3 scope ops; a failing op
  drops the bundle under construction — an embedder build-time bug, not a source
  condition) + `with_resolver(…)` (default `NoResolver`); `Default` where `L::Driver:
  Default`. Wholesale `StateData` replacement deferred until a consumer demonstrates
  the need. The `Lang` hook remains the seed source for `Language`-less parses.
- **The advanced path is accessors, not a `session()` method**: `initial_state()`/
  `driver()`/`resolver()`; the sketch's `session()` dropped — after the Phase 6
  amendment `ParserSession` carries no `Language` borrow and `ParserSession::new()` is
  argument-free, so a `Language::session()` would return exactly that (misleading
  discoverability sugar). `ParseResult` likewise stays borrow-free (nodes are
  self-contained; results outlive the bundle).
- **The root drive loop promotes the Phase 6 rehearsal** (nodes_parser
  `root_driver_skips_a_stray_close_and_continues`): loop `cx.parse_nodes` under
  `StopSpec::none()` (through the driver factory — the 7.2 uniform-routing contract
  covers the top-level site); on `UnexpectedGroupClose` diagnose the new core
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

*(Amended July 2026, stray-close review-fix session: the recorded quirk — each
tolerant resume re-entered under the frozen **seed**, with the stray token re-peeked
under it — fired all three of its latent failure modes once after-effect deltas
became reachable at the root (agent code review; tests
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
of the entry state.)*

*(Amended July 2026, Phase 7.9 acceptance: the skip's remaining quirk — the consumed
delimiter's bytes were dropped from the tree, the 7.4 "accepted tolerant
byte-accounting break" — fell to the acceptance suite's invariant gate on its first
real document: any tolerant root recovery (direct, or reached through an
environment-body unwind, as in the ported pylatexenc `test_errors` document)
produced a tree failing `check_tree_invariants`' partition check, colliding with the
phase-exit criterion "invariants clean on every acceptance parse". The skip now
**stages the consumed delimiter as a `Chars` node** under the loop's evolved state —
the markup-in-chars recovery artifact every other tolerant recovery already produces
(orphan `\end`, malformed `\begin`, forbidden characters) — so the root partition
holds across skips. Exempting recovered parses from the invariant was rejected: the
partition is the byte-accounting contract exactness consumers (`NodeSlice::span`)
build on, and a "recovered trees are less true" carve-out would silently spread to
every downstream walker.)*

**`Language::with_provider`: push-a-provider seed sugar** — DECIDED (user, July 2026,
Phase 7.9 session).
The dominant seed customization — "define a package, add it to the language" — gets a
first-class spelling: `with_provider(provider)` ≡
`with_seed_delta(ParsingStateDelta::new().push_provider(provider))`, fallible like the
derive path underneath. Promoted from the preset's `test_support` under 7.9's dedup
mandate (genuinely multi-purpose helper code becomes public API instead of being
duplicated into the integration-test crate).
*Rationale:* the delta spelling buries the everyday operation under two concepts
(delta + scope op); every guide example and suite fixture reads better as one call.
*Rejected:* an infallible signature via `expect` ("Push cannot fail today") — fragile
against future push semantics and against whatever `finalize_transition` does in the
derivation; the `Result` mirrors `with_seed_delta` honestly.

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
from `CallableSpec` being a trait), pointer type (above), content backing (a plain `String`
on `Source`; the `SourceContent` seam was retired, §3.1). Every proposed new `Lang`
associated type should be challenged against §2.1/§2.2 first.

### 3.8 Errors and diagnostics

**Panic policy: `Result` everywhere; panics only for verifiably unreachable invariants** —
DECIDED (user, July 2026, Action 04; refines the original one-line CLAUDE.md constraint).
Four rules:

1. Panics are allowed only for **verifiably unreachable** code — impossibility guaranteed by
   this crate's own structure (a bounds check in the same function; a private constructor
   that always establishes the invariant), *independent of anything outer layers do*:
   problematic user input, a buggy `Lang` hook in a preset, or a misbehaving custom
   argument/construct parser must never panic a core routine. Written as
   `unreachable!`/`expect` with the invariant stated in the message.
2. The violation of a documented input contract is **not by itself** a reason to panic — it
   returns an `Err` (translatable to, e.g., a Python exception by a wrapper).
3. Individual indexing-style exceptions require explicit user approval. Approved (July
   2026): `NodeTree::node`/`nodes_in`, `Span::slice`, `TextContent::resolve`, and
   `ChildRegion`'s resolved-only accessors keep their documented panics **with
   non-panicking companions** (`NodeTree::get`, `Span::get`) — the std `Index`-vs-`get`
   convention: the panicking form for ids/spans the caller minted from this very
   tree/source, the `Option` form for values of unknown provenance.
4. Everything else returns an error.

Consequences applied with the decision (July 2026):
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
  `Lang::refine_diagnostic` pass applies. ~~`StdInvocationParser`'s slots check reports
  the same condition.~~ *(Slots session, July 2026: the slots check is gone with the
  spec-side slot list — nothing declarable is left for it to catch.)*
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

*Rationale:* an invariant assertion that can only fire on a core bug is better loud than
silently wrong; but a panic reachable through an extension author's mistake turns their bug
into a crash of the host application — an error naming the violated contract is strictly
more useful, in every build profile.
*Rejected:* sanctioning the builder's panic-on-caller-bug policy (the Action-04 report's
original recommendation) — it violates rule 2's "outer layers must not panic the core";
`Option`-returning tree accessors everywhere — clutters every legitimate traversal for a
misuse the `get` companions already cover.
*Revisit if:* profiling shows the always-on builder validation measurably costs on the hot
staging path (all checks are O(1) per region/payload today).

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
*Landed (Phase 2, July 2026):* `TokenError<'s>` = structured `TokenErrorKind` (closed enum:
end-of-stream-after-escape, forbidden-char — replaces pylatexenc's stringly `error_type_info`)
+ byte `Span` + `Option<TokenRecovery<'s>>`, where `TokenRecovery` = placeholder token + an
explicit `resume_pos` (explicit rather than derived from the token: a custom source's
placeholder need not end where reading resumes, and the explicit position carries the
advancement contract; the built-in recoveries now all resume at their placeholder's span
end — the end-of-stream-after-escape placeholder is a `Char(escape_char)` covering the
escape byte, revised July 2026, Action 02). Token-level errors
carry plain `Span`s, not `SourceSpan`s — they are transient like tokens; the session converts
whatever it reports into Arc-span `Diagnostic`s (Phase 6). The reader itself is policy-free:
it always returns `Err` with the recovery attached, and the session's `Recovery` policy
decides (the WIP's per-reader `tolerant_parsing` flag is superseded).

**Detection-site recovery; `Err` means abort** — DECIDED (user, July 2026, Phase 6 plan
session; Phase 6 notes item C4 concretized). Three rules:
1. *Recovery happens where the problem is detected.* `ParseContext` exposes the `Recovery`
   policy and the diagnostics sink behind a helper (tolerant: record the diagnostic and
   continue; strict: return `Err`). Token errors continue with their `TokenRecovery` token
   (the reader is already repositioned via `resume_pos`); each parse-level condition
   defines its recovery at its site — unresolvable command: diagnostic + span-backed
   chars-node fallback (markup text in a `Chars` node is an accepted tolerant-recovery
   artifact, always accompanied by a diagnostic); missing mandatory argument: absent +
   diagnostic; unmatched group close at the root: diagnostic + skip; terminator mismatch:
   close-without-consuming (§3.6).
2. *Abnormal endings of sub-parses are data, not errors.* `NodesParser` returns a
   `StopCause`; only the caller knows whether EOF-before-`\end{align}` is an error. Nobody
   ever continues *past* an `Err` — which is what keeps the reader position and the state
   `Arc`s coherent through recovery, by construction.
3. *`Err(ParseError)` = strict-mode abort or genuinely unrecoverable.* It carries no
   recovery payload and bubbles freely. State deltas from an abandoned parse are dropped
   unless the recovering site explicitly returns one; abandoned staged nodes are dropped by
   the builder (designed for this).
*Rejected:* pylatexenc's recovery-attributes-on-exceptions (`recovery_nodes`,
`recovery_at_token`, `recovery_past_token`, caller-applied repositioning) — a workaround
for having no context object, and exactly the caller/callee reader-state ambiguity that
rule 2 eliminates.

**`TokenRecovery::resume_pos` must advance the reader; violations abort even in tolerant
mode** — DECIDED (user, July 2026, code-review follow-up). The content loop's recovery arm
is the one arm that consumes no token, so its termination rests entirely on `resume_pos`
repositioning the reader strictly past the failed read's start. Both in-crate producers
satisfy this, but the contract is reachable by third-party code through two public
extension points (a custom `TokenReader::peek`, a `Lang::scan_specials` returning a
`TokenRecovery`), and a violating `resume_pos` was demonstrated to hang `NodesParser` in
release builds while growing the diagnostics sink unboundedly. The contract is now stated
on `TokenRecovery::resume_pos` and enforced at the adoption site (`nodes_parser.rs`
content loop): if the reader did not advance after the positional move (`move_to_pos`,
née `resume_at` — Action-02 token entry, item 4), the parse aborts with the
token error as a `ParseError` — *even in tolerant mode*, whose promise is a best-effort
tree, not tolerance of non-termination; an abort is the doctrine-blessed failure mode
(no panic, rule 3 above). The guard lives at the adoption site and not inside the move
because `move_to_pos` is deliberately bidirectional (it is also the absent-argument and
environment-name rewind), so it can assert nothing about direction.
*Noted for the future (user, July 2026):* contract violations by extension-point code are
a different *category* from malformed input, and the error model may eventually want to
distinguish them (e.g. a `ParseError` vs. an `ImplementationError`/contract-violation
kind), so callers can tell "your document is broken" from "your `Lang`/reader is broken".
Today both surface as `ParseError` (here: the token error's kind and span); revisit if
more contract guards accumulate.

**Structured diagnostics: condition payloads, not prose** — DECIDED (user + design sessions,
July 2026; supersedes the "grow `ParseErrorKind` variants" intent noted at its definition —
implementation plan in CodeReportAction_01.md). `Diagnostic` and `ParseError` carry a
structured condition payload `Box<dyn DiagnosticData>` plus span and traceback frames — no
`message: String` field, no kind enum, and no string-message constructors
(`Diagnostic::error("…", span)` is removed, with no ad-hoc escape type). The human message is
a pure function of the payload (its `Display`); a wording difference that is not worth a
field in the payload is not worth existing. Condition types are plain public-field data
structs defined **next to the construct that detects them** (group conditions in the group
parser, environment conditions with the environment helper, token conditions in the token
layer, FLM's in FLM) — third-party conditions are structurally identical citizens.
*Rationale:* the tolerant path — the one tools consume — reduced every condition to
severity + sentence + span, and `ParseErrorKind::Syntax { message }` had become the only
parse-level kind, `format!`-ing away exactly the fields a linter/LSP needs. A kind *enum*
cannot be made right: core-owned variants would privilege construct-level vocabulary
("environment" is not a core concept, §2.3), and `#[non_exhaustive]` extension is crate-only,
so downstream languages would stay stringly forever — the same disease one level out. On the
message side, the "same condition, subtly different context" case decomposes without
remainder: a semantic difference belongs in a payload field (tools want it too); a positional
difference is what frames render. This conforms to §2.4: the *structure*
(severity/payload/span/frames) stays closed; the openness is payload-level, like specs, and
serializability is preserved (see the serialization entry below). Exhaustive matching over an
open-ended condition space is not meaningful — consumers handle what they know, via
identifier or downcast. Severity stays a separate field (conditions do not choose it; the
recover funnel records errors), and the `Diagnostic*` nomenclature deliberately leaves room
for warnings later. The contract-violation category noted above also gets its mechanism for
free: ordinary condition types under e.g. `core.contract.*`.
*Rejected:* promoting recurring conditions to enum variants (the original Action-01
proposal — layering and extension flaws above); a `Lang(L::ErrorKind)` static arm (spreads
`L` into `Diagnostic`/`ParseError`, blocking cross-language aggregation, and callables need
dyn anyway since specs live as `Arc<dyn CallableSpec<L>>`); a message-override `String` on
`Diagnostic` (two truths: prose drifts from data, and re-renderers must ignore one of them).

**`DiagnosticInfo` (implementor) / `DiagnosticData` (dyn facade) split** — DECIDED (user +
design sessions, July 2026). Implementors write a plain data struct (pub fields,
`#[non_exhaustive]` + constructor for semver headroom, ordinary `Clone`/`Debug` derives), a
`Display` impl for the wording, and a `DiagnosticInfo` impl: `const IDENTIFIER: &'static str`
plus a defaulted `serializable_data()`. The dyn-compatible facade `DiagnosticData`
(`identifier()`, `serializable_data()`, `clone_box()`) is blanket-implemented for every
`DiagnosticInfo` type and **sealed** — the blanket impl is the only way in.
*Rationale:* the split is forced (an associated const makes a trait non-dyn-compatible) and
buys everything else: `clone_box` boilerplate vanishes (the blanket impl uses the ordinary
`Clone` derive), the identifier is a compile-time constant, and sealing enforces the
const-identifier discipline. Downcasting targets the data struct itself — one type, one
identity.
*Rejected:* macro-generated wrapper types implementing the dyn trait (Rust separates data
from impls, so no wrapper is needed; a wrapper would make downcasts target the wrapper,
splitting each condition's identity in two); getters over pub fields (invariant-free
records).
(Unsealing `DiagnosticData` later is non-breaking; re-sealing is not.)

**Condition-declaration derive: `#[derive(DiagnosticInfo)]`, syn accepted** — DECIDED (user +
design session, July 2026; realizes the derive previously noted as *Future* here; the
generated surface is specified in ARCHITECTURE.md §"Condition declaration via derive").
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

**Two identities: the type in-process, an explicit string on the wire** — DECIDED (user +
design sessions, July 2026). In-process identity is the concrete type (downcast via `Any` —
collision-proof, compiler-checked at producer and consumer); the string `identifier()` exists
only for boundaries where types cannot go (JSON output, linter config, logs). Identifiers are
hand-chosen, namespaced `<layer-or-preset>.<area>.<condition>` (provisional scheme — user,
July 2026: `core.token.*`, `core.nodes_parser.*`, … for library conditions, areas mirroring
today's modules; `<preset-name>.<namespaced-name>` for presets and downstream languages),
exposed as `pub const IDENTIFIER` so consumers compare against the const rather than a
literal. Identifiers and serialization field names are semver-stable API surface: although
the provisional scheme mirrors today's module areas, the strings are frozen independently of
future code moves.
*Rationale:* no compiler mechanism yields a stable wire identity — `type_name` has an
explicitly unstable format and encodes module paths (a refactor must not break a user's
linter config), and `TypeId` differs per build and is not serializable. Wire naming is
convention-based in every ecosystem (rustc lints, ESLint rules, LSP codes); what convention
*can* get is hardening: single-definition consts and a documented namespace rule.
*Rejected:* deriving the identifier from the type name (the two have different change
cadences — a struct rename is an internal refactor, a wire-id change is a silent break; the
derive macro will *require* the id attribute); method name `diagnostic_identifier()`
(stutters as `DiagnosticData::diagnostic_identifier`; the trait context already qualifies,
§3.10); a per-`Lang` `diagnostic_catalog()` with a uniqueness test (user, July 2026:
maintenance work to keep in sync, and namespace prefixes already prevent collisions — can be
added later without breakage).

**Serialization is a derived projection; the struct is the schema** — DECIDED (user + design
sessions, July 2026). `serializable_data() -> DiagnosticValue` (a minimal alloc-only value
tree: null/bool/int/string/list/map) serves serialization boundaries and generic tooling
only — programmatic consumers downcast to the typed struct; there is no stringly-keyed access
API anywhere. The method is defaulted (empty) so the trait ships before the serialization
work. No hand-written shipped schemas.
*Rationale:* `serde::Serialize` is not dyn-compatible (the ecosystem workaround,
`erased-serde`, is a dependency — §3.9), hence the own value tree. The authoritative schema
is the Rust struct itself. pylatexenc's `error_type_info` weakness was ad-hoc dicts assembled
at every raise site; here the keys are written once, adjacent to the struct fields, and the
eventual derive macro generates them from the field names. A shipped machine schema would be
a third representation that drifts from the other two; if external consumers ever need one,
the derive generates it from the same source of truth.

**Parse traceback: an explicit frame stack on `ParserSession`** — DECIDED (user + design
sessions, July 2026). `Vec<Frame<L>>` on the session, maintained by a closure-scoped
`cx.with_frame(frame, |cx| …)` at the descent points (invocation, argument, group interior,
environment body); the recover funnel snapshots the live stack into every `Diagnostic` and
`ParseError` as `L`-free `TraceFrame<O>`s (rendered `title: String` + `SourceSpan<O>`),
innermost first — this finally produces `format_traceback`'s input and renders as
pylatexenc-style "while parsing …" tracebacks (exactly LSP `relatedInformation` shape). Live
frames allocate nothing: `FrameTitle<L>` stores *mechanisms, not a construct taxonomy* — a
`&'static str` label, a quoted source slice, or an `Arc<dyn CallableSpec<L>>` + role whose
title is produced only at snapshot time via a new defaulted, dyn-compatible
`CallableSpec::stack_frame_title(…)` hook.
*Rationale:* pylatexenc attaches `open_contexts` in `except` clauses as exceptions bubble;
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
"context": `ParseContext` already owns that word (§3.10 sibling-vocabulary rule).
*Rejected:* frames in `ParsingState` (above); structured machine fields on frames (frames are
the human-facing projection — machine data belongs in the condition payload; title + span is
what tools need); wrapping-on-bubble (the tolerant path never bubbles).

**`Lang::refine_diagnostic` hook** — DECIDED (user + design sessions, July 2026).
`fn refine_diagnostic(Box<dyn DiagnosticData>, &ParsingState<L>) -> Box<dyn DiagnosticData>`,
default identity, applied exactly once in the recover funnel (at the `ParseContext` level,
where the state is in scope). A `Lang` can replace a generic condition with its own — FLM
maps a forbidden-`$` token condition to a `DollarMathDisabled { … }` whose `Display` explains
the config option — and the replacement is *structured*, so tools see (and can attach
quickfixes to) the refined condition, not just better prose. The original condition's fields
can be embedded in the refined type where faithfulness matters.
*Rationale:* a presentation-only hook improves messages but hides the real condition from
machines; refinement serves both needs with one mechanism, and wording stays a pure function
of the payload. State-dependent information the message needs is baked into the payload's
fields at refine time — errors stay self-contained after the parse (no `Arc<ParsingState>`
inside errors, no lazy `L`-dependent rendering).
*Rejected:* `L::format_message(&payload, &state) -> Option<String>` (subsumed by refinement;
a second wording path would reintroduce drift).

**Token layer joins the same model** — DECIDED (user + design sessions, July 2026). The two
`TokenErrorKind` variants become plain condition structs (`EndOfStreamAfterEscape`,
`ForbiddenChar`, each a `DiagnosticInfo` impl) wrapped by the enum, which gains
`Custom(Box<dyn DiagnosticData>)` for `Lang::scan_specials`; the enum loses `Copy`
(accepted), and `TokenError::kind()` returns a reference. The lift into diagnostics boxes the
built-in structs and *unwraps* `Custom`; a named `ParseError::from_token_error(…)`
constructor replaces the lift currently duplicated at `try_peek` (`constructs/mod.rs`) and
the content-loop recovery arm (`nodes_parser.rs`).
*Rationale:* `scan_specials` participates in the recovery protocol but could only lie with
tokenizer-internal kinds; one extension mechanism (`DiagnosticData`) serves both layers,
while the token layer keeps a concrete matchable enum for the recovery protocol.

**`Diagnostics` retention is capped; collection rendering shares line indices
(`render_all`)** — DECIDED (user, July 2026, Action-06 review). Two bounded-resource
fixes in one: (i) `Diagnostics` retains at most `limit` items (`with_limit(n)`;
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
*Rejected:* an unbounded default (the failure mode is silent and input-controlled), and
a public `DiagnosticRenderer` type (no second consumer yet; `render_all` covers the
need — promote the cache if one appears).

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

**Map containers after hashbrown (`BTreeMap` vs `HashMap`)** — DECIDED in part (user +
discussion, July 2026). hashbrown entered the tree for the engine's `StateMemo` (which needed
its `Equivalent` borrowed-key seam); that does not make it the default map. Choose per map by
use: `MapResolver` and the `CallableTypeId`-keyed maps stay `BTreeMap`; `Library`'s inner
name→spec map is the one hash-worthy candidate (string keys, one lookup per callable
invocation, potentially hundreds of entries) but is deferred to the planned structural revisit
of `library`.
*Rationale:* the `CallableTypeId`-keyed maps hold a handful of entries, where a `BTreeMap`
lookup is one or two integer comparisons — hashing gains nothing. Two non-obvious costs of
hashing to weigh whenever this is reopened: (a) iteration order becomes nondeterministic and
varies per process (hashbrown's default foldhash seeds from a static's address under
`no_std`), which any future "list defined names" API or snapshot test would inherit — sort at
the boundary if so; (b) `no_std` hash seeding has no OS entropy, so if untrusted documents
ever *insert* into a map (e.g. `\newcommand` definitions into a runtime library), collision
DoS becomes theoretically possible, whereas `BTreeMap` guarantees O(log n) worst case.
*Also decided:* public APIs must not name a concrete map type — `MapResolver`'s
`From<BTreeMap<String, String>>` was generalized to `From<I: IntoIterator<Item = (String,
String)>>` (July 2026) so the backing container stays an implementation detail; exposing
`hashbrown::HashMap` in a signature would couple the public API to hashbrown's 0.x semver
churn (0.14→0.15 already swapped default hashers).
*Revisit if:* profiling flags `Library` name-lookup cost, or the `library` structural revisit
lands.


### 3.10 Naming

Decided conventions (NAMING_STRATEGY.md, Dec 2025; two examples revised July 2026):

- **No `Latex` prefixes** — the library is markup-generic (`Token`, not `LatexToken`).
- **Specificity over brevity** where confusion is possible: `ParsingStateDelta` not
  `StateDelta`.
- **Context makes qualifiers redundant — unless sibling vocabulary competes in scope**:
  Dec 2025 chose `Arguments` over `ParsedArguments`; reversed July 2026 (current-level
  review) because the spec-side `ArgumentSpec`/`ArgumentParserSpec` vocabulary now coexists
  wherever the parsed records appear, and pylatexenc parity favors `ParsedArguments`.
  (`ArgumentStructureSpec` — the old clarity-over-brevity example — was dropped with the
  argument-model rebuild, §3.4.)
- **Collision avoidance beats tradition**: `Language<L>` replaces March's `FLMEnvironment`
  (fatal collision with `EnvironmentSpec`/`EnvironmentNode`); `ConstructParser` avoids clashing
  with any high-level `Parser` type; `Lang` replaces `LanguageSpecification` (too long for a
  parameter appearing in nearly every signature).
- **Phase 6 plan session (July 2026):** `ConstructParserResult<T>` (= `Result<T, ParseError>`)
  over the sketched `ParseOutcome` — unambiguous next to the engine-level `ParseResult`;
  clarity over brevity. `NodesParser` over `ContentParser` — the regions session gave
  "content" a precise technical meaning (`ContentNodes`, designated argument/slot content)
  that a general nodes parser has nothing to do with. `StopCause` for the parser-returned
  ending cause; `Invocation` for the resolved-invocation value; `make_*` for factory hooks
  (`make_invocation_parser`, `make_paragraph_break_node`).

When naming something new: check NAMING_STRATEGY.md, then ask "does this collide with or
shadow an existing concept in LaTeX terminology or in this codebase?"

### 3.11 Crate organization and dependency model

**Three strata + three rules replace the strict L0–L7 layer ladder** — DECIDED (user-led,
July 2026; ARCHITECTURE.md §3 revised accordingly).
S0 *foundation* (Lang-free, a true DAG: source, error/diagnostics, `Span`/`Token`/`TokenKind`,
`TokenRules` + `PrefixTable` + the concrete scanning core, `TextContent`); S1 *core* (a single
mutually-recursive stratum: `Lang` + `NodeExtTypes`, state, spec/library, node, constructs,
engine — modules are topics for navigation, not dependency ranks); S2 *presets*. Three
enforced rules: (1) S0 never names `Lang` (import-checkable); (2) S1 never names a preset
(import-checkable); (3) the runtime ownership graph is acyclic — nodes → {states, specs,
sources}; states → specs; specs → parsers; sources → sources; no runtime value references
nodes (field-inspection-checkable).
*Rationale:* the discussion started from "`Language<L>` is listed at L6 but its information is
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
*runtime ownership* graph (must stay acyclic — rule 3, generalizing §3.1's
sources-never-reference-nodes invariant); and the *build order* (§9 phases sequence concrete
machinery, which stays DAG-shaped even where signatures are mutually recursive — Phase 2's
tokenizer runs against a hardcoded `TokenRules`). Within S1 the useful distinction is by
*role* — data / contracts / standard machinery / orchestration — not by rank.
Consequences worth pinning:
- `TokenRules` and `PrefixTable` are *defined* in the token topic and merely *stored*/cached
  by `ParsingState`. *(Revised July 2026, token-design review: the token topic is wholly
  **S1** — tokens are generic over `L` (`Specials` carries its spec) and token errors may
  grow state context; the earlier "scanning core is S0" split is superseded, and `Span`
  moved to the source topic. S0-testability was traded for state-context freedom; a trivial
  test `Lang` restores it at negligible cost. See §3.2.)*
- The `TokenReader<L>` trait keeps `&ParsingState<L>` (not `&TokenRules`) in `peek`:
  it is the documented catcode escape hatch, and such a reader keeps its tables in
  `L::StateExt`; narrowing to the rules would sever the escape hatch from language state.
- `Lang` and `NodeExtTypes` are defined in the core next to the state types
  (`finalize_transition` names `StateData`/`ParsingState`, fixing their home); `NodeExtTypes`
  does not move into `node/` despite its meaning being a node concern — that would recreate a
  cycle for cosmetics.
- `Language<L>` contributes at exactly one moment: seeding the initial state (default rules,
  base libraries, default ext) at session start.
*Rejected:* renumbering/reshuffling layers (no assignment makes an SCC a DAG); collapsing
everything into one stratum (loses the two boundaries that are real and checkable: the
Lang-free line and the preset line); moving `TokenReader` "up a layer" away from `Token`
(the trait/impl split by rank served no invariant and read as unnatural); narrowing the
`TokenReader` contract to `&TokenRules` to keep it "L1" (see above).
*Revisit if:* the crate is ever split into multiple crates — crate boundaries force true
DAGs; S0 is the natural split candidate, while S1 cannot be split along topic lines.

---

**Repo layout: virtual workspace, every crate in its own subfolder** — DECIDED (user-led,
July 2026). The root `Cargo.toml` is a virtual manifest (`[workspace]` only); `techy` lives
in `techy/`, alongside `techy-derive/`, with a future CLI/instantiation crate as a third
sibling. Shared metadata (version, edition, `rust-version`, authors, license, repository) is
inherited via `[workspace.package]`; profiles live in the root manifest (the only place
they are honored). *Rationale:* the previous root-package layout (`techy` at the repo root
hosting `[workspace]`) is fine for "lib + satellite" (cf. thiserror, regex) but degrades at
three crates: root-level `cargo build`/`test` target the root package only, silently
skipping other members, whereas a virtual workspace targets all members by default; and
with the package root equal to the repo root, every repo-level file (ARCHITECTURE.md,
dev-docs/, TODO_Big.md, …) is a packaging candidate needing a perpetually-honest `exclude`
list — with a subfolder, `cargo package` ships exactly the crate's files. This is the
serde/tokio/clap layout, and serde is precisely our shape (lib + derive companion).
Non-obvious pitfalls pinned during the move: (1) a virtual root has no `edition` to infer
the dependency resolver from, so `resolver = "2"` must be explicit — v1 would unify
features across the no_std-leaning core and std-linking members; (2) `include_str!`'d docs
(`docs/guide.md`, `docs/parsing-model.md`) must live *inside* the package directory or
`cargo package` breaks, so `docs/` moved into `techy/docs/`; (3) `readme = "../README.md"`
works from a subfolder (cargo copies it into the package). The CLI "linking std" is
orthogonal to layout — governed per-crate by features, not folder placement. *Rejected:*
keeping the root-package layout until the CLI lands (the move only gets more expensive);
a `crates/` super-directory (needless nesting at three crates; plain siblings suffice).

---

### 3.12 Documentation

**Narrative docs included with rustdoc, not a separate site** — DECIDED (user-led, July 2026).
API documentation is rustdoc; narrative pages (usage, concepts, design patterns — the role
of pylatexenc's Sphinx pages) are markdown files in `docs/`, rendered as doc-only modules
under `techy::guide` via `#[cfg(doc)]` + `#[doc = include_str!(...)]` in `lib.rs` (the clap
pattern). Stubs landed July 2026.
*Rationale:* one site and one search index, and — decisively — compiler-checked intra-doc
links plus doctest-compiled examples: during the ongoing review-and-rename churn, links and
code samples in a separate book would silently rot, whereas rustdoc breaks the docs build
instead. Zero extra toolchain; docs.rs hosts it on publish.
*Rejected:* mdBook alongside rustdoc — proper book navigation, but a second toolchain and
unchecked book→API links. Not precluded: the `docs/*.md` sources move into mdBook chapters
nearly verbatim if the narrative later outgrows rustdoc.
*Revisit if:* the guide needs ordered, book-style chapter navigation that rustdoc's
module-shaped layout can't carry.

### 3.13 The latexlike preset

**The preset's group taxonomy is two classes: `Content` and `Math`** — DECIDED (user,
July 2026, Phase 7.5 checkpoint).
`GroupType` has a *single* math class covering `$…$`, `$$…$$`, `\(…\)`, `\[…\]`; inline
vs. display is neither a class nor a mode. Display-ness is a delimiter fact, read off the
node's recorded delimiters by the preset sugar `NodeRef::math_style()` →
`MathStyle::{Inline, Display}` (pylatexenc parity: `LatexMathNode.displaytype` is likewise
delimiter-derived).
*Rationale:* the class taxonomy cuts at parse-behavior joints, and inline and display math
parse identically — same interior `Mode::Math`, same definition visibility — so a split
would do no parse-time work; it would also break the class/mode symmetry (three classes
over two modes).
*Rejected:* the plan sketch's `MathInline`/`MathDisplay` split (typed display-ness that a
rule author declares — its one real advantage: embedder-registered math delimiters would
classify themselves, where `math_style()`'s table answers `None`); a `Bracket` class and
`[]` in the default rules — `[`/`]` are plain characters in LaTeX outside
optional-argument positions (`a [b] c` is text), and `OptionalGroupArgumentParser`
recognizes them through its own per-spec `temporary_groups` rule, so neither the class nor
the base rule has a consumer (user-caught; the original plan listed both).
*Revisit if:* a consumer needs typed display-ness on custom math delimiters (the split
stays open under `#[non_exhaustive]`).

**Inside math the math delimiters stop opening (no nested math); a stray `$` is
forbidden** — DECIDED (user, July 2026, Phase 7.5 review).
`LatexlikeDriver::group_interior_delta` for a math rule returns, besides `mode(Mode::Math)`,
a `TokenRulesOverrides` derived from the **outer** state: the interior's group rules are the
outer rules minus the `Math` openers, and `$` is merged into the outer `forbidden_chars`.
The descent invariant still installs `expecting_group_close`, so the current group's close
works; a `$` that is *not* that close is a forbidden-char diagnostic, not the opener of a
nested inline group. Example: tolerant `$$a$b$$` is one display group over `a$b` (one
diagnostic), never a display group left unclosed around a spurious inner `$…$`.
*Rationale:* LaTeX forbids nested math; without this a lone `$` inside display math opened a
fresh inline group and consumed the trailing `$$` as two separate closes, leaving the
display group unclosed (the surprising tree the 7.5 review flagged). Deriving from the outer
state (not the seed) preserves any embedder rule changes in force at the `$`, and *merging*
(not replacing) `forbidden_chars` keeps the embedder's forbidden set. `\(`/`\[` inside math,
their openers likewise gone, fall through to the command path (a stray single-char command)
— acceptable.
*Rejected:* leaving the math openers active in math (the pre-review behavior, with its
unclosed-group trees); a bespoke "no nested math" condition for `\(`/`\[` (the generic
unresolvable-command / forbidden-char diagnostics already localize the error).

**Preset vocabulary names are bare and module-scoped** — DECIDED (user, July 2026, Phase
7.5 checkpoint).
`GroupType`/`CallableType`/`Mode` with short variants (`Content`/`Math`;
`Macro`/`Environment`/`Specials`; `Text`/`Math`), reading as `latexlike::Mode::Math`;
preset items are **not** re-exported at the crate root. All three enums are
`#[non_exhaustive]` (verbatim-ish variants expected in 7.7); in-crate matches stay
exhaustive on purpose, so a new variant surfaces every site.
*Rationale:* NAMING_STRATEGY principle 4 — no sibling vocabulary competes, since the core
has only the *associated types* (`Lang::GroupTypeId` …), never concrete types with these
names; the module path disambiguates everywhere else.
*Rejected:* `Latex`-/`Latexlike`-prefixed enum names (length that does no disambiguation
work inside a namespaced preset); the `MACRO`/`ENVIRONMENT`/`SPECIALS` spelling (an
artifact of the u32-const test era, not Rust variant style).

**The seed ships a `"base"` package: pylatexenc's default specials as data** — DECIDED
(user, July 2026, Phase 7.5 checkpoint; package name user-chosen).
`Latexlike::initial_state_data()` seeds the scope stack with one package `"base"` holding
zero-argument specials for `&`, `~`, ``` `` ```, `''`, `--`, `---`, `` !` ``, `` ?` `` —
pylatexenc's default context (its *latex-base* + *nonascii-specials* categories). Droppable
wholesale by name (`ScopeOp::Unload`), shadowable per-trigger by pushing a provider.
Macro/environment definitions deliberately stay out until the std-DB port. The typography
ligatures (``` `` ```, `''`, `--`, `---`, `` !` ``, `` ?` ``) are registered **text-mode
only** (they carry no math meaning — inside `$…$` they stay plain chars); `&` and `~` are
visible in every mode (7.5 review; the per-entry mode gate below). `\begin`/`\end` (7.6)
stay all-modes so math environments still open in math.
*Rationale:* out-of-the-box parity with pylatexenc's default node shapes for these
triggers — with one deliberate exception: the `\n\n` paragraph-break special of
pylatexenc's *latex-paragraph* category is omitted, so a multi-newline break is a
whitespace chars node here (`enable_multi_newline_paragraphs`), not a specials node. The
multi-character ligatures exercise the 7.3 longest-match fold (`---` beats `--`) in real
defaults rather than only in tests.
*Rejected:* an empty seed stack (purest, but `~`/`&` would parse as plain chars out of the
box — silent divergence from pylatexenc); seeding only `&`/`~` (leaves the fold's only
real-data consumer test-side).

**Per-definition mode visibility on `Package` — the fine gate under `set_visible_modes`**
— DECIDED (user, July 2026, Phase 7.5 review).
`Package::insert_in_modes`/`insert_specials_in_modes` attach an optional mode list to a
*single* definition; `retrieve_spec`/`scan_specials` check it against `ParsingState::mode`
under the pre-existing package-level `set_visible_modes` — **both** gates must admit the
mode (`None` = every mode the package is visible in). One loadable, unloadable package can
then hold text-only ligatures and (later) math-only `^`/`_` scripts together.
*Rationale:* the base package must keep `\begin`/`\end` visible in math while hiding the
text ligatures there — package-level visibility alone cannot express that without splitting
`"base"` into several names, which would break the single-name `Unload("base")` contract
and the specials-as-one-category model. Per-entry visibility is the minimal mechanism that
keeps one package. The trigger-char union deliberately stays mode-blind (a hidden entry's
first chars remain in the filter; its scan declines) — the established 7.3 caching contract.
*Rejected:* multiple mode-scoped seed packages (changes the unload semantics, multiplies
seed names); a whole-package flip to text-only (would hide `\begin`/`\end` in math too).

**Default whitespace is the ASCII set, not Unicode-aware** — DECIDED (user, July 2026,
Phase 7.5 checkpoint).
`default_token_rules()` sets `WhitespaceRules.chars` to the six ASCII whitespace
characters (space, tab, `\n`, `\r`, vertical tab, form feed); a Unicode space (NBSP
U+00A0, U+2028, …) is ordinary content, diverging from pylatexenc's Unicode-aware
`str.isspace()` (which swallows e.g. an NBSP after `\emph` as post-macro space, yielding a
different node shape).
*Rationale:* the `WhitespaceRules` model is a fixed char-set membership test, and an ASCII
set is deterministic and needs no Unicode tables; the divergence is narrow (only exotic
Unicode spaces in a source) and now recorded rather than silent.
*Rejected:* matching pylatexenc by widening `WhitespaceRules` to a `char::is_whitespace`
predicate — deferred as an unforced core-model change; revisit if real inputs demand it.

**`NodeRef` preset sugar is inherent, not an extension trait** — DECIDED (user, July 2026,
Phase 7.5 checkpoint).
The accessors (`is_math_group`, `math_style`, `macro_name`, `environment_name`,
`specials_name`) are inherent methods on `NodeRef<'_, Latexlike>`, written in the preset
module — legal because the preset shares the crate with `node`.
*Rationale:* zero-import ergonomics on the majority path; an out-of-crate language (FLM)
must use an extension trait regardless, and that pattern needs no in-tree demonstration.
*Rejected:* a `LatexNodeRefExt` trait for the preset (a `use` tax on every consumer, buying
only symmetry with a constraint the preset does not have).

**`\begin`/`\end` dispatch is scope-stack data: ordinary `Macro` entries of `"base"`** —
DECIDED (user, July 2026, Phase 7.6 checkpoint, decision (a)).
`BeginSpec` (the environment composition) and `EndSpec` (orphan-`\end` diagnostics) are
registered under `begin`/`end` in the seed package like any definition — resolvable
through the unchanged `LatexlikeDriver::resolve_command`, shadowable, and unloadable
(`Unload("base")` removes environments along with the specials; pinned in a test).
Consequence: the `Invocation` arrives typed `Macro`, so the composition stamps
`CallableType::Environment` (and the environment's own name and spec) on the staged
node itself — the dispatcher's identity appears nowhere in the tree.
*Rationale:* the phase's direction is "everything through the stack" (even specials are
data); a hardcoded `resolve_command` arm would be the one un-shadowable definition in
the language.
*Rejected:* the test-lang rehearsal's driver arm (`if name == "begin"`), which made
`\begin` structural syntax.

**The environment spec surface: `EnvironmentSpec` wraps a dyn `EnvironmentBehavior`;
`with_body_delta` overrides by adapter** — DECIDED (user, July 2026, Phase 7.6
checkpoint, decision (b); executes the §3.4 funnel and D4's defaulted
`make_body_parser()`).
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
*Rejected:* a delta field on the wrapper next to the behavior (two sources of truth); a
`Result`-returning builder gated on the behavior being the standard one (ergonomics
tax on the 99% case).

**`MacroSpec`/`SpecialsSpec` are real types, not constructor functions** — DECIDED
(user, July 2026, Phase 7.6 checkpoint, decision (c)).
Both are `StdCallableSpec`-shaped declarative types whose `stack_frame_title` speaks
the preset vocabulary ("macro ‘\frac’", "argument #1 of macro ‘\frac’",
"specials ‘~’"); `base_package()`'s specials switched to a shared `SpecialsSpec`.
Generic specs remain first-class everywhere.
*Rationale:* functions returning `StdCallableSpec` would leave tracebacks saying
"callable ‘…’" — the vocabulary hook exists precisely for presets — and concrete preset
types are stable downcast targets for later `finalize_node` work.

**Orphan-`\end` recovery: dispatch-time diagnosis, chars over the consumed extent** —
DECIDED (user, July 2026, Phase 7.6 checkpoint, decisions (d)/(e)).
Inside a body, `\end` is the stop condition and never reaches resolution, so a
*dispatched* `\end` is always an orphan: `EndSpec`'s parser reads the rigid name group
when present, records `OrphanEnd` (message quoting `\end{name}` when the name parsed),
and tolerantly stages the consumed extent as one `Chars` node — `\end{name}` whole, so
`{name}` is not re-parsed as a stray group. Preset condition ids are namespaced
**`latexlike.environments.*`** (`malformed-begin`, `unknown-environment`, `orphan-end`;
user-chosen over `latexlike.begin.*`/`latexlike.end.*`). Implementation fact worth
remembering: the tolerant chars fallbacks (malformed `\begin`, nameless orphan `\end`)
must cover the trigger's syntactic *post-space* too — the token span includes it, and
trimming it would break the sibling partition invariant; the §G rehearsal had the same
shape. The body-unwind path that leaves a stray `}` for the root re-crosses the
recorded root byte-accounting break (stray bytes dropped from the tree) — accepted, no
new mechanism.

**The verbatim family (Phase 7.7): recipe → production parsers, group+chars shapes** —
DECIDED (July 2026, 7.7 landing; N7 of ParserLibraryParity.md).
`constructs::verbatim_parser` promotes the pinned recipe (§3.2 Action-02 entry; the
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
  (the §3.5 "noise policy is inseparable from argument syntax" case in the flesh).
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

`GroupType::Verbatim` joins the preset vocabulary (the 7.5 "verbatim-ish variants"
slot); **no `Mode::Verbatim`** — verbatimness is rules-borne (a derived-state fact
scoped to the region), not a mode the scope stack or content interpretation keys on.
*Rejected:* a char-level reader API (pylatexenc `next_chars`; the recipe already
delivers per-byte `Char` tokens); a shared "base parser" type with pluggable stop
conditions (two users, one private loop helper + the public delta builder suffice).

**`EnvironmentBody.content`: the body parser designates the slot's content** — DECIDED
(July 2026, 7.7 landing).
`EnvironmentBody` gains a `content: ContentNodes` field (and drops `Copy`); the
`\begin` composition (and the §G test composition) mints the `"body"` slot record from
it instead of designating all-children itself. Forced by the newline gobble: pylatexenc
*drops* the newline right after `\begin{verbatim}` from its chars node, but techy trees
keep every byte — so the gobbled newline is **staged as a leading whitespace `Chars`
node inside the body `List` and designated out of the content**. Putting it anywhere
else breaks an invariant: excluded from the `List`'s span it either gaps the callable's
children block (arguments before the body, the `lstlisting[opts]` shape — invariant 3)
or un-tiles the `List` interior (invariant 2). The standard `EnvironmentBodyParser`
designates all children (7.6 behavior unchanged); "which body nodes are content" is the
staging parser's knowledge, exactly as for arguments (§3.5 parse-time designation).
*Rejected:* letting the newline ride the scaffolding gap (works only for argument-less
environments; would weaken invariant 3 to legalize the rest); an `Option`al designation
field (the default parser knows its answer — make every producer say it).

**The argument-code factory: `latexlike::argument_specs`** — DECIDED (July 2026, 7.7
landing; N8 of ParserLibraryParity.md).
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
  with the 7.8 accessor work if extraction wants it.
- `s`/`*`, `t<c>` → `MarkerArgumentParser`; `v`/`v<c1><c2>` →
  `VerbatimArgumentParser::new(GroupType::Verbatim)` (+ `.with_delimiters`).

Factory specs carry no names and no per-argument deltas (attach via `ArgumentSpec`
builders). No flyweight cache and no singletons: specs are built once per language.
`e{…}` [N3] and `AnyDelimited` [N2] stay deferred with their parsers.
*Rejected:* accepting a `&[&str]` list-of-codes signature alongside (one grammar, one
entry; the string form covers the deferred `e{…}` shape too when it arrives).
*(Reversed July 2026 — the list form is now primary; see the list-primary revision
entry below.)*

**`GroupArgumentParser`: the single-expression fallback becomes the orthogonal
`expression_fallback` knob** — DECIDED (user, July 2026, follow-up session to the 7.7
landing).
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
— is now documented on both parser types (it was previously only implicit in §3.5 and
the parity table).

**Paragraph-break emission is a driver flag: `ParagraphBreakStyle` on
`LatexlikeDriver`** — DECIDED (user, July 2026, Phase 7.9 session).
`with_paragraph_break_style`: `Chars` (default — the core hook's whitespace-chars
shape, pylatexenc-legacy's) or `Specials` (pylatexenc-modern's shape — a
`Specials`-formed callable named by the **canonical** `"\n\n"` vocabulary key, its
span covering the actual whitespace run, its argument-less `SpecialsSpec` minted per
break). Node-level only: the token stays `ParagraphBreak`, and the emitted name lives
in no provider, so it is invisible to `iter_symbols` enumeration.
*Rationale:* (user) paragraph breaks are special enough to warrant a dedicated driver
flag; correlating the shape with package contents would be error-prone and
counterintuitive — and factually dead configuration: the tokenizer detects paragraph
breaks within leading whitespace, *before* the specials scan can run, so a
package-registered `"\n\n"` specials entry could never fire.
*Rejected:* probing the scope stack for a `"\n\n"` entry inside
`make_paragraph_break_node` (the first sketch — package-correlated behavior, plus a
swallowed `ProviderError` in a hook with no diagnostic channel); reordering the
tokenizer's detection priority (specials before paragraph breaks) — tangles
whitespace skipping for one preset feature; caching the spec `Arc` on the driver —
would cost `LatexlikeDriver` its `Copy`/`Eq` config-value nature to save a
negligible per-break allocation (specs are behavior, never compared).
*Revisit if:* a *scoped* shape switch is ever wanted — the flag is driver-global by
design; per-scope suppression already exists orthogonally through the
`enable_multi_newline_paragraphs` gate (verbatim's features-off state uses it).

**The 7.9 acceptance suite: an integration-crate port of pylatexenc's walker
slice** — DECIDED (user, July 2026, Phase 7.9 session).
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
list-of-codes signature (since landed — next entry).

**`argument_specs` goes list-primary; the compact string becomes
`argument_specs_from_str`** — DECIDED (user, July 2026, follow-up session; revises
the 7.7 factory entry above and reverses its list-signature rejection).
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
*Rejected:* a typed `ArgumentCode` enum as the primary currency (duplicates the
parser vocabulary one level up; hand-built `ArgumentSpec`s with concrete parsers are
already the fully-typed path — the factory's value *is* the compact codes).

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
  semantics into the tokenizer; `\begin` is an ordinary command, environments are a parser concern.
- **Invocation-form ids (`CallableTypeId`) on tokens resolved at parse time** (§3.2) —
  invocation form is resolution output, not tokenization output; carrying it on `Command`
  tokens re-creates the "token says MACRO, node says ENVIRONMENT" wart. (Scoped: does
  *not* apply to `Specials`, where recognition *is* resolution — the token carries the
  full `ResolvedCallable` pair, §3.2 July 2026 amendment.)
- **Whitespace as its own token kind** (§3.2) — every construct parser's peek grows a
  "maybe whitespace first" case; pre/post-space spans localize the cost in the tokenizer.
- **Uniform `post_space` field on `Token`** (§3.2) — post-space is a per-kind syntactic
  fact (commands, comments); an accessor serves `move_past`, the field taxed every token.
- **Maximal-run `Chars` tokens** (§3.2) — a token is an atomic unit; run-splitting
  machinery (conservative stop sets) bought speed the node level didn't need and cost
  char-by-char construct parsing.
- **Specials trigger strings enumerated in `TokenRules`** (§3.2, §3.3) — trigger sets can
  be large and library-driven; recognition belongs to the preset (`Lang::scan_specials`,
  name + spec in one call), guarded by the cached `TriggerChars` filter.
- **A strict dependency ladder through the crate's middle (the old L2–L6 layering)** (§3.11) —
  the middle is a strongly-connected component by intention (each cycle edge is a decided
  feature); enforce the three real rules (Lang-free foundation, preset line, acyclic runtime
  ownership) instead of a fictional ranking.

---

## 5. Non-goals

Decided intentional limitations (PROPOSALS.md §4 gap analysis, reaffirmed July 2026):

- **techy is not a TeX engine.** No catcode system, no macro expansion engine, no conditional
  (`\if…`) evaluation, no full primitive set. Target use cases are structural parsing for
  conversion, analysis, and tooling — pylatexenc's niche, and FLM's need.
- Escape hatch, documented: anyone needing catcode-like tokenization implements `TokenReader`.
- `\newcommand` **is** supported, but at the parse level (a library-extension delta defining a
  new spec), not as token-stream expansion.
- Deferred: memory-mapped sources (an embedder can already hand in mmap-validated text;
  the `SourceContent` trait seam was retired as information-equivalent to `&str`, §3.1),
  streaming/incremental parsing, `Rc` pointer genericity (§3.7).

---

## 6. Open questions

Current list — remove entries as they are settled (move the outcome into §3):

1. ~~**`SpecLookup` semantics and behavior**~~ — settled July 2026 (Phase 4 design
   session): `CallableQuery`-based lookup with explicit `CallableSyntax` + optional token,
   and stack-built-in per-`CallableTypeId` fallbacks. Outcome moved to §3.4.
1b. **Precompiled-table merging (`PrefixTable`++)**: detection consults several per-state
   structures (group-delimiter `PrefixTable`, specials `TriggerChars`, per-rule command
   escape and comment-start checks). Worth evaluating a single merged first-character /
   prefix table per state once the hot loop can be profiled (noted July 2026, user
   request; also flagged in ARCHITECTURE.md §token). Not a design blocker.
2. ~~**`ArgsLayout` / children encoding in flat `NodeData`**~~ — settled July 2026 (Phase 5
   design session): one node per region (one child per present argument, one `List` child per
   slot), with presence/offsets and per-instance syntax in `ArgsLayout`/`SlotsLayout`.
   Outcome moved to §3.5.
3. ~~**Top-level convenience API**~~ — settled July 2026 (Phase 6 plan session): no facade,
   and no `Language<L>`/`parse()` convenience entry point in Phase 6 at all — `ParserSession`
   is the root object; convenience API deferred to the phase that demonstrates the need
   (Phase 7+). Outcome moved to §3.6.
4. **`CompactString`**: plain `String` initially; whether a small-string optimization ever pays
   for delimiter/specials storage is a profiling question, not a design question.
5. ~~**`Comment` node recomposition fields**~~ — settled July 2026 (Phase 6 plan session):
   `Comment` grows `start` + `post_space` per-instance syntax fields (notes Q4, Option A).
   Outcome moved to §3.5.
6. ~~**Disabling specials by delta**~~ — settled July 2026 (child-state design session
   follow-up): `enable_specials` joins the `TokenRules` `enable_*` flag family; `freeze()`
   stores the empty `TriggerChars` when disabled, so the scan hook is unreachable. Outcome
   moved to §3.2.
7. ~~**LibraryStack structure and delta expressiveness**~~ — settled July 2026 (Phase 7
   plan session): the scope-stack redesign — dyn `SpecsProvider` entries,
   `Package`/`Scope` standard impls, in-stack fallback providers, definition/stack delta
   ops replacing `push_libraries`. Outcome moved to §3.4. (The seed-path half had been
   settled earlier — §3.3, "Seed states are crate-frozen `Lang` data".)
8. **Structured-diagnostics implementation details** (opened July 2026 — decisions in §3.8,
   plan in CodeReportAction_01.md). Sub-items settled July 2026: MSRV bumped to 1.86 (dyn
   trait upcasting); `FrameTitle` variants as sketched; `DiagnosticValue` barebones with no
   float variant (serialize as string if ever needed); `diagnostic_catalog()` dropped.
   Remaining: the identifier scheme (`core.<area>.*` / `<preset-name>.<namespaced-name>`) is
   provisional — a final naming/identifier pass is due before a public release makes the
   strings semver surface.

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
