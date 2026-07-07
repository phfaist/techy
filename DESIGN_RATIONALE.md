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
  NAMING_STRATEGY.md > everything in `docs/archive/` (which includes SOURCE_ARCHITECTURE.md,
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
nothing. `SourceContent` trait abstracts backing storage so mmap can arrive later without
parser changes (DEFERRED until a real need). No file-system resolver is shipped (no_std
policy, §3.9): an embedder implements `SourceResolver` on its side, where the I/O capability
lives; the in-memory `MapResolver` covers tests and fully preloaded setups.

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
July 2026, Phase 1 kickoff). The trait exists (implemented by `str` and `String`) and
`SourceCursor<'s, C: SourceContent + ?Sized = str>` is generic over it, but `Source` stores a
concrete `String`, with all content access behind methods so the backing can later become
generic (mmap) without changing the public API. Explicitly: keep the enabling pattern, do not
implement mmap until a real need.

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
  literal `&`).
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
  returns a `SpecialsMatch` carrying name **and** spec in one call — scanning/lookup
  normalization or scoping mismatches are impossible by construction, and unknown-name
  fallback is the scan's own business (a `Specials` token's spec is never absent). It is a
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
  the terminator is end-of-line implicitly, independent of `WhitespaceRules`. Corner
  pinned: `a% c\n\nb` — the comment's terminating newline belongs to a `\n\s*\n` sequence,
  so the comment takes **no** post-space and the `ParagraphBreak` survives as its own
  token (TeX-observable behavior: the blank line still yields `\par`). Consequence:
  `CommentParser` is vestigial — comment nodes come straight from tokens.
- **Terminal `EndOfStream` token; `peek` never returns an `Option`.** `EndOfStream` is
  idempotent and its `pre_space` carries the input's final whitespace, so trailing
  whitespace reaches the node tree through the ordinary token path — the nodes parser
  never reaches around the reader into raw content (which a custom `TokenReader` might not
  meaningfully expose). Also serves as the recovery placeholder for a dangling escape at
  EOF (Phase 2 used an empty `Chars` token — impossible once `Chars` became `Char(char)`).
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
*Revisit if:* a streaming token source can't expose stable slices (then the `SourceContent`
boundary is the place to solve it, not the token type).

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
expects; a stray close needs no more); `expecting_group_close` holds the rule;
`GroupData.group_type` stays but records the class.
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

**`TokenRules::multi_newline_paragraphs` (renamed from `double_newline_paragraphs`)** —
DECIDED (user, July 2026, Phase 6 plan session). Any run of two or more newlines (however
many, with interleaved inline whitespace) forms one paragraph break; the old name misread as
"exactly two".

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
would cost `Sync`), and both derivations are cheap relative to a transition; revisit only if
transition cost ever shows up in profiles. `TokenRulesOverrides` collections are replaced
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
`ParsedSlot { spec, region: ChildRegion }`; a resolved `ChildRegion` =
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
3. *Callable post-space:* claimed **by the invocation parser**, not by the dispatch loop —
   peek + `move_to(tok, rewind_pre_space = false)`, packaged as a one-call helper.
   Accepted consequence: a custom parser that skips the helper corrupts nothing — the
   whitespace lands as following sibling content (a behavioral difference, not a broken
   invariant). Groups have no post-space (space after `}` is content). Comment post-space
   is the token's (newline + indentation, stopping at paragraph breaks).
4. *End of stream:* `EndOfStream.pre_space` materializes as a final whitespace-only `Chars`
   node.
5. *Partition invariant:* sibling spans partition the parent's *content interior* exactly —
   `List` bodies, `Group` interiors, the root. For callables: argument/slot regions tile
   the child list (builder-enforced), the children block is span-contiguous, and unrecorded
   rigid scaffolding is the reconstructible complement (previous entry). Checked
   mechanically by a test-utility `check_tree_invariants()` — deliberately a test aid, not
   builder law, so a future construct that legitimately breaks byte-accounting amends a
   test, not the architecture.

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
*Revisit if — **flagged, to check before Phase 6 closes**:* the per-invocation `Box`
allocation shows up in parse-throughput profiles (run a micro-benchmark; see
Phase6Execution.md). If it ever matters, the dispatch loop can special-case the default
path without touching the trait.

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
`data.spec`, downcasts to its own spec trait, attaches its `flm_specinfo`-like ext), so the
core needs no spec-level hook at all; and *uniform* per-node initialization (fields every
node of a language carries) gets a natural home, which a callables-only spec hook could
never provide.
*Consequences:* the hook must tolerate re-staging (transform-built trees pass nodes through
a new builder — finalization runs again on already-finalized data; implementations must be
idempotent); it runs on speculatively staged nodes that may be abandoned (harmless — they
drop unreachable); the builder grows a small staged-node read view (also wanted by
node-based stop predicates, below).
*Rejected:* spec-level finalize in core (callables-only; custom invocation parsers must
remember to call it); a `ParseContext`-side helper (forgettable, and transforms bypass it).

