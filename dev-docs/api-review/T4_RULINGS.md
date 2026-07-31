# Phase 2b — T4 session: interim rulings (full working detail)

Session started 2026-07-31. Brief: T4_BRIEF.md (verified against 9643d7d). Updated
every round; durable DESIGN_RATIONALE entries + PLAN.md decision-log line at session
close. All rulings are the user's.

## B — `\input` wiring + resolver move (RULED round 1; sketches → round 1b)

**Driver `Copy`/`Eq` RULED (user): drop them — no clear reason to keep.**
"Why would we want Copy/Eq on the driver?" — no in-crate reliance exists (verified,
brief B1); shipped drivers keep `Clone + Debug` only. Amends T3_RULINGS D
("manual impls keep `Copy`/`Eq`" clause struck) → amendment note on the T3-D durable
record ([§dd-dr:preset-driver-pillars]) at session close. Resolves the brief's
stale-claim #4 collision in P4's favor.

**B1 RULED: option 1** — defaulted trait accessor
`ParseDriver::source_resolver(&self) -> Option<&dyn SourceResolver<L::SourceOrigin>>`,
default `None` ("this language resolves nothing"); shipped drivers
(`StdParseDriver`, `ScopesResolvingDriver`, `LatexlikeDriver<LLL>`) gain an
`Option<Arc<dyn SourceResolver<…>>>` field + `with_resolver(…)` builder. `None`
replaces the `NoResolver`-by-default role (`NoResolver`'s own fate → Tier-C batch,
lean keep).

**B2 RULED with user amendment: the door takes the construct parser.**
`cx.parse_attached_source(source, state, parser)` — the caller specifies which
parser drives the sub-parse (not a hard-coded root nodes-parser drive); exact
parameter type at application (the parser-invocation vocabulary; the default choice
for `\input`-style inclusion is the root nodes-parse shape). Accepted as briefed:
name `parse_attached_source`; fresh inner context internals (reader lifetimes);
stages content nodes only — slot assembly stays the invocation parser's job (one
staging door holds); local stray-close recovery (never unwinds the includer);
resolution stays OUTSIDE the door (B1 accessor → free `resolve_source` → door; the
free fn becomes the canonical composition once `Language::resolve_source` leaves);
new core condition **`UnresolvableSourceReference`**, identifier
`core.sources.unresolvable-reference` (A slate row R1); traceback `Frame` pushed by
the door.

**B3 RULED: option 1 — hold the contract; recursion control is NOT core's job**
(user; rejects the depth-knob recommendation). Decisive addition (user): legitimate
self-inclusion exists — e.g. `.dtx` self-documenting files include themselves; a
core depth/cycle mechanism would fight real documents. A-slate row R2
(include-depth-exceeded) is DROPPED. **Instead (user): provide a public general
helper** implementing the logic consumers will most likely want to invoke directly
(rather than write manually) in their resolvers; to identify it, sketch a
consumer-side std file-I/O resolver incl. provenance-based cycle detection and
extract the general parts (round 1b). The helper must be no_std-clean (techy ships
no I/O).

**B4 RULED (direction): preset ships an opt-in `\input` `MacroSpec` constructor**
(never preloaded; `LLL`-generic; embedders insert it into their own package).
**User requirement**: the *logic* `\input` implements must be writable in very
brief form with good helpers — decoupled from the specific one-argument form, so
`\input[options]{file}` or `\input*{file1,file2,file3}` variants are easy custom
spec work. Sketch the preset spec + the helper decomposition (round 1b); final
constructor name at session close (working name `input_macro_spec()`).

**Round 1b (B sketches → rulings):**
- **`Source::including_sources()` RULED: accept** — iterator over the chain of
  including sources (self → primary, following `provenance().triggered_at()`
  hops); the general primitive under every cycle/depth/counting policy.
- **Two failure conditions RULED (user)**: "no resolver configured" and "resolver
  returned an error" are genuinely different — two condition types, not one with
  detail. The resolver-error condition must preserve the underlying cause (e.g.
  `io::Error`). Constraint verified: `DiagnosticInfo: Any + Clone + …`
  (error.rs:51) while `ResolveError` is deliberately not `Clone` — so the
  condition stores **`Arc<ResolveError>`** (Clone ✓, live cause preserved for
  downcast; `serializable_data` renders message + cause chain). Names/identifiers
  proposed round 1b, confirmed with A's slate.
