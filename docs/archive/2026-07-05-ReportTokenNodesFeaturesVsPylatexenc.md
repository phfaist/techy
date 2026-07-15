# techy tokens/nodes vs. pylatexenc — feature and pattern analysis

**Status: ANALYSIS REPORT, July 2026 — written after Phase 5.**
Compares the implemented techy architecture (Phases 1–5: `source`/`error`, `token`, `state`,
`spec`+`library`, `node`) against pylatexenc 3's `latexnodes` + `macrospec` modules (checked
against the sources at `~/Research/util/pylatexenc`). Focus, per the request: (1)
extensibility, (2) ease of writing future construct parsers, (3) suitability as a basis for
FLM. Items already earmarked for Phase 6 in `PreliminaryPlanNotesArchitectureExecutionPhase6.md`
are cross-referenced as **[→ Phase 6 Qn/Cn]** rather than re-proposed.

---

## 1. Executive summary

The techy architecture **covers every load-bearing pattern of pylatexenc's parsing engine**,
usually in a strictly more principled form: reified state deltas generalize pylatexenc's
delta-class zoo, the de-keyed `CallableSpec` + `CallableTypeId` generalizes the
macro/environment/specials triplet, span-based `TextContent` + mandatory `SourceSpan`s
strictly dominate `pos`/`pos_end` + `latex_verbatim()`, and tolerant parsing is first-class
rather than exception-filtering. Nothing in pylatexenc's *engine* is architecturally out of
reach.

The genuine gaps are not in the engine but in pylatexenc's **convenience layer** — the parts
FLM leans on hardest and that make writing specs and construct parsers pleasant:

1. **Per-argument custom parsers and per-argument/per-body parsing-state deltas**
   (`LatexArgumentSpec(parser=…, parsing_state_delta=…)`,
   `make_body_parsing_state_delta`). techy's `ArgumentKind` is currently a closed data enum
   with no parser or state-delta slot. This is the single most important pylatexenc
   capability not yet reflected in techy's spec surface. (§5.1, §5.2)
2. **The spec-level `finalize_node` hook** — how FLM attaches `flm_specinfo` to nodes
   without writing a full custom parser. techy has the storage (`CallableNodeExt`) but no
   hook by which the *default* declarative parser populates it. (§5.3)
3. **Argument/content extraction utilities** (`ParsedArgumentsInfo`,
   `get_content_as_chars()`, keyval/comma-list parsing, `LatexNodeList.split_at_chars()`).
   Small, boring, and used on nearly every line of FLM feature code. (§5.4)
4. **Spec-construction terseness** (`std_macro('frac', '{{')`-style shorthands) and
   **library introspection** (category filtering, spec iteration). (§5.5, §5.6)

None of these require architectural change — they are additive API surface, and several are
already sketched in the Phase 6 notes. The recommendation is to treat §5.1–§5.3 as Phase 6
design obligations (they shape `ArgumentSpec`/`SlotSpec`/`CallableSpec` signatures, which are
being pinned down now) and §5.4–§5.7 as a deliberate "FLM ergonomics" work package in
Phase 7/8, validated by the FLM spike.

On the three focus questions:

- **Extensibility: parity or better.** Every pylatexenc extension point has a techy
  counterpart, most of them structurally safer (§3). The one *pattern* to protect during
  Phase 6: pylatexenc lets extensibility in at **every** granularity — per-argument, per-body,
  per-invocation, per-lookup, per-tokenizer — and techy currently has the coarse and fine ends
  but a gap in the middle (per-argument/per-slot).
- **Ease of writing construct parsers: comparable, likely better once Phase 6 lands.**
  techy's `ParseContext` removes pylatexenc's three-argument threading; the builder and
  layout types add ceremony but the compiler enforces the protocol pylatexenc documents in
  prose (§4). The risk is not difficulty but *volume of required correctness* (spans,
  layouts, post-space) — mitigated exactly by making the declarative path cover more cases
  so custom parsers stay rare.
- **FLM readiness: architecture fits; ergonomics need the §5 items.** FLM's needs map onto
  `StateExt`/`Event`, `CallableNodeExt`, custom invocation parsers, `SourceResolver`, and
  frozen-tree post-processing (§6). What FLM would miss *today* is precisely the convenience
  layer above.

---

## 2. What was compared

