# RESTRUCTURE_MAP — temporary scaffolding for the documentation restructure

Deleted at the end of the migration (content preserved in the migration commit
messages). Working tables for the old-reference → label conversion. Label names
become **immutable** once approved — review Table B especially.

## Citation-format conventions (used everywhere after Stage 0)

- Developer docs, Rust `//` comments, Cargo.toml comments: bare bracketed label,
  e.g. `cf. [§dd-dr:panic-policy]`, optionally followed by the quoted entry title
  for readability. Never a file path or a section number — the `dd-arch:`/`dd-dr:`
  prefix identifies the document, and labels survive moves and reorganization.
- Cite the most specific label that matches: entry label when the cite targets one
  decision, topic label otherwise.
- Old `DESIGN_RATIONALE.md §3.8` cites about the panic/`Err` contract →
  `[§dd-dr:panic-policy]`; other §3.8 cites → `[§dd-dr:errors]`.
- User-facing rustdoc text (`///`, `//!`, guide pages): NO dev-doc references,
  except user-approved D-cases in the verbose form
  `(cf. developer docs "ARCHITECTURE", <Section title>, §dd-arch:<name>)`.

## Table A — DESIGN_RATIONALE structure: old → new

| Old | New (heading + label) |
|---|---|
| §0 How to use and maintain | `# How to use and maintain this document [§dd-dr:self-meta]` (absorbs §7 template) |
| §2 Meta-principles | `# Implementation design principles [§dd-dr:impl-design-principles]` |
| §1 Project-level goals | `## Project-level goals and constraints [§dd-dr:goals]` (under impl-design-principles) |
| §2.1 | `## Data where values change at runtime; traits where behavior changes [§dd-dr:data-vs-traits]` |
| §2.2 | `## One generic parameter, defaults everywhere [§dd-dr:one-generic-param]` |
| §2.3 | `## No privileged language concepts in the core [§dd-dr:no-privileged-concepts]` |
| §2.4 | `## Closed structural core, open payloads [§dd-dr:closed-core-open-payloads]` |
| §2.5 | `## Zero-copy by default [§dd-dr:zero-copy]` |
| §2.6 | `## Deterministic dispatch over registry scanning [§dd-dr:deterministic-dispatch]` |
| §5 Non-goals | `## Non-goals [§dd-dr:non-goals]` (under impl-design-principles) |
| §3 Decision register | `# Decision register [§dd-dr:decisions]` |
| §3.1 Sources and spans | `## Sources and spans [§dd-dr:sources-and-spans]` |
| §3.2 Tokens and tokenization | `## Tokens and tokenization [§dd-dr:tokens]` |
| §3.3 Parsing state and deltas | `## Parsing state and deltas [§dd-dr:parsing-state]` |
| §3.4 Specs and libraries | `## Specs and scopes [§dd-dr:specs]` |
| §3.5 Nodes and AST | `## Nodes and the syntax tree [§dd-dr:nodes]` |
| §3.6 Construct parsers, dispatch, engine | `## Construct parsers, dispatch, engine [§dd-dr:parsers-engine]` |
| §3.7 Generics strategy | `## Generics strategy [§dd-dr:generics]` |
| §3.8 Errors and diagnostics | `## Errors and diagnostics [§dd-dr:errors]` |
| §3.9 Dependencies | `## Dependencies [§dd-dr:dependencies]` |
| §3.10 Naming | `## Naming [§dd-dr:naming]` |
| §3.11 Crate organization | `## Crate organization and dependency model [§dd-dr:crates]` |
| §3.12 Documentation | `## Documentation [§dd-dr:documentation]` |
| §3.13 The latexlike preset | `## The latexlike preset [§dd-dr:latexlike]` |
| §4 Rejected patterns | `## Rejected patterns — do not reintroduce [§dd-dr:rejected-patterns]` (register tail) |
| §6 Open questions | `## Open questions [§dd-dr:open-questions]` (register tail) |
| §7 Entry template | folded into `[§dd-dr:self-meta]` |

## Table B — decision entries → proposed entry labels (by topic, with source line in old file)

Entries become `#### <Title> [§dd-dr:<label>]`. Merge candidates are curation
proposals (Stage 2c) — they still get labels now; a retired label's citers are
retargeted at merge time.

