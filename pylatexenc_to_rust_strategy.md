# PyLaTeXenc to Rust: Port Strategy & Architecture Analysis

## Executive Summary

The pylatexenc library is a sophisticated LaTeX parser that has evolved through 3 major versions. Your focus modules (`latexnodes`, `macrospec`, `latexwalker`) form the core parsing engine. The library architecture is well-suited for a Rust port, as it emphasizes type safety, clear separation of concerns, and extensibility through trait-like patterns.

## Current Architecture Overview

### Module Structure (v3.0)

The library consists of three tightly integrated modules:

1. **`latexnodes`** (new in v3.0) - Core data structures and parsing framework
   - `latexnodes.nodes` - Node type definitions (AST)
   - `latexnodes.parsers` - Parser implementations
   - Core types: `LatexNode`, `LatexNodeList`, `ParsingState`, `LatexToken`

2. **`macrospec`** - Specification system for extensibility
   - `MacroSpec`, `EnvironmentSpec`, `SpecialsSpec` - Define construct syntax
   - `LatexContextDb` - Database of known constructs
   - `LatexArgumentsParser` - Argument parsing framework
   - Parsing state delta system for context changes

3. **`latexwalker`** - High-level parsing API (legacy compatibility layer)
   - `LatexWalker` - Main parser entry point
   - Delegates to `latexnodes` parsers internally
   - Maintains backwards compatibility with v1.x and v2.x

### Key Design Patterns

#### 1. **Node-Based AST**
The parser builds an Abstract Syntax Tree using these node types:
- `LatexCharsNode` - Plain text
- `LatexMacroNode` - Macros like `\textbf{...}`
- `LatexEnvironmentNode` - Environments like `\begin{equation}...\end{equation}`
- `LatexGroupNode` - Braced groups `{...}`
- `LatexCommentNode` - Comments `% ...`
- `LatexMathNode` - Math mode `$...$` or `$$...$$`
- `LatexSpecialsNode` - Special characters like `&`, `~`, etc.

Each node stores:
- `pos` / `pos_end` - Source location (Rust: use `Span`)
- `parsing_state` - Context (math mode, etc.)
- `latex_walker` - Reference to parser (Rust: lifetime or Arc)
- Type-specific fields (macro name, arguments, etc.)

#### 2. **Context System**
- `ParsingState` - Tracks current parsing context
  - In math mode?
  - Current LaTeX context (available macros/environments)
  - Can create sub-contexts via deltas
  
- `ParsingStateDelta` - Represents state changes
  - `ParsingStateDeltaEnterMathMode` - Enter math mode
  - `ParsingStateDeltaExtendLatexContextDb` - Add temporary definitions

#### 3. **Extensibility via Specifications**
The `macrospec` module enables users to define custom macros/environments:

```python
# Python example
MacroSpec(
    macroname="mycommand",
    arguments_spec_list="[{",  # Optional arg, then mandatory arg
)

EnvironmentSpec(
    environmentname="myenv",
    arguments_spec_list="[[",  # Two optional args
)
```

#### 4. **Parser Objects Pattern (v3.0)**
Everything is parsed by specialized parser objects:
- `LatexParserBase` - Abstract base
- `LatexGeneralNodesParser` - Parse sequence of nodes
- `LatexSingleNodeParser` - Parse one node
- `LatexMacroCallParser` - Parse macro invocation
- `LatexEnvironmentCallParser` - Parse environment
- `LatexStandardArgumentParser` - Parse standard arguments
- `LatexDelimitedVerbatimParser` - Parse verbatim content

Each parser implements: `parse(latex_walker, token_reader, parsing_state) -> (result, state_delta)`

#### 5. **Token Reader Pattern**
- `LatexTokenReader` - Abstracts token stream
- `LatexToken` types: `char`, `macro`, `begin_environment`, `end_environment`, `mathmode_inline`, `mathmode_display`, `comment`, `brace_open`, `brace_close`, `specials`

---

## Identified Problems with Current Architecture

