# techy

A fast, extensible LaTeX parser for Rust.

## Overview

`techy` is a Rust library for parsing LaTeX-like markup languages. It builds an Abstract Syntax Tree (AST) from LaTeX source code, allowing you to analyze, transform, or convert LaTeX documents.

This is a Rust port of the Python [pylatexenc](https://github.com/phfaist/pylatexenc) library, focusing on the `latexnodes`, `macrospec`, and `latexwalker` modules.

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

Then parse some LaTeX:

```rust
use techy::Parser;

let source = r"\textbf{Hello} \emph{world}!";
let walker = Parser::new(source.to_string());
let ast = walker.parse().unwrap();

println!("Parsed {} nodes", ast.nodes.len());
```

## Architecture

The parser follows a three-stage pipeline:

1. **Tokenization** (`token` module): Break source into tokens
2. **Parsing** (`parser` module): Build AST from tokens  
3. **Processing** (`node` module): Traverse and manipulate AST

### Core Concepts

- **Tokens**: Basic lexical units (macros, braces, text, etc.)
- **Nodes**: AST elements representing LaTeX constructs
- **Specs**: Definitions for how to parse macros and environments
- **Context**: Database of known LaTeX constructs
- **State**: Tracks parsing context (math mode, etc.)

## Usage Examples

### Basic Parsing

```rust
use techy::Parser;

let walker = Parser::new(r"\section{Introduction}".to_string());
let ast = walker.parse()?;
```

### Custom Macros

```rust
use techy::{Parser, ContextDb, MacroSpec};

let mut context = ContextDb::default();

// Define a custom macro: \highlight[color]{text}
context.add_macro(MacroSpec::simple("highlight", "[{"));

let walker = Parser::with_context(
    r"\highlight[yellow]{important text}".to_string(),
    context
);

let ast = walker.parse()?;
```

### Traversing the AST

```rust
use techy::{Parser, Node};

let walker = Parser::new(source.to_string());
let ast = walker.parse()?;

for node in &ast.nodes {
    match node {
        Node::Macro(macro_node) => {
            println!("Found macro: \\{}", macro_node.name);
        }
        Node::Chars(chars_node) => {
            println!("Found text: {}", chars_node.chars);
        }
        _ => {}
    }
}
```

## Module Documentation

- **`token`**: Token types and tokenization
- **`node`**: AST node definitions
- **`parser`**: Parser implementations
- **`spec`**: Macro/environment specifications
- **`state`**: Parsing state management
- **`walker`**: High-level parsing API
- **`error`**: Error types

## Development Status

This is a work in progress. Currently implemented:

- ✅ Basic tokenization
- ✅ Text parsing
- ✅ Macro recognition
- ✅ Group parsing (`{...}`)
- ✅ Comment parsing
- ✅ Specification system
- ⏳ Argument parsing (in progress)
- ⏳ Environment parsing (in progress)
- ⏳ Math mode handling (in progress)
- ⏳ Verbatim content (planned)

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

## Contributing

Contributions are welcome! Areas that need work:

- [ ] Complete argument parsing for all argument types
- [ ] Environment body parsing
- [ ] Math mode delimiter handling
- [ ] Verbatim content parsing (`\verb`, `verbatim` environment)
- [ ] More comprehensive test coverage
- [ ] Performance optimizations
- [ ] Documentation improvements

## License

MIT License - see LICENSE file for details.

## References

- [pylatexenc](https://github.com/phfaist/pylatexenc) - The original Python library
- [LaTeX Project](https://www.latex-project.org/) - Official LaTeX documentation

## Acknowledgments

This library is a port of [pylatexenc](https://github.com/phfaist/pylatexenc) by Philippe Faist. The architecture and design patterns are heavily inspired by that excellent library.