### sources-and-spans
- L135 `arc-source-ownership` — "Arc-based source ownership" (DECIDED)
- L147 `provenance-on-source` — "Provenance lives on `Source`, not on every location" (PROPOSED)
- L158 `source-node-registry` — "Source→triggering-node mapping … session-owned registry" (general direction)
- L167 `lazy-line-col` — "Line/column is a lazy, standalone utility" (DECIDED)
- L173 `source-resolver` — "Pluggable content resolution" (DECIDED)
- L180 `origin-genericity` — "Origin genericity without `Lang`" (DECIDED)
- L181 `origin-url-simplification` — "default origin simplified to an optional URL string" (REVISED; merge candidate → `origin-genericity`, reversal note)
- L207 `source-content-boundary` — "`SourceContent` is a trait boundary, not (yet) a `Source` parameter" (DECIDED)
- L215 `source-cursor-retired` — "`SourceCursor`, `Source::cursor()`, and `SourceContent` retired" (DECIDED; reversal-flavored vs L207)
- L237 `span-extend-to` — "`Span` has private fields; … monotone `extend_to`" (DECIDED)

### tokens
- L290 `minimal-tokens` — "Tokens are minimal and structural" (DECIDED)
- L302 `token-model` — "The token-design review: final token model" (DECIDED)
- L435 `zero-copy-tokens` — "Zero-copy tokens with ephemeral lifetime" (DECIDED)
- L443 `token-reader` — "`TokenReader` is the behavior extension point" (DECIDED)
- L458 `expecting-group-close` — "Ambiguous group delimiters resolved by data" (DECIDED)
- L514 `command-escape-char` — "`TokenKind::Command` records its escape character" (DECIDED)
- L523 `token-contract-hardening` — "Token-layer contract hardening (Action 02)" (DECIDED)
- L587 `token-list-reader-demoted` — "`TokenListReader` demoted to internal test infrastructure" (DECIDED)
- L599 `multi-newline-paragraphs` — "`TokenRules::multi_newline_paragraphs` (renamed …)" (DECIDED)

### parsing-state
- L641 `token-rules-data` — "Tokenization config is plain data (`TokenRules`), not per-facet traits" (PROPOSED)
- L657 `state-ext` — "Language-specific state is a typed extension (`L::StateExt`)" (PROPOSED)
- L662 `immutable-state-deltas` — "Immutable state, explicit deltas, Arc-shared snapshots" (DECIDED)
- L672 `state-option-c` — "Settings are stored data; dependent settings recomputed at transitions (Option C)" (DECIDED)
- L712 `lang-token-hooks` — "Token-level language hooks live on `Lang`" (DECIDED)
- L756 `first-class-mode` — "Parsing mode is first-class state data: `StateData.mode`" (DECIDED)

### specs
- L794 `unified-callable-spec` — "Unified `CallableSpec` with self-supplied invocation parser" (PROPOSED)
- L803 `lexical-shadowing` — "Library stack with lexical shadowing; no `ConflictStrategy`" (DECIDED)
- L810 `callable-query` — "`SpecLookup` receives a `CallableQuery`" (DECIDED)
- L853 `spec-structure-staging` — "Phase 4 ships structure-spec skeletons …" (DECIDED; merge candidate → `unified-callable-spec`)
- L917 `closed-type-ids` — "`CallableTypeId` and `GroupTypeId` are closed per-`Lang` associated types" (DECIDED)
- L942 `spec-thread-safety` — "Thread safety is a core contract: `Send + Sync` supertraits" (DECIDED)
- L981 `spec-downcasting` — "`CallableSpec: Any` — downcasting …; `Lang: 'static`" (DECIDED)

### nodes
- L1159 `flat-node-tree` — "Flat `NodeTree` … frozen after parse, `NodeRef` proxy access" (DECIDED)
- L1182 `no-core-math-node` — "No core `MathNode`" (DECIDED)
- L1188 `region-nodes` — "Args/slots ↔ children encoding: one node per region" (DECIDED)
- L1262 `slot-ext` — "`SlotExt` — slot records carry per-instance ext" (DECIDED)
- L1271 `iter-storage-order` — "`NodeTree::iter` renamed …; no `parent` stored" (DECIDED)
- L1359 `group-delimiters` — "Group nodes store their delimiters" (DECIDED)
- L1383 `mandatory-node-spans` — "Node spans stay mandatory; synthetic-node representation deferred" (DECIDED)
- L1393 `staging-builder` — "Staging builder with breadth-first flatten" (DECIDED)
- L1405 `text-content-s0` — "`TextContent` is S0 …; no `PartialEq` on node types yet" (DECIDED)
- L1414 `comment-delimiters` — "`Comment` nodes store their start delimiter and post-space" (DECIDED)
- L1453 `span-invariants` — "Whitespace and span invariants pinned" (DECIDED)
- L1491 `node-id-provenance` — "Cross-tree `NodeId` misuse: debug-only provenance tags" (DECIDED)
- L1602 `node-summary` — "`NodeRef::summary()`: the compact node description is core API" (DECIDED)

