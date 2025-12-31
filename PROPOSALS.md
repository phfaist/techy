# Architecture Proposals (Under Discussion)

This document contains architectural proposals that are still being discussed and refined. These are separate from the main strategy document, which focuses on decided architecture and implementation status.

---

## 1. Library System Design

### Problem with pylatexenc's LatexContextDb

The `LatexContextDb` in pylatexenc is a flat database that stores all macro/environment/specials definitions together:

```python
# pylatexenc approach
db = LatexContextDb()
db.add_context_category('macros', [...])
db.add_context_category('environments', [...])
db.add_context_category('specials', [...])
```

**Limitations**:
1. **No organization**: All definitions in one flat namespace
2. **No mode awareness**: Can't have different definitions for text vs math mode
3. **No modularity**: Difficult to manage standard library vs user definitions
4. **No conflict resolution**: Last definition wins, no error detection
5. **No composability**: Can't easily layer libraries (base + extension + user)

### Proposed Library System Design

#### Core Concepts

**1. Library** - A collection of definitions

```rust
/// A library contains macro, environment, and specials definitions
pub struct Library {
    name: String,
    macros: HashMap<String, Arc<MacroSpec>>,
    environments: HashMap<String, Arc<EnvironmentSpec>>,
    specials: HashMap<String, Arc<SpecialsSpec>>,

    /// Optional mode-specific overrides
    math_mode_macros: HashMap<String, Arc<MacroSpec>>,
    math_mode_environments: HashMap<String, Arc<EnvironmentSpec>>,
}

impl Library {
    /// Create a new empty library
    pub fn new(name: impl Into<String>) -> Self { ... }

    /// Add a macro definition
    pub fn add_macro(&mut self, spec: MacroSpec) -> &mut Self { ... }

    /// Add a macro that only applies in math mode
    pub fn add_math_mode_macro(&mut self, spec: MacroSpec) -> &mut Self { ... }

    /// Builder pattern for fluent API
    pub fn with_macro(mut self, spec: MacroSpec) -> Self { ... }
}
```

**2. LibrarySet** - Manages multiple libraries with resolution

```rust
/// Manages multiple libraries with defined load order and conflict resolution
pub struct LibrarySet {
    libraries: Vec<Library>,
    conflict_strategy: ConflictStrategy,
}

/// How to handle name conflicts between libraries
#[derive(Debug, Clone, Copy)]
pub enum ConflictStrategy {
    /// First definition wins (earlier library takes precedence)
    FirstWins,
    /// Last definition wins (later library overrides)
    LastWins,
    /// Error on conflicts
    Error,
}

impl LibrarySet {
    /// Create a new library set
    pub fn new() -> Self { ... }

    /// Add a library (appends to load order)
    pub fn add_library(&mut self, library: Library) -> &mut Self { ... }

    /// Set conflict resolution strategy
    pub fn with_conflict_strategy(mut self, strategy: ConflictStrategy) -> Self { ... }

    /// Resolve a macro by name
    pub fn resolve_macro(&self, name: &str, mode: Mode) -> Option<&Arc<MacroSpec>> {
        // Search libraries in order based on conflict strategy
        // Consider mode-specific overrides
        ...
    }

    /// Resolve an environment by name
    pub fn resolve_environment(&self, name: &str, mode: Mode) -> Option<&Arc<EnvironmentSpec>> { ... }

    /// Resolve specials by character(s)
    pub fn resolve_specials(&self, chars: &str, mode: Mode) -> Option<&Arc<SpecialsSpec>> { ... }
}
```

**3. Mode** - Text vs Math mode context

```rust
/// Parsing mode affects which definitions apply
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Text,
    Math,
}
```

#### Standard Libraries

