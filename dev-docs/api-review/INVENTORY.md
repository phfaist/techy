# techy public API inventory (Phase 0)

Generated 2026-07-28 from rustdoc JSON (format_version 57, `RUSTC_BOOTSTRAP=1 cargo rustdoc … --output-format json`) on workspace `/Users/philippe/projects/techy` @ commit 2110bbb (branch main, clean).

## Summary

### Item counts

**Per crate**: `techy` 203 public items (excluding `#[doc(hidden)] __private` and the `cfg(doc)` `guide` module), `techy-derive` 2 (both derive macros, both re-exported through `techy::error`). Total 205.

**Per module** (techy): source 14, error 14 (incl. 2 derive-macro re-exports), token 20, state 9, spec 6, scopes 16, node 30, node::extract 9, constructs 52, engine 9, latexlike 23, crate root 1 (`VERSION`).

**Per kind** (techy): 122 structs, 33 enums, 19 functions, 16 traits, 10 type aliases, 2 derive-macro re-exports, 1 const. 38 items are `#[non_exhaustive]` (mostly diagnostic conditions and public enums). No `#[deprecated]` items; the only `#[doc(hidden)]` public item is `__private`.

### Doc coverage

**100 %.** All 203 techy items and both techy-derive macros have doc comments; a clean `cargo build` emits **zero** `missing_docs` warnings (the workspace has `missing_docs = warn`, and the lint also covers fields, variants, and methods). No missing-docs list to report.

### Root re-export analysis

- **140 items (138 distinct names) are visible at the crate root** — everything except `latexlike` (23, deliberate), `node::extract` (9), the constructs argument/takeover parsers and helpers (22), the 8 node ext aliases, and `token::PrefixEntry`. The root is close to "the whole API minus the preset".
- Of the 140, **~70 carry no provisional T1 or T2 tag** (T3/T4-only): all of token's machinery (12 non-cond items), state's Lang plumbing (6), the constructs core-dispatch layer (14), 12 constructs condition types + their 3 payload-detail enums, engine's session/driver internals (7), source's provenance/resolver surface (10), and node's builder internals (7). Under a "simple things at root, detail in modules" policy these are the candidates for demotion from the root (not from the crate).
- The ~18 root-level diagnostic condition structs are a judgment call: T1 consumers downcast to them, but only in advanced error handling; they could live with their modules like the three verbatim/tack-on conditions already do.

### Provisional tier size estimate (names touched, double-counting allowed; all provisional)

- **T1** ≈ 47 core names (root: Source/SourceSpan/TextContent/LineIndex, the 10-name error/diagnostics surface, 14 node-reading names, Language/ParseResult/VERSION; module-level: 9 extract names, 7 latexlike names) — plus ~22 condition structs as optional downcast targets.
- **T2** ≈ 60–70 names (spec 6, scopes 16, latexlike 18, token rule structs 5, state deltas 3, 7 argument parsers + Invocation, diagnostics-defining traits/macros 6, plus shared T2/T3 items).
- **T3** ≈ 95–105 names (all of token and state, most of constructs, engine internals, node builder surface, ext aliases) — the largest tier by far.
- **T4** ≈ 13 names (all in source).
- **none**: no item ended up wholly untagged; nearest to "none" are `resolve_source` (redundant with `Language::resolve_source`), `skip_whitespace` at root, and `PrefixEntry`.

### Oddities / candidates for discussion

1. **Root breadth**: 138 distinct root names for a library whose T1 story needs ~25; about half the root surface has no T1/T2 tag. Largest single contributor: constructs (30 root names, 16 of them condition types).
2. **Condition family split**: 16 of 19 constructs conditions are at root; `ExpectedVerbatimDelimiter`, `UnterminatedVerbatim`, `RepeatedTackOnField` are not (they follow their non-root parsers — internally consistent, but the family is split across visibility levels).
3. **`resolve_source` free fn** duplicates `Language::resolve_source` — one of them looks removable from the public surface.
4. **`NodeBuildError` has 15 variants** — the largest enum; builder-invariant reporting exposed at crate root.
5. **Error-type granularity**: ~10 distinct error types, several single-use (`ResolveError`, `DeriveError`, `ArgumentCodeError`, `ExtractError`, `ScopeStackError`); scopes alone has three (`ProviderError`, `ScopeOpError`, `ScopeStackError`).
6. **Builder internals at root**: `NodeTreeBuilder`, `StagedNodes`, `StagedNodeView`, `BuildId`, `ContentNodes`, `ChildRegion`, `check_tree_invariants` — T3-only staging machinery beside the T1 node-reading API.
7. **Two "frame" vocabularies**: engine `Frame`/`FrameTitle<L>` (live stack) vs error `TraceFrame<O>` (diagnostics) — related but distinct types with adjacent names, all at root.
8. **Deliberate name doubling**: `DiagnosticInfo` and `ToDiagnosticValue` each exist as trait + derive macro under one name (standard Rust pattern; costs 2 root items).
9. **`PrefixEntry`** is the only token item not at root, yet root-visible `PrefixTable::entries` returns it — visibility-level inconsistency.
10. **13-field twins**: `token::TokenRules` and `state::TokenRulesOverrides` mirror each other field-for-field across modules — a parallel-maintenance surface (by design per the delta model, but worth confirming).
11. **`ImplementationError`** is a very generic name for one specific condition (contract-violation reporting) — naming-review candidate.
12. **`NodeRef` carries 42 methods**, including latexlike sugar (`is_math_group`, `macro_name`, `environment_name`, …) via `impl NodeRef<'t, Latexlike>` in the preset — properly gated, but the method list on the core type is large.