### 1. **Poor Module Structure**
- In v3.0, node definitions moved from `latexwalker` to `latexnodes.nodes`
- Backward compatibility aliases create confusion
- `latexwalker` is now mostly a thin wrapper
- **Recommendation for Rust**: Start clean with proper module hierarchy

### 2. **Type Confusion**
- `ParsedArguments` vs `ParsedMacroArgs` (legacy)
- `LatexNodeList` vs `list` ambiguity
- Multiple ways to represent the same thing for backwards compat
- **Recommendation for Rust**: Use strong typing from the start

### 3. **Mutable State Everywhere**
- Nodes hold references to the walker
- Walker holds the string being parsed
- Parsing state gets mutated and passed around
- **Recommendation for Rust**: Use immutable structures + ownership

### 4. **Reference Management**
- Circular references (nodes -> walker -> nodes)
- No clear lifetime management
- **Recommendation for Rust**: This is where Rust shines - use lifetimes properly

### 5. **Error Handling**
- Mix of exceptions: `LatexWalkerError`, `LatexWalkerParseError`, `LatexWalkerEndOfStream`
- Some methods return `None` on error, others throw
- **Recommendation for Rust**: Use `Result<T, E>` consistently

---

## Proposed Rust Architecture

### Module Structure

```
techy/                   # Your Rust crate
├── src/
│   ├── lib.rs           # Public API
│   ├── token/           # Tokenization
│   │   ├── mod.rs
│   │   ├── token.rs     # LatexToken enum
│   │   └── reader.rs    # TokenReader trait + impls
│   ├── node/            # AST nodes
│   │   ├── mod.rs
│   │   ├── base.rs      # LatexNode trait
│   │   ├── types.rs     # Concrete node types
│   │   └── visitor.rs   # Visitor pattern for traversal
│   ├── parser/          # Parsing logic
│   │   ├── mod.rs
│   │   ├── base.rs      # Parser trait
│   │   ├── general.rs   # GeneralNodesParser
│   │   ├── macro.rs     # MacroCallParser
│   │   ├── env.rs       # EnvironmentCallParser
│   │   └── args.rs      # Argument parsers
│   ├── spec/            # Specifications (macrospec)
│   │   ├── mod.rs
│   │   ├── macro_spec.rs
│   │   ├── env_spec.rs
│   │   ├── special_spec.rs
│   │   └── context.rs   # LatexContextDb
│   ├── state/           # Parsing state
│   │   ├── mod.rs
│   │   ├── parsing_state.rs
│   │   └── delta.rs     # State changes
│   ├── walker/          # High-level API
│   │   └── mod.rs       # LatexWalker
│   └── error.rs         # Error types
```

### Core Type Design

#### 1. **Token System**

```rust
use std::ops::Range;

/// Represents a position in the source text
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Token types in LaTeX
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    Char(String),              // Regular characters
    Macro(String),             // \command
    BeginEnvironment(String),  // \begin{name}
    EndEnvironment(String),    // \end{name}
    MathModeInline,            // $ or \( \)
    MathModeDisplay,           // $$ or \[ \]
    Comment(String),           // % comment
    BraceOpen,                 // {
    BraceClose,                // }
    Specials(String),          // &, ~, etc.
}

/// A token with source location
#[derive(Debug, Clone)]
pub struct LatexToken {
    pub token_type: TokenType,
    pub span: Span,
    pub pre_space: String,   // Whitespace before token
}

/// Trait for reading tokens from a source
pub trait TokenReader {
    fn peek_token(&mut self) -> Result<Option<LatexToken>>;
    fn next_token(&mut self) -> Result<Option<LatexToken>>;
    fn position(&self) -> usize;
}
```

#### 2. **Node System**