```rust
/// Standard LaTeX library (base definitions)
pub fn standard_library() -> Library {
    Library::new("latex-std")
        .with_macro(MacroSpec::simple("textbf", "{"))
        .with_macro(MacroSpec::simple("emph", "{"))
        .with_macro(MacroSpec::simple("section", "[{"))
        // Math mode has different semantics for some macros
        .with_math_mode_macro(MacroSpec::simple("sqrt", "[{"))
        .with_math_mode_macro(MacroSpec::simple("frac", "{{"))
        // ... many more
}

/// AMS-LaTeX extensions
pub fn ams_library() -> Library {
    Library::new("ams")
        .with_environment(EnvironmentSpec::new("align"))
        .with_environment(EnvironmentSpec::new("gather"))
        // ... AMS-specific definitions
}

/// TikZ library
pub fn tikz_library() -> Library {
    Library::new("tikz")
        .with_environment(EnvironmentSpec::new("tikzpicture"))
        // ... TikZ definitions
}
```

#### Usage Examples

**Basic usage:**

```rust
use techy::{Parser, LibrarySet, standard_library};

// Use standard library
let libs = LibrarySet::new()
    .add_library(standard_library());

let parser = Parser::with_libraries(source, libs);
let ast = parser.parse()?;
```

**Layered libraries with user definitions:**

```rust
use techy::{Parser, LibrarySet, Library, standard_library, ams_library, ConflictStrategy};

// Layer: standard + AMS + user
let user_lib = Library::new("user")
    .with_macro(MacroSpec::simple("highlight", "[{"))
    .with_macro(MacroSpec::simple("todo", "{"));

let libs = LibrarySet::new()
    .with_conflict_strategy(ConflictStrategy::LastWins)
    .add_library(standard_library())
    .add_library(ams_library())
    .add_library(user_lib);

let parser = Parser::with_libraries(source, libs);
```

**Mode-specific definitions:**

```rust
let lib = Library::new("custom")
    // \vec in text mode - maybe an abbreviation?
    .with_macro(MacroSpec::simple("vec", "{"))
    // \vec in math mode - vector notation with arrow
    .with_math_mode_macro(MacroSpec {
        name: "vec".to_string(),
        args: ArgumentStructureSpec::new(vec![ArgumentSpec::Mandatory]),
        // Could have different rendering/semantics
    });
```

**Conflict detection:**

```rust
let lib1 = Library::new("lib1")
    .with_macro(MacroSpec::simple("custom", "{"));

let lib2 = Library::new("lib2")
    .with_macro(MacroSpec::simple("custom", "[{")); // Different signature!

let libs = LibrarySet::new()
    .with_conflict_strategy(ConflictStrategy::Error)
    .add_library(lib1)
    .add_library(lib2);

// Resolution will error on conflict
match libs.resolve_macro("custom", Mode::Text) {
    Ok(spec) => { /* use spec */ }
    Err(LibraryError::Conflict { name, libraries }) => {
        eprintln!("Conflict for '{}' between: {:?}", name, libraries);
    }
}
```

#### Integration with ParsingState

```rust
pub struct ParsingState<'libs> {
    /// Current mode
    pub mode: Mode,

    /// Reference to the library set
    pub libraries: &'libs LibrarySet,
}

impl<'libs> ParsingState<'libs> {
    /// Look up a macro
    pub fn resolve_macro(&self, name: &str) -> Option<&Arc<MacroSpec>> {
        self.libraries.resolve_macro(name, self.mode)
    }

    /// Look up an environment
    pub fn resolve_environment(&self, name: &str) -> Option<&Arc<EnvironmentSpec>> {
        self.libraries.resolve_environment(name, self.mode)
    }
}
```

#### Benefits

1. **Organization**: Clear separation between different libraries
2. **Mode awareness**: Different definitions for text vs math mode
3. **Modularity**: Easy to create and share library definitions
4. **Composability**: Layer libraries (standard + package + user)
5. **Conflict resolution**: Explicit strategies for handling conflicts
6. **Extensibility**: Users can easily add their own libraries
7. **Performance**: Arc-based sharing means zero-copy across parsers

#### Future Extensions

- **Lazy loading**: Libraries could be loaded on-demand
- **Serialization**: Save/load library definitions from files
- **Package system**: Could build a package manager for library definitions
- **Versioning**: Libraries could have versions and dependencies
- **Scoping**: Local library scopes for specific document sections

