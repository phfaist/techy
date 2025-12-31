# Alignment Audit: Design Docs vs Implementation

**Date:** 2025-12-31
**Status:** All top-level .md files reviewed, all .rs files reviewed

## Executive Summary

The Rust implementation is **well-aligned** with the design strategy documents. All major naming conventions from NAMING_STRATEGY.md have been implemented correctly. However, there are several **unresolved design questions** and **future architectural decisions** that need clarification before proceeding with major new features.

---

## ✅ What's Aligned

### 1. Naming Conventions (NAMING_STRATEGY.md) - **FULLY IMPLEMENTED**

All core naming decisions have been correctly implemented:

| Strategy Document | Implementation | Status |
|-------------------|----------------|--------|
| `Parser` (not `LatexWalker`) | `src/parser/mod.rs` ✓ | ✅ Correct |
| `Token` (not `LatexToken`) | `src/token/mod.rs` ✓ | ✅ Correct |
| `ContextDb` (not `LatexContextDb`) | `src/spec/mod.rs` ✓ | ✅ Correct |
| `ParsingStateDelta` (not `StateDelta`) | `src/state/mod.rs` ✓ | ✅ Correct |
| `ArgumentStructureSpec` (not `ArgumentsSpec`) | `src/spec/mod.rs` ✓ | ✅ Correct |
| `Arguments` (not `ParsedArguments`) | `src/node/mod.rs` ✓ | ✅ Correct |
| Module `parser` (high-level API) | `src/parser/mod.rs` ✓ | ✅ Correct |
| Module `constructs` (construct parsers) | `src/constructs/mod.rs` ✓ | ✅ Correct |

**Node names** - All kept as designed:
- `MacroNode`, `EnvironmentNode`, `CharsNode`, `GroupNode`, `CommentNode`, `MathNode`, `SpecialsNode` ✓

**Re-exports in lib.rs** - Correct:
```rust
pub use node::{Arguments, Node, NodeList};
pub use spec::{ArgumentSpec, ArgumentStructureSpec, ContextDb, ...};
pub use state::{ParsingState, ParsingStateDelta};
pub use token::{Span, Token, TokenType};
```

### 2. Module Organization - **CORRECT**

The module structure follows pylatexenc_to_rust_strategy.md:

```
src/
├── lib.rs              ✓ Public API exports
├── error.rs            ✓ Error types
├── token/              ✓ Tokenization
│   ├── mod.rs          ✓ Token, Span, TokenType
│   └── reader.rs       ✓ StringTokenReader
├── node/               ✓ AST nodes
│   └── mod.rs          ✓ All node types
├── spec/               ✓ Extensibility
│   └── mod.rs          ✓ MacroSpec, EnvironmentSpec, ContextDb
├── state/              ✓ Parsing state
│   └── mod.rs          ✓ ParsingState, ParsingStateDelta
├── parser/             ✓ High-level API
│   └── mod.rs          ✓ Parser struct (main entry point)
└── constructs/         ✓ Construct parsers
    ├── mod.rs          ✓ Parser trait
    └── general.rs      ✓ GeneralNodesParser
```

### 3. Core Architecture - **IMPLEMENTED AS DESIGNED**

Following pylatexenc_to_rust_strategy.md:

- ✅ Token-based parsing with `TokenReader` trait
- ✅ Node-based AST with enum variants
- ✅ Specification system (MacroSpec, EnvironmentSpec)
- ✅ Context database (ContextDb)
- ✅ Parsing state with deltas
- ✅ Parser trait system
- ✅ Error handling with Result<T, E>

### 4. Type Safety Improvements - **DELIVERED**

Rust improvements over Python as planned:
- ✅ Enums instead of isinstance() checks
- ✅ Option<T> instead of Optional[X]
- ✅ Arc<T> for shared specs
- ✅ Lifetimes for ParsingState<'ctx>
- ✅ Result<T, E> for error handling
- ✅ Pattern matching on tokens and nodes

---

## ⚠️ Unresolved Design Questions

### 1. **ContextDb Future: Library System** ⏸️ UNDER DISCUSSION

**Current Status:**
- NAMING_STRATEGY.md notes: "ContextDb - UNDER DISCUSSION: name may not be specific enough"
- PROPOSALS.md contains detailed design for replacement "Library System"
- Current implementation uses simple `ContextDb` as placeholder

**The Question:**
Should we implement the full Library System now, or continue with ContextDb?

**Library System Design (from PROPOSALS.md):**
```rust
// Proposed replacement for ContextDb
pub struct Library { ... }           // A collection of definitions
pub struct LibrarySet { ... }        // Manages multiple libraries
pub enum ConflictStrategy { ... }    // How to handle conflicts
pub enum Mode { Text, Math }         // Mode-specific definitions
```

