# Phase 6 Execution Plan — `constructs` + engine core

**Status: EXECUTION PLAN, July 2026.** Supersedes
`PreliminaryPlanNotesArchitectureExecutionPhase6.md` (to be removed by the user before
execution). All design questions of the preliminary notes (Q1–Q4, C1–C7, D1–D6, naming)
were settled in the July 2026 **Phase 6 plan session**; the decisions and their rationales
are recorded in DESIGN_RATIONALE.md (§3.2, §3.5, §3.6, §3.8, §3.10) and reflected in
ARCHITECTURE.md (§specs, §constructs, §engine, §nodes, §9). This file adds the *execution*
detail: subphases, work items, tests, exit criteria.

**How to execute.** One subphase per Claude Code session. At session start: read CLAUDE.md
(automatic), this file, DESIGN_RATIONALE.md §3.5/§3.6/§3.8, and ARCHITECTURE.md
§constructs/§engine/§nodes; run `cargo test` to confirm the green baseline. The signatures
in §2 below are the decided shapes — small implementation-forced adjustments are fine, but
**any deviation from a recorded decision needs the user's sign-off first** (CLAUDE.md
rule 1). At subphase end: `cargo build && cargo test` green, update the progress table
below, and summarize what shipped and what (if anything) deviated.

---

## Progress

