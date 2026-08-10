# AI guide: custom languages

Condensed reference: implementing a language, its driver, its token rules,
and its construct parsers. Compressed from
[Defining a custom language](crate::guide::custom_lang),
[The parsing model](crate::guide::parsing_model), and
[Custom construct parsers](crate::guide::construct_parsers) (the full
chapters). Terms: a **language** is an implementation of the
[`Lang`](crate::core::Lang) trait — one compile-time bundle of vocabulary
types, hooks, and defaults; every core type takes the single `L: Lang`
parameter. A **driver** ([`ParseDriver`](crate::core::ParseDriver)
instance) carries all parse-time behavior. A **spec**
([`CallableSpec`](crate::core::specs::CallableSpec)) describes one
callable's arguments and behavior; a **construct parser**
([`ConstructParser`](crate::core::constructs::ConstructParser)) parses one
syntactic construct. The **parsing state**
([`ParsingState`](crate::core::ParsingState)) is an immutable snapshot
(token rules, mode, definitions, language state); change is a
[`ParsingStateDelta`](crate::core::ParsingStateDelta) value applied by
[`derived()`](crate::core::ParsingState::derived).

## The `Lang` contract

A minimal language is a unit struct with the associated types filled in.
**The only required method is
[`make_node_ext`](crate::core::Lang::make_node_ext)** (node exts have no
default value; a no-ext language writes the empty one-liner); every other
method has a working default. Contracts live on
[`Lang`](crate::core::Lang)'s API page — summary:

| Associated type | Role | Trivial default |
|---|---|---|
| [`Features`](crate::core::Lang::Features) | compile-time presence declarations: which parsing features the language has at all ([`LangFeatures`](crate::core::LangFeatures) bundle; absent features get zero-sized storage, writes are compile errors, feature-requiring APIs bound `LangHas*`) | [`AllLangFeatures`](crate::core::AllLangFeatures) |
| [`GroupTypeId`](crate::core::Lang::GroupTypeId) | group *class* vocabulary (content/math/verbatim), closed per language, detached from delimiter spellings — delimiter pairs are runtime data | `u32` |
| [`CallableTypeId`](crate::core::Lang::CallableTypeId) | invocation-*form* vocabulary (macro/environment/specials), closed — new callables register at runtime, new forms never do | `u32` |
| [`ModeId`](crate::core::Lang::ModeId) | the parsing mode a state is in (text/math), first-class state data | `()` |
| [`StateExt`](crate::core::Lang::StateExt) | the language's own slice of the parsing state; plain value type, **no interior mutability** (states freeze at construction) | `()` |
| [`Event`](crate::core::Lang::Event) | semantic transition events on deltas; two classes (see below) | `()` |
| [`SessionExt`](crate::core::Lang::SessionExt) | parse-global mutable extension on the session (history accumulation, caches) | `()` |
| [`SourceOrigin`](crate::core::Lang::SourceOrigin) | origin metadata type of sources | `Option<String>` |
| [`NodeExts`](crate::core::Lang::NodeExts) | per-node / per-argument / per-slot extension type bundle | `()` |
| [`InvocationSyntax`](crate::core::Lang::InvocationSyntax) | recorded trigger-spelling facts per callable invocation — this channel makes recomposition accuracy the language's choice | `()` |
| [`Driver`](crate::core::Lang::Driver) | the language's [`ParseDriver`](crate::core::ParseDriver) type | [`StdParseDriver`](crate::core::StdParseDriver) |

Static hooks on `Lang` (callable outside a driven parse):
[`initial_state_data`](crate::core::Lang::initial_state_data) (the seed;
default: empty rules, nothing recognized),
[`finalize_transition`](crate::core::Lang::finalize_transition),
[`scan_specials`](crate::core::Lang::scan_specials) /
[`specials_trigger_chars`](crate::core::Lang::specials_trigger_chars),
[`make_node_ext`](crate::core::Lang::make_node_ext). Everything that runs
only while a parse is driven lives on the driver.

## Two starting points

**Experiments**: `impl TrivialLang for MyLang {}` — a complete
[`Lang`](crate::core::Lang) with all defaults (no modes, no exts, a driver
that resolves nothing; every feature declared present, seed rules empty). The blanket
implementation makes [`TrivialLang`](crate::core::TrivialLang) and a direct
`Lang` implementation mutually exclusive: the first real vocabulary type or
hook means implementing `Lang` yourself.

```rust
use techy::core::{Language, ParsingState, StdParseDriver, TrivialLang};
use techy::error::Recovery;

#[derive(Debug, Clone, Copy)]
struct MyLang;
impl TrivialLang for MyLang {}

let language: Language<MyLang> = Language::new(
    StdParseDriver::new(Recovery::Strict, ()),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse("hello").unwrap();
assert_eq!(result.tree.root().child(0).unwrap().chars(), Some("hello"));
```

