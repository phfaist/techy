# Phase 2b — Tier-C Batch Decision-Session Brief (the no-usage-signal sweep)

Prepared 2026-08-03. Inputs: PLAN.md decision log (P1–P5 + T1/T2 + T3 + T4 + T5 +
recompose rulings, all binding), SYNTHESIS.md §3 (the item universe), INVENTORY.md
(per-item descriptions), P4_RULING.md, T1T2/T3/T4/T5/RECOMPOSE_RULINGS.md,
DESIGN_RATIONALE entries as cited. Every code claim re-verified against the working
tree at commit 6326db2 (branch api-review; file:line paths relative to `techy/src/`).
The brief recommends; all rulings are the user's.

**What this session decides.** The five persona walkthroughs (simulated users of the
library: a document consumer, an extender, a language designer, a tooling author, and
a framework builder) touched 64 of the crate's 140 root re-exports. A *root
re-export* is a name additionally spelled at the crate top level (`techy::Foo` beside
`techy::token::Foo`); under the ruled topology those dual spellings disappear anyway
(one canonical path per item). What remains to rule is the fate of the **76 root
re-exports no walkthrough touched** (SYNTHESIS §3): for each item, **pub-and-stable
vs `pub(crate)`**. Per the P5 stability rubric there is exactly ONE stability class —
everything `pub` is a stable, semver-guarded commitment; there is no experimental
tier. `pub(crate)` means the item becomes visible only inside the techy crate itself:
it vanishes from the public API. The asymmetry to keep in mind throughout:
`pub(crate)` is reversible (re-publishing later is an additive, non-breaking change),
`pub` is forever — but the review's success criterion is that FRAMEWORK development
on top of techy never forces a techy restructuring, so a surface a framework
plausibly reaches for should stay `pub` now.

**Placement is (almost) fixed.** The P1 "C5" topology ruling
([§dd-dr:public-namespace-topology]) already assigns homes: `techy::{source, error,
extract, visit, transform, recompose}` top-level; `techy::core` = the flat hub
(the `Lang` contract, parsing state, token machinery, engine); satellites
`core::constructs` (construct-parsing library + its conditions), `core::specs`
("author-side": callable specs, the argument model, providers/packages, scopes),
`core::node` (node trees); `techy::latexlike` unchanged. Conditions live
**producer-side** (declared beside the machinery that raises them — the registry
module was rejected in P1). This brief flags placement only where the rulings left
it genuinely open (two cases: `FrameRole`, `ParsedArgumentNodes` — see G7/G8).

**The "forced pub" lens.** For every item below, the brief states whether it is
*forced pub*: an item that appears inside another public item's signature — a public
function's parameter or return type, a public struct field's type, a public trait
method's types, a variant payload of a public enum. A forced-pub item cannot become
`pub(crate)` without also changing (or demoting) the signatures that name it; the
brief says explicitly what would have to change. The single biggest empirical finding
of this preparation: **the large majority of the 76 "untouched" items are forced pub
through signatures of items the walkthroughs DID use** — "no usage signal" mostly
means "the type the used method returns was never spelled out", not "removable".

**Verification ledger (surprises found while preparing):**
1. **`Diagnostics::into_vec` does not exist** (grep: no `into_vec` anywhere in
   `techy/src/`). It was a T5 walkthrough *wish*; the recorded "lean reject" (T5
   I-17) means *do not add it* — there is nothing to demote. Rider R5 records the
   disposal.
2. **`NodeData` is the only pub item in the node group that appears in zero public
   signatures** (verified: defined node/tree.rs:110; the sole non-definition use is
   the private helper node/node_ref.rs:356; `NodeRef` mediates every read). It is a
   clean `pub(crate)` candidate — the rarity of that situation is itself the
   finding.
3. **`Scope` is not actually dead machinery**: `ScopeStack::apply_op` *creates*
   `Scope` values internally when applying a define-operation to a named scope that
   doesn't exist yet (scopes/mod.rs:1382). The "entire mutable-provider story went
   unused" reading in SYNTHESIS §3 is true of the walkthroughs but the type is the
   load-bearing carrier of runtime definitions (`\newcommand`-class behavior).
4. **`StagedNodes` still sits in a public trait signature today** —
   `Lang::finalize_node(…, staged: &StagedNodes<'_, Self>)` (state/lang.rs:351) —
   but P4 *deletes* that hook (replaced by `make_node_ext` taking the new
   `StagedChildren` view). What keeps the staged-view pair public after P4 is
   different: `StopSpec`'s public `node` predicate field takes `StagedNodeView`
   (constructs/nodes_parser.rs:297), and P4 point 3 explicitly keeps "a public read
   view `cx.staged_nodes()`" for node-based stop predicates.
