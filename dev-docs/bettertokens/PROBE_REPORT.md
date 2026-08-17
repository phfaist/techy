# Better tokens — Stage 0 probe report

Stage 0 of `PLAN.md` (§2). Date: 2026-08-17. Branch `bt-probe`, worktree
`/Users/philippe/projects/techy/.claude/worktrees/bt-probe`, probe crate
`bettertokens-probe/` (standalone, its own workspace, zero dependencies, never merged).
Toolchain: `cargo 1.97.0` / `rustc 1.97.0`, edition 2021, `rust-version = "1.86"`.

**Verdict: every shape §1 prescribes compiles.** Seven probes pass outright; P8 fails
only for the `StdTokenReader` impl header as §1.8/§2 write it down, and passes with the
two extra associated-type bounds rustc itself suggests. No fallback from §9 is needed, and the
probe opens no design question.

Gates:

```
$ cargo check
    Checking bettertokens-probe v0.0.0 (…/bt-probe/bettertokens-probe)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.36s

$ cargo test
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests bettertokens_probe
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

No warnings.

---

## Results at a glance

| Probe | Question | Result |
|---|---|---|
| P1 | Is `TokenReader<'s, L>` (§1.6, all methods) object-safe? | **PASS** |
| P2 | Can a `TokenKind<'t, L>` view live in `Invocation` across a `&mut ParseContext` sub-parse? | **PASS** |
| P3 | Can a token-owning reader satisfy the same `token_kind` signature? | **PASS** |
| P4 | Wrapper reader minting `StdToken`s and delegating interpretation? | **PASS** (needs a qualified call, see below) |
| P5 | `L::StreamPosition` in `StopCause<L>` & co. with manual `Debug`/`PartialEq`/`Clone`? | **PASS** |
| P6 | `L::InvocationSyntax::from_invocation(&invocation, &*cx.tokens)` under a `&mut ParseContext`? | **PASS** |
| P7 | `StdToken<L>` bounds, and `type Token = StdToken<Self>`? | **PASS** |
| P8 | The `StdTokenReader` impl header with a bare `Lang<SourceOrigin = O>` bound (§1.8/§2)? | **FAIL as written — PASS with two added associated-type bounds** |

---

## P1 — object safety of `TokenReader<'s, L>`

**PASS.** Dyn-compatibility breaks on *type*/const generics on methods and on `Self` in
argument or return position; a method generic over a **lifetime**, carrying a
lifetime-only `where` clause, is fine. Every method — including the two provided ones —
was called through `&mut dyn TokenReader<'_, MockLang>`, through the
`Box<dyn TokenReader<'s, L> + 's>` that the mock driver's `make_token_reader` returns,
and through the `tokens` field of the mock `ParseContext` (a reborrow of
`&'a mut dyn TokenReader<'s, L>`, which is how a construct parser reaches the reader) —
`src/p1.rs::exercise_every_method` is called from all three.

Final trait text used (`bettertokens-probe/src/mock/reader.rs`; identical to §1.6 apart
from trimmed doc comments):

```rust
pub trait TokenReader<'s, L: Lang> {
    fn peek(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<L, L::Token>;

    fn next(&mut self, state: &Arc<ParsingState<L>>) -> TokenResult<L, L::Token> {
        let token = self.peek(state)?;
        self.move_to(&token, TokenEdge::EndPastPostSpace);
        Ok(token)
    }

    fn move_to(&mut self, tok: &L::Token, edge: TokenEdge);
    fn move_to_position(&mut self, at: &L::StreamPosition);

    fn token_kind<'t>(&self, tok: &'t L::Token) -> TokenKind<'t, L>
    where
        's: 't;

    fn source_span_between(
        &self,
        tok: &L::Token,
        a: TokenEdge,
        b: TokenEdge,
    ) -> SourceSpan<L::SourceOrigin>;

    fn source_span_of(&self, tok: &L::Token) -> SourceSpan<L::SourceOrigin> {
        self.source_span_between(tok, TokenEdge::Start, TokenEdge::EndPastPostSpace)
    }

    fn position_here(&self) -> L::StreamPosition;
    fn position_at(&self, tok: &L::Token, edge: TokenEdge) -> L::StreamPosition;
    fn source_position_at(&self, at: &L::StreamPosition) -> SourcePos<L::SourceOrigin>;

    fn source_span_within(
        &self,
        begin: &L::StreamPosition,
        end: &L::StreamPosition,
    ) -> Option<SourceSpan<L::SourceOrigin>>;
}
```

