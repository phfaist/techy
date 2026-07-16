# Action 02 — Token model questions

**Status: open — five independent decisions on the token layer's API/contracts.**

No live correctness bug in the shipped code; each item below is a contract gap or design
question that becomes expensive after third-party `TokenReader`/`Lang` implementations
exist.

## 1. `TokenKind::Comment` should carry a content (or start-delimiter) span

`TokenKind::Comment` (`src/token/token.rs`) carries `content: &'s str` but no span for
it. Both consumers reconstruct the three comment sub-spans by byte arithmetic —
`nodes_parser.rs` (`dispatch` comment arm, ~line 593) and `argument_parsers.rs`
(`scan_argument_noise`, ~line 124) contain the byte-identical block:

```rust
let content_span = Span::new(post_space.start - content.len(), post_space.start);
let start_span = Span::new(token.span.start, content_span.start);
let kind = NodeKind::comment(start_span, content_span, *post_space);
```

Problems:

- The arithmetic works *only* because `StdTokenReader::read_comment` slices content
  verbatim from the source, so `content.len()` equals its byte extent. `TokenReader` is
  a **public extension point**: a third-party reader that normalizes comment content
  (strips `\r`, decodes, synthesizes) makes `post_space.start - content.len()` underflow
  — a debug panic, or a wrapped `usize` in release that trips later span machinery.
  That is a lib-code panic reachable from a legitimate impl of a public trait.
- No consumer uses `content` *as a string* — both sites use only `content.len()`. The
  `&'s str` is currently a length carrier.
- Two copies of fragile logic drift independently.

**Recommended fix:** add `start: Span` (the matched start-delimiter span) to
`TokenKind::Comment`, mirroring `NodeKind::Comment { start, content, post_space }`
(which has `start` for the same "which delimiter fired is per-instance" reason). The
reader already computes it (`content_start = pos + start.len()`). Then
`content_span = Span::new(start.end, post_space.start)` — no subtraction, no dependence
on `content.len()`, and both duplicated blocks collapse into one shared
`NodeKind::comment(...)` construction (a `Token::comment_spans()` helper or a
`NodeKind::comment_from_token(&token)` constructor). A helper alone (without the token
field) would DRY the duplication but not close the third-party-reader hole.

## 2. The dangling-escape recovery drops a source byte from the tree

`StdTokenReader`'s escape-at-EOF recovery (`read_command`'s EOF branch) builds:

```rust
let placeholder = Token::new(TokenKind::EndOfStream, Span::empty(pos), pre_space);
… TokenRecovery { token: placeholder, resume_pos: s.len() }
```

