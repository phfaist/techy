**Subphase 6.4 is complete: 230 tests green (229 lib + 1 doc, up from 220), zero warnings, docs updated.** The full invocation dispatch loop is now live for argument-less callables, and the escape hatch is proven. Nothing is committed — all changes are in the working tree for your review.

## What shipped

- **`CallableSpec::make_invocation_parser`** (`spec/callable.rs`) — the defaulted factory per the decided §2 shape, returning a fresh boxed parser per invocation; overriding it is the full-takeover escape hatch.
- **`StdInvocationParser`** (new `src/constructs/invocation_parser.rs`) — zero-arg/zero-slot: owned name, empty `ParsedArguments`/`ParsedSlots`, node span = trigger token span.
- **`Command`/`Specials` arms** (`nodes_parser.rs`) — `Command` resolves via `Lang::resolve_command` under the loop's own state (resolution precedes policy); `None` keeps the chars-fallback recovery; `Specials` dispatches the resolution riding on the token. The `InvocationChildState` policy is now consulted, and after-invocation deltas are applied session-mediated to the loop's state for subsequent siblings.
- **Tests** — macro and specials end-to-end against both readers, post-space placement (multi-char, `\&`, paragraph-break cutoff), library-miss recovery, the `\newcommand`-shaped push-library delta, a raw-token takeover parser (consumes markup up to a `!` marker, stages an untyped-group shape, returns a comments-disabling delta — the C6 proof), the `finalize_node` callable-ext rehearsal, and `Fixed`/`Compute` invocation policies.

## Three user-approved amendments (recorded in DESIGN_RATIONALE, ARCHITECTURE, and the plan's progress table)

1. **`post_space` = exactly the trigger token's own syntactic post-space; nothing claimed beyond it** — your revision, matching TeX/pylatexenc (`\& b`'s space is sibling content). `claim_post_space` was never shipped; §3.5 invariant 3 amended.
2. **`ParseContext.source`** — `NodesParser::new`/`GroupParser::new` dropped their `source` params; factory-created and takeover parsers stage `SourceSpan`s through the context.
3. **`TokenKind::Specials`/`SpecialsMatch` carry `callable_type`** — recognition = resolution, so the full `ResolvedCallable` pair rides the token; §3.2's "no `CallableTypeId` on tokens" bullet was scoped to `Command`.

## Two things flagged for 6.5

- `check_tree_invariants` rule 3 asserts a `Spanned` callable post-space is *trailing* — true for zero-arg nodes, but once arguments land the post-space sits between the name and the first region, so that assertion needs revising in 6.5 (noted in the progress table).
- `StdInvocationParser` carries a `debug_assert` that the spec declares no arguments/slots — the explicit 6.4 scope boundary, to be removed when 6.5 completes the parser.