---

## 2. Source Tracking & Provenance

### Currently Implemented (v0.1)

We've implemented a lazy source location tracking system:

- **`Source`** - Owns source content, computes line/column on-demand
- **`SourceLocation<'src>`** - Lightweight reference to source with byte positions
- **`SourceLocationDetails<'src>`** - Lazy-computed line/column information

See `src/source.rs` for implementation. Key features:
- Zero upfront cost: line info computed only when needed (e.g., error reporting)
- Efficient reuse: `other_details()` shares computed line information
- Immutable API: all `SourceLocationDetails` methods take `&self`

### Future Enhancement: Rich Provenance

**Requirement**: Nodes should retain rich information about where they came from, not just byte offsets.

**Extensions for possible source origin and kind**: We might want to extend the `Source` object to easily include information about source file, etc., to better handle more advanced structures such as `\include` or auto-generated content.

**Updated Node Structure**: Still TODO in rs files!

```rust
pub struct MacroNode {
    pub location: SourceLocation,  // was: span: Span
    pub name: String,
    pub spec: Option<Arc<MacroSpec>>,
    pub args: Arguments,
    pub post_space: String,
}
```

**Benefits**:
- Rich error messages with file:line:col
- Track content across `\input`/`\include`
- Support for multi-source documents
- Better IDE integration (jump to definition, etc.)

---

## 3. Extensibility: Generic Nodes & Custom State

**Requirement**: Extensions (Python, JS bindings, custom tools) need to:
1. Attach custom data to parsing state
2. Attach custom data to individual nodes
3. Extend the parser without modifying core code

**Problem**: Current design is rigid - can't extend without modifying source

**Proposed Solution**: Generic nodes and extensible state

### Option A: Generic Nodes (Compile-time)

```rust
/// User-defined extension data for nodes
pub trait NodeExtension: Clone + std::fmt::Debug {
    /// Create default extension data
    fn default() -> Self;
}

/// Default: no extension data
#[derive(Debug, Clone, Default)]
pub struct NoExtension;
impl NodeExtension for NoExtension {
    fn default() -> Self { NoExtension }
}

/// Generic node with optional extension data
pub struct MacroNode<Ext: NodeExtension = NoExtension> {
    pub location: SourceLocation,
    pub name: String,
    pub spec: Option<Arc<MacroSpec>>,
    pub args: Arguments<Ext>,
    pub post_space: String,
    /// Extension-specific data
    pub ext: Ext,
}

// Usage in Python bindings:
#[derive(Debug, Clone)]
struct PythonNodeData {
    py_object: Option<PyObjectRef>,
    custom_attrs: HashMap<String, PyValue>,
}

type PythonMacroNode = MacroNode<PythonNodeData>;
```

### Option B: Type-Erased Extensions (Runtime)

```rust
use std::any::Any;

/// Node extension using type erasure
pub struct NodeExtData {
    data: Option<Box<dyn Any + Send + Sync>>,
}

impl NodeExtData {
    pub fn new() -> Self {
        NodeExtData { data: None }
    }

    pub fn set<T: Any + Send + Sync>(&mut self, value: T) {
        self.data = Some(Box::new(value));
    }

    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.data.as_ref()?.downcast_ref()
    }

    pub fn get_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
        self.data.as_mut()?.downcast_mut()
    }
}

pub struct MacroNode {
    pub location: SourceLocation,
    pub name: String,
    pub spec: Option<Arc<MacroSpec>>,
    pub args: Arguments,
    pub post_space: String,
    /// Extension data (type-erased)
    pub ext: NodeExtData,
}
```

### Extensible Parsing State

