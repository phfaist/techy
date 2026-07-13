# Action 05 — Extension surface and read-API gaps

**Status: open — API additions/decisions. Grouped by the type they land on; each
sub-section is independently actionable. Several become breaking-to-change once
downstream implementors (FLM) exist, so "decide now" beats "decide when needed".**

## A. `CallableSpec` has no downcast path

`CallableSpec` (`src/spec/callable.rs`) has supertraits `fmt::Debug + Send + Sync` —
no `Any`, no `as_any`. But the documented preset extension pattern *requires* the
downcast: `Lang::finalize_node`'s docs say "match a `Callable`, read its `spec`,
**downcast**, attach ext", and DESIGN_RATIONALE promises the same (it is the stated
reason the core needs no spec-level finalize hook). That downcast is not expressible
today — the crate's own finalize-node rehearsal test concedes it and works around it.

Fix: `Arc<dyn CallableSpec<L>>` already implies `'static`, so adding
`+ core::any::Any` to the supertraits costs implementors nothing and enables
`downcast_ref::<ConcreteSpec>()`. Caveat: downcasting to *another trait object* ("its
own spec trait") needs more than `Any`; if that is the real intent, a preset-facing
hook is needed instead. This also gates the "special-case the default factory path"
escape hatch for eliding the per-invocation `Box` (the core currently cannot detect
that a spec uses `StdCallableSpec`'s default).

## B. `StdCallableSpec` and the declarative surface

1. **The slots trap.** `StdCallableSpec::new(arguments, slots)` accepts a slot list,
   and the trait doc invites it ("an environment-shaped callable has exactly one") —
   but a spec that declares slots *without* overriding `make_invocation_parser` gets
   `StdInvocationParser`, which opens with
   `debug_assert!(spec.slots().is_empty(), "StdInvocationParser is macro-shaped…")`.
   The natural user action panics in debug and silently drops the body in release — a
   lib panic driven by a user's spec declaration, not an internal invariant. Fixes, in
   order of strength: (a) document the constraint on `StdCallableSpec` and
   `CallableSpec::slots()`; (b) make `StdInvocationParser` degrade with a diagnostic
   instead of asserting; (c) longer-term, make the preset's `EnvironmentSpec`
   constructor the only slot-bearing entry point.
2. **The declarative lists are advisory, but the core reads them as a predicate.**
   Takeover specs legitimately diverge: `BeginSpec` declares neither arguments nor
   slots; the raw-block exemplar mints `ParsedSlot` records its spec never declared.
   The builder never cross-checks records against declarations. Consequence worth
   documenting on the trait: `argument_parsers.rs` uses
   `!spec.arguments().is_empty() || !spec.slots().is_empty()` as the "cannot use ‘X’ as
   a single expression: it takes arguments" guard — a declares-nothing takeover spec
   bypasses it entirely (`\frac\begin{center}…` would dispatch the environment as
   `\frac`'s argument; pylatexenc behaves the same, so probably intended, but invisible
   from the docs). Also: `Invocation`'s doc says `name` is "the invocation spelling, as
   written" — false for the environment shape, where the node records the
   *environment's* name.
3. **Slot-bearing composition helper.** An environment-shaped callable currently
   implements `CallableSpec` from scratch and hand-rolls ~60 lines of argument parsing,
   slot-delta stacking, `ParsedSlots`/`ChildRegion` assembly, content designation and
   span-end bookkeeping (today it exists only as test code in
   `environment_parser.rs`) — exactly the region bookkeeping the builder enforces by
   panic. Phase 6.6 reserved the right to promote the composition to a core helper "if
   it proves nontrivial"; the shipped test composition shows it is. Recommend a core
   slots-aware sibling of `StdInvocationParser`.
4. **Ergonomics (low, user's call).** `StdCallableSpec::new(Vec<Arc<…>>, Vec<Arc<…>>)`
   forces triple nesting (`Arc::new(ArgumentSpec::new(Arc::new(GroupArgumentParser::…`)
   and is out of step with the builder-style siblings (`ArgumentSpec::new(p).named(…)`).
   A `.with_arguments(…)` / `.with_slots(…)` chain that Arc-wraps internally would
   match the prevailing style — or declare `new()` plumbing and point users at the
   planned preset one-liners (`MacroSpec`/`EnvironmentSpec`/`SpecialsSpec`, Phase 7).

## C. Public-trait obligations implemented by crate-private helpers

`ArgumentParser` is a public trait whose docs require implementors to follow two
protocols: "absent ⇒ nothing consumed, rewound" and "token error while probing ⇒ absent
without diagnosing (tolerant) / abort (strict)". The helpers that encode those
protocols — `try_peek` and `resume_at` (`constructs/mod.rs`) — are `pub(crate)`. An
out-of-crate argument parser (or a `make_invocation_parser` takeover parser doing
`\verb`-style repositioning) must re-derive both from scratch, and any divergence
produces exactly the double-report the design set out to avoid.

- Make `try_peek` and `resume_at` `pub`, re-exported alongside `scan_argument_noise` /
  `stage_pre_space` / `ArgumentNoise` (which are already public).
- **Enumerate the builder-enforced obligations in `ArgumentParser::parse_argument`'s
  doc.** They are currently discoverable only by crashing: (1) `content` ranges must be
  in bounds (`InRegion(r)`: `r.end <= nodes.len()`; `InChildrenOf(b, r)`: `b` staged,
  range within its children, `b` inside the region's subtree); (2) returned nodes must
  be staged, unclaimed, in source order; (3) regions must be span-contiguous with the
  preceding argument's region; (4) the region list must tile the child list exactly.
- **Crate-root re-exports** were never updated for 6.4/6.5: `StdInvocationParser`, the
  four standard argument parsers, `EnvironmentBody`/`EnvironmentBodyParser`,
  `scan_argument_noise`, `stage_pre_space`, `ArgumentNoise` are reachable only via
  `techy::constructs::…` while the root prelude is the advertised surface.

## D. `ParseContext`: scoped state swap and constructor

The `cx.state` swap/restore protocol is hand-rolled at seven sites, each of which must
remember to restore **before** the `?`:

| site | what is scoped |
|---|---|
| `nodes_parser.rs` invocation descent | after-effect threading |
| `nodes_parser.rs` group descent | interior state |
| `group_parser.rs` | group interior |
| `invocation_parser.rs` | per-argument delta |
| `argument_parsers.rs` (probe) | optional-argument probe peek |
| `argument_parsers.rs` (contents) | optional-argument contents |
| `environment_parser.rs` | rigid name-group interior |

All seven are correct today, but the correctness is per-site discipline — and the probe
site shows how unnatural it gets (a `Result` deliberately held un-`?`-ed across a
restore). Proposal:

```rust
pub fn with_state<R>(&mut self, state: Arc<ParsingState<L>>,
                     f: impl FnOnce(&mut Self) -> R) -> R
```

Frame it as **ordering enforcement, not unwind safety** (the crate is `no_std`; an
unwind tears down the borrowed context; a `Drop` guard would be over-engineering).
Secondary: `ParseContext` is constructed by struct literal at 7 sites with all-public
fields, while its stated purpose is "one place to grow (depth limits, cancellation)" —
every new field is a breaking change at every construction site, including external
drivers until `Language::parse` lands. Add `ParseContext::new(tokens, source, state,
session)` (fields can stay public).

## E. `SourceResolver` — decide the contracts before wiring

The seam has **no consumer yet** (nothing calls `resolve()`; the resolver is planned to
live on `Language<L>`, deferred to Phase 7), which makes now the cheap moment:

1. **`Send + Sync` supertraits** — highest priority. Every other extension trait got
   them in the July 2026 sweep (`CallableSpec`, `SpecLookup`, `ArgumentParser`,
   `SourceOrigin`); `SourceResolver` has none. It will be stored in the shared,
   long-lived `Language<L>` (parallel parses require `R: Sync`), and it is the trait
   most likely to hold caches/connections. Adding bounds later breaks implementors —
   the recorded reason the sweep happened "while the API is fluid". If exclusion is
   deliberate, record why in DESIGN_RATIONALE.
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
   reader with an `io::Error`; the type offers two `String`s and no `source()`
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

1. **`NodeRef::tree()`** — one line (`pub fn tree(&self) -> &'t NodeTree<L>`; the field
   exists). Without it the read API dead-ends: `arguments()`/`slots()` hand back
   records whose payload is `NodeId`/`Range<u32>`, unresolvable from the `NodeRef`
   alone; every consumer independently carries the `&NodeTree`. Highest-value addition.
2. **`parent: u32` in `NodeData` — decide now.** No upward navigation exists and none
   is derivable in less than O(n). `finish()` **already computes the full parent
   vector** and discards it; `check_tree_invariants` rebuilds it by hand. Storing it
   costs 4 bytes/node (~7% on the struct) and unlocks `parent()`, `next_sibling()`,
   `prev_sibling()`, `ancestors()`. It is a storage-layout change — breaking after
   Phase 7 starts.
3. **Document-order traversal.** `NodeTree::iter()` is **breadth-first** (storage
   order): for `a{b}c` it yields `a`, `c`, `b` — scrambled text for the natural
   post-processing one-liner. Add a pre-order `descendants()` on `NodeRef` and a
   "not document order; recurse via `children()`" warning on `iter()`.
4. **Accessor symmetry:** add `argument_content_parent(i)` (slots have a content-parent
   accessor; arguments don't, though it answers the most-asked question — "give me the
   group node of argument 0"), and by-name access (`argument_nodes_named("title")`;
   `ParsedArguments::get_named` exists but the node-level accessors don't).
5. **`NodeTree::get(id) -> Option<NodeRef>`** — the standard arena escape hatch for
   consumers holding ids of unknown provenance (id-mapping, deserialization, tooling);
   keep `node()` as the assert-y convenience.

## G. `Span` — API completeness and the pub-fields question

1. **Pub fields vs validating constructor.** `Span::new` debug-asserts
   `start <= end`, but the fields are `pub` and four lib sites mutate `end` in place
   (all monotone extensions today), so the invariant is advisory. Inconsistent with
   `SourceSpan` (private fields, accessors, validating `new`). Two coherent positions:
   (i) declare `Span` a transparent value type and demote the assert to documentation;
   (ii) privatize + `start()`/`end()` + explicit `extend_to(end)`. The middle ground
   gives the appearance of enforcement without the substance.
2. **Missing operations consumers hand-roll:** `cover`/`merge` (five sites build
   `Span::new(a.start, b.end)` by hand), `contains` (invariant checker), non-panicking
   `get(content) -> Option<&str>` (companion to panicking `slice()`; one immediate
   consumer). **If `contains`/`overlaps` are added, the empty-span semantics must be
   decided at the same commit** (does `2..5` contain `empty(5)`? does `empty(3)`
   overlap `3..7`?) — both answers are defensible and will be silently depended on;
   pin in docs + tests, record in DESIGN_RATIONALE §3.1.
3. **Bridge to `SourceSpan`:** 21 occurrences of
   `SourceSpan::new(&cx.source, span.range())`; no inverse (`SourceSpan::span()`), so
   the invariant checker works entirely in `Range<usize>`. Candidates:
   `SourceSpan::span() -> Span` and `Span::in_source(&Arc<Source<O>>) -> SourceSpan<O>`.
   Design note: the latter puts a `SourceSpan`-returning method on the deliberately
   Arc-free type — fine for the module's invariants, but weigh the mental model.
4. Doc: the struct doc's "must fall on `char` boundaries" reads as a type guarantee but
   is a caller contract enforced downstream (`slice()` panics, `SourceSpan::new`
   debug-asserts, reader advances by `len_utf8`) — one clause fixes it; `new()`'s doc
   should state its `start <= end` precondition.

## H. `Lang` type-bundle gaps

1. **`SlotExt` is missing.** `ParsedArgument` carries `ext: ArgumentExt<L>`;
   `ParsedSlot` carries only `spec` + `region`. The asymmetry bites where FLM is
   richest: an environment's *body* is a slot, and per-instance derived data about a
   body (tabular cells, enumerate items) has no home except the whole-callable ext.
   Nothing in the design docs motivates the absence. Add `SlotExt` (one line on
   `NodeExtTypes`, one field on `ParsedSlot`, `()` under `SimpleLang`) — cheap now,
   breaking later — or record the rationale for its absence.
2. **The `SimpleLang` cliff.** `SimpleLang` supplies all seven associated types via a
   blanket impl; overriding a single hook forces implementing `Lang` directly and
   restating every associated type. In-crate evidence: 16 hand-written `Lang` impls,
   most retyping `type GroupTypeId = u32; …` verbatim to override one hook (~100 lines
   of boilerplate) — the shape every FLM-adjacent experiment will hit. Options: (a) a
   `LangTypes` supertrait split (`trait Lang: LangTypes`) so defaults-plus-one-override
   works; (b) an `impl_lang!` macro; (c) accept the cliff and say so in the doc as a
   considered choice.

## Suggested order

A (one-line supertrait, unblocks two documented patterns) → B.1 + C (extension-author
safety: the slots trap and the protocol helpers) → F.1/F.2 (`tree()` is trivial;
`parent` is the decide-now layout question) → E.1 (one-way-door supertrait) → the rest
as they come up in Phase 6.6/7 planning. G and H are independent and can be batched
with any related work.