| side | scope |
|---|---|
| pylatexenc 3 | `latexnodes` (`_token`, `_tokenreader`, `_parsingstate`, `_parsingstatedelta`, `_callablespecbase`, `_latexcontextdbbase`, `_walkerbase`, `_nodescollector`, `nodes`, `_parsedargs`, `_parsedargsinfo`, `_latex_recomposer`, `parsers/*`) and `macrospec` (`_specclasses`, `_latexcontextdb`, `_macrocallparser`, `_argumentsparser`, `_environmentbodyparser`) |
| techy | Phases 1–5 as implemented (`source`, `error`, `token`, `state`, `spec`, `library`, `node`), plus the *designed* Phase 6 surface (ARCHITECTURE.md §constructs/§engine and the Phase 6 preliminary notes) where explicitly marked |

Where a techy capability is **designed but unimplemented**, it is marked *(Phase 6)* /
*(Phase 7)* — the report distinguishes "deferred by plan" from "not accounted for".

---

## 3. Capability map

Legend: ✅ implemented · 🔵 designed, lands Phase 6/7 · 🟡 partially covered / needs a decision · ❌ no counterpart yet.

| capability | pylatexenc | techy | status |
|---|---|---|---|
| Zero-ish-copy tokens with pre/post-space | `LatexToken` (`pos`/`pos_end`/`pre_space`/`post_space`) | `Token<'s,L>` with `Span` + `pre_space`; post-space per kind | ✅ better (typed kinds, true zero-copy) |
| Environment recognition | at token level (`begin_environment` tok, needs context db in tokenizer) | at parse time (preset `\begin` spec) | ✅ better (tokens stay structural) |
| Custom tokenization | `LatexTokenReader` subclass; `LatexTokenListTokenReader` | `TokenReader<L>` trait; no token-list reader yet | ✅ / ❌ list-reader (trivial, add when needed — §5.7) |
| Parsing state | `ParsingState`, ~20 fields, math mode privileged | `ParsingState<L>` = `TokenRules` + `LibraryStack` + `L::StateExt`; no privileged modes | ✅ better |
| State change | `sub_context(**kwargs)` + `ParsingStateDelta` subclasses (`EnterMathMode`, `ExtendLatexContextDb`, `ReplaceParsingState`, `Chained`, walker events) | one `ParsingStateDelta<L>` value (overrides + `push_libraries` + `ext` + `events`) + `Lang::finalize_transition` | ✅ better (one mergeable value; the walker-event handler generalized to `finalize_transition`) |
| Definitions db | `LatexContextDb`: categories, `extended_with`, `filtered_context`, unknown-spec fallbacks, `test_for_specials` | `Library`/`LibraryStack`/`SpecLookup`: named libraries, lexical shadowing, per-`CallableTypeId` fallbacks, mode-aware lookup via state | ✅ core; 🟡 introspection/iteration missing (§5.6) |
| Spec → parser escape hatch | `CallableSpecBase.get_node_parser(token)` — *the* extension point | `CallableSpec::invocation_parser()` / `parse_invocation` | 🔵 [→ Phase 6 Q2] — load-bearing, correctly prioritized |
| Per-argument parser | `LatexArgumentSpec(parser=<any parser or shorthand>)` | closed `ArgumentKind` enum | ❌ **top gap** (§5.1) |
| Per-argument state delta | `LatexArgumentSpec(parsing_state_delta=…)` | none | ❌ (§5.2) |
| Body/slot state delta | `spec.make_body_parsing_state_delta` | none on `SlotSpec` | ❌ (§5.2) |
| After-invocation delta (`\newcommand`) | `spec.make_after_parsing_state_delta` | parser returns `Option<ParsingStateDelta>`; caller scopes it | ✅ better (producer/scope split) |
| Post-parse node hook | `spec.finalize_node(node)` | none (ext storage exists, no population hook) | ❌ (§5.3) |
| Node model | 7 class kinds (incl. `LatexMathNode`), dynamic attrs, `parsing_state`/`latex_walker` refs | closed 5-kind `NodeKind<L>`, two-tier typed ext, `Arc<ParsingState>` per node | ✅ better (typed; math de-privileged with preset sugar planned) |
| Parsed arguments record | `ParsedArguments` + `argnlist` (None for absent) | `ArgsLayout`/`SlotsLayout` + one-node-per-region children | ✅ (+ records syntax for recomposition, which pylatexenc lacks) |
| Argument info helpers | `ParsedArgumentsInfo`, `SingleParsedArgumentInfo` (`get_content_as_chars`, keyval) | `NodeRef::argument(i)` only | ❌ (§5.4) |
| Node list utilities | `LatexNodeList.filter/split_at_node/split_at_chars/parse_keyval_content/get_content_as_chars` | `NodeRef::children()` iterator | 🟡 (§5.4) |
| Visitor | `LatexNodesVisitor` (per-kind methods) | deliberately post-Phase-6 | 🔵 (planned; exhaustive `match` already covers ad-hoc walks) |
| Verbatim recomposition | `latex_verbatim()` (requires contiguous parse, breaks on transformed trees) | `NodeRef::span_content()` — works detached/mixed-origin | ✅ better |
| Reconstructing recomposition | `LatexNodesLatexRecomposer` (guesses delimiters where not recorded) | level-2 recomposition: syntax *recorded* in layouts | 🔵 [→ Phase 6 Q3/Q4] — design is stronger ("reproduce, don't guess") |
| Tolerant parsing | `tolerant_parsing` flag, `check_tolerant_parsing_ignore_error`, exceptions as control flow | `Recovery` policy + recovery tokens + `Diagnostics` on `ParseResult` | ✅ better |
| Serialization | `to_json_object` on nodes/state/args | none (zero-dep policy) | 🟡 (§5.8) |
| Line/col + provenance | `pos_to_lineno_colno` | `LineIndex`, `SourceProvenance` chains, `format_traceback` | ✅ better |
| Stop conditions | `stop_token_condition` / `stop_nodelist_condition` closures + `handle_stop_condition_token` | reified `StopConditionSpec` data | 🔵 [→ Phase 6 Q1] — see caveat §4.3 |