**Benefits of Library System:**
- Mode-aware definitions (text vs math mode)
- Layered libraries (standard + user)
- Conflict resolution strategies
- Better organization

**Current ContextDb Limitations:**
- Flat namespace
- No mode awareness
- No conflict detection
- No modularity

**Decision Needed:**
1. Keep ContextDb as-is (simple, works for MVP)
2. Implement Library System now (future-proof, more complex)
3. Hybrid: Add mode support to ContextDb, defer full Library System

**Recommendation:**
Decision Point #1 in this audit - need to choose before implementing advanced features.

---

### 2. **Trait-Based Architecture** ⏸️ DESIGN READY, NOT IMPLEMENTED

**Current Status:**
- TRAIT_BASED_ARCHITECTURE.md contains complete design
- TRAIT_ARCHITECTURE_QUICKREF.md provides implementation roadmap
- Current implementation uses **concrete types** (simple approach)

**The Question:**
Should we migrate to trait-based architecture for extensibility?

**Current Implementation:**
```rust
// Concrete enum
pub enum Node {
    Chars(CharsNode),
    Macro(MacroNode),
    // ... fixed variants
}
```

**Trait-Based Proposal:**
```rust
// Extensible trait
pub trait Node { ... }

// Users can add custom node types
impl Node for MyCustomNode { ... }
```

**Trade-offs:**

| Aspect | Current (Concrete) | Proposed (Trait-based) |
|--------|-------------------|------------------------|
| Simplicity | ✅ Very simple | ⚠️ More complex |
| Extensibility | ❌ Limited | ✅ Maximum |
| Compile time | ✅ Fast | ⚠️ Slower (monomorphization) |
| Binary size | ✅ Small | ⚠️ Larger |
| Runtime perf | ✅ Fast | ✅ Fast (static) / ⚠️ Slower (dynamic) |

**Implementation Phases (from TRAIT_ARCHITECTURE_QUICKREF.md):**
- Phase 1: Foundation (Define traits) - Week 1-2
- Phase 2: Generification (NodeList<N>, Arguments<N>) - Week 3-4
- Phase 3: Parser Generification (Parser<N,S,C>) - Week 5
- Phase 4: Dynamic Extensions (Registry) - Week 6
- Phase 5: Documentation - Week 7
- Phase 6: Performance Testing - Week 8

**Decision Needed:**
1. Stick with concrete types (simple, works for most users)
2. Implement trait-based architecture (extensibility for power users)
3. Hybrid: Keep concrete types as default, add opt-in trait layer

**Recommendation:**
Decision Point #2 - This is a 8-week project if we choose to do it. Should we?

---

### 3. **Source Tracking & Provenance** ⏸️ PROPOSAL ONLY

**Current Status:**
- PROPOSALS.md Section 2 contains detailed design
- Current implementation uses simple `Span { start, end }`

**The Question:**
Do we need rich source location tracking beyond byte offsets?

**Current:**
```rust
pub struct Span {
    pub start: usize,
    pub end: usize,
}
```

**Proposed:**
```rust
pub enum Source {
    File { path: PathBuf, hash: Option<u64> },
    Url(String),
    String { name: Option<String> },
    Synthetic { generator: String, parent: ... },
}

pub struct SourceLocation {
    pub source: Arc<Source>,
    pub span: Span,
    pub line_col: Option<(usize, usize)>,
}
```

**Use Cases:**
- Multi-file documents (`\input`, `\include`)
- Web-fetched content
- Better error messages (file:line:col)
- IDE integration

**Decision Needed:**
Is this needed for your use case? Or is simple byte-offset Span sufficient?

**Recommendation:**
Decision Point #3 - Defer unless multi-file parsing is a requirement.

---

### 4. **Generic Nodes & Custom State** ⏸️ PROPOSAL ONLY

**Current Status:**
- PROPOSALS.md Section 3 contains design
- TRAIT_BASED_ARCHITECTURE.md also addresses this
- Not implemented

**The Question:**
Do extensions (Python bindings, custom tools) need to attach custom data?

**Proposed:**
```rust
// Option A: Generic nodes (compile-time)
pub struct MacroNode<Ext: NodeExtension> {
    // ... standard fields
    pub ext: Ext,  // User-defined extension data
}

// Option B: Type-erased (runtime)
pub struct MacroNode {
    // ... standard fields
    pub ext: NodeExtData,  // Box<dyn Any>
}
```

**Use Cases:**
- Python bindings need to attach PyObject
- Custom annotations for syntax highlighting
- Statistics tracking
- Domain-specific metadata