**`Lang::resolve_command` hook** — DECIDED (user, July 2026, Phase 6 plan session; Phase 6
notes item C1 as sketched). `Command` tokens resolve through
`fn resolve_command(state, &token) -> Option<ResolvedCallable<Self>>`
(`{ callable_type, spec }`); typically dispatches to the state's libraries via
`CallableQuery { syntax: Command { escape_char }, … }` — the token now carries its escape
char (§3.2). Default `None` → the nodes parser diagnoses and recovers (§3.8). Specials need
no hook: recognition = resolution; the token already carries its spec.
*Rationale:* the dispatch loop needs `(CallableTypeId, spec)` for command tokens and the
core cannot know a preset's type ids; follows the `scan_specials` precedent (a `Lang` hook,
recognition kept close to resolution).

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
- *token condition* — a small closed enum (`Command(name)`, `GroupClose(group_type)`,
  `ParagraphBreak`, …) **or** a programmatic predicate (`Fn(&Token) -> bool`);
- *node condition* — a programmatic predicate consulted after each node is assembled,
  receiving (node count, view of the just-staged node) — covers pylatexenc's
  `stop_nodelist_condition` uses (stop-after-one-node, `LatexSingleNodeParser`).
Semantics pinned: a token-condition match leaves the token **unconsumed** (the caller
consumes and interprets it — pylatexenc's `handle_stop_condition_token` ambiguity removed);
a node-condition match includes the triggering node and stops after it; conditions are
tested only at the parser's own nesting level (nested groups are consumed whole by the
group parser, so an `\end` inside a brace group cannot terminate an environment body).
`NodesParser` returns its `StopCause` — `StopConditionMet` / `EndOfInput` /
`UnexpectedGroupClose` — and the *caller* decides which causes are errors (§3.8).
Deliberate deviations from pylatexenc: the node predicate sees (count, last node), not the
whole node list on every iteration (pylatexenc's `stop_nodelist_condition(nodelist)`
invites O(n²) rescans); predicates live only in tier-2 parser temporaries, never in spec
data (§2.1).
*Rejected:* a declarative stop-condition language in spec data (the Q1 ruling: terminators
are parser business); closure storage in specs.

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

**No `Language<L>` type in Phase 6; `ParserSession` is the root object** — DECIDED (user,
July 2026, Phase 6 plan session; amends Phase 6 notes item C5 and the §engine timing).
Phase 6 ships `ParserSession` (builder + diagnostics + `Recovery` policy), driven directly
by tests; the `Language<L>` runtime bundle and any `parse()` convenience entry point are
deferred to the phase that demonstrates the need (Phase 7 at the earliest) — convenience
code is not written before its convenience is demonstrable. Consequence: type-id interning
stays deferred exactly as §3.4 recorded (it presupposed `Language`). The "`Language<L>`
owns no per-parse state" principle above is untouched — it binds the type when it arrives.

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
*Landed (Phase 2, July 2026):* `TokenError<'s>` = structured `TokenErrorKind` (closed enum:
end-of-stream-after-escape, forbidden-char — replaces pylatexenc's stringly `error_type_info`)
+ byte `Span` + `Option<TokenRecovery<'s>>`, where `TokenRecovery` = placeholder token + an
explicit `resume_pos` (the two can differ: after end-of-stream-after-escape the placeholder is
an empty chars token but reading resumes at end of input, per pylatexenc). Token-level errors
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
- **Invocation-form ids (`CallableTypeId`) on tokens** (§3.2) — invocation form is
  resolution output, not tokenization output; carrying it on tokens re-creates the
  "token says MACRO, node says ENVIRONMENT" wart.
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
- Deferred, with trait boundaries already in place so no parser changes are needed later:
  memory-mapped sources (`SourceContent`), streaming/incremental parsing, `Rc` pointer
  genericity (§3.7).

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