---

## 4. Ease of writing construct parsers

### 4.1 What a pylatexenc construct parser looks like

The pylatexenc contract is small and this is its greatest strength:

```python
class MyConstructParser(LatexParserBase):
    def parse(self, latex_walker, token_reader, parsing_state, **kwargs):
        tok = token_reader.next_token(parsing_state=parsing_state)
        ...
        inner_nodes, delta = latex_walker.parse_content(
            LatexGeneralNodesParser(stop_token_condition=...),
            token_reader, my_sub_state)
        node = latex_walker.make_node(LatexMacroNode, macroname=..., pos=..., pos_end=...,
                                      parsing_state=parsing_state, ...)
        return node, carryover_info
```

Four properties make this easy: (a) **re-entrancy** — any parser can invoke any other,
including the general nodes parser, with a modified state; (b) **the walker builds nodes**
(`make_node`) so parsers never manage storage; (c) **stop conditions** let a parser reuse
`LatexGeneralNodesParser` for "content until X" instead of writing loops; (d) Python lets you
skip everything you don't care about (no spans required, attach attributes freely).

### 4.2 The projected techy equivalent

Against the Phase 6 design (`ParseContext`, `ConstructParser`, staging builder):

```rust
impl<L: Lang> ConstructParser<L> for MyConstructParser {
    type Output = BuildId;
    fn parse(&self, cx: &mut ParseContext<'_, '_, '_, L>)
        -> ParseOutcome<(BuildId, Option<ParsingStateDelta<L>>)>
    {
        let tok = cx.tokens.next(&cx.state)?;
        let sub_state = cx.state.derived(&delta);          // inward scoping, structural revert
        let (children, delta) = NodesParser::until(stop).parse(&mut cx.with_state(sub_state))?;
        let id = cx.session.builder.add(NodeKind::…, span, cx.state.clone(), children);
        Ok((id, None))
    }
}
```

Point by point against 4.1: (a) re-entrancy is preserved and *simplified* — one `cx`
instead of three threaded arguments; (b) the session's builder plays `make_node`, with
cycle-freedom and single-parent enforced instead of documented; (c) stop conditions become
data [→ Phase 6 Q1]; (d) is where Rust charges its toll — spans, layouts, and post-space are
**obligations**, not options, because the span-partition invariant and level-2 recomposition
are stated contracts pylatexenc never made (`latex_verbatim()` silently breaks on transformed
trees; the recomposer guesses delimiters).

