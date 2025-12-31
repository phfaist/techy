# Trait-Based Extensible Architecture Strategy

## Executive Summary

This document outlines a strategy to design the techy LaTeX parser with **maximum extensibility** while maintaining a **lightweight compiled base**. The core idea is to make all critical components (nodes, parsing state, parsers) trait-based, allowing users to:

1. **Extend node types** without modifying the core library
2. **Add custom parsing state** for domain-specific needs
3. **Implement custom parsers** that integrate seamlessly
4. **Zero-cost abstractions** - no runtime overhead for extensions
5. **Compile-time type safety** - extensions verified at compile time

## Status

**Library is not yet public** - no backward compatibility concerns. All naming follows NAMING_STRATEGY.md.

## Core Design Principles

### 1. **Traits Over Concrete Types**
Every major component is defined as a trait with default implementations:
- `Node` trait (not enum)
- `ParsingState` trait (not struct)
- `Parser` trait (for parsers)
- `Context` trait (replaces Library/ContextDb concept)

### 2. **Type Parameters for Extensibility**
Core structures are generic over extension types:
```rust
pub struct Parser<N, S, C>
where
    N: Node,
    S: ParsingState,
    C: Context,
{ /* ... */ }
```

### 3. **Default Implementations for Common Cases**
Users shouldn't need to implement everything for simple use cases:
```rust
// Simple case - use defaults
let parser = StandardParser::with_standard_context(source);

// Extended case - provide custom types
let parser = Parser::<MyNode, MyState, MyContext>::new(source, context);
```

### 4. **Zero-Cost Abstractions**
All abstractions compile down to the same code as hand-written specializations:
- Static dispatch via trait bounds
- Monomorphization eliminates indirection
- No vtables unless explicitly requested via `dyn`

---

## Detailed Architecture

## 1. Trait-Based Node System

### Current Problem
Using a closed `Node` enum prevents users from adding new node types:

```rust
// CLOSED - users can't add custom variants
pub enum Node {
    Chars(CharsNode),
    Macro(MacroNode),
    Environment(EnvironmentNode),
    // Can't extend!
}
```

### Proposed Solution: Node Trait

```rust
use std::fmt::Debug;
use std::any::Any;

/// Core trait that all nodes must implement
pub trait Node: Debug + Clone + Any {
    /// Get the source span this node covers
    fn span(&self) -> Span;
    
    /// Get the raw LaTeX source text for this node
    fn latex_verbatim<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span().start..self.span().end]
    }
    
    /// Convert to Any for downcasting
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    /// Convert to mutable Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    
    /// Clone into a boxed trait object
    fn clone_box(&self) -> Box<dyn Node>;
    
    /// Optional: Type tag for runtime type checking without downcasting
    fn node_type(&self) -> NodeType {
        NodeType::Custom
    }
}

/// Standard node types (extensible enum)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NodeType {
    Chars,
    Macro,
    Environment,
    Group,
    Comment,
    Math,
    Specials,
    Custom,
}

/// Helper macro for implementing Clone for trait objects
macro_rules! impl_node_clone {
    ($type:ty) => {
        impl Clone for Box<$type> {
            fn clone(&self) -> Self {
                self.as_ref().clone_box()
            }
        }
    };
}

impl_node_clone!(dyn Node);
```

### Standard Node Implementations

