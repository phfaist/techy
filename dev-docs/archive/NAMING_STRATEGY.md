# Naming Strategy for techy

Living document recording the naming rules and the current authoritative names for the
`techy` crate. **Last updated July 2026** to incorporate the resolved decisions of
[ARCHITECTURE.md](ARCHITECTURE.md) [§dd-arch:naming] (which this document must stay consistent with).
Superseded names and the reasons they changed are collected at the end; the full history
of earlier revisions lives in git.

## Design Principles

1. **Generic over specific** — no `Latex` prefixes anywhere (`Token`, not `LatexToken`;
   `Parser`-family names, not `LatexWalker`). The library targets LaTeX-*like* languages;
   the familiar LaTeX behavior is a *preset*, and LaTeX-flavored names live there.
2. **Specificity matters** — `ParsingStateDelta`, not `StateDelta` (delta of *what*?).
3. **Clarity over brevity** — `TokenResult`, not `TokResult`; `ParsedArguments`, not
   `Arguments` (the spec-side vocabulary `ArgumentSpec`/`ArgumentParser` coexists in
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
   `Lang::ModeId` (Phase 7 plan session, July 2026) is the third closed per-language
   vocabulary but deliberately **not** a `…TypeId`: it names the mode a parsing state
   *is in*, not a classification of a syntactic object.
6. **Transitions read as adjectives** — `ParsingState::derived()` per Rust's
   `to_uppercase` convention: signals a *transition* producing a new value, not a field copy.
7. **`make_*` for factory hooks** (Phase 6 plan session, July 2026) — hooks that
   construct and hand over a fresh value: `CallableSpec::make_invocation_parser`,
   `Lang::make_paragraph_break_node`.

## Current Authoritative Names (July 2026)

### Modules / strata (ARCHITECTURE.md [§dd-arch:arch] — modules are topics, not dependency ranks)

| Stratum | Module | Contents |
|---|---|---|
| S2 | `latexlike` preset | `Latexlike` lang, LaTeX-flavored helpers |
| S1 | `engine` | `Language<L>`, `ParserSession`, `ParseResult`, `NodeRef` |
| S1 | `constructs` | `ConstructParser` trait + standard construct parsers |
| S1 | `node` | `NodeTree`, `NodeKind<L>`, `GroupData`, `CallableData`, `ParsedArguments`/`ParsedSlots`, `NodeRef`, `NodeTreeBuilder`, ext aliases |
| S1 | `spec` + `scopes` | `CallableSpec`, `StdCallableSpec`, `ArgumentSpec`, `ArgumentParser`, `CallableQuery`; `SpecsProvider`, `Package`, `Scope`, `ScopeStack`, `FallbackProvider`, `ErrorCallableSpec` (Phase 7.3 redesign of `Library`/`LibraryStack`; module renamed `library` → `scopes` — full vocabulary switch, user-decided at the 7.3 checkpoint) |
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
| Seed-provider sugar | `Language::with_provider` | 7.9 promotion from preset test support: `with_seed_delta(push_provider(…))` as one call; fallible like the derive path underneath |
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
| Argument parsing | `ArgumentParser<L>` (trait; `ArgumentSpec.parser` is `Arc<dyn ArgumentParser>`) | an argument *is* a parser (pylatexenc `LatexArgumentSpec`); no core data variants — the standard parsers ship in the core, parameterized (`GroupArgumentParser`, `OptionalGroupArgumentParser`, `MarkerArgumentParser`, `ExpressionParser`, `VerbatimArgumentParser`; July 2026: `EmbellishmentsArgumentParser`, `CharsGroupArgumentParser`, `TackOnFieldsArgumentParser`); the preset adds one-liner constructors (Phase 7) |
| Definition providers | `SpecsProvider<L>` (trait), `Package<L>`, `Scope<L>`, `ScopeStack<L>`, `FallbackProvider<L>` | Phase 7 plan session (landed 7.3): package = immutable loaded collection; scope = delta-targeted definitions (CoW via `with_…` methods); innermost-wins shadowing kept (no `ConflictStrategy`); fallbacks = ordinary bottom-of-stack providers |
| Scope-stack ops | `ScopeOp<L>` (delta level, flat: `Push`/`Unload`/`Replace`/`ReplaceStack`/`Define`/`Remove`) in `ParsingStateDelta.scope_ops`; `DefinitionOp<L>` (provider level: `Define`/`Remove`, no scope name) | 7.3 checkpoint: the delta op carries the target scope name; `with_definitions` receives ops with the routing consumed — the provider *is* the scope. Sugar `.scope_op(…)` + `.push_provider(…)` (the `push_library` successor) |
| Scope-stack failures | `ProviderError` (provider level), `ScopeStackError { provider, error }` (in-stack attribution), `ScopeOpError` (per-op: `UnknownProvider` / `Provider`), `DeriveError<L>` (per-derivation: failures + recovered state + applied delta), `ScopeOpFailed` (the recover-funnel condition) | 7.3 checkpoint: mechanical error layers named by *where* the failure is observed, not who is to blame — classification is the caller's |
| Miss detail | `ScopeStack::searched_providers()` → `SearchedProviders` (Display adapter) | a miss is `Ok(None)`; the searched set is a property of the (visibility-blind) stack, not of one miss |
| Undefined-on-purpose spec | `ErrorCallableSpec` | an ordinary definition whose invocation parser diagnoses; replaces any `Masked` lookup outcome |
| Parsing mode | `Lang::ModeId`; `StateData.mode`; `ParsingStateDelta.mode` | third closed per-language vocabulary (see principle 5); deltas initiate mode changes, `finalize_transition` interprets them |
| Parse driving | `ParseDriver<L>` (trait), `StdParseDriver`, `Lang::Driver`; `ParseContext.driver`; cx wrappers `parse_nodes`/`parse_group`, sugar `derived_state`/`group_interior_state` | defaulted-methods trait (Phase 7 plan session; landed 7.2, home `engine::driver` — `CommandResolution`/`ResolvedCallable` moved there with `resolve_command`); construct provision (`make_nodes_parser`/`make_group_parser`/`make_invocation_parser`), `group_interior_delta`, recovery policy (`ParserSession::new()` now argument-free), migrated parse-time hooks |
| Lookup request | `CallableQuery<'a, 's, L>`, `CallableSyntax` | query struct: invocation form + name + syntax context + optional token (Phase 4) |
| Construct parser trait | `ConstructParser<L>` | avoids clashing with any high-level parser type |
| Parser context | `ParseContext<'a, 's, L>` | bundles tokens + source + state + session (source added 6.4 — factory-created parsers have no ctor to thread it) |
| Construct-parser result | `ConstructParserResult<L, T>` | `= Result<T, ParseError>`; lang-first like `TokenResult` (6.1); over the sketched `ParseOutcome` — unambiguous next to the engine-level `ParseResult` |
| Content-run parser | `NodesParser`, `NodesOutcome` | over `ContentParser`: the regions session gave "content" a precise technical meaning (designated argument/slot content) |
| Stop machinery | `StopSpec`, `TokenStopCondition { kind, consume }`, `TokenStopKind`, `StopCause` | abnormal endings are data, not errors; `StopCause` = `TokenCondition`/`NodeCondition`/`EndOfInput`/`UnexpectedGroupClose` (token-bearing causes carry the matched span) |
| Descent-state policy | `ChildStateSpec`, `GroupChildState`, `InvocationChildState` | per-use config on `NodesParser` (child-state session, July 2026) |
| Group parser | `GroupParser` | engine temporary (tier 2), per-use config, dropped with the frame |
| Invocation dispatch | `Invocation`, `CommandResolution`, `ResolvedCallable`, `ParseDriver::resolve_command` (moved off `Lang`, Phase 7 plan session) | `Invocation` = the resolved-invocation value moved into the parser; `CommandResolution` = the hook's outcome (`Resolved`/`Unresolved { detail }`, July 2026); `ResolvedCallable` = invocation form + spec pair |
| Default invocation parser | `StdInvocationParser` | `Std…` prefix per `StdTokenReader`/`StdCallableSpec`; `parse_declared_arguments` = its shared argument half (pub, slots session) |
| Environment body parsing | `EnvironmentBodyParser`, `EnvironmentBody`, `with_match_invocation_name` | core, parameterized — terminator data = ctor params ([§dd-dr:parsers-engine]); `read_rigid_name_group` + `NameGroup` = the shared rigid-scaffolding reader (pub, slots session); `EnvironmentBody.content` = the parser's slot-content designation (7.7) |
| Verbatim family (7.7) | `verbatim_state_delta` (the pinned recipe as data), `VerbatimArgumentParser` (delimited `\verb\|…\|`, the `v` codes; family-consistent `…ArgumentParser`), `VerbatimBodyParser` (environment contents to a literal terminator; sibling of `EnvironmentBodyParser`), conditions `UnterminatedVerbatim`/`ExpectedVerbatimDelimiter` | pylatexenc `LatexDelimitedVerbatimParser` / `LatexVerbatimEnvironmentContentsParser`; no `Latex` prefixes, roles named over mechanisms. `GroupArgumentParser::with_rule` = the mandatory minted-rule form (the `r<c1><c2>` code); `with_expression_fallback` = the orthogonal fallback knob (defaults: class on, rule off) |
| Parity parsers N2–N6 (July 2026, user-reviewed) | `GroupArgumentParser::any_of` / `OptionalGroupArgumentParser::any_of` (multi-rule forms; `with_rule` = one-element sugar), `EmbellishmentsArgumentParser` (plural — user choice), `CharsGroupArgumentParser` (knobs `with_comments`/`with_nested_groups`/`with_restricted_descent`), `TackOnFieldsArgumentParser` (`with_field`/`with_repeatable_field`), condition `RepeatedTackOnField` | pylatexenc `LatexDelimitedMultiDelimGroupParser` (dissolved into `any_of` — no new type, per the resolved `### PhF` note), `LatexOptionalEmbellishmentArgsParser`, `LatexCharsGroupParser`, `LatexTackOnInformationFieldMacrosParser` — "macro" is preset vocabulary, so the tack-on name speaks of *fields* (role over mechanism); family-consistent `…ArgumentParser` suffixes; `any_of` echoes the `AnyDelimited` code |
| Node finalization hook | `Lang::finalize_node` | run by `NodeTreeBuilder::add` for every staged node, all kinds |
| Staged read views | `StagedNodes`, `StagedNodeView` | read-only builder views for `finalize_node` and node stop predicates |
| Tree invariant checker | `check_tree_invariants` | public test utility in `node` (span partition, `Spanned` residency, region tiling) |
| Pre-scanned token reader | `TokenListReader` | `TokenReader` over a pre-built token list; unit-test isolation (documented re-tokenization fidelity limit) |
| Construct-level error | `ParseError<O>` | abort-only (`Err` means abort, [§dd-dr:errors]); carries no recovery payload |
| Node storage | `NodeTree<L>`, `NodeData<L>`, `NodeId`, `NodeRef<'pr>` | flat, frozen, index-based; proxy access |
| Tree building | `NodeTreeBuilder<L>`, `BuildId` | staging ids ≠ final `NodeId`s (BFS flatten) |
| Parsed argument/slot records | `ParsedArguments` (`ParsedArgument` entries), `ParsedSlots` (`ParsedSlot`) | self-describing: argument entry = `Arc`'d spec + optional child region + ext (regions session, July 2026); slot entry = own `name` + region + ext (slots session, July 2026 — no spec pointer) |
| Argument/slot region record | `ChildRegion` (two-phase), `ContentNodes` (staged content designation: `InRegion` / `InChildrenOf`) | region = contiguous run of the callable's children (noise + syntax); content parser-designated, resolved to global node-index ranges by `finish()` (July 2026) |
| Sibling-run view | `NodeSlice<'t, L>` (+ `NodeSliceIter`) | 7.8: the node-list currency — `Copy` view `{tree, range}` returned by `children()` and the region/content accessors; exact `span()`/`source_text()` (partition invariant), `None` for empty/mixed-source runs. Over `NodeRun`/`Siblings`; "List" excluded (collides with `NodeKind::List`) |
| By-name argument/slot access | `argument_nodes_named` / `argument_content_nodes_named` / `slot_content_nodes_named` | plain `_named` suffixes beside the index twins (7.8; user choice — no polymorphic key type) |
| Document-order walk | `NodeRef::descendants()`, `NodeTree::descendants()` → `Descendants` | preorder DFS, self excluded; the deliberate contrast to `iter_storage_order` (7.8) |
| Compact node description | `NodeRef::summary()` | 7.9 promotion from preset test support (`chars(ab)` / `Macro(emph)` / …); "summary" over "shape" — the phase docs use "shape" for *tree* structure, and the string includes content text; format documented as human-oriented, not a stability contract |
| Extraction helpers | `node::extract`: `content_as_chars`, `split_at_chars` → `Split` (`segment(i)`/`segments()`), `parse_keyval` → `KeyVals` (`keyval(i)`/`get`/`get_combined_with`) with `KeyValEntry` (`value`/`value_content`), `ExtractError`; run readers `split_embellishments` / `split_tack_on_fields` → `KeyVals` (July 2026, N2–N6 session — the keyval result type reused wholesale), `ExtractError::UnexpectedContent` | 7.8: free functions, not core methods (user); result wrappers own minted trees privately, primary access returns `NodeSlice` views; strict `Result`s (read-time, no tolerance mode) |
| Symbol enumeration | `SpecsProvider::iter_symbols(callable_type, mode)`, `ScopeStack::iter_symbols` → `SymbolEntry` | 7.8: required type filter (user; specials rows enumerate under their recorded type, trigger spelling as name); `None` = not enumerable; stack dedup = first-visible-wins innermost-first |
| Enumerable vocabulary | `ClosedVocabulary` (`const ALL`) | 7.8: opt-in tooling bound making "closed per language" statically listable; deliberately not a `Lang` bound (`SimpleLang`'s `u32` ids have no value list); preset implements for all three vocabularies |
| Node taxonomy | `NodeKind<L>`: `Chars` / `Group` / `Callable` / `Comment` / `List` | closed structural core; no `Custom` variant |
| Group payload | `GroupData<L>` | delimiters stored on the node + `Option<Lang::GroupTypeId>` class |
| Callable payload | `CallableData<L>` | invocation form + spelling + spec + parsed arguments/slots |
| Node textual payload | `TextContent` (`Spanned` / `Owned`) | logical content first-class; span = provenance |
| Node ext types | `NodeExt` (uniform) + `CharsNodeExt`, `GroupNodeExt`, `CallableNodeExt`, `CommentNodeExt`, `ListNodeExt`, `ArgumentExt`; bundled as `Lang::NodeExts: NodeExtTypes` | `SimpleLang` defaults them all to `()`; `ArgumentExt` rides on `ParsedArgument` records |
| Source model | `Source`, `SourceSpan`, `SourceProvenance`, `SourceResolver`, `LineIndex` | per SOURCE_ARCHITECTURE.md (its `SourceContent`/`SourceCursor` retired July 2026) |
| Origin metadata | `SourceOrigin` (trait); default impl on `Option<String>` | no named `Std…` type: the default origin is a plain optional URL string (July 2026 revision) |
| Resolvers | `NoResolver` (ZST default), `MapResolver`, `ResolveError` | per SOURCE_ARCHITECTURE.md; no `FileResolver` — file I/O lives with the embedder (no_std policy) |
| Diagnostics | `Diagnostic`, `Diagnostics`, `Severity`, `Recovery` | span-based; `Recovery` = tolerant-parsing policy (strict/tolerant) |

### Preset-layer names (`techy::latexlike`)

"Macro", "environment", and "specials" are **invocation forms, not core concepts**. The
familiar names survive in the preset layer only. Preset items are namespaced
(`techy::latexlike::…`, no crate-root re-exports), and the vocabulary enums use **bare,
module-scoped names with short variants** — principle 4 applies: no sibling competes,
the core has only the associated types (user-decided, 7.5 checkpoint):

- `Latexlike` — ZST implementing `Lang`.
- `LatexlikeDriver` — the preset's `ParseDriver` (scope-stack `resolve_command`, the
  math-mode `group_interior_delta` plug; landed 7.5 — package-loading helper methods
  wait for a package registry).
- `ParagraphBreakStyle` (`Chars` / `Specials`; `#[non_exhaustive]`, `#[default]
  Chars`) + `LatexlikeDriver::with_paragraph_break_style` — the paragraph-break
  emission flag (7.9, user-decided): a two-variant enum over the sketched boolean
  `with_emit_specials_for_paragraph_breaks()` — names the concept, reads at call
  sites, leaves room for future styles. The `Specials` node's name is the canonical
  `"\n\n"` (a vocabulary key like `"~"`; no named constant — it is preset vocabulary,
  not configuration).
- `GroupType` (`Content` / `Math` / `Verbatim`) — group classes. A *single* math class:
  inline vs. display is a delimiter fact, read by the `NodeRef::math_style()` sugar →
  `MathStyle` (`Inline` / `Display`). No `Bracket` class: `[]` is not a default group
  (optional arguments recognize it via per-spec `temporary_groups` rules). `Verbatim`
  (landed 7.7) marks raw regions (`\verb` groups, minted terminator rules) — never a
  tokenizer-declared rule.
- `CallableType` (`Macro` / `Environment` / `Specials`) — invocation forms. (CamelCase
  variants; supersedes the `MACRO`/`ENVIRONMENT`/`SPECIALS` const-era spelling.)
- `Mode` (`Text` / `Math`; `#[default] Text`) — parsing modes. Deliberately no
  `Mode::Verbatim`: verbatimness is rules-borne, not a mode (7.7).
- All three vocabulary enums are `#[non_exhaustive]`.
- `default_token_rules()` / `base_package()` — the canonical seed data; `"base"` is the
  seeded package of pylatexenc's default specials (user-named, 7.5 checkpoint).
- `MacroSpec` / `EnvironmentSpec` / `SpecialsSpec` — the preset's spec types (real
  concrete types, user-decided at the 7.6 checkpoint — they carry the preset's
  traceback vocabulary "macro ‘\frac’" / "environment ‘align’" / "specials ‘~’" and
  are stable downcast targets). `MacroSpec`/`SpecialsSpec` are declarative
  (`StdCallableSpec`-shaped); `EnvironmentSpec` is the [§dd-dr:specs] funnel wrapper over
  `EnvironmentBehavior` (the inner dyn trait: defaulted `arguments()`,
  `body_state_delta()`, `make_body_parser()`; hooks receive an
  `EnvironmentInvocation` facts struct). Builder `with_body_delta(…)` overrides the
  delta over *any* behavior (adapter-wrapping, total for custom behaviors too).