**Assessment.** Writing a techy construct parser will be *comparably easy in structure* and
*harder in required precision*. That is the intended trade (the precision is what buys
trustworthy span math and recomposition), but it has a practical consequence worth stating:
**the declarative path must cover more ground than pylatexenc's did**, because "just write a
little custom parser" costs more here. The Phase 6 notes already lean the right way
(declarative slot terminators, Q1 Option A; pylatexenc-parity argument acceptance, Q3
Option A). §5.1–§5.3 below extend the same logic to the remaining spec hooks.

Two further mitigations worth adopting:

- **Span/invariant test helpers as public-ish test utilities** — a `check_tree_invariants()`
  that mechanically verifies the partition invariant, layout/children consistency, and
  `TextContent::Spanned` residency (the Phase 6 notes plan this for techy's own tests, §G;
  exposing it lets construct-parser authors — FLM included — get the same guarantee for one
  assert).
- **A `ParseContext::with_state`-style scoped-state helper** (as sketched above), so the
  ubiquitous "derive a child state, parse, structurally revert" dance is one expression and
  the `Arc` discipline is not something every parser author re-derives.

### 4.3 Stop conditions: one expressiveness caveat

pylatexenc's stop conditions are two *closures* — `stop_token_condition(token)` and
`stop_nodelist_condition(nodelist)` — plus `handle_stop_condition_token`. Reifying these as
`StopConditionSpec` data [→ Phase 6 Q1] is the right default (inspectable, recomposable,
deterministic), but note what the closures could express that the sketched data cannot:
node-count/nodelist-shaped conditions (`LatexSingleNodeParser` stops after one non-comment
node), and ad-hoc token predicates (`LatexCharsGroupParser`'s "stop at any non-chars token").
These have a home — `ExpressionParser` covers the first; the custom-parser escape hatch
covers the rest — but the Phase 6 plan should say so explicitly, so nobody tries to grow
`StopConditionSpec` into a closure language. Recommendation: keep `StopConditionSpec`
minimal, and let `NodesParser` *also* accept a programmatic stop (a `dyn Fn(&Token)`-shaped
parameter available only to parser code, never stored in specs — specs stay data).

---

## 5. Convenient pylatexenc aspects to replicate (prioritized)

### 5.1 Per-argument custom parsers — **top priority, shapes Phase 6 API**

In pylatexenc, an argument *is* a parser: `LatexArgumentSpec(parser, argname,
parsing_state_delta)`, where `parser` is either a shorthand string (`'{'`, `'['`, `'*'`,
`'v'`) or **any `LatexParserBase` instance**. This is how the ecosystem gets, with zero
engine changes: chars-only arguments (`LatexCharsGroupParser` — `\label{...}`),
comma-separated lists (`LatexCharsCommaSeparatedListParser` — `\cite{a,b}`), verbatim
arguments (`\verb`), and FLM's bespoke argument types. It is pylatexenc's *mid-granularity*
extension point: much cheaper than taking over the whole invocation, much more powerful than
picking from a fixed enum.

techy's `ArgumentKind` is a closed enum (`Mandatory`/`Optional`/`Star`, growing in Phase 6).
A closed *starter* inventory is fine; a closed *architecture* here would be a real
regression — the full-takeover `parse_invocation` hatch technically substitutes, but then
one custom argument means hand-writing the whole invocation (all other arguments, layouts,
post-space), which is exactly the expensive path §4.2 says to keep rare.

**Recommendation:** give `ArgumentKind` (or `ArgumentSpec`) a custom variant carrying
`Arc<dyn ConstructParser<L, Output = …>>`. This is consistent with the decided design
vocabulary — "specs carry their invocation parser" is already core law, and parsers are the
sanctioned *behavior* extension point (DESIGN_RATIONALE §2.1); extending that from
whole-invocation to per-argument granularity introduces no new concept. It does make
`ArgumentSpec` generic over `L` (today it is `L`-free data) — worth the cost; pylatexenc's
whole argument ecosystem hangs off this slot. If Phase 6 wants to defer the *implementation*,
reserving the variant (or making `ArgumentKind` non_exhaustive with the slot's type worked
out) is the minimum: the decision affects `PartialEq`/`Debug` derives and the `StdCallableSpec`
type, which are being frozen now.

### 5.2 Per-argument and per-slot parsing-state deltas

Two pylatexenc hooks with no techy counterpart yet:

- `LatexArgumentSpec.parsing_state_delta` — parse *this argument* under a modified state.
  Canonical uses: `\text{...}` leaving math mode for its argument; `\href`'s URL argument
  disabling specials; FLM uses this pervasively.
- `CallableSpec.make_body_parsing_state_delta` — parse the *body* under a modified state.
  Canonical uses: verbatim environments, math environments (`\begin{align}` body in math
  mode), FLM's block-level environments.

techy has all the machinery (deltas, `derived()`, structural reversion) but no *declarative
slot* for "this region parses under delta D". Without it, every state-scoped argument or body
needs a custom parser — same argument as §5.1.

**Recommendation:** `ArgumentSpec` and `SlotSpec` each get an optional
`parsing_state_delta: Option<ParsingStateDelta<L>>` (applied via `derived()` around that
region, reverted structurally). Note this also makes `SlotSpec` generic over `L`, and note
the interaction with [→ Phase 6 Q1]: a slot's terminator must be scanned under the *outer*
state or the *inner* state depending on the construct (verbatim bodies must recognize
`\end{verbatim}` while everything else is disabled — pylatexenc solves this inside its
verbatim parsers). The declarative form should define which state the terminator scan uses
(inner state, with the terminator's trigger syntax guaranteed recognizable, is the behavior
that matches LaTeX practice).

### 5.3 A hook for populating node ext from the default parser (`finalize_node`)

pylatexenc's `CallableSpec.finalize_node(node)` runs after the standard invocation parser
builds the node, and may adjust or annotate it. **This is FLM's main attachment point**: FLM
specs attach `flm_specinfo` (and derived flags like block-levelness) to every node this way,
without writing custom parsers.

techy's two-tier ext system is the *storage* answer (typed, better than dynamic attributes),
but there is currently no way for a spec to *populate* `CallableNodeExt` when the **default**
declarative parser builds the node — `NodeTreeBuilder::add` uses `Default`. Overriding
`parse_invocation` for this is, again, the too-big hammer.

**Recommendation:** as part of [→ Phase 6 Q2], give `CallableSpec` a defaulted finalize hook
that the standard invocation path calls with the assembled invocation facts before staging,
e.g. `fn make_node_ext(&self, invocation: &Invocation<L>, args: &ArgsLayout, …) ->
CallableNodeExt<L>` (or a `finish_invocation(&self, data: &mut CallableData<L>)` over the
not-yet-staged data). Data-shape details are Phase 6's to settle; the requirement to record
is: *spec-driven ext population must not require a custom parser*. This single hook is what
makes `CallableNodeExt` reachable for the 95% of FLM constructs that are declaratively
parseable.

### 5.4 Content-extraction utilities (the FLM workhorses)

Used constantly by FLM feature code and by any real consumer:

- `SingleParsedArgumentInfo.get_content_as_chars()` — argument → plain string, erroring if
  the content isn't just chars/comments (e.g. `\label`, `\cite` keys, keyval values).
- `get_content_nodelist(unwrap_double_group=…)` — argument → node list, unwrapping the
  delimiting group.
- `parse_content_as_keyval()` / `LatexNodeList.parse_keyval_content()` — `key=value` pairs.
- `LatexNodeList.split_at_chars(sep)` — split content at `,`/`&`/`\\`-like separators
  (tabular cells, citation lists); `split_at_node`, `filter`.
- `ParsedArgumentsInfo.get_all_arguments_info(...)` — by-name argument access (techy's
  `ArgumentSpec.name` field already anticipates this).

None of this is architectural; all of it belongs as `NodeRef` methods / small helper types
(a `NodeRef::argument_info(i)` returning a `SingleArgumentInfo`-like view; iterator adapters
over `children()` for split/filter). **Recommendation:** collect these as an explicit
work package — Phase 7 at the earliest (they need parsed trees to test against), validated
against real FLM call sites in the Phase 8 spike. Keyval/comma-splitting may alternatively
be argument *parsers* (§5.1) rather than post-hoc utilities — pylatexenc offers both; decide
per construct, but ensure at least one of the two paths exists for each pattern.

### 5.5 Spec-construction ergonomics

pylatexenc definitions are one-liners: `std_macro('frac', '{{')`,
`std_environment('enumerate', '[', is_math_mode=False)`. techy currently requires assembling
`StdCallableSpec { arguments: ArgumentStructureSpec { arguments: vec![ArgumentSpec::new(
ArgumentKind::Mandatory { group_type: GT_BRACE })] }, … }`. The plan already assigns
`MacroSpec`/`EnvironmentSpec`/`SpecialsSpec` constructor helpers to the latexlike preset
(Phase 7) — good; the recommendation is only to hold that work to the one-liner bar
(builder-style: `MacroSpec::new("frac").arg(mandatory()).arg(mandatory())`, and library
population macros or bulk-insert helpers for the standard library), and to port a *terse
core* (`ArgumentStructureSpec::parse("{{")`-style shorthand is probably not worth it in
typed Rust — builders reach the same terseness without stringly typing).

### 5.6 Library introspection

`LatexContextDb` supports iterating specs (`iter_macro_specs`), listing `categories()`, and
deriving filtered contexts (`filtered_context(keep_categories=…)`) — used by tooling, docs
generation, and FLM's feature composition (each feature contributes a category; a document
class selects features). techy's `Library` exposes `get`/`insert`/`len` only.

