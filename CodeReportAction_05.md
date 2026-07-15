# Action 05 — Extension surface and read-API gaps

**Status: open — API additions/decisions. Synced against the code 2026-07-15: items
resolved by earlier action items (the panic-cleanup sweep, the reader `move_to_pos`
rework, the read-API additions) are marked ✅ and their text updated; everything else
re-verified against the current sources. Grouped by the type they land on; each
sub-section is independently actionable. Several become breaking-to-change once
downstream implementors (FLM) exist, so "decide now" beats "decide when needed".**

## A. `CallableSpec` has no downcast path — unchanged

`CallableSpec` (`src/spec/callable.rs:57`) has supertraits `fmt::Debug + Send + Sync` —
no `Any`, no `as_any`. But the documented preset extension pattern *requires* the
downcast: `Lang::finalize_node`'s docs (`src/state/lang.rs:360`) say "match a
`Callable`, read its `spec`, **downcast**, attach ext", and DESIGN_RATIONALE (line
~1298) promises a preset "downcasts **to its own spec trait**, attaches its
`flm_specinfo`-like ext" (it is the stated reason the core needs no spec-level finalize
hook). That downcast is not expressible today — the crate's own finalize-node rehearsal
test (`nodes_parser.rs`, `finalize_node_populates_callable_ext_through_the_dispatch_loop`)
concedes it in a comment and derives the ext from declarative facts instead.