```rust
/// Base trait for all LaTeX nodes
pub trait LatexNode {
    fn span(&self) -> Span;
    fn accept<V: NodeVisitor>(&self, visitor: &mut V) -> V::Result;
    
    // Utility methods
    fn latex_verbatim<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span().start..self.span().end]
    }
}

/// Specific node types
#[derive(Debug, Clone)]
pub enum Node {
    Chars(CharsNode),
    Group(GroupNode),
    Macro(MacroNode),
    Environment(EnvironmentNode),
    Comment(CommentNode),
    Math(MathNode),
    Specials(SpecialsNode),
}

#[derive(Debug, Clone)]
pub struct CharsNode {
    pub span: Span,
    pub chars: String,
}

#[derive(Debug, Clone)]
pub struct MacroNode {
    pub span: Span,
    pub name: String,
    pub spec: Option<Arc<MacroSpec>>,  // Shared reference
    pub args: ParsedArguments,
    pub post_space: String,
}

#[derive(Debug, Clone)]
pub struct EnvironmentNode {
    pub span: Span,
    pub name: String,
    pub spec: Option<Arc<EnvironmentSpec>>,
    pub args: ParsedArguments,
    pub body: NodeList,
}

/// A list of nodes (replaces Python's LatexNodeList)
#[derive(Debug, Clone)]
pub struct NodeList {
    pub nodes: Vec<Node>,
    pub span: Span,
}

/// Parsed arguments for macros/environments
#[derive(Debug, Clone)]
pub struct ParsedArguments {
    pub args: Vec<(String, Option<Node>)>,  // (name, value)
    pub span: Span,
}
```

#### 3. **Parsing State**

```rust
/// Parsing context
#[derive(Clone)]
pub struct ParsingState<'ctx> {
    /// Is this in math mode?
    pub in_math_mode: bool,
    
    /// Available LaTeX definitions
    pub latex_context: &'ctx LatexContextDb,
    
    /// Parent state (for scoping)
    parent: Option<Box<ParsingState<'ctx>>>,
}

impl<'ctx> ParsingState<'ctx> {
    /// Create a new parsing state
    pub fn new(context: &'ctx LatexContextDb) -> Self {
        Self {
            in_math_mode: false,
            latex_context: context,
            parent: None,
        }
    }
    
    /// Create a sub-state with modifications
    pub fn sub_state(&self) -> Self {
        // Copy current state for sub-context
        self.clone()
    }
    
    /// Apply a delta to create new state
    pub fn apply_delta(&self, delta: StateDelta) -> Self {
        let mut new_state = self.clone();
        match delta {
            StateDelta::EnterMathMode => {
                new_state.in_math_mode = true;
            }
            StateDelta::ExitMathMode => {
                new_state.in_math_mode = false;
            }
            // ... other deltas
        }
        new_state
    }
}

/// State changes
#[derive(Debug, Clone)]
pub enum StateDelta {
    EnterMathMode,
    ExitMathMode,
    ExtendContext { /* ... */ },
}
```

#### 4. **Specification System**

```rust
use std::sync::Arc;

/// Specification for a macro
#[derive(Debug, Clone)]
pub struct MacroSpec {
    pub name: String,
    pub args_spec: ArgumentsSpec,
}

/// Specification for an environment
#[derive(Debug, Clone)]
pub struct EnvironmentSpec {
    pub name: String,
    pub args_spec: ArgumentsSpec,
    pub body_parser: BodyParserSpec,
}

/// Specification for special characters
#[derive(Debug, Clone)]
pub struct SpecialsSpec {
    pub chars: String,
    pub args_spec: Option<ArgumentsSpec>,
}

/// Argument specification
#[derive(Debug, Clone)]
pub struct ArgumentsSpec {
    pub arguments: Vec<ArgumentSpec>,
}

#[derive(Debug, Clone)]
pub enum ArgumentSpec {
    Optional,         // [...]
    Mandatory,        // {...}
    Verbatim,         // For \verb|...|
    Star,             // * after macro
}

/// Database of known LaTeX constructs
pub struct LatexContextDb {
    macros: HashMap<String, Arc<MacroSpec>>,
    environments: HashMap<String, Arc<EnvironmentSpec>>,
    specials: HashMap<String, Arc<SpecialsSpec>>,
}

impl LatexContextDb {
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            environments: HashMap::new(),
            specials: HashMap::new(),
        }
    }
    
    pub fn add_macro(&mut self, spec: MacroSpec) {
        self.macros.insert(spec.name.clone(), Arc::new(spec));
    }
    
    pub fn get_macro(&self, name: &str) -> Option<&Arc<MacroSpec>> {
        self.macros.get(name)
    }
    
    // Similar for environments and specials...
    
    /// Get default context with standard LaTeX commands
    pub fn default() -> Self {
        let mut db = Self::new();
        
        // Add standard macros
        db.add_macro(MacroSpec {
            name: "textbf".to_string(),
            args_spec: ArgumentsSpec {
                arguments: vec![ArgumentSpec::Mandatory],
            },
        });
        
        // ... many more standard definitions
        
        db
    }
}
```

