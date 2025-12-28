# Naming Strategy: pylatexenc → techy

This document outlines the renaming strategy for moving away from LaTeX-specific terminology to more generic, purpose-focused names. The goal is to make `techy` applicable to any LaTeX-like markup language, not just LaTeX itself.

## Status

**Implementation Status**: IMMEDIATE - No migration needed (library not yet public)

All renames will be applied directly without backward compatibility aliases.

## Design Principles

1. **Generic over Specific**: Prefer terms that describe structure/purpose rather than LaTeX-specific concepts
2. **Consistency**: Use the same naming pattern across similar concepts
3. **Clarity**: Names should be self-explanatory and specific enough to avoid ambiguity
4. **Brevity**: Avoid redundant prefixes when the module/context makes it clear
5. **Distinctiveness**: Names should be sufficiently specific to avoid confusion with unrelated concepts

## Core Naming Changes

### Module-Level Changes

| pylatexenc | techy | Rationale |
|------------|-------|-----------|
| `latexnodes` | `node` | Module name implies content; "latex" prefix redundant |
| `latexwalker` | `walker` or `parser` | "latex" prefix redundant; consider renaming to `parser` for clarity |
| `macrospec` | `spec` | Module name already implies it's for specifications |

### Primary Types

#### Node Types

| pylatexenc | Current techy | Final techy | Decision |
|------------|---------------|-------------|----------|
| `LatexNode` | `Node` | `Node` ✓ | **KEEP** - Already correct |
| `LatexNodeList` | `NodeList` | `NodeList` ✓ | **KEEP** - Already correct |
| `LatexCharsNode` | `CharsNode` | `CharsNode` ✓ | **KEEP** - Already correct |
| `LatexMacroNode` | `MacroNode` | `MacroNode` ✓ | **KEEP** - "Macro" well-understood in markup languages |
| `LatexEnvironmentNode` | `EnvironmentNode` | `EnvironmentNode` ✓ | **KEEP** - Clear and descriptive |
| `LatexGroupNode` | `GroupNode` | `GroupNode` ✓ | **KEEP** - Already correct |
| `LatexCommentNode` | `CommentNode` | `CommentNode` ✓ | **KEEP** - Already correct |
| `LatexMathNode` | `MathNode` | `MathNode` ✓ | **KEEP** - Math is inherently LaTeX-like |
| `LatexSpecialsNode` | `SpecialsNode` | `SpecialsNode` ✓ | **KEEP** - Already correct |

**Decision**: All node names are good as-is. No changes needed.

#### Context & State Types

| pylatexenc | Current techy | Final techy | Decision |
|------------|---------------|-------------|----------|
| `LatexWalker` | `LatexWalker` | `Parser` | **RENAME** - More descriptive; "Walker" is too vague |
| `LatexContextDb` | `LatexContextDb` | `ContextDb` | **RENAME** - Remove "Latex" prefix; **UNDER DISCUSSION** - name may not be specific enough |
| `ParsingState` | `ParsingState` | `ParsingState` ✓ | **KEEP** - Already correct |
| `ParsingStateDelta` | `StateDelta` | `ParsingStateDelta` | **RENAME** - "StateDelta" not specific enough; needs to clearly reference ParsingState |

**Decisions**:
- `Parser`: More accurate than "Walker" - this is the main parsing entry point
- `ContextDb`: Remove "Latex" prefix for now. **OPEN QUESTION**: Is "Context" specific enough for a database of known markup constructs (macros/environments/specials) with defined syntax and semantics?
- `ParsingStateDelta`: Specificity important - clarifies it's a delta for `ParsingState`, not just any state

#### Specification Types

| pylatexenc | Current techy | Final techy | Decision |
|------------|---------------|-------------|----------|
| `MacroSpec` | `MacroSpec` | `MacroSpec` ✓ | **KEEP** - "Macro" widely understood |
| `EnvironmentSpec` | `EnvironmentSpec` | `EnvironmentSpec` ✓ | **KEEP** - Clear and descriptive |
| `SpecialsSpec` | `SpecialsSpec` | `SpecialsSpec` ✓ | **KEEP** - Already good |
| `ArgumentsSpec` | `ArgumentsSpec` | `ArgumentStructureSpec` | **RENAME** - Too similar to `ArgumentSpec`; "Structure" clarifies it defines the structure of arguments |
| `ArgumentSpec` | `ArgumentSpec` | `ArgumentSpec` ✓ | **KEEP** - Clear for individual argument |

**Decisions**:
- `ArgumentStructureSpec`: Avoids confusion with singular `ArgumentSpec`. Clearly indicates this defines the structure/pattern of multiple arguments.

#### Token Types

