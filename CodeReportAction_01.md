# Action 01 — Structured errors and diagnostics

**Status: open — needs a design decision (user sign-off), then implementation.**

The error/diagnostic model is a `String` model. The mechanics are right (Arc-based
`SourceSpan`s, self-contained errors, hand-written `Display`, zero-dependency/`no_std`),
but the *payload* is prose: the tolerant path throws away every bit of structure the
strict path keeps, and two-location conditions can only point at one end. This is the
difference between "diagnostics you can print" and "diagnostics a tool (FLM, linter,
LSP) can act on".

## 1. `ParseErrorKind` is stringly

`ParseErrorKind` (`src/error.rs`) has exactly two variants: `Token(TokenErrorKind)`
(structured) and `Syntax { message: String }`. Every parse-level condition in the crate
routes through `Syntax { message: format!(…) }`:

- `group_parser.rs:123` — unclosed group at EOF (carries: expected close delimiter)
- `group_parser.rs:137` — mismatched/stray close (carries: expected close delimiter)
- `environment_parser.rs:253` — terminator name mismatch (carries: expected name, found name)
- `environment_parser.rs:272` — malformed terminator (carries: environment name)
- `environment_parser.rs:323, 338, 512, 539, 661` — missing-terminator variants
- `argument_parsers.rs:235, 276, 361, 452` — missing/malformed argument
- `nodes_parser.rs:430` — unresolvable command