```rust
/// Text content node
#[derive(Debug, Clone)]
pub struct CharsNode {
    pub span: Span,
    pub chars: String,
}

impl Node for CharsNode {
    fn span(&self) -> Span {
        self.span
    }
    
    fn clone_box(&self) -> Box<dyn Node> {
        Box::new(self.clone())
    }
    
    fn node_type(&self) -> NodeType {
        NodeType::Chars
    }
}

/// Macro invocation node
#[derive(Debug, Clone)]
pub struct MacroNode<N: Node = Box<dyn Node>> {
    pub span: Span,
    pub name: String,
    pub spec: Option<Arc<MacroSpec>>,
    pub args: Arguments<N>,
    pub post_space: String,
}

impl<N: Node> Node for MacroNode<N> {
    fn span(&self) -> Span {
        self.span
    }
    
    fn clone_box(&self) -> Box<dyn Node> {
        Box::new(self.clone())
    }
    
    fn node_type(&self) -> NodeType {
        NodeType::Macro
    }
}

/// Environment node
#[derive(Debug, Clone)]
pub struct EnvironmentNode<N: Node = Box<dyn Node>> {
    pub span: Span,
    pub name: String,
    pub spec: Option<Arc<EnvironmentSpec>>,
    pub args: Arguments<N>,
    pub body: NodeList<N>,
}

impl<N: Node> Node for EnvironmentNode<N> {
    fn span(&self) -> Span {
        self.span
    }
    
    fn clone_box(&self) -> Box<dyn Node> {
        Box::new(self.clone())
    }
    
    fn node_type(&self) -> NodeType {
        NodeType::Environment
    }
}

/// Container for multiple nodes
#[derive(Debug, Clone)]
pub struct NodeList<N: Node = Box<dyn Node>> {
    pub nodes: Vec<N>,
    pub span: Span,
}

/// Parsed arguments (renamed from ParsedArguments)
#[derive(Debug, Clone)]
pub struct Arguments<N: Node = Box<dyn Node>> {
    pub args: Vec<(String, Option<N>)>,
    pub span: Span,
}
```

### User Extension Example

```rust
// User-defined node type for syntax highlighting
#[derive(Debug, Clone)]
pub struct HighlightedNode {
    span: Span,
    inner: Box<dyn Node>,
    color: String,
    style: HighlightStyle,
}

impl Node for HighlightedNode {
    fn span(&self) -> Span {
        self.span
    }
    
    fn clone_box(&self) -> Box<dyn Node> {
        Box::new(self.clone())
    }
    
    fn node_type(&self) -> NodeType {
        NodeType::Custom
    }
}

// Use in parsing
type MyNodeList = NodeList<HighlightedNode>;
```

---

## 2. Trait-Based Parsing State

### Current Problem
`ParsingState` as a concrete struct can't accommodate custom fields:

```rust
// CLOSED - can't add custom state
pub struct ParsingState<'ctx> {
    pub in_math_mode: bool,
    pub context: &'ctx Context,
}
```

### Proposed Solution: State Trait

```rust
/// Core trait for parsing state
pub trait ParsingState: Clone + Debug {
    /// Type of context used by this state
    type Context: Context;
    
    /// Is currently in math mode?
    fn in_math_mode(&self) -> bool;
    
    /// Get the current context
    fn context(&self) -> &Self::Context;
    
    /// Create a sub-state (for nested parsing)
    fn sub_state(&self) -> Self {
        self.clone()
    }
    
    /// Apply a state delta to create new state
    fn apply_delta(&self, delta: ParsingStateDelta) -> Self {
        let mut new_state = self.clone();
        new_state.apply_delta_mut(delta);
        new_state
    }
    
    /// Apply a state delta in place
    fn apply_delta_mut(&mut self, delta: ParsingStateDelta);
    
    /// Enter math mode
    fn enter_math_mode(&mut self) {
        self.apply_delta_mut(ParsingStateDelta::EnterMathMode);
    }
    
    /// Exit math mode
    fn exit_math_mode(&mut self) {
        self.apply_delta_mut(ParsingStateDelta::ExitMathMode);
    }
}

/// Standard state deltas (renamed from StateDelta)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ParsingStateDelta {
    EnterMathMode,
    ExitMathMode,
    ExtendContext(/* ... */),
    Custom(Box<dyn CustomDelta>),
}

/// Trait for custom state deltas
pub trait CustomDelta: Debug + Clone {
    fn apply(&self, state: &mut dyn ParsingState);
}
```

### Default Implementation

```rust
/// Standard parsing state implementation
#[derive(Debug, Clone)]
pub struct StandardParsingState<C: Context> {
    pub in_math_mode: bool,
    pub context: C,
    /// Extension storage for custom state
    extensions: HashMap<TypeId, Box<dyn Any>>,
}

impl<C: Context> ParsingState for StandardParsingState<C> {
    type Context = C;
    
    fn in_math_mode(&self) -> bool {
        self.in_math_mode
    }
    
    fn context(&self) -> &Self::Context {
        &self.context
    }
    
    fn apply_delta_mut(&mut self, delta: ParsingStateDelta) {
        match delta {
            ParsingStateDelta::EnterMathMode => {
                self.in_math_mode = true;
            }
            ParsingStateDelta::ExitMathMode => {
                self.in_math_mode = false;
            }
            ParsingStateDelta::Custom(custom) => {
                custom.apply(self);
            }
            _ => {}
        }
    }
}

impl<C: Context> StandardParsingState<C> {
    /// Get extension data
    pub fn get_ext<T: Any>(&self) -> Option<&T> {
        self.extensions
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref())
    }
    
    /// Set extension data
    pub fn set_ext<T: Any + Clone>(&mut self, value: T) {
        self.extensions.insert(TypeId::of::<T>(), Box::new(value));
    }
}
```