## Legend

- **Root?** — re-exported at the `techy` crate root (`techy/src/lib.rs`). `latexlike` is deliberately not re-exported at root.
- **Docs?** — item has a doc comment.
- **Flags** — `NE` = `#[non_exhaustive]`; `pubf=n` = n public fields (`pubf=0` = fully private fields); `Nv` = enum variant count; generic parameters shown as `<…>` (lifetimes included). No item in the workspace is `#[deprecated]`; the only `#[doc(hidden)]` public item is `techy::__private` (derive support, excluded from this inventory).
- **Tier (prov.)** — provisional persona tags, multiple allowed; **all subject to user overrule**:
  - **T1** document consumer (parse with latexlike preset, walk/query AST, extract text/arguments, display diagnostics)
  - **T2** extender (custom macros/environments via specs, packages, scopes)
  - **T3** language designer (custom `Lang`, token rules, custom construct parsers, state deltas)
  - **T4** tooling author (source model, spans, provenance, `SourceResolver`, line/column)
  - **none** — no persona plausibly needs it directly (demotion candidate)
- "cond" in Notes = diagnostic condition payload struct (plain data + `Display`); its *direct* users are T1 consumers who downcast/match `identifier()`, so condition types get the producing layer's tier plus "(T1 downcast)".
- `*` after a trait member = has a default implementation.

## Module `source` (S0) — 14 items

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `Source` | struct | Y | Y | `<O>`, pubf=0 | T1, T4 | entry point of every parse; `Arc<Source>`-shared |
| `Span` | struct | Y | Y | pubf=0 | T3, T4 | plain byte range; T1 sees `SourceSpan` instead |
| `SourceSpan` | struct | Y | Y | `<O>`, pubf=0 | T1, T4 | on every node and diagnostic |
| `SourceProvenance` | enum | Y | Y | `<O>`, 3v | T4 | Primary/Resolved/Synthesized |
| `ProvenanceChain` | struct | Y | Y | `<'a, O>`, pubf=0 | T4 | iterator over provenance hops |
| `SourceOrigin` | trait | Y | Y | 1 member | T3, T4 | bound of `Lang::SourceOrigin` |
| `SourceResolver` | trait | Y | Y | `<O>`, 1 member | T4 | embedder I/O seam (`\input`-like); T1 touches only to enable resolution |
| `NoResolver` | struct | Y | Y | unit | T4 | zero-cost default |
| `MapResolver` | struct | Y | Y | pubf=0 | T4 | in-memory convenience (tests/demos) |
| `ResolveError` | struct | Y | Y | pubf=0 | T4 | error of `SourceResolver::resolve` only |
| `ResolvedContent` | struct | Y | Y | `<O>`, pubf=2 | T4 | resolver return value |
| `resolve_source` | fn | Y | Y | `<O, R>` | T4 | free fn; overlaps `Language::resolve_source` |
| `TextContent` | enum | Y | Y | 2v | T1, T3 | span-backed or owned node text |
| `LineIndex` | struct | Y | Y | `<'c>`, pubf=0 | T1, T4 | lazy line/col, display-only |

Method surfaces: `Source` — new, resolved, synthesized, with_origin, with_provenance, with_line_column_number_offsets, content, origin, provenance, line_number_offset, column_number_offset, line_index, provenance_chain. `Span` — new, empty, start, end, len, is_empty, range, extend_to, cover, slice, get. `SourceSpan` — new, entire, source, start, end, range, span, len, is_empty, content, same_source. `LineIndex` — new, with_line_column_number_offsets, set_max_scan_len, line_col. `MapResolver` — new, insert, with_reference_as_origin. `ResolveError` — new, with_cause, reference, message. `TextContent` — empty, resolve, materialized, is_owned.

