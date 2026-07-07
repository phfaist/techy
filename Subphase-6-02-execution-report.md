**Subphase 6.2 (`NodesParser` core) is complete: 181 tests + 1 doctest green** (152 baseline + 29 new), clippy clean for the new code, progress table updated. I have not committed — the working tree holds the changes for your review.

## What shipped

**`src/constructs/nodes_parser.rs`** (~600 lines + tests) — the content dispatch loop with the decided §2 shapes:

- **`NodesParser`** (tier-2 temporary, `ConstructParser` impl with `Output = NodesOutcome`), plus **`StopSpec`**, **`TokenStopCondition`** (Command / GroupClose-by-class / ParagraphBreak / Predicate), **`StopCause`**, **`NodesOutcome`** — exported from `constructs` and the crate root.
- **Chars accumulation** per invariant 1: maximal runs, every token's pre-space joins the pending run, whitespace-only `Chars` nodes, always `Spanned`. **Paragraph breaks** staged from `Lang::make_paragraph_break_node` (hook-returned kind, token span; runs never merge across). **Comments** built from whole-comment tokens with `start`/`content`/`post_space` sub-spans recovered from the token geometry. **EOF** stages the trailing-whitespace node, then `EndOfInput`.
- **Stop machinery**: token condition on peek (token left unconsumed), node condition after each staged node (count + `StagedNodeView`); `UnexpectedGroupClose` returned as data, no diagnostic — caller decides.
- **Recovery**: strict aborts via `cx.recover`; tolerant adopts `TokenRecovery` placeholder tokens with the reader repositioned to `resume_pos`. The `GroupOpen`/`Command`/`Specials` arms take the placeholder recovery (diagnostic + span-backed chars fallback) until 6.3/6.4 wire in the real parsers.
- **Tests** run against both `StdTokenReader` and `TokenListReader` in lockstep (shapes, stop cause, position, diagnostics count must agree), with exact-span assertions and a partition-invariant checker on every clean tree.

## Small decisions I made within implementation freedom — please review

1. **Position seam (pinned in the progress table note):** on *any* stop, the reader is parked at the stop token's `span.start` via `move_to(tok, false)`; the token's pre-space has already been flushed into sibling content (the partition invariant forces this — whitespace before `}` or `\end` is interior content). Re-peeking yields the token with empty pre-space, so no byte is ever represented twice. `TokenListReader`'s pre-space clipping supports exactly this.
2. **`NodesParser::new(source, stop)`** takes the `Arc<Source>` as per-use config — token spans are plain byte ranges, so the parser needs the Arc to mint `SourceSpan`s. Note for 6.4: `StdInvocationParser` will need the same Arc, but the decided `make_invocation_parser(invocation)` signature doesn't carry one — `Invocation` may need a source field (or `ParseContext` does). Worth settling before 6.4.
3. **`resume_at` helper:** `TokenReader` has no absolute-positioning method, so repositioning to `resume_pos` is phrased as `move_to` on a zero-width marker token. It's correct for both readers but slightly hacky — alternative would be promoting the readers' existing inherent `move_to_pos` into the trait. Your call; I didn't touch the decided trait.
4. **Sibling-delta application deferred to 6.4** (a deviation from the 6.2 work-item list): no 6.2 arm produces a `ParsingStateDelta`, so the application code would be unreachable; the seam is marked in the loop, and the test lands with 6.4's `\newcommand`-shaped test as already planned there.
5. **Unresolved-command fallback swallows the token's post-space** (the chars node covers the full token span): consuming the command without its post-space would desynchronize `TokenListReader`'s fixed list, and the fallback is a diagnosed artifact anyway.
6. **Node condition firing on the final flush at EOF reports `StopConditionMet`**, not `EndOfInput` (uniform "consulted after each node" rule).

If you'd like, items 1, 5, and 6 could be recorded as amendments in DESIGN_RATIONALE.md §3.5/§3.6 — I left the doc untouched pending your sign-off.




---
---


# REPORT after above subphase implementation on parity with pylatexenc.

> We just implemented subphase 6.2 of @Phase6Execution.md in this project, with a report in @Subphase-6-02-execution-report.md . Highlight any deviations from pylatexenc's behavior.

---


I've now read the 6.2 implementation, the execution plan, the report, and the repo's own pylatexenc feature comparison. Here's my assessment of where subphase 6.2's `NodesParser` deviates from pylatexenc's `LatexGeneralNodesParser` / `LatexNodesCollector`.

## By-design divergences (intended, but they change the tree vs. pylatexenc)

**1. Whitespace is materialized into the tree — the partition invariant.**
This is the biggest observable divergence. techy's rule (`nodes_parser.rs:41-44`, invariants 1 & 4) is that *every* byte lands in exactly one sibling node. pylatexenc does not guarantee this and routinely drops or re-attaches whitespace. Concretely, 6.2 produces nodes pylatexenc would not:
- Pre-space before a stop token becomes a sibling `Chars` node — `" \end"` → `chars " "` then stop (`lone_pre_space_before_a_stop_token_becomes_a_whitespace_node`, line 896). In pylatexenc that space rides on the `\end` token's `pre_space` and never appears in the body nodelist.
- Trailing whitespace at EOF becomes a `Chars` node (`EndOfStream` arm, line 378-388). pylatexenc leaves it as `pre_space` on the EOF token and discards it.
- Whitespace-only input → a whitespace `Chars` node (line 760). 

