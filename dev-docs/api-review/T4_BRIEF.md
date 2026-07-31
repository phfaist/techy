# Phase 2b — T4 Decision-Session Brief (tooling-author persona)

Prepared 2026-07-31. Inputs: PLAN.md decision log (P1–P5 + T1/T2 + T3 rulings all
binding), POLICY_BRIEF.md §Routing (T4 line), P4_RULING.md points 7–9 + deferred
agenda, SYNTHESIS.md (F6/F7/F8/F9, wishes 24–29, §3), walkthroughs/tooling/
(+ the surviving runnable project, task2_position.rs cited below), DESIGN_RATIONALE
entries as cited. Every code claim re-verified against the working tree at commit
9643d7d (file:line cited; paths relative to `techy/src/`). The brief recommends; all
rulings are the user's.

**Reading key — unapplied rulings.** The code is still pre-application for
P2/P3/P4/P5/E4 and the T1/T2 + T3 sessions (verified: `Language::new(driver)` is
one-arg with `Default`/`with_provider`/`with_seed_delta`/`with_resolver` all present,
engine/language.rs:81–136 + :286–293; `CommandResolution::resolve_via_scopes` is
still the associated fn, engine/driver.rs:446; no `SourcePos`, `parent()`,
`index_in_parent()`, or `node_at` anywhere in node/ (grep); the `finish()` parent
table is computed then dropped, node/builder.rs:234 → :270; `NodeId`'s tree tag is
debug-only, node/tree.rs:34–40; `ParserSession::builder` is still `pub`,
engine/mod.rs:141). Every "current code" citation is the pre-ruling state, with ruled
changes layered on explicitly. Constraints honored: one canonical path +
specs=author-side/hub=run-side (P1); fix-the-real-API (P2); pillar composition + `LLL`
(P3); the [§dd-dr:transform] entries (P4); one stability class, soft freeze,
identifiers semver-stable with concept-named areas (P5); the T1/T2
shorthand-not-second-path principle. Format per point: Context → Evidence →
Options → **Recommendation** → Cost.

**Stale-claim ledger (found while verifying; details in the points):**
1. P5 decision log / PLAN / [§dd-dr:wire-identifier-stability]: "**14 of 18** core.*
   identifiers use internal file names" — the verified total is **19** core.*
   production identifiers (14 file-named + 5 concept-named); `tack_on_parser`
   predates P5 (commit b747896), so this was a miscount, not drift. The *14
   file-named* half is exact.
2. SYNTHESIS §3 counts `UnclosedGroupFound`, `MissingTerminatorFound`,
   `UnusableRecoveryTokenKind` among "condition types" — they are
   `ToDiagnosticValue` payload enums with no identifiers (group_parser.rs:70,
   environment_parser.rs:111, nodes_parser.rs:195). Harmless for the slate (A
   inventories identifier-bearing types only).
3. SYNTHESIS wish 24 / POLICY_BRIEF P4 line spell the lookup `node_at(offset)` —
   superseded by P4's `node_at(&SourcePos)` ([§dd-dr:tree-navigation]); expected,
   noted so nobody re-imports the offset-only signature.
4. T3_RULINGS/D records "manual impls keep `Copy`/`Eq`" for `LatexlikeDriver<LLL>` —
   in tension with the P4 resolver-move if the driver stores an
   `Arc<dyn SourceResolver>` (B1 below); one of the two needs an amendment.

---

## B. `\input` engine wiring + resolver move (P4 rider; hard structural)

**Context.** P4 point 9 ruled the shape ([§dd-dr:input-attachment]): the callable's
spec parser resolves the reference and **sub-parses the resolved source into the same
builder**, staged as an **`Attached` slot**; multi-source parse trees are first-class;
**the resolver moves `Language` → driver**. T4 designs the wiring: the driver-side
resolver surface, the `ParseContext` sub-parse door, diagnostics/span behavior,
recursion policy, and the `Language::with_resolver` removal ([§dd-dr:language-init]
amendment expects the surface to collapse toward the constructor alone).

**Evidence — the current wiring (verified).**
- The resolver lives on `Language`: field `resolver: Arc<dyn
  SourceResolver<L::SourceOrigin>>` (engine/language.rs:75, default `NoResolver`
  :85), `with_resolver` (:130–136), accessor `resolver()` (:150), and
  `Language::resolve_source` (:159–165) wrapping the free fn `resolve_source`
  (source/resolver.rs:100–110 — mints the `Source`, stamps per-include-site
  provenance).
- **Nothing reaches the resolver mid-parse**: `ParseDriver` has no resolver access
  (the full hook surface is driver.rs:72–333 — recovery, recover, probe_token,
  resolve_command, make_paragraph_break_node, refine_diagnostic, observe_transition,
  group_interior_delta, three factories; E4 adds `resolve_state_event`), and
  `ParseContext` carries `{tokens, source, state, session, driver}` only
  (constructs/mod.rs:108–129). The only workflow is the embedder loop the T4
  walkthrough wrote (tooling/FRICTION.md Task 3) — an uncorrelated forest.
- The sub-parse mechanics the ruling anticipates already exist in pieces:
  `Language::parse_source` is exactly the root-drive shape (reader over
  `source.content()`, `parse_nodes` to `EndOfInput`, stray-close diagnose-and-skip,
  engine/language.rs:197–269); `BuildId`s are session-global, so nodes staged during
  a nested context land in the same builder; slots designate regions of the
  callable's own child list, so sub-parsed nodes become the `\input` callable's
  children with spans in the resolved source — precisely the sibling-coherence story
  the entry records.