### parsers-engine
- L1617 `parse-context` — "Single-context parsing API (`ParseContext`)" (PROPOSED)
- L1632 `token-kind-dispatch` — "Dispatch by token kind + library lookup" (PROPOSED)
- L1636 `stateless-language` — "`Language<L>` owns no per-parse state" (DECIDED)
- L1654 `invocation-parser-factory` — "`CallableSpec::make_invocation_parser` — a factory …" (DECIDED)
- L1723 `resolve-command-hook` — "`Lang::resolve_command` hook" (DECIDED)
- L1785 `paragraph-break-hook` — "`Lang::make_paragraph_break_node` hook" (DECIDED)
- L1921 `terminator-mismatch-recovery` — "Terminator mismatch recovery: close without consuming" (DECIDED)
- L1943 `parser-session-root` — "No `Language<L>` type in Phase 6; `ParserSession` is the root object" (DECIDED; merge candidate → `language-parse-api`, reversal-flavored evolution)
- L1952 `child-state-spec` — "`ChildStateSpec`: per-use descent-state policy" (DECIDED)
- L2004 `state-memoization` — "Group interior states are memoized in the session" (DECIDED)
- L2043 `session-derivation` — "Session-mediated derivation …; transitions have two levels" (DECIDED)
- L2178 `temporary-group-rules` — "temporary group rules scoped in state data" (amendment-style note; merge candidate into its surrounding entry)
- L2457 `parity-parsers` — "The deferred parity parsers N2/N3/N4/N6 landed" (DECIDED)
- L2612 `language-parse-api` — "`Language<L>` + `parse()`: the runtime bundle's landed surface" (DECIDED)
- L2702 `with-provider` — "`Language::with_provider`: push-a-provider seed sugar" (DECIDED)

### generics
- L2718 `defer-rc-arc` — "Defer `Rc`/`Arc` genericity" (DECIDED)
- L2726 `lang-genericity-scope` — "What is generic (via `Lang`) and what is not" (PROPOSED)

### errors
- L2734 `panic-policy` — "Panic policy: `Result` everywhere; panics only for verifiably unreachable invariants" (DECIDED) ← pre-promoted in Stage 0
- L2791 `arc-error-spans` — "Errors carry Arc-based `SourceSpan`, not `'src` lifetimes" (PROPOSED)
- L2796 `tolerant-parsing` — "Tolerant parsing via recovery tokens + diagnostics sink" (PROPOSED)
- L2805 `recovery-staging` — "Recovery mechanism split across phases" (DECIDED; merge candidate → `tolerant-parsing`)
- L2824 `err-means-abort` — "Detection-site recovery; `Err` means abort" (DECIDED)
- L2871 `structured-diagnostics` — "Structured diagnostics: condition payloads, not prose" (DECIDED)
- L2904 `diagnostic-info-data-split` — "`DiagnosticInfo` (implementor) / `DiagnosticData` (dyn facade) split" (DECIDED)
- L2922 `diagnostic-derive` — "Condition-declaration derive …, syn accepted" (DECIDED)
- L2945 `condition-identities` — "Two identities: the type in-process, an explicit string on the wire" (DECIDED)
- L2969 `serialized-schema` — "Serialization is a derived projection; the struct is the schema" (DECIDED)
- L2983 `parse-traceback` — "Parse traceback: an explicit frame stack on `ParserSession`" (DECIDED)
- L3013 `refine-diagnostic-hook` — "`Lang::refine_diagnostic` hook" (DECIDED)
- L3029 `token-diagnostics` — "Token layer joins the same model" (DECIDED)

### dependencies
- L3063 `minimal-dependencies` — "Absolute minimal mandatory dependencies" (DECIDED)
- L3084 `no-std` — "`no_std`-friendly, alloc-only" (DECIDED)
- L3094 `map-containers` — "Map containers after hashbrown (`BTreeMap` vs `HashMap`)" (DECIDED in part)

