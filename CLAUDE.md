# Quick Start Guide for Claude

## Project Overview

**techy** is a Rust port of Python's `pylatexenc` library for parsing LaTeX-like markup languages. It builds an Abstract Syntax Tree (AST) from LaTeX source code.

Original python project is at: https://github.com/phfaist/pylatexenc

## Key Architecture

```
token → constructs → node (AST)
```

- **source**: Source location tracking (Source, SourceLocation, SourceLocationDetails)
- **token**: Tokenization (Token, TokenType, TokenReader)
- **constructs**: Parsers for individual constructs (Parser trait + implementations)
- **parser**: High-level API (`Parser` struct - the main entry point)
- **node**: AST types (Node, NodeList, CharsNode, MacroNode, etc.)
- **spec**: Extensibility (MacroSpec, EnvironmentSpec, ContextDb)
- **state**: Parsing context (ParsingState, ParsingStateDelta)
- **error**: Error handling (ParseError, Result)

## Critical Naming Conventions

**Read [NAMING_STRATEGY.md](NAMING_STRATEGY.md) first!**

Key naming rules:
- **No "Latex" prefixes**: Use `Token` not `LatexToken`, `Parser` not `LatexWalker`
- **Specificity matters**: `ParsingStateDelta` not `StateDelta` (too vague)
- **Clarity over brevity**: `ArgumentStructureSpec` not `ArgumentsSpec` (distinguishes from `ArgumentSpec`)
- **Context determines names**: `Arguments` not `ParsedArguments` (context makes "parsed" obvious)

**Module organization**:
- `parser` module = high-level public API (`Parser` struct)
- `constructs` module = parsers for individual LaTeX constructs (traits, parsers)
- Node names keep simple forms: `MacroNode`, `EnvironmentNode` (already generic enough)


## Development Workflow

```bash
cargo build          # Build
cargo test           # Run tests (39/40 passing)
cargo test -- --nocapture  # With output
cargo test <name>    # Specific test
```

## Important Files

- [ARCHITECTURE_PLAN.md] - Plan for how to organize and continue this project.  To be executed [as of July 2026].
- [DESIGN_RATIONALE.md] - Living log of decisions and rationales, to keep the code base consistent and to guide future design decisions.

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
4. **Use Result<T,E>** consistently, never panic in lib code
5. **Always check naming strategy** before suggesting names
6. **Prefer existing patterns** from ARCHITECTURE_PLAN.md, NAMING_STRATEGY.md and DESIGN_RATIONALE.md. (Older strategy documents live in docs/archive/ and are no longer authoritative.  Do not read them unless authorized to do so by the user.)
7. **Add tests** for new functionality
8. **Keep it simple**: No over-engineering or premature optimization
9. **Document public APIs** with examples

