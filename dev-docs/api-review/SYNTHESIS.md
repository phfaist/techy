# SYNTHESIS — Phase 1 persona walkthroughs → Phase 2a policy inputs

Generated 2026-07-28 from `INVENTORY.md` (205 public items, 140 root re-exports) and the
four persona walkthrough pairs (`walkthroughs/{consumer,extender,langdesign,tooling}/
{API-SURFACE.md,FRICTION.md}`). Personas: T1 document consumer, T2 extender, T3 language
designer, T4 tooling author.

Normalization: an item reached via root re-export and via module path is ONE item; item =
type/trait/fn/alias path (methods, fields, and variants are not separate rows — the
surfaces' own method tallies are quoted in §6). "Used" means the item appears in that
persona's API-SURFACE.md as touched by their final code. `(x)` = touched indirectly
(type never imported/named: reached through inference, a field, a returned value, or a
trait-object coercion). Items a persona *read but rejected* (T3: `SimpleLang`,
`StdParseDriver`) are excluded from the matrix and flagged in §3.

## 1. Name × persona usage matrix

**How items were reached (per the surface files' own legends).** T1: every import used
the module path ("that is what the guide teaches"); [root] marks availability only.
T3: "all items below were imported via module paths"; [R] marks availability. T2 records
availability, not paths; its code imitates learn-by-example, i.e. module paths. T4 shows
paths as actually used: module paths except **three root-path uses** (`techy::format_position`,
`techy::format_traceback`, `techy::UnresolvableCommand`). So the Root column below is an
*availability* fact; actual root-path traffic across all four personas was 3 items, all T4.

**Disagreement with INVENTORY (1 found):** T3 marks `constructs::GroupArgumentParser`
as [R]; INVENTORY says the argument parsers are *not* root re-exported (Root? = n).
T3's closing claim "every touched item is root-re-exported except the latexlike preset"
is wrong for exactly this one item. All other markers agree with INVENTORY.

73 rows, sorted by persona count (desc), then module (source, error, token, state, spec,
scopes, node, constructs, engine, latexlike).

| Item | Kind | T1 | T2 | T3 | T4 | # | Root (INV) | Notes |
|---|---|---|---|---|---|---|---|---|
| `source::SourceSpan` | struct | x | (x) | x | x | 4 | Y | T2 via method receiver only |
| `error::Recovery` | enum | x | x | x | x | 4 | Y | |
| `error::ParseError` | struct | x | (x) | (x) | (x) | 4 | Y | T2/T3/T4 via `parse()` Err, never imported |
| `error::Diagnostic` | struct | x | (x) | (x) | x | 4 | Y | |
| `error::Diagnostics` | struct | x | (x) | (x) | (x) | 4 | Y | |
| `scopes::Package` | struct | x | x | x | x | 4 | Y | the one extensibility item all four share |
| `node::NodeRef` | struct | x | (x) | x | x | 4 | Y | largest method surface used (42 methods incl. preset sugar) |
| `node::NodeSlice` | struct | x | (x) | x | (x) | 4 | Y | |
| `engine::Language` | struct | x | x | x | x | 4 | Y | |
| `engine::ParseResult` | struct | x | (x) | (x) | x | 4 | Y | pub fields `tree`/`diagnostics` |
| `node::NodeTree` | struct | x | (x) | – | (x) | 3 | Y | absent from T3's surface though `tree.root()` is implied by its code path |
| `node::NodeKind` | enum | x | – | x | x | 3 | Y | |
| `latexlike::Latexlike` | struct | x | x | – | x | 3 | n | preset; module-only by design; T3 deliberately avoids it |
| `latexlike::LatexlikeDriver` | struct | x | x | – | x | 3 | n | |
| `latexlike::CallableType` | enum | x | x | – | x | 3 | n | |
| `latexlike::MacroSpec` | struct | x | x | – | x | 3 | n | |
| `latexlike::EnvironmentSpec` | struct | x | x | – | x | 3 | n | |
| `latexlike::argument_specs` | fn | x | x | – | x | 3 | n | |
| `source::Source` | struct | x | – | – | x | 2 | Y | |
| `source::Span` | struct | – | – | x | (x) | 2 | Y | T4: "never named", reached via range equivalents |
| `source::LineIndex` | struct | x | – | – | x | 2 | Y | |
| `error::Severity` | enum | (x) | – | – | x | 2 | Y | T1 never named it (Display via `severity()`) |
| `error::format_position` | fn | x | – | – | x | 2 | Y | T4 called via ROOT path |
| `state::ParsingStateDelta` | struct | – | x | x | – | 2 | Y | |
| `spec::ArgumentSpec` | struct | – | x | x | – | 2 | Y | |
| `node::Descendants` | struct | x | – | – | (x) | 2 | Y | both use it only as an unnamed `Iterator` |
| `node::ParsedArguments` | struct | – | (x) | x | – | 2 | Y | T3 `empty()` (build side); T2 `get`/`len` (read side) |
| `node::extract::content_as_chars` | fn | x | – | – | x | 2 | n | the one mandatory deep path; both personas flagged the inconsistency |
| `constructs::UnresolvableCommand` | struct | – | – | x | x | 2 | Y | `IDENTIFIER` (T3); field `name` + ROOT-path import (T4) |
| `source::TextContent` | enum | – | – | x | – | 1 | Y | `From<Span>` in takeover parser |
| `source::SourceProvenance` | enum | – | – | – | x | 1 | Y | variant matching, pub fields |
| `source::MapResolver` | struct | – | – | – | x | 1 | Y | |
| `source::ResolveError` | struct | – | – | – | (x) | 1 | Y | |
| `source::SourceOrigin` | trait | – | – | – | (x) | 1 | Y | as the default `Option<String>` |
| `error::format_traceback` | fn | – | – | – | x | 1 | Y | ROOT-path call |
| `error::DiagnosticInfo` | trait | – | – | x | – | 1 | Y | imported for `IDENTIFIER` consts; derive macro of same name unused |
| `error::TraceFrame` | struct | – | – | – | (x) | 1 | Y | |
| `token::TokenRules` | struct | – | – | x | – | 1 | Y | constructed, all 13 fields |
| `token::CommandRule` | struct | – | – | x | – | 1 | Y | |
| `token::CommentRule` | struct | – | – | x | – | 1 | Y | |
| `token::GroupRule` | struct | – | – | x | – | 1 | Y | |
| `token::WhitespaceRules` | struct | – | – | x | – | 1 | Y | |
| `token::Token` | struct | – | – | x | – | 1 | Y | |
| `token::TokenKind` | enum | – | – | x | – | 1 | Y | |
| `token::TokenResult` | alias | – | – | x | – | 1 | Y | |
| `token::SpecialsMatch` | struct | – | – | x | – | 1 | Y | |
| `token::TriggerChars` | enum | – | – | x | – | 1 | Y | |
| `token::TokenReader` | trait | – | – | x | – | 1 | Y | methods via `cx.tokens`; trait not implemented |
| `state::Lang` | trait | – | – | x | – | 1 | Y | implemented (9 assoc types + 3 methods) |
| `state::StateData` | struct | – | – | x | – | 1 | Y | |
| `state::ParsingState` | struct | – | – | x | – | 1 | Y | |
| `state::TokenRulesOverrides` | struct | – | – | x | – | 1 | Y | |
| `state::ClosedVocabulary` | trait | – | – | x | – | 1 | Y | |
| `spec::StdCallableSpec` | struct | – | – | x | – | 1 | Y | |
| `spec::CallableSpec` | trait | – | – | x | – | 1 | Y | implemented (takeover) |
| `scopes::ScopeStack` | struct | – | – | x | – | 1 | Y | |
| `scopes::SpecsProvider` | trait | – | – | (x) | – | 1 | Y | satisfied by Package; `Arc<dyn …>` coercion |
| `node::ParsedArgument` | struct | – | (x) | – | – | 1 | Y | `is_provided()` |
| `node::ParsedSlot` | struct | – | – | x | – | 1 | Y | |
| `node::CallableData` | struct | – | – | x | – | 1 | Y | constructed, 7 fields |
| `node::ChildRegion` | struct | – | – | x | – | 1 | Y | |
| `node::ContentNodes` | enum | – | – | x | – | 1 | Y | |
| `node::BuildId` | struct | – | – | x | – | 1 | Y | parser `Output` type |
| `node::NodeTreeBuilder` | struct | – | – | x | – | 1 | Y | via `cx.session.builder` |
| `constructs::ConstructParser` | trait | – | – | x | – | 1 | Y | implemented |
| `constructs::ConstructParserResult` | alias | – | – | x | – | 1 | Y | |
| `constructs::ParseContext` | struct | – | – | x | – | 1 | Y | |
| `constructs::Invocation` | struct | – | – | x | – | 1 | Y | |
| `constructs::GroupArgumentParser` | struct | – | – | x | – | 1 | **n** | T3 marked [R] — contradicts INVENTORY (module-only) |
| `constructs::MissingMandatoryArgument` | struct | – | – | x | – | 1 | Y | `IDENTIFIER` |
| `engine::ParseDriver` | trait | – | – | x | – | 1 | Y | implemented (2 of 11 defaulted methods) |
| `engine::CommandResolution` | enum | – | – | x | – | 1 | Y | `resolve_via_scopes` — T3's "most load-bearing helper" |
| `latexlike::argument_specs_from_str` | fn | – | x | – | – | 1 | n | |

## 2. Shared core vs per-tier increments

### Items used by 3+ personas — 18 items ("everyone needs this" core)

By 4: `Language`, `ParseResult`, `Recovery`, `ParseError`, `Diagnostic`, `Diagnostics`,
`Package`, `NodeRef`, `NodeSlice`, `SourceSpan` (10).
By 3: `NodeTree`, `NodeKind` (T1/T3/T4); `Latexlike`, `LatexlikeDriver`, `CallableType`,
`MacroSpec`, `EnvironmentSpec`, `argument_specs` (T1/T2/T4 — the whole latexlike
happy path; T3 avoids the preset by design) (8).

Notable: 6 of the 18 core items are **not** root re-exported (all latexlike). The empirical
core = "parse entry + result + diagnostics + node reading + Package + the preset's 6 names".

### Items unique to one persona

- **T1 (consumer): 0 items.** Every name the consumer touched is also touched by another
  persona. T1 is a strict subset of the union of T2/T3/T4 — strong evidence that a
  root/curated tier built for T1 costs nothing extra.
- **T2 (extender): 2 items** — `latexlike::argument_specs_from_str`, `node::ParsedArgument`.
- **T3 (language designer): 36 items** — all of token used (11: `TokenRules`, `CommandRule`,
  `CommentRule`, `GroupRule`, `WhitespaceRules`, `Token`, `TokenKind`, `TokenResult`,
  `SpecialsMatch`, `TriggerChars`, `TokenReader`); state minus the shared delta (5:
  `Lang`, `StateData`, `ParsingState`, `TokenRulesOverrides`, `ClosedVocabulary`);
  spec (2: `StdCallableSpec`, `CallableSpec`); scopes (2: `ScopeStack`, `SpecsProvider`);
  constructs (6: `ConstructParser`, `ConstructParserResult`, `ParseContext`, `Invocation`,
  `GroupArgumentParser`, `MissingMandatoryArgument`); node build side (6: `ParsedSlot`,
  `CallableData`, `ChildRegion`, `ContentNodes`, `BuildId`, `NodeTreeBuilder`);
  engine (2: `ParseDriver`, `CommandResolution`); error (1: `DiagnosticInfo` trait);
  source (1: `TextContent`).
- **T4 (tooling): 6 items** — `format_traceback`, `TraceFrame`, `SourceProvenance`,
  `MapResolver`, `ResolveError`, `SourceOrigin` (all provenance/rendering; 4 of 6 in source).

### Cumulative tier sizes

| Tier | New items | Cumulative |
|---|---|---|
| T1 | 24 | **24** |
| + T2 | +5 (`argument_specs_from_str`, `ArgumentSpec`, `ParsingStateDelta`, `ParsedArguments`, `ParsedArgument`) | **29** |
| + T3 | +38 (the 36 unique + `Span`, `UnresolvableCommand` shared with T4) | **67** |
| + T4 | +6 | **73** |

The tier structure is empirically real and steep: consumer+extender together need **29
names**; the language designer nearly **triples** that; the tooling author adds a thin
source/provenance layer. (INVENTORY's provisional estimates — T1≈47, T2≈60–70, T3≈95–105 —
were 2–3x the observed task-driven usage in every tier; conditions and optional machinery
account for most of the gap.)

## 3. Unused-by-everyone: root re-exports no persona touched

Of the **140** root re-exports, **64 were used** by at least one persona (any path) and
**76 (54%) were touched by nobody** — the empirical demotion-candidate list for a
"simple at root, detail in modules" policy. Items marked `*` have an implicit-usage or
read-and-rejected caveat (detailed below); **66 of the 76 have no usage signal at all**.

- **source (5):** `SourceResolver`\*, `NoResolver`, `ProvenanceChain`\*, `ResolvedContent`,
  `resolve_source` (the free fn INVENTORY already flagged as redundant — confirmed unused;
  T4 used `Language::resolve_source` instead).
- **error (5):** `DiagnosticData`, `DiagnosticValue`, `DiagnosticInfo` (derive macro),
  `ToDiagnosticValue` (trait), `ToDiagnosticValue` (derive macro). Nobody defined a custom
  condition, so the whole diagnostics-*defining* surface went unused; the `DiagnosticInfo`
  *trait* was used (T3, for `IDENTIFIER` consts only).
- **token (8):** `StdTokenReader`\*, `PrefixTable`, `TokenError`, `TokenErrorKind`,
  `TokenRecovery`, `EndOfStreamAfterEscape`, `ForbiddenChar`, `skip_whitespace`.
  Even the full custom-language walkthrough never named the error/recovery/prefix half
  of token.
- **state (3):** `SimpleLang`\* (read and *rejected* by T3 — dead-ends with commands, F2),
  `NodeExtTypes`, `DeriveError`.
- **spec (3):** `ArgumentParser`\*, `ParsedArgumentNodes`, `FrameRole`.
- **scopes (13):** `Scope`, `FallbackProvider`, `ErrorCallableSpec`, `CallableDefinedAsError`,
  `CallableQuery`, `CallableSyntax`, `SymbolEntry`, `SearchedProviders`, `DefinitionOp`,
  `ScopeOp`, `ScopeOpError`, `ScopeStackError`, `ProviderError`. Everything except
  `Package`/`ScopeStack`/`SpecsProvider` — `Package` alone covered every walkthrough's
  provider needs, including T2's scoped-definitions stretch task.
- **node (9):** `NodeData`, `NodeId`\*, `NodeSliceIter`\*, `GroupData`, `ParsedSlots`\*,
  `StagedNodes`, `StagedNodeView`, `NodeBuildError`, `check_tree_invariants`.
  Note: even T3's hand-staging takeover parser never named `StagedNodes`/`StagedNodeView`/
  `NodeBuildError` (the builder's error surfaced only as contract prose).
- **constructs (24):** the core-dispatch layer (10): `NodesParser`, `NodesOutcome`,
  `GroupParser`, `StopSpec`, `StopCause`, `TokenStopCondition`, `TokenStopKind`,
  `ChildStateSpec`, `GroupChildState`, `InvocationChildState`; plus 14 of the 16
  root condition types: `CommandResolutionFailed`, `UnclosedGroup`, `UnclosedGroupFound`,
  `StrayGroupClose`, `ExpectedExpressionArgument`, `ExpressionCallableRequiresContent`,
  `MissingEnvironmentTerminator`, `MissingTerminatorFound`, `EnvironmentTerminatorMismatch`,
  `MalformedEnvironmentTerminator`, `ScopeOpFailed`, `UnusableRecoveryToken`,
  `UnusableRecoveryTokenKind`, `ImplementationError` (the *type*; T3 used the
  `ParseContext::implementation_error` method, never the struct).
- **engine (5):** `ParserSession`\*, `StdParseDriver`\*, `ResolvedCallable`, `Frame`,
  `FrameTitle`.
- **crate root (1):** `VERSION`.

### Caveats on the starred items (implicit use — weakens the demotion case for these 10)

- `ParserSession` — T3 used it constantly as `cx.session` (field of `ParseContext`) incl.
  `session.builder`; never imported the type. Demoting it from root is safe; removing it
  from reachability is not.
- `SourceResolver` — T4's `MapResolver` + `Language::with_resolver` imply it; trait never named.
- `ProvenanceChain` — T4 iterated `Source::provenance_chain()`; return type never named.
- `StdTokenReader` — engaged implicitly by every parse (T3 notes this); never named.
- `ArgumentParser` — T2's probe rebuilt specs around the public `ArgumentSpec.parser`
  field, whose type is `Arc<dyn ArgumentParser>`; trait never named.
- `NodeId` — T4 debug-printed `node.id()`; type never named.
- `NodeSliceIter` — T1/T2 called `.iter()`; iterator type never named.
- `ParsedSlots` — absent from T3's API-SURFACE but its FRICTION.md says it was built via
  `From<Vec<_>>` (and wishes `ParsedSlots::new`); treat as *used-by-T3, surface-file omission*.
- `SimpleLang`, `StdParseDriver` — T3 read both and rejected both (F2); their *existence*
  shaped the walkthrough even though no final code uses them.

### Cross-check against INVENTORY's provisional tier tags

**Tagged T1 or T2 but unused by everyone** (the provisional tags overestimated these):

- Tagged T1: `DiagnosticData` (T1,T2,T3), `DiagnosticValue` (T1,T2), `GroupData`,
  `NodeData`, `NodeId`, `NodeSliceIter`, `ParsedSlots`, `VERSION`. T1 matched
  `NodeKind::Group(_)`/`Callable(_)` without ever touching the boxed payload types —
  `GroupData` in particular is tagged T1 yet only `CallableData` was used, and by **T3
  (build side)**, not T1 (read side).
- Tagged T2: `Scope` (the entire mutable-provider story went unused — `Package` sufficed
  even for scoped definitions), `FallbackProvider`, `ErrorCallableSpec`,
  `CallableDefinedAsError`, `CallableQuery`, `CallableSyntax`, `SymbolEntry`,
  `SearchedProviders`, `DefinitionOp`, `ScopeOp`, `ScopeOpError`, `ScopeStackError`,
  `ProviderError`, `ArgumentParser`, `ParsedArgumentNodes`, `FrameRole`, both derive
  macros + `ToDiagnosticValue` trait.
- The "(T1 downcast)" annotation on condition types found **no empirical support**: T1
  displayed diagnostics exclusively via `severity()`/`message()`/`span()`/`render()`/
  `render_all()` and never downcast or matched an identifier. The only condition-type
  users were T3 and T4 (2 of 19 condition types: `UnresolvableCommand`,
  `MissingMandatoryArgument`), and T4 is precisely the persona INVENTORY did *not* tag
  on conditions.

**Tagged none/near-none but used:** none — INVENTORY's three near-"none" items
(`resolve_source`, `skip_whitespace`, `PrefixEntry`) were indeed untouched. No untagged
item was used. Where tags erred, they erred wide, never narrow, with one nuance:
several node items tagged T1-only (`ParsedArguments`, `ParsedArgument`, `CallableData`,
`Descendants`, `NodeTree`) were actually used by T2/T3/T4 — the tag missed *which*
personas, not *whether*.

## 4. Friction themes across personas

Context for calibration (one paragraph, then the themes): **all four personas compiled
first try and passed their assertions first (or near-first) run**; every walkthrough
singles out doc-*comment* quality as production-grade, and diagnostics rendering, the
macro/environment symmetry, and the provenance model as excellent. Every "doc gap" below
is a missing *guide* chapter/paragraph, not a missing doc comment.

**F1. The guide is load-bearing; off-guide = signature hunt** — T1, T2, T3, T4 — **doc-gap
(highest leverage).** T1: tasks absent from learn-by-example (plain text, line/col,
diagnostics display) each forced a `src/**` signature hunt — "the correlation is perfect".
T2: `concepts-overview.md` is a self-declared skeleton, `parsing-model.md` a 57-byte stub;
the body-scoped-definitions pattern was found by grepping and guessing. T3 (F1, worst
gap): no custom-Lang guide chapter exists; the only end-to-end examples live in
`#[cfg(test)]` modules. T4: the include workflow and re-parse/span-stability contracts
are documented nowhere. Fixable entirely by writing guides.

**F2. Root/module curation is inverted** — T1, T2 (+T3/T4 as evidence) — **structural /
policy input.** T1: the guide teaches deep paths while the root re-exports ~100 names
whose autocomplete floods a consumer with staging machinery ("the curation is currently
inverted for this persona"). T2: the extender's home module (latexlike) is the only one
that *requires* full paths while unused machinery sits at root. T3: touched 52 items,
~all root-available, yet imported every one via module path. T4: the only persona to
use root paths at all (3 items). This is the empirical frame for Phase 2a.

**F3. Registration/setup ceremony** — T1, T2, T3-lite — **missing-convenience.** T1: four
nested constructors + `Arc` + `unwrap` per macro definition; `with_provider` returns a
can't-fail `Result`. T2: same, plus wishes `package.define_macro("greet", "om")`;
`insert` vs `insert_specials` flip parameter order; two documented activation idioms
(`with_provider` vs `with_seed_delta`+`push_provider`) read as two models. T3: the
generic layer lacks any shorthand factory (acceptable at ~4 lines/argument, but wished).
Pure sugar; no design change needed.

**F4. Entry path and recovery configuration scatter** — T1 (T2 minor) — **missing-convenience.**
One-call parse doesn't exist (`Language` + `Latexlike` + driver + provider knowledge
needed in minute one); `Recovery` spans three modules; `Language::<Latexlike>::default()`
turbofish; strict-vs-tolerant requires two `Language` values.

**F5. Silent traps in everyday extender flows** — T2 — **trap (worst individual finding
in the review).** (a) Registering `"\greet"` *with* backslash is accepted silently, then
fails at parse time with a message claiming the package doesn't contain it. (b) The
single-expression fallback makes `\greet word` silently take `w` as the mandatory
argument — guide never mentions it, and no "mandatory braced group, no fallback" code
exists. (c) `argument_content_nodes(i)` returns `None` for both absent-optional and
index-out-of-range. (d) `MacroSpec`-under-`CallableType::Environment` is an unchecked
convention that parses fine. (a) and (d) want insert-time validation (API); (b) wants a
new argument code + a loud doc callout; (c) a documented contract or richer return.

**F6. Line/col and position-rendering chain** — T1, T4 — **missing-convenience + one trap.**
T1: node→span→source→line_index→line_col is a 4-hop chain with `&mut` and a dual-meaning
`Option`; the 100 000-byte `max_scan_len` default silently returns `None` on files >100KB
(the trap — both personas flag it). T4: no `line_of(offset)`/`line_range(line)` inverse,
so caret/underline excerpts re-scan for `\n` by hand; `format_position` renders only span
start and isn't machine-splittable; the Arc/borrowck `LineIndex`-from-node pattern is
shown nowhere.

**F7. The cursor primitive is absent** — T4 — **missing-feature (largest genuine API gap).**
No position→node query (`node_at(offset)`) and no `NodeRef::parent()`/`ancestors()`.
Every editor integration will hand-roll the descent loop with its subtleties (half-open
containment, empty spans, trigger-token offsets); nodes found via `descendants()` cannot
recover their context at all. T4's verdict: "genuinely missing feature, not reasonable
composition left to the user."

**F8. Include/`\input` wiring is absent** — T4 — **missing-feature + doc-gap.** The
resolver seam is excellent but nothing connects it to parsing: no preset construct
triggers it, so the only workflow is an undocumented embedder-driven parse/scan/resolve/
re-parse loop producing an uncorrelated forest. `node/mod.rs` mentions mixed-origin trees
as a capability; no public path produces one. Minimum fix is a guide chapter blessing the
loop; the real fix is a construct.

**F9. Diagnostic identifiers are unguessable; no registry** — T3, T4 — **doc-gap + a
stability design question.** Both personas guessed literal identifier strings and lost a
compile/run cycle (`core.constructs.…`/`core.parse.…` vs actual
`core.nodes_parser.unresolvable-command`). The fix both found: typed
`Condition::IDENTIFIER` consts (requires importing `DiagnosticInfo`). Two follow-ups:
document "match via `T::IDENTIFIER`/`is::<T>()`, never literals" + publish an
identifier↔type registry table; and note T4's semver point that the identifier `<area>`
segment is the *internal file name*, which contradicts the documented decoupling
principle (renaming `nodes_parser.rs` would strand the identifier).

**F10. Language-designer on-ramp cliffs** — T3 — **missing-convenience + one trap.**
(a) `SimpleLang` dead-ends the moment a language has commands (blanket impl excludes a
custom driver): the step from 1 line to 9 assoc types + driver + `initial_state_data` is
discontinuous. (b) 13-field `TokenRules` + 4-field `StateData` must be transcribed from
a doc comment because the neutral value isn't callable (`TokenRules::disabled()`/
`StateData::neutral()` missing). (c) Specials require two hand-written delegating hooks
plus a gate; forgetting `specials_trigger_chars` is a documented-but-silent failure
(the trap). ~7 concept clusters across 5 modules before "hello world".

**F11. Takeover-parser staging boilerplate** — T3 — **missing-convenience.** ~40 of 132
task-5 lines are `CallableData` literal + builder calls + span bookkeeping every takeover
parser will repeat (`stage_callable` wished); no terminator-less raw-state helper beside
`verbatim_state_delta` (forced T3's only deliberate implementation-body read);
`ParsedArguments`/`ParsedSlots` construct only via undiscoverable `From<Vec>` impls.

**F12. Generic node-inspection accessors missing** — T1, T2, T4 — **missing-convenience.**
Kind label needs a 5-arm match exposing boxed payload shapes (T1, T4 independently wish
`NodeKind::label()`/`name()`); three per-type name getters but no generic callable name
(T2; T1 found the existing generic `name()` only by scanning `node_ref.rs` — not in the
guide); `descendants()` carries no depth (T4); `argument_nodes` vs
`argument_content_nodes` naming doesn't teach the distinction (T1, T2).

**F13. Diagnostics presentation nits** — T1, T2, T3 — **missing-convenience/cosmetic.**
Diagnostics arrive in recovery order, not source order, with no sort helper (T1); the
`@` anonymous-origin placeholder in rendered positions reads as a formatting bug (T2,
T3 independently); empty-span `content()` yields `""` for missing-argument diagnostics
(T2); strict unclosed-group abort printed no traceback while missing-argument printed a
full one (T3, unverified).

Split for the decision brief: **doc-gap-only themes: F1, F9-part** (guides, registry,
contract sentences). **API-code themes: F5, F7, F8** (validation, cursor primitive,
include construct). **Sugar-only themes: F3, F4, F6, F10, F11, F12, F13** (additive
helpers, no design change). F2 is the policy decision itself.

## 5. Consolidated wishlist

All "wished it existed" entries from the four surfaces, deduped (33 → 30 rows; kind:
**S** = additive sugar over existing capability, **C** = new capability, **D** = doc/guide
fix, **V** = validation/behavior change). Names as wished by the personas.

| # | Wish (as proposed) | Wanted by | Kind | Friction theme |
|---|---|---|---|---|
| 1 | `latexlike::parse(src)` / `parse_tolerant(src)` → `ParseResult` | T1 | S | F4 |
| 2 | `Language::with_recovery(Recovery)` / `::tolerant()` | T1 | S | F4 |
| 3 | Standard LaTeX definitions package (`\emph`, `\cite`, `itemize`, …), even explicitly incomplete | T1 | C | F1/F3 (onboarding) |
| 4 | `extract::plain_text(nodes)` with a documented (even naive) callable/specials policy | T1 | C | — (T1 task 3; pylatexenc-parity gap) |
| 5 | Registration one-liner: `Package::define_macro(name, codes)` / `define_environment` (T2); `MacroSpec::with_args("om")` + `insert(impl Into<Arc<_>>)` (T1) | T1, T2 | S | F3 |
| 6 | `MacroSpec::empty()` (or `Default`) for zero-arg macros | T2 | S | F3 |
| 7 | Named-code factory: `argument_specs([("o","greeting"), ("m","name")])` | T2 | S | F3 |
| 8 | Core-level argument-spec shorthand factory (cousin of latexlike's codes; mandatory/optional/marker) | T3 | S | F3 |
| 9 | Argument code for "mandatory braced group, **no** expression fallback" | T2 | C (small) | F5b |
| 10 | Insert-time rejection (or warning) of escape-prefixed names; near-miss keys in resolve errors | T2 | V | F5a |
| 11 | Distinguishable/documented absent-optional vs out-of-range in `argument_content_nodes` | T2 | D or S | F5c |
| 12 | `EnvironmentSpec::with_body_provider(Arc<Package>)` | T2 | S | F3 (keeps `state::` out of extender code) |
| 13 | Canned text-mode-argument helper in latexlike (the `\text`/`\mbox` shape) | T2 | S | F3 (replaces 4-internal-imports recipe) |
| 14 | `Language::with_providers(impl IntoIterator<…>)` | T2 | S | F3 (minor) |
| 15 | Generic `callable_name()` beside the three per-type getters | T2 | S | F12 |
| 16 | `NodeKind::label()` / `name() -> &'static str` (or `Display`) | T1, T4 | S | F12 |
| 17 | `StateData::neutral()` / `TokenRules::disabled()` | T3 | S | F10b |
| 18 | Non-dead-end quick-start tier: `SimpleLang` with overridable `Driver`, or `ScopeResolvingDriver<CT>` | T3 | C (small) | F10a |
| 19 | Packaged specials wiring (mixin/derive or one-line delegation to `ScopeStack`) | T3 | S | F10c |
| 20 | `stage_callable(cx, &invocation, children, slots, end)` for takeover parsers | T3 | S | F11 |
| 21 | Terminator-less raw-state delta helper (rest-of-line / until-predicate), sibling of `verbatim_state_delta` | T3 | S | F11 |
| 22 | `ParsedArguments::new(Vec)` / `ParsedSlots::new(Vec)` (the `From` impls exist, undiscoverable) | T3 | S | F11 |
| 23 | Diagnostic-identifier registry page + stated rule "match via `T::IDENTIFIER` / `is::<T>()`" | T3, T4 | D | F9 |
| 24 | `NodeTree::node_at(offset)` (innermost) + `NodeRef::parent()` / `ancestors()` | T4 | C | F7 |
| 25 | `\input` wiring story: preset construct triggering the resolver mid-parse, or a guide chapter blessing the embedder loop | T4 | C or D | F8 |
| 26 | `LineIndex::line_of(offset) -> Range<usize>` / `line_range(line_no)` | T4 | S | F6 (T4's top small wish) |
| 27 | `NodeRef::line_col()` / `SourceSpan::line_col()`; non-`&mut` `LineIndex`; `line_col_span(span)` | T1, T4 | S | F6 |
| 28 | Compiler-style caret/underline excerpt renderer; machine-splittable position format | T4 | S/C | F6 |
| 29 | Depth-carrying descendants iterator (`(depth, NodeRef)` or `Descendants::depth()`) | T4 | S | F12 |
| 30 | `Diagnostics::sorted_by_position()` (or documented source-order iteration) | T1 | S | F13 |

Doc-only wishes folded into F1 rather than listed: re-parse/span-stability paragraph (T4),
`LineIndex`-from-a-node guide example (T4), `body()` `None` semantics (T2), `descendants()`
self-inclusion sentence (T1), enum-alternative documentation of the `["o","m"]` codes (T1).
Overlap summary: 20 of 30 are pure additive sugar; 5 are new capability (3, 4, 24, 25,
plus small 9/18); 1 is a behavior/validation change (10); the rest doc-first. Multi-persona
wishes (5, 16, 23, 27) are the strongest candidates; 24 and 25 are single-persona but
T4-critical.

## 6. Headline stats

| Persona | Distinct items used | Surface's own count (incl. methods) | Root-available | Reached via root path in code |
|---|---|---|---|---|
| T1 consumer | 24 | ~41 | 17/24 (71%) | 0 (guide-style module paths throughout) |
| T2 extender | 22 | ~50 | 15/22 (68%) | 0 recorded (guide-style imports) |
| T3 lang designer | 52 | ~55 | 51/52 (98%) | 0 (explicit: all module paths) |
| T4 tooling | 32 | ~100 | 25/32 (78%) | 3 (9%): `format_position`, `format_traceback`, `UnresolvableCommand` |

- Union: **73 distinct items** (36% of the 205-item API); 130 persona-item uses; mean
  1.78 personas/item.
- Of the **140 root re-exports**: **64 (46%) used at all** by anyone (almost always via
  module paths), **76 (54%) touched by nobody**, and just **3 (2%) ever accessed through
  the root path** — all by T4, and two of those are the `format_*` functions whose root
  spelling is arguably their natural home.
- The starkest single datum for Phase 2a: the root re-export layer received essentially
  **zero traffic as an access path** while costing T1 real friction (autocomplete flood,
  F2); meanwhile all 9 module-only items that personas *did* need (latexlike ×7,
  `extract::content_as_chars`, `GroupArgumentParser`) sit off-root by design and caused
  no reach failures — only T1's note that `extract`'s mandatory deep path is
  *inconsistent* with everything else being dual-pathed.
- T1 uniqueness is zero: a curated "consumer core" (24 items + the 6-name latexlike happy
  path within it) is a strict subset of what T2–T4 need anyway.