| subphase | scope | status |
|---|---|---|
| 6.0 | Documentation pre-update (ARCHITECTURE.md, DESIGN_RATIONALE.md) | ✅ done (July 2026, plan session) |
| 6.1 | Contracts & amendments (token/node/Lang/builder/module scaffolds) | ✅ done (July 2026). `ConstructParserResult<L, T>` takes `L` (user-approved; `TokenResult` convention, §2 updated); `check_tree_invariants` deferred to 6.3 as allowed |
| 6.2 | `NodesParser` core (chars, paragraphs, comments, stop conditions, recovery) | ✅ done (July 2026). Sibling-delta application deferred to 6.4 (no 6.2 arm produces a delta; the mid-stream test lands with the `\newcommand`-shaped test). Position-seam convention pinned: on a *leave* stop, the reader stands at the stop token's `span.start`, its pre-space already staged as sibling content. Post-review amendment (user, July 2026): token stop conditions gained a `consume` switch (`TokenStopCondition` is now `{ kind: TokenStopKind, consume }`) and `StopCause` split into `TokenCondition {span}` / `NodeCondition` (+ `span` on `UnexpectedGroupClose`) — a consumed stop token is taken whole (`move_past`, post-space included) and its span reported; see DESIGN_RATIONALE §3.6 |
| 6.3 | Groups + `check_tree_invariants()` | ✅ done (July 2026). Also shipped, per the child-state design session's 6.3 timing (DESIGN_RATIONALE §3.6): `ChildStateSpec` (struct + both enums; `group` arm active, `invocation` consulted from 6.4) and the whole session derivation seam (`Lang::SessionExt` + `observe_transition`, `ParserSession::derived_state`, ptr-keyed `group_interior_state` memo). Implementation notes: the dispatch arm consumes the `GroupOpen` trigger and hands `GroupParser` `(open_span, rule)` — the uniform `parse(&mut self, cx)` signature cannot tie a stored `Token<'s>` to the context's reader, and arm-side consumption keeps the token consumed under the state that tokenized it; interior `UnexpectedGroupClose` → diagnostic + close-without-consuming (decision 8 unwinding), root diagnose+skip lives in the root driver (tests now, `Language::parse` in Phase 7) and drops the skipped byte — the one accepted byte-accounting break, exempted from the checker; `SessionExt` bounds grew `Send + Sync` for consistency with the trait's other associated types |
| 6.4 | Invocation dispatch + `make_invocation_parser` + `StdInvocationParser` (zero-arg) | ✅ done (July 2026). Three user-approved amendments: (1) **`CallableData.post_space` = exactly the trigger token's own syntactic post-space; nothing is ever claimed beyond it** (TeX/pylatexenc parity — `\& b`'s space is sibling content; also keeps `TokenListReader` faithful, which a re-peeking claim helper would desync). `claim_post_space` never shipped; §3.5 invariant 3 amended. 6.5/6.6 consequences: with arguments the recorded post-space sits between name and first region (⇒ `check_tree_invariants` rule 3's *trailing* assertion must be revised in 6.5); environments will record empty post-space. (2) `ParseContext` gained `source: Arc<Source<L::SourceOrigin>>` (factory-created parsers have no ctor to thread it; tokens/readers deliberately carry only byte spans, §3.8) — `NodesParser::new`/`GroupParser::new` dropped their source params. (3) `TokenKind::Specials`/`SpecialsMatch` carry `callable_type`: recognition = resolution ⇒ the full `ResolvedCallable` pair rides the token (§3.2 amended). Implementation notes: the arm consumes the trigger **whole** before the parser runs (GroupOpen precedent — loop progress by construction; takeover parsers reposition via `move_past(token, false)` for the `\verb` idiom); recovery-placeholder Command/Specials tokens are never dispatched (chars fallback); the after-effect delta is applied in the dispatch helper via `session.derived_state` *after* the structural revert, i.e. to the loop's own state. Shipped tests: macro + specials end-to-end (both readers), post-space placement (multi-char/single-char/¶ cutoff), library-miss recovery, `\newcommand`-shaped push-library delta, raw-token takeover (untyped-group shape + tokenization-affecting delta, C6 proven), `finalize_node` callable-ext rehearsal, `InvocationChildState` Fixed/Compute |
| 6.5 | Arguments (`ArgumentParser` entry point, standard argument parsers, regions) | ✅ done (July 2026). Shipped per §2/§3: `ArgumentParser::parse_argument` + `ParsedArgumentNodes` (spec side; `ConstructParserResult<L, …>` per the 6.1 lang-first convention), `scan_argument_noise`/`stage_pre_space` + absent-rewind (position-based via the shared `resume_at` — equivalent to the planned move_to-first-noise-token), `GroupArgumentParser`/`OptionalGroupArgumentParser`/`MarkerArgumentParser`/`ExpressionParser` in `constructs`, `StdInvocationParser` completed (per-argument delta session-mediated, structural revert; 6.4's debug_assert narrowed to slots = the 6.6 boundary), checker rule 3 revised as flagged (post-space ends at the first child, or at span end when childless). Implementation decisions flagged for sign-off in Subphase-6-05-execution-report.md: requires-args-in-expression recovery = bare single-token callable (all declared args absent, nothing consumed past the trigger — pylatexenc parity); optional probe rule *prepended* (wins ties); **optional-group nesting resolved by user review (July 2026): brackets balance, pylatexenc parity** — the minted rule scopes the whole argument extent, same-rule child groups keep the contents state, all other child descents revert to the argument state (braces protect; DESIGN_RATIONALE §3.6 new entry; `GroupParser` gained per-use `with_child_states`); lone-group unwrap opt-in via `unwrap_lone_group` on the optional parser only; expression content = the node itself, matched-group content = group children; token error while probing → absent without diagnosing (tolerant; the loop re-reads and diagnoses) / abort (strict); expression-position after-effect deltas dropped (no delta channel in the decided signature). The Phase 5 hand-built example tree (node/mod.rs) still encoded the superseded post-space claim and was updated to the 6.4 semantics. Optional/delta tests run `StdTokenReader`-only (a pre-scanned list cannot re-tokenize under momentary states — `TokenListReader`'s documented fidelity limit) |
| 6.6 | Environment bodies (`EnvironmentBodyParser`) + end-to-end suite | ✅ done (July 2026). `EnvironmentBodyParser` shipped in core (`constructs/environment_parser.rs`): body via a `Command`-stop `NodesParser` (consume = false — the decided manual post-stop lookahead), rigid `\end{name}` reading (chars-only, no whitespace, name group immediately next; the command's own post-space is the one tolerated gap), decision-8 recovery matrix (mismatch → diagnostic + close **without consuming**; malformed → diagnostic + consume the terminator **command alone**; EOF → diagnostic anchored at the trigger; stray close in the body → diagnostic + unwind without consuming), body `List` staged over the content interior (empty body = empty `List`). Two small shape adjustments flagged for sign-off (report): the constructor gained `trigger_span` (the EOF diagnostic's anchor — `GroupParser` unclosed-at-open precedent), and `match_invocation_name` is a builder switch beside an always-required `invocation_name` (diagnostics need the name regardless). The environment-shaped composition stayed **test-side** per the plan's implementation freedom (`\begin` dispatcher spec → name-group read → env lookup → arguments → body → `ParsedSlots` → empty post-space); the argument loop was extracted from `StdInvocationParser` as pub(crate) `parse_declared_arguments` and shared, and the slots `debug_assert` is now the permanent macro-shape boundary (factory override = the environment path). Lifetime finding for Phase 7: a composition inside `parse(cx)` cannot mint a new `Invocation` for the resolved environment (the `'s` content is unreachable through `cx`), and stored trigger tokens cannot be handed back to the reader — takeover repositioning is positional (`resume_at`). §G end-to-end suite + the full §2 test matrix shipped (27 tests, both readers where tokenization is state-stable; slot-delta/optional/verbatim cases `StdTokenReader`-only per the documented fidelity limit) |
| 6.7 | Closing docs, quarry, benchmark check | ☐ |

