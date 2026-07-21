# techy

A fast, extensible parser for a LaTeX-like markup language.

## Overview

`techy` is a Rust library for parsing LaTeX-like markup languages. It builds an Abstract Syntax Tree (AST) from LaTeX source code, allowing you to analyze, transform, or convert LaTeX documents.

This is loosely a Rust port of the Python [pylatexenc](https://github.com/phfaist/pylatexenc) library, focusing on the `latexnodes`, `macrospec`, and `latexwalker` modules.

## Features

- **Fast**: Zero-copy parsing where possible, efficient memory usage
- **Extensible**: Define custom macros, environments, and special characters
- **Type-safe**: Leverages Rust's type system for correctness
- **Flexible**: Support for standard LaTeX and custom LaTeX-like languages
- **Well-tested**: Comprehensive test suite

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
techy = "0.1"
```

(Todo: include example once all design patterns are finalized and prototype code is set up.)

## Architecture

The parser follows a three-stage pipeline:

1. **Tokenization** (`token` module): Break source into tokens
2. **Parsing** (`parser` module): Build AST from tokens  
3. **Processing** (`node` module): Traverse and manipulate AST

### Core Concepts

- **Tokens**: Basic lexical units (macros, braces, text, etc.)
- **Nodes**: AST elements representing LaTeX constructs
- **Specs**: Definitions for how to parse macros and environments
- **Context**: Database of known LaTeX constructs  [NOTE: "Context" Likely to be renamed]
- **State**: Tracks parsing context (math mode, etc.)

## Usage Examples

(TODO, after design decisions finalized and minimal code is set up.)

## Module Documentation

- **`token`**: Token types and tokenization
- **`node`**: AST node definitions
- **`parser`**: Parser implementations
- **`spec`**: Macro/environment specifications
- **`state`**: Parsing state management
- **`walker`**: High-level parsing API
- **`error`**: Error types

To build HTML documentation:

```bash
cargo docs  # alias for 'cargo doc --workspace --no-deps'
```

If you accidentally ran `cargo doc` instead of `cargo docs`, delete `target/doc` once
to drop the stale dependency pages (rustdoc merges new output into what is already
there).


## Development Status

This is a work in progress.

## Differences from pylatexenc

While this library is inspired by pylatexenc, there are some intentional differences:

1. **Clean Architecture**: No backwards compatibility baggage from v1.x/v2.x
2. **Strong Typing**: Leverages Rust's type system for safety
3. **Performance**: Significantly faster due to Rust's zero-cost abstractions
4. **Error Handling**: Uses `Result` instead of exceptions
5. **Memory Safety**: Rust's ownership system prevents common bugs

## Testing

Run the test suite:

```bash
cargo test
```

Run tests with output:

```bash
cargo test -- --nocapture
```

Run a specific test:

```bash
cargo test test_macro_parsing
```

## Performance

Run benchmarks (requires nightly Rust):

```bash
cargo bench
```

## License

MIT License - see LICENSE file for details.

## References

- [pylatexenc](https://github.com/phfaist/pylatexenc) - The original Python library
