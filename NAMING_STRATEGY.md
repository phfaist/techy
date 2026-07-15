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
3. **Clarity over brevity** — `TokenResult`, not `TokResult`; `ParsedArguments`, not
   `Arguments` (the spec-side vocabulary `ArgumentSpec`/`ArgumentParserSpec` coexists in
   scope, so the parsed-side types carry the distinguishing prefix — July 2026 revision of
   the Dec 2025 "context makes parsed obvious" call, which predated the richer spec-side
   argument vocabulary).
4. **Context determines names** — but only when no sibling vocabulary competes in the same
   scope (see the `ParsedArguments` reversal under principle 3).
5. **Id naming rule** (systematic across the crate):
   `…Kind` = closed core enum, exhaustively matchable (`TokenKind`, `NodeKind`);
   `…TypeId` = per-language *classification*, an associated type on `Lang`
   (`Lang::GroupTypeId`, `Lang::CallableTypeId`) — typically a small closed enum in a real
   language definition, `u32` under `SimpleLang`; classifies (group classes, invocation
   forms), never identifies a delimiter spelling. (July 2026 revisions: formerly open
   registry ids interned in `Language`; then per-delimiter-pair identities.)
6. **Transitions read as adjectives** — `ParsingState::derived()` per Rust's
   `to_uppercase` convention: signals a *transition* producing a new value, not a field copy.

## Current Authoritative Names (July 2026)

### Modules / strata (ARCHITECTURE.md §3 — modules are topics, not dependency ranks)

| Stratum | Module | Contents |
|---|---|---|
| S2 | `latexlike` preset | `Latexlike` lang, LaTeX-flavored helpers |
| S1 | `engine` | `Language<L>`, `ParserSession`, `ParseResult`, `NodeRef` |
| S1 | `constructs` | `ConstructParser` trait + standard construct parsers |
| S1 | `node` | `NodeTree`, `NodeKind<L>`, `GroupData`, `CallableData`, `ParsedArguments`/`ParsedSlots`, `NodeRef`, `NodeTreeBuilder`, ext aliases |
| S1 | `spec` + `library` | `CallableSpec`, `StdCallableSpec`, `ArgumentSpec`, `ArgumentParser`, `Library`, `LibraryStack`, `CallableQuery` |
| S1 | `state` | `Lang`, `ParsingState<L>`, `StateData`, `ParsingStateDelta` |
| S1 | `token` | `Token<'s, L>`, `TokenKind`, `TokenRules`, `TokenReader`, `StdTokenReader` |
| S0 | `source` | `Source`, `Span`, `SourceSpan`, `SourceProvenance`, `SourceResolver`, `LineIndex`, `TextContent` |
| S0 | `error` | span-based diagnostics |

### The command → callable → macro/environment terminology stack

Each term is scoped to its stratum; using one at the wrong level is a naming bug:

- **Command** — *token-level syntactic form*: escape char + name (`TokenKind::Command`,
  `CommandRule`). TeX lineage ("control sequence"). `\begin` is a command; so is `\foobar`.
  Not "escape" (a future `@MARKER@`-style syntax would have no escape character; and
  "escape token" wrongly suggests escaped-character semantics). Not "macro" (that is a
  preset concept).
- **Callable** — *parse-level concept* (Decision 3): anything invocable, resolved to a
  `CallableSpec` with a `CallableTypeId` invocation form. Both command and specials tokens
  parse into `Callable` nodes.