## P2 — the view held across mutable use of the reader

**PASS.** The shape §2 asks for compiles as written
(`bettertokens-probe/src/p2.rs::parse_invocation`):

```rust
let tok = cx.tokens.peek(&state).map_err(|_| ())?;
let TokenKind::Command { name, .. } = cx.tokens.token_kind(&tok) else { return Err(()) };
let invocation = Invocation {
    callable_type: 7u8, name, spec, token: &tok, kind: cx.tokens.token_kind(&tok),
};
cx.tokens.move_to(invocation.token, TokenEdge::EndPastPostSpace);
sub_parse(cx)?;                       // takes &mut ParseContext; peeks and moves
let name_after = invocation.name.to_string();
assert!(matches!(invocation.kind, TokenKind::Command { .. }));
let span = cx.tokens.source_span_of(invocation.token);
```

The reason it works is worth writing into the rustdoc: `token_kind`'s `&self` receiver
lifetime **does not appear in the return type**. The view therefore borrows the token
(`'t`) and, for a content-scanning reader, the content (`'s`, which outlives `'t`) —
never the reader. The shared borrow of `*cx.tokens` ends when the call returns, so the
later `&mut` reborrow is unobstructed.

Consequence for §1.16: **`Invocation.kind` stays a field.** No `Invocation.name: String`
copy is needed; no allocation per invocation.

## P3 — a token-owning reader

**PASS.** `OwnedToken<O> { source: Arc<Source<O>>, span, pre_space, … }` has no lifetime
parameter, and `OwnedTokenReader<'s>` (whose `'s` is a `PhantomData` — it borrows
nothing) satisfies the very same trait, returning `&'t str`s sliced out of
`tok.source.content()`:

```rust
fn token_kind<'t>(&self, tok: &'t OwnedToken) -> TokenKind<'t, OwnedLang>
where
    's: 't,
{
    let text: &'t str = &tok.source.content()[tok.span.start()..tok.span.end()];
    …
}
```

This settles the worry the `where 's: 't` clause raises: `'s: 't` is an *outlives
obligation on the caller*, not a promise that the view's strings come from `'s`. A future
expanding reader can mint sources mid-parse. The test also checks that
`source_span_between` may qualify the span by the **token's** source rather than the
reader's current one, and that the token still answers after the caller's own `Arc` to
the source is dropped.

## P4 — a wrapper reader over std tokens

**PASS**, with one ergonomic note.

`WrapperReader<'s, L> { inner: StdTokenReader<'s, L::SourceOrigin>, … }` implements
`TokenReader<'s, L>` for the same `L`, re-classifies `~` characters by minting a
`Specials` token through the public `StdToken::specials` constructor, and delegates
`token_kind`, `source_span_between`, `position_here`, `position_at`,
`source_position_at`, `source_span_within` to the inner reader. Both readers were driven
through `&mut dyn TokenReader<'_, MockLang>` in one test.

Two findings for §1.8's documented pattern:

1. **The public API suffices for the wrapper**, including the `Span`s the constructors
   want: the wrapper recovers them from
   `inner.source_span_between(&token, StartBeforePreSpace, Start).span()` and
   `inner.source_span_between(&token, Start, EndPastPostSpace).span()`. No `pub(crate)`
   accessor is needed. (This holds because a `StdTokenReader`'s reader-relative offsets
   are offsets into its `Source`'s content. Worth stating in the pattern's rustdoc.)