Update the status column (☐ → ✅ + date, or ⚠ + note) when a subphase completes.

---

## 1. Decisions in force (recap — full arguments in DESIGN_RATIONALE.md)

1. **Two-tier parser ownership** (§3.6). Stored behavior objects (specs, `ArgumentParser`s):
   `Arc`-shared, `Send + Sync`, immutable, `&self` + context arguments. Engine construct
   parsers: **temporaries** — constructed with per-use config, `parse(&mut self, cx)`,
   free to borrow, dropped with the frame. No `'static`, no `OnceLock`.
2. **`CallableSpec::make_invocation_parser`** (§3.6): a factory returning a fresh boxed
   parser per invocation, ownership moved to the caller; the `Invocation` travels inside
   the parser instance. Overriding the factory = the full-takeover escape hatch.
   Per-invocation `Box` accepted — **benchmark before phase close** (§6.7).
3. **`Lang::finalize_node`** (§3.6): centralized finalization, called by
   `NodeTreeBuilder::add` for **every** staged node (all kinds); presets delegate to their
   own spec types; implementations must be idempotent (transforms re-stage). Supersedes
   any spec-level finalize hook.
4. **`Lang::resolve_command`** and **`Lang::make_paragraph_break_node`** hooks (§3.6).
5. **Slot terminators are parser business** (§3.6): `SlotSpec` stays
   `{ name, parsing_state_delta }`; the terminator data parameterizes the core
   `EnvironmentBodyParser`. Verbatim bodies: custom parser, no doctrine needed.
6. **Environment scaffolding is reconstructed, not stored** (§3.5): rigid
   `\begin{name}`/`\end{name}` syntax (name group = immediately next token; no comments/
   newlines; inline whitespace tolerated and unrecorded — level-2 normalizes). Children =
   argument regions + body `List`, one span-contiguous block; scaffolding = the complement.
   Presets wanting the verbatim strings extract the two sub-spans in `finalize_node` → ext.
7. **Stop conditions** (§3.6): token condition (closed enum or predicate) + node condition
   (predicate over (count, last staged node)); a matched token is left unconsumed or
   consumed per the condition's `consume` switch (6.2 amendment; default = leave);
   `NodesParser` returns **`StopCause`** — `TokenCondition`/`NodeCondition`/`EndOfInput`/
   `UnexpectedGroupClose`, the token-bearing causes carrying the matched span — abnormal
   endings are data, not errors.
8. **Terminator mismatch: close without consuming** (§3.6); malformed terminator (no name
   group): diagnose + consume + close. Loop safety: every level consumes or unwinds; the
   root always consumes.
9. **Detection-site recovery; `Err` means abort** (§3.8): tolerant recovery happens where
   detected (cx helper: record-diagnostic-and-continue vs strict `Err`); `ParseError`
   carries no recovery payload.
10. **Whitespace/span invariants pinned** (§3.5, numbered): chars accumulation, paragraph
    breaks via hook, post-space claimed *by the invocation parser* (one-call helper),
    EOF pre-space node, partition of content interiors (test-utility-checked).
11. **`Comment` nodes grow `start` + `post_space`** (`TextContent`) (§3.5).
12. **Token amendments** (§3.2): `Command { name, escape_char, post_space }`;
    `TokenRules::multi_newline_paragraphs` (renamed).
13. **No `Language<L>` in Phase 6** (§3.6): `ParserSession` is the root object; interning
    stays deferred; test langs use const type ids as in Phases 3–5.