**Decision Needed:**
1. No extension support (current - simple)
2. Type-erased extension data (flexible, small overhead)
3. Generic extension parameter (zero-cost, complex)

**Recommendation:**
Decision Point #4 - Defer unless building language bindings or plugin system.

---

### 5. **Full TeX Compliance** ⏸️ GAP ANALYSIS DONE

**Current Status:**
- PROPOSALS.md Section 4 contains complete gap analysis
- Intentionally NOT aiming for full TeX compliance

**Key Missing Features for Full TeX:**
- ❌ Catcodes (Very High difficulty, Low value)
- ❌ Macro expansion & `\def` (Very High difficulty, Medium value)
- ❌ Conditionals (`\if`, `\ifx`) (High difficulty, Low value)

**Intentional Design Decision:**
techy is **NOT a TeX engine**, it's a **LaTeX-like markup parser** for:
- Conversion tools (LaTeX → HTML, Markdown)
- Document analysis
- Syntax highlighting
- AST-based transformations

**Easy Wins Identified:**
- ✅ Additional standard macros (accents, line breaks, spacing)
- ✅ Ligature detection
- ✅ More verbatim environments

**Decision Needed:**
Accept this as documented limitation? Or need to support specific TeX features?

**Recommendation:**
Decision Point #5 - Explicitly document scope boundaries in README.

---

## 🔍 Points Needing Clarification

### 1. **LatexNodeTrait vs Node Enum**

**Found in:** `src/node/mod.rs`

```rust
pub trait LatexNodeTrait {
    fn span(&self) -> Span;
    // ...
}

pub enum Node {
    Chars(CharsNode),
    // ...
}

impl LatexNodeTrait for Node { ... }
```

**Question:** Why have both `LatexNodeTrait` and the concrete `Node` enum?

**Possible Answers:**
1. Preparatory work for trait-based architecture
2. Legacy from earlier design iteration
3. Intentional dual-path support

**Clarification Needed:** Is this intentional? Should we remove the trait or keep it for future extensibility?

---

### 2. **ArgumentValue Complexity**

**Found in:** `src/node/mod.rs`

```rust
pub enum ArgumentValue {
    Node(Box<Node>),
    NodeList(NodeList),
    Verbatim(String),
    None,
}
```

**Question:** Why does an argument value have 4 variants instead of just Node/NodeList?

**Design Documents Say:**
- pylatexenc_to_rust_strategy.md shows: `Vec<(String, Option<Node>)>`
- Current implementation is more complex

**Clarification Needed:**
- Is `Verbatim` for `\verb|...|` support? (TODO.md lists verbatim as planned)
- Is `None` for optional arguments that weren't provided?
- Should this match the original simpler design?

---

### 3. **Module Naming: `parser` vs `constructs`** ✅ RESOLVED

**Decision Made:** Module renamed from `parsing` → `constructs`

**Current Implementation:**
- `src/parser/mod.rs` - High-level API (Parser struct)
- `src/constructs/mod.rs` - Parsers for individual constructs (Parser trait + implementations)

**Rationale:**
- `constructs` clearly describes content: parsers for individual LaTeX constructs
- Distinct from `parser` - no confusion
- Semantic: "construct" is well-understood in language parsing
- Natural organization: `constructs/macro.rs`, `constructs/environment.rs`, etc.

**Benefits:**
- ✅ Clear separation of concerns
- ✅ High-level users never see low-level trait
- ✅ Semantically accurate naming
- ✅ No confusion between `parser` and `constructs`

---

### 4. **Testing Gap: 39/40 Passing**

**Found in:** CLAUDE.md and NAMING_STRATEGY.md
- "cargo test (39/40 tests passing - 1 pre-existing failure)"

**Clarification Needed:**
What's the 1 failing test? Is it:
1. A known TODO (unimplemented feature)
2. A bug that needs fixing
3. An expected failure for a design decision

**Action:** Should identify and document the failing test.

---

### 5. **Standard Library Completeness**

**Found in:** `src/spec/mod.rs` - `add_standard_definitions()`

Currently defined:
- Text formatting: `textbf`, `textit`, `emph`, `texttt`, `underline`
- Sectioning: `section`, `subsection`, `subsubsection`, `chapter`, `part`
- References: `label`, `ref`, `cite`
- Math: `frac`, `sqrt`
- Environments: `equation`, `align`, `itemize`, `enumerate`, `center`, `quote`
- Specials: `&`, `~`, `#`, ` `` `, `''`

**PROPOSALS.md suggests adding:**
- Accents: `\'`, `` \` ``, `\"`, `\^`, `\~`, `\=`
- Line breaks: `\\`, `\par`
- Spacing: `\hspace`, `\vspace`
- Many more standard LaTeX commands