### User Extension Example

```rust
// User-defined state for tracking nesting depth
#[derive(Debug, Clone)]
pub struct NestingTrackingState<C: Context> {
    base: StandardParsingState<C>,
    nesting_depth: usize,
    nesting_stack: Vec<String>,
}

impl<C: Context> ParsingState for NestingTrackingState<C> {
    type Context = C;
    
    fn in_math_mode(&self) -> bool {
        self.base.in_math_mode()
    }
    
    fn context(&self) -> &Self::Context {
        self.base.context()
    }
    
    fn apply_delta_mut(&mut self, delta: ParsingStateDelta) {
        // Delegate to base
        self.base.apply_delta_mut(delta.clone());
        
        // Handle custom logic
        match delta {
            ParsingStateDelta::Custom(custom) => {
                if let Some(nesting) = custom.as_any().downcast_ref::<NestingDelta>() {
                    match nesting {
                        NestingDelta::Enter(name) => {
                            self.nesting_depth += 1;
                            self.nesting_stack.push(name.clone());
                        }
                        NestingDelta::Exit => {
                            self.nesting_depth = self.nesting_depth.saturating_sub(1);
                            self.nesting_stack.pop();
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
enum NestingDelta {
    Enter(String),
    Exit,
}

impl CustomDelta for NestingDelta {
    fn apply(&self, state: &mut dyn ParsingState) {
        // Implementation...
    }
}
```

---

## 3. Trait-Based Context System

### Current Problem
Need a flexible system for managing macro/environment/specials definitions.

### Proposed Solution: Context Trait

```rust
/// Trait for definition contexts
pub trait Context: Clone + Debug {
    /// Look up a macro by name
    fn resolve_macro(&self, name: &str) -> Option<&Arc<MacroSpec>>;
    
    /// Look up an environment by name
    fn resolve_environment(&self, name: &str) -> Option<&Arc<EnvironmentSpec>>;
    
    /// Look up specials by character
    fn resolve_specials(&self, chars: &str) -> Option<&Arc<SpecialsSpec>>;
    
    /// Get all macro names (for IDE autocomplete, etc.)
    fn macro_names(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(std::iter::empty())
    }
    
    /// Get all environment names
    fn environment_names(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(std::iter::empty())
    }
}

/// Standard context implementation
#[derive(Debug, Clone)]
pub struct StandardContext {
    macros: HashMap<String, Arc<MacroSpec>>,
    environments: HashMap<String, Arc<EnvironmentSpec>>,
    specials: HashMap<String, Arc<SpecialsSpec>>,
}

impl Context for StandardContext {
    fn resolve_macro(&self, name: &str) -> Option<&Arc<MacroSpec>> {
        self.macros.get(name)
    }
    
    fn resolve_environment(&self, name: &str) -> Option<&Arc<EnvironmentSpec>> {
        self.environments.get(name)
    }
    
    fn resolve_specials(&self, chars: &str) -> Option<&Arc<SpecialsSpec>> {
        self.specials.get(chars)
    }
    
    fn macro_names(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.macros.keys().map(|s| s.as_str()))
    }
    
    fn environment_names(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.environments.keys().map(|s| s.as_str()))
    }
}

impl StandardContext {
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
    
    pub fn add_environment(&mut self, spec: EnvironmentSpec) {
        self.environments.insert(spec.name.clone(), Arc::new(spec));
    }
    
    /// Get default context with standard LaTeX
    pub fn standard() -> Self {
        let mut ctx = Self::new();
        // Add standard macros...
        ctx
    }
}
```

### User Extension Example