```rust
use std::collections::HashMap;
use std::any::{Any, TypeId};

pub struct ParsingState<'libs> {
    /// Current mode
    pub mode: Mode,
    /// Reference to the library set
    pub libraries: &'libs LibrarySet,

    /// Extension data storage (type-erased)
    extensions: HashMap<TypeId, Box<dyn Any>>,
}

impl<'libs> ParsingState<'libs> {
    /// Set extension data
    pub fn set_ext<T: Any>(&mut self, value: T) {
        self.extensions.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get extension data
    pub fn get_ext<T: Any>(&self) -> Option<&T> {
        self.extensions
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref())
    }

    /// Get mutable extension data
    pub fn get_ext_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.extensions
            .get_mut(&TypeId::of::<T>())
            .and_then(|b| b.downcast_mut())
    }
}

// Usage example:
#[derive(Debug)]
struct CustomParserData {
    depth: usize,
    custom_flags: Vec<String>,
}

let mut state = ParsingState::new(&libs);
state.set_ext(CustomParserData {
    depth: 0,
    custom_flags: vec![],
});

// Later:
if let Some(data) = state.get_ext_mut::<CustomParserData>() {
    data.depth += 1;
}
```

**Recommendation**: Use Option B (type-erased) for flexibility. Option A requires recompiling, which doesn't work for dynamic language bindings.

---

## 4. Full TeX Compliance: Gap Analysis

**Requirement**: Document features needed for full TeX compliance and assess implementation difficulty.

### Current Limitations

| Feature | TeX/LaTeX | techy Status | Difficulty | Notes |
|---------|-----------|--------------|------------|-------|
| **Catcodes** | Full support | ❌ Not supported | High | Core TeX feature; changes tokenization rules |
| **Expansion** | Full expansion | ❌ Not supported | High | Would need TeX expansion engine |
| **Primitives** | 300+ primitives | ⚠️ Partial | Medium | Many primitives not recognized |
| **Conditionals** | `\if`, `\ifx`, etc. | ❌ Not supported | High | Requires expansion & evaluation |
| **Assignments** | `\def`, `\let`, etc. | ❌ Not supported | High | Requires mutable state |
| **Groups** | TeX groups | ✅ Supported | Easy | Already handled |
| **Math modes** | Display/inline | ✅ Supported | Easy | Already tracked |
| **Arguments** | Standard args | ⚠️ Partial | Medium | Some arg types not impl |
| **Verbatim** | Full verbatim | ⚠️ Partial | Medium | Basic support exists |
| **Comments** | `%` comments | ✅ Supported | Easy | Already handled |
| **Line breaks** | `\\`, `\par` | ⚠️ Partial | Easy | Recognized but not semantic |
| **Spacing** | Full spacing rules | ⚠️ Partial | Medium | Not all rules implemented |
| **Accents** | `\'e`, etc. | ⚠️ Partial | Easy | Can add specs |
| **Ligatures** | `--`, `---`, etc. | ❌ Not supported | Low | Tokenizer feature |
| **Active chars** | `~`, etc. | ⚠️ Partial | Low | Via SpecialsSpec |

### Critical Missing Features for TeX Compliance

**1. Catcodes** (Most fundamental difference)