5. SYNTHESIS §3 counts `UnclosedGroupFound`, `MissingTerminatorFound`, and
   `UnusableRecoveryTokenKind` among "condition types" — they are payload-detail
   enums (fields of conditions), not identifier-bearing conditions (already noted as
   T4 stale-claim #2). Harmless; G3 treats them separately.
6. No contradiction found between session rulings that bears on this batch. The one
   near-miss: CORE_SPLIT_OPTIONS §9 recorded C5's "specs/scopes stay in hub" as a
   now-or-never sub-ruling, but the final P1 ruling extracted `core::specs` *with
   scopes in it* ("providers/packages, scopes" — the entry text), so the scopes rows
   in this batch have a fixed home.
7. Two items in the batch changed identity since SYNTHESIS was written:
   `SimpleLang` is ruled to become `TrivialLang` (T3-A) and `resolve_source`'s
   status flipped from "redundant, removable" (INVENTORY oddity 3) to "the one
   canonical composition" (T4-B2, once `Language::resolve_source` leaves).

---

## 1. Already ruled — 12 of the 76 (confirm, do not re-litigate)

These items were disposed of by later sessions; they appear in the sweep table
(§4) with pointers so nothing dangles.

| Item | Ruling | Pointer |
|---|---|---|
| `SourceResolver` (trait) | Keep pub — verified to BE the minimal filesystem-interface trait; the resolver seam moves onto the driver (`ParseDriver::source_resolver`). Home `techy::source`. | T4-C, T4-B1; [§dd-dr:source-resolver] |
| `resolve_source` (free fn) | Keep pub — becomes the ONE canonical resolve composition once `Language::resolve_source` leaves with the Language collapse. Home `techy::source`. Rider R3 confirms below. | T4-B2/B4; [§dd-dr:input-wiring] |
| `SimpleLang` (trait) | Keep pub, renamed **`TrivialLang`** — repositioned as the test/prototype lang; wish 18a (overridable driver) rejected. | T3-A; [§dd-dr:trivial-lang] |
| `ArgumentParser` (trait) | Keep pub — home **`core::constructs`**, beside `ConstructParser` and the shipped argument parsers. | T3 sweep; [§dd-dr:public-namespace-topology] amendment |
| `CallableQuery` | Keep pub — moves to **`core::specs`** beside `resolve_command_in_scopes` (the whole resolution family lands once). | T3-H; [§dd-dr:resolution-extraction] |
| `CallableSyntax` | Keep pub — same move (resolution family). | T3-H |
| `SearchedProviders` | Keep pub — same move (resolution family; it is the miss-detail carrier the did-you-mean ruling builds on). | T3-H; [§dd-dr:registration-ergonomics] |
| `ResolvedCallable` | Keep pub — same move (payload of `CommandResolution::Resolved`, engine/driver.rs:376/409). | T3-H |
| `ParsedSlots` | Keep pub — gained the discoverable constructor `ParsedSlots::new(Vec)` (wish 22); SYNTHESIS itself reclassifies it "used-by-T3, surface-file omission". Home `core::node`. | T3 C+G |
| `NodeId` | Keep pub — P4 made it `{index, tree_tag}` with the tag in `Eq`/`Hash`; it is the FFI round-trip handle (`Arc<NodeTree>` + `NodeId`) the framework walkthrough verified. Home `core::node`. | P4 point 2; T5/Phase-1b |
| `ParserSession` | Type stays pub (forced: it is the public `session` field of `ParseContext`, constructs/mod.rs:108–129, used constantly as `cx.session`); the **`builder` field becomes `pub(crate)`** (staging goes through `cx.stage_node()`); `diagnostics`/`ext` fields stay pub. Home `techy::core`. | P4 point 3 |
| `StdParseDriver` | Keep pub — `default()` removed; repositioned as the test-carrier driver ("resolves nothing") beside the new `ScopesResolvingDriver`. Rider R4 carries the doc obligation. Home `techy::core`. | T3-A+F; [§dd-dr:scopes-resolving-driver] docs clause |

**Count.** 12 ruled + 3 rider items (§3: `NoResolver`, `ProvenanceChain`,
`ResolvedContent`) leaves **61 items in the decision groups below** — of which
roughly 50 are forced pub or doctrine-bound, and about **11 are genuinely open
judgment calls** (marked ◆ in the groups).

---

## 2. Decision groups

Format per group: Context → the items (with verified state) → Options →
**Recommendation** → What a framework builder loses if it went `pub(crate)`.
"Framework builder" throughout = the primary persona: latex2text/FLM/latexpp-class
frameworks, likely exposing Python bindings.

### G1. The scopes/provider machinery family (9 items → `core::specs`)

**Context.** The scopes module implements the definitions stack: *providers*
(objects answering "what does `\foo` mean?") stacked so inner definitions shadow
outer ones. The walkthroughs needed only `Package` (the immutable provider),
`ScopeStack`, and the `SpecsProvider` trait — everything else in the module went
untouched. But the untouched half is the *runtime-definitions* machinery: how a
document defines new commands mid-parse (`\newcommand`-class behavior), how
unknown names get policy answers, and how failures are reported. Placement is
fixed: the P1 entry text puts "providers/packages, scopes" in `core::specs`
(author-side — what you write to define callables and organize definitions).

**The items (verified).**

| Item | What it is (plain) | Defined | Forced pub? |
|---|---|---|---|
| `ScopeOp` | The operation vocabulary for changing the provider stack mid-parse (push/unload/replace/define/remove — 6 variants), carried by state deltas. | scopes/mod.rs:316 | **Yes** — `ParsingStateDelta.scope_ops: Vec<ScopeOp<L>>` is a public field (state/delta.rs), and `ScopeStack::apply_op(&ScopeOp)` is public. Demoting it would gut the delta model. |
| `DefinitionOp` | The define/remove sub-vocabulary a mutable provider accepts (one definition added or removed). | scopes/mod.rs:253 | **Yes** — parameter of the public trait method `SpecsProvider::with_definitions(ops: &[DefinitionOp<L>])` (scopes/mod.rs:494–497). |
| `Scope` ◆ | The mutable-by-replacement standard provider — the container runtime definitions land in. | scopes/mod.rs:894 | No public signature names it — but `ScopeStack::apply_op` *creates* one internally when a define targets a not-yet-existing named scope (scopes/mod.rs:1382), and it is the only shipped mutable provider. |
| `FallbackProvider` ◆ | A provider answering EVERY name of a given callable type with one shared spec — the "unknown command policy" building block. | scopes/mod.rs:1030 | No. Constructed by consumers; consumed as `Arc<dyn SpecsProvider>`. |
| `ErrorCallableSpec` ◆ | A spec meaning "this name is *defined to be an error*" — invoking it records a diagnostic and recovers (pylatexenc parity for deliberately-poisoned names). | scopes/mod.rs:1122 | No. Constructed by consumers. Its condition `CallableDefinedAsError` is slate-frozen (G3). |
| `SymbolEntry` | One row of a provider's symbol listing (name + spec + visibility) — what `iter_symbols` yields. | scopes/mod.rs:525 | **Yes** — item type of the public trait method `SpecsProvider::iter_symbols` (scopes/mod.rs:513–517); also the substrate of the ruled did-you-mean detail and the parse-init escape-char warning ([§dd-dr:registration-ergonomics]). |
| `ProviderError` | Error a provider returns from lookup/mutation (not-mutable / internal failure / …). | scopes/mod.rs:155 | **Yes** — `SpecsProvider::retrieve_spec` and `with_definitions` return `Result<_, ProviderError>` (trait methods third parties implement — T3 did). |
| `ScopeStackError` | A provider error wrapped with WHICH provider failed (stack-level context). | scopes/mod.rs:187 | **Yes** — return of `ScopeStack::retrieve_spec` (scopes/mod.rs:1259) and payload of `ScopeOpError::Provider`. |
| `ScopeOpError` | Error of applying one scope operation (unknown provider / provider failure). | scopes/mod.rs:216 | **Yes** — return of `ScopeStack::apply_op` (scopes/mod.rs:1347) AND public field `DeriveError.failures: Vec<ScopeOpError>` (state/parsing_state.rs:289). |

**Options.**
1. **Keep all nine pub** in `core::specs`.
2. Keep the six forced items; demote `Scope`/`FallbackProvider`/`ErrorCallableSpec`
   to `pub(crate)`. Consequence: `ScopeOp::Define` would still WORK (the stack
   creates scopes internally), but consumers could not pre-build a scope, inspect
   one, or register fallback/error policies — and `DefinitionOp`/`with_definitions`
   would be a public contract whose only shipped implementor is invisible.
3. Demote the whole non-forced set and re-publish on demand. Rejected shape:
   option 2 already shows the seams don't cut cleanly.

**Recommendation: 1 — keep all nine.** The forced six are not really decisions;
the three ◆ items are the shipped implementations of contracts that stay public
either way, and each has a concrete framework consumer story: `Scope` +
`ScopeOp`/`DefinitionOp` are the `\newcommand` mechanism FLM-class languages need
mid-parse; `FallbackProvider` is latex2text's "unknown macro → passthrough/warn"
policy expressed in-parse; `ErrorCallableSpec` is the "this command is
deliberately an error here" story whose diagnostic identifier
(`core.specs.callable-defined-as-error`) is already frozen in the T4 slate —
a public, stable wire identifier raised by a crate-private spec would be
incoherent.
**Framework loss under pub(crate):** the runtime-definitions tier — a framework
implementing `\newcommand` would have to re-implement a provider from scratch
against `SpecsProvider` (possible but exactly the "rebuild what techy has"
outcome the review exists to prevent); unknown-name policy would move to
post-parse patching.

### G2. The token error/recovery/prefix family (6 items → `techy::core`)

**Context.** The token module's error half and its two helper types went
untouched: even the full custom-language walkthrough (T3) never *named* them —
because it consumed them through inference (`cx.tokens.peek()?` propagates
`TokenError` without spelling it). The tokenizer contract `TokenReader` IS
public and T3-implemented-against, which forces most of this family.

**The items (verified).**

| Item | What it is (plain) | Defined | Forced pub? |
|---|---|---|---|
| `TokenError` | The tokenizer's error value: what went wrong + where + an optional pre-built recovery token. | token/error.rs:104 | **Yes** — `TokenReader::peek/next` return `TokenResult<…> = Result<T, TokenError>` (token/reader.rs:49, error.rs:25); `TokenReader` is a used public trait. |
| `TokenErrorKind` | The closed list of tokenizer failure kinds (3 variants, non_exhaustive). | token/error.rs:75 | **Yes** — `TokenError::kind()` returns it; `ParseError::from_token_error(kind, span)` takes it (error.rs:632). |
| `TokenRecovery` | The "how to continue after this token error" payload (recovery token + resume position). | token/error.rs:126 | **Yes** — `TokenError::recovery()/into_recovery()` return it (error.rs:155–160); it is the tolerant-recovery protocol's carrier. |
| `PrefixTable` | The precomputed longest-match table of group-delimiter spellings, derived from the token rules — what a tokenizer consults at each position. | token/prefix_table.rs:51 | **Yes** — `ParsingState::prefix_table()` is public (state/parsing_state.rs:189); it exists so token readers (std or custom) share one match mechanism. |
| `StdTokenReader` ◆ | The standard tokenizer implementation over a `&str` — the thing `Language::parse` drives, and the thing an embedder mints to drive `ParseContext` by hand. | token/reader.rs:146 | No public signature names it (engine uses it internally, language.rs:201) — but the manual-drive pattern (`StdTokenReader::new(content)` then `ParseContext::new`) is the documented example (engine/language.rs:58) and the shape every in-crate test harness uses. |
| `skip_whitespace` ◆ | The one shared whitespace primitive implementing the paragraph rule (never silently consume a paragraph break) — the subtle contract both std reader paths lean on. | token/reader.rs:95 | No. Free fn; used internally by the std reader (reader.rs:194, 379, 426). |

**Companion (not in the 76):** `PrefixEntry` — one row of the prefix table; today
the ONLY token item not root-exported while root-visible `PrefixTable::entries()`
returns it (INVENTORY oddity 9). Forced pub via `entries()`/`match_at()`. Under C5
the root question disappears; the pair simply lives together in `core`.

**Options.**
1. **Keep all six pub** (+ `PrefixEntry`).
2. Keep the four forced; demote `StdTokenReader` and `skip_whitespace`.
   Consequence: the public `TokenReader` seam remains, but techy ships no public
   implementation of it and no public paragraph-rule primitive — a custom-Lang
   author writing tests, a tooling author re-tokenizing a snippet, or a T4-style
   manual drive would have nothing to instantiate; a custom `TokenReader` would
   transcribe the paragraph rule by hand to stay conformant.
3. Keep `StdTokenReader`, demote only `skip_whitespace` (narrowest cut).

**Recommendation: 1 — keep all six.** The error/recovery trio and `PrefixTable`
are forced by the `TokenReader` contract T3 builds against. `StdTokenReader` is
the standard implementation of a public trait — the `StdCallableSpec`/
`StdParseDriver` pattern; SYNTHESIS starred it precisely because every parse
engages it. `skip_whitespace` is the weakest keep (◆): if the user doubts the
custom-tokenizer audience, `pub(crate)` is the reversible direction — but the
paragraph rule is exactly the kind of subtle shared semantics that should have
one public source of truth rather than N transcriptions.
**Framework loss under pub(crate):** custom tokenization (FLM variants with
different comment/escape conventions that still want conformant whitespace and
delimiter matching) and any tooling that drives parsing below `Language::parse`.

### G3. The unused condition types (17 items → producer-side homes)

**Context.** A *condition type* is the Rust struct carrying one diagnostic's
structured data; each has a stable string *wire identifier* (e.g.
`core.groups.unclosed-group`) and is matched BY TYPE (`diag.is::<UnclosedGroup>()`
/ `T::IDENTIFIER`) — the documented matching rule (F9). No walkthrough downcast
these 14 (the only condition types anyone used were `UnresolvableCommand` and
`MissingMandatoryArgument`, not in this batch). Three binding rulings constrain
the group:
- **T4-A froze the identifier slate for all 22 production conditions** — including
  every condition here. A frozen, semver-stable identifier is a commitment that
  the condition exists and is matchable.
- **P1 ruled conditions live producer-side** (declared beside the machinery that
  raises them; registry rejected).
- **The F9 documentation plan** is the rustdoc "implementors of `DiagnosticInfo`"
  page plus a guide table — which lists *public* implementors only.

Making any of these `pub(crate)` would produce a diagnostic that still renders and
still carries its frozen public identifier on the wire, but that no consumer can
match typed — string-matching, the exact practice F9 exists to prevent, would
become the only option. The types are therefore **doctrine-bound keeps**, not free
choices, even though (unlike G1/G2) most are not signature-forced: a condition is
raised internally and boxed as `dyn DiagnosticData`, so the compiler would not
object to demotion — the *diagnostics contract* would break, silently.

**The items.** 14 identifier-bearing conditions + 3 payload enums:

| Item | Raised when (plain) | Slate identifier (T4-A frozen) | Home |
|---|---|---|---|
| `CommandResolutionFailed` (constructs/nodes_parser.rs:139) | A provider ERRORED during command lookup (vs. clean miss). | `core.specs.command-resolution-failed` | producer (nodes_parser) |
| `UnclosedGroup` (constructs/group_parser.rs:61) | A `{` was never closed. | `core.groups.unclosed-group` | producer |
| `StrayGroupClose` (constructs/nodes_parser.rs:330) | A `}` with no open group. | `core.groups.stray-group-close` | producer |
| `ExpectedExpressionArgument` (constructs/argument_parsers.rs:97) | Expression-style argument position had nothing usable. | `core.arguments.expected-expression-argument` | producer |
| `ExpressionCallableRequiresContent` (constructs/nodes_parser.rs:170) | An expression-position callable needed content and got none. | `core.arguments.expression-callable-requires-content` | producer |
| `MissingEnvironmentTerminator` (constructs/environment_parser.rs:102) | `\begin{x}` with no `\end{x}` before input ran out. | `core.environments.missing-terminator` | producer |
| `EnvironmentTerminatorMismatch` (environment_parser.rs:73) | `\end{y}` closing `\begin{x}`. | `core.environments.terminator-mismatch` | producer |
| `MalformedEnvironmentTerminator` (environment_parser.rs:89) | An `\end` that could not be read as a terminator. | `core.environments.malformed-terminator` | producer |
| `ScopeOpFailed` (constructs/mod.rs:453) | A state delta's scope operation failed during derivation. | `core.specs.scope-op-failed` | producer (constructs) |
| `UnusableRecoveryToken` (nodes_parser.rs:186) | Tolerant recovery produced a placeholder the loop could not use. | `core.recovery.unusable-recovery-token` | producer |
| `ImplementationError` (constructs/mod.rs:437) | An extension contract was violated (the panic-free reporting channel of the panic policy). | `core.constructs.implementation-error` | producer |
| `EndOfStreamAfterEscape` (token/error.rs:34) | Input ended right after the escape char. | `core.token.end-of-stream-after-escape` (keep) | hub (token conditions) |
| `ForbiddenChar` (token/error.rs:46) | A char the current state forbids. | `core.token.forbidden-char` (keep) | hub |
| `CallableDefinedAsError` (scopes/mod.rs:1093) | An `ErrorCallableSpec`-defined name was invoked. | `core.specs.callable-defined-as-error` | specs (producer) |
| `UnclosedGroupFound` (group_parser.rs) | Payload enum: what was found instead of the close. | — (no identifier) | with its condition |
| `MissingTerminatorFound` (environment_parser.rs) | Payload enum: what ended the search. | — | with its condition |
| `UnusableRecoveryTokenKind` (nodes_parser.rs:195) | Payload enum: why the token was unusable. | — | with its condition |

The three payload enums ARE signature-forced (public fields of their conditions).
Family-consistency note: the three conditions that were never root-exported
(`ExpectedVerbatimDelimiter`, `UnterminatedVerbatim`, `RepeatedTackOnField` — not
in this batch, INVENTORY oddity 2) prove the module-only life these types are
headed for; under C5 the whole family becomes uniformly producer-side and the
root/module split dissolves.

**Options.**
1. **Keep all 17 pub**, producer-side per P1.
2. Demote the never-downcast ones. Breaks the frozen slate's typed-matching story
   and the F9 implementors page for exactly those rows; saves nothing a consumer
   ever sees (the types are off every happy path already).

**Recommendation: 1 — keep all 17.** This is the ruled diagnostics architecture
completing itself, not new surface.
**Framework loss under pub(crate):** typed error handling — a latex2text-class
tool distinguishing "unclosed group" from "unknown environment" for user-facing
messages, or an editor plugin suppressing specific conditions, falls back to
matching stability-exempt `Display` strings.

### G4. The diagnostics-DEFINING surface (5 items → `techy::error`)

**Context.** Distinct from G3 (shipped conditions), these five are what a *third
party* uses to define NEW conditions. No walkthrough defined a custom condition —
but the P5 wire-identifier ruling explicitly plans for downstream vocabularies
(`flm.*` conditions raised by a framework's own parsers), and every custom
construct parser that reports errors properly needs this surface.

| Item | What it is (plain) | Forced pub? |
|---|---|---|
| `DiagnosticData` (trait, error.rs:82) | The object-safe carrier every diagnostic stores — "some condition data, type-erased". | **Yes** — `Diagnostic::data()` / `ParseError::data()` return `&dyn DiagnosticData` (error.rs:380, 655). |
| `DiagnosticValue` (enum, error.rs:138) | The serializable value tree a condition renders its payload into (the wire-data format). | **Yes** — return of `DiagnosticData::serializable_data()` and `ToDiagnosticValue::to_diagnostic_value()`. |
| `ToDiagnosticValue` (trait, error.rs:172) | "This field type knows how to render itself into a `DiagnosticValue`" — the payload-field contract. | **Yes** — the derive-emitted code calls it on every payload field; it is the public bound field types must satisfy. |
| `DiagnosticInfo` (derive macro, techy-derive) | Writes the condition boilerplate (identifier const, `Display` glue, serialization) from an attribute. Used 32× in-crate — every shipped condition is declared with it. | Not signature-forced; capability-critical (below). |
| `ToDiagnosticValue` (derive macro) | Same, for payload enums (5 in-crate uses, e.g. `UnclosedGroupFound`). | Not signature-forced; capability-critical. |

Note the `DiagnosticInfo` *trait* is not in this batch (T3 used it for
`IDENTIFIER` consts); only the derive macro of the same name is. The derive
re-exports already route their emitted paths through `#[doc(hidden)] __private`
(P1 rider), so the macros impose no topology constraint.

**Options.**
1. **Keep all five pub.**
2. Keep the three forced; drop the two derive re-exports (users would hand-write
   `DiagnosticInfo` impls). Saves two names; costs every downstream condition
   author the exact boilerplate techy wrote a derive to avoid — and techy itself
   uses the derive 32 times.

**Recommendation: 1 — keep all five.** The framework persona is the strongest
argument in this whole batch: an FLM-class language raising `flm.*` conditions is
a *planned, ruled* scenario (P5 first-segment rule), and it is impossible without
this surface.
**Framework loss under pub(crate):** the ability to define first-class
diagnostics at all — custom parsers would be reduced to `ImplementationError`
or ad-hoc strings.

### G5. Node build/staging internals (7 items → `core::node`)

**Context.** The builder-side machinery beside the T1 read API (INVENTORY oddity
6). P4 restructured this corner heavily: staging now goes through
`cx.stage_node()`, `ParserSession::builder` becomes `pub(crate)`, `make_node_ext`
gets the new `StagedChildren` view, and T5 added `validate_tree` (the public
`Result`-returning all-trees validator, home `core::node`) plus the level-0
restage primitive on `NodeTreeBuilder`. That leaves per-item fates:

| Item | What it is (plain) | Defined | Forced pub? |
|---|---|---|---|
| `GroupData` | The payload of a group node (delimiters, group class). | node/kind.rs:143 | **Yes** — variant payload `NodeKind::Group(Box<GroupData<L>>)` of the closed public `NodeKind` (kind.rs:41). |
| `NodeSliceIter` | The iterator type behind `NodeSlice::iter()` (starred: T1/T2 called `.iter()` without naming it). | node/slice.rs:128 | **Yes** — return type of `iter()` and the `IntoIterator::IntoIter` associated type (slice.rs:82, 166). |
| `NodeBuildError` | The builder's error enum (15 variants of tree-invariant violations). | node/builder.rs | **Yes** — `NodeTreeBuilder::add/finish` return `Result<_, NodeBuildError>` (builder.rs:113–218), and the builder stays public for the transform tier (P4/T5: `restage_node`, `RestageError` interplay). |
| `StagedNodes` | Read view over already-staged (not yet finished) nodes. | node/builder.rs:346 | After P4: yes-by-ruling — P4 point 3 keeps the public read view `cx.staged_nodes()` (its return type); today also in `Lang::finalize_node`'s signature, which P4 deletes (ledger #4). |
| `StagedNodeView` | One staged node as seen through that view. | node/builder.rs:370 | **Yes** — `StopSpec`'s public `node` predicate field takes it (`FnMut(usize, StagedNodeView) -> bool`, constructs/nodes_parser.rs:297). |
| `NodeData` ◆ | One stored node's raw record inside the tree — the storage struct `NodeRef` wraps. | node/tree.rs:110 | **NO** — appears in zero public signatures (verified; ledger #2). All reads go through `NodeRef`, all writes through the builder. |
| `check_tree_invariants` ◆ | The panicking tree checker (asserts, for tests/debug). | node/invariants.rs | No — free fn; ~15 in-crate `#[cfg(test)]` call sites. T5-F2 ruled `validate_tree` (Result-returning) as the public validator and said this one "keeps its panicking test-utility role" — visibility was NOT ruled. |

**Options (the two ◆ items; the rest are forced/ruled keeps).**
- `NodeData`: (1) **`pub(crate)`** — nothing public names it; `NodeRef` is the
  read API; P4's annotation design keeps it internal storage
  (`Arc<TreeCore<L>>` holds `Vec<NodeData>`). Reversible if a real consumer
  appears. (2) Keep pub as a documented storage type — costs a permanently
  stable struct whose only public role is being pointed at by docs.
- `check_tree_invariants`: (1) **`pub(crate)`** — in-crate tests keep using it;
  external test authors get `validate_tree` (a `Result` is *better* in
  assertions: `assert!(validate_tree(&t).is_ok())` with the violation in the
  panic message); removes a public panicking fn (the panic-policy exception
  becomes unnecessary the moment the non-panicking sibling ships). (2) Keep both
  pub with documented roles (panicking = test sugar; Result = production) — two
  public spellings of one check, rubbing against one-canonical-path.

**Recommendation:** keep the five forced/ruled items pub; **`NodeData` →
`pub(crate)`; `check_tree_invariants` → `pub(crate)`** once `validate_tree`
lands (sequencing note: the demotion rides the same Phase-3 commit that adds
`validate_tree`, so external users never face a gap).
**Framework loss:** none for `NodeData` (no reachable capability disappears —
every field is reachable through `NodeRef` accessors); for `check_tree_invariants`
only the panicking *flavor* is lost, the check itself remains via `validate_tree`.

### G6. The parse-dispatch layer (10 items → `core::constructs`)

**Context.** The machinery a custom construct parser drives when it needs
sub-parsing: "parse content until X" (`parse_nodes` + `StopSpec`), "parse one
group" (`parse_group`), and the policy for which state child content parses under
(`ChildStateSpec`). T3 *used the methods* (`cx.parse_nodes`, `cx.parse_group`)
without naming most of the types — inference again. Two driver factory hooks
(`ParseDriver::make_nodes_parser`/`make_group_parser`, engine/driver.rs:286–304)
exist specifically so a driver can substitute or wrap the standard dispatch.

| Item | What it is (plain) | Forced pub? |
|---|---|---|
| `NodesOutcome` (nodes_parser.rs:371) | What a content-parse run returns: staged node ids + why it stopped + the final state. Public fields. | **Yes** — return of `ParseContext::parse_nodes` (constructs/mod.rs:356) and the `Output` type in `make_nodes_parser`'s return signature. |
| `StopSpec` (nodes_parser.rs:285) | "Stop parsing when…" — a token condition and/or a staged-node predicate. | **Yes** — parameter of `parse_nodes` and both factory hooks. |
| `TokenStopCondition` (nodes_parser.rs:273) | The token half of a stop rule (+ consume flag). | **Yes** — public field `StopSpec.token`. |
| `TokenStopKind` (nodes_parser.rs:230) | Which token to stop at (4 variants). | **Yes** — public field `TokenStopCondition.kind`. |
| `StopCause` (nodes_parser.rs:341) | Why the run stopped (4 variants). | **Yes** — public field `NodesOutcome.stop`. |
| `ChildStateSpec` (child_state.rs:81) | The descent policy pair: which base state group interiors and invocations parse under. | **Yes** — parameter of `parse_nodes`/`parse_group` and the factories. |
| `GroupChildState` (child_state.rs:48) | The group half of that policy. | **Yes** — public field `ChildStateSpec.group`. |
| `InvocationChildState` (child_state.rs:66) | The invocation half. | **Yes** — public field `ChildStateSpec.invocation`. |
| `NodesParser` ◆ (nodes_parser.rs:418) | The standard content-dispatch loop as a constructible parser value. | Not literally — the factories return `Box<dyn ConstructParser<…>>`, naming only the trait. But it is what a factory-overriding driver constructs/wraps (the default bodies do exactly `NodesParser::new(stop).with_child_states(…)`). |
| `GroupParser` ◆ (group_parser.rs:102) | The standard single-group parser, likewise. | Same situation (`GroupParser::new(open_span, rule)`). |

**Options.**
1. **Keep all ten pub.**
2. Keep the eight forced; demote `NodesParser`/`GroupParser`. Consequence: the
   `make_nodes_parser`/`make_group_parser` override seam survives in name but a
   custom driver could no longer delegate-to/wrap the standard behavior — it
   could only replace it wholesale, which inverts the factories' documented
   purpose (and the T3 persona pattern: implement 2 of 11 hooks, inherit the
   rest).

**Recommendation: 1 — keep all ten.** The eight are forced by methods T3
demonstrably needs; the two parsers are the composition units of a public seam
(same std-implementation-of-public-contract logic as `StdTokenReader`, G2).
**Framework loss under pub(crate):** custom drivers/wrapped dispatch — e.g. an
FLM driver adding bookkeeping around content parsing while reusing the standard
loop.

### G7. The engine frame family (3 items: `Frame`, `FrameTitle` → `techy::core`; `FrameRole` → flag)

**Context.** The *live* parse traceback: while parsing, the engine keeps a stack
of frames ("while parsing environment ‘tabular’…") that gets snapshotted into
diagnostics as `TraceFrame`s (that type is separate, was used by T4, and is not
in this batch). INVENTORY oddity 7 flagged the adjacent vocabularies
(`Frame`/`FrameTitle` vs `TraceFrame`); they are different types for different
moments (live stack entry vs frozen diagnostic frame) and no rename is proposed.

| Item | What it is (plain) | Forced pub? |
|---|---|---|
| `Frame` (engine/mod.rs:51) | One live stack entry: a title + the span being parsed. Public fields. | **Yes** — parameter of the public `ParseContext::with_frame(frame, f)` (constructs/mod.rs:398), which every takeover parser uses for correct tracebacks, and which the T4-B2 door ruling builds on ("the door pushes a `Frame`"). |
| `FrameTitle` (engine/mod.rs:62) | What the frame is about (static text / callable + role / …). | **Yes** — public field `Frame.title` (engine/mod.rs:53). |
| `FrameRole` (spec/callable.rs:25) | Which part of a callable a frame describes: the invocation itself or argument #i. | **Yes** — parameter of the public defaulted trait method `CallableSpec::stack_frame_title(role, name)` (spec/callable.rs:129) and payload of `FrameTitle::Callable { role }` (engine/mod.rs:79). |

**The one open question is placement of `FrameRole`.** The P1 entry listed it
among the specs/hub-boundary judgment calls whose placement "waits for that
design" — T3-H then placed the resolution family but never `FrameRole`. Options:
(a) **`core::specs`**, beside `CallableSpec` — it parametrizes an author-side
hook (spec authors match on it when customizing traceback wording); the
`FrameTitle::Callable` reference from the hub is an accepted cross-boundary
signature reference (the P1 allowance, used in both directions already).
(b) `techy::core` (hub), beside `Frame`/`FrameTitle` — it describes engine stack
frames; the spec hook reference then crosses the other way.

**Recommendation: all three keep pub (forced); place `FrameRole` in
`core::specs` (option a)** — placement by what it is *for* (the hook an author
implements), the same grounds as `ArgumentParser`-beside-its-implementations.
**Framework loss under pub(crate):** correct tracebacks from custom parsers
(`with_frame`) and customized frame wording (`stack_frame_title`) — both real
polish surfaces for user-facing tools.

### G8. Misc singletons (4 items)

| Item | What it is / verified state | Forced pub? | Recommendation |
|---|---|---|---|
| `NodeExtTypes` (trait, state/lang.rs:39) | The bundle of extension-slot types a `Lang` declares for node payloads. P4 reshapes it (8 assoc types → 3: `NodeExt`/`ArgumentExt`/`SlotExt`) but keeps the trait as the `Lang::NodeExts` bound (lang.rs:166). | **Yes** — bound of a `Lang` associated type; every `Lang` implementor names it. | Keep pub (hub). Nothing to decide beyond confirming; the P4 reshape is application work. |
| `DeriveError` (struct, state/parsing_state.rs:287) | The error of deriving a new parsing state from a delta — owns the failure list, a recovered state, and the delta (recovery payload by design). T1/T2-E4 additionally folds fallible `finalize_transition` into it. | **Yes** — return of `ParsingState::derived()` (parsing_state.rs:119), P2's blessed embedder idiom; three public fields. | Keep pub (hub). |
| `ParsedArgumentNodes` (struct, spec/structure.rs:47) | What an argument parser returns for one provided argument: the staged node ids + which of them are the argument's *content*. P4 adds the `ext` field (parser-minted `ArgumentExt`). | **Yes** — return of `ArgumentParser::parse_argument` (structure.rs:107+), a trait third parties implement. | Keep pub. **Placement flag:** the trait moved to `core::constructs` (T3 sweep); recommend its output record moves WITH it (a parsing-contract type, same grounds), not to `core::specs`. |
| `VERSION` ◆ (const, lib.rs:168) | `env!("CARGO_PKG_VERSION")` — techy's own version string at runtime. Zero internal uses. | No. | **Keep pub** (crate root — the one root item; a const is not a path-topology concern). The concrete consumer is the bindings story: a Python module reporting the underlying techy version has no other runtime source for it (a dependent crate's `env!` reads its OWN version). Cost of keeping ≈ zero; contract trivially stable forever. `pub(crate)` defensible if the user prefers a spartan root, reversible. |

---

## 3. Riders (accumulated from earlier sessions)

**R1 — `NoResolver` (lean keep).** Unit struct implementing `SourceResolver` by
always failing ("resolution not available", source/resolver.rs:195). Its original
role — `Language`'s default resolver (engine/language.rs:74–85) — is *gone* under
T4-B1: drivers store `Option<Arc<dyn SourceResolver>>` and `None` is the ruled
"this language resolves nothing" default. What remains: an explicit inert value
for slots that demand a resolver object, and a deterministic always-fail resolver
in tests of failure paths. Honest counterpoint: an empty `MapResolver` fails
every reference too (different message), and `None` is now the canonical
spelling of "no resolution" — so the type is near-redundant. T4 recorded "lean
keep" twice (T4_BRIEF B1; T4_RULINGS sweep). **Recommendation: keep pub** in
`techy::source` per the recorded lean — a five-line unit struct with a contract
that cannot rot; demotion is the reversible direction if the user prefers the
minimal surface. Framework loss if demoted: none of substance (workarounds are
one line); the cost would be purely the recorded-intent churn.

**R2 — `ProvenanceChain` / `ResolvedContent` placements.** Both resolve
trivially under C5 + T4: they are forced pub and `techy::source` is their fixed
home. `ProvenanceChain` (source/source.rs:331) is the iterator returned by the
public `Source::provenance_chain()` (source.rs:145) — and the T4-ruled
`Source::including_sources()` walks the same chain, making the provenance-walk
surface *more* load-bearing, not less. `ResolvedContent` (source/resolver.rs:115,
two public fields) is the return type of `SourceResolver::resolve` — the trait
every embedder resolver implements. **Recommendation: keep both pub in
`techy::source`; nothing else to decide** (the old "root vs module" placement
question dissolved with the root layer itself).

**R3 — free `resolve_source` now canonical (confirm).** INVENTORY oddity 3
called the free fn redundant with `Language::resolve_source`; T4-B2/B4 removed
the Language method (surface collapse) and made the free fn (source/resolver.rs:100
— resolve + mint the `Source` + stamp per-include-site provenance) the one
canonical composition, called between the driver's resolver accessor and the
`parse_attached_source` door. **Recommendation: confirm keep pub, home
`techy::source`** — this is a consequence of a frozen ruling, listed so the
sweep table has no dangling row.

**R4 — `StdParseDriver` test-carrier docs (doc obligation, not a fate).** The
fate is ruled (keep pub, `default()` removed — T3-A+F). The rider is the T3-B
docs clause: the driver's rustdoc must say which to reach for —
`ScopesResolvingDriver` for actual scope-stack resolution, `StdParseDriver` as
the inert test/prototype carrier ("resolves nothing"), pairing naturally with
`TrivialLang`. **Recommendation: confirm the sentence lands with the Phase-3
application** (checklist item; no new decision).

**R5 — `Diagnostics::into_vec` (lean reject; the method does not exist).** T5
I-17 routed this walkthrough wish here so the disposal is recorded: extracting
owned diagnostics is `iter().cloned().collect()` over a length-known iterator,
and `sorted_by_position()` (T1/T2-E6) covers the ordered-extraction case.
**Recommendation: reject — do not add.** Additive later if a real consumer
demonstrates need.

---

## 4. Full sweep table — all 76 items

Disposition key: **ruled** (pointer) · **G#** (decision group, with the brief's
recommendation: keep = pub-and-stable, crate = pub(crate)) · **R#** (rider).
Homes are the C5 modules; "—" = follows its group's stated home.