```rust
// Layered context with fallback
#[derive(Debug, Clone)]
pub struct LayeredContext<Base: Context, Override: Context> {
    base: Base,
    overrides: Override,
}

impl<Base: Context, Override: Context> Context for LayeredContext<Base, Override> {
    fn resolve_macro(&self, name: &str) -> Option<&Arc<MacroSpec>> {
        self.overrides.resolve_macro(name)
            .or_else(|| self.base.resolve_macro(name))
    }
    
    fn resolve_environment(&self, name: &str) -> Option<&Arc<EnvironmentSpec>> {
        self.overrides.resolve_environment(name)
            .or_else(|| self.base.resolve_environment(name))
    }
    
    fn resolve_specials(&self, chars: &str) -> Option<&Arc<SpecialsSpec>> {
        self.overrides.resolve_specials(chars)
            .or_else(|| self.base.resolve_specials(chars))
    }
}

// Usage
let base = StandardContext::standard();
let mut user = StandardContext::new();
user.add_macro(MacroSpec::simple("highlight", "{"));

let context = LayeredContext {
    base,
    overrides: user,
};
```

---

## 4. Enhanced Parsing Trait

### Current Design (Good Foundation)

```rust
pub trait Parsing {
    type Output;
    
    fn parse<'s, S: ParsingState>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &S,
    ) -> ParseResult<Self::Output>;
}
```

### Enhanced for Full Extensibility

```rust
/// Enhanced parsing trait (in the `parsing` module)
pub trait Parsing {
    /// Output type (must be a Node)
    type Output: Node;
    
    /// Parsing state type
    type State: ParsingState;
    
    /// Parse with given state
    fn parse<'s>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &Self::State,
    ) -> ParseResult<Self::Output>;
    
    /// Optional: Check if this parser can handle the next token
    fn can_parse(&self, token: &Token, state: &Self::State) -> bool {
        true  // Default: try to parse
    }
    
    /// Optional: Priority when multiple parsers can handle a token
    fn priority(&self) -> i32 {
        0  // Default priority
    }
}

/// Parser that can be dynamically dispatched
pub trait DynParsing<N: Node, S: ParsingState> {
    fn parse_dyn<'s>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &S,
    ) -> ParseResult<N>;
    
    fn can_parse_dyn(&self, token: &Token, state: &S) -> bool;
    fn priority_dyn(&self) -> i32;
}

// Blanket implementation
impl<T, N, S> DynParsing<N, S> for T
where
    T: Parsing<Output = N, State = S>,
    N: Node,
    S: ParsingState,
{
    fn parse_dyn<'s>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &S,
    ) -> ParseResult<N> {
        self.parse(source, token_reader, state)
    }
    
    fn can_parse_dyn(&self, token: &Token, state: &S) -> bool {
        self.can_parse(token, state)
    }
    
    fn priority_dyn(&self) -> i32 {
        self.priority()
    }
}
```

---

## 5. Extensible Parser API

### Type-Parameterized Parser

```rust
/// Main parser entry point (generic over extensions)
/// Located in the `parser` module (high-level API)
pub struct Parser<N, S, C>
where
    N: Node,
    S: ParsingState<Context = C>,
    C: Context,
{
    source: String,
    context: C,
    _phantom: PhantomData<(N, S)>,
}

impl<N, S, C> Parser<N, S, C>
where
    N: Node,
    S: ParsingState<Context = C>,
    C: Context,
{
    pub fn new(source: String, context: C) -> Self {
        Self {
            source,
            context,
            _phantom: PhantomData,
        }
    }
    
    pub fn parse(&self) -> Result<NodeList<N>, ParseError> {
        let mut token_reader = StringTokenReader::new(&self.source);
        let state = S::default_with_context(&self.context);
        
        let general_parser = GeneralNodesParser::<N, S>::default();
        let (nodelist, _) = general_parser.parse(&self.source, &mut token_reader, &state)?;
        Ok(nodelist)
    }
}

// Type alias for common case with standard types
pub type StandardParser = Parser<
    Box<dyn Node>,
    StandardParsingState<StandardContext>,
    StandardContext,
>;

impl StandardParser {
    /// Convenient constructor for standard case
    pub fn with_standard_context(source: String) -> Self {
        Self::new(source, StandardContext::standard())
    }
}
```

### User Extension Example

```rust
// User's custom types
type MyNode = HighlightedNode;
type MyState = NestingTrackingState<StandardContext>;
type MyContext = StandardContext;

// Use custom parser
let parser = Parser::<MyNode, MyState, MyContext>::new(
    source,
    MyContext::standard(),
);

let ast = parser.parse()?;
```

