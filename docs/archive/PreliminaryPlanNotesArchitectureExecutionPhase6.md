# Phase 6 — Preliminary Design Notes (pre-plan cogitations)

**Status: WORKING NOTES, July 2026 — for discussion before the Phase 6 implementation plan.**
Written after re-reading ARCHITECTURE.md (§constructs, §engine, §nodes invariants, §9),
DESIGN_RATIONALE.md (§3.2, §3.4, §3.5, §3.6, §6), and the implemented Phase 1–5 source.
Nothing here is decided; the numbered **[Q1]–[Q4]** items are the questions I raised, with
options and my recommendation. §C collects smaller proposals I intended to fold into the plan
directly (each flagged, veto-able). §D–§F sketch the machinery that follows from whichever
options are chosen.

---

## A. Where things stand

- Phases 1–5 are ✅: `source`/`error` (S0), `token`, `state`, `spec`+`library`, `node`.
  `cargo build` + 123 tests green. Old exploratory `src/constructs/{mod,general}.rs` and
  `src/parser/` are **uncompiled quarry** (not in `lib.rs`'s module graph); convention is to
  rename superseded quarry to `*_JUNK._rs`.
- Phase 6 scope per ARCHITECTURE §9: `ParseContext`, `NodesParser`, group/comment/callable
  parsers, `ArgumentsParser` + `SlotsParser`, `Language<L>`/`ParserSession`/`ParseResult`;
  pin down the whitespace/span invariants of §nodes; end-to-end tests on real LaTeX-ish
  snippets (the latexlike preset itself is Phase 7 — Phase 6 tests use minimal test langs,
  as Phases 3–5 did).
- Explicitly deferred **to** Phase 6 by earlier design sessions:
  1. Full `ArgumentKind` inventory + acceptance semantics (Phase 4 shipped skeletons).
  2. Slot separators/terminators incl. the invocation-name back-reference (`\end{align}`).
  3. `CallableSpec::invocation_parser()` — the full-takeover escape hatch.
  4. `CallableTypeId`/`GroupTypeId` *interning* (needs `Language<L>`).
  5. `ArgsLayout` per-instance syntax records growing with their consumers
     (DESIGN_RATIONALE §6.5 / §3.5).
     *[UPDATE July 2026, regions session — landed pre-Phase-6: per-instance argument
     syntax (pre-argument whitespace, comments, markers) is now ordinary *nodes* inside
     per-argument/slot child regions (`ChildRegion`, parser-designated `content_range`,
     builder-resolved); `pre_space` is gone. DESIGN_RATIONALE §3.5.]*
  6. Comment recomposition fields (open question §6.5).
  7. Top-level convenience API (open question §6.3): `Parser` facade vs `Language::parse()`.
  8. Whitespace/span invariants of §nodes ("decided in principle; pinned down in phase 6").

## B. The four load-bearing open questions

### [Q1] Slot terminators/separators: how declarative?

The tension: ARCHITECTURE §specs says slot "terminator patterns may reference the invocation
name — `\end{align}` must match the `align` that opened; a `---` fence closes with `---`",
but `\end{name}` is latexlike-shaped syntax and the core must not privilege it (§2.3). Both
options below keep the *mechanism* generic; they differ in how much lives as core spec data.

**Option A (recommended) — declarative terminator spec.** `SlotSpec` grows a data-driven
terminator description; ordinary environments and fence constructs become expressible with
zero custom parser code:

```rust
pub struct SlotSpec {
    pub name: Option<Box<str>>,
    /// None = the slot ends where the enclosing scope ends (EOF / group close).
    pub terminator: Option<SlotTerminatorSpec>,
}

pub struct SlotTerminatorSpec {
    /// Token-level condition where slot content stops (token left unconsumed by content).
    pub stop: StopConditionSpec,
    /// What the terminator itself consumes after its trigger token (e.g. \end's {name}).
    pub arguments: ArgumentStructureSpec,
    /// Index of the terminator argument whose text must spell the invocation name.
    pub match_invocation_name: Option<usize>,
}

pub enum StopConditionSpec {
    Command  { name: NamePattern },        // \end…
    Specials { name: NamePattern },        // …--- fence close
    GroupClose { group_type: GroupTypeId },
    ParagraphBreak,
}

pub enum NamePattern {
    Fixed(Box<str>),
    InvocationName,   // a fence that closes with its own trigger text
}
```

latexlike environment = pure data: one slot, `terminator.stop = Command{Fixed("end")}`,
`arguments = [Mandatory{brace}]`, `match_invocation_name = Some(0)`. A `---` fence:
`Specials{InvocationName}`, no arguments. Separators for multi-slot constructs: a
`separator: Option<StopConditionSpec>` on `SlotStructureSpec` (between consecutive slots).

- - Environments/fences fully declarative; Phase 7's `EnvironmentSpec` helper is a
    constructor, not a parser; recomposition of the terminator is spec-driven (reproduce).
- − Richer core vocabulary; the terminator's own argument parsing reuses `ArgumentsParser`
    (fine — it exists anyway).

**Option B — minimal core.** `SlotSpec` carries only `stop: Option<StopConditionSpec>`;
the default parser stops *before* the terminator token and leaves consuming + name-matching
(`\end{align}` == `align`) to a custom invocation parser, which the preset writes once for
`\begin` in Phase 7.

- - Smaller core surface now.
- − Every environment-like construct needs (shared) custom parser code; the "declarative
    common path" promise of §specs is weakened; terminator syntax isn't recorded
    spec-side for level-2 recomposition (the custom parser must use the
    `CallableSpec::recompose()` hook eventually).

Note either way: `StopConditionSpec` doubles as the **stop-condition data for
`NodesParser`** (group interior stops at `GroupClose`, slot content stops per spec, root
stops at `EndOfStream`) — one reified type, used by both, instead of pylatexenc's predicate
closures. The nuclear escape hatch (custom parser) covers exotic stop logic.

### [Q2] The invocation-parser escape hatch: accessor object vs method

The default declarative parser must know *which* invocation it is parsing (callable type,
name spelling, spec, trigger token) — the sketched
`fn invocation_parser(&self) -> &dyn ConstructParser<L, Output = NodeId>` gives it no
channel for that.

**Option A (recommended) — method with explicit argument.** Replace the accessor with a
defaulted method on `CallableSpec`; overriding it *is* the takeover hatch:

```rust
pub trait CallableSpec<L: Lang>: fmt::Debug {
    fn arguments(&self) -> &ArgumentStructureSpec { … }
    fn slots(&self) -> &SlotStructureSpec { … }

    /// Parse one invocation of this callable. Default: declarative parsing driven by
    /// arguments() + slots(). Overriding = full takeover (\verb, tabular preambles, FLM).
    fn parse_invocation(
        &self,
        cx: &mut ParseContext<'_, '_, '_, L>,
        invocation: &Invocation<'_, L>,
    ) -> ParseOutcome<(BuildId, Option<ParsingStateDelta<L>>)> {
        parse_std_invocation(cx, invocation)
    }
}

pub struct Invocation<'a, L: Lang> {
    pub callable_type: CallableTypeId,
    pub name: &'a str,                       // as written (node stores it owned)
    pub spec: &'a Arc<dyn CallableSpec<L>>,  // = the spec being called (for the node)
    pub token: &'a Token<'a, L>,             // trigger token (span, pre_space, post_space)
}
```

- - No hidden protocol; the context value is a plain argument; nothing to forget to set;
    no parser-object storage question on `StdCallableSpec` (its override slot becomes
    `Option<Arc<dyn InvocationParser<L>>>` consulted by its `parse_invocation` impl, or is
    simply "implement your own spec type").
- − Deviates from the ARCHITECTURE sketch (would be recorded in DESIGN_RATIONALE §3.4/§3.6
    as a Phase 6 session outcome).

**Option B — keep the accessor.** `invocation_parser() -> &dyn ConstructParser<…>`; the
pending `Invocation` travels as an `Option<Invocation>` field *inside* `ParseContext`, set
by the dispatch loop before the call, `None` for all other parsers.

- - Uniform `ConstructParser` interface for everything, matches the written sketch.
- − Must-be-set-first field protocol (runtime invariant the compiler can't see); specs must
    own or point to parser objects (default = shared singleton — a `static`… which needs
    `L`-independence gymnastics or `OnceLock`-per-`L`, awkward in `no_std`).

(Output type note: the builder stages with `BuildId`; `NodeId` only exists after
`finish()`. The sketch's `Output = NodeId` becomes `BuildId` either way.)

### [Q3] Argument acceptance semantics (default `ArgumentsParser`)

**Option A (recommended) — pylatexenc parity, whitespace recorded.**
- Whitespace is allowed and skipped before each argument (`\frac {a} {b}`, `\section *`),
  **recorded per-argument** in the layout so level-2 recomposition reproduces it
  ("reproduce, don't guess") and the invocation span stays exactly accounted for:

```rust
pub enum ArgLayout {
    Absent,
    Present { child: u32, pre_space: TextContent },
    Marker  { text: TextContent, pre_space: TextContent },
}
```

*[UPDATE July 2026, regions session: Option A's **acceptance** semantics stand (they were
adopted with the `ParsedArguments` decision), but the record sketch above is superseded —
whitespace before an argument is a whitespace-only `Chars` node inside the argument's
`ChildRegion`, comments likewise (no `pre_space` field, no `Marker` variant; markers are
`Chars` nodes counted as content). The argument parser owns its region's noise scan (shared
helper; never a centralized pre-scan — verbatim-delimiter arguments must see raw tokens) and
must rewind fully when reporting the argument absent. DESIGN_RATIONALE §3.5.]*

- `Mandatory { group_type }` accepts the delimited group **or**, failing that, a single
  expression — one `Char` token → `Chars` node; one `Command` → the *full nested
  invocation* (`\frac12`, `\frac1\alpha`; LaTeX rule, pylatexenc's std `{` argument).
- `Optional`/`Star` are present only when their open delimiter / marker is next
  (after skippable whitespace); otherwise `Absent` and the whitespace is left unconsumed
  (it remains the following token's pre-space — content).
- Paragraph breaks never get skipped inside argument scanning (`skip_whitespace` already
  guarantees this at the token level; an argument search stopping at a `ParagraphBreak`
  token reports the argument absent/missing).

**Option B — parity, whitespace *not* recorded.** Same acceptance; `ArgLayout` unchanged.
Level-1 (span) recomposition still exact; level-2 re-emits args back-to-back — usually
re-parses equivalently, but weakens the stated recomposition requirement.

**Option C — strict groups-only.** `\frac12` = diagnostic + recovery. Simplest parser;
departs from LaTeX/pylatexenc; Phase 7 acceptance tests (ported pylatexenc cases) would fail
on common input.

Also in scope under this question (proposal, either option): record the **matched delimiter
alternative** where an argument kind ever allows alternatives, and the star marker's exact
spelling (already there). Verbatim-delimited argument kinds: **defer to Phase 7** with
`\verb` (see §C6).

### [Q4] `Comment` node recomposition fields (open question §6.5)

With several `CommentRule`s in scope, *which start delimiter fired* and *what syntactic
post-space followed* (newline + indentation) are per-instance facts. The decided
`Comment { content, ext }` stores neither (level-1 covered by the span; level-2 not).

- **Option A (recommended):** grow both — `start: TextContent` + `post_space: TextContent`
  (mirrors `CallableData.post_space` and the recorded-delimiter-alternative principle;
  fully self-contained level-2, synthesized comments included). Cost: two fields on a rare
  kind.
- **Option B:** `post_space` only; delimiter recovered from the span when span-backed,
  `Language` default for synthesized comments (guessing, but mildly).
- **Option C:** no new fields; the recomposer reads everything from the span, synthesized
  comments use the language default. Least storage, most recomposer special-casing.

Node-span convention regardless of option: a comment node's span covers
`start delimiter + content + post_space` (matches the token's span convention).

---

## C. Smaller proposals I planned to make directly in the plan (flag any objection)

**C1 — `Lang::resolve_command` hook (new).** The nodes-parser dispatch for
`TokenKind::Command` needs `(CallableTypeId, spec)`, and the core cannot know the preset's
type ids. Following the `scan_specials` precedent (a `Lang` hook that typically dispatches
to the state's libraries; recognition close to resolution):

```rust
/// Resolve a Command token to an invocation form + spec. Typically:
/// state.libraries().resolve(&CallableQuery::new(CT_MACRO, name, syntax).with_token(tok), state).
/// Default: None — the nodes parser emits a diagnostic and recovers (§C4).
fn resolve_command<'s>(
    state: &ParsingState<Self>,
    token: &Token<'s, Self>,
) -> Option<ResolvedCallable<Self>>;   // { callable_type: CallableTypeId, spec: Arc<dyn CallableSpec<Self>> }
```

**C2 — `escape_char` on `TokenKind::Command` (token-model amendment).** DESIGN_RATIONALE
§3.4 already established the escape char is *not recoverable from the token* yet is required
to build `CallableQuery { syntax: Command { escape_char } }`; and the nodes parser must not
reach around the reader into raw content (§3.2, `EndOfStream` rationale). The tokenizer
knows which `CommandRule` fired — record it:

```rust
Command { name: &'s str, escape_char: char, post_space: Span }
```

This is syntactic fact (which rule fired), not resolution output — consistent with the
"no `CallableTypeId` on tokens" line. Small ripple through Phase 3 token tests.

**C3 — `ParagraphBreak` → whitespace-only `Chars` node (core default).** ARCHITECTURE
§nodes leaves the representation (whitespace chars vs specials-type callable) as a *preset*
decision in Phase 7. Core default now: its own whitespace-only `Chars` node (span = token
span, `TextContent::Spanned`). No `Lang` hook yet; Phase 7 adds one only if the preset needs
the callable representation. Post-break indentation is the next token's pre-space and flows
through normal accumulation (two adjacent whitespace-only nodes are possible and fine —
deterministic, span-partition preserved).

**C4 — Tolerant-parsing behavior of the nodes parser.** Per §errors: token errors carrying a
recovery token → `Recovery::Tolerant` records a `Diagnostic` and continues with the recovery
token; `Recovery::Strict` aborts the parse with the error. Parse-level conditions (unmatched
group close, missing mandatory argument, unresolvable command with no fallback, terminator
name mismatch) do the same, each with a defined recovery (skip token / argument absent /
chars-node fallback / accept-with-diagnostic respectively). Diagnostics accumulate in the
session and ride on `ParseResult` even for successful tolerant parses.

**C5 — Top-level API: `Language::parse()` is the entry point; no `Parser` facade**
(open question §6.3 → close as "no facade"). Type aliases in the preset keep simple usage
terse. Can be revisited post-Phase-7 if ergonomics demand it.

**C6 — Scope deferrals.** `VerbatimParser` + verbatim argument kinds → Phase 7 (with
`\verb`; the escape hatch is proven in Phase 6 by a test-lang custom `parse_invocation`).
`DelimitedParser` ships only in the form the group/argument parsers need. Recomposition
*implementation* (level 2) stays post-Phase-6; Phase 6 only guarantees the *records* exist
(that's Q3/Q4). Synthetic-source registry on `ParserSession`: skeleton only (creation of
resolved/synthesized sources works; the node-back-reference design stays deferred as
decided). Transform/visitor APIs: post-Phase-6, unchanged.

**C7 — Old quarry.** `src/constructs/{mod,general}.rs`, `src/parser/{mod,minparser}.rs`
get the `*_JUNK._rs` treatment when the new modules land (established convention; nothing
is deleted).

---

## D. Whitespace & span invariants (the §nodes pin-down, to be recorded in DESIGN_RATIONALE)

1. **Chars accumulation.** `Char` tokens accumulate into maximal `Chars` nodes; a token's
   pre-space (content whitespace) joins the accumulating run. A run flushes when a
   non-`Char` construct starts; pending whitespace with no adjacent chars becomes a
   **whitespace-only `Chars` node** (pylatexenc behavior). Content is `TextContent::Spanned`
   (zero-copy) — always the exact span slice.
2. **Callable post-space.** Whitespace immediately following a completed invocation is the
   *node's* `post_space` (all callables — macro-, environment-, specials-formed), stopping
   at any paragraph break; it is included in the node's span (trailing sub-range). Mechanik:
   after the last argument/slot, the parser peeks the next token and claims its pre-space
   via `move_to(tok, rewind_pre_space = false)` — this is exactly what the reader-protocol
   flags exist for.
3. **Groups have no post-space** (space after `}` is content); **comments'** post-space is
   the terminating newline + indentation (from the token), stopping at paragraph breaks
   (token level already guarantees it).
4. **`EndOfStream.pre_space`** materializes as a final whitespace-only `Chars` node.
5. **Partition invariant.** Sibling spans partition the parent's interior exactly — no gaps,
   no overlap — for *content* interiors (`List` bodies, `Group` interiors, the root).
   Inside a `Callable`'s own span, the children (argument/slot nodes) plus the layout's
   recorded syntax (name, markers with pre-space, argument pre-space, terminator syntax,
   post-space) jointly account for every byte (Q1/Q3 records are what make this exact).
   *[UPDATE July 2026, regions session — simpler now: a callable's interior = name syntax
   + children in order (argument/slot regions tile the child list; the builder
   debug-asserts it, so markers, pre-argument whitespace, and comments are all just
   nodes) + terminator records (Q1) + post-space. DESIGN_RATIONALE §3.5.]*
6. **Paragraph breaks** are their own whitespace-only `Chars` nodes (C3), never merged into
   neighboring whitespace nodes.

## E. Engine shapes (per ARCHITECTURE §engine, March 2026 ownership model kept)

```rust
pub struct Language<L: Lang> {
    // interning registries (Phase 6 finally provides the machinery):
    //   group_types:    name ↔ GroupTypeId    (ids stay direct-constructible for tests)
    //   callable_types: name ↔ CallableTypeId
    default_rules: TokenRules,
    base_libraries: LibraryStack<L>,       // incl. per-type fallbacks
    // default StateExt comes from L::StateExt: Default
    // SourceResolver: owned Box<dyn SourceResolver<L::SourceOrigin>> (NoResolver default)
}
impl<L: Lang> Language<L> {
    pub fn parse(&self, input: …) -> Result<ParseResult<'_, L>, ParseError>;  // strict-abort = Err
    pub fn session(&self) -> ParserSession<'_, L>;                            // advanced path
}

pub struct ParserSession<'env, L: Lang> {
    language: &'env Language<L>,
    builder: NodeTreeBuilder<L>,
    diagnostics: Diagnostics<L::SourceOrigin>,
    recovery: Recovery,
    // synthetic-source registry: skeleton (C6)
}
pub struct ParseResult<'env, L: Lang> {
    language: &'env Language<L>,   // registry/spec lookups stay available for analysis
    tree: NodeTree<L>,
    diagnostics: Diagnostics<L::SourceOrigin>,
}
```

- **Seeding, not dependency** (§3 stratum note): at session start `Language` builds the
  initial `Arc<ParsingState<L>>` (default rules + base libraries + default ext); afterwards
  the loop reads only the state.
- **Lifetime knot** for `'s` (content borrow): `Language::parse` pins the `Arc<Source>` in
  its own frame, borrows `&'s str` content from it, builds `StdTokenReader<'s>`, drives the
  parse, and `finish()`es into the `'s`-free `NodeTree` before returning. `'s` never
  escapes; `ParserSession` itself stays `'s`-free (the reader is passed through
  `ParseContext`, not stored in the session).

```rust
pub struct ParseContext<'a, 's, 'env, L: Lang> {
    pub tokens: &'a mut dyn TokenReader<'s, L>,
    pub state: Arc<ParsingState<L>>,          // current state (parsers derive children locally)
    pub session: &'a mut ParserSession<'env, L>,
    // + Option<Invocation> here iff Q2 = Option B
}

pub trait ConstructParser<L: Lang> {
    type Output;                              // BuildId, Vec<BuildId>, ArgsLayout, …
    fn parse(&self, cx: &mut ParseContext<'_, '_, '_, L>)
        -> ParseOutcome<(Self::Output, Option<ParsingStateDelta<L>>)>;
}
```

- **Naming to settle in the plan:** engine result = `ParseResult<'env, L>` (as decided);
  the per-parser return alias therefore can't also be `ParseResult` →
  `ParseOutcome<T> = Result<T, ParseError>` with a span-carrying `error::ParseError`
  (hand-written, zero-dep, per §errors). Open to better names (`ConstructResult`?).
- Returned deltas propagate exactly as §state prescribes: the *caller* applies them to its
  own state for subsequent siblings (`state.derived(&delta)` → new `Arc`).

## F. The dispatch loop (`NodesParser`) — concretizing ARCHITECTURE §constructs

```
loop (peek with current state):
  Char           → accumulate (D1)
  ParagraphBreak → flush run; whitespace-only Chars node (C3)
  GroupOpen(t)   → flush; GroupParser(t): derived state (expecting_group_close for
                   ambiguous delims, per §3.2), recurse NodesParser with
                   StopCondition GroupClose(t), consume close, Group node
  Comment        → flush; Comment node straight from the token (whole-comment tokens)
  Command        → flush; Lang::resolve_command (C1); spec.parse_invocation(cx, inv) (Q2)
  Specials(spec) → flush; spec already on token; parse_invocation likewise
  GroupClose     → stop-condition match? return to caller : diagnostic + recover (C4)
  EndOfStream    → stop (root/slot); trailing-whitespace Chars node from pre_space (D4)
  returned delta → state = state.derived(&delta) for subsequent siblings
```

Standard parsers shipped: `NodesParser` (with `StopConditionSpec` list), `GroupParser`,
the std invocation path (`parse_std_invocation` = `ArgumentsParser` + `SlotsParser`),
`ExpressionParser` (single node: group / full invocation / single char — Q3's fallback).
No `CommentParser` (vestigial). Module home: `src/constructs/` (S1 topic) + `src/engine/`
(`Language`, `ParserSession`, `ParseResult`, `NodeRef` re-export point stays in `node`).

## G. Testing plan (sketch)

- Unit: each construct parser against minimal test langs (Phase 3–5 pattern), including
  delta propagation (`\newcommand`-shaped: parser returns push-library delta; subsequent
  siblings resolve differently), tolerant vs strict recovery paths, stop conditions,
  nested groups/invocations, arguments (present/absent/marker/single-token), slots with
  terminator + name back-reference (Q1A) incl. mismatch diagnostics.
- End-to-end (`Language::parse`): LaTeX-ish snippets under a hand-rolled test lang with
  `\`-commands, `{}`/`[]` groups, `%` comments, a small library (`\frac`, `\section*`,
  an environment pair, a `~`-like specials) — asserting tree shape, spans (partition
  invariant D5 checked mechanically over the whole tree), diagnostics, post-space
  placement, paragraph-break nodes.
- The pylatexenc walker-suite port remains Phase 7 (needs the real preset).

## H. Documentation obligations at phase end

- DESIGN_RATIONALE: record Q1–Q4 outcomes + C1–C5 (short entries, decisive reason each);
  strike §6.3, §6.5 from open questions; add whitespace/span invariants (D) under §3.5/§3.6.
- ARCHITECTURE §9: mark Phase 6 ✅ with the shipped/deferred split (mirroring earlier
  phase entries); amend the `invocation_parser()` sketch if Q2 = Option A.
- NAMING_STRATEGY: `ParseOutcome` (or chosen name), `Invocation`, `StopConditionSpec`,
  `SlotTerminatorSpec`, `ResolvedCallable` — whatever the session settles.