| # | Item (module) | Disposition | Recommendation / pointer |
|---|---|---|---|
| 1 | `source::SourceResolver` * | ruled | keep — T4-C/B1 (IS the FS trait); `techy::source` |
| 2 | `source::NoResolver` | R1 | keep (lean, recorded) |
| 3 | `source::ProvenanceChain` * | R2 | keep (forced: `provenance_chain()`) |
| 4 | `source::ResolvedContent` | R2 | keep (forced: `SourceResolver::resolve`) |
| 5 | `source::resolve_source` | ruled + R3 | keep — T4-B2/B4 canonical |
| 6 | `error::DiagnosticData` | G4 | keep (forced: `Diagnostic::data`) |
| 7 | `error::DiagnosticValue` | G4 | keep (forced: `serializable_data`) |
| 8 | `error::DiagnosticInfo` (derive) | G4 | keep (framework condition-defining) |
| 9 | `error::ToDiagnosticValue` (trait) | G4 | keep (forced: payload-field contract) |
| 10 | `error::ToDiagnosticValue` (derive) | G4 | keep |
| 11 | `token::StdTokenReader` * | G2 | keep (std impl of public `TokenReader`) |
| 12 | `token::PrefixTable` | G2 | keep (forced: `ParsingState::prefix_table`) |
| 13 | `token::TokenError` | G2 | keep (forced: `TokenReader` returns) |
| 14 | `token::TokenErrorKind` | G2 | keep (forced: `kind()`, `from_token_error`) |
| 15 | `token::TokenRecovery` | G2 | keep (forced: `recovery()`) |
| 16 | `token::EndOfStreamAfterEscape` | G3 | keep (slate: `core.token.end-of-stream-after-escape`) |
| 17 | `token::ForbiddenChar` | G3 | keep (slate: `core.token.forbidden-char`) |
| 18 | `token::skip_whitespace` | G2 ◆ | keep (paragraph-rule primitive; weakest keep) |
| 19 | `state::SimpleLang` * | ruled | keep as **`TrivialLang`** — T3-A |
| 20 | `state::NodeExtTypes` | G8 | keep (forced: `Lang::NodeExts` bound; P4 reshape) |
| 21 | `state::DeriveError` | G8 | keep (forced: `derived()` return) |
| 22 | `spec::ArgumentParser` * | ruled | keep — T3 sweep; `core::constructs` |
| 23 | `spec::ParsedArgumentNodes` | G8 | keep (forced); placement flag → constructs |
| 24 | `spec::FrameRole` | G7 | keep (forced); placement flag → specs |
| 25 | `scopes::Scope` | G1 ◆ | keep (runtime-definitions carrier) |
| 26 | `scopes::FallbackProvider` | G1 ◆ | keep (unknown-name policy) |
| 27 | `scopes::ErrorCallableSpec` | G1 ◆ | keep (produces slate-frozen condition) |
| 28 | `scopes::CallableDefinedAsError` | G3 | keep (slate: `core.specs.callable-defined-as-error`) |
| 29 | `scopes::CallableQuery` | ruled | keep — T3-H; `core::specs` |
| 30 | `scopes::CallableSyntax` | ruled | keep — T3-H |
| 31 | `scopes::SymbolEntry` | G1 | keep (forced: `iter_symbols`; did-you-mean substrate) |
| 32 | `scopes::SearchedProviders` | ruled | keep — T3-H |
| 33 | `scopes::DefinitionOp` | G1 | keep (forced: `with_definitions`) |
| 34 | `scopes::ScopeOp` | G1 | keep (forced: delta field + `apply_op`) |
| 35 | `scopes::ScopeOpError` | G1 | keep (forced: `apply_op` + `DeriveError.failures`) |
| 36 | `scopes::ScopeStackError` | G1 | keep (forced: stack lookup + `ScopeOpError::Provider`) |
| 37 | `scopes::ProviderError` | G1 | keep (forced: `retrieve_spec`) |
| 38 | `node::NodeData` | G5 ◆ | **pub(crate)** (zero public signatures) |
| 39 | `node::NodeId` * | ruled | keep — P4 tree tags; FFI handle |
| 40 | `node::NodeSliceIter` * | G5 | keep (forced: `NodeSlice::iter`) |
| 41 | `node::GroupData` | G5 | keep (forced: `NodeKind::Group` payload) |
| 42 | `node::ParsedSlots` * | ruled | keep — T3 wish 22 (`new`) |
| 43 | `node::StagedNodes` | G5 | keep (P4 read view `cx.staged_nodes()`) |
| 44 | `node::StagedNodeView` | G5 | keep (forced: `StopSpec.node` predicate) |
| 45 | `node::NodeBuildError` | G5 | keep (forced: builder `Result`s) |
| 46 | `node::check_tree_invariants` | G5 ◆ | **pub(crate)** once `validate_tree` lands (T5-F) |
| 47 | `constructs::NodesParser` | G6 ◆ | keep (factory-seam composition unit) |
| 48 | `constructs::NodesOutcome` | G6 | keep (forced: `parse_nodes` return) |
| 49 | `constructs::GroupParser` | G6 ◆ | keep (factory-seam composition unit) |
| 50 | `constructs::StopSpec` | G6 | keep (forced: `parse_nodes` param) |
| 51 | `constructs::StopCause` | G6 | keep (forced: `NodesOutcome.stop`) |
| 52 | `constructs::TokenStopCondition` | G6 | keep (forced: `StopSpec.token`) |
| 53 | `constructs::TokenStopKind` | G6 | keep (forced: `TokenStopCondition.kind`) |
| 54 | `constructs::ChildStateSpec` | G6 | keep (forced: `parse_nodes` param) |
| 55 | `constructs::GroupChildState` | G6 | keep (forced: `ChildStateSpec.group`) |
| 56 | `constructs::InvocationChildState` | G6 | keep (forced: `ChildStateSpec.invocation`) |
| 57 | `constructs::CommandResolutionFailed` | G3 | keep (slate: `core.specs.command-resolution-failed`) |
| 58 | `constructs::UnclosedGroup` | G3 | keep (slate: `core.groups.unclosed-group`) |
| 59 | `constructs::UnclosedGroupFound` | G3 | keep (forced: condition payload) |
| 60 | `constructs::StrayGroupClose` | G3 | keep (slate: `core.groups.stray-group-close`) |
| 61 | `constructs::ExpectedExpressionArgument` | G3 | keep (slate: `core.arguments.expected-expression-argument`) |
| 62 | `constructs::ExpressionCallableRequiresContent` | G3 | keep (slate: `core.arguments.expression-callable-requires-content`) |
| 63 | `constructs::MissingEnvironmentTerminator` | G3 | keep (slate: `core.environments.missing-terminator`) |
| 64 | `constructs::MissingTerminatorFound` | G3 | keep (forced: condition payload) |
| 65 | `constructs::EnvironmentTerminatorMismatch` | G3 | keep (slate: `core.environments.terminator-mismatch`) |
| 66 | `constructs::MalformedEnvironmentTerminator` | G3 | keep (slate: `core.environments.malformed-terminator`) |
| 67 | `constructs::ScopeOpFailed` | G3 | keep (slate: `core.specs.scope-op-failed`) |
| 68 | `constructs::UnusableRecoveryToken` | G3 | keep (slate: `core.recovery.unusable-recovery-token`) |
| 69 | `constructs::UnusableRecoveryTokenKind` | G3 | keep (forced: condition payload) |
| 70 | `constructs::ImplementationError` | G3 | keep (slate: `core.constructs.implementation-error`; panic-policy channel) |
| 71 | `engine::ParserSession` * | ruled | keep type; `builder` field pub(crate) — P4 |
| 72 | `engine::StdParseDriver` * | ruled + R4 | keep — T3-A+F; test-carrier docs |
| 73 | `engine::ResolvedCallable` | ruled | keep — T3-H; `core::specs` |
| 74 | `engine::Frame` | G7 | keep (forced: `with_frame`) |
| 75 | `engine::FrameTitle` | G7 | keep (forced: `Frame.title`) |
| 76 | crate root `VERSION` | G8 ◆ | keep (bindings version reporting; ~zero cost) |

