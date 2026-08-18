# Tokenization: a language declares its tokenization as one type

Status: DONE (2026-08-18). Executed directly on `main` in the primary checkout,
implementer + reviewer agents. Supersedes bettertokens ruling **O-4** and the
"`Lang::TokenReader` associated type" / "factory on `Lang`" rejections recorded in
[§dd-dr:token-opacity] and [§dd-dr:token-reader-hook] (user decision, 2026-08-18).
Shipped as [§dd-dr:tokenization]. Final `git diff --stat 25de12c`:
42 files changed, 959 insertions(+), 670 deletions(-).

## 1. Goal

A user plugs a different token reader into a language while reusing the same driver
implementation (`LatexlikeDriver<LLL>`, `StdParseDriver`, any driver). Today the driver
pins the reader (`make_token_reader` is required and `LatexlikeLang` pins
`Token = StdToken<Self>, StreamPosition = StdStreamPosition`).

## 2. Design (final, user-approved)

### 2.1 The `Tokenization` bundle replaces `Lang::Token` + `Lang::StreamPosition`

New file `techy/src/token/tokenization.rs`:

```rust
/// A language's tokenization, declared once at the type level: the token type its
/// readers produce, the stream-position type they hand out, and how the reader for a
/// parse over one source is built. Implemented by a zero-sized type; never a value.
pub trait Tokenization<L: Lang> {
    /// The token type. Opaque: … (move the opacity contract here from the deleted
    /// marker trait `Token<L>` and from the old `Lang::Token` docs).
    type Token: Clone + fmt::Debug + PartialEq + Send + Sync;
    /// The stream-position type. Opaque, equality only: … (from the old
    /// `Lang::StreamPosition` docs).
    type StreamPosition: Clone + fmt::Debug + PartialEq + Eq + Send + Sync;
    /// Build the reader for one parse over `source`. Static (no receiver): a reader
    /// that needs runtime data reads it from the parsing state passed to `peek`
    /// ([§dd-dr:token-reader]) or is built by a driver overriding
    /// `ParseDriver::make_token_reader`, the per-instance door.
    fn make_token_reader<'s>(
        source: &'s Arc<Source<L::SourceOrigin>>,
    ) -> Box<dyn TokenReader<'s, L> + 's>;
}

/// The token type of `L` — projection through `Lang::Tokenization`
/// (the `NodeExt<L>` / `ArgumentExt<L>` / `SlotExt<L>` precedent, node/mod.rs:81-85).
pub type Token<L> = <<L as Lang>::Tokenization as Tokenization<L>>::Token;
/// The stream-position type of `L`.
pub type StreamPosition<L> = <<L as Lang>::Tokenization as Tokenization<L>>::StreamPosition;

/// The standard tokenization: `StdToken<L>` tokens, `StdStreamPosition` positions,
/// `StdTokenReader` readers.
#[derive(Debug, Clone, Copy)]
pub struct StdTokenization;

// NOTE the bound: `Tokenization = StdTokenization`, NOT the equality on Token /
// StreamPosition — the latter cycles through the projection (E0275, verified).
impl<L: Lang<Tokenization = StdTokenization>> Tokenization<L> for StdTokenization {
    type Token = StdToken<L>;
    type StreamPosition = StdStreamPosition;
    fn make_token_reader<'s>(source: &'s Arc<Source<L::SourceOrigin>>)
        -> Box<dyn TokenReader<'s, L> + 's> {
        Box::new(StdTokenReader::new(source))
    }
}
```

`Lang`:
- **remove** `type Token`, `type StreamPosition`;
- **add** `type Tokenization: Tokenization<Self>;` at the same place (after
  `SourceOrigin`), with docs: what it declares; `StdTokenization` for every language
  tokenized by the standard reader; a custom reader = a ZST implementing the trait; the
  driver's `make_token_reader` remains the per-instance override door.
- `TrivialLang` blanket impl: `type Tokenization = StdTokenization;` (replacing the two).

The marker trait `Token<L>` (`token/token.rs`) is **deleted** together with
`impl<L: Lang> Token<L> for StdToken<L> {}`; its bounds move onto
`Tokenization::Token` (symmetric with `StreamPosition`, which never had a marker). The
name `Token` exported from `techy::token` / `techy::core` is now the alias.

