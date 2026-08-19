# Quick Start Guide for Claude

## Project Overview

**techy** is a Rust rewrite of Python's `pylatexenc` library for parsing LaTeX-like markup languages. It builds an Abstract Syntax Tree (AST) from LaTeX source code.

Original python project is at: https://github.com/phfaist/pylatexenc

## Key Architecture

```
token → constructs → node (AST)
```

**Public topology** ([§dd-dr:public-namespace-topology]): the public API is exported
exclusively via facades, exactly one canonical public path per item; internal src
modules are `pub(crate)` and invisible to public paths:

- **techy::source**: Source model (Source, SourceSpan, Span, SourceProvenance, LineIndex, LineIndexCache, TextContent, SourceResolver, the include-chain helpers)
- **techy::error**: Diagnostics (Diagnostic, Diagnostics, ParseError, Severity, Recovery; the DiagnosticInfo/ToDiagnosticValue derives)
- **techy::extract**: Extraction helpers over parsed trees (SplitAtChars, KeyVals, free fns; the producers mint output annotations via per-part callbacks)
- **techy::transform**: Tree→tree transformation — the streaming restage driver (TreeRestager, RestageVisitor, Restage, RestageContext + region ops, RestagedArgument/RestagedSlot, RestageError)
- **techy::visit**: Read-only structural traversal (TreeWalker, NodeVisitor, VisitFlow, VisitContext, WalkError; the walk is role-blind and depth-guarded)
- **techy::recompose**: Tree→value recomposition — the meaning-free piece fold (TreeRecomposer, Recomposer, Recompose::{Emit, Concat(ConcatPieces)}, ComposePiece, RecomposeContext + region ops, RecomposeError, core_source_instruction)
- **techy::serialize**: Serialization to and from a format-independent value model (SerialValue, SerialIndex + `serial_index!`; SerdeSession, Segment, ObjectSerdeDriver, TableHandle; the SerializableObject/DeserializableObject traits with the SerializableLang opt-in; KnownProviders; the `serde` feature gates rendering only — `to_value`/`from_value`); the preset's opt-in is `latexlike::serialize`
- **techy::core**: The flat machinery hub — Lang/state (Lang, ParsingState, ParsingStateDelta, TrivialLang), engine (Language + `parse()`, ParseDriver, ParserSession, ParseResult, Frame/FrameTitle/FrameRole)
- **techy::core::token**: The tokenization library (Tokenization/StdTokenization — the per-language bundle behind the `Token<L>`/`StreamPosition<L>` aliases, StdToken, TokenKind, TokenEdge; TokenReader, StdTokenReader, skip_whitespace; TokenRules and its per-feature blocks with the matching *Overrides and the PrefixTable/TriggerChars caches; SpecialsMatch/SpecialsScanError; the TokenError family)
- **techy::core::specs**: Defining callables (CallableSpec, StdCallableSpec, ArgumentSpec; SpecsProvider, Package, Scope, ScopeStack; the command-resolution family)
- **techy::core::constructs**: Construct parsers (ConstructParser trait + standard parsers, ArgumentParser, their diagnostic conditions)
- **techy::core::node**: AST storage (NodeTree, NodeKind, NodeRef, GroupData, CallableData, CommentData, NodeTreeBuilder)
- **techy::latexlike**: The LaTeX-behavior preset (Latexlike lang, LatexlikeDriver, preset specs, the SourceRecomposer source re-emission; `latexlike::minidefs` = the opt-in toy `minilatex` package)

Internal file layout (techy/src/token, techy/src/state, techy/src/engine,
techy/src/spec, techy/src/scopes, …) is organizational only — it never shows in
public paths.

## Critical Naming Conventions

**The naming principles live in dev-docs/ARCHITECTURE.md [§dd-arch:naming] — check them
before suggesting names.**

Key naming rules:
- **No "Latex" prefixes**: Use `Token` not `LatexToken` (LaTeX names live in the preset)
- **Specificity matters**: `ParsingStateDelta` not `StateDelta` (too vague)
- **Clarity over brevity**: `ParsedArguments` not `Arguments` (the spec-side `ArgumentSpec`/`ArgumentParser` vocabulary coexists in scope)
- **Context determines names** — but only when no sibling vocabulary competes in the same scope ([§dd-arch:naming] principles 3–4)
- Names consciously rejected or replaced must not come back: DESIGN_RATIONALE [§dd-dr:superseded-names]