## Module `error` (S0) — 14 items

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `Diagnostic` | struct | Y | Y | `<O>`, pubf=0 | T1 | |
| `Diagnostics` | struct | Y | Y | `<O>`, pubf=0 | T1 | sink + query; `DEFAULT_LIMIT` const |
| `Severity` | enum | Y | Y | 3v | T1 | Error/Warning/Note |
| `ParseError` | struct | Y | Y | `<O>`, pubf=0 | T1 | the abort error; no recovery payload |
| `TraceFrame` | struct | Y | Y | `<O>`, pubf=0 | T1 | traceback frame on Diagnostic/ParseError |
| `DiagnosticData` | trait | Y | Y | 3 members | T1, T2, T3 | object-safe carrier; usually via `DiagnosticInfo` |
| `DiagnosticInfo` | trait | Y | Y | 2 members | T2, T3 | impl to define third-party conditions |
| `DiagnosticInfo` | derive macro | Y | Y (in techy-derive) | | T2, T3 | re-export of `techy_derive::DiagnosticInfo`; same name as trait (std pattern) |
| `ToDiagnosticValue` | trait | Y | Y | 1 member | T2, T3 | payload-field serialization |
| `ToDiagnosticValue` | derive macro | Y | Y (in techy-derive) | | T2, T3 | re-export of `techy_derive::ToDiagnosticValue` |
| `DiagnosticValue` | enum | Y | Y | 6v | T1, T2 | wire value tree (serializable_data) |
| `Recovery` | enum | Y | Y | 2v | T1, T3 | Tolerant/Strict; T1 passes it to `LatexlikeDriver::new` |
| `format_position` | fn | Y | Y | `<O>` | T1 | |
| `format_traceback` | fn | Y | Y | `<O>` | T1 | |

Method surfaces: `Diagnostic` — new, error, warning, note, severity, data, identifier, message, span, frames, render. `Diagnostics` — DEFAULT_LIMIT (const), new, with_limit, push, len, is_empty, limit, suppressed, has_errors, iter, with_identifier, conditions, as_slice, render_all. `ParseError` — new, from_token_error, with_frames, data, identifier, message, span, frames, render. `DiagnosticData` (trait) — identifier, serializable_data, clone_box. `DiagnosticInfo` (trait) — const IDENTIFIER, serializable_data\*. `TraceFrame` — new, title, span.

## Module `token` (S1) — 20 items

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `Token` | struct | Y | Y | `<'s, L>`, pubf=3 | T3 | zero-copy; carries `Span` not `SourceSpan` |
| `TokenKind` | enum | Y | Y | `<'s, L>`, 8v | T3 | |
| `TokenReader` | trait | Y | Y | `<'s, L>`, 6 members | T3 | |
| `StdTokenReader` | struct | Y | Y | `<'s>`, pubf=0 | T3 | |
| `TokenRules` | struct | Y | Y | `<L>`, pubf=13 | T2, T3 | 13 pub fields; twin of `TokenRulesOverrides` (state) |
| `CommandRule` | struct | Y | Y | pubf=2 | T2, T3 | |
| `CommentRule` | struct | Y | Y | pubf=1 | T2, T3 | |
| `GroupRule` | struct | Y | Y | `<L>`, pubf=3 | T2, T3 | |
| `WhitespaceRules` | struct | Y | Y | pubf=1 | T2, T3 | |
| `SpecialsMatch` | struct | Y | Y | `<'s, L>`, pubf=4 | T3 | return of `scan_specials` hooks |
| `TriggerChars` | enum | Y | Y | 2v | T3 | specials pre-filter |
| `PrefixTable` | struct | Y | Y | `<L>`, pubf=0 | T3 | derived cache on state |
| `PrefixEntry` | struct | n | Y | `<L>`, pubf=0 | T3 | only non-root token item, yet returned by root-visible `PrefixTable::entries` |
| `TokenError` | struct | Y | Y | `<'s, L>`, pubf=0 | T3 | carries recovery token |
| `TokenErrorKind` | enum | Y | Y | 3v, NE | T3 | |
| `TokenRecovery` | struct | Y | Y | `<'s, L>`, pubf=2 | T3 | |
| `TokenResult` | type alias | Y | Y | `<'s, L, T>` | T3 | |
| `EndOfStreamAfterEscape` | struct | Y | Y | pubf=1, NE | T3 | cond (T1 downcast) |
| `ForbiddenChar` | struct | Y | Y | pubf=1, NE | T3 | cond (T1 downcast) |
| `skip_whitespace` | fn | Y | Y | `<L>` | T3 | reader helper at crate root |