**Anything LaTeX-shaped**: join the `latexlike` family instead of forking
the preset. Requirements: a `Lang` whose vocabulary/payload types implement
the **role traits** —
[`LatexlikeGroupType`](crate::latexlike::LatexlikeGroupType),
[`LatexlikeCallableType`](crate::latexlike::LatexlikeCallableType),
[`LatexlikeMode`](crate::latexlike::LatexlikeMode),
[`LatexlikeEvent`](crate::latexlike::LatexlikeEvent),
[`LatexlikeInvocationSyntax`](crate::latexlike::LatexlikeInvocationSyntax)
(constructors/predicates *on your own types*: which value is the content
group, which count as math, which plays the macro role, …) — plus the
one-line opt-in `impl LatexlikeLang for MyLang {}`
([`LatexlikeLang`](crate::latexlike::LatexlikeLang); its defaulted methods
— math-delimiter table, parse-initialization checks — are overridable per
member). Behavior reuse goes through the preset's **behavior functions**
([`math_group_interior_delta`](crate::latexlike::math_group_interior_delta),
[`exit_math_context_delta`](crate::latexlike::exit_math_context_delta),
[`make_paragraph_break_node`](crate::latexlike::make_paragraph_break_node)):
[`LatexlikeDriver`](crate::latexlike::LatexlikeDriver)'s hook bodies are
one-line delegations to them and contain no behavior these functions do not —
a member wanting preset-behavior-plus-one-custom-hook writes its own
driver composing the same functions (a struct cannot be partially
overridden). **Projection pattern**: the preset's driver and spec types are
generic over the family
([`LatexlikeDriver<LLL>`](crate::latexlike::LatexlikeDriver),
[`MacroSpec<LLL>`](crate::latexlike::MacroSpec),
[`EnvironmentSpec<LLL>`](crate::latexlike::EnvironmentSpec),
[`SpecialsSpec<LLL>`](crate::latexlike::SpecialsSpec)), so a framework
language keeps its own vocabularies, node extensions, and
invocation-syntax payload while reusing them unchanged.

## Token rules and the specials double-hook trap

Tokenization is data: [`TokenRules`](crate::core::TokenRules) (one block
per feature — whitespace set, group delimiter pairs, command escape rules,
comment markers, forbidden characters — each, except forbidden
characters, with an `enabled` flag) is
stored in the parsing state, so all of it can change mid-parse. Three
spellings of "off", documented on the type: **disabled** = flag off,
scoped, data preserved; **empty** = no rules data, nothing recognized;
**absent** = the language has no such feature at all, declared at compile
time via [`Lang::Features`](crate::core::Lang::Features) (see the `Lang`
table). Different tokenization *behavior* (not
just data) = implement the [`TokenReader`](crate::core::TokenReader)
trait. Worked example:
[`default_token_rules`](crate::latexlike::default_token_rules).

**Specials trap (silent)**: specials recognition uses two `Lang` hooks that
must be wired together.
[`scan_specials`](crate::core::Lang::scan_specials) answers "is a trigger
here?" — but it is only consulted when the current character is in
[`specials_trigger_chars`](crate::core::Lang::specials_trigger_chars)'s
returned set (computed per frozen state, the hot-path filter). A first
character missing from that set means the trigger **silently never fires**
— no error, no diagnostic. Second quiet obligation: specials have the
*lowest* recognition precedence (group delimiters, command escapes,
comment starts are tried first), so a trigger overlapping any of those
also silently never fires. The preset wires both hooks to the scope stack,
so registered specials bring their trigger characters automatically.

## Extension types: attaching custom data

A group with custom data is still a group to all generic tooling. The
governing rule (documented on [`NodeExtTypes`](crate::core::NodeExtTypes)):
**population is initialization** — an ext value is minted exactly once, at
creation, by the party with the knowledge; there is no
"default now, populate later" state.

| Attachment | Minted by |
|---|---|
| per node ([`NodeExts`](crate::core::Lang::NodeExts) bundle) | [`make_node_ext`](crate::core::Lang::make_node_ext) — sees the node's parts and the subtree-deep, descent-only staged-children view; deliberately no parent access (staging is bottom-up) |
| per parsed argument | the [`ArgumentParser`](crate::core::constructs::ArgumentParser) that parsed it |
| per parsed slot | the invocation composition minting the slot record (the preset claims this for its body marker behind [`NodeRef::body`](crate::core::node::NodeRef::body)) |
| invocation spelling ([`InvocationSyntax`](crate::core::Lang::InvocationSyntax)) | the invocation parser staging the node; standard sites use the opt-in [`FromInvocation`](crate::core::constructs::FromInvocation) contract |

