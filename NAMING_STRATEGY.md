# Naming Strategy for techy

Living document recording the naming rules and the current authoritative names for the
`techy` crate. **Last updated July 2026** to incorporate the resolved decisions of
[ARCHITECTURE.md](ARCHITECTURE.md) §7 (which this document must stay consistent with).
Superseded names and the reasons they changed are collected at the end; the full history
of earlier revisions lives in git.

## Design Principles

1. **Generic over specific** — no `Latex` prefixes anywhere (`Token`, not `LatexToken`;
   `Parser`-family names, not `LatexWalker`). The library targets LaTeX-*like* languages;
   the familiar LaTeX behavior is a *preset*, and LaTeX-flavored names live there.
2. **Specificity matters** — `ParsingStateDelta`, not `StateDelta` (delta of *what*?).
3. **Clarity over brevity** — `ArgumentStructureSpec`, not `ArgumentsSpec` (one letter away
   from `ArgumentSpec` is not enough distance).
4. **Context determines names** — `Arguments`, not `ParsedArguments` (in a parser's output,
   "parsed" is implied).
5. **Registry naming rule** (systematic across the crate):
   `…Kind` = closed core enum, exhaustively matchable (`TokenKind`, `NodeKind`);
   `…TypeId` = open registry, interned in `Language`, preset-registered
   (`GroupTypeId`, `CallableTypeId`).
6. **Transitions read as adjectives** — `ParsingState::derived()` per Rust's
   `to_uppercase` convention: signals a *transition* producing a new value, not a field copy.

## Current Authoritative Names (July 2026)

### Modules / layers (ARCHITECTURE.md §3)

| Layer | Module | Contents |
|---|---|---|
| L7 | `presets` (`techy::latexlike`) | `Latexlike` lang, LaTeX-flavored helpers |
| L6 | `engine` | `Language<L>`, `ParserSession`, `ParseResult`, `NodeRef` |
| L5 | `constructs` | `ConstructParser` trait + standard construct parsers |
| L4 | `node` | `NodeTree`, `NodeKind<L>`, `CallableData`, `TextContent`, ext payloads |
| L3 | `spec` + `library` | `CallableSpec`, `StdCallableSpec`, `CallableTypeId`, `Library`, `LibraryStack` |
| L2 | `state` | `ParsingState<L>`, `StateData`, `TokenRules`, `ParsingStateDelta` |
| L1 | `token` | `Token<'s>`, `TokenKind`, `TokenReader`, `StdTokenReader` |
| L0 | `source` | `Source`, `SourceSpan`, `SourceProvenance`, `SourceResolver`, `LineIndex` |
| — | `error` | span-based diagnostics, recovery tokens |

### Core types

| Concept | Name | Notes |
|---|---|---|
| Compile-time type bundle | `Lang` (trait) | one generic parameter everywhere: `L: Lang` |
| Runtime config bundle | `Language<L>` | "define a language once, parse many documents" |
| High-level entry point | `Language::parse()` | a convenience `Parser` struct on top is a deferred bikeshed |
| Parse session / result | `ParserSession`, `ParseResult` | session is transient; `finish()` freezes |
| Parsing state | `ParsingState<L>` over private `StateData<L>` | getters are the public surface |
| Tokenization data | `TokenRules` | plain stored data inside `StateData` |
| State change value | `ParsingStateDelta<L>` | overrides-struct + `L::Event`s; a value, not a closure |
| State transition | `ParsingState::derived()` | the sole constructor of non-initial states |
| Transition customizer | `Lang::finalize_transition` | the choke-point hook |
| Byte range | `Span` | `Copy`, no `Arc`; transient parsing use |
| Arc-carrying range | `SourceSpan` | replaces lifetime-bound source locations in nodes/errors |
| Tokens | `Token<'s>`, `TokenKind<'s>` | `…Kind` per the registry rule |
| Token reading | `TokenReader<'s, L>` (trait), `StdTokenReader` | trait = behavior extension point |
| Callable behavior | `CallableSpec<L>` (trait), `StdCallableSpec` | de-keyed: no name, no invocation form |
| Invocation-form registry | `CallableTypeId` | interned in `Language`, like `GroupTypeId` |
| Argument/slot structure | `ArgumentStructureSpec`, `SlotStructureSpec` | args configure; slots hold content regions |
| Parsed argument values | `Arguments` / `ArgsLayout`, `SlotsLayout` | context makes "parsed" obvious |
| Definition lookup | `SpecLookup<L>` (trait), `Library<L>`, `LibraryStack<L>` | ordered stack, lexical shadowing; no `ConflictStrategy` |
| Construct parser trait | `ConstructParser<L>` | avoids clashing with any high-level parser type |
| Parser context | `ParseContext<'a, 's, L>` | bundles tokens + state + session |
| Node storage | `NodeTree<L>`, `NodeData<L>`, `NodeRef<'pr>` | flat, frozen, index-based; proxy access |
| Node taxonomy | `NodeKind<L>`: `Chars` / `Group` / `Callable` / `Comment` / `List` | closed structural core; no `Custom` variant |
| Callable payload | `CallableData<L>` | invocation form + spelling + spec + args/slots |
| Node textual payload | `TextContent` (`Spanned` / `Owned`) | logical content first-class; span = provenance |
| Node ext types | `NodeExt` (uniform) + `CharsNodeExt`, `GroupNodeExt`, `CallableNodeExt`, `CommentNodeExt`, `ListNodeExt`; bundled as `Lang::NodeExts: NodeExtTypes` | `SimpleLang` defaults them all to `()` |
| Source model | `Source`, `SourceSpan`, `SourceProvenance`, `SourceResolver`, `SourceContent`, `SourceCursor`, `LineIndex` | per SOURCE_ARCHITECTURE.md |
| Origin metadata | `SourceOrigin` (trait); default impl on `Option<String>` | no named `Std…` type: the default origin is a plain optional URL string (July 2026 revision) |
| Resolvers | `NoResolver` (ZST default), `MapResolver`, `ResolveError` | per SOURCE_ARCHITECTURE.md; no `FileResolver` — file I/O lives with the embedder (no_std policy) |
| Diagnostics | `Diagnostic`, `Diagnostics`, `Severity`, `Recovery` | span-based; `Recovery` = tolerant-parsing policy (strict/tolerant) |