Method surfaces: `TokenReader` (trait) — peek, move_past, move_to, move_to_pos, pos, next\*. `StdTokenReader` — new, content, pos, is_at_end, move_to_pos. `Token` — new, post_space. `TokenError` — new, kind, span, recovery, into_recovery. `PrefixTable` — for_rules, match_at, entries. `TriggerChars` — may_start, union. `PrefixEntry` — delim, open, close.

## Module `state` (S1) — 9 items

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `Lang` | trait | Y | Y | 14 members (9 assoc types) | T3 | the central language contract |
| `SimpleLang` | trait | Y | Y | 0 members | T3 | marker/blanket convenience |
| `ClosedVocabulary` | trait | Y | Y | 1 member (`const ALL`) | T3 | bound for GroupTypeId/CallableTypeId/ModeId |
| `NodeExtTypes` | trait | Y | Y | 8 assoc types | T3 | ext bundle; mirrored by 8 node aliases |
| `ParsingState` | struct | Y | Y | `<L>`, pubf=0 | T2, T3 | reachable from `NodeRef::parsing_state` (T1-adjacent) |
| `ParsingStateDelta` | struct | Y | Y | `<L>`, pubf=5 | T2, T3 | carried by `ArgumentSpec::with_state_delta` |
| `StateData` | struct | Y | Y | `<L>`, pubf=4 | T3 | |
| `TokenRulesOverrides` | struct | Y | Y | `<L>`, pubf=13 | T2, T3 | 13 fields mirror `TokenRules` field-for-field |
| `DeriveError` | struct | Y | Y | `<L>`, pubf=3 | T3 | error of state derivation only |

Method surfaces: `Lang` (trait) — 9 assoc types (GroupTypeId, CallableTypeId, ModeId, StateExt, Event, SessionExt, SourceOrigin, NodeExts, Driver) + initial_state_data\*, finalize_transition\*, scan_specials\*, specials_trigger_chars\*, finalize_node\*. `ParsingState` — initial, derived, rules, scopes, mode, ext, prefix_table, trigger_chars. `ParsingStateDelta` — new, rules, scope_op, push_provider, mode, ext, event. `TokenRulesOverrides` — apply.

## Module `spec` (S1) — 6 items

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `CallableSpec` | trait | Y | Y | `<L>`, 4 members (all defaulted) | T2 | |
| `StdCallableSpec` | struct | Y | Y | `<L>`, pubf=1 | T2 | |
| `ArgumentSpec` | struct | Y | Y | `<L>`, pubf=3 | T2 | |
| `ArgumentParser` | trait | Y | Y | `<L>`, 2 members | T2, T3 | an argument *is* a parser |
| `ParsedArgumentNodes` | struct | Y | Y | pubf=2 | T2, T3 | return of `parse_argument` |
| `FrameRole` | enum | Y | Y | 2v, NE | T2 | param of `stack_frame_title` |

Method surfaces: `CallableSpec` (trait) — arguments\*, requires_content\*, make_invocation_parser\*, stack_frame_title\*. `ArgumentParser` (trait) — parse_argument, can_match_empty\*. `ArgumentSpec` — new, named, with_state_delta.

## Module `scopes` (S1) — 16 items

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `SpecsProvider` | trait | Y | Y | `<L>`, 6 members | T2, T3 | stack-entry contract |
| `Package` | struct | Y | Y | `<L>`, pubf=0 | T2 | immutable std provider |
| `Scope` | struct | Y | Y | `<L>`, pubf=0 | T2 | mutable-by-replacement std provider |
| `ScopeStack` | struct | Y | Y | `<L>`, pubf=0 | T2, T3 | |
| `FallbackProvider` | struct | Y | Y | `<L>`, pubf=0 | T2, T3 | unknown-callable policy |
| `ErrorCallableSpec` | struct | Y | Y | pubf=1 | T2 | "defined to be an error" |
| `CallableDefinedAsError` | struct | Y | Y | pubf=2, NE | T2 | cond (T1 downcast) |
| `CallableQuery` | struct | Y | Y | `<'a, 's, L>`, pubf=4 | T2, T3 | param of `retrieve_spec` |
| `CallableSyntax` | enum | Y | Y | 3v | T2 | |
| `SymbolEntry` | struct | Y | Y | `<'a, L>`, pubf=3 | T2 | row of `iter_symbols` |
| `SearchedProviders` | struct | Y | Y | `<'a, L>`, pubf=0 | T2, T3 | diagnostic detail (which providers were tried) |
| `DefinitionOp` | enum | Y | Y | `<L>`, 2v | T2 | |
| `ScopeOp` | enum | Y | Y | `<L>`, 6v | T2 | |
| `ScopeOpError` | enum | Y | Y | 2v, NE | T2, T3 | one of three scopes error types |
| `ScopeStackError` | struct | Y | Y | pubf=2 | T2, T3 | one of three scopes error types |
| `ProviderError` | enum | Y | Y | 3v, NE | T2, T3 | one of three scopes error types |

