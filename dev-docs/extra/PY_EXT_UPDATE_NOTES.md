# Update notes for the techy-py bindings project

*Courtesy notes accompanying the techy changes made in response to
`PY_EXT_BINDINGS_REPORTED_GAPS_WISHES.md`. Everything below is on techy `main`
as one batch; the fallibility sweep landed as a single breaking unit so
implementors adapt once. Rationale for each decision lives in
`dev-docs/DESIGN_RATIONALE.md` under the labels quoted at the end.*

---

## 1. The `LineIndex` multi-byte panic is fixed

`LineIndex`'s incremental scan no longer records a resume point inside a
multi-byte UTF-8 character, so `display_tree` (and any `line_col` sequence) over
content whose first character is multi-byte no longer panics. Consequence for
your suite: **`test_display_contains_techys_multibyte_renderer_panic` is
designed to fail against this techy version** — that is the signal that the
containment (the pre-check guard around the renderer) can come out.

Related message fixes: the `UnresolvableSourceReference` message now names the
reference *as written* in the document (the structured field was already
correct); `format_position_with`'s no-line-info fallback now reads
`@ char pos N (no line info)` without asserting a cause (a consumer-supplied
`LineColProvider` may answer `None` for any reason).

## 2. Signature changes: the hook-fallibility sweep

Fourteen extension points now return `Result` (previously bare values). Each is
one line here; the semantics are on each hook's rustdoc. Hooks inside a parse
err with `ParseError<L::SourceOrigin>`; an `Err` aborts under any recovery
policy; hook errors with no frames get the live traceback attached at the
consultation site.

