# Parsing Strategy: State-Based Architecture

DOCUMENT STATUS: IDEAS, NOT FINAL DECISIONS. I AM ALREADY RECONSIDERING
DECISIONS IN THIS DOCUMENT.  DOCUMENT IS NOT UP TO DATE.


## Core parsing concepts

The parser is designed to be extremely extensible and be used to parse markup-type
structured languages like as pylatexenc's LaTeX-inspired language.
The core concepts are:

- content characters - stores document content
- a macro with some arguments (whose structure is specified by the language)
- "callable objects", including macros, which are allowed to dynamically modify
  the tokenization rules and specify how to parse their invocation (i.e. their
  arguments and/or body). These include macros, environments, and special
  character sequences ("specials").
- comments (single-line, at least for now)
- environments (\begin{env}...\end{env}, but think perhaps also an XML tag
  block). Is a "callable object".
- specials (any string, say "~", "&", "---", perhaps "\n\n").
  is a "callable object"
- groups — these are a collection of content, including chars, macros,
  environments, specials, comments, etc., and which are usually delimited
  by some particular construct or characters.  A group is noncallable.
  (Think `{...}` in LaTeX)


The core parser code identifies simple tokens (that are context-aware!) and
delegates the parsing of the language constructs to individual construct
parsers.  The construct parsers are specified through some generic trait
implementation (LanguageSpecification), see below.


## Design decision

**techy eliminates privileged parsing modes** (including math mode) in favor of a state architecture in which the parser delegates construct parsing to other parsers
which are specified by a language core specification object as well as a library of known callable specifications.

## Key Principle

All parsing contexts—including what pylatexenc calls "math mode"—are represented as **parsing state** manipulated through **state deltas**. No language-specific concepts are hard-coded into the parser core.

---

## Language Specification Architecture

The parser core is generic over a `LanguageSpecification` trait that bundles all language-specific types:

```rust
pub trait LanguageSpecification {
    type TokenizationState: TokenizationState;
    type ParsingState: ParsingState;
    type NodeTypes: NodeTypes;
    type SourceOrigin: SourceOrigin;
}
```

This single trait provides all type information needed by the parser core, avoiding proliferation of generic parameters.

---

## Tokenization Context

The `TokenizationState` trait provides information needed to tokenize source strings:

```rust
pub trait TokenizationState {
    fn macro_escape_char(&self) -> char;
    fn macro_alpha_chars(&self) -> &str;
    fn group_delimiters(&self) -> &[(String, String)];
    fn enable_environments(&self) -> bool;
    fn enable_comments(&self) -> bool;
    fn comment_char(&self) -> Option<char>;
    fn special_strings(&self) -> &[String];
}
```

**Key point:** The tokenization context does **not** contain library/macro definitions. It only provides structural information about how to break strings into tokens. Special strings (like `$`, `&`, `~`) are listed, but their semantic meaning is determined by parsers, not the tokenizer.

**Token type:** In pylatexenc, the token type contained rich information beyond individual language tokens to actual semantics (e.g. comment content, environment name in `\begin{environment}`). Here, we'll switch tactics: the token only contains information about the (near-)minimal string piece that identifies what type of object to parse next (e.g. `\macroname`, `\begin`, `\end`, `%`, `{`). The parser can then delegate to core construct parsers for macro, environment, group, specials, comment, etc.

---

## Parsing State

Parsing state is separate from tokenization context and contains library-defined state:

```rust
pub trait ParsingState {
    type TokenizationState: TokenizationState;

    fn tokenization_context(&self) -> &Self::TokenizationState;
    fn library_set(&self) -> &dyn LibrarySet;
}
```

Custom libraries, built-in more language-specific parsing states (e.g.
latex-like language) will have trait definitions which include additional
fields such as `in_math_mode`, etc.


### State Deltas

All state changes occur through explicit deltas:

```rust
pub trait StateDelta {
    fn apply<S: ParsingState>(&self, state: & S) -> S; // or something to this effect
}
```

State deltas are fully custom—libraries define what state changes they need.  They
return the new parsing state.

---

## Math Mode Handling

Math mode delimiters (`$`, `\[`, `\(`) are registered as **special strings** in the tokenization context. The standard library provides construct parsers that handle these tokens.

