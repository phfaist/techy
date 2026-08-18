# Span tiling — implementation plan

Design session 2026-08-18/19 (Philippe). This document is the normative spec the
implementer and reviewer agents work from; §1 is checked line by line by reviewers.
State on disk: `dev-docs/spantiling/PROGRESS.md`. Prior projects with the same
protocol: `dev-docs/bettertokens/PLAN.md`, `dev-docs/tokenization/PLAN.md`.

---

## 0. Why: the problem in one page

The token layer was designed for a reader that serves one parse from several sources
— "a reader that substitutes a macro's definition into the stream as it reads is the
motivating future case" ([§dd-dr:token-opacity]; contract clause 3 of `TokenReader`;
the `Tokenization::Token`/`StreamPosition` docs). The tree layer never agreed: the
parse-tree oracle (`node/invariants.rs`, [§dd-dr:span-invariants],
[§dd-dr:input-attachment] "every sibling run stays single-source",
[§dd-dr:slot-roles] "declaration replaces source-change inference") requires that the
children of every `List`/`Group` tile the parent's interior gap-free in one source, and
about ten parser sites turn two stream positions into one `SourceSpan` and abort when
they cannot (`ParseContext::source_span_within` → implementation error). The first
third-party expanding reader hit exactly that abort
(`constructs/nodes_parser.rs:624`, "the token reader broke the in-order, gap-free
token contract").

That abort protects real value — exact verbatim recompose, `NodeSlice::span`/
`source_text`, span→node lookup — which latex2text/FLM/latexpp rely on. It also turned
out that the run check at `:624` is not a tiling check at all: it verifies contract
clause 2 (a peeked token's `StartBeforePreSpace` edge is where the peek happened) plus
the meaning of `move_to` — and the expanding reader in question violated *that*.

**Decision (user, 2026-08-19).** The property becomes a statically declared fact of
the language: `Lang::OBEYS_SPAN_TILING: bool` (default `true`). Languages that obey it
keep today's enforcement and guarantees unchanged. Languages that declare `false` get
parsers that make **no assumption** about where tokens come from: multi-token nodes get
spans the reader *describes* (new required `TokenReader::source_span_describing`),
content is recorded as owned text unless it provably lies in the node's own source, no
tiling holds, and trees still satisfy the all-trees law (`validate_tree`). Techy's own
consumers read node data, never node spans, so they work exactly as documented in both
cases; recompose of a non-tiled tree re-emits the tree as stored (no byte-equality with
any source).

Rejected on the way (record these in Stage 5): a per-reader "I may break the contract"
capability flag; flushing chars runs on source change alone (fixes one of ten sites,
leaves silent tree-law violations); relaxing contract clause 2 at source seams (breaks
the un-consume/stop-token seam parsers rely on); putting the declaration in
`LangFeatures` (that axis gates *storage* — `Store<T>` — not tree guarantees); a marker
type instead of a const (breaking, and nothing needs the type level); a default body for
`source_span_describing` (a missing implementation must be a compile error, never a
misleading span).

---

## 1. Target design (normative — reviewers check diffs against this section)

### 1.1 Vocabulary

- **Span tiling** — the property of a parse tree that: the children of every `List`
  and `Group` node tile the parent's interior (one source, no gaps, no overlaps); a
  `Callable`'s children block is span-contiguous within the node's span (with the
  `Attached`/`Hidden` exclusions of [§dd-dr:slot-roles]); and every positional payload
  (chars content = the node's span; comment start/content/post-space partition the
  node's span; group delimiters are prefix/suffix; …) sits at its pinned position. A
  tree with this property is **span-tiled**. "A node has exactly one span" is *not*
  part of the definition — it holds in both cases ([§dd-dr:mandatory-node-spans]
  unchanged).
- **`OBEYS_SPAN_TILING`** — the `Lang` const declaring whether the language's parse
  trees are span-tiled. It is deterministic information about the language (its
  tokenization and parsers), not a knob; hence "obeys".
- **Tiled language / non-tiled language** — a language with the const `true` / `false`.
  In this plan "relaxed" is shorthand for the `false` case; **rustdoc never coins a
  mode name**: it says "a language with `OBEYS_SPAN_TILING = false`" (docs clarity
  rule: no undefined terms).
- **Seam** — the place in a token stream where the next token comes from a different
  source than the previous one (only under `false`).
- **Describing span** — the `SourceSpan` a reader answers from
  `source_span_describing(begin, end)`: any span the reader considers a useful
  description of the stretch of stream from `begin` to `end`. Techy derives nothing
  from it.
- **The node-data rule** — the existing crate rule (`constructs/mod.rs`
  `node_text_content`): a spelling fact the reader answered as a `SourceSpan` is
  recorded on a node spanning `node_span` as `TextContent::Spanned` when the fact lies
  in the node's own source, as `TextContent::Owned` (the fact's text) otherwise. It
  involves no assumption about tokens: both arms read the very same reader-answered
  span; residency in the node's *source* (not its span) is what the all-trees law
  checks.
- **The all-trees law** — `validate_tree` (`node/invariants.rs`): structural sanity;
  region tiling (index tiling of a `Callable`'s children block by its argument/slot
  regions); `Spanned` residency (every `Spanned` payload is a valid char-boundary range
  of the node's own source). Explicitly *not* byte accounting, not
  children-share-parent's-source, not sibling order, not payload pins. Unchanged by this
  plan; non-tiled parse trees satisfy it.
- **The span-tiling law** — the *test-only* oracle `check_tree_invariants` /
  `check_parse_law_node` (and the preset's `check_latexlike_tree_invariants`): the
  all-trees law plus the byte accounting of span tiling. Currently called "the
  parse-tree law"; renamed (docs) by this plan. It applies to tiled languages only.

### 1.2 `Lang::OBEYS_SPAN_TILING` (`techy/src/state/lang.rs`)

```rust
pub trait Lang: … {
    /// Whether this language's parse trees are **span-tiled** — the property that
    /// the children of every `List` and `Group` node tile the parent's interior
    /// (one source, no gaps, no overlaps), a `Callable`'s children block is
    /// span-contiguous within the node's span, and every positional payload
    /// (chars content, comment start/content/post-space, group delimiters, …)
    /// sits at its pinned position.
    ///
    /// This is a fact about the language's tokenization and parsers, not a
    /// choice: the property holds exactly when the language's token readers
    /// serve each parse in reading order, without gaps, from one source, and a
    /// source changes only where a parser builds a new reader over another
    /// source (`ParseContext::parse_attached_source`).
    ///
    /// `true` (the default): the parsing machinery enforces the property — a
    /// token stream that breaks it is reported as an implementation error — and
    /// every span-based accessor answers exactly (…links…).
    ///
    /// `false`: the language's readers may serve tokens from several sources at
    /// one nesting level (a reader that expands macros as it reads). The parsers
    /// then make no assumption about where tokens come from: a node covering
    /// several tokens is recorded with the span the reader *describes*
    /// ([`TokenReader::source_span_describing`]); its content is recorded as
    /// owned text unless it lies in the node's own source; and no tiling holds —
    /// span-based accessors answer the coordinates the parser recorded, nothing
    /// more. Trees still satisfy the all-trees law ([`validate_tree`]).
    const OBEYS_SPAN_TILING: bool = true;
    …
}
```

Defaulted: no existing `Lang` impl changes; `TrivialLang`, `Latexlike`, every test
language stays tiled unless it says otherwise. Not a `LangFeatures` member, not a
marker type (rejected alternatives, §0).

### 1.3 `TokenReader` (`techy/src/token/reader.rs`)

**New required method** (no default body — a missing implementation is a compile
error):

```rust
/// A source span describing the stretch of stream from `begin` to `end` — what a
/// node covering several tokens is recorded with when the language does not obey
/// span tiling ([`Lang::OBEYS_SPAN_TILING`] `= false`) and the two positions need
/// not delimit one range of one source.
///
/// The answer is the reader's to choose: any `SourceSpan` it considers a useful
/// description of that stretch. The parsing machinery derives nothing from it — no
/// content, no structure, no ordering; it becomes the node's span and shows in
/// diagnostics. Recommended: `begin`'s source, from `begin` to where the stream
/// last stood in that source before reaching `end`. When the two positions do
/// delimit one range of one source, answer that range (what
/// [`source_span_within`](TokenReader::source_span_within) returns). Always
/// answers — the empty span at `begin` ([`SourceSpan::at`] of
/// [`source_position_at`](TokenReader::source_position_at)) is always available.
///
/// A tiled language's parsers never call this method (they use
/// `source_span_within` and treat `None` as an implementation error).
fn source_span_describing(
    &self,
    begin: &StreamPosition<L>,
    end: &StreamPosition<L>,
) -> SourceSpan<L::SourceOrigin>;
```

Implementations to add (every in-crate impl of the trait — grep `impl.*TokenReader<`):
`StdTokenReader` (the exact range; for an inverted pair the empty span at `begin` — a
caller bug, mirroring `source_span_within`'s `None`), `TokenListReader`
(`token/list_reader.rs`, same rule), the test readers `CountingReader`
(`token/tokenization.rs`), `BrokenReader` (`constructs/argument_parsers.rs`),
`FlakyReader` (`constructs/environment_parser.rs`), `StuckRecoveryReader` and
`TabooReader` (`constructs/nodes_parser.rs`), `CommentEmittingReader`
(`techy/tests/lang_features.rs`), and the "Writing a reader over standard tokens"
doc example in `reader.rs` (delegating line).

**Contract additions** (the `# Contract` list of the trait docs; wording may be
polished, content is normative):

- **Clause 7 — moving sets the position.** After `move_to(&tok, edge)`,
  `position_here() == position_at(&tok, edge)`; after `move_to_position(&p)`,
  `position_here() == p`. Corollary with clause 2: for two consecutive tokens,
  `position_at(next, StartBeforePreSpace) == position_at(prev, EndPastPostSpace)` —
  in every reader, seams included. The content loop checks this corollary and reports
  a violation as an implementation error (both tiled and non-tiled languages).
  (`StdTokenReader` already satisfies it — tested at `reader.rs` ~2629; the clause
  writes the tested fact down.)
- **Clause 8 — one source, in reading order, without gaps (tiled languages).** Under
  `OBEYS_SPAN_TILING = true` a reader serves one parse from one source, tokens in
  reading order, every byte between two consecutive tokens' `Start` edges belonging to
  exactly one of them (as pre-space or post-space); a source changes only where a parser
  builds a new reader over another source. This is what makes parse trees span-tiled;
  the machinery enforces it (the implementation errors of `ParseContext::source_span_within`
  and the content loop). Under `false` none of this is promised, and the reader answers
  `source_span_describing` for multi-token constructs.
- **Seams (readers of non-tiled languages).** Clauses 2 and 7 hold at seams too, so
  the first token drawn from a new source carries the *outer trigger position* as its
  `StartBeforePreSpace` edge (un-consuming it — `move_to(&tok, StartBeforePreSpace)` —
  returns to the trigger, and the next `peek` reproduces the expansion, clause 1), and
  the position past the last token of an exhausted source is the *resume position* in
  the outer source. One position value names such a shared place; the reader chooses
  its value (an outer position, an inner one, or a composite) and what
  `source_position_at` reports for it (recommended: the outer/resume coordinate). A
  token may thus have edges in two sources — its sub-spans are answered one at a time
  by `source_span_between`, each a span of one source. Because seam positions compare
  equal, a chars run may legitimately extend across a seam; that is why non-tiled
  languages record such content as owned text.
- **Further sentences for non-tiled readers:** termination is the reader's
  responsibility (an expansion that never ends is an endless token stream; the engine's
  descent guard tracks parser nesting only); positions and tokens stay valid inside
  sources the stream has already left (clause 3 — parsers rewind across seams:
  argument probing, un-consumed stop tokens); mint expansion sources with
  `SourceProvenance::Synthesized { by, triggered_at }` so diagnostics inside an
  expansion carry the provenance chain (no `Frame` is pushed for an expansion);
  `EndOfStream` is the end of the *whole* input — an exhausted expansion is not
  `EndOfStream`, the reader continues in the outer source; the final-whitespace-as-
  `EndOfStream`-pre-space rule applies to the whole input's end only.
- **Reword** the existing multi-source sentences (`reader.rs` "Locations leave a
  reader in exactly one form … several sources"; clause 3's "a reader serving several
  sources"; `tokenization.rs` `Token`/`StreamPosition` docs "serve tokens from more
  than one source during one parse — a macro expander, say") so each points at
  `OBEYS_SPAN_TILING = false` as the condition under which a reader may do that at one
  nesting level (a tiled language's readers serve one source per parse; several
  sources arise only through `parse_attached_source`).

### 1.4 `ParseContext` (`techy/src/constructs/mod.rs`)

`source_span_within(begin, end)` (pub) and `invocation_span_within` (private, the
`stage_invocation` span) dispatch on `L::OBEYS_SPAN_TILING`:

- `true`: as today — `tokens.source_span_within` and `None` → the implementation error
  (unchanged wording).
- `false`: `Ok(tokens.source_span_describing(begin, end))` — never an error (an
  inverted same-source pair is not an error either: no assumptions).

Doc of `source_span_within`: state both arms; under `false` the returned span is what
the reader described (a coordinate hint recorded as the node's span), and callers must
not derive content or structure from it. No new method; construct parsers written
outside the crate get the behavior through the same helper.

Grep gate: no site outside these two helpers calls `cx.tokens.source_span_within`
directly to compute a node/body/name span (the inventory in §1.5 lists the callers;
they all go through `cx.source_span_within`).

### 1.5 Parser rules under `OBEYS_SPAN_TILING = false`

Nothing changes for tiled languages (bit-identical trees; the existing tests are the
regression net). Rules R1–R7 apply where `!L::OBEYS_SPAN_TILING` (branch on the const;
the compiler drops the untaken arm):

- **R1 — Chars runs** (`constructs/nodes_parser.rs`: the `run` field, `take_pre_space`,
  `extend_run`, `extend_run_to`, `flush`, `flush_through`, the end-of-stream
  whitespace-only run — invariant 4). The position check in `extend_run_to` **stays in
  both cases** — it verifies clause 7's corollary — but its message and the doc comments
  above it stop saying "in-order, gap-free": the message names the actual violation
  ("the token's `StartBeforePreSpace` edge {start:?} is not the position the stream stood
  at when the token was peeked ({run_end:?}) — the token reader violates the `TokenReader`
  contract (a peeked token starts where the peek happened; moving to an edge sets the
  position)"). Under `false` the run additionally accumulates **owned text**: for each
  extension, the text of the extended stretch as the reader answers it for that token —
  pre-space = `source_span_between(tok, StartBeforePreSpace, Start).content()`; a
  `Char` token = its pre-space text + the `TokenKind::Char(c)` spelling +
  `source_span_between(tok, End, EndPastPostSpace).content()` (normally empty). At
  flush: span = `cx.source_span_within(&start, &end)?` (dispatches to the describing
  span), kind = `NodeKind::chars(TextContent::Owned(text))`. Under `true` the run stays
  exactly as today (`Spanned` over the exact run slice). Representation suggestion:
  `run: Option<PendingRun<L>>` with `{ start, end, text: Option<String> }` (`text` is
  `Some` only under `false`); keep it simple.
- **R2 — single-token facts recorded as node data** go through the node-data rule
  (`node_text_content(fact, node_span)`) in both cases. Already the case at group
  delimiters (`group_parser.rs`), verbatim delimiters (`verbatim_parser.rs`),
  embellishment markers (`embellishments_parser.rs`), environment end syntax
  (`latexlike/invocation_syntax.rs` ~404). **Switch** the comment sub-spans
  (`comment_node_kind`, `constructs/mod.rs` ~140: currently bare `.span()` with the
  argument "the three sub-spans tile the token" — under `false` a seam token may have
  edges in two sources, so the argument fails) to `node_text_content(&sub, &span)`.
  Sites where the node span *is* the fact's own span (pre-space-only chars nodes,
  recovery fallback chars over one token, the default paragraph-break node, noise
  chars in `argument_parsers.rs`, `attached_source.rs` placeholder) keep the bare
  `Spanned` (trivially resident) — list them in PROGRESS.md as checked.
- **R3 — payloads built before the node span is known.**
  `FromInvocation::from_invocation` for the macro arm
  (`latexlike/invocation_syntax.rs` ~129–141) records `post_space` as
  `TextContent::Owned` (the reader's span rendered) under `false`, `Spanned` under
  `true`; rewrite its comment (the "sound because the node starts at this very token"
  argument holds under tiling only). Grep for any other pre-staging payload builder
  (`FromInvocation` impls, `*_form` constructors) and apply the same rule.
- **R4 — multi-token content other than chars runs**: verbatim content
  (`verbatim_parser.rs` ~408–412, ~736, ~760–764) and the environment verbatim body
  (`environment_parser.rs` ~1189–1193): under `false` accumulate owned text with the R1
  recipe (what the tokens said, token by token) and record `Owned`; span via
  `cx.source_span_within` (dispatch). Under `true` unchanged.
- **R5 — multi-token node spans** (`group_parser.rs` ~340; `environment_parser.rs`
  ~340/603/704/1074/1189/1223; `argument_parsers.rs` ~997; `embellishments_parser.rs`
  ~300; `nodes_parser.rs` ~653/703; the `stage_invocation` span): via the dispatch —
  no code change beyond R1/R4; verify by grep (§1.4 gate).
- **R6 — test oracles.** `check_tree_invariants` (`node/invariants.rs`): under
  `!L::OBEYS_SPAN_TILING` run `validate_tree` only (the byte accounting does not
  apply) and say so in its docs; rename its docs' "parse-tree law" to "span-tiling law".
  `check_latexlike_tree_invariants` (`latexlike/invariants.rs`, the payload pins):
  same gate.
- **R7 — the preset stays generic.** Every preset site reads `L::`/
  `LLL::OBEYS_SPAN_TILING` of the language it is instantiated for
  (`LatexlikeDriver<LLL>` reuses these parsers for downstream languages). Grep
  `impl … for Latexlike` (concrete) blocks that build node data or spans — expected
  none; if any, make them generic or consult the const through the concrete type.
  Stage 3b proves it with a `LatexlikeLang` test language declaring `false`.

Recovery paths (`TokenRecovery` placeholders, `recover_as_chars`, dangling-escape
placeholder) need no change: placeholder tokens are reader-issued and follow clauses
2/7 like any token.

### 1.6 Consumers

Rule (goes to ARCHITECTURE in Stage 5): *techy's consumers obtain content from node
data — `TextContent` (resolved against the node's own source), names, delimiters,
payloads — never from node spans; node spans are provenance coordinates. The
coordinate accessors (`NodeSlice::span`/`source_text`, `NodeRef::span_content`,
`SourceSpan::content`) say what they are and answer exactly what the coordinates
say.* Consequences, verified in Stage 4:

- Recompose already conforms (`recompose/mod.rs` ~481–486 `content.resolve(source)`;
  `latexlike/recompose.rs` ~159–182 payload `TextContent`s and recorded spellings). Doc
  line: for a language with `OBEYS_SPAN_TILING = false` the source recomposer re-emits
  the tree *as stored* (owned text where the parser recorded it) — no byte-equality
  with any source is claimed.
- `NodeSlice::span`/`source_text` (`node/slice.rs`), `NodeRef::span_content`
  (`node/node_ref.rs`), the covering-run lookup docs (`node/tree.rs` ~557): "exact —
  sibling runs of a parsed tree are span-contiguous" becomes "exact for a tiled
  language (the span-tiling law); for a language with `OBEYS_SPAN_TILING = false` the
  answer is the covering span of the coordinates the parser recorded (holes included),
  `None` across sources as before".
- `extract` (`extract.rs`): audit that every documented answer holds for `Owned` chars
  content (e.g. `extract.rs` ~207/~519 build piece spans from `Spanned` content —
  check what the docs promise for pieces of `Owned` content and that the code matches;
  fix code or doc, never "best effort").
- `serialize`, `transform`, `visit`: no change expected (Owned round-trips; restage
  already treats trees as untiled; the walk is structural). Confirm by reading their
  module docs for tiling claims.
- Guide chapters (`docs/*.md`, module-level `*.md` under `techy/src`): grep
  "partition invariant", "gap-free", "tile", "tiling", "single-source" and update
  wording to the new vocabulary and the conditional.

### 1.7 Test infrastructure — the scripted multi-source reader (Stage 3a)

`techy/src/token/scripted_reader.rs`, `#[cfg(test)]`, `pub(crate)`, exported through
`token/mod.rs` under `cfg(test)` (like `TokenListReader`). Purpose: exercise every
non-tiled path without implementing an expander.

- **`ScriptedTokenization`** — a ZST implementing `Tokenization<L>` for
  `L: Lang<Tokenization = ScriptedTokenization>` with its **own** token and position
  types (the first in-crate exercise of a non-standard `StreamPosition`):
  `ScriptedToken` (Clone/Debug/PartialEq/Send/Sync — an entry index plus whatever the
  reader needs to interpret it; storing the segment's `StdToken<L>` and source index
  inside is fine) and `ScriptedPosition` (Clone/Debug/PartialEq/Eq/Send/Sync — an
  `(entry index, TokenEdge)` pair in **canonical form**: `(i, EndPastPostSpace) ≡
  (i+1, StartBeforePreSpace)` canonicalizes to the latter, and within one entry the
  edges that coincide in offset canonicalize to the earliest — so `==` answers "same
  place" and clauses 2/7 hold at seams by construction).
  `make_token_reader` is not how tests build it (a script is runtime data): tests
  build the reader directly and drive parsers through `ParseContext`/`ParserSession`
  the way the two-reader agreement suites do, or through a `ParseDriver`
  `make_token_reader` override that captures the script — implementer's choice, keep
  it small.
- **`ScriptedReader<'s, L>`** — built from **segments**: `&[(&'s Arc<Source<…>>,
  Range<usize>)]` tokenized with `StdTokenReader` under a given `ParsingState` (so
  tokens are realistic and the crate's tokenizer is reused; one inner
  `StdTokenReader<'s>` per source is kept for `token_kind` interpretation — the
  "reader over standard tokens" recipe). Concatenation: middle segments' `EndOfStream`
  tokens are dropped (a middle segment with trailing whitespace is a scripting error —
  assert), the last segment's `EndOfStream` (with its pre-space) is the final entry.
  Segments may repeat a source (`A[0..1], B[0..3], A[5..7]` scripts a splice with a
  hole; `A, B` a chain; `A[0..1], A[5..6]` a hole in one source).
- **Answers:** `peek` = the entry at the cursor (a fixed script — the same fidelity
  gap as `TokenListReader`, documented); `move_to`/`move_to_position` set the cursor
  (positions valid anywhere, backward included); `position_at`/`position_here` in
  canonical form; `source_span_between` = the entry's source and its edge offsets;
  `source_position_at` = the entry's coordinate (for a seam position, the *outer/
  resume* coordinate — i.e. the coordinate of entry `i` at `StartBeforePreSpace`);
  `source_span_within(begin, end)` = `Some` iff `begin` precedes-or-equals `end` in
  sequence order and every entry from `begin`'s to `end`'s lies in one source with
  offset-contiguous edges (`EndPastPostSpace` of one == `StartBeforePreSpace` of the
  next) — then the exact range — else `None`; `source_span_describing(begin, end)` =
  the recommended shape (`begin`'s source, from `begin`'s offset to the last offset in
  that source among the entries up to `end`; the empty span at `begin` if `end`
  precedes `begin`).
- **A deliberately broken variant** (a constructor flag or a wrapper) that reports a
  *non-canonical* seam position — used to test that the content loop reports the
  clause-7 violation as an implementation error in both tiled and non-tiled languages.
- Unit tests of the reader itself: canonical-position equalities at seams, `within`
  vs `describing` on chain/splice/hole scripts, rewinding into an exhausted segment.

Test languages (Stage 3a/3b): `RelaxedLang` (core: `Tokenization =
ScriptedTokenization`, `OBEYS_SPAN_TILING = false`), `RelaxedStdLang` (core:
`StdTokenization`, `false` — same-source relaxed parsing), and a `LatexlikeLang`
implementation with `false` over `ScriptedTokenization` (Stage 3b; check what
`LatexlikeLang` requires in `latexlike/lang.rs`).

### 1.8 Naming register (reviewers enforce; [§dd-arch:naming])

| Item | Name | Notes |
|---|---|---|
| The property | **span tiling** / **span-tiled** | not "single-source" (a tiled tree may be multi-source through `Attached` regions), not "list" (covers `Group` and `Callable` children too, and `List` is a `NodeKind`) |
| The declaration | `Lang::OBEYS_SPAN_TILING: bool` | "obeys": a fact, not a choice; associated const, default `true` |
| The reader answer | `TokenReader::source_span_describing(begin, end) -> SourceSpan` | required; the twin of `source_span_within` |
| The test-only oracle | the **span-tiling law** (`check_tree_invariants`) | replaces "parse-tree law" in docs; "the all-trees law" (`validate_tree`) unchanged |
| The nodes-parser doc heading | keep "Whitespace and span invariants"; its closing "partition invariant" sentence becomes "for a tiled language these give span tiling: …" | |
| Prose for the `false` case | "a language with `OBEYS_SPAN_TILING = false`" / "a language that does not obey span tiling" | no coined mode name in rustdoc; "relaxed" is plan shorthand only |
| Test reader | `ScriptedReader`, `ScriptedTokenization`, `ScriptedToken`, `ScriptedPosition` | `cfg(test)`, `pub(crate)` |
| Superseded phrases | "in-order, gap-free token contract" (as the name of the run check), "partition invariant" (as a name), "parse-tree law" | rewrite where met |

### 1.9 Small decisions with defaults (do not ask; record in PROGRESS.md if deviated)

- D1 — Single-token facts keep the node-data rule (Spanned iff same source as the node
  span) in both cases rather than unconditional `Owned` under `false`: no assumption is
  involved (both arms read the same reader-answered span), it keeps zero-copy, and it is
  the rule already in place. Multi-token content (R1, R4) and pre-staging payloads (R3)
  are `Owned` under `false`. (The user accepted "own everything if needed"; if they rule
  "Owned everywhere under `false`", replace the `node_text_content` calls under
  `!OBEYS_SPAN_TILING` — one helper.)
- D2 — `StdTokenReader::source_span_describing` for an inverted pair: the empty span at
  `begin`.
- D3 — Under `false`, an inverted same-source pair given to `ParseContext::source_span_within`
  is not an error (the describing span is recorded).
- D4 — The `PendingRun` owned-text accumulation reads pre-space/post-space text through
  the reader's `source_span_between(...).content()` (span rendering of a reader answer
  about *that* token — permitted by [§dd-dr:token-contract-hardening] item 5's doctrine).
- D5 — Guide/rustdoc wording: define "span tiling" once where introduced (the `Lang`
  const doc is the canonical definition; other sites link to it).

### 1.10 Rulings (user, 2026-08-18/19)

1. The property is declared statically on `Lang` as `OBEYS_SPAN_TILING` (const,
   default `true`); techy enforces it for languages that obey; otherwise no assumptions
   about tokens (sources, order, gaps).
2. Under `false`, node building accepts tokens from any sources; multi-token spans are
   what the reader describes (`source_span_describing`, required, no default; any fuzzy
   span, techy assumes nothing); content is owned where not provably in the node's
   source — "even if this means owning all the parsed strings".
3. Consumers: techy-provided consumers work precisely as documented regardless of the
   const; content from node data, never node spans; recompose re-emits the tree as
   stored (no byte-equality under `false`).
4. Contract clauses (moving sets the position; seams; termination; rewinds; provenance;
   `EndOfStream` = whole input) are documented on `TokenReader`.
5. Not in `LangFeatures` (that axis gates storage).
6. Execution: Opus implementer + reviewer agents per stage, worktrees, ff-merges to
   `main`, commit regularly.

---

## 2. Stage 1 — contract surface (`st-1-contract` ← `main`)

Files: `state/lang.rs`, `token/reader.rs`, `token/list_reader.rs`,
`token/tokenization.rs`, `constructs/mod.rs`, the test readers listed in §1.3,
`techy/tests/lang_features.rs`.

Steps:
1. Add `Lang::OBEYS_SPAN_TILING` (§1.2) with its doc; link targets must resolve
   (`cargo docs`).
2. Add `TokenReader::source_span_describing` (§1.3) and implement it in every in-crate
   impl (§1.3 list) plus the doc example. `StdTokenReader`: exact range / D2.
3. Write the contract additions (clause 7, clause 8, the seams paragraph, the further
   sentences, the rewordings) — §1.3.
4. `ParseContext::source_span_within` and `invocation_span_within` dispatch (§1.4) with
   docs.
5. Tests: `Lang` default is `true` (a compile-time assert in a test); a test `Lang` with
   `false` compiles and parses a trivial input over `StdTokenReader` (structure only —
   the parser rules come in Stage 2, so assert nothing about content representation
   yet, or gate the assertion behind Stage 2 — implementer's call, note it);
   `StdTokenReader::source_span_describing` on ordered/equal/inverted pairs;
   `TokenListReader` likewise; clause 7 as a unit test on `StdTokenReader` (already
   present ~2629 — extend to `move_to_position`).
6. Do not touch `nodes_parser.rs` beyond compiling (Stage 2 owns it), nor dev-docs.

Gates: `cargo build`, `cargo test --workspace`, `cargo test --workspace --all-features`,
`cargo clippy --workspace --all-targets -- -D warnings`, the same with
`--all-features`, `cargo docs --all-features` (no broken intra-doc links; the alias is
`doc --workspace --no-deps`).

Reviewer checklist: (a) const name/doc/default per §1.2, definition complete
(interior tiling + callable contiguity + payload pins), no mode name coined; (b) method
signature/doc per §1.3, required (no default), every impl present (grep count matches
§1.3 list + doc example); (c) contract clauses present and accurate — clause 7,
clause 8 conditional on the const, seams paragraph with the trigger/resume rule,
termination, rewinds, provenance, `EndOfStream`; multi-source sentences reworded;
(d) `ParseContext` dispatch both helpers, docs; (e) tests of step 5; (f) no lib
panics added; (g) naming register §1.8; (h) gates verbatim.

---

## 3. Stage 2 — parsers under `OBEYS_SPAN_TILING = false` (`st-2-parsers` ← `st-1-contract`)

Files: `constructs/nodes_parser.rs`, `constructs/mod.rs` (`comment_node_kind`),
`constructs/verbatim_parser.rs`, `constructs/environment_parser.rs`,
`latexlike/invocation_syntax.rs`, `node/invariants.rs`, `latexlike/invariants.rs`,
plus whatever the R3/R7 greps find.

Steps:
1. R1 (chars runs): message + doc rewrite of the position check (both cases); the
   owned-text accumulation under `false`; the nodes-parser module docs' "partition
   invariant" sentence → span tiling wording (§1.8).
2. R2: `comment_node_kind` through `node_text_content`; list the "node span == fact
   span" sites as checked in PROGRESS.md.
3. R3: `from_invocation` macro arm; grep for other pre-staging payload builders.
4. R4: verbatim content and environment verbatim body.
5. R5: grep gate of §1.4 (no direct `cx.tokens.source_span_within` for node spans).
6. R6: oracle gating + doc rename ("span-tiling law").
7. R7: grep concrete `Latexlike` impls.
8. Tests (no multi-source reader yet — Stage 3): `RelaxedStdLang` (§1.7, `false`
   over `StdTokenization`): parse a representative input (chars, group, comment,
   paragraph break, a macro with arguments, an environment via the preset test lang if
   cheap) under a tiled and the relaxed language → identical structure (kinds, child
   counts, spans), Chars/verbatim content `Owned` under `false` and equal as text,
   macro `post_space` `Owned`; `validate_tree` passes; `check_tree_invariants` runs
   `validate_tree` only under `false`. Clause-7 violation message test (a broken reader
   giving an inconsistent `StartBeforePreSpace` — reuse `BrokenReader`-style test
   readers) → implementation error under both `true` and `false`.
9. Existing suites unchanged and green (the tiled path is bit-identical).

Gates: as Stage 1.

Reviewer checklist: (a) R1 — check kept in both cases, message/docs no longer say
"gap-free" as the check's name, owned accumulation only under `false`, tiled path
unchanged (diff of the tiled arms is wording only); (b) R2 comment sub-spans; the
"checked" list in PROGRESS.md is complete against `grep -n "NodeKind::chars(\|TextContent::Spanned("`
in `constructs/` and `latexlike/`; (c) R3 site + comment rewritten; (d) R4 both
sites; (e) R5 grep clean; (f) R6 gates + doc rename; (g) R7 grep result recorded;
(h) tests of step 8; (i) no lib panics; (j) naming; (k) gates verbatim.

---

## 4. Stage 3a — scripted multi-source reader (`st-3a-scripted` ← `st-1-contract`, ∥ Stage 2)

Files: new `token/scripted_reader.rs`, `token/mod.rs` (cfg(test) export).

Steps: implement §1.7 (types, reader, broken variant, unit tests). No parser tests
here (Stage 3b). Keep it as small as the requirements allow; reuse `StdTokenReader`
for tokenizing segments and interpreting kinds.

Gates: as Stage 1.

Reviewer checklist: (a) own `Token`/`StreamPosition` types through a `Tokenization`
impl; (b) canonical positions — clauses 2 and 7 hold at seams by construction (unit
tests prove the equalities); (c) `within` vs `describing` semantics per §1.7 on
chain/splice/hole scripts; (d) rewinding into an exhausted segment works; (e) the
broken variant exists; (f) middle-segment trailing-whitespace assert; (g) `cfg(test)`,
`pub(crate)`, no public surface; (h) gates.

---

## 5. Stage 3b — non-tiled parse tests (`st-3b-tests` ← `main` after Stages 2 and 3a merged)

Files: test modules under `constructs/` and `latexlike/` (or a new
`techy/tests/span_tiling.rs` if the scripted reader can be reached from there — it is
`cfg(test)` in-crate, so in-crate test modules are the expected place), `latexlike`
test language.

Tests (each asserts structure, content representation, spans as recorded,
`validate_tree` OK, and — where meaningful — recompose output "as stored"):
- T1 chars run across a seam: script `A[0..1]="a"`, `B[0..3]="xyz"`, `A[5..7]=" b"`
  under `RelaxedLang` → one `Chars` node, `Owned("axyz b")`, span = the describing
  span (`A[0..7]` with the recommended shape); the same script under a *tiled* language
  over the same reader → implementation error (clause 8's enforcement:
  `source_span_within` is `None`) — proves enforcement.
- T2 group spanning a seam: `A="{a"`, `B="b}"` → `Group`, span describing, `open`
  `Spanned` (in the node's source), `close` `Owned`, child `Chars` `Owned("ab")`.
- T3 hole in one source: `A[0..1]="a"`, `A[5..6]="b"` (as if `\foo` were consumed) →
  one `Chars` `Owned("ab")`, describing span `A[0..6]`.
- T4 preset: `\begin{env}` in A, body in B, `\end{env}` in A, under the relaxed
  `LatexlikeLang` → environment node built; body slot content `Owned`; terminator
  payload per the node-data rule; recompose emits `\begin{env}…\end{env}` with the
  body as stored.
- T5 un-consumed stop token at a seam: `NodesParser` with a token stop condition
  matching the first token of B, `consume = false` → returns; re-peeking yields that
  token with empty pre-space at the same position (clauses 2/7 through the seam).
- T6 backtracking across a seam: an optional-argument probe (`\cmd` in A, `[x]` in B, or
  a probe that fails and rewinds into A) parses correctly.
- T7 comment token and paragraph-break token inside B → nodes built; payloads per rule.
- T8 macro payload: `\foo` in A with post-space → `post_space` `Owned` under `false`.
- T9 clause-7 violation via the broken scripted variant → implementation error under
  both `true` and `false`.
- T10 `check_tree_invariants` on every relaxed tree above (gated: `validate_tree` only).
- T11 diagnostics inside B rendered with the provenance chain when B is minted
  `Synthesized { by, triggered_at }` (a smoke test on the rendered report).
- T12 recompose of T1's tree = `"axyz b"`.

Gates: as Stage 1.

Reviewer checklist: every test T1–T12 present and asserting what §5 says; the tiled
enforcement counter-test (T1 second half, T9) present; no test weakened to pass; gates.

---

## 6. Stage 4 — consumers: docs and audit (`st-4-consumers` ← `st-1-contract`, ∥ Stage 2; rebase before merge)

Files: `node/slice.rs`, `node/node_ref.rs`, `node/tree.rs` (docs), `extract.rs`,
`recompose/mod.rs`, `latexlike/recompose.rs` (doc lines), `serialize`/`transform`/
`visit` module docs (read only unless a tiling claim is found), guide chapters
(`docs/*.md`) and module-level markdown.

Steps: §1.6 in full. Extract audit: read the documented contract of every public
`extract` item, run its doctests, and check the code paths that pattern-match
`TextContent::Spanned` (~207, ~519) for `Owned` input — the documented answer must
hold; add a unit test with an `Owned` chars node (transform-created, e.g. via
`NodeKind::chars(TextContent::Owned(..))` through the builder) per affected item.
Wording sweep for the superseded phrases (§1.8) in the files of this stage.

Gates: as Stage 1 (docs gate is the important one).

Reviewer checklist: (a) each accessor doc states exact-under-tiling / recorded
coordinates otherwise; (b) extract audit recorded per item with the doctest/unit
evidence; (c) recompose doc line; (d) no consumer reads content from node spans
(grep `span_content()`/`source_text()`/`.span().content()` outside tests → only the
coordinate accessors themselves); (e) superseded phrases gone in the touched files;
(f) gates.

---

## 7. Stage 5 — record (`st-5-record` ← `main` after everything else merged)

Files: `dev-docs/DESIGN_RATIONALE.md`, `dev-docs/ARCHITECTURE.md`, `PROGRESS.md`,
`CLAUDE.md` only if the user asks.

- New DR entry **`[§dd-dr:span-tiling]`** — "Span tiling is a declared property of the
  language; parsers assume nothing otherwise". Status: DECIDED (user, 2026-08-19).
  Content: the §0 story in DR register style (decision, definition, the const, the
  reader method and its no-assumptions contract, the seam analysis — clause 7 corollary,
  the run check is a clause-2/7 check, runs cross seams, hence owned content — the
  node-data rule, the consumers rule, `validate_tree` unchanged, the scripted reader as
  the enforcement/test tool, cost accepted: owned strings under `false`, no zero-copy
  for multi-token content). Rejected alternatives: the §0 list. Revisit if: a reader
  needs a per-instance declaration (driver-level), or zero-copy content under `false`
  is demonstrated to matter (verify-then-intern).
- Amend: [§dd-dr:span-invariants] (the invariants hold for tiled languages; pointer),
  [§dd-dr:input-attachment] ("every sibling run stays single-source" — for tiled
  languages), [§dd-dr:token-opacity] (the motivating expander case is now supported
  under `OBEYS_SPAN_TILING = false`), [§dd-dr:stream-position] (positions at seams;
  clause 7), [§dd-dr:token-contract-hardening] (cross-reference clause 7/8 numbering if
  the entry lists clauses). Follow the DR maintenance rules (labels immutable, every
  entry referenced from ARCHITECTURE — add the reference).
- ARCHITECTURE: a subsection on span tiling in the tree/token topic (the definition by
  link to the const doc, the two regimes, the consumers rule of §1.6, the reader
  contract pointer), naming register additions (§1.8), reference to
  `[§dd-dr:span-tiling]`; wording sweep for superseded phrases in ARCHITECTURE.
- Follow the doc split rule (structure in ARCHITECTURE, concise rationale incl.
  accepted costs in DESIGN_RATIONALE) and the docs clarity rules (define terms, no
  metaphors).

Gates: `cargo docs --all-features` (nothing else changes), plus a grep that every new
`[§dd-dr:…]` label is referenced from ARCHITECTURE.

Reviewer checklist: entry template followed; every §1.10 ruling and every §0 rejected
alternative recorded; amendments present; ARCHITECTURE reference present; naming
register updated; no superseded phrase reintroduced.

---

## 8. Execution protocol (orchestrator)

**Roles.** The orchestrator never edits source itself; per stage it spawns one
*implementer* agent and one *reviewer* agent (fresh context), reads compact reports,
resolves flagged points (asks the user when a point is a design decision), merges.
**Every subagent runs with `model: "opus"`** (user directive).

**Worktrees and branches.** Worktrees live under
`/Users/philippe/projects/techy/.claude/worktrees/<branch>` (gitignored, inside the
sandbox-writable tree); never edit the primary checkout. Branches:
`st-1-contract` ← `main`; `st-2-parsers` ← `st-1-contract`; `st-3a-scripted` ←
`st-1-contract`; `st-4-consumers` ← `st-1-contract`; `st-3b-tests` ← `main` (after
2 and 3a merged); `st-5-record` ← `main` (after all merged). Waves: A = {1};
B = {2, 3a, 4}; C = {3b}; D = {5}. Reviewer of stage N reviews
`git diff <base>..<branch>`.

**Merging** (user's standing procedure, no PRs): after review passes, rebase the branch
onto current `main`, run `cargo test --workspace`, confirm the primary checkout is clean
(`git -C /Users/philippe/projects/techy status --porcelain` empty — if not, wait/ask),
then `git -C /Users/philippe/projects/techy merge --ff-only <branch>` **with the
sandbox bypassed**. Never merge while the checkout is dirty. Do not push unless the
user says so. Remove the worktree after merge (`git worktree remove`), keep the branch
until the project ends.

**Agent prompts.** Implementer: "You are implementing Stage N of
`dev-docs/spantiling/PLAN.md` (read §0, §1 and §N in full; also PROGRESS.md). Work only
in worktree `<path>` on branch `<name>` (already created; run cargo there). Do not touch
dev-docs/ARCHITECTURE.md or DESIGN_RATIONALE.md (Stage 5 does). Follow CLAUDE.md
(naming, panic policy — Result not panic, tests for new behavior, US English, docs
clarity: define terms, no metaphors). Run the stage's gates before reporting. Commit in
small logical commits with the configured trailers. Update PROGRESS.md (your stage's
section) and commit it. Report: (1) what changed per file, (2) gate results verbatim
(test counts, clippy, docs), (3) any deviation from §1 with reason, (4) any open
question — do not decide design questions yourself. Never end without a final report."
Reviewer: "You are reviewing Stage N of `dev-docs/spantiling/PLAN.md` in worktree
`<path>`, branch `<name>`, base `<base>`. Read §1 and §N; run every gate yourself;
read the full diff; check the reviewer checklist item by item; check naming against
§1.8 and dev-docs/ARCHITECTURE.md [§dd-arch:naming]; check the panic policy; run the
greps the plan names. Report PASS/FAIL per item with file:line evidence and a list of
required fixes. Do not fix things yourself."

**Fix loop.** Reviewer FAIL → send the required-fixes list to the implementer
(SendMessage, same agent) → re-review the delta. Two failed rounds on one point →
escalate to the user.

**State on disk.** `PROGRESS.md`: per stage — branch, worktree, status
(started/implemented/reviewed/merged), gate results, decisions under §1.9, open
questions and answers. A fresh session resumes from PLAN.md + PROGRESS.md + `git log`.

**Never end a turn without live children or a final report.**

---

## 9. Risks and fallbacks

| Risk | Detect | Fallback |
|---|---|---|
| A parser site computes a node span from two positions bypassing `ParseContext` (direct `cx.tokens.source_span_within`) | Stage 2 grep (§1.4 gate) | route through the helper |
| Owned-text accumulation changes tiled behavior by accident | Stage 2 existing suites; reviewer diff of tiled arms | keep the tiled arms textually as before |
| `token_kind` view lifetime makes the scripted reader awkward (view must not borrow the reader) | Stage 3a | store the segment `StdToken` inside `ScriptedToken` and delegate `token_kind` to the per-source inner reader (view borrows the token) |
| Canonical seam positions clash with a parser that compares `position_at(tok, Start)` against a stop-token position | Stage 3b T5/T6 | it should not (canonicalization is only for coinciding places); report if it does |
| `LatexlikeLang` requires more than expected to instantiate over a custom tokenization | Stage 3b | use the preset's `LatexlikeDriver<LLL>` generalization; if blocked, report — do not weaken R7's proof silently |
| Extract's documented answers do not hold for `Owned` content | Stage 4 audit | fix code (preferred) or docs; never "best effort" |
| Docs gate: broken intra-doc links from the many new cross-references | every stage | fix links, do not drop them |

---

## 10. Deferred (not part of this plan)

1. A per-driver-instance declaration (a driver that swaps in a non-tiled reader for a
   tiled language) — the const is per language; a driver override must install a
   reader consistent with it (violations are still caught).
2. Zero-copy multi-token content under `OBEYS_SPAN_TILING = false` (verify-then-intern).
3. The expanding reader itself (lives outside techy — `techy-xp`); this plan gives it the
   contract and the test tool.
4. `LatexlikeDriver::with_token_reader` knob (from `dev-docs/bettertokens/PLAN.md` §10).
