# techy

A fast, extensible parser for a LaTeX-like markup language.

## Overview

`techy` is a Rust library for parsing LaTeX-like markup languages. It builds an Abstract Syntax Tree (AST) from LaTeX source code, allowing you to analyze, transform, or convert LaTeX documents.

This is loosely a Rust port of the Python [pylatexenc](https://github.com/phfaist/pylatexenc) library, focusing on the `latexnodes`, `macrospec`, and `latexwalker` modules.

**For AI agents:** → Read `docs/ai-guide.md`, a guide optimized for AI agents.
(Humans with a penchant for densely packed, condensed details may read this, too.)

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

Parse LaTeX-like input with the built-in `latexlike` preset:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("inline $x+y$ math").unwrap();
let math = result.tree.root().child(1).unwrap();
assert!(math.is_math_group());
```

The narrative guide (chapters under `techy::guide` in the generated
documentation) walks through parsing, defining macros and environments, math
modes, verbatim, error recovery, and content extraction.

## Architecture

The public API is exported exclusively through facade modules — exactly one
canonical public path per item, placed by role: data models and consumer tool
libraries at the top level, the machinery in `techy::core`, the preset in
`techy::latexlike`:

- **`techy::source`**: source content, byte spans, provenance, pluggable
  resolution, lazy line/column
- **`techy::error`**: span-based structured diagnostics, tolerant parsing policy
- **`techy::extract`**: content-extraction helpers over parsed node trees
- **`techy::visit`**, **`techy::transform`**, **`techy::recompose`**: read-only
  traversal, tree-to-tree transformation, and tree-to-value recomposition of
  parsed node trees
- **`techy::serialize`**: parsed trees, parsing states, definitions and whole
  parse results to and from a format-independent value model
- **`techy::core`**: the flat machinery hub — the `Lang` trait and immutable
  parsing state, and the parse engine (`Language` + `parse()`, drivers, sessions,
  results) — with four satellites:
  - **`techy::core::token`**: the tokenization library — zero-copy tokens, the
    token reader, and data-driven tokenization rules
  - **`techy::core::specs`**: defining callables — callable specs and argument
    structures, definition packages, the scope stack, command resolution
  - **`techy::core::constructs`**: the construct parsers and the content
    dispatch loop
  - **`techy::core::node`**: the flat, frozen node tree — reading, payloads,
    building
- **`techy::latexlike`**: the familiar LaTeX behavior as a preset

Internally the crate is organized in three strata (a `Lang`-free foundation, one
mutually recursive core, the presets); that file layout is private and never
shows in public paths.

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

## License

MIT License - see LICENSE file for details.

## References

- [pylatexenc](https://github.com/phfaist/pylatexenc) - The original Python library