14. **Names** (§3.10): `NodesParser` (not `ContentParser` — "content" now means designated
    argument content), `ConstructParserResult<T>`, `StopCause`, `Invocation`,
    `ResolvedCallable`, `make_*` for factory hooks.

---

## 2. Design reference (decided shapes; minor lifetime/name adjustments allowed)

Module homes: construct parsers in `src/constructs/` (S1 topic); `ParserSession` in
`src/engine/`; `ParseError` in `src/error.rs`; `TokenListReader` in `src/token/`;
`check_tree_invariants` + staged read view in `src/node/`.

```rust
// --- constructs -------------------------------------------------------------------

pub struct ParseContext<'a, 's, L: Lang> {
    pub tokens: &'a mut dyn TokenReader<'s, L>,
    // (6.4 amendment, user-approved) the source token spans refer into — what staging a
    // SourceSpan requires; factory-created parsers have no ctor to thread it through, and
    // tokens/readers deliberately carry only byte spans (§3.8). NodesParser::new /
    // GroupParser::new dropped their redundant source params.
    pub source: Arc<Source<L::SourceOrigin>>,
    pub state: Arc<ParsingState<L>>,      // the parser's INPUT state (caller sets it)
    pub session: &'a mut ParserSession<L>,
}

// (6.1 adjustment: lang-first, like TokenResult<'s, L, T> — ParseError is origin-generic)
pub type ConstructParserResult<L, T> = Result<T, ParseError<<L as Lang>::SourceOrigin>>;

pub trait ConstructParser<L: Lang> {
    type Output;
    fn parse(&mut self, cx: &mut ParseContext<'_, '_, L>)
        -> ConstructParserResult<L, (Self::Output, Option<ParsingStateDelta<L>>)>;
}

pub struct Invocation<'a, 's, L: Lang> {
    pub callable_type: L::CallableTypeId,  // for Specials: carried on the token (6.4 amendment)
    pub name: &'s str,                     // as written (the node stores an owned copy)
    pub spec: &'a Arc<dyn CallableSpec<L>>,
    pub token: &'a Token<'s, L>,           // trigger token: span, pre_space, escape_char
}

// on CallableSpec (added in 6.4, defaulted by StdInvocationParser). Contract pinned in
// 6.4: the dispatching arm consumes the trigger token WHOLE (move_past(tok, true) —
// GroupOpen precedent, loop progress by construction) before the parser runs; a takeover
// parser wanting the post-space bytes raw repositions itself (move_past(token, false)).
// CallableData.post_space = exactly the trigger token's own syntactic post-space — the
// claim_post_space helper was superseded before shipping (user decision, 6.4; see the
// progress-table note and DESIGN_RATIONALE §3.5 invariant 3).
fn make_invocation_parser<'a, 's>(&'a self, invocation: Invocation<'a, 's, L>)
    -> Box<dyn ConstructParser<L, Output = BuildId> + 'a>
where 's: 'a
{ Box::new(StdInvocationParser::new(invocation)) }

// --- NodesParser stop machinery ---------------------------------------------------

pub enum TokenStopKind<'p, L: Lang> {    // token-condition discriminant (was TokenStopCondition; 6.2 amendment)
    Command { name: &'p str },
    // matches the exact (group_type, close) pairing: close disambiguates within a class
    // (`]` vs `{`), group_type within a delimiter (a delta-reclassed `}`); class resolved
    // from the current state (the `GroupClose` token carries only `delim`) — 6.2 amendment
    GroupClose { group_type: L::GroupTypeId, close: &'p str },
    ParagraphBreak,
    Predicate(&'p dyn Fn(&Token<'_, L>) -> bool),
}

// consume switch bound to the condition (6.2 amendment): on a match NodesParser consumes
// the matched token (move_past, syntactic post-space included) when `consume`, else leaves
// it unconsumed at span.start. No `leave_post_space` knob — a closer has no content space.
pub struct TokenStopCondition<'p, L: Lang> {
    pub kind: TokenStopKind<'p, L>,
    pub consume: bool,
}

pub struct StopSpec<'p, L: Lang> {       // both triggers optional and independent
    pub token: Option<TokenStopCondition<'p, L>>,
    pub node: Option<&'p mut dyn FnMut(usize, StagedNodeView<'_, L>) -> bool>,
}

pub enum StopCause {                     // split + spans (6.2 amendment)
    TokenCondition { span: Span },        // token stop matched; span = the matched (consumed) token
    NodeCondition,                        // node cond hit on the last staged node
    EndOfInput,                           // EndOfStream reached (trailing-whitespace node staged)
    UnexpectedGroupClose { span: Span },  // close token left unconsumed; caller decides (§3.8)
}

pub struct NodesOutcome { pub nodes: Vec<BuildId>, pub stop: StopCause }
// NodesParser: ConstructParser<L, Output = NodesOutcome>

// --- Lang hooks (state/lang.rs) -----------------------------------------------------

pub struct ResolvedCallable<L: Lang> {
    pub callable_type: L::CallableTypeId,
    pub spec: Arc<dyn CallableSpec<L>>,
}
fn resolve_command(state: &ParsingState<Self>, token: &Token<'_, Self>)
    -> Option<ResolvedCallable<Self>> { None }

fn make_paragraph_break_node(state: &ParsingState<Self>, token: &Token<'_, Self>)
    -> NodeKind<Self> { /* whitespace-only Chars, Spanned over the full token span */ }

fn finalize_node(
    kind: &mut NodeKind<Self>,
    ext: &mut NodeExt<Self>,
    span: &SourceSpan<Self::SourceOrigin>,
    parsing_state: &Arc<ParsingState<Self>>,
    children: &[BuildId],
    staged: &StagedNodes<'_, Self>,   // read-only view into the builder (kind/span/children by BuildId)
) {}

// --- engine -------------------------------------------------------------------------

pub struct ParserSession<L: Lang> {
    pub builder: NodeTreeBuilder<L>,
    pub diagnostics: Diagnostics<L::SourceOrigin>,
    pub recovery: Recovery,
}
// finish: minimal ParseResult { tree: NodeTree<L>, diagnostics } — NO 'env lifetime,
// no Language reference (deferred, decision 13).
```