#### 5. **Parser Trait System**

```rust
pub type ParseResult<T> = Result<(T, Option<StateDelta>), ParseError>;

/// Base parser trait
pub trait Parser {
    type Output;
    
    fn parse<'s, 'ctx>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState<'ctx>,
    ) -> ParseResult<Self::Output>;
}

/// Parser for a sequence of nodes
pub struct GeneralNodesParser {
    pub stop_on_brace: Option<char>,
    pub stop_on_environment: Option<String>,
    pub max_nodes: Option<usize>,
}

impl Parser for GeneralNodesParser {
    type Output = NodeList;
    
    fn parse<'s, 'ctx>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState<'ctx>,
    ) -> ParseResult<NodeList> {
        let mut nodes = Vec::new();
        let start_pos = token_reader.position();
        
        while let Some(token) = token_reader.peek_token()? {
            // Check stop conditions
            if self.should_stop(&token) {
                break;
            }
            
            // Parse based on token type
            let node = match token.token_type {
                TokenType::Char(_) => self.parse_chars(token_reader)?,
                TokenType::Macro(_) => self.parse_macro(source, token_reader, state)?,
                TokenType::BeginEnvironment(_) => self.parse_environment(source, token_reader, state)?,
                // ... other cases
                _ => return Err(ParseError::UnexpectedToken(token)),
            };
            
            nodes.push(node);
            
            if let Some(max) = self.max_nodes {
                if nodes.len() >= max {
                    break;
                }
            }
        }
        
        let end_pos = token_reader.position();
        
        Ok((
            NodeList {
                nodes,
                span: Span { start: start_pos, end: end_pos },
            },
            None,  // No state delta
        ))
    }
}

/// Parser for macro calls
pub struct MacroCallParser {
    pub spec: Arc<MacroSpec>,
}

impl Parser for MacroCallParser {
    type Output = MacroNode;
    
    fn parse<'s, 'ctx>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState<'ctx>,
    ) -> ParseResult<MacroNode> {
        // Parse the macro token
        let token = token_reader.next_token()?
            .ok_or(ParseError::UnexpectedEndOfInput)?;
        
        let name = match token.token_type {
            TokenType::Macro(n) => n,
            _ => return Err(ParseError::ExpectedMacro),
        };
        
        // Parse arguments according to spec
        let args = self.parse_arguments(source, token_reader, state)?;
        
        // Consume trailing whitespace
        let post_space = self.parse_post_space(token_reader)?;
        
        Ok((
            MacroNode {
                span: Span { start: token.span.start, end: token_reader.position() },
                name,
                spec: Some(self.spec.clone()),
                args,
                post_space,
            },
            None,
        ))
    }
}
```

#### 6. **Walker (High-Level API)**

```rust
/// High-level parser interface
pub struct LatexWalker {
    source: String,
    context: LatexContextDb,
}

impl LatexWalker {
    pub fn new(source: String) -> Self {
        Self::with_context(source, LatexContextDb::default())
    }
    
    pub fn with_context(source: String, context: LatexContextDb) -> Self {
        Self { source, context }
    }
    
    /// Parse the entire document
    pub fn parse(&self) -> Result<NodeList, ParseError> {
        let mut token_reader = StringTokenReader::new(&self.source);
        let state = ParsingState::new(&self.context);
        
        let parser = GeneralNodesParser {
            stop_on_brace: None,
            stop_on_environment: None,
            max_nodes: None,
        };
        
        let (nodelist, _delta) = parser.parse(&self.source, &mut token_reader, &state)?;
        Ok(nodelist)
    }
    
    /// Parse a single expression (for arguments, etc.)
    pub fn parse_expression(
        &self,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState,
    ) -> Result<Node, ParseError> {
        let parser = SingleNodeParser;
        let (node, _delta) = parser.parse(&self.source, token_reader, state)?;
        Ok(node)
    }
}
```