Method surfaces: `SpecsProvider` (trait) — name, retrieve_spec, scan_specials\*, specials_trigger_chars\*, with_definitions\*, iter_symbols\*. `Package` — new, name, insert, insert_in_modes, insert_specials, insert_specials_in_modes, set_visible_modes, get, len, is_empty. `Scope` — new, name, insert, remove, get, len, is_empty. `ScopeStack` — new, push, providers, provider_names, searched_providers, len, is_empty, retrieve_spec, scan_specials, specials_trigger_chars, iter_symbols, apply_op. `FallbackProvider` — new, set, get. `ErrorCallableSpec` — new, with_detail. `CallableQuery` — new, with_token.

## Module `node` (S1) — 30 items (+ 9 in `node::extract`)

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `NodeTree` | struct | Y | Y | `<L>`, pubf=0 | T1 | |
| `NodeRef` | struct | Y | Y | `<'t, L>`, pubf=0 | T1 | 42 public methods (incl. latexlike sugar defined in preset) |
| `NodeKind` | enum | Y | Y | `<L>`, 5v | T1 | closed: Chars/Group/Callable/Comment/List |
| `NodeData` | struct | Y | Y | `<L>`, pubf=0 | T1 | |
| `NodeId` | struct | Y | Y | tuple, pubf=0 | T1 | |
| `NodeSlice` | struct | Y | Y | `<'t, L>`, pubf=0 | T1 | |
| `NodeSliceIter` | struct | Y | Y | `<'t, L>`, pubf=0 | T1 | bare iterator type at root |
| `Descendants` | struct | Y | Y | `<'t, L>`, pubf=0 | T1 | bare iterator type at root |
| `GroupData` | struct | Y | Y | `<L>`, pubf=4 | T1 | |
| `CallableData` | struct | Y | Y | `<L>`, pubf=7 | T1 | |
| `ParsedArguments` | struct | Y | Y | `<L>`, pubf=1 | T1 | |
| `ParsedArgument` | struct | Y | Y | `<L>`, pubf=3 | T1 | |
| `ParsedSlots` | struct | Y | Y | `<L>`, pubf=1 | T1 | |
| `ParsedSlot` | struct | Y | Y | `<L>`, pubf=3 | T1 | |
| `ContentNodes` | enum | Y | Y | 2v | T3 | region content designation (builder side) |
| `ChildRegion` | struct | Y | Y | pubf=0 | T2, T3 | two-phase staged/resolved region |
| `NodeTreeBuilder` | struct | Y | Y | `<L>`, pubf=0 | T3 | driven by ParserSession |
| `StagedNodes` | struct | Y | Y | `<'b, L>`, pubf=0 | T3 | builder internals at root |
| `StagedNodeView` | struct | Y | Y | `<'b, L>`, pubf=0 | T3 | builder internals at root |
| `BuildId` | struct | Y | Y | tuple, pubf=0 | T3 | staging-phase node id |
| `NodeBuildError` | enum | Y | Y | 15v, NE | T3 | 15 variants; invariant-violation reporting |
| `check_tree_invariants` | fn | Y | Y | `<L>` | T3 | testing/debug aid at root |
| `NodeExt` | type alias | n | Y | `<L>` | T3 | 8 aliases mirroring `NodeExtTypes` assoc types |
| `CharsNodeExt` | type alias | n | Y | `<L>` | T3 | |
| `GroupNodeExt` | type alias | n | Y | `<L>` | T3 | |
| `CallableNodeExt` | type alias | n | Y | `<L>` | T3 | |
| `CommentNodeExt` | type alias | n | Y | `<L>` | T3 | |
| `ListNodeExt` | type alias | n | Y | `<L>` | T3 | |
| `ArgumentExt` | type alias | n | Y | `<L>` | T3 | |
| `SlotExt` | type alias | n | Y | `<L>` | T3 | |

Method surfaces: `NodeRef` — is_math_group, math_style, macro_name, environment_name, specials_name (preset sugar); id, kind, ext, span, span_content, parsing_state, summary, child_count, child, children, descendants, is_chars, is_group, is_callable, is_comment, is_list, chars, comment, comment_start, comment_post_space, group, group_type, group_delimiters, callable, callable_type, name, spec, post_space, arguments, slots, argument_nodes, argument_nodes_named, argument_content_nodes, argument_content_nodes_named, slot_content_parent, slot_content_nodes, slot_content_nodes_named, body. `NodeTree` — root, node, get, descendants, node_count, iter_storage_order, nodes_in, materialize. `NodeSlice` — len, is_empty, get, first, last, iter, range, span, source_text. `NodeTreeBuilder` — new, add, add_with_ext, staged_nodes, finish. `ParsedArguments`/`ParsedSlots` — empty, len, is_empty, get, get_named, iter. `ChildRegion` — new, single, is_resolved, children, content_range, content_parent. `NodeKind` — chars, group, callable, comment, list. `StagedNodeView` — id, kind, ext, span, parsing_state, children.