---

## 6. Parsing Registry for Dynamic Extension

### Problem
Users want to register custom parsers without recompiling the library.

### Solution: Parsing Registry

```rust
/// Registry of parsers with dynamic dispatch
pub struct ParsingRegistry<N: Node, S: ParsingState> {
    parsers: Vec<Box<dyn DynParsing<N, S>>>,
}

impl<N: Node, S: ParsingState> ParsingRegistry<N, S> {
    pub fn new() -> Self {
        Self {
            parsers: Vec::new(),
        }
    }
    
    /// Register a parser
    pub fn register<P>(&mut self, parser: P)
    where
        P: Parsing<Output = N, State = S> + 'static,
    {
        self.parsers.push(Box::new(parser));
    }
    
    /// Find parser for token
    pub fn find_parser<'a>(
        &'a self,
        token: &Token,
        state: &S,
    ) -> Option<&'a dyn DynParsing<N, S>> {
        self.parsers
            .iter()
            .filter(|p| p.can_parse_dyn(token, state))
            .max_by_key(|p| p.priority_dyn())
            .map(|b| b.as_ref())
    }
    
    /// Parse with appropriate parser
    pub fn parse<'s>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &S,
    ) -> ParseResult<N> {
        let token = token_reader.peek_token()?
            .ok_or(ParseError::UnexpectedEndOfInput)?;
        
        let parser = self.find_parser(&token, state)
            .ok_or_else(|| ParseError::NoParser(token.clone()))?;
        
        parser.parse_dyn(source, token_reader, state)
    }
}

/// Parser with custom parsing registry
pub struct ExtensibleParser<N, S, C>
where
    N: Node,
    S: ParsingState<Context = C>,
    C: Context,
{
    source: String,
    context: C,
    registry: ParsingRegistry<N, S>,
    _phantom: PhantomData<S>,
}

impl<N, S, C> ExtensibleParser<N, S, C>
where
    N: Node + 'static,
    S: ParsingState<Context = C>,
    C: Context,
{
    pub fn new(source: String, context: C) -> Self {
        let mut registry = ParsingRegistry::new();
        
        // Register default parsers
        registry.register(CharsParser::default());
        registry.register(MacroParser::default());
        registry.register(EnvironmentParser::default());
        registry.register(GroupParser::default());
        registry.register(CommentParser::default());
        
        Self {
            source,
            context,
            registry,
            _phantom: PhantomData,
        }
    }
    
    /// Add a custom parser
    pub fn register_parser<P>(&mut self, parser: P)
    where
        P: Parsing<Output = N, State = S> + 'static,
    {
        self.registry.register(parser);
    }
    
    pub fn parse(&self) -> Result<NodeList<N>, ParseError> {
        // Use registry for parsing...
        todo!()
    }
}
```

---

## 7. Usage Examples

### Example 1: Simple Default Usage

```rust
use techy::prelude::*;

fn main() -> Result<(), ParseError> {
    let source = r"\textbf{Hello} world!";
    let parser = StandardParser::with_standard_context(source.to_string());
    
    let ast = parser.parse()?;
    
    for node in ast.nodes {
        match node.node_type() {
            NodeType::Macro => {
                let macro_node = node.as_any()
                    .downcast_ref::<MacroNode>()
                    .unwrap();
                println!("Macro: {}", macro_node.name);
            }
            NodeType::Chars => {
                let chars_node = node.as_any()
                    .downcast_ref::<CharsNode>()
                    .unwrap();
                println!("Text: {}", chars_node.chars);
            }
            _ => {}
        }
    }
    
    Ok(())
}
```

### Example 2: Custom Node Types

