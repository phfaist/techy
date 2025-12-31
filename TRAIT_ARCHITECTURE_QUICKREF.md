# Trait-Based Architecture: Quick Reference

## Naming Conventions

All naming follows **NAMING_STRATEGY.md**:
- `Parser` (not LatexWalker) - main parsing API
- `Token` (not LatexToken) - tokenization
- `ParsingStateDelta` (not StateDelta) - state changes
- `ArgumentStructureSpec` (not ArgumentsSpec) - argument patterns
- `Arguments` (not ParsedArguments) - parsed results
- Module `parser` (high-level API) vs `parsing` (low-level implementation)

**No backward compatibility** - library not yet public.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        User Code Layer                          │
│  (Can extend any trait without modifying library)               │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │
┌─────────────────────────────┼─────────────────────────────────┐
│                             │                                   │
│        Core Trait Layer (Extension Points)                      │
│                             │                                   │
│  ┌──────────┐  ┌──────────┐│┌──────────┐  ┌──────────┐       │
│  │   Node   │  │  State   ││││ Context  │  │ Parsing  │       │
│  │  Trait   │  │  Trait   ││││  Trait   │  │  Trait   │       │
│  └──────────┘  └──────────┘│└──────────┘  └──────────┘       │
│       ▲             ▲       │      ▲            ▲              │
└───────┼─────────────┼───────┼──────┼────────────┼─────────────┘
        │             │       │      │            │
┌───────┼─────────────┼───────┼──────┼────────────┼─────────────┐
│       │             │       │      │            │              │
│  Standard Implementations (Batteries Included)                 │
│       │             │       │      │            │              │
│  ┌────┴────┐   ┌────┴────┐ │ ┌────┴────┐  ┌────┴────┐        │
│  │ Macro   │   │Standard │ │ │Standard │  │ General │        │
│  │ Node    │   │Parsing  │ │ │Context  │  │  Nodes  │        │
│  │ Chars   │   │State    │ │ │         │  │  Parser │        │
│  │ Group   │   │         │ │ │         │  │         │        │
│  │ Env     │   │         │ │ │         │  │         │        │
│  │ ...     │   │         │ │ │         │  │         │        │
│  └─────────┘   └─────────┘ │ └─────────┘  └─────────┘        │
└────────────────────────────┼──────────────────────────────────┘
                             │
┌────────────────────────────┼──────────────────────────────────┐
│                            │                                   │
│         High-Level API (Generic over traits)                   │
│                            │                                   │
│     ┌──────────────────────▼─────────────────────┐            │
│     │  Parser<N, S, C>                           │            │
│     │    where N: Node                           │            │
│     │          S: ParsingState<Context = C>      │            │
│     │          C: Context                        │            │
│     └────────────────────────────────────────────┘            │
│                            │                                   │
│     ┌──────────────────────▼─────────────────────┐            │
│     │  Type Alias for Common Case                │            │
│     │                                             │            │
│     │  StandardParser = Parser<                  │            │
│     │    Box<dyn Node>,                          │            │
│     │    StandardParsingState<StandardContext>,  │            │
│     │    StandardContext                         │            │
│     │  >                                          │            │
│     └─────────────────────────────────────────────┘            │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

## Three Levels of Usage

### Level 1: Simple (90% of users)

```rust
// Just works, no configuration needed
use techy::prelude::*;

let parser = StandardParser::with_standard_context(source);
let ast = parser.parse()?;

// Traverse
for node in ast.nodes {
    match node.node_type() {
        NodeType::Macro => { /* handle macro */ }
        NodeType::Chars => { /* handle text */ }
        _ => {}
    }
}
```

**Characteristics:**
- Zero configuration
- Concrete types (fast compile)
- Standard library definitions
- No generics visible to user

### Level 2: Custom Definitions (9% of users)

```rust
// Add custom macros/environments
use techy::prelude::*;

let mut context = StandardContext::standard();
context.add_macro(MacroSpec::simple("highlight", "{"));
context.add_environment(EnvironmentSpec::new("myenv", "[["));

let parser = StandardParser::new(source, context);
let ast = parser.parse()?;
```