## State transitions: `finalize_transition`

[`finalize_transition`](crate::core::Lang::finalize_transition) is the
transition customizer: runs exactly once per
[`derived()`](crate::core::ParsingState::derived) call, after the delta's
overrides are applied, before the state freezes. Cross-cutting rules live
here and nowhere else ("in math mode the escape character changes").
Obligations (from the hook's documentation):

- Pure function of `(new, prev, events)` — derivations are deduplicated,
  so it runs once per unique derivation, not once per transition; anything
  history-shaped belongs in
  [`ParseDriver::observe_transition`](crate::core::ParseDriver::observe_transition)
  (fires on every transition).
- Mode changes are interpreted here: the applied override is the signal —
  compare `prev.mode()` with the new mode. No event needed for mode-shaped
  transitions.
- [`Event`](crate::core::Lang::Event)s come in two classes: context-free
  events reach the customizer wherever the delta is applied;
  context-dependent ones (the preset's exit-math restore) are lowered by
  the driver inside a parse and must be **refused loudly** (`Err`) if they
  ever reach the bare customizer.
- Never runs on the seed state — the seed data must already satisfy every
  invariant the customizer maintains
  ([`initial_state_data`](crate::core::Lang::initial_state_data)'s
  coherence contract).

**Replay granularity**: a content run's sibling after-effects are exported
as **one merged delta**
([`NodesOutcome::after_effects`](crate::core::constructs::NodesOutcome) —
later field overrides win; scope operations and context-free events
concatenate in application order). A construct forwarding that merged
record as its own after-effect (the shipped `\input` state persistence,
[`InputMacroSpec`](crate::latexlike::InputMacroSpec)) yields a *single*
derivation: `finalize_transition` sees one transition carrying the merged
delta — intermediate values (a mode entered and left inside the included
file) are invisible; field overrides arrive collapsed. Design customizers
against the merged form.

## The driver

[`Lang::Driver`](crate::core::Lang::Driver) is an instance (so behavior
carries configuration, e.g. the recovery policy). Every
[`ParseDriver`](crate::core::ParseDriver) method has a working default;
the trait page groups the five concerns: recovery policy; parse-time hooks
(command resolution, paragraph-break emission, diagnostic refinement,
transition observation, event lowering); source resolution; the group
descent-delta channel
([`group_interior_delta`](crate::core::ParseDriver::group_interior_delta)
— how a group class changes its interior's state; the preset's math groups
enter math mode this way); and construct provision (the
[`make_nodes_parser`](crate::core::ParseDriver::make_nodes_parser) /
[`make_group_parser`](crate::core::ParseDriver::make_group_parser) /
[`make_invocation_parser`](crate::core::ParseDriver::make_invocation_parser)
factories, routed through by every descent site — one override applies to
the whole parse).

**Command resolution** is the hook a command-bearing language cannot leave
defaulted (the core cannot know which callable type commands resolve
under). [`StdParseDriver`](crate::core::StdParseDriver) makes it a plug-in
strategy ([`CommandResolver`](crate::core::CommandResolver)): `()` resolves
nothing (right for test languages);
[`ScopesCommandResolver`](crate::core::specs::ScopesCommandResolver)
resolves through the state's scope stack under one fixed callable type
(packaged from
[`resolve_command_in_scopes`](crate::core::specs::resolve_command_in_scopes)).
Several command-shaped callable types, or non-scope-stack resolution: write
your own resolver or driver — the documented normal path.

## Construct parsers

The execution model
([The parsing model](crate::guide::parsing_model) is the full map): the
content loop ([`NodesParser`](crate::core::constructs::NodesParser))
dispatches on token kind; a resolved command's parser comes from the
spec's factory
([`CallableSpec::make_invocation_parser`](crate::core::specs::CallableSpec::make_invocation_parser))
— overriding it is the full-takeover route (`\verb`-like). The trait:

```text
trait ConstructParser<L: Lang> {
    type Output;
    fn parse(&mut self, cx: &mut ParseContext<'_, '_, L>)
        -> ConstructParserResult<L, (Self::Output, Option<Box<ParsingStateDelta<L>>>)>;
}
```

Two-tier ownership: construct parsers are **temporaries** (per-use
configuration and working state in fields, dropped after the construct);
stored behavior objects (specs, argument parsers) are `Arc`-shared,
immutable, `Send + Sync`. The returned delta is exclusively the
construct's **after-effect for the caller** (`\newcommand`-style; the
caller applies it — the parser never does); `None` is the common case.
`Err` means **abort** — recovery from source problems happens *before*
returning, at the detection site.

[`ParseContext`](crate::core::constructs::ParseContext) is the parser's
whole toolkit:

| Need | Use |
|---|---|
| read tokens | `cx.tokens` ([`TokenReader`](crate::core::TokenReader)); prefer [`cx.probe_token(&state)`](crate::core::constructs::ParseContext::probe_token) (maps tokenizer errors per recovery policy) |
| stage a node | [`cx.stage_node(kind, span, state, children)`](crate::core::constructs::ParseContext::stage_node) — the single staging entry point; mints the node ext, returns a [`BuildId`](crate::core::node::BuildId); children staged first, bottom-up |
| derive/scope state | [`cx.derive_state(&delta)`](crate::core::constructs::ParseContext::derive_state); [`cx.with_parsing_state`](crate::core::constructs::ParseContext::with_parsing_state) / [`with_derived_state`](crate::core::constructs::ParseContext::with_derived_state) scope with structural restore — state-scoping utilities only, never a route into a sub-parse |
| run a sub-parser (descend) | [`cx.parse_construct(parser, state, frame)`](crate::core::constructs::ParseContext::parse_construct) — the one entry point every `ConstructParser` run MUST go through (`state: None` = the current state, same scoping; optional traceback frame). For child content and groups, the thin wrappers [`cx.parse_nodes(state, stop, child_states)`](crate::core::constructs::ParseContext::parse_nodes) / [`cx.parse_group(…)`](crate::core::constructs::ParseContext::parse_group) add the driver's parser factories — never instantiate loop parsers yourself (driver factories must apply) |
| report a source problem | [`cx.recover(condition, span)`](crate::core::constructs::ParseContext::recover) — strict: hands back `Err` to propagate; tolerant: records the diagnostic, returns `Ok`, then **your parser performs its documented local recovery and continues** |
| report an extension-contract violation | [`cx.implementation_error(detail, span)`](crate::core::constructs::ParseContext::implementation_error) — an abort no recovery policy can swallow |
| traceback frame | [`cx.with_frame(frame, f)`](crate::core::constructs::ParseContext::with_frame) |

**Takeover essentials** (contract on
[`StdInvocationParser`](crate::core::constructs::StdInvocationParser)): the
trigger token is already consumed, post-space included (reposition via
`move_to_pos(token.post_space().start())` for raw `\verb`-style needs);
`cx.state` is the invocation's base state; a spec that declares no
arguments but consumes content must override
[`requires_content`](crate::core::specs::CallableSpec::requires_content)
to `true`. **Two staging calls** for the callable node:
[`cx.stage_invocation(…)`](crate::core::constructs::ParseContext::stage_invocation)
— the transcription shorthand for macro-shaped takeovers (builds
[`CallableData`](crate::core::node::CallableData) from the
[`Invocation`](crate::core::constructs::Invocation) bundle; pass `end_pos`
when the consumed extent outruns the last child) — and
[`cx.stage_node`](crate::core::constructs::ParseContext::stage_node) with
an explicit `CallableData` — the canonical door for compositions (a node
recording `align`, not `begin`). A callable's children must be tiled by
its argument/slot regions (the door documentation states the expectations).
Raw-text reading uses the shipped
[`verbatim_state_delta`](crate::core::constructs::verbatim_state_delta)
recipe. A complete compile-checked takeover parser (`\until … ;`) is in
[Custom construct parsers](crate::guide::construct_parsers#a-complete-takeover-parser).

**Arguments are parsers too** — the stored
[`ArgumentParser`](crate::core::constructs::ArgumentParser) contract: called
with `cx.state` already set to the argument's own state; returns
`Ok(Some(ParsedArgumentNodes))`, or `Ok(None)` for absent — and **absent
means nothing was consumed**. The parser owns its region's leading noise
(whitespace, comments); absence-is-error is the parser's own policy,
diagnosed at this detection site. A parser requiring syntax overrides
[`can_match_empty`](crate::core::constructs::ArgumentParser::can_match_empty)
to `false`. Takeovers that keep declared-argument parsing call
[`parse_declared_arguments`](crate::core::constructs::parse_declared_arguments).

**Conditions**: a condition is a data struct implementing
[`DiagnosticInfo`](crate::error::DiagnosticInfo) (derive available: declare
the semver-stable identifier + a message format string; third-party
conditions flow through the same carriers as the library's own). Document
your recovery, as every shipped condition type's page does. Extension-bug
paths use
[`ImplementationError`](crate::core::constructs::ImplementationError) —
loud even in tolerant parsing.
