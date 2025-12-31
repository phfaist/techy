# TODO List

## High Priority

- [ ] **Argument Parsing**: Implement full argument parsing based on ArgumentSpec
  - [ ] Create `constructs/args.rs` for argument parsers
  - [ ] Mandatory arguments `{...}`
  - [ ] Optional arguments `[...]`
  - [ ] Star arguments `*`
  - [ ] Multiple arguments
  - [ ] Named argument access

- [ ] **Environment Parsing**: Complete environment support
  - [ ] Create `constructs/env.rs` for environment parser
  - [ ] Parse environment body
  - [ ] Handle environment arguments
  - [ ] Validate begin/end matching
  - [ ] Support nested environments

- [ ] **Math Mode**: Full math mode support
  - [ ] Track math mode state
  - [ ] Handle `$...$` inline math
  - [ ] Handle `$$...$$` display math
  - [ ] Handle `\(...\)` and `\[...\]`
  - [ ] State deltas for entering/exiting math mode

## Medium Priority

- [ ] **Verbatim Content**: Special handling for verbatim text
  - [ ] `\verb|...|` and variants
  - [ ] `\begin{verbatim}...\end{verbatim}`
  - [ ] `\begin{lstlisting}...\end{lstlisting}`
  - [ ] Custom delimiters

- [ ] **Special Characters**: Complete specials support
  - [ ] Parse special character arguments
  - [ ] Handle ligatures (`--`, `---`, `` ` ``, `''`)
  - [ ] Configurable specials

- [ ] **Error Messages**: Improve error reporting
  - [ ] Better error messages with context
  - [ ] Suggestions for common mistakes
  - [ ] Multiple error reporting

- [ ] **Performance**: Optimize hot paths
  - [ ] Profile parser
  - [ ] Reduce allocations
  - [ ] Consider arena allocation for nodes

## Low Priority

- [ ] **Visitor Pattern**: Implement visitor for AST traversal
  - [ ] Define Visitor trait
  - [ ] Implement accept methods
  - [ ] Add convenience visitors (e.g., macro collector)

- [ ] **Pretty Printing**: Convert AST back to LaTeX
  - [ ] Implement Display for nodes
  - [ ] Preserve formatting
  - [ ] Optional reformatting

- [ ] **Advanced Features**:
  - [ ] Streaming parser (for large files)
  - [ ] Incremental parsing
  - [ ] Error recovery
  - [ ] LSP support

- [ ] **Documentation**:
  - [ ] API documentation
  - [ ] Tutorial
  - [ ] Cookbook with examples
  - [ ] Comparison with pylatexenc

## Testing

- [ ] More unit tests for edge cases
- [ ] Property-based testing with proptest
- [ ] Fuzzing with cargo-fuzz
- [ ] Test against real LaTeX documents
- [ ] Benchmark against pylatexenc

## Code Quality

- [ ] Enable all clippy lints
- [ ] Add more doc comments
- [ ] Add examples to doc comments
- [ ] CI/CD setup (GitHub Actions)
- [ ] Code coverage reporting

## Future Ideas

- [ ] Python bindings (PyO3)
- [ ] WASM support
- [ ] Syntax highlighting support
- [ ] LSP server
- [ ] LaTeX formatter
- [ ] LaTeX to Markdown converter
- [ ] LaTeX validator/linter
