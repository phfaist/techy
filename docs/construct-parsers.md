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
        -> ConstructParserResult<L, (Self::Output, Option<ParsingStateDelta<L>>)>;
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
[`ParseContext`](crate::core::constructs::ParseContext), bundling the five
parse inputs — the token reader, the source, the input parsing state, the
session, and the driver. Its methods are the parser's entire toolkit:

**Token reading.** `cx.tokens` is the
[`TokenReader`](crate::core::TokenReader): peek or consume tokens under an
explicitly passed state, and reposition
([`move_past`](crate::core::TokenReader::move_past),
[`move_to_pos`](crate::core::TokenReader::move_to_pos),
[`pos`](crate::core::TokenReader::pos)). Prefer
[`cx.probe_token(&state)`](crate::core::constructs::ParseContext::probe_token)
over a raw peek: it maps tokenizer errors per the recovery policy (strict:
abort; tolerant: report `None` so you treat the position as unusable, while
the enclosing content loop takes care of diagnosing the token — the probe
protocol is documented on the method).

**Node staging.**
[`cx.stage_node(kind, span, state, children)`](crate::core::constructs::ParseContext::stage_node)
is the single staging entry point — every node is staged through it. It mints the node's language extension
([`Lang::make_node_ext`](crate::core::Lang::make_node_ext)) and stages the
node, returning its [`BuildId`](crate::core::node::BuildId). Children are
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
[`parse_scoped`](crate::core::constructs::ParseContext::parse_scoped) /
[`with_derived_state`](crate::core::constructs::ParseContext::with_derived_state)
are the same primitive in parser-shaped and delta-shaped form.

**Descents.** To parse child content, do not instantiate loop parsers
yourself:
[`cx.parse_nodes(state, stop, child_states)`](crate::core::constructs::ParseContext::parse_nodes)
runs one content-loop descent and
[`cx.parse_group(…)`](crate::core::constructs::ParseContext::parse_group)
one group descent, each obtaining its parser from the driver's factories so
driver overrides apply uniformly. The stop conditions
([`StopSpec`](crate::core::constructs::StopSpec)) and the outcome contract
([`NodesOutcome`](crate::core::constructs::NodesOutcome),
[`StopCause`](crate::core::constructs::StopCause)) are documented with
[`NodesParser`](crate::core::constructs::NodesParser).

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

`parse` succeeds with `(output, after_effect)`. The optional
[`ParsingStateDelta`](crate::core::ParsingStateDelta) is **exclusively the
construct's after-effect for the caller** — a state change that must outlive
the construct, like `\newcommand` defining a macro for the following
siblings. It is *not* for the parser's internal state scoping (that is what
`with_parsing_state` and its siblings are for), and the parser never applies
it itself: deltas are plain values, and the *caller* decides whether and
where they apply — the content loop applies a returned after-effect to its
own live state so it holds for the siblings that follow. Return `None` when
the construct has no after-effect, which is the common case.

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
  (`move_to_pos(token.post_space().start())`).
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
  through the last staged child; pass `end_pos` when the consumed extent
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
use techy::core::{GroupRule, Language, ParsingState, ParsingStateDelta, TokenKind};
use techy::error::{DiagnosticInfo, Recovery};
use techy::latexlike::{BodyMarker, CallableType, GroupType, Latexlike, LatexlikeDriver};
use techy::source::{SourceSpan, Span};

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

impl CallableSpec<Latexlike> for UntilSpec {
    // No declared arguments, but material is consumed: say so, for the
    // expression-position guard.
    fn requires_content(&self) -> bool {
        true
    }

    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, Latexlike>,
    ) -> Box<dyn ConstructParser<Latexlike, Output = BuildId> + 'a>
    where
        's: 'a,
    {
        Box::new(UntilParser { invocation })
    }
}

/// The per-invocation temporary: the invocation bundle travels inside it.
struct UntilParser<'a, 's> {
    invocation: Invocation<'a, 's, Latexlike>,
}

impl ConstructParser<Latexlike> for UntilParser<'_, '_> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, Latexlike>,
    ) -> ConstructParserResult<
        Latexlike,
        (BuildId, Option<ParsingStateDelta<Latexlike>>),
    > {
        // The dispatch loop already consumed the trigger token whole
        // (post-space included): the reader stands on the raw content.
        let start = cx.tokens.pos();

        // The raw-reading state: features off, `;` the one terminator.
        let terminator = Arc::new(GroupRule {
            group_type: GroupType::Verbatim,
            open: String::new(), // close-expectation carrier only, never an opener
            close: ";".into(),
        });
        let raw = cx.derive_state(&verbatim_state_delta(terminator))?;

        // Read chars until the terminator or end of input.
        let (content_end, end_pos) = loop {
            let Some(token) = cx.probe_token(&raw)? else {
                // Tolerated unreadable token: end the region here; the
                // enclosing content loop re-reads and recovers it itself.
                break (cx.tokens.pos(), cx.tokens.pos());
            };
            match &token.kind {
                TokenKind::Char(_) => cx.tokens.move_past(&token, true),
                TokenKind::GroupClose { .. } => {
                    cx.tokens.move_past(&token, true);
                    break (token.span.start(), token.span.end());
                }
                TokenKind::EndOfStream => {
                    // Detection-site recovery: strict aborts here (`?`);
                    // tolerant records the diagnostic, and our recovery is
                    // to keep the content read so far.
                    cx.recover(
                        MissingTerminator::new(self.invocation.name),
                        SourceSpan::new(&cx.source, token.span),
                    )?;
                    break (token.span.start(), token.span.start());
                }
                other => {
                    return Err(cx.implementation_error(
                        format!("unexpected {other} token under the raw state"),
                        token.span,
                    ))
                }
            }
        };

        // Stage the raw content as one chars node, recorded under the raw
        // state it was read in…
        let mut children = Vec::new();
        let content = Span::new(start, content_end);
        if content_end > start {
            let id = cx
                .stage_node(
                    NodeKind::chars(content),
                    SourceSpan::new(&cx.source, content),
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
            Some(end_pos),
        )?;
        Ok((id, None))
    }
}

// Register the spec and parse.
let mut package: Package<Latexlike> = Package::new("mydefs");
package.insert(CallableType::Macro, "until", UntilSpec);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial_with_packages([package]),
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
flows through the same diagnostics carriers as the library's own; and the
staged chars node records the state it was actually read under.

Read next: [Defining a custom language](crate::guide::custom_lang) — the
`Lang` contract that construct parsers, specs, and drivers all plug into.