**Recommendation (low cost, non-urgent):** add iteration (`Library::iter() → (CallableTypeId,
&str, &Arc<dyn CallableSpec>)`) and note that pylatexenc's *category* layer maps to "one
`Library` per feature, composed in a `LibraryStack`" — which techy already does better
(ordered, shadowing, mid-parse pushable). Filtering = building a new stack from selected
libraries; no new mechanism needed, but a line of documentation should say this is *the*
intended replacement for categories, so FLM's feature system is designed against it.

### 5.7 Small paritems

- **`LatexTokenListTokenReader`** (a `TokenReader` over a pre-built token vector): trivial to
  add when first needed (testing construct parsers in isolation; re-parsing recorded token
  runs). Worth having for Phase 6's parser unit tests.
- **`LatexExpressionParser`** parity — already planned (Phase 6 standard parsers). Its
  pylatexenc niceties to keep: skipping comments/whitespace before the expression, and the
  "macro that requires arguments encountered as single-token argument" diagnostic
  (`_check_if_requires_args`).
- **`Token` → `CallableQuery` escape-char plumbing** — already identified [→ Phase 6 C2].

### 5.8 Serialization (deliberate divergence, keep an eye on it)

pylatexenc nodes/state/args all implement `to_json_object`, used by its test suite (golden
JSON trees) and downstream tooling. techy's zero-dependency policy rules out built-in serde,
and the flat `NodeTree` is serialization-friendly by construction. **Recommendation:** no
action now; when golden tests arrive (Phase 7 acceptance suite), implement a small
hand-rolled debug/JSON dump for trees (test-only), and consider an optional `serde` cargo
feature post-1.0. Record it as a known divergence, not a gap.

