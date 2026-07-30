# CORE_SPLIT_OPTIONS — splitting the `techy::core` machinery namespace

Written 2026-07-29, follow-up to NAMESPACE_OPTIONS.md after the user's P1 partial
rulings. Inputs: NAMESPACE_OPTIONS.md (facts F-a..F-h, §3.2 freeze analysis, Appendix
A), INVENTORY.md (item roster), SYNTHESIS.md (persona data — used for the coherence
check only, never for placement), FRAMEWORK-ANALYSIS.md (the planned P4 surfaces),
ARCHITECTURE.md ([§dd-arch:naming], [§dd-arch:arch], [§dd-arch:engine]),
DESIGN_RATIONALE ([§dd-dr:superseded-names], [§dd-dr:three-strata]), the ten module
doc headers in techy/src. **Everything here is a proposal for the user's decision;
recommendations are labeled as such.**

## Binding frame (the user's rulings)

1. **Exactly one canonical path per public item.** No dual spellings, no curated-root
   re-exports, no aliases. NAMESPACE_OPTIONS R1/O3 (tiered root + superset core) is
   REJECTED.
2. Paths must **sound obvious from a clear logic of function/use** — not frequency of
   use, not internal implementation layout.
3. The machinery facade is named **`core`** (chosen over `parsing`).
4. Question under evaluation: split `core` into **2–4 function-based parts** (the
   user's sketch: lang / parsers / definitions / node), vs the flat fallback.
5. Internal modules go private regardless; every public namespace is a **re-export
   facade** (P2 taxonomy: internal file moves are invisible; moving an item between
   *public* namespaces is breaking; renames out of scope here).

Consequence of ruling 1 worth stating up front: with no root curation layer, **every
placement decision is final and exclusive** — an item's group is its only spelling
forever. The stakes of "sounds obvious" are therefore maximal, and so is the cost of a
coin-flip assignment. Conversely, curation-quality worries disappear: there is no
curated list to get wrong, only the taxonomy.

---

## 0. Groundwork

### 0.1 The roster: 180 S0/S1 items in 20 blocks

Every candidate below assigns these blocks (plus listed item-level exceptions).
Together they cover the full S0/S1 surface — 179 items + `VERSION` = 180. latexlike
(23 items) is out of scope and unchanged under every candidate.

| Block | n | Items |
|---|---|---|
| **SRC** | 14 | `Source`, `Span`, `SourceSpan`, `SourceProvenance`, `ProvenanceChain`, `SourceOrigin`, `SourceResolver`, `NoResolver`, `MapResolver`, `ResolveError`, `ResolvedContent`, `resolve_source`, `TextContent`, `LineIndex` |
| **DIAG-C** (consuming) | 8 | `Diagnostic`, `Diagnostics`, `Severity`, `ParseError`, `TraceFrame`, `Recovery`, `format_position`, `format_traceback` |
| **DIAG-D** (defining) | 6 | `DiagnosticData`, `DiagnosticInfo` (trait), `DiagnosticInfo` (derive), `ToDiagnosticValue` (trait), `ToDiagnosticValue` (derive), `DiagnosticValue` |
| **COND-TOK** | 2 | `EndOfStreamAfterEscape`, `ForbiddenChar` |
| **COND-SCOPE** | 1 | `CallableDefinedAsError` |
| **COND-CON** | 19 | `UnresolvableCommand`, `CommandResolutionFailed`, `UnclosedGroup`, `UnclosedGroupFound`, `StrayGroupClose`, `MissingMandatoryArgument`, `ExpectedExpressionArgument`, `ExpressionCallableRequiresContent`, `MissingEnvironmentTerminator`, `MissingTerminatorFound`, `EnvironmentTerminatorMismatch`, `MalformedEnvironmentTerminator`, `ScopeOpFailed`, `UnusableRecoveryToken`, `UnusableRecoveryTokenKind`, `ImplementationError`, `ExpectedVerbatimDelimiter`, `UnterminatedVerbatim`, `RepeatedTackOnField` |
| **TOK-DATA** | 5 | `TokenRules`, `CommandRule`, `CommentRule`, `GroupRule`, `WhitespaceRules` |
| **TOK-RT** | 13 | `Token`, `TokenKind`, `TokenReader`, `StdTokenReader`, `SpecialsMatch`, `TriggerChars`, `PrefixTable`, `PrefixEntry`, `TokenError`, `TokenErrorKind`, `TokenRecovery`, `TokenResult`, `skip_whitespace` |
| **STATE** | 9 | `Lang`, `SimpleLang`, `ClosedVocabulary`, `NodeExtTypes`, `ParsingState`, `ParsingStateDelta`, `StateData`, `TokenRulesOverrides`, `DeriveError` |
| **SPEC** | 6 | `CallableSpec`, `StdCallableSpec`, `ArgumentSpec`, `ArgumentParser`, `ParsedArgumentNodes`, `FrameRole` |
| **SCOPE** | 15 | `SpecsProvider`, `Package`, `Scope`, `ScopeStack`, `FallbackProvider`, `ErrorCallableSpec`, `CallableQuery`, `CallableSyntax`, `SymbolEntry`, `SearchedProviders`, `DefinitionOp`, `ScopeOp`, `ScopeOpError`, `ScopeStackError`, `ProviderError` |
| **NODE-READ** | 14 | `NodeTree`, `NodeRef`, `NodeKind`, `NodeData`, `NodeId`, `NodeSlice`, `NodeSliceIter`, `Descendants`, `GroupData`, `CallableData`, `ParsedArguments`, `ParsedArgument`, `ParsedSlots`, `ParsedSlot` |
| **NODE-BUILD** | 8 | `NodeTreeBuilder`, `StagedNodes`, `StagedNodeView`, `BuildId`, `NodeBuildError`, `ContentNodes`, `ChildRegion`, `check_tree_invariants` |
| **NODE-EXT** | 8 | `NodeExt`, `CharsNodeExt`, `GroupNodeExt`, `CallableNodeExt`, `CommentNodeExt`, `ListNodeExt`, `ArgumentExt`, `SlotExt` |
| **EXTRACT** | 9 | `content_as_chars`, `parse_keyval`, `split_at_chars`, `split_embellishments`, `split_tack_on_fields`, `KeyVals`, `KeyValEntry`, `Split`, `ExtractError` |
| **CON-DISP** | 14 | `ConstructParser`, `ConstructParserResult`, `ParseContext`, `Invocation`, `NodesParser`, `NodesOutcome`, `GroupParser`, `StopSpec`, `StopCause`, `TokenStopCondition`, `TokenStopKind`, `ChildStateSpec`, `GroupChildState`, `InvocationChildState` |
| **CON-STD** | 19 | `GroupArgumentParser`, `OptionalGroupArgumentParser`, `CharsGroupArgumentParser`, `MarkerArgumentParser`, `VerbatimArgumentParser`, `EmbellishmentsArgumentParser`, `TackOnFieldsArgumentParser`, `ExpressionParser`, `StdInvocationParser`, `EnvironmentBodyParser`, `EnvironmentBody`, `VerbatimBodyParser`, `NameGroup`, `ArgumentNoise`, `parse_declared_arguments`, `read_rigid_name_group`, `scan_argument_noise`, `stage_pre_space`, `verbatim_state_delta` |
| **ENG-ENTRY** | 2 | `Language`, `ParseResult` |
| **ENG-RT** | 3 | `ParserSession`, `Frame`, `FrameTitle` |
| **ENG-DRV** | 4 | `ParseDriver`, `StdParseDriver`, `CommandResolution`, `ResolvedCallable` |
| root | 1 | `VERSION` (stays at the crate root under every candidate — conventional, uncontested) |

### 0.2 The seven straddle families (what decides every ranking below)

[§dd-dr:three-strata] is the frame: S1 is **one mutually-recursive stratum by
decision** — "modules are topics for navigation, NOT dependency ranks", and every
cross-topic cycle edge is itself a decided feature. A public split of `core` can
therefore only ever be a *navigation taxonomy*; any split that claims to be a pipeline
or a layer ladder re-litigates the settled rejection of the L0–L7 ladder. The items
that resist assignment are precisely the decided cycle edges. Seven families account
for essentially every straggler in every candidate:

1. **The 22 conditions** (COND-TOK + COND-SCOPE + COND-CON). Produced by three
   different topics (tokenizer, scopes, construct parsers), consumed through one
   (diagnostics downcasting / `T::IDENTIFIER`, per F9). Producer-side placement
   scatters the family across every group boundary that separates its producers;
   consumer-side placement removes them from their producers' doc pages. There is no
   third option — except a **registry submodule** (§6.2), which makes "being a
   condition" the assignment key and resolves all 22 at once.
2. **The engine nine.** Three functions live in today's 9-item `engine` module:
   *entry/output* (ENG-ENTRY), *per-parse runtime* (ENG-RT), *behavior contract*
   (ENG-DRV). ARCHITECTURE's own prose splits them: "only `Language<L>` is genuinely
   an orchestration type"; `ParseDriver` is "the parse-behavior instance" and is
   literally `Lang::Driver`, an associated type of the language contract. Every
   candidate must either keep the nine together (and name the group after parsing
   runtime) or cut them (and decide where `Language` "obviously" lives).
3. **Token data vs token runtime.** TOK-DATA is definition-side data *stored in
   `StateData`*; TOK-RT is parse-time machinery. But the two halves are the most
   tightly coupled pair in the crate: the 13-field twins `TokenRules` (token) /
   `TokenRulesOverrides` (state), the state-cached `PrefixTable`/`TriggerChars`, and
   the recorded history that the token topic moved wholly into S1 *because of* state
   coupling ([§dd-dr:three-strata] revision note). Any candidate that separates
   TOK-DATA from TOK-RT, or token from state, pays for it in stragglers.
4. **Node read-side vs build-side vs payloads.** SYNTHESIS shows the payload types are
   used on *both* sides (`ParsedArguments`: T3 `empty()` build-side, T2 `get`/`len`
   read-side; `CallableData` constructed by T3 takeover parsers, read by consumers).
   The planned P4 surfaces (FRAMEWORK-ANALYSIS: `TreeTransformer`/subtree copy,
   `BuildId`→`NodeId` correspondence from `finish()`, transform-tree validator) each
   *consume the read side and produce through the build side in one API*. **Killer-flaw
   test: any candidate that puts NODE-READ and NODE-BUILD in different groups has no
   home for the transformer.** Every serious candidate below keeps the node topic
   whole; the test is applied to C2's strict form, which fails it.
5. **The argument model.** `ArgumentSpec` + the `ArgumentParser` trait +
   `ParsedArgumentNodes` are spec-side vocabulary ("an argument *is* a parser" —
   [§dd-dr:argument-parser-model]); the seven standard implementations (CON-STD) are
   parser-library citizens. The fusion is deliberate, so *every* candidate that
   separates a specs group from a parsers group cuts this family; the only choice is
   where (trait with the spec, impls with the parsers, is the least-bad cut everywhere).
6. **Diagnostics defining vs consuming.** DIAG-D (the `DiagnosticInfo` family + both
   derive macros) serves condition *authors* (T2/T3); DIAG-C serves everyone. They are
   one coherent model ([§dd-dr:structured-diagnostics]) and already co-reside in
   `error`; candidates that split them by audience violate ruling 2 (that is tier
   logic, not function logic). All candidates below keep them together.
7. **`Lang` is the hub.** Its associated types reach into every group (`Driver`,
   `NodeExts`, `StateExt`, `SessionExt`, `SourceOrigin`, `Event`); whichever group
   holds `Lang` could claim half the crate by the "it's part of the Lang contract"
   argument. The discipline used below: `Lang` lives with the state family (the
   recorded reason: `finalize_transition` names `StateData`/`ParsingState`, which
   fixed its *source* home — [§dd-dr:three-strata]); the associated-type *bound*
   types live with their own topics (facade ≠ src tree, so `NodeExtTypes` + the 8
   aliases can be publicly re-exported in the node group even though the trait is
   *defined* next to `Lang` — the facade's freedom actively fixes what the src tree
   could not).

### 0.3 Riders available under every candidate

- **R-a: conditions registry submodule** — collect all 22 core conditions in one
  `conditions` submodule (placement per candidate; full argument §6.2). Critically:
  this must be decided **now**. Under one-canonical-path there is no additive retrofit
  — introducing the registry later means *moving* 22 public items (breaking).
- **R-b: `extract` stays a qualified submodule** (NAMESPACE_OPTIONS §3.1: flattening
  it would force renames of `Split`/`KeyVals`; the qualified-reading design is
  settled). Its parent is a per-candidate question (§6.3).
- **R-c: P1 derive paths through `__private`** (NAMESPACE_OPTIONS §1) — unconditional;
  removes techy-derive from every topology consideration. Assumed below.

---

## 1. C0 — baseline: single flat `techy::core` (the user's fallback)

```rust
pub mod core {
    /* all 20 blocks, flat: ~180 names (~170 with R-a) */
    pub mod extract { … }               // R-b
    pub mod conditions { … }            // R-a (recommended even here)
}
pub mod latexlike;
pub const VERSION: &str = …;
```

**Assignment**: every block → `core`, flat; EXTRACT → `core::extract`; with R-a,
COND-* → `core::conditions` (~148 flat names + 2 submodules). `VERSION` at root.

**Straggler audit**: none — the question "which group?" does not exist. This is C0's
structural advantage and it is genuine, not a technicality: under ruling 1 every
assignment is permanent, and C0 is the only candidate that makes zero permanent
assignments inside the machinery.

**How bad is 180-in-one, honestly?**

- *rustdoc*: the module page groups by kind — ~122 structs, 33 enums, 16 traits, 19
  fns, 10 aliases — each list alphabetical. Lookup by name is instant (page Ctrl-F,
  rustdoc search, IDE jump). Enumeration by topic ("what argument parsers exist?",
  "what's the staging surface?") is genuinely poor: alphabetical order interleaves
  `CharsGroupArgumentParser` between `CharsNodeExt` and `ChildRegion`. The crate's
  systematic suffix naming (`…ArgumentParser`, `…Rule`, `…Ext`) partially compensates
  via Ctrl-F, prefix naming does not (`Token*` clusters, `Parsed*` clusters, but
  `GroupData`/`GroupParser`/`GroupRule` are three topics side by side).
- *Ctrl-F / search*: fine — F-a guarantees crate-unique names, so search never
  disambiguates by module. This is the strongest honest defense of C0.
- *Guides carry topic navigation anyway*: F1/F2 are unambiguous — all four personas
  navigated by guide-taught paths, none by browsing module trees; the guide is
  load-bearing *today*, with nine topic modules in place. A split's discoverability
  gain accrues to reference-browsing and autocomplete, not to the learning path.
- *Autocomplete*: `techy::core::` offers ~150–180 completions. Noisy for discovery;
  harmless for recall (3 typed chars narrow it). This is F2's T1 complaint at the
  root, relocated one level down — but no longer *inverted* (it is the machinery
  namespace; T1's taught path is `latexlike` + a handful of core names).
- *Precedent*: `syn` ships ~200 public types essentially flat and is the most-used
  parsing library in the ecosystem; nobody learns `syn` by browsing its item list, and
  nobody fails to use it for lack of submodules. Flat-at-scale is charmless but
  proven.

**Freeze-risk**: zero taxonomy. All four live revisions (stop-conditions move,
spec+scopes merge, node read/build internal split, conditions registry *if R-a is
taken now*) are invisible. P3 additions land as new flat names; every P4 surface
(transformer, parent map, recompose, id correspondence) lands as new flat names —
no home question because there are no homes. Without R-a, the conditions registry
becomes unreachable later (breaking) — **R-a should be taken even under C0**.

**Naming**: no new names at all (5/5). `core` caveats (extern-prelude E0659 paper
cut) as recorded in NAMESPACE_OPTIONS §3.0.

**Wrapper sub-question**: moot — `core` *is* the single wrapper (ruling 3).

**Persona coherence**: no signal — all personas live in one group plus `latexlike`.
Import ergonomics are the best of any candidate: T1 touches 2 namespaces
(`core` + `latexlike`), T3 one. Path length `techy::core::X` is the shortest possible
under ruling 3.

**Verdict**: better than the fear. Its real weaknesses are (a) reference-page
browsability and autocomplete noise, (b) the namespace communicates nothing — the
architecture's topics exist only in prose, and (c) it spends none of the one-time
restructuring window on self-description. Its real strengths: zero stragglers, zero
taxonomy freeze, shortest paths, no new names.

---

## 2. C1 — the user's 4-way sketch: lang / parsers / definitions / node

The sketch, verbatim intent: "lang-related things (token, lang, driver, ...), parsers
library (construct, expression, std-argument-parser, verbatim parser, etc.),
definitions (callable specs, scopes, packages), and node-related things (nodetree)".
Evaluated first exactly as sketched (best-effort completion of what the sketch leaves
open), then §5 presents the repaired version.

**Complete assignment (best-effort completion; sketch-silent placements marked "?")**

| Group | Blocks | n |
|---|---|---|
| `core::lang` | TOK-DATA, TOK-RT, STATE, ENG-DRV ("driver"), COND-TOK?, `Language`? | ~34 |
| `core::parsers` | CON-DISP, CON-STD, COND-CON?, ENG-RT?, `ParseResult`? | ~56 |
| `core::definitions` | SPEC, SCOPE, COND-SCOPE? | ~22 |
| `core::node` | NODE-READ, NODE-BUILD, NODE-EXT, EXTRACT (submodule) | 39 |
| *(unassigned by the sketch)* | SRC, DIAG-C, DIAG-D | 28 |

The sketch's four groups contain **no home for the source model or the diagnostics
model** — S0 is simply absent. Folding them in fails on sight (`Diagnostic` under
`parsers`? `Span` under `lang`? `Diagnostic` under `node`?), so the only serious
completion makes `source` and `error` their own public modules (§6.1) — i.e. the
"4-way split" is really a **six-namespace layout**, and should be evaluated as such.

**Straggler audit — the decisive test.**

- *The 22 conditions scatter three ways*: COND-TOK (2) land in `lang` beside the
  tokenizer, COND-CON (19) in `parsers`, COND-SCOPE (1) in `definitions`. Every
  individual placement is defensible ("beside its producer") and the *family* has no
  home: a T3/T4 user matching `T::IDENTIFIER`s must know which layer produced a
  condition to find its type — exactly the F9 friction, now with public-path
  consequences. Producer-side placement also freezes the producer relationship
  (a condition gaining a second producer, or a parser reorganization, strands the
  path). This is the sketch's largest systemic gap: **22 items whose paths fail
  "sounds obvious"** unless rider R-a is added.
- *The engine nine*: the sketch names only "driver" (→ `lang`). Unassigned:
  `Language`, `ParseResult`, `ParserSession`, `Frame`, `FrameTitle` — plus
  `CommandResolution`/`ResolvedCallable` riding with the driver. Every completion is
  a coin flip: `Language` in `lang` (reads perfectly) leaves `ParseResult` in
  `parsers` (its only plausible other home) — the crate's central signature
  `Language::parse() -> ParseResult` then spans two groups; `ParserSession` in
  `parsers` is fine but `Frame`/`FrameTitle` (live traceback stack) sit ambiguously
  between `parsers` and `error`. Net: **~6 coin-flip items** in the crate's most
  visible module.
- *`Token`+`Lang`+driver grouping under scrutiny* (the task's explicit test): the
  token/state half **survives** — TOK-DATA is stored in `StateData`, the twins
  `TokenRules`/`TokenRulesOverrides` finally co-reside (fixing INVENTORY oddity 10's
  cross-module twin maintenance), `PrefixTable`/`TriggerChars` are state-cached, and
  the S0→S1 token move was *caused* by state coupling. "Token things are part of the
  language mechanics" is a real, teachable logic. The **driver membership is the shaky
  part**: `ParseDriver` is `Lang::Driver` (contract logic → `lang`) but ARCHITECTURE
  calls it "the parse-behavior instance — everything that only runs while a parse is
  driven" (runtime logic → `parsers`). Defensible either way = coin flip by the
  user's own bar. §5 turns this into an explicit sub-decision instead of a silent one.
- *Node payload + staging*: the sketch keeps the node topic whole — **passes the P4
  killer-flaw test**. Transformer, parent map, recompose, id-correspondence all have
  the obvious home `core::node`. This is the sketch's strongest property.
- *`NodeExtTypes` + 8 aliases*: trait defined next to `Lang` (src), aliases in node.
  As-sketched, `NodeExtTypes` → `lang` splits the ext family from its 8 aliases
  (`node`). Fixable at the facade: re-export the whole family in `node` (§0.2 #7).
- *Argument model*: `ArgumentSpec`/`ArgumentParser`/`ParsedArgumentNodes` in
  `definitions`, the seven implementations in `parsers` (family 5's forced cut).
- *Loose items*: `skip_whitespace` → `lang` (token primitive, fine);
  `verbatim_state_delta` → `parsers` (sits with the verbatim parsers, but *produces*
  state vocabulary — mild); `Invocation` → `parsers` (named in `definitions`'
  `make_invocation_parser` signature — mild); `check_tree_invariants` → `node` (fine);
  `resolve_source` → `source` (fine); DIAG-D → `error` (fine, once `error` exists).

Straggler count as sketched: 22 (conditions) + 6 (engine) + 9 (ext family, fixable)
+ ~4 mild (argument-model cut, `Invocation`, `verbatim_state_delta`,
`ParsedArgumentNodes` name-adjacency to node's `ParsedArguments`) ≈ **35–41 of 180
(~78–81% obvious-home rate)**. With the two mechanical fixes any completion should
adopt (R-a registry, ext-family → `node`): **~10 (≈94%)**.

**Freeze-risk.** Stop-conditions move: internal to `parsers` — invisible ✓.
Spec+scopes merge: both inside `definitions` — invisible ✓ (the sketch pre-applies
this revision; good). Node read/build split: internal to `node` — invisible ✓.
Conditions registry: **breaking to retrofit** unless R-a is taken now ✗. P3
(preset generalization: liftable driver core, generic token-rule defaults): homes in
`lang` — obvious ✓. P4: all in `node` — obvious ✓.

**Naming critique** ([§dd-arch:naming], [§dd-dr:superseded-names]):

- **`lang`** — module named after its central trait. Precedent exists and is
  respectable (`std::error::Error`); `core::lang::Lang` stutters but reads. The real
  objection recorded in NAMESPACE_OPTIONS ("too narrow; near `Lang`") targeted `lang`
  as the *whole-facade* name — as a *group* name, narrow is now correct. Two residual
  risks: (i) if `Language` does NOT live in this group, `core::lang` without
  `Language` is a principle-4 sibling trap (the competing vocabulary is its own
  near-namesake); (ii) `language` as an alternative is *worse* (head-on collision
  with `Language<L>` in another group). Verdict: acceptable, contingent on the
  `Language` placement.
- **`parsers`** — does not reintroduce a superseded name (the rejected item was the
  *trait* `Parser`; a plural topic module is a different thing). But it competes with
  the established topic word **`constructs`** (ARCHITECTURE section, `ConstructParser`,
  the module docs' "the parsing layer of the S1 core"). If the group also holds
  `ParserSession`/`ParseResult`, "parsers" (things that parse) misdescribes them —
  `parsing` (the activity/machinery) covers parsers *and* session *and* result.
  Recommendation within this sketch: **`parsing`**.
- **`definitions`** — the serious problem. P2 plans `latexlike::defs` (or
  `::definitions`) as the standard-definitions *database* — actual `\emph`/`itemize`
  definitions. `core::definitions` would then be the *mechanism for defining* while
  `latexlike::defs` is the *definitions themselves*: one word, two referents, in the
  same crate, at the same nesting depth, on the two sides of the crate's central
  boundary. That is the exact situation naming principle 4 exists to prevent (sibling
  vocabulary competing in scope). One of the two must yield, and the database's claim
  to the word is stronger (its contents *are* definitions; the core group's contents
  are specs, providers, and scopes). Recommendation: the core group takes the
  established vocabulary **`specs`** ([§dd-arch:specs] "Specs and scopes";
  `CallableSpec`, `ArgumentSpec`, `SpecsProvider`, `argument_specs` all already speak
  it). `defs` remains free for latexlike.
- **`node`** — established topic word, correct contents, no critique. (Plural
  `nodes` would be equally defensible; changing an established name gratuitously
  fails principle "no churn without cause".)

**Wrapper sub-question**: with S0 forced to the top level, the layout is
`techy::{source, error, core::{lang, parsers, definitions, node}, latexlike}`. The
wrapper's job here is real: it marks the S1 boundary (`core` = exactly what
ARCHITECTURE labels "core"), keeps the root page at five entries, and preserves the
wire alignment (`core.*` identifiers ↔ `core::` paths). Dropping it would put six
sibling modules at the root and make `lang`/`definitions` compete with `latexlike`
at the same level with no marked stratum boundary. Verdict for C1: **keep the
wrapper** (detail in §6.4).

**Persona coherence** (informational): T2 → `definitions` + `latexlike` (excellent);
T3 → `lang` + `parsers` + node-build (good map of their mental model); T4 → `source`
+ `error` (excellent); T1 → spans `error`, `node`, `source`, `parsers`
(`ParseResult`), `definitions` (`Package`) + `latexlike` — i.e. all six namespaces
for hello-world. That spread is a direct consequence of ruling 1 (no curated tier)
and is shared by every split candidate; it is mitigated by guides, not by placement.

**Verdict**: the sketch's instincts are right where it is specific (token+state
together; spec+scopes together; node whole — the one decision that keeps P4 safe) and
wrong where it is silent: no diagnostics/conditions story, no engine story, no S0
story — and those silences sit exactly on straddle families 1, 2, and 6, which is
where ~30 of its ~37 stragglers come from. `definitions` collides with the planned
defs database. C1 is not so much an option as a *specification of the hard cases*;
§5 (C4) is C1 with the three silences decided and the names repaired.

---

## 3. C2 — pipeline 3-way: define → parse → result

The strongest concrete version (chosen to dodge the killer flaw; stage names are
placeholders, naming critique below):

| Group | Blocks | n |
|---|---|---|
| `core::define` | STATE, TOK-DATA, TOK-RT, SPEC, SCOPE, ENG-DRV | ~52 |
| `core::parse` | CON-DISP, CON-STD, ENG-ENTRY, ENG-RT, COND-* (or registry) | ~60 |
| `core::result` | NODE-READ, NODE-BUILD, NODE-EXT, EXTRACT | 39 |
| top level | SRC → `source`, DIAG-C+DIAG-D → `error` (same forced completion as C1) | 28 |

Design notes on this best version: the whole token topic goes to `define` (splitting
TOK-RT out to `parse` would cut straddle family 3 — the reader is configured by the
rules, `GroupOpen` tokens carry `Arc<GroupRule>`); the whole node topic goes to
`result` **including the builder** — the naive pipeline assignment (builder = parse
runtime, tree = result) is exactly the P4 killer flaw: `TreeTransformer` consumes the
read side and produces through `NodeTreeBuilder` in one API, `extract` helpers mint
trees through the builder route, and the payload types are used on both sides
(§0.2 #4). Any C2 variant that follows the pipeline honestly at the node boundary
fails the test outright; this version survives only by *breaking its own logic* and
calling the builder a "result" thing.

**Straggler audit.** The distinctive C2 failure is not scattered items but
**misdescription**: pipeline stage is a property of *use moments*, not of items, and
the crate's central types deliberately live in several stages at once:

- `ParsingState` under `define` is wrong on its face — it is the *parse-time* state,
  evolved during parsing, recorded on every node, read back at result time. Its
  *rules* are define-side; the state itself is all three stages. (`StateData`,
  `ParsingStateDelta` likewise.)
- `ScopeStack` under `define` — it is runtime state mutated by `\newcommand` deltas
  mid-parse; `Package`/`Scope` are define-side. One family, two stages.
- `Language` — assembled at define time, *is* the parse entry. `define::Language` with
  `parse()` on it, or `parse::Language` holding your definitions? Coin flip.
- `NodeTreeBuilder`/`StagedNodes`/`BuildId` under `result` — runs during parsing (see
  above; the survival hack).
- Conditions: produced in `parse`, consumed from `result`'s diagnostics — same family
  1 problem; R-a (a `parse::conditions` registry) is the only clean answer.
- The argument-model cut (family 5) recurs unchanged: trait in `define`, impls in
  `parse`.

Counting only genuinely surprising/coin-flip placements: STATE's runtime members
(~4), `ScopeStack`, ENG-ENTRY (2), NODE-BUILD misdescribed (8), conditions (22
without R-a), argument cut (1), `Invocation`, `verbatim_state_delta` ≈ **40 of 180
(~78%)**; with R-a still ≈ 18 (**90%**) — and the remaining 18 are not fixable,
because they are the two-stage items themselves.

**Freeze-risk.** Same mechanics as C1 for the four live revisions (all internal to
one group ✓ / registry must be decided now). One extra structural risk: pipeline
taxonomies invite pipeline *additions* — the planned include/`\input` wiring (F8) is
define-time (a construct spec) + parse-time (resolver trigger) + result-time
(mixed-origin trees); wherever it lands, the stage story frays further.

**Naming critique.** `define` is a verb (Rust module names are nouns — `fmt`, `io`,
`collections`; there is no `std::compute`); `parse` as a module name collides
conceptually with the method vocabulary `Language::parse`/`parse_source` (legal, but
every sentence about "parse" becomes ambiguous); `result` collides mentally with
`std::result` and with `ParseResult` while containing *trees*, not results —
`NodeTree` is in it, `ParseResult` is not (it is engine output in `parse`). Renaming
the stages to nouns (`specification`/`machinery`/`trees`) fixes grammar but not the
misdescription problem above. **Fails [§dd-arch:naming] worse than any other
candidate.**

**Wrapper**: same forced completion as C1 (S0 at top level, wrapper kept) — nothing
new.

**Persona coherence**: actually the best surface story of all candidates (T2 lives in
`define`, T1 in `result` + `error`, T3 spans `define`+`parse` — the pipeline *is* the
persona sequence). This is worth naming precisely because it is a trap: the pipeline
is the *use story*, and the use story already has a home — the guide, which teaches
in exactly this order. Making it the *namespace* story means every dual-stage item
(state, scopes, builder, entry) permanently sits in the wrong half of its own life.

**Verdict: rejected on principle, not on execution.** [§dd-dr:three-strata] already
adjudicated this: stage/layer assignment of a deliberately mutually-recursive stratum
was tried (the L0–L7 ladder) and dismantled because "the middle layers form a
strongly-connected component **by intention**". A pipeline split is that ladder with
three rungs. Its natural home is the guide's chapter order, not the path structure.

---

## 4. C3 — 2-way coarse split: specify vs run

The compelling 2-way exists, and it is not "defining vs trees" but **"what you give
the engine" vs "what the engine does and gives back"**:

| Group | Blocks | n |
|---|---|---|
| `core::lang` | STATE (−`NodeExtTypes`), TOK-DATA, TOK-RT, SPEC, SCOPE, ENG-DRV, `Language` | ~52 |
| `core::parse` | CON-DISP, CON-STD, ENG-RT, `ParseResult`, NODE-READ, NODE-BUILD, NODE-EXT (+`NodeExtTypes`), EXTRACT, `conditions` registry (R-a) | ~99 incl. submodules |
| top level | `source` (SRC 14), `error` (DIAG-C+DIAG-D 14) | 28 |

(`core::lang` here = "everything that specifies your language": contract, mechanics,
definitions, behavior, assembled bundle. `core::parse` = "everything that happens
when you parse and everything you get out": the parser library, the session, the
trees, the extraction helpers, the conditions.)

**Straggler audit.** Two-way splits have one internal boundary, so family damage is
minimal by construction:

- Family 1 (conditions): all 22 in `parse::conditions` (R-a) — "conditions are things
  a parse reports" covers even the token conditions naturally ✓. Without R-a: 2+1
  land in `lang`, 19 in `parse` — the scatter returns; R-a is load-bearing here too.
- Family 2 (engine): `Language` + driver in `lang` ("your language, ready to use"),
  session/result in `parse` — the `Language::parse() -> ParseResult` signature still
  spans the boundary (1 straggler: `ParseResult`; mild — "the result of parsing" in
  `parse` is arguably *more* obvious than beside `Language`).
- Family 3 (token): whole topic in `lang` ✓. Family 4 (node): whole topic in `parse`
  ✓ P4-safe; every planned transform/navigation surface lands in `parse`. Family 5:
  the argument cut recurs (trait+`ArgumentSpec` in `lang`-side SPEC, impls in
  `parse`) — 1–2 stragglers, same as everywhere. Families 6, 7: resolved as in C4
  (DIAG together in `error`; ext family reunited in `parse`).
- Residual oddities: `Frame`/`FrameTitle` (`parse`) vs `TraceFrame` (`error`) —
  pre-existing; `verbatim_state_delta` in `parse` beside its parsers, producing
  `lang` vocabulary — mild.

Straggler count: **~5 of 180 (≈97%)** — the best rate of any split, bought by having
only one boundary for families to straddle.

**Freeze-risk**: the lowest of any split. One internal boundary; all four live
revisions internal to one side ✓; P3 homes in `lang` ✓; P4 homes in `parse` ✓;
registry baked in ✓. The frozen public names: `source`, `error`, `lang`, `parse` (+2
submodule names). **Important asymmetry: coarseness is permanent.** Under ruling 1
there is no additive path from C3 to a finer split later — subdividing `parse`
publicly would move items (breaking). Choosing C3 is choosing this granularity
forever.

**Naming**: `lang` — as in C1, acceptable-with-stutter, and here `Language` IS in the
group, defusing the principle-4 sibling trap ✓. `parse` — a verb-ish module name;
`parsing` reads better and was pre-cleared in NAMESPACE_OPTIONS as "descriptive, zero
collision" (its facade-level vacuousness objection does not apply to a group that
excludes the specification side). Within C3 the honest name for the second group is
actually hard: it holds parsing *and* the trees; `parsing` underdescribes the node
half (`NodeRef` under `parsing` is a mild surprise for a T1 reader who never parses —
they receive trees). Candidates: `parsing` (activity), `parse` (terse), `output`
(stage word, C2 disease). None is excellent; `parsing` is the least bad. This naming
awkwardness is intrinsic to 2-way coarseness: the group is too big to name crisply —
which is itself evidence about the granularity.
**Wrapper sub-question**: with only two machinery groups, the wrapper is *optional*
for legibility (root would hold `source, error, lang, parsing, latexlike` — five
entries, still clean, and dropping `core` eliminates the extern-prelude E0659 paper
cut entirely because no module is named `core`). But ruling 3 named the facade
`core`, the wire says `core.*`, and consistency with every other candidate says keep
it: `techy::core::{lang, parsing}`. Flagged as a genuine either-way; detail §6.4.

**Persona coherence**: T3 → `lang`+`parsing` (both, heavily — fine, that is their
job); T2 → `lang` (SPEC/SCOPE) + `latexlike` (good); T1 → `parsing` (trees) +
`error` + `latexlike` (good — 3 namespaces, the best T1 story of any split); T4 →
`source`+`error` (+`parsing` for trees) (good).

**Verdict**: the low-risk, low-payoff split. It buys the one distinction with the
strongest functional logic ("specify" vs "run/consume"), nearly eliminates
stragglers, and freezes almost nothing — but `parsing` at ~70 flat names (plus its two submodules) retains
most of C0's reference-page problem at half scale, and the granularity can never be
refined. It is the right choice if the user's doubt about taxonomy freeze outweighs
their discomfort with big flat pages; it is dominated by C4 if the reference pages
are the point of splitting at all.

---

## 5. C4 — the repaired 4-way: `source` | `error` | `core::{lang, specs, parsing, node}`

C1 with its three silences decided, its names repaired, and the conditions registry
baked in. This is the candidate the analysis converges on; **recommended** (§8).

```rust
pub mod source;      // S0 — the source model (14)
pub mod error;       // S0 — the diagnostics model (14)
pub mod core {       // S1 — exactly what ARCHITECTURE labels "core"
    pub mod lang;        // specifying a language: Lang contract, state, tokenization (26)
    pub mod specs;       // defining callables: specs, argument model, providers, scopes (21)
    pub mod parsing;     // running a parse: entry, drivers, sessions, parser library, result (42)
    pub mod node;        // the trees: reading, payloads, building, transforming (39)
                         //   └ node::extract (9)
    pub mod conditions;  // the closed registry of core diagnostic conditions (22)
}
pub mod latexlike;   // S2 — unchanged (23)
pub const VERSION: &str = …;
```

**Complete assignment**

| Namespace | Blocks + exceptions | n |
|---|---|---|
| `source` | SRC | 14 |
| `error` | DIAG-C, DIAG-D (derive macros included — they follow their traits; note top-level `error` keeps today's derive-emitted `::techy::error::…` paths valid even before R-c) | 14 |
| `core::lang` | STATE −`NodeExtTypes`, TOK-DATA, TOK-RT | 26 |
| `core::specs` | SPEC, SCOPE | 21 |
| `core::parsing` | CON-DISP, CON-STD, ENG-ENTRY, ENG-RT, ENG-DRV | 42 |
| `core::node` | NODE-READ, NODE-BUILD, NODE-EXT, +`NodeExtTypes`; `extract` submodule (EXTRACT) | 40 |
| `core::conditions` | COND-TOK, COND-SCOPE, COND-CON | 22 |
| root | `VERSION` | 1 |

Sum: 180. Every group has a one-line description that covers 100% of its contents —
the test C2 and (partially) C3 fail.

**The engine cut — the one genuine sub-decision (user must pick):**

- **Variant A (the user's sketch)**: ENG-DRV + `Language` → `lang`; ENG-RT +
  `ParseResult` → `parsing`. Logic: "lang = your language, including its behavior
  instance and the assembled, ready-to-parse bundle". Costs: `Language::parse()` and
  its result live in different groups; `ParseDriver` (in `lang`) hands out construct
  parsers that live in `parsing`; `CommandResolution`/`ResolvedCallable` sit in
  `lang` while their only use site is the dispatch loop. Residual stragglers: 4
  (`ParseResult`, `CommandResolution`, `ResolvedCallable`, `ParserSession`-vs-driver
  adjacency).
- **Variant B (recommended)**: all nine engine items → `parsing`. Logic —
  ARCHITECTURE's own sentences: "only `Language<L>` is genuinely an orchestration
  type"; `ParseDriver` is "the parse-behavior instance — everything that only runs
  while a parse is driven". `core::parsing` then reads as one story: *entry point,
  drivers, sessions, the construct-parser library, result*. "Where is `Language`?" —
  it is how you parse → `parsing` (the entry point beside its result and its
  drivers). Costs: `core::lang` does not contain `Language` (the principle-4
  near-namesake trap; must be defused by the first line of `lang`'s module docs and a
  doc-link — "the runtime bundle `Language` lives in [`parsing`]"), and `ParseDriver`
  lives apart from the `Lang` trait whose associated type it is. Residual stragglers:
  2 (the `lang`-without-`Language` surprise, `ParseDriver`-vs-`Lang` adjacency).

B is recommended because its groups each tell one true story and because A scatters
the driver's collaborators; but A is the user's sketch and is fully workable — this
is a taste call, not a correctness call.

**Straggler audit (Variant B), the complete honest list:**

1. **`ArgumentParser` (trait) + `ParsedArgumentNodes` in `specs`, the seven std
   implementations in `parsing`** — family 5's forced cut; the worst straggler of
   this candidate. "Where is `VerbatimArgumentParser`?" → parsing ✓, but "where is
   the trait it implements?" → specs — a trait and its impls on two pages.
   (Alternative — everything argument-related into `specs` — was rejected: it drags
   7 parser-library citizens away from `EnvironmentBodyParser`/`ExpressionParser`,
   splitting the *parser library* instead, a worse cut of the same family.)
2. `lang` does not contain `Language` (Variant B's cost, above).
3. `ParseDriver` in `parsing`, apart from `Lang` (`Lang::Driver`) in `lang` —
   the two halves of the language contract on two pages (Variant B's other cost;
   Variant A trades exactly these two for its four).
4. `Frame`/`FrameTitle` (`parsing`) vs `TraceFrame` (`error`) — the two frame
   vocabularies (INVENTORY oddity 7) now in different namespaces. Defensible (live
   stack = runtime; snapshot = diagnostics) but the adjacent names will surprise.
5. `Invocation` in `parsing` while `specs`' `make_invocation_parser` names it in its
   signature — mild.
6. `verbatim_state_delta` in `parsing` (beside the verbatim parsers) while producing
   `lang`-vocabulary (`ParsingStateDelta`) — mild.
7. `skip_whitespace` in `lang` (token primitive) though its callers are parser
   implementors — mild.
8. `Recovery` in `error` though it is the driver's policy knob — established home,
   the diagnostics model owns the tolerance vocabulary; mild.
9. `ParsedArgumentNodes` (`specs`) vs `ParsedArguments`/`ParsedArgument` (`node`) —
   near-identical names in different groups; pre-existing adjacency (spec-side return
   vs node-side record), now with different prefixes to memorize.

Count: **9 items** with any coin-flip/surprise character (of which 4 are mild and 2
are pre-existing oddities the split merely relocates) → **obvious-home rate ≈ 171/180
≈ 95%**; counting only the genuinely contestable (1, 2, 3, 4): ≈ 98%. Everything
else passes the "sounds obvious" test cleanly, including every item the task flagged:
conditions → `conditions` (all 22, one rule); `ParserSession`/`Frame`(s) → `parsing`;
token+state reunited in `lang` with the `TokenRules`/`TokenRulesOverrides` twins
finally co-resident; all payload+staging+ext types → `node` (single home);
`extract` → `node::extract`; `DiagnosticInfo` family + derives → `error`;
`resolve_source` → `source`; `check_tree_invariants` → `node`; `VERSION` → root.

**Freeze-risk.** The four live revisions: stop-conditions move — internal to
`parsing` ✓; spec+scopes merge — *pre-applied* (they are one group) ✓; node
read/build split — internal to `node` ✓; conditions registry — baked in ✓. P3
(preset generalization): a liftable driver core → `parsing`; generic token-rule
defaults → `lang`; a generic spec-type family → `specs` — every plausible addition
has exactly one candidate group ✓. P4: `TreeTransformer`, subtree copy,
`BuildId`→`NodeId` correspondence, `ParentMap`/`parent()`, `recompose`, transform
validator — **all `core::node`**, unambiguously ✓ (recompose could be argued into
`node::extract`; both spellings are inside the same group, so the argument is
intra-group and cosmetic). New freeze taken on: seven public names (`source`,
`error`, `lang`, `specs`, `parsing`, `node`, `conditions`) and the four-way topic
assignment itself. Each name is an established ARCHITECTURE topic word; the
assignment's stress points are exactly the 9 stragglers above, all of which have
been stable vocabulary for the whole review. Residual risk: a future item genuinely
of two groups — no known planned item qualifies.

**Naming compliance.** `source`, `error`, `node`: today's names, zero churn.
`specs`: the [§dd-arch:specs] section title vocabulary; avoids the
`definitions`-vs-`latexlike::defs` collision entirely (C1 critique). `parsing`: the
activity word; pre-vetted (NAMESPACE_OPTIONS: "descriptive, zero collision") — its
facade-level vacuousness objection ("everything in techy is parsing") does not apply
to a group that sits beside `lang`, `specs`, and `node`, which are precisely the
parts of techy that are *not* the parsing machinery. `lang`: acceptable with the
`std::error::Error` stutter precedent; the group genuinely is the `Lang`-and-its-
vocabulary surface (Variant B) — considered and rejected: `language` (collides with
`Language` in `parsing` — actively harmful), `syntax` (wrong scope: `Lang` bundles
driver and exts, more than syntax), `mechanics`/`machinery` (vague, principle 2),
`state`+`token` as separate groups (right names, but 5–6 topic groups exits the
user's 2–4 window and re-approaches O2b's 9-topic freeze). `conditions`: matches the
crate-wide "condition" terminology ([§dd-dr:structured-diagnostics]); under the
`core` wrapper, principle 4 gives it its qualifier (`core::conditions` = the core's
conditions, mirroring `latexlike`'s own three staying in `latexlike`). No superseded
name is reintroduced by any of the seven.

**Wrapper sub-question.** Keep it (**recommended**): the root page becomes five
entries (`source`, `error`, `core`, `latexlike`, `guide` + `VERSION`) that *are* the
architecture diagram — S0 foundation, S1 core, S2 preset; `core` = exactly the
stratum ARCHITECTURE labels "core", so paths, prose, and wire identifiers (`core.*`)
tell one story; and `conditions` gets its principle-4 qualifier. Cost: T3's imports
gain one segment (`use techy::core::parsing::ParseContext;` — nested-use syntax
amortizes it: `use techy::core::{lang::Lang, parsing::{ParseContext, StopSpec}};`),
and the extern-prelude E0659 paper cut stays (unchanged from NAMESPACE_OPTIONS §3.0:
loud, avoidable, worst for the no_std audience). The no-wrapper variant
(`techy::{source, error, lang, specs, parsing, node, conditions, latexlike}`) is
viable and kills the E0659 case, but: eight root siblings with no marked stratum
boundary; bare `techy::conditions` loses its qualifier ("conditions of what?" —
principle 2 violated at root); the wire's `core.*` prefix would name nothing in the
paths; and ruling 3 chose `core`. If the user's no_std sympathies make E0659
intolerable, dropping the wrapper is the coherent way to do it — but then the S0/S1
boundary lives only in docs.

**Persona coherence** (informational). T2 → `specs` + `latexlike` (+`error` when
defining custom conditions): tightly concentrated ✓. T4 → `source` + `error`
(+`node` read side): tightly concentrated ✓. T3 → `lang` + `parsing` + `node`
(build) + `conditions` (identifiers): spread, but the spread *maps their workflow*
(implement Lang → lang; write parsers → parsing; stage nodes → node) ✓. T1 →
`parsing` (`Language`/`ParseResult`) + `error` + `node` + `source` + `specs`
(`Package`) + `latexlike`: **six namespaces for hello-world** — the widest spread of
any persona under any candidate, and the direct price of ruling 1 (no curated tier).
Mitigation is the guide's import blocks, not placement; no assignment change can fix
it without reintroducing dual paths.

---

## 6. Cross-cutting sub-questions

### 6.1 `source` and `error`: top-level modules vs inside `core`

**Under any split candidate: top-level (recommended).** The objections
NAMESPACE_OPTIONS §5 recorded against O4-mod dissolve in the split context: "freezes
two topic names without need" — a split freezes four to seven names anyway, and
source/error are on record as "the two best candidates if any topic names are to be
frozen"; "makes the facade story inconsistent (everything behind core except these
two)" — under a split the facade story *is* topic modules, and S0-at-top-level is
not an exception but the strata made visible: `source`/`error` are the Lang-free
foundation (usable standalone by tooling that never parses), `core` is S1, and the
future crate-split seam ([§dd-dr:three-strata] revisit-if) converts each module to
`pub use techy_source::*;` with zero path breaks. Two bonuses: T4's working set
becomes two short top-level paths; and today's derive-emitted `::techy::error::…`
textual paths remain valid as-is (R-c still recommended, but no longer load-bearing).
The cost that remains (and is intrinsic, not fixable by placement): T1's everyday
vocabulary spans `source`, `error`, and `core::node` — the F4 "3-module scatter"
friction survives every candidate because ruling 1 forbids the curated tier that
would have papered over it.

**Under C0 (pure flat): inside `core`** (O4-core) — a lone pair of topic modules
beside one flat namespace would be the inconsistency the original objection
described. If the user wants source/error visible even under C0, that is a
deliberate 3-namespace layout (`source`, `error`, `core`) — coherent, but it
concedes the topic-module principle and invites "why not the others?".

### 6.2 The conditions registry (`core::conditions`) — decide now, not later

Recommended under **every** candidate, including C0. The arguments, consolidated:
(a) it is the only resolution of straddle family 1 that gives all 22 items one
obvious home under one rule ("it is a diagnostic condition"); (b) it is F9's
identifier↔type registry made manifest in code — the doc page *is* the wire-format
reference, and `use techy::core::conditions as cond;` + `cond::UnclosedGroup::IDENTIFIER`
is the matching idiom both T3 and T4 independently wished for; (c) "condition-ness"
is a stable property (plain data + `Display` + `IDENTIFIER`), so the registry
re-freezes nothing that any live revision wants to move — unlike producer-side
placement, which freezes the producer relationships F9 already flags as
semver-fragile; (d) it removes the largest uniform population from every group page
(the 19-name `Missing*/Unclosed*/Stray*` wall would otherwise dominate `parsing`'s
struct list). Costs, honestly: conditions leave their producers' doc pages (mitigate
with doc-links from each parser's docs — "may report [`UnclosedGroup`]"; rustdoc
makes this cheap); and the registry holds *core's* conditions only — latexlike's
three stay in `latexlike` (consistent: each area namespaces its own conditions,
matching the wire's `core.*`/`latexlike.*` prefixes; a third-party crate's
conditions live in that crate — the registry is an area index, not a global one).
Placement: `core::conditions` under wrapper candidates (principle-4 qualifier);
`parse::conditions`/`parsing::conditions` under C3 ("things a parse reports" —
equally sound). Under src-tree discipline nothing moves: condition structs stay
*defined* beside their detecting constructs ([§dd-dr:structured-diagnostics]); the
registry is a facade page, which is exactly what P2's facade rule makes free.
The one hard rule: **whichever candidate wins, this ships with it** — under
one-canonical-path there is no additive retrofit.

### 6.3 `extract` — a qualified submodule of the node group

Unchanged from NAMESPACE_OPTIONS §3.1 (names designed for qualified reading;
flattening forces `Split`/`KeyVals` renames — reopens settled names). Its parent
under a split: **the node group** (`core::node::extract` in C4, `parsing`'s node
half in C3) — extraction helpers are functions *over node trees*; ruling 2 places
them by that function, not by their T1 audience. The resulting canonical path is
deep (`techy::core::node::extract::content_as_chars`) but the depth is import-side
only: the designed usage is `use techy::core::node::extract;` then
`extract::content_as_chars(…)`. A `core::extract` shortcut placement (sibling of the
groups) would shave one segment at the price of breaking the "groups are topics,
submodules belong to their topic" rule that makes the rest of the layout predictable
— not recommended.

### 6.4 The `core` wrapper — one rule

Consolidating the per-candidate findings: the wrapper earns its keep exactly when
the machinery has **multiple sibling groups** (C1/C4: it marks the S1 stratum, keeps
the root at five entries, gives `conditions` its qualifier, and keeps path ↔ wire ↔
ARCHITECTURE vocabulary aligned). At C3's two groups it is defensible either way; at
C0 it is the namespace itself (ruling 3). Nobody drops it except to buy the
extern-prelude E0659 fix — a real but loud-failure paper cut. **Recommended: keep
the wrapper under every candidate**, consistent with ruling 3.

### 6.5 Migration mechanics (delta to NAMESPACE_OPTIONS Appendix D)

Identical scale to O2a for every candidate here: lib.rs rewritten to facades (the
facade is *larger* by 4–6 `pub mod` blocks but the same ~180 `pub use` lines);
~97 textual import lines updated (F-f) — the split candidates change *which* new
path each line gets, not the count; R-c (derive → `__private`) unchanged; no
signatures, no renames (F-a holds per-group a fortiori). One addition: group-level
module docs (7 short pages under C4) — new prose, half a day. Total remains "a
focused half-day to a day, agent-executable in a worktree".

---

## 7. Scoring and comparison

Criteria per the task: obvious-home rate (weighted highest), freeze-risk against
known+planned changes, naming compliance, path ergonomics, rustdoc quality.
Scores 1–5; weighted sum uses ×3 / ×2 / ×1 / ×1 / ×1 (max 40). All candidates
assume riders R-a/R-b/R-c and (for splits) top-level `source`/`error`.

| Criterion (weight) | C0 flat | C1 sketch | C2 pipeline | C3 two-way | C4 repaired |
|---|---|---|---|---|---|
| Obvious-home rate (×3) | 5\* (no assignments exist) | 2 (~80%; ~94% fixed) | 2 (~78%; misdescription unfixable) | 4.5 (~97%) | 4.5 (~95–98%) |
| Freeze-risk (×2) | 5 | 3 (registry unbaked; else fine) | 3 | 4.5 (one boundary; coarseness permanent) | 4 (three boundaries, all revision-proof; 7 names frozen) |
| Naming compliance (×1) | 5 (no new names) | 2.5 (`definitions` clash; `parsers`; unexamined `lang`) | 1.5 (stage verbs; misdescribing names) | 4 (`parsing` underdescribes its node half) | 4.5 (all established words; `lang` stutter noted) |
| Path ergonomics (×1) | 4.5 (2 namespaces; shortest paths) | 3 | 3 | 4 (T1 in 3 namespaces — best split) | 3.5 (T1 in 6 namespaces; depth 3–4) |
| Rustdoc quality (×1) | 2.5 (150-name page; kind-grouping only) | 4 | 3.5 | 3 (~70-name `parsing` page) | 4.5 (largest page 42; every page one story) |
| **Weighted sum** | **37** | **21.5** | **20** | **33.5** | **34** |

\* C0's obvious-home score is degenerate — the criterion cannot distinguish "every
placement is obvious" from "no placement exists". The matrix therefore **cannot
decide flat-vs-split**; it decides *among splits* (C4 > C3 ≫ C1 > C2, robust to any
reasonable reweighting). Flat-vs-split is a straight value trade the user must make:

> **C0 → C4 buys**: reference pages that each tell one true story (2.5→4.5), a
> namespace that teaches the architecture, autocomplete scoped to topic, and the
> conditions registry's wire-doc page — at the price of: ~9 items whose home is
> arguable rather than obvious, 7 frozen topic names + one frozen 4-way taxonomy,
> T1's imports spread over six namespaces instead of two, and one extra path
> segment everywhere. Neither risk is recoverable later: C0 cannot be split
> additively, and C4 cannot be re-flattened additively (ruling 1 forbids the dual
> spellings either transition would need). This is a one-shot, symmetric-regret
> decision.

---

## 8. Ranked recommendations (labeled as such; the user decides)

**R1 — C4 Variant B**: `techy::{source, error}` + `techy::core::{lang, specs,
parsing, node}` + `core::conditions` + `node::extract`; all nine engine items in
`parsing`; wrapper kept; riders R-a/R-b/R-c.
*Decisive trade-off*: freeze seven established topic names (and one 4-way taxonomy
whose every stress point is catalogued in §5) in exchange for a public surface where
every page tells one story, every planned P3/P4 addition has exactly one obvious
home, and all four live revisions are already internal. Worst residual straggler:
the argument model's forced cut (`ArgumentParser` trait in `specs`, its seven
implementations in `parsing`). If the engine cut feels wrong, Variant A (the user's
sketch: driver + `Language` in `lang`) is the same candidate with four stragglers
instead of two — a taste call flagged for the ruling.

**R2 — C3 two-way** (`core::{lang, parsing}` + top-level `source`/`error` +
`parsing::conditions`): choose if taxonomy freeze worries dominate. *Decisive
trade-off*: the fewest frozen names and the best straggler rate (~97%) of any split,
but the ~70-name `parsing` page keeps most of the flat-page problem — you buy the
one big distinction (specify vs run/consume) and no browsability below it, and the
coarseness is permanent (no additive refinement exists under ruling 1).

**R3 — C0 flat `core`** with `core::extract` + `core::conditions` (R-a matters most
here): choose if any coin-flip placement is deemed worse than no placement. *Decisive
trade-off*: zero stragglers and zero taxonomy freeze, shortest paths — and a
reference that is a phone book, with the architecture's topics existing only in
guides. Honest calibration from §1: with the conditions registry taken, the flat page
is ~150 names and `syn`-precedented; the gap between C0-with-riders and C4 is
smaller than the raw "180 in one module" fear suggests.

Not recommended: **C1 as sketched** (it is C4 minus the decisions that matter — its
~35-straggler count is concentrated exactly in the families it leaves silent;
adopting it means making C4's choices anyway, just implicitly), and **C2 pipeline**
(rejected on [§dd-dr:three-strata] grounds: stage taxonomies of a deliberately
mutually-recursive stratum misdescribe every dual-stage item permanently; the
pipeline is the guide's story, not the path structure's).

Sub-decisions packaged for the ruling session, in dependency order:
1. Split or flat (C4 / C3 / C0)?
2. If split: engine cut A (sketch-faithful) or B (runtime united; recommended)?
3. Conditions registry (recommended under every answer to 1 — including C0).
4. `source`/`error` top-level (recommended under any split; under C0, inside core).
5. Wrapper kept (recommended everywhere; drop only to buy the E0659 fix).
6. Names: `specs` not `definitions` (defs-database collision), `parsing` not
   `parsers` (covers session/entry/result), `lang` accepted with stutter caveat,
   `node`/`source`/`error`/`conditions` as established.

---

## 9. Addendum (2026-07-29, after user counterproposal) — C5: hub + extracted subsets

The user reframed the question: not "how to partition core" but "which obvious
subsets to *extract* from it", leaving the rest as a flat hub. The counterproposal,
formalized (specs/scopes were unmentioned → hub by default; conditions
producer-side per the user's explicit rejection of the registry, examined below):

```rust
pub mod source;                  // 14  (as C4)
pub mod error;                   // 14  (as C4)
pub mod extract;                 //  9  — top-level: helpers extracting data from trees
pub mod core {                   // ~57 flat hub: Lang/state (9), token (18),
                                 //   specs+scopes (21), engine (9), 3 conditions
    pub mod constructs;          // 52: CON-DISP + CON-STD + COND-CON (19)
    pub mod node;                // 30: NODE-READ + NODE-BUILD + NODE-EXT (+NodeExtTypes)
}
pub mod latexlike;               // 23  (unchanged)
// future, additive: pub mod transform;   // P4 tree-transformation infrastructure
pub const VERSION: &str = …;
```

**Why the hub shape is structurally strong here**: the extraction rule ("take a
subset only where its boundary is crisp") is exactly complementary to the straddle
families (§0.2) — the families live in the *hub*, uncut. Family 2 (engine nine):
together in the hub — `Language::parse() -> ParseResult` on one page, no
`lang`-without-`Language` trap, C4's one genuine sub-decision (engine cut A/B)
dissolves. Families 3 (token data/runtime) and 7 (`Lang` hub): together. Spec+scopes
merge: pre-absorbed. Family 4 (node): whole in the satellite ✓ P4-safe. Family 5
(argument model): the usual cut — trait+`ArgumentSpec` hub-side, 7 implementations
in `constructs` — softened, since "the rest of core" is a gentler home for the trait
than a dedicated `specs` group that advertises the separation. Family 1
(conditions): see below. Family 6 (diagnostics): together in `error`.

**Straggler audit**: argument-model cut (1, everywhere); conditions in two homes —
19 in `constructs`, 3 in hub — but by one comprehensible rule, "beside the machinery
that reports them" (mild, user-endorsed); `verbatim_state_delta`, `skip_whitespace`,
`Frame`/`TraceFrame`, `ParsedArgumentNodes`/`ParsedArguments` adjacency — the same
four milds/pre-existing as C4. **≈6 items, ~97% — equal-best with C3, and strictly
fewer *forced decisions* than C4** (no engine cut, no `lang` naming, no
`specs`-vs-`definitions` issue). One systemic note replaces C4's strata purity:
`extract` is Lang-generic (S1-dependent), so the top level is no longer "S0 only" —
the top-level logic becomes *role-based* ("data models and consumer tool-libraries
up top; machinery in `core`; preset in `latexlike`"), which is a clear logic, just a
different one; the planned `transform` fits it perfectly (`extract` reads trees,
`transform` rewrites them, both are tool libraries over `core::node` types).

**Conditions registry — user's rejection examined and CONCEDED.** The user's three
arguments: (i) a central module is coupled to every parser's internals; (ii) the
family is *open* — custom parsers define new condition types in their own crates, so
the registry can never be exhaustive, only "built-in conditions" (§6.2 conceded this
as "an area index"); (iii) error logic splits between parser and registry.
Decisive additional fact: **the documentary need F9 identified is served for free by
rustdoc** — every condition implements `DiagnosticInfo`, so the trait's implementors
list *is* the auto-maintained, always-exhaustive-for-this-crate registry page; a
guide table (identifier ↔ type ↔ producer) covers the wire-format reference. The
registry's only unique value was a *path* commitment, and under C5's geometry the
scatter it was designed to prevent barely occurs (19 of 22 land together in
`constructs` by the producer rule). Registry DROPPED; F9's identifier-area
decoupling (P5) is unaffected and still required.

**Freeze-risk**: frozen names: `source`, `error`, `extract`, `core`, `constructs`,
`node` (+ `transform` reserved, additive when it lands). All four live revisions
invisible or pre-absorbed ✓. P3 additions → hub ✓. P4 → `transform` + `node` ✓.
One-shot warnings that must be ruled *now*: (a) extracting any further subset from
the hub later (e.g. `core::specs`) is breaking — the counterproposal's implicit
"specs/scopes stay in the hub" is itself a permanent ruling; (b) reversing the
registry rejection later is breaking (adding `conditions` would move 22 items).

**Names within C5**: satellite name — **`constructs` recommended** over
`parsers`/`parsing`: it is today's established topic word (zero churn, principle
"no churn without cause"), and the module holds more than parsers (`Invocation`,
`ParseContext`, `StopSpec`, 19 conditions) — "the construct-parsing layer" covers
all of it, while `parsers` under-describes and `parsing` was only needed in C4
because the engine lived there (it does not, here). `extract`: keep the name —
established, designed for qualified reading; no better candidate found (`read`,
`query`, `text` all vaguer or narrower). Hub description is a true one-liner:
"defining and running languages: the `Lang` contract, parsing state, token rules,
callable specs and scopes, and the engine".

**Scores (same weights as §7)**: obvious-home 4.5 (×3), freeze 4.5 (×2, one fewer
frozen name than C4, engine cut gone; hub-extraction one-shots noted), naming 4.5
(all established words, no stutter, no collision), path ergonomics 4.5 (hot T3
vocabulary at 2-segment `core::X`; best of any split), rustdoc 4 (hub ~57 mixed;
`constructs` carries the 19-condition wall — optional mitigation: a *local*
`constructs::conditions` submodule, purely cosmetic, decide at guide-writing time).
**Weighted sum ≈ 36 — the highest of any candidate** (C4 34, C3 33.5, C0 37\*
degenerate).

**Verdict: C5 supersedes C4 as the recommendation.** It keeps every strength that
made C4 win among partitions (crisp satellites for the two biggest coherent
libraries; S0 models top-level; all revisions invisible) while dissolving C4's
residual costs (engine cut, `lang` naming, specs/defs collision) — at the price of
one medium heterogeneous hub page and the role-based (rather than strata-based)
top level. Remaining sub-rulings: satellite name (`constructs` rec.); specs/scopes
stay in hub (now-or-never); `extract` name + top-level placement; conditions
producer-side (constructs + hub); `transform` reserved for P4.
