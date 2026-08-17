# Custom construct parsers

A **construct parser** parses one syntactic construct — reading tokens,
staging nodes, reporting problems — through the
[`ConstructParser`](crate::core::constructs::ConstructParser) trait. The
standard parsers (the content loop, groups, invocations, arguments,
environment bodies, verbatim) are all implementations of it, and so is any
parser you write. This chapter shows how to write one: what a parser
receives and returns, how a definition takes over parsing of its own
invocations, how argument parsing plugs in, and how to raise conditions. It
assumes the execution picture from
[The parsing model](crate::guide::parsing_model); the contracts themselves
live in the [`core::constructs`](crate::core::constructs) module
documentation, which this chapter points into. A complete, compile-checked
takeover parser closes the chapter.

## The trait, and the two-tier ownership model

The trait's shape ([`ConstructParser`](crate::core::constructs::ConstructParser)):

```text
trait ConstructParser<L: Lang> {
    type Output;
    fn parse(&mut self, cx: &mut ParseContext<'_, '_, L>)
        -> ConstructParserResult<L, (Self::Output, Option<Box<ParsingStateDelta<L>>>)>;
}
```

`Output` is whatever the parser produces — typically one staged node id
([`BuildId`](crate::core::node::BuildId)), sometimes a list or a richer
outcome value. The rest of the signature carries the two standing
conventions:

- **Construct parsers are temporaries.** A parser is constructed with its
  per-use configuration where it is needed, keeps working state in its own
  fields (`parse` takes `&mut self`), may freely borrow, and is dropped
  when its construct's parse ends. Contrast the *stored* behavior objects —
  specs and argument parsers — which are `Arc`-shared, immutable, and
  `Send + Sync` by contract. The
  [`core::constructs`](crate::core::constructs) module docs call this the
  two-tier ownership model.