- `BeginSpec` / `EndSpec` — the `\begin` dispatcher and the orphan-`\end` diagnoser:
  ordinary `Macro` entries of `base_package()` (7.6 checkpoint decision (a): dispatch
  is scope-stack data, not driver code — shadowable/unloadable).
- Preset condition ids are namespaced `latexlike.environments.*`
  (`malformed-begin`, `unknown-environment`, `orphan-end`; user-decided 7.6).
- `NodeRef` accessor sugar as **inherent** methods on `NodeRef<'_, Latexlike>`
  (same-crate privilege, user-decided 7.5; out-of-crate languages use an extension
  trait): `is_math_group`, `math_style`, `macro_name`, `environment_name`,
  `specials_name`.
- `argument_specs(codes)` / `ArgumentCodeError` — the argument-code factory (7.7,
  N8): xparse-like code string → configured `ArgumentSpec`s; a plain function (reads
  as "argument specs for these codes"), error named for the malformed *code*.
- `VerbatimBehavior` — the raw-body `EnvironmentBehavior` (7.7): one instance serves
  any environment name (`verbatim`, `verbatim*`, listing-style with arguments); wraps
  the core `VerbatimBodyParser`.

Type aliases (`type LatexParseResult = ParseResult<Latexlike>` …) remain a deferred
bikeshed — none shipped in 7.5; `Language<Latexlike>` reads fine without them.

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
| `Arguments` | `ParsedArguments` | Dec 2025 chose `Arguments`; reversed July 2026 — spec-side argument vocabulary (`ArgumentSpec`/`ArgumentParser`, at the time `ArgumentParserSpec`) coexists in scope, and pylatexenc parity |
| `ArgumentsSpec` (Dec 2025 → `ArgumentStructureSpec`) | `Vec<Arc<ArgumentSpec>>` slices on `CallableSpec` | wrapper dropped July 2026 with the pylatexenc-shaped argument model; too close to `ArgumentSpec` anyway |
| `ArgumentKind`, then `ArgumentParserSpec` data variants | `ArgumentParser` objects only | an argument *is* a parser; closed enums were a regression vs. pylatexenc's per-argument parsers — the core cannot know a language's argument forms (July 2026, two steps) |
| `ArgsLayout`/`ArgLayout`, `SlotsLayout`/`SlotLayout` | `ParsedArguments`/`ParsedArgument`, `ParsedSlots`/`ParsedSlot` | "layout" opaque; records now self-describing (Arc'd specs), markers are `Chars` nodes (July 2026) |
| `ParsedArgument.child` + `.pre_space`; `ParsedSlot.child` | `.region` (`ChildRegion` with `children`/`content_range`/`content_parent`) | regions session July 2026: inter-argument noise (comments, whitespace) kept as region nodes, content parser-designated; `NodeRef::argument()`/`argument_named()` → `argument_nodes()`/`argument_content_nodes()` |
| open `GroupTypeId`/`CallableTypeId` (u32, interned in `Language`) | `Lang::GroupTypeId`/`Lang::CallableTypeId` associated types | forms and group *classes* are static per language definition; closed enums, no ids floating around (July 2026; `GroupTypeId` reframed identity → class the same month — delimiter pairings are runtime `GroupRule`s, not enum variants) |
| `Parser` trait (in `constructs`) | `ConstructParser` | avoids clash with high-level parser type |
| `ContentParser` (Phase 6 notes) | `NodesParser` | regions session gave "content" a precise meaning (designated argument/slot content) a general nodes parser doesn't have |
| `ParseOutcome` (Phase 6 notes) | `ConstructParserResult<L, T>` | unambiguous next to the engine-level `ParseResult`; clarity over brevity |
| `TokenStopCondition` (closed enum, Phase 6 plan) | `TokenStopKind` + `TokenStopCondition { kind, consume }` | 6.2 amendment: the consume switch is bound to the condition |
| `claim_post_space` (planned 6.4 helper) | (nothing) | superseded before shipping: `post_space` = exactly the trigger token's own syntactic post-space, nothing beyond it is ever claimed ([§dd-dr:nodes] invariant 3) |
| `peek_argument_token` | `try_peek` (pub(crate)) | hoisted in 6.6 — the same probe policy also serves the terminator flow |
| `SourceLocation<'src>` | `SourceSpan` | Arc spans remove the `'src` lifetime infection |
| `SourceContent`, `SourceCursor`, `Source::cursor()` | (nothing — `StdTokenReader` scans `&str` directly) | retired July 2026 (Action 06): the scanner needs random-access slicing, not a char cursor; the borrow-returning trait was information-equivalent to `&str` (DESIGN_RATIONALE [§dd-dr:sources-and-spans]) |
| `SpecLookup`, `Library`, `LibraryStack`; `ParsingStateDelta::push_libraries` | `SpecsProvider`, `Package` + `Scope`, `ScopeStack`; `ParsingStateDelta.scope_ops` (`ScopeOp`/`DefinitionOp`) | Phase 7 plan session, landed 7.3: data-first multi-method provider contract (fallible, specials-participating, functionally updatable via `with_…`); package/scope role split (immutable loadable vs delta-mutable); fallbacks in-stack |
| `library` module; `StateData.libraries`, `ParsingState::libraries()` | `scopes` module; `StateData.scopes`, `ParsingState::scopes()` | 7.3 naming checkpoint: no type named `Library` survived the redesign — module and field follow the vocabulary they hold |
| parse-time `Lang` hooks (`resolve_command`, `make_paragraph_break_node`, `observe_transition`, `refine_diagnostic`); `ParserSession.recovery` | `ParseDriver` methods; driver-held `Recovery` | placement doctrine (Phase 7 plan session): what only runs while a parse is driven lives on the driver; the session stays pure scratch/output |
| "namespace", `CallableKind` | `CallableTypeId` | "namespace" confusable with package/library; `…TypeId` = per-language id type |
| `GroupExt`, `NodeGroupExt` | `GroupNodeExt` (etc.) | `GroupExt` too vague; `NodeGroupExt` parses wrong |
| `parser` module (high-level API) | `engine` module | layered architecture of ARCHITECTURE.md [§dd-arch:arch] |
| `apply()` / `copy_with()` | `ParsingState::derived()` | adjective form; signals a transition |
| `impl Iterator` returns of `children()` and the region/content accessors | `NodeSlice` | 7.8: span information belongs in the return types (exact, partition-invariant-backed); adaptor chains insert `.iter()` |
| `Segment` / `SegmentPiece` (7.8 shape draft) | (nothing — segments are `NodeSlice` views into minted result trees) | user: no second node-list currency ("that's why we have node lists in the first place"); the builder route mints real trees, pylatexenc-style |
| `covering_span()` free helper (7.8 shape draft) | `NodeSlice::span()` / `NodeSlice::source_text()` | user: no best-effort recomputation in a helper; typed unavailability (`Option`) on the view itself |

## Module Rename History

- **`parsing` → `constructs`** (Dec 2025): `parsing` was too close to `parser`;
  "constructs" describes the content — parsers for individual constructs. The name and
  rationale carry over unchanged into the L5 layer.
- **`walker` → `parser` → `engine`** (Dec 2025, then July 2026): the high-level API module
  was first renamed from `walker` to `parser`; the July 2026 architecture places the
  high-level machinery (`Language<L>`, `ParserSession`) in `engine` instead.
