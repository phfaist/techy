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
mangles the same pathological shape *with* diagnostics (the brace group inside the
nested bracket level surfaces `UnexpectedGroupClose` and decision-8 unwinding takes
over). A consumer needing every-depth protection drives the nesting recursion itself —
the parser-composition escape hatch above.
*Revisit — planned mechanism, direction decided (user, July 2026, follow-up
discussion):* the one-level pitfall is to be closed by **temporary group rules scoped in
state data** — reversion by reconstruction (stripping), the only vehicle that reaches
depth N: the outer `Arc` sits N frames up and is unknowable to the descending site, and
caller-side descent policies are one level deep by design. Direction pinned: temporariness
is reified in **core rules data** (a `temporary_groups` list next to `TokenRules::groups`,
or a `transient` flag on `GroupRule`) — *not* a `Lang` callback recording a `StateExt`
flag (a core parser's parsing correctness must not depend on `Lang` cooperation — the
same ground the ChildStateSpec entry's `StateExt`-routing rejection stands on;
`finalize_transition` stays reserved for genuine language semantics) and *not* the
session (the session layer is pinned data-equivalent to `derived()` and may never alter
a resulting state). Stripping lives in the **pure derivation path**, keyed on the
`expecting_group_close` change: a derivation installing a *non-temporary* expected close
clears the temporary rules. The trigger self-disambiguates — a nested minted `[` installs
the temporary rule (kept, so brackets keep balancing), a brace installs a normal one
(stripped, so braces protect at any depth) — and remains a pure function of
`(base, rule)`, so the session's derivation memo is untouched. With it, `\item[a[b{c]}]]`
parses as expected — beyond pylatexenc, which mangles exactly that input (3.0a33
checked: childless nested group, leaked `]`). Open sub-questions for the implementing
session (the seam ships whole, 6.3 precedent): (i) the stripping site — a `derived()`
core rule vs. the `group_interior_state` delta construction; (ii) the fate of the
optional parser's `ChildStateSpec` wiring — the group half becomes redundant, and the
invocation half's `Fixed(outer)` revert differs observably from state-carried stripping
(an invocation inside `[…]` would tokenize its non-group tokens with the bracket rule
still in force), needing its own ruling; (iii) the encoding — rules-list vs. rule flag
(both mechanically break struct literals).

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
  `Lang::refine_diagnostic` pass applies. `StdInvocationParser`'s slots check reports the
  same condition.
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
*Future:* `#[derive(DiagnosticInfo)]` proc-macro, thiserror-style (`#[diagnostic(id = "…",
message = "…{field}…")]`; rustc's internal `derive(Diagnostic)` is prior art) — generates
identifier, message wording, and serialization keys from the struct definition, making drift
between them impossible; build-time dependency only, runtime stays zero-dep. Unsealing later
is non-breaking; re-sealing is not.

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
6. ~~**Disabling specials by delta**~~ — settled July 2026 (child-state design session
   follow-up): `enable_specials` joins the `TokenRules` `enable_*` flag family; `freeze()`
   stores the empty `TriggerChars` when disabled, so the scan hook is unreachable. Outcome
   moved to §3.2.
7. **LibraryStack structure and delta expressiveness** (opened July 2026, code-review
   follow-up session; the seed-path half is settled — outcome moved to §3.3, "Seed
   states are crate-frozen `Lang` data"). The delta vocabulary for libraries is push-only
   (`push_libraries`); the July 2026 audit found `LibraryStack::fallbacks`
   (per-`CallableTypeId` unknown-callable fallback specs) entirely delta-inexpressible,
   and the user wants deltas to become much more expressive about library manipulation
   generally — up to replacing the library wholesale in a state transition. Requires
   revisiting `LibraryStack`'s structure itself; until then, seed-side library/fallback
   setup is the `Lang` author's business inside `initial_state_data`.
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