### naming (topic currently has NO bold entries — created during curation)
- (new) `parsed-arguments-naming` — the ParsedArguments reversal (from NAMING_STRATEGY principles 3/4; dated reversal note)
- (new) `superseded-names` — do-not-reintroduce name list (distilled from NAMING_STRATEGY's superseded-names table)

### crates
- L3148 `three-strata` — "Three strata + three rules replace the strict L0–L7 layer ladder" (DECIDED)
- L3200 `workspace-layout` — "Repo layout: virtual workspace, every crate in its own subfolder" (DECIDED)

### documentation
- L3227 `rustdoc-guides` — "Narrative docs included with rustdoc, not a separate site" (DECIDED)
- (new) `docs-restructure` — this restructure decision (added in Stage 2c)

### latexlike
- L3244 `group-taxonomy` — "The preset's group taxonomy is two classes: `Content` and `Math`" (DECIDED)
- L3285 `preset-vocabulary` — "Preset vocabulary names are bare and module-scoped" (DECIDED)
- L3299 `base-package` — "The seed ships a `\"base\"` package" (DECIDED)
- L3336 `ascii-whitespace` — "Default whitespace is the ASCII set, not Unicode-aware" (DECIDED)
- L3349 `inherent-preset-sugar` — "`NodeRef` preset sugar is inherent, not an extension trait" (DECIDED)
- L3359 `begin-end-dispatch` — "`\begin`/`\end` dispatch is scope-stack data" (DECIDED)
- L3396 `concrete-spec-types` — "`MacroSpec`/`SpecialsSpec` are real types, not constructor functions" (DECIDED)
- L3406 `orphan-end-recovery` — "Orphan-`\end` recovery: dispatch-time diagnosis" (DECIDED)
- L3422 `verbatim-family` — "The verbatim family: recipe → production parsers" (DECIDED)
- L3465 `environment-body-content` — "`EnvironmentBody.content`: the body parser designates the slot's content" (DECIDED)
- L3482 `argument-specs-factory` — "The argument-code factory: `latexlike::argument_specs`" (DECIDED)

Excluded from Table B (not entries): L646 (emphasis line inside `token-rules-data`),
L3732 (the entry template).

## Table C — ARCHITECTURE old sections → dispositions

| Old section | Disposition |
|---|---|
| Header status block ("Status: PROPOSAL …") | delete |
| §1 Assessment of where things stand | delete (self-declared outdated) |
| §2 Design principles | rewrite → `[§dd-arch:lib-design-principles]` |
| §3 intro (three strata, three rules) | rewrite → `[§dd-arch:arch]` |
| §3 `### source (S0)` | rewrite → `[§dd-arch:source]` |
| §3 `### token (S1)` | rewrite → `[§dd-arch:token]` |
| §3 `### state (S1)` | rewrite → `[§dd-arch:state]` |
| §3 `### specs (S1)` | rewrite → `[§dd-arch:specs]` |
| §3 `### nodes (S1)` | rewrite → `[§dd-arch:nodes]` |
| §3 `### constructs (S1)` | rewrite → `[§dd-arch:constructs]` |
| §3 `### engine (S1)` | rewrite → `[§dd-arch:engine]` |
| §3 `### Errors and tolerant parsing` | rewrite → `[§dd-arch:errors]` (fixes the dangling `§errors` anchor) |
| §4 (Decision-1 resolution narrative) | fold → `[§dd-dr:state-option-c]` · then delete |
| §4b (Decision-3 resolution narrative) | fold → nodes entries (`flat-node-tree`, `no-core-math-node`, `region-nodes`; evolution as reversal note) · delete |
| §5 Generics strategy | rewrite → `[§dd-arch:generics]` |
| §6 FLM fit check | fold unique arguments → `[§dd-dr:goals]` + relevant entries · delete |
| §7 Naming (deltas) | rewrite → `[§dd-arch:naming]` (+ NAMING_STRATEGY principles + terminology stack) |
| §8 latexlike preset | rewrite → `[§dd-arch:latexlike]` |
| §9 Implementation phases | delete (Phase-8 FLM-spike idea → TODO_Big.md or `[§dd-dr:open-questions]`) |
| §10 Documentation hygiene | superseded by Documentation_Structure.md · delete |
| §11 Collected decision points | fold per Table D · delete |

Old-anchor conversion for citers: `§source/§token/§state/§specs/§nodes/§constructs/§engine/§errors`
→ same-stem `[§dd-arch:…]`; `§3` → `[§dd-arch:arch]`; `§2 principle N` →
`[§dd-arch:lib-design-principles]`; `§9` → reword or drop (historical); `§4b` → the
folded nodes entries.

## Table D — "Decision N" axis → new homes

| Old | New home |
|---|---|
| Decision 1 (state Option C) | `[§dd-dr:state-option-c]` |
| Decision 2 (Lang/Language naming) | `[§dd-dr:naming]` topic |
| Decision 3 (unified Callable + ext + TextContent) | nodes topic — primary `[§dd-dr:flat-node-tree]` + siblings |
| Decision 4 (defer Rc/Arc) | `[§dd-dr:defer-rc-arc]` |
| Decision 5 (zero mandatory dependencies) | `[§dd-dr:minimal-dependencies]` |
| Decision 6 (no ConflictStrategy) | `[§dd-dr:lexical-shadowing]` |
| Decision 7 (rebuild phase-by-phase) | drop (process history; at most one line in `[§dd-dr:crates]`) |
| Decision 8 (three strata) | `[§dd-dr:three-strata]` |
| Decision 9 (ParseDriver + mode + scope stack) | `[§dd-dr:first-class-mode]` + `[§dd-dr:language-parse-api]` + specs entries |