| pylatexenc | Current techy | Final techy | Decision |
|------------|---------------|-------------|----------|
| `LatexToken` | `LatexToken` | `Token` | **RENAME** - Remove "Latex" prefix |
| `LatexTokenReader` | `TokenReader` | `TokenReader` ✓ | **KEEP** - Already correct (trait) |
| `StringTokenReader` | `StringTokenReader` | `StringTokenReader` ✓ | **KEEP** - Already correct |
| `TokenType` | `TokenType` | `TokenType` ✓ | **KEEP** - Already correct |

**Decision**: `Token` is appropriately generic.

#### Parsed Results

| pylatexenc | Current techy | Final techy | Decision |
|------------|---------------|-------------|----------|
| `ParsedArguments` | `ParsedArguments` | `Arguments` | **RENAME** - "Parsed" is implied by context; simpler is better |
| `ParsedMacroArgs` | N/A | N/A | Legacy pylatexenc - not needed |

**Decision**: `Arguments` is cleaner and context makes it clear they're parsed.

## Final Naming Decisions

All changes will be implemented immediately (no migration phase needed):

```rust
// Core API Types
pub struct Parser { ... }                    // was: LatexWalker
pub struct ContextDb { ... }                 // was: LatexContextDb (UNDER DISCUSSION)
pub struct Token { ... }                     // was: LatexToken

// Node Types (all kept as-is)
pub enum Node { ... }                        // was: LatexNode
pub struct NodeList { ... }                  // was: LatexNodeList
pub struct MacroNode { ... }                 // was: LatexMacroNode
pub struct EnvironmentNode { ... }           // was: LatexEnvironmentNode
pub struct MathNode { ... }                  // was: LatexMathNode
// ... etc (all node types keep current names)

// Specification Types
pub struct MacroSpec { ... }                 // kept as-is
pub struct EnvironmentSpec { ... }           // kept as-is
pub struct SpecialsSpec { ... }              // kept as-is
pub struct ArgumentStructureSpec { ... }     // was: ArgumentsSpec
pub struct ArgumentSpec { ... }              // kept as-is

// State Types
pub struct ParsingState { ... }              // kept as-is
pub enum ParsingStateDelta { ... }           // was: StateDelta

// Parsed Results
pub struct Arguments { ... }                 // was: ParsedArguments
```

## Import Path Examples

### Before (Current)

```rust
use techy::{LatexWalker, LatexContextDb, LatexToken};
use techy::node::{Node, MacroNode, EnvironmentNode};
use techy::spec::{MacroSpec, ArgumentsSpec, ArgumentSpec};
use techy::state::{ParsingState, StateDelta};
```

### After (Final)

```rust
use techy::{Parser, ContextDb, Token};
use techy::node::{Node, MacroNode, EnvironmentNode, Arguments};
use techy::spec::{MacroSpec, ArgumentStructureSpec, ArgumentSpec};
use techy::state::{ParsingState, ParsingStateDelta};
```

## Implementation Strategy

**No Migration Needed** - Library is not yet public, so we can rename immediately.

### Implementation Checklist

1. **Core Types** (High Priority - Public API)
   - [ ] `LatexWalker` → `Parser` in `src/walker/mod.rs`
   - [ ] `LatexContextDb` → `ContextDb` in `src/spec/mod.rs` (**PENDING FINAL NAME DECISION**)
   - [ ] `LatexToken` → `Token` in `src/token/mod.rs`
   - [ ] `StateDelta` → `ParsingStateDelta` in `src/state/mod.rs`

2. **Specification Types** (Medium Priority)
   - [ ] `ArgumentsSpec` → `ArgumentStructureSpec` in `src/spec/mod.rs`
   - [ ] `ParsedArguments` → `Arguments` in `src/node/mod.rs`

3. **Update Re-exports** (Critical)
   - [ ] Update `src/lib.rs` public exports
   - [ ] Update module documentation

4. **Update Documentation** (High Priority)
   - [ ] README.md examples
   - [ ] QUICKSTART.md examples
   - [ ] DEVELOPMENT.md examples
   - [ ] Doc comments in all source files

5. **Update Tests & Examples**
   - [ ] `tests/integration.rs`
   - [ ] `examples/basic.rs`
   - [ ] `examples/custom_macros.rs`

6. **Verify Build**
   - [ ] `cargo build` succeeds
   - [ ] `cargo test` passes
   - [ ] Examples run correctly

## Rationale for Decisions

### Why These Names Were Kept

1. **MacroNode**: "Macro" is widely understood in markup/template languages (not just LaTeX)
2. **EnvironmentNode**: "Environment" is clear and generic (cf. HTML/XML environments)
3. **MathNode**: Math notation is inherently LaTeX-like; no clearer generic alternative
4. **MacroSpec / EnvironmentSpec / SpecialsSpec**: Already appropriately generic

