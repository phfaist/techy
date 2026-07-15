# Action 05 — Extension surface and read-API gaps

**Status: largely resolved (2026-07-15 session). ✅ = implemented and tested; "→ plan" =
direction decided, design details deferred to
[PlanSlotsAndConvenienceSurface.md](PlanSlotsAndConvenienceSurface.md) for a dedicated
discussion. Line references were synced against the code on 2026-07-15 and drift as the
code moves.**

## A. `CallableSpec` downcast path — ✅ done (2026-07-15)

`CallableSpec` gained the `Any` supertrait (user-approved); `Lang`/`SimpleLang` gained
the `'static` bound this requires (free — `Lang`s are unit marker types; the stored
`Arc<dyn CallableSpec<L>>` was implicitly `'static` all along). The finalize-node
rehearsal test now performs the real downcast through the dispatch loop
(trait-upcast to `&dyn Any`, `downcast_ref::<StdCallableSpec<_>>()`, field access
through the concrete type). The trait-object case (open set of preset spec types) is
covered by the concrete-wrapper pattern, and the `Lang`-associated-dyn-type
alternative is recorded as the upgrade path — both in DESIGN_RATIONALE §3.4. This also
unblocks the §3.6 default-factory escape hatch (eliding the per-invocation `Box`) if
profiling ever asks.

## B. `StdCallableSpec` and the declarative surface — ✅ mostly done (plan §A/§B executed 2026-07-15)

1. ✅ **Slots trap:** structurally dissolved — `SlotSpec`, `CallableSpec::slots()` and
   `StdCallableSpec.slots` are deleted (no spec-side slot vocabulary; the callable
   spec's sanctioned parser populates the reshaped
   `ParsedSlot { name, region, ext }` records directly); the implementation-error arm
   and its pinned test went with them. Executed per plan §A, recorded in
   DESIGN_RATIONALE §3.6.
2. ✅ **Advisory declarative lists / expression guard:**
   `ArgumentParser::can_match_empty()` + `CallableSpec::requires_content()` (names
   user-decided 2026-07-15), guard rewired, behavior pins tested (optional-only bare
   use valid; `BeginSpec`-style override diagnosed). The `Invocation::name` doc drift
   is fixed. Condition renamed `ExpressionCallableRequiresContent` (flagged for
   sign-off).
3. **Slot-bearing composition helper:** `parse_declared_arguments` and
   `read_rigid_name_group` are now `pub` building blocks; where the standard `\begin`
   composition lives (core vs preset) stays open: plan §A.5.
4. **Constructor ergonomics:** plan §C (open).

## C. Public-trait obligations — ✅ mostly done (2026-07-15)

- ✅ **`ParseContext::probe_token(&state)`** replaces the crate-private `try_peek`: the
  argument-probe protocol (tolerant ⇒ `Ok(None)` without diagnosing/consuming;
  unrecoverable or strict ⇒ abort) is now a public method, with the state an explicit
  parameter — which also dissolved the optional-argument probe swap site entirely.
- ✅ **`ParserSession::snapshot_frames` is public** (custom parsers building their own
  `ParseError`s); `push_frame`/`pop_frame` stay crate-private behind `with_frame`.
- (Earlier) the rewind half was already public: `TokenReader::move_to_pos` +
  `ArgumentNoise::rewind`.
- **Remaining, folded into plan §C:** enumerate the builder-enforced obligations on
  `ArgumentParser::parse_argument`'s doc (write once alongside the new `SlotParser`
  doc), and the crate-root re-exports (`StdInvocationParser`, the four argument
  parsers, `EnvironmentBody(Parser)`, `scan_argument_noise`, `stage_pre_space`,
  `ArgumentNoise`).

## D. `ParseContext` scoped state — ✅ done (2026-07-15)

`ParseContext::parse_scoped(state, &mut parser)` (public; the pylatexenc
`walker.parse_content` analog on the context) plus the `pub(crate)`
`with_scoped_state` closure primitive replaced every hand-rolled swap/restore — seven
lib sites plus the two test-composition sites. The probe site is gone via
`probe_token`. Delta stays pass-through (caller-applies law). DESIGN_RATIONALE §3.6
entry added. ✅ `ParseContext::new(tokens, source, state, session)` added (2026-07-15,
user-approved); all construction sites migrated, fields stay public for access.
`ParserSession::parse(…)` as the top-level driver entry remains planned for Phase 7.