### Submodule `node::extract` — 9 items (none at root)

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `content_as_chars` | fn | n | Y | `<'t, L>` | T1 | |
| `parse_keyval` | fn | n | Y | `<L>` | T1 | |
| `split_at_chars` | fn | n | Y | `<L>` | T1 | |
| `split_embellishments` | fn | n | Y | `<L>` | T1 | |
| `split_tack_on_fields` | fn | n | Y | `<L>` | T1 | |
| `KeyVals` | struct | n | Y | `<L>`, pubf=0 | T1 | |
| `KeyValEntry` | struct | n | Y | `<'k, L>`, pubf=0 | T1 | |
| `Split` | struct | n | Y | `<L>`, pubf=0 | T1 | |
| `ExtractError` | enum | n | Y | 4v, NE | T1 | |

Method surfaces: `KeyVals` — len, is_empty, keyval, get, iter, get_combined_with, tree, into_tree. `KeyValEntry` — key, value, value_content. `Split` — len, is_empty, segment, segments, tree, into_tree.

## Module `constructs` (S1) — 52 items (largest module; 30 at root, 22 not)

Core contract and dispatch (all at root):

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `ConstructParser` | trait | Y | Y | `<L>`, 2 members (`type Output`, `parse`) | T3 | "the single most important trait in the system" |
| `ConstructParserResult` | type alias | Y | Y | `<L, T>` | T3 | |
| `ParseContext` | struct | Y | Y | `<'a, 's, L>`, pubf=5 | T3 | the one-value context |
| `Invocation` | struct | Y | Y | `<'a, 's, L>`, pubf=4 | T2, T3 | input to invocation/argument parsers |
| `NodesParser` | struct | Y | Y | `<'p, L>`, pubf=0 | T3 | content dispatch loop |
| `NodesOutcome` | struct | Y | Y | `<L>`, pubf=3 | T3 | |
| `GroupParser` | struct | Y | Y | `<'p, L>`, pubf=0 | T3 | |
| `StopSpec` | struct | Y | Y | `<'p, L>`, pubf=2 | T3 | |
| `StopCause` | enum | Y | Y | 4v | T3 | |
| `TokenStopCondition` | struct | Y | Y | `<'p, L>`, pubf=2 | T3 | |
| `TokenStopKind` | enum | Y | Y | `<'p, L>`, 4v | T3 | |
| `ChildStateSpec` | struct | Y | Y | `<'p, L>`, pubf=2 | T3 | descent policy |
| `GroupChildState` | enum | Y | Y | `<'p, L>`, 3v | T3 | |
| `InvocationChildState` | enum | Y | Y | `<'p, L>`, 3v | T3 | |

Standard argument parsers and takeover parsers (none at root):

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `GroupArgumentParser` | struct | n | Y | `<L>`, pubf=0 | T2 | delimited-group argument |
| `OptionalGroupArgumentParser` | struct | n | Y | `<L>`, pubf=0 | T2 | |
| `CharsGroupArgumentParser` | struct | n | Y | `<L>`, pubf=0 | T2 | |
| `MarkerArgumentParser` | struct | n | Y | pubf=0 | T2 | literal marker (e.g. `*`) |
| `VerbatimArgumentParser` | struct | n | Y | `<L>`, pubf=0 | T2 | `\verb`-style |
| `EmbellishmentsArgumentParser` | struct | n | Y | pubf=0 | T2 | |
| `TackOnFieldsArgumentParser` | struct | n | Y | `<L>`, pubf=0 | T2 | |
| `ExpressionParser` | struct | n | Y | unit | T3 | single-expression reader |
| `StdInvocationParser` | struct | n | Y | `<'a, 's, L>`, pubf=0 | T3 | behind `make_invocation_parser` factory |
| `EnvironmentBodyParser` | struct | n | Y | `<'p, L>`, pubf=0 | T3 | |
| `EnvironmentBody` | struct | n | Y | pubf=3 | T3 | its output |
| `VerbatimBodyParser` | struct | n | Y | `<'p, L>`, pubf=0 | T3 | |
| `NameGroup` | struct | n | Y | pubf=2 | T3 | output of `read_rigid_name_group` |
| `ArgumentNoise` | struct | n | Y | `<'s, L>`, pubf=3 | T3 | output of `scan_argument_noise` |
| `parse_declared_arguments` | fn | n | Y | `<L>` | T3 | |
| `read_rigid_name_group` | fn | n | Y | `<L>` | T3 | |
| `scan_argument_noise` | fn | n | Y | `<'s, L>` | T3 | |
| `stage_pre_space` | fn | n | Y | `<L>` | T3 | |
| `verbatim_state_delta` | fn | n | Y | `<L>` | T3 | |