```rust
use techy::prelude::*;

// Custom node with annotations
#[derive(Debug, Clone)]
struct AnnotatedNode {
    span: Span,
    inner: Box<dyn Node>,
    annotations: Vec<String>,
}

impl Node for AnnotatedNode {
    fn span(&self) -> Span {
        self.span
    }
    
    fn clone_box(&self) -> Box<dyn Node> {
        Box::new(self.clone())
    }
}

// Custom parser that annotates nodes
struct AnnotatingParser;

impl Parsing for AnnotatingParser {
    type Output = AnnotatedNode;
    type State = StandardParsingState<StandardContext>;
    
    fn parse<'s>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &Self::State,
    ) -> ParseResult<AnnotatedNode> {
        // Parse inner node
        let inner_parser = StandardNodeParser;
        let (inner, delta) = inner_parser.parse(source, token_reader, state)?;
        
        // Add annotation
        let annotated = AnnotatedNode {
            span: inner.span(),
            inner: inner.clone_box(),
            annotations: vec!["Parsed successfully".to_string()],
        };
        
        Ok((annotated, delta))
    }
}

fn main() -> Result<(), ParseError> {
    let source = r"\textbf{Hello}";
    
    type MyParser = ExtensibleParser<
        AnnotatedNode,
        StandardParsingState<StandardContext>,
        StandardContext,
    >;
    
    let mut parser = MyParser::new(source.to_string(), StandardContext::standard());
    parser.register_parser(AnnotatingParser);
    
    let ast = parser.parse()?;
    
    Ok(())
}
```

### Example 3: Custom Parsing State

```rust
use techy::prelude::*;

// State that tracks macro usage statistics
#[derive(Debug, Clone)]
struct StatsTrackingState {
    base: StandardParsingState<StandardContext>,
    macro_counts: Arc<Mutex<HashMap<String, usize>>>,
}

impl ParsingState for StatsTrackingState {
    type Context = StandardContext;
    
    fn in_math_mode(&self) -> bool {
        self.base.in_math_mode()
    }
    
    fn context(&self) -> &Self::Context {
        self.base.context()
    }
    
    fn apply_delta_mut(&mut self, delta: ParsingStateDelta) {
        self.base.apply_delta_mut(delta);
    }
}

impl StatsTrackingState {
    fn record_macro(&self, name: &str) {
        let mut counts = self.macro_counts.lock().unwrap();
        *counts.entry(name.to_string()).or_insert(0) += 1;
    }
}

// Custom macro parser that records statistics
struct StatsMacroParser;

impl Parsing for StatsMacroParser {
    type Output = MacroNode;
    type State = StatsTrackingState;
    
    fn parse<'s>(
        &self,
        source: &'s str,
        token_reader: &mut dyn TokenReader,
        state: &Self::State,
    ) -> ParseResult<MacroNode> {
        // Parse macro normally
        let default_parser = MacroParser::default();
        let (node, delta) = default_parser.parse(source, token_reader, state)?;
        
        // Record in stats
        state.record_macro(&node.name);
        
        Ok((node, delta))
    }
    
    fn can_parse(&self, token: &Token, _state: &Self::State) -> bool {
        matches!(token.token_type, TokenType::Macro(_))
    }
}
```

### Example 4: Custom Context (Database-Backed)

```rust
use techy::prelude::*;

// Context that loads from database
#[derive(Debug, Clone)]
struct DatabaseContext {
    db_connection: Arc<DatabaseConnection>,
    cache: Arc<RwLock<HashMap<String, Arc<MacroSpec>>>>,
}

impl Context for DatabaseContext {
    fn resolve_macro(&self, name: &str) -> Option<&Arc<MacroSpec>> {
        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(spec) = cache.get(name) {
                // SAFETY: This is safe because Arc keeps the data alive
                return Some(unsafe { &*(spec as *const Arc<MacroSpec>) });
            }
        }
        
        // Load from database
        if let Some(spec) = self.load_from_db(name) {
            let mut cache = self.cache.write().unwrap();
            cache.insert(name.to_string(), Arc::new(spec));
            
            let cache = cache.downgrade();
            cache.get(name)
        } else {
            None
        }
    }
    
    // ... other methods
}

impl DatabaseContext {
    fn load_from_db(&self, name: &str) -> Option<MacroSpec> {
        // Query database
        self.db_connection.query_macro(name)
    }
}
```

---

## 8. Performance Considerations

### Static Dispatch (Fast Path)

When types are known at compile time, use static dispatch:

```rust
// Monomorphized - no vtable overhead
pub struct ConcreteParser {
    parser: Parser<
        MacroNode,
        StandardParsingState<StandardContext>,
        StandardContext,
    >,
}

impl ConcreteParser {
    pub fn parse(&self) -> Result<NodeList<MacroNode>, ParseError> {
        // All calls are statically dispatched
        self.parser.parse()
    }
}
```