**Characteristics:**
- Extend standard library
- Still using standard types
- No custom traits needed
- Context is the only extension point used

### Level 3: Full Extension (1% of users)

```rust
// Custom nodes, state, context, parsers
use techy::prelude::*;

// 1. Custom node type
#[derive(Debug, Clone)]
struct AnnotatedNode {
    span: Span,
    inner: Box<dyn Node>,
    annotations: HashMap<String, String>,
}

impl Node for AnnotatedNode {
    fn span(&self) -> Span { self.span }
    fn clone_box(&self) -> Box<dyn Node> { Box::new(self.clone()) }
}

// 2. Custom parsing state
#[derive(Debug, Clone)]
struct StatsState {
    base: StandardParsingState<StandardContext>,
    stats: Arc<Mutex<ParseStats>>,
}

impl ParsingState for StatsState {
    type Context = StandardContext;
    // ... implement required methods
}

// 3. Custom parser
struct AnnotatingParser;

impl Parsing for AnnotatingParser {
    type Output = AnnotatedNode;
    type State = StatsState;
    
    fn parse(/* ... */) -> ParseResult<AnnotatedNode> {
        // Custom parsing logic
    }
}

// 4. Use custom types
type MyParser = ExtensibleParser<AnnotatedNode, StatsState, StandardContext>;

let mut parser = MyParser::new(source, StandardContext::standard());
parser.register_parser(AnnotatingParser);
let ast = parser.parse()?;
```

**Characteristics:**
- Full trait implementation
- Complete control over behavior
- Plugin architecture
- May require understanding generics

---

## Implementation Roadmap

### Phase 1: Foundation (Week 1-2)
**Goal:** Establish trait layer

- [ ] Define `Node` trait
  - [ ] Core methods (`span()`, `clone_box()`)
  - [ ] Optional methods (`node_type()`, `as_any()`)
  - [ ] Implement for existing node types

- [ ] Define `ParsingState` trait
  - [ ] Core methods (`in_math_mode()`, `context()`)
  - [ ] State delta support (`ParsingStateDelta`)
  - [ ] Implement for `StandardParsingState`

- [ ] Define `Context` trait
  - [ ] Resolution methods (`resolve_macro()`, etc.)
  - [ ] Implement for standard context
  - [ ] Add extension point for custom contexts

- [ ] Tests
  - [ ] Trait object tests
  - [ ] Type compatibility tests

**Deliverable:** Traits defined, implementations working

### Phase 2: Generification (Week 3-4)
**Goal:** Make core structures generic over traits

- [ ] Genericize `NodeList<N: Node>`
  - [ ] Update all usages
  - [ ] Add type alias `StandardNodeList`
  - [ ] Tests

- [ ] Genericize `Arguments<N: Node>`
  - [ ] Update parser signatures
  - [ ] Type alias `StandardArguments`
  - [ ] Tests

- [ ] Genericize node types (MacroNode, EnvironmentNode, etc.)
  - [ ] Make generic over inner node type
  - [ ] Recursive generics (`MacroNode<N: Node>`)
  - [ ] Tests

- [ ] Update `Parsing` trait
  - [ ] Add associated types for State
  - [ ] Generic parse signatures
  - [ ] Tests

**Deliverable:** Generic core, type aliases for common usage

### Phase 3: Parser Generification (Week 5)
**Goal:** Generic parser supporting custom types

- [ ] Create `Parser<N, S, C>`
  - [ ] Generic over node, state, context
  - [ ] PhantomData for unused generics
  - [ ] Tests

- [ ] Create `StandardParser` type alias
  - [ ] Point to concrete types
  - [ ] Convenience constructors
  - [ ] Documentation

- [ ] Create `ExtensibleParser<N, S, C>`
  - [ ] Parsing registry support
  - [ ] Dynamic parser dispatch
  - [ ] Tests

**Deliverable:** Fully generic parser

### Phase 4: Dynamic Extensions (Week 6)
**Goal:** Runtime extensibility via registry