Diagnostic condition types (16 of 19 at root; all NE, all with `new()`):

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `UnresolvableCommand` | struct | Y | Y | pubf=3, NE | T3 | cond (T1 downcast) |
| `CommandResolutionFailed` | struct | Y | Y | pubf=3, NE | T3 | cond (T1 downcast) |
| `UnclosedGroup` | struct | Y | Y | pubf=2, NE | T3 | cond (T1 downcast) |
| `UnclosedGroupFound` | enum | Y | Y | 2v, NE | T3 | payload detail of `UnclosedGroup` |
| `StrayGroupClose` | struct | Y | Y | pubf=1, NE | T3 | cond (T1 downcast) |
| `MissingMandatoryArgument` | struct | Y | Y | pubf=1, NE | T2, T3 | cond (T1 downcast) |
| `ExpectedExpressionArgument` | struct | Y | Y | pubf=1, NE | T2, T3 | cond (T1 downcast) |
| `ExpressionCallableRequiresContent` | struct | Y | Y | pubf=1, NE | T2, T3 | cond (T1 downcast) |
| `MissingEnvironmentTerminator` | struct | Y | Y | pubf=2, NE | T3 | cond (T1 downcast) |
| `MissingTerminatorFound` | enum | Y | Y | 2v, NE | T3 | payload detail of `MissingEnvironmentTerminator` |
| `EnvironmentTerminatorMismatch` | struct | Y | Y | pubf=2, NE | T3 | cond (T1 downcast) |
| `MalformedEnvironmentTerminator` | struct | Y | Y | pubf=1, NE | T3 | cond (T1 downcast) |
| `ScopeOpFailed` | struct | Y | Y | pubf=1, NE | T2, T3 | cond (T1 downcast) |
| `UnusableRecoveryToken` | struct | Y | Y | pubf=2, NE | T3 | cond (T1 downcast) |
| `UnusableRecoveryTokenKind` | enum | Y | Y | 2v, NE | T3 | payload detail of `UnusableRecoveryToken` |
| `ImplementationError` | struct | Y | Y | pubf=1, NE | T3 | cond; contract-violation reporting (panic-free policy) |
| `ExpectedVerbatimDelimiter` | struct | n | Y | pubf=1, NE | T2, T3 | cond — NOT at root, unlike siblings |
| `UnterminatedVerbatim` | struct | n | Y | pubf=1, NE | T2, T3 | cond — NOT at root, unlike siblings |
| `RepeatedTackOnField` | struct | n | Y | pubf=2, NE | T2, T3 | cond — NOT at root, unlike siblings |

Method surfaces: `ParseContext` — new, probe_token, parse_scoped, recover, derived_state, group_interior_state, parse_nodes, parse_group, with_frame, implementation_error. `NodesParser`/`GroupParser` — new, with_child_states. `GroupArgumentParser` — new, with_rule, any_of, with_expression_fallback. `OptionalGroupArgumentParser` — new, any_of, with_unwrap_lone_group. `CharsGroupArgumentParser` — new, with_comments, with_nested_groups, with_restricted_descent. `VerbatimArgumentParser` — new, with_delimiters, with_auto_delimiters. `TackOnFieldsArgumentParser` — new, with_field, with_repeatable_field. `EnvironmentBodyParser` — new, with_match_invocation_name, with_invocation_name_span. `VerbatimBodyParser` — new, with_gobble_leading_newline, with_invocation_name_span. `StopSpec` — none, at_token. `ChildStateSpec` — inherit.

## Module `engine` (S1) — 9 items

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `Language` | struct | Y | Y | `<L>`, pubf=0 | T1 | main entry (`Language::<Latexlike>::default()`, `parse()`) |
| `ParseResult` | struct | Y | Y | `<L>`, pubf=2 | T1 | pub `tree` + `diagnostics`; self-contained |
| `ParserSession` | struct | Y | Y | `<L>`, pubf=3 | T3 | pub `builder`/`diagnostics`/`ext` fields |
| `ParseDriver` | trait | Y | Y | `<L>`, 11 members (all defaulted) | T3 | behavior-placement seam |
| `StdParseDriver` | struct | Y | Y | pubf=1 | T3 | pub `recovery` field |
| `CommandResolution` | enum | Y | Y | `<L>`, 3v, NE | T3 | |
| `ResolvedCallable` | struct | Y | Y | `<L>`, pubf=2 | T3 | |
| `Frame` | struct | Y | Y | `<L>`, pubf=2 | T3 | live frame stack entry; distinct from error's `TraceFrame` |
| `FrameTitle` | enum | Y | Y | `<L>`, 3v | T3 | |

