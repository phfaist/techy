# DOC_GAPS — Phase 4 documentation gap/check register

Working scaffolding for the guide-writing stages (deleted with this directory
at review completion). Guide chapters are written from public documentation
only; when a drafting agent cannot support a claim from the documentation, it
files an entry here instead of assuming behavior. Two entry types:

- **GAP** — the public documentation is incomplete: a claim a guide chapter
  needs (or a reader will need) is not stated anywhere in rustdoc. Resolution
  is normally a rustdoc fix.
- **CHECK** — documented behavior (or a claim a chapter must make) needs
  verification against code/tests/rulings before the guide may rely on it, or
  a documented behavior seems in tension with the library's intent.

Entries are resolved by dedicated verification agents (promptly, or in G5),
never by drafting-agent guesswork. Entry format:

```
## <N>. [GAP|CHECK] <one-line title>
- Raised-by: <stage / chapter>
- Question/Claim: <what needs verifying, or what is missing>
- Why it matters: <consumer impact>
- Status: OPEN | RESOLVED — <resolution>
```

## 1. [CHECK] Every diagnostic condition type's rustdoc page must visibly display its stable identifier string

- Raised-by: G1 (seeded from the ruled parsing.md chapter scope; the chapter
  itself lands in G2).
- Question/Claim: parsing.md is ruled to state the matching rule ("match
  conditions via `T::IDENTIFIER` / `is::<T>()`, never literal strings") and
  link the auto-generated `DiagnosticInfo` implementors listing instead of
  duplicating an identifier table. That only works if each condition type's
  own rustdoc page visibly shows its stable identifier string (the
  `#[diagnostic(id = "…")]` value). Verify this holds for every condition
  type in the rendered docs; if any page does not display its identifier,
  that is a GAP to fix in rustdoc (e.g. via the derive emitting the id into
  the type's docs, or a doc line per condition type).
- Why it matters: consumers matching conditions at a boundary (logs, wire,
  bindings) need to find a condition's identifier from its API page; without
  it, they will guess strings — exactly what the ruled matching rule forbids.
- Status: RESOLVED — verified at G2 (parsing.md milestone) on a `cargo docs`
  build: every public condition type's rendered page displays its identifier,
  because the derive-generated `impl DiagnosticInfo` renders the associated
  constant with its literal value (`const IDENTIFIER: &'static str =
  "core.token.forbidden-char"`) in the page's Trait Implementations section.
  Checked mechanically for all 25 public condition types (core:
  EndOfStreamAfterEscape, ForbiddenChar, UnterminatedVerbatim,
  ExpectedVerbatimDelimiter, ImplementationError, ScopeOpFailed,
  UnresolvableCommand, CommandResolutionFailed, StrayGroupClose,
  UnusableRecoveryToken, RepeatedTackOnField, EnvironmentTerminatorMismatch,
  MalformedEnvironmentTerminator, MissingEnvironmentTerminator,
  MissingMandatoryArgument, ExpectedExpressionArgument,
  ExpressionCallableRequiresContent, UnclosedGroup, NoSourceResolver,
  UnresolvableSourceReference, ProviderCommandsShadowedByEscape,
  CallableDefinedAsError; latexlike: MalformedBegin, UnknownEnvironment,
  OrphanEnd): 25/25 show the identifier string on their canonical page. The
  `DiagnosticInfo` trait page's auto-generated Implementors section lists
  the condition types, so parsing.md's link plus per-page identifiers covers
  the Wish-23/F9 need with no duplicated table and no rustdoc changes.

## 2. [CHECK] WebAssembly suitability is stated in the introduction but not named in rustdoc

- Raised-by: G1 / introduction.md ("Where techy runs").
- Question/Claim: introduction.md presents WebAssembly builds as a use realm
  (per the ruled chapter map, which records "verified: alloc-only no_std").
  The crate-level rustdoc documents the supporting facts — `no_std`-friendly,
  depends only on `core` and `alloc`, target must support atomics, no I/O of
  its own — but never names WebAssembly. Verify a `wasm32` target build (e.g.
  `cargo build --target wasm32-unknown-unknown`) and consider adding the
  WebAssembly mention to the crate-level `no_std` rustdoc section so the
  guide claim has a direct documentation anchor.
- Why it matters: the introduction is the first page readers meet; its
  claims should each have a rustdoc sentence behind them, and embedders
  choosing techy for browser/plugin targets will act on this one.
- Status: OPEN (narrowed) — build half RESOLVED at the G1 review:
  `cargo build --target wasm32-unknown-unknown -p techy` verified clean in the
  stage worktree. Remaining action: the crate-rustdoc WebAssembly mention
  (G5).
