# Hook fallibility — discussion roster

**Status: RULED 2026-08-10 — see Decision record at the end.** Tier A and Tier B
become fallible; Tier C stays infallible with rationale docs. Concrete signatures
and ripples: PLAN.md Stage 3. The tier tables below are kept as the analysis record. The bindings report counts twenty
extension points that return a bare value with no way to report a failure, against
fourteen that return a `Result`. The blanket fix (a `Result` on all twenty) was
declined as churn; this file is the per-hook roster for deciding which ones have a
genuine case — including for Rust implementors — and which should instead be
documented as deliberately infallible.

For every hook below, "binding's reason" is what the Python-bindings project
concretely hit: a Python implementation can fail for reasons the hook cannot see
(an `AttributeError`, a `KeyError` in the author's lookup table, a
`KeyboardInterrupt`), the binding's own rules forbid swallowing the exception, and
each seam forced a different invented reporting policy.

A useful sorting question for every row: **is the failure a diagnosis of the
document, or a defect in the embedder's code?** Document-diagnoses want the ordinary
diagnostic channel; embedder defects fit `ImplementationError` — and for those, a
cheaper uniform answer than twenty signature changes may be a single session-level
"abort with implementation error" escape reachable from hooks.

Verified signature facts as of `df1d17a`: `ParsingState::derived` returns
`Result<_, DeriveError>`; `CommandResolution` has only `Resolved` and
`Unresolved { detail }` variants; `observe_parse_start` receives
`&mut Diagnostics` (the one bare-`()` hook that *does* have a report channel — the
report holds it up as the model shape).

## Tier A — a real Rust-side case exists (strongest candidates)

| Hook | Today | Binding's reason / workaround | Rust-side case |
|---|---|---|---|
| `GroupChildState::Compute` (`&dyn Fn(&Arc<ParsingState>, &Token) -> Arc<ParsingState>`) | must answer a state | a raising callback is reported via `sys.unraisablehook` and the *inherited* state is silently answered — the parse continues under the wrong state | **the callback's natural body calls `derived()`, which is fallible** (`DeriveError`); today a failed derivation can only panic or silently answer the wrong state |
| `InvocationChildState::Compute` (same, over `&Invocation`) | same | same | same |
| `Lang::initial_state_data() -> StateData` | must answer a seed | runs before any parse exists; a broken Python seed is *parsed with*, and the binding runs the whole parse to completion and discards it to report the failure | a seed assembled from config/files can be invalid; `Result<StateData, FinalizeError>` would surface it from `Language` construction exactly like `derived()` does |
| `ParseDriver::resolve_command(state, token) -> CommandResolution` | resolved or clean miss | park-and-re-raise from the enclosing `Language` operation | the all-`dyn` provider design exists to admit lazily-loaded providers; a provider that *fails to load* can today only mis-report as `Unresolved` — an "unknown command" diagnosis against the document for what is an I/O failure. Alternative to a `Result`: a third `CommandResolution` variant carrying the failure |
| `ParseDriver::resolve_state_event(event, stack) -> Option<Delta>` | delta or silently ignore | park-and-re-raise | an event carrying document-derived data can be malformed by the language's own rules; `None` silently ignores it — that is a document diagnosis with no channel |

## Tier B — deferral or a narrower fix already answers it