- **`attach_source_reference` RULED: accept as sketched** (user; "tiny" musing
  answered by the single-raising-site argument — one wording for both conditions
  across every `\input`-variant spec and framework). Home: pending the
  `techy::helpers` question (round 1b).
- **Cycle/depth helper (user proposal)**: single helper
  `(reference, triggered_at, key_fn, max_depth)` wrapping the walk — accepted in
  principle; name + home + keying design settled round 1c.

**Round 1c (B CLOSED):**
- **`check_include_chain` RULED** (name accepted): `check_include_chain<O, K:
  PartialEq>(target_key: &K, triggered_at: &SourceSpan<O>, origin_key: impl
  Fn(&O) -> Option<K>, max_depth: Option<usize>) -> Result<(), ResolveError>`.
  Keying design (user-driven): compare **origins**, not provenance reference
  strings — the primary source participates (embedder mints it with a suitable
  canonical name — user's point); the caller passes the already-canonicalized
  target key (the resolver computes it during resolution anyway); `origin_key`
  is a cheap conversion when the resolver mints canonical origins (documented
  invariant); `None` key = skipped. Distinct messages for cycle vs depth-exceeded.
  Home: **`techy::source`**.
- **`techy::helpers` REJECTED** (util-grounds: vague-name bucket vs placement by
  logical function; adding a module later is additive, dissolving one is
  breaking). `attach_source_reference` home: **core, beside
  `parse_attached_source`** on the `ParseContext` surface.
- **`ResolveError` cause → `Option<Arc<dyn Error + Send + Sync>>` RULED** (user
  proposal): `ResolveError` derives `Clone` again; principle recorded — **techy
  error types stay uniformly `Clone`; out-of-crate information sits behind the
  `Arc`**. `Error::source()` downcast path unaffected (verified); `with_cause`
  wraps with `Arc::new`. The failure conditions store plain `ResolveError`.
  Amendment (reversal note) on [§dd-dr:resolver-contract] at close.
- Two condition names confirmed with A: `NoSourceResolver`
  (`core.sources.no-resolver`), `UnresolvableSourceReference`
  (`core.sources.unresolvable-reference`, payload reference + `ResolveError`).

## C — FS-trait closure (RULED, round 2)

**`SourceResolver` IS the minimal filesystem-interface trait** (verified: techy
consumes exactly reference→content — never list/stat/watch); **techy ships
nothing** — the PLAN companion-section question closes as "already answered by
the existing seam"; the ~20-line std resolver (using `check_include_chain`)
lands as a doc-tested recipe in Phase 4's include chapter. Rejected: std-gated
`FsResolver` (freezes path/sandboxing policy under P5; breaks the zero-features
no_std claim); a separate open/read FS trait (second abstraction, no techy-side
consumer). PLAN companion bullet updated at session close.

## A — wire-identifier rename slate (RULED, round 3)

**User amendment: area `specs`, not `resolution`** ("resolution of what?" — also
fixes the latent ambiguity against source resolution / `core.sources.*`; wire
vocabulary now tracks the public `core::specs` home the H ruling gave the
family). **`scopes` area merges into `specs`** (supersedes the P5 entry's
illustrative example list — noted in the durable record). Segment policy RULED:
**keep segments** (minimal diff; self-descriptive alone) — and the one stutter
case dissolves under the specs area. Judgment calls ruled as recommended
(expression pair → `arguments`; `repeated-tack-on-field` segment rename;
keep-flags confirmed).

**THE FROZEN SLATE** (identifier level; lands in Phase 3 before guides print):
- `core.specs.unresolvable-command` (was `core.nodes_parser.unresolvable-command`)
- `core.specs.command-resolution-failed` (was `core.nodes_parser.command-resolution-failed`)
- `core.specs.callable-defined-as-error` (was `core.scopes.callable-defined-as-error`)
- `core.specs.scope-op-failed` (was `core.constructs.scope-op-failed`)
- `core.groups.unclosed-group` (was `core.group_parser.unclosed-group`)
- `core.groups.stray-group-close` (was `core.nodes_parser.stray-group-close`)
- `core.environments.terminator-mismatch` / `.malformed-terminator` /
  `.missing-terminator` (were `core.environment_parser.*`)
- `core.arguments.missing-mandatory-argument` / `.expected-expression-argument`
  (were `core.argument_parsers.*`)
- `core.arguments.expression-callable-requires-content` (was
  `core.nodes_parser.expression-callable-requires-content`)
- `core.arguments.repeated-tack-on-field` (was `core.tack_on_parser.repeated-field`
  — segment renamed, vague outside its own area)
- `core.recovery.unusable-recovery-token` (was `core.nodes_parser.unusable-recovery-token`)
- `core.verbatim.unterminated-verbatim` / `.expected-verbatim-delimiter`
  (were `core.verbatim_parser.*`)
- KEEP: `core.token.end-of-stream-after-escape`, `core.token.forbidden-char`,
  `core.constructs.implementation-error`, `latexlike.environments.*` ×3.
- NEW (B): `core.sources.no-resolver`, `core.sources.unresolvable-reference`.
- RESERVED (T1/T2 A1(iv) warning): `core.specs.<segment>` — suggested
  `provider-commands-shadowed-by-escape`, wording at application.
- DROPPED: R2 include-depth-exceeded (B3 ruling).

Consumer impact: zero for typed (`T::IDENTIFIER`) consumers; in-crate churn = 22
attribute strings + identifier-asserting tests. Durable record: applied-slate note
on [§dd-dr:wire-identifier-stability] at session close.

## E + D — navigation naming + cursor reconciliation (RULED, round 4)

**E RULED — the naming table as proposed**: `NodeTree::node_at(&SourcePos)` →
`Option<NodeRef>`; **`NodeTree::covering_slice(&SourceSpan)`** → `Option<NodeSlice>`
(over `slice_at`/`nodes_covering` — the name carries the may-cover-more fact);
`NodeRef::parent()` / `index_in_parent()` → `Option` (P4 names confirmed);
`SourcePos` accessors `source()`/`pos()` (`pos` over `offset` —
`TokenReader::pos()` precedent); `SourceSpan::start_pos()`/`end_pos()`
(exclusivity doc sentence); `NodeRef::tree()` goes pub (P4 point 6 confirmed);
**`Span::contains(pos)` added now** — `node_at` is the consumer
[§dd-dr:span-extend-to] deferred for; ruled empty-span semantics (never match);
`overlaps` only if `covering_slice`'s impl wants it. Homes: lookups on `NodeTree`;
`SourcePos` in `techy::source` beside `SourceSpan`.

**`ancestors()` REJECTED (user)**: tree visiting is top-down; `parent()` covers
the upward hop and an ancestry walk has zero trap surface
(`iter::successors(node.parent(), |n| n.parent())` — correct first try), so the
shorthand bought only a combinator at the price of a permanently-stable iterator
type. The one-line recipe goes in `parent()`'s rustdoc; `ancestors()`/`Ancestors`
→ superseded-names (consciously rejected).

**D RULED**: cursor-vocabulary reconciliation recorded — the retired
`SourceCursor` (char-scanning content cursor, [§dd-dr:source-cursor-retired]) and
F7's editor-cursor node lookup are disjoint concepts sharing a word; one
clarifying sentence lands in the [§dd-dr:tree-navigation] amendment. All four
walkthrough subtleties verified covered by P4's ruled semantics; **F7 closes**
via `node_at` + `parent()` (no `ancestors()`).

## F — wishlist sweep (RULED, rounds 5–5b)

- **26 RULED**: `LineIndex::line_of(offset) -> Option<(usize, Range<usize>)>` —
  line number (line_col's numbering conventions) + byte range (user amendment:
  number included; range alone would force a second call). `line_range(line_no)`
  SKIPPED (no demonstrated consumer; additive later).
- **27a RULED**: `line_col_span(impl Into<Range<usize>>) -> Option<((l,c),(l,c))>`.
- **27b/27c/F6 RESOLVED by the ownership design (round 5b)**:
  - Lang-coupled options REJECTED (Source-as-Lang-trait / `SourceAnalyzer`
    associated type): the source model is deliberately Lang-free
    ([§dd-dr:origin-genericity] is load-bearing — error.rs rendering and T4
    tooling are Lang-free; same Arc<Source> under two Langs would duplicate
    work; data-vs-traits: precompute/lazy/interior-mutable are strategies of one
    pure function). Source-owned lazy cache REMAINS blocked dep-free (recorded
    at error.rs:699–700: alloc has no Mutex, OnceCell !Sync).
  - **`techy::source::LineIndexCache<O>` NEW**: public persistent cache — one
    owned line-starts table per source, keyed by Arc identity (entries own
    Arc<Source> + Vec<usize>, not the borrowing LineIndex view); API mirrors
    line_col/line_of/line_col_span. Layered responsibility recorded: parse
    computes nothing ([§dd-dr:lazy-line-col] holds); renderer keeps its
    per-call cache; persistence = whoever holds a LineIndexCache (valid forever
    — content immutable; editors keep their own Arc across parses per the
    span-stability doctrine). `&self`-vs-`&mut` moot (consumer-held; own lock
    std-side if shared; techy buys no no_std sync).
  - **Provider trait NEW (user, round 5c)**: the reporting machinery accepts
    `&mut impl <provider trait>` (`_with` variants on render entry points;
    no-arg forms = transient-cache shorthand) — single method
    `line_col(&mut self, source, offset) -> Option<(usize, usize)>`;
    `LineIndexCache` implements it; editor tools plug incremental caches that
    survive edits (Arc-keyed recompute answered at the right layer). Trait name
    at sweep: `LineColProvider` (recommended — provides line/col answers, not
    caches) vs user's `LineIndexCacheProvider`.
  - Per-node `line_col()` methods REJECTED (hidden per-call index build,
    O(k·N)); bind-the-Arc one-index pattern = guide example (incl. E0716
    gotcha).
  - **F6 RULED**: `DEFAULT_MAX_SCAN_LEN` 100_000 → **500_000** (user;
    line-starts table ≈ 100 KB for 500 KB text — still bounded);
    `set_max_scan_len` kept; loud docs on LineIndex + line_col + tooling
    chapter; `Option` returns kept (no Result split).
- **28 RULED**: caret renderer REJECTED for techy (presentation policy;
  ~10-line hand-roll once 26+27a exist); `format_position` shape documented
  as NOT a contract (doc-only).
- **29 RULED**: `Descendants::with_depth()` REJECTED — it patched flat
  iteration's structure loss. The honest structural read is an enter/exit
  walker (`enter(node, depth) -> VisitFlow{Descend, SkipChildren, Stop}`,
  `exit(node)`) — NOT transform (restage rebuilds; wasteful for read-only) but
  the skeleton of recompose → **routed to the recompose design session** so the
  walk vocabulary is designed once. `descendants()` stays (flat iteration is
  legitimate for structure-free queries).
- **23 + doc wishes CONFIRMED for Phase 4**: identifier↔type guide table
  (post-slate identifiers only), span-stability paragraph, bind-the-Arc
  LineIndex example, include chapter.

## Sweep (RULED, round 6 — session complete 2026-07-31)

- Resolved-by-prior list confirmed (user); nothing re-litigated.
- **Names RULED (user)**: provider trait **`LineColProvider`**; preset constructor
  **`input_macro_spec()`**.
- **T5 handoffs** (routed to PLAN): restage detailing + `stage_invocation`
  signature; FLM probe acceptance; driver knobs / extension seam — now incl. the
  resolver field as a new datum; pillar-signature sufficiency; honest-slice /
  validator application details; `\input` splice-a-cached-parse affordance
  question.
- **Recompose-session handoffs**: the read-only structural walker (enter/exit,
  depth, `VisitFlow`); the verbatim strategy's `Attached`-exclusion rule.
- **Tier-C handoffs**: `NoResolver` (lean keep); `ProvenanceChain` /
  `ResolvedContent` placements; free `resolve_source` flipped to canonical.
- Durable records written this session: new DESIGN_RATIONALE entries
  **[§dd-dr:input-wiring]**, **[§dd-dr:include-chain-helpers]**,
  **[§dd-dr:line-col-ownership]**; amendments on [§dd-dr:wire-identifier-stability]
  (applied slate), [§dd-dr:resolver-contract] (ResolveError Clone + recursion
  clause), [§dd-dr:language-init] (collapse complete), [§dd-dr:tree-navigation]
  (names + cursor vocabulary + ancestors rejection), [§dd-dr:span-extend-to]
  (contains consumer), [§dd-dr:preset-driver-pillars] (Copy/Eq strike),
  [§dd-dr:recompose] (walker routing), [§dd-dr:source-resolver] (wiring landed),
  [§dd-dr:lazy-line-col] (ownership pointer); superseded-names T4 block;
  ARCHITECTURE footer refs (source + engine sections).