### Why These Names Changed

1. **LatexWalker → Parser**:
   - "Walker" is vague (walks what? how?)
   - "Parser" accurately describes what it does
   - Main entry point deserves clear, accurate name

2. **LatexContextDb → NEW LIBRARY SYSTEM** (**ARCHITECTURAL REDESIGN**):

   **DECISION**: Don't just rename `LatexContextDb` - supersede it with a more powerful library system.

   **Problems with pylatexenc's LatexContextDb**:
   - Flat namespace - no organization or modularity
   - No mode-specific definitions (text vs math mode)
   - No conflict resolution between different definition sources
   - No clear library composition or load order
   - Difficult to manage standard vs user definitions

   **New Design: Library System**

   Core concepts:
   - **`Library`**: A collection of macro/environment/specials definitions
   - **`LibrarySet`**: Multiple libraries with defined load order and resolution rules
   - **Mode-specific definitions**: Separate definitions for text mode vs math mode
   - **Name resolution**: Clear rules for handling conflicts
   - **Composability**: Easy to combine standard + user libraries

   Key types in the new system:
   - `Library` - A single library of definitions
   - `LibrarySet` or `LibraryResolver` - Manages multiple libraries with resolution
   - `ModeContext` - Text mode vs Math mode (affects which definitions apply)

   **Naming candidates for the resolver/manager**:
   - `LibrarySet` - Set of libraries with resolution
   - `LibraryResolver` - Resolves definitions across libraries
   - `DefinitionResolver` - Resolves definition lookups
   - `SyntaxResolver` - Resolves syntax definitions

   See `pylatexenc_to_rust_strategy.md` for detailed design.

3. **LatexToken → Token**:
   - "Latex" prefix redundant in a markup-generic library
   - "Token" is universally understood in parsing contexts

4. **StateDelta → ParsingStateDelta**:
   - "StateDelta" too vague - delta of what state?
   - Specificity prevents confusion with other potential state deltas
   - Clear connection to `ParsingState`

5. **ArgumentsSpec → ArgumentStructureSpec**:
   - Too similar to `ArgumentSpec` (only one letter difference!)
   - "Structure" clarifies this defines the structure/pattern of multiple arguments
   - Reduces cognitive load and naming collisions

6. **ParsedArguments → Arguments**:
   - "Parsed" is implied by context (it's the result of parsing)
   - Simpler name, clearer in usage
   - Follows Rust convention of concise type names

## Future Considerations

### If Supporting Non-LaTeX Syntaxes

If `techy` later supports radically different syntaxes (Markdown, etc.), consider:

```rust
// Syntax-specific nodes
pub enum Node {
    // Generic
    Chars(CharsNode),
    Group(GroupNode),
    Comment(CommentNode),

    // LaTeX-specific
    Macro(MacroNode),      // or Command
    Environment(EnvironmentNode),  // or Block
    Math(MathNode),

    // Future: Markdown-specific
    // Heading(HeadingNode),
    // Link(LinkNode),
}
```

Or use trait-based approach with syntax-specific implementations.

## Summary

**Changes to implement:**
1. `LatexWalker` → `Parser` ✓ **DECIDED**
2. `LatexContextDb` → **NEW LIBRARY SYSTEM** 🔄 **ARCHITECTURAL REDESIGN**
   - Not a simple rename - complete redesign of definition management
   - New types: `Library`, `LibrarySet`/`LibraryResolver`, `ModeContext`
   - Support for: mode-specific definitions, library composition, conflict resolution
   - See detailed design in `pylatexenc_to_rust_strategy.md`
3. `LatexToken` → `Token` ✓ **DECIDED**
4. `StateDelta` → `ParsingStateDelta` ✓ **DECIDED**
5. `ArgumentsSpec` → `ArgumentStructureSpec` ✓ **DECIDED**
6. `ParsedArguments` → `Arguments` ✓ **DECIDED**

**Kept as-is:**
- All node type names (`MacroNode`, `EnvironmentNode`, `MathNode`, etc.)
- Individual spec names (`MacroSpec`, `EnvironmentSpec`, `SpecialsSpec`)
- `ArgumentSpec` (individual argument specification)
- `ParsingState`
- `NodeList`, `Node`, and other already-generic types

This provides an optimal balance:
- ✅ Removes "LaTeX" coupling from core API
- ✅ Increases specificity where needed (`ParsingStateDelta`)
- ✅ Reduces naming confusion (`ArgumentStructureSpec` vs `ArgumentSpec`)
- ✅ Maintains clarity with established terms (`MacroNode`, etc.)
- ✅ Simplifies where appropriate (`Arguments`)
- 🔄 `LatexContextDb` → Complete redesign as library system (not just renamed)