| Hook | Today | Binding's reason / workaround | Assessment |
|---|---|---|---|
| `TokenStopKind::Predicate(&dyn Fn(&Token) -> bool)` | bool | a raising predicate answers `false` = "keep parsing": the parse *succeeds* and the tree is silently wrong | sharpest correctness hole of the twenty, but tier-2 parser temporaries only — churn is confined to the parser that installs them. Decide together with the arms above |
| `StopSpec::node` (node-based stop) | value | same policy | same |
| `ParseDriver::make_nodes_parser` / `make_group_parser` (parser factories) | must answer a parser | the binding wraps returned parsers for its depth guard; failures park-and-re-raise | a factory that cannot build can answer a parser whose `parse()` immediately errs — `parse()` is already fallible. If that deferral is sanctioned, say so in the factory docs, and give the refusal its own condition instead of borrowing `ImplementationError` (the report's complaint). **Descent-exposed: the depth-guard work is touching exactly these.** |
| `CallableSpec::make_invocation_parser(invocation)` | must answer a parser | binding answers a stub parser whose `parse()` errs one call later than the failure | same deferral story; document it as the sanctioned shape |
| `EnvironmentBehavior::body_state_delta(invocation) -> Option<Delta>` | delta or `None` | `sys.unraisablehook`, answer `None` | the behavior reads argument data that can be malformed (that is a document diagnosis); but the shipped behaviors are pure. Middle-weight candidate |
| `Lang::make_node_ext(kind, span, state, children) -> NodeExt` | must answer a value | park-and-re-raise from the enclosing operation | computing an ext from spec data can hit contract violations; today panic or a poisoned value. Middle-weight |
| `ParseDriver::observe_transition(&mut SessionExt, prev, new, delta)` | `()` | park-and-re-raise | rather than a `Result`, consider giving it what `observe_parse_start` already has: a `&mut Diagnostics` sink. An observer that fails records a diagnostic; the parse never aborts on observation |

## Tier C — document as deliberately infallible (no signature change)

| Hook | Why infallible is right |
|---|---|
| `ParseDriver::recovery() -> Recovery` | pure policy read |
| `ParseDriver::refine_diagnostic(data, state) -> data` | identity fallback is always sound — a refiner that cannot refine passes the payload through |
| `ParseDriver::make_paragraph_break_node(...) -> NodeKind` | a fixed default node is always answerable |
| `ParseDriver::source_resolver() -> Option<&dyn SourceResolver>` | accessor; failure belongs on `SourceResolver::resolve`, which is already fallible |
| `Lang::specials_trigger_chars(data) -> TriggerChars` | a conservative superset is always answerable; the contract already says so |
| `ComposePiece::append(&mut self, other)` | both shipped impls (`String`, `()`) genuinely cannot fail; the report's own fallback ask is one sentence saying the infallibility is intentional |
| `LineColProvider::line_col -> Option<_>` | `Option` *is* the no-answer channel; the real defect was the renderer asserting a specific cause for every `None` (PLAN item 1.5) |

The one report request that stands regardless of tiering: for every hook that stays
infallible, **say so in its docs** — the bindings project spent four milestones not
knowing whether each absence of a `Result` was a constraint or an oversight.

## Decision record

**Ruled by the user, 2026-08-10:**

1. **Tier A hooks return `Result`.** Error channel stays simple: hooks inside a
   parse err with `ParseError<L::SourceOrigin>`; `initial_state_data` errs with
   `FinalizeError` (surfacing through the now-fallible `lang_initial` family).
2. **Tier B hooks return `Result` too** (narrow channel). In addition,
   `observe_transition` receives the diagnostics sink (`&mut Diagnostics` is
   already public API in this exact position on `observe_parse_start`) — sink for
   recording, `Err` for aborting; the roles are documented.
3. **A new general condition for operational hook failures — name `HookFailed`,
   ruled by the user 2026-08-10.** (Rationale on record: `ExtensionError` rejected —
   collides with the `NodeExt`/`StateExt`/`SessionExt` extension-*data* vocabulary;
   `OperationalError` rejected — vague, DB-API flavor.) `ImplementationError` keeps
   its verbatim meaning ("extension contract violation") and is not reused for
   operational failures. Cause-chain field modeled on `ResolveError`'s
   `Option<Arc<dyn Error + Send + Sync>>`: open sub-question, recommended yes
   (PLAN.md open question 1).
4. **Tier C hooks stay infallible**, each documented with the rationale and the
   recommended course of action for embedding code whose implementation can still
   fail (report through the embedding's own channel; answer the documented neutral
   value — pass the payload through, the default node, the conservative superset,
   `None`).
5. **Cost model caveat recorded:** for statically dispatched hooks (everything on
   `Lang` / `L::Driver` / specs — monomorphized), an infallible implementation lets
   the compiler fold the `Result` channel away, so the channel is free. For the
   `&dyn Fn` callbacks (`Predicate`, `StopSpec::node`, both `Compute` arms) the
   callee is opaque and the `Result` is materialized on every call — still cheap,
   but on the hottest loops; Stage 3 carries a size gate (box the `Err` arm if
   frames bloat, following the descent merge's boxed-deltas precedent).
6. **Batching:** the whole sweep lands as one breaking unit (`ParseDriver` already
   changed in the descent merge; this is the second and last planned sweep).