Each site `format!`s the very fields a consumer would want into a sentence and discards
them. The `#[non_exhaustive]` attribute and the doc comment ("parse-level conditions
are added as the construct parsers grow") show the intent was to grow real variants —
but Phases 6.2–6.6 landed and none were added. Note the token level already did this
right: `TokenErrorKind` is a structured closed enum, deliberately replacing pylatexenc's
stringly `error_type_info`. The parse level reintroduced the pattern.

Consequences: no error codes, no programmatic filtering, no "which environment?" without
re-parsing the message, no localization hook. Messages are also constructed eagerly at
the detection site even in strict mode where nobody may read them.

**Proposal:** promote the recurring conditions to variants while `#[non_exhaustive]`
makes it cheap, e.g.:

```rust
UnclosedGroup { expected_close: String },
MismatchedGroupClose { expected_close: String },
TerminatorMismatch { expected: String, found: String },
MissingTerminator { name: String },
MissingArgument { argument_name: Option<String>, callable_name: String },
UnresolvableCommand { name: String },
```

Keep `Syntax { message }` as the escape hatch for third-party construct parsers. The
`Display` impl then owns the wording in one place (also where localization would hook in).

## 2. The tolerant path is lossy: `Diagnostic` holds only a `String`

`Diagnostic { severity, message: String, span }`. Strict mode returns
`ParseError::new(kind, span)` with the kind intact; tolerant mode does
`Diagnostic::error(kind.to_string(), span)` (`engine/mod.rs`, in `recover`) — verified
to be the **only** diagnostic producer in the library. So every diagnostic a real parse
produces is severity + sentence + span; even `Token(TokenErrorKind)`, which is `Copy`,
is flattened.

**Proposal:** an optional kind on `Diagnostic`:

```rust
pub struct Diagnostic<O: SourceOrigin = Option<String>> {
    severity: Severity,
    kind: Option<ParseErrorKind>,   // present for parser-reported conditions
    message: String,
    span: SourceSpan<O>,
}
```

with `Diagnostic::from_error_kind(kind, span)` doing the `to_string()` once, and
`Diagnostics` gaining a `filter_by_kind`-style accessor. This and item 1 are really one
decision: there is no point carrying a kind that is itself just a string.

## 3. One span per error; `format_traceback` has no producer

`ParseError` and `Diagnostic` each carry exactly one `SourceSpan`, but several
conditions are inherently two-location, and the code visibly struggles with the choice:

- unclosed group at EOF points at the **open** delimiter — the message says "before end
  of input" while the caret is on `{`, nowhere near the end of input;
- stray/mismatched close points at the **close** — the open brace, the thing the user
  needs to find, is not reported at all;
- the environment parser splits the same way between `\begin` trigger and stray close.

The idiom for this is a secondary/related span list (rustc's "unclosed delimiter here",
LSP `relatedInformation`). ARCHITECTURE.md anticipates it ("open-blocks traceback — the
existing `format_traceback` work slots in here"), and `format_traceback`
(`src/error.rs`) is ported and tested — but **nothing produces its
`&[(SourceSpan, String)]` argument**. Neither error type has a field to hold related
spans, so no rendered message can ever include a traceback.

**Proposal:** add `related: Vec<(SourceSpan<O>, String)>` (or a small `Note` struct) to
both `Diagnostic` and `ParseError`; have `render()` append
`format_traceback(&self.related)`; populate it at the two-location sites — the group
parser already holds `self.open_span`, the environment parser already holds
`self.trigger_span`. This is the highest-value structural change: it wires the ported
helper to a producer and fixes the caret-in-the-wrong-place cases in one move.

## 4. Related smaller items (same decision cluster)

- **`TokenErrorKind` gives custom scanners no escape hatch.** `Lang::scan_specials`
  returns `TokenResult` and participates in the recovery protocol, but the only kinds it
  can construct are `EndOfStreamAfterEscape` and `ForbiddenChar` — both
  tokenizer-internal. A scanner reporting its own condition (e.g. an unterminated
  `~~`-style trigger) has to lie. Suggest `TokenErrorKind::Custom { message: String }`,
  mirroring the parse-level `Syntax` escape hatch. Cost: `TokenErrorKind` loses `Copy`,
  and `TokenError::kind()` must return `&TokenErrorKind` — small, contained, and worth
  doing before downstreams exist. (`TokenErrorKind` is already `#[non_exhaustive]`.)
- **`ParseError::from_token_error(&TokenError<L>, &Arc<Source<O>>)`.** The
  `TokenError → ParseError` lift (build `ParseErrorKind::Token(kind)` + `SourceSpan`
  from the byte span) is duplicated verbatim at `constructs/mod.rs` (`try_peek`) and
  `nodes_parser.rs` (recovery arm). It cannot be a `From` impl (needs the source Arc);
  a named constructor in `error.rs` puts the layering rule in one place.
- **Derive `PartialEq`/`Eq` on `Diagnostic` and `ParseError`.** `SourceSpan` and
  `ParseErrorKind` already implement both; tests currently compare field-by-field.
- **`core::error::Error::source()`**: if `TokenErrorKind` implements
  `core::error::Error` (it is `Copy + 'static + Display` today), `ParseError::source()`
  can return `Some(kind)` for the `Token` arm — a real source chain for free. (Interacts
  with the `Custom` variant losing `Copy`; decide together.)
- **`try_peek` recoverability inconsistency** (`constructs/mod.rs`): the content loop
  distinguishes recoverable from unrecoverable token errors even in tolerant mode
  (`recovery: None` → hard abort), but `try_peek` returns `Ok(None)` for *any* token
  error under `Tolerant`. For an unrecoverable error inside an argument probe this
  yields a misleading "missing mandatory argument" diagnostic before the enclosing loop
  aborts anyway. One-line fix: mirror the loop
  (`if error.recovery().is_none() { return Err(..) }`). Only reachable via a custom
  reader / `scan_specials` today.
- **Rendered fallback opacity**: when line info is unavailable because the source
  exceeded the line-index scan limit, the rendered `@ char pos 42` gives the user no
  hint why. A parenthetical in the rendered string would close that loop.

## Decision points

1. Which conditions get real `ParseErrorKind` variants, and their exact field shapes
   (owned `String`s vs ids)?
2. Does `Diagnostic` carry `Option<ParseErrorKind>` (recommended) or stay message-only?
3. `related` spans: on both `ParseError` and `Diagnostic`? Plain
   `Vec<(SourceSpan<O>, String)>` or a named `Note` type?
4. `TokenErrorKind::Custom` — accept the loss of `Copy`?
5. Sequencing: items 1–3 are one coherent change to `error.rs` + the 13 producer sites;
   item 4 is independent and smaller.