Method surfaces: `Language` — new, with_seed_delta, with_provider, with_resolver, initial_state, driver, resolver, resolve_source, parse, parse_source. `ParserSession` — new, snapshot_frames, derived_state, group_interior_state, recover, finish. `ParseDriver` (trait) — recovery\*, recover\*, probe_token\*, resolve_command\*, make_paragraph_break_node\*, refine_diagnostic\*, observe_transition\*, group_interior_delta\*, make_nodes_parser\*, make_group_parser\*, make_invocation_parser\*. `CommandResolution` — resolve_via_scopes.

## Module `latexlike` (S2 preset) — 23 items (deliberately none at root)

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `Latexlike` | struct | n | Y | unit (ZST) | T1, T2 | the preset `Lang` |
| `LatexlikeDriver` | struct | n | Y | pubf=2 | T1, T2 | `new(Recovery)`; T1's second required name |
| `default_token_rules` | fn | n | Y | | T2, T3 | canonical seed data |
| `base_package` | fn | n | Y | | T2 | seeds `\begin`/`\end` |
| `argument_specs` | fn | n | Y | `<I>` | T2 | `["o", "{"]` codes |
| `argument_specs_from_str` | fn | n | Y | | T2 | compact whole-spec strings |
| `ArgumentCodeError` | enum | n | Y | 4v, NE | T2 | error of the two fns above |
| `MacroSpec` | struct | n | Y | pubf=1 | T2 | |
| `EnvironmentSpec` | struct | n | Y | pubf=0 | T2 | |
| `SpecialsSpec` | struct | n | Y | pubf=1 | T2 | |
| `EnvironmentBehavior` | trait | n | Y | 3 members (all defaulted) | T2 | body behavior hook |
| `VerbatimBehavior` | struct | n | Y | pubf=0 | T2 | `verbatim`-style bodies |
| `BeginSpec` | struct | n | Y | unit | T2 | rarely constructed directly (seeded in base_package) |
| `EndSpec` | struct | n | Y | unit | T2 | rarely constructed directly |
| `EnvironmentInvocation` | struct | n | Y | `<'p>`, pubf=3, NE | T2 | passed to `EnvironmentBehavior` |
| `CallableType` | enum | n | Y | 3v, NE | T1, T2 | Macro/Environment/Specials |
| `GroupType` | enum | n | Y | 3v, NE | T1, T2 | |
| `Mode` | enum | n | Y | 2v, NE | T1, T2 | Text/Math |
| `MathStyle` | enum | n | Y | 2v | T1 | via `NodeRef::math_style` sugar |
| `ParagraphBreakStyle` | enum | n | Y | 2v, NE | T1 | driver config |
| `MalformedBegin` | struct | n | Y | unit, NE | T3 | cond (T1 downcast) |
| `OrphanEnd` | struct | n | Y | pubf=1, NE | T3 | cond (T1 downcast) |
| `UnknownEnvironment` | struct | n | Y | pubf=1, NE | T3 | cond (T1 downcast) |

Method surfaces: `EnvironmentBehavior` (trait) — arguments\*, body_state_delta\*, make_body_parser\*. `EnvironmentSpec` — new, from_behavior, with_body_delta, behavior. `LatexlikeDriver` — new, with_paragraph_break_style. `MacroSpec`/`SpecialsSpec`/`VerbatimBehavior` — new.

## Crate root extras

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `VERSION` | const | Y (root-only) | Y | `&str` | T1 | `CARGO_PKG_VERSION` |
| `__private` | module | Y | Y | `#[doc(hidden)]` | none | derive support only; excluded from counts |
| `guide` | module | Y | Y | `#[cfg(doc)]` | T1 | narrative docs; doc-only, not part of compiled API |

## Crate `techy-derive` — 2 items

| Item | Kind | Root? | Docs? | Flags | Tier (prov.) | Notes |
|---|---|---|---|---|---|---|
| `DiagnosticInfo` | derive macro | — | Y | | T2, T3 | re-exported as `techy::error::DiagnosticInfo` (and root) |
| `ToDiagnosticValue` | derive macro | — | Y | | T2, T3 | re-exported as `techy::error::ToDiagnosticValue` (and root) |