- **`Err` means abort.** A construct parser that returns `Err` ends the
  whole parse; recovery from source problems happens *before* returning,
  at the detection site (see [Raising conditions](#raising-conditions)
  below).

The second element of the success pair is the construct's **after-effect
delta** — covered [below](#what-a-parser-returns).

## What a parser receives: the `ParseContext`

Every parser gets one context value,
[`ParseContext`](crate::core::constructs::ParseContext), bundling the four
parse inputs — the token reader, the input parsing state, the session, and
the driver. Its methods are the parser's entire toolkit:

**Token reading.** `cx.tokens` is the
[`TokenReader`](crate::core::TokenReader): peek or consume tokens under an
explicitly passed state. Prefer
[`cx.probe_token(&state)`](crate::core::constructs::ParseContext::probe_token)
over a raw peek: it maps tokenizer errors per the recovery policy (strict:
abort; tolerant: report `None` so you treat the place you stand as
unusable, while the enclosing content loop takes care of diagnosing the
token — the probe protocol is documented on the method).

A token is an opaque value. Your parser holds it and passes it on; the
reader that produced it answers every question about it, and there are
three questions to ask:

- *What is this token?*
  [`cx.tokens.token_kind(&token)`](crate::core::TokenReader::token_kind)
  answers with a [`TokenKind`](crate::core::TokenKind) — the character, the
  command name and its escape character, the group delimiter and its rule,
  and so on. The answer borrows from the token, and stays usable as long as
  you hold it.
- *Where is it in the text?*
  [`cx.tokens.source_span_of(&token)`](crate::core::TokenReader::source_span_of)
  answers with the token's own [`SourceSpan`](crate::source::SourceSpan) — a
  source together with a byte range in it — and
  [`source_span_between(&token, a, b)`](crate::core::TokenReader::source_span_between)
  with the span between two of the token's *edges*.
  [`TokenEdge`](crate::core::TokenEdge) names the five boundaries of a token
  in reading order, from where its leading whitespace begins to where its
  trailing whitespace ends.
- *Where does the stream stand?* A **stream position** names a place in the
  token stream. It is opaque as well, and the reader is the only source of
  one: [`position_here()`](crate::core::TokenReader::position_here) for the
  place the stream stands at, and
  [`position_at(&token, edge)`](crate::core::TokenReader::position_at) for an
  edge of a token. Two positions become a span through
  [`cx.source_span_within(&begin, &end)`](crate::core::constructs::ParseContext::source_span_within),
  which is how a construct reading several tokens computes the span it
  stages its node with;
  [`cx.here()`](crate::core::constructs::ParseContext::here) is the empty
  span at the current position, the anchor for a problem reported where the
  parser stands.

Repositioning takes a token or a position:
[`cx.tokens.move_to(&token, edge)`](crate::core::TokenReader::move_to) puts
the stream at an edge of a token the reader read, and
[`move_to_position(&position)`](crate::core::TokenReader::move_to_position)
at a position it handed out earlier.

**Node staging.**
[`cx.stage_node(kind, span, state, children)`](crate::core::constructs::ParseContext::stage_node)
is the single staging entry point — every node is staged through it. It mints the node's language extension
([`Lang::make_node_ext`](crate::core::Lang::make_node_ext)) and stages the
node, returning a `Result`: its [`BuildId`](crate::core::node::BuildId) on
success, or a [`NodeBuildError`](crate::core::node::NodeBuildError) to lift
with
[`implementation_error`](crate::core::constructs::ParseContext::implementation_error)
— a staging failure is an extension-contract violation, never something
tolerant recovery may swallow (the chapter's closing example shows the
lift) — except for the ext mint's own reported failure
([`ExtMintFailed`](crate::core::node::NodeBuildError::ExtMintFailed)),
which is an operational failure to lift as a
[`HookFailed`](crate::error::HookFailed) condition (also an abort under
any policy; `stage_node`'s page states the split). Children are
staged first, bottom-up, and claimed by the parent's own staging call.
[`cx.staged_nodes()`](crate::core::constructs::ParseContext::staged_nodes)
is the read-only view over what has been staged so far.

**State derivation and scoping.** `cx.state` is the parser's *input* state
— the caller sets it.
[`cx.derive_state(&delta)`](crate::core::constructs::ParseContext::derive_state)
is the parser-facing derivation entry point (event lowering plus the
session-mediated derivation);
[`cx.with_parsing_state(state, f)`](crate::core::constructs::ParseContext::with_parsing_state)
scopes a derived state over a closure with structural restore, and
[`with_derived_state`](crate::core::constructs::ParseContext::with_derived_state)
is the same primitive in delta-shaped form. Both are state-scoping
utilities only, never a route into a sub-parse — running another construct
parser goes through `parse_construct` (next paragraph). A group descent's
interior state has its own derivation entry point:
[`cx.group_interior_state(&rule)`](crate::core::constructs::ParseContext::group_interior_state)
derives the state a just-entered group's interior parses under — the
entered rule installed as the expected close, merged with the driver's
[`group_interior_delta`](crate::core::ParseDriver::group_interior_delta)
(how a group class changes its interior's state) — and the session
**memoizes** the result per (base state, rule) pair for the whole parse.
Use it rather than assembling the interior delta by hand: a hand
derivation forfeits the memo and, worse, silently loses the driver's delta
— under the shipped preset, a math group whose interior never enters math
mode.

**Descents.** Running another construct parser — child content, a group, a
body — has exactly one entry point, and using it is a **MUST** of the
[`ConstructParser`](crate::core::constructs::ConstructParser) contract:
[`cx.parse_construct(parser, state, frame)`](crate::core::constructs::ParseContext::parse_construct)
scopes the sub-parse's input state (`None` runs it under the current
state) and optionally pushes a traceback frame around the run. Two thin
wrappers delegate to it, adding the driver's parser factories:
[`cx.parse_nodes(state, stop, child_states)`](crate::core::constructs::ParseContext::parse_nodes)
runs one content-loop descent and
[`cx.parse_group(…)`](crate::core::constructs::ParseContext::parse_group)
one group descent — never instantiate the loop parsers yourself; going
through the wrappers is what makes driver overrides apply uniformly. The
stop conditions
([`StopSpec`](crate::core::constructs::StopSpec)) and the outcome contract
([`NodesOutcome`](crate::core::constructs::NodesOutcome),
[`StopCause`](crate::core::constructs::StopCause)) are documented with
[`NodesParser`](crate::core::constructs::NodesParser).

**Attached sources** (`\input`-style inclusion). Two entry points sub-parse
another [`Source`](crate::source::Source) into the running parse, and the
choice between them is the whole decision.
[`cx.attach_source_reference(reference, at, state, parser)`](crate::core::constructs::ParseContext::attach_source_reference)
is the resolve-and-diagnose form: it looks `reference` up through the
driver's
[`source_resolver`](crate::core::ParseDriver::source_resolver), raises the
two failure conditions
([`NoSourceResolver`](crate::core::constructs::NoSourceResolver),
[`UnresolvableSourceReference`](crate::core::constructs::UnresolvableSourceReference))
through the recovery entry point — under tolerant recovery the failure is
recorded and `Ok(None)` comes back with nothing attached — and delegates on
success.
[`cx.parse_attached_source(source, state, parser)`](crate::core::constructs::ParseContext::parse_attached_source)
is the form underneath it, for when you already hold the minted
[`Source`](crate::source::Source). The sub-parse joins the **running
session** — same builder, so the staged ids it returns are yours to stage
(for `\input`, as an
[`Attached`](crate::core::node::SlotRole::Attached) slot of your callable
node) — and drives the parser you supply (built from the driver's
[`make_nodes_parser`](crate::core::ParseDriver::make_nodes_parser) factory
for the `\input` shape) through `parse_construct` like any descent, the
descent guard included. Both return an
[`AttachedSourceOutcome`](crate::core::constructs::AttachedSourceOutcome):
the content nodes plus the included run's merged after-effect record, which
your parser forwards as its own after-effect or drops — the
persist-vs-transparent choice
([`input_macro_spec`](crate::latexlike::input_macro_spec)'s `persist_state`
parameter).

**Problem channels.**
[`cx.recover(condition, span)`](crate::core::constructs::ParseContext::recover)
is the recovery entry point for problems detected in the source;
[`cx.implementation_error(detail, span)`](crate::core::constructs::ParseContext::implementation_error)
builds the abort for extension-contract violations. Both are covered in
[Raising conditions](#raising-conditions).

**Tracebacks.**
[`cx.with_frame(frame, f)`](crate::core::constructs::ParseContext::with_frame)
pushes a live traceback [`Frame`](crate::core::Frame) around a descent;
every condition recorded while the closure runs carries it.

## What a parser returns

`parse` succeeds with `(output, after_effect)`. The optional boxed
[`ParsingStateDelta`](crate::core::ParsingStateDelta) is **exclusively the
construct's after-effect for the caller** — a state change that must outlive
the construct, like `\newcommand` defining a macro for the following
siblings. (It is boxed so that the common `None` case costs one
pointer-sized return slot per nesting level rather than the full delta
struct.) It is *not* for the parser's internal state scoping (that is what
`with_parsing_state` and its siblings are for), and the parser never applies
it itself: deltas are plain values, and the *caller* decides whether and
where they apply — the content loop applies a returned after-effect to its
own live state so it holds for the siblings that follow. Return `None` when
the construct has no after-effect, which is the common case. Writing a
parser is not the only way to produce one: for the plain macro shape the
declarative route is
[`MacroSpec::with_after_effect`](crate::latexlike::MacroSpec::with_after_effect)
— a preset macro whose every invocation leaves a given delta behind, no
custom parser involved.

## The invocation route: a spec takes over its own parsing

When the content loop dispatches a resolved callable, the parser it runs
comes from the spec's factory,
[`CallableSpec::make_invocation_parser`](crate::core::specs::CallableSpec::make_invocation_parser)
— overriding that factory is how *your definition* takes over parsing of
its invocations. The factory receives the resolved
[`Invocation`](crate::core::constructs::Invocation) bundle (invocation
form, name as written, the spec, the trigger token) and returns a fresh
boxed parser for this one invocation; the bundle travels inside the parser
instance.

The contract your parser runs under is documented on
[`StdInvocationParser`](crate::core::constructs::StdInvocationParser) (the
default factory's parser). The essentials:

- **The trigger token is already consumed, whole** — syntactic post-space
  included — by the dispatching arm; your parser starts on whatever follows
  it. A parser that needs the post-space bytes raw (the `\verb` idiom)
  repositions the reader itself
  (`cx.tokens.move_to(token, TokenEdge::End)` — the end of the token
  proper, before its post-space).
- **`cx.state` is the invocation's base state**; the caller has already
  resolved any descent-state policy and scopes the state structurally.
- **Announce consumed material.** A spec that declares no arguments but
  consumes content anyway (a body, raw text) must override
  [`requires_content`](crate::core::specs::CallableSpec::requires_content)
  to `true` — it is the only channel telling the expression-position guard
  that a bare use (`\frac\mymacro 2`) would be malformed.

### The two staging calls

A takeover parser stages its callable node through one of two calls:

- [`cx.stage_invocation(…)`](crate::core::constructs::ParseContext::stage_invocation)
  — the transcription shorthand for **macro-shaped** takeovers: it builds
  the [`CallableData`](crate::core::node::CallableData) by transcribing the
  invocation form, name, and spec from the `Invocation` bundle, mints the
  invocation-syntax payload, computes the span (by default from the trigger
  through the last staged child; pass `end` when the consumed extent
  outruns the last child — rest-of-line and delimiter-terminated shapes),
  and stages. You supply the argument/slot records and the flat child list
  they tile.
- [`cx.stage_node(…)`](crate::core::constructs::ParseContext::stage_node)
  with an explicit `CallableData` — the general form, for **compositions**
  the shorthand deliberately does not cover: a node whose recorded name or
  invocation form differs from the trigger's (an environment node records
  `align`, not `begin`), or an environment-shaped span. Its
  documentation states the region-tiling expectations.

## Argument parsing

Arguments are parsers too — a separate, *stored* contract:
[`ArgumentParser`](crate::core::constructs::ArgumentParser). An argument
spec ([`ArgumentSpec`](crate::core::specs::ArgumentSpec)) is essentially a
parser handle plus an optional name and an optional per-argument state
delta; the shipped implementations (mandatory group, optional group,
literal marker, expression, delimited verbatim, and more) live in
[`core::constructs`](crate::core::constructs), and the latexlike code
factory ([`argument_specs`](crate::latexlike::argument_specs)) resolves the
`"m"`/`"o"`/… shorthands into configured instances of them.

The contract, from
[`parse_argument`](crate::core::constructs::ArgumentParser::parse_argument)'s
documentation: the parser is called with `cx.state` already set to the
argument's own state (the spec's delta stacked on the invocation's base);
it returns `Ok(Some(…))` with a
[`ParsedArgumentNodes`](crate::core::constructs::ParsedArgumentNodes) — the
region's staged nodes in source order, the content designation among them,
and the argument's minted extension value — or `Ok(None)` for an absent
argument, and **absent means nothing was consumed**. An argument parser
owns its region's leading noise (whitespace, comments) and stages it ahead
of the argument's syntax; whether absence is an error is the parser's own
policy, diagnosed at this detection site before reporting absent. A parser
that requires syntax overrides
[`can_match_empty`](crate::core::constructs::ArgumentParser::can_match_empty)
to `false` — that answer feeds the default of the spec-level
`requires_content`.

Takeover parsers that keep declared-argument parsing get it as a building
block:
[`parse_declared_arguments`](crate::core::constructs::parse_declared_arguments)
runs the spec's argument list exactly as the standard parser does —
per-argument states, traceback frames, the collected child list and
argument records — leaving the rest of the invocation (a body, a
terminator) to your code.

## Raising conditions

Problems your parser detects in the *source* go through the recovery entry point:
[`cx.recover(condition, span)`](crate::core::constructs::ParseContext::recover).
Under strict recovery it hands you back an `Err` to propagate; under
tolerant recovery it records the diagnostic and returns `Ok(())` — and then
**your parser performs its local recovery and continues**: recovery happens
where the problem is detected, not in some caller. Document what your
recovery is (stage a fallback node, treat the argument as absent, end the
region early), the way every shipped condition type's page does.

A condition is an ordinary data struct implementing
[`DiagnosticInfo`](crate::error::DiagnosticInfo) — third-party conditions
are structurally identical to the library's own. The
[derive](crate::error::DiagnosticInfo) writes the boilerplate: you declare
the semver-stable identifier and a message format string, and get the trait
implementation, a `Display`, and a constructor (the derive macro's
documentation in `techy_derive` lists exactly what is generated). The
example below defines one.

Contract violations by *extension code* — yours or another's — are not
source conditions:
[`cx.implementation_error(detail, span)`](crate::core::constructs::ParseContext::implementation_error)
builds an [`ImplementationError`](crate::core::constructs::ImplementationError)
abort that ignores the recovery policy. Use it for "this cannot happen
unless a contract was broken" paths — for example, a
[`NodeBuildError`](crate::core::node::NodeBuildError) from a staging call.
The full split is three-way: `ImplementationError` for contract violations
(loud even in tolerant parsing), [`HookFailed`](crate::error::HookFailed)
for operational failures in consumer-supplied hook code (an input/output
failure, a runtime failure behind a language binding — also an abort), and
ordinary domain conditions — through `cx.recover` — for problems diagnosed
in the parsed document.

## A complete takeover parser

Everything above in one compile-checked example: `\until … ;` reads
everything up to the next `;` as raw text — groups, comments, and commands
lose their meaning inside, like `\verb`. The raw-reading state comes from
the shipped recipe
[`verbatim_state_delta`](crate::core::constructs::verbatim_state_delta):
every tokenization feature off, the terminator installed as the one
recognized closing delimiter, so content arrives as plain `Char` tokens and
the terminator as a single `GroupClose` token.

```rust
use std::sync::Arc;

use techy::core::constructs::{
    verbatim_state_delta, ConstructParser, ConstructParserResult, Invocation,
    ParseContext,
};
use techy::core::node::{
    BodySlotExt, BuildId, ChildRegion, ContentNodes, NodeKind, ParsedArguments,
    ParsedSlot, ParsedSlots, SlotRole,
};
use techy::core::specs::{CallableSpec, Package};
use techy::core::{
    GroupRule, Language, ParsingState, ParsingStateDelta, TokenEdge, TokenKind,
};
use techy::error::{DiagnosticInfo, ParseError, Recovery};
use techy::latexlike::{BodyMarker, CallableType, GroupType, Latexlike, LatexlikeDriver};
use techy::serialize::SerializableObject;
use techy::source::SourceSpan;

/// Condition: the terminating `;` never appeared.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[diagnostic(
    id = "mydefs.until.missing-terminator",
    message = "missing the terminating ‘;’ of ‘\\{name}’"
)]
struct MissingTerminator {
    name: String,
}

/// `\until … ;` — the spec: a takeover, declaring no arguments.
#[derive(Debug)]
struct UntilSpec;

// Every callable spec carries the serialization capability; this one does not
// participate, which the empty impl states.
impl SerializableObject<Latexlike> for UntilSpec {}

impl CallableSpec<Latexlike> for UntilSpec {
    // No declared arguments, but material is consumed: say so, for the
    // expression-position guard.
    fn requires_content(&self) -> bool {
        true
    }

    // An infallible factory wraps its parser in `Ok(...)` — the `Err` channel
    // means "the parser could not be built" (never a depth refusal, which is the
    // descent guard's business).
    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, Latexlike>,
    ) -> Result<
        Box<dyn ConstructParser<Latexlike, Output = BuildId> + 'a>,
        ParseError,
    > {
        Ok(Box::new(UntilParser { invocation }))
    }
}

/// The per-invocation temporary: the invocation bundle travels inside it.
struct UntilParser<'a> {
    invocation: Invocation<'a, Latexlike>,
}

impl ConstructParser<Latexlike> for UntilParser<'_> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, Latexlike>,
    ) -> ConstructParserResult<
        Latexlike,
        (BuildId, Option<Box<ParsingStateDelta<Latexlike>>>),
    > {
        // The dispatch loop already consumed the trigger token whole
        // (post-space included): the reader stands on the raw content.
        let start = cx.tokens.position_here();

        // The raw-reading state: features off, `;` the one terminator.
        let terminator = Arc::new(GroupRule {
            group_type: GroupType::Verbatim,
            open: String::new(), // close-expectation carrier only, never an opener
            close: ";".into(),
        });
        let raw = cx.derive_state(&verbatim_state_delta(terminator))?;

        // Read chars until the terminator or end of input.
        let (content_end, end_position) = loop {
            let Some(token) = cx.probe_token(&raw)? else {
                // Tolerated unreadable token: end the region here; the
                // enclosing content loop re-reads and recovers it itself.
                break (cx.tokens.position_here(), cx.tokens.position_here());
            };
            match cx.tokens.token_kind(&token) {
                TokenKind::Char(_) => {
                    cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace)
                }
                TokenKind::GroupClose { .. } => {
                    let content_end =
                        cx.tokens.position_at(&token, TokenEdge::Start);
                    let end_position =
                        cx.tokens.position_at(&token, TokenEdge::EndPastPostSpace);
                    cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                    break (content_end, end_position);
                }
                TokenKind::EndOfStream => {
                    // Detection-site recovery: strict aborts here (`?`);
                    // tolerant records the diagnostic, and our recovery is
                    // to keep the content read so far.
                    cx.recover(
                        MissingTerminator::new(self.invocation.name),
                        cx.tokens.source_span_of(&token),
                    )?;
                    let at = cx.tokens.position_at(&token, TokenEdge::Start);
                    break (at.clone(), at);
                }
                other => {
                    let at = cx.tokens.source_span_of(&token);
                    return Err(cx.implementation_error(
                        format!("unexpected {other} token under the raw state"),
                        at,
                    ))
                }
            }
        };

        // Stage the raw content as one chars node, recorded under the raw
        // state it was read in…
        let mut children = Vec::new();
        // The two positions delimit the content; an incoherent pair would be a
        // bug in this parser, which `source_span_within` reports as one.
        let content: SourceSpan = cx.source_span_within(&start, &content_end)?;
        if !content.is_empty() {
            let id = cx
                .stage_node(
                    NodeKind::chars(content.span()),
                    content.clone(),
                    Arc::clone(&raw),
                    Vec::new(),
                )
                .map_err(|error| cx.implementation_error(error, content))?;
            children.push(id);
        }
        // …recorded as the invocation's one content slot (a callable's
        // children must be tiled by its argument/slot regions), marked as
        // the node's body…
        let count = children.len() as u32;
        let slots = if count == 0 {
            ParsedSlots::empty()
        } else {
            ParsedSlots::new(vec![ParsedSlot::new(
                ChildRegion::new(0..count, ContentNodes::InRegion(0..count)),
                "content",
                SlotRole::Content,
                BodyMarker::make_body(),
            )])
        };
        // …then the callable node through the transcription shorthand, its
        // span extended past the consumed terminator.
        let id = cx.stage_invocation(
            &self.invocation,
            ParsedArguments::empty(),
            slots,
            children,
            Some(&end_position),
        )?;
        Ok((id, None))
    }
}

// Register the spec and parse.
let mut package: Package<Latexlike> = Package::new("mydefs");
package.insert(CallableType::Macro, "until", UntilSpec);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

let result = language.parse(r"a \until {raw} % text; b").unwrap();
assert!(result.diagnostics.is_empty());
let until = result.tree.root().child(1).unwrap();
assert_eq!(until.name(), Some("until"));
assert_eq!(until.span_content(), r"\until {raw} % text;");
// The content was read raw — no group, no comment: one chars node,
// findable both structurally and as the node's body (the marked slot).
assert_eq!(until.child(0).unwrap().chars(), Some("{raw} % text"));
assert_eq!(until.body().unwrap().get(0).unwrap().chars(), Some("{raw} % text"));

// Missing terminator: diagnosed at the detection site, recovered tolerantly.
let result = language.parse(r"\until oops").unwrap();
assert_eq!(result.diagnostics.len(), 1);
let diagnostic = result.diagnostics.iter().next().unwrap();
assert!(diagnostic.data().is::<MissingTerminator>());
assert_eq!(diagnostic.identifier(), MissingTerminator::IDENTIFIER);
```

Worth noticing, tying back to the sections above: the spec is the stored
object (registered once, `Arc`-shared) while `UntilParser` is the
temporary; the trigger was consumed before `parse` ran; the raw state is
derived through the context and passed to `probe_token` explicitly, without
ever swapping `cx.state`; the condition is a custom derive-backed type that
flows through the same diagnostics carriers as the library's own; the staged
chars node records the state it was actually read under; and every location
in the parser is an answer from the reader — the content's two stream
positions turned into a span by `cx.source_span_within`, and the terminator's
position handed to `stage_invocation` so that the node's span reaches past
the `;`.

Read next: back to the [Developer Guide](crate::guide#developer-guide) index —
the other chapters on extending and embedding techy.