TeX assigns category codes to characters:
- 0: Escape (`\`)
- 1: Begin group (`{`)
- 2: End group (`}`)
- 3: Math shift (`$`)
- 4: Alignment tab (`&`)
- ... (16 categories total)

```rust
// What would be needed:
pub struct CatcodeTable {
    codes: [u8; 256],  // Category for each byte
}

pub enum Catcode {
    Escape = 0,
    BeginGroup = 1,
    EndGroup = 2,
    MathShift = 3,
    AlignmentTab = 4,
    EndOfLine = 5,
    Parameter = 6,
    Superscript = 7,
    Subscript = 8,
    Ignored = 9,
    Space = 10,
    Letter = 11,
    Other = 12,
    Active = 13,
    Comment = 14,
    Invalid = 15,
}

// Issue: Current tokenizer is hardcoded
// Would need: Dynamic tokenizer based on catcode table
```

**Difficulty**: Very High - requires complete tokenizer rewrite
**Value for techy**: Low - most use cases don't need this
**Recommendation**: Document as intentional limitation

**2. Macro Expansion & `\def`**

TeX expands macros during parsing:

```tex
\def\foo{Hello}
\foo World  % Expands to: Hello World
```

**What's needed**:
- Expansion engine that processes token stream
- Mutable definition environment
- Expansion vs execution separation

**Difficulty**: Very High - fundamental architecture change
**Value for techy**: Medium - useful for some preprocessing
**Recommendation**: Phase 2 feature, optional expansion mode

**3. Conditionals (`\if`, `\ifx`, etc.)**

```tex
\ifx\foo\bar
  % Execute if \foo equals \bar
\else
  % Otherwise
\fi
```

**What's needed**:
- Token comparison
- Conditional evaluation during parsing
- Skip false branches

**Difficulty**: High - needs expansion engine
**Value for techy**: Low - most documents don't use
**Recommendation**: Could support as "skip unknown conditionals"

### Easy Wins for Better Coverage

**1. Additional Standard Macros**

```rust
// Easy to add to standard library:
pub fn extended_standard_library() -> Library {
    standard_library()
        // Accents
        .with_macro(MacroSpec::simple(r"\'", "{"))  // \'e → é
        .with_macro(MacroSpec::simple(r"\`", "{"))  // \`e → è
        .with_macro(MacroSpec::simple(r"\"", "{"))  // \"o → ö
        .with_macro(MacroSpec::simple(r"\^", "{"))  // \^e → ê
        .with_macro(MacroSpec::simple(r"\~", "{"))  // \~n → ñ
        .with_macro(MacroSpec::simple(r"\=", "{"))  // \=a → ā
        // Line breaks
        .with_macro(MacroSpec::simple(r"\\", "["))   // \\[spacing]
        .with_macro(MacroSpec::simple("par", ""))    // \par
        // Spacing
        .with_macro(MacroSpec::simple("hspace", "*{"))
        .with_macro(MacroSpec::simple("vspace", "*{"))
        // ... many more
}
```

**2. Ligature Detection**

```rust
// In tokenizer, detect common ligatures:
fn detect_ligatures(text: &str) -> Vec<Token> {
    text.replace("---", "—")  // em-dash
        .replace("--", "–")   // en-dash
        .replace("``", """)   // open quote
        .replace("''", """)   // close quote
        // ...
}
```

**Difficulty**: Low
**Value**: Medium - better typography
**Recommendation**: Add as tokenizer option

**3. More Verbatim Modes**

```rust
// Add verbatim specs:
pub fn verbatim_library() -> Library {
    Library::new("verbatim")
        .with_environment(EnvironmentSpec::verbatim("verbatim"))
        .with_environment(EnvironmentSpec::verbatim("lstlisting"))
        .with_macro(MacroSpec::verbatim_delimited("verb"))
        // ...
}
```

**Difficulty**: Low (infrastructure exists)
**Value**: High - common in documents
**Recommendation**: High priority addition

### Summary: Gap Analysis

**techy is NOT a TeX engine**. It's a LaTeX-like markup parser focused on:
- ✅ Structural parsing (macros, environments, arguments)
- ✅ Extensibility (custom definitions, libraries)
- ✅ Practical documents (standard LaTeX, common packages)

**techy intentionally does NOT**:
- ❌ Implement TeX expansion/execution semantics
- ❌ Support catcode manipulation
- ❌ Evaluate conditionals or assignments
- ❌ Replicate full TeX primitive set

**This is by design**: techy targets parsing LaTeX-like documents for:
- Conversion tools (LaTeX → HTML, Markdown, etc.)
- Document analysis
- Syntax highlighting
- Custom document processors
- AST-based transformations

**Not for**: Building a TeX typesetting engine

**Easy additions** that improve coverage:
1. ✅ Extended macro library (accents, spacing, etc.)
2. ✅ Ligature detection in tokenizer
3. ✅ More verbatim environments
4. ⚠️ Better argument parsing (all standard types)
5. ⚠️ Active character handling via SpecialsSpec
