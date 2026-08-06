# Memory footprint of the parsing engine and the node tree

*Exploration report — no code changed. Audit of runtime (heap) memory for tiny builds
handling lots of data (wasm), covering the parse engine and the produced `NodeTree`.
Language-feature **code-size** gating was covered separately in
`GateFeaturesOptimizedLangs.md`; this report is about **bytes per document**, not bytes
of payload.*

---

## 0. Executive summary

Runtime memory is **O(nodes)**, not O(input bytes), at a fairly heavy constant:

| | 64-bit (measured) | wasm32 (derived) |
|---|---|---|
| Retained per node, tree + states | **≈ 225 B** | **≈ 120 B** |
| Markup-heavy document (11.5 input bytes/node) | **19.5 × input** | ≈ 10.4 × input |
| Prose-heavy document (105 input bytes/node) | **1.6 × input** | ≈ 0.85 × input |
| Peak during parse / retained after | **≈ 1.9 ×** | ≈ 1.9 × |

(Plus one copy of the input: `parse()` takes the `String` by value, reuses its buffer, and
holds it for the tree's lifetime — that is not counted above.)

Four findings drive almost all of it, in order of payoff-per-unit-of-churn:

1. **`NodeKind::Comment` sets the size of every node.** Its three inline `TextContent`
   fields make the enum 72 B (36 B wasm), so `NodeData` is 112 B (60 B). Boxing that one
   variant — exactly as `Group` and `Callable` already are — drops `NodeData` to **64 B
   (36 B wasm)**, a 43 % / 40 % cut of the single largest line item in the budget
   (the node vector is half of everything retained).
   Measured on mock layouts, not estimated. The doc comment on `NodeKind::Group` states
   the intent ("`Chars` should keep dominating the enum size"); today `Comment` does.
2. **`L::InvocationSyntax` is stored inline in every `CallableData`**, and for
   `Latexlike` its size is set by the environment arm: 128 B (64 B wasm) on every
   callable node, macro invocations included. `CallableData` is 216 B, of which 59 % is
   this one field.
3. **Parsing-state churn is the surprise term**: 10 004 distinct `ParsingState`s retained
   by a 1.3 MB document, 529 B each = **21 % of all retained memory**. Two constructs are
   responsible (measured, §4): optional/delimited arguments and scope-pushing
   environment bodies. Everything else memoizes to a constant. A non-memoized state also
   **cascades** — every derivation below it misses the memo too.
4. **The staging builder doubles peak.** `Staged` (136 B / 68 B wasm) plus a
   `Vec<BuildId>` per parent coexists with the flattened `Vec<NodeData>` inside
   `finish()`, plus ~20 B/node of scratch tables.

There is no leak and no unbounded retention: diagnostics are already capped
(`Diagnostics::DEFAULT_LIMIT = 1000`), the session memo dies with the session, and
dropping the tree returns everything.

---

## 1. Method

- **Sizes**: `size_of` printed from a temporary in-crate test module (so `pub(crate)`
  types were reachable). wasm32 sizes obtained by compiling
  `const _: [(); 0] = [(); size_of::<T>()];` for `wasm32-unknown-unknown` and reading the
  size out of the type error — no wasm runtime needed.
- **Heap**: a counting `#[global_allocator]` wrapping `System`, tracking current/peak
  bytes and allocation count; tests run with `--test-threads=1`. Numbers are *requested*
  bytes — a real allocator adds per-allocation headers and rounding (wasm's dlmalloc:
  ~8–16 B per allocation), which at the measured 4.3 allocations/node adds a further
  ~10–15 % on wasm.
- **Cost decomposition** by selective drop: clone the tree's distinct `Arc<ParsingState>`s
  and the `Arc<Source>` out, drop the tree (→ tree cost), drop the states (→ state cost),
  drop the source (→ source cost).
- **Corpora**, both ≈ 1.29 MB, parsed with `Latexlike` + `minidefs::minilatex_package`,
  `Recovery::Tolerant`:
  - *markup-heavy* — prose with `\emph`/`\textbf`/`\textit`, three `$…$` groups, a `%`
    comment, ligatures, and an `itemize` with three `\item`s (one `\item[*]`) per
    paragraph → **11.5 input bytes per node**;
  - *prose-heavy* — long plain paragraphs with one `\emph` each → **105 input bytes per
    node**.
- Debug and release builds gave byte-identical heap numbers (layout-determined), as
  expected.

Reproducing: the probe module was removed after measurement; it is ~250 lines
(counting allocator + `size_of` table + the corpora + the decomposition test) and easy to
re-create against these notes.

---

## 2. Type sizes

| Type | 64-bit | wasm32 |
|---|---:|---:|
| **`NodeData<Latexlike>`** | **112** | **60** |
| `NodeKind<Latexlike>` | 72 | 36 |
| `GroupData<Latexlike>` (boxed) | 56 | 28 |
| `CallableData<Latexlike>` (boxed) | 216 | 108 |
| ⤷ `InvocationSyntaxData` inside it | 128 | 64 |
| `CallableData<TrivialLang>` (`InvocationSyntax = ()`) | 88 | — |
| `TextContent` | 24 | 12 |
| `Span` | 16 | 8 |
| `SourceSpan` | 24 | 12 |
| `ParsedArguments` / `ParsedArgument` | 24 / 40 | 12 / 36 |
| `ParsedSlots` / `ParsedSlot` | 24 / 48 | 12 / 40 |
| `ChildRegion` | 28 | 28 |
| `NodeTree` / `TreeCore` | 32 / 56 | 16 / 32 |
| `Staged<Latexlike>` (builder) | 136 | 68 |
| **`ParsingState<Latexlike>`** | **208** | **108** |
| ⤷ `StateData` / `TokenRules` | 176 / 144 | 92 / 76 |
| `ScopeStack` / `PrefixTable` / `TriggerChars` | 24 / 24 / 24 | — |
| `ParsingStateDelta` / `TokenRulesOverrides` | 208 / 152 | 108 / 80 |
| `Token` / `TokenKind` | 88 / 56 | 44 / 28 |
| `StdTokenReader` | 24 | — |
| `Frame` / `FrameTitle` | 80 / 56 | 40 / — |
| `ParserSession` | 224 | 120 |
| `Source` | 120 | 60 |
| `ArgumentSpec` | 240 | 124 |
| `Diagnostic` / `Diagnostics` | 72 / 48 | — |

wasm32 is uniformly half of 64-bit — everything here is pointers and `usize` offsets.

**`NodeData` anatomy (64-bit / wasm32):**

```
kind          NodeKind<L>              72 / 36   ← set by the Comment variant
ext           NodeExt<L>                0 /  0   (() for Latexlike — free)
span          SourceSpan               24 / 12   (Arc<Source> + 2 × usize)
parsing_state Arc<ParsingState<L>>      8 /  4
children      Range<u32>                8 /  8
                                      ---------
                                      112 / 60
```

**`NodeKind` variant payloads (64-bit):** `Chars` 24, `Group` 8 (boxed), `Callable` 8
(boxed), **`Comment` 72** (three `TextContent`s inline), `List` 0. The enum tag folds
into a `TextContent` niche, so the enum size is exactly the largest payload.

**`CallableData` anatomy (64-bit):** `callable_type` 1, `name: Box<str>` 16,
`spec: Arc<dyn …>` 16, `arguments` 24, `slots` 24, **`invocation_syntax` 128** → 216.
`InvocationSyntaxData` is 128 because of `Environment(StdEnvironmentSyntax)`, which holds
two `StdEnvironmentSideSyntax` (each: `char` + two `TextContent` + `Arc<GroupRule>` = 64).
A `\emph` node needs 28 B of it and pays 128.

---

## 3. Where the bytes go — measured

All figures below are *newly allocated* bytes. The input `String` is **not** among them:
`parse()` takes it by value and the `Source` reuses its buffer — but it is then held for
the tree's lifetime, so the embedder's total residency is these numbers **plus one copy
of the input** (1 443 976 B of `String` capacity for 1 291 780 B of text in the corpus
below — growth slack, avoidable with `String::with_capacity`/`shrink_to_fit` on the
embedder's side).

### Markup-heavy, 1 291 780 input bytes, 112 001 nodes (64-bit)

```
retained (new allocations)     25 221 446   (19.5 × input, 225 B/node)
├─ tree                        20 058 188   (179 B/node)            79.5 %
│   ├─ node vector             12 544 112   (112 B × 112 001)       49.7 %
│   ├─ boxed callable payloads  5 184 000   (216 B × 24 000)        20.6 %
│   ├─ boxed group payloads     1 120 000   (56 B × 20 000)          4.4 %
│   ├─ names, arg/slot vecs       762 072                            3.0 %
│   └─ parent table               448 004   (4 B/node)               1.8 %
└─ parsing states               5 294 194   (10 004 × 529 B)        21.0 %
   (+ the retained input buffer 1 443 976)

peak during parse              48 458 187   (37.5 × input, 433 B/node, 1.92 × retained)
allocations                       486 141   (4.34 per node)
```

*(The two branch totals sum to 100.5 % — the decomposition's own bookkeeping vectors are
inside the measurement window. Treat the split as ±0.5 %.)*

### Prose-heavy, 1 262 890 input bytes, 12 001 nodes (64-bit)

```
retained (new allocations)      2 024 644   (1.6 × input, 169 B/node)
├─ tree                         2 024 188   (node vector 1 344 112, parent 48 004)
└─ parsing states                     352   (2 distinct states — the memo works)
   (+ the retained input buffer 1 288 360)
peak                            4 553 569   (3.6 × input)
allocations                        20 077   (1.67 per node)
```

The amplification factor is **entirely a function of markup density**. The honest
statement of techy's cost is "≈ 225 B per node (64-bit) / ≈ 120 B per node (wasm32),
plus the input buffer"; the "× input" figure follows from the document.

### wasm32 projection, markup-heavy

| Term | bytes |
|---|---:|
| node vector (60 B × 112 001) | 6 720 060 |
| callable payloads (108 B × 24 000) | 2 592 000 |
| parsing states (≈ 270 B × 10 004) | ≈ 2 700 000 |
| group payloads (28 B × 20 000) | 560 000 |
| parent table | 448 004 |
| names, arg/slot vecs | ≈ 420 000 |
| **retained total** | **≈ 13.4 MB ≈ 10.4 × input (120 B/node)** |
| *(+ retained input buffer)* | *1.29 MB* |

Same projection for the prose-heavy corpus: ≈ 1.07 MB ≈ **0.85 × input** (89 B/node).

---

## 4. Parsing-state churn — the non-obvious bottleneck

Distinct `ParsingState`s **retained by the finished tree** (each node holds an `Arc`),
per construct, at 1 and 50 repetitions:

| Construct | ×1 | ×50 | states per instance | bytes/state |
|---|---:|---:|---:|---:|
| plain text | 1 | **1** | 0 | 32 |
| `{a}` group | 2 | **2** | 0 | 176 |
| `$x$` math group | 2 | **2** | 0 | 273 |
| `% comment` | 1 | **1** | 0 | 32 |
| `\emph{a}` (mandatory arg) | 2 | **2** | 0 | 176 |
| `\begin{itemize}\item a\end{itemize}` | 2 | **51** | **1** | 351 |
| `\begin{itemize}\item[*] a\end{itemize}` | 4 | **151** | **3** | 584 |

("states per instance" = growth per repetition of the whole line. The last two rows
differ only by the `[*]`, so the optional argument alone accounts for 2 of that 3.)

Read: groups, math, comments and mandatory arguments memoize perfectly — 50 repetitions
cost the same as one. Two constructs do not:

- **Scope-pushing bodies** (an environment whose body delta carries a `ScopeOp`, e.g.
  `itemize` pushing the `minilatex.item` package): **one fresh state per instance**. This
  is by construction — the session derivation memo deliberately excludes deltas carrying
  `scope_ops` (documented under [§dd-dr:scope-stack]: "the session memo gate extends the old
  `push_libraries` exclusion to `scope_ops`" — only successes, and only rules/mode-shaped
  deltas, are cached).
- **Optional / delimited arguments** (`\item[*]`): **two further fresh states per
  invocation**. The optional-argument machinery mints a *temporary `GroupRule`* for the
  occasion; the memo keys rule payloads by `Arc` identity, so a freshly minted rule is a
  guaranteed miss every time.

**The cascade is the real cost.** A non-memoized state becomes a *fresh base* for
everything parsed beneath it, so all interior derivations miss too. In the markup-heavy
corpus each paragraph mints ≈ 5 states: 1 for the `itemize` body, 2 for the `\item[*]`,
and ~2 more for constructs *inside* the body (the `$f(x)$` math group etc.) that would
otherwise have been memo hits at the document level. 2 000 paragraphs → 10 004 states,
5.3 MB.

Each retained state pins its `TokenRules` (five `Vec`s of `Arc`s), its `PrefixTable`
(rebuilt per state), its `TriggerChars` string, and its `ScopeStack` `Vec` — hence 529 B
average rather than the 208 B struct size.

Candidate mitigations, cheapest first:

- **Cache the minted temporary `GroupRule` on the argument spec** (or in a small
  per-session interner keyed by delimiters + class) so the `Arc` is stable across
  invocations. Then the memo hits, and cost becomes O(distinct argument specs) instead of
  O(invocations). Kills both the 2-states-per-optional-argument term *and* its cascade.
- **Extend the memo key to `scope_ops`** where the ops are identity-comparable
  (`ScopeOp::Push(Arc<dyn SpecsProvider>)` is: pointer equality). By-name ops
  (`Define`, `Replace`, …) mutate a provider and must stay excluded. This is the
  environment-body term. Note the memo currently caches successes only, and that property
  must be preserved.
- **A `Scopes` feature facet** (`GateFeaturesOptimizedLangs.md`, option B) removes the
  `ScopeStack` from `StateData` entirely for languages that resolve from a fixed table —
  removing both the field and this whole churn class for the tiny-build case.

---

## 5. Peak vs. retained — the staging builder

`peak / retained ≈ 1.9` on both corpora; 433 B/node peak on the markup-heavy one.

`NodeTreeBuilder::finish()` moves rather than clones, but the two representations still
coexist: it builds `Vec<Option<Staged<L>>>` (136 B / 68 B wasm per node, each with its own
`Vec<BuildId>` child list) and fills a fresh `Vec<NodeData>` (112 B / 60 B) from it,
element by element, while also holding four scratch tables (`order`, `ranges`, `parent`,
`final_of` ≈ 20 B/node). Nothing is freed until `finish` returns.

Reducing peak without changing the design:

- Drop each `Staged`'s child `Vec` as soon as pass 1 has consumed it (pass 2 only needs
  `kind`/`ext`/`span`/`parsing_state`) — frees one allocation per parent early.
- Shrink `Staged` by the same `NodeKind` fix as `NodeData` (72 → 24 B of `kind`).
- `Vec::with_capacity` on the staged vector is impossible (count unknown), but the
  flattened vector is already pre-sized.

The transform/restage path has the same shape one level up: it builds a second tree while
the first is alive, so a restage peaks at roughly 2 × the tree.

---

## 6. Smaller terms, and what is already fine

- **Diagnostics are already bounded.** `Diagnostics::DEFAULT_LIMIT = 1000`, with overflow
  counted rather than stored, and configurable via `with_limit`. Measured on pathological
  input (20 000 stray `}` in tolerant mode): 1 000 diagnostics ≈ 99 KB, i.e. ~99 B each
  including the rendered traceback. Frames are allocation-free to push and titles render
  only at snapshot time — that design is doing its job.
- **Annotations cost nothing at `A = ()`** — `Vec<()>` allocates nothing.
- **Ext types cost nothing for `Latexlike`**: `NodeExt = ()`, `ArgumentExt = ()`,
  `SlotExt = BodyMarker` (1 B, absorbed into `ParsedSlot`'s padding).
- **Specs are shared, not per-instance**: `Arc<dyn CallableSpec>` and
  `Arc<ArgumentSpec>` (240 B each) are flyweights; a 24 000-callable document holds a
  handful.
- **Tokens never allocate** — `Token` is 88 B on the stack, spans only.
- **`materialize()` roughly doubles a tree** (measured: +5.3 MB on a 322 KB input /
  28 001-node tree) — one `Box<str>` per text field plus a full node-vector copy. Expected
  and opt-in; worth documenting as "don't materialize on wasm unless you must".
- **Allocation count** is 4.34/node on markup-heavy input (486 141 allocations for 1.3 MB).
  On wasm's dlmalloc that is a real secondary cost in both bytes (headers) and time. The
  biggest contributors are the per-callable `Box<str>` name, `Box<CallableData>`,
  `Box<GroupData>`, the per-parent child `Vec`, and the several allocations each minted
  state makes (five rules `Vec`s, the `PrefixTable`, the trigger-chars `String`, the
  `ScopeStack` `Vec`). Fixing #2 below removes a large share of these too.

---

## 7. Candidate changes, ranked

Savings quoted for the markup-heavy corpus on **wasm32** (the tiny-build case),
against the ≈ 13.4 MB retained baseline.

| # | Change | Saving | Churn | Notes |
|---|---|---:|---|---|
| 1 | **Box `NodeKind::Comment`'s payload** (`Comment(Box<CommentData>)`) | **2.69 MB (20 %)** | Low | `NodeData` 60 → 36 B. Touches the `NodeKind` match sites only. Also shrinks `Staged` by the same 24 B, cutting peak. Consistent with the existing `Group`/`Callable` precedent and its stated rationale. |
| 2 | **Fix the state churn** (§4: stable temporary-`GroupRule` `Arc`s + identity-keyed `scope_ops` in the memo) | **≈ 2.6 MB (19 %)** | Medium | Also cuts allocation count and transition cost. No public API change. |
| 3 | **Box the `Environment` arm of `InvocationSyntaxData`** | **≈ 1.15 MB (9 %)** | Low | Preset-local. `InvocationSyntaxData` 64 → ~16 B; `CallableData` 108 → ~60 B. Environments (rare) pay one allocation; macros (common) stop paying 48 B each. |
| 3′ | *(alternative to 3, no code change)* a custom `LatexlikeLang` with `type InvocationSyntax = ()` | 1.54 MB (11 %) | None | **This knob already exists.** Costs byte-exact re-emission of trigger spelling/post-space. Worth documenting as the wasm recipe. |
| 4 | **Gate the parent table** (`TreeCore::parent`, needed only for `NodeRef::parent`/ancestors) | 0.45 MB (3.3 %) | Low | Natural fit for a tree-level or `Lang`-level capability const. |
| 5 | **Per-tree source table**: node stores `Span` + `u16` source index instead of `SourceSpan` | 0.45 MB (3.3 %) on wasm, 0 on 64-bit | High | `single_source` is already computed in `finish()`. Weak on wasm (`Arc` is 4 B there); the 64-bit win is eaten by alignment. **Not worth it alone** — only as part of #6. |
| 6 | **Per-tree state table**: node stores `u16`/`u32` state index instead of `Arc<ParsingState>` | with #5: node 36 → 32 B, 0.45 MB | High | Only interesting after #2 makes the state set small. Breaks `NodeRef::parsing_state`'s `&Arc` return. Deep change to the ownership story ([§dd-dr:flat-node-tree]). |
| 7 | **`u32` span offsets** instead of `usize` | **0** | — | **Negative finding:** `TextContent`'s size is set by its `Owned(Box<str>)` arm (16 B), so shrinking the `Spanned` arm buys nothing; and wasm32's `usize` is already 4 B. Don't chase this. |
| 8 | Drop each `Staged`'s child `Vec` after pass 1 of `finish()` | peak only | Low | ~4 B/child of peak, plus earlier allocator reuse. |

Doing **1 + 2 + 3** takes the markup-heavy wasm32 case from ≈ 13.4 MB to **≈ 7.0 MB**
(≈ 5.4 × input, **≈ 62 B/node**, plus the input buffer) with no public API change beyond
the `NodeKind` and `InvocationSyntaxData` variant shapes. Adding 4 + 8 trims a further
~0.45 MB and noticeably reduces peak.

---

## 8. Interaction with the `Lang`-generic feature gating

`GateFeaturesOptimizedLangs.md` proposed `const FEATURES: FeatureSet` (phase 1) and
facet associated types (phase 2) for *code size*. Their memory-side effects, now
quantified:

- Gating **comments** off does **not** shrink nodes — the `Comment` variant's size is
  paid by every node regardless. Change #1 is the fix, and it is orthogonal to gating
  (worth doing even with comments enabled).
- Gating **scopes** off removes `ScopeStack` from `StateData` *and* removes the
  environment-body state churn class (§4) — a bigger runtime win than the 24 B of struct
  it deletes.
- Gating **groups**/**temporary groups** off removes the `PrefixTable` per state and the
  optional-argument churn class; but a language with callables that take optional
  arguments cannot gate groups off (the dependency lattice already noted in that
  document), so #2 remains the right fix for languages that keep them.
- Gating **callables** off removes `CallableData`/`ParsedArguments`/`ArgumentSpec`
  entirely from monomorphization — but such a language is chars+groups only, where the
  measured footprint is already the benign 1.6 × input.

The per-node terms (#1, #3) are the ones that matter for *every* language, gated or not;
the state terms (#2) matter for every language that uses scopes or delimited arguments.

---

## 9. Open questions for you

1. **#1 (box `Comment`)** — any reason the three fields were kept inline that I should
   know about before this is proposed as a change? The stated rationale for boxing
   `Group`/`Callable` ("`Chars` should keep dominating the enum size") argues *for* it;
   the cost is one allocation per comment node and one indirection on comment reads.
2. **#2 (state churn)** — is stabilizing the temporary `GroupRule` `Arc` per argument
   spec acceptable given the "minted for the occasion" semantics? The rule's *content* is
   fixed per spec; only its identity is currently fresh, and identity is exactly what the
   memo and the `derived()` stripping rule key on — so there may be a semantic reason for
   freshness I have not found.
3. **#3 vs #3′** — do you want the preset to shrink (box the environment arm), or should
   the wasm story be "define your own `LatexlikeLang` with a smaller `InvocationSyntax`"?
   The second is free today but silently gives up byte-exact recomposition.
4. **#4 (parent table)** — a `NodeTree`-level build option, a `Lang` const, or not worth
   it?
5. Is there interest in a **`bytes_per_node` regression test / criterion bench** so these
   numbers stop being a one-off measurement? The probe is small and self-contained.
