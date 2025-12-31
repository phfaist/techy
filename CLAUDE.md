# Quick Start Guide for Claude

## Project Overview

**techy** is a Rust port of Python's `pylatexenc` library for parsing LaTeX-like markup languages. It builds an Abstract Syntax Tree (AST) from LaTeX source code.

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

## Current Implementation Status

✅ **Complete:**
- Basic tokenization
- Text/chars parsing
- Macro recognition
- Groups `{...}` and comments
- Specification system (MacroSpec, EnvironmentSpec)
- Core naming migration complete

⏳ **In Progress:**
- Argument parsing
- Environment parsing
- Math mode handling

📋 **Planned:**
- Verbatim content parsing

## pylatexenc → Rust Strategy

**Read [pylatexenc_to_rust_strategy.md](pylatexenc_to_rust_strategy.md) for detailed architecture!**

Key improvements over Python:
1. **Type safety**: Enums instead of isinstance() checks
2. **Memory safety**: Lifetimes prevent dangling refs, Arc for shared specs
3. **Performance**: Zero-cost abstractions, stack allocation, &str slices
4. **Error handling**: Result<T,E> instead of exceptions
5. **Pattern matching**: Exhaustive token/node matching

Key design patterns:
- **Node-based AST**: CharsNode, MacroNode, EnvironmentNode, etc.
- **Spec system**: Define custom macros/environments via ContextDb
- **Parser traits**: Everything parsed by specialized parser objects
- **State deltas**: Immutable state transformations
- **Token reader**: Abstraction over token streams

## Quick Reference

### Main Types
```rust
// High-level API
Parser              // Entry point (was LatexWalker)
ContextDb           // Database of macro/env specs (was LatexContextDb)

// Source Location
Source              // Owns source content, lazy line/column computation
SourceLocation<'src> // Lightweight reference to source + byte positions
SourceLocationDetails<'src> // Computed line/column information

// Tokens
Token               // A token
TokenType           // Token variants

// AST Nodes
Node                // Enum of all node types
NodeList            // Vec of nodes + span
CharsNode, MacroNode, EnvironmentNode, GroupNode, etc.

// Specs (extensibility)
MacroSpec           // Define custom macros
EnvironmentSpec     // Define custom environments
ArgumentStructureSpec  // Argument patterns (was ArgumentsSpec)
ArgumentSpec        // Individual argument

// State
ParsingState        // Parsing context
ParsingStateDelta   // State transitions (was StateDelta)

// Parsed results
Arguments           // Parsed macro/env args (was ParsedArguments)
```

### Common Usage
```rust
// Basic parsing
let parser = Parser::new(source.to_string());
let ast = parser.parse()?;

// Custom macros
let mut context = ContextDb::default();
context.add_macro(MacroSpec::simple("highlight", "[{"));
let parser = Parser::with_context(source, context);
```

## Development Workflow

```bash
cargo build          # Build
cargo test           # Run tests (39/40 passing)
cargo test -- --nocapture  # With output
cargo test <name>    # Specific test
```

## Important Files

- [pylatexenc_to_rust_strategy.md](pylatexenc_to_rust_strategy.md) - Complete architecture analysis & migration plan
- [NAMING_STRATEGY.md](NAMING_STRATEGY.md) - Naming conventions & rationale
- [README.md](README.md) - User-facing docs
- [src/lib.rs](src/lib.rs) - Public API exports
- [src/parser/mod.rs](src/parser/mod.rs) - Main Parser struct (high-level API)
- [src/constructs/mod.rs](src/constructs/mod.rs) - Parser trait and construct parsers
- [src/node/mod.rs](src/node/mod.rs) - AST node definitions
- [src/spec/mod.rs](src/spec/mod.rs) - Extensibility system

## Design Philosophy

1. **Clean slate**: No Python backwards compatibility baggage
2. **Rust-first**: Leverage ownership, lifetimes, zero-cost abstractions
3. **Extensibility**: Easy custom macros/environments via specs
4. **Type safety**: Compiler catches errors Python couldn't
5. **Performance**: 5-10x faster than Python target
6. **Generic**: "techy" not "latex" - works for LaTeX-like languages

## When Helping

1. **Always check naming strategy** before suggesting names
2. **Prefer existing patterns** from pylatexenc_to_rust_strategy.md
3. **Use Result<T,E>** consistently, never panic in lib code
4. **Add tests** for new functionality
5. **Keep it simple**: No over-engineering or premature optimization
6. **Document public APIs** with examples

## Future Architectural Considerations

See bottom of [pylatexenc_to_rust_strategy.md](pylatexenc_to_rust_strategy.md) and PROPOSALS.md:
- Library system redesign (replacing ContextDb)
- Source tracking & provenance
- Generic nodes & custom state for language bindings
- TeX compliance gap analysis