- [ ] Implement `ParsingRegistry<N, S>`
  - [ ] Registration API
  - [ ] Priority-based selection
  - [ ] Tests

- [ ] Implement `DynParsing<N, S>` trait
  - [ ] Dynamic dispatch support
  - [ ] Blanket implementation
  - [ ] Tests

- [ ] Hook into `ExtensibleParser`
  - [ ] Use registry for parsing
  - [ ] Fallback to defaults
  - [ ] Tests

**Deliverable:** Plugin system working

### Phase 5: Documentation & Examples (Week 7)
**Goal:** Comprehensive documentation for all levels

- [ ] Update README with three usage levels
- [ ] Write trait implementation guides
  - [ ] Custom nodes tutorial
  - [ ] Custom state tutorial
  - [ ] Custom context tutorial
  - [ ] Custom parser tutorial

- [ ] Create examples
  - [ ] `examples/simple_usage.rs` (Level 1)
  - [ ] `examples/custom_macros.rs` (Level 2)
  - [ ] `examples/annotating_parser.rs` (Level 3)
  - [ ] `examples/stats_tracking.rs` (Level 3)
  - [ ] `examples/plugin_system.rs` (Level 3)

- [ ] API documentation
  - [ ] Doc comments on all traits
  - [ ] Usage examples in docs
  - [ ] `cargo doc` review

**Deliverable:** Complete documentation

### Phase 6: Performance Testing (Week 8)
**Goal:** Verify zero-cost abstractions

- [ ] Benchmarks
  - [ ] Baseline (concrete types)
  - [ ] Generic static dispatch
  - [ ] Generic dynamic dispatch
  - [ ] Compare to pylatexenc

- [ ] Profile hot paths
  - [ ] Identify monomorphization issues
  - [ ] Optimize critical generics
  - [ ] Binary size analysis

- [ ] Optimization
  - [ ] Inline hints where needed
  - [ ] Avoid unnecessary allocations
  - [ ] Cache frequent lookups

**Deliverable:** Performance report, optimizations

---

## Quick Decision Matrix

**When to use which approach:**

| Need | Approach | Example |
|------|----------|---------|
| Parse standard LaTeX | Level 1 | `StandardParser::with_standard_context()` |
| Add custom macros | Level 2 | Extend `StandardContext` |
| Track parse statistics | Level 2 | Extension data in `StandardParsingState` |
| Custom node annotations | Level 3 | Implement `Node` trait |
| Domain-specific syntax | Level 3 | Custom parser + registry |
| Language bindings | Level 3 | Wrap in FFI-safe traits |
| Plugin system | Level 3 | `ExtensibleParser` + registry |

---

## Type Signature Examples

### Simple Case
```rust
// User doesn't see generics
fn parse_document(source: &str) -> Result<StandardNodeList, ParseError>
```

### Intermediate Case
```rust
// Generic over context only
fn parse_with_context<C: Context>(
    source: &str,
    context: C,
) -> Result<NodeList<Box<dyn Node>>, ParseError>
```

### Advanced Case
```rust
// Fully generic
fn parse_generic<N, S, C>(
    source: &str,
    context: C,
) -> Result<NodeList<N>, ParseError>
where
    N: Node,
    S: ParsingState<Context = C>,
    C: Context,
```

---

## Core Type Naming (Per NAMING_STRATEGY.md)

### Modules
- `parser` - High-level parsing API (main entry point)
- `parsing` - Low-level parser trait and implementations
- `node` - AST node types
- `spec` - Macro/environment/specials specifications
- `state` - Parsing state management
- `token` - Tokenization

### Primary Types

