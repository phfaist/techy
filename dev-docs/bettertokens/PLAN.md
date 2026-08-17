# Better tokens — implementation plan

Status: **design approved 2026-08-16 (Philippe); execution not started.**
Companion files (created during execution, same directory): `PROGRESS.md` (stage
log, resumable state), `PROBE_REPORT.md` (Stage 0 findings).

This document is self-contained: a fresh Claude Code session (the *orchestrator*)
executes it without the design conversation. Read §0–§1 fully before starting; §2–§6
are the code stages and §7 the documentation stage; §8 is the execution protocol
(worktrees, agents, reviewers, merges); §9 the risk register; §10 what is deliberately
deferred.

Repository facts the plan relies on (verify with `git log` if in doubt): workspace root
`/Users/philippe/projects/techy`, crate `techy/` (`techy/src/…`), integration tests
`techy/tests/`, user guides `docs/*.md` (included into rustdoc through
`techy/src/lib.rs`, so their Rust code blocks are **doctests**), rust-version 1.86,
`missing_docs = deny`, `broken_intra_doc_links = deny`. Line numbers quoted below were
refreshed against commit `9a3c0ac` (the design session's tree was `1c59e66`); treat them
as pointers.

**Two standing instructions from the user (binding):**

1. **`dev-docs/ARCHITECTURE.md` and `dev-docs/DESIGN_RATIONALE.md` are updated only in
   Stage 5 (§7), after every code stage has merged** — never piecemeal from a code
   stage. Stage 5 follows those documents' own maintenance rules
   (`Documentation_Structure.md`; the register's entry template; every rationale entry
   referenced from ARCHITECTURE) and its merge requires the user's explicit OK on the
   drafted text.
2. Ask before taking design decisions not covered here. §1.16 lists the small
   decisions an implementer may hit, each with a default; anything else — stop and
   ask (the orchestrator relays to the user).

---

## 0. Why: the problem in one page

The token reader (`techy/src/token/reader.rs`) and the construct parsers
(`techy/src/constructs/*.rs`) communicate positions as bare `usize` byte offsets and
spans as bare `Span`s that the parser pairs with `ParseContext::source`. One number does
three jobs: (a) *a place in the text* (what a `SourceSpan` needs), (b) *a place in the
token stream* (rewind/resume targets: `move_to_pos`, `TokenRecovery::resume_pos`,
`ArgumentNoise::start`, `EnvironmentBody.end`, `NameGroup.end`, `stage_invocation`'s
`end_pos`), and (c) a comparable quantity for "did the reader advance" checks. Under a
single-source reader these coincide. They stop coinciding for any reader that serves
tokens from more than one source during one parse (an in-place macro expander in the
style of TeX's "mouth", the motivating future feature): a stream position is then a
*stack* of (source, offset), only the reader knows it, and pairing a token span with
`ParseContext::source` silently produces a valid-looking wrong span.

Also on the table: `Token<'s, L>` borrows content strings for `'s`, which such a
reader cannot honor for sources minted mid-parse; the parser interprets token spans
itself (an unenforceable "relative to which source?" contract); and there is no way to
install a custom `TokenReader` (`Language::parse_source` and
`ParseContext::parse_attached_source` construct `StdTokenReader` directly).

The approved design (§1) makes the **reader the sole interpreter of its tokens** and of
positions in its stream:

- **Tokens are opaque values** chosen per language (`Lang::Token`); the parser holds
  and passes them but reads nothing off them. The reader answers *what* a token is
  (`token_kind` → a borrowed `TokenKind<'t, L>` view with real `&str` names) and *where*
  it is (`source_span_of`/`source_span_between` → `SourceSpan`; `position_at` → stream
  position).
- **Stream positions are opaque, unforgeable values** chosen per language
  (`Lang::StreamPosition`), obtainable only from the reader (`position_here`,
  `position_at`), usable for navigation (`move_to_position`) and for span computation
  (`source_span_within(begin, end)`); no arithmetic, no constructor.
- Navigation is `move_to(&token, TokenEdge)` (one method, four edges) plus
  `move_to_position(&pos)`. `move_to_pos`, `pos()`, `move_past`, and the two-flag
  `move_to` are deleted.
- **`ParseContext::source` is removed**: a construct parser has no handle to pair a
  number with; every `SourceSpan` it stages comes from the reader (or from another
  `SourceSpan`).
- `TokenError` carries a source-qualified location; `TokenRecovery` carries the
  reader's own resume position; `Lang::scan_specials` returns plain errors and no
  recovery (recovery is the reader's job).
- `ParseDriver::make_token_reader` is the door for custom readers; both reader
  construction sites route through it.
- The **node tree is untouched**: node spans stay single-source `SourceSpan`s and node
  data sub-spans stay node-relative `Span`s. Only how the parser *obtains* them
  changes.
- Zero-copy is preserved: `StdToken` stores spans and `Arc`s (no strings, no
  lifetime); `StdTokenReader` slices its `&'s str` content when asked. No per-token
  allocation, no per-token `Arc` clone.

What this plan does **not** do: implement an expanding reader; relax the chars-run
"gap-free" contract; add a reader-configuration knob to `LatexlikeDriver` (§10). The
architecture/rationale entries for the decisions above are written in Stage 5 (§7).

---

## 1. Target design (normative — reviewers check diffs against this section)

### 1.1 Vocabulary

- **Token**: an opaque value produced by a reader; meaningful only through the reader
  that produced it (or a reader over the same content, e.g. the test list reader).
- **Stream position**: an opaque value naming a place in the token stream, minted by
  the reader; for `StdTokenReader` a byte offset behind a private newtype.
- **Edge** (`TokenEdge`): one of the four boundaries of a token in stream order:
  `StartBeforePreSpace` (where its pre-space begins), `Start` (where the token proper
  begins), `End` (where the token proper ends = where its post-space begins),
  `EndPastPostSpace` (where its post-space ends). For kinds without post-space
  `End == EndPastPostSpace`; for tokens without pre-space `StartBeforePreSpace == Start`.
- **Text location**: a `SourceSpan`/`SourcePos` (S0 types, `Arc<Source>` + offsets).
  This is the *only* form in which locations leave the reader.
- **Reader-relative span**: an internal notion of `StdToken`; never visible to parsers.

### 1.2 `Lang` additions (`techy/src/state/lang.rs`)

```rust
pub trait Lang: Sized + 'static {
    // … existing associated types …
    /// The token type this language's readers produce. Opaque: a construct parser
    /// holds and passes tokens; only a `TokenReader` interprets them.
    type Token: Token<Self>;
    /// The reader-defined position in the token stream. Opaque and unforgeable:
    /// obtainable only from a `TokenReader` (`position_here`, `position_at`).
    type StreamPosition: Clone + fmt::Debug + PartialEq + Eq + Send + Sync;
    // …
}
```

Every explicit `impl Lang for …` gains the two associated types. Recounted at
`9a3c0ac` with
`grep -rnE '\bimpl(<[^>]*>)? Lang for|impl crate::(state|core)::Lang for|impl techy::core::Lang for' techy/src techy/tests docs`:
**57 sites** — 53 in `techy/src`, 3 in `techy/tests/lang_features.rs`, 1 in a
**doctest** (`docs/custom-lang.md:196`, which must compile). Per file:

| File | Sites |
|---|---|
| `techy/src/constructs/nodes_parser.rs` | 16 |
| `techy/src/engine/mod.rs` | 6 |
| `techy/src/engine/language.rs` | 5 |
| `techy/src/state/parsing_state.rs` | 5 |
| `techy/src/node/mod.rs` | 4 |
| `techy/src/token/reader.rs` | 4 |
| `techy/tests/lang_features.rs` | 3 |
| `techy/src/latexlike/mod.rs` | 2 |
| `techy/src/serialize/drivers/tree_tests.rs` | 2 |
| `techy/src/constructs/argument_parsers.rs` | 1 |
| `techy/src/constructs/attached_source.rs` | 1 |
| `techy/src/constructs/environment_parser.rs` | 1 |
| `techy/src/constructs/verbatim_parser.rs` | 1 |
| `techy/src/scopes/mod.rs` | 1 |
| `techy/src/serialize/drivers/diagnostic_tests.rs` | 1 |
| `techy/src/serialize/drivers/tests.rs` | 1 |
| `techy/src/spec/mod.rs` | 1 |
| `techy/src/state/lang.rs` | 1 — **the blanket `impl<T: TrivialLang> Lang for T`** (`state/lang.rs:524`) |
| `docs/custom-lang.md` | 1 (doctest, line 196) |

The ~30 `impl TrivialLang for X {}` sites need **no** edit: they receive both
associated types from the blanket impl. Each site gains:

```rust
    type Token = StdToken<Self>;
    type StreamPosition = StdStreamPosition;
```

`Lang::scan_specials` changes signature (§1.7). No other `Lang` change.

### 1.3 The token: `Token<L>` (trait), `TokenKind<'t, L>` (view), `StdToken<L>`

`techy/src/token/token.rs`.

```rust
/// Marker contract of a language's token type. A token is a transient value: it is
/// produced by a `TokenReader`, passed around by construct parsers, and read only
/// through the reader. Equality compares the facts the reader recorded (test
/// harnesses compare tokens from two readers over the same content).
pub trait Token<L: Lang>: Clone + fmt::Debug + PartialEq + Send + Sync {}
```

The **parser-facing view** — the closed core enum, `Copy`, borrowing from the token
(or, for the std reader, from the content):

```rust
pub enum TokenKind<'t, L: Lang> {
    Char(char),
    GroupOpen  { delim: &'t str, rule: &'t Arc<GroupRule<L>> },
    GroupClose { delim: &'t str },
    Command    { name: &'t str, escape_char: char },
    Specials   { callable_type: L::CallableTypeId, name: &'t str, spec: &'t Arc<dyn CallableSpec<L>> },
    Comment    { start_delim: &'t str, content: &'t str },
    ParagraphBreak,
    EndOfStream,
}
```

- **No span fields.** Spans and positions come from the reader (§1.6).
- Keeps `as_str()`, `Display` (renders written spellings, comment content truncated as
  today), `Debug`, `PartialEq` (specs by `Arc` identity, rules structurally — as
  today); manual `Clone`/`Copy` impls (no `L: Clone` bound).
- The `Specials` name is the matched text (see `SpecialsMatch`, §1.7).

**`StdToken<L>`** — the token of `StdTokenReader` and of every language that uses it:

- Private data: kind data (`Char(char)`, `GroupOpen { rule }`, `GroupClose`,
  `Command { escape_char, post_space: Span }`, `Specials { callable_type, spec }`,
  `Comment { start: Span, post_space: Span }`, `ParagraphBreak`, `EndOfStream`), plus
  `span: Span`, `pre_space: Span`. Spans are reader-relative (offsets into the content
  the issuing reader scans). **No `&str`, no lifetime.**
- Public constructors, one per kind (custom readers that reuse `StdToken` mint tokens
  with them): `StdToken::char(c, span, pre_space)`, `group_open(rule, span, pre_space)`,
  `group_close(span, pre_space)`, `command(escape_char, span, pre_space, post_space)`,
  `specials(callable_type, spec, span, pre_space)`, `comment(start, span, pre_space,
  post_space)`, `paragraph_break(span, pre_space)`, `end_of_stream(pre_space)` (span =
  empty at `pre_space.end()`). Each asserts the same span coherence `Token::new` asserts
  today (`pre_space.end() == span.start()`; post-space a trailing sub-range of `span`;
  a comment start a leading sub-range ending before its post-space) — these
  constructors inherit `Token::new`'s registered always-on-assert exception; update the
  documentation Panics list in `docs/panics.md` accordingly: the
  `- [`Token::new`](crate::core::Token::new) — requires the documented coherence of the
  token's spans;` line (`docs/panics.md:22-23`) is **replaced** by one line per
  `StdToken` constructor family — or one line naming all eight constructors — stating
  the coherence asserts, and the sentence above it ("**Precondition asserts.** Six value
  functions …", `docs/panics.md:9`) has its count corrected. Use the current rustdoc
  link spelling `[Panics list](techy::guide::panics)` on the constructors.
  *(CLAUDE.md rule 4's parenthetical also names `Token::new` among the six approved
  value functions. **No code stage edits CLAUDE.md** — orchestrator decision pending on
  who does.)*
- `pub(crate)` accessors used by the two in-crate readers: `span()`, `pre_space()`,
  `post_space()`, `edge_offset(TokenEdge) -> usize`, `kind_data()`. **Not public**: a
  third-party reader over `StdToken`s interprets them by delegating to a
  `StdTokenReader` over the same content (the pattern §1.8 documents), never by reading
  fields.
- `impl<L: Lang> Token<L> for StdToken<L>`; manual `Clone`, `Debug`, `PartialEq`.

### 1.4 `TokenEdge` (`techy/src/token/reader.rs` or a small `edge.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenEdge { StartBeforePreSpace, Start, End, EndPastPostSpace }
```

`Ord` is stream order (declaration order). Used by `move_to`, `position_at`,
`source_span_between`.

### 1.5 `StdStreamPosition` (`techy/src/token/reader.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StdStreamPosition(usize);   // private field
impl StdStreamPosition {
    pub(crate) fn at(offset: usize) -> Self;   // in-crate readers only
    pub(crate) fn offset(self) -> usize;
}
```

Not constructible from outside the crate: no public constructor, no arithmetic. (If a
third-party reader over std tokens ever needs to mint positions, graduate a
constructor then — the embedding-feedback policy: graduate on demonstrated need.)

### 1.6 `TokenReader<'s, L>` (`techy/src/token/reader.rs`)

`'s` is the borrow of the content the reader scans (as today). The trait has **no
associated types** (it uses `L::Token` and `L::StreamPosition`), so it stays
object-safe and `ParseContext.tokens` stays `&'a mut dyn TokenReader<'s, L>`.

```rust
pub trait TokenReader<'s, L: Lang> {
    // --- reading ---------------------------------------------------------------
    /// Parse the token at the current stream position without advancing.
    fn peek(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<L, L::Token>;
    /// `peek` + `move_to(&token, TokenEdge::EndPastPostSpace)`.
    fn next(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<L, L::Token> { /* default */ }

    // --- navigation (the only two ways to move) ---------------------------------
    /// Reposition the stream at `edge` of `tok` (forward or backward).
    fn move_to(&mut self, tok: &L::Token, edge: TokenEdge);
    /// Reposition the stream at a position this reader handed out earlier.
    fn move_to_position(&mut self, at: &L::StreamPosition);

    // --- what a token is -----------------------------------------------------
    /// The parser-facing view of `tok`. Borrows from the token (and, for readers
    /// that scan borrowed content, from that content) — never from the reader.
    fn token_kind<'t>(&self, tok: &'t L::Token) -> TokenKind<'t, L> where 's: 't;

    // --- where a token is, in text coordinates ------------------------------
    /// The source span delimited by two edges of `tok` (either argument order).
    fn source_span_between(&self, tok: &L::Token, a: TokenEdge, b: TokenEdge)
        -> SourceSpan<L::SourceOrigin>;
    /// `source_span_between(tok, Start, EndPastPostSpace)` — the token's span in
    /// today's sense (pre-space excluded, post-space included).
    fn source_span_of(&self, tok: &L::Token) -> SourceSpan<L::SourceOrigin> { /* default */ }

    // --- where the stream is ----------------------------------------------------
    fn position_here(&self) -> L::StreamPosition;
    fn position_at(&self, tok: &L::Token, edge: TokenEdge) -> L::StreamPosition;
    /// The text location of a stream position (an empty-span anchor for
    /// diagnostics is `SourceSpan::at(&pos)`).
    fn source_position_at(&self, at: &L::StreamPosition) -> SourcePos<L::SourceOrigin>;
    /// The source span from `begin` to `end`, when the two positions delimit one
    /// range of one source; `None` otherwise (an incoherent pair — a caller bug the
    /// caller lifts to an implementation error).
    fn source_span_within(&self, begin: &L::StreamPosition, end: &L::StreamPosition)
        -> Option<SourceSpan<L::SourceOrigin>>;
}
```

Contract clauses (write them into the trait's rustdoc; the reviewer checks they are
there):

1. **`peek` is speculative and idempotent per (stream position, state instance)** —
   the existing clause, with "position" now meaning *stream position*; a repeated
   `peek` at the same position under the same `Arc<ParsingState>` returns an equal
   token; a different state instance voids the obligation. `move_to`/`next` commit.
2. **A peeked token's `StartBeforePreSpace` edge is the current stream position** —
   `move_to(&tok, StartBeforePreSpace)` right after `peek` is a no-op, and later
   returns the stream to exactly where that peek happened.
3. **Every token and every position a reader hands out during a parse remains a valid
   argument to `move_to`, `move_to_position`, `position_at`, `source_span_between`,
   `source_span_within` for the rest of that parse** — a reader that keeps several
   sources must keep them addressable. Positions are compared with `==` only (no
   ordering across sources).
4. **Interpretation stays with the issuing reader** (or a reader over the same content):
   handing a token to a reader that did not produce it is a caller contract violation.
   `StdTokenReader` cannot detect it and answers from the token's offsets;
   `TokenListReader` (test) rejects it (§1.8).
5. Absent-features clause: unchanged from today.
6. Ordering of `source_span_between`'s two edges is immaterial: the result is the
   span between them in stream order.

### 1.7 Errors and the specials hook (`techy/src/token/error.rs`, `specials.rs`)

```rust
pub struct TokenError<L: Lang> {
    kind: TokenErrorKind,
    span: SourceSpan<L::SourceOrigin>,       // was: Span (parser paired it with cx.source)
    recovery: Option<Box<TokenRecovery<L>>>,
}
pub struct TokenRecovery<L: Lang> {
    pub token: L::Token,                     // the placeholder to emit
    pub resume: L::StreamPosition,           // was: resume_pos: usize
}
pub type TokenResult<L, T> = Result<T, TokenError<L>>;
```

- `TokenError::new(kind, span: SourceSpan, recovery)`, `kind()`, `span()`,
  `recovery()`, `into_recovery()`; manual `Clone`/`Debug`/`Display`/`Error` impls as
  today. `TokenErrorKind` unchanged.
- **Advancement contract** (reworded): `resume` must move the stream — the content
  loop checks `position_here()` differs after `move_to_position(&resume)` and aborts
  otherwise (equality, not ordering).
- The specials hook family returns **plain errors, no recovery**:

```rust
/// A `Lang::scan_specials` / `SpecsProvider::scan_specials` failure, in the hook's
/// content coordinates. Recovery, if any, is the reader's business.
pub struct SpecialsScanError { pub kind: TokenErrorKind, pub span: Span }
pub struct SpecialsMatch<L: Lang> {          // no lifetime, no `name`
    pub end: usize,
    pub callable_type: L::CallableTypeId,
    pub spec: Arc<dyn CallableSpec<L>>,
}
// Lang / SpecsProvider / Package / ScopeStack:
fn scan_specials(state: &ParsingState<L>, content: &str, pos: usize)
    -> Result<Option<SpecialsMatch<L>>, SpecialsScanError>;
```

  Rationale (short, for rustdoc): the hook detects a trigger in a `&str`; it neither
  knows the reader's token type nor its stream positions, so it cannot describe a
  recovery — and a document-level condition detectable during the scan is expressed
  the way the hook expresses everything: as a match to a spec whose parser diagnoses.
  The reader lifts a scan error into `TokenError { kind, span: <source-qualified>,
  recovery: None }` (unrecoverable, as `Package::scan_specials`'s errors already are).
  The specials *name* is the matched text `content[pos..end]` — the contract "should
  be the matched slice" becomes "is"; the field is dropped.

### 1.8 `StdTokenReader` and `TokenListReader`

`StdTokenReader<'s, O: SourceOrigin>`:

- `new(source: &'s Arc<Source<O>>)` — holds `&'s Arc<Source<O>>` (no clone) and
  `content: &'s str = source.content()`, `pos: usize`. `content()` stays as an inherent
  accessor; `is_at_end()` may stay; **`pos()` and `move_to_pos()` are removed** (the
  trait's position API replaces them).
- `impl<'s, L: Lang<SourceOrigin = O>> TokenReader<'s, L> for StdTokenReader<'s, O>`
  (**probe P8 settles this spelling**; if it does not compile, P8's reported fallback —
  e.g. `StdTokenReader<'s, L>` generic over the language — is used instead):
  `token_kind` slices `content` by the token's spans; `source_span_between` =
  `SourceSpan::new(source, a_offset..b_offset)`; positions wrap offsets;
  `source_span_within` = `Some(SourceSpan::new(source, begin..end))` if `begin <= end`,
  else `None`; `move_to`/`move_to_position` set `pos` (validated lazily at the next
  `peek`, as today); scan errors from `L::scan_specials` are lifted as in §1.7.
- Scanning core (`peek_impl`, `detect_*`, `read_*`, `skip_whitespace`) unchanged in
  logic; token construction goes through the `StdToken` constructors.

`TokenListReader<'s, L>` (`cfg(test)`, `pub(crate)`): built from
`(source: &'s Arc<Source>, tokens: Vec<StdToken<L>>)`; interprets tokens exactly like
`StdTokenReader` (slice the same content), **and rejects tokens and positions it did
not issue** — every `move_to`/`position_at`/`source_span_between` first checks the
token is one of its list entries (`PartialEq`), and `move_to_position` checks the
offset is one it handed out (a `BTreeSet<usize>` of issued offsets, seeded with the
initial position). Violations panic (test infrastructure). This is the mechanical
guard against forged tokens/positions: every construct-parser suite runs its parses
against both readers (the lockstep harness in `nodes_parser.rs`, `run_both`).

Documented pattern for a **custom reader over std tokens** (rustdoc on `TokenReader`
and the `custom-lang` guide): keep an inner `StdTokenReader` over the same content;
mint tokens with the `StdToken` constructors; delegate `token_kind`,
`source_span_between`, `position_*`, `source_position_at`, `source_span_within` to the
inner reader. `techy/tests/lang_features.rs`'s `CommentEmittingReader` is rewritten this
way.

### 1.9 `ParseContext` (`techy/src/constructs/mod.rs`)

- **Remove the `source` field.** The constructor (`techy/src/constructs/mod.rs:179`)
  loses its second parameter and keeps the other four in order — verbatim:

  ```rust
  pub fn new(
      tokens: &'a mut dyn TokenReader<'s, L>,
      state: Arc<ParsingState<L>>,
      session: &'a mut ParserSession<L>,
      driver: &'a L::Driver,
  ) -> ParseContext<'a, 's, L>
  ```

  (its rustdoc says "Bundle the **five** parse inputs" — make it "four"). `ParseContext`
  keeps **both** lifetime parameters (`'s` still comes from `TokenReader<'s, L>`), so no
  `ParseContext<'_, '_, L>` mention anywhere in the crate changes shape.
- `implementation_error(detail, span: SourceSpan)` and `staging_error(error, span:
  SourceSpan)` take source-qualified spans (they paired a `Span` with `self.source`).
- New helpers (thin, documented as the preferred spellings):
  - `here(&self) -> SourceSpan<L::SourceOrigin>` — the empty span at
    `position_here()` (= `SourceSpan::at(&self.tokens.source_position_at(&pos))`); the
    diagnostics anchor that replaces every `Span::empty(cx.tokens.pos())`.
  - `source_span_within(&self, begin, end) -> ConstructParserResult<L, SourceSpan>` —
    lifts the reader's `None` to an `ImplementationError` ("positions do not delimit
    one range of one source").
- `probe_token(&mut self, state)` unchanged for callers. On the driver
  (`techy/src/engine/driver.rs:181-187`) it happens in two steps: **Stage 1 drops the
  `source: &Arc<Source<L::SourceOrigin>>` parameter** and leaves the return type alone
  (`ConstructParserResult<L, Option<Token<'s, L>>>`); **Stage 3b** changes the return
  type to `ConstructParserResult<L, Option<L::Token>>` (and drops the `'s`).
- `stage_invocation(&mut self, invocation, arguments, slots, children, end: Option<&L::StreamPosition>)`.
  Let `trigger = self.tokens.source_span_of(invocation.token)` (the trigger's span,
  `Start..EndPastPostSpace`, in the trigger's source) and
  `trigger_start = self.tokens.position_at(invocation.token, TokenEdge::Start)`. The
  node span is computed as follows:
  - `Some(end)`: `self.source_span_within(&trigger_start, end)?` — the reader's `None`
    becomes the existing "invalid computed span" `ImplementationError`.
  - `None` — the **standard rule**: look at the last staged child (`children.last()`
    resolved through `self.staged_nodes().get(..)`, exactly as today; a foreign id
    falls through as if there were no child, as today).
    - If there is such a child and `child.span().same_source(&trigger)`: the node runs
      from `trigger.start()` to `child.span().end()` in that source:
      `SourceSpan::new(trigger.source(), trigger.start()..child.span().end())`; if
      `child.span().end() < trigger.start()`, return the "invalid computed span"
      `ImplementationError` instead (never let `SourceSpan::new` assert).
    - Otherwise (no child, or the child's span is in another source): the node ends at
      the current stream position:
      `self.source_span_within(&trigger_start, &self.tokens.position_here())?`
      (a `None` = "invalid computed span" `ImplementationError`).

  The `self.source.content().get(start..end)` validation disappears (the reader and the
  checks above validate). Under `StdTokenReader`, for a childless invocation staged in
  the ordinary flow (the parser stands right past the trigger's post-space), the
  standard rule yields exactly today's `token.span` end — **if any existing test's
  expected node span changes because of this rule, do not adjust the test: stop and
  report** (that would be a behavior change the user must rule on).
- `parse_group(base, open: &L::Token, rule, child_states, frame)` (was `open_span:
  Span`); `GroupParser::new(open: L::Token, rule)` (stores a clone).
- `invocation_frame`: name span = `source_span_between(token, Start, End)`, frame span
  = `source_span_of(token)`.
- `parse_construct`'s descent-guard warning/refusal anchors: `self.here()`.
- `recover_derive_failure`: `self.here()`.
- Root loop (`engine/language.rs`) and `parse_attached_source` loop: on
  `StopCause::UnexpectedGroupClose { span, after }`, `recover(StrayGroupClose {
  delim: span.content().to_string() }, span.clone())`, `move_to_position(&after)`,
  stage `NodeKind::chars(span.span())` with `span`.
- Reader construction: `let mut reader = driver.make_token_reader(&source);` at both
  sites (`Language::parse_source`, `parse_attached_source`).

`Invocation` (same file):

```rust
pub struct Invocation<'a, L: Lang> {          // 's is gone
    pub callable_type: L::CallableTypeId,
    pub name: &'a str,                        // from the token_kind view of `token`
    pub spec: &'a Arc<dyn CallableSpec<L>>,
    pub token: &'a L::Token,
    pub kind: TokenKind<'a, L>,               // the view, so consumers need no re-query
}
pub trait FromInvocation<L: Lang>: Sized {
    fn from_invocation(invocation: &Invocation<'_, L>, tokens: &dyn TokenReader<'_, L>) -> Self;
}
```

`from_invocation` receives the reader (shared borrow, call-scoped) because a payload
that records spelling spans (the latexlike `Macro { post_space }`) must obtain them
from the reader; the recorded `TextContent::Spanned` range is
`tokens.source_span_between(token, End, EndPastPostSpace).span()` — same source as the
node `stage_invocation` stages (the node span starts at the same token), so the
node-relative reading is sound.

### 1.10 `ParseDriver` hooks (`techy/src/engine/driver.rs`)

- New: `fn make_token_reader<'s>(&'s self, source: &'s Arc<Source<L::SourceOrigin>>)
  -> Box<dyn TokenReader<'s, L> + 's>` — default `Box::new(StdTokenReader::new(source))`.
  Documented as *the* door for custom tokenization (`Lang::Token`/`StreamPosition`
  fix the data types; the driver supplies the reader instance). `make_*` = factory
  hook naming rule.
- `probe_token(&self, tokens: &mut dyn TokenReader<'_, L>, session, state) ->
  ConstructParserResult<L, Option<L::Token>>` — no `source` parameter; the error's
  location is `error.span()`.
- `resolve_command(&self, state, name: &str, escape_char: char)` and
  `CommandResolver::resolve_command` likewise; `resolve_command_in_scopes` likewise
  (the token was only read for these two facts; hooks have no reader).
- `make_paragraph_break_node(&self, state, break_span: &SourceSpan<L::SourceOrigin>)
  -> NodeKind<L>` — replaces `(state, token, source_content)`: the span carries both
  the range (`.span()`, for the default `NodeKind::chars`) and the text (`.content()`,
  for a callable-shaped kind recording the spelling).
- `make_group_parser<'p>(&'p self, open: &L::Token, rule: Arc<GroupRule<L>>, child_states: ChildStateSpec<'p, L>)` — only the first parameter changes (`open_span: Span` → `open: &L::Token`); the `&'p self` receiver and the `'p` on `ChildStateSpec` stay (`engine/driver.rs:559-564`).
- `make_invocation_parser<'a>(&'a self, invocation: Invocation<'a, L>)` (no `'s`).
- `CallableSpec::make_invocation_parser` likewise (`techy/src/spec/callable.rs`).

### 1.11 Construct-parser layer: types and the site inventory

Public/`pub(crate)` types that carried bare positions or spans:

| Type | Was | Becomes |
|---|---|---|
| `TokenRecovery` | `{ token, resume_pos: usize }` | `{ token: L::Token, resume: L::StreamPosition }` |
| `StopCause` (`nodes_parser.rs`) | `TokenCondition { span: Span }`, `UnexpectedGroupClose { span: Span }` | `StopCause<L>`: `TokenCondition { span: SourceSpan, after: L::StreamPosition }`, `UnexpectedGroupClose { span: SourceSpan, after: L::StreamPosition }` (`after` = the position past the token, `EndPastPostSpace`); `NodeCondition`, `EndOfInput` unchanged |
| `ArgumentNoise` (`argument_parsers.rs`) | `start: usize` | `start: L::StreamPosition` (= `position_here()` before the scan); `rewind` = `move_to_position(&start)` |
| `NameGroup` (`environment_parser.rs`) | `name_span: Span, end: usize` | `name: SourceSpan` (the name between the delimiters; `.content()` is the text), `end: L::StreamPosition` |
| `EnvironmentBody` | `end: usize` | `end: L::StreamPosition` — kept: it is the body's stream end position; the driving parser computes the environment span with `source_span_within(begin_start, &body.end)` |
| `EnvironmentTerminatorSyntaxData` | `Scanned { command_word: Span, post_space: Span, .. }`, `Literal { span: Span }` | the same variants with `SourceSpan` fields (parse output, not node data). The composition that records them on the node (`latexlike/environments.rs`) converts to node-relative `Span`s with `same_source(&node_span)` + `.span()`, and records "not scanned" (`None`) if a fact is not in the node's source |
| `GroupParser` | `open_span: Span` | `open: L::Token` |
| `RawContentEnd` and similar internal helpers (`verbatim_parser.rs`) | `usize` positions | `L::StreamPosition` / `SourceSpan` (implementer's choice, same principle) |
| `stage_pre_space(cx, nodes, pre_space: Span)` (`argument_parsers.rs`) | | `stage_pre_space(cx, nodes, tok: &L::Token)` — stages `source_span_between(tok, StartBeforePreSpace, Start)` when non-empty |
| `stage(cx, kind, span: Span)` — **two** functions of that shape: the free `pub(super) fn stage` (`argument_parsers.rs:211`) and the private `NodesParser::stage` (`nodes_parser.rs:605`); both do `SourceSpan::new(&cx.source, span)` | | `stage(cx, kind, span: SourceSpan)` in both |

**Crate facts that changed after this plan was drafted** (commits `1c59e66..9a3c0ac`,
almost all of them the new `techy::serialize` module) — an implementer writing new code
in a touched file must honor them:

- `CallableSpec<L>` and `SpecsProvider<L>` gained a **`SerializableObject<L>`
  supertrait** (`spec/callable.rs:97`, `scopes/mod.rs`). Every spec/provider type —
  including every *test* spec this plan rewrites or adds — needs the one-line empty
  `impl crate::serialize::SerializableObject<TheLang> for TheSpec {}`.
- `StdCallableSpec` gained fields: a struct literal now needs
  `StdCallableSpec { arguments, ..Default::default() }`.
- `NodeKind::List` is a **unit** variant (was `List { .. }`).
- `minidefs::minilatex_package()` / `minilatex_item_package()` /
  `latexlike::builtin_package()` return `Arc<Package<LLL>>` (was `Package<LLL>`).
- The rustdoc link to the panics list is now `[Panics list](techy::guide::panics)`
  (was `[crate-level Panics list](crate#panics)`) — the `StdToken` constructors use
  the new spelling.
- `Lang` itself is **unchanged** since `1c59e66` (`git diff 1c59e66..HEAD --
  techy/src/state/lang.rs` is empty): the plan's "`Lang` gains exactly `Token`,
  `StreamPosition`, and the `scan_specials` re-signature; no other `Lang` change"
  still holds.
- `techy/src/serialize/**` never reads a token, a `TokenReader`, a `StdTokenReader`,
  `scan_specials`, or a span taken from a token. It serializes *node* data
  (`CommentData::post_space`, the latexlike `InvocationSyntaxData::Macro { post_space }`
  and `StdEnvironmentSideSyntax`, all already `TextContent`/`Span` node facts) and
  `TokenRules` (state). Its only contact points with this plan are the 4 `impl Lang`
  sites listed in §1.2 and two stub `ArgumentParser` impls that name
  `ParseContext<'_, '_, L>` (`serialize/tests.rs:78`,
  `serialize/drivers/tree_tests.rs:69`) — unaffected, since `ParseContext` keeps both
  lifetime parameters.

Site inventory (every non-test use of `move_to_pos`/`pos()`/`cx.source` outside the
token module, with its replacement). Test-only sites follow the same rules.

Until the end of Stage 2b the new edge-based method is spelled
`move_to_edge(&tok, TokenEdge)`; every `move_to(&x, TokenEdge::…)` written in this
table means `move_to_edge` while the old two-flag `move_to` still exists; 2b deletes the
old method and renames `move_to_edge` → `move_to` in one commit (afterwards
`grep -rn move_to_edge techy docs` is empty).

| Site | Today | Replacement |
|---|---|---|
| `argument_parsers.rs:148,174` `ArgumentNoise` | `start = pos()`, `move_to_pos(start)` | `start = position_here()`, `move_to_position(&start)` |
| `argument_parsers.rs` `Span::empty(cx.tokens.pos())` (5 sites) | diagnostic anchor | `cx.here()` |
| `embellishments_parser.rs:157` | `move_to_pos(noise.start)` | `move_to_position(&noise.start)` |
| `embellishments_parser.rs:252,269` | `move_past(first, true)`, `move_past(&token, true)` | `move_to(&tok, EndPastPostSpace)` |
| `embellishments_parser.rs:276-280` | `best: Option<(usize, usize)>` (index + end offset), `move_to_pos(match_end)` after over-scanning, and the returned `Span::new(first.span.start(), match_end)` | keep the best-so-far **token** in `best` (`L::Token` is `Clone`); `move_to(&best_token, EndPastPostSpace)`; the returned span becomes a `SourceSpan` = `cx.source_span_within(&position_at(first, Start), &position_at(&best_token, EndPastPostSpace))?` (the caller's use of that span changes with it — check `:157` and the marker-staging site) |
| `environment_parser.rs:271-294` `read_rigid_name_group` | `entry = pos()`, `move_to_pos(entry)` | `move_to(&open, StartBeforePreSpace)` (or `entry = position_here()` + `move_to_position`) |
| `environment_parser.rs:549` `after_command = pos()`, `:586` drift check | | `after_command = position_here()`; `position_here() != after_command` |
| `environment_parser.rs:577` mismatch | `move_to(&end_token, false)` | `move_to(&end_token, Start)` |
| `environment_parser.rs:641-708` body start/anchors | `pos()` | `position_here()`, `cx.here()` |
| `environment_parser.rs:1109` test `RawBlockParser` | `move_to_pos(trigger.post_space().start())` | `move_to(self.invocation.token, End)` (compiles now: no lifetime coupling) |
| `verbatim_parser.rs:318-347` | `entry = pos()`, `move_to_pos(entry)` ×3 after non-consuming probes | delete (no-ops) — or `entry = position_here()`/`move_to_position` if the reviewer prefers explicitness |
| `verbatim_parser.rs:169,365,693,726` | `content_start/end = pos()` | positions/`source_span_within` |
| `nodes_parser.rs:826-828` recovery arm | `before = pos(); move_to_pos(resume_pos); pos() <= before` | `before = position_here(); move_to_position(&recovery.resume); position_here() == before → abort` |
| `nodes_parser.rs:512-563` chars run (`take_pre_space`, `extend_run`, `flush`) | `Span` arithmetic + `SourceSpan::new(&cx.source, run)` | run = `Option<(L::StreamPosition, L::StreamPosition)>`; extend: `position_at(&tok, StartBeforePreSpace) == run.end` else the existing "gap-free contract" implementation error; `run.end = position_at(&tok, EndPastPostSpace)` (pre-space-only extension: `Start`); flush: `cx.source_span_within(&start, &end)?` → `NodeKind::chars(span.span())` staged with `span` |
| `nodes_parser.rs` stop-token matching, dispatch | `token.kind` | `cx.tokens.token_kind(&token)` |
| `group_parser.rs:189-227` | `Span::empty(pos())`, `(TextContent::Spanned(span), span.end())`, `SourceSpan::new(&cx.source, ..)` | `cx.here()`; close = `Spanned(span.span())` if `span.same_source(&node_span)` else `Owned(span.content())`; end position from `StopCause::after` / `position_here()`; node span = `cx.source_span_within(&position_at(&open, Start), &end)?` |
| `invocation_parser.rs:64` | `SourceSpan::new(&cx.source, Span::empty(pos()))` | `cx.here()` |
| `tack_on_parser.rs:175`, `:206` | `Span::empty(cx.tokens.pos())`, `SourceSpan::new(&cx.source, token.span)` | `cx.here()`, `cx.tokens.source_span_of(&token)` |
| `chars_group_parser.rs:180` | `TokenKind::GroupOpen { rule, .. }` match | `cx.tokens.token_kind(&token)` (Stage 3a only — this file has **no** `pos()`/`move_to_pos`/`cx.source` site) |
| `constructs/mod.rs:103-117` `invocation_frame` | `SourceSpan::new(&cx.source, ..)` | §1.9 |
| `constructs/mod.rs:262-330` `stage_invocation` | `end_pos: Option<usize>`, `self.source.content().get(..)` | §1.9 |
| `constructs/mod.rs:434,445` guard anchors, `:795` | `pos()` | `self.here()` |
| `attached_source.rs:155-162` reader construction, `:195` skip | `StdTokenReader::new(content)`, `move_to_pos(span.end())` | `driver.make_token_reader(&source)`, `move_to_position(&after)` |
| `engine/language.rs:184,245,269` | reader construction, skip, anchor | same |
| `latexlike/invocation_syntax.rs:779-784` test `RestOfLineParser` | raw `find('\n')` + `move_to_pos(end)` | read `Char` tokens under a derived state with every feature gate off (the verbatim recipe) until a `'\n'` char, `move_to(&last, EndPastPostSpace)`; `stage_invocation(.., Some(&position_here()))` |
| `latexlike/*.rs` (environments, input, driver, recompose) | `SourceSpan::new(&cx.source, ..)`, `token.kind` | reader queries / view |
| `scopes/mod.rs:1607-1620` `ErrorCallableSpec::make_invocation_parser`, `:1624-1654` `ErrorInvocationParser` | **a real (non-test) construct parser the plan never named**: `Invocation<'a, 's, L>`, `token.span`, `SourceSpan::new(&cx.source, token.span)` ×2, `NodeKind::chars(token.span)`, `cx.staging_error(error, token.span)` | Stage 2a/2b: `cx.tokens.source_span_of(self.invocation.token)` for both spans, `NodeKind::chars(span.span())`, `staging_error(error, span)`; Stage 3b: `Invocation<'a, L>`, `ErrorInvocationParser<'a, L>` |
| `scopes/mod.rs:104-115` `CallableQuery<'a, 's, L>` | public field `token: Option<&'a Token<'s, L>>` | `CallableQuery<'a, L>` with `token: Option<&'a L::Token>` (Stage 3b). **Open question — §1.16 does not cover it**: a `SpecsProvider` has no reader, so an opaque token is uninterpretable to it. Default if nobody rules otherwise: keep the field, and document that interpreting it needs a reader over the same content |
| `latexlike/invariants.rs:60` | doc comment: "a takeover's `stage_invocation(.., end_pos: Some)`" | reword to `end: Some(&position)` (Stage 2a, with the rename) |
| `spec/callable.rs:162-171` `CallableSpec::make_invocation_parser<'a, 's>` | `invocation: Invocation<'a, 's, L>` | `make_invocation_parser<'a>(&'a self, invocation: Invocation<'a, L>, ..)` (§1.10) — Stage 3b |
| `latexlike/spec.rs:132-134` | `make_invocation_parser<'a, 's>(.., Invocation<'a, 's, LLL>)` | same — Stage 3b |
| `constructs/child_state.rs:106` | `&Invocation<'_, '_, L>` in the compute-closure type | `&Invocation<'_, L>` — Stage 3b |
| every type whose `'s` comes only from a token or an `Invocation` | `ArgumentNoise<'s, L>` (`argument_parsers.rs:126`), `MintedGroupMatch<'s, L>` (`:710`), `StdInvocationParser<'a, 's, L>` (`invocation_parser.rs:164`), `CallableQuery<'a, 's, L>` (`scopes/mod.rs:104`), `ErrorInvocationParser<'a, 's, L>` (`scopes/mod.rs:1624`), `EnvironmentInvocationParser<'a, 's, LLL>` (`latexlike/environments.rs:766`), `OrphanEndParser<'a, 's, LLL>` (`:990`), `InputInvocationParser<'a, 's, LLL>` (`latexlike/input.rs:291`), `AfterEffectInvocationParser<'a, 's, LLL>` (`latexlike/spec.rs:157`), and the test parsers `DefParser<'a, 's>` (`argument_parsers.rs:2143`, `nodes_parser.rs:2995`), `TakeParser<'a, 's>` (`nodes_parser.rs:3079`), `EnvironmentInvocationParser<'a, 's>` (`environment_parser.rs:944`), `RawBlockParser<'a, 's>` (`:1091`), `RestOfLineParser<'a, 's>` / `BadEndParser<'a, 's>` (`latexlike/invocation_syntax.rs:764, 824`) | **Stage 3b drops the `'s` from all of them.** `ParseContext<'a, 's, L>` is the one exception: its `'s` comes from `TokenReader<'s, L>` and stays |
| `token/mod.rs:50-67` (internal module facade) | `pub use` of `Token`, `TokenKind`, `TokenError`, `TokenRecovery`, `SpecialsMatch`, `StdTokenReader`, `TokenReader` | add `TokenEdge`, `StdStreamPosition` (Stage 1), `SpecialsScanError` (Stage 1), `StdToken` + the `Token` **trait** (Stage 3b). `techy::core` re-exports through this module, so it is edited first |
| `core/mod.rs:60-66` (public facade) | the single `pub use crate::token::{ … };` block | add `StdStreamPosition`, `TokenEdge`, `SpecialsScanError` (Stage 1); add `StdToken` and let the existing `Token` entry export the **trait** (Stage 3b). `TokenKindView` is never exported. `Span`/`SourceSpan` are **not** re-exported here — they live on `techy::source` (`source/mod.rs:69-72`) |
| `core/constructs.rs:54-70`, `core/specs.rs:40-52` (public facades) | re-export `ArgumentNoise`, `EnvironmentBody`, `EnvironmentTerminatorSyntaxData`, `FromInvocation`, `GroupParser`, `Invocation`, `NameGroup`, `NodesOutcome`, `StopCause`, `stage_pre_space`, `read_rigid_name_group`, `resolve_command_in_scopes` | **no edit**: these are name-only re-exports and no name changes. Stated so an implementer does not go looking |
| `serialize/drivers/{tree_tests.rs:1280,1397, diagnostic_tests.rs:45, tests.rs:45}` | 4 `impl Lang for …` test languages in the `serialize` module (added after this plan was drafted) | the two associated types (§1.2); nothing else in `techy/src/serialize/**` touches tokens, readers, `ParseContext` spans, or `scan_specials` |
| `token/rules.rs:20-21,251,288`, `token/mod.rs:22,34`, `state/mod.rs:27`, `docs/ai-guide.md:244` | prose/intra-doc mentions of `Lang::scan_specials`, `SpecialsMatch`, `TokenRecovery` "resume position" | reword to the new signatures; the intra-doc link targets survive the re-signature, so this is prose only (Stage 1 step 5 for the module docs, Stage 4 for `docs/ai-guide.md`) |
| `docs/construct-parsers.md:58-63` (intra-doc links to `TokenReader::{move_past, move_to_pos, pos}` — `broken_intra_doc_links = deny`, so deleting those methods **breaks the docs build** until this is rewritten), `:195`, `:211`, `:334-384` (`Invocation<'a, 's, Latexlike>` in the doctest's `make_invocation_parser`), `:399-455` (the doctest body: `pos()`, `move_past`, `token.kind`, `token.span`, `SourceSpan::new(&cx.source, ..)`) | prose + doctest on the old API | rewrite to the new API (doctests must compile) |
| `docs/ai-guide-custom-lang.md:263`, `:274` | `cx.tokens` row; `move_to_pos(token.post_space().start())` | `cx.tokens` row reworded; `move_to(&token, TokenEdge::End)` |
| `docs/concepts-overview.md:36-46`, `docs/parsing-model.md:36`, `:101` | prose describing tokens as span-carrying values read by parsers | prose: opaque tokens, the reader interprets, `make_token_reader` |
| `docs/custom-lang.md:196` `impl Lang for BracesOnlyLang` (doctest), `:81`, `:89` | `impl Lang` block; `TokenReader`/`scan_specials` prose | add `type Token`/`type StreamPosition`; prose to the new hook signature |

### 1.12 Span rules at staging (unchanged model, new spelling)

- A node's span is a `SourceSpan` obtained from the reader: one token's span, or
  `source_span_within(begin, end)` for multi-token constructs.
- Node data sub-spans (`TextContent::Spanned`, `NodeKind::comment(start, content,
  post_space)`, `command_word`, `post_space`) are node-relative bare `Span`s **taken
  from a `SourceSpan` that passed `same_source(&node_span)`**; a fact from another
  source is recorded as `TextContent::Owned(text)` or as absent (`None`), per the
  recording site. Under the std reader every fact is same-source; the check is cheap
  and makes the sub-span/source pairing explicit rather than assumed.
- The parse-tree byte-partition oracle (`node/invariants.rs`, `cfg(test)`) is
  unchanged: this plan introduces no multi-source parse.

### 1.13 S0 additions (`techy/src/source/source.rs`)

- `SourceSpan::at(pos: &SourcePos<O>) -> SourceSpan<O>` — the empty span at a
  position (mirror of `start_pos`/`end_pos`).
- Nothing else. (`same_source` and `span()` exist.)

### 1.14 Naming register

New public names (all checked against `dev-docs/ARCHITECTURE.md` [§dd-arch:naming]:
generic over specific, specificity, clarity over brevity, `…Kind` = closed core enum,
`make_*` = factory hook): `Token` (trait), `StdToken`, `TokenKind` (the view — still the
closed core enum), `TokenEdge`, `StdStreamPosition`, `Lang::Token`,
`Lang::StreamPosition`, `SpecialsScanError`, `TokenReader::{move_to, move_to_position,
token_kind, source_span_between, source_span_of, position_here, position_at,
source_position_at, source_span_within}`, `ParseContext::{here, source_span_within}`,
`ParseDriver::make_token_reader`, `SourceSpan::at`, `StopCause::*::after`,
`Invocation::kind`.

Superseded (must not come back — recorded in DESIGN_RATIONALE [§dd-dr:superseded-names]
in Stage 5, §7): `Token::new` (the struct constructor; the
struct itself is now `StdToken`), `TokenKind` variants with `&'s str`/`Span` fields,
`Token<'s, L>` (lifetime on the token), `TokenReader::{move_past, move_to(tok, bool),
move_to_pos, pos}`, `StdTokenReader::{pos, move_to_pos}`, `TokenRecovery::resume_pos`,
`ParseContext::source`, `stage_invocation(.., end_pos: Option<usize>)`,
`SpecialsMatch::name`, `SpecialsMatch<'s, L>`, `TokenResult<'s, L, T>`,
`Invocation<'a, 's, L>`, `make_paragraph_break_node(.., token, source_content)`,
`probe_token(.., source, ..)`.

### 1.15 Rustdoc contracts to write (checked by reviewers)

On `TokenReader`: clauses 1–6 of §1.6; the custom-reader-over-std-tokens pattern
(§1.8). On `Lang::Token`/`Lang::StreamPosition`: opacity ("a construct parser holds
and passes; only the reader interprets"). On `TokenRecovery::resume`: the advancement
contract. On `ParseContext`: "no source handle — spans come from `cx.tokens`; the
preferred spellings are `here()`, `source_span_within()`, `cx.tokens.source_span_of()`".
On `stage_invocation`: the standard end rule as spelled out in §1.9 (three cases). On
`StdToken` constructors: the coherence asserts (+ the `docs/panics.md` Panics list —
the `Token::new` entry replaced by the constructors and the "Six value functions" count
corrected; CLAUDE.md rule 4 is *not* edited by a code stage, orchestrator decision
pending). On the
`docs/construct-parsers.md` guide: the "how do I get a span / go back" FAQ rewritten
in terms of tokens, edges, and positions.

### 1.16 Small decisions with defaults (do not ask; do record in PROGRESS.md)

- **`Token` marker trait vs bare bounds on `Lang::Token`**: marker trait (decided).
- **`source_span_between` with equal edges** returns the empty span at that edge.
- **`StdTokenReader::source_span_between` on a token whose offsets lie outside its
  content** (only reachable by handing it a foreign token): relies on
  `SourceSpan::new`'s registered always-on assert (contract violation by the calling
  reader). Document it; do not add an `Option`. If the reviewer objects, ask.
- **`TokenListReader` position validation** tracks issued offsets in a set (test
  code; cost irrelevant).
- **`StopCause` gains an `L` parameter** (`StopCause<L>`); update `NodesOutcome<L>` and
  every `match`. Keep `PartialEq`/`Debug` (manual impls if derives demand `L:` bounds).
- **`ArgumentNoise` keeps `next: Option<L::Token>`** as today (the peeked non-noise
  token).
- **`Invocation.kind`**: keep as a field (a `Copy` view); if the probe shows a
  lifetime problem holding a `TokenKind<'a, L>` in the struct, drop the field and have
  consumers call `tokens.token_kind(invocation.token)`.
- **`is_at_end()` on `StdTokenReader`**: keep if any caller remains, else delete.
- **Chars-run contiguity failure message**: keep it an `ImplementationError` with the
  two positions' `Debug` renderings.
- **Doc example that hand-builds tokens** (`docs/*.md`): rewrite with the `StdToken`
  constructors + a delegating reader, or replace by a `StdTokenReader`-based example
  — whichever keeps the doctest short.

---

## 2. Stage 0 — compiler probe (throwaway; report only)

Purpose: verify, in an afternoon and with mock types, that the compiler accepts the
shapes in §1 — before ~150 sites are touched. Nothing from this stage is merged
except `PROBE_REPORT.md`.

**Setup.** Worktree `bt-probe` off `main`. Create a standalone crate
`bettertokens-probe/` at the workspace root (**not** added to `[workspace] members`;
run `cargo check`/`cargo test` inside it; `edition = "2021"`, `rust-version = "1.86"`).
A package directory *inside* a Cargo workspace that is not listed in
`[workspace] members` fails with "current package believes it's in a workspace when
it's not": the probe crate's own `Cargo.toml` must therefore contain an **empty
`[workspace]` table**, which makes it its own workspace. The root `Cargo.toml` stays
untouched.
Mock the minimum: `trait Lang: 'static { type Token: Token<Self>; type StreamPosition:
Clone + Debug + PartialEq + Eq + Send + Sync; type CallableTypeId: Copy; }`, a `Span`,
a `Source` with `content: String`, `SourceSpan { source: Arc<Source>, start, end }`,
`SourcePos`, `Arc<dyn CallableSpec>`-like `Arc<dyn Any + Send + Sync>` where a spec is
needed, `Arc<GroupRule>` as `Arc<String>`.

**Probes (each a compiling snippet or a documented compile failure):**

- P1 **Object safety**: `trait TokenReader<'s, L: Lang>` exactly as §1.6 (all methods,
  incl. `token_kind<'t>(&self, tok: &'t L::Token) -> TokenKind<'t, L> where 's: 't`),
  then `let r: &mut dyn TokenReader<'_, MockLang> = &mut std_reader;` and calls to every
  method through the `dyn`. Also `Box<dyn TokenReader<'s, L> + 's>` returned by
  `fn make_token_reader<'s>(&'s self, source: &'s Arc<Source>) -> Box<dyn TokenReader<'s, L> + 's>`
  on a mock driver, and stored as the `tokens` field of a mock
  `ParseContext<'a, 's, L> { tokens: &'a mut dyn TokenReader<'s, L>, .. }`.
- P2 **View held across mutable reader use**: mock `StdToken` (spans only) +
  `StdTokenReader<'s>` (content `&'s str`) implementing `token_kind` by slicing content
  and returning `TokenKind<'t, L>` with `&'t str` names (requires `'s: 't`). Then the
  nodes-parser shape: `peek` → token on the stack; `let TokenKind::Command { name, .. }
  = cx.tokens.token_kind(&tok) else { .. }`; build `Invocation<'a, L> { name, token:
  &tok, kind }`; call a function taking `&mut ParseContext` (which calls
  `cx.tokens.move_to(..)` and `cx.tokens.peek(..)`) **while `invocation` is alive and
  used afterwards**. Must compile.
- P3 **Token-owning reader**: a second mock reader whose token holds `Arc<Source>`
  and spans (no lifetime), implementing `token_kind` by borrowing through the token's
  `Arc` — must satisfy the same trait signature. (This is the shape a future expanding
  reader takes; it proves the `'s: 't` clause does not force strings to live in the
  content.)
- P4 **Wrapper reader**: a reader that mints `StdToken`s itself and delegates all
  interpretive methods to an inner `StdTokenReader` (the `CommentEmittingReader`
  rewrite shape). Both readers used through `dyn` for the same `L`.
- P5 **`L::StreamPosition` in lifetime-free outputs**: mock `StopCause<L>`,
  `NodesOutcome<L>`, `EnvironmentBody<L>` holding `L::StreamPosition`; manual
  `Debug`/`PartialEq`/`Clone` impls without `L:` bounds compile.
- P6 **`FromInvocation::from_invocation(&Invocation<'_, L>, &dyn TokenReader<'_, L>)`**
  called from inside a method that holds `cx: &mut ParseContext` (reborrow `&*cx.tokens`
  for the call). Must compile.
- P8 **The `StdTokenReader` impl-bound spelling**: mock `trait SourceOrigin`,
  `Source<O: SourceOrigin>`, `SourceSpan<O>`, `Lang { type SourceOrigin: SourceOrigin;
  … }`, and write
  `impl<'s, O: SourceOrigin, L: Lang<SourceOrigin = O>> TokenReader<'s, L> for StdTokenReader<'s, O>`.
  Use it through `&mut dyn TokenReader<'_, L>` **and** via
  `Box<dyn TokenReader<'s, L> + 's>` returned from `make_token_reader`. Must compile;
  if it does not, report the error **and** the fallback spelling that does compile
  (e.g. `StdTokenReader<'s, L>`, generic over the language rather than over the
  origin). P8 settles the spelling §1.8 leaves open.
- P7 **Trait bounds on `Lang::Token`**: `StdToken<L>: Clone + Debug + PartialEq + Send
  + Sync` with `Arc<dyn Trait + Send + Sync>` inside; a `Lang` impl naming
  `type Token = StdToken<Self>` (recursion through `Self` is fine? — confirm).

**Deliverable.** `dev-docs/bettertokens/PROBE_REPORT.md`: per probe, PASS with the
final signatures used, or FAIL with the error and the fallback chosen (§9). If P1
fails on `where 's: 't`, fallback A: keep `dyn` and declare `token_kind<'t>(&'t self,
tok: &'t L::Token) -> TokenKind<'t, L>` **and** verify P2 still passes with a
`Invocation.name: String` copy (allocation per invocation — report the trade-off);
fallback B: static dispatch (`Lang::TokenReader<'s>` GAT). Either fallback needs the
user's ruling — stop and ask. Merge only the report (copy it into the next stage's
worktree; the probe branch is deleted).

**Gate.** `cargo check`/`cargo test` green in the probe crate; report written.

---

## 3. Stage 1 — positions, spans, the reader door (old API kept alongside)

Everything in this stage compiles and passes tests with the old positional API still
present; new API is added, `TokenError`/hooks change, reader construction moves behind
the driver. `Token<'s, L>` stays as is.

Branch `bt-1-positions` (worktree), chained off `main` after Stage 0's report merge.

Steps:

1. `token/reader.rs`: add `TokenEdge` (§1.4), `StdStreamPosition` (§1.5). Add to
   `TokenReader` (temporarily *alongside* `move_past`/`move_to(bool)`/`move_to_pos`/
   `pos`): `move_to(&tok, TokenEdge)` — **name clash** with the old two-flag `move_to`:
   introduce the new one as `move_to_edge` in this stage and rename to `move_to` in
   Stage 2 when the old one is deleted — `move_to_position`, `source_span_between`,
   `source_span_of` (default), `position_here`, `position_at`, `source_position_at`,
   `source_span_within`. `next()` default unchanged for now.
2. `StdTokenReader::new(source: &'s Arc<Source<O>>)` (was `&'s str`); implement the new
   methods; keep the old ones for now. Update every construction site (2 real + tests).
3. `Lang::StreamPosition` + `type StreamPosition = StdStreamPosition;` in all 57
   explicit `impl Lang for …` sites of the §1.2 table — 53 in `techy/src`
   (including the blanket `impl<T: TrivialLang> Lang for T` at `state/lang.rs:524`,
   which covers every `impl TrivialLang` site), 3 in `techy/tests/lang_features.rs`,
   and the **doctest** at `docs/custom-lang.md:196` (it must compile). Four of the
   `techy/src` sites are in `techy/src/serialize/**`
   (`drivers/tree_tests.rs` ×2, `drivers/diagnostic_tests.rs`, `drivers/tests.rs`) —
   a module that did not exist when this plan was drafted.
4. `token/error.rs`: `TokenError { kind, span: SourceSpan, recovery }`, `TokenRecovery {
   token, resume: L::StreamPosition }`, `TokenResult<'s, L, T>` keeps `'s` for now
   (token still has it). Update the std reader's recovery construction and the content
   loop (`nodes_parser.rs:803-833`: `move_to_position(&resume)`, `position_here()`
   equality check), `ParseDriver::probe_token` (drop `source`), `ParseContext::probe_token`.
5. `token/specials.rs` + `state/lang.rs` + `scopes/mod.rs` (`SpecsProvider`, `Package`,
   `ScopeStack`) + `latexlike/mod.rs`: `SpecialsScanError`, `SpecialsMatch<L>` without
   `name`, `scan_specials(..) -> Result<Option<SpecialsMatch<L>>, SpecialsScanError>`;
   the std reader lifts scan errors (`recovery: None`) and slices `content[pos..end]`
   for the token's `Specials { name }` string. Update the test hooks that produced
   recoveries (`nodes_parser.rs:2296,2376`, `environment_parser.rs:2010`): they now
   either return `Ok(None)`/a diagnosing-spec match, or an `Err` (unrecoverable) —
   keep each test's intent (a recoverable token error is still exercised through the
   std reader's own recoveries: forbidden char, dangling escape).
6. `engine/driver.rs`: `make_token_reader` with the default; route
   `Language::parse_source` and `parse_attached_source` through it.
7. `source/source.rs`: `SourceSpan::at(&SourcePos)`.
8a. `techy/tests/lang_features.rs:466-490` `CommentEmittingReader` — a **hand-written
   `TokenReader` impl** (currently exactly `peek`, `move_past`, `move_to(bool)`,
   `move_to_pos`, `pos`). The new trait methods have no defaults, so this impl stops
   compiling the moment step 1 lands: it must be rewritten in **Stage 1**, not deferred
   to 3b. It also holds **no source** today (the `Arc<Source>` was supplied to
   `ParseContext::new`), so it must be given one to answer `source_span_between` /
   `source_position_at`. §1.8's "delegating wrapper over an inner `StdTokenReader`" is
   the shape; 3b then only swaps `Token::new` for the `StdToken` constructors.
8. `TokenListReader` (test): the constructor
   (`techy/src/token/list_reader.rs:57`) goes from
   `pub fn new(tokens: Vec<Token<'s, L>>) -> TokenListReader<'s, L>` to
   `pub fn new(source: &'s Arc<Source<L::SourceOrigin>>, tokens: Vec<Token<'s, L>>)`.
   **25 construction sites** at `9a3c0ac` (`grep -rn "TokenListReader::new(" techy/src
   techy/tests docs`): `engine/mod.rs` 11, `token/list_reader.rs` 8,
   `constructs/mod.rs` 3, `constructs/argument_parsers.rs` 1,
   `constructs/environment_parser.rs` 1, `constructs/nodes_parser.rs` 1 — update all.
   Implement the new methods; add issued-token/position validation (§1.8).
9. **Facade exports** — new public types get their one canonical public path in the
   stage that introduces them (CLAUDE.md: exactly one public path per item). Add
   `TokenEdge`, `StdStreamPosition`, `SpecialsScanError` to the internal module facade
   `techy/src/token/mod.rs:50-67`, then to the public facade — the single
   `pub use crate::token::{ … };` block at **`techy/src/core/mod.rs:60-66`**, next to the
   existing `StdTokenReader, Token, TokenError, TokenErrorKind, TokenKind, TokenReader,
   TokenRecovery, TokenResult, TokenRules` entries. Nothing is removed. (`Span` and
   `SourceSpan` are *not* re-exported from `techy::core`; they live on `techy::source`.)
10. Rustdoc for every new item (`missing_docs` is deny); contract clauses 1–6 (§1.6)
    on the trait now (they hold for the std readers already).

Gates (all stages): `cargo build`, `cargo test` (unit + integration + doctests),
`cargo clippy --all-targets -- -D warnings`, `rm -rf target/doc && cargo docs`
(link check), `scripts/check_semver.sh` runs and its report is saved to PROGRESS.md.

- **Clippy**: `main` at `9a3c0ac` **is clean** under `cargo clippy --all-targets --
  -D warnings` (verified, exit 0). The gate is therefore "**clean**", not "no new
  warnings" — a stage that leaves a single warning fails the gate.
- **Docs**: `broken_intra_doc_links = deny`, and `docs/*.md` are included into rustdoc
  through `techy/src/lib.rs`. Deleting `TokenReader::{move_past, move_to_pos, pos}`
  therefore *breaks the docs build* until `docs/construct-parsers.md:58-63` is
  rewritten — that rewrite belongs to the same stage as the deletion (2b), not to
  Stage 4.
- **Semver**: `cargo-semver-checks` 0.50.0 is installed; the baseline is the
  `api-baseline` **branch** (`scripts/check_semver.sh`, `BASELINE_REV` overrides it).
  The script clears `RUSTDOCFLAGS` itself (the workspace injects
  `docs/rustdoc-header.html` by a root-relative path that does not resolve in
  cargo-semver-checks' scratch builds) — do not set `RUSTDOCFLAGS` around it.
  Breaking changes are *expected*: soft freeze; capture the report, do not "fix" them.

Reviewer checklist (Stage 1): new items match §1.4–§1.8/§1.10 signatures verbatim (or
the probe report's settled variants); contract clauses present; no behavior change in
scanning (`token/reader.rs` tests unchanged and green); lockstep harness green; the
list reader's validation is exercised by at least one new negative test (a forged
token panics); `SpecialsScanError` lift is unrecoverable; every changed `Lang` impl
compiles; no `dev-docs/ARCHITECTURE.md`/`DESIGN_RATIONALE.md` edits (those belong to
Stage 5, §7).

---

## 4. Stage 2 — port construct parsers off bare positions; delete the old API; remove `ParseContext::source`

Split into 2a and 2b so each diff is reviewable. Old API remains until the end of 2b.

**2a — core** (`bt-2a-core` off `bt-1-positions`): `constructs/mod.rs`
(`ParseContext::{here, source_span_within}`, `implementation_error`/`staging_error`
take `SourceSpan`, `stage_invocation(end: Option<&L::StreamPosition>)` with the
standard rule of §1.9, `invocation_frame`, `parse_group(open: &L::Token, ..)`,
`parse_construct`/`recover_derive_failure` anchors), `nodes_parser.rs` (chars run on
positions; `StopCause<L>` with `after`; recovery arm already done), `group_parser.rs`,
`argument_parsers.rs` (`ArgumentNoise.start`, `stage_pre_space(tok)`, `stage(SourceSpan)`,
anchors), `invocation_parser.rs`, `engine/language.rs` root loop, `attached_source.rs`
loop, `engine/driver.rs` (`make_group_parser(open: &L::Token, ..)`,
`make_paragraph_break_node(state, &SourceSpan)`), and the one doc comment that names
the renamed parameter (`latexlike/invariants.rs:60`). Every `SourceSpan::new(&cx.source,
..)` in these files is replaced; `cx.source` reads in these files reach zero.

**2b — the rest + deletion** (`bt-2b-rest` off `bt-2a-core`): `environment_parser.rs`
(`NameGroup`, `EnvironmentBody.end`, `EnvironmentTerminatorSyntaxData` with
`SourceSpan`s, terminator flow, `read_rigid_name_group`, test `RawBlockParser`),
`verbatim_parser.rs`, `embellishments_parser.rs`, `tack_on_parser.rs`,
`chars_group_parser.rs`, `latexlike/*.rs` (`environments.rs` conversion with
`same_source`; `input.rs`; `driver.rs`; `recompose.rs`; test `RestOfLineParser`),
`docs/*.md` (the §1.11 docs rows) and `techy/tests/lang_features.rs` (the only
integration test that names any of these symbols — verified by grep at `9a3c0ac`).
Then **delete**:
`TokenReader::{move_past, move_to(bool), move_to_pos, pos}`, `StdTokenReader::{pos,
move_to_pos}`, `TokenListReader::move_to_pos`, `ParseContext::source` (and the
`source` parameter of `ParseContext::new`), and rename `move_to_edge` → `move_to`;
`next()` default = `peek` + `move_to(EndPastPostSpace)`. Sweep — both greps must come
out as described:

- `grep -rn move_to_edge techy docs` — **empty** (the temporary name is gone).
- `grep -rn "move_to_pos\|\.pos()\|move_past\|cx\.source\b\|self\.source\b\|&source, " techy/src techy/tests docs`
  — only the legitimate hits below. The list was enumerated at `9a3c0ac`; a hit outside
  it is a missed port.

| Where (glob) | Why it is legitimate |
|---|---|
| `techy/src/source/{source,span,line_index,text_content}.rs` | the `self.source` fields of `SourceSpan`/`SourcePos`, `SourcePos::pos()`, and `SourceSpan::new(&source, …)` in their own rustdoc/tests |
| `techy/src/node/{mod,invariants,display,node_ref,tree}.rs` | `SourceSpan::new(&source, …)` in unit tests, `self.source` on the S0 values they hold, and `SourcePos::pos()` at `node/tree.rs:328` |
| `techy/src/error.rs`, `techy/src/visit.rs`, `techy/src/lib.rs:93`, `techy/src/recompose/tests.rs`, `techy/src/transform/tests.rs`, `techy/tests/derive_conditions.rs` | `SourceSpan::new(&source, …)` built from a local `source` binding in doc examples and tests — no parse context involved |
| `techy/src/serialize/**` | `SourceSpan::new(&source, …)` in tests, and `cx.source(wire.source)` at `drivers/source.rs:382` — that `cx` is a `DeserializeContext`, an unrelated method |
| `techy/src/scopes/mod.rs:2846` | `SourceSpan::new(&source, …)` in a unit test |
| `techy/src/engine/language.rs` | `SourceSpan::entire(&source)` for the root node — `source` is the parse's own binding, not a context field |
| `techy/src/token/reader.rs` | `StdTokenReader`'s **own** `self.source` field (introduced in Stage 1) |

`ParseContext::source` itself must have **zero** hits: `grep -rn "cx\.source\b\|self\.source\b" techy/src/constructs techy/src/engine techy/src/latexlike techy/src/scopes`
returns nothing outside `techy/src/token/`.

Gates: as Stage 1; plus the **timing check**, run exactly as follows:

1. Write `techy/examples/bt_timing.rs` on this branch. It generates a deterministic
   ~5 MB LaTeX-like `String` (fixed seed; mixed chars, commands, groups, comments,
   paragraph breaks), parses it with `Latexlike` (the default driver) and prints the
   elapsed wall-clock **milliseconds of the parse only** (generation excluded).
   **Commit it on the branch and delete it in Stage 4** — so the reviewer can re-run it.
2. On the branch: `cargo run --release --example bt_timing`, **5 times**; record all
   five numbers.
3. Create a throwaway worktree of `main` at
   `/Users/philippe/projects/techy/.claude/worktrees/bt-timing-main`, copy the *same*
   example file into it (untracked), run it 5 times there, then remove the worktree
   (`git worktree remove --force`).
4. Compare **medians**. Acceptance: ≤ 10 % slowdown. Record both five-number series and
   both medians in PROGRESS.md.

If the 10 % is exceeded, optimize the chars-run path (e.g. a reader method that answers
"does `tok` extend the run ending at `pos`?" in one call) before merging; if it is still
exceeded, report and ask.

Reviewer checklist (2a/2b): every site in the §1.11 inventory handled as specified;
no `Span` from a token is paired with anything but through the reader; node data
sub-spans pass through `same_source` where the source is not structurally the same
token; `StopCause::after` used for skips (no re-peek in the root loop);
`stage_invocation`'s standard rule text matches §1.9; deletions complete (grep sweep
in the report); docs doctests green; lockstep harness green; timing numbers reported.

---

## 5. Stage 3 — token opacity

**3a — the view** (`bt-3a-view` off `bt-2b-rest`): add `TokenKind<'t, L>` **as a new
view type** next to the current stored `TokenKind<'s, L>` — to avoid two types with one
name during the transition, introduce the view under the temporary name
`TokenKindView<'t, L>` and `TokenReader::token_kind(&tok) -> TokenKindView<'t, L> where
's: 't` (std: built from the stored kind's strings; list reader: same). During 3a
`TokenKindView` is `pub` in its crate-internal module but is **not** re-exported from
`techy::core` — it is renamed to `TokenKind` in 3b *before* it becomes public, so the
name `TokenKindView` never reaches a public path. It still needs a `missing_docs`-clean
rustdoc (the lint fires on `pub` items regardless of reachability). During 3a
`Invocation` **keeps its `'s` parameter** (`Invocation<'a, 's, L>`) and its new `kind`
field has type `TokenKindView<'a, L>`; 3b renames the type in place and drops the `'s`. Port **every**
`token.kind` / `TokenKind::…` match outside `token/*` to `cx.tokens.token_kind(&token)`
(files: `nodes_parser.rs`, `argument_parsers.rs`, `environment_parser.rs`,
`verbatim_parser.rs`, `embellishments_parser.rs`, `tack_on_parser.rs`,
`group_parser.rs`, `chars_group_parser.rs`, `engine/*`, `latexlike/*`, `docs/*.md`,
`tests/lang_features.rs`). `resolve_command(state, name, escape_char)` and
`CommandResolver`; `FromInvocation::from_invocation(&Invocation, &dyn TokenReader)`;
`Invocation` gains `kind`. After this stage no code outside `token/*` reads
`token.kind`, `token.span`, `token.pre_space`, or calls `token.post_space()`
(grep-verified; the chars run and pre-space staging were converted in Stage 2).

**3b — opaque token** (`bt-3b-opaque` off `bt-3a-view`): rename the struct
`Token<'s, L>` → `StdToken<L>`, drop its lifetime and strings (private data +
constructors, §1.3); rename `TokenKindView` → `TokenKind` (the only `TokenKind` left);
add `trait Token<L>` and `Lang::Token` (+ `type Token = StdToken<Self>;` in all impls);
`TokenError<L>`/`TokenRecovery<L>`/`TokenResult<L, T>` lose `'s`; `Invocation<'a, L>`,
`make_invocation_parser<'a>` (`engine/driver.rs`, `spec/callable.rs:162-171`,
`latexlike/spec.rs:132-134`) and the compute-closure type in
`constructs/child_state.rs:106`, `ParseDriver::probe_token`
signature (`Option<L::Token>`), `StdTokenReader`'s token construction, `TokenListReader`,
every test that calls `Token::new` (`token/reader.rs` ≈50, `engine/mod.rs` 5,
`nodes_parser.rs` 4, `environment_parser.rs` 2, `latexlike/*` 2, `list_reader.rs`) →
`StdToken::…` constructors (`token/reader.rs` ≈50, `list_reader.rs` 5,
`engine/mod.rs` 5, `nodes_parser.rs` 4, `environment_parser.rs` 2,
`latexlike/{invocation_syntax,driver}.rs` 1 each, `tests/lang_features.rs` 1 —
counts verified at `9a3c0ac`); `tests/lang_features.rs` (`CommentEmittingReader` →
delegating wrapper, §1.8); `docs/panics.md` Panics list; **facade exports** — in
`techy/src/token/mod.rs:50-67` and then in the `pub use crate::token::{ … };` block at
`techy/src/core/mod.rs:60-66`: the existing `Token` entry now exports the **trait**
(same public name, different item) and `StdToken` is added next to it. `TokenEdge`,
`StdStreamPosition` and `SpecialsScanError` were already exported in Stage 1;
`TokenKindView` is **never** exported (it is renamed to `TokenKind` here, and
`TokenKind` is already in the block). Drop nothing else — one canonical path per item.
Rustdoc on opacity (§1.15).

Gates: as Stage 1 (+ timing check repeated once at the end of 3b — expected
unchanged).

Reviewer checklist (3a/3b): `StdToken` has no public field/accessor exposing spans;
`TokenKind` view has no span fields and is `Copy`; `token_kind`'s `where 's: 't` is
present; no `'s` remains on `Token`/`TokenError`/`Invocation`/`SpecialsMatch`;
`Lang::Token` bound is the marker trait; the custom-reader pattern is documented and
exercised by the rewritten integration test; the doc guides compile; naming register
§1.14 respected (no superseded name reappears; run
`grep -rn "move_to_pos\|resume_pos\|Token::new\|TokenKindView\|end_pos" techy docs`).

---

## 6. Stage 4 — final sweep

Branch `bt-4-final` off `bt-3b-opaque`.

- Full gates; `cargo docs` with `rm -rf target/doc` first; `scripts/check_semver.sh`
  report captured (breaking, expected).
- `docs/*.md` prose pass: `construct-parsers.md` (reader section, "positions" FAQ,
  the verbatim example), `ai-guide-custom-lang.md` (table rows on `cx.tokens`,
  the `\verb` idiom now `move_to(token, End)`), `custom-lang.md` and
  `concepts-overview.md`/`parsing-model.md` (token/reader description: opaque tokens,
  reader interprets, `make_token_reader`), `pylatexenc-migration.md` if it mentions
  `move_to_pos`/`cur_pos`.
- `TODO_Big.md`: add the deferred items from §10 if the file tracks such items.
- `PROGRESS.md` final entry: what was merged, the timing numbers, the semver report,
  the list of §10 follow-ups.
- Delete probe/timing scaffolding (`bettertokens-probe/`, `examples/bt_timing.rs`)
  from every branch; leave `PROBE_REPORT.md`.

Reviewer (Stage 4, Opus): end-to-end read of `git diff main..bt-4-final -- techy/src/token
techy/src/constructs/mod.rs techy/src/engine/driver.rs docs/construct-parsers.md`
against §1; confirm the naming register and the deferred list; confirm no
ARCHITECTURE/DESIGN_RATIONALE edits (Stage 5 makes them).

---

## 7. Stage 5 — architecture and rationale documentation

Branch `bt-5-docs` off `main` **after Stage 4 has merged** (the docs describe merged
code, not a branch). Touches only `dev-docs/ARCHITECTURE.md` and
`dev-docs/DESIGN_RATIONALE.md` (plus `PROGRESS.md`). This is CLAUDE.md rule 7 applied to
the design session behind this plan; the material is §0–§1 of this document plus
`PROBE_REPORT.md` and any decisions recorded in `PROGRESS.md` under §1.16.

**Rules the implementer must follow** (from the two documents' own maintenance
sections and `Documentation_Structure.md` — read them first):

- Entry template of DESIGN_RATIONALE (`#### <title> [§dd-dr:<label>]`, `Status:` with
  who/context and **no dates**, the decision + the one decisive reason, `Rejected
  alternatives:` each with its killing flaw, `Revisit if:`). Short and argumentative;
  no history, no narrative of the session; **no reference to this plan, its stages,
  branches, or the probe** (content rule: no plan/phase references on `main`).
- Labels are immutable addresses: add new ones, never rename or reuse; amend existing
  entries in place where a decision supersedes them, recording a *conscious reversal*
  where that is what happened (that is the one place a date is allowed).
- **Every new `[§dd-dr:…]` entry must be referenced from ARCHITECTURE** in the same
  change (the "Decisions behind this section" lists, or inline where the structure is
  described). Gate: for each new label, `git grep -n '<label>' dev-docs/ARCHITECTURE.md`
  hits.
- Cross-references are bare labels; user-facing docs are never cited from here and
  vice-versa.
- Define every coined term on first use (token, stream position, edge, view — plain
  words, no metaphors); US English.

**DESIGN_RATIONALE — new entries** (titles/labels are proposals; keep them short):

1. *Tokens are opaque and reader-interpreted* `[§dd-dr:token-opacity]` — token = *what*
   (kind view), reader = *where* (source spans, stream positions); `Lang::Token` as a
   marker-trait-bound associated type; `StdToken` spans-only, no lifetime; the view
   borrows from the token/content, never from the reader (`token_kind<'t> … where
   's: 't`) and why (a reader-borrowed view would lock the `&mut` reader across an
   invocation parse). Rejected: `Arc<Source>` on a shared concrete token (per-token
   refcount, and one type cannot fit every reader); `Cow`-of-source; string-free
   `TokenKind` interpreted by parsers (unenforceable "which source?" assumption);
   fully reader-borrowed views; a `Lang::TokenReader` associated type (loses `dyn`
   readers and the lockstep harness — data types on `Lang`, instance from the driver).
   Revisit if: a reader needs per-token data the marker trait cannot express.
2. *Stream positions are opaque and unforgeable* `[§dd-dr:stream-position]` —
   `Lang::StreamPosition`, `position_here/at`, `move_to_position`,
   `source_span_within`; the four `TokenEdge`s and the single `move_to(&tok, edge)`;
   why `move_to_pos(usize)` (which had itself replaced a phantom-token trick) is
   retired again and how forged positions are made inexpressible (no constructor, no
   arithmetic) and detectable (the list reader rejects unissued tokens/positions);
   equality-only comparison. Rejected: bare `usize`; span-relative navigation only
   (needs a marker for "where I was"); `Ord` on positions.
3. *`ParseContext` has no source handle* `[§dd-dr:no-context-source]` — every
   `SourceSpan` a parser stages comes from the reader or another `SourceSpan`; the
   `same_source` conversion for node-data sub-spans; `here()`. Rejected: keeping
   `cx.source` for convenience (the natural code becomes the wrong code under a
   multi-source reader).
4. *The reader sees only the parsing state* `[§dd-dr:reader-context-purity]` — `peek`
   receives `&Arc<ParsingState>` only; a reader that needs the driver takes it at
   construction (`make_token_reader`); expansion depth is a reader-owned limit surfacing
   as an unrecoverable `TokenError`, not a descent-guard concern; tracebacks for
   synthesized sources come from the provenance chain, not from reader-pushed frames.
   Rejected: passing session/driver into `peek`.
5. *Specials scanning reports errors, never recoveries* `[§dd-dr:specials-scan-errors]`
   — the hook works on `&str`, knows neither token type nor positions; a document
   condition detectable during the scan is a match to a diagnosing spec; the name is
   the matched text. Rejected: hook-produced `TokenRecovery`.
6. *`make_token_reader` is the door for custom tokenization* `[§dd-dr:token-reader-door]`
   — on `ParseDriver`, default std, both construction sites route through it.
   Rejected: a parameter on the parse entry point (misses attached sources); a session
   field.
7. Amend in place. The **complete** list of entries that name a superseded symbol was
   produced at `9a3c0ac` by mapping every hit of
   `grep -n "Token::new\|move_to_pos\|resume_pos\|Token<'s\|\.pos()\|move_past\|cx\.source" dev-docs/DESIGN_RATIONALE.md dev-docs/ARCHITECTURE.md`
   to its enclosing `#### … [§dd-dr:…]` heading:

   | Entry | What it names | Line(s) |
   |---|---|---|
   | `[§dd-dr:source-cursor-retired]` | `move_to_pos` as a requirement, `TokenRecovery::resume_pos` | 311, 312 |
   | `[§dd-dr:token-model]` | `Token<'s, L> { kind, span, pre_space }`, `Token::post_space()`, `move_past`/`move_to` | 496, 555, 605 |
   | `[§dd-dr:zero-copy-tokens]` | `Token<'s, L>` holding `&'s str` (still zero-copy; the "revisit if" is now answered by opacity) | 624 |
   | `[§dd-dr:token-reader]` | the peek/`move_past`/`move_to` protocol | 638 |
   | `[§dd-dr:token-contract-hardening]` | item 4: `move_to_pos(pos: usize)` is a required method — **record the reversal**; `resume_pos` | 733, 746 |
   | `[§dd-dr:invocation-parser-factory]` | `move_to_pos(token.post_space().start())` | 3424 |
   | `[§dd-dr:stop-conditions]` | `move_past(token, true)` as the consume spelling | 3584, 3595 |
   | `[§dd-dr:panic-policy]` | `Token::new` among the always-on-assert value functions — **the eight `StdToken` constructors inherit that slot** | 5140 |
   | `[§dd-dr:tolerant-parsing]` | the placeholder token + explicit `resume_pos` | 5229 |
   | `[§dd-dr:err-means-abort]` | "the reader is already repositioned via `resume_pos`" | 5249 |
   | `[§dd-dr:resume-pos-contract]` | the whole entry is about `resume_pos` + `move_to_pos` | 5268-5285 |
   | `[§dd-dr:preset-driver-pillars]` | "an accessor serves `move_past`" | 7284 |

   Also amend `[§dd-dr:superseded-names]` (add §1.14's list) and
   `[§dd-dr:token-list-reader-demoted]` (now also the forged-token guard) — neither
   currently names a superseded symbol, so neither shows in the table.

   ARCHITECTURE sections with hits: `[§dd-arch:arch]` (the S1 line of the layer
   diagram, `ARCHITECTURE.md:103`, spells `Token<'s, L>`) and `[§dd-arch:errors]`
   (`:807`, "an explicit `resume_pos`").

   After the edits, `git grep -n 'move_to_pos\|resume_pos\|Token::new\|Token<.s' dev-docs`
   must show no stale mention.

**ARCHITECTURE — updates**: [§dd-arch:token] (token as opaque `Lang::Token`, the view,
`StdToken`, edges, positions, the reader trait's method families and contract
clauses, the custom-reader-over-std-tokens pattern, `make_token_reader`),
[§dd-arch:constructs] (`ParseContext` without `source`; how parsers obtain spans and
positions; `StopCause`/`EnvironmentBody`/`NameGroup`/`ArgumentNoise` carrying
positions), [§dd-arch:engine] (the reader door on the driver; `probe_token`),
[§dd-arch:errors] if it describes `TokenError`'s location, [§dd-arch:naming] only if a
new naming principle emerged (none expected — the register §1.14 goes to
DESIGN_RATIONALE). Add every new `[§dd-dr:…]` label to the relevant "Decisions behind
this section" list.

**Gates**: the label-reference grep above; `git grep -n 'bettertokens\|Stage [0-9]\|bt-[0-9]'
dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md` is empty; the two documents
render as Markdown (no broken lists/fences); no code changes on the branch.

**Review**: an Opus reviewer checks each entry against §1 of this plan (decisions
stated correctly, decisive reason present, rejected alternatives with flaws, revisit
clause), against the maintenance rules (template, no dates, no plan references,
ARCHITECTURE reference for every entry, immutable labels), and against
`Documentation_Structure.md`'s cross-referencing rules. Then — **before merging** —
the orchestrator sends the user the list of new/amended entries with their labels and
one-line summaries and waits for the user's OK (the user reviews design documents
personally). Merge as in §8 after the OK.

---

## 8. Execution protocol (orchestrator)

**Roles.** The orchestrator (this session) never edits source itself; it spawns one
*implementer* agent and one *reviewer* agent per stage, reads their compact reports,
resolves flagged points (asks the user when a point is a design decision), and merges.
Rationale (user direction, prior projects): keep the orchestrator's context lean so
the whole plan can run in one session; reviewers verify against this document.

**Worktrees and branches.** Every agent works in a git worktree
(`EnterWorktree` / `git worktree add`), never in the primary checkout — the user runs
other agents there concurrently. Branch chain: `bt-probe` (discarded),
`bt-1-positions` ← `main`, `bt-2a-core` ← `bt-1-positions`, `bt-2b-rest` ← `bt-2a-core`,
`bt-3a-view` ← `bt-2b-rest`, `bt-3b-opaque` ← `bt-3a-view`, `bt-4-final` ←
`bt-3b-opaque`, and `bt-5-docs` ← `main` (only after `bt-4-final` has merged). Each
code stage's implementer branches from the previous stage's branch (so stages can
start before the previous one is merged to `main`; the reviewer of stage N reviews
`git diff <prev-branch>..<branch>`).

**Merging** (user's standing procedure, no PRs): after a stage passes review, rebase
its branch onto current `main`, run `cargo test`, confirm the primary checkout is clean
(`git -C /Users/philippe/projects/techy status --porcelain` empty — if not, wait/ask),
then `git -C /Users/philippe/projects/techy merge --ff-only <branch>` **with the
sandbox bypassed** (a sandboxed merge in the primary checkout fails mid-checkout).
Never merge while the checkout is dirty. Do not push unless the user says so. Later
branches are rebased onto the new `main` before their own merge.

**Agent prompts.** Implementer prompt = "You are implementing Stage N of
`dev-docs/bettertokens/PLAN.md` (read §0, §1, and §N in full; also PROGRESS.md and
PROBE_REPORT.md). Work only in worktree `<path>` on branch `<name>` (already created).
Code stages: do not touch dev-docs/ARCHITECTURE.md or DESIGN_RATIONALE.md (Stage 5
does). Stage 5: touch only those two files and PROGRESS.md. Follow CLAUDE.md
(naming, panic policy, Result-not-panic, tests for new behavior, US English). Run the
gates of §3 (Stage 5: the gates of §7) before reporting. Report: (1) what changed per file, (2) gate results
verbatim (test counts, clippy, docs), (3) any deviation from §1 with reason, (4) any
open question — do not decide design questions yourself. Commit in small logical
commits with the trailer `Claude-Session:` line as configured." Reviewer prompt =
"You are reviewing Stage N of `dev-docs/bettertokens/PLAN.md` in worktree `<path>`,
branch `<name>`, base `<prev>`. Read §1 and §N; run every gate yourself; read the full
diff (`git diff <prev>..<name>`); check the stage's reviewer checklist item by item;
check naming against dev-docs/ARCHITECTURE.md [§dd-arch:naming] and the register
§1.14; check the panic policy (no new lib panics except through the registered
exceptions named in §1); check that no site of the §1.11 inventory was missed
(grep). Report PASS/FAIL per checklist item with file:line evidence, plus a list of
required fixes. Do not fix things yourself."
**Every subagent this plan spawns — implementers, reviewers, and any helper agent —
runs with `model: "opus"`** (user directive, 2026-08-17).
Reviewers get a fresh context (never the implementer's).

**Fix loop.** Reviewer FAIL → send the required-fixes list to the implementer
(SendMessage, same agent, same worktree) → re-review the delta only. Two failed rounds
on the same point → escalate to the user.

**State on disk.** `dev-docs/bettertokens/PROGRESS.md` (committed on each stage
branch and merged with it): per stage — branch, worktree path, status (started /
implemented / reviewed / merged), gate results, timing numbers, decisions taken under
§1.16, open questions and their answers. Any fresh session must be able to resume
from `PLAN.md` + `PROGRESS.md` + `git log`.

**Never end a turn without live children or a final report** (a stalled orchestrator
is the failure mode seen in earlier multi-stage runs); if a child's report reads like
a mid-flight status, nudge it.

**Context budget.** If the orchestrator's context grows large (roughly beyond 60 % of
the window), write PROGRESS.md, hand off: start a fresh session with "continue
`dev-docs/bettertokens/PLAN.md` from PROGRESS.md".

---

## 9. Risk register and fallbacks

| Risk | Detect | Fallback |
|---|---|---|
| `TokenReader` not object-safe with `token_kind<'t>(&self, ..) where 's: 't` | Stage 0 P1 | (A) `&'t self` receiver + `Invocation.name: String`; (B) static dispatch via `Lang::TokenReader<'s>` GAT. **Ask the user** which; do not proceed on a guess. |
| Borrow conflict holding a `TokenKind<'a, L>` in `Invocation` across a sub-parse | Stage 0 P2 | drop `Invocation.kind`; consumers call `token_kind` again (documented) |
| Perf regression from marker-based chars run | Stage 2b timing check | one-call reader helper for run extension; if still > 10 %, report and ask |
| Hidden dependence on `cx.source` in third-party-facing docs/tests | Stage 2b/4 grep + doctests | rewrite examples; the door is `cx.tokens` |
| `same_source` conversion misses a node-data site (a bare token `Span` recorded as node data without the check) | reviewer grep for `TextContent::Spanned(`, `NodeKind::comment(`, `command_word` | add the check |
| Semver script noise | Stage gates | expected; capture, do not act |
| `TokenListReader` validation too strict for a legit test (e.g. clipping pre-space produces a token unequal to the listed one) | Stage 1 lockstep failures | compare on span + kind, not on the clipped `pre_space` |
| A test hook relied on scan-time recovery | Stage 1 step 5 | re-express through the std reader's own recoveries or a diagnosing spec; keep the assertion's intent |

---

## 10. Deferred (not part of this plan's execution)

To be handled in a later session, with the user (the architecture/rationale entries
are **not** deferred — they are Stage 5, §7):

1. **Gap-free contract relaxation** for multi-source readers (flush on source
   change / declared may-skip-bytes capability) — needed only with an expanding
   reader. Stage 5 records the current strict contract and this as its "revisit if".
2. **`LatexlikeDriver::with_token_reader(...)`** knob — needed only when a custom
   reader for the latexlike family exists.
3. **`StdStreamPosition` public constructor** — graduate on demonstrated need.
4. The expanding reader itself lives in `techy-xp` (see
   `~/projects/techy-ext/techy-xp/techy_expanding_token_reader_design.md` for its
   original findings; most of its "problems" are answered by this design).