2. **Delegation needs the trait named.** In a wrapper generic over `L`, plain method
   syntax `self.inner.token_kind(tok)` fails, because the inner reader's impl is generic
   over the language too and `L` is not inferable from the receiver:

   ```
   error[E0284]: type annotations needed
     --> src/p4.rs:60:33
      |
   60 |         if !matches!(self.inner.token_kind(&token), TokenKind::Char('~')) {
      |                                 ^^^^^^^^^^
      = note: cannot satisfy `<_ as Lang>::Token == StdToken<L>`
   ```

   Fixed either by a fully qualified call or — what the probe uses, and what reads best —
   by two private helpers that coerce once:

   ```rust
   fn inner(&self) -> &dyn TokenReader<'s, L> { &self.inner }
   fn inner_mut(&mut self) -> &mut dyn TokenReader<'s, L> { &mut self.inner }
   ```

   Recommend showing this in the `custom-lang` guide's wrapper example so third parties
   do not hit E0284 first thing.

## P5 — `L::StreamPosition` in lifetime-free outputs

**PASS.** `StopCause<L>`, `NodesOutcome<L>` and `EnvironmentBody<L>` hold
`L::StreamPosition` (and `SourceSpan<L::SourceOrigin>`) and get manual
`Clone`/`Debug`/`PartialEq` impls with **no** `L:` bound beyond `L: Lang`; the field
types carry their own bounds from the associated types' declarations.

The derive is genuinely unusable, and its failure is deferred to the *use* site, which is
easy to miss: `#[derive(Debug)]` is accepted where the type is defined and then fails
wherever a generic function formats it. Recorded in `src/p5.rs` behind `#[cfg(any())]`:

```
error[E0277]: `L` doesn't implement `Debug`
   |
   |     format!("{b:?}")
   |              ^^^^^ `L` cannot be formatted using `{:?}` because it doesn't implement `Debug`
note: required for `EnvironmentBodyDerived<L>` to implement `Debug`
   = help: consider manually implementing `Debug` to avoid undesired bounds
```

## P6 — `from_invocation` with the reader

**PASS**, in both arrangements:

```rust
pub fn syntax(&self, cx: &mut ParseContext<'_, '_, L>) -> L::InvocationSyntax {
    let syntax = L::InvocationSyntax::from_invocation(&self.invocation, &*cx.tokens);
    cx.tokens.move_to(self.invocation.token, TokenEdge::EndPastPostSpace);  // still usable
    syntax
}
```

— the invocation as a field of the parser (borrowed from `&self`), and the invocation
built inside the method from a freshly peeked token. The shared reborrow `&*cx.tokens`
out of the `&'a mut dyn TokenReader<'s, L>` field ends with the call, so the reader is
mutably usable immediately afterwards. The mock payload does what latexlike's
`Macro { post_space }` does: `tokens.source_span_between(token, End, EndPastPostSpace).span()`.

Note for implementers: `L::InvocationSyntax::from_invocation(…)` is the right spelling
for a *type parameter*; on a concrete language it must be written
`<Latexlike as Lang>::InvocationSyntax::from_invocation(…)` (plain
`Latexlike::InvocationSyntax::…` is `error[E0223]: ambiguous associated type`).

## P7 — `Lang::Token` bounds and the `Self` recursion

**PASS.** `StdToken<L>` holding `Arc<dyn Any + Send + Sync>` (the spec),
`Arc<GroupRule<L>>` and `L::CallableTypeId` satisfies
`Clone + Debug + PartialEq + Send + Sync + 'static` with manual `Clone`/`Debug`/`PartialEq`
(the derives would demand `L: Clone` etc., cf. P5) and auto-derived `Send`/`Sync` — the
latter because `Lang::CallableTypeId` is declared `Send + Sync` and the `Arc` payloads are.
A token was moved into another thread and formatted there.