### Dynamic Dispatch (Flexible Path)

When runtime flexibility is needed:

```rust
// Uses trait objects - small vtable overhead
pub type DynamicParser = ExtensibleParser<
    Box<dyn Node>,
    Box<dyn ParsingState<Context = Box<dyn Context>>>,
    Box<dyn Context>,
>;
```

### Hybrid Approach

```rust
// Static dispatch for core, dynamic for extensions
pub struct HybridParser<Ext: Node> {
    base: Parser<
        MacroNode,
        StandardParsingState<StandardContext>,
        StandardContext,
    >,
    extension_parsers: Vec<Box<dyn DynParsing<Ext, StandardParsingState<StandardContext>>>>,
}
```

### Benchmark Comparison

Expected performance (relative to hand-written specialized code):

| Approach | Overhead | Use Case |
|----------|----------|----------|
| Static monomorphic | 0% | Production parsers with fixed types |
| Static with generics | 0-5% | Library code, known at compile time |
| Dynamic trait objects | 5-15% | Plugin systems, runtime extensibility |
| Hybrid (static + dynamic) | 2-8% | Most applications (fast core + plugins) |

---

## 9. Compile-Time vs Runtime Extensibility

### Compile-Time Extensions (Zero-Cost)

```rust
// Define custom types
struct MyNode { /* ... */ }
struct MyState { /* ... */ }
struct MyContext { /* ... */ }

// Implement traits
impl Node for MyNode { /* ... */ }
impl ParsingState for MyState { /* ... */ }
impl Context for MyContext { /* ... */ }

// Compiler monomorphizes - same performance as hand-written
type MyParser = Parser<MyNode, MyState, MyContext>;

// ✅ Zero runtime overhead
// ✅ Type-safe
// ✅ No recompilation needed (library already public)
```

### Runtime Extensions (Small Overhead)

```rust
// Register parsers at runtime
let mut parser = ExtensibleParser::new(source, context);
parser.register_parser(CustomParser1);
parser.register_parser(CustomParser2);
parser.register_parser(CustomParser3);

// ✅ No recompilation needed
// ✅ Plugin architecture possible
// ⚠️ Small vtable overhead (~5-15%)
```

### Recommendation

**Use compile-time extensions by default**, fall back to runtime extensions only when:
- Building a plugin system
- Creating language bindings (Python, JS)
- Need to load parsers from config files
- Rapid prototyping during development

---

## 10. Implementation Strategy

### Phase 1: Introduce Traits

```rust
// Define traits
pub trait Node { /* ... */ }
pub trait ParsingState { /* ... */ }
pub trait Context { /* ... */ }

// Implement for existing types
impl Node for MacroNode { /* ... */ }
impl Node for CharsNode { /* ... */ }
impl ParsingState for StandardParsingState { /* ... */ }
impl Context for StandardContext { /* ... */ }
```

### Phase 2: Genericize Core Structures

```rust
// Make generic
pub struct NodeList<N: Node> {
    pub nodes: Vec<N>,
}

pub struct Arguments<N: Node> {
    pub args: Vec<(String, Option<N>)>,
}

// Concrete types for standard usage
pub type StandardNodeList = NodeList<Box<dyn Node>>;
pub type StandardArguments = Arguments<Box<dyn Node>>;
```

### Phase 3: Update Parser API

```rust
// Generic parser
pub struct Parser<N, S, C>
where
    N: Node,
    S: ParsingState<Context = C>,
    C: Context,
{ /* ... */ }

// Standard type for common usage
pub type StandardParser = Parser<
    Box<dyn Node>,
    StandardParsingState<StandardContext>,
    StandardContext,
>;
```

### Phase 4: Full Trait-Based API

All public API uses traits and type parameters. Users choose their abstraction level.

---

## 11. Documentation Strategy

### For Basic Users