This is the "techy is ahead" item in the repo's own report (§7.2), so it's deliberate — but it means environment bodies and argument groups will carry boundary-whitespace `Chars` nodes that pylatexenc trees don't have. Worth keeping in mind for any golden-tree comparison against pylatexenc.

**2. Paragraph break → a `Chars` node by default, not a specials node.**
`make_paragraph_break_node` (line 415) defaults to a whitespace-only `Chars` spanning the `\n\n` token. pylatexenc v3 treats `\n\n` as a **specials** token that resolves to a `\par`-like `LatexSpecialsNode`. techy's is a `Lang` hook, so a latexlike preset (Phase 7) can override it to emit a specials-shaped node — but the *default* diverges, and `TokenStopCondition::ParagraphBreak` stopping semantics are keyed to a token kind rather than a resolved specials spec.

**3. Stop tokens are *always* left unconsumed.**
`StopCause::StopConditionMet` parks the reader at the stop token's `span.start` (line 364-368). pylatexenc's `stop_token_condition` pairs with `handle_stop_condition_token`, which lets a condition consume-and-interpret the stop token. The code comment at line 78-79 calls this out explicitly ("pylatexenc's `handle_stop_condition_token` ambiguity removed"). Callers must now consume the stop token themselves.

**4. Node stop condition is `(count, last-staged node)`, not the whole nodelist.**
`StopSpec.node` (line 116) gets `FnMut(usize, StagedNodeView)`. pylatexenc's `stop_nodelist_condition(nodelist)` sees the entire accumulated list and can inspect any earlier node. A condition that needs to look back more than one node has no home here (the report §4.3 flags this — it's meant to be covered by `ExpressionParser` / the custom-parser hatch instead). Documented deviation (line 111-113).

**5. Abnormal endings are data, not errors — and 6.2 emits no diagnostic for a stray close.**
An unmatched `}` returns `StopCause::UnexpectedGroupClose` with the token unconsumed and **no diagnostic** (`group_close_without_any_stop_condition_is_unexpected`, line 968, asserts `diagnostics.is_empty()`). pylatexenc diagnoses a stray brace at the detection site under tolerant parsing. In techy the diagnostic is deferred to whoever owns the frame (root loop / group parser, wired in 6.3). So at the 6.2 boundary a stray close is currently silent — that's expected for the subphase, but it is a real behavioral gap until 6.3 lands the caller side.

**6. Comment nodes record the `%` start delimiter.**
`NodeKind::comment(start_span, content_span, post_space)` (line 436) stores the delimiter span. pylatexenc's `LatexCommentNode` keeps only `comment` + `comment_post_space` and re-synthesizes `%` on recomposition. This is a richer record (decision 11), harmless, but the node shape differs from pylatexenc's.

## Recovery-model divergences (worth an explicit flag)

**7. Unresolvable command → `Chars` fallback + diagnostic, vs. pylatexenc's unknown-macro-as-node.**
Right now every `Command` is recovered as chars (line 445-454) because dispatch lands in 6.4 — that part is pure scaffolding. But note the *final* design (Phase6Execution §6.4: `resolve_command` returns `None` → "diagnostic + span-backed chars-node fallback") keeps that shape for genuinely unresolvable commands. pylatexenc's default is the opposite: an unknown macro gets `LatexContextDb`'s `unknown_macro_spec` (an empty-args spec) and becomes a real `LatexMacroNode`, optionally with a warning — it does **not** degrade to chars. A pylatexenc-parity `Lang` can recover this behavior by having `resolve_command` hand back a default no-args spec instead of `None`, but the out-of-the-box `None` path diverges. Also note the fallback deliberately swallows the command's post-space (report item 5, line 296-299) — a recovery artifact, not general behavior.

**8. Tolerant recovery is diagnostics + recovery tokens, not exceptions-as-control-flow.**
The whole `recover` / `TokenRecovery` mechanism (line 344-358) replaces pylatexenc's `tolerant_parsing` exception filtering. A recovered placeholder token is processed as content and the stop condition is deliberately **not** consulted on it (line 364, "recovered" guard) — no pylatexenc analog. Architectural, decided in Phases 1–5.

## My read on parity risk

None of these are bugs — 1–6 are decisions recorded in DESIGN_RATIONALE §3.5/§3.6/§3.8 and the code comments are faithful to them. The two I'd surface to you for an explicit parity decision, since they'll show up the moment anyone diffs a techy tree against a pylatexenc one:

- **#1 (boundary whitespace nodes)** and **#2 (paragraph = chars, not specials)** are the ones that change *default* tree shape most visibly. #2 in particular: if the latexlike preset is meant to match pylatexenc, it will need to override `make_paragraph_break_node` to a specials-shaped node — worth confirming that's the plan before 6.4/Phase 7.
- **#7 (unresolvable → chars)**: confirm the intended latexlike preset supplies fallback specs via `resolve_command` so unknown macros become macro nodes as in pylatexenc, rather than chars.

The report's own flagged items (position seam #1, post-space swallowing #5, EOF-node-condition #6) are internal and don't affect pylatexenc parity.

Want me to draft the DESIGN_RATIONALE amendments for #1, #2, and #7 (the report offered to record items 1/5/6 pending your sign-off), or leave the docs untouched?