## E. `SourceResolver` — ✅ done (2026-07-15, all user-decided)

Settled before any consumer exists; DESIGN_RATIONALE §3.1 entry records the batch.

1. **✅ `Send + Sync` supertraits** added (thread-safe interior mutability for caches:
   locks/atomics, not `RefCell` — same contract note as `CallableSpec`).
2. **✅ Recursion: core does none of it** (user decision — core never interprets
   reference strings; the std/I/O command-line driver enforces its own depth/cycle
   policy via `provenance_chain()`). Documented on the trait.
3. **✅ Restructured: `resolve()` returns `ResolvedContent { content, origin }`**; the
   `resolve_source` composition mints the `Source` in core, stamping this trigger's
   provenance — the stale-provenance trap is now unrepresentable, and resolvers may
   cache content freely. Test pins two include sites getting distinct provenance.
4. **✅ `ResolveError`: strings + optional structured cause** (`Box<dyn core::error::
   Error + Send + Sync>` behind `with_cause`, exposed via `Error::source()`; no longer
   `Clone` — single-owner box, nothing relied on it).
5. **✅ Smalls**: forwarding impls (`&R`/`Box<R>`/`Arc<R>`), compile-time object-safety
   pin, `MapResolver::with_reference_as_origin()` (blanket impl narrowed to
   `O: From<String>`), future-tense doc claim fixed.

## F. `NodeRef` / `NodeTree` — settled (2026-07-15)

1. **`tree()` — declined** (user). The 6.5 record-resolving accessors
   (`argument_nodes`, `argument_content_nodes`, `slot_content_nodes`,
   `slot_content_parent`, `body`) cover the real need.
2. **✅ `parent` in `NodeData` — decided no** (user). Upward navigation not needed;
   recorded in DESIGN_RATIONALE §3.5.
3. **✅ `iter()` renamed `iter_storage_order()`** with a "not document order" doc;
   document-order `descendants()` deferred until the Phase 7 read API has a consumer.
4. **Named argument accessors — deferred** to the Phase 7 pylatexenc-style
   argument-access package (user: part of a more comprehensive package, not piecemeal).
5. **✅ `NodeTree::get(id)`** — done earlier (debug provenance tags on `NodeId`).

## G. `Span` — ✅ done (2026-07-15)

1. **✅ Private fields** (user decision): `start()`/`end()` accessors, monotone
   `extend_to(end)` for the sanctioned in-place growth, `cover(other)` (order-agnostic
   byte-range union) added; ~70 read sites swept mechanically; DESIGN_RATIONALE §3.1
   entry records the decision and the rejected `std::ops::Range` precedent.
2. `get()` non-panicking slice — done earlier. `contains`/`overlaps` deliberately
   deferred until a consumer exists (empty-span semantics must be pinned in the same
   commit — recorded in the §3.1 entry).
3. **✅ `SourceSpan` bridge**: `SourceSpan::new` accepts `impl Into<Range<usize>>`
   (a `Span` passes directly; call sites migrated), `From<Span> for Range<usize>`, and
   `SourceSpan::span()` as the inverse. `span.rs` stays ignorant of `SourceSpan`.
4. Doc clauses (struct-level "char boundaries" phrasing, `new`'s precondition) were
   rewritten as part of the privatization.

## H. `Lang` type-bundle gaps

1. **✅ `SlotExt` — done** (`NodeExtTypes::SlotExt`, `ParsedSlot.ext`, test +
   DESIGN_RATIONALE §3.5 entry).
2. **`SimpleLang` cliff — accepted for now, revisit later** (user): keep the note that
   helper macros (or a `LangTypes` split) are wanted once FLM-adjacent implementors
   multiply; 20 hand-written `Lang` impls in-crate at decision time.

## What remains on this action

- **B (all) + C-residue:** the dedicated plan discussion
  ([PlanSlotsAndConvenienceSurface.md](PlanSlotsAndConvenienceSurface.md)).

Everything else on this action is resolved (A, C, D, E, F, G, H — see the per-section
✅ marks and the DESIGN_RATIONALE entries in §3.1, §3.4, §3.5, §3.6).