**Module organization**:
- `techy::core` = the flat machinery hub (`Language::parse()`, `ParserSession`, `ParseDriver`, state); `core::token` = the tokenization library (token types, reader, rules); `core::constructs` = parsers for individual constructs (traits, parsers); `core::specs` = author-side definitions; `core::node` = the node tree
- Node taxonomy is the closed `NodeKind`: `Chars`/`Group`/`Callable`/`Comment`/`List` — "macro"/"environment" are preset vocabulary, not node kinds


## Development Workflow

```bash
cargo build          # Build
cargo test           # Run tests
cargo test -- --nocapture  # With output
cargo test <name>    # Specific test
cargo docs           # Build documentation (alias: doc --workspace --no-deps);
                     # rm -rf target/doc first when verifying links
```

## Important Files

- [Documentation_Structure.md] - The specification of the documentation system itself
  (pillars, label scheme, cross-referencing rules). Follow it for any documentation work.
- [dev-docs/ARCHITECTURE.md] - The present-day structure of the library (strata, topics,
  design principles). Sections carry immutable `[§dd-arch:<name>]` labels.
- [dev-docs/DESIGN_RATIONALE.md] - The decision register: why the library is shaped this
  way, with rejected alternatives. Entries carry immutable `[§dd-dr:<name>]` labels; every
  entry must be referenced from ARCHITECTURE (manual grep discipline — see its
  maintenance rules).

If you need to consult `pylatexenc` sources, they are available at `$HOME/Research/util/pylatexenc/`.  Overall, we should try to achieve more or less parity with pylatexenc's capabilities on the features we are planning to implement, while taking advantage of the opportunity to improve on some bugs and quicks of pylatexenc.

## Design Philosophy

1. **Clean slate**: No Python backwards compatibility baggage
2. **Rust-first**: Leverage ownership, lifetimes, zero-cost abstractions
3. **Extensibility**: Easy custom macros/environments via specs
4. **Type safety**: Compiler catches errors Python couldn't
5. **Performance**: faster than Python target
6. **Generic**: "techy" not "latex" - works for LaTeX-like languages

## When Helping

1. **Ask before taking design decisions** as I (the user) want a high degree of
   control and discussion put in design decisions.
2. **Keep in mind that most of the code base is likely to change significantly** as
   I (the user) am progressing through files individually, reviewing them one by one with significant changes. The changes aim to granularily review design decisions and ultimately implement a library that is as powerful and extensible as the original pylatexenc project.
3. **Never undo my code edits** before confirming my intent on these edits in the first
   place. Do NOT remove any code that appears useless before asking.
4. **Use Result<T,E>** consistently — never panic in lib code on input, *including* input
   from outer library layers (specs, hooks, custom parsers): a documented-contract
   violation returns an `Err`, it does not panic. Panics are allowed only for verifiably
   unreachable invariants (`unreachable!`/`expect` with the invariant stated), plus a
   small register of individually user-approved exceptions in deep std-convention code:
   the indexing-style accessors with non-panicking `get` companions, and the always-on
   precondition asserts of the approved value functions (see `docs/panics.md`).
   Full policy: DESIGN_RATIONALE.md [§dd-dr:panic-policy] rule 3.
   New exceptions need explicit user approval. The user-facing exhaustive list of
   panicking public items is the guide chapter `docs/panics.md`
   (`techy::guide::panics`) — any change to documented panicking behavior updates it.
5. **Always check the naming principles** (dev-docs/ARCHITECTURE.md [§dd-arch:naming]) before suggesting names
6. **Prefer existing patterns** from dev-docs/ARCHITECTURE.md and dev-docs/DESIGN_RATIONALE.md. (Older strategy documents live in `dev-docs/archive/` and are no longer authoritative.  Do not read them unless authorized to do so by the user.)
7. **Document learnings from interactive design decision sessions**: After a discussion about a design decision with the user, record the important points, issues, examples, and non-obvious pitfalls that were considered or that appeared in the discussion as a labeled entry in dev-docs/DESIGN_RATIONALE.md (follow its entry template and maintenance rules — including adding an ARCHITECTURE reference for the new entry).
8. **Add tests** for new functionality
9. **Keep it simple**: No over-engineering or premature optimization
10. Use US English spelling and language standards.