The placeholder sits at `pos` (the escape's position) while reading resumes at
`s.len()`, so the escape byte `pos..s.len()` is claimed by no token and no node.
Evidence: tolerant parse of `"ab \\"` yields one node `chars 0..3 "ab "` with the reader
at 4 — the root children do not tile the content, and the corresponding test is the one
tokenizer-recovery test that does *not* assert the partition invariant.

This is inconsistent with the project's own recovery principle (DESIGN_RATIONALE §3.8:
"markup text inside a `Chars` node is an accepted tolerant-recovery artifact, always
accompanied by a diagnostic") — the unresolvable-command recovery follows it by staging
a chars node over the whole token span. The `ForbiddenChar` recovery is also
content-preserving (a `Char(c)` placeholder with the real span).

§3.2 decided `EndOfStream` here, but the recorded rationale is only that the Phase-2
*empty-`Chars`* sentinel became impossible when `Chars` became `Char(char)`; a
**non-empty** `Char(escape_char)` placeholder spanning `pos..s.len()` with
`resume_pos = s.len()` was never considered. It keeps the byte in the tree (joins the
pending chars run) and makes the tolerant escape case partition-clean.

**Decision:** switch to the `Char(escape_char)` placeholder (restores the partition), or
keep `EndOfStream` (pylatexenc parity — its recovery token is empty too) and document on
`TokenRecovery` that *bytes between `token.span.end` and `resume_pos` are dropped from
the AST*. Either way: DESIGN_RATIONALE §3.8 still says the placeholder is "an empty
chars token" — stale, needs the one-line fix regardless.

## 3. `peek` cannot honor its own memoization contract

`TokenReader::peek` doc (`src/token/reader.rs`): *"implementations may memoize on that
key (states are immutable, so `Arc` pointer identity is a sound cache key)"*. But the
signature is `fn peek(&mut self, state: &ParsingState<L>)` — a memoizing reader can only
derive a raw pointer, and cannot hold a strong reference to keep the allocation alive.
When the engine drops a state (structural revert after a group/argument), the allocator
may hand the same address to the next derived state and the reader serves a stale token
for a different rule set (classic ABA). The engine's own group-interior memo avoids
exactly this *by pinning its key `Arc`s* — the trait is the one place the API doesn't
provide the key it documents.

Every call site already holds an `Arc`: `ParseContext::state` is
`Arc<ParsingState<L>>`, and `cx.tokens.peek(&cx.state)` compiles today via deref
coercion. So changing the parameter to `&Arc<ParsingState<L>>` is **source-compatible at
every existing call site**.

**Decision:** widen the signature (recommended), or amend the doc to say memoization
requires the implementation to obtain and retain a state `Arc` by other means. Contract
and signature should agree — the doc is what a third-party reader author builds on.

## 4. `TokenReader` has no positional move; `resume_at` bypasses the readers' guards

`resume_at` (`constructs/mod.rs`) expresses "go to `pos`" by synthesizing a zero-width
`EndOfStream` marker token and calling the trait's `move_to`. Consequences:

- `StdTokenReader::move_to_pos` (inherent) debug-asserts `pos <= content.len()` and
  char-boundary; the trait path never reaches it — `move_to` is a bare
  `self.pos = tok.span.start`. A bogus `resume_pos` from a third-party reader lands
  mid-`char` or past the end with no diagnosis; the next `peek` panics on
  `content[pos..]`.
- The marker trick silently adds an unstated implementor contract: `move_to` must be
  computed from the token's spans, never from token identity/list membership.

**Recommended fix:** add `fn move_to_pos(&mut self, pos: usize)` to `TokenReader` with a
default body equal to today's marker trick (no implementor breaks); both std readers
override with their guarded inherent versions; `resume_at` becomes a forwarding call (or
is deleted in favor of the trait method). Also relevant pre-Phase-7: a `\verb`-style
verbatim parser needs raw content + absolute repositioning, and `content()`/
`move_to_pos()` are currently inherent-only — deciding trait membership now avoids
manufacturing tokens that mean "cursor".

## 5. Box the recovery payload: hot-path `Result` 104 → 80 bytes

Measured (against `SimpleLang`): `Token` = 72 B, `TokenRecovery` = 80 B,
`TokenError` = 104 B, so `Result<Token, TokenError>` — the value every `peek`/`next`
returns on every token — is 104 bytes for a 72-byte payload and an essentially-never
error case. Making `TokenError`'s recovery field `Option<Box<TokenRecovery<'s, L>>>`
drops `TokenError` to 32 B and the `Result` to 80 B (payload + tag). The error path is
cold by construction, so the boxing cost lands exactly where it doesn't matter.
`recovery()`/`into_recovery()` keep their signatures (`Option<&TokenRecovery>` /
`Option<TokenRecovery>` via `map(|b| *b)`). Modest but free.

## 6. Minor / cosmetic (batch with whichever item above lands first)

- **`Display for TokenKind` drops the escape char**: `Command("foo")` renders `\foo` and
  `@foo` identically, so `argument_parsers.rs` builds its own spelling with an eager
  `format!("{}{}", escape_char, name)` on the hot expression-dispatch path, consumed
  only in a rare error branch. A `Token`/`TokenKind` helper yielding the written form
  centralizes the spelling and lets that site defer the allocation into the error branch.
- **CRLF comment quirk**: `read_comment` sets `content_end` at the `'\n'`, so on `\r\n`
  input the `'\r'` stays inside comment `content` rather than post-space, even when
  `'\r'` is declared whitespace. Byte-for-byte pylatexenc parity (quirk inherited, not a
  regression) — but the project charter invites improving on pylatexenc quirks and this
  one is cheap.
- **Inherent/trait duplication**: `pos()` exists both inherent and on the trait
  (identical today; divergence would be invisible); `content()`/`is_at_end()` are
  inherent-only and thus unavailable to generic `TokenReader` consumers — fine if
  intended, worth a doc word (interacts with item 4).

## Decision points

1. Comment span: token field (`start: Span`) vs helper-only? (Field recommended — only
   the field closes the extension-point panic path.)
2. Dangling escape: `Char(escape_char)` placeholder vs documented byte drop?
3. `peek(&Arc<ParsingState<L>>)` — widen or re-document?
4. `move_to_pos` on the trait (with marker-trick default) — yes/no, and does `content()`
   join it before Phase 7?
5. Box the recovery — mechanical, approve and apply?