### 2.2 `ParseDriver::make_token_reader` becomes defaulted

```rust
fn make_token_reader<'s>(&'s self, source: &'s Arc<Source<L::SourceOrigin>>)
    -> Box<dyn TokenReader<'s, L> + 's> {
    L::Tokenization::make_token_reader(source)
}
```

Docs: "every hook is defaulted" again (trait docs, `an_empty_driver_impl_is_complete`
test, `docs/custom-lang.md`, ARCHITECTURE); the hook stays *the door* for a driver that
hands its reader configuration it holds. Delete every override whose body is
`Box::new(StdTokenReader::new(source))` (`StdParseDriver`, `LatexlikeDriver`, the ~34
test drivers). Overrides installing a *different* reader (`BrokenReader`,
`FlakyReader`, `StuckRecoveryReader`, `TabooReader`, `CommentEmittingReader` in
`techy/tests/lang_features.rs`, …) stay — they are the door's examples.

`StdParseDriver`'s `impl ParseDriver<L>` drops its
`L: Lang<Token = StdToken<L>, StreamPosition = StdStreamPosition>` bound (and the
comment above it): the ready-made driver now serves every language.

### 2.3 Bounds and spellings

- Every `L::Token` / `LLL::Token` / `Self::Token` → `Token<L>` (alias; import
  `crate::token::Token`); every `L::StreamPosition` → `StreamPosition<L>`.
- Every equality bound `L: Lang<Token = StdToken<L>, StreamPosition = StdStreamPosition>`
  (`StdTokenReader`'s impls in `token/reader.rs`, `TokenListReader`, the
  `nodes_parser.rs` scan helpers, …) → **`L::Tokenization: Tokenization<L, Token =
  StdToken<L>, StreamPosition = StdStreamPosition>`** (where-clause form). Never
  `L: Lang<Tokenization = StdTokenization>` there: the equality form is what keeps the
  documented "reader over standard tokens" wrapper pattern compiling (a language with its
  own `Tokenization` whose reader wraps an inner `StdTokenReader`). The one legitimate
  `Tokenization = StdTokenization` bound is `StdTokenization`'s own impl (§2.1).
- Every explicit `impl Lang for …` (≈55 in `techy/src`, plus `techy/tests`): the two
  lines become `type Tokenization = StdTokenization;`.
- `LatexlikeLang`: drop the two pins and their comment. `Latexlike`:
  `type Tokenization = StdTokenization;`. `LatexlikeDriver`: delete its
  `make_token_reader` impl and the `StdTokenReader` import.
  `latexlike/environments.rs:~1048`: positions are no longer known `Copy` in the family
  — clone `group.end` (drop the "`Copy` (`StdStreamPosition`)" comment).
- Exports: `techy/src/token/mod.rs` and the `techy::core` facade
  (`techy/src/core/mod.rs`) export `Tokenization`, `StdTokenization`, `Token` (alias),
  `StreamPosition` (alias); the facade module docs' "Tokens" bullet names them.

### 2.4 Verified facts (standalone probe, 2026-08-18)

- Compiles on stable (MSRV 1.86): the alias projections, the defaulted door, the
  wrapper-reader-over-std-tokens pattern under the equality bound, an own-token
  language, generic code spelled with the aliases.
- A supertrait-with-blanket-impl trick to keep the `L::Token` spelling does **not**
  work (the param-env candidate shadows the blanket impl; the projection never
  normalizes) — hence the aliases.
- A reader *type* on `Lang` would have to be a GAT (`TokenReader<'s, L>` carries the
  source lifetime) and would push `'s` into every token-holding type; and a constructor
  on `TokenReader` presumes source-only construction (`TokenListReader` takes a list).
  Hence the lifetime-free bundle with a factory fn.

## 3. Documentation

Rustdoc is doctested and `broken_intra_doc_links` is deny — every link to
`Lang::Token` / `Lang::StreamPosition` / the marker trait must be retargeted
(`Tokenization::Token`, `Tokenization::StreamPosition`, `Token<L>`…).