- `GroupChildState::Compute`: `&dyn Fn(&Arc<ParsingState<L>>, &Token<'_, L>) -> Result<Arc<ParsingState<L>>, ParseError<_>>`
- `InvocationChildState::Compute`: same shape, over `&Invocation<'_, '_, L>`
- `TokenStopKind::Predicate`: `&dyn Fn(&Token<'_, L>) -> Result<bool, ParseError<_>>`
- `StopSpec::node` (node-based stop): same `Result<bool, _>` shape
- `Lang::initial_state_data() -> Result<StateData<Self>, FinalizeError>`
- `ParseDriver::make_nodes_parser` / `make_group_parser` / `make_invocation_parser`: `-> Result<Box<dyn ConstructParser<…>>, ParseError<_>>` (an `Err` is "could not build the parser" — distinct from the descent guard's depth refusal)
- `CallableSpec::make_invocation_parser`: same — your stub-parser workaround (a parser whose `parse()` errs one call later) is no longer needed
- `ParseDriver::resolve_command` (and `CommandResolver::resolve_command`, in step): `-> Result<CommandResolution<L>, ParseError<_>>`; `CommandResolution::Failed` stays the diagnose-and-recover outcome, `Err` is the abort channel
- `ParseDriver::resolve_state_event`: `-> Result<Option<ParsingStateDelta<L>>, ParseError<_>>`
- `EnvironmentBehavior::body_state_delta`: `-> Result<Option<ParsingStateDelta<LLL>>, ParseError<_>>`
- `Lang::make_node_ext`: `-> Result<NodeExt<Self>, NodeBuildError>` — a refused mint is the new `NodeBuildError::ExtMintFailed { detail }` (message-only; render your cause chain into `detail`)
- `ParseDriver::observe_transition`: now `(&self, ext: &mut L::SessionExt, diagnostics: &mut Diagnostics<_>, prev, new, delta) -> Result<(), ParseError<_>>` — the sink records without affecting the parse (an error-severity diagnostic does not abort); `Err` aborts
- `ParseContext::stage_invocation`: a bad computed span is now an `Err` (`ImplementationError`), no longer a panic

Ripples and companions:

- `ParsingState::lang_initial()` / `lang_initial_with_packages(…)` return
  `Result<ParsingState<L>, FinalizeError>` (the seed hook may refuse).
- `ParseResult` gained the public field `session_ext: L::SessionExt` — the
  read-back for `observe_transition` accumulation (previously dropped by
  `finish`).
- `ParseDriver::diagnostics_limit(&self) -> Option<usize>` (defaulted `None`):
  the per-parse retention cap for the diagnostics sink; hand-built sessions
  apply `with_limit` themselves.
- For Python-raised exceptions inside hooks, the intended condition split is:
  `HookFailed` for operational failures (see below), `ImplementationError` for
  violated techy contracts, any domain condition for document diagnoses. Your
  park-and-re-raise and `sys.unraisablehook` policies can retire on these seams.

The seven hooks that stayed infallible (`recovery`, `refine_diagnostic`,
`make_paragraph_break_node`, `source_resolver`, `specials_trigger_chars`,
`ComposePiece::append`, `LineColProvider::line_col`) now each document that the
infallibility is deliberate, plus the neutral value a failing embedding should
answer while reporting through its own channel.

Also semver-visible from the same batch: `SpecsProvider`, `ArgumentParser`,
`EnvironmentBehavior`, and `SourceResolver` gained an `Any` supertrait
(downcasting from stored `Arc<dyn _>` is now sanctioned, as it already was for
`CallableSpec`); `MacroSpec` gained a private field, so struct-literal
construction is no longer possible (use `new()`/builders).

## 3. New API closing the filed gaps

- **`HookFailed`** (`techy::error`, identifier `core.hooks.hook-failed`): the
  general condition for operational hook failures — `new(detail, cause)` with
  `cause: Option<Arc<dyn Error + Send + Sync>>`, `with_cause(…)` by-value
  sugar, cause chain rendered by `serializable_data`. This is the condition to
  carry a translated Python exception.
- **`DiagnosticInfo::identifier(&self)`** (defaulted): binding adapter types may
  answer a per-instance identifier — the seam for Python-defined conditions
  carried by one Rust adapter type. The const stays the norm; note the E0034
  ambiguity for downstream code importing both `DiagnosticInfo` and
  `DiagnosticData` (use the qualified spelling).
- **`Language::new(driver, impl Into<Arc<ParsingState<L>>>)`**: seed a
  `Language` from an already-shared state handle with identity preserved
  (`Arc::ptr_eq` holds against `initial_state()`); by-value call sites compile
  unchanged.
- **`NodeTree::slice(range) -> Option<NodeSlice>`**: validated public
  constructor — `Some` iff the range is a sibling run.
- **`NodeTree::tree_tag()`** is public: pre-check a `NodeId` against a tree
  before the panicking accessors.
- **`TreeViolation::new(node, kind)`**: manufacture values to test your
  `validate_tree` handling code.
- **`MalformedBegin::new()`**: the condition now has the derive-emitted
  constructor like the other shipped conditions.
- **`MacroSpec::with_after_effect(delta)`**: the declarative sibling
  after-effect route (previously reachable only via a takeover parser).
- **`ChildRegion::staged()`** is public — the non-panicking companion of the
  resolved-only accessors, whose Panics sections now point at
  `is_resolved()`/`staged()`.
- **`TreeViolationKind::as_str()` / `TokenKind::as_str()`** (`pub const fn`):
  stable variant-name projections for logs/config matching.
- **`Package::scan_specials`** with an invalid `pos` (out of range or not a
  char boundary) returns the token-error path (`ImplementationError` semantics)
  instead of panicking; the `pos` contract is documented on the trait.
- **Documentation batch**: source identity on `Language::parse`/`Source::new`
  (fresh identity per bare-`&str` parse), `GroupRule` identity vs structural
  `==`, `ParseDriver` intra-trait dispatch + wrapping-a-driver notes,
  `ParseContext::session` reachable surface, replay granularity in the
  custom-language guide, `parse_attached_source`/`attach_source_reference`/
  `group_interior_state` chapter coverage, `Package::get` vs specials,
  `NodeRef::summary()` stability caveat, the extract `*_keep_annotations`
  triple, and guide rows for the new items above.

## 4. Declined, with rationale on record

The following were ruled out; the arguments live in
`dev-docs/DESIGN_RATIONALE.md` under these labels:

- `ParsingState::data()`/`from_data()`, `ConcatPieces::into_parts`, a
  `test-support` cargo feature, and the small-accessor batch
  (`NodeTree::id_at`, `Diagnostic::with_frames`, `NamedAccessError`/
  `ArgumentCodeError` accessors, `KeyValEntry::value_node`, and the rest) —
  **[§dd-dr:embedding-feedback-policy]** (states stay crate-frozen; build-only
  instruction types; fixtures are the embedder's, indispensable items graduate
  to real API).
- The deliberately infallible hooks and the neutral-answer guidance —
  **[§dd-dr:hook-fallibility]** (also the full fallibility reasoning and the
  `HookFailed` split).
- The scope of the `identifier()` override (binding/embedding adapters only) —
  **[§dd-dr:runtime-condition-identity]**.