**State-threading convention** (pins the §state "caller applies deltas" law to `cx`):
`cx.state` is the parser's *input* state. A parser that scopes a child state (group
interior, argument extent, slot body) derives it locally and either builds a child `cx` or
swaps `cx.state` and restores it (structural revert — `Arc` clone is cheap). The
`Option<ParsingStateDelta>` in the return value is exclusively the *after-effect for the
caller* (`\newcommand`); `NodesParser` applies returned sibling deltas internally as it
loops and itself returns `None` (no current consumer of a merged pass-through delta —
revisit when one appears).

**Recovery convention** (§3.8): `ParseContext` (or `ParserSession`) exposes a helper —
tolerant: push `Diagnostic`, continue; strict: return `Err(ParseError)`. Token errors:
consume the `TokenRecovery` token and continue (tolerant) / abort (strict). Never continue
past an `Err`.

---

## 3. Subphases

### 6.0 — Documentation pre-update ✅

Done in the plan session: DESIGN_RATIONALE.md entries (§3.2 ×2, §3.5 ×3, §3.6 ×9, §3.8 ×1,
§3.10 ×1, §6 strikes ×2); ARCHITECTURE.md (§specs factory + terminator note, §constructs
sketch/loop/parser list, §engine hooks + `Language` deferral, §nodes invariants +
rigid-scaffolding clause, §9 Phase 6 entry).

### 6.1 — Contracts & amendments

Types and plumbing only; no parsing behavior. Everything spec-facing becomes final here.