(* = SYNTHESIS's ten starred items — implicit-use caveats. Companion item not in
the 76 but rationalized in G2: `token::PrefixEntry`, forced pub, lives beside
`PrefixTable`; the old root/module inconsistency dissolves under C5. Non-item
rider: R5 `Diagnostics::into_vec` — reject, never existed.)

**Tally of recommendations:** 74 keep (12 already ruled + 62 recommended keep,
of which ~50 forced or doctrine-bound) · 2 pub(crate) (`NodeData`,
`check_tree_invariants`) · 1 rejection of a never-added method (R5). The
headline shape: the "no usage signal" list is overwhelmingly the *signature
closure* of the used API plus the ruled diagnostics architecture — the untouched
half of the crate is load-bearing, just never spelled out in consumer code.

---

## 5. Session logistics (proposed round order)

Interim rulings file `TIERC_RULINGS.md`, updated every round (established
pattern).

1. **Round 1 — the forced-pub confirmation block** (G1 forced six, G2 forced
   four, G6 forced eight, G7, G8 minus VERSION): one confirmation sweep; each
   item's demotion would require demoting a used signature, so these are
   ratifications unless the user wants to reopen a signature.
2. **Round 2 — conditions doctrine** (G3, 17 items + G4, 5 items): confirm the
   keep-all consequence of the frozen slate + typed matching + F9 implementors
   page; rule the two derive re-exports explicitly.
3. **Round 3 — the genuine judgment calls** (◆): `Scope`/`FallbackProvider`/
   `ErrorCallableSpec` (G1); `StdTokenReader`/`skip_whitespace` (G2);
   `NodesParser`/`GroupParser` (G6); `NodeData`/`check_tree_invariants` (G5);
   `VERSION` (G8).
4. **Round 4 — placement flags**: `FrameRole` (specs vs hub);
   `ParsedArgumentNodes` (constructs-with-its-trait vs specs).
5. **Round 5 — riders R1–R5** (quick: one lean keep, two forced-keep
   confirmations, one doc obligation, one rejection-for-the-record).
6. **Sweep** — walk the §4 table for completeness; durable records: one
   DESIGN_RATIONALE entry for the batch outcome (or amendment notes on
   [§dd-dr:public-namespace-topology] + [§dd-dr:stability-rubric]), a
   superseded-names check (none proposed here), PLAN.md decision-log line,
   Phase-3 checklist additions (`NodeData`/`check_tree_invariants` demotions
   ride the `validate_tree` commit; R4 doc sentence; doc-link updates for
   demoted items).