### Preset-layer names (`techy::latexlike`)

"Macro", "environment", and "specials" are **invocation forms, not core concepts**. The
familiar names survive in the preset layer only:

- `Latexlike` — ZST implementing `Lang`.
- `MacroSpec` / `EnvironmentSpec` / `SpecialsSpec` — constructor helpers producing
  `StdCallableSpec`s.
- `MACRO` / `ENVIRONMENT` / `SPECIALS` — the preset's registered `CallableTypeId`s.
- `NodeRef` accessor sugar (`as_math()`-style environment/macro views over `Callable` nodes).

Type aliases (`type LatexParseResult = ParseResult<Latexlike>` …) keep simple usage
generics-free.

## Superseded Names

Decided July 2026 unless noted; rationale in ARCHITECTURE.md §4/§4b and DESIGN_RATIONALE.md.

| Old name | Superseded by | Why |
|---|---|---|
| `LatexWalker` → `Parser` struct | `Language::parse()` entry point | "walker" vague; whether a convenience `Parser` struct remains is deferred |
| `LatexContextDb` / `ContextDb` | `Library`, `LibraryStack`, `SpecLookup` | flat namespace → ordered stack with lexical shadowing |
| `LibrarySet`, `LibraryResolver`, `ModeContext` | `LibraryStack` + state-aware `SpecLookup` | hard-coded mode tables rejected (no privileged modes); `ConflictStrategy` dropped — shadowing *is* the semantic |
| `LanguageSpecification` | `Lang` | too long for a parameter appearing everywhere |
| `FLMEnvironment` | `Language<L>` | fatal collision with LaTeX environments |
| `TokenType` | `TokenKind` | registry naming rule (`…Kind` = closed enum) |
| `StringTokenReader` | `StdTokenReader` | driven by `TokenRules` data, not tied to `String` input |
| `StateDelta` (trait) / `StandardDelta` (enum) | `ParsingStateDelta<L>` (struct of optional overrides + events) | deltas are reified values; no apply/trait machinery |
| `TokenizationState` / per-facet state traits | `TokenRules` stored in `StateData<L>` | materialized state + transition choke point (Decision 1) |
| `Node` enum with `MacroNode`, `EnvironmentNode`, `SpecialsNode` variants | `NodeKind::Callable` + `CallableTypeId` | Macro/Environment/Specials differ by invocation form, not parsed shape (Decision 3) |
| `MathNode` | `Group` with math `GroupTypeId` + preset state ext | no privileged math mode in the core |
| `CharsNode`, `GroupNode`, `CommentNode`, `NodeList` (struct-per-type) | `NodeKind::{Chars, Group, Comment, List}` in flat `NodeTree` | flat index-based storage, `NodeRef` proxies |
| `ParsedArguments` | `Arguments` | "parsed" implied by context (Dec 2025) |
| `ArgumentsSpec` | `ArgumentStructureSpec` | too close to `ArgumentSpec` (Dec 2025) |
| `Parser` trait (in `constructs`) | `ConstructParser` | avoids clash with high-level parser type |
| `SourceLocation<'src>` | `SourceSpan` | Arc spans remove the `'src` lifetime infection |
| "namespace", `CallableKind` | `CallableTypeId` | "namespace" confusable with package/library; open registry ⇒ `…TypeId` |
| `GroupExt`, `NodeGroupExt` | `GroupNodeExt` (etc.) | `GroupExt` too vague; `NodeGroupExt` parses wrong |
| `parser` module (high-level API) | `engine` module | layered architecture of ARCHITECTURE.md §3 |
| `apply()` / `copy_with()` | `ParsingState::derived()` | adjective form; signals a transition |

## Module Rename History

- **`parsing` → `constructs`** (Dec 2025): `parsing` was too close to `parser`;
  "constructs" describes the content — parsers for individual constructs. The name and
  rationale carry over unchanged into the L5 layer.
- **`walker` → `parser` → `engine`** (Dec 2025, then July 2026): the high-level API module
  was first renamed from `walker` to `parser`; the July 2026 architecture places the
  high-level machinery (`Language<L>`, `ParserSession`) in `engine` instead.