---

## 6. FLM fit check (goal 3)

Mapping FLM's actual usage patterns of pylatexenc onto techy:

| FLM need (as exercised in the flm codebase) | pylatexenc mechanism | techy counterpart | verdict |
|---|---|---|---|
| Math/block-level/etc. modes in state | `ParsingState` subclass (`FLMParsingState`) | `L::StateExt` + `Event` + `finalize_transition` | ✅ cleaner (typed; transition rules centralized) |
| Feature-provided definitions, composed per document class | context-db categories | one `Library` per feature in a `LibraryStack` | ✅ (document the mapping, §5.6) |
| Semantic info on nodes (`flm_specinfo`, block-level flags) | dynamic attributes via `finalize_node` | `CallableNodeExt` / uniform `NodeExt` | 🟡 storage ✅, population hook needed (§5.3) |
| Custom constructs (`\verb`-likes, tabular-ish preambles, delimited content) | `get_node_parser` override | `parse_invocation` override [→ Phase 6 Q2] | 🔵 designed |
| Mode-scoped arguments (`\text`, URL args, verbatim args) | per-argument parser + delta | — | ❌ §5.1/§5.2 — **blocking for FLM ergonomics** |
| Argument content extraction (labels, refs, keyval) | `ParsedArgumentsInfo` et al. | — | ❌ §5.4 |
| `\input`-like resolution, synthesized content | walker-level handling | `SourceResolver` + `SourceProvenance` | ✅ better (provenance chains, `triggered_at`) |
| Render pipeline over parsed trees | walks node lists, no tree mutation | `NodeRef` traversal over frozen `ParseResult` | ✅ (visitor/transform API post-Phase-6 as planned) |
| Cross-document environment reuse | one `LatexContextDb` shared | `Language<L>` owns no per-parse state | ✅ |
| Precise user-facing error positions | `pos_to_lineno_colno` | `LineIndex` + `Diagnostics` + traceback formatting | ✅ better |