#### 7. **Error Handling**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("unexpected end of input")]
    UnexpectedEndOfInput,
    
    #[error("unexpected token: {0:?}")]
    UnexpectedToken(LatexToken),
    
    #[error("expected macro, found {0:?}")]
    ExpectedMacro,
    
    #[error("unknown macro: {0}")]
    UnknownMacro(String),
    
    #[error("unknown environment: {0}")]
    UnknownEnvironment(String),
    
    #[error("unmatched environment: expected \\end{{{expected}}}, found \\end{{{found}}}")]
    UnmatchedEnvironment {
        expected: String,
        found: String,
    },
    
    #[error("unmatched brace at position {0}")]
    UnmatchedBrace(usize),
    
    #[error("invalid argument specification: {0}")]
    InvalidArgumentSpec(String),
}

pub type Result<T> = std::result::Result<T, ParseError>;
```

---

## Key Rust Improvements Over Python

### 1. **Type Safety**
- No more `isinstance()` checks - use enums
- No `Optional[X]` confusion - use `Option<T>`
- No mutable default arguments - use builder pattern

### 2. **Memory Safety**
- Lifetimes prevent dangling references
- `Arc<T>` for shared specs (instead of Python's reference counting)
- No circular references possible

### 3. **Performance**
- Zero-cost abstractions
- No GIL (if you want parallel parsing later)
- Stack allocation for most structures
- `&str` slices instead of copying strings

### 4. **Pattern Matching**
- Match on token types
- Match on node types
- Exhaustive checking (compiler ensures you handle all cases)

### 5. **Error Handling**
- `Result<T, E>` instead of exceptions
- `?` operator for propagation
- No silent failures

---

## Migration Strategy

### Phase 1: Core Infrastructure (Weeks 1-2)
1. ✅ Define token types and `TokenReader` trait
2. ✅ Implement basic tokenizer
3. ✅ Define node types
4. ✅ Implement error types
5. ✅ Write comprehensive unit tests for tokenization

### Phase 2: Basic Parsing (Weeks 3-4)
1. ✅ Implement `ParsingState`
2. ✅ Create `GeneralNodesParser` for basic parsing
3. ✅ Handle chars, groups, comments
4. ✅ Parse simple macros (no arguments)
5. ✅ Test against real LaTeX samples

### Phase 3: Specification System (Weeks 5-6)
1. ✅ Implement `MacroSpec`, `EnvironmentSpec`, `SpecialsSpec`
2. ✅ Create `LatexContextDb`
3. ✅ Add default LaTeX definitions
4. ✅ Implement argument parsing
5. ✅ Test macro argument variations

### Phase 4: Advanced Features (Weeks 7-8)
1. ✅ Math mode handling
2. ✅ Environment parsing
3. ✅ Verbatim content (verb, lstlisting, etc.)
4. ✅ State deltas and context extension
5. ✅ Test complex documents

### Phase 5: High-Level API (Week 9)
1. ✅ Implement `LatexWalker`
2. ✅ Visitor pattern for tree traversal
3. ✅ Helper methods for common operations
4. ✅ Documentation and examples

### Phase 6: Polish & Testing (Week 10)
1. ✅ Performance benchmarks
2. ✅ Fuzzing
3. ✅ Documentation
4. ✅ Examples and cookbook
5. ✅ Publish as crate

---

## Extensibility Design

### User-Defined Macros

```rust
// User code
let mut context = LatexContextDb::default();

context.add_macro(MacroSpec {
    name: "mycommand".to_string(),
    args_spec: ArgumentsSpec {
        arguments: vec![
            ArgumentSpec::Optional,   // [...]
            ArgumentSpec::Mandatory,  // {...}
        ],
    },
});

let walker = LatexWalker::with_context(source, context);
let ast = walker.parse()?;
```

### Custom Parsers

```rust
// User implements custom parser for special syntax
struct MyCustomParser;