Math parsers create **local state** for parsing math content. The "math mode" concept is defined by the library through state extensions, not by the parser core.

---

## State-Aware Tokenization

The tokenizer is state-aware and consults the `TokenizationState`:

```rust
pub trait TokenReader<LS: LanguageSpecification> {
    fn next_token(
        &mut self,
        tokenization_context: &LS::TokenizationState,
    ) -> Result<Option<Token>>;
}
```

Tokenization behavior depends on the tokenization context:

- **Character interpretation**: Which characters are special depends on context
- **Active characters**: Context determines which characters trigger special tokenization
- **Catcode-like behavior**: Character categories vary by context
- **Context-specific tokenization**: Different token rules in different parsing contexts

The tokenization context is obtained from the parsing state via `state.tokenization_context()`.

---

## Differences from pylatexenc

### pylatexenc Approach

pylatexenc has **privileged math mode** in `ParsingState`:

```python
# pylatexenc
class LatexWalkerParsingState:
    def __init__(self, in_math_mode=False, latex_context=None):
        self.in_math_mode = in_math_mode  # Hard-coded boolean
        self.latex_context = latex_context
```

Math mode is **baked into the core parser**.

### techy Approach

techy has **no privileged modes**. The parser core is generic over a `LanguageSpecification` trait:

```rust
pub trait LanguageSpecification {
    type TokenizationState: TokenizationState;
    type ParsingState: ParsingState;
    type NodeTypes: NodeTypes;
    type SourceOrigin: SourceOrigin;
}
```

All language-specific behavior is provided through these traits. Math mode is defined by the standard library via state extensions, not the parser core.

### Summary of Differences

| Aspect | pylatexenc | techy |
|--------|-----------|-------|
| Architecture | Hard-coded LaTeX-specific types | Generic over `LanguageSpecification` trait |
| Math mode | Hard-coded `in_math_mode: bool` | Library-defined state extension |
| State deltas | Explicit `EnterMathMode`/`ExitMathMode` classes | Specific `StateDelta` trait implementations |
| Tokenization | Implicit LaTeX tokenization rules | Explicit `TokenizationState` trait |
| Extensibility | Subclass state delta types | Implement specification traits |
| Custom languages | Must follow LaTeX state model | Define complete `LanguageSpecification` |

---

## Benefits of Language Specification Architecture

### 1. True Language Independence

Zero language-specific concepts in the parser core. All language behavior is defined through the `LanguageSpecification` trait and its associated types.

### 2. Type Bundling

A single `LanguageSpecification` trait bundles all language-specific types, avoiding proliferation of generic parameters throughout the codebase.

### 3. Explicit Contracts

Each trait (`TokenizationState`, `ParsingState`, `NodeTypes`, `SourceOrigin`) defines a clear contract for what the parser core needs from that component.

### 4. Separation of Concerns

Tokenization context is separate from parsing state. Tokenization rules don't depend on library definitions, and vice versa.

### 5. Full Extensibility

Libraries define arbitrary state via extensions and manipulate it through custom state deltas. No privileged concepts like "math mode" exist in the parser core.

### 6. Compatibility with pylatexenc Design

The overall parsing architecture remains compatible with pylatexenc:

- **Construct parsers** parse individual language elements
- **State deltas** represent state transitions
- **Libraries** manage macro/environment/specials definitions
- **Token-based parsing** drives the parser

The key difference: **techy generalizes** what pylatexenc hard-codes.

---

## Conclusion

techy eliminates privileged parsing modes in favor of a **generic, trait-based architecture**. The parser core is generic over a `LanguageSpecification` trait that bundles all language-specific types.

This architecture provides:

- ✅ **True language independence**—zero hard-coded language concepts
- ✅ **Type bundling**—single `LanguageSpecification` trait avoids generic parameter proliferation
- ✅ **Separation of concerns**—tokenization, parsing state, nodes, and source tracking are independent
- ✅ **Full extensibility**—libraries define arbitrary state and deltas
- ✅ **Explicit contracts**—clear trait boundaries define component responsibilities

The parser core is a generic engine. Language semantics—including what pylatexenc calls "math mode"—are library concerns implemented through the specification traits, not parser concerns.