Conclusion unchanged from ARCHITECTURE §6 but sharpened: **no missing capability, four
missing conveniences** (§5.1–§5.4), of which two (§5.1, §5.2) shape Phase 6 type signatures
and should be decided there rather than retrofitted, and one (§5.3) belongs to the Q2 design.
The Phase 8 FLM spike should explicitly exercise: a mode-scoped argument, a spec-populated
`CallableNodeExt`, and a `get_content_as_chars`-style extraction — the three patterns FLM
hits in its first hundred lines.

---

## 7. Where techy is ahead (for completeness)

Not requests — things the new architecture does that pylatexenc cannot, worth protecting:

1. **Detached/transformed-tree recomposition.** `latex_verbatim()` requires the original
   contiguous string and silently misbehaves on synthesized/transformed nodes; techy's
   Arc-span level 1 plus recorded-syntax level 2 are contracts.
2. **Exact sibling-span partition invariant** — pylatexenc has no such guarantee (its
   whitespace/pos conventions are close but unverified); techy states it and can test it
   mechanically.
3. **One state-transition choke point** (`derived` + `finalize_transition`) vs. pylatexenc's
   scattered `sub_context` calls + delta subclasses + a separate walker event handler for
   math mode only.
4. **Recognition=resolution for specials** (`SpecialsMatch` carries the spec) — pylatexenc's
   `test_for_specials` can disagree with later lookup.
5. **Structural safety**: single-parent/cycle-free trees, never-`None` specs, no
   `**kwargs` node construction, no dynamic attributes; tolerant parsing without
   exceptions-as-control-flow.
6. **No privileged math mode** — pylatexenc hard-codes math delimiters and
   `LatexMathNode`; techy's group-types + `StateExt` pattern is what lets FLM (or any
   non-LaTeX language) define different mode semantics without fighting the engine.

---

## 8. Summary of recommendations

| # | item | when | cost | why |
|---|---|---|---|---|
| R1 | Per-argument custom parser slot in `ArgumentKind`/`ArgumentSpec` (§5.1) | **Phase 6** (shapes frozen types) | medium | pylatexenc's mid-granularity extension point; keeps custom invocation parsers rare |
| R2 | `parsing_state_delta` on `ArgumentSpec` + `SlotSpec`; define terminator-scan state (§5.2) | **Phase 6** | small–medium | `\text`, verbatim bodies, FLM modes — declaratively |
| R3 | Spec hook to populate `CallableNodeExt` from the default parser (§5.3) | **Phase 6** (part of Q2) | small | FLM's `finalize_node` pattern; makes ext reachable without custom parsers |
| R4 | Keep `StopConditionSpec` minimal; programmatic stop param on `NodesParser` for parser code only (§4.3) | Phase 6 | small | avoids growing a closure language in spec data |
| R5 | Scoped-state helper on `ParseContext`; public tree-invariant checker (§4.2) | Phase 6 | small | parser-author ergonomics + correctness |
| R6 | Token-list `TokenReader` for parser unit tests (§5.7) | Phase 6 (tests) | trivial | isolate construct-parser tests |
| R7 | Argument/content extraction utilities on `NodeRef` (chars, nodelist, keyval, split) (§5.4) | Phase 7, validated in Phase 8 | medium | FLM workhorses; port call-site-by-call-site from flm |
| R8 | One-liner spec builders + library bulk helpers (§5.5) | Phase 7 | small | pylatexenc's definition terseness |
| R9 | `Library` iteration; document "library-per-feature ≈ category" (§5.6) | Phase 7 | trivial | tooling + FLM feature composition |
| R10 | Test-only tree dump now; optional serde feature later (§5.8) | Phase 7+ | small | golden tests; known divergence, not a gap |

The through-line: pylatexenc's engine has been fully matched or improved upon; what remains
is to match its *generosity at the middle granularities* — per-argument, per-slot, per-node —
before Phase 6 freezes the spec-facing types, and to schedule the convenience layer FLM
actually lives in.
