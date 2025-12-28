# Quick Start Guide

Welcome to techy! This guide will get you up and running quickly.

## Prerequisites

- Rust 1.70 or later
- Cargo (comes with Rust)

Install Rust from: https://rustup.rs/

## Setup

1. **Extract the ZIP file**:
   ```bash
   unzip techy.zip
   cd techy
   ```

2. **Build the project**:
   ```bash
   cargo build
   ```

3. **Run the tests**:
   ```bash
   cargo test
   ```

4. **Run the example**:
   ```bash
   cargo run --example basic
   ```

## Your First Parser

Create a file `my_parser.rs`:

```rust
use techy::LatexWalker;

fn main() {
    let source = r"\textbf{Hello} world!";
    let walker = LatexWalker::new(source.to_string());
    
    match walker.parse() {
        Ok(ast) => {
            println!("Success! Parsed {} nodes", ast.nodes.len());
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}
```

Run it:
```bash
rustc my_parser.rs -L target/debug/deps --extern techy=target/debug/libtechy.rlib
./my_parser
```

Or add it to `examples/` and run:
```bash
cargo run --example my_parser
```

## Project Structure

```
techy/
├── src/                    # Source code
│   ├── lib.rs             # Library entry point
│   ├── error.rs           # Error types
│   ├── token/             # Tokenization
│   ├── node/              # AST nodes
│   ├── parser/            # Parsing logic
│   ├── spec/              # Macro/env specifications
│   ├── state/             # Parsing state
│   └── walker/            # High-level API
├── examples/              # Usage examples
├── tests/                 # Integration tests
├── README.md              # Main documentation
├── DEVELOPMENT.md         # Development guide
└── TODO.md               # Feature roadmap
```

## Common Tasks

### Run Tests
```bash
cargo test                  # All tests
cargo test -- --nocapture  # With output
cargo test integration     # Integration tests only
```

### Check Code
```bash
cargo check                # Fast compile check
cargo clippy              # Linting
cargo fmt                 # Format code
```

### Build Documentation
```bash
cargo doc --open          # Build and open docs
```

## What Works Now

✅ Basic tokenization
✅ Text parsing  
✅ Macro recognition (no arguments yet)
✅ Group parsing `{...}`
✅ Comment parsing `% ...`
✅ Specification system
✅ Custom macro definitions

## What's Coming Next

⏳ Macro argument parsing
⏳ Environment parsing
⏳ Math mode handling
⏳ Verbatim content

See `TODO.md` for the full roadmap.

## Examples to Try

### 1. Basic Parsing
```bash
cargo run --example basic
```

### 2. Custom Macros
```bash
cargo run --example custom_macros
```

### 3. Your Own Parser

Edit `examples/basic.rs` or create a new file in `examples/`.

## Next Steps

1. **Read the documentation**: `cargo doc --open`
2. **Check out examples**: Look in `examples/` directory
3. **Read DEVELOPMENT.md**: Learn about the architecture
4. **Start contributing**: See TODO.md for ideas

## Need Help?

- Check the examples in `examples/`
- Read `DEVELOPMENT.md` for implementation details
- Look at tests in `tests/` and `src/*/mod.rs`
- Open an issue on GitHub (once you publish)

## Tips

1. **Use `cargo check`** for fast feedback while developing
2. **Run tests frequently** with `cargo test`
3. **Use `--nocapture`** to see println! output in tests
4. **Read error messages carefully** - Rust's errors are helpful!
5. **Start small** - parse simple LaTeX before complex documents

## Troubleshooting

### "cargo: command not found"
Install Rust from https://rustup.rs/

### "error: linker failed"
Make sure you have a C compiler installed:
- Linux: `sudo apt install build-essential`
- macOS: `xcode-select --install`
- Windows: Install Visual Studio C++ tools

### Tests fail
This is a work-in-progress library. Some features aren't implemented yet.
Check TODO.md to see what's done and what's planned.

## Contributing

The library is in active development. Priority areas:

1. Argument parsing (see `src/parser/`)
2. Environment parsing
3. More tests
4. Better error messages

See DEVELOPMENT.md for detailed contribution guidelines.

Happy parsing! 🚀