- **Macro / environment / specials** — *preset-level invocation flavors*: the latexlike
  preset's registered `CallableTypeId`s. "`\begin` is a command but not a macro."

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
| Byte range | `Span` (in `source`) | `Copy`, no `Arc`; transient parsing use |
| Arc-carrying range | `SourceSpan` | replaces lifetime-bound source locations in nodes/errors |
| Tokens | `Token<'s, L>`, `TokenKind<'s, L>` | `…Kind` per the registry rule; `Clone`, not `Copy` (`Specials` carries an `Arc`) |
| Token reading | `TokenReader<'s, L>` (trait), `StdTokenReader` | trait = behavior extension point; peek idempotent per (position, state instance) |
| Tokenization rule facets | `WhitespaceRules`, `CommandRule`, `CommentRule`, `GroupRule` | sub-structs of `TokenRules`; `Option`/empty-`Vec` = feature disabled; `GroupRule` = delimiter pair + group class, `Arc`d (travels on `GroupOpen` tokens) |
| Whitespace primitive | `skip_whitespace` | one function for pre-space and all post-space; never consumes a `\n\s*\n` newline |
| Specials scan | `Lang::scan_specials` → `SpecialsMatch<'s, L>`; `Lang::specials_trigger_chars` → `TriggerChars` | recognition = resolution (name + spec in one call) |
| Derived delimiter table | `PrefixTable`, `PrefixEntry` | built from `TokenRules`, cached per parsing state |
| Token-level errors | `TokenError<'s, L>`, `TokenErrorKind`, `TokenRecovery<'s, L>`, `TokenResult<'s, L, T>` | transient `Span`-based; `TokenResult` not `TokResult` (clarity over brevity) |
| Callable behavior | `CallableSpec<L>` (trait), `StdCallableSpec<L>` | de-keyed: no name, no invocation form |
| Invocation-form id | `Lang::CallableTypeId` | closed per-language type (enum in real langs; `u32` under `SimpleLang`) |
| Group class | `Lang::GroupTypeId` | closed per-language classification (content vs. math group), detached from delimiters; delimiter *rules* stay runtime data any parser can mint |
| Argument structure | `Vec<Arc<ArgumentSpec<L>>>` on the spec (slice accessor) | args configure; `Arc`d so parsed records share them. Slots are record-level only — no `SlotSpec` (slots session, July 2026) |
| Emptiness surface | `ArgumentParser::can_match_empty()`, `CallableSpec::requires_content()` | user-decided names (July 2026): "absent" is the record word, "contents" reads oddly for a parser; negative spec-side polarity so takeover overrides read `true` |
| Argument parsing | `ArgumentParser<L>` (trait; `ArgumentSpec.parser` is `Arc<dyn ArgumentParser>`) | an argument *is* a parser (pylatexenc `LatexArgumentSpec`); no core data variants — standard parsers are preset-provided (July 2026) |
| Definition lookup | `SpecLookup<L>` (trait), `Library<L>`, `LibraryStack<L>` | ordered stack, lexical shadowing; no `ConflictStrategy` |
| Lookup request | `CallableQuery<'a, 's, L>`, `CallableSyntax` | query struct: invocation form + name + syntax context + optional token (Phase 4) |
| Construct parser trait | `ConstructParser<L>` | avoids clashing with any high-level parser type |
| Parser context | `ParseContext<'a, 's, L>` | bundles tokens + state + session |
| Node storage | `NodeTree<L>`, `NodeData<L>`, `NodeId`, `NodeRef<'pr>` | flat, frozen, index-based; proxy access |
| Tree building | `NodeTreeBuilder<L>`, `BuildId` | staging ids ≠ final `NodeId`s (BFS flatten) |
| Parsed argument/slot records | `ParsedArguments` (`ParsedArgument` entries), `ParsedSlots` (`ParsedSlot`) | self-describing: argument entry = `Arc`'d spec + optional child region + ext (regions session, July 2026); slot entry = own `name` + region + ext (slots session, July 2026 — no spec pointer) |
| Argument/slot region record | `ChildRegion` (two-phase), `ContentNodes` (staged content designation: `InRegion` / `InChildrenOf`) | region = contiguous run of the callable's children (noise + syntax); content parser-designated, resolved to global node-index ranges by `finish()` (July 2026) |
| Node taxonomy | `NodeKind<L>`: `Chars` / `Group` / `Callable` / `Comment` / `List` | closed structural core; no `Custom` variant |
| Group payload | `GroupData<L>` | delimiters stored on the node + `Option<Lang::GroupTypeId>` class |
| Callable payload | `CallableData<L>` | invocation form + spelling + spec + parsed arguments/slots |
| Node textual payload | `TextContent` (`Spanned` / `Owned`) | logical content first-class; span = provenance |
| Node ext types | `NodeExt` (uniform) + `CharsNodeExt`, `GroupNodeExt`, `CallableNodeExt`, `CommentNodeExt`, `ListNodeExt`, `ArgumentExt`; bundled as `Lang::NodeExts: NodeExtTypes` | `SimpleLang` defaults them all to `()`; `ArgumentExt` rides on `ParsedArgument` records |
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
- `MACRO` / `ENVIRONMENT` / `SPECIALS` — the variants of the preset's `CallableTypeId` enum.
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
| `TokenKind::Macro`, `MacroRules` | `TokenKind::Command`, `CommandRule` | token level knows syntactic forms, not preset flavors; "command" scales to non-escape syntaxes (July 2026 token review) |
| `TokenKind::Chars(&str)` (maximal runs) | `TokenKind::Char(char)` | tokens are atomic units; construct parsers may need char-by-char reading |
| `TokenKind::CommentStart`, `CommentRules` | `TokenKind::Comment` (whole comment), `CommentRule` | parser has no business inside comment content |
| `TokenKind::Specials { chars }` + `TokenRules::specials` list | `TokenKind::Specials { name, spec }` via `Lang::scan_specials` | recognition = resolution; trigger sets are library-driven |
| `TokenRules::paragraph_breaks` | `TokenRules::multi_newline_paragraphs` | gates both the token and the skip rule; renamed from `double_newline_paragraphs`, July 2026 — "double" misread as "exactly two" |
| uniform `Token::post_space` field | per-variant `post_space` + `Token::post_space()` accessor | post-space is a per-kind syntactic fact (commands, comments only) |
| `peek → Ok(None)` at EOF | `TokenKind::EndOfStream` token | terminal + idempotent; `pre_space` reports final whitespace |
| `StringTokenReader` | `StdTokenReader` | driven by `TokenRules` data, not tied to `String` input |
| `TokenizerError` | `TokenError` + `TokenErrorKind` | names the failing *thing* (a token), structured kind instead of string tags (Phase 2) |
| `StateDelta` (trait) / `StandardDelta` (enum) | `ParsingStateDelta<L>` (struct of optional overrides + events) | deltas are reified values; no apply/trait machinery |
| `TokenizationState` / per-facet state traits | `TokenRules` stored in `StateData<L>` | materialized state + transition choke point (Decision 1) |
| `Node` enum with `MacroNode`, `EnvironmentNode`, `SpecialsNode` variants | `NodeKind::Callable` + `Lang::CallableTypeId` | Macro/Environment/Specials differ by invocation form, not parsed shape (Decision 3) |
| `MathNode` | `Group` with math group type + preset state ext | no privileged math mode in the core |
| `CharsNode`, `GroupNode`, `CommentNode`, `NodeList` (struct-per-type) | `NodeKind::{Chars, Group, Comment, List}` in flat `NodeTree` | flat index-based storage, `NodeRef` proxies |
| `Arguments` | `ParsedArguments` | Dec 2025 chose `Arguments`; reversed July 2026 — spec-side `ArgumentSpec`/`ArgumentParserSpec` now coexist in scope, and pylatexenc parity |
| `ArgumentsSpec` (Dec 2025 → `ArgumentStructureSpec`) | `Vec<Arc<ArgumentSpec>>` slices on `CallableSpec` | wrapper dropped July 2026 with the pylatexenc-shaped argument model; too close to `ArgumentSpec` anyway |
| `ArgumentKind`, then `ArgumentParserSpec` data variants | `ArgumentParser` objects only | an argument *is* a parser; closed enums were a regression vs. pylatexenc's per-argument parsers — the core cannot know a language's argument forms (July 2026, two steps) |
| `ArgsLayout`/`ArgLayout`, `SlotsLayout`/`SlotLayout` | `ParsedArguments`/`ParsedArgument`, `ParsedSlots`/`ParsedSlot` | "layout" opaque; records now self-describing (Arc'd specs), markers are `Chars` nodes (July 2026) |
| `ParsedArgument.child` + `.pre_space`; `ParsedSlot.child` | `.region` (`ChildRegion` with `children`/`content_range`/`content_parent`) | regions session July 2026: inter-argument noise (comments, whitespace) kept as region nodes, content parser-designated; `NodeRef::argument()`/`argument_named()` → `argument_nodes()`/`argument_content_nodes()` |
| open `GroupTypeId`/`CallableTypeId` (u32, interned in `Language`) | `Lang::GroupTypeId`/`Lang::CallableTypeId` associated types | forms and group *classes* are static per language definition; closed enums, no ids floating around (July 2026; `GroupTypeId` reframed identity → class the same month — delimiter pairings are runtime `GroupRule`s, not enum variants) |
| `Parser` trait (in `constructs`) | `ConstructParser` | avoids clash with high-level parser type |
| `SourceLocation<'src>` | `SourceSpan` | Arc spans remove the `'src` lifetime infection |
| "namespace", `CallableKind` | `CallableTypeId` | "namespace" confusable with package/library; `…TypeId` = per-language id type |
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
