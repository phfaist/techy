# techy

A fast, extensible parser for a LaTeX-like markup language.


## Overview

`techy` is a Rust library for parsing LaTeX-like markup languages. It builds an
Abstract Syntax Tree (AST) from LaTeX source code, allowing you to analyze,
transform, or convert LaTeX documents.

This is loosely a Rust port of the Python
[pylatexenc](https://github.com/phfaist/pylatexenc) library, focusing on the
`latexnodes`, `macrospec`, and `latexwalker` modules.

**For AI agents:** → Read
[`docs/ai-guide.md`](https://github.com/phfaist/techy/blob/main/docs/ai-guide.md)
in this repo, a guide optimized for AI agents.  (Humans with a penchant for
densely packed, condensed details may read this, too.)

**Experimental status:** This library is still at an experimental development
stage. The API might still change!

`techy` is NOT a TeX engine. It parses LaTeX-like constructs as a markup language,
yielding content structure.

See also:

- The [`techy-xp`](https://github.com/phfaist/techy-xp) package: `techy-xp`
  plugs into `techy`'s extension mechanism to provide parsing that more closely
  approximates TeX/LaTeX, honoring `\newcommand`, `\def`, and other similar
  commands.
  
- The [`techxt`](https://github.com/phfaist/techxt) package, which provides
  conversion of LaTeX code to Unicode text.  It is a Rust-based redesign of
  pylatexenc's [`latex2text`](https://github.com/phfaist/pylatexenc) based on
  `techy` and `techy-xp`.  You can try [it out
  here](https://phfaist.github.io/techxt/) and even install it as an app on your
  device!
  
- The [`untechxt`](https://github.com/phfaist/untechxt) package provides a way
  to perform the reverse conversion, from Unicode text to LaTeX code.  The
  `untechxt` package is a Rust-based redesign of pylatexenc's
  [`latexencode`](https://github.com/phfaist/pylatexenc).  The `untechxt`
  package does not depend on `techy` since it operates in the other direction
  from Unicode to LaTeX; it is only closely related.


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


## Documentation

Build the HTML documentation with:

```bash
cargo docs  # alias for 'cargo doc --workspace --no-deps'
```

If you accidentally ran `cargo doc` instead of `cargo docs`, delete `target/doc` once
to drop the stale dependency pages (rustdoc merges new output into what is already
there).

The documentation contains a series of **guides** linked with the rustdoc API
that aim to help you out in using the library.  Having trouble compiling the
docs with Cargo?  The guides can also be read as plain Markdown files in this
repo's `docs/` folder.


## Relation with pylatexenc

This library is a Rust-based redesigned version of pylatexenc's LaTeX-like
parser. We did not attempt to provide backwards compatibility with pylatexenc's
API, allowing flexibility for a better parser design and API.

Rust's strong typing and memory ownership model enable faster, safer code.  We've
found a speedup of about ~20x on some quick internal benchmarks between the
Rust version and pylatexenc.

See also the *Migrating from pylatexenc* guide in the compiled documentation
(`cargo docs`), also to be found in this repo as `docs/pylatexenc-migration.md`.

Some level of Python bindings are potentially planned.  However, given the
general, versatile nature of `techy` and its many extension knobs, we currently
recommend you write the core logic where you need `techy` in *Rust*; this allows
you to make use of `techy`'s native features, all while exposing bindings at a
higher level for your project in Python (or any other language).


## Testing

Run the test suite:

```bash
cargo test
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.


## References

- [pylatexenc](https://github.com/phfaist/pylatexenc) - The original Python library
