# Quick Start Guide for Claude

## Project Overview

**techy** is a Rust rewrite of Python's `pylatexenc` library for parsing LaTeX-like markup languages. It builds an Abstract Syntax Tree (AST) from LaTeX source code.

Original python project is at: https://github.com/phfaist/pylatexenc

## Key Architecture

```
token → constructs → node (AST)
```

- **source**: Source model (Source, SourceSpan, Span, SourceProvenance, LineIndex, TextContent)
- **token**: Tokenization (Token, TokenKind, TokenRules, TokenReader, StdTokenReader)
- **constructs**: Parsers for individual constructs (ConstructParser trait + standard parsers)
- **engine**: High-level machinery (ParserSession, ParseResult; `Language::parse()` arrives Phase 7)
- **node**: AST storage (NodeTree, NodeKind, NodeRef, GroupData, CallableData)
- **spec** + **library**: Extensibility (CallableSpec, StdCallableSpec, ArgumentSpec, ArgumentParser; Library, LibraryStack)
- **state**: Parsing context (Lang, ParsingState, ParsingStateDelta)
- **error**: Diagnostics (Diagnostic, Diagnostics, ParseError, Severity, Recovery)

## Critical Naming Conventions

**Read [NAMING_STRATEGY.md](NAMING_STRATEGY.md) first!**

Key naming rules:
- **No "Latex" prefixes**: Use `Token` not `LatexToken`, `Parser` not `LatexWalker`
- **Specificity matters**: `ParsingStateDelta` not `StateDelta` (too vague)
- **Clarity over brevity**: `ParsedArguments` not `Arguments` (the spec-side `ArgumentSpec`/`ArgumentParser` vocabulary coexists in scope — revised July 2026)
- **Context determines names** — but only when no sibling vocabulary competes in the same scope (see NAMING_STRATEGY.md principles 3–4)

**Module organization**:
- `engine` module = high-level machinery (`ParserSession`; the public `Language::parse()` entry arrives Phase 7)
- `constructs` module = parsers for individual constructs (traits, parsers)
- Node taxonomy is the closed `NodeKind`: `Chars`/`Group`/`Callable`/`Comment`/`List` — "macro"/"environment" are preset vocabulary, not node kinds


## Development Workflow

```bash
cargo build          # Build
cargo test           # Run tests
cargo test -- --nocapture  # With output
cargo test <name>    # Specific test
```

## Important Files

- [ARCHITECTURE.md] - Plan for how to organize and continue this project.  To be executed [as of July 2026].
- [DESIGN_RATIONALE.md] - Living log of decisions and rationales, to keep the code base consistent and to guide future design decisions.

- [Phase6Execution.md] - Detailed plan and recorded progress during execution of the sub-phases of Phase 6 of our Architecture plan.

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
   unreachable invariants (`unreachable!`/`expect` with the invariant stated), plus the
   explicitly approved indexing-style accessors that have non-panicking `get` companions.
   Full policy: DESIGN_RATIONALE.md §3.8 ("Panic policy"). New exceptions need explicit
   user approval.
5. **Always check naming strategy** before suggesting names
6. **Prefer existing patterns** from ARCHITECTURE.md, NAMING_STRATEGY.md and DESIGN_RATIONALE.md. (Older strategy documents live in `docs/archive/` and are no longer authoritative.  Do not read them unless authorized to do so by the user.)
7. **Document learnings from interactive design decision sessions**: After a discussion about a design decision with the user, record the important points, issues, examples, and non-obvious pitfalls that were considered or that appeared in the discussion with a concise paragraph in DESIGN_RATIONALE.md.
8. **Add tests** for new functionality
9. **Keep it simple**: No over-engineering or premature optimization