- Reader lifetimes force a **fresh inner context**, not a field swap:
  `cx.tokens: &'a mut dyn TokenReader<'s, L>` pins `'s` to the *outer* source's
  content (constructs/mod.rs:110); a resolved source's `&str` has a shorter, local
  life. `ParseContext::new(&mut new_reader, new_source, state, session-reborrow,
  driver)` inside a scoped method is the workable mechanic (the entry's
  revisit-clause concern does not materialize).

### B1. The driver-side resolver surface

**Options.**
1. **Defaulted trait accessor** — `ParseDriver::source_resolver(&self) ->
   Option<&dyn SourceResolver<L::SourceOrigin>>`, default `None` ("this language
   resolves nothing"); shipped drivers (`StdParseDriver`, `ScopesResolvingDriver`,
   `LatexlikeDriver`) gain an `Option<Arc<dyn SourceResolver<…>>>` field + a
   `with_resolver(...)` builder. Pro: one uniform seam; generic machinery (the B2
   door) needs no downcast; `None` replaces the `NoResolver`-by-default role.
   Con: **the field kills `Copy` (and honest `Eq`) on every driver that carries
   it** — `LatexlikeDriver` is `Copy + Eq` today (latexlike/driver.rs:63–69) and
   T3's D ruling explicitly kept `Copy`/`Eq` on `LatexlikeDriver<LLL>`
   (T3_RULINGS D). One of the two gives way (stale-claim #4).
2. **Behavior method, not accessor** — `ParseDriver::resolve_reference(&self,
   reference, triggered_at) -> Result<ResolvedContent<…>, ResolveError>`, default
   = fail (the `NoResolver` body). Pro: the driver *is* the resolver seam — no
   stored object mandated, a driver may compute/delegate however it likes; matches
   the hook style of the rest of the trait. Con: drivers that just want to *carry*
   an embedder resolver still need the storage field (same `Copy` cost), and
   composition loses the object identity (`resolver()`-style introspection gone —
   nothing needs it today).
3. **Resolver stays a per-parse argument** (e.g. `parse_source_with(resolver, …)`).
   Rejected out of hand: re-litigates P4's ruled direction; also wrong-shapes the
   nested case (the *construct parser* needs the resolver mid-descent, and it holds
   only `cx`).

**Recommendation: 1**, with the `Copy` cost accepted and recorded: shipped drivers
keep `Clone + Debug`; `Copy`/`Eq`/`Hash` are dropped where the resolver field
lands (amendment note on the T3 D record; nothing in-crate relies on driver
`Copy` — verified: no `*driver` copy sites outside tests). Option 2 is the
fallback if the user prefers zero stored state on the trait side; it does not
avoid the `Copy` cost for the shipped drivers, which is the only real cost.
The accessor's `Option<&dyn …>` default also cleanly answers "what replaces
`NoResolver`-as-default" — `None` does; `NoResolver` itself becomes a Tier-C
question (an explicit always-fail value some embedder slots may still want;
lean keep, rule in the Tier-C batch).
**Cost.** `Copy`/`Eq` loss on resolver-carrying drivers; a 12th defaulted trait
method (13th with E4's `resolve_state_event`).

### B2. The `ParseContext` sub-parse door

**Sketch (recommended).** One new method on `ParseContext`:

```rust
pub fn parse_attached_source(
    &mut self,
    source: Arc<Source<L::SourceOrigin>>,
    state: Arc<ParsingState<L>>,        // the state AT the \input point (caller's choice)
) -> ConstructParserResult<L, Vec<BuildId>>
```

Internals: mint a `StdTokenReader` over `source.content()` (the
engine/language.rs:201 precedent), build an inner `ParseContext` sharing
`self.session`/`self.driver`, and run the root-drive loop shape
(`parse_nodes(StopSpec::none())` to `EndOfInput`; stray group close inside the
included source is diagnosed-and-skipped *locally*, exactly like
language.rs:230–253 — an included file's stray `}` must not unwind the includer).
Returns the staged sibling ids; the calling invocation parser appends them to its
children and covers them with a `ParsedSlot` whose role is `Attached` (P4's
[§dd-dr:slot-roles]). It deliberately does **not** stage the slot itself — slot
assembly stays the invocation parser's job (one staging door, P4 point 3; this
method stages nothing but content nodes, all through the normal descent machinery).

- **Naming.** Candidates: `parse_attached_source` (teaches the ruled pattern — the
  result is Attached-slot material; recommended), `parse_source_nodes` (honest but
  vague about the multi-source point), `sub_parse_source` ("sub" says nesting, not
  source-switching — weaker). `cx.parse_source(...)` rejected: collides
  head-on with `Language::parse_source` in reader vocabulary while behaving
  differently (returns ids, not a result) — the sibling-vocabulary rule
  ([§dd-arch:naming] principle 4) says don't reuse the name for a different
  contract in the same conceptual scope.
- **Resolution stays outside the door**: the construct parser calls the B1 accessor
  → `resolve_source(resolver, reference, cx-span)` (the free fn, source/resolver.rs:100,
  becomes the one canonical composition once `Language::resolve_source` leaves — the
  INVENTORY "redundant free fn" flag resolves in the free fn's *favor*) → then
  `parse_attached_source`. Keeping resolve and parse separate lets caching
  frameworks substitute either half (the entry's separate-parse-then-splice option
  stays open by construction).
- **Failure condition (new, core-defined).** A failed resolve must become a
  diagnostic: tolerant = callable staged with the argument recorded and no attached
  content + diagnostic; strict = abort through the recover funnel. Condition type
  candidates: **`UnresolvableSourceReference`** (mirrors `UnresolvableCommand`;
  payload `reference: String`, `detail: Option<String>` from
  `ResolveError::message`; recommended) or `SourceResolutionFailed`. Identifier
  slotted in A's slate: `core.sources.unresolvable-reference`. Core declares it
  beside the door (producer-side, P1); the preset construct reuses it.
- **Tracebacks**: the door pushes a `Frame` (the [§dd-dr:parse-traceback]
  mechanism) so nested diagnostics render "while parsing ‹\input …›"; frame title
  via the existing spec-based `FrameTitle` route — no new machinery.
- **Spans/diagnostics across sources need zero work**: diagnostics recorded in the
  inner context carry `SourceSpan`s into the resolved source and the *same* session
  sink (session.recover, engine/mod.rs:352–366); include chains already render from
  provenance (walkthrough Task 3 verified `render`/`render_all` do this today).
  T1/T2's `sorted_by_position()` was designed source-major for exactly these trees.

### B3. Recursion/cycle policy

**Evidence.** [§dd-dr:resolver-contract]: "the core performs no recursion
checking"; `Source::provenance_chain` is the ready-made embedder tool
(source/source.rs:145–147). That contract was written when the *embedder* drove the
loop. Under B2 the recursion happens on **core's stack**: a resolver reachable from
its own output (`a.tex → \input{a.tex}`) now overflows inside techy — a crash on
input, which the panic policy forbids core to cause. Counter-evidence: unbounded
recursion on hostile input **already exists** — every group nesting level is a
`parse_nodes` recursion (deep `{{{{…` overflows today); no depth limit exists
anywhere, and `ParseContext` is the documented "one place to grow (depth limits,
cancellation)" (constructs/mod.rs:106–107).

**Options.**
1. **Hold the contract** — no core checking; the guide's resolver recipe (C) shows
   the depth bound (`triggered_at.source().provenance_chain().count()`), and the
   door's docs say loudly that recursion control is resolver business. Pro:
   consistent with the existing no-depth-limit posture (\input adds a new trigger,
   not a new hazard class) and with never-interpret-references; zero API. Con: the
   hazard is now nearer — an embedder who wires a naive FS resolver gets a crash
   from a 2-line document, where deep `{` nesting at least requires proportional
   input bytes.
2. **Include-depth knob** — a small limit consulted by `parse_attached_source`
   (default e.g. 64; a driver-side or door-side setting), exceed → recover-funnel
   condition (`core.sources.include-depth-exceeded`, reserved in A's slate). Pro:
   bounds core's own stack without interpreting references (a depth count is not
   reference semantics); honest Err instead of a crash. Con: a new knob + condition;
   partially duplicates what a careful resolver does anyway; the general
   nesting-depth question remains open either way (a full answer is the future
   `ParseContext` depth-limit work, out of T4 scope).
3. Provenance-chain cycle *detection* (same reference already in the chain) —
   rejected: references are opaque strings (two spellings can name one file), so
   this is a heuristic that gives false confidence; and it IS reference
   interpretation in the contract's sense.

**Recommendation: 2** — the narrow depth bound, framed as core bounding *its own
stack*, not policing references; record it as an explicit amendment note on
[§dd-dr:resolver-contract] ("no recursion checking" survives for reference
semantics; the engine bounds nesting of the sub-parse door it now owns). If the
user prefers 1 (contract purity + one hazard class), the guide recipe becomes
mandatory content, and the A-slate row for the depth condition is dropped.
**Cost.** One knob + one condition identifier; the default value is a judgment
call to rule (any bound deep enough for real documents, shallow enough to save
the stack — 32/64 both fine).

### B4. `Language` surface after the move + who ships the construct

- **`Language` collapses to `{driver, initial_state}`**: `with_resolver`,
  `resolver()`, and `Language::resolve_source` (language.rs:130–165) all leave;
  with P2/T1-T2/T3 already removing `Default`/`with_provider`/`with_seed_delta`/
  driver `Default`s, the surface is `new(driver, initial_state)` + `parse` +
  `parse_source` + two accessors — completing [§dd-dr:language-init]'s expected
  collapse. Completion note on that entry + [§dd-dr:source-resolver]'s wiring
  paragraph at session close.
- **The preset construct is opt-in, not preloaded.** T1/T2 slimmed `"_builtin"` to
  `\begin`/`\end` only; an always-on `\input` is useless-to-hostile without an
  embedder resolver (default `None` → every `\input` diagnoses). Recommend: the
  preset ships a spec constructor (working name `input_macro_spec()`; final name at
  session — nothing nearby in [§dd-dr:superseded-names]), documented in the include
  chapter; embedders insert it into their own package. `LLL`-generic per P3.
  Option (flag, don't rule): a core-generic canned argument parser
  ("resolve-then-attach") under it — lean **preset-only for now**; the core door is
  the reusable part, and a second generic layer before a second consumer is
  speculative.
- **Mixed-origin trees become producible by a public path** — closing the
  walkthrough's "node/mod.rs mentions mixed-origin trees; no public path produces
  one" gap (tooling/FRICTION.md Task 3), which in turn makes `node_at`'s per-source
  descent (E) and honest slices (P4) load-bearing rather than theoretical.

---

## C. F8 filesystem-trait option

**Context.** PLAN's companion section: leaning AGAINST a std-tools crate — "logic
stays in techy (no_std), embedder implements a minimal filesystem-interface trait
(SourceResolver pattern)". The T4 question: is `SourceResolver` already that trait,
and what (if anything) should techy ship?

**Evidence (verified).**
- `SourceResolver` is reference→content, nothing more: `resolve(&self, reference:
  &str, triggered_at: &SourceSpan<O>) -> Result<ResolvedContent<O>, ResolveError>`
  (source/resolver.rs:48–56), `Send + Sync`, object-safe (compile-time pin :60),
  forwarding impls (:65–93). `ResolveError` carries strings + optional structured
  cause for `io::Error` downcast (:145–178) — the std-error bridge already exists.
- techy consumes exactly one filesystem-shaped capability: *fetch content for a
  reference string*. It never lists directories, stats files, or watches — there is
  no second operation a broader FS trait would carry.
- The no_std posture is recorded twice ([§dd-dr:source-resolver],
  [§dd-dr:dependencies]) and restated in the module docs (source/mod.rs:40–44,
  resolver.rs:2–5): no shipped I/O resolver, the embedder implements the trait
  where the I/O capability lives. `MapResolver` covers tests/preloaded setups
  (:216–262).

**Options.**
1. **`SourceResolver` IS the trait; ship a guide recipe only.** A reference
  std impl in the include-workflow chapter (doc-tested `no_run`): root-dir
  resolution of relative references (optionally against the includer's origin via
  `triggered_at.source().origin()`), the B3 depth bound, `io::Error` via
  `with_cause`, and the explicit warning about `..`-traversal/sandboxing being
  embedder policy. ~20 lines.
2. Ship a std-gated `FsResolver` behind a `std` feature. Con: the crate currently
  has zero features and a clean no_std claim; path semantics, canonicalization,
  encoding of "root dir", and sandboxing policy are exactly the judgment calls the
  PLAN's anti-std-tools lean says frameworks own — a shipped impl freezes one
  policy forever under P5 for a component every serious embedder rewrites.
3. Define a *separate* minimal FS trait (open/read) that techy adapts into a
  `SourceResolver`. Rejected: a second abstraction with no techy-side consumer for
  its extra surface; `SourceResolver` already is the minimal interface (the PLAN
  wording "filesystem-interface trait (SourceResolver pattern)" resolves to
  "SourceResolver is that trait").

**Recommendation: 1.** Nothing ships in the library; the companion-section
question closes as "already answered by the existing seam". Record the closure in
the PLAN companion bullet at session end; the recipe lands with Phase 4's include
chapter (the walkthrough's single most-wanted doc addition, FRICTION Task 3).
**Cost.** None now; one guide chapter obligation (already queued as F8's doc
half).

---

## A. Wire-identifier rename slate (P5 rider)

**Context.** P5 ruled ([§dd-dr:wire-identifier-stability]): identifiers are
semver-stable wire material; scheme `<owner>.<area>.<condition>`; the `<area>`
segment **names a construct concept or subsystem, never a file/module/type name**;
first segment = defining vocabulary. The concrete slate was deferred to T4 because
the resolution-family conditions needed T3's [§dd-dr:resolution-extraction] to name
their concept. The slate lands with the Phase 3 application, before guides print
any identifier; pre-freeze, *both* segments are still freely renameable — this is
the last cheap moment.

**Evidence — complete verified inventory.** 22 production condition types crate-wide
(19 `core.*` + 3 `latexlike.*`; grep of all `#[diagnostic(id = …)]` attributes,
non-`#[cfg(test)]`; the only hand-written `DiagnosticInfo` impls are in test
modules — error.rs:869, engine/mod.rs:449). Of the 19 core identifiers, **14 use
internal file names as areas** (`nodes_parser` ×5, `environment_parser` ×3,
`argument_parsers` ×2, `verbatim_parser` ×2, `group_parser` ×1, `tack_on_parser`
×1) and 5 are concept-named (`token` ×2, `constructs` ×2, `scopes` ×1). P5's "14
of 18" total was a miscount (stale-claim #1); the file-named count is confirmed.
A load-bearing reading note: `token`/`scopes`/`constructs` coincide with module
names, but they are *subsystem* names — and post-P1, `constructs` is frozen public
vocabulary (`techy::core::constructs`, [§dd-dr:public-namespace-topology]), so the
"never a module name" rule reads as "never an implementation-artifact name", which
is how the P5 entry's own good-area examples (`token`, `scopes`, …) treat it.

**The slate.** Grouped by proposed area; "keep" = no change. Recommendation on
condition segments: **keep them unchanged** except where the area change makes the
segment vague (one case, noted) — segments are quoted alone in suppression lists
and stay self-descriptive; the area stutters this creates
(`groups.stray-group-close`) are read-whole-string harmless, and a minimal-diff
slate is reviewable. The de-stutter alternative (e.g. `core.groups.stray-close`,
`core.verbatim.unterminated`) is listed per-row where it exists; ruling it wholesale
is a one-word decision the session can flip.

| # | Current identifier | Type (declared at) | Proposed | Notes / judgment calls |
|---|---|---|---|---|
| **area `resolution`** — the concept [§dd-dr:resolution-extraction] defines (command → callable lookup through the scopes; raised by the content loop *and* the expression path, nodes_parser + argument_parsers.rs:62) | | | | |
| 1 | `core.nodes_parser.unresolvable-command` | `UnresolvableCommand` (nodes_parser.rs:102) | `core.resolution.unresolvable-command` | The identifier both walkthrough personas guessed wrong (F9); T4 imported this type by name. |
| 2 | `core.nodes_parser.command-resolution-failed` | `CommandResolutionFailed` (nodes_parser.rs:139) | `core.resolution.command-resolution-failed` | Stutter; alternates `…resolution.provider-failed` (names the actual failure — a provider errored, per its docs) or `…resolution.failed` (vague alone). **Judgment call**; keep-segment is the default recommendation. |
| **area `groups`** (group pairing/closing concept) | | | | |
| 3 | `core.group_parser.unclosed-group` | `UnclosedGroup` (group_parser.rs:61) | `core.groups.unclosed-group` | Alt `…groups.unclosed`. |
| 4 | `core.nodes_parser.stray-group-close` | `StrayGroupClose` (nodes_parser.rs:330) | `core.groups.stray-group-close` | Alt `…groups.stray-close`. Note the type stays declared beside `StopCause` in nodes_parser (producer-side, P1) — identifier decoupled from placement, exactly the P5 principle at work. |
| **area `environments`** (concept already used by the preset's own area — different owner segment, no clash) | | | | |
| 5 | `core.environment_parser.terminator-mismatch` | `EnvironmentTerminatorMismatch` (environment_parser.rs:73) | `core.environments.terminator-mismatch` | |
| 6 | `core.environment_parser.malformed-terminator` | `MalformedEnvironmentTerminator` (:89) | `core.environments.malformed-terminator` | |
| 7 | `core.environment_parser.missing-terminator` | `MissingEnvironmentTerminator` (:102) | `core.environments.missing-terminator` | |
| **area `arguments`** (the argument model, incl. its expression and tack-on sub-machinery — all three producers are argument parsers) | | | | |
| 8 | `core.argument_parsers.missing-mandatory-argument` | `MissingMandatoryArgument` (argument_parsers.rs:75) | `core.arguments.missing-mandatory-argument` | Alt `…arguments.missing-mandatory`. |
| 9 | `core.argument_parsers.expected-expression-argument` | `ExpectedExpressionArgument` (:97) | `core.arguments.expected-expression-argument` | Alt `…arguments.expected-expression`. |
| 10 | `core.nodes_parser.expression-callable-requires-content` | `ExpressionCallableRequiresContent` (nodes_parser.rs:170; **raised at argument_parsers.rs:345**) | `core.arguments.expression-callable-requires-content` | Declared in nodes_parser but raised by the expression argument parser — concept is the expression *position* of the argument model. Alt: a separate `expressions` area for 9+10 — rejected (one concept family; would strand 9 from 8). **Judgment call.** |
| 11 | `core.tack_on_parser.repeated-field` | `RepeatedTackOnField` (tack_on_parser.rs:78) | `core.arguments.repeated-tack-on-field` | **Segment renamed** (`repeated-field` is too vague once outside its own area). Alt: own area `tack_on` (underscore area precedent exists only in the file-named set being abolished; a one-condition area for an argument-parser feature is taxonomy for its own sake). **Judgment call.** |
| **area `recovery`** (the tolerant-recovery protocol — `Recovery`, `TokenRecovery` vocabulary) | | | | |
| 12 | `core.nodes_parser.unusable-recovery-token` | `UnusableRecoveryToken` (nodes_parser.rs:186) | `core.recovery.unusable-recovery-token` | Alt `…recovery.unusable-token`. Alt area `token` (it concerns a recovery *placeholder token*) — rejected: the condition is about the recovery protocol's placeholder contract, not tokenization rules. |
| **area `verbatim`** (the verbatim construct family, [§dd-dr:verbatim-family]) | | | | |
| 13 | `core.verbatim_parser.unterminated-verbatim` | `UnterminatedVerbatim` (verbatim_parser.rs:71) | `core.verbatim.unterminated-verbatim` | Alt `…verbatim.unterminated`. |
| 14 | `core.verbatim_parser.expected-verbatim-delimiter` | `ExpectedVerbatimDelimiter` (:84) | `core.verbatim.expected-verbatim-delimiter` | Alt `…verbatim.expected-delimiter`. |
| **area `scopes`** (scope/definition operations — already a P5 good-area example) | | | | |
| 15 | `core.constructs.scope-op-failed` | `ScopeOpFailed` (constructs/mod.rs:450) | `core.scopes.scope-op-failed` | **Moved out of `constructs`**: the concept is scope ops (the payload is a rendered `ScopeOpError`), the producer (the derivation sugars) is incidental. Alt keep `constructs`; alt segment `…scopes.op-failed`. **Judgment call.** |
| 16 | `core.scopes.callable-defined-as-error` | `CallableDefinedAsError` (scopes/mod.rs:1092) | keep | Definition-semantics concept (`ErrorCallableSpec`), not resolution machinery — stays `scopes`, not `resolution`. **Judgment call** (flag only). |
| **keep as-is (already concept-named)** | | | | |
| 17 | `core.token.end-of-stream-after-escape` | `EndOfStreamAfterEscape` (token/error.rs:34) | keep | |
| 18 | `core.token.forbidden-char` | `ForbiddenChar` (token/error.rs:46) | keep | |
| 19 | `core.constructs.implementation-error` | `ImplementationError` (constructs/mod.rs:434) | keep | The condition spans *all* extension contracts (Lang hooks, spec factories — :426–430), so `constructs` is slightly narrow; but post-P1 `constructs` is stable public vocabulary and the alternative (`parse`?) is vaguer. **Judgment call** (flag only). |
| 20 | `latexlike.environments.malformed-begin` | `MalformedBegin` (latexlike/environments.rs:100) | keep | |
| 21 | `latexlike.environments.unknown-environment` | `UnknownEnvironment` (:113) | keep | |
| 22 | `latexlike.environments.orphan-end` | `OrphanEnd` (:128) | keep | |
| **reserved rows (identifiers for already-ruled or this-session features; final wording at application)** | | | | |
| R1 | — | B2's resolve-failure condition | `core.sources.unresolvable-reference` | Area `sources` = the source model (deliberately NOT `resolution`, which T3 defined as *command* resolution — two concepts, two areas). |
| R2 | — | B3's depth condition (if option 2) | `core.sources.include-depth-exceeded` | Falls away under B3 option 1. |
| R3 | — | T1/T2 A1(iv) parse-init all-escape-char provider warning | `core.scopes.…` (suggest `…scopes.provider-commands-shadowed-by-escape` — wording at application) | First warning-severity core condition; the identifier area is `scopes` (provider definitions vs token rules). |

**Re-homing rider (P5): verified currently empty.** No condition type is slated to
relocate preset→core under P3 — the preset's three conditions stay preset-declared
(`latexlike.*` is correct under the defining-vocabulary rule even inside
foreign-`LLL` parses, ruled P5), and the core `environment_parser` conditions were
already core. If the P3 application surprises with a relocation, the rider fires
then; nothing to rule now.

**Consumer impact.** Zero for typed consumers (`T::IDENTIFIER` consts — the
documented matching rule); the walkthrough personas' code compiles unchanged.
String-matching consumers don't exist yet (that is the point of ruling now).
In-crate churn: the 22 attribute strings + tests asserting rendered identifiers.

**Recommendation.** Adopt the table's "Proposed" column; rule the four flagged
judgment calls (#2 segment, #10/#11 grouping, #15 move) and the wholesale
keep-vs-de-stutter segment question explicitly; confirm R1–R3 area choices so B's
application can mint them without another session. Record the final slate in the
[§dd-dr:wire-identifier-stability] entry (or a companion applied-slate note) —
it is the freeze baseline the guide table will print.
**Cost.** One mechanical sweep + test updates; pre-freeze, so free forever after.

---

## D. F7 cursor primitive — reconciliation and remainder

**Context.** SYNTHESIS F7 calls the missing "cursor primitive" the largest genuine
API gap of the T4 walkthrough. The charter asks: reconcile honestly with
[§dd-dr:source-cursor-retired] (a cursor was RETIRED) — does F7 ask for the same
thing back?

**Evidence (verified).**
- **They are different things sharing a word.** The retired `SourceCursor` was a
  *content-scanning* abstraction over source text — position-local char-at-a-time
  primitives (`peek_char`/`next_char`/`advance`/`mark`/`rewind`), retired because
  `StdTokenReader` scans `&str` directly and the trait was information-equivalent
  to `&str` ([§dd-dr:source-cursor-retired]). F7's "cursor" is the **editor
  cursor**: a *node-tree reverse lookup* (position → deepest node) plus ancestry
  (`parent()`/`ancestors()`) — tooling/FRICTION.md Task 2; the surviving runnable
  project hand-rolls exactly the descent loop (task2_position.rs:46–60: `covers()`
  half-open test + per-level covering-child scan). Nothing in F7 wants char
  iteration or position arithmetic over content; no re-introduction hazard exists.
  The SYNTHESIS phrasing "cursor primitive" is the only bridge between the two —
  worth one clarifying sentence in the durable record so a future reader doesn't
  conflate them.
- **P4 already ruled the substance** ([§dd-dr:tree-navigation]): stored parent
  table + `parent()`/`index_in_parent()`, `SourcePos`, deepest-node point lookup,
  covering-slice span lookup. Checking the ruled semantics against the
  walkthrough's four recorded subtleties (FRICTION Task 2): half-open containment
  ✓ (`start ≤ pos < end`); empty spans never match ✓; offsets inside a trigger
  spelling/terminator resolve to the callable (no deeper child) ✓ ("an offset
  inside a node but in none of its children resolves to that node"); `offset ==
  len` outside every span ✓ (half-open). **Nothing the walkthrough missed is
  outside the ruling**; F7's remainder is naming (→ E) plus two slivers below.
- The composition failure F7 flagged — "nodes found via `descendants()` cannot
  recover context" — is cured by `parent()` alone; verified `descendants()` today
  yields no depth and `NodeRef` has no parent (node/node_ref.rs:145, full pub-fn
  sweep).

**Options for the remainder.**
1. **`NodeRef::ancestors()`** — iterator over the parent chain (innermost first,
   root last). A shorthand of repeated `parent()` (shorthand-not-second-path
   compliant), directly wished (API-SURFACE wish 1). Recommend **accept**; trivial
   over the stored parent table.
2. A documented descent recipe instead of `node_at` — moot; P4 ruled the method.
3. "Already served / doc gap" verdict for the rest: yes — once E's names land,
   F7 closes entirely at application; the include-aware behavior (per-source
   descent) is already specified by the ruling and becomes *reachable* via B.

**Recommendation.** Record the cursor-vocabulary reconciliation (one sentence,
amendment note on [§dd-dr:tree-navigation] or the session record); accept
`ancestors()`; declare F7 fully covered otherwise.
**Cost.** One iterator type name (`Ancestors<'t, L>`).

---

## E. Reverse-lookup + navigation naming (P4 rider)

**Context.** P4 fixed shapes and left "method naming in 2b"
([§dd-dr:tree-navigation]). Names proposed here were checked against
[§dd-arch:naming] (specificity, clarity-over-brevity, sibling vocabulary,
adjective transitions) and [§dd-dr:superseded-names] (nothing nearby; "cursor" is
not proposed as a name anywhere, per D).

**The surface to name (with homes and signatures):**

| Item | Proposed | Alternatives / notes |
|---|---|---|
| Point-lookup on the tree | `NodeTree::node_at(&SourcePos<L::SourceOrigin>) -> Option<NodeRef<'_, L>>` | P4's working name; "deepest containing node" stated in docs, not the name (`deepest_node_at` over-specifies the common call). No clash with `NodeTree::node(id)` (tree.rs:151) — different argument vocabulary, "at" reads positional. `None` = offset in no node **or query source foreign to the tree** (doc both). |
| Span-lookup on the tree | `NodeTree::covering_slice(&SourceSpan<L::SourceOrigin>) -> Option<NodeSlice<'_, L>>` | Says the ruled semantics (minimal covering sibling run). Alternatives: `slice_at` (symmetric with `node_at` but hides "covering"), `nodes_covering`. **Judgment call**; recommend `covering_slice`. |
| Parent | `NodeRef::parent() -> Option<NodeRef<'t, L>>` | Ruled name (P4). `None` at root. |
| Index among siblings | `NodeRef::index_in_parent() -> Option<usize>` | Ruled name (P4); `Option` mirrors `parent()` (root has neither). O(1) per the ruling (own index − parent block start). |
| Ancestry iterator (D) | `NodeRef::ancestors() -> Ancestors<'t, L>` | Innermost-first (matches traceback order, [§dd-dr:parse-traceback]); self **not** included (doc explicitly — the `descendants()` self-inclusion sentence was a T1 doc wish; don't repeat the ambiguity). |
| Position type | `SourcePos<O> { source, pos }` + `new(&Arc<Source<O>>, pos)`, `source()`, `pos()`, `line_col(&mut LineIndex)`-style helper deferred | Type name ruled (P4). Accessor `pos()` over `offset()`: matches the field, `TokenReader::pos()` precedent. Debug like `SourceSpan`'s (source.rs:278–286 pattern). |
| Span→pos bridges | `SourceSpan::start_pos()` / `end_pos()` | Ruled (P4). Note `end_pos()` is exclusive — one doc sentence (a `node_at(span.end_pos())` never finds *this* node; half-open). |
| Read-honesty rider | `NodeRef::tree()` made `pub` | Currently `pub(crate)` (node_ref.rs:50); ruled by P4 point 6. Confirm only. |
| `Span` containment rider | add `Span::contains(pos)` (+ possibly `overlaps`) **now** | [§dd-dr:span-extend-to] deliberately deferred these "until a consumer arrives, pinned by docs + tests in the same commit" — `node_at` is that consumer; the empty-span semantics are now *ruled* (never match). Recommend adding `contains` with exactly the ruled semantics; leave `overlaps` unless `covering_slice`'s implementation wants it. |

**Homes.** Both lookups on `NodeTree` (tree-scope queries; not free fns — nothing
else in the node group is a free fn except `display_tree` (T1/T2 D), which is
display, not query; not on `NodeRef` — subtree-scoped variants are speculative and
additive later). `SourcePos` in the source topic beside `SourceSpan`
(`techy::source` under P1). `Ancestors` beside `Descendants` in the node group.

**Recommendation.** Adopt the table; the only genuine open choice is
`covering_slice` vs `slice_at` — recommend `covering_slice` (the name carries the
one fact callers must know: it may cover *more* than the queried span).
**Cost.** Five stable method names + two small types (`SourcePos` already ruled,
`Ancestors` new); the `Span::contains` docs+tests obligation from
[§dd-dr:span-extend-to] rides the same commit.

---

## F. T4-routed wishlist sweep (SYNTHESIS §5; POLICY_BRIEF routing: "LineIndex helpers F6; identifier registry F9")

| # | Wish | Verified current state | Recommendation |
|---|---|---|---|
| 24 | `node_at` + `parent()`/`ancestors()` | Absent (node_ref.rs pub-fn sweep) | Ruled in **P4**; T4 = names (**E**) + `ancestors()` (**D**) |
| 25 | `\input` wiring story | Resolver seam unwired (B evidence) | Ruled in **P4**; T4 = wiring (**B**) + guide chapter (Phase 4) |
| 26 | `LineIndex::line_of(offset) -> Range<usize>` (line's byte range, for caret/underline excerpts) | No inverse exists — `line_col` only (line_index.rs:120–141) | **Accept** as `line_of(&mut self, offset) -> Option<Range<usize>>` (Option: beyond `max_scan_len`/out of bounds, mirroring `line_col`). The companion `line_range(line_no)` has **no demonstrated consumer** (the walkthrough's caret path starts from an offset, FRICTION Task 4) — skip it (list-of-uses check), additive later. |
| 27a | `LineIndex::line_col_span(span) -> Option<((l,c),(l,c))>` | Composed by hand at every report site (FRICTION Task 1.3) | **Accept** — `line_col_span(impl Into<Range<usize>>) -> Option<…>` (accepts `Span`/`Range`, the SourceSpan::new bridge precedent, source.rs:198). Multi-persona (T1+T4). |
| 27b | Non-`&mut` `LineIndex` | `&mut` is the laziness contract (line_index.rs:120; documented) | **Reject** — interior mutability costs `Sync` or a lock in a no_std crate for a transient local value; the `&mut` is honest. Doc note only. |
| 27c | `NodeRef::line_col()` / `SourceSpan::line_col()` | Absent | **Reject as methods** — each call would build (or hide) a per-call `LineIndex`, the O(k·N) failure mode [§dd-dr:diagnostics-retention] exists to prevent; the honest pattern (bind the Arc, make one index, query many) becomes the guide example the walkthrough asked for (the E0716 gotcha, FRICTION Task 1.4). |
| 28 | Caret/underline excerpt renderer + machine-splittable position format | `format_position` fixed shape (error.rs:800); `render`/`render_all` are the shipped human format | **Reject for techy now** — presentation belongs to tools (backend positioning, PLAN scope); with 26+27a the hand-roll is ~10 lines (the walkthrough itself demotes it to nice-to-have once line_of exists, FRICTION ranked list #4). Machine-splittable position: **doc-only** — tools have structured access (span + line_col); document that `format_position`'s shape is not a contract. Revisit only if techy-totext/CLI grows a reporting need. |
| 29 | Depth-carrying descendants | `Descendants` yields `NodeRef` only (node_ref.rs:145) | **Accept small**: `Descendants::with_depth()` adapter yielding `(usize, NodeRef)` (depth 0 = the traversal root; name-checked, no sibling clash). Chosen over a stateful `Descendants::depth()` accessor (reads as an iterator-state peek — un-Rusty) and over parent-chain recomputation (O(depth) per node). |
| 23 | Identifier registry + "match via `T::IDENTIFIER`" rule | Registry module REJECTED in P1; rustdoc `DiagnosticInfo` implementors listing + guide table is the ruled answer | Nothing to rule — confirm the guide table + the matching-rule sentence are queued for Phase 4, printing **A's post-slate identifiers** (P5: guides only print post-restructure names). |
| 16 | `NodeKind` label accessor | — | Ruled **T1/T2 E5** (`as_str()`); do not re-open. |
| 30 | Diagnostics position sort | — | Ruled **T1/T2 E6** (`sorted_by_position()`, source-major). |
| F6 trap | 100 000-byte `max_scan_len` silently disables line/col (`None`) | Verified: DEFAULT_MAX_SCAN_LEN = 100_000 (line_index.rs:41), silent abandonment (:89–97); documented but quiet | **Docs-only, loud**: callout on `LineIndex` + `line_col` + the guide's tooling chapter ("editors over >100 KB files must `set_max_scan_len`"). Raising/removing the default trades a documented bound for unbounded memory on hostile input — keep the bound. Optionally flag the dual-meaning `None` (out-of-bounds vs unindexed) in docs; an API split (`Result`-shaped) is over-engineering for a display utility ([§dd-dr:lazy-line-col]). |
| doc wishes | Re-parse/span-stability paragraph; `LineIndex`-from-a-node example; include chapter | All still undocumented (verified docs/ untouched on these topics) | Confirm queued for Phase 4 (F1 umbrella); the span-stability sentence ("correlate across parses ⇒ own `Arc<Source>` + `parse_source`, never `parse`") is walkthrough-verified behavior (task5), cheap and high-value. |

---

## Resolved by prior rulings — do not re-litigate

- **`latexlike.*` identifiers inside foreign-`LLL` parses** — P5 (defining
  vocabulary names the raising machinery).
- **Conditions registry module** — rejected in P1; the need is served by the
  rustdoc implementors listing + guide table (F #23 above).
- **`SourcePos` as a type, parent table stored, honest slices, lookup semantics**
  — P4 ([§dd-dr:tree-navigation]); T4 only names methods (E).
- **`\input` = same-builder sub-parse into an `Attached` slot; resolver → driver
  direction** — P4 ([§dd-dr:input-attachment]); T4 designs the wiring (B), not the
  shape.
- **Resolution family home + `resolve_command_in_scopes`** — T3 H
  ([§dd-dr:resolution-extraction]); A consumes the concept name only.
- **`NodeKind::as_str()`, `sorted_by_position()`** — T1/T2 (E5/E6).
- **Stability class of everything accepted here** — P5 (ordinary stable pub, soft
  freeze).

## Session logistics (proposed order, hard structural first)

Interim rulings file `T4_RULINGS.md`, updated every round (T1/T2/T3 pattern).

1. **B** — `\input` wiring: B1 resolver surface (incl. the Copy/Eq amendment),
   B2 door + failure condition, B3 recursion policy, B4 Language collapse +
   preset construct. Output feeds A's reserved rows R1/R2 and C.
2. **C** — FS-trait closure (quick; mostly confirming the lean with the B1 surface
   in hand).
3. **A** — the identifier slate: rule the four judgment calls + the
   keep-vs-de-stutter segment policy + reserved rows. Largest table, but
   mechanical once B has named the source-side conditions.
4. **E** — navigation naming (quick; one genuine open choice, `covering_slice`).
5. **D** — cursor reconciliation + `ancestors()` (quick).
6. **F** — wishlist sweep (26/27/28/29 + F6 trap + Phase-4 confirmations).
7. **Sweep** — resolved-by-prior confirmation; durable records list
   (DESIGN_RATIONALE: new entry for the `\input` wiring/door + amendments on
   [§dd-dr:source-resolver], [§dd-dr:resolver-contract], [§dd-dr:language-init]
   (completion), [§dd-dr:wire-identifier-stability] (applied slate),
   [§dd-dr:tree-navigation] (names + cursor-vocabulary note),
   [§dd-dr:span-extend-to] (contains-consumer note), T3-D Copy/Eq amendment;
   superseded-names additions as they arise); PLAN.md updates (companion-section
   closure, T5/recompose handoffs below).

**Belongs to other sessions (flag, don't rule here):**
- **T5**: honest-slice/validator application details; `stage_invocation` signature
  (co-designed with restage bundles); whether the transform surface needs any
  `\input`-specific restage affordance (splice-a-cached-parse — the entry's
  caching-framework route); driver knobs/extension seam (B1's resolver field is a
  new datum for that discussion).
- **Recompose session**: the verbatim strategy's reliance on `Attached` exclusion
  (emit `\input{file}`, never the content; expansion as explicit strategy) — B only
  restates it, the recompose design owns it.
- **Tier-C batch**: per-item fates of the untouched source-module exports
  (`NoResolver` — B1 note, `ProvenanceChain`, `ResolvedContent`, `SourceResolver`
  root-vs-module placement); note the free fn `resolve_source`'s status flips from
  "redundant" to "canonical" under B4 — carry that into the batch list.