```rust
/// # Quick Start
///
/// For most users, use the standard parser:
///
/// ```rust
/// use techy::prelude::*;
///
/// let parser = StandardParser::with_standard_context(source);
/// let ast = parser.parse()?;
/// ```
///
/// The standard parser uses sensible defaults and requires no configuration.
```

### For Advanced Users

```rust
/// # Custom Node Types
///
/// To create custom node types, implement the `Node` trait:
///
/// ```rust
/// use techy::Node;
///
/// #[derive(Debug, Clone)]
/// struct MyNode {
///     span: Span,
///     // ... custom fields
/// }
///
/// impl Node for MyNode {
///     fn span(&self) -> Span { self.span }
///     fn clone_box(&self) -> Box<dyn Node> { Box::new(self.clone()) }
/// }
/// ```
```

### For Library Authors

```rust
/// # Building Extensions
///
/// Library authors can create reusable extensions:
///
/// ```rust
/// pub struct MyExtension;
///
/// impl MyExtension {
///     pub fn install<N, S, C>(parser: &mut ExtensibleParser<N, S, C>)
///     where
///         N: Node,
///         S: ParsingState<Context = C>,
///         C: Context,
///     {
///         parser.register_parser(MyCustomParser);
///         // ... register other components
///     }
/// }
/// ```
```

---

## 12. Testing Strategy

### Test Each Abstraction Level

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    // Test concrete types
    #[test]
    fn test_macro_node_creation() {
        let node = MacroNode {
            span: Span { start: 0, end: 5 },
            name: "test".to_string(),
            spec: None,
            args: Arguments::empty(),
            post_space: String::new(),
        };
        
        assert_eq!(node.span(), Span { start: 0, end: 5 });
    }
    
    // Test trait implementation
    #[test]
    fn test_node_trait() {
        let node: Box<dyn Node> = Box::new(MacroNode { /* ... */ });
        assert_eq!(node.span(), Span { start: 0, end: 5 });
    }
    
    // Test generic parser
    #[test]
    fn test_generic_parser() {
        let parser = Parser::<
            Box<dyn Node>,
            StandardParsingState<StandardContext>,
            StandardContext,
        >::new(source, StandardContext::standard());
        
        let ast = parser.parse().unwrap();
        assert!(ast.nodes.len() > 0);
    }
    
    // Test custom implementation
    #[test]
    fn test_custom_node() {
        struct CustomNode {
            span: Span,
            data: String,
        }
        
        impl Node for CustomNode {
            fn span(&self) -> Span { self.span }
            fn clone_box(&self) -> Box<dyn Node> { Box::new(self.clone()) }
        }
        
        let node = CustomNode {
            span: Span { start: 0, end: 5 },
            data: "test".to_string(),
        };
        
        let trait_obj: Box<dyn Node> = Box::new(node);
        assert_eq!(trait_obj.span(), Span { start: 0, end: 5 });
    }
}
```

---

## 13. Summary

### Key Benefits

1. **Maximum Extensibility**
   - Users can extend nodes, state, context without forking
   - Plugin architecture via parsing registry
   - Trait-based design enables composition

2. **Zero-Cost Abstractions**
   - Static dispatch when types known at compile time
   - Monomorphization eliminates overhead
   - Optional dynamic dispatch only when needed

3. **Type Safety**
   - Compiler verifies all extensions
   - No runtime type errors
   - Clear API boundaries

4. **Clean Design**
   - No backward compatibility baggage
   - Clear, consistent naming (per NAMING_STRATEGY.md)
   - Progressive disclosure (simple → advanced)

### Trade-offs

| Aspect | Concrete Types | Trait-Based |
|--------|---------------|-------------|
| Simplicity | ✅ Very simple | ⚠️ More complex |
| Extensibility | ❌ Limited | ✅ Maximum |
| Compile time | ✅ Fast | ⚠️ Slower (monomorphization) |
| Binary size | ✅ Small | ⚠️ Larger (monomorphization) |
| Runtime performance | ✅ Fast | ✅ Fast (static) / ⚠️ Slower (dynamic) |
| IDE support | ✅ Good | ⚠️ Variable |

### Recommendation

**Start with a hybrid approach:**

1. **Core types use traits** for maximum extensibility
2. **Provide standard implementations** for common cases
3. **Offer both concrete and generic APIs**
4. **Let users choose** their preferred level of abstraction

**Default path (simple):**
```rust
use techy::StandardParser;
let ast = StandardParser::with_standard_context(source).parse()?;
```

**Extension path (advanced):**
```rust
use techy::{Parser, ExtensibleParser};
type MyParser = ExtensibleParser<MyNode, MyState, MyContext>;
let ast = MyParser::new(source, context).parse()?;
```

This gives us **the best of both worlds**: simplicity for common cases, power for advanced use cases.
