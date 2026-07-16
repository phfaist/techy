# Development Guide

This document provides guidance for developing and extending the techy library.

## Project Structure

```
techy/
├── src/
│   ├── lib.rs              # Main library entry point
│   ├── error.rs            # Error types
│   ├── token/              # Tokenization
│   │   ├── mod.rs          # Token types and traits
│   │   └── reader.rs       # StringTokenReader implementation
│   ├── node/               # AST nodes
│   │   └── mod.rs          # Node type definitions
│   ├── spec/               # Specifications
│   │   └── mod.rs          # MacroSpec, EnvironmentSpec, etc.
│   ├── state/              # Parsing state
│   │   └── mod.rs          # ParsingState and ParsingStateDelta
│   ├── parser/             # High-level parsing API
│   │   └── mod.rs          # Parser struct (main entry point)
│   └── constructs/         # Parsers for individual constructs
│       ├── mod.rs          # Parser trait and base implementations
│       └── general.rs      # GeneralNodesParser
├── examples/               # Usage examples
├── tests/                  # Integration tests
└── benches/                # Performance benchmarks
```

## Development Workflow

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Check without building
cargo check
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests for specific module
cargo test token::
```

### Documentation

```bash
# Build and open docs
cargo doc --open

# Check doc examples
cargo test --doc
```

## Implementation Roadmap

### Phase 1: Core Infrastructure ✅

- [x] Token types
- [x] TokenReader trait
- [x] StringTokenReader implementation
- [x] Basic error types
- [x] Node types
- [x] Span tracking

### Phase 2: Basic Parsing ✅

- [x] GeneralNodesParser
- [x] Character parsing
- [x] Group parsing
- [x] Comment parsing
- [x] Basic macro recognition

### Phase 3: Specification System ✅

- [x] MacroSpec
- [x] EnvironmentSpec
- [x] SpecialsSpec
- [x] ContextDb
- [x] Default LaTeX definitions

### Phase 4: Argument Parsing ⏳

- [ ] ArgumentParser trait
- [ ] Mandatory argument parser `{...}`
- [ ] Optional argument parser `[...]`
- [ ] Star argument parser `*`
- [ ] Verbatim argument parser
- [ ] Named arguments in ParsedArguments

### Phase 5: Advanced Features ⏳

- [ ] Environment body parsing
- [ ] Math mode tracking
- [ ] Math delimiter handling (`$`, `$$`, `\(`, `\)`, `\[`, `\]`)
- [ ] State delta application
- [ ] Context extension

### Phase 6: Special Cases ⏳

- [ ] Verbatim environments (`\begin{verbatim}...\end{verbatim}`)
- [ ] Verb-like macros (`\verb|...|`)
- [ ] Custom delimiter handling
- [ ] Nested environment validation

### Phase 7: Polish 📅

- [ ] Comprehensive test suite
- [ ] Performance benchmarks
- [ ] Documentation
- [ ] Examples cookbook
- [ ] Error message improvements

## Adding a New Feature

### 1. Add Node Type (if needed)

If you're adding support for a new construct, you may need a new node type:

```rust
// In src/node/mod.rs
#[derive(Debug, Clone, PartialEq)]
pub struct MyNewNode {
    pub span: Span,
    // ... fields
}

// Add to Node enum
pub enum Node {
    // ... existing variants
    MyNew(MyNewNode),
}
```

### 2. Add Token Type (if needed)

If you need to recognize a new token:

```rust
// In src/token/mod.rs
pub enum TokenType {
    // ... existing variants
    MyNewToken(String),
}

// Update reader in src/token/reader.rs
```

### 3. Add Parser

Create a new parser or extend existing ones:

```rust
// In src/constructs/mynew.rs
pub struct MyNewParser;

impl Parser for MyNewParser {
    type Output = MyNewNode;

    fn parse<'ctx>(
        &self,
        source: &str,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState<'ctx>,
    ) -> ParseResult<Self::Output> {
        // Implementation
        todo!()
    }
}
```

### 4. Add Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_my_new_feature() {
        // Test implementation
    }
}
```

### 5. Update Documentation

- Add doc comments to public items
- Update README if it's a major feature
- Add example if helpful

## Testing Guidelines

### Unit Tests

- Test each component in isolation
- Place tests in the same file as the code (`#[cfg(test)] mod tests`)
- Use descriptive test names: `test_<what>_<condition>_<expected>`

### Integration Tests

- Place in `tests/` directory
- Test complete workflows
- Use realistic LaTeX samples

### Property-Based Tests

Use `proptest` for testing properties:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_parse_doesnt_crash(s in "\\PC*") {
        let walker = Parser::new(s);
        let _ = walker.parse(); // Shouldn't panic
    }
}
```

## Common Pitfalls

### 1. Lifetime Issues

The parsing state holds a reference to the context with lifetime `'ctx`:

```rust
// ✓ Correct
fn parse<'ctx>(&self, state: &ParsingState<'ctx>) -> ParseResult<Node> {
    // ...
}

// ✗ Wrong - missing lifetime
fn parse(&self, state: &ParsingState) -> ParseResult<Node> {
    // ...
}
```

### 2. Borrowing in TokenReader

Remember that `peek_token()` needs `&mut self`:

```rust
// ✓ Correct
let token = token_reader.peek_token()?;
if should_stop(&token) { ... }

// ✗ Wrong - can't borrow as immutable after mutable borrow
let token = token_reader.peek_token()?;
let other = token_reader.peek_token()?; // Error!
```

### 3. Error Handling

Always use `?` for propagation, don't unwrap:

```rust
// ✓ Correct
let token = token_reader.next_token()?
    .ok_or_else(|| ParseError::UnexpectedEndOfInput(pos))?;

// ✗ Wrong - panic on error
let token = token_reader.next_token().unwrap().unwrap();
```

## Performance Considerations

### 1. Avoid Unnecessary Cloning

Use references where possible:

```rust
// ✓ Good
fn process_node(node: &Node) { ... }

// ✗ Wasteful
fn process_node(node: Node) { ... }
```

### 2. Use String Slices

Don't copy strings unnecessarily:

```rust
// ✓ Good
let text = &source[span.start..span.end];

// ✗ Wasteful
let text = source[span.start..span.end].to_string();
```

### 3. Pre-allocate Collections

If you know the size:

```rust
// ✓ Good
let mut nodes = Vec::with_capacity(expected_size);

// ✗ Less efficient
let mut nodes = Vec::new();
```

## Code Style

Follow Rust conventions:

- Use `rustfmt`: `cargo fmt`
- Use `clippy`: `cargo clippy`
- Document public items
- Write examples in doc comments
- Keep functions focused and small
- Prefer iterators over loops where clear

## Debugging Tips

### 1. Enable Logging

Add logging to track parser state:

```rust
#[cfg(debug_assertions)]
eprintln!("Parsing token at pos {}: {:?}", pos, token);
```

### 2. Print AST

Use the Debug trait:

```rust
println!("{:#?}", ast);
```

### 3. Test Incrementally

Build complex tests from simple ones:

```rust
#[test]
fn test_simple() { /* ... */ }

#[test]
fn test_nested() { /* ... */ }

#[test]
fn test_complex() { /* combines above */ }
```

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [pylatexenc docs](https://pylatexenc.readthedocs.io/)
- [LaTeX reference](https://www.latex-project.org/help/documentation/)

## Getting Help

- Check existing tests for examples
- Read the pylatexenc source for reference
- Open an issue for questions
- Join Rust community forums