`impl Lang for MockLang { type Token = StdToken<Self>; … }` — recursion through `Self` in
an associated type whose bound (`Token<Self>`) in turn requires `Self: Lang` — is accepted
without a cycle error; two languages (`MockLang` with `CallableTypeId = u8`, `AltLang`
with an enum) both use `StdToken<Self>`.

Equality on the specials variant compares the spec by `Arc` identity, as §1.3 says; the
probe asserts two tokens with equal spec *text* but different `Arc`s are unequal.

## P8 — the `StdTokenReader` impl-bound spelling

**FAIL for the bare `Lang<SourceOrigin = O>` bound; PASS with the two added
associated-type equalities.** (The header probed is §2's spelling of the impl, which
completes §1.8's — §1.8 writes `impl<'s, L: Lang<SourceOrigin = O>> …`, without
introducing `O` in the generic list at all.)

`impl<'s, O: SourceOrigin, L: Lang<SourceOrigin = O>> TokenReader<'s, L> for StdTokenReader<'s, O>`
does not compile: `Lang<SourceOrigin = O>` says nothing about `L::Token` and
`L::StreamPosition`, while the std reader can only produce `StdToken<L>` and
`StdStreamPosition`. Nine errors, of three shapes (full text kept in `src/p8.rs` behind
`#[cfg(any())]`):

```
error[E0308]: mismatched types
    |         Ok(StdToken::end_of_stream(Span::empty(self.pos)))
    |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected associated type, found `StdToken<_>`
    = note: expected associated type `<L as Lang>::Token`
                        found struct `StdToken<_>`
help: consider constraining the associated type `<L as Lang>::Token` to `StdToken<_>`

error[E0599]: no method named `edge_offset` found for reference `&<L as Lang>::Token` … (×4)
error[E0599]: no method named `offset` found for reference `&<L as Lang>::StreamPosition` … (×2)
error[E0308]: mismatched types … (×3: position_here, position_at, source_position_at)
```

The spelling that compiles — rustc's own suggestion, and the one every other probe uses:

```rust
impl<'s, O: SourceOrigin, L> TokenReader<'s, L> for StdTokenReader<'s, O>
where
    L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
{ … }
```

The reader stays generic over the **origin**, not over the language: the §9 fallback
`StdTokenReader<'s, L>` is **not** needed. Used in the probe through
`&mut dyn TokenReader<'_, MockLang>`, through `&mut dyn TokenReader<'_, AltLang>` (same
reader type, two languages, one source), and through the
`Box<dyn TokenReader<'s, MockLang> + 's>` returned by `make_token_reader`.

**Inference caveat, worth knowing in Stages 1–3** (cheap to work around, no design
impact): because the impl is generic over `L`, a call *on the concrete
`StdTokenReader`* whose only argument mentions `L` indirectly (`&L::Token`,
`&L::StreamPosition`) cannot pin `L`:

```
error[E0284]: type annotations needed
    |             span: reader.source_span_of(&close),
    |                          ^^^^^^^^^^^^^^
    = note: cannot satisfy `<_ as Lang>::Token == StdToken<lang::MockLang>`
```

Methods that take `&Arc<ParsingState<L>>` (`peek`, `next`) are unaffected. Binding the
reader as `let r: &mut dyn TokenReader<'_, TheLang> = &mut std_reader;` (or a fully
qualified call) fixes it. Construct-parser code never meets this — it always holds
`cx.tokens: &mut dyn TokenReader<'s, L>` — but `token/reader.rs`'s own unit tests and the
`TokenListReader` harness will.

---

## Settled spellings — copy these into the later stages

1. **The `TokenReader<'s, L>` trait**: exactly the §1.6 text, quoted in full under P1
   above. No associated types; `token_kind` is the only lifetime-generic method; `next`
   and `source_span_of` are provided.

2. **`token_kind`**:

   ```rust
   fn token_kind<'t>(&self, tok: &'t L::Token) -> TokenKind<'t, L>
   where
       's: 't;
   ```

   `&self`, **not** `&'t self` — the receiver's lifetime must stay out of the return type;
   that is what lets P2 hold the view across a sub-parse.

3. **The `StdTokenReader` impl header**:

   ```rust
   impl<'s, O: SourceOrigin, L> TokenReader<'s, L> for StdTokenReader<'s, O>
   where
       L: Lang<SourceOrigin = O, Token = StdToken<L>, StreamPosition = StdStreamPosition>,
   ```

   (The same `where` clause is needed on any inherent helper of `StdTokenReader` that
   builds tokens, e.g. the scanning core, since it must name `StdToken<L>`.)

4. **`Invocation`** (§1.9, unchanged by the probe — `kind` stays):

   ```rust
   pub struct Invocation<'a, L: Lang> {
       pub callable_type: L::CallableTypeId,
       pub name: &'a str,
       pub spec: &'a Arc<dyn CallableSpec<L>>,
       pub token: &'a L::Token,
       pub kind: TokenKind<'a, L>,
   }

   pub trait FromInvocation<L: Lang>: Sized {
       fn from_invocation(invocation: &Invocation<'_, L>, tokens: &dyn TokenReader<'_, L>) -> Self;
   }
   ```

5. **`make_token_reader`** (§1.10, verbatim, verified through a `dyn`-returning mock
   driver):

   ```rust
   fn make_token_reader<'s>(
       &'s self,
       source: &'s Arc<Source<L::SourceOrigin>>,
   ) -> Box<dyn TokenReader<'s, L> + 's>;
   ```

6. **Manual `Clone`/`Debug`/`PartialEq`** on every `L`-parameterized output type
   (`StopCause<L>`, `NodesOutcome<L>`, `EnvironmentBody<L>`, `TokenError<L>`,
   `TokenRecovery<L>`, `StdToken<L>`, `TokenKind<'t, L>`) — never a derive.

7. **Delegation inside a language-generic wrapper reader** goes through
   `&dyn TokenReader<'s, L>` / `&mut dyn TokenReader<'s, L>` helpers (or fully qualified
   calls), never plain method syntax on the inner concrete reader.

---

## What the probe did **not** cover

Out of §2's scope, and therefore unverified by the compiler so far: `stage_invocation`'s
three-case end rule (§1.9), the chars-run marker arithmetic (§1.11,
`nodes_parser.rs:512-563`), `TokenListReader`'s issued-token/position validation (§1.8),
and the specials hook's re-signature beyond declaring `SpecialsScanError` —
`SpecialsMatch` was not mocked at all. None
of these raise a lifetime or dyn-safety question the probes leave open; they are ordinary
Stage 1–2 work.

## Open questions from Stage 0

**None from the probe itself.** The §1.17 rulings O-1 and O-2 were closed by the user on
`main` (commit `b528eea`) while Stage 0 ran; Stage 0 neither needed nor affected them.
One incidental confirmation for O-1's ruling (`CallableQuery` carries
`token_kind: Option<TokenKind<'a, L>>` by value and the resolve chain takes
`token_kind: TokenKind<'_, L>`): the view is `Copy`, and holding it **by value** in a
lifetime-parameterized struct compiles and survives later mutable use of the reader —
that is exactly the `Invocation.kind` shape P2 exercises, and P7's `assert_token_bounds`
covers the manual `Clone`/`Copy` impls it relies on.

Two incidental notes for the orchestrator:

- The worktree's `.cargo/config.toml` sets
  `rustdocflags = ["--html-in-header", "docs/rustdoc-header.html"]`, and cargo applies it
  to the standalone probe crate as well (config discovery walks up from the invocation
  directory, workspace membership is irrelevant). Without a `docs/rustdoc-header.html`
  inside the probe crate, `cargo test`'s doctest step fails with
  `error reading 'docs/rustdoc-header.html'`. The probe carries an empty stand-in file.
  This affects only standalone crates created under the worktree; the techy crate itself
  is unaffected.
- The probe crate needs no network access (zero dependencies); `cargo check`/`cargo test`
  ran offline without incident.