**Clarification Needed:**
What's the target completeness level for standard library?
1. Minimal (current - just enough to parse basic docs)
2. Standard LaTeX (all common commands)
3. Extended (LaTeX + common packages)

---

## 📋 Recommendations

### Immediate Actions

1. **Document Failing Test**
   - Identify the 1/40 failing test
   - Add comment explaining status (TODO vs bug vs expected)

2. **Clarify LatexNodeTrait**
   - Either remove it (if unused) or document its purpose
   - If keeping for future extensibility, add comment referencing TRAIT_BASED_ARCHITECTURE.md

3. **Document Scope Boundaries**
   - Add section to README.md: "What techy is NOT"
   - Link to PROPOSALS.md Section 4 for TeX compliance gap analysis

4. **Reconcile ArgumentValue Design**
   - Verify if current 4-variant design is intentional
   - Update pylatexenc_to_rust_strategy.md if design evolved

### Major Design Decisions Needed

**Priority 1: Library System vs ContextDb**
- **Impact:** High - affects all future extensibility work
- **Effort:** Medium - PROPOSALS.md has complete design
- **Timeline:** Decide before implementing argument parsing

**Decision Options:**
- A. Keep ContextDb, add mode awareness (incremental)
- B. Implement full Library System (future-proof)
- C. Hybrid: ContextDb + mode support, LibrarySet later

**Recommendation:** Option A for now (mode awareness), Option B when needed.

---

**Priority 2: Trait-Based Architecture**
- **Impact:** Very High - affects all APIs
- **Effort:** High - 8 weeks estimated
- **Timeline:** Decide before 1.0 release

**Decision Options:**
- A. Stay with concrete types (simple, sufficient for 90% of users)
- B. Implement trait-based architecture (power users, extensibility)
- C. Both: Concrete types as default, traits as opt-in

**Recommendation:** Option A for 1.0, Option C for 2.0 if demand exists.

---

**Priority 3: Standard Library Completeness**
- **Impact:** Medium - affects out-of-box usability
- **Effort:** Low - just adding more specs
- **Timeline:** Can be done incrementally

**Decision Options:**
- A. Minimal (current)
- B. Standard LaTeX (~100 common commands)
- C. Extended (packages like amsmath, tikz)

**Recommendation:** Option B - add common commands from PROPOSALS.md "Easy Wins"

---

**Priority 4: Source Tracking & Custom Extension Data**
- **Impact:** Low - only needed for specific use cases
- **Effort:** Medium
- **Timeline:** Defer until needed

**Recommendation:** Defer both unless specific use case identified.

---

## ✅ Action Items

1. **Add SCOPE.md** - Clarify what techy is/isn't
   - Link TeX compliance gap analysis
   - Set user expectations
   - Reference related projects for missing features

2. **Create DECISIONS.md** - Document architectural decisions
   - Why concrete types vs traits?
   - Why ContextDb (for now)?
   - Why simple Span vs rich SourceLocation?
   - Link to proposal docs for future options

3. **Update TODO.md** - Align with design decisions
   - Remove items that are design proposals (move to PROPOSALS.md)
   - Keep only items that are decided to be implemented
   - Add priority levels

4. **Fix Documentation Inconsistencies**
   - Update pylatexenc_to_rust_strategy.md module structure (split parser/parsing)
   - Ensure all examples use correct names (Parser not LatexWalker)
   - Add cross-references between related docs

5. **Enhance README.md**
   - Add "Architecture Decision Records" section
   - Link to NAMING_STRATEGY.md
   - Link to TRAIT_BASED_ARCHITECTURE.md for extensibility info
   - Add "What's Implemented" vs "What's Planned" sections

---

## 🎯 Summary

**Overall Alignment: GOOD (8/10)**

**Strengths:**
- ✅ All naming conventions correctly implemented
- ✅ Module organization follows strategy
- ✅ Core architecture matches design docs
- ✅ Type safety improvements delivered
- ✅ Clean separation of concerns

**Areas Needing Attention:**
- ⚠️ Several major design decisions still open (Library System, Traits, etc.)
- ⚠️ Some minor inconsistencies between docs
- ⚠️ Scope boundaries not clearly documented
- ⚠️ Future extensibility path unclear

**Critical Path:**
1. Make design decisions (Library System, Traits)
2. Document scope clearly
3. Align all docs with decisions
4. Continue implementation based on chosen path

**Recommendation:**
The project has excellent foundations. Before adding major features (argument parsing, environments, etc.), clarify the 5 design decision points above to ensure we're building on the right architecture.
