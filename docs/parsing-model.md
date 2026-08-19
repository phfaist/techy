# The parsing model

This chapter is the map of how techy executes a parse: what happens when you
call [`parse()`](crate::core::Language::parse), which component decides what
at each step, and — most importantly for a reader of the Developer Guide —
where the extension points sit. It stays deliberately high-level: the
contracts live in the API documentation of the [`core`](crate::core) hub and
its [`core::constructs`](crate::core::constructs) module, and this chapter
points there rather than repeating them. The User Guide chapter
[Running the parser](crate::guide::parsing) covers the everyday, outside view
of the same machinery.

Two recurring terms, up front. A **construct parser** is an object that
parses one syntactic construct — a group, a callable invocation, a run of
sibling content — implementing the
[`ConstructParser`](crate::core::constructs::ConstructParser) trait
([concepts](crate::guide::concepts_overview#construct-parsers)). A **spec**
is the stored description of a
[callable](crate::guide::concepts_overview#callable-specs-and-arguments)'s
behavior — a [`CallableSpec`](crate::core::specs::CallableSpec)
implementation, registered under a name, resolved at parse time.

## Anatomy of a parse

A [`Language`](crate::core::Language) bundles what outlives any one parse:
the frozen initial [parsing
state](crate::guide::concepts_overview#parsing-state-and-deltas) and the
language's **driver** — its
[`ParseDriver`](crate::core::ParseDriver) instance, the value that carries
all parse-time behavior (recovery policy, command resolution, the parse-time
hooks). Calling [`parse()`](crate::core::Language::parse) (or
[`parse_source`](crate::core::Language::parse_source), the same entry for a
pre-minted [`Source`](crate::source::Source)) sets up three transient
objects and runs the parse to completion:

- a **token reader** — the one the driver's
  [`make_token_reader`](crate::core::ParseDriver::make_token_reader) hook
  returns, which by default is the reader the language's
  [`Tokenization`](crate::core::Lang::Tokenization) names
  ([`StdTokenReader`](crate::core::token::StdTokenReader) unless the language
  supplies its own) — over the source content, which produces
  [`Token`](crate::core::token::Token)s on demand under whatever token rules the
  current parsing state holds. A token is opaque: whoever holds one asks the
  reader what it is and where it is
  ([the concept](crate::guide::concepts_overview#tokens-and-token-rules));
- a **session** ([`ParserSession`](crate::core::ParserSession)) — the root
  object of the parse, accumulating everything the parse produces: the
  staged nodes, the [diagnostics](crate::error::Diagnostics) sink, and the
  live frame stack used for error tracebacks. Sessions are one-parse
  scratch: no reuse, no configuration;
- a **parse context** ([`ParseContext`](crate::core::constructs::ParseContext))
  — the single value handed to every construct parser, bundling the token
  reader, the current parsing state, the session, and the driver.

The entry point then drives the **root content loop** over the whole source,
stages a root `List` node spanning it, and freezes the session into a
[`ParseResult`](crate::core::ParseResult) — the tree plus the diagnostics.
The result holds no reference to the `Language`; results outlive their
bundle. The exact sequence, including how a stray `}` at the top level is
diagnosed and skipped, is documented on
[`Language::parse_source`](crate::core::Language::parse_source).

"Staging" is the parse-side word for node creation: a construct parser
stages a node into the session through
[`ParseContext::stage_node`](crate::core::constructs::ParseContext::stage_node)
— the single entry point through which every node is staged —
receiving a build-time id; the caller later claims staged nodes as children
of the node it stages itself. The tree is assembled bottom-up and becomes a
readable [`NodeTree`](crate::core::node::NodeTree) only at the final freeze.

## The content dispatch loop

The core of the parse is the content loop,
[`NodesParser`](crate::core::constructs::NodesParser): it parses a run of
sibling nodes — the top-level content, a group's interior, and an
environment's body each run one. The loop peeks one token at a time and
dispatches on the token's kind — there is no registry of parsers keyed by
syntax:

- **Character tokens** accumulate into maximal character-run nodes;
  **comment tokens** become comment nodes; **paragraph-break tokens**
  become the node the driver's
  [`make_paragraph_break_node`](crate::core::ParseDriver::make_paragraph_break_node)
  hook chooses. These arms stage directly, no descent.
- **A group-open token** starts a descent: the loop consumes the open
  delimiter and runs a group parser
  ([`GroupParser`](crate::core::constructs::GroupParser)) over the interior,
  under an interior state derived for the entered group rule (see
  [below](#how-parsing-state-flows)).
- **A command token** (`\emph`) is where definition lookup happens: the
  loop asks the driver's
  [`resolve_command`](crate::core::ParseDriver::resolve_command) hook to
  resolve the token under the current state — the hook receives the token
  and the reader that produced it, so a language may decide on any detail of
  the trigger, not just its name. A successful resolution names
  the invocation form and the spec
  ([`ResolvedCallable`](crate::core::specs::ResolvedCallable)); the loop
  then consumes the trigger token, builds an
  [`Invocation`](crate::core::constructs::Invocation) bundle, and runs the
  invocation parser supplied for this spec (next section). An unresolved
  command is diagnosed
  ([`UnresolvableCommand`](crate::core::constructs::UnresolvableCommand))
  and, in tolerant parsing, recovered as a character-run node over the
  token's span.
- **A specials token** (`~`, `--`) skips resolution entirely: for specials,
  recognition *is* resolution — the reader's answer for the token already
  names the spec the specials scan resolved (see
  [`TokenKind::Specials`](crate::core::token::TokenKind); the asymmetry is
  documented on [`resolve_command`](crate::core::ParseDriver::resolve_command)).
  The loop dispatches the invocation the same way from there.

The loop's own contract — its stop conditions
([`StopSpec`](crate::core::constructs::StopSpec)), how a run reports its
ending as data ([`StopCause`](crate::core::constructs::StopCause)), what a
finished run exports ([`NodesOutcome`](crate::core::constructs::NodesOutcome)),
and the whitespace and span invariants — is on the
[`NodesParser`](crate::core::constructs::NodesParser) page.

## Specs supply the parser for their invocation

When a command resolves (or a specials trigger fires), the construct parser
that takes over is chosen by the *definition*, not by the loop: every
[`CallableSpec`](crate::core::specs::CallableSpec) has a factory method,
[`make_invocation_parser`](crate::core::specs::CallableSpec::make_invocation_parser),
returning a fresh parser for each resolved invocation. This is the spec
trait's behavioral surface, and it is what makes definitions extensible all
the way down:

- The **default** factory returns the standard declarative parser
  ([`StdInvocationParser`](crate::core::constructs::StdInvocationParser)),
  which parses the arguments the spec declares (its
  [`ArgumentSpec`](crate::core::specs::ArgumentSpec) list — each argument
  *is* a parser, an
  [`ArgumentParser`](crate::core::constructs::ArgumentParser)
  implementation) and stages the standard callable node.
- **Overriding** the factory is the full-takeover route: the spec's own
  parser reads tokens however it wants — raw `\verb` content, an
  environment body up to `\end{name}` — and stages its own node shape.
  [Custom construct parsers](crate::guide::construct_parsers) is the
  chapter on writing one.

The driver sits between the loop and the spec as an interception point: the
dispatch arms obtain every invocation parser through
[`ParseDriver::make_invocation_parser`](crate::core::ParseDriver::make_invocation_parser),
whose default delegates to the spec's factory — a custom driver can wrap or
substitute parsers uniformly, which no per-spec override could do.

Which definitions are visible in the first place is the business of the
[scope stack](crate::guide::concepts_overview#scopes-and-packages) stored in
the parsing state; the standard resolution body is
[`resolve_command_in_scopes`](crate::core::specs::resolve_command_in_scopes),
packaged as the
[`ScopesCommandResolver`](crate::core::specs::ScopesCommandResolver)
strategy. A driver is free to resolve differently — resolution is a hook,
not a fixed rule.

## How parsing state flows

Parsing state is immutable: a
[`ParsingState`](crate::core::ParsingState) is a frozen snapshot of the
token rules, the parsing mode, the visible definitions, and the language's
own state extension. Nothing ever mutates a state; change is expressed as a
[`ParsingStateDelta`](crate::core::ParsingStateDelta) — a plain value
listing overrides and operations, not a closure — and applying a delta
produces a *new* state.

There is exactly one derivation point:
[`ParsingState::derived`](crate::core::ParsingState::derived) is the sole
constructor of non-initial states. It applies the delta and runs the
language's transition customizer,
[`Lang::finalize_transition`](crate::core::Lang::finalize_transition),
exactly once before freezing the result — so any cross-cutting rule the
language maintains ("in math mode the escape character changes") holds for
every state a parse can ever see. The seed state comes from
[`ParsingState::lang_initial`](crate::core::ParsingState::lang_initial) and
is the one state that does not pass through the customizer (it has no
predecessor; the language author guarantees its coherence — see the hook's
documentation).

Inside a driven parse, construct parsers do not call `derived` directly:
they derive through the context
([`ParseContext::derive_state`](crate::core::constructs::ParseContext::derive_state)),
which routes through the session so the driver observes every transition
and identical derivations are deduplicated
([`ParserSession::derived_state`](crate::core::ParserSession::derived_state)
documents that mechanism).

Two conventions govern where a state change *applies* — both pinned in the
[`core::constructs`](crate::core::constructs) module documentation:

- **Scoped descent.** A construct that parses child content under a
  modified state — a group interior, an argument extent — derives the child
  state and scopes it structurally: the outer state is restored when the
  descent returns, because the caller still holds it. Nothing needs to be
  undone; leaving a scope means the caller continues with the `Arc` of the
  outer state it kept all along. Group interiors get their state through a
  dedicated channel: the descent invariant (the interior always expects the
  entered rule's close delimiter) merged with the driver's
  [`group_interior_delta`](crate::core::ParseDriver::group_interior_delta)
  hook — the data channel by which a group class changes its interior's state
  (the preset's math groups enter math mode this way).
- **The after-effect channel.** A construct whose effect must *outlive* it
  — `\newcommand` defining a macro for the rest of the document — returns
  its delta to the caller instead of applying it:
  [`ConstructParser::parse`](crate::core::constructs::ConstructParser::parse)'s
  return value pairs the output with an optional delta, and the *caller*
  decides where it applies. The content loop applies a returned
  after-effect to its own live state, so it holds for the following
  siblings, and exports the merged record on its outcome
  ([`NodesOutcome::after_effects`](crate::core::constructs::NodesOutcome))
  for callers that propagate effects further. That a definition made inside
  a group ends with the group falls out structurally: the loop's evolved
  state is dropped with the descent, and parsing resumes under the outer
  state. A language whose constructs may *escape* their group — TeX's
  `\gdef` — installs the
  [`GroupAfterEffectsFn`](crate::core::constructs::GroupAfterEffectsFn) hook
  through its
  [`make_group_parser`](crate::core::ParseDriver::make_group_parser), which
  maps the interior's record to the group's own after-effect; the content
  loop then applies and records it like an invocation's, so an escape
  composes outward one nesting level per hook.

## How problems flow

Problems surface as **conditions** — typed values, one concrete type per
kind of problem
([concepts](crate::guide::concepts_overview#diagnostics-and-tolerant-parsing)).
The flow has one entry point: a construct parser that detects a problem calls
[`ParseContext::recover`](crate::core::constructs::ParseContext::recover)
with the condition and its span. `recover` hands the condition to the
driver ([`ParseDriver::recover`](crate::core::ParseDriver::recover)), which
applies its policy:

- **Tolerant** ([`Recovery::Tolerant`](crate::error::Recovery)): the
  condition is recorded as an error-severity
  [`Diagnostic`](crate::error::Diagnostic) — with a snapshot of the live
  frame stack as its traceback — and `recover` returns `Ok`. The parser
  then applies its **documented local recovery at the detection site** (each
  condition type's page states its recovery) and continues. Recovery is
  never deferred upward: by the time a caller sees control again, the
  problem is already repaired.
- **Strict** ([`Recovery::Strict`](crate::error::Recovery)): the condition
  comes back as a [`ParseError`](crate::error::ParseError), and the parser
  propagates it. An `Err` from a construct parser means **abort** — nobody
  continues past an `Err`; that is the module-level error contract of
  [`core::constructs`](crate::core::constructs).

The default driver path also gives the language one refinement pass
([`refine_diagnostic`](crate::core::ParseDriver::refine_diagnostic)):
replacing a generic condition with a language-specific one — still a typed
value — before it is recorded. And a custom driver can override
[`recover`](crate::core::ParseDriver::recover) itself to a policy richer
than the strict/tolerant pair (per-condition decisions, budgets).

**Implementation errors are a separate path.** When an *extension* violates
a library contract — a custom parser stages children the builder rejects, a
hook breaks its purity obligation — that is not a source-input problem, and
it deliberately bypasses `recover` and the recovery policy:
[`ParseContext::implementation_error`](crate::core::constructs::ParseContext::implementation_error)
builds an [`ImplementationError`](crate::core::constructs::ImplementationError)
abort that no recovery policy can absorb. A bug in extension code fails
loudly even in tolerant parsing. The split is three-way:
`ImplementationError` for contract violations,
[`HookFailed`](crate::error::HookFailed) for operational failures in
consumer-supplied hook code (an input/output failure, a runtime failure
behind a language binding — also an abort under any policy), and ordinary
domain conditions — through `recover` — for problems a hook diagnoses in
the parsed document.

## Where the extension points are

Reading the sections above as a checklist, from the least to the most
invasive way of changing how parsing works:

- **Definitions** — register different specs. No new code beyond spec
  values; the User Guide chapter
  [Defining macros, environments, and specials](crate::guide::specs) covers
  it for the latexlike preset,
  [`core::specs`](crate::core::specs) is the general contract.
- **A spec with custom parsing** — override
  [`make_invocation_parser`](crate::core::specs::CallableSpec::make_invocation_parser)
  (or supply custom
  [`ArgumentParser`](crate::core::constructs::ArgumentParser)s) to change
  how one callable's invocations parse, from argument quirks to full
  takeover. See [Custom construct parsers](crate::guide::construct_parsers).
- **The driver** — implement
  [`ParseDriver`](crate::core::ParseDriver) (every method has a working
  default) to change parse-wide behavior: the recovery policy, command
  resolution, condition refinement, group descent deltas, paragraph-break
  emission — and construct provision: the driver's
  [`make_nodes_parser`](crate::core::ParseDriver::make_nodes_parser) /
  [`make_group_parser`](crate::core::ParseDriver::make_group_parser) /
  [`make_invocation_parser`](crate::core::ParseDriver::make_invocation_parser)
  factories are routed through by *every* descent site, so one override
  applies uniformly to the whole parse.
- **The language** — implement [`Lang`](crate::core::Lang) itself: the
  vocabularies (group classes, callable types, modes), the token rules and
  specials recognition, the state extension and transition customizer, the
  node extension types. That is the subject of
  [Defining a custom language](crate::guide::custom_lang).

The [`core`](crate::core) module page is the reference map of the machinery
hub; [`core::constructs`](crate::core::constructs) is the reference for the
parsing layer's contracts.

Read next: back to the [Developer Guide](crate::guide#developer-guide) index —
the other chapters on extending and embedding techy.