Fix: `Arc<dyn CallableSpec<L>>` already implies `'static`, so adding
`+ core::any::Any` to the supertraits costs implementors nothing and enables
`downcast_ref::<ConcreteSpec>()`. Caveat — now sharpened by the sync: the rationale's
own wording is downcast to *another trait object* ("its own spec trait"), which needs
more than `Any`; if that is the real intent, a preset-facing hook is needed instead.
This also gates the "special-case the default factory path" escape hatch for eliding
the per-invocation `Box` (the core currently cannot detect that a spec uses
`StdCallableSpec`'s default).

## B. `StdCallableSpec` and the declarative surface

1. **✅ The slots trap — resolved (panic-cleanup sweep).** `StdInvocationParser::parse`
   no longer `debug_assert!`s: a spec declaring slots without overriding
   `make_invocation_parser` now gets a clean `Err` abort via
   `cx.implementation_error(…)` whose message explains the contract
   (`invocation_parser.rs:154`), and a test pins the message
   (`argument_parsers.rs`, "slots need a factory override"). No debug panic, no silent
   body drop. *Residue (doc polish):* `CallableSpec::slots()`'s doc still invites the
   declaration ("an environment-shaped callable has exactly one") and neither it nor
   `StdCallableSpec` mentions the factory-override requirement — today it is
   discoverable only by hitting the runtime error. The longer-term option (the preset's
   `EnvironmentSpec` constructor as the only slot-bearing entry point) remains a
   Phase 7 question.
2. **The declarative lists are advisory, but the core reads them as a predicate —
   unchanged.** Takeover specs legitimately diverge: `BeginSpec` declares neither
   arguments nor slots; the raw-block exemplar mints `ParsedSlot` records its spec never
   declared (both still test-only compositions in `environment_parser.rs`). The builder
   never cross-checks records against declarations. Consequence worth documenting on
   the trait: `argument_parsers.rs:329` uses
   `!spec.arguments().is_empty() || !spec.slots().is_empty()` as the "cannot use ‘X’ as
   a single expression: it takes arguments" guard — a declares-nothing takeover spec
   bypasses it entirely (`\frac\begin{center}…` would dispatch the environment as
   `\frac`'s argument; pylatexenc behaves the same, so probably intended, but invisible
   from the docs). Also: `Invocation::name`'s doc (`constructs/mod.rs:291`) says "the
   invocation spelling, as written (the node stores an owned copy)" — the parenthetical
   is false for the environment shape, where the node records the *environment's* name
   (`environment_parser.rs:784`, "The environment's own name and spec, not the
   dispatcher's").
3. **Slot-bearing composition helper — unchanged.** An environment-shaped callable
   still implements `CallableSpec` from scratch and hand-rolls ~100 lines of argument
   parsing, slot-delta stacking, `ParsedSlots`/`ChildRegion` assembly, content
   designation and span-end bookkeeping (it still exists only as test code in
   `environment_parser.rs` — `BeginSpec`/`EnvironmentInvocationParser`, plus the
   `RawBlockSpec` variant) — exactly the region bookkeeping the builder enforces by
   `Err`. Phase 6.6 reserved the right to promote the composition to a core helper "if
   it proves nontrivial"; the shipped test composition shows it is. Recommend a core
   slots-aware sibling of `StdInvocationParser`.
4. **Ergonomics (low, user's call) — unchanged.**
   `StdCallableSpec::new(Vec<Arc<…>>, Vec<Arc<…>>)` (`callable.rs:126`) forces triple
   nesting (`Arc::new(ArgumentSpec::new(Arc::new(GroupArgumentParser::…`) and is out of
   step with the builder-style siblings (`ArgumentSpec::new(p).named(…)`). A
   `.with_arguments(…)` / `.with_slots(…)` chain that Arc-wraps internally would match
   the prevailing style — or declare `new()` plumbing and point users at the planned
   preset one-liners (`MacroSpec`/`EnvironmentSpec`/`SpecialsSpec`, Phase 7).

## C. Public-trait obligations implemented by crate-private helpers — half resolved

`ArgumentParser` is a public trait whose docs require implementors to follow two
protocols: "absent ⇒ nothing consumed, rewound" and "token error while probing ⇒ absent
without diagnosing (tolerant) / abort (strict)".

- **✅ The rewind half is now public (reader rework).** The old crate-private
  `resume_at` is gone: repositioning is `TokenReader::move_to_pos` — a public trait
  method whose doc names "an argument parser's absent-argument rewind target"
  explicitly (`token/reader.rs:59–69`) — and `ArgumentNoise::rewind` is `pub`
  (`argument_parsers.rs:155`), wrapping it for the noise-scan case. `\verb`-style
  takeover repositioning is expressible out of crate.
- **The probe half is still crate-private — and not even re-derivable.** `try_peek`
  (`constructs/mod.rs:82`) remains `pub(crate)`, and its abort path needs
  `ParserSession::snapshot_frames`, which is also `pub(crate)` (`engine/mod.rs:184`) —
  an out-of-crate argument parser cannot reproduce the strict-mode abort with the
  traceback attached at all (`ParseError::from_token_error` is public, the frames are
  not). Any hand-rolled divergence produces exactly the double-report the design set
  out to avoid. Make `try_peek` `pub` (which sidesteps the `snapshot_frames` question),
  re-exported alongside `scan_argument_noise` / `stage_pre_space` / `ArgumentNoise`.
- **Enumerate the builder-enforced obligations in `ArgumentParser::parse_argument`'s
  doc — unchanged.** The doc (`spec/structure.rs:91–113`) now covers the absent
  protocol and noise ownership well, but the builder-enforced obligations are still
  discoverable only by crashing: (1) `content` ranges must be in bounds
  (`InRegion(r)`: `r.end <= nodes.len()`; `InChildrenOf(b, r)`: `b` staged, range
  within its children, `b` inside the region's subtree); (2) returned nodes must be
  staged, unclaimed, in source order; (3) regions must be span-contiguous with the
  preceding argument's region; (4) the region list must tile the child list exactly.
  The token-error-while-probing protocol is likewise still undocumented there (it lives
  only on the crate-private `try_peek`).
- **Crate-root re-exports — unchanged.** Still reachable only via
  `techy::constructs::…` while the root prelude is the advertised surface:
  `StdInvocationParser`, the four standard argument parsers (`GroupArgumentParser`,
  `OptionalGroupArgumentParser`, `MarkerArgumentParser`, `ExpressionParser`),
  `EnvironmentBody`/`EnvironmentBodyParser`, `scan_argument_noise`, `stage_pre_space`,
  `ArgumentNoise`. (Their *condition* types are at the root; the parsers themselves are
  not.)

## D. `ParseContext`: scoped state swap and constructor — unchanged

The `cx.state` swap/restore protocol is still hand-rolled; no `with_state`, no
`ParseContext::new`. The seven lib-code sites:

| site | what is scoped |
|---|---|
| `nodes_parser.rs` invocation descent (~597/605) | after-effect threading |
| `nodes_parser.rs` group descent (~853) | interior state |
| `group_parser.rs` (~173) | group interior |
| `invocation_parser.rs` (~116) | per-argument delta |
| `argument_parsers.rs` probe (~604–606) | optional-argument probe peek |
| `argument_parsers.rs` contents (~654–656) | optional-argument contents |
| `environment_parser.rs` (~231) | rigid name-group interior |

(The test-only environment compositions hand-roll two more instances of the same
pattern — every future out-of-crate composition will too.) All are correct today, but
the correctness is per-site discipline — and the probe site still shows how unnatural
it gets (`argument_parsers.rs:605–607`: a `Result` deliberately held un-`?`-ed across
the restore). Proposal:

```rust
pub fn with_state<R>(&mut self, state: Arc<ParsingState<L>>,
                     f: impl FnOnce(&mut Self) -> R) -> R
```

Frame it as **ordering enforcement, not unwind safety** (the crate is `no_std`; an
unwind tears down the borrowed context; a `Drop` guard would be over-engineering).
Secondary: `ParseContext` is constructed by struct literal with all-public fields —
today only in test code (~10 sites; no lib-code driver constructs one yet), but its
stated purpose is "one place to grow (depth limits, cancellation)", and until
`Language::parse` lands every external driver will construct it by literal too — each
new field then breaks them all. Add `ParseContext::new(tokens, source, state, session)`
(fields can stay public).

## E. `SourceResolver` — decide the contracts before wiring — verified unchanged

Re-verified 2026-07-15: nothing in this section moved. The seam still has **no consumer**
(nothing calls `resolve()`; the resolver is planned to live on `Language<L>`, deferred
to Phase 7), which makes now the cheap moment:

1. **`Send + Sync` supertraits** — highest priority. Every other extension trait got
   them in the July 2026 sweep (`CallableSpec`, `SpecLookup`, `ArgumentParser`,
   `SourceOrigin`); `SourceResolver` (`source/resolver.rs:25`) still has none. It will
   be stored in the shared, long-lived `Language<L>` (parallel parses require
   `R: Sync`), and it is the trait most likely to hold caches/connections. Adding
   bounds later breaks implementors — the recorded reason the sweep happened "while the
   API is fluid". If exclusion is deliberate, record why in DESIGN_RATIONALE.
2. **Recursion guard.** A self-/mutually-including document is unbounded recursion +
   memory (verified by probe: `MapResolver` mapping `a.tex → \input{a.tex}` resolves
   forever; the provenance graph stays a tree — this is non-termination, not a cycle).
   Decide whose job: a session-enforced max include depth with a diagnostic
   (recommended: policy, always correct as a backstop) and/or a documented implementor
   contract plus a chain-walk helper (`provenance_chain()` already exposes every
   enclosing `Resolved { reference }`; note reference strings are not canonical
   identities — `./a.tex` vs `a.tex` — so a guaranteed check needs embedder
   canonicalization).
3. **Fresh-source-per-resolve contract.** `resolve` returning `Arc<Source<O>>` invites
   caching one `Arc` for a twice-included file — but provenance lives on the `Source`,
   so a shared source records only the first trigger site and every diagnostic inside
   the second inclusion renders the wrong include chain. Silently wrong, no test would
   notice. State the contract on the trait ("must return a source whose provenance
   records *this* `triggered_at`"); at the future call site,
   `debug_assert!` the provenance matches. Flip side worth one doc line: content is
   necessarily duplicated per include site (forced by `Source`'s `content: String`
   today) — don't let anyone "optimize" it into the trap; an `Arc<str>` backing is the
   real fix if it matters.
4. **`ResolveError` cannot carry a cause.** The primary intended implementor is a file
   reader with an `io::Error`; the type still offers two `String`s and no `source()`
   override. Dep-free fix: `cause: Option<Box<dyn core::error::Error + Send + Sync +
   'static>>` + `fn source()`. Also missing: any bridge from `ResolveError` into
   `ParseErrorKind`/`Diagnostic` — a failed `\input` under tolerant vs strict is a
   `Recovery`-policy question to decide at wiring time; the error shape is cheaper to
   fix first.
5. **Small:** blanket forwarding impls (`&R`, `Box<R>`, `Arc<R>`); object safety holds
   today but no test pins it; `MapResolver` can never label its sources (blanket impl
   over `O` can only produce `O::default()`) — a `with_reference_as_origin()` toggle
   would make multi-file diagnostics self-describing; the trait doc's claim "the parser
   is generic over the resolver" is future tense in reality — say "will be" until true.

## F. `NodeRef` / `NodeTree` — the Phase 7 read-API primitives

1. **`NodeRef::tree()` — still missing, but demoted.** The original "the read API
   dead-ends without it" is no longer true: `NodeRef` now resolves its own records —
   `argument_nodes(i)`, `argument_content_nodes(i)`, `slot_content_nodes(i)`,
   `slot_content_parent(i)`, `body()` all exist (`node_ref.rs:228–274`) and consult the
   tree internally. `tree()` remains a one-line addition (`pub fn tree(&self) -> &'t
   NodeTree<L>`; the field exists) useful for generic consumers that need to escape
   into arbitrary tree traversal; no longer the highest-value item.
2. **✅ `parent: u32` in `NodeData` — DECIDED no (user, 2026-07-15).** Upward
   navigation is not needed; nodes stay parent-free. (Context kept for the record:
   `finish()` computes a transient parent vector for `resolve_regions` and drops it;
   `check_tree_invariants` rebuilds it by hand — both are internal and fine.)
3. **Document-order traversal — unchanged in substance.** `NodeTree::iter()` is still
   storage order (breadth-first): for `a{b}c` it yields `a`, `c`, `b` — scrambled text
   for the natural post-processing one-liner. Its doc now *says* "storage order (root
   first; every node's children contiguous)" but stops short of warning it is not
   document order; there is still no pre-order `descendants()` on `NodeRef`.
   (`nodes_in(range)` exists for region ranges, which covers the argument/slot cases.)
4. **Accessor symmetry — narrowed but open:** slots have a content-parent accessor
   (`slot_content_parent`); arguments still don't (`argument_content_parent(i)` — it
   answers the most-asked question, "give me the group node of argument 0"). By-name
   node-level access is still missing (`argument_nodes_named("title")`;
   `ParsedArguments::get_named`/`ParsedSlots::get_named` exist at the record level).
5. **✅ `NodeTree::get(id) -> Option<NodeRef>` — done.** Implemented (`tree.rs:163`)
   with debug-build provenance tags on `NodeId` (cross-tree ids are a miss in debug);
   `node()` stays as the assert-y convenience with the panic documented as the approved
   indexing-style exception.

## G. `Span` — API completeness and the pub-fields question

1. **Pub fields vs validating constructor — open, but the code has drifted toward
   (i).** `Span::new` still debug-asserts `start <= end` with `pub` fields, and the
   crate now *documents and tests* the benign behavior of invariant-violating spans:
   `len()` is saturating with a rationale comment naming the pub-fields loophole, and a
   test pins `Span { start: 7, end: 3 }` as constructible-but-benign
   (`span.rs:36–42, 130–137`). That is position (i) — transparent value type, assert
   advisory — in all but a recorded decision. Either record (i) in
   DESIGN_RATIONALE §3.1 and adjust the docs to say so, or commit to (ii)
   (privatize + `start()`/`end()` + explicit `extend_to(end)`, matching `SourceSpan`).
   The remaining in-place `end` mutations at lib sites are all monotone extensions.
2. **Missing operations consumers hand-roll — partially done:** ✅ non-panicking
   `get(content) -> Option<&str>` now exists as the documented companion of the
   panicking `slice()` (`span.rs:76`). Still missing: `cover`/`merge` (half a dozen
   sites build `Span::new(a.start, b.end)` across two spans by hand —
   `constructs/mod.rs:108`, `invocation_parser.rs:166,199`, `group_parser.rs:208`,
   `nodes_parser.rs:744`, `argument_parsers.rs:194`) and `contains` (the invariant
   checker works in raw ranges). **If `contains`/`overlaps` are added, the empty-span
   semantics must be decided at the same commit** (does `2..5` contain `empty(5)`? does
   `empty(3)` overlap `3..7`?) — both answers are defensible and will be silently
   depended on; pin in docs + tests, record in DESIGN_RATIONALE §3.1.
3. **Bridge to `SourceSpan` — unchanged, and growing:** now 39 occurrences of the
   `SourceSpan::new(&cx.source, …)` pattern (was 21 at the original report); still no
   inverse (`SourceSpan::span()`), so the invariant checker works entirely in
   `Range<usize>`. Candidates: `SourceSpan::span() -> Span` and
   `Span::in_source(&Arc<Source<O>>) -> SourceSpan<O>`. Design note: the latter puts a
   `SourceSpan`-returning method on the deliberately Arc-free type — fine for the
   module's invariants, but weigh the mental model.
4. **Doc — partially done:** ✅ `slice()`'s panic contract is now documented with a
   pointer to `get()`. Remaining: the struct doc's "must fall on `char` boundaries"
   (`span.rs:11`) still reads as a type guarantee but is a caller contract enforced
   downstream; `new()`'s doc still doesn't state its `start <= end` precondition (only
   the debug_assert embodies it).

## H. `Lang` type-bundle gaps

1. **✅ `SlotExt` — done (2026-07-15, user-approved).** `NodeExtTypes` gained
   `type SlotExt` (`()` under the no-ext bundle), `ParsedSlot` gained `ext: SlotExt<L>`
   mirroring `ParsedArgument.ext`; test + DESIGN_RATIONALE entry added.
2. **The `SimpleLang` cliff — DECIDED: accept for now, revisit later (user,
   2026-07-15).** Nothing to do immediately; keep the note that this needs revisiting
   (e.g. helper macros) once FLM-adjacent implementors multiply. Evidence at decision
   time: 20 hand-written `Lang` impls in-crate, most retyping the associated types
   verbatim to override one hook. Options when revisited: (a) a `LangTypes` supertrait
   split; (b) an `impl_lang!` macro; (c) documented as a considered choice.

## Suggested order (updated)

~~B.1~~ (done — panic replaced by `Err`) and the `resume_at` half of C (done —
`move_to_pos` is public) drop out. Remaining: A (one-line supertrait — but settle the
concrete-type vs trait-object question first, since the rationale's wording implies the
latter) → C (make `try_peek` public + document the `parse_argument` obligations + root
re-exports) → F.2 (`parent` is the decide-now layout question; F.1 `tree()` is a
trivial add-along) → E.1 (one-way-door supertrait) → the rest as they come up in
Phase 6.6/7 planning. G and H are independent and can be batched with any related work;
G.1 mostly needs its de-facto position (i) recorded.