impl Parser for MyCustomParser {
    type Output = Node;
    
    fn parse<'s, 'ctx>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &ParsingState<'ctx>,
    ) -> ParseResult<Node> {
        // Custom parsing logic
        todo!()
    }
}
```

### AST Traversal

```rust
// Visitor pattern for processing AST
trait NodeVisitor {
    type Result;
    
    fn visit_chars(&mut self, node: &CharsNode) -> Self::Result;
    fn visit_macro(&mut self, node: &MacroNode) -> Self::Result;
    fn visit_environment(&mut self, node: &EnvironmentNode) -> Self::Result;
    // ... other node types
}

// Example: Extract all macro names
struct MacroCollector {
    names: Vec<String>,
}

impl NodeVisitor for MacroCollector {
    type Result = ();
    
    fn visit_macro(&mut self, node: &MacroNode) -> Self::Result {
        self.names.push(node.name.clone());
        // Recursively visit arguments
        for arg in &node.args.args {
            if let Some(n) = &arg.1 {
                n.accept(self);
            }
        }
    }
    
    // Default implementations for other types...
}
```

---

## Testing Strategy

### Unit Tests
- Each parser component separately
- Token reader implementations
- State management
- Error conditions

### Integration Tests
- Complete documents
- Real LaTeX files from papers
- Edge cases (nested environments, etc.)

### Fuzz Testing
- Use `cargo-fuzz` to find edge cases
- Generate random LaTeX-like input
- Ensure no panics

### Regression Tests
- Port pylatexenc's test suite
- Add Rust-specific tests

### Performance Benchmarks
- Compare with Python version
- Measure allocation patterns
- Profile hot paths

---

## Open Questions & Decisions

1. **Should we support streaming/incremental parsing?**
   - Python version parses full string
   - Rust could support `Read` trait for large files
   - Decision: Start with string, add streaming later

2. **How to handle backwards compatibility?**
   - Don't need it - clean slate
   - But document migration from pylatexenc

3. **Should we support Python bindings (PyO3)?**
   - Would allow gradual migration
   - Adds complexity
   - Decision: Defer until core is stable

4. **Arena allocation for nodes?**
   - Could reduce allocations
   - Adds lifetime complexity
   - Decision: Profile first, optimize if needed

5. **Async parsing?**
   - Not needed for initial version
   - Could add later for parallel processing

---

## Success Criteria

The Rust port should:
- ✅ Parse all LaTeX constructs that pylatexenc handles
- ✅ Provide equivalent extensibility (custom macros/envs)
- ✅ Be type-safe (no runtime type errors)
- ✅ Be memory-safe (no segfaults, no leaks)
- ✅ Be faster than Python version (goal: 5-10x)
- ✅ Have comprehensive documentation
- ✅ Have >90% test coverage
- ✅ Be easy to use for common cases

---

## Future Architectural Considerations

The following architectural proposals are still under discussion and have been moved to [PROPOSALS.md](PROPOSALS.md) for detailed consideration:

1. **Library System Design**: Replacement for `LatexContextDb` with modular libraries, mode-aware definitions, and conflict resolution
2. **Source Tracking & Provenance**: Rich source location tracking beyond byte spans (files, URLs, synthetic sources)
3. **Extensibility**: Generic nodes and custom state for language bindings
4. **TeX Compliance Gap Analysis**: What features are missing for full TeX compliance, and which are intentional limitations

See [PROPOSALS.md](PROPOSALS.md) for detailed designs and discussion.

---

## Conclusion

The pylatexenc architecture translates very naturally to Rust:
- Token-based parsing → Rust enums and pattern matching
- Node trees → Rust structs and enums
- Extensibility → Traits and generics
- Context management → Lifetimes and borrowing

The main challenge is managing lifetimes for the source string and context, but this is exactly where Rust's ownership system shines. The resulting library will be faster, safer, and more maintainable than the Python original.

**Recommended Starting Point**: Begin with token.rs and reader.rs, write thorough tests, then build up the node types and parsers incrementally. The modular design means you can test each component in isolation before integrating.