**Token module:**
- `TokenKind::Command` gains `escape_char: char` (decision 12): `token.rs` (variant,
  `detached()`, `PartialEq`, `Debug`), `reader.rs` (`read_command` records the fired
  `CommandRule`'s escape char), Phase 3 test ripple.
- Rename `double_newline_paragraphs` → `multi_newline_paragraphs` everywhere
  (`rules.rs`, `prefix_table.rs`, `reader.rs`, docs).
- `TokenListReader` (new `src/token/list_reader.rs`): a `TokenReader<'s, L>` over a
  pre-built `Vec<Token<'s, L>>`. Faithful `move_to`/`move_past` (locate the token by span
  identity; `rewind_pre_space`/`skip_post_space` semantics must match `StdTokenReader`),
  `pos()` = byte position consistent with span conventions. Purpose: construct-parser unit
  tests in isolation (report R6).

**Node module:**
- `Comment` kind grows `start: TextContent` + `post_space: TextContent` (decision 11):
  `kind.rs`, builder `debug_assert_spanned_contents`, `NodeRef` accessors
  (`comment_start()`, `comment_post_space()`), materialization plumbing.
- `StagedNodes<'_, L>` read view on the builder (kind/span/ext/children of a staged node
  by `BuildId`) — consumed by `Lang::finalize_node` and node stop predicates
  (`StagedNodeView` = one staged node's view).
- `NodeTreeBuilder::add_with_ext` calls `L::finalize_node(…)` on the node parts **before**
  the staging checks (hook mutations are validated too).
- `check_tree_invariants()` skeleton may land here or in 6.3 (see 6.3).

**Lang hooks (`state/lang.rs`):** `ResolvedCallable<L>`, `resolve_command`,
`make_paragraph_break_node`, `finalize_node` — signatures + defaults per §2.

**Constructs/engine scaffolds:** rename old quarry `src/constructs/{mod,general}.rs` →
`*_JUNK._rs`; new `src/constructs/mod.rs` with `ConstructParser`, `ConstructParserResult`,
`ParseContext`, `Invocation` (referenced by `CallableSpec` docs now, trait method lands
in 6.4). New `src/engine/mod.rs` with `ParserSession<L>` + the recovery helper. Finish
`ParseError` in `src/error.rs` (kind enum + `SourceSpan`, no recovery payload, `Display`,
`core::error::Error`).

**Tests:** token amendments (escape char through reader; rename); `TokenListReader`
round-trips + `move_to` semantics; `Comment` fields through builder + materialize;
`finalize_node` invoked for every staged kind (test lang counting calls / mutating ext);
staged-view accessors.

**Exit:** `cargo build && cargo test` green (existing suite + new units); all spec-facing
types final; no parsing behavior yet.

### 6.2 — `NodesParser` core

The dispatch loop for content without groups/invocations, plus the stop machinery.

- `src/constructs/nodes_parser.rs`: `NodesParser` configured with `StopSpec`; output
  `NodesOutcome`. Loop arms this subphase: `Char` (accumulation), `ParagraphBreak` (via
  `Lang::make_paragraph_break_node`, staged by the loop), `Comment` (node from token,
  incl. `start`/`post_space`), `EndOfStream` (stage trailing-whitespace `Chars` from
  `pre_space`, then `StopCause::EndOfInput`). `GroupOpen`/`GroupClose`/`Command`/
  `Specials` arms: minimal tolerant recovery (diagnostic + skip / chars fallback) with
  wiring completed in 6.3/6.4.
- Chars accumulation per invariant 1 (§3.5): maximal runs, pre-space joins, flush on
  non-`Char`; whitespace-only `Chars` nodes; always `TextContent::Spanned`; runs flush at
  paragraph breaks, no merging across them.
- Stop machinery per decision 7: token condition checked on peek (token left unconsumed);
  node condition checked after each staged node (via `StagedNodeView`).
- Recovery per decision 9: token-error path (consume `TokenRecovery`, diagnose, continue),
  strict-mode abort path; the cx recovery helper.
- Sibling-delta application inside the loop (`state.derived(&delta)`), per §2 convention.

**Tests:** run against both `StdTokenReader` and `TokenListReader`; chars/whitespace/
paragraph/comment shapes (spans exact); stop conditions of every kind incl. predicates and
stop-after-one-node; tolerant vs strict on tokenizer errors; delta application between
siblings (test-lang ext change mid-stream).

**Exit:** content-only parses produce invariant-correct trees; `StopCause` semantics
demonstrated.

### 6.3 — Groups + invariant checker

- `GroupOpen` arm: group parsing — derived state (`expecting_group_close` from the token's
  `Arc<GroupRule>`), recurse `NodesParser` with the `GroupClose` stop condition carrying
  **both** `group_type` and `close` from that opening rule (pass both — matching either
  alone re-admits the `]`-closes-`{` / reclassified-`}` bugs the §2 note describes),
  consume the close, stage `Group` with `GroupData` (delimiters `Spanned`, `group_type` =
  rule class). Structural state revert after the group.
- Unclosed group (EOF inside): diagnostic + empty `close: TextContent` recovery (per §3.5
  `GroupData` rationale). `UnexpectedGroupClose` at root: diagnostic + skip (decision 9).
- `check_tree_invariants(&NodeTree)` public test utility in `node` (report R5): sibling
  spans partition content interiors (`List`/`Group` interiors, root); `TextContent::Spanned`
  residency; region tiling and children-block span-contiguity for callables; single-parent/
  reachability sanity. Apply it retroactively to all 6.2 tests.

**Tests:** nested groups, ambiguous delimiters (`$…$`-like via `expecting_group_close`),
unclosed/stray-close recovery in both modes, checker over every tree in the suite.

**Exit:** groups round-trip with exact spans; checker green over the whole test corpus.

### 6.4 — Invocation dispatch + escape hatch

- `Invocation` finalized; `CallableSpec::make_invocation_parser` added with default
  `StdInvocationParser` (this subphase: zero-argument, zero-slot callables — name from
  token, owned; empty `ParsedArguments`/`ParsedSlots`; post-space claim).
- `Command` arm: `Lang::resolve_command`; `None` → diagnostic + span-backed chars-node
  fallback (decision 9). `Specials` arm: spec from token, same factory path.
- ~~`claim_post_space(cx)` helper (peek + `move_to(tok, rewind_pre_space = false)`,
  whitespace only, stops at paragraph breaks) — invariant 3.~~ *(Superseded during 6.4,
  user decision: `post_space` = exactly the trigger token's own syntactic post-space;
  nothing is ever claimed beyond it — no helper. Invariant 3 amended in
  DESIGN_RATIONALE §3.5.)*
- After-invocation delta propagation: parser returns delta; `NodesParser` applies for
  subsequent siblings. `\newcommand`-shaped test (push-library delta; later sibling
  resolves against it).
- Escape-hatch proof (C6 obligation): a test-lang spec whose factory returns a custom
  parser (consumes tokens up to a marker, stages a custom node shape, returns a delta).
  True raw-content verbatim is Phase 7.

**Tests:** simple macros and specials end-to-end through the loop; post-space placement
(incl. paragraph-break cutoff); unresolvable-command recovery; the takeover parser;
`finalize_node` populating callable ext from spec data (FLM pattern rehearsal).

**Exit:** the full dispatch loop is live for argument-less callables; escape hatch proven.

### 6.5 — Arguments

- Grow `ArgumentParser`'s entry point (spec side, `&self` — tier 1):

  ```rust
  pub struct ParsedArgumentNodes {
      pub nodes: Vec<BuildId>,            // the region's nodes, in source order
      pub content: ContentNodes,          // designation relative to this region
  }
  fn parse_argument(&self, cx: &mut ParseContext<'_, '_, L>, spec: &ArgumentSpec<L>)
      -> ConstructParserResult<Option<ParsedArgumentNodes>>;   // None = absent, NOTHING consumed
  ```

- Noise-scan helper shared by standard parsers (whitespace/comment nodes staged ahead of
  the argument; on absent: `move_to` rewind to the first noise token, staged nodes left
  unclaimed for the builder to drop) — §3.5 noise-ownership contract.
- Standard argument parsers in `src/constructs/` (core, parameterized by group
  types/rules — preset one-liners are Phase 7): delimited-group (mandatory; falls back to
  single expression — `\frac12`, `\frac1\alpha`, with pylatexenc's
  requires-arguments-in-single-token diagnostic), optional-group (mints its `GroupRule`
  via a momentary state delta), marker (`*`, staged as a `Chars` content node), and the
  underlying `ExpressionParser` (single node: group / full invocation / single char).
- Per-argument `parsing_state_delta` applied via `derived()` around the argument's extent,
  noise scan included; structural revert after.
- `StdInvocationParser` completed: iterate `spec.arguments()`, run each argument's parser,
  assemble the child list + `ParsedArguments` (staged `ChildRegion`s from the returned
  offsets), missing-mandatory recovery (absent + diagnostic).

**Tests:** `\frac{a}{b}`, `\frac 1 2`, `\frac1\alpha`, `\section*`, `\item[label]`;
absent-optional rewind (probed noise re-parsed as enclosing content — the probe/rewind
case from §3.5); comments between arguments (region nodes); inner-group content
designation (`[{arg with ]}]` → `InChildrenOf`); per-argument state delta (e.g. rules
override active only inside the argument); regions verified by `check_tree_invariants`.

**Exit:** declarative argument parsing at pylatexenc parity; records exercised end-to-end.

### 6.6 — Environment bodies + end-to-end suite

- `EnvironmentBodyParser` (core, `src/constructs/`): constructor params
  `{ stop_command_name, name_group_rule/type, match_invocation_name: bool }`. Runs
  `NodesParser` with the `Command` stop condition under the slot's `parsing_state_delta`
  (derived; structural revert); on stop: consume the terminator command, require the name
  group as the **immediately next token** (rigid syntax, decision 6), extract its
  chars-only content, verify the back-reference. Stages the body `List` (span = content
  interior; empty body = empty `List`). Recovery per decision 8: mismatch → diagnostic +
  close **without consuming**; malformed → diagnostic + consume + close; `EndOfInput` →
  missing-terminator diagnostic + close.
- Environment-shaped invocation flow: arguments (6.5 machinery) + body + `ParsedSlots` +
  post-space claim, composed behind a spec's `make_invocation_parser` (test-lang
  `EnvironmentSpec` analog; promote the composition to a core helper only if it proves
  nontrivial — implementation freedom).
- End-to-end suite (§G of the old notes): hand-rolled test lang (`\`-commands, `{}`/`[]`
  groups, `%` comments, a `~`-like specials, `\frac`, `\section*`, an environment pair)
  over realistic snippets; assert tree shapes, exact spans (`check_tree_invariants` over
  every tree), diagnostics, post-space placement, paragraph-break nodes.

**Tests (additional):** nested environments; mismatch unwinding
(`\begin{A}…\begin{B}…\end{A}` → B diagnosed + closed, A consumes its terminator); orphan
`\end` at root (unresolvable-command recovery); environment body in a group / group in a
body; verbatim-style body via the takeover hatch; scaffolding reconstruction check
(node span minus children block == `\begin{name}` prefix + `\end{name}` suffix).

**Exit:** the full §G matrix green; every decided behavior demonstrated by a test.

### 6.7 — Closing documentation, quarry, benchmark

- **Benchmark check (flagged in DESIGN_RATIONALE §3.6, decision 2):** measure
  per-invocation `Box` cost — an `#[ignore]`d timing test (zero-dep; `std::time` in tests
  is fine) parsing a macro-heavy document, compared against a hand-inlined default path.
  Record the result in the §3.6 *Revisit if* note (either "measured, negligible" or open
  an issue to special-case the default path).
- NAMING_STRATEGY.md: add the §3.10 Phase 6 names.
- ARCHITECTURE.md §9: mark Phase 6 ✅ with the shipped/deferred split (mirroring earlier
  phase entries).
- DESIGN_RATIONALE.md: record any implementation-forced deviations that were approved
  during 6.1–6.6 (each as an amendment to its entry); confirm no §6 open questions were
  invalidated.
- Quarry (C7): `src/parser/{mod,minparser}.rs` → `*_JUNK._rs` (the constructs quarry was
  renamed in 6.1). Nothing deleted.
- The user removes `PreliminaryPlanNotesArchitectureExecutionPhase6.md` and, when Phase 6
  is accepted, this file's progress table gets its final ✅ row.

**Exit:** docs consistent with the implementation; `cargo build && cargo test` green;
Phase 7 (latexlike preset) unblocked.

---

## 4. Deferred out of Phase 6 (unchanged decisions, for orientation)

- `Language<L>` runtime bundle + `parse()` convenience entry; type-id interning (with it).
- `VerbatimParser` + verbatim argument forms → Phase 7 (`\verb`); the hatch is proven in
  6.4/6.6 by test-lang takeover parsers.
- Preset one-liner spec constructors (`MacroSpec`/`EnvironmentSpec`/`SpecialsSpec`),
  standard-library population → Phase 7.
- Content-extraction views (chars flattening, keyval, split) → Phase 7 work package
  (report R7); `NodeRef` region accessors exist since the regions session.
- Transform/visitor APIs; synthetic-source registry beyond the skeleton; level-2
  recomposer *implementation* (Phase 6 only guarantees the records/reconstructibility).
- Multi-slot separators (fence blocks with `+++`) — first real consumer decides.
- Temporary group rules scoped in state data (depth-N bracket protection,
  `\item[a[b{c]}]]`) — direction decided July 2026, recorded as the *Revisit* note on
  the balanced-optional-arguments entry (DESIGN_RATIONALE §3.6); seam + stripping
  semantics ship together in a later session.