- Guides: `docs/custom-lang.md` (the `impl Lang` example ~219, the reader paragraph
  ~87-90, the `make_token_reader` "no default" paragraph ~437), `docs/ai-guide-custom-lang.md`
  (table rows 41-42, ~125), `docs/concepts-overview.md` (~37, ~48), `docs/parsing-model.md`
  (~37). Prose rule ([techy-docs-clarity]): no metaphors, define terms; US English.
  "The standard tokenizer" for `StdTokenReader` → "the standard reader" (the trait now
  owns the word *tokenization*).
- `dev-docs/ARCHITECTURE.md`: lines ~104, 138, 222, 244, 249, 296-299, 764, 782, 845 —
  `Lang::Tokenization`, the aliases, no marker trait, `make_token_reader` defaulted;
  reference the new rationale entry.
- `dev-docs/DESIGN_RATIONALE.md`:
  - **new entry** `#### A language declares its tokenization as one type [§dd-dr:tokenization]`
    (template of the file): the decision (bundle on `Lang`, aliases, `StdTokenization`,
    defaulted door), what it buys (reader-agnostic drivers, `LatexlikeLang` unpinned,
    `Lang` one type smaller, every `ParseDriver` hook defaulted), the pitfalls of §2.4,
    the `Tokenization = StdTokenization` bound rule and its E0275 reason, the equality-bound
    rule for the wrapper pattern, rejected: reader type on `Lang` (GAT), constructor on
    `TokenReader`, add-only variant (`Lang` keeping `Token`/`StreamPosition` beside the
    bundle — the redundancy), keeping the marker trait (renamed `TokenBase`; no methods,
    `StreamPosition` never had one), the supertrait sugar; revisit-if.
  - amend **[§dd-dr:token-reader-hook]**: defaulted now; the standard body's home is
    `StdTokenization`; the "factory on `Lang`" rejection and the "revisit if Rust gains
    specialization" line are superseded (say so, dated); status line updated.
  - amend **[§dd-dr:token-opacity]** (`Lang::Token` → `Tokenization::Token`/`Token<L>`,
    no marker trait; the "`Lang::TokenReader` associated type" rejection rewritten: what
    was rejected is a reader *type* on `Lang`, replaced by the bundle), **[§dd-dr:stream-position]**,
    **[§dd-dr:zero-copy-tokens]**, **[§dd-dr:reader-context-purity]** (mentions of
    `Lang::Token`/`Lang::StreamPosition`/`make_token_reader` as required), and any other
    entry naming them (grep `Lang::Token\|Lang::StreamPosition\|make_token_reader\|Token<L>`).
  - **[§dd-dr:superseded-names]**: add `Lang::Token`, `Lang::StreamPosition`, the marker
    trait `Token<L>`.
  - Maintenance rule: every entry referenced from ARCHITECTURE (the new one too).
- `dev-docs/bettertokens/PLAN.md`: at O-4 and §1.17/§7 item 6 add a dated
  *Superseded (2026-08-18): see dev-docs/tokenization/PLAN.md, [§dd-dr:tokenization]*
  note; `PROGRESS.md` a one-line trailer. Do not rewrite history there.
- `TODO_Big.md`: check the two token-layer items still read correctly.
- Update the Status line of this file when done, with the final diffstat.

## 4. Gates (all must be clean before each commit)

`cargo build` · `cargo test` (unit + integration + doctests) ·
`cargo clippy --all-targets -- -D warnings` (main is clean; stay clean) ·
`rm -rf target/doc && cargo docs` (zero warnings) ·
grep gates: no `Lang::Token`, `Lang::StreamPosition`, `L::Token\b`, `LLL::Token\b`,
`::StreamPosition\b` (other than the alias definitions), no `impl<L: Lang> Token<L> for`,
no `Token = StdToken<` / `StreamPosition = StdStreamPosition` inside a `Lang<…>` bound,
no `Box::new(StdTokenReader::new(source))` outside `StdTokenization`.
`scripts/check_semver.sh` is *not* a gate (soft freeze; breaking changes expected).

## 5. Execution

Commits on `main`, in order: (1) this plan; (2) code + rustdoc + guides (one commit,
gates green); (3) dev-docs (ARCHITECTURE, DESIGN_RATIONALE, bettertokens notes, this
plan's status); (4) review fixes if any. Implementer and reviewer are separate Opus
agents run one after the other in the primary checkout — never concurrently.