```rust
// Core API (in parser module)
pub struct Parser<N, S, C> { ... }
pub type StandardParser = Parser<...>;

// Nodes (in node module)
pub trait Node { ... }
pub struct MacroNode<N: Node> { ... }
pub struct EnvironmentNode<N: Node> { ... }
pub struct CharsNode { ... }
pub struct GroupNode<N: Node> { ... }
pub struct CommentNode { ... }
pub struct MathNode<N: Node> { ... }
pub struct SpecialsNode { ... }
pub struct NodeList<N: Node> { ... }
pub struct Arguments<N: Node> { ... }  // was ParsedArguments

// State (in state module)
pub trait ParsingState { ... }
pub struct StandardParsingState<C: Context> { ... }
pub enum ParsingStateDelta { ... }  // was StateDelta

// Context (in spec module)
pub trait Context { ... }
pub struct StandardContext { ... }

// Specifications (in spec module)
pub struct MacroSpec { ... }
pub struct EnvironmentSpec { ... }
pub struct SpecialsSpec { ... }
pub struct ArgumentStructureSpec { ... }  // was ArgumentsSpec
pub struct ArgumentSpec { ... }

// Tokens (in token module)
pub struct Token { ... }  // was LatexToken
pub trait TokenReader { ... }
pub struct StringTokenReader { ... }
pub enum TokenType { ... }

// Parsing (in parsing module)
pub trait Parsing { ... }  // low-level parser trait
pub trait DynParsing<N, S> { ... }
pub struct ParsingRegistry<N, S> { ... }
```

---

## Import Examples

### Level 1: Simple Usage
```rust
use techy::prelude::*;

let parser = StandardParser::with_standard_context(source);
let ast = parser.parse()?;
```

### Level 2: Custom Context
```rust
use techy::{StandardParser, StandardContext};
use techy::spec::{MacroSpec, ArgumentStructureSpec};

let mut ctx = StandardContext::standard();
ctx.add_macro(MacroSpec::simple("highlight", "{"));

let parser = StandardParser::new(source, ctx);
let ast = parser.parse()?;
```

### Level 3: Full Custom
```rust
use techy::{Parser, ExtensibleParser};
use techy::node::Node;
use techy::state::ParsingState;
use techy::spec::Context;
use techy::parsing::Parsing;

// Define custom types...
type MyParser = ExtensibleParser<MyNode, MyState, MyContext>;

let mut parser = MyParser::new(source, context);
parser.register_parser(MyCustomParser);
let ast = parser.parse()?;
```

---

## Performance Characteristics

| Approach | Compile Time | Binary Size | Runtime | Flexibility |
|----------|--------------|-------------|---------|-------------|
| Level 1: StandardParser | Fast | Small | Fast (0% overhead) | Low |
| Level 2: Custom Context | Fast | Small | Fast (0% overhead) | Medium |
| Level 3: Static Generics | Slower | Larger | Fast (0-5% overhead) | High |
| Level 3: Dynamic Traits | Fast | Small | Slower (5-15% overhead) | Maximum |

**Recommendation:** Start with Level 1, move to Level 2 for custom macros, only use Level 3 when building extensible tools/plugins.

---

## Key Design Decisions

1. **No Backward Compatibility**
   - Library not yet public
   - Clean slate for naming
   - Follow NAMING_STRATEGY.md strictly

2. **Progressive Disclosure**
   - Simple API (StandardParser) for common use
   - Intermediate API (custom context) for extensions
   - Advanced API (full traits) for libraries/tools

3. **Zero-Cost Abstractions**
   - Static dispatch by default
   - Dynamic dispatch opt-in via trait objects
   - No runtime overhead for static cases

4. **Trait-Based Everything**
   - Node: extensible AST
   - ParsingState: extensible state
   - Context: extensible definitions
   - Parsing: extensible parsers

5. **Sensible Defaults**
   - StandardParser: uses Box<dyn Node>
   - StandardContext: standard LaTeX definitions
   - StandardParsingState: basic state tracking

---

## Summary

**The trait-based architecture provides three clean layers:**

1. **Trait layer:** Defines contracts (extensibility)
2. **Standard implementations:** Batteries included (usability)
3. **High-level API:** Generic but simple (ergonomics)

**Result:** 
- 90% of users get simplicity (Level 1)
- 9% get easy customization (Level 2)
- 1% get full power (Level 3)
- Everyone benefits from type safety and performance
- Clean naming following NAMING_STRATEGY.md
- No backward compatibility baggage
